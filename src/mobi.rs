//! Bounded import of legacy MOBI and PalmDOC into a self-contained XHTML chapter.
//!
//! Palm's File Format Specification describes the database record table.
//! Library of Congress FDD 472 records the provenance of the MOBI container.
//! MobileRead's MOBI and PalmDOC format notes document the text fields and LZ tokens.
//! https://jichu4n.github.io/palm-pdb/assets/Palm%20File%20Format%20Specification.pdf
//! https://www.loc.gov/preservation/digital/formats/fdd/fdd000472.shtml
//! https://github.com/bfabiszewski/libmobi/blob/public/src/read.c
//! https://wiki.mobileread.com/wiki/MOBI
//! https://wiki.mobileread.com/wiki/PalmDOC

use html5ever::tokenizer::states::RawKind;
use html5ever::tokenizer::{
    BufferQueue, Tag, TagKind, Token, TokenSink, TokenSinkResult, Tokenizer,
};
pub(crate) mod cover;
mod ereader_pml;
mod huff;
pub(crate) use huff::COMPRESSION as HUFF_CDIC;
mod markup;
mod presentation;
pub(crate) use markup::{Addition, apply as apply_additions};
pub(crate) use presentation::Presentation;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// Read only the Palm database header and the encryption field. A large protected
/// Kindle book can be identified without loading its contents into Reader.
pub(crate) fn protected_kindle_header(path: &std::path::Path) -> std::io::Result<bool> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(path)?;
    let mut header = [0; 86];
    file.read_exact(&mut header)?;
    if &header[60..68] != b"BOOKMOBI" {
        return Ok(false);
    }
    let count = u16::from_be_bytes([header[76], header[77]]);
    let first = u32::from_be_bytes(header[78..82].try_into().unwrap()) as u64;
    if count == 0 || first < 78 + 8 * u64::from(count) || first + 20 > file.metadata()?.len() {
        return Ok(false);
    }
    file.seek(SeekFrom::Start(first + 12))?;
    let mut flags = [0; 8];
    file.read_exact(&mut flags)?;
    Ok(&flags[4..8] == b"MOBI" && u16::from_be_bytes([flags[0], flags[1]]) != 0)
}

pub const MAX_MOBI_BYTES: usize = 32 * 1024 * 1024;
pub(crate) const MAX_TEXT_BYTES: usize = 12 * 1024 * 1024;
const MAX_XHTML_BYTES: usize = 16 * 1024 * 1024;
const MAX_RECORD_BYTES: usize = 32 * 1024;
const MAX_PDB_RECORDS: usize = 8192;
const MAX_EXTH_RECORDS: usize = 512;
const MAX_METADATA_BYTES: usize = 4096;
const MAX_HTML_DEPTH: usize = 48;
const PDB_HEADER_BYTES: usize = 78;
const RECORD_ENTRY_BYTES: usize = 8;
const DOC_HEADER_BYTES: usize = 16;

const XHTML_START: &str = "<html xmlns=\"http://www.w3.org/1999/xhtml\"><head></head><body>";
const XHTML_END: &str = "</body></html>";

#[derive(Debug)]
pub struct MobiDocument {
    pub(crate) metadata: crate::publication_metadata::Metadata,
    pub language: Option<parley::Language>,
    pub title: Option<String>,
    pub author: Option<String>,
    pub xhtml: String,
    pub(crate) additions: Vec<Addition>,
    pub(crate) presentation: Presentation,
    pub(crate) contents: crate::kf8::legacy_contents::Contents,
    pub(crate) shadow: Option<Shadow>,
    pub resources: BTreeMap<String, Vec<u8>>,
}

#[derive(Debug)]
pub(crate) struct Shadow {
    pub additions: Vec<Addition>,
    pub contents: crate::kf8::legacy_contents::Contents,
    pub ids: BTreeSet<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MobiError {
    UnsupportedContainer,
    Protected,
    Kf8,
    UnsupportedCompression(u16),
    UnsupportedEncoding(u32),
    InvalidHeader,
    InvalidHuffman,
    InvalidText,
    TooLarge,
    TextTooLarge,
}

impl fmt::Display for MobiError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedContainer => {
                formatter.write_str("This is not a supported MOBI or PalmDOC database.")
            }
            Self::Protected => {
                formatter.write_str("This Kindle book is encrypted. Open it in the Kindle Library.")
            }
            Self::Kf8 => formatter.write_str(
                "This Kindle file uses a KF8 variant or feature Reader does not support.",
            ),
            Self::InvalidHuffman => formatter.write_str(
                "The MOBI HUFF/CDIC dictionary is invalid or exceeds its decoding limits.",
            ),
            Self::UnsupportedCompression(value) => {
                write!(formatter, "Unsupported MOBI compression type {value}.")
            }
            Self::UnsupportedEncoding(value) => {
                write!(formatter, "Unsupported MOBI character encoding {value}.")
            }
            Self::InvalidHeader => formatter
                .write_str("This MOBI or PalmDOC database has an invalid record table or header."),
            Self::InvalidText => formatter
                .write_str("Reader could not recover the text of this MOBI or PalmDOC document."),
            Self::TooLarge => {
                formatter.write_str("This MOBI or PalmDOC document exceeds the 32 MiB file limit.")
            }
            Self::TextTooLarge => formatter
                .write_str("This MOBI or PalmDOC document exceeds Reader's text layout limits."),
        }
    }
}

impl std::error::Error for MobiError {}

