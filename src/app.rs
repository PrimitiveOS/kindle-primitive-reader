//! The daily reader interface and foreground lifecycle.
use std::collections::HashSet;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use primitive_app::{App, Change, HEIGHT, WIDTH, with_storage};
use primitive_ui::{
    BODY, CAPTION, Canvas, Editor, GrayImage, Input, Layout, Press, Rect, Row, RowAccessory, Rows,
    TextStyle, header, keyboard_key,
};

use crate::annotation_export;
use crate::epub::{Book, Destination};
use crate::fixed::{PdfError, PdfLinkTarget};
use crate::fixed_session::{FixedReading, PdfSelection};
use crate::fixed_text::PdfTextRect;
use crate::koreader_import::{self, ExportEntry, QuoteMatch, QuoteResolver};
use crate::koreader_sidecar::{self, SidecarProgress};
use crate::library::{
    BackupImport, BookPreferences, BookState, Bookmark, DocumentFormat, FixedPreferences,
    Highlight, Library, LibraryBook, MAX_BACKUP_BYTES, PdfHighlight, ReadingPosition,
    ReadingStatus,
};
use crate::ocr_job::{OcrAssets, OcrJob};
use crate::pdf_annotations;
use crate::reading_controls::ReadingControls;
use crate::reading_fonts;
use crate::reading_profiles::{self, Profile};
use crate::reading_stats::ReadingStats;
use crate::search::{MAX_SEARCH_HITS, PdfSearch, PdfSearchHit, Search, SearchHit};
use crate::selection::TextSelection;
use crate::session::{
    BookConfig, DONE, PAGE_HEIGHT, PAGE_REFRESH_GAP, PagePaint, PageTurns, Reading,
};
use crate::text_flow::TextFlow;
use crate::{Engine, Result, Settings, TextAlignment};

const ROW_HEIGHT: i32 = 116;
const LIST_X: i32 = 40;
const LIST_Y: i32 = 154;
const ROWS_PER_PAGE: usize = 11;
const OVERVIEW_PAGES: usize = 4;
const PREV: Rect = Rect::new(40, HEIGHT as i32 - 104, 240, 72);
const NEXT: Rect = Rect::new(WIDTH as i32 - 280, HEIGHT as i32 - 104, 240, 72);
const SAVE_DELAY: Duration = Duration::from_millis(800);
const SEARCH_DELAY: Duration = Duration::from_millis(60);
const SEARCH_REFRESH: Duration = Duration::from_millis(450);
const MARK: Rect = Rect::new(473, PAGE_HEIGHT as i32 + 5, 200, 60);
const NOTE: Rect = Rect::new(686, PAGE_HEIGHT as i32 + 5, 154, 60);
const LOOKUP: Rect = Rect::new(853, PAGE_HEIGHT as i32 + 5, 184, 60);
const CANCEL_MARK: Rect = Rect::new(1050, PAGE_HEIGHT as i32 + 5, 186, 60);
const TOP_BAR_HEIGHT: i32 = 94;
const BOTTOM_BAR_HEIGHT: i32 = 98;
const LIBRARY_CONTROL: Rect = Rect::new(20, 15, 160, 62);
const LOCATION_CONTROL: Rect = Rect::new(570, 15, 245, 62);
const BOOKMARK_CONTROL: Rect = Rect::new(829, 15, 205, 62);
const EXIT_CONTROL: Rect = Rect::new(1048, 15, 196, 62);
const UNLOCK_TOUCH: Rect = Rect::new(832, PAGE_HEIGHT as i32 + 5, 258, 60);
const CANCEL_OCR: Rect = Rect::new(624, PAGE_HEIGHT as i32 + 5, 200, 60);
const FIND_CASE: Rect = Rect::new(LIST_X, LIST_Y + 260, 335, 70);
const OCR_POLL: Duration = Duration::from_millis(120);
const AUTO_CROP: Rect = Rect::new(674, PAGE_HEIGHT as i32 + 5, 182, 60);
const RESET_CROP: Rect = Rect::new(868, PAGE_HEIGHT as i32 + 5, 172, 60);
const CANCEL_CROP: Rect = Rect::new(1052, PAGE_HEIGHT as i32 + 5, 192, 60);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Screen {
    Library,
    LibraryFind,
    OrganizeLibrary,
    BookList,
    BookDetails,
    CollectionName,
    RemoveCollection,
    ReadingControls,
    ReadingData,
    ClearOcr,
    RestoreBackup,
    ImportAnnotations,
    ImportPosition,
    ImportPdfAnnotations,
    ExportAnnotations,
    Book,
    Illustration,
    Fixed,
    FixedTools,
    FixedBookmarks,
    FixedContents,
    FixedCrop,
    ScanColumns,
    ReadFixedText,
    OcrLanguage,
    PdfPassword,
    GoToPage,
    PageOverview,
    BookTools,
    Contents,
    Links,
    Bookmarks,
    Typography,
    ReadingFonts,
    ReadingProfiles,
    ProfileName,
    RemoveProfiles,
    Find,
    Matches,
    Highlights,
    Annotation,
    RemoveHighlight,
    Note,
    EditNote,
    Definition,
}

impl Screen {
    fn title(self) -> &'static str {
        match self {
            Self::Library => "Reader",
            Self::LibraryFind => "Find a document",
            Self::OrganizeLibrary => "Library organization",
            Self::BookList => "Books",
            Self::BookDetails => "Book details",
            Self::CollectionName => "Collection name",
            Self::RemoveCollection => "Remove collection?",
            Self::ReadingControls => "Reading controls",
            Self::ReadingData => "Reading data",
            Self::ClearOcr => "Clear recognized text?",
            Self::RestoreBackup => "Preview restore",
            Self::ImportAnnotations => "KOReader annotations",
            Self::ImportPosition => "KOReader reading position",
            Self::ImportPdfAnnotations => "PDF annotations",
            Self::ExportAnnotations => "Export annotations",
            Self::Book
            | Self::Fixed
            | Self::BookTools
            | Self::FixedTools
            | Self::FixedCrop
            | Self::Illustration => "",
            Self::GoToPage => "Go to page",
            Self::PageOverview => "Pages",
            Self::ReadFixedText => "",
            Self::OcrLanguage => "Recognize text",
            Self::PdfPassword => "Unlock PDF",
            Self::ScanColumns => "",
            Self::Contents | Self::FixedContents => "Contents",
            Self::Links => "Links on this page",
            Self::Bookmarks | Self::FixedBookmarks => "Bookmarks",
            Self::Typography => "Text settings",
            Self::ReadingFonts => "Reading font",
            Self::ReadingProfiles => "Reading profiles",
            Self::ProfileName => "Save reading profile",
            Self::RemoveProfiles => "Remove a reading profile",
            Self::Find => "Find in book",
            Self::Matches => "Search results",
            Self::Highlights => "Highlights and notes",
            Self::Annotation => "Highlight",
            Self::RemoveHighlight => "Remove highlight?",
            Self::Note => "Add note",
            Self::EditNote => "Edit note",
            Self::Definition => "Dictionary",
        }
    }
}

enum Action {
    Open(LibraryBook),
    OpenTracked(u64),
    Continue,
    ClearLibraryFilter,
    ListBooks(BookList),
    OrganizeCurrent,
    Favorite(bool),
    SetStatus(ReadingStatus),
    CollectionMembership(String, bool),
    NameCollection(Option<String>),
    RemoveCollection,
    SetReadingControls(ReadingControls),
    LockTouch,
    ExportBackup,
    ClearOcr,
    PreviewBackup,
    MergeBackup,
    PreviewKoReader,
    ApplyKoReader,
    PreviewKoReaderPosition,
    ApplyKoReaderPosition,
    PreviewPdfAnnotations,
    ApplyPdfAnnotations,
    ExportAnnotatedPdf,
    Show(Screen),
    Jump(Destination),
    Bookmark(usize),
    AddBookmark,
    ReturnToLocation,
    Text(Settings),
    ShowFonts,
    SetFont(Option<String>),
    ApplyProfile(usize),
    RemoveProfile(usize),
    ResetText,
    ReadingMode(bool),
    SearchHit(usize),
    PdfSearchHit(usize),
    PdfHighlight(usize),
    Highlight(usize),
    ManageHighlight(usize),
    EditNote,
    RemoveHighlight,
    ExportHighlights(annotation_export::Format),
    FitPage(bool),
    FitHeight,
    FixedScroll(bool),
    ComicSpread(bool),
    ScanContrast(bool),
    ScanColumns,
    Overview,
    FixedPage(usize),
    Heading,
    RotatePage,
    ReversePageDirection,
    CropPage,
    RecognizeText,
    RecognizeLanguage(String),
    ReadFixedText,
}

enum BookList {
    Favorites,
    Status(ReadingStatus),
    Collection(String),
}

struct BookDetails {
    entry: LibraryBook,
    id: u64,
    back: Screen,
}

struct PageOverview {
    first: usize,
    tiles: Vec<Option<GrayImage>>,
}

struct ScanReading {
    original: FixedPreferences,
    columns: [PdfTextRect; 2],
    index: usize,
    turns: PageTurns,
}

struct Item {
    title: String,
    detail: Option<String>,
    action: Action,
}

impl Item {
    fn new(title: impl Into<String>, action: Action) -> Self {
        Self {
            title: title.into(),
            detail: None,
            action,
        }
    }

    fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

struct ActiveBook {
    entry: LibraryBook,
    id: u64,
    reading: Reading,
}

struct ActiveFixed {
    entry: LibraryBook,
    id: u64,
    reading: FixedReading,
    password_supplied: bool,
}

struct ActiveOcr {
    page: usize,
    language: String,
    job: OcrJob,
    deadline: Instant,
}

struct AnnotationImport {
    entries: Vec<ExportEntry>,
    next: usize,
    resolver: Option<QuoteResolver>,
    matched: Vec<Highlight>,
    unresolved: usize,
    deadline: Option<Instant>,
    refresh: Instant,
}

pub struct Reader {
    library: Library,
    library_query: String,
    book_list: Option<BookList>,
    book_details: Option<BookDetails>,
    overview: Option<PageOverview>,
    scan_columns: Option<ScanReading>,
    rename_collection: Option<String>,
    controls_path: PathBuf,
    controls: ReadingControls,
    touch_locked: bool,
    stats: ReadingStats,
    stats_path: PathBuf,
    stats_awake: bool,
    backup_import: Option<BackupImport>,
    annotation_import: Option<AnnotationImport>,
    position_import: Option<SidecarProgress>,
    pdf_annotation_import: Vec<PdfHighlight>,
    import_path: PathBuf,
    fonts: Vec<PathBuf>,
    fonts_directory: PathBuf,
    available_fonts: Vec<String>,
    profiles: Vec<Profile>,
    profiles_path: PathBuf,
    active: Option<ActiveBook>,
    illustration: Option<FixedReading>,
    fixed: Option<ActiveFixed>,
    ocr_assets: Option<OcrAssets>,
    ocr_root: PathBuf,
    ocr: Option<ActiveOcr>,
    text_flow: Option<TextFlow>,
    locked: Option<LibraryBook>,
    crop_dialog: Option<(FixedPreferences, Option<(f32, f32)>)>,
    screen: Screen,
    list_page: usize,
    back: Vec<ReadingPosition>,
    notice: Option<String>,
    dirty: bool,
    save_at: Option<Instant>,
    editor: Editor,
    search: Option<Search>,
    find_match_case: bool,
    pdf_search: Option<PdfSearch>,
    pdf_hits: Vec<PdfSearchHit>,
    pdf_result_region: Option<(usize, PdfTextRect)>,
    pdf_selection: Option<PdfSelection>,
    hits: Vec<SearchHit>,
    result_selection: Option<TextSelection>,
    search_at: Option<Instant>,
    next_search_refresh: Instant,
    selection: Option<TextSelection>,
    highlight_index: usize,
    exports: PathBuf,
    dictionaries: PathBuf,
    definitions: Vec<(String, String)>,
}

impl Reader {
    pub fn open(documents: &Path, state: &Path, fonts: Vec<PathBuf>) -> io::Result<Self> {
        let library = with_storage(|| {
            let mut library = Library::open(documents, state)?;
            library.scan()?;
            Ok(library)
        })?;
        let data = state.parent().unwrap_or(documents);
        let controls_path = data.join("reading-controls.json");
        let controls = ReadingControls::load(&controls_path);
        let stats_path = data.join("reading-stats.json");
        let stats = ReadingStats::load(&stats_path);
        let profiles_path = data.join("reading-profiles.json");
        let profiles = reading_profiles::load(&profiles_path);
        let ocr_assets = with_storage(|| Ok(data.parent().and_then(OcrAssets::discover)))?;
        Ok(Self {
            library,
            library_query: String::new(),
            book_list: None,
            book_details: None,
            overview: None,
            scan_columns: None,
            rename_collection: None,
            controls_path,
            controls,
            touch_locked: false,
            stats,
            stats_path,
            stats_awake: true,
            backup_import: None,
            annotation_import: None,
            position_import: None,
            pdf_annotation_import: Vec::new(),
            import_path: state
                .parent()
                .unwrap_or(documents)
                .join("reader-import.json"),
            fonts,
            fonts_directory: data.join("fonts"),
            available_fonts: Vec::new(),
            profiles,
            profiles_path,
            active: None,
            illustration: None,
            fixed: None,
            ocr_assets,
            ocr_root: data.join("ocr-cache"),
            ocr: None,
            text_flow: None,
            locked: None,
            crop_dialog: None,
            screen: Screen::Library,
            list_page: 0,
            back: Vec::new(),
            notice: None,
            dirty: false,
            save_at: None,
            editor: Editor::new(64),
            search: None,
            find_match_case: false,
            pdf_search: None,
            pdf_hits: Vec::new(),
            pdf_result_region: None,
            pdf_selection: None,
            hits: Vec::new(),
            result_selection: None,
            search_at: None,
            next_search_refresh: Instant::now(),
            selection: None,
            highlight_index: 0,
            exports: state.parent().unwrap_or(documents).join("exports"),
            dictionaries: state.parent().unwrap_or(documents).join("dictionaries"),
            definitions: Vec::new(),
        })
    }

    pub fn open_path(&mut self, path: &Path) -> Result<()> {
        let entry = with_storage(|| self.library.resolve(path))?;
        self.open_book(entry)
    }

    fn open_book(&mut self, entry: LibraryBook) -> Result<()> {
        self.open_with_password(entry, None)
    }

    fn open_with_password(&mut self, mut entry: LibraryBook, password: Option<&str>) -> Result<()> {
        let (absolute, id) = with_storage(|| {
            Ok((
                self.library.absolute_path(&entry.path)?,
                self.library.identify(&entry)?,
            ))
        })?;
        let mut settings = device_settings();
        let mut scroll = false;
        let mut fixed_preferences = FixedPreferences::default();
        let position = self.library.state(id).and_then(|state| {
            settings = state.preferences.apply_to(settings);
            scroll = state.preferences.scroll;
            fixed_preferences = state.fixed_preferences;
            state.position.clone()
        });
        let fixed = FixedReading::open(
            entry.format,
            &absolute,
            position.as_ref().map_or(0, |saved| saved.page),
            &self.fonts,
            fixed_preferences,
            password,
        );
        let fixed = match fixed {
            Err(error)
                if error.downcast_ref::<PdfError>() == Some(&PdfError::PasswordProtected) =>
            {
                self.locked = Some(entry);
                self.show(Screen::PdfPassword);
                self.editor = Editor::new(64);
                if password.is_some() {
                    self.notice = Some("Password rejected or encryption unsupported".into());
                }
                return Ok(());
            }
            result => result?,
        };
        self.locked = None;
        self.touch_locked = false;
        self.ocr = None;
        self.hits.clear();
        self.pdf_hits.clear();
        self.result_selection = None;
        if let Some(mut reading) = fixed {
            reading.set_ocr_path(self.ocr_root.join(format!("{id:016x}")));
            if let Some(title) = reading.title() {
                entry.title = title;
            }
            entry.author = reading.author();
            self.active = None;
            self.fixed = Some(ActiveFixed {
                entry,
                id,
                reading,
                password_supplied: password.is_some_and(|value| !value.is_empty()),
            });
            self.pdf_result_region = None;
            self.pdf_selection = None;
            self.back.clear();
            self.selection = None;
            self.show(Screen::Fixed);
            self.record_position();
            if let Some(fixed) = &self.fixed {
                self.library.cache_author(id, fixed.entry.author.as_deref());
            }
            return Ok(());
        }
        let custom_font = self
            .library
            .state(id)
            .and_then(|state| state.preferences.font_file.as_deref());
        let fonts = self
            .fonts_for(custom_font)
            .unwrap_or_else(|_| self.fonts.clone());
        let config = BookConfig {
            engine: Engine::Parley,
            book: absolute,
            fonts,
            settings,
            scroll,
        };
        let reading = if let Some(position) = position {
            Reading::restore(
                config.clone(),
                position.chapter,
                position.page,
                position.anchor.as_ref(),
            )
            .or_else(|_| Reading::open(config))?
        } else {
            Reading::open(config)?
        };
        entry.author.clone_from(&reading.probe.author);
        self.active = Some(ActiveBook { entry, id, reading });
        self.fixed = None;
        self.back.clear();
        self.selection = None;
        self.show(Screen::Book);
        self.record_position();
        if let Some(active) = &self.active {
            self.library
                .cache_author(active.id, active.reading.probe.author.as_deref());
        }
        Ok(())
    }

    fn show(&mut self, screen: Screen) {
        if screen != Screen::Illustration {
            self.illustration = None;
        }
        if screen != Screen::ScanColumns {
            self.scan_columns = None;
        }
        if screen != Screen::ReadFixedText {
            self.text_flow = None;
        }
        if screen != Screen::ImportAnnotations {
            self.annotation_import = None;
        }
        if screen != Screen::ImportPosition {
            self.position_import = None;
        }
        if screen != Screen::ImportPdfAnnotations {
            self.pdf_annotation_import.clear();
        }
        if screen != Screen::BookDetails {
            self.book_details = None;
        }
        if screen != Screen::PageOverview {
            self.overview = None;
        }
        if screen == Screen::Find && self.screen != Screen::Matches {
            self.editor = Editor::new(64);
        }
        if screen == Screen::LibraryFind {
            self.editor = Editor::new(96);
            self.editor.text = self.library_query.chars().take(96).collect();
            self.editor.caret = self.editor.text.len();
        }
        if screen == Screen::GoToPage {
            self.editor = Editor::new(if self.fixed.is_some() { 96 } else { 11 });
        }
        if screen == Screen::ProfileName {
            self.editor = Editor::new(reading_profiles::MAX_NAME);
        }
        if self.screen == Screen::Fixed
            && screen != Screen::Fixed
            && let Some(fixed) = &mut self.fixed
        {
            fixed.reading.pause();
        }
        if self.screen == Screen::Book
            && screen != Screen::Book
            && let Some(active) = &mut self.active
        {
            active.reading.pause_turns();
        }
        if screen != Screen::Matches {
            self.search = None;
            self.pdf_search = None;
            self.search_at = None;
        }
        self.screen = screen;
        self.update_stats_activity(Instant::now());
        self.list_page = 0;
        self.notice = None;
    }

    fn record_position(&mut self) {
        if let Some(fixed) = &self.fixed {
            let saved = self
                .library
                .remember(&fixed.entry, fixed.id, &fixed.entry.title);
            saved.position = Some(fixed_position(&fixed.reading));
            saved.fixed_preferences = fixed.reading.preferences();
            self.mark_dirty();
            return;
        }
        let Some(active) = &self.active else { return };
        let title = &active.reading.probe.title;
        let saved = self.library.remember(&active.entry, active.id, title);
        saved.position = Some(position(&active.reading));
        self.mark_dirty();
    }

    fn mark_dirty(&mut self) {
        self.dirty = true;
        self.save_at = Some(Instant::now() + SAVE_DELAY);
    }

    fn update_stats_activity(&mut self, now: Instant) {
        let book = match self.screen {
            Screen::Book | Screen::Illustration if self.stats_awake => {
                self.active.as_ref().map(|book| book.id)
            }
            Screen::Fixed | Screen::ReadFixedText | Screen::ScanColumns if self.stats_awake => {
                self.fixed.as_ref().map(|book| book.id)
            }
            _ => None,
        };
        self.stats.set_active(book, now);
    }

    fn save_library(&mut self) -> io::Result<()> {
        if self.dirty {
            self.library.persist()?;
            self.dirty = false;
        }
        self.save_at = None;
        Ok(())
    }

