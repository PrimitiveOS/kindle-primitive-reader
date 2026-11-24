//! Reader-wide settings snapshots applied explicitly to an individual book.

use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use primitive_app::with_storage;
use serde::{Deserialize, Serialize};

use crate::document_defaults::{FixedDefaults, validate_preferences};
use crate::library::{BookPreferences, FixedPreferences, FixedTileAxis};

const MAX_BYTES: u64 = 32 * 1024;
const MAX_PROFILES: usize = 16;
pub(crate) const MAX_NAME: usize = 48;

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(untagged, deny_unknown_fields)]
pub(crate) enum Profile {
    Text {
        name: String,
        preferences: BookPreferences,
    },
    Pdf {
        name: String,
        fixed: PdfProfile,
    },
}

impl Profile {
    pub(crate) fn name(&self) -> &str {
        match self {
            Self::Text { name, .. } | Self::Pdf { name, .. } => name,
        }
    }

    #[cfg(test)]
    pub(crate) fn text_preferences(&self) -> &BookPreferences {
        match self {
            Self::Text { preferences, .. } => preferences,
            Self::Pdf { .. } => panic!("Expected a reflowable-book profile"),
        }
    }

    pub(crate) fn set_name(&mut self, value: &str) {
        match self {
            Self::Text { name, .. } | Self::Pdf { name, .. } => {
                *name = value.to_owned();
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PdfProfile {
    settings: FixedDefaults,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) tile: Option<Tile>,
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Tile {
    pub(crate) axis: FixedTileAxis,
    pub(crate) count_tenths: u8,
}

impl PdfProfile {
    pub(crate) fn from_view(view: FixedPreferences) -> Self {
        Self {
            settings: FixedDefaults::from_view(FixedPreferences {
                spread: false,
                right_to_left: false,
                epub_spread: None,
                epub_right_to_left: None,
                force_ocr: false,
                ..view
            }),
            tile: view.tile_zoom.map(|tile| Tile {
                axis: tile.axis,
                count_tenths: tile.count_tenths,
            }),
        }
    }

    pub(crate) fn apply(self, view: FixedPreferences) -> FixedPreferences {
        FixedPreferences {
            // The target's OCR policy belongs to the publication.
            force_ocr: view.force_ocr,
            ..self.settings.apply(view)
        }
    }

    fn is_valid(self) -> bool {
        let view = self.settings.apply(FixedPreferences::default());
        view.is_valid()
            && self.settings == Self::from_view(view).settings
            && (!view.continuous || view.width_fit)
            && self.tile.is_none_or(|tile| {
                (1..=80).contains(&tile.count_tenths)
                    && view.zoom == 0
                    && !view.width_fit
                    && !view.height_fit
                    && !view.continuous
            })
    }
}

fn read(path: &Path) -> io::Result<Vec<Profile>> {
    let info = match fs::symlink_metadata(path) {
        Ok(info) => info,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    if !info.is_file() || info.len() > MAX_BYTES {
        return Err(invalid());
    }
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(invalid());
    }
    decode(serde_json::from_slice(&bytes).map_err(io::Error::other)?)
}

pub(crate) fn load(path: &Path) -> Vec<Profile> {
    with_storage(|| read(path)).unwrap_or_default()
}

pub(crate) fn decode(value: serde_json::Value) -> io::Result<Vec<Profile>> {
    let profiles: Vec<Profile> = serde_json::from_value(value).map_err(io::Error::other)?;
    validate(&profiles)?;
    Ok(profiles)
}

fn validate(profiles: &[Profile]) -> io::Result<()> {
    let mut names = std::collections::BTreeSet::new();
    if profiles.len() > MAX_PROFILES {
        return Err(io::Error::other("Reader reached its reading profile limit"));
    }
    for profile in profiles {
        if !valid_name(profile.name()) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Enter a profile name up to 48 characters",
            ));
        }
        if !names.insert(profile.name().to_lowercase()) {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "A reading profile already uses that name",
            ));
        }
        match profile {
            Profile::Text { preferences, .. } => validate_preferences(preferences)?,
            Profile::Pdf { fixed, .. } if !fixed.is_valid() => return Err(invalid()),
            Profile::Pdf { .. } => {}
        }
    }
    Ok(())
}

pub(crate) fn save(path: &Path, previous: &[Profile], profiles: &[Profile]) -> io::Result<()> {
    validate(profiles)?;
    let bytes = serde_json::to_vec(profiles).map_err(io::Error::other)?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(invalid());
    }
    with_storage(|| {
        let existing = read(path).map_err(|_| {
            io::Error::other(
                "Saved reading profiles could not be read. The original file was kept.",
            )
        })?;
        if existing != previous {
            return Err(io::Error::other(
                "Reading profiles changed on disk. Reopen Reader before editing them.",
            ));
        }
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        let scratch = path.with_file_name(format!(
            ".reading-profiles-{}-{}.tmp",
            std::process::id(),
            stamp.as_nanos(),
        ));
        let mut file = fs::File::create_new(&scratch)?;
        let write = file.write_all(&bytes).and_then(|()| file.sync_all());
        drop(file);
        let result = write.and_then(|()| fs::rename(&scratch, path));
        if result.is_err() {
            let _ = fs::remove_file(scratch);
        }
        result
    })
}

fn valid_name(name: &str) -> bool {
    !name.trim().is_empty()
        && name.chars().count() <= MAX_NAME
        && !name.chars().any(char::is_control)
}

fn invalid() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "Reading profiles exceed Reader's supported format or limits",
    )
}
