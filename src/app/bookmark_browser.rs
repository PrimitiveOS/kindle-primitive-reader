//! Browse saved marks across the library without changing an open document.
mod bulk_marks;
mod confirmation;
mod edit_note;
pub(super) mod focus;
mod metadata;
mod metadata_view;
pub(super) mod note_verify;
mod other_pdf;
use super::{
    Action, AnnotationListText, Book, Canvas, Change, Editor, Input, Item, Reader, Rect, Result,
    Screen, mark_chapters::location_caption, navigation_layout::NavigationLayout, with_storage,
};
use crate::Anchor;
use crate::highlight_style::{HighlightColor, HighlightStyle};
use crate::library::{
    BookState, Bookmark, BrowseKind, DocumentFormat, Highlight, Library, PdfHighlight,
    ReadingPosition, fold_search,
};
use crate::search::Query;
use primitive_ui::{BODY, CAPTION, Press, keyboard_key};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
use std::io::Read;
use std::ops::Range;
use std::path::PathBuf;

#[cfg(test)]
const CASE: Rect = Rect::new(40, 314, 386, 72);
const FIRST_MARK: Rect = Rect::new(40, 147, 210, 70);
const CONTROL_WIDTH: i32 = 386;
// Both review lists start with Select all, Clear selection and Done.
const REVIEW_SELECTION_CONTROL_ROWS: usize = 3;

#[derive(Clone, Copy)]
struct BrowserLayout(NavigationLayout);

impl BrowserLayout {
    fn search_field(self, paste: bool) -> Rect {
        let mut field = self.0.body(154, 130);
        if paste {
            field.w = (field.w - 176).max(1);
            if self.0.mirrored {
                field.x += 176;
            }
        }
        field
    }

    fn search_paste(self) -> Rect {
        self.0
            .horizontal(Rect::new(self.0.width - 200, 169, 160, 100))
    }

    fn search_option(self, index: usize) -> Rect {
        let gap = 12;
        let width = ((self.0.width - 80 - gap * 3) / 4).max(1);
        self.0.horizontal(Rect::new(
            40 + index as i32 * (width + gap),
            314,
            width,
            100,
        ))
    }

    fn body(self, y: i32, height: i32) -> Rect {
        self.0.body(y, height)
    }

    fn text(self) -> Rect {
        self.body(370, (self.0.height - 650).max(48))
    }

    fn copy_action(self, note: bool) -> Rect {
        let width = ((self.0.width - 96) / 2).min(260).max(1);
        let x = if note { self.0.width - 40 - width } else { 40 };
        self.0.horizontal(Rect::new(x, 258, width, 100))
    }

    fn control(self, column: i32, inset: i32, height: i32) -> Rect {
        let step = (self.0.width - 80 - CONTROL_WIDTH) / 2;
        Rect::new(
            40 + column * step,
            self.0.height - inset,
            CONTROL_WIDTH,
            height,
        )
    }

    fn centered_caption(self, inset: i32) -> Rect {
        Rect::new((self.0.width - 364) / 2, self.0.height - inset, 364, 55)
    }

    fn last_mark(self) -> Rect {
        Rect::new(self.0.width - 250, FIRST_MARK.y, FIRST_MARK.w, FIRST_MARK.h)
    }

    fn select_note_page(self) -> Rect {
        self.0
            .horizontal(Rect::new(self.0.width - 560, 40, 260, 100))
    }

