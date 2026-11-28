use primitive_ui::{Icon, icons};

use super::quick_display::DisplayLevel;
use super::reading_bars::BarPanel;
use super::top_config::TopGroup;
use super::{Action, Screen};
use crate::library::{FixedContentFit, FixedTileAxis};

const fn line(stroke: &'static [&'static str]) -> Icon {
    Icon { stroke, fill: &[] }
}

pub(super) const BOOK: Icon = line(&[
    "M4 6 H10 A2 2 0 0 1 12 8 V19 A2 2 0 0 0 10 17 H4 Z",
    "M20 6 H14 A2 2 0 0 0 12 8 V19 A2 2 0 0 1 14 17 H20 Z",
]);
pub(super) const CODE: Icon = line(&["M9 6 L3 12 L9 18 M15 6 L21 12 L15 18"]);
pub(super) const TEXT: Icon = line(&["M4 19 L9 6 L14 19 M6.5 14 H11.5 M16 19 L19 11 L22 19"]);
pub(super) const EXIT: Icon = line(&["M14 5 H19 V19 H14", "M4 12 H15 M11 8 L15 12 L11 16"]);
pub(super) const BOOKMARK: Icon = line(&["M7 4 H17 V20 L12 16 L7 20 Z"]);
const PAGE_BOOKMARK_PREVIOUS: Icon = Icon {
    stroke: &["M8 8 L4 12 L8 16"],
    fill: &["M13 5 H19 V19 L16 16 L13 19 Z"],
};
const PAGE_BOOKMARK_NEXT: Icon = Icon {
    stroke: &["M16 8 L20 12 L16 16"],
    fill: &["M5 5 H11 V19 L8 16 L5 19 Z"],
};
pub(super) const PAGE: Icon = line(&["M5 4 H19 V20 H5 Z", "M8 9 H16 M8 13 H16 M8 17 H13"]);
pub(super) const GRID: Icon = line(&[
    "M4 4 H10 V10 H4 Z M14 4 H20 V10 H14 Z",
    "M4 14 H10 V20 H4 Z M14 14 H20 V20 H14 Z",
]);
pub(super) const SEARCH: Icon = line(&["M12 4 A6 6 0 1 0 12 16 A6 6 0 1 0 12 4 Z M16 16 L21 21"]);
pub(super) const FOLDER: Icon = line(&["M3 6 H10 L12 8 H21 V19 H3 Z"]);
const ARCHIVE: Icon = line(&["M4 5 H20 V9 H4 Z M5 9 V20 H19 V9 M12 11 V17 M9 14 L12 17 L15 14"]);
pub(super) const HIGHLIGHT: Icon = line(&["M4 15 L13 6 L18 11 L9 20 H4 Z M4 20 H20"]);
pub(super) const SELECT_TEXT: Icon = line(&[
    "M4 7 V5 H7 M17 5 H20 V7 M4 17 V19 H7 M17 19 H20 V17 M8 9 V7 H16 V9 M12 7 V17 M10 17 H14",
]);
pub(super) const EDIT: Icon = line(&[
    "M4 16.5 L16.5 4 A2.1 2.1 0 0 1 19.5 7 L7 19.5 L3 21 Z",
    "M14 6.5 L17.5 10",
]);
pub(super) const COPY: Icon = line(&["M8 7 H19 V21 H8 Z M5 17 H3 V3 H15 V5"]);
pub(super) const CLOCK: Icon = line(&["M12 3 A9 9 0 1 0 12 21 A9 9 0 1 0 12 3 Z M12 7 V12 L16 15"]);
pub(super) const REFRESH: Icon =
    line(&["M20 7 V3 M20 7 H16 M20 7 A8 8 0 0 0 5 8 M4 17 V21 M4 17 H8 M4 17 A8 8 0 0 0 19 16"]);
pub(super) const LINK: Icon =
    line(&["M10 8 L13 5 A4 4 0 0 1 19 11 L16 14 M14 16 L11 19 A4 4 0 0 1 5 13 L8 10 M9 15 L15 9"]);