#[derive(Clone, Copy)]
enum Encoding {
    Utf8,
    Cp1252,
    CodePage(&'static encoding_rs::Encoding),
    Auto,
}

struct MobiHeader {
    encoding: Encoding,
    extra_data: u16,
    is_plain_text: bool,
    labels: Labels,
}

#[derive(Default)]
pub(crate) struct Labels {
    pub(crate) metadata: crate::publication_metadata::Metadata,
    pub title: Option<String>,
    pub author: Option<String>,
    pub language: Option<parley::Language>,
}

/// Decode the first, legacy text section of a database. All returned content is local.
pub fn decode(bytes: &[u8]) -> Result<MobiDocument, MobiError> {
    let mut document = decode_legacy(bytes)?;
    if !document.contents.insertions.is_empty() {
        document.additions.append(&mut document.contents.insertions);
        document.additions.sort_by_key(|edit| edit.at);
    }
    if !document.additions.is_empty() || !document.presentation.is_empty() {
        let edits = document
            .presentation
            .edits(0..document.xhtml.len(), &document.additions);
        document.xhtml = markup::apply(&document.xhtml, &edits, 0, 0)?;
        document.additions.clear();
    }
    Ok(document)
}

/// Preserve the previous text-only markup for chapter splitting and saved anchors.
pub(crate) fn decode_legacy(bytes: &[u8]) -> Result<MobiDocument, MobiError> {
    if bytes.len() > MAX_MOBI_BYTES {
        return Err(MobiError::TooLarge);
    }
    if bytes.get(60..68) == Some(b"PNRdPPrs") {
        return decode_plain_ereader(bytes);
    }
    let is_mobi = match bytes.get(60..68) {
        Some(b"BOOKMOBI") => true,
        Some(b"TEXtREAd") => false,
        _ => return Err(MobiError::UnsupportedContainer),
    };
    let records = record_offsets(bytes)?;
    let header = &bytes[records[0]..records[1]];
    if header.len() < DOC_HEADER_BYTES {
        return Err(MobiError::InvalidHeader);
    }
    if is_mobi && u16_at(header, 12)? != 0 {
        return Err(MobiError::Protected);
    }
    let mobi = is_mobi.then(|| mobi_header(header)).transpose()?;
    let extra_data = mobi.as_ref().map_or(0, |mobi| mobi.extra_data);
    let text = decode_records(
        bytes,
        &records,
        header,
        extra_data,
        u32_at(header, 4)? as usize,
    )?;
    let encoding = mobi.as_ref().map_or(Encoding::Auto, |mobi| mobi.encoding);
    let text = decode_text(text, encoding)?;
    // A damaged optional navigation index must not hide the publication's text.
    let mut contents =
        crate::kf8::legacy_contents::Contents::read(bytes, &records, header).unwrap_or_default();
    let mut shadow = None;
    let (xhtml, additions, resources, presentation) =
        if mobi.as_ref().is_none_or(|mobi| mobi.is_plain_text) {
            (
                plain_xhtml(text.trim_start_matches('\u{feff}'))?,
                Vec::new(),
                BTreeMap::new(),
                Presentation::default(),
            )
        } else {
            let first = u32_at(header, 108).ok().map(|n| n as usize).filter(|n| {
                *n > usize::from(u16_at(header, 8).unwrap_or(u16::MAX)) && *n < records.len() - 1
            });
            let mut shadow_contents = contents.clone();
            let (xhtml, mut features, has_text) =
                normalize_contents_html(&text, Some(encoding), &contents, false)?;
            let presentation = std::mem::take(&mut features.presentation);
            let (additions, resources, _) =
                features.finish(bytes, &records, first, &mut contents)?;
            // Shadow markup never becomes the publication. It is used only to
            // project new links onto an identical canonical Source.
            if xhtml.len() <= 512 * 1024
                && !markup::target_text(&text, encoding, &shadow_contents).is_empty()
                && let Ok((shadow_xhtml, shadow_features, _)) =
                    normalize_contents_html(&text, Some(encoding), &shadow_contents, true)
                && shadow_xhtml == xhtml
                && let Ok((shadow_additions, _, ids)) =
                    shadow_features.finish(bytes, &records, first, &mut shadow_contents)
                && !ids.is_empty()
            {
                shadow = Some(Shadow {
                    additions: shadow_additions,
                    contents: shadow_contents,
                    ids,
                });
            }
            if !has_text && resources.is_empty() {
                return Err(MobiError::InvalidText);
            }
            (xhtml, additions, resources, presentation)
        };

    let labels = mobi.map(|header| header.labels).unwrap_or_default();
    Ok(MobiDocument {
        metadata: labels.metadata,
        title: labels.title.or_else(|| read_label(&bytes[..32], encoding)),
        author: labels.author,
        language: labels.language,
        xhtml,
        additions,
        presentation,
        contents,
        shadow,
        resources,
    })
}

/// Strict first subset of Palm eReader PDB: unencrypted PalmDOC-compressed
/// plain text or the explicitly supported PML subset. Unsupported commands
/// and auxiliary content must not be silently displayed as ordinary text.
fn decode_plain_ereader(bytes: &[u8]) -> Result<MobiDocument, MobiError> {
    let records = record_offsets(bytes)?;
    let header = bytes
        .get(records[0]..records[1])
        .ok_or(MobiError::InvalidHeader)?;
    if header.len() < 132 || u16_at(header, 6)? != 0x6240 {
        return Err(MobiError::InvalidHeader);
    }
    let compression = u16_at(header, 0)?;
    if matches!(compression, 260 | 272) {
        return Err(MobiError::Protected);
    }
    if compression != 2 {
        return Err(MobiError::UnsupportedCompression(compression));
    }
    // Do not silently lose linked or auxiliary reading content.
    if [20, 22, 28, 30]
        .into_iter()
        .any(|at| u16_at(header, at).ok() != Some(0))
    {
        return Err(MobiError::UnsupportedContainer);
    }
    let end = usize::from(u16_at(header, 12)?);
    if end <= 1 || end >= records.len() {
        return Err(MobiError::InvalidHeader);
    }
    let mut raw = Vec::new();
    for pair in records[1..=end].windows(2) {
        let payload = bytes
            .get(pair[0]..pair[1])
            .ok_or(MobiError::InvalidHeader)?;
        let remaining = MAX_TEXT_BYTES.saturating_sub(raw.len());
        if remaining == 0 {
            return Err(MobiError::TextTooLarge);
        }
        decode_palmdoc_record(payload, remaining.min(MAX_RECORD_BYTES), &mut raw)?;
    }
    if raw.is_empty()
        || raw
            .iter()
            .any(|&byte| byte < 32 && !matches!(byte, b'\t' | b'\n' | b'\r'))
    {
        return Err(MobiError::InvalidText);
    }
    let text = decode_text(raw, Encoding::Cp1252)?;
    if text
        .chars()
        .any(|ch| ch.is_control() && !ch.is_whitespace())
    {
        return Err(MobiError::InvalidText);
    }
    // Preserve the original output byte-for-byte for already readable plain
    // documents. Only previously rejected backslash-bearing text uses PML.
    let (xhtml, contents) = if text.contains('\\') {
        match ereader_pml::xhtml(&text) {
            Ok(xhtml) => (xhtml, Default::default()),
            Err(MobiError::InvalidText) => {
                let (xhtml, rows) = ereader_pml::with_navigation(&text)?;
                (xhtml, crate::kf8::legacy_contents::Contents::from_pml(rows))
            }
            Err(error) => return Err(error),
        }
    } else {
        (plain_xhtml(&text)?, Default::default())
    };
    Ok(MobiDocument {
        metadata: Default::default(),
        language: None,
        title: read_label(&bytes[..32], Encoding::Cp1252),
        author: None,
        xhtml,
        additions: Vec::new(),
        presentation: Presentation::default(),
        contents,
        shadow: None,
        resources: BTreeMap::new(),
    })
}

pub(crate) fn decode_records(
    bytes: &[u8],
    records: &[usize],
    header: &[u8],
    extra_data: u16,
    text_length: usize,
) -> Result<Vec<u8>, MobiError> {
    let compression = u16_at(header, 0)?;
    if !matches!(compression, 1 | 2 | huff::COMPRESSION) {
        return Err(MobiError::UnsupportedCompression(compression));
    }
    let text_records = usize::from(u16_at(header, 8)?);
    let record_size = usize::from(u16_at(header, 10)?);
    if text_length > MAX_TEXT_BYTES || record_size > MAX_RECORD_BYTES {
        return Err(MobiError::TextTooLarge);
    }
    if text_length == 0
        || text_records == 0
        || record_size == 0
        || text_records + 1 >= records.len()
    {
        return Err(MobiError::InvalidHeader);
    }

    let dictionary = (compression == huff::COMPRESSION)
        .then(|| huff::Dictionary::read(bytes, records, header, text_records))
        .transpose()?;
    let mut text = Vec::with_capacity(text_length);
    let mut huff_work = (text_length + record_size) * 16 + 4096;
    for pair in records[1..=text_records + 1].windows(2) {
        let payload = without_trailers(&bytes[pair[0]..pair[1]], extra_data)?;
        let limit = record_size.min((text_length + record_size).saturating_sub(text.len()));
        match compression {
            1 if payload.len() <= limit => text.extend_from_slice(payload),
            1 => return Err(MobiError::InvalidText),
            2 => decode_palmdoc_record(payload, limit, &mut text)?,
            _ => dictionary
                .as_ref()
                .ok_or(MobiError::InvalidHuffman)?
                .expand(payload, limit, &mut text, &mut huff_work)?,
        }
    }
    if text.len() < text_length || text[text_length..].iter().any(|&byte| byte != 0) {
        return Err(MobiError::InvalidText);
    }
    text.truncate(text_length);
    Ok(text)
}

pub(crate) fn record_offsets(bytes: &[u8]) -> Result<Vec<usize>, MobiError> {
    let attributes = u16_at(bytes, 32)?;
    let next_list = u32_at(bytes, 72)?;
    if attributes & 1 != 0 || next_list != 0 {
        return Err(MobiError::UnsupportedContainer);
    }
    let count = usize::from(u16_at(bytes, 76)?);
    if !(2..=MAX_PDB_RECORDS).contains(&count) {
        return Err(MobiError::InvalidHeader);
    }
    let end = PDB_HEADER_BYTES + count * RECORD_ENTRY_BYTES;
    let entries = bytes
        .get(PDB_HEADER_BYTES..end)
        .ok_or(MobiError::InvalidHeader)?;
    let mut offsets = Vec::with_capacity(count + 1);
    let mut previous = end;
    for entry in entries.as_chunks::<RECORD_ENTRY_BYTES>().0 {
        let offset = u32_at(entry, 0)? as usize;
        if offset < previous || offset > bytes.len() {
            return Err(MobiError::InvalidHeader);
        }
        offsets.push(offset);
        previous = offset;
    }
    offsets.push(bytes.len());
    Ok(offsets)
}

fn mobi_header(record: &[u8]) -> Result<MobiHeader, MobiError> {
    let mobi_len = u32_at(record, 20)? as usize;
    let end = DOC_HEADER_BYTES
        .checked_add(mobi_len)
        .ok_or(MobiError::InvalidHeader)?;
    let header = record.get(..end).ok_or(MobiError::InvalidHeader)?;
    if header.get(16..20) != Some(b"MOBI") || mobi_len < 24 {
        return Err(MobiError::InvalidHeader);
    }
    let kind = u32_at(header, 24)?;
    let version = u32_at(header, 36)?;
    if u32_at(header, 172).is_ok_and(|count| !matches!(count, 0 | u32::MAX)) {
        return Err(MobiError::Protected);
    }
    if version >= 8 || kind == 248 {
        return Err(MobiError::Kf8);
    }
    let encoding = match u32_at(header, 28)? {
        1252 => Encoding::Cp1252,
        65001 => Encoding::Utf8,
        other => Encoding::CodePage(
            extended_code_page(other).ok_or(MobiError::UnsupportedEncoding(other))?,
        ),
    };
    Ok(MobiHeader {
        encoding,
        extra_data: if version >= 5 {
            u16_at(header, 242).unwrap_or(0)
        } else {
            0
        },
        is_plain_text: matches!(kind, 3 | 517),
        labels: mobi_labels(record, header, encoding),
    })
}

fn mobi_labels(record: &[u8], header: &[u8], encoding: Encoding) -> Labels {
    let end = header.len();
    let full_name = u32_at(header, 84)
        .ok()
        .zip(u32_at(header, 88).ok())
        .and_then(|(offset, length)| {
            let offset = offset as usize;
            let length = length as usize;
            if length > MAX_METADATA_BYTES || offset < end {
                return None;
            }
            read_label(record.get(offset..offset.checked_add(length)?)?, encoding)
        });
    let mut labels = exth_labels(&record[end..], encoding);
    labels.title = labels.title.or(full_name);
    labels
}

pub(crate) fn kf8_labels(pdb: &[u8], record: &[u8], header: &[u8]) -> Labels {
    let mut labels = mobi_labels(record, header, Encoding::Utf8);
    labels.title = labels
        .title
        .or_else(|| read_label(&pdb[..32], Encoding::Utf8));
    labels
}

fn exth_labels(bytes: &[u8], encoding: Encoding) -> Labels {
    let mut labels = Labels::default();
    if bytes.get(..4) != Some(b"EXTH") {
        return labels;
    }
    let Some(end) = u32_at(bytes, 4)
        .ok()
        .filter(|size| (12..=bytes.len() as u32).contains(size))
    else {
        return labels;
    };
    let count = u32_at(bytes, 8).unwrap_or(0) as usize;
    let mut bytes = &bytes[12..end as usize];
    for _ in 0..count.min(MAX_EXTH_RECORDS) {
        let record = u32_at(bytes, 0).ok().zip(u32_at(bytes, 4).ok());
        let Some((kind, length)) =
            record.filter(|&(_, length)| (8..=bytes.len() as u32).contains(&length))
        else {
            break;
        };
        let length = length as usize;
        let value = &bytes[8..length];
        match kind {
            100 if labels.author.is_none() => labels.author = read_label(value, encoding),
            103 if labels.metadata.description.is_none() && value.len() <= 16 * 1024 => {
                labels.metadata.description = decode_text(value.to_vec(), encoding).ok();
            }
            105 if value.len() <= MAX_METADATA_BYTES && labels.metadata.keywords.len() < 128 => {
                if let Ok(value) = decode_text(value.to_vec(), encoding) {
                    labels.metadata.keywords.push(value);
                }
            }
            503 if labels.title.is_none() => labels.title = read_label(value, encoding),
            524 if labels.language.is_none()
                && value.len() <= crate::source::MAX_LANGUAGE_BYTES =>
            {
                labels.metadata.language = std::str::from_utf8(value).ok().map(str::to_owned);
                labels.language = std::str::from_utf8(value)
                    .ok()
                    .and_then(|tag| parley::Language::parse(tag.trim()).ok());
            }
            _ => {}
        }
        bytes = &bytes[length..];
    }
    labels
}

fn without_trailers(mut bytes: &[u8], flags: u16) -> Result<&[u8], MobiError> {
    // Higher flag bits describe end-of-record entries in descending bit order.
    // The low bit accounts for UTF-8 bytes repeated at the next record boundary.
    for _ in 0..(flags >> 1).count_ones() {
        let mut entry_len = None;
        let mut value = 0usize;
        for (index, &byte) in bytes.iter().rev().take(4).enumerate() {
            value |= usize::from(byte & 127) << (7 * index);
            if byte & 128 != 0 {
                if value < index + 1 || value > bytes.len() {
                    return Err(MobiError::InvalidText);
                }
                entry_len = Some(value);
                break;
            }
        }
        let remove = entry_len.ok_or(MobiError::InvalidText)?;
        bytes = &bytes[..bytes.len() - remove];
    }
    if flags & 1 != 0 {
        let remove = usize::from(*bytes.last().ok_or(MobiError::InvalidText)? & 3) + 1;
        bytes = bytes
            .get(
                ..bytes
                    .len()
                    .checked_sub(remove)
                    .ok_or(MobiError::InvalidText)?,
            )
            .ok_or(MobiError::InvalidText)?;
    }
    Ok(bytes)
}

fn decode_palmdoc_record(
    mut input: &[u8],
    limit: usize,
    output: &mut Vec<u8>,
) -> Result<(), MobiError> {
    let start = output.len();
    while let Some((&token, rest)) = input.split_first() {
        input = rest;
        let available = limit - (output.len() - start);
        match token {
            1..=8 => {
                let count = usize::from(token);
                if count > available || count > input.len() {
                    return Err(MobiError::InvalidText);
                }
                output.extend_from_slice(&input[..count]);
                input = &input[count..];
            }
            0x80..=0xbf => {
                let (&next, rest) = input.split_first().ok_or(MobiError::InvalidText)?;
                input = rest;
                let pair = (u16::from_be_bytes([token, next]) & 0x3fff) as usize;
                let distance = pair >> 3;
                let count = (pair & 7) + 3;
                if count > available || distance == 0 || distance > output.len() - start {
                    return Err(MobiError::InvalidText);
                }
                for _ in 0..count {
                    output.push(output[output.len() - distance]);
                }
            }
            0xc0..=0xff if available >= 2 => output.extend_from_slice(&[b' ', token ^ 0x80]),
            0 | 9..=0x7f if available > 0 => output.push(token),
            _ => return Err(MobiError::InvalidText),
        }
    }
    Ok(())
}

// These WHATWG decoders are all single-byte. Existing 1252 remains on its
// original path so its legacy mappings and saved source positions do not move.
pub(crate) fn extended_code_page(code: u32) -> Option<&'static encoding_rs::Encoding> {
    Some(match code {
        874 => encoding_rs::WINDOWS_874,
        1250 => encoding_rs::WINDOWS_1250,
        1251 => encoding_rs::WINDOWS_1251,
        1253 => encoding_rs::WINDOWS_1253,
        1254 => encoding_rs::WINDOWS_1254,
        1255 => encoding_rs::WINDOWS_1255,
        1256 => encoding_rs::WINDOWS_1256,
        1257 => encoding_rs::WINDOWS_1257,
        1258 => encoding_rs::WINDOWS_1258,
        _ => return None,
    })
}

