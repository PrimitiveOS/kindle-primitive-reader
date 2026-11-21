//! Offline search over the same normalized text and anchors used by the reader.

mod nearby;
mod pattern;
#[cfg(feature = "kindle")]
pub(crate) use nearby::ChapterText;
pub use nearby::{NearbySearch, SearchDirection};

use std::borrow::Cow;
use std::ops::Range;
use std::sync::atomic::{AtomicBool, Ordering};

use icu_casemap::{CaseMapper, CaseMapperBorrowed};
use icu_normalizer::DecomposingNormalizer;
use icu_properties::{CodePointMapData, props::GeneralCategory};

use crate::epub::Book;
use crate::source::Source;
use crate::{Anchor, Result};

#[cfg(feature = "kindle")]
use crate::fixed_text::{PdfPageText, PdfTextRect, PdfTextSpan};

pub const MAX_SEARCH_HITS: usize = 100;
pub(crate) const MAX_QUERY_BYTES: usize = 256;
const CONTEXT_CHARACTERS: usize = 28;
const CASE_MAPPER: CaseMapperBorrowed<'static> = CaseMapper::new();
const SOFT_HYPHEN: char = '\u{ad}';

#[cfg(feature = "kindle")]
pub(crate) fn definition_hits(text: &str, query: &str) -> Result<Vec<Range<usize>>> {
    Ok(Query::with_options(query, false, false)?.matches(text, MAX_SEARCH_HITS))
}

pub(crate) fn without_soft_hyphens(text: &str) -> Cow<'_, str> {
    if text.contains(SOFT_HYPHEN) {
        Cow::Owned(text.replace(SOFT_HYPHEN, ""))
    } else {
        Cow::Borrowed(text)
    }
}