pub(super) const JUMP: Icon = line(&["M4 12 H19 M14 7 L19 12 L14 17 M5 5 V19"]);
pub(super) const FIRST_PAGE: Icon = line(&["M15 5 L8 12 L15 19 M5 5 V19"]);
pub(super) const PREVIOUS_PAGE: Icon = line(&["M15 5 L8 12 L15 19"]);
pub(super) const NEXT_PAGE: Icon = line(&["M9 5 L16 12 L9 19"]);
pub(super) const LAST_PAGE: Icon = line(&["M9 5 L16 12 L9 19 M19 5 V19"]);
pub(super) const CLOSE: Icon = line(&["M6 6 L18 18 M18 6 L6 18"]);
pub(super) const BACK: Icon = line(&["M20 12 H5 M10 7 L5 12 L10 17"]);
const CROP: Icon = line(&["M7 3 V17 H21 M3 7 H17 V21"]);
pub(super) const LEFT_MARGIN_LESS: Icon = line(&["M3 5 V19", "M20 12 H6 M11 7 L6 12 L11 17"]);
pub(super) const LEFT_MARGIN_MORE: Icon = line(&["M3 5 V19", "M6 12 H20 M15 7 L20 12 L15 17"]);
pub(super) const RIGHT_MARGIN_LESS: Icon = line(&["M21 5 V19", "M4 12 H18 M13 7 L18 12 L13 17"]);
pub(super) const RIGHT_MARGIN_MORE: Icon = line(&["M21 5 V19", "M18 12 H4 M9 7 L4 12 L9 17"]);
pub(super) const TOP_MARGIN_LESS: Icon = line(&["M4 3 H20", "M12 21 V7 M7 12 L12 7 L17 12"]);
pub(super) const TOP_MARGIN_MORE: Icon = line(&["M4 3 H20", "M12 7 V21 M7 16 L12 21 L17 16"]);
pub(super) const BOTTOM_MARGIN_LESS: Icon = line(&["M4 21 H20", "M12 3 V17 M7 12 L12 17 L17 12"]);
pub(super) const BOTTOM_MARGIN_MORE: Icon = line(&["M4 21 H20", "M12 17 V3 M7 8 L12 3 L17 8"]);
pub(super) const DISPLAY: Icon = line(&["M3 5 H21 V17 H3 Z M8 21 H16 M12 17 V21"]);
pub(super) const ZOOM_PAGE: Icon = line(&["M5 3 H19 V21 H5 Z", "M8 7 H16 M8 11 H16 M8 15 H13"]);
pub(super) const ZOOM_CONTENT: Icon = line(&[
    "M3 7 V3 H7 M17 3 H21 V7 M3 17 V21 H7 M17 21 H21 V17",
    "M8 8 H16 V16 H8 Z",
]);
pub(super) const ZOOM_COLUMNS: Icon =
    line(&["M4 3 H20 V21 H4 Z", "M9 4 V20 M15 4 V20"]);
pub(super) const ZOOM_ROWS: Icon =
    line(&["M4 3 H20 V21 H4 Z", "M5 9 H19 M5 15 H19"]);
pub(super) const ZOOM_MANUAL: Icon = line(&[
    "M4 12 A8 8 0 1 0 20 12 A8 8 0 1 0 4 12 Z",
    "M12 7 V12 L15 14",
    "M18 18 L22 22",
]);
pub(super) const FIT_FULL: Icon = line(&["M3 8 V3 H8 M16 3 H21 V8 M3 16 V21 H8 M16 21 H21 V16"]);
pub(super) const FIT_WIDTH: Icon = line(&[
    "M5 5 H19 V19 H5 Z",
    "M3 12 H9 M3 12 L6 9 M3 12 L6 15",
    "M21 12 H15 M21 12 L18 9 M21 12 L18 15",
]);
pub(super) const FIT_HEIGHT: Icon = line(&[
    "M5 5 H19 V19 H5 Z",
    "M12 3 V9 M12 3 L9 6 M12 3 L15 6",
    "M12 21 V15 M12 21 L9 18 M12 21 L15 18",
]);
pub(super) const ZOOM_OUT: Icon = line(&["M4 12 A8 8 0 1 0 20 12 A8 8 0 1 0 4 12 Z M8 12 H16"]);
pub(super) const ZOOM_IN: Icon = line(&[
    "M4 12 A8 8 0 1 0 20 12 A8 8 0 1 0 4 12 Z",
    "M8 12 H16 M12 8 V16",
]);
const SPREAD: Icon = line(&["M3 5 H10 V19 H3 Z", "M14 5 H21 V19 H14 Z", "M12 4 V20"]);
const DIRECTION: Icon = line(&["M3 8 H21 M17 4 L21 8 L17 12", "M21 16 H3 M7 12 L3 16 L7 20"]);
const LANGUAGE: Icon = line(&[
    "M12 3 A9 9 0 1 0 12 21 A9 9 0 1 0 12 3 Z",
    "M3 12 H16 M12 3 C9 6 9 18 12 21",
    "M18 13 L21 20 M19 18 H23",
]);
pub(super) const GESTURE: Icon = line(&[
    "M8 12 V6 A2 2 0 0 1 12 6 V11",
    "M12 10 V5 A2 2 0 0 1 16 5 V11",
    "M16 10 V7 A2 2 0 0 1 20 7 V15 C20 19 17 21 13 21 H11 C8 21 7 19 6 17 L4 14 A2 2 0 0 1 7 12 Z",
]);
const CONTRAST: Icon = line(&["M12 3 A9 9 0 1 0 12 21 Z", "M12 7 V17"]);
const ANNOTATION_EXCHANGE: Icon = line(&[
    "M4 5 H13 V16 H4 Z",
    "M11 8 H20 V19 H11 Z",
    "M7 3 H17 M7 3 L10 1 M7 3 L10 5",
    "M17 21 H7 M17 21 L14 19 M17 21 L14 23",
]);
pub(super) const SLIDERS: Icon = icons::SETTINGS;
pub(super) const READFLOW: Icon = line(&["M5 6 H19 M5 10 H19 M5 14 H19 M5 18 H13"]);
const READING_GUIDES: Icon = line(&["M5 4 V20 M19 4 V20 M9 9 H15 M9 13 H15 M9 17 H13"]);
pub(super) const FAVORITE: Icon = line(&[
    "M12 4 L14.4 9.2 L20 9.8 L15.8 13.6 L17 19.2 L12 16.3 L7 19.2 L8.2 13.6 L4 9.8 L9.6 9.2 Z",
]);
const MARK_FIRST: Icon = line(&[
    "M14 5 H20 V19 L17 16 L14 19 Z",
    "M9 8 L5 12 L9 16",
    "M3 5 V19",
]);
const MARK_LAST: Icon = line(&[
    "M4 5 H10 V19 L7 16 L4 19 Z",
    "M15 8 L19 12 L15 16",
    "M21 5 V19",
]);
const MARK_LATEST: Icon = line(&[
    "M4 4 H12 V20 L8 17 L4 20 Z",
    "M17 9 A4 4 0 1 0 17 17 A4 4 0 1 0 17 9 Z",
    "M17 10.5 V13 L19 14",
]);
const INFO: Icon = line(&["M12 3 A9 9 0 1 0 12 21 A9 9 0 1 0 12 3 Z M12 11 V16 M12 8 V8.1"]);
const SAVE: Icon = line(&["M4 4 H16 L20 8 V20 H4 Z M8 4 V10 H16 V4 M8 20 V14 H16 V20"]);