fn decode_text(bytes: Vec<u8>, encoding: Encoding) -> Result<String, MobiError> {
    match encoding {
        Encoding::Utf8 => String::from_utf8(bytes).map_err(|_| MobiError::InvalidText),
        Encoding::Cp1252 => Ok(bytes.into_iter().map(cp1252).collect()),
        Encoding::CodePage(code_page) => {
            let text = code_page
                .decode_without_bom_handling_and_without_replacement(&bytes)
                .ok_or(MobiError::InvalidText)?;
            // filepos values count original bytes, so retain only a decoding
            // with precisely one Unicode scalar for every source byte.
            if text.chars().count() != bytes.len() {
                return Err(MobiError::InvalidText);
            }
            Ok(text.into_owned())
        }
        Encoding::Auto => match String::from_utf8(bytes) {
            Ok(text) => Ok(text),
            Err(error) => Ok(error.into_bytes().into_iter().map(cp1252).collect()),
        },
    }
}

pub(crate) fn navigation_label(bytes: &[u8], encoding: usize) -> Result<String, MobiError> {
    let encoding = match encoding {
        65001 => Encoding::Utf8,
        1252 => Encoding::Cp1252,
        v if v == u32::MAX as usize => Encoding::Cp1252,
        v => Encoding::CodePage(
            u32::try_from(v)
                .ok()
                .and_then(extended_code_page)
                .ok_or(MobiError::InvalidHeader)?,
        ),
    };
    let text = decode_text(bytes.to_vec(), encoding)?;
    if text.chars().any(|c| c.is_control() && !c.is_whitespace()) {
        return Err(MobiError::InvalidText);
    }
    Ok(text.split_whitespace().collect::<Vec<_>>().join(" "))
}

