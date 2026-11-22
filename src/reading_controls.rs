use std::cell::Cell;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::Duration;

use primitive_app::{Change, WIDTH, with_storage};
use primitive_ui::Input;
use serde::{Deserialize, Serialize};

use crate::session::PAGE_HEIGHT;

const MAX_FILE_BYTES: u64 = 4096;
pub(crate) const MAX_AUTO_TURN_SECONDS: u32 = 86_400;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub(crate) struct ReadingControls {
    pub hide_nonlinear_default: bool,
    #[serde(skip_serializing_if = "crate::highlight_style::HighlightStyle::is_default")]
    pub highlight_style: crate::highlight_style::HighlightStyle,
    #[serde(skip_serializing_if = "crate::highlight_style::HighlightColor::is_default")]
    pub highlight_color: crate::highlight_style::HighlightColor,
    /// Stock's reader-wide opt-in; the current book supplies its saved color.
    pub selection_uses_highlight_color: bool,
    #[serde(skip_serializing_if = "crate::highlight_style::NoteMarker::is_default")]
    pub note_marker: crate::highlight_style::NoteMarker,
    #[serde(skip_serializing_if = "crate::highlight_style::Appearance::is_default")]
    pub highlight_appearance: crate::highlight_style::Appearance,
    pub night_pages: bool,
    pub footnote_popups: bool,
    pub prefer_footnotes: bool,
    pub left_touch: bool,
    pub large_toolbar: bool,
    pub disable_page_taps: bool,
    pub page_refresh: PageRefresh,
    /// Persistent KOReader-style page-turn timeout. Zero disables automatic turns.
    pub auto_turn_seconds: u32,
    pub both_toolbars: bool,
    pub orientation: ReadingOrientation,
    pub palette: ScreenPalette,
    pub status_bar: StatusBar,
    pub overlap_footer: bool,
    pub footer_scope: ProgressScope,
    pub hide_chapter_markers: bool,
    pub footer_all_at_once: bool,
    pub footer_combined_time: Option<ProgressScope>,
    pub footer_tap_locked: bool,
    pub skim_on_footer_hold: bool,
    pub menu_activation: MenuActivation,
    pub numbering: NumberingDefaults,
    pub end_action: EndAction,
    pub finish_at_end: bool,
    #[serde(default = "legacy_metadata_save")]
    pub metadata_save: MetadataSave,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum MetadataSave {
    CloseOrSleep,
    FiveMinutes,
    #[default]
    FifteenMinutes,
    ThirtyMinutes,
    OneHour,
    AfterChange,
}

fn legacy_metadata_save() -> MetadataSave {
    MetadataSave::AfterChange
}

impl MetadataSave {
    pub const ALL: [Self; 6] = [
        Self::CloseOrSleep,
        Self::FiveMinutes,
        Self::FifteenMinutes,
        Self::ThirtyMinutes,
        Self::OneHour,
        Self::AfterChange,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::CloseOrSleep => "On close or sleep",
            Self::FiveMinutes => "Every 5 minutes",
            Self::FifteenMinutes => "Every 15 minutes",
            Self::ThirtyMinutes => "Every 30 minutes",
            Self::OneHour => "Every 60 minutes",
            Self::AfterChange => "After each change",
        }
    }

    pub fn interval(self) -> Option<Duration> {
        let minutes = match self {
            Self::FiveMinutes => 5,
            Self::FifteenMinutes => 15,
            Self::ThirtyMinutes => 30,
            Self::OneHour => 60,
            Self::CloseOrSleep | Self::AfterChange => return None,
        };
        Some(Duration::from_secs(60 * minutes))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum EndAction {
    #[default]
    Ask,
    Stay,
    Status,
    Next,
    Beginning,
    Library,
    Finish,
    StatusThenLibrary,
    Delete,
}

impl EndAction {
    pub const ALL: [Self; 9] = [
        Self::Ask,
        Self::Stay,
        Self::Status,
        Self::Next,
        Self::Beginning,
        Self::Library,
        Self::Finish,
        Self::StatusThenLibrary,
        Self::Delete,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Ask => "Show actions",
            Self::Stay => "Stay on the last page",
            Self::Status => "Show book status",
            Self::Next => "Open next file",
            Self::Beginning => "Go to beginning",
            Self::Library => "Return to Library",
            Self::Finish => "Mark as finished",
            Self::StatusThenLibrary => "Show book status, then Library",
            Self::Delete => "Ask to delete the file",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub(crate) struct NumberingDefaults {
    pub stride: Option<u16>,
    pub override_publisher: bool,
    pub prompt_publisher: bool,
    pub footer: bool,
    pub margins: bool,
    pub label_size: u8,
}

impl Default for NumberingDefaults {
    fn default() -> Self {
        Self {
            stride: None,
            override_publisher: false,
            prompt_publisher: false,
            footer: false,
            margins: false,
            label_size: 14,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum MenuActivation {
    #[default]
    TapAndSwipe,
    Tap,
    Swipe,
}

impl MenuActivation {
    pub fn label(self) -> &'static str {
        match self {
            Self::TapAndSwipe => "Tap and swipe",
            Self::Tap => "Tap",
            Self::Swipe => "Swipe",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::TapAndSwipe => Self::Tap,
            Self::Tap => Self::Swipe,
            Self::Swipe => Self::TapAndSwipe,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum StatusBar {
    #[default]
    PageNumbers,
    Progress,
    Chapter,
    Remaining,
    ChapterTime,
    BookTime,
    Minimal,
}

impl StatusBar {
    pub fn label(self) -> &'static str {
        match self {
            Self::PageNumbers => "Page numbers",
            Self::Progress => "Book progress",
            Self::Chapter => "Current chapter",
            Self::Remaining => "Chapter pages left",
            Self::ChapterTime => "Chapter time left",
            Self::BookTime => "Book time left",
            Self::Minimal => "Minimal",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::PageNumbers => Self::Progress,
            Self::Progress => Self::Chapter,
            Self::Chapter => Self::Remaining,
            Self::Remaining => Self::ChapterTime,
            Self::ChapterTime => Self::BookTime,
            Self::BookTime => Self::Minimal,
            Self::Minimal => Self::PageNumbers,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ProgressScope {
    #[default]
    Book,
    Chapter,
    Off,
}

impl ProgressScope {
    pub fn next(self) -> Self {
        match self {
            Self::Book => Self::Chapter,
            Self::Chapter => Self::Off,
            Self::Off => Self::Book,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Book => "Whole book",
            Self::Chapter => "Current chapter",
            Self::Off => "Off",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ReadingOrientation {
    #[default]
    FollowLauncher,
    Upright,
    UpsideDown,
}

impl ReadingOrientation {
    pub fn label(self) -> &'static str {
        match self {
            Self::FollowLauncher => "Follow Launcher",
            Self::Upright => "Upright",
            Self::UpsideDown => "Upside down",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::FollowLauncher => Self::Upright,
            Self::Upright => Self::UpsideDown,
            Self::UpsideDown => Self::FollowLauncher,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ScreenPalette {
    #[default]
    FollowLauncher,
    Light,
    Dark,
}

impl ScreenPalette {
    pub fn label(self) -> &'static str {
        match self {
            Self::FollowLauncher => "Follow Launcher",
            Self::Light => "Light",
            Self::Dark => "Dark",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::FollowLauncher => Self::Light,
            Self::Light => Self::Dark,
            Self::Dark => Self::FollowLauncher,
        }
    }

    pub fn inverted(self) -> Option<bool> {
        match self {
            Self::FollowLauncher => None,
            Self::Light => Some(false),
            Self::Dark => Some(true),
        }
    }
}

/// The theme of Primitive Launcher, the system layer. Opened from the launcher (`PRIMITIVE_LAUNCHER=1`), the whole
/// Reader follows the launcher's dark mode, its menus and its pages, and has no light or dark switch of its own.
/// The launcher asks for a redraw (SIGUSR1) when its theme changes; the app kit reads the theme again for the menus,
/// and Reader reads it again for the pages each time it draws.
#[derive(Debug)]
pub(crate) struct SystemTheme {
    settings: PathBuf,
    dark: Cell<bool>,
}

impl SystemTheme {
    /// The launcher's theme, or `None` when Reader was not opened from the launcher.
    pub fn from_env() -> Option<Self> {
        if std::env::var("PRIMITIVE_LAUNCHER").as_deref() != Ok("1") {
            return None;
        }
        let launcher = std::env::var_os("KL_DIR")
            .map_or_else(|| PathBuf::from("/mnt/us/launcher"), PathBuf::from);
        let theme = Self::new(launcher.join("settings.txt"));
        theme.refresh();
        Some(theme)
    }

    pub fn new(settings: PathBuf) -> Self {
        Self {
            settings,
            dark: Cell::new(false),
        }
    }

    /// Read the launcher's dark mode again.
    pub fn refresh(&self) {
        let text = fs::read_to_string(&self.settings).unwrap_or_default();
        self.dark.set(launcher_dark_mode(&text));
    }

    pub fn dark(&self) -> bool {
        self.dark.get()
    }
}

/// Whether the launcher's settings turn dark mode on (`dark_mode=on`).
pub(crate) fn launcher_dark_mode(settings: &str) -> bool {
    settings.lines().any(|line| {
        line.split_once('=')
            .is_some_and(|(key, value)| key.trim() == "dark_mode" && value.trim() == "on")
    })
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PageRefresh {
    #[default]
    Default,
    EveryPage,
    EveryFivePages,
    EveryTenPages,
}

impl PageRefresh {
    pub fn next(self) -> Self {
        match self {
            Self::Default => Self::EveryPage,
            Self::EveryPage => Self::EveryFivePages,
            Self::EveryFivePages => Self::EveryTenPages,
            Self::EveryTenPages => Self::Default,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Default => "Default",
            Self::EveryPage => "Every page",
            Self::EveryFivePages => "Every 5 pages",
            Self::EveryTenPages => "Every 10 pages",
        }
    }

    pub fn after_page(self, since_last: &mut u8) -> Change {
        let interval = match self {
            Self::Default => return Change::Partial,
            Self::EveryPage => 1,
            Self::EveryFivePages => 5,
            Self::EveryTenPages => 10,
        };
        *since_last += 1;
        if *since_last >= interval {
            *since_last = 0;
            Change::Full
        } else {
            Change::Partial
        }
    }
}

impl ReadingControls {
    pub(crate) fn auto_turn_interval(self) -> Option<Duration> {
        (1..=MAX_AUTO_TURN_SECONDS)
            .contains(&self.auto_turn_seconds)
            .then(|| Duration::from_secs(u64::from(self.auto_turn_seconds)))
    }

    pub fn load(path: &Path, existing_library: bool) -> Self {
        with_storage(|| {
            let file = fs::File::open(path)?;
            let mut bytes = Vec::new();
            file.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes)?;
            if bytes.len() as u64 > MAX_FILE_BYTES {
                return Err(io::Error::other("Reading controls settings are too large"));
            }
            let mut controls: Self = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
            if controls.auto_turn_seconds > MAX_AUTO_TURN_SECONDS {
                controls.auto_turn_seconds = 0;
            }
            Ok(controls)
        })
        .unwrap_or_else(|_| {
            // Fresh profiles match the stock note-preview behavior. Existing
            // valid files (including an explicit false or sparse legacy file)
            // were deserialized above and retain their saved choice.
            let mut controls = Self {
                footnote_popups: true,
                ..Self::default()
            };
            if existing_library {
                controls.metadata_save = MetadataSave::AfterChange;
            }
            controls
        })
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        let bytes = serde_json::to_vec(self).map_err(io::Error::other)?;
        primitive_app::save(path, &bytes)
    }

    /// Accept only inputs already routed to the page-turn handler.
    pub fn page_input(self, input: Input) -> Option<Input> {
        match input {
            Input::Tap(_, y) if self.disable_page_taps && y < PAGE_HEIGHT as f32 => None,
            Input::Tap(x, y) if self.left_touch && y < PAGE_HEIGHT as f32 => {
                Some(Input::Tap(WIDTH as f32 - x, y))
            }
            _ => Some(input),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_note_previews_are_on_and_saved_or_sparse_legacy_choices_are_preserved() {
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "primitive-note-default-{}-{suffix}",
            std::process::id()
        ));
        std::fs::create_dir(&directory).unwrap();
        let path = directory.join("reading-controls.json");
        for existing in [false, true] {
            assert!(ReadingControls::load(&path, existing).footnote_popups);
        }
        for enabled in [false, true] {
            let controls = ReadingControls {
                footnote_popups: enabled,
                ..Default::default()
            };
            controls.save(&path).unwrap();
            let before = std::fs::read(&path).unwrap();
            for existing in [false, true] {
                assert_eq!(
                    ReadingControls::load(&path, existing).footnote_popups,
                    enabled
                );
            }
            assert_eq!(std::fs::read(&path).unwrap(), before);
        }
        std::fs::write(&path, b"{}").unwrap();
        assert!(!ReadingControls::load(&path, true).footnote_popups);
        assert_eq!(std::fs::read(&path).unwrap(), b"{}");
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn the_system_theme_is_the_launchers_dark_mode() {
        assert!(launcher_dark_mode("rotation=upright\ndark_mode=on\n"));
        assert!(launcher_dark_mode(" dark_mode = on "));
        assert!(!launcher_dark_mode("dark_mode=off"));
        assert!(!launcher_dark_mode(""), "no setting is light");

        let path = std::env::temp_dir().join(format!("reader-theme-{}.txt", std::process::id()));
        let theme = SystemTheme::new(path.clone());
        fs::write(&path, "dark_mode=on\n").unwrap();
        theme.refresh();
        assert!(theme.dark());
        fs::write(&path, "dark_mode=off\n").unwrap();
        theme.refresh();
        assert!(!theme.dark(), "a change is read again");
        fs::remove_file(&path).unwrap();
        theme.refresh();
        assert!(!theme.dark(), "no settings file is light");
    }

    #[test]
    fn old_settings_keep_the_default_partial_page_refresh() {
        let controls: ReadingControls = serde_json::from_str(r#"{"night_pages":true}"#).unwrap();
        assert!(controls.night_pages);
        assert_eq!(controls.auto_turn_seconds, 0);
        assert_eq!(controls.auto_turn_interval(), None);
        assert_eq!(controls.page_refresh, PageRefresh::Default);
        let mut since_last = 0;
        assert_eq!(
            controls.page_refresh.after_page(&mut since_last),
            Change::Partial
        );
        assert_eq!(since_last, 0);
    }

    #[test]
    fn auto_turn_timeout_round_trips_and_rejects_out_of_range_values() {
        let mut controls = ReadingControls {
            auto_turn_seconds: 75,
            ..ReadingControls::default()
        };
        assert_eq!(controls.auto_turn_interval(), Some(Duration::from_secs(75)));
        let bytes = serde_json::to_vec(&controls).unwrap();
        controls = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(controls.auto_turn_seconds, 75);
        assert_eq!(controls.auto_turn_interval(), Some(Duration::from_secs(75)));

        controls.auto_turn_seconds = MAX_AUTO_TURN_SECONDS + 1;
        assert_eq!(controls.auto_turn_interval(), None);
    }
}
