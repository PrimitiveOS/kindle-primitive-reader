//! Read progress from KOReader's generated `metadata.<format>.lua` tables.
//!
//! Field names and sampling behavior were verified against KOReader revision
//! 042d9b39d4835408121970229cc37624e4f794bc.
//! https://github.com/koreader/koreader/blob/042d9b39d4835408121970229cc37624e4f794bc/frontend/docsettings.lua
//! https://github.com/koreader/koreader/blob/042d9b39d4835408121970229cc37624e4f794bc/frontend/util.lua#L1084-L1111
//! https://github.com/koreader/koreader/blob/042d9b39d4835408121970229cc37624e4f794bc/frontend/apps/reader/modules/readerpaging.lua
//! https://github.com/koreader/koreader/blob/042d9b39d4835408121970229cc37624e4f794bc/frontend/apps/reader/modules/readerrolling.lua
//! https://www.lua.org/manual/5.1/manual.html#2.1

mod annotations;
#[cfg(feature = "kindle")]
pub(crate) mod custom_metadata;
#[cfg(feature = "kindle")]
pub(crate) mod exchange;
pub use annotations::{SidecarAnnotations, SidecarMark, parse_annotations};

use std::borrow::Cow;
use std::fmt;
use std::io::{self, Read, Seek, SeekFrom};

pub const MAX_SIDECAR_BYTES: usize = 2 * 1024 * 1024;
const MAX_DEPTH: usize = 48;
const MAX_FIELDS: usize = 65_536;
const MAX_STRING_BYTES: usize = 512 * 1024;
const MAX_POINTER_BYTES: usize = 8192;
const MAX_LONG_LEVEL: usize = 16;
const SAMPLE_BYTES: usize = 1024;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SidecarProgress {
    /// KOReader stores the current fixed page starting at one.
    pub last_page: Option<u32>,
    pub doc_pages: Option<u32>,
    pub partial_md5_checksum: Option<[u8; 16]>,
    /// A normalized CREngine pointer from a reflowable book.
    pub last_xpointer: Option<String>,
    /// Newer KOReader versions also save an EPUB Canonical Fragment Identifier.
    pub last_epubcfi: Option<String>,
    pub cre_dom_version: Option<u32>,
    /// The progress fraction, including the legacy `last_percent` fallback.
    pub percent_finished: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SidecarError {
    TooLarge,
    InvalidData,
    InvalidField(&'static str),
    DuplicateField(&'static str),
    MissingPosition,
    ReflowablePointer,
    MissingChecksum,
    ChecksumMismatch,
    MissingPageCount,
    PageCountMismatch,
    PageOutOfBounds,
}

impl fmt::Display for SidecarError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLarge => {
                formatter.write_str("The KOReader sidecar exceeds Reader's import limits.")
            }
            Self::InvalidData => {
                formatter.write_str("This file is not a supported KOReader data table.")
            }
            Self::InvalidField(name) => write!(
                formatter,
                "The KOReader sidecar has an invalid {name} field."
            ),
            Self::DuplicateField(name) => {
                write!(formatter, "The KOReader sidecar has two {name} fields.")
            }
            Self::MissingPosition => {
                formatter.write_str("The KOReader sidecar has no fixed-page position.")
            }
            Self::ReflowablePointer => {
                formatter.write_str("This position uses a KOReader reflowable book pointer.")
            }
            Self::MissingChecksum => {
                formatter.write_str("The sidecar has no KOReader document checksum.")
            }
            Self::ChecksumMismatch => formatter
                .write_str("The selected book differs from the sidecar's KOReader checksum."),
            Self::MissingPageCount => formatter.write_str("The sidecar has no saved page count."),
            Self::PageCountMismatch => formatter
                .write_str("The selected book has a different page count from the sidecar."),
            Self::PageOutOfBounds => {
                formatter.write_str("The saved position is outside the selected book.")
            }
        }
    }
}

