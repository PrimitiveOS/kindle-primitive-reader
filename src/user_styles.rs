//! Bounded, independently selected user-origin reflow styles.
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

pub(crate) const FILE: &str = "style-tweaks.json";
pub(crate) const EPUB_FOOTNOTES: &str = "footnote-inpage_epub";
pub(crate) const CLASSIC_FOOTNOTES: &str = "footnote-inpage_classic_classnames";
pub(crate) const WIKIPEDIA_FOOTNOTES: &str = "footnote-inpage_wikipedia";
pub(crate) const FB2_FOOTNOTES: &str = "footnote-inpage_fb2";
pub(crate) const FB2_ENDNOTES: &str = "footnote-inpage_fb2_comments";
pub(crate) const FB2_REGULAR_NOTE_SIZE: &str = "fb2_footnotes_regular_font_size";
pub(crate) const EXTEND_NOTES: &str = "extend-footnote-inpage_any";
pub(crate) const EXTEND_NOTES_UNTIL_HEADING: &str = "extend-footnote-inpage_until_heading";
pub(crate) const NOTE_INDENTATION: &str = "inpage_footnote_text-indent_0";
pub(crate) const NOTE_JUSTIFICATION: &str = "inpage_footnote_text_justify";
pub(crate) const NOTE_BLOCK_SPACING: &str = "inpage_footnote_no_block_spacing";
pub(crate) const NOTE_COSMETIC_SPACING: &str = "inpage_footnote_no_inline_spacing";
pub(crate) const NOTE_INLINE_SIZING: &str = "inpage_footnote_regularize_text";
pub(crate) const MAX_FILE_BYTES: u64 = 4 * 1024 * 1024;
pub(crate) const MAX_BOOK_CSS: usize = 8 * 1024;
pub(crate) const MAX_CSS: usize = 64 * 1024;
pub(crate) const MAX_RULES: usize = 128;
pub(crate) const MAX_DECLARATIONS: usize = 32;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FootnoteSize {
    Publisher,
    #[default]
    Legacy,
    Full,
    Ninety,
    EightyFive,
    Eighty,
    SeventyFive,
    Seventy,
    SixtyFive,
}

impl FootnoteSize {
    #[cfg(feature = "kindle")]
    pub(crate) const CHOICES: [Self; 9] = [
        Self::Legacy,
        Self::Full,
        Self::Ninety,
        Self::EightyFive,
        Self::Eighty,
        Self::SeventyFive,
        Self::Seventy,
        Self::SixtyFive,
        Self::Publisher,
    ];

