//! Reader-wide settings snapshots applied explicitly to an individual book.

use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use primitive_app::with_storage;
use serde::{Deserialize, Serialize};

use crate::document_defaults::{FixedDefaults, validate_preferences};
use crate::library::{BookPreferences, FixedPreferences, FixedTileAxis};

mod fields;
mod values;
pub(crate) use fields::Field;
pub(crate) use values::{MarginEdge, ValueField};

const MAX_BYTES: u64 = 32 * 1024;
const MAX_PROFILES: usize = 16;
pub(crate) const MAX_NAME: usize = 48;

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(untagged, deny_unknown_fields)]
pub(crate) enum Profile {
    Text {
        name: String,
        preferences: BookPreferences,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fields: Option<Vec<Field>>,
    },
    Fixed {
        name: String,
        fixed: FixedProfile,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fields: Option<Vec<Field>>,
    },
}

impl Profile {
    pub(crate) fn name(&self) -> &str {
        match self {
            Self::Text { name, .. } | Self::Fixed { name, .. } => name,
        }
    }

    #[cfg(test)]
    pub(crate) fn text_preferences(&self) -> &BookPreferences {
        match self {
            Self::Text { preferences, .. } => preferences,
            Self::Fixed { .. } => panic!("Expected a reflowable-book profile"),
        }
    }

    pub(crate) fn set_name(&mut self, value: &str) {
        match self {
            Self::Text { name, .. } | Self::Fixed { name, .. } => *name = value.to_owned(),
        }
    }

    pub(crate) fn available(&self) -> Vec<Field> {
        match self {
            Self::Text { .. } => Field::TEXT.to_vec(),
            Self::Fixed { fixed, .. } => fixed.provider.fields().collect(),
        }
    }

    pub(crate) fn selection(&self) -> Vec<Field> {
        self.mask().clone().unwrap_or_else(|| self.available())
    }

    fn mask(&self) -> &Option<Vec<Field>> {
        match self {
            Self::Text { fields, .. } | Self::Fixed { fields, .. } => fields,
        }
    }

    pub(crate) fn set_fields(&mut self, selection: Vec<Field>) -> io::Result<()> {
        let available = self.available();
        validate_selection(&selection, &available)?;
        let ordered: Vec<_> = available
            .iter()
            .copied()
            .filter(|field| selection.contains(field))
            .collect();
        let fields = (ordered != available).then_some(ordered);
        match self {
            Self::Text { fields: stored, .. } | Self::Fixed { fields: stored, .. } => {
                *stored = fields
            }
        }
        Ok(())
    }

    pub(crate) fn apply_text(&self, current: &BookPreferences) -> Option<BookPreferences> {
        let Self::Text {
            preferences,
            fields,
            ..
        } = self
        else {
            return None;
        };
        let Some(fields) = fields else {
            return Some(preferences.clone());
        };
        let mut settings = current.clone();
        for field in fields {
            field.merge_text(preferences, &mut settings);
        }
        Some(settings)
    }

