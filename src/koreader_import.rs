//! Read-only import of KOReader's JSON annotation export.
//!
//! The exporter and clipping fields were verified against KOReader revision
//! 042d9b39d4835408121970229cc37624e4f794bc in `plugins/exporter.koplugin`.
//! The supported inputs are a single exported book or an envelope of documents.
//! https://github.com/koreader/koreader/blob/042d9b39d4835408121970229cc37624e4f794bc/plugins/exporter.koplugin/target/json.lua
//! https://github.com/koreader/koreader/blob/042d9b39d4835408121970229cc37624e4f794bc/plugins/exporter.koplugin/clip.lua

use serde::de::{self, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use std::fmt;
use std::marker::PhantomData;
use std::ops::Range;

use crate::Anchor;
use crate::epub::Book;
use crate::selection::TextSelection;
use crate::source::Source;

pub const MAX_EXPORT_BYTES: usize = 2 * 1024 * 1024;
const MAX_DOCUMENTS: usize = 128;
const MAX_ENTRIES: usize = 4096;
const MAX_LABEL_BYTES: usize = 1024;
const MAX_PAGE_LABEL_BYTES: usize = 128;
const MAX_QUOTE_BYTES: usize = 8192;
const MAX_NOTE_BYTES: usize = 4096;
const MAX_SCAN_BYTES: usize = 16 * 1024 * 1024;
const MAX_SCAN_CHAPTERS: usize = 1024;

#[derive(Debug, PartialEq, Eq)]
pub struct AnnotationExport {
    pub created_on: Option<i64>,
    pub documents: Vec<ExportDocument>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ExportDocument {
    pub title: String,
    pub author: Option<String>,
    pub number_of_pages: Option<u32>,
    pub entries: Vec<ExportEntry>,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
pub struct ExportEntry {
    #[serde(default, deserialize_with = "read_drawer")]
    pub drawer: crate::highlight_style::HighlightStyle,
    pub text: String,
    pub note: Option<String>,
    pub page: Option<ExportPage>,
    pub chapter: Option<String>,
    pub time: Option<i64>,
}

fn read_drawer<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<crate::highlight_style::HighlightStyle, D::Error> {
    let name = Option::<String>::deserialize(deserializer)?;
    Ok(crate::highlight_style::HighlightStyle::from_drawer(
        name.as_deref().unwrap_or(""),
    ))
}

/// Page numbering is retained for preview and has no role in text matching.
#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum ExportPage {
    Number(u32),
    Label(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuoteMatch {
    Unique(TextSelection),
    Ambiguous,
    Missing,
    /// At least one chapter was unreadable or the scan reached its bounds.
    Incomplete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportError {
    TooLarge,
    InvalidExport,
    EmptyQuote,
    QuoteTooLarge,
    FixedLayout,
    ChangedBook,
}

impl fmt::Display for ImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::TooLarge => "This KOReader annotation export exceeds Reader's import limits.",
            Self::InvalidExport => {
                "This file does not contain a supported KOReader JSON annotation export."
            }
            Self::EmptyQuote => "This annotation has no text to match.",
            Self::QuoteTooLarge => "This annotation exceeds Reader's quotation limit.",
            Self::FixedLayout => "Quote matching requires a reflowable book.",
            Self::ChangedBook => "The selected book changed during the quote scan.",
        })
    }
}

impl std::error::Error for ImportError {}

#[derive(Deserialize)]
struct RawExport {
    created_on: Option<i64>,
    documents: Option<Bounded<RawDocument, MAX_DOCUMENTS>>,
    title: Option<String>,
    author: Option<String>,
    number_of_pages: Option<u32>,
    entries: Option<Bounded<ExportEntry, MAX_ENTRIES>>,
}

#[derive(Deserialize)]
struct RawDocument {
    title: String,
    author: Option<String>,
    number_of_pages: Option<u32>,
    entries: Bounded<ExportEntry, MAX_ENTRIES>,
}

struct Bounded<T, const LIMIT: usize>(Vec<T>);

impl<'de, T: Deserialize<'de>, const LIMIT: usize> Deserialize<'de> for Bounded<T, LIMIT> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ArrayVisitor<T, const LIMIT: usize>(PhantomData<T>);
        impl<'de, T: Deserialize<'de>, const LIMIT: usize> Visitor<'de> for ArrayVisitor<T, LIMIT> {
            type Value = Bounded<T, LIMIT>;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "an array with at most {LIMIT} items")
            }

            fn visit_seq<A: SeqAccess<'de>>(
                self,
                mut sequence: A,
            ) -> Result<Self::Value, A::Error> {
                let mut items = Vec::new();
                while items.len() < LIMIT {
                    let Some(item) = sequence.next_element()? else {
                        return Ok(Bounded(items));
                    };
                    items.push(item);
                }
                if sequence.next_element::<de::IgnoredAny>()?.is_some() {
                    return Err(de::Error::invalid_length(LIMIT + 1, &self));
                }
                Ok(Bounded(items))
            }
        }
        deserializer.deserialize_seq(ArrayVisitor::<T, LIMIT>(PhantomData))
    }
}

