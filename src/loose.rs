//! Bounded imports of standalone reading documents.
//!
//! The FictionBook 2 subset covers title and author metadata, headed and unheaded
//! sections, textual paragraphs and emphasis, and note and return links. The
//! package tests exercise the normal file entry point. Unit tests exercise source
//! anchor resolution, reading order, TOC IDs, and rejection of malformed inputs.
//! Plain, ZIP, and gzip FB2 entries share the same reading adapter. Embedded
//! PNG and JPEG assets are confined to each publication.
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{BufReader, Read};
use std::path::Path;

use base64::{Engine as _, engine::general_purpose::STANDARD_PAD_INDIFFERENT};
use encoding_rs::{Encoding, UTF_8, WINDOWS_1252};
use flate2::read::MultiGzDecoder;
use image::ImageFormat;
use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag};
use roxmltree::{Document as Xml, Node, ParsingOptions};
use zip::{CompressionMethod, ZipArchive};

use crate::Result;
use crate::epub::{
    Book, BookCover, CoverKind, Destination, TocEntry, check_zip_directory, is_archive_path,
};

pub(crate) const CHAPTER_PATH: &str = "document.xhtml";
pub(crate) const MAX_INPUT: usize = 512 * 1024;
pub(crate) const MAX_FB2_INPUT: usize = 8 * 1024 * 1024;
pub(crate) const MAX_HTMLZ_INPUT: usize = 16 * 1024 * 1024;
const MAX_OUTPUT: usize = 1024 * 1024;
const MAX_HTMLZ_ENTRIES: usize = 256;
const MAX_HTMLZ_PATH: usize = 512;
const MAX_HTMLZ_RESOURCES: usize = 8 * 1024 * 1024;
const MAX_HTMLZ_IMAGE: usize = 2 * 1024 * 1024;
const MAX_HTMLZ_CSS: usize = 256 * 1024;
const MAX_HTMLZ_METADATA: usize = 64 * 1024;
const MAX_HTMLZ_HEADINGS: usize = 512;
const MAX_FB2_ZIP_ENTRIES: usize = 64;
const MAX_FB2_IMAGES: usize = 32;
const MAX_FB2_IMAGE: usize = 1024 * 1024;
const MAX_FB2_IMAGE_BYTES: usize = 4 * 1024 * 1024;
const MAX_FB2_IMAGE_PIXELS: usize = 8 * 1024 * 1024;
const FB2_NS: &str = "http://www.gribuser.ru/xml/fictionbook/2.0";
const XLINK_NS: &str = "http://www.w3.org/1999/xlink";
const MAX_FB2_DEPTH: usize = 48;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Format {
    Text,
    Html,
    Htmlz,
    Docx,
    Docm,
    Odt,
    Chm,
    Markdown,
    Mobi,
    Rtf,
    Tcr,
    Fb2,
    Fb3,
}