    fn items(&self) -> Vec<Item> {
        if self.screen == Screen::Library {
            let mut items = Vec::new();
            if let Some(recent) = self.library.recently_read().next() {
                items.push(Item::new("Continue reading", Action::Continue).detail(&recent.title));
            }
            let find = Item::new("Find a document", Action::Show(Screen::LibraryFind));
            if self.library_query.is_empty() {
                items.push(find);
            } else {
                items.push(find.detail(&self.library_query));
                items.push(Item::new(
                    "Clear library search",
                    Action::ClearLibraryFilter,
                ));
            }
            items.push(Item::new(
                "Reading controls",
                Action::Show(Screen::ReadingControls),
            ));
            items.push(Item::new("Reading data", Action::Show(Screen::ReadingData)));
            items.push(Item::new(
                "Organize library",
                Action::Show(Screen::OrganizeLibrary),
            ));
            items.extend(
                self.library
                    .matching(&self.library_query)
                    .cloned()
                    .map(|entry| {
                        let title = entry.title.clone();
                        let author = entry.author.clone();
                        let mut item = Item::new(title, Action::Open(entry));
                        item.detail = author;
                        item
                    }),
            );
            return items;
        }
        if let Some(items) = self.organization_items() {
            return items;
        }
        if self.screen == Screen::ReadingControls {
            let current = self.controls;
            let mut items = vec![
                Item::new(
                    "Night pages",
                    Action::SetReadingControls(ReadingControls {
                        night_pages: !current.night_pages,
                        ..current
                    }),
                )
                .detail(if current.night_pages {
                    "Light text on dark pages"
                } else {
                    "Dark text on light pages"
                }),
                Item::new(
                    "Page turn tap zones",
                    Action::SetReadingControls(ReadingControls {
                        left_touch: !current.left_touch,
                        ..current
                    }),
                )
                .detail(if current.left_touch {
                    "Mirrored touch zones"
                } else {
                    "Standard touch zones"
                }),
                Item::new(
                    "Reading toolbar size",
                    Action::SetReadingControls(ReadingControls {
                        large_toolbar: !current.large_toolbar,
                        ..current
                    }),
                )
                .detail(if current.large_toolbar {
                    "Large labels and controls"
                } else {
                    "Compact labels and controls"
                }),
            ];
            if self.active.is_some() || self.fixed.is_some() {
                items.push(Item::new("Book details", Action::OrganizeCurrent));
                items.push(
                    Item::new("Lock page touch", Action::LockTouch)
                        .detail("Physical page buttons, Unlock touch and Done stay available"),
                );
                items.push(
                    Item::new("Import KOReader position", Action::PreviewKoReaderPosition)
                        .detail("Copy metadata to reader/data/koreader-progress.lua"),
                );
            }
            return items;
        }
        if self.screen == Screen::ReadingData {
            return vec![
                Item::new("Back up reading data", Action::ExportBackup)
                    .detail("Saves progress, bookmarks, highlights, and preferences to exports"),
                Item::new("Restore a backup", Action::PreviewBackup)
                    .detail("Copy a backup to reader/data/reader-import.json"),
                Item::new("Clear recognized text", Action::Show(Screen::ClearOcr))
                    .detail("Clear saved OCR text from document pages"),
            ];
        }
        if self.screen == Screen::ClearOcr {
            return vec![
                Item::new("Clear OCR cache", Action::ClearOcr),
                Item::new("Keep OCR cache", Action::Show(Screen::ReadingData)),
            ];
        }
        if self.screen == Screen::RestoreBackup
            && let Some(import) = &self.backup_import
        {
            let summary = import.summary();
            let highlights = summary.highlights_added + summary.pdf_highlights_added;
            return vec![
                Item::new("Apply backup", Action::MergeBackup).detail(format!(
                    "{} new books and {} known books",
                    summary.new_books, summary.existing_books
                )),
                Item::new(
                    format!(
                        "{} bookmarks and {highlights} highlights",
                        summary.bookmarks_added
                    ),
                    Action::Heading,
                )
                .detail(format!(
                    "{} notes and {} note conflicts",
                    summary.notes_added, summary.note_conflicts
                )),
                Item::new(
                    format!(
                        "{} duplicates and {} skipped",
                        summary.duplicates, summary.skipped
                    ),
                    Action::Heading,
                ),
                Item::new(
                    format!(
                        "{} collections and {} memberships",
                        summary.collections_added, summary.memberships_added
                    ),
                    Action::Heading,
                ),
            ];
        }
        if self.screen == Screen::ImportPosition {
            return self
                .position_import
                .as_ref()
                .map_or_else(Vec::new, |import| {
                    if let Some(fixed) = &self.fixed {
                        return vec![
                            Item::new("Apply reading position", Action::ApplyKoReaderPosition)
                                .detail(format!(
                                    "Page {} → {} of {}",
                                    fixed.reading.page() + 1,
                                    import.last_page.unwrap_or(0),
                                    fixed.reading.count()
                                )),
                            Item::new("Edition check", Action::Heading)
                                .detail("KOReader sampled fingerprint and page count match"),
                        ];
                    }
                    let mut rows = vec![
                        Item::new("Position unresolved", Action::Heading)
                            .detail("Reader cannot map this reflowable position to a passage"),
                    ];
                    if let Some(percent) = import.percent_finished {
                        rows.push(Item::new(
                            format!("KOReader progress {:.1}%", percent * 100.0),
                            Action::Heading,
                        ));
                    }
                    rows
                });
        }
        if self.screen == Screen::ImportPdfAnnotations {
            let count = self.pdf_annotation_import.len();
            let mut rows = vec![
                Item::new("Import annotations", Action::ApplyPdfAnnotations)
                    .detail(format!("{count} new highlights from this PDF")),
            ];
            rows.extend(self.pdf_annotation_import.iter().map(|mark| {
                Item::new(&mark.text, Action::Heading)
                    .detail(mark.note.as_deref().unwrap_or("PDF highlight"))
            }));
            return rows;
        }
        if self.screen == Screen::ExportAnnotations {
            return annotation_export::Format::ALL
                .into_iter()
                .map(|format| Item::new(format.label(), Action::ExportHighlights(format)))
                .collect();
        }
        if self.screen == Screen::ImportAnnotations
            && let Some(import) = &self.annotation_import
        {
            let checked = format!("{} of {} quotes checked", import.next, import.entries.len());
            let matched = format!(
                "{} matched and {} unresolved",
                import.matched.len(),
                import.unresolved
            );
            if import.deadline.is_some() {
                return vec![Item::new(checked, Action::Heading).detail(matched)];
            }
            return vec![
                Item::new("Add matched annotations", Action::ApplyKoReader).detail(matched),
                Item::new(checked, Action::Heading),
            ];
        }
        if self.screen == Screen::Annotation {
            let passage = if self.fixed.is_some() {
                Action::PdfHighlight(self.highlight_index)
            } else {
                Action::Highlight(self.highlight_index)
            };
            return vec![
                Item::new("Read passage", passage),
                Item::new("Edit note", Action::EditNote),
                Item::new("Remove highlight", Action::Show(Screen::RemoveHighlight)),
            ];
        }
        if self.screen == Screen::RemoveHighlight {
            return vec![
                Item::new("Remove highlight and note", Action::RemoveHighlight),
                Item::new("Keep highlight", Action::Show(Screen::Annotation)),
            ];
        }
        if let Some(fixed) = &self.fixed {
            return match self.screen {
                Screen::FixedBookmarks => self.bookmark_items(fixed.id),
                Screen::Highlights => {
                    let state = self.library.state(fixed.id);
                    let mut items = state.map_or_else(Vec::new, |state| {
                        annotation_items(
                            state
                                .pdf_highlights
                                .iter()
                                .map(|mark| (mark.text.as_str(), mark.note.as_deref())),
                        )
                    });
                    if fixed.entry.format == DocumentFormat::Pdf && !fixed.password_supplied {
                        items.push(Item::new(
                            "Read embedded annotations",
                            Action::PreviewPdfAnnotations,
                        ));
                        if state.is_some_and(|state| !state.pdf_highlights.is_empty()) {
                            items.push(
                                Item::new("Export annotated PDF", Action::ExportAnnotatedPdf)
                                    .detail("Saves a separate PDF in Reader exports"),
                            );
                        }
                    }
                    items
                }
                Screen::FixedContents => fixed
                    .reading
                    .outline()
                    .iter()
                    .map(|entry| {
                        Item::new(
                            format!("{}{}", "  ".repeat(entry.depth.min(6)), entry.title),
                            entry.page.map_or(Action::Heading, Action::FixedPage),
                        )
                    })
                    .collect(),
                Screen::Matches => self
                    .pdf_hits
                    .iter()
                    .enumerate()
                    .map(|(index, hit)| {
                        Item::new(&hit.snippet, Action::PdfSearchHit(index))
                            .detail(fixed_page_label(&fixed.reading, hit.page))
                    })
                    .collect(),
                Screen::OcrLanguage => self.ocr_assets.as_ref().map_or_else(Vec::new, |assets| {
                    assets
                        .languages()
                        .iter()
                        .map(|language| {
                            Item::new(
                                ocr_language_name(language),
                                Action::RecognizeLanguage(language.clone()),
                            )
                        })
                        .collect()
                }),
                _ => Vec::new(),
            };
        }
        let Some(active) = &self.active else {
            return Vec::new();
        };
        let reading = &active.reading;
        match self.screen {
            Screen::Contents if reading.probe.toc.is_empty() => reading
                .probe
                .chapter_entries()
                .iter()
                .map(|chapter| {
                    Item::new(
                        &chapter.label,
                        Action::Jump(Destination {
                            path: chapter.href.clone(),
                            fragment: None,
                        }),
                    )
                })
                .collect(),
            Screen::Contents => reading
                .probe
                .toc
                .iter()
                .map(|entry| {
                    Item::new(
                        format!("{}{}", "  ".repeat(entry.depth.min(4)), entry.label),
                        entry
                            .destination
                            .clone()
                            .map_or(Action::Heading, Action::Jump),
                    )
                })
                .collect(),
            Screen::Links => reading
                .links()
                .into_iter()
                .map(|(label, target)| Item::new(label, Action::Jump(target)))
                .collect(),
            Screen::Bookmarks => self.bookmark_items(active.id),
            Screen::Typography => {
                let mut items = text_items(reading.config.settings, reading.config.scroll);
                let current = self
                    .library
                    .state(active.id)
                    .and_then(|book| book.preferences.font_file.as_deref());
                items.insert(
                    0,
                    Item::new("Reading font", Action::ShowFonts)
                        .detail(current.unwrap_or("Device fonts")),
                );
                items.insert(
                    1,
                    Item::new("Reading profiles", Action::Show(Screen::ReadingProfiles)),
                );
                items
            }
            Screen::ReadingFonts => {
                let mut items = vec![Item::new("Device fonts", Action::SetFont(None))];
                items.extend(
                    self.available_fonts.iter().map(|filename| {
                        Item::new(filename, Action::SetFont(Some(filename.clone())))
                    }),
                );
                items.push(
                    Item::new("Add more fonts", Action::Heading)
                        .detail("Copy TTF or OTF files to reader/data/fonts"),
                );
                items
            }
            Screen::ReadingProfiles => {
                let mut items = vec![Item::new(
                    "Save current text settings",
                    Action::Show(Screen::ProfileName),
                )];
                items.extend(self.profiles.iter().enumerate().map(|(index, profile)| {
                    Item::new(&profile.name, Action::ApplyProfile(index))
                        .detail("Apply to this book")
                }));
                if !self.profiles.is_empty() {
                    items.push(Item::new(
                        "Remove a profile",
                        Action::Show(Screen::RemoveProfiles),
                    ));
                }
                items
            }
            Screen::RemoveProfiles => self
                .profiles
                .iter()
                .enumerate()
                .map(|(index, profile)| {
                    Item::new(&profile.name, Action::RemoveProfile(index))
                        .detail("Remove this profile")
                })
                .collect(),
            Screen::Matches => self
                .hits
                .iter()
                .enumerate()
                .map(|(index, hit)| {
                    Item::new(&hit.snippet, Action::SearchHit(index)).detail(&hit.chapter_title)
                })
                .collect(),
            Screen::Highlights => {
                let mut items = vec![
                    Item::new("Import KOReader annotations", Action::PreviewKoReader)
                        .detail("Copy a JSON export to reader/data/koreader-import.json"),
                ];
                if let Some(state) = self.library.state(active.id) {
                    items.extend(annotation_items(
                        state
                            .highlights
                            .iter()
                            .map(|mark| (mark.selection.text.as_str(), mark.note.as_deref())),
                    ));
                }
                items
            }
            _ => Vec::new(),
        }
    }

    fn bookmark_items(&self, id: u64) -> Vec<Item> {
        self.library
            .state(id)
            .into_iter()
            .flat_map(|state| state.bookmarks.iter().enumerate())
            .map(|(index, bookmark)| Item::new(&bookmark.label, Action::Bookmark(index)))
            .collect()
    }

    fn organization_items(&self) -> Option<Vec<Item>> {
        let mut items = Vec::new();
        match self.screen {
            Screen::OrganizeLibrary => {
                items.push(Item::new(
                    "Favorites",
                    Action::ListBooks(BookList::Favorites),
                ));
                items.extend(ReadingStatus::ALL.into_iter().map(|status| {
                    Item::new(status.label(), Action::ListBooks(BookList::Status(status)))
                }));
                items.push(Item::new("New collection", Action::NameCollection(None)));
                items.extend(self.library.collections().iter().map(|collection| {
                    Item::new(
                        &collection.name,
                        Action::ListBooks(BookList::Collection(collection.name.clone())),
                    )
                    .detail(format!("{} books", collection.book_ids().len()))
                }));
                items.push(Item::new("Hold a book to organize it", Action::Heading));
            }
            Screen::BookList => match self.book_list.as_ref()? {
                BookList::Favorites => {
                    items.extend(self.library.favorites().map(tracked_book_item))
                }
                BookList::Status(status) => {
                    items.extend(
                        self.library
                            .recents()
                            .iter()
                            .filter(|book| book.status() == *status)
                            .map(tracked_book_item),
                    );
                    if *status == ReadingStatus::Unread {
                        let tracked: HashSet<_> = self
                            .library
                            .recents()
                            .iter()
                            .map(|book| &book.path)
                            .collect();
                        items.extend(
                            self.library
                                .entries()
                                .iter()
                                .filter(|entry| !tracked.contains(&entry.path))
                                .cloned()
                                .map(|entry| Item::new(entry.title.clone(), Action::Open(entry))),
                        );
                    }
                }
                BookList::Collection(name) => {
                    let collection = self.library.collection(name)?;
                    items.extend(
                        collection
                            .book_ids()
                            .iter()
                            .filter_map(|id| self.library.state(*id))
                            .map(tracked_book_item),
                    );
                    items.push(Item::new(
                        "Rename collection",
                        Action::NameCollection(Some(name.clone())),
                    ));
                    items.push(Item::new(
                        "Remove collection",
                        Action::Show(Screen::RemoveCollection),
                    ));
                }
            },
            Screen::BookDetails => {
                let book = self.book_details.as_ref()?;
                items.push(
                    Item::new(&book.entry.title, Action::Heading)
                        .detail(book.entry.path.to_string_lossy()),
                );
                let seconds = self.stats.seconds_read(book.id, Instant::now());
                items
                    .push(Item::new("Reading time", Action::Heading).detail(reading_time(seconds)));
                let state = self.library.state(book.id);
                let favorite = state.is_some_and(|state| state.favorite);
                items.push(Item::new(
                    if favorite {
                        "Remove from favorites"
                    } else {
                        "Add to favorites"
                    },
                    Action::Favorite(!favorite),
                ));
                let current = state.map_or(ReadingStatus::Unread, |state| state.status());
                items.extend(ReadingStatus::ALL.into_iter().map(|status| {
                    if status == current {
                        Item::new(status.label(), Action::Heading).detail("Current reading status")
                    } else {
                        Item::new(
                            format!("Mark as {}", status.label().to_lowercase()),
                            Action::SetStatus(status),
                        )
                    }
                }));
                items.extend(self.library.collections().iter().map(|collection| {
                    let member = collection.contains(book.id);
                    Item::new(
                        &collection.name,
                        Action::CollectionMembership(collection.name.clone(), !member),
                    )
                    .detail(if member {
                        "Remove from collection"
                    } else {
                        "Add to collection"
                    })
                }));
            }
            Screen::RemoveCollection => {
                let BookList::Collection(name) = self.book_list.as_ref()? else {
                    return None;
                };
                items.push(Item::new("Remove collection", Action::RemoveCollection).detail(name));
                items.push(Item::new("Keep collection", Action::Show(Screen::BookList)));
            }
            _ => return None,
        }
        Some(items)
    }

    fn reading_tools(&self) -> Vec<Item> {
        if let Some(fixed) = &self.fixed {
            let mut tools = vec![
                Item::new("Go to", Action::Show(Screen::GoToPage)),
                Item::new("Pages", Action::Overview),
                Item::new("Bookmarks", Action::Show(Screen::FixedBookmarks)),
                Item::new("Rotate", Action::RotatePage),
            ];
            if !fixed.reading.is_spread() {
                tools.push(Item::new("Fit page", Action::FitPage(false)));
                tools.push(Item::new("Fit width", Action::FitPage(true)));
                tools.push(Item::new("Fit height", Action::FitHeight));
            }
            if fixed.reading.supports_spreads() {
                let spread = fixed.reading.is_spread();
                tools.push(Item::new(
                    if spread { "One page" } else { "Two pages" },
                    Action::ComicSpread(!spread),
                ));
            }
            if fixed.reading.supports_scan_contrast() {
                let higher = fixed.reading.scan_contrast();
                tools.push(Item::new(
                    if higher { "Contrast −" } else { "Contrast +" },
                    Action::ScanContrast(!higher),
                ));
            }
            if !fixed.reading.outline().is_empty() {
                tools.insert(
                    0,
                    Item::new("Contents", Action::Show(Screen::FixedContents)),
                );
            }
            if fixed.reading.supports_continuous() {
                let continuous = fixed.reading.is_continuous();
                tools.push(Item::new(
                    if continuous { "Page turns" } else { "Scroll" },
                    Action::FixedScroll(!continuous),
                ));
            }
            if fixed.reading.supports_text() {
                tools.push(Item::new("Find", Action::Show(Screen::Find)));
                tools.push(Item::new("Read text", Action::ReadFixedText));
                tools.push(Item::new("Highlights", Action::Show(Screen::Highlights)));
                if !self.pdf_hits.is_empty() {
                    tools.push(Item::new("Matches", Action::Show(Screen::Matches)));
                }
            }
            if fixed.reading.supports_rtl() {
                let label = if fixed.reading.preferences().right_to_left {
                    "Left to right"
                } else {
                    "Right to left"
                };
                tools.push(Item::new(label, Action::ReversePageDirection));
            }
            if fixed.reading.supports_crop() {
                tools.push(Item::new("Crop", Action::CropPage));
                tools.push(Item::new("Columns", Action::ScanColumns));
                if self.ocr_assets.is_some() && self.ocr.is_none() {
                    tools.push(Item::new("OCR", Action::RecognizeText));
                }
            }
            tools.push(Item::new("Controls", Action::Show(Screen::ReadingControls)));
            return tools;
        }
        let mut tools = vec![
            Item::new("Contents", Action::Show(Screen::Contents)),
            Item::new("Bookmarks", Action::Show(Screen::Bookmarks)),
            Item::new("Highlights", Action::Show(Screen::Highlights)),
            Item::new("Find", Action::Show(Screen::Find)),
            Item::new("Text", Action::Show(Screen::Typography)),
            Item::new("Go to", Action::Show(Screen::GoToPage)),
            Item::new("Pages", Action::Overview),
        ];
        if !self.hits.is_empty() {
            tools.push(Item::new("Matches", Action::Show(Screen::Matches)));
        }
        if let Some(active) = &self.active
            && !active.reading.links().is_empty()
        {
            tools.push(Item::new("Links", Action::Show(Screen::Links)));
        }
        tools.push(Item::new("Controls", Action::Show(Screen::ReadingControls)));
        tools
    }

    fn draw_reading_tools(&self, canvas: &mut Canvas) {
        let width = WIDTH as i32;
        let tools = self.reading_tools();
        let toolbar = ToolbarLayout::new(tools.len(), self.controls.large_toolbar);
        canvas.fill(Rect::new(0, 0, width, TOP_BAR_HEIGHT), 255);
        canvas.fill(Rect::new(0, TOP_BAR_HEIGHT - 1, width, 1), 166);
        canvas.fill(
            Rect::new(0, toolbar.top, width, HEIGHT as i32 - toolbar.top),
            255,
        );
        canvas.fill(Rect::new(0, toolbar.top, width, 1), 166);
        toolbar.draw(canvas, LIBRARY_CONTROL, "Library");
        toolbar.draw(canvas, BOOKMARK_CONTROL, "Bookmark +");
        toolbar.draw(canvas, EXIT_CONTROL, "Exit Reader");
        let mut title_right = BOOKMARK_CONTROL.x;
        if !self.back.is_empty() {
            toolbar.draw(canvas, LOCATION_CONTROL, "Previous location");
            title_right = LOCATION_CONTROL.x;
        }
        let title = self
            .active
            .as_ref()
            .map(|book| book.reading.probe.title.as_str())
            .or_else(|| self.fixed.as_ref().map(|book| book.entry.title.as_str()))
            .unwrap_or("Reader");
        canvas.text(
            Rect::new(
                LIBRARY_CONTROL.x + LIBRARY_CONTROL.w + 18,
                16,
                title_right - 210,
                60,
            ),
            self.notice.as_deref().unwrap_or(title),
            BODY,
        );
        for (index, tool) in tools.iter().enumerate() {
            toolbar.draw(canvas, toolbar.rect(index), &tool.title);
        }
    }

    fn page_paint(&self) -> PagePaint {
        let inverted = self.controls.night_pages;
        if matches!(self.screen, Screen::BookTools | Screen::FixedTools) {
            let toolbar =
                ToolbarLayout::new(self.reading_tools().len(), self.controls.large_toolbar);
            PagePaint {
                visible: TOP_BAR_HEIGHT as u32..toolbar.top as u32,
                inverted,
            }
        } else {
            PagePaint::full(inverted)
        }
    }

    fn reading_tools_input(&mut self, input: Input) -> Change {
        let reading_screen = if self.screen == Screen::FixedTools {
            Screen::Fixed
        } else {
            Screen::Book
        };
        match input {
            Input::Tap(x, y) => {
                let tools = self.reading_tools();
                let toolbar = ToolbarLayout::new(tools.len(), self.controls.large_toolbar);
                let action = if EXIT_CONTROL.contains(x, y) {
                    return self.exit();
                } else if LIBRARY_CONTROL.contains(x, y) {
                    Some(Action::Show(Screen::Library))
                } else if BOOKMARK_CONTROL.contains(x, y) {
                    Some(Action::AddBookmark)
                } else if LOCATION_CONTROL.contains(x, y) && !self.back.is_empty() {
                    Some(Action::ReturnToLocation)
                } else {
                    tools
                        .into_iter()
                        .enumerate()
                        .find(|(index, _)| toolbar.rect(*index).contains(x, y))
                        .map(|(_, tool)| tool.action)
                };
                match action {
                    Some(action) => match self.activate(action) {
                        Ok(()) => Change::Partial,
                        Err(error) => self.report(error),
                    },
                    None if y >= TOP_BAR_HEIGHT as f32 && y < toolbar.top as f32 => {
                        self.show(reading_screen);
                        Change::Partial
                    }
                    None => Change::None,
                }
            }
            Input::PageBack | Input::PageForward | Input::Swipe { .. } => {
                self.show(reading_screen);
                let result = self.input(input);
                if result == Change::None {
                    Change::Partial
                } else {
                    result
                }
            }
            _ => Change::None,
        }
    }