/// Parse the export as data. Filesystem paths, location expressions, and other
/// unrecognized fields are ignored by the deserializer.
pub fn parse(bytes: &[u8]) -> Result<AnnotationExport, ImportError> {
    if bytes.len() > MAX_EXPORT_BYTES {
        return Err(ImportError::TooLarge);
    }
    let bytes = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes);
    if bytes
        .iter()
        .copied()
        .find(|byte| !byte.is_ascii_whitespace())
        != Some(b'{')
    {
        return Err(ImportError::InvalidExport);
    }
    let raw: RawExport = serde_json::from_slice(bytes).map_err(|_| ImportError::InvalidExport)?;
    let documents = match (raw.documents, raw.title, raw.entries) {
        (Some(Bounded(documents)), None, None) if !documents.is_empty() => documents,
        (None, Some(title), Some(entries)) => vec![RawDocument {
            title,
            author: raw.author,
            number_of_pages: raw.number_of_pages,
            entries,
        }],
        _ => return Err(ImportError::InvalidExport),
    };
    let total = documents
        .iter()
        .map(|document| document.entries.0.len())
        .sum::<usize>();
    if total > MAX_ENTRIES {
        return Err(ImportError::TooLarge);
    }
    let documents = documents
        .into_iter()
        .map(ExportDocument::try_from)
        .collect::<Result<_, _>>()?;
    Ok(AnnotationExport {
        created_on: raw.created_on,
        documents,
    })
}

impl TryFrom<RawDocument> for ExportDocument {
    type Error = ImportError;

    fn try_from(raw: RawDocument) -> Result<Self, ImportError> {
        let RawDocument {
            title,
            author,
            number_of_pages,
            entries: Bounded(entries),
        } = raw;
        validate_field(&title, MAX_LABEL_BYTES)?;
        if title.trim().is_empty() {
            return Err(ImportError::InvalidExport);
        }
        if let Some(author) = &author {
            validate_field(author, MAX_LABEL_BYTES)?;
        }
        for entry in &entries {
            validate_field(&entry.text, MAX_QUOTE_BYTES)?;
            if let Some(note) = &entry.note {
                validate_field(note, MAX_NOTE_BYTES)?;
            }
            if let Some(chapter) = &entry.chapter {
                validate_field(chapter, MAX_LABEL_BYTES)?;
            }
            if let Some(ExportPage::Label(page)) = &entry.page {
                validate_field(page, MAX_PAGE_LABEL_BYTES)?;
            }
        }
        Ok(Self {
            title,
            author,
            number_of_pages,
            entries,
        })
    }
}

fn validate_field(text: &str, limit: usize) -> Result<(), ImportError> {
    if text.len() > limit {
        return Err(ImportError::TooLarge);
    }
    if text
        .chars()
        .any(|ch| ch.is_control() && !matches!(ch, '\t' | '\r' | '\n'))
    {
        return Err(ImportError::InvalidExport);
    }
    Ok(())
}

/// Match one quotation a chapter at a time so a caller can cancel between reads.
/// Match text after folding whitespace and ignoring discretionary soft hyphens.
pub struct QuoteResolver {
    quote: String,
    book: Option<(u64, usize)>,
    next_chapter: usize,
    bytes_scanned: usize,
    incomplete: bool,
    seen_match: bool,
    first: Option<TextSelection>,
    outcome: Option<QuoteMatch>,
}

impl QuoteResolver {
    pub fn new(quote: &str) -> Result<Self, ImportError> {
        if quote.len() > MAX_QUOTE_BYTES {
            return Err(ImportError::QuoteTooLarge);
        }
        let plain = crate::search::without_soft_hyphens(quote);
        let quote = canonical_text(plain.split_whitespace());
        if quote.is_empty() {
            return Err(ImportError::EmptyQuote);
        }
        Ok(Self {
            quote,
            book: None,
            next_chapter: 0,
            bytes_scanned: 0,
            incomplete: false,
            seen_match: false,
            first: None,
            outcome: None,
        })
    }

