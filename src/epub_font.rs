//! EPUB publisher-font obfuscation for manifest fonts explicitly listed in
//! the container's IDPF or Adobe encryption metadata.
use std::collections::{BTreeMap, BTreeSet};

use roxmltree::{Document, Node, ParsingOptions};

use crate::epub::Book;
use crate::epub_members::Members;

const CONTAINER: &str = "urn:oasis:names:tc:opendocument:xmlns:container";
const XMLENC: &str = "http://www.w3.org/2001/04/xmlenc#";
const DC: &str = "http://purl.org/dc/elements/1.1/";
const IDPF: &str = "http://www.idpf.org/2008/embedding";
const ADOBE: &str = "http://ns.adobe.com/pdf/enc#RC";
const MAX_METADATA: usize = 96 * 1024;
const MAX_FACES: usize = 128;

#[derive(Clone, Copy)]
enum Method {
    Idpf,
    Adobe,
}

#[derive(Clone, Copy)]
enum FontKey {
    Idpf([u8; 20]),
    Adobe([u8; 16]),
}

pub(crate) struct EmbeddedFonts {
    paths: BTreeMap<String, FontKey>,
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
            let method = if reference.children().any(|n| n.is_element()) {
                None
            } else {
                match method {
                    Some(IDPF) => Some(Method::Idpf),
                    Some(ADOBE) => Some(Method::Adobe),
                    _ => None,
                }
            };
            methods
                .entry(path)
                .and_modify(|existing| *existing = None)
                .or_insert(method);
        }
        let metadata = package.children().find(|n| n.has_tag_name("metadata"))?;
        let idpf = idpf_key(package, metadata).map(FontKey::Idpf);
        // CREngine tries identifiers in package order for Adobe fonts. Require
        // a canonical UUID so unrelated metadata cannot silently supply a key.
        let adobe = metadata
            .children()
            .filter(|n| {
                n.has_tag_name((DC, "identifier")) && !n.children().any(|child| child.is_element())
            })
            .filter_map(|n| n.text())
            .find_map(adobe_key)
            .map(FontKey::Adobe);
        let paths = methods
            .into_iter()
            .filter_map(|(path, method)| {
                let key = match method? {
                    Method::Idpf => idpf?,
                    Method::Adobe => adobe?,
                };
                Some((path, key))
            })
            .collect::<BTreeMap<_, _>>();
        (!paths.is_empty()).then_some(Self { paths })
    }

    pub(crate) fn apply(&self, path: &str, bytes: &mut [u8]) {
        match self.paths.get(path) {
            Some(FontKey::Idpf(key)) => xor_prefix(bytes, key, 1040),
            Some(FontKey::Adobe(key)) => xor_prefix(bytes, key, 1024),
            None => {}
        }
    }
}

fn xor_prefix(bytes: &mut [u8], key: &[u8], limit: usize) {
    for (i, byte) in bytes.iter_mut().take(limit).enumerate() {
        *byte ^= key[i % key.len()];
    }
}

fn idpf_key(package: Node<'_, '_>, metadata: Node<'_, '_>) -> Option<[u8; 20]> {
    let id = package.attribute("unique-identifier")?;
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
    digest.as_ref().try_into().ok()
}

fn adobe_key(value: &str) -> Option<[u8; 16]> {
    let text = value.trim();
    let text = text.strip_prefix("urn:uuid:").unwrap_or(text);
    let bytes = text.as_bytes();
    if bytes.len() != 36 || [8, 13, 18, 23].iter().any(|&i| bytes[i] != b'-') {
        return None;
    }
    let mut digits = [0u8; 32];
    let mut count = 0;
    for (i, &byte) in bytes.iter().enumerate() {
        if matches!(i, 8 | 13 | 18 | 23) {
            continue;
        }
        digits[count] = (byte as char).to_digit(16)? as u8;
        count += 1;
    }
    let mut key = [0; 16];
    for (i, byte) in key.iter_mut().enumerate() {
        *byte = digits[2 * i] * 16 + digits[2 * i + 1];
    }
    Some(key)
}

fn unique_child<'a, 'input>(parent: Node<'a, 'input>, tag: &str) -> Option<Node<'a, 'input>> {
    let mut nodes = parent.children().filter(|n| n.has_tag_name((XMLENC, tag)));
    let child = nodes.next()?;
    nodes.next().is_none().then_some(child)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adobe_uuid_key_and_prefix_are_exact_and_bounded() {
        let key = adobe_key(" urn:uuid:00112233-4455-6677-8899-AABBCCDDEEFF \n").unwrap();
        assert_eq!(
            key,
            [
                0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd,
                0xee, 0xff
            ]
        );
        for invalid in [
            "",
            "12345678",
            "00112233445566778899aabbccddeeff",
            "urn:uuid:00112233-4455-6677-8899-aabbccddeefg",
            "00112233-4455-6677-8899-aabbccddeeff-extra",
        ] {
            assert!(adobe_key(invalid).is_none(), "{invalid}");
        }
        let mut paths = BTreeMap::new();
        paths.insert("font.ttf".to_owned(), FontKey::Adobe(key));
        let fonts = EmbeddedFonts { paths };
        let mut data = vec![0u8; 1100];
        fonts.apply("other.ttf", &mut data);
        assert!(data.iter().all(|byte| *byte == 0));
        fonts.apply("font.ttf", &mut data);
        for index in 0..1024 {
            assert_eq!(data[index], key[index % 16]);
        }
        assert!(data[1024..].iter().all(|byte| *byte == 0));
        fonts.apply("font.ttf", &mut data);
        assert!(data.iter().all(|byte| *byte == 0));
    }
}