    fn activate(&mut self, action: Action) -> Result<()> {
        match action {
            Action::Open(entry) => self.open_book(entry)?,
            Action::OpenTracked(id) => {
                let entry = self.locate_tracked(id)?;
                self.open_book(entry)?;
            }
            Action::Continue => {
                let state = self
                    .library
                    .recently_read()
                    .next()
                    .ok_or("No recent book")?;
                let entry = with_storage(|| self.library.locate(state))?;
                self.open_book(entry)?;
            }
            Action::ClearLibraryFilter => {
                self.library_query.clear();
                self.show(Screen::Library);
            }
            Action::ListBooks(list) => {
                self.book_list = Some(list);
                self.show(Screen::BookList);
            }
            Action::OrganizeCurrent => {
                let (entry, back) = if let Some(fixed) = &self.fixed {
                    (fixed.entry.clone(), Screen::FixedTools)
                } else {
                    (
                        self.active
                            .as_ref()
                            .ok_or("Open a book first")?
                            .entry
                            .clone(),
                        Screen::BookTools,
                    )
                };
                self.organize_book(entry, back)?;
            }
            Action::Favorite(favorite) => {
                let book = self.book_details.as_ref().ok_or("Choose a book first")?;
                self.library.track(&book.entry, book.id).favorite = favorite;
                self.mark_dirty();
            }
            Action::SetStatus(status) => {
                let book = self.book_details.as_ref().ok_or("Choose a book first")?;
                self.library.track(&book.entry, book.id).set_status(status);
                self.mark_dirty();
            }
            Action::CollectionMembership(name, included) => {
                let book = self.book_details.as_ref().ok_or("Choose a book first")?;
                self.library.track(&book.entry, book.id);
                self.library
                    .set_collection_membership(&name, book.id, included)?;
                self.mark_dirty();
            }
            Action::NameCollection(existing) => {
                self.show(Screen::CollectionName);
                self.editor = Editor::new(64);
                if let Some(name) = &existing {
                    self.editor.text = name.chars().collect();
                    self.editor.caret = self.editor.text.len();
                }
                self.rename_collection = existing;
            }
            Action::RemoveCollection => {
                let Some(BookList::Collection(name)) = &self.book_list else {
                    return Err("Choose a collection first".into());
                };
                if self.library.remove_collection(name) {
                    self.mark_dirty();
                }
                self.book_list = None;
                self.show(Screen::OrganizeLibrary);
            }
            Action::SetReadingControls(controls) => {
                controls.save(&self.controls_path)?;
                self.controls = controls;
            }
            Action::LockTouch => {
                self.selection = None;
                self.pdf_selection = None;
                self.touch_locked = true;
                self.show(if self.fixed.is_some() {
                    Screen::Fixed
                } else {
                    Screen::Book
                });
            }
            Action::ExportBackup => self.export_backup()?,
            Action::ClearOcr => {
                with_storage(|| match fs::remove_dir_all(&self.ocr_root) {
                    Ok(()) => Ok(()),
                    Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
                    Err(error) => Err(error),
                })?;
                self.show(Screen::ReadingData);
                self.notice = Some("OCR cache cleared".into());
            }
            Action::PreviewBackup => self.preview_backup()?,
            Action::MergeBackup => self.restore_backup()?,
            Action::PreviewKoReader => self.preview_koreader()?,
            Action::ApplyKoReader => self.apply_koreader()?,
            Action::PreviewKoReaderPosition => self.preview_koreader_position()?,
            Action::ApplyKoReaderPosition => self.apply_koreader_position()?,
            Action::PreviewPdfAnnotations => self.preview_pdf_annotations()?,
            Action::ApplyPdfAnnotations => self.apply_pdf_annotations()?,
            Action::ExportAnnotatedPdf => self.export_annotated_pdf()?,
            Action::Show(Screen::Library) => {
                self.save()?;
                self.active = None;
                self.fixed = None;
                self.touch_locked = false;
                self.ocr = None;
                self.selection = None;
                self.back.clear();
                self.show(Screen::Library);
            }
            Action::Show(screen) => self.show(screen),
            Action::Jump(target) => self.follow(target)?,
            Action::ReturnToLocation => {
                let previous = self.back.last().cloned().ok_or("No previous location")?;
                if let Some(fixed) = &mut self.fixed {
                    fixed.reading.jump(previous.page)?;
                    self.pdf_result_region = None;
                    self.pdf_selection = None;
                    self.show(Screen::Fixed);
                    self.record_position();
                } else {
                    self.restore_position(&previous)?;
                }
                self.back.pop();
            }
            Action::FitPage(width_fit) => {
                self.fixed
                    .as_mut()
                    .ok_or("No open document")?
                    .reading
                    .fit(width_fit)?;
                self.show(Screen::Fixed);
                self.record_position();
            }
            Action::FitHeight => {
                let fixed = self
                    .fixed
                    .as_mut()
                    .ok_or("Open a fixed-page document first")?;
                fixed.reading.fit_height()?;
                self.show(Screen::Fixed);
                self.record_position();
            }
            Action::FixedScroll(enabled) => {
                let fixed = self.fixed.as_mut().ok_or("No open document")?;
                fixed.reading.set_continuous(enabled)?;
                self.pdf_selection = None;
                self.show(Screen::Fixed);
                self.record_position();
            }
            Action::ComicSpread(enabled) => {
                let fixed = self.fixed.as_mut().ok_or("Open a comic first")?;
                fixed.reading.set_spread(enabled)?;
                self.show(Screen::Fixed);
                self.record_position();
            }
            Action::ScanContrast(higher) => {
                let fixed = self
                    .fixed
                    .as_mut()
                    .ok_or("Open a supported document first")?;
                fixed.reading.set_scan_contrast(higher)?;
                self.show(Screen::Fixed);
                self.record_position();
                self.notice = Some(
                    if higher {
                        "Higher contrast"
                    } else {
                        "Normal contrast"
                    }
                    .into(),
                );
            }
            Action::ScanColumns => self.open_scan_columns()?,
            Action::Overview => {
                let (_, current) = self.overview_range().ok_or("Open a book first")?;
                self.show(Screen::PageOverview);
                self.update_overview(current / OVERVIEW_PAGES * OVERVIEW_PAGES);
            }
            Action::RotatePage => {
                let fixed = self.fixed.as_mut().ok_or("No open document")?;
                fixed.reading.rotate()?;
                self.show(Screen::Fixed);
                self.record_position();
            }
            Action::ReversePageDirection => {
                let fixed = self.fixed.as_mut().ok_or("No open document")?;
                fixed.reading.reverse_direction()?;
                self.show(Screen::Fixed);
                self.record_position();
            }
            Action::CropPage => {
                let fixed = self.fixed.as_mut().ok_or("No open document")?;
                self.crop_dialog = Some((fixed.reading.start_crop()?, None));
                self.show(Screen::FixedCrop);
            }
            Action::RecognizeText => self.start_ocr(None)?,
            Action::RecognizeLanguage(language) => self.start_ocr(Some(&language))?,
            Action::ReadFixedText => self.read_fixed_text()?,
            Action::FixedPage(page) => {
                self.follow_fixed(page, None)?;
            }
            Action::Heading => {}
            Action::Bookmark(index) if self.fixed.is_some() => {
                let fixed = self.fixed.as_mut().ok_or("No open document")?;
                let page = self
                    .library
                    .state(fixed.id)
                    .and_then(|state| state.bookmarks.get(index))
                    .ok_or("That bookmark is unavailable")?
                    .position
                    .page;
                self.follow_fixed(page, None)?;
            }
            Action::Bookmark(index) => {
                let active = self.active.as_ref().ok_or("No open book")?;
                let bookmark = self
                    .library
                    .state(active.id)
                    .and_then(|state| state.bookmarks.get(index))
                    .ok_or("That bookmark is unavailable")?
                    .position
                    .clone();
                let previous = position(&active.reading);
                self.restore_position(&bookmark)?;
                self.push_history(previous);
            }
            Action::AddBookmark => {
                self.add_bookmark()?;
            }
            Action::Text(settings) => {
                let active = self.active.as_mut().ok_or("No open book")?;
                active.reading.reflow(settings)?;
                let preferences = &mut self
                    .library
                    .remember(&active.entry, active.id, &active.reading.probe.title)
                    .preferences;
                preferences.font_size = Some(settings.font_size);
                preferences.margin = Some(settings.margin);
                preferences.line_height = Some(settings.line_height);
                preferences.scroll = active.reading.config.scroll;
                preferences.alignment = settings.alignment;
                preferences.bold_text = settings.bold_text;
                preferences.override_fonts = settings.override_fonts;
                self.show(Screen::Book);
                self.record_position();
            }
            Action::ShowFonts => {
                with_storage(|| fs::create_dir_all(&self.fonts_directory))?;
                self.available_fonts = reading_fonts::available(&self.fonts_directory)?;
                self.show(Screen::ReadingFonts);
            }
            Action::SetFont(font) => {
                let fonts = self.fonts_for(font.as_deref())?;
                let active = self.active.as_mut().ok_or("Open a book first")?;
                active.reading.set_fonts(fonts)?;
                self.library
                    .remember(&active.entry, active.id, &active.reading.probe.title)
                    .preferences
                    .font_file = font;
                self.show(Screen::Book);
                self.record_position();
            }
            Action::ApplyProfile(index) => {
                let profile = self
                    .profiles
                    .get(index)
                    .ok_or("That profile is unavailable")?;
                self.apply_text_profile(profile.preferences.clone())?;
            }
            Action::RemoveProfile(index) => {
                if index >= self.profiles.len() {
                    return Err("That profile is unavailable".into());
                }
                let mut profiles = self.profiles.clone();
                profiles.remove(index);
                reading_profiles::save(&self.profiles_path, &profiles)?;
                self.profiles = profiles;
                self.show(Screen::ReadingProfiles);
            }
            Action::ResetText => self.apply_text_profile(BookPreferences::default())?,
            Action::ReadingMode(scroll) => {
                let active = self.active.as_mut().ok_or("No open book")?;
                active.reading.set_scroll(scroll)?;
                self.library
                    .remember(&active.entry, active.id, &active.reading.probe.title)
                    .preferences
                    .scroll = scroll;
                self.show(Screen::Book);
                self.record_position();
            }
            Action::SearchHit(index) => {
                let hit = self.hits.get(index).ok_or("That result is unavailable")?;
                let end = hit.end.clone();
                let previous = position(&self.active.as_ref().ok_or("No open book")?.reading);
                let target = ReadingPosition {
                    chapter: hit.chapter,
                    page: 0,
                    anchor: Some(hit.anchor.clone()),
                };
                self.restore_position(&target)?;
                if let Some(active) = &self.active
                    && let Some(start) = target.anchor
                {
                    self.result_selection = active.reading.probe.select_range(&start, &end);
                }
                self.push_history(previous);
            }
            Action::PdfSearchHit(index) => {
                let hit = self
                    .pdf_hits
                    .get(index)
                    .ok_or("That result is unavailable")?;
                self.follow_fixed(hit.page, hit.region)?;
            }
            Action::PdfHighlight(index) => {
                let fixed = self.fixed.as_ref().ok_or("No open PDF")?;
                let mark = self
                    .library
                    .state(fixed.id)
                    .and_then(|state| state.pdf_highlights.get(index))
                    .ok_or("That highlight is unavailable")?;
                self.follow_fixed(mark.page, mark.bounds.first().copied())?;
            }
            Action::Highlight(index) => {
                let active = self.active.as_ref().ok_or("No open book")?;
                let highlight = self
                    .library
                    .state(active.id)
                    .and_then(|state| state.highlights.get(index))
                    .ok_or("That highlight is unavailable")?;
                let anchor = highlight.selection.start.clone();
                let chapter = active
                    .reading
                    .probe
                    .chapter_index(&anchor.chapter)
                    .ok_or("That chapter is unavailable")?;
                let previous = position(&active.reading);
                self.restore_position(&ReadingPosition {
                    chapter,
                    page: 0,
                    anchor: Some(anchor),
                })?;
                self.push_history(previous);
            }
            Action::ManageHighlight(index) => {
                self.highlight_index = index;
                self.show(Screen::Annotation);
            }
            Action::EditNote => {
                let index = self.highlight_index;
                let pdf = self.fixed.is_some();
                let state = self.highlight_state()?;
                let note = if pdf {
                    &state.pdf_highlights[index].note
                } else {
                    &state.highlights[index].note
                };
                let text: Vec<_> = note.as_deref().unwrap_or("").chars().take(240).collect();
                self.editor = Editor::new(240);
                self.editor.text = text;
                self.editor.caret = self.editor.text.len();
                self.show(Screen::EditNote);
            }
            Action::RemoveHighlight => {
                let index = self.highlight_index;
                let pdf = self.fixed.is_some();
                let state = self.highlight_state()?;
                if pdf {
                    state.pdf_highlights.remove(index);
                } else {
                    state.highlights.remove(index);
                }
                self.mark_dirty();
                self.show(Screen::Highlights);
                self.notice = Some("Highlight removed".into());
            }
            Action::ExportHighlights(format) => self.export_highlights(format)?,
        }
        Ok(())
    }

    fn locate_tracked(&self, id: u64) -> Result<LibraryBook> {
        let state = self.library.state(id).ok_or("That book is unavailable")?;
        Ok(with_storage(|| self.library.locate(state))?)
    }

