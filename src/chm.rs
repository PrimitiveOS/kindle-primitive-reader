//! Static CHM help topics, with lazy reads from a bounded, local archive.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;

use encoding_rs::{Encoding, UTF_8, WINDOWS_1252};
use html5ever::tokenizer::states::RawKind;
use html5ever::tokenizer::{
    BufferQueue, Tag, TagKind, Token, TokenSink, TokenSinkResult, Tokenizer,
};
use libchm::{ChmError, ChmFile, Entry, EntrySel};
use roxmltree::{Document, Node, ParsingOptions};

use crate::Result;
use crate::epub::{Book, Chapter, Destination, TocEntry, is_archive_path};

pub(crate) const MAX_CHM_BYTES: u64 = 32 * 1024 * 1024;
const MAX_TOPIC_BYTES: usize = 512 * 1024;
const MAX_ASSET_BYTES: usize = 4 * 1024 * 1024;
const MAX_OUTPUT: usize = 1024 * 1024;
const MAX_TOPICS: usize = 1024;
const MAX_TOC: usize = 2048;
const MAX_DEPTH: usize = 48;
const MAX_REFERENCE: usize = 512;
const SYSTEM_BYTES: usize = 64 * 1024;
const XHTML_START: &str = "<html xmlns=\"http://www.w3.org/1999/xhtml\"><head></head><body>";

pub(crate) struct Chm {
    pub title: Option<String>,
    pub chapters: Vec<Chapter>,
    pub toc: Vec<TocEntry>,
    archive: ChmFile,
    members: BTreeMap<String, Entry>,
    encoding: &'static Encoding,
    alternate_topics: Option<BTreeSet<String>>,
}

impl Chm {
    pub(crate) fn open(file: File) -> Result<Self> {
        let mut archive = ChmFile::open_file(file)?;
        let system = match archive.find("/#SYSTEM") {
            Ok(entry) => system_records(&read_limited(&mut archive, &entry, SYSTEM_BYTES)?)?,
            Err(ChmError::NotFound(_)) => System::default(),
            Err(error) => return Err(error.into()),
        };
        let mut members = BTreeMap::new();
        for entry in archive.entries(EntrySel::NORMAL | EntrySel::FILES)? {
            let Some(path) = entry
                .path
                .strip_prefix('/')
                .filter(|path| is_archive_path(path))
            else {
                continue;
            };
            if members.insert(path.to_ascii_lowercase(), entry).is_some() {
                return Err("The CHM directory contains ambiguous case-insensitive paths".into());
            }
        }
        let mut topics: BTreeSet<_> = members
            .keys()
            .filter(|path| is_topic(path))
            .cloned()
            .collect();
        let alternate = topics.is_empty();
        if alternate {
            topics = members
                .keys()
                .filter(|path| is_alternate_topic(path))
                .cloned()
                .collect();
        }
        if topics.is_empty() || topics.len() > MAX_TOPICS {
            return Err("The CHM file has no readable topics or exceeds the topic limit".into());
        }
        let encoding = system_encoding(system.locale)
            .or_else(|| system.font_charset.and_then(font_encoding))
            .unwrap_or(WINDOWS_1252);
        let title = system
            .title
            .as_deref()
            .map(|bytes| decode(bytes, encoding))
            .transpose()?
            .and_then(|text| clean_label(&text));
        let contents = system
            .contents
            .as_deref()
            .map(|bytes| decode(bytes, encoding))
            .transpose()?
            .and_then(|path| destination("index.html", &path).map(|target| target.path))
            .or_else(|| {
                let mut matches = members.keys().filter(|path| path.ends_with(".hhc"));
                let only = matches.next()?;
                matches.next().is_none().then(|| only.clone())
            });
        let mut toc = if let Some((path, entry)) = contents
            .as_ref()
            .and_then(|path| members.get_key_value(path))
        {
            let input = decode_html(
                &read_limited(&mut archive, entry, MAX_TOPIC_BYTES)?,
                encoding,
            )?;
            read_toc(&input, path, &topics)?
        } else {
            Vec::new()
        };
        let home = system
            .home
            .as_deref()
            .map(|bytes| decode(bytes, encoding))
            .transpose()?
            .and_then(|path| destination("index.html", &path).map(|target| target.path))
            .filter(|path| topics.contains(path));
        if alternate {
            let referenced: BTreeSet<_> = home
                .iter()
                .chain(
                    toc.iter()
                        .filter_map(|item| item.destination.as_ref().map(|target| &target.path)),
                )
                .cloned()
                .collect();
            topics.retain(|path| referenced.contains(path));
            if topics.is_empty() {
                return Err("The CHM has no metadata-referenced readable topics".into());
            }
        }
        let mut added = BTreeSet::new();
        let mut chapters = Vec::with_capacity(topics.len());
        // The declared home topic opens first. Repeated TOC references still share one chapter.
        let order = home
            .iter()
            .chain(
                toc.iter()
                    .filter_map(|item| item.destination.as_ref().map(|target| &target.path)),
            )
            .chain(topics.iter());
        let labels: BTreeMap<_, _> = toc
            .iter()
            .rev()
            .filter_map(|item| {
                item.destination
                    .as_ref()
                    .map(|target| (target.path.as_str(), item.label.as_str()))
            })
            .collect();
        for path in order {
            if added.insert(path) {
                chapters.push(Chapter {
                    nonlinear: false,
                    href: path.clone(),
                    label: labels
                        .get(path.as_str())
                        .map_or_else(|| topic_name(path), |name| (*name).to_owned()),
                    is_svg: false,
                });
            }
        }
        if toc.is_empty() {
            toc = chapters
                .iter()
                .map(|chapter| TocEntry {
                    label: chapter.label.clone(),
                    depth: 0,
                    destination: Some(Destination {
                        path: chapter.href.clone(),
                        fragment: None,
                    }),
                })
                .collect();
        }
        Ok(Self {
            title,
            chapters,
            toc,
            archive,
            members,
            encoding,
            alternate_topics: alternate.then_some(topics),
        })
    }

