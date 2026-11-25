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
pub(crate) const INLINE_ALL_FOOTNOTE_CONTENT: &str = "inpage_footnote_all_inline";
pub(crate) const COMBINE_NONLINEAR_NOTES: &str = "inpage_footnote_combine_non_linear";
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
    layout: LayoutTweaks,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct LayoutTweaks {
    pub(crate) cjk_tailored: bool,
    pub(crate) monochrome: bool,
    pub(crate) browser_paragraphs: bool,
    pub(crate) paragraph_spacing: ParagraphSpacing,
    pub(crate) full_width_images: bool,
    pub(crate) link_ink: Option<LinkInk>,
    pub(crate) ignore_table_hints: bool,
    pub(crate) native_table_widths: bool,
    pub(crate) disable_generated_content: bool,
    pub(crate) disable_first_letter: bool,
    pub(crate) disable_first_line: bool,
    pub(crate) normal_line_height_percent: Option<u8>,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LinkInk {
    Black = 0,
    // KOReader’s Kindle framebuffer converts pure blue to (0 + 0 + 255) / 4.
    Blue = 63,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ParagraphSpacing {
    #[default]
    Unchanged,
    Full,
    Half,
    None,
}

impl LayoutTweaks {
    pub(crate) fn paragraph_rules(self) -> impl Iterator<Item = &'static str> {
        let browser = self
            .browser_paragraphs
            .then_some("p {text-align:start;text-indent:0;margin-top:1em;margin-bottom:1em;}");
        let spacing = match self.paragraph_spacing {
            ParagraphSpacing::Unchanged => None,
            ParagraphSpacing::Full => Some("p + p {margin-top:1em !important;}"),
            ParagraphSpacing::Half => Some("p + p {margin-top:.5em !important;}"),
            ParagraphSpacing::None => {
                Some("p {margin-top:0 !important;margin-bottom:0 !important;}")
            }
        };
        browser.into_iter().chain(spacing)
    }

    pub(crate) fn paragraph_edges(self, follows_paragraph: bool) -> [bool; 2] {
        if self.browser_paragraphs || self.paragraph_spacing == ParagraphSpacing::None {
            [true, true]
        } else {
            [
                follows_paragraph
                    && matches!(
                        self.paragraph_spacing,
                        ParagraphSpacing::Full | ParagraphSpacing::Half
                    ),
                false,
            ]
        }
    }
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
    combine_nonlinear: bool,
    separate_nonlinear: bool,
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
    pub(crate) inline_all: bool,
    pub(crate) regularize: bool,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            css: Arc::from(""),
            digest: md5::compute(b"").0,
            notes: InPageNotes::default(),
            layout: LayoutTweaks::default(),
        }
    }
}
impl Snapshot {
    pub(crate) fn new(css: String) -> crate::Result<Self> {
        Self::with_layout(css, InPageNotes::default(), LayoutTweaks::default())
    }
    fn with_layout(css: String, notes: InPageNotes, layout: LayoutTweaks) -> crate::Result<Self> {
        crate::book_style::validate_user_css(&css)?;
        let mut digest = md5::Context::new();
        digest.consume(css.as_bytes());
        if layout != LayoutTweaks::default() {
            digest.consume(b"\0layout-tweaks");
            digest.consume([
                u8::from(layout.cjk_tailored),
                u8::from(layout.monochrome),
                u8::from(layout.browser_paragraphs),
                layout.paragraph_spacing as u8,
            ]);
            if let Some(ink) = layout.link_ink {
                digest.consume(b"link-ink");
                digest.consume([ink as u8]);
            }
        }
        if layout.full_width_images {
            digest.consume(b"\0full-width-images");
        }
        if layout.ignore_table_hints {
            digest.consume(b"\0ignore-table-hints");
        }
        if layout.native_table_widths {
            digest.consume(b"\0native-table-widths");
        }
        if layout.disable_generated_content {
            digest.consume(b"\0disable-generated-content");
        }
        if layout.disable_first_letter {
            digest.consume(b"\0disable-first-letter");
        }
        if layout.disable_first_line {
            digest.consume(b"\0disable-first-line");
        }
        if let Some(percent) = layout.normal_line_height_percent {
            digest.consume(b"\0normal-line-height");
            digest.consume([percent]);
        }
        Ok(Self {
            css: css.into(),
            digest: digest.finalize().0,
            notes,
            layout,
        })
    }
    pub(crate) fn css(&self) -> &str {
        &self.css
    }
    pub(crate) fn is_empty(&self) -> bool {
        self.css.is_empty()
            && self.layout == LayoutTweaks::default()
            && self.notes.epub_size.is_none()
            && !self.has_in_page_fb2_notes()
    }
    pub(crate) fn layout_tweaks(&self) -> LayoutTweaks {
        self.layout
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
    pub(crate) fn without_inline_footnotes(&self) -> Self {
        let mut snapshot = self.clone();
        snapshot.notes.formatting.inline_all = false;
        snapshot
    }
    pub(crate) fn note_extension(&self) -> NoteExtension {
        self.notes.extension
    }
    pub(crate) fn note_formatting(&self) -> NoteFormatting {
        self.notes.formatting
    }
    pub(crate) fn combines_nonlinear_notes(&self) -> bool {
        self.notes.combine_nonlinear
    }
    pub(crate) fn separates_nonlinear_notes(&self) -> bool {
        self.notes.separate_nonlinear
    }
    #[cfg(feature = "kindle")]
    pub(crate) fn for_nonlinear_notes(mut self, hide: bool) -> Self {
        self.notes.separate_nonlinear = hide && self.combines_nonlinear_notes();
        self
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
    NativeTableWidth(&'static str),
    TailorCjk,
    Monochrome,
    BrowserParagraphs,
    ParagraphSpacing(ParagraphSpacing),
    LinkInk(LinkInk),
    IgnoreTableHints,
    DisableGeneratedContent,
    DisableFirstLetter,
    DisableFirstLine,
    NormalLineHeight(u8),
    InPageEpubFootnotes,
    FullWidthImages,
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
    InlineAllFootnoteContent,
    RegularizeNoteText,
    CombineNonlinearNotes,
}
#[derive(Clone, Copy)]
pub(crate) struct Preset {
    pub id: &'static str,
    pub title: &'static str,
    pub group: &'static str,
    pub priority: u8,
    pub effect: Effect,
}

macro_rules! normal_line_height_preset {
    ($id:literal, $title:literal, $percent:literal) => {
        Preset {
            id: $id,
            title: $title,
            group: "normal-line-height",
            priority: 0,
            effect: Effect::NormalLineHeight($percent),
        }
    };
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
        id: INLINE_ALL_FOOTNOTE_CONTENT,
        title: "Inline all footnote content",
        group: "in-page-note-flatten",
        priority: 0,
        effect: Effect::InlineAllFootnoteContent,
    },
    Preset {
        id: NOTE_INLINE_SIZING,
        title: "Regularize footnote text",
        group: "in-page-note-inline-sizing",
        priority: 0,
        effect: Effect::RegularizeNoteText,
    },
    Preset {
        id: COMBINE_NONLINEAR_NOTES,
        title: "Combine footnotes in a non-linear flow",
        group: "in-page-note-nonlinear",
        priority: 0,
        effect: Effect::CombineNonlinearNotes,
    },
    Preset {
        id: "paragraph_web_browser_style",
        title: "Generic web browser paragraph style",
        group: "browser-paragraphs",
        priority: 0,
        effect: Effect::BrowserParagraphs,
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
        id: "paragraph_following_no_indent",
        title: "No indentation on following paragraphs",
        group: "following-indent",
        priority: 2,
        effect: Effect::Css("p + p {text-indent:0 !important;}"),
    },
    Preset {
        id: "cjk_tailored",
        title: "Tailor widths and text-indent for CJK",
        group: "cjk-tailored",
        priority: 0,
        effect: Effect::TailorCjk,
    },
    Preset {
        id: "paragraph_whitespace",
        title: "Spacing between paragraphs",
        group: "paragraph-spacing",
        priority: 5,
        effect: Effect::ParagraphSpacing(ParagraphSpacing::Full),
    },
    Preset {
        id: "paragraph_whitespace_half",
        title: "Spacing between paragraphs (half)",
        group: "paragraph-spacing",
        priority: 5,
        effect: Effect::ParagraphSpacing(ParagraphSpacing::Half),
    },
    Preset {
        id: "paragraph_no_whitespace",
        title: "No spacing between paragraphs",
        group: "paragraph-spacing",
        priority: 5,
        effect: Effect::ParagraphSpacing(ParagraphSpacing::None),
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
        id: "text_align_all_left",
        title: "Left align all elements",
        group: "alignment",
        priority: 2,
        effect: Effect::Css("* {text-align:left !important;}"),
    },
    Preset {
        id: "text_align_all_justify",
        title: "Justify all elements",
        group: "alignment",
        priority: 0,
        effect: Effect::Css("* {text-align:justify !important;}"),
    },
    Preset {
        id: "headings-centered",
        title: "Center headings",
        group: "headings",
        priority: 4,
        effect: Effect::Css("h1,h2,h3,h4,h5,h6 {text-align:center !important;}"),
    },
    Preset {
        id: "body_direction_rtl",
        title: "Document direction RTL",
        group: "document-direction",
        priority: 2,
        effect: Effect::Css("body {direction:rtl !important;}"),
    },
    Preset {
        id: "body_direction_ltr",
        title: "Document direction LTR",
        group: "document-direction",
        priority: 0,
        effect: Effect::Css("body {direction:ltr !important;}"),
    },
    Preset {
        id: "text_align_most_right",
        title: "Right-align common text",
        group: "right-alignment",
        priority: 3,
        effect: Effect::Css("body,p,li,h1,h2,h3,h4,h5,h6 {text-align:right !important;}"),
    },
    Preset {
        id: "text_align_all_right",
        title: "Right-align all elements",
        group: "right-alignment",
        priority: 3,
        effect: Effect::Css("*,body,p,li,h1,h2,h3,h4,h5,h6 {text-align:right !important;}"),
    },
    Preset {
        id: "margin_body_0",
        title: "Ignore publisher page margins",
        group: "body-margin",
        priority: 0,
        effect: Effect::Css("body {margin:0 !important;}"),
    },
    Preset {
        id: "margin_horizontal_all_0",
        title: "Ignore all horizontal margins",
        group: "horizontal-margin",
        priority: 0,
        effect: Effect::Css("* {margin-left:0 !important;margin-right:0 !important;}"),
    },
    Preset {
        id: "paragraph_no_horizontal_margin",
        title: "Ignore horizontal paragraph margins",
        group: "horizontal-margin",
        priority: 0,
        effect: Effect::Css("p, li {margin-left:0 !important;margin-right:0 !important;}"),
    },
    Preset {
        id: "horizontal-padding-none",
        title: "Remove horizontal padding",
        group: "h-padding",
        priority: 0,
        effect: Effect::Css("* {padding-left:0 !important;padding-right:0 !important;}"),
    },
    Preset {
        id: "paragraph_no_horizontal_padding",
        title: "Ignore horizontal paragraph padding",
        group: "h-padding",
        priority: 0,
        effect: Effect::Css("p, li {padding-left:0 !important;padding-right:0 !important;}"),
    },
    Preset {
        id: "margin_vertical_all_0",
        title: "Ignore all vertical margins",
        group: "vertical-margin",
        priority: 0,
        effect: Effect::Css("* {margin-top:0 !important;margin-bottom:0 !important;}"),
    },
    Preset {
        id: "paragraph_no_vertical_margin",
        title: "Ignore vertical paragraph margins",
        group: "vertical-margin",
        priority: 0,
        effect: Effect::Css("p, li {margin-top:0 !important;margin-bottom:0 !important;}"),
    },
    Preset {
        id: "vertical-padding-none",
        title: "Remove vertical padding",
        group: "v-padding",
        priority: 0,
        effect: Effect::Css("* {padding-top:0 !important;padding-bottom:0 !important;}"),
    },
    Preset {
        id: "paragraph_no_vertical_padding",
        title: "Ignore vertical paragraph padding",
        group: "v-padding",
        priority: 0,
        effect: Effect::Css("p, li {padding-top:0 !important;padding-bottom:0 !important;}"),
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
        id: "h1_page-break-before_always",
        title: "New page on H1",
        group: "heading-break-h1",
        priority: 0,
        effect: Effect::Css("h1 {page-break-before:always !important;}"),
    },
    Preset {
        id: "h2_page-break-before_always",
        title: "New page on H2",
        group: "heading-break-h2",
        priority: 0,
        effect: Effect::Css(
            "h2 {page-break-before:always !important;} h1 + h2 {page-break-before:avoid !important;}",
        ),
    },
    Preset {
        id: "h3_page-break-before_always",
        title: "New page on H3",
        group: "heading-break-h3",
        priority: 0,
        effect: Effect::Css(
            "h3 {page-break-before:always !important;} h1 + h3,h2 + h3 {page-break-before:avoid !important;}",
        ),
    },
    Preset {
        id: "h4_page-break-before_always",
        title: "New page on H4",
        group: "heading-break-h4",
        priority: 0,
        effect: Effect::Css(
            "h4 {page-break-before:always !important;} h1 + h4,h2 + h4,h3 + h4 {page-break-before:avoid !important;}",
        ),
    },
    Preset {
        id: "h5_page-break-before_always",
        title: "New page on H5",
        group: "heading-break-h5",
        priority: 0,
        effect: Effect::Css(
            "h5 {page-break-before:always !important;} h1 + h5,h2 + h5,h3 + h5,h4 + h5 {page-break-before:avoid !important;}",
        ),
    },
    Preset {
        id: "h6_page-break-before_always",
        title: "New page on H6",
        group: "heading-break-h6",
        priority: 0,
        effect: Effect::Css(
            "h6 {page-break-before:always !important;} h1 + h6,h2 + h6,h3 + h6,h4 + h6,h5 + h6 {page-break-before:avoid !important;}",
        ),
    },
    Preset {
        id: "protect-lines",
        title: "Protect widows and orphans",
        group: "lines",
        priority: 0,
        effect: Effect::Css("* {orphans:2 !important;widows:2 !important;}"),
    },
    Preset {
        id: "widows_orphans_avoid",
        title: "Avoid widows and orphans",
        group: "lines",
        priority: 2,
        effect: Effect::Css("body {orphans:2;widows:2;}"),
    },
    Preset {
        id: "widows_avoid_orphans_allow",
        title: "Avoid widows but allow orphans",
        group: "lines",
        priority: 2,
        effect: Effect::Css("body {orphans:1;widows:2;}"),
    },
    Preset {
        id: "widows_orphans_all_inherit",
        title: "Ignore publisher orphan and widow rules",
        group: "publisher-widow-orphan-rules",
        priority: 0,
        effect: Effect::Css("* {orphans:inherit !important;widows:inherit !important;}"),
    },
    Preset {
        id: "link-black",
        title: "Links always black",
        group: "link-ink",
        priority: 0,
        effect: Effect::LinkInk(LinkInk::Black),
    },
    Preset {
        id: "link-blue",
        title: "Links always blue",
        group: "link-ink",
        priority: 0,
        effect: Effect::LinkInk(LinkInk::Blue),
    },
    Preset {
        id: "link-bold",
        title: "Links always bold",
        group: "link-weight",
        priority: 0,
        effect: Effect::Css("a, a * {font-weight:bold !important;}"),
    },
    Preset {
        id: "link-no-bold",
        title: "Links never bold",
        group: "link-weight",
        priority: 0,
        effect: Effect::Css("a, a * {font-weight:normal !important;}"),
    },
    Preset {
        id: "link-italic",
        title: "Links always italic",
        group: "link-slant",
        priority: 0,
        effect: Effect::Css("a, a * {font-style:italic !important;}"),
    },
    Preset {
        id: "link-no-italic",
        title: "Links never italic",
        group: "link-slant",
        priority: 0,
        effect: Effect::Css("a, a * {font-style:normal !important;}"),
    },
    Preset {
        id: "link-underline",
        title: "Links always underlined",
        group: "link-line",
        priority: 0,
        effect: Effect::Css("a[href], a[href] * {text-decoration:underline !important;}"),
    },
    Preset {
        id: "link-no-underline",
        title: "Links never underlined",
        group: "link-line",
        priority: 0,
        effect: Effect::Css("a, a * {text-decoration:none !important;}"),
    },
    Preset {
        id: "image-text-width",
        title: "Fit images to text width",
        group: "images",
        priority: 0,
        effect: Effect::Css("img {width:100% !important;height:auto !important;}"),
    },
    Preset {
        id: "image_full_width",
        title: "Full-width images",
        group: "images",
        priority: 0,
        effect: Effect::FullWidthImages,
    },
    Preset {
        id: "image_valign_middle",
        title: "Vertically center-align images relative to text",
        group: "image-alignment",
        priority: 0,
        effect: Effect::Css("img {vertical-align:middle;}"),
    },
    Preset {
        id: "no_pseudo_element_before_after",
        title: "Disable before/after pseudo elements",
        group: "pseudo-elements",
        priority: 0,
        effect: Effect::DisableGeneratedContent,
    },
    Preset {
        id: "no_pseudo_element_first_letter",
        title: "Disable first-letter pseudo elements",
        group: "pseudo-elements-first-letter",
        priority: 0,
        effect: Effect::DisableFirstLetter,
    },
    Preset {
        id: "no_pseudo_element_first_line",
        title: "Disable first-line pseudo elements",
        group: "pseudo-elements-first-line",
        priority: 0,
        effect: Effect::DisableFirstLine,
    },
    Preset {
        id: "table_no_presentational_hints",
        title: "Ignore HTML table presentation",
        group: "table-presentation",
        priority: 0,
        effect: Effect::IgnoreTableHints,
    },
    Preset {
        id: "td_vertical_align_none",
        title: "Ignore publisher vertical alignment in tables",
        group: "table-vertical-align",
        priority: 0,
        effect: Effect::Css("td {vertical-align:baseline !important;}"),
    },
    Preset {
        id: "table_row_odd_even",
        title: "Alternate background color of table rows",
        group: "table-row-colors",
        priority: 0,
        effect: Effect::Css(
            "tr:nth-child(odd) {background-color:#eee !important;} tr:nth-child(even) {background-color:#ccc !important;}",
        ),
    },
    Preset {
        id: "table_force_border",
        title: "Show borders on all tables",
        group: "tables",
        priority: 0,
        effect: Effect::Css(
            "table,tcaption,tr,th,td {border:black solid 1px;border-collapse:collapse;}",
        ),
    },
    Preset {
        id: "table_full_width",
        title: "Full-width tables",
        group: "table-full-width",
        priority: 2,
        effect: Effect::NativeTableWidth("table {width:100% !important;}"),
    },
    Preset {
        id: "table_td_width_auto",
        title: "Ignore publisher table and cell widths",
        group: "table-publisher-widths",
        priority: 0,
        effect: Effect::NativeTableWidth("table, td, th {width:auto !important;}"),
    },
    Preset {
        id: "table_margin_left_right_auto",
        title: "Center small tables",
        group: "table-alignment",
        priority: 3,
        effect: Effect::NativeTableWidth(
            "table {margin-left:auto !important;margin-right:auto !important;}",
        ),
    },
    Preset {
        id: "table-visible-borders",
        title: "Force table borders (legacy)",
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
        id: "hyphenate_all_auto",
        title: "Allow hyphenation on all text",
        group: "hyphenate-publisher-text",
        priority: 0,
        effect: Effect::Css(
            "* {hyphens:auto !important;} h1,h2,h3,h4,h5,h6 {hyphens:none !important;}",
        ),
    },
    Preset {
        id: "ligature_all_no_common_ligature",
        title: "Disable common ligatures",
        group: "common-ligatures",
        priority: 0,
        effect: Effect::Css("* {font-variant:no-common-ligatures;}"),
    },
    Preset {
        id: "font_size_most_reset",
        title: "Reset main text font size",
        group: "font-size",
        priority: 0,
        effect: Effect::Css("body,p,li,div,blockquote {font-size:1rem !important;}"),
    },
    Preset {
        id: "lineheight_all_inherit",
        title: "Ignore publisher line heights",
        group: "publisher-line-height",
        priority: 0,
        effect: Effect::Css("* {line-height:inherit !important;}"),
    },
    normal_line_height_preset!("normal_line-height_90", "Normal line height: 0.90", 90),
    normal_line_height_preset!("normal_line-height_95", "Normal line height: 0.95", 95),
    normal_line_height_preset!("normal_line-height_100", "Normal line height: 1.00", 100),
    normal_line_height_preset!("normal_line-height_105", "Normal line height: 1.05", 105),
    normal_line_height_preset!("normal_line-height_110", "Normal line height: 1.10", 110),
    normal_line_height_preset!("normal_line-height_115", "Normal line height: 1.15", 115),
    normal_line_height_preset!("normal_line-height_120", "Normal line height: 1.20", 120),
    normal_line_height_preset!("normal_line-height_125", "Normal line height: 1.25", 125),
    normal_line_height_preset!("normal_line-height_130", "Normal line height: 1.30", 130),
    normal_line_height_preset!("normal_line-height_135", "Normal line height: 1.35", 135),
    normal_line_height_preset!("normal_line-height_140", "Normal line height: 1.40", 140),
    normal_line_height_preset!("normal_line-height_145", "Normal line height: 1.45", 145),
    normal_line_height_preset!("normal_line-height_150", "Normal line height: 1.50", 150),
    Preset {
        id: "ruby_most_line_height_larger",
        title: "Larger spacing between ruby lines",
        group: "ruby-line-height",
        priority: 0,
        effect: Effect::Css("p,li {line-height:2 !important;}"),
    },
    Preset {
        id: "ruby_font_sans_serif",
        title: "Sans-serif font for ruby",
        group: "ruby-font-family",
        priority: 0,
        effect: Effect::Css(
            "rt {font-family:'Noto Sans CJK SC',sans-serif !important;font-weight:400 !important;}",
        ),
    },
    Preset {
        id: "ruby_font_size_larger",
        title: "Larger ruby text size",
        group: "ruby-font-size",
        priority: 0,
        effect: Effect::Css("rt {font-size:50% !important;}"),
    },
    Preset {
        id: "sub_sup_smaller",
        title: "Smaller sub- and superscript",
        group: "sub-sup-size",
        priority: 5,
        effect: Effect::Css(
            "sup {font-size:50% !important;vertical-align:super !important;} sub {font-size:50% !important;vertical-align:sub !important;}",
        ),
    },
    Preset {
        id: "ul_li_type_disc",
        title: "Force bullets with unordered lists",
        group: "list-marker-disc",
        priority: 0,
        effect: Effect::Css("ul > li {list-style-type:disc !important;}"),
    },
    Preset {
        id: "ol_li_type_decimal",
        title: "Force decimal numbers with ordered lists",
        group: "list-marker-decimal",
        priority: 0,
        effect: Effect::Css("ol > li {list-style-type:decimal !important;}"),
    },
    Preset {
        id: "pure_black_and_white",
        title: "Pure black and white",
        group: "monochrome-css",
        priority: 0,
        effect: Effect::Monochrome,
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
        if !matches!(
            target.effect,
            Effect::Css(_)
                | Effect::TailorCjk
                | Effect::Monochrome
                | Effect::BrowserParagraphs
                | Effect::FullWidthImages
                | Effect::ParagraphSpacing(_)
                | Effect::NormalLineHeight(_)
                | Effect::LinkInk(_)
                | Effect::IgnoreTableHints
        ) {
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
        let mut layout = LayoutTweaks::default();
        for preset in selected {
            match preset.effect {
                Effect::Css(snippet) => {
                    css.push_str(snippet);
                    css.push('\n');
                }
                Effect::NativeTableWidth(snippet) => {
                    layout.native_table_widths = true;
                    css.push_str(snippet);
                    css.push('\n');
                }
                Effect::TailorCjk => layout.cjk_tailored = true,
                Effect::Monochrome => layout.monochrome = true,
                Effect::BrowserParagraphs => layout.browser_paragraphs = true,
                Effect::ParagraphSpacing(spacing) => layout.paragraph_spacing = spacing,
                Effect::FullWidthImages => {
                    layout.full_width_images = true;
                    css.push_str("img {width:100% !important;}\n");
                }
                Effect::LinkInk(ink) => layout.link_ink = Some(ink),
                Effect::IgnoreTableHints => layout.ignore_table_hints = true,
                Effect::DisableGeneratedContent => layout.disable_generated_content = true,
                Effect::DisableFirstLetter => layout.disable_first_letter = true,
                Effect::DisableFirstLine => layout.disable_first_line = true,
                Effect::NormalLineHeight(percent) => {
                    layout.normal_line_height_percent = Some(percent)
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
                Effect::InlineAllFootnoteContent => notes.formatting.inline_all = true,
                Effect::RegularizeNoteText => notes.formatting.regularize = true,
                Effect::CombineNonlinearNotes => notes.combine_nonlinear = true,
            }
        }
        // CREngine's EPUB tree has a DocFragment element for the selected
        // defaults; Reader has one body per chapter. Restore those defaults on
        // the body when the independent publisher-rule override is also on.
        if self.enabled(book, "widows_orphans_all_inherit") {
            if self.enabled(book, "widows_orphans_avoid") {
                css.push_str("body {orphans:2 !important;widows:2 !important;}\n");
            } else if self.enabled(book, "widows_avoid_orphans_allow") {
                css.push_str("body {orphans:1 !important;widows:2 !important;}\n");
            } else if self.enabled(book, "protect-lines") {
                css.push_str("* {orphans:2 !important;widows:2 !important;}\n");
            }
        }
        if b.custom_enabled
            && let Some(custom) = &b.custom
        {
            css.push_str(custom);
        }
        Snapshot::with_layout(css, notes, layout)
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
