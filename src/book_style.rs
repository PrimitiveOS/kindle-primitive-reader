//! Publisher typography for the compact EPUB layout. The cascade handles
//! compound and descendant selectors, author order, inline styles, importance,
//! and inherited text properties. See https://www.w3.org/TR/css-cascade-5/.

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
use crate::source::{Element, Stylesheet};

const MAX_SHEETS: usize = 8;
const MAX_SHEET_BYTES: usize = 96 * 1024;
const MAX_CSS_BYTES: usize = 256 * 1024;
const MAX_INLINE_BYTES: usize = 4 * 1024;
const MAX_RULES: usize = 256;
const MAX_SELECTOR_BYTES: usize = 128;
const MAX_SELECTORS_PER_RULE: usize = 24;
const MAX_SELECTOR_STEPS: usize = 8;
const MAX_FONT_READS: usize = 6;
const MAX_FONT_BYTES: usize = 2 * 1024 * 1024;
const MAX_TOTAL_FONT_BYTES: usize = 4 * 1024 * 1024;

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

#[derive(Clone, Copy)]
pub(crate) enum ImageLength {
    Auto,
    Pixels(f32),
    Fraction(f32),
    Em(f32),
}

impl ImageLength {
    pub(crate) fn pixels(self, available: u32, font_size: f32) -> Option<f32> {
        match self {
            Self::Auto => None,
            Self::Pixels(px) => Some(px),
            Self::Fraction(ratio) => Some(ratio * available as f32),
            Self::Em(scale) => Some(scale * font_size),
        }
    }
}

#[derive(Clone)]
pub(crate) struct Style {
    pub font_family: Family,
    pub size_scale: f32,
    pub font_weight: FontWeight,
    pub font_style: FontStyle,
    pub line_height: Option<f32>,
    pub background: Option<u8>,
    pub image_width: Option<ImageLength>,
    pub image_height: Option<ImageLength>,
    pub image_align: parley::VerticalAlign,
    pub alignment: Option<Alignment>,
    pub line_break: Option<LineBreak>,
    pub word_break: WordBreak,
    pub soft_hyphens: bool,
    pub writing_mode: WritingMode,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            font_family: Family::Generic(GenericFamily::Serif),
            size_scale: 1.0,
            font_weight: FontWeight::NORMAL,
            font_style: FontStyle::Normal,
            line_height: None,
            background: None,
            image_width: None,
            image_height: None,
            image_align: parley::VerticalAlign::BASELINE,
            alignment: None,
            line_break: None,
            word_break: WordBreak::Normal,
            soft_hyphens: true,
            writing_mode: WritingMode::Horizontal,
        }
    }
}

#[derive(Default)]
pub(crate) struct BookStyle {
    author_inline: bool,
    rules: Vec<Rule>,
    families: HashMap<String, Arc<str>>,
    font_paths: HashSet<String>,
    font_reads: usize,
    font_bytes: usize,
}

