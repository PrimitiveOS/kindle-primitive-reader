//! A bounded, local subset of the EPUB 2 and 3 package format.
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use roxmltree::{Document, Node, ParsingOptions};
use zip::{CompressionMethod, ZipArchive};

use crate::{Result, loose};

pub(crate) const MAX_ARCHIVE: u64 = 128 * 1024 * 1024;
const MAX_ENTRIES: usize = 8192;
const MAX_ZIP_DIRECTORY: u64 = 4 * 1024 * 1024;
const MAX_XML: usize = 512 * 1024;
const MAX_RESOURCE: usize = 8 * 1024 * 1024;

type Archive = ZipArchive<BufReader<File>>;

#[derive(Clone, Debug)]
pub struct Chapter {
    pub href: String,
    pub label: String,
}

/// A local reading destination with a decoded archive path and optional element ID.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Destination {
    pub path: String,
    pub fragment: Option<String>,
}

/// An entry in the publication's table of contents, in depth-first reading order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TocEntry {
    pub label: String,
    pub depth: usize,
    pub destination: Option<Destination>,
}

pub struct Book {
    pub title: String,
    pub author: Option<String>,
    pub chapters: Vec<Chapter>,
    pub toc: Vec<TocEntry>,
    revision: u64,
    fixed_layout: bool,
    content: Content,
}

enum Content {
    Archive(Archive),
    Loose {
        xhtml: String,
        resources: BTreeMap<String, Vec<u8>>,
    },
}

struct Item {
    path: String,
    media: String,
    navigation: bool,
}

