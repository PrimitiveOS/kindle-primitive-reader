//! Offline FictionBook 3 package importer. Parts stay inside the source ZIP.
mod body;
mod section_plan;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Seek};

use encoding_rs::Encoding;
use roxmltree::{Document as Xml, Node, ParsingOptions};
use zip::{CompressionMethod, ZipArchive};

use crate::Result;
use crate::epub::{Book, BookCover, CoverKind, check_zip_directory, is_archive_path};
use crate::loose::{Document, read_bounded};

pub(crate) const MAX_INPUT: usize = 16 * 1024 * 1024;
const MAX_ENTRIES: usize = 512;
const MAX_PATH: usize = 512;
const MAX_BODY: usize = 2 * 1024 * 1024;
const MAX_METADATA: usize = 256 * 1024;
const MAX_ASSET: usize = 2 * 1024 * 1024;
const MAX_ASSETS: usize = 8 * 1024 * 1024;
const TYPES_NS: &str = "http://schemas.openxmlformats.org/package/2006/content-types";
const RELS_NS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const CORE_NS: &str = "http://schemas.openxmlformats.org/package/2006/metadata/core-properties";
const DC_NS: &str = "http://purl.org/dc/elements/1.1/";
const FB3_BODY_NS: &str = "http://www.fictionbook.org/FictionBook3/body";
const FB3_DESC_NS: &str = "http://www.fictionbook.org/FictionBook3/description";
const BODY_TYPE: &str = "application/fb3-body+xml";
const DESC_TYPE: &str = "application/fb3-description+xml";
const CORE_TYPE: &str = "application/vnd.openxmlformats-package.core-properties+xml";
const IMAGE_REL: &str = "http://www.fictionbook.org/FictionBook3/relationships/image";
const COVER_REL: &str =
    "http://schemas.openxmlformats.org/package/2006/relationships/metadata/thumbnail";

pub(crate) struct Imported {
    pub(crate) first: Document,
    pub(crate) rest: Vec<(String, String)>,
    pub(crate) revision: u64,
    pub(crate) cover: Option<BookCover>,
    pub(crate) incomplete_note_ids: BTreeSet<String>,
}

pub(crate) fn open(input: impl Read + Seek, len: u64, fallback: &str) -> Result<Imported> {
    let mut zip = Package::open(input, len)?;
    let contents = zip.xml("[content_types].xml", MAX_METADATA)?;
    let contents = Types::parse(parse_xml(&contents)?.root_element())?;
    let body_path = contents
        .role(BODY_TYPE)?
        .ok_or("The FB3 package has no reading body")?;
    let body_xml = zip.xml(body_path, MAX_BODY)?;
    let body_tree = parse_xml(&body_xml)?;
    let root = body_tree.root_element();
    if !root.has_tag_name((FB3_BODY_NS, "fb3-body")) {
        return Err("The FB3 reading body has the wrong root or namespace".into());
    }

    let core = contents
        .role(CORE_TYPE)?
        .and_then(|path| zip.xml(path, MAX_METADATA).ok());
    let core = core.as_deref().and_then(|value| parse_xml(value).ok());
    let core = core
        .as_ref()
        .map(Xml::root_element)
        .filter(|root| root.has_tag_name((CORE_NS, "coreProperties")));
    let description = contents
        .role(DESC_TYPE)?
        .and_then(|path| zip.xml(path, MAX_METADATA).ok());
    let description = description
        .as_deref()
        .and_then(|value| parse_xml(value).ok());
    let description = description
        .as_ref()
        .map(Xml::root_element)
        .filter(|root| root.has_tag_name((FB3_DESC_NS, "fb3-description")));
    let dc = |name| core.and_then(|node| child_text(node, DC_NS, name));
    let desc_title = description.and_then(|root| {
        child(root, FB3_DESC_NS, "title").and_then(|node| child_text(node, FB3_DESC_NS, "main"))
    });
    let title = dc("title").or(desc_title);
    let author = dc("creator").map(str::to_owned);
    let mut metadata = core
        .map(crate::publication_metadata::Metadata::opf)
        .unwrap_or_default();
    metadata.language = description
        .and_then(|root| child_text(root, FB3_DESC_NS, "lang"))
        .or_else(|| dc("language"))
        .map(str::to_owned);

    let cover = zip
        .relationships("_rels/.rels", "", COVER_REL)?
        .into_values()
        .next();
    let (parent, file) = body_path.rsplit_once('/').unwrap_or(("", body_path));
    let rels_path = if parent.is_empty() {
        format!("_rels/{file}.rels")
    } else {
        format!("{parent}/_rels/{file}.rels")
    };
    let images = zip.relationships(&rels_path, body_path, IMAGE_REL)?;
    let mut assets = Assets {
        zip,
        contents,
        paths: BTreeMap::new(),
        data: BTreeMap::new(),
        remaining: MAX_ASSETS,
    };
    let cover_path = cover.as_deref().and_then(|path| assets.load(path));
    let cover = cover_path.as_ref().and_then(|path| {
        let kind = match path.rsplit_once('.')?.1 {
            "gif" => CoverKind::Gif,
            "jpg" => CoverKind::Jpeg,
            "png" => CoverKind::Png,
            "svg" => CoverKind::Svg,
            _ => return None,
        };
        Some(BookCover {
            path: path.clone(),
            kind,
        })
    });
    let output = body::convert(
        root,
        title.unwrap_or_default(),
        metadata.language.as_deref(),
        cover_path.as_deref(),
        &images,
        |path| assets.load(path),
    )?;
    let mut chapters = output.chapters.into_iter();
    let (chapter_path, xhtml) = chapters.next().ok_or("The FB3 has no reading sections")?;
    Ok(Imported {
        first: Document {
            metadata,
            title: title.unwrap_or(fallback).to_owned(),
            author,
            chapter_path,
            xhtml,
            toc: output.toc,
            resources: assets.data,
            fb2_note_sections: Default::default(),
        },
        rest: chapters.collect(),
        revision: crate::revision(&body_xml),
        cover,
        incomplete_note_ids: output.incomplete_note_ids,
    })
}

