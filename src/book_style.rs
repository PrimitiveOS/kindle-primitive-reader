//! Publisher typography for the compact EPUB layout. The cascade handles
//! compound and descendant selectors, author order, inline styles, importance,
//! and inherited text properties. See https://www.w3.org/TR/css-cascade-5/.

#[cfg(test)]
mod base_stylesheet_tests;
mod base_stylesheets;
pub(crate) mod borders;
pub(crate) mod flow;
#[cfg(test)]
mod flow_tests;
mod media;
mod semantic_type;
use crate::float_layout::{Clear, Side};
use flow::{Display, Visibility};
mod user_css;
#[cfg(test)]
mod user_css_tests;
pub(crate) use user_css::{matches as user_css_matches, validate as validate_user_css};
#[cfg(test)]
mod dpi_tests;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use cssparser::{
    AtRuleParser, CowRcStr, DeclarationParser, Delimiter, ParseError, Parser, ParserState,
    QualifiedRuleParser, RuleBodyItemParser, RuleBodyParser, StyleSheetParser, Token,
    parse_important,
};
use parley::{
    Alignment, FontContext, FontFamily, FontStyle, FontWeight, GenericFamily, LineBreak, WordBreak,
    fontique::{Blob, FontInfoOverride},
};

use crate::epub::Book;
use crate::footnotes::in_page::Targets as FootnoteTargets;
use crate::source::{Element, Source, Stylesheet};

const MAX_SHEETS: usize = 8;
const MAX_IMPORTS: usize = 8;
const MAX_IMPORT_DEPTH: usize = 4;
const MAX_SHEET_BYTES: usize = 96 * 1024;
const MAX_CSS_BYTES: usize = 256 * 1024;
const MAX_INLINE_BYTES: usize = 4 * 1024;
const MAX_RULES: usize = 256;
const MAX_SELECTOR_BYTES: usize = 128;
const MAX_SELECTORS_PER_RULE: usize = 24;
const MAX_SELECTOR_STEPS: usize = 8;
#[cfg(feature = "kindle")]
mod embedded_inventory;
pub(crate) mod pseudo;
pub(crate) mod table_hints;
#[cfg(feature = "kindle")]
pub(crate) use embedded_inventory::MAX_INVENTORY_SHEETS;

const MAX_FACE_RULES: usize = 64;
const MAX_FONT_READS: usize = 6;
const MAX_FONT_BYTES: usize = 2 * 1024 * 1024;
// Four normal/italic/bold/bold-italic publication faces can be ~1.4 MiB each.
const MAX_TOTAL_FONT_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Family {
    Named(Arc<str>),
    Generic(GenericFamily),
}

