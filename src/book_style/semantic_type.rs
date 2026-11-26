//! Semantic type selectors used by publisher footnote rules. CRE intentionally
//! ignores namespace prefixes; this does not implement CSS namespace bindings.
use super::*;

pub(super) enum Attribute {
    Href,
    Value(Selector),
}
#[derive(Clone, Copy)]
enum Field {
    Type,
    Class,
    Id,
    Href,
}
#[derive(Clone, Copy)]
enum Operation {
    Present,
    Equal,
    Word,
    Dash,
    Prefix,
    Suffix,
    Substring,
}
pub(super) struct Selector {
    field: Field,
    operation: Operation,
    value: String,
    insensitive: bool,
}
impl Selector {
    pub(super) fn ambiguous(&self, element: &Element) -> bool {
        let mask = match self.field {
            Field::Class => 1,
            Field::Id => 2,
            Field::Href => 4,
            Field::Type => 0,
        };
        element.selector_ambiguous & mask != 0
    }
    pub(super) fn matches(&self, element: &Element) -> bool {
        let value = match self.field {
            Field::Type => element.type_attribute.as_deref(),
            Field::Class => element.selector_class_attribute.as_deref(),
            Field::Id => element.selector_id_attribute.as_deref(),
            Field::Href => element.selector_href_attribute.as_deref(),
        };
        let Some(value) = value else {
            return false;
        };
        if matches!(self.operation, Operation::Present) {
            return true;
        }
        let folded;
        // The pinned CREngine's dash-word branch checks the prefix operator's
        // enum before folding, so its `i` flag folds the pattern but not the
        // attribute. Preserve that behavior until upstream changes it.
        let value = if self.insensitive && !matches!(self.operation, Operation::Dash) {
            folded = value.to_lowercase();
            folded.as_str()
        } else {
            value
        };
        match self.operation {
            Operation::Present => true,
            Operation::Equal => value == self.value,
            // Pinned CRE uses literal U+0020 boundaries. XML normalizes literal
            // attribute whitespace; character-reference tabs remain distinct.
            Operation::Word => value.split(' ').any(|word| word == self.value),
            Operation::Dash => {
                value == self.value
                    || value
                        .strip_prefix(self.value.as_str())
                        .is_some_and(|rest| rest.starts_with('-'))
            }
            Operation::Prefix => value.starts_with(self.value.as_str()),
            Operation::Suffix => value.ends_with(self.value.as_str()),
            Operation::Substring => value.contains(self.value.as_str()),
        }
    }
}
pub(super) fn parse(input: &mut Parser<'_>) -> CssResult<Attribute> {
    let name = match input.next()?.clone() {
        Token::Ident(name) => {
            if input.try_parse(|p| p.expect_delim('|')).is_ok() {
                input.expect_ident_cloned()?.to_string()
            } else {
                name.to_string()
            }
        }
        Token::Delim('*') => {
            input.expect_delim('|')?;
            input.expect_ident_cloned()?.to_string()
        }
        Token::Delim('|') => input.expect_ident_cloned()?.to_string(),
        _ => return Err(ParseError::unexpected_token()),
    };
    if name.eq_ignore_ascii_case("href") && input.is_exhausted() {
        return Ok(Attribute::Href);
    }
    let field = match name.as_str() {
        "type" => Field::Type,
        "class" => Field::Class,
        "id" => Field::Id,
        value if value.eq_ignore_ascii_case("href") => Field::Href,
        _ => return Err(ParseError::unexpected_token()),
    };
    let operation = if input.is_exhausted() {
        Operation::Present
    } else {
        match input.next()? {
            Token::Delim('=') => Operation::Equal,
            Token::IncludeMatch => Operation::Word,
            Token::DashMatch => Operation::Dash,
            Token::PrefixMatch => Operation::Prefix,
            Token::SuffixMatch => Operation::Suffix,
            Token::SubstringMatch => Operation::Substring,
            _ => return Err(ParseError::unexpected_token()),
        }
    };
    let mut value = String::new();
    let mut insensitive = false;
    if !matches!(operation, Operation::Present) {
        value = input.expect_ident_or_string()?.to_string();
        if matches!(operation, Operation::Word) && (value.is_empty() || value.contains(' ')) {
            return Err(ParseError::unexpected_token());
        }
        if !input.is_exhausted() {
            input.expect_ident_matching("i")?;
            insensitive = true;
            value = value.to_lowercase();
        }
    }
    input.expect_exhausted()?;
    Ok(Attribute::Value(Selector {
        field,
        operation,
        value,
        insensitive,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn selected(selector: &str, xml: &str) -> Vec<String> {
        let source = Source::parse(xml).unwrap();
        let selector = super::super::Selector::parse(&mut Parser::new(selector)).unwrap();
        source
            .elements
            .iter()
            .enumerate()
            .filter(|(index, _)| selector.matches(&source.elements, *index))
            .filter_map(|(_, e)| e.id.clone())
            .collect()
    }
    #[test]
    fn semantic_type_presence_exact_and_word_selectors_retain_cre_local_name_policy() {
        let xml = r#"<html xmlns:epub="http://www.idpf.org/2007/ops" xmlns:other="urn:other"><body><aside id="a" epub:type="footnote"/><aside id="b" other:type="endnote footnote"/><p id="c" type="footnotes"/><p id="d"/><p id="empty" type=""/></body></html>"#;
        for selector in [
            "[epub|type~=footnote]",
            "[unknown|type~=footnote]",
            "[*|type~=footnote]",
            "[|type~=footnote]",
            "[type~=footnote]",
        ] {
            assert_eq!(selected(selector, xml), ["a", "b"]);
        }
        assert_eq!(selected("aside[epub|type=footnote]", xml), ["a"]);
        assert_eq!(selected("[type]", xml), ["a", "b", "c", "empty"]);
        assert_eq!(selected("[type=\"\"]", xml), ["empty"]);
    }
    #[test]
    fn type_token_case_and_character_reference_boundaries_follow_the_pinned_subset() {
        let xml = "<html><body><p id='caps' type='FOOTNOTE'/><p id='literal' type='endnote&#x20;footnote'/><p id='tab' type='endnote&#x9;footnote'/><p id='suffix' type='footnotes'/></body></html>";
        assert_eq!(selected("[type~=footnote]", xml), ["literal"]);
        assert_eq!(selected("[type~=footnote i]", xml), ["caps", "literal"]);
        assert_eq!(selected("[type=FOOTNOTE]", xml), ["caps"]);
    }
    #[test]
    fn unsupported_attributes_operators_and_flags_are_not_silently_broadened() {
        for selector in [
            "[role=footnote]",
            "[type|=footnote]",
            "[type*=footnote]",
            "[type=footnote s]",
            "[type~=\"\"]",
            "[type~=\"two words\"]",
            "[href=value]",
            "[epub|]",
        ] {
            assert!(
                super::super::Selector::parse(&mut Parser::new(selector)).is_err(),
                "{selector}"
            );
        }
    }
}

#[cfg(all(test, feature = "kindle"))]
mod reading_tests {
    use super::*;
    use crate::{Engine, Probe, Settings, flow_view::ChapterFlow, user_styles::Snapshot};

    #[test]
    fn publisher_semantic_rule_omits_main_flow_without_changing_source_or_popup_inline_css() {
        let xml = r##"<html xmlns:epub="http://www.idpf.org/2007/ops"><head><style>@namespace epub "http://www.idpf.org/2007/ops"; aside[epub|type~="footnote"]{display:none}</style></head><body><p>Visible prose<a epub:type="noteref" href="#n">1</a></p><aside id="n" epub:type="footnote"><p>Independent note</p></aside></body></html>"##;
        let open = |publisher_styles| {
            Probe::from_book_content(
                Book::open(&crate::test_fixture("field-notes-epub3.epub")).unwrap(),
                Engine::Parley,
                &[crate::test_fixture("fonts/NotoSans-Regular.ttf")],
                Settings {
                    width: 600,
                    height: 800,
                    publisher_styles,
                    ..Default::default()
                },
                0,
                Some(xml.into()),
            )
            .unwrap()
        };
        let plain = open(false);
        let styled = open(true);
        assert_eq!(plain.source.revision, styled.source.revision);
        assert_eq!(plain.source.fragments, styled.source.fragments);
        assert_eq!(
            plain
                .source
                .blocks
                .iter()
                .map(|b| &b.text)
                .collect::<Vec<_>>(),
            styled
                .source
                .blocks
                .iter()
                .map(|b| &b.text)
                .collect::<Vec<_>>()
        );
        assert!(!plain.matches("Independent note").is_empty());
        assert!(styled.matches("Independent note").is_empty());
        assert_eq!(
            styled.fragment_presentation("n"),
            Some(crate::SourcePresentation::CssOmitted)
        );
        assert!(styled.fragment("n").is_none());
        let excerpt = crate::footnotes::extract(
            xml,
            "n",
            &crate::footnotes::ReferenceContext {
                kind: crate::footnotes::Reference::Explicit,
                label: "1".into(),
                in_prose: true,
                source_block: 0,
            },
            std::cmp::Ordering::Equal,
            false,
            false,
        )
        .unwrap();
        assert!(!excerpt.contains("<style"));
        let mut popup = Probe::from_book_content(
            Book::open(&crate::test_fixture("field-notes-epub3.epub")).unwrap(),
            Engine::Parley,
            &[crate::test_fixture("fonts/NotoSans-Regular.ttf")],
            Settings {
                width: 600,
                height: 800,
                ..Default::default()
            },
            0,
            Some(excerpt),
        )
        .unwrap();
        assert!(!popup.matches("Independent note").is_empty());
        assert!(popup.capture(0).unwrap().iter().any(|&v| v < 255));
    }

    #[test]
    #[ignore = "Requires an authorized unchanged owner EPUB via READER_SEMANTIC_ORIGINAL"]
    fn original_semantic_footnotes_keep_canonical_text_and_leave_main_flow_explicitly() {
        let path = std::path::PathBuf::from(std::env::var_os("READER_SEMANTIC_ORIGINAL").unwrap());
        let before = ring::digest::digest(&ring::digest::SHA256, &std::fs::read(&path).unwrap());
        let mut book = Book::open(&path).unwrap();
        let mut notes = 0;
        let mut omitted_bytes = 0;
        for chapter in 0..book.chapters.len() {
            let xml = book.chapter(chapter).unwrap();
            let source = Source::parse(&xml).unwrap();
            let href = book.chapters[chapter].href.clone();
            let style =
                BookStyle::load_for_flow(&mut book, &href, &source, true, &Snapshot::default())
                    .unwrap();
            let computed = style.compute(&source.elements);
            for (index, element) in source.elements.iter().enumerate() {
                if element
                    .type_attribute
                    .as_deref()
                    .is_some_and(|value| value.split(' ').any(|word| word == "footnote"))
                {
                    assert!(
                        computed[index].flow.suppressed,
                        "Publisher's actual note rule must match"
                    );
                    notes += 1;
                }
            }
            let flow = ChapterFlow::new(&source, computed.iter().map(|style| style.flow).collect())
                .unwrap();
            for block in &source.blocks {
                let view = flow.view(&source, block).unwrap();
                omitted_bytes += block.text.len() - view.text().len();
            }
        }
        assert!(notes > 0 && omitted_bytes > 0);
        let after = ring::digest::digest(&ring::digest::SHA256, &std::fs::read(&path).unwrap());
        assert_eq!(before.as_ref(), after.as_ref());
        println!("semantic footnotes={notes} omitted_source_bytes={omitted_bytes}");
    }
}
