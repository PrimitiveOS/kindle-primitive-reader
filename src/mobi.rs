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
use std::cell::RefCell;
use std::fmt;

pub const MAX_MOBI_BYTES: usize = 32 * 1024 * 1024;
const MAX_TEXT_BYTES: usize = 512 * 1024;
const MAX_XHTML_BYTES: usize = 1024 * 1024;
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
    pub title: Option<String>,
    pub author: Option<String>,
    pub xhtml: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MobiError {
    UnsupportedContainer,
    Protected,
    Kf8,
    UnsupportedCompression(u16),
    UnsupportedEncoding(u32),
    InvalidHeader,
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
            Self::Protected => formatter.write_str("This MOBI file is encrypted."),
            Self::Kf8 => formatter.write_str(
                "This Kindle file uses a KF8 variant or feature Reader does not support.",
            ),
            Self::UnsupportedCompression(17_480) => {
                formatter.write_str("This MOBI file uses HUFF/CDIC compression.")
            }
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
    Auto,
}

struct MobiHeader {
    encoding: Encoding,
    extra_data: u16,
    is_plain_text: bool,
    title: Option<String>,
    author: Option<String>,
}

/// Decode the first, legacy text section of a database. All returned content is local.
pub fn decode(bytes: &[u8]) -> Result<MobiDocument, MobiError> {
    if bytes.len() > MAX_MOBI_BYTES {
        return Err(MobiError::TooLarge);
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
    let text = decode_records(bytes, &records, header, extra_data)?;
    let encoding = mobi.as_ref().map_or(Encoding::Auto, |mobi| mobi.encoding);
    let text = decode_text(text, encoding)?;
    let text = text.trim_start_matches('\u{feff}');
    let xhtml = if mobi.as_ref().is_none_or(|mobi| mobi.is_plain_text) {
        plain_xhtml(text)?
    } else {
        html_xhtml(text)?
    };

    let fallback_title = || read_label(&bytes[..32], encoding);
    let (title, author) = match mobi {
        Some(mobi) => (mobi.title.or_else(fallback_title), mobi.author),
        None => (fallback_title(), None),
    };
    Ok(MobiDocument {
        title,
        author,
        xhtml,
    })
}

pub(crate) fn decode_records(
    bytes: &[u8],
    records: &[usize],
    header: &[u8],
    extra_data: u16,
) -> Result<Vec<u8>, MobiError> {
    let compression = u16_at(header, 0)?;
    if !matches!(compression, 1 | 2) {
        return Err(MobiError::UnsupportedCompression(compression));
    }
    let text_length = u32_at(header, 4)? as usize;
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

    let mut text = Vec::with_capacity(text_length);
    for pair in records[1..=text_records + 1].windows(2) {
        let payload = without_trailers(&bytes[pair[0]..pair[1]], extra_data)?;
        let limit = record_size.min(text_length + record_size - text.len());
        match compression {
            1 if payload.len() <= limit => text.extend_from_slice(payload),
            1 => return Err(MobiError::InvalidText),
            _ => decode_palmdoc_record(payload, limit, &mut text)?,
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
        other => return Err(MobiError::UnsupportedEncoding(other)),
    };
    let (title, author) = mobi_labels(record, header, encoding);
    Ok(MobiHeader {
        encoding,
        extra_data: if version >= 5 {
            u16_at(header, 242).unwrap_or(0)
        } else {
            0
        },
        is_plain_text: matches!(kind, 3 | 517),
        title,
        author,
    })
}

fn mobi_labels(
    record: &[u8],
    header: &[u8],
    encoding: Encoding,
) -> (Option<String>, Option<String>) {
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
    let (updated_title, author) = exth_labels(&record[end..], encoding);
    (updated_title.or(full_name), author)
}

pub(crate) fn kf8_labels(
    pdb: &[u8],
    record: &[u8],
    header: &[u8],
) -> (Option<String>, Option<String>) {
    let (title, author) = mobi_labels(record, header, Encoding::Utf8);
    (
        title.or_else(|| read_label(&pdb[..32], Encoding::Utf8)),
        author,
    )
}

fn exth_labels(bytes: &[u8], encoding: Encoding) -> (Option<String>, Option<String>) {
    let mut title = None;
    let mut author = None;
    if bytes.get(..4) != Some(b"EXTH") {
        return (title, author);
    }
    let Some(end) = u32_at(bytes, 4)
        .ok()
        .filter(|size| (12..=bytes.len() as u32).contains(size))
    else {
        return (title, author);
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
            100 if author.is_none() => author = read_label(value, encoding),
            503 if title.is_none() => title = read_label(value, encoding),
            _ => {}
        }
        bytes = &bytes[length..];
        if title.is_some() && author.is_some() {
            break;
        }
    }
    (title, author)
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

fn decode_text(bytes: Vec<u8>, encoding: Encoding) -> Result<String, MobiError> {
    match encoding {
        Encoding::Utf8 => String::from_utf8(bytes).map_err(|_| MobiError::InvalidText),
        Encoding::Cp1252 => Ok(bytes.into_iter().map(cp1252).collect()),
        Encoding::Auto => match String::from_utf8(bytes) {
            Ok(text) => Ok(text),
            Err(error) => Ok(error.into_bytes().into_iter().map(cp1252).collect()),
        },
    }
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

fn html_xhtml(text: &str) -> Result<String, MobiError> {
    let input = BufferQueue::default();
    input.push_back(text.into());
    let tokenizer = Tokenizer::new(
        Normalizer(RefCell::new(Markup::default())),
        Default::default(),
    );
    let _ = tokenizer.feed(&input);
    tokenizer.end();
    let mut markup = tokenizer.sink.0.into_inner();
    if let Some(error) = markup.error {
        return Err(error);
    }
    if !markup.has_text {
        return Err(MobiError::InvalidText);
    }
    markup.close_to(0);
    finish_xhtml(markup.output)
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
}

impl Default for Markup {
    fn default() -> Self {
        Self {
            output: XHTML_START.to_owned(),
            open: Vec::new(),
            ignore: None,
            error: None,
            has_text: false,
        }
    }
}

struct Normalizer(RefCell<Markup>);

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
                markup.error = escape_xml(&mut markup.output, &text).err();
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
        let mut exth = b"EXTH\0\0\0\0\0\0\0\x02".to_vec();
        for (kind, value) in [(100u32, "A. Reader"), (503, "Harbor Notes")] {
            exth.extend(kind.to_be_bytes());
            exth.extend(((value.len() + 8) as u32).to_be_bytes());
            exth.extend(value.as_bytes());
        }
        let len = exth.len() as u32;
        exth[4..8].copy_from_slice(&len.to_be_bytes());
        header.extend(exth);
        let mut record = html.to_vec();
        record.extend([0xc3, 1, 0xfa, 0x82]);
        let marked = decode(&pdb(b"BOOKMOBI", &[&header, &record, b"BOUNDARY"])).unwrap();
        assert_eq!(marked.title.as_deref(), Some("Harbor Notes"));
        assert_eq!(marked.author.as_deref(), Some("A. Reader"));
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
    fn rejects_encrypted_huff_kf8_and_out_of_range_record_tables() {
        let text = b"<p>Tide</p>";
        let header = |compression, encoding, version| {
            mobi_header_for_test(compression, text.len(), encoding, version, 0)
        };
        for (metadata, expected) in [
            (
                header(17_480, 65001, 6),
                MobiError::UnsupportedCompression(17_480),
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
