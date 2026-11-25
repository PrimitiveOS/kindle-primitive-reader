//! Resolve generated quotation marks in publisher DOM order.
#[cfg(test)]
mod tests;

use super::{BookStyle, ContentPart, Element, Generated, Side, Style, expand};
use crate::{Settings, epub::Book, user_styles};
use std::{collections::HashMap, sync::Arc};

const MAX_PREFIX_CHAPTERS: usize = 16;
const MAX_PREFIX_BYTES: usize = 512 * 1024;

#[derive(Clone, Copy)]
pub(crate) enum Token {
    Open,
    Close,
    SilentOpen,
    SilentClose,
}

impl Token {
    pub(super) fn parse(word: &str) -> Option<Self> {
        if word.eq_ignore_ascii_case("open-quote") {
            Some(Self::Open)
        } else if word.eq_ignore_ascii_case("close-quote") {
            Some(Self::Close)
        } else if word.eq_ignore_ascii_case("no-open-quote") {
            Some(Self::SilentOpen)
        } else if word.eq_ignore_ascii_case("no-close-quote") {
            Some(Self::SilentClose)
        } else {
            None
        }
    }

    pub(super) fn output_bytes(self) -> usize {
        match self {
            Self::Open | Self::Close => 6,
            Self::SilentOpen | Self::SilentClose => 0,
        }
    }
}

#[derive(Default)]
pub(crate) struct Depths(HashMap<Box<str>, i32>);

pub(crate) fn prefix(
    book: &mut Book,
    chapter: usize,
    settings: Settings,
    user: &user_styles::Snapshot,
    shaping_language: Option<parley::Language>,
    main: &str,
) -> Depths {
    if chapter > MAX_PREFIX_CHAPTERS || !book.is_reflowable_epub() {
        return Depths::default();
    }
    (|| {
        let mut remaining = MAX_PREFIX_BYTES;
        let mut depths = Depths::default();
        for index in 0..chapter {
            let href = book.chapters.get(index)?.href.clone();
            let xhtml = book.chapter(index).ok()?;
            remaining = remaining.checked_sub(xhtml.len())?;
            let source = book
                .chapter_source_with_language(
                    &href,
                    &xhtml,
                    shaping_language,
                    settings.typography.respect_embedded,
                )
                .ok()?;
            let author = BookStyle::load_for_flow_with_base(
                book,
                &href,
                &source,
                settings.publisher_styles,
                user,
                settings.base_stylesheet,
            )
            .ok()?;
            let targets = if book.spine_rtl {
                None
            } else {
                crate::footnotes::in_page::Targets::epub_for_settings(
                    &href, &xhtml, &source, settings, user,
                )
            };
            let (mut styles, quotes) =
                author.compute_deferred_quotes(&source.elements, settings, targets.as_ref());
            quotes.resolve(
                &source.elements,
                &mut styles,
                main,
                settings.typography.respect_embedded,
                &mut depths,
            );
        }
        Some(depths)
    })()
    .unwrap_or_default()
}

impl Depths {
    fn append(&mut self, language: &str, token: Token, output: &mut String) {
        let depth = if let Some(depth) = self.0.get_mut(language) {
            depth
        } else {
            self.0.entry(language.into()).or_default()
        };
        let opening = matches!(token, Token::Open | Token::SilentOpen);
        if opening {
            *depth = depth.saturating_add(1);
        }
        let pair = usize::from(*depth % 2 == 0);
        if !opening {
            *depth = depth.saturating_sub(1);
        }
        if matches!(token, Token::SilentOpen | Token::SilentClose) {
            return;
        }
        let (left, right) = pairs(language)[pair];
        if opening {
            output.push(left);
            output.push('\u{2060}');
        } else {
            output.push('\u{2060}');
            output.push(right);
        }
    }
}

fn pairs(language: &str) -> [(char, char); 2] {
    let language = language.as_bytes();
    let starts = |prefix: &[u8]| {
        language
            .get(..prefix.len())
            .is_some_and(|start| start.eq_ignore_ascii_case(prefix))
            && language.get(prefix.len()).is_none_or(|&next| next == b'-')
    };
    if starts(b"fr-ch") {
        [('«', '»'), ('‹', '›')]
    } else if starts(b"fr") {
        [('«', '»'), ('“', '”')]
    } else if starts(b"de") {
        [('„', '“'), ('‚', '‘')]
    } else {
        [('“', '”'), ('‘', '’')]
    }
}

pub(super) struct Pending<'a> {
    pub(super) host: usize,
    pub(super) side: Side,
    pub(super) parts: &'a [ContentPart],
    pub(super) style: Option<Style>,
}

#[derive(Default)]
pub(crate) struct Plan<'a>(pub(super) Vec<Pending<'a>>);

impl Plan<'_> {
    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(crate) fn resolve(
        self,
        elements: &[Element],
        styles: &mut [Style],
        main: &str,
        respect_embedded: bool,
        depths: &mut Depths,
    ) {
        if self.is_empty() {
            return;
        }
        let mut pending = self.0.into_iter().peekable();
        let mut ancestors: Vec<(usize, &str, Option<Pending<'_>>)> = Vec::new();
        for (index, element) in elements.iter().enumerate() {
            while ancestors
                .last()
                .is_some_and(|entry| Some(entry.0) != element.parent)
            {
                let (_, language, after) = ancestors.pop().unwrap();
                if let Some(after) = after {
                    after.resolve(elements, styles, language, depths);
                }
            }
            let inherited = ancestors.last().map_or(main, |entry| entry.1);
            let language = if respect_embedded {
                element.quote_language.as_deref().unwrap_or(inherited)
            } else {
                main
            };
            let mut after = None;
            while pending.peek().is_some_and(|part| part.host == index) {
                let part = pending.next().unwrap();
                match part.side {
                    Side::Before => part.resolve(elements, styles, language, depths),
                    Side::After => after = Some(part),
                }
            }
            ancestors.push((index, language, after));
        }
        for (_, language, after) in ancestors.into_iter().rev() {
            if let Some(after) = after {
                after.resolve(elements, styles, language, depths);
            }
        }
    }
}

impl Pending<'_> {
    fn resolve(
        self,
        elements: &[Element],
        styles: &mut [Style],
        language: &str,
        depths: &mut Depths,
    ) {
        let text = expand(self.parts, &elements[self.host], |token, output| {
            depths.append(language, token, output);
        });
        if let (Some(text), Some(style)) = (text, self.style) {
            let pair = styles[self.host]
                .generated
                .get_or_insert_with(|| Arc::new([None, None]));
            Arc::make_mut(pair)[self.side as usize] = Some(Generated { text, style });
        }
    }
}
