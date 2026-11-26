//! Physical border declarations expanded before the normal CSS cascade.
use super::{CssLength, CssResult, ParseError, Parser, parse_css_length};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Pattern {
    #[default]
    None,
    Hidden,
    Solid,
    Double,
    Dotted,
    Dashed,
}

#[derive(Clone, Copy)]
pub(crate) struct Border {
    pub width: CssLength,
    pub pattern: Pattern,
    pub gray: Option<(u8, u8)>,
    // Keep the symbolic value for explicit inheritance by a child whose
    // foreground may differ from this element's resolved foreground.
    pub current_color: bool,
}

impl Default for Border {
    fn default() -> Self {
        Self {
            width: CssLength::Pixels(3.),
            pattern: Pattern::None,
            gray: Some((0, 255)),
            current_color: true,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum Value {
    Width(Option<CssLength>),
    Pattern(Option<Pattern>),
    Gray(Option<Option<(u8, u8)>>),
    CurrentColor,
}
impl Value {
    pub fn component(self) -> usize {
        match self {
            Self::Width(_) => 0,
            Self::Pattern(_) => 1,
            Self::Gray(_) | Self::CurrentColor => 2,
        }
    }
    pub fn apply(self, to: &mut Border, from: Border, font: f32, root: f32) {
        match self {
            Self::Width(v) => to.width = v.map_or(from.width, |w| w.computed(font, root)),
            Self::Pattern(v) => to.pattern = v.unwrap_or(from.pattern),
            Self::Gray(Some(gray)) => {
                to.gray = gray;
                to.current_color = false;
            }
            Self::Gray(None) => {
                to.gray = from.gray;
                to.current_color = from.current_color;
            }
            Self::CurrentColor => to.current_color = true,
        }
    }
}

fn width(p: &mut Parser<'_>) -> CssResult<CssLength> {
    if let Ok(name) = p.try_parse(Parser::expect_ident_cloned) {
        return match name.to_ascii_lowercase().as_str() {
            "thin" => Ok(CssLength::Pixels(1.)),
            "medium" => Ok(CssLength::Pixels(3.)),
            "thick" => Ok(CssLength::Pixels(5.)),
            _ => Err(ParseError::unexpected_token()),
        };
    }
    let w = parse_css_length(p)?;
    if matches!(w, CssLength::Fraction(_)) || w.pixels(100, 40.) < 0. {
        return Err(ParseError::unexpected_token());
    }
    Ok(w)
}
fn pattern(p: &mut Parser<'_>) -> CssResult<Pattern> {
    match p.expect_ident()?.to_ascii_lowercase().as_str() {
        "none" => Ok(Pattern::None),
        "hidden" => Ok(Pattern::Hidden),
        "solid" => Ok(Pattern::Solid),
        "double" => Ok(Pattern::Double),
        "dotted" => Ok(Pattern::Dotted),
        "dashed" => Ok(Pattern::Dashed),
        _ => Err(ParseError::unexpected_token()),
    }
}
pub(super) fn color(p: &mut Parser<'_>) -> CssResult<Option<(u8, u8)>> {
    let (gray, alpha) = color_unquantized(p)?;
    let alpha = (255. * alpha).round() as u8;
    Ok((alpha > 0).then_some((gray, alpha)))
}

pub(super) fn opaque_or_transparent(p: &mut Parser<'_>) -> CssResult<Option<u8>> {
    let (gray, alpha) = color_unquantized(p)?;
    if alpha == 0.0 {
        Ok(None)
    } else if alpha == 1.0 {
        Ok(Some(gray))
    } else {
        Err(ParseError::unexpected_token())
    }
}

fn color_unquantized(p: &mut Parser<'_>) -> CssResult<(u8, f32)> {
    use cssparser::{
        Token,
        color::{parse_hash_color, parse_named_color},
    };
    let (r, g, b, a) = match p.next()?.clone() {
        Token::Ident(name) if name.eq_ignore_ascii_case("currentcolor") => (0, 0, 0, 1.),
        Token::Ident(name) if name.eq_ignore_ascii_case("transparent") => (0, 0, 0, 0.),
        Token::Ident(name) => {
            let (r, g, b) = parse_named_color(&name).map_err(|_| ParseError::unexpected_token())?;
            (r, g, b, 1.)
        }
        Token::Hash(s) | Token::IDHash(s) => {
            parse_hash_color(s.as_bytes()).map_err(|_| ParseError::unexpected_token())?
        }
        Token::Function(name)
            if name.eq_ignore_ascii_case("rgb") || name.eq_ignore_ascii_case("rgba") =>
        {
            p.parse_nested_block(|p| {
                fn channel(p: &mut Parser<'_>, scale: f32) -> CssResult<f32> {
                    match p.next()? {
                        Token::Number { value, .. } => Ok((*value / scale).clamp(0., 1.)),
                        Token::Percentage { unit_value, .. } => Ok((*unit_value).clamp(0., 1.)),
                        _ => Err(ParseError::unexpected_token()),
                    }
                }
                let r = channel(p, 255.)?;
                let commas = p.try_parse(Parser::expect_comma).is_ok();
                let g = channel(p, 255.)?;
                if commas {
                    p.expect_comma()?;
                }
                let b = channel(p, 255.)?;
                let a = if p.is_exhausted() {
                    1.
                } else {
                    if commas {
                        p.expect_comma()?;
                    } else {
                        p.expect_delim('/')?;
                    }
                    channel(p, 1.)?
                };
                Ok((
                    (255. * r).round() as u8,
                    (255. * g).round() as u8,
                    (255. * b).round() as u8,
                    a,
                ))
            })?
        }
        _ => return Err(ParseError::unexpected_token()),
    };
    Ok((
        ((77 * u32::from(r) + 150 * u32::from(g) + 29 * u32::from(b) + 128) >> 8) as u8,
        a,
    ))
}
fn initial(component: usize) -> Value {
    let b = Border::default();
    match component {
        0 => Value::Width(Some(b.width)),
        1 => Value::Pattern(Some(b.pattern)),
        _ => Value::CurrentColor,
    }
}
fn inherit(component: usize) -> Value {
    match component {
        0 => Value::Width(None),
        1 => Value::Pattern(None),
        _ => Value::Gray(None),
    }
}

fn color_value(p: &mut Parser<'_>) -> CssResult<Value> {
    if p.try_parse(|p| p.expect_ident_matching("currentcolor"))
        .is_ok()
    {
        Ok(Value::CurrentColor)
    } else {
        Ok(Value::Gray(Some(color(p)?)))
    }
}

pub(super) fn parse(name: &str, p: &mut Parser<'_>) -> CssResult<Vec<(usize, Value)>> {
    let mut side = None;
    let mut component = None;
    for token in name.split('-').skip(1) {
        match token {
            "top" => side = Some(0),
            "right" => side = Some(1),
            "bottom" => side = Some(2),
            "left" => side = Some(3),
            "width" => component = Some(0),
            "style" => component = Some(1),
            "color" => component = Some(2),
            _ => return Err(ParseError::unexpected_token()),
        }
    }
    let mut result = Vec::new();
    if let Ok(global) = p.try_parse(|p| {
        let word = p.expect_ident()?.to_ascii_lowercase();
        if ["initial", "inherit", "revert", "unset"].contains(&word.as_str()) {
            Ok(word)
        } else {
            Err(ParseError::<()>::unexpected_token())
        }
    }) {
        for s in 0..4 {
            for c in 0..3 {
                if side.is_none_or(|n| n == s) && component.is_none_or(|n| n == c) {
                    result.push((
                        s,
                        if global == "inherit" {
                            inherit(c)
                        } else {
                            initial(c)
                        },
                    ));
                }
            }
        }
    } else if let Some(c) = component {
        let mut values = Vec::new();
        while !p.is_exhausted() {
            if values.len() == if side.is_some() { 1 } else { 4 } {
                return Err(ParseError::unexpected_token());
            }
            values.push(match c {
                0 => Value::Width(Some(width(p)?)),
                1 => Value::Pattern(Some(pattern(p)?)),
                _ => color_value(p)?,
            });
        }
        let all = match values.as_slice() {
            [a] => [*a; 4],
            [a, b] => [*a, *b, *a, *b],
            [a, b, c] => [*a, *b, *c, *b],
            [a, b, c, d] => [*a, *b, *c, *d],
            _ => return Err(ParseError::unexpected_token()),
        };
        for (s, value) in all.into_iter().enumerate() {
            if side.is_none_or(|n| n == s) {
                result.push((s, value));
            }
        }
    } else {
        let mut parts = [None; 3];
        while !p.is_exhausted() {
            let value = if let Ok(w) = p.try_parse(width) {
                Value::Width(Some(w))
            } else if let Ok(line) = p.try_parse(pattern) {
                Value::Pattern(Some(line))
            } else {
                color_value(p)?
            };
            if parts[value.component()].replace(value).is_some() {
                return Err(ParseError::unexpected_token());
            }
        }
        if parts.iter().all(Option::is_none) {
            return Err(ParseError::unexpected_token());
        }
        for s in 0..4 {
            if side.is_none_or(|n| n == s) {
                for (c, part) in parts.iter().enumerate() {
                    result.push((s, part.unwrap_or_else(|| initial(c))));
                }
            }
        }
    }
    Ok(result)
}
