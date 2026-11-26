//! Bounded immutable ZIP snapshot for verifying another book's saved mark.
use crate::library::{DocumentFormat, fingerprint};
use crate::session::screen_pages::FileStamp;
use primitive_app::with_storage;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

const MAX_BYTES: u64 = 16 * 1024 * 1024;
const MAX_COMPONENTS: usize = 32;

#[derive(Clone)]
pub(in crate::app) struct Proof {
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

    if DocumentFormat::for_path(path) != Some(DocumentFormat::Epub) {
        return Err(io::Error::other("This saved mark is not in an EPUB"));
    }
    let mut directory = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(root)?;
    let mut parts = path.components().peekable();
    let mut count = 0;
    while let Some(part) = parts.next() {
        let Component::Normal(name) = part else {
            return Err(io::Error::other("The EPUB path is unavailable"));
        };
        count += 1;
        if count > MAX_COMPONENTS || name.as_bytes().len() > 255 {
            return Err(io::Error::other("The EPUB path is too deep"));
        }
        let name = CString::new(name.as_bytes()).map_err(io::Error::other)?;
        let mut flags = libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC;
        if parts.peek().is_some() {
            flags |= libc::O_DIRECTORY;
        }
        // SAFETY: the directory descriptor is live and the name is NUL terminated.
        let fd = unsafe { libc::openat(directory.as_raw_fd(), name.as_ptr(), flags) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: openat returned a new descriptor owned by this function.
        directory = unsafe { File::from_raw_fd(fd) };
    }
    if count == 0 || !directory.metadata()?.is_file() {
        return Err(io::Error::other("The EPUB is unavailable"));
    }
    Ok(directory)
}

#[cfg(not(unix))]
fn open_file(_root: &Path, _path: &Path) -> io::Result<File> {
    Err(io::Error::other(
        "EPUB verification is unavailable on this platform",
    ))
}

struct Cancellable<'a, R> {
    reader: R,
    cancel: &'a AtomicBool,
}

impl<R: Read> Read for Cancellable<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        cancelled(self.cancel)?;
        self.reader.read(buf)
    }
}

impl Proof {
    pub(in crate::app) fn capture(
        root: PathBuf,
        path: PathBuf,
        id: u64,
        cancel: &AtomicBool,
    ) -> io::Result<(Self, Arc<[u8]>)> {
        let (bytes, stamp, digest) = with_storage(|| {
            cancelled(cancel)?;
            let mut file = open_file(&root, &path)?;
            let metadata = file.metadata()?;
            let size = metadata.len();
            if size == 0 || size > MAX_BYTES {
                return Err(io::Error::other(
                    "This EPUB exceeds the note verification limit",
                ));
            }
            let stamp = FileStamp::from_metadata(&metadata)
                .ok_or_else(|| io::Error::other("The EPUB cannot be verified"))?;
            let mut bytes = Vec::new();
            bytes
                .try_reserve_exact(size as usize)
                .map_err(io::Error::other)?;
            let mut buffer = [0; 32 * 1024];
            let mut digest = ring::digest::Context::new(&ring::digest::SHA256);
            loop {
                cancelled(cancel)?;
                let n = file.read(&mut buffer)?;
                if n == 0 {
                    break;
                }
                if bytes.len().saturating_add(n) > size as usize {
                    return Err(io::Error::other("The EPUB changed while verifying"));
                }
                digest.update(&buffer[..n]);
                bytes.extend_from_slice(&buffer[..n]);
            }
            if bytes.len() != size as usize
                || FileStamp::from_metadata(&file.metadata()?) != Some(stamp.clone())
            {
                return Err(io::Error::other("The EPUB changed while verifying"));
            }
            let digest: [u8; 32] = digest
                .finish()
                .as_ref()
                .try_into()
                .map_err(|_| io::Error::other("The EPUB digest is unavailable"))?;
            Ok((Arc::<[u8]>::from(bytes), stamp, digest))
        })?;
        cancelled(cancel)?;
        let archive =
            crate::epub_zip::open_snapshot(Arc::clone(&bytes)).map_err(io::Error::other)?;
        let (input, offset) = archive.zip.into_inner().original_directory()?;
        let length = (bytes.len() as u64)
            .checked_sub(offset)
            .ok_or_else(|| io::Error::other("The EPUB directory is invalid"))?;
        let edition = fingerprint(
            Cancellable {
                reader: input,
                cancel,
            },
            bytes.len() as u64,
            DocumentFormat::Epub.tag(),
            length,
        )?;
        if edition != id {
            return Err(io::Error::other("The EPUB does not match this saved mark"));
        }
        cancelled(cancel)?;
        let proof = Self {
            root,
            path,
            stamp,
            digest,
        };
        proof.revalidate()?;
        cancelled(cancel)?;
        Ok((proof, bytes))
    }

    pub(in crate::app) fn revalidate(&self) -> io::Result<()> {
        with_storage(|| {
            let file = open_file(&self.root, &self.path)?;
            if FileStamp::from_metadata(&file.metadata()?) != Some(self.stamp.clone()) {
                return Err(io::Error::other("The EPUB changed; review the note again"));
            }
            Ok(())
        })
    }

    pub(in crate::app) fn digest(&self) -> [u8; 32] {
        self.digest
    }
}
