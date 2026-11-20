//! Bounded publisher styles for the compact EPUB layout. The subset supports
//! simple selectors and local outline fonts. It applies linked sheets in order,
//! then document style blocks, then per-element declarations.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use cssparser::{
    AtRuleParser, CowRcStr, DeclarationParser, ParseError, Parser, ParserState,
    QualifiedRuleParser, RuleBodyItemParser, RuleBodyParser, StyleSheetParser, Token,
};
use parley::{
    Alignment, FontContext,
    fontique::{Blob, FontInfoOverride},
};

use crate::epub::Book;

const MAX_SHEETS: usize = 8;
const MAX_SHEET_BYTES: usize = 96 * 1024;
const MAX_CSS_BYTES: usize = 256 * 1024;
const MAX_INLINE_BYTES: usize = 4 * 1024;
const MAX_RULES: usize = 256;
const MAX_SELECTOR_BYTES: usize = 128;
const MAX_FONT_READS: usize = 6;
const MAX_FONT_BYTES: usize = 2 * 1024 * 1024;
const MAX_TOTAL_FONT_BYTES: usize = 4 * 1024 * 1024;

#[derive(Default)]
pub(crate) struct Style {
    pub font_family: Option<String>,
    pub size_scale: Option<f32>,
    pub line_height: Option<f32>,
    pub background: Option<u8>,
    pub alignment: Option<Alignment>,
}

#[derive(Default)]
pub(crate) struct BookStyle {
    rules: Vec<Rule>,
    families: HashMap<String, String>,
    font_paths: HashSet<String>,
    font_reads: usize,
    font_bytes: usize,
}

impl BookStyle {
    pub fn load(
        book: &mut Book,
        chapter: &str,
        links: &[String],
        inline_sheets: &[String],
        fonts: &mut FontContext,
    ) -> Self {
        let mut styled = Self::default();
        styled.families.extend(
            fonts
                .collection
                .family_names()
                .map(|name| (name.to_ascii_lowercase(), name.to_owned())),
        );
        let mut remaining = MAX_CSS_BYTES;
        for link in links.iter().take(MAX_SHEETS) {
            let Ok(path) = Book::resolve(chapter, link) else {
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
        for sheet in inline_sheets.iter().take(MAX_SHEETS) {
            styled.read_sheet(book, chapter, sheet, &mut remaining, fonts);
        }
        styled
    }

    pub fn properties(
        &self,
        tag: &str,
        classes: &str,
        id: Option<&str>,
        inline: Option<&str>,
    ) -> Style {
        let mut style = Style::default();
        let mut priority = [0_u8; 5];
        let mut apply = |declarations: &Declarations, specificity| {
            if let Some(families) = &declarations.families
                && specificity >= priority[0]
            {
                priority[0] = specificity;
                style.font_family = families
                    .iter()
                    .map(|name| name.to_ascii_lowercase())
                    .take_while(|name| {
                        !matches!(
                            name.as_str(),
                            "serif" | "sans-serif" | "monospace" | "system-ui"
                        )
                    })
                    .find_map(|name| self.families.get(&name))
                    .cloned();
            }
            if let Some(scale) = declarations.size_scale
                && specificity >= priority[1]
            {
                priority[1] = specificity;
                style.size_scale = Some(scale);
            }
            if let Some(height) = declarations.line_height
                && specificity >= priority[2]
            {
                priority[2] = specificity;
                style.line_height = Some(height);
            }
            if let Some(background) = declarations.background
                && specificity >= priority[3]
            {
                priority[3] = specificity;
                style.background = background;
            }
            if let Some(alignment) = declarations.alignment
                && specificity >= priority[4]
            {
                priority[4] = specificity;
                style.alignment = Some(alignment);
            }
        };

        for rule in &self.rules {
            if let Some(specificity) = rule
                .selectors
                .iter()
                .filter_map(|selector| selector.specificity(tag, classes, id))
                .max()
            {
                apply(&rule.declarations, specificity);
            }
        }
        if let Some(text) = inline.filter(|value| value.len() <= MAX_INLINE_BYTES) {
            let mut parser = Parser::new(text);
            apply(&Declarations::parse(&mut parser), 4);
        }
        style
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
        declaration: Declarations,
        fonts: &mut FontContext,
    ) {
        let Some(alias) = declaration
            .families
            .as_ref()
            .filter(|families| families.len() == 1)
            .and_then(|families| families.first())
        else {
            return;
        };
        for url in declaration.sources {
            if self.font_reads >= MAX_FONT_READS || self.font_bytes >= MAX_TOTAL_FONT_BYTES {
                return;
            }
            let Ok(path) = Book::resolve(base, &url) else {
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
                self.families.insert(alias.to_ascii_lowercase(), family);
                return;
            }
        }
    }
}

enum Selector {
    Tag(String),
    Class(String),
    Id(String),
}

impl Selector {
    fn parse<'i>(input: &mut Parser<'i>) -> CssResult<Self> {
        let (name, kind) = match input.next()?.clone() {
            Token::Ident(name) => (name, 1),
            Token::Delim('.') => (input.expect_ident_cloned()?, 2),
            Token::IDHash(name) => (name, 3),
            _ => return Err(ParseError::unexpected_token()),
        };
        if name.len() > MAX_SELECTOR_BYTES {
            return Err(ParseError::unexpected_token());
        }
        input.expect_exhausted()?;
        Ok(match kind {
            1 => Self::Tag(name.to_string()),
            2 => Self::Class(name.to_string()),
            _ => Self::Id(name.to_string()),
        })
    }

    fn specificity(&self, tag: &str, classes: &str, id: Option<&str>) -> Option<u8> {
        match self {
            Self::Tag(name) if tag.eq_ignore_ascii_case(name) => Some(1),
            Self::Class(name) if classes.split_ascii_whitespace().any(|class| class == name) => {
                Some(2)
            }
            Self::Id(name) if id == Some(name.as_str()) => Some(3),
            _ => None,
        }
    }
}

struct Rule {
    selectors: Vec<Selector>,
    declarations: Declarations,
}

enum SheetItem {
    Rule(Rule),
    Font(Declarations),
}

type CssResult<T> = Result<T, ParseError<()>>;

struct SheetParser;

impl<'i> QualifiedRuleParser<'i> for SheetParser {
    type Prelude = Vec<Selector>;
    type QualifiedRule = SheetItem;
    type Error = ();

