//! Bounded CSS reference discovery; this never executes CSS or opens paths.
use std::collections::BTreeSet;

use cssparser::{ParseError, Parser, Token};

use crate::epub::Book;

const MAX_NESTING: usize = 16;

pub(super) fn collect(css: &str, base: &str, paths: &mut BTreeSet<String>, limit: usize) {
    fn add(base: &str, value: &str, paths: &mut BTreeSet<String>, limit: usize) {
        if paths.len() >= limit {
            return;
        }
        if let Ok(path) = Book::resolve(base, value)
            && super::local_html_resource_limit(&path).is_some()
        {
            paths.insert(path);
        }
    }
    fn scan(
        input: &mut Parser<'_>,
        base: &str,
        paths: &mut BTreeSet<String>,
        limit: usize,
        depth: usize,
    ) {
        if depth > MAX_NESTING {
            return;
        }
        let mut import = false;
        while paths.len() < limit {
            let Ok(token) = input.next().cloned() else {
                break;
            };
            match token {
                Token::AtKeyword(name) => import = name.eq_ignore_ascii_case("import"),
                Token::QuotedString(value) if import => {
                    add(base, &value, paths, limit);
                    import = false;
                }
                Token::UnquotedUrl(value) => {
                    add(base, &value, paths, limit);
                    import = false;
                }
                Token::Function(name) if name.eq_ignore_ascii_case("url") => {
                    let value: Result<String, ParseError<()>> =
                        input.parse_nested_block(|nested| {
                            let value = nested.expect_string_cloned()?.to_string();
                            nested.expect_exhausted()?;
                            Ok(value)
                        });
                    if let Ok(value) = value {
                        add(base, &value, paths, limit);
                    }
                    import = false;
                }
                Token::Function(_)
                | Token::ParenthesisBlock
                | Token::SquareBracketBlock
                | Token::CurlyBracketBlock => {
                    let _: Result<(), ParseError<()>> = input.parse_nested_block(|nested| {
                        scan(nested, base, paths, limit, depth + 1);
                        Ok(())
                    });
                    import = false;
                }
                _ => import = false,
            }
        }
    }
    scan(&mut Parser::new(css), base, paths, limit, 0);
}