const FILE: Icon = line(&[
    "M6 3 H15 L20 8 V21 H6 Z",
    "M15 3 V8 H20",
    "M9 13 H17 M9 17 H17",
]);
const HEART: Icon = line(&["M12 20 L4 12 A5 5 0 0 1 11 5 L12 6 L13 5 A5 5 0 0 1 20 12 Z"]);
const DOWNLOAD: Icon = line(&["M12 3 V15 M7 10 L12 15 L17 10", "M4 18 V21 H20 V18"]);
const UPLOAD: Icon = line(&["M12 15 V3 M7 8 L12 3 L17 8", "M4 18 V21 H20 V18"]);
const TRASH: Icon = line(&[
    "M4 7 H20 M9 7 V4 H15 V7 M6 7 L7 20 H17 L18 7",
    "M10 11 V16 M14 11 V16",
]);
const TAP: Icon = line(&[
    "M12 20 V12 A2 2 0 0 1 16 12 V14 H18 A2 2 0 0 1 20 16 V18 A4 4 0 0 1 16 22 H12 A5 5 0 0 1 8 20 L5 17 A2 2 0 0 1 8 14 L12 18",
]);
const STATUS: Icon = line(&["M4 5 H20 V19 H4 Z", "M7 15 L10 12 L13 14 L17 9"]);
const IMAGE: Icon = line(&[
    "M4 5 H20 V19 H4 Z",
    "M6 16 L11 11 L14 14 L16 12 L19 16",
    "M8 9 A1 1 0 1 0 8 9.1",
]);
const BATTERY: Icon = line(&[
    "M3 7 H19 V17 H3 Z",
    "M19 10 H22 V14 H19",
    "M6 10 H10 V14 H6 Z",
]);
const LOCK: Icon = line(&[
    "M5 11 H19 V21 H5 Z",
    "M8 11 V7 A4 4 0 0 1 16 7 V11",
    "M12 15 V18",
]);