    pub(crate) fn chapter(&mut self, path: &str) -> Result<String> {
        let entry = self
            .members
            .get(path)
            .filter(|_| {
                is_topic(path)
                    || self
                        .alternate_topics
                        .as_ref()
                        .is_some_and(|topics| topics.contains(path))
            })
            .ok_or("The CHM topic is unavailable")?;
        let raw = read_limited(&mut self.archive, entry, MAX_TOPIC_BYTES)?;
        let input = decode_html(&raw, self.encoding)?;
        if let Some(topics) = &self.alternate_topics {
            read_topic_allowed(&input, path, &self.members, Some(topics))
        } else {
            read_topic(&input, path, &self.members)
        }
    }

    #[cfg(feature = "kindle")]
    pub(crate) fn chapter_length(&self, path: &str) -> Result<u64> {
        self.members
            .get(path)
            .filter(|_| {
                is_topic(path)
                    || self
                        .alternate_topics
                        .as_ref()
                        .is_some_and(|topics| topics.contains(path))
            })
            .map(|entry| entry.length)
            .ok_or_else(|| "The CHM topic is unavailable".into())
    }

    pub(crate) fn resource_limited(&mut self, path: &str, cap: usize) -> Result<Vec<u8>> {
        let entry = self
            .members
            .get(path)
            .filter(|_| {
                is_image(path) || matches!(extension(path), "bmp" | "tif" | "tiff" | "webp")
            })
            .ok_or("The CHM resource is unavailable")?;
        let bytes = read_limited(&mut self.archive, entry, cap.min(MAX_ASSET_BYTES))?;
        let expected = if path.ends_with(".png") {
            bytes.starts_with(b"\x89PNG\r\n\x1a\n")
        } else if path.ends_with(".gif") {
            crate::gif_image::has_signature(&bytes)
        } else if path.ends_with(".bmp") {
            bytes.starts_with(b"BM")
        } else if matches!(extension(path), "tif" | "tiff") {
            bytes.starts_with(b"II*\0") || bytes.starts_with(b"MM\0*")
        } else if path.ends_with(".webp") {
            bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP")
        } else {
            bytes.starts_with(b"\xff\xd8\xff")
        };
        if !expected {
            return Err("The CHM resource is not a supported raster image".into());
        }
        if path.ends_with(".bmp") {
            let reader = crate::image_reader(&bytes)?;
            if reader.format() != Some(image::ImageFormat::Bmp) {
                return Err("The CHM bitmap has an invalid format".into());
            }
            let (width, height) = reader.into_dimensions()?;
            if !crate::comic::image_dimensions_valid(width, height) {
                return Err("The CHM bitmap exceeds Reader's decoded image limits".into());
            }
        }
        if matches!(extension(path), "tif" | "tiff") {
            crate::tiff_image::decode(&bytes)?;
        } else if extension(path) == "webp" {
            let png = crate::comic::ImagePage::webp_as_png(&bytes)?;
            if png.len() as u64 > crate::comic::MAX_WEBP_BYTES {
                return Err("The converted CHM WebP exceeds its image limit".into());
            }
        }
        Ok(bytes)
    }
}

fn read_limited(archive: &mut ChmFile, entry: &Entry, cap: usize) -> Result<Vec<u8>> {
    if entry.length > cap as u64 {
        return Err("The CHM member exceeds its read limit".into());
    }
    Ok(archive.read(entry)?)
}

fn extension(path: &str) -> &str {
    path.rsplit('/')
        .next()
        .unwrap_or_default()
        .rsplit_once('.')
        .map_or("", |(_, ext)| ext)
}

fn is_topic(path: &str) -> bool {
    matches!(extension(path), "html" | "htm" | "xhtml")
}

fn is_alternate_topic(path: &str) -> bool {
    matches!(extension(path), "asp" | "aspx" | "shtml" | "xml")
}

fn is_image(path: &str) -> bool {
    matches!(extension(path), "png" | "jpg" | "jpeg" | "gif")
}

fn topic_name(path: &str) -> String {
    path.rsplit('/')
        .next()
        .unwrap_or(path)
        .rsplit_once('.')
        .map_or(path, |(name, _)| name)
        .chars()
        .take(160)
        .collect()
}

