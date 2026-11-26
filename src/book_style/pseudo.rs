//! CSS pseudo-element typography and generated text backed by the normal cascade.
pub(crate) mod quotes;

use super::{
    BookStyle, BoxMargin, CascadeOrigin, ComputedLineHeight, CssLength, CssResult, Display,
    Element, Family, LigatureExtras, LineHeightLength, Priority, Property, Rule, Size, Style,
    TextDecoration, TextTransform, VariantCaps, VariantEastAsian, VariantNumeric, VariantPosition,
    Weight, flow,
};
use cssparser::{Parser, Token};
use parley::{FontStyle, FontWeight, GenericFamily};
use std::{ops::Range, sync::Arc};

const MAX_CONTENT_PARTS: usize = 16;
const MAX_CONTENT_BYTES: usize = 1024;
const MAX_GENERATED_HOSTS: usize = 1024;
const MAX_FIRST_LINE_STYLES: usize = 4096;
pub(crate) const MAX_ATTRIBUTE_BYTES: usize = 64 * 1024;
const MAX_ATTRIBUTE_VALUE: usize = 256;
const MAX_BLOCK_EDGES: usize = 512;

#[derive(Clone, Copy)]
pub(crate) enum Side {
    Before,
    After,
}

#[derive(Clone, Copy)]
pub(crate) enum Target {
    Generated(Side),
    FirstLetter,
    FirstLine,
}

impl Target {
    pub(crate) fn parse(name: &str) -> Option<Self> {
        if name.eq_ignore_ascii_case("before") {
            Some(Self::Generated(Side::Before))
        } else if name.eq_ignore_ascii_case("after") {
            Some(Self::Generated(Side::After))
        } else if name.eq_ignore_ascii_case("first-letter") {
            Some(Self::FirstLetter)
        } else if name.eq_ignore_ascii_case("first-line") {
            Some(Self::FirstLine)
        } else {
            None
        }
    }
}

pub(crate) struct Edge {
    pub(crate) at: usize,
    pub(crate) host: usize,
    pub(crate) side: Side,
}

#[derive(Default)]
pub(crate) struct Edges {
    stack: Vec<(usize, usize, usize)>,
    edges: Vec<Edge>,
    exhausted: bool,
}

impl Edges {
    pub(crate) fn open(&mut self, raw_end: usize, at: usize, host: usize) {
        if self.exhausted {
            return;
        }
        if self.edges.len() + self.stack.len() + 2 > MAX_BLOCK_EDGES {
            self.exhausted = true;
            self.stack.clear();
            self.edges.clear();
            return;
        }
        self.stack.push((raw_end, at, host));
        self.edges.push(Edge {
            at,
            host,
            side: Side::Before,
        });
    }

    pub(crate) fn close_before(&mut self, raw_at: usize, at: usize) {
        while self
            .stack
            .last()
            .is_some_and(|(raw_end, ..)| *raw_end <= raw_at)
        {
            let (_, open_at, host) = self.stack.pop().unwrap();
            self.edges.push(Edge {
                at: open_at.max(at),
                host,
                side: Side::After,
            });
        }
    }

    pub(crate) fn finish(mut self, len: usize, budget: &mut usize) -> Box<[Edge]> {
        self.close_before(usize::MAX, len);
        if self.edges.len() > *budget {
            return Box::default();
        }
        *budget -= self.edges.len();
        for edge in &mut self.edges {
            edge.at = edge.at.min(len);
        }
        self.edges.sort_by_key(|edge| edge.at);
        self.edges.into_boxed_slice()
    }
}

pub(crate) enum ContentPart {
    Text(String),
    Attribute(String),
    Quote(quotes::Token),
}

pub(super) fn parse_content(input: &mut Parser<'_>) -> CssResult<Vec<ContentPart>> {
    let mut parts = Vec::new();
    while !input.is_exhausted() {
        if parts.len() == MAX_CONTENT_PARTS {
            return Err(super::ParseError::unexpected_token());
        }
        let token = input.next()?.clone();
        let part = match token {
            Token::QuotedString(text) if text.len() <= MAX_CONTENT_BYTES => {
                ContentPart::Text(text.to_string())
            }
            Token::Function(name) if name.eq_ignore_ascii_case("attr") => ContentPart::Attribute(
                input.parse_nested_block(|input| Ok(input.expect_ident_cloned()?.to_string()))?,
            ),
            Token::Ident(word)
                if word.eq_ignore_ascii_case("none") || word.eq_ignore_ascii_case("normal") =>
            {
                input.expect_exhausted()?;
                return Ok(Vec::new());
            }
            Token::Ident(word) => ContentPart::Quote(
                quotes::Token::parse(&word).ok_or_else(super::ParseError::unexpected_token)?,
            ),
            _ => return Err(super::ParseError::unexpected_token()),
        };
        parts.push(part);
    }
    Ok(parts)
}

