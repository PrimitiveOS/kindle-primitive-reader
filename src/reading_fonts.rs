//! User-provided fonts in the Reader data directory.

use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use primitive_app::with_storage;

const MAX_FONT_BYTES: u64 = 12 * 1024 * 1024;
const MAX_FONT_ENTRIES: usize = 256;

pub(crate) fn available(directory: &Path) -> io::Result<Vec<String>> {
    with_storage(|| {
        let entries = match fs::read_dir(directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error),
        };
        if !fs::symlink_metadata(directory)?.is_dir() {
            return Ok(Vec::new());
        }
        let mut fonts: Vec<_> = entries
            .take(MAX_FONT_ENTRIES)
            .filter_map(Result::ok)
            .filter_map(|entry| entry.file_name().into_string().ok())
            .filter(|name| font_file(directory, name).is_ok())
            .collect();
        fonts.sort_unstable_by_key(|name| name.to_lowercase());
        Ok(fonts)
    })
}

pub(crate) fn resolve(directory: &Path, name: &str) -> io::Result<PathBuf> {
    with_storage(|| font_file(directory, name))
}

fn font_file(directory: &Path, name: &str) -> io::Result<PathBuf> {
    let path = Path::new(name);
    let valid_name = name.len() <= 255
        && !name.chars().any(|c| c.is_control() || c == '\\')
        && path.file_name() == Some(OsStr::new(name))
        && path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("ttf") || ext.eq_ignore_ascii_case("otf"));
    if !valid_name || !fs::symlink_metadata(directory)?.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Choose a TTF or OTF from Reader's font folder",
        ));
    }
    let path = directory.join(name);
    let metadata = fs::symlink_metadata(&path)?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_FONT_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Reader's font must be a regular file up to 12 MB",
        ));
    }
    Ok(path)
}
