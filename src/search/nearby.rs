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
                .map(ChapterText::with_source_only_verified_layout)
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
        read: impl FnMut(usize) -> Result<ChapterText>,
    ) -> Result<Option<SearchHit>> {
        self.scan_from(
            origin,
            anchor.book,
            Some(anchor),
            None,
            cancel,
            chapters,
            read,
        )
    }

    /// Search from a publication edge up to the current visible page.
    /// The boundary is the current page end (forward) or start (backward).
    pub(crate) fn scan_edge(
        &self,
        publication: u64,
        stop_chapter: usize,
        stop_anchor: Option<&Anchor>,
        cancel: &AtomicBool,
        chapters: usize,
        read: impl FnMut(usize) -> Result<ChapterText>,
    ) -> Result<Option<SearchHit>> {
        if stop_chapter >= chapters {
            return Err("The current chapter is unavailable".into());
        }
        let origin = match self.direction {
            SearchDirection::Next => 0,
            SearchDirection::Previous => {
                chapters.checked_sub(1).ok_or("The book has no chapters")?
            }
        };
        self.scan_from(
            origin,
            publication,
            None,
            Some((stop_chapter, stop_anchor)),
            cancel,
            chapters,
            read,
        )
    }

    fn scan_from(
        &self,
        origin: usize,
        publication: u64,
        anchor: Option<&Anchor>,
        stop: Option<(usize, Option<&Anchor>)>,
        cancel: &AtomicBool,
        chapters: usize,
        mut read: impl FnMut(usize) -> Result<ChapterText>,
    ) -> Result<Option<SearchHit>> {
        if origin >= chapters {
            return Err("The starting chapter is unavailable".into());
        }
        let mut ci = origin;
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Ok(None);
            }
            let text = read(ci)?;
            if text.flow_config != self.flow_config {
                return Err("The reading styles changed during the search".into());
            }
            if text.book != publication
                || ci == origin && anchor.is_some_and(|anchor| text.href != anchor.chapter)
                || stop.is_some_and(|(chapter, boundary)| {
                    ci == chapter
                        && boundary.is_some_and(|boundary| {
                            text.href != boundary.chapter || boundary.book != publication
                        })
                })
            {
                return Err("The publication changed during the search".into());
            }
            if cancel.load(Ordering::Relaxed) {
                return Ok(None);
            }
            let stop_here = stop.is_some_and(|(chapter, _)| ci == chapter);
            if stop_here && stop.and_then(|(_, boundary)| boundary).is_none() {
                return Ok(None);
            }
            // The display-only notice is not the publication's text. It
            // cannot establish that there is no nearer match in this chapter.
            if text.placeholder {
                return Err("Some chapters could not be searched".into());
            }
            if let Some(source) = text.source {
                if crate::parley_layout::RAW_LAYOUT_ENABLED && text.raw_layout.is_none() {
                    return Err("Search requires the current verified chapter layout".into());
                }
                if let Some(layout) = &text.raw_layout {
                    layout.verify(text.book, &text.href, &source, &text.flow_config)?;
                }
                let ordered = super::raw_layout::ordered_blocks(&source, text.raw_layout.as_ref());
                let boundary = if let Some(anchor) = anchor.filter(|_| ci == origin) {
                    let i = ordered
                        .binary_search_by_key(&anchor.block, |block| block.key)
                        .map_err(|_| "The starting text is unavailable")?;
                    if anchor.raw_caret.is_some()
                        && !text
                            .raw_layout
                            .as_ref()
                            .is_some_and(|layout| layout.contains(anchor.block))
                    {
                        return Err("The starting raw position has no verified layout".into());
                    }
                    Some((i, anchor))
                } else {
                    None
                };
                let stop_boundary = if stop_here {
                    let anchor = stop.and_then(|(_, boundary)| boundary).unwrap();
                    let i = ordered
                        .binary_search_by_key(&anchor.block, |block| block.key)
                        .map_err(|_| "The current page text is unavailable")?;
                    if anchor.raw_caret.is_some()
                        && !text
                            .raw_layout
                            .as_ref()
                            .is_some_and(|layout| layout.contains(anchor.block))
                    {
                        return Err("The current raw position has no verified layout".into());
                    }
                    Some((i, anchor))
                } else {
                    None
                };
                let previous = self.direction == SearchDirection::Previous;
                let mut blocks = ordered.iter().copied().enumerate();
                while let Some((index, block)) = if previous {
                    blocks.next_back()
                } else {
                    blocks.next()
                } {
                    if cancel.load(Ordering::Relaxed) {
                        return Ok(None);
                    }
                    if let Some((at, _)) = stop_boundary {
                        if previous && index < at || !previous && index > at {
                            break;
                        }
                    }
                    let view = text
                        .flow
                        .view(&source, block)
                        .map_err(|error| error.to_string())?;
                    let raw = if text
                        .raw_layout
                        .as_ref()
                        .is_some_and(|layout| layout.contains(block.key))
                    {
                        Some(
                            block
                                .raw_whitespace
                                .as_ref()
                                .and_then(|s| s.bind(&block.text))
                                .ok_or("The raw search source has changed")?,
                        )
                    } else {
                        None
                    };
                    let projection = text
                        .raw_layout
                        .as_ref()
                        .and_then(|layout| layout.projection(block.key));
                    let rendered = raw.as_ref().map_or_else(
                        || view.text(),
                        |bound| projection.map_or(bound.raw(), |projection| projection.text()),
                    );
                    let mut left = 0;
                    let mut right = rendered.len();
                    if let Some((at, anchor)) = boundary {
                        if previous && index > at || !previous && index < at {
                            continue;
                        }
                        if index == at {
                            let affinity = if previous {
                                crate::flow_projection::Affinity::Before
                            } else {
                                crate::flow_projection::Affinity::After
                            };
                            let offset = boundary_offset(
                                anchor,
                                &source,
                                block,
                                &view,
                                raw.as_ref(),
                                projection,
                                affinity,
                            )?;
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
                        if let Some((_, anchor)) = stop_boundary.filter(|(at, _)| *at == index) {
                            let affinity = if previous {
                                crate::flow_projection::Affinity::Before
                            } else {
                                crate::flow_projection::Affinity::After
                            };
                            let offset = boundary_offset(
                                anchor,
                                &source,
                                block,
                                &view,
                                raw.as_ref(),
                                projection,
                                affinity,
                            )?;
                            if previous && span.end <= offset || !previous && span.start >= offset {
                                break;
                            }
                        }
                        let (start, end, source_spans) = if raw.is_some() {
                            super::raw_layout::matches_anchors(
                                text.book,
                                &text.href,
                                &source,
                                block,
                                std::slice::from_ref(&span),
                                projection,
                            )?
                            .pop()
                            .ok_or("The raw search result changed")?
                        } else {
                            let source_spans = view
                                .source_spans(span.clone())
                                .ok_or("The result has invalid rendered text boundaries")?;
                            let Some(first) = source_spans.first() else {
                                continue;
                            };
                            let start = Anchor {
                                raw_caret: None,
                                book: text.book,
                                chapter: text.href.clone(),
                                revision: source.revision,
                                block: block.key,
                                offset: first.start,
                                illustration: None,
                                side_bias: None,
                            };
                            let end = Anchor {
                                raw_caret: None,
                                offset: source_spans.last().unwrap().end,
                                ..start.clone()
                            };
                            (start, end, source_spans)
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
            } else if anchor.is_some() && ci == origin {
                return Err("The starting chapter could not be read".into());
            } else {
                // A later hit cannot be called the nearest when this chapter
                // may contain a closer occurrence. Keep the reading position
                // and report an incomplete search rather than skipping it.
                return Err("Some chapters could not be searched".into());
            }
            if stop_here {
                return Ok(None);
            }
            match self.direction {
                SearchDirection::Previous if ci > 0 => ci -= 1,
                SearchDirection::Next if ci + 1 < chapters => ci += 1,
                _ => return Ok(None),
            }
        }
    }
}

pub(super) fn boundary_offset(
    anchor: &Anchor,
    source: &Source,
    block: &crate::source::Block,
    view: &crate::flow_projection::Projection<'_>,
    raw: Option<&crate::source::raw_whitespace::Bound<'_>>,
    projection: Option<&crate::source::pre_line_projection::PreLineProjection>,
    affinity: crate::flow_projection::Affinity,
) -> Result<usize> {
    if anchor.illustration.is_some() {
        return Err("The search boundary is not text".into());
    }
    if let Some(caret) = &anchor.raw_caret {
        let bound = raw.ok_or("The raw search boundary has no verified layout")?;
        if anchor.revision != source.revision || anchor.offset != caret.canonical_offset {
            return Err("The raw search boundary has changed".into());
        }
        bound
            .validate_caret(caret)
            .ok_or("The raw search boundary is invalid")?;
        return projection
            .map_or(Some(caret.raw_offset), |projection| {
                projection.raw_to_visible(caret.raw_offset, affinity)
            })
            .ok_or_else(|| "The raw search boundary is not visible".into());
    }
    let offset = source
        .project_offset(block, anchor.revision, anchor.offset)
        .ok_or("The search boundary is unavailable")?;
    if let Some(bound) = raw {
        let raw = bound
            .canonical_to_raw(offset, affinity)
            .ok_or("The search boundary cannot be mapped to raw text")?;
        projection
            .map_or(Some(raw), |projection| {
                projection.raw_to_visible(raw, affinity)
            })
            .ok_or_else(|| "The search boundary cannot be mapped to visible text".into())
    } else {
        view.project_nearest(offset, affinity)
            .ok_or_else(|| "The search boundary cannot be mapped to rendered text".into())
    }
}

pub(crate) struct ChapterText {
    pub(super) book: u64,
    pub(super) label: String,
    pub(super) href: String,
    pub(super) placeholder: bool,
    pub(super) source: Option<Source>,
    pub(super) flow: crate::flow_view::ChapterFlow,
    pub(super) flow_config: FlowConfig,
    pub(super) raw_layout: Option<super::raw_layout::VerifiedRawLayout>,
}

impl ChapterText {
    /// Bind this worker's independently parsed chapter to an already shaped
    /// layout. Without a handoff, search continues to use canonical text.
    #[allow(dead_code)]
    pub(crate) fn with_verified_raw_layout(mut self, probe: &crate::Probe) -> Result<Self> {
        let layout = super::raw_layout::VerifiedRawLayout::from_probe(probe)?;
        let source = self
            .source
            .as_ref()
            .ok_or("The search chapter is unavailable")?;
        layout.verify(self.book, &self.href, source, &self.flow_config)?;
        self.raw_layout = Some(layout);
        Ok(self)
    }

    /// Source-only search (no probe, so no raw-whitespace spans to verify) uses an explicit verified
    /// layout with no raw blocks; it orders blocks as canonical search does. Production search always
    /// attaches a probe-verified layout instead (`app::nearby_search`, `with_verified_raw_layout`).
    pub(crate) fn with_source_only_verified_layout(mut self) -> Self {
        if let Some(source) = &self.source {
            self.raw_layout = Some(super::raw_layout::VerifiedRawLayout::verified_without_raw_blocks(
                self.book,
                &self.href,
                source.revision,
                self.flow_config.clone(),
            ));
        }
        self
    }

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
        let placeholder = book.recovered_placeholder(&info);
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
            placeholder,
            source,
            flow,
            flow_config: config.clone(),
            raw_layout: None,
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

    #[test]
    fn nearby_edge_search_preserves_source_anchors_and_publication_identity() {
        let path = crate::test_fixture("field-notes-epub3.epub");
        let mut book = Book::open(&path).unwrap();
        let publication = book.revision();
        let chapters = book.chapters.len();
        let cancelled = AtomicBool::new(false);
        let search = |direction| NearbySearch::new("Linh", false, false, direction).unwrap();
        let mut read = |index| ChapterText::read(&mut book, index).map(ChapterText::with_source_only_verified_layout);
        let first = search(SearchDirection::Next)
            .scan_from(0, publication, None, None, &cancelled, chapters, &mut read)
            .unwrap()
            .unwrap();
        let last = search(SearchDirection::Previous)
            .scan_from(
                chapters - 1,
                publication,
                None,
                None,
                &cancelled,
                chapters,
                &mut read,
            )
            .unwrap()
            .unwrap();
        assert!(first.chapter < last.chapter);
        assert_eq!(first.anchor.book, publication);
        assert_eq!(last.anchor.book, publication);
        assert_eq!(&first.snippet[first.matched.clone()], "Linh");
        assert_eq!(&last.snippet[last.matched.clone()], "Linh");
        assert_eq!(
            search(SearchDirection::Next)
                .scan_edge(
                    publication,
                    first.chapter,
                    Some(&first.end),
                    &cancelled,
                    chapters,
                    &mut read
                )
                .unwrap()
                .unwrap()
                .anchor,
            first.anchor
        );
        assert!(
            search(SearchDirection::Next)
                .scan_edge(
                    publication,
                    first.chapter,
                    Some(&first.anchor),
                    &cancelled,
                    chapters,
                    &mut read
                )
                .unwrap()
                .is_none()
        );
        assert_eq!(
            search(SearchDirection::Previous)
                .scan_edge(
                    publication,
                    last.chapter,
                    Some(&last.anchor),
                    &cancelled,
                    chapters,
                    &mut read
                )
                .unwrap()
                .unwrap()
                .anchor,
            last.anchor
        );
        assert!(
            search(SearchDirection::Previous)
                .scan_edge(
                    publication,
                    last.chapter,
                    Some(&last.end),
                    &cancelled,
                    chapters,
                    &mut read
                )
                .unwrap()
                .is_none()
        );
        assert!(
            search(SearchDirection::Next)
                .scan_edge(
                    publication.wrapping_add(1),
                    last.chapter,
                    Some(&last.end),
                    &cancelled,
                    chapters,
                    &mut read
                )
                .is_err()
        );
        cancelled.store(true, Ordering::Relaxed);
        assert!(
            search(SearchDirection::Previous)
                .scan_edge(
                    publication,
                    last.chapter,
                    Some(&last.end),
                    &cancelled,
                    chapters,
                    |_| panic!("cancelled search read storage")
                )
                .unwrap()
                .is_none()
        );
    }
}
