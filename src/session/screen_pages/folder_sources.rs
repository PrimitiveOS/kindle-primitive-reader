//! Bounded source proof for an extracted EPUB. This is not a layout gate until
//! the asynchronous lifecycle and storage-epoch handoff are connected.
#![allow(dead_code)]

use std::fs::{self, File, Metadata};
use std::io::{self, Read};
#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::SystemTime;

use ring::digest::{Context, SHA256};

// Keep both the inventory and the full-content pass bounded on small devices.
const MAX_ENTRIES: usize = 512;
const MAX_BYTES: u64 = 16 * 1024 * 1024;
const MAX_DEPTH: usize = 32;
const MAX_PATH: usize = 4096;
const BUFFER_SIZE: usize = 32 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Stamp {
    len: u64,
    modified: SystemTime,
    #[cfg(unix)]
    identity: (u64, u64, i64, i64),
}

impl Stamp {
    fn read(metadata: &Metadata) -> io::Result<Self> {
        Ok(Self {
            len: metadata.len(),
            modified: metadata.modified()?,
            #[cfg(unix)]
            identity: (
                metadata.dev(),
                metadata.ino(),
                metadata.ctime(),
                metadata.ctime_nsec(),
            ),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Entry {
    path: String,
    directory: bool,
    stamp: Stamp,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct FolderProof {
    root: PathBuf,
    inventory: Vec<Entry>,
    digest: [u8; 32],
}

fn check(cancel: &AtomicBool) -> io::Result<()> {
    if cancel.load(Ordering::Relaxed) {
        Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "Folder verification cancelled",
        ))
    } else {
        Ok(())
    }
}

#[cfg(unix)]
fn open_root(path: &Path) -> io::Result<File> {
    fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(path)
}

#[cfg(not(unix))]
fn open_root(_path: &Path) -> io::Result<File> {
    Err(io::Error::other(
        "Folder EPUB verification is unavailable on this platform",
    ))
}

#[cfg(unix)]
fn open_relative(root: &File, relative: &Path, directory: bool) -> io::Result<File> {
    use std::ffi::CString;
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::ffi::OsStrExt;
    let mut current = root.try_clone()?;
    let mut parts = relative.components().peekable();
    while let Some(part) = parts.next() {
        let std::path::Component::Normal(name) = part else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid folder EPUB member",
            ));
        };
        let name = CString::new(name.as_bytes()).map_err(io::Error::other)?;
        let mut flags = libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC;
        if parts.peek().is_some() || directory {
            flags |= libc::O_DIRECTORY;
        }
        // SAFETY: the parent descriptor is live and the component is NUL terminated.
        let fd = unsafe { libc::openat(current.as_raw_fd(), name.as_ptr(), flags) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: openat returned a new descriptor owned by this function.
        current = unsafe { File::from_raw_fd(fd) };
    }
    Ok(current)
}

#[cfg(not(unix))]
fn open_relative(_root: &File, _relative: &Path, _directory: bool) -> io::Result<File> {
    Err(io::Error::other(
        "Folder EPUB verification is unavailable on this platform",
    ))
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn directory_names(directory: &File, cancel: &AtomicBool) -> io::Result<Vec<String>> {
    use std::ffi::CStr;
    use std::os::fd::AsRawFd;
    // Open a fresh file description: dup would share the directory cursor
    // with the original descriptor and make subsequent inventories incomplete.
    // SAFETY: the directory descriptor is live and "." is NUL terminated.
    let fd = unsafe {
        libc::openat(
            directory.as_raw_fd(),
            b".\0".as_ptr().cast(),
            libc::O_RDONLY
                | libc::O_DIRECTORY
                | libc::O_NOFOLLOW
                | libc::O_NONBLOCK
                | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: fd is a fresh directory descriptor.
    let stream = unsafe { libc::fdopendir(fd) };
    if stream.is_null() {
        let error = io::Error::last_os_error();
        // SAFETY: fdopendir did not take ownership on failure.
        unsafe { libc::close(fd) };
        return Err(error);
    }
    struct Directory(*mut libc::DIR);
    impl Drop for Directory {
        fn drop(&mut self) {
            unsafe {
                libc::closedir(self.0);
            }
        }
    }
    let stream = Directory(stream);
    let mut names = Vec::new();
    loop {
        check(cancel)?;
        #[cfg(target_os = "linux")]
        let errno = unsafe { libc::__errno_location() };
        #[cfg(target_os = "macos")]
        let errno = unsafe { libc::__error() };
        // SAFETY: errno is thread-local and stream owns a live DIR pointer.
        unsafe {
            *errno = 0;
        }
        let entry = unsafe { libc::readdir(stream.0) };
        if entry.is_null() {
            let error = unsafe { *errno };
            if error != 0 {
                return Err(io::Error::from_raw_os_error(error));
            }
            break;
        }
        // SAFETY: readdir provides a NUL-terminated name valid until the next call.
        let name = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) }
            .to_str()
            .map_err(io::Error::other)?;
        if name == "." || name == ".." {
            continue;
        }
        if names.len() >= MAX_ENTRIES {
            return Err(io::Error::other(
                "Folder EPUB exceeds verification entry limit",
            ));
        }
        names.push(name.to_owned());
    }
    Ok(names)
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn directory_names(_directory: &File, _cancel: &AtomicBool) -> io::Result<Vec<String>> {
    Err(io::Error::other(
        "Folder EPUB verification is unavailable on this platform",
    ))
}

fn inventory(root: &File, cancel: &AtomicBool) -> io::Result<Vec<Entry>> {
    let mut pending = vec![PathBuf::new()];
    let root_metadata = root.metadata()?;
    if !root_metadata.is_dir() {
        return Err(io::Error::other("Folder EPUB root is not a directory"));
    }
    let mut entries = vec![Entry {
        path: String::new(),
        directory: true,
        stamp: Stamp::read(&root_metadata)?,
    }];
    let mut bytes = 0u64;
    while let Some(relative) = pending.pop() {
        check(cancel)?;
        let directory = open_relative(root, &relative, true)?;
        let metadata = directory.metadata()?;
        if !metadata.is_dir() {
            return Err(io::Error::other("Folder EPUB contains an unsafe directory"));
        }
        for name in directory_names(&directory, cancel)? {
            check(cancel)?;
            let child_relative = relative.join(name);
            let name = child_relative.to_str().ok_or(io::ErrorKind::InvalidData)?;
            if name.len() > MAX_PATH || child_relative.components().count() > MAX_DEPTH {
                return Err(io::Error::other(
                    "Folder EPUB member path exceeds verification limits",
                ));
            }
            let child_metadata = open_relative(root, &child_relative, false)?.metadata()?;
            let kind = child_metadata.file_type();
            if kind.is_symlink() || !(kind.is_dir() || kind.is_file()) {
                return Err(io::Error::other(
                    "Folder EPUB contains an unsupported member",
                ));
            }
            if entries.len() >= MAX_ENTRIES {
                return Err(io::Error::other(
                    "Folder EPUB exceeds verification entry limit",
                ));
            }
            if kind.is_file() {
                bytes = bytes
                    .checked_add(child_metadata.len())
                    .filter(|total| *total <= MAX_BYTES)
                    .ok_or_else(|| {
                        io::Error::other("Folder EPUB exceeds verification byte limit")
                    })?;
            } else {
                pending.push(child_relative.clone());
            }
            entries.push(Entry {
                path: name.to_owned(),
                directory: kind.is_dir(),
                stamp: Stamp::read(&child_metadata)?,
            });
        }
        if Stamp::read(&directory.metadata()?)? != Stamp::read(&metadata)? {
            return Err(io::Error::other("Folder EPUB changed during verification"));
        }
    }
    entries.sort_unstable_by(|a, b| a.path.cmp(&b.path));
    Ok(entries)
}

impl FolderProof {
    /// Caller must hold the storage lease throughout capture and layout.
    pub(super) fn capture(marker: &Path, cancel: &AtomicBool) -> io::Result<Self> {
        if !crate::epub_members::is_folder_book(marker) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Not a folder EPUB",
            ));
        }
        let root = marker
            .parent()
            .ok_or(io::ErrorKind::InvalidInput)?
            .canonicalize()?;
        let root_handle = open_root(&root)?;
        let before = inventory(&root_handle, cancel)?;
        let mut digest = Context::new(&SHA256);
        digest.update(b"PrimitiveOS folder EPUB proof v1\0");
        let mut buffer = [0u8; BUFFER_SIZE];
        for entry in &before {
            check(cancel)?;
            digest.update(&(entry.path.len() as u64).to_le_bytes());
            digest.update(entry.path.as_bytes());
            digest.update(&[u8::from(entry.directory)]);
            if entry.directory {
                continue;
            }
            digest.update(&entry.stamp.len.to_le_bytes());
            let mut file = open_relative(&root_handle, Path::new(&entry.path), false)?;
            if !file.metadata()?.is_file() || Stamp::read(&file.metadata()?)? != entry.stamp {
                return Err(io::Error::other(
                    "Folder EPUB member changed during verification",
                ));
            }
            let mut count = 0u64;
            loop {
                check(cancel)?;
                let n = file.read(&mut buffer)?;
                if n == 0 {
                    break;
                }
                count = count
                    .checked_add(n as u64)
                    .filter(|count| *count <= entry.stamp.len)
                    .ok_or_else(|| {
                        io::Error::other("Folder EPUB member changed during verification")
                    })?;
                digest.update(&buffer[..n]);
            }
            if count != entry.stamp.len || Stamp::read(&file.metadata()?)? != entry.stamp {
                return Err(io::Error::other(
                    "Folder EPUB member changed during verification",
                ));
            }
        }
        if inventory(&root_handle, cancel)? != before {
            return Err(io::Error::other("Folder EPUB changed during verification"));
        }
        Ok(Self {
            root,
            inventory: before,
            digest: digest
                .finish()
                .as_ref()
                .try_into()
                .map_err(io::Error::other)?,
        })
    }

    /// Recheck every member before consuming a proof. This must run in a
    /// cancellable worker, not as an unbounded foreground source lookup.
    pub(super) fn revalidate(&self, marker: &Path, cancel: &AtomicBool) -> io::Result<()> {
        if !crate::epub_members::is_folder_book(marker)
            || marker
                .parent()
                .ok_or(io::ErrorKind::InvalidInput)?
                .canonicalize()?
                != self.root
            || inventory(&open_root(&self.root)?, cancel)? != self.inventory
        {
            return Err(io::Error::other("Folder EPUB changed; reopen the book"));
        }
        Ok(())
    }
}