impl Book {
    pub fn open(path: &Path) -> Result<Self> {
        if loose::Format::for_path(path).is_some() {
            return Ok(Self::from_loose(loose::Document::open(path)?));
        }
        let file = File::open(path)?;
        let metadata = file.metadata()?;
        if !metadata.is_file() || metadata.len() > MAX_ARCHIVE {
            return Err("The EPUB file exceeds the archive limit or is unavailable".into());
        }
        let mut input = BufReader::new(file);
        let entries = check_zip_directory(&mut input, metadata.len())?;
        let mut archive = ZipArchive::new(input)?;
        if archive.len() != entries {
            return Err("The EPUB archive has ambiguous or duplicate entries".into());
        }
        {
            let marker = archive.by_index(0)?;
            if marker.name() != "mimetype" || marker.compression() != CompressionMethod::Stored {
                return Err("The EPUB archive has no stored mimetype marker".into());
            }
        }
        if read_entry(&mut archive, "mimetype", 20)? != b"application/epub+zip" {
            return Err("The package is not an EPUB book".into());
        }
        let container = xml_entry(&mut archive, "META-INF/container.xml")?;
        let container = parse_xml(&container)?;
        let root = container.root_element();
        if !root.has_tag_name("container") {
            return Err("The EPUB container is invalid".into());
        }
        let package_path = root
            .descendants()
            .find(|node| {
                node.has_tag_name("rootfile")
                    && node.attribute("media-type") == Some("application/oebps-package+xml")
            })
            .and_then(|node| node.attribute("full-path"))
            .ok_or("The EPUB has no package document")?;
        let package_path = Self::resolve("index.xhtml", package_path)?;
        let package = xml_entry(&mut archive, &package_path)?;
        let revision = crate::revision(&package);
        let package = parse_xml(&package)?;
        let root = package.root_element();
        if !root.has_tag_name("package")
            || !root
                .attribute("version")
                .is_some_and(|v| v.starts_with("2.") || v.starts_with("3."))
        {
            return Err("The EPUB package version is unsupported".into());
        }
        let metadata = child(root, "metadata")?;
        let default_layout = package_layout(metadata)?;
        let title = metadata
            .children()
            .find(|node| node.has_tag_name("title"))
            .and_then(|node| node.text())
            .filter(|title| !title.trim().is_empty())
            .unwrap_or("Untitled book")
            .trim()
            .to_owned();
        let authors = metadata
            .children()
            .filter(|node| node.has_tag_name(("http://purl.org/dc/elements/1.1/", "creator")))
            .filter_map(|node| node.text().map(str::trim))
            .filter(|name| !name.is_empty())
            .collect::<Vec<_>>();
        let author = (!authors.is_empty()).then(|| authors.join(", "));
        let mut manifest = BTreeMap::new();
        for node in child(root, "manifest")?
            .children()
            .filter(|node| node.has_tag_name("item"))
        {
            let id = node
                .attribute("id")
                .ok_or("An EPUB manifest item has no ID")?;
            let href = node
                .attribute("href")
                .ok_or("An EPUB manifest item has no path")?;
            let Ok(path) = Self::resolve(&package_path, href) else {
                continue;
            };
            let item = Item {
                path,
                media: node.attribute("media-type").unwrap_or_default().to_owned(),
                navigation: node
                    .attribute("properties")
                    .unwrap_or_default()
                    .split_ascii_whitespace()
                    .any(|p| p == "nav"),
            };
            if manifest.insert(id, item).is_some() {
                return Err("The EPUB manifest contains duplicate IDs".into());
            }
        }
        let spine = child(root, "spine")?;
        let toc = if let Some(nav) = manifest.values().find(|item| item.navigation) {
            let xml = xml_entry(&mut archive, &nav.path)?;
            let xml = loose::without_plain_html_doctype(&xml);
            let doc = parse_xml(&xml)?;
            xhtml_toc(&doc, &nav.path)
        } else if let Some(ncx) = spine.attribute("toc").and_then(|id| manifest.get(id)) {
            let xml = xml_entry(&mut archive, &ncx.path)?;
            let doc = parse_xml(&xml)?;
            ncx_toc(&doc, &ncx.path)
        } else {
            Vec::new()
        };
        let mut labels = BTreeMap::new();
        for entry in &toc {
            if let Some(destination) = &entry.destination {
                labels
                    .entry(destination.path.as_str())
                    .or_insert(entry.label.as_str());
            }
        }
        let mut chapters = Vec::new();
        let mut fixed_layout = None;
        for reference in spine.children().filter(|node| node.has_tag_name("itemref")) {
            let item = reference
                .attribute("idref")
                .and_then(|id| manifest.get(id))
                .ok_or("The EPUB spine refers to a missing or external item")?;
            let fixed = spine_layout(reference, default_layout)?;
            if fixed_layout
                .replace(fixed)
                .is_some_and(|previous| previous != fixed)
            {
                return Err(
                    "Reader cannot open an EPUB that mixes fixed-layout and reflowable spine items"
                        .into(),
                );
            }
            if fixed && item.media != "application/xhtml+xml" {
                return Err("Reader supports fixed-layout EPUB pages stored as XHTML".into());
            }
            if item.media != "application/xhtml+xml" && item.media != "text/html" {
                return Err("The probe supports only XHTML spine documents".into());
            }
            let label = labels
                .get(item.path.as_str())
                .map(|label| (*label).to_owned())
                .unwrap_or_else(|| format!("Chapter {}", chapters.len() + 1));
            chapters.push(Chapter {
                href: item.path.clone(),
                label,
            });
        }
        if chapters.is_empty() {
            return Err("The EPUB contains no reading chapters".into());
        }
        Ok(Self {
            title,
            author,
            chapters,
            toc,
            revision,
            fixed_layout: fixed_layout.unwrap_or(false),
            content: Content::Archive(archive),
        })
    }

    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }

    #[cfg(feature = "kindle")]
    pub(crate) fn from_text(title: String, text: &str) -> Result<Self> {
        Ok(Self::from_loose(loose::Document {
            title,
            author: None,
            xhtml: loose::text_as_xhtml(text)?,
            toc: Vec::new(),
            resources: BTreeMap::new(),
        }))
    }

    fn from_loose(document: loose::Document) -> Self {
        Self {
            revision: crate::revision(&document.xhtml),
            fixed_layout: false,
            chapters: vec![Chapter {
                href: loose::CHAPTER_PATH.to_owned(),
                label: document.title.clone(),
            }],
            title: document.title,
            author: document.author,
            toc: document.toc,
            content: Content::Loose {
                xhtml: document.xhtml,
                resources: document.resources,
            },
        }
    }

    pub fn is_fixed_layout(&self) -> bool {
        self.fixed_layout
    }

    /// Return the publication's ordered reading contents.
    pub fn toc(&self) -> &[TocEntry] {
        &self.toc
    }

    pub fn chapter(&mut self, index: usize) -> Result<String> {
        let chapter = self
            .chapters
            .get(index)
            .ok_or("That EPUB chapter is unavailable")?;
        match &mut self.content {
            Content::Archive(archive) => xml_entry(archive, &chapter.href),
            Content::Loose { xhtml, .. } => Ok(xhtml.clone()),
        }
    }

    /// Look up a canonical archive path in the reading spine.
    pub fn chapter_index(&self, path: &str) -> Option<usize> {
        self.chapters
            .iter()
            .position(|chapter| chapter.href == path)
    }

    /// Read a canonical, decoded archive path without extracting it to disk.
    pub fn resource(&mut self, path: &str) -> Result<Vec<u8>> {
        if !is_archive_path(path) {
            return Err("The EPUB resource path is invalid".into());
        }
        match &mut self.content {
            Content::Archive(archive) => read_entry(archive, path, MAX_RESOURCE),
            Content::Loose { resources, .. } => resources
                .get(path)
                .cloned()
                .ok_or_else(|| "The standalone document has no such local resource".into()),
        }
    }

    /// Resolve a URI reference against a canonical archive path.
    pub fn resolve(base: &str, relative: &str) -> Result<String> {
        let raw = relative.split(['#', '?']).next().unwrap_or_default();
        if raw.is_empty() {
            return if base.is_empty() {
                Err("An EPUB resource path is empty".into())
            } else {
                Ok(base.to_owned())
            };
        }
        if raw.starts_with('/') || raw.contains(':') {
            return Err("The EPUB resource is outside the book".into());
        }
        let mut components = base.rsplit_once('/').map_or_else(Vec::new, |(parent, _)| {
            parent.split('/').map(str::to_owned).collect()
        });
        for part in raw.split('/') {
            let part = decode_segment(part)?;
            match part.as_str() {
                "." => (),
                ".." => {
                    components
                        .pop()
                        .ok_or("The EPUB resource escapes the book")?;
                }
                path if !path.is_empty() && safe_segment(path) => components.push(part),
                _ => return Err("The EPUB resource path is invalid".into()),
            }
        }
        if components.is_empty() {
            return Err("The EPUB resource path is empty".into());
        }
        Ok(components.join("/"))
    }

    /// Resolve an in-book link against a canonical archive path and decode its fragment once.
    pub fn resolve_link(base: &str, href: &str) -> Result<Destination> {
        if !is_archive_path(base) {
            return Err("The EPUB link source path is invalid".into());
        }
        let (reference, fragment) = href.split_once('#').unwrap_or((href, ""));
        let path = Self::resolve(base, reference)?;
        let fragment = decode_segment(fragment)?;
        if fragment.chars().any(char::is_control) {
            return Err("The EPUB link fragment is invalid".into());
        }
        Ok(Destination {
            path,
            fragment: (!fragment.is_empty()).then_some(fragment),
        })
    }
}

