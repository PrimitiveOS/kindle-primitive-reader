//! Bounded, independently selected user-origin reflow styles.
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

pub(crate) const FILE: &str = "style-tweaks.json";
pub(crate) const EPUB_FOOTNOTES: &str = "footnote-inpage_epub";
pub(crate) const MAX_FILE_BYTES: u64 = 4 * 1024 * 1024;
pub(crate) const MAX_BOOK_CSS: usize = 8 * 1024;
pub(crate) const MAX_CSS: usize = 64 * 1024;
pub(crate) const MAX_RULES: usize = 128;
pub(crate) const MAX_DECLARATIONS: usize = 32;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Snapshot {
    css: Arc<str>,
    pub digest: [u8; 16],
    in_page_epub_footnotes: bool,
}
impl Default for Snapshot {
    fn default() -> Self {
        Self {
            css: Arc::from(""),
            digest: md5::compute(b"").0,
            in_page_epub_footnotes: false,
        }
    }
}
impl Snapshot {
    pub(crate) fn new(css: String) -> crate::Result<Self> {
        Self::with_footnotes(css, false)
    }
    fn with_footnotes(css: String, in_page_epub_footnotes: bool) -> crate::Result<Self> {
        crate::book_style::validate_user_css(&css)?;
        let digest = md5::compute(css.as_bytes()).0;
        Ok(Self {
            css: css.into(),
            digest,
            in_page_epub_footnotes,
        })
    }
    pub(crate) fn css(&self) -> &str {
        &self.css
    }
    pub(crate) fn is_empty(&self) -> bool {
        self.css.is_empty() && !self.in_page_epub_footnotes
    }
    pub(crate) fn in_page_epub_footnotes(&self) -> bool {
        self.in_page_epub_footnotes
    }
    #[cfg(feature = "kindle")]
    pub(crate) fn for_note_layout(&self, allowed: bool) -> Self {
        Self {
            in_page_epub_footnotes: allowed && self.in_page_epub_footnotes,
            ..self.clone()
        }
    }
}
#[derive(Clone, Copy)]
pub(crate) enum Effect {
    Css(&'static str),
    InPageEpubFootnotes,
}
#[derive(Clone, Copy)]
pub(crate) struct Preset {
    pub id: &'static str,
    pub title: &'static str,
    pub group: &'static str,
    pub priority: u8,
    pub effect: Effect,
}
// Authored CSS and native layout effects for supported style tweaks.
pub(crate) const PRESETS: &[Preset] = &[
    Preset {
        id: EPUB_FOOTNOTES,
        title: "In-page EPUB footnotes",
        group: "in-page-footnotes",
        priority: 0,
        effect: Effect::InPageEpubFootnotes,
    },
    Preset {
        id: "paragraph-indent-none",
        title: "No paragraph indentation",
        group: "indent",
        priority: 0,
        effect: Effect::Css("p {text-indent:0 !important;}"),
    },
    Preset {
        id: "paragraph-indent-em",
        title: "Indent paragraphs",
        group: "indent",
        priority: 0,
        effect: Effect::Css("p {text-indent:1.2em !important;}"),
    },
    Preset {
        id: "first-paragraph-flush",
        title: "First paragraph flush",
        group: "first-indent",
        priority: 2,
        effect: Effect::Css("p:first-child {text-indent:0 !important;}"),
    },
    Preset {
        id: "paragraph-align-left",
        title: "Left-align paragraphs",
        group: "alignment",
        priority: 0,
        effect: Effect::Css("p, li {text-align:left !important;}"),
    },
    Preset {
        id: "paragraph-align-justify",
        title: "Justify paragraphs",
        group: "alignment",
        priority: 0,
        effect: Effect::Css("p, li {text-align:justify !important;}"),
    },
    Preset {
        id: "headings-centered",
        title: "Center headings",
        group: "headings",
        priority: 2,
        effect: Effect::Css("h1,h2,h3,h4,h5,h6 {text-align:center !important;}"),
    },
    Preset {
        id: "horizontal-padding-none",
        title: "Remove horizontal padding",
        group: "h-padding",
        priority: 0,
        effect: Effect::Css("* {padding-left:0 !important;padding-right:0 !important;}"),
    },
    Preset {
        id: "vertical-padding-none",
        title: "Remove vertical padding",
        group: "v-padding",
        priority: 0,
        effect: Effect::Css("* {padding-top:0 !important;padding-bottom:0 !important;}"),
    },
    Preset {
        id: "publisher-breaks-none",
        title: "Ignore authored page breaks",
        group: "breaks",
        priority: 0,
        effect: Effect::Css("* {break-before:auto !important;break-after:auto !important;}"),
    },
    Preset {
        id: "headings-new-page",
        title: "Start headings on a new page",
        group: "heading-breaks",
        priority: 2,
        effect: Effect::Css("h1,h2 {break-before:page !important;}"),
    },
    Preset {
        id: "protect-lines",
        title: "Protect widows and orphans",
        group: "lines",
        priority: 0,
        effect: Effect::Css("* {orphans:2 !important;widows:2 !important;}"),
    },
    Preset {
        id: "link-underline",
        title: "Underline links",
        group: "link-line",
        priority: 0,
        effect: Effect::Css("a[href] {text-decoration:underline !important;}"),
    },
    Preset {
        id: "link-no-underline",
        title: "Remove link underlines",
        group: "link-line",
        priority: 0,
        effect: Effect::Css("a[href] {text-decoration:none !important;}"),
    },
    Preset {
        id: "link-bold",
        title: "Bold links",
        group: "link-weight",
        priority: 0,
        effect: Effect::Css("a[href] {font-weight:bold !important;}"),
    },
    Preset {
        id: "image-text-width",
        title: "Fit images to text width",
        group: "images",
        priority: 0,
        effect: Effect::Css("img {width:100% !important;height:auto !important;}"),
    },
    Preset {
        id: "table-visible-borders",
        title: "Show table borders",
        group: "tables",
        priority: 0,
        effect: Effect::Css(
            "table,td,th {border:1px solid black !important;border-collapse:collapse !important;}",
        ),
    },
    Preset {
        id: "inherit-font-size",
        title: "Ignore publisher font sizes",
        group: "font-size",
        priority: 0,
        effect: Effect::Css("* {font-size:inherit !important;}"),
    },
    Preset {
        id: "inherit-font-family",
        title: "Inherit the reading font",
        group: "font-family",
        priority: 0,
        effect: Effect::Css(
            "* {font-family:inherit !important;} pre,code,kbd,samp {font-family:monospace !important;}",
        ),
    },
];
pub(crate) fn preset(id: &str) -> Option<Preset> {
    PRESETS.iter().copied().find(|p| p.id == id)
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct BookTweaks {
    pub enabled: bool,
    pub choices: BTreeMap<String, bool>,
    pub custom: Option<String>,
    pub custom_enabled: bool,
}
impl Default for BookTweaks {
    fn default() -> Self {
        Self {
            enabled: true,
            choices: BTreeMap::new(),
            custom: None,
            custom_enabled: false,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Data {
    version: u8,
    pub global: BTreeSet<String>,
    pub books: BTreeMap<u64, BookTweaks>,
}
impl Default for Data {
    fn default() -> Self {
        Self {
            version: 1,
            global: BTreeSet::new(),
            books: BTreeMap::new(),
        }
    }
}
impl Data {
    pub(crate) fn enabled(&self, book: u64, id: &str) -> bool {
        self.books
            .get(&book)
            .and_then(|b| b.choices.get(id))
            .copied()
            .unwrap_or_else(|| self.global.contains(id))
    }
    pub(crate) fn book(&self, book: u64) -> BookTweaks {
        self.books.get(&book).cloned().unwrap_or_default()
    }
    pub(crate) fn choose(&mut self, book: u64, id: &str, value: Option<bool>) -> crate::Result<()> {
        let target = preset(id).ok_or("This style tweak is unavailable")?;
        let enabling = value.unwrap_or_else(|| self.global.contains(id));
        let global = self.global.clone();
        let b = self.books.entry(book).or_default();
        if enabling {
            for other in PRESETS
                .iter()
                .filter(|p| p.id != id && p.group == target.group)
            {
                if global.contains(other.id) {
                    b.choices.insert(other.id.into(), false);
                } else if b.choices.get(other.id) == Some(&true) {
                    b.choices.remove(other.id);
                }
            }
        }
        if let Some(value) = value {
            b.choices.insert(id.into(), value);
        } else {
            b.choices.remove(id);
        }
        Ok(())
    }
    pub(crate) fn make_global(&mut self, book: u64, id: &str, enabled: bool) -> crate::Result<()> {
        let target = preset(id).ok_or("This style tweak is unavailable")?;
        if matches!(target.effect, Effect::InPageEpubFootnotes) {
            if enabled {
                self.global.insert(id.into());
            } else {
                self.global.remove(id);
                if let Some(tweaks) = self.books.get_mut(&book)
                    && tweaks.choices.get(id) == Some(&false)
                {
                    tweaks.choices.remove(id);
                }
            }
            return Ok(());
        }
        if enabled {
            self.global
                .retain(|old| preset(old).is_none_or(|p| p.group != target.group));
            self.global.insert(id.into());
        } else {
            self.global.remove(id);
        }
        let b = self.books.entry(book).or_default();
        b.choices.remove(id);
        if enabled {
            b.choices
                .retain(|old, _| preset(old).is_none_or(|p| p.group != target.group));
        }
        Ok(())
    }
    pub(crate) fn snapshot(&self, book: u64) -> crate::Result<Snapshot> {
        let b = self.book(book);
        if !b.enabled {
            return Ok(Snapshot::default());
        }
        let mut selected: Vec<_> = PRESETS
            .iter()
            .filter(|p| self.enabled(book, p.id))
            .collect();
        selected.sort_by_key(|p| (p.priority, p.id));
        let mut css = String::new();
        let mut in_page_epub_footnotes = false;
        for preset in selected {
            match preset.effect {
                Effect::Css(snippet) => {
                    css.push_str(snippet);
                    css.push('\n');
                }
                Effect::InPageEpubFootnotes => in_page_epub_footnotes = true,
            }
        }
        if b.custom_enabled
            && let Some(custom) = &b.custom
        {
            css.push_str(custom);
        }
        Snapshot::with_footnotes(css, in_page_epub_footnotes)
    }
    pub(crate) fn validate(&self) -> crate::Result<()> {
        let valid_id = |id: &str| {
            !id.is_empty()
                && id.len() <= 128
                && id
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.'))
        };
        if self.version != 1
            || self.books.len() > 1024
            || self.global.len() > 128
            || self.global.iter().any(|id| !valid_id(id))
        {
            return Err("The saved style configuration is unsupported or too large".into());
        }
        for (id, b) in &self.books {
            if b.choices.len() > 128
                || b.choices.keys().any(|id| !valid_id(id))
                || b.custom.as_ref().is_some_and(|s| s.len() > MAX_BOOK_CSS)
            {
                return Err("A saved book style exceeds its limits".into());
            }
            if let Some(css) = &b.custom {
                Snapshot::new(css.clone())?;
            }
            self.snapshot(*id)?;
        }
        // Validate global selection even when no book has a saved override.
        self.snapshot(0)?;
        Ok(())
    }
}
#[cfg(feature = "kindle")]
pub(crate) mod storage {
    use super::*;
    use std::{
        fs,
        io::{self, Read},
        path::PathBuf,
    };
    pub(crate) struct Store {
        pub path: PathBuf,
        pub data: Data,
        pub problem: Option<String>,
    }
    pub(crate) fn decode(bytes: &[u8]) -> io::Result<Data> {
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err(io::Error::other("Style settings exceed 4 MiB"));
        }
        let data: Data = serde_json::from_slice(bytes)?;
        data.validate().map_err(io::Error::other)?;
        Ok(data)
    }
    impl Store {
        pub fn load(path: PathBuf) -> Self {
            let result = primitive_app::with_storage(|| {
                if !fs::symlink_metadata(&path)?.file_type().is_file() {
                    return Err(io::Error::other("Style settings are not a regular file"));
                }
                let mut bytes = Vec::new();
                fs::File::open(&path)?
                    .take(MAX_FILE_BYTES + 1)
                    .read_to_end(&mut bytes)?;
                decode(&bytes)
            });
            match result {
                Ok(data) => Self {
                    path,
                    data,
                    problem: None,
                },
                Err(e) if e.kind() == io::ErrorKind::NotFound => Self {
                    path,
                    data: Data::default(),
                    problem: None,
                },
                Err(_) => Self {
                    path,
                    data: Data::default(),
                    problem: Some(
                        "Saved style tweaks are unreadable or unsupported; the file was preserved."
                            .into(),
                    ),
                },
            }
        }
        pub fn save(&mut self, data: Data) -> crate::Result<()> {
            if self.problem.is_some() {
                return Err("The saved styles file is read-only".into());
            }
            data.validate()?;
            let bytes = serde_json::to_vec(&data).map_err(|e| e.to_string())?;
            decode(&bytes).map_err(|e| e.to_string())?;
            primitive_app::save(&self.path, &bytes).map_err(|e| e.to_string())?;
            self.data = data;
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests;
