//! Conditions evaluated for a particular book-opening event.
use crate::library::{Library, MAX_COLLECTIONS, collection_name, fold_search};
use crate::reading_controls::ReadingOrientation;
use icu_casemap::CaseMapper;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

pub(crate) mod path;

pub(crate) const PATH_LIMIT: usize = 160;
pub(crate) const METADATA_LIMIT: usize = 160;
const MAX_METADATA_VALUE_BYTES: usize = 16 * 1024;
const MAX_METADATA_VALUES: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum MetadataField {
    Title,
    Authors,
    Series,
    SeriesIndex,
    Language,
    Keywords,
    Description,
}

impl MetadataField {
    pub(crate) const ALL: [Self; 7] = [
        Self::Title,
        Self::Authors,
        Self::Series,
        Self::SeriesIndex,
        Self::Language,
        Self::Keywords,
        Self::Description,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Title => "Title",
            Self::Authors => "Authors",
            Self::Series => "Series",
            Self::SeriesIndex => "Series index",
            Self::Language => "Language",
            Self::Keywords => "Keywords",
            Self::Description => "Description",
        }
    }
}

#[derive(Default)]
pub(crate) struct Metadata(BTreeMap<MetadataField, Vec<String>>);

impl Metadata {
    pub(crate) fn add(&mut self, field: MetadataField, value: &str) {
        let end = value.floor_char_boundary(value.len().min(MAX_METADATA_VALUE_BYTES));
        let value = value[..end].trim();
        if value.is_empty() {
            return;
        }
        let values = self.0.entry(field).or_default();
        if values.len() < MAX_METADATA_VALUES && !values.iter().any(|saved| saved == value) {
            values.push(value.to_owned());
        }
    }

    pub(crate) fn current_phrase(&self, field: MetadataField) -> Option<String> {
        self.0.get(&field)?.iter().find_map(|value| {
            value
                .split(|c: char| c == ',' || c.is_control())
                .find_map(|part| {
                    let phrase: String = part.trim().chars().take(METADATA_LIMIT).collect();
                    valid_metadata(phrase.trim()).then(|| phrase.trim().to_owned())
                })
        })
    }

    fn matches(&self, field: MetadataField, pattern: &str) -> bool {
        let Some(values) = self.0.get(&field) else {
            return false;
        };
        let lowercase = |text: &str| -> String {
            let mapper = CaseMapper::new();
            text.chars().map(|c| mapper.simple_lowercase(c)).collect()
        };
        let needles: Vec<_> = pattern.split(',').map(str::trim).map(lowercase).collect();
        values.iter().map(|text| lowercase(text)).any(|text| {
            needles
                .iter()
                .any(|needle| !needle.is_empty() && text.contains(needle))
        })
    }
}

pub(crate) const ORIENTATIONS: [ReadingOrientation; 5] = [
    ReadingOrientation::FollowLauncher,
    ReadingOrientation::Upright,
    ReadingOrientation::UpsideDown,
    ReadingOrientation::LandscapeLeft,
    ReadingOrientation::LandscapeRight,
];

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Conditions {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub orientations: Vec<ReadingOrientation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<MetadataField, String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub collections: Vec<String>,
    pub new_only: bool,
}

pub(crate) struct Book<'a> {
    pub(crate) id: u64,
    pub(crate) path: &'a Path,
    pub(crate) library: &'a Library,
    pub(crate) orientation: ReadingOrientation,
    pub(crate) is_new: bool,
    pub(crate) metadata: Option<&'a Metadata>,
    pub(crate) local_minute: Option<u16>,
    pub(crate) download_folder: &'a Path,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TimeWindow {
    pub(crate) start: u16,
    pub(crate) end: u16,
}

impl TimeWindow {
    pub(super) fn valid(self) -> bool {
        self.start < 1440 && self.end < 1440
    }

    pub(crate) fn includes(self, minute: u16) -> bool {
        if self.start < self.end {
            (self.start..=self.end).contains(&minute)
        } else {
            minute < 1440 && (minute >= self.start || minute <= self.end)
        }
    }
}

pub(crate) fn valid_path(pattern: &str) -> bool {
    valid_pattern(pattern, PATH_LIMIT)
}

pub(crate) fn valid_metadata(pattern: &str) -> bool {
    valid_pattern(pattern, METADATA_LIMIT)
}

fn valid_pattern(pattern: &str, limit: usize) -> bool {
    pattern == pattern.trim()
        && pattern.chars().count() <= limit
        && !pattern.chars().any(char::is_control)
        && pattern.split(',').all(|part| !part.trim().is_empty())
}

impl Conditions {
    pub(super) fn valid(&self) -> bool {
        self.orientations.len() <= ORIENTATIONS.len()
            && self
                .orientations
                .iter()
                .enumerate()
                .all(|(index, mode)| !self.orientations[..index].contains(mode))
            && self.path.as_deref().is_none_or(valid_path)
            && self
                .metadata
                .values()
                .all(|pattern| valid_metadata(pattern))
            && self.collections.len() <= MAX_COLLECTIONS
            && {
                let mut seen = BTreeSet::new();
                self.collections.iter().all(|name| {
                    collection_name(name).is_ok_and(|trimmed| trimmed == name)
                        && seen.insert(fold_search(name))
                })
            }
    }

    pub(crate) fn has_matchers(&self) -> bool {
        !self.orientations.is_empty()
            || self.path.is_some()
            || !self.metadata.is_empty()
            || !self.collections.is_empty()
    }

    pub(super) fn matches(&self, book: &Book<'_>) -> bool {
        if self.new_only && !book.is_new {
            return false;
        }
        self.orientations.contains(&book.orientation)
            || self.path.as_ref().is_some_and(|pattern| {
                path::matches(pattern, book.path, book.library, book.download_folder)
            })
            || book.metadata.is_some_and(|metadata| {
                self.metadata
                    .iter()
                    .any(|(&field, pattern)| metadata.matches(field, pattern))
            })
            || self.collections.iter().any(|name| {
                book.library
                    .collection(name)
                    .is_some_and(|collection| collection.contains(book.id))
            })
    }
}
