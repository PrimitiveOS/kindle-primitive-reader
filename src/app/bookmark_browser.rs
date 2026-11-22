//! Browse saved marks across the library without changing an open document.
use super::{Action, Book, Canvas, Change, Editor, HEIGHT, Input, Item, Reader, Rect, Result,
    Screen, WIDTH, with_storage};
use crate::highlight_style::HighlightStyle;
use crate::library::{BookState, BrowseKind, DocumentFormat, Library, fold_search};
use crate::search::Query;
use primitive_ui::{BODY, CAPTION, Layout, Press, keyboard_key};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use std::io::Read;
use std::path::{Path, PathBuf};

const TEXT: Rect = Rect::new(40, 258, WIDTH as i32 - 80, HEIGHT as i32 - 538);
const SOURCES: Rect = Rect::new(40, HEIGHT as i32 - 198, 386, 72);
const FILTERS: Rect = Rect::new(439, HEIGHT as i32 - 198, 386, 72);
const FIND: Rect = Rect::new(838, HEIGHT as i32 - 198, 386, 72);
const READ: Rect = Rect::new(439, HEIGHT as i32 - 115, 386, 84);
const PREV_MARK: Rect = Rect::new(40, HEIGHT as i32 - 115, 386, 84);
const NEXT_MARK: Rect = Rect::new(838, HEIGHT as i32 - 115, 386, 84);
const PREV_TEXT: Rect = Rect::new(40, HEIGHT as i32 - 214, 386, 72);
const NEXT_TEXT: Rect = Rect::new(838, HEIGHT as i32 - 214, 386, 72);
const FIRST_MARK: Rect = Rect::new(40, 147, 210, 70);
const LAST_MARK: Rect = Rect::new(WIDTH as i32 - 250, 147, 210, 70);
const CASE: Rect = Rect::new(40, 314, 386, 72);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Page {
    Sources,
    Results,
    Filters,
    Search,
    Detail,
    Books,
    ChooseBooks,
    Collections,
    Folder,
    Book,
}

impl Page {
    pub(super) const fn title(self) -> &'static str {
        match self {
            Self::Sources => "Bookmark sources",
            Self::Results => "Bookmark browser",
            Self::Filters => "Bookmark filters",
            Self::Search => "Find saved marks",
            Self::Detail => "Saved mark",
            Self::Books => "Books with matches",
            Self::ChooseBooks => "Choose books",
            Self::Collections => "Choose collections",
            Self::Folder => "Choose a folder",
            Self::Book => "Book information",
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
            Self::Collections(names) => !archived && names.iter().any(|name| {
                library.collection(name).is_some_and(|set| set.contains(book.id))
            }),
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

struct Mark<'a> {
    quote: &'a str,
    note: Option<&'a str>,
    style: Option<HighlightStyle>,
    location: (usize, f64, f64),
    location_label: String,
}

impl Mark<'_> {
    fn kind(&self) -> Kind {
        match (self.style, self.note) {
            (None, _) => Kind::Bookmark,
            (_, Some(_)) => Kind::Note,
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
    ToggleDefault(Source),
    OpenFolder(PathBuf, bool),
    ToggleBook(u64),
    ToggleCollection(String),
    ToggleAllBooks,
    ApplyChoices,
    RestrictBooks,
    SetKind(Option<Kind>),
    SetStyle(Option<HighlightStyle>),
    Reverse,
    ClearFilters,
    GotoBook(u64),
    BookInfo(u64),
    ViewBook(u64),
    Inspect(MarkKey),
    ViewMark(MarkKey),
}

struct ListedBook {
    id: u64,
    archived: bool,
    count: usize,
    chapters: HashMap<String, usize>,
}

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default)]
struct Preferences {
    source: Option<Source>,
    kind: Option<Kind>,
    style: Option<HighlightStyle>,
    reverse: bool,
}

