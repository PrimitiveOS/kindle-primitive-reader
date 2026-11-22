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
use crate::source::{Element, Source, Stylesheet};

const MAX_SHEETS: usize = 8;
const MAX_SHEET_BYTES: usize = 96 * 1024;
const MAX_CSS_BYTES: usize = 256 * 1024;
const MAX_INLINE_BYTES: usize = 4 * 1024;
const MAX_RULES: usize = 256;
const MAX_SELECTOR_BYTES: usize = 128;
const MAX_SELECTORS_PER_RULE: usize = 24;
const MAX_SELECTOR_STEPS: usize = 8;
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

#[derive(Clone, Copy)]
enum IndentLength {
    Em(f32),
    RootEm(f32),
    Pixels(f32),
    Fraction(f32),
}

#[derive(Clone, Copy)]
pub(crate) struct TextIndent {
    length: IndentLength,
    pub hanging: bool,
}

impl Default for TextIndent {
    fn default() -> Self {
        Self {
            length: IndentLength::Pixels(0.0),
            hanging: false,
        }
    }
}

impl TextIndent {
    // Resolve font-relative values at the declaring element so descendants that
    // change font size inherit the computed length. Percentages stay relative
    // to the width of the block being laid out.
    fn computed(self, font: f32, root_font: f32) -> Self {
        let length = match self.length {
            IndentLength::Em(em) => IndentLength::Em(em * font),
            IndentLength::RootEm(em) => IndentLength::Em(em * root_font),
            length => length,
        };
        Self { length, ..self }
    }

    pub fn pixels(self, width: u32, reader_font: f32) -> f32 {
        match self.length {
            IndentLength::Em(em) | IndentLength::RootEm(em) => em * reader_font,
            IndentLength::Pixels(px) => px,
            IndentLength::Fraction(ratio) => ratio * width as f32,
        }
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

#[derive(Clone)]
pub(crate) struct Style {
    pub font_family: Family,
    pub size_scale: f32,
    pub text_rise: TextRise,
    pub font_weight: FontWeight,
    pub font_style: FontStyle,
    pub line_height: Option<f32>,
    pub text_indent: TextIndent,
    pub background: Option<u8>,
    pub marker: crate::list_marker::MarkerKind,
    pub image_width: Option<ImageLength>,
    pub image_height: Option<ImageLength>,
    pub image_align: parley::VerticalAlign,
    pub alignment: Option<Alignment>,
    pub line_break: Option<LineBreak>,
    pub word_break: WordBreak,
    pub hyphens: Hyphens,
    pub writing_mode: WritingMode,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            font_family: Family::Generic(GenericFamily::Serif),
            size_scale: 1.0,
            text_rise: TextRise::default(),
            font_weight: FontWeight::NORMAL,
            font_style: FontStyle::Normal,
            line_height: None,
            text_indent: TextIndent::default(),
            background: None,
            marker: crate::list_marker::MarkerKind::None,
            image_width: None,
            image_height: None,
            image_align: parley::VerticalAlign::BASELINE,
            alignment: None,
            line_break: None,
            word_break: WordBreak::Normal,
            hyphens: Hyphens::Auto,
            writing_mode: WritingMode::Horizontal,
        }
    }
}

#[derive(Default)]
pub(crate) struct BookStyle {
    author_inline: bool,
    rules: Vec<Rule>,
    families: HashMap<String, Arc<str>>,
}

