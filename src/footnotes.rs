//! Recognize publication notes before opening a reflowable internal link.
//! The note gets a separate short-lived layout; saved book anchors stay in the original chapter.
use roxmltree::{Document, Node, ParsingOptions};

const EPUB: &str = "http://www.idpf.org/2007/ops";
const XML: &str = "http://www.w3.org/XML/1998/namespace";
const MAX_CHARS: usize = 10_000;
const MAX_MARKUP: usize = 96 * 1024;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Reference {
    Explicit,
    Ordinary,
    Raised,
    #[default]
    Unknown,
}

impl Reference {
    pub fn from_node(link: Node<'_, '_>) -> Self {
        let marked = |node| has_type(node, &["noteref"], &["doc-noteref"]);
        let excluded = |node| has_type(node, &["link", "backlink"], &["doc-link", "doc-backlink"]);
        for item in std::iter::once(link).chain(link.children().filter(Node::is_element)) {
            if marked(item) {
                return Self::Explicit;
            }
            if excluded(item) {
                return Self::Ordinary;
            }
        }
        let raised = |n: Node<'_, '_>| {
            n.has_tag_name("sup")
                || n.has_tag_name("sub")
                || n.attribute("style").is_some_and(|css| {
                    css.split(';').any(|decl| {
                        decl.split_once(':').is_some_and(|(key, value)| {
                            key.trim().eq_ignore_ascii_case("vertical-align")
                                && matches!(value.trim(), "super" | "sub" | "top" | "bottom")
                        })
                    })
                })
        };
        if link
            .ancestors()
            .take_while(|n| !n.has_tag_name("body"))
            .any(raised)
            || link
                .descendants()
                .filter(Node::is_text)
                .filter(|n| n.text().is_some_and(|t| !t.trim().is_empty()))
                .all(|n| n.ancestors().take_while(|n| *n != link).any(raised))
        {
            Self::Raised
        } else {
            Self::Unknown
        }
    }
}

pub(crate) struct ReferenceContext {
    pub kind: Reference,
    pub label: String,
    pub in_prose: bool,
    pub source_block: usize,
}

fn has_type(node: Node<'_, '_>, epub: &[&str], roles: &[&str]) -> bool {
    [
        (
            node.attribute((EPUB, "type"))
                .or_else(|| node.attribute("type")),
            epub,
        ),
        (node.attribute("role"), roles),
    ]
    .into_iter()
    .any(|(value, allowed)| {
        value.is_some_and(|value| {
            value
                .split_ascii_whitespace()
                .any(|word| allowed.iter().any(|token| token.eq_ignore_ascii_case(word)))
        })
    })
}

fn heading(node: Node<'_, '_>) -> bool {
    matches!(
        node.tag_name().name(),
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6"
    )
}
fn note_container(node: Node<'_, '_>) -> bool {
    has_type(
        node,
        &["note", "footnote", "rearnote", "endnote"],
        &["doc-note", "doc-footnote", "doc-rearnote", "doc-endnote"],
    )
}
fn block(node: Node<'_, '_>) -> bool {
    matches!(
        node.tag_name().name(),
        "p" | "li" | "dd" | "dt" | "aside" | "div" | "section" | "blockquote" | "article"
    )
}

fn looks_like_marker(text: &str) -> bool {
    let text = text.trim().trim_matches(|c: char| !c.is_alphanumeric());
    if text.is_empty() || text.len() > 4 || !text.is_ascii() {
        return false;
    }
    let digits = text.bytes().filter(u8::is_ascii_digit).count();
    let letters = text.bytes().filter(u8::is_ascii_alphabetic).count();
    digits + letters == text.len()
        && (letters == 0 && digits <= 3 || (1..=2).contains(&letters) && digits <= 2)
}

fn visible_text(node: Node<'_, '_>) -> bool {
    node.descendants()
        .any(|n| n.is_text() && n.text().is_some_and(|t| !t.trim().is_empty()))
}