#[derive(Default)]
struct System {
    contents: Option<Vec<u8>>,
    home: Option<Vec<u8>>,
    title: Option<Vec<u8>>,
    locale: Option<u32>,
    font_charset: Option<u8>,
}

// #SYSTEM records have a 32-bit version followed by 16-bit type/length pairs.
// https://git.tartarus.org/?p=simon/halibut.git;a=blob;f=chm.c
fn system_records(bytes: &[u8]) -> Result<System> {
    let mut remaining = bytes.get(4..).ok_or("Invalid CHM system metadata")?;
    let mut output = System::default();
    while !remaining.is_empty() {
        let (header, tail) = remaining
            .split_first_chunk::<4>()
            .ok_or("Invalid CHM system record")?;
        let code = u16::from_le_bytes([header[0], header[1]]);
        let size = usize::from(u16::from_le_bytes([header[2], header[3]]));
        let (value, next) = tail
            .split_at_checked(size)
            .ok_or("Invalid CHM system record size")?;
        let data = || value.split(|byte| *byte == 0).next().unwrap_or_default();
        match code {
            0 | 2 | 3 if data().len() <= MAX_REFERENCE => {
                let slot = match code {
                    0 => &mut output.contents,
                    2 => &mut output.home,
                    _ => &mut output.title,
                };
                if slot.is_none() {
                    *slot = Some(data().to_vec());
                }
            }
            4 if value.len() >= 4 => {
                output.locale = value.first_chunk().copied().map(u32::from_le_bytes)
            }
            16 if data().len() <= MAX_REFERENCE => {
                output.font_charset = data()
                    .iter()
                    .rposition(|&byte| byte == b',')
                    .and_then(|comma| std::str::from_utf8(&data()[comma + 1..]).ok())
                    .and_then(|number| number.trim().parse().ok());
            }
            _ => {}
        }
        remaining = next;
    }
    Ok(output)
}

fn system_encoding(locale: Option<u32>) -> Option<&'static Encoding> {
    Some(match locale? & 0x3ff {
        0x04 if matches!(locale, Some(0x0404 | 0x0c04 | 0x1404)) => encoding_rs::BIG5,
        0x04 => encoding_rs::GBK,
        0x11 => encoding_rs::SHIFT_JIS,
        0x12 => encoding_rs::EUC_KR,
        0x02 | 0x19 | 0x22 | 0x23 => encoding_rs::WINDOWS_1251,
        0x05 | 0x0e | 0x15 | 0x18 | 0x1a | 0x1b | 0x24 => encoding_rs::WINDOWS_1250,
        0x08 => encoding_rs::WINDOWS_1253,
        0x1f => encoding_rs::WINDOWS_1254,
        0x0d => encoding_rs::WINDOWS_1255,
        0x01 | 0x29 => encoding_rs::WINDOWS_1256,
        0x25..=0x27 => encoding_rs::WINDOWS_1257,
        0x2a => encoding_rs::WINDOWS_1258,
        0x1e => encoding_rs::WINDOWS_874,
        0x03 | 0x06 | 0x07 | 0x09 | 0x0a | 0x0b | 0x0c | 0x0f | 0x10 | 0x13 | 0x14 | 0x16
        | 0x1d | 0x21 | 0x2d | 0x36 => WINDOWS_1252,
        _ => return None,
    })
}

fn font_encoding(charset: u8) -> Option<&'static Encoding> {
    Some(match charset {
        0 => WINDOWS_1252,
        204 => encoding_rs::WINDOWS_1251,
        238 => encoding_rs::WINDOWS_1250,
        161 => encoding_rs::WINDOWS_1253,
        162 => encoding_rs::WINDOWS_1254,
        186 => encoding_rs::WINDOWS_1257,
        177 => encoding_rs::WINDOWS_1255,
        178 => encoding_rs::WINDOWS_1256,
        _ => return None,
    })
}

fn decode(input: &[u8], fallback: &'static Encoding) -> Result<String> {
    let (encoding, bytes) = if let Some((encoding, start)) = Encoding::for_bom(input) {
        (encoding, &input[start..])
    } else if std::str::from_utf8(input).is_ok() {
        (UTF_8, input)
    } else {
        (fallback, input)
    };
    let (text, errors) = encoding.decode_without_bom_handling(bytes);
    if errors || text.len() > MAX_OUTPUT {
        return Err("The CHM text encoding or decoded size is unsupported".into());
    }
    Ok(text.into_owned())
}

fn decode_html(input: &[u8], fallback: &'static Encoding) -> Result<String> {
    if Encoding::for_bom(input).is_some() {
        return decode(input, fallback);
    }
    if let Some(encoding) = html_encoding(input) {
        let (text, errors) = encoding.decode_without_bom_handling(input);
        if errors || text.len() > MAX_OUTPUT {
            return Err("The CHM HTML encoding or decoded size is unsupported".into());
        }
        return Ok(text.into_owned());
    }
    decode(input, fallback)
}

