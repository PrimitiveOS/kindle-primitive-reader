pub mod word_spacing;
pub use word_spacing::WordSpacePercent;
#[cfg(feature = "kindle")]
mod annotation_export;
#[cfg(feature = "kindle")]
pub mod app;
mod blitz;
mod block_margins;
mod book_menus;
mod book_style;
mod cbr;
mod chm;
pub mod comic;
pub mod cre_pointer;
pub mod dictionary;
pub mod djvu;
#[cfg(feature = "kindle")]
pub mod document_check;
#[cfg(feature = "kindle")]
mod document_defaults;
mod docx;
#[cfg(feature = "kindle")]
mod embedded_font_usage;
pub mod epub;
pub mod epub_cfi;
mod epub_font;
mod epub_members;
mod epub_xml;
mod epub_zip;
pub mod fixed;
pub mod fixed_epub;
#[cfg(feature = "kindle")]
mod fixed_session;
pub mod fixed_text;
mod float_layout;
mod flow_probe;
mod flow_projection;
mod flow_view;
mod font_render;
mod font_synthesis;
#[cfg(feature = "kindle")]
mod generated_contents;
mod gray_glyph;
pub mod logical_font;
mod pdf_crop;
pub use flow_probe::SourcePresentation;
#[cfg(feature = "kindle")]
mod footnotes;
mod gif_image;
pub mod highlight_style;
mod hyphenation;
mod jpeg2000;
mod kf8;
mod kindle_sections;
pub mod koreader_import;
pub mod koreader_sidecar;
pub mod library;
mod list_marker;
mod loose;
mod mark_time;
pub mod mobi;
#[cfg(feature = "kindle")]
mod note_marker;
pub mod ocr;
#[cfg(feature = "kindle")]
mod ocr_job;
#[cfg(feature = "kindle")]
pub mod online_lookup;
mod parley_layout;
#[cfg(feature = "kindle")]
mod pdf_annotations;
mod publication_metadata;
mod publisher_breaks;
mod publisher_padding;
#[cfg(feature = "kindle")]
mod reading_backup;
#[cfg(feature = "kindle")]
mod reading_controls;
#[cfg(feature = "kindle")]
mod reading_fonts;
#[cfg(feature = "kindle")]
mod reading_pace;
#[cfg(feature = "kindle")]
mod reading_profiles;
#[cfg(feature = "kindle")]
mod reading_stats;
mod rtf;
#[cfg(feature = "kindle")]
mod scan_columns;
#[cfg(feature = "kindle")]
mod scan_deskew;
#[cfg(feature = "kindle")]
mod scan_reflow;
pub mod search;
pub mod selection;
#[cfg(feature = "kindle")]
mod session;
mod source;
mod svg_image;
#[cfg(feature = "kindle")]
mod synthetic_pages;
mod table_borders;
mod table_layout;
mod table_widths;
mod tcr;
#[cfg(feature = "kindle")]
mod text_flow;
mod tiff_image;
mod user_hyphenation;
mod user_styles;
mod vertical_layout;
mod vertical_padding;

use std::error::Error;
use std::fs;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use parley::{FontContext, GenericFamily};
use serde::{Deserialize, Serialize};

use crate::epub::{Book, Chapter, Destination, TocEntry};
use crate::source::Source;

pub type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

#[cfg(test)]
fn test_fixture(name: &str) -> PathBuf {
    std::env::var_os("READER_TEST_FIXTURES")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures"))
        .join(name)
}

#[cfg(test)]
fn test_symbol_font() -> PathBuf {
    std::env::var_os("READER_TEST_FIXTURES").map_or_else(
        || Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/fonts/PrimitiveSymbols.ttf"),
        |root| PathBuf::from(root).join("fonts/PrimitiveSymbols.ttf"),
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Engine {
    Blitz,
    Parley,
}

impl FromStr for Engine {
    type Err = &'static str;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        match value {
            "blitz" => Ok(Self::Blitz),
            "parley" => Ok(Self::Parley),
            _ => Err("Choose either blitz or parley"),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TextAlignment {
    #[default]
    Publisher,
    Start,
    Center,
    Justify,
    /// The preview has its own defaults, with start alignment for RTL text.
    #[doc(hidden)]
    #[serde(skip)]
    FootnoteJustify,
}

/// Grid fitting applied to the native grayscale glyph renderer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FontHinting {
    #[default]
    Off,
    Native,
    Auto,
}

/// Text shaping mode used for glyph kerning and related OpenType features.
///
/// `Best` keeps the full shaping behavior. `Good` enables the lightweight
/// kerning, mark positioning, cursive positioning, and localized-form feature
/// set used by KOReader's HarfBuzz-light mode. `Fast` limits shaping to basic
/// pair kerning and disables substitutions. The Rust shaper cannot use
/// FreeType's separate fast kerning path, so this mode keeps the same reading
/// contract with a reduced Rust shaping feature set.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FontKerningMode {
    Off,
    Fast,
    Good,
    #[default]
    Best,
}

/// Pinned KOReader content-scale presets. Zero disables physical CSS units.
pub const RENDER_DPI_PRESETS: [u16; 6] = [0, 48, 96, 167, 212, 300];

/// Pinned content-DPI More Options: integer 0..=300, separate from screen DPI.
pub const fn valid_render_dpi(value: u16) -> bool {
    value <= 300
}

/// KOReader's visible line-spacing presets in the Text/Layout controls.
pub const LINE_SPACING_PRESETS: [u16; 13] =
    [70, 75, 80, 85, 90, 95, 100, 105, 110, 115, 120, 125, 130];
pub const DEFAULT_LINE_SPACING_PERCENT: u16 = 100;

/// KOReader's custom interline percentage range.
pub const fn valid_line_spacing_percent(value: u16) -> bool {
    value >= 50 && value <= 200
}

/// Reader's User-Agent stylesheet policy for reflowable documents.
///
/// `Auto` follows KOReader's format default; the remaining choices mirror its
/// base stylesheet selector while staying within the CSS subset this renderer
/// can faithfully apply.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BaseStylesheet {
    #[default]
    Auto,
    None,
    Traditional,
    HtmlStandard,
    FictionBook,
}

impl BaseStylesheet {
    pub const ALL: [Self; 5] = [
        Self::Auto,
        Self::None,
        Self::Traditional,
        Self::HtmlStandard,
        Self::FictionBook,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::None => "None",
            Self::Traditional => "Traditional",
            Self::HtmlStandard => "HTML",
            Self::FictionBook => "FB2",
        }
    }
}

/// KOReader's visual cue for the portion repeated between scroll-page turns.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PageOverlapStyle {
    None,
    #[default]
    Dim,
    Arrow,
    Line,
    DashedLine,
}

impl PageOverlapStyle {
    pub const ALL: [Self; 5] = [
        Self::None,
        Self::Dim,
        Self::Arrow,
        Self::Line,
        Self::DashedLine,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::None => "No indicator",
            Self::Dim => "Gray out",
            Self::Arrow => "Arrow",
            Self::Line => "Solid line",
            Self::DashedLine => "Dashed line",
        }
    }
}

/// Experimental publisher block spacing for the horizontal Parley provider.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BlockMarginMode {
    #[default]
    Ignore,
    Separate,
    Collapse,
    CollapseNegative,
}

/// Experimental width policy for ordinary publisher text boxes.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BlockWidthMode {
    #[default]
    Ignore,
    Publisher,
    PublisherWithWrappers,
    PublisherWithConstraints,
}

/// Experimental intrinsic sizing for ordinary publisher paragraphs.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FitContentMode {
    #[default]
    Ignore,
    Book,
    Web,
}

/// Experimental horizontal float policy for the Parley provider.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FloatMode {
    #[default]
    Ignore,
    ContentHeight,
    PublisherHeight,
}

/// Experimental forced-page topology for empty publisher blocks.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EmptyBreakMode {
    #[default]
    Current,
    Coalesce,
    PreserveStructure,
    PreserveSized,
}

impl EmptyBreakMode {
    pub(crate) fn expands(self) -> bool {
        matches!(self, Self::PreserveStructure | Self::PreserveSized)
    }
}

