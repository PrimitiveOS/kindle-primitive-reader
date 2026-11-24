//! Browse and read closed snapshots relative to the physical Documents directory.
use crate::library::{Library, MAX_BROWSE_ENTRIES, MAX_PATH_DEPTH, MAX_SCAN_ENTRIES, safe_path};
use crate::reading_stats::history::sqlite_import::MAX_SOURCE_BYTES;
use std::{
    ffi::{CStr, CString},
    fs::{File, Metadata, OpenOptions},
    io::{self, Read},
    mem::MaybeUninit,
    os::{
        fd::{AsRawFd, FromRawFd, IntoRawFd},
        unix::fs::{MetadataExt, OpenOptionsExt},
    },
    path::{Path, PathBuf},
};

pub(super) struct Entry {
    pub(super) path: PathBuf,
    pub(super) folder: bool,
}

pub(super) struct Listing {
    pub(super) entries: Vec<Entry>,
    pub(super) truncated: bool,
}

pub(super) fn list(library: &Library, relative: &Path) -> io::Result<Listing> {
    let folder = open_folder(library, relative)?;
    let mut stream = DirectoryStream::open(folder.try_clone()?)?;
    let mut listing = Listing {
        entries: Vec::new(),
        truncated: false,
    };
    let allow_children = relative.components().count() + 1 < MAX_PATH_DEPTH;
    for scanned in 0..=MAX_SCAN_ENTRIES {
        let Some(name) = stream.next_name()? else {
            break;
        };
        if scanned == MAX_SCAN_ENTRIES {
            listing.truncated = true;
            break;
        }
        let Ok(visible) = name.to_str() else { continue };
        // Path components normalize a trailing dot after joining it to a folder.
        if visible.starts_with('.') || visible.chars().any(char::is_control) {
            continue;
        }
        let path = relative.join(visible);
        if !safe_path(&path) {
            continue;
        }
        let Ok(entry) = stat_at(&folder, name) else {
            continue;
        };
        let folder = match entry.st_mode & libc::S_IFMT {
            libc::S_IFDIR if !extension(&path, "sdr") => {
                if !allow_children {
                    listing.truncated = true;
                    continue;
                }
                true
            }
            libc::S_IFREG if snapshot_name(&path) => false,
            _ => continue,
        };
        if listing.entries.len() == MAX_BROWSE_ENTRIES {
            listing.truncated = true;
            break;
        }
        listing.entries.push(Entry { path, folder });
    }
    listing.entries.sort_by_cached_key(|entry| {
        (
            !entry.folder,
            entry.path.to_string_lossy().to_lowercase(),
            entry.path.clone(),
        )
    });
    Ok(listing)
}

pub(super) fn read(library: &Library, relative: &Path) -> io::Result<Vec<u8>> {
    if !safe_path(relative) || !snapshot_name(relative) {
        return Err(changed());
    }
    let parent = relative.parent().ok_or_else(changed)?;
    let name = relative
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(changed)?;
    let folder = open_folder(library, parent)?;
    let filename = CString::new(name)?;
    let sidecars = [
        CString::new(format!("{name}-wal"))?,
        CString::new(format!("{name}-journal"))?,
    ];
    check_sidecars(&folder, &sidecars)?;
    let entry = stat_at(&folder, &filename)?;
    if entry.st_mode & libc::S_IFMT != libc::S_IFREG {
        return Err(changed());
    }
    let mut source = open_at(&folder, &filename, libc::O_RDONLY | libc::O_NONBLOCK)?;
    let original = source.metadata()?;
    if !original.is_file() || !same_entry(&original, &entry) || original.len() > MAX_SOURCE_BYTES {
        return Err(io::Error::other(
            "Database is unavailable or larger than 8 MiB",
        ));
    }
    let mut bytes = Vec::new();
    (&mut source)
        .take(MAX_SOURCE_BYTES + 1)
        .read_to_end(&mut bytes)?;
    let after = source.metadata()?;
    let current_folder = open_folder(library, parent)?;
    check_sidecars(&current_folder, &sidecars)?;
    let current = stat_at(&current_folder, &filename)?;
    if !same_entry(&after, &current)
        || after.len() != original.len()
        || bytes.len() as u64 != original.len()
        || after.mtime() != original.mtime()
        || after.mtime_nsec() != original.mtime_nsec()
        || after.ctime() != original.ctime()
        || after.ctime_nsec() != original.ctime_nsec()
    {
        return Err(changed());
    }
    Ok(bytes)
}

