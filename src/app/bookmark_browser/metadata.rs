//! Metadata kept by edition identity for live books and retained annotation records.
use super::{
    Book, BookState, Change, Command, Item, Page, Reader, Result, Screen, item, with_storage,
};
use crate::library::{DocumentFormat, Library};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::time::{Duration, Instant};

const MAX_CACHE_BYTES: u64 = 8 * 1024 * 1024;
const MAX_DESCRIPTION: usize = 8192;
const MAX_VALUE: usize = 512;
const MAX_VALUES: usize = 128;

impl Reader {
    /// Resolve a result row against its own book, not the currently open one.
    pub(in crate::app) fn bookmark_browser_row_colors(
        &self,
        action: &crate::app::Action,
    ) -> Option<(
        crate::highlight_style::HighlightColor,
        crate::highlight_style::HighlightColor,
    )> {
        let crate::app::Action::BookmarkBrowser(Command::Inspect(key)) = action else {
            return None;
        };
        let state = self.library.state(key.book)?;
        let color = match key.locator {
            super::Locator::Bookmark(_) => return None,
            super::Locator::Reflow(index) => state.highlights.get(index)?.color,
            super::Locator::Fixed(index) => state.pdf_highlights.get(index)?.color,
        };
        Some((
            color,
            state
                .highlight_color
                .unwrap_or(self.controls.highlight_color),
        ))
    }
}

pub(super) fn is_cbz_book(book: &BookState) -> bool {
    book.path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("cbz"))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::app) enum Property {
    Series,
    Language,
    Keywords,
}