    fn fonts_for(&self, file: Option<&str>) -> io::Result<Vec<PathBuf>> {
        let mut fonts = self.fonts.clone();
        if let Some(file) = file {
            let font = reading_fonts::resolve(&self.fonts_directory, file)?;
            with_storage(|| {
                crate::font_context(std::slice::from_ref(&font)).map_err(|_| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        "Reader could not load the selected font",
                    )
                })
            })?;
            fonts.insert(0, font);
        }
        Ok(fonts)
    }

    fn apply_text_profile(&mut self, preferences: BookPreferences) -> Result<()> {
        let fonts = self.fonts_for(preferences.font_file.as_deref())?;
        let active = self.active.as_mut().ok_or("Open a book first")?;
        let position = position(&active.reading);
        let mut config = active.reading.config.clone();
        config.settings = preferences.apply_to(device_settings());
        config.scroll = preferences.scroll;
        config.fonts = fonts;
        active.reading = Reading::restore(
            config,
            position.chapter,
            position.page,
            position.anchor.as_ref(),
        )?;
        self.library
            .remember(&active.entry, active.id, &active.reading.probe.title)
            .preferences = preferences;
        self.show(Screen::Book);
        self.record_position();
        Ok(())
    }

    fn save_reading_profile(&mut self, name: &str) -> Result<()> {
        let active = self.active.as_ref().ok_or("Open a book first")?;
        let config = &active.reading.config;
        let settings = config.settings;
        let font_file = self
            .library
            .state(active.id)
            .and_then(|state| state.preferences.font_file.clone());
        let preferences = BookPreferences {
            font_size: Some(settings.font_size),
            margin: Some(settings.margin),
            line_height: Some(settings.line_height),
            scroll: config.scroll,
            alignment: settings.alignment,
            bold_text: settings.bold_text,
            override_fonts: settings.override_fonts,
            font_file,
        };
        let mut profiles = self.profiles.clone();
        reading_profiles::insert(&mut profiles, name, preferences)?;
        reading_profiles::save(&self.profiles_path, &profiles)?;
        self.profiles = profiles;
        self.show(Screen::ReadingProfiles);
        self.notice = Some("Reading profile saved".into());
        Ok(())
    }

    fn organize_book(&mut self, entry: LibraryBook, back: Screen) -> Result<()> {
        let id = with_storage(|| self.library.identify(&entry))?;
        self.show(Screen::BookDetails);
        self.book_details = Some(BookDetails { entry, id, back });
        Ok(())
    }

    fn name_editor_input(&mut self, input: Input) -> Change {
        let profile = self.screen == Screen::ProfileName;
        if matches!(input, Input::PageBack)
            || matches!(input, Input::Tap(x, y) if primitive_ui::back_button(WIDTH as i32).contains(x, y))
        {
            let back = if profile {
                Screen::ReadingProfiles
            } else if self.rename_collection.take().is_some() {
                Screen::BookList
            } else {
                Screen::OrganizeLibrary
            };
            self.show(back);
            return Change::Partial;
        }
        let Input::Tap(x, y) = input else {
            return Change::None;
        };
        let layout = Layout::new(WIDTH, HEIGHT, 4, 5);
        let Some(key) = keyboard_key(&layout, &self.editor, "Save", x, y) else {
            return Change::None;
        };
        if matches!(self.editor.press(key), Press::Go) {
            let name = self.editor.string();
            let result = if profile {
                self.save_reading_profile(name.trim())
            } else {
                self.save_collection_name(name.trim())
            };
            if let Err(error) = result {
                return self.report(error);
            }
        }
        Change::Partial
    }

    fn save_collection_name(&mut self, name: &str) -> Result<()> {
        let changed = if let Some(old) = &self.rename_collection {
            self.library.rename_collection(old, name)?
        } else {
            self.library.create_collection(name)?;
            true
        };
        if changed {
            self.mark_dirty();
        }
        self.rename_collection = None;
        self.activate(Action::ListBooks(BookList::Collection(name.to_owned())))
    }

    fn add_bookmark(&mut self) -> Result<()> {
        let (entry, id, title, position, label, screen) = if let Some(fixed) = &self.fixed {
            let position = fixed_position(&fixed.reading);
            let label = format!("Page {}", position.page + 1);
            (
                &fixed.entry,
                fixed.id,
                fixed.entry.title.as_str(),
                position,
                label,
                Screen::Fixed,
            )
        } else {
            let active = self.active.as_ref().ok_or("No open book")?;
            let reading = &active.reading;
            let label = format!(
                "{} · Page {}",
                reading.probe.chapter_label,
                reading.page + 1
            );
            (
                &active.entry,
                active.id,
                reading.probe.title.as_str(),
                position(reading),
                label,
                Screen::Book,
            )
        };
        let state = self.library.remember(entry, id, title);
        if !state.bookmarks.iter().any(|item| item.position == position) {
            if state.bookmarks.len() >= 256 {
                return Err("This book has reached its bookmark limit".into());
            }
            state.bookmarks.push(Bookmark { label, position });
            self.mark_dirty();
        }
        self.show(screen);
        self.notice = Some("Page bookmarked".into());
        Ok(())
    }

    fn follow(&mut self, target: Destination) -> Result<()> {
        let active = self.active.as_mut().ok_or("No open book")?;
        let previous = position(&active.reading);
        let chapter = active
            .reading
            .probe
            .chapter_index(&target.path)
            .ok_or("That link is outside the reading order")?;
        active.reading.jump(chapter, target.fragment.as_deref())?;
        self.push_history(previous);
        self.selection = None;
        self.result_selection = None;
        self.show(Screen::Book);
        self.record_position();
        Ok(())
    }

    fn follow_fixed(&mut self, page: usize, region: Option<PdfTextRect>) -> Result<()> {
        let fixed = self.fixed.as_mut().ok_or("No open document")?;
        let previous = fixed_position(&fixed.reading);
        fixed.reading.jump(page)?;
        if page != previous.page {
            self.push_history(previous);
        }
        self.pdf_selection = None;
        self.pdf_result_region = region.map(|bounds| (page, bounds));
        self.show(Screen::Fixed);
        self.record_position();
        Ok(())
    }

    fn restore_position(&mut self, new_position: &ReadingPosition) -> Result<()> {
        let reading = &mut self.active.as_mut().ok_or("No open book")?.reading;
        *reading = Reading::restore(
            reading.config.clone(),
            new_position.chapter,
            new_position.page,
            new_position.anchor.as_ref(),
        )?;
        self.selection = None;
        self.result_selection = None;
        self.show(Screen::Book);
        self.record_position();
        Ok(())
    }

    fn push_history(&mut self, previous: ReadingPosition) {
        if self.back.len() == 64 {
            self.back.remove(0);
        }
        self.back.push(previous);
    }

    fn report(&mut self, error: impl std::fmt::Display) -> Change {
        self.notice = Some(error.to_string());
        Change::Partial
    }

    fn highlight_state(&mut self) -> Result<&mut crate::library::BookState> {
        let (entry, id, title, pdf) = if let Some(fixed) = &self.fixed {
            (&fixed.entry, fixed.id, fixed.entry.title.as_str(), true)
        } else {
            let active = self.active.as_ref().ok_or("No open book")?;
            (
                &active.entry,
                active.id,
                active.reading.probe.title.as_str(),
                false,
            )
        };
        let state = self.library.remember(entry, id, title);
        let count = if pdf {
            state.pdf_highlights.len()
        } else {
            state.highlights.len()
        };
        if self.highlight_index >= count {
            return Err("That highlight is unavailable".into());
        }
        Ok(state)
    }

    fn handle_find(&mut self, input: Input) -> Change {
        let edit = self.screen == Screen::EditNote;
        let note = self.screen == Screen::Note || edit;
        if matches!(input, Input::PageBack)
            || matches!(input, Input::Tap(x, y) if primitive_ui::back_button(WIDTH as i32).contains(x, y))
        {
            self.show(match self.screen {
                Screen::LibraryFind => Screen::Library,
                Screen::EditNote => Screen::Annotation,
                Screen::Note if self.fixed.is_some() => Screen::Fixed,
                Screen::Note => Screen::Book,
                _ if self.fixed.is_some() => Screen::FixedTools,
                _ => Screen::BookTools,
            });
            return Change::Partial;
        }
        let Input::Tap(x, y) = input else {
            return Change::None;
        };
        if self.screen == Screen::Find && FIND_CASE.contains(x, y) {
            self.find_match_case = !self.find_match_case;
            return Change::Partial;
        }
        let layout = Layout::new(WIDTH, HEIGHT, 4, 5);
        let action = if note { "Save" } else { "Find" };
        let Some(key) = keyboard_key(&layout, &self.editor, action, x, y) else {
            return Change::None;
        };
        if matches!(self.editor.press(key), Press::Go) {
            if note {
                let value = self.editor.string();
                let result = if edit {
                    self.update_note(value)
                } else if self.fixed.is_some() {
                    self.save_pdf_highlight(Some(value))
                } else {
                    self.save_highlight(Some(value))
                };
                return match result {
                    Ok(()) => Change::Partial,
                    Err(error) => self.report(error),
                };
            }
            let query = self.editor.string();
            if self.screen == Screen::LibraryFind {
                self.library_query = query.trim().to_owned();
                self.show(Screen::Library);
                return Change::Partial;
            }
            if self.fixed.is_some() {
                return match PdfSearch::new(&query, self.find_match_case) {
                    Ok(search) => {
                        self.show(Screen::Matches);
                        self.pdf_hits.clear();
                        self.pdf_search = Some(search);
                        self.search_at = Some(Instant::now() + SEARCH_DELAY);
                        self.next_search_refresh = Instant::now() + SEARCH_REFRESH;
                        Change::Partial
                    }
                    Err(error) => self.report(error),
                };
            }
            match Search::with_case(&query, self.find_match_case) {
                Ok(search) => {
                    self.show(Screen::Matches);
                    self.hits.clear();
                    self.search = Some(search);
                    self.search_at = Some(Instant::now() + SEARCH_DELAY);
                    self.next_search_refresh = Instant::now() + SEARCH_REFRESH;
                }
                Err(error) => return self.report(error),
            }
        }
        Change::Partial
    }

    fn update_note(&mut self, value: String) -> Result<()> {
        let index = self.highlight_index;
        let pdf = self.fixed.is_some();
        let note = (!value.trim().is_empty()).then(|| value.trim().to_owned());
        let state = self.highlight_state()?;
        if pdf {
            state.pdf_highlights[index].note = note;
        } else {
            state.highlights[index].note = note;
        }
        self.mark_dirty();
        self.show(Screen::Highlights);
        self.notice = Some("Note saved".into());
        Ok(())
    }

    fn handle_page_number(&mut self, input: Input) -> Change {
        if matches!(input, Input::PageBack)
            || matches!(input, Input::Tap(x, y) if primitive_ui::back_button(WIDTH as i32).contains(x, y))
        {
            self.show(if self.fixed.is_some() {
                Screen::FixedTools
            } else {
                Screen::BookTools
            });
            return Change::Partial;
        }
        let Input::Tap(x, y) = input else {
            return Change::None;
        };
        let layout = Layout::new(WIDTH, HEIGHT, 4, 5);
        let Some(key) = keyboard_key(&layout, &self.editor, "Go", x, y) else {
            return Change::None;
        };
        if matches!(self.editor.press(key), Press::Go) {
            if self.fixed.is_none() {
                return match self.jump_reflow_number() {
                    Ok(()) => Change::Partial,
                    Err(error) => self.report(error),
                };
            }
            let target = self.editor.string();
            if let Err(error) = self.jump_fixed_number(target.trim()) {
                return self.report(error);
            }
        }
        Change::Partial
    }

    fn jump_fixed_number(&mut self, target: &str) -> Result<()> {
        let fixed = self.fixed.as_ref().ok_or("No open document")?;
        let reading = &fixed.reading;
        let count = reading.count();
        let page = if let Some(label) = target.strip_prefix("label ") {
            reading.find_page_label(label.trim())
        } else if let Some(percent) = target.strip_suffix('%') {
            Some(page_at_percent(percent, count)?)
        } else if let Ok(number) = target.parse::<usize>() {
            (1..=count).contains(&number).then(|| number - 1)
        } else {
            reading.find_page_label(target)
        };
        let page = page
            .ok_or_else(|| format!("Enter a page from 1 to {count} or a unique printed label"))?;
        self.follow_fixed(page, None)
    }

    fn handle_pdf_password(&mut self, input: Input) -> Change {
        if matches!(input, Input::PageBack)
            || matches!(input, Input::Tap(x, y) if primitive_ui::back_button(WIDTH as i32).contains(x, y))
        {
            self.locked = None;
            self.editor = Editor::new(64);
            return match self.activate(Action::Show(Screen::Library)) {
                Ok(()) => Change::Partial,
                Err(error) => self.report(error),
            };
        }
        let Input::Tap(x, y) = input else {
            return Change::None;
        };
        let layout = Layout::new(WIDTH, HEIGHT, 4, 5);
        let Some(key) = keyboard_key(&layout, &self.editor, "Open", x, y) else {
            return Change::None;
        };
        if matches!(self.editor.press(key), Press::Go) {
            let Some(entry) = self.locked.clone() else {
                return self.report("Choose the PDF again");
            };
            let password = self.editor.string();
            self.editor = Editor::new(64);
            if let Err(error) = self.open_with_password(entry, Some(&password)) {
                return self.report(error);
            }
        }
        Change::Partial
    }

    fn step_search(&mut self, now: Instant) -> Result<Change> {
        if self.pdf_search.is_some() {
            return self.step_pdf_search(now);
        }
        let search = self.search.as_mut().ok_or("No active search")?;
        let book = &self
            .active
            .as_ref()
            .ok_or("No open book")?
            .reading
            .config
            .book;
        let previous = self.hits.len();
        with_storage(|| {
            let mut book = Book::open(book).map_err(io::Error::other)?;
            for _ in 0..4 {
                self.hits
                    .extend(search.step(&mut book).map_err(io::Error::other)?);
                if search.is_finished() {
                    break;
                }
            }
            Ok(())
        })?;
        if search.is_finished() {
            self.search_at = None;
            if search.limit_reached() {
                self.notice = Some("Showing first 100 matches".into());
            } else if search.skipped_chapters() > 0 {
                self.notice = Some("Some chapters could not be searched".into());
            }
            Ok(Change::Partial)
        } else {
            self.search_at = Some(Instant::now() + SEARCH_DELAY);
            if self.hits.len() != previous && now >= self.next_search_refresh {
                self.next_search_refresh = now + SEARCH_REFRESH;
                Ok(Change::Partial)
            } else {
                Ok(Change::None)
            }
        }
    }

    fn jump_reflow_number(&mut self) -> Result<()> {
        let active = self.active.as_ref().ok_or("No open book")?;
        let input = self.editor.string();
        let input = input.trim();
        if let Some(percent) = input.strip_suffix('%') {
            let page = page_at_percent(percent, active.reading.probe.page_count())?;
            return self.follow_reflow_page(active.reading.chapter, page);
        }
        let number = |value: &str| {
            value
                .trim()
                .parse::<usize>()
                .ok()
                .and_then(|n| n.checked_sub(1))
        };
        let (chapter, page) = if let Some((chapter, page)) = input.split_once(':') {
            (
                number(chapter).ok_or("Enter a valid chapter number")?,
                number(page),
            )
        } else {
            (active.reading.chapter, number(input))
        };
        let page = page.ok_or("Enter a page number, or a chapter and page separated by a colon")?;
        self.follow_reflow_page(chapter, page)
    }

    fn follow_reflow_page(&mut self, chapter: usize, page: usize) -> Result<()> {
        let active = self.active.as_mut().ok_or("No open book")?;
        if chapter >= active.reading.probe.chapter_count() {
            return Err("That chapter is outside the book".into());
        }
        let previous = position(&active.reading);
        active.reading.go_to_page(chapter, page)?;
        self.push_history(previous);
        self.show(Screen::Book);
        self.record_position();
        Ok(())
    }

    fn overview_range(&self) -> Option<(usize, usize)> {
        if let Some(fixed) = &self.fixed {
            Some((fixed.reading.count(), fixed.reading.page()))
        } else {
            let reading = &self.active.as_ref()?.reading;
            Some((reading.probe.page_count(), reading.page))
        }
    }

    fn update_overview(&mut self, first: usize) {
        let Some((count, _)) = self.overview_range() else {
            return;
        };
        let pages = first..(first + OVERVIEW_PAGES).min(count);
        let tiles = if let Some(fixed) = &self.fixed {
            pages
                .map(|page| fixed.reading.thumbnail(page).ok())
                .collect()
        } else if let Some(active) = &mut self.active {
            pages
                .map(|page| active.reading.thumbnail(page).ok())
                .collect()
        } else {
            Vec::new()
        };
        self.overview = Some(PageOverview { first, tiles });
    }

    fn draw_overview(&self, canvas: &mut Canvas) {
        let Some(overview) = &self.overview else {
            return;
        };
        let Some((count, current)) = self.overview_range() else {
            return;
        };
        let section = self.active.as_ref().map_or_else(
            || format!("{count} pages"),
            |active| format!("{}  ·  {count} pages", active.reading.probe.chapter_label),
        );
        canvas.text(
            Rect::new(LIST_X, LIST_Y, WIDTH as i32 - 2 * LIST_X, 66),
            self.notice.as_deref().unwrap_or(&section),
            CAPTION,
        );
        for (index, tile) in overview.tiles.iter().enumerate() {
            let rect = overview_tile(index);
            canvas.card(rect);
            let page = overview.first + index;
            let label = self.fixed.as_ref().map_or_else(
                || format!("Page {}", page + 1),
                |fixed| fixed_page_label(&fixed.reading, page),
            );
            if let Some(tile) = tile {
                let mut thumbnail = tile.clone();
                if self.controls.night_pages {
                    for pixel in &mut thumbnail.pixels {
                        *pixel = 255 - *pixel;
                    }
                }
                let width = thumbnail.width as i32;
                let height = thumbnail.height as i32;
                let frame = Rect::new(rect.x + (rect.w - width) / 2, rect.y + 21, width, height);
                canvas.fill(frame.inset(-1), 170);
                canvas.picture(frame, thumbnail);
            } else {
                canvas.text(
                    Rect::new(rect.x + 10, rect.y + 120, rect.w - 20, 64),
                    "Preview unavailable",
                    CAPTION.center(),
                );
            }
            canvas.button(
                Rect::new(rect.x + 18, rect.y + rect.h - 84, rect.w - 36, 68),
                &label,
                page == current,
                false,
            );
        }
        if overview.first != 0 {
            canvas.button(PREV, "Previous", false, false);
        }
        if overview.first + OVERVIEW_PAGES < count {
            canvas.button(NEXT, "Next", false, false);
        }
    }

    fn overview_input(&mut self, input: Input) -> Change {
        let Some(overview) = &self.overview else {
            return Change::None;
        };
        let step = match input {
            Input::PageForward => OVERVIEW_PAGES as isize,
            Input::PageBack => -(OVERVIEW_PAGES as isize),
            Input::Swipe { dx, dy, .. } if dy.abs() > dx.abs() => {
                if dy < 0.0 {
                    OVERVIEW_PAGES as isize
                } else {
                    -(OVERVIEW_PAGES as isize)
                }
            }
            Input::Tap(x, y) if PREV.contains(x, y) => -(OVERVIEW_PAGES as isize),
            Input::Tap(x, y) if NEXT.contains(x, y) => OVERVIEW_PAGES as isize,
            Input::Tap(x, y) if primitive_ui::back_button(WIDTH as i32).contains(x, y) => {
                self.show(if self.fixed.is_some() {
                    Screen::FixedTools
                } else {
                    Screen::BookTools
                });
                return Change::Partial;
            }
            Input::Tap(x, y) => {
                let Some(index) =
                    (0..overview.tiles.len()).find(|&index| overview_tile(index).contains(x, y))
                else {
                    return Change::None;
                };
                let page = overview.first + index;
                let result = if let Some(active) = &self.active {
                    self.follow_reflow_page(active.reading.chapter, page)
                } else {
                    self.follow_fixed(page, None)
                };
                return match result {
                    Ok(()) => Change::Partial,
                    Err(error) => self.report(error),
                };
            }
            _ => return Change::None,
        };
        if let Some(first) = overview.first.checked_add_signed(step)
            && self
                .overview_range()
                .is_some_and(|(count, _)| first < count)
        {
            self.update_overview(first);
            Change::Partial
        } else {
            Change::None
        }
    }

    fn step_pdf_search(&mut self, now: Instant) -> Result<Change> {
        let search = self.pdf_search.as_mut().ok_or("No active PDF search")?;
        let reading = &self.fixed.as_ref().ok_or("No open PDF")?.reading;
        let previous = self.pdf_hits.len();
        let started = Instant::now();
        let stop_page = (search.next_page + 4).min(reading.count());
        while search.next_page < stop_page && self.pdf_hits.len() < MAX_SEARCH_HITS {
            let page = search.next_page;
            search.next_page += 1;
            match reading.page_text(page) {
                Ok(text) => self.pdf_hits.extend(search.hits(
                    page,
                    &text,
                    MAX_SEARCH_HITS - self.pdf_hits.len(),
                )),
                Err(_) => search.skipped += 1,
            }
            if started.elapsed() >= SEARCH_DELAY {
                break;
            }
        }
        if search.next_page == reading.count() || self.pdf_hits.len() == MAX_SEARCH_HITS {
            self.search_at = None;
            self.notice = if self.pdf_hits.len() == MAX_SEARCH_HITS {
                Some("Showing first 100 matches".into())
            } else if search.skipped > 0 {
                Some("Some pages could not be searched".into())
            } else if search.text_pages == 0 {
                Some("This document has no embedded text to search".into())
            } else {
                None
            };
            return Ok(Change::Partial);
        }
        self.search_at = Some(Instant::now() + SEARCH_DELAY);
        if self.pdf_hits.len() != previous && now >= self.next_search_refresh {
            self.next_search_refresh = now + SEARCH_REFRESH;
            Ok(Change::Partial)
        } else {
            Ok(Change::None)
        }
    }

    fn handle_book(&mut self, input: Input) -> Change {
        if let Input::LongTap(x, y) = input {
            match self.inspect_illustration(x, y) {
                Ok(true) => return Change::Partial,
                Err(error) => return self.report(error),
                Ok(false) => {}
            }
            let Some(active) = &self.active else {
                return Change::None;
            };
            self.selection = active.reading.word_at(x, y);
            self.notice = None;
            return Change::Partial;
        }
        if let Input::Tap(x, y) = input {
            if self.selection.is_some() {
                if CANCEL_MARK.contains(x, y) {
                    self.selection = None;
                    return Change::Partial;
                }
                if MARK.contains(x, y) {
                    return match self.save_highlight(None) {
                        Ok(()) => Change::Partial,
                        Err(error) => self.report(error),
                    };
                }
                if NOTE.contains(x, y) {
                    self.editor = Editor::new(128);
                    self.show(Screen::Note);
                    return Change::Partial;
                }
                if LOOKUP.contains(x, y) {
                    return match self.look_up_selection() {
                        Ok(()) => Change::Partial,
                        Err(error) => self.report(error),
                    };
                }
                if let (Some(active), Some(selection)) = (&self.active, &self.selection)
                    && let Some(word) = active.reading.word_at(x, y)
                {
                    let start = if (word.start.block, word.start.offset)
                        < (selection.start.block, selection.start.offset)
                    {
                        &word.start
                    } else {
                        &selection.start
                    };
                    let end = if (word.end.block, word.end.offset)
                        > (selection.end.block, selection.end.offset)
                    {
                        &word.end
                    } else {
                        &selection.end
                    };
                    if let Some(extended) = active.reading.probe.select_range(start, end) {
                        self.selection = Some(extended);
                    }
                }
                return Change::Partial;
            }
            if DONE.contains(x, y) {
                return self.exit();
            }
            if let Some(target) = self.active.as_ref().and_then(|active| {
                let reading = &active.reading;
                reading.link_at(x, y)
            }) {
                return match self.follow(target) {
                    Ok(()) => Change::Partial,
                    Err(error) => self.report(error),
                };
            }
            if reading_tools_gesture(x, y) {
                self.show(Screen::BookTools);
                return Change::Partial;
            }
        }
        let Some(active) = &mut self.active else {
            return Change::None;
        };
        let change = active.reading.input(self.controls.page_input(input));
        if change == Change::Partial {
            self.notice = None;
            self.clear_stale_selection();
            self.record_position();
        }
        change
    }

    fn inspect_illustration(&mut self, x: f32, y: f32) -> Result<bool> {
        let Some(book) = &self.active else {
            return Ok(false);
        };
        let Some(path) = book.reading.illustration_at(x, y) else {
            return Ok(false);
        };
        let image = book.reading.open_illustration(path)?;
        let view = FixedReading::inspect_image(image)?;
        self.show(Screen::Illustration);
        self.illustration = Some(view);
        Ok(true)
    }

    fn exit(&mut self) -> Change {
        match self.save() {
            Ok(()) => Change::Exit,
            Err(error) => self.report(error),
        }
    }

    fn handle_fixed(&mut self, input: Input) -> Change {
        if let Input::LongTap(x, y) = input
            && let Some(fixed) = &self.fixed
            && fixed.reading.supports_text()
        {
            return match fixed.reading.select_word(x, y) {
                Ok(Some(selection)) => {
                    self.pdf_selection = Some(selection);
                    self.pdf_result_region = None;
                    self.notice = None;
                    Change::Partial
                }
                Ok(None) => self.report("No selectable text at this point"),
                Err(error) => self.report(error),
            };
        }
        if let Input::Tap(x, y) = input {
            if self.ocr.is_some() && self.pdf_selection.is_none() && CANCEL_OCR.contains(x, y) {
                self.ocr = None;
                self.notice = Some("Text recognition cancelled".into());
                return Change::Partial;
            }
            if let Some(selection) = &mut self.pdf_selection {
                if CANCEL_MARK.contains(x, y) {
                    self.pdf_selection = None;
                } else if MARK.contains(x, y) {
                    if let Err(error) = self.save_pdf_highlight(None) {
                        return self.report(error);
                    }
                } else if NOTE.contains(x, y) {
                    self.editor = Editor::new(128);
                    self.show(Screen::Note);
                } else if LOOKUP.contains(x, y) {
                    if let Err(error) = self.look_up_selection() {
                        return self.report(error);
                    }
                } else if let Some(fixed) = &self.fixed {
                    fixed.reading.extend_selection(selection, x, y);
                }
                return Change::Partial;
            }
            if DONE.contains(x, y) {
                return self.exit();
            }
            if let Some(target) = self
                .fixed
                .as_ref()
                .and_then(|fixed| fixed.reading.link_at(x, y))
            {
                return match target {
                    PdfLinkTarget::Page(page) => match self.follow_fixed(page, None) {
                        Ok(()) => Change::Partial,
                        Err(error) => self.report(error),
                    },
                    PdfLinkTarget::External => {
                        self.report("This link opens a resource outside the PDF")
                    }
                    PdfLinkTarget::Unavailable => self.report("This PDF link is unavailable"),
                };
            }
            if reading_tools_gesture(x, y) {
                self.show(Screen::FixedTools);
                return Change::Partial;
            }
        }
        let Some(fixed) = &mut self.fixed else {
            return Change::None;
        };
        let page = fixed.reading.page();
        match fixed.reading.input(self.controls.page_input(input)) {
            Ok(true) => {
                self.notice = None;
                if fixed.reading.page() != page || fixed.reading.is_continuous() {
                    self.pdf_selection = None;
                    self.pdf_result_region = None;
                }
                self.record_position();
                Change::Partial
            }
            Ok(false) => Change::None,
            Err(error) => self.report(error),
        }
    }

    fn read_fixed_text(&mut self) -> Result<()> {
        let fixed = self.fixed.as_ref().ok_or("Open a document first")?;
        let page = fixed.reading.page();
        let text = fixed.reading.page_text(page)?;
        if text.text.trim().is_empty() {
            return Err("This page has no text. Use OCR to recognize a scan.".into());
        }
        let text_flow = TextFlow::open(
            page,
            fixed.reading.count(),
            &fixed.entry.title,
            &text.text,
            &self.fonts,
            device_settings(),
        )?;
        self.show(Screen::ReadFixedText);
        self.text_flow = Some(text_flow);
        Ok(())
    }

    fn finish_text_flow_update(&mut self, result: Result<bool>) -> Change {
        match result {
            Ok(false) => Change::None,
            Ok(true) => {
                self.notice = None;
                if let (Some(flow), Some(fixed)) = (&self.text_flow, &mut self.fixed)
                    && fixed.reading.page() != flow.document_page()
                {
                    if let Err(error) = fixed.reading.jump(flow.document_page()) {
                        self.show(Screen::Fixed);
                        return self.report(error);
                    }
                    self.record_position();
                }
                Change::Partial
            }
            Err(error) => self.report(error),
        }
    }

    fn open_scan_columns(&mut self) -> Result<()> {
        let fixed = self
            .fixed
            .as_mut()
            .ok_or("Open a supported document first")?;
        let raster = fixed.reading.ocr_raster()?;
        let columns =
            crate::scan_columns::detect_columns(&raster.pixels, raster.width, raster.height)
                .ok_or("No clear two-column layout on this page")?;
        let original = fixed.reading.preferences();
        fixed
            .reading
            .restore_view(column_view(original, columns[0]))?;
        self.show(Screen::ScanColumns);
        self.scan_columns = Some(ScanReading {
            original,
            columns,
            index: 0,
            turns: PageTurns::new(Instant::now()),
        });
        Ok(())
    }

    fn scan_column_input(&mut self, input: Input) -> Change {
        let (Some(scan), Some(fixed)) = (&mut self.scan_columns, &mut self.fixed) else {
            return Change::None;
        };
        let input = self.controls.page_input(input);
        let direction = match input {
            Input::Tap(x, y) if DONE.contains(x, y) => {
                if let Err(error) = fixed.reading.restore_view(scan.original) {
                    return self.report(error);
                }
                self.show(Screen::Fixed);
                return Change::Partial;
            }
            Input::PageForward => 1,
            Input::PageBack => -1,
            Input::Swipe { dx, dy, .. } if dx.abs() > dy.abs() => {
                if dx < 0.0 {
                    1
                } else {
                    -1
                }
            }
            Input::Tap(x, y) if y < PAGE_HEIGHT as f32 => {
                if x < WIDTH as f32 / 3.0 {
                    -1
                } else if x > WIDTH as f32 * 2.0 / 3.0 {
                    1
                } else {
                    return Change::None;
                }
            }
            _ => {
                return match fixed.reading.input(input) {
                    Ok(true) => Change::Partial,
                    Ok(false) => Change::None,
                    Err(error) => self.report(error),
                };
            }
        };
        scan.turns.pending.push(direction);
        self.step_scan_column(Instant::now())
    }

    fn step_scan_column(&mut self, now: Instant) -> Change {
        let (Some(scan), Some(fixed)) = (&mut self.scan_columns, &mut self.fixed) else {
            return Change::None;
        };
        let index = scan
            .turns
            .take_due(now)
            .into_iter()
            .fold(scan.index, |index, step| {
                index.saturating_add_signed(step).min(1)
            });
        if index == scan.index {
            return Change::None;
        }
        if let Err(error) = fixed
            .reading
            .restore_view(column_view(scan.original, scan.columns[index]))
        {
            return self.report(error);
        }
        scan.index = index;
        scan.turns.next_refresh = Instant::now() + PAGE_REFRESH_GAP;
        self.notice = None;
        Change::Partial
    }

    fn start_ocr(&mut self, language: Option<&str>) -> Result<()> {
        let assets = self
            .ocr_assets
            .as_ref()
            .ok_or("OCR requires the installed language pack")?;
        if language.is_none() && assets.languages().len() > 1 {
            self.show(Screen::OcrLanguage);
            return Ok(());
        }
        let language = language
            .unwrap_or_else(|| assets.default_language())
            .to_owned();
        let fixed = self.fixed.as_ref().ok_or("Open a PDF or DjVu first")?;
        let page = fixed.reading.page();
        let raster = fixed.reading.ocr_raster()?;
        let job = if language == assets.default_language() {
            OcrJob::start(assets, &raster.pixels, raster.width, raster.height)?
        } else {
            OcrJob::start_in_language(
                assets,
                &language,
                &raster.pixels,
                raster.width,
                raster.height,
            )?
        };
        self.show(Screen::Fixed);
        self.notice = Some(format!(
            "Recognizing page {} · {}",
            page + 1,
            ocr_language_name(&language)
        ));
        self.ocr = Some(ActiveOcr {
            page,
            language,
            job,
            deadline: Instant::now() + OCR_POLL,
        });
        Ok(())
    }

    fn poll_ocr(&mut self, now: Instant) -> Result<Change> {
        let Some(active) = &mut self.ocr else {
            return Ok(Change::None);
        };
        if now < active.deadline {
            return Ok(Change::None);
        }
        let Some(text) = active.job.poll()? else {
            active.deadline = now + OCR_POLL;
            return Ok(Change::None);
        };
        let active = self.ocr.take().ok_or("No active OCR job")?;
        let page = active.page;
        if text.text.trim().is_empty() {
            self.notice = Some(format!("Page {} has no recognized text", page + 1));
            return Ok(Change::Partial);
        }
        let fixed = self
            .fixed
            .as_mut()
            .ok_or("The document closed during OCR")?;
        fixed.reading.remember_ocr(page, &active.language, text)?;
        self.notice = Some(format!("Text recognized on page {}", page + 1));
        Ok(Change::Partial)
    }

    fn crop_input(&mut self, input: Input) -> Change {
        let Some((original, first)) = self.crop_dialog else {
            return Change::None;
        };
        let Some(fixed) = &self.fixed else {
            return Change::None;
        };
        let (next, save) = match input {
            Input::PageBack => (original, false),
            Input::Tap(x, y) if CANCEL_CROP.contains(x, y) => (original, false),
            Input::Tap(x, y) if RESET_CROP.contains(x, y) => (
                FixedPreferences {
                    crop: None,
                    offset_x: 0.0,
                    offset_y: 0.0,
                    ..original
                },
                true,
            ),
            Input::Tap(x, y) => {
                let region = if AUTO_CROP.contains(x, y) {
                    fixed.reading.auto_crop()
                } else {
                    let Some(second) = fixed.reading.crop_point(x, y) else {
                        return self.report("Tap within the page");
                    };
                    let Some(first) = first else {
                        self.crop_dialog = Some((original, Some(second)));
                        self.notice = None;
                        return Change::Partial;
                    };
                    Some(PdfTextRect {
                        x0: first.0.min(second.0),
                        x1: first.0.max(second.0),
                        y0: first.1.min(second.1),
                        y1: first.1.max(second.1),
                    })
                };
                let Some(region) = region.filter(|r| r.x1 - r.x0 >= 0.025 && r.y1 - r.y0 >= 0.025)
                else {
                    return self.report("Choose a larger area of the page");
                };
                (
                    FixedPreferences {
                        crop: Some(region),
                        turns: original.turns,
                        right_to_left: original.right_to_left,
                        width_fit: original.continuous,
                        continuous: original.continuous,
                        height_fit: original.height_fit,
                        scan_contrast: original.scan_contrast,
                        ..FixedPreferences::default()
                    },
                    true,
                )
            }
            _ => return Change::None,
        };
        if let Some(fixed) = &mut self.fixed
            && let Err(error) = fixed.reading.restore_view(next)
        {
            return self.report(error);
        }
        self.crop_dialog = None;
        self.show(Screen::Fixed);
        if save {
            self.record_position();
        }
        Change::Partial
    }

    fn clear_stale_selection(&mut self) {
        self.result_selection = None;
        if let (Some(active), Some(selection)) = (&self.active, &self.selection)
            && selection.start.chapter != active.reading.probe.chapter
        {
            self.selection = None;
        }
    }

    fn export_backup(&mut self) -> Result<()> {
        let bytes = self.library.backup_bytes()?;
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis();
        let filename = format!("reader-backup-{suffix}.json");
        let path = self.exports.join(&filename);
        with_storage(|| fs::create_dir_all(&self.exports))?;
        primitive_app::save(&path, &bytes)?;
        self.notice = Some(format!("Saved to Reader exports/{filename}"));
        Ok(())
    }

    fn preview_backup(&mut self) -> Result<()> {
        let bytes = read_import_file(&self.import_path, MAX_BACKUP_BYTES).map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                io::Error::new(
                    error.kind(),
                    "Copy a backup to reader/data/reader-import.json",
                )
            } else {
                error
            }
        })?;
        self.backup_import = Some(self.library.preview_backup(&bytes)?);
        self.show(Screen::RestoreBackup);
        Ok(())
    }

    fn restore_backup(&mut self) -> Result<()> {
        let import = self
            .backup_import
            .take()
            .ok_or("Preview the backup again")?;
        let summary = self.library.merge_backup(import)?;
        self.mark_dirty();
        self.show(Screen::ReadingData);
        self.save()?;
        self.notice = Some(format!(
            "Merged reading data for {} books",
            summary.new_books + summary.existing_books
        ));
        Ok(())
    }

    fn preview_koreader(&mut self) -> Result<()> {
        let book_path = &self
            .active
            .as_ref()
            .ok_or("Open a reflowable book first")?
            .reading
            .config
            .book;
        let path = self.import_path.with_file_name("koreader-import.json");
        let bytes =
            read_import_file(&path, koreader_import::MAX_EXPORT_BYTES as u64).map_err(|error| {
                if error.kind() == io::ErrorKind::NotFound {
                    io::Error::new(
                        error.kind(),
                        "Copy a KOReader JSON export to reader/data/koreader-import.json",
                    )
                } else {
                    error
                }
            })?;
        let book = with_storage(|| Book::open(book_path).map_err(io::Error::other))?;
        let export = koreader_import::parse(&bytes)?;
        let mut matches = export.documents.into_iter().filter(|export| {
            book.title.trim() == export.title.trim()
                && book
                    .author
                    .as_deref()
                    .zip(export.author.as_deref())
                    .is_none_or(|(book, export)| book.trim() == export.trim())
        });
        let matched = matches
            .next()
            .ok_or("This export contains no entry for the open book")?;
        if matches.next().is_some() {
            return Err("This export contains several entries for the open book".into());
        }
        if matched.entries.is_empty() || matched.entries.len() > 128 {
            return Err("Choose an export with 1 to 128 annotations for this book".into());
        }
        self.show(Screen::ImportAnnotations);
        let now = Instant::now();
        self.annotation_import = Some(AnnotationImport {
            entries: matched.entries,
            next: 0,
            resolver: None,
            matched: Vec::new(),
            unresolved: 0,
            deadline: Some(now + SEARCH_DELAY),
            refresh: now + SEARCH_REFRESH,
        });
        Ok(())
    }

    fn preview_koreader_position(&mut self) -> Result<()> {
        let path = self.import_path.with_file_name("koreader-progress.lua");
        let bytes = read_import_file(&path, koreader_sidecar::MAX_SIDECAR_BYTES as u64).map_err(
            |error| {
                if error.kind() == io::ErrorKind::NotFound {
                    io::Error::new(
                        error.kind(),
                        "Copy KOReader metadata to reader/data/koreader-progress.lua",
                    )
                } else {
                    error
                }
            },
        )?;
        let progress = koreader_sidecar::parse(&bytes)?;
        if self.fixed.is_some() {
            self.resolve_koreader_position(&progress)?;
        } else if self.active.is_none() {
            return Err("Open the matching book first".into());
        }
        self.show(Screen::ImportPosition);
        self.position_import = Some(progress);
        Ok(())
    }

    fn resolve_koreader_position(&self, progress: &SidecarProgress) -> Result<usize> {
        let fixed = self
            .fixed
            .as_ref()
            .ok_or("Open a fixed-page document first")?;
        let checksum = with_storage(|| {
            if self.library.identify(&fixed.entry)? != fixed.id {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "The document changed. Reopen it and preview the import again.",
                ));
            }
            let path = self.library.absolute_path(&fixed.entry.path)?;
            koreader_sidecar::partial_md5(&mut fs::File::open(path)?)
        })?;
        Ok(progress.resolve_fixed(checksum, fixed.reading.count())?)
    }

    fn apply_koreader_position(&mut self) -> Result<()> {
        let progress = self
            .position_import
            .as_ref()
            .ok_or("Preview the KOReader position first")?;
        let page = self.resolve_koreader_position(progress)?;
        self.follow_fixed(page, None)?;
        self.save()?;
        self.notice = Some(format!("KOReader position applied to page {}", page + 1));
        Ok(())
    }

    fn step_koreader(&mut self, now: Instant) -> Result<Change> {
        let import = self.annotation_import.as_mut().ok_or("No active import")?;
        let book_path = &self
            .active
            .as_ref()
            .ok_or("No open book")?
            .reading
            .config
            .book;
        with_storage(|| {
            let mut book = Book::open(book_path).map_err(io::Error::other)?;
            let started = Instant::now();
            for _ in 0..4 {
                let Some(entry) = import.entries.get(import.next) else {
                    break;
                };
                if import.resolver.is_none() {
                    match QuoteResolver::new(&entry.text) {
                        Ok(resolver) => import.resolver = Some(resolver),
                        Err(_) => {
                            import.unresolved += 1;
                            import.next += 1;
                            continue;
                        }
                    }
                }
                let resolver = import.resolver.as_mut().unwrap();
                if let Some(outcome) = resolver.step(&mut book).map_err(io::Error::other)? {
                    if let QuoteMatch::Unique(selection) = outcome {
                        import.matched.push(Highlight {
                            selection,
                            note: entry.note.clone(),
                        });
                    } else {
                        import.unresolved += 1;
                    }
                    import.resolver = None;
                    import.next += 1;
                }
                if started.elapsed() >= SEARCH_DELAY {
                    break;
                }
            }
            Ok(())
        })?;
        if import.next == import.entries.len() {
            import.deadline = None;
            return Ok(Change::Partial);
        }
        import.deadline = Some(Instant::now() + SEARCH_DELAY);
        if now >= import.refresh {
            import.refresh = now + SEARCH_REFRESH;
            Ok(Change::Partial)
        } else {
            Ok(Change::None)
        }
    }

    fn apply_koreader(&mut self) -> Result<()> {
        let import = self
            .annotation_import
            .take()
            .ok_or("Preview the export again")?;
        if import.deadline.is_some() {
            self.annotation_import = Some(import);
            return Err("Wait for the quote check to finish".into());
        }
        let active = self.active.as_ref().ok_or("No open book")?;
        let state = self
            .library
            .remember(&active.entry, active.id, &active.reading.probe.title);
        let mut added = 0;
        let mut updated = 0;
        for mark in import.matched {
            if let Some(existing) = state
                .highlights
                .iter_mut()
                .find(|existing| existing.selection == mark.selection)
            {
                if existing.note.is_none() && mark.note.is_some() {
                    existing.note = mark.note;
                    updated += 1;
                }
            } else if state.highlights.len() < 256 {
                state.highlights.push(mark);
                added += 1;
            }
        }
        if added > 0 || updated > 0 {
            self.mark_dirty();
        }
        self.show(Screen::Highlights);
        self.notice = Some(format!(
            "Added {added} highlights and updated {updated} notes"
        ));
        Ok(())
    }

    fn look_up_selection(&mut self) -> Result<()> {
        let word = self
            .pdf_selection
            .as_ref()
            .map(PdfSelection::text)
            .or_else(|| {
                self.selection
                    .as_ref()
                    .map(|selection| selection.text.as_str())
            })
            .ok_or("Select a word first")?;
        let definitions = with_storage(|| crate::dictionary::lookup(&self.dictionaries, word))?;
        self.definitions.clear();
        for definition in definitions {
            let title = format!("{} · {}", definition.word, definition.dictionary);
            let mut text = definition.text.trim();
            while !text.is_empty() && self.definitions.len() < 80 {
                let limit = text
                    .char_indices()
                    .nth(700)
                    .map_or(text.len(), |(index, _)| index);
                let end = if limit == text.len() {
                    limit
                } else {
                    text[..limit]
                        .rfind(char::is_whitespace)
                        .filter(|&index| index > 0)
                        .unwrap_or(limit)
                };
                self.definitions
                    .push((title.clone(), text[..end].to_owned()));
                text = text[end..].trim_start();
            }
        }
        self.show(Screen::Definition);
        Ok(())
    }

    fn definition_input(&mut self, input: Input) -> Change {
        if matches!(input, Input::Tap(x, y) if primitive_ui::back_button(WIDTH as i32).contains(x, y))
        {
            self.show(if self.fixed.is_some() {
                Screen::Fixed
            } else {
                Screen::Book
            });
            return Change::Partial;
        }
        let direction = match input {
            Input::PageBack => -1,
            Input::PageForward => 1,
            Input::Tap(x, y) if PREV.contains(x, y) => -1,
            Input::Tap(x, y) if NEXT.contains(x, y) => 1,
            _ => return Change::None,
        };
        if let Some(page) = self
            .list_page
            .checked_add_signed(direction)
            .filter(|&page| page < self.definitions.len())
        {
            self.list_page = page;
            Change::Partial
        } else {
            Change::None
        }
    }

    fn save_highlight(&mut self, note: Option<String>) -> Result<()> {
        let active = self.active.as_ref().ok_or("No open book")?;
        let selection = self.selection.as_ref().ok_or("Select some text first")?;
        if selection.text.len() > 8192 {
            return Err("Select a shorter passage".into());
        }
        let state = self
            .library
            .remember(&active.entry, active.id, &active.reading.probe.title);
        let note = note
            .map(|note| note.trim().to_owned())
            .filter(|note| !note.is_empty());
        if let Some(previous) = state
            .highlights
            .iter_mut()
            .find(|item| item.selection == *selection)
        {
            if note.is_some() {
                previous.note = note;
            }
        } else {
            if state.highlights.len() >= 256 {
                return Err("This book has reached its highlight limit".into());
            }
            state.highlights.push(Highlight {
                selection: selection.clone(),
                note,
            });
        }
        self.selection = None;
        self.mark_dirty();
        self.show(Screen::Book);
        self.notice = Some("Highlight saved".into());
        Ok(())
    }

    fn export_highlights(&mut self, format: annotation_export::Format) -> Result<()> {
        let id = self
            .fixed
            .as_ref()
            .map(|book| book.id)
            .or_else(|| self.active.as_ref().map(|book| book.id))
            .ok_or("No open book")?;
        let state = self.library.state(id).ok_or("No reading history")?;
        let quotes = state
            .highlights
            .iter()
            .map(|mark| (mark.selection.text.as_str(), mark.note.as_deref()))
            .chain(
                state
                    .pdf_highlights
                    .iter()
                    .map(|mark| (mark.text.as_str(), mark.note.as_deref())),
            );
        let contents = format.render(&state.title, quotes);
        with_storage(|| fs::create_dir_all(&self.exports))?;
        let filename = format!("{id:016x}.{}", format.extension());
        primitive_app::save(&self.exports.join(&filename), contents.as_bytes())?;
        self.notice = Some(format!("Exported to Reader data/exports/{filename}"));
        Ok(())
    }

    fn verify_pdf_edit_source(&self, fixed: &ActiveFixed) -> Result<Vec<u8>> {
        if fixed.entry.format != DocumentFormat::Pdf {
            return Err("Open a PDF first".into());
        }
        if fixed.password_supplied {
            return Err("PDF annotation interchange requires an unencrypted document".into());
        }
        if fixed.entry.size > pdf_annotations::MAX_INPUT_BYTES as u64 {
            return Err("This PDF exceeds Reader's annotation interchange size limit".into());
        }
        let path = with_storage(|| {
            if self.library.identify(&fixed.entry)? != fixed.id {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "The PDF changed. Reopen it first.",
                ));
            }
            self.library.absolute_path(&fixed.entry.path)
        })?;
        Ok(read_import_file(
            &path,
            pdf_annotations::MAX_INPUT_BYTES as u64,
        )?)
    }

    fn preview_pdf_annotations(&mut self) -> Result<()> {
        let fixed = self.fixed.as_ref().ok_or("Open a PDF first")?;
        let source = self.verify_pdf_edit_source(fixed)?;
        let annotations =
            pdf_annotations::import_highlights(&source, |page| fixed.reading.pdf_geometry(page))?;
        let existing = self
            .library
            .state(fixed.id)
            .map_or(&[][..], |book| book.pdf_highlights.as_slice());
        let pending: Vec<_> = annotations
            .into_iter()
            .filter_map(|annotation| {
                let mark = PdfHighlight {
                    page: annotation.page,
                    bounds: annotation.bounds,
                    text: annotation
                        .text
                        .filter(|quote| !quote.trim().is_empty())
                        .unwrap_or_else(|| {
                            format!(
                                "Highlight on {}",
                                fixed_page_label(&fixed.reading, annotation.page)
                            )
                        }),
                    note: annotation.note,
                };
                (mark.is_valid()
                    && !existing
                        .iter()
                        .any(|other| same_pdf_highlight(other, &mark)))
                .then_some(mark)
            })
            .collect();
        if pending.is_empty() {
            self.notice = Some("No new supported highlights in this PDF".into());
        } else {
            self.show(Screen::ImportPdfAnnotations);
            self.pdf_annotation_import = pending;
        }
        Ok(())
    }

    fn apply_pdf_annotations(&mut self) -> Result<()> {
        let id = self.fixed.as_ref().ok_or("Open a PDF first")?.id;
        let state = self.library.state(id).ok_or("No reading history")?;
        let mut staged = state.clone();
        let mut count = 0;
        for mark in &self.pdf_annotation_import {
            if !staged
                .pdf_highlights
                .iter()
                .any(|other| same_pdf_highlight(other, mark))
            {
                staged.add_pdf_highlight(mark.clone())?;
                count += 1;
            }
        }
        let fixed = self.fixed.as_ref().ok_or("Open a PDF first")?;
        self.library
            .remember(&fixed.entry, fixed.id, &fixed.entry.title)
            .pdf_highlights = staged.pdf_highlights;
        self.mark_dirty();
        self.show(Screen::Highlights);
        self.save()?;
        self.notice = Some(format!("Imported {count} PDF highlights"));
        Ok(())
    }

    fn export_annotated_pdf(&mut self) -> Result<()> {
        let fixed = self.fixed.as_ref().ok_or("Open a PDF first")?;
        let state = self.library.state(fixed.id).ok_or("No reading history")?;
        if state.pdf_highlights.is_empty() {
            return Err("Add a highlight before exporting a PDF copy".into());
        }
        let source = self.verify_pdf_edit_source(fixed)?;
        let contents = pdf_annotations::export_copy(&source, &state.pdf_highlights, |page| {
            fixed.reading.pdf_geometry(page)
        })?;
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis();
        let filename = format!("annotated-{:016x}-{suffix}.pdf", fixed.id);
        with_storage(|| fs::create_dir_all(&self.exports))?;
        primitive_app::save(&self.exports.join(&filename), &contents)?;
        self.notice = Some(format!("Saved to Reader exports/{filename}"));
        Ok(())
    }

    fn save_pdf_highlight(&mut self, note: Option<String>) -> Result<()> {
        let fixed = self.fixed.as_ref().ok_or("No open PDF")?;
        let selection = self
            .pdf_selection
            .as_ref()
            .ok_or("Select some text first")?;
        if selection.text().len() > 8192 {
            return Err("Select a shorter passage".into());
        }
        let bounds: Vec<_> = selection.bounds().collect();
        let note = note
            .map(|note| note.trim().to_owned())
            .filter(|note| !note.is_empty());
        let state = self
            .library
            .remember(&fixed.entry, fixed.id, &fixed.entry.title);
        if let Some(mark) = state
            .pdf_highlights
            .iter_mut()
            .find(|mark| mark.page == selection.page() && mark.bounds == bounds)
        {
            if note.is_some() {
                mark.note = note;
            }
        } else {
            state.add_pdf_highlight(PdfHighlight {
                page: selection.page(),
                bounds,
                text: selection.text().into(),
                note,
            })?;
        }
        self.pdf_selection = None;
        self.mark_dirty();
        self.show(Screen::Fixed);
        self.notice = Some("Highlight saved".into());
        Ok(())
    }
}