fn child<'a, 'input>(node: Node<'a, 'input>, name: &str) -> Result<Node<'a, 'input>> {
    node.children()
        .find(|node| node.has_tag_name(name))
        .ok_or_else(|| format!("The EPUB has no {name} element").into())
}

// EPUB 3.3 §8.2.2.1: https://www.w3.org/TR/epub-33/#layout
fn package_layout(metadata: Node<'_, '_>) -> Result<bool> {
    let mut declarations = metadata.children().filter(|node| {
        node.has_tag_name("meta") && node.attribute("property") == Some("rendition:layout")
    });
    let Some(declaration) = declarations.next() else {
        return Ok(false);
    };
    if declarations.next().is_some() || declaration.has_attribute("refines") {
        return Err("The EPUB has conflicting rendition:layout declarations".into());
    }
    match declaration.text().map(str::trim) {
        Some("pre-paginated") => Ok(true),
        Some("reflowable") => Ok(false),
        _ => Err("The EPUB rendition:layout value is unsupported".into()),
    }
}

fn spine_layout(reference: Node<'_, '_>, default: bool) -> Result<bool> {
    let mut layout = None;
    for property in reference
        .attribute("properties")
        .unwrap_or_default()
        .split_ascii_whitespace()
    {
        let fixed = match property {
            "rendition:layout-pre-paginated" => true,
            "rendition:layout-reflowable" => false,
            _ => continue,
        };
        if layout.replace(fixed).is_some() {
            return Err("The EPUB spine item has more than one layout override".into());
        }
    }
    Ok(layout.unwrap_or(default))
}