// Only scan the bounded HTML prefix and only inspect actual meta tags, not
// text inside comments or script. The archive locale remains the fallback.
pub(crate) fn html_encoding(input: &[u8]) -> Option<&'static Encoding> {
    let prefix = String::from_utf8_lossy(&input[..input.len().min(1024)]);
    let sink = tokenize(&prefix, CharsetSink(RefCell::new((None, false))));
    sink.0.into_inner().0
}

struct CharsetSink(RefCell<(Option<&'static Encoding>, bool)>);

impl TokenSink for CharsetSink {
    type Handle = ();

    fn process_token(&self, token: Token, _: u64) -> TokenSinkResult<()> {
        let Token::TagToken(tag) = token else {
            return TokenSinkResult::Continue;
        };
        if tag.kind != TagKind::StartTag {
            return TokenSinkResult::Continue;
        }
        let mut state = self.0.borrow_mut();
        let name: &str = tag.name.as_ref();
        if name == "body" {
            state.1 = true;
        }
        if !state.1 && state.0.is_none() && name == "meta" {
            let label = attr(&tag, "charset").or_else(|| {
                attr(&tag, "http-equiv")
                    .filter(|value| value.eq_ignore_ascii_case("content-type"))
                    .and_then(|_| attr(&tag, "content"))
                    .and_then(content_type_charset)
            });
            state.0 = label
                .and_then(|label| Encoding::for_label(label.as_bytes()))
                .map(|encoding| {
                    if encoding == encoding_rs::UTF_16LE || encoding == encoding_rs::UTF_16BE {
                        UTF_8
                    } else {
                        encoding
                    }
                });
        }
        match name {
            "script" => TokenSinkResult::RawData(RawKind::ScriptData),
            "style" => TokenSinkResult::RawData(RawKind::Rawtext),
            _ => TokenSinkResult::Continue,
        }
    }
}

fn content_type_charset(content: &str) -> Option<&str> {
    content.split(';').find_map(|parameter| {
        let (name, value) = parameter.split_once('=')?;
        name.trim()
            .eq_ignore_ascii_case("charset")
            .then(|| value.trim().trim_matches(['\'', '"']))
    })
}

fn clean_label(text: &str) -> Option<String> {
    let filtered: String = text
        .chars()
        .filter(|ch| !ch.is_control())
        .take(160)
        .collect();
    let text = filtered.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

fn destination(base: &str, text: &str) -> Option<Destination> {
    let text = text.trim();
    if text.is_empty()
        || text.len() > MAX_REFERENCE
        || text.contains([':', '?'])
        || text.starts_with("//")
    {
        return None;
    }
    let text = text.replace('\\', "/");
    let mut target = if let Some(path) = text.strip_prefix('/') {
        Book::resolve_link("index.html", path)
    } else {
        Book::resolve_link(base, &text)
    }
    .ok()?;
    target.path.make_ascii_lowercase();
    Some(target)
}

fn reference(base: &str, target: &Destination) -> String {
    let mut value = String::new();
    if target.path != base || target.fragment.is_none() {
        if let Some((parent, _)) = base.rsplit_once('/') {
            for _ in parent.split('/') {
                value.push_str("../");
            }
        }
        uri_escape(&mut value, &target.path, true);
    }
    if let Some(fragment) = &target.fragment {
        value.push('#');
        uri_escape(&mut value, fragment, false);
    }
    value
}

fn uri_escape(output: &mut String, text: &str, path: bool) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric()
            || matches!(byte, b'.' | b'_' | b'-' | b'~')
            || (path && byte == b'/')
        {
            output.push(char::from(byte));
        } else {
            output.push('%');
            output.push(char::from(HEX[usize::from(byte >> 4)]));
            output.push(char::from(HEX[usize::from(byte & 15)]));
        }
    }
}

fn tokenize<S: TokenSink>(input: &str, sink: S) -> S {
    let buffer = BufferQueue::default();
    buffer.push_back(input.into());
    let parser = Tokenizer::new(sink, Default::default());
    let _ = parser.feed(&buffer);
    parser.end();
    parser.sink
}

fn attr<'a>(tag: &'a Tag, name: &str) -> Option<&'a str> {
    tag.attrs
        .iter()
        .find(|item| &*item.name.local == name)
        .map(|item| item.value.trim())
        .filter(|value| !value.is_empty() && value.len() <= MAX_REFERENCE)
}

#[derive(Default)]
struct SitemapItem {
    label: Option<String>,
    local: Option<String>,
}

#[derive(Default)]
struct Contents {
    item: Option<SitemapItem>,
    entries: Vec<(usize, SitemapItem)>,
    depth: usize,
    invalid: bool,
}

struct TocSink(RefCell<Contents>);

impl TokenSink for TocSink {
    type Handle = ();