/// Locate an exact quote and report a second match, including overlapping ones.
/// The caller supplies text with the same whitespace normalization on both sides.
pub(crate) fn find_quote(source: &str, quote: &str) -> Option<(Range<usize>, bool)> {
    let normalized = source
        .contains(SOFT_HYPHEN)
        .then(|| FoldedText::new(source, true));
    let text = normalized
        .as_ref()
        .map_or(source, |text| text.text.as_str());
    let start = text.find(quote)?;
    let end = start + quote.len();
    let next = start + quote.chars().next()?.len_utf8();
    let repeated = text[next..].contains(quote);
    let span = if let Some(normalized) = normalized {
        normalized.source_offset(start, true)?..normalized.source_offset(end, false)?
    } else {
        start..end
    };
    Some((span, repeated))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchHit {
    /// Zero-based index in the book's spine.
    pub chapter: usize,
    pub chapter_title: String,
    pub anchor: Anchor,
    pub end: Anchor,
    /// Original wording with a short amount of context.
    pub snippet: String,
    /// The visible portion of the match in the bounded snippet, in UTF-8 bytes.
    pub matched: Range<usize>,
}

/// A bounded search task. Each step processes at most one spine document.
/// The caller can stop or replace the task between steps. Search borrows
/// the book only during the step, so a device can release the storage lease.
pub struct Search {
    query: ReflowQuery,
    publication: Option<(u64, usize)>,
    next_chapter: usize,
    hits: usize,
    skipped: usize,
    stopped: bool,
}

impl Search {
    /// Queries use Unicode default case folding and literal text matching.
    /// Matches stay within a source text block and preserve its UTF-8 offsets.
    pub fn new(query: &str) -> Result<Self> {
        Self::with_case(query, false)
    }

    pub fn with_case(query: &str, match_case: bool) -> Result<Self> {
        Self::with_options(query, match_case, false)
    }

    pub fn with_options(query: &str, match_case: bool, ignore_accents: bool) -> Result<Self> {
        Self::with_query(ReflowQuery::Literal(Query::with_options(
            query,
            match_case,
            ignore_accents,
        )?))
    }

    pub fn with_regular_expression(pattern: &str, match_case: bool) -> Result<Self> {
        Self::with_query(ReflowQuery::Pattern(pattern::Expression::new(
            pattern, match_case,
        )?))
    }

    fn with_query(query: ReflowQuery) -> Result<Self> {
        Ok(Self {
            query,
            publication: None,
            next_chapter: 0,
            hits: 0,
            skipped: 0,
            stopped: false,
        })
    }

    pub fn scanned_chapters(&self) -> usize {
        self.next_chapter
    }

    pub fn skipped_chapters(&self) -> usize {
        self.skipped
    }

    pub fn limit_reached(&self) -> bool {
        self.hits >= MAX_SEARCH_HITS
    }

    pub fn is_finished(&self) -> bool {
        self.stopped
            || self.limit_reached()
            || self
                .publication
                .is_some_and(|(_, chapters)| self.next_chapter >= chapters)
    }

    /// Return only the hits found in the next chapter, in reading order.
    /// Chapters that cannot be parsed count as skipped and do not stop a scan.
    pub fn step(&mut self, book: &mut Book) -> Result<Vec<SearchHit>> {
        if self.is_finished() {
            return Ok(Vec::new());
        }
        if book.chapters.is_empty() {
            self.stopped = true;
            return Ok(Vec::new());
        }
        let text = nearby::ChapterText::read(book, self.next_chapter)?;
        self.step_loaded(text, book.chapters.len(), &AtomicBool::new(false))
    }

    pub(crate) fn step_loaded(
        &mut self,
        text: nearby::ChapterText,
        chapters: usize,
        cancel: &AtomicBool,
    ) -> Result<Vec<SearchHit>> {
        let publication = (text.book, chapters);
        if *self.publication.get_or_insert(publication) != publication {
            self.stopped = true;
            return Err("The publication changed during the search".into());
        }
        let chapter = self.next_chapter;
        self.next_chapter += 1;
        let source = match text
            .xhtml
            .as_deref()
            .and_then(|xhtml| Source::parse(xhtml).ok())
        {
            Some(source) => source,
            None => {
                self.skipped += 1;
                return Ok(Vec::new());
            }
        };

        let mut hits = Vec::new();
        for block in source.blocks {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            let ranges = match &self.query {
                ReflowQuery::Literal(query) => {
                    query.matches(&block.text, MAX_SEARCH_HITS - self.hits)
                }
                ReflowQuery::Pattern(expression) => expression.ranges(
                    &block.text,
                    0..block.text.len(),
                    MAX_SEARCH_HITS - self.hits,
                    false,
                    cancel,
                )?,
            };
            for range in ranges {
                let offset = range.start;
                let end = range.end;
                let (snippet, matched) = snippet(&block.text, range);
                hits.push(SearchHit {
                    chapter,
                    chapter_title: text.label.clone(),
                    anchor: Anchor {
                        illustration: None,
                        book: publication.0,
                        chapter: text.href.clone(),
                        revision: source.revision,
                        block: block.key,
                        offset,
                    },
                    end: Anchor {
                        illustration: None,
                        book: publication.0,
                        chapter: text.href.clone(),
                        revision: source.revision,
                        block: block.key,
                        offset: end,
                    },
                    snippet,
                    matched,
                });
                self.hits += 1;
            }
            if self.limit_reached() {
                break;
            }
        }
        Ok(hits)
    }
}

enum ReflowQuery {
    Literal(Query),
    Pattern(pattern::Expression),
}

pub(crate) struct Query {
    text: String,
    match_case: bool,
    ignore_accents: bool,
}

impl Query {
    #[cfg(test)]
    fn new(text: &str, match_case: bool) -> Result<Self> {
        Self::with_options(text, match_case, false)
    }

    pub(crate) fn with_options(text: &str, match_case: bool, ignore_accents: bool) -> Result<Self> {
        let text = text.trim();
        if text.len() > MAX_QUERY_BYTES {
            return Err("Enter a search term of up to 256 UTF-8 bytes".into());
        }
        let unwrapped = without_soft_hyphens(text);
        let term = unwrapped.trim();
        if term.is_empty() {
            return Err("Enter a search term of up to 256 UTF-8 bytes".into());
        }
        let text = FoldedText::with_options(term, match_case, ignore_accents).text;
        if text.trim().is_empty() {
            return Err("Enter at least one searchable character".into());
        }
        Ok(Self {
            text,
            match_case,
            ignore_accents,
        })
    }

    pub(crate) fn matches(&self, source: &str, limit: usize) -> Vec<Range<usize>> {
        if self.match_case && !self.ignore_accents && !source.contains(SOFT_HYPHEN) {
            source
                .match_indices(&self.text)
                .take(limit)
                .map(|(start, found)| start..start + found.len())
                .collect()
        } else {
            FoldedText::with_options(source, self.match_case, self.ignore_accents)
                .matches(&self.text)
                .take(limit)
                .map(|mut range| {
                    // Include combining marks in the original selected word.
                    if self.ignore_accents {
                        range.end += source[range.end..]
                            .chars()
                            .take_while(|&ch| accent_mark(ch))
                            .map(char::len_utf8)
                            .sum::<usize>();
                    }
                    range
                })
                .collect()
        }
    }
}

#[cfg(test)]
#[test]
fn match_case_preserves_original_utf8_offsets() {
    let source = "straße Straße STRASSE";
    let exact = Query::new("Straße", true).unwrap().matches(source, 10);
    assert_eq!(exact.len(), 1);
    assert_eq!(&source[exact[0].clone()], "Straße");
    let insensitive = Query::new("Straße", false).unwrap().matches(source, 10);
    assert_eq!(insensitive.len(), 3);
}

#[cfg(test)]
#[test]
fn accent_matching_keeps_the_original_vietnamese_word_offsets_and_case() {
    let source = "Cà phê · CA\u{300} PHE\u{302} · ca phe";
    let query = Query::with_options("CA PHE", false, true).unwrap();
    let matches: Vec<_> = query
        .matches(source, 5)
        .into_iter()
        .map(|span| &source[span])
        .collect();
    assert_eq!(matches, ["Cà phê", "CA\u{300} PHE\u{302}", "ca phe"]);
    assert_eq!(
        Query::with_options("CA PHE", true, true)
            .unwrap()
            .matches(source, 5)
            .len(),
        1
    );
    assert!(Query::with_options("\u{301}", false, true).is_err());
    assert_eq!(
        Query::with_options("ca phe", false, false)
            .unwrap()
            .matches(source, 5)
            .len(),
        1
    );
}

#[cfg(test)]
#[test]
fn soft_hyphens_are_invisible_to_search_and_preserved_in_match_offsets() {
    let source = "\u{ad}Über\u{ad}\u{ad}gang Straße\u{ad}n";
    for (query, match_case, expected) in [
        ("Übergang", true, "Über\u{ad}\u{ad}gang"),
        ("ÜBERGANG", false, "Über\u{ad}\u{ad}gang"),
        ("Über", true, "Über"),
        ("gang", true, "gang"),
        ("STRASSE", false, "Straße"),
        ("n", true, "n"),
        ("Über\u{ad}gang", true, "Über\u{ad}\u{ad}gang"),
    ] {
        let matches = Query::new(query, match_case).unwrap().matches(source, 1);
        assert_eq!(&source[matches[0].clone()], expected);
    }
    assert!(Query::new("\u{ad}", false).is_err());
}

#[cfg(feature = "kindle")]
pub(crate) struct PdfSearch {
    query: Query,
    pub(crate) next_page: usize,
    pub(crate) text_pages: usize,
    pub(crate) skipped: usize,
}

#[cfg(feature = "kindle")]
pub(crate) struct PdfSearchHit {
    pub(crate) page: usize,
    pub(crate) snippet: String,
    pub(crate) bounds: Vec<PdfTextRect>,
}

#[cfg(feature = "kindle")]
impl PdfSearch {
    #[cfg(test)]
    pub(crate) fn new(query: &str, match_case: bool) -> Result<Self> {
        Self::with_options(query, match_case, false)
    }

    pub(crate) fn with_options(
        query: &str,
        match_case: bool,
        ignore_accents: bool,
    ) -> Result<Self> {
        let mut query = Query::with_options(query, match_case, ignore_accents)?;
        query.text = query.text.split_whitespace().collect::<Vec<_>>().join(" ");
        Ok(Self {
            query,
            next_page: 0,
            text_pages: 0,
            skipped: 0,
        })
    }

    pub(crate) fn hits(
        &mut self,
        page: usize,
        text: &PdfPageText,
        limit: usize,
    ) -> Vec<PdfSearchHit> {
        if !text.text.trim().is_empty() {
            self.text_pages += 1;
        }
        let searchable = if self.query.text.contains(' ') {
            phrase_text(text)
        } else {
            Cow::Borrowed(text.text.as_str())
        };
        self.query
            .matches(&searchable, limit)
            .into_iter()
            .map(|range| {
                let bounds = text
                    .words
                    .iter()
                    .filter(|word| word.bytes.start < range.end && word.bytes.end > range.start)
                    .map(|word| word.bounds)
                    .collect();
                PdfSearchHit {
                    page,
                    snippet: snippet(&text.text, range).0,
                    bounds,
                }
            })
            .collect()
    }
}

/// Join a wrapped phrase in visual reading order and keep every original UTF-8 boundary.
/// A PDF text draw can place separate columns on the same baseline or switch columns
/// between lines. Neither gap should make its two words into a single search phrase.
#[cfg(feature = "kindle")]
fn phrase_text(page: &PdfPageText) -> Cow<'_, str> {
    let mut text = Cow::Borrowed(page.text.as_str());
    let mut previous = None;
    for line in &page.lines {
        let first = page
            .words
            .partition_point(|word| word.bytes.end <= line.bytes.start);
        let rest = &page.words[first..];
        let end = rest.partition_point(|word| word.bytes.end <= line.bytes.end);
        let words = &rest[..end];
        let Some(start) = words.first() else {
            previous = None;
            continue;
        };
        let mut segment = start.bounds;
        let mut first_segment = None;
        for pair in words.windows(2) {
            let (a, b) = (&pair[0], &pair[1]);
            let gap = a.bytes.end..b.bytes.start;
            if page.text.get(gap.clone()) == Some(" ") && column_gap(page, a, b) {
                first_segment.get_or_insert(segment);
                text.to_mut().replace_range(gap, "\n");
                segment = b.bounds;
            } else {
                segment = PdfTextRect {
                    x0: segment.x0.min(b.bounds.x0),
                    y0: segment.y0.min(b.bounds.y0),
                    x1: segment.x1.max(b.bounds.x1),
                    y1: segment.y1.max(b.bounds.y1),
                };
            }
        }
        if let Some((end, tail)) = previous {
            let gap = end..line.bytes.start;
            if page.text.get(gap.clone()) == Some("\n")
                && wrapped_line(tail, first_segment.unwrap_or(segment))
            {
                text.to_mut().replace_range(gap, " ");
            }
        }
        previous = Some((line.bytes.end, segment));
    }
    text
}