impl Format {
    pub(crate) fn for_path(path: &Path) -> Option<Self> {
        let extension = path.extension()?.to_str()?.to_ascii_lowercase();
        match extension.as_str() {
            "txt" | "log" => Some(Self::Text),
            "htm" | "html" | "xht" | "xhtml" | "xml" => Some(Self::Html),
            "htmlz" => Some(Self::Htmlz),
            "docx" => Some(Self::Docx),
            "docm" => Some(Self::Docm),
            "odt" => Some(Self::Odt),
            "chm" => Some(Self::Chm),
            "md" | "markdown" => Some(Self::Markdown),
            "mobi" | "prc" | "azw" | "azw3" | "pdb" => Some(Self::Mobi),
            "rtf" => Some(Self::Rtf),
            "tcr" => Some(Self::Tcr),
            "fb2" | "fbz" => Some(Self::Fb2),
            "fb3" => Some(Self::Fb3),
            "gz" | "zip"
                if path
                    .file_stem()
                    .is_some_and(|name| has_extension(Path::new(name), "fb2")) =>
            {
                Some(Self::Fb2)
            }
            "zip"
                if path
                    .file_stem()
                    .is_some_and(|name| has_extension(Path::new(name), "rtf")) =>
            {
                Some(Self::Rtf)
            }
            "zip" => {
                let stem = Path::new(path.file_stem()?);
                if has_extension(stem, "txt") || has_extension(stem, "log") {
                    Some(Self::Text)
                } else if has_extension(stem, "md") {
                    Some(Self::Markdown)
                } else if has_extension(stem, "html") || has_extension(stem, "htm") {
                    Some(Self::Htmlz)
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Fb2NoteKind {
    Footnote,
    Endnote,
}

impl Fb2NoteKind {
    fn from_body_name(name: &str) -> Option<Self> {
        if name.eq_ignore_ascii_case("notes") {
            Some(Self::Footnote)
        } else if name.eq_ignore_ascii_case("comments") {
            Some(Self::Endnote)
        } else {
            None
        }
    }
}

pub(crate) struct Document {
    pub(crate) metadata: crate::publication_metadata::Metadata,
    pub(crate) title: String,
    pub(crate) author: Option<String>,
    pub(crate) cover: Option<BookCover>,
    pub(crate) chapter_path: String,
    pub(crate) xhtml: String,
    pub(crate) toc: Vec<TocEntry>,
    pub(crate) resources: BTreeMap<String, Vec<u8>>,
    pub(crate) fb2_note_sections: BTreeMap<String, Fb2NoteKind>,
}

impl Document {
    /// Open the selected file with any supported resources contained inside it.
    #[cfg(test)]
    pub(crate) fn open(path: &Path, format: Format) -> Result<Self> {
        Self::open_classified(path, format).map(|(document, _)| document)
    }

    /// Keep the path-derived format for identity, but return the content format
    /// separately when an XML document was successfully decoded as FictionBook.
    pub(crate) fn open_classified(path: &Path, format: Format) -> Result<(Self, Format)> {
        let mut effective_format = format;
        let document = Self::open_inner(path, format, &mut effective_format)?;
        Ok((document, effective_format))
    }

    fn open_inner(path: &Path, format: Format, effective_format: &mut Format) -> Result<Self> {
        let metadata = fs::metadata(path)?;
        let limit = match format {
            Format::Fb2 => MAX_FB2_INPUT,
            Format::Htmlz => MAX_HTMLZ_INPUT,
            Format::Docx | Format::Docm => crate::docx::MAX_INPUT,
            Format::Odt => crate::odt::MAX_INPUT,
            Format::Rtf => crate::rtf::MAX_INPUT,
            Format::Tcr => crate::tcr::MAX_INPUT,
            _ => MAX_INPUT,
        };
        if !metadata.is_file() || metadata.len() > limit as u64 {
            return Err(format!(
                "The document is not a regular file or exceeds the {} KiB input limit",
                limit / 1024
            )
            .into());
        }
        let file = File::open(path)?;
        let fallback_title = fallback_title(path);
        if format == Format::Htmlz {
            return read_htmlz(file, metadata.len(), fallback_title);
        }
        if format == Format::Docx {
            return crate::docx::open(BufReader::new(file), metadata.len(), fallback_title);
        }
        if format == Format::Docm {
            return crate::docx::open_docm(BufReader::new(file), metadata.len(), fallback_title);
        }
        if format == Format::Odt {
            return crate::odt::open(BufReader::new(file), metadata.len(), fallback_title);
        }
        let bytes = if has_extension(path, "gz") {
            read_bounded(MultiGzDecoder::new(file), limit)?
        } else if has_extension(path, "zip") && format == Format::Rtf {
            read_rtf_zip(file, metadata.len())?
        } else if has_extension(path, "zip") && matches!(format, Format::Text | Format::Markdown) {
            let stem = Path::new(path.file_stem().ok_or("The text ZIP has no name")?);
            let extension = stem
                .extension()
                .and_then(|name| name.to_str())
                .ok_or("The text ZIP has no document extension")?;
            read_text_zip(file, metadata.len(), extension)?
        } else if has_extension(path, "zip") || has_extension(path, "fbz") {
            read_fb2_zip(file, metadata.len())?
        } else {
            read_bounded(file, limit)?
        };
        if format == Format::Rtf {
            return html_document(&crate::rtf::decode(&bytes)?, fallback_title);
        }
        let (bytes, format) = if format == Format::Tcr {
            (crate::tcr::decode(&bytes)?, Format::Text)
        } else {
            (bytes, format)
        };
        let input = decode_document(&bytes, format)?;
        let input = input.trim_start_matches('\u{feff}');
        match format {
            Format::Text => text_document(input, fallback_title),
            Format::Html => {
                if has_extension(path, "xml") {
                    let root = Xml::parse_with_options(
                        input,
                        ParsingOptions {
                            nodes_limit: 80_000,
                            ..ParsingOptions::default()
                        },
                    )
                    .ok()
                    .map(|xml| xml.root_element().tag_name().name().to_owned());
                    if root.as_deref() == Some("FictionBook") {
                        // fb2_document checks the exact FictionBook namespace as
                        // well as document validity before this affects behavior.
                        let document = fb2_document(input, fallback_title)?;
                        *effective_format = Format::Fb2;
                        return Ok(document);
                    }
                    if root.as_deref() != Some("html") {
                        // Generic XML has no universal document stylesheet. Preserve
                        // its original markup visibly instead of discarding content.
                        return text_document(input, fallback_title);
                    }
                }
                let mut document = html_document(input, fallback_title)?;
                load_local_html_resources(path, &mut document);
                Ok(document)
            }
            Format::Markdown => markdown_document(input, fallback_title),
            Format::Fb2 => fb2_document(input, fallback_title),
            Format::Htmlz
            | Format::Docx
            | Format::Docm
            | Format::Odt
            | Format::Fb3
            | Format::Chm
            | Format::Mobi
            | Format::Rtf
            | Format::Tcr => {
                unreachable!()
            }
        }
    }
}

pub(crate) fn fallback_title(path: &Path) -> &str {
    path.file_stem()
        .map(|name| {
            let stem = Path::new(name);
            if (has_extension(path, "zip") || has_extension(path, "gz"))
                && (has_extension(stem, "fb2")
                    || has_extension(stem, "rtf")
                    || has_extension(stem, "txt")
                    || has_extension(stem, "log")
                    || has_extension(stem, "md")
                    || has_extension(stem, "html")
                    || has_extension(stem, "htm"))
            {
                stem.file_stem().unwrap_or(name)
            } else {
                name
            }
        })
        .and_then(|name| name.to_str())
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or("Untitled document")
}

fn decode_document(bytes: &[u8], format: Format) -> Result<Cow<'_, str>> {
    let (encoding, offset) = if let Some(bom) = Encoding::for_bom(bytes) {
        bom
    } else if let Some(label) = declared_encoding(bytes, format) {
        (
            Encoding::for_label(label.as_bytes())
                .ok_or("The document declares an unknown encoding")?,
            0,
        )
    } else if format == Format::Text && std::str::from_utf8(bytes).is_err() {
        (WINDOWS_1252, 0)
    } else {
        (UTF_8, 0)
    };
    let text = encoding
        .decode_without_bom_handling_and_without_replacement(&bytes[offset..])
        .ok_or("The document does not match its text encoding")?;
    if text.contains('\0') {
        return Err("The document contains binary characters".into());
    }
    Ok(text)
}

fn declared_encoding(bytes: &[u8], format: Format) -> Option<String> {
    if format == Format::Text {
        return None;
    }
    let header = String::from_utf8_lossy(&bytes[..bytes.len().min(1024)]).to_ascii_lowercase();
    if let Some(xml) = header.trim_start().strip_prefix("<?xml")
        && let Some((declaration, _)) = xml.split_once("?>")
        && let Some(label) = attribute_value(declaration, "encoding")
    {
        return Some(label);
    }
    if format == Format::Html {
        for tag in header.split('<').filter_map(|tag| tag.strip_prefix("meta")) {
            if tag.starts_with(char::is_whitespace)
                && let Some((tag, _)) = tag.split_once('>')
                && let Some(label) = attribute_value(tag, "charset")
            {
                return Some(label);
            }
        }
    }
    None
}

fn attribute_value(tag: &str, attribute: &str) -> Option<String> {
    let (prefix, tail) = tag.split_once(attribute)?;
    if !prefix.ends_with(char::is_whitespace) {
        return None;
    }
    let tail = tail.trim_start().strip_prefix('=')?.trim_start();
    let value = if let Some(quote @ ('\'' | '"')) = tail.chars().next() {
        tail[1..].split_once(quote)?.0
    } else {
        tail.split(|ch: char| ch.is_whitespace() || ch == '/')
            .next()?
    };
    (!value.is_empty()).then(|| value.into())
}

fn has_extension(path: &Path, expected: &str) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case(expected))
}

pub(crate) fn read_bounded(input: impl Read, limit: usize) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    input.take(limit as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(format!(
            "The decoded document exceeds the {} KiB input limit",
            limit / 1024
        )
        .into());
    }
    Ok(bytes)
}

fn read_fb2_zip(file: File, length: u64) -> Result<Vec<u8>> {
    let mut input = BufReader::new(file);
    let count = check_zip_directory(&mut input, length)
        .map_err(|_| "The FictionBook ZIP directory is invalid or exceeds the reader limits")?;
    if count > MAX_FB2_ZIP_ENTRIES {
        return Err("The FictionBook ZIP has too many entries".into());
    }
    let mut archive = ZipArchive::new(input)?;
    if archive.len() != count {
        return Err("The FictionBook ZIP has ambiguous or duplicate entries".into());
    }
    let mut book = None;
    for index in 0..archive.len() {
        let entry = archive.by_index_raw(index)?;
        let is_book = entry.is_file()
            && entry
                .enclosed_name()
                .is_some_and(|name| has_extension(&name, "fb2"));
        if is_book && book.replace(index).is_some() {
            return Err("The FictionBook ZIP contains more than one .fb2 file".into());
        }
    }
    let entry =
        archive.by_index(book.ok_or("The FictionBook ZIP contains no readable .fb2 file")?)?;
    if entry.size() > MAX_FB2_INPUT as u64 {
        return Err("The FictionBook ZIP's .fb2 file exceeds the input limit".into());
    }
    read_bounded(entry, MAX_FB2_INPUT)
}

fn read_rtf_zip(file: File, length: u64) -> Result<Vec<u8>> {
    let mut input = BufReader::new(file);
    let count = check_zip_directory(&mut input, length)
        .map_err(|_| "The RTF ZIP directory is invalid or exceeds Reader's limits")?;
    if count > MAX_FB2_ZIP_ENTRIES {
        return Err("The RTF ZIP has too many entries".into());
    }
    let mut archive = ZipArchive::new(input)?;
    if archive.len() != count {
        return Err("The RTF ZIP has ambiguous or duplicate entries".into());
    }
    let mut document = None;
    for index in 0..archive.len() {
        let entry = archive.by_index_raw(index)?;
        let is_document = entry.is_file()
            && entry
                .enclosed_name()
                .is_some_and(|name| has_extension(&name, "rtf"));
        if is_document {
            if entry.encrypted()
                || entry.is_symlink()
                || !matches!(
                    entry.compression(),
                    CompressionMethod::Stored | CompressionMethod::Deflated
                )
            {
                return Err(
                    "The RTF ZIP contains an encrypted, linked or unsupported document".into(),
                );
            }
            if document.replace(index).is_some() {
                return Err("The RTF ZIP contains more than one .rtf file".into());
            }
        }
    }
    let entry = archive.by_index(document.ok_or("The RTF ZIP contains no readable .rtf file")?)?;
    if entry.size() > crate::rtf::MAX_INPUT as u64 {
        return Err("The RTF ZIP's document exceeds Reader's input limit".into());
    }
    read_bounded(entry, crate::rtf::MAX_INPUT)
}

fn read_text_zip(file: File, length: u64, extension: &str) -> Result<Vec<u8>> {
    let mut input = BufReader::new(file);
    let count = check_zip_directory(&mut input, length)
        .map_err(|_| "The text ZIP directory is invalid or exceeds Reader's limits")?;
    if count > MAX_FB2_ZIP_ENTRIES {
        return Err("The text ZIP has too many entries".into());
    }
    let mut archive = ZipArchive::new(input)?;
    if archive.len() != count {
        return Err("The text ZIP has ambiguous or duplicate entries".into());
    }
    let mut document = None;
    for index in 0..archive.len() {
        let entry = archive.by_index_raw(index)?;
        let path = entry
            .enclosed_name()
            .ok_or("The text ZIP has an unsafe entry path")?;
        if entry.is_symlink()
            || entry.encrypted()
            || !matches!(
                entry.compression(),
                CompressionMethod::Stored | CompressionMethod::Deflated
            )
        {
            return Err("The text ZIP contains an encrypted, linked or unsupported entry".into());
        }
        if entry.is_file() && has_extension(&path, extension) && document.replace(index).is_some() {
            return Err("The text ZIP contains more than one matching document".into());
        }
    }
    let entry =
        archive.by_index(document.ok_or("The text ZIP contains no matching readable document")?)?;
    if entry.size() > MAX_INPUT as u64 {
        return Err("The text ZIP's document exceeds Reader's input limit".into());
    }
    read_bounded(entry, MAX_INPUT)
}

fn read_htmlz(file: File, length: u64, fallback: &str) -> Result<Document> {
    let mut input = BufReader::new(file);
    let count = check_zip_directory(&mut input, length)
        .map_err(|_| "The HTMLZ has an invalid ZIP directory")?;
    if count > MAX_HTMLZ_ENTRIES {
        return Err("The HTMLZ ZIP has too many entries".into());
    }
    let mut archive = ZipArchive::new(input)?;
    if archive.len() != count {
        return Err("The HTMLZ ZIP has ambiguous or duplicate entries".into());
    }
    let mut chapter = None;
    let mut metadata = None;
    let mut resources = Vec::new();
    let mut remaining = MAX_HTMLZ_RESOURCES as u64;
    for index in 0..archive.len() {
        let entry = archive.by_index_raw(index)?;
        let name = entry.name();
        let path = if entry.is_dir() {
            name.strip_suffix('/').unwrap_or(name)
        } else {
            name
        };
        if path.len() > MAX_HTMLZ_PATH || !is_archive_path(path) || entry.is_symlink() {
            return Err("The HTMLZ ZIP has an unsafe entry path".into());
        }
        if entry.encrypted()
            || !matches!(
                entry.compression(),
                CompressionMethod::Stored | CompressionMethod::Deflated
            )
        {
            return Err("The HTMLZ ZIP contains an encrypted or unsupported entry".into());
        }
        if !entry.is_file() {
            continue;
        }
        match path
            .rsplit_once('.')
            .map(|(_, ext)| ext.to_ascii_lowercase())
            .as_deref()
        {
            Some("htm" | "html" | "xht" | "xhtml") if !path.contains('/') => {
                if chapter.replace((index, path.to_owned())).is_some() {
                    return Err("The HTMLZ ZIP has more than one top-level HTML document".into());
                }
            }
            Some("opf") if path.eq_ignore_ascii_case("metadata.opf") => {
                if metadata.replace(index).is_some() {
                    return Err("The HTMLZ ZIP has more than one metadata entry".into());
                }
            }
            Some(extension @ ("css" | "jpeg" | "jpg" | "png" | "gif" | "svg")) => {
                let limit = if extension == "css" {
                    MAX_HTMLZ_CSS
                } else {
                    MAX_HTMLZ_IMAGE
                };
                if entry.size() > limit as u64 {
                    return Err("An HTMLZ resource exceeds its size limit".into());
                }
                remaining = remaining
                    .checked_sub(entry.size())
                    .ok_or("The HTMLZ exceeds its total resource limit")?;
                resources.push((index, path.to_owned(), limit));
            }
            _ => {}
        }
    }
    let (index, chapter_path) = chapter.ok_or("The HTMLZ ZIP has no top-level HTML document")?;
    let bytes = read_htmlz_member(&mut archive, index, MAX_INPUT)?;
    let input = decode_document(&bytes, Format::Html)?;
    // Calibre may emit named HTML entities in otherwise XML-compatible markup.
    let input = input
        .replace("&shy;", "\u{ad}")
        .replace("&mdash;", "—")
        .replace("&ndash;", "–")
        .replace("&nbsp;", "\u{a0}");
    check_output(&input)?;
    let mut document = html_document(&input, fallback)?;
    document.chapter_path = chapter_path;
    add_htmlz_contents(&mut document)?;
    if let Some(index) = metadata
        && let Some((title, author, metadata)) =
            htmlz_metadata(&read_htmlz_member(&mut archive, index, MAX_HTMLZ_METADATA)?)
    {
        if let Some(title) = title {
            document.title = title;
        }
        if let Some(author) = author {
            document.author = Some(author);
        }
        document.metadata = metadata;
    }
    for (index, name, limit) in resources {
        let bytes = read_htmlz_member(&mut archive, index, limit)?;
        document.resources.insert(name, bytes);
    }
    Ok(document)
}

fn read_htmlz_member(
    archive: &mut ZipArchive<BufReader<File>>,
    index: usize,
    limit: usize,
) -> Result<Vec<u8>> {
    let entry = archive.by_index(index)?;
    let size = usize::try_from(entry.size())?;
    if size > limit {
        return Err("An HTMLZ member exceeds its size limit".into());
    }
    let mut bytes = Vec::with_capacity(size);
    entry.take(size as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() != size {
        return Err("An HTMLZ member's size does not match its ZIP directory".into());
    }
    Ok(bytes)
}

fn htmlz_metadata(
    bytes: &[u8],
) -> Option<(
    Option<String>,
    Option<String>,
    crate::publication_metadata::Metadata,
)> {
    const DC: &str = "http://purl.org/dc/elements/1.1/";
    let xml = decode_document(bytes, Format::Htmlz).ok()?;
    let document = Xml::parse_with_options(
        &xml,
        ParsingOptions {
            nodes_limit: 4_000,
            ..ParsingOptions::default()
        },
    )
    .ok()?;
    let metadata = document
        .root_element()
        .children()
        .find(|node| node.has_tag_name("metadata"))?;
    let value = |tag| {
        metadata
            .children()
            .filter(|node| node.has_tag_name((DC, tag)))
            .filter_map(|node| node.text().map(str::trim))
            .find(|text| !text.is_empty())
            .map(str::to_owned)
    };
    Some((
        value("title"),
        value("creator"),
        crate::publication_metadata::Metadata::opf(metadata),
    ))
}

fn add_htmlz_contents(document: &mut Document) -> Result<()> {
    let xml = Xml::parse_with_options(
        &document.xhtml,
        ParsingOptions {
            nodes_limit: 80_000,
            ..ParsingOptions::default()
        },
    )?;
    let body = xml
        .root_element()
        .children()
        .find(|node| node.has_tag_name("body"))
        .ok_or("The HTMLZ entry has no body")?;
    let ids = body
        .descendants()
        .filter_map(|node| node.attribute("id"))
        .collect::<BTreeSet<_>>();
    let mut next_id = 0;
    let mut generated = Vec::new();
    for node in body.descendants().filter(Node::is_element) {
        if document.toc.len() == MAX_HTMLZ_HEADINGS {
            break;
        }
        let depth = match node.tag_name().name() {
            "h1" => 0,
            "h2" => 1,
            "h3" => 2,
            "h4" => 3,
            "h5" => 4,
            "h6" => 5,
            _ => continue,
        };
        if node.ancestors().any(|node| {
            node.has_attribute("hidden")
                || matches!(
                    node.tag_name().name(),
                    "script" | "style" | "noscript" | "template" | "svg"
                )
        }) {
            continue;
        }
        let label = node
            .descendants()
            .filter(Node::is_text)
            .filter_map(|node| node.text())
            .flat_map(str::split_whitespace)
            .take(25)
            .collect::<Vec<_>>()
            .join(" ");
        if label.is_empty() {
            continue;
        }
        let fragment = if let Some(id) = node.attribute("id") {
            if id.is_empty() || id.chars().any(char::is_control) {
                continue;
            }
            id.to_owned()
        } else {
            let id = loop {
                next_id += 1;
                let id = format!("reader-htmlz-heading-{next_id}");
                if !ids.contains(id.as_str()) {
                    break id;
                }
            };
            let start = node.range().start;
            let offset = start
                + document.xhtml[start..]
                    .find(|ch: char| ch.is_ascii_whitespace() || matches!(ch, '/' | '>'))
                    .ok_or("The HTMLZ entry has an incomplete heading")?;
            generated.push((offset, id.clone()));
            id
        };
        document.toc.push(TocEntry {
            label,
            depth,
            destination: Some(Destination {
                path: document.chapter_path.clone(),
                fragment: Some(fragment),
            }),
        });
    }
    if !generated.is_empty() {
        let mut previous_end = 0;
        let mut output = String::with_capacity(document.xhtml.len());
        for (offset, id) in generated {
            output.push_str(&document.xhtml[previous_end..offset]);
            output.push_str(" id=\"");
            output.push_str(&id);
            output.push('"');
            previous_end = offset;
        }
        output.push_str(&document.xhtml[previous_end..]);
        check_output(&output)?;
        document.xhtml = output;
    }
    Ok(())
}

fn text_document(input: &str, title: &str) -> Result<Document> {
    let cleaned = strip_terminal_colors(input);
    let start = cleaned.trim_start();
    let html_tag = start
        .get(..5)
        .is_some_and(|tag| tag.eq_ignore_ascii_case("<html"))
        && start
            .as_bytes()
            .get(5)
            .is_some_and(|byte| *byte == b'>' || byte.is_ascii_whitespace());
    let html_doctype = start
        .get(..15)
        .is_some_and(|head| head.eq_ignore_ascii_case("<!doctype html>"));
    if html_tag || html_doctype {
        // Some reading material distributed as .txt is an HTML export. Preserve well-formed
        // markup and otherwise use the existing bounded HTML tokenizer for the visible text.
        if let Ok(document) = html_document(&cleaned, title) {
            return Ok(document);
        }
        return html_document(&crate::mobi::html_xhtml(&cleaned)?, title);
    }
    Ok(Document {
        title: title.to_owned(),
        metadata: Default::default(),
        author: None,
        cover: None,
        chapter_path: CHAPTER_PATH.to_owned(),
        xhtml: text_as_xhtml(&cleaned)?,
        toc: Vec::new(),
        resources: BTreeMap::new(),
        fb2_note_sections: BTreeMap::new(),
    })
}

fn strip_terminal_colors(input: &str) -> Cow<'_, str> {
    if !input.contains('\x1b') {
        return Cow::Borrowed(input);
    }
    let mut output = String::with_capacity(input.len());
    let mut rest = input;
    while let Some((text, sequence)) = rest.split_once('\x1b') {
        output.push_str(text);
        if let Some(params) = sequence.strip_prefix('[') {
            let length = params
                .bytes()
                .take_while(|b| b.is_ascii_digit() || matches!(b, b';' | b':'))
                .count();
            if let Some(after) = params[length..].strip_prefix('m') {
                rest = after;
                continue;
            }
        }
        output.push('\x1b');
        rest = sequence;
    }
    output.push_str(rest);
    Cow::Owned(output)
}

pub(crate) fn text_as_xhtml(input: &str) -> Result<String> {
    if input.trim().is_empty() {
        return Err("The text file is empty".into());
    }
    let mut output =
        String::from("<html xmlns=\"http://www.w3.org/1999/xhtml\"><head></head><body>");
    let input = input.replace("\r\n", "\n").replace('\r', "\n");
    let mut in_paragraph = false;
    for line in input.lines() {
        if line.trim().is_empty() {
            if in_paragraph {
                output.push_str("</p>");
                in_paragraph = false;
            }
            continue;
        }
        output.push_str(if in_paragraph { "<br />" } else { "<p>" });
        in_paragraph = true;
        xml_text(&mut output, line)?;
    }
    if in_paragraph {
        output.push_str("</p>");
    }
    output.push_str("</body></html>");
    check_output(&output)?;
    Ok(output)
}

pub(crate) fn xml_text(output: &mut String, text: &str) -> Result<()> {
    for ch in text.chars() {
        match ch {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            '\r'
            | '\n'
            | '\t'
            | '\u{20}'..='\u{d7ff}'
            | '\u{e000}'..='\u{fffd}'
            | '\u{10000}'..='\u{10ffff}' => {
                output.push(ch);
            }
            _ => return Err("The document contains an unsupported control character".into()),
        }
        check_output(output)?;
    }
    Ok(())
}

pub(crate) fn check_output(output: &str) -> Result<()> {
    if output.len() > MAX_OUTPUT {
        return Err("The converted document exceeds the one MiB layout input limit".into());
    }
    Ok(())
}

fn html_document(input: &str, fallback_title: &str) -> Result<Document> {
    let xhtml = without_plain_html_doctype(input);
    let document = Xml::parse_with_options(
        &xhtml,
        ParsingOptions {
            nodes_limit: 80_000,
            ..ParsingOptions::default()
        },
    )
    .map_err(|error| {
        format!("Reader requires well-formed XHTML or XML-compatible HTML: {error}")
    })?;
    let html = document.root_element();
    if !html.has_tag_name("html") || !html.children().any(|node| node.has_tag_name("body")) {
        return Err("The HTML document must contain an html element with a body".into());
    }
    let head = html.children().find(|node| node.has_tag_name("head"));
    let title = head
        .and_then(|head| head.children().find(|node| node.has_tag_name("title")))
        .and_then(|title| title.text())
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .unwrap_or(fallback_title)
        .to_owned();
    let author = head.and_then(|head| {
        head.children()
            .filter(|node| {
                node.has_tag_name("meta")
                    && node
                        .attribute("name")
                        .is_some_and(|name| name.eq_ignore_ascii_case("author"))
            })
            .filter_map(|node| node.attribute("content").map(str::trim))
            .find(|name| !name.is_empty())
            .map(str::to_owned)
    });
    Ok(Document {
        title,
        author,
        cover: None,
        metadata: crate::publication_metadata::Metadata::html(html),
        chapter_path: CHAPTER_PATH.to_owned(),
        xhtml: xhtml.into_owned(),
        toc: Vec::new(),
        resources: BTreeMap::new(),
        fb2_note_sections: BTreeMap::new(),
    })
}

/// Snapshot only directly referenced local assets, without changing XHTML or
/// its source revision. Failure to load an optional asset leaves text readable.
fn load_local_html_resources(path: &Path, document: &mut Document) {
    const MAX_REFERENCES: usize = 64;
    let references = {
        let Ok(xml) = Xml::parse_with_options(
            &document.xhtml,
            ParsingOptions {
                nodes_limit: 80_000,
                ..ParsingOptions::default()
            },
        ) else {
            return;
        };
        let mut found = BTreeSet::new();
        for node in xml.descendants().filter(Node::is_element) {
            let tag = node.tag_name().name();
            let value = if tag == "img" {
                node.attribute("src")
            } else if tag == "image"
                && node.tag_name().namespace() == Some("http://www.w3.org/2000/svg")
            {
                node.attribute("href")
                    .or_else(|| node.attribute(("http://www.w3.org/1999/xlink", "href")))
            } else if tag == "link"
                && node.attribute("rel").is_some_and(|rel| {
                    rel.split_ascii_whitespace()
                        .any(|token| token.eq_ignore_ascii_case("stylesheet"))
                })
            {
                node.attribute("href")
            } else {
                None
            };
            if let Some(value) = value
                && let Ok(relative) = Book::resolve(CHAPTER_PATH, value)
                && local_html_resource_limit(&relative).is_some()
            {
                found.insert(relative);
                if found.len() == MAX_REFERENCES {
                    break;
                }
            }
        }
        found
    };
    let Some(parent) = path.parent() else { return };
    let Ok(root) = parent.canonicalize() else {
        return;
    };
    let Some(root) = open_local_html_root(&root) else {
        return;
    };
    let mut remaining = MAX_HTMLZ_RESOURCES;
    for relative in references {
        let Some(limit) = local_html_resource_limit(&relative) else {
            continue;
        };
        let Some(file) = open_local_html_resource(&root, &relative) else {
            continue;
        };
        let Ok(meta) = file.metadata() else { continue };
        let limit = limit.min(remaining);
        if !meta.is_file() || meta.len() > limit as u64 {
            continue;
        }
        let Ok(bytes) = read_bounded(file, limit) else {
            continue;
        };
        remaining -= bytes.len();
        document.resources.insert(relative, bytes);
    }
}

fn local_html_resource_limit(path: &str) -> Option<usize> {
    let extension = path.rsplit_once('.')?.1.to_ascii_lowercase();
    match extension.as_str() {
        "css" => Some(MAX_HTMLZ_CSS),
        "png" | "jpg" | "jpeg" | "gif" | "svg" | "bmp" | "pnm" | "pbm" | "pgm" | "ppm" | "pam" => {
            Some(MAX_HTMLZ_IMAGE)
        }
        _ => None,
    }
}

#[cfg(unix)]
fn open_local_html_root(path: &Path) -> Option<File> {
    use std::os::unix::fs::OpenOptionsExt;
    fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .ok()
}

#[cfg(unix)]
fn open_local_html_resource(root: &File, relative: &str) -> Option<File> {
    use std::ffi::CString;
    use std::os::fd::{AsRawFd, FromRawFd};
    if !is_archive_path(relative)
        || relative.len() > MAX_HTMLZ_PATH
        || relative.split('/').count() > 32
    {
        return None;
    }
    let mut current = root.try_clone().ok()?;
    let mut components = relative.split('/').peekable();
    while let Some(component) = components.next() {
        let name = CString::new(component).ok()?;
        let mut flags = libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK;
        if components.peek().is_some() {
            flags |= libc::O_DIRECTORY;
        }
        // Each component is opened relative to the already-open directory, so
        // replacing an ancestor with a symlink cannot escape the selected root.
        // SAFETY: current owns a live directory descriptor and name is NUL-terminated.
        let fd = unsafe { libc::openat(current.as_raw_fd(), name.as_ptr(), flags) };
        if fd < 0 {
            return None;
        }
        // SAFETY: openat returned a new owned file descriptor.
        current = unsafe { File::from_raw_fd(fd) };
    }
    Some(current)
}

#[cfg(not(unix))]
fn open_local_html_root(_path: &Path) -> Option<File> {
    None
}

#[cfg(not(unix))]
fn open_local_html_resource(_root: &File, _relative: &str) -> Option<File> {
    None
}

fn markdown_document(input: &str, fallback_title: &str) -> Result<Document> {
    let mut xhtml =
        String::from("<html xmlns=\"http://www.w3.org/1999/xhtml\"><head></head><body>");
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_HEADING_ATTRIBUTES;
    let mut heading = 0;
    let events = Parser::new_ext(input, options).map(|event| match event {
        Event::Html(text) | Event::InlineHtml(text) => Event::Text(text),
        Event::Start(Tag::Heading {
            level,
            id,
            classes,
            attrs,
        }) => {
            heading += 1;
            Event::Start(Tag::Heading {
                level,
                id: id.or_else(|| Some(format!("reader-heading-{heading}").into())),
                classes,
                attrs,
            })
        }
        event => event,
    });
    pulldown_cmark::html::push_html(&mut xhtml, events);
    xhtml.push_str("</body></html>");
    if xhtml.len() > MAX_OUTPUT {
        return Err("The Markdown document exceeds the reading layout limit".into());
    }
    let mut document = html_document(&xhtml, fallback_title)?;
    let xml = Xml::parse(&xhtml)?;
    for node in xml.descendants().filter(Node::is_element) {
        let level = match node.tag_name().name() {
            "h1" => HeadingLevel::H1,
            "h2" => HeadingLevel::H2,
            "h3" => HeadingLevel::H3,
            "h4" => HeadingLevel::H4,
            "h5" => HeadingLevel::H5,
            "h6" => HeadingLevel::H6,
            _ => continue,
        };
        let label = node
            .descendants()
            .filter(Node::is_text)
            .filter_map(|text| text.text())
            .flat_map(str::split_whitespace)
            .take(25)
            .collect::<Vec<_>>()
            .join(" ");
        if label.is_empty() || document.toc.len() >= 512 {
            continue;
        }
        document.toc.push(TocEntry {
            depth: level as usize - 1,
            label,
            destination: node.attribute("id").map(|id| Destination {
                path: CHAPTER_PATH.into(),
                fragment: Some(id.into()),
            }),
        });
    }
    Ok(document)
}

/// Permit only the short HTML5 declaration. XML parsing keeps DTD processing disabled.
pub(crate) fn without_plain_html_doctype(input: &str) -> Cow<'_, str> {
    let input = input.trim_start();
    if let Some(range) = html_doctype(input) {
        let mut xhtml = input.to_owned();
        xhtml.replace_range(range, "");
        Cow::Owned(xhtml)
    } else {
        Cow::Borrowed(input)
    }
}

