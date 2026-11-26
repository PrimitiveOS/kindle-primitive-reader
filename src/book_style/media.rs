//! Bounded publisher media-type rules; unknown conditions never match.
use super::*;

const MAX_MEDIA_DEPTH: u8 = 4;

enum Prelude {
    Ordinary(SheetPrelude),
    Media(bool),
    IgnoredImport,
}

struct PublisherParser<'a> {
    depth: u8,
    remaining: &'a mut usize,
    context: Option<MediaContext>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MediaContext {
    width: u32,
    height: u32,
    dpi: u16,
}

impl MediaContext {
    pub(crate) fn from_settings(settings: crate::Settings) -> Self {
        Self {
            width: settings.text_width(),
            height: settings.text_height(),
            dpi: settings.render_dpi,
        }
    }
}

fn feature_query(input: &mut Parser<'_>, context: Option<MediaContext>) -> CssResult<Option<bool>> {
    input.expect_parenthesis_block()?;
    input.parse_nested_block(|input| {
        let feature = input.expect_ident_cloned()?.to_ascii_lowercase();
        input.expect_colon()?;
        if feature == "orientation" {
            let value = input.expect_ident_cloned()?.to_ascii_lowercase();
            input.expect_exhausted()?;
            return Ok(context.and_then(|context| match value.as_str() {
                "portrait" => Some(context.height >= context.width),
                "landscape" => Some(context.height < context.width),
                _ => None,
            }));
        }
        let pixels = match input.next()?.clone() {
            Token::Dimension { value, unit, .. }
                if unit.eq_ignore_ascii_case("px")
                    && value.is_finite()
                    && (0.0..=32767.0).contains(&value)
                    && value.fract() == 0.0 =>
            {
                value as u32
            }
            Token::Number { value: 0.0, .. } => 0,
            _ => return Err(ParseError::unexpected_token()),
        };
        input.expect_exhausted()?;
        Ok(context.and_then(|context| {
            let scaled = u64::from(pixels)
                * 256
                * u64::from(if context.dpi == 0 { 96 } else { context.dpi })
                / 96;
            let scaled = ((scaled + 127) / 256).max(u64::from(pixels != 0 && context.dpi != 0));
            let (actual, relation) = match feature.as_str() {
                "width" => (context.width, 0),
                "min-width" => (context.width, 1),
                "max-width" => (context.width, -1),
                "height" => (context.height, 0),
                "min-height" => (context.height, 1),
                "max-height" => (context.height, -1),
                _ => return None,
            };
            Some(match relation {
                1 => u64::from(actual) >= scaled,
                -1 => u64::from(actual) <= scaled,
                _ => u64::from(actual) == scaled,
            })
        }))
    })
}

fn media_query_known(
    input: &mut Parser<'_>,
    context: Option<MediaContext>,
) -> CssResult<Option<bool>> {
    // A query may start with a feature or a medium. The accumulator remains
    // indeterminate if any feature is unknown, even when another is false.
    if let Ok(value) = input.try_parse(|input| feature_query(input, context)) {
        return media_conjunction(input, context, value, 1);
    }
    let Ok(first) = input.try_parse(Parser::expect_ident_cloned) else {
        // Feature-only or unknown branches are false independently; another
        // supported alternative in this comma-separated list may still match.
        while input.next().is_ok() {}
        return Ok(None);
    };
    let first = first.to_ascii_lowercase();
    let (modifier, medium) = match first.as_str() {
        "only" | "not" => {
            let Ok(medium) = input.try_parse(Parser::expect_ident_cloned) else {
                while input.next().is_ok() {}
                return Ok(None);
            };
            (first.as_str(), medium.to_ascii_lowercase())
        }
        _ => ("", first.clone()),
    };
    let matches = match medium.as_str() {
        "all" | "screen" => true,
        "print" => false,
        _ => {
            while input.next().is_ok() {}
            return Ok(None);
        }
    };
    if modifier == "not" {
        // Invert a bare medium only, never a feature or an unknown condition.
        if input.expect_exhausted().is_err() {
            while input.next().is_ok() {}
            return Ok(None);
        }
        return Ok(Some(!matches));
    }
    media_conjunction(input, context, Some(matches), 0)
}

