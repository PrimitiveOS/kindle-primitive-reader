//! User-origin parser: unsupported input is an error, never a silently discarded tweak.
use super::*;
use cssparser::{ParseErrorKind, SourceLocation};
fn message(location: SourceLocation, text: impl std::fmt::Display) -> String {
    format!(
        "Line {}, column {}: {text}",
        location.line + 1,
        location.column
    )
}
fn error(input: &Parser<'_>, text: impl std::fmt::Display) -> ParseError<String> {
    ParseError::custom(message(input.current_source_location(), text))
}
#[derive(Clone, Copy)]
pub(super) enum Reset {
    Inherit,
    Initial,
    Unset,
}
impl Reset {
    fn parent(self, key: usize) -> bool {
        matches!(self, Self::Inherit)
            || matches!(self, Self::Unset)
                && matches!(
                    key,
                    0 | 1 | 2 | 4 | 5 | 6 | 7 | 8 | 9 | 10 | 14 | 15 | 20 | 21 | 24 | 37
                )
    }
}
pub(super) fn reset(input: &mut Parser<'_>, name: &str) -> Option<Property> {
    let value = input.try_parse(Parser::expect_ident_cloned).ok()?;
    let kind = match value.to_ascii_lowercase().as_str() {
        "inherit" => Reset::Inherit,
        "initial" => Reset::Initial,
        "unset" => Reset::Unset,
        _ => return None,
    };
    let keys = match name {
        "font-family" => vec![0],
        "font-size" => vec![1],
        "line-height" => vec![2],
        "background" | "background-color" => vec![3],
        "text-align" => vec![4],
        "font-weight" => vec![5],
        "font-style" => vec![6],
        "line-break" => vec![7],
        "word-break" => vec![8],
        "hyphens" | "-epub-hyphens" | "-webkit-hyphens" => vec![9],
        "writing-mode" | "-epub-writing-mode" | "-webkit-writing-mode" => vec![10],
        "list-style-type" | "list-style" => vec![14],
        "text-indent" => vec![15],
        "text-decoration" | "text-decoration-line" => vec![16],
        "break-before" | "page-break-before" => vec![17],
        "break-after" | "page-break-after" => vec![18],
        "break-inside" | "page-break-inside" => vec![19],
        "orphans" => vec![20],
        "widows" => vec![21],
        "padding-left" => vec![22],
        "padding-right" => vec![23],
        "padding-top" => vec![38],
        "padding-bottom" => vec![39],
        "padding" => vec![22, 23, 38, 39],
        "border-collapse" => vec![24],
        "border-spacing" => vec![37],
        _ => return None,
    };
    Some(Property::ResetMany(keys, kind))
}
pub(super) fn apply_reset(
    style: &mut Style,
    key: usize,
    reset: Reset,
    parent: &Style,
    initial: &Style,
) {
    let value = if reset.parent(key) { parent } else { initial };
    match key {
        0 => style.font_family = value.font_family.clone(),
        1 => style.size_scale = value.size_scale,
        2 => style.line_height = value.line_height,
        3 => style.background = value.background,
        4 => style.alignment = value.alignment,
        5 => style.font_weight = value.font_weight,
        6 => style.font_style = value.font_style,
        7 => style.line_break = value.line_break,
        8 => style.word_break = value.word_break,
        9 => style.hyphens = value.hyphens,
        10 => style.writing_mode = value.writing_mode,
        14 => style.marker = value.marker,
        15 => style.text_indent = value.text_indent,
        17 => style.page_breaks.before = value.page_breaks.before,
        18 => style.page_breaks.after = value.page_breaks.after,
        19 => style.page_breaks.keep_together = value.page_breaks.keep_together,
        20 => style.orphans = value.orphans,
        21 => style.widows = value.widows,
        22 => style.padding[0] = value.padding[0],
        23 => style.padding[1] = value.padding[1],
        24 => style.border_collapse = value.border_collapse,
        37 => style.border_spacing = value.border_spacing,
        38 => style.vertical_padding[0] = value.vertical_padding[0],
        39 => style.vertical_padding[1] = value.vertical_padding[1],
        _ => unreachable!("expanded supported reset key"),
    }
}
pub(super) fn decoration(reset: Reset, parent: TextDecoration) -> TextDecoration {
    if reset.parent(16) {
        parent
    } else {
        Default::default()
    }
}

