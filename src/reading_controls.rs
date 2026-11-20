use std::fs;
use std::io::{self, Read};
use std::path::Path;

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
