//! The daily reader interface and foreground lifecycle.
mod annotation_search;
mod bar_icons;
pub(crate) const NOTE_MARK_ICON: primitive_ui::Icon = bar_icons::HIGHLIGHT;
mod bookmark_browser;
mod reading_history;
mod custom_contents;
mod contents_position;
pub(crate) mod publisher_pages;
mod character_page_settings;
use character_page_settings::NumberField;
mod dictionary_panel;
mod default_settings;
mod cjk_selection;
mod dictionary_sources;
use dictionary_sources::{SourceAction, SourcePreferences};
mod end_of_book;
mod fixed_book_map;
mod gestures;
mod extend_highlight;
mod external_link;
#[cfg(test)]
mod epub_gif_tests;
#[cfg(test)]
mod fixed_results_tests;
mod sidecar_marks;
mod library_view;
mod library_export;
mod lookup_history;
mod progress_sync;
mod vocabulary;
mod metadata_save;
mod nearby_search;
mod online_panel;
mod opds;
mod quick_display;
mod reader_timer;
mod read_alarm;
mod reading_backup;
mod reading_bars;
mod result_context;
mod reading_footer;
mod selection_popup;
mod saved_highlight_popup;
mod highlight_palette;
mod user_hyphenation;
mod footnote_popup;
#[cfg(test)]
mod font_render_tests;
mod top_config;
mod hidden_flows;
#[cfg(test)]
mod nonlinear_tests;
#[cfg(test)]
mod workflow_integration_tests;

use std::collections::{BTreeSet, HashSet};
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use primitive_app::{App, Change, HEIGHT, WIDTH, with_storage};
use primitive_ui::{
    BODY, CAPTION, Canvas, Editor, Input, Layout, Press, Rect, Row, RowAccessory, Rows,
    header, keyboard_key,
};

use crate::annotation_export;
use crate::dictionary::{self, DictionarySources, InstalledDictionary};
use crate::epub::{Book, Destination};
use crate::fixed::{PdfError, PdfLinkTarget};
use crate::fixed_session::{FixedJump, FixedReading, PdfSelection};
use crate::fixed_text::PdfTextRect;
use crate::generated_contents::{GeneratedContents, OutlineTarget};
use crate::koreader_import::{self, ExportEntry, QuoteMatch, QuoteResolver};
use crate::koreader_sidecar::{self, SidecarProgress};
use crate::library::{
    BookMetadata, BookPreferences, BookState, Bookmark, DocumentFormat,
    FixedPreferences, Highlight, Library, LibraryBook, PdfHighlight,
    ReadingPosition, ReadingStatus,
};
use crate::ocr_job::{OcrAssets, OcrJob};
use crate::pdf_annotations;
use crate::reading_controls::{
    MenuActivation, ReadingControls, ReadingOrientation, SystemTheme,
};
use crate::reading_fonts;
use crate::reading_profiles::{self, Profile};
use crate::reading_stats::ReadingStats;
use crate::search::{MAX_FIXED_SEARCH_HITS, PdfSearch, PdfSearchHit, Search, SearchHit};
use crate::selection::TextSelection;
use crate::session::{
    BookConfig, DONE, PAGE_HEIGHT, PAGE_REFRESH_GAP, PageThumbnail, PageTurns, Reading, page_thumbnail,
};
use crate::text_flow::TextFlow;
use crate::{Engine, Probe, Result, Settings, TextAlignment};

use self::custom_contents::CustomAction;
use self::gestures::{Binding, Gesture, GestureBindings};
use self::library_view::{FileBrowser, file_type};
use self::lookup_history::{LookupHistory, SearchQuery};
use self::quick_display::{DisplayLevel, QuickDisplay};
use self::reader_timer::AutoTurn;
use self::reading_bars::BarPanel;
use self::top_config::TopGroup;

const ROW_HEIGHT: i32 = 116;
const LIST_X: i32 = 40;
const LIST_Y: i32 = 154;
const ROWS_PER_PAGE: usize = 11;
const OVERVIEW_PAGES: usize = 4;
const FIRST_MATCH_PAGE: Rect = Rect::new(40, HEIGHT as i32 - 104, 160, 72);
const PREV_MATCH_PAGE: Rect = Rect::new(210, HEIGHT as i32 - 104, 210, 72);
const NEXT_MATCH_PAGE: Rect = Rect::new(WIDTH as i32 - 420, HEIGHT as i32 - 104, 210, 72);
const LAST_MATCH_PAGE: Rect = Rect::new(WIDTH as i32 - 200, HEIGHT as i32 - 104, 160, 72);
const PREV: Rect = Rect::new(40, HEIGHT as i32 - 104, 240, 72);
const NEXT: Rect = Rect::new(WIDTH as i32 - 280, HEIGHT as i32 - 104, 240, 72);
const SAVE_DELAY: Duration = Duration::from_millis(800);
const SEARCH_DELAY: Duration = Duration::from_millis(60);
// Return to the input loop between fixed-page batches. An expensive individual
// page still owns the provider until its embedded text has been extracted.
const FIXED_SEARCH_YIELD: Duration = Duration::from_millis(8);
const SEARCH_REFRESH: Duration = Duration::from_millis(450);
const UNLOCK_TOUCH: Rect = Rect::new(832, PAGE_HEIGHT as i32 + 5, 258, 60);
const CANCEL_OCR: Rect = Rect::new(624, PAGE_HEIGHT as i32 + 5, 200, 60);
const NEXT_COMIC: Rect = Rect::new(598, PAGE_HEIGHT as i32 + 5, 223, 60);
const FIND_CASE: Rect = Rect::new(LIST_X, LIST_Y + 260, 335, 70);
const FIND_PATTERN: Rect = Rect::new(LIST_X + 795, LIST_Y + 260, 380, 70);
const FIND_ACCENTS: Rect = Rect::new(LIST_X + 355, LIST_Y + 260, 420, 70);
const MATCH_PREV: Rect = Rect::new(420, PAGE_HEIGHT as i32 + 5, 196, 60);
const MATCH_NEXT: Rect = Rect::new(628, PAGE_HEIGHT as i32 + 5, 196, 60);
const DEFINITION_BOTTOM: i32 = HEIGHT as i32 - 82;
const OCR_POLL: Duration = Duration::from_millis(120);
const OVERVIEW_TRACK: Rect = Rect::new(80, 1310, WIDTH as i32 - 160, 75);
const AUTO_CROP: Rect = Rect::new(674, PAGE_HEIGHT as i32 + 5, 182, 60);
const RESET_CROP: Rect = Rect::new(868, PAGE_HEIGHT as i32 + 5, 172, 60);
const CANCEL_CROP: Rect = Rect::new(1052, PAGE_HEIGHT as i32 + 5, 192, 60);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Screen {
    Footnote,
    Library,
    LibraryFind,
    OrganizeLibrary,
    BookmarkBrowser(bookmark_browser::Page),
    ReadingHistory(reading_history::Page),
    Catalog(opds::Page),
    BookList,
    BookDetails,
    CollectionName,
    RemoveCollection,
    ReadingControls,
    FooterSettings,
    EndActionSettings,
    MetadataSaveSettings,
    HiddenFlows,
    Gestures,
    GestureBinding(Gesture),
    ReadingData,
    LibraryExport(library_export::Page),
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
    PublisherPages,
    PageNumbering,
    NumberingValue(NumberField),
    NumberingDefaults,
    PublisherOffer,
    PageOverview,
    BookMap,
    AutoTurn,
    LocationHistory,
    BookTools,
    Contents,
    GeneratedContents,
    CustomContents,
    CustomTitle,
    Hyphenate,
    CustomEdits,
    CustomBookmarks,
    Links,
    Bookmarks,
    Typography,
    ReadingFonts,
    ReadingProfiles,
    DocumentDefaults,
    ProfileName,
    RemoveProfiles,
    Find,
    Matches,
    SearchHistory,
    BookmarkFind,
    BookmarkMatches,
    Highlights,
    Annotation,
    RemoveHighlight,
    Note,
    EditNote,
    Definition,
    DictionaryEntry,
    OnlineLookup,
    OnlineEdit,
    OnlineHistory,
    ClearOnlineHistory,
    DictionaryHistory,
    Vocabulary,
    ProgressSync,
    ProgressSyncSettings,
    SyncEdit(progress_sync::Field),
    VocabularyWord,
    VocabularyContext,
    VocabularyOptions,
    VocabularyBooks,
    VocabularySearch,
    Dictionaries,
    DictionaryOptions(usize),
}

impl Screen {
    fn title(self) -> &'static str {
        match self {
            Self::Footnote => "Footnote",
            Self::Library => "Reader",
            Self::LibraryFind => "Find a document",
            Self::OrganizeLibrary => "Library organization",
            Self::BookmarkBrowser(page) => page.title(),
            Self::ReadingHistory(page) => page.title(),
            Self::Catalog(page) => page.title(),
            Self::BookList => "Books",
            Self::BookDetails => "Book details",
            Self::CollectionName => "Collection name",
            Self::RemoveCollection => "Remove collection?",
            Self::ReadingControls => "Reading controls",
            Self::FooterSettings => "Reading status bar",
            Self::EndActionSettings => "At the end of a book",
            Self::MetadataSaveSettings => "Save reading data",
            Self::HiddenFlows => "Custom hidden flows",
            Self::Gestures => "Reading gestures",
            Self::GestureBinding(gesture) => gesture.label(),
            Self::ReadingData => "Reading data",
            Self::LibraryExport(page) => page.title(),
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
            Self::PageNumbering => "Page numbering",
            Self::NumberingValue(NumberField::MarginSize) => "Margin label size",
            Self::NumberingValue(_) => "Characters per page",
            Self::NumberingDefaults => "Page numbering defaults",
            Self::PublisherOffer => "Publisher page numbers",
            Self::PublisherPages => "Stable page number list",
            Self::PageOverview => "Pages",
            Self::BookMap => "Book map",
            Self::AutoTurn => "Automatic page turns",
            Self::LocationHistory => "Location history",
            Self::ReadFixedText => "",
            Self::OcrLanguage => "Recognize text",
            Self::PdfPassword => "Unlock PDF",
            Self::ScanColumns => "",
            Self::Contents | Self::FixedContents => "Contents",
            Self::CustomContents => "Custom contents",
            Self::CustomEdits => "Edit chapters",
            Self::CustomBookmarks => "Choose a bookmark",
            Self::Hyphenate => "Hyphenate",
            Self::CustomTitle => "Chapter title",
            Self::GeneratedContents => "Generated outline",
            Self::Links => "Links on this page",
            Self::Bookmarks | Self::FixedBookmarks => "Bookmarks",
            Self::Typography => "Text settings",
            Self::ReadingFonts => "Reading font",
            Self::ReadingProfiles => "Reading profiles",
            Self::DocumentDefaults => "Document defaults",
            Self::ProfileName => "Save reading profile",
            Self::RemoveProfiles => "Remove a reading profile",
            Self::Find => "Find in book",
            Self::Matches => "Search results",
            Self::SearchHistory => "Recent searches",
            Self::BookmarkFind => "Find bookmarks",
            Self::BookmarkMatches => "Bookmark search results",
            Self::Highlights => "Highlights and notes",
            Self::Annotation => "Highlight",
            Self::RemoveHighlight => "Remove highlight?",
            Self::Note => "Add note",
            Self::EditNote => "Edit note",
            Self::Definition | Self::DictionaryEntry => "Dictionary",
            Self::OnlineLookup => "Online lookup",
            Self::OnlineEdit => "Online lookup",
            Self::OnlineHistory => "Online lookup history",
            Self::ClearOnlineHistory => "Online lookup history",
            Self::DictionaryHistory => "Dictionary history",
            Self::Vocabulary => "Vocabulary",
            Self::ProgressSync => "Progress sync",
            Self::ProgressSyncSettings => "Sync settings",
            Self::SyncEdit(field) => field.title(),
            Self::VocabularyWord => "Word review",
            Self::VocabularyContext => "Reading context",
            Self::VocabularyOptions => "Vocabulary options",
            Self::VocabularyBooks => "Vocabulary books",
            Self::VocabularySearch => "Find a saved word",
            Self::Dictionaries => "Dictionaries",
            Self::DictionaryOptions(_) => "Dictionary settings",
        }
    }
}

enum Action {
    Catalog(opds::Command),
    Open(LibraryBook),
    OpenTracked(u64),
    OpenPreviousDocument,
    Browse(PathBuf),
    Continue,
    ClearLibraryFilter,
    ListBooks(BookList),
    OrganizeCurrent,
    BookmarkBrowser(bookmark_browser::Command),
    ReadingHistory(reading_history::Command),
    ToggleNonlinear,
    Flow(hidden_flows::FlowAction),
    ViewCover,
    Favorite(bool),
    SetStatus(ReadingStatus),
    CollectionMembership(String, bool),
    NameCollection(Option<String>),
    RemoveCollection,
    SetReadingControls(ReadingControls),
    BindGesture(Gesture, Binding),
    SetDisplayLevel(DisplayLevel, u32),
    RefreshDisplay,
    AutoTurn(Option<Duration>),
    ReadAlarm,
    SaveBookData,
    HighlightStyle,
    LockTouch,
    ExportBackup,
    ClearOcr,
    PreviewBackup,
    MergeBackup,
    ToggleBackupComponent(usize),
    PreviewNearbyMarks,
    ApplyNearbyMarks,
    PreviewKoReader,
    ApplyKoReader,
    PreviewKoReaderPosition,
    PreviewCopiedPosition,
    ApplyKoReaderPosition,
    PreviewPdfAnnotations,
    ApplyPdfAnnotations,
    ExportAnnotatedPdf,
    Show(Screen),
    Jump(Destination),
    SetCharacterPages(Option<u16>),
    CharacterPage(usize),
    AcceptPublisherNumbers,
    TogglePublisherFooter,
    TogglePublisherMargins,
    ShowGeneratedContents,
    Custom(CustomAction),
    GeneratedEntry(usize),
    Bookmark(usize),
    AddBookmark,
    ToggleBookmark,
    ReturnToLocation,
    ReturnToHistory(usize),
    NextLocation,
    SetUserHyphenation(bool),
    Text(Settings),
    ShowFonts,
    ShowDictionaries,
    DictionarySource(SourceAction),
    DocumentDefaults(default_settings::Command),
    LookUp(String),
    ProgressSync(progress_sync::Command),
    Vocabulary(vocabulary::Command),
    Wikipedia,
    TranslatePage,
    RepeatOnline(usize),
    ToggleOnlineHistory,
    ClearOnlineHistory,
    RepeatSearch(SearchQuery),
    StepNearby(crate::search::SearchDirection),
    SetFont(Option<String>),
    ApplyProfile(usize),
    RemoveProfile(usize),
    ResetText,
    ReadingMode(bool),
    FacingPages(bool),
    SearchHit(usize),
    PdfSearchHit(usize),
    PdfHighlight(usize),
    Highlight(usize),
    ManageHighlight(usize),
    ExtendHighlight,
    EditNote,
    RemoveHighlight,
    ExportHighlights(annotation_export::Format),
    LibraryExport(library_export::Command),
    FitPage(bool),
    FitHeight,
    SetFixedZoom(usize),
    SetFixedRotation(u8),
    FixedScroll(bool),
    ComicSpread(bool),
    ScanContrast(bool),
    FixedDither(bool),
    FixedGap(u16),
    ScanWhiteLevel(Option<u8>),
    StraightenScan(u8),
    ForceOcr(bool),
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
    ReflowScan,
}

enum BookList {
    Favorites,
    Recent,
    Authors,
    Author(Option<String>),
    Status(ReadingStatus),
    Collection(String),
}

struct BookDetails {
    entry: LibraryBook,
    id: u64,
    back: Screen,
    close_document: bool,
    metadata: Option<BookMetadata>,
}

struct PageOverview {
    first: usize,
    count: usize,
    chapter: Option<usize>,
    preview: Option<Box<Probe>>,
    tiles: Vec<Option<PageThumbnail>>,
}

struct OutlineScan {
    contents: GeneratedContents,
    deadline: Option<Instant>,
    next_refresh: Instant,
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
    folder: bool,
}

impl Item {
    fn new(title: impl Into<String>, action: Action) -> Self {
        Self {
            title: title.into(),
            detail: None,
            action,
            folder: false,
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
    numbering: Option<crate::synthetic_pages::CharacterPages>,
}

#[derive(Clone, Debug, PartialEq)]
struct HistoryLocation {
    position: ReadingPosition,
    scan_point: Option<hayro::kurbo::Point>,
}

impl From<ReadingPosition> for HistoryLocation {
    fn from(position: ReadingPosition) -> Self {
        Self {
            position,
            scan_point: None,
        }
    }
}

struct ActiveFixed {
    footer_chapters: Vec<(usize, String)>,
    entry: LibraryBook,
    id: u64,
    reading: FixedReading,
}

struct ActiveOcr {
    page: usize,
    selection_point: Option<hayro::kurbo::Point>,
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

struct PositionImport {
    source: &'static str,
    sidecar: SidecarProgress,
    target: std::result::Result<ReadingPosition, String>,
}

pub struct Reader {
    library: Library,
    catalog: opds::Panel,
    user_hyphenation: std::rc::Rc<crate::user_hyphenation::Rules>,
    user_hyphenation_path: PathBuf,
    hyphenation_edit: Option<user_hyphenation::Draft>,
    browser: FileBrowser,
    bookmark_browser: bookmark_browser::Browser,
    library_query: String,
    book_list: Option<BookList>,
    book_details: Option<BookDetails>,
    overview: Option<PageOverview>,
    flow_popup: Option<hidden_flows::FlowPopup>,
    outline: Option<OutlineScan>,
    numbering_scan: Option<character_page_settings::Scan>,
    numbering_directory: PathBuf,
    chapter_draft: Option<custom_contents::Draft>,
    scan_columns: Option<ScanReading>,
    rename_collection: Option<String>,
    gestures: GestureBindings,
    controls_path: PathBuf,
    controls: ReadingControls,
    end_dialog: Option<end_of_book::Dialog>,
    pages_since_refresh: u8,
    touch_locked: bool,
    stats: ReadingStats,
    reading_history: crate::reading_stats::history::History,
    history_view: reading_history::View,
    pace: crate::reading_pace::ReadingPace,
    stats_path: PathBuf,
    stats_awake: bool,
    backup_import: Option<reading_backup::Preview>,
    annotation_import: Option<AnnotationImport>,
    nearby_marks: Option<sidecar_marks::MarkImport>,
    position_import: Option<PositionImport>,
    pdf_annotation_import: Vec<PdfHighlight>,
    import_path: PathBuf,
    fonts: Vec<PathBuf>,
    fonts_directory: PathBuf,
    available_fonts: Vec<String>,
    profiles: Vec<Profile>,
    profiles_path: PathBuf,
    document_defaults: crate::document_defaults::Store,
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
    reading_panel: Option<BarPanel>,
    last_text_panel: BarPanel,
    last_fixed_panel: BarPanel,
    top_group: Option<TopGroup>,
    last_top_group: TopGroup,
    reading_top: bool,
    reading_bottom: bool,
    quick_display: QuickDisplay,
    /// Opened from Primitive Launcher: the launcher's theme, which the whole Reader follows. `None` standalone.
    system_theme: Option<SystemTheme>,
    refresh_display: bool,
    auto_turn: AutoTurn,
    read_alarm: read_alarm::ReadAlarm,
    list_page: usize,
    matches_page: usize,
    result_context: Option<result_context::ResultContext>,
    focused_result: Option<usize>,
    current_contents_row: Option<usize>,
    publisher_label_canvas: std::cell::RefCell<Option<Canvas>>,
    map_window: fixed_book_map::MapWindow,
    back: Vec<HistoryLocation>,
    forward: Vec<HistoryLocation>,
    notice: Option<String>,
    dirty: bool,
    save_at: Option<Instant>,
    metadata_checkpoint: metadata_save::Checkpoint,
    editor: Editor,
    nearby: nearby_search::Nearby,
    find_match_case: bool,
    find_ignore_accents: bool,
    find_regex: bool,
    search_result: Option<usize>,
    pdf_search: Option<PdfSearch>,
    pdf_hits: Vec<PdfSearchHit>,
    pdf_result_regions: Vec<(usize, PdfTextRect)>,
    pdf_selection: Option<PdfSelection>,
    hits: Vec<SearchHit>,
    result_selection: Option<TextSelection>,
    search_at: Option<Instant>,
    next_search_refresh: Instant,
    selection: Option<TextSelection>,
    footnote: Option<footnote_popup::Footnote>,
    highlight_index: usize,
    range_edit: Option<extend_highlight::RangeEdit>,
    link_popup: Option<external_link::LinkPopup>,
    copied_address: Option<String>,
    saved_popup: Option<saved_highlight_popup::SavedPopup>,
    exports: PathBuf,
    library_export: library_export::Export,
    dictionaries: DictionarySources,
    dictionary_preference: PathBuf,
    dictionary_settings: SourcePreferences,
    dictionary_origin: Screen,
    installed_dictionaries: Vec<InstalledDictionary>,
    lookup_history_path: PathBuf,
    lookup_history: LookupHistory,
    progress_sync: progress_sync::View,
    vocabulary_view: vocabulary::View,
    online: Option<online_panel::Panel>,
    annotation_search: annotation_search::AnnotationSearch,
    definition_word: String,
    definitions: Vec<dictionary::Definition>,
    definition_results_page: Option<usize>,
    definition_expanded: bool,
    dictionary_navigation: dictionary_panel::Navigation,
    spelling_lookup: Option<dictionary_panel::SpellingLookup>,
    cjk_selection_lookup: Option<cjk_selection::Lookup>,
}

impl Reader {
    pub fn open(documents: &Path, state: &Path, fonts: Vec<PathBuf>) -> io::Result<Self> {
        reading_backup::recover(state)?;
        Self::open_data(documents, state, fonts)
    }

    fn open_data(documents: &Path, state: &Path, fonts: Vec<PathBuf>) -> io::Result<Self> {
        let (library, browser) = with_storage(|| {
            let mut library = Library::open(documents, state)?;
            library.scan()?;
            let browser = FileBrowser::new(library.browse(Path::new(""))?);
            Ok((library, browser))
        })?;
        let data = state.parent().unwrap_or(documents);
        let controls_path = data.join("reading-controls.json");
        let controls = ReadingControls::load(&controls_path, !library.recents().is_empty());
        let stats_path = data.join("reading-stats.json");
        let stats = ReadingStats::load(&stats_path);
        let reading_history = crate::reading_stats::history::History::load(
            data.join("reading-history.json"), stats.recorded_totals(),
        );
        let profiles_path = data.join("reading-profiles.json");
        let profiles = reading_profiles::load(&profiles_path);
        let dictionary_preference = data.join("dictionary-preference.json");
        let lookup_history_path = data.join("lookup-history.json");
        let lookup_history = LookupHistory::load(&lookup_history_path);
        let dictionary_settings = SourcePreferences::load(&dictionary_preference);
        let ocr_assets = with_storage(|| Ok(data.parent().and_then(OcrAssets::discover)))?;
        let user_hyphenation_path = data.join("user-hyphenation.json");
        let user_hyphenation = user_hyphenation::load(&user_hyphenation_path)
            .unwrap_or_default()
            .into();
        Ok(Self {
            user_hyphenation,
            user_hyphenation_path,
            hyphenation_edit: None,
            library,
            catalog: opds::Panel::load(documents,data.join("opds-catalogs.json"))?,
            browser,
            bookmark_browser: bookmark_browser::Browser::load(data.join("bookmark-browser.json")),
            library_query: String::new(),
            book_list: None,
            book_details: None,
            overview: None,
            flow_popup: None,
            outline: None,
            numbering_scan: None,
            numbering_directory: data.join("character-pages"),
            chapter_draft: None,
            scan_columns: None,
            rename_collection: None,
            controls_path,
            controls,
            end_dialog: None,
            pages_since_refresh: 0,
            touch_locked: false,
            stats,
            reading_history,
            history_view: Default::default(),
            pace: crate::reading_pace::ReadingPace::new(data.join("reading-pace")),
            stats_path,
            stats_awake: true,
            backup_import: None,
            annotation_import: None,
            nearby_marks: None,
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
            document_defaults: crate::document_defaults::Store::load(data.join(crate::document_defaults::FILE)),
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
            reading_panel: None,
            last_text_panel: BarPanel::Text,
            last_fixed_panel: BarPanel::View,
            top_group: None,
            last_top_group: TopGroup::Navigation,
            reading_top: false,
            reading_bottom: false,
            gestures: GestureBindings::load(data),
            quick_display: QuickDisplay::read(),
            system_theme: SystemTheme::from_env(),
            refresh_display: false,
            auto_turn: AutoTurn::default(),
            read_alarm: read_alarm::ReadAlarm::open(data.join("read-timer.json")),
            list_page: 0,
            matches_page: 0,
            result_context: None,
            focused_result: None,
            current_contents_row: None,
            publisher_label_canvas: Default::default(),
            map_window: fixed_book_map::MapWindow::default(),
            back: Vec::new(),
            forward: Vec::new(),
            notice: None,
            dirty: false,
            save_at: None,
            metadata_checkpoint: metadata_save::Checkpoint::new(Instant::now()),
            editor: Editor::new(64),
            nearby: nearby_search::Nearby::default(),
            find_match_case: false,
            find_ignore_accents: false,
            find_regex: false,
            search_result: None,
            pdf_search: None,
            pdf_hits: Vec::new(),
            pdf_result_regions: Vec::new(),
            pdf_selection: None,
            hits: Vec::new(),
            result_selection: None,
            search_at: None,
            next_search_refresh: Instant::now(),
            selection: None,
            footnote: None,
            highlight_index: 0,
            range_edit: None,
            link_popup: None,
            copied_address: None,
            saved_popup: None,
            exports: state.parent().unwrap_or(documents).join("exports"),
            library_export: library_export::Export::load(data.join("highlight-export.json")),
            dictionaries: DictionarySources::for_reader(documents, data),
            dictionary_preference,
            dictionary_settings,
            dictionary_origin: Screen::ReadingControls,
            installed_dictionaries: Vec::new(),
            lookup_history_path,
            lookup_history,
            progress_sync: progress_sync::View::load(data.join("progress-sync.json")),
            vocabulary_view: Default::default(),
            online: None,
            annotation_search: annotation_search::AnnotationSearch::default(),
            definition_word: String::new(),
            definitions: Vec::new(),
            definition_results_page: None,
            definition_expanded: false,
            dictionary_navigation: Default::default(),
            spelling_lookup: None,
            cjk_selection_lookup: None,
        })
    }

    pub fn open_path(&mut self, path: &Path) -> Result<()> {
        let entry = with_storage(|| self.library.resolve(path))?;
        self.open_book(entry)
    }

    fn open_book(&mut self, entry: LibraryBook) -> Result<()> {
        let publication = with_storage(|| Ok(self.library.publication_for_chapter(&entry)))?;
        if let Some((marker, chapter, path)) = publication {
            self.open_with_password(marker, None)?;
            if self.fixed.is_some() {
                return self.follow_fixed(chapter, None);
            }
            return self.follow(Destination { path, fragment: None });
        }
        self.open_with_password(entry, None)
    }

    fn open_with_password(&mut self, mut entry: LibraryBook, password: Option<&str>) -> Result<()> {
        let (absolute, id, prepared_pdf) = with_storage(|| {
            let absolute = self.library.absolute_path(&entry.path)?;
            if entry.format == DocumentFormat::Pdf
                && DocumentFormat::for_path(&entry.path) == Some(DocumentFormat::Pdf)
            {
                let bytes = crate::fixed::pdf_open_timing("read", || {
                    crate::fixed_session::pdf_bytes(&absolute, Some(entry.size))
                })?;
                let id = crate::fixed::pdf_open_timing("identify", || {
                    crate::library::fingerprint_bytes(&bytes, entry.format.tag())
                });
                Ok((absolute, id, Some(bytes)))
            } else {
                Ok((absolute, self.library.identify(&entry)?, None))
            }
        })?;
        let unseen = self.library.state(id).is_none();
        let mut initial_preferences = self.document_defaults.templates.reflow.clone().unwrap_or_default();
        let mut default_font_missing = false;
        let mut settings = initial_preferences.apply_to(self.device_reading_settings());
        let mut scroll = initial_preferences.scroll;
        let mut facing_pages = initial_preferences.facing_pages;
        let mut fixed_preferences = self.document_defaults.templates.fixed.unwrap_or_default().apply(FixedPreferences::default());
        let position = self.library.state(id).and_then(|state| {
            // A saved state inherits the device baseline, never a newly changed
            // template, including older records with empty/default preferences.
            settings = state.preferences.apply_to(self.device_reading_settings());
            scroll = state.preferences.scroll;
            facing_pages = state.preferences.facing_pages;
            fixed_preferences = state.fixed_preferences;
            state.position.clone()
        });
        let fixed = FixedReading::open(
            self.device_reading_settings().height,
            entry.format,
            &absolute,
            position.as_ref().map_or(0, |saved| saved.page),
            &self.fonts,
            fixed_preferences,
            password,
            prepared_pdf,
        );
        let fixed = match fixed {
            Err(error)
                if error.downcast_ref::<PdfError>() == Some(&PdfError::PasswordProtected) =>
            {
                self.locked = Some(entry);
                self.show(Screen::PdfPassword);
                self.editor = Editor::new(127);
                if password.is_some() {
                    self.notice =
                        Some("Password rejected. Enter the document or owner password.".into());
                }
                return Ok(());
            }
            result => result?,
        };
        self.locked = None;
        self.numbering_scan = None;
        self.touch_locked = false;
        self.ocr = None;
        self.hits.clear();
        self.pdf_hits.clear();
        self.cancel_search();
        self.search_result = None;
        self.result_selection = None;
        if let Some(mut reading) = fixed {
            let flows = self.library.state(id).map(|state| state.hidden_flows.clone()).unwrap_or_default();
            reading.restore_hidden_flows(&flows, position.as_ref().map_or(0, |saved| saved.page), fixed_preferences)?;
            reading.set_ocr_path(self.ocr_root.join(format!("{id:016x}")));
            reading.ocr_language = self.library.state(id).and_then(|state| state.ocr_language.clone())
                .filter(|code| self.ocr_assets.as_ref().is_none_or(|assets| assets.languages().contains(code)));
            if let Some(title) = reading.title() {
                entry.title = title;
            }
            entry.author = reading.author();
            self.save_library()?;
            self.active = None;
            self.pages_since_refresh = 0;
            self.range_edit = None;
            self.saved_popup = None;
            self.fixed = Some(ActiveFixed {
                footer_chapters: reading_footer::fixed_chapters(&reading),
                entry,
                id,
                reading,
            });
            self.pdf_result_regions.clear();
            self.pdf_selection = None;
            self.back.clear();
            self.forward.clear();
            self.auto_turn.configure(None, Instant::now());
            self.selection = None;
            self.annotation_search = annotation_search::AnnotationSearch::default();
            self.restore_reading_menus(id);
            self.show(Screen::Fixed);
            self.record_position();
            if let Some(fixed) = &self.fixed {
                self.library.cache_author(id, fixed.entry.author.as_deref());
            }
            return Ok(());
        }
        if unseen && initial_preferences.font_file.is_some()
            && self.fonts_for(initial_preferences.font_file.as_deref()).is_err() {
            initial_preferences.font_file = None;
            default_font_missing = true;
        }
        let custom_font = self.library.state(id).map_or(initial_preferences.font_file.as_deref(),
            |state| state.preferences.font_file.as_deref());
        let fonts = self
            .fonts_for(custom_font)
            .unwrap_or_else(|_| self.fonts.clone());
        let config = BookConfig {
            user_hyphenation: self.user_hyphenation.clone(),
            chapter_offsets: Default::default(),
            engine: Engine::Parley,
            book: absolute,
            fonts,
            settings,
            scroll,
            facing_pages,
            hide_nonlinear: self.library.state(id).and_then(|state| state.hide_nonlinear)
                .unwrap_or(self.controls.hide_nonlinear_default),
            hidden_flows: self.library.state(id).map(|state| state.hidden_flows.clone())
                .unwrap_or_default().into(),
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
        self.save_library()?;
        self.pages_since_refresh = 0;
        self.range_edit = None;
        self.saved_popup = None;
        self.active = Some(ActiveBook { entry, id, reading, numbering: None });
        if unseen {
            let active = self.active.as_ref().unwrap();
            self.library.remember(&active.entry, id, &active.reading.probe.title).preferences = initial_preferences;
        }
        let offer_publisher = self.apply_new_numbering_defaults();
        self.restore_character_pages();
        self.fixed = None;
        self.back.clear();
        self.forward.clear();
        self.auto_turn.configure(None, Instant::now());
        self.selection = None;
        self.annotation_search = annotation_search::AnnotationSearch::default();
        self.restore_reading_menus(id);
        self.show(if offer_publisher { Screen::PublisherOffer } else { Screen::Book });
        self.record_position();
        if default_font_missing { self.notice = Some("Default reading font unavailable; using device fonts".into()); }
        if let Some(active) = &self.active {
            self.library
                .cache_author(active.id, active.reading.probe.author.as_deref());
        }
        Ok(())
    }

    fn show(&mut self, screen: Screen) {
        if !matches!(screen,Screen::Catalog(_)) { self.catalog.cancel(); }
        if !matches!(screen, Screen::ProgressSync | Screen::ProgressSyncSettings | Screen::SyncEdit(_)) {
            self.leave_progress_sync();
        }
        self.flow_popup = None;
        self.end_dialog = None;
        if screen != Screen::Definition { self.spelling_lookup = None; }
        self.cjk_selection_lookup = None;
        self.result_context = None;
        if !matches!(screen, Screen::Footnote | Screen::Definition | Screen::OnlineLookup | Screen::OnlineEdit) {
            self.footnote = None;
        }
        if screen != Screen::Hyphenate {
            self.hyphenation_edit = None;
        }
        if screen != Screen::CustomTitle { self.chapter_draft = None; }
        if !matches!(screen, Screen::OnlineLookup | Screen::OnlineEdit) {
            self.online = None;
        }
        if screen != Screen::Fixed {
            self.cancel_ocr_selection();
        }
        self.reading_panel = None;
        self.top_group = None;
        let reading_tools = matches!(screen, Screen::BookTools | Screen::FixedTools);
        self.reading_top = reading_tools;
        self.reading_bottom = reading_tools;
        if screen == Screen::ReadingControls
            || (reading_tools && !matches!(self.screen, Screen::BookTools | Screen::FixedTools))
        {
            self.quick_display = QuickDisplay::read();
        }
        if matches!(screen, Screen::DictionaryEntry | Screen::BookmarkFind) {
            self.editor = Editor::new(96);
            if screen == Screen::BookmarkFind {
                if self.screen != Screen::BookmarkMatches { self.annotation_search.origin = Some(self.screen); }
                self.editor.text = self.annotation_search.text.chars().collect();
                self.editor.caret = self.editor.text.len();
            }
        }
        if screen != Screen::Illustration {
            self.illustration = None;
        }
        if screen != Screen::ScanColumns {
            self.scan_columns = None;
        }
        if !matches!(screen, Screen::ReadFixedText | Screen::BookDetails)
            && !(matches!(screen, Screen::Illustration | Screen::ReadingHistory(_)) && self.book_details.is_some())
        {
            self.text_flow = None;
        }
        if screen != Screen::ImportAnnotations {
            self.annotation_import = None;
            self.nearby_marks = None;
        }
        if screen != Screen::ImportPosition {
            self.position_import = None;
        }
        if screen != Screen::ImportPdfAnnotations {
            self.pdf_annotation_import.clear();
        }
        if !matches!(screen, Screen::BookDetails | Screen::Illustration | Screen::ReadingHistory(_)) {
            self.book_details = None;
        }
        if screen != Screen::PageOverview {
            self.overview = None;
        }
        if screen != Screen::GeneratedContents {
            self.outline = None;
        }
        if screen == Screen::Find && self.screen != Screen::Matches {
            self.editor = Editor::new(crate::search::MAX_QUERY_BYTES);
        }
        if screen == Screen::LibraryFind {
            self.editor = Editor::new(96);
            self.editor.text = self.library_query.chars().take(96).collect();
            self.editor.caret = self.editor.text.len();
        }
        if matches!(screen, Screen::NumberingValue(_)) {
            self.editor = Editor::new(4);
        }
        if screen == Screen::GoToPage {
            self.editor = Editor::new(96);
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
        if !matches!(screen, Screen::Matches | Screen::Book | Screen::Fixed) {
            self.cancel_search();
        }
        if self.screen == Screen::Matches && screen != Screen::Matches {
            self.matches_page = self.list_page;
        }
        self.link_popup = None;
        self.screen = screen;
        self.map_window = fixed_book_map::MapWindow::default();
        self.sync_auto_turn(Instant::now());
        self.update_stats_activity(Instant::now());
        if screen == Screen::Library { self.reading_history.finish(Instant::now()); }
        self.current_contents_row = if screen == Screen::PublisherPages {
            self.visible_publisher_page()
        } else {
            self.locate_contents_row()
        };
        self.list_page = if screen == Screen::Matches {
            self.matches_page
        } else {
            self.current_contents_row.unwrap_or_default() / ROWS_PER_PAGE
        };
        self.notice = None;
    }

    fn cancel_search(&mut self) {
        self.nearby.clear();
        self.pdf_search = None;
        self.search_at = None;
    }

    fn record_position(&mut self) {
        self.record_position_at(Instant::now());
    }

    fn record_position_at(&mut self, now: Instant) {
        self.update_stats_activity(now);
        let position_changed = if let Some(fixed) = &self.fixed {
            let saved = self
                .library
                .remember(&fixed.entry, fixed.id, &fixed.entry.title);
            saved.highlight_color.get_or_insert(self.controls.highlight_color);
            let position = fixed_position(&fixed.reading);
            let changed = saved.position.as_ref() != Some(&position);
            saved.position = Some(position);
            saved.fixed_preferences = fixed.reading.preferences();
            saved.ocr_language.clone_from(&fixed.reading.ocr_language);
            changed
        } else if let Some(active) = &self.active {
            let title = &active.reading.probe.title;
            let saved = self.library.remember(&active.entry, active.id, title);
            saved.highlight_color.get_or_insert(self.controls.highlight_color);
            let position = position(&active.reading);
            let changed = saved.position.as_ref() != Some(&position);
            saved.position = Some(position);
            changed
        } else {
            return;
        };
        self.dirty = true;
        self.metadata_checkpoint.record(self.controls.metadata_save, position_changed, now);
    }

    fn mark_dirty(&mut self) {
        self.dirty = true;
        self.save_at = Some(Instant::now() + SAVE_DELAY);
    }

    fn update_stats_activity(&mut self, now: Instant) {
        let reading = self.stats_awake && self.end_dialog.is_none();
        let book = match self.screen {
            Screen::Book if reading => self.active.as_ref().map(|book| book.id),
            Screen::Illustration if reading && self.book_details.is_none() => self
                .active
                .as_ref()
                .map(|book| book.id)
                .or_else(|| self.fixed.as_ref().map(|book| book.id)),
            Screen::Fixed | Screen::ReadFixedText | Screen::ScanColumns if reading => {
                self.fixed.as_ref().map(|book| book.id)
            }
            _ => None,
        };
        let book = book.filter(|id| {
            let options = self.reading_history.options();
            options.enabled && (!options.freeze_finished
                || self.library.state(*id).is_none_or(|state| state.status() != ReadingStatus::Finished))
        });
        self.stats.set_active(book, now);
        let page = book.and_then(|id| {
            if let Some(fixed) = &self.fixed {
                let span = if fixed.reading.is_spread() {
                    fixed.reading.visible_pages().count().clamp(1, 2) as u8
                } else {
                    1
                };
                return Some((id, fixed.reading.page() as u64, span));
            }
            let reading = &self.active.as_ref()?.reading;
            let page_key = reading.probe.pace_key(reading.chapter, reading.page);
            let visible_pages = reading
                .last_visible_page()
                .saturating_sub(reading.page)
                .saturating_add(1)
                .min(2);
            Some((id, page_key, visible_pages as u8))
        });
        self.pace.set_page(page, now);
        if let Some(id) = book {
            let title = self.active.as_ref().filter(|b| b.id == id).map(|b| b.entry.title.as_str())
                .or_else(|| self.fixed.as_ref().filter(|b| b.id == id).map(|b| b.entry.title.as_str()));
            if let Some(title) = title { self.reading_history.remember_title(id, title); }
        }
        let history_page = page.map(|(book, page, _)| (book, page));
        self.reading_history.set_page(history_page, now);
    }

    fn save_library(&mut self) -> io::Result<()> {
        self.save_library_at(Instant::now())
    }

    fn save_library_at(&mut self, now: Instant) -> io::Result<()> {
        if self.dirty {
            self.library.persist()?;
            self.dirty = false;
        }
        self.save_at = None;
        self.metadata_checkpoint = metadata_save::Checkpoint::new(now);
        Ok(())
    }

    fn list_window(&self) -> (usize, Vec<Item>) {
        let first = self.list_page * ROWS_PER_PAGE;
        if let Screen::BookmarkBrowser(page) = self.screen {
            return self.bookmark_window(page, first, ROWS_PER_PAGE);
        }
        if self.screen == Screen::PublisherPages {
            return self.publisher_page_window(first, ROWS_PER_PAGE);
        }
        if self.screen == Screen::Matches {
            if let Some(fixed) = &self.fixed {
                let items = self.pdf_hits.iter().enumerate().skip(first).take(ROWS_PER_PAGE)
                    .map(|(index, hit)| {
                        Item::new(&hit.snippet, Action::PdfSearchHit(index))
                            .detail(fixed_page_label(&fixed.reading, hit.page))
                    })
                    .collect();
                return (self.pdf_hits.len(), items);
            }
            let items = self.hits.iter().enumerate().skip(first).take(ROWS_PER_PAGE)
                .map(|(index, hit)| {
                    Item::new(&hit.snippet, Action::SearchHit(index)).detail(&hit.chapter_title)
                })
                .collect();
            return (self.hits.len(), items);
        }
        let all = self.items();
        (
            all.len(),
            all.into_iter().skip(first).take(ROWS_PER_PAGE).collect(),
        )
    }

    fn has_list_shortcuts(&self, count: usize) -> bool {
        matches!(self.screen, Screen::Matches | Screen::PublisherPages) && count > ROWS_PER_PAGE
    }

    fn items(&self) -> Vec<Item> {
        if let Screen::ReadingHistory(page) = self.screen { return self.reading_history_items(page); }
        if matches!(self.screen, Screen::ProgressSync | Screen::ProgressSyncSettings) { return self.sync_items(); }
        if matches!(self.screen, Screen::Vocabulary | Screen::VocabularyWord | Screen::VocabularyOptions | Screen::VocabularyBooks) { return self.vocabulary_items(); }
        if let Screen::LibraryExport(page) = self.screen { return self.library_export_items(page); }
        if let Screen::Catalog(page) = self.screen { return self.catalog_items(page); }
        if self.screen == Screen::HiddenFlows { return self.hidden_flow_items(); }
        if matches!(self.screen, Screen::CustomContents | Screen::CustomEdits | Screen::CustomBookmarks) ||
             (matches!(self.screen, Screen::Contents | Screen::FixedContents) && self.custom_contents_enabled()) {
            return self.custom_items();
        }
        match self.screen {
            Screen::FooterSettings => return self.footer_items(),
            Screen::Gestures => return self.gestures.gestures(),
            Screen::GestureBinding(gesture) => return self.gestures.choices(gesture),
            Screen::BookMap => return self.book_map_items(),
            Screen::AutoTurn => {
                let selected = self.auto_turn.interval();
                return [0, 10, 30, 60, 120]
                    .into_iter()
                    .map(|seconds| {
                        let interval = (seconds != 0).then(|| Duration::from_secs(seconds));
                        let label = if seconds == 0 {
                            "Off".to_owned()
                        } else {
                            format!("Every {seconds} seconds")
                        };
                        let item = Item::new(label, Action::AutoTurn(interval));
                        if selected == interval {
                            item.detail("Selected")
                        } else {
                            item
                        }
                    })
                    .collect();
            }
            Screen::OnlineHistory | Screen::ClearOnlineHistory => return self.online_history_items(),
            Screen::SearchHistory => {
                return self
                    .lookup_history
                    .searches
                    .iter()
                    .map(|query| {
                        let item = Item::new(&query.text, Action::RepeatSearch(query.clone()));
                        if query.regular_expression { item.detail("Regular expression") } else { item }
                    })
                    .collect();
            }
            Screen::DictionaryHistory => {
                let words = &self.lookup_history.dictionary;
                return words
                    .iter()
                    .map(|word| Item::new(word, Action::LookUp(word.clone())))
                    .collect();
            }
            Screen::BookmarkMatches => return self.annotation_results(),
            _ => {}
        }
        if self.screen == Screen::Library {
            return self.library_items().collect();
        }
        if let Some(items) = self.organization_items() {
            return items;
        }
        if self.screen == Screen::LocationHistory {
            return self
                .back
                .iter()
                .enumerate()
                .rev()
                .map(|(index, location)| {
                    let position = &location.position;
                    let action = Action::ReturnToHistory(index);
                    if let Some(fixed) = &self.fixed {
                        Item::new(fixed_page_label(&fixed.reading, position.page), action)
                    } else {
                        let chapter = self.active.as_ref().and_then(|active| {
                            active.reading.probe.chapter_entries().get(position.chapter)
                        });
                        Item::new(chapter.map_or("Chapter", |chapter| &chapter.label), action)
                            .detail(format!(
                                "Chapter {} · Page {}",
                                position.chapter + 1,
                                position.page + 1
                            ))
                    }
                })
                .collect();
        }
        if self.screen == Screen::ReadingControls {
            let mut items = self.reading_settings_items();
            items.push(self.preferred_dictionary_item());
            if self.active.is_some() || self.fixed.is_some() {
                items.push(Item::new("Book details", Action::OrganizeCurrent));
                items.push(
                    Item::new("Import KOReader position", Action::PreviewKoReaderPosition)
                        .detail("Preview the saved position for this book"),
                );
            }
            return items;
        }
        if self.screen == Screen::EndActionSettings {
            return self.end_action_rows();
        }
        if self.screen == Screen::MetadataSaveSettings {
            return self.metadata_save_items();
        }
        if self.screen == Screen::Dictionaries { return self.source_rows(); }
        if let Screen::DictionaryOptions(i) = self.screen { return self.source_options(i); }
        if self.screen == Screen::ReadingData {
            let mut items = vec![
                Item::new("Export highlights", Action::LibraryExport(library_export::Command::Open))
                    .detail("From History, chosen books or collections"),
                Item::new("Progress sync", Action::ProgressSync(progress_sync::Command::Open)).detail("Optional WebDAV server"),
                Item::new("Back up reading data", Action::ExportBackup)
                    .detail("Books, vocabulary, history and chosen local preferences"),
                Item::new("Restore a backup", Action::PreviewBackup)
                    .detail("Copy a backup to reader/data/reader-import.json"),
                Item::new("Clear recognized text", Action::Show(Screen::ClearOcr))
                    .detail("Clear saved OCR text from document pages"),
            ];
            if self.open_book_id().is_some() {
                items.push(Item::new("Import a copied KOReader position", Action::PreviewCopiedPosition)
                    .detail("From reader/data/koreader-progress.lua"));
            }
            items.push(Item::new("Reading statistics", Action::ReadingHistory(reading_history::Command::Open(None)))
                .detail("Daily history, calendar, and book totals"));
            return items;
        }
        if self.screen == Screen::ClearOcr {
            return vec![
                Item::new("Clear OCR cache", Action::ClearOcr),
                Item::new("Keep OCR cache", Action::Show(Screen::ReadingData)),
            ];
        }
        if self.screen == Screen::RestoreBackup { return self.reading_backup_items(); }
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
                                    import.sidecar.last_page.unwrap_or(0),
                                    fixed.reading.count()
                                )),
                            Item::new(import.source, Action::Heading)
                                .detail("KOReader sampled fingerprint and page count match"),
                        ];
                    }
                    let mut rows = match &import.target {
                        Ok(target) => {
                            let chapter = self.active.as_ref().and_then(|active| {
                                active.reading.probe.chapter_entries().get(target.chapter)
                            });
                            vec![
                                Item::new("Apply reading position", Action::ApplyKoReaderPosition)
                                    .detail(
                                        chapter.map_or("EPUB reading position", |chapter| {
                                            &chapter.label
                                        }),
                                    ),
                                Item::new("Edition check", Action::Heading).detail(
                                    "Reading position and KOReader sampled fingerprint match",
                                ),
                            ]
                        }
                        Err(error) => {
                            vec![Item::new("Position unresolved", Action::Heading).detail(error)]
                        }
                    };
                    if let Some(percent) = import.sidecar.percent_finished {
                        rows.push(Item::new(
                            format!("KOReader progress {:.1}%", percent * 100.0),
                            Action::Heading,
                        ));
                    }
                    rows.push(Item::new(import.source, Action::Heading));
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
            && let Some(import) = &self.nearby_marks
        {
            return import.items();
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
            let items = vec![
                Item::new("Read passage", passage),
                Item::new("Extend highlight", Action::ExtendHighlight),
                Item::new("Edit note", Action::EditNote),
                Item::new("Remove highlight", Action::Show(Screen::RemoveHighlight)),
            ];
            return items;
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
                    if fixed.entry.format == DocumentFormat::Pdf
                        && !fixed.reading.is_encrypted_pdf()
                    {
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
                Screen::FixedContents => std::iter::once(Item::new("Custom contents", Action::Show(Screen::CustomContents))).chain(fixed
                    .reading
                    .outline()
                    .iter()
                    .map(|entry| {
                        Item::new(
                            format!("{}{}", "  ".repeat(entry.depth.min(6)), entry.title),
                            entry.page.map_or(Action::Heading, Action::FixedPage),
                        )
                    })
                    ).collect(),
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
            Screen::Contents => {
                let mut items = Vec::new();
                if active.entry.format == DocumentFormat::Epub {
                    items.push(
                        Item::new("Generated outline", Action::ShowGeneratedContents)
                            .detail("Build from headings in the book"),
                    );
                }
                items.push(Item::new("Custom contents", Action::Show(Screen::CustomContents)));
                if reading.probe.toc.is_empty() {
                    items.extend(reading.probe.chapter_entries().iter().map(|chapter| {
                        Item::new(
                            &chapter.label,
                            Action::Jump(Destination {
                                path: chapter.href.clone(),
                                fragment: None,
                            }),
                        )
                    }));
                } else {
                    items.extend(reading.probe.toc.iter().map(|entry| {
                        Item::new(
                            format!("{}{}", "  ".repeat(entry.depth.min(4)), entry.label),
                            entry
                                .destination
                                .clone()
                                .map_or(Action::Heading, Action::Jump),
                        )
                    }));
                }
                items
            }
            Screen::PageNumbering => self.numbering_items(),
            Screen::NumberingDefaults => self.numbering_defaults_items(),
            Screen::PublisherOffer => self.publisher_offer_items(),
            Screen::PublisherPages => self.publisher_page_window(0, usize::MAX).1,
            Screen::GeneratedContents => self.outline.as_ref().map_or_else(Vec::new, |outline| {
                outline
                    .contents
                    .entries
                    .iter()
                    .enumerate()
                    .map(|(index, entry)| {
                        Item::new(
                            format!("{}{}", "  ".repeat(entry.depth.min(4)), entry.title),
                            Action::GeneratedEntry(index),
                        )
                    })
                    .collect()
            }),
            Screen::Links => reading
                .links()
                .into_iter()
                .map(|(label, target)| Item::new(label, Action::Jump(target)))
                .collect(),
            Screen::Bookmarks => self.bookmark_items(active.id),
            Screen::Typography => {
                let settings = reading.config.settings;
                let mut items = text_items(settings, reading.config.scroll);
                if !reading.probe.is_vertical()
                    && settings.hyphenation_mode() == crate::HyphenationMode::Automatic
                {
                    let enabled = self.user_hyphenation.enabled;
                    items.push(Item::new("Custom hyphenation rules", Action::SetUserHyphenation(!enabled))
                        .detail(if enabled { "On" } else { "Off" }));
                }
                if reading.probe.is_vertical() {
                    items.retain(|item| match item.action {
                        Action::ReadingMode(_) => false,
                        Action::Text(next) => {
                            next.alignment == settings.alignment
                                && next.hyphenation_mode() == settings.hyphenation_mode()
                        }
                        _ => true,
                    });
                }
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
            Screen::DocumentDefaults => Vec::new(),
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
            Screen::Highlights => {
                let mut items = vec![
                    Item::new("Import KOReader annotations", Action::PreviewKoReader)
                        .detail("Copy a JSON export to reader/data/koreader-import.json"),
                ];
                if active.entry.format == DocumentFormat::Epub {
                    items.insert(
                        0,
                        Item::new("Import marks saved beside this book", Action::PreviewNearbyMarks)
                            .detail("Preview KOReader bookmarks, highlights and notes"),
                    );
                }
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
        let Some(state) = self.library.state(id) else { return Vec::new() };
        let mut items: Vec<_> = state.bookmarks.iter().enumerate()
            .map(|(index, bookmark)| Item::new(&bookmark.label, Action::Bookmark(index)))
            .collect();
        if !items.is_empty() || !state.highlights.is_empty() || !state.pdf_highlights.is_empty() {
            items.insert(0, Item::new("Find bookmarks", Action::Show(Screen::BookmarkFind)));
        }
        items
    }

    fn book_map_items(&self) -> Vec<Item> {
        let Some(id) = self.open_book_id() else {
            return Vec::new();
        };
        let state = self.library.state(id);
        let Some(active) = &self.active else {
            return Vec::new();
        };
        let reading = &active.reading;
        reading
            .probe
            .chapter_entries()
            .iter()
            .enumerate()
            .map(|(index, chapter)| {
                let marks = state.map_or(0, |state| {
                    state
                        .bookmarks
                        .iter()
                        .filter(|mark| mark.position.chapter == index)
                        .count()
                });
                let highlights = state.map_or(0, |state| {
                    state
                        .highlights
                        .iter()
                        .filter(|mark| mark.selection.start.chapter == chapter.href)
                        .count()
                });
                Item::new(
                    &chapter.label,
                    Action::Jump(Destination {
                        path: chapter.href.clone(),
                        fragment: None,
                    }),
                )
                .detail({
                    let detail = map_detail(index == reading.chapter, marks, highlights);
                    reading.hidden_chapter(index).map_or(detail.clone(), |all|
                        format!("{} · {detail}", if all { "Hidden" } else { "Includes hidden pages" }))
                })
            })
            .collect()
    }

    fn organization_items(&self) -> Option<Vec<Item>> {
        let mut items = Vec::new();
        match self.screen {
            Screen::OrganizeLibrary => {
                items.push(Item::new(
                    "Bookmark browser",
                    Action::BookmarkBrowser(bookmark_browser::Command::Open),
                ));
                items.push(Item::new(
                    "Favorites",
                    Action::ListBooks(BookList::Favorites),
                ));
                items.push(Item::new(
                    "Recently read",
                    Action::ListBooks(BookList::Recent),
                ));
                items.push(Item::new("Authors", Action::ListBooks(BookList::Authors)));
                items.extend(ReadingStatus::ALL.into_iter().map(|status| {
                    Item::new(status.label(), Action::ListBooks(BookList::Status(status)))
                }));
                items.push(Item::new("Online catalogs",Action::Catalog(opds::Command::Open)));
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
                BookList::Recent => {
                    items.extend(self.library.recently_read().map(tracked_book_item))
                }
                BookList::Authors => {
                    let authors: BTreeSet<_> = self
                        .library
                        .entries()
                        .iter()
                        .filter_map(|book| book.author.as_deref())
                        .collect();
                    items.extend(authors.into_iter().map(|name| {
                        Item::new(name, Action::ListBooks(BookList::Author(Some(name.into()))))
                    }));
                    if self
                        .library
                        .entries()
                        .iter()
                        .any(|book| book.author.is_none())
                    {
                        items.push(Item::new(
                            "Unknown author",
                            Action::ListBooks(BookList::Author(None)),
                        ));
                    }
                }
                BookList::Author(author) => items.extend(
                    self.library
                        .entries()
                        .iter()
                        .filter(|entry| entry.author == *author)
                        .cloned()
                        .map(library_book_item),
                ),
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
                                .map(library_book_item),
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
                let description = book.entry.author.as_deref().map_or_else(
                    || book.entry.path.to_string_lossy().into_owned(),
                    str::to_owned,
                );
                items.push(Item::new(&book.entry.title, Action::Heading).detail(description));
                if let Some(metadata) = &book.metadata {
                    if metadata.has_cover {
                        items.push(Item::new("View cover", Action::ViewCover));
                    }
                    items.extend(metadata.series.iter().map(|series| {
                        Item::new(&series.name, Action::Heading).detail(
                            series.position.as_ref().map_or_else(
                                || "Series".into(),
                                |position| format!("Series · {position}"),
                            ),
                        )
                    }));
                }
                let seconds = self.stats.seconds_read(book.id, Instant::now());
                items
                    .push(Item::new("Reading time", Action::ReadingHistory(reading_history::Command::Open(Some(book.id)))).detail(reading_time(seconds)));
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

    fn activate(&mut self, action: Action) -> Result<()> {
        match action {
            Action::Catalog(command) => self.catalog_action(command)?,
            Action::BookmarkBrowser(command) => self.activate_bookmark_browser(command)?,
            Action::ReadingHistory(command) => self.activate_reading_history(command)?,
            Action::Flow(action) => self.flow_action(action)?,
            Action::ToggleNonlinear => {
                let active = self.active.as_mut().ok_or("Open a book first")?;
                if active.reading.config.scroll || active.reading.config.facing_pages {
                    return Err("Use single-page mode to hide non-linear fragments".into());
                }
                active.reading.set_hide_nonlinear(!active.reading.config.hide_nonlinear);
                let id = active.id;
                let enabled = active.reading.config.hide_nonlinear;
                self.library.state_mut(id).ok_or("This book is unavailable")?.hide_nonlinear = Some(enabled);
                self.mark_book_changed();
            }
            Action::LibraryExport(command) => self.activate_library_export(command)?,
            Action::Open(entry) => self.open_book(entry)?,
            Action::Browse(directory) => self.browse_to(directory)?,
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
                self.browser.first_page();
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
            Action::ViewCover => self.view_cover()?,
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
                let resized = if controls.overlap_footer != self.controls.overlap_footer {
                    self.active
                        .as_ref()
                        .map(|book| {
                            let reading = &book.reading;
                            let mut config = reading.config.clone();
                            config.settings.height = if controls.overlap_footer {
                                HEIGHT
                            } else {
                                PAGE_HEIGHT
                            };
                            Reading::restore(
                                config,
                                reading.chapter,
                                reading.page,
                                reading.anchor().as_ref(),
                            )
                        })
                        .transpose()?
                } else {
                    None
                };
                if controls.overlap_footer != self.controls.overlap_footer
                    && let Some(fixed) = &mut self.fixed
                {
                    let height = if controls.overlap_footer {
                        HEIGHT
                    } else {
                        PAGE_HEIGHT
                    };
                    fixed
                        .reading
                        .resize(height, || controls.save(&self.controls_path))?;
                    self.record_position();
                } else {
                    controls.save(&self.controls_path)?;
                }
                if let (Some(reading), Some(book)) = (resized, &mut self.active) {
                    book.reading = reading;
                    self.record_position();
                }
                if controls.page_refresh != self.controls.page_refresh {
                    self.pages_since_refresh = 0;
                }
                if controls.night_pages != self.controls.night_pages
                    || controls.orientation != self.controls.orientation
                    || controls.palette != self.controls.palette
                {
                    self.refresh_display = true;
                }
                if controls.metadata_save != self.controls.metadata_save {
                    self.metadata_checkpoint.cancel();
                    if self.dirty {
                        self.metadata_checkpoint.record(controls.metadata_save, false, Instant::now());
                    }
                }
                self.controls = controls;
            }
            Action::BindGesture(gesture, binding) => {
                self.gestures.set(gesture, binding)?;
                self.show(Screen::Gestures);
                self.list_page = Gesture::ALL
                    .iter()
                    .position(|entry| entry.0 == gesture)
                    .unwrap()
                    / ROWS_PER_PAGE;
            }
            Action::SetDisplayLevel(control, level) => self.quick_display.set(control, level)?,
            Action::RefreshDisplay => {
                self.pages_since_refresh = 0;
                self.refresh_display = true;
            }
            Action::HighlightStyle => self.open_highlight_styles(None),
            Action::ReadAlarm => { self.open_read_alarm(); }
            Action::SaveBookData => {
                self.save_library()?;
                self.notice = Some("Reading data saved".into());
            }
            Action::AutoTurn(interval) => {
                let now = Instant::now();
                self.auto_turn.configure(interval, now);
                self.sync_auto_turn(now);
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
                self.ocr = None;
                with_storage(|| match fs::remove_dir_all(&self.ocr_root) {
                    Ok(()) => Ok(()),
                    Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
                    Err(error) => Err(error),
                })?;
                if let Some(fixed) = &mut self.fixed {
                    fixed.reading.forget_ocr();
                }
                self.show(Screen::ReadingData);
                self.notice = Some("OCR cache cleared".into());
            }
            Action::PreviewBackup => self.preview_backup()?,
            Action::MergeBackup => self.restore_backup()?,
            Action::ToggleBackupComponent(index) => self.toggle_backup_component(index)?,
            Action::PreviewNearbyMarks => self.preview_nearby_marks()?,
            Action::ApplyNearbyMarks => self.apply_nearby_marks()?,
            Action::PreviewKoReader => self.preview_koreader()?,
            Action::ApplyKoReader => self.apply_koreader()?,
            Action::PreviewKoReaderPosition => self.preview_koreader_position(false)?,
            Action::PreviewCopiedPosition => self.preview_koreader_position(true)?,
            Action::ApplyKoReaderPosition => self.apply_koreader_position()?,
            Action::PreviewPdfAnnotations => self.preview_pdf_annotations()?,
            Action::ApplyPdfAnnotations => self.apply_pdf_annotations()?,
            Action::ExportAnnotatedPdf => self.export_annotated_pdf()?,
            Action::Show(Screen::Library) => {
                self.save()?;
                self.active = None;
                self.numbering_scan = None;
                self.fixed = None;
                self.touch_locked = false;
                self.ocr = None;
                self.selection = None;
                self.back.clear();
                self.forward.clear();
                self.auto_turn.configure(None, Instant::now());
                self.show(Screen::Library);
            }
            Action::Show(screen) => self.show(screen),
            Action::Jump(target) => self.follow(target)?,
            Action::Custom(action) => self.custom_action(action)?,
            Action::SetCharacterPages(stride) => self.set_character_pages(stride)?,
            Action::AcceptPublisherNumbers => self.accept_publisher_numbers()?,
            Action::CharacterPage(row) => self.follow_character_page(row)?,
            Action::TogglePublisherFooter => self.toggle_publisher_labels(false)?,
            Action::TogglePublisherMargins => self.toggle_publisher_labels(true)?,
            Action::ShowGeneratedContents => self.start_generated_contents()?,
            Action::GeneratedEntry(index) => self.follow_generated_entry(index)?,
            Action::ReturnToLocation => {
                self.return_to_location(self.back.len().saturating_sub(1))?;
            }
            Action::ReturnToHistory(index) => self.return_to_location(index)?,
            Action::NextLocation => self.next_location()?,
            Action::OpenPreviousDocument => self.open_previous_document()?,
            Action::Wikipedia => self.prepare_online(true, false)?,
            Action::TranslatePage => self.prepare_online(false, true)?,
            Action::RepeatOnline(index) => self.repeat_online(index)?,
            Action::ToggleOnlineHistory => self.update_lookup_history(|h| h.online_languages.forget_queries = !h.online_languages.forget_queries)?,
            Action::ClearOnlineHistory => {self.update_lookup_history(|h| h.online.clear())?; self.show(Screen::OnlineHistory);},
            Action::LookUp(word) => self.look_up(&word)?,
            Action::ProgressSync(command) => self.progress_sync_command(command)?,
            Action::Vocabulary(command) => self.vocabulary_command(command)?,
            Action::RepeatSearch(query) => {
                if query.regular_expression && self.active.is_none() {
                    return Err("Patterns require a reflowable book".into());
                }
                self.find_regex = query.regular_expression;
                self.find_match_case = query.match_case;
                self.find_ignore_accents = query.ignore_accents;
                self.start_document_search(&query.text)?;
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
            Action::SetFixedZoom(level) => {
                let fixed = self
                    .fixed
                    .as_mut()
                    .ok_or("Open a fixed-page document first")?;
                fixed.reading.set_zoom(level)?;
                self.show(Screen::Fixed);
                self.record_position();
            }
            Action::SetFixedRotation(turns) => {
                let fixed = self
                    .fixed
                    .as_mut()
                    .ok_or("Open a fixed-page document first")?;
                fixed.reading.set_rotation(turns)?;
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
            Action::FixedDither(enabled) => {
                let fixed = self
                    .fixed
                    .as_mut()
                    .ok_or("Open a fixed-page document first")?;
                fixed.reading.set_dither(enabled);
                self.record_position();
            }
            Action::FixedGap(gap) => {
                let fixed = self
                    .fixed
                    .as_mut()
                    .ok_or("Open a fixed-page document first")?;
                fixed.reading.set_continuous_gap(gap)?;
                self.record_position();
            }
            Action::ForceOcr(enabled) => {
                self.cancel_ocr_selection();
                let fixed = self.fixed.as_mut().ok_or("Open a PDF or DjVu first")?;
                fixed.reading.set_force_ocr(enabled)?;
                self.pdf_selection = None;
                self.record_position();
            }
            Action::StraightenScan(limit) => {
                let fixed = self.fixed.as_mut().ok_or("Open a PDF or DjVu first")?;
                fixed.reading.set_straighten(limit)?;
                self.record_position();
            }
            Action::ScanWhiteLevel(level) => {
                let fixed = self
                    .fixed
                    .as_mut()
                    .ok_or("Open a fixed-page document first")?;
                fixed.reading.set_scan_white_level(level)?;
                self.record_position();
            }
            Action::ScanColumns => self.open_scan_columns()?,
            Action::Overview => {
                let (chapter, current) = if let Some(fixed) = &self.fixed {
                    (None, fixed.reading.page())
                } else {
                    let reading = &self.active.as_ref().ok_or("Open a book first")?.reading;
                    (Some(reading.chapter), reading.page)
                };
                self.update_overview(chapter, Some(current / OVERVIEW_PAGES * OVERVIEW_PAGES))?;
                self.show(Screen::PageOverview);
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
            Action::ReadFixedText => self.read_fixed_text(false)?,
            Action::ReflowScan => self.read_fixed_text(true)?,
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
            Action::ToggleBookmark => self.toggle_bookmark()?,
            Action::SetUserHyphenation(enabled) => self.set_user_hyphenation(enabled)?,
            Action::Text(settings) => {
                let active = self.active.as_mut().ok_or("No open book")?;
                active.reading.reflow(settings)?;
                let preferences = &mut self
                    .library
                    .remember(&active.entry, active.id, &active.reading.probe.title)
                    .preferences;
                preferences.font_size = Some(settings.font_size);
                preferences.render_dpi = Some(settings.render_dpi);
                preferences.margin = Some(settings.margin);
                preferences.margin_sides = settings.margin_sides;
                preferences.margin_left = settings.margin_left;
                preferences.margin_right = settings.margin_right;
                preferences.link_vertical_margins = settings.link_vertical_margins;
                preferences.margin_top = settings.margin_top;
                preferences.margin_bottom = settings.margin_bottom;
                preferences.line_height = Some(settings.line_height);
                preferences.word_spacing = settings.word_spacing;
                preferences.word_expansion = Some(settings.word_expansion);
                preferences.letter_spacing = settings.letter_spacing;
                preferences.cjk_width_percent = (settings.cjk_width_percent != 100).then_some(settings.cjk_width_percent);
                preferences.scroll = active.reading.config.scroll;
                preferences.alignment = settings.alignment;
                preferences.bold_text = settings.bold_text;
                preferences.text_contrast = settings.text_contrast;
                preferences.font_gamma_index = Some(settings.font_gamma_index);
                preferences.font_weight_delta = Some(settings.font_weight_delta);
                preferences.publisher_styles = Some(settings.publisher_styles);
                preferences.font_kerning = Some(settings.font_kerning);
                preferences.font_hinting = settings.font_hinting;
                preferences.smooth_images = Some(settings.smooth_images);
                preferences.invert_night_images = settings.invert_night_images;
                preferences.override_fonts = settings.override_fonts;
                preferences.disable_soft_hyphens = settings.disable_soft_hyphens;
                preferences.automatic_hyphenation = Some(settings.automatic_hyphenation);
                self.show(Screen::Book);
                self.record_position();
            }
            Action::ShowFonts => {
                with_storage(|| fs::create_dir_all(&self.fonts_directory))?;
                self.available_fonts = reading_fonts::available(&self.fonts_directory)?;
                self.show(Screen::ReadingFonts);
            }
            Action::ShowDictionaries => self.open_sources()?,
            Action::DictionarySource(action) => self.change_source(action)?,
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
            Action::DocumentDefaults(command) => self.activate_document_defaults(command)?,
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
            Action::FacingPages(facing_pages) => {
                let active = self.active.as_mut().ok_or("No open book")?;
                active.reading.set_facing_pages(facing_pages)?;
                self.library
                    .remember(&active.entry, active.id, &active.reading.probe.title)
                    .preferences
                    .facing_pages = facing_pages;
                self.show(Screen::Book);
                self.record_position();
            }
            Action::StepNearby(direction) => self.begin_nearby(direction, true)?,
            Action::SearchHit(index) => {
                let hit = self.hits.get(index).ok_or("That result is unavailable")?;
                let end = hit.end.clone();
                let previous = position(&self.active.as_ref().ok_or("No open book")?.reading);
                let target = ReadingPosition {
                    chapter: hit.chapter,
                    page: 0,
                    anchor: Some(hit.anchor.clone()),
                };
                let first_result = self.search_result.is_none();
                self.restore_position(&target)?;
                if let Some(active) = &self.active
                    && let Some(start) = target.anchor
                {
                    self.result_selection = active.reading.probe.select_range(&start, &end);
                }
                if first_result {
                    self.push_history(previous);
                }
                self.search_result = Some(index);
                self.matches_page = index / ROWS_PER_PAGE;
            }
            Action::PdfSearchHit(index) => {
                let hit = self
                    .pdf_hits
                    .get(index)
                    .ok_or("That result is unavailable")?;
                let (page, target) = (hit.page, FixedJump::regions(&hit.bounds));
                let first_result = self.search_result.is_none();
                let previous = self.jump_fixed(page, target)?;
                if first_result {
                    self.push_history(previous);
                }
                self.search_result = Some(index);
                self.matches_page = index / ROWS_PER_PAGE;
            }
            Action::PdfHighlight(index) => {
                let fixed = self.fixed.as_ref().ok_or("No open PDF")?;
                let mark = self
                    .library
                    .state(fixed.id)
                    .and_then(|state| state.pdf_highlights.get(index))
                    .ok_or("That highlight is unavailable")?;
                self.follow_fixed(mark.page, FixedJump::regions(&mark.bounds))?;
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
            Action::ExtendHighlight => self.begin_extending_highlight()?,
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
                let text: Vec<_> = note.as_deref().unwrap_or("").chars().collect();
                self.editor = Editor::new(crate::library::MAX_NOTE_BYTES);
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
                self.mark_book_changed();
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
        let base = self.device_reading_settings();
        let active = self.active.as_mut().ok_or("Open a book first")?;
        let position = position(&active.reading);
        let mut config = active.reading.config.clone();
        config.settings = preferences.apply_to(base);
        config.scroll = preferences.scroll;
        config.facing_pages = preferences.facing_pages;
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
        let preferences = self.current_text_preferences()?;
        let mut profiles = self.profiles.clone();
        reading_profiles::insert(&mut profiles, name, preferences)?;
        reading_profiles::save(&self.profiles_path, &profiles)?;
        self.profiles = profiles;
        self.show(Screen::ReadingProfiles);
        self.notice = Some("Reading profile saved".into());
        Ok(())
    }

    fn organize_book(&mut self, mut entry: LibraryBook, back: Screen) -> Result<()> {
        let (id, metadata) = with_storage(|| {
            Ok((
                self.library.identify(&entry)?,
                self.library.metadata(&entry).ok(),
            ))
        })?;
        if let Some(metadata) = &metadata {
            if !metadata.title.is_empty() {
                entry.title.clone_from(&metadata.title);
            }
            entry.author.clone_from(&metadata.author);
        }
        self.show(Screen::BookDetails);
        self.book_details = Some(BookDetails {
            entry,
            id,
            back,
            close_document: false,
            metadata,
        });
        Ok(())
    }

    fn view_cover(&mut self) -> Result<()> {
        let details = self.book_details.as_ref().ok_or("Choose a book first")?;
        let image = with_storage(|| {
            if self.library.identify(&details.entry)? != details.id {
                return Err(io::Error::other(
                    "The publication changed. Reopen its book details.",
                ));
            }
            let path = self.library.absolute_path(&details.entry.path)?;
            let mut book = Book::open(&path).map_err(io::Error::other)?;
            let cover = book.cover().cloned().ok_or_else(|| io::Error::other("The book has no supported cover"))?;
            let (_, bytes) = book.read_cover().map_err(io::Error::other)?
                .ok_or_else(|| io::Error::other("The book has no supported cover"))?;
            if cover.kind == crate::epub::CoverKind::Svg {
                return crate::svg_image::original_package_image(&mut book, &cover.path, &bytes)
                    .map_err(io::Error::other);
            }
            crate::parley_layout::original_illustration(&bytes).map_err(io::Error::other)
        })?;
        let view = FixedReading::inspect_image(image)?;
        self.show(Screen::Illustration);
        self.illustration = Some(view);
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
        let already_bookmarked = self.page_is_bookmarked();
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
        if !already_bookmarked {
            if state.bookmarks.len() >= 256 {
                return Err("This book has reached its bookmark limit".into());
            }
            state.bookmarks.push(Bookmark {
                created_at: crate::mark_time::now(),
                label,
                position,
            });
            self.mark_book_changed();
        }
        self.show(screen);
        self.notice = Some("Page bookmarked".into());
        Ok(())
    }

    fn open_book_id(&self) -> Option<u64> {
        self.fixed
            .as_ref()
            .map(|book| book.id)
            .or_else(|| self.active.as_ref().map(|book| book.id))
    }

    fn current_position(&self) -> Option<ReadingPosition> {
        self.fixed
            .as_ref()
            .map(|book| fixed_position(&book.reading))
            .or_else(|| self.active.as_ref().map(|book| position(&book.reading)))
    }

    fn page_is_bookmarked(&self) -> bool {
        self.current_bookmark_index().is_some()
    }

    fn current_bookmark_index(&self) -> Option<usize> {
        let book = self.library.state(self.open_book_id()?)?;
        if let Some(fixed) = &self.fixed {
            return book
                .bookmarks
                .iter()
                .position(|mark| mark.position.page == fixed.reading.page());
        }
        let reading = &self.active.as_ref()?.reading;
        book.bookmarks.iter().position(|mark| {
            let position = &mark.position;
            position.chapter == reading.chapter
                && position
                    .anchor
                    .as_ref()
                    .and_then(|anchor| reading.probe.page_for(anchor))
                    .unwrap_or(position.page)
                    == reading.page
        })
    }

    fn toggle_bookmark(&mut self) -> Result<()> {
        let Some(index) = self.current_bookmark_index() else {
            return self.add_bookmark();
        };
        let state = if let Some(fixed) = &self.fixed {
            self.library
                .remember(&fixed.entry, fixed.id, &fixed.entry.title)
        } else {
            let active = self.active.as_ref().ok_or("Open a document first")?;
            self.library
                .remember(&active.entry, active.id, &active.reading.probe.title)
        };
        state.bookmarks.remove(index);
        self.mark_book_changed();
        self.show(if self.fixed.is_some() {
            Screen::Fixed
        } else {
            Screen::Book
        });
        self.notice = Some("Bookmark removed".into());
        Ok(())
    }

    fn has_previous_document(&self) -> bool {
        let current = self.open_book_id();
        self.library
            .recently_read()
            .any(|book| Some(book.id) != current)
    }

    fn open_previous_document(&mut self) -> Result<()> {
        let current = self.open_book_id();
        let entry = self
            .library
            .recently_read()
            .filter(|book| Some(book.id) != current)
            .find_map(|book| self.locate_tracked(book.id).ok())
            .ok_or("No previous document in the library")?;
        self.open_book(entry)
    }

    fn follow(&mut self, target: Destination) -> Result<()> {
        let active = self.active.as_mut().ok_or("No open book")?;
        let previous = position(&active.reading);
        let Some(chapter) = active.reading.probe.chapter_index(&target.path) else {
            if target.fragment.is_none() && active.reading.probe.links_to_image(&target.path) {
                let image = active.reading.open_illustration(&target.path)?;
                let view = FixedReading::inspect_image(image)?;
                self.show(Screen::Illustration);
                self.illustration = Some(view);
                return Ok(());
            }
            return Err("That link is outside the reading order".into());
        };
        active.reading.jump(chapter, target.fragment.as_deref())?;
        self.finish_link_jump(previous);
        Ok(())
    }

    fn finish_link_jump(&mut self, previous: ReadingPosition) {
        self.push_history(previous);
        self.selection = None;
        self.result_selection = None;
        self.search_result = None;
        self.show(Screen::Book);
        self.record_position();
    }

    fn start_generated_contents(&mut self) -> Result<()> {
        if self
            .active
            .as_ref()
            .is_none_or(|active| active.entry.format != DocumentFormat::Epub)
        {
            return Err("Open an EPUB book to build an outline".into());
        }
        let now = Instant::now();
        self.show(Screen::GeneratedContents);
        self.outline = Some(OutlineScan {
            contents: GeneratedContents::default(),
            deadline: Some(now + SEARCH_DELAY),
            next_refresh: now,
        });
        Ok(())
    }

    fn step_generated_contents(&mut self, now: Instant) -> Result<Change> {
        let outline = self
            .outline
            .as_mut()
            .ok_or("There is no outline to build")?;
        let active = self.active.as_ref().ok_or("No open book")?;
        let previous = outline.contents.entries.len();
        let finished = with_storage(|| {
            let mut book = Book::open(&active.reading.config.book).map_err(io::Error::other)?;
            outline.contents.step(&mut book).map_err(io::Error::other)
        })?;
        if finished {
            outline.deadline = None;
            self.notice = if outline.contents.truncated {
                Some("Reached the outline size limit".into())
            } else if outline.contents.skipped > 0 {
                Some("Some chapters could not be scanned".into())
            } else {
                None
            };
            return Ok(Change::Partial);
        }
        let now = now.max(Instant::now());
        outline.deadline = Some(now + SEARCH_DELAY);
        if outline.contents.entries.len() != previous && now >= outline.next_refresh {
            outline.next_refresh = now + SEARCH_REFRESH;
            Ok(Change::Partial)
        } else {
            Ok(Change::None)
        }
    }

    fn follow_generated_entry(&mut self, index: usize) -> Result<()> {
        let target = self
            .outline
            .as_ref()
            .and_then(|outline| outline.contents.entries.get(index))
            .ok_or("That outline entry is unavailable")?
            .target
            .clone();
        let active = self.active.as_mut().ok_or("No open book")?;
        let previous = position(&active.reading);
        match target {
            OutlineTarget::Chapter(chapter) => active.reading.jump(chapter, None)?,
            OutlineTarget::Heading(chapter, anchor) => {
                active.reading.jump_to_anchor(chapter, &anchor)?
            }
        }
        self.finish_link_jump(previous);
        Ok(())
    }

    fn follow_fixed(&mut self, page: usize, target: Option<FixedJump>) -> Result<()> {
        let positioned = target.is_some();
        let previous = self.jump_fixed(page, target)?;
        if page != previous.position.page
            || positioned && self.current_history_location().as_ref() != Some(&previous)
        {
            self.push_history(previous);
        }
        Ok(())
    }

    fn jump_fixed(&mut self, page: usize, target: Option<FixedJump>) -> Result<HistoryLocation> {
        let fixed = self.fixed.as_mut().ok_or("No open document")?;
        let previous = HistoryLocation {
            position: fixed_position(&fixed.reading),
            scan_point: fixed.reading.history_point(),
        };
        fixed.reading.jump_to(page, target.as_ref())?;
        self.search_result = None;
        self.pdf_selection = None;
        self.pdf_result_regions.clear();
        if let Some(FixedJump::Regions(bounds)) = target {
            self.pdf_result_regions
                .extend(bounds.into_iter().map(|rect| (page, rect)));
        }
        self.show(Screen::Fixed);
        self.record_position();
        Ok(previous)
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
        self.search_result = None;
        self.show(Screen::Book);
        self.record_position();
        Ok(())
    }

    fn current_history_location(&self) -> Option<HistoryLocation> {
        Some(HistoryLocation {
            position: self.current_position()?,
            scan_point: self
                .fixed
                .as_ref()
                .and_then(|book| book.reading.history_point()),
        })
    }

    fn push_history(&mut self, previous: impl Into<HistoryLocation>) {
        self.forward.clear();
        Self::remember_location(&mut self.back, previous.into());
    }

    fn remember_location(history: &mut Vec<HistoryLocation>, entry: HistoryLocation) {
        if history.last() == Some(&entry) {
            return;
        }
        if history.len() == 64 {
            history.remove(0);
        }
        history.push(entry);
    }

    fn return_to_location(&mut self, index: usize) -> Result<()> {
        let previous = self
            .back
            .get(index)
            .cloned()
            .ok_or("No previous location")?;
        let current = self
            .current_history_location()
            .ok_or("Open a document first")?;
        self.go_to_location(&previous)?;
        Self::remember_location(&mut self.forward, current);
        for location in self.back.drain(index..).rev() {
            if location != previous {
                Self::remember_location(&mut self.forward, location);
            }
        }
        Ok(())
    }

    fn next_location(&mut self) -> Result<()> {
        let next = self.forward.last().cloned().ok_or("No next location")?;
        let current = self
            .current_history_location()
            .ok_or("Open a document first")?;
        self.go_to_location(&next)?;
        self.forward.pop();
        Self::remember_location(&mut self.back, current);
        Ok(())
    }

    fn go_to_location(&mut self, target: &HistoryLocation) -> Result<()> {
        if self.fixed.is_some() {
            self.jump_fixed(
                target.position.page,
                target.scan_point.map(FixedJump::LinkPoint),
            )?;
        } else {
            self.restore_position(&target.position)?;
        }
        Ok(())
    }

    fn report(&mut self, error: impl std::fmt::Display) -> Change {
        self.notice = Some(error.to_string());
        Change::Partial
    }

    fn applied_change(&mut self) -> Change {
        if std::mem::take(&mut self.refresh_display) {
            Change::Full
        } else {
            Change::Partial
        }
    }

    /// Whether the pages are drawn light on dark. Under Primitive Launcher the launcher's theme decides; standalone,
    /// the Night pages setting does.
    fn uses_night_pages(&self) -> bool {
        self.system_theme
            .as_ref()
            .map_or(self.controls.night_pages, SystemTheme::dark)
    }

    /// Opened from Primitive Launcher, which owns the theme: Reader shows no light or dark switch of its own.
    pub(super) fn under_launcher(&self) -> bool {
        self.system_theme.is_some()
    }

    fn auto_turn_eligible(&self) -> bool {
        self.stats_awake
            && !self.touch_locked
            && self.end_dialog.is_none()
            && self.link_popup.is_none()
            && !self.read_alarm.modal()
            && self.range_edit.is_none()
            && self.saved_popup.is_none()
            && self.selection.is_none()
            && self.pdf_selection.is_none()
            && matches!(self.screen, Screen::Book | Screen::Fixed)
    }

    fn sync_auto_turn(&mut self, now: Instant) {
        if self.auto_turn_eligible() {
            self.auto_turn.resume(now);
        } else {
            self.auto_turn.pause();
        }
    }

    fn step_auto_turn(&mut self, now: Instant) -> Change {
        let change = self.input(Input::PageForward);
        self.auto_turn.activity(now);
        let turn_pending = self
            .active
            .as_ref()
            .and_then(|book| book.reading.deadline())
            .or_else(|| self.fixed.as_ref().and_then(|book| book.reading.deadline()))
            .is_some();
        if change == Change::None && !turn_pending {
            self.auto_turn.configure(None, now);
        }
        change
    }

    fn reading_edge_swipe(&mut self, input: Input, start_x: f32) -> Option<Change> {
        let Input::Swipe { dx, dy, start_y } = input else {
            return None;
        };
        if self.touch_locked
            || self.controls.menu_activation == MenuActivation::Tap
            || !matches!(
                self.screen,
                Screen::Book | Screen::BookTools | Screen::Fixed | Screen::FixedTools
            )
            || dy.abs() < 70.0
            || dy.abs() <= dx.abs() * 1.5
        {
            return None;
        }
        let width = WIDTH as f32;
        let height = HEIGHT as f32;
        let central = (width / 4.0..=width * 0.75).contains(&start_x);
        let edge = if central { height / 5.0 } else { height / 8.0 };
        let from_top = dy > 0.0 && (0.0..=edge).contains(&start_y);
        let from_bottom = dy < 0.0 && (height - edge..=height).contains(&start_y);
        if from_top || from_bottom {
            self.selection = None;
            self.pdf_selection = None;
            return Some(self.open_reading_edge(from_top, start_x));
        }
        None
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
                Screen::VocabularySearch => Screen::Vocabulary,
                Screen::EditNote if self.saved_popup.is_some() => {
                    if self.fixed.is_some() { Screen::Fixed } else { Screen::Book }
                }
                Screen::EditNote => Screen::Annotation,
                Screen::BookmarkFind => self.annotation_search.origin.unwrap_or(self.tools_screen()),
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
        if self.screen == Screen::BookmarkFind && self.annotation_search.toggle_at(x, y) {
            return Change::Partial;
        }
        if self.screen == Screen::Find {
            if let Some(change) = self.nearby_input(x, y) {
                return change;
            }
            if self.nearby.deadline().is_some() {
                self.nearby.cancel();
                self.notice = None;
            }
        }
        if self.screen == Screen::Find && FIND_CASE.contains(x, y) {
            self.find_match_case = !self.find_match_case;
            return Change::Partial;
        }
        if self.screen == Screen::Find && self.active.is_some() && FIND_PATTERN.contains(x, y) {
            self.find_regex = !self.find_regex;
            if self.find_regex { self.find_ignore_accents = false; }
            self.notice = None;
            return Change::Partial;
        }
        if self.screen == Screen::Find && FIND_ACCENTS.contains(x, y) {
            self.find_regex = false;
            self.find_ignore_accents = !self.find_ignore_accents;
            return Change::Partial;
        }
        let layout = Layout::new(WIDTH, HEIGHT, 4, 5);
        let action = match self.screen {
            Screen::DictionaryEntry => "Look up",
            _ if note => "Save",
            Screen::Find => "Find all",
            _ => "Find",
        };
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
                self.browser.first_page();
                self.show(Screen::Library);
                return Change::Partial;
            }
            if self.screen == Screen::VocabularySearch {
                self.vocabulary_view.query = query.trim().to_owned();
                self.show(Screen::Vocabulary);
                return Change::Partial;
            }
            let result = match self.screen {
                Screen::DictionaryEntry => self.look_up(&query),
                Screen::BookmarkFind => self.annotation_search.submit(&query).map(|()| {
                    self.show(Screen::BookmarkMatches);
                }),
                _ => self.start_document_search(&query),
            };
            if let Err(error) = result {
                return self.report(error);
            }
        }
        Change::Partial
    }

    fn start_document_search(&mut self, query: &str) -> Result<()> {
        let match_case = self.find_match_case;
        let ignore_accents = self.find_ignore_accents;
        if self.fixed.is_none() {
            return self.start_all_reflow(query);
        }
        let pdf_search = PdfSearch::with_options(query, match_case, ignore_accents)?;
        self.update_lookup_history(|history| {
            history.search(query, match_case, ignore_accents, false)
        })?;
        self.nearby.clear();
        self.search_result = None;
        self.matches_page = 0;
        self.focused_result = None;
        self.show(Screen::Matches);
        self.hits.clear();
        self.pdf_hits.clear();
        self.pdf_search = Some(pdf_search);
        let now = Instant::now();
        self.search_at = Some(now);
        self.next_search_refresh = now + SEARCH_REFRESH;
        Ok(())
    }

    fn update_lookup_history<T>(
        &mut self,
        update: impl FnOnce(&mut LookupHistory) -> T,
    ) -> Result<T> {
        let mut history = self.lookup_history.clone();
        let result = update(&mut history);
        history.save(&self.lookup_history_path)?;
        self.lookup_history = history;
        Ok(result)
    }

    fn update_note(&mut self, value: String) -> Result<()> {
        let index = self.highlight_index;
        let pdf = self.fixed.is_some();
        if value.trim().len() > crate::library::MAX_NOTE_BYTES {
            return Err("The note exceeds the saved note size limit".into());
        }
        let note = (!value.trim().is_empty()).then(|| value.trim().to_owned());
        let state = self.highlight_state()?;
        if pdf {
            state.pdf_highlights[index].note = note;
        } else {
            state.highlights[index].note = note;
        }
        self.mark_book_changed();
        if self.saved_popup.is_some() {
            self.saved_note_updated(index);
        } else {
            self.show(Screen::Highlights);
        }
        self.notice = Some("Note saved".into());
        Ok(())
    }

    fn handle_page_number(&mut self, input: Input) -> Change {
        if matches!(input, Input::PageBack)
            || matches!(input, Input::Tap(x, y) if primitive_ui::back_button(WIDTH as i32).contains(x, y))
        {
            self.show(if let Screen::NumberingValue(field) = self.screen {
                field.parent()
            } else if self.fixed.is_some() {
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
        let action = if matches!(self.screen, Screen::NumberingValue(_)) { "Set" } else { "Go" };
        let Some(key) = keyboard_key(&layout, &self.editor, action, x, y) else {
            return Change::None;
        };
        if matches!(self.editor.press(key), Press::Go) {
            if let Screen::NumberingValue(field) = self.screen {
                return match self.apply_numbering_input(field) {
                    Ok(()) => Change::Partial,
                    Err(error) => self.report(error),
                };
            }
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
            if self.bookmark_browser.after_password.take().is_some() {
                self.show(Screen::BookmarkBrowser(bookmark_browser::Page::Detail));
                return Change::Partial;
            }
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
            if let Err(error) = self.open_with_password(entry, Some(&password))
                .and_then(|_| self.bookmark_after_password())
            {
                return self.report(error);
            }
        }
        Change::Partial
    }

    fn jump_reflow_number(&mut self) -> Result<()> {
        let input = self.editor.string();
        let input = input.trim();
        if let Some(percent) = input.strip_suffix('%') {
            let active = self.active.as_mut().ok_or("No open book")?;
            let previous = position(&active.reading);
            active.reading.jump_progress(percent_fraction(percent)?)?;
            self.finish_reflow_jump(previous);
            return Ok(());
        }
        let active = self.active.as_ref().ok_or("No open book")?;
        if let Some(label) = input.strip_prefix("label ") {
            if let Some(map) = &active.numbering {
                let row = label.trim().parse::<usize>().ok().and_then(|n| n.checked_sub(1))
                    .filter(|row| *row < map.count).ok_or("Enter a stable page number from the list")?;
                return self.follow_character_page(row);
            }
            let mut matches = active
                .reading
                .probe
                .printed_pages
                .iter()
                .filter_map(|entry| {
                    (entry.label == label.trim())
                        .then_some(entry.destination.as_ref())
                        .flatten()
                });
            let target = matches
                .next()
                .ok_or("Enter a printed page label from the book")?;
            if matches.next().is_some() {
                return Err("That printed page label occurs more than once".into());
            }
            return self.follow(target.clone());
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
        self.finish_reflow_jump(previous);
        Ok(())
    }

    fn finish_reflow_jump(&mut self, previous: ReadingPosition) {
        self.push_history(previous);
        self.selection = None;
        self.result_selection = None;
        self.search_result = None;
        self.show(Screen::Book);
        self.record_position();
    }

    fn update_overview(&mut self, chapter: Option<usize>, first: Option<usize>) -> Result<()> {
        let mut preview = None;
        let (first, count, tiles) = if let Some(fixed) = &self.fixed {
            let count = fixed.reading.count();
            let first = overview_start(first, count);
            let tiles = (first..(first + OVERVIEW_PAGES).min(count))
                .map(|page| fixed.reading.thumbnail(page).ok().map(PageThumbnail::plain))
                .collect();
            (first, count, tiles)
        } else {
            let reading = &mut self.active.as_mut().ok_or("Open a book first")?.reading;
            let chapter = chapter.ok_or("Choose a chapter first")?;
            if chapter != reading.chapter {
                preview = if let Some(previous) = self.overview.as_mut()
                    && previous.chapter == Some(chapter)
                {
                    previous.preview.take()
                } else {
                    Some(Box::new(reading.config.chapter(chapter)?))
                };
            }
            let probe = preview.as_deref_mut().unwrap_or(&mut reading.probe);
            let count = probe.page_count();
            let first = overview_start(first, count);
            let tiles = (first..(first + OVERVIEW_PAGES).min(count))
                .map(|page| page_thumbnail(probe, page).ok())
                .collect();
            (first, count, tiles)
        };
        self.overview = Some(PageOverview {
            first,
            count,
            chapter,
            preview,
            tiles,
        });
        self.notice = None;
        Ok(())
    }

    fn draw_overview(&self, canvas: &mut Canvas) {
        let Some(overview) = &self.overview else {
            return;
        };
        let count = overview.count;
        let (section, current, chapters) = if let Some(active) = &self.active {
            let reading = &active.reading;
            let probe = overview.preview.as_deref().unwrap_or(&reading.probe);
            (
                format!("{}  ·  {count} pages", probe.chapter_label),
                (overview.chapter == Some(reading.chapter)).then_some(reading.page),
                probe.chapter_count(),
            )
        } else {
            (
                format!("{count} pages"),
                self.fixed.as_ref().map(|fixed| fixed.reading.page()),
                0,
            )
        };
        canvas.text(
            Rect::new(LIST_X, LIST_Y, WIDTH as i32 - 2 * LIST_X, 66),
            self.notice.as_deref().unwrap_or(&section),
            CAPTION,
        );
        for (index, tile) in overview.tiles.iter().enumerate() {
            let rect = overview_tile(index);
            canvas.card(rect);
            let page = overview.first + index;
            let hidden = self.active.as_ref().and_then(|active| overview.chapter.and_then(|chapter|
                active.reading.custom_flow_for_page(chapter, page, overview.count)))
                .or_else(|| self.fixed.as_ref().and_then(|fixed| fixed.reading.hidden_flow_map().map(|flows| flows.flow(page))))
                .is_some_and(|flow| flow != 0);
            if hidden { canvas.panel(rect, 22, 2, 235); }
            let mut label = self.fixed.as_ref().map_or_else(
                || format!("Page {}", page + 1),
                |fixed| fixed_page_label(&fixed.reading, page),
            );
            if hidden { label.push_str(" · Hidden"); }
            if let Some(tile) = tile {
                let thumbnail = tile.for_theme(self.uses_night_pages());
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
                Some(page) == current,
                false,
            );
        }
        if overview.first != 0 {
            canvas.button(PREV, "Previous", false, false);
        } else if overview.chapter.is_some_and(|chapter| chapter > 0) {
            canvas.button(PREV, "Prev chapter", false, false);
        }
        if overview.first + OVERVIEW_PAGES < count {
            canvas.button(NEXT, "Next", false, false);
        } else if overview
            .chapter
            .is_some_and(|chapter| chapter + 1 < chapters)
        {
            canvas.button(NEXT, "Next chapter", false, false);
        }
        self.draw_overview_track(canvas, overview, chapters);
    }

    fn draw_overview_track(&self, canvas: &mut Canvas, overview: &PageOverview, chapters: usize) {
        let (selected, count, title) = overview
            .chapter
            .map_or((overview.first, overview.count, "Skim pages"), |chapter| {
                (chapter, chapters, "Skim chapters")
            });
        let detail = format!("{title} · {} of {count}", selected + 1);
        canvas.text(
            Rect::new(80, OVERVIEW_TRACK.y - 55, OVERVIEW_TRACK.w, 45),
            &detail,
            CAPTION,
        );
        let center = OVERVIEW_TRACK.y + OVERVIEW_TRACK.h / 2;
        canvas.fill(
            Rect::new(OVERVIEW_TRACK.x, center - 3, OVERVIEW_TRACK.w, 6),
            151,
        );
        let portion = selected as f64 / count.saturating_sub(1).max(1) as f64;
        let x = OVERVIEW_TRACK.x + (portion * (OVERVIEW_TRACK.w - 20) as f64).round() as i32;
        canvas.fill(Rect::new(x, center - 15, 20, 30), 0);
    }

    fn scrub_overview(&mut self, x: f32) -> Change {
        let Some(overview) = &self.overview else {
            return Change::None;
        };
        let fraction = ((x - OVERVIEW_TRACK.x as f32) / OVERVIEW_TRACK.w as f32).clamp(0.0, 1.0);
        let result = if let Some(active) = &self.active {
            let count = active.reading.probe.chapter_count();
            let chapter = (fraction * count.saturating_sub(1) as f32).round() as usize;
            if overview.chapter == Some(chapter) {
                return Change::None;
            }
            self.update_overview(Some(chapter), Some(0))
        } else {
            let page = (fraction * overview.count.saturating_sub(1) as f32).round() as usize;
            self.update_overview(None, Some(page))
        };
        match result {
            Ok(()) => Change::Partial,
            Err(error) => self.report(error),
        }
    }

    fn overview_input(&mut self, input: Input) -> Change {
        let Some(overview) = &self.overview else {
            return Change::None;
        };
        let direction = match input {
            Input::LongTap(x, y) => {
                let Some(index) = (0..overview.tiles.len()).find(|&index| overview_tile(index).contains(x, y))
                    else { return Change::None; };
                let result = self.show_flow_page_popup(overview.chapter, overview.first + index);
                return match result { Ok(()) => Change::Partial, Err(error) => self.report(error) };
            }
            Input::PageForward => 1,
            Input::PageBack => -1,
            Input::Swipe { dx, dy, .. } if dy.abs() > dx.abs() => {
                if dy < 0.0 {
                    1
                } else {
                    -1
                }
            }
            Input::Tap(x, y) if PREV.contains(x, y) => -1,
            Input::Tap(x, y) if NEXT.contains(x, y) => 1,
            Input::Tap(x, y) if OVERVIEW_TRACK.contains(x, y) => return self.scrub_overview(x),
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
                let result = if let Some(chapter) = overview.chapter {
                    self.follow_reflow_page(chapter, page)
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
        let chapter = overview.chapter;
        let (chapter, first) = if let Some(first) = overview
            .first
            .checked_add_signed(direction * OVERVIEW_PAGES as isize)
            .filter(|&first| first < overview.count)
        {
            (chapter, Some(first))
        } else if let Some(chapter) = chapter
            && let Some(active) = &self.active
            && let Some(next) = chapter.checked_add_signed(direction)
            && next < active.reading.probe.chapter_count()
        {
            (Some(next), (direction > 0).then_some(0))
        } else {
            return Change::None;
        };
        match self.update_overview(chapter, first) {
            Ok(()) => Change::Partial,
            Err(error) => self.report(error),
        }
    }

    fn step_pdf_search(&mut self, now: Instant) -> Result<Change> {
        let search = self.pdf_search.as_mut().ok_or("No active PDF search")?;
        let reading = &self.fixed.as_ref().ok_or("No open PDF")?.reading;
        let previous = self.pdf_hits.len();
        let started = Instant::now();
        let stop_page = (search.next_page + 4).min(reading.count());
        while search.next_page < stop_page && self.pdf_hits.len() < MAX_FIXED_SEARCH_HITS {
            let page = search.next_page;
            search.next_page += 1;
            match reading.page_text(page) {
                Ok(text) => {
                    search.append_page(page, &text, &mut self.pdf_hits, MAX_FIXED_SEARCH_HITS);
                }
                Err(_) => search.skipped += 1,
            }
            if search.memory_limited || started.elapsed() >= SEARCH_DELAY {
                break;
            }
        }
        if search.memory_limited
            || search.next_page == reading.count()
            || self.pdf_hits.len() == MAX_FIXED_SEARCH_HITS
        {
            self.search_at = None;
            self.notice = if self.screen != Screen::Matches {
                None
            } else if search.memory_limited {
                Some("Search result memory limit reached".into())
            } else if self.pdf_hits.len() == MAX_FIXED_SEARCH_HITS {
                Some("Showing first 5,000 matches".into())
            } else if search.skipped > 0 {
                Some("Some pages could not be searched".into())
            } else if search.text_pages == 0 {
                Some("This document has no embedded text to search".into())
            } else {
                None
            };
            return Ok(Change::Partial);
        }
        self.search_at = Some(Instant::now() + FIXED_SEARCH_YIELD);
        if self.pdf_hits.len() != previous && (previous == 0 || now >= self.next_search_refresh) {
            self.next_search_refresh = now + SEARCH_REFRESH;
            Ok(Change::Partial)
        } else {
            Ok(Change::None)
        }
    }

    fn handle_book(&mut self, input: Input) -> Change {
        if let Input::LongTap(x, y) = input
            && self.selection.is_none()
            && let Some(change) = self.hold_footer(x, y)
        {
            return change;
        }
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
            self.start_cjk_selection();
            self.notice = None;
            return Change::Partial;
        }
        if let Input::Tap(x, y) = input {
            if self.selection.is_some() {
                if let Some(change) = self.selection_popup_input(x, y) {
                    return change;
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
            if let Some(change) = self.search_result_input(x, y) {
                return change;
            }
            if DONE.contains(x, y) {
                return self.exit();
            }
            if let Some(target) = self.active.as_ref().and_then(|active| {
                let reading = &active.reading;
                reading.link_at(x, y)
            }) {
                if self.open_note_at(target.clone(), x, y) {
                    return Change::Partial;
                }
                return match self.follow(target) {
                    Ok(()) => Change::Partial,
                    Err(error) => self.report(error),
                };
            }
            if let Some(change) = self.tap_footer(x, y) {
                return change;
            }
            if self.reveal_reading_tools(x, y, Screen::BookTools) {
                return Change::Partial;
            }
        }
        let Some(active) = &mut self.active else {
            return Change::None;
        };
        let Some(input) = self.controls.page_input(input) else {
            return Change::None;
        };
        let previous = (active.reading.chapter, active.reading.page);
        let change = active.reading.input(input);
        self.finish_book_update(change, previous)
    }

    fn finish_book_update(&mut self, change: Change, previous: (usize, usize)) -> Change {
        let attempted = self.active.as_mut().is_some_and(|active| active.reading.take_end_attempt());
        if change != Change::Partial {
            return self.finish_end_attempt(attempted, change);
        }
        let page_changed = self.active.as_ref().is_some_and(|active| {
            active.reading.paged_frame_ready()
                && (active.reading.chapter, active.reading.page) != previous
        });
        self.notice = None;
        self.clear_stale_selection();
        self.record_position();
        let change = self.reading_update(page_changed);
        self.finish_end_attempt(attempted, change)
    }

    fn reading_update(&mut self, page_changed: bool) -> Change {
        if page_changed {
            self.controls
                .page_refresh
                .after_page(&mut self.pages_since_refresh)
        } else {
            Change::Partial
        }
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
        let show_zoom = !self.hidden_overlap_footer();
        if let Input::LongTap(x, y) = input
            && self.pdf_selection.is_none()
            && let Some(change) = self.hold_footer(x, y)
        {
            return change;
        }
        if let Input::LongTap(x, y) = input
            && let Some(fixed) = &mut self.fixed
            && fixed.entry.format.is_comic()
        {
            return match fixed.reading.inspect_comic_panel(x, y) {
                Ok(Some(panel)) => {
                    fixed.reading.pause();
                    self.show(Screen::Illustration);
                    self.illustration = Some(panel);
                    Change::Partial
                }
                Ok(None) => Change::None,
                Err(error) => self.report(error),
            };
        }
        if let Input::LongTap(x, y) = input
            && let Some(fixed) = &self.fixed
            && fixed.reading.supports_text()
        {
            use crate::fixed_session::WordAtTouch;
            return match fixed.reading.select_word(x, y) {
                Ok(WordAtTouch::Ready(Some(selection))) => {
                    self.cancel_ocr_selection();
                    self.pdf_selection = Some(selection);
                    self.pdf_result_regions.clear();
                    self.notice = None;
                    Change::Partial
                }
                Ok(WordAtTouch::Ready(None)) => self.report("No selectable text at this point"),
                Ok(WordAtTouch::Recognize { page, point }) => {
                    if let Some(active) = &mut self.ocr
                        && active.page == page
                    {
                        active.selection_point = Some(point);
                        return Change::None;
                    }
                    match self.recognize_page(page, None, Some(point)) {
                        Ok(()) => Change::Partial,
                        Err(error) => self.report(error),
                    }
                }
                Err(error) => self.report(error),
            };
        }
        if let Input::Tap(x, y) = input {
            if self.ocr.is_some() && self.pdf_selection.is_none() && CANCEL_OCR.contains(x, y) {
                self.ocr = None;
                self.notice = Some("Text recognition cancelled".into());
                return Change::Partial;
            }
            if self.pdf_selection.is_some() {
                if let Some(change) = self.selection_popup_input(x, y) {
                    return change;
                }
                if let (Some(fixed), Some(selection)) = (&self.fixed, &mut self.pdf_selection) {
                    fixed.reading.extend_selection(selection, x, y);
                }
                return Change::Partial;
            }
            if let Some(change) = self.search_result_input(x, y) {
                return change;
            }
            if DONE.contains(x, y) {
                return self.exit();
            }
            if NEXT_COMIC.contains(x, y)
                && self.ocr.is_none()
                && let Some(next) = self.next_comic().cloned()
            {
                return match self.open_book(next) {
                    Ok(()) => Change::Partial,
                    Err(error) => self.report(error),
                };
            }
            if let Some(link) = self
                .fixed
                .as_ref()
                .and_then(|fixed| fixed.reading.link_at(x, y))
            {
                return match link.target {
                    PdfLinkTarget::Page(page) => match self
                        .follow_fixed(page, link.destination_point.map(FixedJump::LinkPoint))
                    {
                        Ok(()) => Change::Partial,
                        Err(error) => self.report(error),
                    },
                    PdfLinkTarget::External => match link.uri {
                        Some(address) => self.show_link_address(address),
                        None => self.report("This link refers to a resource outside the PDF"),
                    },
                    PdfLinkTarget::Unavailable => self.report("This PDF link is unavailable"),
                };
            }
            if let Some(change) = self.tap_footer(x, y) {
                return change;
            }
            if self.reveal_reading_tools(x, y, Screen::FixedTools) {
                return Change::Partial;
            }
        }
        let Some(fixed) = &mut self.fixed else {
            return Change::None;
        };
        let page = fixed.reading.page();
        let Some(input) = self.controls.page_input(input) else {
            return Change::None;
        };
        let result = fixed.reading.input_with_status(input, show_zoom);
        self.finish_fixed_update(result, page)
    }

    fn finish_fixed_update(&mut self, result: Result<bool>, previous: usize) -> Change {
        let attempted = self.fixed.as_mut().is_some_and(|fixed| fixed.reading.take_end_attempt());
        let change = match result {
            Ok(true) => {
                let (changed, continuous) = self.fixed.as_ref().map_or((false, false), |fixed| {
                    (
                        fixed.reading.page() != previous,
                        fixed.reading.is_continuous(),
                    )
                });
                self.notice = None;
                self.cancel_ocr_selection();
                if changed || continuous {
                    self.nearby.clear();
                    self.search_result = None;
                    self.pdf_selection = None;
                    self.pdf_result_regions.clear();
                }
                self.record_position();
                self.reading_update(changed && !continuous)
            }
            Ok(false) => Change::None,
            Err(error) => return self.report(error),
        };
        self.finish_end_attempt(attempted, change)
    }

    fn next_comic(&self) -> Option<&LibraryBook> {
        let fixed = self.fixed.as_ref()?;
        if !fixed.entry.format.is_comic()
            || !fixed
                .reading
                .visible_pages()
                .max()
                .is_some_and(|page| fixed.reading.next_reading_page(page, 1).is_none())
        {
            return None;
        }
        self.library.next_comic_in_folder(&fixed.entry)
    }

    fn search_step(&self, step: isize) -> Option<Action> {
        let current = self.search_result?;
        if self.nearby.result() {
            let direction = if step < 0 {
                crate::search::SearchDirection::Previous
            } else {
                crate::search::SearchDirection::Next
            };
            return self.nearby.can_step(direction).then_some(Action::StepNearby(direction));
        }
        let index = current.checked_add_signed(step)?;
        if self.fixed.is_some() {
            (index < self.pdf_hits.len()).then_some(Action::PdfSearchHit(index))
        } else {
            (index < self.hits.len()).then_some(Action::SearchHit(index))
        }
    }

    fn search_result_input(&mut self, x: f32, y: f32) -> Option<Change> {
        if self.ocr.is_some() {
            return None;
        }
        if self.fixed_result_navigation() && !self.nearby.searching_direction() {
            return self.fixed_result_input(x, y);
        }
        let step = if MATCH_PREV.contains(x, y) {
            -1
        } else if MATCH_NEXT.contains(x, y) {
            1
        } else {
            return None;
        };
        if self.nearby.searching_direction() {
            self.nearby.cancel();
            self.notice = None;
            return Some(Change::Partial);
        }
        let action = self.search_step(step)?;
        Some(match self.activate(action) {
            Ok(()) => Change::Partial,
            Err(error) => self.report(error),
        })
    }

    fn draw_search_steps(&self, canvas: &mut Canvas) {
        if self.nearby.searching_direction() {
            canvas.button(MATCH_PREV, "Cancel", false, false);
            return;
        }
        if self.draw_fixed_result_navigation(canvas) { return; }
        if self.search_step(-1).is_some() {
            canvas.button(MATCH_PREV, "Previous", false, false);
        }
        if self.search_step(1).is_some() {
            canvas.button(MATCH_NEXT, "Next", false, false);
        }
    }

    fn search_caption(&self) -> Option<String> {
        let index = self.search_result?;
        if self.nearby.result() {
            if let Some(fixed) = &self.fixed {
                return Some(format!("Matching page {} of {}", fixed.reading.page() + 1, fixed.reading.count()));
            }
            return Some("Search match".into());
        }
        let count = if self.fixed.is_some() {
            self.pdf_hits.len()
        } else {
            self.hits.len()
        };
        let found = if self.search_at.is_some() || self.nearby.searching_all() {
            " found"
        } else {
            ""
        };
        Some(format!("Match {} of {count}{found}", index + 1))
    }

    fn read_fixed_text(&mut self, scan: bool) -> Result<()> {
        let fixed = self.fixed.as_ref().ok_or("Open a document first")?;
        let text_flow = TextFlow::open(
            &fixed.reading,
            &fixed.entry.title,
            &self.fonts,
            device_settings(),
            scan,
        )?;
        self.show(Screen::ReadFixedText);
        self.text_flow = Some(text_flow);
        Ok(())
    }

    fn finish_text_flow_update(&mut self, result: Result<bool>) -> Change {
        let attempted = self.text_flow.as_mut().is_some_and(TextFlow::take_end_attempt);
        let change = match result {
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
            Err(error) => return self.report(error),
        };
        self.finish_end_attempt(attempted, change)
    }

    fn open_scan_columns(&mut self) -> Result<()> {
        let fixed = self
            .fixed
            .as_mut()
            .ok_or("Open a supported document first")?;
        let raster = fixed.reading.ocr_raster(fixed.reading.page())?;
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
        let Some(input) = self.controls.page_input(input) else {
            return Change::None;
        };
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

    fn cancel_ocr_selection(&mut self) {
        if self.ocr.as_ref().is_some_and(|job| job.selection_point.is_some()) {
            self.ocr = None;
        }
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
        let page = self.fixed.as_ref().ok_or("Open a PDF or DjVu first")?.reading.page();
        self.recognize_page(page, language, None)
    }

    fn recognize_page(&mut self, page: usize, language: Option<&str>, selection_point: Option<hayro::kurbo::Point>) -> Result<()> {
        self.ocr = None;
        let assets = self.ocr_assets.as_ref().ok_or("OCR requires an installed language pack")?;
        let fixed = self.fixed.as_ref().ok_or("Open a PDF or DjVu first")?;
        let explicit_language = language.is_some();
        let language = language.or(fixed.reading.ocr_language.as_deref())
            .unwrap_or_else(|| assets.default_language()).to_owned();
        let raster = fixed.reading.ocr_raster(page)?;
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
        if explicit_language && let Some(fixed) = &mut self.fixed {
            fixed.reading.ocr_language = Some(language.clone());
            self.record_position();
        }
        self.show(Screen::Fixed);
        self.notice = Some(format!(
            "Recognizing page {} · {}",
            page + 1,
            ocr_language_name(&language)
        ));
        self.pdf_selection = None;
        self.ocr = Some(ActiveOcr {
            page,
            selection_point,
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
        let requested_word = active.selection_point.filter(|_| self.screen == Screen::Fixed && fixed.reading.visible_pages().any(|p| p == page));
        fixed.reading.remember_ocr(page, &active.language, text)?;
        if let Some(point) = requested_word {
            self.pdf_selection = fixed
                .reading
                .cached_ocr(page)
                .and_then(|words| PdfSelection::at(page, point, words));
            self.pdf_result_regions.clear();
            self.notice = self
                .pdf_selection
                .is_none()
                .then(|| "No recognized word at this point".to_owned());
        } else {
            self.notice = Some(format!("Text recognized on page {}", page + 1));
        }
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
                        dither: original.dither,
                        straighten_limit: original.straighten_limit,
                        force_ocr: original.force_ocr,
                        scan_white_level: original.scan_white_level,
                        continuous_gap: original.continuous_gap,
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
        self.nearby.clear();
        self.result_selection = None;
        self.search_result = None;
        if let (Some(active), Some(selection)) = (&self.active, &self.selection)
            && selection.start.chapter != active.reading.probe.chapter
        {
            self.selection = None;
        }
    }

    fn export_backup(&mut self) -> Result<()> {
        let bytes = self.reading_backup_bytes()?;
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
        let bytes = read_import_file(&self.import_path, crate::reading_backup::LIMIT).map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                io::Error::new(
                    error.kind(),
                    "Copy a backup to reader/data/reader-import.json",
                )
            } else {
                error
            }
        })?;
        self.backup_import = Some(self.prepare_reading_backup(&bytes)?);
        self.show(Screen::RestoreBackup);
        Ok(())
    }

    fn restore_backup(&mut self) -> Result<()> {
        self.apply_reading_backup()?;
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

    fn preview_koreader_position(&mut self, copied_only: bool) -> Result<()> {
        let entry = self.fixed.as_ref().map(|book| &book.entry)
            .or_else(|| self.active.as_ref().map(|book| &book.entry))
            .ok_or("Open the matching book first")?;
        let nearby = if copied_only { None } else {
            with_storage(|| self.library.nearby_koreader_metadata(&entry.path))?
        };
        let source = if nearby.is_some() { "Saved beside this book" } else { "Copied to Reader data" };
        let path = nearby.unwrap_or_else(|| self.import_path.with_file_name("koreader-progress.lua"));
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
        let sidecar = koreader_sidecar::parse(&bytes)?;
        let target = self.resolve_koreader_position(&sidecar);
        let target = if self.fixed.is_some() {
            Ok(target?)
        } else {
            target.map_err(|error| error.to_string())
        };
        self.show(Screen::ImportPosition);
        self.position_import = Some(PositionImport { sidecar, target, source });
        Ok(())
    }

    fn resolve_koreader_position(&self, progress: &SidecarProgress) -> Result<ReadingPosition> {
        let (entry, id) = self
            .fixed
            .as_ref()
            .map(|fixed| (&fixed.entry, fixed.id))
            .or_else(|| {
                self.active
                    .as_ref()
                    .map(|active| (&active.entry, active.id))
            })
            .ok_or("Open the matching book first")?;
        if self.fixed.is_none() && entry.format != DocumentFormat::Epub {
            return Err("This format uses a KOReader pointer that Reader cannot map".into());
        }
        let position = with_storage(|| {
            if self.library.identify(entry)? != id {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "The document changed. Reopen it and preview the import again.",
                ));
            }
            let path = self.library.absolute_path(&entry.path)?;
            let checksum = koreader_sidecar::partial_md5(&mut fs::File::open(&path)?)?;
            if let Some(fixed) = &self.fixed {
                let page = progress
                    .resolve_fixed(checksum, fixed.reading.count())
                    .map_err(io::Error::other)?;
                Ok(ReadingPosition {
                    chapter: 0,
                    page,
                    anchor: None,
                })
            } else {
                progress
                    .check_fingerprint(checksum)
                    .map_err(io::Error::other)?;
                let mut book = Book::open(&path).map_err(io::Error::other)?;
                let target = progress
                    .reflow_position(&mut book)
                    .map_err(io::Error::other)?;
                Ok(ReadingPosition {
                    chapter: target.chapter,
                    page: 0,
                    anchor: Some(target.anchor),
                })
            }
        })?;
        Ok(position)
    }

    fn apply_koreader_position(&mut self) -> Result<()> {
        let import = self
            .position_import
            .as_ref()
            .ok_or("Preview the KOReader position first")?;
        let preview = import.target.as_ref().map_err(|error| error.as_str())?;
        let target = self.resolve_koreader_position(&import.sidecar)?;
        if &target != preview {
            return Err("The reading position changed. Preview the import again.".into());
        }
        if let Some(active) = &self.active {
            let previous = position(&active.reading);
            self.restore_position(&target)?;
            self.push_history(previous);
        } else {
            self.follow_fixed(target.page, None)?;
        }
        self.save()?;
        self.notice = Some("KOReader reading position applied".into());
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
                            color: Default::default(),
                            created_at: entry.time,
                            style: entry.drawer,
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
        let mut timestamps_added = 0;
        for mark in import.matched {
            if let Some(existing) = state
                .highlights
                .iter_mut()
                .find(|existing| existing.selection == mark.selection)
            {
                if existing.created_at.is_none() && mark.created_at.is_some() {
                    existing.created_at = mark.created_at;
                    timestamps_added += 1;
                }
                if existing.note.is_none() && mark.note.is_some() {
                    existing.note = mark.note;
                    updated += 1;
                }
            } else if state.highlights.len() < 256 {
                state.highlights.push(mark);
                added += 1;
            }
        }
        if added > 0 || updated > 0 || timestamps_added > 0 {
            self.mark_dirty();
        }
        self.show(Screen::Highlights);
        self.notice = Some(format!(
            "Added {added} highlights and updated {updated} notes"
        ));
        Ok(())
    }

    fn selected_passage(&self) -> Result<&str> {
        self.pdf_selection
            .as_ref()
            .map(PdfSelection::text)
            .or_else(|| {
                self.selection
                    .as_ref()
                    .map(|selection| selection.text.as_str())
            })
            .ok_or_else(|| "Select a word first".into())
    }

    fn look_up_selection(&mut self) -> Result<()> {
        let word = self.selected_passage()?.to_owned();
        self.look_up(&word)
    }

    fn find_selection(&mut self) -> Result<()> {
        self.find_regex = false;
        let query = self
            .selected_passage()?
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if query.is_empty() {
            return Err("The selection has no searchable text".into());
        }
        let excerpt: Vec<char> = query
            .char_indices()
            .take(128)
            .take_while(|(offset, ch)| offset + ch.len_utf8() <= crate::search::MAX_QUERY_BYTES)
            .map(|(_, ch)| ch)
            .collect();
        let shortened = query.len() > crate::search::MAX_QUERY_BYTES || query.chars().count() > excerpt.len();
        if shortened {
            self.show(Screen::Find);
        } else {
            self.start_document_search(&query)?;
        }
        self.editor = Editor::new(128);
        self.editor.caret = excerpt.len();
        self.editor.text = excerpt;
        if shortened {
            self.notice = Some("Edit this excerpt to search a shorter passage".into());
        }
        self.selection = None;
        self.pdf_selection = None;
        Ok(())
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
                color: state.highlight_color.unwrap_or(self.controls.highlight_color),
                created_at: crate::mark_time::now(),
                style: state
                    .highlight_style
                    .unwrap_or(self.controls.highlight_style),
                selection: selection.clone(),
                note,
            });
        }
        self.selection = None;
        self.mark_book_changed();
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
        if fixed.reading.is_encrypted_pdf() {
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
                    color: annotation.color,
                    created_at: None,
                    style: annotation.style,
                    continuation: Vec::new(),
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
            .find(|mark| mark.continuation.is_empty() && mark.page == selection.page() && mark.bounds == bounds)
        {
            if note.is_some() {
                mark.note = note;
            }
        } else {
            state.add_pdf_highlight(PdfHighlight {
                color: state.highlight_color.unwrap_or(self.controls.highlight_color),
                created_at: crate::mark_time::now(),
                style: state
                    .highlight_style
                    .unwrap_or(self.controls.highlight_style),
                continuation: Vec::new(),
                page: selection.page(),
                bounds,
                text: selection.text().into(),
                note,
            })?;
        }
        self.pdf_selection = None;
        self.mark_book_changed();
        self.show(Screen::Fixed);
        self.notice = Some("Highlight saved".into());
        Ok(())
    }
}

impl Reader {
    fn draw_content(&self, canvas: &mut Canvas) {
        if self.screen == Screen::DocumentDefaults { self.draw_document_defaults(canvas); return; }
        if self.draw_reading_history(canvas) { return; }
        if let Screen::Catalog(page) = self.screen && self.draw_catalog(canvas,page) { return; }
        if let Screen::SyncEdit(field) = self.screen { self.draw_sync_editor(canvas, field); return; }
        // The launcher's theme may have changed (it asks for a redraw when it does).
        if let Some(theme) = &self.system_theme {
            theme.refresh();
        }
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
            let title = if self.book_details.is_some() {
                "Book cover"
            } else if self.fixed.is_some() {
                "Comic panel"
            } else {
                "Illustration"
            };
            let notice = self.notice.as_deref().unwrap_or(title);
            let mut paint = self.page_paint();
            if self.book_details.is_none() && self.active.as_ref().is_some_and(
                |active| !active.reading.config.settings.invert_night_images
            ) {
                paint.inverted = false;
            }
            illustration.draw(canvas, Some(notice), &[], &[], paint);
            return;
        }
        if self.screen == Screen::ReadFixedText && let Some(flow) = &self.text_flow {
            flow.draw(canvas, self.notice.as_deref(), self.uses_night_pages());
            self.draw_end_dialog(canvas);
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
        if (matches!(self.screen, Screen::Fixed | Screen::FixedTools | Screen::OnlineLookup)
            || self.definition_over_book())
            && let Some(fixed) = &self.fixed
        {
            let mut marks: Vec<_> = self
                .library
                .state(fixed.id)
                .into_iter()
                .flat_map(|state| &state.pdf_highlights)
                .enumerate()
                .filter_map(|(index, saved)| match self.range_edit.as_ref().filter(|edit| edit.index == index).map(|edit| &edit.draft) {
                    Some(extend_highlight::Draft::Fixed {current, ..}) => Some(current),
                    None => Some(saved),
                    _ => None,
                })
                .flat_map(|mark| {
                    mark.parts()
                        .map(move |(page, rects, _)| (page, rects, mark.style, mark.color, mark.note.is_some()))
                })
                .filter(|(index, _, _, _, _)| fixed.reading.visible_pages().any(|page| page == *index))
                .flat_map(|(page, rects, style, color, note)| {
                    rects.iter().filter(move |bounds|fixed.reading.visible_rectangle(page,**bounds).is_some()).enumerate().map(move |(i, &bounds)| {
                        ((page, bounds), crate::highlight_style::Paint::Annotation {style, color, note, first:i==0, night:self.uses_night_pages()})
                    })
                })
                .collect();
            marks.extend(
                self.pdf_result_regions
                    .iter()
                    .copied()
                    .map(|rect| (rect, crate::highlight_style::Paint::Temporary(220))),
            );
            let selection: Vec<_> = self
                .pdf_selection
                .as_ref()
                .into_iter()
                .flat_map(|selection| selection.bounds().map(|bounds| (selection.page(), bounds)))
                .collect();
            let search_caption = self.search_caption();
            let status_caption = self.status_caption();
            fixed.reading.draw_with_footer(
                canvas,
                self.notice
                    .as_deref()
                    .or(search_caption.as_deref())
                    .or(status_caption.as_deref()),
                &marks,
                &selection,
                self.page_paint(),
                self.hidden_overlap_footer(),
            );
            self.draw_footer_track(canvas);
            if self.saved_popup_over_book() {
                self.draw_saved_popup(canvas);
                return;
            }
            if self.screen == Screen::Fixed && self.range_edit.is_some() {
                self.draw_range_tools(canvas);
                return;
            }
            if self.screen == Screen::Fixed {
                self.draw_selection_popup(canvas);
            }
            if self.screen == Screen::FixedTools {
                self.draw_reading_tools(canvas);
            } else if self.touch_locked {
                draw_locked_footer(canvas, self.notice.as_deref());
            } else if self.ocr.is_some() && selection.is_empty() {
                canvas.button(CANCEL_OCR, "Cancel OCR", false, false);
            } else if self.screen == Screen::Fixed
                && selection.is_empty()
                && self.next_comic().is_some()
            {
                canvas.button(NEXT_COMIC, "Next comic", false, false);
            } else if self.screen == Screen::Fixed && selection.is_empty() {
                self.draw_search_steps(canvas);
            }
            if let Some(popup) = &self.link_popup { popup.draw(canvas); }
            if self.screen == Screen::OnlineLookup { self.draw_online(canvas); }
            if self.definition_over_book() {
                self.draw_definition_popup(canvas);
            }
            self.draw_end_dialog(canvas);
            return;
        }
        if (matches!(
            self.screen,
            Screen::Footnote | Screen::Book | Screen::BookTools | Screen::OnlineLookup
        ) || self.definition_over_book())
            && let Some(active) = &self.active
        {
            let reading = &active.reading;
            let mut highlights: Vec<_> = self
                .library
                .state(active.id)
                .into_iter()
                .flat_map(|state| &state.highlights)
                .enumerate()
                .flat_map(|(index, highlight)| {
                    let selected = self.range_edit.as_ref().filter(|edit| edit.index == index)
                        .and_then(|edit| match &edit.draft {
                            extend_highlight::Draft::Text {current, ..} => Some(current),
                            _ => None,
                        })
                        .unwrap_or(&highlight.selection);
                    reading
                        .highlight_rects(selected)
                        .into_iter()
                        .enumerate().map(move |(i, rect)| {
                            (rect, crate::highlight_style::Paint::Annotation {style:highlight.style, color:highlight.color, note:highlight.note.is_some(), first:i==0, night:self.uses_night_pages()})
                        })
                })
                .collect();
            if let Some(reference) = self
                .footnote
                .as_ref()
                .and_then(|note| note.reference.as_ref())
            {
                highlights.extend(
                    reading
                        .highlight_rects(reference)
                        .into_iter()
                        .map(|r| (r, crate::highlight_style::Paint::Temporary(220))),
                );
            }
            if let Some(result) = &self.result_selection {
                highlights.extend(
                    reading
                        .highlight_rects(result)
                        .into_iter()
                        .map(|r| (r, crate::highlight_style::Paint::Temporary(220))),
                );
            }
            let selection = self
                .selection
                .as_ref()
                .map_or_else(Vec::new, |selection| reading.highlight_rects(selection));
            let search_caption = self.search_caption();
            let status_caption = self.status_caption();
            reading.draw_page(
                canvas,
                self.selection.is_none().then_some("Exit"),
                self.hidden_overlap_footer(),
                self.notice
                    .as_deref()
                    .or(search_caption.as_deref())
                    .or(status_caption.as_deref()),
                &highlights,
                &selection,
                self.page_paint(),
            );
            self.draw_publisher_margins(canvas);
            self.draw_footer_track(canvas);
            if self.saved_popup_over_book() {
                self.draw_saved_popup(canvas);
                return;
            }
            if self.screen == Screen::Book && self.range_edit.is_some() {
                self.draw_range_tools(canvas);
                return;
            }
            if self.screen == Screen::Book {
                self.draw_selection_popup(canvas);
            }
            if self.screen == Screen::BookTools {
                self.draw_reading_tools(canvas);
            } else if self.touch_locked {
                draw_locked_footer(canvas, self.notice.as_deref());
            } else if self.screen == Screen::Book && selection.is_empty() {
                self.draw_search_steps(canvas);
            }
            if let Some(note) = &self.footnote {
                note.draw(canvas, self.uses_night_pages());
            }
            if self.screen == Screen::OnlineLookup { self.draw_online(canvas); }
            if self.definition_over_book() {
                self.draw_definition_popup(canvas);
            }
            self.draw_end_dialog(canvas);
            return;
        }
        if self.screen == Screen::OnlineEdit { self.draw_online_editor(canvas); return; }
        if self.screen == Screen::OnlineLookup { self.draw_online(canvas); return; }
        if self.screen == Screen::Hyphenate {
            self.draw_hyphenation(canvas);
            return;
        }
        if self.screen == Screen::CustomTitle { self.draw_chapter_editor(canvas); return; }
        if self.screen == Screen::Library {
            self.draw_library(canvas);
            return;
        }
        if self.screen == Screen::Definition {
            self.draw_definition_popup(canvas);
            return;
        }
        let title = match (self.screen, self.book_list.as_ref()) {
            (Screen::BookList, Some(BookList::Favorites)) => "Favorites",
            (Screen::BookList, Some(BookList::Recent)) => "Recently read",
            (Screen::BookList, Some(BookList::Authors)) => "Authors",
            (Screen::BookList, Some(BookList::Author(author))) => {
                author.as_deref().unwrap_or("Unknown author")
            }
            (Screen::BookList, Some(BookList::Status(status))) => status.label(),
            (Screen::BookList, Some(BookList::Collection(name))) => name,
            _ => self.screen.title(),
        };
        header(canvas, title, "Back");
        if self.screen==Screen::LibraryExport(library_export::Page::Colors) {self.draw_export_colors(canvas);return;}
        if self.screen == Screen::VocabularyContext { self.draw_vocabulary_context(canvas); return; }
        if let Screen::BookmarkBrowser(page) = self.screen
            && self.draw_bookmark_browser_controls(canvas, page)
        {
            return;
        }
        if self.screen == Screen::BookMap && self.fixed.is_some() {
            self.draw_fixed_book_map(canvas);
            return;
        }
        if self.screen == Screen::PageOverview {
            self.draw_overview(canvas);
            return;
        }
        if matches!(
            self.screen,
            Screen::Find
                | Screen::LibraryFind
                | Screen::VocabularySearch
                | Screen::DictionaryEntry
                | Screen::BookmarkFind
                | Screen::CollectionName
                | Screen::ProfileName
                | Screen::Note
                | Screen::EditNote
                | Screen::GoToPage
                | Screen::NumberingValue(_)
                | Screen::PdfPassword
        ) {
            let (hint, action) = match self.screen {
                Screen::Note | Screen::EditNote => ("Note text", "Save"),
                Screen::LibraryFind => ("Title, author or filename", "Find"),
                Screen::VocabularySearch => ("Saved word", "Find"),
                Screen::DictionaryEntry => ("Word or phrase", "Look up"),
                Screen::BookmarkFind => ("Bookmark, highlighted text, or note", "Find"),
                Screen::CollectionName => ("Collection name", "Save"),
                Screen::ProfileName => ("Profile name", "Save"),
                Screen::PdfPassword => ("PDF password", "Open"),
                Screen::NumberingValue(NumberField::MarginSize) => ("Margin text size from 8 to 20", "Set"),
                Screen::NumberingValue(_) => ("500 to 3,000 characters per page", "Set"),
                Screen::GoToPage if self.fixed.is_none() => {
                    let hint = if self
                        .active
                        .as_ref()
                        .is_some_and(|book| book.numbering.is_some() || !book.reading.probe.printed_pages.is_empty())
                    {
                        "Page, chapter:page, book %, or label followed by a stable page"
                    } else {
                        "Page, chapter:page, or book percentage"
                    };
                    (hint, "Go")
                }
                Screen::GoToPage => ("Page, printed label, or % of document", "Go"),
                Screen::Find if self.active.is_some() && self.find_regex => ("Regular expression", "Find all"),
                Screen::Find => ("Search text", "Find all"),
                _ => ("Search text", "Find"),
            };
            canvas.single_line_field(
                Rect::new(LIST_X, LIST_Y, WIDTH as i32 - 2 * LIST_X, 130),
                &self.editor,
                hint,
                self.screen == Screen::PdfPassword,
            );
            canvas.keyboard(self.editor.layer, action, None);
            if self.can_paste_address() { external_link::draw_paste(canvas); }
            if self.screen == Screen::BookmarkFind {
                self.annotation_search.draw_filters(canvas);
            }
            if self.screen == Screen::Find {
                let label = if self.find_match_case {
                    "Match case on"
                } else {
                    "Match case off"
                };
                canvas.button(FIND_CASE, label, self.find_match_case, false);
                canvas.button(FIND_ACCENTS, "Ignore accents", self.find_ignore_accents, false);
                if self.active.is_some() {
                    canvas.button(FIND_PATTERN, "Regular expression", self.find_regex, false);
                }
                self.draw_nearby(canvas);
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
        let (count, mut items) = self.list_window();
        if let Some(current) = self
            .current_contents_row
            .and_then(|row| row.checked_sub(self.list_page * ROWS_PER_PAGE))
            .and_then(|row| items.get_mut(row))
        {
            let context = match self.screen {
                Screen::PublisherPages if self.active.as_ref().is_some_and(|book| book.numbering.is_some()) => "Current stable page",
                Screen::PublisherPages => "Current printed page",
                _ => "Current section",
            };
            current.detail = Some(match current.detail.take() {
                Some(detail) => format!("{context} · {detail}"),
                None => context.into(),
            });
        }
        let rows: Vec<_> = items
            .iter()
            .map(|item| Row {
                title: if self.screen == Screen::Matches { "" } else { &item.title },
                detail: item.detail.as_deref(),
                accessory: if self.screen == Screen::MetadataSaveSettings
                    || matches!(item.action, Action::Heading)
                {
                    RowAccessory::None
                } else {
                    RowAccessory::Disclosure
                },
            })
            .collect();
        let group = Rows { bounds: list_rect(rows.len()), rows: &rows };
        group.draw(canvas, None);
        if self.screen == Screen::BookMap && let Some(active) = &self.active {
            for (index, item) in items.iter().enumerate() {
                if let Action::Jump(target) = &item.action
                    && let Some(chapter) = active.reading.probe.chapter_index(&target.path)
                    && active.reading.hidden_chapter(chapter).is_some()
                    && let Some(rect) = group.row_rect(index) {
                    // Gray rail keeps the shared row's text and disclosure target intact.
                    canvas.fill(Rect::new(rect.x + 8, rect.y + 14, 12, rect.h - 28), 187);
                }
            }
        }
        if self.screen == Screen::Matches {
            self.draw_match_rows(canvas, &group);
        }
        if rows.is_empty() {
            let empty = match self.screen {
                Screen::Bookmarks | Screen::FixedBookmarks => "No bookmarks in this book",
                Screen::Highlights => "No highlights or notes in this book",
                Screen::BookmarkMatches => "No matching bookmarks, highlights, or notes",
                Screen::BookmarkBrowser(bookmark_browser::Page::Results) => "No matching saved marks",
                Screen::DictionaryHistory => "No lookups yet",
                Screen::OnlineHistory => "No completed online lookups",
                Screen::SearchHistory => "No recent searches",
                Screen::Vocabulary => "No saved words",
                Screen::Matches if self.search_at.is_some() && self.fixed.is_some() => {
                    "Searching pages"
                }
                Screen::Matches if self.search_at.is_some() || self.nearby.searching_all() => "Searching chapters",
                Screen::Matches => "No matches",
                Screen::GeneratedContents
                    if self
                        .outline
                        .as_ref()
                        .is_some_and(|outline| outline.deadline.is_some()) =>
                {
                    "Building the outline"
                }
                Screen::GeneratedContents => "No readable chapters found",
                _ => "No entries",
            };
            canvas.text(
                Rect::new(LIST_X, LIST_Y, WIDTH as i32 - 2 * LIST_X, 110),
                empty,
                BODY,
            );
        }
        if self.has_list_shortcuts(count) {
            let last = count.saturating_sub(1) / ROWS_PER_PAGE;
            for (rect, icon, available) in [
                (FIRST_MATCH_PAGE, bar_icons::FIRST_PAGE, self.list_page > 0),
                (
                    PREV_MATCH_PAGE,
                    bar_icons::PREVIOUS_PAGE,
                    self.list_page > 0,
                ),
                (NEXT_MATCH_PAGE, bar_icons::NEXT_PAGE, self.list_page < last),
                (LAST_MATCH_PAGE, bar_icons::LAST_PAGE, self.list_page < last),
            ] {
                if available {
                    canvas.button(rect, "", false, false);
                }
                canvas.icon(
                    Rect::new(
                        rect.x + (rect.w - 42) / 2,
                        rect.y + (rect.h - 42) / 2,
                        42,
                        42,
                    ),
                    icon,
                    if available { 20 } else { 178 },
                );
            }
            let first = self.list_page * ROWS_PER_PAGE + 1;
            let last = (first - 1 + ROWS_PER_PAGE).min(count);
            let more = if self.nearby.searching_all() || self.search_at.is_some() { "+" } else { "" };
            canvas.text(
                Rect::new(435, HEIGHT as i32 - 104, WIDTH as i32 - 870, 72),
                &format!("{first}–{last} of {count}{more}"),
                CAPTION,
            );
            if let Some(notice) = &self.notice {
                canvas.text(
                    Rect::new(LIST_X, HEIGHT as i32 - 179, WIDTH as i32 - 2 * LIST_X, 64),
                    notice,
                    CAPTION,
                );
            }
        } else {
            if self.list_page > 0 {
                canvas.button(PREV, "Previous", false, false);
            }
            if (self.list_page + 1) * ROWS_PER_PAGE < count {
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
        if let Some(popup) = &self.result_context { popup.draw(canvas); }
        if self.screen == Screen::ClearOnlineHistory { self.draw_clear_online_history(canvas); }
    }

}

impl App for Reader {
    fn draw(&self, canvas: &mut Canvas) {
        self.draw_content(canvas);
        self.draw_flow_popup(canvas);
        self.read_alarm.draw(canvas, self.screen == Screen::Book);
        self.draw_vocabulary_dialog(canvas);
        self.draw_catalog_popup(canvas);
        self.draw_sync_dialog(canvas);
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

    fn swipe(&mut self, input: Input, start_x: f32) -> Change {
        if let Screen::Catalog(page) = self.screen && let Some(change) = self.catalog_input(page,input) { return change; }
        if let Some(change) = self.sync_dialog_input(input) { return change; }
        if let Some(change) = self.vocabulary_dialog_input(input) { return change; }
        if self.flow_popup.is_some() { return self.flow_popup_input(input); }
        if self.read_alarm.modal() { return self.read_alarm_input(input); }
        if let Some(change) = self.reading_history_input(input) { return change; }
        if self.end_dialog.is_some() { return self.end_dialog_input(input); }
        if self.range_edit.is_some() { return self.range_input(input); }
        if self.saved_popup_over_book() {
            return self.saved_popup_input(input);
        }
        if let Screen::BookmarkBrowser(page) = self.screen {
            return self.bookmark_browser_input(page, input).unwrap_or_else(|| self.input(input));
        }
        if self.screen == Screen::PageOverview
            && let Input::Swipe { dx, start_y, .. } = input
            && OVERVIEW_TRACK.contains(start_x, start_y)
        {
            return self.scrub_overview(start_x + dx);
        }
        if let Some(change) = self.swipe_display_control(input, start_x) {
            return change;
        }
        self.assigned_gesture(input, Some(start_x))
            .or_else(|| self.reading_edge_swipe(input, start_x))
            .unwrap_or_else(|| self.input(input))
    }

    fn preferred_rotation(&self) -> Option<primitive_app::Rotation> {
        use primitive_app::Rotation;

        match self.controls.orientation {
            ReadingOrientation::FollowLauncher => None,
            ReadingOrientation::Upright => Some(Rotation::Upright),
            ReadingOrientation::UpsideDown => Some(Rotation::UpsideDown),
        }
    }

    fn preferred_dark_mode(&self) -> Option<bool> {
        // Under Primitive Launcher the menus follow the launcher's theme, which the app kit reads.
        if self.under_launcher() {
            return None;
        }
        self.controls.palette.inverted()
    }

    fn input(&mut self, input: Input) -> Change {
        // Any further user action owns the selection; late automatic results
        // must never replace a manually extended range or an open popup.
        self.cjk_selection_lookup = None;
        if let Some(change) = self.sync_dialog_input(input) { return change; }
        if let Some(change) = self.vocabulary_dialog_input(input) { return change; }
        if let Some(change) = self.document_defaults_input(input) { return change; }
        if input == Input::Sleep {
            self.cancel_catalog_for_sleep();
            self.cancel_progress_sync();
            self.spelling_lookup = None;
            self.cancel_library_export();
        }
        if input == Input::Sleep && let Some(panel) = &mut self.online && panel.job.take().is_some() {
            panel.message = Some("Lookup cancelled when Reader went to sleep.".into());
        }
        if matches!(input, Input::Sleep) && self.ocr.take().is_some() {
            self.notice = None;
        }
        if matches!(input, Input::Sleep | Input::Wake) {
            self.stats_awake = input == Input::Wake;
            if input == Input::Wake {
                self.metadata_checkpoint = metadata_save::Checkpoint::new(Instant::now());
            }
            self.sync_auto_turn(Instant::now());
            self.pages_since_refresh = 0;
            if with_storage(|| Ok(())).is_ok() {
                self.update_stats_activity(Instant::now());
            } else {
                self.stats.discard_active();
                self.reading_history.discard_active();
                self.pace.storage_returned();
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
            return if input == Input::Wake { self.poll_read_alarm(Instant::now(), true) } else { Change::None };
        }
        if self.read_alarm.modal() { return self.read_alarm_input(input); }
        if let Some(change) = self.reading_history_input(input) { return change; }
        self.sync_auto_turn(Instant::now());
        if let Screen::Catalog(page) = self.screen && let Some(change) = self.catalog_input(page,input) { return change; }
        if self.flow_popup.is_some() { return self.flow_popup_input(input); }
        self.auto_turn.activity(Instant::now());
        if self.end_dialog.is_some() { return self.end_dialog_input(input); }
        if self.link_popup.is_some() { return self.link_address_input(input); }
        if self.can_paste_address() && matches!(input, Input::Tap(x, y) if external_link::PASTE.contains(x,y)) {
            return self.paste_address();
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
        if self.range_edit.is_some() { return self.range_input(input); }
        if self.saved_popup_over_book() {
            return self.saved_popup_input(input);
        }
        if let Screen::BookmarkBrowser(page) = self.screen
            && let Some(change) = self.bookmark_browser_input(page, input)
        {
            return change;
        }
        if self.screen==Screen::LibraryExport(library_export::Page::Colors) {return self.export_colors_input(input);}
        if matches!(self.screen, Screen::Book | Screen::Fixed)
            && let Some(change) = self.overlay_footer_input(input)
        {
            return change;
        }
        if let Input::Tap(x, y) = input && let Some(change) = self.tap_saved_mark(x, y) {
            return change;
        }
        if let Some(change) = self.assigned_gesture(input, None) {
            return change;
        }
        if matches!(self.screen, Screen::Book | Screen::Fixed)
            && self.selection.is_none()
            && self.pdf_selection.is_none()
            && matches!(input, Input::Tap(x, y) if x >= WIDTH as f32 - 58.0 && y < 58.0)
        {
            return match self.toggle_bookmark() {
                Ok(()) => self.applied_change(),
                Err(error) => self.report(error),
            };
        }
        if self.screen == Screen::ReadFixedText {
            if matches!(input, Input::Tap(x, y) if DONE.contains(x, y)) {
                self.show(Screen::Fixed);
                return Change::Partial;
            }
            if let (Some(flow), Some(fixed)) = (&mut self.text_flow, &self.fixed) {
                let Some(input) = self.controls.page_input(input) else {
                    return Change::None;
                };
                let result = flow.input(input, &fixed.reading);
                return self.finish_text_flow_update(result);
            }
        }
        if self.screen == Screen::Book {
            return self.handle_book(input);
        }
        if self.screen == Screen::Illustration {
            if matches!(input, Input::Tap(x, y) if DONE.contains(x, y)) {
                self.show(if self.book_details.as_ref().is_some_and(|details|
                    details.back == Screen::BookmarkBrowser(bookmark_browser::Page::Book)) {
                    Screen::BookmarkBrowser(bookmark_browser::Page::Book)
                } else if self.book_details.is_some() {
                    Screen::BookDetails
                } else if self.fixed.is_some() {
                    Screen::Fixed
                } else {
                    Screen::Book
                });
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
        if self.screen == Screen::Hyphenate {
            return self.hyphenation_input(input);
        }
        if self.screen == Screen::CustomTitle { return self.chapter_editor_input(input); }
        if self.screen == Screen::ClearOnlineHistory { return self.clear_online_history_input(input); }
        if self.screen == Screen::Footnote {
            return self.footnote_input(input);
        }
        if self.screen == Screen::OnlineLookup { return self.online_input(input); }
        if self.screen == Screen::OnlineEdit { return self.online_editor_input(input); }
        if self.screen == Screen::Definition {
            return self.definition_input(input);
        }
        if let Screen::SyncEdit(field) = self.screen { return self.sync_editor_input(input, field); }
        if self.screen == Screen::VocabularyContext { return self.vocabulary_context_input(input); }
        if matches!(self.screen, Screen::GoToPage | Screen::NumberingValue(_)) {
            return self.handle_page_number(input);
        }
        if self.screen == Screen::BookMap && self.fixed.is_some() {
            return self.fixed_map_input(input);
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
            Screen::Find
                | Screen::LibraryFind
                | Screen::VocabularySearch
                | Screen::DictionaryEntry
                | Screen::BookmarkFind
                | Screen::Note
                | Screen::EditNote
        ) {
            return self.handle_find(input);
        }
        if self.screen == Screen::Library {
            return self.library_input(input);
        }
        if let Some(popup) = &mut self.result_context {
            if let Input::Swipe { dx, dy, .. } = input && dy.abs() > dx.abs() {
                popup.scroll(dy);
                return Change::Partial;
            }
            if matches!(input, Input::Tap(..) | Input::PageBack | Input::PageForward) {
                self.result_context = None;
                return Change::Partial;
            }
            return Change::None;
        }
        let (count, mut items) = self.list_window();
        let broad = self.has_list_shortcuts(count);
        let (prev, next) = if broad {
            (PREV_MATCH_PAGE, NEXT_MATCH_PAGE)
        } else {
            (PREV, NEXT)
        };
        let direction = match input {
            Input::Tap(x, y) if broad && FIRST_MATCH_PAGE.contains(x, y) && self.list_page > 0 => {
                self.list_page = 0;
                return Change::Partial;
            }
            Input::Tap(x, y)
                if broad
                    && LAST_MATCH_PAGE.contains(x, y)
                    && (self.list_page + 1) * ROWS_PER_PAGE < count =>
            {
                self.list_page = count.saturating_sub(1) / ROWS_PER_PAGE;
                return Change::Partial;
            }
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
                    Screen::LibraryExport(page) => self.back_from_library_export(page),
                    Screen::Catalog(page) => self.catalog_back(page),
                    Screen::OrganizeLibrary => self.show(Screen::Library),
                    Screen::BookList if matches!(self.book_list, Some(BookList::Author(_))) => {
                        self.book_list = Some(BookList::Authors);
                        self.show(Screen::BookList);
                    }
                    Screen::BookList => self.show(Screen::OrganizeLibrary),
                    Screen::RemoveCollection => self.show(Screen::BookList),
                    Screen::GestureBinding(gesture) => {
                        self.show(Screen::Gestures);
                        self.list_page = Gesture::ALL
                            .iter()
                            .position(|entry| entry.0 == gesture)
                            .unwrap()
                            / ROWS_PER_PAGE;
                    }
                    Screen::Gestures => self.show(Screen::ReadingControls),
                    Screen::ReadingFonts => self.show(Screen::Typography),
                    Screen::DictionaryOptions(i) => {
                        self.show(Screen::Dictionaries);
                        self.list_page = i / ROWS_PER_PAGE;
                    }
                    Screen::Dictionaries => self.show(self.dictionary_origin),
                    Screen::FooterSettings | Screen::EndActionSettings | Screen::MetadataSaveSettings => {
                        self.show(Screen::ReadingControls)
                    }
                    Screen::DocumentDefaults => self.show(Screen::ReadingControls),
                    Screen::ReadingProfiles => self.show(Screen::Typography),
                    Screen::RemoveProfiles => self.show(Screen::ReadingProfiles),
                    Screen::BookDetails => {
                        if self.book_details.as_ref().is_some_and(|book| book.close_document) {
                            return match self.leave_finished_book() {
                                Ok(()) => Change::Partial,
                                Err(error) => self.report(error),
                            };
                        }
                        let back = self
                            .book_details
                            .as_ref()
                            .map_or(Screen::Library, |book| book.back);
                        self.show(back);
                    }
                    Screen::ReadingHistory(_) => self.history_back(),
                    Screen::ReadingData => self.show(Screen::Library),
                    Screen::ProgressSync => self.show(self.tools_screen()),
                    Screen::ProgressSyncSettings => self.show(Screen::ProgressSync),
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
                    Screen::Highlights | Screen::LocationHistory if self.fixed.is_some() => {
                        self.show(Screen::FixedTools)
                    }
                    Screen::Matches => self.show(Screen::Find),
                    Screen::CustomContents => self.show(self.contents_screen()),
                    Screen::CustomEdits | Screen::CustomBookmarks => self.show(Screen::CustomContents),
                    Screen::NumberingDefaults => self.show(Screen::PageNumbering),
                    Screen::PageNumbering | Screen::PublisherOffer => self.show(Screen::Book),
                    Screen::GeneratedContents => self.show(Screen::Contents),
                    Screen::BookmarkMatches => self.show(Screen::BookmarkFind),
                    Screen::Annotation | Screen::ImportAnnotations => self.show(Screen::Highlights),
                    Screen::ImportPosition => self.show(Screen::ReadingControls),
                    Screen::ImportPdfAnnotations => self.show(Screen::Highlights),
                    Screen::ExportAnnotations => self.show(Screen::Highlights),
                    Screen::RemoveHighlight => self.show(Screen::Annotation),
                    Screen::VocabularyWord | Screen::VocabularyOptions => self.show(Screen::Vocabulary),
                    Screen::VocabularyBooks => self.show(Screen::VocabularyOptions),
                    _ if self.fixed.is_some() => self.show(Screen::FixedTools),
                    _ => self.show(Screen::BookTools),
                }
                return Change::Partial;
            }
            Input::Tap(x, y) if prev.contains(x, y) => -1,
            Input::Tap(x, y) if next.contains(x, y) => 1,
            Input::LongTap(x, y) if self.screen == Screen::Matches => {
                let Some(index) = list_item_at(self.list_page, count, x, y) else {
                    return Change::None;
                };
                return match self.preview_result(index) {
                    Ok(()) => Change::Partial,
                    Err(error) => self.report(error),
                };
            }
            Input::LongTap(x, y) if self.screen == Screen::BookList => {
                let Some(index) = list_item_at(self.list_page, count, x, y) else {
                    return Change::None;
                };
                let entry = match items.swap_remove(index % ROWS_PER_PAGE).action {
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
                    let action = items.swap_remove(index % ROWS_PER_PAGE).action;
                    return match self.activate(action) {
                        Ok(()) => self.applied_change(),
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
            .chain(self.export_deadline())
            .chain(self.catalog.job.as_ref().map(|job| job.poll_at))
            .chain(self.bookmark_metadata_deadline())
            .chain(self.progress_sync.deadline())
            .chain(self.nearby_marks.as_ref().and_then(|import| import.deadline))
            .chain(self.save_at)
            .chain(self.metadata_checkpoint.deadline())
            .chain(self.stats_awake.then(|| self.read_alarm.deadline()).flatten())
            .chain(self.search_at)
            .chain(self.nearby.deadline())
            .chain(self.spelling_lookup.as_ref().map(|job| job.deadline))
            .chain(self.cjk_selection_lookup.as_ref().map(|job| job.deadline))
            .chain(self.outline.as_ref().and_then(|outline| outline.deadline))
            .chain(self.numbering_scan.as_ref().map(|scan| scan.deadline))
            .chain(
                self.annotation_import
                    .as_ref()
                    .and_then(|import| import.deadline),
            )
            .chain(self.online.as_ref().and_then(|p| p.job.as_ref().map(|j| j.poll_at)))
            .chain(self.ocr.as_ref().map(|job| job.deadline))
            .chain(self.text_flow.as_ref().and_then(TextFlow::deadline))
            .chain(self.stats.deadline())
            .chain(self.reading_history.deadline())
            .chain(
                self.auto_turn
                    .deadline()
                    .filter(|_| self.auto_turn_eligible()),
            )
            .chain(
                self.scan_columns
                    .as_ref()
                    .and_then(|scan| scan.turns.deadline()),
            )
            .min()
    }

    fn tick(&mut self, now: Instant) -> Change {
        if let Some(change) = self.poll_progress_sync(now) { return change; }
        if self.stats_awake {
            let alarm = self.poll_read_alarm(now, false);
            if alarm != Change::None { return alarm; }
        }
        if let Some(change) = self.poll_cjk_selection(now) { return change; }
        if let Some(change) = self.poll_spelling_lookup(now) { return change; }
        if let Some(change) = self.poll_online(now) { return change; }
        if let Some(change) = self.poll_catalog(now) { return change; }
        if self.save_at.is_some_and(|at| at <= now)
            || self.metadata_checkpoint.deadline().is_some_and(|at| at <= now)
        {
            self.save_at = None;
            self.metadata_checkpoint.cancel();
            if let Err(error) = self.save_library_at(now)
                && error.kind() != io::ErrorKind::WouldBlock
            {
                return self.report(error);
            }
        }
        if self.reading_history.deadline().is_some_and(|at| at <= now)
            && let Err(error) = self.reading_history.save(now, crate::mark_time::now(), false)
            && error.kind() != io::ErrorKind::WouldBlock { return self.report(error); }
        if self.stats.deadline().is_some_and(|at| at <= now)
            && let Err(error) = self
                .stats
                .checkpoint(&self.stats_path, now, false)
                .and_then(|_| self.pace.checkpoint(now))
            && error.kind() != io::ErrorKind::WouldBlock
        {
            return self.report(error);
        }
        if self.export_deadline().is_some_and(|deadline| deadline <= now) {
            return match self.step_library_export(now) {
                Ok(change) => change,
                Err(error) => {
                    self.show(Screen::LibraryExport(library_export::Page::Options));
                    self.report(error)
                }
            };
        }
        if self.bookmark_metadata_deadline().is_some_and(|deadline| deadline <= now) {
            return match self.step_bookmark_metadata(now) {
                Ok(change) => change,
                Err(error) => self.report(error),
            };
        }
        if self
            .scan_columns
            .as_ref()
            .and_then(|scan| scan.turns.deadline())
            .is_some_and(|at| at <= now)
        {
            return self.step_scan_column(now);
        }
        if self.nearby_marks.as_ref()
            .and_then(|import| import.deadline).is_some_and(|at| at <= now)
        {
            return match self.step_nearby_marks() {
                Ok(change) => change,
                Err(error) => {
                    self.show(Screen::Highlights);
                    self.report(error)
                }
            };
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
        if self.nearby.deadline().is_some_and(|at| at <= now) {
            return match self.poll_nearby(now) {
                Ok(change) => change,
                Err(error) => self.report(error),
            };
        }
        if self.search_at.is_some_and(|at| at <= now) {
            return match self.step_pdf_search(now) {
                Ok(change) => change,
                Err(error) => {
                    self.search_at = None;
                    self.report(error)
                }
            };
        }
        if self
            .outline
            .as_ref()
            .and_then(|outline| outline.deadline)
            .is_some_and(|at| at <= now)
        {
            return match self.step_generated_contents(now) {
                Ok(change) => change,
                Err(error) => {
                    self.show(Screen::Contents);
                    self.report(error)
                }
            };
        }
        if self.numbering_scan.as_ref().is_some_and(|scan| scan.deadline <= now) {
            return match self.step_character_pages(now) {
                Ok(change) => change,
                Err(error) => { self.numbering_scan = None; self.report(error) }
            };
        }
        if self.screen == Screen::ReadFixedText
            && let (Some(flow), Some(fixed)) = (&mut self.text_flow, &self.fixed)
        {
            let result = flow.tick(now, &fixed.reading);
            return self.finish_text_flow_update(result);
        }
        if self.auto_turn_eligible() && self.auto_turn.take_due(now) {
            return self.step_auto_turn(now);
        }
        if self.screen == Screen::Book
            && let Some(active) = &mut self.active
        {
            let previous = (active.reading.chapter, active.reading.page);
            let change = active.reading.tick(now);
            return self.finish_book_update(change, previous);
        }
        if self.screen == Screen::Fixed
            && let Some(fixed) = &mut self.fixed
        {
            let previous = fixed.reading.page();
            let result = fixed.reading.tick(now);
            return self.finish_fixed_update(result, previous);
        }
        Change::None
    }

    fn save(&mut self) -> io::Result<()> {
        self.catalog.cancel();
        self.cancel_progress_sync();
        self.save_library()?;
        let now = Instant::now();
        self.stats.checkpoint(&self.stats_path, now, true)?;
        self.reading_history.save(now, crate::mark_time::now(), true)?;
        self.pace.checkpoint(now)?;
        if let Err(error) = self.save_bookmark_metadata() {
            eprintln!("reader metadata_cache_save_failed={error}");
        }
        Ok(())
    }

    fn storage_returned(&mut self) -> io::Result<()> {
        self.document_defaults.reload();
        self.cancel_progress_sync();

        self.cjk_selection_lookup = None;
        self.spelling_lookup = None;
        self.cancel_ocr_selection();
        self.pages_since_refresh = 0;
        self.auto_turn.activity(Instant::now());
        if self.screen == Screen::GeneratedContents {
            self.show(Screen::Contents);
        }
        self.stats.discard_active();
        self.reading_history.discard_active();
        self.pace.storage_returned();
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
        self.refresh_browser()?;
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
    let fraction = percent_fraction(percent)?;
    let last = count.saturating_sub(1);
    Ok(((last as f64 * fraction).round() as usize).min(last))
}

fn percent_fraction(input: &str) -> Result<f64> {
    let percent = input
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|value| (0.0..=100.0).contains(value))
        .ok_or("Enter a percentage from 0% to 100%")?;
    Ok(percent / 100.0)
}

fn definition_bounds(height: i32, selection: Option<Rect>) -> Rect {
    const TOP: i32 = 64;
    let above_selection = selection.is_some_and(|selected| {
        selected.y >= PAGE_HEIGHT as i32 / 2 && TOP + height + 24 <= selected.y
    });
    let y = if above_selection {
        TOP
    } else {
        DEFINITION_BOTTOM - height
    };
    Rect::new(64, y, WIDTH as i32 - 128, height)
}

fn definition_actions(popup: Rect) -> [Rect; 3] {
    let y = popup.bottom() - 82;
    [
        Rect::new(94, y, 214, 66),
        Rect::new(WIDTH as i32 / 2 - 105, y, 210, 66),
        Rect::new(WIDTH as i32 - 308, y, 214, 66),
    ]
}

fn same_pdf_highlight(left: &PdfHighlight, right: &PdfHighlight) -> bool {
    left.parts().any(|(page, bounds, _)| page == right.page
        && bounds.len() == right.bounds.len()
        && bounds.iter().zip(&right.bounds).all(|(a, b)| {
            [a.x0 - b.x0, a.y0 - b.y0, a.x1 - b.x1, a.y1 - b.y1]
                .into_iter()
                .all(|delta| delta.abs() <= 0.000_01)
        }))
}

fn column_view(original: FixedPreferences, bounds: PdfTextRect) -> FixedPreferences {
    FixedPreferences {
        width_fit: true,
        crop: Some(bounds),
        scan_contrast: original.scan_contrast,
        dither: original.dither,
        straighten_limit: original.straighten_limit,
                        force_ocr: original.force_ocr,
        scan_white_level: original.scan_white_level,
        continuous_gap: original.continuous_gap,
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

fn overview_start(first: Option<usize>, count: usize) -> usize {
    let last = count.saturating_sub(1) / OVERVIEW_PAGES * OVERVIEW_PAGES;
    first.map_or(last, |first| first.min(last))
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

fn map_detail(current: bool, bookmarks: usize, highlights: usize) -> String {
    let markers = format!("Bookmarks {bookmarks} · Highlights {highlights}");
    if current {
        format!("Current position · {markers}")
    } else {
        markers
    }
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

fn library_book_item(entry: LibraryBook) -> Item {
    let title = entry.title.clone();
    let mut detail = file_type(&entry.path);
    if let Some(author) = &entry.author {
        detail.push_str(" · ");
        detail.push_str(author);
    }
    detail.push_str(" · ");
    detail.push_str(&entry.path.to_string_lossy());
    Item::new(title, Action::Open(entry)).detail(detail)
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

fn draw_locked_footer(canvas: &mut Canvas, notice: Option<&str>) {
    let top = PAGE_HEIGHT as i32;
    canvas.fill(Rect::new(0, top, WIDTH as i32, HEIGHT as i32 - top), 255);
    canvas.text(
        Rect::new(38, top + 12, UNLOCK_TOUCH.x - 60, 48),
        notice.unwrap_or("Touch locked. Use the physical page buttons."),
        CAPTION,
    );
    canvas.button(UNLOCK_TOUCH, "Unlock touch", true, false);
    canvas.button(DONE, "Exit", false, false);
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

fn text_size_items(settings: Settings) -> [Item; 2] {
    let size = format!("Current size {} px", settings.font_size.round());
    [
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
    ]
}

fn text_items(settings: Settings, scroll: bool) -> Vec<Item> {
    let margin = format!("Current margin {} px", settings.margin);
    let spacing = format!("Current spacing {:.2}×", settings.line_height);
    let (alignment, next_alignment) = match settings.alignment {
        TextAlignment::Publisher => ("Publisher", TextAlignment::Start),
        TextAlignment::Start => ("Start", TextAlignment::Center),
        TextAlignment::Center => ("Centered", TextAlignment::Justify),
        TextAlignment::Justify => ("Justified", TextAlignment::Publisher),
    };
    let [larger, smaller] = text_size_items(settings);
    vec![
        Item::new(
            if scroll {
                "Switch to pages"
            } else {
                "Switch to continuous scroll"
            },
            Action::ReadingMode(!scroll),
        ),
        larger,
        smaller,
        Item::new(
            "Wider margins",
            Action::Text(Settings {
                margin: (settings.margin + 12).min(150),
                margin_sides: None,
                margin_left: None,
                margin_right: None,
                margin_top: None,
                margin_bottom: None,
                ..settings
            }),
        )
        .detail(&margin),
        Item::new(
            "Narrower margins",
            Action::Text(Settings {
                margin: settings.margin.saturating_sub(12),
                margin_sides: None,
                margin_left: None,
                margin_right: None,
                margin_top: None,
                margin_bottom: None,
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
            "Hyphenation",
            Action::Text(settings.with_hyphenation(settings.hyphenation_mode().next())),
        )
        .detail(settings.hyphenation_mode().label()),
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
        user_hyphenation: Default::default(),
        chapter_offsets: Default::default(),
        engine,
        book,
        fonts,
        settings: device_settings(),
        scroll: false,
        facing_pages: false,
        hide_nonlinear: false,
        hidden_flows: Default::default(),
    })?;
    primitive_app::run(reading, home).map_err(Into::into)
}

#[cfg(test)]
mod external_range_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use primitive_ui::GrayImage;
    use std::io::{Cursor, Write};
    use std::sync::atomic::{AtomicUsize, Ordering};

    static FIXTURE_ID: AtomicUsize = AtomicUsize::new(0);

    pub(super) struct Fixture {
        pub(super) root: PathBuf,
        pub(super) book: PathBuf,
        pub(super) state: PathBuf,
    }

    impl Fixture {
        pub(super) fn new() -> Self {
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
    fn library_browses_folders_and_distinguishes_openable_documents() {
        let fixture = Fixture::new();
        let folder = fixture.root.join("A Reference.pdf");
        fs::create_dir(&folder).unwrap();
        let nested = folder.join("Volume 1");
        fs::create_dir(&nested).unwrap();
        fs::write(nested.join("Archive.zip"), b"archive").unwrap();
        fs::write(nested.join("Essay.TXT"), b"A readable essay.").unwrap();
        let mut app = fixture.app();
        let tap_row = |index| {
            Input::Tap(
                150.0,
                (library_view::LIBRARY_LIST_Y + ROW_HEIGHT * index + 30) as f32,
            )
        };

        let first = &app.items()[0];
        assert_eq!(first.title, "A Reference.pdf");
        assert!(first.folder);
        assert_eq!(first.detail.as_deref(), Some("Folder"));
        assert_eq!(app.input(tap_row(0)), Change::Partial);
        assert_eq!(app.items()[0].title, "Volume 1");
        assert_eq!(app.input(tap_row(0)), Change::Partial);

        let files = app.items();
        assert_eq!(files[0].title, "Archive.zip");
        assert_eq!(
            files[0].detail.as_deref(),
            Some("ZIP · 7 B · Not supported in Reader")
        );
        assert_eq!(app.input(tap_row(0)), Change::None);
        assert_eq!(app.items()[1].detail.as_deref(), Some("TXT · 17 B"));
        assert_eq!(app.input(tap_row(1)), Change::Partial);
        assert_eq!(app.screen, Screen::Book);
        app.activate(Action::Show(Screen::Library)).unwrap();
        assert_eq!(app.items()[0].title, "Archive.zip");

        app.library_query = "Essay".into();
        let results = app.items();
        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].detail.as_deref(),
            Some("TXT · A Reference.pdf/Volume 1/Essay.TXT")
        );
        assert_eq!(app.input(Input::Tap(100.0, 170.0)), Change::Partial);
        assert_eq!(app.items()[0].title, "Archive.zip");
        assert_eq!(app.input(Input::PageBack), Change::Partial);
        assert_eq!(app.items()[0].title, "Volume 1");

        fs::remove_dir_all(&folder).unwrap();
        app.storage_returned().unwrap();
        assert!(app.items().iter().all(|item| item.title != "Volume 1"));
        assert_eq!(app.screen, Screen::Library);
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
                (library_view::LIBRARY_LIST_Y + ROW_HEIGHT * index as i32 + 33) as f32
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
    fn previews_a_cover_and_series_from_library_without_creating_progress() {
        let fixture = Fixture::new();
        let mut original =
            zip::ZipArchive::new(Cursor::new(fs::read(&fixture.book).unwrap())).unwrap();
        let mut repacked = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for index in 0..original.len() {
            let mut file = original.by_index(index).unwrap();
            let name = file.name().to_owned();
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes).unwrap();
            if name == "EPUB/package.opf" {
                bytes = String::from_utf8(bytes).unwrap().replace(
                    "media-type=\"image/png\"/>", "media-type=\"image/png\" properties=\"cover-image\"/>",
                ).replace("</metadata>", concat!(
                    "<meta property=\"belongs-to-collection\" id=\"series\">Harbor Notes</meta>",
                    "<meta refines=\"#series\" property=\"collection-type\">series</meta>",
                    "<meta refines=\"#series\" property=\"group-position\">2.2.1</meta>",
                    "</metadata>"
                )).into_bytes();
            }
            repacked
                .start_file(
                    name,
                    zip::write::SimpleFileOptions::default()
                        .compression_method(zip::CompressionMethod::Stored),
                )
                .unwrap();
            repacked.write_all(&bytes).unwrap();
        }
        fs::write(&fixture.book, repacked.finish().unwrap().into_inner()).unwrap();
        let mut app = fixture.app();
        let entry = app.library.resolve(&fixture.book).unwrap();
        app.organize_book(entry, Screen::Library).unwrap();
        let metadata = app
            .book_details
            .as_ref()
            .unwrap()
            .metadata
            .as_ref()
            .unwrap();
        assert!(metadata.has_cover);
        assert_eq!(metadata.series[0].name, "Harbor Notes");
        assert_eq!(metadata.series[0].position.as_deref(), Some("2.2.1"));
        assert!(
            app.items()
                .iter()
                .any(|item| matches!(item.action, Action::ViewCover))
        );
        app.activate(Action::ViewCover).unwrap();
        assert_eq!(app.screen, Screen::Illustration);
        assert!(app.illustration.is_some());
        assert_eq!(
            app.input(Input::Tap((DONE.x + 15) as f32, (DONE.y + 15) as f32)),
            Change::Partial
        );
        assert_eq!(app.screen, Screen::BookDetails);
        assert!(app.library.recently_read().next().is_none());
    }

    #[test]
    fn under_the_launcher_the_whole_reader_follows_its_theme_with_no_switch_of_its_own() {
        let fixture = Fixture::new();
        let mut app = fixture.app();
        let titles = |app: &Reader| {
            app.reading_settings_items()
                .into_iter()
                .map(|item| item.title)
                .collect::<Vec<_>>()
        };
        let rows = |app: &Reader| app.screen_row_labels();
        let controls = ReadingControls {
            palette: crate::reading_controls::ScreenPalette::Light,
            ..ReadingControls::default()
        };
        app.activate(Action::SetReadingControls(controls)).unwrap();
        // Standalone: Reader's own switches.
        assert_eq!(app.preferred_dark_mode(), Some(false));
        assert!(!app.uses_night_pages());
        assert!(titles(&app).iter().any(|t| t == "Interface palette"));
        assert!(titles(&app).iter().any(|t| t == "Night pages"));
        assert!(rows(&app).iter().any(|r| r == "Page colors"));

        let settings = fixture.root.join("launcher-settings.txt");
        fs::write(&settings, "dark_mode=on\n").unwrap();
        app.system_theme = Some(SystemTheme::new(settings.clone()));
        let page = GrayImage {
            width: WIDTH,
            height: PAGE_HEIGHT,
            pixels: vec![255; (WIDTH * PAGE_HEIGHT) as usize],
        };
        // Each frame reads the launcher's theme again (`draw`).
        app.system_theme.as_ref().unwrap().refresh();
        // The menus follow the launcher (the app kit reads its theme), and the pages are dark too.
        assert_eq!(app.preferred_dark_mode(), None);
        assert!(app.uses_night_pages());
        assert_eq!(app.page_paint().apply(page.clone()).1.pixels[0], 0);
        assert!(!titles(&app).iter().any(|t| t == "Interface palette"));
        assert!(!titles(&app).iter().any(|t| t == "Night pages"));
        assert_eq!(rows(&app), ["Screen orientation"]);

        fs::write(&settings, "dark_mode=off\n").unwrap();
        app.system_theme.as_ref().unwrap().refresh();
        assert!(!app.uses_night_pages(), "the launcher's light mode");
        assert_eq!(app.page_paint().apply(page).1.pixels[0], 255);
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
            ..ReadingControls::default()
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
            Some(Input::Tap(WIDTH as f32 - 100.0, 700.0))
        );
        reopened.open_path(&fixture.book).unwrap();
        reopened.show(Screen::BookTools);
        tap_reading_control(&mut reopened, "Contents");
        assert_eq!(reopened.screen, Screen::Contents);
    }

    #[test]
    fn full_refresh_counts_coalesced_page_frames_and_persists_the_interval() {
        use crate::reading_controls::PageRefresh;

        let fixture = Fixture::new();
        let long = fixture.root.join("Long Chapter.epub");
        fs::write(&long, include_bytes!("../tests/fixtures/long-chapter.epub")).unwrap();
        let mut app = fixture.app();
        app.activate(Action::SetReadingControls(ReadingControls {
            page_refresh: PageRefresh::EveryFivePages,
            ..app.controls
        }))
        .unwrap();
        let mut app = fixture.app();
        assert_eq!(app.controls.page_refresh, PageRefresh::EveryFivePages);
        app.open_path(&long).unwrap();

        let deadline = |app: &Reader| app.active.as_ref().unwrap().reading.deadline().unwrap();
        assert_eq!(app.input(Input::PageForward), Change::None);
        assert_eq!(app.input(Input::PageBack), Change::None);
        assert_eq!(app.tick(deadline(&app)), Change::None);
        assert_eq!(app.pages_since_refresh, 0);

        for frame in 1..=5 {
            assert_eq!(app.input(Input::PageForward), Change::None);
            assert_eq!(app.input(Input::PageForward), Change::None);
            let expected = if frame == 5 {
                Change::Full
            } else {
                Change::Partial
            };
            assert_eq!(app.tick(deadline(&app)), expected);
            assert_eq!(app.active.as_ref().unwrap().reading.page, frame * 2);
        }
        assert_eq!(app.pages_since_refresh, 0);
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
            automatic_hyphenation: false,
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
        assert_eq!(
            reset.config.settings.hyphenation_mode(),
            crate::HyphenationMode::Automatic
        );
        assert!(!reset.config.scroll);
        assert_eq!(position(reset).anchor, original.anchor);
        app.save().unwrap();

        let mut reopened = fixture.app();
        reopened.open_path(&fixture.book).unwrap();
        reopened.activate(Action::ApplyProfile(0)).unwrap();
        let reading = &reopened.active.as_ref().unwrap().reading;
        assert_eq!(reading.config.settings.font_size, 58.0);
        assert_eq!(
            reading.config.settings.hyphenation_mode(),
            crate::HyphenationMode::Authored
        );
        assert!(reading.config.scroll);
        assert_eq!(position(reading).anchor, original.anchor);
        reopened.activate(Action::RemoveProfile(0)).unwrap();
        assert!(fixture.app().profiles.is_empty());
    }

    #[test]
    fn adjacent_koreader_metadata_is_previewed_without_changing_either_book_or_source() {
        let fixture = Fixture::new();
        let pdf = fixture.root.join("Log.book.PDF");
        fs::write(&pdf, crate::fixed::tests::document()).unwrap();
        let mut app = fixture.app();
        app.open_path(&pdf).unwrap();
        let hash = md5::Digest(koreader_sidecar::partial_md5(&mut fs::File::open(&pdf).unwrap()).unwrap());
        let progress = |page| format!("return {{last_page = {page}, doc_pages = 2, partial_md5_checksum = '{hash:x}'}}");
        let nearby = pdf.with_extension("sdr");
        fs::create_dir(&nearby).unwrap();
        let metadata = nearby.join("metadata.PDF.lua");
        fs::write(&metadata, progress(2)).unwrap();
        fs::write(nearby.join("metadata.epub.lua"), "return {}").unwrap();
        fs::write(fixture.root.join("koreader-progress.lua"), progress(1)).unwrap();
        app.activate(Action::PreviewKoReaderPosition).unwrap();
        assert!(app.items().iter().any(|item| item.title == "Saved beside this book"));
        assert_eq!(app.position_import.as_ref().unwrap().target.as_ref().unwrap().page, 1);
        assert_eq!(app.current_position().unwrap().page, 0);
        app.activate(Action::ApplyKoReaderPosition).unwrap();
        assert_eq!(app.current_position().unwrap().page, 1);
        app.show(Screen::ReadingData);
        tap_contents_item(&mut app, "Import a copied KOReader position");
        assert!(app.items().iter().any(|item| item.title == "Copied to Reader data"));
        assert_eq!(app.position_import.as_ref().unwrap().target.as_ref().unwrap().page, 0);
        assert_eq!(app.current_position().unwrap().page, 1);
        assert_eq!(fs::read_to_string(metadata).unwrap(), progress(2));
    }

    #[test]
    fn real_adjacent_position_uses_an_isolated_profile_and_preserves_source_metadata() {
        let fixture = Fixture::new();
        let external = std::env::var_os("READER_TEST_NEARBY_DOCUMENT").map(PathBuf::from);
        let book = if let Some(path) = external {
            path
        } else {
            let path = fixture.root.join("Coastal chart.pdf");
            fs::write(&path, crate::fixed::tests::document()).unwrap();
            let hash = koreader_sidecar::partial_md5(&mut fs::File::open(&path).unwrap()).unwrap();
            let directory = path.with_extension("sdr");
            fs::create_dir(&directory).unwrap();
            fs::write(directory.join("metadata.pdf.lua"), format!(
                "return {{last_page=2, doc_pages=2, partial_md5_checksum='{:x}'}}", md5::Digest(hash)
            )).unwrap();
            path
        };
        let mut app = Reader::open(book.parent().unwrap(), &fixture.state,
            vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")]).unwrap();
        assert!(app.open_path(&book).is_ok());
        let entry = &app.fixed.as_ref().unwrap().entry;
        let metadata = app.library.nearby_koreader_metadata(&entry.path).unwrap().unwrap();
        let before = fs::read(&metadata).unwrap();
        let expected = koreader_sidecar::parse(&before).unwrap().last_page.unwrap() as usize - 1;
        let old = app.current_position().unwrap();
        app.show(Screen::ReadingControls);
        tap_contents_item(&mut app, "Import KOReader position");
        assert!(app.items().iter().any(|row| row.title == "Saved beside this book"));
        assert_eq!(app.current_position(), Some(old.clone()));
        tap_contents_item(&mut app, "Apply reading position");
        assert_eq!(app.current_position().unwrap().page, expected);
        if old.page != expected {
            app.activate(Action::ReturnToLocation).unwrap();
            assert_eq!(app.current_position(), Some(old));
        }
        assert!(fs::read(metadata).unwrap() == before);
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
        let mut reopened = fixture.app();
        let entry = reopened.library.matching("primitiveos").next().unwrap();
        assert_eq!(entry.author.as_deref(), Some("PrimitiveOS"));
        reopened
            .activate(Action::ListBooks(BookList::Authors))
            .unwrap();
        assert_eq!(reopened.items()[0].title, "PrimitiveOS");
        reopened
            .activate(Action::ListBooks(BookList::Author(Some(
                "PrimitiveOS".into(),
            ))))
            .unwrap();
        assert!(matches!(reopened.items()[0].action, Action::Open(_)));
        reopened
            .activate(Action::ListBooks(BookList::Recent))
            .unwrap();
        assert!(matches!(reopened.items()[0].action, Action::OpenTracked(_)));
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
    fn browses_other_chapters_without_moving_until_a_thumbnail_is_chosen() {
        let fixture = Fixture::new();
        let mut app = fixture.app();
        app.open_path(&fixture.book).unwrap();
        let original = position(&app.active.as_ref().unwrap().reading);
        app.activate(Action::Overview).unwrap();
        let batches = app
            .overview
            .as_ref()
            .unwrap()
            .count
            .div_ceil(OVERVIEW_PAGES);
        for _ in 0..batches {
            assert_eq!(app.input(Input::PageForward), Change::Partial);
        }
        let overview = app.overview.as_ref().unwrap();
        assert_eq!((overview.chapter, overview.first), (Some(1), 0));
        assert!(overview.preview.is_some());
        assert!(overview.tiles[0].is_some());
        assert_eq!(position(&app.active.as_ref().unwrap().reading), original);
        assert_eq!(app.input(Input::PageBack), Change::Partial);
        let overview = app.overview.as_ref().unwrap();
        assert_eq!(overview.chapter, Some(0));
        assert_eq!(overview.first, (batches - 1) * OVERVIEW_PAGES);
        assert_eq!(app.input(Input::PageForward), Change::Partial);
        let tile = overview_tile(0);
        assert_eq!(
            app.input(Input::Tap((tile.x + 20) as f32, (tile.y + 30) as f32)),
            Change::Partial
        );
        assert_eq!(app.active.as_ref().unwrap().reading.chapter, 1);
        app.activate(Action::ReturnToLocation).unwrap();
        assert_eq!(position(&app.active.as_ref().unwrap().reading), original);
    }

    #[test]
    fn jumps_to_a_whole_book_percentage_and_returns_to_the_starting_passage() {
        let fixture = Fixture::new();
        let book = fixture.root.join("Harbor Notebook.epub");
        fs::write(
            &book,
            include_bytes!("../tests/fixtures/many-chapters.epub"),
        )
        .unwrap();
        let mut app = fixture.app();
        app.open_path(&book).unwrap();
        let original = position(&app.active.as_ref().unwrap().reading);
        app.show(Screen::GoToPage);
        app.editor.text = "75%".chars().collect();
        app.jump_reflow_number().unwrap();
        let active = app.active.as_ref().unwrap();
        assert!((35..=36).contains(&active.reading.chapter));
        app.activate(Action::ReturnToLocation).unwrap();
        assert_eq!(position(&app.active.as_ref().unwrap().reading), original);

        app.show(Screen::GoToPage);
        app.editor.text = "100%".chars().collect();
        app.jump_reflow_number().unwrap();
        let reading = &app.active.as_ref().unwrap().reading;
        assert_eq!(reading.chapter + 1, reading.probe.chapter_count());
        assert_eq!(reading.page + 1, reading.probe.page_count());
    }

    #[test]
    fn returns_to_a_chosen_older_location_from_the_reading_toolbar() {
        let fixture = Fixture::new();
        let long = fixture.root.join("Long Chapter.epub");
        fs::write(&long, include_bytes!("../tests/fixtures/long-chapter.epub")).unwrap();
        let mut app = fixture.app();
        app.open_path(&long).unwrap();
        let original = position(&app.active.as_ref().unwrap().reading);
        app.follow_reflow_page(0, 1).unwrap();
        app.follow_reflow_page(0, 2).unwrap();
        app.show(Screen::BookTools);
        assert_eq!(tap_reading_control(&mut app, "Pages"), Change::Partial);
        assert_eq!(tap_reading_control(&mut app, "History"), Change::Partial);
        assert_eq!(app.screen, Screen::LocationHistory);
        assert_eq!(app.items().len(), 2);
        assert_eq!(
            app.input(Input::Tap(
                (LIST_X + 20) as f32,
                (LIST_Y + ROW_HEIGHT + 20) as f32
            )),
            Change::Partial
        );
        assert_eq!(position(&app.active.as_ref().unwrap().reading), original);
        assert!(app.back.is_empty());
    }

    #[test]
    fn imports_a_koreader_epub_pointer_and_keeps_the_sidecar_and_return_position() {
        let fixture = Fixture::new();
        let path = fixture.state.with_file_name("koreader-progress.lua");
        let authored = include_bytes!("../tests/fixtures/koreader-epub-position.lua");
        fs::write(&path, authored).unwrap();
        let fonts = [crate::test_fixture("fonts/NotoSans-Regular.ttf")];
        let mut app = Reader::open(&fixture.root, &fixture.state, fonts.to_vec()).unwrap();
        app.open_path(&fixture.book).unwrap();
        app.activate(Action::Text(Settings { font_size: 50.0, ..device_settings() })).unwrap();
        let original = position(&app.active.as_ref().unwrap().reading);
        app.activate(Action::PreviewKoReaderPosition).unwrap();
        let expected = app
            .position_import
            .as_ref()
            .unwrap()
            .target
            .as_ref()
            .unwrap()
            .anchor
            .clone()
            .unwrap();
        assert_eq!(expected.offset, 4);
        capture_test_screen(&app, &fonts, "READER_KOREADER_POSITION_PREVIEW", false);
        app.activate(Action::ApplyKoReaderPosition).unwrap();
        let reading = &app.active.as_ref().unwrap().reading;
        assert_eq!(reading.probe.page_for(&expected), Some(reading.page));
        assert_eq!(expected.block, reading.probe.fragment("door").unwrap().block);
        assert_ne!(reading.chapter, original.chapter);
        capture_test_screen(&app, &fonts, "READER_KOREADER_POSITION_PAGE", false);
        assert_eq!(fs::read(&path).unwrap(), authored);
        app.activate(Action::ReturnToLocation).unwrap();
        assert_eq!(position(&app.active.as_ref().unwrap().reading), original);

        let invalid = std::str::from_utf8(authored)
            .unwrap()
            .replace("p[1]", "p[900]");
        fs::write(&path, invalid.as_bytes()).unwrap();
        app.activate(Action::PreviewKoReaderPosition).unwrap();
        assert!(app.position_import.as_ref().unwrap().target.is_err());
        assert!(app.activate(Action::ApplyKoReaderPosition).is_err());
        assert_eq!(position(&app.active.as_ref().unwrap().reading), original);
        assert_eq!(fs::read(&path).unwrap(), invalid.as_bytes());
    }

    #[test]
    fn previews_an_exact_koreader_cfi_and_applies_it_only_to_the_matching_book() {
        let fixture = Fixture::new();
        let mut app = fixture.app();
        app.open_path(&fixture.book).unwrap();
        let original = position(&app.active.as_ref().unwrap().reading);
        let sidecar = app.import_path.with_file_name("koreader-progress.lua");
        let cfi = "epubcfi(/6/2!/4/20[tide-b]/1:4)";
        let write_progress = |fingerprint: &str| {
            fs::write(
                &sidecar,
                format!(
                    "return {{last_epubcfi = '{cfi}', partial_md5_checksum = '{fingerprint}'}}"
                ),
            )
            .unwrap();
        };
        write_progress("00000000000000000000000000000000");
        app.activate(Action::PreviewKoReaderPosition).unwrap();
        assert!(app.position_import.as_ref().unwrap().target.is_err());
        assert!(app.activate(Action::ApplyKoReaderPosition).is_err());
        assert_eq!(position(&app.active.as_ref().unwrap().reading), original);

        let fingerprint = md5::Digest(
            koreader_sidecar::partial_md5(&mut fs::File::open(&fixture.book).unwrap()).unwrap(),
        );
        write_progress(&format!("{fingerprint:x}"));
        let bytes = fs::read(&sidecar).unwrap();
        app.activate(Action::PreviewKoReaderPosition).unwrap();
        let target = app
            .position_import
            .as_ref()
            .unwrap()
            .target
            .as_ref()
            .unwrap();
        let anchor = target.anchor.clone().unwrap();
        assert_eq!(target.chapter, 0);
        let reader = &app.active.as_ref().unwrap().reading;
        assert_eq!(anchor.block, reader.probe.fragment("tide-b").unwrap().block);
        assert_eq!(anchor.offset, 4);
        assert_ne!(anchor.block, reader.probe.fragment("tide-a").unwrap().block);
        app.activate(Action::ApplyKoReaderPosition).unwrap();
        let reader = &app.active.as_ref().unwrap().reading;
        assert_eq!(reader.probe.page_for(&anchor), Some(reader.page));
        assert_eq!(fs::read(&sidecar).unwrap(), bytes);
        app.activate(Action::ReturnToLocation).unwrap();
        assert_eq!(position(&app.active.as_ref().unwrap().reading), original);
    }

    #[test]
    fn tap_turns_can_be_disabled_while_leaving_tools_and_buttons_available() {
        let fixture = Fixture::new();
        let mut app = fixture.app();
        app.open_path(&fixture.book).unwrap();
        app.activate(Action::SetReadingControls(ReadingControls {
            disable_page_taps: true,
            ..ReadingControls::default()
        }))
        .unwrap();
        let before = position(&app.active.as_ref().unwrap().reading);
        assert_eq!(
            app.input(Input::Tap(WIDTH as f32 - 25.0, 500.0)),
            Change::None
        );
        assert_eq!(position(&app.active.as_ref().unwrap().reading), before);
        app.input(Input::PageForward);
        if let Some(deadline) = app.deadline() {
            app.tick(deadline);
        }
        assert_ne!(position(&app.active.as_ref().unwrap().reading), before);
        assert_eq!(
            app.input(Input::Tap(WIDTH as f32 / 2.0, 25.0)),
            Change::Partial
        );
        assert_eq!(app.screen, Screen::BookTools);
        assert!(fixture.app().controls.disable_page_taps);
    }

    #[test]
    fn follows_a_direct_kindle_image_link_and_returns_to_the_same_page() {
        let fixture = Fixture::new();
        let path = Path::new("Picture.azw3");
        fs::write(fixture.root.join(path), crate::kf8::tests::linked_image_fixture()).unwrap();
        let mut app = fixture.app();
        app.open_path(path).unwrap();
        let before = position(&app.active.as_ref().unwrap().reading);
        let target = app.active.as_ref().unwrap().reading.links().into_iter().find_map(|(label,target)| (label.contains("View original")).then_some(target)).unwrap();
        app.activate(Action::Jump(target)).unwrap();
        assert_eq!(app.screen,Screen::Illustration);
        assert!(app.illustration.is_some());
        app.input(Input::Tap((DONE.x + 15) as f32,(DONE.y + 15) as f32));
        assert_eq!(app.screen,Screen::Book);
        assert_eq!(position(&app.active.as_ref().unwrap().reading), before);
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
    fn dictionary_preview_expands_and_dismisses_without_moving_the_book() {
        let fixture = Fixture::new();
        let mut app = fixture.app();
        app.open_path(&fixture.book).unwrap();
        let original = position(&app.active.as_ref().unwrap().reading);
        app.definitions = ["The movement of the sea", "A flow of water"]
            .map(|text| dictionary::Definition { dictionary: "Sample".into(), word: "Tide".into(), text: text.into() })
            .into();
        app.show(Screen::Definition);
        assert!(app.definition_over_book());
        assert_eq!(app.input(Input::PageForward), Change::Partial);
        assert_eq!(app.list_page, 1);
        let [_, expand, _] = definition_actions(app.definition_layout().0);
        assert_eq!(
            app.input(Input::Tap((expand.x + 15) as f32, (expand.y + 15) as f32)),
            Change::Partial
        );
        assert!(app.definition_expanded);
        let back = primitive_ui::back_button(WIDTH as i32);
        assert_eq!(
            app.input(Input::Tap((back.x + 15) as f32, (back.y + 15) as f32)),
            Change::Partial
        );
        assert!(app.definition_over_book());
        let (_, popup_done, _) = app.definition_layout();
        assert_eq!(
            app.input(Input::Tap(
                (popup_done.x + 15) as f32,
                (popup_done.y + 15) as f32
            )),
            Change::Partial
        );
        assert_eq!(app.screen, Screen::Book);
        assert_eq!(position(&app.active.as_ref().unwrap().reading), original);
    }

    #[test]
    fn dictionary_result_moves_to_the_other_side_of_selected_text() {
        let lower_text = Rect::new(410, 1080, 55, 32);
        let upper_text = Rect::new(410, 160, 55, 32);
        let above = definition_bounds(420, Some(lower_text));
        let below = definition_bounds(420, Some(upper_text));
        assert!(above.bottom() < lower_text.y);
        assert!(below.y > upper_text.bottom());
        for popup in [above, below] {
            assert!(
                definition_actions(popup)
                    .into_iter()
                    .all(|button| button.y >= popup.y && button.bottom() < popup.bottom())
            );
        }
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
                color: Default::default(),
                created_at: None,
                style: Default::default(),
                continuation: Vec::new(),
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
                (library_view::LIBRARY_LIST_Y + ROW_HEIGHT * index as i32 + 33) as f32
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
    fn generated_outline_opens_without_publisher_navigation_and_survives_repagination() {
        let fixture = Fixture::new();
        write_outline_fixture(&fixture.book, None, None);
        let mut app = fixture.app();
        app.open_path(&fixture.book).unwrap();
        assert!(app.outline.is_none());

        let mut first_landing = None;
        for font_size in [42.0, 68.0] {
            app.activate(Action::Text(Settings {
                font_size,
                ..device_settings()
            }))
            .unwrap();
            app.activate(Action::Show(Screen::Contents)).unwrap();
            assert!(matches!(
                app.items()[0].action,
                Action::ShowGeneratedContents
            ));
            let row = |index: usize| {
                Input::Tap(
                    (LIST_X + 30) as f32,
                    (LIST_Y + index as i32 * ROW_HEIGHT + 40) as f32,
                )
            };
            assert_eq!(app.input(row(0)), Change::Partial);
            assert_eq!(app.screen, Screen::GeneratedContents);
            assert!(app.outline.as_ref().unwrap().contents.entries.is_empty());

            let deadline = |app: &Reader| app.outline.as_ref().unwrap().deadline.unwrap();
            assert_eq!(app.tick(deadline(&app)), Change::Partial);
            assert_eq!(app.items().len(), 14);
            assert_eq!(app.tick(deadline(&app)), Change::Partial);
            assert_eq!(app.items().len(), 15);
            assert_eq!(app.items()[14].title, "Chapter 2");
            assert_eq!(app.input(Input::PageForward), Change::Partial);
            assert_eq!(app.list_page, 1);
            assert_eq!(app.input(Input::PageBack), Change::Partial);
            assert_eq!(app.list_page, 0);
            assert_eq!(app.input(Input::PageForward), Change::Partial);

            let target = app.outline.as_ref().unwrap().contents.entries[12]
                .target
                .clone();
            let OutlineTarget::Heading(0, anchor) = target else {
                panic!("Expected an anchored heading without an element ID")
            };
            assert_eq!(app.input(row(12 - ROWS_PER_PAGE)), Change::Partial);
            assert_eq!(app.screen, Screen::Book);
            let reading = &app.active.as_ref().unwrap().reading;
            assert_eq!(reading.probe.page_for(&anchor), Some(reading.page));
            if let Some((original_anchor, original_page)) = first_landing {
                assert_eq!(anchor, original_anchor);
                assert_ne!(reading.page, original_page);
            }
            first_landing = Some((anchor, reading.page));
        }
    }

    #[test]
    fn generated_outline_recovers_from_broken_optional_navigation_and_can_be_canceled() {
        let fixture = Fixture::new();
        for nav in [b"<malformed".as_slice(), br#"<html><body><nav type="toc"><ol><li><a href="missing.xhtml">Broken link</a></li></ol></nav></body></html>"#] {
            write_outline_fixture(&fixture.book, Some(nav), None);
            let mut app = fixture.app();
            app.open_path(&fixture.book).unwrap();
            assert!(app.outline.is_none());
            app.activate(Action::Show(Screen::Contents)).unwrap();
            app.activate(Action::ShowGeneratedContents).unwrap();
            let pending = app.outline.as_ref().unwrap().deadline.unwrap();
            let back = primitive_ui::back_button(WIDTH as i32);
            assert_eq!(app.input(Input::Tap((back.x + 20) as f32, (back.y + 20) as f32)), Change::Partial);
            assert_eq!(app.screen, Screen::Contents);
            assert_eq!(app.tick(pending), Change::None);
            assert!(app.outline.is_none());
            app.activate(Action::ShowGeneratedContents).unwrap();
            let pending = app.outline.as_ref().unwrap().deadline.unwrap();
            assert_eq!(app.tick(pending), Change::Partial);
            assert_eq!(app.items()[0].title, "Passage 1");
            assert_eq!(app.tick(app.outline.as_ref().unwrap().deadline.unwrap()), Change::Partial);
            app.activate(Action::GeneratedEntry(14)).unwrap();
            let reading = &app.active.as_ref().unwrap().reading;
            assert_eq!((reading.chapter, reading.page), (1, 0));
        }
    }

    #[test]
    fn generated_outline_falls_back_to_ncx_for_missing_navigation_sections() {
        let fixture = Fixture::new();
        let ncx = br#"<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/">
          <navMap><navPoint><navLabel><text>NCX contents</text></navLabel><content src="first.xhtml"/></navPoint></navMap>
          <pageList><pageTarget><navLabel><text>7</text></navLabel><content src="second.xhtml"/></pageTarget></pageList>
          </ncx>"#;
        let print_nav = br#"<html><body>
          <nav type="toc"><ol><li><a href="missing.xhtml">Broken link</a></li></ol></nav>
          <nav type="page-list"><ol><li><a href="second.xhtml">v</a></li></ol></nav>
          </body></html>"#;
        write_outline_fixture(&fixture.book, Some(print_nav), Some(ncx));
        let first = Book::open(&fixture.book).unwrap();
        assert_eq!(first.toc[0].label, "NCX contents");
        assert_eq!(first.printed_pages()[0].label, "v");
        drop(first);

        let toc_nav = br#"<html><body><nav type="toc"><ol><li><a href="first.xhtml">XHTML contents</a></li></ol></nav></body></html>"#;
        write_outline_fixture(&fixture.book, Some(toc_nav), Some(ncx));
        let second = Book::open(&fixture.book).unwrap();
        assert_eq!(second.toc[0].label, "XHTML contents");
        assert_eq!(second.printed_pages()[0].label, "7");
    }

    #[test]
    fn recovered_publication_excludes_print_labels_in_lost_chapters() {
        let f = Fixture::new();
        let nav = br#"<html><body><nav type="page-list"><ol><li><a href="first.xhtml#passage2">1</a></li><li><a href="second.xhtml">2</a></li></ol></nav></body></html>"#;
        write_outline_fixture(&f.book, Some(nav), None);
        let mut bytes = fs::read(&f.book).unwrap();
        let removed = {
            let mut zip = zip::ZipArchive::new(Cursor::new(&bytes)).unwrap();
            let file = zip.by_name("EPUB/first.xhtml").unwrap();
            file.header_start() as usize..(file.data_start() + file.compressed_size()) as usize
        };
        bytes.drain(removed);
        fs::write(&f.book, bytes).unwrap();
        let book = Book::open(&f.book).unwrap();
        assert!(book.recovered_placeholder(&book.chapters[0]));
        assert_eq!(book.chapters.len(), 2);
        let [label] = book.printed_pages() else { panic!("Expected the surviving publisher boundary") };
        assert_eq!(label.label, "2");
    }

    #[test]
    fn publisher_pages_browse_exact_destinations_and_follow_the_visible_passage() {
        let fixture = Fixture::new();
        let mut nav = String::from(r#"<html><body><nav type="page-list"><ol>"#);
        nav.push_str(r#"<li><a href="first.xhtml#passage2">iii</a></li>"#);
        for n in 3..=14 {
            nav.push_str(&format!(r#"<li><a href="first.xhtml#passage{n}">{n}</a></li>"#));
        }
        nav.push_str(r#"<li><a href="second.xhtml">14</a></li></ol></nav></body></html>"#);
        write_outline_fixture(&fixture.book, Some(nav.as_bytes()), None);
        let fonts = std::env::var_os("READER_TEST_FONTS")
            .map(|value| std::env::split_paths(&value).collect::<Vec<_>>())
            .unwrap_or_else(|| vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")]);
        let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
        app.open_path(&fixture.book).unwrap();
        assert_eq!(app.visible_publisher_page(), None);
        assert_eq!(app.status_caption(), None);
        app.open_reading_edge(true, 520.);
        tap_reading_control(&mut app, "Printed pages in footer");
        assert_eq!(app.status_caption().as_deref(), Some("Printed page iii"));
        app.open_reading_edge(true, 520.);
        tap_reading_control(&mut app, "Stable page number list");
        assert_eq!(app.current_contents_row, None);
        assert_eq!(app.items().len(), 14);
        let target = app.items().remove(12);
        assert_eq!(target.title, "14");
        app.activate(target.action).unwrap();
        assert_eq!(app.active.as_ref().unwrap().reading.chapter, 0);
        let frame = |app: &Reader, dark| {
            let paths = fonts.iter().map(|p| p.to_string_lossy().into_owned()).collect::<Vec<_>>();
            let mut canvas = Canvas::new(WIDTH, HEIGHT, &paths).unwrap();
            primitive_app::frame(app, &mut canvas, dark)
        };
        let original = frame(&app, false);
        app.open_reading_edge(true, 520.);
        tap_reading_control(&mut app, "Printed pages in margin");
        let marked = frame(&app, false);
        let margin = app.active.as_ref().unwrap().reading.probe.settings.side_margin();
        let changes = original.iter().zip(&marked).enumerate().filter(|(_, (a, b))| a != b)
            .map(|(index, _)| index).collect::<Vec<_>>();
        assert!(!changes.is_empty(), "A visible source boundary should draw a label");
        assert!(changes.iter().all(|index| index % WIDTH as usize >= (WIDTH - margin) as usize
            && index / (WIDTH as usize) < PAGE_HEIGHT as usize));
        capture_test_screen(&app, &fonts, "READER_PRINTED_MARGIN_SCREENSHOT", false);
        app.activate(Action::TogglePublisherMargins).unwrap();
        assert_eq!(frame(&app, false), original);
        app.show(Screen::PublisherPages);
        assert_eq!(app.current_contents_row, Some(12));
        assert_eq!(app.list_page, 12 / ROWS_PER_PAGE);
        let later_page = app.list_page;
        app.input(Input::PageBack);
        capture_test_screen(&app, &fonts, "READER_PRINTED_LIST_SCREENSHOT", false);
        app.input(Input::PageForward);
        assert_eq!(app.list_page, later_page);
        let target = app.items().remove(13);
        app.activate(target.action).unwrap();
        assert_eq!(app.active.as_ref().unwrap().reading.chapter, 1);
        assert_eq!(app.status_caption().as_deref(), Some("Printed page 14"));
        app.activate(Action::ReturnToLocation).unwrap();
        let before = app.current_publisher_page();
        app.save().unwrap();
        let mut reopened = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
        reopened.activate(Action::Continue).unwrap();
        assert_eq!(reopened.current_publisher_page(), before);
        assert_eq!(reopened.status_caption().as_deref(), Some("Printed page 14"));
        let next = Settings { font_size: 48., ..reopened.active.as_ref().unwrap().reading.config.settings };
        reopened.activate(Action::Text(next)).unwrap();
        assert_eq!(reopened.status_caption().as_deref(), Some("Printed page 14"));
        reopened.activate(Action::TogglePublisherFooter).unwrap();
        assert_eq!(reopened.status_caption(), None);
        let mut persisted = fixture.app();
        persisted.activate(Action::Continue).unwrap();
        assert!(!persisted.publisher_footer_enabled());
    }

    fn finish_character_numbering(app: &mut Reader) {
        let limit = app.active.as_ref().unwrap().reading.probe.chapter_count() + 1;
        for _ in 0..limit {
            let Some(scan) = &app.numbering_scan else { return };
            app.step_character_pages(scan.deadline).unwrap();
        }
        assert!(app.numbering_scan.is_none());
    }

    #[test]
    fn result_context_tracks_publisher_and_character_pages_at_the_passage() {
        let fixture = Fixture::new();
        let nav = br#"<html><body><nav type="page-list"><ol>
          <li><a href="first.xhtml#passage2">iii</a></li>
          <li><a href="first.xhtml#passage5">vii</a></li>
          <li><a href="second.xhtml">xi</a></li>
          </ol></nav></body></html>"#;
        write_outline_fixture(&fixture.book, Some(nav), None);
        let mut app = fixture.app();
        app.open_path(&fixture.book).unwrap();
        app.start_document_search("Passage").unwrap();
        wait_for_search(&mut app, 14);
        assert_eq!(app.hits.len(), 14);
        // CREngine clamps passages before the first marker to its first label.
        for (row, expected) in [(0, "Page iii"), (1, "Page iii"), (3, "Page iii"), (4, "Page vii")] {
            app.preview_result(row).unwrap();
            assert_eq!(app.result_context.as_ref().unwrap().label, expected);
        }

        app.activate(Action::SetCharacterPages(Some(500))).unwrap();
        finish_character_numbering(&mut app);
        app.start_document_search("headings here").unwrap();
        wait_for_search(&mut app, 1);
        let last = app.active.as_ref().unwrap().numbering.as_ref().unwrap().count;
        assert!(last > 14);
        assert_eq!(app.hits[0].chapter, 1);
        app.preview_result(0).unwrap();
        assert_eq!(app.result_context.as_ref().unwrap().label, format!("Page {last}"));
        assert_eq!(app.active.as_ref().unwrap().reading.chapter, 0);
    }

    #[test]
    fn character_numbering_reflows_resumes_and_restores_the_publisher_choice() {
        let f = Fixture::new();
        let nav = br#"<html><body><nav type="page-list"><ol><li><a href="first.xhtml#passage2">iii</a></li></ol></nav></body></html>"#;
        write_outline_fixture(&f.book, Some(nav), None);
        let fonts = simulation_fonts();
        let mut app = Reader::open(&f.root, &f.state, fonts.clone()).unwrap();
        app.open_path(&f.book).unwrap();
        app.open_reading_edge(true, 520.);
        tap_reading_control(&mut app, "Page numbering");
        assert_eq!(app.items()[0].title, "Publisher page numbers");
        assert!(app.set_character_pages(Some(499)).is_err());
        app.activate(Action::SetCharacterPages(Some(500))).unwrap();
        assert_eq!(app.screen, Screen::PageNumbering);
        finish_character_numbering(&mut app);
        assert_eq!(app.screen, Screen::PublisherPages);
        let count = app.list_window().0;
        assert!(count > 14);
        app.activate(Action::TogglePublisherFooter).unwrap();
        app.activate(Action::TogglePublisherMargins).unwrap();
        let current = 12;
        app.show(Screen::PublisherPages);
        let action = app.publisher_page_window(current, 1).1.remove(0).action;
        app.activate(action).unwrap();
        assert!(app.status_caption().unwrap().starts_with("Stable page "));
        app.show(Screen::PublisherPages);
        assert!(app.current_contents_row.unwrap() >= current);
        capture_test_screen(&app, &fonts, "READER_CHARACTER_PAGE_LIST", false);
        let id = app.active.as_ref().unwrap().id;
        let before = app.active.as_ref().unwrap().numbering.as_ref().unwrap().locate(current);
        app.show(Screen::Book);
        let settings = Settings { font_size: 48., ..app.active.as_ref().unwrap().reading.config.settings };
        app.activate(Action::Text(settings)).unwrap();
        app.activate(Action::CharacterPage(current)).unwrap();
        app.save().unwrap();
        let mut resumed = Reader::open(&f.root, &f.state, fonts.clone()).unwrap();
        resumed.activate(Action::Continue).unwrap();
        assert!(resumed.numbering_scan.is_none());
        assert_eq!(resumed.active.as_ref().unwrap().numbering.as_ref().unwrap().locate(current), before);
        resumed.show(Screen::GoToPage);
        resumed.editor.text = (format!("label {}", current + 1)).chars().collect();
        resumed.jump_reflow_number().unwrap();
        assert!(resumed.current_publisher_page().unwrap() >= current);
        fs::write(resumed.numbering_path(id), b"{truncated").unwrap();
        let mut restored = Reader::open(&f.root, &f.state, fonts).unwrap();
        restored.activate(Action::Continue).unwrap();
        assert!(restored.numbering_scan.is_some());
        finish_character_numbering(&mut restored);
        assert_eq!(restored.active.as_ref().unwrap().numbering.as_ref().unwrap().count, count);
        restored.activate(Action::SetCharacterPages(None)).unwrap();
        assert_eq!(restored.status_caption().as_deref(), Some("Printed page iii"));
        assert!(restored.library.state(id).unwrap().character_page_stride.is_none());
        restored.activate(Action::SetCharacterPages(Some(3000))).unwrap();
        restored.activate(Action::SetCharacterPages(None)).unwrap();
        assert!(restored.numbering_scan.is_none());
    }

    #[test]
    fn page_number_defaults_only_apply_on_first_read_and_keep_publisher_labels() {
        let f = Fixture::new();
        let nav = br#"<html><body><nav type="page-list"><ol><li><a href="first.xhtml#passage2">iii</a></li></ol></nav></body></html>"#;
        write_outline_fixture(&f.book, Some(nav), None);
        let mut app = f.app();
        let mut defaults = crate::reading_controls::NumberingDefaults {
            stride: Some(750), prompt_publisher: true, ..Default::default()
        };
        app.activate(Action::SetReadingControls(ReadingControls { numbering: defaults, ..app.controls })).unwrap();
        app.open_path(&f.book).unwrap();
        assert_eq!(app.screen, Screen::PublisherOffer);
        assert!(app.numbering_scan.is_none());
        app.activate(Action::AcceptPublisherNumbers).unwrap();
        assert!(app.publisher_footer_enabled() && app.publisher_margins_enabled());
        app.show(Screen::NumberingValue(NumberField::MarginSize));
        app.editor.text = "21".chars().collect();
        assert!(app.apply_numbering_input(NumberField::MarginSize).is_err());
        app.editor.text = "20".chars().collect();
        app.apply_numbering_input(NumberField::MarginSize).unwrap();
        assert_eq!(app.screen, Screen::NumberingDefaults);
        assert_eq!(app.controls.numbering.label_size, 20);
        let other = f.root.join("Second.epub");
        fs::write(&other, include_bytes!("../tests/fixtures/field-notes-epub3.epub")).unwrap();
        app.open_path(&other).unwrap();
        assert!(app.numbering_scan.is_some());
        finish_character_numbering(&mut app);
        assert_eq!(app.active.as_ref().unwrap().numbering.as_ref().unwrap().stride, 750);
        defaults.override_publisher = true;
        defaults.label_size = 20;
        app.activate(Action::SetReadingControls(ReadingControls { numbering: defaults, ..app.controls })).unwrap();
        app.open_path(&f.book).unwrap();
        assert_eq!(app.screen, Screen::Book);
        assert!(app.numbering_scan.is_none() && app.active.as_ref().unwrap().numbering.is_none());
        assert!(app.publisher_footer_enabled());
        app.save().unwrap();
        let mut resumed = f.app();
        assert_eq!(resumed.controls.numbering, defaults);
        let third = f.root.join("Third.epub");
        let other_nav = br#"<html><body><nav type="page-list"><ol><li><a href="second.xhtml">vi</a></li></ol></nav></body></html>"#;
        write_outline_fixture(&third, Some(other_nav), None);
        resumed.open_path(&third).unwrap();
        assert_eq!(resumed.screen, Screen::Book);
        assert!(resumed.numbering_scan.is_some());
        finish_character_numbering(&mut resumed);
        assert_eq!(resumed.active.as_ref().unwrap().numbering.as_ref().unwrap().stride, 750);
    }

    #[test]
    #[ignore = "requires an original EPUB and its installed CREngine comparison"]
    fn follows_character_pages_in_an_original_document() {
        let path = PathBuf::from(std::env::var_os("READER_SYNTHETIC_BOOK").unwrap());
        let stride: u16 = std::env::var("READER_SYNTHETIC_STRIDE").unwrap().parse().unwrap();
        let expected: Vec<String> = serde_json::from_slice(&fs::read(std::env::var_os("READER_SYNTHETIC_REFERENCE").unwrap()).unwrap()).unwrap();
        let f = Fixture::new();
        let mut app = Reader::open(path.parent().unwrap(), &f.state, simulation_fonts()).unwrap();
        app.open_path(&path).unwrap();
        app.activate(Action::SetCharacterPages(Some(stride))).unwrap();
        finish_character_numbering(&mut app);
        assert_eq!(app.list_window().0, expected.len());
        let previous = app.active.as_ref().unwrap().reading.anchor();
        for row in [0, expected.len()/2, expected.len()-1] {
            let action = app.publisher_page_window(row, 1).1.remove(0).action;
            app.activate(action).unwrap();
            let active = app.active.as_ref().unwrap();
            let map = active.numbering.as_ref().unwrap();
            let (chapter, point) = map.locate(row).unwrap();
            assert_eq!(chapter, active.reading.chapter);
            if let Some(point) = point {
                let anchor = map.anchor(chapter, point, &active.reading.probe.chapter).unwrap();
                assert_eq!(active.reading.probe.page_for(&anchor), Some(active.reading.page));
            } else {
                assert_eq!(active.reading.page, 0);
            }
            assert!(app.current_publisher_page().is_some_and(|active_row| active_row >= row));
            app.activate(Action::ReturnToLocation).unwrap();
            assert_eq!(app.active.as_ref().unwrap().reading.anchor(), previous);
        }
        println!("Followed 3 stable pages and returned exactly across {} stock labels", expected.len());
    }

    #[test]
    #[ignore = "requires an original EPUB and independent page labels"]
    fn compares_publisher_pages_in_an_original_document() {
        let original = PathBuf::from(std::env::var_os("READER_PAGE_MAP_BOOK").expect("EPUB"));
        let labels: Vec<String> = serde_json::from_slice(&fs::read(std::env::var_os("READER_PAGE_MAP_LABELS").expect("labels")).unwrap()).unwrap();
        let f = Fixture::new();
        let mut app = Reader::open(original.parent().unwrap(), &f.state, simulation_fonts()).unwrap();
        app.open_path(&original).unwrap();
        app.activate(Action::TogglePublisherFooter).unwrap();
        app.open_reading_edge(true, 520.);
        tap_reading_control(&mut app, "Stable page number list");
        let actual: Vec<_> = app.items().into_iter().filter(|item| matches!(item.action, Action::Jump(_))).map(|item| item.title).collect();
        assert_eq!(actual.len(), labels.len());
        assert!(actual == labels, "Publisher labels or order differed");
        let active = app.active.as_ref().unwrap();
        let mut by_chapter: std::collections::BTreeMap<usize, Vec<&Destination>> = Default::default();
        for entry in &active.reading.probe.printed_pages {
            let dest = entry.destination.as_ref().expect("Missing print destination");
            let chapter = active.reading.probe.chapter_index(&dest.path).expect("Missing print chapter");
            by_chapter.entry(chapter).or_default().push(dest);
        }
        let mut mapped = 0;
        for (chapter, entries) in by_chapter {
            let probe = active.reading.config.chapter(chapter).unwrap();
            for destination in entries {
                if let Some(id) = &destination.fragment {
                    let anchor = probe.fragment(id).expect("Unresolved printed page ID");
                    assert!(probe.page_for(&anchor).is_some(), "No visible geometry for print boundary");
                }
                mapped += 1;
            }
        }
        assert_eq!(mapped, labels.len());
        for n in [0, labels.len()/2, labels.len()-1] {
            let target = app.items().remove(n).action;
            let Action::Jump(dest) = &target else { panic!("Missing print destination") };
            let expected = dest.clone();
            app.activate(target).unwrap();
            let reading = &app.active.as_ref().unwrap().reading;
            assert_eq!(reading.probe.chapter, expected.path);
            if let Some(id) = &expected.fragment {
                let page = reading.probe.page_for(&reading.probe.fragment(id).expect("Missing printed page anchor")).unwrap();
                assert_eq!(reading.page, page);
            }
            assert!(app.status_caption().is_some_and(|s| s.starts_with("Printed page ")));
            app.show(Screen::PublisherPages);
            let row = app.current_contents_row.expect("Selected print range");
            assert!(row >= n && row < labels.len());
        }
        println!("Matched {} publisher labels, {} source locations and three exact jumps", labels.len(), mapped);
    }

    fn write_outline_fixture(path: &Path, nav: Option<&[u8]>, ncx: Option<&[u8]>) {
        let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        let container = br#"<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container" version="1.0"><rootfiles><rootfile full-path="EPUB/book.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#;
        let nav_item = nav.map_or("", |_| r#"<item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>"#);
        let ncx_item = ncx.map_or(
            "",
            |_| r#"<item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>"#,
        );
        let spine_toc = ncx.map_or("", |_| r#" toc="ncx""#);
        let package = format!(
            r#"<package xmlns="http://www.idpf.org/2007/opf" version="3.0"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>Outline sample</dc:title></metadata><manifest><item id="first" href="first.xhtml" media-type="application/xhtml+xml"/><item id="second" href="second.xhtml" media-type="application/xhtml+xml"/>{nav_item}{ncx_item}</manifest><spine{spine_toc}><itemref idref="first"/><itemref idref="second"/></spine></package>"#
        );
        let lyric = "Beyond the shore the morning gathering stirred in the wind. ".repeat(38);
        let mut first = String::from(
            r#"<html xmlns="http://www.w3.org/1999/xhtml"><body><h1 hidden="hidden">Omit this heading</h1>"#,
        );
        for index in 1..=14 {
            use std::fmt::Write as _;
            write!(first, r#"<h1 id="passage{index}">Passage {index}</h1><p>{lyric}</p>"#).unwrap();
        }
        first.push_str("</body></html>");
        let second = br#"<html xmlns="http://www.w3.org/1999/xhtml"><body><p>There are no headings here.</p></body></html>"#;
        for (name, bytes) in [
            ("mimetype", b"application/epub+zip".as_slice()),
            ("META-INF/container.xml", container.as_slice()),
            ("EPUB/book.opf", package.as_bytes()),
            ("EPUB/first.xhtml", first.as_bytes()),
            ("EPUB/second.xhtml", second.as_slice()),
        ]
        .into_iter()
        .chain(nav.map(|xml| ("EPUB/nav.xhtml", xml)))
        .chain(ncx.map(|xml| ("EPUB/toc.ncx", xml)))
        {
            zip.start_file(name, options).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
    }

    fn wait_for_search(app: &mut Reader, until_hits: usize) {
        let until = Instant::now() + Duration::from_secs(120);
        while app.nearby.searching_all() && app.hits.len() < until_hits && Instant::now() < until {
            app.poll_nearby(Instant::now()).unwrap();
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(!app.nearby.searching_all() || app.hits.len() >= until_hits);
    }

    #[test]
    fn large_search_list_opens_its_final_result_and_returns_to_the_original_passage() {
        let fixture = Fixture::new();
        let font = crate::test_fixture("fonts/NotoSans-Regular.ttf");
        let external = std::env::var_os("READER_TEST_RESULTS_BOOK").map(PathBuf::from);
        let authored = external.is_none();
        let path = external.unwrap_or_else(|| {
            let path = fixture.root.join("Coastal observations.epub");
            fs::write(&path, include_bytes!("../tests/fixtures/long-chapter.epub")).unwrap();
            path
        });
        let query = std::env::var("READER_TEST_RESULTS_QUERY").unwrap_or_else(|_| "Observation".into());
        let mut app = Reader::open(path.parent().unwrap(), &fixture.state, vec![font.clone()]).unwrap();
        app.open_path(&path).unwrap();
        let original = position(&app.active.as_ref().unwrap().reading);
        app.find_match_case = true;
        app.start_document_search(&query).unwrap();
        wait_for_search(&mut app, usize::MAX);
        let count = app.hits.len();
        assert!(count > 100);
        if let Ok(expected) = std::env::var("READER_TEST_RESULTS_COUNT") {
            assert_eq!(count, expected.parse::<usize>().unwrap());
        }
        let last = app.hits[count - 1].clone();
        let last_page = (count - 1) / ROWS_PER_PAGE;
        let (x, y) = rectangle_center(LAST_MATCH_PAGE);
        assert_eq!(app.input(Input::Tap(x, y)), Change::Partial);
        assert_eq!(app.list_page, last_page);
        assert_eq!(app.list_window().1.len(), count - last_page * ROWS_PER_PAGE);
        if authored {
            capture_test_screen(&app, std::slice::from_ref(&font), "READER_DEEP_RESULTS", false);
        }
        let (x, y) = rectangle_center(FIRST_MATCH_PAGE);
        assert_eq!(app.input(Input::Tap(x, y)), Change::Partial);
        assert_eq!(app.list_page, 0);
        let (x, y) = rectangle_center(LAST_MATCH_PAGE);
        app.input(Input::Tap(x, y));
        let row = (count - 1) % ROWS_PER_PAGE;
        app.input(Input::Tap(
            (LIST_X + 30) as f32,
            (LIST_Y + row as i32 * ROW_HEIGHT + 20) as f32,
        ));
        assert_eq!(app.screen, Screen::Book);
        let found = app.result_selection.as_ref().unwrap();
        assert_eq!((&found.start, &found.end), (&last.anchor, &last.end));
        assert_eq!(
            app.search_caption(),
            Some(format!("Match {count} of {count}"))
        );
        app.activate(Action::ReturnToLocation).unwrap();
        assert_eq!(position(&app.active.as_ref().unwrap().reading), original);
    }

    #[test]
    fn replacing_a_partially_delivered_dense_search_discards_queued_results() {
        let fixture = Fixture::new();
        let path = fixture.root.join("Coastal observations.epub");
        fs::write(&path, include_bytes!("../tests/fixtures/long-chapter.epub")).unwrap();
        let mut app = fixture.app();
        app.open_path(&path).unwrap();
        app.start_document_search("e").unwrap();
        wait_for_search(&mut app, 1);
        assert!(app.nearby.searching_all());
        let (x, y) = rectangle_center(primitive_ui::back_button(WIDTH as i32));
        app.input(Input::Tap(x, y));
        assert_eq!(app.screen, Screen::Find);
        assert!(!app.nearby.searching_all());
        app.start_document_search("Observation 239.").unwrap();
        wait_for_search(&mut app, usize::MAX);
        assert_eq!(app.hits.len(), 1);
        let hit = &app.hits[0];
        assert_eq!(&hit.snippet[hit.matched.clone()], "Observation 239.");
    }

    #[test]
    fn ordinary_search_keyboard_selects_original_quotes_and_spaces_in_both_directions() {
        let f = Fixture::new();
        let path = f.root.join("Café.xhtml");
        fs::write(
            &path,
            "<html><body><p>L’été\u{a0}arrive. L’été arrive.</p></body></html>",
        )
        .unwrap();
        let mut app = f.app();
        app.open_path(&path).unwrap();
        app.show(Screen::Find);
        app.editor.text = "L'été arrive".chars().collect();
        app.editor.caret = app.editor.text.len();
        let settle = |app: &mut Reader| {
            let stop = Instant::now() + Duration::from_secs(10);
            while app.nearby.deadline().is_some() && Instant::now() < stop {
                app.poll_nearby(Instant::now()).unwrap();
                std::thread::sleep(Duration::from_millis(2));
            }
            assert!(app.nearby.deadline().is_none());
        };
        app.input(Input::Tap(900.0, 560.0));
        settle(&mut app);
        let first = app.result_selection.clone().unwrap();
        assert_eq!(first.text, "L’été\u{a0}arrive");
        app.activate(app.search_step(1).unwrap()).unwrap();
        settle(&mut app);
        assert_eq!(app.result_selection.as_ref().unwrap().text, "L’été arrive");
        app.activate(app.search_step(-1).unwrap()).unwrap();
        settle(&mut app);
        assert_eq!(app.result_selection.as_ref().unwrap().start, first.start);
        app.show(Screen::Find);
        app.start_document_search("L'été arrive").unwrap();
        wait_for_search(&mut app, usize::MAX);
        assert_eq!(app.hits.len(), 2);
        assert_eq!(app.hits[0].anchor, first.start);
        assert_eq!(app.hits[0].end, first.end);
    }

    #[test]
    fn regex_search_reflow_controls_history_navigation_and_literal_pdf() {
        let fixture = Fixture::new();
        let font = crate::test_fixture("fonts/NotoSans-Regular.ttf");
        let path = fixture.root.join("Harbour.xhtml");
        fs::write(
            &path,
            include_bytes!("../tests/fixtures/nearby-search.xhtml"),
        )
        .unwrap();
        let mut app = Reader::open(&fixture.root, &fixture.state, vec![font.clone()]).unwrap();
        app.open_path(&path).unwrap();
        let origin = position(&app.active.as_ref().unwrap().reading);
        let settle = |app: &mut Reader| {
            let stop = Instant::now() + Duration::from_secs(15);
            while app.nearby.deadline().is_some() && Instant::now() < stop {
                app.poll_nearby(Instant::now()).unwrap();
                std::thread::sleep(Duration::from_millis(3));
            }
            assert!(app.nearby.deadline().is_none());
        };
        let pattern = r"tide\s+keeps|caf.";
        app.show(Screen::Find);
        app.input(Input::Tap(
            (FIND_PATTERN.x + 10) as f32,
            (FIND_PATTERN.y + 10) as f32,
        ));
        assert!(app.find_regex);
        app.editor.text = pattern.chars().collect();
        app.editor.caret = app.editor.text.len();
        capture_test_screen(
            &app,
            std::slice::from_ref(&font),
            "READER_PATTERN_INPUT",
            false,
        );
        app.input(Input::Tap(900.0, 560.0));
        settle(&mut app);
        let first = app.result_selection.as_ref().unwrap().clone();
        assert_eq!(first.text, "tide keeps");
        app.activate(app.search_step(1).unwrap()).unwrap();
        settle(&mut app);
        assert_eq!(app.result_selection.as_ref().unwrap().text, "café");
        app.activate(app.search_step(-1).unwrap()).unwrap();
        settle(&mut app);
        assert_eq!(app.result_selection.as_ref().unwrap().start, first.start);
        app.activate(Action::ReturnToLocation).unwrap();
        assert_eq!(position(&app.active.as_ref().unwrap().reading), origin);
        app.show(Screen::Find);
        assert!(app.start_document_search("[").is_err());
        assert_eq!(app.screen, Screen::Find);
        app.start_document_search(pattern).unwrap();
        settle(&mut app);
        assert_eq!(app.hits.len(), 3);
        assert!(app.lookup_history.searches[0].regular_expression);
        capture_test_screen(
            &app,
            std::slice::from_ref(&font),
            "READER_PATTERN_RESULTS",
            false,
        );
        let saved = app.lookup_history.searches[0].clone();
        app.activate(Action::RepeatSearch(saved.clone())).unwrap();
        app.show(Screen::Find);
        assert!(!app.nearby.searching_all());
        assert!(app.find_regex);
        let pdf = fixture.root.join("Signal.pdf");
        fs::write(&pdf, crate::fixed::tests::document()).unwrap();
        app.open_path(&pdf).unwrap();
        assert!(app.activate(Action::RepeatSearch(saved)).is_err());
        app.start_document_search("H.rbor").unwrap();
        assert!(app.pdf_search.is_some());
        assert!(!app.nearby.searching_all());
    }

    #[test]
    fn directional_find_uses_the_current_passage_and_returns_to_the_original_position() {
        use crate::search::SearchDirection;
        let fixture = Fixture::new();
        let font = crate::test_fixture("fonts/NotoSans-Regular.ttf");
        let path = fixture.root.join("Harbour.xhtml");
        fs::write(
            &path,
            include_bytes!("../tests/fixtures/nearby-search.xhtml"),
        )
        .unwrap();
        let mut app = Reader::open(&fixture.root, &fixture.state, vec![font.clone()]).unwrap();
        app.open_path(&path).unwrap();
        let original = position(&app.active.as_ref().unwrap().reading);
        app.show(Screen::Find);
        app.editor.text = "the tide keeps its own time".chars().collect();
        app.editor.caret = app.editor.text.len();
        capture_test_screen(
            &app,
            std::slice::from_ref(&font),
            "READER_NEARBY_FIND_SCREENSHOT",
            false,
        );
        let await_result = |app: &mut Reader| {
            let deadline = Instant::now() + Duration::from_secs(5);
            while app.nearby.deadline().is_some() && Instant::now() < deadline {
                app.poll_nearby(Instant::now()).unwrap();
                std::thread::sleep(Duration::from_millis(2));
            }
            assert!(app.nearby.deadline().is_none());
        };
        app.input(Input::Tap(900.0, 560.0));
        await_result(&mut app);
        assert_eq!(app.screen, Screen::Book);
        assert_eq!(
            app.result_selection.as_ref().unwrap().text.to_lowercase(),
            "the tide keeps its own time"
        );
        capture_test_screen(
            &app,
            std::slice::from_ref(&font),
            "READER_NEARBY_MATCH_SCREENSHOT",
            false,
        );
        let first = app.result_selection.as_ref().unwrap().start.clone();
        assert_eq!(app.back.len(), 1);
        app.activate(app.search_step(1).unwrap()).unwrap();
        await_result(&mut app);
        assert_ne!(app.result_selection.as_ref().unwrap().start, first);
        assert_eq!(app.back.len(), 1);
        app.activate(app.search_step(-1).unwrap()).unwrap();
        await_result(&mut app);
        assert_eq!(app.result_selection.as_ref().unwrap().start, first);
        app.begin_nearby(SearchDirection::Previous, true).unwrap();
        await_result(&mut app);
        assert_eq!(app.notice.as_deref(), Some("No earlier match"));
        assert!(app.search_step(-1).is_none());
        app.activate(Action::ReturnToLocation).unwrap();
        let restored = position(&app.active.as_ref().unwrap().reading);
        assert_eq!(original.anchor, restored.anchor);

        app.show(Screen::Find);
        app.editor.text = "absent-from-this-reading".chars().collect();
        app.input(Input::Tap(900.0, 560.0));
        assert!(app.nearby.deadline().is_some());
        app.input(Input::Tap(270.0, 560.0));
        assert!(app.nearby.deadline().is_none());
        assert_eq!(app.screen, Screen::Find);
    }

    #[test]
    fn finds_repeated_text_then_returns_from_a_result_to_the_original_location() {
        let fixture = Fixture::new();
        let mut app = fixture.app();
        app.open_path(&fixture.book).unwrap();
        let original = position(&app.active.as_ref().unwrap().reading);
        app.show(Screen::Matches);
        app.start_document_search("THE TIDE KEEPS ITS OWN TIME")
            .unwrap();
        wait_for_search(&mut app, usize::MAX);
        assert_eq!(app.hits.len(), 2);
        assert_ne!(app.hits[0].anchor, app.hits[1].anchor);
        let target = app.hits[1].anchor.clone();
        app.activate(Action::SearchHit(0)).unwrap();
        assert_eq!(app.search_result, Some(0));
        assert!(app.search_step(-1).is_none());
        assert_eq!(
            app.input(Input::Tap(
                (MATCH_NEXT.x + 20) as f32,
                (MATCH_NEXT.y + 20) as f32,
            )),
            Change::Partial
        );
        assert_eq!(app.search_result, Some(1));
        assert!(app.search_step(1).is_none());
        let reading = &app.active.as_ref().unwrap().reading;
        assert_eq!(reading.probe.page_for(&target), Some(reading.page));
        app.activate(Action::ReturnToLocation).unwrap();
        assert_eq!(position(&app.active.as_ref().unwrap().reading), original);
        assert_eq!(app.search_result, None);
    }

    #[test]
    fn keeps_finding_matches_after_opening_an_early_result() {
        let fixture = Fixture::new();
        let many = fixture.root.join("Harbor Notebook.epub");
        fs::write(
            &many,
            include_bytes!("../tests/fixtures/many-chapters.epub"),
        )
        .unwrap();
        let mut app = fixture.app();
        app.open_path(&many).unwrap();
        let original = position(&app.active.as_ref().unwrap().reading);
        app.show(Screen::Matches);
        app.start_document_search("harbor notebook records")
            .unwrap();
        wait_for_search(&mut app, 4);
        assert_eq!(app.hits.len(), 4);
        app.activate(Action::SearchHit(3)).unwrap();
        assert!(app.search_step(1).is_none());
        wait_for_search(&mut app, 8);
        assert_eq!(app.hits.len(), 8);
        assert_eq!(
            app.input(Input::Tap(
                (MATCH_NEXT.x + 20) as f32,
                (MATCH_NEXT.y + 20) as f32
            )),
            Change::Partial
        );
        assert_eq!(app.active.as_ref().unwrap().reading.chapter, 4);
        app.activate(Action::ReturnToLocation).unwrap();
        assert_eq!(position(&app.active.as_ref().unwrap().reading), original);
    }

    #[test]
    fn tapping_an_illustrated_reference_opens_its_chapter_and_returns_to_the_sentence() {
        let fixture = Fixture::new();
        fs::copy(crate::test_fixture("signal-guide.epub"), &fixture.book).unwrap();
        let mut app = fixture.app();
        app.open_path(&fixture.book).unwrap();
        let reading = &app.active.as_ref().unwrap().reading;
        let start = position(reading);
        let (x, y) = (0..PAGE_HEIGHT).step_by(5).find_map(|y| {
            (0..WIDTH).step_by(5).find_map(|x| {
                reading.illustration_at(x as f32, y as f32)
                    .is_some_and(|p| p.ends_with("beacon.svg"))
                    .then_some((x as f32, y as f32))
            })
        }).unwrap();
        assert_eq!(app.input(Input::Tap(x, y)), Change::Partial);
        let reading = &app.active.as_ref().unwrap().reading;
        assert_eq!(reading.chapter, 1);
        assert_eq!(reading.anchor(), reading.probe.fragment("beacon"));
        app.activate(Action::ReturnToLocation).unwrap();
        assert_eq!(position(&app.active.as_ref().unwrap().reading), start);
        app.save().unwrap();
        let mut reopened = fixture.app();
        reopened.open_path(&fixture.book).unwrap();
        assert_eq!(position(&reopened.active.as_ref().unwrap().reading), start);
    }

    fn source_link_point(app: &Reader, label: &str) -> (f32, f32) {
        let reading = &app.active.as_ref().unwrap().reading;
        let start = reading.probe.matches(label).into_iter().next().unwrap();
        let mut end = start.clone();
        end.offset += label.len();
        let selection = reading.probe.select_range(&start, &end).unwrap();
        let rect = reading.highlight_rects(&selection)[0];
        (
            (rect.x + rect.width / 2) as f32,
            (rect.y + rect.height / 2) as f32,
        )
    }

    fn toggle_footnote_menu(app: &mut Reader) {
        app.open_reading_edge(true, 100.0);
        assert_eq!(
            tap_reading_control(app, "Footnote previews"),
            Change::Partial
        );
        app.show(Screen::Book);
    }

    #[test]
    fn footnote_previews_recognize_a_publisher_note_and_an_unmarked_epub2_number() {
        for file in ["field-notes-epub3.epub", "field-notes-epub2.epub"] {
            let fixture = Fixture::new();
            fs::copy(crate::test_fixture(file), &fixture.book).unwrap();
            let mut app = fixture.app();
            app.open_path(&fixture.book).unwrap();
            toggle_footnote_menu(&mut app);
            let original = position(&app.active.as_ref().unwrap().reading);
            let (x, y) = source_link_point(&app, "[1]");
            app.input(Input::Tap(x, y));
            assert_eq!(app.screen, Screen::Footnote, "{file}");
            let note = app.footnote.as_ref().unwrap();
            assert_eq!(note.reference.as_ref().unwrap().text, "[1]");
            let (wx, wy) = note.test_word("first");
            app.input(Input::LongTap(wx, wy));
            assert_eq!(app.definition_word, "first");
            assert_eq!(position(&app.active.as_ref().unwrap().reading), original);
            assert!(app.back.is_empty());
        }
    }

    #[test]
    fn cross_chapter_footnote_previews_lookup_return_and_full_page_fallback() {
        let fixture = Fixture::new();
        fs::copy(crate::test_fixture("night-landing.epub"), &fixture.book).unwrap();
        let mut fonts = simulation_fonts();
        fonts.push(crate::test_fixture("fonts/NotoSans-Regular.ttf"));
        let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
        app.open_path(&fixture.book).unwrap();
        let original = position(&app.active.as_ref().unwrap().reading);
        let (x, y) = source_link_point(&app, "[1]");
        assert!(!app.controls.footnote_popups);
        assert_eq!(app.input(Input::Tap(x, y)), Change::Partial);
        assert_eq!(app.active.as_ref().unwrap().reading.chapter, 1);
        app.activate(Action::ReturnToLocation).unwrap();
        assert_eq!(position(&app.active.as_ref().unwrap().reading), original);

        toggle_footnote_menu(&mut app);
        let history = app.back.len();
        app.input(Input::Tap(x, y));
        assert_eq!(app.screen, Screen::Footnote);
        assert_eq!(position(&app.active.as_ref().unwrap().reading), original);
        assert_eq!(app.back.len(), history);
        let note = app.footnote.as_ref().unwrap();
        assert_eq!(note.reference.as_ref().unwrap().text, "[1]");
        let panel = note.panel();
        assert!(panel.y >= HEIGHT as i32 * 2 / 3);
        assert!(panel.bottom() <= HEIGHT as i32);
        let (wx, wy) = note.test_word("shore");
        capture_test_screen(&app, &fonts, "READER_FOOTNOTE_SCREENSHOT", false);
        let font_paths: Vec<_> = fonts.iter().map(|p| p.display().to_string()).collect();
        let mut canvas = Canvas::new(WIDTH, HEIGHT, &font_paths).unwrap();
        app.system_theme = None;
        let paper = ((panel.y + 80) * WIDTH as i32 + panel.x + 18) as usize;
        assert!(primitive_app::frame(&app, &mut canvas, false)[paper] > 230);
        app.controls.night_pages = true;
        assert!(primitive_app::frame(&app, &mut canvas, true)[paper] < 25);
        app.controls.night_pages = false;
        app.input(Input::LongTap(wx, wy));
        assert_eq!(app.screen, Screen::Definition);
        assert_eq!(app.definition_word, "shore");
        let wiki = dictionary_panel::action_row(app.definition_layout().0)[2];
        let (wx, wy) = rectangle_center(wiki);
        app.input(Input::Tap(wx, wy));
        assert_eq!(app.screen, Screen::OnlineLookup);
        assert!(app.footnote.is_some());
        let (cx, cy) = rectangle_center(online_panel::CLOSE);
        app.input(Input::Tap(cx, cy));
        assert_eq!(app.screen, Screen::Definition);
        let close = app.definition_layout().1;
        app.input(Input::Tap((close.x + 10) as f32, (close.y + 10) as f32));
        assert_eq!(app.screen, Screen::Footnote);
        let (lx, ly) = app.footnote.as_ref().unwrap().test_word("stands");
        app.input(Input::Tap(lx, ly));
        assert_eq!(
            app.footnote.as_ref().unwrap().test_selected(),
            Some("shore lamp stands")
        );
        assert!(app.selection.is_none());
        let (x2, y2) = source_link_point(&app, "[2]");
        app.input(Input::Tap(x2, y2));
        let note = app.footnote.as_ref().unwrap();
        let (_, pages) = note.test_page();
        assert!(pages > 1);
        app.input(Input::PageForward);
        assert_eq!(app.footnote.as_ref().unwrap().test_page().0, 1);
        app.input(Input::PageBack);
        assert_eq!(app.footnote.as_ref().unwrap().test_page().0, 0);
        app.swipe(
            Input::Swipe {
                dx: 0.0,
                dy: 140.0,
                start_y: 40.0,
            },
            800.0,
        );
        assert_eq!(app.screen, Screen::Book);
        app.input(Input::Tap(x, y));
        app.input(Input::Tap(10.0, 12.0));
        assert_eq!(app.screen, Screen::Book);
        assert_eq!(position(&app.active.as_ref().unwrap().reading), original);
        assert_eq!(app.back.len(), history);
        app.input(Input::Tap(x, y));
        let swipe = Input::Swipe {
            dx: -250.0,
            dy: 0.0,
            start_y: (HEIGHT - 55) as f32,
        };
        app.swipe(swipe, 400.0);
        assert_eq!(app.screen, Screen::Book);
        let reading = &app.active.as_ref().unwrap().reading;
        assert_eq!(reading.chapter, 1);
        assert_eq!(
            Some(reading.page),
            reading
                .probe
                .page_for(&reading.probe.fragment("signal").unwrap())
        );
        app.activate(Action::ReturnToLocation).unwrap();
        assert_eq!(position(&app.active.as_ref().unwrap().reading), original);
        app.input(Input::Tap(x, y));
        let open = app.footnote.as_ref().unwrap().follow_button();
        app.input(Input::Tap((open.x + 12) as f32, (open.y + 12) as f32));
        assert_eq!(app.active.as_ref().unwrap().reading.chapter, 1);
        app.activate(Action::ReturnToLocation).unwrap();
        assert_eq!(position(&app.active.as_ref().unwrap().reading), original);
        app.input(Input::Tap(x, y));
        let (bx, by) = app.footnote.as_ref().unwrap().test_word("Return");
        app.input(Input::Tap(bx, by));
        assert_eq!(app.screen, Screen::Book);
        let reading = &app.active.as_ref().unwrap().reading;
        assert_eq!(reading.chapter, 0);
        assert_eq!(position(reading).anchor, reading.probe.fragment("reference"));
        let (ox, oy) = source_link_point(&app, "harbor appendix");
        app.input(Input::Tap(ox, oy));
        assert_eq!(app.screen, Screen::Book);
        assert_eq!(app.active.as_ref().unwrap().reading.chapter, 1);
        app.save().unwrap();
        let reopened = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
        assert!(reopened.controls.footnote_popups);
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

    fn write_cbz(comic: &Path, pages: &[&str]) {
        write_cbz_image(
            comic,
            pages,
            image::GrayImage::from_pixel(8, 12, image::Luma([32])),
        );
    }

    fn write_cbz_image(comic: &Path, pages: &[&str], image: image::GrayImage) {
        let mut png = Cursor::new(Vec::new());
        image::DynamicImage::ImageLuma8(image)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        let mut zip = zip::ZipWriter::new(fs::File::create(comic).unwrap());
        for &name in pages {
            zip.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(png.get_ref()).unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn jpeg2000_images_open_from_the_picker_and_the_comic_resumes_its_second_page() {
        let fixture = Fixture::new();
        let files = [
            ("harbor-beacon.jp2", "Beacon.JP2"),
            ("signal-flags.j2k", "Flags.J2K"),
            ("harbor-signals.cbz", "Signals.cbz"),
        ];
        for (source, name) in files {
            fs::copy(crate::test_fixture(source), fixture.root.join(name)).unwrap();
        }
        let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
        let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
        for name in ["Beacon.JP2", "Flags.J2K", "Signals.cbz"] {
            let items = app.items();
            let idx = items.iter().position(|item| item.title == name).unwrap();
            assert!(matches!(items[idx].action, Action::Open(_)));
            assert_eq!(
                app.input(Input::Tap(
                    150.0,
                    (library_view::LIBRARY_LIST_Y + ROW_HEIGHT * idx as i32 + 30) as f32
                )),
                Change::Partial
            );
            assert_eq!(app.screen, Screen::Fixed);
            let active = &app.fixed.as_ref().unwrap().reading;
            if name == "Signals.cbz" {
                assert_eq!(active.count(), 2);
                app.input(Input::PageForward);
                if let Some(due) = app.fixed.as_ref().unwrap().reading.deadline() {
                    app.tick(due);
                }
                assert_eq!(app.fixed.as_ref().unwrap().reading.page(), 1);
                app.save().unwrap();
            } else {
                assert_eq!(active.count(), 1);
                if name == "Beacon.JP2" {
                    capture_test_screen(&app, &fonts, "READER_JPEG2000_SCREENSHOT", false);
                }
            }
            app.activate(Action::Show(Screen::Library)).unwrap();
        }
        let mut reopen = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
        reopen.open_path(&fixture.root.join("Signals.cbz")).unwrap();
        assert_eq!(reopen.fixed.as_ref().unwrap().reading.page(), 1);
    }

    #[test]
    fn opens_a_comic_panel_without_changing_the_underlying_page_or_fit() {
        let fixture = Fixture::new();
        let comic = fixture.root.join("Panels.cbz");
        let mut image = image::GrayImage::from_pixel(300, 400, image::Luma([255]));
        for (left, top, shade) in [
            (12, 12, 197),
            (158, 12, 131),
            (12, 208, 95),
            (158, 208, 220),
        ] {
            for y in top..top + 180 {
                for x in left..left + 130 {
                    let border = x < left + 4 || x >= left + 126 || y < top + 4 || y >= top + 176;
                    image.put_pixel(x, y, image::Luma([if border { 24 } else { shade }]));
                }
            }
        }
        write_cbz_image(&comic, &["page1.png", "page2.png"], image);
        let mut app = fixture.app();
        app.open_path(&comic).unwrap();
        let fixed = &mut app.fixed.as_mut().unwrap().reading;
        fixed.jump(1).unwrap();
        let view = fixed.preferences();
        assert_eq!(app.input(Input::LongTap(300.0, 300.0)), Change::Partial);
        assert_eq!(app.screen, Screen::Illustration);
        assert!(app.illustration.is_some());
        assert_eq!(
            app.input(Input::Tap((DONE.x + 15) as f32, (DONE.y + 15) as f32)),
            Change::Partial
        );
        assert_eq!(app.screen, Screen::Fixed);
        let fixed = &app.fixed.as_ref().unwrap().reading;
        assert_eq!(fixed.page(), 1);
        assert_eq!(fixed.preferences(), view);
    }

    #[test]
    fn reads_a_fixed_page_comic_and_preserves_progress_after_reopening() {
        let fixture = Fixture::new();
        let comic = fixture.root.join("Field Comics.cbz");
        write_cbz(&comic, &["page2.png", "page1.png"]);

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
    fn offers_the_next_comic_only_from_the_last_visible_page() {
        let fixture = Fixture::new();
        let first = fixture.root.join("Volume 2.cbz");
        let next = fixture.root.join("Volume 10.cbz");
        write_cbz(&first, &["page1.png", "page2.png"]);
        write_cbz(&next, &["page3.png"]);
        let mut app = fixture.app();
        app.open_path(&first).unwrap();
        assert!(app.next_comic().is_none());
        // A hidden rear page is outside the regular reading flow, so the next
        // volume remains available at the last regular page.
        let mut flows = crate::library::hidden_flows::HiddenFlows::default();
        flows.enabled = true;
        flows.toggle(ReadingPosition { chapter: 0, page: 1, anchor: None }, None).unwrap();
        app.fixed.as_mut().unwrap().reading.set_hidden_flows(&flows).unwrap();
        assert_eq!(app.next_comic().unwrap().path, Path::new("Volume 10.cbz"));
        flows.enabled = false;
        app.fixed.as_mut().unwrap().reading.set_hidden_flows(&flows).unwrap();
        assert!(app.next_comic().is_none());
        app.activate(Action::FixedPage(1)).unwrap();
        assert_eq!(app.next_comic().unwrap().path, Path::new("Volume 10.cbz"));
        assert_eq!(
            app.input(Input::Tap(
                (NEXT_COMIC.x + 15) as f32,
                (NEXT_COMIC.y + 15) as f32
            )),
            Change::Partial
        );
        assert_eq!(
            app.fixed.as_ref().unwrap().entry.path,
            Path::new("Volume 10.cbz")
        );
        assert!(app.next_comic().is_none());
        assert_eq!(app.library.recently_read().count(), 2);
    }

    fn tap_reading_control(app: &mut Reader, label: &str) -> Change {
        let rect = app.reading_button(label);
        app.input(Input::Tap(
            (rect.x + rect.w / 2) as f32,
            (rect.y + rect.h / 2) as f32,
        ))
    }

    #[test]
    fn reopens_the_last_menu_of_each_document_after_restart() {
        let fixture = Fixture::new();
        let second = fixture.root.join("Second.epub");
        fs::write(&second, include_bytes!("../tests/fixtures/long-chapter.epub")).unwrap();
        let mut app = fixture.app();
        app.open_path(&fixture.book).unwrap();
        app.input(Input::Tap(630.0, 32.0));
        tap_reading_control(&mut app, "Search");
        tap_reading_control(&mut app, "Text");
        tap_reading_control(&mut app, "Layout");
        app.open_path(&second).unwrap();
        assert_eq!(app.last_top_group, TopGroup::Navigation);
        assert_eq!(app.last_text_panel, BarPanel::Text);
        app.save().unwrap();
        let mut restarted = fixture.app();
        restarted.open_path(&fixture.book).unwrap();
        restarted.open_reading_edge(true, 630.0);
        assert_eq!(restarted.top_group, Some(TopGroup::Search));
        restarted.open_reading_edge(false, 630.0);
        assert_eq!(restarted.reading_panel, Some(BarPanel::TextLayout));
    }

    #[test]
    fn reading_gestures_assign_through_settings_apply_and_restore_defaults() {
        let fixture = Fixture::new();
        let mut app = fixture.app();
        app.open_path(&fixture.book).unwrap();
        let row = |slot: i32| Input::Tap(220.0, (LIST_Y + slot * ROW_HEIGHT + 48) as f32);
        app.input(Input::Tap(630.0, 28.0));
        tap_reading_control(&mut app, "Settings");
        tap_reading_control(&mut app, "Reading gestures");
        assert_eq!(app.screen, Screen::Gestures);
        app.input(row(1));
        assert_eq!(app.screen, Screen::GestureBinding(Gesture::TapTopRight));
        app.input(row(4));
        assert_eq!(app.screen, Screen::Gestures);

        let mut resumed = fixture.app();
        resumed.open_path(&fixture.book).unwrap();
        resumed.input(Input::Tap(WIDTH as f32 - 20.0, 20.0));
        assert_eq!(resumed.screen, Screen::Contents);
        resumed.show(Screen::Book);
        assert!(!resumed.page_is_bookmarked());
        resumed.touch_locked = true;
        assert_eq!(
            resumed.input(Input::Tap(WIDTH as f32 - 20.0, 20.0)),
            Change::None
        );
        assert_eq!(resumed.screen, Screen::Book);
        resumed.touch_locked = false;
        resumed
            .activate(Action::BindGesture(Gesture::TapTopRight, Binding::Default))
            .unwrap();
        resumed.show(Screen::Book);
        resumed.input(Input::Tap(WIDTH as f32 - 20.0, 20.0));
        assert!(resumed.page_is_bookmarked());
        assert_eq!(resumed.screen, Screen::Book);
    }

    #[test]
    fn assigned_swipes_respect_overlays_footer_and_page_selection() {
        let fixture = Fixture::new();
        let mut app = fixture.app();
        app.open_path(&fixture.book).unwrap();
        app.gestures.set(Gesture::RightUp, Binding::Skim).unwrap();
        let swipe = Input::Swipe {
            dx: 1.0,
            dy: -160.0,
            start_y: 865.0,
        };
        app.swipe(swipe, WIDTH as f32 - 30.0);
        assert_eq!(app.screen, Screen::PageOverview);
        app.show(Screen::Book);
        let diagonal = Input::Swipe {
            dx: 130.0,
            dy: -116.0,
            start_y: 810.0,
        };
        assert_eq!(app.swipe(diagonal, 630.0), Change::Full);
        app.gestures
            .set(Gesture::ShortDiagonal, Binding::Ignore)
            .unwrap();
        assert_eq!(app.swipe(diagonal, 630.0), Change::None);
        app.gestures
            .set(Gesture::TapBottomRight, Binding::Bookmarks)
            .unwrap();
        assert_eq!(
            app.input(Input::Tap((DONE.x + 25) as f32, (DONE.y + 20) as f32)),
            Change::Exit
        );
    }

    #[test]
    fn edge_swipes_route_left_middle_right_and_remember_bottom_configuration() {
        let fixture = Fixture::new();
        let mut app = fixture.app();
        app.open_path(&fixture.book).unwrap();
        let original = app.current_position();
        let downward = Input::Swipe {
            dx: 5.0,
            dy: 260.0,
            start_y: 25.0,
        };
        assert_eq!(app.swipe(downward, 30.0), Change::Partial);
        assert_eq!(app.top_group, Some(TopGroup::Navigation));
        assert!(app.reading_top && !app.reading_bottom);
        tap_reading_control(&mut app, "Search");
        assert_eq!(app.top_group, Some(TopGroup::Search));
        app.swipe(downward, WIDTH as f32 / 2.0);
        assert_eq!(app.top_group, Some(TopGroup::Search));
        app.swipe(downward, WIDTH as f32 - 30.0);
        assert_eq!(app.top_group, Some(TopGroup::Library));
        let upward = Input::Swipe {
            dx: 5.0,
            dy: -270.0,
            start_y: HEIGHT as f32 - 26.0,
        };
        app.swipe(upward, WIDTH as f32 / 2.0);
        assert!(!app.reading_top && app.reading_bottom);
        assert_eq!(app.reading_panel, Some(BarPanel::Text));
        tap_reading_control(&mut app, "Layout");
        assert_eq!(app.last_text_panel, BarPanel::TextLayout);
        app.swipe(downward, 35.0);
        app.swipe(upward, 35.0);
        assert_eq!(app.reading_panel, Some(BarPanel::TextLayout));
        assert_eq!(app.current_position(), original);
    }

    #[test]
    fn lookup_history_and_vocabulary_survive_reopening() {
        let fixture = Fixture::new();
        let mut app = fixture.app();
        app.open_path(&fixture.book).unwrap();
        app.start_document_search("shore").unwrap();
        app.look_up("tide").unwrap();
        let (popup, _, _) = app.definition_layout();
        let button = dictionary_panel::definition_vocabulary(popup);
        assert_eq!(
            app.input(Input::Tap((button.x + 10) as f32, (button.y + 10) as f32)),
            Change::Partial
        );
        let mut reopened = fixture.app();
        assert_eq!(reopened.lookup_history.searches[0].text, "shore");
        assert_eq!(reopened.lookup_history.dictionary, ["tide"]);
        assert_eq!(reopened.lookup_history.vocabulary, ["tide"]);
        reopened.open_path(&fixture.book).unwrap();
        reopened.show(Screen::Vocabulary);
        assert!(reopened.items().iter().any(|item| item.title == "tide"));
    }

    #[test]
    fn previous_and_next_jumps_keep_bidirectional_history() {
        let fixture = Fixture::new();
        let pdf = fixture.root.join("History.pdf");
        fs::write(&pdf, crate::fixed::tests::document()).unwrap();
        let mut app = fixture.app();
        app.open_path(&pdf).unwrap();
        assert_eq!(app.current_position().unwrap().page, 0);
        app.activate(Action::FixedPage(1)).unwrap();
        app.activate(Action::ReturnToLocation).unwrap();
        assert_eq!(app.current_position().unwrap().page, 0);
        app.activate(Action::NextLocation).unwrap();
        assert_eq!(app.current_position().unwrap().page, 1);
        assert!(app.forward.is_empty());
        app.activate(Action::ToggleBookmark).unwrap();
        assert!(app.page_is_bookmarked());
        app.activate(Action::ToggleBookmark).unwrap();
        assert!(!app.page_is_bookmarked());
    }

    fn simulation_fonts() -> Vec<PathBuf> {
        std::env::var_os("READER_TEST_FONTS")
            .map(|paths| std::env::split_paths(&paths).collect())
            .unwrap_or_else(|| vec![crate::test_fixture("fonts/Silkscreen-Regular.ttf")])
    }

    pub(super) fn capture_test_screen(app: &Reader, fonts: &[PathBuf], output_variable: &str, dark: bool) {
        if let Some(path) = std::env::var_os(output_variable) {
            let font_paths = fonts.iter().map(|p| p.display().to_string()).collect::<Vec<_>>();
            let mut canvas = Canvas::new(WIDTH, HEIGHT, &font_paths).unwrap();
            let pixels = primitive_app::frame(app, &mut canvas, dark);
            primitive_app::write_png(Path::new(&path), WIDTH, HEIGHT, &pixels).unwrap();
        }
    }

    #[test]
    fn image_controls_follow_night_mode_and_resume_with_the_book() {
        let fixture = Fixture::new();
        let fonts = simulation_fonts();
        let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
        app.system_theme = None;
        app.open_path(&fixture.book).unwrap();
        app.open_reading_edge(false, 500.0);
        tap_reading_control(&mut app, "Rendering");
        assert_eq!(tap_reading_control(&mut app, "Fast"), Change::Partial);
        assert!(!app.active.as_ref().unwrap().reading.config.settings.smooth_images);
        let settings = ReadingControls { night_pages: true, ..app.controls };
        app.activate(Action::SetReadingControls(settings)).unwrap();
        assert_eq!(app.applied_change(), Change::Full);
        let probe = &app.active.as_ref().unwrap().reading.probe;
        let crate::Renderer::Parley(layout) = &probe.renderer else { unreachable!() };
        let image_page = (0..probe.page_count()).find(|&page| {
            layout.visible_illustrations(probe.page_range(page), probe.settings).next().is_some()
        }).unwrap();
        app.follow_reflow_page(0, image_page).unwrap();
        capture_test_screen(&app, &fonts, "READER_NIGHT_IMAGE_SCREENSHOT", true);
        app.open_reading_edge(false, 500.0);
        capture_test_screen(&app, &fonts, "READER_IMAGE_CONTROLS_SCREENSHOT", true);
        assert_eq!(tap_reading_control(&mut app, "Inverted"), Change::Partial);
        assert!(app.active.as_ref().unwrap().reading.config.settings.invert_night_images);
        app.save().unwrap();
        let mut reopened = fixture.app();
        reopened.activate(Action::Continue).unwrap();
        let prefs = reopened.active.as_ref().unwrap().reading.config.settings;
        assert!(!prefs.smooth_images && prefs.invert_night_images);
    }

    pub(super) fn tap_contents_item(app: &mut Reader, label: &str) {
        let index = app.items().iter().position(|item| item.title == label).unwrap_or_else(|| panic!("No row {label} in {:?}", app.screen));
        let page = index / ROWS_PER_PAGE;
        while app.list_page < page { app.input(Input::PageForward); }
        while app.list_page > page { app.input(Input::PageBack); }
        let y = LIST_Y + (index % ROWS_PER_PAGE) as i32 * ROW_HEIGHT + ROW_HEIGHT / 2;
        assert_eq!(app.input(Input::Tap(160.0, y as f32)), Change::Partial);
    }

    #[test]
    fn legacy_mobi_contents_opens_from_the_toolbar_and_returns_to_the_same_passage() {
        let fixture = Fixture::new();
        let title = "Coastal field notes.mobi";
        fs::write(
            fixture.root.join(title),
            include_bytes!("../tests/fixtures/mobi/coastal-contents.mobi"),
        )
        .unwrap();
        let font = crate::test_fixture("fonts/NotoSans-Regular.ttf");
        let mut app = Reader::open(&fixture.root, &fixture.state, vec![font.clone()]).unwrap();
        let index = app.items().iter().position(|row| row.title == title).unwrap();
        let y = library_view::LIBRARY_LIST_Y + index as i32 * ROW_HEIGHT + 32;
        assert_eq!(app.input(Input::Tap(100.0, y as f32)), Change::Partial);
        assert_eq!(app.screen, Screen::Book);
        let initial = app.current_position().unwrap();
        let mut canvas = Canvas::new(WIDTH, HEIGHT, &[font.display().to_string()]).unwrap();
        let opening = primitive_app::frame(&app, &mut canvas, false);

        for title in ["Summit notes", "  Survey notes"] {
            assert_eq!(app.input(Input::Tap(630.0, 25.0)), Change::Partial);
            if app.top_group != Some(TopGroup::Navigation) {
                tap_reading_control(&mut app, "Navigation");
            }
            assert_eq!(tap_reading_control(&mut app, "Contents"), Change::Partial);
            assert_eq!(app.screen, Screen::Contents);
            assert!(app.items().iter().any(|i| i.title == "  Elevation profile"));
            if title == "Summit notes" {
                capture_test_screen(
                    &app,
                    std::slice::from_ref(&font),
                    "READER_MOBI_CONTENTS_SCREENSHOT",
                    false,
                );
            }
            tap_contents_item(&mut app, title);
            assert_eq!(app.screen, Screen::Book);
            if title == "Summit notes" {
                assert!(app.current_position().unwrap().page > initial.page);
            }
            app.activate(Action::ReturnToLocation).unwrap();
            assert_eq!(app.current_position(), Some(initial.clone()));
            assert!(opening == primitive_app::frame(&app, &mut canvas, false));
        }
    }

    #[test]
    fn kf8_publisher_contents_opens_child_entries_and_restores_the_original_frame() {
        let fixture = Fixture::new();
        let path = fixture.root.join("Shoreline.azw3");
        fs::write(&path, crate::kf8::tests::publisher_contents_fixture()).unwrap();
        let font = crate::test_fixture("fonts/NotoSans-Regular.ttf");
        let mut app = Reader::open(&fixture.root, &fixture.state, vec![font.clone()]).unwrap();
        app.open_path(&path).unwrap();
        let original = app.current_position().unwrap();
        let mut canvas = Canvas::new(WIDTH, HEIGHT, &[font.display().to_string()]).unwrap();
        let opening = primitive_app::frame(&app, &mut canvas, false);

        for title in ["Arrival", "  Trail directions", "  Elevation figure"] {
            app.open_reading_edge(true, 630.0);
            if app.top_group != Some(TopGroup::Navigation) {
                tap_reading_control(&mut app, "Navigation");
            }
            assert_eq!(tap_reading_control(&mut app, "Contents"), Change::Partial);
            if title == "Arrival" {
                capture_test_screen(
                    &app,
                    std::slice::from_ref(&font),
                    "READER_KF8_CONTENTS_SCREENSHOT",
                    false,
                );
            }
            tap_contents_item(&mut app, title);
            assert_eq!(app.screen, Screen::Book);
            if title == "Arrival" {
                assert_eq!(app.active.as_ref().unwrap().reading.chapter, 1);
            }
            app.activate(Action::ReturnToLocation).unwrap();
            assert_eq!(app.current_position(), Some(original.clone()));
            assert!(opening == primitive_app::frame(&app, &mut canvas, false));
        }
    }

    #[test]
    fn publisher_landmarks_open_the_group_target_and_return_without_moving_saved_marks() {
        let fixture = Fixture::new();
        fs::write(
            &fixture.book,
            include_bytes!("../tests/fixtures/landmark-survey.epub"),
        )
        .unwrap();
        let font = crate::test_fixture("fonts/NotoSans-Regular.ttf");
        let fonts = vec![font.clone()];
        let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
        app.open_path(&fixture.book).unwrap();
        let original = app.current_position().unwrap();
        let id = app.active.as_ref().unwrap().id;
        assert_eq!(original.page, 0);
        assert_eq!(app.active.as_ref().unwrap().reading.chapter, 0);
        let mut canvas = Canvas::new(WIDTH, HEIGHT, &[font.display().to_string()]).unwrap();
        let opening = primitive_app::frame(&app, &mut canvas, false);
        let mut targets = Vec::new();
        for (index, label) in ["Start the survey", "Reference", "  Source notes"]
            .iter()
            .enumerate()
        {
            app.open_reading_edge(index != 1, 630.0);
            if index != 1 && app.top_group != Some(TopGroup::Navigation) {
                tap_reading_control(&mut app, "Navigation");
            }
            assert_eq!(tap_reading_control(&mut app, "Contents"), Change::Partial);
            if index == 0 {
                capture_test_screen(&app, &fonts, "READER_LANDMARK_CONTENTS_SCREENSHOT", false);
            }
            tap_contents_item(&mut app, label);
            assert_eq!(app.screen, Screen::Book);
            assert_eq!(app.active.as_ref().unwrap().reading.chapter, 1);
            targets.push(app.current_position().unwrap());
            app.activate(Action::ReturnToLocation).unwrap();
            assert_eq!(app.current_position(), Some(original.clone()));
            assert!(primitive_app::frame(&app, &mut canvas, false) == opening);
        }
        assert_ne!(targets[0], targets[1]);
        assert_eq!(targets[1], targets[2]);
        assert!(app.library.state(id).unwrap().bookmarks.is_empty());
        app.save().unwrap();
        drop(app);
        let mut reopened = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
        reopened.activate(Action::Continue).unwrap();
        assert_eq!(reopened.current_position(), Some(original));
    }

    #[test]
    fn publisher_landmarks_keep_primary_contents_and_printed_pages_independent() {
        let fixture = Fixture::new();
        let primary = r#"<nav epub:type="toc"><ol><li><a href="first.xhtml#passage1">Primary</a></li></ol></nav>"#;
        let landmarks = r#"<nav epub:type="landmarks"><ol><li><span>Reference</span><ol><li><span>Nested</span><ol><li><a href="missing.xhtml">Missing document</a></li><li><a href="first.xhtml#missing">Missing anchor</a></li><li><a href="first.xhtml#passage12">Sources</a></li></ol></li></ol></li><li><span>Empty branch</span><ol><li><a href="https://example.org/">External</a></li></ol></li><li><a href="second.xhtml">Appendix</a></li></ol></nav>"#;
        let printed = r#"<nav epub:type="page-list"><ol><li><a href="first.xhtml#passage2">10</a></li></ol></nav>"#;
        let ncx = br#"<ncx><navMap><navPoint><navLabel><text>NCX entry</text></navLabel><content src="second.xhtml"/></navPoint></navMap></ncx>"#;
        let open = |nav: String| {
            let nav = format!(
                r#"<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><body>{nav}</body></html>"#
            );
            write_outline_fixture(&fixture.book, Some(nav.as_bytes()), Some(ncx));
            crate::epub::Book::open(&fixture.book).unwrap()
        };
        let selected = open(format!("{landmarks}{printed}{primary}"));
        assert_eq!(selected.toc()[0].label, "Primary");
        assert_eq!(selected.toc().len(), 1);
        assert_eq!(selected.printed_pages()[0].label, "10");
        let landmarks_book = open(format!("{printed}{landmarks}"));
        let rows = landmarks_book.toc();
        assert_eq!(rows[0].destination, rows[4].destination);
        assert_eq!(rows[1].destination, rows[4].destination);
        assert_eq!(
            rows[4].destination.as_ref().unwrap().fragment.as_deref(),
            Some("passage12")
        );
        assert_eq!(rows[5].label, "Empty branch");
        assert!(rows[5].destination.is_none());
        let pages = open(printed.to_owned());
        assert_eq!(pages.toc(), pages.printed_pages());
        let malformed = open(format!(r#"{landmarks}<nav epub:type="toc"><ol/></nav>"#));
        assert_eq!(malformed.toc()[0].label, "NCX entry");
    }

    #[test]
    #[ignore = "compares five independently authored files against the installed KOReader control"]
    fn publisher_landmarks_match_independent_reference_controls() {
        let folder = PathBuf::from(std::env::var_os("READER_TEST_PUBLISHER_NAV_VARIANTS").unwrap());
        for (file, expected) in [
            (
                "primary",
                vec![("Primary entry", 0, "first.xhtml", "start")],
            ),
            (
                "landmarks",
                vec![
                    ("Start the survey", 0, "second.xhtml", "body"),
                    ("Reference", 0, "second.xhtml", "sources"),
                    ("Source notes", 1, "second.xhtml", "sources"),
                ],
            ),
            (
                "landmarks-before-ncx",
                vec![
                    ("Start the survey", 0, "second.xhtml", "body"),
                    ("Reference", 0, "second.xhtml", "sources"),
                    ("Source notes", 1, "second.xhtml", "sources"),
                ],
            ),
            (
                "pages",
                vec![
                    ("10", 0, "first.xhtml", "start"),
                    ("21", 0, "second.xhtml", "body"),
                ],
            ),
            ("epub2-guide", vec![]),
        ] {
            let book = crate::epub::Book::open(&folder.join(format!("{file}.epub"))).unwrap();
            assert_eq!(book.toc().len(), expected.len());
            for (row, (label, depth, path, fragment)) in book.toc().iter().zip(expected) {
                assert_eq!((row.label.as_str(), row.depth), (label, depth));
                let destination = row.destination.as_ref().unwrap();
                assert_eq!(destination.path, path);
                assert_eq!(destination.fragment.as_deref(), Some(fragment));
            }
            assert_eq!(book.chapters.len(), 2);
            assert_eq!(
                book.printed_pages().len(),
                if file == "epub2-guide" { 0 } else { 2 }
            );
        }
    }

    #[test]
    fn contents_opens_at_the_current_section_from_either_reading_bar() {
        let fixture = Fixture::new();
        let path = fixture.root.join("Document.epub");
        let external = std::env::var_os("READER_TEST_CONTENTS_EPUB").map(PathBuf::from);
        fs::copy(
            external.as_deref().unwrap_or(&crate::test_fixture("many-chapters.epub")),
            &path,
        ).unwrap();
        let font = crate::test_fixture("fonts/NotoSans-Regular.ttf");
        let mut canvas = Canvas::new(WIDTH, HEIGHT, &[font.display().to_string()]).unwrap();
        let mut app = Reader::open(&fixture.root, &fixture.state, vec![font.clone()]).unwrap();
        // Keep external publication names and rendered pages out of test reports.
        assert!(app.open_path(&path).is_ok());
        app.show(Screen::Contents);
        let items = app.items();
        let row = if external.is_some() {
            items.len() / 2
        } else {
            items.iter().position(|item| item.title == "Day 38").unwrap()
        };
        assert!(row >= ROWS_PER_PAGE);
        let Action::Jump(destination) = &items[row].action else {
            panic!("The fixture needs an internal destination at the chosen list row");
        };
        assert!(app.follow(destination.clone()).is_ok());
        let before = app.current_position().unwrap();
        let reading_frame = primitive_app::frame(&app, &mut canvas, false);
        let history_len = app.back.len();
        for upper in [true, false] {
            let y = if upper { 27.0 } else { 1420.0 };
            assert_eq!(app.input(Input::Tap(630.0, y)), Change::Partial);
            if upper && app.top_group != Some(TopGroup::Navigation) {
                tap_reading_control(&mut app, "Navigation");
            }
            assert_eq!(tap_reading_control(&mut app, "Contents"), Change::Partial);
            assert_eq!(app.current_contents_row, Some(row));
            assert_eq!(app.list_page, row / ROWS_PER_PAGE);
            let contents_frame = primitive_app::frame(&app, &mut canvas, false);
            assert!(contents_frame != reading_frame);
            if upper && external.is_none() {
                capture_test_screen(&app, std::slice::from_ref(&font), "READER_CURRENT_CONTENTS_SCREENSHOT", false);
            }
            assert_eq!(app.input(Input::PageBack), Change::Partial);
            assert_eq!(app.list_page, row / ROWS_PER_PAGE - 1);
            let (x, y) = rectangle_center(primitive_ui::back_button(WIDTH as i32));
            assert_eq!(app.input(Input::Tap(x, y)), Change::Partial);
            assert_eq!(app.input(Input::Tap(630.0, 1010.0)), Change::Partial);
            assert_eq!(app.screen, Screen::Book);
            assert_eq!(app.current_contents_row, None);
            assert!(app.current_position().as_ref() == Some(&before));
            assert_eq!(app.back.len(), history_len);
            assert!(primitive_app::frame(&app, &mut canvas, false) == reading_frame);
        }
    }

    #[test]
    fn contents_resolves_same_chapter_fragments_and_ignores_unreadable_targets() {
        let fixture = Fixture::new();
        let path = fixture.root.join("Anchored contents.epub");
        let nav = br##"<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><body><nav epub:type="toc"><ol><li><a href="first.xhtml#absent">Missing</a></li><li><a href="first.xhtml#passage2">Opening</a></li><li><a href="first.xhtml#passage12">Later section</a></li><li><a href="first.xhtml#passage12">Duplicate heading</a></li><li><a href="second.xhtml">Closing</a></li></ol></nav></body></html>"##;
        write_outline_fixture(&path, Some(nav), None);
        let mut app = fixture.app();
        app.open_path(&path).unwrap();
        let active = &mut app.active.as_mut().unwrap().reading;
        active.jump(0, Some("passage12")).unwrap();
        let before = app.current_position().unwrap();
        app.show(Screen::Contents);
        assert_eq!(app.items()[app.current_contents_row.unwrap()].title, "Later section");
        assert_eq!(app.current_position(), Some(before));
    }

    #[test]
    fn fixed_book_map_browses_without_moving_and_returns_from_a_selected_page() {
        let fixture = Fixture::new();
        let path = fixture.root.join("Atlas.pdf");
        let external = std::env::var_os("READER_TEST_BOOK_MAP_PDF");
        if let Some(source) = &external {
            fs::copy(source, &path).unwrap();
        } else {
            let kids = (3..99).map(|id| format!("{id} 0 R")).collect::<Vec<_>>().join(" ");
            let mut objects = vec![
                "<< /Type /Catalog /Pages 2 0 R >>".into(),
                format!("<< /Type /Pages /Count 96 /Kids [{kids}] /MediaBox [0 0 300 400] >>"),
            ];
            objects.extend((0..96).map(|_| "<< /Type /Page /Parent 2 0 R >>".into()));
            fs::write(&path, crate::fixed::tests::pdf_objects(&objects)).unwrap();
        }
        let font = crate::test_fixture("fonts/NotoSans-Regular.ttf");
        let mut app = Reader::open(&fixture.root, &fixture.state, vec![font.clone()]).unwrap();
        assert!(app.open_path(&path).is_ok());
        let count = app.fixed.as_ref().unwrap().reading.count();
        assert!((49..=2304).contains(&count));
        let original = count / 2;
        app.follow_fixed(original, None).unwrap();
        app.activate(Action::ToggleBookmark).unwrap();
        if external.is_none() {
            let id = app.open_book_id().unwrap();
            let custom = &mut app.library.state_mut(id).unwrap().custom_contents;
            custom.enabled = true;
            for (page, label) in [(0, "Harbor"), (12, "Estuary"), (24, "Narrows"), (36, "Ferry"), (48, "Headland"), (60, "Beacon"), (72, "Passage"), (84, "Inlet")] {
                custom.save(ReadingPosition { chapter: 0, page, anchor: None }, label).unwrap();
            }
        }
        app.show(Screen::Fixed);
        if external.is_none() {
            use crate::reading_controls::StatusBar;
            app.controls.status_bar = StatusBar::Chapter;
            assert_eq!(app.status_caption().as_deref(), Some("Headland  ·  Page 1 of 12"));
            let id = app.open_book_id().unwrap();
            app.library.state_mut(id).unwrap().custom_contents.enabled = false;
            assert_eq!(app.status_caption().as_deref(), Some("Document  ·  Page 49 of 96"));
            app.library.state_mut(id).unwrap().custom_contents.enabled = true;
        }
        let prior = app.back.clone();
        let fonts = [font.display().to_string()];
        let mut canvas = Canvas::new(WIDTH, HEIGHT, &fonts).unwrap();
        let frame = primitive_app::frame(&app, &mut canvas, false);
        app.open_reading_edge(true, 1.0);
        tap_reading_control(&mut app, "Book map");
        assert_eq!(app.screen, Screen::BookMap);
        if external.is_none() {
            capture_test_screen(&app, &[font], "READER_TEST_BOOK_MAP_SCREEN", false);
        }
        let tap = |i| {
            let rect = fixed_book_map::cell(i);
            Input::Tap((rect.x + rect.w / 2) as f32, (rect.y + rect.h / 2) as f32)
        };
        let slot_width = count.div_ceil(48);
        app.input(tap(1));
        assert_eq!(app.input(Input::PageForward), Change::Partial);
        assert_eq!(app.input(Input::PageBack), Change::Partial);
        assert_eq!(app.current_position().unwrap().page, original);
        assert_eq!(app.back, prior);
        assert_eq!(app.input(tap(0)), Change::Full);
        assert_eq!(app.screen, Screen::Fixed);
        assert_eq!(app.current_position().unwrap().page, slot_width);
        app.activate(Action::ReturnToLocation).unwrap();
        assert_eq!(app.current_position().unwrap().page, original);
        assert!(primitive_app::frame(&app, &mut canvas, false) == frame);
    }

    #[test]
    fn djvu_contents_uses_both_reading_bars_and_returns_to_the_same_scan() {
        let fixture = Fixture::new();
        let path = fixture.root.join("Beacon Atlas.djvu");
        fs::copy(crate::test_fixture("beacon-atlas.djvu"), &path).unwrap();
        let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
        let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
        app.open_path(&path).unwrap();
        app.activate(Action::SetFixedZoom(3)).unwrap();
        app.input(Input::Swipe { dx: -180.0, dy: -220.0, start_y: 640.0 });
        let cover = app.current_position().unwrap();
        let origin = app.fixed.as_ref().unwrap().reading.history_point().unwrap();
        assert!(origin.x > 0.0 && origin.y > 0.0);
        assert!(app.fixed.as_ref().unwrap().footer_chapters.iter().all(|(_, title)| title != "Next plate"));

        assert_eq!(app.fixed.as_ref().unwrap().reading.page(), 0);
        app.input(Input::Tap(630.0, 27.0));
        if app.top_group != Some(TopGroup::Navigation) {
            tap_reading_control(&mut app, "Navigation");
        }
        tap_reading_control(&mut app, "Contents");
        assert_eq!(app.screen, Screen::FixedContents);
        capture_test_screen(&app, &fonts, "READER_DJVU_CONTENTS_SCREENSHOT", false);
        let items = app.items();
        assert!(items.iter().any(|item| item.title == "Source website" && matches!(item.action, Action::Heading)));
        assert!(items.iter().any(|item| item.title == "Unknown plate" && matches!(item.action, Action::Heading)));
        tap_contents_item(&mut app, "Unknown plate");
        assert_eq!(app.screen, Screen::FixedContents);
        assert_eq!(app.current_position(), Some(cover.clone()));
        tap_contents_item(&mut app, "  Outer reef");
        let reef = app.current_position().unwrap();
        assert_eq!(app.fixed.as_ref().unwrap().reading.page(), 2);
        app.input(Input::Tap(630.0, 1420.0));
        tap_reading_control(&mut app, "Contents");
        assert_eq!(app.screen, Screen::FixedContents);
        assert_eq!(app.items()[app.current_contents_row.unwrap()].title, "  Outer reef");
        tap_contents_item(&mut app, "  Nearest station");
        assert_eq!(app.fixed.as_ref().unwrap().reading.page(), 4);
        app.activate(Action::ReturnToLocation).unwrap();
        assert_eq!(app.current_position(), Some(reef));
        app.activate(Action::ReturnToLocation).unwrap();
        assert_eq!(app.current_position(), Some(cover));
        let restored = app.fixed.as_ref().unwrap().reading.history_point().unwrap();
        assert!((origin - restored).hypot() < 1.0);
        app.save().unwrap();
        let mut reopened = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
        reopened.activate(Action::Continue).unwrap();
        assert_eq!(reopened.fixed.as_ref().unwrap().reading.page(), 0);
        reopened.input(Input::PageForward);
        reopened.tick(Instant::now() + Duration::from_secs(1));
        assert_eq!(reopened.fixed.as_ref().unwrap().reading.page(), 1);
        reopened.input(Input::PageBack);
        reopened.tick(Instant::now() + Duration::from_secs(2));
        assert_eq!(reopened.fixed.as_ref().unwrap().reading.page(), 0);
    }

    #[test]
    fn fixed_contents_uses_publisher_and_custom_page_destinations() {
        let fixture = Fixture::new();
        let path = fixture.root.join("Chart booklet.pdf");
        fs::write(&path, crate::fixed::tests::pdf_objects(&[
            "<< /Type /Catalog /Pages 2 0 R /Outlines 5 0 R >>",
            "<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 120] >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 120] >>",
            "<< /Type /Outlines /First 6 0 R /Last 7 0 R >>",
            "<< /Title (First chart) /Dest [3 0 R /Fit] /Parent 5 0 R /Next 7 0 R >>",
            "<< /Title (Second chart) /Dest [4 0 R /Fit] /Parent 5 0 R /Prev 6 0 R >>",
        ])).unwrap();
        let mut app = fixture.app();
        app.open_path(&path).unwrap();
        app.follow_fixed(1, None).unwrap();
        let before = app.current_position().unwrap();
        app.show(Screen::FixedContents);
        assert_eq!(app.items()[app.current_contents_row.unwrap()].title, "Second chart");
        assert_eq!(app.current_position(), Some(before.clone()));

        let fixed = app.fixed.as_ref().unwrap();
        let custom = &mut app.library.remember(&fixed.entry, fixed.id, &fixed.entry.title).custom_contents;
        custom.save(before.clone(), "Local soundings").unwrap();
        custom.enabled = true;
        app.show(Screen::FixedContents);
        assert_eq!(app.items()[app.current_contents_row.unwrap()].title, "Local soundings");
        assert_eq!(app.current_position(), Some(before));
    }

    #[test]
    fn extends_a_saved_epub_note_across_chapters_without_changing_the_original_on_cancel() {
        let fixture = Fixture::new();
        let font = crate::test_fixture("fonts/NotoSans-Regular.ttf");
        let mut app = Reader::open(&fixture.root, &fixture.state, vec![font.clone()]).unwrap();
        app.open_path(&fixture.book).unwrap();
        let reading = &app.active.as_ref().unwrap().reading;
        let mark = reading.probe.fragment("tide-note").unwrap();
        let mut end = mark.clone();
        end.offset += "The first step".len();
        app.selection = reading.probe.select_range(&mark, &end);
        app.save_highlight(Some("Keep this writing".into()))
            .unwrap();
        app.follow_reflow_page(1, 0).unwrap();
        let origin = app.current_history_location().unwrap();
        let id = app.open_book_id().unwrap();
        let original = app.library.state(id).unwrap().highlights[0]
            .selection
            .clone();
        let history_before = app.back.clone();
        for save in [false, true] {
            app.activate(Action::ManageHighlight(0)).unwrap();
            let mut canvas = Canvas::new(WIDTH, HEIGHT, &[font.display().to_string()]).unwrap();
            let before = primitive_app::frame(&app, &mut canvas, false);
            tap_contents_item(&mut app, "Extend highlight");
            assert!(app.range_edit.is_some());
            for _ in 0..32 {
                if app.active.as_ref().unwrap().reading.chapter == 1 {
                    break;
                }
                app.input(Input::PageForward);
                app.tick(Instant::now() + Duration::from_secs(1));
            }
            let reading = &app.active.as_ref().unwrap().reading;
            assert_eq!(reading.chapter, 1);
            let start = reading.probe.fragment("door").unwrap();
            let mut end = start.clone();
            end.offset += 2;
            let word = reading.probe.select_range(&start, &end).unwrap();
            let bounds = reading.highlight_rects(&word)[0];
            assert_eq!(
                app.input(Input::Tap(
                    (bounds.x + bounds.width as i32 / 2) as f32,
                    (bounds.y + bounds.height as i32 / 2) as f32
                )),
                Change::Partial
            );
            let extend_highlight::Draft::Text {current: draft,..} = &app.range_edit.as_ref().unwrap().draft else { panic!("expected an EPUB highlight") };
            assert_ne!(draft.start.chapter, draft.end.chapter);
            assert!(draft.text.contains("The first step"));
            assert!(draft.text.ends_with("At"));
            let reading = &app.active.as_ref().unwrap().reading;
            assert!(!reading.highlight_rects(draft).is_empty());
            let first = reading.config.chapter(0).unwrap();
            assert!(
                !first
                    .selection_rects(first.page_for(&draft.start).unwrap(), draft)
                    .is_empty()
            );
            let at = reading.probe.select_range(&start, &end).unwrap();
            let first_word = first.select_range(&original.start, &original.end).unwrap();
            let reverse = reading.extend_saved_quote(&at, &first_word).unwrap();
            assert_eq!(&reverse, draft);
            assert_eq!(
                app.library.state(id).unwrap().highlights[0].selection,
                original
            );
            if save {
                capture_test_screen(
                    &app,
                    std::slice::from_ref(&font),
                    "READER_EXTEND_HIGHLIGHT_SCREENSHOT",
                    false,
                );
                app.input(Input::Tap(1000.0, HEIGHT as f32 - 43.0));
                assert!(app.range_edit.is_none());
                app.save().unwrap();
            } else {
                app.input(Input::Tap(125.0, HEIGHT as f32 - 43.0));
                assert!(app.range_edit.is_none());
                assert_eq!(app.screen, Screen::Annotation);
                assert_eq!(app.current_history_location(), Some(origin.clone()));
                assert_eq!(app.back, history_before);
                assert!(primitive_app::frame(&app, &mut canvas, false) == before);
                assert_eq!(
                    app.library.state(id).unwrap().highlights[0].selection,
                    original
                );
            }
        }
        let reopened = Reader::open(&fixture.root, &fixture.state, vec![font]).unwrap();
        let marks = &reopened.library.state(id).unwrap().highlights;
        assert_eq!(marks.len(), 1);
        assert_eq!(marks[0].note.as_deref(), Some("Keep this writing"));
        assert_eq!(marks[0].selection.start, original.start);
        assert_ne!(
            marks[0].selection.start.chapter,
            marks[0].selection.end.chapter
        );
    }

    fn submit_annotation_search(app: &mut Reader, query: &str) {
        assert_eq!(app.screen, Screen::BookmarkFind);
        app.editor.text = query.chars().collect();
        app.editor.caret = app.editor.text.len();
        let go = Layout::new(WIDTH, HEIGHT, 4, 5).keyboard_keys_with(app.editor.layer, "Find")
            .into_iter().find(|(key, _)| key.key == primitive_ui::Key::Go).unwrap().1;
        let (x, y) = rectangle_center(go);
        assert_eq!(app.input(Input::Tap(x, y)), Change::Partial);
        assert_eq!(app.screen, Screen::BookmarkMatches);
    }

    #[test]
    fn annotation_search_finds_note_only_text_and_returns_to_the_source_on_either_bar() {
        let fixture = Fixture::new();
        let font = crate::test_fixture("fonts/NotoSans-Regular.ttf");
        let mut app = Reader::open(&fixture.root, &fixture.state, vec![font.clone()]).unwrap();
        app.open_path(&fixture.book).unwrap();
        app.activate(Action::AddBookmark).unwrap();
        for (chapter, fragment, excerpt, note) in [
            (0, "lantern", "The copper lantern", Some("Compare the Amber chart")),
            (1, "door", "At dusk the door", None),
        ] {
            let reading = &mut app.active.as_mut().unwrap().reading;
            reading.jump(chapter, Some(fragment)).unwrap();
            let start = reading.probe.fragment(fragment).unwrap();
            let mut end = start.clone();
            end.offset += excerpt.len();
            app.selection = reading.probe.select_range(&start, &end);
            assert_eq!(app.selection.as_ref().unwrap().text, excerpt);
            app.save_highlight(note.map(str::to_owned)).unwrap();
        }
        let original = app.current_position().unwrap();
        app.open_reading_edge(false, 630.0);
        assert_eq!(tap_reading_control(&mut app, "Bookmarks"), Change::Partial);
        tap_contents_item(&mut app, "Find bookmarks");
        capture_test_screen(&app, std::slice::from_ref(&font), "READER_ANNOTATION_FILTERS_SCREENSHOT", false);
        submit_annotation_search(&mut app, "amber");
        assert_eq!(app.items().len(), 1);
        tap_contents_item(&mut app, "Compare the Amber chart");
        assert_eq!(app.screen, Screen::Book);
        let reading = &app.active.as_ref().unwrap().reading;
        assert_eq!(reading.chapter, 0);
        assert_eq!(reading.probe.page_for(&reading.probe.fragment("lantern").unwrap()), Some(reading.page));
        app.activate(Action::ReturnToLocation).unwrap();
        assert_eq!(app.current_position(), Some(original));

        app.input(Input::Tap(630.0, 35.0));
        tap_reading_control(&mut app, "Search");
        tap_reading_control(&mut app, "Find bookmarks");
        assert_eq!(app.editor.string(), "amber");
        assert_eq!(app.input(Input::Tap(160.0, 330.0)), Change::Partial);
        submit_annotation_search(&mut app, "amber");
        assert!(app.items().is_empty());
        let (bx,by) = rectangle_center(primitive_ui::back_button(WIDTH as i32));
        app.input(Input::Tap(bx,by));
        assert_eq!(app.screen, Screen::BookmarkFind);
        submit_annotation_search(&mut app, "Amber");
        assert_eq!(app.items().len(), 1);
        app.input(Input::Tap(bx,by));
        submit_annotation_search(&mut app, "");
        assert_eq!(app.items().len(), 3);
        capture_test_screen(&app, std::slice::from_ref(&font), "READER_ANNOTATION_RESULTS_SCREENSHOT", false);
        app.input(Input::Tap(bx,by));
        app.input(Input::Tap(1050.0, 424.0)); // Exclude notes.
        submit_annotation_search(&mut app, "copper");
        assert!(app.items().is_empty()); // A highlight with a note belongs to the Notes category.
        app.input(Input::Tap(bx,by));
        app.input(Input::Tap(640.0, 424.0)); // Exclude highlights.
        submit_annotation_search(&mut app, "");
        assert_eq!(app.items().len(), 1);
        assert!(matches!(app.items()[0].action, Action::Bookmark(0)));
    }

    #[test]
    fn extends_one_saved_pdf_note_across_three_pages_and_restores_the_original_on_cancel() {
        let fixture = Fixture::new();
        let path = fixture.root.join("Harbor chart.pdf");
        let mut objects = vec![
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R 4 0 R 5 0 R] /Count 3 >>".to_string(),
        ];
        for page in 0..3 {
            objects.push(format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 140] /Contents {} 0 R /Resources << /Font << /F1 9 0 R >> >> >>",page+6));
        }
        for line in ["Before low tide", "Quiet water", "After high tide"] {
            let stream = format!("BT /F1 9 Tf 8 100 Td ({line}) Tj ET\n");
            objects.push(format!(
                "<< /Length {} >>\nstream\n{stream}endstream",
                stream.len()
            ));
        }
        objects.push("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into());
        let bytes = crate::fixed::tests::pdf_objects(
            &objects.iter().map(String::as_str).collect::<Vec<_>>(),
        );
        fs::write(&path, &bytes).unwrap();
        let font = crate::test_fixture("fonts/NotoSans-Regular.ttf");
        let mut app = Reader::open(&fixture.root, &fixture.state, vec![font.clone()]).unwrap();
        app.open_path(&path).unwrap();
        let fixed = app.fixed.as_ref().unwrap();
        let id = fixed.id;
        let layer = fixed.reading.page_text(0).unwrap();
        let first = layer.words.last().unwrap();
        let point = fixed.reading.visible_rectangle(0, first.bounds).unwrap().center();
        let before = crate::mark_time::now().unwrap();
        app.input(Input::LongTap(point.x as f32, point.y as f32));
        app.save_pdf_highlight(Some("Check the harbor".into())).unwrap();
        let fixed = app.fixed.as_ref().unwrap();
        let mark = &mut app.library.remember(&fixed.entry, id, &fixed.entry.title).pdf_highlights[0];
        assert!((before..=crate::mark_time::now().unwrap()).contains(&mark.created_at.unwrap()));
        // An edit of an older annotation must keep its independently saved creation time.
        mark.created_at = Some(1_600_000_001);
        let initial = mark.clone();
        app.follow_fixed(1, None).unwrap();
        let origin = app.current_history_location().unwrap();
        let history = app.back.clone();
        for save in [false, true] {
            app.activate(Action::ManageHighlight(0)).unwrap();
            let mut canvas = Canvas::new(WIDTH, HEIGHT, &[font.display().to_string()]).unwrap();
            let before = primitive_app::frame(&app, &mut canvas, false);
            tap_contents_item(&mut app, "Extend highlight");
            assert_eq!(app.screen, Screen::Fixed);
            assert_eq!(app.fixed.as_ref().unwrap().reading.page(), 0);
            for _ in 0..2 {
                app.input(Input::PageForward);
                app.tick(Instant::now() + Duration::from_secs(1));
            }
            let fixed = app.fixed.as_ref().unwrap();
            assert_eq!(fixed.reading.page(), 2);
            let text = fixed.reading.page_text(2).unwrap();
            let point = fixed
                .reading
                .visible_rectangle(2, text.words[1].bounds)
                .unwrap()
                .center();
            app.input(Input::Tap(point.x as f32, point.y as f32));
            let extend_highlight::Draft::Fixed { current, .. } =
                &app.range_edit.as_ref().unwrap().draft
            else {
                panic!("expected a fixed-page highlight")
            };
            assert_eq!(current.text, "tide\nQuiet water\nAfter high");
            assert_eq!(
                current
                    .parts()
                    .map(|(page, bounds, text)| (page, bounds.len(), text))
                    .collect::<Vec<_>>(),
                [(0, 1, "tide"), (1, 2, "Quiet water"), (2, 2, "After high")]
            );
            assert_eq!(
                app.library.state(id).unwrap().pdf_highlights,
                vec![initial.clone()]
            );
            if save {
                capture_test_screen(
                    &app,
                    std::slice::from_ref(&font),
                    "READER_EXTEND_PDF_SCREENSHOT",
                    false,
                );
                app.input(Input::Tap(1000.0, HEIGHT as f32 - 43.0));
            } else {
                app.input(Input::Tap(125.0, HEIGHT as f32 - 43.0));
                assert_eq!(app.screen, Screen::Annotation);
                assert_eq!(app.current_history_location(), Some(origin.clone()));
                assert_eq!(app.back, history);
                assert!(primitive_app::frame(&app, &mut canvas, false) == before);
            }
        }
        app.save().unwrap();
        let reopened = Reader::open(&fixture.root, &fixture.state, vec![font]).unwrap();
        let records = &reopened.library.state(id).unwrap().pdf_highlights;
        assert_eq!(records.len(), 1);
        let mark = &records[0];
        assert_eq!(mark.note.as_deref(), Some("Check the harbor"));
        assert_eq!(mark.created_at, Some(1_600_000_001));
        assert_eq!(mark.continuation.len(), 2);
        let fixed = app.fixed.as_ref().unwrap();
        let geometry = |page| fixed.reading.pdf_geometry(page);
        let exported = pdf_annotations::export_copy(&bytes, records, geometry).unwrap();
        let native = pdf_annotations::import_highlights(&exported, geometry).unwrap();
        assert_eq!(native.len(), 3);
        for ((page, bounds, quote), pdf) in mark.parts().zip(&native) {
            assert_eq!(page, pdf.page);
            assert_eq!(bounds.len(), pdf.bounds.len());
            assert_eq!(pdf.text.as_deref(), Some(quote));
            assert_eq!(pdf.note, mark.note);
        }
        assert_eq!(
            pdf_annotations::export_copy(&exported, records, geometry).unwrap(),
            exported
        );
    }

    #[test]
    fn annotation_search_opens_a_saved_pdf_note_and_restores_the_source_page() {
        let fixture = Fixture::new();
        let path = fixture.root.join("Log sheets.pdf");
        fs::write(&path, crate::fixed::tests::document()).unwrap();
        let mut app = fixture.app();
        app.open_path(&path).unwrap();
        let fixed = app.fixed.as_ref().unwrap();
        let layer = fixed.reading.page_text(1).unwrap();
        let word = layer.words.first().unwrap();
        let state = app
            .library
            .remember(&fixed.entry, fixed.id, &fixed.entry.title);
        state
            .add_pdf_highlight(PdfHighlight {
                color: Default::default(),
                created_at: None,
                style: Default::default(),
                continuation: Vec::new(),
                page: 1,
                bounds: vec![word.bounds],
                text: layer.text[word.bytes.clone()].to_owned(),
                note: Some("Harbor sounding".into()),
            })
            .unwrap();
        app.show(Screen::FixedBookmarks);
        tap_contents_item(&mut app, "Find bookmarks");
        submit_annotation_search(&mut app, "harbor");
        assert_eq!(app.items().len(), 1);
        tap_contents_item(&mut app, "Harbor sounding");
        assert_eq!(app.fixed.as_ref().unwrap().reading.page(), 1);
        app.activate(Action::ReturnToLocation).unwrap();
        assert_eq!(app.fixed.as_ref().unwrap().reading.page(), 0);
        app.show(Screen::FixedBookmarks);
        tap_contents_item(&mut app, "Find bookmarks");
        let (x,y) = rectangle_center(primitive_ui::back_button(WIDTH as i32));
        app.input(Input::Tap(x,y));
        assert_eq!(app.screen, Screen::FixedBookmarks);
    }

    fn save_chapter_title(app: &mut Reader, title: &str) {
        assert_eq!(app.screen, Screen::CustomTitle);
        app.editor.text = title.chars().collect();
        let go = Layout::new(WIDTH, HEIGHT, 4, 5).keyboard_keys_with(app.editor.layer, "Save")
            .into_iter().find(|(key, _)| key.key == primitive_ui::Key::Go).unwrap().1;
        let (x,y) = rectangle_center(go);
        app.input(Input::Tap(x,y));
    }

    #[test]
    fn custom_contents_edits_anchored_bookmarks_and_preserves_publisher_contents() {
        let fixture = Fixture::new();
        let fonts = simulation_fonts();
        let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
        app.open_path(&fixture.book).unwrap();
        app.activate(Action::Jump(Destination { path: "EPUB/chapters/dusk.xhtml".into(), fragment: Some("door".into()) })).unwrap();
        let destination = app.current_position().unwrap();
        app.activate(Action::AddBookmark).unwrap();
        app.activate(Action::Jump(Destination { path: "EPUB/chapters/shore.xhtml".into(), fragment: Some("tide-b".into()) })).unwrap();
        let origin = app.current_position().unwrap();
        app.show(Screen::Contents);
        tap_contents_item(&mut app, "Custom contents");
        tap_contents_item(&mut app, "Add from bookmark");
        let label = app.items()[0].title.clone();
        tap_contents_item(&mut app, &label);
        save_chapter_title(&mut app, "Return after dark");
        tap_contents_item(&mut app, "Add current location");
        app.input(Input::PageBack);
        assert_eq!(app.library.state(app.open_book_id().unwrap()).unwrap().custom_contents.entries().len(), 1);
        tap_contents_item(&mut app, "Add current location");
        save_chapter_title(&mut app, "Tide observation");
        tap_contents_item(&mut app, "Use custom contents");
        assert_eq!(app.items().iter().map(|i| &*i.title).collect::<Vec<_>>(), ["Manage custom contents", "Tide observation", "Return after dark"]);
        capture_test_screen(&app, &fonts, "READER_CUSTOM_CONTENTS_SCREENSHOT", false);
        tap_contents_item(&mut app, "Return after dark");
        assert_eq!(app.current_position(), Some(destination.clone()));
        app.activate(Action::ReturnToLocation).unwrap();
        assert_eq!(app.current_position(), Some(origin.clone()));
        app.activate(Action::Text(Settings { font_size: 76.0, ..device_settings() })).unwrap();
        app.save().unwrap();
        let mut reopened = fixture.app();
        reopened.activate(Action::Continue).unwrap();
        reopened.show(Screen::Contents);
        tap_contents_item(&mut reopened, "Return after dark");
        assert_eq!(reopened.current_position().unwrap().anchor, destination.anchor);
        reopened.show(Screen::Contents);
        tap_contents_item(&mut reopened, "Manage custom contents");
        tap_contents_item(&mut reopened, "Edit chapters");
        tap_contents_item(&mut reopened, "Return after dark");
        save_chapter_title(&mut reopened, "Dusk");
        tap_contents_item(&mut reopened, "Dusk");
        reopened.input(Input::Tap(100.0,(LIST_Y + 266) as f32));
        reopened.input(Input::PageBack);
        reopened.input(Input::PageBack);
        assert!(reopened.items().iter().any(|item| item.title == "Dusk"));
        tap_contents_item(&mut reopened, "Dusk");
        reopened.input(Input::Tap(100.0,(LIST_Y+266) as f32));
        reopened.input(Input::Tap(800.0,832.0));
        assert_eq!(reopened.library.state(reopened.open_book_id().unwrap()).unwrap().custom_contents.entries().len(), 1);
        reopened.show(Screen::CustomContents);
        tap_contents_item(&mut reopened, "Use publisher contents");
        assert!(reopened.items().iter().any(|item| item.title == "Returning at Dusk"));
        assert_eq!(reopened.library.state(reopened.open_book_id().unwrap()).unwrap().custom_contents.entries().len(), 1);
    }

    #[test]
    fn custom_contents_from_a_selection_keeps_selection_on_cancel_and_exact_anchor_on_save() {
        let fixture = Fixture::new();
        let mut app = fixture.app();
        app.open_path(&fixture.book).unwrap();
        app.custom_action(CustomAction::Toggle).unwrap();
        app.show(Screen::Book);
        let probe = &app.active.as_ref().unwrap().reading.probe;
        let start = probe.fragment("tide-b").unwrap();
        let mut end = start.clone();
        end.offset += 3;
        let selected = probe.select_range(&start, &end).unwrap();
        app.selection = Some(selected.clone());
        tap_selection_action(&mut app, selection_popup::SelectionAction::Chapter);
        assert_eq!(app.editor.string(), selected.text);
        app.input(Input::PageBack);
        assert_eq!(app.screen, Screen::Book);
        assert_eq!(app.selection, Some(selected.clone()));
        tap_selection_action(&mut app, selection_popup::SelectionAction::Chapter);
        save_chapter_title(&mut app, "Second tide");
        assert!(app.selection.is_none());
        assert!(app.library.state(app.open_book_id().unwrap()).unwrap().custom_contents.entries()[0].position.anchor.as_ref() == Some(&start));
        app.show(Screen::Contents);
        tap_contents_item(&mut app, "Second tide");
        assert_eq!(app.current_position().unwrap().anchor, Some(start));
    }

    #[test]
    fn custom_contents_adds_and_follows_fixed_pages_without_a_publisher_outline() {
        let fixture = Fixture::new();
        let mut app = fixture.app();
        let path = fixture.root.join("No outline.pdf");
        fs::write(&path, crate::fixed::tests::pdf_objects(&[
            "<< /Type /Catalog /Pages 2 0 R >>",
            "<< /Type /Pages /Count 2 /Kids [3 0 R 4 0 R] >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 120] >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 120] >>",
        ])).unwrap();
        app.open_path(&path).unwrap();
        assert!(app.fixed.as_ref().unwrap().reading.outline().is_empty());
        app.follow_fixed(1, None).unwrap();
        app.open_reading_edge(false,500.0);
        tap_reading_control(&mut app, "Contents");
        tap_contents_item(&mut app, "Custom contents");
        tap_contents_item(&mut app, "Add current location");
        save_chapter_title(&mut app, "Second page");
        tap_contents_item(&mut app, "Use custom contents");
        app.follow_fixed(0,None).unwrap();
        app.show(Screen::FixedContents);
        tap_contents_item(&mut app, "Second page");
        assert_eq!(app.current_position().unwrap().page, 1);
        app.activate(Action::ReturnToLocation).unwrap();
        assert_eq!(app.current_position().unwrap().page, 0);
    }

    #[test]
    fn rtl_book_opens_from_documents_follows_a_left_leaf_link_and_resumes() {
        let fixture = Fixture::new();
        let path = fixture.root.join("Harbor notebook.epub");
        fs::copy(crate::test_fixture("rtl-notebook.epub"), &path).unwrap();
        let fonts = simulation_fonts();
        let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
        app.open_path(&path).unwrap();
        app.swipe(
            Input::Swipe {
                dx: 0.0,
                dy: -280.0,
                start_y: HEIGHT as f32 - 28.0,
            },
            500.0,
        );
        assert_eq!(tap_reading_control(&mut app, "Layout"), Change::Partial);
        assert_eq!(tap_reading_control(&mut app, "Two pages"), Change::Partial);
        let outside = Input::Tap(WIDTH as f32 / 2.0, 700.0);
        app.input(outside);
        app.input(outside);
        assert_eq!(app.screen, Screen::Book);
        let reading = &app.active.as_ref().unwrap().reading;
        assert!(reading.uses_facing_pages());
        assert!(reading.probe.pages_right_to_left());
        let origin = reading.anchor();
        capture_test_screen(&app, &fonts, "READER_RTL_SCREENSHOT", false);
        // Find a rendered link on the second leaf, then send ordinary touchscreen input.
        let (x, y) = (90..PAGE_HEIGHT)
            .step_by(8)
            .find_map(|y| {
                (40..WIDTH / 2)
                    .step_by(8)
                    .find(|&x| {
                        reading
                            .link_at(x as f32, y as f32)
                            .is_some_and(|dest| dest.fragment.as_deref() == Some("key"))
                    })
                    .map(|x| (x, y))
            })
            .expect("A link to the second chapter on the left leaf");
        app.input(Input::Tap(x as f32, y as f32));
        let reading = &app.active.as_ref().unwrap().reading;
        assert_eq!(reading.chapter, 1);
        let at_key = reading.anchor();
        assert_ne!(at_key, origin);
        assert_eq!(
            app.input(Input::Tap((DONE.x + 40) as f32, (DONE.y + 25) as f32)),
            Change::Exit
        );
        let mut reopened = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
        reopened.activate(Action::Continue).unwrap();
        let reading = &reopened.active.as_ref().unwrap().reading;
        assert_eq!(reading.chapter, 1);
        assert!(reading.uses_facing_pages());
        assert_eq!(reading.anchor(), at_key);
    }

    #[test]
    fn epub_footer_cycles_persists_and_keeps_the_anchor_when_skimming() {
        use crate::reading_controls::StatusBar;

        let fixture = Fixture::new();
        let mut app = fixture.app();
        app.open_path(&fixture.book).unwrap();
        let position_before = position(&app.active.as_ref().unwrap().reading);
        let tap = Input::Tap(250.0, HEIGHT as f32 - 32.0);
        let hold = Input::LongTap(250.0, HEIGHT as f32 - 32.0);
        assert_eq!(app.input(hold), Change::None);

        for mode in [
            StatusBar::Progress,
            StatusBar::Chapter,
            StatusBar::Remaining,
            StatusBar::ChapterTime,
            StatusBar::BookTime,
            StatusBar::Minimal,
            StatusBar::PageNumbers,
        ] {
            assert_eq!(app.input(tap), Change::Partial);
            assert_eq!(app.screen, Screen::Book);
            assert_eq!(app.controls.status_bar, mode);
            assert_eq!(ReadingControls::load(&app.controls_path, true).status_bar, mode);
            match mode {
                StatusBar::Progress => assert!(app.status_caption().unwrap().ends_with("% of book")),
                StatusBar::Chapter => assert!(app.status_caption().unwrap().contains("Page 1 of")),
                StatusBar::Remaining => assert!(app.status_caption().unwrap().ends_with("left")),
                StatusBar::ChapterTime => {
                    assert!(app.status_caption().unwrap().contains("in chapter"))
                }
                StatusBar::BookTime => assert!(app.status_caption().unwrap().contains("in book")),
                StatusBar::Minimal => assert_eq!(app.status_caption().unwrap(), ""),
                StatusBar::PageNumbers => assert!(app.status_caption().is_none()),
            }
        }
        app.activate(Action::SetReadingControls(ReadingControls {
            skim_on_footer_hold: true,
            footer_all_at_once: true,
            ..app.controls
        })).unwrap();
        assert!(app.status_caption().unwrap().contains("left"));
        app.input(tap);
        assert_eq!(app.controls.status_bar, StatusBar::Minimal);
        assert_eq!(app.input(hold), Change::None);
        app.input(tap);
        assert_eq!(app.input(hold), Change::Partial);
        assert_eq!(app.screen, Screen::PageOverview);
        assert_eq!(position(&app.active.as_ref().unwrap().reading), position_before);

        app.show(Screen::Book);
        app.controls.footer_tap_locked = true;
        assert_eq!(app.input(tap), Change::None);
        app.activate(Action::LockTouch).unwrap();
        assert_eq!(app.input(hold), Change::None);
        assert_eq!(app.screen, Screen::Book);
        assert_eq!(app.input(Input::Tap(DONE.x as f32 + 20.0, DONE.y as f32 + 20.0)), Change::Exit);
    }

    #[test]
    fn selected_wikipedia_and_translation_preview_keep_the_reading_anchor_and_make_no_request() {
        let fixture = Fixture::new();
        let mut app = fixture.app();
        app.open_path(&fixture.book).unwrap();
        let before = position(&app.active.as_ref().unwrap().reading);
        let reading=&app.active.as_ref().unwrap().reading;
        let selected=reading.probe.matches("first").into_iter().next().unwrap();
        let mut end=selected.clone(); end.offset+=5;
        app.selection=reading.probe.select_range(&selected,&end);
        for action in [selection_popup::SelectionAction::Wikipedia, selection_popup::SelectionAction::Translate] {
            let button=app.selection_button(action);
            assert_eq!(app.input(Input::Tap((button.x+button.w/2) as f32,(button.y+button.h/2) as f32)),Change::Partial);
            assert_eq!(app.screen,Screen::OnlineLookup);
            assert!(app.online.as_ref().unwrap().job.is_none());
            assert_eq!(app.online.as_ref().unwrap().request.text,"first");
            assert!(app.lookup_history.online.is_empty());
            app.input(Input::Tap(1090.0,366.0));
            assert_eq!(app.screen,Screen::Book);
            assert_eq!(app.selection.as_ref().unwrap().text,"first");
            assert_eq!(position(&app.active.as_ref().unwrap().reading),before);
        }
        app.show(Screen::BookTools);
        app.activate(Action::TranslatePage).unwrap();
        assert!(app.online.as_ref().unwrap().request.text.contains("first"));
        assert!(app.online.as_ref().unwrap().job.is_none());
        app.input(Input::Tap(1090.0,366.0));
        app.activate(Action::Wikipedia).unwrap();
        assert_eq!(app.screen,Screen::OnlineEdit);
        app.input(Input::PageBack);
        assert_eq!(app.screen,Screen::BookTools);
    }

    #[test]
    fn pdf_footer_uses_outline_pages_and_leaves_zoom_and_done_available() {
        use crate::reading_controls::StatusBar;

        let fixture = Fixture::new();
        let pdf = fixture.root.join("Footer.pdf");
        fs::write(&pdf, crate::fixed::tests::document()).unwrap();
        let mut app = fixture.app();
        app.open_path(&pdf).unwrap();
        let tap = Input::Tap(240.0, HEIGHT as f32 - 30.0);
        app.input(tap);
        assert_eq!(app.status_caption().as_deref(), Some("50% of book"));
        app.input(tap);
        assert!(app.status_caption().unwrap().contains("Page 1 of"));
        let before = app.fixed.as_ref().unwrap().reading.preferences().zoom;
        assert_eq!(app.input(Input::Tap(1030.0, HEIGHT as f32 - 34.0)), Change::Partial);
        assert!(app.fixed.as_ref().unwrap().reading.preferences().zoom > before);
        assert_eq!(app.controls.status_bar, StatusBar::Chapter);
        app.fixed.as_mut().unwrap().reading.jump(1).unwrap();
        app.input(tap);
        assert!(app.status_caption().unwrap().ends_with("0 pages left"));
        assert_eq!(app.input(Input::Tap(DONE.x as f32 + 30.0, DONE.y as f32 + 30.0)), Change::Exit);
    }

    #[test]
    fn epub_layout_applies_and_remembers_facing_pages_from_the_bottom_bar() {
        let fixture = Fixture::new();
        let book = fixture.root.join("Facing pages.epub");
        fs::write(&book, include_bytes!("../tests/fixtures/long-chapter.epub")).unwrap();
        let mut app = fixture.app();
        app.open_path(&book).unwrap();
        app.follow_reflow_page(0, 2).unwrap();
        let before = app.active.as_ref().unwrap().reading.anchor();

        let lower_menu = Input::Tap(WIDTH as f32 / 2.0, HEIGHT as f32 * 0.87);
        assert_eq!(app.input(lower_menu), Change::Partial);
        assert_eq!(tap_reading_control(&mut app, "Layout"), Change::Partial);
        assert_eq!(tap_reading_control(&mut app, "Two pages"), Change::Partial);
        assert_eq!(app.screen, Screen::BookTools);
        assert_eq!(app.reading_panel, Some(BarPanel::TextLayout));
        let reading = &app.active.as_ref().unwrap().reading;
        assert!(reading.config.facing_pages);
        assert!(reading.probe.settings.width < WIDTH);
        assert_eq!(reading.anchor(), before);
        app.save().unwrap();

        let mut reopened = fixture.app();
        reopened.activate(Action::Continue).unwrap();
        let reading = &reopened.active.as_ref().unwrap().reading;
        assert!(reading.config.facing_pages);
        assert_eq!(reading.anchor(), before);
        assert_eq!(reopened.input(lower_menu), Change::Partial);
        assert_eq!(
            tap_reading_control(&mut reopened, "Layout"),
            Change::Partial
        );
        assert_eq!(
            tap_reading_control(&mut reopened, "One page"),
            Change::Partial
        );
        let reading = &reopened.active.as_ref().unwrap().reading;
        assert!(!reading.config.facing_pages);
        assert_eq!(reading.probe.settings.width, WIDTH);
        assert_eq!(reading.anchor(), before);
    }

    #[test]
    fn epub_reading_bars_keep_the_anchor_and_show_one_secondary_panel() {
        let fixture = Fixture::new();
        let long = fixture.root.join("Long Chapter.epub");
        fs::write(&long, include_bytes!("../tests/fixtures/long-chapter.epub")).unwrap();
        let mut app = fixture.app();
        app.open_path(&long).unwrap();
        let reading = |app: &Reader| position(&app.active.as_ref().unwrap().reading);
        let before = reading(&app);
        let open = Input::Tap(WIDTH as f32 / 2.0, 23.0);
        assert_eq!(app.input(open), Change::Partial);
        assert_eq!(reading(&app), before);
        let plain = app.page_paint().visible;
        assert!(plain.start <= 224 && HEIGHT - plain.end <= 112);

        assert_eq!(tap_reading_control(&mut app, "Pages"), Change::Partial);
        assert_eq!(app.reading_panel, Some(BarPanel::Pages));
        assert!(app.page_paint().visible.end < plain.end);
        assert_eq!(
            tap_reading_control(&mut app, "Add bookmark"),
            Change::Partial
        );
        let id = app.active.as_ref().unwrap().id;
        assert_eq!(app.library.state(id).unwrap().bookmarks[0].position, before);
        assert_eq!(app.screen, Screen::Book);
        assert_eq!(app.input(open), Change::Partial);
        assert_eq!(tap_reading_control(&mut app, "Pages"), Change::Partial);
        assert_eq!(tap_reading_control(&mut app, "Text"), Change::Partial);
        assert_eq!(app.reading_panel, Some(BarPanel::Text));
        assert_eq!(reading(&app), before);
        assert_eq!(app.input(Input::Tap(450.0, 810.0)), Change::Partial);
        assert_eq!(app.reading_panel, None);
        assert_eq!(app.screen, Screen::BookTools);
        assert_eq!(app.page_paint().visible, plain);

        let blank = Input::Tap(450.0, 810.0);
        assert_eq!(app.input(blank), Change::Partial);
        assert_eq!(app.screen, Screen::Book);
        let lower_menu = Input::Tap(WIDTH as f32 / 2.0, HEIGHT as f32 * 0.87);
        assert_eq!(app.input(lower_menu), Change::Partial);
        assert_eq!(app.reading_panel, Some(BarPanel::Text));
        app.input(Input::PageForward);
        if let Some(due) = app.active.as_ref().unwrap().reading.deadline() {
            app.tick(due);
        }
        assert_eq!(app.screen, Screen::Book);
        assert_eq!(app.reading_panel, None);
        assert_eq!(reading(&app).page, before.page + 1);
    }

    #[test]
    fn forced_ocr_selects_a_word_over_embedded_text_and_preserves_native_search() {
        use std::os::unix::fs::PermissionsExt;

        let fixture = Fixture::new();
        let pdf = fixture.root.join("Harbor notes.pdf");
        fs::copy(crate::test_fixture("tilted-scan.pdf"), &pdf).unwrap();
        let mut app = fixture.app();
        app.open_path(&pdf).unwrap();
        {
            let reading = &mut app.fixed.as_mut().unwrap().reading;
            reading.restore_view(FixedPreferences {
                straighten_limit: 10,
                turns: 0,
                crop: Some(PdfTextRect { x0: 0.05, y0: 0.04, x1: 0.94, y1: 0.94 }),
                ..Default::default()
            }).unwrap();
        }
        let reading = &app.fixed.as_ref().unwrap().reading;
        let source = reading.page_text(0).unwrap();
        let word = source.words.iter().find(|w| &source.text[w.bytes.clone()] == "Trail").unwrap().bounds;
        let point = reading.visible_rectangle(0, word).unwrap().center();
        let raster = reading.ocr_raster(0).unwrap();
        // The simulated worker reports a different word at the source rectangle, just as an OCR
        // engine can disagree with an inaccurate invisible embedded text layer.
        let pack = fixture.root.join("engine");
        let tessdata = pack.join("ocr/tessdata");
        fs::create_dir_all(&tessdata).unwrap();
        let real = std::env::var_os("READER_TEST_TESSERACT").zip(std::env::var_os("READER_TEST_TESSDATA"));
        let expected = if let Some((engine, model)) = real {
            fs::copy(engine, pack.join("ocr/tesseract")).unwrap();
            fs::copy(model, tessdata.join("eng.traineddata")).unwrap();
            "Trail"
        } else {
            fs::write(tessdata.join("eng.traineddata"), b"fixture").unwrap();
            let script = format!(
                "#!/bin/sh\ncat >/dev/null\nprintf '%b\\n' 'level\\tpage_num\\tblock_num\\tpar_num\\tline_num\\tword_num\\tleft\\ttop\\twidth\\theight\\tconf\\ttext' '1\\t1\\t0\\t0\\t0\\t0\\t0\\t0\\t{}\\t{}\\t-1\\t' '5\\t1\\t1\\t1\\t1\\t1\\t{}\\t{}\\t{}\\t{}\\t98\\tHarbor'\n",
                raster.width, raster.height,
                (word.x0 * raster.width as f32) as usize, (word.y0 * raster.height as f32) as usize,
                ((word.x1 - word.x0) * raster.width as f32) as usize, ((word.y1 - word.y0) * raster.height as f32) as usize
            );
            fs::write(pack.join("ocr/tesseract"), script).unwrap();
            "Harbor"
        };
        fs::set_permissions(pack.join("ocr/tesseract"), fs::Permissions::from_mode(0o700)).unwrap();
        let capture = |name: &str, app: &Reader| {
            if let (Some(dir), Some(fonts)) = (std::env::var_os("READER_OCR_SHOTS"), std::env::var_os("READER_TEST_FONTS")) {
                let fonts = std::env::split_paths(&fonts).map(|p| p.display().to_string()).collect::<Vec<_>>();
                let mut canvas = Canvas::new(WIDTH, HEIGHT, &fonts).unwrap();
                let dir = Path::new(&dir);
                fs::create_dir_all(dir).unwrap();
                primitive_app::write_png(&dir.join(name), WIDTH, HEIGHT, &primitive_app::frame(app, &mut canvas, false)).unwrap();
            }
        };
        app.ocr_assets = OcrAssets::discover(&pack);
        assert!(app.ocr_assets.is_some());
        app.input(Input::Tap(630.0, 28.0));
        tap_reading_control(&mut app, "Scan");
        assert_eq!(tap_reading_control(&mut app, "Force OCR"), Change::Partial);
        capture("forced-ocr-control.png", &app);
        app.input(Input::Tap(630.0, 720.0));
        if app.screen == Screen::FixedTools { app.input(Input::Tap(630.0, 720.0)); }
        assert_eq!(app.screen, Screen::Fixed);
        let hold = Input::LongTap(point.x as f32, point.y as f32);
        app.input(hold);
        assert!(app.ocr.is_some() && app.pdf_selection.is_none());
        app.input(Input::Tap((CANCEL_OCR.x + 15) as f32, (CANCEL_OCR.y + 15) as f32));
        assert!(app.ocr.is_none());
        assert!(app.fixed.as_ref().unwrap().reading.cached_ocr(0).is_none());
        app.input(hold);
        let deadline = Instant::now() + Duration::from_secs(35);
        while let Some(job) = &app.ocr {
            assert!(Instant::now() < deadline, "OCR selection timed out");
            app.poll_ocr(job.deadline).unwrap();
            std::thread::sleep(Duration::from_millis(12));
        }
        let selected = app.pdf_selection.as_ref().expect("recognized word at original touch");
        assert_eq!(selected.text(), expected);
        assert_eq!(selected.page(), 0);
        capture("forced-ocr-selection.png", &app);
        let reading = &app.fixed.as_ref().unwrap().reading;
        assert!(reading.selection_bounds(selected).unwrap().contains(point.x as f32, point.y as f32));
        let native_search = reading.page_text(0).unwrap();
        assert!(native_search.text.contains("Trail"));
        assert!(!native_search.text.contains("Harbor"));
        app.pdf_selection = None;
        let reading = &mut app.fixed.as_mut().unwrap().reading;
        reading.rotate().unwrap();
        let rotated_point = reading.visible_rectangle(0, word).unwrap().center();
        let hold = Input::LongTap(rotated_point.x as f32, rotated_point.y as f32);
        assert_eq!(app.input(hold), Change::Partial);
        assert!(app.ocr.is_none());
        assert_eq!(app.pdf_selection.as_ref().unwrap().text(), expected);
        app.save().unwrap();
        let mut reopened = fixture.app();
        reopened.open_path(&pdf).unwrap();
        assert!(reopened.fixed.as_ref().unwrap().reading.preferences().force_ocr);
        reopened.start_document_search("Trail").unwrap();
        while reopened.search_at.is_some() {
            reopened.step_pdf_search(Instant::now()).unwrap();
        }
        assert_eq!(reopened.pdf_hits.iter().map(|hit| hit.page).collect::<Vec<_>>(), [0, 1]);
        reopened.start_document_search("Harbor").unwrap();
        while reopened.search_at.is_some() {
            reopened.step_pdf_search(Instant::now()).unwrap();
        }
        assert!(reopened.pdf_hits.is_empty(), "native text takes priority over recognition in search");
        app.activate(Action::ForceOcr(false)).unwrap();
        app.input(hold);
        assert_eq!(app.pdf_selection.as_ref().unwrap().text(), "Trail");
        app.activate(Action::ClearOcr).unwrap();
        assert!(app.fixed.as_ref().unwrap().reading.cached_ocr(0).is_none());
    }

    #[test]
    fn scan_straighten_changes_visible_page_from_bottom_controls_and_reopens() {
        let fixture = Fixture::new();
        let pdf = fixture.root.join("Harbor notes.pdf");
        fs::copy(crate::test_fixture("tilted-scan.pdf"), &pdf).unwrap();
        let fonts: Vec<PathBuf> = std::env::var_os("READER_TEST_FONTS")
            .map(|paths| std::env::split_paths(&paths).collect())
            .unwrap_or_else(|| vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")]);
        let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
        let fonts = fonts.iter().map(|p| p.display().to_string()).collect::<Vec<_>>();
        let mut canvas = Canvas::new(WIDTH, HEIGHT, &fonts).unwrap();
        let mut capture = |name: &str, app: &Reader| {
            let pixels = primitive_app::frame(app, &mut canvas, false);
            if let Some(dir) = std::env::var_os("READER_DESKEW_SHOTS") {
                let dir = Path::new(&dir);
                fs::create_dir_all(dir).unwrap();
                primitive_app::write_png(&dir.join(name), WIDTH, HEIGHT, &pixels).unwrap();
            }
            pixels
        };
        app.open_path(&pdf).unwrap();
        let before = capture("scan-original.png", &app);
        app.input(Input::Tap(630.0, 28.0));
        tap_reading_control(&mut app, "Scan");
        assert_eq!(tap_reading_control(&mut app, "10°"), Change::Partial);
        assert_eq!(app.reading_panel, Some(BarPanel::Scan));
        assert_eq!(tap_reading_control(&mut app, "Increase 1°"), Change::Partial);
        assert_eq!(tap_reading_control(&mut app, "Decrease 1°"), Change::Partial);
        capture("scan-control.png", &app);
        app.input(Input::Tap(630.0, 720.0));
        if app.screen == Screen::FixedTools { app.input(Input::Tap(630.0, 720.0)); }
        assert_eq!(app.screen, Screen::Fixed);
        let after = capture("scan-straightened.png", &app);
        assert!(before[..(WIDTH * PAGE_HEIGHT) as usize] != after[..(WIDTH * PAGE_HEIGHT) as usize]);
        assert_eq!(app.input(Input::Tap((DONE.x + 30) as f32, (DONE.y + 24) as f32)), Change::Exit);
        let mut reopened = fixture.app();
        reopened.activate(Action::Continue).unwrap();
        let fixed = &reopened.fixed.as_ref().unwrap().reading;
        assert_eq!(fixed.preferences().straighten_limit, 10);
        assert_eq!(fixed.page(), 0);
    }

    #[test]
    fn search_and_saved_highlights_reveal_the_same_words_with_location_history() {
        let fixture = Fixture::new();
        let path = fixture.root.join("Signal chart.pdf");
        let content = concat!(
            "BT /F1 14 Tf 30 775 Td (Harbor light) Tj ET ",
            "BT /F1 16 Tf 410 180 Td (East pier) Tj ET ",
            "BT /F1 11 Tf 410 151 Td (The channel turns north) Tj 0 -15 Td (past the outer pier.) Tj ET ",
            "BT /F1 14 Tf 430 72 Td (Harbor light) Tj ET ",
            "BT /F1 10 Tf 410 49 Td (Two flashes every ten seconds.) Tj ET"
        );
        let objects = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
            "<< /Type /Pages /Count 2 /Kids [3 0 R 4 0 R] /MediaBox [0 0 600 800] >>".to_owned(),
            "<< /Type /Page /Parent 2 0 R >>".to_owned(),
            "<< /Type /Page /Parent 2 0 R /Contents 5 0 R /Resources << /Font << /F1 6 0 R >> >> >>".to_owned(),
            format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_owned(),
        ];
        fs::write(&path, crate::fixed::tests::pdf_objects(&objects)).unwrap();
        let mut app = fixture.app();
        app.open_path(&path).unwrap();
        app.fixed.as_mut().unwrap().reading.set_zoom(4).unwrap();
        app.start_document_search("Harbor light").unwrap();
        while app
            .pdf_search
            .as_ref()
            .is_some_and(|search| search.next_page < 2)
        {
            app.step_pdf_search(Instant::now()).unwrap();
        }
        assert_eq!(app.pdf_hits.len(), 2);
        app.activate(Action::PdfSearchHit(1)).unwrap();
        let hit = app.pdf_hits[1].bounds.clone();
        assert_eq!(hit.len(), 2);
        assert_eq!(app.pdf_result_regions, vec![(1, hit[0]), (1, hit[1])]);
        let fixed = app.fixed.as_ref().unwrap();
        let bottom = fixed.reading.preferences();
        assert_eq!(fixed.reading.page(), 1);
        assert!(
            hit.iter()
                .all(|&r| fixed.reading.visible_rectangle(1, r).is_some())
        );
        capture_test_screen(
            &app,
            &[crate::test_fixture("fonts/NotoSans-Regular.ttf")],
            "READER_PDF_SEARCH_TARGET",
            false,
        );
        let top = app.pdf_hits[0].bounds.clone();
        app.library
            .remember(&fixed.entry, fixed.id, "Signal chart")
            .add_pdf_highlight(PdfHighlight {
                color: Default::default(),
                created_at: None,
                style: Default::default(),
                continuation: Vec::new(),
                page: 1,
                bounds: top.clone(),
                text: "Harbor light".into(),
                note: None,
            })
            .unwrap();
        app.activate(Action::PdfHighlight(0)).unwrap();
        let fixed = app.fixed.as_ref().unwrap();
        let saved = fixed.reading.preferences();
        assert_ne!(bottom, saved);
        assert!(
            top.iter()
                .all(|&r| fixed.reading.visible_rectangle(1, r).is_some())
        );
        app.activate(Action::ReturnToLocation).unwrap();
        assert_eq!(app.fixed.as_ref().unwrap().reading.preferences(), bottom);
        app.next_location().unwrap();
        assert_eq!(app.fixed.as_ref().unwrap().reading.preferences(), saved);
        app.save().unwrap();
        let mut reopened = fixture.app();
        reopened.open_path(&path).unwrap();
        assert_eq!(
            reopened.fixed.as_ref().unwrap().reading.preferences(),
            saved
        );

        // This phrase begins on the second line of the chart and wraps back
        // to the left. Both pairs of search marks fit even at 300% zoom.
        for continuous in [false, true] {
            let reading = &mut reopened.fixed.as_mut().unwrap().reading;
            reading.set_continuous(continuous).unwrap();
            reading.set_zoom(4).unwrap();
            reading.jump(0).unwrap();
            reopened.start_document_search("turns north past the").unwrap();
            while reopened.search_at.is_some() {
                reopened.step_pdf_search(Instant::now()).unwrap();
            }
            assert_eq!(reopened.pdf_hits.len(), 1);
            reopened.activate(Action::PdfSearchHit(0)).unwrap();
            let words = &reopened.pdf_hits[0].bounds;
            assert_eq!(words.len(), 4);
            assert!(words[2].y0 > words[1].y0 && words[2].x0 < words[1].x0);
            let reading = &reopened.fixed.as_ref().unwrap().reading;
            assert_eq!(reading.page(), 1);
            assert_eq!(reading.is_continuous(), continuous);
            assert_eq!(
                reopened.pdf_result_regions,
                words.iter().map(|&bounds| (1, bounds)).collect::<Vec<_>>()
            );
            let (size, view) = reading.page_viewport().unwrap();
            for &word in words {
                let rect = reading.visible_rectangle(1, word).unwrap();
                let width = (word.x1 - word.x0) * size.0 * view.scale;
                let height = (word.y1 - word.y0) * size.1 * view.scale;
                assert!((rect.width() - f64::from(width)).abs() < 1.0);
                assert!((rect.height() - f64::from(height)).abs() < 1.0);
            }
            if !continuous {
                capture_test_screen(
                    &reopened,
                    &[crate::test_fixture("fonts/NotoSans-Regular.ttf")],
                    "READER_PDF_WRAPPED_SEARCH",
                    false,
                );
            }
            reopened.activate(Action::ReturnToLocation).unwrap();
            assert_eq!(reopened.fixed.as_ref().unwrap().reading.page(), 0);
        }
    }

    #[test]
    fn tapping_a_zoomed_pdf_link_restores_its_destination_on_forward_and_reopen() {
        let fixture = Fixture::new();
        let path = fixture.root.join("Harbor.pdf");
        let content = concat!(
            "BT /F1 16 Tf 60 655 Td (Harbor approach) Tj ",
            "/F1 11 Tf 0 -23 Td (Follow the shore until the first) Tj ",
            "0 -15 Td (white buoy comes into view.) Tj ",
            "0 -23 Td (At low tide the sand bank) Tj ",
            "0 -15 Td (extends past the old pier.) Tj ",
            "0 -15 Td (Keep the buoy on your left.) Tj ET"
        );
        let objects = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
            "<< /Type /Pages /Count 2 /Kids [3 0 R 4 0 R] /MediaBox [0 0 600 800] >>".to_owned(),
            "<< /Type /Page /Parent 2 0 R /Annots [5 0 R] >>".to_owned(),
            "<< /Type /Page /Parent 2 0 R /Contents 6 0 R /Annots [8 0 R] /Resources << /Font << /F1 7 0 R >> >> >>".to_owned(),
            "<< /Subtype /Link /Rect [270 380 330 420] /Dest [4 0 R /XYZ 60 680 null] >>".to_owned(),
            format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_owned(),
            "<< /Subtype /Link /Rect [60 644 150 670] /Dest [4 0 R /XYZ 490 200 null] >>".to_owned(),
        ];
        fs::write(&path, crate::fixed::tests::pdf_objects(&objects)).unwrap();
        let mut app = fixture.app();
        app.open_path(&path).unwrap();
        app.fixed.as_mut().unwrap().reading.set_zoom(4).unwrap();
        let before = app.fixed.as_ref().unwrap().reading.preferences();
        assert_eq!(
            app.input(Input::Tap(WIDTH as f32 / 2.0, PAGE_HEIGHT as f32 / 2.0)),
            Change::Partial
        );
        let target = &app.fixed.as_ref().unwrap().reading;
        assert_eq!(target.page(), 1);
        let (size, request) = target.page_viewport().unwrap();
        let pixel = request.page_to_viewport(size).unwrap() * hayro::kurbo::Point::new(60.0, 120.0);
        assert!(pixel.x.abs() < 1.0 && pixel.y.abs() < 1.0);
        let after = target.preferences();
        capture_test_screen(
            &app,
            &[crate::test_fixture("fonts/NotoSans-Regular.ttf")],
            "READER_PDF_LINK_SCREENSHOT",
            false,
        );
        let second_link =
            request.page_to_viewport(size).unwrap() * hayro::kurbo::Point::new(100.0, 145.0);
        app.input(Input::Tap(second_link.x as f32, second_link.y as f32));
        assert_eq!(app.fixed.as_ref().unwrap().reading.page(), 1);
        assert_ne!(app.fixed.as_ref().unwrap().reading.preferences(), after);
        app.activate(Action::ReturnToLocation).unwrap();
        assert_eq!(app.fixed.as_ref().unwrap().reading.preferences(), after);
        app.activate(Action::ReturnToLocation).unwrap();
        assert_eq!(app.fixed.as_ref().unwrap().reading.page(), 0);
        assert_eq!(app.fixed.as_ref().unwrap().reading.preferences(), before);
        app.next_location().unwrap();
        assert_eq!(app.fixed.as_ref().unwrap().reading.preferences(), after);
        app.save().unwrap();
        let mut reopened = fixture.app();
        reopened.open_path(&path).unwrap();
        assert_eq!(reopened.fixed.as_ref().unwrap().reading.page(), 1);
        assert_eq!(
            reopened.fixed.as_ref().unwrap().reading.preferences(),
            after
        );
    }

    #[test]
    fn pdf_reading_bars_route_view_scan_and_navigation() {
        let fixture = Fixture::new();
        let pdf = fixture.root.join("Reading sample.pdf");
        fs::write(&pdf, crate::fixed::tests::document()).unwrap();
        let mut app = fixture.app();
        app.open_path(&pdf).unwrap();
        let open = Input::Tap(WIDTH as f32 / 2.0, 23.0);
        assert_eq!(app.input(open), Change::Partial);
        assert_eq!(app.screen, Screen::FixedTools);
        assert_eq!(app.reading_panel, None);
        assert_eq!(tap_reading_control(&mut app, "View"), Change::Partial);
        assert_eq!(app.reading_panel, Some(BarPanel::View));
        assert_eq!(tap_reading_control(&mut app, "Fit width"), Change::Partial);
        assert_eq!(app.screen, Screen::FixedTools);
        assert_eq!(app.reading_panel, Some(BarPanel::View));
        assert!(app.fixed.as_ref().unwrap().reading.preferences().width_fit);

        assert_eq!(tap_reading_control(&mut app, "Scan"), Change::Partial);
        assert_eq!(app.reading_panel, Some(BarPanel::Scan));
        assert_eq!(
            tap_reading_control(&mut app, "Higher contrast"),
            Change::Partial
        );
        assert!(app.fixed.as_ref().unwrap().reading.scan_contrast());

        app.input(Input::PageForward);
        let deadline = app.fixed.as_ref().unwrap().reading.deadline();
        if let Some(due) = deadline {
            app.tick(due);
        }
        assert_eq!(app.screen, Screen::Fixed);
        assert_eq!(app.reading_panel, None);
        assert_eq!(app.fixed.as_ref().unwrap().reading.page(), 1);
        assert_eq!(app.input(open), Change::Partial);
        assert_eq!(tap_reading_control(&mut app, "Bookmarks"), Change::Partial);
        assert_eq!(app.screen, Screen::FixedBookmarks);
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
        assert_eq!(tap_reading_control(&mut app, "Exit"), Change::Exit);
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

    /// Compare selected words in real publications with an independent, exact
    /// lookup from the stock Kindle KOReader sdcv. The reference file is private.
    #[test]
    #[ignore = "Supply READER_DICTIONARY_REFERENCE and an existing documents directory"]
    fn selected_words_match_installed_koreader_dictionaries() {
        #[derive(serde::Deserialize)]
        struct Reference {
            expected_count: usize,
            cases: Vec<Lookup>,
        }
        #[derive(serde::Deserialize)]
        struct Lookup {
            book: PathBuf,
            chapter: usize,
            word: String,
            // Dictionary title, returned headword and optional digest of visible definition text.
            results: Vec<(String, String, Option<String>)>,
        }
        let reference: Reference = serde_json::from_slice(&fs::read(
            std::env::var_os("READER_DICTIONARY_REFERENCE").unwrap()
        ).unwrap()).unwrap();
        let documents = PathBuf::from(std::env::var_os("PRIMITIVE_READER_DOCUMENTS").unwrap());
        let fixture = Fixture::new();
        let font = crate::test_fixture("fonts/NotoSans-Regular.ttf");
        let mut app = Reader::open(&documents, &fixture.state, vec![font]).unwrap();
        app.activate(Action::ShowDictionaries).unwrap();
        assert_eq!(app.installed_dictionaries.len(), reference.expected_count);
        let mut looked_up = 0;
        for case in reference.cases {
            app.open_path(&case.book).unwrap();
            let probe = app.active.as_ref().unwrap().reading.config.chapter(case.chapter).unwrap();
            let passage = probe.source.blocks.iter().find_map(|b| {
                let range = crate::selection::word_ranges(&b.text)
                    .find(|range| b.text[range.clone()] == case.word)?;
                probe.select_range(&probe.anchor(b.key, range.start), &probe.anchor(b.key, range.end))
            }).expect("Reference word occurs in the specified chapter");
            app.restore_position(&ReadingPosition {
                chapter: case.chapter, page: 0, anchor: Some(passage.start.clone())
            }).unwrap();
            let origin = app.current_history_location().unwrap();
            let bounds = app.active.as_ref().unwrap().reading.highlight_rects(&passage)[0];
            let (x, y) = rectangle_center(Rect::new(bounds.x, bounds.y, bounds.width, bounds.height));
            app.input(Input::LongTap(x, y));
            assert!(app.selection.as_ref().is_some_and(|selected| selected.text == case.word));
            let start = Instant::now();
            tap_selection_action(&mut app, selection_popup::SelectionAction::Dictionary);
            assert_eq!(app.screen, Screen::Definition);
            assert!(app.definition_over_book());
            let deadline = Instant::now() + Duration::from_secs(180);
            while app.spelling_lookup.is_some() {
                assert!(Instant::now() < deadline, "Dictionary suggestions exceeded three minutes");
                app.tick(Instant::now());
                if app.spelling_lookup.is_some() { std::thread::sleep(Duration::from_millis(20)); }
            }
            assert_eq!(app.definitions.len(), case.results.len(), "dictionary result count for case {looked_up}");
            let mut remaining: Vec<_> = app.definitions.iter().collect();
            for (dictionary, word, visible_digest) in case.results {
                let index = remaining.iter().position(|entry| {
                    if entry.dictionary != dictionary || entry.word != word {
                        return false;
                    }
                    visible_digest.as_ref().is_none_or(|digest| {
                        let normalized = entry.text.split_whitespace().collect::<Vec<_>>().join(" ");
                        format!("{:x}", md5::compute(normalized)) == *digest
                    })
                }).expect("Same dictionary, headword and visible definition as the stock result");
                remaining.remove(index);
            }
            eprintln!("Dictionary check {} took {} ms", looked_up + 1, start.elapsed().as_millis());
            let (x, y) = rectangle_center(app.definition_layout().1);
            app.input(Input::Tap(x, y));
            assert_eq!(app.screen, Screen::Book);
            assert!(app.current_history_location().as_ref() == Some(&origin));
            assert!(app.selection.as_ref().is_some_and(|selected| selected.text == case.word));
            app.selection = None;
            looked_up += 1;
        }
        assert!(looked_up > 0);
    }

    fn rectangle_center(rect: Rect) -> (f32, f32) {
        ((rect.x + rect.w / 2) as f32, (rect.y + rect.h / 2) as f32)
    }

    fn tap_selection_action(app: &mut Reader, action: selection_popup::SelectionAction) -> Change {
        let (x, y) = rectangle_center(app.selection_button(action));
        app.input(Input::Tap(x, y))
    }

    #[test]
    fn epub_selection_popup_preserves_extension_dictionary_and_exit() {
        use selection_popup::SelectionAction;

        let fixture = Fixture::new();
        let mut app = fixture.app();
        app.open_path(&fixture.book).unwrap();
        let reading = &app.active.as_ref().unwrap().reading;
        let target = Book::resolve_link(&reading.probe.chapter, "#tide-note").unwrap();
        app.follow(target).unwrap();
        let reading = &app.active.as_ref().unwrap().reading;
        let origin = reading.probe.fragment("tide-note").unwrap();
        let point = |offset: usize, len: usize| {
            let mut start = origin.clone();
            start.offset += offset;
            let mut end = start.clone();
            end.offset += len;
            let source = reading.probe.select_range(&start, &end).unwrap();
            let rect = reading.highlight_rects(&source)[0];
            rectangle_center(Rect::new(rect.x, rect.y, rect.width, rect.height))
        };
        let first = point(0, "The".len());
        let next = point("The ".len(), "first".len());
        let id = app.active.as_ref().unwrap().id;
        let saved = position(reading);
        assert_eq!(app.input(Input::LongTap(first.0, first.1)), Change::Partial);
        assert_eq!(app.selection.as_ref().unwrap().text, "The");
        assert_eq!(app.input(Input::Tap(next.0, next.1)), Change::Partial);
        assert_eq!(app.selection.as_ref().unwrap().text, "The first");
        assert_eq!(
            tap_selection_action(&mut app, SelectionAction::Cancel),
            Change::Partial
        );
        assert!(app.selection.is_none());

        app.input(Input::LongTap(first.0, first.1));
        assert_eq!(
            tap_selection_action(&mut app, SelectionAction::Dictionary),
            Change::Partial
        );
        assert_eq!(app.screen, Screen::Definition);
        let (x, y) = rectangle_center(app.definition_layout().1);
        assert_eq!(app.input(Input::Tap(x, y)), Change::Partial);
        assert_eq!(app.screen, Screen::Book);
        assert_eq!(app.selection.as_ref().unwrap().text, "The");
        assert_eq!(
            tap_selection_action(&mut app, SelectionAction::Highlight),
            Change::Partial
        );
        assert_eq!(
            app.library.state(id).unwrap().highlights[0].selection.text,
            "The"
        );
        assert_eq!(position(&app.active.as_ref().unwrap().reading), saved);

        app.input(Input::LongTap(first.0, first.1));
        let (x, y) = rectangle_center(DONE);
        assert_eq!(app.input(Input::Tap(x, y)), Change::Exit);
        let mut reopened = fixture.app();
        reopened.activate(Action::Continue).unwrap();
        assert_eq!(position(&reopened.active.as_ref().unwrap().reading), saved);
    }

    #[test]
    fn searches_from_a_selection_on_reflow_and_rotated_pdf_pages() {
        let fixture = Fixture::new();
        let fonts = simulation_fonts();
        let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
        app.open_path(&fixture.book).unwrap();
        let reading = &app.active.as_ref().unwrap().reading;
        let target = Book::resolve_link(&reading.probe.chapter, "#tide-note").unwrap();
        app.follow(target).unwrap();
        let reading = &app.active.as_ref().unwrap().reading;
        let start = reading.probe.fragment("tide-note").unwrap();
        let mut end = start.clone();
        end.offset += "The first".len();
        app.selection = reading.probe.select_range(&start, &end);
        let original = position(reading);
        capture_test_screen(&app, &fonts, "READER_SELECTION_FIND_SCREENSHOT", false);
        assert_eq!(tap_selection_action(&mut app, selection_popup::SelectionAction::Find), Change::Partial);
        assert_eq!(app.screen, Screen::Matches);
        assert_eq!(app.editor.string(), "The first");
        assert!(app.selection.is_none());
        wait_for_search(&mut app, usize::MAX);
        assert!(
            app.hits
                .iter()
                .any(|hit| hit.anchor == start && hit.end == end)
        );
        app.activate(Action::SearchHit(0)).unwrap();
        app.activate(Action::ReturnToLocation).unwrap();
        assert_eq!(position(&app.active.as_ref().unwrap().reading), original);

        let pdf = fixture.root.join("Search selection.pdf");
        fs::write(&pdf, crate::fixed::tests::document()).unwrap();
        app.open_path(&pdf).unwrap();
        app.activate(Action::RotatePage).unwrap();
        let reading = &app.fixed.as_ref().unwrap().reading;
        let text = reading.page_text(0).unwrap();
        let (size, viewport) = reading.page_viewport().unwrap();
        let word = &text.words[0];
        let expected = text.text[word.bytes.clone()].to_owned();
        let point = word.bounds.in_viewport(size, viewport).unwrap().center();
        app.input(Input::LongTap(point.x as f32, point.y as f32));
        assert_eq!(tap_selection_action(&mut app, selection_popup::SelectionAction::Find), Change::Partial);
        assert_eq!(app.screen, Screen::Matches);
        app.step_pdf_search(Instant::now()).unwrap();
        assert!(
            app.pdf_hits
                .iter()
                .any(|hit| hit.page == 0 && !hit.bounds.is_empty())
        );
        assert_eq!(app.lookup_history.searches[0].text, expected);
        assert!(app.pdf_selection.is_none());
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
        app.step_pdf_search(Instant::now()).unwrap();
        if let Some(due) = app.search_at {
            app.tick(due);
        }
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
        assert_eq!(
            tap_selection_action(&mut app, selection_popup::SelectionAction::Note),
            Change::Partial,
        );
        assert_eq!(app.screen, Screen::Note);
        app.editor.text = "Read again".chars().collect();
        let save = Layout::new(WIDTH, HEIGHT, 4, 5)
            .keyboard_keys_with(app.editor.layer, "Save")
            .into_iter()
            .find(|(key, _)| key.key == primitive_ui::Key::Go)
            .unwrap()
            .1;
        let (x, y) = rectangle_center(save);
        assert_eq!(app.input(Input::Tap(x, y)), Change::Partial);
        assert_eq!(app.screen, Screen::Fixed);
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
    fn advances_an_original_pixel_scan_reflow_only_across_recognized_pages() {
        let fixture = Fixture::new();
        let pdf = fixture.root.join("Scan sample.pdf");
        fs::write(&pdf, crate::fixed::tests::document()).unwrap();
        let mut app = fixture.app();
        app.open_path(&pdf).unwrap();
        assert!(app.activate(Action::ReflowScan).is_err());
        let fixed = &mut app.fixed.as_mut().unwrap().reading;
        let mut text = fixed.page_text(0).unwrap();
        text.words.truncate(1);
        text.lines.clear();
        text.text.truncate(text.words[0].bytes.end);
        fixed.remember_ocr(0, "eng", text).unwrap();

        app.activate(Action::ReflowScan).unwrap();
        assert_eq!(app.screen, Screen::ReadFixedText);
        assert_eq!(
            app.input(Input::Tap(1030.0, PAGE_HEIGHT as f32 + 30.0)),
            Change::Partial
        );
        assert_eq!(app.text_flow.as_ref().unwrap().document_page(), 0);
        app.input(Input::PageForward);
        if let Some(due) = app.text_flow.as_ref().and_then(TextFlow::deadline) {
            app.tick(due);
        }
        assert_eq!(app.text_flow.as_ref().unwrap().document_page(), 0);
        assert_eq!(app.fixed.as_ref().unwrap().reading.page(), 0);
        assert_eq!(
            app.input(Input::Tap((DONE.x + 10) as f32, (DONE.y + 10) as f32)),
            Change::Partial
        );
        assert_eq!(app.screen, Screen::Fixed);

        let fixed = &mut app.fixed.as_mut().unwrap().reading;
        let mut text = fixed.page_text(1).unwrap();
        text.words.truncate(1);
        text.lines.clear();
        text.text.truncate(text.words[0].bytes.end);
        fixed.remember_ocr(1, "eng", text).unwrap();
        app.activate(Action::ReflowScan).unwrap();
        app.input(Input::PageForward);
        if let Some(due) = app.text_flow.as_ref().and_then(TextFlow::deadline) {
            app.tick(due);
        }
        assert_eq!(app.text_flow.as_ref().unwrap().document_page(), 1);
        assert_eq!(app.fixed.as_ref().unwrap().reading.page(), 1);
    }

    #[test]
    fn protected_pdf_prompt_search_and_resume_preserve_both_location_and_secret() {
        let fixture = Fixture::new();
        let bytes = include_bytes!("../tests/fixtures/pdf-password/r6-long.pdf");
        let path = fixture.root.join("Weather survey.pdf");
        fs::write(&path, bytes).unwrap();
        let font = crate::test_fixture("fonts/NotoSans-Regular.ttf");
        let fonts = vec![font.clone()];
        let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
        app.open_path(&path).unwrap();
        assert_eq!(app.screen, Screen::PdfPassword);
        let press = |app: &mut Reader, target: primitive_ui::Key| {
            let keys =
                Layout::new(WIDTH, HEIGHT, 4, 5).keyboard_keys_with(app.editor.layer, "Open");
            let (_, rect) = keys.into_iter().find(|(key, _)| key.key == target).unwrap();
            let (x, y) = rectangle_center(rect);
            assert_eq!(app.input(Input::Tap(x, y)), Change::Partial);
        };
        for c in "wrong".chars() {
            press(&mut app, primitive_ui::Key::Char(c));
        }
        press(&mut app, primitive_ui::Key::Go);
        assert_eq!(app.screen, Screen::PdfPassword);
        assert!(app.notice.as_deref().unwrap().contains("Password rejected"));
        let secret = format!("estuary{}river", "n".repeat(88));
        for (i, c) in secret.chars().enumerate() {
            press(&mut app, primitive_ui::Key::Char(c));
            if i == 6 {
                capture_test_screen(&app, &fonts, "READER_PROTECTED_PDF_SCREENSHOT", false);
            }
        }
        assert_eq!(app.editor.string().len(), secret.len());
        press(&mut app, primitive_ui::Key::Go);
        assert_eq!(app.screen, Screen::Fixed);
        assert!(app.editor.string().is_empty());
        assert!(app.fixed.as_ref().unwrap().reading.is_encrypted_pdf());
        let opening = app.current_position().unwrap();
        app.start_document_search("Wind survey").unwrap();
        while app.search_at.is_some() {
            app.step_pdf_search(Instant::now()).unwrap();
        }
        assert_eq!(app.pdf_hits.len(), 1);
        app.activate(Action::PdfSearchHit(0)).unwrap();
        assert_eq!(app.current_position().unwrap().page, 1);
        assert!(!app.pdf_result_regions.is_empty());
        app.activate(Action::ReturnToLocation).unwrap();
        assert!(app.current_position().as_ref() == Some(&opening));
        app.input(Input::PageForward);
        app.tick(Instant::now() + Duration::from_secs(1));
        let saved = app.current_position().unwrap();
        assert_eq!(saved.page, 1);
        app.save().unwrap();
        drop(app);
        let mut reopened = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
        reopened.open_path(&path).unwrap();
        assert_eq!(reopened.screen, Screen::PdfPassword);
        let entry = reopened.locked.clone().unwrap();
        reopened.open_with_password(entry, Some(&secret)).unwrap();
        assert!(reopened.current_position().as_ref() == Some(&saved));
        assert_eq!(fs::read(&path).unwrap().as_slice(), bytes);
        assert!(
            !fs::read_to_string(&fixture.state)
                .unwrap()
                .contains(&secret)
        );
    }

    #[test]
    fn protected_pdf_with_an_empty_reading_password_leaves_local_annotations_available() {
        let fixture = Fixture::new();
        let path = fixture.root.join("Weather survey.pdf");
        fs::write(
            &path,
            include_bytes!("../tests/fixtures/pdf-password/r4-owner-only.pdf"),
        )
        .unwrap();
        let mut app = fixture.app();
        app.open_path(&path).unwrap();
        assert_eq!(app.screen, Screen::Fixed);
        assert!(app.fixed.as_ref().unwrap().reading.is_encrypted_pdf());
        app.show(Screen::Highlights);
        assert!(!app.items().iter().any(|item| matches!(
            item.action,
            Action::PreviewPdfAnnotations | Action::ExportAnnotatedPdf
        )));
        assert!(
            app.verify_pdf_edit_source(app.fixed.as_ref().unwrap())
                .unwrap_err()
                .to_string()
                .contains("unencrypted")
        );
        app.show(Screen::Fixed);
        app.activate(Action::ToggleBookmark).unwrap();
        assert!(app.page_is_bookmarked());
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
                {"text":"The copper lantern stood beside the door.","note":"Check the lantern","time":1750000001},
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
        assert_eq!(highlights[0].created_at, Some(1_750_000_001));
        app.save().unwrap();
        assert_eq!(fixture.app().library.state(id).unwrap().highlights[0].created_at, Some(1_750_000_001));
    }
}
