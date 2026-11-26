//! Unwired, bounded capture of the configured font files for a long chapter.
//!
//! These snapshots bind only the named files and their read bytes. They do not
//! establish the live Reader's font selection, system fallback or layout.
use crate::session::screen_pages::FileStamp;
use primitive_app::with_storage;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

const MAX_FONTS: usize = 8;
const MAX_BYTES: usize = 16 * 1024 * 1024;

struct Entry {
    path: PathBuf,
    stamp: FileStamp,
    bytes: Arc<Vec<u8>>,
    digest: [u8; 32],
}

/// Ordered immutable inputs; never a proof of the complete resolved font set.
pub(crate) struct FontSnapshots {
    entries: Vec<Entry>,
}

fn check(cancel: &AtomicBool) -> io::Result<()> {
    if cancel.load(Ordering::Relaxed) {
        Err(io::Error::other("Font capture was cancelled"))
    } else {
        Ok(())
    }
}

#[cfg(unix)]
fn open(path: &Path) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(path)?;
    if !file.metadata()?.is_file() {
        return Err(io::Error::other("A reading font is not a regular file"));
    }
    Ok(file)
}

#[cfg(not(unix))]
fn open(_path: &Path) -> io::Result<File> {
    Err(io::Error::other(
        "Font capture is unavailable on this platform",
    ))
}

fn stamp(file: &File) -> io::Result<FileStamp> {
    FileStamp::from_metadata(&file.metadata()?)
        .ok_or_else(|| io::Error::other("A reading font cannot be verified"))
}

fn recheck(entries: &[Entry], cancel: &AtomicBool) -> io::Result<()> {
    for entry in entries {
        check(cancel)?;
        if stamp(&open(&entry.path)?)? != entry.stamp {
            return Err(io::Error::other("A reading font changed during capture"));
        }
    }
    check(cancel)
}

impl FontSnapshots {
    /// Call only on a worker. A failed or cancelled capture returns no inputs.
    pub(crate) fn capture(paths: &[PathBuf], cancel: &AtomicBool) -> io::Result<Self> {
        if paths.is_empty() || paths.len() > MAX_FONTS {
            return Err(io::Error::other("The reading font stack is unavailable"));
        }
        with_storage(|| {
            let mut entries = Vec::with_capacity(paths.len());
            let mut total = 0usize;
            for path in paths {
                check(cancel)?;
                let mut file = open(path)?;
                let metadata = file.metadata()?;
                let expected = FileStamp::from_metadata(&metadata)
                    .ok_or_else(|| io::Error::other("A reading font cannot be verified"))?;
                let size = usize::try_from(metadata.len())
                    .map_err(|_| io::Error::other("A reading font is too large"))?;
                total = total
                    .checked_add(size)
                    .ok_or_else(|| io::Error::other("The reading fonts are too large"))?;
                if size == 0 || total > MAX_BYTES {
                    return Err(io::Error::other("The reading fonts are too large or empty"));
                }
                let mut bytes = Vec::new();
                bytes.try_reserve_exact(size).map_err(io::Error::other)?;
                let mut hash = ring::digest::Context::new(&ring::digest::SHA256);
                let mut buffer = [0; 32 * 1024];
                loop {
                    check(cancel)?;
                    let n = file.read(&mut buffer)?;
                    if n == 0 {
                        break;
                    }
                    if n > size.saturating_sub(bytes.len()) {
                        return Err(io::Error::other("A reading font changed during capture"));
                    }
                    hash.update(&buffer[..n]);
                    bytes.extend_from_slice(&buffer[..n]);
                }
                if bytes.len() != size || stamp(&file)? != expected {
                    return Err(io::Error::other("A reading font changed during capture"));
                }
                let mut digest = [0; 32];
                digest.copy_from_slice(hash.finish().as_ref());
                entries.push(Entry {
                    path: path.clone(),
                    stamp: expected,
                    bytes: Arc::new(bytes),
                    digest,
                });
            }
            recheck(&entries, cancel)?;
            Ok(Self { entries })
        })
    }

    /// Check current metadata/path identity on a worker, not file contents.
    pub(crate) fn revalidate(&self, cancel: &AtomicBool) -> io::Result<()> {
        with_storage(|| recheck(&self.entries, cancel))
    }

    pub(crate) fn bytes(&self) -> Vec<Arc<Vec<u8>>> {
        self.entries
            .iter()
            .map(|entry| entry.bytes.clone())
            .collect()
    }

    pub(crate) fn digests(&self) -> Vec<[u8; 32]> {
        self.entries.iter().map(|entry| entry.digest).collect()
    }
}