    fn parse_prelude(&mut self, input: &mut Parser<'i>) -> CssResult<Self::Prelude> {
        input.parse_comma_separated(Selector::parse)
    }

    fn parse_block(
        &mut self,
        selectors: Self::Prelude,
        _: &ParserState,
        input: &mut Parser<'i>,
    ) -> CssResult<SheetItem> {
        Ok(SheetItem::Rule(Rule {
            selectors,
            declarations: Declarations::parse(input),
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
        Ok(SheetItem::Font(Declarations::parse(input)))
    }
}

#[derive(Default)]
struct Declarations {
    families: Option<Vec<String>>,
    sources: Vec<String>,
    size_scale: Option<f32>,
    line_height: Option<f32>,
    background: Option<Option<u8>>,
    alignment: Option<Alignment>,
}

impl Declarations {
    fn parse(input: &mut Parser<'_>) -> Self {
        let mut result = Self::default();
        for declaration in RuleBodyParser::new(input, &mut Properties).flatten() {
            match declaration {
                Property::Family(value) => result.families = Some(value),
                Property::FontSource(value) => result.sources = value,
                Property::Size(value) => result.size_scale = Some(value),
                Property::LineHeight(value) => result.line_height = Some(value),
                Property::Background(value) => result.background = Some(value),
                Property::Alignment(value) => result.alignment = Some(value),
            }
        }
        result
    }
}

enum Property {
    Family(Vec<String>),
    FontSource(Vec<String>),
    Size(f32),
    LineHeight(f32),
    Background(Option<u8>),
    Alignment(Alignment),
}

struct Properties;

impl<'i> DeclarationParser<'i> for Properties {
    type Declaration = Property;
    type Error = ();

    fn parse_value(
        &mut self,
        name: CowRcStr<'i>,
        input: &mut Parser<'i>,
        _: &ParserState,
    ) -> CssResult<Property> {
        let property = match name.to_ascii_lowercase().as_ref() {
            "font-family" => Property::Family(input.parse_comma_separated(font_family)?),
            "src" => Property::FontSource(input.parse_comma_separated_ignoring_errors(font_url)),
            "font-size" => {
                let scale = match input.next()?.clone() {
                    Token::Percentage { unit_value, .. } => unit_value,
                    Token::Dimension { value, unit, .. } if unit.eq_ignore_ascii_case("em") => {
                        value
                    }
                    _ => return Err(ParseError::unexpected_token()),
                };
                if !(0.45..=2.0).contains(&scale) {
                    return Err(ParseError::unexpected_token());
                }
                Property::Size(scale)
            }
            "line-height" => {
                let height = input.expect_number()?;
                if !(1.0..=3.0).contains(&height) {
                    return Err(ParseError::unexpected_token());
                }
                Property::LineHeight(height)
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
            _ => return Err(ParseError::unexpected_token()),
        };
        input.expect_exhausted()?;
        Ok(property)
    }
}

impl<'i> QualifiedRuleParser<'i> for Properties {
    type Prelude = ();
    type QualifiedRule = Property;
    type Error = ();
}

impl<'i> AtRuleParser<'i> for Properties {
    type Prelude = ();
    type AtRule = Property;
    type Error = ();
}

impl<'i> RuleBodyItemParser<'i, Property, ()> for Properties {
    fn parse_declarations(&self) -> bool {
        true
    }
    fn parse_qualified(&self) -> bool {
        false
    }
}

fn font_family<'i>(input: &mut Parser<'i>) -> CssResult<String> {
    if let Ok(quoted) = input.try_parse(Parser::expect_string_cloned) {
        return Ok(quoted.to_string());
    }
    let mut name = input.expect_ident_cloned()?.to_string();
    while let Ok(part) = input.try_parse(Parser::expect_ident_cloned) {
        name.push(' ');
        name.push_str(&part);
    }
    Ok(name)
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