#[cfg(feature = "kindle")]
fn column_gap(page: &PdfPageText, a: &PdfTextSpan, b: &PdfTextSpan) -> bool {
    // Compare on the same axis. Normalized coordinates have different x/y scales.
    let advance = |word: &PdfTextSpan| {
        let n = page
            .text
            .get(word.bytes.clone())
            .map_or(1, |text| text.chars().count().max(1)) as f32;
        (word.bounds.x1 - word.bounds.x0) / n
    };
    let gap = (b.bounds.x0 - a.bounds.x1).max(a.bounds.x0 - b.bounds.x1);
    gap > 5.0 * advance(a).max(advance(b))
}

#[cfg(feature = "kindle")]
fn wrapped_line(a: PdfTextRect, b: PdfTextRect) -> bool {
    let overlap = a.x1.min(b.x1) - a.x0.max(b.x0);
    let narrow = (a.x1 - a.x0).min(b.x1 - b.x0);
    let step = (b.y0 + b.y1 - a.y0 - a.y1) * 0.5;
    let ha = a.y1 - a.y0;
    let hb = b.y1 - b.y0;
    // The extractor measures glyph ink, which can occupy less than half the
    // baseline spacing of a double-spaced page.
    overlap > narrow * 0.3 && step > 0.2 * ha.min(hb) && step <= 3.0 * ha.max(hb)
}