fn screen_icon(screen: Screen) -> Icon {
    match screen {
        Screen::Footnote
        | Screen::Book
        | Screen::FixedContents
        | Screen::Contents
        | Screen::GeneratedContents
        | Screen::CustomContents
        | Screen::ContentsSettings
        | Screen::HiddenFlows
        | Screen::Vocabulary
        | Screen::VocabularyWord
        | Screen::VocabularyContext
        | Screen::VocabularyOptions
        | Screen::VocabularyBooks
        | Screen::VocabularySearch
        | Screen::Dictionaries
        | Screen::DictionaryOptions(_)
        | Screen::DictionaryButtons(_)
        | Screen::DictionaryPresets(_)
        | Screen::Definition
        | Screen::DictionaryEntry => BOOK,
        Screen::Library
        | Screen::OrganizeLibrary
        | Screen::BookList
        | Screen::LiveMetadata(_)
        | Screen::CollectionName => FOLDER,
        Screen::LibraryFind
        | Screen::ContentsSearch
        | Screen::Find
        | Screen::Matches
        | Screen::SearchHistory
        | Screen::MenuSearch
        | Screen::MenuSearchResults
        | Screen::BookmarkFind
        | Screen::BookmarkMatches
        | Screen::SearchSettings => SEARCH,
        Screen::FolderOrder
        | Screen::PageTurnZones
        | Screen::PageTurnSizes(_)
        | Screen::PageTurnDefaults
        | Screen::NavigationSettings
        | Screen::PageTurns => DIRECTION,
        Screen::BackOptions(_) => BACK,
        Screen::BookArchive
        | Screen::ArchiveFolder
        | Screen::MetadataArchive(_)
        | Screen::RestoreBackup => ARCHIVE,
        Screen::BookShortcuts(_)
        | Screen::BookmarkBrowser(_)
        | Screen::CustomBookmarks
        | Screen::Bookmarks
        | Screen::FixedBookmarks
        | Screen::BookmarkListSettings => BOOKMARK,
        Screen::ReadingHistory(_)
        | Screen::PastNotifications
        | Screen::AutoTurn
        | Screen::AutoTurnDistance
        | Screen::AutoTurnPan
        | Screen::LocationHistory
        | Screen::OnlineHistory
        | Screen::DictionaryHistory
        | Screen::ReadingNotifications => CLOCK,
        Screen::BookIdentityEdit(_)
        | Screen::CustomTitle
        | Screen::CustomEdits
        | Screen::FooterCustomText
        | Screen::ProfileName(_)
        | Screen::OnlineEdit
        | Screen::Note
        | Screen::EditNote
        | Screen::StyleEditor => EDIT,
        Screen::RemoveCollection
        | Screen::FooterPresetDelete(_)
        | Screen::RemoveHighlight
        | Screen::ClearOcr
        | Screen::ClearOnlineHistory
        | Screen::ClearDictionaryHistory
        | Screen::StyleClear => TRASH,
        Screen::AnnotationExchange
        | Screen::AnnotationExchangePreview
        | Screen::ImportAnnotations
        | Screen::ImportPosition
        | Screen::ImportPdfAnnotations
        | Screen::ExportAnnotations => ANNOTATION_EXCHANGE,
        Screen::AnnotationListSettings
        | Screen::HighlightPromptSettings
        | Screen::Highlights
        | Screen::Annotation => HIGHLIGHT,
        Screen::VeryLongPressSettings
        | Screen::SelectionPosition
        | Screen::LongPressSettings
        | Screen::SelectionHtml => SELECT_TEXT,
        Screen::HeaderBatterySettings => BATTERY,
        Screen::FooterSettings
        | Screen::FooterItemWidths
        | Screen::FooterFontFaces
        | Screen::FooterPresets
        | Screen::FooterPreset(_) => DISPLAY,
        Screen::FooterCustomTextGuide
        | Screen::ReadingGuidesAbout
        | Screen::BookDetails
        | Screen::StyleReview => INFO,
        Screen::EndActionSettings => EXIT,
        Screen::MetadataSaveSettings | Screen::ReadingData => SAVE,
        Screen::ReadingGuides => READING_GUIDES,
        Screen::Scrolling | Screen::ReadFixedText | Screen::Hyphenate => READFLOW,
        Screen::GestureBinding(_)
        | Screen::Multiswipes
        | Screen::CustomMultiswipes
        | Screen::RecordMultiswipe
        | Screen::MultiswipeBinding(_)
        | Screen::MultiswipeActions(_)
        | Screen::AddMultiswipeAction(_)
        | Screen::MultiswipeMode(_)
        | Screen::MultiswipeQuickMenuSettings(_)
        | Screen::MultiswipeQuickMenuPosition(_)
        | Screen::EditMultiswipeAction(..)
        | Screen::MultiswipeActionName(..)
        | Screen::RemoveMultiswipe(_)
        | Screen::GestureOverview
        | Screen::Gestures
        | Screen::MenuActivationSettings => GESTURE,
        Screen::LibraryExport(_) | Screen::ExportSettings(_, _) => UPLOAD,
        Screen::Illustration | Screen::ImageStyles => IMAGE,
        Screen::FixedTools | Screen::FixedCrop | Screen::BookTools => CROP,
        Screen::ScanColumns | Screen::PageOverview | Screen::BookMap => GRID,
        Screen::OcrLanguage
        | Screen::TypographyLanguages(_)
        | Screen::InterfaceLanguage
        | Screen::OcrLanguageCatalog
        | Screen::OcrLanguageCatalogItem(_) => LANGUAGE,
        Screen::PdfPassword => LOCK,
        Screen::Skim | Screen::GoToPage | Screen::ChapterNavigation => JUMP,
        Screen::FootnoteFontSize(_)
        | Screen::ReadingFonts
        | Screen::FontRendering(_)
        | Screen::HyphenationOptions
        | Screen::HyphenationDefaults
        | Screen::HyphenationLimits(_)
        | Screen::HyphenationLimitValue(_, _)
        | Screen::Typography
        | Screen::TypographyRules
        | Screen::FootnoteTweaks
        | Screen::InPageFootnoteSize(_)
        | Screen::NormalLineHeightStyles
        | Screen::ParagraphSpacing => TEXT,
        Screen::ReadingProfiles
        | Screen::ReadingProfile(_)
        | Screen::ReadingProfileFields(_)
        | Screen::ReadingProfileValues(_)
        | Screen::ReadingProfileZoom(_)
        | Screen::ReadingProfileFonts(_)
        | Screen::ReadingProfileMargins(_)
        | Screen::ReadingProfileFontRendering(_)
        | Screen::ReadingProfileBookRendering(_)
        | Screen::ReadingProfileChoice(_, _)
        | Screen::ReadingProfileOpening(_, _)
        | Screen::ReadingProfileClosing(_, _)
        | Screen::ConfirmProfile(_, _)
        | Screen::ReadingControls
        | Screen::DocumentDefaults
        | Screen::PublisherRenderDefaults
        | Screen::StyleTweaks
        | Screen::StylePreset
        | Screen::TableStyles
        | Screen::PseudoElementStyles
        | Screen::ListItemStyles
        | Screen::StyleFiles
        | Screen::LinkStyles => SLIDERS,
        Screen::OnlineLookup | Screen::Links | Screen::LinkSettings | Screen::WikipediaSettings => {
            LINK
        }
        Screen::DictionaryCatalog => DOWNLOAD,
        Screen::DictionaryCatalogLanguage(_) => LANGUAGE,
        Screen::DictionaryCatalogItem(_, _) => DOWNLOAD,
        Screen::TranslationSettings => LANGUAGE,
        Screen::Fixed
        | Screen::PageNumbering
        | Screen::NumberingValue(_)
        | Screen::NumberingDefaults
        | Screen::PublisherPages
        | Screen::PublisherOffer => PAGE,
        Screen::FooterPresetName(_) => EDIT,
    }
}