fn read_label(bytes: &[u8], encoding: Encoding) -> Option<String> {
    if bytes.len() > MAX_METADATA_BYTES {
        return None;
    }
    let bytes = bytes.split(|&byte| byte == 0).next()?;
    let text = decode_text(bytes.to_vec(), encoding).ok()?;
    let text: String = text
        .trim()
        .chars()
        .filter(|ch| !ch.is_control())
        .take(256)
        .collect();
    (!text.is_empty()).then_some(text)
}

fn cp1252(byte: u8) -> char {
    const C1: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8d}', 'Ž',
        '\u{8f}', '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9d}',
        'ž', 'Ÿ',
    ];
    match byte {
        0x80..=0x9f => C1[usize::from(byte - 0x80)],
        _ => char::from(byte),
    }
}

pub(crate) fn u16_at(bytes: &[u8], offset: usize) -> Result<u16, MobiError> {
    Ok(u16::from_be_bytes(
        *bytes
            .get(offset..)
            .and_then(<[u8]>::first_chunk)
            .ok_or(MobiError::InvalidHeader)?,
    ))
}

pub(crate) fn u32_at(bytes: &[u8], offset: usize) -> Result<u32, MobiError> {
    Ok(u32::from_be_bytes(
        *bytes
            .get(offset..)
            .and_then(<[u8]>::first_chunk)
            .ok_or(MobiError::InvalidHeader)?,
    ))
}

