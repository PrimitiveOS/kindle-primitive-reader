//! Offline search over the same normalized text and anchors used by the reader.

use std::borrow::Cow;
use std::ops::Range;

use icu_casemap::{CaseMapper, CaseMapperBorrowed};
use icu_normalizer::DecomposingNormalizer;
use icu_properties::{CodePointMapData, props::GeneralCategory};

use crate::epub::Book;
use crate::source::Source;
use crate::{Anchor, Result};

#[cfg(feature = "kindle")]
use crate::fixed_text::{PdfPageText, PdfTextRect};

pub const MAX_SEARCH_HITS: usize = 100;
pub(crate) const MAX_QUERY_BYTES: usize = 256;
const CONTEXT_CHARACTERS: usize = 28;
const CASE_MAPPER: CaseMapperBorrowed<'static> = CaseMapper::new();
const SOFT_HYPHEN: char = '\u{ad}';

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
    /// The match's UTF-8 byte range within `snippet`.
    pub matched: Range<usize>,
}

/// A bounded search task. Each step processes at most one spine document.
/// The caller can stop or replace the task between steps. Search borrows
/// the book only during the step, so a device can release the storage lease.
pub struct Search {
    query: Query,
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
        Ok(Self {
            query: Query::with_options(query, match_case, ignore_accents)?,
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
        let publication = (book.revision(), book.chapters.len());
        if *self.publication.get_or_insert(publication) != publication {
            self.stopped = true;
            return Err("The publication changed during the search".into());
        }
        let chapter = self.next_chapter;
        let Some(info) = book.chapters.get(chapter).cloned() else {
            return Ok(Vec::new());
        };
        self.next_chapter += 1;
        let source = match book
            .chapter(chapter)
            .and_then(|xhtml| Source::parse(&xhtml))
        {
            Ok(source) => source,
            Err(_) => {
                self.skipped += 1;
                return Ok(Vec::new());
            }
        };

        let mut hits = Vec::new();
        for block in source.blocks {
            for range in self.query.matches(&block.text, MAX_SEARCH_HITS - self.hits) {
                let offset = range.start;
                let end = range.end;
                let (snippet, matched) = snippet(&block.text, range);
                hits.push(SearchHit {
                    chapter,
                    chapter_title: info.label.clone(),
                    anchor: Anchor {
                        book: publication.0,
                        chapter: info.href.clone(),
                        revision: source.revision,
                        block: block.key,
                        offset,
                    },
                    end: Anchor {
                        book: publication.0,
                        chapter: info.href.clone(),
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

struct Query {
    text: String,
    match_case: bool,
    ignore_accents: bool,
}

impl Query {
    #[cfg(test)]
    fn new(text: &str, match_case: bool) -> Result<Self> {
        Self::with_options(text, match_case, false)
    }

    fn with_options(text: &str, match_case: bool, ignore_accents: bool) -> Result<Self> {
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

    fn matches(&self, source: &str, limit: usize) -> Vec<Range<usize>> {
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
    pub(crate) region: Option<PdfTextRect>,
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
            query: Query::with_options(query, match_case, ignore_accents)?,
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
        self.query
            .matches(&text.text, limit)
            .into_iter()
            .map(|range| {
                let region = text
                    .words
                    .iter()
                    .filter(|word| word.bytes.start < range.end && word.bytes.end > range.start)
                    .map(|word| word.bounds)
                    .reduce(|a, b| PdfTextRect {
                        x0: a.x0.min(b.x0),
                        y0: a.y0.min(b.y0),
                        x1: a.x1.max(b.x1),
                        y1: a.y1.max(b.y1),
                    });
                PdfSearchHit {
                    page,
                    snippet: snippet(&text.text, range).0,
                    region,
                }
            })
            .collect()
    }
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
        if original.is_ascii() {
            return Self {
                text: if match_case {
                    original.to_owned()
                } else {
                    original.to_ascii_lowercase()
                },
                edits: Vec::new(),
            };
        }
        let mut text = String::with_capacity(original.len());
        let mut edits = Vec::new();
        let mut scalar = [0; 4];
        for (source_start, ch) in original.char_indices() {
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
        Self { text, edits }
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
        let mut cursor = 0;
        std::iter::from_fn(move || {
            loop {
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
    let last = text[matched.end..]
        .char_indices()
        .nth(CONTEXT_CHARACTERS)
        .map_or(text.len(), |(index, _)| matched.end + index);
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
    (preview, match_start..match_start + matched.len())
}

#[cfg(test)]
mod tests {

    use super::*;

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
