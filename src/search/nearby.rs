//! One offline match from a source position. Searches stop at the book boundary.
use super::{FlowConfig, FoldedText, Query, ReflowQuery, SearchHit, accent_mark, snippet};
use crate::{Anchor, Result, epub::Book, source::Source};
use std::{
    ops::Range,
    sync::atomic::{AtomicBool, Ordering},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchDirection {
    Previous,
    Next,
}

pub struct NearbySearch {
    query: ReflowQuery,
    direction: SearchDirection,
    flow_config: FlowConfig,
}

impl NearbySearch {
    pub fn new(
        text: &str,
        match_case: bool,
        ignore_accents: bool,
        direction: SearchDirection,
    ) -> Result<Self> {
        Ok(Self {
            query: ReflowQuery::Literal(Query::reflow(text, match_case, ignore_accents)?),
            direction,
            flow_config: FlowConfig::default(),
        })
    }

    pub fn with_regular_expression(
        pattern: &str,
        match_case: bool,
        direction: SearchDirection,
    ) -> Result<Self> {
        Ok(Self {
            query: ReflowQuery::Pattern(super::pattern::Expression::new(pattern, match_case)?),
            direction,
            flow_config: FlowConfig::default(),
        })
    }

    pub fn find(
        &self,
        book: &mut Book,
        chapter: usize,
        anchor: &Anchor,
        cancel: &AtomicBool,
    ) -> Result<Option<SearchHit>> {
        self.scan(chapter, anchor, cancel, book.chapters.len(), |ci| {
            ChapterText::read_with_flow(book, ci, &self.flow_config)
        })
    }

    pub(crate) fn with_flow_config(mut self, config: FlowConfig) -> Self {
        self.flow_config = config;
        self
    }

    pub(crate) fn scan(
        &self,
        origin: usize,
        anchor: &Anchor,
        cancel: &AtomicBool,
        chapters: usize,
        mut read: impl FnMut(usize) -> Result<ChapterText>,
    ) -> Result<Option<SearchHit>> {
        if origin >= chapters {
            return Err("The starting chapter is unavailable".into());
        }
        let mut ci = origin;
        let mut skipped = false;
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Ok(None);
            }
            let text = read(ci)?;
            if text.flow_config != self.flow_config {
                return Err("The reading styles changed during the search".into());
            }
            if text.book != anchor.book || ci == origin && text.href != anchor.chapter {
                return Err("The publication changed during the search".into());
            }
            if cancel.load(Ordering::Relaxed) {
                return Ok(None);
            }
            if let Some(source) = text.source {
                let boundary = if ci == origin {
                    let i = source
                        .blocks
                        .binary_search_by_key(&anchor.block, |block| block.key)
                        .map_err(|_| "The starting text is unavailable")?;
                    let offset = source
                        .project_offset(&source.blocks[i], anchor.revision, anchor.offset)
                        .ok_or("The starting position is unavailable")?;
                    Some((i, offset))
                } else {
                    None
                };
                let previous = self.direction == SearchDirection::Previous;
                let mut blocks = source.blocks.iter().enumerate();
                while let Some((index, block)) = if previous {
                    blocks.next_back()
                } else {
                    blocks.next()
                } {
                    if cancel.load(Ordering::Relaxed) {
                        return Ok(None);
                    }
                    let view = text
                        .flow
                        .view(&source, block)
                        .map_err(|error| error.to_string())?;
                    let rendered = view.text();
                    let mut left = 0;
                    let mut right = rendered.len();
                    if let Some((at, offset)) = boundary {
                        if previous && index > at || !previous && index < at {
                            continue;
                        }
                        if index == at {
                            let affinity = if previous {
                                crate::flow_projection::Affinity::Before
                            } else {
                                crate::flow_projection::Affinity::After
                            };
                            let offset = view
                                .project_nearest(offset, affinity)
                                .ok_or("The starting rendered position is unavailable")?;
                            if previous {
                                right = offset
                            } else {
                                left = offset
                            }
                        }
                    }
                    let Some(window) = rendered.get(left..right) else {
                        return Err("The starting character is unavailable".into());
                    };
                    let hit = match &self.query {
                        ReflowQuery::Literal(query) => query
                            .nearest(window, previous, cancel)
                            .map(|r| r.start + left..r.end + left),
                        ReflowQuery::Pattern(expression) => expression
                            .ranges(rendered, left..right, 1, previous, cancel)?
                            .pop(),
                    };
                    if let Some(span) = hit {
                        let source_spans = view
                            .source_spans(span.clone())
                            .ok_or("The result has invalid rendered text boundaries")?;
                        let Some(first) = source_spans.first() else {
                            continue;
                        };
                        let start = Anchor {
                            book: text.book,
                            chapter: text.href.clone(),
                            revision: source.revision,
                            block: block.key,
                            offset: first.start,
                            illustration: None,
                        };
                        let end = Anchor {
                            offset: source_spans.last().unwrap().end,
                            ..start.clone()
                        };
                        let (snippet, matched) = snippet(rendered, span, None);
                        return Ok(Some(SearchHit {
                            chapter: ci,
                            chapter_title: text.label,
                            anchor: start,
                            end,
                            source_spans,
                            snippet,
                            matched,
                        }));
                    }
                }
            } else if ci == origin {
                return Err("The starting chapter could not be read".into());
            } else {
                skipped = true;
            }
            match self.direction {
                SearchDirection::Previous if ci > 0 => ci -= 1,
                SearchDirection::Next if ci + 1 < chapters => ci += 1,
                _ if skipped => return Err("Some chapters could not be searched".into()),
                _ => return Ok(None),
            }
        }
    }
}