pub(crate) fn read_attributes(
    node: roxmltree::Node<'_, '_>,
    budget: &mut usize,
) -> Box<[(String, String)]> {
    node.attributes()
        .filter_map(|attr| {
            let (name, value) = (attr.name(), attr.value());
            // id and class are already retained for selector matching.
            let bytes = name.len() + value.len() + 16;
            if matches!(name, "id" | "class")
                || name.len() > 64
                || value.len() > MAX_ATTRIBUTE_VALUE
                || bytes > *budget
            {
                None
            } else {
                *budget -= bytes;
                Some((name.to_owned(), value.to_owned()))
            }
        })
        .collect()
}

#[derive(Clone)]
pub(crate) struct Generated {
    pub(crate) text: String,
    pub(crate) style: Style,
}

pub(crate) type Pair = [Option<Generated>; 2];

/// Overrides inherit from the styled text node, which may be inside emphasis
/// or a link, independently of the selector's actual block host.
#[derive(Clone)]
pub(crate) struct FirstLetter {
    typography: Typography,
    pub(crate) placement: Option<Placement>,
    trailing_margin: BoxMargin,
}

#[derive(Clone, Copy)]
pub(crate) enum Placement {
    Float,
    Numeric(InitialLetter),
}

#[derive(Clone, Copy)]
pub(crate) struct InitialLetter {
    pub(crate) size: f32,
    pub(crate) sink: u8,
}

pub(super) fn parse_initial_letter(input: &mut Parser<'_>) -> CssResult<Option<InitialLetter>> {
    fn keyword(input: &mut Parser<'_>) -> Result<bool, cssparser::BasicParseError> {
        input
            .try_parse(|p| p.expect_ident_matching("raise"))
            .map(|()| true)
            .or_else(|_| input.expect_ident_matching("drop").map(|()| false))
    }
    if input
        .try_parse(|p| p.expect_ident_matching("normal"))
        .is_ok()
    {
        return Ok(None);
    }
    let leading = input.try_parse(keyword).ok();
    let size = input.expect_number()?;
    let options = leading.or_else(|| input.try_parse(keyword).ok());
    let sink = match options {
        Some(true) => 1,
        Some(false) => size.ceil() as i32,
        None if input.is_exhausted() => size.ceil() as i32,
        None => input.expect_integer()?,
    };
    if !(1.0..=255.0).contains(&size) || !(1..=255).contains(&sink) {
        return Err(super::ParseError::unexpected_token());
    }
    Ok(Some(InitialLetter {
        size,
        sink: sink as u8,
    }))
}

#[derive(Clone, Default)]
struct Typography {
    visibility: Option<flow::Visibility>,
    size: Option<Size>,
    root_scale: f32,
    family: Option<Family>,
    weight: Option<Weight>,
    slant: Option<FontStyle>,
    decoration: Option<TextDecoration>,
    text_ink: Option<Option<u8>>,
    line_height: Option<Option<f32>>,
    length_line_height: Option<LineHeightLength>,
    letter_spacing: Option<CssLength>,
    common_ligatures: Option<bool>,
    ligature_extras: Option<LigatureExtras>,
    variant_caps: Option<VariantCaps>,
    variant_numeric: Option<VariantNumeric>,
    variant_east_asian: Option<VariantEastAsian>,
    variant_position: Option<VariantPosition>,
    variant_historical: Option<bool>,
    optical_sizing: Option<bool>,
    text_transform: Option<TextTransform>,
    dpi: u16,
}

fn computed_spacing(length: CssLength, font: f32, root: f32) -> CssLength {
    let length = match length {
        CssLength::Fraction(value) => CssLength::Em(value),
        value => value,
    };
    length.computed(font, root)
}