struct StrictProperties;
impl<'i> DeclarationParser<'i> for StrictProperties {
    type Declaration = Declaration;
    type Error = String;
    fn parse_value(
        &mut self,
        name: CowRcStr<'i>,
        input: &mut Parser<'i>,
        state: &ParserState,
    ) -> Result<Declaration, ParseError<String>> {
        let before = input.state();
        let reset = input.try_parse(Parser::expect_ident_cloned).ok();
        input.reset(&before);
        if name.eq_ignore_ascii_case("src")
            || reset.is_some_and(|s| {
                s.eq_ignore_ascii_case("revert") || s.eq_ignore_ascii_case("revert-layer")
            })
        {
            return Err(error(input, format!("Unsupported user CSS: {name}")));
        }
        Properties
            .parse_value(name.clone(), input, state)
            .map_err(|_| error(input, format!("Unsupported property or value: {name}")))
    }
}
impl<'i> AtRuleParser<'i> for StrictProperties {
    type Prelude = ();
    type AtRule = Declaration;
    type Error = String;
}
impl<'i> QualifiedRuleParser<'i> for StrictProperties {
    type Prelude = ();
    type QualifiedRule = Declaration;
    type Error = String;
}
impl<'i> RuleBodyItemParser<'i, Declaration, String> for StrictProperties {
    fn parse_declarations(&self) -> bool {
        true
    }
    fn parse_qualified(&self) -> bool {
        false
    }
}
struct StrictSheet;
impl<'i> QualifiedRuleParser<'i> for StrictSheet {
    type Prelude = Vec<Selector>;
    type QualifiedRule = Rule;
    type Error = String;
    fn parse_prelude(
        &mut self,
        input: &mut Parser<'i>,
    ) -> Result<Vec<Selector>, ParseError<String>> {
        let selectors = input.parse_comma_separated(|p| {
            Selector::parse(p).map_err(|_| error(p, "Unsupported selector"))
        })?;
        if selectors.len() > MAX_SELECTORS_PER_RULE {
            return Err(error(input, "Too many selectors in one rule"));
        }
        Ok(selectors)
    }
    fn parse_block(
        &mut self,
        selectors: Vec<Selector>,
        _: &ParserState,
        input: &mut Parser<'i>,
    ) -> Result<Rule, ParseError<String>> {
        let mut declarations = Vec::new();
        let mut count = 0;
        let location = input.current_source_location();
        for result in RuleBodyParser::new(input, &mut StrictProperties) {
            let declaration = result.map_err(|(e, _, location)| match e.kind {
                ParseErrorKind::Custom(text) if text.starts_with("Line ") => {
                    ParseError::custom(text)
                }
                other => ParseError::custom(message(location, other)),
            })?;
            count += 1;
            if count > crate::user_styles::MAX_DECLARATIONS {
                return Err(ParseError::custom(message(
                    location,
                    "Too many declarations in one rule",
                )));
            }
            expand(declaration, &mut declarations);
        }
        if declarations.is_empty() {
            return Err(error(input, "A style rule needs a supported declaration"));
        }
        Ok(Rule {
            selectors,
            declarations,
        })
    }
}
impl<'i> AtRuleParser<'i> for StrictSheet {
    type Prelude = ();
    type AtRule = Rule;
    type Error = String;
}
fn expand(decl: Declaration, out: &mut Vec<Declaration>) {
    match decl.property {
        Property::ResetMany(keys, reset) => out.extend(keys.into_iter().map(|key| Declaration {
            property: Property::Reset(key, reset),
            important: decl.important,
        })),
        Property::PaddingBoth(sides) => out.extend(sides.into_iter().enumerate().map(
            |(side, length)| Declaration {
                property: Property::Padding(side, length),
                important: decl.important,
            },
        )),
        Property::BorderParts(parts) => {
            out.extend(parts.into_iter().map(|(side, part)| Declaration {
                property: Property::Border(side, part),
                important: decl.important,
            }))
        }
        _ => out.push(decl),
    }
}
pub(super) fn parse(css: &str) -> crate::Result<Vec<Rule>> {
    if css.len() > crate::user_styles::MAX_CSS {
        return Err("User CSS exceeds 64 KiB".into());
    }
    if css
        .chars()
        .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
    {
        return Err("User CSS contains a control character".into());
    }
    let mut parser = Parser::new(css);
    let mut out = Vec::new();
    for result in StyleSheetParser::new(&mut parser, &mut StrictSheet) {
        let rule = result.map_err(|(e, _, location)| match e.kind {
            ParseErrorKind::Custom(text) if text.starts_with("Line ") => text,
            ParseErrorKind::Custom(text) => message(location, text),
            _ => message(location, "Unsupported CSS syntax or at-rule"),
        })?;
        if out.len() >= crate::user_styles::MAX_RULES {
            return Err("User CSS exceeds 128 rules".into());
        }
        out.push(rule);
    }
    Ok(out)
}
pub(crate) fn validate(css: &str) -> crate::Result<()> {
    parse(css).map(|_| ())
}
pub(crate) fn matches(css: &str, elements: &[Element]) -> crate::Result<(usize, usize)> {
    let rules = parse(css)?;
    let matched: HashSet<_> = elements
        .iter()
        .enumerate()
        .filter(|(_, e)| !e.selector_only)
        .filter(|(index, _)| {
            rules.iter().any(|rule| {
                rule.selectors
                    .iter()
                    .any(|selector| selector.matches(elements, *index))
            })
        })
        .map(|(index, _)| index)
        .collect();
    Ok((rules.len(), matched.len()))
}