pub(super) struct Browser {
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
    kind: Option<Kind>,
    style: Option<HighlightStyle>,
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
    text_page: usize,
    text_pages: std::cell::Cell<usize>,
    pub(super) after_password: Option<MarkKey>,
}

impl Default for Browser {
    fn default() -> Self {
        Self {
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
            kind: None,
            style: None,
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
            text_page: 0,
            text_pages: std::cell::Cell::new(0),
            after_password: None,
        }
    }
}

impl Browser {
    pub(super) fn load(path: PathBuf) -> Self {
        const LIMIT: usize = 1024 * 1024;
        let preferences: Preferences = (|| {
            let mut bytes = Vec::new();
            std::fs::File::open(&path).ok()?.take((LIMIT + 1) as u64).read_to_end(&mut bytes).ok()?;
            if bytes.len() > LIMIT { return None; }
            serde_json::from_slice(&bytes).ok()
        })().unwrap_or_default();
        Self {
            path,
            default_source: preferences.source,
            kind: preferences.kind,
            style: preferences.style,
            reverse: preferences.reverse,
            ..Self::default()
        }
    }

    fn save(&self) -> Result<()> {
        let bytes = serde_json::to_vec(&Preferences {
            source: self.default_source.clone(),
            kind: self.kind,
            style: self.style,
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
            format.is_still_image() || format.is_comic()
                || matches!(format, DocumentFormat::Pdf | DocumentFormat::Djvu)
        })
}

fn get_mark<'a>(
    book: &'a BookState,
    locator: Locator,
    chapter_for: impl Fn(&str) -> Option<usize>,
) -> Option<Mark<'a>> {
    Some(match locator {
        Locator::Bookmark(index) => {
            let mark = book.bookmarks.get(index)?;
            let position = &mark.position;
            let location = if is_fixed(book) {
                (position.page, 0.0, 0.0)
            } else {
                position.anchor.as_ref().map_or((position.chapter, position.page as f64, 0.0),
                    |anchor| (position.chapter, anchor.block as f64, anchor.offset as f64))
            };
            Mark { quote: &mark.label, note: None, style: None, location,
                location_label: if is_fixed(book) {
                    format!("Page {}", position.page + 1)
                } else {
                    format!("Chapter {} · Page {}", position.chapter + 1, position.page + 1)
                },
            }
        }
        Locator::Reflow(index) => {
            let mark = book.highlights.get(index)?;
            let anchor = &mark.selection.start;
            let chapter = chapter_for(&anchor.chapter).or_else(|| {
                book.bookmarks.iter().find_map(|mark| {
                    mark.position.anchor.as_ref()
                        .filter(|candidate| candidate.chapter == anchor.chapter)
                        .map(|_| mark.position.chapter)
                })
            });
            Mark { quote: &mark.selection.text, note: mark.note.as_deref(), style: Some(mark.style),
                location: (chapter.unwrap_or(usize::MAX), anchor.block as f64, anchor.offset as f64),
                location_label: chapter.map_or_else(
                    || "Saved text location".into(), |n| format!("Chapter {}", n + 1),
                ),
            }
        }
        Locator::Fixed(index) => {
            let mark = book.pdf_highlights.get(index)?;
            let (y, x) = mark.bounds.first().map_or((0.0, 0.0),
                |rect| (f64::from(rect.y0), f64::from(rect.x0)));
            Mark { quote: &mark.text, note: mark.note.as_deref(), style: Some(mark.style),
                location: (mark.page, y, x), location_label: format!("Page {}", mark.page + 1),
            }
        }
    })
}

fn item(text: impl Into<String>, action: Command) -> Item {
    Item::new(text, Action::BookmarkBrowser(action))
}

fn excerpt(text: &str) -> String {
    text.split_whitespace().take(22).collect::<Vec<_>>().join(" ")
}

fn author_title(book: &BookState) -> String {
    book.author.as_deref().map_or_else(|| book.title.clone(),
        |author| format!("{} · {}", author.lines().next().unwrap_or(author), book.title))
}