    pub fn scanned_chapters(&self) -> usize {
        self.next_chapter
    }

    /// Advance by at most one chapter. `None` means more chapters remain.
    pub fn step(&mut self, book: &mut Book) -> Result<Option<QuoteMatch>, ImportError> {
        if book.is_fixed_layout() {
            return Err(ImportError::FixedLayout);
        }
        let publication = (book.revision(), book.chapters.len());
        if *self.book.get_or_insert(publication) != publication {
            return Err(ImportError::ChangedBook);
        }
        if let Some(outcome) = &self.outcome {
            return Ok(Some(outcome.clone()));
        }
        let index = self.next_chapter;
        if index == publication.1 {
            return Ok(Some(self.finish()));
        }
        if index == MAX_SCAN_CHAPTERS {
            self.incomplete = true;
            return Ok(Some(self.finish()));
        }

        self.next_chapter += 1;
        let Ok(xhtml) = book.chapter(index) else {
            self.incomplete = true;
            return Ok((self.next_chapter == publication.1).then(|| self.finish()));
        };
        self.bytes_scanned += xhtml.len();
        if self.bytes_scanned > MAX_SCAN_BYTES {
            self.incomplete = true;
            return Ok(Some(self.finish()));
        }
        // ODT has a dedicated extraction mode for explicit text:s whitespace.
        // Use the same source model as the reader, so imported byte offsets
        // refer to exactly the text and chapter being rendered.
        let source = if book.is_odt() {
            book.chapter_source(&book.chapters[index].href, &xhtml)
        } else {
            Source::parse(&xhtml)
        };
        match source {
            Ok(source) => self.scan(&source, publication.0, &book.chapters[index].href),
            Err(_) => self.incomplete = true,
        }
        if let Some(outcome) = &self.outcome {
            return Ok(Some(outcome.clone()));
        }
        Ok((self.next_chapter == publication.1).then(|| self.finish()))
    }

    fn scan(&mut self, source: &Source, book: u64, chapter: &str) {
        let text = canonical_text(words(source).map(|(_, _, word)| word));
        let Some((span, repeated)) = crate::search::find_quote(&text, &self.quote) else {
            return;
        };
        if self.seen_match || repeated {
            self.outcome = Some(QuoteMatch::Ambiguous);
            return;
        }
        self.seen_match = true;
        self.first = match selection(source, book, chapter, span) {
            Some(mark) => Some(mark),
            None => {
                self.incomplete = true;
                None
            }
        };
    }

    fn finish(&mut self) -> QuoteMatch {
        let outcome = if self.incomplete {
            QuoteMatch::Incomplete
        } else {
            self.first
                .take()
                .map_or(QuoteMatch::Missing, QuoteMatch::Unique)
        };
        self.outcome = Some(outcome.clone());
        outcome
    }
}

/// Convenience for a caller already running a complete scan off the UI thread.
pub fn resolve_quote(book: &mut Book, quote: &str) -> Result<QuoteMatch, ImportError> {
    let mut scan = QuoteResolver::new(quote)?;
    loop {
        if let Some(outcome) = scan.step(book)? {
            return Ok(outcome);
        }
    }
}

fn canonical_text<'a>(words: impl Iterator<Item = &'a str>) -> String {
    let mut text = String::new();
    for word in words {
        if !text.is_empty() {
            text.push(' ');
        }
        text.push_str(word);
    }
    text
}

fn words(source: &Source) -> impl Iterator<Item = (usize, usize, &str)> {
    source.blocks.iter().enumerate().flat_map(|(index, block)| {
        let mut offset = 0;
        block
            .text
            .split_whitespace()
            .map(move |word| {
                let start = offset + block.text[offset..].find(word).unwrap();
                offset = start + word.len();
                (index, start, word)
            })
            .filter(|(_, _, word)| word.chars().any(|ch| ch != '\u{ad}'))
    })
}

