//! Layout metadata for the visible text of a StarDict HTML field.
use std::{collections::BTreeSet, ops::Range};

use html5ever::{
    LocalName,
    tokenizer::{Tag, TagKind},
};
use parley::{BaseDirection, Language};

const MAX_DEPTH: usize = 32;
const MAX_TRANSITIONS: usize = 1024;
pub(super) const MAX_PARAGRAPHS: usize = 512;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Article {
    pub(crate) paragraphs: Vec<Paragraph>,
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
}

pub(super) fn language(tag: &str) -> Option<Language> {
    (tag.len() <= crate::source::MAX_LANGUAGE_BYTES)
        .then(|| Language::parse(tag).ok())
        .flatten()
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
        }
    }

    fn style(&self) -> Style {
        self.frames.last().map_or(self.root, |frame| frame.style)
    }

    pub(super) fn tag(&mut self, tag: &Tag, offset: usize) {
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

    pub(super) fn finish(self, text: &str) -> (String, Vec<Paragraph>) {
        if self.overflow {
            return (super::display_text(text), Vec::new());
        }
        let mut output = String::new();
        let mut paragraphs = Vec::new();
        let mut marked = BTreeSet::new();
        let mut transitions = self.transitions.into_iter().peekable();
        let mut style = self.root;
        let mut start = 0;
        for line in text.split_inclusive('\n') {
            let first = start + line.len() - line.trim_start().len();
            start += line.len();
            while transitions.peek().is_some_and(|event| event.0 <= first) {
                style = transitions.next().unwrap().1;
            }
            let Some(range) = super::push_display_line(&mut output, line) else {
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
        (output, paragraphs)
    }
}