    fn process_token(&self, token: Token, _: u64) -> TokenSinkResult<()> {
        let Token::TagToken(tag) = token else {
            return TokenSinkResult::Continue;
        };
        let mut state = self.0.borrow_mut();
        if state.invalid {
            return TokenSinkResult::Continue;
        }
        match (tag.name.as_ref(), tag.kind) {
            ("ul", TagKind::StartTag) => {
                state.depth += 1;
                state.invalid |= state.depth > 24;
            }
            ("ul", TagKind::EndTag) => state.depth = state.depth.saturating_sub(1),
            ("object", TagKind::StartTag)
                if attr(&tag, "type")
                    .is_some_and(|value| value.eq_ignore_ascii_case("text/sitemap")) =>
            {
                state.item = Some(SitemapItem::default());
            }
            ("object", TagKind::EndTag) => {
                if let Some(item) = state.item.take() {
                    let depth = state.depth.saturating_sub(1);
                    state.entries.push((depth, item));
                    state.invalid |= state.entries.len() > MAX_TOC;
                }
            }
            ("param", TagKind::StartTag) => {
                if let (Some(item), Some(name), Some(value)) =
                    (&mut state.item, attr(&tag, "name"), attr(&tag, "value"))
                {
                    if name.eq_ignore_ascii_case("name") && item.label.is_none() {
                        item.label = clean_label(value);
                    }
                    if name.eq_ignore_ascii_case("local") && item.local.is_none() {
                        item.local = Some(value.to_owned());
                    }
                }
            }
            _ => {}
        }
        TokenSinkResult::Continue
    }
}

fn read_toc(input: &str, base: &str, topics: &BTreeSet<String>) -> Result<Vec<TocEntry>> {
    let state = tokenize(input, TocSink(RefCell::new(Contents::default())))
        .0
        .into_inner();
    if state.invalid {
        return Err("The CHM table of contents exceeds its limits".into());
    }
    Ok(state
        .entries
        .into_iter()
        .filter_map(|(depth, item)| {
            Some(TocEntry {
                label: item.label?,
                depth,
                destination: item
                    .local
                    .as_deref()
                    .and_then(|link| destination(base, link))
                    .filter(|target| topics.contains(&target.path)),
            })
        })
        .collect())
}

struct Topic<'a> {
    base: &'a str,
    members: &'a BTreeMap<String, Entry>,
    xhtml: String,
    stack: Vec<&'static str>,
    raw: Option<&'static str>,
    in_head: bool,
    invalid: bool,
    content: bool,
    image_mode: u8,
    alternate_topics: Option<&'a BTreeSet<String>>,
}

struct TopicSink<'a>(RefCell<Topic<'a>>);

impl TokenSink for TopicSink<'_> {
    type Handle = ();

    fn process_token(&self, token: Token, _: u64) -> TokenSinkResult<()> {
        let mut topic = self.0.borrow_mut();
        if topic.invalid {
            return TokenSinkResult::Continue;
        }
        let result = match token {
            Token::TagToken(tag) => topic.tag(tag),
            Token::CharacterTokens(text) if !topic.in_head && topic.raw.is_none() => {
                topic.content |= text
                    .chars()
                    .any(|ch| !ch.is_whitespace() && !ch.is_control());
                xml(&mut topic.xhtml, &text);
                TokenSinkResult::Continue
            }
            _ => TokenSinkResult::Continue,
        };
        topic.invalid |= topic.xhtml.len() > MAX_OUTPUT;
        result
    }
}

impl Topic<'_> {
    fn tag(&mut self, tag: Tag) -> TokenSinkResult<()> {
        let name = tag.name.as_ref();
        let start = tag.kind == TagKind::StartTag;
        if let Some(raw) = self.raw {
            if !start && name == raw {
                self.raw = None;
            }
            return TokenSinkResult::Continue;
        }
        if start
            && let Some((name, state)) = match name {
                "script" => Some(("script", RawKind::ScriptData)),
                "style" => Some(("style", RawKind::Rawtext)),
                "iframe" => Some(("iframe", RawKind::Rawtext)),
                "title" => Some(("title", RawKind::Rcdata)),
                _ => None,
            }
        {
            self.raw = Some(name);
            return TokenSinkResult::RawData(state);
        }
        if name == "head" {
            self.in_head = start;
        }
        if name == "body" {
            self.in_head = false;
        }
        if self.in_head {
            return TokenSinkResult::Continue;
        }
        let Some(name) = safe_tag(name) else {
            return TokenSinkResult::Continue;
        };
        if !start {
            self.close(name);
            return TokenSinkResult::Continue;
        }
        if matches!(
            name,
            "p" | "div"
                | "h1"
                | "h2"
                | "h3"
                | "h4"
                | "h5"
                | "h6"
                | "hr"
                | "ul"
                | "ol"
                | "dl"
                | "table"
                | "blockquote"
                | "pre"
        ) {
            self.close("p");
        }
        if matches!(name, "li" | "dt" | "dd" | "td" | "th" | "tr" | "a")
            && self.stack.last() == Some(&name)
        {
            self.close(name);
        }
        if self.stack.len() >= MAX_DEPTH {
            self.invalid = true;
            return TokenSinkResult::Continue;
        }
        let src = if name == "img" {
            let src = attr(&tag, "src")
                .and_then(|value| destination(self.base, value))
                .filter(|target| {
                    (is_image(&target.path)
                        || (self.image_mode >= 1 && extension(&target.path) == "bmp")
                        || (self.image_mode >= 2
                            && matches!(extension(&target.path), "tif" | "tiff" | "webp")))
                        && target.fragment.is_none()
                        && self.members.contains_key(&target.path)
                });
            if src.is_none() {
                return TokenSinkResult::Continue;
            }
            self.content = true;
            src
        } else {
            None
        };
        self.xhtml.push('<');
        self.xhtml.push_str(name);
        if let Some(id) =
            attr(&tag, "id").or_else(|| (name == "a").then(|| attr(&tag, "name")).flatten())
        {
            attribute(&mut self.xhtml, "id", id);
        }
        for key in ["lang", "dir"] {
            if let Some(value) = attr(&tag, key) {
                attribute(&mut self.xhtml, key, value);
            }
        }
        if name == "a"
            && let Some(target) = attr(&tag, "href").and_then(|input| destination(self.base, input))
            && (is_topic(&target.path)
                || self
                    .alternate_topics
                    .is_some_and(|topics| topics.contains(&target.path)))
            && self.members.contains_key(&target.path)
        {
            attribute(&mut self.xhtml, "href", &reference(self.base, &target));
        }
        if let Some(src) = src {
            attribute(&mut self.xhtml, "src", &reference(self.base, &src));
            if let Some(alt) = attr(&tag, "alt") {
                attribute(&mut self.xhtml, "alt", alt);
            }
        }
        if matches!(name, "img" | "br" | "hr") || tag.self_closing {
            self.xhtml.push_str(" />");
        } else {
            self.xhtml.push('>');
            self.stack.push(name);
        }
        TokenSinkResult::Continue
    }

    fn close(&mut self, name: &str) {
        if let Some(index) = self.stack.iter().rposition(|open| *open == name) {
            self.close_to(index);
        }
    }

    fn close_to(&mut self, index: usize) {
        while self.stack.len() > index {
            self.xhtml.push_str("</");
            self.xhtml.push_str(self.stack.pop().unwrap());
            self.xhtml.push('>');
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
        "img",
        "kbd",
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
        "center" | "section" | "article" => Some("div"),
        _ => TAGS.iter().copied().find(|tag| *tag == name),
    }
}

