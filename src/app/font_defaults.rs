//! Reader-wide reading font defaults: a default font, used by books that have no font choice,
//! and one font for each generic family. KOReader keeps the same kinds of choices as
//! `fallback_font`, `monospace_font` and `cre_font_family_fonts`.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

use primitive_app::with_storage;
use serde::{Deserialize, Serialize};

use crate::reading_fonts;

const MAX_BYTES: u64 = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Role {
    Default,
    Monospace,
    Serif,
    SansSerif,
    Cursive,
    Fantasy,
    Emoji,
    FangSong,
    Math,
}

impl Role {
    pub(crate) const ALL: [Role; 9] = [
        Role::Default,
        Role::Monospace,
        Role::Serif,
        Role::SansSerif,
        Role::Cursive,
        Role::Fantasy,
        Role::Emoji,
        Role::FangSong,
        Role::Math,
    ];

    /// The key in the saved file, which follows KOReader's family tags.
    pub(crate) fn key(self) -> &'static str {
        match self {
            Role::Default => "default",
            Role::Monospace => "monospace",
            Role::Serif => "serif",
            Role::SansSerif => "sans-serif",
            Role::Cursive => "cursive",
            Role::Fantasy => "fantasy",
            Role::Emoji => "emoji",
            Role::FangSong => "fangsong",
            Role::Math => "math",
        }
    }

    pub(crate) fn title(self) -> &'static str {
        match self {
            Role::Default => "Default font",
            Role::Monospace => "Monospace font",
            Role::Serif => "Serif font",
            Role::SansSerif => "Sans-serif font",
            Role::Cursive => "Cursive font",
            Role::Fantasy => "Fantasy font",
            Role::Emoji => "Emoji font",
            Role::FangSong => "Fangsong font",
            Role::Math => "Math font",
        }
    }

    /// The generic family a file serves. The default font has none: it is the book's primary font.
    pub(crate) fn generic(self) -> Option<parley::GenericFamily> {
        use parley::GenericFamily as G;
        Some(match self {
            Role::Default => return None,
            Role::Monospace => G::Monospace,
            Role::Serif => G::Serif,
            Role::SansSerif => G::SansSerif,
            Role::Cursive => G::Cursive,
            Role::Fantasy => G::Fantasy,
            Role::Emoji => G::Emoji,
            Role::FangSong => G::FangSong,
            Role::Math => G::Math,
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct FontDefaults {
    files: BTreeMap<String, String>,
    /// Font files most recently chosen in the chooser, newest first.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    recent: Vec<String>,
}

const MAX_RECENT: usize = 8;

/// The chooser's order: recently chosen fonts first, newest first, then the rest by name.
pub(crate) fn ordered(names: &[String], recent: &[String]) -> Vec<String> {
    let mut ordered: Vec<String> = recent
        .iter()
        .filter(|name| names.contains(name))
        .cloned()
        .collect();
    ordered.extend(names.iter().filter(|name| !recent.contains(name)).cloned());
    ordered
}

impl FontDefaults {
    pub(crate) fn get(&self, role: Role) -> Option<&str> {
        self.files.get(role.key()).map(String::as_str)
    }

    pub(crate) fn recent(&self) -> &[String] {
        &self.recent
    }

    /// Put a font at the front of the recent list.
    pub(crate) fn remember(&mut self, file: &str) {
        self.recent.retain(|name| name != file);
        self.recent.insert(0, file.to_owned());
        self.recent.truncate(MAX_RECENT);
    }

    pub(crate) fn set(&mut self, role: Role, file: Option<String>) {
        match file {
            Some(file) => {
                self.files.insert(role.key().to_owned(), file);
            }
            None => {
                self.files.remove(role.key());
            }
        }
    }

    /// The font files for every family role that has one. Files that no longer exist are left out.
    pub(crate) fn roles(&self, fonts_directory: &Path) -> crate::FontRoles {
        let mut entries = Vec::new();
        for role in Role::ALL {
            if let (Some(generic), Some(file)) = (role.generic(), self.get(role))
                && let Ok(path) = reading_fonts::resolve(fonts_directory, file)
            {
                entries.push((generic, path));
            }
        }
        crate::FontRoles::new(entries)
    }
}

pub(crate) fn load(path: &Path, fonts_directory: &Path) -> FontDefaults {
    let mut defaults = with_storage(|| {
        let metadata = fs::symlink_metadata(path)?;
        if !metadata.is_file() || metadata.len() > MAX_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid font defaults",
            ));
        }
        serde_json::from_slice::<FontDefaults>(&fs::read(path)?).map_err(io::Error::other)
    })
    .unwrap_or_default();
    defaults
        .files
        .retain(|key, file| known_key(key) && reading_fonts::resolve(fonts_directory, file).is_ok());
    defaults
}

pub(crate) fn save(path: &Path, fonts_directory: &Path, defaults: &FontDefaults) -> io::Result<()> {
    for (key, file) in &defaults.files {
        if !known_key(key) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Unknown font role"));
        }
        reading_fonts::resolve(fonts_directory, file)?;
    }
    let bytes = serde_json::to_vec(defaults).map_err(io::Error::other)?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Font defaults are too long",
        ));
    }
    primitive_app::save(path, &bytes)
}

fn known_key(key: &str) -> bool {
    Role::ALL.iter().any(|role| role.key() == key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_fonts_lead_the_chooser_in_recency_order() {
        let names: Vec<String> = ["A.ttf", "B.ttf", "C.ttf", "D.ttf"].map(String::from).to_vec();
        let mut defaults = FontDefaults::default();
        defaults.remember("C.ttf");
        defaults.remember("A.ttf");
        defaults.remember("C.ttf");
        assert_eq!(defaults.recent(), ["C.ttf", "A.ttf"]);
        assert_eq!(
            ordered(&names, defaults.recent()),
            ["C.ttf", "A.ttf", "B.ttf", "D.ttf"]
        );
        // A recent font that is gone is skipped, not shown.
        defaults.remember("Gone.ttf");
        assert_eq!(ordered(&names, defaults.recent())[0], "C.ttf");
        for index in 0..20 {
            defaults.remember(&format!("F{index}.ttf"));
        }
        assert_eq!(defaults.recent().len(), MAX_RECENT);
    }

    #[test]
    fn every_role_has_a_distinct_key_and_only_families_have_a_generic() {
        let mut keys: Vec<_> = Role::ALL.iter().map(|role| role.key()).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), Role::ALL.len());
        assert_eq!(Role::Default.generic(), None);
        assert!(Role::ALL.iter().skip(1).all(|role| role.generic().is_some()));
    }
}