impl App for Reader {
    fn draw(&self, canvas: &mut Canvas) {
        if let (Some(scan), Some(fixed)) = (&self.scan_columns, &self.fixed) {
            let status = format!(
                "Column {} of 2 · Page {}",
                scan.index + 1,
                fixed.reading.page() + 1
            );
            fixed.reading.draw(
                canvas,
                Some(self.notice.as_deref().unwrap_or(&status)),
                &[],
                &[],
                self.page_paint(),
            );
            return;
        }
        if let Some(illustration) = &self.illustration {
            let notice = self.notice.as_deref().unwrap_or("Illustration");
            illustration.draw(canvas, Some(notice), &[], &[], self.page_paint());
            return;
        }
        if let Some(flow) = &self.text_flow {
            flow.draw(canvas, self.notice.as_deref(), self.controls.night_pages);
            return;
        }
        if self.screen == Screen::FixedCrop
            && let Some(fixed) = &self.fixed
        {
            let first = self.crop_dialog.and_then(|(_, first)| first);
            let point = first.map(|(x, y)| PdfTextRect {
                x0: (x - 0.009).max(0.0),
                y0: (y - 0.009).max(0.0),
                x1: (x + 0.009).min(1.0),
                y1: (y + 0.009).min(1.0),
            });
            let selection = point.map(|bounds| (fixed.reading.page(), bounds));
            fixed
                .reading
                .draw(canvas, None, &[], selection.as_slice(), self.page_paint());
            canvas.fill(
                Rect::new(
                    0,
                    PAGE_HEIGHT as i32,
                    WIDTH as i32,
                    (HEIGHT - PAGE_HEIGHT) as i32,
                ),
                255,
            );
            let hint = if first.is_some() {
                "Tap the opposite corner"
            } else {
                "Tap one corner of the area to keep"
            };
            canvas.text(
                Rect::new(28, PAGE_HEIGHT as i32 + 12, 630, 52),
                self.notice.as_deref().unwrap_or(hint),
                CAPTION,
            );
            draw_toolbar_button(canvas, AUTO_CROP, "Auto crop");
            draw_toolbar_button(canvas, RESET_CROP, "Reset");
            draw_toolbar_button(canvas, CANCEL_CROP, "Cancel");
            return;
        }
        if matches!(self.screen, Screen::Fixed | Screen::FixedTools)
            && let Some(fixed) = &self.fixed
        {
            let mut marks: Vec<_> = self
                .library
                .state(fixed.id)
                .into_iter()
                .flat_map(|state| &state.pdf_highlights)
                .filter(|mark| fixed.reading.visible_pages().any(|page| page == mark.page))
                .flat_map(|mark| mark.bounds.iter().map(|&bounds| (mark.page, bounds)))
                .collect();
            marks.extend(self.pdf_result_region);
            let selection: Vec<_> = self
                .pdf_selection
                .as_ref()
                .into_iter()
                .flat_map(|selection| selection.bounds().map(|bounds| (selection.page(), bounds)))
                .collect();
            fixed.reading.draw(
                canvas,
                self.notice.as_deref(),
                &marks,
                &selection,
                self.page_paint(),
            );
            if !selection.is_empty() {
                draw_selection_footer(canvas, self.notice.as_deref());
            }
            if self.screen == Screen::FixedTools {
                self.draw_reading_tools(canvas);
            } else if self.touch_locked {
                draw_locked_footer(canvas, self.notice.as_deref());
            } else if self.ocr.is_some() && selection.is_empty() {
                canvas.button(CANCEL_OCR, "Cancel OCR", false, false);
            }
            return;
        }
        if matches!(self.screen, Screen::Book | Screen::BookTools)
            && let Some(active) = &self.active
        {
            let reading = &active.reading;
            let mut highlights: Vec<_> = self
                .library
                .state(active.id)
                .into_iter()
                .flat_map(|state| &state.highlights)
                .flat_map(|highlight| reading.highlight_rects(&highlight.selection))
                .collect();
            if let Some(result) = &self.result_selection {
                highlights.extend(reading.highlight_rects(result));
            }
            let selection = self
                .selection
                .as_ref()
                .map_or_else(Vec::new, |selection| reading.highlight_rects(selection));
            reading.draw_page(
                canvas,
                self.selection.is_none().then_some("Done"),
                self.notice.as_deref(),
                &highlights,
                &selection,
                self.page_paint(),
            );
            if self.selection.is_some() {
                draw_selection_footer(canvas, self.notice.as_deref());
            }
            if self.screen == Screen::BookTools {
                self.draw_reading_tools(canvas);
            } else if self.touch_locked {
                draw_locked_footer(canvas, self.notice.as_deref());
            }
            return;
        }
        let title = match (self.screen, self.book_list.as_ref()) {
            (Screen::BookList, Some(BookList::Favorites)) => "Favorites",
            (Screen::BookList, Some(BookList::Status(status))) => status.label(),
            (Screen::BookList, Some(BookList::Collection(name))) => name,
            _ => self.screen.title(),
        };
        header(
            canvas,
            title,
            if self.screen == Screen::Library {
                "Done"
            } else {
                "Back"
            },
        );
        if self.screen == Screen::PageOverview {
            self.draw_overview(canvas);
            return;
        }
        if self.screen == Screen::Definition {
            if let Some((title, text)) = self.definitions.get(self.list_page) {
                canvas.text(
                    Rect::new(LIST_X, LIST_Y, WIDTH as i32 - 2 * LIST_X, 68),
                    title,
                    BODY.bold(),
                );
                canvas.text(
                    Rect::new(LIST_X, LIST_Y + 87, WIDTH as i32 - 2 * LIST_X, 1240),
                    text,
                    BODY.lines(28),
                );
            } else {
                canvas.text(
                    Rect::new(LIST_X, LIST_Y, WIDTH as i32 - 2 * LIST_X, 110),
                    "No matches in installed dictionaries",
                    BODY,
                );
            }
            if self.list_page > 0 {
                canvas.button(PREV, "Previous", false, false);
            }
            if self.list_page + 1 < self.definitions.len() {
                canvas.button(NEXT, "Next", false, false);
            }
            return;
        }
        if matches!(
            self.screen,
            Screen::Find
                | Screen::LibraryFind
                | Screen::CollectionName
                | Screen::ProfileName
                | Screen::Note
                | Screen::EditNote
                | Screen::GoToPage
                | Screen::PdfPassword
        ) {
            let (hint, action) = match self.screen {
                Screen::Note | Screen::EditNote => ("Note text", "Save"),
                Screen::LibraryFind => ("Title, author or filename", "Find"),
                Screen::CollectionName => ("Collection name", "Save"),
                Screen::ProfileName => ("Profile name", "Save"),
                Screen::PdfPassword => ("PDF password", "Open"),
                Screen::GoToPage if self.fixed.is_none() => {
                    ("Page, chapter:page, or % of chapter", "Go")
                }
                Screen::GoToPage => ("Page, printed label, or % of document", "Go"),
                _ => ("Search text", "Find"),
            };
            canvas.single_line_field(
                Rect::new(LIST_X, LIST_Y, WIDTH as i32 - 2 * LIST_X, 130),
                &self.editor,
                hint,
                self.screen == Screen::PdfPassword,
            );
            canvas.keyboard(self.editor.layer, action, None);
            if self.screen == Screen::Find {
                let label = if self.find_match_case {
                    "Match case on"
                } else {
                    "Match case off"
                };
                canvas.button(FIND_CASE, label, self.find_match_case, false);
            }
            if let Some(notice) = &self.notice {
                canvas.text(
                    Rect::new(LIST_X, LIST_Y + 158, WIDTH as i32 - 2 * LIST_X, 75),
                    notice,
                    CAPTION,
                );
            }
            return;
        }
        let items = self.items();
        let rows: Vec<_> = items
            .iter()
            .skip(self.list_page * ROWS_PER_PAGE)
            .take(ROWS_PER_PAGE)
            .map(|item| Row {
                title: &item.title,
                detail: item.detail.as_deref(),
                accessory: if matches!(item.action, Action::Heading) {
                    RowAccessory::None
                } else {
                    RowAccessory::Disclosure
                },
            })
            .collect();
        Rows {
            bounds: list_rect(rows.len()),
            rows: &rows,
        }
        .draw(canvas, None);
        if rows.is_empty() {
            let empty = match self.screen {
                Screen::Library => "Copy a book or document to Documents to start reading",
                Screen::Bookmarks | Screen::FixedBookmarks => "No bookmarks in this book",
                Screen::Highlights => "No highlights or notes in this book",
                Screen::Matches if self.search_at.is_some() && self.fixed.is_some() => {
                    "Searching pages"
                }
                Screen::Matches if self.search_at.is_some() => "Searching chapters",
                Screen::Matches => "No matches",
                _ => "No entries",
            };
            canvas.text(
                Rect::new(LIST_X, LIST_Y, WIDTH as i32 - 2 * LIST_X, 110),
                empty,
                BODY,
            );
        }
        if self.list_page > 0 {
            canvas.button(PREV, "Previous", false, false);
        }
        if (self.list_page + 1) * ROWS_PER_PAGE < items.len() {
            canvas.button(NEXT, "Next", false, false);
        }
        if let Some(notice) = &self.notice {
            canvas.text(
                Rect::new(310, HEIGHT as i32 - 104, WIDTH as i32 - 620, 80),
                notice,
                CAPTION,
            );
        }
    }