/// Byte positions changed by full case folding or removed soft hyphens.
/// Between edits, offsets differ by a constant and need no per-byte index.
struct OffsetEdit {
    source_end: usize,
    folded_start: usize,
    folded_end: usize,
}

struct FoldedText {
    text: String,
    edits: Vec<OffsetEdit>,
}

fn accent_mark(ch: char) -> bool {
    matches!(
        CodePointMapData::<GeneralCategory>::new().get(ch),
        GeneralCategory::NonspacingMark
            | GeneralCategory::SpacingMark
            | GeneralCategory::EnclosingMark
    )
}

impl FoldedText {
    fn new(original: &str, match_case: bool) -> Self {
        Self::with_options(original, match_case, false)
    }

    fn with_options(original: &str, match_case: bool, ignore_accents: bool) -> Self {
        Self::build(original, match_case, ignore_accents, || false).unwrap()
    }

    fn build(
        original: &str,
        match_case: bool,
        ignore_accents: bool,
        cancelled: impl Fn() -> bool,
    ) -> Option<Self> {
        if cancelled() {
            return None;
        }
        if original.is_ascii() {
            return Some(Self {
                text: if match_case {
                    original.to_owned()
                } else {
                    original.to_ascii_lowercase()
                },
                edits: Vec::new(),
            });
        }
        let mut text = String::with_capacity(original.len());
        let mut edits = Vec::new();
        let mut scalar = [0; 4];
        for (index, (source_start, ch)) in original.char_indices().enumerate() {
            if index % 2048 == 0 && cancelled() {
                return None;
            }
            if ch.is_ascii() || (match_case && !ignore_accents && ch != SOFT_HYPHEN) {
                text.push(if match_case {
                    ch
                } else {
                    ch.to_ascii_lowercase()
                });
                continue;
            }
            let folded = if ch == SOFT_HYPHEN {
                Cow::Borrowed("")
            } else if match_case {
                Cow::Borrowed(&*ch.encode_utf8(&mut scalar))
            } else {
                CASE_MAPPER.fold_string(ch.encode_utf8(&mut scalar))
            };
            let folded = if ignore_accents {
                Cow::Owned(
                    DecomposingNormalizer::new_nfd()
                        .normalize_iter(folded.chars())
                        .filter(|&ch| !accent_mark(ch))
                        .collect::<String>(),
                )
            } else {
                folded
            };
            let folded_start = text.len();
            text.push_str(&folded);
            if folded.len() != ch.len_utf8() || folded.chars().nth(1).is_some() {
                edits.push(OffsetEdit {
                    source_end: source_start + ch.len_utf8(),
                    folded_start,
                    folded_end: text.len(),
                });
            }
        }
        Some(Self { text, edits })
    }