impl Typography {
    fn apply(&self, base: &Style) -> Style {
        let mut style = base.clone();
        style.generated = None;
        style.first_letter = None;
        style.first_line = None;
        style.display = Display::Inline;
        if let Some(visibility) = self.visibility {
            style.visibility = visibility;
        }
        if let Some(size) = self.size {
            style.size_scale = match size {
                Size::Parent(scale) => base.size_scale * scale,
                Size::Root(scale) => self.root_scale * scale,
                Size::Reference(scale) => scale,
            }
            .clamp(0.125, 8.0);
        }
        if let Some(family) = &self.family {
            style.font_family = family.clone();
        }
        if let Some(weight) = self.weight {
            style.font_weight = weight.resolve(base.font_weight);
        }
        if let Some(slant) = self.slant {
            style.font_style = slant;
        }
        if let Some(decoration) = self.decoration {
            style.decoration = base.decoration.including(decoration);
        }
        if let Some(ink) = self.text_ink {
            style.text_ink = ink.or(base.text_ink);
        }
        if let Some(height) = self.line_height {
            style.line_height = height;
            style.line_height_relative = None;
        }
        if let Some(value) = self.length_line_height {
            style.line_height = None;
            style.line_height_relative = Some(ComputedLineHeight {
                value,
                font_scale: style.size_scale,
                allow_small: style.allow_small_note_text,
            });
        }
        if let Some(length) = self.letter_spacing {
            style.letter_spacing =
                computed_spacing(length, style.size_scale, self.root_scale).for_dpi(self.dpi);
        }
        if let Some(value) = self.common_ligatures {
            style.common_ligatures = value;
        }
        if let Some(value) = self.ligature_extras {
            style.ligature_extras = value;
        }
        if let Some(value) = self.variant_caps {
            style.variant_caps = value;
        }
        if let Some(value) = self.variant_numeric {
            style.variant_numeric = value;
        }
        if let Some(value) = self.variant_east_asian {
            style.variant_east_asian = value;
        }
        if let Some(value) = self.variant_position {
            style.variant_position = value;
        }
        if let Some(value) = self.variant_historical {
            style.variant_historical = value;
        }
        if let Some(value) = self.optical_sizing {
            style.optical_sizing = value;
        }
        if let Some(value) = self.text_transform {
            style.text_transform = value;
        }
        style.flow = flow::resolve(base.flow, style.display, style.visibility);
        style
    }
}

impl FirstLetter {
    pub(crate) fn apply(&self, base: &Style) -> Style {
        self.typography.apply(base)
    }

    pub(super) fn set_dpi(&mut self, dpi: u16) {
        self.typography.dpi = dpi;
    }

    pub(crate) fn gap(&self, width: u32, font: f32) -> f32 {
        self.trailing_margin.length().box_pixels(width, font)
    }
}

/// The typography of a host or descendant during the first line only.
/// Descendants that inherit the same values share the host's record.
#[derive(Clone, PartialEq)]
pub(crate) struct FirstLine {
    pub(crate) host: usize,
    family: Family,
    size: f32,
    weight: FontWeight,
    slant: FontStyle,
    line_height: Option<f32>,
    line_height_relative: Option<ComputedLineHeight>,
    letter_spacing: CssLength,
    common_ligatures: bool,
    ligature_extras: LigatureExtras,
    variant_caps: VariantCaps,
    variant_numeric: VariantNumeric,
    variant_east_asian: VariantEastAsian,
    variant_position: VariantPosition,
    variant_historical: bool,
    optical_sizing: bool,
    text_transform: TextTransform,
    root_scale: f32,
    decoration: TextDecoration,
    text_ink: Option<u8>,
}

impl FirstLine {
    fn new(host: usize, style: &Style, root_scale: f32) -> Self {
        Self {
            host,
            family: style.font_family.clone(),
            size: style.size_scale,
            weight: style.font_weight,
            slant: style.font_style,
            line_height: style.line_height,
            line_height_relative: style.line_height_relative,
            letter_spacing: style.letter_spacing,
            common_ligatures: style.common_ligatures,
            ligature_extras: style.ligature_extras,
            variant_caps: style.variant_caps,
            variant_numeric: style.variant_numeric,
            variant_east_asian: style.variant_east_asian,
            variant_position: style.variant_position,
            variant_historical: style.variant_historical,
            optical_sizing: style.optical_sizing,
            text_transform: style.text_transform,
            root_scale,
            decoration: style.decoration,
            text_ink: style.text_ink,
        }
    }

    pub(crate) fn apply(&self, source: &Style) -> Style {
        let mut style = source.clone();
        style.font_family = self.family.clone();
        style.size_scale = self.size;
        style.font_weight = self.weight;
        style.font_style = self.slant;
        style.line_height = self.line_height;
        style.line_height_relative = self.line_height_relative;
        style.letter_spacing = self.letter_spacing;
        style.common_ligatures = self.common_ligatures;
        style.ligature_extras = self.ligature_extras;
        style.variant_caps = self.variant_caps;
        style.variant_numeric = self.variant_numeric;
        style.variant_east_asian = self.variant_east_asian;
        style.variant_position = self.variant_position;
        style.variant_historical = self.variant_historical;
        style.optical_sizing = self.optical_sizing;
        style.text_transform = self.text_transform;
        style.decoration = style.decoration.including(self.decoration);
        style.text_ink = self.text_ink;
        style
    }