const LEGACY_DEFAULT_LINE_HEIGHT: f32 = 1.35;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    pub width: u32,
    pub height: u32,
    /// Space reserved for the optional Kindle status header above reflowed text.
    pub status_header_height: u32,
    pub font_size: f32,
    /// None preserves the original physical-pixel renderer and bounds.
    pub logical_font_size: Option<f64>,
    pub font_scale: logical_font::ScaleContext,
    /// Scale CSS absolute lengths and images independently of the reading font.
    pub render_dpi: u16,
    pub margin: u32,
    pub margin_sides: Option<u32>,
    pub margin_left: Option<u32>,
    pub margin_right: Option<u32>,
    pub link_vertical_margins: bool,
    pub margin_top: Option<u32>,
    pub margin_bottom: Option<u32>,
    /// The default preserves the existing book layout and preference contract.
    #[doc(hidden)]
    pub block_margins: BlockMarginMode,
    /// Keep the incoming top margin at a supported forced publisher break.
    #[doc(hidden)]
    pub preserve_forced_page_margins: bool,
    #[doc(hidden)]
    pub block_width: BlockWidthMode,
    /// Reconstruct missing table rows and roots from supported publisher CSS boxes.
    #[doc(hidden)]
    pub repair_css_tables: bool,
    #[doc(hidden)]
    pub fit_content: FitContentMode,
    /// Apply positive physical sides to supported auto-width publisher paragraphs.
    #[doc(hidden)]
    pub publisher_horizontal_margins: bool,
    /// Apply one negative physical side when the publisher text box fits the screen.
    #[doc(hidden)]
    pub publisher_signed_horizontal_margins: bool,
    /// Honor definite publisher heights on supported horizontal text boxes.
    #[doc(hidden)]
    pub publisher_block_height: bool,
    #[doc(hidden)]
    pub float_mode: FloatMode,
    #[doc(hidden)]
    pub empty_break_mode: EmptyBreakMode,
    /// Position and paint supported empty boxes between ordinary text blocks.
    #[doc(hidden)]
    pub empty_break_geometry: bool,
    /// Let an oversized ordinary publisher keep group flow into available pages.
    #[doc(hidden)]
    pub relax_oversized_publisher_keeps: bool,
    /// Give an eligible explicit page-break-before priority at its opening edge.
    #[doc(hidden)]
    pub prefer_publisher_opening_breaks: bool,
    pub line_height: f32,
    /// None preserves legacy multiplier-based line spacing for existing profiles.
    pub line_spacing_percent: Option<u16>,
    pub word_spacing: f32,
    /// None preserves the existing additive-pixel layout exactly.
    pub percent_word_spacing: Option<WordSpacePercent>,
    /// Maximum tracking in justified words, as a percentage of font size.
    pub word_expansion: u8,
    pub letter_spacing: f32,
    /// Advance of eligible horizontal CJK glyphs, as a percentage of the shaped width.
    pub cjk_width_percent: u8,
    pub alignment: TextAlignment,
    pub bold_text: bool,
    pub text_contrast: bool,
    /// Index into the 57-level reflow glyph contrast scale. Index 15 is neutral.
    pub font_gamma_index: u8,
    /// Delta from regular weight, in 100-weight-class units. Range -3 to +5.5.
    pub font_weight_delta: f32,
    pub override_fonts: bool,
    /// Register font assets supplied by the publication's own CSS.
    pub embedded_fonts: bool,
    pub disable_soft_hyphens: bool,
    pub automatic_hyphenation: bool,
    /// Load author stylesheets and apply inline CSS during native reflow.
    pub publisher_styles: bool,
    /// Select the low-priority base stylesheet used beneath publisher CSS.
    pub base_stylesheet: BaseStylesheet,
    /// Use each font's normal OpenType kerning when shaping text.
    /// Kept for old book settings. New settings use `font_kerning_mode`.
    pub font_kerning: bool,
    /// None interprets the legacy boolean as Off or Best.
    pub font_kerning_mode: Option<FontKerningMode>,
    pub font_hinting: FontHinting,
    pub smooth_images: bool,
    pub invert_night_images: bool,
    /// Reuse the last lines on the next full-screen scroll turn.
    pub page_overlap_enabled: bool,
    /// Number of text lines retained between scroll turns, from 1 to 10.
    pub page_overlap_lines: u8,
    /// Visual marker drawn over the repeated area after a scroll-page turn.
    pub page_overlap_style: PageOverlapStyle,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            width: 1264,
            height: 1680,
            status_header_height: 0,
            font_size: 42.0,
            logical_font_size: None,
            font_scale: logical_font::ScaleContext::default(),
            render_dpi: 96,
            margin: 54,
            margin_sides: None,
            margin_left: None,
            margin_right: None,
            link_vertical_margins: false,
            margin_top: None,
            margin_bottom: None,
            block_margins: BlockMarginMode::Ignore,
            preserve_forced_page_margins: false,
            block_width: BlockWidthMode::Ignore,
            repair_css_tables: false,
            fit_content: FitContentMode::Ignore,
            publisher_horizontal_margins: false,
            publisher_signed_horizontal_margins: false,
            publisher_block_height: false,
            float_mode: FloatMode::Ignore,
            empty_break_mode: EmptyBreakMode::Current,
            empty_break_geometry: false,
            relax_oversized_publisher_keeps: false,
            prefer_publisher_opening_breaks: false,
            line_height: 1.35,
            line_spacing_percent: None,
            word_spacing: 0.0,
            percent_word_spacing: None,
            word_expansion: 0,
            letter_spacing: 0.0,
            cjk_width_percent: 100,
            alignment: TextAlignment::Publisher,
            bold_text: false,
            text_contrast: false,
            font_gamma_index: font_render::NORMAL_GAMMA,
            font_weight_delta: 0.0,
            override_fonts: false,
            embedded_fonts: true,
            disable_soft_hyphens: false,
            automatic_hyphenation: true,
            publisher_styles: true,
            base_stylesheet: BaseStylesheet::Auto,
            font_kerning: true,
            font_kerning_mode: None,
            font_hinting: FontHinting::Off,
            smooth_images: true,
            invert_night_images: false,
            page_overlap_enabled: false,
            page_overlap_lines: 1,
            page_overlap_style: PageOverlapStyle::Dim,
        }
    }
}

impl Settings {
    /// Resolve the four-mode setting while preserving old boolean profiles.
    pub const fn effective_font_kerning_mode(&self) -> FontKerningMode {
        match self.font_kerning_mode {
            Some(mode) => mode,
            None if self.font_kerning => FontKerningMode::Best,
            None => FontKerningMode::Off,
        }
    }

    /// Resolve the user line-spacing control as a multiplier for authored line height.
    pub fn line_height_scale(&self) -> f32 {
        self.line_spacing_percent.map_or_else(
            || self.line_height / LEGACY_DEFAULT_LINE_HEIGHT,
            |percent| f32::from(percent) / 100.0,
        )
    }

    /// Resolve the line-height multiplier used by the legacy fixed-layout renderer.
    pub fn effective_line_height(&self) -> f32 {
        self.line_spacing_percent
            .map_or(self.line_height, |percent| {
                LEGACY_DEFAULT_LINE_HEIGHT * f32::from(percent) / 100.0
            })
    }

    pub fn effective_line_spacing_percent(&self) -> u16 {
        self.line_spacing_percent.unwrap_or_else(|| {
            (self.line_height / LEGACY_DEFAULT_LINE_HEIGHT * 100.0)
                .round()
                .clamp(50.0, 200.0) as u16
        })
    }

    pub fn with_line_spacing_percent(self, percent: u16) -> Self {
        Self {
            line_spacing_percent: Some(percent),
            ..self
        }
    }

    pub fn page_overlap_height(&self) -> f32 {
        if !self.page_overlap_enabled {
            return 0.0;
        }
        let line_height =
            self.font_size * 1.1 * f32::from(self.effective_line_spacing_percent()) / 100.0;
        (line_height * f32::from(self.page_overlap_lines.clamp(1, 10))).max(0.0)
    }
}

#[cfg(feature = "kindle")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HyphenationMode {
    Automatic,
    Authored,
    Off,
}

#[cfg(feature = "kindle")]
impl HyphenationMode {
    const ALL: [Self; 3] = [Self::Automatic, Self::Authored, Self::Off];
    fn label(self) -> &'static str {
        match self {
            Self::Automatic => "Automatic",
            Self::Authored => "Authored",
            Self::Off => "Off",
        }
    }
    fn next(self) -> Self {
        match self {
            Self::Automatic => Self::Authored,
            Self::Authored => Self::Off,
            Self::Off => Self::Automatic,
        }
    }
}
impl Settings {
    #[cfg(feature = "kindle")]
    fn hyphenation_mode(self) -> HyphenationMode {
        if self.disable_soft_hyphens {
            HyphenationMode::Off
        } else if self.automatic_hyphenation {
            HyphenationMode::Automatic
        } else {
            HyphenationMode::Authored
        }
    }
    #[cfg(feature = "kindle")]
    fn with_hyphenation(self, mode: HyphenationMode) -> Self {
        Self {
            disable_soft_hyphens: mode == HyphenationMode::Off,
            automatic_hyphenation: mode == HyphenationMode::Automatic,
            ..self
        }
    }
    fn validate(self) -> Result<()> {
        let margin_limit = self.width.min(self.height) / 4;
        if !(160..=4096).contains(&self.width)
            || !(160..=4096).contains(&self.height)
            || self.margin > margin_limit
            || self.left_margin() > margin_limit
            || self.right_margin() > margin_limit
            || self.top_margin() > self.height / 4
            || self.bottom_margin() > self.height / 4
            || !valid_render_dpi(self.render_dpi)
            || if let Some(logical) = self.logical_font_size {
                !logical_font::valid(logical)
                    || self.font_scale.effective(logical).ok() != Some(self.font_size)
            } else {
                !(12.0..=144.0).contains(&self.font_size)
            }
            || !(1.0..=3.0).contains(&self.line_height)
            || !(-2.0..=12.0).contains(&self.word_spacing)
            || self.percent_word_spacing.is_some_and(|pair| !pair.valid())
            || self.word_expansion > 20
            || !(-1.0..=4.0).contains(&self.letter_spacing)
            || !(100..=150).contains(&self.cjk_width_percent)
            || self.text_height() as f32
                <= self.font_size
                    * self.line_height
                    * if self.logical_font_size.is_some() {
                        1.0
                    } else {
                        2.0
                    }
        {
            return Err("Reading layout settings are out of range".into());
        }
        Ok(())
    }

