//! Folder variables used by opening-profile path conditions.
use crate::library::Library;
use icu_casemap::CaseMapper;
use std::borrow::Cow;
use std::path::{Path, PathBuf};

const MAX_EXPANDED_BYTES: usize = 64 * 1024;

pub(crate) struct Folders {
    home: PathBuf,
    download: PathBuf,
    screenshots: Option<PathBuf>,
    wikipedia: PathBuf,
}

impl Folders {
    pub(crate) fn new(library: &Library, download: &Path) -> Self {
        let home = library.documents_path().to_owned();
        Self {
            download: home.join(download),
            screenshots: library
                .state_path()
                .parent()
                .map(|path| path.join("screenshots")),
            wikipedia: home.join("Wikipedia"),
            home,
        }
    }

    fn value(&self, code: u8) -> Option<&str> {
        match code {
            b'H' => &self.home,
            b'D' => &self.download,
            b'S' => self.screenshots.as_deref()?,
            b'W' => &self.wikipedia,
            _ => return None,
        }
        .to_str()
    }

    pub(crate) fn expand<'a>(&self, pattern: &'a str) -> Option<Cow<'a, str>> {
        if !pattern.contains('%') {
            return (pattern.len() <= MAX_EXPANDED_BYTES).then_some(pattern.into());
        }
        let mut expanded = String::with_capacity(pattern.len());
        let mut tail = pattern;
        while let Some(start) = tail.find('%') {
            expanded.push_str(&tail[..start]);
            tail = &tail[start..];
            let width = match tail.as_bytes() {
                [b'%', code, b'%', ..] if code.is_ascii_alphabetic() => {
                    expanded.push_str(self.value(*code).unwrap_or(&tail[..3]));
                    3
                }
                _ => {
                    expanded.push('%');
                    1
                }
            };
            tail = &tail[width..];
            if expanded.len() > MAX_EXPANDED_BYTES {
                return None;
            }
        }
        expanded.push_str(tail);
        (expanded.len() <= MAX_EXPANDED_BYTES).then_some(expanded.into())
    }
}

pub(crate) fn contains_shortcut(pattern: &str) -> bool {
    ["%H%", "%D%", "%S%", "%W%"]
        .iter()
        .any(|token| pattern.contains(token))
}

pub(super) fn matches(pattern: &str, path: &Path, library: &Library, download: &Path) -> bool {
    let canonical_root = library.canonical_documents_path();
    let canonical = if path.is_absolute() {
        Cow::Borrowed(path)
    } else {
        Cow::Owned(canonical_root.join(path))
    };
    let original = canonical
        .strip_prefix(canonical_root)
        .ok()
        .filter(|_| canonical_root != library.documents_path())
        .map(|relative| library.documents_path().join(relative));
    let expanded = if pattern.contains('%') {
        let folders = Folders::new(library, download);
        let Some(expanded) = folders.expand(pattern) else {
            return false;
        };
        expanded
    } else {
        Cow::Borrowed(pattern)
    };
    let lowercase = |text: &str| -> String {
        let mapper = CaseMapper::new();
        text.chars().map(|c| mapper.simple_lowercase(c)).collect()
    };
    let canonical = lowercase(&canonical.to_string_lossy());
    let original = original
        .as_ref()
        .map(|path| lowercase(&path.to_string_lossy()));
    expanded
        .split(',')
        .map(str::trim)
        .filter(|phrase| !phrase.is_empty())
        .any(|phrase| {
            let phrase = lowercase(phrase);
            canonical.contains(&phrase)
                || original.as_ref().is_some_and(|path| path.contains(&phrase))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reading_profiles::opening::{PATH_LIMIT, valid_path};

    #[test]
    fn expands_exact_stock_tokens_once_and_preserves_literals() {
        let folders = Folders {
            home: "/documents".into(),
            download: "/documents/Almanacs, older %W%".into(),
            screenshots: None,
            wikipedia: "/documents/Wikipedia".into(),
        };
        for (pattern, want) in [
            ("%H%%D%", "/documents/documents/Almanacs, older %W%"),
            ("%S%, %W%, %h%, %1", "%S%, /documents/Wikipedia, %h%, %1"),
            (
                "\"%D%/Book\", %Q%",
                "\"/documents/Almanacs, older %W%/Book\", %Q%",
            ),
            ("%%H%%", "%/documents%"),
            ("%Q%H%", "%Q%H%"),
            ("%é%H%", "%é/documents"),
        ] {
            assert_eq!(folders.expand(pattern).as_deref(), Some(want), "{pattern}");
        }
    }

    #[test]
    fn keeps_source_limit_and_discards_expansion_beyond_the_output_limit() {
        let folders = Folders {
            home: format!("/{}", "a".repeat(2 * 1024)).into(),
            download: "/documents".into(),
            screenshots: None,
            wikipedia: "/documents/Wikipedia".into(),
        };
        let pattern = "%H%".repeat(PATH_LIMIT / 3);
        assert!(valid_path(&pattern));
        assert!(folders.expand(&pattern).is_none());
        assert!(!valid_path(&(pattern + "ab")));
        assert!(!valid_path("%H%, , %D%"));
        assert!(folders.expand("%H%").is_some());
    }
}