    pub(super) fn scale_dpi(&mut self, dpi: u16) {
        self.letter_spacing = self.letter_spacing.for_dpi(dpi);
    }

    fn descendant(&self, style: &Style, tag: &str, winners: &super::Winners<'_>) -> Self {
        use super::user_css::Reset;
        let declaration = |slot: usize| winners[slot].map(|(_, property)| property);
        let inherits = |slot| {
            matches!(
                declaration(slot),
                None | Some(Property::Reset(_, Reset::Inherit | Reset::Unset))
            )
        };
        let size = match declaration(1) {
            Some(Property::Size(Size::Parent(scale))) => self.size * scale,
            None if matches!(tag, "sup" | "sub") => self.size * 0.7,
            _ if inherits(1) => self.size,
            _ => style.size_scale,
        };
        let weight = match declaration(5) {
            Some(Property::Weight(weight)) => weight.resolve(self.weight),
            None if matches!(tag, "b" | "strong") => Weight::Bolder.resolve(self.weight),
            _ if inherits(5) => self.weight,
            _ => style.font_weight,
        };
        let size = size.clamp(0.125, 8.0);
        macro_rules! inherited_variant {
            ($slot:expr, $field:ident) => {
                if inherits($slot) {
                    self.$field
                } else {
                    style.$field
                }
            };
        }
        Self {
            host: self.host,
            family: if inherits(0) {
                &self.family
            } else {
                &style.font_family
            }
            .clone(),
            size,
            weight,
            slant: match declaration(6) {
                None if matches!(tag, "em" | "i") => FontStyle::Italic,
                _ if inherits(6) => self.slant,
                _ => style.font_style,
            },
            line_height: if inherits(2) {
                self.line_height
            } else {
                style.line_height
            },
            line_height_relative: match declaration(2) {
                Some(Property::LengthLineHeight(value)) => Some(ComputedLineHeight {
                    value: *value,
                    font_scale: size,
                    allow_small: style.allow_small_note_text,
                }),
                _ if inherits(2) => self.line_height_relative,
                _ => style.line_height_relative,
            },
            letter_spacing: match declaration(super::LETTER_SPACING) {
                Some(Property::LetterSpacing(length)) => {
                    computed_spacing(*length, size, self.root_scale)
                }
                _ if inherits(super::LETTER_SPACING) => self.letter_spacing,
                _ => style.letter_spacing,
            },
            common_ligatures: inherited_variant!(super::COMMON_LIGATURES, common_ligatures),
            ligature_extras: inherited_variant!(super::LIGATURE_EXTRAS, ligature_extras),
            variant_caps: inherited_variant!(super::VARIANT_CAPS, variant_caps),
            variant_numeric: inherited_variant!(super::VARIANT_NUMERIC, variant_numeric),
            variant_east_asian: inherited_variant!(super::VARIANT_EAST_ASIAN, variant_east_asian),
            variant_position: inherited_variant!(super::VARIANT_POSITION, variant_position),
            variant_historical: inherited_variant!(super::VARIANT_HISTORICAL, variant_historical),
            optical_sizing: inherited_variant!(super::OPTICAL_SIZING, optical_sizing),
            text_transform: inherited_variant!(super::TEXT_TRANSFORM, text_transform),
            text_ink: match declaration(super::TEXT_COLOR) {
                Some(Property::TextColor(None)) => self.text_ink,
                _ if inherits(super::TEXT_COLOR) => self.text_ink,
                _ => style.text_ink,
            },
            root_scale: self.root_scale,
            decoration: self.decoration.including(style.decoration),
        }
    }
}

#[derive(Clone)]
pub(crate) struct LetterRun<'a> {
    pub(crate) bytes: Range<usize>,
    pub(crate) owner: usize,
    pub(crate) rule: &'a FirstLetter,
}

impl<'a> LetterRun<'a> {
    pub(crate) fn resolve(
        block: &crate::source::Block,
        host: usize,
        view: &crate::flow_projection::Projection<'_>,
        styles: &'a [Style],
    ) -> Option<Self> {
        let rule = styles[host].first_letter.as_deref()?;
        let (owner, first) = block
            .flow_origins
            .first_text(|element| styles[element].flow.suppressed)?;
        let len = first_letter_len(block.text.get(first.clone())?)?;
        let mut spans = view
            .layout_spans(first.start..first.start + len)?
            .into_iter();
        let bytes = spans.next()?;
        if spans.next().is_some() {
            return None;
        }
        Some(Self { bytes, owner, rule })
    }
}