pub(crate) struct ChapterText {
    pub(super) book: u64,
    pub(super) label: String,
    pub(super) href: String,
    pub(super) source: Option<Source>,
    pub(super) flow: crate::flow_view::ChapterFlow,
    pub(super) flow_config: FlowConfig,
}

impl ChapterText {
    pub(crate) fn read(book: &mut Book, index: usize) -> Result<Self> {
        Self::read_with_flow(book, index, &FlowConfig::default())
    }

    pub(crate) fn read_with_flow(
        book: &mut Book,
        index: usize,
        config: &FlowConfig,
    ) -> Result<Self> {
        let info = book
            .chapters
            .get(index)
            .ok_or("That chapter is unavailable")?
            .clone();
        let mut source = book
            .chapter(index)
            .ok()
            .and_then(|xhtml| book.chapter_source(&info.href, &xhtml).ok());
        if let Some(source) = &mut source {
            for previous in book.previous_mobi_chapters(&info.href) {
                source.accept_previous_mobi(previous)?;
            }
        }
        let flow = match &source {
            Some(source) => config.chapter(book, &info.href, source)?,
            None => Default::default(),
        };
        Ok(Self {
            book: book.revision(),
            label: info.label,
            href: info.href,
            source,
            flow,
            flow_config: config.clone(),
        })
    }
}

impl Query {
    pub(super) fn nearest(
        &self,
        source: &str,
        previous: bool,
        cancel: &AtomicBool,
    ) -> Option<Range<usize>> {
        if self.use_original(source) {
            let start = if previous {
                source.rfind(&self.text)
            } else {
                source.find(&self.text)
            }?;
            return Some(start..start + self.text.len());
        }
        let folded = FoldedText::build(
            source,
            self.match_case,
            self.ignore_accents,
            self.typography,
            || cancel.load(Ordering::Relaxed),
        )?;
        let mut span = if previous {
            folded.last_match(&self.text, cancel)
        } else {
            folded
                .matches_while(&self.text, || !cancel.load(Ordering::Relaxed))
                .next()
        }?;
        if self.ignore_accents {
            span.end += source[span.end..]
                .chars()
                .take_while(|&ch| accent_mark(ch))
                .map(char::len_utf8)
                .sum::<usize>();
        }
        Some(span)
    }
}

impl FoldedText {
    fn last_match(&self, query: &str, cancel: &AtomicBool) -> Option<Range<usize>> {
        let mut limit = self.text.len();
        while !cancel.load(Ordering::Relaxed) {
            let start = self.text[..limit].rfind(query)?;
            let end = start + query.len();
            if let (Some(a), Some(b)) = (
                self.source_offset(start, true),
                self.source_offset(end, false),
            ) {
                return Some(a..b);
            }
            limit = end - self.text[..end].chars().next_back()?.len_utf8();
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Engine, Probe, Settings};

    #[test]
    fn nearby_search_matches_utf8_offsets_on_both_sides_of_a_source_position() {
        let cancel = AtomicBool::new(false);
        let q = Query::with_options("STRASSE", false, false).unwrap();
        let source = "Straße and STRA\u{ad}SSE";
        let forward = q.nearest(source, false, &cancel).unwrap();
        let backward = q.nearest(source, true, &cancel).unwrap();
        assert_eq!(&source[forward], "Straße");
        assert_eq!(&source[backward], "STRA\u{ad}SSE");
        let folded = Query::with_options("CAFE", false, true).unwrap();
        let source = "Café cafe\u{301}";
        assert_eq!(
            &source[folded.nearest(source, true, &cancel).unwrap()],
            "cafe\u{301}"
        );
        assert_eq!(
            Query::new("s", false).unwrap().nearest("ß", true, &cancel),
            None
        );
        assert_eq!(
            Query::new("aba", true)
                .unwrap()
                .nearest("ababa", true, &cancel),
            Some(2..5)
        );
    }

    #[test]
    fn nearby_search_crosses_chapters_without_wrapping_and_cancel_prevents_reads() {
        let path = crate::test_fixture("field-notes-epub3.epub");
        let fonts = [crate::test_fixture("fonts/NotoSans-Regular.ttf")];
        let probe = Probe::open(&path, Engine::Parley, &fonts, Settings::default(), 1).unwrap();
        let needle = "Linh";
        let at = probe.matches(needle).remove(0);
        let mut end = at.clone();
        end.offset += needle.len();
        let mut book = Book::open(&path).unwrap();
        let go = |d| NearbySearch::new(needle, false, false, d).unwrap();
        let cancel = AtomicBool::new(false);
        let prev = go(SearchDirection::Previous)
            .find(&mut book, 1, &at, &cancel)
            .unwrap()
            .unwrap();
        assert_eq!(prev.chapter, 0);
        assert!(
            go(SearchDirection::Previous)
                .find(&mut book, 0, &prev.anchor, &cancel)
                .unwrap()
                .is_none()
        );
        let next = go(SearchDirection::Next)
            .find(&mut book, 0, &prev.end, &cancel)
            .unwrap()
            .unwrap();
        assert_eq!(next.anchor, at);
        assert_eq!(next.end, end);
        assert_eq!(&next.snippet[next.matched], needle);
        cancel.store(true, Ordering::Relaxed);
        assert!(
            go(SearchDirection::Next)
                .scan(1, &at, &cancel, 3, |_| panic!(
                    "cancelled search read storage"
                ))
                .unwrap()
                .is_none()
        );
    }
}