fn selection(
    source: &Source,
    book: u64,
    chapter: &str,
    canonical: Range<usize>,
) -> Option<TextSelection> {
    let mut start = None;
    let mut end = None;
    let mut offset = 0;
    for (index, original, word) in words(source) {
        let word_end = offset + word.len();
        if (offset..word_end).contains(&canonical.start) {
            start = Some((index, original + canonical.start - offset));
        }
        if (offset + 1..=word_end).contains(&canonical.end) {
            end = Some((index, original + canonical.end - offset));
            break;
        }
        offset = word_end + 1;
    }
    let ((first, first_byte), (last, last_byte)) = (start?, end?);
    let mut quote = String::new();
    for (offset, block) in source.blocks[first..=last].iter().enumerate() {
        let index = first + offset;
        let text = &block.text;
        let start = if index == first { first_byte } else { 0 };
        let end = if index == last { last_byte } else { text.len() };
        let selected = text.get(start..end)?;
        if selected.is_empty() {
            continue;
        }
        if !quote.is_empty() {
            quote.push('\n');
        }
        if quote.len() + selected.len() > MAX_QUOTE_BYTES {
            return None;
        }
        quote.push_str(selected);
    }
    let anchor = |index: usize, offset| Anchor {
        illustration: None,
        book,
        chapter: chapter.to_owned(),
        revision: source.revision,
        block: source.blocks[index].key,
        offset,
    };
    Some(TextSelection {
        start: anchor(first, first_byte),
        end: anchor(last, last_byte),
        text: quote,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const ONE_BOOK: &[u8] = br#"{
        "title": "Harbor Notes",
        "author": "A. Reader",
        "created_on": 1769020001,
        "version": "v2026.10",
        "file": "/synthetic/private/Harbor Notes.epub",
        "md5sum": "4860d04621c0abc2e0e5df9be42a0579",
        "number_of_pages": 42,
        "entries": [
            {"sort": "highlight", "page": 14, "time": 1769019000, "chapter": "The harbor",
             "text": "The boats turned together.", "note": "Review on Friday.\nThen revisit chapter 2.",
             "pn_xp": "return os.execute('fixture')", "drawer": "strikeout", "color": "yellow"},
            {"sort": "highlight", "page": "iv", "text": "A small map."}
        ]
    }"#;

    #[test]
    fn parses_export_metadata_quotes_notes_and_page_labels() {
        let import = parse(ONE_BOOK).unwrap();
        assert_eq!(import.created_on, Some(1769020001));
        assert_eq!(import.documents.len(), 1);
        let book = &import.documents[0];
        assert_eq!(book.title, "Harbor Notes");
        assert_eq!(book.author.as_deref(), Some("A. Reader"));
        assert_eq!(book.number_of_pages, Some(42));
        assert_eq!(book.entries[0].text, "The boats turned together.");
        assert_eq!(
            book.entries[0].note.as_deref(),
            Some("Review on Friday.\nThen revisit chapter 2.")
        );
        assert_eq!(book.entries[0].page, Some(ExportPage::Number(14)));
        assert_eq!(book.entries[0].chapter.as_deref(), Some("The harbor"));
        assert_eq!(book.entries[0].time, Some(1769019000));
        assert_eq!(
            book.entries[0].drawer,
            crate::highlight_style::HighlightStyle::Strikethrough
        );
        assert_eq!(
            book.entries[1].drawer,
            crate::highlight_style::HighlightStyle::Lighten
        );
        assert_eq!(
            book.entries[1].page,
            Some(ExportPage::Label("iv".to_owned()))
        );
        assert!(!format!("{import:?}").contains("/synthetic/private"));
        assert!(!format!("{import:?}").contains("os.execute"));
    }

    #[test]
    fn parses_multi_book_exports_and_rejects_lua_or_unrelated_json() {
        let export = br#"{
            "documents": [
                {"title":"Wind", "author":null, "entries":[{"text":"A reading."}]},
                {"title":"Rain", "entries":[{"text":"", "image":{"hash":"a"}}]}
            ], "created_on":100
        }"#;
        let mut bom = b"\xef\xbb\xbf".to_vec();
        bom.extend(export);
        let import = parse(&bom).unwrap();
        assert_eq!(import.documents.len(), 2);
        assert_eq!(import.documents[0].author, None);
        assert_eq!(import.documents[1].entries[0].text, "");
        assert!(matches!(
            QuoteResolver::new(&import.documents[1].entries[0].text),
            Err(ImportError::EmptyQuote)
        ));
        for unsupported in [
            b"return { annotations = {} }".as_slice(),
            br#"{"title":"Harbor Notes", "annotations":[]}"#,
            br#"{"title":"Harbor Notes", "entries":[{}]}"#,
            br#"{"title":"Harbor Notes", "entries":[], "documents":[]}"#,
            br#"[null, null, "Harbor Notes", null, null, [{"text":"A reading."}]]"#,
        ] {
            assert_eq!(parse(unsupported), Err(ImportError::InvalidExport));
        }
    }

    #[test]
    fn bounds_export_size_entries_and_note_fields() {
        let too_big = vec![b' '; MAX_EXPORT_BYTES + 1];
        assert_eq!(parse(&too_big), Err(ImportError::TooLarge));
        let entries = vec![serde_json::json!({"text":"x"}); MAX_ENTRIES / 2 + 1];
        let multi = serde_json::json!({"documents":[
            {"title":"A", "entries":entries}, {"title":"B", "entries":entries},
        ]});
        assert_eq!(
            parse(&serde_json::to_vec(&multi).unwrap()),
            Err(ImportError::TooLarge)
        );
        let long_note = serde_json::json!({"title":"B", "entries":[{"text":"Yes", "note":"x".repeat(MAX_NOTE_BYTES + 1)}]});
        assert_eq!(
            parse(&serde_json::to_vec(&long_note).unwrap()),
            Err(ImportError::TooLarge)
        );
        let single = serde_json::json!({"title":"A", "entries":vec![serde_json::json!({"text":"x"}); MAX_ENTRIES + 1]});
        assert_eq!(
            parse(&serde_json::to_vec(&single).unwrap()),
            Err(ImportError::InvalidExport)
        );
    }

    #[test]
    fn cross_block_quotes_preserve_original_unicode_offsets_and_wording() {
        let source = Source::parse(
            "<html><head></head><body><p>Near 柴 café</p><p>مرحبا tide.</p></body></html>",
        )
        .unwrap();
        let mut scan = QuoteResolver::new("café \r\n\t مرحبا").unwrap();
        scan.scan(&source, 712, "shore.xhtml");
        let QuoteMatch::Unique(mark) = scan.finish() else {
            panic!("expected one quote match")
        };
        assert_eq!(mark.text, "café\nمرحبا");
        assert_eq!(
            mark.start,
            Anchor {
                illustration: None,
                book: 712,
                chapter: "shore.xhtml".to_owned(),
                revision: source.revision,
                block: source.blocks[0].key,
                offset: "Near 柴 ".len()
            }
        );
        assert_eq!(
            mark.end,
            Anchor {
                illustration: None,
                block: source.blocks[1].key,
                offset: "مرحبا".len(),
                ..mark.start.clone()
            }
        );
        let overlap = Source::parse("<html><body><p>aaaa</p></body></html>").unwrap();
        let mut ambiguous = QuoteResolver::new("aaa").unwrap();
        ambiguous.scan(&overlap, 55, "overlap.xhtml");
        assert_eq!(ambiguous.outcome, Some(QuoteMatch::Ambiguous));
    }

    #[test]
    fn imports_soft_hyphen_quotes_with_original_positions_and_detects_ambiguity() {
        let source =
            Source::parse("<html><body><p>Near Über&#173;gang</p><p>at dusk.</p></body></html>")
                .unwrap();
        let mut scan = QuoteResolver::new("Übergang at dusk").unwrap();
        scan.scan(&source, 712, "shore.xhtml");
        let QuoteMatch::Unique(mark) = scan.finish() else {
            panic!("expected one quote match")
        };
        assert_eq!(mark.start.offset, "Near ".len());
        assert_eq!(mark.end.offset, "at dusk".len());
        assert_eq!(mark.text, "Über\u{ad}gang\nat dusk");

        let source =
            Source::parse("<html><body><p>Über&#173;gang Übergang</p></body></html>").unwrap();
        let mut scan = QuoteResolver::new("Über\u{ad}gang").unwrap();
        scan.scan(&source, 712, "shore.xhtml");
        assert_eq!(scan.outcome, Some(QuoteMatch::Ambiguous));
    }

    #[test]
    fn walks_the_reading_spine_and_reports_missing_ambiguous_and_incomplete() {
        let mut book = Book::open(&crate::test_fixture("field-notes-epub3.epub")).unwrap();
        let mut step = QuoteResolver::new("Return to the second tide observation").unwrap();
        assert_eq!(step.step(&mut book).unwrap(), None);
        assert_eq!(step.scanned_chapters(), 1);
        let Some(QuoteMatch::Unique(mark)) = step.step(&mut book).unwrap() else {
            panic!("expected the dusk chapter")
        };
        assert_eq!(mark.start.chapter, book.chapters[1].href);
        assert_eq!(mark.end.book, book.revision());
        assert_eq!(
            resolve_quote(&mut book, "The tide keeps its own time.").unwrap(),
            QuoteMatch::Ambiguous
        );
        assert_eq!(
            resolve_quote(&mut book, "the tide keeps its OWN time.").unwrap(),
            QuoteMatch::Missing
        );

        book.chapters[1].href = "unavailable.xhtml".to_owned();
        assert_eq!(
            resolve_quote(&mut book, "Morning observation.").unwrap(),
            QuoteMatch::Incomplete
        );
    }
}
