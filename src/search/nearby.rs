//! One offline match from a source position. Searches stop at the book boundary.
use super::{FoldedText, Query, SOFT_HYPHEN, SearchHit, accent_mark, snippet};
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
    query: Query,
    direction: SearchDirection,
}

impl NearbySearch {
    pub fn new(
        text: &str,
        match_case: bool,
        ignore_accents: bool,
        direction: SearchDirection,
    ) -> Result<Self> {
        Ok(Self {
            query: Query::with_options(text, match_case, ignore_accents)?,
            direction,
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
            ChapterText::read(book, ci)
        })
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
            if text.book != anchor.book || ci == origin && text.href != anchor.chapter {
                return Err("The publication changed during the search".into());
            }
            if cancel.load(Ordering::Relaxed) {
                return Ok(None);
            }
            if let Some(source) = text
                .xhtml
                .as_deref()
                .and_then(|xml| Source::parse(xml).ok())
            {
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
                    let mut left = 0;
                    let mut right = block.text.len();
                    if let Some((at, offset)) = boundary {
                        if previous && index > at || !previous && index < at {
                            continue;
                        }
                        if index == at {
                            if previous {
                                right = offset
                            } else {
                                left = offset
                            }
                        }
                    }
                    let Some(window) = block.text.get(left..right) else {
                        return Err("The starting character is unavailable".into());
                    };
                    if let Some(range) = self.query.nearest(window, previous, cancel) {
                        let span = range.start + left..range.end + left;
                        let start = Anchor {
                            book: text.book,
                            chapter: text.href.clone(),
                            revision: source.revision,
                            block: block.key,
                            offset: span.start,
                            illustration: None,
                        };
                        let end = Anchor {
                            offset: span.end,
                            ..start.clone()
                        };
                        let (snippet, matched) = snippet(&block.text, span);
                        return Ok(Some(SearchHit {
                            chapter: ci,
                            chapter_title: text.label,
                            anchor: start,
                            end,
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
    book: u64,
    label: String,
    href: String,
    xhtml: Option<String>,
}

impl ChapterText {
    pub(crate) fn read(book: &mut Book, index: usize) -> Result<Self> {
        let info = book
            .chapters
            .get(index)
            .ok_or("That chapter is unavailable")?
            .clone();
        Ok(Self {
            book: book.revision(),
            label: info.label,
            href: info.href,
            xhtml: book.chapter(index).ok(),
        })
    }
}

impl Query {
    fn nearest(&self, source: &str, previous: bool, cancel: &AtomicBool) -> Option<Range<usize>> {
        if self.match_case && !self.ignore_accents && !source.contains(SOFT_HYPHEN) {
            let start = if previous {
                source.rfind(&self.text)
            } else {
                source.find(&self.text)
            }?;
            return Some(start..start + self.text.len());
        }
        let folded = FoldedText::build(source, self.match_case, self.ignore_accents, || {
            cancel.load(Ordering::Relaxed)
        })?;
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