impl BookStyle {
    pub fn load(
        book: &mut Book,
        chapter: &str,
        sheets: &[Stylesheet],
        fonts: &mut FontContext,
    ) -> Self {
        let mut styled = Self {
            author_inline: true,
            ..Self::default()
        };
        styled.families.extend(
            fonts
                .collection
                .family_names()
                .map(|name| (name.to_ascii_lowercase(), Arc::from(name))),
        );
        let mut remaining = MAX_CSS_BYTES;
        let mut linked = 0;
        let mut inline = 0;
        for sheet in sheets {
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
                    let Ok(bytes) = book.resource(&path) else {
                        continue;
                    };
                    if let Ok(text) = std::str::from_utf8(&bytes) {
                        styled.read_sheet(book, &path, text, &mut remaining, fonts);
                    }
                }
                Stylesheet::Inline(sheet) if inline < MAX_SHEETS => {
                    inline += 1;
                    styled.read_sheet(book, chapter, sheet, &mut remaining, fonts);
                }
                _ => {}
            }
        }
        styled
    }

    pub fn compute(&self, elements: &[Element]) -> Vec<Style> {
        let defaults = Style::default();
        let mut result: Vec<Style> = Vec::with_capacity(elements.len());
        for (index, element) in elements.iter().enumerate() {
            let parent = element.parent.map_or(&defaults, |parent| &result[parent]);
            let mut style = parent.clone();
            // A parent's background paints its box. It is not a text property.
            style.background = None;
            style.image_width = None;
            style.image_height = None;
            style.image_align = parley::VerticalAlign::BASELINE;
            match element.tag.as_str() {
                "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                    style.size_scale *= 1.4;
                    style.font_weight = FontWeight::BOLD;
                }
                "b" | "strong" | "th" => {
                    style.font_weight = Weight::Bolder.resolve(parent.font_weight)
                }
                "em" | "i" => style.font_style = FontStyle::Italic,
                _ => {}
            }
            let inline = element
                .inline_style
                .as_deref()
                .filter(|text| self.author_inline && text.len() <= MAX_INLINE_BYTES)
                .map_or_else(Vec::new, |text| declarations(&mut Parser::new(text)));
            let mut winners = Winners::default();
            for rule in &self.rules {
                if let Some(specificity) = rule
                    .selectors
                    .iter()
                    .filter(|selector| selector.matches(elements, index))
                    .map(|selector| selector.specificity)
                    .max()
                {
                    choose(&mut winners, &rule.declarations, false, specificity);
                }
            }
            choose(&mut winners, &inline, true, (0, 0, 0));
            for (_, property) in winners.into_iter().flatten() {
                match property {
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
                    Property::LineHeight(height) => style.line_height = *height,
                    Property::Background(gray) => style.background = *gray,
                    Property::Alignment(alignment) => style.alignment = Some(*alignment),
                    Property::Weight(weight) => {
                        style.font_weight = weight.resolve(parent.font_weight)
                    }
                    Property::Slant(slant) => style.font_style = *slant,
                    Property::LineBreak(breaking) => style.line_break = *breaking,
                    Property::WordBreak(breaking) => style.word_break = *breaking,
                    Property::SoftHyphens(enabled) => style.soft_hyphens = *enabled,
                    Property::WritingMode(mode) => style.writing_mode = *mode,
                    Property::Width(value) => style.image_width = Some(*value),
                    Property::Height(value) => style.image_height = Some(*value),
                    Property::VerticalAlign(value) => style.image_align = *value,
                    Property::FontSource(_) => {}
                }
            }
            style.size_scale = style.size_scale.clamp(0.125, 8.0);
            result.push(style);
        }
        result
    }

    fn read_sheet(
        &mut self,
        book: &mut Book,
        base: &str,
        text: &str,
        remaining: &mut usize,
        fonts: &mut FontContext,
    ) {
        if text.len() > MAX_SHEET_BYTES || text.len() > *remaining {
            return;
        }
        *remaining -= text.len();
        let mut parser = Parser::new(text.trim_start_matches('\u{feff}'));
        for rule in StyleSheetParser::new(&mut parser, &mut SheetParser)
            .take(MAX_RULES)
            .flatten()
        {
            match rule {
                SheetItem::Rule(rule) if self.rules.len() < MAX_RULES => self.rules.push(rule),
                SheetItem::Font(declarations) => {
                    self.register_font(book, base, declarations, fonts)
                }
                _ => {}
            }
        }
    }

    fn register_font(
        &mut self,
        book: &mut Book,
        base: &str,
        declarations: Vec<Declaration>,
        fonts: &mut FontContext,
    ) {
        let Some([Family::Named(alias)]) =
            declarations.iter().rev().find_map(|declaration| {
                match (&declaration.property, declaration.important) {
                    (Property::Family(families), false) => Some(families.as_slice()),
                    _ => None,
                }
            })
        else {
            return;
        };
        let sources = declarations
            .iter()
            .rev()
            .find_map(
                |declaration| match (&declaration.property, declaration.important) {
                    (Property::FontSource(sources), false) => Some(sources.as_slice()),
                    _ => None,
                },
            )
            .unwrap_or_default();
        for url in sources {
            if self.font_reads >= MAX_FONT_READS || self.font_bytes >= MAX_TOTAL_FONT_BYTES {
                return;
            }
            let Ok(path) = Book::resolve(base, url) else {
                continue;
            };
            if !path
                .rsplit_once('.')
                .is_some_and(|(_, ext)| matches!(ext.to_ascii_lowercase().as_str(), "ttf" | "otf"))
            {
                continue;
            }
            if !self.font_paths.insert(path.clone()) {
                continue;
            }
            self.font_reads += 1;
            let Ok(bytes) = book.resource(&path) else {
                continue;
            };
            let length = bytes.len();
            if length > MAX_FONT_BYTES
                || self.font_bytes + length > MAX_TOTAL_FONT_BYTES
                || !(bytes.starts_with(b"\0\x01\0\0") || bytes.starts_with(b"OTTO"))
            {
                continue;
            }
            let family = format!("PrimitiveOS EPUB Face {}", self.font_reads);
            let data = Blob::new(Arc::new(bytes));
            let overrides = FontInfoOverride {
                family_name: Some(&family),
                ..Default::default()
            };
            if !fonts
                .collection
                .register_fonts(data, Some(overrides))
                .is_empty()
            {
                self.font_bytes += length;
                self.families
                    .insert(alias.to_ascii_lowercase(), Arc::from(family));
                return;
            }
        }
    }
}