fn text_container<'a, 'i>(target: Node<'a, 'i>) -> Option<Node<'a, 'i>> {
    if let Some(note) = target.ancestors().find(|n| note_container(*n)) {
        return Some(note);
    }
    if !visible_text(target) && !target.has_tag_name("img") {
        // Anchor-only elements are commonly placed between adjacent paragraphs.
        // An otherwise empty paragraph may wrap the anchor. Stop walking outward
        // once the current element shares a paragraph with actual note text.
        for position in target
            .ancestors()
            .take_while(|node| node.is_element() && !visible_text(*node))
        {
            if let Some(next) = position.next_siblings().skip(1).find(Node::is_element)
                && block(next)
                && visible_text(next)
            {
                return Some(next);
            }
        }
    }
    target
        .ancestors()
        .find(|node| block(*node) && visible_text(*node))
}

fn attr(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('"', "&quot;")
}

pub(crate) fn extract(
    xhtml: &str,
    fragment: &str,
    context: &ReferenceContext,
    chapter_order: std::cmp::Ordering,
    toc_target: bool,
    prefer: bool,
) -> Option<String> {
    if context.kind == Reference::Ordinary || fragment.is_empty() {
        return None;
    }
    let stripped;
    let mut xml = xhtml;
    let options = || ParsingOptions {
        nodes_limit: 100_000,
        ..Default::default()
    };
    let doc = match Document::parse_with_options(xml, options()) {
        Err(roxmltree::Error::DtdDetected) if xml.contains("<!DOCTYPE html>") => {
            stripped = xml.replacen("<!DOCTYPE html>", "               ", 1);
            xml = &stripped;
            Document::parse_with_options(xml, options()).ok()?
        }
        other => other.ok()?,
    };
    let target = doc.descendants().find(|n| {
        n.attribute("id") == Some(fragment)
            || n.attribute((XML, "id")) == Some(fragment)
            || n.has_tag_name("a") && n.attribute("name") == Some(fragment)
    })?;
    let container = text_container(target)?;
    let explicit = context.kind == Reference::Explicit || note_container(container);
    if !explicit {
        let after = match chapter_order {
            std::cmp::Ordering::Less => true,
            std::cmp::Ordering::Equal => target.range().start > context.source_block,
            std::cmp::Ordering::Greater => false,
        };
        if !context.in_prose
            || toc_target
            || (!prefer && !after)
            || target.ancestors().any(heading)
            || container.descendants().any(heading)
            || !(prefer || context.kind == Reference::Raised || looks_like_marker(&context.label))
        {
            return None;
        }
    }
    let body = &xml[container.range()];
    if body.len() > MAX_MARKUP || !visible_text(container) {
        return None;
    }
    let length = container
        .descendants()
        .filter(Node::is_text)
        .filter_map(|n| n.text())
        .flat_map(str::chars)
        .take(MAX_CHARS + 1)
        .count();
    if length > MAX_CHARS {
        return None;
    }
    let mut result = String::from("<html");
    for ns in container.namespaces() {
        match ns.name() {
            Some("xml") => {}
            Some(prefix) => result.push_str(&format!(" xmlns:{prefix}=\"{}\"", attr(ns.uri()))),
            None => result.push_str(&format!(" xmlns=\"{}\"", attr(ns.uri()))),
        }
    }
    for (key, value) in [
        (
            "xml:lang",
            container
                .ancestors()
                .find_map(|n| n.attribute((XML, "lang")).or_else(|| n.attribute("lang"))),
        ),
        (
            "dir",
            container.ancestors().find_map(|n| n.attribute("dir")),
        ),
    ] {
        if let Some(value) = value {
            result.push_str(&format!(" {key}=\"{}\"", attr(value)));
        }
    }
    result.push_str("><head><title>Footnote</title></head><body>");
    result.push_str(body);
    result.push_str("</body></html>");
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cmp::Ordering;

    fn context(kind: Reference, label: &str) -> ReferenceContext {
        ReferenceContext {
            kind,
            label: label.into(),
            in_prose: true,
            source_block: 0,
        }
    }

    #[test]
    fn semantic_footnote_keeps_only_its_container_and_inherits_language_and_direction() {
        let xml = r##"<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops" xml:lang="ar"><body dir="rtl"><p>First paragraph</p><aside epub:type="footnote"><p><span id="n"/>A <em>marked</em> phrase.</p><p><a href="#r" role="doc-backlink">Return</a></p></aside><p>Later paragraph</p></body></html>"##;
        let excerpt = extract(
            xml,
            "n",
            &context(Reference::Unknown, "anything"),
            Ordering::Greater,
            false,
            false,
        )
        .unwrap();
        let doc = Document::parse(&excerpt).unwrap();
        assert_eq!(doc.root_element().attribute((XML, "lang")), Some("ar"));
        assert_eq!(doc.root_element().attribute("dir"), Some("rtl"));
        assert!(excerpt.contains("<em>marked</em>") && excerpt.contains("doc-backlink"));
        assert!(!excerpt.contains("First paragraph") && !excerpt.contains("Later paragraph"));
        assert!(
            extract(
                xml,
                "n",
                &context(Reference::Ordinary, "1"),
                Ordering::Less,
                false,
                true
            )
            .is_none()
        );
    }

    #[test]
    fn ordinary_navigation_and_missing_or_oversized_notes_keep_their_full_page_jump() {
        let xml = r#"<html><body><p><a name="n"/></p><p>Actual note.</p><h2 id="section">Heading</h2><p>Next paragraph.</p></body></html>"#;
        let ctx = context(Reference::Unknown, "[2]");
        assert!(
            extract(xml, "n", &ctx, Ordering::Less, false, false)
                .unwrap()
                .contains("<p>Actual note.</p>")
        );
        assert!(extract(xml, "n", &ctx, Ordering::Greater, false, false).is_none());
        assert!(extract(xml, "n", &ctx, Ordering::Greater, false, true).is_some());
        assert!(extract(xml, "n", &ctx, Ordering::Less, true, true).is_none());
        assert!(extract(xml, "section", &ctx, Ordering::Less, false, true).is_none());
        assert!(extract(xml, "absent", &ctx, Ordering::Less, false, true).is_none());
        assert!(extract(xml, "", &ctx, Ordering::Less, false, true).is_none());
        assert!(
            extract(
                xml,
                "n",
                &context(Reference::Unknown, "next chapter"),
                Ordering::Less,
                false,
                false
            )
            .is_none()
        );
        assert!(
            extract(
                xml,
                "n",
                &ReferenceContext {
                    in_prose: false,
                    ..ctx
                },
                Ordering::Less,
                false,
                false
            )
            .is_none()
        );
        let long = format!(
            "<html><body><aside id='big' role='doc-endnote'>{}</aside></body></html>",
            "a".repeat(MAX_CHARS + 1)
        );
        assert!(
            extract(
                &long,
                "big",
                &context(Reference::Explicit, "1"),
                Ordering::Less,
                false,
                true
            )
            .is_none()
        );
    }

    #[test]
    fn reference_uses_publisher_semantics_before_marker_appearance() {
        let xml = r#"<p xmlns:epub="http://www.idpf.org/2007/ops"><a epub:type="noteref">a</a><a role="doc-backlink"><sup>b</sup></a><sup><a>c</a></sup><a><span style="vertical-align: super">d</span></a><a>Ordinary</a><a type="NOTEREF">e</a></p>"#;
        let doc = Document::parse(xml).unwrap();
        let kinds: Vec<_> = doc
            .descendants()
            .filter(|n| n.has_tag_name("a"))
            .map(Reference::from_node)
            .collect();
        assert_eq!(
            kinds,
            vec![
                Reference::Explicit,
                Reference::Ordinary,
                Reference::Raised,
                Reference::Raised,
                Reference::Unknown,
                Reference::Explicit
            ]
        );
    }
}
