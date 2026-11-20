//! A bounded, local subset of the EPUB 2 and 3 package format.
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use roxmltree::{Document, Node, ParsingOptions};
use zip::{CompressionMethod, ZipArchive};

use crate::Result;

const MAX_ARCHIVE: u64 = 128 * 1024 * 1024;
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

pub struct Book {
    pub title: String,
    pub chapters: Vec<Chapter>,
    revision: u64,
    archive: Archive,
}

struct Item {
    path: String,
    media: String,
    navigation: bool,
}

impl Book {
    pub fn open(path: &Path) -> Result<Self> {
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
        let title = child(root, "metadata")?
            .children()
            .find(|node| node.has_tag_name("title"))
            .and_then(|node| node.text())
            .filter(|title| !title.trim().is_empty())
            .unwrap_or("Untitled book")
            .trim()
            .to_owned();
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
        let mut labels = BTreeMap::new();
        if let Some(nav) = manifest.values().find(|item| item.navigation) {
            let xml = xml_entry(&mut archive, &nav.path)?;
            let doc = parse_xml(&xml)?;
            if let Some(toc) = doc.descendants().find(|node| {
                node.has_tag_name("nav")
                    && node.attributes().any(|attr| {
                        attr.name() == "type"
                            && attr
                                .value()
                                .split_ascii_whitespace()
                                .any(|kind| kind == "toc")
                    })
            }) {
                for link in toc.descendants().filter(|node| node.has_tag_name("a")) {
                    add_label(&mut labels, &nav.path, link.attribute("href"), link);
                }
            }
        } else if let Some(ncx) = spine.attribute("toc").and_then(|id| manifest.get(id)) {
            let xml = xml_entry(&mut archive, &ncx.path)?;
            let doc = parse_xml(&xml)?;
            for point in doc
                .descendants()
                .filter(|node| node.has_tag_name("navPoint"))
            {
                let content = point.children().find(|node| node.has_tag_name("content"));
                if let Ok(label) = child(point, "navLabel") {
                    add_label(
                        &mut labels,
                        &ncx.path,
                        content.and_then(|node| node.attribute("src")),
                        label,
                    );
                }
            }
        }
        let mut chapters = Vec::new();
        for reference in spine.children().filter(|node| node.has_tag_name("itemref")) {
            let item = reference
                .attribute("idref")
                .and_then(|id| manifest.get(id))
                .ok_or("The EPUB spine refers to a missing or external item")?;
            if item.media != "application/xhtml+xml" && item.media != "text/html" {
                return Err("The probe supports only XHTML spine documents".into());
            }
            let label = labels
                .get(&item.path)
                .cloned()
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
            chapters,
            revision,
            archive,
        })
    }

    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }

    pub fn chapter(&mut self, index: usize) -> Result<String> {
        let chapter = self
            .chapters
            .get(index)
            .ok_or("That EPUB chapter is unavailable")?;
        xml_entry(&mut self.archive, &chapter.href)
    }

    /// Read a canonical, decoded archive path without extracting it to disk.
    pub fn resource(&mut self, path: &str) -> Result<Vec<u8>> {
        if path
            .split('/')
            .any(|part| part.is_empty() || matches!(part, "." | "..") || !safe_segment(part))
        {
            return Err("The EPUB resource path is invalid".into());
        }
        read_entry(&mut self.archive, path, MAX_RESOURCE)
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
}

fn child<'a, 'input>(node: Node<'a, 'input>, name: &str) -> Result<Node<'a, 'input>> {
    node.children()
        .find(|node| node.has_tag_name(name))
        .ok_or_else(|| format!("The EPUB has no {name} element").into())
}

fn add_label(
    labels: &mut BTreeMap<String, String>,
    base: &str,
    href: Option<&str>,
    node: Node<'_, '_>,
) {
    if let Some(path) = href.and_then(|href| Book::resolve(base, href).ok()) {
        let label = node
            .descendants()
            .filter(Node::is_text)
            .filter_map(|node| node.text())
            .flat_map(str::split_whitespace)
            .collect::<Vec<_>>()
            .join(" ");
        if !label.is_empty() {
            labels.entry(path).or_insert(label);
        }
    }
}

fn xml_entry(archive: &mut Archive, path: &str) -> Result<String> {
    let mut bytes = read_entry(archive, path, MAX_XML)?;
    if bytes.starts_with(b"\xef\xbb\xbf") {
        bytes.drain(..3);
    }
    Ok(String::from_utf8(bytes)?)
}

/// Reject unsupported ZIP64 and implausible directory sizes before ZIP metadata allocation.
fn check_zip_directory(input: &mut (impl Read + Seek), len: u64) -> Result<usize> {
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
