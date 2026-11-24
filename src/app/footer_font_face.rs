//! Persistent face selection for the reading status footer.

use std::fs;
use std::io;
use std::path::Path;

use primitive_app::with_storage;

use crate::reading_fonts;

const MAX_BYTES: u64 = 512;

pub(crate) fn load(path: &Path, fonts_directory: &Path) -> Option<String> {
    with_storage(|| {
        let metadata = fs::symlink_metadata(path)?;
        if !metadata.is_file() || metadata.len() > MAX_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid status bar font preference",
            ));
        }
        let value: Option<String> =
            serde_json::from_slice(&fs::read(path)?).map_err(io::Error::other)?;
        Ok(value)
    })
    .ok()
    .flatten()
    .filter(|name| reading_fonts::resolve(fonts_directory, name).is_ok())
}

pub(crate) fn save(path: &Path, fonts_directory: &Path, file: Option<&str>) -> io::Result<()> {
    if let Some(file) = file {
        reading_fonts::resolve(fonts_directory, file)?;
    }
    let bytes = serde_json::to_vec(&file).map_err(io::Error::other)?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Status bar font preference is too long",
        ));
    }
    primitive_app::save(path, &bytes)
}