    fn side_margin(self) -> u32 {
        self.margin_sides.unwrap_or(self.margin)
    }

    fn left_margin(self) -> u32 {
        self.margin_left.unwrap_or_else(|| self.side_margin())
    }

    fn right_margin(self) -> u32 {
        self.margin_right.unwrap_or_else(|| self.side_margin())
    }

    fn text_columns(self) -> Range<usize> {
        self.left_margin() as usize..(self.width - self.right_margin()) as usize
    }

    fn text_hit_column(self, x: f32) -> Option<bool> {
        let inside =
            (self.left_margin() as f32..(self.width - self.right_margin()) as f32).contains(&x);
        let allows_bleed =
            self.publisher_signed_horizontal_margins || self.fit_content == FitContentMode::Web;
        (inside || allows_bleed && (0.0..self.width as f32).contains(&x)).then_some(inside)
    }

    fn top_margin(self) -> u32 {
        self.margin_top
            .unwrap_or(self.margin)
            .saturating_add(self.status_header_height)
            .min(self.height / 4)
    }

    fn bottom_margin(self) -> u32 {
        self.margin_bottom.unwrap_or(self.margin)
    }

    fn text_width(self) -> u32 {
        self.width - self.left_margin() - self.right_margin()
    }

    fn text_height(self) -> u32 {
        self.height - self.top_margin() - self.bottom_margin()
    }
}

/// A text location in a specific revision of a chapter. Page numbers are derived.
/// Offsets are UTF-8 bytes in the whitespace-normalized text of a block.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Anchor {
    pub book: u64,
    pub chapter: String,
    pub revision: u64,
    pub block: usize,
    pub offset: usize,
    /// Zero-based picture within a text-empty block holding several illustrations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub illustration: Option<usize>,
}

struct Line {
    block: usize,
    bytes: Range<usize>,
    /// Exact canonical spans when projected layout crosses omitted source text.
    /// `bytes` remains the envelope for pagination endpoints only.
    spans: Option<std::sync::Arc<[Range<usize>]>>,
    top: f32,
    bottom: f32,
    // Attached paint at the first and last line of a fitted paragraph.
    // Text geometry and canonical offsets continue to use top and bottom.
    page_extension: [f32; 2],
    missing_glyphs: usize,
}

impl Line {
    fn page_extent(&self) -> Range<f32> {
        self.top - self.page_extension[0]..self.bottom + self.page_extension[1]
    }

    fn keep_page_extension(&mut self, extension: [f32; 2], page_height: f32) {
        if self.bottom - self.top + extension[0] + extension[1] <= page_height {
            self.page_extension = extension;
        }
    }

    fn source_spans(&self) -> &[Range<usize>] {
        self.spans
            .as_deref()
            .unwrap_or_else(|| std::slice::from_ref(&self.bytes))
    }

    fn contains_source(&self, offset: usize) -> bool {
        self.source_spans()
            .iter()
            .any(|span| span.contains(&offset))
    }

    fn project(
        &mut self,
        view: &flow_projection::Projection<'_>,
        bytes: Range<usize>,
    ) -> Result<()> {
        let spans = view
            .source_spans(bytes.clone())
            .ok_or("A layout line has invalid projected boundaries")?;
        let mut kept: Vec<Range<usize>> = Vec::new();
        for span in spans {
            if let Some(last) = kept.last_mut()
                && last.end == span.start
            {
                last.end = span.end;
            } else {
                kept.push(span);
            }
        }
        self.bytes = match (kept.first(), kept.last()) {
            (Some(first), Some(last)) => first.start..last.end,
            _ => {
                let point = view
                    .source_offset(bytes.start, flow_projection::Affinity::After)
                    .or_else(|| view.source_offset(bytes.start, flow_projection::Affinity::Before))
                    .unwrap_or(0);
                point..point
            }
        };
        self.spans = (kept.len() > 1).then(|| kept.into());
        Ok(())
    }
}

struct Pagination {
    bounds: Vec<f32>,
}

#[derive(Debug)]
struct PageContentTooTall;
impl std::fmt::Display for PageContentTooTall {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("The chapter contains content taller than one reading page")
    }
}
impl Error for PageContentTooTall {}

impl Pagination {
    fn fixed_stride(height: f32, extent: f32) -> Result<Self> {
        Self::fixed_stride_capped(height, extent, 2_000_000.0)
    }
    fn fixed_stride_capped(height: f32, extent: f32, limit: f32) -> Result<Self> {
        if !extent.is_finite() || !(0.0..=limit).contains(&extent) {
            return Err("The chapter exceeds the layout height limit".into());
        }
        let pages = (extent / height).ceil().max(1.0) as usize;
        let mut bounds = (0..pages)
            .map(|page| page as f32 * height)
            .collect::<Vec<_>>();
        bounds.push(extent);
        Ok(Self { bounds })
    }

    fn new(lines: &[Line], height: f32, content_height: f32) -> Result<Self> {
        Self::with_breaks(lines, height, content_height, &[])
    }

    fn with_breaks(
        lines: &[Line],
        height: f32,
        content_height: f32,
        breaks: &[publisher_breaks::BoxRules],
    ) -> Result<Self> {
        Self::with_breaks_capped(lines, height, content_height, breaks, 2_000_000.0)
    }
    fn with_breaks_capped(
        lines: &[Line],
        height: f32,
        content_height: f32,
        breaks: &[publisher_breaks::BoxRules],
        limit: f32,
    ) -> Result<Self> {
        Self::with_split_spaces(lines, height, content_height, breaks, &[], &[], limit)
    }

    fn with_split_spaces(
        lines: &[Line],
        height: f32,
        mut content_height: f32,
        breaks: &[publisher_breaks::BoxRules],
        split_spaces: &[Range<f32>],
        fitted_tails: &[publisher_padding::fit_content::HeightTail],
        limit: f32,
    ) -> Result<Self> {
        let mut intervals: Vec<Range<f32>> = Vec::new();
        let mut tails = fitted_tails.iter().peekable();
        let mut include = |extent: Range<f32>| {
            content_height = content_height.max(extent.end);
            match intervals.last_mut() {
                Some(last) if extent.start < last.end => {
                    last.start = last.start.min(extent.start);
                    last.end = last.end.max(extent.end);
                }
                _ => intervals.push(extent),
            }
        };
        for line in lines {
            if !line.top.is_finite() || !line.bottom.is_finite() || line.top > line.bottom {
                return Err("The renderer returned invalid line positions".into());
            }
            let extent = line.page_extent();
            while tails
                .peek()
                .is_some_and(|tail| tail.closing.start < extent.start)
            {
                include(tails.next().unwrap().closing.clone());
            }
            include(extent);
        }
        for tail in tails {
            include(tail.closing.clone());
        }
        if !content_height.is_finite() || !(0.0..=limit).contains(&content_height) {
            return Err("The chapter exceeds the layout height limit".into());
        }
        let boundaries = publisher_breaks::boundaries(&intervals, breaks);
        let extra_splits = publisher_breaks::structural_splits(&intervals, breaks);
        let physical_cuts = publisher_breaks::physical_cuts(&intervals, breaks);
        let mut bounds = vec![0.0];
        let mut allowed_start = 0.0;
        for (index, interval) in intervals.iter().enumerate() {
            use publisher_breaks::Edge;
            if interval.end - interval.start > height {
                return Err(Box::new(PageContentTooTall));
            }
            let origin = *bounds.last().unwrap();
            let edge = boundaries.get(index).copied().unwrap_or_default();
            if edge != Edge::Avoid {
                allowed_start = interval.start;
            }
            let forced = index > 0 && edge == Edge::Force;
            if forced
                && let Some(cuts) = physical_cuts.get(index).copied().flatten()
                && cuts.len()
                    == 1 + usize::from(extra_splits.get(index).copied().unwrap_or_default())
                && cuts[0] > origin
                && cuts[0] <= origin + height
                && interval.end <= cuts[cuts.len() - 1] + height
            {
                bounds.extend_from_slice(cuts);
                continue;
            }
            let overflows = interval.end > origin + height && interval.start > origin;
            if overflows && edge == Edge::Auto && index > 0 {
                let cut = origin + height;
                let spaces = split_spaces.partition_point(|space| space.start <= cut);
                // A rendered CSS height can have splittable trailing space.
                // Only use it when the cut lies between real text/image lines.
                if intervals[index - 1].end <= cut
                    && cut < interval.start
                    && interval.end <= cut + height
                    && spaces > 0
                    && cut < split_spaces[spaces - 1].end
                {
                    bounds.push(cut);
                    continue;
                }
            }
            if forced || overflows {
                let next = if allowed_start > origin && interval.end <= allowed_start + height {
                    allowed_start
                } else {
                    interval.start
                };
                while next >= *bounds.last().unwrap() + 2.0 * height {
                    bounds.push(bounds.last().unwrap() + height);
                }
                if next > *bounds.last().unwrap() {
                    bounds.push(next);
                    if forced {
                        let count = extra_splits.get(index).copied().unwrap_or_default();
                        bounds.extend(std::iter::repeat_n(next, usize::from(count)));
                    }
                }
            }
        }
        while content_height > *bounds.last().unwrap() + height {
            bounds.push(bounds.last().unwrap() + height);
        }
        bounds.push(content_height.max(*bounds.last().unwrap()));
        Ok(Self { bounds })
    }