    fn source_offset(&self, boundary: usize, match_start: bool) -> Option<usize> {
        let preceding = self.edits.partition_point(|edit| {
            edit.folded_end < boundary
                || (edit.folded_end == boundary && (match_start || edit.folded_start < boundary))
        });
        if self
            .edits
            .get(preceding)
            .is_some_and(|edit| edit.folded_start < boundary)
        {
            // A match may cover the original scalar or miss it entirely.
            // An offset inside a multi-character fold has no source boundary.
            return None;
        }
        match preceding
            .checked_sub(1)
            .and_then(|index| self.edits.get(index))
        {
            Some(edit) => Some(edit.source_end + boundary - edit.folded_end),
            None => Some(boundary),
        }
    }

    fn matches<'a>(&'a self, query: &'a str) -> impl Iterator<Item = Range<usize>> + 'a {
        self.matches_while(query, || true)
    }

    fn matches_while<'a>(
        &'a self,
        query: &'a str,
        keep_searching: impl Fn() -> bool + 'a,
    ) -> impl Iterator<Item = Range<usize>> + 'a {
        let mut cursor = 0;
        std::iter::from_fn(move || {
            loop {
                if !keep_searching() {
                    return None;
                }
                let folded_start = cursor + self.text.get(cursor..)?.find(query)?;
                let folded_end = folded_start + query.len();
                if let (Some(start), Some(end)) = (
                    self.source_offset(folded_start, true),
                    self.source_offset(folded_end, false),
                ) {
                    cursor = folded_end;
                    return Some(start..end);
                }
                // Resume one scalar later. An invalid partial fold can overlap
                // a valid match that begins at the following source character.
                cursor = folded_start + self.text[folded_start..].chars().next()?.len_utf8();
            }
        })
    }
}