fn html_doctype(input: &str) -> Option<std::ops::Range<usize>> {
    const HTML_DOCTYPE: &str = "<!DOCTYPE html>";
    let after_declaration = if input.starts_with("<?xml ") {
        input.find("?>")? + 2
    } else {
        0
    };
    let remainder = input[after_declaration..].trim_start();
    let start = input.len() - remainder.len();
    let end = start + HTML_DOCTYPE.len();
    input
        .get(start..end)?
        .eq_ignore_ascii_case(HTML_DOCTYPE)
        .then_some(start..end)
}

// FictionBook 2 schema: https://github.com/gribuser/fb2/blob/master/FictionBook.xsd
fn fb2_document(input: &str, fallback_title: &str) -> Result<Document> {
    let book = Xml::parse_with_options(
        input,
        ParsingOptions {
            nodes_limit: 80_000,
            ..ParsingOptions::default()
        },
    )
    .map_err(|error| {
        format!("Reader requires a well-formed UTF-8 FictionBook 2 document: {error}")
    })?;
    let root = book.root_element();
    if !root.has_tag_name((FB2_NS, "FictionBook")) {
        return Err("The file has no FictionBook 2 root with the required namespace".into());
    }
    let info = fb2_child(root, "description").and_then(|item| fb2_child(item, "title-info"));
    let book_title = info
        .and_then(|item| fb2_child(item, "book-title"))
        .and_then(fb2_text);
    let title = book_title.unwrap_or(fallback_title).to_owned();

    let mut xhtml = String::from("<html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>");
    // Keep fallback filenames out of the layout input so its revision survives rename.
    xml_text(&mut xhtml, book_title.unwrap_or_default())?;
    xhtml.push_str("</title>");
    let mut authors = Vec::new();
    if let Some(info) = info {
        for author in info
            .children()
            .filter(|item| item.has_tag_name((FB2_NS, "author")))
        {
            let name = ["first-name", "middle-name", "last-name"]
                .into_iter()
                .filter_map(|part| fb2_child(author, part).and_then(fb2_text))
                .collect::<Vec<_>>()
                .join(" ");
            let name = if name.is_empty() {
                fb2_child(author, "nickname")
                    .and_then(fb2_text)
                    .unwrap_or_default()
            } else {
                &name
            };
            if !name.is_empty() {
                xhtml.push_str("<meta name=\"author\" content=\"");
                xml_text(&mut xhtml, name)?;
                xhtml.push_str("\" />");
                authors.push(name.to_owned());
            }
        }
    }
    xhtml.push_str("</head><body>");
    let mut bodies = root
        .children()
        .filter(|item| item.has_tag_name((FB2_NS, "body")))
        .peekable();
    if bodies.peek().is_none() {
        return Err("The FictionBook has no reading body".into());
    }
    let mut authored_ids = BTreeSet::new();
    let mut note_sections = BTreeMap::new();
    for item in root
        .descendants()
        .filter(|item| item.is_element() && item.tag_name().namespace() == Some(FB2_NS))
    {
        let Some(id) = item.attribute("id").filter(|id| !id.is_empty()) else {
            continue;
        };
        // A duplicate destination is ambiguous, even if only one occurrence
        // belongs to a note. Leave ordinary link handling in charge.
        if !authored_ids.insert(id) {
            note_sections.remove(id);
        } else if item.has_tag_name((FB2_NS, "section"))
            && let Some(kind) = item
                .ancestors()
                .skip(1)
                .find(|parent| parent.has_tag_name((FB2_NS, "body")))
                .and_then(|body| body.attribute("name"))
                .and_then(Fb2NoteKind::from_body_name)
        {
            note_sections.insert(id.to_owned(), kind);
        }
    }
    let mut output = Fb2Output {
        xhtml,
        toc: Vec::new(),
        images: fb2_images(root),
        image_placements: 0,
        authored_ids,
        section_id: 0,
    };
    let cover = info
        .and_then(|item| fb2_child(item, "coverpage"))
        .and_then(|item| fb2_child(item, "image"))
        .and_then(fb2_image_id)
        .and_then(|id| output.images.get(id.as_str()))
        .and_then(|image| {
            let kind = if image.path.ends_with(".png") {
                CoverKind::Png
            } else if image.path.ends_with(".jpg") {
                CoverKind::Jpeg
            } else {
                return None;
            };
            Some(BookCover {
                path: image.path.clone(),
                kind,
            })
        });
    if let Some(cover) = info
        .and_then(|item| fb2_child(item, "coverpage"))
        .and_then(|item| fb2_child(item, "image"))
    {
        output.write_image(cover)?;
    }
    for body in bodies {
        output.write(body, 0)?;
    }
    output.xhtml.push_str("</body></html>");
    check_output(&output.xhtml)?;
    Ok(Document {
        title,
        author: (!authors.is_empty()).then(|| authors.join(", ")),
        cover,
        metadata: info
            .map(|info| crate::publication_metadata::Metadata::fb2(info, input))
            .unwrap_or_default(),
        chapter_path: CHAPTER_PATH.to_owned(),
        xhtml: output.xhtml,
        toc: output.toc,
        fb2_note_sections: note_sections,
        resources: output
            .images
            .into_values()
            .map(|image| (image.path, image.bytes))
            .collect(),
    })
}