// EPUB 3.3 §7.3–7.4: https://www.w3.org/TR/epub-33/#sec-nav-def-model
fn xhtml_toc(doc: &Document<'_>, base: &str) -> Vec<TocEntry> {
    let Some(toc) = doc.descendants().find(|node| {
        node.has_tag_name("nav")
            && node
                .attribute(("http://www.idpf.org/2007/ops", "type"))
                .or_else(|| node.attribute("type"))
                .is_some_and(|kinds| kinds.split_ascii_whitespace().any(|kind| kind == "toc"))
    }) else {
        return Vec::new();
    };
    let mut entries = Vec::new();
    let mut pending = Vec::new();
    push_xhtml_items(&mut pending, toc, 0);
    while let Some((item, depth)) = pending.pop() {
        if let Some(label) = item
            .children()
            .find(|node| node.has_tag_name("a") || node.has_tag_name("span"))
        {
            let href = label
                .has_tag_name("a")
                .then(|| label.attribute("href"))
                .flatten();
            entries.extend(toc_entry(base, href, label, depth));
        }
        push_xhtml_items(&mut pending, item, depth + 1);
    }
    entries
}

fn push_xhtml_items<'a, 'input>(
    pending: &mut Vec<(Node<'a, 'input>, usize)>,
    parent: Node<'a, 'input>,
    depth: usize,
) {
    for list in parent
        .children()
        .rev()
        .filter(|node| node.has_tag_name("ol"))
    {
        pending.extend(
            list.children()
                .rev()
                .filter(|node| node.has_tag_name("li"))
                .map(|node| (node, depth)),
        );
    }
}

// OPF 2.0.1 §2.4.1: https://idpf.org/epub/20/spec/OPF_2.0.1_draft.htm#Section2.4.1
fn ncx_toc(doc: &Document<'_>, base: &str) -> Vec<TocEntry> {
    let Ok(map) = child(doc.root_element(), "navMap") else {
        return Vec::new();
    };
    let mut entries = Vec::new();
    let mut pending: Vec<_> = map.children().rev().map(|node| (node, 0)).collect();
    while let Some((point, depth)) = pending.pop() {
        if !point.has_tag_name("navPoint") {
            continue;
        }
        if let Ok(label) = child(point, "navLabel").and_then(|node| child(node, "text")) {
            let href = child(point, "content")
                .ok()
                .and_then(|node| node.attribute("src"));
            entries.extend(toc_entry(base, href, label, depth));
        }
        pending.extend(point.children().rev().map(|node| (node, depth + 1)));
    }
    entries
}

fn toc_entry(base: &str, href: Option<&str>, node: Node<'_, '_>, depth: usize) -> Option<TocEntry> {
    let text: String = node
        .descendants()
        .filter(Node::is_text)
        .filter_map(|node| node.text())
        .collect();
    let text = if text.trim().is_empty() {
        node.attribute("title").unwrap_or_default()
    } else {
        &text
    };
    let label = text.split_whitespace().collect::<Vec<_>>().join(" ");
    (!label.is_empty()).then(|| TocEntry {
        label,
        depth,
        destination: href.and_then(|href| Book::resolve_link(base, href).ok()),
    })
}

