//! The daily reader interface and foreground lifecycle.
mod alternative_contents;
mod annotation_exchange;
mod annotation_search;
mod bar_icons;
pub(crate) const NOTE_MARK_ICON: primitive_ui::Icon = bar_icons::HIGHLIGHT;
#[cfg(test)]
mod base_stylesheet_tests;
mod book_archive;
mod book_shortcuts;
mod bookmark_browser;
mod chapter_navigation;
mod character_page_settings;
mod content_dpi;
mod contents_density;
mod contents_position;
mod contents_tree;
mod contents_view;
mod custom_contents;
mod line_spacing;
mod list_density;
mod logical_font;
#[cfg(test)]
mod page_overlap_tests;
pub(crate) mod publisher_pages;
mod reading_history;
mod reading_notifications;
mod saved_marks_view;
pub(crate) mod scrolling;
mod search_presentation;
mod search_results_navigation;
mod word_spacing;
use character_page_settings::NumberField;
mod cjk_selection;
mod default_settings;
mod dictionary_panel;
use dictionary_panel::buttons as dictionary_buttons;
mod dictionary_presets;
mod dictionary_sources;
use dictionary_sources::{SourceAction, SourcePreferences};
pub(crate) mod back_navigation;
#[cfg(test)]
mod backup_defaults_integration_tests;
pub(crate) mod eager_typography;
mod embedded_fonts;
mod end_of_book;
#[cfg(test)]
mod epub_gif_tests;
mod export_settings;
mod extend_highlight;
mod external_link;
mod fixed_book_map;
#[cfg(test)]
mod fixed_results_tests;
mod flexible_results;
#[cfg(test)]
mod font_render_tests;
mod footer_custom_text;
mod footer_font_face;
mod footer_presets;
mod footnote_font;
mod footnote_popup;
mod gestures;
mod go_to_page;
mod hidden_flows;
mod highlight_palette;
mod illustration;
mod library_export;
mod library_view;
mod link_taps;
mod live_metadata;
mod lookup_history;
mod mark_chapters;
mod menu_search;
mod metadata_archive;
mod metadata_save;
mod navigation_layout;
mod navigation_preferences;
mod nearby_search;
#[cfg(test)]
mod nonlinear_tests;
mod numeric_editor;
mod online_panel;
mod overview_bookmarks;
mod page_overview;
pub(crate) mod page_turn_settings;
#[cfg(test)]
mod pdf_password_viewport_tests;
mod profile_panel;
mod progress_markers;
mod publisher_render_settings;
mod quick_display;
mod quickstart;
mod read_alarm;
mod reader_timer;
mod reading_backup;
mod reading_bars;
mod reading_footer;
pub(crate) mod reading_guides;
mod reading_orientation;
mod reading_viewport_stage;
mod result_context;
mod saved_highlight_popup;
mod search_chapter_scope;
mod search_layout;
mod selection_html;
mod selection_popup;
#[cfg(test)]
mod selection_viewport_tests;
mod semi_auto_crop;
mod sidecar_marks;
mod skim;
mod step_buttons;
mod style_tweaks;
mod top_config;
mod top_header;
mod top_header_font;
mod typeface;
pub(crate) mod typography_settings;
mod user_hyphenation;
mod visual_navigation;
mod vocabulary;
#[cfg(test)]
mod workflow_integration_tests;

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use primitive_app::{App, Change, HEIGHT, WIDTH, with_storage};
use primitive_ui::{
    BODY, CAPTION, Canvas, Editor, Input, Layout, Press, Rect, Row, RowAccessory, Rows, header,
    keyboard_key,
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
    BookMetadata, BookPreferences, BookState, Bookmark, DocumentFormat, FixedPreferences,
    FixedTileAxis, FolderDirection, FolderOrder, Highlight, Library, LibraryBook, PdfHighlight,
    ReadingPosition, ReadingStatus,
};
use crate::ocr_job::{OcrAssets, OcrJob};
use crate::pdf_annotations;
use crate::reading_controls::{
    AnnotationListText, HighlightPrompt, MenuActivation, ReadingControls, ReadingOrientation,
    SelectionLongPressAction, SystemTheme,
};
use crate::reading_fonts;
use crate::reading_profiles::{self, Profile};
use crate::reading_stats::ReadingStats;
use crate::search::{MAX_FIXED_SEARCH_HITS, PdfSearch, PdfSearchHit, Search, SearchHit};
use crate::selection::TextSelection;
#[cfg(test)]
use crate::session::DONE;
use crate::session::screen_pages::background::Intent as ScreenPageIntent;
use crate::session::{BookConfig, PAGE_HEIGHT, PAGE_REFRESH_GAP, PageTurns, Reading};
use crate::text_flow::TextFlow;
use crate::{Engine, Probe, Result, Settings, TextAlignment};

use self::custom_contents::CustomAction;
use self::footer_presets::Preset as FooterPreset;
use self::gestures::{Binding, Gesture, GestureBindings, multiswipes};
use self::library_view::{FileBrowser, file_type};
use self::lookup_history::{LookupHistory, SearchQuery};
use self::page_overview::PageOverview;
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
const OCR_POLL: Duration = Duration::from_millis(120);
const AUTO_CROP: Rect = Rect::new(674, PAGE_HEIGHT as i32 + 5, 182, 60);
const RESET_CROP: Rect = Rect::new(868, PAGE_HEIGHT as i32 + 5, 172, 60);
const CANCEL_CROP: Rect = Rect::new(1052, PAGE_HEIGHT as i32 + 5, 192, 60);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Screen {
    Footnote,
    Library,
    LibraryFind,
    FolderOrder,
    OrganizeLibrary,
    BookArchive,
    ArchiveFolder,
    MetadataArchive(metadata_archive::Page),
    BookShortcuts(book_shortcuts::Page),
    LiveMetadata(live_metadata::Page),
    BookmarkBrowser(bookmark_browser::Page),
    ReadingHistory(reading_history::Page),
    BookList,
    BookDetails,
    BookIdentityEdit(top_header::IdentityField),
    CollectionName,
    RemoveCollection,
    ReadingControls,
    ReadingNotifications,
    PastNotifications,
    NavigationSettings,
    PageTurns,
    PageTurnZones,
    PageTurnSizes(page_turn_settings::Sizes),
    PageTurnDefaults,
    BackOptions(back_navigation::Options),
    ReadingGuides,
    ReadingGuidesAbout,
    BookmarkListSettings,
    AnnotationExchange,
    AnnotationExchangePreview,
    AnnotationListSettings,
    SelectionPosition,
    LongPressSettings,
    VeryLongPressSettings,
    HighlightPromptSettings,
    FooterSettings,
    FooterItemWidths,
    HeaderBatterySettings,
    FooterFontFaces,
    FooterCustomText,
    FooterCustomTextGuide,
    FooterPresets,
    FooterPreset(usize),
    FooterPresetDelete(usize),
    FooterPresetName(FooterPresetName),
    EndActionSettings,
    MetadataSaveSettings,
    HiddenFlows,
    ChapterNavigation,
    Gestures,
    MenuActivationSettings,
    Scrolling,
    GestureBinding(Gesture),
    Multiswipes,
    CustomMultiswipes,
    RecordMultiswipe,
    MultiswipeBinding(multiswipes::Shape),
    RemoveMultiswipe(multiswipes::Shape),
    GestureOverview,
    ReadingData,
    LibraryExport(library_export::Page),
    ClearOcr,
    RestoreBackup,
    ImportAnnotations,
    ImportPosition,
    ImportPdfAnnotations,
    ExportAnnotations,
    SelectionHtml,
    ExportSettings(export_settings::Origin, export_settings::Page),
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
    Skim,
    BookMap,
    AutoTurn,
    AutoTurnDistance,
    AutoTurnPan,
    LocationHistory,
    BookTools,
    Contents,
    ContentsSearch,
    ContentsSettings,
    GeneratedContents,
    CustomContents,
    CustomTitle,
    Hyphenate,
    CustomEdits,
    CustomBookmarks,
    Links,
    LinkSettings,
    FootnoteFontSize(footnote_font::Mode),
    Bookmarks,
    Typography,
    ReadingFonts,
    ReadingProfiles,
    DocumentDefaults,
    FontRendering(eager_typography::Scope),
    PublisherRenderDefaults,
    TypographyRules,
    HyphenationOptions,
    HyphenationDefaults,
    HyphenationLimits(typography_settings::hyphenation_controls::LimitsDraft),
    HyphenationLimitValue(
        typography_settings::hyphenation_controls::LimitsDraft,
        typography_settings::hyphenation_controls::Side,
    ),
    TypographyLanguages(typography_settings::LanguageScope),
    StyleTweaks,
    ParagraphSpacing,
    LinkStyles,
    FootnoteTweaks,
    StylePreset,
    ImageStyles,
    TableStyles,
    PseudoElementStyles,
    ListItemStyles,
    InPageFootnoteSize(style_tweaks::NoteSizeScope),
    StyleFiles,
    StyleReview,
    StyleEditor,
    StyleClear,
    ProfileName(profile_panel::Name),
    ReadingProfile(usize),
    ReadingProfileFields(usize),
    ReadingProfileValues(usize),
    ReadingProfileMargins(usize),
    ReadingProfileFontRendering(usize),
    ReadingProfileBookRendering(usize),
    ReadingProfileChoice(usize, reading_profiles::Field),
    ReadingProfileOpening(usize, profile_panel::opening_rules::Page),
    ReadingProfileClosing(usize, profile_panel::opening_rules::Page),
    ConfirmProfile(usize, profile_panel::Confirm),
    Find,
    Matches,
    SearchHistory,
    SearchSettings,
    MenuSearch,
    MenuSearchResults,
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
    TranslationSettings,
    WikipediaSettings,
    ClearOnlineHistory,
    ClearDictionaryHistory,
    DictionaryHistory,
    Vocabulary,
    VocabularyWord,
    VocabularyContext,
    VocabularyOptions,
    VocabularyBooks,
    VocabularySearch,
    Dictionaries,
    DictionaryOptions(usize),
    DictionaryButtons(dictionary_buttons::Page),
    DictionaryPresets(dictionary_presets::Page),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FooterPresetName {
    Create,
    Rename(usize),
}

impl Screen {
    fn title(self) -> &'static str {
        match self {
            Self::Footnote => "Footnote",
            Self::Library => "Reader",
            Self::LibraryFind => "Find a document",
            Self::FolderOrder => "Folder order",
            Self::OrganizeLibrary => "Library organization",
            Self::BookArchive => "Move to archive",
            Self::ArchiveFolder => "Archive folder",
            Self::MetadataArchive(page) => page.title(),
            Self::BookShortcuts(page) => page.title(),
            Self::LiveMetadata(page) => page.title(),
            Self::BookmarkBrowser(page) => page.title(),
            Self::ReadingHistory(page) => page.title(),
            Self::BookList => "Books",
            Self::BookDetails => "Book details",
            Self::BookIdentityEdit(field) => field.editor_title(),
            Self::CollectionName => "Collection name",
            Self::RemoveCollection => "Remove collection?",
            Self::ReadingControls => "Reading controls",
            Self::ReadingNotifications => "Notifications",
            Self::PastNotifications => "Past notifications",
            Self::NavigationSettings => "Navigation settings",
            Self::PageTurns => "Page turns",
            Self::PageTurnZones => "Tap zones",
            Self::PageTurnSizes(_) => "Tap zone sizes",
            Self::PageTurnDefaults => "Defaults for new books",
            Self::BackOptions(options) => options.title(),
            Self::ReadingGuides => "Reading guides",
            Self::ReadingGuidesAbout => "About reading guides",
            Self::BookmarkListSettings => "Bookmarks and notes",
            Self::AnnotationExchange => "Annotation exchange",
            Self::AnnotationExchangePreview => "Import annotation transfer",
            Self::AnnotationListSettings => "Highlight list text",
            Self::SelectionPosition => "Selection dialog position",
            Self::LongPressSettings => "Long-press on text",
            Self::VeryLongPressSettings => "Very long press interval",
            Self::HighlightPromptSettings => "Highlight prompt",
            Self::FooterSettings => "Reading status bar",
            Self::FooterItemWidths => "Item maximum width",
            Self::HeaderBatterySettings => "Header battery status",
            Self::FooterFontFaces => "Status bar font",
            Self::FooterCustomText => "Custom footer text",
            Self::FooterCustomTextGuide => "Custom text tokens",
            Self::FooterPresets => "Status bar presets",
            Self::FooterPreset(_) => "Status bar preset",
            Self::FooterPresetDelete(_) => "Delete status bar preset?",
            Self::FooterPresetName(_) => "Name status bar preset",
            Self::EndActionSettings => "At the end of a book",
            Self::MetadataSaveSettings => "Save reading data",
            Self::HiddenFlows => "Custom hidden flows",
            Self::ChapterNavigation => "Chapter navigation",
            Self::Gestures => "Reading gestures",
            Self::MenuActivationSettings => "Menu activation",
            Self::Scrolling => "Scrolling",
            Self::GestureBinding(gesture) => gesture.label(),
            Self::Multiswipes => "Preset multiswipes",
            Self::CustomMultiswipes => "Custom multiswipes",
            Self::RecordMultiswipe => "Record a multiswipe",
            Self::MultiswipeBinding(_) => "Multiswipe action",
            Self::RemoveMultiswipe(_) => "Remove multiswipe?",
            Self::GestureOverview => "Gesture overview",
            Self::ReadingData => "Reading data",
            Self::LibraryExport(page) => page.title(),
            Self::ClearOcr => "Clear recognized text?",
            Self::RestoreBackup => "Preview restore",
            Self::ImportAnnotations => "KOReader annotations",
            Self::ImportPosition => "KOReader reading position",
            Self::ImportPdfAnnotations => "PDF annotations",
            Self::ExportAnnotations => "Export annotations",
            Self::SelectionHtml => "Selection HTML",
            Self::ExportSettings(_, export_settings::Page::Settings) => "Export settings",
            Self::ExportSettings(
                export_settings::Origin::Exchange,
                export_settings::Page::Folders,
            ) => "Export / import folder",
            Self::ExportSettings(_, export_settings::Page::Folders) => "Export folder",
            Self::ExportSettings(_, export_settings::Page::Template(scope)) => scope.title(),
            Self::ExportSettings(_, export_settings::Page::Tokens) => "Filename tokens",
            Self::Book
            | Self::Fixed
            | Self::BookTools
            | Self::FixedTools
            | Self::FixedCrop
            | Self::Illustration => "",
            Self::GoToPage => "Go to page",
            Self::PageNumbering => "Page numbering",
            Self::NumberingValue(NumberField::ContentDpi(_)) => "Content zoom",
            Self::NumberingValue(NumberField::LogicalFont(_)) => "Font size",
            Self::NumberingValue(NumberField::LineSpacing(_)) => "Line spacing",
            Self::NumberingValue(NumberField::ProfileValue(_, field)) => field.label(),
            Self::NumberingValue(NumberField::WordSpacing(draft)) => {
                if draft.profile_index().is_some() {
                    "Saved word spacing"
                } else {
                    "Word spacing"
                }
            }
            Self::NumberingValue(NumberField::AutoTurn) => "Automatic page turns",
            Self::NumberingValue(NumberField::CornerHoldPause) => "Corner hold pause",
            Self::NumberingValue(NumberField::ScrollDelay) => "Scroll activation delay",
            Self::NumberingValue(NumberField::AutoTurnOverlapHorizontal) => "Horizontal overlap",
            Self::NumberingValue(NumberField::FixedTileZoom(axis)) => axis.label(),
            Self::NumberingValue(NumberField::CropMargin) => "Crop margin",
            Self::NumberingValue(NumberField::ReadingGuide(field)) => field.title(),
            Self::NumberingValue(NumberField::AutoTurnOverlapVertical) => "Vertical overlap",
            Self::NumberingValue(NumberField::FooterTrackMinimumWidth) => "Progress bar width",
            Self::NumberingValue(NumberField::FooterTrackMarginWidth) => "Progress bar margins",
            Self::NumberingValue(NumberField::FooterBatteryHideThreshold) => "Battery threshold",
            Self::NumberingValue(NumberField::FooterItemFontSize) => "Footer text size",
            Self::NumberingValue(NumberField::FooterItemWidth(item, _)) => item.width_label(),
            Self::NumberingValue(NumberField::FooterContainerHeight) => "Footer item height",
            Self::NumberingValue(NumberField::FooterContainerBottomMargin) => {
                "Footer bottom margin"
            }
            Self::NumberingValue(NumberField::FooterCustomTextRepetitions) => {
                "Custom text repetitions"
            }
            Self::NumberingValue(NumberField::MarginSize) => "Margin label size",
            Self::NumberingValue(_) => "Characters per page",
            Self::NumberingDefaults => "Page numbering defaults",
            Self::PublisherOffer => "Publisher page numbers",
            Self::PublisherPages => "Stable page number list",
            Self::PageOverview => "Pages",
            Self::Skim => "Skim",
            Self::BookMap => "Book map",
            Self::AutoTurn => "Automatic page turns",
            Self::AutoTurnDistance => "Automatic scroll distance",
            Self::AutoTurnPan => "Fixed-page pan",
            Self::LocationHistory => "Location history",
            Self::ReadFixedText => "",
            Self::OcrLanguage => "Recognize text",
            Self::PdfPassword => "Unlock PDF",
            Self::ScanColumns => "",
            Self::Contents | Self::FixedContents => "Contents",
            Self::ContentsSearch => "Search contents",
            Self::ContentsSettings => "Contents settings",
            Self::CustomContents => "Custom contents",
            Self::CustomEdits => "Edit chapters",
            Self::CustomBookmarks => "Choose a bookmark",
            Self::Hyphenate => "Hyphenate",
            Self::CustomTitle => "Chapter title",
            Self::GeneratedContents => "Generated outline",
            Self::Links => "Links on this page",
            Self::LinkSettings => "Links",
            Self::FootnoteFontSize(_) => "Footnote popup font size",
            Self::Bookmarks | Self::FixedBookmarks => "Bookmarks",
            Self::Typography => "Text settings",
            Self::ReadingFonts => "Reading font",
            Self::ReadingProfiles => "Reading profiles",
            Self::DocumentDefaults => "Document defaults",
            Self::PublisherRenderDefaults => "Default render mode",
            Self::TypographyRules => "Typography rules",
            Self::HyphenationOptions => "Hyphenation",
            Self::HyphenationDefaults => "Hyphenation defaults",
            Self::HyphenationLimits(_) => "Hyphenation limits",
            Self::HyphenationLimitValue(_, side) => side.title(),
            Self::TypographyLanguages(scope) => scope.title(),
            Self::FontRendering(eager_typography::Scope::Book) => "Font size rendering",
            Self::FontRendering(eager_typography::Scope::Default) => "Default font size rendering",
            Self::StyleTweaks => "Style tweaks",
            Self::ParagraphSpacing => "Spacing between paragraphs",
            Self::LinkStyles => "Links",
            Self::FootnoteTweaks => "Footnote options",
            Self::StylePreset => "Style tweak",
            Self::ImageStyles => "Images",
            Self::TableStyles => "Tables",
            Self::PseudoElementStyles => "Pseudo-elements",
            Self::ListItemStyles => "List items",
            Self::InPageFootnoteSize(style_tweaks::NoteSizeScope::Book) => "In-page footnote size",
            Self::InPageFootnoteSize(style_tweaks::NoteSizeScope::Default) => {
                "Default footnote size"
            }
            Self::StyleFiles => "Import CSS",
            Self::StyleReview => "Review CSS",
            Self::StyleEditor => "Book CSS",
            Self::StyleClear => "Clear book CSS?",
            Self::ProfileName(profile_panel::Name::OpeningPath(_)) => "Opening file path",
            Self::ProfileName(profile_panel::Name::ClosingPath(_)) => "Closing file path",
            Self::ProfileName(
                profile_panel::Name::OpeningMetadata(_, field)
                | profile_panel::Name::ClosingMetadata(_, field),
            ) => field.label(),
            Self::ProfileName(
                profile_panel::Name::OpeningTime(.., part)
                | profile_panel::Name::ClosingTime(.., part),
            ) => part.title(),
            Self::ProfileName(_) => "Name reading profile",
            Self::ReadingProfile(_) | Self::ConfirmProfile(_, _) => "Reading profile",
            Self::ReadingProfileFields(_) => "Included settings",
            Self::ReadingProfileValues(_) => "Saved profile values",
            Self::ReadingProfileMargins(_) => "Saved page margins",
            Self::ReadingProfileFontRendering(_) => "Saved font rendering",
            Self::ReadingProfileBookRendering(_) => "Saved book rendering",
            Self::ReadingProfileChoice(_, field) => field.label(),
            Self::ReadingProfileOpening(_, page) => {
                page.title(profile_panel::opening_rules::Event::Opening)
            }
            Self::ReadingProfileClosing(_, page) => {
                page.title(profile_panel::opening_rules::Event::Closing)
            }
            Self::Find => "Find in book",
            Self::Matches => "Search results",
            Self::SearchSettings => "Search settings",
            Self::MenuSearch => "Menu search",
            Self::MenuSearchResults => "Menu search results",
            Self::SearchHistory => "Recent searches",
            Self::BookmarkFind => "Find bookmarks",
            Self::BookmarkMatches => "Bookmark search results",
            Self::Highlights => "Saved marks",
            Self::Annotation => "Highlight",
            Self::RemoveHighlight => "Remove highlight?",
            Self::Note => "Add note",
            Self::EditNote => "Edit note",
            Self::Definition | Self::DictionaryEntry => "Dictionary",
            Self::OnlineLookup => "Online lookup",
            Self::OnlineEdit => "Online lookup",
            Self::OnlineHistory => "Online lookup history",
            Self::TranslationSettings => "Translation settings",
            Self::WikipediaSettings => "Wikipedia settings",
            Self::ClearOnlineHistory => "Online lookup history",
            Self::ClearDictionaryHistory => "Dictionaries",
            Self::DictionaryHistory => "Dictionary history",
            Self::Vocabulary => "Vocabulary",
            Self::VocabularyWord => "Word review",
            Self::VocabularyContext => "Reading context",
            Self::VocabularyOptions => "Vocabulary options",
            Self::VocabularyBooks => "Vocabulary books",
            Self::VocabularySearch => "Find a saved word",
            Self::Dictionaries => "Dictionaries",
            Self::DictionaryOptions(_) => "Dictionary settings",
            Self::DictionaryButtons(page) => page.title(),
            Self::DictionaryPresets(page) => page.title(),
        }
    }
}

enum Action {
    Open(LibraryBook),
    OpenTracked(u64),
    OpenPreviousDocument,
    OpenFolderFile(FolderDirection),
    Browse(PathBuf),
    CurrentBookFolder,
    Continue,
    ClearLibraryFilter,
    SetFolderOrder(FolderOrder),
    ListBooks(BookList),
    OrganizeCurrent,
    BookArchive(book_archive::Command),
    MetadataArchive(metadata_archive::Command),
    BookShortcut(book_shortcuts::Command),
    LiveMetadata(live_metadata::Command),
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
    SearchReadingMenus,
    OpenQuickstart,
    PreviewReadingMenu(usize),
    PageTurnSetting(page_turn_settings::Command),
    ToggleNotificationSource(reading_notifications::Source),
    SetReadingOrientation(ReadingOrientation),
    KeepReadingOrientation(bool),
    ConfirmReadingOrientationDefault(ReadingOrientation),
    EditListDensity(list_density::Scope, list_density::Field),
    EditSearchPresentation(search_presentation::Field),
    EditFooterItemWidth(crate::reading_controls::FooterTextItem),
    BindGesture(Gesture, Binding),
    Multiswipe(multiswipes::Command),
    MultiswipePreview(multiswipes::Shape),
    SetDisplayLevel(DisplayLevel, u32),
    RefreshDisplay,
    AutoTurn(Option<Duration>),
    AdjustAutoTurnDistance(i16),
    ResetAutoTurnDistance,
    AutoTurnPan(AutoTurnPanChange),
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
    BookMapChapter(usize),
    OpenFixedLink(crate::fixed::PdfLink),
    OpenLinkAddress(String),
    ToggleLinkPreference(link_taps::LinkPreference),
    SetMenuActivation(reading_bars::ActivationOption),
    Chapter(chapter_navigation::Direction),
    ChapterLevel(chapter_navigation::LevelSetting),
    SetCharacterPages(Option<u16>),
    CharacterPage(usize),
    AcceptPublisherNumbers,
    TogglePublisherFooter,
    TogglePublisherMargins,
    ShowGeneratedContents,
    ToggleAlternativeContents,
    AlternativeContentsEntry(OutlineTarget),
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
    ApplyWordSpacing(u64, crate::WordSpacePercent),
    ApplyLogicalFont(u64, f64),
    FontRendering(eager_typography::Scope, Option<eager_typography::Mode>),
    CancelEagerFont,
    TypographyLanguage(
        typography_settings::LanguageScope,
        Option<typography_settings::CatalogLanguage>,
    ),
    RespectEmbeddedLanguages(bool),
    SetHangingPunctuation {
        enabled: bool,
        for_new_books: bool,
    },
    Hyphenation(typography_settings::hyphenation_controls::Command),
    SetBookPublisherRenderMode(crate::PublisherRenderMode),
    SetPublisherRenderDefault(crate::PublisherRenderMode),
    ShowFonts,
    EmbeddedFontReport,
    ShowFooterFonts,
    SetFooterFontFace(Option<String>),
    ShowDictionaries,
    DictionaryPreset(dictionary_presets::Command),
    DictionarySource(SourceAction),
    DictionaryButtons(dictionary_buttons::Command),
    DocumentDefaults(default_settings::Command),
    Style(style_tweaks::Command),
    LookUp(String),
    Vocabulary(vocabulary::Command),
    Wikipedia,
    TranslatePage,
    RepeatOnline(usize),
    ToggleOnlineHistory,
    ToggleRomanizations,
    ToggleWikipediaImage,
    ToggleWikipediaMoreImages,
    ClearOnlineHistory,
    ClearDictionaryHistory,
    RepeatSearch(SearchQuery),
    StepNearby(crate::search::SearchDirection),
    SetFont(Option<String>),
    ApplyProfile(usize),
    RemoveProfile(usize),
    UpdateReadingProfile(usize),
    EditReadingProfileFields(usize),
    ToggleReadingProfileField(reading_profiles::Field),
    SetReadingProfileFields(bool),
    SetClosingProfile(usize, Option<reading_profiles::OnClose>),
    OpeningProfileRule(usize, profile_panel::opening_rules::Command),
    ClosingProfileRule(usize, profile_panel::opening_rules::Command),
    SetProfileRendering(usize, profile_panel::rendering_choices::Value),
    ToggleProfileMarginLink(usize),
    UpdateFooterPreset(usize),
    ApplyFooterPreset(usize),
    RenameFooterPreset(usize),
    DeleteFooterPreset(usize),
    ResetText,
    ReadingMode(bool),
    TopStatusHeader(bool),
    AboutTopHeader,
    EditHeaderFontSize,
    ConfirmTopStatusHeaderDefault(bool),
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
    ExportBookClippings,
    LibraryExport(library_export::Command),
    ExportSettings(export_settings::Origin, export_settings::Command),
    AnnotationExchange(annotation_exchange::Command),
    FitPage(bool),
    FitHeight,
    SetFixedZoom(usize),
    SetFixedTileZoom(FixedTileAxis, u8),
    SetFixedPageOverlap(bool, crate::PageOverlapStyle),
    SetFixedRotation(u8),
    FixedScroll(bool),
    ComicSpread(bool),
    ScanContrast(bool),
    ScanGamma(u16),
    FixedDither(bool),
    FixedGap(u16),
    ScanWhiteLevel(Option<u8>),
    StraightenScan(u8),
    ForceOcr(bool),
    ScanColumns,
    Overview,
    Skim,
    FixedPage(usize),
    Heading,
    RotatePage,
    ReversePageDirection,
    CropPage,
    SemiAutoCrop,
    SetCropMargin(u8),
    RecognizeText,
    RecognizeLanguage(String),
    ReadFixedText,
    ReflowScan,
}

#[derive(Clone, Copy)]
enum AutoTurnPanChange {
    ToggleVerticalFirst,
    ToggleBottomToTop,
    Reset,
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
    embedded_font_inventory: Option<embedded_fonts::Availability>,
    header_metadata: Option<top_header::Metadata>,
}

struct SavedMarkRow {
    created_at: i64,
    location: (usize, usize, usize, usize),
    kind: u8,
    storage_index: usize,
    item: Item,
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum SelectionSearchOrigin {
    Reading,
    Dictionary,
}

#[derive(Clone)]
enum TouchHoldBase {
    Reflow(TextSelection),
    Fixed(PdfSelection),
}

struct TouchHold {
    base: Option<TouchHoldBase>,
    corner_armed: bool,
}

pub struct Reader {
    library: Library,
    quickstart: Option<quickstart::Guide>,
    user_hyphenation: std::rc::Rc<crate::user_hyphenation::Rules>,
    user_hyphenation_path: PathBuf,
    hyphenation_edit: Option<user_hyphenation::Draft>,
    browser: FileBrowser,
    library_viewport: crate::session::ReadingViewport,
    archive: book_archive::Archive,
    metadata_archive: metadata_archive::State,
    book_shortcuts: book_shortcuts::State,
    live_metadata: live_metadata::State,
    bookmark_browser: bookmark_browser::Browser,
    library_query: String,
    book_list: Option<BookList>,
    book_details: Option<BookDetails>,
    overview: Option<PageOverview>,
    skim: Option<skim::Session>,
    staged_viewport: Option<(
        primitive_app::viewport::Viewport,
        Option<reading_viewport_stage::Stage>,
    )>,
    accepted_viewport: Option<primitive_app::viewport::Viewport>,
    negotiated_opening_profiles: bool,
    opening_profile_epoch: u64,
    pending_opening_profiles: Option<profile_panel::on_open::OpeningPlan>,
    opening_execution: Option<profile_panel::opening_execution::OpeningExecution>,
    closing_profile: Option<profile_panel::on_close::Closing>,
    closing_before_switch: Option<profile_panel::on_close::BeforeSwitch>,
    pending_orientation: Option<reading_orientation::PendingOrientation>,
    staged_frame_orientation: Option<ReadingOrientation>,
    flow_popup: Option<hidden_flows::FlowPopup>,
    outline: Option<OutlineScan>,
    alternative_contents: alternative_contents::AlternativeContents,
    numbering_scan: Option<character_page_settings::Scan>,
    numbering_directory: PathBuf,
    chapter_draft: Option<custom_contents::Draft>,
    scan_columns: Option<ScanReading>,
    rename_collection: Option<String>,
    gestures: GestureBindings,
    controls_path: PathBuf,
    controls: ReadingControls,
    footer_font_path: PathBuf,
    footer_font_file: Option<String>,
    footer_font_render_path: Option<PathBuf>,
    footer_custom_text_path: PathBuf,
    footer_custom_text: footer_custom_text::Preference,
    footer_initial_position: Option<reading_footer::InitialPosition>,
    footer_refresh_at: Option<Instant>,
    status_battery_last: Option<(u32, bool)>,
    end_dialog: Option<end_of_book::Dialog>,
    pages_since_refresh: u8,
    reading_guide_progress: reading_guides::Progress,
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
    font_chooser_return: Option<typeface::FontChooserReturn>,
    profiles: Vec<Profile>,
    profile_fields: Vec<reading_profiles::Field>,
    profile_opening_time_draft: u16,
    opening_delay_draft: u8,
    profiles_path: PathBuf,
    footer_presets: Vec<FooterPreset>,
    footer_presets_path: PathBuf,
    document_defaults: crate::document_defaults::Store,
    style_store: crate::user_styles::storage::Store,
    style_view: style_tweaks::View,
    active: Option<ActiveBook>,
    logical_recovery: Option<logical_font::Recovery>,
    illustration: Option<FixedReading>,
    fixed: Option<ActiveFixed>,
    ocr_assets: Option<OcrAssets>,
    ocr_root: PathBuf,
    ocr: Option<ActiveOcr>,
    text_flow: Option<TextFlow>,
    locked: Option<LibraryBook>,
    folder_password_return: Option<Screen>,
    crop_dialog: Option<(FixedPreferences, Option<(f32, f32)>)>,
    semi_auto_crop: Option<semi_auto_crop::Draft>,
    screen: Screen,
    reading_panel: Option<BarPanel>,
    embedded_font_report: Option<embedded_fonts::Inspection>,
    last_text_panel: BarPanel,
    last_fixed_panel: BarPanel,
    top_group: Option<TopGroup>,
    last_top_group: TopGroup,
    reading_top: bool,
    reading_bottom: bool,
    quick_display: QuickDisplay,
    /// Opened from Primitive Launcher: the launcher's theme, which the whole Reader follows. `None` standalone.
    system_theme: Option<SystemTheme>,
    launcher_orientation_handoff: bool,
    refresh_display: bool,
    auto_turn: AutoTurn,
    read_alarm: read_alarm::ReadAlarm,
    top_header_default_prompt: Option<bool>,
    orientation_default_prompt: Option<reading_orientation::DefaultPrompt>,
    top_header_about: bool,
    top_header_font_picker: Option<top_header_font::Picker>,
    list_page: usize,
    reading_list_anchor: Option<navigation_layout::ReadingListAnchor>,
    menu_search: menu_search::Search,
    matches_page: usize,
    match_window: Option<search_layout::MatchWindow>,
    matches_return_to_reading: bool,
    result_context: Option<result_context::ResultContext>,
    flexible_results: std::cell::RefCell<Option<flexible_results::FlexibleResults>>,
    focused_result: Option<usize>,
    search_results_navigation: search_results_navigation::Navigation,
    current_contents_row: Option<usize>,
    contents_view: Option<contents_view::ContentsView>,
    list_density_picker: Option<list_density::Picker>,
    flexible_marks: Option<saved_marks_view::FlexibleMarks>,
    publisher_label_canvas: std::cell::RefCell<Option<Canvas>>,
    map_window: fixed_book_map::MapWindow,
    book_map_focus: Option<usize>,
    ribbon_maps: Vec<page_overview::RibbonMap>,
    back: Vec<HistoryLocation>,
    forward: Vec<HistoryLocation>,
    swipe_back_notice_pending: bool,
    back_navigation: back_navigation::State,
    notice: Option<String>,
    notifications: reading_notifications::Notifications,
    dirty: bool,
    save_at: Option<Instant>,
    metadata_checkpoint: metadata_save::Checkpoint,
    editor: Editor,
    opening_metadata_phrase: Option<String>,
    opening_path_overlay: Option<profile_panel::opening_shortcuts::Overlay>,
    nearby: nearby_search::Nearby,
    screen_page_jump: Option<go_to_page::PendingJump>,
    screen_page_worker: crate::session::screen_pages::background::Worker,
    eager_typography: eager_typography::State,
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
    selection_html: Option<selection_html::Inspector>,
    touch_hold: Option<TouchHold>,
    scroll_pan: scrolling::Pan,
    selection_mode: bool,
    highlight_prompt_stage: Option<selection_popup::HighlightPromptStage>,
    pending_highlight_style: Option<crate::highlight_style::HighlightStyle>,
    pending_highlight_color: Option<crate::highlight_style::HighlightColor>,
    pending_selection_action: Option<SelectionLongPressAction>,
    footnote: Option<footnote_popup::Footnote>,
    highlight_index: usize,
    range_edit: Option<extend_highlight::RangeEdit>,
    link_popup: Option<external_link::LinkPopup>,
    clipboard_text: Option<String>,
    saved_popup: Option<saved_highlight_popup::SavedPopup>,
    exports: PathBuf,
    export_settings: export_settings::Settings,
    annotation_exchange: annotation_exchange::State,
    library_export: library_export::Export,
    dictionaries: DictionarySources,
    dictionary_preference: PathBuf,
    dictionary_settings: SourcePreferences,
    dictionary_origin: Screen,
    installed_dictionaries: Vec<InstalledDictionary>,
    lookup_history_path: PathBuf,
    lookup_history: LookupHistory,
    vocabulary_view: vocabulary::View,
    online: Option<online_panel::Panel>,
    annotation_search: annotation_search::AnnotationSearch,
    definition_word: String,
    definitions: Vec<dictionary::LookupEntry>,
    definition_expanded: bool,
    dictionary_navigation: dictionary_panel::Navigation,
    spelling_lookup: Option<dictionary_panel::SpellingLookup>,
    cjk_selection_lookup: Option<cjk_selection::Lookup>,
}

impl Reader {
    /// Immutable inputs for diagnostics to reproduce this live reflow session.
    /// Fixed-page documents deliberately have no reflow configuration.
    pub(crate) fn verification_book_config(&self) -> Option<BookConfig> {
        self.active
            .as_ref()
            .map(|active| active.reading.config.clone())
    }

    pub fn open(documents: &Path, state: &Path, fonts: Vec<PathBuf>) -> io::Result<Self> {
        reading_backup::recover(state)?;
        Self::open_data(documents, state, fonts)
    }

    fn open_data(documents: &Path, state: &Path, fonts: Vec<PathBuf>) -> io::Result<Self> {
        let library = with_storage(|| {
            let mut library = Library::open(documents, state)?;
            library.scan()?;
            Ok(library)
        })?;
        let data = state.parent().unwrap_or(documents);
        let controls_path = data.join("reading-controls.json");
        let controls = ReadingControls::load(&controls_path, !library.recents().is_empty());
        let browser = FileBrowser::new(with_storage(|| {
            library.browse_with_order(Path::new(""), controls.folder_order)
        })?);
        let fonts_directory = data.join("fonts");
        let footer_font_path = data.join("status-bar-font.json");
        let footer_font_file = footer_font_face::load(&footer_font_path, &fonts_directory);
        let footer_font_render_path = footer_font_file
            .as_deref()
            .and_then(|file| reading_fonts::resolve(&fonts_directory, file).ok());
        let footer_custom_text_path = data.join("status-bar-custom-text.json");
        let footer_custom_text = footer_custom_text::load(&footer_custom_text_path);
        let stats_path = data.join("reading-stats.json");
        let stats = ReadingStats::load(&stats_path);
        let reading_history = crate::reading_stats::history::History::load(
            data.join("reading-history.json"),
            stats.recorded_totals(),
        );
        let profiles_path = data.join("reading-profiles.json");
        let profiles = reading_profiles::load(&profiles_path);
        let footer_presets_path = data.join("status-bar-presets.json");
        let footer_presets = footer_presets::load(&footer_presets_path);
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
            quickstart: None,
            browser,
            archive: book_archive::Archive::load(data.join("archive-folder.json")),
            metadata_archive: metadata_archive::State::load(
                data.join("book-metadata-archive.json"),
            ),
            book_shortcuts: book_shortcuts::State::load(data.join("book-shortcuts.json")),
            live_metadata: Default::default(),
            bookmark_browser: bookmark_browser::Browser::load(data.join("bookmark-browser.json")),
            library_query: String::new(),
            book_list: None,
            book_details: None,
            overview: None,
            skim: None,
            library_viewport: crate::session::ReadingViewport::PORTRAIT,
            staged_viewport: None,
            accepted_viewport: None,
            negotiated_opening_profiles: false,
            opening_profile_epoch: 0,
            pending_opening_profiles: None,
            opening_execution: None,
            closing_profile: None,
            closing_before_switch: None,
            pending_orientation: None,
            staged_frame_orientation: None,
            flow_popup: None,
            outline: None,
            alternative_contents: Default::default(),
            numbering_scan: None,
            numbering_directory: data.join("character-pages"),
            chapter_draft: None,
            scan_columns: None,
            rename_collection: None,
            controls_path,
            controls,
            footer_font_path,
            footer_font_file,
            footer_font_render_path,
            footer_custom_text_path,
            footer_custom_text,
            footer_initial_position: None,
            footer_refresh_at: None,
            status_battery_last: None,
            end_dialog: None,
            pages_since_refresh: 0,
            reading_guide_progress: reading_guides::Progress::default(),
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
            fonts_directory,
            available_fonts: Vec::new(),
            font_chooser_return: None,
            profiles,
            profile_fields: Vec::new(),
            profile_opening_time_draft: 0,
            opening_delay_draft: 0,
            profiles_path,
            footer_presets,
            footer_presets_path,
            document_defaults: crate::document_defaults::Store::load(
                data.join(crate::document_defaults::FILE),
            ),
            style_store: crate::user_styles::storage::Store::load(
                data.join(crate::user_styles::FILE),
            ),
            style_view: Default::default(),
            active: None,
            logical_recovery: None,
            illustration: None,
            fixed: None,
            ocr_assets,
            ocr_root: data.join("ocr-cache"),
            ocr: None,
            text_flow: None,
            locked: None,
            folder_password_return: None,
            crop_dialog: None,
            semi_auto_crop: None,
            screen: Screen::Library,
            reading_panel: None,
            embedded_font_report: None,
            last_text_panel: BarPanel::Text,
            last_fixed_panel: BarPanel::View,
            top_group: None,
            last_top_group: TopGroup::Navigation,
            reading_top: false,
            reading_bottom: false,
            gestures: GestureBindings::load(data),
            quick_display: QuickDisplay::read(),
            system_theme: SystemTheme::from_env(),
            launcher_orientation_handoff: false,
            refresh_display: false,
            auto_turn: AutoTurn::default(),
            read_alarm: read_alarm::ReadAlarm::open(data.join("read-timer.json")),
            top_header_default_prompt: None,
            orientation_default_prompt: None,
            top_header_about: false,
            top_header_font_picker: None,
            list_page: 0,
            reading_list_anchor: None,
            menu_search: menu_search::Search::open(state),
            matches_page: 0,
            match_window: None,
            matches_return_to_reading: false,
            result_context: None,
            flexible_results: Default::default(),
            focused_result: None,
            search_results_navigation: search_results_navigation::Navigation::default(),
            current_contents_row: None,
            contents_view: None,
            list_density_picker: None,
            flexible_marks: None,
            publisher_label_canvas: Default::default(),
            map_window: fixed_book_map::MapWindow::default(),
            book_map_focus: None,
            ribbon_maps: Vec::new(),
            back: Vec::new(),
            forward: Vec::new(),
            swipe_back_notice_pending: false,
            back_navigation: back_navigation::State::default(),
            notice: None,
            notifications: Default::default(),
            dirty: false,
            save_at: None,
            metadata_checkpoint: metadata_save::Checkpoint::new(Instant::now()),
            editor: Editor::new(64),
            opening_metadata_phrase: None,
            opening_path_overlay: None,
            nearby: nearby_search::Nearby::default(),
            screen_page_jump: None,
            screen_page_worker: Default::default(),
            eager_typography: Default::default(),
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
            selection_html: None,
            touch_hold: None,
            scroll_pan: scrolling::Pan::default(),
            selection_mode: false,
            highlight_prompt_stage: None,
            pending_highlight_style: None,
            pending_highlight_color: None,
            pending_selection_action: None,
            footnote: None,
            highlight_index: 0,
            range_edit: None,
            link_popup: None,
            clipboard_text: None,
            saved_popup: None,
            exports: state.parent().unwrap_or(documents).join("exports"),
            export_settings: export_settings::Settings::load(
                data.join("annotation-export-files.json"),
            ),
            annotation_exchange: annotation_exchange::State::load(
                data.join("annotation-exchange.json"),
            ),
            library_export: library_export::Export::load(data.join("highlight-export.json")),
            dictionaries: DictionarySources::for_reader(documents, data),
            dictionary_preference,
            dictionary_settings,
            dictionary_origin: Screen::ReadingControls,
            installed_dictionaries: Vec::new(),
            lookup_history_path,
            lookup_history,
            vocabulary_view: Default::default(),
            online: None,
            annotation_search: annotation_search::AnnotationSearch::default(),
            definition_word: String::new(),
            definitions: Vec::new(),
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
        self.request_close(profile_panel::on_close::AfterClose::Open {
            entry,
            password_return: None,
        })?;
        Ok(())
    }

    fn open_book_now(&mut self, entry: LibraryBook) -> Result<()> {
        let publication = with_storage(|| Ok(self.library.publication_for_chapter(&entry)))?;
        if let Some((marker, chapter, path)) = publication {
            self.open_with_password(marker, None)?;
            if self.metadata_archive.waiting_to_restore() {
                return Ok(());
            }
            if self.fixed.is_some() {
                return self.follow_fixed(chapter, None);
            }
            return self.follow(Destination {
                path,
                fragment: None,
            });
        }
        self.open_with_password(entry, None)
    }

    fn open_with_password(&mut self, entry: LibraryBook, password: Option<&str>) -> Result<()> {
        self.open_with_password_mode(entry, password, true)
    }

    fn open_with_password_mode(
        &mut self,
        entry: LibraryBook,
        password: Option<&str>,
        check_archive: bool,
    ) -> Result<()> {
        if check_archive && password.is_none() {
            self.metadata_archive.abandon_archive_consumption();
        }
        let opened = self.open_with_archive_check(entry, password, check_archive);
        if opened.is_ok() {
            self.complete_archive_consumption();
        } else if self.screen != Screen::PdfPassword {
            self.metadata_archive.abandon_archive_consumption();
        }
        opened
    }

    fn open_with_archive_check(
        &mut self,
        mut entry: LibraryBook,
        password: Option<&str>,
        check_archive: bool,
    ) -> Result<()> {
        self.eager_typography.cancel();
        self.closing_profile = None;
        self.opening_execution = None;
        self.screen_page_worker.cancel();
        self.close_fixed_search_fit()?;
        self.logical_recovery = None;
        self.footer_initial_position = None;
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
        with_storage(|| {
            self.library.recover_live_metadata(&entry, id);
            Ok(())
        })?;
        let unseen = self.library.state(id).is_none();
        if unseen && check_archive && password.is_none() && self.offer_archived_metadata(&entry, id)
        {
            return Ok(());
        }
        let top_status_header = self.top_header_for_book(id);
        let mut initial_preferences = self
            .document_defaults
            .templates
            .reflow
            .clone()
            .unwrap_or_else(BookPreferences::for_new_reflow);
        if entry.format == DocumentFormat::Epub {
            initial_preferences.publisher_render_mode = self.controls.publisher_render_mode_default;
        }
        let mut default_font_missing = false;
        let mut settings = initial_preferences.apply_to(self.device_reading_settings());
        let mut scroll = initial_preferences.scroll;
        settings.status_header_height =
            crate::reading_controls::TopHeader::height(top_status_header, scroll);
        let mut facing_pages = initial_preferences.facing_pages;
        let mut fixed_preferences = self
            .document_defaults
            .templates
            .fixed
            .unwrap_or_default()
            .apply(FixedPreferences::default());
        let position = self.library.state(id).and_then(|state| {
            // A saved state inherits the device baseline, never a newly changed
            // template, including older records with empty/default preferences.
            settings = state.preferences.apply_to(self.device_reading_settings());
            scroll = state.preferences.scroll;
            settings.status_header_height =
                crate::reading_controls::TopHeader::height(top_status_header, scroll);
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
            self.library
                .state(id)
                .map(|state| state.pdf_crop_seeds.clone()),
        );
        let mut fixed = match fixed {
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
        if self.screen == Screen::PdfPassword
            && let Some(reading) = &mut fixed
            && reading.viewport() != self.reading_viewport()
        {
            let prepared = reading.prepare_viewport(self.reading_viewport())?;
            reading.commit_viewport(prepared)?;
        }
        self.locked = None;
        self.numbering_scan = None;
        self.touch_locked = false;
        self.ocr = None;
        self.hits.clear();
        self.pdf_hits.clear();
        self.search_results_navigation = Default::default();
        self.cancel_search();
        self.search_result = None;
        self.result_selection = None;
        let local_minute = self.profile_opening_minute();
        if let Some(mut reading) = fixed {
            if fixed_preferences.semi_auto_crop && !fixed_preferences.continuous {
                fixed_preferences = reading.preferences();
            }
            let flows = self
                .library
                .state(id)
                .map(|state| state.hidden_flows.clone())
                .unwrap_or_default();
            reading.restore_hidden_flows(
                &flows,
                position.as_ref().map_or(0, |saved| saved.page),
                fixed_preferences,
            )?;
            let is_new = position.is_none();
            let metadata = self.opening_fixed_metadata(&reading, &absolute, is_new);
            let opening = reading.profile_provider().and_then(|provider| {
                self.opening_profile_plan(
                    id,
                    &absolute,
                    is_new,
                    Some(provider),
                    metadata.as_ref(),
                    local_minute,
                )
            });
            let queued = if opening.is_none() {
                reading
                    .profile_provider()
                    .map_or_else(Vec::new, |provider| {
                        self.legacy_opening_queue(
                            id,
                            &absolute,
                            is_new,
                            Some(provider),
                            metadata.as_ref(),
                            local_minute,
                        )
                    })
            } else {
                Vec::new()
            };
            let profile_warning = if opening.is_some() {
                let target = self.opening_display_viewport();
                if target != reading.viewport() {
                    let baseline = reading.prepare_viewport(target)?;
                    reading.commit_viewport(baseline)?;
                }
                None
            } else {
                self.apply_opening_fixed_profiles(
                    &mut reading,
                    id,
                    &absolute,
                    is_new,
                    metadata.as_ref(),
                    local_minute,
                )
                .err()
            };
            reading.set_ocr_path(self.ocr_root.join(format!("{id:016x}")));
            reading.ocr_language = self
                .library
                .state(id)
                .and_then(|state| state.ocr_language.clone())
                .filter(|code| {
                    self.ocr_assets
                        .as_ref()
                        .is_none_or(|assets| assets.languages().contains(code))
                });
            if let Some(title) = reading.title() {
                entry.title = title;
            }
            entry.author = reading.author();
            self.save_library()?;
            let closing_warning = self.finish_before_switch(id);
            self.pending_orientation = None;
            self.install_opening_profile_plan(opening);
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
            self.matches_return_to_reading = false;
            self.clear_location_history();
            self.auto_turn
                .configure(self.controls.auto_turn_interval(), Instant::now());
            self.selection = None;
            self.annotation_search = annotation_search::AnnotationSearch::default();
            self.restore_reading_menus(id);
            self.reading_history.begin_session(crate::mark_time::now());
            self.show(Screen::Fixed);
            if self.pending_opening_profiles.is_none() {
                self.settle_opening_location();
                self.arm_opening_execution(queued, Instant::now());
            }
            if let Some(error) = profile_warning {
                self.notice = Some(format!("Opening profile could not be applied: {error}"));
            } else if closing_warning.is_some() {
                self.notice = closing_warning;
            }
            if let Some(fixed) = &self.fixed {
                self.library.cache_author(id, fixed.entry.author.as_deref());
            }
            self.library.request_live_metadata(id);
            return Ok(());
        }
        if unseen
            && initial_preferences.font_file.is_some()
            && self
                .fonts_for(initial_preferences.font_file.as_deref())
                .is_err()
        {
            initial_preferences.font_file = None;
            default_font_missing = true;
        }
        let custom_font = self
            .library
            .state(id)
            .map_or(initial_preferences.font_file.as_deref(), |state| {
                state.preferences.font_file.as_deref()
            });
        let fonts = self
            .fonts_for(custom_font)
            .unwrap_or_else(|_| self.fonts.clone());
        let config = BookConfig {
            viewport: crate::session::ReadingViewport::PORTRAIT,
            user_styles: self.style_store.data.snapshot(id)?,
            user_hyphenation: self.user_hyphenation.clone(),
            chapter_offsets: Default::default(),
            quote_prefixes: Default::default(),
            screen_pages: Default::default(),
            eager_layouts: Default::default(),
            engine: Engine::Parley,
            book: absolute,
            fonts,
            settings,
            scroll,
            facing_pages,
            hide_nonlinear: self
                .library
                .state(id)
                .and_then(|state| state.hide_nonlinear)
                .unwrap_or(self.controls.hide_nonlinear_default),
            hidden_flows: self
                .library
                .state(id)
                .map(|state| state.hidden_flows.clone())
                .unwrap_or_default()
                .into(),
        };
        let saved_preferences = self
            .library
            .state(id)
            .map_or(&initial_preferences, |state| &state.preferences);
        let is_new = position.is_none();
        let metadata = self.opening_text_metadata(&config, is_new);
        let opening = self
            .opening_profile_plan(
                id,
                &config.book,
                is_new,
                None,
                metadata.as_ref(),
                local_minute,
            )
            .map(|mut plan| {
                plan.text = Some((saved_preferences.clone(), top_status_header));
                plan
            });
        let (automatic, profile_warning) = if opening.is_some() {
            (None, None)
        } else {
            match self.prepare_opening_text_profiles(
                &config,
                saved_preferences,
                position.as_ref(),
                top_status_header,
                id,
                metadata.as_ref(),
                local_minute,
            ) {
                Ok(prepared) => (prepared, None),
                Err(error) => (None, Some(error)),
            }
        };
        let queued = if opening.is_none() {
            self.legacy_opening_queue(
                id,
                &config.book,
                is_new,
                None,
                metadata.as_ref(),
                local_minute,
            )
        } else {
            Vec::new()
        };
        if automatic.is_none()
            && config.settings.logical_font_size.is_some()
            && config.settings.validate().is_err()
            && (Settings {
                logical_font_size: None,
                font_size: 12.,
                ..config.settings
            })
            .validate()
            .is_ok()
        {
            // Prove this is an otherwise readable edition before offering a
            // layout reset. Parser/DRM/font/source-anchor failures propagate.
            let mut control = config.clone();
            control.settings = Settings {
                logical_font_size: None,
                font_size: 12.,
                ..control.settings
            };
            let checked = if let Some(saved) = &position {
                Reading::restore(control, saved.chapter, saved.page, saved.anchor.as_ref())?
            } else {
                Reading::open(control)?
            };
            if let Some(anchor) = position.as_ref().and_then(|saved| saved.anchor.as_ref())
                && checked.probe.restore_anchor(anchor).is_none()
            {
                return Err(io::Error::other(
                    "The saved source anchor is unavailable; this is not a font-fit error",
                )
                .into());
            }
            self.logical_recovery = Some(logical_font::Recovery {
                entry,
                id,
                config,
                position,
                preferences: self
                    .library
                    .state(id)
                    .map_or(initial_preferences, |state| state.preferences.clone()),
            });
            return Ok(());
        }
        self.logical_recovery = None;
        let (mut reading, preferences) = if let Some((reading, preferences)) = automatic {
            (reading, Some(preferences))
        } else {
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
            (reading, unseen.then_some(initial_preferences))
        };
        reading.set_status_header_height(crate::reading_controls::TopHeader::height(
            top_status_header,
            reading.is_continuous(),
        ))?;
        if opening.is_some() && reading.config.viewport != self.opening_display_viewport() {
            reading = reading.prepare_viewport(self.opening_display_viewport())?;
        }
        entry.author.clone_from(&reading.probe.author);
        self.save_library()?;
        let closing_warning = self.finish_before_switch(id);
        self.pages_since_refresh = 0;
        self.range_edit = None;
        self.saved_popup = None;
        self.pending_orientation = None;
        self.install_opening_profile_plan(opening);
        let header_metadata =
            top_status_header.then(|| self.load_header_metadata(entry.library_path()));
        self.active = Some(ActiveBook {
            entry,
            id,
            reading,
            numbering: None,
            embedded_font_inventory: None,
            header_metadata,
        });
        self.style_view = Default::default();
        let active = self.active.as_ref().unwrap();
        let state = self
            .library
            .remember(&active.entry, id, &active.reading.probe.title);
        if let Some(preferences) = preferences {
            default_font_missing &= preferences.font_file.is_none();
            state.preferences = preferences;
        }
        state.top_status_header = Some(top_status_header);
        state.preferences.record_typography(&active.reading.probe);
        state
            .preferences
            .record_hyphenation(active.reading.config.settings);
        let offer_publisher = self.apply_new_numbering_defaults();
        self.restore_character_pages();
        self.fixed = None;
        self.matches_return_to_reading = false;
        self.clear_location_history();
        self.auto_turn
            .configure(self.controls.auto_turn_interval(), Instant::now());
        self.selection = None;
        self.touch_hold = None;
        self.selection_mode = false;
        self.pdf_selection = None;
        self.annotation_search = annotation_search::AnnotationSearch::default();
        self.restore_reading_menus(id);
        self.reading_history.begin_session(crate::mark_time::now());
        self.show(if offer_publisher {
            Screen::PublisherOffer
        } else {
            Screen::Book
        });
        if self.pending_opening_profiles.is_none() {
            self.settle_opening_location();
            self.arm_opening_execution(queued, Instant::now());
        }
        if let Some(error) = profile_warning {
            self.notice = Some(format!("Opening profile could not be applied: {error}"));
        } else if default_font_missing {
            self.notice = Some("Default reading font unavailable; using device fonts".into());
        } else if closing_warning.is_some() {
            self.notice = closing_warning;
        }
        if let Some(active) = &self.active {
            self.library
                .cache_author(active.id, active.reading.probe.author.as_deref());
        }
        self.library.request_live_metadata(id);
        Ok(())
    }

    fn show(&mut self, screen: Screen) {
        self.menu_search.clear_overlays();
        self.embedded_font_report = None;
        self.finish_interrupted_scroll();
        self.gestures.multiswipes.clear_contact();
        if screen != Screen::RecordMultiswipe {
            self.gestures.multiswipes.clear_draft();
        }
        self.opening_path_overlay = None;
        if !matches!(
            screen,
            Screen::Book | Screen::Fixed | Screen::PublisherOffer
        ) {
            self.settle_pending_opening();
        }
        self.reading_list_anchor = None;
        if screen != Screen::ReadingFonts {
            self.font_chooser_return = None;
        }
        if matches!(self.screen, Screen::BookmarkBrowser(_))
            || matches!(screen, Screen::BookmarkBrowser(_))
        {
            self.bookmark_browser.reset_list_anchor();
        }
        self.archive.close_surfaces(screen);
        self.metadata_archive.close_surfaces(screen);
        self.book_shortcuts.close_surfaces(screen);
        if !matches!(screen, Screen::ReadingProfileFields(_)) {
            self.profile_fields.clear();
        }
        if screen != Screen::SelectionHtml {
            self.selection_html = None;
        }
        if !matches!(
            screen,
            Screen::ExportSettings(_, export_settings::Page::Folders)
        ) {
            self.export_settings.close_picker();
        }
        if screen != Screen::AnnotationExchangePreview {
            self.annotation_exchange.finish_preview();
        }
        if screen != Screen::PdfPassword {
            self.folder_password_return = None;
        }
        if screen != Screen::GoToPage {
            self.screen_page_jump = None;
        }
        if !(matches!(screen, Screen::Book | Screen::GoToPage)
            || screen == Screen::Skim && self.uses_alternative_contents())
        {
            self.screen_page_worker.cancel();
        }
        self.search_results_navigation.menu_open = false;
        self.top_header_about = false;
        self.top_header_font_picker = None;
        self.orientation_default_prompt = None;
        let previous = self.screen;
        let fit_error = if screen != Screen::Fixed && screen != previous {
            self.close_fixed_search_fit().err()
        } else {
            None
        };
        if screen != previous {
            self.list_density_picker = None;
        }
        if !matches!(screen, Screen::Book | Screen::Fixed) {
            self.touch_hold = None;
        }
        if !matches!(screen, Screen::Book | Screen::Fixed) {
            self.selection_mode = false;
        }
        self.flow_popup = None;
        self.end_dialog = None;
        self.close_dictionary_preset_picker();
        if screen != Screen::Dictionaries {
            self.close_dictionary_font_size();
        }
        if !matches!(screen, Screen::DictionaryButtons(_)) {
            self.close_dictionary_button_editor();
        }
        if screen != Screen::Definition {
            self.spelling_lookup = None;
        }
        self.cjk_selection_lookup = None;
        self.result_context = None;
        if !matches!(
            screen,
            Screen::Footnote | Screen::Definition | Screen::OnlineLookup | Screen::OnlineEdit
        ) {
            self.footnote = None;
        }
        if screen != Screen::Hyphenate {
            self.hyphenation_edit = None;
        }
        if screen != Screen::CustomTitle {
            self.chapter_draft = None;
        }
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
                if self.screen != Screen::BookmarkMatches {
                    self.annotation_search.origin = Some(self.screen);
                }
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
            && !(matches!(screen, Screen::Illustration | Screen::ReadingHistory(_))
                && self.book_details.is_some())
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
        if !matches!(
            screen,
            Screen::BookDetails
                | Screen::BookIdentityEdit(_)
                | Screen::Illustration
                | Screen::ReadingHistory(_)
        ) {
            self.book_details = None;
        }
        if screen != Screen::PageOverview {
            self.overview = None;
        }
        if !matches!(screen, Screen::PageOverview | Screen::BookMap) {
            self.ribbon_maps.clear();
        }
        if screen != Screen::Skim {
            self.skim = None;
        }
        if screen != Screen::GeneratedContents {
            self.outline = None;
        }
        if screen != Screen::ContentsSettings {
            self.alternative_contents.confirm = None;
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
            self.editor = Editor::new(
                if matches!(
                    screen,
                    Screen::NumberingValue(
                        NumberField::LogicalFont(_)
                            | NumberField::ProfileValue(
                                _,
                                reading_profiles::ValueField::FontSize
                                    | reading_profiles::ValueField::FontWeight
                                    | reading_profiles::ValueField::CropMargin
                                    | reading_profiles::ValueField::ScanGamma
                            )
                    )
                ) {
                    24
                } else if matches!(screen, Screen::NumberingValue(NumberField::AutoTurn)) {
                    5
                } else if matches!(screen, Screen::NumberingValue(NumberField::ScrollDelay)) {
                    4
                } else if matches!(
                    screen,
                    Screen::NumberingValue(
                        NumberField::AutoTurnOverlapHorizontal
                            | NumberField::AutoTurnOverlapVertical
                            | NumberField::CornerHoldPause
                    )
                ) {
                    2
                } else if matches!(
                    screen,
                    Screen::NumberingValue(
                        NumberField::FooterTrackMinimumWidth
                            | NumberField::FooterTrackMarginWidth
                            | NumberField::FooterBatteryHideThreshold
                            | NumberField::FooterItemFontSize
                            | NumberField::FooterItemWidth(_, _)
                            | NumberField::FooterContainerHeight
                            | NumberField::FooterContainerBottomMargin
                            | NumberField::FooterCustomTextRepetitions
                    )
                ) {
                    if matches!(
                        screen,
                        Screen::NumberingValue(
                            NumberField::FooterItemFontSize
                                | NumberField::FooterContainerHeight
                                | NumberField::FooterContainerBottomMargin
                                | NumberField::FooterCustomTextRepetitions
                        )
                    ) {
                        2
                    } else {
                        3
                    }
                } else {
                    4
                },
            );
            if matches!(screen, Screen::NumberingValue(NumberField::AutoTurn)) {
                let seconds = if self.controls.auto_turn_seconds == 0 {
                    30
                } else {
                    self.controls.auto_turn_seconds
                };
                self.editor.text = seconds.to_string().chars().collect();
                self.editor.caret = self.editor.text.len();
            }
            match screen {
                Screen::NumberingValue(NumberField::ScrollDelay) => {
                    self.set_scroll_delay_draft(self.scroll_delay_ms());
                }
                Screen::NumberingValue(NumberField::CornerHoldPause) => {
                    self.editor.text = gestures::corner_pause_seconds(&self.controls)
                        .to_string()
                        .chars()
                        .collect();
                    self.editor.caret = self.editor.text.len();
                }
                Screen::NumberingValue(NumberField::ReadingGuide(field)) => {
                    self.editor.text = field.value(self.controls.reading_guides).chars().collect();
                    self.editor.caret = self.editor.text.len();
                }
                Screen::NumberingValue(NumberField::FooterTrackMinimumWidth) => {
                    let width = self.controls.footer_track_min_width_pct.unwrap_or(20);
                    self.editor.text = width.to_string().chars().collect();
                    self.editor.caret = self.editor.text.len();
                }
                Screen::NumberingValue(NumberField::FooterTrackMarginWidth) => {
                    let width = self.controls.footer_track_margin_width.unwrap_or(10);
                    self.editor.text = width.to_string().chars().collect();
                    self.editor.caret = self.editor.text.len();
                }
                Screen::NumberingValue(NumberField::FooterBatteryHideThreshold) => {
                    let threshold = self.controls.footer_battery_hide_threshold.unwrap_or(100);
                    self.editor.text = threshold.to_string().chars().collect();
                    self.editor.caret = self.editor.text.len();
                }
                Screen::NumberingValue(NumberField::FooterItemFontSize) => {
                    let size = self.controls.footer_item_font_size.unwrap_or(14);
                    self.editor.text = size.to_string().chars().collect();
                    self.editor.caret = self.editor.text.len();
                }
                Screen::NumberingValue(NumberField::FooterItemWidth(_, width)) => {
                    self.editor.text = width.to_string().chars().collect();
                    self.editor.caret = self.editor.text.len();
                }
                Screen::NumberingValue(NumberField::FooterContainerHeight) => {
                    let height = self.controls.footer_container_height.unwrap_or(14);
                    self.editor.text = height.to_string().chars().collect();
                    self.editor.caret = self.editor.text.len();
                }
                Screen::NumberingValue(NumberField::FooterContainerBottomMargin) => {
                    let margin = self.controls.footer_container_bottom_margin.unwrap_or(1);
                    self.editor.text = margin.to_string().chars().collect();
                    self.editor.caret = self.editor.text.len();
                }
                Screen::NumberingValue(NumberField::FooterCustomTextRepetitions) => {
                    self.editor.text = self
                        .footer_custom_text
                        .repetitions
                        .to_string()
                        .chars()
                        .collect();
                    self.editor.caret = self.editor.text.len();
                }
                _ => {}
            }
            if let Screen::NumberingValue(
                field @ (NumberField::AutoTurnOverlapHorizontal
                | NumberField::AutoTurnOverlapVertical),
            ) = screen
                && let Some(fixed) = &self.fixed
            {
                let preferences = fixed.reading.preferences();
                let value = if field == NumberField::AutoTurnOverlapHorizontal {
                    preferences.zoom_overlap_h()
                } else {
                    preferences.zoom_overlap_v()
                };
                self.editor.text = value.to_string().chars().collect();
                self.editor.caret = self.editor.text.len();
            }
        }
        if let Screen::NumberingValue(NumberField::FixedTileZoom(axis)) = screen
            && let Some(fixed) = &self.fixed
        {
            let count = fixed
                .reading
                .preferences()
                .tile_zoom
                .filter(|tile| tile.axis == axis)
                .map_or(1.0, |tile| f32::from(tile.count_tenths) / 10.0);
            self.editor.text = format!("{count:.1}").chars().collect();
            self.editor.caret = self.editor.text.len();
        }
        if screen == Screen::NumberingValue(NumberField::CropMargin)
            && let Some(fixed) = &self.fixed
        {
            let margin = f32::from(fixed.reading.preferences().crop_margin()) / 100.0;
            self.editor.text = format!("{margin:.2}").chars().collect();
            self.editor.caret = self.editor.text.len();
        }
        if screen == Screen::GoToPage {
            self.editor = Editor::new(96);
        }
        if let Screen::ProfileName(mode) = screen {
            self.prepare_profile_name(mode);
        }
        if let Screen::ReadingProfileOpening(
            index,
            profile_panel::opening_rules::Page::TimeEdit(endpoint),
        ) = screen
        {
            self.prepare_profile_time(
                profile_panel::opening_rules::Event::Opening,
                index,
                endpoint,
            );
        }
        if let Screen::ReadingProfileClosing(
            index,
            profile_panel::opening_rules::Page::TimeEdit(endpoint),
        ) = screen
        {
            self.prepare_profile_time(
                profile_panel::opening_rules::Event::Closing,
                index,
                endpoint,
            );
        }
        if let Screen::ReadingProfileOpening(
            index,
            profile_panel::opening_rules::Page::CustomDelay,
        ) = screen
        {
            self.prepare_opening_delay(index);
        }
        if let Screen::DictionaryPresets(dictionary_presets::Page::Name(mode)) = screen {
            self.prepare_dictionary_preset_name(mode);
        }
        if let Screen::BookIdentityEdit(field) = screen {
            self.prepare_book_identity_edit(field);
        }
        if let Screen::FooterPresetName(mode) = screen {
            self.editor = Editor::new(footer_presets::MAX_NAME);
            if let FooterPresetName::Rename(index) = mode
                && let Some(preset) = self.footer_presets.get(index)
            {
                self.editor.text = preset.name.chars().collect();
                self.editor.caret = self.editor.text.len();
            }
        }
        if screen == Screen::FooterCustomText {
            self.editor = Editor::new(footer_custom_text::MAX_CHARS);
            self.editor.text = self.footer_custom_text.text.chars().collect();
            self.editor.caret = self.editor.text.len();
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
            self.retain_match_window();
        }
        self.link_popup = None;
        self.screen = screen;
        self.sync_alternative_contents();
        self.prepare_content_dpi_editor();
        self.prepare_logical_font_editor();
        self.prepare_line_spacing_editor();
        self.prepare_profile_value_editor();
        self.prepare_word_spacing_editor();
        self.prepare_footnote_font_editor();
        self.map_window = fixed_book_map::MapWindow::default();
        self.book_map_focus = if screen == Screen::BookMap {
            self.active.as_ref().map(|book| book.reading.chapter)
        } else {
            None
        };
        self.sync_auto_turn(Instant::now());
        self.update_stats_activity(Instant::now());
        if screen == Screen::Library {
            self.reading_history.finish(Instant::now());
            self.refresh_reading_order();
        }
        self.current_contents_row = None;
        if screen == Screen::PublisherPages {
            self.current_contents_row = self.visible_publisher_page();
        } else if screen.is_contents() {
            self.prepare_contents(previous != Screen::ContentsSearch);
        }
        self.list_page = if screen == Screen::Matches {
            self.reopen_match_window()
        } else if let Some(chapter) = self.book_map_focus {
            chapter / self.list_geometry().rows
        } else {
            let page_rows = if screen.is_contents() {
                self.contents_rows()
            } else {
                self.list_geometry().rows
            };
            self.current_contents_row.unwrap_or_default() / page_rows
        };
        self.notice = fit_error.map(|error| error.to_string());
        self.prepare_flexible_marks();
        self.update_footer_clock(Instant::now());
    }

    fn cancel_search(&mut self) {
        self.clear_flexible_results();
        self.nearby.clear();
        self.pdf_search = None;
        self.search_at = None;
    }

    fn record_position(&mut self) {
        self.record_position_at(Instant::now());
    }

    fn record_position_at(&mut self, now: Instant) {
        if self.pending_opening_profiles.is_some()
            || self.fixed_search_fit_active()
            || self.crop_dialog.is_some()
            || self.semi_auto_crop.is_some()
        {
            return;
        }
        self.record_read_view();
        self.update_stats_activity(now);
        let reading_percent = self.folder_reading_percent();
        let default_orientation = self.default_reading_orientation();
        let document_turns = self.default_document_turns();
        let position_changed = if let Some(fixed) = &self.fixed {
            let saved = self
                .library
                .remember(&fixed.entry, fixed.id, &fixed.entry.title);
            saved
                .highlight_color
                .get_or_insert(self.controls.highlight_color);
            saved.screen_orientation.get_or_insert(default_orientation);
            saved.document_turns.fill_missing(document_turns);
            let position = fixed_position(&fixed.reading);
            let changed = saved.position.as_ref() != Some(&position)
                || saved.reading_percent != reading_percent;
            saved.position = Some(position);
            saved.reading_percent = reading_percent;
            saved.fixed_preferences = fixed.reading.preferences();
            saved.ocr_language.clone_from(&fixed.reading.ocr_language);
            changed
        } else if let Some(active) = &self.active {
            let title = &active.reading.probe.title;
            let saved = self.library.remember(&active.entry, active.id, title);
            saved
                .highlight_color
                .get_or_insert(self.controls.highlight_color);
            saved.screen_orientation.get_or_insert(default_orientation);
            saved.document_turns.fill_missing(document_turns);
            let position = position(&active.reading);
            let changed = saved.position.as_ref() != Some(&position)
                || saved.reading_percent != reading_percent;
            saved.position = Some(position);
            saved.reading_percent = reading_percent;
            changed
        } else {
            return;
        };
        self.dirty = true;
        self.metadata_checkpoint
            .record(self.controls.metadata_save, position_changed, now);
    }

    fn mark_dirty(&mut self) {
        self.dirty = true;
        self.save_at = Some(Instant::now() + SAVE_DELAY);
    }

    fn update_stats_activity(&mut self, now: Instant) {
        let reading = self.stats_awake
            && self.quickstart.is_none()
            && self.closing_profile.is_none()
            && self.pending_opening_profiles.is_none()
            && self.end_dialog.is_none()
            && !self.fixed_search_fit_active();
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
            options.enabled
                && (!options.freeze_finished
                    || self
                        .library
                        .state(*id)
                        .is_none_or(|state| state.status() != ReadingStatus::Finished))
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
        let pace_generation = book.map_or(0, |id| self.reading_history.pace_generation(id));
        self.pace.set_page(page, now, pace_generation);
        if let Some(id) = book {
            let title = self
                .active
                .as_ref()
                .filter(|b| b.id == id)
                .map(|b| b.entry.title.as_str())
                .or_else(|| {
                    self.fixed
                        .as_ref()
                        .filter(|b| b.id == id)
                        .map(|b| b.entry.title.as_str())
                });
            if let Some(title) = title {
                self.reading_history.remember_title(id, title);
            }
        }
        let history_page = page.map(|(book, page, _)| (book, page));
        self.reading_history.set_page(history_page, now);
    }

    fn save_library(&mut self) -> io::Result<()> {
        self.save_library_at(Instant::now())
    }

    fn save_library_at(&mut self, now: Instant) -> io::Result<()> {
        if self.dirty || self.library.live_metadata_pending() {
            self.library.persist()?;
            self.dirty = false;
            if let Some(warning) = self.library.take_live_metadata_warning() {
                self.notice = Some(warning);
            }
        }
        self.save_at = None;
        self.metadata_checkpoint = metadata_save::Checkpoint::new(now);
        Ok(())
    }

    fn list_window(&self) -> (usize, Vec<Item>) {
        if self.screen.is_contents() {
            let rows = self.contents_rows();
            return self.contents_items(self.list_page * rows, rows);
        }
        let rows = self.list_geometry().rows;
        let first = self.list_page * rows;
        if let Screen::BookmarkBrowser(page) = self.screen {
            return self.bookmark_window(page, first, rows);
        }
        if self.screen == Screen::PublisherPages {
            return self.publisher_page_window(first, rows);
        }
        if let Screen::ReadingHistory(page) = self.screen
            && let Some(report) = self.history_report_window(page, first, rows)
        {
            return report;
        }
        if self.screen == Screen::Matches {
            let range = self
                .flexible_result_range()
                .unwrap_or(first..first.saturating_add(rows));
            let order = self.search_result_order();
            let indices = range.filter_map(|ordinal| order.provider_index(ordinal));
            if let Some(fixed) = &self.fixed {
                let items = indices
                    .map(|index| {
                        let hit = &self.pdf_hits[index];
                        Item::new(&hit.snippet, Action::PdfSearchHit(index))
                            .detail(fixed_page_label(&fixed.reading, hit.page))
                    })
                    .collect();
                return (order.len(), items);
            }
            let items = indices
                .map(|index| {
                    let hit = &self.hits[index];
                    Item::new(&hit.snippet, Action::SearchHit(index)).detail(&hit.chapter_title)
                })
                .collect();
            return (order.len(), items);
        }
        let all = self.items();
        (all.len(), all.into_iter().skip(first).take(rows).collect())
    }

    fn navigate_list_page(&mut self, page: usize) {
        self.list_page = page;
        self.reading_list_anchor = None;
        match self.screen {
            Screen::Matches => self.remember_match_window(page),
            Screen::BookmarkBrowser(_) => self.bookmark_browser.reset_list_anchor(),
            _ => {}
        }
    }

    fn has_list_shortcuts(&self, count: usize) -> bool {
        matches!(self.screen, Screen::Matches | Screen::PublisherPages)
            && count > self.list_geometry().rows
    }

    fn items(&self) -> Vec<Item> {
        self.items_for(self.screen)
    }

    fn items_for(&self, screen: Screen) -> Vec<Item> {
        if screen == Screen::MenuSearchResults {
            return self.menu_search.items();
        }
        if matches!(
            screen,
            Screen::PageTurns | Screen::PageTurnZones | Screen::PageTurnDefaults
        ) {
            return self.page_turn_items(screen);
        }
        if screen == Screen::NavigationSettings {
            return self.navigation_preference_items();
        }
        if let Screen::BackOptions(options) = screen {
            return self.back_option_items(options);
        }
        if screen == Screen::ReadingGuides {
            return self.reading_guide_items();
        }
        if matches!(
            screen,
            Screen::ReadingProfiles
                | Screen::ReadingProfile(_)
                | Screen::ReadingProfileFields(_)
                | Screen::ReadingProfileValues(_)
                | Screen::ReadingProfileMargins(_)
                | Screen::ReadingProfileFontRendering(_)
                | Screen::ReadingProfileBookRendering(_)
                | Screen::ReadingProfileChoice(_, _)
                | Screen::ReadingProfileOpening(_, _)
                | Screen::ReadingProfileClosing(..)
                | Screen::ConfirmProfile(_, _)
        ) {
            return self.reading_profile_items();
        }
        if screen == Screen::ReadingGuidesAbout {
            return vec![
                Item::new("Two vertical lines", Action::Heading)
                    .detail("Use them to mark the area to focus on while reading."),
                Item::new("Moving guides", Action::Heading)
                    .detail("The lines can move inward as you turn pages. Set the interval to 0 to keep them still."),
            ];
        }
        if screen.is_contents() {
            return self.contents_items(0, usize::MAX).1;
        }
        if screen == Screen::ContentsSettings {
            return self.contents_settings_items();
        }
        if screen == Screen::SearchSettings {
            let mut items = vec![
                Item::new(
                    "Show all results on text selection",
                    Action::SetReadingControls(ReadingControls {
                        selection_search_all: !self.controls.selection_search_all,
                        ..self.controls
                    }),
                )
                .detail("Open the results list when you tap Find on selected text"),
                Item::new(
                    "Fit fixed pages during search",
                    Action::SetReadingControls(ReadingControls {
                        fixed_search_full_page: !self.controls.fixed_search_full_page,
                        ..self.controls
                    }),
                )
                .detail("Temporarily show the whole page when finding the next or previous match"),
            ];
            items.extend(self.search_presentation_items());
            return items;
        }
        if matches!(
            screen,
            Screen::StyleTweaks
                | Screen::ParagraphSpacing
                | Screen::LinkStyles
                | Screen::FootnoteTweaks
                | Screen::StylePreset
                | Screen::ImageStyles
                | Screen::TableStyles
                | Screen::PseudoElementStyles
                | Screen::ListItemStyles
                | Screen::InPageFootnoteSize(_)
                | Screen::StyleFiles
                | Screen::StyleReview
                | Screen::StyleClear
                | Screen::StyleEditor
        ) {
            return self.style_items(screen);
        }
        if let Screen::ReadingHistory(page) = screen {
            return self.reading_history_items(page);
        }
        if matches!(
            screen,
            Screen::Vocabulary
                | Screen::VocabularyWord
                | Screen::VocabularyOptions
                | Screen::VocabularyBooks
        ) {
            return self.vocabulary_items();
        }
        if let Screen::LibraryExport(page) = screen {
            return self.library_export_items(page);
        }
        if screen == Screen::HiddenFlows {
            return self.hidden_flow_items();
        }
        if screen == Screen::ChapterNavigation {
            return self.chapter_level_items();
        }
        if matches!(
            screen,
            Screen::CustomContents | Screen::CustomEdits | Screen::CustomBookmarks
        ) {
            return self.custom_items();
        }
        match screen {
            Screen::FooterSettings => return self.footer_items(),
            Screen::FooterItemWidths => return self.footer_item_widths(),
            Screen::HeaderBatterySettings => return self.header_battery_items(),
            Screen::FooterPresets => {
                let mut items = vec![
                    Item::new(
                        "Save current status bar",
                        Action::Show(Screen::FooterPresetName(FooterPresetName::Create)),
                    )
                    .detail("Save the current layout and progress bar options"),
                ];
                items.extend(
                    self.footer_presets
                        .iter()
                        .enumerate()
                        .map(|(index, preset)| {
                            Item::new(&preset.name, Action::Show(Screen::FooterPreset(index)))
                                .detail("Open preset actions")
                        }),
                );
                return items;
            }
            Screen::FooterCustomTextGuide => {
                return [
                    ("%T", "Book title"),
                    ("%A", "Author"),
                    ("%S", "Series name and position"),
                    ("%t", "Total pages"),
                    ("%c", "Current page"),
                    ("%l", "Pages left in the current chapter"),
                    ("%p", "Book progress as a whole percentage"),
                    ("%H", "Estimated time left in the book"),
                    ("%C", "Current chapter title"),
                    ("%P", "Chapter progress as a whole percentage"),
                    ("%h", "Estimated time left in the chapter"),
                    ("%F", "Full file path"),
                    ("%f", "File name"),
                    ("%b", "Battery percentage"),
                    ("%B", "KOReader battery or charging symbol"),
                    ("%r", "Selected status-item separator"),
                    ("%D", "Date in year-month-day format"),
                    ("%d", "Date in month-day format"),
                    ("%m", "Current time in hour:minute format"),
                    ("%M", "Current time with seconds"),
                    ("%%", "Two percent signs stay literal"),
                ]
                .into_iter()
                .map(|(token, description)| Item::new(token, Action::Heading).detail(description))
                .collect();
            }
            Screen::FooterPreset(index) => {
                let Some(preset) = self.footer_presets.get(index) else {
                    return Vec::new();
                };
                return vec![
                    Item::new("Apply to this Reader", Action::ApplyFooterPreset(index)),
                    Item::new(
                        "Update from current status bar",
                        Action::UpdateFooterPreset(index),
                    ),
                    Item::new("Rename", Action::RenameFooterPreset(index)),
                    Item::new("Delete", Action::Show(Screen::FooterPresetDelete(index))),
                    Item::new(&preset.name, Action::Heading).detail("Saved status bar preset"),
                ];
            }
            Screen::FooterPresetDelete(index) => {
                let name = self
                    .footer_presets
                    .get(index)
                    .map_or("this preset", |preset| preset.name.as_str());
                return vec![
                    Item::new(format!("Delete {name}"), Action::DeleteFooterPreset(index)),
                    Item::new("Keep preset", Action::Show(Screen::FooterPreset(index))),
                ];
            }
            Screen::BookmarkListSettings => {
                let mut items = vec![
                    Item::new(
                        "Sort by page",
                        Action::SetReadingControls(ReadingControls {
                            bookmark_sort_by_date: false,
                            ..self.controls
                        }),
                    )
                    .detail(if self.controls.bookmark_sort_by_date {
                        "Document reading order"
                    } else {
                        "Selected"
                    }),
                    Item::new(
                        "Sort by date",
                        Action::SetReadingControls(ReadingControls {
                            bookmark_sort_by_date: true,
                            ..self.controls
                        }),
                    )
                    .detail(if self.controls.bookmark_sort_by_date {
                        "Selected"
                    } else {
                        "Newest and oldest by saved time"
                    }),
                    Item::new(
                        "Reverse sorting",
                        Action::SetReadingControls(ReadingControls {
                            bookmark_reverse: !self.controls.bookmark_reverse,
                            ..self.controls
                        }),
                    )
                    .detail(if self.controls.bookmark_reverse {
                        "On"
                    } else {
                        "Off"
                    }),
                ];
                items.push(
                    Item::new(
                        "Show in highlight rows",
                        Action::Show(Screen::AnnotationListSettings),
                    )
                    .detail(self.controls.annotation_list_text.label()),
                );
                items.push(self.bookmark_lines_item());
                items.extend(self.list_density_items(list_density::Scope::Bookmarks));
                let flexible = self.controls.bookmark_max_lines().is_some();
                let shrink_action = if flexible {
                    Action::Heading
                } else {
                    Action::SetReadingControls(ReadingControls {
                        bookmark_shrink_to_fit: !self.controls.bookmark_shrink_to_fit,
                        ..self.controls
                    })
                };
                items.push(
                    Item::new("Shrink bookmark font size to fit more text", shrink_action).detail(
                        if flexible {
                            "Unavailable with flexible rows"
                        } else if self.controls.bookmark_shrink_to_fit {
                            "On"
                        } else {
                            "Off"
                        },
                    ),
                );
                items.extend(self.bookmark_appearance_settings_items());
                items.push(Item::new(
                    "Annotation exchange",
                    Action::Show(Screen::AnnotationExchange),
                ));
                return items;
            }
            Screen::AnnotationExchange => return self.annotation_exchange_settings(),
            Screen::AnnotationExchangePreview => return self.annotation_exchange_preview_items(),
            Screen::AnnotationListSettings => {
                return crate::reading_controls::AnnotationListText::ALL
                    .into_iter()
                    .map(|display| {
                        let item = Item::new(
                            display.label(),
                            Action::SetReadingControls(ReadingControls {
                                annotation_list_text: display,
                                ..self.controls
                            }),
                        );
                        if display == self.controls.annotation_list_text {
                            item.detail("Selected")
                        } else {
                            item
                        }
                    })
                    .collect();
            }
            Screen::SelectionPosition => {
                return crate::reading_controls::SelectionPopupPosition::ALL
                    .into_iter()
                    .map(|position| {
                        let item = Item::new(
                            position.label(),
                            Action::SetReadingControls(ReadingControls {
                                selection_popup_position: position,
                                ..self.controls
                            }),
                        );
                        if position == self.controls.selection_popup_position {
                            item.detail("Selected")
                        } else {
                            item
                        }
                    })
                    .collect();
            }
            Screen::LongPressSettings => {
                let dictionary = self.controls.dictionary_on_single_word.unwrap_or(true);
                let corner_scroll = self.controls.selection_corner_scroll_enabled();
                let mut items = vec![
                    Item::new(
                        "Dictionary on a single word",
                        Action::SetReadingControls(ReadingControls {
                            dictionary_on_single_word: Some(!dictionary),
                            ..self.controls
                        }),
                    )
                    .detail(if dictionary {
                        "On · opens the local dictionary"
                    } else {
                        "Off · uses the default action"
                    }),
                    Item::new(
                        "Auto-scroll when selection reaches a corner",
                        Action::SetReadingControls(ReadingControls {
                            selection_corner_scroll: Some(!corner_scroll),
                            ..self.controls
                        }),
                    )
                    .detail(if corner_scroll { "On" } else { "Off" }),
                    Item::new(
                        "Highlight style and color prompt",
                        Action::Show(Screen::HighlightPromptSettings),
                    )
                    .detail(self.controls.highlight_prompt.label()),
                    Item::new(
                        "Very long press interval",
                        Action::Show(Screen::VeryLongPressSettings),
                    )
                    .detail(self.controls.very_long_press_label()),
                ];
                items.extend(SelectionLongPressAction::ALL.into_iter().map(|action| {
                    let item = Item::new(
                        action.label(),
                        Action::SetReadingControls(ReadingControls {
                            selection_long_press_action: action,
                            ..self.controls
                        }),
                    );
                    if action == self.controls.selection_long_press_action {
                        item.detail("Selected")
                    } else {
                        item
                    }
                }));
                return items;
            }
            Screen::VeryLongPressSettings => {
                let current = self.controls.very_long_press_tenths();
                let mut choices = Vec::new();
                for (label, delta) in [
                    ("−0.5 seconds", -5i32),
                    ("−0.1 seconds", -1),
                    ("+0.1 seconds", 1),
                    ("+0.5 seconds", 5),
                ] {
                    let value = (i32::from(current) + delta).clamp(
                        i32::from(crate::reading_controls::MIN_VERY_LONG_PRESS_TENTHS),
                        i32::from(crate::reading_controls::MAX_VERY_LONG_PRESS_TENTHS),
                    ) as u16;
                    choices.push(
                        Item::new(
                            label,
                            Action::SetReadingControls(ReadingControls {
                                very_long_press_tenths: Some(value),
                                ..self.controls
                            }),
                        )
                        .detail(format!(
                            "{}.{:01} seconds",
                            value / 10,
                            value % 10
                        )),
                    );
                }
                choices.push(
                    Item::new(
                        "Reset to default",
                        Action::SetReadingControls(ReadingControls {
                            very_long_press_tenths: None,
                            ..self.controls
                        }),
                    )
                    .detail("3.0 seconds"),
                );
                return choices;
            }
            Screen::HighlightPromptSettings => {
                return HighlightPrompt::ALL
                    .into_iter()
                    .map(|prompt| {
                        let item = Item::new(
                            prompt.label(),
                            Action::SetReadingControls(ReadingControls {
                                highlight_prompt: prompt,
                                ..self.controls
                            }),
                        );
                        if prompt == self.controls.highlight_prompt {
                            item.detail("Selected")
                        } else {
                            item
                        }
                    })
                    .collect();
            }
            Screen::Gestures => return self.reading_gesture_items(),
            Screen::MenuActivationSettings => return self.menu_activation_items(),
            Screen::Scrolling => return self.scroll_settings_items(),
            Screen::GestureBinding(gesture) => {
                return self.gestures.choices(gesture, &self.book_shortcuts);
            }
            Screen::Multiswipes
            | Screen::CustomMultiswipes
            | Screen::RecordMultiswipe
            | Screen::MultiswipeBinding(_)
            | Screen::RemoveMultiswipe(_) => {
                return self
                    .gestures
                    .multiswipes
                    .items(screen, &self.book_shortcuts);
            }
            Screen::GestureOverview => return self.gesture_overview_items(),
            Screen::BookMap => return self.book_map_items(),
            Screen::AutoTurn => {
                let selected = self.auto_turn.interval();
                let mut items: Vec<_> = [0, 10, 30, 60, 120]
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
                let current = self.controls.auto_turn_seconds;
                items.push(
                    Item::new(
                        "Custom interval",
                        Action::Show(Screen::NumberingValue(NumberField::AutoTurn)),
                    )
                    .detail(if current > 0 {
                        format!("{current} seconds")
                    } else {
                        "Enter 1 to 86,400 seconds".to_owned()
                    }),
                );
                items.push(
                    Item::new("Scroll distance", Action::Show(Screen::AutoTurnDistance)).detail(
                        format!(
                            "{:+}% of a page",
                            i32::from(self.controls.auto_turn_distance_tenths()) * 10
                        ),
                    ),
                );
                if self.fixed.is_some() {
                    let preferences = self.fixed.as_ref().unwrap().reading.preferences();
                    items.push(
                        Item::new("Fixed-page pan", Action::Show(Screen::AutoTurnPan)).detail(
                            format!(
                                "{}% horizontal · {}% vertical overlap",
                                preferences.zoom_overlap_h(),
                                preferences.zoom_overlap_v()
                            ),
                        ),
                    );
                }
                return items;
            }
            Screen::AutoTurnDistance => {
                return vec![
                    Item::new("Decrease by 50%", Action::AdjustAutoTurnDistance(-5))
                        .detail("Move 50 percentage points back"),
                    Item::new("Decrease by 10%", Action::AdjustAutoTurnDistance(-1))
                        .detail("Move 10 percentage points back"),
                    Item::new("Reset to 100%", Action::ResetAutoTurnDistance)
                        .detail("One visible page at a time"),
                    Item::new("Increase by 10%", Action::AdjustAutoTurnDistance(1))
                        .detail("Move 10 percentage points forward"),
                    Item::new("Increase by 50%", Action::AdjustAutoTurnDistance(5))
                        .detail("Move 50 percentage points forward"),
                    Item::new("Current distance", Action::Heading).detail(format!(
                        "{:+}% of a page · KOReader range −2,000% to +2,000%",
                        i32::from(self.controls.auto_turn_distance_tenths()) * 10
                    )),
                ];
            }
            Screen::AutoTurnPan => {
                let Some(fixed) = &self.fixed else {
                    return vec![Item::new(
                        "Open a fixed-page document to change pan behavior",
                        Action::Heading,
                    )];
                };
                let preferences = fixed.reading.preferences();
                return vec![
                    Item::new(
                        "Horizontal overlap",
                        Action::Show(Screen::NumberingValue(
                            NumberField::AutoTurnOverlapHorizontal,
                        )),
                    )
                    .detail(format!("{}% · 0–90%", preferences.zoom_overlap_h())),
                    Item::new(
                        "Vertical overlap",
                        Action::Show(Screen::NumberingValue(NumberField::AutoTurnOverlapVertical)),
                    )
                    .detail(format!("{}% · 0–90%", preferences.zoom_overlap_v())),
                    Item::new(
                        "Pan order",
                        Action::AutoTurnPan(AutoTurnPanChange::ToggleVerticalFirst),
                    )
                    .detail(if preferences.zoom_direction_vertical() {
                        "Vertical first · horizontal at the edge · tap to change"
                    } else {
                        "Horizontal first · vertical at the edge · tap to change"
                    }),
                    Item::new(
                        "Vertical direction",
                        Action::AutoTurnPan(AutoTurnPanChange::ToggleBottomToTop),
                    )
                    .detail(if preferences.zoom_bottom_to_top() {
                        "Bottom to top · tap to reverse"
                    } else {
                        "Top to bottom · tap to reverse"
                    }),
                    Item::new("Horizontal reading direction", Action::Heading).detail(
                        if preferences.right_to_left {
                            "Right to left · change in View settings"
                        } else {
                            "Left to right · change in View settings"
                        },
                    ),
                    Item::new(
                        "Use document defaults",
                        Action::AutoTurnPan(AutoTurnPanChange::Reset),
                    ),
                ];
            }
            Screen::OnlineHistory | Screen::ClearOnlineHistory => {
                return self.online_history_items();
            }
            Screen::TranslationSettings => {
                return self.translation_settings_items();
            }
            Screen::WikipediaSettings => return self.wikipedia_settings_items(),
            Screen::SearchHistory => {
                return self
                    .lookup_history
                    .searches
                    .iter()
                    .map(|query| {
                        let item = Item::new(&query.text, Action::RepeatSearch(query.clone()));
                        if query.regular_expression {
                            item.detail("Regular expression")
                        } else {
                            item
                        }
                    })
                    .collect();
            }
            Screen::DictionaryHistory => {
                let mut items = Vec::new();
                let mut preceding_title = None;
                for lookup in &self.lookup_history.dictionary {
                    let title = lookup.book_title();
                    if let Some(title) = title.filter(|_| title != preceding_title) {
                        items.push(Item::new(title, Action::Heading));
                    }
                    preceding_title = title;
                    let word = lookup.word();
                    let row = Item::new(word, Action::LookUp(word.to_owned()));
                    items.push(
                        match lookup.time().and_then(crate::mark_time::local_datetime) {
                            Some(time) => row.detail(time),
                            None => row,
                        },
                    );
                }
                return items;
            }
            Screen::BookmarkMatches => return self.annotation_results(),
            _ => {}
        }
        if screen == Screen::Library {
            return self.library_items().collect();
        }
        if screen == Screen::FolderOrder {
            return self.folder_order_items();
        }
        if matches!(screen, Screen::BookArchive | Screen::ArchiveFolder) {
            return self.archive_items();
        }
        if let Screen::MetadataArchive(page) = screen {
            return self.metadata_archive_items(page);
        }
        if let Screen::BookShortcuts(page) = screen {
            return self.book_shortcut_items(page);
        }
        if let Screen::LiveMetadata(page) = screen {
            return self.live_metadata_items(page);
        }
        if let Some(items) = self.organization_items() {
            return items;
        }
        if screen == Screen::LocationHistory {
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
        if screen == Screen::ReadingControls {
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
        if let Some(items) = self.notification_items(screen) {
            return items;
        }
        if screen == Screen::Links {
            return self.page_link_items();
        }
        if screen == Screen::LinkSettings {
            return self.link_settings_items();
        }
        if screen == Screen::EndActionSettings {
            return self.end_action_rows();
        }
        if screen == Screen::MetadataSaveSettings {
            return self.metadata_save_items();
        }
        if matches!(
            screen,
            Screen::Dictionaries | Screen::ClearDictionaryHistory
        ) {
            return self.source_rows();
        }
        if let Screen::DictionaryOptions(i) = screen {
            return self.source_options(i);
        }
        if let Screen::DictionaryButtons(page) = screen {
            return self.dictionary_button_items(page);
        }
        if let Screen::DictionaryPresets(page) = screen {
            return self.dictionary_preset_items(page);
        }
        if screen == Screen::ReadingData {
            let mut items = vec![
                self.metadata_archive_item(),
                self.live_metadata_item(),
                Item::new(
                    "Export highlights",
                    Action::LibraryExport(library_export::Command::Open),
                )
                .detail("From History, chosen books or collections"),
                Item::new("Back up reading data", Action::ExportBackup)
                    .detail("Books, vocabulary, history and chosen local preferences"),
                Item::new("Restore a backup", Action::PreviewBackup)
                    .detail("Copy a backup to reader/data/reader-import.json"),
                Item::new("Clear recognized text", Action::Show(Screen::ClearOcr))
                    .detail("Clear saved OCR text from document pages"),
            ];
            if self.open_book_id().is_some() {
                items.push(
                    Item::new(
                        "Import a copied KOReader position",
                        Action::PreviewCopiedPosition,
                    )
                    .detail("From reader/data/koreader-progress.lua"),
                );
            }
            items.push(
                Item::new(
                    "Reading statistics",
                    Action::ReadingHistory(reading_history::Command::Open(None)),
                )
                .detail("Daily history, calendar, and book totals"),
            );
            return items;
        }
        if screen == Screen::ClearOcr {
            return vec![
                Item::new("Clear OCR cache", Action::ClearOcr),
                Item::new("Keep OCR cache", Action::Show(Screen::ReadingData)),
            ];
        }
        if screen == Screen::RestoreBackup {
            return self.reading_backup_items();
        }
        if screen == Screen::ImportPosition {
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
        if screen == Screen::ImportPdfAnnotations {
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
        if let Screen::ExportSettings(origin, page) = screen {
            return self.export_settings_items(origin, page);
        }
        if screen == Screen::ExportAnnotations {
            return annotation_export::Format::ALL
                .into_iter()
                .map(|format| Item::new(format.label(), Action::ExportHighlights(format)))
                .chain([
                    Item::new("Kindle clippings text", Action::ExportBookClippings),
                    Item::new(
                        "Export settings",
                        Action::ExportSettings(
                            export_settings::Origin::Book,
                            export_settings::Command::Open,
                        ),
                    ),
                ])
                .collect();
        }
        if screen == Screen::ImportAnnotations
            && let Some(import) = &self.nearby_marks
        {
            return import.items();
        }
        if screen == Screen::ImportAnnotations
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
        if screen == Screen::Annotation {
            let passage = if self.fixed.is_some() {
                Action::PdfHighlight(self.highlight_index)
            } else {
                Action::Highlight(self.highlight_index)
            };
            let mut items = vec![
                Item::new("Read passage", passage),
                Item::new("Extend highlight", Action::ExtendHighlight),
                Item::new("Edit note", Action::EditNote),
                Item::new("Remove highlight", Action::Show(Screen::RemoveHighlight)),
            ];
            if let Some(state) = self.open_book_id().and_then(|id| self.library.state(id)) {
                let chapter_title = if self.fixed.is_some() {
                    state
                        .pdf_highlights
                        .get(self.highlight_index)
                        .and_then(|mark| mark.chapter_title.as_deref())
                } else {
                    state
                        .highlights
                        .get(self.highlight_index)
                        .and_then(|mark| mark.chapter_title.as_deref())
                };
                if let Some(title) = chapter_title {
                    items.push(Item::new("Chapter", Action::Heading).detail(title));
                }
            }
            return items;
        }
        if screen == Screen::RemoveHighlight {
            return vec![
                Item::new("Remove highlight and note", Action::RemoveHighlight),
                Item::new("Keep highlight", Action::Show(Screen::Annotation)),
            ];
        }
        if let Some(fixed) = &self.fixed {
            return match screen {
                Screen::FixedBookmarks => self.bookmark_items(fixed.id),
                Screen::Highlights => self.bookmark_items(fixed.id),
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
        match screen {
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
            Screen::Bookmarks => self.bookmark_items(active.id),
            Screen::Typography => {
                let settings = reading.config.settings;
                let mut items = text_items(
                    settings,
                    reading.config.scroll,
                    active.id,
                    self.pending_logical_size(active.id),
                );
                if !reading.probe.is_vertical()
                    && settings.hyphenation_mode() != crate::HyphenationMode::Off
                {
                    let enabled = self.user_hyphenation.enabled;
                    items.push(
                        Item::new(
                            "Custom hyphenation rules",
                            Action::SetUserHyphenation(!enabled),
                        )
                        .detail(if enabled { "On" } else { "Off" }),
                    );
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
            Screen::ReadingFonts => self.reading_font_rows(),
            Screen::FooterFontFaces => {
                let mut items = vec![
                    Item::new("Device default", Action::SetFooterFontFace(None)).detail(
                        if self.footer_font_file.is_none() {
                            "Selected · Noto Sans"
                        } else {
                            "Noto Sans"
                        },
                    ),
                ];
                items.extend(self.available_fonts.iter().map(|filename| {
                    Item::new(filename, Action::SetFooterFontFace(Some(filename.clone()))).detail(
                        if self.footer_font_file.as_deref() == Some(filename) {
                            "Selected"
                        } else {
                            ""
                        },
                    )
                }));
                items.push(
                    Item::new("Add more fonts", Action::Heading)
                        .detail("Copy TTF or OTF files to reader/data/fonts"),
                );
                items
            }
            Screen::FontRendering(scope) => return self.font_rendering_items(scope),
            Screen::PublisherRenderDefaults => return self.publisher_render_default_items(),
            Screen::TypographyRules => return self.typography_items(),
            Screen::HyphenationOptions => return self.hyphenation_items(),
            Screen::HyphenationDefaults => return self.hyphenation_defaults_items(),
            Screen::HyphenationLimits(draft) => return self.hyphenation_limit_items(draft),
            Screen::HyphenationLimitValue(draft, side) => {
                return self.hyphenation_limit_values(draft, side);
            }
            Screen::TypographyLanguages(scope) => return self.typography_language_items(scope),
            Screen::DocumentDefaults
            | Screen::StyleTweaks
            | Screen::ParagraphSpacing
            | Screen::LinkStyles
            | Screen::FootnoteTweaks
            | Screen::StylePreset
            | Screen::ImageStyles
            | Screen::TableStyles
            | Screen::PseudoElementStyles
            | Screen::ListItemStyles
            | Screen::InPageFootnoteSize(_)
            | Screen::StyleFiles
            | Screen::StyleReview
            | Screen::StyleClear
            | Screen::StyleEditor => Vec::new(),
            Screen::Highlights => self.bookmark_items(active.id),
            _ => Vec::new(),
        }
    }

    fn bookmark_items(&self, id: u64) -> Vec<Item> {
        let full_excerpt =
            self.controls.bookmark_shrink_to_fit || self.controls.bookmark_max_lines().is_some();
        let state = self.library.state(id);
        let active = self.active.as_ref().filter(|book| book.id == id);
        let fixed = self.fixed.as_ref().filter(|book| book.id == id);
        let mut chapter_layouts = HashMap::new();
        let mut rows = Vec::new();
        let has_text_annotations = state
            .is_some_and(|state| !state.highlights.is_empty() || !state.pdf_highlights.is_empty());

        if let Some(state) = state {
            for (index, bookmark) in state.bookmarks.iter().enumerate() {
                let location = if let Some(active) = active {
                    reflow_mark_location(
                        active,
                        bookmark.position.chapter,
                        bookmark.position.page,
                        bookmark.position.anchor.as_ref(),
                        &mut chapter_layouts,
                    )
                } else {
                    (0, bookmark.position.page, 0, 0)
                };
                let caption = match active {
                    Some(active) if bookmark.chapter_title.is_none() => {
                        reflow_location_label(active, location, None)
                    }
                    _ => format!("Page {}", location.1 + 1),
                };
                let detail = saved_mark_detail(
                    bookmark.created_at,
                    format!("Page bookmark · {caption}"),
                    self.controls.bookmark_sort_by_date,
                );
                rows.push(SavedMarkRow {
                    created_at: bookmark.created_at.unwrap_or_default(),
                    location,
                    kind: 0,
                    storage_index: index,
                    item: Item::new(
                        bookmark
                            .chapter_title
                            .as_deref()
                            .map_or_else(|| bookmark.label.clone(), |title| format!("In {title}")),
                        Action::Bookmark(index),
                    )
                    .detail(detail),
                });
            }

            if let Some(active) = active {
                for (index, mark) in state.highlights.iter().enumerate() {
                    let location = reflow_mark_location(
                        active,
                        0,
                        0,
                        Some(&mark.selection.start),
                        &mut chapter_layouts,
                    );
                    let caption =
                        reflow_location_label(active, location, mark.chapter_title.as_deref());
                    let note = mark.note.as_deref().filter(|note| !note.trim().is_empty());
                    let excerpt = bookmark_excerpt(&mark.selection.text, full_excerpt);
                    let title = match self.controls.annotation_list_text {
                        AnnotationListText::HighlightedText
                        | AnnotationListText::HighlightAndNote => excerpt,
                        AnnotationListText::NoteIfSet => note.unwrap_or(&excerpt).to_owned(),
                    };
                    let detail = if self.controls.annotation_list_text
                        == AnnotationListText::HighlightAndNote
                        && let Some(note) = note
                    {
                        format!("Note: {note}")
                    } else {
                        if note.is_some() {
                            "Note".into()
                        } else {
                            "Highlight".into()
                        }
                    };
                    let detail = saved_mark_detail(
                        mark.created_at,
                        format!("{detail} · {caption}"),
                        self.controls.bookmark_sort_by_date,
                    );
                    rows.push(SavedMarkRow {
                        created_at: mark.created_at.unwrap_or_default(),
                        location,
                        kind: 1,
                        storage_index: index,
                        item: Item::new(title, Action::Highlight(index)).detail(detail),
                    });
                }
            }

            if fixed.is_some() {
                for (index, mark) in state.pdf_highlights.iter().enumerate() {
                    let location = (0, mark.page, 0, 0);
                    let caption = format!("Page {}", mark.page + 1);
                    let note = mark.note.as_deref().filter(|note| !note.trim().is_empty());
                    let excerpt = bookmark_excerpt(&mark.text, full_excerpt);
                    let title = match self.controls.annotation_list_text {
                        AnnotationListText::HighlightedText
                        | AnnotationListText::HighlightAndNote => excerpt,
                        AnnotationListText::NoteIfSet => note.unwrap_or(&excerpt).to_owned(),
                    };
                    let detail = if self.controls.annotation_list_text
                        == AnnotationListText::HighlightAndNote
                        && let Some(note) = note
                    {
                        format!("Note: {note}")
                    } else {
                        if note.is_some() {
                            "Note".into()
                        } else {
                            "Highlight".into()
                        }
                    };
                    let detail = saved_mark_detail(
                        mark.created_at,
                        mark_chapters::location_caption(
                            format!("{detail} · {caption}"),
                            mark.chapter_title.as_deref(),
                        ),
                        self.controls.bookmark_sort_by_date,
                    );
                    rows.push(SavedMarkRow {
                        created_at: mark.created_at.unwrap_or_default(),
                        location,
                        kind: 2,
                        storage_index: index,
                        item: Item::new(title, Action::PdfHighlight(index)).detail(detail),
                    });
                }
            }
        }

        if self.controls.bookmark_sort_by_date {
            rows.sort_by_key(|row| {
                (
                    row.created_at,
                    row.location.0,
                    row.location.1,
                    row.location.2,
                    row.location.3,
                    row.kind,
                    row.storage_index,
                )
            });
        } else {
            rows.sort_by_key(|row| {
                (
                    row.location.0,
                    row.location.1,
                    row.location.2,
                    row.location.3,
                    row.created_at,
                    row.kind,
                    row.storage_index,
                )
            });
        }
        if self.controls.bookmark_reverse {
            rows.reverse();
        }

        let has_marks = !rows.is_empty();
        let mut items = Vec::new();
        if has_marks {
            items.insert(
                0,
                Item::new("Find bookmarks", Action::Show(Screen::BookmarkFind)),
            );
        }
        if active.is_some_and(|book| book.entry.format == DocumentFormat::Epub) {
            items.push(
                Item::new(
                    "Import marks saved beside this book",
                    Action::PreviewNearbyMarks,
                )
                .detail("Preview KOReader bookmarks, highlights and notes"),
            );
        }
        if active.is_some_and(|book| {
            matches!(
                book.entry.format,
                DocumentFormat::Epub | DocumentFormat::Pdf
            )
        }) || fixed.is_some_and(|book| book.entry.format == DocumentFormat::Pdf)
        {
            items.push(
                Item::new("Import KOReader annotations", Action::PreviewKoReader)
                    .detail("Copy a JSON export to reader/data/koreader-import.json"),
            );
        }
        if has_text_annotations {
            items.push(Item::new(
                "Export annotations",
                Action::Show(Screen::ExportAnnotations),
            ));
        }
        if fixed.is_some_and(|book| {
            book.entry.format == DocumentFormat::Pdf && !book.reading.is_encrypted_pdf()
        }) {
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
        items.extend(rows.into_iter().map(|row| row.item));
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
                Item::new(&chapter.label, Action::BookMapChapter(index)).detail({
                    let detail = map_detail(index == reading.chapter, marks, highlights);
                    let detail = if self.ribbon_map_target()
                        == Some(page_overview::RibbonMapTarget::Chapter(index))
                    {
                        format!("Map focus · {detail}")
                    } else {
                        detail
                    };
                    reading.hidden_chapter(index).map_or(detail.clone(), |all| {
                        format!(
                            "{} · {detail}",
                            if all {
                                "Hidden"
                            } else {
                                "Includes hidden pages"
                            }
                        )
                    })
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
                items.push(Self::book_shortcuts_item());
                items.push(Item::new("Authors", Action::ListBooks(BookList::Authors)));
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
                if self.editable_book_identity().is_some() {
                    items.extend(self.book_identity_items());
                } else {
                    let description = book.entry.author.as_deref().map_or_else(
                        || book.entry.path.to_string_lossy().into_owned(),
                        str::to_owned,
                    );
                    items.push(Item::new(&book.entry.title, Action::Heading).detail(description));
                }
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
                let seconds = self
                    .reading_history
                    .elapsed_since_reset(book.id, self.stats.seconds_read(book.id, Instant::now()));
                items.push(
                    Item::new(
                        "Reading time",
                        Action::ReadingHistory(reading_history::Command::Open(Some(book.id))),
                    )
                    .detail(reading_time(seconds)),
                );
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
            Action::ToggleNotificationSource(source) => self.toggle_notification_source(source)?,
            Action::EditListDensity(scope, field) => self.open_list_density(scope, field),
            Action::EditSearchPresentation(field) => self.open_search_presentation(field),
            Action::EditFooterItemWidth(item) => self.open_footer_width(item, None),
            Action::BookmarkBrowser(command) => self.activate_bookmark_browser(command)?,
            Action::ReadingHistory(command) => self.activate_reading_history(command)?,
            Action::Flow(action) => self.flow_action(action)?,
            Action::ToggleNonlinear => {
                let active = self.active.as_mut().ok_or("Open a book first")?;
                if active.reading.config.scroll || active.reading.config.facing_pages {
                    return Err("Use single-page mode to hide non-linear fragments".into());
                }
                active
                    .reading
                    .set_hide_nonlinear(!active.reading.config.hide_nonlinear)?;
                let id = active.id;
                let enabled = active.reading.config.hide_nonlinear;
                self.library
                    .state_mut(id)
                    .ok_or("This book is unavailable")?
                    .hide_nonlinear = Some(enabled);
                self.mark_book_changed();
            }
            Action::LibraryExport(command) => self.activate_library_export(command)?,
            Action::AnnotationExchange(command) => self.annotation_exchange_command(command)?,
            Action::ExportSettings(origin, command) => {
                self.export_settings_command(origin, command)?
            }
            Action::Open(entry) => self.open_book(entry)?,
            Action::Browse(directory) => self.browse_to(directory)?,
            Action::BookArchive(command) => self.archive_command(command)?,
            Action::MetadataArchive(command) => self.metadata_archive_command(command)?,
            Action::BookShortcut(command) => self.book_shortcut_command(command)?,
            Action::LiveMetadata(command) => self.live_metadata_command(command)?,
            Action::CurrentBookFolder => self.current_book_folder()?,
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
            Action::SetFolderOrder(order) => self.set_folder_order(order)?,
            Action::SetReadingOrientation(orientation) => {
                self.set_reading_orientation(orientation)?
            }
            Action::KeepReadingOrientation(keep) => self.keep_reading_orientation(keep)?,
            Action::ConfirmReadingOrientationDefault(orientation) => {
                self.confirm_reading_orientation_default(orientation);
            }
            Action::PageTurnSetting(command) => self.change_page_turn_setting(command)?,
            Action::SetReadingControls(mut controls) => {
                if controls.status_bar == crate::reading_controls::StatusBar::Custom
                    && self.controls.status_bar != crate::reading_controls::StatusBar::Custom
                {
                    controls.footer_all_at_once = false;
                    if controls.footer_items == 0 {
                        controls.footer_items = crate::reading_controls::FooterItem::DEFAULT_MASK;
                    }
                }
                if let Some(cycle) = &mut controls.footer_item_cycle
                    && cycle.selected.is_some_and(|item| {
                        !item.can_cycle() || controls.footer_items & item.bit() == 0
                    })
                {
                    cycle.selected =
                        crate::reading_controls::FooterItem::ordered(controls.footer_order)
                            .into_iter()
                            .find(|item| {
                                item.can_cycle() && controls.footer_items & item.bit() != 0
                            });
                }
                let footer_overlap_changed =
                    controls.overlap_footer != self.controls.overlap_footer;
                let limits_changed =
                    controls.hyphenation_limits != self.controls.hyphenation_limits;
                let resized = if footer_overlap_changed || limits_changed {
                    self.active
                        .as_ref()
                        .map(|book| {
                            let reading = &book.reading;
                            let mut config = reading.config.clone();
                            config.settings.height =
                                config.viewport.document_height(controls.overlap_footer);
                            config.settings.hyphenation_limits = controls.hyphenation_limits;
                            config.prepare_page_layout(&reading.config);
                            let anchor = reading.anchor();
                            let candidate = Reading::restore_with_scroll_top(
                                config,
                                reading.chapter,
                                reading.page,
                                anchor.as_ref(),
                                if limits_changed {
                                    reading.scroll_history_top()
                                } else {
                                    None
                                },
                            )?;
                            if limits_changed
                                && (candidate.probe.book != reading.probe.book
                                    || anchor.is_some() && candidate.anchor() != anchor)
                            {
                                return Err("Could not keep this book's reading position".into());
                            }
                            Result::Ok(candidate)
                        })
                        .transpose()?
                } else {
                    None
                };
                if footer_overlap_changed && let Some(fixed) = &mut self.fixed {
                    let height = fixed
                        .reading
                        .viewport()
                        .document_height(controls.overlap_footer);
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
                        self.metadata_checkpoint.record(
                            controls.metadata_save,
                            false,
                            Instant::now(),
                        );
                    }
                }
                self.update_search_presentation(&controls);
                if controls.swipe_to_go_back != self.controls.swipe_to_go_back {
                    self.swipe_back_notice_pending = false;
                }
                self.reading_guide_progress
                    .settings_changed(self.controls.reading_guides, controls.reading_guides);
                let previous_back_settings = self.controls.back_navigation;
                self.controls = controls;
                self.back_preferences_changed(previous_back_settings);
                self.update_footer_clock(Instant::now());
            }
            Action::BindGesture(gesture, binding) => {
                self.gestures.set(gesture, binding)?;
                self.return_to_gesture_list(gesture);
            }
            Action::Multiswipe(command) => self.activate_multiswipe(command)?,
            Action::SetDisplayLevel(control, level) => self.quick_display.set(control, level)?,
            Action::RefreshDisplay => {
                self.pages_since_refresh = 0;
                self.refresh_display = true;
            }
            Action::HighlightStyle => self.open_highlight_styles(None),
            Action::ReadAlarm => {
                self.open_read_alarm();
            }
            Action::SaveBookData => {
                self.notice = None;
                self.save_library()?;
                if self.notice.is_none() {
                    self.notice = Some("Reading data saved".into());
                }
            }
            Action::AutoTurn(interval) => {
                if let Some(interval) = interval {
                    let seconds = interval
                        .as_secs()
                        .min(u64::from(crate::reading_controls::MAX_AUTO_TURN_SECONDS))
                        as u32;
                    self.set_auto_turn_seconds(seconds)?;
                } else {
                    self.disable_auto_turn()?;
                }
            }
            Action::AdjustAutoTurnDistance(delta) => {
                self.adjust_auto_turn_distance(delta)?;
            }
            Action::ResetAutoTurnDistance => self.reset_auto_turn_distance()?,
            Action::AutoTurnPan(change) => self.change_auto_turn_pan(change)?,
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
                self.request_close(profile_panel::on_close::AfterClose::Library)?;
            }
            Action::Show(screen) => self.show(screen),
            Action::SearchReadingMenus => self.begin_menu_search()?,
            Action::OpenQuickstart => {
                self.open_quickstart();
                self.refresh_display = true;
            }
            Action::PreviewReadingMenu(index) => self.menu_search.preview(index)?,

            Action::Jump(target) => self.follow(target)?,
            Action::BookMapChapter(chapter) => {
                self.select_book_map_page(page_overview::MapOrigin::Chapter(chapter), 0)?;
            }
            Action::OpenFixedLink(link) => self.open_fixed_link(link)?,
            Action::OpenLinkAddress(address) => {
                self.show(self.surface_screen());
                self.show_link_address(address);
            }
            Action::ToggleLinkPreference(setting) => self.toggle_link_preference(setting)?,
            Action::SetMenuActivation(option) => self.change_menu_activation(option)?,
            Action::Chapter(direction) => self.jump_chapter(direction)?,
            Action::ChapterLevel(setting) => self.set_chapter_level(setting)?,
            Action::Custom(action) => self.custom_action(action)?,
            Action::SetCharacterPages(stride) => self.set_character_pages(stride)?,
            Action::AcceptPublisherNumbers => self.accept_publisher_numbers()?,
            Action::CharacterPage(row) => self.follow_character_page(row)?,
            Action::TogglePublisherFooter => self.toggle_publisher_labels(false)?,
            Action::TogglePublisherMargins => self.toggle_publisher_labels(true)?,
            Action::ToggleAlternativeContents => {
                if self.active.is_some() && !self.custom_contents_enabled() {
                    self.alternative_contents.confirm = Some(!self.uses_alternative_contents());
                    self.notice = None;
                }
            }
            Action::AlternativeContentsEntry(target) => self.follow_outline_target(target)?,
            Action::ShowGeneratedContents => self.start_generated_contents()?,
            Action::GeneratedEntry(index) => self.follow_generated_entry(index)?,
            Action::ReturnToLocation => {
                self.return_to_location(self.back.len().saturating_sub(1))?;
            }
            Action::ReturnToHistory(index) => self.return_to_location(index)?,
            Action::NextLocation => self.next_location()?,
            Action::OpenPreviousDocument => self.open_previous_document()?,
            Action::OpenFolderFile(direction) => self.open_folder_file(direction)?,
            Action::Wikipedia => self.prepare_online(true, false)?,
            Action::TranslatePage => self.prepare_online(false, true)?,
            Action::RepeatOnline(index) => self.repeat_online(index)?,
            Action::ToggleOnlineHistory => self.update_lookup_history(|h| {
                h.online_languages.forget_queries = !h.online_languages.forget_queries
            })?,
            Action::ToggleRomanizations => self.update_lookup_history(|h| {
                h.online_languages.show_romanizations = !h.online_languages.show_romanizations
            })?,
            Action::ToggleWikipediaImage => self.update_lookup_history(|h| {
                h.online_languages.show_wiki_image = !h.online_languages.show_wiki_image
            })?,
            Action::ToggleWikipediaMoreImages
                if self.lookup_history.online_languages.show_wiki_image =>
            {
                self.update_lookup_history(|h| {
                    h.online_languages.show_wiki_more = !h.online_languages.show_wiki_more
                })?;
            }
            Action::ToggleWikipediaMoreImages => {}
            Action::ClearOnlineHistory => {
                self.update_lookup_history(|h| h.online.clear())?;
                self.show(Screen::OnlineHistory);
            }
            Action::ClearDictionaryHistory => {
                self.update_lookup_history(|h| h.dictionary.clear())?;
                self.show(Screen::Dictionaries);
            }
            Action::LookUp(word) => self.look_up(&word)?,
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
            Action::SetFixedTileZoom(axis, count) => {
                let fixed = self
                    .fixed
                    .as_mut()
                    .ok_or("Open a fixed-page document first")?;
                fixed.reading.set_tile_zoom(axis, count)?;
                self.show(Screen::Fixed);
                self.record_position();
            }
            Action::SetFixedPageOverlap(enabled, style) => {
                let fixed = self
                    .fixed
                    .as_mut()
                    .ok_or("Open a fixed-page document first")?;
                fixed.reading.set_page_overlap(enabled, style)?;
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
            Action::ScanGamma(tenths) => {
                let fixed = self
                    .fixed
                    .as_mut()
                    .ok_or("Open a fixed-page document first")?;
                fixed.reading.set_scan_gamma(tenths)?;
                self.show(Screen::Fixed);
                self.record_position();
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
            Action::Overview => self.open_overview()?,
            Action::Skim => self.start_skim()?,
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
                if fixed.reading.is_continuous() {
                    self.start_native_pdf_crop(true)?;
                } else {
                    self.crop_dialog = Some((fixed.reading.start_crop()?, None));
                    self.show(Screen::FixedCrop);
                }
            }
            Action::SemiAutoCrop => self.start_native_pdf_crop(false)?,
            Action::SetCropMargin(hundredths) => {
                let fixed = self.fixed.as_mut().ok_or("Open a PDF first")?;
                fixed.reading.set_crop_margin(hundredths)?;
                self.show(Screen::Fixed);
                self.record_position();
            }
            Action::RecognizeText => self.start_ocr(None)?,
            Action::RecognizeLanguage(language) => self.start_ocr(Some(&language))?,
            Action::ReadFixedText => self.read_fixed_text(false)?,
            Action::ReflowScan => self.read_fixed_text(true)?,
            Action::FixedPage(page) => {
                self.follow_fixed(page, None)?;
            }
            Action::Heading | Action::MultiswipePreview(_) => {}
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
            Action::ApplyWordSpacing(owner, value) => self.apply_word_spacing(owner, value)?,
            Action::ApplyLogicalFont(owner, value) => self.apply_logical_font(owner, value)?,
            Action::EmbeddedFontReport => self.open_embedded_font_report(),
            Action::FontRendering(scope, mode) => self.set_font_rendering(scope, mode)?,
            Action::TypographyLanguage(scope, choice) => {
                self.choose_typography_language(scope, choice)?
            }
            Action::SetHangingPunctuation {
                enabled,
                for_new_books,
            } => {
                self.set_hanging_punctuation(enabled, for_new_books)?;
            }
            Action::RespectEmbeddedLanguages(respect) => {
                self.set_embedded_language_respect(respect)?
            }
            Action::Hyphenation(command) => self.activate_hyphenation(command)?,
            Action::SetBookPublisherRenderMode(mode) => {
                self.set_book_publisher_render_mode(mode)?
            }
            Action::SetPublisherRenderDefault(mode) => self.set_publisher_render_default(mode)?,
            Action::CancelEagerFont => {
                self.eager_typography.cancel();
                self.notice = None;
            }
            Action::Text(settings) => {
                let active = self.active.as_mut().ok_or("No open book")?;
                active.reading.reflow(settings)?;
                let preferences = &mut self
                    .library
                    .remember(&active.entry, active.id, &active.reading.probe.title)
                    .preferences;
                if settings.logical_font_size.is_none() {
                    preferences.font_size = Some(settings.font_size);
                }
                preferences.logical_font_size = settings.logical_font_size;
                preferences.render_dpi = Some(settings.render_dpi);
                preferences.margin = Some(settings.margin);
                preferences.margin_sides = settings.margin_sides;
                preferences.margin_left = settings.margin_left;
                preferences.margin_right = settings.margin_right;
                preferences.link_vertical_margins = settings.link_vertical_margins;
                preferences.margin_top = settings.margin_top;
                preferences.margin_bottom = settings.margin_bottom;
                preferences.line_height = Some(settings.line_height);
                preferences.line_spacing_percent = settings.line_spacing_percent;
                preferences.word_spacing = settings.word_spacing;
                preferences.percent_word_spacing = settings.percent_word_spacing;
                preferences.word_expansion = Some(settings.word_expansion);
                preferences.letter_spacing = settings.letter_spacing;
                preferences.cjk_width_percent =
                    (settings.cjk_width_percent != 100).then_some(settings.cjk_width_percent);
                preferences.scroll = active.reading.config.scroll;
                preferences.alignment = settings.alignment;
                preferences.bold_text = settings.bold_text;
                preferences.text_contrast = settings.text_contrast;
                preferences.font_gamma_index = Some(settings.font_gamma_index);
                preferences.font_weight_delta = Some(settings.font_weight_delta);
                preferences.publisher_styles = Some(settings.publisher_styles);
                preferences.embedded_fonts = Some(settings.embedded_fonts);
                preferences.base_stylesheet = Some(settings.base_stylesheet);
                preferences.font_kerning = Some(settings.font_kerning);
                preferences.font_kerning_mode = Some(settings.effective_font_kerning_mode());
                preferences.font_hinting = settings.font_hinting;
                preferences.smooth_images = Some(settings.smooth_images);
                preferences.invert_night_images = settings.invert_night_images;
                preferences.override_fonts = settings.override_fonts;
                preferences.record_hyphenation(settings);
                preferences.record_typography(&active.reading.probe);
                preferences.page_overlap_enabled = Some(settings.page_overlap_enabled);
                preferences.page_overlap_lines = Some(settings.page_overlap_lines.clamp(1, 10));
                preferences.page_overlap_style = Some(settings.page_overlap_style);
                self.show(Screen::Book);
                self.record_position();
            }
            Action::ShowFonts => self.open_font_chooser()?,
            Action::ShowFooterFonts => {
                with_storage(|| fs::create_dir_all(&self.fonts_directory))?;
                self.available_fonts = reading_fonts::available(&self.fonts_directory)?;
                self.show(Screen::FooterFontFaces);
            }
            Action::SetFooterFontFace(font) => {
                self.set_footer_font_file(font)?;
                self.show(Screen::FooterSettings);
            }
            Action::ShowDictionaries => self.open_sources()?,
            Action::DictionaryPreset(command) => self.dictionary_preset_command(command)?,
            Action::DictionarySource(action) => self.change_source(action)?,
            Action::DictionaryButtons(action) => self.change_dictionary_buttons(action)?,
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
            Action::ApplyProfile(index) => self.apply_reading_profile(index)?,
            Action::RemoveProfile(index) => self.remove_reading_profile(index)?,
            Action::UpdateReadingProfile(index) => self.update_reading_profile(index)?,
            Action::EditReadingProfileFields(index) => self.edit_profile_fields(index)?,
            Action::ToggleReadingProfileField(field) => self.toggle_profile_field(field)?,
            Action::SetReadingProfileFields(all) => self.set_profile_fields(all)?,
            Action::SetClosingProfile(index, policy) => self.set_closing_profile(index, policy)?,
            Action::OpeningProfileRule(index, command) => {
                self.change_opening_rule(index, command)?
            }
            Action::ClosingProfileRule(index, command) => self.change_profile_rule(
                profile_panel::opening_rules::Event::Closing,
                index,
                command,
            )?,
            Action::SetProfileRendering(index, value) => {
                self.save_profile_rendering_choice(index, value)?
            }
            Action::ToggleProfileMarginLink(index) => self.toggle_profile_margin_link(index)?,
            Action::UpdateFooterPreset(index) => {
                let mut presets = self.footer_presets.clone();
                footer_presets::update(
                    &mut presets,
                    index,
                    self.controls,
                    self.footer_font_file.clone(),
                    self.footer_custom_text.clone(),
                )?;
                footer_presets::save(&self.footer_presets_path, &presets)?;
                self.footer_presets = presets;
                self.show(Screen::FooterPreset(index));
                self.notice = Some("Status bar preset updated".into());
            }
            Action::ApplyFooterPreset(index) => {
                let preset = self
                    .footer_presets
                    .get(index)
                    .ok_or("That status bar preset is unavailable")?;
                let (controls, mut font_file, custom_text) = preset.apply(self.controls);
                if font_file
                    .as_deref()
                    .is_some_and(|file| self.fonts_for(Some(file)).is_err())
                {
                    font_file = None;
                }
                self.set_footer_font_file(font_file)?;
                footer_custom_text::save(&self.footer_custom_text_path, &custom_text)?;
                self.footer_custom_text = custom_text;
                self.activate(Action::SetReadingControls(controls))?;
                self.show(Screen::FooterSettings);
                self.notice = Some("Status bar preset applied".into());
            }
            Action::RenameFooterPreset(index) => {
                if index >= self.footer_presets.len() {
                    return Err("That status bar preset is unavailable".into());
                }
                self.show(Screen::FooterPresetName(FooterPresetName::Rename(index)));
                let name = &self.footer_presets[index].name;
                self.editor.text = name.chars().collect();
                self.editor.caret = self.editor.text.len();
            }
            Action::DeleteFooterPreset(index) => {
                if index >= self.footer_presets.len() {
                    return Err("That status bar preset is unavailable".into());
                }
                let mut presets = self.footer_presets.clone();
                presets.remove(index);
                footer_presets::save(&self.footer_presets_path, &presets)?;
                self.footer_presets = presets;
                self.show(Screen::FooterPresets);
                self.notice = Some("Status bar preset deleted".into());
            }
            Action::ResetText => {
                let mut preferences = BookPreferences::for_new_reflow();
                preferences.record_hyphenation(self.device_reading_settings());
                self.apply_text_profile(preferences)?;
            }
            Action::DocumentDefaults(command) => self.activate_document_defaults(command)?,
            Action::Style(command) => self.activate_style(command)?,
            Action::TopStatusHeader(enabled) => self.set_book_top_header(enabled)?,
            Action::AboutTopHeader => self.top_header_about = true,
            Action::EditHeaderFontSize => self.open_header_font_picker(),
            Action::ConfirmTopStatusHeaderDefault(enabled) => {
                self.top_header_default_prompt = Some(enabled);
            }
            Action::ReadingMode(scroll) => {
                let id = self.active.as_ref().ok_or("No open book")?.id;
                let status_header_height = crate::reading_controls::TopHeader::height(
                    self.top_header_for_book(id),
                    scroll,
                );
                let active = self.active.as_mut().unwrap();
                active
                    .reading
                    .set_scroll_and_header(scroll, status_header_height)?;
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
                self.ensure_reflow_search_current()?;
                if self.search_result_order().ordinal(index).is_none() {
                    return Err("That result is unavailable".into());
                }
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
                let page = self.match_page_for_index(index);
                self.remember_match_window(page);
            }
            Action::PdfSearchHit(index) => self.open_fixed_list_result(index)?,
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
            Action::ExportBookClippings => self.export_book_clippings()?,
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

    fn apply_text_profile(&mut self, mut preferences: BookPreferences) -> Result<()> {
        crate::document_defaults::validate_preferences(&preferences)?;
        let active = self.active.as_ref().ok_or("Open a book first")?;
        let id = active.id;
        let current = &self
            .library
            .state(id)
            .ok_or("Reading state unavailable")?
            .preferences;
        if preferences.automatic_hyphenation.is_none() && !preferences.disable_soft_hyphens {
            preferences.disable_soft_hyphens = current.disable_soft_hyphens;
            preferences.automatic_hyphenation = current.automatic_hyphenation;
            preferences.algorithmic_hyphenation = current.algorithmic_hyphenation;
        }
        if preferences.trust_soft_hyphens.is_none() {
            preferences.trust_soft_hyphens = current.trust_soft_hyphens;
        }
        if preferences.typography_language.is_none() {
            preferences
                .typography_language
                .clone_from(&current.typography_language);
        }
        if preferences.respect_embedded_languages.is_none() {
            preferences.respect_embedded_languages = current.respect_embedded_languages;
        }
        if preferences.publisher_render_mode.is_none() {
            preferences.publisher_render_mode = current.publisher_render_mode;
        }
        if preferences.hanging_punctuation.is_none() {
            preferences.hanging_punctuation = current.hanging_punctuation;
        }
        let fonts = if current.font_file == preferences.font_file {
            active.reading.config.fonts.clone()
        } else {
            self.fonts_for(preferences.font_file.as_deref())?
        };
        if with_storage(|| self.library.identify(&active.entry))? != id {
            return Err("The document edition changed; reopen it before applying a profile".into());
        }
        let location = position(&active.reading);
        let mut config = active.reading.config.clone();
        config.settings = preferences.apply_to(self.device_reading_settings_for(config.viewport));
        let top_status_header = self.top_header_for_book(id);
        config.settings.status_header_height =
            crate::reading_controls::TopHeader::height(top_status_header, preferences.scroll);
        config.scroll = preferences.scroll;
        config.facing_pages = preferences.facing_pages;
        config.fonts = fonts;
        config.prepare_page_layout(&active.reading.config);
        let mut candidate = Reading::restore_with_scroll_top(
            config,
            location.chapter,
            location.page,
            location.anchor.as_ref(),
            active.reading.scroll_history_top(),
        )?;
        candidate.set_status_header_height(crate::reading_controls::TopHeader::height(
            top_status_header,
            candidate.is_continuous(),
        ))?;
        if candidate.probe.book != active.reading.probe.book
            || with_storage(|| self.library.identify(&active.entry))? != id
        {
            return Err("The document edition changed during profile application".into());
        }
        if location.anchor.is_some() && candidate.anchor() != location.anchor {
            return Err("This profile could not keep the saved passage".into());
        }
        let prior = self
            .library
            .state(id)
            .cloned()
            .ok_or("Reading state unavailable")?;
        let state = self.library.state_mut(id).unwrap();
        state.preferences = preferences;
        state.position = Some(position(&candidate));
        if let Err(error) = self.library.persist() {
            *self.library.state_mut(id).unwrap() = prior;
            return Err(error.into());
        }
        self.eager_typography.cancel();
        self.active.as_mut().unwrap().reading = candidate;
        self.show(Screen::Book);
        self.record_position();
        Ok(())
    }

    fn save_footer_preset_name(&mut self, mode: FooterPresetName, name: &str) -> Result<()> {
        let mut presets = self.footer_presets.clone();
        match mode {
            FooterPresetName::Create => footer_presets::insert(
                &mut presets,
                name,
                self.controls,
                self.footer_font_file.clone(),
                self.footer_custom_text.clone(),
            )?,
            FooterPresetName::Rename(index) => footer_presets::rename(&mut presets, index, name)?,
        }
        footer_presets::save(&self.footer_presets_path, &presets)?;
        self.footer_presets = presets;
        self.show(match mode {
            FooterPresetName::Create => Screen::FooterPresets,
            FooterPresetName::Rename(index) => Screen::FooterPreset(index),
        });
        self.notice = Some(match mode {
            FooterPresetName::Create => "Status bar preset saved".into(),
            FooterPresetName::Rename(_) => "Status bar preset renamed".into(),
        });
        Ok(())
    }

    fn save_footer_custom_text(&mut self) -> Result<()> {
        let mut preference = self.footer_custom_text.clone();
        preference.set_text(&self.editor.string())?;
        footer_custom_text::save(&self.footer_custom_text_path, &preference)?;
        self.footer_custom_text = preference;
        self.show(Screen::FooterSettings);
        self.notice = Some("Custom footer text saved".into());
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
        self.reload_active_header_metadata(id);
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

    fn name_editor_input(&mut self, input: Input) -> Change {
        if let Some(change) = self.opening_path_shortcut_input(input) {
            return change;
        }
        let profile = match self.screen {
            Screen::ProfileName(mode) => Some(mode),
            _ => None,
        };
        let dictionary_preset = match self.screen {
            Screen::DictionaryPresets(dictionary_presets::Page::Name(mode)) => Some(mode),
            _ => None,
        };
        let footer_preset = match self.screen {
            Screen::FooterPresetName(mode) => Some(mode),
            _ => None,
        };
        let book_identity = match self.screen {
            Screen::BookIdentityEdit(field) => Some(field),
            _ => None,
        };
        let navigation = if self.screen.uses_navigation_viewport() {
            self.navigation_layout()
        } else {
            navigation_layout::NavigationLayout::PORTRAIT
        };
        if matches!(input, Input::PageBack)
            || matches!(input, Input::Tap(x, y) if navigation.back().contains(x, y))
        {
            let back = if let Some(mode) = profile {
                mode.back()
            } else if let Some(mode) = footer_preset {
                match mode {
                    FooterPresetName::Create => Screen::FooterPresets,
                    FooterPresetName::Rename(index) => Screen::FooterPreset(index),
                }
            } else if let Some(mode) = dictionary_preset {
                Screen::DictionaryPresets(mode.back())
            } else if book_identity.is_some() {
                Screen::BookDetails
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
        if let Some(field) = book_identity
            && let Some(reset) = self.book_identity_reset(field)
            && reset.contains(x, y)
        {
            return match self.save_book_identity(field, "") {
                Ok(()) => Change::Partial,
                Err(error) => self.report(error),
            };
        }
        if let Some(change) = self.choose_current_opening_book(x, y) {
            return change;
        }
        let layout = navigation.keyboard();
        let setting_time = matches!(
            profile,
            Some(profile_panel::Name::OpeningTime(..) | profile_panel::Name::ClosingTime(..))
        );
        let action = if setting_time { "Set" } else { "Save" };
        let Some(key) = keyboard_key(&layout, &self.editor, action, x, y) else {
            return Change::None;
        };
        if matches!(self.editor.press(key), Press::Go) {
            let name = self.editor.string();
            let result = if let Some(mode) = profile {
                self.edit_reading_profile_name(mode, name.trim())
            } else if let Some(mode) = footer_preset {
                self.save_footer_preset_name(mode, name.trim())
            } else if let Some(mode) = dictionary_preset {
                self.save_dictionary_preset_name(mode, name.trim())
            } else if let Some(field) = book_identity {
                self.save_book_identity(field, &name)
            } else {
                self.save_collection_name(name.trim())
            };
            if let Err(error) = result {
                return self.report(error);
            }
        } else if setting_time {
            self.notice = None;
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
        if !already_bookmarked {
            let probe = self.active.as_ref().map(|active| &active.reading.probe);
            let chapter_title = self.bookmark_chapter_title(&position, probe);
            let state = self.library.remember(entry, id, title);
            overview_bookmarks::push_page_bookmark(state, position, label, chapter_title)?;
            self.mark_book_changed();
        }
        if self.screen != Screen::Skim {
            self.show(screen);
        }
        self.notice = Some("Page bookmarked".into());
        Ok(())
    }

    fn open_book_id(&self) -> Option<u64> {
        self.fixed
            .as_ref()
            .map(|book| book.id)
            .or_else(|| self.active.as_ref().map(|book| book.id))
    }

    fn reading_viewport(&self) -> crate::session::ReadingViewport {
        if let Some(book) = &self.active {
            book.reading.config.viewport
        } else if let Some(book) = &self.fixed {
            book.reading.viewport()
        } else {
            self.library_viewport
        }
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
            return overview_bookmarks::bookmark_index_for_page(
                book,
                None,
                fixed.reading.page(),
                None,
            );
        }
        let reading = &self.active.as_ref()?.reading;
        overview_bookmarks::bookmark_index_for_page(
            book,
            Some(reading.chapter),
            reading.page,
            Some(&reading.probe),
        )
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
        if self.screen != Screen::Skim {
            self.show(if self.fixed.is_some() {
                Screen::Fixed
            } else {
                Screen::Book
            });
        }
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
                return self.open_illustration_view(FixedReading::inspect_image(image)?);
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
        self.follow_outline_target(target)
    }

    fn follow_outline_target(&mut self, target: OutlineTarget) -> Result<()> {
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
        self.close_fixed_search_fit()?;
        let fixed = self.fixed.as_mut().ok_or("No open document")?;
        let previous = HistoryLocation {
            position: fixed_position(&fixed.reading),
            scan_point: fixed.reading.history_point(),
        };
        fixed.reading.jump_to(page, target.as_ref())?;
        self.finish_fixed_jump(page, target);
        Ok(previous)
    }

    fn finish_fixed_jump(&mut self, page: usize, target: Option<FixedJump>) {
        self.search_result = None;
        self.pdf_selection = None;
        self.pdf_result_regions.clear();
        if let Some(FixedJump::Regions(bounds)) = target {
            self.pdf_result_regions
                .extend(bounds.into_iter().map(|rect| (page, rect)));
        }
        self.show(Screen::Fixed);
        self.record_position();
    }

    fn restore_position(&mut self, new_position: &ReadingPosition) -> Result<()> {
        self.restore_reading_position(new_position, None)
    }

    fn restore_reading_position(
        &mut self,
        new_position: &ReadingPosition,
        scroll_history_top: Option<f32>,
    ) -> Result<()> {
        let reading = &mut self.active.as_mut().ok_or("No open book")?.reading;
        *reading = Reading::restore_with_scroll_top(
            reading.config.clone(),
            new_position.chapter,
            new_position.page,
            new_position.anchor.as_ref(),
            scroll_history_top,
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
        self.swipe_back_notice_pending = false;
        self.reset_back_resistance();
        self.forward.clear();
        Self::remember_location(&mut self.back, previous.into());
    }

    fn clear_location_history(&mut self) {
        self.back.clear();
        self.forward.clear();
        self.swipe_back_notice_pending = false;
        self.back_navigation = back_navigation::State::default();
    }

    fn close_book_to_library(&mut self) {
        self.eager_typography.cancel();
        self.closing_profile = None;
        self.closing_before_switch = None;
        self.opening_execution = None;
        self.settle_pending_opening();
        self.pending_orientation = None;
        self.active = None;
        self.numbering_scan = None;
        self.fixed = None;
        self.touch_locked = false;
        self.ocr = None;
        self.selection = None;
        self.clear_location_history();
        self.auto_turn.configure(None, Instant::now());
        self.show(Screen::Library);
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
        self.close_fixed_search_fit()?;
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
        self.close_fixed_search_fit()?;
        let current = self
            .current_history_location()
            .ok_or("Open a document first")?;
        self.go_to_location(&next)?;
        self.forward.pop();
        self.swipe_back_notice_pending = false;
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
            && self.quickstart.is_none()
            && !self.scroll_pan.busy()
            && self.closing_profile.is_none()
            && !self.opening_ask_visible()
            && self.pending_opening_profiles.is_none()
            && !self.fixed_search_fit_active()
            && !self.touch_locked
            && self.end_dialog.is_none()
            && !self.back_exit_confirmation_open()
            && self.link_popup.is_none()
            && !self.read_alarm.modal()
            && self.range_edit.is_none()
            && self.saved_popup.is_none()
            && self.selection.is_none()
            && self.pdf_selection.is_none()
            && matches!(self.screen, Screen::Book | Screen::Fixed)
    }

    fn needs_screen_page_count(&self) -> bool {
        if self.screen == Screen::GoToPage {
            return self.stats_awake && self.screen_page_jump.is_some();
        }
        let header = self.controls.top_header;
        if self.stats_awake
            && (self.screen == Screen::Skim
                || self.screen == Screen::Book
                    && ((self.top_header_visible() && header.chapter_marks)
                        || (self.controls.footer_scope
                            == crate::reading_controls::ProgressScope::Book
                            && !self.controls.hide_chapter_markers)))
            && self
                .open_book_id()
                .and_then(|id| self.alternative_entries(id))
                .is_some_and(|entries| !entries.is_empty())
        {
            return true;
        }
        self.stats_awake
            && self.screen == Screen::Book
            && self.top_header_visible()
            && ((header.chapter_marks
                && self.active.as_ref().is_some_and(|book| {
                    !book.reading.probe.toc.is_empty()
                        && self
                            .library
                            .state(book.id)
                            .is_some_and(|state| !state.custom_contents.enabled)
                }))
                || header.reading_percent
                || ((header.page_number || header.page_count)
                    && self.top_header_page_labels().is_none()))
    }

    fn screen_page_count_intent(&self) -> Option<ScreenPageIntent> {
        if self.eager_typography.pending()
            && (self.screen != Screen::GoToPage || self.eager_typography.waiting_for_layout())
        {
            return None;
        }
        if self.needs_screen_page_count() {
            return Some(ScreenPageIntent::Visible);
        }
        (self.stats_awake
            && self.screen == Screen::Book
            && self.touch_selection_enabled()
            && self.touch_hold.is_none()
            && self.selection.is_none()
            && self.closing_profile.is_none()
            && self.pending_opening_profiles.is_none()
            && !self.opening_ask_visible()
            && self
                .active
                .as_ref()
                .is_some_and(|book| book.reading.config.screen_pages.typography_pending()))
        .then_some(ScreenPageIntent::Typography)
    }

    fn sync_auto_turn(&mut self, now: Instant) {
        if self.auto_turn_eligible() {
            self.auto_turn.resume(now);
        } else {
            self.auto_turn.pause();
        }
    }

    fn step_auto_turn(&mut self, now: Instant) -> Change {
        let distance = self.controls.auto_turn_distance_tenths();
        let change = match self.screen {
            Screen::Book => {
                let Some((previous, change)) = self.active.as_mut().map(|active| {
                    let previous = (active.reading.chapter, active.reading.page);
                    let change = active.reading.automatic_turn(distance);
                    (previous, change)
                }) else {
                    return Change::None;
                };
                self.finish_book_update(change, previous)
            }
            Screen::Fixed => {
                let Some((previous, result)) = self.fixed.as_mut().map(|fixed| {
                    let previous = fixed.reading.page();
                    let result = fixed.reading.automatic_turn(distance);
                    (previous, result)
                }) else {
                    return Change::None;
                };
                self.finish_fixed_update(result, previous)
            }
            _ => self.input(Input::PageForward),
        };
        self.auto_turn.activity(now);
        self.sync_auto_turn(now);
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

    fn reading_edge_swipe_direction(&self, input: Input, start_x: f32) -> Option<bool> {
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
        let viewport = self.reading_viewport();
        let (width, screen_height) = viewport.dimensions();
        let width = width as f32;
        if !(0.0..width).contains(&start_x) {
            return None;
        }
        let screen_height = screen_height as f32;
        let edge = reading_bars::menu_edge_height(width, screen_height, start_x);
        let from_top = dy > 0.0 && (0.0..=edge).contains(&start_y);
        let from_bottom = dy < 0.0 && (screen_height - edge..=screen_height).contains(&start_y);
        (from_top || from_bottom).then_some(from_top)
    }

    fn reading_edge_swipe(&mut self, input: Input, start_x: f32) -> Option<Change> {
        let from_top = self.reading_edge_swipe_direction(input, start_x)?;
        self.selection = None;
        self.pdf_selection = None;
        Some(self.open_reading_edge(from_top, start_x))
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
        let search = self.search_layout();
        let navigation = if self.screen.uses_navigation_viewport() {
            self.navigation_layout()
        } else {
            navigation_layout::NavigationLayout::PORTRAIT
        };
        if matches!(input, Input::PageBack)
            || matches!(input, Input::Tap(x, y) if navigation.back().contains(x, y))
        {
            if self.screen == Screen::MenuSearch {
                self.leave_menu_search();
                return Change::Partial;
            }
            self.show(match self.screen {
                Screen::LibraryFind => Screen::Library,
                Screen::VocabularySearch => Screen::Vocabulary,
                Screen::EditNote if self.saved_popup.is_some() => {
                    if self.fixed.is_some() {
                        Screen::Fixed
                    } else {
                        Screen::Book
                    }
                }
                Screen::EditNote => Screen::Annotation,
                Screen::BookmarkFind => {
                    self.annotation_search.origin.unwrap_or(self.tools_screen())
                }
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
        if self.screen == Screen::BookmarkFind
            && self.annotation_search.toggle_at(navigation.width, x, y)
        {
            return Change::Partial;
        }
        if self.screen == Screen::Find {
            if let Some(change) = self.nearby_input(x, y) {
                return change;
            }
            if self.nearby.deadline().is_some() {
                if let Err(error) = self.cancel_directional_search() {
                    return self.report(error);
                }
            }
        }
        if self.screen == Screen::Find && search.find_option(FIND_CASE).contains(x, y) {
            self.find_match_case = !self.find_match_case;
            return Change::Partial;
        }
        if self.screen == Screen::Find
            && self.active.is_some()
            && search.find_option(FIND_PATTERN).contains(x, y)
        {
            self.find_regex = !self.find_regex;
            if self.find_regex {
                self.find_ignore_accents = false;
            }
            self.notice = None;
            return Change::Partial;
        }
        if self.screen == Screen::Find && search.find_option(FIND_ACCENTS).contains(x, y) {
            self.find_regex = false;
            self.find_ignore_accents = !self.find_ignore_accents;
            return Change::Partial;
        }
        if self.screen == Screen::DictionaryEntry
            && let Some(change) = self.open_dictionary_preset_picker(x, y)
        {
            return change;
        }
        let layout = navigation.keyboard();
        let action = match self.screen {
            Screen::DictionaryEntry => "Look up",
            Screen::MenuSearch => "Search",
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
                Screen::MenuSearch => self.submit_menu_search(&query),
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
        self.close_fixed_search_fit()?;
        let match_case = self.find_match_case;
        let ignore_accents = self.find_ignore_accents;
        if self.fixed.is_none() {
            self.start_all_reflow(query)?;
            self.matches_return_to_reading = false;
            return Ok(());
        }
        let pdf_search = PdfSearch::with_options(query, match_case, ignore_accents)?
            .with_context_words(self.search_context_words());
        self.update_lookup_history(|history| {
            history.search(query, match_case, ignore_accents, false)
        })?;
        self.clear_flexible_results();
        self.matches_return_to_reading = false;
        self.remember_all_results_origin();
        self.nearby.clear();
        self.search_result = None;
        self.reset_match_window();
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
            state.pdf_highlights[index].updated_at = crate::mark_time::now();
        } else {
            state.highlights[index].note = note;
            state.highlights[index].updated_at = crate::mark_time::now();
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
        if let Some(change) = self.scroll_delay_input(input) {
            return change;
        }
        if let Some(change) = self.footnote_font_input(input) {
            return change;
        }
        if let Some(change) = self.logical_font_input(input) {
            return change;
        }
        if let Some(change) = self.content_dpi_input(input) {
            return change;
        }
        if let Some(change) = self.line_spacing_input(input) {
            return change;
        }
        if let Some(change) = self.word_spacing_input(input) {
            return change;
        }
        if let Some(change) = self.profile_value_input(input) {
            return change;
        }
        let navigation = if self.screen.uses_navigation_viewport() {
            self.navigation_layout()
        } else {
            navigation_layout::NavigationLayout::PORTRAIT
        };
        if matches!(input, Input::PageBack)
            || matches!(input, Input::Tap(x, y) if navigation.back().contains(x, y))
        {
            if let Screen::NumberingValue(NumberField::FooterItemWidth(item, draft)) = self.screen {
                self.show(Screen::FooterItemWidths);
                self.open_footer_width(item, Some(draft));
                return Change::Partial;
            }
            self.show(match self.screen {
                Screen::NumberingValue(field) => field.parent(),
                Screen::FootnoteFontSize(_) => Screen::LinkSettings,
                Screen::FooterCustomText | Screen::FooterCustomTextGuide => Screen::FooterSettings,
                _ if self.fixed.is_some() => Screen::FixedTools,
                _ => Screen::BookTools,
            });
            return Change::Partial;
        }
        let Input::Tap(x, y) = input else {
            return Change::None;
        };
        let layout = navigation.keyboard();
        let action = if matches!(
            self.screen,
            Screen::NumberingValue(NumberField::FooterItemWidth(_, _))
        ) {
            "Use width"
        } else if matches!(
            self.screen,
            Screen::NumberingValue(_) | Screen::FootnoteFontSize(_)
        ) {
            "Set"
        } else if self.screen == Screen::FooterCustomText {
            "Save"
        } else {
            "Go"
        };
        let Some(key) = keyboard_key(&layout, &self.editor, action, x, y) else {
            return Change::None;
        };
        let pressed = self.editor.press(key);
        if self.screen == Screen::GoToPage && !matches!(pressed, Press::Go) {
            self.screen_page_jump = None;
            self.notice = None;
        }
        if matches!(pressed, Press::Go) {
            self.screen_page_jump = None;
            match self.screen {
                Screen::NumberingValue(field) => {
                    return match self.apply_numbering_input(field) {
                        Ok(()) => Change::Partial,
                        Err(error) => self.report(error),
                    };
                }
                Screen::FootnoteFontSize(mode) => {
                    return match self.apply_footnote_font_input(mode) {
                        Ok(()) => Change::Partial,
                        Err(error) => self.report(error),
                    };
                }
                Screen::FooterCustomText => {
                    return match self.save_footer_custom_text() {
                        Ok(()) => Change::Partial,
                        Err(error) => self.report(error),
                    };
                }
                _ => {}
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

    fn handle_pdf_password(&mut self, input: Input) -> Change {
        let navigation = self.navigation_layout();
        if matches!(input, Input::PageBack)
            || matches!(input, Input::Tap(x, y) if navigation.back().contains(x, y))
        {
            self.closing_before_switch = None;
            self.locked = None;
            self.editor = Editor::new(64);
            if self.bookmark_browser.after_password.take().is_some() {
                self.show(Screen::BookmarkBrowser(bookmark_browser::Page::Detail));
                return Change::Partial;
            }
            if let Some(screen) = self.folder_password_return.take() {
                self.show(screen);
                return Change::Partial;
            }
            self.library_viewport = self.reading_viewport();
            return match self.activate(Action::Show(Screen::Library)) {
                Ok(()) => Change::Partial,
                Err(error) => self.report(error),
            };
        }
        let Input::Tap(x, y) = input else {
            return Change::None;
        };
        let layout = navigation.keyboard();
        let Some(key) = keyboard_key(&layout, &self.editor, "Open", x, y) else {
            return Change::None;
        };
        if matches!(self.editor.press(key), Press::Go) {
            let Some(entry) = self.locked.clone() else {
                return self.report("Choose the PDF again");
            };
            let password = self.editor.string();
            self.editor = Editor::new(64);
            if let Err(error) = self
                .open_with_password(entry, Some(&password))
                .and_then(|_| self.bookmark_after_password())
            {
                if self.locked.is_none() {
                    self.closing_before_switch = None;
                }
                return self.report(error);
            }
        }
        Change::Partial
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
        if self.pending_selection_action.is_some() && !matches!(input, Input::LongTap(..)) {
            self.pending_selection_action = None;
            self.cjk_selection_lookup = None;
        }
        if let Input::LongTap(x, y) = input
            && self.selection.is_none()
            && let Some(change) = self.hold_footer(x, y)
        {
            return change;
        }
        if let Input::LongTap(x, y) = input
            && self.selection_mode
        {
            self.extend_reflow_selection(x, y);
            return Change::Partial;
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
            self.pending_selection_action = None;
            self.selection = active.reading.word_at(x, y);
            self.start_cjk_selection();
            self.notice = None;
            return if self.selection.is_some() {
                self.handle_text_long_press()
            } else {
                Change::Partial
            };
        }
        if let Input::Tap(x, y) = input {
            if self.selection.is_some() {
                if let Some(change) = self.selection_popup_input(x, y) {
                    return change;
                }
                self.extend_reflow_selection(x, y);
                return Change::Partial;
            }
            if let Some(change) = self.search_result_input(x, y) {
                return change;
            }
            if self.reading_viewport().done().contains(x, y) {
                return self.exit();
            }
            if let Some((link, link_x, link_y)) = self.reflow_tap_link(x, y) {
                return self.open_reflow_link(link, link_x, link_y);
            }
            if let Some(change) = self.tap_footer(x, y) {
                return change;
            }
            if self.reveal_reading_tools(x, y, Screen::BookTools) {
                return Change::Partial;
            }
        }
        let Some(active) = &self.active else {
            return Change::None;
        };
        let input = match input {
            Input::Tap(x, y) => self.tap_turn(x, y, active.reading.frame_height()),
            Input::Swipe { dx, dy, .. } if dx.abs() > dy.abs() => self.swipe_turn(dx, dy),
            other => Some(other),
        };
        let Some(input) = input else {
            return Change::None;
        };
        let active = self.active.as_mut().unwrap();
        let previous = (active.reading.chapter, active.reading.page);
        let change = active.reading.input(input);
        self.finish_book_update(change, previous)
    }

    fn finish_book_update(&mut self, change: Change, previous: (usize, usize)) -> Change {
        let attempted = self
            .active
            .as_mut()
            .is_some_and(|active| active.reading.take_end_attempt());
        if change != Change::Partial {
            return self.finish_end_attempt(attempted, change);
        }
        let page_changed = self.active.as_ref().is_some_and(|active| {
            active.reading.paged_frame_ready()
                && (active.reading.chapter, active.reading.page) != previous
        });
        self.notice = None;
        self.reset_back_resistance();
        self.clear_stale_selection();
        self.record_position();
        let change = self.reading_update(page_changed);
        self.finish_end_attempt(attempted, change)
    }

    fn reading_update(&mut self, page_changed: bool) -> Change {
        if page_changed {
            if self.screen != Screen::Skim {
                self.reading_guide_progress
                    .update(self.controls.reading_guides);
            }
            self.controls
                .page_refresh
                .after_page(&mut self.pages_since_refresh)
        } else {
            Change::Partial
        }
    }

    fn exit(&mut self) -> Change {
        match self.request_close(profile_panel::on_close::AfterClose::Exit) {
            Ok(change) => change,
            Err(error) => self.report(error),
        }
    }

    fn handle_fixed(&mut self, input: Input) -> Change {
        if self.ocr.is_some()
            && self.pdf_selection.is_none()
            && (input == Input::PageBack
                || matches!(input, Input::Tap(x, y)
                    if self.reading_viewport().bottom_right(CANCEL_OCR).contains(x, y)))
        {
            self.ocr = None;
            self.notice = Some("Text recognition cancelled".into());
            return Change::Partial;
        }
        let show_zoom = !self.hidden_overlap_footer();
        if let Input::LongTap(x, y) = input
            && self.pdf_selection.is_none()
            && let Some(change) = self.hold_footer(x, y)
        {
            return change;
        }
        if let Input::LongTap(x, y) = input
            && self.selection_mode
            && let (Some(fixed), Some(selection)) = (&self.fixed, &mut self.pdf_selection)
        {
            self.notice = (!fixed.reading.extend_selection(selection, x, y))
                .then(|| "A readable text layer is needed on each selected page".into());
            return Change::Partial;
        }
        if let Input::LongTap(x, y) = input
            && let Some(fixed) = &mut self.fixed
            && fixed.entry.format.is_comic()
        {
            return match fixed.reading.inspect_comic_panel(x, y) {
                Ok(Some(panel)) => {
                    fixed.reading.pause();
                    match self.open_illustration_view(panel) {
                        Ok(()) => Change::Partial,
                        Err(error) => self.report(error),
                    }
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
                    self.handle_text_long_press()
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
            if self.pdf_selection.is_some() {
                if let Some(change) = self.selection_popup_input(x, y) {
                    return change;
                }
                if let (Some(fixed), Some(selection)) = (&self.fixed, &mut self.pdf_selection) {
                    if !fixed.reading.extend_selection(selection, x, y) {
                        self.notice =
                            Some("A readable text layer is needed on each selected page".into());
                    }
                }
                return Change::Partial;
            }
            if let Some(change) = self.search_result_input(x, y) {
                return change;
            }
            if self.reading_viewport().done().contains(x, y) {
                return self.exit();
            }
            if self
                .reading_viewport()
                .bottom_right(NEXT_COMIC)
                .contains(x, y)
                && self.ocr.is_none()
                && let Some(next) = self.next_comic().cloned()
            {
                return match self.open_book(next) {
                    Ok(()) => Change::Partial,
                    Err(error) => self.report(error),
                };
            }
            if let Some(link) = self.fixed_tap_link(x, y) {
                return match self.open_fixed_link(link) {
                    Ok(()) => Change::Partial,
                    Err(error) => self.report(error),
                };
            }
            if let Some(change) = self.tap_footer(x, y) {
                return change;
            }
            if self.reveal_reading_tools(x, y, Screen::FixedTools) {
                return Change::Partial;
            }
        }
        let Some(fixed) = &self.fixed else {
            return Change::None;
        };
        let page = fixed.reading.page();
        let input = match input {
            Input::Tap(x, y)
                if !(show_zoom && {
                    let (smaller, larger) = fixed.reading.zoom_buttons();
                    smaller.contains(x, y) || larger.contains(x, y)
                }) =>
            {
                self.tap_turn(x, y, fixed.reading.frame_height())
            }
            Input::Swipe { dx, dy, .. } if !fixed.reading.swipe_pans(dx, dy) => {
                self.swipe_turn(dx, dy)
            }
            other => Some(other),
        };
        let Some(input) = input else {
            return Change::None;
        };
        let fixed = self.fixed.as_mut().unwrap();
        let result = fixed.reading.input_with_status(input, show_zoom);
        self.finish_fixed_update(result, page)
    }

    fn finish_fixed_update(&mut self, result: Result<bool>, previous: usize) -> Change {
        let attempted = self
            .fixed
            .as_mut()
            .is_some_and(|fixed| fixed.reading.take_end_attempt());
        let change = match result {
            Ok(true) => {
                let (changed, continuous) = self.fixed.as_ref().map_or((false, false), |fixed| {
                    (
                        fixed.reading.page() != previous,
                        fixed.reading.is_continuous(),
                    )
                });
                self.notice = None;
                self.reset_back_resistance();
                self.cancel_ocr_selection();
                if changed || continuous {
                    self.nearby.clear();
                    self.search_result = None;
                    if !self.selection_mode {
                        self.pdf_selection = None;
                    }
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
            return self
                .nearby
                .can_step(direction)
                .then_some(Action::StepNearby(direction));
        }
        let index = self.search_result_order().step(current, step)?;
        Some(if self.fixed.is_some() {
            Action::PdfSearchHit(index)
        } else {
            Action::SearchHit(index)
        })
    }

    fn search_result_input(&mut self, x: f32, y: f32) -> Option<Change> {
        if self.ocr.is_some() {
            return None;
        }
        if self.fixed_result_navigation() && !self.nearby.searching_direction() {
            return self.fixed_result_input(x, y);
        }
        let layout = self.search_layout();
        let step = if layout.reading_footer(MATCH_PREV).contains(x, y) {
            -1
        } else if layout.reading_footer(MATCH_NEXT).contains(x, y) {
            1
        } else {
            return None;
        };
        if self.nearby.searching_direction() {
            return Some(match self.cancel_directional_search() {
                Ok(()) => Change::Partial,
                Err(error) => self.report(error),
            });
        }
        let action = self.search_step(step)?;
        Some(match self.activate(action) {
            Ok(()) => Change::Partial,
            Err(error) => self.report(error),
        })
    }

    fn draw_search_steps(&self, canvas: &mut Canvas) {
        let layout = self.search_layout();
        if self.draw_fixed_list_search(canvas) {
            return;
        }
        if self.nearby.searching_direction() {
            canvas.button(layout.reading_footer(MATCH_PREV), "Cancel", false, false);
            return;
        }
        if self.draw_fixed_result_navigation(canvas) {
            return;
        }
        if self.search_step(-1).is_some() {
            canvas.button(layout.reading_footer(MATCH_PREV), "Previous", false, false);
        }
        if self.search_step(1).is_some() {
            canvas.button(layout.reading_footer(MATCH_NEXT), "Next", false, false);
        }
    }

    fn search_caption(&self) -> Option<String> {
        let index = self.search_result?;
        if self.nearby.result() {
            if let Some(fixed) = &self.fixed {
                return Some(format!(
                    "Matching page {} of {}",
                    fixed.reading.page() + 1,
                    fixed.reading.count()
                ));
            }
            return Some("Search match".into());
        }
        let order = self.search_result_order();
        let ordinal = order.ordinal(index)?;
        let count = order.len();
        let found = if self.search_at.is_some() || self.nearby.searching_all() {
            " found"
        } else {
            ""
        };
        Some(format!("Match {} of {count}{found}", ordinal + 1))
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
        let attempted = self
            .text_flow
            .as_mut()
            .is_some_and(TextFlow::take_end_attempt);
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
        let viewport = self.reading_viewport();
        let input = match input {
            Input::Tap(x, y) if y < viewport.page_height() as f32 => {
                self.tap_turn(x, y, viewport.page_height())
            }
            Input::Swipe { dx, dy, .. } if dx.abs() > dy.abs() => self.swipe_turn(dx, dy),
            other => Some(other),
        };
        let Some(input) = input else {
            return Change::None;
        };
        let (Some(scan), Some(fixed)) = (&mut self.scan_columns, &mut self.fixed) else {
            return Change::None;
        };
        let direction = match input {
            Input::Tap(x, y) if viewport.done().contains(x, y) => {
                if let Err(error) = fixed.reading.restore_view(scan.original) {
                    return self.report(error);
                }
                self.show(Screen::Fixed);
                return Change::Partial;
            }
            Input::PageForward => 1,
            Input::PageBack => -1,
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
        if self
            .ocr
            .as_ref()
            .is_some_and(|job| job.selection_point.is_some())
        {
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
        let page = self
            .fixed
            .as_ref()
            .ok_or("Open a PDF or DjVu first")?
            .reading
            .page();
        self.recognize_page(page, language, None)
    }

    fn recognize_page(
        &mut self,
        page: usize,
        language: Option<&str>,
        selection_point: Option<hayro::kurbo::Point>,
    ) -> Result<()> {
        self.ocr = None;
        let assets = self
            .ocr_assets
            .as_ref()
            .ok_or("OCR requires an installed language pack")?;
        let fixed = self.fixed.as_ref().ok_or("Open a PDF or DjVu first")?;
        let explicit_language = language.is_some();
        let language = language
            .or(fixed.reading.ocr_language.as_deref())
            .unwrap_or_else(|| assets.default_language())
            .to_owned();
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
        let requested_word = active.selection_point.filter(|_| {
            self.screen == Screen::Fixed && fixed.reading.visible_pages().any(|p| p == page)
        });
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
        if let Some(change) = self.semi_auto_crop_input(input) {
            return change;
        }
        let Some((original, first)) = self.crop_dialog else {
            return Change::None;
        };
        let Some(fixed) = &self.fixed else {
            return Change::None;
        };
        let viewport = self.reading_viewport();
        let auto = viewport.bottom_right(AUTO_CROP);
        let (next, save) = match input {
            Input::PageBack => (original, false),
            Input::Tap(x, y) if viewport.bottom_right(CANCEL_CROP).contains(x, y) => {
                (original, false)
            }
            Input::Tap(x, y) if viewport.bottom_right(RESET_CROP).contains(x, y) => (
                FixedPreferences {
                    crop: None,
                    semi_auto_crop: false,
                    continuous_crop: None,
                    offset_x: 0.0,
                    offset_y: 0.0,
                    ..original
                },
                true,
            ),
            Input::Tap(x, y) => {
                let region = if auto.contains(x, y) {
                    fixed.reading.auto_crop()
                } else {
                    if y >= viewport.page_height() as f32 {
                        return Change::None;
                    }
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
                        semi_auto_crop: false,
                        continuous_crop: None,
                        offset_x: 0.0,
                        offset_y: 0.0,
                        ..original
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
        if self.selection.is_none() && self.pdf_selection.is_none() {
            self.selection_mode = false;
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
        let bytes =
            read_import_file(&self.import_path, crate::reading_backup::LIMIT).map_err(|error| {
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
        let entry = self
            .fixed
            .as_ref()
            .map(|book| &book.entry)
            .or_else(|| self.active.as_ref().map(|book| &book.entry))
            .ok_or("Open the matching book first")?;
        let nearby = if copied_only {
            None
        } else {
            with_storage(|| self.library.nearby_koreader_metadata(&entry.path))?
        };
        let source = if nearby.is_some() {
            "Saved beside this book"
        } else {
            "Copied to Reader data"
        };
        let path =
            nearby.unwrap_or_else(|| self.import_path.with_file_name("koreader-progress.lua"));
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
        self.position_import = Some(PositionImport {
            sidecar,
            target,
            source,
        });
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
                            updated_at: None,
                            chapter_title: None,
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

    fn find_selection(&mut self, origin: SelectionSearchOrigin) -> Result<()> {
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
        let shortened =
            query.len() > crate::search::MAX_QUERY_BYTES || query.chars().count() > excerpt.len();
        // The selection action uses the normal literal search, regardless of
        // the options last selected in the Find input.
        let from_reading = origin == SelectionSearchOrigin::Reading;
        if from_reading {
            self.find_match_case = false;
            self.find_ignore_accents = false;
        }
        self.find_regex = false;
        if shortened {
            self.show(Screen::Find);
        }
        self.editor = Editor::new(128);
        self.editor.caret = excerpt.len();
        self.editor.text = excerpt;
        if shortened {
            self.notice = Some("Edit this excerpt to search a shorter passage".into());
        } else if !from_reading || self.controls.selection_search_all {
            self.start_document_search(&query)?;
            self.matches_return_to_reading = from_reading;
        } else {
            self.begin_nearby(crate::search::SearchDirection::Next, false)?;
            self.matches_return_to_reading = false;
            self.hits.clear();
            self.pdf_hits.clear();
        }
        self.selection = None;
        self.pdf_selection = None;
        self.selection_mode = false;
        self.result_selection = None;
        self.pdf_result_regions.clear();
        Ok(())
    }

    fn save_highlight(&mut self, note: Option<String>) -> Result<()> {
        self.save_highlight_with_navigation(note, true)
    }

    fn save_highlight_in_place(&mut self, note: Option<String>) -> Result<()> {
        self.save_highlight_with_navigation(note, false)
    }

    fn save_highlight_with_navigation(
        &mut self,
        note: Option<String>,
        navigate_to_highlights: bool,
    ) -> Result<()> {
        let active = self.active.as_ref().ok_or("No open book")?;
        let selection = self.selection.as_ref().ok_or("Select some text first")?;
        if selection.text.len() > 8192 {
            return Err("Select a shorter passage".into());
        }
        let chosen_color = self.pending_highlight_color;
        let chosen_style = self.pending_highlight_style;
        let chapter_title = self.highlight_chapter_title(&selection.start);
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
                previous.updated_at = crate::mark_time::now();
            }
        } else {
            if state.highlights.len() >= 256 {
                return Err("This book has reached its highlight limit".into());
            }
            state.highlights.push(Highlight {
                updated_at: None,
                chapter_title,
                color: chosen_color.unwrap_or_else(|| {
                    state
                        .highlight_color
                        .unwrap_or(self.controls.highlight_color)
                }),
                created_at: crate::mark_time::now(),
                style: chosen_style.unwrap_or_else(|| {
                    state
                        .highlight_style
                        .unwrap_or(self.controls.highlight_style)
                }),
                selection: selection.clone(),
                note,
            });
        }
        self.selection = None;
        self.selection_mode = false;
        self.mark_book_changed();
        if navigate_to_highlights {
            self.show(Screen::Book);
        }
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
        self.save_book_highlight_export(id, format.extension(), &contents)
    }

    fn export_book_clippings(&mut self) -> Result<()> {
        use annotation_export::kindle_clippings;
        let id = self.open_book_id().ok_or("No open book")?;
        let state = self.library.state(id).ok_or("No reading history")?;
        let mut contents = String::new();
        if let Some(active) = &self.active {
            let reading = &active.reading;
            for mark in &state.highlights {
                let start = &mark.selection.start;
                let location = reading
                    .probe
                    .chapter_entries()
                    .iter()
                    .position(|chapter| chapter.href == start.chapter)
                    .map(|chapter| {
                        let page = (chapter == reading.chapter)
                            .then(|| reading.probe.page_for(start))
                            .flatten();
                        match page {
                            Some(page) => format!("Chapter {}, page {}", chapter + 1, page + 1),
                            None => format!("Chapter {}", chapter + 1),
                        }
                    });
                kindle_clippings::write_highlight(
                    &mut contents,
                    &state.title,
                    state.author.as_deref(),
                    location.as_deref(),
                    mark.created_at,
                    &mark.selection.text,
                    mark.note.as_deref(),
                );
            }
        }
        if self.fixed.is_some() {
            for mark in &state.pdf_highlights {
                let location = format!("page {}", mark.page + 1);
                kindle_clippings::write_highlight(
                    &mut contents,
                    &state.title,
                    state.author.as_deref(),
                    Some(&location),
                    mark.created_at,
                    &mark.text,
                    mark.note.as_deref(),
                );
            }
        }
        self.save_book_highlight_export(id, "clippings.txt", &contents)
    }

    fn save_book_highlight_export(
        &mut self,
        id: u64,
        extension: &str,
        contents: &str,
    ) -> Result<()> {
        let state = self.library.state(id).ok_or("No reading history")?;
        let path = self.write_annotation_export(
            &self.export_settings.preferences,
            export_settings::FilenameContext {
                source: Some(state.into()),
                bundled: false,
                started_at: crate::mark_time::now(),
                last_opened: None,
            },
            extension,
            contents.as_bytes(),
        )?;
        self.notice = Some(format!("Exported to {path}"));
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
                    updated_at: None,
                    chapter_title: None,
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
        self.save_pdf_highlight_with_navigation(note, true)
    }

    fn save_pdf_highlight_in_place(&mut self, note: Option<String>) -> Result<()> {
        self.save_pdf_highlight_with_navigation(note, false)
    }

    fn save_pdf_highlight_with_navigation(
        &mut self,
        note: Option<String>,
        navigate_to_highlights: bool,
    ) -> Result<()> {
        let fixed = self.fixed.as_ref().ok_or("No open PDF")?;
        let selection = self
            .pdf_selection
            .as_ref()
            .ok_or("Select some text first")?;
        if selection.text().len() > 8192 {
            return Err("Select a shorter passage".into());
        }
        let note = note
            .map(|note| note.trim().to_owned())
            .filter(|note| !note.is_empty());
        let chosen_color = self.pending_highlight_color;
        let chosen_style = self.pending_highlight_style;
        let mut candidate = selection.highlight().clone();
        candidate.color = chosen_color.unwrap_or_else(|| {
            self.library
                .state(fixed.id)
                .and_then(|state| state.highlight_color)
                .unwrap_or(self.controls.highlight_color)
        });
        candidate.style = chosen_style.unwrap_or_else(|| {
            self.library
                .state(fixed.id)
                .and_then(|state| state.highlight_style)
                .unwrap_or(self.controls.highlight_style)
        });
        candidate.created_at = crate::mark_time::now();
        candidate.chapter_title = self.fixed_chapter_title(candidate.page);
        candidate.note = note.clone();
        let state = self
            .library
            .remember(&fixed.entry, fixed.id, &fixed.entry.title);
        if let Some(mark) = state.pdf_highlights.iter_mut().find(|mark| {
            mark.page == candidate.page
                && mark.bounds == candidate.bounds
                && mark.continuation == candidate.continuation
        }) {
            if note.is_some() {
                mark.note = note;
                mark.updated_at = crate::mark_time::now();
            }
        } else {
            state.add_pdf_highlight(candidate)?;
        }
        self.pdf_selection = None;
        self.selection_mode = false;
        self.mark_book_changed();
        if navigate_to_highlights {
            self.show(Screen::Fixed);
        }
        self.notice = Some("Highlight saved".into());
        Ok(())
    }
}

impl Reader {
    fn draw_content(&self, canvas: &mut Canvas) {
        if self.screen == Screen::DictionaryButtons(dictionary_buttons::Page::Preview) {
            self.draw_dictionary_button_preview(canvas);
            return;
        }
        if self.dictionary_editor_fullscreen() {
            self.draw_definition_popup(canvas);
            return;
        }
        if self.draw_style_editor(canvas) {
            self.read_alarm.draw(canvas, false);
            return;
        }
        if self.screen == Screen::DocumentDefaults {
            self.draw_document_defaults(canvas);
            return;
        }
        if self.draw_reading_history(canvas) {
            return;
        }
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
        if self.draw_illustration(canvas) {
            return;
        }
        if self.screen == Screen::ReadFixedText
            && let Some(flow) = &self.text_flow
        {
            flow.draw(
                canvas,
                self.notice.as_deref(),
                self.uses_night_pages(),
                self.guide_lines(),
            );
            self.draw_end_dialog(canvas);
            return;
        }
        if self.screen == Screen::FixedCrop
            && let Some(fixed) = &self.fixed
        {
            if self.draw_semi_auto_crop(canvas) {
                return;
            }
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
            let viewport = fixed.reading.viewport();
            let (width, height) = viewport.dimensions();
            let top = viewport.page_height() as i32;
            let auto = viewport.bottom_right(AUTO_CROP);
            canvas.fill(Rect::new(0, top, width as i32, height as i32 - top), 255);
            let hint = if first.is_some() {
                "Tap the opposite corner"
            } else {
                "Tap one corner of the area to keep"
            };
            canvas.text(
                Rect::new(28, top + 12, auto.x - 44, 52),
                self.notice.as_deref().unwrap_or(hint),
                CAPTION,
            );
            draw_toolbar_button(canvas, auto, "Auto crop");
            draw_toolbar_button(canvas, viewport.bottom_right(RESET_CROP), "Reset");
            draw_toolbar_button(canvas, viewport.bottom_right(CANCEL_CROP), "Cancel");
            return;
        }
        if (matches!(
            self.screen,
            Screen::Fixed | Screen::FixedTools | Screen::OnlineLookup | Screen::Skim
        ) || self.definition_over_book())
            && let Some(fixed) = &self.fixed
        {
            let mut marks: Vec<_> = self
                .library
                .state(fixed.id)
                .into_iter()
                .flat_map(|state| &state.pdf_highlights)
                .enumerate()
                .filter_map(|(index, saved)| {
                    match self
                        .range_edit
                        .as_ref()
                        .filter(|edit| edit.index == index)
                        .map(|edit| &edit.draft)
                    {
                        Some(extend_highlight::Draft::Fixed { current, .. }) => Some(current),
                        None => Some(saved),
                        _ => None,
                    }
                })
                .flat_map(|mark| {
                    mark.parts().map(move |(page, rects, _)| {
                        (page, rects, mark.style, mark.color, mark.note.is_some())
                    })
                })
                .filter(|(index, _, _, _, _)| {
                    fixed.reading.visible_pages().any(|page| page == *index)
                })
                .flat_map(|(page, rects, style, color, note)| {
                    rects
                        .iter()
                        .filter(move |bounds| {
                            fixed.reading.visible_rectangle(page, **bounds).is_some()
                        })
                        .enumerate()
                        .map(move |(i, &bounds)| {
                            (
                                (page, bounds),
                                crate::highlight_style::Paint::Annotation {
                                    style,
                                    color,
                                    note,
                                    first: i == 0,
                                    night: self.uses_night_pages(),
                                },
                            )
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
                .flat_map(|selection| selection.page_bounds())
                .collect();
            let search_caption = self.search_caption();
            let footer_font_px = self.footer_caption_font_px();
            let footer_style = self.footer_caption_style();
            let footer_font_file = self.footer_font_render_file();
            let allow_custom_footer = self.notice.is_none()
                && search_caption.is_none()
                && self.pdf_selection.is_none()
                && self.selection.is_none();
            let footer_slot_width = fixed.reading.zoom_buttons().0.x.saturating_sub(60) as u32;
            let custom_footer = allow_custom_footer
                .then(|| {
                    self.custom_footer_line(canvas, footer_slot_width, footer_font_file.as_deref())
                })
                .flatten();
            let caption =
                (custom_footer.is_none() && self.notice.is_none() && search_caption.is_none())
                    .then(|| self.status_caption())
                    .flatten();
            let display_notice = self
                .notice
                .as_deref()
                .or(search_caption.as_deref())
                .or(caption.as_deref());
            let footer_caption = fixed.reading.footer_caption(display_notice);
            let status_text_width = custom_footer.as_ref().map_or_else(
                || {
                    if let Some(font_file) = footer_font_file.as_deref() {
                        canvas.measure_with_font_file(
                            &footer_caption,
                            footer_font_px,
                            self.controls.footer_item_bold,
                            font_file,
                        )
                    } else {
                        canvas.measure_with_weight(
                            &footer_caption,
                            footer_font_px,
                            self.controls.footer_item_bold,
                        )
                    }
                },
                |line| line.width(),
            );
            let status_rect = self.footer_caption_rect(
                custom_footer
                    .as_ref()
                    .and_then(|line| line.gap_width())
                    .unwrap_or_else(|| {
                        self.footer_caption_width(footer_slot_width, status_text_width)
                    }),
                status_text_width,
                footer_font_px,
            );
            fixed.reading.draw_with_footer_cover(
                canvas,
                if custom_footer.is_some() {
                    Some("")
                } else {
                    display_notice
                },
                &marks,
                &selection,
                self.page_paint(),
                self.hidden_overlap_footer(),
                status_rect,
                footer_style,
                self.controls.footer_horizontal_separator,
                footer_font_file.as_deref(),
                self.footer_cover_top(status_rect, status_text_width),
            );
            if let Some(line) = custom_footer {
                line.draw(
                    canvas,
                    status_rect,
                    footer_style,
                    footer_font_file.as_deref(),
                );
            }
            self.draw_footer_track(canvas, status_text_width);
            if self.saved_popup_over_book() {
                self.draw_saved_popup(canvas);
                return;
            }
            if self.screen == Screen::Fixed && self.range_edit.is_some() {
                self.draw_range_tools(canvas);
                return;
            }
            if self.screen == Screen::Fixed && self.touch_hold.is_none() {
                self.draw_selection_popup(canvas);
            }
            if self.screen == Screen::FixedTools {
                self.draw_reading_tools(canvas);
            } else if self.touch_locked {
                draw_locked_footer(canvas, self.reading_viewport(), self.notice.as_deref());
            } else if self.ocr.is_some() && selection.is_empty() {
                canvas.button(
                    self.reading_viewport().bottom_right(CANCEL_OCR),
                    "Cancel",
                    false,
                    false,
                );
            } else if self.screen == Screen::Fixed
                && selection.is_empty()
                && self.next_comic().is_some()
            {
                canvas.button(
                    self.reading_viewport().bottom_right(NEXT_COMIC),
                    "Next comic",
                    false,
                    false,
                );
            } else if self.screen == Screen::Fixed && selection.is_empty() {
                self.draw_search_steps(canvas);
                if self.fixed_search_fit_active() {
                    self.draw_fixed_search_fit_close(canvas);
                }
            }
            if let Some(popup) = &self.link_popup {
                popup.draw(canvas);
            }
            if self.screen == Screen::OnlineLookup {
                self.draw_online(canvas);
            }
            if self.definition_over_book() {
                self.draw_definition_popup(canvas);
            }
            self.draw_end_dialog(canvas);
            return;
        }
        if (matches!(
            self.screen,
            Screen::Footnote
                | Screen::Book
                | Screen::BookTools
                | Screen::OnlineLookup
                | Screen::Skim
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
                    let selected = self
                        .range_edit
                        .as_ref()
                        .filter(|edit| edit.index == index)
                        .and_then(|edit| match &edit.draft {
                            extend_highlight::Draft::Text { current, .. } => Some(current),
                            _ => None,
                        })
                        .unwrap_or(&highlight.selection);
                    reading
                        .highlight_rects(selected)
                        .into_iter()
                        .enumerate()
                        .map(move |(i, rect)| {
                            (
                                rect,
                                crate::highlight_style::Paint::Annotation {
                                    style: highlight.style,
                                    color: highlight.color,
                                    note: highlight.note.is_some(),
                                    first: i == 0,
                                    night: self.uses_night_pages(),
                                },
                            )
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
            let footer_font_px = self.footer_caption_font_px();
            let footer_style = self.footer_caption_style();
            let footer_font_file = self.footer_font_render_file();
            let note_covers_footer = self.footnote.is_some();
            let allow_custom_footer = !note_covers_footer
                && reading.footer_caption(Some("")).is_empty()
                && self.notice.is_none()
                && search_caption.is_none()
                && self.selection.is_none()
                && self.pdf_selection.is_none();
            let footer_slot_width = self.reading_viewport().done().x.saturating_sub(60) as u32;
            let custom_footer = allow_custom_footer
                .then(|| {
                    self.custom_footer_line(canvas, footer_slot_width, footer_font_file.as_deref())
                })
                .flatten();
            let caption =
                (custom_footer.is_none() && self.notice.is_none() && search_caption.is_none())
                    .then(|| self.status_caption())
                    .flatten();
            let display_notice = self
                .notice
                .as_deref()
                .or(search_caption.as_deref())
                .or(caption.as_deref());
            let footer_caption = reading.footer_caption(display_notice);
            let status_text_width = custom_footer.as_ref().map_or_else(
                || {
                    if let Some(font_file) = footer_font_file.as_deref() {
                        canvas.measure_with_font_file(
                            &footer_caption,
                            footer_font_px,
                            self.controls.footer_item_bold,
                            font_file,
                        )
                    } else {
                        canvas.measure_with_weight(
                            &footer_caption,
                            footer_font_px,
                            self.controls.footer_item_bold,
                        )
                    }
                },
                |line| line.width(),
            );
            let status_rect = self.footer_caption_rect(
                custom_footer
                    .as_ref()
                    .and_then(|line| line.gap_width())
                    .unwrap_or_else(|| {
                        self.footer_caption_width(footer_slot_width, status_text_width)
                    }),
                status_text_width,
                footer_font_px,
            );
            reading.draw_page_with_footer_cover(
                canvas,
                (self.selection.is_none() && !note_covers_footer).then_some("Exit"),
                self.hidden_overlap_footer(),
                if custom_footer.is_some() {
                    Some("")
                } else {
                    display_notice
                },
                status_rect,
                footer_style,
                &highlights,
                &selection,
                self.page_paint(),
                self.controls.footer_horizontal_separator && !note_covers_footer,
                footer_font_file.as_deref(),
                self.footer_cover_top(status_rect, status_text_width),
            );
            if let Some(line) = custom_footer {
                line.draw(
                    canvas,
                    status_rect,
                    footer_style,
                    footer_font_file.as_deref(),
                );
            }
            self.draw_publisher_margins(canvas);
            self.draw_top_header(canvas);
            if !note_covers_footer {
                self.draw_footer_track(canvas, status_text_width);
            }
            if self.saved_popup_over_book() {
                self.draw_saved_popup(canvas);
                return;
            }
            if self.screen == Screen::Book && self.range_edit.is_some() {
                self.draw_range_tools(canvas);
                return;
            }
            if self.screen == Screen::Book && self.touch_hold.is_none() {
                self.draw_selection_popup(canvas);
            }
            if self.screen == Screen::BookTools {
                self.draw_reading_tools(canvas);
            } else if self.touch_locked && !note_covers_footer {
                draw_locked_footer(canvas, self.reading_viewport(), self.notice.as_deref());
            } else if self.screen == Screen::Book && selection.is_empty() {
                self.draw_search_steps(canvas);
            }
            if let Some(note) = &self.footnote {
                note.draw(canvas, self.uses_night_pages());
            }
            if let Some(popup) = &self.link_popup {
                popup.draw(canvas);
            }
            if self.screen == Screen::OnlineLookup {
                self.draw_online(canvas);
            }
            if self.definition_over_book() {
                self.draw_definition_popup(canvas);
            }
            self.draw_end_dialog(canvas);
            return;
        }
        if self.screen == Screen::OnlineEdit {
            self.draw_online_editor(canvas);
            return;
        }
        if self.screen == Screen::OnlineLookup {
            self.draw_online(canvas);
            return;
        }
        if self.screen == Screen::Hyphenate {
            self.draw_hyphenation(canvas);
            return;
        }
        if self.screen == Screen::CustomTitle {
            self.draw_chapter_editor(canvas);
            return;
        }
        if self.screen == Screen::Library {
            self.draw_library(canvas);
            return;
        }
        if self.screen == Screen::Definition {
            self.draw_definition_popup(canvas);
            return;
        }
        let title = match (self.screen, self.book_list.as_ref()) {
            (Screen::Contents, _) if self.uses_alternative_contents() => "Alternative contents",
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
        let scoped_title;
        let title = if self.screen == Screen::MenuSearchResults {
            scoped_title = format!("Menu search · {}", self.menu_search.query());
            &scoped_title
        } else if self.screen == Screen::Matches && self.search_chapter_active() {
            scoped_title = format!("Chapter results ({})", self.search_result_order().len());
            &scoped_title
        } else {
            title
        };
        if self.screen == Screen::SelectionHtml {
            self.navigation_layout().header(canvas, title);
            self.draw_selection_html(canvas);
            return;
        }
        let navigation = if self.screen.uses_navigation_viewport() {
            let layout = self.navigation_layout();
            let action = if matches!(self.screen, Screen::ReadingProfileFields(_)) {
                "Save"
            } else {
                "Back"
            };
            layout.header_action(canvas, title, action);
            layout
        } else {
            header(canvas, title, "Back");
            navigation_layout::NavigationLayout::PORTRAIT
        };
        if let Screen::ExportSettings(_, export_settings::Page::Template(scope)) = self.screen {
            self.draw_export_template(canvas, scope);
            return;
        }
        if let Screen::PageTurnSizes(sizes) = self.screen {
            self.draw_page_turn_sizes(canvas, sizes);
            return;
        }
        self.draw_search_results_options(canvas);
        if self.screen == Screen::Matches && self.draw_flexible_results(canvas) {
            return;
        }
        if self.draw_flexible_marks(canvas) {
            return;
        }
        if self.screen.is_contents() {
            self.draw_contents(canvas);
            return;
        }
        if self.screen == Screen::LibraryExport(library_export::Page::Colors) {
            self.draw_export_colors(canvas);
            return;
        }
        if self.screen == Screen::VocabularyContext {
            self.draw_vocabulary_context(canvas);
            return;
        }
        if let Screen::BookmarkBrowser(page) = self.screen
            && self.draw_bookmark_browser_controls(canvas, page)
        {
            return;
        }
        if self.screen == Screen::BookMap {
            self.draw_book_map_tap_option(canvas);
            if self.fixed.is_some() {
                self.draw_fixed_book_map(canvas);
                return;
            }
        }
        if self.screen == Screen::PageOverview {
            self.draw_overview(canvas);
            return;
        }
        if matches!(
            self.screen,
            Screen::Find
                | Screen::MenuSearch
                | Screen::ContentsSearch
                | Screen::LibraryFind
                | Screen::VocabularySearch
                | Screen::DictionaryEntry
                | Screen::BookmarkFind
                | Screen::CollectionName
                | Screen::BookIdentityEdit(_)
                | Screen::ProfileName(_)
                | Screen::FooterPresetName(_)
                | Screen::DictionaryPresets(dictionary_presets::Page::Name(_))
                | Screen::FooterCustomText
                | Screen::Note
                | Screen::EditNote
                | Screen::GoToPage
                | Screen::NumberingValue(_)
                | Screen::FootnoteFontSize(_)
                | Screen::PdfPassword
        ) {
            let (hint, action) = match self.screen {
                Screen::Note | Screen::EditNote => ("Note text", "Save"),
                Screen::LibraryFind => ("Title, author or filename", "Find"),
                Screen::ContentsSearch => ("Heading text, or * to show all", "Find"),
                Screen::MenuSearch => ("Menu name", "Search"),
                Screen::VocabularySearch => ("Saved word", "Find"),
                Screen::DictionaryEntry => ("Word or phrase", "Look up"),
                Screen::BookmarkFind => ("Bookmark, highlighted text, or note", "Find"),
                Screen::CollectionName => ("Collection name", "Save"),
                Screen::BookIdentityEdit(field) => (field.label(), "Save"),
                Screen::ProfileName(
                    profile_panel::Name::OpeningPath(_) | profile_panel::Name::ClosingPath(_),
                ) => ("Path text, separated by commas", "Save"),
                Screen::ProfileName(
                    profile_panel::Name::OpeningMetadata(..)
                    | profile_panel::Name::ClosingMetadata(..),
                ) => ("Phrases separated by commas", "Save"),
                Screen::ProfileName(
                    profile_panel::Name::OpeningTime(.., part)
                    | profile_panel::Name::ClosingTime(.., part),
                ) => (part.range_label(), "Set"),
                Screen::ProfileName(_) => ("Profile name", "Save"),
                Screen::FooterPresetName(_) => ("Status bar preset name", "Save"),
                Screen::DictionaryPresets(dictionary_presets::Page::Name(_)) => {
                    ("Dictionary preset name", "Save")
                }
                Screen::FooterCustomText => (
                    "Use %T title, %A author, %c page, and %t total pages",
                    "Save",
                ),
                Screen::PdfPassword => ("PDF password", "Open"),
                Screen::FootnoteFontSize(mode) => (mode.hint(), "Set"),
                Screen::NumberingValue(NumberField::ContentDpi(_)) => {
                    ("Content zoom: 0 to 300 dpi", "Set")
                }
                Screen::NumberingValue(NumberField::LogicalFont(_)) => {
                    ("Logical font size: 12 to 255", "Set")
                }
                Screen::NumberingValue(NumberField::LineSpacing(_)) => {
                    ("Line spacing: 50 to 200%", "Set")
                }
                Screen::NumberingValue(NumberField::ProfileValue(_, _)) => {
                    ("Value in the saved profile", "Set")
                }
                Screen::NumberingValue(NumberField::WordSpacing(draft)) => (
                    match draft.field {
                        word_spacing::Field::Scaling => "Scaling: 10 to 500%",
                        word_spacing::Field::Minimum => "Reduction: 25 to 100%",
                    },
                    "Set",
                ),
                Screen::NumberingValue(NumberField::AutoTurn) => {
                    ("Page-turn interval: 1 to 86,400 seconds", "Set")
                }
                Screen::NumberingValue(NumberField::CornerHoldPause) => ("5 to 30 seconds", "Set"),
                Screen::NumberingValue(NumberField::ScrollDelay) => ("0 to 2,000 ms", "Set"),
                Screen::NumberingValue(
                    NumberField::AutoTurnOverlapHorizontal | NumberField::AutoTurnOverlapVertical,
                ) => ("Zoom overlap: 0 to 90%", "Set"),
                Screen::NumberingValue(NumberField::FixedTileZoom(_)) => {
                    ("Choose 0.1 to 8.0, in steps of 0.1", "Set")
                }
                Screen::NumberingValue(NumberField::CropMargin) => {
                    ("Choose 0.00 to 1.50 inches", "Set")
                }
                Screen::NumberingValue(NumberField::ReadingGuide(field)) => (field.hint(), "Set"),
                Screen::NumberingValue(NumberField::FooterTrackMinimumWidth) => {
                    ("Minimum progress bar width: 5 to 90%, in 5% steps", "Set")
                }
                Screen::NumberingValue(NumberField::FooterTrackMarginWidth) => {
                    ("Progress bar margin: 0 to 140 pixels", "Set")
                }
                Screen::NumberingValue(NumberField::FooterBatteryHideThreshold) => {
                    ("Hide battery when charge is above: 0 to 100%", "Set")
                }
                Screen::NumberingValue(NumberField::FooterItemFontSize) => {
                    ("Footer text size: 8 to 36; default 14", "Set")
                }
                Screen::NumberingValue(NumberField::FooterItemWidth(_, _)) => {
                    ("Screen width from 10 to 100%", "Use width")
                }
                Screen::NumberingValue(NumberField::FooterContainerHeight) => {
                    ("Footer item height: 7 to 98; default 14", "Set")
                }
                Screen::NumberingValue(NumberField::FooterContainerBottomMargin) => {
                    ("Footer bottom margin: 0 to 49; default 1", "Set")
                }
                Screen::NumberingValue(NumberField::FooterCustomTextRepetitions) => {
                    ("Custom text repetitions: 1 to 99", "Set")
                }
                Screen::NumberingValue(NumberField::MarginSize) => {
                    ("Margin text size from 8 to 20", "Set")
                }
                Screen::NumberingValue(_) => ("500 to 3,000 characters per page", "Set"),
                Screen::GoToPage if self.fixed.is_none() => {
                    ("Page, +/- pages, chapter:page, %, or label", "Go")
                }
                Screen::GoToPage => ("Page, +/- pages, %, or printed label", "Go"),
                Screen::Find if self.active.is_some() && self.find_regex => {
                    ("Regular expression", "Find all")
                }
                Screen::Find => ("Search text", "Find all"),
                _ => ("Search text", "Find"),
            };
            let search = self.search_layout();
            canvas.single_line_field(
                if self.screen == Screen::Find {
                    search.find_field(self.can_paste_text())
                } else if self.screen.is_numeric_editor() {
                    self.numeric_layout().field()
                } else {
                    navigation.body(LIST_Y, 130)
                },
                &self.editor,
                hint,
                self.screen == Screen::PdfPassword,
            );
            if self.screen.is_numeric_editor() {
                let numeric = self.numeric_layout();
                if numeric.is_landscape() && !self.editor.text.is_empty() {
                    canvas.text(numeric.hint(), hint, CAPTION);
                }
            }
            if !self.dictionary_preset_picker_open() {
                canvas.keyboard(self.editor.layer, action, None);
            }
            if let Screen::BookIdentityEdit(field) = self.screen
                && let Some(reset) = self.book_identity_reset(field)
            {
                canvas.button(reset, "Restore book metadata", false, false);
            }
            if self.screen == Screen::DictionaryEntry {
                self.draw_lookup_preset_button(canvas);
            }
            if let Some(button) = self.current_opening_book_button() {
                canvas.button(button, "Current book", false, false);
            }
            self.draw_scroll_delay(canvas);
            self.draw_content_dpi(canvas);
            self.draw_logical_font(canvas);
            self.draw_line_spacing(canvas);
            self.draw_profile_value_editor(canvas);
            self.draw_word_spacing(canvas);
            if self.screen == Screen::GoToPage {
                self.draw_page_jump_context(canvas);
            }
            if let Screen::FootnoteFontSize(mode) = self.screen {
                self.draw_footnote_font_editor(canvas, mode);
            }
            if self.can_paste_text() {
                canvas.button(self.editor_paste_rect(), "Paste", false, false);
            }
            if self.screen == Screen::BookmarkFind {
                self.annotation_search
                    .draw_filters(canvas, navigation.width);
            }
            if self.screen == Screen::Find {
                let label = if self.find_match_case {
                    "Match case on"
                } else {
                    "Match case off"
                };
                canvas.button(
                    search.find_option(FIND_CASE),
                    label,
                    self.find_match_case,
                    false,
                );
                canvas.button(
                    search.find_option(FIND_ACCENTS),
                    "Ignore accents",
                    self.find_ignore_accents,
                    false,
                );
                if self.active.is_some() {
                    canvas.button(
                        search.find_option(FIND_PATTERN),
                        "Regular expression",
                        self.find_regex,
                        false,
                    );
                }
                self.draw_nearby(canvas);
            }
            if let Some(notice) = &self.notice {
                canvas.text(
                    if self.screen == Screen::Find {
                        search.find_notice()
                    } else if self.screen.is_numeric_editor() {
                        self.numeric_layout().notice()
                    } else {
                        navigation.body(LIST_Y + 158, 75)
                    },
                    notice,
                    if self.screen.is_numeric_editor() && self.numeric_layout().is_landscape() {
                        CAPTION.lines(3)
                    } else {
                        CAPTION
                    },
                );
            }
            self.draw_opening_path_shortcuts(canvas);
            return;
        }
        let (count, mut items) = self.list_window();
        if let Some(current) = self
            .current_contents_row
            .and_then(|row| row.checked_sub(self.list_page * self.list_geometry().rows))
            .and_then(|row| items.get_mut(row))
        {
            let context = match self.screen {
                Screen::PublisherPages
                    if self
                        .active
                        .as_ref()
                        .is_some_and(|book| book.numbering.is_some()) =>
                {
                    "Current stable page"
                }
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
                title: if self.screen == Screen::Matches
                    || multiswipes::Shape::for_row(&item.action).is_some()
                {
                    ""
                } else {
                    &item.title
                },
                detail: item.detail.as_deref(),
                accessory: if self.screen == Screen::MetadataSaveSettings
                    || matches!(&item.action, Action::Heading | Action::MultiswipePreview(_))
                {
                    RowAccessory::None
                } else {
                    match &item.action {
                        Action::Multiswipe(multiswipes::Command::Toggle) => {
                            RowAccessory::Toggle(self.gestures.multiswipes.enabled())
                        }
                        Action::ToggleNotificationSource(source) => {
                            RowAccessory::Toggle(source.enabled(self.controls.notification_sources))
                        }
                        Action::ToggleOnlineHistory => RowAccessory::Toggle(
                            !self.lookup_history.online_languages.forget_queries,
                        ),
                        Action::ToggleRomanizations => RowAccessory::Toggle(
                            self.lookup_history.online_languages.show_romanizations,
                        ),
                        Action::ReadingHistory(reading_history::Command::Configure(next))
                            if next.calendar_uses_shift()
                                != self.reading_history.options().calendar_uses_shift() =>
                        {
                            RowAccessory::Toggle(
                                self.reading_history.options().calendar_uses_shift(),
                            )
                        }
                        Action::ToggleReadingProfileField(field) => {
                            RowAccessory::Toggle(self.profile_fields.contains(field))
                        }
                        Action::SetReadingProfileFields(_) => RowAccessory::None,
                        Action::OpeningProfileRule(index, command) => self
                            .profile_rule_checked(
                                profile_panel::opening_rules::Event::Opening,
                                *index,
                                command,
                            )
                            .map_or(RowAccessory::None, RowAccessory::Toggle),
                        Action::ClosingProfileRule(index, command) => self
                            .profile_rule_checked(
                                profile_panel::opening_rules::Event::Closing,
                                *index,
                                command,
                            )
                            .map_or(RowAccessory::None, RowAccessory::Toggle),
                        Action::SetProfileRendering(_, _) => RowAccessory::None,
                        Action::ToggleProfileMarginLink(index) => RowAccessory::Toggle(
                            self.profiles
                                .get(*index)
                                .and_then(Profile::margins_linked)
                                .unwrap_or(false),
                        ),
                        Action::ToggleWikipediaImage => RowAccessory::Toggle(
                            self.lookup_history.online_languages.show_wiki_image,
                        ),
                        Action::ToggleWikipediaMoreImages => RowAccessory::Toggle(
                            self.lookup_history.online_languages.show_wiki_more,
                        ),
                        Action::DictionarySource(SourceAction::LargeWindow) => {
                            RowAccessory::Toggle(self.dictionary_settings.large_window)
                        }
                        Action::DictionarySource(SourceAction::JustifyText) => {
                            RowAccessory::Toggle(self.dictionary_settings.justify_text)
                        }
                        Action::DictionarySource(SourceAction::DictionaryHistory) => {
                            RowAccessory::Toggle(
                                !self.dictionary_settings.disable_dictionary_history,
                            )
                        }
                        Action::DictionarySource(SourceAction::DefaultWordForms) => {
                            RowAccessory::Toggle(self.dictionary_settings.word_forms_default())
                        }
                        Action::DictionarySource(SourceAction::BookWordForms) => {
                            RowAccessory::Toggle(self.word_forms_enabled())
                        }
                        Action::DictionarySource(SourceAction::DefaultFuzzySearch) => {
                            RowAccessory::Toggle(self.dictionary_settings.fuzzy_search_default())
                        }
                        Action::DictionarySource(SourceAction::BookFuzzySearch) => {
                            RowAccessory::Toggle(self.fuzzy_search_enabled())
                        }
                        Action::ChapterLevel(setting) => {
                            RowAccessory::Toggle(self.chapter_level_checked(setting))
                        }
                        Action::Hyphenation(command) => self.hyphenation_accessory(*command),
                        Action::SetHangingPunctuation { enabled, .. } => {
                            RowAccessory::Toggle(!enabled)
                        }
                        Action::RespectEmbeddedLanguages(_) => {
                            RowAccessory::Toggle(self.active.as_ref().is_some_and(|book| {
                                book.reading.config.settings.typography.respect_embedded
                            }))
                        }
                        Action::PageTurnSetting(command) => self.page_turn_accessory(*command),
                        Action::ToggleLinkPreference(setting) => {
                            RowAccessory::Toggle(setting.checked(self.controls))
                        }
                        Action::SetMenuActivation(option) => {
                            RowAccessory::Toggle(option.checked(self.controls))
                        }
                        Action::SetReadingControls(_) if self.screen == Screen::TypographyRules => {
                            RowAccessory::Toggle(
                                self.controls
                                    .respect_embedded_languages_default
                                    .unwrap_or(true),
                            )
                        }
                        Action::SetReadingControls(_) if self.screen == Screen::Scrolling => {
                            RowAccessory::None
                        }
                        Action::MetadataArchive(metadata_archive::Command::Toggle(reason)) => {
                            RowAccessory::Toggle(self.metadata_archive.enabled(*reason))
                        }
                        Action::BookShortcut(book_shortcuts::Command::Recursive) => {
                            RowAccessory::Toggle(self.book_shortcuts.recursive())
                        }
                        Action::LiveMetadata(live_metadata::Command::Select(_)) => {
                            RowAccessory::None
                        }
                        Action::AnnotationExchange(command) => self
                            .annotation_exchange
                            .toggle(*command)
                            .map_or(RowAccessory::None, RowAccessory::Toggle),
                        Action::ExportSettings(_, export_settings::Command::ToggleBookFolder) => {
                            RowAccessory::Toggle(self.export_settings.preferences.book_folder)
                        }
                        Action::ExportSettings(_, export_settings::Command::ToggleBookName) => {
                            RowAccessory::Toggle(self.export_settings.preferences.book_title)
                        }
                        Action::LibraryExport(library_export::Command::ToggleKindleClippings) => {
                            RowAccessory::Toggle(self.library_export.kindle_clippings_enabled())
                        }
                        Action::SetReadingControls(next)
                            if self.screen == Screen::Gestures
                                && next.ignore_corner_holds
                                    != self.controls.ignore_corner_holds =>
                        {
                            RowAccessory::Toggle(self.controls.ignore_corner_holds)
                        }
                        Action::SetReadingControls(_) if self.screen == Screen::ReadingGuides => {
                            RowAccessory::Toggle(self.controls.reading_guides.enabled)
                        }
                        Action::SetReadingControls(next)
                            if self.screen == Screen::NavigationSettings =>
                        {
                            if next.remember_opening_location
                                != self.controls.remember_opening_location
                            {
                                RowAccessory::Toggle(self.controls.remember_opening_location)
                            } else {
                                RowAccessory::None
                            }
                        }
                        Action::SetReadingControls(controls)
                            if self.screen == Screen::FooterSettings
                                && controls.footer_item_cycle.is_some()
                                    != self.controls.footer_item_cycle.is_some() =>
                        {
                            RowAccessory::Toggle(self.controls.footer_item_cycle.is_some())
                        }
                        Action::SetReadingControls(controls)
                            if self.screen == Screen::SearchSettings
                                && controls.selection_search_all
                                    != self.controls.selection_search_all =>
                        {
                            RowAccessory::Toggle(self.controls.selection_search_all)
                        }
                        Action::SetReadingControls(controls)
                            if self.screen == Screen::SearchSettings
                                && controls.fixed_search_full_page
                                    != self.controls.fixed_search_full_page =>
                        {
                            RowAccessory::Toggle(self.controls.fixed_search_full_page)
                        }
                        _ => RowAccessory::Disclosure,
                    }
                },
            })
            .collect();
        let group = Rows {
            bounds: self.list_geometry().bounds(rows.len()),
            rows: &rows,
        };
        if self.screen == Screen::Matches {
            self.draw_match_rows(canvas, &group);
        } else if self.screen.is_bookmark_list() {
            self.draw_saved_mark_rows(canvas, &group, &items);
        } else if self.screen == Screen::ReadingFonts {
            self.draw_reading_font_rows(canvas, &group, &items);
        } else if self.screen == Screen::BookMap && navigation.mirrored {
            self.draw_mirrored_chapter_rows(canvas, &group);
        } else {
            group.draw(canvas, None);
        }
        for (index, item) in items.iter().enumerate() {
            if let Some(shape) = multiswipes::Shape::for_row(&item.action)
                && let Some(rect) = group.row_rect(index)
            {
                shape.draw(canvas, rect);
            }
        }
        if self.screen == Screen::BookMap
            && let Some(active) = &self.active
        {
            for (index, item) in items.iter().enumerate() {
                if let Action::BookMapChapter(chapter) = item.action
                    && active.reading.hidden_chapter(chapter).is_some()
                    && let Some(rect) = group.row_rect(index)
                {
                    // Gray rail keeps the shared row's text and disclosure target intact.
                    let rail = Rect::new(rect.x + 8, rect.y + 14, 12, rect.h - 28);
                    canvas.fill(navigation.horizontal(rail), 187);
                }
            }
        }
        if let Some(ordinal) = self.menu_search.focused_list(self.screen)
            && let Some(index) = ordinal.checked_sub(self.list_page * self.list_geometry().rows)
            && let Some(rect) = group.row_rect(index)
        {
            menu_search::mark_control(canvas, rect);
        }
        if rows.is_empty() {
            let empty = match self.screen {
                Screen::MenuSearchResults => "No matching menu entries",
                Screen::Bookmarks | Screen::FixedBookmarks | Screen::Highlights => {
                    "No saved marks in this book"
                }
                Screen::BookmarkMatches => "No matching bookmarks, highlights, or notes",
                Screen::BookmarkBrowser(bookmark_browser::Page::Results) => {
                    "No matching saved marks"
                }
                Screen::DictionaryHistory => "No lookups yet",
                Screen::OnlineHistory => "No completed online lookups",
                Screen::SearchHistory => "No recent searches",
                Screen::Vocabulary => "No saved words",
                Screen::Matches => self.search_results_empty_label(),
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
            canvas.text(navigation.body(LIST_Y, 110), empty, BODY);
        }
        if self.has_list_shortcuts(count) {
            let page_rows = self.list_geometry().rows;
            let last = count.saturating_sub(1) / page_rows;
            for (rect, icon, available) in [
                (
                    navigation.edge_footer(FIRST_MATCH_PAGE),
                    bar_icons::FIRST_PAGE,
                    self.list_page > 0,
                ),
                (
                    navigation.edge_footer(PREV_MATCH_PAGE),
                    bar_icons::PREVIOUS_PAGE,
                    self.list_page > 0,
                ),
                (
                    navigation.edge_footer(NEXT_MATCH_PAGE),
                    bar_icons::NEXT_PAGE,
                    self.list_page < last,
                ),
                (
                    navigation.edge_footer(LAST_MATCH_PAGE),
                    bar_icons::LAST_PAGE,
                    self.list_page < last,
                ),
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
                    if canvas.is_control_pressed(rect) {
                        255
                    } else if available {
                        20
                    } else {
                        178
                    },
                );
            }
            let first = self.list_page * page_rows + 1;
            let last = (first - 1 + page_rows).min(count);
            let more = if self.nearby.searching_all() || self.search_at.is_some() {
                "+"
            } else {
                ""
            };
            canvas.text(
                navigation.footer_text(435, 104, 72),
                &format!("{first}–{last} of {count}{more}"),
                self.search_layout().counter_style(),
            );
            if let Some(notice) = &self.notice {
                canvas.text(navigation.footer_text(LIST_X, 179, 64), notice, CAPTION);
            }
        } else {
            if self.list_page > 0 {
                canvas.button(navigation.previous(), "Previous", false, false);
            }
            let rows = self.list_geometry().rows;
            if (self.list_page + 1) * rows < count {
                canvas.button(navigation.next(), "Next", false, false);
            }
            if let Some(notice) = &self.notice {
                canvas.text(navigation.footer_text(310, 104, 80), notice, CAPTION);
            } else if self.screen.is_bookmark_list() && count > rows {
                canvas.text(
                    navigation.footer_text(310, 93, 50),
                    &format!("{} / {}", self.list_page + 1, count.div_ceil(rows)),
                    CAPTION.center(),
                );
            }
        }
        if let Some(popup) = &self.result_context {
            popup.draw(canvas);
        }
        if matches!(
            self.screen,
            Screen::ClearOnlineHistory | Screen::ClearDictionaryHistory
        ) {
            self.draw_clear_lookup_history(canvas);
        }
    }
}

impl Reader {
    fn touch_selection_enabled(&self) -> bool {
        matches!(self.screen, Screen::Book | Screen::Fixed)
            && self.logical_recovery.is_none()
            && self.end_dialog.is_none()
            && self.flow_popup.is_none()
            && self.link_popup.is_none()
            && self.saved_popup.is_none()
            && self.range_edit.is_none()
            && self.reading_panel.is_none()
            && self.top_group.is_none()
            && !self.reading_top
            && !self.reading_bottom
            && !self.touch_locked
            && !self.read_alarm.modal()
            && self.ocr.is_none()
    }

    fn touch_hold_input(&mut self, input: Input) -> Option<Change> {
        match input {
            Input::HoldStart(x, y) => Some(
                self.start_assigned_corner_hold(x, y)
                    .unwrap_or_else(|| self.start_touch_hold(x, y)),
            ),
            Input::HoldPan(x, y) => Some(self.pan_touch_hold(x, y)),
            Input::HoldRelease {
                start_x,
                start_y,
                held_ms,
                ..
            } => {
                let Some(hold) = self.touch_hold.take() else {
                    return Some(self.input(Input::LongTap(start_x, start_y)));
                };
                if let Some(base) = hold.base {
                    match base {
                        TouchHoldBase::Reflow(selection) => {
                            if self.selection.is_none() {
                                self.selection = Some(selection);
                            }
                        }
                        TouchHoldBase::Fixed(selection) => {
                            if self.pdf_selection.is_none() {
                                self.pdf_selection = Some(selection);
                            }
                        }
                    }
                    self.notice = None;
                    return Some(self.handle_text_long_press_with_force_ask(
                        held_ms >= self.controls.very_long_press_millis(),
                    ));
                }
                Some(self.unassigned_hold_release(start_x, start_y))
            }
            _ => None,
        }
    }

    fn start_touch_hold(&mut self, x: f32, y: f32) -> Change {
        let mut base = None;
        if self.touch_selection_enabled() {
            self.selection = None;
            self.pdf_selection = None;
            self.pending_selection_action = None;
            self.notice = None;
            match self.screen {
                Screen::Book => {
                    let at_illustration = self
                        .active
                        .as_ref()
                        .is_some_and(|active| active.reading.illustration_at(x, y).is_some());
                    if !at_illustration
                        && let Some(selection) = self
                            .active
                            .as_ref()
                            .and_then(|active| active.reading.word_at(x, y))
                    {
                        self.selection = Some(selection.clone());
                        self.start_cjk_selection();
                        base = Some(TouchHoldBase::Reflow(selection));
                    }
                }
                Screen::Fixed => {
                    if let Some(fixed) = &self.fixed
                        && fixed.reading.supports_text()
                        && let Ok(crate::fixed_session::WordAtTouch::Ready(Some(selection))) =
                            fixed.reading.select_word(x, y)
                    {
                        self.pdf_selection = Some(selection.clone());
                        self.pdf_result_regions.clear();
                        base = Some(TouchHoldBase::Fixed(selection));
                    }
                }
                _ => {}
            }
        }
        let corner_armed = self.touch_corner_direction(x, y).is_none();
        self.touch_hold = Some(TouchHold { base, corner_armed });
        if self
            .touch_hold
            .as_ref()
            .is_some_and(|hold| hold.base.is_some())
        {
            Change::Partial
        } else {
            Change::None
        }
    }

    fn touch_corner_direction(&self, x: f32, y: f32) -> Option<i16> {
        if !self.controls.selection_corner_scroll_enabled() {
            return None;
        }
        match self.screen {
            Screen::Book
                if !self
                    .active
                    .as_ref()
                    .is_some_and(|active| active.reading.config.scroll) =>
            {
                return None;
            }
            Screen::Fixed => {}
            _ => return None,
        }
        let (width, height) = self.reading_viewport().dimensions();
        let width = width as f32;
        let height = height as f32;
        let left = x <= width * 0.12;
        let right = x >= width * 0.88;
        let top = y <= height * 0.10;
        let bottom = y >= height * 0.90;
        let rtl = match self.screen {
            Screen::Book => self
                .active
                .as_ref()
                .is_some_and(|active| active.reading.probe.pages_right_to_left()),
            Screen::Fixed => self
                .fixed
                .as_ref()
                .is_some_and(|fixed| fixed.reading.pages_right_to_left()),
            _ => false,
        };
        match (rtl, left, right, top, bottom) {
            (false, true, _, true, _) | (true, _, true, true, _) => Some(-3),
            (false, _, true, _, true) | (true, true, _, _, true) => Some(3),
            _ => None,
        }
    }

    fn pan_touch_hold(&mut self, x: f32, y: f32) -> Change {
        if self
            .touch_hold
            .as_ref()
            .is_some_and(|hold| hold.base.is_some())
        {
            self.cjk_selection_lookup = None;
            self.pending_selection_action = None;
        }
        let Some(direction) = self.touch_corner_direction(x, y) else {
            if let Some(hold) = &mut self.touch_hold {
                hold.corner_armed = true;
            }
            return self.extend_touch_selection(x, y);
        };
        let (armed, base) = self.touch_hold.as_mut().map_or((false, None), |hold| {
            let armed = hold.corner_armed;
            hold.corner_armed = false;
            (armed, hold.base.clone())
        });
        if armed && base.is_some() {
            return self.turn_for_touch_corner(direction);
        }
        Change::None
    }

    fn extend_touch_selection(&mut self, x: f32, y: f32) -> Change {
        let base = self.touch_hold.as_ref().and_then(|hold| hold.base.clone());
        match base {
            Some(TouchHoldBase::Reflow(base)) => {
                let Some((active, word)) = self
                    .active
                    .as_ref()
                    .and_then(|active| active.reading.word_at(x, y).map(|word| (active, word)))
                else {
                    return Change::None;
                };
                if let Ok(selection) = active.reading.extend_saved_quote(&base, &word)
                    && self.selection.as_ref() != Some(&selection)
                {
                    self.selection = Some(selection);
                    return Change::Partial;
                }
            }
            Some(TouchHoldBase::Fixed(base)) => {
                let Some(fixed) = &self.fixed else {
                    return Change::None;
                };
                let mut selection = base;
                if fixed.reading.extend_selection(&mut selection, x, y)
                    && self
                        .pdf_selection
                        .as_ref()
                        .is_none_or(|current| current.text() != selection.text())
                {
                    self.pdf_selection = Some(selection);
                    return Change::Partial;
                }
            }
            None => {}
        }
        Change::None
    }

    fn turn_for_touch_corner(&mut self, direction: i16) -> Change {
        match self.screen {
            Screen::Book => {
                let Some((previous, change)) = self.active.as_mut().map(|active| {
                    let previous = (active.reading.chapter, active.reading.page);
                    (previous, active.reading.automatic_turn(direction))
                }) else {
                    return Change::None;
                };
                let change = self.finish_book_update(change, previous);
                if self.selection.is_none()
                    && let Some(TouchHold {
                        base: Some(TouchHoldBase::Reflow(selection)),
                        ..
                    }) = &self.touch_hold
                {
                    self.selection = Some(selection.clone());
                }
                change
            }
            Screen::Fixed => {
                let Some((previous, result)) = self.fixed.as_mut().map(|fixed| {
                    let previous = fixed.reading.page();
                    (previous, fixed.reading.automatic_turn(direction))
                }) else {
                    return Change::None;
                };
                let change = self.finish_fixed_update(result, previous);
                if self.pdf_selection.is_none()
                    && let Some(TouchHold {
                        base: Some(TouchHoldBase::Fixed(selection)),
                        ..
                    }) = &self.touch_hold
                {
                    self.pdf_selection = Some(selection.clone());
                }
                change
            }
            _ => Change::None,
        }
    }
}

impl App for Reader {
    fn draw(&self, canvas: &mut Canvas) {
        if self.draw_quickstart(canvas) {
            return;
        }
        self.draw_content(canvas);
        if self.screen == Screen::Skim {
            self.draw_skim(canvas);
        }
        self.draw_flow_popup(canvas);
        self.draw_archive_confirmation(canvas);
        self.draw_metadata_archive_confirmation(canvas);
        self.draw_remove_book_shortcut(canvas);
        self.draw_profile_confirmation(canvas);
        self.draw_opening_time(canvas);
        self.draw_profile_field_controls(canvas);
        self.draw_profile_values_button(canvas);
        self.draw_list_density_picker(canvas);
        self.draw_search_results_navigation(canvas);
        self.draw_top_header_default_prompt(canvas);
        self.draw_orientation_default_prompt(canvas);
        self.draw_top_header_about(canvas);
        self.draw_header_font_picker(canvas);
        self.read_alarm.draw(canvas, self.screen == Screen::Book);
        self.draw_vocabulary_dialog(canvas);
        self.draw_back_exit_confirmation(canvas);
        self.draw_history_timeline_start(canvas);
        self.draw_history_duration_limits(canvas);
        self.draw_dictionary_preset_picker(canvas);
        self.draw_dictionary_font_size(canvas);
        self.draw_alternative_contents_confirmation(canvas);
        self.draw_embedded_font_report(canvas);
        self.draw_logical_recovery(canvas);
        self.draw_opening_delay(canvas);
        self.draw_opening_ask(canvas);
        self.draw_closing_profile(canvas);
        self.draw_reading_notification(canvas);
        self.draw_annotation_exchange_confirmation(canvas);
        self.draw_menu_search_preview(canvas);
    }

    fn press_point(&self, input: Input) -> Option<(f32, f32)> {
        if let Some(point) = self.book_shortcut_removal_press(input) {
            return point;
        }
        if let Some(point) = self.metadata_archive_press(input) {
            return point;
        }
        if let Some(point) = self.menu_search_preview_press(input) {
            return point;
        }
        if let Some(point) = self.annotation_exchange_press(input) {
            return point;
        }
        if let Some(point) = self.closing_profile_press(input) {
            return point;
        }
        if let Some(point) = self.opening_modal_press(input) {
            return point;
        }
        if let Some(point) = self.embedded_font_report_press(input) {
            return point;
        }
        if let Some(point) = self.dictionary_font_size_press(input) {
            return point;
        }
        match input {
            Input::Tap(x, y)
                if !self.touch_locked
                    || self.screen == Screen::PdfPassword
                    || self
                        .reading_viewport()
                        .bottom_right(UNLOCK_TOUCH)
                        .contains(x, y)
                    || self.reading_viewport().done().contains(x, y) =>
            {
                Some((x, y))
            }
            _ => None,
        }
    }

    fn swipe(&mut self, input: Input, start_x: f32) -> Change {
        if let Some(change) = self.quickstart_input(input) {
            return change;
        }
        self.notifications.handle_input(input);
        self.gestures.multiswipes.clear_contact();
        self.finish_interrupted_scroll();
        if let Some(change) = self.embedded_font_report_input(input) {
            return change;
        }
        if let Some(change) = self.annotation_exchange_input(input) {
            return change;
        }
        if let Some(change) = self.closing_profile_input(input) {
            return change;
        }
        if let Some(change) = self.alternative_contents_confirmation(input) {
            return change;
        }
        if let Some(change) = self.dictionary_font_size_input(input) {
            return change;
        }
        if let Some(change) = self
            .opening_delay_input(input)
            .or_else(|| self.opening_ask_input(input, Instant::now()))
        {
            return change;
        }
        if let Some(change) = self.opening_time_input(input) {
            return change;
        }
        if let Some(change) = self.dictionary_preset_picker_input(input) {
            return change;
        }
        if self.screen == Screen::Fixed && self.fixed_search_fit_active() {
            return match self.cancel_directional_search() {
                Ok(()) => match self.swipe(input, start_x) {
                    Change::None => Change::Partial,
                    change => change,
                },
                Err(error) => self.report(error),
            };
        }
        if let Some(change) = self.logical_recovery_input(input) {
            return change;
        }
        if let Some(change) = self.back_exit_confirmation_input(input) {
            return change;
        }
        if let Some(change) = self.vocabulary_dialog_input(input) {
            return change;
        }
        if self.flow_popup.is_some() {
            return self.flow_popup_input(input);
        }
        if let Some(change) = self.archive_confirmation_input(input) {
            return change;
        }
        if let Some(change) = self.metadata_archive_confirmation_input(input) {
            return change;
        }
        if let Some(change) = self.book_shortcut_removal_input(input) {
            return change;
        }
        if self.read_alarm.modal() {
            return self.read_alarm_input(input);
        }
        if let Some(change) = self.top_header_default_input(input) {
            return change;
        }
        if let Some(change) = self.orientation_default_input(input) {
            return change;
        }
        if let Some(change) = self.top_header_about_input(input) {
            return change;
        }
        if let Some(change) = self.header_font_picker_input(input) {
            return change;
        }
        if let Some(change) = self.search_results_navigation_input(input) {
            return change;
        }
        if let Some(change) = self.style_editor_input(input) {
            return change;
        }
        if let Some(change) = self.reading_history_input(input) {
            return change;
        }
        if self.end_dialog.is_some() {
            return self.end_dialog_input(input);
        }
        if self.link_popup.is_some() {
            return self.input(input);
        }
        if self.range_edit.is_some() {
            return self.range_input(input);
        }
        if self.saved_popup_over_book() {
            return self.saved_popup_input(input);
        }
        if let Screen::BookmarkBrowser(page) = self.screen {
            return self
                .bookmark_browser_input(page, input)
                .unwrap_or_else(|| self.input(input));
        }
        if self.screen == Screen::PageOverview
            && let Some(change) = self.overview_swipe(input, start_x)
        {
            return change;
        }
        if self.screen == Screen::Skim {
            return self.skim_swipe(input, start_x);
        }
        if self.touch_locked {
            return self.input(input);
        }
        if let Some(change) = self.swipe_display_control(input, start_x) {
            return change;
        }
        self.assigned_gesture(input, Some(start_x))
            .or_else(|| self.reading_edge_swipe(input, start_x))
            .or_else(|| self.swipe_nearest_link(input, start_x))
            .or_else(|| self.swipe_latest_bookmark(input, start_x))
            .unwrap_or_else(|| self.input(input))
    }

    fn live_touch_pan(&self) -> bool {
        true
    }

    fn touch_pan(&mut self, pan: primitive_app::TouchPan) -> Change {
        if self.quickstart.is_some() {
            return Change::None;
        }
        self.notifications.dismiss();
        self.recognize_multiswipe(pan)
            .unwrap_or_else(|| self.continuous_touch_pan(pan))
    }

    fn preferred_rotation(&self) -> Option<primitive_app::Rotation> {
        use primitive_app::Rotation;

        match self.reading_orientation() {
            ReadingOrientation::FollowLauncher
            | ReadingOrientation::LandscapeLeft
            | ReadingOrientation::LandscapeRight => None,
            ReadingOrientation::Upright => Some(Rotation::Upright),
            ReadingOrientation::UpsideDown => Some(Rotation::UpsideDown),
        }
    }

    fn viewport_request(&self) -> Option<primitive_app::viewport::ViewportRequest> {
        use primitive_app::viewport::ViewportRequest;

        // Password entry and crop drafts belong to the currently displayed viewport.
        if matches!(self.screen, Screen::PdfPassword | Screen::FixedCrop)
            && let Some(viewport) = self.accepted_viewport
        {
            return Some(ViewportRequest::Fixed(viewport.turn));
        }
        if let Some(fallback) = self
            .pending_opening_profiles
            .as_ref()
            .and_then(|plan| plan.fallback)
        {
            return Some(fallback);
        }
        Some(self.requested_reading_orientation().viewport_policy())
    }

    fn viewport_needs_stage(&self, accepted: primitive_app::viewport::Viewport) -> bool {
        self.has_pending_orientation()
            || self.pending_opening_profiles.is_some()
            || self.reading_viewport().dimensions() != accepted.logical_size
    }

    fn stage_viewport(&mut self, candidate: primitive_app::viewport::Viewport) -> io::Result<()> {
        use crate::session::ReadingViewport;

        self.finish_interrupted_scroll();
        self.gestures.multiswipes.clear_contact();
        self.staged_viewport = Some((candidate, None));
        let target = match candidate.logical_size {
            (WIDTH, HEIGHT) => ReadingViewport::PORTRAIT,
            (HEIGHT, WIDTH) => ReadingViewport::LANDSCAPE,
            _ => {
                return Err(io::Error::other(
                    "Reader received unsupported viewport dimensions",
                ));
            }
        };
        let staged = self
            .prepare_reading_opening_viewport(target, candidate.turn)
            .map_err(io::Error::other)?;
        self.staged_viewport = Some((candidate, Some(staged)));
        Ok(())
    }

    fn draw_staged_viewport(&mut self, canvas: &mut Canvas) -> io::Result<()> {
        let (candidate, mut staged) = self
            .staged_viewport
            .take()
            .ok_or_else(|| io::Error::other("No prepared Reader viewport"))?;
        // The runtime presents this frame before it commits the preference.
        self.staged_frame_orientation = self.pending_orientation_for_turn(candidate.turn);
        let render = staged
            .as_mut()
            .ok_or_else(|| io::Error::other("No prepared Reader viewport"))
            .and_then(|stage| {
                self.draw_prepared_reading_viewport(stage, canvas)
                    .map_err(io::Error::other)
            });
        self.staged_frame_orientation = None;
        self.staged_viewport = Some((candidate, staged));
        render
    }

    fn finish_viewport(
        &mut self,
        candidate: primitive_app::viewport::Viewport,
        decision: primitive_app::viewport::ViewportDecision,
    ) {
        let Some((_, staged)) = self
            .staged_viewport
            .take_if(|(expected, _)| *expected == candidate)
        else {
            return;
        };
        if decision != primitive_app::viewport::ViewportDecision::Commit {
            if let Some(plan) = &mut self.pending_opening_profiles {
                plan.fallback = Some(self.accepted_viewport.map_or(
                    primitive_app::viewport::ViewportRequest::FollowLauncher,
                    |prior| primitive_app::viewport::ViewportRequest::Fixed(prior.turn),
                ));
            }
            self.reject_pending_orientation();
            return;
        }
        if let Some(staged) = staged {
            match self.commit_reading_viewport(staged) {
                Ok(()) => {
                    self.accepted_viewport = Some(candidate);
                    self.accept_pending_orientation(candidate.turn);
                    if self.embedded_font_report.is_some() {
                        self.open_embedded_font_report();
                    }
                }
                Err(error) => {
                    eprintln!("reader viewport_commit_failed={error}");
                    self.reject_pending_orientation();
                }
            }
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
        if let Some(change) = self.quickstart_input(input) {
            return change;
        }
        self.gestures.multiswipes.clear_contact();
        if let Some(change) = self.menu_search_input(input) {
            return change;
        }
        self.notifications.handle_input(input);
        self.finish_interrupted_scroll();
        if self.gestures.consume_assigned_corner_hold(input) {
            return Change::None;
        }
        if input == Input::Sleep {
            self.eager_typography.cancel();
            self.embedded_font_report = None;
        }
        if let Some(change) = self.embedded_font_report_input(input) {
            return change;
        }
        if let Some(change) = self.annotation_exchange_input(input) {
            return change;
        }
        if let Some(change) = self.closing_profile_input(input) {
            return change;
        }
        if let Some(change) = self.alternative_contents_confirmation(input) {
            return change;
        }
        if let Some(change) = self.dictionary_font_size_input(input) {
            return change;
        }
        if let Some(change) = self
            .opening_delay_input(input)
            .or_else(|| self.opening_ask_input(input, Instant::now()))
        {
            return change;
        }
        if let Some(change) = self.opening_time_input(input) {
            return change;
        }
        if matches!(input, Input::HoldStart(..) | Input::LongTap(..)) {
            self.settle_pending_opening();
        }
        if let Some(change) = self.back_exit_confirmation_input(input) {
            return change;
        }
        if input == Input::Sleep {
            self.screen_page_worker.cancel();
            self.cancel_screen_page_jump();
        }
        if matches!(
            input,
            Input::HoldStart(..) | Input::HoldPan(..) | Input::HoldRelease { .. }
        ) {
            if let Some(change) = self.fixed_search_fit_input(input) {
                return change;
            }
            return self.touch_hold_input(input).unwrap_or(Change::None);
        }
        if let Some(change) = self.logical_recovery_input(input) {
            return change;
        }
        // Any further user action owns the selection; late automatic results
        // must never replace a manually extended range or an open popup.
        self.cjk_selection_lookup = None;
        if let Some(change) = self.vocabulary_dialog_input(input) {
            return change;
        }
        if let Some(change) = self.document_defaults_input(input) {
            return change;
        }
        if input == Input::Sleep {
            self.touch_hold = None;
            self.spelling_lookup = None;
            self.cancel_library_export();
        }
        if input == Input::Sleep
            && let Some(panel) = &mut self.online
        {
            panel.cancel_for_sleep();
        }
        if matches!(input, Input::Sleep) && self.ocr.take().is_some() {
            self.notice = None;
        }
        if matches!(input, Input::Sleep | Input::Wake) {
            self.stats_awake = input == Input::Wake;
            if input == Input::Wake {
                self.metadata_checkpoint = metadata_save::Checkpoint::new(Instant::now());
                self.reading_history.begin_session(crate::mark_time::now());
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
            let progress_changed = if input == Input::Wake {
                self.reset_footer_initial_position();
                self.screen == Screen::ReadingHistory(reading_history::Page::Progress)
                    && self.refresh_reading_progress()
            } else {
                false
            };
            self.update_footer_clock(Instant::now());
            return if input == Input::Wake {
                match self.poll_read_alarm(Instant::now(), true) {
                    Change::None if progress_changed => Change::Partial,
                    change => change,
                }
            } else {
                Change::None
            };
        }
        if let Some(change) = self.dictionary_preset_picker_input(input) {
            return change;
        }
        if self.read_alarm.modal() {
            return self.read_alarm_input(input);
        }
        if let Some(change) = self.top_header_default_input(input) {
            return change;
        }
        if let Some(change) = self.orientation_default_input(input) {
            return change;
        }
        if let Some(change) = self.top_header_about_input(input) {
            return change;
        }
        if let Some(change) = self.header_font_picker_input(input) {
            return change;
        }
        if let Some(change) = self.search_results_navigation_input(input) {
            return change;
        }
        if let Some(change) = self.style_editor_input(input) {
            return change;
        }
        if let Some(change) = self.reading_history_input(input) {
            return change;
        }
        self.sync_auto_turn(Instant::now());
        if let Some(change) = self.profile_values_button_input(input) {
            return change;
        }
        if let Some(change) = self.profile_field_controls_input(input) {
            return change;
        }
        if let Some(change) = self.profile_confirmation_input(input) {
            return change;
        }
        if let Some(change) = self.archive_confirmation_input(input) {
            return change;
        }
        if let Some(change) = self.metadata_archive_confirmation_input(input) {
            return change;
        }
        if let Some(change) = self.book_shortcut_removal_input(input) {
            return change;
        }
        if self.flow_popup.is_some() {
            return self.flow_popup_input(input);
        }
        self.auto_turn.activity(Instant::now());
        if self.end_dialog.is_some() {
            return self.end_dialog_input(input);
        }
        if self.link_popup.is_some() {
            return self.link_address_input(input);
        }
        if self.can_paste_text()
            && matches!(input, Input::Tap(x, y) if self.editor_paste_rect().contains(x,y))
        {
            return self.paste_text();
        }
        if self.screen == Screen::PdfPassword {
            return self.handle_pdf_password(input);
        }
        if let Screen::PageTurnSizes(sizes) = self.screen {
            return self.page_turn_sizes_input(input, sizes);
        }
        let input = if self.touch_locked {
            match input {
                Input::Tap(x, y)
                    if self
                        .reading_viewport()
                        .bottom_right(UNLOCK_TOUCH)
                        .contains(x, y) =>
                {
                    self.touch_locked = false;
                    self.notice = None;
                    return Change::Partial;
                }
                Input::Tap(x, y) if self.reading_viewport().done().contains(x, y) => {
                    return self.exit();
                }
                Input::PageBack | Input::PageForward => input,
                _ => match self.locked_swipe_turn(input) {
                    Some(turn) => turn,
                    None => return Change::None,
                },
            }
        } else {
            input
        };
        if self.range_edit.is_some() {
            return self.range_input(input);
        }
        if self.saved_popup_over_book() {
            return self.saved_popup_input(input);
        }
        if let Screen::BookmarkBrowser(page) = self.screen
            && let Some(change) = self.bookmark_browser_input(page, input)
        {
            return change;
        }
        if self.screen == Screen::LibraryExport(library_export::Page::Colors) {
            return self.export_colors_input(input);
        }
        if let Some(change) = self.fixed_search_fit_input(input) {
            return change;
        }
        if matches!(self.screen, Screen::Book | Screen::Fixed)
            && let Some(change) = self.overlay_footer_input(input)
        {
            return change;
        }
        if let Input::Tap(x, y) = input
            && let Some(change) = self.tap_saved_mark(x, y)
        {
            return change;
        }
        if let Some(change) = self.assigned_gesture(input, None) {
            return change;
        }
        if let Some(change) = self.swipe_back_input(input) {
            return change;
        }
        if matches!(self.screen, Screen::Book | Screen::Fixed)
            && self.selection.is_none()
            && self.pdf_selection.is_none()
            && matches!(input, Input::Tap(x, y) if x >= self.reading_viewport().dimensions().0 as f32 - 58.0 && y < 58.0)
        {
            return match self.toggle_bookmark() {
                Ok(()) => self.applied_change(),
                Err(error) => self.report(error),
            };
        }
        if self.screen == Screen::ReadFixedText {
            if matches!(input, Input::Tap(x, y) if self.reading_viewport().done().contains(x, y)) {
                self.show(Screen::Fixed);
                return Change::Partial;
            }
            let turn = match input {
                Input::Tap(x, y) => self.tap_turn(x, y, self.reading_viewport().page_height()),
                Input::Swipe { dx, dy, .. } => self.swipe_turn(dx, dy),
                other => Some(other),
            };
            if let (Some(flow), Some(fixed)) = (&mut self.text_flow, &mut self.fixed) {
                let previous = flow.preferences();
                let result = flow.input_with_turn(input, turn, &fixed.reading);
                let current = flow.preferences();
                if result.is_ok() && current != previous {
                    fixed.reading.set_fixed_reflow_spacing(current.spacing);
                    fixed
                        .reading
                        .apply_page_overlap(current.overlap_enabled, current.marker);
                    self.record_position();
                }
                return self.finish_text_flow_update(result);
            }
        }
        if self.screen == Screen::Book {
            return self.handle_book(input);
        }
        if self.screen == Screen::Illustration {
            return self.illustration_input(input);
        }
        if matches!(self.screen, Screen::BookTools | Screen::FixedTools) {
            return self.reading_tools_input(input);
        }
        if self.screen == Screen::Hyphenate {
            return self.hyphenation_input(input);
        }
        if self.screen == Screen::CustomTitle {
            return self.chapter_editor_input(input);
        }
        if matches!(
            self.screen,
            Screen::ClearOnlineHistory | Screen::ClearDictionaryHistory
        ) {
            return self.clear_lookup_history_input(input);
        }
        if self.screen == Screen::Footnote {
            return self.footnote_input(input);
        }
        if self.screen == Screen::SelectionHtml {
            return self.selection_html_input(input);
        }
        if self.screen == Screen::OnlineLookup {
            return self.online_input(input);
        }
        if self.screen == Screen::OnlineEdit {
            return self.online_editor_input(input);
        }
        if let Screen::ExportSettings(origin, export_settings::Page::Template(scope)) = self.screen
        {
            return self.export_template_input(origin, scope, input);
        }
        if self.screen == Screen::Definition {
            return self.definition_input(input);
        }
        if self.screen == Screen::DictionaryButtons(dictionary_buttons::Page::Preview) {
            return self.dictionary_button_preview_input(input);
        }
        if self.screen == Screen::VocabularyContext {
            return self.vocabulary_context_input(input);
        }
        if matches!(
            self.screen,
            Screen::GoToPage
                | Screen::NumberingValue(_)
                | Screen::FooterCustomText
                | Screen::FootnoteFontSize(_)
        ) {
            return self.handle_page_number(input);
        }
        if self.screen == Screen::BookMap {
            if let Some(change) = self.book_map_tap_option_input(input) {
                return change;
            }
            if let Some(change) = self.ribbon_book_map_back(input) {
                return change;
            }
            if self.fixed.is_some() {
                return self.fixed_map_input(input);
            }
        }
        if self.screen == Screen::PageOverview {
            return self.overview_input(input);
        }
        if self.screen == Screen::Skim {
            return self.skim_input(input);
        }
        if matches!(
            self.screen,
            Screen::CollectionName
                | Screen::BookIdentityEdit(_)
                | Screen::ProfileName(_)
                | Screen::FooterPresetName(_)
                | Screen::DictionaryPresets(dictionary_presets::Page::Name(_))
        ) {
            return self.name_editor_input(input);
        }
        if self.screen == Screen::ContentsSearch {
            return self.contents_search_input(input);
        }
        if self.screen.is_contents() {
            return self.contents_input(input);
        }
        if self.flexible_marks.is_some() {
            return self.flexible_mark_input(input);
        }
        if let Some(change) = self.list_density_input(input) {
            return change;
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
                | Screen::MenuSearch
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
            if let Input::Swipe { dx, dy, .. } = input
                && dy.abs() > dx.abs()
            {
                popup.scroll(dy);
                return Change::Partial;
            }
            if matches!(input, Input::Tap(..) | Input::PageBack | Input::PageForward) {
                self.result_context = None;
                return Change::Partial;
            }
            return Change::None;
        }
        if let Some(change) = self.flexible_result_input(input) {
            return change;
        }
        let (count, mut items) = self.list_window();
        let geometry = self.list_geometry();
        let broad = self.has_list_shortcuts(count);
        let navigation = self
            .screen
            .uses_navigation_viewport()
            .then(|| self.navigation_layout());
        let back = navigation.map_or_else(
            || primitive_ui::back_button(WIDTH as i32),
            |layout| layout.back(),
        );
        let shortcuts = navigation.unwrap_or(navigation_layout::NavigationLayout::PORTRAIT);
        let (prev, next) = if broad {
            (
                shortcuts.edge_footer(PREV_MATCH_PAGE),
                shortcuts.edge_footer(NEXT_MATCH_PAGE),
            )
        } else if let Some(layout) = navigation {
            (layout.previous(), layout.next())
        } else {
            (PREV, NEXT)
        };
        let last = count.saturating_sub(1) / geometry.rows;
        if let Some(page) =
            navigation_layout::pager_destination(input, prev, next, self.list_page, last)
        {
            self.navigate_list_page(page);
            return Change::Partial;
        }
        let direction = match input {
            Input::Tap(x, y)
                if broad
                    && shortcuts.edge_footer(FIRST_MATCH_PAGE).contains(x, y)
                    && self.list_page > 0 =>
            {
                self.navigate_list_page(0);
                return Change::Partial;
            }
            Input::Tap(x, y)
                if broad
                    && shortcuts.edge_footer(LAST_MATCH_PAGE).contains(x, y)
                    && self.list_page < last =>
            {
                self.navigate_list_page(last);
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
            Input::Tap(x, y) if back.contains(x, y) => {
                match self.screen {
                    Screen::LibraryExport(page) => self.back_from_library_export(page),
                    Screen::ExportSettings(origin, page) => {
                        self.back_from_export_settings(origin, page)
                    }
                    Screen::OrganizeLibrary | Screen::FolderOrder => self.show(Screen::Library),
                    Screen::BookArchive => self.show(self.surface_screen()),
                    Screen::ArchiveFolder => self.show(Screen::BookArchive),
                    Screen::MetadataArchive(page) => self.back_from_metadata_archive(page),
                    Screen::BookShortcuts(page) => self.back_from_book_shortcuts(page),
                    Screen::LiveMetadata(page) => self.back_from_live_metadata(page),
                    Screen::BookList if matches!(self.book_list, Some(BookList::Author(_))) => {
                        self.book_list = Some(BookList::Authors);
                        self.show(Screen::BookList);
                    }
                    Screen::BookList => self.show(Screen::OrganizeLibrary),
                    Screen::RemoveCollection => self.show(Screen::BookList),
                    Screen::GestureBinding(gesture) => self.return_to_gesture_list(gesture),
                    Screen::MultiswipeBinding(shape) => self.return_to_multiswipe_list(shape),
                    Screen::RemoveMultiswipe(shape) => self.show(Screen::MultiswipeBinding(shape)),
                    Screen::RecordMultiswipe => self.show(Screen::CustomMultiswipes),
                    Screen::Multiswipes | Screen::CustomMultiswipes | Screen::GestureOverview => {
                        self.show(Screen::Gestures);
                    }
                    Screen::Scrolling => self.show(Screen::Gestures),
                    Screen::ReadingNotifications => self.show(Screen::ReadingControls),
                    Screen::PastNotifications => {
                        self.show(Screen::ReadingNotifications);
                        self.restore_reading_list_ordinal(reading_notifications::HISTORY_ROW);
                    }
                    Screen::Gestures | Screen::LinkSettings | Screen::MenuActivationSettings => {
                        self.show(Screen::ReadingControls)
                    }
                    Screen::BackOptions(_) => self.show(Screen::NavigationSettings),
                    Screen::PageTurnZones | Screen::PageTurnDefaults => {
                        self.show(Screen::PageTurns)
                    }
                    Screen::PageTurns => self.show(Screen::ReadingControls),
                    Screen::ReadingFonts => self.close_font_chooser(),
                    Screen::DictionaryOptions(i) => {
                        self.show(Screen::Dictionaries);
                        let row = self.source_rows().iter().position(|item| {
                            matches!(item.action, Action::DictionarySource(SourceAction::Open(index)) if index == i)
                        });
                        if let Some(row) = row {
                            self.restore_reading_list_ordinal(row);
                        }
                    }
                    Screen::Dictionaries => self.show(self.dictionary_origin),
                    Screen::DictionaryButtons(page) => self.dictionary_button_editor_back(page),
                    Screen::DictionaryPresets(page) => self.back_from_dictionary_presets(page),
                    Screen::FooterSettings
                    | Screen::BookmarkListSettings
                    | Screen::NavigationSettings
                    | Screen::EndActionSettings
                    | Screen::MetadataSaveSettings
                    | Screen::SelectionPosition
                    | Screen::LongPressSettings => self.show(Screen::ReadingControls),
                    Screen::FooterFontFaces => self.show(Screen::FooterSettings),
                    Screen::FooterItemWidths => self.show(Screen::FooterSettings),
                    Screen::HeaderBatterySettings => self.show(Screen::FooterSettings),
                    Screen::FooterPresets => self.show(Screen::FooterSettings),
                    Screen::FooterPreset(_) => self.show(Screen::FooterPresets),
                    Screen::FooterPresetDelete(index) => self.show(Screen::FooterPreset(index)),
                    Screen::AnnotationListSettings => self.show(Screen::BookmarkListSettings),
                    Screen::VeryLongPressSettings => self.show(Screen::LongPressSettings),
                    Screen::HighlightPromptSettings => self.show(Screen::LongPressSettings),
                    Screen::DocumentDefaults => self.show(Screen::ReadingControls),
                    Screen::HyphenationLimitValue(draft, _) => {
                        self.show(Screen::HyphenationLimits(draft))
                    }
                    Screen::HyphenationLimits(_) | Screen::HyphenationDefaults => {
                        self.show(Screen::HyphenationOptions)
                    }
                    Screen::TypographyLanguages(_) | Screen::HyphenationOptions => {
                        self.show(Screen::TypographyRules)
                    }
                    Screen::PublisherRenderDefaults => {
                        self.open_reading_edge(
                            false,
                            self.reading_viewport().dimensions().0 as f32 / 2.0,
                        );
                        self.reading_panel = Some(BarPanel::TextLayout);
                    }
                    Screen::TypographyRules => {
                        self.open_reading_edge(true, (WIDTH / 2) as f32);
                        self.top_group = Some(TopGroup::Appearance);
                    }
                    Screen::FontRendering(eager_typography::Scope::Book) => {
                        self.show(Screen::ReadingControls)
                    }
                    Screen::FontRendering(eager_typography::Scope::Default) => {
                        self.show(Screen::FontRendering(eager_typography::Scope::Book))
                    }
                    Screen::StyleTweaks => self.show(self.tools_screen()),
                    Screen::InPageFootnoteSize(style_tweaks::NoteSizeScope::Default) => self.show(
                        Screen::InPageFootnoteSize(style_tweaks::NoteSizeScope::Book),
                    ),
                    Screen::InPageFootnoteSize(style_tweaks::NoteSizeScope::Book) => {
                        self.show(Screen::StyleTweaks)
                    }
                    Screen::ReadingGuides => {
                        self.open_reading_edge(true, WIDTH as f32 / 2.0);
                    }
                    Screen::ReadingGuidesAbout => self.show(Screen::ReadingGuides),
                    Screen::StylePreset => self.show(self.style_preset_parent()),
                    Screen::FootnoteTweaks
                    | Screen::ParagraphSpacing
                    | Screen::LinkStyles
                    | Screen::ImageStyles
                    | Screen::TableStyles
                    | Screen::PseudoElementStyles
                    | Screen::ListItemStyles
                    | Screen::StyleFiles
                    | Screen::StyleClear => self.show(Screen::StyleTweaks),
                    Screen::StyleReview => self.show(if self.style_view.editor.is_some() {
                        Screen::StyleEditor
                    } else {
                        Screen::StyleTweaks
                    }),
                    Screen::StyleEditor => self.show(Screen::StyleTweaks),
                    Screen::ReadingProfiles => self.show(if self.fixed.is_some() {
                        Screen::Fixed
                    } else if self.active.is_some() {
                        Screen::Typography
                    } else {
                        Screen::Library
                    }),
                    Screen::ReadingProfile(_) => self.show(Screen::ReadingProfiles),
                    Screen::ReadingProfileValues(index) => self.show(Screen::ReadingProfile(index)),
                    Screen::ReadingProfileMargins(index)
                    | Screen::ReadingProfileFontRendering(index)
                    | Screen::ReadingProfileBookRendering(index) => {
                        self.show(Screen::ReadingProfileValues(index))
                    }
                    Screen::ReadingProfileChoice(index, field) => {
                        self.show(profile_panel::rendering_choices::parent(index, field));
                    }
                    Screen::ReadingProfileOpening(index, page) => {
                        self.show(page.back(profile_panel::opening_rules::Event::Opening, index));
                    }
                    Screen::ReadingProfileClosing(index, page) => {
                        self.show(page.back(profile_panel::opening_rules::Event::Closing, index));
                    }
                    Screen::BookDetails => {
                        if self
                            .book_details
                            .as_ref()
                            .is_some_and(|book| book.close_document)
                        {
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
                    Screen::ReadingControls => self.show(if self.fixed.is_some() {
                        Screen::FixedTools
                    } else if self.active.is_some() {
                        Screen::BookTools
                    } else {
                        Screen::Library
                    }),
                    Screen::AnnotationExchange => self.show(Screen::BookmarkListSettings),
                    Screen::AnnotationExchangePreview => self.show(Screen::AnnotationExchange),
                    Screen::ClearOcr => self.show(Screen::ReadingData),
                    Screen::RestoreBackup => {
                        self.backup_import = None;
                        self.show(Screen::ReadingData);
                    }
                    Screen::FixedBookmarks | Screen::OcrLanguage => self.show(Screen::FixedTools),
                    Screen::ContentsSettings => {
                        self.open_reading_edge(
                            true,
                            self.reading_viewport().dimensions().0 as f32 * 0.5,
                        );
                    }
                    Screen::Highlights | Screen::LocationHistory if self.fixed.is_some() => {
                        self.show(Screen::FixedTools)
                    }
                    Screen::Matches if self.matches_return_to_reading => {
                        self.cancel_search();
                        self.show(self.surface_screen());
                    }
                    Screen::Matches => self.show(Screen::Find),
                    Screen::MenuSearchResults => self.edit_menu_search(),
                    Screen::SearchSettings => {
                        self.open_reading_edge(true, WIDTH as f32 / 2.0);
                        self.top_group = Some(TopGroup::Search);
                    }
                    Screen::CustomContents => self.show(self.contents_screen()),
                    Screen::CustomEdits | Screen::CustomBookmarks => {
                        self.show(Screen::CustomContents)
                    }
                    Screen::ChapterNavigation => {
                        self.open_reading_edge(true, 100.0);
                    }
                    Screen::NumberingDefaults => self.show(Screen::PageNumbering),
                    Screen::AutoTurnDistance => self.show(Screen::AutoTurn),
                    Screen::AutoTurnPan => self.show(Screen::AutoTurn),
                    Screen::PageNumbering | Screen::PublisherOffer => self.show(Screen::Book),
                    Screen::GeneratedContents => self.show(Screen::Contents),
                    Screen::BookmarkMatches => self.show(Screen::BookmarkFind),
                    Screen::Annotation | Screen::ImportAnnotations => self.show(Screen::Highlights),
                    Screen::ImportPosition => self.show(Screen::ReadingControls),
                    Screen::ImportPdfAnnotations => self.show(Screen::Highlights),
                    Screen::ExportAnnotations => self.show(Screen::Highlights),
                    Screen::RemoveHighlight => self.show(Screen::Annotation),
                    Screen::VocabularyWord | Screen::VocabularyOptions => {
                        self.show(Screen::Vocabulary)
                    }
                    Screen::VocabularyBooks => self.show(Screen::VocabularyOptions),
                    _ if self.fixed.is_some() => self.show(Screen::FixedTools),
                    _ => self.show(Screen::BookTools),
                }
                return Change::Partial;
            }
            Input::LongTap(x, y)
                if self.screen == Screen::BookShortcuts(book_shortcuts::Page::List) =>
            {
                let Some(index) = geometry.item_at(self.list_page, count, x, y) else {
                    return Change::None;
                };
                if let Action::BookShortcut(book_shortcuts::Command::Launch(id)) =
                    items[index % geometry.rows].action
                {
                    self.confirm_remove_book_shortcut(id);
                    return Change::Partial;
                }
                return Change::None;
            }
            Input::LongTap(x, y) if self.screen == Screen::Matches => {
                let index = geometry
                    .item_at(self.list_page, count, x, y)
                    .and_then(|ordinal| self.search_result_order().provider_index(ordinal));
                let Some(index) = index else {
                    return Change::None;
                };
                return match self.preview_result(index) {
                    Ok(()) => Change::Partial,
                    Err(error) => self.report(error),
                };
            }
            Input::LongTap(x, y) if self.screen.is_bookmark_list() => {
                let Some(index) = geometry.item_at(self.list_page, count, x, y) else {
                    return Change::None;
                };
                let action = match items.swap_remove(index % geometry.rows).action {
                    Action::Highlight(index) | Action::PdfHighlight(index) => {
                        Action::ManageHighlight(index)
                    }
                    _ => return Change::None,
                };
                return match self.activate(action) {
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
                if let Some(index) = geometry.item_at(self.list_page, count, x, y) {
                    let action = items.swap_remove(index % geometry.rows).action;
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
            .filter(|page| page * geometry.rows < count)
        else {
            return Change::None;
        };
        self.navigate_list_page(page);
        Change::Partial
    }

    fn deadline(&self) -> Option<Instant> {
        let page = self
            .active
            .as_ref()
            .filter(|_| matches!(self.screen, Screen::Book | Screen::Skim))
            .and_then(|active| active.reading.deadline());
        let fixed_page = self
            .fixed
            .as_ref()
            .filter(|_| matches!(self.screen, Screen::Fixed | Screen::Skim))
            .and_then(|fixed| fixed.reading.deadline());
        let screen_pages = self
            .screen_page_count_intent()
            .and_then(|intent| self.active.as_ref().map(|active| (&active.reading, intent)));
        page.into_iter()
            .chain(fixed_page)
            .chain(self.screen_page_worker.deadline(screen_pages))
            .chain(self.notifications.deadline())
            .chain(self.eager_font_deadline())
            .chain(self.embedded_font_report_deadline())
            .chain(self.export_deadline())
            .chain(self.bookmark_metadata_deadline())
            .chain(
                self.nearby_marks
                    .as_ref()
                    .and_then(|import| import.deadline),
            )
            .chain(self.save_at)
            .chain(self.metadata_checkpoint.deadline())
            .chain(self.opening_execution_deadline())
            .chain(
                self.stats_awake
                    .then(|| self.read_alarm.deadline())
                    .flatten(),
            )
            .chain(self.search_at)
            .chain(self.nearby.deadline())
            .chain(self.spelling_lookup.as_ref().map(|job| job.deadline))
            .chain(self.cjk_selection_lookup.as_ref().map(|job| job.deadline))
            .chain(self.outline.as_ref().and_then(|outline| outline.deadline))
            .chain(self.alternative_contents_deadline())
            .chain(self.numbering_scan.as_ref().map(|scan| scan.deadline))
            .chain(
                self.annotation_import
                    .as_ref()
                    .and_then(|import| import.deadline),
            )
            .chain(self.online.as_ref().and_then(online_panel::Panel::deadline))
            .chain(self.ocr.as_ref().map(|job| job.deadline))
            .chain(self.text_flow.as_ref().and_then(TextFlow::deadline))
            .chain(self.stats.deadline())
            .chain(self.reading_history.deadline())
            .chain(self.footer_refresh_at)
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
        if self.notifications.tick(now) {
            // The notification deadline has been consumed. Let any due
            // reading event run at this same instant before repainting.
            return match self.tick(now) {
                Change::None => Change::Partial,
                change => change,
            };
        }
        if let Some(change) = self.poll_embedded_font_report(now) {
            return change;
        }
        if let Some(change) = self.eager_font_tick(now) {
            return change;
        }
        if let Some(change) = self.opening_execution_tick(now) {
            return change;
        }
        let intent = self.screen_page_count_intent();
        let screen_pages =
            intent.and_then(|intent| self.active.as_ref().map(|active| (&active.reading, intent)));
        let page_count_changed = self.screen_page_worker.tick(now, screen_pages);
        if let Some(change) = self.poll_screen_page_jump() {
            return change;
        }
        if page_count_changed && intent == Some(ScreenPageIntent::Visible) {
            return Change::Partial;
        }
        if self
            .footer_refresh_at
            .is_some_and(|deadline| deadline <= now)
        {
            let clock_visible = self.clock_refresh_visible();
            let battery_changed = self.refresh_status_battery_changed();
            self.schedule_footer_clock(now);
            return if clock_visible || battery_changed {
                Change::Partial
            } else {
                Change::None
            };
        }
        if self.stats_awake {
            let alarm = self.poll_read_alarm(now, false);
            if alarm != Change::None {
                return alarm;
            }
        }
        if let Some(change) = self.poll_cjk_selection(now) {
            return change;
        }
        if let Some(change) = self.poll_spelling_lookup(now) {
            return change;
        }
        if let Some(change) = self.poll_online(now) {
            return change;
        }
        if self.save_at.is_some_and(|at| at <= now)
            || self
                .metadata_checkpoint
                .deadline()
                .is_some_and(|at| at <= now)
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
            && let Err(error) = self
                .reading_history
                .save(now, crate::mark_time::now(), false)
            && error.kind() != io::ErrorKind::WouldBlock
        {
            return self.report(error);
        }
        if self.stats.deadline().is_some_and(|at| at <= now)
            && let Err(error) = self
                .stats
                .checkpoint(&self.stats_path, now, false)
                .and_then(|_| self.pace.checkpoint(now))
            && error.kind() != io::ErrorKind::WouldBlock
        {
            return self.report(error);
        }
        if self
            .export_deadline()
            .is_some_and(|deadline| deadline <= now)
        {
            return match self.step_library_export(now) {
                Ok(change) => change,
                Err(error) => {
                    self.show(Screen::LibraryExport(library_export::Page::Options));
                    self.report(error)
                }
            };
        }
        if self
            .bookmark_metadata_deadline()
            .is_some_and(|deadline| deadline <= now)
        {
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
        if self
            .nearby_marks
            .as_ref()
            .and_then(|import| import.deadline)
            .is_some_and(|at| at <= now)
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
        if let Some(change) = self.poll_alternative_contents(now) {
            return change;
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
        if self
            .numbering_scan
            .as_ref()
            .is_some_and(|scan| scan.deadline <= now)
        {
            return match self.step_character_pages(now) {
                Ok(change) => change,
                Err(error) => {
                    self.numbering_scan = None;
                    self.report(error)
                }
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
        if self.screen == Screen::Skim {
            return self.skim_tick(now);
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
        self.finish_interrupted_scroll();
        self.embedded_font_report = None;
        self.eager_typography.cancel();
        self.screen_page_worker.cancel();
        self.save_library()?;
        let now = Instant::now();
        self.stats.checkpoint(&self.stats_path, now, true)?;
        self.reading_history
            .save(now, crate::mark_time::now(), true)?;
        self.pace.checkpoint(now)?;
        if let Err(error) = self.save_bookmark_metadata() {
            eprintln!("reader metadata_cache_save_failed={error}");
        }
        Ok(())
    }

    fn storage_returned(&mut self) -> io::Result<()> {
        self.gestures.multiswipes.clear_contact();
        self.finish_interrupted_scroll();
        self.embedded_font_report = None;
        self.gestures.clear_assigned_corner_hold();
        if let Some(book) = &mut self.active {
            book.embedded_font_inventory = None;
        }
        self.eager_typography.cancel();
        self.alternative_contents = Default::default();
        self.sync_alternative_contents();
        if self.screen.is_contents() {
            self.prepare_contents(true);
            self.list_page = self.current_contents_row.unwrap_or_default() / self.contents_rows();
        }
        self.close_dictionary_font_size();
        if matches!(self.screen, Screen::DictionaryButtons(_)) {
            self.show(Screen::Dictionaries);
        }
        self.closing_profile = None;
        if self.closing_before_switch.is_some() && self.screen == Screen::PdfPassword {
            self.handle_pdf_password(Input::PageBack);
        }
        self.closing_before_switch = None;
        self.opening_execution = None;
        self.install_opening_profile_plan(None);
        self.screen_page_worker.cancel();
        self.cancel_screen_page_jump();
        self.document_defaults.reload();

        self.cjk_selection_lookup = None;
        self.spelling_lookup = None;
        self.cancel_ocr_selection();
        self.pages_since_refresh = 0;
        let now = Instant::now();
        self.auto_turn.activity(now);
        self.sync_auto_turn(now);
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

fn definition_bounds(
    height: i32,
    selection: Option<Rect>,
    viewport: crate::session::ReadingViewport,
) -> Rect {
    const TOP: i32 = 64;
    let (screen_width, screen_height) = viewport.dimensions();
    let above_selection = selection.is_some_and(|selected| {
        selected.y >= viewport.page_height() as i32 / 2 && TOP + height + 24 <= selected.y
    });
    let y = if above_selection {
        TOP
    } else {
        screen_height as i32 - 82 - height
    };
    definition_column(screen_width, y, height)
}

fn definition_column(screen_width: u32, y: i32, height: i32) -> Rect {
    let width = WIDTH as i32 - 128;
    Rect::new((screen_width as i32 - width) / 2, y, width, height)
}

fn definition_actions(popup: Rect) -> [Rect; 3] {
    let y = popup.bottom() - 82;
    [
        Rect::new(popup.x + 30, y, 214, 66),
        Rect::new(popup.x + popup.w / 2 - 105, y, 210, 66),
        Rect::new(popup.right() - 244, y, 214, 66),
    ]
}

fn same_pdf_highlight(left: &PdfHighlight, right: &PdfHighlight) -> bool {
    left.parts().any(|(page, bounds, _)| {
        page == right.page
            && bounds.len() == right.bounds.len()
            && bounds.iter().zip(&right.bounds).all(|(a, b)| {
                [a.x0 - b.x0, a.y0 - b.y0, a.x1 - b.x1, a.y1 - b.y1]
                    .into_iter()
                    .all(|delta| delta.abs() <= 0.000_01)
            })
    })
}

fn column_view(original: FixedPreferences, bounds: PdfTextRect) -> FixedPreferences {
    FixedPreferences {
        width_fit: true,
        crop: Some(bounds),
        scan_contrast: original.scan_contrast,
        scan_gamma_tenths: original.scan_gamma_tenths,
        fixed_reflow_spacing: original.fixed_reflow_spacing,
        dither: original.dither,
        straighten_limit: original.straighten_limit,
        force_ocr: original.force_ocr,
        scan_white_level: original.scan_white_level,
        continuous_gap: original.continuous_gap,
        ..FixedPreferences::default()
    }
}

#[cfg(test)]
fn list_rect(rows: usize) -> Rect {
    list_density::Geometry::STANDARD.bounds(rows)
}

#[cfg(test)]
fn overview_tile(index: usize) -> Rect {
    page_overview::OverviewLayout::new(navigation_layout::NavigationLayout::PORTRAIT).tile(index)
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
    list_density::Geometry::STANDARD.item_at(page, count, x, y)
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

fn draw_locked_footer(
    canvas: &mut Canvas,
    viewport: crate::session::ReadingViewport,
    notice: Option<&str>,
) {
    let (width, height) = viewport.dimensions();
    let top = viewport.page_height() as i32;
    let unlock = viewport.bottom_right(UNLOCK_TOUCH);
    canvas.fill(Rect::new(0, top, width as i32, height as i32 - top), 255);
    canvas.text(
        Rect::new(38, top + 12, unlock.x - 60, 48),
        notice.unwrap_or("Touch locked. Use the physical page buttons."),
        CAPTION,
    );
    canvas.button(unlock, "Unlock touch", true, false);
    canvas.button(viewport.done(), "Exit", false, false);
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

fn reflow_mark_location(
    active: &ActiveBook,
    fallback_chapter: usize,
    fallback_page: usize,
    anchor: Option<&crate::Anchor>,
    layouts: &mut HashMap<usize, Option<crate::Probe>>,
) -> (usize, usize, usize, usize) {
    let chapters = active.reading.probe.chapter_entries();
    if chapters.is_empty() {
        return (fallback_chapter, fallback_page, 0, 0);
    }
    let chapter = anchor
        .and_then(|anchor| active.reading.probe.chapter_index(&anchor.chapter))
        .unwrap_or(fallback_chapter)
        .min(chapters.len() - 1);
    let Some(probe) = layouts
        .entry(chapter)
        .or_insert_with(|| active.reading.config.chapter(chapter).ok())
        .as_ref()
    else {
        return (chapter, fallback_page, 0, 0);
    };
    let resolved_anchor = anchor
        .filter(|anchor| anchor.chapter == chapters[chapter].href)
        .cloned()
        .or_else(|| probe.page_anchor(fallback_page));
    let page = resolved_anchor
        .as_ref()
        .and_then(|anchor| probe.page_for(anchor))
        .unwrap_or(fallback_page.min(probe.page_count().saturating_sub(1)));
    resolved_anchor.map_or((chapter, page, 0, 0), |anchor| {
        (chapter, page, anchor.block, anchor.offset)
    })
}

fn reflow_location_label(
    active: &ActiveBook,
    location: (usize, usize, usize, usize),
    saved_title: Option<&str>,
) -> String {
    let chapter = saved_title
        .or_else(|| {
            active
                .reading
                .probe
                .chapter_entries()
                .get(location.0)
                .map(|chapter| chapter.label.as_str())
        })
        .unwrap_or("Book");
    format!("{chapter} · Page {}", location.1 + 1)
}

fn bookmark_excerpt(text: &str, fit_more: bool) -> String {
    let mut words = text.split_whitespace();
    if fit_more {
        return words.collect::<Vec<_>>().join(" ");
    }
    let excerpt = words.by_ref().take(22).collect::<Vec<_>>().join(" ");
    if words.next().is_some() {
        format!("{excerpt}…")
    } else {
        excerpt
    }
}

fn saved_mark_detail(created_at: Option<i64>, detail: String, date_first: bool) -> String {
    if date_first && let Some(date) = created_at.and_then(crate::mark_time::local_datetime) {
        return format!("{date} · {detail}");
    }
    detail
}

fn device_settings() -> Settings {
    Settings {
        width: WIDTH,
        height: PAGE_HEIGHT,
        font_scale: crate::logical_font::ScaleContext::new(WIDTH, HEIGHT),
        ..Settings::default()
    }
}

fn text_size_items(settings: Settings, owner: u64, pending_size: Option<f64>) -> [Item; 3] {
    let size = format!("Current size {}", settings.font_label());
    let change = |delta: f64| {
        pending_size.or(settings.logical_font_size).map_or_else(
            || {
                Action::Text(Settings {
                    font_size: (settings.font_size + settings.font_scale.physical_delta(delta))
                        .clamp(20., 100.),
                    ..settings
                })
            },
            |value| Action::ApplyLogicalFont(owner, (value + delta).clamp(12., 255.)),
        )
    };
    [
        Item::new("Larger text", change(0.5)).detail(&size),
        Item::new("Smaller text", change(-0.5)),
        Item::new(
            "Set font size",
            Action::Show(Screen::NumberingValue(NumberField::LogicalFont(owner))),
        ),
    ]
}

fn text_items(
    settings: Settings,
    scroll: bool,
    owner: u64,
    pending_size: Option<f64>,
) -> Vec<Item> {
    let margin = format!("Current margin {} px", settings.margin);
    let spacing = settings.effective_line_spacing_percent();
    let (alignment, next_alignment) = match settings.alignment {
        TextAlignment::Publisher | TextAlignment::FootnoteJustify => {
            ("Publisher", TextAlignment::Start)
        }
        TextAlignment::Start => ("Start", TextAlignment::Center),
        TextAlignment::Center => ("Centered", TextAlignment::Justify),
        TextAlignment::Justify => ("Justified", TextAlignment::Publisher),
    };
    let [larger, smaller, set_size] = text_size_items(settings, owner, pending_size);
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
        set_size,
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
            Action::Text(settings.with_line_spacing_percent((spacing + 5).min(200))),
        )
        .detail(format!("Current spacing {spacing}%")),
        Item::new(
            "Less line spacing",
            Action::Text(settings.with_line_spacing_percent(spacing.saturating_sub(5).max(50))),
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
    let library_path = data.join("library.json");
    let mut app = Reader::open(documents, &library_path, fonts)?;
    let run =
        std::env::var_os("KL_RUN").map_or_else(|| PathBuf::from("/var/tmp/kl"), PathBuf::from);
    let turn_file = std::env::var_os("KL_APP_TURN_FILE").map(PathBuf::from);
    app.launcher_orientation_handoff = has_launcher_orientation_handoff(&run, turn_file.as_deref());
    if std::env::var("PRIMITIVE_LAUNCHER").as_deref() == Ok("1") {
        let offer = std::env::var_os("KL_APP_VIEWPORT_FILE").map(PathBuf::from);
        app.negotiated_opening_profiles =
            primitive_app::viewport::ViewportPublisher::new(&run, offer)?.is_some();
    }
    if let Some(file) = file {
        if let Err(error) = app.open_path(file) {
            app.report(error);
        }
    } else if let Err(error) = app.show_first_run_quickstart(&library_path) {
        eprintln!("reader quickstart_seen_failed={error}");
    }
    primitive_app::run(ReaderRuntime(app), home).map_err(Into::into)
}

fn has_launcher_orientation_handoff(run: &Path, offered: Option<&Path>) -> bool {
    let Some(path) = offered.filter(|path| path.file_name().is_some_and(|name| name == "turn"))
    else {
        return false;
    };
    // The supervisor creates the directory before launch. The turn file appears after the first draw.
    let Some(parent) = path
        .parent()
        .filter(|parent| parent.parent() == Some(run) && parent.is_dir())
    else {
        return false;
    };
    parent
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_prefix("app-turn."))
        .is_some_and(|token| {
            token.len() == 6 && token.bytes().all(|byte| byte.is_ascii_alphanumeric())
        })
}

/// The device runtime receives physical page keys as `PageForward`/`PageBack`.
/// Keep their user-configured inversion at this boundary so touch gestures and
/// Reader's own synthetic page turns retain their intended reading direction.
struct ReaderRuntime(Reader);

impl App for ReaderRuntime {
    fn draw(&self, canvas: &mut Canvas) {
        self.0.draw(canvas);
    }

    fn input(&mut self, input: Input) -> Change {
        self.0.input(self.0.controls.invert_page_turn_input(input))
    }

    fn swipe(&mut self, input: Input, start_x: f32) -> Change {
        self.0.swipe(input, start_x)
    }

    fn live_touch_pan(&self) -> bool {
        self.0.live_touch_pan()
    }

    fn touch_pan(&mut self, pan: primitive_app::TouchPan) -> Change {
        self.0.touch_pan(pan)
    }

    fn preferred_rotation(&self) -> Option<primitive_app::Rotation> {
        if self.0.under_launcher() && !self.0.launcher_orientation_handoff {
            return None;
        }
        self.0.preferred_rotation()
    }

    fn viewport_request(&self) -> Option<primitive_app::viewport::ViewportRequest> {
        self.0.viewport_request()
    }

    fn viewport_needs_stage(&self, accepted: primitive_app::viewport::Viewport) -> bool {
        self.0.viewport_needs_stage(accepted)
    }

    fn stage_viewport(&mut self, candidate: primitive_app::viewport::Viewport) -> io::Result<()> {
        self.0.stage_viewport(candidate)
    }

    fn draw_staged_viewport(&mut self, canvas: &mut Canvas) -> io::Result<()> {
        self.0.draw_staged_viewport(canvas)
    }

    fn finish_viewport(
        &mut self,
        candidate: primitive_app::viewport::Viewport,
        decision: primitive_app::viewport::ViewportDecision,
    ) {
        self.0.finish_viewport(candidate, decision);
    }

    fn preferred_dark_mode(&self) -> Option<bool> {
        self.0.preferred_dark_mode()
    }

    fn press_point(&self, input: Input) -> Option<(f32, f32)> {
        self.0.press_point(input)
    }

    fn deadline(&self) -> Option<Instant> {
        self.0.deadline()
    }

    fn tick(&mut self, now: Instant) -> Change {
        self.0.tick(now)
    }

    fn save(&mut self) -> io::Result<()> {
        self.0.save()
    }

    fn storage_returned(&mut self) -> io::Result<()> {
        self.0.storage_returned()
    }
}

pub fn run_probe(engine: Engine, book: PathBuf, fonts: Vec<PathBuf>, home: &Path) -> Result<()> {
    let reading = Reading::open(BookConfig {
        viewport: crate::session::ReadingViewport::PORTRAIT,
        user_styles: Default::default(),
        user_hyphenation: Default::default(),
        chapter_offsets: Default::default(),
        quote_prefixes: Default::default(),
        screen_pages: Default::default(),
        eager_layouts: Default::default(),
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
mod viewport_chrome_tests;

#[cfg(test)]
mod typography_pagination_tests;

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

        pub(super) fn app(&self) -> Reader {
            let fonts = [crate::test_fixture("fonts/Silkscreen-Regular.ttf")];
            let mut app = Reader::open(&self.root, &self.state, fonts.to_vec()).unwrap();
            // Most interaction fixtures exercise the action popup explicitly;
            // the stock single-word dictionary default has focused coverage below.
            app.controls.dictionary_on_single_word = Some(false);
            app
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

        let mut reopened = Reader::open(
            &fixture.root,
            &fixture.state,
            vec![crate::test_fixture("fonts/Silkscreen-Regular.ttf")],
        )
        .unwrap();
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
        assert_eq!(rows(&app), ["Screen orientation", "Keep orientation"]);

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
        let mut reopened = Reader::open(
            &fixture.root,
            &fixture.state,
            vec![crate::test_fixture("fonts/Silkscreen-Regular.ttf")],
        )
        .unwrap();
        assert_eq!(reopened.controls, controls);
        reopened.open_path(&fixture.book).unwrap();
        assert_eq!(
            reopened.tap_turn(100.0, 700.0, PAGE_HEIGHT),
            Some(Input::PageForward)
        );
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
        // Deliver a batch at the refresh boundary without depending on how
        // long the scheduler waits between two test input calls.
        let queue = |app: &mut Reader, directions| {
            app.active
                .as_mut()
                .unwrap()
                .reading
                .queue_page_turns(directions);
        };
        queue(&mut app, [1, -1]);
        assert_eq!(app.tick(deadline(&app)), Change::None);
        assert_eq!(app.pages_since_refresh, 0);

        for frame in 1..=5 {
            queue(&mut app, [1, 1]);
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
            logical_font_size: None,
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
    fn status_bar_presets_support_create_apply_update_rename_delete_and_reopen() {
        use crate::reading_controls::{FooterItem, StatusBar};

        let fixture = Fixture::new();
        let mut app = fixture.app();
        let fonts = vec![
            crate::test_fixture("fonts/NotoSans-Regular.ttf"),
            crate::test_fixture("fonts/NotoSans-Bold.ttf"),
            crate::test_fixture("fonts/Silkscreen-Regular.ttf"),
        ];
        app.show(Screen::FooterSettings);
        capture_test_screen(
            &app,
            &fonts,
            "READER_FOOTER_PRESETS_SETTINGS_SCREENSHOT",
            false,
        );
        app.show(Screen::FooterPresets);
        capture_test_screen(&app, &fonts, "READER_FOOTER_PRESETS_LIST_SCREENSHOT", false);
        app.controls = ReadingControls {
            status_bar: StatusBar::Custom,
            footer_items: FooterItem::CustomText.bit(),
            footer_horizontal_separator: true,
            invert_page_turn_buttons: true,
            night_pages: true,
            ..ReadingControls::default()
        };
        let mut custom_text = footer_custom_text::Preference::default();
        custom_text.set_text("%T · %c/%t").unwrap();
        custom_text.set_repetitions(3).unwrap();
        app.footer_custom_text = custom_text.clone();
        app.save_footer_preset_name(FooterPresetName::Create, "Evening")
            .unwrap();
        assert_eq!(app.screen, Screen::FooterPresets);
        assert_eq!(app.footer_presets.len(), 1);
        capture_test_screen(
            &app,
            &fonts,
            "READER_FOOTER_PRESETS_LIST_SAVED_SCREENSHOT",
            false,
        );
        app.show(Screen::FooterPreset(0));
        capture_test_screen(
            &app,
            &fonts,
            "READER_FOOTER_PRESET_DETAIL_SCREENSHOT",
            false,
        );
        app.show(Screen::FooterPresetDelete(0));
        capture_test_screen(
            &app,
            &fonts,
            "READER_FOOTER_PRESET_DELETE_SCREENSHOT",
            false,
        );

        app.controls = ReadingControls {
            footer_horizontal_separator: false,
            invert_page_turn_buttons: false,
            night_pages: false,
            ..ReadingControls::default()
        };
        app.footer_custom_text = footer_custom_text::Preference::default();
        app.activate(Action::ApplyFooterPreset(0)).unwrap();
        assert!(app.controls.footer_horizontal_separator);
        assert!(!app.controls.invert_page_turn_buttons);
        assert!(!app.controls.night_pages);
        assert_eq!(app.footer_custom_text, custom_text);
        assert_eq!(
            footer_custom_text::load(&app.footer_custom_text_path),
            custom_text
        );

        app.controls.footer_item_bold = true;
        app.activate(Action::UpdateFooterPreset(0)).unwrap();
        app.activate(Action::RenameFooterPreset(0)).unwrap();
        assert_eq!(
            app.screen,
            Screen::FooterPresetName(FooterPresetName::Rename(0))
        );
        app.save_footer_preset_name(FooterPresetName::Rename(0), "Evening Reading")
            .unwrap();
        assert_eq!(app.footer_presets[0].name, "Evening Reading");
        assert!(
            app.footer_presets[0]
                .apply(ReadingControls::default())
                .0
                .footer_item_bold
        );
        assert_eq!(
            app.footer_presets[0].apply(ReadingControls::default()).2,
            custom_text
        );

        let mut reopened = fixture.app();
        assert_eq!(reopened.footer_presets[0].name, "Evening Reading");
        assert_eq!(reopened.footer_custom_text, custom_text);
        reopened.show(Screen::FooterPresets);
        assert!(
            reopened
                .items()
                .iter()
                .any(|item| item.title == "Evening Reading")
        );
        reopened.activate(Action::DeleteFooterPreset(0)).unwrap();
        assert!(reopened.footer_presets.is_empty());
        assert!(fixture.app().footer_presets.is_empty());
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
        assert_eq!(
            reset.config.settings.logical_font_size,
            Some(crate::logical_font::DEFAULT)
        );
        assert_eq!(
            reset.config.settings.font_size,
            device_settings()
                .font_scale
                .effective(crate::logical_font::DEFAULT)
                .unwrap()
        );
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
        let hash =
            md5::Digest(koreader_sidecar::partial_md5(&mut fs::File::open(&pdf).unwrap()).unwrap());
        let progress = |page| {
            format!(
                "return {{last_page = {page}, doc_pages = 2, partial_md5_checksum = '{hash:x}'}}"
            )
        };
        let nearby = pdf.with_extension("sdr");
        fs::create_dir(&nearby).unwrap();
        let metadata = nearby.join("metadata.PDF.lua");
        fs::write(&metadata, progress(2)).unwrap();
        fs::write(nearby.join("metadata.epub.lua"), "return {}").unwrap();
        fs::write(fixture.root.join("koreader-progress.lua"), progress(1)).unwrap();
        app.activate(Action::PreviewKoReaderPosition).unwrap();
        assert!(
            app.items()
                .iter()
                .any(|item| item.title == "Saved beside this book")
        );
        assert_eq!(
            app.position_import
                .as_ref()
                .unwrap()
                .target
                .as_ref()
                .unwrap()
                .page,
            1
        );
        assert_eq!(app.current_position().unwrap().page, 0);
        app.activate(Action::ApplyKoReaderPosition).unwrap();
        assert_eq!(app.current_position().unwrap().page, 1);
        app.show(Screen::ReadingData);
        tap_contents_item(&mut app, "Import a copied KOReader position");
        assert!(
            app.items()
                .iter()
                .any(|item| item.title == "Copied to Reader data")
        );
        assert_eq!(
            app.position_import
                .as_ref()
                .unwrap()
                .target
                .as_ref()
                .unwrap()
                .page,
            0
        );
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
            fs::write(
                directory.join("metadata.pdf.lua"),
                format!(
                    "return {{last_page=2, doc_pages=2, partial_md5_checksum='{:x}'}}",
                    md5::Digest(hash)
                ),
            )
            .unwrap();
            path
        };
        let mut app = Reader::open(
            book.parent().unwrap(),
            &fixture.state,
            vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")],
        )
        .unwrap();
        assert!(app.open_path(&book).is_ok());
        let entry = &app.fixed.as_ref().unwrap().entry;
        let metadata = app
            .library
            .nearby_koreader_metadata(&entry.path)
            .unwrap()
            .unwrap();
        let before = fs::read(&metadata).unwrap();
        let expected = koreader_sidecar::parse(&before).unwrap().last_page.unwrap() as usize - 1;
        let old = app.current_position().unwrap();
        app.show(Screen::ReadingControls);
        tap_contents_item(&mut app, "Import KOReader position");
        assert!(
            app.items()
                .iter()
                .any(|row| row.title == "Saved beside this book")
        );
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

    fn skim_control(rect: Rect) -> Input {
        Input::Tap((rect.x + rect.w / 2) as f32, (rect.y + rect.h / 2) as f32)
    }

    #[test]
    fn skim_previews_epub_and_cancel_restores_origin_without_history() {
        let fixture = Fixture::new();
        let long = fixture.root.join("Long Chapter.epub");
        fs::write(&long, include_bytes!("../tests/fixtures/long-chapter.epub")).unwrap();
        let mut app = fixture.app();
        app.open_path(&long).unwrap();
        let origin = app.current_history_location().unwrap();
        let history = app.back.clone();

        app.activate(Action::Skim).unwrap();
        assert_eq!(app.screen, Screen::Skim);
        capture_test_screen(&app, &simulation_fonts(), "READER_SKIM_SCREENSHOT", false);
        assert_eq!(app.input(Input::PageForward), Change::Partial);
        for _ in 0..8 {
            if !app.active.as_ref().unwrap().reading.is_turning() {
                break;
            }
            let due = app.deadline().unwrap();
            app.tick(due + Duration::from_secs(1));
        }
        assert!(!app.active.as_ref().unwrap().reading.is_turning());
        assert_ne!(app.current_history_location().as_ref(), Some(&origin));

        let cancel = skim_control(app.skim_layout().cancel);
        assert_eq!(app.input(cancel), Change::Partial);
        assert_eq!(app.screen, Screen::Book);
        assert_eq!(app.current_history_location().as_ref(), Some(&origin));
        assert_eq!(app.back, history);
    }

    #[test]
    fn skim_cancel_during_queued_epub_turns_restores_origin_and_drops_the_queue() {
        let fixture = Fixture::new();
        let long = fixture.root.join("Long Chapter.epub");
        fs::write(&long, include_bytes!("../tests/fixtures/long-chapter.epub")).unwrap();
        let mut app = fixture.app();
        app.open_path(&long).unwrap();
        let origin = app.current_history_location().unwrap();
        let history = app.back.clone();

        app.activate(Action::Skim).unwrap();
        for _ in 0..4 {
            assert_eq!(app.input(Input::PageForward), Change::Partial);
        }
        assert!(app.active.as_ref().unwrap().reading.is_turning());

        assert_eq!(
            app.input(skim_control(app.skim_layout().cancel)),
            Change::Partial
        );
        assert_eq!(app.screen, Screen::Book);
        assert_eq!(app.current_history_location().as_ref(), Some(&origin));
        assert_eq!(app.back, history);
        assert!(!app.active.as_ref().unwrap().reading.is_turning());
    }

    #[test]
    fn skim_commits_one_history_return_after_a_queued_epub_turn() {
        let fixture = Fixture::new();
        let long = fixture.root.join("Long Chapter.epub");
        fs::write(&long, include_bytes!("../tests/fixtures/long-chapter.epub")).unwrap();
        let mut app = fixture.app();
        app.open_path(&long).unwrap();
        let origin = app.current_history_location().unwrap();
        let history_len = app.back.len();

        app.activate(Action::Skim).unwrap();
        assert_eq!(app.input(Input::PageForward), Change::Partial);
        let read_here = skim_control(app.skim_layout().commit);
        assert_eq!(app.input(read_here), Change::Partial);
        assert_eq!(app.screen, Screen::Skim);
        for _ in 0..8 {
            if app.screen != Screen::Skim {
                break;
            }
            let due = app.deadline().unwrap();
            app.tick(due + Duration::from_secs(1));
        }
        assert_eq!(app.screen, Screen::Book);
        assert_eq!(app.back.len(), history_len + 1);
        assert_eq!(app.back.last(), Some(&origin));
        assert_ne!(app.current_history_location().as_ref(), Some(&origin));

        app.activate(Action::ReturnToLocation).unwrap();
        assert_eq!(app.current_history_location().as_ref(), Some(&origin));
    }

    #[test]
    fn skim_progress_drag_previews_across_chapters_and_commits_a_return_location() {
        let fixture = Fixture::new();
        let book = fixture.root.join("Many chapters.epub");
        fs::copy(crate::test_fixture("many-chapters.epub"), &book).unwrap();
        let mut app = fixture.app();
        app.open_path(&book).unwrap();
        assert!(
            app.active
                .as_ref()
                .unwrap()
                .reading
                .probe
                .chapter_entries()
                .len()
                > 2
        );
        let origin = app.current_history_location().unwrap();

        app.activate(Action::Skim).unwrap();
        let track = app.skim_layout().track;
        let track_y = (track.y + track.h / 2) as f32;
        assert_eq!(app.input(Input::Tap(300.0, track_y)), Change::Partial);
        let swipe = Input::Swipe {
            dx: 600.0,
            dy: 0.0,
            start_y: track_y,
        };
        assert_eq!(app.swipe(swipe, 400.0), Change::Partial);
        let preview = app.current_history_location().unwrap();
        assert!(preview.position.chapter > origin.position.chapter);

        assert_eq!(
            app.input(skim_control(app.skim_layout().commit)),
            Change::Partial
        );
        assert_eq!(app.screen, Screen::Book);
        assert_eq!(app.current_history_location().as_ref(), Some(&preview));
        assert_eq!(app.back.last(), Some(&origin));
        app.activate(Action::ReturnToLocation).unwrap();
        assert_eq!(app.current_history_location().as_ref(), Some(&origin));
    }

    #[test]
    fn skim_cancels_fixed_page_scrub_back_to_original_page() {
        let fixture = Fixture::new();
        let pdf = fixture.root.join("Tilted Scan.pdf");
        fs::copy(crate::test_fixture("tilted-scan.pdf"), &pdf).unwrap();
        let mut app = fixture.app();
        app.open_path(&pdf).unwrap();
        let origin = app.current_history_location().unwrap();
        let history = app.back.clone();

        app.activate(Action::Skim).unwrap();
        capture_test_screen(
            &app,
            &simulation_fonts(),
            "READER_SKIM_FIXED_SCREENSHOT",
            false,
        );
        let track = app.skim_layout().track;
        let scrub = Input::Tap(1100.0, (track.y + track.h / 2) as f32);
        assert_eq!(app.input(scrub), Change::Partial);
        assert_eq!(app.fixed.as_ref().unwrap().reading.page(), 1);
        let cancel = skim_control(app.skim_layout().cancel);
        assert_eq!(app.input(cancel), Change::Partial);
        assert_eq!(app.screen, Screen::Fixed);
        assert_eq!(app.current_history_location().as_ref(), Some(&origin));
        assert_eq!(app.back, history);
    }

    #[test]
    fn skim_keeps_done_exit_available() {
        let fixture = Fixture::new();
        let mut app = fixture.app();
        app.open_path(&fixture.book).unwrap();
        app.activate(Action::Skim).unwrap();
        assert_eq!(app.screen, Screen::Skim);
        assert_eq!(
            app.input(Input::Tap((DONE.x + 20) as f32, (DONE.y + 20) as f32)),
            Change::Exit
        );
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
        app.activate(Action::Text(Settings {
            font_size: 50.0,
            ..device_settings()
        }))
        .unwrap();
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
        assert_eq!(
            expected.block,
            reading.probe.fragment("door").unwrap().block
        );
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
        fs::write(
            fixture.root.join(path),
            crate::kf8::tests::linked_image_fixture(),
        )
        .unwrap();
        let mut app = fixture.app();
        app.open_path(path).unwrap();
        let before = position(&app.active.as_ref().unwrap().reading);
        let target = app
            .active
            .as_ref()
            .unwrap()
            .reading
            .links()
            .into_iter()
            .find_map(|(label, target)| (label.contains("View original")).then_some(target))
            .unwrap();
        app.activate(Action::Jump(target)).unwrap();
        assert_eq!(app.screen, Screen::Illustration);
        assert!(app.illustration.is_some());
        app.input(Input::Tap((DONE.x + 15) as f32, (DONE.y + 15) as f32));
        assert_eq!(app.screen, Screen::Book);
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
            .map(|text| dictionary::LookupEntry {
                definition: dictionary::Definition {
                    dictionary: "Sample".into(),
                    word: "Tide".into(),
                    text: text.into(),
                },
                article: None,
                source: PathBuf::new(),
            })
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
        let [_, collapse, _] = definition_actions(app.definition_layout().0);
        assert_eq!(
            app.input(Input::Tap(
                (collapse.x + 15) as f32,
                (collapse.y + 15) as f32
            )),
            Change::Partial
        );
        assert!(app.definition_over_book());
        let [_, expand, _] = definition_actions(app.definition_layout().0);
        assert_eq!(
            app.input(Input::Tap((expand.x + 15) as f32, (expand.y + 15) as f32)),
            Change::Partial
        );
        let back = primitive_ui::back_button(WIDTH as i32);
        assert_eq!(
            app.input(Input::Tap((back.x + 15) as f32, (back.y + 15) as f32)),
            Change::Partial
        );
        assert_eq!(app.screen, Screen::Book);
        assert_eq!(position(&app.active.as_ref().unwrap().reading), original);
    }

    #[test]
    fn dictionary_result_moves_to_the_other_side_of_selected_text() {
        let lower_text = Rect::new(410, 1080, 55, 32);
        let upper_text = Rect::new(410, 160, 55, 32);
        let viewport = crate::session::ReadingViewport::PORTRAIT;
        let above = definition_bounds(420, Some(lower_text), viewport);
        let below = definition_bounds(420, Some(upper_text), viewport);
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
                updated_at: None,
                chapter_title: None,
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
    fn bookmark_list_uses_reading_order_and_keeps_original_targets() {
        let fixture = Fixture::new();
        let mut app = fixture.app();
        app.open_path(&fixture.book).unwrap();
        let (entry, id, title) = {
            let active = app.active.as_ref().unwrap();
            (active.entry.clone(), active.id, active.entry.title.clone())
        };
        let state = app.library.remember(&entry, id, &title);
        state.bookmarks = vec![
            Bookmark {
                chapter_title: None,
                created_at: Some(10),
                label: "Later chapter".into(),
                position: ReadingPosition {
                    chapter: 1,
                    page: 0,
                    anchor: None,
                },
            },
            Bookmark {
                chapter_title: None,
                created_at: Some(20),
                label: "Earlier chapter".into(),
                position: ReadingPosition {
                    chapter: 0,
                    page: 4,
                    anchor: None,
                },
            },
        ];

        app.show(Screen::Bookmarks);
        let items = app.items();
        let earlier = items
            .iter()
            .find(|item| item.title == "Earlier chapter")
            .unwrap();
        let later = items
            .iter()
            .find(|item| item.title == "Later chapter")
            .unwrap();
        assert!(matches!(earlier.action, Action::Bookmark(1)));
        assert!(matches!(later.action, Action::Bookmark(0)));
        let font = crate::test_fixture("fonts/NotoSans-Regular.ttf");
        capture_test_screen(
            &app,
            std::slice::from_ref(&font),
            "READER_BOOKMARK_ORDER_SCREENSHOT",
            false,
        );
    }

    #[test]
    fn bookmark_settings_select_date_and_reverse_order_and_persist() {
        let fixture = Fixture::new();
        let mut app = fixture.app();
        app.open_path(&fixture.book).unwrap();
        let (entry, id, title) = {
            let active = app.active.as_ref().unwrap();
            (active.entry.clone(), active.id, active.entry.title.clone())
        };
        let state = app.library.remember(&entry, id, &title);
        state.bookmarks = vec![
            Bookmark {
                chapter_title: None,
                created_at: Some(10),
                label: "Later chapter".into(),
                position: ReadingPosition {
                    chapter: 1,
                    page: 0,
                    anchor: None,
                },
            },
            Bookmark {
                chapter_title: None,
                created_at: Some(20),
                label: "Earlier chapter".into(),
                position: ReadingPosition {
                    chapter: 0,
                    page: 4,
                    anchor: None,
                },
            },
        ];

        app.show(Screen::ReadingControls);
        let bookmark_list = app
            .items()
            .into_iter()
            .find(|item| item.title == "Bookmarks and notes")
            .unwrap();
        app.activate(bookmark_list.action).unwrap();
        assert_eq!(app.screen, Screen::BookmarkListSettings);
        let font = crate::test_fixture("fonts/NotoSans-Regular.ttf");
        capture_test_screen(
            &app,
            std::slice::from_ref(&font),
            "READER_BOOKMARK_SETTINGS_SCREENSHOT",
            false,
        );
        assert!(
            app.items()
                .iter()
                .any(|item| item.title == "Sort by page"
                    && item.detail.as_deref() == Some("Selected"))
        );

        let date = app
            .items()
            .into_iter()
            .find(|item| item.title == "Sort by date")
            .unwrap();
        app.activate(date.action).unwrap();
        assert!(app.controls.bookmark_sort_by_date);
        assert!(
            crate::reading_controls::ReadingControls::load(&app.controls_path, false)
                .bookmark_sort_by_date
        );
        app.show(Screen::Bookmarks);
        let items = app.items();
        let titles: Vec<_> = items
            .iter()
            .filter(|item| matches!(item.action, Action::Bookmark(_)))
            .map(|item| item.title.as_str())
            .collect();
        assert_eq!(titles, ["Later chapter", "Earlier chapter"]);

        app.show(Screen::BookmarkListSettings);
        let reverse = app
            .items()
            .into_iter()
            .find(|item| item.title == "Reverse sorting")
            .unwrap();
        app.activate(reverse.action).unwrap();
        assert!(app.controls.bookmark_reverse);
        assert!(
            crate::reading_controls::ReadingControls::load(&app.controls_path, false)
                .bookmark_reverse
        );
        app.show(Screen::Bookmarks);
        let items = app.items();
        let titles: Vec<_> = items
            .iter()
            .filter(|item| matches!(item.action, Action::Bookmark(_)))
            .map(|item| item.title.as_str())
            .collect();
        assert_eq!(titles, ["Earlier chapter", "Later chapter"]);
    }

    #[test]
    fn saved_marks_mix_bookmarks_and_highlights_without_changing_targets() {
        let fixture = Fixture::new();
        let mut app = fixture.app();
        app.open_path(&fixture.book).unwrap();

        {
            let reading = &mut app.active.as_mut().unwrap().reading;
            reading.jump(0, Some("lantern")).unwrap();
            let start = reading.probe.fragment("lantern").unwrap();
            let mut end = start.clone();
            end.offset += "The copper lantern".len();
            app.selection = reading.probe.select_range(&start, &end);
        }
        app.save_highlight(Some("First chapter note".into()))
            .unwrap();
        {
            let reading = &mut app.active.as_mut().unwrap().reading;
            reading.jump(1, Some("door")).unwrap();
            let start = reading.probe.fragment("door").unwrap();
            let mut end = start.clone();
            end.offset += "At dusk the door".len();
            app.selection = reading.probe.select_range(&start, &end);
        }
        app.save_highlight(Some("Second chapter note".into()))
            .unwrap();

        let active = app.active.as_ref().unwrap();
        let state = app.library.state_mut(active.id).unwrap();
        state.bookmarks.push(Bookmark {
            chapter_title: None,
            created_at: Some(1_750_000_001),
            label: "Saved page".into(),
            position: ReadingPosition {
                chapter: 0,
                page: 0,
                anchor: None,
            },
        });
        state.highlights[0].created_at = Some(1_750_000_002);
        state.highlights[1].created_at = Some(1_750_000_003);
        app.controls.bookmark_sort_by_date = true;

        app.show(Screen::Highlights);
        let fonts = simulation_fonts();
        capture_test_screen(&app, &fonts, "READER_SAVED_MARKS_SCREENSHOT", false);
        let marks: Vec<_> = app
            .items()
            .into_iter()
            .filter(|item| matches!(item.action, Action::Bookmark(_) | Action::Highlight(_)))
            .collect();
        assert_eq!(
            marks
                .iter()
                .map(|item| item.title.as_str())
                .collect::<Vec<_>>(),
            ["Saved page", "First chapter note", "Second chapter note"]
        );
        assert!(matches!(marks[0].action, Action::Bookmark(0)));
        assert!(matches!(marks[1].action, Action::Highlight(0)));
        assert!(matches!(marks[2].action, Action::Highlight(1)));
        let saved = crate::mark_time::local_datetime(1_750_000_002).unwrap();
        assert_eq!(
            marks[1].detail.as_deref(),
            Some(format!("{saved} · Note · Reading the Shore · Page 1").as_str())
        );

        app.controls.bookmark_sort_by_date = false;
        let page_order: Vec<_> = app
            .items()
            .into_iter()
            .filter(|item| matches!(item.action, Action::Bookmark(_) | Action::Highlight(_)))
            .map(|item| item.title)
            .collect();
        assert_eq!(
            page_order,
            ["Saved page", "First chapter note", "Second chapter note"]
        );
        app.controls.bookmark_reverse = true;
        let reverse_order: Vec<_> = app
            .items()
            .into_iter()
            .filter(|item| matches!(item.action, Action::Bookmark(_) | Action::Highlight(_)))
            .map(|item| item.title)
            .collect();
        assert_eq!(
            reverse_order,
            ["Second chapter note", "First chapter note", "Saved page"]
        );
        app.controls.bookmark_sort_by_date = true;
        app.controls.bookmark_reverse = false;

        let second_mark = app
            .items()
            .iter()
            .position(|item| matches!(item.action, Action::Highlight(1)))
            .unwrap();
        let row = app.list_geometry().row(second_mark);
        let y = row.y + row.h / 2;
        assert_eq!(
            app.input(Input::Tap(LIST_X as f32 + 30.0, y as f32)),
            Change::Partial
        );
        assert_eq!(app.screen, Screen::Book);
        assert_eq!(app.active.as_ref().unwrap().reading.chapter, 1);

        app.show(Screen::Highlights);
        let second_mark = app
            .items()
            .iter()
            .position(|item| matches!(item.action, Action::Highlight(1)))
            .unwrap();
        let row = app.list_geometry().row(second_mark);
        let y = row.y + row.h / 2;
        assert_eq!(
            app.input(Input::LongTap(LIST_X as f32 + 30.0, y as f32)),
            Change::Partial
        );
        assert_eq!(app.screen, Screen::Annotation);
        assert_eq!(app.highlight_index, 1);
        app.activate(Action::Highlight(1)).unwrap();
        assert_eq!(app.active.as_ref().unwrap().reading.chapter, 1);
    }

    #[test]
    fn saved_pdf_marks_share_bookmarks_order_and_keep_pdf_actions() {
        let fixture = Fixture::new();
        let pdf = fixture.root.join("Saved marks.pdf");
        fs::write(&pdf, crate::fixed::tests::document()).unwrap();
        let mut app = fixture.app();
        app.open_path(&pdf).unwrap();
        let (id, entry, title) = {
            let fixed = app.fixed.as_ref().unwrap();
            (fixed.id, fixed.entry.clone(), fixed.entry.title.clone())
        };
        let state = app.library.remember(&entry, id, &title);
        state.bookmarks.push(Bookmark {
            chapter_title: None,
            created_at: Some(1_750_000_001),
            label: "Page bookmark".into(),
            position: ReadingPosition {
                chapter: 0,
                page: 0,
                anchor: None,
            },
        });
        state
            .add_pdf_highlight(PdfHighlight {
                updated_at: None,
                chapter_title: None,
                color: Default::default(),
                created_at: Some(1_750_000_002),
                style: Default::default(),
                continuation: Vec::new(),
                page: 0,
                bounds: vec![PdfTextRect {
                    x0: 0.1,
                    y0: 0.1,
                    x1: 0.2,
                    y1: 0.2,
                }],
                text: "A saved PDF passage".into(),
                note: Some("PDF note".into()),
            })
            .unwrap();
        app.controls.bookmark_sort_by_date = true;

        app.show(Screen::FixedBookmarks);
        let marks: Vec<_> = app
            .items()
            .into_iter()
            .filter(|item| matches!(item.action, Action::Bookmark(_) | Action::PdfHighlight(_)))
            .collect();
        assert_eq!(
            marks
                .iter()
                .map(|item| item.title.as_str())
                .collect::<Vec<_>>(),
            ["Page bookmark", "PDF note"]
        );
        assert!(matches!(marks[0].action, Action::Bookmark(0)));
        assert!(matches!(marks[1].action, Action::PdfHighlight(0)));
        app.activate(marks.into_iter().nth(1).unwrap().action)
            .unwrap();
        assert_eq!(app.screen, Screen::Fixed);
        app.activate(Action::PdfHighlight(0)).unwrap();
        assert_eq!(app.fixed.as_ref().unwrap().reading.page(), 0);

        app.show(Screen::FixedBookmarks);
        let highlight_row = app
            .items()
            .iter()
            .position(|item| matches!(item.action, Action::PdfHighlight(0)))
            .unwrap();
        let row = app.list_geometry().row(highlight_row);
        let y = row.y + row.h / 2;
        assert_eq!(
            app.input(Input::LongTap(LIST_X as f32 + 30.0, y as f32)),
            Change::Partial
        );
        assert_eq!(app.screen, Screen::Annotation);
        app.activate(Action::PdfHighlight(0)).unwrap();
        assert_eq!(app.screen, Screen::Fixed);
    }

    #[test]
    fn annotation_list_text_modes_choose_excerpt_or_note_and_persist() {
        let fixture = Fixture::new();
        let mut app = fixture.app();
        app.open_path(&fixture.book).unwrap();
        let reading = &mut app.active.as_mut().unwrap().reading;
        reading.jump(0, Some("lantern")).unwrap();
        let start = reading.probe.fragment("lantern").unwrap();
        let mut end = start.clone();
        end.offset += "The copper lantern".len();
        app.selection = reading.probe.select_range(&start, &end);
        app.save_highlight(Some("Compare the Amber chart".into()))
            .unwrap();

        app.show(Screen::Highlights);
        let default_row = app.items().pop().unwrap();
        assert_eq!(default_row.title, "Compare the Amber chart");
        assert_eq!(
            default_row.detail.as_deref(),
            Some("Note · Reading the Shore · Page 1")
        );
        let font = crate::test_fixture("fonts/NotoSans-Regular.ttf");
        capture_test_screen(
            &app,
            std::slice::from_ref(&font),
            "READER_ANNOTATION_LIST_SCREENSHOT",
            false,
        );

        app.show(Screen::ReadingControls);
        let bookmark_settings = app
            .items()
            .into_iter()
            .find(|item| item.title == "Bookmarks and notes")
            .unwrap();
        app.activate(bookmark_settings.action).unwrap();
        let display_settings = app
            .items()
            .into_iter()
            .find(|item| item.title == "Show in highlight rows")
            .unwrap();
        app.activate(display_settings.action).unwrap();
        assert_eq!(app.screen, Screen::AnnotationListSettings);
        let both = app
            .items()
            .into_iter()
            .find(|item| item.title == "Highlighted text and note")
            .unwrap();
        app.activate(both.action).unwrap();
        assert_eq!(
            crate::reading_controls::ReadingControls::load(&app.controls_path, false)
                .annotation_list_text,
            AnnotationListText::HighlightAndNote
        );

        let (x, y) = rectangle_center(primitive_ui::back_button(WIDTH as i32));
        app.input(Input::Tap(x, y));
        assert_eq!(app.screen, Screen::BookmarkListSettings);
        app.input(Input::Tap(x, y));
        assert_eq!(app.screen, Screen::ReadingControls);
        app.show(Screen::Highlights);
        let both_row = app.items().pop().unwrap();
        assert_eq!(both_row.title, "The copper lantern");
        assert_eq!(
            both_row.detail.as_deref(),
            Some("Note: Compare the Amber chart · Reading the Shore · Page 1")
        );

        app.show(Screen::ReadingControls);
        let bookmark_settings = app
            .items()
            .into_iter()
            .find(|item| item.title == "Bookmarks and notes")
            .unwrap();
        app.activate(bookmark_settings.action).unwrap();
        let display_settings = app
            .items()
            .into_iter()
            .find(|item| item.title == "Show in highlight rows")
            .unwrap();
        app.activate(display_settings.action).unwrap();
        let text_only = app
            .items()
            .into_iter()
            .find(|item| item.title == "Highlighted text")
            .unwrap();
        app.activate(text_only.action).unwrap();
        app.show(Screen::Highlights);
        let text_row = app.items().pop().unwrap();
        assert_eq!(text_row.title, "The copper lantern");
        assert_eq!(
            text_row.detail.as_deref(),
            Some("Note · Reading the Shore · Page 1")
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
        let [label] = book.printed_pages() else {
            panic!("Expected the surviving publisher boundary")
        };
        assert_eq!(label.label, "2");
    }

    #[test]
    fn publisher_pages_browse_exact_destinations_and_follow_the_visible_passage() {
        let fixture = Fixture::new();
        let mut nav = String::from(r#"<html><body><nav type="page-list"><ol>"#);
        nav.push_str(r#"<li><a href="first.xhtml#passage2">iii</a></li>"#);
        for n in 3..=14 {
            nav.push_str(&format!(
                r#"<li><a href="first.xhtml#passage{n}">{n}</a></li>"#
            ));
        }
        nav.push_str(r#"<li><a href="second.xhtml">14</a></li></ol></nav></body></html>"#);
        write_outline_fixture(&fixture.book, Some(nav.as_bytes()), None);
        let fonts = std::env::var_os("READER_TEST_FONTS")
            .map(|value| std::env::split_paths(&value).collect::<Vec<_>>())
            .unwrap_or_else(|| vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")]);
        let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
        app.controls.dictionary_on_single_word = Some(false);
        app.activate(Action::SetReadingControls(ReadingControls {
            status_bar: crate::reading_controls::StatusBar::PageNumbers,
            ..app.controls
        }))
        .unwrap();
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
            let paths = fonts
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect::<Vec<_>>();
            let mut canvas = Canvas::new(WIDTH, HEIGHT, &paths).unwrap();
            primitive_app::frame(app, &mut canvas, dark)
        };
        let original = frame(&app, false);
        app.open_reading_edge(true, 520.);
        tap_reading_control(&mut app, "Printed pages in margin");
        let marked = frame(&app, false);
        let margin = app
            .active
            .as_ref()
            .unwrap()
            .reading
            .probe
            .settings
            .side_margin();
        let changes = original
            .iter()
            .zip(&marked)
            .enumerate()
            .filter(|(_, (a, b))| a != b)
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        assert!(
            !changes.is_empty(),
            "A visible source boundary should draw a label"
        );
        assert!(
            changes
                .iter()
                .all(|index| index % WIDTH as usize >= (WIDTH - margin) as usize
                    && index / (WIDTH as usize) < PAGE_HEIGHT as usize)
        );
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
        assert_eq!(
            reopened.status_caption().as_deref(),
            Some("Printed page 14")
        );
        let next = Settings {
            font_size: 48.,
            logical_font_size: None,
            ..reopened.active.as_ref().unwrap().reading.config.settings
        };
        reopened.activate(Action::Text(next)).unwrap();
        assert_eq!(
            reopened.status_caption().as_deref(),
            Some("Printed page 14")
        );
        reopened.activate(Action::TogglePublisherFooter).unwrap();
        assert_eq!(reopened.status_caption(), None);
        let mut persisted = fixture.app();
        persisted.activate(Action::Continue).unwrap();
        assert!(!persisted.publisher_footer_enabled());
    }

    fn finish_character_numbering(app: &mut Reader) {
        let limit = app.active.as_ref().unwrap().reading.probe.chapter_count() + 1;
        for _ in 0..limit {
            let Some(scan) = &app.numbering_scan else {
                return;
            };
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
        for (row, expected) in [
            (0, "Page iii"),
            (1, "Page iii"),
            (3, "Page iii"),
            (4, "Page vii"),
        ] {
            app.preview_result(row).unwrap();
            assert_eq!(app.result_context.as_ref().unwrap().label, expected);
        }

        app.activate(Action::SetCharacterPages(Some(500))).unwrap();
        finish_character_numbering(&mut app);
        app.start_document_search("headings here").unwrap();
        wait_for_search(&mut app, 1);
        let last = app
            .active
            .as_ref()
            .unwrap()
            .numbering
            .as_ref()
            .unwrap()
            .count;
        assert!(last > 14);
        assert_eq!(app.hits[0].chapter, 1);
        app.preview_result(0).unwrap();
        assert_eq!(
            app.result_context.as_ref().unwrap().label,
            format!("Page {last}")
        );
        assert_eq!(app.active.as_ref().unwrap().reading.chapter, 0);
    }

    #[test]
    fn character_numbering_reflows_resumes_and_restores_the_publisher_choice() {
        let f = Fixture::new();
        let nav = br#"<html><body><nav type="page-list"><ol><li><a href="first.xhtml#passage2">iii</a></li></ol></nav></body></html>"#;
        write_outline_fixture(&f.book, Some(nav), None);
        let fonts = simulation_fonts();
        let mut app = Reader::open(&f.root, &f.state, fonts.clone()).unwrap();
        app.activate(Action::SetReadingControls(ReadingControls {
            status_bar: crate::reading_controls::StatusBar::PageNumbers,
            ..app.controls
        }))
        .unwrap();
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
        let before = app
            .active
            .as_ref()
            .unwrap()
            .numbering
            .as_ref()
            .unwrap()
            .locate(current);
        app.show(Screen::Book);
        let settings = Settings {
            font_size: 48.,
            logical_font_size: None,
            ..app.active.as_ref().unwrap().reading.config.settings
        };
        app.activate(Action::Text(settings)).unwrap();
        app.activate(Action::CharacterPage(current)).unwrap();
        app.save().unwrap();
        let mut resumed = Reader::open(&f.root, &f.state, fonts.clone()).unwrap();
        resumed.activate(Action::Continue).unwrap();
        assert!(resumed.numbering_scan.is_none());
        assert_eq!(
            resumed
                .active
                .as_ref()
                .unwrap()
                .numbering
                .as_ref()
                .unwrap()
                .locate(current),
            before
        );
        resumed.show(Screen::GoToPage);
        resumed.editor.text = (format!("label {}", current + 1)).chars().collect();
        resumed.jump_reflow_number().unwrap();
        assert!(resumed.current_publisher_page().unwrap() >= current);
        fs::write(resumed.numbering_path(id), b"{truncated").unwrap();
        let mut restored = Reader::open(&f.root, &f.state, fonts).unwrap();
        restored.activate(Action::Continue).unwrap();
        assert!(restored.numbering_scan.is_some());
        finish_character_numbering(&mut restored);
        assert_eq!(
            restored
                .active
                .as_ref()
                .unwrap()
                .numbering
                .as_ref()
                .unwrap()
                .count,
            count
        );
        restored.activate(Action::SetCharacterPages(None)).unwrap();
        assert_eq!(
            restored.status_caption().as_deref(),
            Some("Printed page iii")
        );
        assert!(
            restored
                .library
                .state(id)
                .unwrap()
                .character_page_stride
                .is_none()
        );
        restored
            .activate(Action::SetCharacterPages(Some(3000)))
            .unwrap();
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
            stride: Some(750),
            prompt_publisher: true,
            ..Default::default()
        };
        app.activate(Action::SetReadingControls(ReadingControls {
            numbering: defaults,
            ..app.controls
        }))
        .unwrap();
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
        fs::write(
            &other,
            include_bytes!("../tests/fixtures/field-notes-epub3.epub"),
        )
        .unwrap();
        app.open_path(&other).unwrap();
        assert!(app.numbering_scan.is_some());
        finish_character_numbering(&mut app);
        assert_eq!(
            app.active
                .as_ref()
                .unwrap()
                .numbering
                .as_ref()
                .unwrap()
                .stride,
            750
        );
        defaults.override_publisher = true;
        defaults.label_size = 20;
        app.activate(Action::SetReadingControls(ReadingControls {
            numbering: defaults,
            ..app.controls
        }))
        .unwrap();
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
        assert_eq!(
            resumed
                .active
                .as_ref()
                .unwrap()
                .numbering
                .as_ref()
                .unwrap()
                .stride,
            750
        );
    }

    #[test]
    #[ignore = "requires an original EPUB and its installed CREngine comparison"]
    fn follows_character_pages_in_an_original_document() {
        let path = PathBuf::from(std::env::var_os("READER_SYNTHETIC_BOOK").unwrap());
        let stride: u16 = std::env::var("READER_SYNTHETIC_STRIDE")
            .unwrap()
            .parse()
            .unwrap();
        let expected: Vec<String> = serde_json::from_slice(
            &fs::read(std::env::var_os("READER_SYNTHETIC_REFERENCE").unwrap()).unwrap(),
        )
        .unwrap();
        let f = Fixture::new();
        let mut app = Reader::open(path.parent().unwrap(), &f.state, simulation_fonts()).unwrap();
        app.open_path(&path).unwrap();
        app.activate(Action::SetCharacterPages(Some(stride)))
            .unwrap();
        finish_character_numbering(&mut app);
        assert_eq!(app.list_window().0, expected.len());
        let previous = app.active.as_ref().unwrap().reading.anchor();
        for row in [0, expected.len() / 2, expected.len() - 1] {
            let action = app.publisher_page_window(row, 1).1.remove(0).action;
            app.activate(action).unwrap();
            let active = app.active.as_ref().unwrap();
            let map = active.numbering.as_ref().unwrap();
            let (chapter, point) = map.locate(row).unwrap();
            assert_eq!(chapter, active.reading.chapter);
            if let Some(point) = point {
                let anchor = map
                    .anchor(chapter, point, &active.reading.probe.chapter)
                    .unwrap();
                assert_eq!(
                    active.reading.probe.page_for(&anchor),
                    Some(active.reading.page)
                );
            } else {
                assert_eq!(active.reading.page, 0);
            }
            assert!(
                app.current_publisher_page()
                    .is_some_and(|active_row| active_row >= row)
            );
            app.activate(Action::ReturnToLocation).unwrap();
            assert_eq!(app.active.as_ref().unwrap().reading.anchor(), previous);
        }
        println!(
            "Followed 3 stable pages and returned exactly across {} stock labels",
            expected.len()
        );
    }

    #[test]
    #[ignore = "requires an original EPUB and independent page labels"]
    fn compares_publisher_pages_in_an_original_document() {
        let original = PathBuf::from(std::env::var_os("READER_PAGE_MAP_BOOK").expect("EPUB"));
        let labels: Vec<String> = serde_json::from_slice(
            &fs::read(std::env::var_os("READER_PAGE_MAP_LABELS").expect("labels")).unwrap(),
        )
        .unwrap();
        let f = Fixture::new();
        let mut app =
            Reader::open(original.parent().unwrap(), &f.state, simulation_fonts()).unwrap();
        app.open_path(&original).unwrap();
        app.activate(Action::TogglePublisherFooter).unwrap();
        app.open_reading_edge(true, 520.);
        tap_reading_control(&mut app, "Stable page number list");
        let actual: Vec<_> = app
            .items()
            .into_iter()
            .filter(|item| matches!(item.action, Action::Jump(_)))
            .map(|item| item.title)
            .collect();
        assert_eq!(actual.len(), labels.len());
        assert!(actual == labels, "Publisher labels or order differed");
        let active = app.active.as_ref().unwrap();
        let mut by_chapter: std::collections::BTreeMap<usize, Vec<&Destination>> =
            Default::default();
        for entry in &active.reading.probe.printed_pages {
            let dest = entry
                .destination
                .as_ref()
                .expect("Missing print destination");
            let chapter = active
                .reading
                .probe
                .chapter_index(&dest.path)
                .expect("Missing print chapter");
            by_chapter.entry(chapter).or_default().push(dest);
        }
        let mut mapped = 0;
        for (chapter, entries) in by_chapter {
            let probe = active.reading.config.chapter(chapter).unwrap();
            for destination in entries {
                if let Some(id) = &destination.fragment {
                    let anchor = probe.fragment(id).expect("Unresolved printed page ID");
                    assert!(
                        probe.page_for(&anchor).is_some(),
                        "No visible geometry for print boundary"
                    );
                }
                mapped += 1;
            }
        }
        assert_eq!(mapped, labels.len());
        for n in [0, labels.len() / 2, labels.len() - 1] {
            let target = app.items().remove(n).action;
            let Action::Jump(dest) = &target else {
                panic!("Missing print destination")
            };
            let expected = dest.clone();
            app.activate(target).unwrap();
            let reading = &app.active.as_ref().unwrap().reading;
            assert_eq!(reading.probe.chapter, expected.path);
            if let Some(id) = &expected.fragment {
                let page = reading
                    .probe
                    .page_for(
                        &reading
                            .probe
                            .fragment(id)
                            .expect("Missing printed page anchor"),
                    )
                    .unwrap();
                assert_eq!(reading.page, page);
            }
            assert!(
                app.status_caption()
                    .is_some_and(|s| s.starts_with("Printed page "))
            );
            app.show(Screen::PublisherPages);
            let row = app.current_contents_row.expect("Selected print range");
            assert!(row >= n && row < labels.len());
        }
        println!(
            "Matched {} publisher labels, {} source locations and three exact jumps",
            labels.len(),
            mapped
        );
    }

    pub(super) fn write_outline_fixture(path: &Path, nav: Option<&[u8]>, ncx: Option<&[u8]>) {
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
            write!(
                first,
                r#"<h1 id="passage{index}">Passage {index}</h1><p>{lyric}</p>"#
            )
            .unwrap();
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

    pub(super) fn wait_for_search(app: &mut Reader, until_hits: usize) {
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
        let query =
            std::env::var("READER_TEST_RESULTS_QUERY").unwrap_or_else(|_| "Observation".into());
        let mut app =
            Reader::open(path.parent().unwrap(), &fixture.state, vec![font.clone()]).unwrap();
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
        let geometry = app.match_geometry();
        let last_page = (count - 1) / geometry.rows;
        let (x, y) = rectangle_center(LAST_MATCH_PAGE);
        assert_eq!(app.input(Input::Tap(x, y)), Change::Partial);
        assert_eq!(app.list_page, last_page);
        assert_eq!(app.list_window().1.len(), count - last_page * geometry.rows);
        if authored {
            capture_test_screen(
                &app,
                std::slice::from_ref(&font),
                "READER_DEEP_RESULTS",
                false,
            );
        }
        let (x, y) = rectangle_center(FIRST_MATCH_PAGE);
        assert_eq!(app.input(Input::Tap(x, y)), Change::Partial);
        assert_eq!(app.list_page, 0);
        let (x, y) = rectangle_center(LAST_MATCH_PAGE);
        app.input(Input::Tap(x, y));
        let row = geometry.row((count - 1) % geometry.rows);
        app.input(Input::Tap((LIST_X + 30) as f32, (row.y + row.h / 2) as f32));
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
        let (x, y) = (0..PAGE_HEIGHT)
            .step_by(5)
            .find_map(|y| {
                (0..WIDTH).step_by(5).find_map(|x| {
                    reading
                        .illustration_at(x as f32, y as f32)
                        .is_some_and(|p| p.ends_with("beacon.svg"))
                        .then_some((x as f32, y as f32))
                })
            })
            .unwrap();
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
            assert!(app.controls.footnote_popups);
            toggle_footnote_menu(&mut app);
            assert!(!app.controls.footnote_popups);
            toggle_footnote_menu(&mut app);
            assert!(app.controls.footnote_popups);
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
        assert!(app.controls.footnote_popups);
        toggle_footnote_menu(&mut app);
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
        let shore_page = note.test_word_page("shore");
        for _ in 0..shore_page {
            assert_eq!(app.input(Input::PageForward), Change::Partial);
        }
        let (wx, wy) = app.footnote.as_ref().unwrap().test_word("shore");
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
        let return_page = app.footnote.as_ref().unwrap().test_word_page("Return");
        for _ in 0..return_page {
            assert_eq!(app.input(Input::PageForward), Change::Partial);
        }
        let (bx, by) = app.footnote.as_ref().unwrap().test_word("Return");
        app.input(Input::Tap(bx, by));
        assert_eq!(app.screen, Screen::Book);
        let reading = &app.active.as_ref().unwrap().reading;
        assert_eq!(reading.chapter, 0);
        assert_eq!(
            position(reading).anchor,
            reading.probe.fragment("reference")
        );
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
        flows
            .toggle(
                ReadingPosition {
                    chapter: 0,
                    page: 1,
                    anchor: None,
                },
                None,
            )
            .unwrap();
        app.fixed
            .as_mut()
            .unwrap()
            .reading
            .set_hidden_flows(&flows)
            .unwrap();
        assert_eq!(app.next_comic().unwrap().path, Path::new("Volume 10.cbz"));
        flows.enabled = false;
        app.fixed
            .as_mut()
            .unwrap()
            .reading
            .set_hidden_flows(&flows)
            .unwrap();
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

    pub(super) fn tap_reading_control(app: &mut Reader, label: &str) -> Change {
        let rect = app.reading_button(label);
        app.input(Input::Tap(
            (rect.x + rect.w / 2) as f32,
            (rect.y + rect.h / 2) as f32,
        ))
    }

    fn tap_reading_config_choice(app: &mut Reader, row: &str, label: &str) -> Change {
        let rect = app.reading_button_in_row(row, label);
        app.input(Input::Tap(
            (rect.x + rect.w / 2) as f32,
            (rect.y + rect.h / 2) as f32,
        ))
    }

    #[test]
    fn fixed_tile_zoom_controls_render_apply_custom_count_and_resume_a_scan() {
        let fixture = Fixture::new();
        let path = fixture.root.join("Harbor Survey.pdf");
        let mut page = String::from(
            "BT /F1 36 Tf 42 532 Td (Harbor Survey) Tj ET\n+             BT /F1 14 Tf 43 501 Td (Approach notes for the eastern basin) Tj ET\n",
        );
        for (x, heading, notes) in [
            (
                43,
                "Approach",
                [
                    "Enter between the shore markers.",
                    "Keep the outer beacon in view.",
                    "The western channel is narrow.",
                ],
            ),
            (
                332,
                "Soundings",
                [
                    "Check the tide before crossing.",
                    "Read depths at the north jetty.",
                    "The rocks are exposed at low tide.",
                ],
            ),
            (
                621,
                "Landing",
                [
                    "Wait beside the sheltered pier.",
                    "Expect a strong ebb near the slip.",
                    "Use the lower steps at low tide.",
                ],
            ),
        ] {
            page.push_str(&format!("BT /F1 22 Tf {x} 434 Td ({heading}) Tj ET\n"));
            for (i, note) in notes.into_iter().enumerate() {
                let y = 391 - i * 29;
                page.push_str(&format!("BT /F1 13 Tf {x} {y} Td ({note}) Tj ET\n"));
            }
        }
        fs::write(
            &path,
            crate::fixed::tests::pdf_objects(&[
                "<< /Type /Catalog /Pages 2 0 R >>".into(),
                "<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 /MediaBox [0 0 900 600] >>".into(),
                "<< /Type /Page /Parent 2 0 R /Resources << /Font << /F1 6 0 R >> >> /Contents 5 0 R >>".into(),
                "<< /Type /Page /Parent 2 0 R >>".into(),
                format!("<< /Length {} >>\nstream\n{page}endstream", page.len()),
                "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),
            ]),
        )
        .unwrap();
        let fonts = vec![
            crate::test_fixture("fonts/NotoSans-Regular.ttf"),
            crate::test_fixture("fonts/NotoSans-Bold.ttf"),
        ];
        let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
        app.open_path(&path).unwrap();
        assert!(
            app.fixed
                .as_ref()
                .unwrap()
                .reading
                .preferences()
                .tile_zoom
                .is_none()
        );

        fn view_controls(app: &mut Reader) {
            app.open_reading_edge(false, WIDTH as f32 / 2.0);
            if app.reading_panel != Some(BarPanel::View) {
                assert_eq!(tap_reading_control(app, "View"), Change::Partial);
            }
        }
        view_controls(&mut app);
        assert_eq!(
            tap_reading_config_choice(&mut app, "Columns", "2"),
            Change::Partial
        );
        let reading = &app.fixed.as_ref().unwrap().reading;
        assert_eq!(
            reading.preferences().tile_zoom.unwrap().axis,
            FixedTileAxis::Columns
        );
        assert!(reading.page_status().contains("2.0 columns"));

        view_controls(&mut app);
        capture_test_screen(&app, &fonts, "READER_FIXED_TILE_ZOOM_FRAME", false);
        assert_eq!(
            tap_reading_config_choice(&mut app, "Rows", "Custom"),
            Change::Partial
        );
        assert_eq!(
            app.screen,
            Screen::NumberingValue(NumberField::FixedTileZoom(FixedTileAxis::Rows))
        );
        let layout = Layout::new(WIDTH, HEIGHT, 4, 5);
        let (_, submit) = layout
            .keyboard_keys_with(app.editor.layer, "Set")
            .into_iter()
            .find(|(key, _)| key.key == primitive_ui::Key::Go)
            .unwrap();
        let set = Input::Tap(
            (submit.x + submit.w / 2) as f32,
            (submit.y + submit.h / 2) as f32,
        );
        app.editor.text = "8.1".chars().collect();
        assert_eq!(app.input(set), Change::Partial);
        assert!(app.notice.as_deref().unwrap().contains("0.1 to 8.0"));
        app.editor.text = "2.5".chars().collect();
        assert_eq!(app.input(set), Change::Partial);
        assert_eq!(app.screen, Screen::Fixed);
        let reading = &app.fixed.as_ref().unwrap().reading;
        assert_eq!(reading.preferences().tile_zoom.unwrap().count_tenths, 25);
        assert!(reading.page_status().contains("2.5 rows"));
        app.input(Input::PageForward);
        app.tick(Instant::now() + Duration::from_secs(1));
        let saved = app.fixed.as_ref().unwrap().reading.preferences();
        let origin = app.fixed.as_ref().unwrap().reading.history_point().unwrap();
        assert_eq!(app.current_position().unwrap().page, 0);
        app.save().unwrap();

        let mut reopened = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
        reopened.open_path(&path).unwrap();
        let reading = &reopened.fixed.as_ref().unwrap().reading;
        assert_eq!(reading.preferences(), saved);
        assert!((reading.history_point().unwrap() - origin).hypot() < 0.01);
    }

    #[test]
    fn reopens_the_last_menu_of_each_document_after_restart() {
        let fixture = Fixture::new();
        let second = fixture.root.join("Second.epub");
        fs::write(
            &second,
            include_bytes!("../tests/fixtures/long-chapter.epub"),
        )
        .unwrap();
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
    fn reader_menus_expose_local_reading_workflows_without_network_services() {
        let fixture = Fixture::new();
        let mut app = fixture.app();
        app.open_path(&fixture.book).unwrap();

        for group in [TopGroup::Library, TopGroup::Tools] {
            let labels: Vec<_> = app
                .top_config_items(group)
                .into_iter()
                .map(|item| item.title)
                .collect();
            assert!(!labels.iter().any(|label| label == "Online catalogs"));
            assert!(!labels.iter().any(|label| label == "Progress sync"));
        }

        app.show(Screen::OrganizeLibrary);
        let labels: Vec<_> = app.items().into_iter().map(|item| item.title).collect();
        assert!(!labels.iter().any(|label| label == "Online catalogs"));
        assert!(labels.iter().any(|label| label == "New collection"));

        app.show(Screen::ReadingData);
        let labels: Vec<_> = app.items().into_iter().map(|item| item.title).collect();
        assert!(!labels.iter().any(|label| label == "Progress sync"));
        assert!(labels.iter().any(|label| label == "Back up reading data"));
        assert!(labels.iter().any(|label| label == "Restore a backup"));
    }

    #[test]
    fn reading_gestures_assign_through_settings_apply_and_restore_defaults() {
        let fixture = Fixture::new();
        let mut app = fixture.app();
        app.open_path(&fixture.book).unwrap();
        let tap_row = |app: &mut Reader, matches: fn(&Action) -> bool| {
            let index = app
                .items()
                .iter()
                .position(|item| matches(&item.action))
                .unwrap();
            let geometry = app.list_geometry();
            app.list_page = index / geometry.rows;
            let row = geometry.row(index % geometry.rows);
            app.input(Input::Tap(
                (row.x + row.w / 2) as f32,
                (row.y + row.h / 2) as f32,
            ));
        };
        app.input(Input::Tap(630.0, 28.0));
        tap_reading_control(&mut app, "Settings");
        tap_reading_control(&mut app, "Reading gestures");
        assert_eq!(app.screen, Screen::Gestures);
        tap_row(&mut app, |action| {
            matches!(
                action,
                Action::Show(Screen::GestureBinding(Gesture::TapTopRight))
            )
        });
        assert_eq!(app.screen, Screen::GestureBinding(Gesture::TapTopRight));
        tap_row(&mut app, |action| {
            matches!(
                action,
                Action::BindGesture(Gesture::TapTopRight, Binding::Contents)
            )
        });
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
        assert_eq!(app.screen, Screen::Skim);
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
        app.controls.legacy_both_toolbars = Some(false);
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
        assert_eq!(reopened.lookup_history.dictionary.len(), 1);
        assert_eq!(reopened.lookup_history.dictionary[0].word(), "tide");
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

    pub(super) fn capture_test_screen(
        app: &Reader,
        fonts: &[PathBuf],
        output_variable: &str,
        dark: bool,
    ) {
        if let Some(path) = std::env::var_os(output_variable) {
            let font_paths = fonts
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>();
            let mut canvas = Canvas::new(WIDTH, HEIGHT, &font_paths).unwrap();
            let pixels = primitive_app::frame(app, &mut canvas, dark);
            primitive_app::write_png(Path::new(&path), WIDTH, HEIGHT, &pixels).unwrap();
        }
    }

    #[test]
    fn image_controls_follow_night_mode_and_resume_with_the_book() {
        let fixture = Fixture::new();
        let fonts = vec![
            crate::test_fixture("fonts/NotoSans-Regular.ttf"),
            crate::test_fixture("fonts/NotoSans-Bold.ttf"),
        ];
        let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
        app.system_theme = None;
        app.open_path(&fixture.book).unwrap();
        app.open_reading_edge(false, 500.0);
        tap_reading_control(&mut app, "Rendering");
        assert_eq!(
            tap_reading_config_choice(&mut app, "Image scaling", "Fast"),
            Change::Partial
        );
        assert!(
            !app.active
                .as_ref()
                .unwrap()
                .reading
                .config
                .settings
                .smooth_images
        );
        let settings = ReadingControls {
            night_pages: true,
            ..app.controls
        };
        app.activate(Action::SetReadingControls(settings)).unwrap();
        assert_eq!(app.applied_change(), Change::Full);
        let probe = &app.active.as_ref().unwrap().reading.probe;
        let crate::Renderer::Parley(layout) = &probe.renderer else {
            unreachable!()
        };
        let image_page = (0..probe.page_count())
            .find(|&page| {
                layout
                    .visible_illustrations(probe.page_range(page), probe.settings)
                    .next()
                    .is_some()
            })
            .unwrap();
        app.follow_reflow_page(0, image_page).unwrap();
        capture_test_screen(&app, &fonts, "READER_NIGHT_IMAGE_SCREENSHOT", true);
        app.open_reading_edge(false, 500.0);
        capture_test_screen(&app, &fonts, "READER_IMAGE_CONTROLS_SCREENSHOT", true);
        assert_eq!(tap_reading_control(&mut app, "Inverted"), Change::Partial);
        assert!(
            app.active
                .as_ref()
                .unwrap()
                .reading
                .config
                .settings
                .invert_night_images
        );
        app.save().unwrap();
        let mut reopened = fixture.app();
        reopened.activate(Action::Continue).unwrap();
        let prefs = reopened.active.as_ref().unwrap().reading.config.settings;
        assert!(!prefs.smooth_images && prefs.invert_night_images);
    }

    fn show_list_item(app: &mut Reader, label: &str) -> usize {
        let index = app
            .items()
            .iter()
            .position(|item| item.title == label)
            .unwrap_or_else(|| panic!("No row {label} in {:?}", app.screen));
        let rows = if app.screen.is_contents() {
            app.contents_rows()
        } else {
            app.list_geometry().rows
        };
        let page = index / rows;
        while app.list_page < page {
            app.input(Input::PageForward);
        }
        while app.list_page > page {
            app.input(Input::PageBack);
        }
        index % rows
    }

    pub(super) fn tap_contents_item(app: &mut Reader, label: &str) {
        let offset = show_list_item(app, label);
        let (x, y) = if app.screen.is_contents() {
            let row = app.contents_row_rect(offset);
            (730.0, row.y + row.h / 2)
        } else {
            let row = app.list_geometry().row(offset);
            (160.0, row.y + row.h / 2)
        };
        assert_eq!(app.input(Input::Tap(x, y as f32)), Change::Partial);
    }

    fn expand_contents_branch(app: &mut Reader, label: &str) {
        let offset = show_list_item(app, label);
        let (button, state) = app.contents_branch_button(offset).expect("a branch");
        if state == contents_tree::Branch::Collapsed {
            let (x, y) = rectangle_center(button);
            assert_eq!(app.input(Input::Tap(x, y)), Change::Partial);
        }
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
        let index = app
            .items()
            .iter()
            .position(|row| row.title == title)
            .unwrap();
        let y = library_view::LIBRARY_LIST_Y + index as i32 * ROW_HEIGHT + 32;
        assert_eq!(app.input(Input::Tap(100.0, y as f32)), Change::Partial);
        assert_eq!(app.screen, Screen::Book);
        let initial = app.current_position().unwrap();
        let mut canvas = Canvas::new(WIDTH, HEIGHT, &[font.display().to_string()]).unwrap();
        let opening = primitive_app::frame(&app, &mut canvas, false);

        for title in ["Summit notes", "Survey notes"] {
            assert_eq!(app.input(Input::Tap(630.0, 25.0)), Change::Partial);
            if app.top_group != Some(TopGroup::Navigation) {
                tap_reading_control(&mut app, "Navigation");
            }
            assert_eq!(tap_reading_control(&mut app, "Contents"), Change::Partial);
            assert_eq!(app.screen, Screen::Contents);
            expand_contents_branch(&mut app, "Coastal field notes");
            assert!(app.items().iter().any(|i| i.title == "Elevation profile"));
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

        for title in ["Arrival", "Trail directions", "Elevation figure"] {
            app.open_reading_edge(true, 630.0);
            if app.top_group != Some(TopGroup::Navigation) {
                tap_reading_control(&mut app, "Navigation");
            }
            assert_eq!(tap_reading_control(&mut app, "Contents"), Change::Partial);
            expand_contents_branch(&mut app, "Shoreline itinerary");
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
        for (index, label) in ["Start the survey", "Reference", "Source notes"]
            .iter()
            .enumerate()
        {
            app.open_reading_edge(index != 1, 630.0);
            if index != 1 && app.top_group != Some(TopGroup::Navigation) {
                tap_reading_control(&mut app, "Navigation");
            }
            assert_eq!(tap_reading_control(&mut app, "Contents"), Change::Partial);
            expand_contents_branch(&mut app, "Reference");
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
            external
                .as_deref()
                .unwrap_or(&crate::test_fixture("many-chapters.epub")),
            &path,
        )
        .unwrap();
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
            items
                .iter()
                .position(|item| item.title == "Day 38")
                .unwrap()
        };
        let rows = app.contents_rows();
        assert!(row >= rows);
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
            assert_eq!(app.list_page, row / rows);
            let contents_frame = primitive_app::frame(&app, &mut canvas, false);
            assert!(contents_frame != reading_frame);
            if upper && external.is_none() {
                capture_test_screen(
                    &app,
                    std::slice::from_ref(&font),
                    "READER_CURRENT_CONTENTS_SCREENSHOT",
                    false,
                );
            }
            assert_eq!(app.input(Input::PageBack), Change::Partial);
            assert_eq!(app.list_page, row / rows - 1);
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
        assert_eq!(
            app.items()[app.current_contents_row.unwrap()].title,
            "Later section"
        );
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
            let kids = (3..99)
                .map(|id| format!("{id} 0 R"))
                .collect::<Vec<_>>()
                .join(" ");
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
            for (page, label) in [
                (0, "Harbor"),
                (12, "Estuary"),
                (24, "Narrows"),
                (36, "Ferry"),
                (48, "Headland"),
                (60, "Beacon"),
                (72, "Passage"),
                (84, "Inlet"),
            ] {
                custom
                    .save(
                        ReadingPosition {
                            chapter: 0,
                            page,
                            anchor: None,
                        },
                        label,
                    )
                    .unwrap();
            }
        }
        app.show(Screen::Fixed);
        if external.is_none() {
            use crate::reading_controls::StatusBar;
            app.controls.status_bar = StatusBar::Chapter;
            assert_eq!(
                app.status_caption().as_deref(),
                Some("Chapter 5 of 8 - Page 1 of 12")
            );
            let id = app.open_book_id().unwrap();
            app.library.state_mut(id).unwrap().custom_contents.enabled = false;
            assert_eq!(
                app.status_caption().as_deref(),
                Some("Chapter 1 of 1 - Page 49 of 96")
            );
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
        let toggle = app.book_map_tap_option();
        app.input(Input::Tap((toggle.x + 10) as f32, (toggle.y + 10) as f32));
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
        app.input(Input::Swipe {
            dx: -180.0,
            dy: -220.0,
            start_y: 640.0,
        });
        let cover = app.current_position().unwrap();
        let origin = app.fixed.as_ref().unwrap().reading.history_point().unwrap();
        assert!(origin.x > 0.0 && origin.y > 0.0);
        assert!(
            app.fixed
                .as_ref()
                .unwrap()
                .footer_chapters
                .iter()
                .all(|(_, title)| title != "Next plate")
        );

        assert_eq!(app.fixed.as_ref().unwrap().reading.page(), 0);
        app.input(Input::Tap(630.0, 27.0));
        if app.top_group != Some(TopGroup::Navigation) {
            tap_reading_control(&mut app, "Navigation");
        }
        tap_reading_control(&mut app, "Contents");
        assert_eq!(app.screen, Screen::FixedContents);
        expand_contents_branch(&mut app, "Northern shore");
        capture_test_screen(&app, &fonts, "READER_DJVU_CONTENTS_SCREENSHOT", false);
        let items = app.items();
        assert!(
            items.iter().any(
                |item| item.title == "Source website" && matches!(item.action, Action::Heading)
            )
        );
        assert!(items.iter().any(|item| item.title == "Unknown plate" && matches!(item.action, Action::Heading)));
        tap_contents_item(&mut app, "Unknown plate");
        assert_eq!(app.screen, Screen::FixedContents);
        assert_eq!(app.current_position(), Some(cover.clone()));
        tap_contents_item(&mut app, "Outer reef");
        let reef = app.current_position().unwrap();
        assert_eq!(app.fixed.as_ref().unwrap().reading.page(), 2);
        app.input(Input::Tap(630.0, 1420.0));
        tap_reading_control(&mut app, "Contents");
        assert_eq!(app.screen, Screen::FixedContents);
        assert_eq!(
            app.items()[app.current_contents_row.unwrap()].title,
            "Outer reef"
        );
        tap_contents_item(&mut app, "Nearest station");
        assert_eq!(app.fixed.as_ref().unwrap().reading.page(), 4);
        app.activate(Action::ReturnToLocation).unwrap();
        assert_eq!(app.current_position(), Some(reef));
        app.activate(Action::ReturnToLocation).unwrap();
        assert_eq!(app.current_position(), Some(cover));
        let restored = app.fixed.as_ref().unwrap().reading.history_point().unwrap();
        assert!((origin - restored).hypot() < 1.0);
        app.save().unwrap();
        let mut reopened = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
        reopened.activate(Action::Continue).unwrap();
        assert_eq!(reopened.fixed.as_ref().unwrap().reading.page(), 0);
        let mut captured_pan = false;
        for _ in 0..32 {
            reopened.input(Input::PageForward);
            reopened.tick(Instant::now() + Duration::from_secs(1));
            let page = reopened.fixed.as_ref().unwrap().reading.page();
            if page == 0 && !captured_pan {
                capture_test_screen(
                    &reopened,
                    &fonts,
                    "READER_FIXED_MANUAL_PAN_SCREENSHOT",
                    false,
                );
                captured_pan = true;
            }
            if page == 1 {
                break;
            }
        }
        assert!(captured_pan);
        assert_eq!(reopened.fixed.as_ref().unwrap().reading.page(), 1);
        for _ in 0..32 {
            reopened.input(Input::PageBack);
            reopened.tick(Instant::now() + Duration::from_secs(2));
            if reopened.fixed.as_ref().unwrap().reading.page() == 0 {
                break;
            }
        }
        assert_eq!(reopened.fixed.as_ref().unwrap().reading.page(), 0);
    }

    #[test]
    fn fixed_contents_uses_publisher_and_custom_page_destinations() {
        let fixture = Fixture::new();
        let path = fixture.root.join("Chart booklet.pdf");
        fs::write(
            &path,
            crate::fixed::tests::pdf_objects(&[
                "<< /Type /Catalog /Pages 2 0 R /Outlines 5 0 R >>",
                "<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>",
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 120] >>",
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 120] >>",
                "<< /Type /Outlines /First 6 0 R /Last 7 0 R >>",
                "<< /Title (First chart) /Dest [3 0 R /Fit] /Parent 5 0 R /Next 7 0 R >>",
                "<< /Title (Second chart) /Dest [4 0 R /Fit] /Parent 5 0 R /Prev 6 0 R >>",
            ]),
        )
        .unwrap();
        let mut app = fixture.app();
        app.open_path(&path).unwrap();
        app.follow_fixed(1, None).unwrap();
        let before = app.current_position().unwrap();
        app.show(Screen::FixedContents);
        assert_eq!(
            app.items()[app.current_contents_row.unwrap()].title,
            "Second chart"
        );
        assert_eq!(app.current_position(), Some(before.clone()));

        let fixed = app.fixed.as_ref().unwrap();
        let custom = &mut app
            .library
            .remember(&fixed.entry, fixed.id, &fixed.entry.title)
            .custom_contents;
        custom.save(before.clone(), "Local soundings").unwrap();
        custom.enabled = true;
        app.show(Screen::FixedContents);
        assert_eq!(
            app.items()[app.current_contents_row.unwrap()].title,
            "Local soundings"
        );
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
            let extend_highlight::Draft::Text { current: draft, .. } =
                &app.range_edit.as_ref().unwrap().draft
            else {
                panic!("expected an EPUB highlight")
            };
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
        let go = Layout::new(WIDTH, HEIGHT, 4, 5)
            .keyboard_keys_with(app.editor.layer, "Find")
            .into_iter()
            .find(|(key, _)| key.key == primitive_ui::Key::Go)
            .unwrap()
            .1;
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
            (
                0,
                "lantern",
                "The copper lantern",
                Some("Compare the Amber chart"),
            ),
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
        capture_test_screen(
            &app,
            std::slice::from_ref(&font),
            "READER_ANNOTATION_FILTERS_SCREENSHOT",
            false,
        );
        submit_annotation_search(&mut app, "amber");
        assert_eq!(app.items().len(), 1);
        tap_contents_item(&mut app, "Compare the Amber chart");
        assert_eq!(app.screen, Screen::Book);
        let reading = &app.active.as_ref().unwrap().reading;
        assert_eq!(reading.chapter, 0);
        assert_eq!(
            reading
                .probe
                .page_for(&reading.probe.fragment("lantern").unwrap()),
            Some(reading.page)
        );
        app.activate(Action::ReturnToLocation).unwrap();
        assert_eq!(app.current_position(), Some(original));

        app.input(Input::Tap(630.0, 35.0));
        tap_reading_control(&mut app, "Search");
        tap_reading_control(&mut app, "Find bookmarks");
        assert_eq!(app.editor.string(), "amber");
        assert_eq!(app.input(Input::Tap(160.0, 330.0)), Change::Partial);
        submit_annotation_search(&mut app, "amber");
        assert!(app.items().is_empty());
        let (bx, by) = rectangle_center(primitive_ui::back_button(WIDTH as i32));
        app.input(Input::Tap(bx, by));
        assert_eq!(app.screen, Screen::BookmarkFind);
        submit_annotation_search(&mut app, "Amber");
        assert_eq!(app.items().len(), 1);
        app.input(Input::Tap(bx, by));
        submit_annotation_search(&mut app, "");
        assert_eq!(app.items().len(), 3);
        capture_test_screen(
            &app,
            std::slice::from_ref(&font),
            "READER_ANNOTATION_RESULTS_SCREENSHOT",
            false,
        );
        app.input(Input::Tap(bx, by));
        app.input(Input::Tap(1050.0, 424.0)); // Exclude notes.
        submit_annotation_search(&mut app, "copper");
        assert!(app.items().is_empty()); // A highlight with a note belongs to the Notes category.
        app.input(Input::Tap(bx, by));
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
        let point = fixed
            .reading
            .visible_rectangle(0, first.bounds)
            .unwrap()
            .center();
        let before = crate::mark_time::now().unwrap();
        app.input(Input::LongTap(point.x as f32, point.y as f32));
        app.save_pdf_highlight(Some("Check the harbor".into()))
            .unwrap();
        let fixed = app.fixed.as_ref().unwrap();
        let mark = &mut app
            .library
            .remember(&fixed.entry, id, &fixed.entry.title)
            .pdf_highlights[0];
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
                updated_at: None,
                chapter_title: None,
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
        let (x, y) = rectangle_center(primitive_ui::back_button(WIDTH as i32));
        app.input(Input::Tap(x, y));
        assert_eq!(app.screen, Screen::FixedBookmarks);
    }

    fn save_chapter_title(app: &mut Reader, title: &str) {
        assert_eq!(app.screen, Screen::CustomTitle);
        app.editor.text = title.chars().collect();
        let go = Layout::new(WIDTH, HEIGHT, 4, 5)
            .keyboard_keys_with(app.editor.layer, "Save")
            .into_iter()
            .find(|(key, _)| key.key == primitive_ui::Key::Go)
            .unwrap()
            .1;
        let (x, y) = rectangle_center(go);
        app.input(Input::Tap(x, y));
    }

    #[test]
    fn custom_contents_edits_anchored_bookmarks_and_preserves_publisher_contents() {
        let fixture = Fixture::new();
        let fonts = simulation_fonts();
        let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
        app.open_path(&fixture.book).unwrap();
        app.activate(Action::Jump(Destination {
            path: "EPUB/chapters/dusk.xhtml".into(),
            fragment: Some("door".into()),
        }))
        .unwrap();
        let destination = app.current_position().unwrap();
        app.activate(Action::AddBookmark).unwrap();
        app.activate(Action::Jump(Destination {
            path: "EPUB/chapters/shore.xhtml".into(),
            fragment: Some("tide-b".into()),
        }))
        .unwrap();
        let origin = app.current_position().unwrap();
        app.show(Screen::Contents);
        tap_contents_item(&mut app, "Custom contents");
        tap_contents_item(&mut app, "Add from bookmark");
        let label = app.items()[0].title.clone();
        tap_contents_item(&mut app, &label);
        save_chapter_title(&mut app, "Return after dark");
        tap_contents_item(&mut app, "Add current location");
        app.input(Input::PageBack);
        assert_eq!(
            app.library
                .state(app.open_book_id().unwrap())
                .unwrap()
                .custom_contents
                .entries()
                .len(),
            1
        );
        tap_contents_item(&mut app, "Add current location");
        save_chapter_title(&mut app, "Tide observation");
        tap_contents_item(&mut app, "Use custom contents");
        assert_eq!(
            app.items().iter().map(|i| &*i.title).collect::<Vec<_>>(),
            [
                "Manage custom contents",
                "Tide observation",
                "Return after dark"
            ]
        );
        capture_test_screen(&app, &fonts, "READER_CUSTOM_CONTENTS_SCREENSHOT", false);
        tap_contents_item(&mut app, "Return after dark");
        assert_eq!(app.current_position(), Some(destination.clone()));
        app.activate(Action::ReturnToLocation).unwrap();
        assert_eq!(app.current_position(), Some(origin.clone()));
        app.activate(Action::Text(Settings {
            font_size: 76.0,
            ..device_settings()
        }))
        .unwrap();
        app.save().unwrap();
        let mut reopened = fixture.app();
        reopened.activate(Action::Continue).unwrap();
        reopened.show(Screen::Contents);
        tap_contents_item(&mut reopened, "Return after dark");
        assert_eq!(
            reopened.current_position().unwrap().anchor,
            destination.anchor
        );
        reopened.show(Screen::Contents);
        tap_contents_item(&mut reopened, "Manage custom contents");
        tap_contents_item(&mut reopened, "Edit chapters");
        tap_contents_item(&mut reopened, "Return after dark");
        save_chapter_title(&mut reopened, "Dusk");
        tap_contents_item(&mut reopened, "Dusk");
        reopened.input(Input::Tap(100.0, (LIST_Y + 266) as f32));
        reopened.input(Input::PageBack);
        reopened.input(Input::PageBack);
        assert!(reopened.items().iter().any(|item| item.title == "Dusk"));
        tap_contents_item(&mut reopened, "Dusk");
        reopened.input(Input::Tap(100.0, (LIST_Y + 266) as f32));
        reopened.input(Input::Tap(800.0, 832.0));
        assert_eq!(
            reopened
                .library
                .state(reopened.open_book_id().unwrap())
                .unwrap()
                .custom_contents
                .entries()
                .len(),
            1
        );
        reopened.show(Screen::CustomContents);
        tap_contents_item(&mut reopened, "Use publisher contents");
        assert!(
            reopened
                .items()
                .iter()
                .any(|item| item.title == "Returning at Dusk")
        );
        assert_eq!(
            reopened
                .library
                .state(reopened.open_book_id().unwrap())
                .unwrap()
                .custom_contents
                .entries()
                .len(),
            1
        );
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
        assert!(
            app.library
                .state(app.open_book_id().unwrap())
                .unwrap()
                .custom_contents
                .entries()[0]
                .position
                .anchor
                .as_ref()
                == Some(&start)
        );
        app.show(Screen::Contents);
        tap_contents_item(&mut app, "Second tide");
        assert_eq!(app.current_position().unwrap().anchor, Some(start));
    }

    #[test]
    fn custom_contents_adds_and_follows_fixed_pages_without_a_publisher_outline() {
        let fixture = Fixture::new();
        let mut app = fixture.app();
        let path = fixture.root.join("No outline.pdf");
        fs::write(
            &path,
            crate::fixed::tests::pdf_objects(&[
                "<< /Type /Catalog /Pages 2 0 R >>",
                "<< /Type /Pages /Count 2 /Kids [3 0 R 4 0 R] >>",
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 120] >>",
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 120] >>",
            ]),
        )
        .unwrap();
        app.open_path(&path).unwrap();
        assert!(app.fixed.as_ref().unwrap().reading.outline().is_empty());
        app.follow_fixed(1, None).unwrap();
        app.open_reading_edge(false, 500.0);
        tap_reading_control(&mut app, "Contents");
        tap_contents_item(&mut app, "Custom contents");
        tap_contents_item(&mut app, "Add current location");
        save_chapter_title(&mut app, "Second page");
        tap_contents_item(&mut app, "Use custom contents");
        app.follow_fixed(0, None).unwrap();
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
    fn runtime_forwards_reading_orientation_and_defers_to_the_launcher_theme() {
        use crate::reading_controls::ScreenPalette;
        use primitive_app::Rotation;

        let fixture = Fixture::new();
        let mut runtime = ReaderRuntime(fixture.app());
        runtime.0.system_theme = None;
        runtime.0.open_path(&fixture.book).unwrap();
        let before = runtime.0.current_position().unwrap();
        runtime
            .0
            .activate(Action::SetReadingControls(ReadingControls {
                palette: ScreenPalette::Dark,
                ..runtime.0.controls
            }))
            .unwrap();

        for (orientation, expected) in [
            (ReadingOrientation::FollowLauncher, None),
            (ReadingOrientation::Upright, Some(Rotation::Upright)),
            (ReadingOrientation::UpsideDown, Some(Rotation::UpsideDown)),
        ] {
            runtime
                .0
                .activate(Action::SetReadingOrientation(orientation))
                .unwrap();
            assert_eq!(runtime.preferred_rotation(), expected);
        }
        assert_eq!(runtime.preferred_dark_mode(), Some(true));
        let settings = fixture.root.join("launcher-settings.txt");
        fs::write(&settings, "dark_mode=off\n").unwrap();
        runtime.0.system_theme = Some(SystemTheme::new(settings));
        let run = fixture.root.join("run");
        let scoped_dir = run.join("app-turn.ABC123");
        fs::create_dir_all(&scoped_dir).unwrap();
        let scoped = scoped_dir.join("turn");
        let legacy = run.join("orientation");
        fs::write(&legacy, "U").unwrap();
        let lost = run.join("app-turn.DEF456/turn");
        for (offered, expected) in [
            (None, None),
            (Some(legacy.as_path()), None),
            (Some(lost.as_path()), None),
            (Some(scoped.as_path()), Some(Rotation::UpsideDown)),
        ] {
            runtime.0.launcher_orientation_handoff =
                has_launcher_orientation_handoff(&run, offered);
            assert_eq!(runtime.preferred_rotation(), expected);
            assert_eq!(runtime.preferred_dark_mode(), None);
            assert!(!runtime.0.uses_night_pages());
            assert_eq!(
                runtime.0.reading_orientation(),
                ReadingOrientation::UpsideDown
            );
            assert_eq!(runtime.0.current_position().unwrap(), before);
        }
    }

    #[test]
    fn runtime_inverts_hardware_page_buttons_without_touching_reader_generated_turns() {
        let fixture = Fixture::new();
        let long = fixture.root.join("Long Chapter.epub");
        fs::write(&long, include_bytes!("../tests/fixtures/long-chapter.epub")).unwrap();
        let mut reader = fixture.app();
        reader.open_path(&long).unwrap();
        reader.controls.invert_page_turn_buttons = true;
        let mut runtime = ReaderRuntime(reader);

        // Swap the runtime's physical PageBack input to reading-forward, then
        // confirm the matching key returns to the previous page.
        runtime.input(Input::PageBack);
        let due = runtime
            .0
            .active
            .as_ref()
            .unwrap()
            .reading
            .deadline()
            .unwrap();
        runtime.tick(due + Duration::from_secs(1));
        assert_eq!(runtime.0.active.as_ref().unwrap().reading.page, 1);

        runtime.input(Input::PageForward);
        let due = runtime
            .0
            .active
            .as_ref()
            .unwrap()
            .reading
            .deadline()
            .unwrap();
        runtime.tick(due + Duration::from_secs(1));
        assert_eq!(runtime.0.active.as_ref().unwrap().reading.page, 0);

        runtime
            .0
            .gestures
            .set(Gesture::TapTopRight, Binding::Next)
            .unwrap();
        runtime.input(Input::Tap(WIDTH as f32 - 20.0, 20.0));
        let due = runtime
            .0
            .active
            .as_ref()
            .unwrap()
            .reading
            .deadline()
            .unwrap();
        runtime.tick(due + Duration::from_secs(1));
        assert_eq!(runtime.0.active.as_ref().unwrap().reading.page, 1);
    }

    #[test]
    fn page_turn_inversion_setting_and_footer_item_match_stock_and_persist() {
        use crate::reading_controls::{FooterItem, FooterSymbolStyle, StatusBar};

        let fixture = Fixture::new();
        let screenshot_fonts = vec![
            crate::test_fixture("fonts/NotoSans-Regular.ttf"),
            crate::test_fixture("fonts/NotoSans-Bold.ttf"),
        ];
        let mut app =
            Reader::open(&fixture.root, &fixture.state, screenshot_fonts.clone()).unwrap();
        app.controls.dictionary_on_single_word = Some(false);
        app.open_path(&fixture.book).unwrap();
        let setting = app
            .reading_settings_items()
            .into_iter()
            .find(|item| item.title == "Invert page-turn buttons")
            .expect("KOReader physical-button preference is available");
        assert_eq!(
            setting.detail.as_deref(),
            Some("Standard physical button directions")
        );
        app.show(Screen::ReadingControls);
        capture_test_screen(
            &app,
            &screenshot_fonts,
            "READER_PAGE_TURN_INVERSION_SETTINGS_SCREENSHOT",
            false,
        );
        capture_test_screen(
            &app,
            &screenshot_fonts,
            "READER_PAGE_TURN_INVERSION_SETTINGS_DARK_SCREENSHOT",
            true,
        );

        app.activate(setting.action).unwrap();
        assert!(app.controls.invert_page_turn_buttons);
        assert!(ReadingControls::load(&app.controls_path, true).invert_page_turn_buttons);
        assert_eq!(
            app.reading_settings_items()
                .into_iter()
                .find(|item| item.title == "Invert page-turn buttons")
                .and_then(|item| item.detail),
            Some("Previous and next buttons are swapped".to_owned())
        );

        app.activate(Action::SetReadingControls(ReadingControls {
            status_bar: StatusBar::Custom,
            footer_items: FooterItem::PageTurningInverted.bit(),
            footer_symbol_style: FooterSymbolStyle::Icons,
            ..app.controls
        }))
        .unwrap();
        app.show(Screen::Book);
        assert!(app.custom_footer_icons_enabled());
        capture_test_screen(
            &app,
            &screenshot_fonts,
            "READER_PAGE_TURN_INVERSION_FOOTER_SCREENSHOT",
            false,
        );
        app.controls.night_pages = true;
        capture_test_screen(
            &app,
            &screenshot_fonts,
            "READER_PAGE_TURN_INVERSION_FOOTER_DARK_SCREENSHOT",
            true,
        );
        app.controls.night_pages = false;

        app.activate(Action::SetReadingControls(ReadingControls {
            footer_symbol_style: FooterSymbolStyle::Letters,
            ..app.controls
        }))
        .unwrap();
        assert_eq!(app.status_caption().as_deref(), Some("Pg: On"));
        app.activate(Action::SetReadingControls(ReadingControls {
            invert_page_turn_buttons: false,
            ..app.controls
        }))
        .unwrap();
        assert_eq!(app.status_caption().as_deref(), Some("Pg: Off"));

        app.activate(Action::SetReadingControls(ReadingControls {
            footer_symbol_style: FooterSymbolStyle::Compact,
            ..app.controls
        }))
        .unwrap();
        assert!(app.status_caption().unwrap().contains("⇉"));
    }

    #[test]
    fn epub_footer_cycles_persists_and_keeps_the_anchor_when_skimming() {
        use crate::reading_controls::{
            FooterItem, FooterSeparator, FooterSymbolStyle, FooterTrackPosition, StatusBar,
        };

        let fixture = Fixture::new();
        let mut app = fixture.app();
        app.open_path(&fixture.book).unwrap();
        assert_eq!(app.controls.status_bar, StatusBar::Chapter);
        assert_eq!(
            app.controls.footer_track_position,
            FooterTrackPosition::Below
        );
        let caption = app.status_caption().unwrap();
        assert!(caption.starts_with("Chapter 1 of "));
        assert!(caption.contains(" - Page 1 of "));
        app.activate(Action::SetReadingControls(ReadingControls {
            footer_separator: FooterSeparator::Bullet,
            ..app.controls
        }))
        .unwrap();
        let caption = app.status_caption().unwrap();
        assert!(caption.contains(" - Page 1 of "));
        assert!(!caption.contains(" • Page 1 of "));
        let position_before = position(&app.active.as_ref().unwrap().reading);
        let tap = Input::Tap(250.0, HEIGHT as f32 - 32.0);
        let hold = Input::LongTap(250.0, HEIGHT as f32 - 32.0);
        assert_eq!(app.input(hold), Change::None);

        for mode in [
            StatusBar::Remaining,
            StatusBar::ChapterTime,
            StatusBar::BookTime,
            StatusBar::Custom,
            StatusBar::Minimal,
            StatusBar::PageNumbers,
            StatusBar::Progress,
            StatusBar::Chapter,
        ] {
            assert_eq!(app.input(tap), Change::Partial);
            assert_eq!(app.screen, Screen::Book);
            assert_eq!(app.controls.status_bar, mode);
            assert_eq!(
                ReadingControls::load(&app.controls_path, true).status_bar,
                mode
            );
            match mode {
                StatusBar::Progress => {
                    assert!(app.status_caption().unwrap().ends_with("% of book"))
                }
                StatusBar::Chapter => assert!(app.status_caption().unwrap().contains("Page 1 of")),
                StatusBar::Remaining => assert!(app.status_caption().unwrap().ends_with("left")),
                StatusBar::ChapterTime => {
                    assert!(app.status_caption().unwrap().contains("in chapter"))
                }
                StatusBar::BookTime => assert!(app.status_caption().unwrap().contains("in book")),
                StatusBar::Custom => {
                    app.activate(Action::ToggleBookmark).unwrap();
                    let footer_order = (0..9).fold(Default::default(), |order, _| {
                        FooterItem::move_order(order, FooterItem::BookmarkCount, -1)
                    });
                    app.activate(Action::SetReadingControls(ReadingControls {
                        footer_items: app.controls.footer_items | FooterItem::BookmarkCount.bit(),
                        footer_order,
                        footer_separator: FooterSeparator::Bullet,
                        footer_pages_left_include_current: true,
                        footer_percentage_digits: 2,
                        ..app.controls
                    }))
                    .unwrap();
                    let caption = app.status_caption().unwrap();
                    assert!(caption.contains("Page 1"));
                    assert!(caption.contains("."));
                    assert!(caption.contains(" • "));
                    assert!(caption.contains("left in chapter"));
                    assert!(caption.contains("1 bookmarks"));
                    assert!(caption.starts_with("1 bookmarks • Page 1"));
                    assert!(app.custom_footer_icons_enabled());
                    assert_eq!(
                        app.controls.footer_items & FooterItem::DEFAULT_MASK,
                        FooterItem::DEFAULT_MASK
                    );
                    // Bookmarking raises a transient notice that would cover
                    // the status caption in the screenshot.
                    app.notice = None;
                    capture_test_screen(
                        &app,
                        &simulation_fonts(),
                        "READER_CUSTOM_FOOTER_STATUS_SCREENSHOT",
                        false,
                    );
                    app.show(Screen::FooterSettings);
                    let items = app.items();
                    assert_eq!(
                        items
                            .iter()
                            .find(|item| item.title == "Status item separator")
                            .and_then(|item| item.detail.as_deref()),
                        Some("Bullet (•)")
                    );
                    assert_eq!(
                        items
                            .iter()
                            .find(|item| item.title == "Status item symbols")
                            .and_then(|item| item.detail.as_deref()),
                        Some("Icons")
                    );
                    assert_eq!(
                        items
                            .iter()
                            .find(|item| item.title == "Pages left includes current page")
                            .and_then(|item| item.detail.as_deref()),
                        Some("On")
                    );
                    assert_eq!(
                        items
                            .iter()
                            .find(|item| item.title == "Progress percentage digits")
                            .and_then(|item| item.detail.as_deref()),
                        Some("2 decimal places")
                    );
                    let symbols = app
                        .items()
                        .into_iter()
                        .find(|item| item.title == "Status item symbols")
                        .expect("custom footer exposes KOReader symbol styles");
                    app.activate(symbols.action).unwrap();
                    assert_eq!(app.controls.footer_symbol_style, FooterSymbolStyle::Letters);
                    assert!(!app.custom_footer_icons_enabled());
                    assert!(
                        app.status_caption()
                            .unwrap()
                            .starts_with("BM: 1 bookmarks • P: Page 1")
                    );
                    let symbols = app
                        .items()
                        .into_iter()
                        .find(|item| item.title == "Status item symbols")
                        .unwrap();
                    app.activate(symbols.action).unwrap();
                    assert_eq!(app.controls.footer_symbol_style, FooterSymbolStyle::Compact);
                    assert!(!app.custom_footer_icons_enabled());
                    let symbols = app
                        .items()
                        .into_iter()
                        .find(|item| item.title == "Status item symbols")
                        .unwrap();
                    app.activate(symbols.action).unwrap();
                    assert_eq!(app.controls.footer_symbol_style, FooterSymbolStyle::Icons);
                    assert!(app.custom_footer_icons_enabled());
                    let move_down = app
                        .footer_items()
                        .into_iter()
                        .find(|item| item.title == "Move down: Bookmark count")
                        .expect("custom footer exposes saved item order controls");
                    app.activate(move_down.action).unwrap();
                    let caption = app.status_caption().unwrap();
                    assert!(caption.starts_with("Page 1"));
                    assert!(caption.find("Page 1").unwrap() < caption.find("1 bookmarks").unwrap());
                    capture_test_screen(
                        &app,
                        &simulation_fonts(),
                        "READER_CUSTOM_FOOTER_SETTINGS_SCREENSHOT",
                        false,
                    );
                    let item = app
                        .items()
                        .into_iter()
                        .find(|item| item.title == "Pages left in chapter")
                        .expect("custom footer exposes its selected items");
                    app.activate(item.action).unwrap();
                    let caption = app.status_caption().unwrap();
                    assert!(caption.contains("Page 1"));
                    assert!(!caption.contains("left in chapter"));
                    app.show(Screen::Book);
                }
                StatusBar::Minimal => assert_eq!(app.status_caption().unwrap(), ""),
                StatusBar::PageNumbers => assert!(app.status_caption().is_none()),
            }
        }
        app.activate(Action::SetReadingControls(ReadingControls {
            skim_on_footer_hold: true,
            footer_all_at_once: true,
            ..app.controls
        }))
        .unwrap();
        assert!(app.status_caption().unwrap().contains("left"));
        app.input(tap);
        assert_eq!(app.controls.status_bar, StatusBar::Minimal);
        assert_eq!(app.input(hold), Change::None);
        app.input(tap);
        assert_eq!(app.input(hold), Change::Partial);
        assert_eq!(app.screen, Screen::Skim);
        assert_eq!(
            position(&app.active.as_ref().unwrap().reading),
            position_before
        );

        app.show(Screen::Book);
        app.controls.footer_tap_locked = true;
        assert_eq!(app.input(tap), Change::None);
        app.activate(Action::LockTouch).unwrap();
        assert_eq!(app.input(hold), Change::None);
        assert_eq!(app.screen, Screen::Book);
        assert_eq!(
            app.input(Input::Tap(DONE.x as f32 + 20.0, DONE.y as f32 + 20.0)),
            Change::Exit
        );
    }

    #[test]
    fn selected_wikipedia_and_translation_preview_keep_the_reading_anchor_and_make_no_request() {
        let fixture = Fixture::new();
        let mut app = fixture.app();
        app.open_path(&fixture.book).unwrap();
        let before = position(&app.active.as_ref().unwrap().reading);
        let reading = &app.active.as_ref().unwrap().reading;
        let selected = reading.probe.matches("first").into_iter().next().unwrap();
        let mut end = selected.clone();
        end.offset += 5;
        app.selection = reading.probe.select_range(&selected, &end);
        for action in [
            selection_popup::SelectionAction::Wikipedia,
            selection_popup::SelectionAction::Translate,
        ] {
            let button = app.selection_button(action);
            assert_eq!(
                app.input(Input::Tap(
                    (button.x + button.w / 2) as f32,
                    (button.y + button.h / 2) as f32
                )),
                Change::Partial
            );
            assert_eq!(app.screen, Screen::OnlineLookup);
            assert!(app.online.as_ref().unwrap().job.is_none());
            assert_eq!(app.online.as_ref().unwrap().request.text, "first");
            assert!(app.lookup_history.online.is_empty());
            app.input(Input::Tap(1090.0, 366.0));
            assert_eq!(app.screen, Screen::Book);
            assert_eq!(app.selection.as_ref().unwrap().text, "first");
            assert_eq!(position(&app.active.as_ref().unwrap().reading), before);
        }
        app.show(Screen::BookTools);
        app.activate(Action::TranslatePage).unwrap();
        assert!(app.online.as_ref().unwrap().request.text.contains("first"));
        assert!(app.online.as_ref().unwrap().job.is_none());
        app.input(Input::Tap(1090.0, 366.0));
        app.activate(Action::Wikipedia).unwrap();
        assert_eq!(app.screen, Screen::OnlineEdit);
        app.input(Input::PageBack);
        assert_eq!(app.screen, Screen::BookTools);
    }

    #[test]
    fn custom_footer_clock_uses_local_time_and_reschedules_only_when_enabled() {
        use crate::reading_controls::{FooterItem, StatusBar};
        use std::time::Duration;

        let fixture = Fixture::new();
        let fonts = vec![
            crate::test_fixture("fonts/NotoSans-Regular.ttf"),
            crate::test_fixture("fonts/Silkscreen-Regular.ttf"),
        ];
        let mut app = fixture.app();
        app.fonts = fonts.clone();
        app.open_path(Path::new("Field Notes.epub")).unwrap();
        app.activate(Action::SetReadingControls(ReadingControls {
            status_bar: StatusBar::Custom,
            footer_items: ReadingControls::default().footer_items | FooterItem::CurrentTime.bit(),
            ..app.controls
        }))
        .unwrap();
        let caption = app.status_caption().unwrap();
        let time = caption
            .split("Time ")
            .nth(1)
            .expect("the selected current-time item is shown");
        assert!(time.as_bytes()[0].is_ascii_digit());
        assert!(time.as_bytes()[1].is_ascii_digit());
        assert_eq!(time.as_bytes()[2], b':');
        assert!(time.as_bytes()[3].is_ascii_digit());
        assert!(time.as_bytes()[4].is_ascii_digit());
        capture_test_screen(&app, &fonts, "READER_CLOCK_FOOTER_SCREENSHOT", false);

        let no_refresh = app.footer_items().into_iter().find(|item| {
            item.title == "Auto refresh items" && item.detail.as_deref() == Some("Off")
        });
        assert!(no_refresh.is_some());
        assert!(app.footer_refresh_at.is_none());
        app.activate(Action::SetReadingControls(ReadingControls {
            footer_auto_refresh: true,
            ..app.controls
        }))
        .unwrap();
        assert!(app.footer_refresh_at.is_some());
        let first = app.footer_refresh_at.unwrap();
        assert!(first.saturating_duration_since(Instant::now()) <= Duration::from_secs(61));
        assert!(first > Instant::now());
        assert_eq!(app.tick(first + Duration::from_millis(1)), Change::Partial);
        assert!(app.footer_refresh_at.unwrap() > first);

        app.input(Input::Sleep);
        assert!(app.footer_refresh_at.is_none());
        app.input(Input::Wake);
        assert!(app.footer_refresh_at.is_some());

        app.activate(Action::SetReadingControls(ReadingControls {
            footer_items: app.controls.footer_items & !FooterItem::CurrentTime.bit(),
            ..app.controls
        }))
        .unwrap();
        assert!(app.footer_refresh_at.is_none());

        use crate::reading_controls::TopHeader;
        app.activate(Action::TopStatusHeader(true)).unwrap();
        assert!(app.footer_refresh_at.is_some());
        app.activate(Action::SetReadingControls(ReadingControls {
            top_header: TopHeader {
                auto_refresh: false,
                ..app.controls.top_header
            },
            ..app.controls
        }))
        .unwrap();
        assert!(app.footer_refresh_at.is_none());
        app.activate(Action::SetReadingControls(ReadingControls {
            top_header: TopHeader {
                auto_refresh: true,
                clock: false,
                battery: crate::reading_controls::HeaderBattery::Off,
                ..app.controls.top_header
            },
            ..app.controls
        }))
        .unwrap();
        assert!(app.footer_refresh_at.is_none());
        app.activate(Action::SetReadingControls(ReadingControls {
            top_header: TopHeader {
                battery: crate::reading_controls::HeaderBattery::Icon,
                ..app.controls.top_header
            },
            ..app.controls
        }))
        .unwrap();
        assert!(app.footer_refresh_at.is_some());
        app.activate(Action::SetReadingControls(ReadingControls {
            top_header: TopHeader {
                battery: crate::reading_controls::HeaderBattery::Off,
                clock: true,
                ..app.controls.top_header
            },
            ..app.controls
        }))
        .unwrap();
        assert!(app.footer_refresh_at.is_some());
        app.input(Input::Sleep);
        assert!(app.footer_refresh_at.is_none());
        app.input(Input::Wake);
        assert!(app.footer_refresh_at.is_some());

        app.activate(Action::TopStatusHeader(false)).unwrap();
        app.activate(Action::SetReadingControls(ReadingControls {
            status_bar: StatusBar::Custom,
            footer_auto_refresh: false,
            footer_items: FooterItem::DEFAULT_MASK | FooterItem::BatteryPercentage.bit(),
            ..app.controls
        }))
        .unwrap();
        assert!(app.footer_refresh_at.is_none());
        let battery_setting = app.footer_items().into_iter().find(|item| {
            item.title == "Battery percentage" && item.detail.as_deref() == Some("On")
        });
        assert!(battery_setting.is_some());
        app.show(Screen::FooterSettings);
        let battery_row = app
            .items()
            .iter()
            .position(|item| item.title == "Battery percentage")
            .expect("battery percentage appears in custom footer items");
        app.list_page = battery_row / ROWS_PER_PAGE;
        capture_test_screen(
            &app,
            &fonts,
            "READER_BATTERY_FOOTER_SETTINGS_SCREENSHOT",
            false,
        );
        app.show(Screen::Book);
        app.activate(Action::SetReadingControls(ReadingControls {
            footer_auto_refresh: true,
            ..app.controls
        }))
        .unwrap();
        assert!(app.footer_refresh_at.is_some());
        app.input(Input::Sleep);
        assert!(app.footer_refresh_at.is_none());
        app.input(Input::Wake);
        assert!(app.footer_refresh_at.is_some());
        app.activate(Action::SetReadingControls(ReadingControls {
            footer_items: app.controls.footer_items & !FooterItem::BatteryPercentage.bit(),
            ..app.controls
        }))
        .unwrap();
        assert!(app.footer_refresh_at.is_none());
    }

    #[test]
    fn custom_footer_text_edits_repeats_renders_and_persists() {
        use crate::reading_controls::{FooterItem, StatusBar};

        let fixture = Fixture::new();
        let fonts = vec![
            crate::test_fixture("fonts/NotoSans-Regular.ttf"),
            crate::test_fixture("fonts/NotoSans-Bold.ttf"),
            crate::test_symbol_font(),
        ];
        let mut app = fixture.app();
        app.fonts = fonts.clone();
        app.open_path(&fixture.book).unwrap();
        app.activate(Action::SetReadingControls(ReadingControls {
            status_bar: StatusBar::Custom,
            footer_items: FooterItem::CustomText.bit(),
            ..app.controls
        }))
        .unwrap();

        app.show(Screen::FooterSettings);
        let settings = app.items();
        assert!(settings.iter().any(|item| item.title == "Edit custom text"));
        assert!(
            settings
                .iter()
                .any(|item| item.title == "Number of repetitions")
        );
        assert!(
            settings
                .iter()
                .any(|item| item.title == "Custom text tokens")
        );
        assert!(settings.iter().any(|item| {
            item.title == "Edit custom text"
                && matches!(&item.action, Action::Show(Screen::FooterCustomText))
        }));
        app.activate(Action::Show(Screen::FooterCustomText))
            .unwrap();
        assert_eq!(app.screen, Screen::FooterCustomText);
        assert_eq!(app.editor.string(), "KOReader");
        capture_test_screen(
            &app,
            &fonts,
            "READER_CUSTOM_FOOTER_EDITOR_SCREENSHOT",
            false,
        );
        app.editor.text = "%f %c/%t".chars().collect();
        app.editor.caret = app.editor.text.len();
        app.save_footer_custom_text().unwrap();
        assert_eq!(
            footer_custom_text::load(&app.footer_custom_text_path).text,
            "%f %c/%t"
        );

        let repetitions = NumberField::FooterCustomTextRepetitions;
        assert!(app.items().iter().any(|item| {
            item.title == "Number of repetitions"
                && matches!(
                    &item.action,
                    Action::Show(Screen::NumberingValue(
                        NumberField::FooterCustomTextRepetitions
                    ))
                )
        }));
        app.activate(Action::Show(Screen::NumberingValue(repetitions)))
            .unwrap();
        assert_eq!(app.screen, Screen::NumberingValue(repetitions));
        app.editor.text = "2".chars().collect();
        app.editor.caret = app.editor.text.len();
        app.apply_numbering_input(repetitions).unwrap();
        assert_eq!(app.footer_custom_text.repetitions, 2);
        assert_eq!(
            app.items()
                .into_iter()
                .find(|item| item.title == "Number of repetitions")
                .and_then(|item| item.detail),
            Some("2 · 1–99".to_owned())
        );
        let settings = app.items();
        app.list_page = settings
            .iter()
            .position(|item| item.title == "Edit custom text")
            .unwrap()
            / ROWS_PER_PAGE;
        capture_test_screen(
            &app,
            &fonts,
            "READER_CUSTOM_FOOTER_SETTINGS_SCREENSHOT",
            false,
        );
        app.list_page = settings
            .iter()
            .position(|item| item.title == "Number of repetitions")
            .unwrap()
            / ROWS_PER_PAGE;
        capture_test_screen(
            &app,
            &fonts,
            "READER_CUSTOM_FOOTER_SETTINGS_MORE_SCREENSHOT",
            false,
        );

        app.show(Screen::Book);
        let expected = "Field Notes.epub 1/1Field Notes.epub 1/1";
        assert_eq!(app.status_caption().as_deref(), Some(expected));
        capture_test_screen(&app, &fonts, "READER_CUSTOM_FOOTER_SCREENSHOT", false);

        let mut reopened = fixture.app();
        reopened.fonts = fonts;
        reopened.open_path(&fixture.book).unwrap();
        assert_eq!(reopened.footer_custom_text.text, "%f %c/%t");
        assert_eq!(reopened.footer_custom_text.repetitions, 2);
        assert_eq!(reopened.status_caption().as_deref(), Some(expected));
    }

    #[test]
    fn battery_footer_threshold_settings_persist_and_bound_hidden_level() {
        use crate::reading_controls::{FooterItem, StatusBar};

        let fixture = Fixture::new();
        let mut app = fixture.app();
        app.open_path(&fixture.book).unwrap();
        app.activate(Action::SetReadingControls(ReadingControls {
            status_bar: StatusBar::Custom,
            footer_items: ReadingControls::default().footer_items
                | FooterItem::BatteryPercentage.bit(),
            ..app.controls
        }))
        .unwrap();

        app.show(Screen::FooterSettings);
        let threshold = app
            .items()
            .into_iter()
            .find(|item| item.title == "Hide battery when above")
            .expect("the battery item exposes its threshold");
        assert_eq!(threshold.detail.as_deref(), Some("100% · 0–100%"));

        let field = NumberField::FooterBatteryHideThreshold;
        app.show(Screen::NumberingValue(field));
        assert_eq!(app.editor.string(), "100");
        app.editor.text = "80".chars().collect();
        app.editor.caret = app.editor.text.len();
        app.apply_numbering_input(field).unwrap();
        assert_eq!(app.controls.footer_battery_hide_threshold, Some(80));
        assert_eq!(
            ReadingControls::load(&app.controls_path, true).footer_battery_hide_threshold,
            Some(80)
        );

        app.show(Screen::NumberingValue(field));
        app.editor.text = "101".chars().collect();
        app.editor.caret = app.editor.text.len();
        assert!(app.apply_numbering_input(field).is_err());
        assert_eq!(app.controls.footer_battery_hide_threshold, Some(80));

        app.show(Screen::FooterSettings);
        assert_eq!(
            app.items()
                .into_iter()
                .find(|item| item.title == "Hide battery when above")
                .and_then(|item| item.detail),
            Some("80% · 0–100%".to_owned())
        );
        app.activate(Action::SetReadingControls(ReadingControls {
            footer_items: app.controls.footer_items & !FooterItem::BatteryPercentage.bit(),
            ..app.controls
        }))
        .unwrap();
        assert!(
            app.items()
                .iter()
                .all(|item| item.title != "Hide battery when above")
        );
    }

    #[test]
    fn footer_item_font_size_matches_stock_range_persists_and_draws_for_both_page_types() {
        let fixture = Fixture::new();
        let fonts = vec![
            crate::test_fixture("fonts/NotoSans-Regular.ttf"),
            crate::test_fixture("fonts/Silkscreen-Regular.ttf"),
        ];
        let mut app = fixture.app();
        app.fonts = fonts.clone();
        app.open_path(&fixture.book).unwrap();
        assert_eq!(app.controls.footer_item_font_size, None);
        assert_eq!(app.footer_caption_font_px(), 28);

        app.show(Screen::FooterSettings);
        let setting = app
            .items()
            .into_iter()
            .find(|item| item.title == "Item font size")
            .expect("the footer settings expose adjustable item text size");
        assert_eq!(setting.detail.as_deref(), Some("14 · 8–36"));
        app.activate(setting.action).unwrap();
        let field = NumberField::FooterItemFontSize;
        assert_eq!(app.screen, Screen::NumberingValue(field));
        assert_eq!(app.editor.string(), "14");

        for invalid in ["7", "37"] {
            app.editor.text = invalid.chars().collect();
            app.editor.caret = app.editor.text.len();
            assert!(app.apply_numbering_input(field).is_err());
            assert_eq!(app.controls.footer_item_font_size, None);
        }
        app.editor.text = "8".chars().collect();
        app.editor.caret = app.editor.text.len();
        app.apply_numbering_input(field).unwrap();
        assert_eq!(app.controls.footer_item_font_size, Some(8));
        assert_eq!(app.footer_caption_font_px(), 16);
        assert_eq!(
            crate::session::footer_caption_rect(500, app.footer_caption_font_px()).h,
            36
        );

        app.show(Screen::NumberingValue(field));
        app.editor.text = "36".chars().collect();
        app.editor.caret = app.editor.text.len();
        app.apply_numbering_input(field).unwrap();
        assert_eq!(app.controls.footer_item_font_size, Some(36));
        assert_eq!(app.footer_caption_font_px(), 40);
        let large_rect = crate::session::footer_caption_rect(500, app.footer_caption_font_px());
        assert_eq!(large_rect.y, PAGE_HEIGHT as i32);
        assert_eq!(large_rect.h, 60);
        assert_eq!(
            ReadingControls::load(&app.controls_path, true).footer_item_font_size,
            Some(36)
        );

        app.activate(Action::SetReadingControls(ReadingControls {
            status_bar: crate::reading_controls::StatusBar::Progress,
            ..app.controls
        }))
        .unwrap();
        app.show(Screen::Book);
        capture_test_screen(&app, &fonts, "READER_FOOTER_FONT_LARGE_SCREENSHOT", false);

        let pdf = fixture.root.join("Footer font.pdf");
        fs::write(&pdf, crate::fixed::tests::document()).unwrap();
        app.open_path(&pdf).unwrap();
        capture_test_screen(&app, &fonts, "READER_FOOTER_FONT_FIXED_SCREENSHOT", false);
        assert!(app.fixed.is_some());
    }

    #[test]
    fn footer_container_height_and_bottom_margin_match_stock_ranges_and_geometry() {
        let fixture = Fixture::new();
        let fonts = vec![
            crate::test_fixture("fonts/NotoSans-Regular.ttf"),
            crate::test_fixture("fonts/Silkscreen-Regular.ttf"),
        ];
        let mut app = fixture.app();
        app.fonts = fonts.clone();
        app.open_path(&fixture.book).unwrap();

        capture_test_screen(
            &app,
            &fonts,
            "READER_FOOTER_CONTAINER_DEFAULT_SCREENSHOT",
            false,
        );

        let original = app.footer_caption_rect(700, 120, 28);
        assert_eq!(original.h, 28);
        assert_eq!(original.y + original.h / 2, PAGE_HEIGHT as i32 + 30);
        let original_track = app.footer_track_rect(120).unwrap();
        assert_eq!(original_track.y, HEIGHT as i32 - 12);

        app.show(Screen::FooterSettings);
        let height_setting = app
            .items()
            .into_iter()
            .find(|item| item.title == "Container height")
            .expect("the footer settings expose the KOReader container height");
        assert_eq!(height_setting.detail.as_deref(), Some("14 · 7–98"));
        app.activate(height_setting.action).unwrap();
        let height_field = NumberField::FooterContainerHeight;
        assert_eq!(app.editor.string(), "14");
        for invalid in ["6", "99"] {
            app.editor.text = invalid.chars().collect();
            app.editor.caret = app.editor.text.len();
            assert!(app.apply_numbering_input(height_field).is_err());
            assert_eq!(app.controls.footer_container_height, None);
        }
        app.editor.text = "7".chars().collect();
        app.editor.caret = app.editor.text.len();
        app.apply_numbering_input(height_field).unwrap();
        app.show(Screen::Book);
        let minimum = app.footer_caption_rect(700, 120, 28);
        assert_eq!(minimum.h, 14);
        assert_eq!(minimum.y + minimum.h / 2, original.y + original.h / 2);

        app.show(Screen::NumberingValue(height_field));
        app.editor.text = "98".chars().collect();
        app.editor.caret = app.editor.text.len();
        app.apply_numbering_input(height_field).unwrap();
        app.show(Screen::Book);
        let expanded = app.footer_caption_rect(700, 120, 28);
        assert_eq!(expanded.h, 196);
        assert!(expanded.y < PAGE_HEIGHT as i32);
        assert!(app.footer_cover_top(expanded, 120) < PAGE_HEIGHT as i32);
        assert_eq!(
            ReadingControls::load(&app.controls_path, true).footer_container_height,
            Some(98)
        );

        let margin_field = NumberField::FooterContainerBottomMargin;
        app.show(Screen::FooterSettings);
        let margin_setting = app
            .items()
            .into_iter()
            .find(|item| item.title == "Container bottom margin")
            .expect("the footer settings expose the KOReader bottom padding");
        assert_eq!(margin_setting.detail.as_deref(), Some("1 · 0–49"));
        app.activate(margin_setting.action).unwrap();
        assert_eq!(app.editor.string(), "1");
        app.editor.text = "50".chars().collect();
        app.editor.caret = app.editor.text.len();
        assert!(app.apply_numbering_input(margin_field).is_err());
        app.editor.text = "0".chars().collect();
        app.editor.caret = app.editor.text.len();
        app.apply_numbering_input(margin_field).unwrap();
        app.show(Screen::Book);
        assert_eq!(app.footer_track_rect(120).unwrap().y, HEIGHT as i32 - 10);
        let zero_margin = app.footer_caption_rect(700, 120, 28);
        assert_eq!(zero_margin.y, expanded.y + 2);

        app.show(Screen::NumberingValue(margin_field));
        app.editor.text = "49".chars().collect();
        app.editor.caret = app.editor.text.len();
        app.apply_numbering_input(margin_field).unwrap();
        app.show(Screen::Book);
        assert_eq!(app.footer_track_rect(120).unwrap().y, HEIGHT as i32 - 108);
        let maximum_margin = app.footer_caption_rect(700, 120, 28);
        assert_eq!(maximum_margin.y, expanded.y - 96);
        assert_eq!(
            ReadingControls::load(&app.controls_path, true).footer_container_bottom_margin,
            Some(49)
        );

        app.activate(Action::SetReadingControls(ReadingControls {
            footer_track_position: crate::reading_controls::FooterTrackPosition::Above,
            ..app.controls
        }))
        .unwrap();
        assert_eq!(
            app.footer_track_rect(120).unwrap().y,
            PAGE_HEIGHT as i32 + 2 - 96
        );
        app.activate(Action::SetReadingControls(ReadingControls {
            footer_track_position: crate::reading_controls::FooterTrackPosition::Alongside,
            ..app.controls
        }))
        .unwrap();
        assert_eq!(
            app.footer_track_rect(120).unwrap().y,
            PAGE_HEIGHT as i32 + 32 - 96
        );
        app.activate(Action::SetReadingControls(ReadingControls {
            footer_track_position: crate::reading_controls::FooterTrackPosition::Below,
            ..app.controls
        }))
        .unwrap();

        app.show(Screen::Book);
        capture_test_screen(
            &app,
            &fonts,
            "READER_FOOTER_CONTAINER_EXPANDED_SCREENSHOT",
            false,
        );
        app.controls.night_pages = true;
        capture_test_screen(
            &app,
            &fonts,
            "READER_FOOTER_CONTAINER_DARK_SCREENSHOT",
            true,
        );
        app.controls.night_pages = false;
        app.show(Screen::FooterSettings);
        let height_row = app
            .items()
            .iter()
            .position(|item| item.title == "Container height")
            .unwrap();
        app.list_page = height_row / ROWS_PER_PAGE;
        capture_test_screen(
            &app,
            &fonts,
            "READER_FOOTER_CONTAINER_SETTINGS_SCREENSHOT",
            false,
        );

        let pdf = fixture.root.join("Footer container.pdf");
        fs::write(&pdf, crate::fixed::tests::document()).unwrap();
        app.open_path(&pdf).unwrap();
        assert!(app.fixed.is_some());
        assert!(app.footer_cover_top(maximum_margin, 120) < PAGE_HEIGHT as i32);
        capture_test_screen(
            &app,
            &fonts,
            "READER_FOOTER_CONTAINER_PDF_SCREENSHOT",
            false,
        );
    }

    #[test]
    fn top_header_relayouts_from_current_anchor_and_displays_configured_reading_metrics() {
        use crate::reading_controls::{HeaderBattery, TopHeader};

        let fixture = Fixture::new();
        let fonts = vec![
            crate::test_fixture("fonts/NotoSans-Regular.ttf"),
            crate::test_fixture("fonts/Silkscreen-Regular.ttf"),
        ];
        let mut app = fixture.app();
        app.fonts = fonts.clone();
        app.open_path(&fixture.book).unwrap();
        let before = position(&app.active.as_ref().unwrap().reading);
        let before_top_margin = app
            .active
            .as_ref()
            .unwrap()
            .reading
            .config
            .settings
            .top_margin();
        app.activate(Action::TopStatusHeader(true)).unwrap();
        app.activate(Action::SetReadingControls(ReadingControls {
            top_header: TopHeader {
                battery: HeaderBattery::Percentage,
                reading_percent: true,
                ..app.controls.top_header
            },
            ..app.controls
        }))
        .unwrap();

        let active = app.active.as_ref().unwrap();
        assert_eq!(
            active.reading.config.settings.status_header_height,
            TopHeader::HEIGHT
        );
        assert_eq!(
            active.reading.config.settings.top_margin(),
            before_top_margin + TopHeader::HEIGHT
        );
        assert_eq!(position(&active.reading), before);
        let caption = app.top_header_caption().unwrap();
        assert!(caption.contains("Field Notes"));
        assert!(caption.contains('/'));
        assert!(caption.contains('%'));
        let settings = app.footer_items();
        assert!(
            settings
                .iter()
                .any(|item| item.title == "Header current time")
        );
        assert!(
            settings
                .iter()
                .any(|item| item.title == "Header battery status")
        );
        assert!(
            settings
                .iter()
                .all(|item| item.title != "Show status at top")
        );
        let mut canvas = Canvas::new(
            WIDTH,
            HEIGHT,
            &fonts
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>(),
        )
        .unwrap();
        let pixels = primitive_app::frame(&app, &mut canvas, false);
        let header_pixels = &pixels[..WIDTH as usize * TopHeader::HEIGHT as usize];
        assert!(
            header_pixels.iter().any(|pixel| *pixel < 255),
            "enabled header should draw pixels in its reserved screen area"
        );
        capture_test_screen(&app, &fonts, "READER_TOP_HEADER_SCREENSHOT", false);

        let mut night = app.controls;
        night.night_pages = true;
        app.activate(Action::SetReadingControls(night)).unwrap();
        let night_pixels = primitive_app::frame(&app, &mut canvas, false);
        assert!(
            night_pixels[..WIDTH as usize * TopHeader::HEIGHT as usize]
                .iter()
                .any(|pixel| *pixel < 50),
            "night pages should invert the header strip with the reading page"
        );
        capture_test_screen(&app, &fonts, "READER_TOP_HEADER_NIGHT_SCREENSHOT", false);
    }

    #[test]
    fn exact_screen_pages_feed_the_opt_in_header_and_keep_the_chapter_footer() {
        let fixture = Fixture::new();
        let fonts = vec![
            crate::test_fixture("fonts/NotoSans-Regular.ttf"),
            crate::test_fixture("fonts/Silkscreen-Regular.ttf"),
        ];
        let mut app = fixture.app();
        app.fonts = fonts.clone();
        app.open_path(&fixture.book).unwrap();
        assert!(app.top_header_caption().is_none());
        app.activate(Action::TopStatusHeader(true)).unwrap();
        let reading = &app.active.as_ref().unwrap().reading;
        let original_position = position(reading);
        let first = reading.probe.page_count();
        let hrefs: Vec<_> = reading
            .probe
            .chapter_entries()
            .iter()
            .map(|entry| entry.href.clone())
            .collect();
        assert!(
            app.top_header_caption()
                .unwrap()
                .contains(&format!("1 / {first}"))
        );
        let original_footer = app.status_caption().unwrap();
        assert_eq!(
            original_footer,
            format!("Chapter 1 of 1 - Page 1 of {first}")
        );
        let mut canvas = Canvas::new(
            WIDTH,
            HEIGHT,
            &fonts
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>(),
        )
        .unwrap();
        let before = primitive_app::frame(&app, &mut canvas, false);
        capture_test_screen(&app, &fonts, "READER_LOCAL_SCREEN_PAGE_SCREENSHOT", false);
        let jump = |path: &str| {
            Action::Jump(Destination {
                path: path.to_owned(),
                fragment: None,
            })
        };
        app.activate(jump(&hrefs[1])).unwrap();
        let second = app.active.as_ref().unwrap().reading.probe.page_count();
        let total = first + second;
        assert!(
            app.top_header_caption()
                .unwrap()
                .contains(&format!("{next} / {total}", next = first + 1))
        );
        app.activate(jump(&hrefs[0])).unwrap();
        assert!(
            app.top_header_caption()
                .unwrap()
                .contains(&format!("1 / {total}"))
        );
        assert_eq!(
            app.status_caption().as_deref(),
            Some(original_footer.as_str())
        );
        assert_eq!(
            position(&app.active.as_ref().unwrap().reading),
            original_position
        );
        let after = primitive_app::frame(&app, &mut canvas, false);
        let header_end = crate::reading_controls::TopHeader::HEIGHT as usize * WIDTH as usize;
        assert!(before[..header_end] != after[..header_end]);
        assert!(
            before[header_end..] == after[header_end..],
            "the document pane and chapter footer should have the same pixels"
        );
        capture_test_screen(
            &app,
            &fonts,
            "READER_WHOLE_BOOK_SCREEN_PAGE_SCREENSHOT",
            false,
        );
    }

    #[test]
    fn screen_pages_background_finishes_the_latin_book_without_a_page_turn() {
        let fixture = Fixture::new();
        let path = fixture.root.join("Many Chapters.epub");
        fs::copy(crate::test_fixture("many-chapters.epub"), &path).unwrap();
        let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
        let mut app = fixture.app();
        app.fonts = fonts.clone();
        app.open_path(&path).unwrap();
        assert!(!app.needs_screen_page_count());
        assert!(app.screen_page_worker.deadline(None).is_none());

        app.activate(Action::TopStatusHeader(true)).unwrap();
        assert!(app.needs_screen_page_count());
        let shown = |reader: &Reader| {
            reader
                .active
                .as_ref()
                .unwrap()
                .reading
                .whole_book_screen_pages()
        };
        assert_eq!(shown(&app), None);
        let initial_caption = app.status_caption().unwrap();
        let initial_position = position(&app.active.as_ref().unwrap().reading);
        let mut canvas = Canvas::new(
            WIDTH,
            HEIGHT,
            &fonts
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>(),
        )
        .unwrap();
        let before = primitive_app::frame(&app, &mut canvas, false);
        capture_test_screen(&app, &fonts, "READER_BACKGROUND_SCREEN_PAGE_BEFORE", false);
        let timeout = Instant::now() + Duration::from_secs(12);
        while shown(&app).is_none() && Instant::now() < timeout {
            app.tick(Instant::now());
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(shown(&app), Some((1, 48)));
        assert!(app.top_header_caption().unwrap().contains("1 / 48"));
        assert_eq!(
            position(&app.active.as_ref().unwrap().reading),
            initial_position
        );
        assert_eq!(app.status_caption().unwrap(), initial_caption);
        let after = primitive_app::frame(&app, &mut canvas, false);
        let header_end = crate::reading_controls::TopHeader::HEIGHT as usize * WIDTH as usize;
        assert!(before[..header_end] != after[..header_end]);
        assert!(before[header_end..] == after[header_end..]);
        capture_test_screen(&app, &fonts, "READER_BACKGROUND_SCREEN_PAGE_AFTER", false);
    }

    #[test]
    #[ignore = "requires a user-authorized DRM-free EPUB in READER_PARITY_EPUB"]
    fn screen_pages_background_verifies_an_original_epub() {
        let original = PathBuf::from(
            std::env::var_os("READER_PARITY_EPUB")
                .expect("set READER_PARITY_EPUB to the authorized original document"),
        )
        .canonicalize()
        .unwrap();
        assert!(
            fs::metadata(&original).unwrap().len() <= 8 * 1024 * 1024,
            "choose an original EPUB inside the background archive limit"
        );
        let digest = md5::compute(fs::read(&original).unwrap());
        let fixture = Fixture::new();
        let fonts = simulation_fonts();
        let paths: Vec<_> = fonts.iter().map(|p| p.display().to_string()).collect();
        let mut canvas = Canvas::new(WIDTH, HEIGHT, &paths).unwrap();
        let mut app = Reader::open(original.parent().unwrap(), &fixture.state, fonts).unwrap();
        app.controls.top_header.book_default = Some(true);
        app.controls.top_header.clock = false;
        let started = Instant::now();
        app.open_path(&original).unwrap();
        if app.screen == Screen::PublisherOffer {
            app.show(Screen::Book);
        }
        if app.publisher_footer_enabled() {
            app.activate(Action::TogglePublisherFooter).unwrap();
        }
        assert!(app.needs_screen_page_count());
        let active = &app.active.as_ref().unwrap().reading;
        let chapters = active.probe.chapter_count();
        assert!(
            (2..=32).contains(&chapters),
            "choose a short multi-chapter original EPUB"
        );
        let reading_position = position(active);
        let before = primitive_app::frame(&app, &mut canvas, false);
        let first_frame_ms = started.elapsed().as_millis();
        let shown = |app: &Reader| {
            app.active
                .as_ref()
                .unwrap()
                .reading
                .whole_book_screen_pages()
        };
        assert_eq!(shown(&app), None);

        let timeout = Instant::now() + Duration::from_secs(90);
        while shown(&app).is_none() && Instant::now() < timeout {
            app.tick(Instant::now());
            std::thread::sleep(Duration::from_millis(20));
        }
        let (ordinal, total) =
            shown(&app).expect("the original EPUB should produce a bounded background count");
        let settled_ms = started.elapsed().as_millis();
        let peak_rss_kib = fs::read_to_string("/proc/self/status")
            .ok()
            .and_then(|status| {
                status.lines().find_map(|line| {
                    line.strip_prefix("VmHWM:")?
                        .split_whitespace()
                        .next()?
                        .parse::<u64>()
                        .ok()
                })
            });
        let reading = &app.active.as_ref().unwrap().reading;
        assert_eq!(position(reading), reading_position);
        let after = primitive_app::frame(&app, &mut canvas, false);
        let header_end = crate::reading_controls::TopHeader::HEIGHT as usize * WIDTH as usize;
        assert!(before[header_end..] == after[header_end..]);
        let mut counts = Vec::with_capacity(chapters);
        let mut expected_marks = Vec::new();
        let mut offset = 0;
        for chapter in 0..chapters {
            let probe = reading.config.chapter(chapter).unwrap();
            expected_marks.extend(
                probe
                    .local_contents_pages()
                    .iter()
                    .map(|&(_, page)| offset + page),
            );
            counts.push(probe.page_count());
            offset += probe.page_count();
        }
        expected_marks.sort_unstable();
        expected_marks.dedup();
        let (marks, marker_count) = reading.whole_book_publisher_markers(|_| true).unwrap();
        assert_eq!(marker_count, total);
        assert_eq!(marks, expected_marks);
        assert_eq!(
            ordinal,
            counts[..reading.chapter].iter().sum::<usize>() + reading.page + 1
        );
        assert_eq!(total, counts.iter().sum::<usize>());
        assert!(md5::compute(fs::read(&original).unwrap()) == digest);
        println!(
            "{}",
            serde_json::json!({
                "chapters": chapters, "screen_page": ordinal, "screen_pages": total,
                "first_frame_ms": first_frame_ms, "whole_book_ready_ms": settled_ms,
                "peak_rss_kib": peak_rss_kib, "publisher_marks": marks.len(),
            })
        );
    }

    #[test]
    fn top_header_is_removed_for_continuous_scroll_and_restored_for_page_turns() {
        use crate::reading_controls::TopHeader;

        let fixture = Fixture::new();
        let long = fixture.root.join("Long Chapter.epub");
        fs::write(&long, include_bytes!("../tests/fixtures/long-chapter.epub")).unwrap();
        let mut app = fixture.app();
        app.open_path(&long).unwrap();
        app.activate(Action::TopStatusHeader(true)).unwrap();
        assert_eq!(
            app.active
                .as_ref()
                .unwrap()
                .reading
                .config
                .settings
                .status_header_height,
            TopHeader::HEIGHT
        );

        app.activate(Action::ReadingMode(true)).unwrap();
        let reading = &app.active.as_ref().unwrap().reading;
        assert!(reading.is_continuous());
        assert_eq!(reading.config.settings.status_header_height, 0);
        assert!(app.top_header_caption().is_none());

        app.activate(Action::ReadingMode(false)).unwrap();
        let reading = &app.active.as_ref().unwrap().reading;
        assert!(!reading.is_continuous());
        assert_eq!(
            reading.config.settings.status_header_height,
            TopHeader::HEIGHT
        );
        assert!(app.top_header_caption().is_some());
    }

    #[test]
    fn pdf_footer_uses_outline_pages_and_leaves_zoom_and_done_available() {
        use crate::reading_controls::StatusBar;

        let fixture = Fixture::new();
        let pdf = fixture.root.join("Footer.pdf");
        fs::write(&pdf, crate::fixed::tests::document()).unwrap();
        let mut app = fixture.app();
        app.activate(Action::SetReadingControls(ReadingControls {
            status_bar: StatusBar::PageNumbers,
            ..app.controls
        }))
        .unwrap();
        app.open_path(&pdf).unwrap();
        let tap = Input::Tap(240.0, HEIGHT as f32 - 30.0);
        app.input(tap);
        assert_eq!(app.status_caption().as_deref(), Some("50% of book"));
        app.input(tap);
        assert!(app.status_caption().unwrap().contains("Page 1 of"));
        let before = app.fixed.as_ref().unwrap().reading.preferences().zoom;
        assert_eq!(
            app.input(Input::Tap(1030.0, HEIGHT as f32 - 34.0)),
            Change::Partial
        );
        assert!(app.fixed.as_ref().unwrap().reading.preferences().zoom > before);
        assert_eq!(app.controls.status_bar, StatusBar::Chapter);
        app.fixed.as_mut().unwrap().reading.jump(1).unwrap();
        app.input(tap);
        assert!(app.status_caption().unwrap().ends_with("0 pages left"));
        assert_eq!(
            app.input(Input::Tap(DONE.x as f32 + 30.0, DONE.y as f32 + 30.0)),
            Change::Exit
        );
    }

    #[test]
    fn footer_alignment_cycles_persists_and_positions_status_text() {
        use crate::reading_controls::FooterAlignment;

        let fixture = Fixture::new();
        let fonts = simulation_fonts();
        let mut app = fixture.app();
        app.open_path(&fixture.book).unwrap();

        let row = app
            .footer_items()
            .into_iter()
            .find(|item| item.title == "Footer alignment")
            .expect("reading footer exposes KOReader alignment choices");
        assert_eq!(row.detail.as_deref(), Some("Left"));
        let left = app.footer_caption_rect(700, 120, 28);
        assert_eq!(left.x, 38);
        assert_eq!(left.w, 120);
        capture_test_screen(
            &app,
            &fonts,
            "READER_FOOTER_ALIGNMENT_LEFT_SCREENSHOT",
            false,
        );

        for (alignment, expected_x, screenshot) in [
            (
                FooterAlignment::Center,
                328,
                "READER_FOOTER_ALIGNMENT_CENTER_SCREENSHOT",
            ),
            (
                FooterAlignment::Right,
                618,
                "READER_FOOTER_ALIGNMENT_RIGHT_SCREENSHOT",
            ),
        ] {
            let row = app
                .footer_items()
                .into_iter()
                .find(|item| item.title == "Footer alignment")
                .unwrap();
            app.activate(row.action).unwrap();
            assert_eq!(app.controls.footer_alignment, alignment);
            assert_eq!(
                ReadingControls::load(&app.controls_path, true).footer_alignment,
                alignment
            );
            let row = app
                .footer_items()
                .into_iter()
                .find(|item| item.title == "Footer alignment")
                .unwrap();
            assert_eq!(row.detail.as_deref(), Some(alignment.label()));
            let rect = app.footer_caption_rect(700, 120, 28);
            assert_eq!(rect.x, expected_x);
            assert_eq!(rect.w, 120);
            capture_test_screen(&app, &fonts, screenshot, false);
        }

        app.activate(Action::SetReadingControls(ReadingControls {
            status_bar: crate::reading_controls::StatusBar::Minimal,
            ..app.controls
        }))
        .unwrap();
        assert!(
            app.footer_items()
                .iter()
                .all(|item| item.title != "Footer alignment")
        );
    }

    #[test]
    fn footer_bold_is_off_by_default_toggle_persists_and_renders_with_matching_width() {
        use crate::reading_controls::FooterTrackPosition;

        let fixture = Fixture::new();
        let fonts = vec![
            crate::test_fixture("fonts/NotoSans-Regular.ttf"),
            crate::test_fixture("fonts/NotoSans-Bold.ttf"),
        ];
        let font_paths = fonts
            .iter()
            .map(|font| font.display().to_string())
            .collect::<Vec<_>>();
        let mut canvas = Canvas::new(WIDTH, HEIGHT, &font_paths).unwrap();
        let mut app = fixture.app();
        app.open_path(&fixture.book).unwrap();

        assert!(!app.controls.footer_item_bold);
        let row = app
            .footer_items()
            .into_iter()
            .find(|item| item.title == "Bold text")
            .expect("KOReader-compatible footer bold control is available");
        assert_eq!(row.detail.as_deref(), Some("Off"));
        let regular = primitive_app::frame(&app, &mut canvas, false);
        capture_test_screen(&app, &fonts, "READER_FOOTER_BOLD_OFF_SCREENSHOT", false);

        app.activate(row.action).unwrap();
        assert!(app.controls.footer_item_bold);
        assert!(ReadingControls::load(&app.controls_path, true).footer_item_bold);
        assert_eq!(
            app.status_caption().as_deref(),
            Some("Chapter 1 of 1 - Page 1 of 2")
        );
        let row = app
            .footer_items()
            .into_iter()
            .find(|item| item.title == "Bold text")
            .expect("footer control remains available after changing it");
        assert_eq!(row.detail.as_deref(), Some("On"));
        let bold = primitive_app::frame(&app, &mut canvas, false);
        assert_ne!(regular, bold, "bold weight changes the rendered footer");
        capture_test_screen(&app, &fonts, "READER_FOOTER_BOLD_ON_SCREENSHOT", false);

        let caption = app.status_caption().unwrap();
        let size = app.footer_caption_font_px();
        let bold_width = canvas.measure_with_weight(&caption, size, true);
        let regular_width = canvas.measure_with_weight(&caption, size, false);
        assert!(bold_width >= regular_width);
        app.controls.footer_track_position = FooterTrackPosition::Alongside;
        if let Some(track) = app.footer_track_rect(bold_width) {
            assert!(track.x + track.w <= DONE.x);
        }
    }

    #[test]
    fn footer_font_face_uses_and_persists_a_reader_font_and_status_bar_presets_keep_it() {
        let fixture = Fixture::new();
        let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
        let font_paths = fonts
            .iter()
            .map(|font| font.display().to_string())
            .collect::<Vec<_>>();
        let mut canvas = Canvas::new(WIDTH, HEIGHT, &font_paths).unwrap();
        let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
        app.open_path(&fixture.book).unwrap();
        fs::create_dir_all(&app.fonts_directory).unwrap();
        let font_file = "Silkscreen-Regular.ttf";
        fs::copy(
            crate::test_fixture("fonts/Silkscreen-Regular.ttf"),
            app.fonts_directory.join(font_file),
        )
        .unwrap();

        let default = primitive_app::frame(&app, &mut canvas, false);
        let font_row = app
            .footer_items()
            .into_iter()
            .find(|item| item.title == "Item font")
            .expect("KOReader's status font face control is available");
        assert_eq!(font_row.detail.as_deref(), Some("Device default"));
        app.activate(font_row.action).unwrap();
        assert_eq!(app.screen, Screen::FooterFontFaces);
        capture_test_screen(&app, &fonts, "READER_FOOTER_FONT_FACES_SCREENSHOT", false);
        let face = app
            .items()
            .into_iter()
            .find(|item| item.title == font_file)
            .expect("Reader lists installed TTF faces");
        app.activate(face.action).unwrap();
        assert_eq!(app.footer_font_file.as_deref(), Some(font_file));
        assert_eq!(
            footer_font_face::load(&app.footer_font_path, &app.fonts_directory).as_deref(),
            Some(font_file)
        );
        capture_test_screen(
            &app,
            &fonts,
            "READER_FOOTER_FONT_FACE_SETTINGS_SCREENSHOT",
            false,
        );

        app.save_footer_preset_name(FooterPresetName::Create, "Silkscreen footer")
            .unwrap();
        app.set_footer_font_file(None).unwrap();
        app.activate(Action::ApplyFooterPreset(0)).unwrap();
        assert_eq!(app.footer_font_file.as_deref(), Some(font_file));

        app.show(Screen::Book);
        let selected = primitive_app::frame(&app, &mut canvas, false);
        assert_ne!(
            default, selected,
            "the selected face changes the footer pixels"
        );
        capture_test_screen(&app, &fonts, "READER_FOOTER_FONT_FACE_SCREENSHOT", false);

        let reopened = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
        assert_eq!(reopened.footer_font_file.as_deref(), Some(font_file));
        assert_eq!(reopened.footer_presets.len(), 1);
        assert_eq!(
            reopened.footer_presets[0]
                .apply(ReadingControls::default())
                .1
                .as_deref(),
            Some(font_file)
        );

        let pdf = fixture.root.join("Footer font face.pdf");
        fs::write(&pdf, crate::fixed::tests::document()).unwrap();
        app.set_footer_font_file(None).unwrap();
        app.open_path(&pdf).unwrap();
        let default_pdf = primitive_app::frame(&app, &mut canvas, false);
        app.set_footer_font_file(Some(font_file.to_owned()))
            .unwrap();
        let selected_pdf = primitive_app::frame(&app, &mut canvas, false);
        assert_ne!(
            default_pdf, selected_pdf,
            "the selected face changes fixed-page footer pixels too"
        );
        capture_test_screen(
            &app,
            &fonts,
            "READER_FOOTER_FONT_FACE_PDF_SCREENSHOT",
            false,
        );
    }

    #[test]
    fn footer_separator_is_optional_persistent_and_matches_its_rendered_line() {
        let fixture = Fixture::new();
        let fonts = vec![
            crate::test_fixture("fonts/NotoSans-Regular.ttf"),
            crate::test_fixture("fonts/NotoSans-Bold.ttf"),
        ];
        let font_paths = fonts
            .iter()
            .map(|font| font.display().to_string())
            .collect::<Vec<_>>();
        let mut canvas = Canvas::new(WIDTH, HEIGHT, &font_paths).unwrap();
        let mut app = fixture.app();
        app.open_path(&fixture.book).unwrap();

        assert!(!app.controls.footer_horizontal_separator);
        let line_pixel = (PAGE_HEIGHT as usize) * WIDTH as usize + WIDTH as usize / 2;
        let without_separator = primitive_app::frame(&app, &mut canvas, false);
        assert_eq!(without_separator[line_pixel], 255);
        capture_test_screen(
            &app,
            &fonts,
            "READER_FOOTER_SEPARATOR_OFF_SCREENSHOT",
            false,
        );

        let row = app
            .footer_items()
            .into_iter()
            .find(|item| item.title == "Show footer separator")
            .expect("KOReader's optional footer separator control is available");
        assert_eq!(row.detail.as_deref(), Some("Off"));
        app.activate(row.action).unwrap();
        assert!(app.controls.footer_horizontal_separator);
        assert!(ReadingControls::load(&app.controls_path, true).footer_horizontal_separator);
        let row = app
            .footer_items()
            .into_iter()
            .find(|item| item.title == "Show footer separator")
            .expect("footer separator control remains available after changing it");
        assert_eq!(row.detail.as_deref(), Some("On"));

        let with_separator = primitive_app::frame(&app, &mut canvas, false);
        assert_eq!(
            with_separator[line_pixel],
            primitive_ui::HAIRLINE,
            "enabled footer separator is painted at the content/footer boundary"
        );
        capture_test_screen(&app, &fonts, "READER_FOOTER_SEPARATOR_ON_SCREENSHOT", false);
        capture_test_screen(
            &app,
            &fonts,
            "READER_FOOTER_SEPARATOR_DARK_SCREENSHOT",
            true,
        );

        app.show(Screen::FooterSettings);
        let item_index = app
            .footer_items()
            .iter()
            .position(|item| item.title == "Show footer separator")
            .unwrap();
        app.list_page = item_index / ROWS_PER_PAGE;
        capture_test_screen(
            &app,
            &fonts,
            "READER_FOOTER_SEPARATOR_SETTINGS_SCREENSHOT",
            false,
        );

        let pdf_path = fixture.root.join("Footer separator.pdf");
        fs::copy(crate::test_fixture("cmyk-print-chart.pdf"), &pdf_path).unwrap();
        app.open_path(&pdf_path).unwrap();
        app.activate(Action::SetReadingControls(ReadingControls {
            footer_horizontal_separator: false,
            ..app.controls
        }))
        .unwrap();
        let fixed_without_separator = primitive_app::frame(&app, &mut canvas, false);
        assert_eq!(fixed_without_separator[line_pixel], 255);
        app.activate(Action::SetReadingControls(ReadingControls {
            footer_horizontal_separator: true,
            ..app.controls
        }))
        .unwrap();
        let fixed_with_separator = primitive_app::frame(&app, &mut canvas, false);
        assert_eq!(fixed_with_separator[line_pixel], primitive_ui::HAIRLINE);
        capture_test_screen(
            &app,
            &fonts,
            "READER_FOOTER_SEPARATOR_PDF_SCREENSHOT",
            false,
        );
    }

    #[test]
    fn footer_progress_track_positions_persist_and_reserve_text_space_for_epub_and_pdf() {
        use crate::reading_controls::{
            ChapterMarkerWidth, FooterTrackPosition, FooterTrackThickness, StatusBar,
        };

        let fixture = Fixture::new();
        let fonts = vec![
            crate::test_fixture("fonts/NotoSans-Regular.ttf"),
            crate::test_fixture("fonts/Silkscreen-Regular.ttf"),
        ];
        let font_paths = fonts
            .iter()
            .map(|font| font.display().to_string())
            .collect::<Vec<_>>();
        let mut measure_canvas = Canvas::new(WIDTH, HEIGHT, &font_paths).unwrap();
        let mut app = fixture.app();
        app.open_path(&fixture.book).unwrap();
        app.activate(Action::SetReadingControls(ReadingControls {
            status_bar: StatusBar::Chapter,
            footer_track_thickness: FooterTrackThickness::Thin,
            ..app.controls
        }))
        .unwrap();

        let caption = app.active.as_ref().unwrap().reading.footer_caption(None);
        let measured_text_width = measure_canvas.measure(&caption, 28);
        let margin = 10;
        let track_right = DONE.x - margin;
        let minimum_track_width = (WIDTH as i32 * 20 + 99) / 100;
        let caption_width = measured_text_width.min(track_right - 38 - 20 - minimum_track_width);
        let alongside_track_width = track_right - 38 - caption_width - 20;
        let expected = [
            (
                FooterTrackPosition::Above,
                Rect::new(margin, PAGE_HEIGHT as i32 + 2, DONE.x - 2 * margin, 3),
            ),
            (
                FooterTrackPosition::Alongside,
                Rect::new(
                    38 + caption_width + 20,
                    PAGE_HEIGHT as i32 + 32,
                    alongside_track_width,
                    3,
                ),
            ),
            (
                FooterTrackPosition::Below,
                Rect::new(margin, HEIGHT as i32 - 12, DONE.x - 2 * margin, 3),
            ),
        ];
        for (position, rect) in expected {
            app.activate(Action::SetReadingControls(ReadingControls {
                footer_track_position: position,
                ..app.controls
            }))
            .unwrap();
            assert_eq!(app.footer_track_rect(measured_text_width), Some(rect));
            assert_eq!(
                app.footer_caption_width((DONE.x - 60) as u32, measured_text_width),
                if position == FooterTrackPosition::Alongside {
                    caption_width as u32
                } else {
                    (DONE.x - 60) as u32
                }
            );
            assert_eq!(
                ReadingControls::load(&app.controls_path, true).footer_track_position,
                position
            );
            let (screenshot, dark_screenshot) = match position {
                FooterTrackPosition::Above => ("READER_FOOTER_ABOVE_SCREENSHOT", None),
                FooterTrackPosition::Alongside => (
                    "READER_FOOTER_ALONGSIDE_SCREENSHOT",
                    Some("READER_FOOTER_ALONGSIDE_DARK_SCREENSHOT"),
                ),
                FooterTrackPosition::Below => ("READER_FOOTER_BELOW_SCREENSHOT", None),
            };
            capture_test_screen(&app, &fonts, screenshot, false);
            if position == FooterTrackPosition::Alongside {
                assert_eq!(rect.x, 38 + caption_width + 20);
                assert!(rect.w >= minimum_track_width);
                capture_test_screen(&app, &fonts, dark_screenshot.unwrap(), true);
            }
        }
        app.activate(Action::SetReadingControls(ReadingControls {
            footer_track_thickness: FooterTrackThickness::Thick,
            ..app.controls
        }))
        .unwrap();
        assert_eq!(app.footer_track_rect(measured_text_width).unwrap().h, 7);
        assert_eq!(
            ReadingControls::load(&app.controls_path, true).footer_track_thickness,
            FooterTrackThickness::Thick
        );
        let position_row = app
            .footer_items()
            .into_iter()
            .find(|item| item.title == "Progress bar position")
            .expect("footer settings expose progress placement");
        app.activate(position_row.action).unwrap();
        assert_eq!(
            app.controls.footer_track_position,
            FooterTrackPosition::Above
        );
        let settings = app.footer_items();
        assert_eq!(
            settings
                .iter()
                .find(|item| item.title == "Progress bar position")
                .and_then(|item| item.detail.as_deref()),
            Some("Above status text")
        );
        assert_eq!(
            settings
                .iter()
                .find(|item| item.title == "Progress bar thickness")
                .and_then(|item| item.detail.as_deref()),
            Some("Thick")
        );
        let marker_width = app
            .footer_items()
            .into_iter()
            .find(|item| item.title == "Chapter marker thickness")
            .expect("book progress settings expose marker thickness");
        assert_eq!(marker_width.detail.as_deref(), Some("Medium"));
        app.activate(marker_width.action).unwrap();
        assert_eq!(app.controls.chapter_marker_width, ChapterMarkerWidth::Thick);
        assert_eq!(
            ReadingControls::load(&app.controls_path, true).chapter_marker_width,
            ChapterMarkerWidth::Thick
        );
        app.activate(Action::SetReadingControls(ReadingControls {
            footer_track_position: FooterTrackPosition::Alongside,
            ..app.controls
        }))
        .unwrap();
        let long_text_track = app.footer_track_rect(i32::MAX).unwrap();
        assert_eq!(long_text_track.w, minimum_track_width);
        assert_eq!(long_text_track.x + long_text_track.w, DONE.x - margin);

        let lock_width = app
            .footer_items()
            .into_iter()
            .find(|item| item.title == "Lock progress bar width at minimum")
            .expect("footer settings expose KOReader's locked minimum-width option");
        assert_eq!(lock_width.detail.as_deref(), Some("Off"));
        app.activate(lock_width.action).unwrap();
        assert!(app.controls.footer_track_lock_min_width);
        assert_eq!(
            ReadingControls::load(&app.controls_path, true).footer_track_lock_min_width,
            true
        );
        let locked_track = app.footer_track_rect(measured_text_width).unwrap();
        assert_eq!(locked_track.w, minimum_track_width);
        assert_eq!(locked_track.x + locked_track.w, DONE.x - margin);
        assert_eq!(
            app.footer_track_rect(0).unwrap(),
            locked_track,
            "locked progress width stays anchored when the caption is short"
        );
        assert_eq!(
            app.footer_track_rect(i32::MAX).unwrap(),
            locked_track,
            "locked progress width stays anchored when the caption is long"
        );
        assert_eq!(
            app.footer_caption_width(500, measured_text_width),
            measured_text_width.min((locked_track.x - 38 - 20).max(0)) as u32
        );
        capture_test_screen(&app, &fonts, "READER_FOOTER_LOCKED_SCREENSHOT", false);
        app.show(Screen::FooterSettings);
        assert_eq!(
            app.footer_items()
                .into_iter()
                .find(|item| item.title == "Lock progress bar width at minimum")
                .and_then(|item| item.detail),
            Some("On".to_owned())
        );

        let minimum_width = app
            .footer_items()
            .into_iter()
            .find(|item| item.title == "Progress bar minimum width")
            .expect("footer settings expose the stock minimum track width");
        assert_eq!(minimum_width.detail.as_deref(), Some("20%"));
        app.activate(minimum_width.action).unwrap();
        assert_eq!(
            app.screen,
            Screen::NumberingValue(NumberField::FooterTrackMinimumWidth)
        );
        app.editor.text = "4".chars().collect();
        app.editor.caret = app.editor.text.len();
        assert!(
            app.apply_numbering_input(NumberField::FooterTrackMinimumWidth)
                .is_err()
        );
        assert_eq!(app.controls.footer_track_min_width_pct, None);
        app.editor.text = "35".chars().collect();
        app.editor.caret = app.editor.text.len();
        app.apply_numbering_input(NumberField::FooterTrackMinimumWidth)
            .unwrap();
        assert_eq!(app.controls.footer_track_min_width_pct, Some(35));
        assert_eq!(
            ReadingControls::load(&app.controls_path, true).footer_track_min_width_pct,
            Some(35)
        );
        app.show(Screen::Book);
        let wider_minimum_track = app.footer_track_rect(i32::MAX).unwrap();
        let min_width_35 = (WIDTH as i32 * 35 + 99) / 100;
        assert_eq!(wider_minimum_track.w, min_width_35);
        app.show(Screen::FooterSettings);

        let margins = app
            .footer_items()
            .into_iter()
            .find(|item| item.title == "Progress bar margins")
            .expect("footer settings expose progress-track margins");
        assert_eq!(margins.detail.as_deref(), Some("10 px"));
        app.activate(margins.action).unwrap();
        assert_eq!(
            app.screen,
            Screen::NumberingValue(NumberField::FooterTrackMarginWidth)
        );
        app.editor.text = "24".chars().collect();
        app.editor.caret = app.editor.text.len();
        app.apply_numbering_input(NumberField::FooterTrackMarginWidth)
            .unwrap();
        assert_eq!(app.controls.footer_track_margin_width, Some(24));
        assert_eq!(
            ReadingControls::load(&app.controls_path, true).footer_track_margin_width,
            Some(24)
        );
        app.show(Screen::Book);
        let margined_track = app.footer_track_rect(i32::MAX).unwrap();
        assert_eq!(margined_track.w, min_width_35);
        assert_eq!(margined_track.x + margined_track.w, DONE.x - 24);
        app.activate(Action::SetReadingControls(ReadingControls {
            footer_track_position: FooterTrackPosition::Below,
            ..app.controls
        }))
        .unwrap();
        app.show(Screen::FooterSettings);
        capture_test_screen(&app, &fonts, "READER_FOOTER_SETTINGS_SCREENSHOT", false);

        let pdf = fixture.root.join("Footer positions.pdf");
        fs::write(&pdf, crate::fixed::tests::document()).unwrap();
        app.activate(Action::SetReadingControls(ReadingControls {
            footer_track_position: FooterTrackPosition::Alongside,
            footer_track_lock_min_width: false,
            ..app.controls
        }))
        .unwrap();
        app.open_path(&pdf).unwrap();
        let pdf_caption = app.fixed.as_ref().unwrap().reading.footer_caption(None);
        let pdf_text_width = measure_canvas.measure(&pdf_caption, 28);
        let pdf_track_right = 840 - 24;
        let pdf_minimum_width = min_width_35;
        let pdf_caption_width = pdf_text_width.min(pdf_track_right - 38 - 20 - pdf_minimum_width);
        assert_eq!(
            app.footer_track_rect(pdf_text_width),
            Some(Rect::new(
                38 + pdf_caption_width + 20,
                PAGE_HEIGHT as i32 + 32,
                pdf_track_right - 38 - pdf_caption_width - 20,
                7,
            ))
        );
        assert_eq!(
            app.footer_caption_width((840 - 60) as u32, pdf_text_width),
            pdf_caption_width as u32
        );
        app.activate(Action::SetReadingControls(ReadingControls {
            footer_track_lock_min_width: true,
            ..app.controls
        }))
        .unwrap();
        let pdf_locked_track = app.footer_track_rect(pdf_text_width).unwrap();
        assert_eq!(pdf_locked_track.w, pdf_minimum_width);
        assert_eq!(pdf_locked_track.x + pdf_locked_track.w, pdf_track_right);
        assert_eq!(
            app.footer_caption_width((840 - 60) as u32, pdf_text_width),
            pdf_text_width.min((pdf_locked_track.x - 38 - 20).max(0)) as u32
        );
    }

    #[test]
    fn initial_position_marker_tracks_epub_and_pdf_and_resets_after_wake() {
        use crate::reading_controls::{ProgressScope, StatusBar};

        let fixture = Fixture::new();
        let fonts = simulation_fonts();
        let mut app = fixture.app();
        app.open_path(&fixture.book).unwrap();
        app.activate(Action::SetReadingControls(ReadingControls {
            status_bar: StatusBar::Progress,
            footer_scope: ProgressScope::Book,
            ..app.controls
        }))
        .unwrap();
        let marker_setting = app
            .footer_items()
            .into_iter()
            .find(|item| item.title == "Show initial position marker")
            .expect("footer settings expose KOReader's initial-position marker");
        assert_eq!(marker_setting.detail.as_deref(), Some("Off"));
        app.activate(marker_setting.action).unwrap();
        assert!(app.controls.footer_initial_marker);
        assert!(ReadingControls::load(&app.controls_path, true).footer_initial_marker);
        assert_eq!(
            app.footer_items()
                .into_iter()
                .find(|item| item.title == "Show initial position marker")
                .and_then(|item| item.detail),
            Some("On".to_owned())
        );
        let epub_start = app
            .footer_initial_fraction()
            .expect("opening an EPUB captures its starting location");
        app.follow_reflow_page(0, 1).unwrap();
        assert_eq!(app.footer_initial_fraction(), Some(epub_start));
        capture_test_screen(&app, &fonts, "READER_FOOTER_INITIAL_SCREENSHOT", false);

        app.activate(Action::SetReadingControls(ReadingControls {
            footer_scope: ProgressScope::Chapter,
            ..app.controls
        }))
        .unwrap();
        app.follow_reflow_page(1, 0).unwrap();
        assert_eq!(
            app.footer_initial_fraction(),
            None,
            "chapter progress hides the start marker after entering another chapter"
        );
        app.input(Input::Sleep);
        app.input(Input::Wake);
        assert!(
            app.footer_initial_fraction().is_some(),
            "wake resets the initial marker to the current chapter"
        );

        let pdf = fixture.root.join("Initial position.pdf");
        fs::write(&pdf, crate::fixed::tests::document()).unwrap();
        app.open_path(&pdf).unwrap();
        let pdf_start = app
            .footer_initial_fraction()
            .expect("opening a PDF captures its starting location");
        app.activate(Action::FixedPage(1)).unwrap();
        assert_eq!(app.footer_initial_fraction(), Some(pdf_start));
        app.input(Input::Sleep);
        app.input(Input::Wake);
        assert!(app.footer_initial_fraction().is_some());
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
    fn landscape_ocr_cancels_with_touch_or_the_back_button_without_moving_the_page() {
        use crate::app::selection_viewport_tests::{adopt, capture, press, tap};
        use crate::session::ReadingViewport;
        use std::os::unix::fs::PermissionsExt;

        let fixture = Fixture::new();
        let pdf = fixture.root.join("Scan.pdf");
        fs::copy(crate::test_fixture("tilted-scan.pdf"), &pdf).unwrap();
        let pack = fixture.root.join("language-pack");
        fs::create_dir_all(pack.join("ocr/tessdata")).unwrap();
        fs::write(pack.join("ocr/tessdata/eng.traineddata"), b"fixture").unwrap();
        let engine = pack.join("ocr/tesseract");
        fs::write(&engine, "#!/bin/sh\ncat >/dev/null\nexec sleep 20\n").unwrap();
        fs::set_permissions(&engine, fs::Permissions::from_mode(0o700)).unwrap();

        let fonts = vec![
            crate::test_fixture("fonts/NotoSans-Regular.ttf"),
            crate::test_fixture("fonts/NotoSans-Bold.ttf"),
        ];
        let mut app = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
        app.open_path(&pdf).unwrap();
        app.ocr_assets = OcrAssets::discover(&pack);
        let mut canvas = adopt(&mut app, ReadingViewport::LANDSCAPE, 1);
        let position = app.current_position();
        let cancel = app.reading_viewport().bottom_right(CANCEL_OCR);
        assert!(cancel.y >= 1190 && cancel.bottom() <= 1264);

        app.recognize_page(0, None, None).unwrap();
        assert!(app.ocr.is_some());
        press(&app, &mut canvas, cancel, false);
        capture(&app, &mut canvas, "landscape-ocr-cancel.png", false);
        tap(&mut app, cancel);
        assert!(app.ocr.is_none());
        assert_eq!(app.current_position(), position);

        app.recognize_page(0, None, None).unwrap();
        assert_eq!(app.input(Input::PageBack), Change::Partial);
        assert!(app.ocr.is_none());
        assert_eq!(app.notice.as_deref(), Some("Text recognition cancelled"));
        assert_eq!(app.current_position(), position);
        assert!(app.fixed.as_ref().unwrap().reading.cached_ocr(0).is_none());
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
            reading
                .restore_view(FixedPreferences {
                    straighten_limit: 10,
                    turns: 0,
                    crop: Some(PdfTextRect {
                        x0: 0.05,
                        y0: 0.04,
                        x1: 0.94,
                        y1: 0.94,
                    }),
                    ..Default::default()
                })
                .unwrap();
        }
        let reading = &app.fixed.as_ref().unwrap().reading;
        let source = reading.page_text(0).unwrap();
        let word = source
            .words
            .iter()
            .find(|w| &source.text[w.bytes.clone()] == "Trail")
            .unwrap()
            .bounds;
        let point = reading.visible_rectangle(0, word).unwrap().center();
        let raster = reading.ocr_raster(0).unwrap();
        // The simulated worker reports a different word at the source rectangle, just as an OCR
        // engine can disagree with an inaccurate invisible embedded text layer.
        let pack = fixture.root.join("engine");
        let tessdata = pack.join("ocr/tessdata");
        fs::create_dir_all(&tessdata).unwrap();
        let real =
            std::env::var_os("READER_TEST_TESSERACT").zip(std::env::var_os("READER_TEST_TESSDATA"));
        let expected = if let Some((engine, model)) = real {
            fs::copy(engine, pack.join("ocr/tesseract")).unwrap();
            fs::copy(model, tessdata.join("eng.traineddata")).unwrap();
            "Trail"
        } else {
            fs::write(tessdata.join("eng.traineddata"), b"fixture").unwrap();
            let script = format!(
                "#!/bin/sh\ncat >/dev/null\nprintf '%b\\n' 'level\\tpage_num\\tblock_num\\tpar_num\\tline_num\\tword_num\\tleft\\ttop\\twidth\\theight\\tconf\\ttext' '1\\t1\\t0\\t0\\t0\\t0\\t0\\t0\\t{}\\t{}\\t-1\\t' '5\\t1\\t1\\t1\\t1\\t1\\t{}\\t{}\\t{}\\t{}\\t98\\tHarbor'\n",
                raster.width,
                raster.height,
                (word.x0 * raster.width as f32) as usize,
                (word.y0 * raster.height as f32) as usize,
                ((word.x1 - word.x0) * raster.width as f32) as usize,
                ((word.y1 - word.y0) * raster.height as f32) as usize
            );
            fs::write(pack.join("ocr/tesseract"), script).unwrap();
            "Harbor"
        };
        fs::set_permissions(
            pack.join("ocr/tesseract"),
            fs::Permissions::from_mode(0o700),
        )
        .unwrap();
        let capture = |name: &str, app: &Reader| {
            if let (Some(dir), Some(fonts)) = (
                std::env::var_os("READER_OCR_SHOTS"),
                std::env::var_os("READER_TEST_FONTS"),
            ) {
                let fonts = std::env::split_paths(&fonts)
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>();
                let mut canvas = Canvas::new(WIDTH, HEIGHT, &fonts).unwrap();
                let dir = Path::new(&dir);
                fs::create_dir_all(dir).unwrap();
                primitive_app::write_png(
                    &dir.join(name),
                    WIDTH,
                    HEIGHT,
                    &primitive_app::frame(app, &mut canvas, false),
                )
                .unwrap();
            }
        };
        app.ocr_assets = OcrAssets::discover(&pack);
        assert!(app.ocr_assets.is_some());
        app.input(Input::Tap(630.0, 28.0));
        tap_reading_control(&mut app, "Scan");
        assert_eq!(tap_reading_control(&mut app, "Force OCR"), Change::Partial);
        capture("forced-ocr-control.png", &app);
        app.input(Input::Tap(630.0, 720.0));
        if app.screen == Screen::FixedTools {
            app.input(Input::Tap(630.0, 720.0));
        }
        assert_eq!(app.screen, Screen::Fixed);
        let hold = Input::LongTap(point.x as f32, point.y as f32);
        app.input(hold);
        assert!(app.ocr.is_some() && app.pdf_selection.is_none());
        app.input(Input::Tap(
            (CANCEL_OCR.x + 15) as f32,
            (CANCEL_OCR.y + 15) as f32,
        ));
        assert!(app.ocr.is_none());
        assert!(app.fixed.as_ref().unwrap().reading.cached_ocr(0).is_none());
        app.input(hold);
        let deadline = Instant::now() + Duration::from_secs(35);
        while let Some(job) = &app.ocr {
            assert!(Instant::now() < deadline, "OCR selection timed out");
            app.poll_ocr(job.deadline).unwrap();
            std::thread::sleep(Duration::from_millis(12));
        }
        let selected = app
            .pdf_selection
            .as_ref()
            .expect("recognized word at original touch");
        assert_eq!(selected.text(), expected);
        assert_eq!(selected.page(), 0);
        capture("forced-ocr-selection.png", &app);
        let reading = &app.fixed.as_ref().unwrap().reading;
        assert!(
            reading
                .selection_bounds(selected)
                .unwrap()
                .contains(point.x as f32, point.y as f32)
        );
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
        assert!(
            reopened
                .fixed
                .as_ref()
                .unwrap()
                .reading
                .preferences()
                .force_ocr
        );
        reopened.start_document_search("Trail").unwrap();
        while reopened.search_at.is_some() {
            reopened.step_pdf_search(Instant::now()).unwrap();
        }
        assert_eq!(
            reopened
                .pdf_hits
                .iter()
                .map(|hit| hit.page)
                .collect::<Vec<_>>(),
            [0, 1]
        );
        reopened.start_document_search("Harbor").unwrap();
        while reopened.search_at.is_some() {
            reopened.step_pdf_search(Instant::now()).unwrap();
        }
        assert!(
            reopened.pdf_hits.is_empty(),
            "native text takes priority over recognition in search"
        );
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
        let fonts = fonts
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>();
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
        assert_eq!(
            tap_reading_control(&mut app, "Increase 1°"),
            Change::Partial
        );
        assert_eq!(
            tap_reading_control(&mut app, "Decrease 1°"),
            Change::Partial
        );
        capture("scan-control.png", &app);
        app.input(Input::Tap(630.0, 720.0));
        if app.screen == Screen::FixedTools {
            app.input(Input::Tap(630.0, 720.0));
        }
        assert_eq!(app.screen, Screen::Fixed);
        let after = capture("scan-straightened.png", &app);
        assert!(
            before[..(WIDTH * PAGE_HEIGHT) as usize] != after[..(WIDTH * PAGE_HEIGHT) as usize]
        );
        assert_eq!(
            app.input(Input::Tap((DONE.x + 30) as f32, (DONE.y + 24) as f32)),
            Change::Exit
        );
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
        assert_eq!(fixed.reading.page(), 1);
        assert_eq!(fixed.reading.preferences().zoom, 0);
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
        app.close_fixed_search_fit().unwrap();
        app.jump_fixed(1, FixedJump::regions(&hit)).unwrap();
        let fixed = app.fixed.as_ref().unwrap();
        let bottom = fixed.reading.preferences();
        assert_eq!(bottom.zoom, 4);
        app.library
            .remember(&fixed.entry, fixed.id, "Signal chart")
            .add_pdf_highlight(PdfHighlight {
                updated_at: None,
                chapter_title: None,
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

        // The fitted preview keeps both pairs of search marks visible when a
        // phrase wraps back to the left and restores the previous reading mode.
        for continuous in [false, true] {
            let reading = &mut reopened.fixed.as_mut().unwrap().reading;
            reading.set_continuous(continuous).unwrap();
            reading.set_zoom(4).unwrap();
            reading.jump(0).unwrap();
            reopened
                .start_document_search("turns north past the")
                .unwrap();
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
            assert!(!reading.is_continuous());
            assert_eq!(reading.preferences().zoom, 0);
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
            let reading = &reopened.fixed.as_ref().unwrap().reading;
            assert_eq!(reading.page(), 0);
            assert_eq!(reading.is_continuous(), continuous);
            assert_eq!(reading.preferences().zoom, 4);
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
    fn scan_gamma_controls_apply_presets_and_fine_steps_then_reopen() {
        let fixture = Fixture::new();
        let pdf = fixture.root.join("Gray scan calibration.pdf");
        let stream = concat!(
            "0.94 g 52 480 496 230 re f\n",
            "0.20 g BT /F1 22 Tf 75 665 Td (Gray scan calibration) Tj ET\n",
            "0.45 g BT /F1 15 Tf 75 626 Td (Harbor survey notes) Tj ET\n",
            "0.60 g 75 566 76 30 re f 0.72 g 165 566 76 30 re f ",
            "0.82 g 255 566 76 30 re f 0.90 g 345 566 76 30 re f ",
            "0.98 g 435 566 76 30 re f\n",
            "0.63 g BT /F1 14 Tf 75 530 Td (Keep the outer marker visible at low tide.) Tj ET\n",
        );
        fs::write(
            &pdf,
            crate::fixed::tests::pdf_objects(&[
                "<< /Type /Catalog /Pages 2 0 R >>".into(),
                "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 600 800] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".into(),
                format!("<< /Length {} >>\nstream\n{stream}endstream", stream.len()),
                "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),
            ]),
        )
        .unwrap();
        let mut app = fixture.app();
        app.open_path(&pdf).unwrap();
        let original = app.current_history_location();
        app.open_reading_edge(false, 500.0);
        assert_eq!(tap_reading_control(&mut app, "Scan"), Change::Partial);
        assert_eq!(
            tap_reading_config_choice(&mut app, "Contrast", "2.0"),
            Change::Partial
        );
        assert_eq!(
            tap_reading_config_choice(&mut app, "Fine · 2.0", "+0.1"),
            Change::Partial
        );
        let prefs = app.fixed.as_ref().unwrap().reading.preferences();
        assert_eq!(prefs.scan_gamma_tenths, Some(21));
        assert_eq!(app.current_history_location(), original);
        let fonts = [
            crate::test_fixture("fonts/NotoSans-Regular.ttf"),
            crate::test_fixture("fonts/NotoSans-Bold.ttf"),
        ];
        capture_test_screen(&app, &fonts, "READER_SCAN_GAMMA_FRAME", false);
        app.save().unwrap();

        let mut reopened = fixture.app();
        reopened.open_path(&pdf).unwrap();
        assert_eq!(
            reopened.fixed.as_ref().unwrap().reading.preferences(),
            prefs
        );
        reopened.open_reading_edge(false, 500.0);
        if reopened.reading_panel != Some(BarPanel::Scan) {
            tap_reading_control(&mut reopened, "Scan");
        }
        tap_reading_config_choice(&mut reopened, "Fine · 2.1", "Adaptive");
        assert!(reopened.fixed.as_ref().unwrap().reading.scan_contrast());
        tap_reading_config_choice(&mut reopened, "Contrast", "1.0");
        let restored = reopened.fixed.as_ref().unwrap().reading.preferences();
        assert!(!restored.scan_contrast);
        assert_eq!(restored.scan_gamma_tenths, None);
        assert_eq!(reopened.current_history_location(), original);
    }

    #[test]
    fn scan_gamma_djvu_panel_uses_adaptive_contrast_and_reopens_it() {
        let fixture = Fixture::new();
        let document = fixture.root.join("Beacon atlas.djvu");
        fs::write(&document, crate::djvu::tests::grayscale_document()).unwrap();
        let mut app = fixture.app();
        app.open_path(&document).unwrap();
        let location = app.current_history_location();
        app.open_reading_edge(false, 500.0);
        assert_eq!(tap_reading_control(&mut app, "Scan"), Change::Partial);
        assert_eq!(
            tap_reading_config_choice(&mut app, "Contrast", "Normal contrast"),
            Change::None
        );
        assert!(!app.fixed.as_ref().unwrap().reading.scan_contrast());
        assert_eq!(
            tap_reading_config_choice(&mut app, "Contrast", "Higher contrast"),
            Change::Partial
        );
        assert!(app.fixed.as_ref().unwrap().reading.scan_contrast());
        assert!(app.activate(Action::ScanGamma(10)).is_err());
        assert!(app.fixed.as_ref().unwrap().reading.scan_contrast());
        assert_eq!(app.current_history_location(), location);
        let fonts = [
            crate::test_fixture("fonts/NotoSans-Regular.ttf"),
            crate::test_fixture("fonts/NotoSans-Bold.ttf"),
        ];
        capture_test_screen(&app, &fonts, "READER_DJVU_SCAN_CONTRAST_FRAME", false);
        app.save().unwrap();

        let mut reopened = fixture.app();
        reopened.open_path(&document).unwrap();
        let reading = &reopened.fixed.as_ref().unwrap().reading;
        assert!(reading.scan_contrast());
        assert_eq!(reading.preferences().scan_gamma_tenths, None);
        assert_eq!(reopened.current_history_location(), location);
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
        assert_eq!(tap_reading_control(&mut app, "Adaptive"), Change::Partial);
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
        let reference: Reference = serde_json::from_slice(
            &fs::read(std::env::var_os("READER_DICTIONARY_REFERENCE").unwrap()).unwrap(),
        )
        .unwrap();
        let documents = PathBuf::from(std::env::var_os("PRIMITIVE_READER_DOCUMENTS").unwrap());
        let fixture = Fixture::new();
        let font = crate::test_fixture("fonts/NotoSans-Regular.ttf");
        let mut app = Reader::open(&documents, &fixture.state, vec![font]).unwrap();
        app.activate(Action::ShowDictionaries).unwrap();
        assert_eq!(app.installed_dictionaries.len(), reference.expected_count);
        let mut looked_up = 0;
        for case in reference.cases {
            app.open_path(&case.book).unwrap();
            let probe = app
                .active
                .as_ref()
                .unwrap()
                .reading
                .config
                .chapter(case.chapter)
                .unwrap();
            let passage = probe
                .source
                .blocks
                .iter()
                .find_map(|b| {
                    let range = crate::selection::word_ranges(&b.text)
                        .find(|range| b.text[range.clone()] == case.word)?;
                    probe.select_range(
                        &probe.anchor(b.key, range.start),
                        &probe.anchor(b.key, range.end),
                    )
                })
                .expect("Reference word occurs in the specified chapter");
            app.restore_position(&ReadingPosition {
                chapter: case.chapter,
                page: 0,
                anchor: Some(passage.start.clone()),
            })
            .unwrap();
            let origin = app.current_history_location().unwrap();
            let bounds = app
                .active
                .as_ref()
                .unwrap()
                .reading
                .highlight_rects(&passage)[0];
            let (x, y) =
                rectangle_center(Rect::new(bounds.x, bounds.y, bounds.width, bounds.height));
            app.input(Input::LongTap(x, y));
            assert!(
                app.selection
                    .as_ref()
                    .is_some_and(|selected| selected.text == case.word)
            );
            let start = Instant::now();
            tap_selection_action(&mut app, selection_popup::SelectionAction::Dictionary);
            assert_eq!(app.screen, Screen::Definition);
            assert!(app.definition_over_book());
            let deadline = Instant::now() + Duration::from_secs(180);
            while app.spelling_lookup.is_some() {
                assert!(
                    Instant::now() < deadline,
                    "Dictionary suggestions exceeded three minutes"
                );
                app.tick(Instant::now());
                if app.spelling_lookup.is_some() {
                    std::thread::sleep(Duration::from_millis(20));
                }
            }
            assert_eq!(
                app.definitions.len(),
                case.results.len(),
                "dictionary result count for case {looked_up}"
            );
            let mut remaining: Vec<_> = app.definitions.iter().collect();
            for (dictionary, word, visible_digest) in case.results {
                let index = remaining
                    .iter()
                    .position(|entry| {
                        if entry.dictionary != dictionary || entry.word != word {
                            return false;
                        }
                        visible_digest.as_ref().is_none_or(|digest| {
                            let normalized =
                                entry.text.split_whitespace().collect::<Vec<_>>().join(" ");
                            format!("{:x}", md5::compute(normalized)) == *digest
                        })
                    })
                    .expect("Same dictionary, headword and visible definition as the stock result");
                remaining.remove(index);
            }
            eprintln!(
                "Dictionary check {} took {} ms",
                looked_up + 1,
                start.elapsed().as_millis()
            );
            let (x, y) = rectangle_center(app.definition_layout().1);
            app.input(Input::Tap(x, y));
            assert_eq!(app.screen, Screen::Book);
            assert!(app.current_history_location().as_ref() == Some(&origin));
            assert!(
                app.selection
                    .as_ref()
                    .is_some_and(|selected| selected.text == case.word)
            );
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
        let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
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
        app.input(Input::Tap(next.0, next.1));
        assert_eq!(app.selection.as_ref().unwrap().text, "The first");
        capture_test_screen(&app, &fonts, "READER_SELECTION_COPY_SCREENSHOT", false);
        assert_eq!(
            tap_selection_action(&mut app, SelectionAction::Copy),
            Change::Partial
        );
        assert_eq!(app.clipboard_text.as_deref(), Some("The first"));
        assert!(app.selection.is_none());
        assert_eq!(app.notice.as_deref(), Some("Selection copied to clipboard"));
        for screen in [
            Screen::Note,
            Screen::EditNote,
            Screen::Find,
            Screen::LibraryFind,
            Screen::BookmarkFind,
            Screen::DictionaryEntry,
            Screen::VocabularySearch,
        ] {
            app.show(screen);
            app.editor = Editor::new(128);
            assert!(
                app.can_paste_text(),
                "Paste should be available on {screen:?}"
            );
            let (x, y) = rectangle_center(external_link::PASTE);
            assert_eq!(app.input(Input::Tap(x, y)), Change::Partial);
            assert_eq!(app.editor.string(), "The first", "Paste into {screen:?}");
        }

        app.show(Screen::Book);
        app.input(Input::LongTap(first.0, first.1));
        let (x, y) = rectangle_center(DONE);
        assert_eq!(app.input(Input::Tap(x, y)), Change::Exit);
        let mut reopened = fixture.app();
        reopened.activate(Action::Continue).unwrap();
        assert_eq!(position(&reopened.active.as_ref().unwrap().reading), saved);
    }

    #[test]
    fn highlight_prompt_applies_chosen_style_and_color_without_changing_defaults() {
        use crate::highlight_style::{HighlightColor, HighlightStyle};
        use crate::reading_controls::{HighlightPrompt, SelectionLongPressAction};
        use selection_popup::{HighlightPromptStage, SelectionAction};

        let fixture = Fixture::new();
        let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
        let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
        app.open_path(&fixture.book).unwrap();
        app.controls.highlight_prompt = HighlightPrompt::StyleAndColor;
        let reading = &app.active.as_ref().unwrap().reading;
        let origin = reading.probe.fragment("tide-note").unwrap();
        let mut start = origin.clone();
        start.offset += "The ".len();
        let mut end = start.clone();
        end.offset += "first".len();
        app.selection = Some(reading.probe.select_range(&start, &end).unwrap());
        let id = app.open_book_id().unwrap();
        let original_book_style = app.library.state(id).unwrap().highlight_style;
        let original_book_color = app.library.state(id).unwrap().highlight_color;

        assert_eq!(
            app.apply_selection_long_press_action(SelectionLongPressAction::Highlight),
            Change::Partial
        );
        assert_eq!(
            app.highlight_prompt_stage,
            Some(HighlightPromptStage::Style)
        );
        assert!(app.library.state(id).unwrap().highlights.is_empty());
        capture_test_screen(
            &app,
            &fonts,
            "READER_HIGHLIGHT_STYLE_PROMPT_SCREENSHOT",
            false,
        );

        assert_eq!(
            tap_selection_action(
                &mut app,
                SelectionAction::PromptStyle(HighlightStyle::Underline)
            ),
            Change::Partial
        );
        assert_eq!(
            app.highlight_prompt_stage,
            Some(HighlightPromptStage::Color)
        );
        assert_eq!(app.selection.as_ref().unwrap().text, "first");
        assert!(app.library.state(id).unwrap().highlights.is_empty());
        capture_test_screen(
            &app,
            &fonts,
            "READER_HIGHLIGHT_COLOR_PROMPT_SCREENSHOT",
            false,
        );

        assert_eq!(
            tap_selection_action(&mut app, SelectionAction::CancelPrompt),
            Change::Partial
        );
        assert_eq!(app.highlight_prompt_stage, None);
        assert_eq!(app.selection.as_ref().unwrap().text, "first");
        assert!(app.library.state(id).unwrap().highlights.is_empty());

        tap_selection_action(&mut app, SelectionAction::Highlight);
        tap_selection_action(
            &mut app,
            SelectionAction::PromptStyle(HighlightStyle::Underline),
        );
        let button = app.selection_button(SelectionAction::PromptColor(HighlightColor::Cyan));
        let mut canvas = Canvas::new(WIDTH, HEIGHT, &[fonts[0].display().to_string()]).unwrap();
        let before = primitive_app::frame(&app, &mut canvas, false);
        assert!(canvas.press_at(
            (button.x + button.w / 2) as f32,
            (button.y + button.h / 2) as f32,
        ));
        let pressed = primitive_app::frame(&app, &mut canvas, false);
        assert_ne!(before, pressed);
        for (index, (before, after)) in before.iter().zip(&pressed).enumerate() {
            if before != after {
                assert!(button.contains(
                    (index % WIDTH as usize) as f32,
                    (index / WIDTH as usize) as f32,
                ));
            }
        }
        canvas.clear_press();
        assert_eq!(
            tap_selection_action(&mut app, SelectionAction::PromptColor(HighlightColor::Cyan)),
            Change::Partial
        );
        let state = app.library.state(id).unwrap();
        assert_eq!(state.highlights.len(), 1);
        assert_eq!(state.highlights[0].selection.text, "first");
        assert_eq!(state.highlights[0].style, HighlightStyle::Underline);
        assert_eq!(state.highlights[0].color, HighlightColor::Cyan);
        assert_eq!(state.highlight_style, original_book_style);
        assert_eq!(state.highlight_color, original_book_color);
        assert_eq!(app.highlight_prompt_stage, None);
        assert!(app.selection.is_none());
    }

    #[test]
    fn highlight_prompt_saves_the_chosen_color_for_pdf_text() {
        use crate::highlight_style::HighlightColor;
        use crate::reading_controls::HighlightPrompt;
        use selection_popup::{HighlightPromptStage, SelectionAction};

        let fixture = Fixture::new();
        let path = fixture.root.join("Prompt.pdf");
        let content = "BT /F1 14 Tf 55 60 Td (PDF sample words) Tj ET";
        let objects = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
            "<< /Type /Pages /Count 1 /Kids [3 0 R] /Resources << /Font << /F1 5 0 R >> >> >>"
                .to_owned(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 240 120] /Contents 4 0 R >>".to_owned(),
            format!(
                "<< /Length {} >>\nstream\n{content}\nendstream",
                content.len()
            ),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_owned(),
        ];
        fs::write(&path, crate::fixed::tests::pdf_objects(&objects)).unwrap();
        let mut app = fixture.app();
        app.open_path(&path).unwrap();
        app.controls.highlight_prompt = HighlightPrompt::Color;
        let fixed = &app.fixed.as_ref().unwrap().reading;
        let text = fixed.page_text(0).unwrap();
        let bounds = text.words[1].bounds;
        let point = hayro::kurbo::Point::new(
            f64::from((bounds.x0 + bounds.x1) / 2.0),
            f64::from((bounds.y0 + bounds.y1) / 2.0),
        );
        app.pdf_selection = Some(PdfSelection::at(0, point, text).unwrap());
        let id = app.open_book_id().unwrap();

        assert_eq!(
            tap_selection_action(&mut app, SelectionAction::Highlight),
            Change::Partial
        );
        assert_eq!(
            app.highlight_prompt_stage,
            Some(HighlightPromptStage::Color)
        );
        assert!(app.library.state(id).unwrap().pdf_highlights.is_empty());
        assert_eq!(
            tap_selection_action(&mut app, SelectionAction::PromptColor(HighlightColor::Cyan)),
            Change::Partial
        );
        let marks = &app.library.state(id).unwrap().pdf_highlights;
        assert_eq!(marks.len(), 1);
        assert_eq!(marks[0].text, "sample");
        assert_eq!(marks[0].color, HighlightColor::Cyan);
        assert_eq!(app.highlight_prompt_stage, None);
    }

    #[test]
    #[ignore = "requires an authorized original DRM-free EPUB path in READER_PARITY_EPUB"]
    fn highlight_prompt_on_an_original_epub_preserves_the_source_file() {
        use crate::highlight_style::{HighlightColor, HighlightStyle};
        use crate::reading_controls::HighlightPrompt;
        use selection_popup::SelectionAction;

        let original = PathBuf::from(
            std::env::var_os("READER_PARITY_EPUB")
                .expect("set READER_PARITY_EPUB to an authorized original document"),
        );
        let original_bytes = fs::read(&original).unwrap();
        let fixture = Fixture::new();
        let path = fixture.root.join("Original.epub");
        fs::write(&path, &original_bytes).unwrap();
        let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
        let mut app = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
        app.open_path(&path).unwrap();
        app.controls.highlight_prompt = HighlightPrompt::StyleAndColor;
        let chapter_count = app.active.as_ref().unwrap().reading.probe.chapter_count();
        let (selected, word) = (0..chapter_count)
            .find_map(|chapter| {
                app.active.as_mut()?.reading.go_to_page(chapter, 0).ok()?;
                let reading = &app.active.as_ref()?.reading;
                let (block, word) = reading.probe.source.blocks.iter().find_map(|block| {
                    block
                        .text
                        .split_whitespace()
                        .find(|word| word.chars().any(char::is_alphanumeric))
                        .map(|word| (block, word.to_owned()))
                })?;
                let start = block.text.find(&word)?;
                let selected = reading.probe.select_range(
                    &reading.probe.anchor(block.key, start),
                    &reading.probe.anchor(block.key, start + word.len()),
                )?;
                Some((selected, word))
            })
            .expect("original publication has selectable words");
        app.selection = Some(selected);
        let id = app.open_book_id().unwrap();
        let previous_style = app.library.state(id).unwrap().highlight_style;
        let previous_color = app.library.state(id).unwrap().highlight_color;

        tap_selection_action(&mut app, SelectionAction::Highlight);
        tap_selection_action(
            &mut app,
            SelectionAction::PromptStyle(HighlightStyle::Underline),
        );
        tap_selection_action(&mut app, SelectionAction::PromptColor(HighlightColor::Cyan));

        let marks = &app.library.state(id).unwrap().highlights;
        assert_eq!(marks.len(), 1);
        assert_eq!(marks[0].selection.text, word);
        assert_eq!(marks[0].style, HighlightStyle::Underline);
        assert_eq!(marks[0].color, HighlightColor::Cyan);
        assert_eq!(
            app.library.state(id).unwrap().highlight_style,
            previous_style
        );
        assert_eq!(
            app.library.state(id).unwrap().highlight_color,
            previous_color
        );
        assert_eq!(fs::read(&original).unwrap(), original_bytes);
    }

    #[test]
    fn selection_popup_position_matches_koreader_choices_and_persists() {
        use crate::reading_controls::SelectionPopupPosition as Position;

        let fixture = Fixture::new();
        let font = crate::test_fixture("fonts/NotoSans-Regular.ttf");
        let mut app = Reader::open(&fixture.root, &fixture.state, vec![font.clone()]).unwrap();
        app.open_path(&fixture.book).unwrap();
        app.controls.dictionary_on_single_word = Some(false);
        assert_eq!(app.controls.selection_popup_position, Position::Center);

        let item = app
            .top_config_items(TopGroup::Settings)
            .into_iter()
            .find(|item| item.title == "Selection dialog position")
            .unwrap();
        assert_eq!(item.detail.as_deref(), Some("Center"));
        assert!(app.reading_settings_items().iter().any(|item| {
            item.title == "Selection dialog position" && item.detail.as_deref() == Some("Center")
        }));
        app.activate(Action::Show(Screen::SelectionPosition))
            .unwrap();
        let options = app.items();
        assert_eq!(options.len(), Position::ALL.len());
        assert!(
            options.iter().any(|item| {
                item.title == "Center" && item.detail.as_deref() == Some("Selected")
            })
        );
        let selected = options
            .into_iter()
            .find(|item| item.title == "By selected text")
            .unwrap();
        app.activate(selected.action).unwrap();
        assert_eq!(app.controls.selection_popup_position, Position::Highlight);
        assert_eq!(
            crate::reading_controls::ReadingControls::load(&app.controls_path, false)
                .selection_popup_position,
            Position::Highlight
        );

        let reading = &app.active.as_ref().unwrap().reading;
        let target = Book::resolve_link(&reading.probe.chapter, "#tide-note").unwrap();
        app.follow(target).unwrap();
        let reading = &app.active.as_ref().unwrap().reading;
        let origin = reading.probe.fragment("tide-note").unwrap();
        let mut start = origin.clone();
        start.offset += "The ".len();
        let mut end = start.clone();
        end.offset += "first".len();
        let selected_text = reading.probe.select_range(&start, &end).unwrap();
        let rect = reading.highlight_rects(&selected_text)[0];
        let point = rectangle_center(Rect::new(rect.x, rect.y, rect.width, rect.height));
        app.input(Input::LongTap(point.0, point.1));
        let anchored = app.selection_popup_rect();
        assert!(
            anchored.y >= Rect::new(rect.x, rect.y, rect.width, rect.height).bottom()
                || anchored.bottom() <= rect.y
        );

        app.activate(Action::SetReadingControls(ReadingControls {
            selection_popup_position: Position::Center,
            ..app.controls
        }))
        .unwrap();
        let centered = app.selection_popup_rect();
        assert_eq!(centered.y, (PAGE_HEIGHT as i32 - centered.h) / 2);
        assert_eq!(centered.x, (WIDTH as i32 - centered.w) / 2);
        if let Some(path) = std::env::var_os("READER_SELECTION_CENTER_SCREENSHOT") {
            let mut canvas = Canvas::new(WIDTH, HEIGHT, &[font.display().to_string()]).unwrap();
            let frame = primitive_app::frame(&app, &mut canvas, false);
            primitive_app::write_png(std::path::Path::new(&path), WIDTH, HEIGHT, &frame).unwrap();
        }
    }

    #[test]
    fn long_press_settings_match_koreader_actions_and_persist() {
        use crate::reading_controls::SelectionLongPressAction as ActionChoice;

        let fixture = Fixture::new();
        let mut app = Reader::open(
            &fixture.root,
            &fixture.state,
            vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")],
        )
        .unwrap();
        app.open_path(&fixture.book).unwrap();
        assert!(app.reading_settings_items().iter().any(|item| {
            item.title == "Long-press on text"
                && item.detail.as_deref()
                    == Some("Dictionary on one word · otherwise Ask with popup dialog")
        }));
        app.activate(Action::Show(Screen::LongPressSettings))
            .unwrap();
        let items = app.items();
        assert_eq!(items.len(), ActionChoice::ALL.len() + 4);
        assert_eq!(items[0].title, "Dictionary on a single word");
        assert_eq!(
            items[0].detail.as_deref(),
            Some("On · opens the local dictionary")
        );
        let corner_scroll = items
            .iter()
            .find(|item| item.title == "Auto-scroll when selection reaches a corner")
            .unwrap();
        assert_eq!(corner_scroll.detail.as_deref(), Some("On"));
        assert!(items.iter().any(|item| {
            item.title == "Ask with popup dialog" && item.detail.as_deref() == Some("Selected")
        }));
        if let Some(path) = std::env::var_os("READER_LONG_PRESS_SETTINGS_SCREENSHOT") {
            let font = crate::test_fixture("fonts/NotoSans-Regular.ttf");
            let mut canvas = Canvas::new(WIDTH, HEIGHT, &[font.display().to_string()]).unwrap();
            let frame = primitive_app::frame(&app, &mut canvas, false);
            primitive_app::write_png(std::path::Path::new(&path), WIDTH, HEIGHT, &frame).unwrap();
        }
        let corner_scroll_action = app
            .items()
            .into_iter()
            .find(|item| item.title == "Auto-scroll when selection reaches a corner")
            .unwrap()
            .action;
        app.activate(corner_scroll_action).unwrap();
        assert!(!app.controls.selection_corner_scroll_enabled());
        assert_eq!(
            ReadingControls::load(&app.controls_path, false).selection_corner_scroll,
            Some(false)
        );
        let corner_scroll = app
            .items()
            .into_iter()
            .find(|item| item.title == "Auto-scroll when selection reaches a corner")
            .unwrap();
        app.activate(corner_scroll.action).unwrap();
        assert!(app.controls.selection_corner_scroll_enabled());
        assert_eq!(
            ReadingControls::load(&app.controls_path, false).selection_corner_scroll,
            Some(true)
        );
        let prompt = items
            .iter()
            .find(|item| item.title == "Highlight style and color prompt")
            .unwrap();
        assert_eq!(prompt.detail.as_deref(), Some("Off"));
        let interval = items
            .iter()
            .find(|item| item.title == "Very long press interval")
            .unwrap();
        assert_eq!(interval.detail.as_deref(), Some("3.0 seconds"));
        let interval_action = app
            .items()
            .into_iter()
            .find(|item| item.title == "Very long press interval")
            .unwrap()
            .action;
        app.activate(interval_action).unwrap();
        assert_eq!(app.screen, Screen::VeryLongPressSettings);
        let intervals = app.items();
        assert_eq!(intervals.len(), 5);
        app.activate(
            intervals
                .into_iter()
                .find(|item| item.title == "+0.5 seconds")
                .unwrap()
                .action,
        )
        .unwrap();
        assert_eq!(app.controls.very_long_press_tenths(), 35);
        assert_eq!(
            ReadingControls::load(&app.controls_path, false).very_long_press_tenths(),
            35
        );
        app.activate(Action::Show(Screen::LongPressSettings))
            .unwrap();
        app.activate(Action::Show(Screen::HighlightPromptSettings))
            .unwrap();
        let choices = app.items();
        assert_eq!(choices.len(), HighlightPrompt::ALL.len());
        assert!(
            choices
                .iter()
                .any(|item| { item.title == "Off" && item.detail.as_deref() == Some("Selected") })
        );
        app.activate(
            choices
                .into_iter()
                .find(|item| item.title == "Style and color")
                .unwrap()
                .action,
        )
        .unwrap();
        assert_eq!(
            app.controls.highlight_prompt,
            HighlightPrompt::StyleAndColor
        );
        assert_eq!(
            ReadingControls::load(&app.controls_path, false).highlight_prompt,
            HighlightPrompt::StyleAndColor
        );
        app.activate(Action::Show(Screen::LongPressSettings))
            .unwrap();
        let items = app.items();
        let selected = items
            .into_iter()
            .find(|item| item.title == "Do nothing")
            .unwrap();
        app.activate(selected.action).unwrap();
        assert_eq!(
            app.controls.selection_long_press_action,
            ActionChoice::Nothing
        );
        assert_eq!(
            crate::reading_controls::ReadingControls::load(&app.controls_path, false)
                .selection_long_press_action,
            ActionChoice::Nothing
        );
    }

    #[test]
    fn long_press_selection_actions_run_natively_and_retain_reading_context() {
        use crate::reading_controls::SelectionLongPressAction as ActionChoice;

        for action in ActionChoice::ALL {
            let fixture = Fixture::new();
            let mut app = fixture.app();
            app.open_path(&fixture.book).unwrap();
            app.controls.dictionary_on_single_word = Some(false);
            app.controls.selection_long_press_action = action;
            let reading = &app.active.as_ref().unwrap().reading;
            let origin = reading.probe.fragment("tide-note").unwrap();
            let mut start = origin.clone();
            start.offset += "The ".len();
            let mut end = start.clone();
            end.offset += "first".len();
            let selected = reading.probe.select_range(&start, &end).unwrap();
            let bounds = reading.highlight_rects(&selected)[0];
            let point =
                rectangle_center(Rect::new(bounds.x, bounds.y, bounds.width, bounds.height));
            let position = app.current_history_location();

            assert_eq!(
                app.input(Input::LongTap(point.0, point.1)),
                Change::Partial,
                "{}",
                action.label()
            );
            match action {
                ActionChoice::Ask => {
                    assert_eq!(app.screen, Screen::Book);
                    assert_eq!(app.selection.as_ref().unwrap().text, "first");
                    assert!(!app.selection_mode);
                }
                ActionChoice::Select => {
                    assert_eq!(app.screen, Screen::Book);
                    assert_eq!(app.selection.as_ref().unwrap().text, "first");
                    assert!(app.selection_mode);
                    assert_eq!(app.selection_popup_action_count(), 3);
                }
                ActionChoice::Nothing => {
                    assert_eq!(app.screen, Screen::Book);
                    assert!(app.selection.is_none());
                }
                ActionChoice::Highlight => {
                    assert_eq!(app.screen, Screen::Book);
                    assert!(app.selection.is_none());
                    assert_eq!(
                        app.library
                            .state(app.open_book_id().unwrap())
                            .unwrap()
                            .highlights
                            .len(),
                        1
                    );
                }
                ActionChoice::Note => assert_eq!(app.screen, Screen::Note),
                ActionChoice::Translate | ActionChoice::Wikipedia => {
                    assert_eq!(app.screen, Screen::OnlineLookup);
                    assert!(app.online.is_some());
                }
                ActionChoice::Dictionary => assert_eq!(app.screen, Screen::Definition),
                ActionChoice::Search => {
                    assert_eq!(app.screen, Screen::Book);
                    assert!(app.nearby.searching_direction());
                }
            }
            assert_eq!(
                app.current_history_location(),
                position,
                "{}",
                action.label()
            );
        }
    }

    #[test]
    fn select_and_highlight_mode_extends_a_reflow_selection_and_can_cancel_or_save() {
        use crate::reading_controls::SelectionLongPressAction;
        use selection_popup::SelectionAction;

        let fixture = Fixture::new();
        let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
        let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
        app.controls.dictionary_on_single_word = Some(false);
        app.open_path(&fixture.book).unwrap();
        app.controls.selection_long_press_action = SelectionLongPressAction::Select;

        let reading = &app.active.as_ref().unwrap().reading;
        let origin = reading.probe.fragment("tide-note").unwrap();
        let mut first = origin.clone();
        first.offset += "The ".len();
        let mut first_end = first.clone();
        first_end.offset += "first".len();
        let selected = reading.probe.select_range(&first, &first_end).unwrap();
        let rect = reading.highlight_rects(&selected)[0];
        let start_point = rectangle_center(Rect::new(rect.x, rect.y, rect.width, rect.height));

        let mut earlier_end = origin.clone();
        earlier_end.offset += "The".len();
        let earlier = reading.probe.select_range(&origin, &earlier_end).unwrap();
        let rect = reading.highlight_rects(&earlier)[0];
        let earlier_point = rectangle_center(Rect::new(rect.x, rect.y, rect.width, rect.height));

        assert_eq!(
            app.input(Input::LongTap(start_point.0, start_point.1)),
            Change::Partial
        );
        assert!(app.selection_mode);
        assert_eq!(app.selection.as_ref().unwrap().text, "first");
        assert_eq!(app.selection_popup_action_count(), 3);
        assert_eq!(
            app.input(Input::Tap(earlier_point.0, earlier_point.1)),
            Change::Partial
        );
        assert_eq!(app.selection.as_ref().unwrap().text, "The first");
        capture_test_screen(&app, &fonts, "READER_SELECT_MODE_SCREENSHOT", false);
        let font_paths = fonts
            .iter()
            .map(|font| font.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        let mut canvas = Canvas::new(WIDTH, HEIGHT, &font_paths).unwrap();
        let plain = primitive_app::frame(&app, &mut canvas, false);
        for action in [SelectionAction::Cancel, SelectionAction::Highlight] {
            let button = app.selection_button(action);
            assert!(canvas.press_at(
                (button.x + button.w / 2) as f32,
                (button.y + button.h / 2) as f32,
            ));
            let pressed = primitive_app::frame(&app, &mut canvas, false);
            assert_ne!(plain, pressed);
            for (index, (before, after)) in plain.iter().zip(&pressed).enumerate() {
                if before != after {
                    assert!(button.contains(
                        (index % WIDTH as usize) as f32,
                        (index / WIDTH as usize) as f32,
                    ));
                }
            }
            canvas.clear_press();
        }
        assert_eq!(
            tap_selection_action(&mut app, SelectionAction::Cancel),
            Change::Partial
        );
        assert!(!app.selection_mode);
        assert!(app.selection.is_none());
        assert!(
            app.library
                .state(app.open_book_id().unwrap())
                .unwrap()
                .highlights
                .is_empty()
        );

        assert_eq!(
            app.input(Input::LongTap(start_point.0, start_point.1)),
            Change::Partial
        );
        assert!(app.selection_mode);
        assert_eq!(
            app.input(Input::LongTap(earlier_point.0, earlier_point.1)),
            Change::Partial
        );
        assert_eq!(
            tap_selection_action(&mut app, SelectionAction::Highlight),
            Change::Partial
        );
        assert!(!app.selection_mode);
        assert!(app.selection.is_none());
        let state = app.library.state(app.open_book_id().unwrap()).unwrap();
        assert_eq!(state.highlights.len(), 1);
        assert_eq!(state.highlights[0].selection.text, "The first");
    }

    #[test]
    fn select_and_highlight_mode_extends_and_saves_a_pdf_selection() {
        use crate::reading_controls::SelectionLongPressAction;
        use selection_popup::SelectionAction;

        let fixture = Fixture::new();
        let path = fixture.root.join("Selection.pdf");
        let content = "BT /F1 14 Tf 55 60 Td (PDF sample words) Tj ET";
        let objects = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
            "<< /Type /Pages /Count 1 /Kids [3 0 R] /Resources << /Font << /F1 5 0 R >> >> >>"
                .to_owned(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 240 120] /Contents 4 0 R >>".to_owned(),
            format!(
                "<< /Length {} >>\nstream\n{content}\nendstream",
                content.len()
            ),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_owned(),
        ];
        fs::write(&path, crate::fixed::tests::pdf_objects(&objects)).unwrap();
        let mut app = fixture.app();
        app.open_path(&path).unwrap();
        app.controls.dictionary_on_single_word = Some(false);
        app.controls.selection_long_press_action = SelectionLongPressAction::Select;
        let fixed = &app.fixed.as_ref().unwrap().reading;
        let text = fixed.page_text(0).unwrap();
        let (size, viewport) = fixed.page_viewport().unwrap();
        let first = text.words[0]
            .bounds
            .in_viewport(size, viewport)
            .unwrap()
            .center();
        let last = text.words[2]
            .bounds
            .in_viewport(size, viewport)
            .unwrap()
            .center();

        assert_eq!(
            app.input(Input::LongTap(first.x as f32, first.y as f32)),
            Change::Partial
        );
        assert!(app.selection_mode);
        assert_eq!(app.pdf_selection.as_ref().unwrap().text(), "PDF");
        assert_eq!(app.selection_popup_action_count(), 3);
        assert_eq!(
            app.input(Input::Tap(last.x as f32, last.y as f32)),
            Change::Partial
        );
        assert_eq!(
            app.pdf_selection.as_ref().unwrap().text(),
            "PDF sample words"
        );
        assert_eq!(
            tap_selection_action(&mut app, SelectionAction::Highlight),
            Change::Partial
        );
        assert!(!app.selection_mode);
        assert!(app.pdf_selection.is_none());
        let state = app.library.state(app.open_book_id().unwrap()).unwrap();
        assert_eq!(state.pdf_highlights.len(), 1);
        assert_eq!(state.pdf_highlights[0].text, "PDF sample words");
    }

    #[test]
    fn pdf_select_mode_spans_pages_in_both_directions_and_saves_continuations() {
        use crate::reading_controls::SelectionLongPressAction;
        use selection_popup::SelectionAction;

        let fixture = Fixture::new();
        let path = fixture.root.join("Two-page selection.pdf");
        let page_one = "BT /F1 14 Tf 30 80 Td (first page ends here) Tj ET";
        let page_two = "BT /F1 14 Tf 30 80 Td (second page starts here) Tj ET";
        let objects = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
            "<< /Type /Pages /Count 2 /Kids [3 0 R 6 0 R] /Resources << /Font << /F1 5 0 R >> >> >>"
                .to_owned(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 240 120] /Contents 4 0 R >>"
                .to_owned(),
            format!(
                "<< /Length {} >>\nstream\n{page_one}\nendstream",
                page_one.len()
            ),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_owned(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 240 120] /Contents 7 0 R >>"
                .to_owned(),
            format!(
                "<< /Length {} >>\nstream\n{page_two}\nendstream",
                page_two.len()
            ),
        ];
        fs::write(&path, crate::fixed::tests::pdf_objects(&objects)).unwrap();
        let mut app = fixture.app();
        app.open_path(&path).unwrap();
        app.controls.dictionary_on_single_word = Some(false);
        app.controls.selection_long_press_action = SelectionLongPressAction::Select;
        let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
        let turn = |app: &mut Reader, input| {
            let change = app.input(input);
            if change != Change::None {
                return change;
            }
            let deadline = app.deadline().expect("queued fixed page turn deadline");
            app.tick(deadline)
        };

        let page_one_text = app.fixed.as_ref().unwrap().reading.page_text(0).unwrap();
        let page_one_bounds = page_one_text.words[0].bounds;
        let first = app
            .fixed
            .as_ref()
            .unwrap()
            .reading
            .visible_rectangle(0, page_one_bounds)
            .unwrap()
            .center();
        assert_eq!(
            app.input(Input::LongTap(first.x as f32, first.y as f32)),
            Change::Partial
        );
        assert!(app.selection_mode);
        assert_eq!(turn(&mut app, Input::PageForward), Change::Partial);
        assert_eq!(app.fixed.as_ref().unwrap().reading.page(), 1);
        assert!(
            app.pdf_selection.is_some(),
            "selection must survive a page turn"
        );

        let page_two_text = app.fixed.as_ref().unwrap().reading.page_text(1).unwrap();
        let expected_forward = format!(
            "{}\n{}",
            &page_one_text.text
                [page_one_text.words[0].bytes.start..page_one_text.words.last().unwrap().bytes.end],
            &page_two_text.text
                [page_two_text.words[0].bytes.start..page_two_text.words[1].bytes.end]
        );
        let end = app
            .fixed
            .as_ref()
            .unwrap()
            .reading
            .visible_rectangle(1, page_two_text.words[1].bounds)
            .unwrap()
            .center();
        assert_eq!(
            app.input(Input::LongTap(end.x as f32, end.y as f32)),
            Change::Partial
        );
        let selection = app.pdf_selection.as_ref().unwrap();
        assert_eq!(selection.text(), expected_forward);
        assert_eq!(
            selection
                .page_bounds()
                .map(|(page, _)| page)
                .collect::<std::collections::BTreeSet<_>>(),
            [0, 1].into_iter().collect()
        );
        capture_test_screen(
            &app,
            &fonts,
            "READER_PDF_CROSS_PAGE_SELECTION_SCREENSHOT",
            false,
        );
        assert_eq!(
            tap_selection_action(&mut app, SelectionAction::Highlight),
            Change::Partial
        );
        let state = app.library.state(app.open_book_id().unwrap()).unwrap();
        assert_eq!(state.pdf_highlights.len(), 1);
        assert_eq!(state.pdf_highlights[0].text, expected_forward);
        assert_eq!(state.pdf_highlights[0].continuation.len(), 1);
        assert_eq!(state.pdf_highlights[0].continuation[0].page, 1);
        let book_id = app.open_book_id().unwrap();

        app.save().unwrap();
        let mut reopened = fixture.app();
        reopened.open_path(&path).unwrap();
        let reopened_marks = &reopened.library.state(book_id).unwrap().pdf_highlights;
        assert_eq!(reopened_marks.len(), 1);
        assert_eq!(reopened_marks[0].text, expected_forward);
        assert_eq!(reopened_marks[0].continuation.len(), 1);
        assert_eq!(reopened_marks[0].continuation[0].page, 1);
        assert_eq!(reopened.fixed.as_ref().unwrap().reading.page(), 1);
        reopened.controls.selection_long_press_action = SelectionLongPressAction::Select;
        let reverse_start = reopened
            .fixed
            .as_ref()
            .unwrap()
            .reading
            .visible_rectangle(1, page_two_text.words[0].bounds)
            .unwrap()
            .center();
        assert_eq!(
            reopened.input(Input::LongTap(
                reverse_start.x as f32,
                reverse_start.y as f32
            )),
            Change::Partial
        );
        assert_eq!(turn(&mut reopened, Input::PageBack), Change::Partial);
        let page_zero_last = reopened
            .fixed
            .as_ref()
            .unwrap()
            .reading
            .visible_rectangle(0, page_one_text.words.last().unwrap().bounds)
            .unwrap()
            .center();
        assert_eq!(
            reopened.input(Input::LongTap(
                page_zero_last.x as f32,
                page_zero_last.y as f32
            )),
            Change::Partial
        );
        assert_eq!(
            reopened.pdf_selection.as_ref().unwrap().text(),
            "here\nsecond"
        );
        assert_eq!(
            tap_selection_action(&mut reopened, selection_popup::SelectionAction::Copy),
            Change::Partial
        );
        assert_eq!(reopened.clipboard_text.as_deref(), Some("here\nsecond"));
        assert!(reopened.pdf_selection.is_none());
    }

    #[test]
    fn pdf_select_mode_rejects_unreadable_intervening_pages_without_losing_selection() {
        use crate::reading_controls::SelectionLongPressAction;

        let fixture = Fixture::new();
        let path = fixture.root.join("Unreadable middle page.pdf");
        let first = "BT /F1 14 Tf 30 80 Td (first selectable page) Tj ET";
        let blank = "q Q";
        let last = "BT /F1 14 Tf 30 80 Td (last selectable page) Tj ET";
        let objects = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
            "<< /Type /Pages /Count 3 /Kids [3 0 R 6 0 R 8 0 R] /Resources << /Font << /F1 5 0 R >> >> >>".to_owned(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 240 120] /Contents 4 0 R >>".to_owned(),
            format!("<< /Length {} >>\nstream\n{first}\nendstream", first.len()),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_owned(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 240 120] /Contents 7 0 R >>".to_owned(),
            format!("<< /Length {} >>\nstream\n{blank}\nendstream", blank.len()),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 240 120] /Contents 9 0 R >>".to_owned(),
            format!("<< /Length {} >>\nstream\n{last}\nendstream", last.len()),
        ];
        fs::write(&path, crate::fixed::tests::pdf_objects(&objects)).unwrap();
        let mut app = fixture.app();
        app.open_path(&path).unwrap();
        app.controls.selection_long_press_action = SelectionLongPressAction::Select;
        let turn = |app: &mut Reader| {
            let change = app.input(Input::PageForward);
            if change != Change::None {
                return change;
            }
            let deadline = app.deadline().expect("queued fixed page turn deadline");
            app.tick(deadline)
        };

        let first_text = app.fixed.as_ref().unwrap().reading.page_text(0).unwrap();
        let start = app
            .fixed
            .as_ref()
            .unwrap()
            .reading
            .visible_rectangle(0, first_text.words[0].bounds)
            .unwrap()
            .center();
        assert_eq!(
            app.input(Input::LongTap(start.x as f32, start.y as f32)),
            Change::Partial
        );
        assert_eq!(app.pdf_selection.as_ref().unwrap().text(), "first");
        assert_eq!(turn(&mut app), Change::Partial);
        assert_eq!(turn(&mut app), Change::Partial);
        assert_eq!(app.fixed.as_ref().unwrap().reading.page(), 2);

        let last_text = app.fixed.as_ref().unwrap().reading.page_text(2).unwrap();
        let end = app
            .fixed
            .as_ref()
            .unwrap()
            .reading
            .visible_rectangle(2, last_text.words[1].bounds)
            .unwrap()
            .center();
        assert_eq!(
            app.input(Input::LongTap(end.x as f32, end.y as f32)),
            Change::Partial
        );
        assert_eq!(app.pdf_selection.as_ref().unwrap().text(), "first");
        assert_eq!(
            app.notice.as_deref(),
            Some("A readable text layer is needed on each selected page")
        );
    }

    #[test]
    fn stock_single_word_long_press_opens_dictionary_and_can_be_disabled() {
        let fixture = Fixture::new();
        let mut app = Reader::open(
            &fixture.root,
            &fixture.state,
            vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")],
        )
        .unwrap();
        app.open_path(&fixture.book).unwrap();
        assert_eq!(app.controls.dictionary_on_single_word, None);
        let reading = &app.active.as_ref().unwrap().reading;
        let start = reading.probe.fragment("tide-note").unwrap();
        let mut end = start.clone();
        end.offset += "The".len();
        let selected = reading.probe.select_range(&start, &end).unwrap();
        let bounds = reading.highlight_rects(&selected)[0];
        let point = rectangle_center(Rect::new(bounds.x, bounds.y, bounds.width, bounds.height));
        assert_eq!(app.input(Input::LongTap(point.0, point.1)), Change::Partial);
        assert!(app.selection.is_some());
        if app.pending_selection_action.is_some() {
            app.tick(Instant::now() + Duration::from_secs(2));
        }
        assert_eq!(app.screen, Screen::Definition);

        let mut disabled = fixture.app();
        disabled.open_path(&fixture.book).unwrap();
        disabled.controls.dictionary_on_single_word = Some(false);
        let reading = &disabled.active.as_ref().unwrap().reading;
        let start = reading.probe.fragment("tide-note").unwrap();
        let mut end = start.clone();
        end.offset += "The".len();
        let selected = reading.probe.select_range(&start, &end).unwrap();
        let bounds = reading.highlight_rects(&selected)[0];
        let point = rectangle_center(Rect::new(bounds.x, bounds.y, bounds.width, bounds.height));
        assert_eq!(
            disabled.input(Input::LongTap(point.0, point.1)),
            Change::Partial
        );
        assert_eq!(disabled.screen, Screen::Book);
        assert_eq!(disabled.selection.as_ref().unwrap().text, "The");
    }

    #[test]
    fn held_drag_extends_epub_selection_and_very_long_release_uses_ask() {
        let fixture = Fixture::new();
        let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
        let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
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
            let selected = reading.probe.select_range(&start, &end).unwrap();
            let rect = reading.highlight_rects(&selected)[0];
            rectangle_center(Rect::new(rect.x, rect.y, rect.width, rect.height))
        };
        let first = point(0, "The".len());
        let next = point("The ".len(), "first".len());

        assert_eq!(
            app.input(Input::HoldStart(first.0, first.1)),
            Change::Partial
        );
        assert_eq!(app.selection.as_ref().unwrap().text, "The");
        capture_test_screen(&app, &fonts, "READER_HOLD_SELECTION_SCREENSHOT", false);
        assert_eq!(app.input(Input::HoldPan(next.0, next.1)), Change::Partial);
        assert_eq!(app.selection.as_ref().unwrap().text, "The first");
        assert_eq!(
            app.input(Input::HoldRelease {
                start_x: first.0,
                start_y: first.1,
                x: next.0,
                y: next.1,
                held_ms: 1_500,
            }),
            Change::Partial
        );
        assert_eq!(app.screen, Screen::Book);

        assert_eq!(
            app.input(Input::HoldStart(first.0, first.1)),
            Change::Partial
        );
        assert_eq!(
            app.input(Input::HoldRelease {
                start_x: first.0,
                start_y: first.1,
                x: first.0,
                y: first.1,
                held_ms: 3_000,
            }),
            Change::Partial
        );
        assert_eq!(
            app.screen,
            Screen::Book,
            "very-long release asks instead of opening the one-word dictionary"
        );
        assert_eq!(app.selection.as_ref().unwrap().text, "The");
    }

    #[test]
    fn held_drag_extends_pdf_text_and_corner_mapping_follows_reading_direction() {
        let fixture = Fixture::new();
        let path = fixture.root.join("Held selection.pdf");
        let content = "BT /F1 14 Tf 55 60 Td (PDF sample words) Tj ET";
        let objects = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
            "<< /Type /Pages /Count 1 /Kids [3 0 R] /Resources << /Font << /F1 5 0 R >> >> >>"
                .to_owned(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 240 120] /Contents 4 0 R >>".to_owned(),
            format!(
                "<< /Length {} >>\nstream\n{content}\nendstream",
                content.len()
            ),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_owned(),
        ];
        fs::write(&path, crate::fixed::tests::pdf_objects(&objects)).unwrap();
        let mut app = fixture.app();
        app.open_path(&path).unwrap();
        app.controls.dictionary_on_single_word = Some(false);

        assert_eq!(app.touch_corner_direction(2.0, 2.0), Some(-3));
        assert_eq!(
            app.touch_corner_direction(WIDTH as f32 - 2.0, PAGE_HEIGHT as f32 - 2.0),
            Some(3)
        );
        assert_eq!(
            app.touch_corner_direction(2.0, PAGE_HEIGHT as f32 - 2.0),
            None
        );
        app.controls.selection_corner_scroll = Some(false);
        assert_eq!(app.touch_corner_direction(2.0, 2.0), None);
        app.controls.selection_corner_scroll = Some(true);

        let fixed = &app.fixed.as_ref().unwrap().reading;
        let text = fixed.page_text(0).unwrap();
        let (size, viewport) = fixed.page_viewport().unwrap();
        let first = text.words[0]
            .bounds
            .in_viewport(size, viewport)
            .unwrap()
            .center();
        let second = text.words[1]
            .bounds
            .in_viewport(size, viewport)
            .unwrap()
            .center();
        let expected = text.text[text.words[0].bytes.start..text.words[1].bytes.end].to_owned();

        assert_eq!(
            app.input(Input::HoldStart(first.x as f32, first.y as f32)),
            Change::Partial
        );
        assert_eq!(
            app.input(Input::HoldPan(second.x as f32, second.y as f32)),
            Change::Partial
        );
        assert_eq!(app.pdf_selection.as_ref().unwrap().text(), expected);
        assert_eq!(
            app.input(Input::HoldRelease {
                start_x: first.x as f32,
                start_y: first.y as f32,
                x: second.x as f32,
                y: second.y as f32,
                held_ms: 1_500,
            }),
            Change::Partial
        );
        assert_eq!(app.screen, Screen::Fixed);
        assert_eq!(app.pdf_selection.as_ref().unwrap().text(), expected);
    }

    #[test]
    fn searches_from_a_selection_on_reflow_and_rotated_pdf_pages() {
        let fixture = Fixture::new();
        let fonts = simulation_fonts();
        let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
        app.controls.dictionary_on_single_word = Some(false);
        app.controls.selection_search_all = true;
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
        assert_eq!(
            tap_selection_action(&mut app, selection_popup::SelectionAction::Find),
            Change::Partial
        );
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
        assert_eq!(
            tap_selection_action(&mut app, selection_popup::SelectionAction::Find),
            Change::Partial
        );
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
    fn epub_selection_search_uses_current_page_by_default_and_lists_all_when_enabled() {
        let fixture = Fixture::new();
        let fonts = vec![
            crate::test_fixture("fonts/NotoSans-Regular.ttf"),
            crate::test_fixture("fonts/NotoSans-Bold.ttf"),
        ];
        let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
        assert!(!app.controls.selection_search_all);
        app.controls.dictionary_on_single_word = Some(false);
        app.open_path(&fixture.book).unwrap();
        let reading = &app.active.as_ref().unwrap().reading;
        let target = Book::resolve_link(&reading.probe.chapter, "#tide-note").unwrap();
        app.follow(target).unwrap();
        let reading = &app.active.as_ref().unwrap().reading;
        let origin = reading.probe.fragment("tide-note").unwrap();
        let point = |offset, len| {
            let mut start = origin.clone();
            start.offset += offset;
            let mut end = start.clone();
            end.offset += len;
            let selection = reading.probe.select_range(&start, &end).unwrap();
            let bounds = reading.highlight_rects(&selection)[0];
            rectangle_center(Rect::new(bounds.x, bounds.y, bounds.width, bounds.height))
        };
        let first = point(0, "The".len());
        let next = point("The ".len(), "first".len());
        let select = |app: &mut Reader| {
            assert_eq!(app.input(Input::LongTap(first.0, first.1)), Change::Partial);
            assert_eq!(app.input(Input::Tap(next.0, next.1)), Change::Partial);
            assert_eq!(app.selection.as_ref().unwrap().text, "The first");
            assert_eq!(
                tap_selection_action(app, selection_popup::SelectionAction::Find),
                Change::Partial
            );
        };
        let initial = app.current_history_location();
        let initial_history = app.back.len();
        app.find_match_case = true;
        app.find_ignore_accents = true;
        app.find_regex = true;
        select(&mut app);
        assert_eq!(app.screen, Screen::Book);
        assert!(app.nearby.searching_direction());
        let cancel = rectangle_center(MATCH_PREV);
        assert_eq!(app.input(Input::Tap(cancel.0, cancel.1)), Change::Partial);
        assert!(!app.nearby.searching_direction());
        assert_eq!(app.current_history_location(), initial);
        assert_eq!(app.back.len(), initial_history);

        select(&mut app);
        let deadline = Instant::now() + Duration::from_secs(10);
        while app.nearby.searching_direction() && Instant::now() < deadline {
            app.poll_nearby(Instant::now()).unwrap();
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(!app.nearby.searching_direction());
        assert_eq!(app.screen, Screen::Book);
        let result = app.result_selection.as_ref().map(|hit| {
            hit.text
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .to_lowercase()
        });
        assert_eq!(result.as_deref(), Some("the first"), "{:#?}", app.hits);
        assert_eq!(
            app.current_position().unwrap().page,
            initial.as_ref().unwrap().position.page
        );
        assert_eq!(app.lookup_history.searches[0].text, "The first");
        assert!(!app.lookup_history.searches[0].match_case);
        assert!(!app.lookup_history.searches[0].ignore_accents);
        assert!(!app.lookup_history.searches[0].regular_expression);
        app.activate(Action::ReturnToLocation).unwrap();
        assert_eq!(app.current_history_location(), initial);

        app.open_reading_edge(true, 100.0);
        assert_eq!(tap_reading_control(&mut app, "Search"), Change::Partial);
        assert_eq!(
            tap_reading_control(&mut app, "Search settings"),
            Change::Partial
        );
        assert_eq!(app.screen, Screen::SearchSettings);
        assert_eq!(
            app.input(Input::Tap(100.0, (LIST_Y + 28) as f32)),
            Change::Partial
        );
        assert!(ReadingControls::load(&app.controls_path, true).selection_search_all);
        let back = rectangle_center(primitive_ui::back_button(WIDTH as i32));
        assert_eq!(app.input(Input::Tap(back.0, back.1)), Change::Partial);
        assert_eq!(app.top_group, Some(TopGroup::Search));
        app.show(Screen::Book);

        select(&mut app);
        assert_eq!(app.screen, Screen::Matches);
        wait_for_search(&mut app, usize::MAX);
        assert!(app.hits.len() >= 2);
        capture_test_screen(&app, &fonts, "READER_SELECTION_SEARCH_RESULTS_FRAME", false);
        assert_eq!(app.input(Input::Tap(back.0, back.1)), Change::Partial);
        assert_eq!(app.screen, Screen::Book);
        assert_eq!(app.current_history_location(), initial);
        assert_eq!(app.back.len(), initial_history);
    }

    #[test]
    fn zoomed_pdf_selection_search_cancels_with_the_original_view_and_saved_position() {
        let fixture = Fixture::new();
        let pdf = fixture.root.join("Search selection.pdf");
        fs::write(&pdf, crate::fixed::tests::document()).unwrap();
        let mut app = fixture.app();
        app.open_path(&pdf).unwrap();
        app.activate(Action::RotatePage).unwrap();
        app.activate(Action::SetFixedZoom(1)).unwrap();
        let reading = &app.fixed.as_ref().unwrap().reading;
        let text = reading.page_text(0).unwrap();
        let point = text
            .words
            .iter()
            .find_map(|word| reading.visible_rectangle(0, word.bounds))
            .unwrap()
            .center();
        let select = |app: &mut Reader| {
            assert_eq!(
                app.input(Input::LongTap(point.x as f32, point.y as f32)),
                Change::Partial
            );
            assert!(app.pdf_selection.is_some());
            assert_eq!(
                tap_selection_action(app, selection_popup::SelectionAction::Find),
                Change::Partial
            );
        };
        let id = app.open_book_id().unwrap();
        let origin = app.current_history_location();
        let prior_view = reading.preferences();
        let saved = app.library.state(id).unwrap().position.clone();
        let saved_view = app.library.state(id).unwrap().fixed_preferences;
        select(&mut app);
        assert_eq!(app.screen, Screen::Fixed);
        let cancel = rectangle_center(MATCH_PREV);
        assert_eq!(app.input(Input::Tap(cancel.0, cancel.1)), Change::Partial);
        assert!(!app.nearby.searching_direction());
        assert_eq!(app.current_history_location(), origin);
        assert_eq!(
            app.fixed.as_ref().unwrap().reading.preferences(),
            prior_view
        );

        app.activate(Action::SetReadingControls(ReadingControls {
            selection_search_all: true,
            ..app.controls
        }))
        .unwrap();
        select(&mut app);
        assert_eq!(app.screen, Screen::Matches);
        // Close the result view without jumping to one of its matches.
        let back = rectangle_center(primitive_ui::back_button(WIDTH as i32));
        assert_eq!(app.input(Input::Tap(back.0, back.1)), Change::Partial);
        assert_eq!(app.screen, Screen::Fixed);
        assert!(app.search_at.is_none());
        assert_eq!(app.current_history_location(), origin);
        assert_eq!(
            app.fixed.as_ref().unwrap().reading.preferences(),
            prior_view
        );
        assert_eq!(app.library.state(id).unwrap().position, saved);
        assert_eq!(app.library.state(id).unwrap().fixed_preferences, saved_view);
        assert!(app.back.is_empty());
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
    fn fixed_reflow_spacing_persists_per_book_and_is_shared_across_the_two_views() {
        use crate::library::FixedReflowSpacing;
        let paths = Fixture::new();
        let source = paths.root.join("Harbor scan.pdf");
        let vector = paths.root.join("Harbor text.pdf");
        fs::copy(crate::test_fixture("scan-reflow/raster.pdf"), &source).unwrap();
        fs::copy(crate::test_fixture("scan-reflow/vector.pdf"), &vector).unwrap();
        let mut app = paths.app();
        app.open_path(&source).unwrap();
        app.activate(Action::FixedPage(1)).unwrap();
        let ocr = crate::ocr::parse_tsv(
            include_bytes!("../tests/fixtures/scan-reflow/page-2-ocr.tsv"),
            1350,
            1800,
        )
        .unwrap();
        app.fixed
            .as_mut()
            .unwrap()
            .reading
            .remember_ocr(1, "eng", ocr.text)
            .unwrap();
        let fixed_position = app.current_history_location();
        app.activate(Action::ReflowScan).unwrap();
        let [toolbar, large, cancel, apply] = app
            .text_flow
            .as_ref()
            .unwrap()
            .spacing_test_targets()
            .map(|rect| {
                let (x, y) = rectangle_center(rect);
                Input::Tap(x, y)
            });
        assert_eq!(app.input(toolbar), Change::Partial);
        assert_eq!(app.input(large), Change::Partial);
        assert_eq!(app.input(cancel), Change::Partial);
        assert_eq!(
            app.text_flow.as_ref().unwrap().line_spacing(),
            FixedReflowSpacing::Medium
        );
        assert_eq!(
            app.fixed
                .as_ref()
                .unwrap()
                .reading
                .preferences()
                .fixed_reflow_spacing,
            None
        );

        app.input(toolbar);
        app.input(large);
        let fonts = [
            crate::test_fixture("fonts/NotoSans-Regular.ttf"),
            crate::test_fixture("fonts/NotoSans-Bold.ttf"),
        ];
        capture_test_screen(&app, &fonts, "READER_FIXED_REFLOW_SPACING", false);
        assert_eq!(app.input(apply), Change::Partial);
        assert_eq!(
            app.text_flow.as_ref().unwrap().line_spacing(),
            FixedReflowSpacing::Large
        );
        assert_eq!(app.current_history_location(), fixed_position);
        app.save().unwrap();
        drop(app);

        let mut reopened = paths.app();
        reopened.open_path(&source).unwrap();
        reopened.activate(Action::ReflowScan).unwrap();
        assert_eq!(
            reopened.text_flow.as_ref().unwrap().line_spacing(),
            FixedReflowSpacing::Large
        );
        reopened.activate(Action::ReadFixedText).unwrap();
        assert_eq!(
            reopened.text_flow.as_ref().unwrap().line_spacing(),
            FixedReflowSpacing::Large
        );
        assert_eq!(reopened.current_history_location(), fixed_position);

        reopened.open_path(&vector).unwrap();
        reopened.activate(Action::ReadFixedText).unwrap();
        assert_eq!(
            reopened.text_flow.as_ref().unwrap().line_spacing(),
            FixedReflowSpacing::Medium
        );
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
        assert_eq!(
            fixture.app().library.state(id).unwrap().highlights[0].created_at,
            Some(1_750_000_001)
        );
    }
}

#[cfg(test)]
mod spine_configuration_tests;