#[derive(Debug)]
struct EmptyTopic;

impl std::fmt::Display for EmptyTopic {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("The CHM topic is empty or exceeds Reader's layout limits")
    }
}

impl std::error::Error for EmptyTopic {}

fn read_topic(input: &str, base: &str, members: &BTreeMap<String, Entry>) -> Result<String> {
    read_topic_allowed(input, base, members, None)
}

fn read_topic_allowed(
    input: &str,
    base: &str,
    members: &BTreeMap<String, Entry>,
    alternate_topics: Option<&BTreeSet<String>>,
) -> Result<String> {
    match read_topic_mode(input, base, members, alternate_topics, 0) {
        Err(error) if error.is::<EmptyTopic>() => {
            match read_topic_mode(input, base, members, alternate_topics, 1) {
                Err(error) if error.is::<EmptyTopic>() => {
                    read_topic_mode(input, base, members, alternate_topics, 2)
                }
                result => result,
            }
        }
        result => result,
    }
}

fn read_topic_mode(
    input: &str,
    base: &str,
    members: &BTreeMap<String, Entry>,
    alternate_topics: Option<&BTreeSet<String>>,
    image_mode: u8,
) -> Result<String> {
    let topic = Topic {
        base,
        members,
        xhtml: String::from(XHTML_START),
        stack: Vec::new(),
        raw: None,
        in_head: false,
        invalid: false,
        content: false,
        image_mode,
        alternate_topics,
    };
    let mut topic = tokenize(input, TopicSink(RefCell::new(topic)))
        .0
        .into_inner();
    topic.close_to(0);
    topic.xhtml.push_str("</body></html>");
    if topic.invalid || topic.xhtml.len() > MAX_OUTPUT {
        return Err("The CHM topic is empty or exceeds Reader's layout limits".into());
    }
    if !topic.content {
        return Err(Box::new(EmptyTopic));
    }
    attach_inline_runs(topic.xhtml)
}