impl std::error::Error for SidecarError {}

impl SidecarProgress {
    /// Return a zero-based page for the explicitly selected fixed-layout book.
    /// The checksum is KOReader's sampled fingerprint. Compare it alongside
    /// the saved page count. The fingerprint does not cover unsampled bytes.
    pub fn resolve_fixed(
        &self,
        checksum: [u8; 16],
        page_count: usize,
    ) -> Result<usize, SidecarError> {
        if self.last_xpointer.is_some() || self.last_epubcfi.is_some() {
            return Err(SidecarError::ReflowablePointer);
        }
        let page = self.last_page.ok_or(SidecarError::MissingPosition)? as usize;
        self.check_fingerprint(checksum)?;
        if self.doc_pages.ok_or(SidecarError::MissingPageCount)? as usize != page_count {
            return Err(SidecarError::PageCountMismatch);
        }
        if page == 0 || page > page_count {
            return Err(SidecarError::PageOutOfBounds);
        }
        Ok(page - 1)
    }

    #[cfg(feature = "kindle")]
    pub(crate) fn reflow_position(
        &self,
        book: &mut crate::epub::Book,
    ) -> crate::Result<crate::epub::EpubPosition> {
        let cfi = self
            .last_epubcfi
            .as_deref()
            .map(|cfi| crate::epub_cfi::resolve(book, cfi));
        let pointer = self
            .last_xpointer
            .as_deref()
            .map(|value| crate::cre_pointer::resolve(book, value, self.cre_dom_version));
        match (cfi, pointer) {
            (Some(Ok(a)), Some(Ok(b))) if !same_reflow_target(&a, &b) => Err(
                "The saved CFI and CREngine pointer identify different reading positions".into(),
            ),
            (Some(Ok(a)), _) | (_, Some(Ok(a))) => Ok(a),
            (Some(Err(error)), _) | (_, Some(Err(error))) => Err(error),
            (None, None) => Err("This sidecar has no EPUB reading position".into()),
        }
    }

    pub(crate) fn check_fingerprint(&self, checksum: [u8; 16]) -> Result<(), SidecarError> {
        let stored = self
            .partial_md5_checksum
            .ok_or(SidecarError::MissingChecksum)?;
        if stored != checksum {
            return Err(SidecarError::ChecksumMismatch);
        }
        Ok(())
    }
}

fn same_reflow_target(
    first: &crate::epub::EpubPosition,
    second: &crate::epub::EpubPosition,
) -> bool {
    let mut first_anchor = first.anchor.clone();
    let mut second_anchor = second.anchor.clone();
    first_anchor.side_bias = None;
    second_anchor.side_bias = None;
    first.chapter == second.chapter && first_anchor == second_anchor
}

/// Calculate the KOReader fingerprint using at most 12 KiB of file samples.
/// The reader's original seek position is restored before returning.
pub fn partial_md5(reader: &mut (impl Read + Seek)) -> io::Result<[u8; 16]> {
    let original = reader.stream_position()?;
    let result = sample_digest(reader);
    reader.seek(SeekFrom::Start(original))?;
    result
}

fn sample_digest(reader: &mut (impl Read + Seek)) -> io::Result<[u8; 16]> {
    let mut digest = md5::Context::new();
    let mut sample = [0u8; SAMPLE_BYTES];
    // LuaJIT's 32-bit shift produces zero for the first loop iteration.
    for offset in std::iter::once(0).chain((0..=10).map(|index| 1024u64 << (2 * index))) {
        reader.seek(SeekFrom::Start(offset))?;
        let mut filled = 0;
        while filled < sample.len() {
            match reader.read(&mut sample[filled..]) {
                Ok(0) => break,
                Ok(size) => filled += size,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            }
        }
        if filled == 0 {
            break;
        }
        digest.consume(&sample[..filled]);
        if filled < sample.len() {
            break;
        }
    }
    Ok(digest.finalize().0)
}