fn fb2_child<'a, 'input>(parent: Node<'a, 'input>, tag: &str) -> Option<Node<'a, 'input>> {
    parent
        .children()
        .find(|item| item.has_tag_name((FB2_NS, tag)))
}

fn fb2_text<'a, 'input>(item: Node<'a, 'input>) -> Option<&'a str> {
    item.text().map(str::trim).filter(|text| !text.is_empty())
}

struct Fb2Output<'a> {
    xhtml: String,
    toc: Vec<TocEntry>,
    authored_ids: BTreeSet<&'a str>,
    section_id: usize,
    images: BTreeMap<&'a str, Fb2Image>,
    image_placements: usize,
}

impl Fb2Output<'_> {
    fn next_section_id(&mut self) -> String {
        loop {
            self.section_id += 1;
            let id = format!("primitiveos-fb2-section-{}", self.section_id);
            if !self.authored_ids.contains(id.as_str()) {
                return id;
            }
        }
    }

    fn write(&mut self, item: Node<'_, '_>, depth: usize) -> Result<()> {
        if depth > MAX_FB2_DEPTH {
            return Err("The FictionBook exceeds the section nesting limit".into());
        }
        if item.is_text() {
            return xml_text(&mut self.xhtml, item.text().unwrap_or_default());
        }
        if !item.is_element() || item.tag_name().namespace() != Some(FB2_NS) {
            return Ok(());
        }
        if item.has_tag_name((FB2_NS, "image")) {
            return self.write_image(item);
        }
        let tag = match item.tag_name().name() {
            "body" if item.attribute("name") == Some("notes") => "aside",
            "body" => "article",
            "section" => "section",
            "annotation" | "title" | "poem" | "stanza" => "div",
            "cite" | "epigraph" => "blockquote",
            "p" if item
                .parent()
                .is_some_and(|parent| parent.has_tag_name((FB2_NS, "title"))) =>
            {
                let section_depth = item
                    .ancestors()
                    .filter(|node| node.has_tag_name((FB2_NS, "section")))
                    .count();
                ["h1", "h2", "h3", "h4", "h5", "h6"][section_depth.min(5)]
            }
            "p" | "v" | "date" | "text-author" => "p",
            "subtitle" => "h3",
            "strong" => "strong",
            "emphasis" => "em",
            "strikethrough" => "s",
            "sub" => "sub",
            "sup" => "sup",
            "code" => "code",
            "style" => "span",
            "a" => "a",
            "empty-line" => {
                self.xhtml.push_str("<p>&#160;</p>");
                return check_output(&self.xhtml);
            }
            _ => return Ok(()),
        };
        let id = item
            .attribute("id")
            .filter(|id| !id.is_empty())
            .map(Cow::Borrowed)
            .or_else(|| (tag == "section").then(|| Cow::Owned(self.next_section_id())));
        if tag == "section"
            && let Some(label) = fb2_section_title(item)
        {
            let depth = item
                .ancestors()
                .skip(1)
                .filter(|node| node.has_tag_name((FB2_NS, "section")))
                .count();
            self.toc.push(TocEntry {
                label,
                depth,
                destination: Some(Destination {
                    path: CHAPTER_PATH.to_owned(),
                    fragment: id.as_deref().map(str::to_owned),
                }),
            });
        }
        self.xhtml.push('<');
        self.xhtml.push_str(tag);
        if let Some(id) = id {
            self.xhtml.push_str(" id=\"");
            xml_text(&mut self.xhtml, &id)?;
            self.xhtml.push('"');
        }
        let class = match item.tag_name().name() {
            "poem" => Some("fb2-poem"),
            "stanza" => Some("fb2-stanza"),
            "annotation" => Some("fb2-annotation"),
            "epigraph" => Some("fb2-epigraph"),
            "cite" => Some("fb2-cite"),
            "v" => Some("fb2-verse"),
            "text-author" => Some("fb2-text-author"),
            "date" => Some("fb2-date"),
            _ => None,
        };
        if let Some(class) = class {
            self.xhtml.push_str(" class=\"");
            self.xhtml.push_str(class);
            self.xhtml.push('\"');
        }
        if tag == "a"
            && let Some(href) = item
                .attribute((XLINK_NS, "href"))
                .filter(|href| href.starts_with('#') && href.len() > 1)
        {
            self.xhtml.push_str(" href=\"");
            xml_text(&mut self.xhtml, href)?;
            self.xhtml.push('"');
        }
        self.xhtml.push('>');
        if tag == "aside" && fb2_child(item, "title").is_none() {
            self.xhtml.push_str("<h1>Notes</h1>");
        }
        for child in item.children() {
            self.write(child, depth + 1)?;
        }
        self.xhtml.push_str("</");
        self.xhtml.push_str(tag);
        self.xhtml.push('>');
        check_output(&self.xhtml)
    }

    fn write_image(&mut self, image: Node<'_, '_>) -> Result<()> {
        let inline = image.ancestors().skip(1).any(|node| {
            node.is_element()
                && node.tag_name().namespace() == Some(FB2_NS)
                && matches!(
                    node.tag_name().name(),
                    "p" | "v" | "subtitle" | "text-author"
                )
        });
        let alt = image.attribute("alt").unwrap_or_default();
        let id = fb2_image_id(image);
        if self.image_placements < MAX_FB2_IMAGES
            && let Some(asset) = id.as_deref().and_then(|id| self.images.get(id))
        {
            self.image_placements += 1;
            if !inline {
                self.xhtml.push_str("<figure>");
            }
            self.xhtml.push_str("<img src=\"");
            xml_text(&mut self.xhtml, &asset.path)?;
            self.xhtml.push_str("\" alt=\"");
            xml_text(&mut self.xhtml, alt)?;
            self.xhtml.push_str("\" />");
            if !inline {
                self.xhtml.push_str("</figure>");
            }
        } else if !alt.trim().is_empty() {
            if !inline {
                self.xhtml.push_str("<p>");
            }
            xml_text(&mut self.xhtml, alt)?;
            if !inline {
                self.xhtml.push_str("</p>");
            }
        }
        check_output(&self.xhtml)
    }
}

