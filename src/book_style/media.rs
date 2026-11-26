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
}

fn media_query(input: &mut Parser<'_>) -> CssResult<bool> {
    let Ok(first) = input.try_parse(Parser::expect_ident_cloned) else {
        // Feature-only or unknown branches are false independently; another
        // supported alternative in this comma-separated list may still match.
        while input.next().is_ok() {}
        return Ok(false);
    };
    let first = first.to_ascii_lowercase();
    let (modifier, medium) = match first.as_str() {
        "only" | "not" => {
            let Ok(medium) = input.try_parse(Parser::expect_ident_cloned) else {
                while input.next().is_ok() {}
                return Ok(false);
            };
            (first.as_str(), medium.to_ascii_lowercase())
        }
        _ => ("", first.clone()),
    };
    if input.expect_exhausted().is_err() {
        // Includes feature queries and unknown conjunctions. Never invert an
        // unsupported condition, even when the query starts with `not`.
        while input.next().is_ok() {}
        return Ok(false);
    }
    let matches = match medium.as_str() {
        "all" | "screen" => true,
        "print" => false,
        _ => return Ok(false),
    };
    Ok(if modifier == "not" { !matches } else { matches })
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
            let queries = input.parse_comma_separated(media_query)?;
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
            let queries = input.parse_comma_separated(media_query)?;
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
    let mut remaining = MAX_RULES;
    let mut parser = PublisherParser {
        depth: 0,
        remaining: &mut remaining,
    };
    StyleSheetParser::new(input, &mut parser)
        .take(MAX_RULES)
        .flatten()
        .flatten()
        .collect()
}
