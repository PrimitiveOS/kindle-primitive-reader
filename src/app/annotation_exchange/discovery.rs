//! Read-only discovery of an adjacent KOReader annotation transfer.
//!
//! Run `discover` on a cancellable worker. A positive result is only a hint:
//! the ordinary preview/import path must still verify the book edition and
//! transfer again before displaying changes or applying anything.

use crate::koreader_sidecar::{MAX_SIDECAR_BYTES, exchange};
use crate::session::screen_pages::FileStamp;
use std::io::{self, Read};
use std::path::{Component, Path};
use std::sync::atomic::{AtomicBool, Ordering};

const MAX_COMPONENTS: usize = 32;

fn check(cancel: &AtomicBool) -> io::Result<()> {
    if cancel.load(Ordering::Relaxed) {
        Err(io::Error::other("Transfer discovery was cancelled"))
    } else {
        Ok(())
    }
}

/// Return the exact transfer digest for an eligible foreign-device candidate.
/// The caller owns worker lifetime, cancellation, and stale-result checks.
pub(super) fn discover(
    root: &Path,
    book: &Path,
    fixed: bool,
    current_device: Option<&str>,
    applied_digest: Option<&str>,
    cancel: &AtomicBool,
) -> io::Result<Option<String>> {
    let (bytes, digest) = primitive_app::with_storage(|| read_adjacent(root, book, cancel))?;
    check(cancel)?;
    if applied_digest == Some(digest.as_str()) {
        return Ok(None);
    }
    // This parser has its own bounded depth, record and field limits. It is a
    // synchronous operation and cannot be interrupted until it returns.
    let snapshot = match exchange::decode(&bytes) {
        Ok(snapshot) => snapshot,
        Err(_) => return Ok(None),
    };
    check(cancel)?;
    Ok(
        (snapshot.fixed == fixed && current_device != Some(snapshot.device.as_str()))
            .then_some(digest),
    )
}

#[cfg(unix)]
fn read_adjacent(root: &Path, book: &Path, cancel: &AtomicBool) -> io::Result<(Vec<u8>, String)> {
    use std::ffi::{CString, OsStr};
    use std::fs::{File, OpenOptions};
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};

    fn open_child(parent: &File, name: &OsStr, directory: bool) -> io::Result<File> {
        let bytes = name.as_bytes();
        if bytes.is_empty() || bytes.len() > 255 {
            return Err(io::Error::other("The transfer path is unavailable"));
        }
        let name = CString::new(bytes).map_err(io::Error::other)?;
        let mut flags = libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC;
        if directory {
            flags |= libc::O_DIRECTORY;
        }
        // SAFETY: the parent descriptor is live and the component is NUL terminated.
        let fd = unsafe { libc::openat(parent.as_raw_fd(), name.as_ptr(), flags) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: openat returned a newly owned descriptor.
        Ok(unsafe { File::from_raw_fd(fd) })
    }

    check(cancel)?;
    let mut directory = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(root)?;
    let mut parts = book.components().peekable();
    let mut count = 0usize;
    let name = loop {
        let Some(Component::Normal(name)) = parts.next() else {
            return Err(io::Error::other("The book path is unavailable"));
        };
        count += 1;
        if count > MAX_COMPONENTS {
            return Err(io::Error::other("The book path is too deep"));
        }
        if parts.peek().is_none() {
            break name;
        }
        check(cancel)?;
        directory = open_child(&directory, name, true)?;
    };
    let book_file = open_child(&directory, name, false)?;
    let book_stamp = FileStamp::from_metadata(&book_file.metadata()?)
        .ok_or_else(|| io::Error::other("The book is unavailable"))?;
    let sidecar = Path::new(name).with_extension("sdr");
    let sidecar_name = sidecar
        .file_name()
        .ok_or_else(|| io::Error::other("The metadata folder is unavailable"))?;
    let sidecar_dir = open_child(&directory, sidecar_name, true)?;
    let sidecar_metadata = sidecar_dir.metadata()?;
    let mut transfer_name = name.to_os_string();
    transfer_name.push(".annotations.lua");
    let mut file = open_child(&sidecar_dir, &transfer_name, false)?;
    let metadata = file.metadata()?;
    let stamp = FileStamp::from_metadata(&metadata)
        .ok_or_else(|| io::Error::other("The transfer is unavailable"))?;
    let size = metadata.len();
    if size == 0 || size > MAX_SIDECAR_BYTES as u64 {
        return Err(io::Error::other("The transfer exceeds the discovery limit"));
    }
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(size as usize)
        .map_err(io::Error::other)?;
    let mut buffer = [0; 32 * 1024];
    let mut hash = ring::digest::Context::new(&ring::digest::SHA256);
    loop {
        check(cancel)?;
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        if bytes.len().saturating_add(n) > size as usize {
            return Err(io::Error::other("The transfer changed during discovery"));
        }
        hash.update(&buffer[..n]);
        bytes.extend_from_slice(&buffer[..n]);
    }
    if bytes.len() != size as usize
        || FileStamp::from_metadata(&file.metadata()?) != Some(stamp.clone())
        || FileStamp::from_metadata(&open_child(&sidecar_dir, &transfer_name, false)?.metadata()?)
            != Some(stamp)
        || FileStamp::from_metadata(&book_file.metadata()?) != Some(book_stamp.clone())
        || FileStamp::from_metadata(&open_child(&directory, name, false)?.metadata()?)
            != Some(book_stamp)
    {
        return Err(io::Error::other(
            "The book or transfer changed during discovery",
        ));
    }
    let sidecar_now = open_child(&directory, sidecar_name, true)?.metadata()?;
    if (sidecar_now.dev(), sidecar_now.ino()) != (sidecar_metadata.dev(), sidecar_metadata.ino()) {
        return Err(io::Error::other(
            "The metadata folder changed during discovery",
        ));
    }
    check(cancel)?;
    let digest = hash
        .finish()
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    Ok((bytes, digest))
}

#[cfg(not(unix))]
fn read_adjacent(
    _root: &Path,
    _book: &Path,
    _cancel: &AtomicBool,
) -> io::Result<(Vec<u8>, String)> {
    Err(io::Error::other(
        "Transfer discovery is unavailable on this platform",
    ))
}