// Put the introductory text of a nested list item, cell, or mixed division in
// its own leaf block. Preserve the authored container, links and inline markup.
fn attach_inline_runs(xhtml: String) -> Result<String> {
    let markup = Document::parse_with_options(
        &xhtml,
        ParsingOptions {
            nodes_limit: 65_536,
            ..ParsingOptions::default()
        },
    )?;
    let body = markup
        .root_element()
        .last_element_child()
        .ok_or("The CHM topic has no body")?;
    let mut has_blocks = BTreeSet::new();
    for node in body.descendants().filter(|node| is_reflow_block(*node)) {
        for parent in node
            .ancestors()
            .skip(1)
            .take_while(|parent| parent.is_element())
        {
            if !has_blocks.insert(parent.range().start) {
                break;
            }
        }
    }
    let mut insertions = Vec::new();
    for container in body
        .descendants()
        .filter(|node| node.is_element() && has_blocks.contains(&node.range().start))
    {
        let (open, close) = match container.tag_name().name() {
            "ul" | "ol" => ("<li>", "</li>"),
            "dl" => ("<dd>", "</dd>"),
            "table" => ("<tbody><tr><td>", "</td></tr></tbody>"),
            "tbody" | "thead" => ("<tr><td>", "</td></tr>"),
            "tr" => ("<td>", "</td>"),
            _ => ("<p>", "</p>"),
        };
        let mut run = None::<(usize, usize, bool)>;
        let mut finish_run = |run: &mut Option<(usize, usize, bool)>| {
            if let Some((start, end, true)) = run.take() {
                insertions.push((start, open));
                insertions.push((end, close));
            }
        };
        for child in container.children() {
            let range = child.range();
            if child.is_element() && (is_reflow_block(child) || has_blocks.contains(&range.start)) {
                finish_run(&mut run);
                continue;
            }
            let visible = child.descendants().any(|node| {
                node.has_tag_name("img")
                    || (node.is_text() && node.text().is_some_and(|value| !value.trim().is_empty()))
            });
            let (_, end, has_content) = run.get_or_insert((range.start, range.start, false));
            *end = range.end;
            *has_content |= visible;
        }
        finish_run(&mut run);
    }
    if insertions.is_empty() {
        return Ok(xhtml);
    }
    let output_size = xhtml.len() + insertions.iter().map(|(_, tag)| tag.len()).sum::<usize>();
    if output_size > MAX_OUTPUT {
        return Err("The CHM topic exceeds Reader's layout limits".into());
    }
    insertions.sort_by_key(|(at, _)| *at);
    let mut normalized = String::with_capacity(output_size);
    let mut at = 0;
    for (next, tag) in insertions {
        normalized.push_str(&xhtml[at..next]);
        normalized.push_str(tag);
        at = next;
    }
    normalized.push_str(&xhtml[at..]);
    Ok(normalized)
}

fn is_reflow_block(node: Node<'_, '_>) -> bool {
    node.is_element()
        && matches!(
            node.tag_name().name(),
            "body"
                | "div"
                | "p"
                | "li"
                | "dd"
                | "dt"
                | "h1"
                | "h2"
                | "h3"
                | "h4"
                | "h5"
                | "h6"
                | "blockquote"
                | "pre"
                | "td"
                | "th"
        )
}

fn xml(output: &mut String, value: &str) {
    for char in value.chars() {
        if output.len() > MAX_OUTPUT {
            break;
        }
        match char {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            '\n' | '\r' | '\t' => output.push(char),
            ch if ch.is_control() || matches!(ch, '\u{fffe}' | '\u{ffff}') => {}
            ch => output.push(ch),
        }
    }
}