fn plain_xhtml(text: &str) -> Result<String, MobiError> {
    if !text
        .chars()
        .any(|ch| !ch.is_whitespace() && !ch.is_control())
    {
        return Err(MobiError::InvalidText);
    }
    let mut output = String::from(XHTML_START);
    let mut paragraph = false;
    for line in text.replace("\r\n", "\n").split(['\n', '\r']) {
        if line.trim().is_empty() {
            if paragraph {
                output.push_str("</p>");
                paragraph = false;
            }
        } else {
            output.push_str(if paragraph { "<br />" } else { "<p>" });
            paragraph = true;
            escape_xml(&mut output, line)?;
        }
    }
    if paragraph {
        output.push_str("</p>");
    }
    finish_xhtml(output)
}

pub(crate) fn html_xhtml(text: &str) -> Result<String, MobiError> {
    Ok(normalize_html(text, None)?.0)
}
fn normalize_html(
    text: &str,
    encoding: Option<Encoding>,
) -> Result<(String, markup::Features, bool), MobiError> {
    normalize_contents_html(
        text,
        encoding,
        &crate::kf8::legacy_contents::Contents::default(),
        false,
    )
}
fn normalize_contents_html(
    text: &str,
    encoding: Option<Encoding>,
    contents: &crate::kf8::legacy_contents::Contents,
    text_targets: bool,
) -> Result<(String, markup::Features, bool), MobiError> {
    let input = BufferQueue::default();
    let tokenizer = Tokenizer::new(
        Normalizer(RefCell::new(Markup::default())),
        Default::default(),
    );
    let mut cursor = text.len() - text.trim_start_matches('\u{feff}').len();
    tokenizer.sink.0.borrow_mut().features = encoding.map(|_| markup::Features::default());
    let mut targets = Vec::new();
    if let Some(encoding) = encoding {
        targets.extend(
            markup::target_tags(text, encoding, contents)
                .into_iter()
                .map(|(raw, at, end, tag)| (raw, at, end, Some(tag))),
        );
        if text_targets {
            targets.extend(
                markup::target_text(text, encoding, contents)
                    .into_iter()
                    .map(|(raw, at)| (raw, at, at, None)),
            );
        }
        targets.sort_by_key(|(_, at, _, _)| *at);
    }
    for (raw, at, end, tag) in targets {
        if at < cursor {
            continue;
        }
        input.push_back(text[cursor..at].into());
        let _ = tokenizer.feed(&input);
        if let Some(tag) = tag {
            tokenizer.sink.destination(Some((raw, tag.to_owned())));
            input.push_back(text[at..end].into());
            let _ = tokenizer.feed(&input);
            tokenizer.sink.destination(None);
        } else if input.is_empty() {
            tokenizer.sink.text_destination(raw);
        }
        cursor = end;
    }
    input.push_back(text[cursor..].into());
    let _ = tokenizer.feed(&input);
    tokenizer.end();
    let mut markup = tokenizer.sink.0.into_inner();
    if let Some(error) = markup.error {
        return Err(error);
    }
    if !markup.has_text
        && !markup
            .features
            .as_ref()
            .is_some_and(markup::Features::has_images)
    {
        return Err(MobiError::InvalidText);
    }
    markup.close_to(0);
    if !markup.has_text && markup.output.len() == XHTML_START.len() {
        markup.output.push_str(XHTML_END);
        return Ok((markup.output, markup.features.unwrap_or_default(), false));
    }
    Ok((
        finish_xhtml(markup.output)?,
        markup.features.unwrap_or_default(),
        markup.has_text,
    ))
}

fn finish_xhtml(mut output: String) -> Result<String, MobiError> {
    if output.len() == XHTML_START.len() {
        return Err(MobiError::InvalidText);
    }
    output.push_str(XHTML_END);
    if output.len() > MAX_XHTML_BYTES {
        return Err(MobiError::TextTooLarge);
    }
    Ok(output)
}

fn escape_xml(output: &mut String, text: &str) -> Result<(), MobiError> {
    for ch in text.chars() {
        match ch {
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '&' => output.push_str("&amp;"),
            '"' => output.push_str("&quot;"),
            '\t' | '\n' | '\r' => output.push(ch),
            ch if ch.is_control() || matches!(ch, '\u{fffe}' | '\u{ffff}') => {}
            ch => output.push(ch),
        }
        if output.len() > MAX_XHTML_BYTES {
            return Err(MobiError::TextTooLarge);
        }
    }
    Ok(())
}

struct Markup {
    output: String,
    open: Vec<&'static str>,
    ignore: Option<&'static str>,
    error: Option<MobiError>,
    has_text: bool,
    features: Option<markup::Features>,
}

impl Default for Markup {
    fn default() -> Self {
        Self {
            output: XHTML_START.to_owned(),
            open: Vec::new(),
            ignore: None,
            error: None,
            has_text: false,
            features: None,
        }
    }
}

struct Normalizer(RefCell<Markup>);

impl Normalizer {
    fn destination(&self, label: Option<(usize, String)>) {
        if let Some(features) = &mut self.0.borrow_mut().features {
            features.next = label;
        }
    }

    fn text_destination(&self, source: usize) {
        let mut markup = self.0.borrow_mut();
        if markup.ignore.is_none() && markup.error.is_none() {
            let at = markup.output.len();
            if let Some(features) = &mut markup.features {
                features.text_destination(source, at);
            }
        }
    }
}

impl TokenSink for Normalizer {
    type Handle = ();

    fn process_token(&self, token: Token, _: u64) -> TokenSinkResult<()> {
        let mut markup = self.0.borrow_mut();
        if markup.error.is_some() {
            return TokenSinkResult::Continue;
        }
        match token {
            Token::TagToken(tag) => return markup.tag(tag),
            Token::CharacterTokens(text) if markup.ignore.is_none() => {
                markup.has_text |= text
                    .chars()
                    .any(|ch| !ch.is_whitespace() && !ch.is_control());
                let start = markup.output.len();
                let written = escape_xml(&mut markup.output, &text);
                let end = markup.output.len();
                markup.error = written
                    .and_then(|_| {
                        if let Some(features) = &mut markup.features {
                            features.presentation.text(start..end)?;
                        }
                        Ok(())
                    })
                    .err();
            }
            _ => {}
        }
        TokenSinkResult::Continue
    }
}

