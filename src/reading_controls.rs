use std::fs;
use std::io::{self, Read};
use std::path::Path;

use primitive_app::{WIDTH, with_storage};
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
    pub fn page_input(self, input: Input) -> Input {
        match input {
            Input::Tap(x, y) if self.left_touch && y < PAGE_HEIGHT as f32 => {
                Input::Tap(WIDTH as f32 - x, y)
            }
            _ => input,
        }
    }
}