    fn press_point(&self, input: Input) -> Option<(f32, f32)> {
        match input {
            Input::Tap(x, y)
                if !self.touch_locked || UNLOCK_TOUCH.contains(x, y) || DONE.contains(x, y) =>
            {
                Some((x, y))
            }
            _ => None,
        }
    }

    fn input(&mut self, input: Input) -> Change {
        if matches!(input, Input::Sleep) && self.ocr.take().is_some() {
            self.notice = None;
        }
        if matches!(input, Input::Sleep | Input::Wake) {
            self.stats_awake = input == Input::Wake;
            if with_storage(|| Ok(())).is_ok() {
                self.update_stats_activity(Instant::now());
            } else {
                self.stats.discard_active();
            }
            if let Some(active) = &mut self.active {
                active.reading.input(input);
            }
            if let Some(fixed) = &mut self.fixed {
                fixed.reading.pause();
            }
            if let Some(illustration) = &mut self.illustration {
                illustration.pause();
            }
            if let Some(scan) = &mut self.scan_columns {
                scan.turns = PageTurns::new(Instant::now());
            }
            if let Some(flow) = &mut self.text_flow {
                flow.pause();
            }
            return Change::None;
        }
        if self.touch_locked {
            match input {
                Input::Tap(x, y) if UNLOCK_TOUCH.contains(x, y) => {
                    self.touch_locked = false;
                    self.notice = None;
                    return Change::Partial;
                }
                Input::Tap(x, y) if DONE.contains(x, y) => return self.exit(),
                Input::PageBack | Input::PageForward => {}
                _ => return Change::None,
            }
        }
        if self.screen == Screen::ReadFixedText {
            if matches!(input, Input::Tap(x, y) if DONE.contains(x, y)) {
                self.show(Screen::Fixed);
                return Change::Partial;
            }
            if let (Some(flow), Some(fixed)) = (&mut self.text_flow, &self.fixed) {
                let result = flow.input(self.controls.page_input(input), |page| {
                    fixed.reading.page_text(page)
                });
                return self.finish_text_flow_update(result);
            }
        }
        if self.screen == Screen::Book {
            return self.handle_book(input);
        }
        if self.screen == Screen::Illustration {
            if matches!(input, Input::Tap(x, y) if DONE.contains(x, y)) {
                self.show(Screen::Book);
                return Change::Partial;
            }
            if let Some(illustration) = &mut self.illustration {
                return match illustration.input(input) {
                    Ok(true) => Change::Partial,
                    Ok(false) => Change::None,
                    Err(error) => self.report(error),
                };
            }
        }
        if matches!(self.screen, Screen::BookTools | Screen::FixedTools) {
            return self.reading_tools_input(input);
        }
        if self.screen == Screen::Definition {
            return self.definition_input(input);
        }
        if self.screen == Screen::GoToPage {
            return self.handle_page_number(input);
        }
        if self.screen == Screen::PageOverview {
            return self.overview_input(input);
        }
        if matches!(self.screen, Screen::CollectionName | Screen::ProfileName) {
            return self.name_editor_input(input);
        }
        if self.screen == Screen::PdfPassword {
            return self.handle_pdf_password(input);
        }
        if self.screen == Screen::FixedCrop {
            return self.crop_input(input);
        }
        if self.screen == Screen::Fixed {
            return self.handle_fixed(input);
        }
        if self.screen == Screen::ScanColumns {
            return self.scan_column_input(input);
        }
        if matches!(
            self.screen,
            Screen::Find | Screen::LibraryFind | Screen::Note | Screen::EditNote
        ) {
            return self.handle_find(input);
        }
        let mut items = self.items();
        let count = items.len();
        let direction = match input {
            Input::PageBack => -1,
            Input::PageForward => 1,
            Input::Swipe { dy, dx, .. } if dy.abs() > dx.abs() => {
                if dy < 0.0 {
                    1
                } else {
                    -1
                }
            }
            Input::Tap(x, y) if primitive_ui::back_button(WIDTH as i32).contains(x, y) => {
                match self.screen {
                    Screen::Library => return self.exit(),
                    Screen::OrganizeLibrary => self.show(Screen::Library),
                    Screen::BookList => self.show(Screen::OrganizeLibrary),
                    Screen::RemoveCollection => self.show(Screen::BookList),
                    Screen::ReadingFonts => self.show(Screen::Typography),
                    Screen::ReadingProfiles => self.show(Screen::Typography),
                    Screen::RemoveProfiles => self.show(Screen::ReadingProfiles),
                    Screen::BookDetails => {
                        let back = self
                            .book_details
                            .as_ref()
                            .map_or(Screen::Library, |book| book.back);
                        self.show(back);
                    }
                    Screen::ReadingData => self.show(Screen::Library),
                    Screen::ReadingControls => self.show(if self.fixed.is_some() {
                        Screen::FixedTools
                    } else if self.active.is_some() {
                        Screen::BookTools
                    } else {
                        Screen::Library
                    }),
                    Screen::ClearOcr => self.show(Screen::ReadingData),
                    Screen::RestoreBackup => {
                        self.backup_import = None;
                        self.show(Screen::ReadingData);
                    }
                    Screen::FixedBookmarks | Screen::FixedContents | Screen::OcrLanguage => {
                        self.show(Screen::FixedTools)
                    }
                    Screen::Highlights if self.fixed.is_some() => self.show(Screen::FixedTools),
                    Screen::Matches => self.show(Screen::Find),
                    Screen::Annotation | Screen::ImportAnnotations => self.show(Screen::Highlights),
                    Screen::ImportPosition => self.show(Screen::ReadingControls),
                    Screen::ImportPdfAnnotations => self.show(Screen::Highlights),
                    Screen::ExportAnnotations => self.show(Screen::Highlights),
                    Screen::RemoveHighlight => self.show(Screen::Annotation),
                    _ => self.show(Screen::BookTools),
                }
                return Change::Partial;
            }
            Input::Tap(x, y) if PREV.contains(x, y) => -1,
            Input::Tap(x, y) if NEXT.contains(x, y) => 1,
            Input::LongTap(x, y) if matches!(self.screen, Screen::Library | Screen::BookList) => {
                let Some(index) = list_item_at(self.list_page, count, x, y) else {
                    return Change::None;
                };
                let entry = match items.swap_remove(index).action {
                    Action::Open(entry) => Ok(entry),
                    Action::OpenTracked(id) => self.locate_tracked(id),
                    _ => return Change::None,
                };
                return match entry.and_then(|entry| self.organize_book(entry, self.screen)) {
                    Ok(()) => Change::Partial,
                    Err(error) => self.report(error),
                };
            }
            Input::Tap(x, y) => {
                if let Some(index) = list_item_at(self.list_page, count, x, y) {
                    let action = items.swap_remove(index).action;
                    return match self.activate(action) {
                        Ok(()) => Change::Partial,
                        Err(error) => self.report(error),
                    };
                }
                return Change::None;
            }
            _ => return Change::None,
        };
        let Some(page) = self
            .list_page
            .checked_add_signed(direction)
            .filter(|page| page * ROWS_PER_PAGE < count)
        else {
            return Change::None;
        };
        self.list_page = page;
        Change::Partial
    }

    fn deadline(&self) -> Option<Instant> {
        let page = self
            .active
            .as_ref()
            .filter(|_| self.screen == Screen::Book)
            .and_then(|active| active.reading.deadline());
        let fixed_page = self
            .fixed
            .as_ref()
            .filter(|_| self.screen == Screen::Fixed)
            .and_then(|fixed| fixed.reading.deadline());
        page.into_iter()
            .chain(fixed_page)
            .chain(self.save_at)
            .chain(self.search_at)
            .chain(
                self.annotation_import
                    .as_ref()
                    .and_then(|import| import.deadline),
            )
            .chain(self.ocr.as_ref().map(|job| job.deadline))
            .chain(self.text_flow.as_ref().and_then(TextFlow::deadline))
            .chain(self.stats.deadline())
            .chain(
                self.scan_columns
                    .as_ref()
                    .and_then(|scan| scan.turns.deadline()),
            )
            .min()
    }

    fn tick(&mut self, now: Instant) -> Change {
        if self.save_at.is_some_and(|at| at <= now) {
            self.save_at = None;
            if let Err(error) = self.save_library()
                && error.kind() != io::ErrorKind::WouldBlock
            {
                return self.report(error);
            }
        }
        if self.stats.deadline().is_some_and(|at| at <= now)
            && let Err(error) = self.stats.checkpoint(&self.stats_path, now, false)
            && error.kind() != io::ErrorKind::WouldBlock
        {
            return self.report(error);
        }
        if self
            .scan_columns
            .as_ref()
            .and_then(|scan| scan.turns.deadline())
            .is_some_and(|at| at <= now)
        {
            return self.step_scan_column(now);
        }
        if self
            .annotation_import
            .as_ref()
            .and_then(|import| import.deadline)
            .is_some_and(|at| at <= now)
        {
            return match self.step_koreader(now) {
                Ok(change) => change,
                Err(error) => {
                    self.show(Screen::Highlights);
                    self.report(error)
                }
            };
        }
        if self.ocr.as_ref().is_some_and(|job| job.deadline <= now) {
            match self.poll_ocr(now) {
                Ok(Change::None) => {}
                Ok(change) => return change,
                Err(error) => {
                    self.ocr = None;
                    return self.report(error);
                }
            }
        }
        if self.search_at.is_some_and(|at| at <= now) {
            return match self.step_search(now) {
                Ok(change) => change,
                Err(error) => {
                    self.search_at = None;
                    self.report(error)
                }
            };
        }
        if let (Some(flow), Some(fixed)) = (&mut self.text_flow, &self.fixed) {
            let result = flow.tick(now, |page| fixed.reading.page_text(page));
            return self.finish_text_flow_update(result);
        }
        if self.screen == Screen::Book
            && let Some(active) = &mut self.active
        {
            let change = active.reading.tick(now);
            if change == Change::Partial {
                self.notice = None;
                self.clear_stale_selection();
                self.record_position();
            }
            return change;
        }
        if self.screen == Screen::Fixed
            && let Some(fixed) = &mut self.fixed
        {
            return match fixed.reading.tick(now) {
                Ok(true) => {
                    self.notice = None;
                    self.pdf_selection = None;
                    self.pdf_result_region = None;
                    self.record_position();
                    Change::Partial
                }
                Ok(false) => Change::None,
                Err(error) => self.report(error),
            };
        }
        Change::None
    }

    fn save(&mut self) -> io::Result<()> {
        self.save_library()?;
        self.stats
            .checkpoint(&self.stats_path, Instant::now(), true)?;
        Ok(())
    }

    fn storage_returned(&mut self) -> io::Result<()> {
        self.stats.discard_active();
        self.update_stats_activity(Instant::now());
        self.ocr_assets = self
            .ocr_root
            .parent()
            .and_then(Path::parent)
            .and_then(OcrAssets::discover);
        if let Some(active) = &mut self.active {
            active.reading.storage_returned()?;
        }
        if let Some(fixed) = &mut self.fixed {
            fixed.reading.pause();
        }
        self.save()?;
        with_storage(|| self.library.scan())?;
        if self.screen == Screen::Library {
            self.list_page = self
                .list_page
                .min(self.items().len().saturating_sub(1) / ROWS_PER_PAGE);
        }
        Ok(())
    }
}

fn read_import_file(path: &Path, limit: u64) -> io::Result<Vec<u8>> {
    with_storage(|| {
        let metadata = fs::symlink_metadata(path)?;
        if !metadata.is_file() || metadata.len() > limit {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Import file is unavailable or too large",
            ));
        }
        let mut bytes = Vec::new();
        fs::File::open(path)?
            .take(limit + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > limit {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Import file is too large",
            ));
        }
        Ok(bytes)
    })
}

fn position(reading: &Reading) -> ReadingPosition {
    ReadingPosition {
        chapter: reading.chapter,
        page: reading.page,
        anchor: reading.anchor(),
    }
}

fn fixed_position(reading: &FixedReading) -> ReadingPosition {
    ReadingPosition {
        chapter: 0,
        page: reading.page(),
        anchor: None,
    }
}

fn fixed_page_label(reading: &FixedReading, page: usize) -> String {
    let physical = (page + 1).to_string();
    match reading.page_label(page) {
        Some(label) if label != physical => format!("{label} · Page {physical}"),
        _ => format!("Page {physical}"),
    }
}

fn page_at_percent(percent: &str, count: usize) -> Result<usize> {
    let percent = percent
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|value| (0.0..=100.0).contains(value))
        .ok_or("Enter a percentage from 0% to 100%")?;
    let last = count.saturating_sub(1);
    Ok(((last as f64 * percent / 100.0).round() as usize).min(last))
}

fn same_pdf_highlight(left: &PdfHighlight, right: &PdfHighlight) -> bool {
    left.page == right.page
        && left.bounds.len() == right.bounds.len()
        && left.bounds.iter().zip(&right.bounds).all(|(a, b)| {
            [a.x0 - b.x0, a.y0 - b.y0, a.x1 - b.x1, a.y1 - b.y1]
                .into_iter()
                .all(|delta| delta.abs() <= 0.000_01)
        })
}

fn column_view(original: FixedPreferences, bounds: PdfTextRect) -> FixedPreferences {
    FixedPreferences {
        width_fit: true,
        crop: Some(bounds),
        scan_contrast: original.scan_contrast,
        ..FixedPreferences::default()
    }
}