impl Markup {
    fn tag(&mut self, tag: Tag) -> TokenSinkResult<()> {
        let input_name: &str = tag.name.as_ref();
        if let Some(ignored) = self.ignore {
            if tag.kind == TagKind::EndTag && input_name == ignored {
                self.ignore = None;
            }
            return TokenSinkResult::Continue;
        }
        if tag.kind == TagKind::StartTag {
            let raw = match input_name {
                "script" => Some(("script", RawKind::ScriptData)),
                "style" => Some(("style", RawKind::Rawtext)),
                "title" => Some(("title", RawKind::Rcdata)),
                "iframe" => Some(("iframe", RawKind::Rawtext)),
                _ => None,
            };
            if let Some((name, state)) = raw {
                self.ignore = Some(name);
                return TokenSinkResult::RawData(state);
            }
        }
        if input_name == "font"
            && let Some(features) = &mut self.features
        {
            self.error = features.presentation.font(&tag).err();
            return TokenSinkResult::Continue;
        }
        if input_name == "img" && tag.kind == TagKind::StartTag {
            if let Some(features) = &mut self.features {
                self.error = features.image(&tag, self.output.len()).err();
            }
            return TokenSinkResult::Continue;
        }
        let Some(name) = safe_tag(input_name) else {
            return TokenSinkResult::Continue;
        };
        if tag.kind == TagKind::EndTag {
            self.close(name);
        } else {
            self.error = self.start(name, &tag).err();
        }
        TokenSinkResult::Continue
    }

    fn start(&mut self, name: &'static str, tag: &Tag) -> Result<(), MobiError> {
        if matches!(
            name,
            "p" | "div"
                | "blockquote"
                | "h1"
                | "h2"
                | "h3"
                | "h4"
                | "h5"
                | "h6"
                | "ul"
                | "ol"
                | "dl"
                | "table"
                | "hr"
                | "pre"
        ) {
            self.close("p");
        }
        if matches!(name, "li" | "td" | "th" | "tr" | "dt" | "dd" | "a")
            && self.open.last() == Some(&name)
        {
            self.close(name);
        }
        if self.open.len() >= MAX_HTML_DEPTH {
            return Err(MobiError::TextTooLarge);
        }
        self.output.push('<');
        self.output.push_str(name);
        if let Some(features) = &mut self.features {
            features.record(tag, name, self.output.len());
            features
                .presentation
                .alignment(tag, name, self.output.len())?;
        }
        let has_id = tag.attrs.iter().any(|attr| {
            let value = attr.value.trim();
            let attribute: &str = attr.name.local.as_ref();
            attribute == "id" && !value.is_empty() && value.len() <= 512
        });
        for attr in &tag.attrs {
            let value = attr.value.trim();
            if value.is_empty() || value.len() > 512 {
                continue;
            }
            let name = match attr.name.local.as_ref() {
                "id" => "id",
                "name" if name == "a" && !has_id => "id",
                "href" if name == "a" && value.starts_with('#') => "href",
                "dir" if matches!(value, "ltr" | "rtl" | "auto") => "dir",
                "lang" => "lang",
                _ => continue,
            };
            self.output.push(' ');
            self.output.push_str(name);
            self.output.push_str("=\"");
            escape_xml(&mut self.output, value)?;
            self.output.push('"');
        }
        if matches!(name, "br" | "hr") || tag.self_closing {
            self.output.push_str(" />");
        } else {
            self.output.push('>');
            self.open.push(name);
        }
        if self.output.len() > MAX_XHTML_BYTES {
            return Err(MobiError::TextTooLarge);
        }
        Ok(())
    }

    fn close(&mut self, name: &str) {
        if let Some(index) = self.open.iter().rposition(|open| *open == name) {
            self.close_to(index);
        }
    }

    fn close_to(&mut self, index: usize) {
        while self.open.len() > index {
            let name = self.open.pop().unwrap();
            self.output.push_str("</");
            self.output.push_str(name);
            self.output.push('>');
        }
    }
}

fn safe_tag(name: &str) -> Option<&'static str> {
    const TAGS: &[&str] = &[
        "a",
        "b",
        "blockquote",
        "br",
        "code",
        "dd",
        "div",
        "dl",
        "dt",
        "em",
        "h1",
        "h2",
        "h3",
        "h4",
        "h5",
        "h6",
        "hr",
        "i",
        "li",
        "ol",
        "p",
        "pre",
        "s",
        "small",
        "span",
        "strong",
        "sub",
        "sup",
        "table",
        "tbody",
        "td",
        "th",
        "thead",
        "tr",
        "u",
        "ul",
    ];
    match name {
        "mbp:pagebreak" => Some("hr"),
        "center" | "section" | "article" => Some("div"),
        _ => TAGS.iter().copied().find(|tag| *tag == name),
    }
}