/// Read a bounded `return { ... }` table as literals and reject executable
/// syntax anywhere in the input, including fields that Reader does not use.
pub fn parse(bytes: &[u8]) -> Result<SidecarProgress, SidecarError> {
    let mut state = ProgressFields::default();
    read_fields(bytes, &mut |key, value| {
        if let Some(key) = key {
            state.insert(&key, value)?;
        }
        Ok(())
    })?;
    if state.seen == 0 {
        return Err(SidecarError::InvalidData);
    }
    state.value.percent_finished = state.value.percent_finished.or(state.legacy_percent);
    Ok(state.value)
}

type Visitor<'v, 'a> = dyn FnMut(Option<Cow<'a, [u8]>>, Value<'a>) -> Result<(), SidecarError> + 'v;

fn read_fields<'a>(bytes: &'a [u8], visitor: &mut Visitor<'_, 'a>) -> Result<(), SidecarError> {
    if bytes.len() > MAX_SIDECAR_BYTES {
        return Err(SidecarError::TooLarge);
    }
    let bytes = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes);
    let mut parser = Parser {
        bytes,
        offset: 0,
        fields: 0,
    };
    parser.space()?;
    if parser.identifier() != Some(b"return") {
        return Err(SidecarError::InvalidData);
    }
    parser.table(1, Some(visitor))?;
    parser.space()?;
    if parser.take(b';') {
        parser.space()?;
    }
    if parser.offset != bytes.len() {
        return Err(SidecarError::InvalidData);
    }
    Ok(())
}

#[derive(Default)]
struct ProgressFields {
    seen: u8,
    value: SidecarProgress,
    legacy_percent: Option<f64>,
}

impl ProgressFields {
    fn insert(&mut self, key: &[u8], value: Value<'_>) -> Result<(), SidecarError> {
        const FIELDS: [&str; 8] = [
            "last_page",
            "doc_pages",
            "partial_md5_checksum",
            "last_xpointer",
            "cre_dom_version",
            "percent_finished",
            "last_percent",
            "last_epubcfi",
        ];
        let Some((index, key)) = FIELDS
            .iter()
            .enumerate()
            .find(|(_, field)| field.as_bytes() == key)
        else {
            return Ok(());
        };
        if self.seen & (1 << index) != 0 {
            return Err(SidecarError::DuplicateField(key));
        }
        self.seen |= 1 << index;
        if matches!(value, Value::Nil) {
            return Ok(());
        }
        let invalid = SidecarError::InvalidField(key);
        match *key {
            "last_page" => {
                self.value.last_page =
                    Some(value.integer().filter(|&page| page > 0).ok_or(invalid)?)
            }
            "doc_pages" => {
                self.value.doc_pages =
                    Some(value.integer().filter(|&pages| pages > 0).ok_or(invalid)?)
            }
            "cre_dom_version" => self.value.cre_dom_version = Some(value.integer().ok_or(invalid)?),
            "partial_md5_checksum" => {
                self.value.partial_md5_checksum =
                    Some(parse_digest(value.string().ok_or(invalid)?).ok_or(invalid)?);
            }
            "last_xpointer" => {
                let bytes = value
                    .string()
                    .filter(|bytes| (1..=MAX_POINTER_BYTES).contains(&bytes.len()))
                    .ok_or(invalid)?;
                let pointer = std::str::from_utf8(bytes).map_err(|_| invalid)?;
                if !pointer.starts_with('/') || pointer.chars().any(char::is_control) {
                    return Err(invalid);
                }
                self.value.last_xpointer = Some(pointer.to_owned());
            }
            "last_epubcfi" => {
                let bytes = value
                    .string()
                    .filter(|bytes| (1..=MAX_POINTER_BYTES).contains(&bytes.len()))
                    .ok_or(invalid)?;
                let cfi = std::str::from_utf8(bytes).map_err(|_| invalid)?;
                if !cfi.starts_with("epubcfi(")
                    || !cfi.ends_with(')')
                    || cfi.chars().any(char::is_control)
                {
                    return Err(invalid);
                }
                self.value.last_epubcfi = Some(cfi.to_owned());
            }
            "percent_finished" => {
                self.value.percent_finished = Some(value.percent().ok_or(invalid)?)
            }
            "last_percent" => self.legacy_percent = Some(value.percent().ok_or(invalid)?),
            _ => unreachable!(),
        }
        Ok(())
    }
}