fn first_letter_len(text: &str) -> Option<usize> {
    use icu_properties::{CodePointMapData, props::GeneralCategory as Gc};
    let categories = CodePointMapData::<Gc>::new();
    let mut last = None;
    let mut found = false;
    for (offset, ch) in text.char_indices() {
        let kind = categories.get(ch);
        if found
            && !matches!(
                kind,
                Gc::NonspacingMark
                    | Gc::EnclosingMark
                    | Gc::SpacingMark
                    | Gc::ClosePunctuation
                    | Gc::FinalPunctuation
                    | Gc::DashPunctuation
                    | Gc::ConnectorPunctuation
                    | Gc::OtherPunctuation
            )
        {
            break;
        }
        if matches!(
            kind,
            Gc::UppercaseLetter
                | Gc::LowercaseLetter
                | Gc::TitlecaseLetter
                | Gc::ModifierLetter
                | Gc::OtherLetter
                | Gc::DecimalNumber
                | Gc::MathSymbol
                | Gc::CurrencySymbol
                | Gc::ModifierSymbol
                | Gc::OtherSymbol
        ) {
            found = true;
        }
        if !ch.is_whitespace() {
            last = Some(offset + ch.len_utf8());
        }
    }
    last
}

fn property_slot(property: &Property) -> Option<usize> {
    Some(match property {
        Property::GeneratedContent(_) => 0,
        Property::Display(_) => 1,
        Property::Visibility(_) => 2,
        Property::Size(_) => 3,
        Property::Family(_) => 4,
        Property::Weight(_) => 5,
        Property::Slant(_) => 6,
        Property::Decoration(_) => 7,
        Property::LineHeight(_) | Property::LengthLineHeight(_) => 8,
        Property::Float(_) => 9,
        Property::InitialLetter(_) => 10,
        Property::HorizontalMargin(1, _) => 11,
        Property::LetterSpacing(_) | Property::Reset(super::LETTER_SPACING, _) => 12,
        Property::CommonLigatures(_) | Property::Reset(super::COMMON_LIGATURES, _) => 13,
        Property::LigatureExtras(_) | Property::Reset(super::LIGATURE_EXTRAS, _) => 14,
        Property::VariantCaps(_) | Property::Reset(super::VARIANT_CAPS, _) => 15,
        Property::VariantNumeric(_) | Property::Reset(super::VARIANT_NUMERIC, _) => 16,
        Property::VariantEastAsian(_) | Property::Reset(super::VARIANT_EAST_ASIAN, _) => 17,
        Property::VariantPosition(_) | Property::Reset(super::VARIANT_POSITION, _) => 18,
        Property::VariantHistorical(_) | Property::Reset(super::VARIANT_HISTORICAL, _) => 19,
        Property::OpticalSizing(_) | Property::Reset(super::OPTICAL_SIZING, _) => 20,
        Property::TextTransform(_) | Property::Reset(super::TEXT_TRANSFORM, _) => 21,
        Property::TextColor(_) | Property::Reset(super::TEXT_COLOR, _) => 22,
        _ => return None,
    })
}

pub(super) fn accepts_user_declaration(target: Target, property: &Property) -> bool {
    match (target, property) {
        (_, Property::Display(value)) => matches!(value, Display::Inline | Display::None),
        (_, Property::VariantShorthand(..) | Property::LigatureShorthand(..)) => true,
        (_, Property::ResetMany(keys, _)) => {
            !keys.is_empty()
                && keys.iter().all(|key| {
                    matches!(
                        *key,
                        super::COMMON_LIGATURES
                            | super::LIGATURE_EXTRAS
                            | super::VARIANT_CAPS
                            | super::VARIANT_NUMERIC
                            | super::VARIANT_EAST_ASIAN
                            | super::VARIANT_POSITION
                            | super::VARIANT_HISTORICAL
                    )
                })
        }
        (Target::Generated(_), _) => {
            property_slot(property).is_some_and(|slot| slot <= 8 || (12..=22).contains(&slot))
        }
        (Target::FirstLine, _) => is_first_line_typography(property),
        (
            Target::FirstLetter,
            Property::GeneratedContent(_) | Property::Float(Some(crate::float_layout::Side::Right)),
        ) => false,
        (Target::FirstLetter, _) => property_slot(property).is_some(),
    }
}

pub(crate) struct Run<'a> {
    pub(crate) bytes: Range<usize>,
    pub(crate) style: &'a Style,
    pub(crate) host: Option<usize>,
}