#[cfg(test)]
pub(crate) fn test_exth(entries: &[(u32, &[u8])]) -> Vec<u8> {
    let mut bytes = b"EXTH".to_vec();
    bytes.extend_from_slice(&[0; 4]);
    bytes.extend_from_slice(&(entries.len() as u32).to_be_bytes());
    for &(kind, value) in entries {
        bytes.extend_from_slice(&kind.to_be_bytes());
        bytes.extend_from_slice(&((8 + value.len()) as u32).to_be_bytes());
        bytes.extend_from_slice(value);
    }
    let length = bytes.len() as u32;
    bytes[4..8].copy_from_slice(&length.to_be_bytes());
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_header(compression: u16, bytes: usize, records: u16) -> Vec<u8> {
        let mut header = vec![0; 16];
        header[0..2].copy_from_slice(&compression.to_be_bytes());
        header[4..8].copy_from_slice(&(bytes as u32).to_be_bytes());
        header[8..10].copy_from_slice(&records.to_be_bytes());
        header[10..12].copy_from_slice(&4096u16.to_be_bytes());
        header
    }

    fn pdb(signature: &[u8; 8], records: &[&[u8]]) -> Vec<u8> {
        let mut output = vec![0; PDB_HEADER_BYTES + 8 * records.len() + 2];
        output[..11].copy_from_slice(b"Reader Test");
        output[60..68].copy_from_slice(signature);
        output[76..78].copy_from_slice(&(records.len() as u16).to_be_bytes());
        for (index, record) in records.iter().enumerate() {
            let length = output.len() as u32;
            let entry = PDB_HEADER_BYTES + index * 8;
            output[entry..entry + 4].copy_from_slice(&length.to_be_bytes());
            output.extend_from_slice(record);
        }
        output
    }

    fn mobi_header_for_test(
        compression: u16,
        bytes: usize,
        encoding: u32,
        version: u32,
        trailers: u16,
    ) -> Vec<u8> {
        let mut header = text_header(compression, bytes, 1);
        header.resize(244, 0);
        header[16..20].copy_from_slice(b"MOBI");
        header[20..24].copy_from_slice(&228u32.to_be_bytes());
        header[24..28].copy_from_slice(&2u32.to_be_bytes());
        header[28..32].copy_from_slice(&encoding.to_be_bytes());
        header[36..40].copy_from_slice(&version.to_be_bytes());
        header[242..244].copy_from_slice(&trailers.to_be_bytes());
        header
    }

    #[test]
    fn full_length_mobi_opens_its_first_and_last_sections_without_losing_content() {
        use std::io::Write;
        let mut text = "<html><body><p id='opening'>Start</p>".to_owned();
        text.push_str(&"<p>The paper is ready. </p>".repeat(26_100));
        text.push_str("<p id='finish'>Finish</p></body></html>");
        let mut header = mobi_header_for_test(2, text.len(), 65001, 6, 0);
        header.extend(test_exth(&[(524, b"en-US")]));
        let chunks: Vec<_> = text.as_bytes().chunks(4096).collect();
        header[8..10].copy_from_slice(&(chunks.len() as u16).to_be_bytes());
        // These ASCII bytes are literal tokens in PalmDOC compression.
        let records: Vec<_> = std::iter::once(header.as_slice()).chain(chunks).collect();
        let path =
            std::env::temp_dir().join(format!("reader-large-mobi-{}.mobi", std::process::id()));
        std::fs::File::create(&path)
            .unwrap()
            .write_all(&pdb(b"BOOKMOBI", &records))
            .unwrap();
        let mut book = crate::epub::Book::open(&path).unwrap();
        std::fs::remove_file(path).unwrap();
        assert!(book.chapters.len() > 4);
        assert_eq!(book.language, parley::Language::parse("en-US").ok());
        let sections = book.chapters.len();
        let mut paragraphs = 0;
        for i in 0..sections {
            let xml = book.chapter(i).unwrap();
            let document = roxmltree::Document::parse(&xml).unwrap();
            paragraphs += document
                .descendants()
                .filter(|n| n.has_tag_name("p"))
                .count();
            if i == 0 {
                assert!(xml.contains("id=\"opening\""));
            }
            if i + 1 == sections {
                assert!(xml.contains("id=\"finish\""));
            }
        }
        assert_eq!(paragraphs, 26_102);
    }

    #[test]
    fn decodes_uncompressed_palm_paragraphs_and_windows_1252() {
        let payload = b"First line \x93quoted\x94\r\nsecond line\r\n\r\nCaf\xe9 & tide.";
        let mut header = text_header(1, payload.len(), 1);
        header[12..16].copy_from_slice(&17u32.to_be_bytes());
        let document = decode(&pdb(b"TEXtREAd", &[&header, payload])).unwrap();
        assert_eq!(document.title.as_deref(), Some("Reader Test"));
        assert_eq!(document.author, None);
        assert!(
            document
                .xhtml
                .contains("<p>First line “quoted”<br />second line</p><p>Café &amp; tide.</p>")
        );
        assert!(roxmltree::Document::parse(&document.xhtml).is_ok());
    }

    #[test]
    fn reads_first_valid_bounded_language_and_leaves_the_normalized_markup_intact() {
        let html = b"<body><p>hyphenation <span lang='vi'>hyphenation</span></p></body>";
        let base = mobi_header_for_test(1, html.len(), 65001, 6, 0);
        let old = decode(&pdb(b"BOOKMOBI", &[&base, html])).unwrap();
        assert!(old.language.is_none());
        let mut header = base;
        let oversized = vec![b'a'; crate::source::MAX_LANGUAGE_BYTES + 1];
        header.extend(test_exth(&[
            (524, b"invalid lang"),
            (524, &oversized),
            (524, &[0xff]),
            (524, b"EN-us"),
            (524, b"en-GB"),
        ]));
        let bytes = pdb(b"BOOKMOBI", &[&header, html]);
        let tagged = decode(&bytes).unwrap();
        assert_eq!(old.xhtml, tagged.xhtml);
        let path =
            std::env::temp_dir().join(format!("reader-language-small-{}.mobi", std::process::id()));
        std::fs::write(&path, bytes).unwrap();
        let mut book = crate::epub::Book::open(&path).unwrap();
        std::fs::remove_file(path).unwrap();
        assert_eq!(book.language, parley::Language::parse("en-US").ok());
        let xml = book.chapter(0).unwrap();
        let source = crate::source::Source::parse_with_language(&xml, book.language).unwrap();
        let runs = &source.blocks[0].styled_text;
        assert_eq!(runs[0].language, parley::Language::parse("en-US").ok());
        assert_eq!(
            runs.last().unwrap().language,
            parley::Language::parse("vi").ok()
        );
    }

    #[test]
    fn expands_overlapping_palmdoc_tokens_within_the_current_record() {
        let record = [b'A', 0x80, 0x0b, 0xc2, 1, 0xe9];
        let header = text_header(2, 11, 2);
        let document = decode(&pdb(b"TEXtREAd", &[&header, &record, b"!"])).unwrap();
        assert!(document.xhtml.contains("<p>AAAAAAA Bé!</p>"));

        let invalid = [0x80, 0x08];
        assert_eq!(
            decode(&pdb(b"TEXtREAd", &[&header, &record, &invalid])).unwrap_err(),
            MobiError::InvalidText
        );
        let mut result = Vec::new();
        assert_eq!(
            decode_palmdoc_record(&record, 3, &mut result),
            Err(MobiError::InvalidText)
        );
        assert_eq!(
            decode_palmdoc_record(&[4, b'A'], 100, &mut result),
            Err(MobiError::InvalidText)
        );
    }

    #[test]
    fn normalizes_legacy_html_and_reads_exth_metadata_with_record_trailers() {
        let html = b"<HEAD><TITLE>Ignore this</TITLE></HEAD><H1>Coast &amp; harbor</H1><p>First<p>Second <a id=\"night\" href='#night'>at night</a><script>discard &amp; instructions</script><a href='https://example.invalid/'>external</a><mbp:pagebreak />";
        let mut header = mobi_header_for_test(1, html.len(), 65001, 6, 3);
        header.extend(test_exth(&[
            (100, b"A. Reader"),
            (503, b"Harbor Notes"),
            (524, b"vi"),
            (103, b"<p>A coastal description.</p>"),
            (105, b"Navigation"),
            (105, b"Tides"),
        ]));
        let mut record = html.to_vec();
        record.extend([0xc3, 1, 0xfa, 0x82]);
        let marked = decode(&pdb(b"BOOKMOBI", &[&header, &record, b"BOUNDARY"])).unwrap();
        assert_eq!(marked.title.as_deref(), Some("Harbor Notes"));
        assert_eq!(marked.author.as_deref(), Some("A. Reader"));
        assert_eq!(marked.language, parley::Language::parse("vi").ok());
        assert_eq!(
            marked.metadata.description.as_deref(),
            Some("<p>A coastal description.</p>")
        );
        assert_eq!(marked.metadata.keywords, ["Navigation", "Tides"]);
        assert!(
            marked
                .xhtml
                .contains("<h1>Coast &amp; harbor</h1><p>First</p><p>Second ")
        );
        assert!(marked.xhtml.contains("id=\"night\" href=\"#night\""));
        assert!(marked.xhtml.contains("<a>external</a></p><hr />"));
        assert!(!marked.xhtml.contains("discard"));
        assert!(!marked.xhtml.contains("Ignore this"));
        assert!(!marked.xhtml.contains("BOUNDARY"));
        assert!(roxmltree::Document::parse(&marked.xhtml).is_ok());
    }

    #[test]
    fn mobi_file_positions_use_original_encoding_and_only_real_local_records() {
        let mut raw = b"<p>Caf".to_vec();
        raw.extend(b"\xe9, <a filepos='AAAAAAAAAA'>lake</a> <a filepos='BBBBBBBBBB'>comment</a> <a filepos='CCCCCCCCCC'>inside tag</a>. <img recindex='1' alt='Map &amp; scale'/><img recindex='2'/></p><!--<p>--> <p title='<p >'>ignored marker</p><div id='shore'>Arrival.</div>");
        let index = |needle: &[u8]| raw.windows(needle.len()).position(|s| s == needle).unwrap();
        let targets = [
            index(b"<div"),
            index(b"<!--<p") + 4,
            index(b"title='<p") + 7,
        ];
        for (symbol, target) in [b'A', b'B', b'C'].into_iter().zip(targets) {
            let place = raw.windows(10).position(|s| s == [symbol; 10]).unwrap();
            raw[place..place + 10].copy_from_slice(format!("{target:010}").as_bytes());
        }
        let mut header = mobi_header_for_test(1, raw.len(), 1252, 6, 0);
        header[108..112].copy_from_slice(&2u32.to_be_bytes());
        let doc = decode_legacy(&pdb(
            b"BOOKMOBI",
            &[&header, &raw, b"GIF89a-test-resource", b"arbitrary"],
        ))
        .unwrap();
        assert_eq!(doc.resources.len(), 1);
        let chapter = crate::kf8::Kf8Chapter {
            path: "document.xhtml".into(),
            label: "Field notes".into(),
            xhtml: doc.xhtml,
            continued: false,
        };
        let (parts, _) = crate::kindle_sections::legacy(
            chapter,
            doc.additions,
            doc.presentation,
            &doc.contents.insertions,
            false,
        )
        .unwrap();
        let xml = roxmltree::Document::parse(&parts[0].xhtml).unwrap();
        let hrefs: Vec<_> = xml
            .descendants()
            .filter_map(|n| n.attribute("href"))
            .collect();
        assert_eq!(hrefs, ["#shore"]);
        let images: Vec<_> = xml
            .descendants()
            .filter(|n| n.has_tag_name("img"))
            .collect();
        assert_eq!(images.len(), 1);
        assert_eq!(images[0].attribute("alt"), Some("Map & scale"));
        assert_eq!(
            doc.resources[images[0].attribute("src").unwrap()],
            b"GIF89a-test-resource"
        );
        assert!(parts[0].xhtml.contains("Café"));
    }

    #[test]
    fn mobi_images_can_stand_alone_with_a_valid_resource() {
        let xml = b"<img recindex='1' />";
        let mut header = mobi_header_for_test(1, xml.len(), 65001, 6, 0);
        header[108..112].copy_from_slice(&2u32.to_be_bytes());
        let good = decode_legacy(&pdb(b"BOOKMOBI", &[&header, xml, b"GIF87a-record"])).unwrap();
        assert_eq!(good.additions.len(), 1);
        assert_eq!(good.resources.len(), 1);
        assert_eq!(
            decode(&pdb(b"BOOKMOBI", &[&header, xml, b"unrecognized"])).unwrap_err(),
            MobiError::InvalidText
        );
    }
    #[test]
    fn rejects_protected_or_invalid_mobi_headers_and_unknown_compression() {
        let text = b"<p>Tide</p>";
        let header = |compression, encoding, version| {
            mobi_header_for_test(compression, text.len(), encoding, version, 0)
        };
        for (metadata, expected) in [
            (header(17_480, 65001, 6), MobiError::InvalidHuffman),
            (
                header(12345, 65001, 6),
                MobiError::UnsupportedCompression(12345),
            ),
            (header(1, 65001, 8), MobiError::Kf8),
            (header(1, 65000, 6), MobiError::UnsupportedEncoding(65000)),
        ] {
            assert_eq!(
                decode(&pdb(b"BOOKMOBI", &[&metadata, text])).unwrap_err(),
                expected
            );
        }
        let mut protected = header(1, 65001, 6);
        protected[12..14].copy_from_slice(&2u16.to_be_bytes());
        assert_eq!(
            decode(&pdb(b"BOOKMOBI", &[&protected, text])).unwrap_err(),
            MobiError::Protected
        );
        protected[12..14].fill(0);
        protected[172..176].copy_from_slice(&1u32.to_be_bytes());
        assert_eq!(
            decode(&pdb(b"BOOKMOBI", &[&protected, text])).unwrap_err(),
            MobiError::Protected
        );
        let mut bad = pdb(b"BOOKMOBI", &[&header(1, 65001, 6), text]);
        bad[86..90].copy_from_slice(&1u32.to_be_bytes());
        assert_eq!(decode(&bad).unwrap_err(), MobiError::InvalidHeader);
        assert_eq!(without_trailers(b"\x80", 2), Err(MobiError::InvalidText));
        assert_eq!(without_trailers(b"\x03", 1), Err(MobiError::InvalidText));
    }

    #[test]
    fn removes_a_multibyte_sized_trailer_and_joins_split_utf8_records() {
        let first = b"<p>Caf\xc3";
        let second = b"\xa9 &amp; tide</p>";
        let mut header = mobi_header_for_test(1, first.len() + second.len(), 65001, 6, 3);
        header[8..10].copy_from_slice(&2u16.to_be_bytes());
        let mut records = [first.to_vec(), second.to_vec()];
        records[0].extend([0xa9, 1]);
        records[1].push(0);
        for record in &mut records {
            record.extend([b'Z'; 130]);
            record.extend([0x81, 4]);
        }
        let book = decode(&pdb(b"BOOKMOBI", &[&header, &records[0], &records[1]])).unwrap();
        assert!(book.xhtml.contains("<p>Café &amp; tide</p>"));
    }
}