struct Fb2Image {
    path: String,
    bytes: Vec<u8>,
}

fn fb2_image_id(image: Node<'_, '_>) -> Option<String> {
    let href = image
        .attribute((XLINK_NS, "href"))
        .filter(|href| href.starts_with('#'))?;
    Book::resolve_link(CHAPTER_PATH, href).ok()?.fragment
}

fn fb2_images<'a, 'input>(root: Node<'a, 'input>) -> BTreeMap<&'a str, Fb2Image> {
    let wanted: BTreeSet<_> = root
        .descendants()
        .filter(|item| item.has_tag_name((FB2_NS, "image")))
        .filter_map(fb2_image_id)
        .take(MAX_FB2_IMAGES * 2)
        .collect();
    let mut images = BTreeMap::new();
    let mut bytes_left = MAX_FB2_IMAGE_BYTES;
    let mut pixels_left = MAX_FB2_IMAGE_PIXELS;
    for binary in root
        .children()
        .filter(|item| item.has_tag_name((FB2_NS, "binary")))
    {
        if images.len() == MAX_FB2_IMAGES || bytes_left == 0 || pixels_left == 0 {
            break;
        }
        let Some(id) = binary
            .attribute("id")
            .filter(|id| wanted.contains(*id) && !images.contains_key(*id))
        else {
            continue;
        };
        let Some((bytes, extension, pixels)) =
            fb2_image_bytes(binary, bytes_left.min(MAX_FB2_IMAGE))
        else {
            continue;
        };
        if pixels > pixels_left {
            continue;
        }
        bytes_left -= bytes.len();
        pixels_left -= pixels;
        images.insert(
            id,
            Fb2Image {
                path: format!("fb2-images/{}.{}", images.len() + 1, extension),
                bytes,
            },
        );
    }
    images
}