fn is_first_line_typography(property: &Property) -> bool {
    matches!(
        property,
        Property::Size(_)
            | Property::Family(_)
            | Property::Weight(_)
            | Property::Slant(_)
            | Property::Decoration(_)
            | Property::LineHeight(_)
            | Property::LengthLineHeight(_)
            | Property::LetterSpacing(_)
            | Property::CommonLigatures(_)
            | Property::LigatureExtras(_)
            | Property::VariantCaps(_)
            | Property::VariantNumeric(_)
            | Property::VariantEastAsian(_)
            | Property::VariantPosition(_)
            | Property::VariantHistorical(_)
            | Property::OpticalSizing(_)
            | Property::TextTransform(_)
            | Property::TextColor(_)
            | Property::Reset(
                super::COMMON_LIGATURES
                    | super::LETTER_SPACING
                    | super::LIGATURE_EXTRAS
                    | super::VARIANT_CAPS
                    | super::VARIANT_NUMERIC
                    | super::VARIANT_EAST_ASIAN
                    | super::VARIANT_POSITION
                    | super::VARIANT_HISTORICAL
                    | super::OPTICAL_SIZING
                    | super::TEXT_TRANSFORM
                    | super::TEXT_COLOR,
                _,
            )
    )
}

const SLOTS: usize = 23;
type Winners<'a> = [Option<(Priority, &'a Property)>; SLOTS];

pub(super) struct Rules<'a> {
    book_style: &'a BookStyle,
    rules: Vec<(CascadeOrigin, &'a Rule)>,
    remaining: usize,
    hosts: usize,
    letter_hosts: usize,
    first_line_styles: usize,
    quotes: quotes::Plan<'a>,
}

impl<'a> Rules<'a> {
    pub(super) fn new(book_style: &'a BookStyle) -> Self {
        let mut rules = Vec::new();
        if !book_style.layout_tweaks.disable_generated_content
            || !book_style.layout_tweaks.disable_first_letter
            || !book_style.layout_tweaks.disable_first_line
        {
            for (origin, sheet) in [
                (CascadeOrigin::UserAgent, &book_style.base_rules),
                (CascadeOrigin::Author, &book_style.rules),
                (CascadeOrigin::User, &book_style.user_rules),
            ] {
                rules.extend(sheet.iter().filter_map(|rule| {
                    rule.selectors
                        .iter()
                        .any(|s| match s.pseudo {
                            Some(Target::Generated(_)) => {
                                !book_style.layout_tweaks.disable_generated_content
                            }
                            Some(Target::FirstLetter) => {
                                !book_style.layout_tweaks.disable_first_letter
                            }
                            Some(Target::FirstLine) => !book_style.layout_tweaks.disable_first_line,
                            None => false,
                        })
                        .then_some((origin, rule))
                }));
            }
        }
        Self {
            book_style,
            rules,
            remaining: 64 * 1024,
            hosts: MAX_GENERATED_HOSTS,
            letter_hosts: MAX_GENERATED_HOSTS,
            first_line_styles: MAX_FIRST_LINE_STYLES,
            quotes: quotes::Plan::default(),
        }
    }

