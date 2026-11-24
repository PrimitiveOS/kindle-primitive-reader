//! Conditions evaluated for a particular book-opening event.
use crate::library::{Library, MAX_COLLECTIONS, collection_name, fold_search};
use crate::reading_controls::ReadingOrientation;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, path::Path};

pub(crate) const PATH_LIMIT: usize = 160;

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
}

pub(crate) fn valid_path(pattern: &str) -> bool {
    pattern == pattern.trim()
        && pattern.chars().count() <= PATH_LIMIT
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
        !self.orientations.is_empty() || self.path.is_some() || !self.collections.is_empty()
    }

    pub(super) fn matches(&self, book: &Book<'_>) -> bool {
        if self.new_only && !book.is_new {
            return false;
        }
        self.orientations.contains(&book.orientation)
            || self.path.as_ref().is_some_and(|pattern| {
                let path = book.path.to_string_lossy().to_lowercase();
                pattern
                    .split(',')
                    .any(|part| path.contains(&part.trim().to_lowercase()))
            })
            || self.collections.iter().any(|name| {
                book.library
                    .collection(name)
                    .is_some_and(|collection| collection.contains(book.id))
            })
    }
}