fn fb2_image_bytes(
    binary: Node<'_, '_>,
    max_bytes: usize,
) -> Option<(Vec<u8>, &'static str, usize)> {
    let (format, extension) = match binary.attribute("content-type")? {
        "image/png" => (ImageFormat::Png, "png"),
        "image/jpeg" | "image/jpg" => (ImageFormat::Jpeg, "jpg"),
        _ => return None,
    };
    let max_encoded = max_bytes.div_ceil(3) * 4;
    let encoded: Vec<u8> = binary
        .children()
        .filter(Node::is_text)
        .filter_map(|item| item.text())
        .flat_map(str::bytes)
        .filter(|byte| !byte.is_ascii_whitespace())
        .take(max_encoded + 1)
        .collect();
    if encoded.len() > max_encoded {
        return None;
    }
    let bytes = STANDARD_PAD_INDIFFERENT.decode(encoded).ok()?;
    if bytes.len() > max_bytes {
        return None;
    }
    let reader = crate::image_reader(&bytes).ok()?;
    if reader.format() != Some(format) {
        return None;
    }
    let (width, height) = reader.into_dimensions().ok()?;
    if !(1..=2048).contains(&width) || !(1..=2048).contains(&height) {
        return None;
    }
    Some((bytes, extension, (width * height) as usize))
}