    pub(crate) fn update_from(&mut self, snapshot: Self) -> io::Result<()> {
        let selection = self.selection();
        match (self, snapshot) {
            (
                Self::Text { preferences, .. },
                Self::Text {
                    preferences: current,
                    ..
                },
            ) => {
                for field in selection {
                    field.merge_text(&current, preferences);
                }
            }
            (Self::Fixed { fixed, .. }, Self::Fixed { fixed: current, .. })
                if fixed.provider == current.provider =>
            {
                let mut values = fixed.values();
                let tile = if selection.contains(&Field::FitAndZoom) {
                    current.tile
                } else {
                    fixed.tile
                };
                for field in selection {
                    field.merge_view(current.values(), &mut values);
                }
                fixed.settings = FixedDefaults::from_view(values);
                fixed.tile = tile;
            }
            _ => {
                return Err(io::Error::other(
                    "Open a book that matches this profile's format",
                ));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FixedProvider {
    #[default]
    Pdf,
    Djvu,
    Comic,
}

impl FixedProvider {
    fn is_pdf(&self) -> bool {
        *self == Self::Pdf
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Pdf => "PDF documents",
            Self::Djvu => "DjVu documents",
            Self::Comic => "Comics",
        }
    }

    fn fields(self) -> impl Iterator<Item = Field> {
        Field::FIXED
            .iter()
            .copied()
            .filter(move |field| match field {
                Field::CropMargin | Field::ScrollGap => self == Self::Pdf,
                Field::Straighten | Field::TextSpacing => self != Self::Comic,
                Field::Spreads | Field::ReadingDirection => self == Self::Comic,
                _ => true,
            })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FixedProfile {
    #[serde(default, skip_serializing_if = "FixedProvider::is_pdf")]
    pub(crate) provider: FixedProvider,
    settings: FixedDefaults,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tile: Option<Tile>,
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Tile {
    pub(crate) axis: FixedTileAxis,
    pub(crate) count_tenths: u8,
}

impl FixedProfile {
    pub(crate) fn from_view(provider: FixedProvider, view: FixedPreferences) -> Self {
        let mut portable = FixedPreferences::default();
        for field in provider.fields() {
            field.merge_view(view, &mut portable);
        }
        if provider != FixedProvider::Pdf {
            portable.continuous = false;
        }
        if provider == FixedProvider::Djvu {
            portable.scan_gamma_tenths = None;
        }
        Self {
            provider,
            settings: FixedDefaults::from_view(portable),
            tile: view
                .tile_zoom
                .filter(|_| provider != FixedProvider::Comic)
                .map(|tile| Tile {
                    axis: tile.axis,
                    count_tenths: tile.count_tenths,
                }),
        }
    }

    fn values(self) -> FixedPreferences {
        self.settings.apply(FixedPreferences::default())
    }

    fn set_overlap(&mut self, horizontal: bool, value: u8) {
        let mut values = self.values();
        if horizontal {
            values.zoom_overlap_h = Some(value);
        } else {
            values.zoom_overlap_v = Some(value);
        }
        self.settings = FixedDefaults::from_view(values);
    }

    pub(crate) fn project(
        self,
        fields: &[Field],
        mut view: FixedPreferences,
    ) -> (FixedPreferences, Option<Tile>) {
        let values = self.values();
        for field in fields {
            field.merge_view(values, &mut view);
        }
        let tile = if fields.contains(&Field::FitAndZoom) {
            view.tile_zoom = None;
            self.tile
        } else {
            view.tile_zoom.map(|tile| Tile {
                axis: tile.axis,
                count_tenths: tile.count_tenths,
            })
        };
        (view, tile)
    }

    fn is_valid(self) -> bool {
        let view = self.values();
        view.is_valid()
            && self.settings == Self::from_view(self.provider, view).settings
            && (!view.continuous || view.width_fit)
            && self.tile.is_none_or(|tile| {
                self.provider != FixedProvider::Comic
                    && (1..=80).contains(&tile.count_tenths)
                    && view.zoom == 0
                    && !view.width_fit
                    && !view.height_fit
                    && !view.continuous
            })
    }
}

fn validate_selection(selection: &[Field], available: &[Field]) -> io::Result<()> {
    if selection.is_empty() {
        return Err(io::Error::other("Include at least one setting"));
    }
    let supported = available
        .iter()
        .filter(|field| selection.contains(field))
        .count();
    if supported != selection.len() {
        return Err(invalid());
    }
    Ok(())
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
        if let Some(fields) = profile.mask() {
            validate_selection(fields, &profile.available())?;
        }
        match profile {
            Profile::Text { preferences, .. } => validate_preferences(preferences)?,
            Profile::Fixed { fixed, .. } if !fixed.is_valid() => return Err(invalid()),
            Profile::Fixed { .. } => {}
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