impl Property {
    pub(super) const fn title(self) -> &'static str {
        match self {
            Self::Series => "Choose a series",
            Self::Language => "Choose a language",
            Self::Keywords => "Choose a keyword",
        }
    }

    fn unknown(self) -> &'static str {
        match self {
            Self::Series => "Unknown series",
            Self::Language => "Unknown language",
            Self::Keywords => "No keywords",
        }
    }

    fn values<'a>(self, snapshot: &'a Snapshot) -> Vec<&'a str> {
        match self {
            Self::Series => snapshot.series.iter().map(String::as_str).collect(),
            Self::Language => snapshot.language.iter().map(String::as_str).collect(),
            Self::Keywords => snapshot.keywords.iter().map(String::as_str).collect(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct Snapshot {
    pub(super) series: Vec<String>,
    pub(super) language: Option<String>,
    pub(super) keywords: Vec<String>,
    pub(super) description: Option<String>,
    pub(super) has_cover: bool,
    pub(super) chapters: HashMap<String, usize>,
    pub(super) fixed: bool,
    // Older cache entries predate ComicInfo. Refresh those CBZs once, while
    // retaining the old snapshot if the source is temporarily unavailable.
    cbz_info_scanned: bool,
}

pub(super) fn backup_value(bytes: &[u8]) -> std::io::Result<serde_json::Value> {
    let books: BTreeMap<u64, Snapshot> = serde_json::from_slice(bytes)?;
    let text = |s: &String| s.len() <= MAX_VALUE && !s.chars().any(char::is_control);
    if bytes.len() as u64 > MAX_CACHE_BYTES
        || books.len() > 16_384
        || books.values().any(|book| {
            book.series.len() > MAX_VALUES
                || book.keywords.len() > MAX_VALUES
                || !book.series.iter().chain(&book.keywords).all(text)
                || book.language.as_ref().is_some_and(|s| !text(s))
                || book
                    .description
                    .as_ref()
                    .is_some_and(|s| s.len() > MAX_DESCRIPTION)
                || book.chapters.len() > 32_768
                || book
                    .chapters
                    .iter()
                    .any(|(path, &index)| path.len() > 4096 || index > 1_000_000)
        })
    {
        return Err(std::io::Error::other(
            "The retained book metadata backup is invalid",
        ));
    }
    serde_json::to_value(books).map_err(std::io::Error::other)
}

#[derive(Default)]
pub(super) struct Catalog {
    path: PathBuf,
    books: BTreeMap<u64, Snapshot>,
    pending: VecDeque<u64>,
    failed: HashSet<u64>,
    next: Option<Instant>,
    dirty: bool,
}

impl Catalog {
    pub(super) fn load(path: PathBuf) -> Self {
        let books = super::super::read_import_file(&path, MAX_CACHE_BYTES)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Self {
            path,
            books,
            ..Self::default()
        }
    }

    pub(super) fn get(&self, id: u64) -> Option<&Snapshot> {
        self.books.get(&id)
    }

    pub(super) fn queue(&mut self, ids: impl IntoIterator<Item = (u64, bool)>) {
        let ids: BTreeSet<_> = ids
            .into_iter()
            .filter(|(id, cbz)| {
                !self.failed.contains(id)
                    && self
                        .books
                        .get(id)
                        .is_none_or(|snapshot| *cbz && !snapshot.cbz_info_scanned)
            })
            .map(|(id, _)| id)
            .collect();
        self.pending = ids.into_iter().collect();
        self.next = (!self.pending.is_empty()).then(Instant::now);
    }

    pub(super) fn loading(&self) -> bool {
        !self.pending.is_empty()
    }

    pub(super) fn save(&mut self) -> Result<()> {
        if self.dirty {
            let bytes = serde_json::to_vec(&self.books)?;
            if bytes.len() as u64 > MAX_CACHE_BYTES {
                return Err("The saved book metadata exceeds the cache size limit".into());
            }
            primitive_app::save(&self.path, &bytes)?;
            self.dirty = false;
        }
        Ok(())
    }
}

fn value(text: &str, limit: usize) -> Option<String> {
    let value: String = text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .filter(|c| !c.is_control())
        .take(limit)
        .collect();
    (!value.is_empty()).then_some(value)
}

fn values<'a>(input: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    input
        .into_iter()
        .filter_map(|text| value(text, MAX_VALUE))
        .filter(|text| seen.insert(text.clone()))
        .take(MAX_VALUES)
        .collect()
}

fn description(text: &str) -> Option<String> {
    let text = &text[..text.floor_char_boundary(MAX_DESCRIPTION.min(text.len()))];
    let text: String = text
        .chars()
        .filter(|c| !c.is_control() || matches!(c, '\n' | '\t'))
        .collect();
    let plain = crate::mobi::html_xhtml(&text)
        .ok()
        .and_then(|html| crate::source::Source::parse(&html).ok())
        .map(|source| {
            source
                .blocks
                .iter()
                .filter(|block| !block.text.is_empty())
                .map(|block| block.text.as_str())
                .collect::<Vec<_>>()
                .join("\n\n")
        });
    let result = plain.filter(|text| !text.is_empty()).unwrap_or(text);
    (!result.trim().is_empty()).then_some(result)
}

fn read_snapshot(library: &Library, state: &BookState) -> Result<Snapshot> {
    let entry = library.locate(state)?;
    let absolute = library.absolute_path(&entry.path)?;
    let mut snapshot = Snapshot::default();
    if entry.format == DocumentFormat::Pdf {
        let bytes = crate::fixed_session::pdf_bytes(&absolute, Some(entry.size))?;
        let pdf = crate::fixed::Pdf::open(bytes)?;
        let (subject, keywords, language) = pdf.description_fields();
        snapshot.fixed = true;
        snapshot.has_cover = true;
        snapshot.description = subject.as_deref().and_then(description);
        snapshot.keywords = values(keywords.as_deref().unwrap_or("").lines());
        snapshot.language = language
            .and_then(|language| value(&language, MAX_VALUE))
            .map(|language| language.to_lowercase());
    } else if let Ok(mut book) = Book::open(&absolute) {
        snapshot.series = values(book.series().iter().map(|series| series.name.as_str()));
        snapshot.language = book
            .language
            .as_ref()
            .map(|language| language.as_str().to_lowercase());
        snapshot.fixed = book.is_fixed_layout();
        snapshot.has_cover = book.cover().is_some() || snapshot.fixed;
        snapshot.chapters = book
            .chapters
            .iter()
            .enumerate()
            .map(|(index, chapter)| (chapter.href.clone(), index))
            .collect();
        for mark in &state.highlights {
            let path = &mark.selection.start.chapter;
            if let Some(index) = book.chapter_index(path) {
                snapshot.chapters.insert(path.clone(), index);
            }
        }
        if let Ok(xml) = book.package_xml() {
            let document = roxmltree::Document::parse(&xml)?;
            if let Some(metadata) = document
                .descendants()
                .find(|node| node.has_tag_name("metadata"))
            {
                const DC: &str = "http://purl.org/dc/elements/1.1/";
                snapshot.language = metadata
                    .children()
                    .filter(|node| node.has_tag_name((DC, "language")))
                    .filter_map(|node| node.text())
                    .find_map(|text| value(text, MAX_VALUE))
                    .map(|text| text.to_lowercase());
                snapshot.keywords = values(
                    metadata
                        .children()
                        .filter(|node| node.has_tag_name((DC, "subject")))
                        .filter_map(|node| node.text())
                        .flat_map(str::lines),
                );
                snapshot.description = metadata
                    .children()
                    .filter(|node| node.has_tag_name((DC, "description")))
                    .filter_map(|node| node.text())
                    .find_map(description);
            }
        } else if let Some(properties) = book.publisher_metadata() {
            snapshot.language = properties
                .language
                .as_deref()
                .and_then(|text| value(text, MAX_VALUE))
                .map(|language| language.to_lowercase())
                .or(snapshot.language);
            snapshot.keywords = values(properties.keywords.iter().flat_map(|text| text.lines()));
            snapshot.description = properties.description.as_deref().and_then(description);
            snapshot.series = values(properties.series.iter().map(String::as_str));
        }
    } else {
        snapshot.fixed = entry.format.is_comic()
            || entry.format.is_still_image()
            || entry.format == DocumentFormat::Djvu;
        snapshot.has_cover = snapshot.fixed;
        if entry.format == DocumentFormat::Cbz {
            if let Some(info) = crate::comic::cbz_info(&absolute)? {
                snapshot.series = values(info.series.iter().map(String::as_str));
                snapshot.language = info
                    .language
                    .as_deref()
                    .and_then(|language| value(language, MAX_VALUE))
                    .map(|language| language.to_lowercase());
                snapshot.keywords = values(info.keywords.iter().map(String::as_str));
                snapshot.description = info.description.as_deref().and_then(description);
            }
            snapshot.cbz_info_scanned = true;
        }
    }
    if library.identify(&entry)? != state.id {
        return Err("The book changed while reading its metadata".into());
    }
    Ok(snapshot)
}

#[cfg(test)]
mod tests;

impl Reader {
    pub(in crate::app) fn bookmark_metadata_deadline(&self) -> Option<Instant> {
        matches!(self.screen, Screen::BookmarkBrowser(_))
            .then_some(self.bookmark_browser.catalog.next)
            .flatten()
    }

    pub(in crate::app) fn save_bookmark_metadata(&mut self) -> Result<()> {
        self.bookmark_browser.catalog.save()
    }

    pub(in crate::app) fn step_bookmark_metadata(&mut self, now: Instant) -> Result<Change> {
        let Some(id) = self.bookmark_browser.catalog.pending.pop_front() else {
            return Ok(Change::None);
        };
        let result: Result<Snapshot> = self
            .library
            .state(id)
            .ok_or_else(|| "That book is unavailable".into())
            .and_then(|book| {
                with_storage(|| read_snapshot(&self.library, book).map_err(std::io::Error::other))
                    .map_err(Into::into)
            });
        let catalog = &mut self.bookmark_browser.catalog;
        match result {
            Ok(snapshot) => {
                catalog.books.insert(id, snapshot);
                catalog.dirty = true;
            }
            Err(_) => {
                catalog.failed.insert(id);
            }
        }
        catalog.next = catalog.loading().then_some(now + Duration::from_millis(40));
        for book in &mut self.bookmark_browser.books {
            if let Some(snapshot) = catalog.get(book.id) {
                book.chapters.clone_from(&snapshot.chapters);
                book.fixed = snapshot.fixed;
            }
        }
        self.refresh_bookmark_browser();
        if !self.bookmark_browser.catalog.loading() {
            self.bookmark_browser.catalog.save()?;
        }
        Ok(Change::Partial)
    }

    pub(super) fn queue_bookmark_metadata(&mut self) {
        let browser = &mut self.bookmark_browser;
        let ids = self
            .library
            .recents()
            .iter()
            .filter(|book| super::has_marks(book))
            .filter(|book| {
                browser.picking.is_none() || browser.books.iter().any(|listed| listed.id == book.id)
            })
            .map(|book| (book.id, is_cbz_book(book)));
        browser.catalog.queue(ids);
    }

    pub(super) fn bookmark_property_rows(&self, property: Property) -> Vec<Item> {
        let browser = &self.bookmark_browser;
        let language = self.controls.interface_language;
        if browser.catalog.loading() {
            return vec![
                Item::new(
                    language.bookmark_text("Reading book metadata"),
                    super::Action::Heading,
                )
                .detail(language.bookmark_books_remaining(browser.catalog.pending.len())),
            ];
        }
        let mut by_value: BTreeMap<Option<String>, usize> = BTreeMap::new();
        for book in self
            .library
            .recents()
            .iter()
            .filter(|book| super::has_marks(book))
            .filter(|book| {
                browser.picking.is_none() || browser.books.iter().any(|listed| listed.id == book.id)
            })
        {
            let values = browser
                .catalog
                .get(book.id)
                .map(|snapshot| property.values(snapshot))
                .unwrap_or_default();
            if values.is_empty() {
                *by_value.entry(None).or_default() += 1;
            }
            for value in values {
                *by_value.entry(Some(value.to_owned())).or_default() += 1;
            }
        }
        by_value
            .into_iter()
            .map(|(value, count)| {
                item(
                    value
                        .as_deref()
                        .unwrap_or_else(|| language.bookmark_text(property.unknown())),
                    Command::SelectProperty(property, value.clone()),
                )
                .detail(language.bookmark_books_count(count))
            })
            .collect()
    }

    pub(super) fn select_bookmark_property(
        &mut self,
        property: Property,
        selected: Option<String>,
    ) {
        let browser = &mut self.bookmark_browser;
        let ids = self
            .library
            .recents()
            .iter()
            .filter(|book| super::has_marks(book))
            .filter(|book| {
                browser.picking.is_none() || browser.books.iter().any(|listed| listed.id == book.id)
            })
            .filter(|book| {
                let values = browser
                    .catalog
                    .get(book.id)
                    .map(|snapshot| property.values(snapshot))
                    .unwrap_or_default();
                selected.as_ref().map_or_else(
                    || values.is_empty(),
                    |selected| values.contains(&selected.as_str()),
                )
            })
            .map(|book| book.id)
            .collect();
        *browser
            .picking
            .as_mut()
            .unwrap_or(&mut browser.picked_books) = ids;
        self.show(Screen::BookmarkBrowser(Page::ChooseBooks));
    }
}