    fn trim_unpainted_tail(&mut self, painted_bottom: f32) {
        if self.len() < 2 || self.bounds[self.len() - 1] < painted_bottom {
            return;
        }
        // Keep at least one view for a deliberately empty spine entry. The source
        // and all of its line positions remain available for links and saved anchors.
        let pages = self.bounds[..self.len()]
            .partition_point(|&start| start < painted_bottom)
            .max(1);
        self.bounds.truncate(pages + 1);
        self.bounds[pages] = painted_bottom;
    }

    fn page(&self, y: f32) -> usize {
        self.bounds[..self.len()]
            .partition_point(|&bound| bound <= y)
            .saturating_sub(1)
    }

    fn len(&self) -> usize {
        self.bounds.len() - 1
    }

    fn range(&self, page: usize) -> Option<Range<f32>> {
        (page < self.len()).then(|| self.bounds[page]..self.bounds[page + 1])
    }
}

enum Renderer {
    Blitz(Box<blitz::BlitzChapter>),
    Parley(parley_layout::ParleyChapter),
    Vertical(Box<vertical_layout::VerticalChapter>),
}

// Keep the shared model fields identical while keeping Blitz's DOM on the
// originating thread. A worker can transport its horizontal Parley layouts.
macro_rules! chapter_probe {
    ($( $(#[$attribute:meta])* $visibility:vis $field:ident : $kind:ty, )*) => {
        /// One bounded chapter layout.
        pub struct Probe {
            $( $(#[$attribute])* $visibility $field: $kind, )*
            renderer: Renderer,
        }

        #[cfg(feature = "kindle")]
        pub(crate) struct PreparedParleyChapter {
            $( $(#[$attribute])* $field: $kind, )*
            renderer: parley_layout::ParleyChapter,
        }

        #[cfg(feature = "kindle")]
        impl Probe {
            fn into_prepared_parley(self) -> Option<PreparedParleyChapter> {
                let Renderer::Parley(renderer) = self.renderer else {
                    return None;
                };
                Some(PreparedParleyChapter {
                    $( $(#[$attribute])* $field: self.$field, )*
                    renderer,
                })
            }
        }

        #[cfg(feature = "kindle")]
        impl PreparedParleyChapter {
            fn into_probe(self) -> Probe {
                Probe {
                    $( $(#[$attribute])* $field: self.$field, )*
                    renderer: Renderer::Parley(self.renderer),
                }
            }
        }
    };
}

chapter_probe! {
    pub(crate) first_series: Option<crate::epub::BookSeries>,
    pub title: String,
    pub author: Option<String>,
    pub chapter: String,
    pub chapter_label: String,
    #[cfg(feature = "kindle")]
    pub(crate) embedded_font_usage: embedded_font_usage::Families,
    book: u64,
    chapters: Vec<Chapter>,
    pub toc: Vec<TocEntry>,
    #[cfg(feature = "kindle")]
    pub(crate) printed_pages: Vec<TocEntry>,
    #[cfg(feature = "kindle")]
    pub(crate) printed_source: Option<String>,
    #[cfg(feature = "kindle")]
    footer_chapters: std::cell::OnceCell<Vec<(usize, String)>>,
    #[cfg(feature = "kindle")]
    local_contents_pages: std::cell::OnceCell<Vec<(usize, usize)>>,
    #[cfg(feature = "kindle")]
    pace_page: std::cell::Cell<Option<(usize, u64)>>,
    #[cfg(feature = "kindle")]
    publisher_positions: std::cell::OnceCell<app::publisher_pages::PublisherPositions>,
    #[cfg(feature = "kindle")]
    pub(crate) flow_pages: Option<crate::library::hidden_flows::PageFlows>,
    #[cfg(feature = "kindle")]
    screen_page_sources: Option<session::screen_pages::ScreenPageSources>,
    settings: Settings,
    spine_rtl: bool,
    #[cfg(feature = "kindle")]
    requests_vertical: bool,
    source: Source,
    reading_flow: flow_view::ChapterFlow,
    #[cfg(feature = "kindle")]
    custom_hyphenation_allowed: Vec<bool>,
    lines: Vec<Line>,
    pages: Pagination,
}

impl Probe {
    pub fn open(
        path: &Path,
        engine: Engine,
        fonts: &[PathBuf],
        settings: Settings,
        chapter: usize,
    ) -> Result<Self> {
        settings.validate()?;
        let book = Book::open(path).map_err(|e| e.to_string())?;
        Self::from_book(book, engine, fonts, settings, chapter)
    }

    #[cfg(feature = "kindle")]
    pub(crate) fn from_text(
        title: String,
        text: &str,
        fonts: &[PathBuf],
        settings: Settings,
    ) -> Result<Self> {
        settings.validate()?;
        Self::from_book(
            Book::from_text(title, text)?,
            Engine::Parley,
            fonts,
            settings,
            0,
        )
    }

    fn from_book(
        book: Book,
        engine: Engine,
        fonts: &[PathBuf],
        settings: Settings,
        chapter: usize,
    ) -> Result<Self> {
        Self::from_book_content(book, engine, fonts, settings, chapter, None)
    }

    fn from_book_content(
        book: Book,
        engine: Engine,
        fonts: &[PathBuf],
        settings: Settings,
        chapter: usize,
        excerpt: Option<String>,
    ) -> Result<Self> {
        Self::from_book_with_rules(
            book,
            engine,
            fonts,
            settings,
            chapter,
            excerpt,
            &user_hyphenation::Rules::default(),
            &user_styles::Snapshot::default(),
        )
    }

    fn from_book_with_rules(
        book: Book,
        engine: Engine,
        fonts: &[PathBuf],
        settings: Settings,
        chapter: usize,
        excerpt: Option<String>,
        user_rules: &user_hyphenation::Rules,
        user_styles: &user_styles::Snapshot,
    ) -> Result<Self> {
        Self::from_book_with_font_loader(
            book,
            engine,
            settings,
            chapter,
            excerpt,
            user_rules,
            user_styles,
            || font_context(fonts),
        )
    }

    fn from_book_with_font_loader(
        mut book: Book,
        engine: Engine,
        settings: Settings,
        chapter: usize,
        excerpt: Option<String>,
        user_rules: &user_hyphenation::Rules,
        user_styles: &user_styles::Snapshot,
        load_fonts: impl FnOnce() -> Result<FontContext>,
    ) -> Result<Self> {
        if book.is_fixed_layout() {
            return Err("Open this fixed-layout EPUB in Reader".into());
        }
        if engine == Engine::Blitz
            && (!user_styles.is_empty()
                || !settings.publisher_styles
                || !settings.embedded_fonts
                || settings.effective_font_kerning_mode() != FontKerningMode::Best
                || settings.font_hinting != FontHinting::Off
                || settings.font_gamma_index != font_render::NORMAL_GAMMA
                || settings.font_weight_delta != 0.0
                || settings.word_expansion != 0)
        {
            return Err(
                "Publisher style, font rendering and word expansion controls require Parley".into(),
            );
        }
        let entry = book
            .chapters
            .get(chapter)
            .ok_or("That chapter is unavailable")?;
        let href = entry.href.clone();
        let chapter_label = entry.label.clone();
        let title = book.title.clone();
        let author = book.author.clone();
        let first_series = book.series().first().cloned();
        let publication = book.revision();
        let spine_rtl = book.spine_rtl;
        let chapters = book.chapters.clone();
        let toc = book.toc.clone();
        #[cfg(feature = "kindle")]
        let printed_pages = book.printed_pages().to_vec();
        #[cfg(feature = "kindle")]
        let printed_source = book.printed_page_source().map(str::to_owned);
        let complete = excerpt.is_none();
        let xhtml = match excerpt {
            Some(value) => value,
            None => book.chapter(chapter).map_err(|e| e.to_string())?,
        };
        let mut source = if complete {
            book.chapter_source(&href, &xhtml)?
        } else {
            Source::parse_with_language(&xhtml, book.language)?
        };
        if complete {
            for previous in book.previous_mobi_chapters(&href) {
                source.accept_previous_mobi(previous)?;
            }
        }
        let mut fonts = load_fonts()?;
        #[cfg(feature = "kindle")]
        let mut custom_hyphenation_allowed = Vec::new();
        let mut publisher_edges = Vec::new();
        let mut reading_flow = flow_view::ChapterFlow::default();
        #[cfg(feature = "kindle")]
        let mut requests_vertical = false;
        #[cfg(feature = "kindle")]
        let mut registered_faces = std::collections::HashMap::new();
        let (renderer, mut lines, content_height) = match engine {
            Engine::Blitz => {
                let (layout, lines, extent) =
                    blitz::BlitzChapter::new(book, &href, &xhtml, &source, fonts, settings)?;
                (Renderer::Blitz(Box::new(layout)), lines, extent)
            }
            Engine::Parley => {
                let author = book_style::BookStyle::load_with_base(
                    &mut book, &href, &source, &mut fonts, settings,
                )
                .with_user_css(user_styles)?;
                #[cfg(feature = "kindle")]
                {
                    registered_faces.clone_from(author.registered_faces());
                }
                let styles = author.compute_for_settings(&source.elements, settings);
                reading_flow = flow_view::ChapterFlow::from_styles(&source, &styles)?;
                #[cfg(feature = "kindle")]
                {
                    custom_hyphenation_allowed =
                        styles.iter().map(hyphenation::allows_user_rules).collect();
                }
                // Decide writing mode from retained display input. Hidden media or
                // horizontal-only subtrees cannot force a visible vertical chapter
                // down the horizontal fallback path.
                let mut has_text = false;
                let mut vertical = !source.elements.iter().enumerate().any(|(index, element)| {
                    !styles[index].flow.suppressed
                        && !element.selector_only
                        && element.tag == "table"
                });
                for original in &source.blocks {
                    if styles[original.element].flow.suppressed {
                        continue;
                    }
                    let block = reading_flow.layout_block(&source, original)?;
                    vertical &= block.images.is_empty();
                    if !block.text.is_empty() {
                        has_text = true;
                        vertical &= styles[block.element].writing_mode
                            == book_style::WritingMode::VerticalRl
                            && block.styled_text.iter().all(|run| {
                                styles[run.element].writing_mode
                                    == book_style::WritingMode::VerticalRl
                            });
                    }
                }
                vertical &= has_text;
                #[cfg(feature = "kindle")]
                {
                    requests_vertical = vertical;
                }
                let vertical = vertical.then(|| {
                    vertical_layout::VerticalChapter::with_flow(
                        &source,
                        &styles,
                        fonts.clone(),
                        settings,
                        &reading_flow,
                    )
                });
                match vertical {
                    Some(Ok((layout, lines, extent))) => {
                        (Renderer::Vertical(Box::new(layout)), lines, extent)
                    }
                    fallback => {
                        if let Some(Err(error)) = fallback {
                            eprintln!("reader vertical layout unavailable: {error}");
                        }
                        let has_table = source.blocks.iter().any(|b| b.table_cell.is_some())
                            || settings.repair_css_tables
                                && styles.iter().any(|style| {
                                    !style.flow.suppressed
                                        && style.display == book_style::flow::Display::TableCell
                                });
                        let mut lay_out = |source: &Source, faces| {
                            parley_layout::ParleyChapter::with_flow(
                                &mut book,
                                &href,
                                source,
                                faces,
                                settings,
                                &styles,
                                user_rules,
                                &reading_flow,
                            )
                        };
                        let (layout, lines, extent) = if has_table {
                            let grid = lay_out(&source, fonts.clone()).and_then(
                                |(layout, mut lines, extent)| {
                                    lines.sort_by(|a, b| a.top.total_cmp(&b.top));
                                    Pagination::with_breaks_capped(
                                        &lines,
                                        settings.text_height() as f32,
                                        extent,
                                        &[],
                                        settings.chapter_height_limit(),
                                    )?;
                                    Ok((layout, lines, extent))
                                },
                            );
                            match grid {
                                Ok(result) => result,
                                Err(error)
                                    if error.downcast_ref::<PageContentTooTall>().is_some() =>
                                {
                                    // A tall row can have no safe horizontal break across
                                    // all its cells. Keep the chapter readable in source order.
                                    drop(lay_out);
                                    parley_layout::ParleyChapter::with_source_order(
                                        &mut book,
                                        &href,
                                        &source,
                                        fonts,
                                        settings,
                                        &styles,
                                        user_rules,
                                        &reading_flow,
                                    )?
                                }
                                Err(error) => return Err(error),
                            }
                        } else {
                            lay_out(&source, fonts)?
                        };
                        publisher_edges = publisher_breaks::layout(
                            &source,
                            &styles,
                            &lines,
                            settings,
                            layout.physical_break_runs(),
                            layout.fitted_height_tails(),
                        );
                        (Renderer::Parley(layout), lines, extent)
                    }
                }
            }
        };
        lines.sort_by(|a, b| a.top.total_cmp(&b.top));
        let height = settings.text_height() as f32;
        let mut pages = if matches!(&renderer, Renderer::Vertical(_)) {
            Pagination::fixed_stride_capped(
                height,
                content_height,
                settings.chapter_height_limit(),
            )?
        } else {
            let (split_spaces, fitted_height_tails) = match &renderer {
                Renderer::Parley(chapter) => (
                    chapter.publisher_height_spaces(),
                    chapter.fitted_height_tails(),
                ),
                _ => (&[][..], &[][..]),
            };
            Pagination::with_split_spaces(
                &lines,
                height,
                content_height,
                &publisher_edges,
                split_spaces,
                fitted_height_tails,
                settings.chapter_height_limit(),
            )?
        };
        if pages.len() > 1
            && let Renderer::Parley(chapter) = &renderer
        {
            pages.trim_unpainted_tail(chapter.last_paint_bottom(&source));
        }
        Ok(Self {
            title,
            author,
            #[cfg(feature = "kindle")]
            first_series,
            chapter: href,
            chapter_label,
            #[cfg(feature = "kindle")]
            embedded_font_usage: embedded_font_usage::Families::from_layout(
                &registered_faces,
                &renderer,
            ),
            book: publication,
            spine_rtl,
            #[cfg(feature = "kindle")]
            requests_vertical,
            chapters,
            toc,
            #[cfg(feature = "kindle")]
            printed_pages,
            #[cfg(feature = "kindle")]
            printed_source,
            #[cfg(feature = "kindle")]
            footer_chapters: Default::default(),
            #[cfg(feature = "kindle")]
            local_contents_pages: Default::default(),
            #[cfg(feature = "kindle")]
            pace_page: Default::default(),
            #[cfg(feature = "kindle")]
            publisher_positions: Default::default(),
            #[cfg(feature = "kindle")]
            flow_pages: None,
            #[cfg(feature = "kindle")]
            screen_page_sources: None,
            settings,
            #[cfg(feature = "kindle")]
            custom_hyphenation_allowed,
            source,
            reading_flow,
            lines,
            pages,
            renderer,
        })
    }

    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    pub(crate) fn is_vertical(&self) -> bool {
        matches!(&self.renderer, Renderer::Vertical(_))
    }

    /// The visual direction of forward page turns for this chapter.
    pub fn pages_right_to_left(&self) -> bool {
        self.spine_rtl || self.is_vertical()
    }

    pub fn chapter_count(&self) -> usize {
        self.chapters.len()
    }

    #[cfg(feature = "kindle")]
    pub(crate) fn has_nonlinear_chapters(&self) -> bool {
        self.chapters.iter().any(|chapter| chapter.nonlinear)
            && self.chapters.iter().any(|chapter| !chapter.nonlinear)
    }

    #[cfg(feature = "kindle")]
    pub(crate) fn chapter_is_nonlinear(&self, index: usize) -> bool {
        self.chapters
            .get(index)
            .is_some_and(|chapter| chapter.nonlinear)
    }

    #[cfg(feature = "kindle")]
    pub(crate) fn proportional_anchor(&self, fraction: f64) -> Option<Anchor> {
        let total: usize = self
            .source
            .blocks
            .iter()
            .filter_map(|block| {
                self.reading_flow
                    .view(&self.source, block)
                    .ok()
                    .map(|view| view.text().len())
            })
            .sum();
        let mut remaining =
            (total.saturating_sub(1) as f64 * fraction.clamp(0.0, 1.0)).round() as usize;
        for block in &self.source.blocks {
            let view = self.reading_flow.view(&self.source, block).ok()?;
            if remaining < view.text().len() {
                let offset = view.text().floor_char_boundary(remaining);
                return Some(self.anchor(
                    block.key,
                    view.source_offset(offset, flow_projection::Affinity::After)?,
                ));
            }
            remaining -= view.text().len();
        }
        None
    }

    pub fn chapter_index(&self, path: &str) -> Option<usize> {
        self.chapters
            .iter()
            .position(|chapter| chapter.href == path)
    }

    pub fn chapter_entries(&self) -> &[Chapter] {
        &self.chapters
    }

    /// Resolve a successfully displayed illustration at a page's viewport coordinates.
    pub fn illustration_at(&self, page: usize, x: f32, y: f32) -> Option<&str> {
        self.illustration_in(self.pages.range(page)?, x, y)
    }

    pub(crate) fn illustration_in(&self, range: Range<f32>, x: f32, y: f32) -> Option<&str> {
        let Renderer::Parley(layout) = &self.renderer else {
            return None;
        };
        layout.illustration_at(range, self.settings, x, y)
    }

    fn links_to_image(&self, path: &str) -> bool {
        let supported = path.rsplit_once('.').is_some_and(|(_, ext)| {
            ["png", "jpg", "jpeg", "gif", "svg"]
                .iter()
                .any(|known| ext.eq_ignore_ascii_case(known))
        });
        supported
            && self.source.blocks.iter().any(|block| {
                block
                    .links
                    .iter()
                    .map(|link| link.href.as_str())
                    .chain(
                        block
                            .images
                            .iter()
                            .filter_map(|image| image.link.as_deref()),
                    )
                    .any(|href| {
                        Book::resolve_link(&self.chapter, href)
                            .is_ok_and(|target| target.fragment.is_none() && target.path == path)
                    })
            })
    }

    /// Load original pixels for a displayed illustration, reopening the local book as needed.
    pub fn open_illustration(&self, path: &Path, package_path: &str) -> Result<comic::ImagePage> {
        if !matches!(&self.renderer, Renderer::Parley(layout) if layout.has_illustration(package_path))
            && !self.links_to_image(package_path)
        {
            return Err("This illustration is unavailable in the current chapter".into());
        }
        let mut book = Book::open(path)?;
        if book.revision() != self.book || book.chapter_index(&self.chapter).is_none() {
            return Err("This book changed after it was opened".into());
        }
        if let Some(xml) = self
            .source
            .blocks
            .iter()
            .flat_map(|block| &block.images)
            .find(|image| image.href == package_path)
            .and_then(|image| image.inline_svg.as_deref())
        {
            let (gray, _) = svg_image::render_inline(
                &mut book,
                &self.chapter,
                xml,
                svg_image::MAX_SVG_SIDE,
                svg_image::MAX_SVG_SIDE,
                8 * 1024 * 1024,
                None,
            )?;
            return comic::ImagePage::from_grayscale(gray.width(), gray.height(), gray.into_raw());
        }
        if svg_image::is_package_svg(package_path) {
            let bytes = book.resource_limited(package_path, svg_image::MAX_SVG_BYTES)?;
            return svg_image::original_package_image(&mut book, package_path, &bytes);
        }
        let bytes = book.resource(package_path)?;
        drop(book);
        parley_layout::original_illustration(&bytes)
    }

    pub fn link_at(&self, page: usize, x: f32, y: f32) -> Option<Destination> {
        let range = self.pages.range(page)?;
        self.link_in(range, x, y)
    }

    pub(crate) fn link_in(&self, range: Range<f32>, x: f32, y: f32) -> Option<Destination> {
        Book::resolve_link(&self.chapter, self.link_href_in(range, x, y)?).ok()
    }

    pub(crate) fn link_href_in(&self, range: Range<f32>, x: f32, y: f32) -> Option<&str> {
        if let Renderer::Parley(layout) = &self.renderer
            && let Some(href) = layout.image_link_at(range.clone(), self.settings, x, y)
        {
            return Some(href);
        }
        let (key, offset) = self.source_at(range, x, y)?;
        let block = self.source.blocks.iter().find(|block| block.key == key)?;
        let link = block
            .links
            .iter()
            .find(|link| link.bytes.contains(&offset))?;
        Some(&link.href)
    }

    fn source_at(&self, range: Range<f32>, x: f32, y: f32) -> Option<(usize, usize)> {
        match &self.renderer {
            Renderer::Vertical(renderer) => renderer.source_at(range, x, y),
            Renderer::Parley(renderer) => {
                let inside = self.settings.text_hit_column(x)?;
                let left = self.settings.left_margin() as f32;
                let local_y = y - self.settings.top_margin() as f32;
                let y = range.start + local_y;
                if !(0.0..self.settings.text_height() as f32).contains(&local_y)
                    || !range.contains(&y)
                {
                    return None;
                }
                renderer.source_at(x - left, y, &range, inside)
            }
            Renderer::Blitz(_) => None,
        }
    }

    pub fn links_on_page(&self, page: usize) -> Vec<(String, Destination)> {
        let Some(bounds) = self.pages.range(page) else {
            return Vec::new();
        };
        self.links_in(bounds)
    }

    pub(crate) fn links_in(&self, bounds: Range<f32>) -> Vec<(String, Destination)> {
        let mut links: Vec<_> = self
            .text_link_hrefs_in(bounds.clone())
            .into_iter()
            .filter_map(|(label, href)| {
                Some((label, Book::resolve_link(&self.chapter, href).ok()?))
            })
            .collect();
        if let Renderer::Parley(layout) = &self.renderer {
            for (label, href) in layout.image_links(bounds, self.settings) {
                if let Ok(target) = Book::resolve_link(&self.chapter, href)
                    && !links.iter().any(|(_, existing)| existing == &target)
                {
                    links.push((label.to_owned(), target));
                }
            }
        }
        links
    }

    pub(crate) fn link_hrefs_in(&self, bounds: Range<f32>) -> Vec<(String, &str)> {
        let mut links = self.text_link_hrefs_in(bounds.clone());
        if let Renderer::Parley(layout) = &self.renderer {
            for (label, href) in layout.image_links(bounds, self.settings) {
                if !links.iter().any(|(_, existing)| *existing == href) {
                    links.push((label.to_owned(), href));
                }
            }
        }
        links
    }

    fn text_link_hrefs_in(&self, bounds: Range<f32>) -> Vec<(String, &str)> {
        // Several cells may have lines crossing the top of a scrolled view.
        // A paginated line can extend upward by at most one reading viewport.
        let earliest = bounds.start - self.settings.text_height() as f32;
        let start = self.lines.partition_point(|line| line.top < earliest);
        let end = self.lines.partition_point(|line| line.top < bounds.end);
        let visible_lines = &self.lines[start..end];
        let mut links = Vec::new();
        for block in &self.source.blocks {
            for link in &block.links {
                let visible = visible_lines.iter().any(|line| {
                    line.bottom > bounds.start
                        && line.block == block.key
                        && line
                            .source_spans()
                            .iter()
                            .any(|span| span.start < link.bytes.end && span.end > link.bytes.start)
                });
                if visible {
                    let Ok(view) = self.reading_flow.view(&self.source, block) else {
                        continue;
                    };
                    let Some(spans) = view.layout_spans(link.bytes.clone()) else {
                        continue;
                    };
                    let label: String = spans
                        .iter()
                        .map(|span| &view.text()[span.clone()])
                        .collect();
                    if !label.trim().is_empty() {
                        links.push((label.trim().to_owned(), link.href.as_str()));
                    }
                }
            }
        }
        links
    }

    pub fn matches(&self, text: &str) -> Vec<Anchor> {
        if text.is_empty() {
            return Vec::new();
        }
        let mut result = Vec::new();
        for block in &self.source.blocks {
            let Ok(view) = self.reading_flow.view(&self.source, block) else {
                continue;
            };
            for (offset, _) in view.text().match_indices(text) {
                if let Some(offset) = view.source_offset(offset, flow_projection::Affinity::After) {
                    result.push(self.anchor(block.key, offset));
                }
            }
        }
        result
    }

    pub fn fragment(&self, name: &str) -> Option<Anchor> {
        if self.fragment_presentation(name)? == SourcePresentation::CssOmitted {
            return None;
        }
        let &(block, offset) = self.source.fragments.get(name)?;
        let original = self.anchor(block, offset);
        if self.page_for(&original).is_some() {
            return Some(original);
        }
        // Container IDs can begin with a removed inline prefix. Resolve only
        // inside their retained subtree; an explicitly hidden ID never reaches
        // this fallback, even if its old caret aliases a visible neighbour.
        let owner = self.source.fragment_owner(name)?;
        for block in &self.source.blocks {
            let view = self.reading_flow.view(&self.source, block).ok()?;
            let candidates: Vec<_> = match view.runs() {
                Some(runs) => runs
                    .iter()
                    .filter(|run| self.element_within(run.element, owner))
                    .map(|run| run.source.start)
                    .collect(),
                None => block
                    .styled_text
                    .iter()
                    .filter(|run| self.element_within(run.element, owner))
                    .map(|run| run.bytes.start)
                    .collect(),
            };
            for offset in candidates {
                let anchor = self.anchor(block.key, offset);
                if self.page_for(&anchor).is_some() {
                    return Some(anchor);
                }
            }
            let input = self.reading_flow.layout_block(&self.source, block).ok()?;
            for image in input.images.iter() {
                if self.element_within(image.element, owner) {
                    let mut anchor = self.anchor(block.key, 0);
                    anchor.illustration = Some(image.source_index);
                    if self.page_for(&anchor).is_some() {
                        return Some(anchor);
                    }
                }
            }
        }
        let is_epub_pagebreak = self
            .source
            .elements
            .get(owner)
            .and_then(|element| element.type_attribute.as_deref())
            .is_some_and(|value| {
                value
                    .split_ascii_whitespace()
                    .any(|token| token.eq_ignore_ascii_case("pagebreak"))
            });
        is_epub_pagebreak
            .then(|| self.nearest_pagebreak_text(&original))
            .flatten()
    }

    /// KOReader hides EPUB pagebreak markers and resolves their destinations to
    /// the next visible text, falling back to the previous visible text at EOF.
    fn nearest_pagebreak_text(&self, original: &Anchor) -> Option<Anchor> {
        let (index, offset) = self.project_anchor(original)?;

        // Prefer source text at or after the marker, including a visible run
        // later in the same block when publisher CSS removed a leading prefix.
        for block_index in index..self.source.blocks.len() {
            let block = &self.source.blocks[block_index];
            let lower_bound = if block_index == index { offset } else { 0 };
            let view = self.reading_flow.view(&self.source, block).ok()?;
            let ranges: Vec<_> = match view.runs() {
                Some(runs) => runs
                    .iter()
                    .filter(|run| run.painted && !run.source.is_empty())
                    .map(|run| run.source.clone())
                    .collect(),
                None => block
                    .styled_text
                    .iter()
                    .filter(|run| {
                        self.reading_flow
                            .element(run.element)
                            .is_ok_and(|flow| !flow.suppressed && flow.painted)
                    })
                    .map(|run| run.bytes.clone())
                    .collect(),
            };
            for range in ranges {
                let floor = range.start.max(lower_bound);
                if floor >= range.end {
                    continue;
                }
                let Some(candidate_offset) = block
                    .text
                    .get(floor..range.end)
                    .and_then(|text| text.char_indices().next().map(|(at, _)| floor + at))
                else {
                    continue;
                };
                let candidate = self.anchor(block.key, candidate_offset);
                if self.page_for(&candidate).is_some() {
                    return Some(candidate);
                }
            }
        }

        // A trailing pagebreak has no next text. Use the nearest preceding
        // painted character, matching KOReader's previousVisibleText fallback.
        for block_index in (0..=index).rev() {
            let block = &self.source.blocks[block_index];
            let upper_bound = if block_index == index {
                offset
            } else {
                block.text.len()
            };
            let view = self.reading_flow.view(&self.source, block).ok()?;
            let ranges: Vec<_> = match view.runs() {
                Some(runs) => runs
                    .iter()
                    .filter(|run| run.painted && !run.source.is_empty())
                    .map(|run| run.source.clone())
                    .collect(),
                None => block
                    .styled_text
                    .iter()
                    .filter(|run| {
                        self.reading_flow
                            .element(run.element)
                            .is_ok_and(|flow| !flow.suppressed && flow.painted)
                    })
                    .map(|run| run.bytes.clone())
                    .collect(),
            };
            for range in ranges.into_iter().rev() {
                let ceiling = range.end.min(upper_bound);
                let candidate_offset = block.text.get(range.start..ceiling).and_then(|text| {
                    text.char_indices()
                        .next_back()
                        .map(|(at, _)| range.start + at)
                });
                let Some(candidate_offset) = candidate_offset else {
                    continue;
                };
                let candidate = self.anchor(block.key, candidate_offset);
                if self.page_for(&candidate).is_some() {
                    return Some(candidate);
                }
            }
        }
        None
    }

    fn element_within(&self, mut element: usize, owner: usize) -> bool {
        loop {
            if element == owner {
                return true;
            }
            let Some(parent) = self
                .source
                .elements
                .get(element)
                .and_then(|item| item.parent)
            else {
                return false;
            };
            element = parent;
        }
    }

    /// Automatic layout/session restoration may snap a hidden source point to
    /// retained content. Explicit links/bookmarks use page_for and never snap.
    /// Saved user flow boundaries are source-order markers, not requests to
    /// reveal their current glyph. Re-resolve them when publisher CSS removes
    /// the marker's text, while retaining their original persisted anchors.
    pub(crate) fn flow_boundary_page(&self, anchor: &Anchor) -> Option<usize> {
        self.page_for(anchor).or_else(|| {
            (self.source_presentation(anchor) == Some(SourcePresentation::CssOmitted))
                .then(|| self.restore_anchor(anchor))
                .flatten()
                .and_then(|anchor| self.page_for(&anchor))
        })
    }

    pub(crate) fn restore_anchor(&self, anchor: &Anchor) -> Option<Anchor> {
        if self.page_for(anchor).is_some() {
            return Some(anchor.clone());
        }
        if self.source_presentation(anchor)? != SourcePresentation::CssOmitted {
            return None;
        }
        let (index, offset) = self.project_anchor(anchor)?;
        let block = &self.source.blocks[index];
        let view = self.reading_flow.view(&self.source, block).ok()?;
        for affinity in [
            flow_projection::Affinity::After,
            flow_projection::Affinity::Before,
        ] {
            if let Some(at) = view
                .project_nearest(offset, affinity)
                .and_then(|at| view.source_offset(at, affinity))
            {
                let candidate = self.anchor(block.key, at);
                if self.page_for(&candidate).is_some() {
                    return Some(candidate);
                }
            }
        }
        for distance in 1..self.source.blocks.len() {
            for candidate_index in [
                index
                    .checked_add(distance)
                    .filter(|i| *i < self.source.blocks.len()),
                index.checked_sub(distance),
            ]
            .into_iter()
            .flatten()
            {
                let block = &self.source.blocks[candidate_index];
                let view = self.reading_flow.view(&self.source, block).ok()?;
                if let Some(offset) = view.source_offset(0, flow_projection::Affinity::After) {
                    let candidate = self.anchor(block.key, offset);
                    if self.page_for(&candidate).is_some() {
                        return Some(candidate);
                    }
                }
                if let Renderer::Parley(renderer) = &self.renderer {
                    for ordinal in 0..block.images.len() {
                        if renderer.image_top(block.key, ordinal).is_some() {
                            let mut candidate = self.anchor(block.key, 0);
                            candidate.illustration = Some(ordinal);
                            return Some(candidate);
                        }
                    }
                }
            }
        }
        None
    }

    pub fn page_for(&self, anchor: &Anchor) -> Option<usize> {
        self.anchor_top(anchor).map(|top| self.pages.page(top))
    }

    pub(crate) fn anchor_top(&self, anchor: &Anchor) -> Option<f32> {
        let (index, offset) = self.project_anchor(anchor)?;
        let block = &self.source.blocks[index];
        if self.source_presentation(anchor) == Some(SourcePresentation::CssOmitted) {
            return None;
        }
        if let Some(ordinal) = anchor.illustration {
            if offset != 0 || ordinal >= block.images.len() {
                return None;
            }
            if let Renderer::Parley(renderer) = &self.renderer {
                return renderer.image_top(block.key, ordinal);
            }
            if !block.text.is_empty() {
                return None;
            }
            return self
                .lines
                .iter()
                .filter(|line| line.block == block.key && line.bytes.is_empty())
                .nth(ordinal)
                .map(|line| line.top);
        }
        let exact = self
            .lines
            .iter()
            .find(|line| line.block == block.key && line.contains_source(offset))
            .map(|line| line.top);
        exact.or_else(|| {
            if offset != block.text.len() {
                return None;
            }
            if block.text.is_empty() {
                self.lines
                    .iter()
                    .find(|line| line.block == block.key)
                    .map(|line| line.top)
            } else {
                self.lines
                    .iter()
                    .rev()
                    .find(|line| line.block == block.key && !line.bytes.is_empty())
                    .map(|line| line.top)
            }
        })
    }

    fn project_anchor(&self, anchor: &Anchor) -> Option<(usize, usize)> {
        if self.book != anchor.book || self.chapter != anchor.chapter {
            return None;
        }
        let blocks = &self.source.blocks;
        let key = self.source.current_key(anchor.block, anchor.revision)?;
        let index = blocks.binary_search_by_key(&key, |block| block.key).ok()?;
        let offset = self
            .source
            .project_offset(&blocks[index], anchor.revision, anchor.offset)?;
        Some((index, offset))
    }

    pub fn page_anchor(&self, page: usize) -> Option<Anchor> {
        self.anchor_in(self.pages.range(page)?)
    }

    pub(crate) fn anchor_in(&self, range: Range<f32>) -> Option<Anchor> {
        self.lines
            .iter()
            .find(|line| {
                !line.bytes.is_empty() && line.bottom > range.start && line.top < range.end
            })
            .or_else(|| {
                self.lines
                    .iter()
                    .find(|line| line.bottom > range.start && line.top < range.end)
            })
            .map(|line| {
                let mut anchor = self.anchor(line.block, line.bytes.start);
                if line.bytes.is_empty()
                    && let Renderer::Parley(renderer) = &self.renderer
                    && let Some(ordinal) = renderer.image_ordinal(line.block, line.top)
                {
                    anchor.offset = 0;
                    if ordinal != 0
                        || self
                            .source
                            .blocks
                            .iter()
                            .find(|block| block.key == line.block)
                            .is_some_and(|block| !block.text.is_empty())
                    {
                        anchor.illustration = Some(ordinal);
                    }
                }
                anchor
            })
    }

    #[cfg(feature = "kindle")]
    pub(crate) fn page_range(&self, page: usize) -> Range<f32> {
        self.pages.range(page).unwrap_or_default()
    }

    #[cfg(feature = "kindle")]
    pub(crate) fn scroll_end(&self) -> f32 {
        (*self.pages.bounds.last().unwrap_or(&0.0) - self.settings.text_height() as f32).max(0.0)
    }

    #[cfg(feature = "kindle")]
    pub(crate) fn scroll_range(&self, top: f32) -> Range<f32> {
        top..(top + self.settings.text_height() as f32)
    }

    #[cfg(feature = "kindle")]
    pub(crate) fn page_at(&self, top: f32) -> usize {
        self.pages.page(top)
    }

    /// Number of non-whitespace source characters represented by a rendered line.
    pub fn coverage(&self) -> (usize, usize) {
        let mut seen = std::collections::BTreeMap::<usize, Vec<Range<usize>>>::new();
        for line in &self.lines {
            seen.entry(line.block)
                .or_default()
                .extend(line.source_spans().iter().cloned());
        }
        self.source
            .blocks
            .iter()
            .fold((0, 0), |(covered, total), block| {
                let spans = seen.entry(block.key).or_default();
                spans.sort_unstable_by_key(|span| span.start);
                let mut visible = spans.iter().peekable();
                let mut visible_end = 0;
                block
                    .text
                    .char_indices()
                    .filter(|(_, c)| !c.is_whitespace())
                    .fold((covered, total), |(covered, total), (index, _)| {
                        while visible.peek().is_some_and(|span| span.start <= index) {
                            visible_end = visible_end.max(visible.next().unwrap().end);
                        }
                        (covered + usize::from(index < visible_end), total + 1)
                    })
            })
    }

    pub fn missing_glyphs(&self) -> usize {
        self.lines.iter().map(|line| line.missing_glyphs).sum()
    }

    pub fn capture(&mut self, page: usize) -> Result<Vec<u8>> {
        let range = self.pages.range(page).ok_or("That page is unavailable")?;
        self.capture_range(range)
    }

    pub(crate) fn capture_range(&mut self, range: Range<f32>) -> Result<Vec<u8>> {
        if range.start == range.end
            && self.settings.empty_break_mode.expands()
            && matches!(self.renderer, Renderer::Parley(_))
        {
            return Ok(vec![
                255;
                (self.settings.width * self.settings.height) as usize
            ]);
        }
        match &mut self.renderer {
            Renderer::Blitz(layout) => layout.capture(range, self.settings),
            Renderer::Parley(layout) => layout.capture(range, self.settings),
            Renderer::Vertical(layout) => layout.capture(range, self.settings),
        }
    }

    fn anchor(&self, block: usize, offset: usize) -> Anchor {
        Anchor {
            illustration: None,
            book: self.book,
            chapter: self.chapter.clone(),
            revision: self.source.revision,
            block,
            offset,
        }
    }
}

fn revision(text: &str) -> u64 {
    revision_bytes(text.as_bytes())
}

fn revision_bytes(bytes: &[u8]) -> u64 {
    bytes
        .iter()
        .copied()
        .fold(0xcbf29ce484222325_u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
        })
}

fn font_context(paths: &[PathBuf]) -> Result<FontContext> {
    font_context_with_firmware(paths, Path::new("/usr/java/lib/fonts"))
}

fn font_context_with_firmware(paths: &[PathBuf], firmware: &Path) -> Result<FontContext> {
    font_context_with_font_data(paths, firmware, |path| {
        let metadata = fs::metadata(path)?;
        if metadata.len() > 64 * 1024 * 1024 {
            return Err("A reading font exceeds the size limit".into());
        }
        Ok(parley::fontique::Blob::new(std::sync::Arc::new(fs::read(
            path,
        )?)))
    })
}

fn font_context_with_font_data(
    paths: &[PathBuf],
    firmware: &Path,
    mut read: impl FnMut(&Path) -> Result<parley::fontique::Blob<u8>>,
) -> Result<FontContext> {
    if paths.is_empty() {
        return Err("Provide at least one reading font".into());
    }
    let mut fonts = FontContext::default();
    let mut families = Vec::new();
    for path in paths {
        if path.parent() == Some(firmware) && path.is_file() {
            if fs::metadata(path)?.len() > 64 * 1024 * 1024 {
                return Err("A reading font exceeds the size limit".into());
            }
            // The firmware volume stays mounted when books and user fonts are removed
            // over USB. Fontique can safely map these fonts on demand without a heap
            // copy of the whole fallback stack on every chapter open.
            let known = fonts.collection.family_ids().collect::<Vec<_>>();
            fonts.collection.load_fonts_from_paths([path]);
            families.extend(
                fonts
                    .collection
                    .family_ids()
                    .filter(|id| !known.contains(id)),
            );
            continue;
        }
        let data = read(path)?;
        families.extend(
            fonts
                .collection
                .register_fonts(data, None)
                .into_iter()
                .map(|(id, _)| id),
        );
    }
    if families.is_empty() {
        return Err("No valid font was loaded".into());
    }
    fonts
        .collection
        .set_generic_families(GenericFamily::Serif, families.iter().copied());
    fonts
        .collection
        .set_generic_families(GenericFamily::SansSerif, families.iter().copied());
    Ok(fonts)
}

#[cfg(test)]
mod firmware_font_tests {
    use super::*;
    use parley::{
        Layout, LayoutContext, PositionedLayoutItem, StyleProperty, fontique::SourceKind,
    };

    fn missing(fonts: &mut FontContext, text: &str) -> usize {
        let mut context = LayoutContext::new();
        let mut builder = context.ranged_builder(fonts, text, 1.0, true);
        builder.push_default(StyleProperty::FontFamily(GenericFamily::Serif.into()));
        let mut layout: Layout<()> = builder.build(text);
        layout.break_all_lines(None);
        layout
            .lines()
            .flat_map(|line| line.items())
            .map(|item| match item {
                PositionedLayoutItem::GlyphRun(run) => {
                    run.positioned_glyphs().filter(|g| g.id == 0).count()
                }
                PositionedLayoutItem::InlineBox(_) => 0,
            })
            .sum()
    }

    #[test]
    fn file_backed_fallback_preserves_priority_and_shapes_missing_characters() {
        let first = test_fixture("fonts/Silkscreen-Regular.ttf");
        let fallback = test_fixture("fonts/NotoSans-Regular.ttf");
        let firmware = fallback.parent().unwrap();
        let paths = [first, fallback.clone()];
        let mut primary = font_context_with_firmware(&paths[..1], firmware).unwrap();
        assert_eq!(missing(&mut primary, "AΩ"), 1);
        let mut fonts = font_context_with_firmware(&paths, firmware).unwrap();
        let stack: Vec<_> = fonts
            .collection
            .generic_families(GenericFamily::Serif)
            .collect();
        assert_eq!(stack.len(), 2);
        for (id, expected_path) in stack.into_iter().zip(&paths) {
            let family = fonts.collection.family(id).unwrap();
            assert!(family.fonts().iter().any(|font|
                matches!(font.source().kind(), SourceKind::Path(path) if path.as_ref() == expected_path)));
        }
        assert_eq!(missing(&mut fonts, "AΩ"), 0);
        // Neither test font assigns a glyph to this private-use code point.
        assert_eq!(missing(&mut fonts, "\u{F0000}"), 1);
    }

    #[test]
    fn bundled_koreader_symbol_subset_covers_seven_observed_private_use_symbols() {
        let primary = test_fixture("fonts/NotoSans-Regular.ttf");
        let symbols = test_symbol_font();
        let paths = [primary, symbols];
        let mut fonts = font_context_with_firmware(&paths, Path::new("/missing-firmware")).unwrap();
        // Eight private-use scalars were observed in original AZW3 002. The
        // exact KOReader Nerd Fonts subset includes seven and omits U+F0B9.
        let kindle_symbols = "\u{F028}\u{F029}\u{F03D}\u{F02D}\u{F02B}\u{F0D0}\u{F040}";
        for scalar in kindle_symbols.chars() {
            assert_eq!(
                missing(&mut fonts, &scalar.to_string()),
                0,
                "unsupported private-use scalar U+{:04X}",
                scalar as u32
            );
        }
        assert_eq!(missing(&mut fonts, kindle_symbols), 0);
        assert_eq!(missing(&mut fonts, &format!("A{kindle_symbols}Ω")), 0);
        assert_eq!(missing(&mut fonts, "\u{F0B9}"), 1);
    }
}

fn grayscale(rgba: &[u8]) -> impl Iterator<Item = u8> + '_ {
    rgba.as_chunks::<4>().0.iter().map(|p| {
        if p[3] == 0 {
            return 255;
        }
        let white = 255 - u32::from(p[3]);
        if p[0] == p[1] && p[1] == p[2] {
            return (white + u32::from(p[0])).min(255) as u8;
        }
        let value = 77 * u32::from(p[0]) + 150 * u32::from(p[1]) + 29 * u32::from(p[2]);
        (white + ((value + 128) >> 8)).min(255) as u8
    })
}

fn image_reader(bytes: &[u8]) -> Result<image::ImageReader<std::io::Cursor<&[u8]>>> {
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(comic::MAX_IMAGE_SIDE);
    limits.max_image_height = Some(comic::MAX_IMAGE_SIDE);
    limits.max_alloc = Some(24 * 1024 * 1024);
    reader.limits(limits);
    if !matches!(
        reader.format(),
        Some(image::ImageFormat::Png | image::ImageFormat::Jpeg)
    ) {
        return Err("The EPUB image format is outside the probe subset".into());
    }
    Ok(reader)
}

fn epub_raster_dimensions(bytes: &[u8]) -> Result<(u32, u32)> {
    if gif_image::has_signature(bytes) {
        return gif_image::dimensions(bytes);
    }
    let (width, height) = image_reader(bytes)?.into_dimensions()?;
    if !comic::image_dimensions_valid(width, height) {
        return Err("The EPUB image exceeds Reader's decoded image limit".into());
    }
    Ok((width, height))
}