pub(super) fn group_icon(group: TopGroup) -> Icon {
    match group {
        TopGroup::Navigation => BOOK,
        TopGroup::Appearance => TEXT,
        TopGroup::Settings => SLIDERS,
        TopGroup::Tools => CROP,
        TopGroup::Search => SEARCH,
        TopGroup::Library => FOLDER,
    }
}

pub(super) fn panel_icon(panel: BarPanel) -> Icon {
    match panel {
        BarPanel::Pages => GRID,
        BarPanel::Text => TEXT,
        BarPanel::TextLayout | BarPanel::ViewLayout => GRID,
        BarPanel::TextMargins => CROP,
        BarPanel::TextStyle => HIGHLIGHT,
        BarPanel::TextSpacing => READFLOW,
        BarPanel::TextRendering => SLIDERS,
        BarPanel::TextScreen | BarPanel::ViewScreen | BarPanel::View => DISPLAY,
        BarPanel::Scan => CROP,
    }
}

// Keep this match exhaustive: a new action must receive an intentional icon
// instead of silently falling back to a generic page symbol.
pub(super) fn action_icon(action: &Action) -> Icon {
    match action {
        Action::Open(_) | Action::OpenTracked(_) => FILE,
        Action::Continue => BOOK,
        Action::SearchReadingMenus => SEARCH,
        Action::EmbeddedFontReport => INFO,
        Action::OpenQuickstart => INFO,
        Action::PreviewReadingMenu(_) => GRID,
        Action::Show(Screen::TypographyRules) => TEXT,
        Action::Show(Screen::ReadingGuides) => READING_GUIDES,
        Action::Show(Screen::Library | Screen::OrganizeLibrary)
        | Action::CurrentBookFolder
        | Action::Browse(_) => FOLDER,
        Action::ClearLibraryFilter => CLOSE,
        Action::SetFolderOrder(_) => DIRECTION,
        Action::Favorite(_) => HEART,
        Action::SetStatus(_) => STATUS,
        Action::CollectionMembership(_, _)
        | Action::NameCollection(_)
        | Action::RemoveCollection => FOLDER,
        Action::ViewCover => IMAGE,
        Action::BookArchive(_) | Action::MetadataArchive(_) => ARCHIVE,
        Action::BookShortcut(_) => BOOKMARK,
        Action::LiveMetadata(_) => FOLDER,
        Action::Show(Screen::Contents | Screen::FixedContents) => BOOK,
        Action::Chapter(super::chapter_navigation::Direction::Previous) => FIRST_PAGE,
        Action::Chapter(super::chapter_navigation::Direction::Next) => LAST_PAGE,
        Action::ChapterLevel(_) => BOOK,
        Action::AnnotationExchange(_) => ANNOTATION_EXCHANGE,
        Action::ClearOcr => TRASH,
        Action::SavedMark(super::chapter_navigation::Direction::Previous) => super::skim::MARK_BACK,
        Action::SavedMark(super::chapter_navigation::Direction::Next) => super::skim::MARK_FORWARD,
        Action::PageBookmark(super::chapter_navigation::Direction::Previous) => {
            PAGE_BOOKMARK_PREVIOUS
        }
        Action::PageBookmark(super::chapter_navigation::Direction::Next) => PAGE_BOOKMARK_NEXT,
        Action::SavedMarkEdge(super::chapter_navigation::Direction::Previous) => MARK_FIRST,
        Action::SavedMarkEdge(super::chapter_navigation::Direction::Next) => MARK_LAST,
        Action::LatestSavedMark => MARK_LATEST,
        Action::OpenFolderFile(super::FolderDirection::Previous) => FIRST_PAGE,
        Action::OpenFolderFile(super::FolderDirection::Next) => LAST_PAGE,
        Action::Show(Screen::FixedBookmarks | Screen::Bookmarks)
        | Action::BookmarkBrowser(_)
        | Action::ToggleBookmark
        | Action::AddBookmark => BOOKMARK,
        Action::Show(
            Screen::Find | Screen::BookmarkFind | Screen::LibraryFind | Screen::Matches,
        ) => SEARCH,
        Action::Show(Screen::DictionaryEntry | Screen::Vocabulary) | Action::ShowDictionaries => {
            BOOK
        }
        Action::Show(
            Screen::DictionaryHistory
            | Screen::LocationHistory
            | Screen::AutoTurn
            | Screen::SearchHistory,
        )
        | Action::ReturnToHistory(_) => CLOCK,
        Action::OpenPreviousDocument => BOOK,
        Action::Show(Screen::ContentsSettings) => BOOK,
        Action::Show(Screen::BookmarkListSettings) => BOOKMARK,
        Action::Show(Screen::SelectionPosition | Screen::LongPressSettings) => SELECT_TEXT,
        Action::Show(Screen::Gestures | Screen::MenuActivationSettings) => GESTURE,
        Action::Show(Screen::PageTurns | Screen::NavigationSettings) => DIRECTION,
        Action::Show(Screen::ChapterNavigation) => JUMP,
        Action::Show(Screen::EndActionSettings) => EXIT,
        Action::Show(Screen::MetadataSaveSettings | Screen::ReadingData) => SAVE,
        Action::Show(Screen::WikipediaSettings) => LINK,
        Action::Show(Screen::InterfaceLanguage) => LANGUAGE,
        Action::Show(Screen::FooterSettings | Screen::FooterItemWidths) => DISPLAY,
        Action::Show(Screen::ReadingNotifications) => CLOCK,
        Action::Show(Screen::BookMap) | Action::Overview => GRID,
        Action::Skim => JUMP,
        Action::EditListDensity(_, _) => GRID,
        Action::PageTurnSetting(_)
        | Action::KeepReadingOrientation(_)
        | Action::ConfirmReadingOrientationDefault(_) => DIRECTION,
        Action::ToggleNotificationSource(_) => CLOCK,
        Action::BindGesture(_, _) | Action::Multiswipe(_) | Action::MultiswipePreview(_) => GESTURE,
        Action::SetMenuActivation(_) => TAP,
        Action::AutoTurn(_)
        | Action::AdjustAutoTurnDistance(_)
        | Action::ResetAutoTurnDistance
        | Action::AutoTurnPan(_) => CLOCK,
        Action::PreviewNearbyMarks | Action::ApplyNearbyMarks => BOOKMARK,
        Action::PreviewKoReader
        | Action::ApplyKoReader
        | Action::PreviewKoReaderPosition
        | Action::PreviewCopiedPosition
        | Action::ApplyKoReaderPosition
        | Action::PreviewPdfAnnotations
        | Action::ApplyPdfAnnotations
        | Action::ExportAnnotatedPdf => ANNOTATION_EXCHANGE,
        Action::Jump(_) | Action::ContentsJump(_) => JUMP,
        Action::BookMapChapter(_) => GRID,
        Action::Show(
            Screen::DocumentDefaults
            | Screen::PublisherRenderDefaults
            | Screen::StyleTweaks
            | Screen::ParagraphSpacing
            | Screen::TableStyles
            | Screen::PseudoElementStyles
            | Screen::ListItemStyles
            | Screen::StylePreset
            | Screen::StyleFiles
            | Screen::StyleReview
            | Screen::StyleEditor
            | Screen::StyleClear,
        )
        | Action::Style(_) => SLIDERS,
        Action::DocumentDefaults(command) => match command {
            super::default_settings::Command::SaveReflow
            | super::default_settings::Command::SaveFixed => SAVE,
            super::default_settings::Command::ResetReflow
            | super::default_settings::Command::ResetFixed => icons::ROTATE,
            _ => CLOSE,
        },
        Action::ToggleNonlinear => BOOK,
        Action::Flow(_) | Action::Show(Screen::HiddenFlows) => BOOK,
        Action::Show(Screen::Links | Screen::LinkSettings | Screen::LinkStyles)
        | Action::ToggleLinkPreference(_)
        | Action::OpenFixedLink(_)
        | Action::OpenLinkAddress(_)
        | Action::OpenLocalDocument(_)
        | Action::Wikipedia => LINK,
        Action::ReadAlarm | Action::Show(Screen::OnlineHistory) | Action::ReadingHistory(_) => {
            CLOCK
        }
        Action::Show(Screen::TranslationSettings) | Action::TranslatePage => LANGUAGE,
        Action::RepeatOnline(_) | Action::ToggleOnlineHistory => CLOCK,
        Action::ToggleRomanizations => LANGUAGE,
        Action::ToggleWikipediaImage | Action::ToggleWikipediaMoreImages => IMAGE,
        Action::ClearOnlineHistory | Action::ClearDictionaryHistory => TRASH,
        Action::DictionaryPreset(_)
        | Action::DictionarySource(_)
        | Action::DictionaryButtons(_)
        | Action::Vocabulary(_) => BOOK,
        Action::InstallDictionary(_, _)
        | Action::DownloadDictionaryWithoutSizeEstimate(_, _)
        | Action::CheckDictionarySize(_, _)
        | Action::InstallOcrLanguage(_) => DOWNLOAD,
        Action::RefreshDictionaryCatalog => REFRESH,
        Action::CycleDictionaryCatalogFilter => SLIDERS,
        Action::CancelDictionaryDownload | Action::CancelOcrLanguageDownload => CLOSE,
        Action::ManageOcrLanguages | Action::RecognizeLanguage(_) => LANGUAGE,
        Action::LookUp(_) => SEARCH,
        Action::RepeatSearch(_) | Action::StepNearby(_) => SEARCH,
        Action::SaveBookData | Action::ToggleBackupComponent(_) => SAVE,
        Action::ExportBackup | Action::ExportHighlights(_) => UPLOAD,
        Action::PreviewBackup | Action::MergeBackup => ARCHIVE,
        Action::HighlightStyle | Action::Show(Screen::Highlights) => HIGHLIGHT,
        Action::Show(
            Screen::PublisherPages
            | Screen::PageNumbering
            | Screen::NumberingValue(_)
            | Screen::NumberingDefaults
            | Screen::PublisherOffer,
        )
        | Action::SetCharacterPages(_)
        | Action::CharacterPage(_)
        | Action::AcceptPublisherNumbers
        | Action::TogglePublisherFooter
        | Action::TogglePublisherMargins => PAGE,
        Action::Show(Screen::GoToPage) | Action::NextLocation => JUMP,
        Action::Show(Screen::ReadingControls | Screen::SearchSettings)
        | Action::EditSearchPresentation(_)
        | Action::EditFooterItemWidth(_) => SLIDERS,
        Action::SetFont(_)
        | Action::SetBaseStylesheet(_)
        | Action::ConfirmBaseStylesheetDefault(_) => TEXT,
        Action::SetUserHyphenation(_)
        | Action::ApplyWordSpacing(_, _)
        | Action::ApplyLogicalFont(_, _)
        | Action::FontRendering(_, _)
        | Action::CancelEagerFont
        | Action::TypographyLanguage(_, _)
        | Action::RespectEmbeddedLanguages(_)
        | Action::SetHangingPunctuation { .. }
        | Action::Hyphenation(_)
        | Action::SetBookPublisherRenderMode(_)
        | Action::SetPublisherRenderDefault(_)
        | Action::ShowFooterFonts
        | Action::SetFooterFontFace(_)
        | Action::ResetText
        | Action::EditHeaderFontSize => TEXT,
        Action::ApplyProfile(_)
        | Action::RemoveProfile(_)
        | Action::UpdateReadingProfile(_)
        | Action::EditReadingProfileFields(_)
        | Action::ToggleReadingProfileField(_)
        | Action::SetReadingProfileFields(_)
        | Action::SetClosingProfile(_, _)
        | Action::OpeningProfileRule(_, _)
        | Action::ClosingProfileRule(_, _)
        | Action::SetProfileRendering(_, _)
        | Action::ShowProfileZoom(_)
        | Action::ShowProfileFonts(_)
        | Action::SetProfileFont(_, _)
        | Action::ToggleProfileMarginLink(_) => SLIDERS,
        Action::SetProfileZoom(_, crate::reading_profiles::ProfileZoomChoice::Target(target)) => {
            match target {
                crate::reading_profiles::ProfileZoomTarget::Page => ZOOM_PAGE,
                crate::reading_profiles::ProfileZoomTarget::Content => ZOOM_CONTENT,
                crate::reading_profiles::ProfileZoomTarget::Columns => ZOOM_COLUMNS,
                crate::reading_profiles::ProfileZoomTarget::Rows => ZOOM_ROWS,
                crate::reading_profiles::ProfileZoomTarget::Manual => ZOOM_MANUAL,
                crate::reading_profiles::ProfileZoomTarget::Custom => DISPLAY,
            }
        }
        Action::SetProfileZoom(_, crate::reading_profiles::ProfileZoomChoice::Fit(fit)) => {
            match fit {
                FixedContentFit::Full => FIT_FULL,
                FixedContentFit::Width => FIT_WIDTH,
                FixedContentFit::Height => FIT_HEIGHT,
            }
        }
        Action::UpdateFooterPreset(_)
        | Action::ApplyFooterPreset(_)
        | Action::RenameFooterPreset(_)
        | Action::DeleteFooterPreset(_) => DISPLAY,
        Action::Show(Screen::Typography) | Action::ShowFonts | Action::Text(_) => TEXT,
        Action::Show(Screen::ReadingProfiles | Screen::ReadingProfile(_)) => SLIDERS,
        Action::Show(Screen::BookDetails) | Action::OrganizeCurrent | Action::AboutTopHeader => {
            INFO
        }
        Action::TopStatusHeader(_) | Action::ConfirmTopStatusHeaderDefault(_) => DISPLAY,
        Action::FacingPages(_) => SPREAD,
        Action::SearchHit(_) | Action::PdfSearchHit(_) => SEARCH,
        Action::PdfHighlight(_)
        | Action::Highlight(_)
        | Action::ManageHighlight(_)
        | Action::ExtendHighlight => HIGHLIGHT,
        Action::Bookmark(_) | Action::EditBookmarkNote(_) => BOOKMARK,
        Action::EditNote => EDIT,
        Action::RemoveHighlight => TRASH,
        Action::ExportBookClippings | Action::LibraryExport(_) | Action::ExportSettings(_, _) => {
            UPLOAD
        }
        Action::ToggleAlternativeContents
        | Action::ShowGeneratedContents
        | Action::AlternativeContentsEntry(_)
        | Action::GeneratedEntry(_) => BOOK,
        Action::Custom(_) => BOOKMARK,
        Action::ReturnToLocation => BACK,
        Action::SetDisplayLevel(DisplayLevel::FrontLight, _) => icons::SUN,
        Action::SetDisplayLevel(DisplayLevel::Warmth, _) => icons::WARMTH,
        Action::RefreshDisplay
        | Action::RotatePage
        | Action::SetFixedRotation(_)
        | Action::SetReadingOrientation(_) => icons::ROTATE,
        Action::FixedScroll(_)
        | Action::ReadingMode(_)
        | Action::ReadFixedText
        | Action::ReflowScan => READFLOW,
        Action::ComicSpread(_) => SPREAD,
        Action::ReversePageDirection => DIRECTION,
        Action::ScanContrast(_) | Action::Dewatermark(_) => CONTRAST,
        Action::CropPage | Action::SemiAutoCrop | Action::RecognizeText | Action::ScanColumns => {
            CROP
        }
        Action::ZoomTargetContent(false) => ZOOM_PAGE,
        Action::ZoomTargetContent(true) => ZOOM_CONTENT,
        Action::ZoomTargetManual => ZOOM_MANUAL,
        Action::SetFixedTileZoom(FixedTileAxis::Columns, _) => ZOOM_COLUMNS,
        Action::SetFixedTileZoom(FixedTileAxis::Rows, _) => ZOOM_ROWS,
        Action::SetFixedFitAxis(FixedContentFit::Full) => FIT_FULL,
        Action::SetFixedFitAxis(FixedContentFit::Width) => FIT_WIDTH,
        Action::SetFixedFitAxis(FixedContentFit::Height) | Action::FitHeight => FIT_HEIGHT,
        Action::AdjustFixedZoomFactor(delta) if *delta < 0 => ZOOM_OUT,
        Action::AdjustFixedZoomFactor(_) => ZOOM_IN,
        Action::FitPage(_)
        | Action::FitContent(_)
        | Action::SetFixedZoom(_)
        | Action::SetFixedPageOverlap(_, _)
        | Action::FixedGap(_)
        | Action::SetFixedZoomFactor(_)
        | Action::AdjustFixedZoomFactor(_) => DISPLAY,
        Action::FixedDither(_)
        | Action::ScanGamma(_)
        | Action::ScanWhiteLevel(_) => CONTRAST,
        Action::StraightenScan(_) => icons::ROTATE,
        Action::ForceOcr(_) => READFLOW,
        Action::SetCropMargin(_) | Action::AdjustReadingMargin(_, _) => CROP,
        Action::FixedPage(_) => PAGE,
        Action::FixedOutline(_) | Action::Heading => BOOK,
        Action::ListBooks(super::BookList::Favorites) => FAVORITE,
        Action::ListBooks(super::BookList::Recent) => CLOCK,
        Action::ListBooks(_) => FOLDER,
        Action::SetReadingControls(_) | Action::LockTouch => SLIDERS,
        Action::Show(screen) => screen_icon(*screen),
    }
}