fn xml_entry(archive: &mut Archive, path: &str) -> Result<String> {
    let mut bytes = read_entry(archive, path, MAX_XML)?;
    if bytes.starts_with(b"\xef\xbb\xbf") {
        bytes.drain(..3);
    }
    Ok(String::from_utf8(bytes)?)
}

/// Reject unsupported ZIP64 and implausible directory sizes before ZIP metadata allocation.
pub(crate) fn check_zip_directory(input: &mut (impl Read + Seek), len: u64) -> Result<usize> {
    const END_SIZE: usize = 22;
    let tail_start = len.saturating_sub((END_SIZE + usize::from(u16::MAX)) as u64);
    let mut tail = vec![0; (len - tail_start) as usize];
    input.seek(SeekFrom::Start(tail_start))?;
    input.read_exact(&mut tail)?;
    let end = tail
        .windows(END_SIZE)
        .rposition(|window| window.starts_with(b"PK\x05\x06"))
        .ok_or("The EPUB archive has no ZIP directory")?;
    let record = &tail[end..end + END_SIZE];
    let u16_field = |index| u16::from_le_bytes([record[index], record[index + 1]]);
    let u32_field = |index| u32::from_le_bytes(record[index..index + 4].try_into().unwrap());
    let entries = usize::from(u16_field(10));
    let size = u64::from(u32_field(12));
    let offset = u64::from(u32_field(16));
    let comment_size = usize::from(u16_field(20));
    let zip64 = end
        .checked_sub(20)
        .is_some_and(|offset| tail[offset..].starts_with(b"PK\x06\x07"));
    if end + END_SIZE + comment_size != tail.len()
        || zip64
        || u16_field(4) != 0
        || u16_field(6) != 0
        || entries != usize::from(u16_field(8))
        || !(1..=MAX_ENTRIES).contains(&entries)
        || size < (entries * 46) as u64
        || size > MAX_ZIP_DIRECTORY
        || offset + size != tail_start + end as u64
    {
        return Err("The EPUB ZIP directory is invalid or exceeds the probe limits".into());
    }
    input.rewind()?;
    Ok(entries)
}

fn read_entry(archive: &mut Archive, path: &str, max_bytes: usize) -> Result<Vec<u8>> {
    let entry = archive.by_name(path)?;
    if entry.is_dir() || entry.size() > max_bytes as u64 {
        return Err(format!("The EPUB resource exceeds its size limit: {path}").into());
    }
    let mut bytes = Vec::with_capacity(entry.size() as usize);
    entry.take(max_bytes as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > max_bytes {
        return Err(format!("The EPUB resource exceeds its size limit: {path}").into());
    }
    Ok(bytes)
}

fn parse_xml(xml: &str) -> Result<Document<'_>> {
    Ok(Document::parse_with_options(
        xml,
        ParsingOptions {
            nodes_limit: 80_000,
            ..ParsingOptions::default()
        },
    )?)
}

fn safe_segment(segment: &str) -> bool {
    !segment
        .chars()
        .any(|c| c.is_control() || matches!(c, '/' | '\\' | ':'))
}

fn is_archive_path(path: &str) -> bool {
    path.split('/')
        .all(|part| !matches!(part, "" | "." | "..") && safe_segment(part))
}

fn decode_segment(segment: &str) -> Result<String> {
    let mut bytes = Vec::with_capacity(segment.len());
    let mut input = segment.bytes();
    while let Some(byte) = input.next() {
        let value = if byte == b'%' {
            let hex = |byte: u8| char::from(byte).to_digit(16).map(|value| value as u8);
            let high = input
                .next()
                .and_then(hex)
                .ok_or("The EPUB URI contains an invalid escape")?;
            let low = input
                .next()
                .and_then(hex)
                .ok_or("The EPUB URI contains an invalid escape")?;
            high * 16 + low
        } else {
            byte
        };
        bytes.push(value);
    }
    Ok(String::from_utf8(bytes)?)
}