    #[cfg(feature = "kindle")]
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Publisher => "Publisher sizes",
            Self::Legacy => "80% (legacy)",
            Self::Full => "100%",
            Self::Ninety => "90%",
            Self::EightyFive => "85%",
            Self::Eighty => "80%",
            Self::SeventyFive => "75%",
            Self::Seventy => "70%",
            Self::SixtyFive => "65%",
        }
    }

    pub(crate) fn scale(self, root: bool, block: bool) -> Option<f32> {
        if self == Self::Legacy {
            return root.then_some(0.8);
        }
        if !block {
            return None;
        }
        Some(match self {
            Self::Full => 1.0,
            Self::Ninety => 0.9,
            Self::EightyFive => 0.85,
            Self::Eighty => 0.8,
            Self::SeventyFive => 0.75,
            Self::Seventy => 0.7,
            Self::SixtyFive => 0.65,
            Self::Publisher | Self::Legacy => return None,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Snapshot {
    css: Arc<str>,
    pub digest: [u8; 16],
    notes: InPageNotes,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct InPageNotes {
    epub_size: Option<FootnoteSize>,
    explicit_epub: bool,
    classic_epub: bool,
    wikipedia_epub: bool,
    fb2_footnotes: bool,
    fb2_endnotes: bool,
    fb2_regular_size: bool,
    extension: NoteExtension,
    formatting: NoteFormatting,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum NoteExtension {
    #[default]
    Off,
    UntilEntry,
    UntilHeading,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct NoteFormatting {
    pub(crate) no_indent: bool,
    pub(crate) justify: bool,
    pub(crate) no_block_spacing: bool,
    pub(crate) no_inline_spacing: bool,
    pub(crate) regularize: bool,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            css: Arc::from(""),
            digest: md5::compute(b"").0,
            notes: InPageNotes::default(),
        }
    }
}
impl Snapshot {
    pub(crate) fn new(css: String) -> crate::Result<Self> {
        Self::with_footnotes(css, InPageNotes::default())
    }
    fn with_footnotes(css: String, notes: InPageNotes) -> crate::Result<Self> {
        crate::book_style::validate_user_css(&css)?;
        let digest = md5::compute(css.as_bytes()).0;
        Ok(Self {
            css: css.into(),
            digest,
            notes,
        })
    }
    pub(crate) fn css(&self) -> &str {
        &self.css
    }
    pub(crate) fn is_empty(&self) -> bool {
        self.css.is_empty() && self.notes.epub_size.is_none() && !self.has_in_page_fb2_notes()
    }
    pub(crate) fn in_page_epub_footnotes(&self) -> bool {
        self.notes.epub_size.is_some()
    }
    pub(crate) fn footnote_size(&self) -> Option<FootnoteSize> {
        self.notes.epub_size
    }
    pub(crate) fn explicit_epub_notes(&self) -> bool {
        self.notes.explicit_epub
    }
    pub(crate) fn classic_epub_notes(&self) -> bool {
        self.notes.classic_epub
    }
    pub(crate) fn wikipedia_epub_notes(&self) -> bool {
        self.notes.wikipedia_epub
    }
    pub(crate) fn without_wikipedia_notes(&self) -> Self {
        let mut snapshot = self.clone();
        snapshot.notes.wikipedia_epub = false;
        if !snapshot.notes.explicit_epub && !snapshot.notes.classic_epub {
            snapshot.notes.epub_size = None;
        }
        snapshot
    }
    pub(crate) fn note_extension(&self) -> NoteExtension {
        self.notes.extension
    }
    pub(crate) fn note_formatting(&self) -> NoteFormatting {
        self.notes.formatting
    }
    pub(crate) fn has_in_page_fb2_notes(&self) -> bool {
        self.notes.fb2_footnotes || self.notes.fb2_endnotes
    }
    pub(crate) fn fb2_note_scale(&self, kind: crate::loose::Fb2NoteKind) -> Option<f32> {
        use crate::loose::Fb2NoteKind;
        let scale = match kind {
            Fb2NoteKind::Footnote if self.notes.fb2_footnotes => 0.75,
            Fb2NoteKind::Endnote if self.notes.fb2_endnotes => 0.85,
            _ => return None,
        };
        Some(if self.notes.fb2_regular_size {
            1.0
        } else {
            scale
        })
    }
    #[cfg(feature = "kindle")]
    pub(crate) fn for_note_layout(&self, allowed: bool) -> Self {
        let mut snapshot = self.clone();
        if !allowed {
            snapshot.notes = InPageNotes::default();
        }
        snapshot
    }
}
#[derive(Clone, Copy)]
pub(crate) enum Effect {
    Css(&'static str),
    InPageEpubFootnotes,
    InPageClassicFootnotes,
    InPageWikipediaFootnotes,
    InPageFb2Footnotes,
    InPageFb2Endnotes,
    Fb2RegularNoteSize,
    ExtendNote(NoteExtension),
    RemoveNoteIndentation,
    JustifyNotes,
    RemoveNoteBlockSpacing,
    RemoveNoteInlineSpacing,
    RegularizeNoteText,
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
        id: CLASSIC_FOOTNOTES,
        title: "In-page classic footnotes",
        group: "in-page-footnotes",
        priority: 0,
        effect: Effect::InPageClassicFootnotes,
    },
    Preset {
        id: WIKIPEDIA_FOOTNOTES,
        title: "In-page Wikipedia footnotes",
        group: "in-page-footnotes",
        priority: 0,
        effect: Effect::InPageWikipediaFootnotes,
    },
    Preset {
        id: FB2_FOOTNOTES,
        title: "In-page FB2 footnotes",
        group: "fb2-footnotes",
        priority: 0,
        effect: Effect::InPageFb2Footnotes,
    },
    Preset {
        id: FB2_ENDNOTES,
        title: "In-page FB2 endnotes",
        group: "fb2-endnotes",
        priority: 0,
        effect: Effect::InPageFb2Endnotes,
    },
    Preset {
        id: FB2_REGULAR_NOTE_SIZE,
        title: "Keep regular FB2 note size",
        group: "fb2-note-font-size",
        priority: 0,
        effect: Effect::Fb2RegularNoteSize,
    },
    Preset {
        id: EXTEND_NOTES,
        title: "Extend notes until next entry",
        group: "note-extension",
        priority: 0,
        effect: Effect::ExtendNote(NoteExtension::UntilEntry),
    },
    Preset {
        id: EXTEND_NOTES_UNTIL_HEADING,
        title: "Extend notes until next heading",
        group: "note-extension",
        priority: 0,
        effect: Effect::ExtendNote(NoteExtension::UntilHeading),
    },
    Preset {
        id: NOTE_INDENTATION,
        title: "No footnote indentation",
        group: "in-page-note-indentation",
        priority: 0,
        effect: Effect::RemoveNoteIndentation,
    },
    Preset {
        id: NOTE_JUSTIFICATION,
        title: "Justify footnote text",
        group: "in-page-note-alignment",
        priority: 0,
        effect: Effect::JustifyNotes,
    },
    Preset {
        id: NOTE_BLOCK_SPACING,
        title: "Remove footnote block spacing",
        group: "in-page-note-block-spacing",
        priority: 0,
        effect: Effect::RemoveNoteBlockSpacing,
    },
    Preset {
        id: NOTE_COSMETIC_SPACING,
        title: "Remove cosmetic footnote spacing",
        group: "in-page-note-cosmetic-spacing",
        priority: 0,
        effect: Effect::RemoveNoteInlineSpacing,
    },
    Preset {
        id: NOTE_INLINE_SIZING,
        title: "Regularize footnote text",
        group: "in-page-note-inline-sizing",
        priority: 0,
        effect: Effect::RegularizeNoteText,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub footnote_size: Option<FootnoteSize>,
}
impl Default for BookTweaks {
    fn default() -> Self {
        Self {
            enabled: true,
            choices: BTreeMap::new(),
            custom: None,
            custom_enabled: false,
            footnote_size: None,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Data {
    version: u8,
    pub global: BTreeSet<String>,
    pub books: BTreeMap<u64, BookTweaks>,
    #[serde(default, skip_serializing_if = "FootnoteSize::is_default")]
    pub footnote_size: FootnoteSize,
}

impl FootnoteSize {
    fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

impl Default for Data {
    fn default() -> Self {
        Self {
            version: 1,
            global: BTreeSet::new(),
            books: BTreeMap::new(),
            footnote_size: FootnoteSize::default(),
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
        if !matches!(target.effect, Effect::Css(_)) {
            if enabled {
                if matches!(target.effect, Effect::ExtendNote(_)) {
                    self.global
                        .retain(|old| preset(old).is_none_or(|p| p.group != target.group));
                }
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
        let mut notes = InPageNotes::default();
        for preset in selected {
            match preset.effect {
                Effect::Css(snippet) => {
                    css.push_str(snippet);
                    css.push('\n');
                }
                Effect::InPageEpubFootnotes => {
                    notes.epub_size = Some(b.footnote_size.unwrap_or(self.footnote_size));
                    notes.explicit_epub = true;
                }
                Effect::InPageClassicFootnotes => {
                    notes.epub_size = Some(b.footnote_size.unwrap_or(self.footnote_size));
                    notes.classic_epub = true;
                }
                Effect::InPageWikipediaFootnotes => {
                    notes.epub_size = Some(b.footnote_size.unwrap_or(self.footnote_size));
                    notes.wikipedia_epub = true;
                }
                Effect::InPageFb2Footnotes => notes.fb2_footnotes = true,
                Effect::InPageFb2Endnotes => notes.fb2_endnotes = true,
                Effect::Fb2RegularNoteSize => notes.fb2_regular_size = true,
                Effect::ExtendNote(extension) => notes.extension = extension,
                Effect::RemoveNoteIndentation => notes.formatting.no_indent = true,
                Effect::JustifyNotes => notes.formatting.justify = true,
                Effect::RemoveNoteBlockSpacing => notes.formatting.no_block_spacing = true,
                Effect::RemoveNoteInlineSpacing => notes.formatting.no_inline_spacing = true,
                Effect::RegularizeNoteText => notes.formatting.regularize = true,
            }
        }
        if b.custom_enabled
            && let Some(custom) = &b.custom
        {
            css.push_str(custom);
        }
        Snapshot::with_footnotes(css, notes)
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