fn fb2_section_title(section: Node<'_, '_>) -> Option<String> {
    let title = fb2_child(section, "title")?;
    let mut label = String::new();
    for paragraph in title
        .children()
        .filter(|item| item.has_tag_name((FB2_NS, "p")))
    {
        let text: String = paragraph
            .descendants()
            .filter(Node::is_text)
            .filter(|item| {
                item.ancestors()
                    .skip(1)
                    .take(MAX_FB2_DEPTH)
                    .take_while(|parent| *parent != paragraph)
                    .all(|parent| parent.tag_name().namespace() == Some(FB2_NS))
            })
            .filter_map(|item| item.text())
            .collect();
        for word in text.split_whitespace() {
            if !label.is_empty() {
                label.push(' ');
            }
            label.push_str(word);
        }
    }
    (!label.is_empty()).then_some(label)
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Write};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use zip::{ZipWriter, write::SimpleFileOptions};

    use super::*;
    use crate::source::Source;

    #[test]
    fn markdown_has_section_anchors_footnotes_and_literal_raw_html() {
        let input = "# Title\n\nA note[^entry].\n\n## Scope {#scope}\n\n<script>ignored()</script>\n\n[^entry]: At dusk.";
        let document = super::markdown_document(input, "Field notes").unwrap();
        assert_eq!(document.toc.len(), 2);
        assert_eq!(
            document.toc[0]
                .destination
                .as_ref()
                .unwrap()
                .fragment
                .as_deref(),
            Some("reader-heading-1")
        );
        assert_eq!(
            document.toc[1]
                .destination
                .as_ref()
                .unwrap()
                .fragment
                .as_deref(),
            Some("scope")
        );
        assert!(document.xhtml.contains("At dusk."));
        assert!(document.xhtml.contains("&lt;script&gt;"));
    }
    #[test]
    fn reads_bom_marked_text_and_declared_legacy_fictionbook() {
        let mut text = vec![0xff, 0xfe];
        text.extend("Bến nước".encode_utf16().flat_map(u16::to_le_bytes));
        assert_eq!(
            super::decode_document(&text, super::Format::Text).unwrap(),
            "Bến nước"
        );
        let mut xml = br#"<?xml version="1.0" encoding="windows-1251"?>"#.to_vec();
        xml.extend_from_slice(b"<p>\xcf\xf0\xe8\xe2\xe5\xf2</p>");
        let word = super::decode_document(&xml, super::Format::Fb2).unwrap();
        assert!(word.ends_with("<p>Привет</p>"));
        assert!(super::decode_document(b"a\0b", super::Format::Text).is_err());
    }
    struct HtmlzFixture(PathBuf);

    impl HtmlzFixture {
        fn new(write: impl FnOnce(&mut ZipWriter<File>)) -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!(
                "reader-htmlz-{}-{}.HTMLZ",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let output = File::options()
                .write(true)
                .create_new(true)
                .open(&path)
                .unwrap();
            let mut zip = ZipWriter::new(output);
            write(&mut zip);
            zip.finish().unwrap();
            Self(path)
        }
    }

    impl Drop for HtmlzFixture {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }

    fn htmlz_member(zip: &mut ZipWriter<File>, name: &str, contents: &[u8]) {
        zip.start_file(
            name,
            SimpleFileOptions::default().compression_method(CompressionMethod::Deflated),
        )
        .unwrap();
        zip.write_all(contents).unwrap();
    }

    #[test]
    fn htmlz_book_preserves_local_styles_images_navigation_and_metadata() {
        let mut image = Cursor::new(Vec::new());
        image::DynamicImage::ImageLuma8(image::GrayImage::from_pixel(3, 2, image::Luma([80])))
            .write_to(&mut image, ImageFormat::Png)
            .unwrap();
        let css = b"h2 { font-style: italic; }";
        let fixture = HtmlzFixture::new(|zip| {
            htmlz_member(zip, "Field Notes.html", br##"<html><head><title>Note</title>
                <link rel="stylesheet" href="style.css" /></head><body>
                <h1 id="tide-a">Shore&nbsp;Watch</h1><h2>Night &mdash; Tide</h2>
                <p id="reader-htmlz-heading-1">Back to <a href="Field%20Notes.html#tide%2Da">the shore</a>.</p>
                <p><img src="images/shore%20view.png" /></p></body></html>"##);
            htmlz_member(zip, "metadata.opf", br#"<package xmlns:dc="http://purl.org/dc/elements/1.1/">
                <metadata><dc:title>Field Notes</dc:title><dc:creator>Linh</dc:creator></metadata></package>"#);
            zip.add_directory("images/", SimpleFileOptions::default())
                .unwrap();
            htmlz_member(zip, "images/shore view.png", image.get_ref());
            htmlz_member(zip, "style.css", css);
        });
        assert_eq!(
            crate::library::DocumentFormat::for_path(&fixture.0),
            Some(crate::library::DocumentFormat::Htmlz)
        );
        let mut book = Book::open(&fixture.0).unwrap();
        assert_eq!(book.title, "Field Notes");
        assert_eq!(book.author.as_deref(), Some("Linh"));
        assert_eq!(book.chapters.len(), 1);
        let path = book.chapters[0].href.clone();
        assert_eq!(path, "Field Notes.html");
        let xhtml = book.chapter(0).unwrap();
        assert!(xhtml.contains("Shore\u{a0}Watch"));
        assert!(xhtml.contains("Night — Tide"));
        let source = Source::parse(&xhtml).unwrap();
        assert_eq!(book.toc().len(), 2);
        assert_eq!(book.toc()[1].label, "Night — Tide");
        let generated = book.toc()[1].destination.as_ref().unwrap();
        assert_eq!(generated.path, path);
        assert_eq!(
            generated.fragment.as_deref(),
            Some("reader-htmlz-heading-2")
        );
        assert!(
            source
                .fragments
                .contains_key(generated.fragment.as_ref().unwrap())
        );
        let link = source
            .blocks
            .iter()
            .flat_map(|block| &block.links)
            .next()
            .unwrap();
        let target = Book::resolve_link(&path, &link.href).unwrap();
        assert_eq!(book.chapter_index(&target.path), Some(0));
        assert_eq!(target.fragment.as_deref(), Some("tide-a"));
        assert!(Book::resolve_link(&path, "../outside.html#tide-a").is_err());
        assert!(Book::resolve_link(&path, "https://example.invalid/#tide-a").is_err());
        let image_href = source
            .blocks
            .iter()
            .flat_map(|block| &block.images)
            .next()
            .unwrap();
        assert_eq!(
            book.resource(&Book::resolve(&path, &image_href.href).unwrap())
                .unwrap(),
            *image.get_ref()
        );
        assert_eq!(book.resource("style.css").unwrap(), css);
        assert!(book.resource("metadata.opf").is_err());
    }

    #[test]
    fn htmlz_rejects_unsafe_ambiguous_or_oversized_archives() {
        let chapter = b"<html><body><p>Only one document</p></body></html>";
        for unsafe_entry in ["../outside.png", "/absolute.png", "folder/./same.png"] {
            let fixture = HtmlzFixture::new(|zip| {
                htmlz_member(zip, "index.html", chapter);
                htmlz_member(zip, unsafe_entry, b"data");
            });
            assert!(
                Book::open(&fixture.0)
                    .err()
                    .unwrap()
                    .to_string()
                    .contains("unsafe entry")
            );
        }
        let fixture = HtmlzFixture::new(|zip| {
            htmlz_member(zip, "index.html", chapter);
            zip.add_symlink("cover.png", "../outside.png", SimpleFileOptions::default())
                .unwrap();
        });
        assert!(
            Book::open(&fixture.0)
                .err()
                .unwrap()
                .to_string()
                .contains("unsafe entry")
        );
        let fixture = HtmlzFixture::new(|zip| {
            htmlz_member(zip, "index.html", chapter);
            htmlz_member(zip, "another.xhtml", chapter);
        });
        assert!(
            Book::open(&fixture.0)
                .err()
                .unwrap()
                .to_string()
                .contains("more than one")
        );
        let fixture = HtmlzFixture::new(|zip| {
            htmlz_member(zip, "index.html", chapter);
            let image = vec![0; MAX_HTMLZ_IMAGE];
            for entry in 0..=MAX_HTMLZ_RESOURCES / MAX_HTMLZ_IMAGE {
                htmlz_member(zip, &format!("images/{entry}.png"), &image);
            }
        });
        assert!(
            Book::open(&fixture.0)
                .err()
                .unwrap()
                .to_string()
                .contains("total resource limit")
        );
    }

    #[test]
    fn text_import_preserves_lines_paragraphs_unicode_and_literals() {
        let input = "Chữ & <literal> > test\r\nNext line\r\n\r\nSecond paragraph\rEnd";
        let xhtml = text_as_xhtml(input).unwrap();
        let source = Source::parse(&xhtml).unwrap();
        assert_eq!(source.blocks.len(), 2);
        assert_eq!(source.blocks[0].text, "Chữ & <literal> > test\nNext line");
        assert_eq!(source.blocks[1].text, "Second paragraph\nEnd");
        assert!(text_as_xhtml("  \n\t ").is_err());
        assert!(text_as_xhtml("bad\0").is_err());
        assert!(text_as_xhtml(&"&".repeat(MAX_INPUT)).is_err());
    }

    #[test]
    fn html_import_preserves_content_and_annotations_without_expanding_dtds() {
        let input = "<?xml version=\"1.0\"?>\n<!doctype html>\n<html><head><title> Field notes </title></head><body><p id=\"first\">Reading <em>the shore</em>.<br /><a href=\"#first\">Back</a></p></body></html>";
        let imported = html_document(input, "Untitled").unwrap();
        let source = Source::parse(&imported.xhtml).unwrap();
        assert_eq!(imported.title, "Field notes");
        assert_eq!(source.blocks[0].text, "Reading the shore.\nBack");
        assert_eq!(source.blocks[0].links[0].href, "#first");
        assert!(source.fragments.contains_key("first"));
        assert!(html_document("<html><body><p>missing end tag</body></html>", "Untitled").is_err());
        assert!(html_document("<!DOCTYPE html SYSTEM \"file:///missing.dtd\"><html><body><p>Content</p></body></html>", "Untitled").is_err());
    }

    #[test]
    fn fictionbook_import_retains_reading_order_and_note_links() {
        let input = r##"<?xml version="1.0"?>
            <fb:FictionBook xmlns:fb="http://www.gribuser.ru/xml/fictionbook/2.0"
                xmlns:xlink="http://www.w3.org/1999/xlink" xmlns:other="urn:other">
                <fb:description><fb:title-info>
                    <fb:author><fb:first-name>Linh</fb:first-name><fb:last-name>Trần</fb:last-name></fb:author>
                    <fb:book-title>Field &amp; Shore</fb:book-title>
                </fb:title-info></fb:description>
                <fb:body><fb:section id="shore"><fb:title><fb:p>The shore</fb:p></fb:title>
                    <fb:section id="tide"><fb:title><fb:p>Watching the tide<other:p>Wrong namespace</other:p></fb:p></fb:title>
                        <fb:p>At the <fb:emphasis>shore</fb:emphasis>.<fb:a id="ref" xlink:href="#ghi%2Dchu">[1]</fb:a></fb:p>
                        <fb:p><fb:a xlink:href="https://example.invalid/">A remote reference</fb:a></fb:p>
                        <other:p>Wrong namespace</other:p>
                    </fb:section>
                </fb:section></fb:body>
                <fb:body name="notes"><fb:section id="ghi-chu">
                    <fb:p>The tide was low. <fb:a xlink:href="#ref">Return</fb:a></fb:p>
                </fb:section></fb:body>
                <fb:binary id="unused">aW1hZ2U=</fb:binary>
            </fb:FictionBook>"##;
        let book = fb2_document(input, "Untitled").unwrap();
        assert_eq!(book.title, "Field & Shore");
        assert!(book.xhtml.contains("name=\"author\" content=\"Linh Trần\""));
        assert!(book.xhtml.contains("<h2>The shore</h2>"));
        assert!(book.xhtml.contains("<h3>Watching the tide</h3>"));
        assert!(!book.xhtml.contains("Wrong namespace"));
        assert!(!book.xhtml.contains("aW1hZ2U="));
        let source = Source::parse(&book.xhtml).unwrap();
        assert_eq!(
            source
                .blocks
                .iter()
                .map(|block| block.text.as_str())
                .collect::<Vec<_>>(),
            [
                "The shore",
                "Watching the tide",
                "At the shore.[1]",
                "A remote reference",
                "Notes",
                "The tide was low. Return",
            ],
        );
        assert!(source.fragments.contains_key("shore"));
        assert!(source.fragments.contains_key("tide"));
        assert!(source.fragments.contains_key("ref"));
        assert!(source.fragments.contains_key("ghi-chu"));
        assert_eq!(book.toc.len(), 2);
        for (entry, (label, id, depth)) in book
            .toc
            .iter()
            .zip([("The shore", "shore", 0), ("Watching the tide", "tide", 1)])
        {
            let destination = entry.destination.as_ref().unwrap();
            assert_eq!(entry.label, label);
            assert_eq!(entry.depth, depth);
            assert_eq!(destination.path, CHAPTER_PATH);
            assert_eq!(destination.fragment.as_deref(), Some(id));
            assert!(source.fragments.contains_key(id));
        }
        assert_eq!(source.blocks[2].links[0].href, "#ghi%2Dchu");
        assert!(source.blocks[3].links.is_empty());
        assert_eq!(source.blocks[5].links[0].href, "#ref");
    }

    #[test]
    fn fictionbook_import_rejects_wrong_namespace_and_excessive_nesting() {
        assert!(fb2_document("<FictionBook><body/></FictionBook>", "Untitled").is_err());
        let depth = MAX_FB2_DEPTH + 1;
        let input = format!(
            "<FictionBook xmlns=\"{FB2_NS}\"><body>{}<p>text</p>{}</body></FictionBook>",
            "<section>".repeat(depth),
            "</section>".repeat(depth),
        );
        assert!(fb2_document(&input, "Untitled").is_err());
    }

    #[test]
    fn format_detection_matches_only_supported_extensions() {
        assert_eq!(Format::for_path(Path::new("Notes.TXT")), Some(Format::Text));
        for name in [
            "document.htm",
            "document.HTML",
            "document.xhtml",
            "document.xht",
        ] {
            assert_eq!(Format::for_path(Path::new(name)), Some(Format::Html));
        }
        assert_eq!(
            Format::for_path(Path::new("document.md")),
            Some(Format::Markdown)
        );
        assert_eq!(Format::for_path(Path::new("book.azw")), Some(Format::Mobi));
        assert_eq!(Format::for_path(Path::new("book.azw3")), Some(Format::Mobi));
        assert_eq!(Format::for_path(Path::new("book.FB2")), Some(Format::Fb2));
    }
}

#[cfg(test)]
mod exported_text_tests {
    use super::*;

    #[test]
    fn reads_a_text_export_with_html_breaks_and_terminal_colors() {
        let source = "<HTML><HEAD><TITLE>Example</TITLE></HEAD><BODY><h3>Notes</h3><p>One<br>\x1b[32mTwo\x1b[0m</BODY></HTML>";
        let book = text_document(source, "Export").unwrap();
        let tree = Xml::parse(&book.xhtml).unwrap();
        assert!(tree.descendants().any(|node| node.text() == Some("Two")));
        assert!(!book.xhtml.contains('\x1b'));
        assert!(!book.xhtml.contains("&lt;BODY"));
        assert!(
            text_document("Remember <em> and <p> are tags.", "Notes")
                .unwrap()
                .xhtml
                .contains("&lt;em&gt;")
        );
        assert!(text_document("An unsupported control: \x01", "Bad").is_err());
    }

    #[test]
    fn fb2_note_section_index_rejects_ambiguous_and_non_section_targets() {
        let input = r#"<FictionBook xmlns="http://www.gribuser.ru/xml/fictionbook/2.0">
            <body><section id="story"><p id="twice">Ordinary passage.</p></section></body>
            <body name="NoTeS">
                <section id="twice"><p>Duplicate location.</p></section>
                <section id="workshop"><p id="note-text">Trusted section text.</p>
                  <section id="nested"><p>Nested note entry.</p></section>
                </section>
                <section><p>Unaddressed section.</p></section>
            </body>
            <body name="COMMENTS"><section id="afterword"><p>Commentary.</p></section></body>
        </FictionBook>"#;
        let book = fb2_document(input, "Workshop").unwrap();
        assert_eq!(
            book.fb2_note_sections,
            BTreeMap::from([
                ("afterword".into(), Fb2NoteKind::Endnote),
                ("nested".into(), Fb2NoteKind::Footnote),
                ("workshop".into(), Fb2NoteKind::Footnote),
            ])
        );
        assert!(book.xhtml.contains("id=\"primitiveos-fb2-section-1\""));
        assert!(!book.xhtml.contains("NoTeS"));
        assert!(!book.xhtml.contains("COMMENTS"));
    }
}

#[cfg(test)]
mod format_alias_tests {
    use super::*;
    use std::io::Write;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use zip::{ZipWriter, write::SimpleFileOptions};

    static NEXT: AtomicUsize = AtomicUsize::new(0);
    struct Scratch(PathBuf);
    impl Scratch {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "reader-aliases-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
        fn zip(&self, filename: &str, members: &[(&str, &[u8])]) -> PathBuf {
            let path = self.0.join(filename);
            let mut archive = ZipWriter::new(File::create(&path).unwrap());
            for &(name, data) in members {
                archive
                    .start_file(
                        name,
                        SimpleFileOptions::default()
                            .compression_method(CompressionMethod::Deflated),
                    )
                    .unwrap();
                archive.write_all(data).unwrap();
            }
            archive.finish().unwrap();
            path
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn archives_route_matching_entries_and_reject_ambiguity() {
        let scratch = Scratch::new();
        for (archive_name, member, bytes, format) in [
            (
                "notes.LOG.ZIP",
                "inside.log",
                b"A plain log".as_slice(),
                Format::Text,
            ),
            (
                "notes.MD.ZIP",
                "inside.md",
                b"# A heading".as_slice(),
                Format::Markdown,
            ),
            (
                "notes.HTML.ZIP",
                "index.html",
                b"<html><body><p>HTML document</p></body></html>".as_slice(),
                Format::Htmlz,
            ),
            (
                "notes.HTM.ZIP",
                "index.htm",
                b"<html><body><p>HTML document</p></body></html>".as_slice(),
                Format::Htmlz,
            ),
        ] {
            let path = scratch.zip(archive_name, &[(member, bytes)]);
            assert_eq!(Format::for_path(&path), Some(format));
            let document = Document::open(&path, format).unwrap();
            assert!(!document.xhtml.is_empty());
            assert_eq!(document.title, "notes");
        }
        let ambiguous = scratch.zip(
            "ambiguous.md.zip",
            &[("one.md", b"one"), ("two.md", b"two")],
        );
        assert!(Document::open(&ambiguous, Format::Markdown).is_err());
        let wrong = scratch.zip("wrong.log.zip", &[("inside.txt", b"text")]);
        assert!(Document::open(&wrong, Format::Text).is_err());
        let unsafe_zip = scratch.zip("unsafe.log.zip", &[("../inside.log", b"text")]);
        assert!(Document::open(&unsafe_zip, Format::Text).is_err());
    }

    #[test]
    fn xml_alias_renders_xhtml_and_preserves_generic_markup() {
        let scratch = Scratch::new();
        let xhtml = scratch.0.join("book.XML");
        fs::write(&xhtml, b"<html><body><p>Readable passage</p></body></html>").unwrap();
        assert_eq!(Format::for_path(&xhtml), Some(Format::Html));
        assert!(
            Document::open(&xhtml, Format::Html)
                .unwrap()
                .xhtml
                .contains("Readable passage")
        );
        let generic = scratch.0.join("generic.xml");
        fs::write(&generic, b"<catalog><item>Readable value</item></catalog>").unwrap();
        assert!(
            Document::open(&generic, Format::Html)
                .unwrap()
                .xhtml
                .contains("&lt;catalog&gt;")
        );
        let fb2 = scratch.0.join("fiction.xml");
        fs::write(&fb2, br#"<FictionBook xmlns="http://www.gribuser.ru/xml/fictionbook/2.0"><body><section><p>Fiction passage</p></section></body></FictionBook>"#).unwrap();
        assert!(
            Document::open(&fb2, Format::Html)
                .unwrap()
                .xhtml
                .contains("Fiction passage")
        );
        assert_eq!(Format::for_path(Path::new("trace.LOG")), Some(Format::Text));
    }
}
