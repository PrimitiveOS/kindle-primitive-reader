//! W3C EPUB 3.3 OCF font obfuscation. This standard font transformation
//! applies only to manifest font members explicitly listed with the IDPF URI.
use std::collections::{BTreeMap, BTreeSet};

use roxmltree::{Document, Node, ParsingOptions};

use crate::epub::Book;
use crate::epub_members::Members;

const CONTAINER: &str = "urn:oasis:names:tc:opendocument:xmlns:container";
const XMLENC: &str = "http://www.w3.org/2001/04/xmlenc#";
const DC: &str = "http://purl.org/dc/elements/1.1/";
const IDPF: &str = "http://www.idpf.org/2008/embedding";
const MAX_METADATA: usize = 96 * 1024;
const MAX_FACES: usize = 128;

pub(crate) struct EmbeddedFonts {
    key: [u8; 20],
    paths: BTreeSet<String>,
}

impl EmbeddedFonts {
    pub(crate) fn read<'a>(
        archive: &mut Members,
        package: Node<'_, '_>,
        fonts: impl Iterator<Item = (&'a str, &'a str)>,
    ) -> Option<Self> {
        let fonts = fonts
            .filter_map(|(path, media)| match media.to_ascii_lowercase().as_str() {
                "font/ttf"
                | "font/otf"
                | "font/sfnt"
                | "application/font-sfnt"
                | "application/vnd.ms-opentype"
                | "application/x-font-ttf"
                | "application/x-font-opentype"
                | "application/x-font-truetype" => Some(path),
                _ => None,
            })
            .take(MAX_FACES + 1)
            .collect::<BTreeSet<_>>();
        if fonts.is_empty() || fonts.len() > MAX_FACES {
            return None;
        }
        let bytes = archive.read("META-INF/encryption.xml", MAX_METADATA).ok()?;
        let text = std::str::from_utf8(&bytes)
            .ok()?
            .trim_start_matches('\u{feff}');
        let map = Document::parse_with_options(
            text,
            ParsingOptions {
                nodes_limit: 2048,
                ..Default::default()
            },
        )
        .ok()?;
        let root = map.root_element();
        if !root.has_tag_name((CONTAINER, "encryption")) {
            return None;
        }
        // Ambiguous or multiply transformed entries are ineligible for this simple method.
        let mut methods = BTreeMap::new();
        for item in root
            .children()
            .filter(|n| n.has_tag_name((XMLENC, "EncryptedData")))
        {
            let method =
                unique_child(item, "EncryptionMethod").and_then(|n| n.attribute("Algorithm"));
            let Some(reference) = unique_child(item, "CipherData")
                .and_then(|data| unique_child(data, "CipherReference"))
            else {
                continue;
            };
            let Some(uri) = reference.attribute("URI") else {
                continue;
            };
            if uri.contains(['?', '#']) {
                continue;
            }
            let Ok(path) = Book::resolve("container.xml", uri) else {
                continue;
            };
            if !fonts.contains(path.as_str()) {
                continue;
            }
            let only_method = method == Some(IDPF) && !reference.children().any(|n| n.is_element());
            methods
                .entry(path)
                .and_modify(|valid| *valid = false)
                .or_insert(only_method);
        }
        let paths = methods
            .into_iter()
            .filter_map(|(p, valid)| valid.then_some(p))
            .collect::<BTreeSet<_>>();
        if paths.is_empty() {
            return None;
        }
        let id = package.attribute("unique-identifier")?;
        let metadata = package.children().find(|n| n.has_tag_name("metadata"))?;
        let mut identifiers = metadata
            .children()
            .filter(|n| n.has_tag_name((DC, "identifier")) && n.attribute("id") == Some(id));
        let identifier = identifiers.next()?;
        if identifiers.next().is_some() || identifier.children().any(|n| n.is_element()) {
            return None;
        }
        let normalized = identifier
            .children()
            .filter(Node::is_text)
            .filter_map(|n| n.text())
            .flat_map(str::bytes)
            .filter(|b| !matches!(b, b' ' | b'\t' | b'\n' | b'\r'))
            .take(4097)
            .collect::<Vec<_>>();
        if normalized.is_empty() || normalized.len() > 4096 {
            return None;
        }
        let digest = ring::digest::digest(&ring::digest::SHA1_FOR_LEGACY_USE_ONLY, &normalized);
        let key = digest.as_ref().try_into().ok()?;
        Some(Self { key, paths })
    }
    pub(crate) fn apply(&self, path: &str, bytes: &mut [u8]) {
        if self.paths.contains(path) {
            for (i, v) in bytes.iter_mut().take(1040).enumerate() {
                *v ^= self.key[i % self.key.len()];
            }
        }
    }
}

fn unique_child<'a, 'input>(parent: Node<'a, 'input>, tag: &str) -> Option<Node<'a, 'input>> {
    let mut nodes = parent.children().filter(|n| n.has_tag_name((XMLENC, tag)));
    let child = nodes.next()?;
    nodes.next().is_none().then_some(child)
}