fn media_conjunction(
    input: &mut Parser<'_>,
    context: Option<MediaContext>,
    mut result: Option<bool>,
    mut count: usize,
) -> CssResult<Option<bool>> {
    const MAX_FEATURES: usize = 8;
    while input
        .try_parse(|input| input.expect_ident_matching("and"))
        .is_ok()
    {
        count += 1;
        let value = input
            .try_parse(|input| feature_query(input, context))
            .ok()
            .flatten();
        result = result.zip(value).map(|(left, right)| left && right);
        if count > MAX_FEATURES || value.is_none() {
            while input.next().is_ok() {}
            return Ok(None);
        }
    }
    if input.expect_exhausted().is_err() {
        while input.next().is_ok() {}
        return Ok(None);
    }
    Ok(result)
}

/// Only discard an HTML stylesheet when every media alternative is known not
/// to match. Unknown, malformed and oversized attributes retain old behavior.
pub(crate) fn stylesheet_known_nonmatching(value: &str) -> bool {
    stylesheet_nonmatching_with_context(value, None)
}

pub(super) fn stylesheet_nonmatching_with_context(
    value: &str,
    context: Option<MediaContext>,
) -> bool {
    if value.is_empty() || value.len() > 1024 {
        return false;
    }
    let mut parser = Parser::new(value);
    parser
        .parse_comma_separated(|input| media_query_known(input, context))
        .is_ok_and(|queries| {
            !queries.is_empty() && queries.iter().all(|query| *query == Some(false))
        })
}

// Only invert a proven result. Unknown or incompletely implemented conditions
// are distinct from known unsupported grid/flex layouts.
fn supports_query(input: &mut Parser<'_>) -> CssResult<bool> {
    let negated = input
        .try_parse(|input| input.expect_ident_matching("not"))
        .is_ok();
    input.expect_parenthesis_block()?;
    let supported = input.parse_nested_block(|input| {
        let state = input.state();
        let name = input.expect_ident_cloned()?;
        if name.eq_ignore_ascii_case("display") {
            input.expect_colon()?;
            let value = input.expect_ident_cloned()?;
            input.expect_exhausted()?;
            return Ok(matches!(
                value.to_ascii_lowercase().as_str(),
                "grid" | "inline-grid" | "flex" | "inline-flex"
            )
            .then_some(false));
        }
        if !matches!(
            name.to_ascii_lowercase().as_str(),
            "font-style" | "font-weight" | "text-align"
        ) {
            while input.next().is_ok() {}
            return Ok(None);
        }
        input.expect_colon()?;
        let supported = Properties.parse_value(name, input, &state).is_ok();
        if input.expect_exhausted().is_err() {
            while input.next().is_ok() {}
            return Ok(None);
        }
        Ok(supported.then_some(true))
    })?;
    input.expect_exhausted()?;
    Ok(supported.is_some_and(|supported| supported != negated))
}

impl<'i> QualifiedRuleParser<'i> for PublisherParser<'_> {
    type Prelude = Vec<Selector>;
    type QualifiedRule = Vec<SheetItem>;
    type Error = ();

    fn parse_prelude(&mut self, input: &mut Parser<'i>) -> CssResult<Self::Prelude> {
        QualifiedRuleParser::parse_prelude(&mut SheetParser, input)
    }

    fn parse_block(
        &mut self,
        selectors: Self::Prelude,
        state: &ParserState,
        input: &mut Parser<'i>,
    ) -> CssResult<Self::QualifiedRule> {
        if *self.remaining == 0 {
            return Err(ParseError::unexpected_token());
        }
        *self.remaining -= 1;
        QualifiedRuleParser::parse_block(&mut SheetParser, selectors, state, input)
            .map(|item| vec![item])
    }
}

