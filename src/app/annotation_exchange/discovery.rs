//! Read-only discovery of a KOReader annotation transfer.
//!
//! Run `discover` on a cancellable worker. A positive result is only a hint:
//! the ordinary preview/import path must still verify the book edition and
//! transfer again before displaying changes or applying anything.

use crate::koreader_sidecar::{MAX_SIDECAR_BYTES, exchange};
use crate::session::screen_pages::FileStamp;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

const MAX_COMPONENTS: usize = 32;

#[cfg(unix)]
fn open_child(
    parent: &std::fs::File,
    name: &std::ffi::OsStr,
    directory: bool,
) -> io::Result<std::fs::File> {
    use std::ffi::CString;
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::ffi::OsStrExt;
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
    Ok(unsafe { std::fs::File::from_raw_fd(fd) })
}

#[cfg(unix)]
fn open_folder(
    root: &std::fs::File,
    folder: &Path,
    cancel: &AtomicBool,
) -> io::Result<std::fs::File> {
    if !folder.as_os_str().is_empty()
        && (!crate::library::safe_path(folder)
            || folder.components().count() >= crate::library::MAX_PATH_DEPTH)
    {
        return Err(io::Error::other("The transfer folder is unavailable"));
    }
    let mut directory = root.try_clone()?;
    for component in folder.components() {
        check(cancel)?;
        let Component::Normal(name) = component else {
            return Err(io::Error::other("The transfer folder is unavailable"));
        };
        if Path::new(name)
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("sdr"))
            || name.to_string_lossy().chars().any(char::is_control)
        {
            return Err(io::Error::other("The transfer folder is unavailable"));
        }
        directory = open_child(&directory, name, true)?;
    }
    Ok(directory)
}

fn check(cancel: &AtomicBool) -> io::Result<()> {
    if cancel.load(Ordering::Relaxed) {
        Err(io::Error::other("Transfer discovery was cancelled"))
    } else {
        Ok(())
    }
}

/// Metadata-bound transfer snapshot. Revalidation is advisory: a later
/// explicit import must reread and verify the transfer digest and book.
#[derive(Clone)]
pub(super) struct TransferProof {
    root: PathBuf,
    book: PathBuf,
    folder: Option<PathBuf>,
    book_dirs: Vec<(u64, u64)>,
    transfer_dirs: Vec<(u64, u64)>,
    book_stamp: FileStamp,
    transfer_stamp: FileStamp,
}

#[cfg(unix)]
struct ProofFiles {
    book: std::fs::File,
    transfer: std::fs::File,
    book_dirs: Vec<(u64, u64)>,
    transfer_dirs: Vec<(u64, u64)>,
}

#[cfg(unix)]
fn proof_files(
    root: &Path,
    book: &Path,
    folder: Option<&Path>,
    cancel: &AtomicBool,
) -> io::Result<ProofFiles> {
    use std::os::unix::fs::OpenOptionsExt;

    fn identity(file: &std::fs::File) -> io::Result<(u64, u64)> {
        use std::os::unix::fs::MetadataExt;
        let metadata = file.metadata()?;
        if !metadata.is_dir() {
            return Err(io::Error::other("The transfer directory is unavailable"));
        }
        Ok((metadata.dev(), metadata.ino()))
    }

    check(cancel)?;
    let root_file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(root)?;
    let root_identity = identity(&root_file)?;
    let mut book_dir = root_file.try_clone()?;
    let mut book_dirs = vec![root_identity];
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
        book_dir = open_child(&book_dir, name, true)?;
        book_dirs.push(identity(&book_dir)?);
    };
    let book_file = open_child(&book_dir, name, false)?;
    let mut transfer_dirs;
    let transfer_dir = if let Some(folder) = folder {
        // Reuse exactly the discovery folder restrictions before tracing the
        // same components, including the empty library-root selection.
        drop(open_folder(&root_file, folder, cancel)?);
        let mut directory = root_file.try_clone()?;
        transfer_dirs = vec![root_identity];
        for part in folder.components() {
            check(cancel)?;
            let Component::Normal(name) = part else {
                return Err(io::Error::other("The transfer folder is unavailable"));
            };
            directory = open_child(&directory, name, true)?;
            transfer_dirs.push(identity(&directory)?);
        }
        directory
    } else {
        let sidecar = Path::new(name).with_extension("sdr");
        let sidecar_name = sidecar
            .file_name()
            .ok_or_else(|| io::Error::other("The metadata folder is unavailable"))?;
        let directory = open_child(&book_dir, sidecar_name, true)?;
        transfer_dirs = book_dirs.clone();
        transfer_dirs.push(identity(&directory)?);
        directory
    };
    let mut transfer_name = name.to_os_string();
    transfer_name.push(".annotations.lua");
    let transfer = open_child(&transfer_dir, &transfer_name, false)?;
    check(cancel)?;
    Ok(ProofFiles {
        book: book_file,
        transfer,
        book_dirs,
        transfer_dirs,
    })
}