fn parse_digest(bytes: &[u8]) -> Option<[u8; 16]> {
    if bytes.len() != 32 {
        return None;
    }
    let mut digest = [0; 16];
    for (value, pair) in digest.iter_mut().zip(bytes.as_chunks::<2>().0) {
        *value = (hex(pair[0])? << 4) | hex(pair[1])?;
    }
    Some(digest)
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

enum Value<'a> {
    String(Cow<'a, [u8]>),
    Number(f64),
    Nil,
    Boolean(bool),
    Table(&'a [u8]),
}

impl Value<'_> {
    fn integer(&self) -> Option<u32> {
        match self {
            Self::Number(value)
                if (0.0..=f64::from(u32::MAX)).contains(value) && value.fract() == 0.0 =>
            {
                Some(*value as u32)
            }
            _ => None,
        }
    }

    fn string(&self) -> Option<&[u8]> {
        match self {
            Self::String(value) => Some(value),
            _ => None,
        }
    }

    fn percent(&self) -> Option<f64> {
        match self {
            Self::Number(value) if (0.0..=1.0).contains(value) => Some(*value),
            _ => None,
        }
    }
}

struct Parser<'a> {
    bytes: &'a [u8],
    offset: usize,
    fields: usize,
}

impl<'a> Parser<'a> {
    fn table(
        &mut self,
        depth: usize,
        mut visitor: Option<&mut Visitor<'_, 'a>>,
    ) -> Result<(), SidecarError> {
        if depth > MAX_DEPTH {
            return Err(SidecarError::TooLarge);
        }
        self.expect(b'{')?;
        self.space()?;
        while !self.take(b'}') {
            self.fields += 1;
            if self.fields > MAX_FIELDS {
                return Err(SidecarError::TooLarge);
            }
            let key = self.key()?;
            let value = self.value(depth)?;
            if let Some(visit) = visitor.as_deref_mut() {
                visit(key, value)?;
            }
            self.space()?;
            if self.take(b',') || self.take(b';') {
                self.space()?;
            } else {
                return self.expect(b'}');
            }
        }
        Ok(())
    }