    fn detail_action(self, index: i32) -> Rect {
        self.0
            .horizontal(Rect::new(self.0.width - 560 + index * 138, 40, 122, 100))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Page {
    Sources,
    Results,
    Filters,
    RemoveNotesConfirm,
    SelectNotes,
    SelectMarks,
    RemoveMarksConfirm,
    EditNote,
    ConfirmClearNote,
    Colors,
    Search,
    Detail,
    Books,
    ChooseBooks,
    Collections,
    Folder,
    Book,
    Authors,
    Property(metadata::Property),
    Description,
}

impl Page {
    fn is_list(self) -> bool {
        !matches!(
            self,
            Self::Colors | Self::Search | Self::Detail | Self::Description | Self::EditNote
        )
    }

    pub(super) const fn title(self) -> &'static str {
        match self {
            Self::Sources => "Bookmark sources",
            Self::Results => "Bookmark browser",
            Self::Filters => "Bookmark filters",
            Self::RemoveNotesConfirm => "Remove notes",
            Self::SelectNotes => "Choose notes",
            Self::SelectMarks => "Choose marks",
            Self::RemoveMarksConfirm => "Remove marks",
            Self::EditNote => "Edit note",
            Self::ConfirmClearNote => "Remove note",
            Self::Colors => "Highlight color filter",
            Self::Search => "Find saved marks",
            Self::Detail => "Saved mark",
            Self::Books => "Books with matches",
            Self::ChooseBooks => "Choose books",
            Self::Collections => "Choose collections",
            Self::Folder => "Choose a folder",
            Self::Book => "Book information",
            Self::Authors => "Choose an author",
            Self::Property(property) => property.title(),
            Self::Description => "Book description",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub(super) enum Source {
    History,
    Archive,
    Folder(PathBuf, bool),
    Books(BTreeSet<u64>),
    Collections(BTreeSet<String>),
}

impl Source {
    fn matches(&self, library: &Library, book: &BookState, archived: bool) -> bool {
        match self {
            Self::History => !archived && book.position.is_some(),
            Self::Archive => archived,
            Self::Folder(path, true) => !archived && book.path.starts_with(path),
            Self::Folder(path, false) => !archived && book.path.parent() == Some(path),
            Self::Books(ids) => ids.contains(&book.id),
            Self::Collections(names) => {
                !archived
                    && names.iter().any(|name| {
                        library
                            .collection(name)
                            .is_some_and(|set| set.contains(book.id))
                    })
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub(super) enum Kind {
    Bookmark,
    Highlight,
    Note,
}

impl Kind {
    fn index(self) -> usize {
        match self {
            Self::Bookmark => 0,
            Self::Highlight => 1,
            Self::Note => 2,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Bookmark => "Page bookmark",
            Self::Highlight => "Highlight",
            Self::Note => "Note",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Locator {
    Bookmark(usize),
    Reflow(usize),
    Fixed(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct MarkKey {
    book: u64,
    locator: Locator,
}

/// Exact mark snapshot retained while an asynchronous folder open is pending.
pub(super) struct DeferredMark {
    key: MarkKey,
    mark: DeferredMarkValue,
}

enum DeferredMarkValue {
    Bookmark(Bookmark),
    Highlight(Highlight),
}

impl DeferredMark {
    fn capture(library: &Library, key: MarkKey) -> Result<Self> {
        let state = library.state(key.book).ok_or("That book is unavailable")?;
        let mark = match key.locator {
            Locator::Bookmark(index) => DeferredMarkValue::Bookmark(
                state
                    .bookmarks
                    .get(index)
                    .ok_or("That bookmark is unavailable")?
                    .clone(),
            ),
            Locator::Reflow(index) => DeferredMarkValue::Highlight(
                state
                    .highlights
                    .get(index)
                    .ok_or("That highlight is unavailable")?
                    .clone(),
            ),
            Locator::Fixed(_) => return Err("This mark cannot be opened in a folder book".into()),
        };
        Ok(Self { key, mark })
    }

    pub(super) fn matches(&self, library: &Library) -> bool {
        library
            .state(self.key.book)
            .is_some_and(|state| match (&self.mark, self.key.locator) {
                (DeferredMarkValue::Bookmark(mark), Locator::Bookmark(index)) => {
                    state.bookmarks.get(index) == Some(mark)
                }
                (DeferredMarkValue::Highlight(mark), Locator::Reflow(index)) => {
                    state.highlights.get(index) == Some(mark)
                }
                _ => false,
            })
    }

    pub(super) fn verify_and_seek(
        &self,
        library: &Library,
        id: u64,
        reading: &mut crate::session::Reading,
    ) -> Result<()> {
        if id != self.key.book || !self.matches(library) {
            return Err("The saved mark changed while opening".into());
        }
        match &self.mark {
            DeferredMarkValue::Bookmark(mark) => {
                if let Some(anchor) = &mark.position.anchor {
                    let chapter = reading
                        .probe
                        .chapter_index(&anchor.chapter)
                        .ok_or("That chapter is unavailable")?;
                    reading.jump_to_anchor(chapter, anchor)?;
                } else {
                    reading.go_to_page(mark.position.chapter, mark.position.page)?;
                }
            }
            DeferredMarkValue::Highlight(mark) => {
                let selection =
                    reading.anchored_quote(&mark.selection.start, &mark.selection.end)?;
                if selection != mark.selection {
                    return Err("The saved highlight changed in this book".into());
                }
                let anchor = &mark.selection.start;
                let chapter = reading
                    .probe
                    .chapter_index(&anchor.chapter)
                    .ok_or("That chapter is unavailable")?;
                reading.jump_to_anchor(chapter, anchor)?;
            }
        }
        Ok(())
    }
}

struct Mark<'a> {
    quote: &'a str,
    note: Option<&'a str>,
    style: Option<HighlightStyle>,
    color: Option<HighlightColor>,
    created_at: Option<i64>,
    location: (usize, f64, f64),
    location_label: String,
}

impl Mark<'_> {
    fn kind(&self) -> Kind {
        match (self.style, self.note) {
            (None, _) => Kind::Bookmark,
            (Some(_), Some(_)) => Kind::Note,
            _ => Kind::Highlight,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RowKey {
    Book(u64),
    Mark(MarkKey),
}

#[derive(Clone, Debug)]
pub(super) enum Command {
    Open,
    Go(Page),
    Use(Source),
    UseCurrentBook,
    FocusCurrentPage,
    InspectLatestMark,
    PreviewRemoveCurrentNotes,
    AskRemoveCurrentNotes,
    ConfirmRemoveCurrentNotes,
    ToggleReviewedNote(usize),
    SetAllReviewedNotes(bool),
    SelectReviewedNotePage,
    PreviewRemoveCurrentMarks,
    AskRemoveCurrentMarks,
    ConfirmRemoveCurrentMarks,
    AskRemoveFocusedMark,
    ConfirmRemoveFocusedMark,
    ToggleReviewedMark(usize),
    SetAllReviewedMarks(bool),
    SelectReviewedMarkPage,
    EditCurrentNote,
    ConfirmClearCurrentNote,
    ResumeCurrentNoteEdit,
    CancelCurrentNoteEdit,
    ToggleDefault(Source),
    OpenFolder(PathBuf, bool),
    ToggleBook(u64),
    ToggleCollection(String),
    ToggleAllBooks,
    ApplyChoices,
    RestrictBooks,
    SelectAuthor(Option<String>),
    SelectProperty(metadata::Property, Option<String>),
    ViewCover,
    SetKind(Option<Kind>),
    SetStyle(Option<HighlightStyle>),
    SetColor(Option<HighlightColor>),
    SortByDate(bool),
    Reverse,
    ClearFilters,
    GotoBook(u64),
    BookInfo(u64),
    ViewBook(u64),
    Inspect(MarkKey),
    ViewMark(MarkKey),
}

/// Complete snapshots prevent an index from silently targeting a changed mark.
enum NoteTarget {
    Bookmark(usize, Bookmark),
    Reflow(usize, Highlight),
    Fixed(usize, PdfHighlight),
}

enum ReviewLocation<'a> {
    Saved(&'a ReadingPosition),
    Reflow(&'a Anchor),
    Fixed(usize),
}

impl NoteTarget {
    fn preview(&self) -> (&str, &str, Option<&str>, ReviewLocation<'_>) {
        match self {
            Self::Bookmark(_, mark) => (
                mark.note.as_deref().unwrap_or(""),
                &mark.label,
                mark.chapter_title.as_deref(),
                ReviewLocation::Saved(&mark.position),
            ),
            Self::Reflow(_, mark) => (
                mark.note.as_deref().unwrap_or(""),
                &mark.selection.text,
                mark.chapter_title.as_deref(),
                ReviewLocation::Reflow(&mark.selection.start),
            ),
            Self::Fixed(_, mark) => (
                mark.note.as_deref().unwrap_or(""),
                &mark.text,
                mark.chapter_title.as_deref(),
                ReviewLocation::Fixed(mark.page),
            ),
        }
    }
    fn capture(book: &BookState, locator: Locator) -> Result<Option<Self>> {
        Ok(match locator {
            Locator::Bookmark(index) => {
                let mark = book
                    .bookmarks
                    .get(index)
                    .ok_or("A matching mark is unavailable")?;
                if mark.note.as_ref().is_none_or(|note| note.trim().is_empty()) {
                    None
                } else {
                    Some(Self::Bookmark(index, mark.clone()))
                }
            }
            Locator::Reflow(index) => {
                let mark = book
                    .highlights
                    .get(index)
                    .ok_or("A matching mark is unavailable")?;
                if mark.note.as_ref().is_none_or(|note| note.trim().is_empty()) {
                    None
                } else {
                    Some(Self::Reflow(index, mark.clone()))
                }
            }
            Locator::Fixed(index) => {
                let mark = book
                    .pdf_highlights
                    .get(index)
                    .ok_or("A matching mark is unavailable")?;
                mark.note
                    .as_ref()
                    .filter(|note| !note.trim().is_empty())
                    .map(|_| Self::Fixed(index, mark.clone()))
            }
        })
    }

    fn verify_raw(&self, reader: &Reader) -> Result<()> {
        let snapshot = match self {
            Self::Bookmark(index, mark)
                if mark
                    .position
                    .anchor
                    .as_ref()
                    .is_some_and(|a| a.raw_caret.is_some()) =>
            {
                Some(bulk_marks::Snapshot::Bookmark(*index, mark.clone()))
            }
            Self::Reflow(index, mark)
                if mark.selection.start.raw_caret.is_some()
                    || mark.selection.end.raw_caret.is_some() =>
            {
                Some(bulk_marks::Snapshot::Reflow(*index, mark.clone()))
            }
            _ => None,
        };
        if let Some(snapshot) = snapshot {
            snapshot.verify_source(reader)?;
        }
        Ok(())
    }

    fn matches(&self, book: &BookState) -> bool {
        match self {
            Self::Bookmark(index, original) => book.bookmarks.get(*index) == Some(original),
            Self::Reflow(index, original) => book.highlights.get(*index) == Some(original),
            Self::Fixed(index, original) => book.pdf_highlights.get(*index) == Some(original),
        }
    }

    fn clear(&self, book: &mut BookState, now: Option<i64>) {
        match self {
            Self::Bookmark(index, _) => {
                book.bookmarks[*index].note = None;
                book.bookmarks[*index].updated_at = now;
            }
            Self::Reflow(index, _) => {
                book.highlights[*index].note = None;
                book.highlights[*index].updated_at = now;
            }
            Self::Fixed(index, _) => {
                book.pdf_highlights[*index].note = None;
                book.pdf_highlights[*index].updated_at = now;
            }
        }
    }
}

struct PendingNoteRemoval {
    book: u64,
    title: String,
    targets: Vec<ReviewedNote>,
}

struct ReviewedNote {
    target: NoteTarget,
    selected: bool,
}

impl PendingNoteRemoval {
    fn selected_count(&self) -> usize {
        self.targets.iter().filter(|item| item.selected).count()
    }
}

struct ListedBook {
    id: u64,
    archived: bool,
    count: usize,
    chapters: HashMap<String, usize>,
    fixed: bool,
}

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default)]
struct Preferences {
    source: Option<Source>,
    kind: Option<Kind>,
    style: Option<HighlightStyle>,
    color: Option<HighlightColor>,
    sort_by_date: bool,
    reverse: bool,
}

pub(super) fn backup_value(bytes: &[u8]) -> std::io::Result<serde_json::Value> {
    let preferences: Preferences = serde_json::from_slice(bytes)?;
    let valid = match &preferences.source {
        Some(Source::Folder(path, _)) => {
            path.components()
                .all(|c| matches!(c, std::path::Component::Normal(_)))
                && path
                    .to_str()
                    .is_some_and(|s| s.len() <= 4096 && !s.chars().any(char::is_control))
        }
        Some(Source::Books(ids)) => ids.len() <= 16_384,
        Some(Source::Collections(names)) => {
            names.len() <= 256
                && names.iter().all(|s| {
                    !s.trim().is_empty()
                        && s.chars().count() <= 64
                        && !s.chars().any(char::is_control)
                })
        }
        _ => true,
    };
    if !valid {
        return Err(std::io::Error::other(
            "The bookmark browser backup is invalid",
        ));
    }
    serde_json::to_value(preferences).map_err(std::io::Error::other)
}

pub(super) fn metadata_backup_value(bytes: &[u8]) -> std::io::Result<serde_json::Value> {
    metadata::backup_value(bytes)
}

pub(super) struct Browser {
    catalog: metadata::Catalog,
    path: PathBuf,
    default_source: Option<Source>,
    origin: Screen,
    sources_from_results: bool,
    source: Source,
    source_label: String,
    books: Vec<ListedBook>,
    rows: Vec<RowKey>,
    marks: Vec<MarkKey>,
    query_text: String,
    query: Option<Query>,
    case_sensitive: bool,
    search_types: [bool; 3],
    draft_search_types: [bool; 3],
    kind: Option<Kind>,
    style: Option<HighlightStyle>,
    color: Option<HighlightColor>,
    sort_by_date: bool,
    reverse: bool,
    restricted: Option<BTreeSet<u64>>,
    picking: Option<BTreeSet<u64>>,
    picked_books: BTreeSet<u64>,
    picked_collections: BTreeSet<String>,
    folder: PathBuf,
    recursive: bool,
    subfolders: Vec<PathBuf>,
    focus: Option<MarkKey>,
    book: Option<u64>,
    text_offset: usize,
    text_pages: RefCell<Vec<Range<usize>>>,
    list_anchor: Option<(usize, usize)>,
    pending_note_removal: Option<PendingNoteRemoval>,
    pending_mark_removal: Option<bulk_marks::Pending>,
    pending_note_edit: Option<edit_note::Pending>,
    confirmation: Option<confirmation::Confirmation>,
    pub(super) after_password: Option<MarkKey>,
}

impl Browser {
    pub(super) fn clear_confirmation(&mut self) {
        self.confirmation = None;
    }

    pub(super) fn reset_list_anchor(&mut self) {
        self.list_anchor = None;
    }

    fn reset_text(&mut self) {
        self.text_offset = 0;
        self.text_pages.get_mut().clear();
    }

    fn text_page(&self, pages: &[Range<usize>]) -> usize {
        pages
            .partition_point(|page| page.start <= self.text_offset)
            .saturating_sub(1)
    }

    pub(super) fn replace_text_pages(&self, pages: Vec<Range<usize>>) -> Vec<Range<usize>> {
        self.text_pages.replace(pages)
    }
}

impl Default for Browser {
    fn default() -> Self {
        Self {
            catalog: metadata::Catalog::default(),
            path: PathBuf::new(),
            default_source: None,
            origin: Screen::Library,
            sources_from_results: false,
            source: Source::History,
            source_label: "History".into(),
            books: Vec::new(),
            rows: Vec::new(),
            marks: Vec::new(),
            query_text: String::new(),
            query: None,
            case_sensitive: false,
            search_types: [true; 3],
            draft_search_types: [true; 3],
            kind: None,
            style: None,
            color: None,
            sort_by_date: false,
            reverse: false,
            restricted: None,
            picking: None,
            picked_books: BTreeSet::new(),
            picked_collections: BTreeSet::new(),
            folder: PathBuf::new(),
            recursive: false,
            subfolders: Vec::new(),
            focus: None,
            book: None,
            text_offset: 0,
            text_pages: RefCell::default(),
            list_anchor: None,
            pending_note_removal: None,
            pending_mark_removal: None,
            pending_note_edit: None,
            confirmation: None,
            after_password: None,
        }
    }
}

impl Browser {
    pub(super) fn load(path: PathBuf) -> Self {
        const LIMIT: usize = 1024 * 1024;
        let preferences: Preferences = (|| {
            let mut bytes = Vec::new();
            std::fs::File::open(&path)
                .ok()?
                .take((LIMIT + 1) as u64)
                .read_to_end(&mut bytes)
                .ok()?;
            if bytes.len() > LIMIT {
                return None;
            }
            serde_json::from_slice(&bytes).ok()
        })()
        .unwrap_or_default();
        Self {
            catalog: metadata::Catalog::load(path.with_file_name("bookmark-metadata.json")),
            path,
            default_source: preferences.source,
            kind: preferences.kind,
            style: preferences.style,
            color: preferences.color,
            sort_by_date: preferences.sort_by_date,
            reverse: preferences.reverse,
            ..Self::default()
        }
    }

    fn save(&self) -> Result<()> {
        let bytes = serde_json::to_vec(&Preferences {
            source: self.default_source.clone(),
            kind: self.kind,
            style: self.style,
            color: self.color,
            sort_by_date: self.sort_by_date,
            reverse: self.reverse,
        })?;
        with_storage(|| primitive_app::save(&self.path, &bytes))?;
        Ok(())
    }
}

fn has_marks(book: &BookState) -> bool {
    !book.bookmarks.is_empty() || !book.highlights.is_empty() || !book.pdf_highlights.is_empty()
}

fn is_fixed(book: &BookState) -> bool {
    !book.pdf_highlights.is_empty()
        || DocumentFormat::for_path(&book.path).is_some_and(|format| {
            format.is_still_image()
                || format.is_comic()
                || matches!(format, DocumentFormat::Pdf | DocumentFormat::Djvu)
        })
}

fn get_mark<'a>(
    book: &'a BookState,
    locator: Locator,
    chapter_for: impl Fn(&str) -> Option<usize>,
    fixed: bool,
) -> Option<Mark<'a>> {
    Some(match locator {
        Locator::Bookmark(index) => {
            let mark = book.bookmarks.get(index)?;
            let position = &mark.position;
            let location = if fixed {
                (position.page, 0.0, 0.0)
            } else {
                position
                    .anchor
                    .as_ref()
                    .map_or((position.chapter, position.page as f64, 0.0), |anchor| {
                        (position.chapter, anchor.block as f64, anchor.offset as f64)
                    })
            };
            Mark {
                quote: &mark.label,
                note: mark.note.as_deref(),
                style: None,
                color: None,
                created_at: mark.created_at,
                location,
                location_label: location_caption(
                    if fixed {
                        format!("Page {}", position.page + 1)
                    } else {
                        format!(
                            "Chapter {} · Page {}",
                            position.chapter + 1,
                            position.page + 1
                        )
                    },
                    mark.chapter_title.as_deref(),
                ),
            }
        }
        Locator::Reflow(index) => {
            let mark = book.highlights.get(index)?;
            let anchor = &mark.selection.start;
            let chapter = chapter_for(&anchor.chapter).or_else(|| {
                book.bookmarks.iter().find_map(|mark| {
                    mark.position
                        .anchor
                        .as_ref()
                        .filter(|candidate| candidate.chapter == anchor.chapter)
                        .map(|_| mark.position.chapter)
                })
            });
            Mark {
                quote: &mark.selection.text,
                note: mark.note.as_deref(),
                style: Some(mark.style),
                color: Some(mark.color),
                created_at: mark.created_at,
                location: (
                    chapter.unwrap_or(usize::MAX),
                    anchor.block as f64,
                    anchor.offset as f64,
                ),
                location_label: location_caption(
                    chapter.map_or_else(
                        || "Saved text location".into(),
                        |n| format!("Chapter {}", n + 1),
                    ),
                    mark.chapter_title.as_deref(),
                ),
            }
        }
        Locator::Fixed(index) => {
            let mark = book.pdf_highlights.get(index)?;
            let (y, x) = mark
                .bounds
                .first()
                .map_or((0.0, 0.0), |rect| (f64::from(rect.y0), f64::from(rect.x0)));
            Mark {
                quote: &mark.text,
                note: mark.note.as_deref(),
                style: Some(mark.style),
                color: Some(mark.color),
                created_at: mark.created_at,
                location: (mark.page, y, x),
                location_label: location_caption(
                    format!("Page {}", mark.page + 1),
                    mark.chapter_title.as_deref(),
                ),
            }
        }
    })
}

fn item(text: impl Into<String>, action: Command) -> Item {
    Item::new(text, Action::BookmarkBrowser(action))
}

fn excerpt(text: &str, limit: usize) -> String {
    text.split_whitespace()
        .take(limit)
        .collect::<Vec<_>>()
        .join(" ")
}

fn mark_count(count: usize) -> String {
    format!("{count} {}", if count == 1 { "mark" } else { "marks" })
}

// The compact controls have the same 100 px minimum touch height as standard rows.
fn contains(control: Rect, x: f32, y: f32) -> bool {
    let extra = (100 - control.h).max(0);
    Rect::new(
        control.x,
        control.y - extra / 2,
        control.w,
        control.h + extra,
    )
    .contains(x, y)
}

fn author_title(book: &BookState) -> String {
    book.author.as_deref().map_or_else(
        || book.title.clone(),
        |author| {
            format!(
                "{} · {}",
                author.lines().next().unwrap_or(author),
                book.title
            )
        },
    )
}

impl Reader {
    pub(super) fn activate_bookmark_browser(&mut self, action: Command) -> Result<()> {
        match action {
            Command::Open => {
                let browser = &mut self.bookmark_browser;
                browser.confirmation = None;
                browser.pending_note_removal = None;
                browser.pending_mark_removal = None;
                browser.pending_note_edit = None;
                browser.origin = match self.screen {
                    Screen::BookTools => Screen::Book,
                    Screen::FixedTools => Screen::Fixed,
                    other => other,
                };
                browser.query_text.clear();
                browser.query = None;
                browser.case_sensitive = false;
                browser.search_types = [true; 3];
                browser.draft_search_types = [true; 3];
                browser.restricted = None;
                browser.after_password = None;
                browser.sources_from_results = false;
                if let Some(source) = browser.default_source.clone() {
                    return self.activate_bookmark_browser(Command::Use(source));
                }
                self.show(Screen::BookmarkBrowser(Page::Sources));
            }
            Command::Go(page) => {
                if !matches!(page, Page::RemoveNotesConfirm | Page::SelectNotes) {
                    self.bookmark_browser.pending_note_removal = None;
                }
                if !matches!(page, Page::RemoveMarksConfirm | Page::SelectMarks) {
                    self.bookmark_browser.pending_mark_removal = None;
                }
                if !matches!(page, Page::EditNote | Page::ConfirmClearNote) {
                    self.bookmark_browser.pending_note_edit = None;
                }
                if page == Page::Sources {
                    self.bookmark_browser.sources_from_results = true;
                }
                if page == Page::Search {
                    self.editor = Editor::new(crate::search::MAX_QUERY_BYTES);
                    self.editor.text = self.bookmark_browser.query_text.chars().collect();
                    self.editor.caret = self.editor.text.len();
                    self.bookmark_browser.draft_search_types = self.bookmark_browser.search_types;
                }
                if page == Page::ChooseBooks {
                    self.bookmark_browser.picking = None;
                    self.bookmark_browser.picked_books = match &self.bookmark_browser.source {
                        Source::Books(ids) => ids.clone(),
                        _ => BTreeSet::new(),
                    };
                }
                if page == Page::Collections {
                    self.bookmark_browser.picked_collections = match &self.bookmark_browser.source {
                        Source::Collections(names) => names.clone(),
                        _ => BTreeSet::new(),
                    };
                }
                if matches!(page, Page::Property(_)) {
                    self.queue_bookmark_metadata();
                }
                if page == Page::Description {
                    self.bookmark_browser.reset_text();
                }
                self.show(Screen::BookmarkBrowser(page));
            }
            Command::Use(source) => {
                let mut books = with_storage(|| {
                    Ok(self
                        .library
                        .recents()
                        .iter()
                        .filter(|book| has_marks(book))
                        .filter_map(|book| {
                            let archived = self.library.absolute_path(&book.path).is_err();
                            source.matches(&self.library, book, archived).then(|| {
                                let (chapters, fixed) =
                                    self.bookmark_browser.catalog.get(book.id).map_or_else(
                                        || (HashMap::new(), is_fixed(book)),
                                        |snapshot| (snapshot.chapters.clone(), snapshot.fixed),
                                    );
                                (
                                    fold_search(&author_title(book)),
                                    ListedBook {
                                        id: book.id,
                                        archived,
                                        count: 0,
                                        chapters,
                                        fixed,
                                    },
                                )
                            })
                        })
                        .collect::<Vec<_>>())
                })?;
                books.sort_by(|(left, _), (right, _)| left.cmp(right));
                self.bookmark_browser.books = books.into_iter().map(|(_, book)| book).collect();
                self.bookmark_browser.restricted = None;
                self.bookmark_browser.source_label = match &source {
                    Source::History => "History".into(),
                    Source::Archive => "Archived books".into(),
                    Source::Folder(path, _) if path.as_os_str().is_empty() => "Documents".into(),
                    Source::Folder(path, _) => {
                        crate::library::display_filename(&path.to_string_lossy())
                    }
                    Source::Books(_) => "Selected books".into(),
                    Source::Collections(_) => "Collections".into(),
                };
                self.bookmark_browser.source = source;
                self.queue_bookmark_metadata();
                self.refresh_bookmark_browser();
                self.show(Screen::BookmarkBrowser(Page::Results));
            }
            Command::UseCurrentBook => {
                // This source is intentionally transient: unlike a chosen set of
                // books it must never become a persistent default for later books.
                if matches!(self.bookmark_browser.origin, Screen::Book | Screen::Fixed)
                    && let Some(id) = self.open_book_id()
                {
                    self.activate_bookmark_browser(Command::Use(Source::Books(BTreeSet::from([
                        id,
                    ]))))?;
                    self.bookmark_browser.source_label = self
                        .controls
                        .interface_language
                        .bookmark_text("Current book")
                        .to_owned();
                }
            }
            Command::FocusCurrentPage => {
                if self.start_bookmark_focus() {
                    return Ok(());
                }
                // Use only filtered, already displayed rows of the open book.
                // Sorting the result list must not affect the target.
                let target = (|| {
                    let current = self.current_position()?;
                    let book_id = match self.bookmark_browser.origin {
                        Screen::Fixed => self.fixed.as_ref()?.id,
                        Screen::Book => self.active.as_ref()?.id,
                        _ => return None,
                    };
                    let state = self.library.state(book_id)?;
                    self.bookmark_browser
                        .rows
                        .iter()
                        .enumerate()
                        .filter_map(|(ordinal, row)| {
                            let RowKey::Mark(key) = row else {
                                return None;
                            };
                            if key.book != book_id {
                                return None;
                            }
                            let page = match self.bookmark_browser.origin {
                                Screen::Fixed => {
                                    let book = self.fixed.as_ref()?;
                                    let page = match key.locator {
                                        Locator::Bookmark(index) => {
                                            state.bookmarks.get(index)?.position.page
                                        }
                                        Locator::Fixed(index) => {
                                            state.pdf_highlights.get(index)?.page
                                        }
                                        Locator::Reflow(_) => return None,
                                    };
                                    (page < book.reading.count()).then_some(page)?
                                }
                                Screen::Book => {
                                    let book = self.active.as_ref()?;
                                    let reading = &book.reading;
                                    let probe = &reading.probe;
                                    let anchor_page = |anchor: &crate::Anchor| {
                                        (probe.chapter_index(&anchor.chapter)
                                            == Some(reading.chapter))
                                        .then_some(())?;
                                        let resolved = probe.restore_anchor(anchor)?;
                                        probe.page_for(&resolved)
                                    };
                                    match key.locator {
                                        Locator::Bookmark(index) => {
                                            let position = &state.bookmarks.get(index)?.position;
                                            if let Some(anchor) = &position.anchor {
                                                anchor_page(anchor)?
                                            } else {
                                                (position.chapter == reading.chapter
                                                    && position.page < probe.page_count())
                                                .then_some(position.page)?
                                            }
                                        }
                                        Locator::Reflow(index) => anchor_page(
                                            &state.highlights.get(index)?.selection.start,
                                        )?,
                                        Locator::Fixed(_) => return None,
                                    }
                                }
                                _ => return None,
                            };
                            Some((ordinal, page))
                        })
                        .min_by_key(|&(ordinal, page)| {
                            if page >= current.page {
                                (false, page - current.page, ordinal)
                            } else {
                                (true, current.page - page, ordinal)
                            }
                        })
                        .map(|(ordinal, _)| ordinal)
                })();
                self.show(Screen::BookmarkBrowser(Page::Results));
                if let Some(ordinal) = target {
                    let page = ordinal / self.list_geometry().rows;
                    self.set_bookmark_list_anchor(page, ordinal);
                } else {
                    self.notice = Some(
                        self.controls
                            .interface_language
                            .bookmark_text(if self.bookmark_browser.origin == Screen::Book {
                                "No matching marks in the current chapter"
                            } else {
                                "No matching marks in the current book"
                            })
                            .to_owned(),
                    );
                }
            }
            Command::InspectLatestMark => {
                let target = (|| {
                    let book_id = match self.bookmark_browser.origin {
                        Screen::Fixed => self.fixed.as_ref()?.id,
                        Screen::Book => self.active.as_ref()?.id,
                        _ => return None,
                    };
                    let state = self.library.state(book_id)?;
                    self.bookmark_browser
                        .rows
                        .iter()
                        .enumerate()
                        .filter_map(|(ordinal, row)| {
                            let RowKey::Mark(key) = row else {
                                return None;
                            };
                            if key.book != book_id {
                                return None;
                            }
                            let (created, kind, index) = match key.locator {
                                Locator::Bookmark(index) => {
                                    (state.bookmarks.get(index)?.created_at, 0, index)
                                }
                                Locator::Reflow(index)
                                    if self.bookmark_browser.origin == Screen::Book =>
                                {
                                    (state.highlights.get(index)?.created_at, 1, index)
                                }
                                Locator::Fixed(index)
                                    if self.bookmark_browser.origin == Screen::Fixed =>
                                {
                                    (state.pdf_highlights.get(index)?.created_at, 2, index)
                                }
                                _ => return None,
                            };
                            Some(((created, kind, index), (ordinal, *key)))
                        })
                        .max_by_key(|(key, _)| *key)
                        .map(|(_, target)| target)
                })();
                self.show(Screen::BookmarkBrowser(Page::Results));
                if let Some((_, key)) = target {
                    self.activate_bookmark_browser(Command::Inspect(key))?;
                } else {
                    self.notice = Some(
                        self.controls
                            .interface_language
                            .bookmark_text("No matching marks in the current book")
                            .to_owned(),
                    );
                }
            }
            Command::PreviewRemoveCurrentNotes => {
                if self.screen != Screen::BookmarkBrowser(Page::Filters)
                    || !matches!(self.bookmark_browser.origin, Screen::Book | Screen::Fixed)
                {
                    return Err("Open a book before removing its notes".into());
                }
                let id = self.open_book_id().ok_or("Open a book first")?;
                let book = self.library.state(id).ok_or("That book is unavailable")?;
                let mut targets = Vec::new();
                let mut seen = BTreeSet::new();
                for key in self
                    .bookmark_browser
                    .marks
                    .iter()
                    .filter(|key| key.book == id)
                {
                    let identity = match key.locator {
                        Locator::Bookmark(index) => (0, index),
                        Locator::Reflow(index) => (1, index),
                        Locator::Fixed(index) => (2, index),
                    };
                    if !seen.insert(identity) {
                        continue;
                    }
                    if let Some(target) = NoteTarget::capture(book, key.locator)? {
                        target.verify_raw(self)?;
                        if targets.len() == 512 {
                            return Err("Too many matching notes; narrow the filters first".into());
                        }
                        targets.push(ReviewedNote {
                            target,
                            selected: true,
                        });
                    }
                }
                if targets.is_empty() {
                    return Err("No matching notes in the current book".into());
                }
                self.bookmark_browser.pending_note_removal = Some(PendingNoteRemoval {
                    book: id,
                    title: book.title.clone(),
                    targets,
                });
                self.notice = None;
                self.show(Screen::BookmarkBrowser(Page::RemoveNotesConfirm));
            }
            Command::AskRemoveCurrentNotes => {
                if self.screen != Screen::BookmarkBrowser(Page::RemoveNotesConfirm)
                    || !self
                        .bookmark_browser
                        .pending_note_removal
                        .as_ref()
                        .is_some_and(|p| p.selected_count() > 0)
                {
                    return Err("Review the notes before removing them".into());
                }
                self.bookmark_browser.confirmation = Some(confirmation::Confirmation::Notes);
            }
            Command::ConfirmRemoveCurrentNotes => {
                if self.screen != Screen::BookmarkBrowser(Page::RemoveNotesConfirm) {
                    return Err("Review the notes before removing them".into());
                }
                if self.bookmark_browser.confirmation != Some(confirmation::Confirmation::Notes) {
                    return Err("Confirm removal before deleting notes".into());
                }
                let pending = self
                    .bookmark_browser
                    .pending_note_removal
                    .as_ref()
                    .ok_or("Review the notes before removing them")?;
                if self.open_book_id() != Some(pending.book)
                    || !matches!(self.bookmark_browser.origin, Screen::Book | Screen::Fixed)
                {
                    return Err("The open book changed; review the notes again".into());
                }
                let id = pending.book;
                let state = self.library.state(id).ok_or("That book is unavailable")?;
                if state.title != pending.title
                    || pending.selected_count() == 0
                    || !pending
                        .targets
                        .iter()
                        .filter(|item| item.selected)
                        .all(|item| item.target.matches(state))
                {
                    return Err("A matching mark changed; review the notes again".into());
                }
                for item in pending.targets.iter().filter(|item| item.selected) {
                    item.target.verify_raw(self)?;
                }
                // Roll back all in-memory changes if the existing atomic library
                // persistence fails; only the reviewed book is ever modified.
                let before = state.clone();
                let state = self
                    .library
                    .state_mut(id)
                    .ok_or("That book is unavailable")?;
                let now = crate::mark_time::now();
                for item in pending.targets.iter().filter(|item| item.selected) {
                    item.target.clear(state, now);
                }
                if let Err(error) = self.library.persist() {
                    if let Some(state) = self.library.state_mut(id) {
                        *state = before;
                    }
                    return Err(error.into());
                }
                self.bookmark_browser.pending_note_removal = None;
                self.mark_book_changed();
                self.refresh_bookmark_browser();
                self.show(Screen::BookmarkBrowser(Page::Results));
                self.notice = Some(
                    self.controls
                        .interface_language
                        .bookmark_text("Notes removed")
                        .into(),
                );
            }
            Command::ToggleReviewedNote(index) => {
                if self.screen != Screen::BookmarkBrowser(Page::SelectNotes) {
                    return Err("Review the notes before selecting them".into());
                }
                let item = self
                    .bookmark_browser
                    .pending_note_removal
                    .as_mut()
                    .and_then(|pending| pending.targets.get_mut(index))
                    .ok_or("That reviewed note is unavailable")?;
                item.selected = !item.selected;
            }
            Command::SetAllReviewedNotes(selected) => {
                if self.screen != Screen::BookmarkBrowser(Page::SelectNotes) {
                    return Err("Review the notes before selecting them".into());
                }
                let pending = self
                    .bookmark_browser
                    .pending_note_removal
                    .as_mut()
                    .ok_or("Review the notes before selecting them")?;
                for item in &mut pending.targets {
                    item.selected = selected;
                }
            }
            Command::SelectReviewedNotePage => {
                if self.screen != Screen::BookmarkBrowser(Page::SelectNotes) {
                    return Err("Review the notes before selecting them".into());
                }
                let rows = self.list_geometry().rows;
                let first = self.list_page.saturating_mul(rows);
                let last = first.saturating_add(rows);
                let pending = self
                    .bookmark_browser
                    .pending_note_removal
                    .as_mut()
                    .ok_or("Review the notes before selecting them")?;
                let first_note = first.saturating_sub(REVIEW_SELECTION_CONTROL_ROWS);
                let last_note = last
                    .saturating_sub(REVIEW_SELECTION_CONTROL_ROWS)
                    .min(pending.targets.len());
                if first_note < last_note {
                    for item in &mut pending.targets[first_note..last_note] {
                        item.selected = true;
                    }
                }
            }
            Command::PreviewRemoveCurrentMarks => bulk_marks::preview(self)?,
            Command::AskRemoveCurrentMarks => {
                if self.screen != Screen::BookmarkBrowser(Page::RemoveMarksConfirm)
                    || !self
                        .bookmark_browser
                        .pending_mark_removal
                        .as_ref()
                        .is_some_and(|p| p.selected_count() > 0)
                {
                    return Err("Review the marks before removing them".into());
                }
                self.bookmark_browser.confirmation = Some(confirmation::Confirmation::Marks);
            }
            Command::ConfirmRemoveCurrentMarks => {
                if self.bookmark_browser.confirmation != Some(confirmation::Confirmation::Marks) {
                    return Err("Confirm removal before deleting marks".into());
                }
                bulk_marks::confirm(self)?;
            }
            Command::AskRemoveFocusedMark => bulk_marks::ask_detail(self)?,
            Command::ConfirmRemoveFocusedMark => {
                if self.bookmark_browser.confirmation
                    != Some(confirmation::Confirmation::SingleMark)
                {
                    return Err("Confirm removal before deleting this mark".into());
                }
                bulk_marks::confirm(self)?;
            }
            Command::EditCurrentNote => edit_note::open(self)?,
            Command::ConfirmClearCurrentNote => edit_note::commit(self)?,
            Command::ResumeCurrentNoteEdit => edit_note::resume(self)?,
            Command::CancelCurrentNoteEdit => edit_note::cancel(self),
            Command::ToggleReviewedMark(index) => {
                if self.screen != Screen::BookmarkBrowser(Page::SelectMarks) {
                    return Err("Review the marks before selecting them".into());
                }
                let item = self
                    .bookmark_browser
                    .pending_mark_removal
                    .as_mut()
                    .and_then(|pending| pending.targets.get_mut(index))
                    .ok_or("That reviewed mark is unavailable")?;
                item.selected = !item.selected;
            }
            Command::SetAllReviewedMarks(selected) => {
                if self.screen != Screen::BookmarkBrowser(Page::SelectMarks) {
                    return Err("Review the marks before selecting them".into());
                }
                let pending = self
                    .bookmark_browser
                    .pending_mark_removal
                    .as_mut()
                    .ok_or("Review the marks before selecting them")?;
                for item in &mut pending.targets {
                    item.selected = selected;
                }
            }
            Command::SelectReviewedMarkPage => {
                if self.screen != Screen::BookmarkBrowser(Page::SelectMarks) {
                    return Err("Review the marks before selecting them".into());
                }
                let rows = self.list_geometry().rows;
                let first = self.list_page.saturating_mul(rows);
                let last = first.saturating_add(rows);
                let pending = self
                    .bookmark_browser
                    .pending_mark_removal
                    .as_mut()
                    .ok_or("Review the marks before selecting them")?;
                let first_mark = first.saturating_sub(REVIEW_SELECTION_CONTROL_ROWS);
                let last_mark = last
                    .saturating_sub(REVIEW_SELECTION_CONTROL_ROWS)
                    .min(pending.targets.len());
                if first_mark < last_mark {
                    for item in &mut pending.targets[first_mark..last_mark] {
                        item.selected = true;
                    }
                }
            }
            Command::ToggleDefault(source) => {
                let browser = &mut self.bookmark_browser;
                let previous = browser.default_source.clone();
                browser.default_source = (previous.as_ref() != Some(&source)).then_some(source);
                if let Err(error) = browser.save() {
                    browser.default_source = previous;
                    return Err(error);
                }
                self.notice = Some(if browser.default_source.is_some() {
                    "Default bookmark source saved".into()
                } else {
                    "Default bookmark source cleared".into()
                });
            }
            Command::OpenFolder(path, recursive) => {
                let listing = with_storage(|| self.library.browse(&path))?;
                let browser = &mut self.bookmark_browser;
                browser.subfolders = listing
                    .entries
                    .into_iter()
                    .filter(|entry| matches!(entry.kind, BrowseKind::Folder))
                    .map(|entry| entry.path)
                    .collect();
                browser.folder = path;
                browser.recursive = recursive;
                self.show(Screen::BookmarkBrowser(Page::Folder));
            }
            Command::ToggleBook(id) => {
                let browser = &mut self.bookmark_browser;
                let chosen = browser
                    .picking
                    .as_mut()
                    .unwrap_or(&mut browser.picked_books);
                if !chosen.remove(&id) {
                    chosen.insert(id);
                }
            }
            Command::ToggleCollection(name) => {
                let chosen = &mut self.bookmark_browser.picked_collections;
                if !chosen.remove(&name) {
                    chosen.insert(name);
                }
            }
            Command::ToggleAllBooks => {
                let browser = &mut self.bookmark_browser;
                let ids: BTreeSet<_> = if browser.picking.is_some() {
                    browser.books.iter().map(|book| book.id).collect()
                } else {
                    self.library
                        .recents()
                        .iter()
                        .filter(|book| has_marks(book))
                        .map(|book| book.id)
                        .collect()
                };
                let chosen = browser
                    .picking
                    .as_mut()
                    .unwrap_or(&mut browser.picked_books);
                *chosen = if *chosen == ids { BTreeSet::new() } else { ids };
            }
            Command::RestrictBooks => {
                let browser = &mut self.bookmark_browser;
                browser.picking = Some(
                    browser
                        .restricted
                        .clone()
                        .unwrap_or_else(|| browser.books.iter().map(|book| book.id).collect()),
                );
                self.show(Screen::BookmarkBrowser(Page::ChooseBooks));
            }
            Command::SelectProperty(property, value) => {
                self.select_bookmark_property(property, value)
            }
            Command::ViewCover => self.view_bookmark_cover()?,
            Command::SelectAuthor(author) => {
                let browser = &mut self.bookmark_browser;
                let ids = self
                    .library
                    .recents()
                    .iter()
                    .filter(|book| has_marks(book))
                    .filter(|book| {
                        browser.picking.is_none() || browser.books.iter().any(|b| b.id == book.id)
                    })
                    .filter(|book| match &author {
                        None => book
                            .author
                            .as_deref()
                            .is_none_or(|name| name.trim().is_empty()),
                        Some(author) => book
                            .author
                            .as_deref()
                            .is_some_and(|names| names.lines().any(|name| name.trim() == author)),
                    })
                    .map(|book| book.id)
                    .collect();
                *browser
                    .picking
                    .as_mut()
                    .unwrap_or(&mut browser.picked_books) = ids;
                self.show(Screen::BookmarkBrowser(Page::ChooseBooks));
            }
            Command::ApplyChoices => {
                let browser = &mut self.bookmark_browser;
                if self.screen == Screen::BookmarkBrowser(Page::Collections) {
                    let source = Source::Collections(browser.picked_collections.clone());
                    self.activate_bookmark_browser(Command::Use(source))?;
                } else if let Some(chosen) = browser.picking.take() {
                    browser.restricted = Some(chosen);
                    self.refresh_bookmark_browser();
                    self.show(Screen::BookmarkBrowser(Page::Results));
                } else {
                    let source = Source::Books(browser.picked_books.clone());
                    self.activate_bookmark_browser(Command::Use(source))?;
                }
            }
            Command::SetKind(value) => {
                self.bookmark_browser.kind = value;
                self.refresh_bookmark_browser();
                self.bookmark_browser.save()?;
            }
            Command::SetStyle(value) => {
                self.bookmark_browser.style = value;
                self.refresh_bookmark_browser();
                self.bookmark_browser.save()?;
            }
            Command::SetColor(value) => {
                let prior = self.bookmark_browser.color;
                self.bookmark_browser.color = value;
                if let Err(error) = self.bookmark_browser.save() {
                    self.bookmark_browser.color = prior;
                    return Err(error);
                }
                self.refresh_bookmark_browser();
            }
            Command::SortByDate(value) => {
                self.bookmark_browser.sort_by_date = value;
                self.refresh_bookmark_browser();
                self.bookmark_browser.save()?;
            }
            Command::Reverse => {
                self.bookmark_browser.reverse = !self.bookmark_browser.reverse;
                self.refresh_bookmark_browser();
                self.bookmark_browser.save()?;
            }
            Command::ClearFilters => {
                let browser = &mut self.bookmark_browser;
                let prior = (browser.kind, browser.style, browser.color);
                browser.kind = None;
                browser.style = None;
                browser.color = None;
                if let Err(error) = browser.save() {
                    (browser.kind, browser.style, browser.color) = prior;
                    return Err(error);
                }
                browser.query_text.clear();
                browser.query = None;
                browser.search_types = [true; 3];
                browser.draft_search_types = [true; 3];
                browser.restricted = None;
                self.refresh_bookmark_browser();
            }
            Command::GotoBook(id) => {
                let page = self
                    .bookmark_browser
                    .rows
                    .iter()
                    .position(|r| *r == RowKey::Book(id));
                self.show(Screen::BookmarkBrowser(Page::Results));
                let ordinal = page.unwrap_or(0);
                self.set_bookmark_list_anchor(ordinal / self.list_geometry().rows, ordinal);
            }
            Command::BookInfo(id) => {
                self.bookmark_browser.book = Some(id);
                let cbz = self.library.state(id).is_some_and(metadata::is_cbz_book);
                self.bookmark_browser.catalog.queue([(id, cbz)]);
                self.show(Screen::BookmarkBrowser(Page::Book));
            }
            Command::ViewBook(id) => {
                if self.open_book_id() != Some(id) {
                    let entry = self.locate_tracked(id)?;
                    self.open_book(entry)?;
                } else {
                    self.show(if self.fixed.is_some() {
                        Screen::Fixed
                    } else {
                        Screen::Book
                    });
                }
            }
            Command::Inspect(key) => {
                self.bookmark_browser.focus = Some(key);
                self.bookmark_browser.reset_text();
                self.show(Screen::BookmarkBrowser(Page::Detail));
            }
            Command::ViewMark(key) => self.view_bookmark_in_book(key)?,
        }
        Ok(())
    }

    pub(super) fn bookmark_list_anchor(&self) -> Option<usize> {
        let Screen::BookmarkBrowser(page) = self.screen else {
            return None;
        };
        page.is_list().then(|| {
            self.bookmark_browser
                .list_anchor
                .filter(|&(page, _)| page == self.list_page)
                .map_or_else(
                    || {
                        self.flexible_mark_first()
                            .unwrap_or(self.list_page * self.list_geometry().rows)
                    },
                    |(_, ordinal)| ordinal,
                )
        })
    }

    pub(super) fn set_bookmark_list_anchor(&mut self, page: usize, ordinal: usize) {
        let page = self.flexible_mark_page_for(ordinal).unwrap_or(page);
        self.list_page = page;
        self.bookmark_browser.list_anchor = Some((page, ordinal));
    }

    fn refresh_bookmark_browser(&mut self) {
        let browser = &mut self.bookmark_browser;
        let anchor = (self.screen == Screen::BookmarkBrowser(Page::Results))
            .then_some(browser.list_anchor)
            .flatten()
            .and_then(|(_, ordinal)| Some((ordinal, *browser.rows.get(ordinal)?)));
        browser.rows.clear();
        browser.marks.clear();
        for listed in &mut browser.books {
            listed.count = 0;
            if browser
                .restricted
                .as_ref()
                .is_some_and(|ids| !ids.contains(&listed.id))
            {
                continue;
            }
            let Some(book) = self.library.state(listed.id) else {
                continue;
            };
            let mut marks = Vec::new();
            for locator in (0..book.bookmarks.len())
                .map(Locator::Bookmark)
                .chain((0..book.highlights.len()).map(Locator::Reflow))
                .chain((0..book.pdf_highlights.len()).map(Locator::Fixed))
            {
                let Some(mark) = get_mark(
                    book,
                    locator,
                    |p| listed.chapters.get(p).copied(),
                    listed.fixed,
                ) else {
                    continue;
                };
                if !browser.search_types[mark.kind().index()]
                    || browser.kind.is_some_and(|kind| mark.kind() != kind)
                    || browser.style.is_some_and(|style| mark.style != Some(style))
                    || browser.color.is_some_and(|color| mark.color != Some(color))
                    || browser.query.as_ref().is_some_and(|query| {
                        query.matches(mark.quote, 1).is_empty()
                            && mark
                                .note
                                .is_none_or(|note| query.matches(note, 1).is_empty())
                    })
                {
                    continue;
                }
                marks.push((
                    mark.location,
                    mark.created_at,
                    MarkKey {
                        book: book.id,
                        locator,
                    },
                ));
            }
            marks.sort_by(|(left, left_date, _), (right, right_date, _)| {
                let location = || {
                    left.0
                        .cmp(&right.0)
                        .then_with(|| left.1.total_cmp(&right.1))
                        .then_with(|| left.2.total_cmp(&right.2))
                };
                let directed = |order: std::cmp::Ordering| {
                    if browser.reverse {
                        order.reverse()
                    } else {
                        order
                    }
                };
                if browser.sort_by_date {
                    match (left_date, right_date) {
                        (Some(left), Some(right)) => directed(left.cmp(right).then_with(location)),
                        _ => right_date
                            .is_some()
                            .cmp(&left_date.is_some())
                            .then_with(|| directed(location())),
                    }
                } else {
                    directed(location())
                }
            });
            if !marks.is_empty() {
                listed.count = marks.len();
                browser.rows.push(RowKey::Book(listed.id));
                for (_, _, mark) in marks {
                    browser.rows.push(RowKey::Mark(mark));
                    browser.marks.push(mark);
                }
            }
        }
        if let Some((ordinal, row)) = anchor
            && browser.rows.get(ordinal) != Some(&row)
        {
            browser.reset_list_anchor();
        }
    }

    fn view_bookmark_in_book(&mut self, key: MarkKey) -> Result<()> {
        if self.open_book_id() == Some(key.book) {
            return self.view_bookmark_in_book_now(key);
        }
        self.request_close(super::profile_panel::on_close::AfterClose::Mark(key))?;
        Ok(())
    }

    pub(super) fn view_bookmark_in_book_now(&mut self, key: MarkKey) -> Result<()> {
        if self.open_book_id() != Some(key.book) {
            let entry = self.locate_tracked(key.book)?;
            let deferred = if crate::epub_members::is_folder_book(&entry.path) {
                Some(DeferredMark::capture(&self.library, key)?)
            } else {
                None
            };
            self.open_book_now(entry)?;
            if self.folder_open.is_some() {
                self.attach_folder_open_mark(deferred.ok_or("That mark is unavailable")?)?;
                return Ok(());
            }
            if self.screen == Screen::PdfPassword {
                self.bookmark_browser.after_password = Some(key);
                return Ok(());
            }
        }
        if self.open_book_id() != Some(key.book) {
            return Err("That book is unavailable".into());
        }
        self.activate(match key.locator {
            Locator::Bookmark(index) => Action::Bookmark(index),
            Locator::Reflow(index) => Action::Highlight(index),
            Locator::Fixed(index) => Action::PdfHighlight(index),
        })
    }

    pub(super) fn bookmark_after_password(&mut self) -> Result<()> {
        if self.screen != Screen::PdfPassword
            && let Some(key) = self.bookmark_browser.after_password.take()
        {
            self.view_bookmark_in_book_now(key)?;
        }
        Ok(())
    }

    fn bookmark_book_available(&self, id: u64) -> bool {
        self.bookmark_browser
            .books
            .iter()
            .any(|book| book.id == id && !book.archived)
    }
}

impl Reader {
    pub(super) fn bookmark_window(
        &self,
        page: Page,
        first: usize,
        count: usize,
    ) -> (usize, Vec<Item>) {
        if page == Page::Results {
            return (
                self.bookmark_browser.rows.len(),
                self.bookmark_browser
                    .rows
                    .iter()
                    .skip(first)
                    .take(count)
                    .filter_map(|key| self.bookmark_result_row(*key))
                    .collect(),
            );
        }
        let rows = self.bookmark_rows(page);
        (
            rows.len(),
            rows.into_iter().skip(first).take(count).collect(),
        )
    }

    fn bookmark_result_row(&self, key: RowKey) -> Option<Item> {
        match key {
            RowKey::Book(id) => {
                let book = self.library.state(id)?;
                let listed = self
                    .bookmark_browser
                    .books
                    .iter()
                    .find(|book| book.id == id)?;
                let detail = if listed.archived {
                    format!("{} · Archived", mark_count(listed.count))
                } else {
                    mark_count(listed.count)
                };
                Some(item(author_title(book), Command::BookInfo(id)).detail(detail))
            }
            RowKey::Mark(key) => {
                let mark = self.bookmark_mark(key)?;
                let note = mark.note.filter(|note| !note.trim().is_empty());
                let title = match self.controls.annotation_list_text {
                    AnnotationListText::HighlightedText => excerpt(mark.quote, 22),
                    AnnotationListText::NoteIfSet => excerpt(note.unwrap_or(mark.quote), 22),
                    AnnotationListText::HighlightAndNote => match note {
                        Some(note) if !mark.quote.trim().is_empty() => {
                            format!("{} · {}", excerpt(mark.quote, 11), excerpt(note, 11))
                        }
                        Some(note) => excerpt(note, 22),
                        None => excerpt(mark.quote, 22),
                    },
                };
                Some(item(title, Command::Inspect(key)).detail(format!(
                    "{} · {}",
                    mark.kind().label(),
                    mark.location_label
                )))
            }
        }
    }

    fn bookmark_mark(&self, key: MarkKey) -> Option<Mark<'_>> {
        let state = self.library.state(key.book)?;
        let listed = self
            .bookmark_browser
            .books
            .iter()
            .find(|book| book.id == key.book)?;
        get_mark(
            state,
            key.locator,
            |name| listed.chapters.get(name).copied(),
            listed.fixed,
        )
    }

    fn review_location_text(&self, location: ReviewLocation<'_>) -> Option<String> {
        let language = self.controls.interface_language;
        if let Some(fixed) = &self.fixed {
            let page = match location {
                ReviewLocation::Saved(position) if position.chapter == 0 => position.page,
                ReviewLocation::Fixed(page) => page,
                _ => return None,
            };
            return (page < fixed.reading.count())
                .then(|| format!("{} {}", language.bookmark_text("Page"), page + 1));
        }
        let reading = &self.active.as_ref()?.reading;
        let probe = &reading.probe;
        let (chapter, anchor) = match location {
            ReviewLocation::Saved(position) => match &position.anchor {
                Some(anchor) => {
                    if anchor.book != probe.book {
                        return None;
                    }
                    (probe.chapter_index(&anchor.chapter)?, Some(anchor))
                }
                None => (position.chapter, None),
            },
            ReviewLocation::Reflow(anchor) => {
                if anchor.book != probe.book {
                    return None;
                }
                (probe.chapter_index(&anchor.chapter)?, Some(anchor))
            }
            ReviewLocation::Fixed(_) => return None,
        };
        if chapter >= probe.chapter_count() {
            return None;
        }
        if let Some(anchor) = anchor.filter(|anchor| anchor.raw_caret.is_some()) {
            let loaded;
            let chapter_probe = if chapter == reading.chapter {
                probe
            } else {
                loaded = reading.config.chapter(chapter).ok()?;
                &loaded
            };
            chapter_probe.page_for(anchor)?;
        }
        let mut result = format!("{} {}", language.bookmark_text("Chapter"), chapter + 1);
        if chapter == reading.chapter
            && let Some(page) = anchor.and_then(|anchor| probe.page_for(anchor))
        {
            result.push_str(&format!(
                " · {} {}",
                language.bookmark_text("Page"),
                page + 1
            ));
        }
        Some(result)
    }

    fn bookmark_rows(&self, page: Page) -> Vec<Item> {
        let browser = &self.bookmark_browser;
        let language = self.controls.interface_language;
        use Command as C;
        match page {
            Page::Sources => {
                let mut rows = vec![
                    item("History", C::Use(Source::History)),
                    item("Collections", C::Go(Page::Collections)),
                    item("Selected books", C::Go(Page::ChooseBooks)),
                    item("Archived books", C::Use(Source::Archive)),
                    item("Documents", C::Use(Source::Folder(PathBuf::new(), false))),
                    item(
                        "Documents and subfolders",
                        C::Use(Source::Folder(PathBuf::new(), true)),
                    ),
                    item("Choose a folder", C::OpenFolder(PathBuf::new(), false)),
                    item(
                        "Choose a folder with subfolders",
                        C::OpenFolder(PathBuf::new(), true),
                    ),
                    Item::new("Hold a source to make it the default", Action::Heading),
                ];
                if matches!(browser.origin, Screen::Book | Screen::Fixed)
                    && self.open_book_id().is_some()
                {
                    rows.insert(0, item("Current book", C::UseCurrentBook));
                }
                for row in &mut rows {
                    if let Action::BookmarkBrowser(C::Use(source)) = &row.action
                        && browser.default_source.as_ref() == Some(source)
                    {
                        row.detail = Some("Default source".into());
                    }
                }
                if let Some(source) = &browser.default_source {
                    rows.push(item(
                        "Clear default source",
                        C::ToggleDefault(source.clone()),
                    ));
                }
                // Only fixed source names and statuses occur in this branch.
                for row in &mut rows {
                    row.title = language.bookmark_text(&row.title).to_owned();
                    if let Some(detail) = &mut row.detail {
                        *detail = language.bookmark_text(detail).to_owned();
                    }
                }
                rows
            }
            Page::Filters => {
                let kind = browser.kind.map_or("All types", Kind::label);
                let style = browser.style.map_or("All styles", HighlightStyle::label);
                let mut rows = vec![
                    item("Clear filters", C::ClearFilters),
                    item("Highlight color", C::Go(Page::Colors))
                        .detail(browser.color.map_or("All colors", HighlightColor::label)),
                    item("Books with matches", C::Go(Page::Books)),
                    item("Filter by book", C::RestrictBooks),
                    item("Sort direction", C::Reverse).detail(if browser.reverse {
                        "Reverse"
                    } else {
                        "Forward"
                    }),
                    selection_row("Reading order", C::SortByDate(false), !browser.sort_by_date),
                    selection_row("Date added", C::SortByDate(true), browser.sort_by_date),
                    item("All types", C::SetKind(None)).detail(kind),
                ];
                if matches!(browser.origin, Screen::Book | Screen::Fixed)
                    && self.open_book_id().is_some()
                {
                    rows.insert(
                        1,
                        item(
                            "Remove matching notes (this book)",
                            C::PreviewRemoveCurrentNotes,
                        ),
                    );
                    rows.insert(
                        2,
                        item(
                            "Remove matching marks (this book)",
                            C::PreviewRemoveCurrentMarks,
                        ),
                    );
                }
                if (browser.origin == Screen::Fixed && self.fixed.is_some())
                    || (browser.origin == Screen::Book && self.active.is_some())
                {
                    rows.insert(0, item("Show current page in results", C::FocusCurrentPage));
                    rows.insert(1, item("Latest saved mark", C::InspectLatestMark));
                }
                rows.extend(
                    [Kind::Bookmark, Kind::Highlight, Kind::Note]
                        .into_iter()
                        .map(|kind| {
                            selection_row(
                                kind.label(),
                                C::SetKind(Some(kind)),
                                browser.kind == Some(kind),
                            )
                        }),
                );
                rows.push(item("All styles", C::SetStyle(None)).detail(style));
                rows.extend(HighlightStyle::ALL.into_iter().map(|style| {
                    selection_row(
                        style.label(),
                        C::SetStyle(Some(style)),
                        browser.style == Some(style),
                    )
                }));
                // Filter rows use only built-in labels and enum values.
                for row in &mut rows {
                    row.title = language.bookmark_text(&row.title).to_owned();
                    if let Some(detail) = &mut row.detail {
                        *detail = language.bookmark_text(detail).to_owned();
                    }
                }
                rows
            }
            Page::RemoveNotesConfirm => {
                let mut rows = Vec::new();
                if let Some(pending) = &browser.pending_note_removal {
                    let message = if pending.selected_count() == 0 {
                        language.bookmark_text("No notes selected").to_owned()
                    } else if language.is_english() {
                        format!(
                            "Delete {} selected notes in this book?",
                            pending.selected_count()
                        )
                    } else {
                        format!(
                            "Xóa {} ghi chú đã chọn trong sách này?",
                            pending.selected_count()
                        )
                    };
                    rows.push(Item::new(message, Action::Heading));
                    rows.push(Item::new(pending.title.clone(), Action::Heading));
                    rows.push(Item::new(
                        language.bookmark_text("Bookmarks and highlights will remain"),
                        Action::Heading,
                    ));
                    rows.push(item(
                        language.bookmark_text("Choose notes"),
                        C::Go(Page::SelectNotes),
                    ));
                    if pending.selected_count() > 0 {
                        rows.push(item(
                            language.bookmark_text("Delete notes"),
                            C::AskRemoveCurrentNotes,
                        ));
                    }
                }
                rows.push(item(language.bookmark_text("Cancel"), C::Go(Page::Filters)));
                rows
            }
            Page::SelectNotes => {
                let mut rows = vec![
                    item(
                        language.bookmark_text("Select all"),
                        C::SetAllReviewedNotes(true),
                    ),
                    item(
                        language.bookmark_text("Clear selection"),
                        C::SetAllReviewedNotes(false),
                    ),
                    item(
                        language.bookmark_text("Done"),
                        C::Go(Page::RemoveNotesConfirm),
                    ),
                ];
                if let Some(pending) = &browser.pending_note_removal {
                    rows.extend(pending.targets.iter().enumerate().map(|(index, item_)| {
                        let (note, quote, chapter, location) = item_.target.preview();
                        let selection = language.bookmark_text(if item_.selected {
                            "Selected"
                        } else {
                            "Not selected"
                        });
                        let mut detail = selection.to_owned();
                        if let Some(location) = self.review_location_text(location) {
                            detail.push_str(" · ");
                            detail.push_str(&location);
                        }
                        if let Some(title) = chapter.filter(|title| !title.trim().is_empty()) {
                            detail.push_str(" · ");
                            detail.push_str(title);
                        }
                        detail.push_str(" · ");
                        detail.push_str(&excerpt(quote, 8));
                        item(excerpt(note, 22), C::ToggleReviewedNote(index)).detail(detail)
                    }));
                }
                rows
            }
            Page::RemoveMarksConfirm => {
                let mut rows = Vec::new();
                if let Some(pending) = &browser.pending_mark_removal {
                    let count = pending.selected_count();
                    let message = if count == 0 {
                        language.bookmark_text("No marks selected").to_owned()
                    } else if language.is_english() {
                        format!("Delete {count} selected marks in this book?")
                    } else {
                        format!("Xóa {count} dấu đã chọn trong sách này?")
                    };
                    rows.push(Item::new(message, Action::Heading));
                    rows.push(Item::new(pending.title.clone(), Action::Heading));
                    rows.push(Item::new(
                        language.bookmark_text("Selected marks and their notes will be deleted"),
                        Action::Heading,
                    ));
                    rows.push(item(
                        language.bookmark_text("Choose marks"),
                        C::Go(Page::SelectMarks),
                    ));
                    if count > 0 {
                        rows.push(item(
                            language.bookmark_text("Delete marks"),
                            C::AskRemoveCurrentMarks,
                        ));
                    }
                }
                rows.push(item(language.bookmark_text("Cancel"), C::Go(Page::Filters)));
                rows
            }
            Page::ConfirmClearNote => {
                let mut rows = Vec::new();
                if let Some(pending) = &browser.pending_note_edit {
                    rows.push(Item::new(
                        language.bookmark_text("Remove this note? The saved mark will remain."),
                        Action::Heading,
                    ));
                    rows.push(Item::new(pending.title.clone(), Action::Heading));
                    rows.push(item(
                        language.bookmark_text("Remove note"),
                        C::ConfirmClearCurrentNote,
                    ));
                    rows.push(item(
                        language.bookmark_text("Keep editing"),
                        C::ResumeCurrentNoteEdit,
                    ));
                }
                rows.push(item(
                    language.bookmark_text("Cancel"),
                    C::CancelCurrentNoteEdit,
                ));
                rows
            }
            Page::SelectMarks => {
                let mut rows = vec![
                    item(
                        language.bookmark_text("Select all"),
                        C::SetAllReviewedMarks(true),
                    ),
                    item(
                        language.bookmark_text("Clear selection"),
                        C::SetAllReviewedMarks(false),
                    ),
                    item(
                        language.bookmark_text("Done"),
                        C::Go(Page::RemoveMarksConfirm),
                    ),
                ];
                if let Some(pending) = &browser.pending_mark_removal {
                    rows.extend(pending.targets.iter().enumerate().map(|(index, reviewed)| {
                        let (kind, text, note, chapter, location) = reviewed.target.preview();
                        let selected = language.bookmark_text(if reviewed.selected {
                            "Selected"
                        } else {
                            "Not selected"
                        });
                        let kind_label = language.bookmark_text(kind);
                        let mut detail = format!("{selected} · {kind_label}");
                        if let Some(location) = self.review_location_text(location) {
                            detail.push_str(" · ");
                            detail.push_str(&location);
                        }
                        if let Some(title) = chapter.filter(|title| !title.trim().is_empty()) {
                            detail.push_str(" · ");
                            detail.push_str(title);
                        }
                        if let Some(note) = note.filter(|note| !note.trim().is_empty()) {
                            detail.push_str(" · ");
                            detail.push_str(language.bookmark_text("Note"));
                            detail.push_str(": ");
                            detail.push_str(&excerpt(note, 8));
                        }
                        let title = if text.trim().is_empty() {
                            kind_label.to_owned()
                        } else {
                            excerpt(text, 22)
                        };
                        item(title, C::ToggleReviewedMark(index)).detail(detail)
                    }));
                }
                rows
            }
            Page::Colors => Vec::new(),
            Page::Books => browser
                .books
                .iter()
                .filter(|book| book.count > 0)
                .filter_map(|book| {
                    Some(
                        item(
                            author_title(self.library.state(book.id)?),
                            C::GotoBook(book.id),
                        )
                        .detail(language.bookmark_mark_count(book.count)),
                    )
                })
                .collect(),
            Page::ChooseBooks => {
                let chosen = browser.picking.as_ref().unwrap_or(&browser.picked_books);
                let mut rows = vec![
                    item("Apply book selection", C::ApplyChoices)
                        .detail(language.bookmark_selected_count(chosen.len())),
                    item("Select or clear all", C::ToggleAllBooks),
                    item("Select by author", C::Go(Page::Authors)),
                    item(
                        "Select by series",
                        C::Go(Page::Property(metadata::Property::Series)),
                    ),
                    item(
                        "Select by language",
                        C::Go(Page::Property(metadata::Property::Language)),
                    ),
                    item(
                        "Select by keyword",
                        C::Go(Page::Property(metadata::Property::Keywords)),
                    ),
                ];
                // Only the fixed control rows exist before the book rows are appended.
                for row in &mut rows {
                    row.title = language.bookmark_text(&row.title).to_owned();
                }
                let mut candidates = self
                    .library
                    .recents()
                    .iter()
                    .filter(|book| has_marks(book))
                    .filter(|book| {
                        browser.picking.is_none()
                            || browser.books.iter().any(|listed| listed.id == book.id)
                    })
                    .collect::<Vec<_>>();
                candidates.sort_by_key(|book| fold_search(&author_title(book)));
                rows.extend(candidates.into_iter().map(|book| {
                    let selected = chosen.contains(&book.id);
                    let mut row =
                        selection_row(author_title(book), C::ToggleBook(book.id), selected);
                    if selected {
                        row.detail = Some(language.bookmark_text("Selected").to_owned());
                    }
                    row
                }));
                rows
            }
            Page::Authors => {
                let authors: BTreeSet<_> = self
                    .library
                    .recents()
                    .iter()
                    .filter(|book| has_marks(book))
                    .filter(|book| {
                        browser.picking.is_none() || browser.books.iter().any(|b| b.id == book.id)
                    })
                    .flat_map(|book| book.author.as_deref().unwrap_or("").lines())
                    .map(str::trim)
                    .filter(|author| !author.is_empty())
                    .map(str::to_owned)
                    .collect();
                let mut rows = vec![item(
                    language.bookmark_text("Unknown author"),
                    C::SelectAuthor(None),
                )];
                rows.extend(
                    authors
                        .into_iter()
                        .map(|author| item(&author, C::SelectAuthor(Some(author.clone())))),
                );
                rows
            }
            Page::Property(property) => self.bookmark_property_rows(property),
            Page::Collections => {
                let mut rows = vec![
                    item(language.bookmark_text("Apply collections"), C::ApplyChoices)
                        .detail(language.bookmark_selected_count(browser.picked_collections.len())),
                ];
                rows.extend(self.library.collections().iter().map(|collection| {
                    let selected = browser.picked_collections.contains(&collection.name);
                    let mut row = selection_row(
                        &collection.name,
                        C::ToggleCollection(collection.name.clone()),
                        selected,
                    );
                    if selected {
                        row.detail = Some(language.bookmark_text("Selected").to_owned());
                    }
                    row
                }));
                rows
            }
            Page::Folder => {
                let mut rows = vec![
                    item(
                        language.bookmark_text(if browser.recursive {
                            "Use this folder and subfolders"
                        } else {
                            "Use this folder"
                        }),
                        C::Use(Source::Folder(browser.folder.clone(), browser.recursive)),
                    )
                    .detail(crate::library::display_filename(
                        &browser.folder.to_string_lossy(),
                    )),
                ];
                if let Some(parent) = browser.folder.parent() {
                    rows.push(item(
                        language.bookmark_text("Up"),
                        C::OpenFolder(parent.to_owned(), browser.recursive),
                    ));
                }
                rows.extend(browser.subfolders.iter().map(|path| {
                    item(
                        crate::library::display_filename(
                            &path.file_name().unwrap_or_default().to_string_lossy(),
                        ),
                        C::OpenFolder(path.clone(), browser.recursive),
                    )
                }));
                rows
            }
            Page::Book => self.bookmark_metadata_rows(),
            Page::Detail | Page::Search | Page::Results | Page::Description | Page::EditNote => {
                Vec::new()
            }
        }
    }

    pub(super) fn draw_bookmark_browser_controls(&self, canvas: &mut Canvas, page: Page) -> bool {
        let layout = BrowserLayout(self.navigation_layout());
        let language = self.controls.interface_language;
        match page {
            Page::SelectNotes | Page::SelectMarks => {
                canvas.button(
                    layout.select_note_page(),
                    language.bookmark_text("Select page"),
                    false,
                    false,
                );
                false
            }
            Page::Colors => {
                super::highlight_palette::draw(
                    canvas,
                    self.bookmark_browser.color.is_none(),
                    |color| self.bookmark_browser.color == Some(color),
                    false,
                    language,
                );
                if let Some(notice) = &self.notice {
                    canvas.text(layout.body(944, 180), notice, CAPTION.lines(4));
                }
                true
            }
            Page::Results => {
                let browser = &self.bookmark_browser;
                let books = browser.books.iter().filter(|book| book.count > 0).count();
                let types = if browser.search_types == [true; 3] {
                    ""
                } else {
                    language.bookmark_text("Types filtered")
                };
                let mut summary = format!(
                    "{} · {}",
                    browser.source_label,
                    language.bookmark_counts(browser.marks.len(), books)
                );
                if !types.is_empty() {
                    summary.push_str(" · ");
                    summary.push_str(types);
                }
                canvas.text(layout.0.footer_text(40, 246, 40), &summary, CAPTION);
                canvas.button(
                    layout.control(0, 198, 72),
                    language.bookmark_text("Sources"),
                    false,
                    false,
                );
                canvas.button(
                    layout.control(1, 198, 72),
                    language.bookmark_text("Filters"),
                    false,
                    false,
                );
                canvas.button(
                    layout.control(2, 198, 72),
                    language.bookmark_text("Find"),
                    false,
                    false,
                );
                false
            }
            Page::Search => {
                canvas.single_line_field(
                    layout.search_field(self.clipboard_text.is_some()),
                    &self.editor,
                    language.bookmark_text("Bookmark, highlighted text or note"),
                    false,
                );
                if self.clipboard_text.is_some() {
                    canvas.button(
                        layout.search_paste(),
                        language.bookmark_text("Paste"),
                        false,
                        false,
                    );
                }
                canvas.text(
                    layout.body(284, 28),
                    language.bookmark_text("Search options"),
                    CAPTION,
                );
                canvas.button(
                    layout.search_option(0),
                    language.bookmark_text("Case"),
                    self.bookmark_browser.case_sensitive,
                    false,
                );
                for (index, label) in ["Bookmark", "Highlight", "Note"].into_iter().enumerate() {
                    canvas.button(
                        layout.search_option(index + 1),
                        language.bookmark_text(label),
                        self.bookmark_browser.draft_search_types[index],
                        false,
                    );
                }
                canvas.keyboard(self.editor.layer, language.bookmark_text("Find"), None);
                true
            }
            Page::Detail => {
                self.draw_bookmark_detail(canvas);
                if edit_note::can_edit(self) {
                    canvas.button(
                        layout.detail_action(0),
                        language.bookmark_text("Edit"),
                        false,
                        false,
                    );
                }
                if edit_note::can_edit_current(self) {
                    canvas.button(
                        layout.detail_action(1),
                        language.bookmark_text("Remove"),
                        false,
                        false,
                    );
                }
                true
            }
            Page::EditNote => {
                edit_note::draw(self, canvas, layout);
                true
            }
            Page::Description => {
                self.draw_bookmark_description(canvas);
                true
            }
            _ => false,
        }
    }

    pub(super) fn draw_bookmark_confirmation(&self, canvas: &mut Canvas) {
        confirmation::draw(self, canvas);
    }

    fn draw_bookmark_detail(&self, canvas: &mut Canvas) {
        let layout = BrowserLayout(self.navigation_layout());
        let language = self.controls.interface_language;
        let browser = &self.bookmark_browser;
        let Some(key) = browser.focus else {
            return;
        };
        let (Some(book), Some(mark)) = (self.library.state(key.book), self.bookmark_mark(key))
        else {
            return;
        };
        let index = browser
            .marks
            .iter()
            .position(|item| *item == key)
            .unwrap_or(0);
        let mut title = format!(
            "{} · {} · {}",
            book.title,
            language.bookmark_text(mark.kind().label()),
            mark.location_label
        );
        if let Some(date) = mark.created_at.and_then(crate::mark_time::local_date) {
            title.push_str(" · ");
            title.push_str(&date);
        }
        canvas.text(
            Rect::new(272, 152, layout.0.width - 544, 94),
            &title,
            CAPTION.center().lines(2),
        );
        if index > 0 {
            canvas.button(FIRST_MARK, language.bookmark_text("First"), false, false);
            canvas.button(
                layout.control(0, 115, 84),
                language.bookmark_text("Previous mark"),
                false,
                false,
            );
        }
        if index + 1 < browser.marks.len() {
            canvas.button(
                layout.last_mark(),
                language.bookmark_text("Last"),
                false,
                false,
            );
            canvas.button(
                layout.control(2, 115, 84),
                language.bookmark_text("Next mark"),
                false,
                false,
            );
        }
        if self.bookmark_book_available(book.id) {
            canvas.button(
                layout.control(1, 115, 84),
                language.bookmark_text("View in book"),
                true,
                false,
            );
        } else {
            canvas.text(
                layout.control(1, 115, 84),
                language.bookmark_text("Archived mark"),
                CAPTION.center(),
            );
        }
        if !mark.quote.is_empty() {
            canvas.button(
                layout.copy_action(false),
                language.bookmark_text(if mark.style.is_some() {
                    "Copy quote"
                } else {
                    "Copy text"
                }),
                false,
                false,
            );
        }
        if mark.note.is_some_and(|note| !note.is_empty()) {
            canvas.button(
                layout.copy_action(true),
                language.bookmark_text("Copy note"),
                false,
                false,
            );
        }
        let text = mark.note.map_or_else(
            || mark.quote.to_owned(),
            |note| format!("{}\n\n{}", note, mark.quote),
        );
        let (text_page, count) = self.draw_bookmark_text(canvas, layout.text(), &text);
        if count > 1 {
            if text_page > 0 {
                canvas.button(
                    layout.control(0, 214, 72),
                    language.bookmark_text("Previous text"),
                    false,
                    false,
                );
            }
            if text_page + 1 < count {
                canvas.button(
                    layout.control(2, 214, 72),
                    language.bookmark_text("Next text"),
                    false,
                    false,
                );
            }
            canvas.text(
                layout.centered_caption(199),
                &language.bookmark_page(text_page + 1, count),
                CAPTION.center(),
            );
        }
    }

    fn draw_bookmark_text(&self, canvas: &mut Canvas, bounds: Rect, text: &str) -> (usize, usize) {
        let style = BODY.lines(19).break_long_words();
        let pages = canvas.text_pages(bounds, text, style);
        let current = self.bookmark_browser.text_page(&pages);
        if let Some(range) = pages.get(current) {
            canvas.text(bounds, &text[range.clone()], style);
        }
        let count = pages.len();
        self.bookmark_browser.replace_text_pages(pages);
        (current, count)
    }

    pub(super) fn bookmark_browser_input(&mut self, page: Page, input: Input) -> Option<Change> {
        if let Some(change) = confirmation::input(self, input) {
            return Some(change);
        }
        let layout = BrowserLayout(self.navigation_layout());
        if matches!(input, Input::Tap(x, y) if layout.0.back().contains(x, y)) {
            self.back_from_bookmark_browser(page);
            return Some(Change::Partial);
        }
        if page == Page::Detail
            && edit_note::can_edit(self)
            && matches!(input, Input::Tap(x, y) if layout.detail_action(0).contains(x, y))
        {
            return Some(self.bookmark_change(Command::EditCurrentNote));
        }
        if page == Page::Detail
            && edit_note::can_edit_current(self)
            && matches!(input, Input::Tap(x, y) if layout.detail_action(1).contains(x, y))
        {
            return Some(self.bookmark_change(Command::AskRemoveFocusedMark));
        }
        if page == Page::Detail
            && let Input::Tap(x, y) = input
        {
            let note = layout.copy_action(true).contains(x, y);
            if note || layout.copy_action(false).contains(x, y) {
                let text = self
                    .bookmark_browser
                    .focus
                    .and_then(|key| self.bookmark_mark(key))
                    .and_then(|mark| {
                        let value = if note { mark.note? } else { mark.quote };
                        (!value.is_empty()).then_some(value)
                    });
                if let Some(value) = text {
                    if value.len() > crate::library::MAX_HIGHLIGHT_TEXT_BYTES {
                        return Some(self.report("The saved text is too long to copy"));
                    }
                    self.clipboard_text = Some(value.to_owned());
                    self.notice = Some(
                        self.controls
                            .interface_language
                            .bookmark_text(if note { "Note copied" } else { "Text copied" })
                            .to_owned(),
                    );
                    return Some(Change::Partial);
                }
            }
        }
        if page == Page::EditNote {
            return edit_note::input(self, input, layout);
        }
        if matches!(page, Page::SelectNotes | Page::SelectMarks)
            && matches!(input, Input::Tap(x, y) if layout.select_note_page().contains(x, y))
        {
            let action = if page == Page::SelectNotes {
                Command::SelectReviewedNotePage
            } else {
                Command::SelectReviewedMarkPage
            };
            return Some(self.bookmark_change(action));
        }
        if page == Page::Colors {
            if input == Input::PageBack {
                self.back_from_bookmark_browser(page);
                return Some(Change::Partial);
            }
            return Some(match input {
                Input::Tap(x, y) => super::highlight_palette::at_width(layout.0.width, x, y)
                    .map_or(Change::None, |color| {
                        self.bookmark_change(Command::SetColor(color))
                    }),
                _ => Change::None,
            });
        }
        if page == Page::Description {
            return Some(self.bookmark_description_input(input));
        }
        if page == Page::Search {
            return Some(self.bookmark_search_input(input));
        }
        if page == Page::Detail {
            return Some(self.bookmark_detail_input(input));
        }
        if let Input::LongTap(x, y) = input {
            let (count, rows) = self.list_window();
            let geometry = self.list_geometry();
            let row = geometry.item_at(self.list_page, count, x, y)? % geometry.rows;
            let source = match rows.get(row).map(|item| &item.action) {
                Some(Action::BookmarkBrowser(Command::Use(source))) => Some(source.clone()),
                Some(Action::BookmarkBrowser(Command::ApplyChoices))
                    if page == Page::Collections =>
                {
                    Some(Source::Collections(
                        self.bookmark_browser.picked_collections.clone(),
                    ))
                }
                Some(Action::BookmarkBrowser(Command::ApplyChoices))
                    if page == Page::ChooseBooks && self.bookmark_browser.picking.is_none() =>
                {
                    Some(Source::Books(self.bookmark_browser.picked_books.clone()))
                }
                _ => None,
            }?;
            return Some(self.bookmark_change(Command::ToggleDefault(source)));
        }
        if page == Page::Results
            && let Input::Tap(x, y) = input
        {
            let page = if contains(layout.control(0, 198, 72), x, y) {
                Page::Sources
            } else if contains(layout.control(1, 198, 72), x, y) {
                Page::Filters
            } else if contains(layout.control(2, 198, 72), x, y) {
                Page::Search
            } else {
                return None;
            };
            return Some(self.bookmark_change(Command::Go(page)));
        }
        None
    }

    fn bookmark_search_input(&mut self, input: Input) -> Change {
        let Input::Tap(x, y) = input else {
            return Change::None;
        };
        let layout = BrowserLayout(self.navigation_layout());
        if contains(layout.search_paste(), x, y) {
            if let Some(text) = self.clipboard_text.as_deref() {
                let existing_bytes: usize = self.editor.text.iter().map(|ch| ch.len_utf8()).sum();
                if existing_bytes.saturating_add(text.len()) > crate::search::MAX_QUERY_BYTES
                    || self.editor.text.len().saturating_add(text.chars().count()) > self.editor.max
                {
                    return self.report("The copied text is too long for this search");
                }
                let caret = self.editor.caret.min(self.editor.text.len());
                let added = text.chars().count();
                self.editor.text.splice(caret..caret, text.chars());
                self.editor.caret = caret + added;
                self.notice = None;
                return Change::Partial;
            }
        }
        if contains(layout.search_option(0), x, y) {
            self.bookmark_browser.case_sensitive = !self.bookmark_browser.case_sensitive;
            return Change::Partial;
        }
        for index in 0..3 {
            if contains(layout.search_option(index + 1), x, y) {
                self.bookmark_browser.draft_search_types[index] =
                    !self.bookmark_browser.draft_search_types[index];
                return Change::Partial;
            }
        }
        let layout = layout.0.keyboard();
        let action = self.controls.interface_language.bookmark_text("Find");
        let Some(key) = keyboard_key(&layout, &self.editor, action, x, y) else {
            return Change::None;
        };
        if matches!(self.editor.press(key), Press::Go) {
            let text = self.editor.string();
            let query = text.trim();
            self.bookmark_browser.query = if query.is_empty() {
                None
            } else {
                match Query::with_options(query, self.bookmark_browser.case_sensitive, false) {
                    Ok(query) => Some(query),
                    Err(error) => return self.report(error),
                }
            };
            self.bookmark_browser.query_text = query.to_owned();
            self.bookmark_browser.search_types = self.bookmark_browser.draft_search_types;
            self.refresh_bookmark_browser();
            self.show(Screen::BookmarkBrowser(Page::Results));
        }
        Change::Partial
    }

    fn back_from_bookmark_browser(&mut self, page: Page) {
        match page {
            Page::Sources if self.bookmark_browser.sources_from_results => {
                self.show(Screen::BookmarkBrowser(Page::Results));
            }
            Page::Sources | Page::Results => self.show(self.bookmark_browser.origin),
            Page::Detail => {
                let row = self.bookmark_browser.focus.and_then(|key| {
                    self.bookmark_browser
                        .rows
                        .iter()
                        .position(|value| *value == RowKey::Mark(key))
                });
                self.show(Screen::BookmarkBrowser(Page::Results));
                let ordinal = row.unwrap_or(0);
                self.set_bookmark_list_anchor(ordinal / self.list_geometry().rows, ordinal);
            }
            Page::ChooseBooks if self.bookmark_browser.picking.take().is_some() => {
                self.show(Screen::BookmarkBrowser(Page::Filters));
            }
            Page::ChooseBooks | Page::Collections | Page::Folder => {
                self.show(Screen::BookmarkBrowser(Page::Sources));
            }
            Page::Authors | Page::Property(_) => {
                self.show(Screen::BookmarkBrowser(Page::ChooseBooks))
            }
            Page::Description => self.show(Screen::BookmarkBrowser(Page::Book)),
            Page::Colors => self.show(Screen::BookmarkBrowser(Page::Filters)),
            Page::RemoveNotesConfirm => {
                self.bookmark_browser.pending_note_removal = None;
                self.show(Screen::BookmarkBrowser(Page::Filters));
            }
            Page::SelectNotes => self.show(Screen::BookmarkBrowser(Page::RemoveNotesConfirm)),
            Page::RemoveMarksConfirm => {
                self.bookmark_browser.pending_mark_removal = None;
                self.show(Screen::BookmarkBrowser(Page::Filters));
            }
            Page::SelectMarks => self.show(Screen::BookmarkBrowser(Page::RemoveMarksConfirm)),
            Page::EditNote => edit_note::cancel(self),
            Page::ConfirmClearNote => {
                if let Err(error) = edit_note::resume(self) {
                    self.report(error);
                }
            }
            _ => self.show(Screen::BookmarkBrowser(Page::Results)),
        }
    }

    fn bookmark_detail_input(&mut self, input: Input) -> Change {
        let layout = BrowserLayout(self.navigation_layout());
        let Some(key) = self.bookmark_browser.focus else {
            return Change::None;
        };
        let Some(index) = self
            .bookmark_browser
            .marks
            .iter()
            .position(|value| *value == key)
        else {
            return Change::None;
        };
        let last = self.bookmark_browser.marks.len().saturating_sub(1);
        let mut target = index;
        match input {
            Input::PageBack => target = index.saturating_sub(1),
            Input::PageForward => target = (index + 1).min(last),
            Input::Swipe { dx, dy, .. } if dx.abs() > dy.abs() => {
                target = if dx < 0.0 {
                    (index + 1).min(last)
                } else {
                    index.saturating_sub(1)
                };
            }
            Input::Swipe { dy, .. } => {
                return self.step_bookmark_text(if dy < 0.0 { 1 } else { -1 });
            }
            Input::Tap(x, y)
                if contains(layout.control(1, 115, 84), x, y)
                    && self.bookmark_book_available(key.book) =>
            {
                return self.bookmark_change(Command::ViewMark(key));
            }
            Input::Tap(x, y) if contains(layout.control(0, 115, 84), x, y) => {
                target = index.saturating_sub(1)
            }
            Input::Tap(x, y) if contains(layout.control(2, 115, 84), x, y) => {
                target = (index + 1).min(last)
            }
            Input::Tap(x, y) if contains(FIRST_MARK, x, y) => target = 0,
            Input::Tap(x, y) if contains(layout.last_mark(), x, y) => target = last,
            Input::Tap(x, y) if contains(layout.control(0, 214, 72), x, y) => {
                return self.step_bookmark_text(-1);
            }
            Input::Tap(x, y) if contains(layout.control(2, 214, 72), x, y) => {
                return self.step_bookmark_text(1);
            }
            _ => {}
        }
        if target == index {
            return Change::None;
        }
        self.bookmark_change(Command::Inspect(self.bookmark_browser.marks[target]))
    }

    fn step_bookmark_text(&mut self, step: isize) -> Change {
        let browser = &mut self.bookmark_browser;
        let pages = browser.text_pages.borrow();
        let current = browser.text_page(&pages);
        let target = current
            .saturating_add_signed(step)
            .min(pages.len().saturating_sub(1));
        if target != current
            && let Some(range) = pages.get(target)
        {
            browser.text_offset = range.start;
            Change::Partial
        } else {
            Change::None
        }
    }

    fn bookmark_change(&mut self, action: Command) -> Change {
        match self.activate_bookmark_browser(action) {
            Ok(()) => Change::Partial,
            Err(error) => self.report(error),
        }
    }
}

fn selection_row(label: impl Into<String>, action: Command, selected: bool) -> Item {
    let row = item(label, action);
    if selected {
        row.detail("Selected")
    } else {
        row
    }
}

#[cfg(test)]
mod tests;