impl Family {
    pub(crate) fn layout(&self) -> FontFamily<'_> {
        match self {
            Self::Named(name) => FontFamily::named(name),
            Self::Generic(family) => (*family).into(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WritingMode {
    Horizontal,
    VerticalRl,
    VerticalLr,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum BoxSizing {
    #[default]
    ContentBox,
    BorderBox,
}

#[derive(Clone, Copy)]
pub(crate) enum ImageLength {
    Auto,
    FitContent,
    Pixels(f32),
    /// Physical CSS units normalized to pixels at 96 DPI.
    Physical(f32),
    Fraction(f32),
    Em(f32),
    RootEm(f32),
}

impl ImageLength {
    pub(crate) fn css_length(self) -> Option<CssLength> {
        match self {
            Self::Auto | Self::FitContent => None,
            Self::Pixels(value) => Some(CssLength::Pixels(value)),
            Self::Physical(value) => Some(CssLength::Physical(value)),
            Self::Fraction(value) => Some(CssLength::Fraction(value)),
            Self::Em(value) => Some(CssLength::Em(value)),
            Self::RootEm(value) => Some(CssLength::RootEm(value)),
        }
    }

    fn for_dpi(self, dpi: u16) -> Self {
        match self {
            // CRE ignores physical image constraints when render DPI is off.
            Self::Physical(_) if dpi == 0 => Self::Auto,
            Self::Pixels(px) | Self::Physical(px) if dpi != 0 && dpi != 96 => {
                Self::Pixels(px * f32::from(dpi) / 96.)
            }
            length => length,
        }
    }

    fn for_fitted_height_dpi(self, dpi: u16) -> Self {
        match self {
            // The fitted block resolver treats physical units as an explicit
            // zero at DPI off and truncates their converted value otherwise.
            Self::Physical(px) => Self::Physical(px * f32::from(dpi) / 96.0),
            value => value.for_dpi(dpi),
        }
    }

    pub(crate) fn pixels(self, available: u32, font_size: f32) -> Option<f32> {
        match self {
            Self::Auto | Self::FitContent => None,
            Self::Pixels(px) | Self::Physical(px) => Some(px),
            Self::Fraction(ratio) => Some(ratio * available as f32),
            Self::Em(scale) | Self::RootEm(scale) => Some(scale * font_size),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum CssLength {
    Em(f32),
    RootEm(f32),
    ComputedEm {
        value: f32,
        font_scale: f32,
    },
    Pixels(f32),
    /// Keep the original 96-DPI float conversion and its physical-unit identity.
    Physical(f32),
    Fraction(f32),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum LineHeightLength {
    Relative(f32),
    DefaultFont(f32),
    Pixels(f32),
}

/// A length line-height computed against its declaring font.
/// Keeping that font's scale preserves CSS inheritance across size changes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ComputedLineHeight {
    pub(crate) value: LineHeightLength,
    pub(crate) font_scale: f32,
    pub(crate) allow_small: bool,
}

impl ComputedLineHeight {
    pub(crate) fn pixels(self, settings: crate::Settings) -> f32 {
        let mut font = settings.styled_font(self.font_scale);
        if self.allow_small {
            font = font.min((settings.font_size * self.font_scale).max(4.0));
        }
        let minimum = if settings.line_spacing_percent.is_some() {
            0.5
        } else {
            1.0
        };
        let ratio = match self.value {
            LineHeightLength::Relative(factor) => factor,
            LineHeightLength::DefaultFont(factor) => factor * settings.styled_font(1.0) / font,
            LineHeightLength::Pixels(px) => {
                CssLength::Pixels(px)
                    .for_dpi(settings.render_dpi)
                    .pixels(0, font)
                    / font
            }
        };
        font * (ratio * settings.line_height_scale()).clamp(minimum, 3.0)
    }
}

impl Default for CssLength {
    fn default() -> Self {
        Self::Pixels(0.0)
    }
}

#[derive(Clone, Copy)]
pub(crate) enum BoxMargin {
    Auto,
    Length(CssLength),
}

impl Default for BoxMargin {
    fn default() -> Self {
        Self::Length(CssLength::default())
    }
}

impl BoxMargin {
    fn length(self) -> CssLength {
        match self {
            Self::Auto => CssLength::default(),
            Self::Length(length) => length,
        }
    }
}

impl CssLength {
    pub(crate) fn for_dpi(self, dpi: u16) -> Self {
        match self {
            Self::Physical(_) if dpi == 0 => Self::Pixels(0.),
            Self::Pixels(px) | Self::Physical(px) if dpi != 0 && dpi != 96 => {
                Self::Pixels(px * f32::from(dpi) / 96.)
            }
            length => length,
        }
    }

    fn computed(self, font: f32, root_font: f32) -> Self {
        match self {
            Self::Em(value) => Self::ComputedEm {
                value,
                font_scale: font,
            },
            Self::RootEm(value) => Self::ComputedEm {
                value,
                font_scale: root_font,
            },
            length => length,
        }
    }

    pub fn pixels(self, width: u32, reader_font: f32) -> f32 {
        match self {
            Self::Em(em) | Self::RootEm(em) => em * reader_font,
            Self::ComputedEm { value, font_scale } => value * font_scale * reader_font,
            Self::Pixels(px) | Self::Physical(px) => px,
            Self::Fraction(ratio) => ratio * width as f32,
        }
    }

    // CREngine stores CSS numbers as signed units of 1/256 and uses integer
    // font sizes and container widths when converting a block edge to pixels.
    pub fn box_pixels(self, width: u32, reader_font: f32) -> f32 {
        let fixed = |value: f32| (f64::from(value) * 256.).round() / 256.;
        let pixels = match self {
            Self::Pixels(px) | Self::Physical(px) => (fixed(px) + 127. / 256.).floor(),
            Self::Fraction(ratio) => (f64::from(width) * fixed(ratio * 100.) / 100.).floor(),
            Self::Em(value) | Self::RootEm(value) => {
                (fixed(value) * f64::from(reader_font).floor()).floor()
            }
            Self::ComputedEm { value, font_scale } => {
                (fixed(value) * f64::from(font_scale * reader_font).floor()).floor()
            }
        };
        pixels as f32
    }
}

#[derive(Clone, Copy)]
pub(crate) struct TextIndent {
    length: CssLength,
    pub hanging: bool,
}

impl Default for TextIndent {
    fn default() -> Self {
        Self {
            length: CssLength::Pixels(0.0),
            hanging: false,
        }
    }
}

impl TextIndent {
    // Resolve font-relative values at the declaring element so descendants that
    // change font size inherit the computed length. Percentages stay relative
    // to the width of the block being laid out.
    fn computed(self, font: f32, root_font: f32) -> Self {
        Self {
            length: self.length.computed(font, root_font),
            ..self
        }
    }

    pub fn pixels(self, width: u32, reader_font: f32) -> f32 {
        self.length.pixels(width, reader_font)
    }

    pub fn box_pixels(self, width: u32, reader_font: f32) -> f32 {
        self.length.box_pixels(width, reader_font)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Hyphens {
    None,
    Manual,
    Auto,
}

/// Source spans are flattened into paragraph byte ranges. Carry the summed parent-relative
/// shifts so emphasis or a link inside a script stays on the script's baseline.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct TextRise {
    height: f32,
    descent: f32,
}

impl TextRise {
    pub(crate) fn from_block_at(
        self,
        block: &Style,
        settings: crate::Settings,
    ) -> parley::VerticalAlign {
        if settings.logical_font_size.is_none() {
            return self.from_block(block);
        }
        let actual_scale = settings.styled_font(block.size_scale) / settings.font_size;
        parley::VerticalAlign::new(
            parley::AlignmentBaseline::Baseline,
            parley::BaselineShift::FontMetrics {
                height: (self.height - block.text_rise.height) / actual_scale,
                descent: (self.descent - block.text_rise.descent) / actual_scale,
            },
        )
    }
    pub(crate) fn from_block(self, block: &Style) -> parley::VerticalAlign {
        parley::VerticalAlign::new(
            parley::AlignmentBaseline::Baseline,
            parley::BaselineShift::FontMetrics {
                height: (self.height - block.text_rise.height) / block.size_scale,
                descent: (self.descent - block.text_rise.descent) / block.size_scale,
            },
        )
    }
}

/// A painted text run can carry more than one publisher line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct TextDecoration {
    pub underline: bool,
    pub overline: bool,
    pub strike: bool,
}

impl TextDecoration {
    fn including(self, other: Self) -> Self {
        Self {
            underline: self.underline || other.underline,
            overline: self.overline || other.overline,
            strike: self.strike || other.strike,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum VariantCaps {
    #[default]
    Normal,
    Small,
    AllSmall,
    Petite,
    AllPetite,
    Unicase,
    Titling,
}

impl VariantCaps {
    fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "normal" => Self::Normal,
            "small-caps" => Self::Small,
            "all-small-caps" => Self::AllSmall,
            "petite-caps" => Self::Petite,
            "all-petite-caps" => Self::AllPetite,
            "unicase" => Self::Unicase,
            "titling-caps" => Self::Titling,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct VariantNumeric(pub(crate) u8);

impl VariantNumeric {
    fn bit(value: &str) -> Option<u8> {
        Some(match value {
            "lining-nums" => 1 << 0,
            "oldstyle-nums" => 1 << 1,
            "proportional-nums" => 1 << 2,
            "tabular-nums" => 1 << 3,
            "slashed-zero" => 1 << 4,
            "ordinal" => 1 << 5,
            "diagonal-fractions" => 1 << 6,
            "stacked-fractions" => 1 << 7,
            _ => return None,
        })
    }

    fn group(bit: u8) -> u8 {
        match bit {
            1 | 2 => 0b0000_0011,
            4 | 8 => 0b0000_1100,
            64 | 128 => 0b1100_0000,
            _ => bit,
        }
    }
}

fn parse_numeric_variant(input: &mut Parser<'_>) -> CssResult<VariantNumeric> {
    let first = input.expect_ident()?.to_ascii_lowercase();
    if first == "normal" {
        input.expect_exhausted()?;
        return Ok(VariantNumeric::default());
    }
    let mut bits = VariantNumeric::bit(&first).ok_or_else(ParseError::unexpected_token)?;
    while !input.is_exhausted() {
        let value = input.expect_ident()?.to_ascii_lowercase();
        let bit = VariantNumeric::bit(&value).ok_or_else(ParseError::unexpected_token)?;
        let group = VariantNumeric::group(bit);
        if bits & group != 0 {
            return Err(ParseError::unexpected_token());
        }
        bits |= bit;
    }
    Ok(VariantNumeric(bits))
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct VariantEastAsian(pub(crate) u8);

impl VariantEastAsian {
    fn bit(value: &str) -> Option<u8> {
        Some(match value {
            "jis78" => 1 << 0,
            "jis83" => 1 << 1,
            "jis04" => 1 << 2,
            "simplified" => 1 << 3,
            "traditional" => 1 << 4,
            "full-width" => 1 << 5,
            "proportional-width" => 1 << 6,
            "ruby" => 1 << 7,
            _ => return None,
        })
    }

    fn group(bit: u8) -> u8 {
        if bit <= 16 {
            0b0001_1111
        } else if bit <= 64 {
            0b0110_0000
        } else {
            bit
        }
    }
}

fn parse_east_asian_variant(input: &mut Parser<'_>) -> CssResult<VariantEastAsian> {
    let first = input.expect_ident()?.to_ascii_lowercase();
    if first == "normal" {
        input.expect_exhausted()?;
        return Ok(VariantEastAsian::default());
    }
    let mut bits = VariantEastAsian::bit(&first).ok_or_else(ParseError::unexpected_token)?;
    while !input.is_exhausted() {
        let value = input.expect_ident()?.to_ascii_lowercase();
        let bit = VariantEastAsian::bit(&value).ok_or_else(ParseError::unexpected_token)?;
        let group = VariantEastAsian::group(bit);
        if bits & group != 0 {
            return Err(ParseError::unexpected_token());
        }
        bits |= bit;
    }
    Ok(VariantEastAsian(bits))
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum VariantPosition {
    #[default]
    Normal,
    Super,
    Sub,
}

impl VariantPosition {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "normal" => Some(Self::Normal),
            "super" => Some(Self::Super),
            "sub" => Some(Self::Sub),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum TextTransform {
    #[default]
    None,
    Uppercase,
    Lowercase,
    Capitalize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum RubyPosition {
    #[default]
    Alternate,
    Over,
    Under,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum WhiteSpace {
    #[default]
    Normal,
    NoWrap,
    Pre,
    PreWrap,
    PreLine,
    BreakSpaces,
}

impl WhiteSpace {
    fn nowrap(self) -> bool {
        matches!(self, Self::NoWrap | Self::Pre)
    }

    fn preserves_spaces(self) -> bool {
        matches!(self, Self::Pre | Self::PreWrap | Self::BreakSpaces)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct LigatureExtras {
    pub(crate) contextual: Option<bool>,
    pub(crate) discretionary: Option<bool>,
    pub(crate) historical: Option<bool>,
}

fn ligature_keyword(value: &str, common: &mut Option<bool>, extra: &mut LigatureExtras) -> bool {
    let (slot, enabled) = match value {
        "common-ligatures" => (common, true),
        "no-common-ligatures" => (common, false),
        "contextual" => (&mut extra.contextual, true),
        "no-contextual" => (&mut extra.contextual, false),
        "discretionary-ligatures" => (&mut extra.discretionary, true),
        "no-discretionary-ligatures" => (&mut extra.discretionary, false),
        "historical-ligatures" => (&mut extra.historical, true),
        "no-historical-ligatures" => (&mut extra.historical, false),
        _ => return false,
    };
    if slot.is_some() {
        return false;
    }
    *slot = Some(enabled);
    true
}

fn parse_ligatures(input: &mut Parser<'_>) -> CssResult<(bool, LigatureExtras)> {
    let first = input.expect_ident()?.to_ascii_lowercase();
    let mut common = None;
    let mut extra = LigatureExtras::default();
    match first.as_str() {
        "normal" => input.expect_exhausted()?,
        "none" => {
            input.expect_exhausted()?;
            common = Some(false);
            extra = LigatureExtras {
                contextual: Some(false),
                discretionary: Some(false),
                historical: Some(false),
            };
        }
        _ => {
            if !ligature_keyword(&first, &mut common, &mut extra) {
                return Err(ParseError::unexpected_token());
            }
            while !input.is_exhausted() {
                let keyword = input.expect_ident()?.to_ascii_lowercase();
                if !ligature_keyword(&keyword, &mut common, &mut extra) {
                    return Err(ParseError::unexpected_token());
                }
            }
        }
    }
    Ok((common.unwrap_or(true), extra))
}

#[derive(Clone)]
pub(crate) struct Style {
    pub display: Display,
    pub visibility: Visibility,
    /// Effective projection masks, not a renderer/user-CSS support declaration.
    pub flow: crate::flow_projection::ElementFlow,
    pub font_family: Family,
    pub font_weights: Arc<HashMap<u64, f32>>,
    pub size_scale: f32,
    /// Computed publisher letter spacing, inherited independently of font size.
    pub(crate) letter_spacing: CssLength,
    /// Computed, nonnegative publisher or user word spacing.
    pub(crate) word_spacing: CssLength,
    pub(crate) allow_small_note_text: bool,
    pub text_rise: TextRise,
    pub decoration: TextDecoration,
    pub(crate) link_ink: Option<u8>,
    pub(crate) ignore_table_hints: bool,
    pub(crate) native_table_widths: bool,
    pub(crate) generated: Option<Arc<pseudo::Pair>>,
    pub(crate) first_letter: Option<Arc<pseudo::FirstLetter>>,
    pub(crate) first_line: Option<Arc<pseudo::FirstLine>>,
    pub page_breaks: crate::publisher_breaks::Rules,
    pub orphans: u8,
    pub widows: u8,
    pub font_weight: FontWeight,
    pub font_style: FontStyle,
    pub(crate) common_ligatures: bool,
    pub(crate) ligature_extras: LigatureExtras,
    pub(crate) variant_caps: VariantCaps,
    pub(crate) variant_numeric: VariantNumeric,
    pub(crate) variant_east_asian: VariantEastAsian,
    pub(crate) variant_position: VariantPosition,
    pub(crate) variant_historical: bool,
    /// Whether a ruby annotation has its own computed font-size declaration.
    pub(crate) ruby_size_explicit: bool,
    pub(crate) direction: parley::BaseDirection,
    pub line_height: Option<f32>,
    pub(crate) line_height_relative: Option<ComputedLineHeight>,
    pub(crate) normal_line_height_percent: Option<u8>,
    pub(crate) steady_line_heights: bool,
    pub text_indent: TextIndent,
    pub padding: [CssLength; 2],
    pub vertical_padding: [CssLength; 2],
    pub vertical_margin: [CssLength; 2],
    pub(crate) paragraph_edges: [bool; 2],
    pub(crate) cjk_tailored: bool,
    pub horizontal_margin: [BoxMargin; 2],
    pub float_side: Option<Side>,
    pub clear: Clear,
    pub border_collapse: bool,
    pub borders: Option<Arc<[borders::Border; 4]>>,
    pub border_spacing: [CssLength; 2],
    pub(crate) has_table_spacing: bool,
    pub(crate) caption_bottom: bool,
    pub background: Option<u8>,
    pub marker: crate::list_marker::MarkerKind,
    pub image_width: Option<ImageLength>,
    pub image_height: Option<ImageLength>,
    pub image_max_width: Option<ImageLength>,
    pub image_max_height: Option<ImageLength>,
    pub min_width: Option<CssLength>,
    pub min_height: Option<ImageLength>,
    pub box_sizing: BoxSizing,
    pub box_sizing_explicit: bool,
    pub image_align: parley::VerticalAlign,
    pub(crate) table_valign_explicit: bool,
    pub(crate) image_full_line: bool,
    pub alignment: Option<Alignment>,
    pub(crate) last_line_alignment: Option<Alignment>,
    pub(crate) text_transform: TextTransform,
    pub(crate) optical_sizing: bool,
    pub(crate) ruby_position: RubyPosition,
    pub line_break: Option<LineBreak>,
    /// CRE's private line-break mode, retained separately from standard CSS.
    pub(crate) loose_nonbreaking: bool,
    pub word_break: WordBreak,
    pub(crate) inline_nowrap: bool,
    pub(crate) white_space: WhiteSpace,
    pub hyphens: Hyphens,
    pub writing_mode: WritingMode,
    /// Whether spacing-sensitive rendering is requested by HTML pre or CSS.
    pub preformatted_spaces: bool,
    /// Publisher CSS is disabled and user CSS is strictly validated.
    pub(crate) simple_default_tab_stops: bool,
    /// Inherited numeric CSS tab size, measured in the block's spaces.
    pub(crate) tab_size_spaces: f32,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            display: Display::Inline,
            visibility: Visibility::Visible,
            flow: Default::default(),
            font_family: Family::Generic(GenericFamily::Serif),
            font_weights: Arc::default(),
            size_scale: 1.0,
            letter_spacing: CssLength::default(),
            word_spacing: CssLength::default(),
            allow_small_note_text: false,
            text_rise: TextRise::default(),
            decoration: TextDecoration::default(),
            link_ink: None,
            ignore_table_hints: false,
            native_table_widths: false,
            generated: None,
            first_letter: None,
            first_line: None,
            page_breaks: Default::default(),
            orphans: 1,
            widows: 1,
            font_weight: FontWeight::NORMAL,
            font_style: FontStyle::Normal,
            common_ligatures: true,
            ligature_extras: LigatureExtras::default(),
            variant_caps: VariantCaps::Normal,
            variant_numeric: VariantNumeric::default(),
            variant_east_asian: VariantEastAsian::default(),
            variant_position: VariantPosition::Normal,
            variant_historical: false,
            ruby_size_explicit: false,
            direction: parley::BaseDirection::Auto,
            line_height: None,
            line_height_relative: None,
            normal_line_height_percent: None,
            steady_line_heights: false,
            text_indent: TextIndent::default(),
            padding: Default::default(),
            vertical_padding: Default::default(),
            vertical_margin: Default::default(),
            paragraph_edges: [false; 2],
            cjk_tailored: false,
            horizontal_margin: Default::default(),
            float_side: None,
            clear: Clear::default(),
            border_collapse: false,
            borders: None,
            border_spacing: [CssLength::Pixels(1.); 2],
            has_table_spacing: false,
            caption_bottom: false,
            background: None,
            marker: crate::list_marker::MarkerKind::None,
            image_width: None,
            image_height: None,
            image_max_width: None,
            image_max_height: None,
            min_width: None,
            min_height: None,
            box_sizing: BoxSizing::default(),
            box_sizing_explicit: false,
            image_align: parley::VerticalAlign::BASELINE,
            table_valign_explicit: false,
            image_full_line: false,
            alignment: None,
            last_line_alignment: None,
            text_transform: TextTransform::None,
            optical_sizing: true,
            ruby_position: RubyPosition::Alternate,
            line_break: None,
            loose_nonbreaking: false,
            word_break: WordBreak::Normal,
            inline_nowrap: false,
            white_space: WhiteSpace::Normal,
            hyphens: Hyphens::Auto,
            writing_mode: WritingMode::Horizontal,
            preformatted_spaces: false,
            simple_default_tab_stops: false,
            tab_size_spaces: 8.0,
        }
    }
}

impl Style {
    pub(crate) fn font_pixels(&self, settings: crate::Settings) -> f32 {
        let size = settings.styled_font(self.size_scale);
        if self.allow_small_note_text {
            // A deliberately small note can contain an even smaller relative
            // script. Keep its scale below the ordinary text-size floor.
            size.min((settings.font_size * self.size_scale).max(4.0))
        } else {
            size
        }
    }
}

#[derive(Default)]
pub(crate) struct BookStyle {
    author_inline: bool,
    rules: Vec<Rule>,
    base_rules: Vec<Rule>,
    user_rules: Vec<Rule>,
    layout_tweaks: crate::user_styles::LayoutTweaks,
    families: HashMap<String, Arc<str>>,
    font_weights: Arc<HashMap<u64, f32>>,
    #[cfg(feature = "kindle")]
    registered_faces: HashMap<u64, Arc<str>>,
}

impl BookStyle {
    pub fn with_user_css(mut self, snapshot: &crate::user_styles::Snapshot) -> crate::Result<Self> {
        self.layout_tweaks = snapshot.layout_tweaks();
        // The built-in paragraph presets have a measured geometry path.
        // Keep arbitrary user CSS under its existing validation rules.
        for css in self.layout_tweaks.paragraph_rules() {
            let mut parser = Parser::new(css);
            for item in StyleSheetParser::new(&mut parser, &mut SheetParser).flatten() {
                if let SheetItem::Rule(rule) = item {
                    self.user_rules.push(rule);
                }
            }
        }
        self.user_rules.extend(user_css::parse(snapshot.css())?);
        Ok(self)
    }

    #[cfg(test)]
    pub fn load(book: &mut Book, chapter: &str, source: &Source, fonts: &mut FontContext) -> Self {
        Self::load_author(book, chapter, source, fonts, true)
    }

    fn load_author(
        book: &mut Book,
        chapter: &str,
        source: &Source,
        fonts: &mut FontContext,
        embedded_fonts: bool,
    ) -> Self {
        let (mut styled, faces) = Self::load_sheets(book, chapter, source);
        styled.families.extend(
            fonts
                .collection
                .family_names()
                .map(|name| (name.to_ascii_lowercase(), Arc::from(name))),
        );
        if embedded_fonts && !faces.is_empty() {
            styled.register_fonts(book, &faces, fonts);
        }
        styled
    }

    pub(crate) fn load_with_base(
        book: &mut Book,
        chapter: &str,
        source: &Source,
        fonts: &mut FontContext,
        settings: crate::Settings,
    ) -> Self {
        let mut styled = if settings.publisher_styles {
            Self::load_author(book, chapter, source, fonts, settings.embedded_fonts)
        } else {
            Self::default()
        };
        let selection = if settings.base_stylesheet == crate::BaseStylesheet::Auto {
            book.automatic_stylesheet()
        } else {
            settings.base_stylesheet
        };
        let stock_tables = matches!(
            settings.publisher_render_mode,
            Some(crate::PublisherRenderMode::Book | crate::PublisherRenderMode::Web)
        );
        styled.read_base_stylesheet(source, selection, stock_tables);
        styled
    }

    #[cfg(feature = "kindle")]
    pub(crate) fn publication_has_embedded_fonts(book: &mut Book, active: usize) -> bool {
        embedded_inventory::has_fonts(book, active)
    }

    #[cfg(feature = "kindle")]
    pub(crate) fn chapter_has_embedded_fonts(book: &mut Book, chapter: usize) -> bool {
        embedded_inventory::chapter_has_fonts(book, chapter)
    }

    #[cfg(feature = "kindle")]
    pub(crate) fn embedded_font_catalog(book: &mut Book, active: usize) -> Vec<Arc<str>> {
        embedded_inventory::catalog(book, active)
    }

    #[cfg(feature = "kindle")]
    pub(crate) fn registered_faces(&self) -> &HashMap<u64, Arc<str>> {
        &self.registered_faces
    }

    /// The exact same stylesheet selection, caps, cascade and inline handling as
    /// native layout, without FontContext or font-face resource registration.
    pub(crate) fn load_for_flow(
        book: &mut Book,
        chapter: &str,
        source: &Source,
        publisher_styles: bool,
        user: &crate::user_styles::Snapshot,
    ) -> crate::Result<Self> {
        let styled = if publisher_styles {
            Self::load_sheets(book, chapter, source).0
        } else {
            Self::default()
        };
        styled.with_user_css(user)
    }

    pub(crate) fn load_for_flow_with_base(
        book: &mut Book,
        chapter: &str,
        source: &Source,
        publisher_styles: bool,
        user: &crate::user_styles::Snapshot,
        selection: crate::BaseStylesheet,
    ) -> crate::Result<Self> {
        let mut styled = Self::load_for_flow(book, chapter, source, publisher_styles, user)?;
        let selection = if selection == crate::BaseStylesheet::Auto {
            book.automatic_stylesheet()
        } else {
            selection
        };
        styled.read_base_stylesheet(source, selection, false);
        Ok(styled)
    }

    fn load_sheets(book: &mut Book, chapter: &str, source: &Source) -> (Self, Vec<FontFace>) {
        let mut styled = Self {
            author_inline: true,
            ..Self::default()
        };
        let mut faces = Vec::new();
        let mut remaining = MAX_CSS_BYTES;
        let mut linked = 0;
        let mut inline = 0;
        let mut imports = 0;
        let mut active_imports = HashSet::new();
        let names = ChapterNames::new(&source.elements);
        for sheet in &source.stylesheets {
            match sheet {
                Stylesheet::Linked(href) if linked < MAX_SHEETS => {
                    linked += 1;
                    let Ok(path) = Book::resolve(chapter, href) else {
                        continue;
                    };
                    if !path
                        .rsplit_once('.')
                        .is_some_and(|(_, ext)| ext.eq_ignore_ascii_case("css"))
                    {
                        continue;
                    }
                    let Ok(bytes) = book.resource_limited(&path, MAX_SHEET_BYTES.min(remaining))
                    else {
                        continue;
                    };
                    if let Ok(text) = std::str::from_utf8(&bytes) {
                        active_imports.insert(path.clone());
                        styled.read_sheet(
                            book,
                            &path,
                            text,
                            &mut remaining,
                            &mut faces,
                            &names,
                            &mut imports,
                            &mut active_imports,
                            0,
                        );
                        active_imports.remove(&path);
                    }
                }
                Stylesheet::Inline(sheet) if inline < MAX_SHEETS => {
                    inline += 1;
                    styled.read_sheet(
                        book,
                        chapter,
                        sheet,
                        &mut remaining,
                        &mut faces,
                        &names,
                        &mut imports,
                        &mut active_imports,
                        0,
                    );
                }
                _ => {}
            }
        }
        (styled, faces)
    }

    fn read_base_stylesheet(
        &mut self,
        source: &Source,
        selection: crate::BaseStylesheet,
        stock_tables: bool,
    ) {
        let names = ChapterNames::new(&source.elements);
        for text in [
            base_stylesheets::css(selection),
            base_stylesheets::traditional_tables(selection, stock_tables),
        ] {
            if text.is_empty() {
                continue;
            }
            let mut parser = Parser::new(text);
            for rule in StyleSheetParser::new(&mut parser, &mut SheetParser)
                .take(MAX_RULES)
                .flatten()
            {
                if let SheetItem::Rule(mut rule) = rule
                    && self.base_rules.len() < MAX_RULES
                    && !rule.declarations.is_empty()
                {
                    rule.selectors.retain(|selector| names.possible(selector));
                    if !rule.selectors.is_empty() {
                        self.base_rules.push(rule);
                    }
                }
            }
        }
    }

    pub fn compute_for_dpi(&self, elements: &[Element], dpi: u16) -> Vec<Style> {
        self.scale_dpi(self.compute(elements), dpi)
    }

    pub(crate) fn compute_for_settings(
        &self,
        elements: &[Element],
        settings: crate::Settings,
    ) -> Vec<Style> {
        self.compute_with_footnotes(elements, settings, None)
    }

    pub(crate) fn compute_with_footnotes(
        &self,
        elements: &[Element],
        settings: crate::Settings,
        footnotes: Option<&FootnoteTargets>,
    ) -> Vec<Style> {
        let (mut styles, quotes) = self.compute_deferred_quotes(elements, settings, footnotes);
        let main = settings.typography.resolve(None);
        quotes.resolve(
            elements,
            &mut styles,
            main.as_ref().map_or("en", |tag| tag.as_str()),
            settings.typography.respect_embedded,
            &mut Default::default(),
        );
        styles
    }

    pub(crate) fn compute_deferred_quotes(
        &self,
        elements: &[Element],
        settings: crate::Settings,
        footnotes: Option<&FootnoteTargets>,
    ) -> (Vec<Style>, pseudo::quotes::Plan<'_>) {
        let (styles, mut quotes) = self.compute_with_font(elements, Some(settings), footnotes);
        quotes.scale_dpi(settings.render_dpi);
        (self.scale_dpi(styles, settings.render_dpi), quotes)
    }

    fn scale_dpi(&self, mut styles: Vec<Style>, dpi: u16) -> Vec<Style> {
        if dpi == 96 {
            return styles;
        }
        for style in &mut styles {
            style.text_indent.length = style.text_indent.length.for_dpi(dpi);
            style.letter_spacing = style.letter_spacing.for_dpi(dpi);
            style.word_spacing = style.word_spacing.for_dpi(dpi);
            if let Some(letter) = &mut style.first_letter {
                Arc::make_mut(letter).set_dpi(dpi);
            }
            if let Some(line) = &mut style.first_line {
                Arc::make_mut(line).scale_dpi(dpi);
            }
            if let Some(generated) = &mut style.generated {
                for part in Arc::make_mut(generated).iter_mut().flatten() {
                    part.style.letter_spacing = part.style.letter_spacing.for_dpi(dpi);
                }
            }
            for edge in style
                .padding
                .iter_mut()
                .chain(style.vertical_padding.iter_mut())
                .chain(style.vertical_margin.iter_mut())
                .chain(style.border_spacing.iter_mut())
            {
                *edge = edge.for_dpi(dpi);
            }
            if let Some(borders) = &mut style.borders {
                for border in Arc::make_mut(borders) {
                    border.width = border.width.for_dpi(dpi);
                }
            }
            style.image_width = style.image_width.map(|value| value.for_dpi(dpi));
            let height_for_dpi = if matches!(style.image_width, Some(ImageLength::FitContent)) {
                ImageLength::for_fitted_height_dpi
            } else {
                ImageLength::for_dpi
            };
            style.image_height = style.image_height.map(|value| height_for_dpi(value, dpi));
            style.image_max_width = style.image_max_width.map(|value| value.for_dpi(dpi));
            style.image_max_height = style
                .image_max_height
                .map(|value| height_for_dpi(value, dpi));
            style.min_width = style.min_width.map(|value| value.for_dpi(dpi));
            style.min_height = style.min_height.map(|value| height_for_dpi(value, dpi));
        }
        styles
    }

    pub fn compute(&self, elements: &[Element]) -> Vec<Style> {
        let (mut styles, quotes) = self.compute_with_font(elements, None, None);
        quotes.resolve(elements, &mut styles, "en", true, &mut Default::default());
        styles
    }

    fn compute_with_font(
        &self,
        elements: &[Element],
        settings: Option<crate::Settings>,
        footnotes: Option<&FootnoteTargets>,
    ) -> (Vec<Style>, pseudo::quotes::Plan<'_>) {
        let structural_borders = settings.is_some_and(|config| {
            config.empty_break_mode.expands()
                || config.empty_break_geometry
                    && config.empty_break_mode != crate::EmptyBreakMode::Current
        });
        let publisher_block_height = settings.is_some_and(|config| config.publisher_block_height);
        let nested_margin_collapse = settings.is_some_and(|config| {
            matches!(
                config.block_margins,
                crate::BlockMarginMode::Collapse | crate::BlockMarginMode::CollapseNegative
            )
        });
        let keep_group_geometry =
            settings.is_some_and(|config| config.relax_oversized_publisher_keeps);
        let opening_break_geometry =
            settings.is_some_and(|config| config.prefer_publisher_opening_breaks);
        let constrained_width = settings.is_some_and(|config| {
            config.block_width == crate::BlockWidthMode::PublisherWithConstraints
        });
        let fit_content =
            settings.is_some_and(|config| config.fit_content != crate::FitContentMode::Ignore);
        let publisher_wrapper_width = constrained_width
            || settings.is_some_and(|config| {
                config.block_width == crate::BlockWidthMode::PublisherWithWrappers
            });
        let publisher_horizontal_margins = settings.is_some_and(|config| {
            config.publisher_horizontal_margins || config.publisher_signed_horizontal_margins
        }) || constrained_width;
        let forced_margin_geometry = nested_margin_collapse
            && settings.is_some_and(|config| config.preserve_forced_page_margins);
        let logical = settings.filter(|settings| settings.logical_font_size.is_some());
        let relative_font = |scale: f32| {
            logical.map_or_else(
                || scale.clamp(0.125, 8.0),
                |settings| settings.styled_font(scale) / settings.font_size,
            )
        };
        let defaults = Style {
            normal_line_height_percent: self.layout_tweaks.normal_line_height_percent,
            steady_line_heights: self.layout_tweaks.steady_line_heights,
            font_weights: self.font_weights.clone(),
            cjk_tailored: self.layout_tweaks.cjk_tailored,
            ignore_table_hints: !self.author_inline || self.layout_tweaks.ignore_table_hints,
            native_table_widths: self.layout_tweaks.native_table_widths,
            simple_default_tab_stops: !self.author_inline,
            ..Style::default()
        };
        let mut pseudo_rules = pseudo::Rules::new(self);
        let mut result: Vec<Style> = Vec::with_capacity(elements.len());
        for (index, element) in elements.iter().enumerate() {
            let parent = element.parent.map_or(&defaults, |parent| &result[parent]);
            if element.selector_only {
                let mut style = defaults.clone();
                style.display = Display::native(&element.tag);
                style.visibility = parent.visibility;
                style.flow = flow::resolve(parent.flow, style.display, style.visibility);
                result.push(style);
                continue;
            }
            let mut style = parent.clone();
            style.ruby_size_explicit = false;
            if let Some(direction) = element.direction_hint {
                style.direction = direction;
            }
            style.generated = None;
            style.first_letter = None;
            style.first_line = None;
            if element.tag == "a" {
                style.link_ink = self.layout_tweaks.link_ink.map(|ink| ink as u8);
            }
            if element.tag == "pre" {
                // Native HTML default; later publisher or user declarations
                // still override all three whitespace properties together.
                style.white_space = WhiteSpace::Pre;
                style.preformatted_spaces = true;
                style.inline_nowrap = true;
            }
            // Display is not inherited. Visibility is inherited independently of
            // ancestor suppression, so a visible descendant can restore paint.
            style.display = Display::native(&element.tag);
            // A parent's background paints its box. It is not a text property.
            style.background = None;
            style.borders = None;
            style.page_breaks = Default::default();
            style.padding = Default::default();
            style.vertical_padding = Default::default();
            style.vertical_margin = Default::default();
            style.paragraph_edges = if element.tag == "p" {
                let follows_p = element
                    .previous_sibling
                    .is_some_and(|sibling| elements[sibling].tag == "p");
                self.layout_tweaks.paragraph_edges(follows_p)
            } else {
                [false; 2]
            };
            style.horizontal_margin = Default::default();
            style.float_side = None;
            style.clear = Clear::default();
            if let Some(kind) = element.list_kind {
                style.marker = kind;
            }
            style.image_width = None;
            style.image_height = None;
            style.image_max_width = None;
            style.image_max_height = None;
            style.min_width = None;
            style.min_height = None;
            style.box_sizing = BoxSizing::default();
            style.box_sizing_explicit = false;
            let table_valign_element = matches!(
                element.tag.as_str(),
                "td" | "th" | "tr" | "thead" | "tbody" | "tfoot"
            );
            let inherits_table_valign = matches!(element.tag.as_str(), "td" | "th" | "tr")
                && element.parent.is_some_and(|parent| {
                    matches!(
                        elements[parent].tag.as_str(),
                        "tr" | "thead" | "tbody" | "tfoot"
                    )
                });
            style.image_align = if inherits_table_valign {
                parent.image_align
            } else {
                parley::VerticalAlign::BASELINE
            };
            style.table_valign_explicit = inherits_table_valign && parent.table_valign_explicit;
            style.image_full_line = self.layout_tweaks.full_width_images && element.tag == "img";
            let mut decoration = TextDecoration {
                underline: matches!(element.tag.as_str(), "u" | "ins")
                    || (element.tag == "a" && element.has_href),
                strike: matches!(element.tag.as_str(), "s" | "strike" | "del"),
                ..TextDecoration::default()
            };
            match element.tag.as_str() {
                "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                    style.size_scale *= 1.4;
                    style.font_weight = FontWeight::BOLD;
                }
                "b" | "strong" | "th" => {
                    style.font_weight = Weight::Bolder.resolve(parent.font_weight)
                }
                "em" | "i" => style.font_style = FontStyle::Italic,
                "sup" | "sub" => {
                    style.size_scale *= 0.7;
                    style.image_align = if element.tag == "sup" {
                        parley::VerticalAlign::SUPER
                    } else {
                        parley::VerticalAlign::SUB
                    };
                }
                _ => {}
            }
            let inline = element
                .inline_style
                .as_deref()
                .filter(|text| self.author_inline && text.len() <= MAX_INLINE_BYTES)
                .map_or_else(Vec::new, |text| declarations(&mut Parser::new(text)));
            let mut winners: Winners = [None; CASCADE_PROPERTY_COUNT];
            let accept_fit = fit_content && element.tag == "p";
            if !style.ignore_table_hints
                && let Some(hints) = &element.table_hints
            {
                hints.cascade(&mut winners);
            }
            for (origin, rules) in [
                (CascadeOrigin::User, &self.user_rules),
                (CascadeOrigin::Author, &self.rules),
                (CascadeOrigin::UserAgent, &self.base_rules),
            ] {
                for rule in rules {
                    if let Some(specificity) = rule
                        .selectors
                        .iter()
                        .filter(|selector| selector.matches(elements, index))
                        .map(|selector| selector.specificity)
                        .max()
                    {
                        choose(
                            &mut winners,
                            &rule.declarations,
                            false,
                            specificity,
                            origin,
                            accept_fit,
                        );
                    }
                }
            }
            choose(
                &mut winners,
                &inline,
                true,
                (0, 0, 0),
                CascadeOrigin::Author,
                accept_fit,
            );
            style.ruby_size_explicit = element.tag == "rt" && winners[1].is_some();
            let display = match winners[40].map(|(_, property)| property) {
                Some(Property::Display(display)) => *display,
                Some(Property::Reset(40, _)) => Display::Inline,
                _ => style.display,
            };
            let block = display == Display::Block || display.is_list_item();
            let formatting = footnotes.and_then(|targets| targets.formatting(index));
            let regularize = formatting.is_some_and(|(options, _)| options.regularize);
            let footnote_size = footnotes.and_then(|targets| {
                // The chosen size replaces block font declarations. Inline
                // elements such as superscripts still compute from the resized parent.
                targets.font_scale(index, block)
            });
            let note_size = if regularize {
                Some(Property::Size(Size::Parent(1.0)))
            } else {
                footnote_size.map(|scale| Property::Size(Size::Root(scale)))
            };
            if let Some(property) = &note_size {
                winners[1] = Some(((5, false, (0, 0, 0)), property));
                style.allow_small_note_text |= footnote_size.is_some();
            }
            for (_, property) in winners.into_iter().flatten() {
                match property {
                    Property::Reset(16, reset) => {
                        decoration = user_css::decoration(*reset, parent.decoration)
                    }
                    Property::Reset(13, reset) => {
                        user_css::apply_reset(&mut style, 13, *reset, parent, &defaults);
                        style.table_valign_explicit = table_valign_element;
                    }
                    Property::Reset(key, reset) => {
                        user_css::apply_reset(&mut style, *key, *reset, parent, &defaults)
                    }
                    Property::ResetMany(..) => {
                        unreachable!("reset shorthand is expanded before cascading")
                    }
                    Property::VariantShorthand(..) | Property::LigatureShorthand(..) => {
                        unreachable!("font variant shorthand is expanded before cascading")
                    }
                    Property::Display(display) => style.display = *display,
                    Property::Visibility(visibility) => style.visibility = *visibility,
                    Property::Float(side) => style.float_side = *side,
                    Property::Clear(clear) => style.clear = *clear,
                    Property::BoxSizing(sizing) => {
                        style.box_sizing = *sizing;
                        style.box_sizing_explicit = true;
                    }
                    Property::Family(families) => {
                        style.font_family = families
                            .iter()
                            .find_map(|family| match family {
                                Family::Named(name) => self
                                    .families
                                    .get(&name.to_ascii_lowercase())
                                    .cloned()
                                    .map(Family::Named),
                                Family::Generic(family) => Some(Family::Generic(*family)),
                            })
                            .unwrap_or(Family::Generic(GenericFamily::Serif));
                    }
                    Property::Size(size) => {
                        let root = result
                            .first()
                            .map_or(defaults.size_scale, |root| root.size_scale);
                        style.size_scale = match size {
                            Size::Parent(scale) => parent.size_scale * scale,
                            Size::Root(scale) => root * scale,
                            Size::Reference(scale) => *scale,
                        };
                    }
                    Property::LetterSpacing(length) => {
                        let font = relative_font(style.size_scale);
                        let root_font = result
                            .first()
                            .map_or(font, |root| relative_font(root.size_scale));
                        // CRE treats percentages as a fraction of the declaring font.
                        let length = match *length {
                            CssLength::Fraction(value) => CssLength::Em(value),
                            value => value,
                        };
                        style.letter_spacing = length.computed(font, root_font);
                    }
                    Property::WordSpacing(length) => {
                        let font = relative_font(style.size_scale);
                        let root_font = result
                            .first()
                            .map_or(font, |root| relative_font(root.size_scale));
                        style.word_spacing = length.unwrap_or_default().computed(font, root_font);
                    }
                    Property::Padding(side, length) => {
                        let font = relative_font(style.size_scale);
                        let root_font = result
                            .first()
                            .map_or(font, |root| relative_font(root.size_scale));
                        let (target, inherited) = if *side < 2 {
                            (&mut style.padding[*side], parent.padding[*side])
                        } else {
                            (
                                &mut style.vertical_padding[*side - 2],
                                parent.vertical_padding[*side - 2],
                            )
                        };
                        *target =
                            length.map_or(inherited, |length| length.computed(font, root_font));
                    }
                    Property::VerticalMargin(side, length) => {
                        let font = relative_font(style.size_scale);
                        let root_font = result
                            .first()
                            .map_or(font, |root| relative_font(root.size_scale));
                        style.vertical_margin[*side] = length
                            .map_or(parent.vertical_margin[*side], |length| {
                                length.computed(font, root_font)
                            });
                    }
                    Property::HorizontalMargin(side, value) => {
                        style.horizontal_margin[*side] = match value {
                            Some(BoxMargin::Length(length))
                                if publisher_horizontal_margins
                                    || matches!(
                                        style.image_width,
                                        Some(ImageLength::FitContent)
                                    ) =>
                            {
                                let font = relative_font(style.size_scale);
                                let root = result
                                    .first()
                                    .map_or(font, |root| relative_font(root.size_scale));
                                BoxMargin::Length(length.computed(font, root))
                            }
                            value => value.unwrap_or(parent.horizontal_margin[*side]),
                        };
                    }
                    Property::PaddingBoth(_) | Property::Margins(_) => {
                        unreachable!("shorthands are expanded before cascading")
                    }
                    Property::Indent(indent) => {
                        if let Some(indent) = indent {
                            let font = relative_font(style.size_scale);
                            let root_font = result
                                .first()
                                .map_or(font, |root| relative_font(root.size_scale));
                            style.text_indent = indent.computed(font, root_font);
                        }
                    }
                    Property::LineHeight(height) => {
                        style.line_height = *height;
                        style.line_height_relative = None;
                    }
                    Property::LengthLineHeight(value) => {
                        style.line_height = None;
                        style.line_height_relative = Some(ComputedLineHeight {
                            value: *value,
                            font_scale: if logical.is_some() {
                                style.size_scale.clamp(f32::MIN_POSITIVE, f32::MAX)
                            } else {
                                style.size_scale.clamp(0.125, 8.0)
                            },
                            allow_small: style.allow_small_note_text,
                        });
                    }
                    Property::Marker(kind) => style.marker = *kind,
                    Property::Background(gray) => style.background = *gray,
                    Property::Alignment(alignment) => style.alignment = Some(*alignment),
                    Property::LastLineAlignment(alignment) => {
                        style.last_line_alignment = *alignment
                    }
                    Property::TextTransform(value) => style.text_transform = *value,
                    Property::OpticalSizing(value) => style.optical_sizing = *value,
                    Property::RubyPosition(value) => style.ruby_position = *value,
                    Property::Weight(weight) => {
                        style.font_weight = weight.resolve(parent.font_weight)
                    }
                    Property::Slant(slant) => style.font_style = *slant,
                    Property::CommonLigatures(enabled) => style.common_ligatures = *enabled,
                    Property::LigatureExtras(extra) => style.ligature_extras = *extra,
                    Property::VariantCaps(caps) => style.variant_caps = *caps,
                    Property::VariantNumeric(numeric) => style.variant_numeric = *numeric,
                    Property::VariantEastAsian(value) => style.variant_east_asian = *value,
                    Property::VariantPosition(value) => style.variant_position = *value,
                    Property::VariantHistorical(value) => style.variant_historical = *value,
                    Property::Direction(direction) => style.direction = *direction,
                    Property::LineBreak(breaking, loose) => {
                        style.line_break = *breaking;
                        style.loose_nonbreaking = *loose;
                    }
                    Property::WordBreak(breaking) => style.word_break = *breaking,
                    Property::WhiteSpace(mode) => {
                        style.inline_nowrap = mode.nowrap();
                        style.preformatted_spaces = mode.preserves_spaces();
                        style.white_space = *mode;
                    }
                    Property::TabSize(value) => style.tab_size_spaces = *value,
                    Property::Border(side, value)
                        if matches!(
                            element.tag.as_str(),
                            "td" | "th" | "tr" | "thead" | "tbody" | "tfoot" | "table" | "caption"
                        ) || (structural_borders
                            && crate::source::is_block_tag(&element.tag))
                            || ((nested_margin_collapse || fit_content)
                                && crate::block_margins::collapsible_wrapper(&element.tag))
                            || ((keep_group_geometry
                                || opening_break_geometry
                                || publisher_wrapper_width)
                                && (element.tag == "p"
                                    || crate::block_margins::collapsible_wrapper(
                                        &element.tag,
                                    )))
                            || ((publisher_block_height
                                || forced_margin_geometry
                                || publisher_horizontal_margins
                                || matches!(style.image_width, Some(ImageLength::FitContent)))
                                && element.tag == "p") =>
                    {
                        let target = style
                            .borders
                            .get_or_insert_with(|| Arc::new([borders::Border::default(); 4]));
                        let inherited = parent
                            .borders
                            .as_ref()
                            .map(|b| b[*side])
                            .unwrap_or_default();
                        let font = relative_font(style.size_scale);
                        let root = result.first().map_or(font, |s| relative_font(s.size_scale));
                        value.apply(&mut Arc::make_mut(target)[*side], inherited, font, root);
                    }
                    Property::Border(..) => (),
                    Property::CaptionBottom(bottom) => style.caption_bottom = *bottom,
                    Property::BorderSpacing(value) => {
                        style.has_table_spacing = value.is_some() || parent.has_table_spacing;
                        style.border_spacing = value.map_or(parent.border_spacing, |values| {
                            if logical.is_some() {
                                let font = relative_font(style.size_scale);
                                let root = result
                                    .first()
                                    .map_or(font, |root| relative_font(root.size_scale));
                                values.map(|value| value.computed(font, root))
                            } else {
                                values
                            }
                        })
                    }
                    Property::BorderParts(_) => unreachable!("expanded before cascading"),
                    Property::BorderCollapse(mode) => {
                        style.border_collapse = mode.unwrap_or(parent.border_collapse)
                    }
                    Property::Orphans(value) => style.orphans = value.unwrap_or(parent.orphans),
                    Property::Widows(value) => style.widows = value.unwrap_or(parent.widows),
                    Property::BreakBefore(value) => {
                        style.page_breaks.before = value.unwrap_or(parent.page_breaks.before)
                    }
                    Property::BreakAfter(value) => {
                        style.page_breaks.after = value.unwrap_or(parent.page_breaks.after)
                    }
                    Property::BreakInside(value) => {
                        style.page_breaks.keep_together =
                            value.unwrap_or(parent.page_breaks.keep_together)
                    }
                    Property::Hyphens(mode) => style.hyphens = *mode,
                    Property::WritingMode(mode) => style.writing_mode = *mode,
                    Property::MinWidth(value) => {
                        if constrained_width
                            || matches!(style.image_width, Some(ImageLength::FitContent))
                        {
                            let font = relative_font(style.size_scale);
                            let root = result
                                .first()
                                .map_or(font, |root| relative_font(root.size_scale));
                            style.min_width =
                                value.css_length().map(|length| length.computed(font, root));
                        }
                    }
                    Property::Width(value)
                    | Property::Height(value)
                    | Property::MaxWidth(value)
                    | Property::MaxHeight(value) => {
                        let fit_height = matches!(style.image_width, Some(ImageLength::FitContent))
                            && matches!(property, Property::Height(_) | Property::MaxHeight(_));
                        let value = match value {
                            ImageLength::RootEm(_) if fit_height => *value,
                            ImageLength::RootEm(value) => {
                                let root = result.first().map_or_else(
                                    || {
                                        if logical.is_some() {
                                            relative_font(style.size_scale)
                                        } else {
                                            style.size_scale
                                        }
                                    },
                                    |root| relative_font(root.size_scale),
                                );
                                ImageLength::RootEm(value * root / relative_font(style.size_scale))
                            }
                            value => *value,
                        };
                        match property {
                            Property::Width(_) => style.image_width = Some(value),
                            Property::Height(_) => style.image_height = Some(value),
                            Property::MaxWidth(_) => style.image_max_width = Some(value),
                            Property::MaxHeight(_) => style.image_max_height = Some(value),
                            _ => unreachable!(),
                        }
                    }
                    Property::MinHeight(value) => style.min_height = Some(*value),
                    Property::GeneratedContent(_) | Property::InitialLetter(_) => (),
                    Property::VerticalAlign(value) => {
                        style.image_align = *value;
                        style.table_valign_explicit = table_valign_element;
                    }
                    Property::Decoration(value) => decoration = value.unwrap_or(parent.decoration),
                    Property::FontSource(_) => {}
                }
            }
            if let Some((options, marginless)) = formatting {
                if options.no_indent {
                    style.text_indent = TextIndent::default();
                }
                if options.justify {
                    style.alignment = Some(Alignment::Justify);
                }
                let inline_like = matches!(
                    display,
                    Display::Inline
                        | Display::Ruby
                        | Display::RunIn
                        | Display::InlineBlock
                        | Display::InlineTable
                );
                let remove_spacing = (options.no_block_spacing && block)
                    || (options.no_inline_spacing && inline_like);
                if marginless || remove_spacing {
                    style.vertical_margin = Default::default();
                    style.horizontal_margin = Default::default();
                }
                if remove_spacing {
                    style.vertical_padding = Default::default();
                    style.padding = Default::default();
                }
                if regularize {
                    style.line_height = parent.line_height;
                    style.line_height_relative = parent.line_height_relative;
                    style.image_align = parley::VerticalAlign::BASELINE;
                }
            }
            if footnotes.is_some_and(|targets| targets.hides_wikipedia_backlink(index)) {
                style.display = Display::None;
            }
            style.decoration = parent.decoration.including(decoration);
            // CREngine places superscripts a quarter of the parent font height above
            // baseline and subscripts four fifths of its descent below it.
            match style.image_align.shift {
                parley::BaselineShift::Super => {
                    style.text_rise.height += 0.25 * relative_font(parent.size_scale)
                }
                parley::BaselineShift::Sub => {
                    style.text_rise.descent -= 0.8 * relative_font(parent.size_scale)
                }
                _ => {}
            }
            style.size_scale = if logical.is_some() {
                style.size_scale.clamp(f32::MIN_POSITIVE, f32::MAX)
            } else {
                style.size_scale.clamp(0.125, 8.0)
            };
            if style.image_full_line {
                style.display = Display::Block;
            }
            style.flow = flow::resolve(parent.flow, style.display, style.visibility);
            if self.layout_tweaks.monochrome {
                style.background = None;
                if let Some(borders) = &mut style.borders {
                    for border in Arc::make_mut(borders) {
                        border.gray = Some((0, 255));
                    }
                }
            }
            let root_scale = result
                .first()
                .map_or(style.size_scale, |root| root.size_scale);
            (style.generated, style.first_letter, style.first_line) =
                pseudo_rules.for_element(elements, index, &style, root_scale, parent, &winners);
            result.push(style);
        }
        (result, pseudo_rules.into_quotes())
    }

    fn read_sheet(
        &mut self,
        book: &mut Book,
        base: &str,
        text: &str,
        remaining: &mut usize,
        faces: &mut Vec<FontFace>,
        names: &ChapterNames<'_>,
        imports: &mut usize,
        active_imports: &mut HashSet<String>,
        depth: usize,
    ) {
        if text.len() > MAX_SHEET_BYTES || text.len() > *remaining {
            return;
        }
        *remaining -= text.len();
        let mut parser = Parser::new(text.trim_start_matches('\u{feff}'));
        let mut imports_allowed = true;
        for rule in media::publisher_items(&mut parser) {
            match rule {
                SheetItem::Import(href)
                    if imports_allowed && depth < MAX_IMPORT_DEPTH && *imports < MAX_IMPORTS =>
                {
                    let Ok(path) = Book::resolve(base, &href) else {
                        continue;
                    };
                    if !path
                        .rsplit_once('.')
                        .is_some_and(|(_, ext)| ext.eq_ignore_ascii_case("css"))
                        || active_imports.contains(&path)
                    {
                        continue;
                    }
                    *imports += 1;
                    let Ok(bytes) = book.resource_limited(&path, MAX_SHEET_BYTES.min(*remaining))
                    else {
                        continue;
                    };
                    let Ok(css) = std::str::from_utf8(&bytes) else {
                        continue;
                    };
                    active_imports.insert(path.clone());
                    self.read_sheet(
                        book,
                        &path,
                        css,
                        remaining,
                        faces,
                        names,
                        imports,
                        active_imports,
                        depth + 1,
                    );
                    active_imports.remove(&path);
                }
                SheetItem::Rule(mut rule) => {
                    imports_allowed = false;
                    if self.rules.len() < MAX_RULES && !rule.declarations.is_empty() {
                        rule.selectors.retain(|selector| names.possible(selector));
                        if !rule.selectors.is_empty() {
                            self.rules.push(rule);
                        }
                    }
                }
                SheetItem::Font(declarations) => {
                    imports_allowed = false;
                    if faces.len() < MAX_FACE_RULES
                        && let Some(face) = FontFace::from_rule(base, declarations)
                    {
                        faces.push(face);
                    }
                }
                SheetItem::MediaBoundary => imports_allowed = false,
                _ => {}
            }
        }
    }

    fn register_fonts(&mut self, book: &mut Book, faces: &[FontFace], fonts: &mut FontContext) {
        let mut assets = FontAssets::default();
        let mut seen = HashSet::new();
        for family in faces.iter().map(|face| &face.family) {
            if !seen.insert(family) {
                continue;
            }
            // Keep the book's CSS families separate from device families of the same name.
            let internal = format!("PrimitiveOS EPUB Face {}", seen.len());
            let mut registered = Vec::new();
            // Later rules for an identical face have priority. A missing source can
            // still fall through to another URL or an earlier face declaration.
            for face in faces.iter().rev().filter(|face| &face.family == family) {
                let descriptor = (face.weight, face.style);
                if registered.contains(&descriptor) {
                    continue;
                }
                for path in &face.paths {
                    let Some(data) = assets.load(book, path) else {
                        continue;
                    };
                    // Each CSS face has its own identity even when several roles
                    // share one font resource. The wrapper shares the original bytes.
                    let data = Blob::new(Arc::new(data));
                    let font_id = data.id();
                    let overrides = FontInfoOverride {
                        family_name: Some(&internal),
                        weight: Some(face.weight),
                        style: Some(face.style),
                        ..Default::default()
                    };
                    if !fonts
                        .collection
                        .register_fonts(data, Some(overrides))
                        .is_empty()
                    {
                        Arc::make_mut(&mut self.font_weights).insert(font_id, face.weight.value());
                        #[cfg(feature = "kindle")]
                        self.registered_faces.insert(font_id, face.name.clone());
                        registered.push(descriptor);
                        break;
                    }
                }
            }
            if !registered.is_empty() {
                self.families.insert(family.clone(), Arc::from(internal));
            }
        }
    }
}

#[derive(Clone, Eq, Hash, PartialEq)]
enum FontPath {
    Local(String),
    Kindle(usize),
}

impl FontPath {
    fn resolve(base: &str, url: &str) -> Option<Self> {
        if let Some(number) = crate::kf8::fonts::reference(url) {
            return Some(Self::Kindle(number));
        }
        let path = Book::resolve(base, url).ok()?;
        path.rsplit_once('.')
            .is_some_and(|(_, ext)| {
                ext.eq_ignore_ascii_case("ttf")
                    || ext.eq_ignore_ascii_case("otf")
                    || ext.eq_ignore_ascii_case("woff")
                    || ext.eq_ignore_ascii_case("woff2")
            })
            .then_some(Self::Local(path))
    }
}

struct FontFace {
    family: String,
    #[cfg(feature = "kindle")]
    name: Arc<str>,
    weight: FontWeight,
    style: FontStyle,
    paths: Vec<FontPath>,
}

impl FontFace {
    fn from_rule(base: &str, declarations: Vec<Declaration>) -> Option<Self> {
        let mut family = None;
        let mut weight = FontWeight::NORMAL;
        let mut style = FontStyle::Normal;
        let mut paths = Vec::new();
        for declaration in declarations
            .into_iter()
            .filter(|declaration| !declaration.important)
        {
            match declaration.property {
                Property::Family(names) => {
                    if let [Family::Named(name)] = names.as_slice() {
                        family = Some(name.clone());
                    }
                }
                Property::Weight(Weight::Fixed(value)) => weight = value,
                Property::Slant(value) => style = value,
                Property::FontSource(urls) => {
                    paths = urls
                        .iter()
                        .filter_map(|url| FontPath::resolve(base, url))
                        .collect();
                }
                _ => {}
            }
        }
        if paths.is_empty() {
            return None;
        }
        let family = family?;
        Some(Self {
            family: family.to_ascii_lowercase(),
            #[cfg(feature = "kindle")]
            name: family.into(),
            weight,
            style,
            paths,
        })
    }
}

#[derive(Default)]
struct FontAssets {
    files: HashMap<FontPath, Option<Blob<u8>>>,
    bytes: usize,
    woff2_deadline: Option<std::time::Instant>,
}

impl FontAssets {
    fn load(&mut self, book: &mut Book, path: &FontPath) -> Option<Blob<u8>> {
        if let Some(cached) = self.files.get(path) {
            return cached.clone();
        }
        if self.files.len() >= MAX_FONT_READS {
            return None;
        }
        let limit = MAX_FONT_BYTES.min(MAX_TOTAL_FONT_BYTES - self.bytes);
        let data = match path {
            FontPath::Local(path) => book.local_font(path, limit),
            FontPath::Kindle(number) => book.kindle_font(*number, limit),
        }
        .ok()
        .and_then(|bytes| {
            let bytes = if bytes.starts_with(b"wOFF") {
                crate::woff::decode(&bytes, limit).ok()?
            } else if bytes.starts_with(b"wOF2") {
                let deadline = *self.woff2_deadline.get_or_insert_with(|| {
                    std::time::Instant::now() + std::time::Duration::from_millis(1500)
                });
                crate::woff2::decode(&bytes, limit, deadline)?
            } else {
                bytes
            };
            if !(bytes.starts_with(b"\0\x01\0\0") || bytes.starts_with(b"OTTO")) {
                return None;
            }
            self.bytes += bytes.len();
            Some(Blob::new(Arc::new(bytes)))
        });
        self.files.insert(path.to_owned(), data.clone());
        data
    }
}

type Specificity = (u8, u8, u8);

struct Selector {
    steps: Vec<Compound>,
    specificity: Specificity,
    pseudo: Option<pseudo::Target>,
}

#[derive(Clone, Copy, Default)]
enum Relation {
    #[default]
    Descendant,
    Child,
    NextSibling,
    PreviousSiblings,
}

#[derive(Default)]
struct Compound {
    tag: Option<String>,
    universal: bool,
    has_href: bool,
    semantic_types: Vec<semantic_type::Selector>,
    is_link: bool,
    empty_element: bool,
    root_element: bool,
    directions: Vec<parley::BaseDirection>,
    classes: Vec<String>,
    ids: Vec<String>,
    first_child: bool,
    last_child: bool,
    first_of_type: bool,
    last_of_type: bool,
    odd_child: Option<bool>,
    odd_child_from_end: Option<bool>,
    nth_child: Vec<(i32, i32)>,
    nth_child_from_end: Vec<(i32, i32)>,
    nth_type: Vec<(i32, i32)>,
    nth_type_from_end: Vec<(i32, i32)>,
    negations: Vec<Compound>,
    alternatives: Vec<Vec<Compound>>,
    relation: Relation,
}

impl Compound {
    fn ambiguous(&self, element: &Element) -> bool {
        self.semantic_types
            .iter()
            .any(|selector| selector.ambiguous(element))
            || self
                .negations
                .iter()
                .any(|negative| negative.ambiguous(element))
            || self
                .alternatives
                .iter()
                .flatten()
                .any(|candidate| candidate.ambiguous(element))
    }
    fn is_empty(&self) -> bool {
        self.tag.is_none()
            && !self.universal
            && !self.has_href
            && self.semantic_types.is_empty()
            && !self.is_link
            && !self.empty_element
            && !self.root_element
            && self.directions.is_empty()
            && self.classes.is_empty()
            && self.ids.is_empty()
            && !self.first_child
            && !self.last_child
            && !self.first_of_type
            && !self.last_of_type
            && self.odd_child.is_none()
            && self.odd_child_from_end.is_none()
            && self.nth_child.is_empty()
            && self.nth_child_from_end.is_empty()
            && self.nth_type.is_empty()
            && self.nth_type_from_end.is_empty()
            && self.negations.is_empty()
            && self.alternatives.is_empty()
    }

    fn matches(&self, element: &Element) -> bool {
        !element.anonymous
            && !self.ambiguous(element)
            && (!self.has_href || element.has_href)
            && self
                .semantic_types
                .iter()
                .all(|selector| selector.matches(element))
            && (!self.is_link || (element.tag == "a" && element.has_href))
            && (!self.empty_element || element.selector_empty)
            && (!self.root_element || element.selector_root)
            && self
                .directions
                .iter()
                .all(|direction| element.selector_direction.as_ref() == Some(direction))
            && (!self.first_child || element.previous_sibling.is_none())
            && (!self.last_child || element.last_child)
            && (!self.first_of_type || element.first_of_type)
            && (!self.last_of_type || element.last_of_type)
            && self.odd_child.is_none_or(|odd| odd == element.odd_sibling)
            && self
                .odd_child_from_end
                .is_none_or(|odd| odd == element.odd_sibling_from_end)
            && self
                .nth_child
                .iter()
                .all(|&formula| nth_matches(formula, element.sibling_index))
            && self
                .nth_child_from_end
                .iter()
                .all(|&formula| nth_matches(formula, element.sibling_index_from_end))
            && self
                .nth_type
                .iter()
                .all(|&formula| nth_matches(formula, element.type_index))
            && self
                .nth_type_from_end
                .iter()
                .all(|&formula| nth_matches(formula, element.type_index_from_end))
            && self
                .negations
                .iter()
                .all(|negative| !negative.matches(element))
            && self
                .alternatives
                .iter()
                .all(|group| group.iter().any(|candidate| candidate.matches(element)))
            && self
                .tag
                .as_ref()
                .is_none_or(|tag| tag.eq_ignore_ascii_case(&element.tag))
            && self.ids.iter().all(|id| element.id.as_ref() == Some(id))
            && self.classes.iter().all(|class| {
                element
                    .classes
                    .split_ascii_whitespace()
                    .any(|name| name == class)
            })
    }
}

fn nth_matches((a, b): (i32, i32), index: usize) -> bool {
    let Ok(index) = i64::try_from(index) else {
        return false;
    };
    let Some(delta) = index.checked_sub(i64::from(b)) else {
        return false;
    };
    let a = i64::from(a);
    if a == 0 {
        delta == 0
    } else {
        delta % a == 0 && delta / a >= 0
    }
}

impl Selector {
    fn parse<'i>(input: &mut Parser<'i>) -> CssResult<Self> {
        Self::parse_with_depth(input, 0)
    }

    fn parse_with_depth<'i>(input: &mut Parser<'i>, depth: u8) -> CssResult<Self> {
        let start = input.position().byte_index();
        let mut steps = Vec::new();
        let mut compound = Compound::default();
        let mut separated = false;
        let mut specificity: Specificity = (0, 0, 0);
        let mut pseudo = None;
        while let Ok(token) = input.next_including_whitespace().cloned() {
            if matches!(token, Token::WhiteSpace(_)) {
                separated = !compound.is_empty();
                continue;
            }
            if let Token::Delim(combinator @ ('>' | '+' | '~')) = token {
                if compound.is_empty() {
                    return Err(ParseError::unexpected_token());
                }
                steps.push(std::mem::take(&mut compound));
                compound.relation = match combinator {
                    '>' => Relation::Child,
                    '+' => Relation::NextSibling,
                    '~' => Relation::PreviousSiblings,
                    _ => unreachable!(),
                };
                separated = false;
                continue;
            }
            if separated {
                steps.push(std::mem::take(&mut compound));
                separated = false;
            }
            match token {
                Token::Ident(tag) if compound.is_empty() => {
                    compound.tag = Some(tag.to_string());
                    specificity.2 += 1;
                }
                Token::Delim('*') if compound.is_empty() => compound.universal = true,
                Token::SquareBracketBlock => {
                    match input.parse_nested_block(semantic_type::parse)? {
                        semantic_type::Attribute::Href => compound.has_href = true,
                        semantic_type::Attribute::Value(selector) => {
                            compound.semantic_types.push(selector)
                        }
                    }
                    specificity.1 = specificity.1.saturating_add(1);
                }
                Token::Colon => {
                    let doubled = input.try_parse(Parser::expect_colon).is_ok();
                    let token = input.next_including_whitespace()?.clone();
                    if let Token::Ident(name) = &token
                        && let Some(side) = pseudo::Target::parse(name)
                    {
                        if compound.is_empty() {
                            compound.universal = true;
                        }
                        pseudo = Some(side);
                        specificity.2 = specificity.2.saturating_add(1);
                        input.expect_exhausted()?;
                        break;
                    }
                    if doubled {
                        return Err(ParseError::unexpected_token());
                    }
                    let mut class_specificity = true;
                    match token {
                        Token::Function(name)
                            if name.eq_ignore_ascii_case("not")
                                || name.eq_ignore_ascii_case("is")
                                || name.eq_ignore_ascii_case("where") =>
                        {
                            if depth >= 4 {
                                return Err(ParseError::unexpected_token());
                            }
                            let nested = input.parse_nested_block(|input| {
                                let mut count = 0;
                                input.parse_comma_separated(|part| {
                                    count += 1;
                                    if count > MAX_SELECTORS_PER_RULE {
                                        return Err(ParseError::unexpected_token());
                                    }
                                    Self::parse_with_depth(part, depth + 1)
                                })
                            })?;
                            if nested.is_empty()
                                || nested.len() > MAX_SELECTORS_PER_RULE
                                || nested
                                    .iter()
                                    .any(|item| item.steps.len() != 1 || item.pseudo.is_some())
                            {
                                return Err(ParseError::unexpected_token());
                            }
                            if !name.eq_ignore_ascii_case("where") {
                                let highest = nested
                                    .iter()
                                    .map(|item| item.specificity)
                                    .max()
                                    .unwrap_or_default();
                                specificity.0 = specificity.0.saturating_add(highest.0);
                                specificity.1 = specificity.1.saturating_add(highest.1);
                                specificity.2 = specificity.2.saturating_add(highest.2);
                            }
                            let items: Vec<_> = nested
                                .into_iter()
                                .map(|mut item| item.steps.pop().expect("one checked selector"))
                                .collect();
                            if name.eq_ignore_ascii_case("not") {
                                compound.negations.extend(items);
                            } else {
                                compound.alternatives.push(items);
                            }
                            class_specificity = false;
                        }
                        Token::Ident(name) if name.eq_ignore_ascii_case("first-of-type") => {
                            compound.first_of_type = true;
                        }
                        Token::Ident(name) if name.eq_ignore_ascii_case("empty") => {
                            compound.empty_element = true;
                        }
                        Token::Ident(name) if name.eq_ignore_ascii_case("root") => {
                            compound.root_element = true;
                        }
                        Token::Function(name) if name.eq_ignore_ascii_case("dir") => {
                            let direction = input.parse_nested_block(|input| {
                                let value = input.expect_ident_cloned()?;
                                input.expect_exhausted()?;
                                if value.eq_ignore_ascii_case("ltr") {
                                    Ok(parley::BaseDirection::Ltr)
                                } else if value.eq_ignore_ascii_case("rtl") {
                                    Ok(parley::BaseDirection::Rtl)
                                } else {
                                    Err(ParseError::unexpected_token())
                                }
                            })?;
                            compound.directions.push(direction);
                        }
                        Token::Ident(name) if name.eq_ignore_ascii_case("last-child") => {
                            compound.last_child = true;
                        }
                        Token::Ident(name) if name.eq_ignore_ascii_case("only-child") => {
                            compound.first_child = true;
                            compound.last_child = true;
                        }
                        Token::Ident(name) if name.eq_ignore_ascii_case("last-of-type") => {
                            compound.last_of_type = true;
                        }
                        Token::Ident(name) if name.eq_ignore_ascii_case("only-of-type") => {
                            compound.first_of_type = true;
                            compound.last_of_type = true;
                        }
                        Token::Ident(name)
                            if name.eq_ignore_ascii_case("link")
                                || name.eq_ignore_ascii_case("any-link") =>
                        {
                            compound.is_link = true;
                        }
                        Token::Ident(name) if name.eq_ignore_ascii_case("first-child") => {
                            compound.first_child = true;
                        }
                        Token::Function(name) if name.eq_ignore_ascii_case("nth-child") => {
                            let formula = input.parse_nested_block(|input| {
                                let formula = cssparser::parse_nth(input)?;
                                input.expect_exhausted()?;
                                Ok(formula)
                            })?;
                            if let Some(odd) = match formula {
                                (2, 1) => Some(true),
                                (2, 0) => Some(false),
                                _ => None,
                            } {
                                if compound.odd_child.is_some_and(|old| old != odd) {
                                    return Err(ParseError::unexpected_token());
                                }
                                compound.odd_child = Some(odd);
                            } else {
                                compound.nth_child.push(formula);
                            }
                        }
                        Token::Function(name) if name.eq_ignore_ascii_case("nth-last-child") => {
                            let formula = input.parse_nested_block(|input| {
                                let formula = cssparser::parse_nth(input)?;
                                input.expect_exhausted()?;
                                Ok(formula)
                            })?;
                            if let Some(odd) = match formula {
                                (2, 1) => Some(true),
                                (2, 0) => Some(false),
                                _ => None,
                            } {
                                if compound.odd_child_from_end.is_some_and(|old| old != odd) {
                                    return Err(ParseError::unexpected_token());
                                }
                                compound.odd_child_from_end = Some(odd);
                            } else {
                                compound.nth_child_from_end.push(formula);
                            }
                        }
                        Token::Function(name)
                            if name.eq_ignore_ascii_case("nth-of-type")
                                || name.eq_ignore_ascii_case("nth-last-of-type") =>
                        {
                            let formula = input.parse_nested_block(|input| {
                                let formula = cssparser::parse_nth(input)?;
                                input.expect_exhausted()?;
                                Ok(formula)
                            })?;
                            if name.eq_ignore_ascii_case("nth-of-type") {
                                compound.nth_type.push(formula);
                            } else {
                                compound.nth_type_from_end.push(formula);
                            }
                        }
                        _ => return Err(ParseError::unexpected_token()),
                    }
                    if class_specificity {
                        specificity.1 = specificity.1.saturating_add(1);
                    }
                }
                Token::IDHash(id) => {
                    compound.ids.push(id.to_string());
                    specificity.0 = specificity.0.saturating_add(1);
                }
                Token::Delim('.') => {
                    let Token::Ident(name) = input.next_including_whitespace()? else {
                        return Err(ParseError::unexpected_token());
                    };
                    compound.classes.push(name.to_string());
                    specificity.1 = specificity.1.saturating_add(1);
                }
                _ => return Err(ParseError::unexpected_token()),
            }
            if steps.len() >= MAX_SELECTOR_STEPS
                || input.position().byte_index() - start > MAX_SELECTOR_BYTES
            {
                return Err(ParseError::unexpected_token());
            }
        }
        if compound.is_empty() {
            return Err(ParseError::unexpected_token());
        }
        steps.push(compound);
        Ok(Self {
            steps,
            specificity,
            pseudo,
        })
    }

    fn matches(&self, elements: &[Element], index: usize) -> bool {
        self.pseudo.is_none() && self.matches_host(elements, index)
    }

    fn matches_host(&self, elements: &[Element], index: usize) -> bool {
        if self
            .steps
            .iter()
            .any(|step| matches!(step.relation, Relation::PreviousSiblings))
        {
            // A general-sibling choice may need backtracking through prior
            // siblings or ancestors. Bound the total visited candidates.
            let mut remaining = 128;
            return self.matches_bounded(self.steps.len() - 1, elements, index, &mut remaining);
        }
        // Check each uninterrupted chain of child/sibling relationships as a unit.
        // A nearer ancestor that fails the chain must not hide a higher matching
        // ancestor. Searching outward only between chains bounds work by depth × steps.
        let Some((mut step, mut node)) = self.chain(self.steps.len() - 1, elements, index) else {
            return false;
        };
        while step > 0 {
            let mut ancestors =
                std::iter::successors(elements[node].parent, |&i| elements[i].parent);
            let Some(matched) = ancestors.find_map(|i| self.chain(step - 1, elements, i)) else {
                return false;
            };
            (step, node) = matched;
        }
        true
    }

    fn matches_bounded(
        &self,
        step: usize,
        elements: &[Element],
        index: usize,
        remaining: &mut usize,
    ) -> bool {
        if *remaining == 0 {
            return false;
        }
        *remaining -= 1;
        let Some(element) = elements.get(index) else {
            return false;
        };
        let compound = &self.steps[step];
        if !compound.matches(element) {
            return false;
        }
        if step == 0 {
            return true;
        }
        match compound.relation {
            Relation::Child => element
                .parent
                .is_some_and(|parent| self.matches_bounded(step - 1, elements, parent, remaining)),
            Relation::NextSibling => element.previous_sibling.is_some_and(|sibling| {
                self.matches_bounded(step - 1, elements, sibling, remaining)
            }),
            Relation::Descendant | Relation::PreviousSiblings => {
                let siblings = matches!(compound.relation, Relation::PreviousSiblings);
                let mut candidate = if siblings {
                    element.previous_sibling
                } else {
                    element.parent
                };
                while let Some(i) = candidate {
                    if *remaining == 0 {
                        return false;
                    }
                    if self.matches_bounded(step - 1, elements, i, remaining) {
                        return true;
                    }
                    let Some(item) = elements.get(i) else {
                        return false;
                    };
                    candidate = if siblings {
                        item.previous_sibling
                    } else {
                        item.parent
                    };
                }
                false
            }
        }
    }

    fn chain(
        &self,
        mut step: usize,
        elements: &[Element],
        mut index: usize,
    ) -> Option<(usize, usize)> {
        loop {
            let compound = &self.steps[step];
            let element = &elements[index];
            if !compound.matches(element) {
                return None;
            }
            if step == 0 {
                return Some((step, index));
            }
            index = match compound.relation {
                Relation::Descendant => return Some((step, index)),
                Relation::Child => element.parent?,
                Relation::NextSibling => element.previous_sibling?,
                Relation::PreviousSiblings => return None, // handled by matches_bounded
            };
            step -= 1;
        }
    }
}

// Shared publisher stylesheets often describe hundreds of classes used only
// in other chapters. Keep their selectors out of the per-chapter rule budget.
struct ChapterNames<'a> {
    classes: Option<HashSet<&'a str>>,
    ids: Option<HashSet<&'a str>>,
}

impl<'a> ChapterNames<'a> {
    fn new(elements: &'a [Element]) -> Self {
        fn bounded<'a>(names: impl Iterator<Item = &'a str>) -> Option<HashSet<&'a str>> {
            let mut found = HashSet::new();
            for name in names {
                found.insert(name);
                // When the index is full, allow every candidate in this category.
                if found.len() > 1024 {
                    return None;
                }
            }
            Some(found)
        }
        Self {
            classes: bounded(
                elements
                    .iter()
                    .flat_map(|el| el.classes.split_ascii_whitespace()),
            ),
            ids: bounded(elements.iter().filter_map(|el| el.id.as_deref())),
        }
    }

    fn possible(&self, selector: &Selector) -> bool {
        selector.steps.iter().all(|step| {
            self.classes
                .as_ref()
                .is_none_or(|names| step.classes.iter().all(|c| names.contains(c.as_str())))
                && self
                    .ids
                    .as_ref()
                    .is_none_or(|names| step.ids.iter().all(|id| names.contains(id.as_str())))
        })
    }
}

struct Rule {
    selectors: Vec<Selector>,
    declarations: Vec<Declaration>,
}

enum SheetItem {
    Rule(Rule),
    Font(Vec<Declaration>),
    Import(String),
    MediaBoundary,
}

enum SheetPrelude {
    Font,
    Import(String),
}

type CssResult<T> = Result<T, ParseError<()>>;

struct SheetParser;

impl<'i> QualifiedRuleParser<'i> for SheetParser {
    type Prelude = Vec<Selector>;
    type QualifiedRule = SheetItem;
    type Error = ();

    fn parse_prelude(&mut self, input: &mut Parser<'i>) -> CssResult<Self::Prelude> {
        let selectors = input.parse_comma_separated(Selector::parse)?;
        if selectors.len() > MAX_SELECTORS_PER_RULE {
            return Err(ParseError::unexpected_token());
        }
        Ok(selectors)
    }

    fn parse_block(
        &mut self,
        selectors: Self::Prelude,
        _: &ParserState,
        input: &mut Parser<'i>,
    ) -> CssResult<SheetItem> {
        Ok(SheetItem::Rule(Rule {
            selectors,
            declarations: declarations(input),
        }))
    }
}

impl<'i> AtRuleParser<'i> for SheetParser {
    type Prelude = SheetPrelude;
    type AtRule = SheetItem;
    type Error = ();

    fn parse_prelude(
        &mut self,
        name: CowRcStr<'i>,
        input: &mut Parser<'i>,
    ) -> CssResult<SheetPrelude> {
        if name.eq_ignore_ascii_case("font-face") {
            input.expect_exhausted()?;
            return Ok(SheetPrelude::Font);
        }
        if name.eq_ignore_ascii_case("import") {
            let href = input
                .try_parse(|p| p.expect_string_cloned())
                .or_else(|_| input.expect_url())?
                .to_string();
            input.expect_exhausted()?;
            return Ok(SheetPrelude::Import(href));
        }
        Err(ParseError::unexpected_token())
    }

    fn rule_without_block(
        &mut self,
        prelude: SheetPrelude,
        _: &ParserState,
    ) -> Result<SheetItem, ()> {
        match prelude {
            SheetPrelude::Import(href) => Ok(SheetItem::Import(href)),
            SheetPrelude::Font => Err(()),
        }
    }

    fn parse_block(
        &mut self,
        prelude: SheetPrelude,
        _: &ParserState,
        input: &mut Parser<'i>,
    ) -> CssResult<SheetItem> {
        match prelude {
            SheetPrelude::Font => Ok(SheetItem::Font(declarations(input))),
            SheetPrelude::Import(_) => Err(ParseError::unexpected_token()),
        }
    }
}

struct Declaration {
    property: Property,
    important: bool,
}

fn declarations(input: &mut Parser<'_>) -> Vec<Declaration> {
    let mut result = Vec::new();
    for decl in RuleBodyParser::new(input, &mut Properties).flatten() {
        if let Property::VariantShorthand(
            ligatures,
            extra,
            caps,
            numeric,
            east_asian,
            position,
            historical,
        ) = decl.property
        {
            result.push(Declaration {
                property: Property::CommonLigatures(ligatures),
                important: decl.important,
            });
            result.push(Declaration {
                property: Property::LigatureExtras(extra),
                important: decl.important,
            });
            result.push(Declaration {
                property: Property::VariantCaps(caps),
                important: decl.important,
            });
            result.push(Declaration {
                property: Property::VariantNumeric(numeric),
                important: decl.important,
            });
            result.push(Declaration {
                property: Property::VariantEastAsian(east_asian),
                important: decl.important,
            });
            result.push(Declaration {
                property: Property::VariantPosition(position),
                important: decl.important,
            });
            result.push(Declaration {
                property: Property::VariantHistorical(historical),
                important: decl.important,
            });
        } else if let Property::LigatureShorthand(common, extra) = decl.property {
            result.push(Declaration {
                property: Property::CommonLigatures(common),
                important: decl.important,
            });
            result.push(Declaration {
                property: Property::LigatureExtras(extra),
                important: decl.important,
            });
        } else if let Property::ResetMany(keys, reset) = decl.property {
            result.extend(keys.into_iter().map(|key| Declaration {
                property: Property::Reset(key, reset),
                important: decl.important,
            }));
        } else if let Property::PaddingBoth(sides) = decl.property {
            for (side, length) in sides.into_iter().enumerate() {
                result.push(Declaration {
                    property: Property::Padding(side, length),
                    important: decl.important,
                });
            }
        } else if let Property::Margins(edges) = decl.property {
            for (side, margin) in edges.into_iter().enumerate() {
                let property = if side < 2 {
                    Property::HorizontalMargin(side, margin)
                } else {
                    Property::VerticalMargin(side - 2, margin.map(BoxMargin::length))
                };
                result.push(Declaration {
                    property,
                    important: decl.important,
                });
            }
        } else if let Property::BorderParts(parts) = decl.property {
            result.extend(parts.into_iter().map(|(side, part)| Declaration {
                property: Property::Border(side, part),
                important: decl.important,
            }));
        } else {
            result.push(decl);
        }
    }
    result
}

type Priority = (u8, bool, Specificity);
const CAPTION_SIDE: usize = 54;
const COMMON_LIGATURES: usize = 55;
const DIRECTION: usize = 56;
const LETTER_SPACING: usize = 57;
const VARIANT_CAPS: usize = 58;
const VARIANT_NUMERIC: usize = 59;
const VARIANT_EAST_ASIAN: usize = 60;
const VARIANT_POSITION: usize = 61;
const VARIANT_HISTORICAL: usize = 62;
const LIGATURE_EXTRAS: usize = 63;
const LAST_LINE_ALIGNMENT: usize = 64;
const TEXT_TRANSFORM: usize = 65;
const OPTICAL_SIZING: usize = 66;
const RUBY_POSITION: usize = 67;
const TAB_SIZE: usize = 68;
const WORD_SPACING: usize = 69;
const CASCADE_PROPERTY_COUNT: usize = 70;
type Winners<'a> = [Option<(Priority, &'a Property)>; CASCADE_PROPERTY_COUNT];

#[derive(Clone, Copy)]
enum CascadeOrigin {
    UserAgent,
    User,
    Author,
}

fn cascade_priority(
    origin: CascadeOrigin,
    important: bool,
    inline: bool,
    specificity: Specificity,
) -> Priority {
    let tier = match (origin, important) {
        (CascadeOrigin::UserAgent, false) => 0,
        (CascadeOrigin::UserAgent, true) => 1,
        (CascadeOrigin::User, false) => 2,
        (CascadeOrigin::Author, false) => 3,
        (CascadeOrigin::Author, true) => 4,
        (CascadeOrigin::User, true) => 5,
    };
    (tier, inline, specificity)
}

fn choose<'a>(
    winners: &mut Winners<'a>,
    declarations: &'a [Declaration],
    inline: bool,
    specificity: Specificity,
    origin: CascadeOrigin,
    accept_fit: bool,
) {
    for declaration in declarations {
        let index = match declaration.property {
            Property::Reset(key, _) => key,
            Property::ResetMany(..) => continue,
            Property::VariantShorthand(..) => continue,
            Property::LigatureShorthand(..) => continue,
            Property::Display(_) => 40,
            Property::Visibility(_) => 41,
            Property::MaxWidth(_) => 42,
            Property::MaxHeight(value) if !accept_fit && value.pixels(1, 1.0) == Some(0.0) => {
                continue;
            }
            Property::MaxHeight(_) => 43,
            Property::VerticalMargin(side, _) => 44 + side,
            Property::Float(_) => 46,
            Property::Clear(_) => 47,
            Property::HorizontalMargin(side, _) => 48 + side,
            Property::MinWidth(_) => 50,
            Property::BoxSizing(_) => 51,
            Property::MinHeight(_) => 52,
            Property::Family(_) => 0,
            Property::Size(_) => 1,
            Property::LineHeight(_) | Property::LengthLineHeight(_) => 2,
            Property::Background(_) => 3,
            Property::Alignment(_) => 4,
            Property::LastLineAlignment(_) => LAST_LINE_ALIGNMENT,
            Property::TextTransform(_) => TEXT_TRANSFORM,
            Property::OpticalSizing(_) => OPTICAL_SIZING,
            Property::RubyPosition(_) => RUBY_POSITION,
            Property::Weight(_) => 5,
            Property::Slant(_) => 6,
            Property::CommonLigatures(_) => COMMON_LIGATURES,
            Property::LigatureExtras(_) => LIGATURE_EXTRAS,
            Property::VariantCaps(_) => VARIANT_CAPS,
            Property::VariantNumeric(_) => VARIANT_NUMERIC,
            Property::VariantEastAsian(_) => VARIANT_EAST_ASIAN,
            Property::VariantPosition(_) => VARIANT_POSITION,
            Property::VariantHistorical(_) => VARIANT_HISTORICAL,
            Property::Direction(_) => DIRECTION,
            Property::LetterSpacing(_) => LETTER_SPACING,
            Property::WordSpacing(_) => WORD_SPACING,
            Property::LineBreak(_, _) => 7,
            Property::WordBreak(_) => 8,
            Property::WhiteSpace(_) => 53,
            Property::TabSize(_) => TAB_SIZE,
            Property::Hyphens(_) => 9,
            Property::WritingMode(_) => 10,
            Property::Width(ImageLength::FitContent) if !accept_fit => continue,
            Property::Width(_) => 11,
            Property::Height(value) if !accept_fit && value.pixels(1, 1.0) == Some(0.0) => {
                continue;
            }
            Property::Height(_) => 12,
            Property::VerticalAlign(_) => 13,
            Property::Marker(_) => 14,
            Property::Indent(_) => 15,
            Property::Decoration(_) => 16,
            Property::BreakBefore(_) => 17,
            Property::BreakAfter(_) => 18,
            Property::BreakInside(_) => 19,
            Property::Orphans(_) => 20,
            Property::Widows(_) => 21,
            Property::BorderCollapse(_) => 24,
            Property::Border(side, part) => 25 + 3 * side + part.component(),
            Property::BorderSpacing(_) => 37,
            Property::CaptionBottom(_) => CAPTION_SIDE,
            Property::BorderParts(_) => continue,
            Property::Padding(side, _) => {
                if side < 2 {
                    22 + side
                } else {
                    36 + side
                }
            }
            Property::PaddingBoth(_)
            | Property::Margins(_)
            | Property::FontSource(_)
            | Property::GeneratedContent(_)
            | Property::InitialLetter(_) => {
                continue;
            }
        };
        let priority = cascade_priority(origin, declaration.important, inline, specificity);
        if winners[index].is_none_or(|(current, _)| priority >= current) {
            winners[index] = Some((priority, &declaration.property));
        }
    }
}

#[derive(Clone, Copy)]
enum Size {
    Parent(f32),
    Root(f32),
    /// Author px and pt use the CSS 16 px reference size, scaled by reader typography.
    Reference(f32),
}

#[derive(Clone, Copy)]
enum Weight {
    Fixed(FontWeight),
    Bolder,
    Lighter,
}

impl Weight {
    fn resolve(self, inherited: FontWeight) -> FontWeight {
        match self {
            Self::Fixed(weight) => weight,
            Self::Bolder if inherited < FontWeight::SEMI_LIGHT => FontWeight::NORMAL,
            Self::Bolder if inherited < FontWeight::new(550.0) => FontWeight::BOLD,
            Self::Bolder if inherited < FontWeight::BLACK => FontWeight::BLACK,
            Self::Lighter if inherited < FontWeight::THIN => inherited,
            Self::Lighter if inherited < FontWeight::new(550.0) => FontWeight::THIN,
            Self::Lighter if inherited < FontWeight::new(750.0) => FontWeight::NORMAL,
            Self::Lighter => FontWeight::BOLD,
            Self::Bolder => inherited,
        }
    }
}

enum Property {
    GeneratedContent(Vec<pseudo::ContentPart>),
    InitialLetter(Option<pseudo::InitialLetter>),
    Display(Display),
    Visibility(Visibility),
    Reset(usize, user_css::Reset),
    ResetMany(Vec<usize>, user_css::Reset),
    Border(usize, borders::Value),
    BorderParts(Vec<(usize, borders::Value)>),
    BorderSpacing(Option<[CssLength; 2]>),
    BorderCollapse(Option<bool>),
    Padding(usize, Option<CssLength>),
    PaddingBoth([Option<CssLength>; 4]),
    VerticalMargin(usize, Option<CssLength>),
    HorizontalMargin(usize, Option<BoxMargin>),
    Margins([Option<BoxMargin>; 4]),
    Float(Option<Side>),
    Clear(Clear),
    Orphans(Option<u8>),
    Widows(Option<u8>),
    BreakBefore(Option<crate::publisher_breaks::Edge>),
    BreakAfter(Option<crate::publisher_breaks::Edge>),
    BreakInside(Option<bool>),
    Decoration(Option<TextDecoration>),
    Indent(Option<TextIndent>),
    Marker(crate::list_marker::MarkerKind),
    Family(Vec<Family>),
    FontSource(Vec<String>),
    Size(Size),
    LineHeight(Option<f32>),
    LengthLineHeight(LineHeightLength),
    Background(Option<u8>),
    Alignment(Alignment),
    LastLineAlignment(Option<Alignment>),
    TextTransform(TextTransform),
    OpticalSizing(bool),
    RubyPosition(RubyPosition),
    Weight(Weight),
    Slant(FontStyle),
    CommonLigatures(bool),
    LigatureExtras(LigatureExtras),
    LigatureShorthand(bool, LigatureExtras),
    VariantCaps(VariantCaps),
    VariantNumeric(VariantNumeric),
    VariantEastAsian(VariantEastAsian),
    VariantPosition(VariantPosition),
    VariantHistorical(bool),
    VariantShorthand(
        bool,
        LigatureExtras,
        VariantCaps,
        VariantNumeric,
        VariantEastAsian,
        VariantPosition,
        bool,
    ),
    Direction(parley::BaseDirection),
    LetterSpacing(CssLength),
    WordSpacing(Option<CssLength>),
    TabSize(f32),
    LineBreak(Option<LineBreak>, bool),
    WordBreak(WordBreak),
    WhiteSpace(WhiteSpace),
    CaptionBottom(bool),
    Hyphens(Hyphens),
    WritingMode(WritingMode),
    Width(ImageLength),
    Height(ImageLength),
    MaxWidth(ImageLength),
    MaxHeight(ImageLength),
    MinWidth(ImageLength),
    MinHeight(ImageLength),
    BoxSizing(BoxSizing),
    VerticalAlign(parley::VerticalAlign),
}

struct Properties;

impl<'i> DeclarationParser<'i> for Properties {
    type Declaration = Declaration;
    type Error = ();

    fn parse_value(
        &mut self,
        name: CowRcStr<'i>,
        input: &mut Parser<'i>,
        _: &ParserState,
    ) -> CssResult<Declaration> {
        let property = input.parse_until_before(Delimiter::Bang, |input| {
            let before = input.state();
            if let Some(reset) = user_css::reset(input, &name.to_ascii_lowercase()) {
                input.expect_exhausted()?;
                return Ok(reset);
            }
            input.reset(&before);
            let property = match name.to_ascii_lowercase().as_ref() {
                "content" => Property::GeneratedContent(pseudo::parse_content(input)?),
                "initial-letter" | "-webkit-initial-letter" => {
                    Property::InitialLetter(pseudo::parse_initial_letter(input)?)
                }
                "display" => Property::Display(
                    Display::parse(&input.expect_ident()?.to_ascii_lowercase())
                        .ok_or_else(ParseError::unexpected_token)?,
                ),
                "visibility" => Property::Visibility(
                    Visibility::parse(&input.expect_ident()?.to_ascii_lowercase())
                        .ok_or_else(ParseError::unexpected_token)?,
                ),
                "float" => {
                    Property::Float(match input.expect_ident()?.to_ascii_lowercase().as_str() {
                        "left" => Some(Side::Left),
                        "right" => Some(Side::Right),
                        "none" | "initial" | "unset" => None,
                        _ => return Err(ParseError::unexpected_token()),
                    })
                }
                "clear" => Property::Clear(
                    Clear::parse(&input.expect_ident()?.to_ascii_lowercase())
                        .ok_or_else(ParseError::unexpected_token)?,
                ),
                "padding-top" => Property::Padding(2, parse_padding(input, false)?[0]),
                "padding-bottom" => Property::Padding(3, parse_padding(input, false)?[0]),
                "padding-left" => Property::Padding(0, parse_padding(input, false)?[0]),
                "padding-right" => Property::Padding(1, parse_padding(input, false)?[0]),
                "padding" => Property::PaddingBoth(parse_padding(input, true)?),
                "margin-left" => {
                    Property::HorizontalMargin(0, parse_box_edges(input, false, true)?[0])
                }
                "margin-right" => {
                    Property::HorizontalMargin(1, parse_box_edges(input, false, true)?[1])
                }
                "margin-top" => Property::VerticalMargin(
                    0,
                    parse_box_edges(input, false, true)?[2].map(BoxMargin::length),
                ),
                "margin-bottom" => Property::VerticalMargin(
                    1,
                    parse_box_edges(input, false, true)?[3].map(BoxMargin::length),
                ),
                "margin" => Property::Margins(parse_box_edges(input, true, true)?),

                "border-spacing" => Property::BorderSpacing(
                    if input
                        .try_parse(|p| p.expect_ident_matching("inherit"))
                        .is_ok()
                        || input
                            .try_parse(|p| p.expect_ident_matching("unset"))
                            .is_ok()
                    {
                        None
                    } else if input
                        .try_parse(|p| p.expect_ident_matching("initial"))
                        .is_ok()
                        || input
                            .try_parse(|p| p.expect_ident_matching("revert"))
                            .is_ok()
                    {
                        Some([CssLength::Pixels(0.); 2])
                    } else {
                        let h = parse_css_length(input)?;
                        let v = if input.is_exhausted() {
                            h
                        } else {
                            parse_css_length(input)?
                        };
                        if [h, v]
                            .iter()
                            .any(|p| matches!(p, CssLength::Fraction(_)) || p.pixels(100, 40.) < 0.)
                        {
                            return Err(ParseError::unexpected_token());
                        }
                        Some([h, v])
                    },
                ),
                "border"
                | "border-width"
                | "border-style"
                | "border-color"
                | "border-top"
                | "border-bottom"
                | "border-left"
                | "border-right"
                | "border-top-width"
                | "border-top-style"
                | "border-top-color"
                | "border-bottom-width"
                | "border-bottom-style"
                | "border-bottom-color"
                | "border-left-width"
                | "border-left-style"
                | "border-left-color"
                | "border-right-width"
                | "border-right-style"
                | "border-right-color" => {
                    Property::BorderParts(borders::parse(&name.to_ascii_lowercase(), input)?)
                }
                "border-collapse" => Property::BorderCollapse(
                    match input.expect_ident()?.to_ascii_lowercase().as_str() {
                        "collapse" => Some(true),
                        "separate" | "initial" => Some(false),
                        "inherit" | "unset" => None,
                        _ => return Err(ParseError::unexpected_token()),
                    },
                ),
                "orphans" => Property::Orphans(line_minimum(input)?),
                "widows" => Property::Widows(line_minimum(input)?),
                "page-break-before" | "break-before" => Property::BreakBefore(break_edge(input)?),
                "page-break-after" | "break-after" => Property::BreakAfter(break_edge(input)?),
                "page-break-inside" | "break-inside" => {
                    Property::BreakInside(match break_edge(input)? {
                        Some(crate::publisher_breaks::Edge::Force) => {
                            return Err(ParseError::unexpected_token());
                        }
                        value => value.map(|edge| edge == crate::publisher_breaks::Edge::Avoid),
                    })
                }
                "text-indent" => Property::Indent(parse_indent(input)?),
                "text-decoration" | "-epub-text-decoration" | "text-decoration-line" => {
                    let mut lines = TextDecoration::default();
                    let mut inherit = false;
                    let mut exclusive = false;
                    let mut first = true;
                    while !input.is_exhausted() {
                        let word = input.expect_ident()?.to_ascii_lowercase();
                        if exclusive {
                            return Err(ParseError::unexpected_token());
                        }
                        match word.as_str() {
                            "underline" => lines.underline = true,
                            "overline" => lines.overline = true,
                            "line-through" => lines.strike = true,
                            "solid"
                                if name.eq_ignore_ascii_case("text-decoration")
                                    || name.eq_ignore_ascii_case("-epub-text-decoration") => {}
                            "none" | "initial" | "unset" | "inherit" if first => {
                                inherit = word == "inherit";
                                exclusive = true;
                            }
                            _ => return Err(ParseError::unexpected_token()),
                        }
                        first = false;
                    }
                    if first {
                        return Err(ParseError::unexpected_token());
                    }
                    Property::Decoration((!inherit).then_some(lines))
                }
                "vertical-align" => {
                    use parley::VerticalAlign;
                    Property::VerticalAlign(
                        match input.expect_ident()?.to_ascii_lowercase().as_str() {
                            "baseline" => VerticalAlign::BASELINE,
                            "middle" => VerticalAlign::MIDDLE,
                            "top" => VerticalAlign::TOP,
                            "bottom" => VerticalAlign::BOTTOM,
                            "text-top" => VerticalAlign::TEXT_TOP,
                            "text-bottom" => VerticalAlign::TEXT_BOTTOM,
                            "super" => VerticalAlign::SUPER,
                            "sub" => VerticalAlign::SUB,
                            _ => return Err(ParseError::unexpected_token()),
                        },
                    )
                }
                "box-sizing" => {
                    Property::BoxSizing(match input.expect_ident()?.to_ascii_lowercase().as_str() {
                        "content-box" => BoxSizing::ContentBox,
                        "border-box" => BoxSizing::BorderBox,
                        _ => return Err(ParseError::unexpected_token()),
                    })
                }
                "width" | "height" | "max-width" | "max-height" | "min-width" | "min-height" => {
                    let maximum = name.eq_ignore_ascii_case("max-width")
                        || name.eq_ignore_ascii_case("max-height");
                    let zero = name.eq_ignore_ascii_case("min-width")
                        || matches!(
                            name.to_ascii_lowercase().as_str(),
                            "height" | "min-height" | "max-height"
                        );
                    let length = match input.next()?.clone() {
                        Token::Ident(s)
                            if name.eq_ignore_ascii_case("width")
                                && s.eq_ignore_ascii_case("fit-content") =>
                        {
                            ImageLength::FitContent
                        }
                        Token::Ident(s)
                            if !maximum && s.eq_ignore_ascii_case("auto")
                                || maximum && s.eq_ignore_ascii_case("none") =>
                        {
                            ImageLength::Auto
                        }
                        Token::Number { value, .. } if zero && value == 0.0 => {
                            ImageLength::Pixels(0.0)
                        }
                        Token::Percentage { unit_value, .. }
                            if unit_value.is_finite()
                                && (unit_value > 0.0 || zero && unit_value == 0.0) =>
                        {
                            ImageLength::Fraction(unit_value)
                        }
                        Token::Dimension { unit, value, .. }
                            if value.is_finite() && (value > 0.0 || zero && value == 0.0) =>
                        {
                            match unit.to_ascii_lowercase().as_str() {
                                "px" => ImageLength::Pixels(value),
                                "em" => ImageLength::Em(value),
                                "rem" => ImageLength::RootEm(value),
                                "pt" => ImageLength::Physical(value * (96.0 / 72.0)),
                                "pc" => ImageLength::Physical(value * 16.0),
                                "in" => ImageLength::Physical(value * 96.0),
                                "cm" => ImageLength::Physical(value * (96.0 / 2.54)),
                                "mm" => ImageLength::Physical(value * (96.0 / 25.4)),
                                "q" => ImageLength::Physical(value * (96.0 / 101.6)),
                                _ => return Err(ParseError::unexpected_token()),
                            }
                        }
                        _ => return Err(ParseError::unexpected_token()),
                    };
                    match name.to_ascii_lowercase().as_str() {
                        "width" => Property::Width(length),
                        "height" => Property::Height(length),
                        "max-width" => Property::MaxWidth(length),
                        "min-width" => Property::MinWidth(length),
                        "min-height" => Property::MinHeight(length),
                        _ => Property::MaxHeight(length),
                    }
                }
                "font-family" => Property::Family(input.parse_comma_separated(font_family)?),
                "src" => {
                    Property::FontSource(input.parse_comma_separated_ignoring_errors(font_url))
                }
                "font-size" => {
                    let size = match input.next()?.clone() {
                        Token::Ident(name) if name.eq_ignore_ascii_case("xx-small") => {
                            Size::Reference(153.0 / 256.0)
                        }
                        Token::Ident(name) if name.eq_ignore_ascii_case("x-small") => {
                            Size::Reference(192.0 / 256.0)
                        }
                        Token::Ident(name) if name.eq_ignore_ascii_case("small") => {
                            Size::Reference(227.0 / 256.0)
                        }
                        Token::Ident(name) if name.eq_ignore_ascii_case("medium") => {
                            Size::Reference(1.0)
                        }
                        Token::Ident(name) if name.eq_ignore_ascii_case("large") => {
                            Size::Reference(307.0 / 256.0)
                        }
                        Token::Ident(name) if name.eq_ignore_ascii_case("x-large") => {
                            Size::Reference(384.0 / 256.0)
                        }
                        Token::Ident(name) if name.eq_ignore_ascii_case("xx-large") => {
                            Size::Reference(2.0)
                        }
                        Token::Ident(name) if name.eq_ignore_ascii_case("xxx-large") => {
                            Size::Reference(3.0)
                        }
                        Token::Ident(name) if name.eq_ignore_ascii_case("smaller") => {
                            Size::Parent(0.8)
                        }
                        Token::Ident(name) if name.eq_ignore_ascii_case("larger") => {
                            Size::Parent(1.25)
                        }
                        Token::Percentage { unit_value, .. } => Size::Parent(unit_value),
                        Token::Dimension { value, unit, .. } if unit.eq_ignore_ascii_case("em") => {
                            Size::Parent(value)
                        }
                        Token::Dimension { value, unit, .. }
                            if unit.eq_ignore_ascii_case("rem") =>
                        {
                            Size::Root(value)
                        }
                        Token::Dimension { value, unit, .. } if unit.eq_ignore_ascii_case("px") => {
                            Size::Reference(value / 16.0)
                        }
                        Token::Dimension { value, unit, .. } if unit.eq_ignore_ascii_case("pt") => {
                            Size::Reference(value / 12.0)
                        }
                        _ => return Err(ParseError::unexpected_token()),
                    };
                    let scale = match size {
                        Size::Parent(value) | Size::Root(value) | Size::Reference(value) => value,
                    };
                    if !(0.125..=8.0).contains(&scale) {
                        return Err(ParseError::unexpected_token());
                    }
                    Property::Size(size)
                }
                "font-weight" => {
                    let weight = match input.next()?.clone() {
                        Token::Number { value, .. } if (1.0..=1000.0).contains(&value) => {
                            Weight::Fixed(FontWeight::new(value))
                        }
                        Token::Ident(keyword) if keyword.eq_ignore_ascii_case("normal") => {
                            Weight::Fixed(FontWeight::NORMAL)
                        }
                        Token::Ident(keyword) if keyword.eq_ignore_ascii_case("bold") => {
                            Weight::Fixed(FontWeight::BOLD)
                        }
                        Token::Ident(keyword) if keyword.eq_ignore_ascii_case("bolder") => {
                            Weight::Bolder
                        }
                        Token::Ident(keyword) if keyword.eq_ignore_ascii_case("lighter") => {
                            Weight::Lighter
                        }
                        _ => return Err(ParseError::unexpected_token()),
                    };
                    Property::Weight(weight)
                }
                "font-style" => {
                    let style = match input.expect_ident()?.to_ascii_lowercase().as_str() {
                        "normal" => FontStyle::Normal,
                        "italic" => FontStyle::Italic,
                        "oblique" => FontStyle::Oblique(None),
                        _ => return Err(ParseError::unexpected_token()),
                    };
                    Property::Slant(style)
                }
                "font-optical-sizing" => Property::OpticalSizing(
                    match input.expect_ident()?.to_ascii_lowercase().as_str() {
                        "auto" => true,
                        "none" => false,
                        _ => return Err(ParseError::unexpected_token()),
                    },
                ),
                "font-variant" => {
                    let mut common = None;
                    let mut extra = LigatureExtras::default();
                    let mut caps = VariantCaps::Normal;
                    let mut numeric = VariantNumeric::default();
                    let mut east_asian = VariantEastAsian::default();
                    let mut position = VariantPosition::Normal;
                    let mut historical = false;
                    let mut seen_caps = false;
                    let mut seen_position = false;
                    let mut count = 0;
                    loop {
                        let value = input.expect_ident()?.to_ascii_lowercase();
                        count += 1;
                        if count > 16 {
                            return Err(ParseError::unexpected_token());
                        }
                        if value == "normal" || value == "none" {
                            if count != 1 {
                                return Err(ParseError::unexpected_token());
                            }
                            input.expect_exhausted()?;
                            if value == "none" {
                                common = Some(false);
                                extra = LigatureExtras {
                                    contextual: Some(false),
                                    discretionary: Some(false),
                                    historical: Some(false),
                                };
                            }
                            break;
                        }
                        if let Some(value) = VariantCaps::parse(value.as_str()) {
                            if seen_caps {
                                return Err(ParseError::unexpected_token());
                            }
                            seen_caps = true;
                            caps = value;
                        } else if let Some(bit) = VariantNumeric::bit(value.as_str()) {
                            if numeric.0 & VariantNumeric::group(bit) != 0 {
                                return Err(ParseError::unexpected_token());
                            }
                            numeric.0 |= bit;
                        } else if let Some(bit) = VariantEastAsian::bit(value.as_str()) {
                            if east_asian.0 & VariantEastAsian::group(bit) != 0 {
                                return Err(ParseError::unexpected_token());
                            }
                            east_asian.0 |= bit;
                        } else if let Some(value) = VariantPosition::parse(value.as_str()) {
                            if seen_position {
                                return Err(ParseError::unexpected_token());
                            }
                            seen_position = true;
                            position = value;
                        } else if value == "historical-forms" {
                            if historical {
                                return Err(ParseError::unexpected_token());
                            }
                            historical = true;
                        } else if !ligature_keyword(value.as_str(), &mut common, &mut extra) {
                            return Err(ParseError::unexpected_token());
                        }
                        if input.is_exhausted() {
                            break;
                        }
                    }
                    Property::VariantShorthand(
                        common.unwrap_or(true),
                        extra,
                        caps,
                        numeric,
                        east_asian,
                        position,
                        historical,
                    )
                }
                "font-variant-caps" => {
                    let value = input.expect_ident()?.to_ascii_lowercase();
                    Property::VariantCaps(
                        VariantCaps::parse(value.as_str())
                            .ok_or_else(ParseError::unexpected_token)?,
                    )
                }
                "font-variant-numeric" => Property::VariantNumeric(parse_numeric_variant(input)?),
                "font-variant-east-asian" => {
                    Property::VariantEastAsian(parse_east_asian_variant(input)?)
                }
                "font-variant-position" => {
                    let value = input.expect_ident()?.to_ascii_lowercase();
                    Property::VariantPosition(
                        VariantPosition::parse(value.as_str())
                            .ok_or_else(ParseError::unexpected_token)?,
                    )
                }
                "font-variant-alternates" => Property::VariantHistorical(
                    match input.expect_ident()?.to_ascii_lowercase().as_str() {
                        "normal" => false,
                        "historical-forms" => true,
                        _ => return Err(ParseError::unexpected_token()),
                    },
                ),
                "font-variant-ligatures" | "-webkit-font-variant-ligatures" => {
                    let (common, extra) = parse_ligatures(input)?;
                    Property::LigatureShorthand(common, extra)
                }
                "direction" => {
                    Property::Direction(match input.expect_ident()?.to_ascii_lowercase().as_str() {
                        "ltr" => parley::BaseDirection::Ltr,
                        "rtl" => parley::BaseDirection::Rtl,
                        _ => return Err(ParseError::unexpected_token()),
                    })
                }
                "line-height" => {
                    if input
                        .try_parse(|input| input.expect_ident_matching("normal"))
                        .is_ok()
                    {
                        Property::LineHeight(None)
                    } else {
                        let before = input.state();
                        let length = match input.next()?.clone() {
                            Token::Percentage { unit_value, .. }
                                if (0.0..=8.0).contains(&unit_value) =>
                            {
                                Some(LineHeightLength::Relative(unit_value))
                            }
                            Token::Dimension { value, unit, .. }
                                if unit.eq_ignore_ascii_case("em")
                                    && (0.0..=8.0).contains(&value) =>
                            {
                                Some(LineHeightLength::Relative(value))
                            }
                            Token::Dimension { value, unit, .. }
                                if unit.eq_ignore_ascii_case("rem")
                                    && (0.0..=8.0).contains(&value) =>
                            {
                                Some(LineHeightLength::DefaultFont(value))
                            }
                            Token::Dimension { value, unit, .. }
                                if (unit.eq_ignore_ascii_case("ex")
                                    || unit.eq_ignore_ascii_case("ch"))
                                    && (0.0..=16.0).contains(&value) =>
                            {
                                Some(LineHeightLength::Relative(value * 0.5))
                            }
                            Token::Dimension { value, unit, .. }
                                if unit.eq_ignore_ascii_case("px")
                                    && (0.0..=4096.0).contains(&value) =>
                            {
                                Some(LineHeightLength::Pixels(value))
                            }
                            _ => None,
                        };
                        if let Some(length) = length {
                            Property::LengthLineHeight(length)
                        } else {
                            input.reset(&before);
                            let height = input.expect_number()?;
                            if !(1.0..=3.0).contains(&height) {
                                return Err(ParseError::unexpected_token());
                            }
                            Property::LineHeight(Some(height))
                        }
                    }
                }
                "list-style-type" | "list-style" => {
                    let mut marker = None;
                    while !input.is_exhausted() {
                        let token = input.expect_ident()?.to_ascii_lowercase();
                        if token == "inside" || token == "outside" {
                            continue;
                        }
                        marker = Some(
                            crate::list_marker::MarkerKind::css(&token)
                                .ok_or_else(ParseError::unexpected_token)?,
                        );
                    }
                    Property::Marker(marker.ok_or_else(ParseError::unexpected_token)?)
                }
                "background" | "background-color" => Property::Background(background(input)?),
                "text-align" => {
                    Property::Alignment(match input.expect_ident()?.to_ascii_lowercase().as_str() {
                        "start" => Alignment::Start,
                        "end" => Alignment::End,
                        "left" => Alignment::Left,
                        "right" => Alignment::Right,
                        "center" => Alignment::Center,
                        "justify" => Alignment::Justify,
                        _ => return Err(ParseError::unexpected_token()),
                    })
                }
                "text-align-last" | "-epub-text-align-last" => Property::LastLineAlignment(
                    match input.expect_ident()?.to_ascii_lowercase().as_str() {
                        "auto" => None,
                        "start" => Some(Alignment::Start),
                        "end" => Some(Alignment::End),
                        "left" => Some(Alignment::Left),
                        "right" => Some(Alignment::Right),
                        "center" => Some(Alignment::Center),
                        "justify" => Some(Alignment::Justify),
                        _ => return Err(ParseError::unexpected_token()),
                    },
                ),
                "text-transform" => Property::TextTransform(
                    match input.expect_ident()?.to_ascii_lowercase().as_str() {
                        "none" => TextTransform::None,
                        "uppercase" => TextTransform::Uppercase,
                        "lowercase" => TextTransform::Lowercase,
                        "capitalize" => TextTransform::Capitalize,
                        _ => return Err(ParseError::unexpected_token()),
                    },
                ),
                "line-break" => {
                    let (breaking, loose) =
                        match input.expect_ident()?.to_ascii_lowercase().as_str() {
                            "auto" => (None, false),
                            "loose" => (Some(LineBreak::Loose), false),
                            "normal" => (Some(LineBreak::Normal), false),
                            "strict" => (Some(LineBreak::Strict), false),
                            "anywhere" => (Some(LineBreak::Anywhere), false),
                            "-cr-loose" => (None, true),
                            _ => return Err(ParseError::unexpected_token()),
                        };
                    Property::LineBreak(breaking, loose)
                }
                "letter-spacing" => {
                    let length = if input
                        .try_parse(|p| p.expect_ident_matching("normal"))
                        .is_ok()
                    {
                        CssLength::default()
                    } else {
                        parse_css_length(input)?
                    };
                    Property::LetterSpacing(length)
                }
                "word-spacing" => {
                    let length = if input
                        .try_parse(|p| p.expect_ident_matching("normal"))
                        .is_ok()
                    {
                        Some(CssLength::default())
                    } else {
                        match input.try_parse(parse_css_length) {
                            Ok(
                                length @ (CssLength::Em(value)
                                | CssLength::RootEm(value)
                                | CssLength::Pixels(value)
                                | CssLength::Physical(value)),
                            ) if value >= 0.0 => Some(length),
                            Ok(_) => None,
                            Err(_) => {
                                // Preserve recognized but unsupported CSS
                                // dimensions as fallback; invalid
                                // identifiers must not win the cascade.
                                match input.next()?.clone() {
                                    Token::Dimension { value, unit, .. }
                                        if value.is_finite()
                                            && matches!(
                                                unit.to_ascii_lowercase().as_str(),
                                                "ch" | "ex"
                                                    | "cap"
                                                    | "ic"
                                                    | "lh"
                                                    | "rlh"
                                                    | "vw"
                                                    | "vh"
                                                    | "vi"
                                                    | "vb"
                                                    | "vmin"
                                                    | "vmax"
                                                    | "svw"
                                                    | "svh"
                                                    | "lvw"
                                                    | "lvh"
                                                    | "dvw"
                                                    | "dvh"
                                                    | "cqw"
                                                    | "cqh"
                                                    | "cqi"
                                                    | "cqb"
                                                    | "cqmin"
                                                    | "cqmax"
                                            ) =>
                                    {
                                        None
                                    }
                                    _ => return Err(ParseError::unexpected_token()),
                                }
                            }
                        }
                    };
                    Property::WordSpacing(length)
                }
                "caption-side" => Property::CaptionBottom(
                    match input.expect_ident()?.to_ascii_lowercase().as_str() {
                        "top" => false,
                        "bottom" => true,
                        _ => return Err(ParseError::unexpected_token()),
                    },
                ),
                "ruby-position" => Property::RubyPosition(
                    match input.expect_ident()?.to_ascii_lowercase().as_str() {
                        "alternate" => RubyPosition::Alternate,
                        "over" => RubyPosition::Over,
                        "under" => RubyPosition::Under,
                        _ => return Err(ParseError::unexpected_token()),
                    },
                ),
                "white-space" => Property::WhiteSpace(
                    match input.expect_ident()?.to_ascii_lowercase().as_str() {
                        "normal" => WhiteSpace::Normal,
                        "nowrap" => WhiteSpace::NoWrap,
                        "pre" => WhiteSpace::Pre,
                        "pre-wrap" => WhiteSpace::PreWrap,
                        "pre-line" => WhiteSpace::PreLine,
                        "break-spaces" => WhiteSpace::BreakSpaces,
                        _ => return Err(ParseError::unexpected_token()),
                    },
                ),
                "tab-size" => {
                    let value = input.expect_number()?;
                    if !value.is_finite() || !(0.0..=32.0).contains(&value) {
                        return Err(ParseError::unexpected_token());
                    }
                    Property::TabSize(value)
                }
                "word-break" => {
                    Property::WordBreak(match input.expect_ident()?.to_ascii_lowercase().as_str() {
                        "normal" => WordBreak::Normal,
                        "break-all" => WordBreak::BreakAll,
                        "keep-all" => WordBreak::KeepAll,
                        _ => return Err(ParseError::unexpected_token()),
                    })
                }
                "hyphens" | "hyphenate" | "-webkit-hyphens" | "-epub-hyphens"
                | "adobe-hyphenate" | "adobe-text-layout" => {
                    Property::Hyphens(match input.expect_ident()?.to_ascii_lowercase().as_str() {
                        "none" => Hyphens::None,
                        "manual" => Hyphens::Manual,
                        "auto" => Hyphens::Auto,
                        "optimizespeed" if name.eq_ignore_ascii_case("adobe-text-layout") => {
                            Hyphens::None
                        }
                        "optimizequality" if name.eq_ignore_ascii_case("adobe-text-layout") => {
                            Hyphens::Auto
                        }
                        "explicit" if name.eq_ignore_ascii_case("adobe-hyphenate") => Hyphens::Auto,
                        _ => return Err(ParseError::unexpected_token()),
                    })
                }
                "writing-mode" | "-epub-writing-mode" | "-webkit-writing-mode" => {
                    Property::WritingMode(
                        match input.expect_ident()?.to_ascii_lowercase().as_str() {
                            "horizontal-tb" => WritingMode::Horizontal,
                            "vertical-rl" => WritingMode::VerticalRl,
                            "vertical-lr" => WritingMode::VerticalLr,
                            _ => return Err(ParseError::unexpected_token()),
                        },
                    )
                }
                _ => return Err(ParseError::unexpected_token()),
            };
            input.expect_exhausted()?;
            Ok(property)
        })?;
        let important = input.try_parse(parse_important).is_ok();
        input.expect_exhausted()?;
        Ok(Declaration {
            property,
            important,
        })
    }
}

impl<'i> QualifiedRuleParser<'i> for Properties {
    type Prelude = ();
    type QualifiedRule = Declaration;
    type Error = ();
}

impl<'i> AtRuleParser<'i> for Properties {
    type Prelude = ();
    type AtRule = Declaration;
    type Error = ();
}

impl<'i> RuleBodyItemParser<'i, Declaration, ()> for Properties {
    fn parse_declarations(&self) -> bool {
        true
    }
    fn parse_qualified(&self) -> bool {
        false
    }
}

fn line_minimum(input: &mut Parser<'_>) -> CssResult<Option<u8>> {
    if let Ok(value) = input.try_parse(Parser::expect_integer) {
        return match value {
            1..=9 => Ok(Some(value as u8)),
            _ => Err(ParseError::unexpected_token()),
        };
    }
    match input.expect_ident()?.to_ascii_lowercase().as_str() {
        "initial" => Ok(Some(1)),
        "inherit" | "unset" | "revert" => Ok(None),
        _ => Err(ParseError::unexpected_token()),
    }
}

fn break_edge(input: &mut Parser<'_>) -> CssResult<Option<crate::publisher_breaks::Edge>> {
    use crate::publisher_breaks::Edge;
    Ok(match input.expect_ident()?.to_ascii_lowercase().as_str() {
        "always" | "page" => Some(Edge::Force),
        "avoid" | "avoid-page" => Some(Edge::Avoid),
        "auto" | "initial" | "unset" | "revert" => Some(Edge::Auto),
        "inherit" => None,
        _ => return Err(ParseError::unexpected_token()),
    })
}

fn parse_indent(input: &mut Parser<'_>) -> CssResult<Option<TextIndent>> {
    if let Ok(keyword) = input.try_parse(Parser::expect_ident_cloned) {
        match keyword.to_ascii_lowercase().as_str() {
            "initial" => return Ok(Some(TextIndent::default())),
            "inherit" | "unset" | "revert" => return Ok(None),
            "hanging" => {
                return Ok(Some(TextIndent {
                    length: parse_css_length(input)?,
                    hanging: true,
                }));
            }
            _ => return Err(ParseError::unexpected_token()),
        }
    }
    let length = parse_css_length(input)?;
    let hanging = input
        .try_parse(|input| input.expect_ident_matching("hanging"))
        .is_ok();
    Ok(Some(TextIndent { length, hanging }))
}

fn parse_padding(input: &mut Parser<'_>, shorthand: bool) -> CssResult<[Option<CssLength>; 4]> {
    Ok(parse_box_edges(input, shorthand, false)?.map(|edge| edge.map(BoxMargin::length)))
}

// Expand CSS top/right/bottom/left into the two horizontal and two vertical sides.
fn parse_box_edges(
    input: &mut Parser<'_>,
    shorthand: bool,
    margins: bool,
) -> CssResult<[Option<BoxMargin>; 4]> {
    let before = input.state();
    if let Ok(keyword) = input.try_parse(Parser::expect_ident_cloned) {
        match keyword.to_ascii_lowercase().as_str() {
            "inherit" => return Ok([None; 4]),
            "initial" | "unset" | "revert" => return Ok([Some(BoxMargin::default()); 4]),
            "auto" if margins => input.reset(&before),
            _ => return Err(ParseError::unexpected_token()),
        }
    }
    let mut values = [BoxMargin::default(); 4];
    let mut count = 0;
    while !input.is_exhausted() {
        if count == if shorthand { 4 } else { 1 } {
            return Err(ParseError::unexpected_token());
        }
        let value = if margins && input.try_parse(|p| p.expect_ident_matching("auto")).is_ok() {
            BoxMargin::Auto
        } else {
            let length = parse_css_length(input)?;
            if !margins && length.pixels(100, 1.0) < 0.0 {
                return Err(ParseError::unexpected_token());
            }
            BoxMargin::Length(length)
        };
        values[count] = value;
        count += 1;
    }
    match count {
        1 => Ok([Some(values[0]); 4]),
        2 => Ok([
            Some(values[1]),
            Some(values[1]),
            Some(values[0]),
            Some(values[0]),
        ]),
        3 => Ok([
            Some(values[1]),
            Some(values[1]),
            Some(values[0]),
            Some(values[2]),
        ]),
        4 => Ok([
            Some(values[3]),
            Some(values[1]),
            Some(values[0]),
            Some(values[2]),
        ]),
        _ => Err(ParseError::unexpected_token()),
    }
}

fn parse_css_length(input: &mut Parser<'_>) -> CssResult<CssLength> {
    match input.next()?.clone() {
        Token::Number { value: 0.0, .. } => Ok(CssLength::Pixels(0.0)),
        Token::Percentage { unit_value, .. } if unit_value.is_finite() => {
            Ok(CssLength::Fraction(unit_value))
        }
        Token::Dimension { value, unit, .. } if value.is_finite() => {
            Ok(match unit.to_ascii_lowercase().as_str() {
                "em" => CssLength::Em(value),
                "rem" => CssLength::RootEm(value),
                "px" => CssLength::Pixels(value),
                "pt" => CssLength::Physical(value * (96.0 / 72.0)),
                "pc" => CssLength::Physical(value * 16.0),
                "in" => CssLength::Physical(value * 96.0),
                "cm" => CssLength::Physical(value * (96.0 / 2.54)),
                "mm" => CssLength::Physical(value * (96.0 / 25.4)),
                "q" => CssLength::Physical(value * (96.0 / 101.6)),
                _ => return Err(ParseError::unexpected_token()),
            })
        }
        _ => Err(ParseError::unexpected_token()),
    }
}

fn font_family<'i>(input: &mut Parser<'i>) -> CssResult<Family> {
    if let Ok(quoted) = input.try_parse(Parser::expect_string_cloned) {
        return Ok(Family::Named(Arc::from(quoted.as_ref())));
    }
    let mut name = input.expect_ident_cloned()?.to_string();
    while let Ok(part) = input.try_parse(Parser::expect_ident_cloned) {
        name.push(' ');
        name.push_str(&part);
    }
    if matches!(
        name.to_ascii_lowercase().as_str(),
        "inherit" | "initial" | "unset" | "revert" | "revert-layer"
    ) {
        return Err(ParseError::unexpected_token());
    }
    Ok(GenericFamily::parse(&name).map_or_else(|| Family::Named(Arc::from(name)), Family::Generic))
}

fn font_url<'i>(input: &mut Parser<'i>) -> CssResult<String> {
    let url = input.expect_url()?.to_string();
    if input
        .try_parse(|parser| parser.expect_function_matching("format"))
        .is_ok()
    {
        input.parse_nested_block(|format| {
            let format_name = format.expect_ident_or_string()?.to_ascii_lowercase();
            if !matches!(
                format_name.as_str(),
                "truetype" | "opentype" | "woff" | "woff2"
            ) {
                return Err(ParseError::unexpected_token());
            }
            Ok(())
        })?;
    }
    Ok(url)
}

fn background<'i>(input: &mut Parser<'i>) -> CssResult<Option<u8>> {
    match input.next()?.clone() {
        Token::Hash(value) | Token::IDHash(value) => {
            let (r, g, b) = match value.len() {
                3 => {
                    let rgb = u16::from_str_radix(&value, 16)
                        .map_err(|_| ParseError::unexpected_token())?;
                    ((rgb >> 8) * 17, ((rgb >> 4) & 15) * 17, (rgb & 15) * 17)
                }
                6 => {
                    let rgb = u32::from_str_radix(&value, 16)
                        .map_err(|_| ParseError::unexpected_token())?;
                    (
                        (rgb >> 16) as u16,
                        ((rgb >> 8) & 255) as u16,
                        (rgb & 255) as u16,
                    )
                }
                _ => return Err(ParseError::unexpected_token()),
            };
            Ok(Some(((r * 77 + g * 150 + b * 29 + 128) >> 8) as u8))
        }
        Token::Ident(value) if value.eq_ignore_ascii_case("transparent") => Ok(None),
        Token::Ident(value) if value.eq_ignore_ascii_case("white") => Ok(Some(255)),
        Token::Ident(value) if value.eq_ignore_ascii_case("lightgray") => Ok(Some(211)),
        Token::Ident(value) if value.eq_ignore_ascii_case("gray") => Ok(Some(128)),
        Token::Ident(value) if value.eq_ignore_ascii_case("black") => Ok(Some(0)),
        _ => Err(ParseError::unexpected_token()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::Source;

    #[cfg(feature = "kindle")]
    #[test]
    fn linked_css_oversized_and_over_budget_sheets_do_not_hide_later_valid_rules() {
        use std::io::{Cursor, Read, Write};
        let original = std::fs::read(crate::test_fixture("quote-spine-long.epub")).unwrap();
        let mut input = zip::ZipArchive::new(Cursor::new(original)).unwrap();
        let mut output = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default();
        for index in 0..input.len() {
            let mut entry = input.by_index(index).unwrap();
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).unwrap();
            output.start_file(entry.name(), options).unwrap();
            output.write_all(&bytes).unwrap();
        }
        let padded = |selector: &str, padding| {
            format!(
                ".{selector} {{ font-weight: 700; }} /*{}*/",
                "x".repeat(padding)
            )
        };
        for (name, text) in [
            ("oversized.css", padded("oversized", MAX_SHEET_BYTES)),
            ("first.css", padded("first", 90 * 1024)),
            ("second.css", padded("second", 90 * 1024)),
            ("over-budget.css", padded("overbudget", 90 * 1024)),
            ("last.css", ".last { font-weight: 700; }".to_owned()),
        ] {
            output.start_file(name, options).unwrap();
            output.write_all(text.as_bytes()).unwrap();
        }
        let mut book = Book::open_snapshot(output.finish().unwrap().into_inner().into()).unwrap();
        let source = Source::parse(r#"<html><head><link rel="stylesheet" href="oversized.css"/><link rel="stylesheet" href="first.css"/><link rel="stylesheet" href="second.css"/><link rel="stylesheet" href="over-budget.css"/><link rel="stylesheet" href="last.css"/></head><body><p class="oversized">a</p><p class="first">b</p><p class="second">c</p><p class="overbudget">d</p><p class="last">e</p></body></html>"#).unwrap();
        let (styled, _) = BookStyle::load_sheets(&mut book, "chapter-00.xhtml", &source);
        let styles = styled.compute(&source.elements);
        let weight = |class: &str| {
            let index = source
                .elements
                .iter()
                .position(|element| element.classes == class)
                .unwrap();
            styles[index].font_weight
        };
        assert_eq!(weight("oversized"), FontWeight::NORMAL);
        assert_eq!(weight("first"), FontWeight::BOLD);
        assert_eq!(weight("second"), FontWeight::BOLD);
        assert_eq!(weight("overbudget"), FontWeight::NORMAL);
        assert_eq!(weight("last"), FontWeight::BOLD);
    }

    fn computed(xhtml: &str) -> (Source, Vec<Style>) {
        let source = Source::parse(xhtml).unwrap();
        let mut book = Book::open(&crate::test_fixture("field-notes-epub3.epub")).unwrap();
        let chapter = book.chapters[0].href.clone();
        let fonts = [crate::test_fixture("fonts/Silkscreen-Regular.ttf")];
        let mut fonts = crate::font_context(&fonts).unwrap();
        let styles =
            BookStyle::load(&mut book, &chapter, &source, &mut fonts).compute(&source.elements);
        (source, styles)
    }

    #[test]
    fn shared_embedded_bytes_keep_distinct_declared_font_weight_roles() {
        let mut book = Book::open(&crate::test_fixture("publisher-faces.epub")).unwrap();
        let href = book.chapters[0].href.clone();
        let source = Source::parse(&book.chapter(0).unwrap()).unwrap();
        let mut context =
            crate::font_context(&[crate::test_fixture("fonts/NotoSans-Regular.ttf")]).unwrap();
        let sheet = BookStyle::load(&mut book, &href, &source, &mut context);
        assert_eq!(sheet.font_weights.len(), 5);
        assert_eq!(
            sheet
                .font_weights
                .values()
                .filter(|&&weight| weight == 400.0)
                .count(),
            3
        );
        assert_eq!(
            sheet
                .font_weights
                .values()
                .filter(|&&weight| weight == 700.0)
                .count(),
            2
        );
        let styles = sheet.compute(&source.elements);
        assert!(
            styles
                .iter()
                .all(|style| Arc::ptr_eq(&style.font_weights, &sheet.font_weights))
        );
    }

    #[test]
    fn author_cascade_preserves_ancestor_typography_and_inline_resets() {
        let (source, styles) = computed(
            r##"<html style="font-size: 125%">
                <head><style>
                    #cell { font-weight: 200; font-style: italic; }
                    section.chapter table p.copy { font-weight: 600 !important; font-weight: 300; }
                    p.copy { font-size: 110%; }
                    p.copy { font-size: 120%; }
                    .chapter span.size { font-size: 150%; font-weight: bolder; }
                    span.size { font-size: 200%; }
                    span.root-size { font-size: 2rem; line-height: normal; }
                    strong.plain { font-weight: normal; font-style: normal; }
                </style></head>
                <body style="font-size: 80%; font-family: Unavailable, sans-serif; text-align: right">
                    <section class="chapter" style="font-size: 1.5em; line-height: 1.8; background: #eee">
                        <table><tr><td><p id="cell" class="copy" style="font-weight: 700; font-style: normal">
                            Outer text <span id="large" class="size" style="font-style: italic">larger text</span>
                            <span id="root-size" class="root-size">root-sized text</span>
                            <strong id="plain" class="plain">plain text</strong>
                        </p></td></tr></table>
                    </section>
                </body></html>"##,
        );
        let style_of = |id| {
            let index = source
                .elements
                .iter()
                .position(|node| node.id.as_deref() == Some(id))
                .unwrap();
            &styles[index]
        };
        let cell = style_of("cell");
        assert!((cell.size_scale - 1.8).abs() < 0.001);
        assert_eq!(cell.font_family, Family::Generic(GenericFamily::SansSerif));
        assert_eq!(cell.font_weight, FontWeight::SEMI_BOLD);
        assert_eq!(cell.font_style, FontStyle::Normal);
        assert_eq!(cell.line_height, Some(1.8));
        assert_eq!(cell.alignment, Some(Alignment::Right));
        assert_eq!(cell.background, None);

        let large = style_of("large");
        assert!((large.size_scale - 2.7).abs() < 0.001);
        assert_eq!(large.font_weight, FontWeight::BLACK);
        assert_eq!(large.font_style, FontStyle::Italic);
        let root = style_of("root-size");
        assert_eq!(root.size_scale, 2.5);
        assert_eq!(root.line_height, None);
        let explicit = style_of("plain");
        assert_eq!(explicit.font_weight, FontWeight::NORMAL);
        assert_eq!(explicit.font_style, FontStyle::Normal);
    }

    #[test]
    fn horizontal_padding_cascades_independently_from_ancestor_boxes() {
        let (source, styles) = computed(
            r#"<html><head><style>
          div { padding: 0 4px 0 14px; } .card { padding-right: 22px !important }
        </style></head><body><div id="shell"><p id="normal">Ordinary</p>
        <p id="child" class="card" style="padding:2px 4px 6px 10%;padding-left:2em;padding-left:-1px">Padded</p>
        <p id="inherited" style="padding:inherit;padding-right:unset">Inherited</p>
        <p id="invalid" style="padding-left:8px;padding:2px auto;padding-right:17px 8px">Invalid</p>
        </div></body></html>"#,
        );
        for (id, expected) in [
            ("shell", [14., 4.]),
            ("normal", [0., 0.]),
            ("child", [40., 22.]),
            ("inherited", [14., 0.]),
            ("invalid", [8., 0.]),
        ] {
            let i = source
                .elements
                .iter()
                .position(|el| el.id.as_deref() == Some(id))
                .unwrap();
            assert_eq!(
                styles[i].padding.map(|p| p.pixels(300, 20.)),
                expected,
                "{id}"
            );
        }
    }

    #[test]
    fn widow_and_orphan_values_follow_the_cascade_and_stock_integer_range() {
        let (source, styles) = computed(
            r#"<html><head><style>
            body {orphans:3;widows:4} .special {widows:5 !important}
        </style></head><body>
        <p id="base">Inherited</p>
        <p id="explicit" class="special" style="orphans:1;widows:9">Explicit</p>
        <p id="reset" style="widows:initial;orphans:unset">Reset</p>
        <p id="invalid" style="widows:2;widows:10;orphans:0;orphans:1.5">Invalid</p>
        </body></html>"#,
        );
        for (id, want) in [
            ("base", (3, 4)),
            ("explicit", (1, 5)),
            ("reset", (3, 1)),
            ("invalid", (3, 2)),
        ] {
            let i = source
                .elements
                .iter()
                .position(|e| e.id.as_deref() == Some(id))
                .unwrap();
            assert_eq!((styles[i].orphans, styles[i].widows), want, "{id}");
        }
    }

    #[test]
    fn unrelated_publisher_selectors_do_not_displace_the_chapters_paragraph_indent() {
        let unrelated = |start| {
            (start..start + 190)
                .map(|n| format!(".appendix{n} p {{font-weight:bold}}"))
                .collect::<String>()
        };
        let markup = format!(
            r#"<html><head>
            <style>p {{text-indent:1em}} {}</style><style>{}</style>
            <style>.citation {{text-indent:-1em}}</style></head>
            <body><p class="citation">Reference to the tide tables.</p></body></html>"#,
            unrelated(0),
            unrelated(190)
        );
        let (source, styles) = computed(&markup);
        assert_eq!(
            styles[source.blocks[0].element]
                .text_indent
                .pixels(500, 32.0),
            -32.0
        );
    }

    #[test]
    fn paragraph_openers_match_only_adjacent_elements_and_direct_firsts_of_their_type() {
        let (source, styles) = computed(include_str!("../tests/fixtures/paragraph-opener.xhtml"));
        let indents = ["arrival", "walk", "weather", "return"].map(|id| {
            let i = source
                .elements
                .iter()
                .position(|e| e.id.as_deref() == Some(id))
                .unwrap();
            styles[i].text_indent.pixels(600, 32.0)
        });
        assert_eq!(indents, [0.0, 48.0, 0.0, 48.0]);

        let markup = r#"<html><head><style>
            p {text-indent:2em}
            h2+p, section>p:first-of-type {text-indent:0}
            .outer > .level .deep { text-indent:3px }
            .pair:first-child + p {text-indent:4px}
            h2 + p, p:unknown-primitiveos-pseudo {text-indent:19px !important}
        </style></head><body>
            <article><h2 hidden="hidden" id="silent">Hidden title</h2><!-- gap --> <p id="follows">A.</p></article>
            <article><h2>Title</h2><span hidden="hidden">Silent</span><p id="separated">B.</p></article>
            <article><h2>Open water</h2>Loose, visible words. <p id="after-text">I.</p></article>
            <article><h2>Open sky</h2><span>Inline note.</span><p id="after-span">J.</p></article>
            <section><p hidden="hidden">Silent first</p><p id="later">C.</p></section>
            <section><aside/><p id="section-start">D.</p><div><p id="nested">E.</p></div></section>
            <article class="outer"><div class="level"><aside><div class="level"><p id="backtrack" class="deep">F.</p></div></aside></div></article>
            <article><span class="pair"/> <p id="pair">G.</p></article>
            <article><span/><span class="pair"/><p id="not-pair">H.</p></article>
        </body></html>"#;
        let (src, styles) = computed(markup);
        for (id, expected) in [
            ("follows", 0.0),
            ("separated", 64.0),
            ("after-text", 0.0),
            ("after-span", 64.0),
            ("later", 64.0),
            ("section-start", 0.0),
            ("nested", 64.0),
            ("backtrack", 3.0),
            ("pair", 4.0),
            ("not-pair", 64.0),
        ] {
            let i = src
                .elements
                .iter()
                .position(|e| e.id.as_deref() == Some(id))
                .unwrap();
            assert_eq!(styles[i].text_indent.pixels(600, 32.0), expected, "{id}");
        }
        assert!(!src.fragments.contains_key("silent"));
        assert!(
            src.blocks
                .iter()
                .all(|b| !b.text.contains("Silent") && !b.text.contains("Hidden"))
        );
    }

    #[test]
    fn linked_and_inline_sheets_follow_their_document_order() {
        let (source, styles) = computed(
            r##"<html><head><style>.notice { background: #000; }</style>
                <link rel="stylesheet" href="../styles/reader.css" />
                </head><body><p class="notice">The last matching sheet wins.</p></body></html>"##,
        );
        assert_eq!(styles[source.blocks[0].element].background, Some(229));
    }

    #[test]
    fn publisher_styles_off_retains_semantic_heading_bold_and_italic() {
        let (source, published) = computed(
            r##"<html><head><style>strong, em {font-style: normal; font-weight: normal}</style></head>
                <body><h1 style="font-size: 100%; font-weight: normal">Title</h1>
                <p style="background: #d0d0d0"><strong>Bold</strong> <em>Italic</em></p></body></html>"##,
        );
        let plain = BookStyle::default().compute(&source.elements);
        let element = |tag| {
            source
                .elements
                .iter()
                .position(|element| element.tag == tag)
                .unwrap()
        };
        assert_eq!(published[element("h1")].size_scale, 1.0);
        assert_eq!(plain[element("h1")].size_scale, 1.4);
        assert_eq!(published[element("strong")].font_weight, FontWeight::NORMAL);
        assert_eq!(plain[element("strong")].font_weight, FontWeight::BOLD);
        assert_eq!(published[element("em")].font_style, FontStyle::Normal);
        assert_eq!(plain[element("em")].font_style, FontStyle::Italic);
        assert_eq!(published[element("p")].background, Some(208));
        assert_eq!(plain[element("p")].background, None);
    }
}