    fn key(&mut self) -> Result<Option<Cow<'a, [u8]>>, SidecarError> {
        if self.peek() == Some(b'[') && self.long_level().is_none() {
            self.offset += 1;
            let token = self.scalar()?;
            self.expect(b']')?;
            self.expect(b'=')?;
            return match token {
                Value::String(text) => Ok(Some(text)),
                Value::Number(_) | Value::Boolean(_) => Ok(None),
                _ => Err(SidecarError::InvalidData),
            };
        }
        let before = self.offset;
        if let Some(name) = self.identifier() {
            self.space()?;
            if self.take(b'=') {
                if matches!(
                    name,
                    b"and"
                        | b"break"
                        | b"do"
                        | b"else"
                        | b"elseif"
                        | b"end"
                        | b"false"
                        | b"for"
                        | b"function"
                        | b"if"
                        | b"in"
                        | b"local"
                        | b"nil"
                        | b"not"
                        | b"or"
                        | b"repeat"
                        | b"return"
                        | b"then"
                        | b"true"
                        | b"until"
                        | b"while"
                ) {
                    return Err(SidecarError::InvalidData);
                }
                return Ok(Some(Cow::Borrowed(name)));
            }
            self.offset = before;
        }
        Ok(None)
    }

    fn value(&mut self, depth: usize) -> Result<Value<'a>, SidecarError> {
        self.space()?;
        if self.peek() == Some(b'{') {
            let start = self.offset;
            self.table(depth + 1, None)?;
            return Ok(Value::Table(&self.bytes[start..self.offset]));
        }
        self.scalar()
    }

    fn scalar(&mut self) -> Result<Value<'a>, SidecarError> {
        self.space()?;
        let value = match self.peek() {
            Some(quote @ (b'\'' | b'"')) => Value::String(self.quoted(quote)?),
            Some(b'[') => {
                let level = self.long_level().ok_or(SidecarError::InvalidData)?;
                let text = self.long_string(level)?;
                if text.len() > MAX_STRING_BYTES {
                    return Err(SidecarError::TooLarge);
                }
                Value::String(Cow::Borrowed(text))
            }
            Some(b'0'..=b'9' | b'-' | b'.') => Value::Number(self.number()?),
            _ => match self.identifier() {
                Some(b"nil") => Value::Nil,
                Some(b"true") => Value::Boolean(true),
                Some(b"false") => Value::Boolean(false),
                _ => return Err(SidecarError::InvalidData),
            },
        };
        Ok(value)
    }

    fn number(&mut self) -> Result<f64, SidecarError> {
        let start = self.offset;
        while self.peek().is_some_and(|byte| {
            byte.is_ascii_hexdigit() || matches!(byte, b'x' | b'X' | b'+' | b'-' | b'.')
        }) && !self.bytes[self.offset..].starts_with(b"--")
        {
            self.offset += 1;
        }
        let token = &self.bytes[start..self.offset];
        if token.len() > 64 {
            return Err(SidecarError::TooLarge);
        }
        let text = std::str::from_utf8(token).map_err(|_| SidecarError::InvalidData)?;
        let (negative, magnitude) = text
            .strip_prefix('-')
            .map_or((false, text), |rest| (true, rest));
        let hex = magnitude
            .strip_prefix("0x")
            .or_else(|| magnitude.strip_prefix("0X"));
        let value = if let Some(hex) = hex {
            let value = u64::from_str_radix(hex, 16).map_err(|_| SidecarError::InvalidData)? as f64;
            if negative { -value } else { value }
        } else {
            text.parse::<f64>().map_err(|_| SidecarError::InvalidData)?
        };
        value
            .is_finite()
            .then_some(value)
            .ok_or(SidecarError::InvalidData)
    }

    fn identifier(&mut self) -> Option<&'a [u8]> {
        let start = self.offset;
        if !self
            .peek()
            .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
        {
            return None;
        }
        self.offset += 1;
        while self
            .peek()
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            self.offset += 1;
        }
        Some(&self.bytes[start..self.offset])
    }

    fn quoted(&mut self, quote: u8) -> Result<Cow<'a, [u8]>, SidecarError> {
        self.offset += 1;
        let start = self.offset;
        let mut segment = start;
        let mut decoded: Option<Vec<u8>> = None;
        loop {
            if self.offset - start > MAX_STRING_BYTES {
                return Err(SidecarError::TooLarge);
            }
            let byte = self.peek().ok_or(SidecarError::InvalidData)?;
            let end = self.offset;
            self.offset += 1;
            if byte == quote {
                return Ok(match decoded {
                    Some(mut decoded) => {
                        decoded.extend_from_slice(&self.bytes[segment..end]);
                        Cow::Owned(decoded)
                    }
                    None => Cow::Borrowed(&self.bytes[start..end]),
                });
            }
            if matches!(byte, b'\n' | b'\r') {
                return Err(SidecarError::InvalidData);
            }
            if byte == b'\\' {
                let decoded = decoded.get_or_insert_with(Vec::new);
                decoded.extend_from_slice(&self.bytes[segment..end]);
                if let Some(escaped) = self.escape()? {
                    decoded.push(escaped);
                }
                segment = self.offset;
            }
        }
    }

    fn escape(&mut self) -> Result<Option<u8>, SidecarError> {
        let byte = self.peek().ok_or(SidecarError::InvalidData)?;
        self.offset += 1;
        let decoded = match byte {
            b'\\' | b'\'' | b'"' => byte,
            b'a' => 7,
            b'b' => 8,
            b'f' => 12,
            b'n' | b'\n' => b'\n',
            b'r' => b'\r',
            b'\r' => {
                self.take(b'\n');
                b'\n'
            }
            b't' => b'\t',
            b'v' => 11,
            b'z' => {
                while self.peek().is_some_and(|byte| byte.is_ascii_whitespace()) {
                    self.offset += 1;
                }
                return Ok(None);
            }
            b'x' => {
                let digits = self
                    .bytes
                    .get(self.offset..self.offset + 2)
                    .ok_or(SidecarError::InvalidData)?;
                self.offset += 2;
                (hex(digits[0]).ok_or(SidecarError::InvalidData)? << 4)
                    | hex(digits[1]).ok_or(SidecarError::InvalidData)?
            }
            b'0'..=b'9' => {
                let mut number = u16::from(byte - b'0');
                for _ in 0..2 {
                    let Some(digit @ b'0'..=b'9') = self.peek() else {
                        break;
                    };
                    self.offset += 1;
                    number = number * 10 + u16::from(digit - b'0');
                }
                u8::try_from(number).map_err(|_| SidecarError::InvalidData)?
            }
            _ => return Err(SidecarError::InvalidData),
        };
        Ok(Some(decoded))
    }

    fn long_level(&self) -> Option<usize> {
        let tail = self.bytes[self.offset..].strip_prefix(b"[")?;
        let level = tail.iter().take_while(|&&byte| byte == b'=').count();
        (tail.get(level) == Some(&b'[')).then_some(level)
    }

    fn long_string(&mut self, level: usize) -> Result<&'a [u8], SidecarError> {
        if level > MAX_LONG_LEVEL {
            return Err(SidecarError::TooLarge);
        }
        self.offset += level + 2;
        self.take(b'\r');
        self.take(b'\n');
        let start = self.offset;
        while let Some(index) = self.bytes[self.offset..]
            .iter()
            .position(|&byte| byte == b']')
        {
            let end = self.offset + index;
            let after = end + level + 2;
            if self
                .bytes
                .get(end + 1..after - 1)
                .is_some_and(|eqs| eqs.iter().all(|&byte| byte == b'='))
                && self.bytes.get(after - 1) == Some(&b']')
            {
                self.offset = after;
                return Ok(&self.bytes[start..end]);
            }
            self.offset = end + 1;
        }
        Err(SidecarError::InvalidData)
    }

    fn space(&mut self) -> Result<(), SidecarError> {
        loop {
            while self.peek().is_some_and(|byte| byte.is_ascii_whitespace()) {
                self.offset += 1;
            }
            if !self.bytes[self.offset..].starts_with(b"--") {
                return Ok(());
            }
            self.offset += 2;
            if let Some(level) = self.long_level() {
                self.long_string(level)?;
            } else {
                while self
                    .peek()
                    .is_some_and(|byte| !matches!(byte, b'\r' | b'\n'))
                {
                    self.offset += 1;
                }
            }
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.offset).copied()
    }

    fn take(&mut self, byte: u8) -> bool {
        if self.peek() == Some(byte) {
            self.offset += 1;
            return true;
        }
        false
    }

    fn expect(&mut self, byte: u8) -> Result<(), SidecarError> {
        self.space()?;
        self.take(byte)
            .then_some(())
            .ok_or(SidecarError::InvalidData)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    const FIXED: &[u8] = br#"
-- metadata.pdf.lua
return {
    ["annotations"] = {
        [1] = {
            ["text"] = "quoted value with \"braces\" { }",
            ["pos0"] = { ["x"] = -2.012e+2; ["y"] = 0x10, },
            ["flags"] = { false, nil, true, [false] = 7, },
        },
    },
    ["doc_pages"] = 25,
    ["last_page"] = 14.0,
    ["partial_md5_checksum"] = "AE3095F24817B7C4BD6335349B26045F",
    ["percent_finished"] = 0.56,
    ["doc_path"] = "/example/external/old-reading-copy.pdf",
    ["note"] = [=[Long text with [[nested delimiter]] and } text.]=],
};
"#;

    fn article(bytes: usize) -> Vec<u8> {
        (0..bytes)
            .map(|index| ((index * 17 + (index >> 8) * 11 + 13) % 256) as u8)
            .collect()
    }

    #[test]
    fn parses_fixed_progress_and_checks_edition_before_converting_page() {
        let sidecar = parse(FIXED).unwrap();
        let digest = partial_md5(&mut Cursor::new(article(4321))).unwrap();
        assert_eq!(sidecar.last_page, Some(14));
        assert_eq!(sidecar.doc_pages, Some(25));
        assert_eq!(sidecar.percent_finished, Some(0.56));
        assert_eq!(sidecar.resolve_fixed(digest, 25), Ok(13));
        assert_eq!(
            sidecar.resolve_fixed(digest, 24),
            Err(SidecarError::PageCountMismatch)
        );
        let mut new_edition = article(4321);
        new_edition[7] ^= 0x80;
        let other_digest = partial_md5(&mut Cursor::new(new_edition)).unwrap();
        assert_eq!(
            sidecar.resolve_fixed(other_digest, 25),
            Err(SidecarError::ChecksumMismatch)
        );
        assert!(!format!("{sidecar:?}").contains("external"));
        let out_of_bounds = SidecarProgress {
            last_page: Some(26),
            ..sidecar
        };
        assert_eq!(
            out_of_bounds.resolve_fixed(digest, 25),
            Err(SidecarError::PageOutOfBounds)
        );
    }

    #[test]
    fn sampled_md5_matches_independent_boundary_vectors_and_restores_position() {
        // Independently computed with hashlib from the listed KOReader offsets.
        for (size, expected) in [
            (0, "d41d8cd98f00b204e9800998ecf8427e"),
            (1024, "4eedbe8241eaec586b0b23ce0bed782e"),
            (1025, "685cf19e31bb3a83e747c7e88e4b7e96"),
            (4096, "eb7269304cbdc5f2a423327e8a7a5df0"),
            (4097, "2ab8fd0af4636d7998dc50e4347f7f05"),
            (66000, "f79353685b59c2c22bde3776299e11ef"),
        ] {
            let mut file = Cursor::new(article(size));
            file.set_position(2);
            assert_eq!(
                partial_md5(&mut file).unwrap(),
                parse_digest(expected.as_bytes()).unwrap()
            );
            assert_eq!(file.position(), 2);
        }
    }

    #[test]
    fn retains_reflowable_pointer_and_fraction_for_preview() {
        let metadata = br#"
            --[=[comments can contain } and quotations]=]
            return {
                last_xpointer = '\x2fbody/DocFragment[1]/body/p[2]/text().3',
                cre_dom_version = 20240114,
                last_percent = 2.5e-1,
                last_page = 7 -- stale fixed-page field
            }
        "#;
        let sidecar = parse(metadata).unwrap();
        assert_eq!(
            sidecar.last_xpointer.as_deref(),
            Some("/body/DocFragment[1]/body/p[2]/text().3")
        );
        assert_eq!(sidecar.cre_dom_version, Some(20240114));
        assert_eq!(sidecar.percent_finished, Some(0.25));
        assert_eq!(
            sidecar.resolve_fixed([0; 16], 7),
            Err(SidecarError::ReflowablePointer)
        );
        assert_eq!(
            parse(b"return {last_page=2}")
                .unwrap()
                .resolve_fixed([0; 16], 25),
            Err(SidecarError::MissingChecksum)
        );
        let sidecar = SidecarProgress {
            partial_md5_checksum: Some([0; 16]),
            ..parse(b"return {last_page=2}").unwrap()
        };
        assert_eq!(
            sidecar.resolve_fixed([0; 16], 25),
            Err(SidecarError::MissingPageCount)
        );
    }

    #[test]
    fn cfi_side_bias_does_not_make_the_same_source_position_disagree() {
        let mut book =
            crate::epub::Book::open(&crate::test_fixture("field-notes-epub3.epub")).unwrap();
        let cfi =
            crate::epub_cfi::resolve(&mut book, "epubcfi(/6/2!/4/20[tide-b]/1:4[;s=b])").unwrap();
        let mut pointer = cfi.clone();
        pointer.anchor.side_bias = Some(crate::SideBias::After);
        assert!(same_reflow_target(&cfi, &pointer));

        pointer.anchor.offset += 1;
        assert!(!same_reflow_target(&cfi, &pointer));
    }

    #[test]
    fn rejects_executable_syntax_duplicates_and_invalid_pointers() {
        for bytes in [
            b"local k=1; return {last_page=k}".as_slice(),
            b"return (function() return {last_page=1} end)()",
            b"return { last_page=1, other=helper() }",
            b"return { last_page=1, other=1+2 }",
            b"return { last_page=1, function=3 }",
            b"return { last_page=1 }; print('fixture')",
            b"return { other = true }",
        ] {
            assert_eq!(parse(bytes), Err(SidecarError::InvalidData));
        }
        assert_eq!(
            parse(b"return {last_page=1,[\"last\\095page\"]=2}"),
            Err(SidecarError::DuplicateField("last_page"))
        );
        assert_eq!(
            parse(b"return {last_page=1.2}"),
            Err(SidecarError::InvalidField("last_page"))
        );
        assert_eq!(
            parse(b"return {last_xpointer='/body/\xff'}"),
            Err(SidecarError::InvalidField("last_xpointer"))
        );
        assert_eq!(
            parse(b"return {partial_md5_checksum='short'}"),
            Err(SidecarError::InvalidField("partial_md5_checksum"))
        );
    }

    #[cfg(feature = "kindle")]
    #[test]
    fn retained_cfi_requires_the_same_sampled_fingerprint() {
        let parsed = parse(
            br#"return {
            last_xpointer = "/body/DocFragment[1]/body/p/text().0",
            last_epubcfi = "epubcfi(/6/2!/4/2/1:0)",
            partial_md5_checksum = "01010101010101010101010101010101",
        }"#,
        )
        .unwrap();
        assert_eq!(parsed.check_fingerprint([1; 16]), Ok(()));
        assert_eq!(
            parsed.last_epubcfi.as_deref().unwrap(),
            "epubcfi(/6/2!/4/2/1:0)"
        );
        assert_eq!(
            parsed.check_fingerprint([2; 16]),
            Err(SidecarError::ChecksumMismatch)
        );
        assert_eq!(
            parsed.resolve_fixed([1; 16], 10),
            Err(SidecarError::ReflowablePointer)
        );
    }

    #[test]
    fn rejects_oversized_or_deeply_nested_tables() {
        assert_eq!(
            parse(&vec![b'x'; MAX_SIDECAR_BYTES + 1]),
            Err(SidecarError::TooLarge)
        );
        let deep = format!(
            "return {{last_page=1, deep={}{}{}}}",
            "{".repeat(MAX_DEPTH),
            "true",
            "}".repeat(MAX_DEPTH)
        );
        assert_eq!(parse(deep.as_bytes()), Err(SidecarError::TooLarge));
        let wide = format!(
            "return {{last_page=1, many={{{}}}}}",
            "nil,".repeat(MAX_FIELDS)
        );
        assert_eq!(parse(wide.as_bytes()), Err(SidecarError::TooLarge));
    }
}