fn snippet(text: &str, matched: Range<usize>) -> (String, Range<usize>) {
    let first = text[..matched.start]
        .char_indices()
        .rev()
        .nth(CONTEXT_CHARACTERS - 1)
        .map_or(0, |(index, _)| index);
    // Regexps may select an entire paragraph. Keep the complete source anchors
    // while bounding the memory kept by a 100-item results list.
    let shown_end = text[matched.clone()]
        .char_indices()
        .nth(256)
        .map_or(matched.end, |(offset, _)| matched.start + offset);
    let last = if shown_end < matched.end {
        shown_end
    } else {
        text[matched.end..]
            .char_indices()
            .nth(CONTEXT_CHARACTERS)
            .map_or(text.len(), |(index, _)| matched.end + index)
    };
    let mut preview = String::with_capacity(last - first + 6);
    if first > 0 {
        preview.push('…');
    }
    let match_start = preview.len() + matched.start - first;
    preview.extend(
        text[first..last]
            .chars()
            .map(|ch| if ch == '\n' { ' ' } else { ch }),
    );
    if last < text.len() {
        preview.push('…');
    }
    (
        preview,
        match_start..match_start + (shown_end - matched.start),
    )
}

#[cfg(test)]
mod tests {

    use super::*;

    #[cfg(feature = "kindle")]
    #[test]
    fn wrapped_pdf_phrases_keep_source_words_and_respect_column_gutters() {
        let mut page = PdfPageText::default();
        // Two rows in parallel columns, followed by a double-spaced paragraph.
        for (y, row) in [
            (
                0.1,
                vec![
                    (0.1, "North"),
                    (0.16, "bank"),
                    (0.62, "South"),
                    (0.68, "bank"),
                ],
            ),
            (
                0.13,
                vec![
                    (0.1, "West"),
                    (0.15, "pier"),
                    (0.62, "East"),
                    (0.67, "pier"),
                ],
            ),
            (0.5, vec![(0.1, "Marée"), (0.16, "basse")]),
            (0.55, vec![(0.1, "au"), (0.13, "quai")]),
            (0.8, vec![(0.1, "Final"), (0.16, "page")]),
        ] {
            if !page.text.is_empty() {
                page.text.push('\n');
            }
            let start = page.text.len();
            let left = row[0].0;
            for (x, token) in row {
                if page.text.len() != start {
                    page.text.push(' ');
                }
                let offset = page.text.len();
                page.text.push_str(token);
                page.words.push(PdfTextSpan {
                    bytes: offset..page.text.len(),
                    bounds: PdfTextRect {
                        x0: x,
                        x1: x + 0.01 * token.chars().count() as f32,
                        y0: y,
                        y1: y + 0.02,
                    },
                });
            }
            page.lines.push(PdfTextSpan {
                bytes: start..page.text.len(),
                bounds: PdfTextRect {
                    x0: left,
                    ..page.words.last().unwrap().bounds
                },
            });
        }
        let found = PdfSearch::with_options("MAREE  basse\n au\tquai", false, true)
            .unwrap()
            .hits(2, &page, 3);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].page, 2);
        assert!(found[0].snippet.contains("Marée basse au quai"));
        assert_eq!(
            found[0].bounds,
            page.words[8..12]
                .iter()
                .map(|w| w.bounds)
                .collect::<Vec<_>>()
        );
        for query in ["bank South", "bank West", "pier East", "quai Final"] {
            assert!(
                PdfSearch::new(query, false)
                    .unwrap()
                    .hits(2, &page, 3)
                    .is_empty(),
                "{query}"
            );
        }
        assert_eq!(
            PdfSearch::new("East pier", true)
                .unwrap()
                .hits(2, &page, 1)
                .len(),
            1
        );
    }

    #[test]
    fn locates_original_utf8_spans_for_full_case_folds() {
        let text = "Café Straße ς İ ﬃ ßßs";
        let folded = FoldedText::new(text, false);
        for (needle, expected) in [
            ("CAFÉ", "Café"),
            ("STRASSE", "Straße"),
            ("Σ", "ς"),
            ("i\u{307}", "İ"),
            ("FFI", "ﬃ"),
            ("SSS", "ßs"),
        ] {
            let query = CASE_MAPPER.fold_string(needle);
            let found = folded
                .matches(&query)
                .map(|range| &text[range])
                .collect::<Vec<_>>();
            assert_eq!(found, [expected]);
        }
        assert_eq!(FoldedText::new("ß", false).matches("s").count(), 0);
    }

    #[test]
    fn steps_follow_the_spine_and_report_exact_original_wording() {
        let fixture = crate::test_fixture("field-notes-epub3.epub");
        let mut book = Book::open(&fixture).unwrap();
        let mut search = Search::new("LINH").unwrap();
        assert!(!search.is_finished());
        for index in 0..book.chapters.len() {
            let hits = search.step(&mut book).unwrap();
            assert_eq!(hits.len(), 1);
            let hit = &hits[0];
            assert_eq!(hit.chapter, index);
            assert_eq!(hit.chapter_title, book.chapters[index].label);
            assert_eq!(hit.anchor.book, book.revision());
            assert_eq!(&hit.snippet[hit.matched.clone()], "Linh");
            let source = Source::parse(&book.chapter(index).unwrap()).unwrap();
            assert_eq!(source.revision, hit.anchor.revision);
            let block = source
                .blocks
                .iter()
                .find(|block| block.key == hit.anchor.block)
                .unwrap();
            assert!(block.text[hit.anchor.offset..].starts_with("Linh"));
        }
        assert!(search.is_finished());
        assert!(search.step(&mut book).unwrap().is_empty());
    }

    #[test]
    fn caps_dense_hits_without_scanning_past_the_current_chapter() {
        let fixture = crate::test_fixture("long-chapter.epub");
        let mut book = Book::open(&fixture).unwrap();
        let mut search = Search::new("observation").unwrap();
        let hits = search.step(&mut book).unwrap();
        assert_eq!(hits.len(), MAX_SEARCH_HITS);
        assert!(search.limit_reached());
        assert!(search.is_finished());
    }
}