impl BookStyle {
    pub fn load(book: &mut Book, chapter: &str, source: &Source, fonts: &mut FontContext) -> Self {
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
        if source.stylesheets.is_empty() {
            return styled;
        }
        let mut faces = Vec::new();
        let mut remaining = MAX_CSS_BYTES;
        let mut linked = 0;
        let mut inline = 0;
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
                    let Ok(bytes) = book.resource(&path) else {
                        continue;
                    };
                    if let Ok(text) = std::str::from_utf8(&bytes) {
                        styled.read_sheet(&path, text, &mut remaining, &mut faces, &names);
                    }
                }
                Stylesheet::Inline(sheet) if inline < MAX_SHEETS => {
                    inline += 1;
                    styled.read_sheet(chapter, sheet, &mut remaining, &mut faces, &names);
                }
                _ => {}
            }
        }
        styled.register_fonts(book, &faces, fonts);
        styled
    }

    pub fn compute(&self, elements: &[Element]) -> Vec<Style> {
        let defaults = Style::default();
        let mut result: Vec<Style> = Vec::with_capacity(elements.len());
        for (index, element) in elements.iter().enumerate() {
            if element.selector_only {
                result.push(Style::default());
                continue;
            }
            let parent = element.parent.map_or(&defaults, |parent| &result[parent]);
            let mut style = parent.clone();
            // A parent's background paints its box. It is not a text property.
            style.background = None;
            if let Some(kind) = element.list_kind {
                style.marker = kind;
            }
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
                    Property::Indent(indent) => {
                        if let Some(indent) = indent {
                            let font = style.size_scale.clamp(0.125, 8.0);
                            let root_font = result.first().map_or(font, |root| root.size_scale);
                            style.text_indent = indent.computed(font, root_font);
                        }
                    }
                    Property::LineHeight(height) => style.line_height = *height,
                    Property::Marker(kind) => style.marker = *kind,
                    Property::Background(gray) => style.background = *gray,
                    Property::Alignment(alignment) => style.alignment = Some(*alignment),
                    Property::Weight(weight) => {
                        style.font_weight = weight.resolve(parent.font_weight)
                    }
                    Property::Slant(slant) => style.font_style = *slant,
                    Property::LineBreak(breaking) => style.line_break = *breaking,
                    Property::WordBreak(breaking) => style.word_break = *breaking,
                    Property::Hyphens(mode) => style.hyphens = *mode,
                    Property::WritingMode(mode) => style.writing_mode = *mode,
                    Property::Width(value) => style.image_width = Some(*value),
                    Property::Height(value) => style.image_height = Some(*value),
                    Property::VerticalAlign(value) => style.image_align = *value,
                    Property::FontSource(_) => {}
                }
            }
            // CREngine places superscripts a quarter of the parent font height above
            // baseline and subscripts four fifths of its descent below it.
            match style.image_align.shift {
                parley::BaselineShift::Super => style.text_rise.height += 0.25 * parent.size_scale,
                parley::BaselineShift::Sub => style.text_rise.descent -= 0.8 * parent.size_scale,
                _ => {}
            }
            style.size_scale = style.size_scale.clamp(0.125, 8.0);
            result.push(style);
        }
        result
    }

    fn read_sheet(
        &mut self,
        base: &str,
        text: &str,
        remaining: &mut usize,
        faces: &mut Vec<FontFace>,
        names: &ChapterNames<'_>,
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
                SheetItem::Rule(mut rule)
                    if self.rules.len() < MAX_RULES && !rule.declarations.is_empty() =>
                {
                    rule.selectors.retain(|selector| names.possible(selector));
                    if !rule.selectors.is_empty() {
                        self.rules.push(rule);
                    }
                }
                SheetItem::Font(declarations) if faces.len() < MAX_FACE_RULES => {
                    if let Some(face) = FontFace::from_rule(base, declarations) {
                        faces.push(face);
                    }
                }
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
                ext.eq_ignore_ascii_case("ttf") || ext.eq_ignore_ascii_case("otf")
            })
            .then_some(Self::Local(path))
    }
}