impl Reader {
    pub(super) fn activate_bookmark_browser(&mut self, action: Command) -> Result<()> {
        match action {
            Command::Open => {
                let browser = &mut self.bookmark_browser;
                browser.origin = match self.screen {
                    Screen::BookTools => Screen::Book,
                    Screen::FixedTools => Screen::Fixed,
                    other => other,
                };
                browser.query_text.clear();
                browser.query = None;
                browser.case_sensitive = false;
                browser.restricted = None;
                browser.after_password = None;
                browser.sources_from_results = false;
                if let Some(source) = browser.default_source.clone() {
                    return self.activate_bookmark_browser(Command::Use(source));
                }
                self.show(Screen::BookmarkBrowser(Page::Sources));
            }
            Command::Go(page) => {
                if page == Page::Sources {
                    self.bookmark_browser.sources_from_results = true;
                }
                if page == Page::Search {
                    self.editor = Editor::new(crate::search::MAX_QUERY_BYTES);
                    self.editor.text = self.bookmark_browser.query_text.chars().collect();
                    self.editor.caret = self.editor.text.len();
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
                self.show(Screen::BookmarkBrowser(page));
            }
            Command::Use(source) => {
                let mut books = with_storage(|| {
                    Ok(self.library.recents().iter().filter(|book| has_marks(book)).filter_map(|book| {
                        let archived = self.library.absolute_path(&book.path).is_err();
                        source.matches(&self.library, book, archived).then(|| {
                            (fold_search(&author_title(book)), ListedBook {
                                id: book.id, archived, count: 0,
                                chapters: if archived { HashMap::new() }
                                    else { chapter_order(&self.library, book) },
                            })
                        })
                    }).collect::<Vec<_>>())
                })?;
                books.sort_by(|(left, _), (right, _)| left.cmp(right));
                self.bookmark_browser.books = books.into_iter().map(|(_, book)| book).collect();
                self.bookmark_browser.restricted = None;
                self.bookmark_browser.source_label = match &source {
                    Source::History => "History".into(),
                    Source::Archive => "Archived books".into(),
                    Source::Folder(path, _) if path.as_os_str().is_empty() => "Documents".into(),
                    Source::Folder(path, _) => crate::library::display_filename(
                        &path.to_string_lossy(),
                    ),
                    Source::Books(_) => "Selected books".into(),
                    Source::Collections(_) => "Collections".into(),
                };
                self.bookmark_browser.source = source;
                self.refresh_bookmark_browser();
                self.show(Screen::BookmarkBrowser(Page::Results));
            }
            Command::ToggleDefault(source) => {
                let browser = &mut self.bookmark_browser;
                let previous = browser.default_source.clone();
                browser.default_source = (previous.as_ref() != Some(&source)).then_some(source);
                if let Err(error) = browser.save() {
                    browser.default_source = previous;
                    return Err(error);
                }
            }
            Command::OpenFolder(path, recursive) => {
                let listing = with_storage(|| self.library.browse(&path))?;
                let browser = &mut self.bookmark_browser;
                browser.subfolders = listing.entries.into_iter()
                    .filter(|entry| matches!(entry.kind, BrowseKind::Folder))
                    .map(|entry| entry.path).collect();
                browser.folder = path;
                browser.recursive = recursive;
                self.show(Screen::BookmarkBrowser(Page::Folder));
            }
            Command::ToggleBook(id) => {
                let browser = &mut self.bookmark_browser;
                let chosen = browser.picking.as_mut().unwrap_or(&mut browser.picked_books);
                if !chosen.remove(&id) { chosen.insert(id); }
            }
            Command::ToggleCollection(name) => {
                let chosen = &mut self.bookmark_browser.picked_collections;
                if !chosen.remove(&name) { chosen.insert(name); }
            }
            Command::ToggleAllBooks => {
                let browser = &mut self.bookmark_browser;
                let ids: BTreeSet<_> = if browser.picking.is_some() {
                    browser.books.iter().map(|book| book.id).collect()
                } else {
                    self.library.recents().iter().filter(|book| has_marks(book)).map(|book| book.id).collect()
                };
                let chosen = browser.picking.as_mut().unwrap_or(&mut browser.picked_books);
                *chosen = if *chosen == ids { BTreeSet::new() } else { ids };
            }
            Command::RestrictBooks => {
                let browser = &mut self.bookmark_browser;
                browser.picking = Some(browser.restricted.clone().unwrap_or_else(||
                    browser.books.iter().map(|book| book.id).collect()));
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
            Command::Reverse => {
                self.bookmark_browser.reverse = !self.bookmark_browser.reverse;
                self.refresh_bookmark_browser();
                self.bookmark_browser.save()?;
            }
            Command::ClearFilters => {
                let browser = &mut self.bookmark_browser;
                browser.kind = None;
                browser.style = None;
                browser.query_text.clear();
                browser.query = None;
                browser.restricted = None;
                self.refresh_bookmark_browser();
                self.bookmark_browser.save()?;
            }
            Command::GotoBook(id) => {
                let page = self.bookmark_browser.rows.iter().position(|r| *r == RowKey::Book(id));
                self.show(Screen::BookmarkBrowser(Page::Results));
                self.list_page = page.unwrap_or(0) / super::ROWS_PER_PAGE;
            }
            Command::BookInfo(id) => {
                self.bookmark_browser.book = Some(id);
                self.show(Screen::BookmarkBrowser(Page::Book));
            }
            Command::ViewBook(id) => {
                if self.open_book_id() != Some(id) {
                    let entry = self.locate_tracked(id)?;
                    self.open_book(entry)?;
                } else {
                    self.show(if self.fixed.is_some() { Screen::Fixed } else { Screen::Book });
                }
            }
            Command::Inspect(key) => {
                self.bookmark_browser.focus = Some(key);
                self.bookmark_browser.text_page = 0;
                self.show(Screen::BookmarkBrowser(Page::Detail));
            }
            Command::ViewMark(key) => self.view_bookmark_in_book(key)?,
        }
        Ok(())
    }

    fn refresh_bookmark_browser(&mut self) {
        let browser = &mut self.bookmark_browser;
        browser.rows.clear();
        browser.marks.clear();
        for listed in &mut browser.books {
            listed.count = 0;
            if browser.restricted.as_ref().is_some_and(|ids| !ids.contains(&listed.id)) {
                continue;
            }
            let Some(book) = self.library.state(listed.id) else { continue; };
            let mut marks = Vec::new();
            for locator in (0..book.bookmarks.len()).map(Locator::Bookmark)
                .chain((0..book.highlights.len()).map(Locator::Reflow))
                .chain((0..book.pdf_highlights.len()).map(Locator::Fixed))
            {
                let Some(mark) = get_mark(book, locator, |p| listed.chapters.get(p).copied())
                else { continue };
                if browser.kind.is_some_and(|kind| mark.kind() != kind)
                    || browser.style.is_some_and(|style| mark.style != Some(style))
                    || browser.query.as_ref().is_some_and(|query|
                        query.matches(mark.quote, 1).is_empty() &&
                        mark.note.is_none_or(|note| query.matches(note, 1).is_empty()))
                {
                    continue;
                }
                marks.push((mark.location, MarkKey { book: book.id, locator }));
            }
            marks.sort_by(|(left, _), (right, _)| left.0.cmp(&right.0)
                .then_with(|| left.1.total_cmp(&right.1))
                .then_with(|| left.2.total_cmp(&right.2)));
            if browser.reverse { marks.reverse(); }
            if !marks.is_empty() {
                listed.count = marks.len();
                browser.rows.push(RowKey::Book(listed.id));
                for (_, mark) in marks {
                    browser.rows.push(RowKey::Mark(mark));
                    browser.marks.push(mark);
                }
            }
        }
    }

    fn view_bookmark_in_book(&mut self, key: MarkKey) -> Result<()> {
        if self.open_book_id() != Some(key.book) {
            let entry = self.locate_tracked(key.book)?;
            self.open_book(entry)?;
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
        if self.screen != Screen::PdfPassword && let Some(key) = self.bookmark_browser.after_password.take() {
            self.view_bookmark_in_book(key)?;
        }
        Ok(())
    }

    fn bookmark_book_available(&self, id: u64) -> bool {
        self.bookmark_browser.books.iter().any(|book| book.id == id && !book.archived)
    }
}

fn chapter_order(library: &Library, book: &BookState) -> HashMap<String, usize> {
    if book.highlights.is_empty() { return HashMap::new(); }
    let Ok(path) = library.absolute_path(&book.path) else { return HashMap::new(); };
    let Ok(source) = Book::open(&path) else { return HashMap::new(); };
    book.highlights.iter().filter_map(|mark| {
        let name = &mark.selection.start.chapter;
        source.chapter_index(name).map(|chapter| (name.clone(), chapter))
    }).collect()
}

impl Reader {
    pub(super) fn bookmark_window(&self, page: Page, first: usize, count: usize) -> (usize, Vec<Item>) {
        if page == Page::Results {
            return (self.bookmark_browser.rows.len(), self.bookmark_browser.rows.iter()
                .skip(first).take(count).filter_map(|key| self.bookmark_result_row(*key)).collect());
        }
        let rows = self.bookmark_rows(page);
        (rows.len(), rows.into_iter().skip(first).take(count).collect())
    }

    fn bookmark_result_row(&self, key: RowKey) -> Option<Item> {
        match key {
            RowKey::Book(id) => {
                let book = self.library.state(id)?;
                let listed = self.bookmark_browser.books.iter().find(|book| book.id == id)?;
                let detail = if listed.archived { format!("{} marks · Archived", listed.count) }
                    else { format!("{} marks", listed.count) };
                Some(item(author_title(book), Command::BookInfo(id)).detail(detail))
            }
            RowKey::Mark(key) => {
                let mark = self.bookmark_mark(key)?;
                let title = mark.note.unwrap_or(mark.quote);
                Some(item(excerpt(title), Command::Inspect(key))
                    .detail(format!("{} · {}", mark.kind().label(), mark.location_label)))
            }
        }
    }

    fn bookmark_mark(&self, key: MarkKey) -> Option<Mark<'_>> {
        let state = self.library.state(key.book)?;
        let listed = self.bookmark_browser.books.iter().find(|book| book.id == key.book)?;
        get_mark(state, key.locator, |name| listed.chapters.get(name).copied())
    }

    fn bookmark_rows(&self, page: Page) -> Vec<Item> {
        let browser = &self.bookmark_browser;
        use Command as C;
        match page {
            Page::Sources => vec![
                item("History", C::Use(Source::History)),
                item("Collections", C::Go(Page::Collections)),
                item("Selected books", C::Go(Page::ChooseBooks)),
                item("Archived books", C::Use(Source::Archive)),
                item("Documents", C::Use(Source::Folder(PathBuf::new(), false))),
                item("Documents and subfolders", C::Use(Source::Folder(PathBuf::new(), true))),
                item("Choose a folder", C::OpenFolder(PathBuf::new(), false)),
                item("Choose a folder with subfolders", C::OpenFolder(PathBuf::new(), true)),
                Item::new("Hold a source to make it the default", Action::Heading),
            ],
            Page::Filters => {
                let kind = browser.kind.map_or("All types", Kind::label);
                let style = browser.style.map_or("All styles", HighlightStyle::label);
                let mut rows = vec![
                    item("Clear filters", C::ClearFilters),
                    item("Books with matches", C::Go(Page::Books)),
                    item("Filter by book", C::RestrictBooks),
                    item("Reading order", C::Reverse)
                        .detail(if browser.reverse { "Reverse" } else { "Forward" }),
                    item("All types", C::SetKind(None)).detail(kind),
                ];
                rows.extend([Kind::Bookmark, Kind::Highlight, Kind::Note].into_iter().map(|kind|
                    selection_row(kind.label(), C::SetKind(Some(kind)), browser.kind == Some(kind))));
                rows.push(item("All styles", C::SetStyle(None)).detail(style));
                rows.extend(HighlightStyle::ALL.into_iter().map(|style|
                    selection_row(style.label(), C::SetStyle(Some(style)), browser.style == Some(style))));
                rows
            }
            Page::Books => browser.books.iter().filter(|book| book.count > 0)
                .filter_map(|book| Some(item(author_title(self.library.state(book.id)?),
                    C::GotoBook(book.id)).detail(format!("{} marks", book.count)))).collect(),
            Page::ChooseBooks => {
                let chosen = browser.picking.as_ref().unwrap_or(&browser.picked_books);
                let mut rows = vec![
                    item("Apply book selection", C::ApplyChoices).detail(format!("{} selected", chosen.len())),
                    item("Select or clear all", C::ToggleAllBooks),
                ];
                let mut candidates = self.library.recents().iter().filter(|book| has_marks(book))
                    .filter(|book| browser.picking.is_none() ||
                        browser.books.iter().any(|listed| listed.id == book.id)).collect::<Vec<_>>();
                candidates.sort_by_key(|book| fold_search(&author_title(book)));
                rows.extend(candidates.into_iter().map(|book|
                    selection_row(author_title(book), C::ToggleBook(book.id), chosen.contains(&book.id))));
                rows
            }
            Page::Collections => {
                let mut rows = vec![item("Apply collections", C::ApplyChoices)
                    .detail(format!("{} selected", browser.picked_collections.len()))];
                rows.extend(self.library.collections().iter().map(|collection|
                    selection_row(&collection.name, C::ToggleCollection(collection.name.clone()),
                        browser.picked_collections.contains(&collection.name))));
                rows
            }
            Page::Folder => {
                let mut rows = vec![item(
                    if browser.recursive { "Use this folder and subfolders" } else { "Use this folder" },
                    C::Use(Source::Folder(browser.folder.clone(), browser.recursive)),
                ).detail(crate::library::display_filename(&browser.folder.to_string_lossy()))];
                if let Some(parent) = browser.folder.parent() {
                    rows.push(item("Up", C::OpenFolder(parent.to_owned(), browser.recursive)));
                }
                rows.extend(browser.subfolders.iter().map(|path|
                    item(crate::library::display_filename(&path.file_name().unwrap_or_default().to_string_lossy()),
                        C::OpenFolder(path.clone(), browser.recursive))));
                rows
            }
            Page::Book => {
                let Some(book) = browser.book.and_then(|id| self.library.state(id)) else { return Vec::new(); };
                let available = self.bookmark_book_available(book.id);
                let mut rows = vec![Item::new(author_title(book), Action::Heading)
                    .detail(if available { "Book" } else { "Archived · Source file unavailable" })];
                if available {
                    rows.push(item("Open book", C::ViewBook(book.id)));
                    rows.push(Item::new("Source file", Action::Heading)
                        .detail(crate::library::display_filename(&book.path.to_string_lossy())));
                }
                rows.push(item("Books with matches", C::Go(Page::Books)));
                rows
            }
            Page::Detail | Page::Search | Page::Results => Vec::new(),
        }
    }

    pub(super) fn draw_bookmark_browser_controls(&self, canvas: &mut Canvas, page: Page) -> bool {
        match page {
            Page::Results => {
                canvas.button(SOURCES, "Sources", false, false);
                canvas.button(FILTERS, "Filters", false, false);
                canvas.button(FIND, "Find", false, false);
                false
            }
            Page::Search => {
                canvas.single_line_field(Rect::new(40, 154, WIDTH as i32 - 80, 130), &self.editor,
                    "Bookmark, highlighted text or note", false);
                canvas.button(CASE, "Match case", self.bookmark_browser.case_sensitive, false);
                canvas.keyboard(self.editor.layer, "Find", None);
                true
            }
            Page::Detail => {
                self.draw_bookmark_detail(canvas);
                true
            }
            _ => false,
        }
    }

    fn draw_bookmark_detail(&self, canvas: &mut Canvas) {
        let browser = &self.bookmark_browser;
        let Some(key) = browser.focus else { return; };
        let (Some(book), Some(mark)) = (self.library.state(key.book), self.bookmark_mark(key))
        else { return; };
        let index = browser.marks.iter().position(|item| *item == key).unwrap_or(0);
        let title = format!("{} · {} · {}", book.title, mark.kind().label(), mark.location_label);
        canvas.text(Rect::new(272, 152, WIDTH as i32 - 544, 94), &title, CAPTION.center().lines(2));
        if index > 0 {
            canvas.button(FIRST_MARK, "First", false, false);
            canvas.button(PREV_MARK, "Previous mark", false, false);
        }
        if index + 1 < browser.marks.len() {
            canvas.button(LAST_MARK, "Last", false, false);
            canvas.button(NEXT_MARK, "Next mark", false, false);
        }
        if self.bookmark_book_available(book.id) {
            canvas.button(READ, "View in book", true, false);
        } else {
            canvas.text(READ, "Archived mark", CAPTION.center());
        }
        let text = mark.note.map_or_else(|| mark.quote.to_owned(),
            |note| format!("{}\n\n{}", note, mark.quote));
        let style = BODY.lines(19).break_long_words();
        let pages = canvas.text_pages(TEXT, &text, style);
        browser.text_pages.set(pages.len());
        if let Some(range) = pages.get(browser.text_page.min(pages.len().saturating_sub(1))) {
            canvas.text(TEXT, &text[range.clone()], style);
        }
        if pages.len() > 1 {
            if browser.text_page > 0 { canvas.button(PREV_TEXT, "Previous text", false, false); }
            if browser.text_page + 1 < pages.len() { canvas.button(NEXT_TEXT, "Next text", false, false); }
            canvas.text(Rect::new(450, PREV_TEXT.y + 15, 364, 55),
                &format!("{} of {}", browser.text_page + 1, pages.len()), CAPTION.center());
        }
    }

    pub(super) fn bookmark_browser_input(&mut self, page: Page, input: Input) -> Option<Change> {
        if matches!(input, Input::Tap(x, y) if primitive_ui::back_button(WIDTH as i32).contains(x, y)) {
            self.back_from_bookmark_browser(page);
            return Some(Change::Partial);
        }
        if page == Page::Search {
            return Some(self.bookmark_search_input(input));
        }
        if page == Page::Detail {
            return Some(self.bookmark_detail_input(input));
        }
        if let Input::LongTap(x, y) = input {
            let (count, mut rows) = self.list_window();
            let row = super::list_item_at(self.list_page, count, x, y)? % super::ROWS_PER_PAGE;
            let source = match rows.get_mut(row).map(|item| &item.action) {
                Some(Action::BookmarkBrowser(Command::Use(source))) => Some(source.clone()),
                Some(Action::BookmarkBrowser(Command::ApplyChoices)) if page == Page::Collections => {
                    Some(Source::Collections(self.bookmark_browser.picked_collections.clone()))
                }
                Some(Action::BookmarkBrowser(Command::ApplyChoices)) if page == Page::ChooseBooks
                    && self.bookmark_browser.picking.is_none() => {
                    Some(Source::Books(self.bookmark_browser.picked_books.clone()))
                }
                _ => None,
            }?;
            return Some(self.bookmark_change(Command::ToggleDefault(source)));
        }
        if page == Page::Results && let Input::Tap(x, y) = input {
            let page = if SOURCES.contains(x, y) { Page::Sources }
                else if FILTERS.contains(x, y) { Page::Filters }
                else if FIND.contains(x, y) { Page::Search }
                else { return None; };
            return Some(self.bookmark_change(Command::Go(page)));
        }
        None
    }

    fn bookmark_search_input(&mut self, input: Input) -> Change {
        let Input::Tap(x, y) = input else { return Change::None; };
        if CASE.contains(x, y) {
            self.bookmark_browser.case_sensitive = !self.bookmark_browser.case_sensitive;
            return Change::Partial;
        }
        let layout = Layout::new(WIDTH, HEIGHT, 4, 5);
        let Some(key) = keyboard_key(&layout, &self.editor, "Find", x, y) else { return Change::None; };
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
                let row = self.bookmark_browser.focus.and_then(|key|
                    self.bookmark_browser.rows.iter().position(|value| *value == RowKey::Mark(key)));
                self.show(Screen::BookmarkBrowser(Page::Results));
                self.list_page = row.unwrap_or(0) / super::ROWS_PER_PAGE;
            }
            Page::ChooseBooks if self.bookmark_browser.picking.take().is_some() => {
                self.show(Screen::BookmarkBrowser(Page::Filters));
            }
            Page::ChooseBooks | Page::Collections | Page::Folder => {
                self.show(Screen::BookmarkBrowser(Page::Sources));
            }
            _ => self.show(Screen::BookmarkBrowser(Page::Results)),
        }
    }