impl TransferProof {
    /// Capture bounded bytes and the exact transfer digest and book fingerprint
    /// under one storage lease. No source or transfer file is modified.
    #[cfg(unix)]
    pub(super) fn capture(
        root: &Path,
        book: &Path,
        folder: Option<&Path>,
        cancel: &AtomicBool,
    ) -> io::Result<(Self, Vec<u8>, String, String)> {
        primitive_app::with_storage(|| {
            let mut files = proof_files(root, book, folder, cancel)?;
            let book_stamp = FileStamp::from_metadata(&files.book.metadata()?)
                .ok_or_else(|| io::Error::other("The book cannot be verified"))?;
            let metadata = files.transfer.metadata()?;
            let transfer_stamp = FileStamp::from_metadata(&metadata)
                .ok_or_else(|| io::Error::other("The transfer cannot be verified"))?;
            let size = metadata.len();
            if size == 0 || size > MAX_SIDECAR_BYTES as u64 {
                return Err(io::Error::other("The transfer exceeds the preview limit"));
            }
            let mut bytes = Vec::new();
            bytes
                .try_reserve_exact(size as usize)
                .map_err(io::Error::other)?;
            let mut hash = ring::digest::Context::new(&ring::digest::SHA256);
            let mut chunk = [0u8; 32 * 1024];
            loop {
                check(cancel)?;
                let n = files.transfer.read(&mut chunk)?;
                if n == 0 {
                    break;
                }
                if bytes.len().saturating_add(n) > size as usize {
                    return Err(io::Error::other("The transfer changed while reading"));
                }
                hash.update(&chunk[..n]);
                bytes.extend_from_slice(&chunk[..n]);
            }
            check(cancel)?;
            let checksum = crate::koreader_sidecar::partial_md5(&mut files.book)?;
            check(cancel)?;
            if bytes.len() != size as usize
                || FileStamp::from_metadata(&files.book.metadata()?) != Some(book_stamp.clone())
                || FileStamp::from_metadata(&files.transfer.metadata()?)
                    != Some(transfer_stamp.clone())
            {
                return Err(io::Error::other(
                    "The book or transfer changed while reading",
                ));
            }
            let proof = Self {
                root: root.to_owned(),
                book: book.to_owned(),
                folder: folder.map(Path::to_owned),
                book_dirs: files.book_dirs,
                transfer_dirs: files.transfer_dirs,
                book_stamp,
                transfer_stamp,
            };
            proof.revalidate_locked(cancel)?;
            let digest = hash
                .finish()
                .as_ref()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect();
            Ok((proof, bytes, digest, format!("{:x}", md5::Digest(checksum))))
        })
    }

    #[cfg(not(unix))]
    pub(super) fn capture(
        _root: &Path,
        _book: &Path,
        _folder: Option<&Path>,
        _cancel: &AtomicBool,
    ) -> io::Result<(Self, Vec<u8>, String, String)> {
        Err(io::Error::other(
            "Transfer verification is unavailable on this platform",
        ))
    }

