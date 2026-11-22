//! Reader-owned staging files and no-overwrite publication in the selected Documents folder.
use super::model::{Link, checked_url};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};

pub(crate) const MAX_DOWNLOAD: u64 = 64 * 1024 * 1024;
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub(crate) struct Catalog {
    pub name: String,
    pub url: String,
    pub username: String,
    pub password: String,
}
impl Catalog {
    pub fn validate(&self) -> Result<(), String> {
        checked_url(&self.url)?;
        if self.name.trim().is_empty()
            || self.name.chars().count() > 100
            || self.name.chars().any(char::is_control)
        {
            return Err("Enter a catalog name of 1 to 100 characters".into());
        }
        if self.username.len() > 1024 || self.password.len() > 4096 || self.username.contains(':') {
            return Err("These catalog credentials are invalid".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub(crate) struct Settings {
    pub catalogs: Vec<Catalog>,
    pub folder: PathBuf,
    pub last_download: Option<PathBuf>,
    pub pending: Option<PathBuf>,
}
impl Settings {
    pub fn load(path: &Path) -> Self {
        let mut settings: Self = fs::read(path)
            .ok()
            .filter(|b| b.len() <= 256 * 1024)
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        settings.catalogs.retain(|c| c.validate().is_ok());
        settings.catalogs.truncate(32);
        if !safe_relative(&settings.folder) {
            settings.folder = PathBuf::new();
        }
        if settings
            .last_download
            .as_ref()
            .is_some_and(|p| !safe_relative(p))
        {
            settings.last_download = None;
        }
        if settings.pending.as_ref().is_some_and(|p| !owned_temp(p)) {
            settings.pending = None;
        }
        settings
    }
    pub fn save(&self, path: &Path) -> io::Result<()> {
        primitive_app::save(path, &serde_json::to_vec(self).map_err(io::Error::other)?)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    }
}
pub(crate) fn safe_relative(path: &Path) -> bool {
    path.components().all(|c| matches!(c, Component::Normal(_)))
}
pub(crate) fn folder(root: &Path, relative: &Path) -> io::Result<PathBuf> {
    if !root.is_absolute()
        || !safe_relative(relative)
        || relative.components().count() >= crate::library::MAX_PATH_DEPTH
        || relative.components().any(|component| {
            component.as_os_str().to_str().is_none_or(|name| {
                name.starts_with('.')
                    || name.to_ascii_lowercase().ends_with(".sdr")
                    || name.chars().any(char::is_control)
            })
        })
    {
        return Err(io::Error::other("Choose a folder inside Documents"));
    }
    let canonical = fs::canonicalize(root)?;
    if canonical != root || !fs::symlink_metadata(root)?.is_dir() {
        return Err(io::Error::other("The Documents folder changed"));
    }
    let mut current = canonical;
    for component in relative.components() {
        current.push(component.as_os_str());
        let metadata = fs::symlink_metadata(&current)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(io::Error::other("Choose a regular folder inside Documents"));
        }
    }
    Ok(current)
}
pub(crate) fn owned_temp(path: &Path) -> bool {
    safe_relative(path)
        && path
            .file_name()
            .and_then(|s| s.to_str())
            .is_some_and(|name| name.starts_with(".primitiveos-opds-") && name.contains(".part."))
}
pub(crate) fn temporary(root: &Path, relative: &Path, extension: &str) -> io::Result<PathBuf> {
    folder(root, relative)?;
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(io::Error::other)?
        .as_nanos();
    Ok(relative.join(format!(
        ".primitiveos-opds-{}-{stamp}-{}.part.{extension}",
        std::process::id(),
        COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )))
}
pub(crate) fn discard(root: &Path, relative: &Path) {
    if owned_temp(relative)
        && let Some(parent) = relative.parent()
        && let Ok(directory) = folder(root, parent)
    {
        let _ = fs::remove_file(directory.join(relative.file_name().unwrap()));
    }
}
pub(super) fn create_temp(root: &Path, relative: &Path) -> io::Result<fs::File> {
    if !owned_temp(relative) {
        return Err(io::Error::other("The download staging path is invalid"));
    }
    let directory = folder(root, relative.parent().unwrap_or_else(|| Path::new("")))?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    options.open(directory.join(relative.file_name().unwrap()))
}
pub(crate) fn validate_document(path: &Path, extension: &str) -> Result<(), String> {
    let result: crate::Result<()> = match extension {
        "pdf" => match crate::fixed::Pdf::open(
            fs::read(path).map_err(|_| "Could not read the downloaded PDF")?,
        ) {
            Ok(_) | Err(crate::fixed::PdfError::PasswordProtected) => Ok(()),
            Err(error) => Err(error.into()),
        },
        "cbz" | "cbt" | "cbr" => crate::comic::Comic::open(path).map(|_| ()),
        "djvu" => fs::read(path).map_err(|e| e.into()).and_then(|bytes| {
            crate::djvu::Djvu::open(bytes)
                .map(|_| ())
                .map_err(|e| e.into())
        }),
        "jpg" | "png" | "webp" | "gif" | "tiff" | "svg" | "bmp" | "pnm" | "jp2" => {
            crate::comic::ImagePage::open(path).map(|_| ())
        }
        _ => crate::Book::open(path).map(|_| ()),
    };
    result.map_err(|_| "The download is not a readable publication in the selected format".into())
}
pub(crate) fn finalize(
    root: &Path,
    temporary: &Path,
    title: &str,
    link: &Link,
) -> io::Result<PathBuf> {
    if !owned_temp(temporary) {
        return Err(io::Error::other(
            "The completed download has an invalid staging path",
        ));
    }
    let extension = link
        .extension()
        .ok_or_else(|| io::Error::other("This acquisition needs another application or service"))?;
    let parent = temporary.parent().unwrap_or_else(|| Path::new(""));
    let directory = folder(root, parent)?;
    let source = directory.join(temporary.file_name().unwrap());
    let metadata = fs::symlink_metadata(&source)?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() == 0
        || metadata.len() > MAX_DOWNLOAD
    {
        return Err(io::Error::other(
            "The staged download is unavailable or too large",
        ));
    }
    let mut name: String = title
        .chars()
        .take(80)
        .map(|c| {
            if c.is_control() || "/\\:*?\"<>|".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    // FAT and common Unix filesystems cap names in bytes, not Unicode scalars.
    // Leave room for the extension and collision suffix without splitting UTF-8.
    while name.len() > 180 {
        name.pop();
    }
    let name = name.trim().trim_matches('.').trim();
    let name = if name.is_empty() { "Book" } else { name };
    for n in 1..10_000 {
        let filename = if n == 1 {
            format!("{name}.{extension}")
        } else {
            format!("{name} ({n}).{extension}")
        };
        let target = directory.join(&filename);
        let reserved = match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
        {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        };
        // Reserve our own empty final name, then publish the fully validated file
        // with one same-directory rename. Existing books and symlinks are skipped.
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let own = reserved.metadata()?;
            let now = fs::symlink_metadata(&target)?;
            if own.ino() != now.ino() || own.dev() != now.dev() || !now.is_file() {
                return Err(io::Error::other(
                    "The destination changed while completing the download",
                ));
            }
        }
        if folder(root, parent)? != directory {
            return Err(io::Error::other("The download folder changed"));
        }
        if let Err(error) = fs::rename(&source, &target) {
            let _ = fs::remove_file(&target);
            return Err(error);
        }
        let _ = reserved.sync_all();
        return Ok(parent.join(filename));
    }
    Err(io::Error::other(
        "This folder has too many publications with that name",
    ))
}

pub(super) fn copy_response(
    mut input: impl Read,
    mut output: impl Write,
    length: Option<u64>,
) -> Result<u64, String> {
    if length.is_some_and(|n| n == 0 || n > MAX_DOWNLOAD) {
        return Err("The publication exceeds the 64 MiB download limit or is empty".into());
    }
    let mut count = 0u64;
    let mut buffer = [0u8; 32 * 1024];
    loop {
        let n = input
            .read(&mut buffer)
            .map_err(|_| "The download ended before the complete publication arrived")?;
        if n == 0 {
            break;
        }
        count += n as u64;
        if count > MAX_DOWNLOAD {
            return Err("The publication exceeds the 64 MiB download limit".into());
        }
        output
            .write_all(&buffer[..n])
            .map_err(|_| "Could not save this download; check available storage")?;
    }
    if count == 0 || length.is_some_and(|length| length != count) {
        return Err("The download is empty or incomplete".into());
    }
    output
        .flush()
        .map_err(|_| "Could not finish saving this publication")?;
    Ok(count)
}
