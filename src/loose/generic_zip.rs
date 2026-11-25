//! One bounded reflowable document in a generic ZIP. No extraction to disk.
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use zip::{CompressionMethod, ZipArchive};

use super::{Document, Format, ParsingOptions, Xml};
use crate::Result;

#[derive(Clone, Copy)]
enum Kind {
    Text,
    Markdown,
    Rtf,
    Fb2,
    Html,
    Xml,
}

/// The caller limits the archive and fingerprints its original bytes. The
/// central directory and selected member are read through the same handle.
pub(super) fn open(file: File, length: u64, title: &str) -> Result<(Document, Format)> {
    if length > super::MAX_HTMLZ_INPUT as u64 || file.metadata()?.len() != length {
        return Err("The ZIP exceeds its size limit or changed while opening".into());
    }
    let mut input = BufReader::new(file);
    let count = super::check_zip_directory(&mut input, length)
        .map_err(|_| "The ZIP directory is invalid or exceeds Reader's limits")?;
    if count > super::MAX_FB2_ZIP_ENTRIES {
        return Err("The ZIP has too many entries".into());
    }
    let mut archive = ZipArchive::new(input)?;
    if archive.len() != count {
        return Err("The ZIP has ambiguous or duplicate entries".into());
    }
    let mut selected = None;
    let mut unsupported_document = false;
    for index in 0..archive.len() {
        let entry = archive.by_index_raw(index)?;
        let raw = entry.name();
        let path = if entry.is_dir() {
            raw.strip_suffix('/').unwrap_or(raw)
        } else {
            raw
        };
        if path.len() > super::MAX_HTMLZ_PATH
            || !super::is_archive_path(path)
            || entry.enclosed_name().is_none()
            || entry.is_symlink()
            || entry.encrypted()
            || !matches!(
                entry.compression(),
                CompressionMethod::Stored | CompressionMethod::Deflated
            )
        {
            return Err(
                "The ZIP contains an unsafe, encrypted, linked or unsupported entry".into(),
            );
        }
        if !entry.is_file() {
            continue;
        }
        let extension = Path::new(path)
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let kind = match extension.as_str() {
            "txt" | "log" => Some(Kind::Text),
            "md" => Some(Kind::Markdown),
            "rtf" => Some(Kind::Rtf),
            "fb2" => Some(Kind::Fb2),
            "html" | "htm" | "xhtml" | "xht" => Some(Kind::Html),
            "xml" => Some(Kind::Xml),
            "pdf" | "epub" | "epub3" | "mobi" | "azw" | "azw3" | "fb3" | "docx" | "docm"
            | "odt" | "chm" | "djvu" | "djv" => {
                unsupported_document = true;
                None
            }
            _ => None,
        };
        if let Some(kind) = kind
            && selected.replace((index, path.to_owned(), kind)).is_some()
        {
            return Err("The ZIP contains more than one readable document".into());
        }
    }
    if unsupported_document {
        return Err(
            "The ZIP contains a document format not supported inside a generic archive".into(),
        );
    }
    let (index, name, kind) = selected.ok_or("The ZIP has no supported readable document")?;
    match kind {
        Kind::Html => Ok((
            super::read_htmlz_archive(archive, count, title)?,
            Format::Htmlz,
        )),
        Kind::Markdown => Ok((
            super::read_markdown_archive(archive, index, name, title)?,
            Format::Markdown,
        )),
        Kind::Text | Kind::Rtf | Kind::Fb2 | Kind::Xml => {
            let limit = match kind {
                Kind::Text => super::MAX_INPUT,
                Kind::Rtf => crate::rtf::MAX_INPUT,
                Kind::Fb2 => super::MAX_FB2_INPUT,
                Kind::Xml => super::MAX_INPUT,
                _ => unreachable!(),
            };
            let bytes = super::read_htmlz_member(&mut archive, index, limit)?;
            match kind {
                Kind::Text => {
                    let text = super::decode_document(&bytes, Format::Text)?;
                    if text.len() > super::MAX_INPUT {
                        return Err("The decoded ZIP text exceeds the single-section limit".into());
                    }
                    Ok((
                        super::text_document_limited(
                            text.trim_start_matches('\u{feff}'),
                            title,
                            false,
                        )?,
                        Format::Text,
                    ))
                }
                Kind::Rtf => Ok((super::rtf_document(&bytes, title)?, Format::Rtf)),
                Kind::Fb2 => {
                    let text = super::decode_document(&bytes, Format::Fb2)?;
                    Ok((
                        super::fb2_document(text.trim_start_matches('\u{feff}'), title)?,
                        Format::Fb2,
                    ))
                }
                Kind::Xml => {
                    let text = super::decode_document(&bytes, Format::Html)?;
                    let text = text.trim_start_matches('\u{feff}');
                    if text.len() > super::MAX_INPUT {
                        return Err("The decoded ZIP XML exceeds the single-section limit".into());
                    }
                    let root = Xml::parse_with_options(
                        text,
                        ParsingOptions {
                            nodes_limit: 80_000,
                            ..ParsingOptions::default()
                        },
                    )
                    .ok()
                    .map(|xml| xml.root_element().tag_name().name().to_owned());
                    match root.as_deref() {
                        Some("FictionBook") => Ok((super::fb2_document(text, title)?, Format::Fb2)),
                        Some("html") => Ok((super::html_document(text, title)?, Format::Html)),
                        _ => Ok((super::text_document(text, title)?, Format::Html)),
                    }
                }
                _ => unreachable!(),
            }
        }
    }
}