type Specificity = (u8, u8, u8);

struct Selector {
    steps: Vec<Compound>,
    specificity: Specificity,
}

#[derive(Default)]
struct Compound {
    tag: Option<String>,
    universal: bool,
    classes: Vec<String>,
    ids: Vec<String>,
}

impl Compound {
    fn is_empty(&self) -> bool {
        self.tag.is_none() && !self.universal && self.classes.is_empty() && self.ids.is_empty()
    }

    fn matches(&self, element: &Element) -> bool {
        self.tag
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

impl Selector {
    fn parse<'i>(input: &mut Parser<'i>) -> CssResult<Self> {
        let start = input.position().byte_index();
        let mut steps = Vec::new();
        let mut compound = Compound::default();
        let mut separated = false;
        let mut specificity: Specificity = (0, 0, 0);
        while let Ok(token) = input.next_including_whitespace().cloned() {
            if matches!(token, Token::WhiteSpace(_)) {
                separated = !compound.is_empty();
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
        Ok(Self { steps, specificity })
    }

    fn matches(&self, elements: &[Element], index: usize) -> bool {
        let (subject, ancestors) = self.steps.split_last().unwrap();
        if !subject.matches(&elements[index]) {
            return false;
        }
        let mut parent = elements[index].parent;
        for step in ancestors.iter().rev() {
            let Some(matched) = std::iter::successors(parent, |&index| elements[index].parent)
                .find(|&index| step.matches(&elements[index]))
            else {
                return false;
            };
            parent = elements[matched].parent;
        }
        true
    }
}

struct Rule {
    selectors: Vec<Selector>,
    declarations: Vec<Declaration>,
}

enum SheetItem {
    Rule(Rule),
    Font(Vec<Declaration>),
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
    type Prelude = ();
    type AtRule = SheetItem;
    type Error = ();

    fn parse_prelude(&mut self, name: CowRcStr<'i>, input: &mut Parser<'i>) -> CssResult<()> {
        if !name.eq_ignore_ascii_case("font-face") {
            return Err(ParseError::unexpected_token());
        }
        Ok(input.expect_exhausted()?)
    }

    fn parse_block(
        &mut self,
        _: (),
        _: &ParserState,
        input: &mut Parser<'i>,
    ) -> CssResult<SheetItem> {
        Ok(SheetItem::Font(declarations(input)))
    }
}

struct Declaration {
    property: Property,
    important: bool,
}

fn declarations(input: &mut Parser<'_>) -> Vec<Declaration> {
    RuleBodyParser::new(input, &mut Properties)
        .flatten()
        .collect()
}

type Priority = (bool, bool, Specificity);
type Winners<'a> = [Option<(Priority, &'a Property)>; 14];

fn choose<'a>(
    winners: &mut Winners<'a>,
    declarations: &'a [Declaration],
    inline: bool,
    specificity: Specificity,
) {
    for declaration in declarations {
        let index = match declaration.property {
            Property::Family(_) => 0,
            Property::Size(_) => 1,
            Property::LineHeight(_) => 2,
            Property::Background(_) => 3,
            Property::Alignment(_) => 4,
            Property::Weight(_) => 5,
            Property::Slant(_) => 6,
            Property::LineBreak(_) => 7,
            Property::WordBreak(_) => 8,
            Property::SoftHyphens(_) => 9,
            Property::WritingMode(_) => 10,
            Property::Width(_) => 11,
            Property::Height(_) => 12,
            Property::VerticalAlign(_) => 13,
            Property::FontSource(_) => continue,
        };
        let priority = (declaration.important, inline, specificity);
        if winners[index].is_none_or(|(current, _)| priority >= current) {
            winners[index] = Some((priority, &declaration.property));
        }
    }
}

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
    Family(Vec<Family>),
    FontSource(Vec<String>),
    Size(Size),
    LineHeight(Option<f32>),
    Background(Option<u8>),
    Alignment(Alignment),
    Weight(Weight),
    Slant(FontStyle),
    LineBreak(Option<LineBreak>),
    WordBreak(WordBreak),
    SoftHyphens(bool),
    WritingMode(WritingMode),
    Width(ImageLength),
    Height(ImageLength),
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
            let property = match name.to_ascii_lowercase().as_ref() {
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
                "width" | "height" => {
                    let length = match input.next()?.clone() {
                        Token::Ident(s) if s.eq_ignore_ascii_case("auto") => ImageLength::Auto,
                        Token::Percentage { unit_value, .. }
                            if unit_value.is_finite() && unit_value > 0.0 =>
                        {
                            ImageLength::Fraction(unit_value)
                        }
                        Token::Dimension { unit, value, .. }
                            if value.is_finite()
                                && value > 0.0
                                && unit.eq_ignore_ascii_case("px") =>
                        {
                            ImageLength::Pixels(value)
                        }
                        Token::Dimension { unit, value, .. }
                            if value.is_finite()
                                && value > 0.0
                                && unit.eq_ignore_ascii_case("em") =>
                        {
                            ImageLength::Em(value)
                        }
                        _ => return Err(ParseError::unexpected_token()),
                    };
                    if name.eq_ignore_ascii_case("width") {
                        Property::Width(length)
                    } else {
                        Property::Height(length)
                    }
                }
                "font-family" => Property::Family(input.parse_comma_separated(font_family)?),
                "src" => {
                    Property::FontSource(input.parse_comma_separated_ignoring_errors(font_url))
                }
                "font-size" => {
                    let size = match input.next()?.clone() {
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
                "line-height" => {
                    if input
                        .try_parse(|input| input.expect_ident_matching("normal"))
                        .is_ok()
                    {
                        Property::LineHeight(None)
                    } else {
                        let height = input.expect_number()?;
                        if !(1.0..=3.0).contains(&height) {
                            return Err(ParseError::unexpected_token());
                        }
                        Property::LineHeight(Some(height))
                    }
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
                "line-break" => {
                    Property::LineBreak(match input.expect_ident()?.to_ascii_lowercase().as_str() {
                        "auto" => None,
                        "loose" => Some(LineBreak::Loose),
                        "normal" => Some(LineBreak::Normal),
                        "strict" => Some(LineBreak::Strict),
                        "anywhere" => Some(LineBreak::Anywhere),
                        _ => return Err(ParseError::unexpected_token()),
                    })
                }
                "word-break" => {
                    Property::WordBreak(match input.expect_ident()?.to_ascii_lowercase().as_str() {
                        "normal" => WordBreak::Normal,
                        "break-all" => WordBreak::BreakAll,
                        "keep-all" => WordBreak::KeepAll,
                        _ => return Err(ParseError::unexpected_token()),
                    })
                }
                "hyphens" | "-webkit-hyphens" => {
                    Property::SoftHyphens(
                        match input.expect_ident()?.to_ascii_lowercase().as_str() {
                            "none" => false,
                            // Authored discretionary points also apply when no automatic dictionary is available.
                            "manual" | "auto" => true,
                            _ => return Err(ParseError::unexpected_token()),
                        },
                    )
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
            if !matches!(format_name.as_str(), "truetype" | "opentype") {
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

    fn computed(xhtml: &str) -> (Source, Vec<Style>) {
        let source = Source::parse(xhtml).unwrap();
        let mut book = Book::open(&crate::test_fixture("field-notes-epub3.epub")).unwrap();
        let chapter = book.chapters[0].href.clone();
        let fonts = [crate::test_fixture("fonts/Silkscreen-Regular.ttf")];
        let mut fonts = crate::font_context(&fonts).unwrap();
        let styles = BookStyle::load(&mut book, &chapter, &source.stylesheets, &mut fonts)
            .compute(&source.elements);
        (source, styles)
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