    pub(super) fn for_element(
        &mut self,
        elements: &[Element],
        index: usize,
        host: &Style,
        root_scale: f32,
        parent: &Style,
        inline_winners: &super::Winners<'_>,
    ) -> (
        Option<Arc<Pair>>,
        Option<Arc<FirstLetter>>,
        Option<Arc<FirstLine>>,
    ) {
        let generated_enabled = self.remaining > 0
            && self.hosts > 0
            && !self.book_style.layout_tweaks.disable_generated_content;
        let letter_enabled =
            self.letter_hosts > 0 && !self.book_style.layout_tweaks.disable_first_letter;
        let line_enabled =
            self.first_line_styles > 0 && !self.book_style.layout_tweaks.disable_first_line;
        if (!generated_enabled && !letter_enabled && !line_enabled)
            || self.rules.is_empty()
            || host.flow.suppressed
        {
            return (None, None, None);
        }
        let block = host.display == Display::Block || host.display.is_list_item();
        let mut winners: [Winners<'a>; 2] = [[None; SLOTS]; 2];
        let mut letter_winners = [None; SLOTS];
        let mut line_winners = [None; SLOTS];
        for (origin, rule) in &self.rules {
            for selector in &rule.selectors {
                let Some(target) = selector
                    .pseudo
                    .filter(|_| selector.matches_host(elements, index))
                else {
                    continue;
                };
                let winners = match target {
                    Target::Generated(side) if generated_enabled => &mut winners[side as usize],
                    Target::FirstLetter if letter_enabled => &mut letter_winners,
                    Target::FirstLine if line_enabled && block => &mut line_winners,
                    _ => continue,
                };
                for decl in &rule.declarations {
                    if matches!(target, Target::FirstLine)
                        && !matches!(decl.property, Property::Display(_))
                        && !is_first_line_typography(&decl.property)
                    {
                        continue;
                    }
                    let Some(slot) = property_slot(&decl.property) else {
                        continue;
                    };
                    let priority = super::cascade_priority(
                        *origin,
                        decl.important,
                        false,
                        selector.specificity,
                    );
                    let target = &mut winners[slot];
                    if target.is_none_or(|(old, _)| priority >= old) {
                        *target = Some((priority, &decl.property));
                    }
                }
            }
        }
        let pending = self.quotes.0.len();
        let [before, after] = winners;
        let pair = [
            self.generated(
                index,
                Side::Before,
                &elements[index],
                host,
                root_scale,
                before,
            ),
            self.generated(
                index,
                Side::After,
                &elements[index],
                host,
                root_scale,
                after,
            ),
        ];
        if self.quotes.0.len() != pending || pair.iter().any(Option::is_some) {
            self.hosts -= 1;
        }
        let generated = pair.iter().any(Option::is_some).then(|| Arc::new(pair));
        let letter = (!matches!(
            letter_winners[1],
            Some((_, Property::Display(Display::None)))
        ) && letter_winners[2..].iter().any(Option::is_some))
        .then(|| {
            self.letter_hosts -= 1;
            let placement = match letter_winners[10] {
                Some((_, Property::InitialLetter(Some(caps)))) => Some(Placement::Numeric(*caps)),
                _ if matches!(
                    letter_winners[9],
                    Some((_, Property::Float(Some(crate::float_layout::Side::Left))))
                ) =>
                {
                    Some(Placement::Float)
                }
                _ => None,
            };
            let trailing_margin = match letter_winners[11] {
                Some((_, Property::HorizontalMargin(1, Some(margin)))) => *margin,
                _ => BoxMargin::default(),
            };
            Arc::new(FirstLetter {
                typography: self.typography(letter_winners, root_scale),
                placement,
                trailing_margin,
            })
        });
        let first_line = if !line_enabled {
            None
        } else if block {
            (!matches!(line_winners[1], Some((_, Property::Display(Display::None))))
                && (line_winners[3..=8].iter().any(Option::is_some)
                    || line_winners[12..].iter().any(Option::is_some)))
            .then(|| {
                self.first_line_styles -= 1;
                let style = self.typography(line_winners, root_scale).apply(host);
                Arc::new(FirstLine::new(index, &style, root_scale))
            })
        } else {
            parent.first_line.as_ref().map(|current| {
                let alternate = current.descendant(host, &elements[index].tag, inline_winners);
                if *current.as_ref() == alternate {
                    Arc::clone(current)
                } else {
                    self.first_line_styles -= 1;
                    Arc::new(alternate)
                }
            })
        };
        (generated, letter, first_line)
    }

    pub(super) fn into_quotes(self) -> quotes::Plan<'a> {
        self.quotes
    }

    fn generated(
        &mut self,
        index: usize,
        side: Side,
        element: &Element,
        host: &Style,
        root_scale: f32,
        slots: Winners<'a>,
    ) -> Option<Generated> {
        let Some((_, Property::GeneratedContent(content))) = slots[0] else {
            return None;
        };
        let display = match slots[1] {
            Some((_, Property::Display(display))) => *display,
            _ => Display::Inline,
        };
        if display == Display::None {
            return None;
        }
        if matches!(content.first(), Some(ContentPart::Quote(_))) {
            // Even a generated block can change quote depth before its siblings.
            // Only inline output has a paint representation in this provider.
            let bytes = content
                .iter()
                .map(|part| match part {
                    ContentPart::Quote(token) => token.output_bytes(),
                    part => literal(part, element).len(),
                })
                .sum::<usize>();
            if bytes <= MAX_CONTENT_BYTES {
                self.remaining = self.remaining.checked_sub(bytes + 1)?;
                let style = (display == Display::Inline)
                    .then(|| self.typography(slots, root_scale).apply(host));
                self.quotes.0.push(quotes::Pending {
                    host: index,
                    side,
                    parts: content,
                    style,
                });
            }
            return None;
        }
        if display != Display::Inline {
            return None;
        }
        let text = expand(content, element, |_, _| {})?;
        self.remaining = self.remaining.checked_sub(text.len() + 1)?;
        Some(Generated {
            text,
            style: self.typography(slots, root_scale).apply(host),
        })
    }