fn check_sidecars(folder: &File, names: &[CString]) -> io::Result<()> {
    for name in names {
        match stat_at(folder, name) {
            Ok(info) if info.st_mode & libc::S_IFMT == libc::S_IFREG && info.st_size == 0 => {}
            Ok(_) => {
                return Err(io::Error::other(
                    "Use a closed KOReader database copy without pending journal changes",
                ));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

fn same_entry(open: &Metadata, visible: &libc::stat) -> bool {
    open.dev() == visible.st_dev as u64
        && open.ino() == visible.st_ino as u64
        && open.len() == visible.st_size as u64
        && visible.st_mode & libc::S_IFMT == libc::S_IFREG
}

fn snapshot_name(path: &Path) -> bool {
    ["sqlite3", "sqlite", "db"]
        .into_iter()
        .any(|suffix| extension(path, suffix))
}

fn extension(path: &Path, suffix: &str) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case(suffix))
}

fn changed() -> io::Error {
    io::Error::other("Choose a regular SQLite database in Documents")
}

fn open_folder(library: &Library, relative: &Path) -> io::Result<File> {
    // Apply the Library's depth, visible-name and sidecar directory limits.
    library.browse_folder(relative)?;
    let mut folder = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
        .open(library.documents_path())?;
    for component in relative.components() {
        let name = component.as_os_str().to_str().ok_or_else(changed)?;
        folder = open_at(
            &folder,
            &CString::new(name)?,
            libc::O_RDONLY | libc::O_DIRECTORY,
        )?;
    }
    Ok(folder)
}

fn open_at(folder: &File, name: &CStr, flags: i32) -> io::Result<File> {
    // The live folder descriptor and terminated component outlive this call.
    let fd = unsafe {
        libc::openat(
            folder.as_raw_fd(),
            name.as_ptr(),
            flags | libc::O_CLOEXEC | libc::O_NOFOLLOW,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // openat returned a new owned descriptor.
    Ok(unsafe { File::from_raw_fd(fd) })
}

fn stat_at(folder: &File, name: &CStr) -> io::Result<libc::stat> {
    let mut result = MaybeUninit::uninit();
    // fstatat initializes the output on success and never follows the leaf symlink.
    if unsafe {
        libc::fstatat(
            folder.as_raw_fd(),
            name.as_ptr(),
            result.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    } < 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { result.assume_init() })
}

struct DirectoryStream(*mut libc::DIR);

impl DirectoryStream {
    fn open(folder: File) -> io::Result<Self> {
        let fd = folder.into_raw_fd();
        // fdopendir owns the descriptor on success only.
        let stream = unsafe { libc::fdopendir(fd) };
        if stream.is_null() {
            let error = io::Error::last_os_error();
            drop(unsafe { File::from_raw_fd(fd) });
            Err(error)
        } else {
            Ok(Self(stream))
        }
    }

    fn next_name(&mut self) -> io::Result<Option<&CStr>> {
        // POSIX distinguishes end of directory from error through the thread's errno.
        #[cfg(target_os = "macos")]
        let errno = unsafe { libc::__error() };
        #[cfg(target_os = "linux")]
        let errno = unsafe { libc::__errno_location() };
        unsafe {
            *errno = 0;
            let entry = libc::readdir(self.0);
            if entry.is_null() {
                if *errno == 0 {
                    Ok(None)
                } else {
                    Err(io::Error::last_os_error())
                }
            } else {
                // Borrow the terminated entry name only until the next call.
                Ok(Some(CStr::from_ptr((*entry).d_name.as_ptr())))
            }
        }
    }
}

impl Drop for DirectoryStream {
    fn drop(&mut self) {
        unsafe { libc::closedir(self.0) };
    }
}