impl<'i> AtRuleParser<'i> for PublisherParser<'_> {
    type Prelude = Prelude;
    type AtRule = Vec<SheetItem>;
    type Error = ();

    fn parse_prelude(&mut self, name: CowRcStr<'i>, input: &mut Parser<'i>) -> CssResult<Prelude> {
        if name.eq_ignore_ascii_case("media") {
            let queries = input.parse_comma_separated(|input| {
                media_query_known(input, self.context).map(|value| value.unwrap_or(false))
            })?;
            return Ok(Prelude::Media(queries.into_iter().any(|value| value)));
        }
        if name.eq_ignore_ascii_case("supports") {
            return supports_query(input).map(Prelude::Media);
        }
        if name.eq_ignore_ascii_case("import") && self.depth == 0 {
            // Preserve the existing bare-import parser and add only the
            // media forms already understood by the publisher media matcher.
            if let Ok(prelude) = input.try_parse(|input| {
                AtRuleParser::parse_prelude(&mut SheetParser, name.clone(), input)
            }) {
                return Ok(Prelude::Ordinary(prelude));
            }
            let href = input
                .try_parse(|input| input.expect_string_cloned())
                .or_else(|_| input.expect_url())?
                .to_string();
            // These are import qualifiers, not media-query alternatives.
            // Do not let a later matching comma branch bypass their absence.
            let start = input.state();
            let unsupported_qualifier = matches!(
                input.next(),
                Ok(Token::Ident(name) | Token::Function(name))
                    if name.eq_ignore_ascii_case("layer")
                        || name.eq_ignore_ascii_case("supports")
            );
            input.reset(&start);
            if unsupported_qualifier {
                return Err(ParseError::unexpected_token());
            }
            let queries = input.parse_comma_separated(|input| {
                media_query_known(input, self.context).map(|value| value.unwrap_or(false))
            })?;
            return Ok(if queries.into_iter().any(|matches| matches) {
                Prelude::Ordinary(SheetPrelude::Import(href))
            } else {
                Prelude::IgnoredImport
            });
        }
        AtRuleParser::parse_prelude(&mut SheetParser, name, input).map(Prelude::Ordinary)
    }

    fn rule_without_block(
        &mut self,
        prelude: Prelude,
        state: &ParserState,
    ) -> Result<Vec<SheetItem>, ()> {
        if *self.remaining == 0 {
            return Err(());
        }
        *self.remaining -= 1;
        match prelude {
            Prelude::Ordinary(SheetPrelude::Import(href)) if self.depth == 0 => {
                Ok(vec![SheetItem::Import(href)])
            }
            Prelude::Ordinary(other) => {
                AtRuleParser::rule_without_block(&mut SheetParser, other, state).map(|_| Vec::new())
            }
            Prelude::IgnoredImport => Ok(Vec::new()),
            Prelude::Media(_) => Err(()),
        }
    }

    fn parse_block(
        &mut self,
        prelude: Prelude,
        state: &ParserState,
        input: &mut Parser<'i>,
    ) -> CssResult<Vec<SheetItem>> {
        if *self.remaining == 0 {
            return Err(ParseError::unexpected_token());
        }
        *self.remaining -= 1;
        match prelude {
            Prelude::IgnoredImport => Err(ParseError::unexpected_token()),
            Prelude::Ordinary(other) => {
                AtRuleParser::parse_block(&mut SheetParser, other, state, input)
                    .map(|item| vec![item])
            }
            Prelude::Media(matches) => {
                let mut result = vec![SheetItem::MediaBoundary];
                if matches && self.depth < MAX_MEDIA_DEPTH {
                    let mut parser = PublisherParser {
                        depth: self.depth + 1,
                        remaining: self.remaining,
                        context: self.context,
                    };
                    for items in StyleSheetParser::new(input, &mut parser)
                        .take(MAX_RULES)
                        .flatten()
                    {
                        result.extend(items);
                    }
                }
                Ok(result)
            }
        }
    }
}

pub(super) fn publisher_items(input: &mut Parser<'_>) -> Vec<SheetItem> {
    publisher_items_with_context(input, None)
}

pub(super) fn publisher_items_with_context(
    input: &mut Parser<'_>,
    context: Option<MediaContext>,
) -> Vec<SheetItem> {
    let mut remaining = MAX_RULES;
    let mut parser = PublisherParser {
        depth: 0,
        remaining: &mut remaining,
        context,
    };
    StyleSheetParser::new(input, &mut parser)
        .take(MAX_RULES)
        .flatten()
        .flatten()
        .collect()
}