    fn typography(&self, slots: Winners<'_>, root_scale: f32) -> Typography {
        let mut style = Typography {
            root_scale,
            dpi: 96,
            ..Typography::default()
        };
        for (_, property) in slots.into_iter().flatten() {
            match property {
                Property::Visibility(value) => style.visibility = Some(*value),
                Property::Size(value) => style.size = Some(*value),
                Property::Family(families) => {
                    style.family = Some(
                        families
                            .iter()
                            .find_map(|family| match family {
                                Family::Named(name) => self
                                    .book_style
                                    .families
                                    .get(&name.to_ascii_lowercase())
                                    .cloned()
                                    .map(Family::Named),
                                Family::Generic(family) => Some(Family::Generic(*family)),
                            })
                            .unwrap_or(Family::Generic(GenericFamily::Serif)),
                    );
                }
                Property::Weight(value) => style.weight = Some(*value),
                Property::Slant(value) => style.slant = Some(*value),
                Property::Decoration(value) => style.decoration = *value,
                Property::LineHeight(value) => style.line_height = Some(*value),
                Property::LengthLineHeight(value) => style.length_line_height = Some(*value),
                Property::LetterSpacing(value) => style.letter_spacing = Some(*value),
                Property::CommonLigatures(value) => style.common_ligatures = Some(*value),
                Property::LigatureExtras(value) => style.ligature_extras = Some(*value),
                Property::VariantCaps(value) => style.variant_caps = Some(*value),
                Property::VariantNumeric(value) => style.variant_numeric = Some(*value),
                Property::VariantEastAsian(value) => style.variant_east_asian = Some(*value),
                Property::VariantPosition(value) => style.variant_position = Some(*value),
                Property::VariantHistorical(value) => style.variant_historical = Some(*value),
                Property::OpticalSizing(value) => style.optical_sizing = Some(*value),
                Property::TextTransform(value) => style.text_transform = Some(*value),
                Property::TextColor(value) => style.text_ink = Some(*value),
                Property::Reset(key, super::user_css::Reset::Initial) => match *key {
                    super::COMMON_LIGATURES => style.common_ligatures = Some(true),
                    super::LIGATURE_EXTRAS => style.ligature_extras = Some(Default::default()),
                    super::VARIANT_CAPS => style.variant_caps = Some(VariantCaps::Normal),
                    super::VARIANT_NUMERIC => style.variant_numeric = Some(Default::default()),
                    super::VARIANT_EAST_ASIAN => {
                        style.variant_east_asian = Some(Default::default())
                    }
                    super::VARIANT_POSITION => {
                        style.variant_position = Some(VariantPosition::Normal)
                    }
                    super::VARIANT_HISTORICAL => style.variant_historical = Some(false),
                    super::OPTICAL_SIZING => style.optical_sizing = Some(true),
                    super::TEXT_TRANSFORM => style.text_transform = Some(TextTransform::None),
                    super::LETTER_SPACING => style.letter_spacing = Some(CssLength::default()),
                    super::TEXT_COLOR => style.text_ink = Some(Some(0)),
                    _ => {}
                },
                Property::Reset(super::TEXT_COLOR, _) => style.text_ink = Some(None),
                _ => (),
            }
        }
        style
    }
}

fn expand(
    parts: &[ContentPart],
    element: &Element,
    mut quote: impl FnMut(quotes::Token, &mut String),
) -> Option<String> {
    let mut text = String::new();
    let mut leading_tokens = true;
    for part in parts {
        if let ContentPart::Quote(token) = part {
            // In the pinned engine's short declarations, quote expansion
            // stops interpreting quote tokens at the first string or attr.
            if leading_tokens {
                quote(*token, &mut text);
            }
        } else {
            leading_tokens = false;
            text.push_str(literal(part, element));
        }
        if text.len() > MAX_CONTENT_BYTES {
            return None;
        }
    }
    (!text.is_empty()).then_some(text)
}

fn literal<'a>(part: &'a ContentPart, element: &'a Element) -> &'a str {
    match part {
        ContentPart::Text(text) => text,
        ContentPart::Attribute(name) if name.eq_ignore_ascii_case("id") => {
            element.id.as_deref().unwrap_or_default()
        }
        ContentPart::Attribute(name) if name.eq_ignore_ascii_case("class") => &element.classes,
        ContentPart::Attribute(name) => element
            .pseudo_attributes
            .iter()
            .find_map(|(key, value)| key.eq_ignore_ascii_case(name).then_some(value.as_str()))
            .unwrap_or_default(),
        ContentPart::Quote(_) => "",
    }
}