struct FontFace {
    family: String,
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
                        family = Some(name.to_ascii_lowercase());
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
        Some(Self {
            family: family?,
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
}

#[derive(Clone, Copy, Default)]
enum Relation {
    #[default]
    Descendant,
    Child,
    NextSibling,
}

#[derive(Default)]
struct Compound {
    tag: Option<String>,
    universal: bool,
    classes: Vec<String>,
    ids: Vec<String>,
    first_child: bool,
    first_of_type: bool,
    relation: Relation,
}

impl Compound {
    fn is_empty(&self) -> bool {
        self.tag.is_none()
            && !self.universal
            && self.classes.is_empty()
            && self.ids.is_empty()
            && !self.first_child
            && !self.first_of_type
    }

    fn matches(&self, element: &Element) -> bool {
        !element.anonymous
            && (!self.first_child || element.previous_sibling.is_none())
            && (!self.first_of_type || element.first_of_type)
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
            if let Token::Delim(combinator @ ('>' | '+')) = token {
                if compound.is_empty() {
                    return Err(ParseError::unexpected_token());
                }
                steps.push(std::mem::take(&mut compound));
                compound.relation = if combinator == '>' {
                    Relation::Child
                } else {
                    Relation::NextSibling
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
                Token::Colon => {
                    let Token::Ident(name) = input.next_including_whitespace()? else {
                        return Err(ParseError::unexpected_token());
                    };
                    if name.eq_ignore_ascii_case("first-of-type") {
                        compound.first_of_type = true;
                    } else if name.eq_ignore_ascii_case("first-child") {
                        compound.first_child = true;
                    } else {
                        return Err(ParseError::unexpected_token());
                    }
                    specificity.1 = specificity.1.saturating_add(1);
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
        Ok(Self { steps, specificity })
    }

    fn matches(&self, elements: &[Element], index: usize) -> bool {
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
type Winners<'a> = [Option<(Priority, &'a Property)>; 16];

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
            Property::Hyphens(_) => 9,
            Property::WritingMode(_) => 10,
            Property::Width(_) => 11,
            Property::Height(_) => 12,
            Property::VerticalAlign(_) => 13,
            Property::Marker(_) => 14,
            Property::Indent(_) => 15,
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
    Indent(Option<TextIndent>),
    Marker(crate::list_marker::MarkerKind),
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
    Hyphens(Hyphens),
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
                "text-indent" => Property::Indent(parse_indent(input)?),
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
                        Token::Ident(name) if name.eq_ignore_ascii_case("smaller") => Size::Parent(1.0 / 1.2),
                        Token::Ident(name) if name.eq_ignore_ascii_case("larger") => Size::Parent(1.2),
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
                "hyphens" | "-webkit-hyphens" | "-epub-hyphens" | "adobe-hyphenate" => {
                    Property::Hyphens(match input.expect_ident()?.to_ascii_lowercase().as_str() {
                        "none" => Hyphens::None,
                        "manual" => Hyphens::Manual,
                        "auto" => Hyphens::Auto,
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

fn parse_indent(input: &mut Parser<'_>) -> CssResult<Option<TextIndent>> {
    if let Ok(keyword) = input.try_parse(Parser::expect_ident_cloned) {
        match keyword.to_ascii_lowercase().as_str() {
            "initial" => return Ok(Some(TextIndent::default())),
            "inherit" | "unset" | "revert" => return Ok(None),
            "hanging" => {
                return Ok(Some(TextIndent {
                    length: indent_length(input)?,
                    hanging: true,
                }));
            }
            _ => return Err(ParseError::unexpected_token()),
        }
    }
    let length = indent_length(input)?;
    let hanging = input
        .try_parse(|input| input.expect_ident_matching("hanging"))
        .is_ok();
    Ok(Some(TextIndent { length, hanging }))
}

fn indent_length(input: &mut Parser<'_>) -> CssResult<IndentLength> {
    match input.next()?.clone() {
        Token::Number { value: 0.0, .. } => Ok(IndentLength::Pixels(0.0)),
        Token::Percentage { unit_value, .. } if unit_value.is_finite() => {
            Ok(IndentLength::Fraction(unit_value))
        }
        Token::Dimension { value, unit, .. } if value.is_finite() => {
            Ok(match unit.to_ascii_lowercase().as_str() {
                "em" => IndentLength::Em(value),
                "rem" => IndentLength::RootEm(value),
                "px" => IndentLength::Pixels(value),
                "pt" => IndentLength::Pixels(value * (96.0 / 72.0)),
                "pc" => IndentLength::Pixels(value * 16.0),
                "in" => IndentLength::Pixels(value * 96.0),
                "cm" => IndentLength::Pixels(value * (96.0 / 2.54)),
                "mm" => IndentLength::Pixels(value * (96.0 / 25.4)),
                "q" => IndentLength::Pixels(value * (96.0 / 101.6)),
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
        let styles =
            BookStyle::load(&mut book, &chapter, &source, &mut fonts).compute(&source.elements);
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
