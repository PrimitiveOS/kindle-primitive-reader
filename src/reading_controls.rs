use std::cell::Cell;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use primitive_app::{Change, WIDTH, with_storage};
use primitive_ui::Input;
use serde::{Deserialize, Serialize};

use crate::session::PAGE_HEIGHT;

const MAX_FILE_BYTES: u64 = 4096;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub(crate) struct ReadingControls {
    pub night_pages: bool,
    pub left_touch: bool,
    pub large_toolbar: bool,
    pub disable_page_taps: bool,
    pub page_refresh: PageRefresh,
    pub both_toolbars: bool,
    pub orientation: ReadingOrientation,
    pub palette: ScreenPalette,
    pub status_bar: StatusBar,
    pub footer_scope: ProgressScope,
    pub hide_chapter_markers: bool,
    pub footer_all_at_once: bool,
    pub footer_tap_locked: bool,
    pub skim_on_footer_hold: bool,
    pub menu_activation: MenuActivation,
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
    Minimal,
}

impl StatusBar {
    pub fn label(self) -> &'static str {
        match self {
            Self::PageNumbers => "Page numbers",
            Self::Progress => "Book progress",
            Self::Chapter => "Current chapter",
            Self::Remaining => "Chapter pages left",
            Self::Minimal => "Minimal",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::PageNumbers => Self::Progress,
            Self::Progress => Self::Chapter,
            Self::Chapter => Self::Remaining,
            Self::Remaining => Self::Minimal,
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
    pub fn load(path: &Path) -> Self {
        with_storage(|| {
            let file = fs::File::open(path)?;
            let mut bytes = Vec::new();
            file.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes)?;
            if bytes.len() as u64 > MAX_FILE_BYTES {
                return Err(io::Error::other("Reading controls settings are too large"));
            }
            serde_json::from_slice(&bytes).map_err(io::Error::other)
        })
        .unwrap_or_default()
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
        assert_eq!(controls.page_refresh, PageRefresh::Default);
        let mut since_last = 0;
        assert_eq!(
            controls.page_refresh.after_page(&mut since_last),
            Change::Partial
        );
        assert_eq!(since_last, 0);
    }
}