    #[cfg(unix)]
    fn revalidate_locked(&self, cancel: &AtomicBool) -> io::Result<()> {
        let files = proof_files(&self.root, &self.book, self.folder.as_deref(), cancel)?;
        if files.book_dirs != self.book_dirs
            || files.transfer_dirs != self.transfer_dirs
            || FileStamp::from_metadata(&files.book.metadata()?) != Some(self.book_stamp.clone())
            || FileStamp::from_metadata(&files.transfer.metadata()?)
                != Some(self.transfer_stamp.clone())
        {
            return Err(io::Error::other(
                "The book or transfer changed since preview",
            ));
        }
        check(cancel)
    }

    pub(super) fn revalidate(&self, cancel: &AtomicBool) -> io::Result<()> {
        #[cfg(unix)]
        {
            primitive_app::with_storage(|| self.revalidate_locked(cancel))
        }
        #[cfg(not(unix))]
        {
            let _ = cancel;
            Err(io::Error::other(
                "Transfer verification is unavailable on this platform",
            ))
        }
    }
}

/// Return the exact transfer digest for an eligible foreign-device candidate.
/// The caller owns worker lifetime, cancellation, and stale-result checks.
pub(super) fn discover(
    root: &Path,
    book: &Path,
    folder: Option<&Path>,
    fixed: bool,
    current_device: Option<&str>,
    applied_digest: Option<&str>,
    cancel: &AtomicBool,
) -> io::Result<Option<String>> {
    let (bytes, digest, fingerprint) =
        primitive_app::with_storage(|| read_transfer(root, book, folder, cancel))?;
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
    Ok((snapshot.fixed == fixed
        && current_device != Some(snapshot.device.as_str())
        && (folder.is_none()
            || snapshot.fingerprint.as_deref().is_some_and(|value| {
                fingerprint
                    .as_deref()
                    .is_some_and(|actual| value.eq_ignore_ascii_case(actual))
            })))
    .then_some(digest))
}

#[cfg(unix)]
fn read_transfer(
    root: &Path,
    book: &Path,
    folder: Option<&Path>,
    cancel: &AtomicBool,
) -> io::Result<(Vec<u8>, String, Option<String>)> {
    use std::fs::OpenOptions;
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};

    check(cancel)?;
    let root_file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(root)?;
    let mut directory = root_file.try_clone()?;
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
    let mut book_file = open_child(&directory, name, false)?;
    let book_stamp = FileStamp::from_metadata(&book_file.metadata()?)
        .ok_or_else(|| io::Error::other("The book is unavailable"))?;
    let sidecar = Path::new(name).with_extension("sdr");
    let sidecar_name = sidecar
        .file_name()
        .ok_or_else(|| io::Error::other("The metadata folder is unavailable"))?;
    let sidecar_dir = if let Some(folder) = folder {
        open_folder(&root_file, folder, cancel)?
    } else {
        open_child(&directory, sidecar_name, true)?
    };
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
    check(cancel)?;
    let fingerprint = if folder.is_some() {
        let checksum = crate::koreader_sidecar::partial_md5(&mut book_file)?;
        check(cancel)?;
        Some(format!("{:x}", md5::Digest(checksum)))
    } else {
        None
    };
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
    let sidecar_now = if let Some(folder) = folder {
        open_folder(&root_file, folder, cancel)?.metadata()?
    } else {
        open_child(&directory, sidecar_name, true)?.metadata()?
    };
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
    Ok((bytes, digest, fingerprint))
}

#[cfg(not(unix))]
fn read_transfer(
    _root: &Path,
    _book: &Path,
    _folder: Option<&Path>,
    _cancel: &AtomicBool,
) -> io::Result<(Vec<u8>, String, Option<String>)> {
    Err(io::Error::other(
        "Transfer discovery is unavailable on this platform",
    ))
}
