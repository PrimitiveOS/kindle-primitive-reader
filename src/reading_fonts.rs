//! User-provided fonts in the Reader data directory.

use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use primitive_app::with_storage;

const MAX_FONT_BYTES: u64 = 12 * 1024 * 1024;
const MAX_FONT_ENTRIES: usize = 256;
const MAX_FONT_DEPTH: usize = 8;

/// Validate a stored relative name without inspecting the filesystem. This is
/// shared with document-default/profile validation for fonts not installed yet.
pub(crate) fn valid_name(name: &str) -> bool {
    let path = Path::new(name);
    !name.is_empty()
        && name.len() <= 255
        && !name.chars().any(|c| c.is_control() || c == '\\')
        && name.split('/').all(|part| !matches!(part, "" | "." | ".."))
        && path.components().count() <= MAX_FONT_DEPTH
        && path
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
        && path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| {
                ext.eq_ignore_ascii_case("ttf")
                    || ext.eq_ignore_ascii_case("otf")
                    || ext.eq_ignore_ascii_case("ttc")
            })
}

pub(crate) fn available(directory: &Path) -> io::Result<Vec<String>> {
    with_storage(|| {
        match fs::symlink_metadata(directory) {
            Ok(meta) if meta.is_dir() => {}
            Ok(_) => return Ok(Vec::new()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error),
        }
        let mut pending = vec![PathBuf::new()];
        let mut visited = 0usize;
        let mut fonts = Vec::new();
        while let Some(relative) = pending.pop() {
            if visited >= MAX_FONT_ENTRIES {
                break;
            }
            // Check every ancestor again before descending; never deliberately
            // traverse a link introduced since it was discovered.
            let mut checked = directory.to_owned();
            let mut safe = fs::symlink_metadata(&checked).is_ok_and(|m| m.is_dir());
            for component in relative.components() {
                checked.push(component.as_os_str());
                if !fs::symlink_metadata(&checked).is_ok_and(|m| m.is_dir()) {
                    safe = false;
                    break;
                }
            }
            if !safe {
                continue;
            }
            let entries = match fs::read_dir(&checked) {
                Ok(entries) => entries,
                Err(error) if relative.as_os_str().is_empty() => return Err(error),
                Err(_) => continue,
            };
            let mut entries = entries
                .take(MAX_FONT_ENTRIES - visited)
                .filter_map(Result::ok)
                .collect::<Vec<_>>();
            entries.sort_unstable_by_key(|entry| entry.file_name());
            let mut children = Vec::new();
            for entry in entries {
                visited += 1;
                let path = relative.join(entry.file_name());
                let Some(name) = path.to_str() else { continue };
                if name.len() > 255 || name.chars().any(|c| c.is_control() || c == '\\') {
                    continue;
                }
                let Ok(kind) = entry.file_type() else {
                    continue;
                };
                if kind.is_dir() && path.components().count() < MAX_FONT_DEPTH {
                    children.push(path);
                } else if kind.is_file() && valid_name(name) && font_file(directory, name).is_ok() {
                    fonts.push(name.to_owned());
                }
            }
            pending.extend(children.into_iter().rev());
        }
        fonts.sort_unstable_by(|a, b| {
            a.to_lowercase()
                .cmp(&b.to_lowercase())
                .then_with(|| a.cmp(b))
        });
        Ok(fonts)
    })
}

pub(crate) fn resolve(directory: &Path, name: &str) -> io::Result<PathBuf> {
    with_storage(|| font_file(directory, name))
}

fn font_file(directory: &Path, name: &str) -> io::Result<PathBuf> {
    if !valid_name(name) || !fs::symlink_metadata(directory)?.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Choose a TTF, OTF or TTC inside Reader's font folder",
        ));
    }
    let mut path = directory.to_owned();
    let components = Path::new(name).components().collect::<Vec<_>>();
    for (index, part) in components.iter().enumerate() {
        path.push(part.as_os_str());
        let metadata = fs::symlink_metadata(&path)?;
        if index + 1 < components.len() {
            if !metadata.is_dir() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "A font folder is unavailable",
                ));
            }
        } else if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_FONT_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Reader's font must be a regular file up to 12 MB",
            ));
        }
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT: AtomicUsize = AtomicUsize::new(0);
    struct TempDir(PathBuf);
    impl TempDir {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "reader-font-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn relative_names_reject_escape_and_allow_nested_faces() {
        for name in ["Book.ttf", "Serif/Family/Book.OTF", "one/Two.TTC"] {
            assert!(valid_name(name), "{name}");
        }
        for name in [
            "",
            "/Book.ttf",
            "../Book.ttf",
            "x/../Book.ttf",
            "x/./Book.ttf",
            "x//Book.ttf",
            "x\\Book.ttf",
            "x/",
            "Book.txt",
            "a/b/c/d/e/f/g/h/Book.ttf",
        ] {
            assert!(!valid_name(name), "{name}");
        }
    }

    #[test]
    fn discovers_and_resolves_bounded_nested_regular_files() {
        let root = TempDir::new();
        fs::create_dir_all(root.0.join("Families/Serif")).unwrap();
        fs::write(root.0.join("Root.ttf"), b"font").unwrap();
        fs::write(root.0.join("Families/Serif/Book.TTC"), b"font").unwrap();
        fs::write(root.0.join("Families/Empty.ttf"), b"").unwrap();
        assert_eq!(
            available(&root.0).unwrap(),
            vec!["Families/Serif/Book.TTC", "Root.ttf"]
        );
        assert_eq!(
            resolve(&root.0, "Families/Serif/Book.TTC").unwrap(),
            root.0.join("Families/Serif/Book.TTC")
        );
        assert!(resolve(&root.0, "Families/Empty.ttf").is_err());
        assert!(resolve(&root.0, "../Root.ttf").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_fonts_and_folders_are_not_followed() {
        use std::os::unix::fs::symlink;
        let root = TempDir::new();
        let outside = TempDir::new();
        fs::write(outside.0.join("Outside.ttf"), b"font").unwrap();
        symlink(&outside.0, root.0.join("linked-directory")).unwrap();
        symlink(outside.0.join("Outside.ttf"), root.0.join("linked.ttf")).unwrap();
        assert!(available(&root.0).unwrap().is_empty());
        assert!(resolve(&root.0, "linked-directory/Outside.ttf").is_err());
        assert!(resolve(&root.0, "linked.ttf").is_err());
    }
}