fn list_rect(rows: usize) -> Rect {
    Rect::new(
        LIST_X,
        LIST_Y,
        WIDTH as i32 - 2 * LIST_X,
        rows as i32 * ROW_HEIGHT,
    )
}

fn overview_tile(index: usize) -> Rect {
    let width = (WIDTH as i32 - 2 * LIST_X - 26) / 2;
    Rect::new(
        LIST_X + index as i32 % 2 * (width + 26),
        LIST_Y + 90 + index as i32 / 2 * 452,
        width,
        420,
    )
}

fn list_item_at(page: usize, count: usize, x: f32, y: f32) -> Option<usize> {
    let first = page * ROWS_PER_PAGE;
    let shown = count.saturating_sub(first).min(ROWS_PER_PAGE);
    list_rect(shown)
        .contains(x, y)
        .then(|| first + ((y as i32 - LIST_Y) / ROW_HEIGHT) as usize)
        .filter(|&index| index < count)
}

fn tracked_book_item(book: &BookState) -> Item {
    Item::new(&book.title, Action::OpenTracked(book.id)).detail(
        book.author
            .as_deref()
            .map_or_else(|| book.path.to_string_lossy().into_owned(), str::to_owned),
    )
}

fn reading_time(seconds: u64) -> String {
    match seconds / 60 {
        0 if seconds == 0 => "No recorded reading time".into(),
        0 => "Less than 1 min".into(),
        minutes if minutes < 60 => format!("{minutes} min"),
        minutes => format!("{} hr {} min", minutes / 60, minutes % 60),
    }
}

fn ocr_language_name(code: &str) -> &str {
    match code {
        "eng" => "English",
        "vie" => "Vietnamese",
        "fra" => "French",
        "deu" => "German",
        "spa" => "Spanish",
        "jpn" => "Japanese",
        "chi_sim" => "Chinese (Simplified)",
        "chi_tra" => "Chinese (Traditional)",
        _ => code,
    }
}

fn reading_tools_gesture(x: f32, y: f32) -> bool {
    (WIDTH as f32 / 3.0..=WIDTH as f32 * 2.0 / 3.0).contains(&x)
        && (0.0..PAGE_HEIGHT as f32 * 0.55).contains(&y)
}

struct ToolbarLayout {
    columns: usize,
    row_height: i32,
    top: i32,
    primary_y: i32,
    large: bool,
}

impl ToolbarLayout {
    fn new(count: usize, large: bool) -> Self {
        let count = count.max(1);
        let max_columns = if large { 6 } else { 7 };
        let rows = count.div_ceil(max_columns);
        let columns = count.min(max_columns);
        let row_height = if large { 76 } else { 61 };
        let primary_y = HEIGHT as i32 - BOTTOM_BAR_HEIGHT - if large { 14 } else { 0 };
        Self {
            columns,
            row_height,
            top: primary_y - (rows as i32 - 1) * row_height,
            primary_y,
            large,
        }
    }

    fn rect(&self, index: usize) -> Rect {
        let step = (WIDTH as i32 - 26) / self.columns as i32;
        Rect::new(
            18 + (index % self.columns) as i32 * step,
            self.primary_y + 16 - (index / self.columns) as i32 * self.row_height,
            step - 10,
            self.row_height - 1,
        )
    }

    fn draw(&self, canvas: &mut Canvas, rect: Rect, title: &str) {
        if !self.large {
            draw_toolbar_button(canvas, rect, title);
            return;
        }
        if canvas.control_pressed(rect, false) {
            canvas.fill(rect, 218);
        }
        canvas.text(
            Rect::new(rect.x, rect.y + 8, rect.w, rect.h - 8),
            title,
            TextStyle::new(34).bold().center(),
        );
    }
}

fn draw_locked_footer(canvas: &mut Canvas, notice: Option<&str>) {
    let top = PAGE_HEIGHT as i32;
    canvas.fill(Rect::new(0, top, WIDTH as i32, HEIGHT as i32 - top), 255);
    canvas.text(
        Rect::new(38, top + 12, UNLOCK_TOUCH.x - 60, 48),
        notice.unwrap_or("Touch locked. Use the physical page buttons."),
        CAPTION,
    );
    canvas.button(UNLOCK_TOUCH, "Unlock touch", true, false);
    canvas.button(DONE, "Done", false, false);
}

fn draw_toolbar_button(canvas: &mut Canvas, rect: Rect, title: &str) {
    if canvas.control_pressed(rect, false) {
        canvas.fill(rect, 218);
    }
    canvas.text(
        Rect::new(rect.x, rect.y + 10, rect.w, rect.h - 10),
        title,
        CAPTION.center().ink(0).bold(),
    );
}

fn draw_selection_footer(canvas: &mut Canvas, notice: Option<&str>) {
    canvas.text(
        Rect::new(29, PAGE_HEIGHT as i32 + 8, 435, 56),
        notice.unwrap_or("Tap a word to extend"),
        CAPTION,
    );
    canvas.button(MARK, "Highlight", false, false);
    canvas.button(NOTE, "Note", false, false);
    canvas.button(LOOKUP, "Look up", false, false);
    canvas.button(CANCEL_MARK, "Cancel", false, false);
}

fn annotation_items<'a>(marks: impl Iterator<Item = (&'a str, Option<&'a str>)>) -> Vec<Item> {
    let mut marks = marks.peekable();
    if marks.peek().is_none() {
        return Vec::new();
    }
    let mut items = vec![Item::new(
        "Export annotations",
        Action::Show(Screen::ExportAnnotations),
    )];
    items.extend(marks.enumerate().map(|(index, (text, note))| {
        let snippet = text
            .split_whitespace()
            .take(22)
            .collect::<Vec<_>>()
            .join(" ");
        let item = Item::new(snippet, Action::ManageHighlight(index));
        if let Some(note) = note {
            item.detail(note)
        } else {
            item
        }
    }));
    items
}

fn device_settings() -> Settings {
    Settings {
        width: WIDTH,
        height: PAGE_HEIGHT,
        ..Settings::default()
    }
}

fn text_items(settings: Settings, scroll: bool) -> Vec<Item> {
    let size = format!("Current size {} px", settings.font_size.round());
    let margin = format!("Current margin {} px", settings.margin);
    let spacing = format!("Current spacing {:.2}×", settings.line_height);
    let (alignment, next_alignment) = match settings.alignment {
        TextAlignment::Publisher => ("Publisher", TextAlignment::Start),
        TextAlignment::Start => ("Start", TextAlignment::Center),
        TextAlignment::Center => ("Centered", TextAlignment::Justify),
        TextAlignment::Justify => ("Justified", TextAlignment::Publisher),
    };
    vec![
        Item::new(
            if scroll {
                "Switch to pages"
            } else {
                "Switch to continuous scroll"
            },
            Action::ReadingMode(!scroll),
        ),
        Item::new(
            "Larger text",
            Action::Text(Settings {
                font_size: (settings.font_size + 4.0).min(100.0),
                ..settings
            }),
        )
        .detail(&size),
        Item::new(
            "Smaller text",
            Action::Text(Settings {
                font_size: (settings.font_size - 4.0).max(20.0),
                ..settings
            }),
        ),
        Item::new(
            "Wider margins",
            Action::Text(Settings {
                margin: (settings.margin + 12).min(150),
                ..settings
            }),
        )
        .detail(&margin),
        Item::new(
            "Narrower margins",
            Action::Text(Settings {
                margin: settings.margin.saturating_sub(12),
                ..settings
            }),
        ),
        Item::new(
            "More line spacing",
            Action::Text(Settings {
                line_height: (settings.line_height + 0.1).min(2.2),
                ..settings
            }),
        )
        .detail(&spacing),
        Item::new(
            "Less line spacing",
            Action::Text(Settings {
                line_height: (settings.line_height - 0.1).max(1.0),
                ..settings
            }),
        ),
        Item::new(
            "Text alignment",
            Action::Text(Settings {
                alignment: next_alignment,
                ..settings
            }),
        )
        .detail(alignment),
        Item::new(
            if settings.bold_text {
                "Use regular body text"
            } else {
                "Use bold body text"
            },
            Action::Text(Settings {
                bold_text: !settings.bold_text,
                ..settings
            }),
        ),
        Item::new(
            if settings.override_fonts {
                "Use publisher fonts"
            } else {
                "Use selected reading font"
            },
            Action::Text(Settings {
                override_fonts: !settings.override_fonts,
                ..settings
            }),
        ),
        Item::new("Reset text settings", Action::ResetText),
    ]
}

pub fn run_reader(
    home: &Path,
    documents: &Path,
    fonts: Vec<PathBuf>,
    file: Option<&Path>,
) -> Result<()> {
    djvu_rs::render_cache::set_budget(crate::djvu::MAX_RENDER_CACHE_BYTES);
    let data = home.join("data");
    with_storage(|| fs::create_dir_all(&data))?;
    let mut app = Reader::open(documents, &data.join("library.json"), fonts)?;
    if let Some(file) = file
        && let Err(error) = app.open_path(file)
    {
        app.report(error);
    }
    primitive_app::run(app, home).map_err(Into::into)
}

