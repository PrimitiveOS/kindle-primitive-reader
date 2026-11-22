use primitive_ui::{Icon, icons};

use super::quick_display::DisplayLevel;
use super::reading_bars::BarPanel;
use super::top_config::TopGroup;
use super::{Action, Screen};

const fn line(stroke: &'static [&'static str]) -> Icon {
    Icon { stroke, fill: &[] }
}

pub(super) const BOOK: Icon = line(&[
    "M4 6 H10 A2 2 0 0 1 12 8 V19 A2 2 0 0 0 10 17 H4 Z",
    "M20 6 H14 A2 2 0 0 0 12 8 V19 A2 2 0 0 1 14 17 H20 Z",
]);
pub(super) const TEXT: Icon = line(&["M4 19 L9 6 L14 19 M6.5 14 H11.5 M16 19 L19 11 L22 19"]);
pub(super) const EXIT: Icon = line(&["M14 5 H19 V19 H14", "M4 12 H15 M11 8 L15 12 L11 16"]);
pub(super) const BOOKMARK: Icon = line(&["M7 4 H17 V20 L12 16 L7 20 Z"]);
pub(super) const PAGE: Icon = line(&["M5 4 H19 V20 H5 Z", "M8 9 H16 M8 13 H16 M8 17 H13"]);
const GRID: Icon = line(&[
    "M4 4 H10 V10 H4 Z M14 4 H20 V10 H14 Z",
    "M4 14 H10 V20 H4 Z M14 14 H20 V20 H14 Z",
]);
pub(super) const SEARCH: Icon = line(&["M12 4 A6 6 0 1 0 12 16 A6 6 0 1 0 12 4 Z M16 16 L21 21"]);
const FOLDER: Icon = line(&["M3 6 H10 L12 8 H21 V19 H3 Z"]);
pub(super) const HIGHLIGHT: Icon = line(&["M4 15 L13 6 L18 11 L9 20 H4 Z M4 20 H20"]);
const CLOCK: Icon = line(&["M12 3 A9 9 0 1 0 12 21 A9 9 0 1 0 12 3 Z M12 7 V12 L16 15"]);
pub(super) const LINK: Icon =
    line(&["M10 8 L13 5 A4 4 0 0 1 19 11 L16 14 M14 16 L11 19 A4 4 0 0 1 5 13 L8 10 M9 15 L15 9"]);
const JUMP: Icon = line(&["M4 12 H19 M14 7 L19 12 L14 17 M5 5 V19"]);
pub(super) const FIRST_PAGE: Icon = line(&["M15 5 L8 12 L15 19 M5 5 V19"]);
pub(super) const PREVIOUS_PAGE: Icon = line(&["M15 5 L8 12 L15 19"]);
pub(super) const NEXT_PAGE: Icon = line(&["M9 5 L16 12 L9 19"]);
pub(super) const LAST_PAGE: Icon = line(&["M9 5 L16 12 L9 19 M19 5 V19"]);
pub(super) const CLOSE: Icon = line(&["M6 6 L18 18 M18 6 L6 18"]);
pub(super) const BACK: Icon = line(&["M20 12 H5 M10 7 L5 12 L10 17"]);
const CROP: Icon = line(&["M7 3 V17 H21 M3 7 H17 V21"]);
const DISPLAY: Icon = line(&["M3 5 H21 V17 H3 Z M8 21 H16 M12 17 V21"]);
const SLIDERS: Icon = icons::SETTINGS;
const READFLOW: Icon = line(&["M5 6 H19 M5 10 H19 M5 14 H19 M5 18 H13"]);
const FAVORITE: Icon = line(&[
    "M12 4 L14.4 9.2 L20 9.8 L15.8 13.6 L17 19.2 L12 16.3 L7 19.2 L8.2 13.6 L4 9.8 L9.6 9.2 Z",
]);
const INFO: Icon = line(&["M12 3 A9 9 0 1 0 12 21 A9 9 0 1 0 12 3 Z M12 11 V16 M12 8 V8.1"]);
const SAVE: Icon = line(&["M4 4 H16 L20 8 V20 H4 Z M8 4 V10 H16 V4 M8 20 V14 H16 V20"]);

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

pub(super) fn action_icon(action: &Action) -> Icon {
    match action {
        Action::Show(Screen::Library | Screen::OrganizeLibrary) => FOLDER,
        Action::Show(Screen::Contents | Screen::FixedContents) => BOOK,
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
        | Action::OpenPreviousDocument => CLOCK,
        Action::Show(Screen::BookMap) | Action::Overview => GRID,
        Action::Skim => JUMP,
        Action::Show(
            Screen::DocumentDefaults
            | Screen::StyleTweaks
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
        Action::Show(Screen::Links) | Action::Wikipedia => LINK,
        Action::TranslatePage => TEXT,
        Action::ReadAlarm | Action::Show(Screen::OnlineHistory) | Action::ReadingHistory(_) => {
            CLOCK
        }
        Action::SaveBookData
        | Action::ExportBackup
        | Action::PreviewBackup
        | Action::MergeBackup
        | Action::ToggleBackupComponent(_) => SAVE,
        Action::HighlightStyle | Action::Show(Screen::Highlights) | Action::ExportHighlights(_) => {
            HIGHLIGHT
        }
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
        Action::Show(
            Screen::Gestures
            | Screen::ReadingControls
            | Screen::ReadingData
            | Screen::FooterSettings,
        ) => SLIDERS,
        Action::Show(Screen::Typography | Screen::ReadingProfiles)
        | Action::ShowFonts
        | Action::Text(_) => TEXT,
        Action::Show(Screen::BookDetails) | Action::OrganizeCurrent => INFO,
        Action::ReturnToLocation => BACK,
        Action::SetDisplayLevel(DisplayLevel::FrontLight, _) => icons::SUN,
        Action::SetDisplayLevel(DisplayLevel::Warmth, _) => icons::WARMTH,
        Action::RefreshDisplay | Action::RotatePage | Action::SetFixedRotation(_) => icons::ROTATE,
        Action::FixedScroll(_)
        | Action::ReadingMode(_)
        | Action::ReadFixedText
        | Action::ReflowScan => READFLOW,
        Action::CropPage | Action::RecognizeText | Action::ScanColumns => CROP,
        Action::FitHeight | Action::FitPage(_) | Action::SetFixedZoom(_) => DISPLAY,
        Action::ListBooks(super::BookList::Favorites) => FAVORITE,
        Action::ListBooks(super::BookList::Recent) => CLOCK,
        Action::ListBooks(_) => FOLDER,
        Action::SetReadingControls(_) | Action::LockTouch => SLIDERS,
        _ => PAGE,
    }
}
