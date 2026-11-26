//! Read package-relative members from a ZIP or an extracted EPUB on Kindle storage.
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use crate::Result;
use crate::epub::{MAX_ARCHIVE, MAX_ENTRIES, is_archive_path};
use crate::epub_zip;
use crate::library::fingerprint;

pub(crate) fn is_epub_path(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("epub") || ext.eq_ignore_ascii_case("epub3"))
}

pub(crate) fn is_folder_book(path: &Path) -> bool {
    path.file_name().is_some_and(|n| n == "mimetype") && path.parent().is_some_and(is_epub_path)
}

pub(crate) enum Members {
    Zip(epub_zip::Archive),
    Folder(PathBuf),
    #[cfg(feature = "kindle")]
    Snapshot(std::sync::Arc<std::collections::BTreeMap<String, std::sync::Arc<[u8]>>>),
}

impl Members {
    pub(crate) fn missing_after_recovery(&self, name: &str) -> bool {
        matches!(self, Self::Zip(archive) if archive.missing.contains_key(name.as_bytes()))
    }

    pub(crate) fn open(path: &Path) -> Result<Self> {
        if is_folder_book(path) {
            return Ok(Self::Folder(path.parent().unwrap().canonicalize()?));
        }
        let file = File::open(path)?;
        let metadata = file.metadata()?;
        if !metadata.is_file() || metadata.len() > MAX_ARCHIVE {
            return Err("The EPUB file exceeds the archive limit or is unavailable".into());
        }
        Ok(Self::Zip(epub_zip::open(file, metadata.len())?))
    }

    #[cfg(feature = "kindle")]
    pub(crate) fn size(&mut self, name: &str) -> Result<u64> {
        match self {
            Self::Zip(zip) => {
                if let Some(&size) = zip.missing.get(name.as_bytes()) {
                    return Ok(u64::from(size));
                }
                let index = zip
                    .zip
                    .index_for_name(name)
                    .ok_or("A chapter is unavailable")?;
                Ok(zip.zip.by_index_raw(index)?.size())
            }
            Self::Folder(root) => Ok(folder_file(root, name)?.metadata()?.len()),
            #[cfg(feature = "kindle")]
            Self::Snapshot(members) => {
                Ok(members.get(name).ok_or("A chapter is unavailable")?.len() as u64)
            }
        }
    }

    pub(crate) fn read(&mut self, name: &str, limit: usize) -> Result<Vec<u8>> {
        match self {
            Self::Zip(zip) => {
                let entry = zip.zip.by_name(name)?;
                if entry.is_dir() {
                    return Err("An EPUB resource points to a directory".into());
                }
                let size = entry.size();
                read_member(entry, name, size, limit)
            }
            Self::Folder(root) => {
                let file = folder_file(root, name)?;
                let size = file.metadata()?.len();
                read_member(file, name, size, limit)
            }
            #[cfg(feature = "kindle")]
            Self::Snapshot(members) => {
                if !is_archive_path(name) {
                    return Err("Invalid extracted EPUB member path".into());
                }
                let bytes = members.get(name).ok_or("An EPUB resource is unavailable")?;
                if bytes.len() > limit {
                    return Err(format!("The EPUB resource exceeds its size limit: {name}").into());
                }
                Ok(bytes.to_vec())
            }
        }
    }
}

fn folder_file(root: &Path, name: &str) -> Result<File> {
    if !is_archive_path(name) || name.len() > 4096 || name.split('/').count() > 32 {
        return Err("Invalid extracted EPUB member path".into());
    }
    let mut path = root.to_owned();
    for part in name.split('/') {
        path.push(part);
        if fs::symlink_metadata(&path)?.is_symlink() {
            return Err("An extracted EPUB resource cannot be a symbolic link".into());
        }
    }
    let canonical = path.canonicalize()?;
    if !canonical.starts_with(root) || !canonical.is_file() {
        return Err("Choose a resource inside this EPUB folder".into());
    }
    Ok(File::open(canonical)?)
}

fn read_member(source: impl Read, name: &str, size: u64, limit: usize) -> Result<Vec<u8>> {
    if size > limit as u64 {
        return Err(format!("The EPUB resource exceeds its size limit: {name}").into());
    }
    let mut bytes = Vec::with_capacity(size as usize);
    source.take(size + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 != size {
        return Err(
            format!("The EPUB resource length does not match its directory entry: {name}").into(),
        );
    }
    Ok(bytes)
}

/// Include ordered member names and content, so editing a chapter invalidates its old
/// positions while moving the publication preserves them. No directory is repackaged.
pub(crate) fn folder_identity(marker: &Path) -> io::Result<u64> {
    let root = marker
        .parent()
        .ok_or(io::ErrorKind::InvalidInput)?
        .canonicalize()?;
    let mut pending = vec![root.clone()];
    let mut members = Vec::new();
    let mut total = 0u64;
    let mut visited = 0;
    while let Some(folder) = pending.pop() {
        for child in fs::read_dir(folder)? {
            let child = child?;
            let path = child.path();
            let relative = path.strip_prefix(&root).map_err(io::Error::other)?;
            visited += 1;
            if visited > MAX_ENTRIES || relative.components().count() > 32 {
                return Err(io::Error::other("This extracted EPUB has too many entries"));
            }
            let kind = child.file_type()?;
            if child.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            if kind.is_dir() {
                pending.push(path);
            } else if kind.is_file() {
                let size = child.metadata()?.len();
                total = total
                    .checked_add(size)
                    .filter(|n| *n <= MAX_ARCHIVE)
                    .ok_or_else(|| {
                        io::Error::other("This extracted EPUB exceeds the format limit")
                    })?;
                members.push((relative.to_owned(), size));
            }
        }
    }
    members.sort();
    let mut signature = Vec::with_capacity(members.len() * 8);
    for (relative, size) in members {
        let name = relative.to_str().ok_or(io::ErrorKind::InvalidData)?;
        let file = folder_file(&root, name).map_err(io::Error::other)?;
        signature.extend(fingerprint(file, size, name.as_bytes(), size)?.to_le_bytes());
    }
    fingerprint(
        &signature[..],
        total,
        b"EPUB directory",
        signature.len() as u64,
    )
}