pub fn run_probe(engine: Engine, book: PathBuf, fonts: Vec<PathBuf>, home: &Path) -> Result<()> {
    let reading = Reading::open(BookConfig {
        engine,
        book,
        fonts,
        settings: device_settings(),
        scroll: false,
    })?;
    primitive_app::run(reading, home).map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};
    use std::sync::atomic::{AtomicUsize, Ordering};

    static FIXTURE_ID: AtomicUsize = AtomicUsize::new(0);

    struct Fixture {
        root: PathBuf,
        book: PathBuf,
        state: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let suffix = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let id = FIXTURE_ID.fetch_add(1, Ordering::Relaxed);
            let root = std::env::temp_dir()
                .join(format!("reader-test-{}-{suffix}-{id}", std::process::id()));
            fs::create_dir(&root).unwrap();
            let book = root.join("Field Notes.epub");
            fs::write(
                &book,
                include_bytes!("../tests/fixtures/field-notes-epub3.epub"),
            )
            .unwrap();
            let state = root.join("library.json");
            Self { root, book, state }
        }

        fn app(&self) -> Reader {
            let fonts = [crate::test_fixture("fonts/Silkscreen-Regular.ttf")];
            Reader::open(&self.root, &self.state, fonts.to_vec()).unwrap()
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn organizes_an_unopened_book_and_preserves_library_collections() {
        let fixture = Fixture::new();
        let mut app = fixture.app();
        app.activate(Action::Show(Screen::OrganizeLibrary)).unwrap();
        app.activate(Action::NameCollection(None)).unwrap();
        app.save_collection_name("Weekend").unwrap();
        assert_eq!(app.screen, Screen::BookList);
        app.show(Screen::Library);
        let index = app
            .items()
            .iter()
            .position(|item| matches!(item.action, Action::Open(_)))
            .unwrap();
        assert_eq!(
            app.input(Input::LongTap(
                85.0,
                (LIST_Y + ROW_HEIGHT * index as i32 + 33) as f32
            )),
            Change::Partial
        );
        assert_eq!(app.screen, Screen::BookDetails);
        app.activate(Action::Favorite(true)).unwrap();
        app.activate(Action::SetStatus(ReadingStatus::OnHold))
            .unwrap();
        app.activate(Action::CollectionMembership("Weekend".into(), true))
            .unwrap();
        assert!(app.library.recently_read().next().is_none());
        app.save().unwrap();

        let mut reopened = fixture.app();
        for list in [
            BookList::Favorites,
            BookList::Status(ReadingStatus::OnHold),
            BookList::Collection("Weekend".into()),
        ] {
            reopened.activate(Action::ListBooks(list)).unwrap();
            assert!(matches!(reopened.items()[0].action, Action::OpenTracked(_)));
        }
        reopened
            .activate(Action::NameCollection(Some("Weekend".into())))
            .unwrap();
        reopened.save_collection_name("Evenings").unwrap();
        assert!(matches!(reopened.items()[0].action, Action::OpenTracked(_)));
        reopened.activate(Action::RemoveCollection).unwrap();
        assert!(reopened.library.collections().is_empty());
        assert!(reopened.library.favorites().next().is_some());
    }

    #[test]
    fn night_pages_and_larger_controls_persist_without_inverting_the_footer() {
        let fixture = Fixture::new();
        let mut app = fixture.app();
        app.open_path(&fixture.book).unwrap();
        let page = GrayImage {
            width: WIDTH,
            height: PAGE_HEIGHT,
            pixels: vec![255; (WIDTH * PAGE_HEIGHT) as usize],
        };
        let (light_area, light) = app.page_paint().apply(page.clone());
        let controls = ReadingControls {
            night_pages: true,
            left_touch: true,
            large_toolbar: true,
        };
        app.activate(Action::SetReadingControls(controls)).unwrap();
        let (night_area, night) = app.page_paint().apply(page);
        assert_eq!((light.pixels[0], night.pixels[0]), (255, 0));
        let content = Rect::new(0, 0, WIDTH as i32, PAGE_HEIGHT as i32);
        assert_eq!(light_area, content);
        assert_eq!(night_area, content);
        let mut reopened = fixture.app();
        assert_eq!(reopened.controls, controls);
        assert_eq!(
            reopened.controls.page_input(Input::Tap(100.0, 700.0)),
            Input::Tap(WIDTH as f32 - 100.0, 700.0)
        );
        reopened.open_path(&fixture.book).unwrap();
        reopened.show(Screen::BookTools);
        let layout = ToolbarLayout::new(reopened.reading_tools().len(), true);
        let contents = layout.rect(0);
        reopened.input(Input::Tap(
            (contents.x + 30) as f32,
            (contents.y + 30) as f32,
        ));
        assert_eq!(reopened.screen, Screen::Contents);
    }

    #[test]
    fn touch_lock_preserves_page_buttons_unlock_and_exit() {
        let fixture = Fixture::new();
        let long = fixture.root.join("Long Chapter.epub");
        fs::write(&long, include_bytes!("../tests/fixtures/long-chapter.epub")).unwrap();
        let mut app = fixture.app();
        app.open_path(&long).unwrap();
        app.activate(Action::LockTouch).unwrap();
        assert_eq!(
            app.input(Input::Tap(WIDTH as f32 / 2.0, 23.0)),
            Change::None
        );
        assert_eq!(
            app.input(Input::Swipe {
                dx: 0.0,
                dy: -500.0,
                start_y: 900.0
            }),
            Change::None
        );
        let before = position(&app.active.as_ref().unwrap().reading);
        app.input(Input::PageForward);
        if let Some(due) = app.active.as_ref().unwrap().reading.deadline() {
            app.tick(due);
        }
        assert_ne!(position(&app.active.as_ref().unwrap().reading), before);
        assert_eq!(
            app.input(Input::Tap(
                (UNLOCK_TOUCH.x + 15) as f32,
                (UNLOCK_TOUCH.y + 15) as f32
            )),
            Change::Partial
        );
        assert!(!app.touch_locked);
        assert_eq!(
            app.input(Input::Tap(WIDTH as f32 / 2.0, 23.0)),
            Change::Partial
        );
        app.activate(Action::LockTouch).unwrap();
        assert_eq!(
            app.input(Input::Tap((DONE.x + 15) as f32, (DONE.y + 15) as f32)),
            Change::Exit
        );
    }

    #[test]
    fn reads_with_a_per_book_font_and_recovers_if_it_is_removed() {
        let fixture = Fixture::new();
        let mut app = fixture.app();
        app.open_path(&fixture.book).unwrap();
        app.activate(Action::ShowFonts).unwrap();
        let path = app.fonts_directory.join("Custom.ttf");
        fs::copy(&app.fonts[0], &path).unwrap();
        fs::write(app.fonts_directory.join("Corrupt.ttf"), b"invalid").unwrap();
        app.activate(Action::ShowFonts).unwrap();
        assert_eq!(app.available_fonts, ["Corrupt.ttf", "Custom.ttf"]);
        assert!(
            app.activate(Action::SetFont(Some("Corrupt.ttf".into())))
                .is_err()
        );
        let passage = position(&app.active.as_ref().unwrap().reading);
        app.activate(Action::SetFont(Some("Custom.ttf".into())))
            .unwrap();
        let reading = &app.active.as_ref().unwrap().reading;
        assert_eq!(reading.config.fonts[0], path);
        assert_eq!(position(reading).anchor, passage.anchor);
        let settings = Settings {
            font_size: 58.0,
            ..reading.config.settings
        };
        app.activate(Action::Text(settings)).unwrap();
        app.save().unwrap();

        let mut reopened = fixture.app();
        reopened.activate(Action::Continue).unwrap();
        assert_eq!(
            reopened.active.as_ref().unwrap().reading.config.fonts[0],
            path
        );
        fs::remove_file(path).unwrap();
        let mut recovered = fixture.app();
        recovered.activate(Action::Continue).unwrap();
        let reading = &recovered.active.as_ref().unwrap().reading;
        assert_eq!(reading.config.fonts, recovered.fonts);
        assert_eq!(reading.config.settings.font_size, 58.0);
    }

    #[test]
    fn reuses_a_typography_profile_and_resets_the_book_without_moving() {
        let fixture = Fixture::new();
        let mut app = fixture.app();
        app.open_path(&fixture.book).unwrap();
        app.activate(Action::Text(Settings {
            font_size: 58.0,
            ..device_settings()
        }))
        .unwrap();
        app.activate(Action::ReadingMode(true)).unwrap();
        app.save_reading_profile("Large and scrolling").unwrap();
        assert!(app.save_reading_profile("LARGE AND SCROLLING").is_err());
        let original = position(&app.active.as_ref().unwrap().reading);
        app.activate(Action::ResetText).unwrap();
        let reset = &app.active.as_ref().unwrap().reading;
        assert_eq!(reset.config.settings.font_size, device_settings().font_size);
        assert!(!reset.config.scroll);
        assert_eq!(position(reset).anchor, original.anchor);
        app.save().unwrap();

        let mut reopened = fixture.app();
        reopened.open_path(&fixture.book).unwrap();
        reopened.activate(Action::ApplyProfile(0)).unwrap();
        let reading = &reopened.active.as_ref().unwrap().reading;
        assert_eq!(reading.config.settings.font_size, 58.0);
        assert!(reading.config.scroll);
        assert_eq!(position(reading).anchor, original.anchor);
        reopened.activate(Action::RemoveProfile(0)).unwrap();
        assert!(fixture.app().profiles.is_empty());
    }

    #[test]
    fn previews_and_applies_checked_koreader_pdf_position() {
        let fixture = Fixture::new();
        let pdf = fixture.root.join("Reading sample.pdf");
        fs::write(&pdf, crate::fixed::tests::document()).unwrap();
        let mut app = fixture.app();
        app.open_path(&pdf).unwrap();
        let hash =
            md5::Digest(koreader_sidecar::partial_md5(&mut fs::File::open(pdf).unwrap()).unwrap());
        let sidecar = fixture.root.join("koreader-progress.lua");
        fs::write(&sidecar, "return {last_page = 2, doc_pages = 2, partial_md5_checksum = '00000000000000000000000000000000'}").unwrap();
        assert!(app.activate(Action::PreviewKoReaderPosition).is_err());
        fs::write(
            &sidecar,
            format!("return {{last_page = 2, doc_pages = 2, partial_md5_checksum = '{hash:x}'}}"),
        )
        .unwrap();
        app.activate(Action::PreviewKoReaderPosition).unwrap();
        assert_eq!(app.screen, Screen::ImportPosition);
        assert_eq!(app.fixed.as_ref().unwrap().reading.page(), 0);
        app.activate(Action::ApplyKoReaderPosition).unwrap();
        assert_eq!(app.fixed.as_ref().unwrap().reading.page(), 1);
        let mut restored = fixture.app();
        restored.activate(Action::Continue).unwrap();
        assert_eq!(restored.fixed.as_ref().unwrap().reading.page(), 1);
        app.activate(Action::ReturnToLocation).unwrap();
        assert_eq!(app.fixed.as_ref().unwrap().reading.page(), 0);
    }

    #[test]
    fn distinguishes_printed_pdf_labels_from_physical_numbers() {
        let fixture = Fixture::new();
        let pdf = fixture.root.join("Printed labels.pdf");
        fs::write(&pdf, crate::fixed::tests::pdf_objects(&[
            "<< /Type /Catalog /Pages 2 0 R /PageLabels << /Nums [0 << /S /r >> 1 << /S /D >>] >> >>",
            "<< /Type /Pages /Count 2 /Kids [3 0 R 4 0 R] >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 120 80] >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 120 80] >>",
        ])).unwrap();
        let mut app = fixture.app();
        app.open_path(&pdf).unwrap();
        app.jump_fixed_number("label 1").unwrap();
        let reading = &app.fixed.as_ref().unwrap().reading;
        assert_eq!(reading.page(), 1);
        assert_eq!(fixed_page_label(reading, reading.page()), "1 · Page 2");
        app.jump_fixed_number("i").unwrap();
        assert_eq!(app.fixed.as_ref().unwrap().reading.page(), 0);
        app.jump_fixed_number("2").unwrap();
        app.jump_fixed_number("1").unwrap();
        assert_eq!(app.fixed.as_ref().unwrap().reading.page(), 0);
        assert!(app.jump_fixed_number("0").is_err());
        app.jump_fixed_number("100%").unwrap();
        assert_eq!(app.fixed.as_ref().unwrap().reading.page(), 1);
        assert!(app.jump_fixed_number("101%").is_err());
    }

    #[test]
    fn reads_two_scan_columns_without_saving_a_temporary_crop() {
        use std::fmt::Write as _;
        let fixture = Fixture::new();
        let pdf = fixture.root.join("Columns.pdf");
        let mut ink = String::from("0 g\n");
        for y in (20..125).step_by(11) {
            for x in [8, 68] {
                writeln!(ink, "{x} {y} 41 3 re f").unwrap();
            }
        }
        fs::write(
            &pdf,
            crate::fixed::tests::pdf_objects(&[
                "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
                "<< /Type /Pages /Count 1 /Kids [3 0 R] >>".to_owned(),
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 120 140] /Contents 4 0 R >>"
                    .to_owned(),
                format!("<< /Length {} >>\nstream\n{ink}endstream", ink.len()),
            ]),
        )
        .unwrap();
        let mut app = fixture.app();
        app.open_path(&pdf).unwrap();
        let original = app.fixed.as_ref().unwrap().reading.preferences();
        app.activate(Action::ScanColumns).unwrap();
        assert_eq!(app.screen, Screen::ScanColumns);
        assert_eq!(app.scan_columns.as_ref().unwrap().index, 0);
        app.input(Input::PageForward);
        if let Some(due) = app
            .scan_columns
            .as_ref()
            .and_then(|mode| mode.turns.deadline())
        {
            app.tick(due);
        }
        assert_eq!(app.scan_columns.as_ref().unwrap().index, 1);
        assert_eq!(app.fixed.as_ref().unwrap().reading.page(), 0);
        let active = app.fixed.as_ref().unwrap();
        assert!(active.reading.preferences().crop.unwrap().x0 > 0.5);
        assert_eq!(
            app.library.state(active.id).unwrap().fixed_preferences,
            original
        );
        assert_eq!(
            app.input(Input::Tap((DONE.x + 15) as f32, (DONE.y + 15) as f32)),
            Change::Partial
        );
        assert_eq!(app.screen, Screen::Fixed);
        assert_eq!(app.fixed.as_ref().unwrap().reading.preferences(), original);
    }

    #[test]
    fn searches_for_a_cached_author_without_opening_books_during_scan() {
        let fixture = Fixture::new();
        let mut app = fixture.app();
        assert_eq!(app.library.matching("primitiveos").count(), 0);
        app.open_path(&fixture.book).unwrap();
        assert_eq!(app.library.matching("primitiveos field").count(), 1);
        app.save().unwrap();
        let reopened = fixture.app();
        let entry = reopened.library.matching("primitiveos").next().unwrap();
        assert_eq!(entry.author.as_deref(), Some("PrimitiveOS"));
    }

    #[test]
    fn browses_page_previews_and_jumps_with_return_history() {
        let fixture = Fixture::new();
        let long = fixture.root.join("Long Chapter.epub");
        fs::write(&long, include_bytes!("../tests/fixtures/long-chapter.epub")).unwrap();
        let mut app = fixture.app();
        app.open_path(&long).unwrap();
        app.activate(Action::Overview).unwrap();
        assert_eq!(app.screen, Screen::PageOverview);
        assert!(
            app.overview
                .as_ref()
                .unwrap()
                .tiles
                .iter()
                .all(Option::is_some)
        );
        assert_eq!(app.input(Input::PageForward), Change::Partial);
        assert_eq!(app.overview.as_ref().unwrap().first, OVERVIEW_PAGES);
        let tile = overview_tile(0);
        assert_eq!(
            app.input(Input::Tap((tile.x + 20) as f32, (tile.y + 30) as f32)),
            Change::Partial
        );
        assert_eq!(app.active.as_ref().unwrap().reading.page, OVERVIEW_PAGES);
        app.activate(Action::ReturnToLocation).unwrap();
        assert_eq!(app.active.as_ref().unwrap().reading.page, 0);
    }

    #[test]
    fn opens_an_illustration_from_the_page_and_returns_to_the_same_passage() {
        let fixture = Fixture::new();
        let mut app = fixture.app();
        app.open_path(&fixture.book).unwrap();
        let reading = &app.active.as_ref().unwrap().reading;
        let initial = position(reading);
        let x = WIDTH as f32 / 2.0;
        let y = (0..PAGE_HEIGHT)
            .step_by(4)
            .find(|&y| reading.illustration_at(x, y as f32).is_some())
            .unwrap();
        assert_eq!(app.input(Input::LongTap(x, y as f32)), Change::Partial);
        assert_eq!(app.screen, Screen::Illustration);
        assert!(app.illustration.is_some());
        assert_eq!(
            app.input(Input::Tap((DONE.x + 15) as f32, (DONE.y + 15) as f32)),
            Change::Partial
        );
        assert!(app.illustration.is_none());
        assert_eq!(app.screen, Screen::Book);
        assert_eq!(position(&app.active.as_ref().unwrap().reading), initial);
    }

    #[test]
    fn imports_embedded_highlights_and_exports_a_separate_pdf() {
        let fixture = Fixture::new();
        let pdf = fixture.root.join("Notes and marks.pdf");
        let bytes = crate::fixed::tests::pdf_objects(&[
            "<< /Type /Catalog /Pages 2 0 R >>",
            "<< /Type /Pages /Count 1 /Kids [3 0 R] >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 120 80] /Rotate 90 /Annots [4 0 R] >>",
            "<< /Type /Annot /Subtype /Highlight /Rect [30 50 70 64] /QuadPoints [30 64 70 64 30 50 70 50] /Contents (Existing note) >>",
        ]);
        fs::write(&pdf, &bytes).unwrap();
        let mut app = fixture.app();
        app.open_path(&pdf).unwrap();
        app.show(Screen::Highlights);
        app.activate(Action::PreviewPdfAnnotations).unwrap();
        assert_eq!(app.screen, Screen::ImportPdfAnnotations);
        assert_eq!(
            app.pdf_annotation_import[0].note.as_deref(),
            Some("Existing note")
        );
        app.activate(Action::ApplyPdfAnnotations).unwrap();
        app.activate(Action::PreviewPdfAnnotations).unwrap();
        assert_eq!(app.screen, Screen::Highlights);
        let id = app.fixed.as_ref().unwrap().id;
        let entry = app.fixed.as_ref().unwrap().entry.clone();
        let state = app.library.remember(&entry, id, &entry.title);
        assert_eq!(state.pdf_highlights.len(), 1);
        state
            .add_pdf_highlight(PdfHighlight {
                page: 0,
                bounds: vec![PdfTextRect {
                    x0: 0.1,
                    y0: 0.1,
                    x1: 0.2,
                    y1: 0.2,
                }],
                text: "Reader quote".into(),
                note: Some("Reader note".into()),
            })
            .unwrap();
        app.activate(Action::ExportAnnotatedPdf).unwrap();
        assert_eq!(fs::read(pdf).unwrap(), bytes);
        let copy = fs::read_dir(&app.exports)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let rendered_copy = crate::fixed::Pdf::open(fs::read(&copy).unwrap()).unwrap();
        assert_eq!(rendered_copy.page_count(), 1);
        let embedded = pdf_annotations::import_highlights(&fs::read(copy).unwrap(), |page| {
            rendered_copy.page_geometry(page)
        })
        .unwrap();
        assert!(
            embedded
                .iter()
                .any(|mark| mark.note.as_deref() == Some("Existing note"))
        );
        assert!(
            embedded
                .iter()
                .any(|mark| mark.text.as_deref() == Some("Reader quote"))
        );
    }

    #[test]
    fn opens_from_library_and_resumes_a_chapter_with_its_bookmark_and_type_size() {
        let fixture = Fixture::new();
        let mut app = fixture.app();
        assert_eq!(app.screen, Screen::Library);
        let index = app
            .items()
            .iter()
            .position(|item| matches!(item.action, Action::Open(_)))
            .unwrap();
        assert_eq!(
            app.input(Input::Tap(
                85.0,
                (LIST_Y + ROW_HEIGHT * index as i32 + 33) as f32
            )),
            Change::Partial
        );
        assert_eq!(app.screen, Screen::Book);
        assert_eq!(
            app.input(Input::Tap(WIDTH as f32 / 2.0, 25.0)),
            Change::Partial
        );
        assert_eq!(app.screen, Screen::BookTools);
        assert_eq!(
            app.input(Input::Tap(85.0, HEIGHT as f32 - 45.0)),
            Change::Partial
        );
        assert_eq!(app.screen, Screen::Contents);
        let target = app
            .items()
            .into_iter()
            .find(|item| item.title.contains("Returning"))
            .unwrap();
        app.activate(target.action).unwrap();
        assert_eq!(app.active.as_ref().unwrap().reading.chapter, 1);
        app.activate(Action::Text(Settings {
            font_size: 58.0,
            ..device_settings()
        }))
        .unwrap();
        app.activate(Action::AddBookmark).unwrap();
        app.save().unwrap();
        drop(app);

        let mut reopened = fixture.app();
        reopened.activate(Action::Continue).unwrap();
        let active = reopened.active.as_ref().unwrap();
        assert_eq!(active.reading.chapter, 1);
        assert_eq!(active.reading.config.settings.font_size, 58.0);
        assert_eq!(
            reopened.library.state(active.id).unwrap().bookmarks.len(),
            1
        );
    }

    #[test]
    fn finds_repeated_text_then_returns_from_a_result_to_the_original_location() {
        let fixture = Fixture::new();
        let mut app = fixture.app();
        app.open_path(&fixture.book).unwrap();
        let original = position(&app.active.as_ref().unwrap().reading);
        app.show(Screen::Matches);
        app.search = Some(Search::new("THE TIDE KEEPS ITS OWN TIME").unwrap());
        app.step_search(Instant::now()).unwrap();
        assert_eq!(app.hits.len(), 2);
        assert_ne!(app.hits[0].anchor, app.hits[1].anchor);
        let target = app.hits[1].anchor.clone();
        app.activate(Action::SearchHit(1)).unwrap();
        let reading = &app.active.as_ref().unwrap().reading;
        assert_eq!(reading.probe.page_for(&target), Some(reading.page));
        app.activate(Action::ReturnToLocation).unwrap();
        assert_eq!(position(&app.active.as_ref().unwrap().reading), original);
    }

    #[test]
    fn follows_a_footnote_and_keeps_a_note_at_its_text_after_reopening() {
        let fixture = Fixture::new();
        let mut app = fixture.app();
        app.open_path(&fixture.book).unwrap();
        let reading = &app.active.as_ref().unwrap().reading;
        let original = position(reading);
        let target = Book::resolve_link(&reading.probe.chapter, "#tide-note").unwrap();
        app.follow(target).unwrap();
        let reading = &app.active.as_ref().unwrap().reading;
        let note = reading.probe.fragment("tide-note").unwrap();
        assert_eq!(reading.probe.page_for(&note), Some(reading.page));
        let mut end = note.clone();
        end.offset += "The first step".len();
        app.selection = reading.probe.select_range(&note, &end);
        app.save_highlight(Some("Check the tide chart".into()))
            .unwrap();
        app.activate(Action::ReturnToLocation).unwrap();
        assert_eq!(position(&app.active.as_ref().unwrap().reading), original);
        app.save().unwrap();

        let mut reopened = fixture.app();
        reopened.open_path(&fixture.book).unwrap();
        let active = reopened.active.as_ref().unwrap();
        let mark = &reopened.library.state(active.id).unwrap().highlights[0];
        assert_eq!(mark.note.as_deref(), Some("Check the tide chart"));
        assert_eq!(mark.selection.text, "The first step");
        reopened.activate(Action::Highlight(0)).unwrap();
        let reading = &reopened.active.as_ref().unwrap().reading;
        assert_eq!(reading.probe.page_for(&note), Some(reading.page));
        reopened.activate(Action::ManageHighlight(0)).unwrap();
        reopened.activate(Action::EditNote).unwrap();
        assert_eq!(reopened.editor.string(), "Check the tide chart");
        reopened.update_note("Read at low tide".into()).unwrap();
        assert_eq!(
            reopened.highlight_state().unwrap().highlights[0]
                .note
                .as_deref(),
            Some("Read at low tide")
        );
        reopened.activate(Action::RemoveHighlight).unwrap();
        reopened.save().unwrap();
        let mut reopened = fixture.app();
        reopened.open_path(&fixture.book).unwrap();
        let active = reopened.active.as_ref().unwrap();
        assert!(
            reopened
                .library
                .state(active.id)
                .unwrap()
                .highlights
                .is_empty()
        );
    }

    #[test]
    fn reads_a_fixed_page_comic_and_preserves_progress_after_reopening() {
        let fixture = Fixture::new();
        let comic = fixture.root.join("Field Comics.cbz");
        let mut png = Cursor::new(Vec::new());
        image::DynamicImage::ImageLuma8(image::GrayImage::from_pixel(8, 12, image::Luma([32])))
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        let mut zip = zip::ZipWriter::new(fs::File::create(&comic).unwrap());
        for name in ["page2.png", "page1.png"] {
            zip.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(png.get_ref()).unwrap();
        }
        zip.finish().unwrap();

        let mut app = fixture.app();
        app.open_path(&comic).unwrap();
        assert_eq!(app.screen, Screen::Fixed);
        assert_eq!(app.fixed.as_ref().unwrap().reading.count(), 2);
        app.input(Input::PageForward);
        if let Some(due) = app.fixed.as_ref().unwrap().reading.deadline() {
            assert_eq!(app.tick(due), Change::Partial);
        }
        assert_eq!(app.fixed.as_ref().unwrap().reading.page(), 1);
        app.activate(Action::RotatePage).unwrap();
        app.activate(Action::FitPage(false)).unwrap();
        app.activate(Action::AddBookmark).unwrap();
        app.activate(Action::Show(Screen::Library)).unwrap();
        app.activate(Action::Continue).unwrap();
        assert_eq!(app.fixed.as_ref().unwrap().reading.page(), 1);
        app.fixed.as_mut().unwrap().reading.jump(0).unwrap();
        app.activate(Action::Bookmark(0)).unwrap();
        assert_eq!(app.fixed.as_ref().unwrap().reading.page(), 1);
        app.save().unwrap();

        let mut reopened = fixture.app();
        reopened.activate(Action::Continue).unwrap();
        assert_eq!(reopened.fixed.as_ref().unwrap().reading.page(), 1);
    }

    #[test]
    fn reading_controls_dismiss_or_exit_without_losing_the_current_passage() {
        let fixture = Fixture::new();
        let long = fixture.root.join("Long Chapter.epub");
        fs::write(&long, include_bytes!("../tests/fixtures/long-chapter.epub")).unwrap();
        let mut app = fixture.app();
        app.open_path(&long).unwrap();
        app.activate(Action::ReadingMode(true)).unwrap();
        let before = position(&app.active.as_ref().unwrap().reading);
        app.input(Input::Swipe {
            dx: 0.0,
            dy: -620.0,
            start_y: 1_000.0,
        });
        if let Some(deadline) = app.active.as_ref().unwrap().reading.deadline() {
            app.tick(deadline);
        }
        let after = position(&app.active.as_ref().unwrap().reading);
        assert_ne!(after.anchor, before.anchor);

        let open = Input::Tap(WIDTH as f32 / 2.0, 23.0);
        assert_eq!(app.input(open), Change::Partial);
        assert_eq!(app.screen, Screen::BookTools);
        assert_eq!(app.input(Input::Tap(450.0, 920.0)), Change::Partial);
        assert_eq!(app.screen, Screen::Book);
        assert_eq!(app.input(open), Change::Partial);
        assert_eq!(
            app.input(Input::Tap(
                (EXIT_CONTROL.x + 20) as f32,
                (EXIT_CONTROL.y + 20) as f32
            )),
            Change::Exit
        );
        let mut reopened = fixture.app();
        reopened.activate(Action::Continue).unwrap();
        let reading = &reopened.active.as_ref().unwrap().reading;
        assert!(reading.config.scroll);
        assert_eq!(position(reading).anchor, after.anchor);
        assert_eq!(
            reopened.input(Input::Tap((DONE.x + 15) as f32, (DONE.y + 15) as f32)),
            Change::Exit
        );
    }

    #[test]
    fn finds_and_annotates_a_pdf_word_after_rotation() {
        let fixture = Fixture::new();
        let pdf = fixture.root.join("Reading sample.pdf");
        fs::write(&pdf, crate::fixed::tests::document()).unwrap();
        let mut app = fixture.app();
        app.open_path(&pdf).unwrap();
        app.show(Screen::Matches);
        app.pdf_search = Some(PdfSearch::new("pdf", false).unwrap());
        app.step_search(Instant::now()).unwrap();
        assert_eq!(app.pdf_hits.len(), 2);
        app.activate(Action::PdfSearchHit(1)).unwrap();
        assert_eq!(app.fixed.as_ref().unwrap().reading.page(), 1);
        app.activate(Action::ReturnToLocation).unwrap();
        assert_eq!(app.fixed.as_ref().unwrap().reading.page(), 0);
        app.activate(Action::ReadFixedText).unwrap();
        assert_eq!(app.screen, Screen::ReadFixedText);
        assert!(app.text_flow.is_some());
        for (direction, page) in [(Input::PageForward, 1), (Input::PageBack, 0)] {
            app.input(direction);
            if let Some(due) = app.text_flow.as_ref().and_then(TextFlow::deadline) {
                app.tick(due);
            }
            assert_eq!(app.text_flow.as_ref().unwrap().document_page(), page);
            assert_eq!(app.fixed.as_ref().unwrap().reading.page(), page);
        }
        assert_eq!(
            app.input(Input::Tap((DONE.x + 10) as f32, (DONE.y + 10) as f32)),
            Change::Partial
        );
        assert_eq!(app.screen, Screen::Fixed);
        app.activate(Action::RotatePage).unwrap();
        let reading = &app.fixed.as_ref().unwrap().reading;
        let text = reading.page_text(0).unwrap();
        let (size, viewport) = reading.page_viewport().unwrap();
        let point = text.words[0]
            .bounds
            .in_viewport(size, viewport)
            .unwrap()
            .center();
        assert_eq!(
            app.input(Input::LongTap(point.x as f32, point.y as f32)),
            Change::Partial
        );
        assert_eq!(app.pdf_selection.as_ref().unwrap().text(), "PDF");
        app.save_pdf_highlight(Some("Read again".into())).unwrap();
        app.activate(Action::CropPage).unwrap();
        let (size, viewport) = app.fixed.as_ref().unwrap().reading.page_viewport().unwrap();
        let transform = viewport.page_to_viewport(size).unwrap();
        for (u, v) in [(0.08, 0.12), (0.88, 0.83)] {
            let point =
                transform * hayro::kurbo::Point::new(f64::from(size.0) * u, f64::from(size.1) * v);
            assert_eq!(
                app.input(Input::Tap(point.x as f32, point.y as f32)),
                Change::Partial
            );
        }
        assert_eq!(app.screen, Screen::Fixed);
        assert!(
            app.fixed
                .as_ref()
                .unwrap()
                .reading
                .preferences()
                .crop
                .is_some()
        );
        assert_eq!(
            app.input(Input::Tap((DONE.x + 10) as f32, (DONE.y + 10) as f32)),
            Change::Exit
        );

        let mut reopened = fixture.app();
        reopened.activate(Action::Continue).unwrap();
        let state = reopened
            .library
            .state(reopened.fixed.as_ref().unwrap().id)
            .unwrap();
        assert_eq!(state.pdf_highlights[0].note.as_deref(), Some("Read again"));
        assert!(state.fixed_preferences.crop.is_some());
        reopened
            .activate(Action::ExportHighlights(annotation_export::Format::Text))
            .unwrap();
        assert!(fs::read_dir(&reopened.exports).unwrap().next().is_some());
    }

    #[test]
    fn password_protected_pdf_opens_after_a_local_retry() {
        let fixture = Fixture::new();
        let locked = fixture.root.join("Locked.pdf");
        fs::write(
            &locked,
            include_bytes!("../tests/fixtures/password-required.pdf"),
        )
        .unwrap();
        let mut app = fixture.app();
        app.open_path(&locked).unwrap();
        assert_eq!(app.screen, Screen::PdfPassword);
        let entry = app.locked.clone().unwrap();
        app.open_with_password(entry.clone(), Some("wrong"))
            .unwrap();
        assert_eq!(app.screen, Screen::PdfPassword);
        assert!(app.notice.as_deref().unwrap().contains("Password rejected"));
        app.open_with_password(entry, Some("reader-test")).unwrap();
        assert_eq!(app.screen, Screen::Fixed);
        assert!(app.locked.is_none());
        let recognized = crate::fixed_text::PdfPageText {
            text: "Recognized words".into(),
            words: vec![crate::fixed_text::PdfTextSpan {
                bytes: 0..10,
                bounds: PdfTextRect {
                    x0: 0.1,
                    y0: 0.1,
                    x1: 0.3,
                    y1: 0.2,
                },
            }],
            ..Default::default()
        };
        app.fixed
            .as_mut()
            .unwrap()
            .reading
            .remember_ocr(0, "eng", recognized)
            .unwrap();
        let mut reopened = fixture.app();
        let entry = reopened.library.resolve(&locked).unwrap();
        reopened
            .open_with_password(entry, Some("reader-test"))
            .unwrap();
        let text = reopened
            .fixed
            .as_ref()
            .unwrap()
            .reading
            .page_text(0)
            .unwrap();
        assert_eq!(text.text, "Recognized words");
        reopened.activate(Action::ReadFixedText).unwrap();
        assert_eq!(reopened.screen, Screen::ReadFixedText);
        app.save().unwrap();
        assert!(
            !fs::read_to_string(&fixture.state)
                .unwrap()
                .contains("reader-test")
        );
    }
    #[test]
    fn koreader_preview_excludes_ambiguous_quotes_and_imports_unique_notes_once() {
        let fixture = Fixture::new();
        let mut app = fixture.app();
        app.open_path(&fixture.book).unwrap();
        let title = &app.active.as_ref().unwrap().reading.probe.title;
        let export = serde_json::json!({
            "title": title,
            "entries": [
                {"text":"The copper lantern stood beside the door.","note":"Check the lantern"},
                {"text":"The tide keeps its own time.","note":"Repeated quote"}
            ]
        });
        fs::write(
            fixture.root.join("koreader-import.json"),
            serde_json::to_vec(&export).unwrap(),
        )
        .unwrap();
        for _ in 0..2 {
            app.preview_koreader().unwrap();
            while app.annotation_import.as_ref().unwrap().deadline.is_some() {
                app.step_koreader(Instant::now()).unwrap();
            }
            let preview = app.annotation_import.as_ref().unwrap();
            assert_eq!(preview.matched.len(), 1);
            assert_eq!(preview.unresolved, 1);
            app.apply_koreader().unwrap();
        }
        let id = app.active.as_ref().unwrap().id;
        let highlights = &app.library.state(id).unwrap().highlights;
        assert_eq!(highlights.len(), 1);
        assert_eq!(highlights[0].note.as_deref(), Some("Check the lantern"));
    }
}
