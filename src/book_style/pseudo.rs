//! CSS-generated inline decoration. The host's style and source bytes stay intact.
use super::{
    BookStyle, BoxMargin, CascadeOrigin, CssResult, Display, Element, Family, Priority, Property,
    Rule, Size, Style, TextDecoration, Weight, flow,
};
use cssparser::{Parser, Token};
use parley::{FontStyle, GenericFamily};
use std::{ops::Range, sync::Arc};

const MAX_CONTENT_PARTS: usize = 16;
const MAX_CONTENT_BYTES: usize = 1024;
const MAX_GENERATED_HOSTS: usize = 1024;
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
}

impl Target {
    pub(crate) fn parse(name: &str) -> Option<Self> {
        if name.eq_ignore_ascii_case("before") {
            Some(Self::Generated(Side::Before))
        } else if name.eq_ignore_ascii_case("after") {
            Some(Self::Generated(Side::After))
        } else if name.eq_ignore_ascii_case("first-letter") {
            Some(Self::FirstLetter)
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

pub(crate) struct Generated {
    pub(crate) text: String,
    pub(crate) style: Style,
}

pub(crate) type Pair = [Option<Generated>; 2];

/// Overrides inherit from the styled text node, which may be inside emphasis
/// or a link, independently of the selector's actual block host.
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

#[derive(Default)]
struct Typography {
    visibility: Option<flow::Visibility>,
    size: Option<Size>,
    root_scale: f32,
    family: Option<Family>,
    weight: Option<Weight>,
    slant: Option<FontStyle>,
    decoration: Option<TextDecoration>,
    line_height: Option<Option<f32>>,
}

impl Typography {
    fn apply(&self, base: &Style) -> Style {
        let mut style = base.clone();
        style.generated = None;
        style.first_letter = None;
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
        if let Some(height) = self.line_height {
            style.line_height = height;
        }
        style.flow = flow::resolve(base.flow, style.display, style.visibility);
        style
    }
}

impl FirstLetter {
    pub(crate) fn apply(&self, base: &Style) -> Style {
        self.typography.apply(base)
    }

    pub(crate) fn gap(&self, width: u32, font: f32) -> f32 {
        self.trailing_margin.length().box_pixels(width, font)
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
        Property::LineHeight(_) => 8,
        Property::Float(_) => 9,
        Property::InitialLetter(_) => 10,
        Property::HorizontalMargin(1, _) => 11,
        _ => return None,
    })
}

pub(super) fn accepts_user_declaration(target: Target, property: &Property) -> bool {
    match (target, property) {
        (_, Property::Display(value)) => matches!(value, Display::Inline | Display::None),
        (Target::Generated(_), _) => property_slot(property).is_some_and(|slot| slot <= 8),
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
}

const SLOTS: usize = 12;
type Winners<'a> = [Option<(Priority, &'a Property)>; SLOTS];

pub(super) struct Rules<'a> {
    book_style: &'a BookStyle,
    rules: Vec<(CascadeOrigin, &'a Rule)>,
    remaining: usize,
    hosts: usize,
    letter_hosts: usize,
}

impl<'a> Rules<'a> {
    pub(super) fn new(book_style: &'a BookStyle) -> Self {
        let mut rules = Vec::new();
        if !book_style.layout_tweaks.disable_generated_content
            || !book_style.layout_tweaks.disable_first_letter
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
        }
    }

    pub(super) fn for_element(
        &mut self,
        elements: &[Element],
        index: usize,
        host: &Style,
        root_scale: f32,
    ) -> (Option<Arc<Pair>>, Option<Arc<FirstLetter>>) {
        let generated_enabled = self.remaining > 0
            && self.hosts > 0
            && !self.book_style.layout_tweaks.disable_generated_content;
        let letter_enabled =
            self.letter_hosts > 0 && !self.book_style.layout_tweaks.disable_first_letter;
        if (!generated_enabled && !letter_enabled) || self.rules.is_empty() || host.flow.suppressed
        {
            return (None, None);
        }
        let mut winners: [Winners<'_>; 2] = [[None; SLOTS]; 2];
        let mut letter_winners = [None; SLOTS];
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
                    _ => continue,
                };
                for decl in &rule.declarations {
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
        let pair = winners.map(|slots| {
            let Some((_, Property::GeneratedContent(content))) = slots[0] else {
                return None;
            };
            // The generated content is an inline decoration. Block/float
            // pseudo boxes need their own flow policy.
            if matches!(slots[1], Some((_, Property::Display(display))) if *display != Display::Inline) {
                return None;
            }
            let style = self.typography(slots, root_scale).apply(host);
            let text = expand(content, &elements[index])?;
            self.remaining = self.remaining.checked_sub(text.len() + 1)?;
            Some(Generated { text, style })
        });
        let generated = pair.iter().any(Option::is_some).then(|| {
            self.hosts -= 1;
            Arc::new(pair)
        });
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
        (generated, letter)
    }

    fn typography(&self, slots: Winners<'_>, root_scale: f32) -> Typography {
        let mut style = Typography {
            root_scale,
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
                _ => (),
            }
        }
        style
    }
}

fn expand(parts: &[ContentPart], element: &Element) -> Option<String> {
    let mut text = String::new();
    for part in parts {
        let value = match part {
            ContentPart::Text(text) => text.as_str(),
            ContentPart::Attribute(name) if name.eq_ignore_ascii_case("id") => {
                element.id.as_deref().unwrap_or_default()
            }
            ContentPart::Attribute(name) if name.eq_ignore_ascii_case("class") => &element.classes,
            ContentPart::Attribute(name) => element
                .pseudo_attributes
                .iter()
                .find_map(|(key, value)| key.eq_ignore_ascii_case(name).then_some(value.as_str()))
                .unwrap_or_default(),
        };
        if text.len() + value.len() > MAX_CONTENT_BYTES {
            return None;
        }
        text.push_str(value);
    }
    (!text.is_empty()).then_some(text)
}
