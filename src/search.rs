//! Offline search over the same normalized text and anchors used by the reader.

#[cfg(feature = "kindle")]
mod fixed_pattern;
mod flow;
#[cfg(test)]
mod flow_tests;
mod nearby;
mod pattern;
pub(crate) use flow::FlowConfig;
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
/// KOReader's whole-book results list ceiling for rolling documents.
pub const MAX_REFLOW_SEARCH_HITS: usize = 5_000;
#[cfg(feature = "kindle")]
pub(crate) const MAX_FIXED_SEARCH_HITS: usize = 5_000;
#[cfg(feature = "kindle")]
const MAX_FIXED_RESULT_BYTES: usize = 20 * 1024 * 1024;
const MAX_RESULT_BYTES: usize = 8 * 1024 * 1024;
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
    /// Retained canonical intervals. An envelope may cross CSS-suppressed text;
    /// consumers must use the rendered view rather than copy that envelope.
    pub source_spans: Vec<Range<usize>>,
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
    flow_config: FlowConfig,
    context_words: Option<u8>,
    publication: Option<(u64, usize)>,
    next_chapter: usize,
    hits: usize,
    limit: usize,
    retained_bytes: usize,
    memory_limited: bool,
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
        Self::with_query(ReflowQuery::Literal(Query::reflow(
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
            flow_config: FlowConfig::default(),
            context_words: None,
            publication: None,
            next_chapter: 0,
            hits: 0,
            limit: MAX_REFLOW_SEARCH_HITS,
            retained_bytes: 0,
            memory_limited: false,
            skipped: 0,
            stopped: false,
        })
    }

    /// Request a smaller result set for an explicit bounded comparison.
    pub fn at_most(mut self, limit: usize) -> Self {
        self.limit = limit.min(MAX_REFLOW_SEARCH_HITS);
        self
    }

    #[cfg(feature = "kindle")]
    pub(crate) fn with_context_words(mut self, words: u8) -> Self {
        self.context_words = Some(words);
        self
    }

    pub(crate) fn with_flow_config(mut self, config: FlowConfig) -> Self {
        self.flow_config = config;
        self
    }

    pub(crate) fn for_publication(mut self, book: u64, chapters: usize) -> Self {
        self.publication = Some((book, chapters));
        self
    }

    pub fn memory_limited(&self) -> bool {
        self.memory_limited
    }

    pub fn scanned_chapters(&self) -> usize {
        self.next_chapter
    }

    pub fn skipped_chapters(&self) -> usize {
        self.skipped
    }

    pub fn limit_reached(&self) -> bool {
        self.hits >= self.limit
    }

    pub fn is_finished(&self) -> bool {
        self.stopped
            || self.memory_limited
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
        let text = nearby::ChapterText::read_with_flow(book, self.next_chapter, &self.flow_config)?;
        self.step_loaded(text, book.chapters.len(), &AtomicBool::new(false))
    }

    pub(crate) fn step_loaded(
        &mut self,
        text: nearby::ChapterText,
        chapters: usize,
        cancel: &AtomicBool,
    ) -> Result<Vec<SearchHit>> {
        if text.flow_config != self.flow_config {
            self.stopped = true;
            return Err("The reading styles changed during the search".into());
        }
        let publication = (text.book, chapters);
        if *self.publication.get_or_insert(publication) != publication {
            self.stopped = true;
            return Err("The publication changed during the search".into());
        }
        let chapter = self.next_chapter;
        self.next_chapter += 1;
        let source = match text.source {
            Some(source) => source,
            None => {
                self.skipped += 1;
                return Ok(Vec::new());
            }
        };

        let mut hits = Vec::new();
        for block in &source.blocks {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            let view = text
                .flow
                .view(&source, block)
                .map_err(|error| error.to_string())?;
            let rendered = view.text();
            let ranges = match &self.query {
                ReflowQuery::Literal(query) => {
                    query.matches_until(rendered, self.limit - self.hits, || {
                        cancel.load(Ordering::Relaxed)
                    })
                }
                ReflowQuery::Pattern(expression) => expression.ranges(
                    rendered,
                    0..rendered.len(),
                    self.limit - self.hits,
                    false,
                    cancel,
                )?,
            };
            for range in ranges {
                let source_spans = view
                    .source_spans(range.clone())
                    .ok_or("The result has invalid rendered text boundaries")?;
                let Some(first) = source_spans.first() else {
                    continue;
                };
                let offset = first.start;
                let end = source_spans.last().unwrap().end;
                let (snippet, matched) = snippet(rendered, range, self.context_words);
                let allocation = std::mem::size_of::<SearchHit>()
                    + text.label.len()
                    + 2 * text.href.len()
                    + snippet.capacity()
                    + source_spans.capacity() * std::mem::size_of::<Range<usize>>();
                if allocation > MAX_RESULT_BYTES.saturating_sub(self.retained_bytes) {
                    self.memory_limited = true;
                    return Ok(hits);
                }
                self.retained_bytes += allocation;
                hits.push(SearchHit {
                    chapter,
                    chapter_title: text.label.clone(),
                    anchor: Anchor {
                        raw_caret: None,
                        illustration: None,
                        book: publication.0,
                        chapter: text.href.clone(),
                        revision: source.revision,
                        block: block.key,
                        offset,
                    },
                    end: Anchor {
                        raw_caret: None,
                        illustration: None,
                        book: publication.0,
                        chapter: text.href.clone(),
                        revision: source.revision,
                        block: block.key,
                        offset: end,
                    },
                    source_spans,
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
    typography: bool,
    match_case: bool,
    ignore_accents: bool,
}

impl Query {
    #[cfg(test)]
    fn new(text: &str, match_case: bool) -> Result<Self> {
        Self::with_options(text, match_case, false)
    }

    pub(crate) fn with_options(text: &str, match_case: bool, ignore_accents: bool) -> Result<Self> {
        Self::prepare(text, match_case, ignore_accents, false)
    }

    fn reflow(text: &str, match_case: bool, ignore_accents: bool) -> Result<Self> {
        Self::prepare(text, match_case, ignore_accents, true)
    }

    fn prepare(
        text: &str,
        match_case: bool,
        ignore_accents: bool,
        typography: bool,
    ) -> Result<Self> {
        let text = text.trim();
        if text.len() > MAX_QUERY_BYTES {
            return Err("Enter a search term of up to 256 UTF-8 bytes".into());
        }
        let unwrapped = without_soft_hyphens(text);
        let term = unwrapped.trim();
        if term.is_empty() {
            return Err("Enter a search term of up to 256 UTF-8 bytes".into());
        }
        let text = FoldedText::build(term, match_case, ignore_accents, typography, || false)
            .unwrap()
            .text;
        if text.trim().is_empty() {
            return Err("Enter at least one searchable character".into());
        }
        Ok(Self {
            text,
            match_case,
            ignore_accents,
            typography,
        })
    }

    fn use_original(&self, source: &str) -> bool {
        self.match_case
            && !self.ignore_accents
            && if self.typography {
                plain_ascii(source)
            } else {
                !source.contains(SOFT_HYPHEN)
            }
    }

    pub(crate) fn matches(&self, source: &str, limit: usize) -> Vec<Range<usize>> {
        self.matches_until(source, limit, || false)
    }

    fn matches_until(
        &self,
        source: &str,
        limit: usize,
        cancelled: impl Fn() -> bool,
    ) -> Vec<Range<usize>> {
        if self.use_original(source) {
            source
                .match_indices(&self.text)
                .take_while(|_| !cancelled())
                .take(limit)
                .map(|(start, found)| start..start + found.len())
                .collect()
        } else {
            let Some(folded) = FoldedText::build(
                source,
                self.match_case,
                self.ignore_accents,
                self.typography,
                &cancelled,
            ) else {
                return Vec::new();
            };
            folded
                .matches_while(&self.text, || !cancelled())
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
    query: fixed_pattern::FixedPattern,
    context_words: Option<u8>,
    pub(crate) next_page: usize,
    pub(crate) text_pages: usize,
    pub(crate) skipped: usize,
    pub(crate) retained_bytes: usize,
    pub(crate) memory_limited: bool,
}

#[cfg(feature = "kindle")]
pub(crate) struct PdfSearchHit {
    pub(crate) page: usize,
    pub(crate) snippet: String,
    pub(crate) matched: Range<usize>,
    pub(crate) bounds: Vec<PdfTextRect>,
}

#[cfg(feature = "kindle")]
pub(crate) struct FixedPageSearchHit {
    pub(crate) focus: Vec<PdfTextRect>,
    pub(crate) regions: Vec<PdfTextRect>,
    pub(crate) limited: bool,
}

#[cfg(feature = "kindle")]
fn matching_words<'a>(text: &'a PdfPageText, range: &Range<usize>) -> &'a [PdfTextSpan] {
    let first = text
        .words
        .partition_point(|word| word.bytes.end <= range.start);
    let rest = &text.words[first..];
    &rest[..rest.partition_point(|word| word.bytes.start < range.end)]
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
        Ok(Self {
            query: fixed_pattern::FixedPattern::new(query, match_case, ignore_accents)?,
            context_words: None,
            next_page: 0,
            text_pages: 0,
            skipped: 0,
            retained_bytes: 0,
            memory_limited: false,
        })
    }

    pub(crate) fn with_context_words(mut self, words: u8) -> Self {
        self.context_words = Some(words);
        self
    }

    #[cfg(test)]
    pub(crate) fn hits(
        &mut self,
        page: usize,
        text: &PdfPageText,
        limit: usize,
    ) -> Vec<PdfSearchHit> {
        let mut results = Vec::new();
        self.append_page(page, text, &mut results, limit);
        results
    }

    /// Keep one bounded list. An individual page can supply the entire result
    /// set, so avoid allocating a second list of its snippets and rectangles.
    pub(crate) fn append_page(
        &mut self,
        page: usize,
        text: &PdfPageText,
        results: &mut Vec<PdfSearchHit>,
        limit: usize,
    ) {
        if !text.text.trim().is_empty() {
            self.text_pages += 1;
        }
        for range in self.ranges(text, limit.saturating_sub(results.len())) {
            let words = matching_words(text, &range);
            let (snippet, matched) = snippet(&text.text, range, self.context_words);
            let allocation = size_of::<PdfSearchHit>()
                + snippet.capacity()
                + words.len() * size_of::<PdfTextRect>();
            if allocation > MAX_FIXED_RESULT_BYTES.saturating_sub(self.retained_bytes) {
                self.memory_limited = true;
                break;
            }
            self.retained_bytes += allocation;
            results.push(PdfSearchHit {
                page,
                snippet,
                matched,
                bounds: words.iter().map(|word| word.bounds).collect(),
            });
        }
    }

    fn ranges(&self, text: &PdfPageText, limit: usize) -> Vec<Range<usize>> {
        self.query.ranges(text, limit)
    }

    pub(crate) fn page_match(
        &self,
        text: &PdfPageText,
        direction: SearchDirection,
    ) -> Option<FixedPageSearchHit> {
        const MAX_PAGE_MATCHES: usize = 5_000;
        let ranges = self.ranges(text, MAX_PAGE_MATCHES + 1);
        let limited = ranges.len() > MAX_PAGE_MATCHES;
        let mut regions = Vec::new();
        let mut focus = None;
        for range in ranges.iter().take(MAX_PAGE_MATCHES) {
            let words = matching_words(text, range);
            if words.is_empty() {
                continue;
            }
            if focus.is_none() || direction == SearchDirection::Previous {
                focus = Some(words);
            }
            for word in words {
                if regions.last() != Some(&word.bounds) {
                    regions.push(word.bounds);
                }
            }
        }
        Some(FixedPageSearchHit {
            focus: focus?.iter().map(|word| word.bounds).collect(),
            regions,
            limited,
        })
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

fn plain_ascii(text: &str) -> bool {
    text.is_ascii() && !text.contains('\t') && !text.contains("  ")
}

// These are CREngine's explicit punctuation equivalences in ordinary rolling
// search. Preserve the source characters in storage, snippets and selections.
fn typographic_char(ch: char) -> Option<char> {
    match ch {
        '\t'
        | ' '
        | '\u{00a0}'
        | '\u{1680}'
        | '\u{2000}'..='\u{200a}'
        | '\u{202f}'
        | '\u{205f}'
        | '\u{3000}' => Some(' '),
        '\u{0027}' | '\u{02bc}' | '\u{055a}' | '\u{05f3}' | '\u{2018}' | '\u{2019}'
        | '\u{ff07}' => Some('\''),
        '-' | '\u{2212}' => Some('-'),
        // The stock provider handles U+1806 as a discretionary format control.
        ch if ch != '\u{1806}'
            && !ch.is_ascii()
            && CodePointMapData::<GeneralCategory>::new().get(ch)
                == GeneralCategory::DashPunctuation =>
        {
            Some('-')
        }
        _ => None,
    }
}

impl FoldedText {
    fn new(original: &str, match_case: bool) -> Self {
        Self::with_options(original, match_case, false)
    }

    fn with_options(original: &str, match_case: bool, ignore_accents: bool) -> Self {
        Self::build(original, match_case, ignore_accents, false, || false).unwrap()
    }

    fn build(
        original: &str,
        match_case: bool,
        ignore_accents: bool,
        typography: bool,
        cancelled: impl Fn() -> bool,
    ) -> Option<Self> {
        if cancelled() {
            return None;
        }
        let copy_ascii = if typography {
            plain_ascii(original)
        } else {
            original.is_ascii()
        };
        if copy_ascii {
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
            if typography && let Some(ascii) = typographic_char(ch) {
                let start = text.len();
                if ascii != ' ' || !text.ends_with(' ') {
                    text.push(ascii);
                }
                if text.len() - start != ch.len_utf8() {
                    edits.push(OffsetEdit {
                        source_end: source_start + ch.len_utf8(),
                        folded_start: start,
                        folded_end: text.len(),
                    });
                }
                continue;
            }
            let discretionary = ch == SOFT_HYPHEN || (typography && ch == '\u{1806}');
            if ch.is_ascii() || (match_case && !ignore_accents && !discretionary) {
                text.push(if match_case {
                    ch
                } else {
                    ch.to_ascii_lowercase()
                });
                continue;
            }
            let folded = if discretionary {
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

    #[cfg(test)]
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

fn snippet(text: &str, matched: Range<usize>, words: Option<u8>) -> (String, Range<usize>) {
    let before = &text[..matched.start];
    let after = &text[matched.end..];
    let words = words.map(usize::from);
    let first = if let Some(words) = words {
        crate::selection::bounded_context_range(before, words, true)
            .map_or(matched.start, |range| range.start)
    } else {
        before
            .char_indices()
            .rev()
            .nth(CONTEXT_CHARACTERS - 1)
            .map_or(0, |(index, _)| index)
    };
    // Regexps may select an entire paragraph. Keep the complete source anchors
    // while bounding the memory kept by the results list.
    let shown_end = text[matched.clone()]
        .char_indices()
        .nth(256)
        .map_or(matched.end, |(offset, _)| matched.start + offset);
    let last = if shown_end < matched.end {
        shown_end
    } else if let Some(words) = words {
        let length = crate::selection::bounded_context_range(after, words, false)
            .map_or(0, |range| range.end);
        // Short punctuation on the edge belongs with the final visible word.
        matched.end
            + length
            + after[length..]
                .char_indices()
                .take(8)
                .take_while(|(_, ch)| !ch.is_whitespace() && !ch.is_alphanumeric())
                .last()
                .map_or(0, |(index, ch)| index + ch.len_utf8())
    } else {
        after
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

    #[test]
    fn search_presentation_counts_unicode_words_and_keeps_match_byte_offsets() {
        let text = "một hai ba bốn năm sáu dấu! bảy tám chín mười mười một hai.";
        let start = text.find("dấu").unwrap();
        let end = start + "dấu".len();
        let (short, emphasis) = snippet(text, start..end, Some(1));
        assert_eq!(short, "…sáu dấu! bảy…");
        assert_eq!(&short[emphasis], "dấu");
        let (full, emphasis) = snippet(text, start..end, Some(5));
        assert_eq!(full, "…hai ba bốn năm sáu dấu! bảy tám chín mười mười…");
        assert_eq!(&full[emphasis], "dấu");
        let long = format!("{} dấu {}", "é".repeat(900), "á".repeat(900));
        let start = long.find("dấu").unwrap();
        let (bounded, emphasis) = snippet(&long, start..start + "dấu".len(), Some(50));
        assert!(bounded.chars().count() <= 1031);
        assert_eq!(&bounded[emphasis], "dấu");
    }

    #[test]
    fn ordinary_reflow_folds_publication_punctuation_without_changing_other_providers() {
        let text = "“L’ÉTÉ   coûte   dix”—l’été Co‑op co−op.";
        let qry = Query::reflow("L'ETE coute dix", false, true).unwrap();
        let hits = qry.matches(text, 8);
        assert_eq!(hits.len(), 1);
        assert_eq!(&text[hits[0].clone()], "L’ÉTÉ   coûte   dix");
        assert_eq!(
            qry.nearest(text, true, &AtomicBool::new(false)),
            Some(hits[0].clone())
        );
        assert!(
            Query::reflow("L'ETE coute dix", false, false)
                .unwrap()
                .matches(text, 8)
                .is_empty()
        );
        let case = Query::reflow("l'été", true, false)
            .unwrap()
            .matches(text, 8);
        assert_eq!(case.len(), 1);
        assert_eq!(&text[case[0].clone()], "l’été");
        let dashes = Query::reflow("co-op", false, false)
            .unwrap()
            .matches(text, 8);
        assert_eq!(
            dashes.iter().map(|r| &text[r.clone()]).collect::<Vec<_>>(),
            ["Co‑op", "co−op"]
        );
        assert!(
            Query::with_options("co-op", false, false)
                .unwrap()
                .matches(text, 8)
                .is_empty()
        );
        let pattern = pattern::Expression::new("co-op", false).unwrap();
        assert!(
            pattern
                .ranges(text, 0..text.len(), 8, false, &AtomicBool::new(false))
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            Query::reflow("A  B", true, false)
                .unwrap()
                .matches("A    B", 1),
            [0..6]
        );
        assert_eq!(
            Query::reflow("don't", true, false)
                .unwrap()
                .matches("don＇t", 1),
            [0..7]
        );
        assert_eq!(
            Query::reflow("”", true, false)
                .unwrap()
                .matches("\"", 1)
                .len(),
            0
        );
    }

    #[cfg(feature = "kindle")]
    #[test]
    fn fixed_word_edges_respect_normalized_spelling_and_literal_spaces_only() {
        let accepts = |pattern: &str, word: &str, case, accents| {
            let page = PdfPageText {
                text: word.into(),
                ..Default::default()
            };
            !PdfSearch::with_options(pattern, case, accents)
                .unwrap()
                .ranges(&page, 2)
                .is_empty()
        };
        assert!(accepts(" cafe ", "Ca\u{301}fé", false, true));
        assert!(!accepts(" cafe ", "Café,", false, true));
        assert!(accepts(" é", "Élan", false, false));
        assert!(!accepts(" é", "Élan", true, false));
        assert!(accepts("ss ", "Fuß", false, false));
        assert!(accepts("s ", "Fuß", false, false));
        assert!(accepts("ana ", "banana", false, false));
        assert!(accepts("  foo  ", "foo", false, false));
        assert!(accepts("\tfoo\t", "beforefooafter", false, false));
        assert!(accepts(" 潮 ", "潮", true, false));
        assert!(accepts(" 潮汐 ", "潮汐", true, false));
        assert!(accepts(" low tide ", "low tide", true, false));
        assert!(PdfSearch::new("  ", false).is_err());
        let transcript = PdfPageText {
            text: "  sandbar\nsand,\t sand".into(),
            ..Default::default()
        };
        let exact = PdfSearch::new(" sand ", false)
            .unwrap()
            .hits(2, &transcript, 10);
        assert_eq!(exact.len(), 1);
        assert!(exact[0].bounds.is_empty());
    }

    #[cfg(feature = "kindle")]
    #[test]
    fn an_exhausted_fixed_list_budget_keeps_the_previous_rows_intact() {
        let mut search = PdfSearch::new("tide", false).unwrap();
        let page = PdfPageText {
            text: "Tide at the pier".into(),
            ..Default::default()
        };
        let mut hits = Vec::new();
        search.append_page(0, &page, &mut hits, MAX_FIXED_SEARCH_HITS);
        assert_eq!(hits.len(), 1);
        search.retained_bytes = MAX_FIXED_RESULT_BYTES - 1;
        search.append_page(1, &page, &mut hits, MAX_FIXED_SEARCH_HITS);
        assert!(search.memory_limited);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].page, 0);
    }

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

    fn chapter_with_text(label: String, html: String) -> nearby::ChapterText {
        nearby::ChapterText {
            book: 1,
            label,
            href: "chapter.xhtml".into(),
            source: Source::parse(&format!("<html><body>{html}</body></html>")).ok(),
            flow: Default::default(),
            flow_config: Default::default(),
        }
    }

    #[test]
    fn whole_list_stops_at_five_thousand_or_the_retained_result_budget() {
        let cancel = AtomicBool::new(false);
        let dense = format!("<p>{}</p>", "x ".repeat(MAX_REFLOW_SEARCH_HITS + 2));
        let mut search = Search::with_case("x", true).unwrap();
        let results = search
            .step_loaded(
                chapter_with_text("Chapter".into(), dense.clone()),
                2,
                &cancel,
            )
            .unwrap();
        assert_eq!(results.len(), MAX_REFLOW_SEARCH_HITS);
        assert!(search.limit_reached() && search.is_finished());
        assert!(!search.memory_limited());

        let mut search = Search::with_case("x", true).unwrap();
        let results = search
            .step_loaded(chapter_with_text("c".repeat(128 * 1024), dense), 2, &cancel)
            .unwrap();
        assert!(!results.is_empty() && results.len() < 64);
        assert!(!search.limit_reached());
        assert!(search.memory_limited() && search.is_finished());
        assert!(search.retained_bytes <= MAX_RESULT_BYTES);
    }

    #[test]
    fn whole_list_spans_inline_styles_and_soft_hyphens_with_original_offsets() {
        let html = "<p>Cloud <i>caf&#x65;&#x301;</i> sea&#xad;bird.</p><p>Wind follows.</p>";
        let cancel = AtomicBool::new(false);
        let search = |query, accents| {
            Search::with_options(query, true, accents)
                .unwrap()
                .step_loaded(chapter_with_text("Chapter".into(), html.into()), 1, &cancel)
                .unwrap()
        };
        let found = search("Cloud cafe seabird", true);
        assert_eq!(found.len(), 1);
        let hit = &found[0];
        let source = "Cloud cafe\u{301} sea\u{ad}bird";
        assert_eq!(&hit.snippet[hit.matched.clone()], source);
        assert_eq!(hit.end.offset - hit.anchor.offset, source.len());
        assert!(search("Cloud cafe seabird", false).is_empty());
        assert!(search("bird. Wind", false).is_empty());
    }

    #[test]
    fn caps_dense_hits_without_scanning_past_the_current_chapter() {
        let fixture = crate::test_fixture("long-chapter.epub");
        let mut book = Book::open(&fixture).unwrap();
        let mut search = Search::new("observation").unwrap().at_most(MAX_SEARCH_HITS);
        let hits = search.step(&mut book).unwrap();
        assert_eq!(hits.len(), MAX_SEARCH_HITS);
        assert!(search.limit_reached());
        assert!(search.is_finished());
    }
}
