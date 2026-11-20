//! Reader-wide typography presets applied to individual reflowable books.

use std::fs;
use std::io::{self, Read};
use std::path::Path;

use primitive_app::with_storage;
use serde::{Deserialize, Serialize};

use crate::library::BookPreferences;

const MAX_BYTES: u64 = 32 * 1024;
const MAX_PROFILES: usize = 16;
pub(crate) const MAX_NAME: usize = 48;

#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct Profile {
    pub name: String,
    pub preferences: BookPreferences,
}

pub(crate) fn load(path: &Path) -> Vec<Profile> {
    with_storage(|| {
        let info = fs::symlink_metadata(path)?;
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
        let profiles: Vec<Profile> = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
        if profiles.len() > MAX_PROFILES || profiles.iter().any(|item| !valid_name(&item.name)) {
            return Err(invalid());
        }
        Ok(profiles)
    })
    .unwrap_or_default()
}

pub(crate) fn insert(
    profiles: &mut Vec<Profile>,
    name: &str,
    preferences: BookPreferences,
) -> io::Result<()> {
    if !valid_name(name) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Enter a profile name up to 48 characters",
        ));
    }
    let folded = name.to_lowercase();
    if profiles
        .iter()
        .any(|profile| profile.name.to_lowercase() == folded)
    {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "A reading profile already uses that name",
        ));
    }
    if profiles.len() == MAX_PROFILES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Reader reached its reading profile limit",
        ));
    }
    profiles.push(Profile {
        name: name.to_owned(),
        preferences,
    });
    Ok(())
}

pub(crate) fn save(path: &Path, profiles: &[Profile]) -> io::Result<()> {
    let bytes = serde_json::to_vec(profiles).map_err(io::Error::other)?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(invalid());
    }
    primitive_app::save(path, &bytes)
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