    fn bookmark_detail_input(&mut self, input: Input) -> Change {
        let Some(key) = self.bookmark_browser.focus else { return Change::None; };
        let Some(index) = self.bookmark_browser.marks.iter().position(|value| *value == key)
        else { return Change::None; };
        let last = self.bookmark_browser.marks.len().saturating_sub(1);
        let mut target = index;
        match input {
            Input::PageBack => target = index.saturating_sub(1),
            Input::PageForward => target = (index + 1).min(last),
            Input::Swipe { dx, dy, .. } if dx.abs() > dy.abs() => {
                target = if dx < 0.0 { (index + 1).min(last) } else { index.saturating_sub(1) };
            }
            Input::Swipe { dy, .. } => return self.step_bookmark_text(if dy < 0.0 { 1 } else { -1 }),
            Input::Tap(x, y) if READ.contains(x, y) && self.bookmark_book_available(key.book) => {
                return self.bookmark_change(Command::ViewMark(key));
            }
            Input::Tap(x, y) if PREV_MARK.contains(x, y) => target = index.saturating_sub(1),
            Input::Tap(x, y) if NEXT_MARK.contains(x, y) => target = (index + 1).min(last),
            Input::Tap(x, y) if FIRST_MARK.contains(x, y) => target = 0,
            Input::Tap(x, y) if LAST_MARK.contains(x, y) => target = last,
            Input::Tap(x, y) if PREV_TEXT.contains(x, y) => return self.step_bookmark_text(-1),
            Input::Tap(x, y) if NEXT_TEXT.contains(x, y) => return self.step_bookmark_text(1),
            _ => {}
        }
        if target == index { return Change::None; }
        self.bookmark_change(Command::Inspect(self.bookmark_browser.marks[target]))
    }

    fn step_bookmark_text(&mut self, step: isize) -> Change {
        let browser = &mut self.bookmark_browser;
        let previous = browser.text_page;
        browser.text_page = previous.saturating_add_signed(step).min(browser.text_pages.get().saturating_sub(1));
        if previous == browser.text_page { Change::None } else { Change::Partial }
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
    if selected { row.detail("Selected") } else { row }
}