fn child<'a, 'i>(node: Node<'a, 'i>, namespace: &str, local: &str) -> Option<Node<'a, 'i>> {
    node.children()
        .find(|child| child.has_tag_name((namespace, local)))
}

fn child_text<'a>(node: Node<'a, '_>, namespace: &str, local: &str) -> Option<&'a str> {
    child(node, namespace, local)?
        .text()
        .map(str::trim)
        .filter(|text| !text.is_empty())
}

fn parse_xml(text: &str) -> Result<Xml<'_>> {
    Xml::parse_with_options(
        text,
        ParsingOptions {
            nodes_limit: 80_000,
            ..Default::default()
        },
    )
    .map_err(|error| format!("The FB3 package contains invalid XML: {error}").into())
}

fn package_path(source: &str, target: &str) -> Result<String> {
    if target.is_empty() || target.len() > MAX_PATH || target.contains(['#', '?', '\\']) {
        return Err("An FB3 package part name is invalid".into());
    }
    let path = match target.strip_prefix('/') {
        Some(rooted) => Book::resolve("", rooted),
        None => Book::resolve(source, target),
    }
    .map_err(|_| "An FB3 package part points outside the package or has an invalid name")?;
    Ok(path.to_ascii_lowercase())
}

struct Package<R: Read + Seek> {
    zip: ZipArchive<R>,
    parts: BTreeMap<String, usize>,
}

impl<R: Read + Seek> Package<R> {
    fn open(mut input: R, len: u64) -> Result<Self> {
        if len > MAX_INPUT as u64 {
            return Err("The FB3 document exceeds the 16 MiB package limit".into());
        }
        let count = check_zip_directory(&mut input, len)?;
        if count > MAX_ENTRIES {
            return Err("The FB3 package contains too many parts".into());
        }
        let mut zip = ZipArchive::new(input)?;
        if zip.len() != count {
            return Err("The FB3 package has duplicate or ambiguous ZIP entries".into());
        }
        let mut parts = BTreeMap::new();
        for index in 0..zip.len() {
            let entry = zip.by_index_raw(index)?;
            let name = std::str::from_utf8(entry.name_raw())?.trim_end_matches('/');
            if !is_archive_path(name) || entry.encrypted() || entry.is_symlink() {
                return Err("The FB3 package contains an invalid, linked or encrypted part".into());
            }
            if !entry.is_dir() && parts.insert(package_path("", name)?, index).is_some() {
                return Err("The FB3 package contains duplicate part names".into());
            }
        }
        Ok(Self { zip, parts })
    }

    fn read(&mut self, path: &str, limit: usize) -> Result<Vec<u8>> {
        let index = self
            .parts
            .get(path)
            .ok_or("An FB3 package part is missing")?;
        let entry = self.zip.by_index(*index)?;
        let len = entry.size();
        if len > limit as u64
            || !matches!(
                entry.compression(),
                CompressionMethod::Stored | CompressionMethod::Deflated
            )
        {
            return Err(
                "An FB3 package part exceeds limits or uses unsupported compression".into(),
            );
        }
        let bytes = read_bounded(entry, limit)?;
        if bytes.len() as u64 != len {
            return Err("An FB3 package part is incomplete".into());
        }
        Ok(bytes)
    }

    fn xml(&mut self, path: &str, limit: usize) -> Result<String> {
        let bytes = self.read(path, limit)?;
        if let Some((encoding, offset)) = Encoding::for_bom(&bytes) {
            return encoding
                .decode_without_bom_handling_and_without_replacement(&bytes[offset..])
                .map(std::borrow::Cow::into_owned)
                .ok_or_else(|| "An FB3 part has an invalid Unicode encoding".into());
        }
        Ok(String::from_utf8(bytes)?)
    }

