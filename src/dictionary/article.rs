//! Layout metadata for the visible text of a StarDict HTML field.
use std::{collections::BTreeSet, ops::Range};

use html5ever::{
    LocalName,
    tokenizer::{Tag, TagKind},
};
use parley::{BaseDirection, Language};

const MAX_DEPTH: usize = 32;
const MAX_TRANSITIONS: usize = 1024;
const MAX_PARAGRAPHS: usize = 512;
const MAX_LINKS: usize = 64;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Article {
    pub(crate) paragraphs: Vec<Paragraph>,
    pub(crate) links: Vec<Link>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Link {
    pub(crate) range: Range<usize>,
    pub(crate) target: Option<String>,
}

impl Article {
    pub(super) fn is_empty(&self) -> bool {
        self.paragraphs.is_empty() && self.links.is_empty()
    }

    pub(super) fn append(&mut self, other: Self, offset: usize) {
        let limit = MAX_PARAGRAPHS.saturating_sub(self.paragraphs.len());
        self.paragraphs.extend(
            other
                .paragraphs
                .into_iter()
                .take(limit)
                .map(|mut paragraph| {
                    paragraph.range.start += offset;
                    paragraph.range.end += offset;
                    paragraph
                }),
        );
        let limit = MAX_LINKS.saturating_sub(self.links.len());
        self.links
            .extend(other.links.into_iter().take(limit).map(|mut link| {
                link.range.start += offset;
                link.range.end += offset;
                link
            }));
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Paragraph {
    pub(crate) range: Range<usize>,
    pub(crate) indent: u8,
    pub(crate) list_item: bool,
    pub(crate) marker: Option<Marker>,
    pub(crate) direction: Option<BaseDirection>,
    pub(crate) language: Option<Language>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Marker {
    Number(u16),
    Bullet,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct Style {
    indent: u8,
    item: Option<(usize, Marker)>,
    direction: Option<BaseDirection>,
    language: Option<Language>,
}

enum List {
    Ordered(u16),
    Unordered,
}

struct Frame {
    name: LocalName,
    style: Style,
    list: Option<List>,
}

pub(super) struct Tracker {
    root: Style,
    frames: Vec<Frame>,
    transitions: Vec<(usize, Style)>,
    next_item: usize,
    overflow: bool,
    links: Vec<Link>,
    open_link: Option<(usize, Option<String>)>,
}

pub(super) fn language(tag: &str) -> Option<Language> {
    (tag.len() <= crate::source::MAX_LANGUAGE_BYTES)
        .then(|| Language::parse(tag).ok())
        .flatten()
}

fn reference(href: &str) -> Option<String> {
    let Some(encoded) = href.strip_prefix("bword://") else {
        // KOReader treats a target without a protocol as a literal headword
        // in the dictionary that provided this definition. Keep percent
        // characters in those literal entries rather than URL-decoding them.
        let word = href.trim();
        return (!word.is_empty()
            && word.len() <= super::MAX_WORD_BYTES
            && !word.contains("://")
            && !word.chars().any(char::is_control))
        .then(|| word.to_owned());
    };
    if encoded.len() > super::MAX_WORD_BYTES * 3 {
        return None;
    }
    let mut bytes = encoded.as_bytes().iter();
    while let Some(&byte) = bytes.next() {
        if byte == b'%' && !(bytes.next()?.is_ascii_hexdigit() && bytes.next()?.is_ascii_hexdigit())
        {
            return None;
        }
    }
    let decoded = percent_encoding::percent_decode_str(encoded)
        .decode_utf8()
        .ok()?;
    if decoded.chars().any(char::is_control) {
        return None;
    }
    let word = decoded.trim();
    (!word.is_empty() && word.len() <= super::MAX_WORD_BYTES && !word.contains("://"))
        .then(|| word.to_owned())
}

impl Tracker {
    pub(super) fn new(language: Option<Language>) -> Self {
        Self {
            root: Style {
                language,
                ..Style::default()
            },
            frames: Vec::new(),
            transitions: Vec::new(),
            next_item: 0,
            overflow: false,
            links: Vec::new(),
            open_link: None,
        }
    }

    fn style(&self) -> Style {
        self.frames.last().map_or(self.root, |frame| frame.style)
    }

    pub(super) fn tag(&mut self, tag: &Tag, offset: usize) {
        if !self.overflow && &*tag.name == "a" {
            self.close_link(offset);
            if tag.kind == TagKind::StartTag
                && !tag.self_closing
                && self.links.len() < MAX_LINKS
                && let Some(href) = tag.attrs.iter().find(|attr| &*attr.name.local == "href")
            {
                self.open_link = Some((offset, reference(&href.value)));
            }
            return;
        }
        if self.overflow
            || !matches!(
                tag.name.as_ref(),
                "html"
                    | "body"
                    | "div"
                    | "section"
                    | "article"
                    | "p"
                    | "dl"
                    | "dt"
                    | "dd"
                    | "ol"
                    | "ul"
                    | "li"
                    | "blockquote"
            )
        {
            return;
        }
        let previous = self.style();
        match tag.kind {
            TagKind::StartTag if !tag.self_closing => self.open(tag),
            TagKind::EndTag => {
                if let Some(index) = self.frames.iter().rposition(|frame| frame.name == tag.name) {
                    self.frames.truncate(index);
                }
            }
            _ => return,
        }
        let style = self.style();
        if style != previous {
            if let Some(last) = self.transitions.last_mut()
                && last.0 == offset
            {
                last.1 = style;
            } else if self.transitions.len() < MAX_TRANSITIONS {
                self.transitions.push((offset, style));
            } else {
                self.overflow = true;
            }
        }
    }

    fn close_link(&mut self, end: usize) {
        if let Some((start, target)) = self.open_link.take()
            && start < end
        {
            self.links.push(Link {
                range: start..end,
                target,
            });
        }
    }

    fn open(&mut self, tag: &Tag) {
        let attr = |name: &str| {
            tag.attrs
                .iter()
                .find(|attr| &*attr.name.local == name)
                .map(|attr| attr.value.as_ref())
        };
        let number = |name| {
            attr(name)
                .and_then(|value| value.parse::<u16>().ok())
                .filter(|value| *value <= 9999)
        };
        let mut marker = None;
        if &*tag.name == "li" {
            if let Some(index) = self.frames.iter().rposition(|frame| frame.list.is_some()) {
                // HTML allows an omitted </li> before the next sibling item.
                self.frames.truncate(index + 1);
                marker = self
                    .frames
                    .last_mut()
                    .and_then(|frame| frame.list.as_mut())
                    .map(|list| match list {
                        List::Ordered(next) => {
                            let current = number("value").unwrap_or(*next);
                            *next = current.saturating_add(1);
                            Marker::Number(current)
                        }
                        List::Unordered => Marker::Bullet,
                    });
            }
        }
        if self.frames.len() == MAX_DEPTH {
            self.overflow = true;
            return;
        }
        let mut style = self.style();
        if let Some(value) = attr("lang").or_else(|| attr("xml:lang")) {
            style.language = language(value);
        }
        if let Some(value) = attr("dir") {
            style.direction = if value.eq_ignore_ascii_case("ltr") {
                Some(BaseDirection::Ltr)
            } else if value.eq_ignore_ascii_case("rtl") {
                Some(BaseDirection::Rtl)
            } else if value.eq_ignore_ascii_case("auto") {
                Some(BaseDirection::Auto)
            } else {
                style.direction
            };
        }
        if let Some(marker) = marker {
            style.item = Some((self.next_item, marker));
            self.next_item += 1;
        }
        let list = match tag.name.as_ref() {
            "ol" => Some(List::Ordered(number("start").unwrap_or(1))),
            "ul" => Some(List::Unordered),
            _ => None,
        };
        if matches!(tag.name.as_ref(), "blockquote" | "dd")
            || (list.is_some() && style.item.is_some())
        {
            style.indent = style.indent.saturating_add(1);
        }
        self.frames.push(Frame {
            name: tag.name.clone(),
            style,
            list,
        });
    }

    pub(super) fn finish(mut self, text: &str) -> (String, Article) {
        if self.overflow {
            return (super::display_text(text), Article::default());
        }
        self.close_link(text.len());
        let mut output = String::new();
        let mut paragraphs = Vec::new();
        let mut links: Vec<Link> = Vec::new();
        let mut last = None;
        let mut source_links = self.links.into_iter().enumerate().peekable();
        let mut marked = BTreeSet::new();
        let mut transitions = self.transitions.into_iter().peekable();
        let mut style = self.root;
        let mut start = 0;
        for line in text.split_inclusive('\n') {
            let line_offset = start;
            let first = start + line.len() - line.trim_start().len();
            start += line.len();
            while transitions.peek().is_some_and(|event| event.0 <= first) {
                style = transitions.next().unwrap().1;
            }
            let Some(range) = super::push_display_line(&mut output, line, |source, visible| {
                let at = line_offset + source;
                while source_links
                    .peek()
                    .is_some_and(|(_, link)| link.range.end <= at)
                {
                    source_links.next();
                }
                let Some((index, link)) = source_links
                    .peek()
                    .filter(|(_, link)| link.range.contains(&at))
                else {
                    return;
                };
                if last == Some(*index) {
                    links.last_mut().unwrap().range.end = visible.end;
                } else {
                    links.push(Link {
                        range: visible,
                        target: link.target.clone(),
                    });
                    last = Some(*index);
                }
            }) else {
                continue;
            };
            if range.is_empty() || style == Style::default() || paragraphs.len() >= MAX_PARAGRAPHS {
                continue;
            }
            paragraphs.push(Paragraph {
                range,
                indent: style.indent,
                list_item: style.item.is_some(),
                marker: style
                    .item
                    .and_then(|(id, marker)| marked.insert(id).then_some(marker)),
                direction: style.direction,
                language: style.language,
            });
        }
        (output, Article { paragraphs, links })
    }
}
