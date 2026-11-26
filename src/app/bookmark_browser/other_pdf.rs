//! Bounded, read-only proof for editing a mark in another local PDF.
use super::bulk_marks::Snapshot;
use crate::library::{DocumentFormat, fingerprint_prefix, fingerprint_update};
use crate::session::screen_pages::FileStamp;
use primitive_app::with_storage;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

const MAX_BYTES: u64 = 32 * 1024 * 1024;
const MAX_COMPONENTS: usize = 32;

#[derive(Clone)]
pub(super) struct Proof {
    root: PathBuf,
    path: PathBuf,
    stamp: FileStamp,
    digest: [u8; 32],
}

fn cancelled(flag: &AtomicBool) -> io::Result<()> {
    if flag.load(Ordering::Relaxed) {
        Err(io::Error::other("Verification was cancelled"))
    } else {
        Ok(())
    }
}

#[cfg(unix)]
fn open_file(root: &Path, path: &Path) -> io::Result<File> {
    use std::ffi::CString;
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::OpenOptionsExt;

    if DocumentFormat::for_path(path) != Some(DocumentFormat::Pdf) {
        return Err(io::Error::other("This saved mark is not in a PDF"));
    }
    let mut directory = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(root)?;
    let mut parts = path.components().peekable();
    let mut count = 0;
    while let Some(part) = parts.next() {
        let Component::Normal(name) = part else {
            return Err(io::Error::other("The PDF path is unavailable"));
        };
        count += 1;
        if count > MAX_COMPONENTS || name.as_bytes().len() > 255 {
            return Err(io::Error::other("The PDF path is too deep"));
        }
        let name = CString::new(name.as_bytes()).map_err(io::Error::other)?;
        let mut flags = libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC;
        if parts.peek().is_some() {
            flags |= libc::O_DIRECTORY;
        }
        // SAFETY: the parent descriptor is live and the component is NUL terminated.
        let fd = unsafe { libc::openat(directory.as_raw_fd(), name.as_ptr(), flags) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: openat returned a new descriptor owned by this function.
        directory = unsafe { File::from_raw_fd(fd) };
    }
    if count == 0 || !directory.metadata()?.is_file() {
        return Err(io::Error::other("The PDF is unavailable"));
    }
    Ok(directory)
}

#[cfg(not(unix))]
fn open_file(_root: &Path, _path: &Path) -> io::Result<File> {
    Err(io::Error::other(
        "PDF verification is unavailable on this platform",
    ))
}

impl Proof {
    pub(super) fn verify(
        root: PathBuf,
        path: PathBuf,
        id: u64,
        mark: &Snapshot,
        cancel: &AtomicBool,
    ) -> io::Result<Self> {
        let (bytes, stamp, fingerprint, digest) = with_storage(|| {
            cancelled(cancel)?;
            let mut file = open_file(&root, &path)?;
            let metadata = file.metadata()?;
            let size = metadata.len();
            if size == 0 || size > MAX_BYTES {
                return Err(io::Error::other(
                    "This PDF exceeds the note verification limit",
                ));
            }
            let stamp = FileStamp::from_metadata(&metadata)
                .ok_or_else(|| io::Error::other("The PDF cannot be verified"))?;
            let mut bytes = Vec::new();
            bytes
                .try_reserve_exact(size as usize)
                .map_err(io::Error::other)?;
            let mut buffer = [0; 32 * 1024];
            let mut fingerprint = fingerprint_prefix(size, DocumentFormat::Pdf.tag());
            let mut digest = ring::digest::Context::new(&ring::digest::SHA256);
            loop {
                cancelled(cancel)?;
                let n = file.read(&mut buffer)?;
                if n == 0 {
                    break;
                }
                if bytes.len().saturating_add(n) > size as usize {
                    return Err(io::Error::other("The PDF changed while verifying"));
                }
                fingerprint = fingerprint_update(fingerprint, &buffer[..n]);
                digest.update(&buffer[..n]);
                bytes.extend_from_slice(&buffer[..n]);
            }
            if bytes.len() != size as usize
                || FileStamp::from_metadata(&file.metadata()?) != Some(stamp.clone())
            {
                return Err(io::Error::other("The PDF changed while verifying"));
            }
            let digest: [u8; 32] = digest
                .finish()
                .as_ref()
                .try_into()
                .map_err(|_| io::Error::other("The PDF digest is unavailable"))?;
            Ok((bytes, stamp, fingerprint, digest))
        })?;
        cancelled(cancel)?;
        if fingerprint != id {
            return Err(io::Error::other("The PDF does not match this saved mark"));
        }
        let pdf = crate::fixed::Pdf::open(bytes).map_err(io::Error::other)?;
        let count = pdf.page_count();
        let valid_page = |page: usize| {
            page < count
                && pdf
                    .page_size(page)
                    .is_ok_and(|(w, h)| w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0)
        };
        let valid = match mark {
            Snapshot::Bookmark(_, mark) => {
                mark.position.chapter == 0
                    && mark.position.anchor.is_none()
                    && valid_page(mark.position.page)
            }
            Snapshot::Fixed(_, mark) => {
                mark.is_valid()
                    && valid_page(mark.page)
                    && mark.continuation.iter().all(|part| valid_page(part.page))
            }
            Snapshot::Reflow(..) => false,
        };
        cancelled(cancel)?;
        if !valid {
            return Err(io::Error::other("The saved PDF location is unavailable"));
        }
        let proof = Self {
            root,
            path,
            stamp,
            digest,
        };
        proof.revalidate()?;
        Ok(proof)
    }

    pub(super) fn revalidate(&self) -> io::Result<()> {
        with_storage(|| {
            let file = open_file(&self.root, &self.path)?;
            if FileStamp::from_metadata(&file.metadata()?) != Some(self.stamp.clone()) {
                return Err(io::Error::other("The PDF changed; review the note again"));
            }
            Ok(())
        })
    }

    pub(super) fn digest(&self) -> [u8; 32] {
        self.digest
    }
}