fn attribute(output: &mut String, name: &str, value: &str) {
    output.push(' ');
    output.push_str(name);
    output.push_str("=\"");
    xml(output, value);
    output.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    const PNG: &[u8] = &[
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 0,
        0, 0, 0, 58, 126, 155, 85, 0, 0, 0, 10, 73, 68, 65, 84, 120, 156, 99, 96, 0, 0, 0, 2, 0, 1,
        72, 175, 164, 113, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
    ];

    fn word(bytes: &mut [u8], at: usize, count: u32) {
        bytes[at..at + 4].copy_from_slice(&count.to_le_bytes());
    }

    fn long(bytes: &mut [u8], at: usize, count: usize) {
        bytes[at..at + 8].copy_from_slice(&(count as u64).to_le_bytes());
    }

    fn cword(data: &mut Vec<u8>, value: usize) {
        let mut bytes = [0_u8; 10];
        bytes[9] = (value & 127) as u8;
        let mut start = 9;
        let mut value = value >> 7;
        while value != 0 {
            start -= 1;
            bytes[start] = ((value & 127) as u8) | 128;
            value >>= 7;
        }
        data.extend_from_slice(&bytes[start..]);
    }

    fn archive(members: &[(&str, &[u8])]) -> Vec<u8> {
        const DIR: usize = 96;
        const HEAD: usize = 84;
        const BLOCK: usize = 4096;
        const DATA: usize = DIR + HEAD + BLOCK;
        let mut bytes = vec![0; DATA];
        bytes[..4].copy_from_slice(b"ITSF");
        word(&mut bytes, 4, 3);
        word(&mut bytes, 8, 96);
        long(&mut bytes, 0x48, DIR);
        long(&mut bytes, 0x50, HEAD + BLOCK);
        long(&mut bytes, 0x58, DATA);
        bytes[DIR..DIR + 4].copy_from_slice(b"ITSP");
        word(&mut bytes, DIR + 4, 1);
        word(&mut bytes, DIR + 8, HEAD as u32);
        word(&mut bytes, DIR + 0x10, BLOCK as u32);
        word(&mut bytes, DIR + 0x1c, u32::MAX);
        let leaf = DIR + HEAD;
        bytes[leaf..leaf + 4].copy_from_slice(b"PMGL");
        word(&mut bytes, leaf + 0x10, u32::MAX);

        let mut entries = Vec::new();
        for (name, member) in members {
            cword(&mut entries, name.len());
            entries.extend_from_slice(name.as_bytes());
            cword(&mut entries, 0);
            cword(&mut entries, bytes.len() - DATA);
            cword(&mut entries, member.len());
            bytes.extend_from_slice(member);
        }
        let free = BLOCK - 20 - entries.len();
        word(&mut bytes, leaf + 4, free as u32);
        bytes[leaf + 20..leaf + 20 + entries.len()].copy_from_slice(&entries);
        bytes
    }

    fn system_fixture() -> Vec<u8> {
        let mut bytes = 3_u32.to_le_bytes().to_vec();
        for (kind, value) in [
            (0_u16, b"Contents.HHC\0".as_slice()),
            (2, b"INDEX.HTML\0".as_slice()),
            (3, b"Caf\xe9 Guide\0".as_slice()),
        ] {
            bytes.extend_from_slice(&kind.to_le_bytes());
            bytes.extend_from_slice(&(value.len() as u16).to_le_bytes());
            bytes.extend_from_slice(value);
        }
        bytes
    }

    #[test]
    fn opens_authored_help_topics_and_assets_through_book_api() {
        let system = system_fixture();
        let bytes = archive(&[
            ("/#SYSTEM", &system),
            ("/Contents.HHC", br#"<UL><LI><OBJECT type=text/sitemap><param name=Name value="Welcome"><param name=Local value="INDEX.HTML"></OBJECT><UL><LI><OBJECT type=text/sitemap><param name=Name value="Nested topic"><param name=Local value="Topics\Second.HTM#part"></OBJECT></UL></UL>"#),
            ("/Pic.PNG", PNG),
            ("/Topics/Second.HTM", b"<HTML><BODY><H2 ID=part>Arrival</H2><p>Second topic</BODY></HTML>"),
            ("/index.html", br#"<!doctype html><HTML><HEAD><TITLE>Ignored title</TITLE></HEAD><BODY><H1>Welcome</H1><P ID=local>Local topic<P><a href='#local'>Jump</a> <a href='TOPICS/SECOND.HTM#part'>Next</a><P><IMG src='/Pic.PNG' alt='The coast' onclick='ignored'> <IMG src='https://example.invalid/pic.png'><script>ignored()</script></BODY></HTML>"#),
        ]);
        let path = std::env::temp_dir().join(format!("reader-authored-{}.chm", std::process::id()));
        let mut file = File::options()
            .create_new(true)
            .write(true)
            .open(&path)
            .unwrap();
        file.write_all(&bytes).unwrap();
        drop(file);
        let opened = Book::open(&path);
        // On Unix the already opened archive can still read through this descriptor.
        std::fs::remove_file(&path).unwrap();
        let mut book = opened.unwrap();
        assert_eq!(book.title, "Café Guide");
        assert_eq!(book.chapters[0].href, "index.html");
        assert_eq!(book.chapters[1].href, "topics/second.htm");
        assert_eq!(book.toc()[1].depth, 1);
        let second = book.chapter(1).unwrap();
        assert!(second.contains("<h2 id=\"part\">Arrival</h2>"));
        let xhtml = book.chapter(0).unwrap();
        assert!(!xhtml.contains("ignored"));
        assert!(!xhtml.contains("example.invalid"));
        let source = crate::source::Source::parse(&xhtml).unwrap();
        assert!(source.fragments.contains_key("local"));
        let links: Vec<_> = source
            .blocks
            .iter()
            .flat_map(|block| &block.links)
            .map(|link| Book::resolve_link(&book.chapters[0].href, &link.href).unwrap())
            .collect();
        assert_eq!(
            links,
            [
                Destination {
                    path: "index.html".into(),
                    fragment: Some("local".into())
                },
                Destination {
                    path: "topics/second.htm".into(),
                    fragment: Some("part".into())
                }
            ]
        );
        assert_eq!(book.toc()[1].destination.as_ref(), Some(&links[1]));
        assert_eq!(book.resource("pic.png").unwrap(), PNG);
        assert!(book.resource_limited("pic.png", PNG.len() - 1).is_err());
    }

    #[test]
    fn caps_text_expansion_and_discards_nonlocal_references() {
        let mut html = String::from("<body><p>");
        html.push_str(&"&".repeat(MAX_TOPIC_BYTES / 2));
        assert!(read_topic(&html, "index.html", &BTreeMap::new()).is_err());
        for href in [
            "../../away.html",
            "//example.invalid/p.html",
            "its:other.chm::/index.html",
            "../%2E%2E/root.html",
        ] {
            assert!(destination("sub/index.html", href).is_none());
        }
    }

    #[test]
    fn wraps_inline_runs_without_losing_nested_topic_labels() {
        let raw = r#"<body>Start<div id="outer"><em>Before list</em><ul><li id="parent"><strong>Parent</strong><ul><li id="child">Child</li></ul>After child</li></ul>After list</div>End</body>"#;
        let xhtml = read_topic(raw, "index.html", &BTreeMap::new()).unwrap();
        let source = crate::source::Source::parse(&xhtml).unwrap();
        assert_eq!(
            source
                .blocks
                .iter()
                .map(|block| block.text.as_str())
                .collect::<Vec<_>>(),
            [
                "Start",
                "Before list",
                "Parent",
                "Child",
                "After child",
                "After list",
                "End"
            ]
        );
        for id in ["outer", "parent", "child"] {
            assert!(source.fragments.contains_key(id));
        }
        assert!(xhtml.contains("<p><strong>Parent</strong></p>"));
    }
}
