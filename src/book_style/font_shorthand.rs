//! A bounded publisher `font` shorthand, expanded atomically into longhands.
use super::{
    CssResult, Family, FontStyle, FontWeight, GenericFamily, LigatureExtras, ParseError, Parser,
    Property, Size, Token, VariantCaps, VariantEastAsian, VariantNumeric, VariantPosition, Weight,
};
use std::sync::Arc;

fn family(input: &mut Parser<'_>) -> CssResult<Family> {
    if let Ok(quoted) = input.try_parse(Parser::expect_string_cloned) {
        return Ok(Family::Named(Arc::from(quoted.as_ref())));
    }
    let mut words = Vec::new();
    while let Ok(word) = input.try_parse(Parser::expect_ident_cloned) {
        let lower = word.to_ascii_lowercase();
        if matches!(
            lower.as_str(),
            "inherit" | "initial" | "unset" | "revert" | "revert-layer" | "default"
        ) {
            return Err(ParseError::unexpected_token());
        }
        words.push(word.to_string());
        if words.len() > 16 {
            return Err(ParseError::unexpected_token());
        }
    }
    if words.is_empty() {
        return Err(ParseError::unexpected_token());
    }
    let generic = |name: &str| {
        matches!(
            name.to_ascii_lowercase().as_str(),
            "serif"
                | "sans-serif"
                | "cursive"
                | "fantasy"
                | "monospace"
                | "system-ui"
                | "ui-serif"
                | "ui-sans-serif"
                | "ui-monospace"
                | "ui-rounded"
                | "emoji"
                | "math"
                | "fangsong"
        )
    };
    if words.len() > 1 && words.iter().any(|word| generic(word)) {
        return Err(ParseError::unexpected_token());
    }
    let name = words.join(" ");
    Ok(GenericFamily::parse(&name).map_or_else(|| Family::Named(Arc::from(name)), Family::Generic))
}

fn size(input: &mut Parser<'_>) -> CssResult<Size> {
    let value = match input.next()?.clone() {
        Token::Ident(name) => match name.to_ascii_lowercase().as_str() {
            "xx-small" => Size::Reference(153.0 / 256.0),
            "x-small" => Size::Reference(192.0 / 256.0),
            "small" => Size::Reference(227.0 / 256.0),
            "medium" => Size::Reference(1.0),
            "large" => Size::Reference(307.0 / 256.0),
            "x-large" => Size::Reference(384.0 / 256.0),
            "xx-large" => Size::Reference(2.0),
            "xxx-large" => Size::Reference(3.0),
            "smaller" => Size::Parent(0.8),
            "larger" => Size::Parent(1.25),
            _ => return Err(ParseError::unexpected_token()),
        },
        Token::Percentage { unit_value, .. } => Size::Parent(unit_value),
        Token::Dimension { value, unit, .. } => match unit.to_ascii_lowercase().as_str() {
            "em" => Size::Parent(value),
            "ex" | "ch" => Size::Parent(value * 0.5),
            "rem" => Size::Reference(value),
            "px" => Size::Reference(value / 16.0),
            "pt" => Size::Reference(value / 12.0),
            _ => return Err(ParseError::unexpected_token()),
        },
        _ => return Err(ParseError::unexpected_token()),
    };
    let factor = match value {
        Size::Parent(factor) | Size::Root(factor) | Size::Reference(factor) => factor,
    };
    if !(0.125..=8.0).contains(&factor) {
        return Err(ParseError::unexpected_token());
    }
    Ok(value)
}

pub(super) fn parse(input: &mut Parser<'_>) -> CssResult<Vec<Property>> {
    let mut slant = None;
    let mut weight = None;
    let mut caps = None;
    let mut normals = 0usize;
    let size = loop {
        if let Ok(size) = input.try_parse(size) {
            break size;
        }
        match input.next()?.clone() {
            Token::Ident(word) if word.eq_ignore_ascii_case("normal") => normals += 1,
            Token::Ident(word) if word.eq_ignore_ascii_case("italic") && slant.is_none() => {
                slant = Some(FontStyle::Italic)
            }
            Token::Ident(word) if word.eq_ignore_ascii_case("oblique") && slant.is_none() => {
                slant = Some(FontStyle::Oblique(None))
            }
            Token::Ident(word) if word.eq_ignore_ascii_case("small-caps") && caps.is_none() => {
                caps = Some(VariantCaps::Small)
            }
            Token::Ident(word) if word.eq_ignore_ascii_case("bold") && weight.is_none() => {
                weight = Some(Weight::Fixed(FontWeight::BOLD))
            }
            Token::Ident(word) if word.eq_ignore_ascii_case("bolder") && weight.is_none() => {
                weight = Some(Weight::Bolder)
            }
            Token::Ident(word) if word.eq_ignore_ascii_case("lighter") && weight.is_none() => {
                weight = Some(Weight::Lighter)
            }
            Token::Number { value, .. } if (1.0..=1000.0).contains(&value) && weight.is_none() => {
                weight = Some(Weight::Fixed(FontWeight::new(value)))
            }
            _ => return Err(ParseError::unexpected_token()),
        }
        if normals
            + usize::from(slant.is_some())
            + usize::from(weight.is_some())
            + usize::from(caps.is_some())
            > 4
        {
            return Err(ParseError::unexpected_token());
        }
    };
    let line_height = if input.try_parse(|p| p.expect_delim('/')).is_ok() {
        if input
            .try_parse(|p| p.expect_ident_matching("normal"))
            .is_ok()
        {
            None
        } else {
            let number = input.expect_number()?;
            if !(1.0..=3.0).contains(&number) {
                return Err(ParseError::unexpected_token());
            }
            Some(number)
        }
    } else {
        None
    };
    let families: Vec<Family> = input.parse_comma_separated(family)?;
    if families.is_empty() || families.len() > 16 {
        return Err(ParseError::unexpected_token());
    }
    Ok(vec![
        Property::Slant(slant.unwrap_or(FontStyle::Normal)),
        Property::Weight(weight.unwrap_or(Weight::Fixed(FontWeight::NORMAL))),
        Property::Size(size),
        Property::LineHeight(line_height),
        Property::Family(families),
        Property::CommonLigatures(true),
        Property::LigatureExtras(LigatureExtras::default()),
        Property::VariantCaps(caps.unwrap_or(VariantCaps::Normal)),
        Property::VariantNumeric(VariantNumeric::default()),
        Property::VariantEastAsian(VariantEastAsian::default()),
        Property::VariantPosition(VariantPosition::Normal),
        Property::VariantHistorical(false),
        Property::OpticalSizing(true),
    ])
}
