//! CSS-generated inline decoration. The host's style and source bytes stay intact.
use super::{
    BookStyle, CascadeOrigin, CssResult, Display, Element, Family, Priority, Property, Rule, Size,
    Style, flow,
};
use cssparser::{Parser, Token};
use parley::GenericFamily;
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

impl Side {
    pub(crate) fn parse(name: &str) -> Option<Self> {
        if name.eq_ignore_ascii_case("before") {
            Some(Self::Before)
        } else if name.eq_ignore_ascii_case("after") {
            Some(Self::After)
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

pub(super) fn property_slot(property: &Property) -> Option<usize> {
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
        _ => return None,
    })
}

pub(crate) struct Run<'a> {
    pub(crate) bytes: Range<usize>,
    pub(crate) style: &'a Style,
}

pub(super) struct Rules<'a> {
    book_style: &'a BookStyle,
    rules: Vec<(CascadeOrigin, &'a Rule)>,
    remaining: usize,
    hosts: usize,
}

impl<'a> Rules<'a> {
    pub(super) fn new(book_style: &'a BookStyle) -> Self {
        let mut rules = Vec::new();
        if !book_style.layout_tweaks.disable_generated_content {
            for (origin, sheet) in [
                (CascadeOrigin::UserAgent, &book_style.base_rules),
                (CascadeOrigin::Author, &book_style.rules),
                (CascadeOrigin::User, &book_style.user_rules),
            ] {
                rules.extend(sheet.iter().filter_map(|rule| {
                    rule.selectors
                        .iter()
                        .any(|s| s.pseudo.is_some())
                        .then_some((origin, rule))
                }));
            }
        }
        Self {
            book_style,
            rules,
            remaining: 64 * 1024,
            hosts: MAX_GENERATED_HOSTS,
        }
    }

    pub(super) fn for_element(
        &mut self,
        elements: &[Element],
        index: usize,
        host: &Style,
        root_scale: f32,
    ) -> Option<Arc<Pair>> {
        if self.remaining == 0 || self.hosts == 0 || self.rules.is_empty() || host.flow.suppressed {
            return None;
        }
        const SLOTS: usize = 9;
        let mut winners: [[Option<(Priority, &Property)>; SLOTS]; 2] = [[None; SLOTS]; 2];
        for (origin, rule) in &self.rules {
            for selector in &rule.selectors {
                let Some(side) = selector
                    .pseudo
                    .filter(|_| selector.matches_host(elements, index))
                else {
                    continue;
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
                    let target = &mut winners[side as usize][slot];
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
            let mut style = host.clone();
            style.display = Display::Inline;
            style.generated = None;
            for (_, property) in slots.into_iter().flatten() {
                match property {
                    Property::Display(display) => style.display = *display,
                    Property::Visibility(visibility) => style.visibility = *visibility,
                    Property::Size(size) => {
                        style.size_scale = match size {
                            Size::Parent(scale) => host.size_scale * scale,
                            Size::Root(scale) => root_scale * scale,
                            Size::Reference(scale) => *scale,
                        }
                        .clamp(0.125, 8.0);
                    }
                    Property::Family(families) => {
                        style.font_family = families
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
                            .unwrap_or(Family::Generic(GenericFamily::Serif));
                    }
                    Property::Weight(weight) => {
                        style.font_weight = weight.resolve(host.font_weight)
                    }
                    Property::Slant(slant) => style.font_style = *slant,
                    Property::Decoration(decoration) => {
                        style.decoration =
                            host.decoration.including(decoration.unwrap_or_default());
                    }
                    Property::LineHeight(height) => style.line_height = *height,
                    _ => (),
                }
            }
            // The generated content is an inline decoration. Block/float
            // pseudo boxes need their own flow policy.
            if style.display != Display::Inline {
                return None;
            }
            style.flow = flow::resolve(host.flow, style.display, style.visibility);
            let text = expand(content, &elements[index])?;
            self.remaining = self.remaining.checked_sub(text.len() + 1)?;
            Some(Generated { text, style })
        });
        pair.iter().any(Option::is_some).then(|| {
            self.hosts -= 1;
            Arc::new(pair)
        })
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