    fn relationships(
        &mut self,
        path: &str,
        source: &str,
        role: &str,
    ) -> Result<BTreeMap<String, String>> {
        let mut result = BTreeMap::new();
        if !self.parts.contains_key(path) {
            return Ok(result);
        }
        let text = self.xml(path, MAX_METADATA)?;
        let tree = parse_xml(&text)?;
        let root = tree.root_element();
        if !root.has_tag_name((RELS_NS, "Relationships")) {
            return Err("An FB3 relationship part has the wrong root or namespace".into());
        }
        for node in root
            .children()
            .filter(|node| node.has_tag_name((RELS_NS, "Relationship")))
        {
            if node.attribute("Type") != Some(role)
                || !matches!(node.attribute("TargetMode"), None | Some("Internal"))
            {
                continue;
            }
            let Some((id, target)) = node.attribute("Id").zip(node.attribute("Target")) else {
                continue;
            };
            if id.is_empty() || id.len() > MAX_PATH || result.len() >= MAX_ENTRIES {
                return Err("The FB3 relationship limit was exceeded".into());
            }
            let target = package_path(source, target)?;
            if result.insert(id.to_owned(), target).is_some() {
                return Err("The FB3 package contains duplicate relationship identifiers".into());
            }
        }
        Ok(result)
    }
}

struct Types {
    overrides: BTreeMap<String, String>,
    defaults: BTreeMap<String, String>,
}

impl Types {
    fn parse(root: Node<'_, '_>) -> Result<Self> {
        if !root.has_tag_name((TYPES_NS, "Types")) {
            return Err("The FB3 content type part has the wrong root or namespace".into());
        }
        let mut result = Self {
            overrides: BTreeMap::new(),
            defaults: BTreeMap::new(),
        };
        for node in root.children().filter(Node::is_element) {
            let Some(kind) = node
                .attribute("ContentType")
                .filter(|value| value.len() <= 128)
            else {
                continue;
            };
            let duplicate = if node.has_tag_name((TYPES_NS, "Override")) {
                let name = node
                    .attribute("PartName")
                    .ok_or("An FB3 type override has no part name")?;
                result
                    .overrides
                    .insert(package_path("", name)?, kind.to_owned())
                    .is_some()
            } else if node.has_tag_name((TYPES_NS, "Default")) {
                let extension = node
                    .attribute("Extension")
                    .filter(|value| value.len() <= 32)
                    .ok_or("An FB3 default type has no valid extension")?;
                result
                    .defaults
                    .insert(extension.to_ascii_lowercase(), kind.to_owned())
                    .is_some()
            } else {
                continue;
            };
            if duplicate || result.overrides.len() + result.defaults.len() > MAX_ENTRIES {
                return Err("The FB3 package contains duplicate or too many content types".into());
            }
        }
        Ok(result)
    }

    fn role(&self, role: &str) -> Result<Option<&str>> {
        let mut parts = self.overrides.iter().filter(|(_, kind)| *kind == role);
        let first = parts.next().map(|(path, _)| path.as_str());
        if parts.next().is_some() {
            return Err("The FB3 package declares more than one part for a document role".into());
        }
        Ok(first)
    }

    fn media(&self, path: &str) -> Option<&str> {
        self.overrides
            .get(path)
            .or_else(|| self.defaults.get(path.rsplit_once('.')?.1))
            .map(String::as_str)
    }
}

struct Assets<R: Read + Seek> {
    zip: Package<R>,
    contents: Types,
    paths: BTreeMap<String, Option<String>>,
    data: BTreeMap<String, Vec<u8>>,
    remaining: usize,
}

impl<R: Read + Seek> Assets<R> {
    fn load(&mut self, path: &str) -> Option<String> {
        if let Some(name) = self.paths.get(path) {
            return name.clone();
        }
        let result = self.load_new(path);
        self.paths.insert(path.to_owned(), result.clone());
        result
    }

    fn load_new(&mut self, path: &str) -> Option<String> {
        if self.data.len() >= 64 || self.remaining == 0 {
            return None;
        }
        let mime = self
            .contents
            .media(path)
            .filter(|mime| mime.starts_with("image/"))?;
        let bytes = self.zip.read(path, self.remaining.min(MAX_ASSET)).ok()?;
        self.remaining -= bytes.len();
        use image::ImageFormat::{Bmp, Gif, Jpeg, Png, WebP};
        let extension = if mime == "image/svg+xml" && bytes.len() <= crate::svg_image::MAX_SVG_BYTES
        {
            "svg"
        } else {
            match image::guess_format(&bytes).ok()? {
                Bmp => "bmp",
                Gif => "gif",
                Jpeg => "jpg",
                Png => "png",
                WebP => "webp",
                _ => return None,
            }
        };
        let name = format!("fb3-images/{}.{}", self.data.len() + 1, extension);
        self.data.insert(name.clone(), bytes);
        Some(name)
    }
}
