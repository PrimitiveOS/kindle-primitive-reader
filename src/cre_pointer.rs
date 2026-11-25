//! Import a normalized CREngine EPUB bookmark from the publication's own XHTML.

use roxmltree::{Document, Node, ParsingOptions};

use crate::epub::{Book, EpubPosition};
use crate::source::Source;
use crate::{Anchor, Result};

const XHTML: &str = "http://www.w3.org/1999/xhtml";
const MAX_STEPS: usize = 64;
const TEXT_SPLIT_SIZE: usize = 8192;
const MARKER_ID: &str = "reader-cre-position-probe";
pub(crate) const DOM_VERSION: u32 = 20240114;

struct Step<'a> {
    name: &'a str,
    ordinal: Option<usize>,
}

impl<'a> Step<'a> {
    fn parse(raw: &'a str) -> Result<Self> {
        let (name, ordinal) = if let Some((name, index)) = raw.split_once('[') {
            let index = number(
                index
                    .strip_suffix(']')
                    .ok_or("Unclosed KOReader node index")?,
            )?;
            if index == 0 {
                return Err("KOReader node indices start at one".into());
            }
            (name, Some(index))
        } else {
            (raw, None)
        };
        if name != "text()"
            && (name.len() > 64
                || !name.starts_with(|c: char| c.is_ascii_alphabetic())
                || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'))
        {
            return Err("This KOReader node selector is unsupported".into());
        }
        Ok(Self { name, ordinal })
    }
}

fn number(raw: &str) -> Result<usize> {
    if raw.is_empty()
        || raw.len() > 9
        || !raw.bytes().all(|b| b.is_ascii_digit())
        || (raw.len() > 1 && raw.starts_with('0'))
    {
        return Err("Invalid KOReader position index".into());
    }
    Ok(raw.parse()?)
}

struct Pointer<'a> {
    fragment: Step<'a>,
    steps: Vec<Step<'a>>,
    offset: usize,
}

impl<'a> Pointer<'a> {
    fn parse(raw: &'a str) -> Result<Self> {
        if raw.len() > 8192 {
            return Err("KOReader position exceeds the import limit".into());
        }
        let (path, offset) = match raw.rsplit_once('.') {
            Some((path, offset)) => (path, number(offset)?),
            None => (raw, 0),
        };
        let mut parts = path
            .strip_prefix("/body/")
            .ok_or("Expected a normalized EPUB DocFragment position")?
            .split('/');
        let fragment = Step::parse(parts.next().unwrap_or_default())?;
        if fragment.name != "DocFragment" || parts.next() != Some("body") {
            return Err("Expected a KOReader DocFragment reading body".into());
        }
        let mut steps = Vec::new();
        for part in parts {
            if steps.len() == MAX_STEPS {
                return Err("KOReader position exceeds the nesting limit".into());
            }
            steps.push(Step::parse(part)?);
        }
        Ok(Self {
            fragment,
            steps,
            offset,
        })
    }
}

fn xml_options<'a>() -> ParsingOptions<'a> {
    ParsingOptions {
        nodes_limit: 100_000,
        ..ParsingOptions::default()
    }
}

fn structural_styles(body: Node<'_, '_>) -> bool {
    body.document().descendants().any(|node| {
        node.has_tag_name("style")
            || node.has_tag_name("link")
                && node.attribute("rel").is_some_and(|r| {
                    r.split_ascii_whitespace()
                        .any(|v| v.eq_ignore_ascii_case("stylesheet"))
                })
            || node.attribute(("http://www.w3.org/XML/1998/namespace", "space")) == Some("preserve")
            || node.attribute("style").is_some_and(|s| {
                may_preserve_spaces(s) || s.to_ascii_lowercase().contains("display")
            })
    })
}

/// Structural selectors can change which text node is addressed. Inside an already
/// resolved node, only a rule that changes whitespace processing can change its offset.
fn may_preserve_spaces(css: &str) -> bool {
    // Escaped identifiers and imports need a full cascade. Unknown rules keep the
    // existing conservative import behavior; ordinary font or color rules do not.
    css.contains('\\')
        || css
            .split(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_')
            .any(|word| {
                [
                    "white-space",
                    "white-space-collapse",
                    "white-space-trim",
                    "all",
                    "import",
                ]
                .iter()
                .any(|w| word.eq_ignore_ascii_case(w))
            })
}

fn uncertain_text_spacing(
    target: Node<'_, '_>,
    linked: &mut impl FnMut(&str) -> Option<String>,
) -> bool {
    if target.ancestors().any(|n| {
        n.attribute(("http://www.w3.org/XML/1998/namespace", "space")) == Some("preserve")
            || n.attribute("style").is_some_and(may_preserve_spaces)
    }) {
        return true;
    }
    target.document().descendants().any(|node| {
        if node.has_tag_name("style") {
            return node
                .children()
                .filter_map(|n| n.text())
                .any(may_preserve_spaces);
        }
        let sheet = node.has_tag_name("link")
            && node.attribute("rel").is_some_and(|r| {
                r.split_ascii_whitespace()
                    .any(|word| word.eq_ignore_ascii_case("stylesheet"))
            });
        sheet
            && node
                .attribute("href")
                .and_then(&mut *linked)
                .is_none_or(|css| may_preserve_spaces(&css))
    })
}

fn omitted_initial_space(parent: Node<'_, '_>, child: Node<'_, '_>) -> bool {
    let previous = child
        .prev_siblings()
        .skip(1)
        .any(|node| node.is_element() || node.is_text());
    !previous
        && child
            .text()
            .is_some_and(|s| !s.is_empty() && s.chars().all(xml_space))
        && matches!(
            parent.tag_name().name(),
            "body"
                | "div"
                | "p"
                | "section"
                | "header"
                | "article"
                | "aside"
                | "h1"
                | "h2"
                | "h3"
                | "h4"
                | "h5"
                | "h6"
                | "blockquote"
                | "li"
                | "table"
                | "td"
                | "th"
        )
}

fn xml_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r')
}

fn child<'a, 'input>(
    parent: Node<'a, 'input>,
    step: &Step<'_>,
    xhtml: &str,
    styled: bool,
) -> Result<Node<'a, 'input>> {
    let mut count = 0;
    let mut found = None;
    for node in parent.children() {
        if step.name == "text()" {
            if !node.is_text() {
                continue;
            }
            if omitted_initial_space(parent, node) {
                if styled {
                    return Err(
                        "A styled leading space makes this KOReader text index ambiguous".into(),
                    );
                }
                continue;
            }
            let raw = &xhtml[node.range()];
            if raw.chars().take(TEXT_SPLIT_SIZE).count() >= TEXT_SPLIT_SIZE
                || raw.starts_with("<![CDATA[")
            {
                return Err("This text index depends on CREngine text splitting".into());
            }
        } else if !node.has_tag_name((XHTML, step.name)) {
            continue;
        }
        count += 1;
        if step.ordinal == Some(count) {
            return Ok(node);
        }
        if step.ordinal.is_none() && found.replace(node).is_some() {
            return Err("The KOReader node selector is ambiguous".into());
        }
    }
    found.ok_or_else(|| "The saved KOReader node is absent from this chapter".into())
}

/// CREngine counts Unicode scalar values in preprocessed text. XML references emit
/// one character directly. Normal runs of ASCII whitespace emit one space.
fn text_boundary(raw: &str, requested: usize, styled: bool) -> Result<usize> {
    let mut cursor = 0;
    let mut count = 0;
    let mut space = false;
    while cursor < raw.len() {
        if count == requested {
            if space {
                while cursor < raw.len() && raw[cursor..].starts_with(xml_space) {
                    if styled {
                        return Err(
                            "Whitespace in this styled KOReader text offset is ambiguous".into(),
                        );
                    }
                    cursor += 1; // XML whitespace is ASCII.
                }
            }
            return Ok(cursor);
        }
        let c = raw[cursor..].chars().next().unwrap();
        if c == '&' {
            let end = raw[cursor..]
                .find(';')
                .ok_or("Invalid XML text reference")?;
            cursor += end + 1;
            count += 1;
            space = false;
        } else {
            cursor += c.len_utf8();
            if xml_space(c) {
                if styled && (c != ' ' || space) {
                    return Err(
                        "Whitespace in this styled KOReader text offset is ambiguous".into(),
                    );
                }
                if !space {
                    count += 1;
                }
                space = true;
            } else {
                count += 1;
                space = false;
            }
        }
    }
    if count == requested {
        Ok(cursor)
    } else {
        Err("KOReader text offset is outside this node".into())
    }
}

fn anchor_at(xhtml: &str, byte: usize, marker_id: &str) -> Result<(u64, usize, usize)> {
    // Let the same parser used for ordinary fragment links map the boundary,
    // including whitespace folding, inline runs and empty anchor elements.
    // The marker exists only during import and never supplies revision or block keys.
    let original = Source::parse(xhtml)?;
    let marker = format!("<span id=\"{marker_id}\"/>");
    let mut marked = String::with_capacity(xhtml.len() + marker.len());
    marked.push_str(&xhtml[..byte]);
    marked.push_str(&marker);
    marked.push_str(&xhtml[byte..]);
    let located = Source::parse(&marked)?;
    let &(key, offset) = located
        .fragments
        .get(marker_id)
        .ok_or("The position has no readable source anchor")?;
    if original.blocks.len() != located.blocks.len()
        || !original.blocks.iter().zip(&located.blocks).all(|(a, b)| {
            (a.key == b.key || a.key + marker.len() == b.key)
                && a.text == b.text
                && a.images
                    .iter()
                    .map(|i| i.offset)
                    .eq(b.images.iter().map(|i| i.offset))
        })
    {
        return Err("The position cannot be projected onto the original reading blocks".into());
    }
    let index = located
        .blocks
        .binary_search_by_key(&key, |b| b.key)
        .map_err(|_| "The original reading block is missing")?;
    let block = &original.blocks[index];
    if block.text.is_empty() || !block.text.is_char_boundary(offset) {
        return Err("The position is outside the original reading block".into());
    }
    Ok((original.revision, block.key, offset))
}

/// Resolve normalized pointers from the DOM version used by current saved KOReader EPUBs.
pub fn resolve(book: &mut Book, value: &str, version: Option<u32>) -> Result<EpubPosition> {
    if version != Some(DOM_VERSION) {
        return Err("This KOReader DOM version is unsupported for EPUB position import".into());
    }
    if !book.is_reflowable_epub() {
        return Err("A CREngine EPUB position requires a reflowable book".into());
    }
    let ptr = Pointer::parse(value)?;
    let index = match ptr.fragment.ordinal {
        Some(index) => index - 1,
        None if book.chapters.len() == 1 => 0,
        None => return Err("The KOReader chapter selector is ambiguous".into()),
    };
    let chapter = book
        .chapters
        .get(index)
        .ok_or("The saved KOReader chapter is unavailable")?;
    let path = chapter.href.clone();
    if book.recovered_placeholder(chapter)
        || book.chapters.iter().filter(|ch| ch.href == path).count() != 1
    {
        return Err("The saved KOReader chapter is missing or ambiguous".into());
    }
    let xhtml = book.chapter(index)?;
    let mut remaining = 512 * 1024;
    let mut sheets = 0;
    let (revision, block, offset) = locate_in_chapter(&xhtml, &ptr.steps, ptr.offset, |href| {
        if sheets == 16 || remaining == 0 {
            return None;
        }
        sheets += 1;
        let name = Book::resolve(&path, href).ok()?;
        let bytes = book.resource_limited(&name, remaining).ok()?;
        remaining -= bytes.len();
        String::from_utf8(bytes).ok()
    })?;
    Ok(EpubPosition {
        chapter: index,
        anchor: Anchor {
            illustration: None,
            book: book.revision(),
            chapter: path,
            revision,
            block,
            offset,
        },
    })
}

/// Encode a canonical source point only when both position models agree.
#[cfg(feature = "kindle")]
pub(crate) fn encode(book: &mut Book, anchor: &Anchor, end: bool) -> Result<String> {
    use std::fmt::Write;

    if !book.is_reflowable_epub() || anchor.illustration.is_some() || anchor.book != book.revision()
    {
        return Err("The mark does not address this reflowable EPUB".into());
    }
    let chapter = book
        .chapter_index(&anchor.chapter)
        .ok_or("The chapter has changed")?;
    let xhtml = book.chapter(chapter)?;
    let source = Source::parse(&xhtml)?;
    if anchor.revision != source.revision {
        return Err("The chapter text has changed".into());
    }
    let xml = crate::epub_xml::without_plain_doctype(&xhtml);
    let document = Document::parse_with_options(&xml, xml_options())?;
    let byte = source
        .publisher_text_boundary(&document, &xhtml, anchor.block, anchor.offset, end)
        .ok_or("The EPUB mark has no exact publisher text boundary")?;
    let node = document
        .descendants()
        .find(|node| {
            let range = node.range();
            node.is_text() && (range.contains(&byte) || byte == range.end)
        })
        .ok_or("The EPUB mark does not address a stock text node")?;
    let raw = &xml[node.range()];
    let distance = byte - node.range().start;
    let offset = text_offset(raw, distance).ok_or("The EPUB text boundary is ambiguous")?;

    let ancestors: Vec<_> = node.ancestors().take(MAX_STEPS + 4).collect();
    let body = ancestors
        .iter()
        .position(|node| node.has_tag_name((XHTML, "body")))
        .ok_or("The mark is outside the XHTML reading body")?;
    if body > MAX_STEPS {
        return Err("The EPUB mark exceeds the KOReader nesting limit".into());
    }
    let mut pointer = format!("/body/DocFragment[{}]/body", chapter + 1);
    for node in ancestors[..body].iter().rev().copied() {
        let parent = node.parent().ok_or("Missing KOReader parent node")?;
        let name = if node.is_text() {
            "text()"
        } else {
            node.tag_name().name()
        };
        let mut siblings = parent.children().filter(|sibling| {
            if node.is_text() {
                sibling.is_text() && !omitted_initial_space(parent, *sibling)
            } else {
                sibling.has_tag_name((XHTML, name))
            }
        });
        let ordinal = siblings
            .position(|sibling| sibling == node)
            .ok_or("The EPUB mark has no KOReader node selector")?
            + 1;
        write!(&mut pointer, "/{name}")?;
        if ordinal > 1 || siblings.next().is_some() {
            write!(&mut pointer, "[{ordinal}]")?;
        }
    }
    write!(&mut pointer, ".{offset}")?;
    if resolve(book, &pointer, Some(DOM_VERSION))?.anchor != *anchor {
        return Err("The KOReader mark cannot recover the exact source point".into());
    }
    Ok(pointer)
}

#[cfg(feature = "kindle")]
fn text_offset(raw: &str, end: usize) -> Option<usize> {
    let mut offset = 0;
    let mut chars = raw.get(..end)?.chars();
    let mut space = false;
    while let Some(ch) = chars.next() {
        if ch == '&' {
            loop {
                if chars.next()? == ';' {
                    break;
                }
            }
        } else if xml_space(ch) && space {
            continue;
        }
        offset += 1;
        space = xml_space(ch);
    }
    Some(offset)
}

/// A CRE element point is an index between children of its actual DOM node.
/// Infer it from publisher markup only for an inline tree whose child list is
/// unaffected by CSS, HTML repair, hidden nodes, or engine text splitting.
fn plain_inline_element_boundary(
    target: Node<'_, '_>,
    offset: usize,
    xml: &str,
    styled: bool,
) -> Result<usize> {
    fn inline(name: &str) -> bool {
        matches!(
            name,
            "p" | "span" | "b" | "i" | "em" | "strong" | "a" | "code" | "small" | "s"
        )
    }
    if styled || !target.is_element() || !inline(target.tag_name().name()) {
        return Err("This KOReader element point needs its rendered child tree".into());
    }
    for node in target.descendants() {
        if node.is_element() {
            if node.tag_name().namespace() != Some(XHTML)
                || !inline(node.tag_name().name())
                || node.has_attribute("hidden")
                || node.has_attribute("style")
            {
                return Err("This KOReader element point has an ambiguous child".into());
            }
        } else if node.is_text() {
            let raw = &xml[node.range()];
            if raw.starts_with("<![CDATA[")
                || raw.chars().take(TEXT_SPLIT_SIZE).count() >= TEXT_SPLIT_SIZE
                || raw.chars().all(xml_space)
            {
                return Err("This KOReader element point has ambiguous text nodes".into());
            }
        } else {
            return Err("This KOReader element point has an unsupported child node".into());
        }
    }
    let children: Vec<_> = target.children().collect();
    if offset > children.len() {
        return Err("The KOReader element point is outside this node".into());
    }
    if let Some(next) = children.get(offset) {
        Ok(next.range().start)
    } else {
        children
            .last()
            .map(|last| last.range().end)
            .ok_or_else(|| "The KOReader element point has no readable child".into())
    }
}

fn locate_in_chapter(
    xhtml: &str,
    steps: &[Step<'_>],
    offset: usize,
    mut linked: impl FnMut(&str) -> Option<String>,
) -> Result<(u64, usize, usize)> {
    let xml = crate::epub_xml::without_plain_doctype(xhtml);
    let document = Document::parse_with_options(&xml, xml_options())?;
    let root = document.root_element();
    if !root.has_tag_name((XHTML, "html")) {
        return Err("Expected an authored XHTML chapter".into());
    }
    let body = child(
        root,
        &Step {
            name: "body",
            ordinal: None,
        },
        &xml,
        false,
    )?;
    let styled = structural_styles(body);
    let mut target = body;
    for step in steps {
        if !target.is_element() {
            return Err("A KOReader text node cannot have children".into());
        }
        target = child(target, step, &xml, styled)?;
    }
    if target.ancestors().any(|n| {
        n.is_element()
            && (n.tag_name().namespace() != Some(XHTML)
                || n.has_attribute("hidden")
                || matches!(
                    n.tag_name().name(),
                    "pre" | "ruby" | "script" | "style" | "noscript" | "template"
                ))
    }) {
        return Err("The KOReader position addresses unsupported or hidden content".into());
    }
    let byte = if target.is_text() {
        let raw = &xml[target.range()];
        // Reading the linked stylesheets is only necessary for an ambiguous whitespace run.
        let byte = match text_boundary(raw, offset, styled) {
            Ok(byte) => byte,
            Err(_) if styled && !uncertain_text_spacing(target, &mut linked) => {
                text_boundary(raw, offset, false)?
            }
            Err(err) => return Err(err),
        };
        target.range().start + byte
    } else if offset == 0 && target != body {
        target.range().start
    } else if offset == 0 {
        body.first_child()
            .ok_or("The EPUB body is empty")?
            .range()
            .start
    } else {
        plain_inline_element_boundary(target, offset, &xml, styled)?
    };
    // A publication may itself use the preferred marker as an ID.
    let marker = if document
        .descendants()
        .any(|n| n.attribute("id") == Some(MARKER_ID))
    {
        let ids: std::collections::HashSet<_> = document
            .descendants()
            .filter_map(|n| n.attribute("id"))
            .collect();
        (1..=ids.len() + 1)
            .map(|i| format!("{MARKER_ID}-{i}"))
            .find(|id| !ids.contains(id.as_str()))
            .unwrap()
    } else {
        MARKER_ID.to_owned()
    };
    drop(document);
    drop(xml);
    anchor_at(xhtml, byte, &marker)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn location(markup: &str, suffix: &str) -> Result<(String, usize)> {
        let xhtml = format!("<html xmlns='{XHTML}'><head/><body>{markup}</body></html>");
        let raw = format!("/body/DocFragment[1]/body{suffix}");
        let pointer = Pointer::parse(&raw)?;
        let (revision, key, offset) =
            locate_in_chapter(&xhtml, &pointer.steps, pointer.offset, |_| None)?;
        let original = Source::parse(&xhtml)?;
        assert_eq!(revision, original.revision);
        Ok((
            original
                .blocks
                .into_iter()
                .find(|b| b.key == key)
                .unwrap()
                .text,
            offset,
        ))
    }

    #[test]
    fn maps_scalar_offsets_and_the_initial_block_space_to_the_original_source() {
        let xml = "\n <b>Beacon</b><br/>\n<br/>Count   😀 &amp; bright.";
        let (text, at) = location(xml, "/text()[2].7").unwrap();
        assert_eq!(&text[..at], "Beacon\n\nCount 😀");
        assert_eq!(&text[at..], " & bright.");
        let (text, at) = location(xml, "/text()[2].9").unwrap();
        assert_eq!(&text[at..], " bright.");
        assert!(location(xml, "/text()[1].4").is_err());
    }

    #[test]
    fn empty_nodes_and_inline_runs_preserve_the_original_reading_order() {
        let xml =
            "<h1><span/>Tide pools</h1><p><a/></p><div/>Look at the <b>north</b> inlet.<p>End.</p>";
        for (suffix, expected) in [
            ("/h1/span.0", "Tide pools"),
            ("/p[1].0", "Look at the north inlet."),
            ("/div.0", "Look at the north inlet."),
        ] {
            let (text, at) = location(xml, suffix).unwrap();
            assert_eq!(&text[at..], expected);
        }
        let (text, at) = location(xml, "/b/text().2").unwrap();
        assert_eq!(&text[at..], "rth inlet.");
        let (a, i) = location("<p>North.</p><a/>", "/a.0").unwrap();
        assert_eq!(i, a.len());
    }

    #[test]
    fn ordinary_linked_typography_keeps_a_normalized_text_offset_unambiguous() {
        let xhtml = format!(
            r#"<html xmlns='{XHTML}'><head><link rel="stylesheet" href="book.css"/></head><body><p>A   B C</p></body></html>"#
        );
        let pointer = Pointer::parse("/body/DocFragment[1]/body/p/text().2").unwrap();
        let regular = locate_in_chapter(&xhtml, &pointer.steps, 2, |_| {
            Some("p { display: block; font-family: serif; }".into())
        })
        .unwrap();
        assert_eq!(regular.2, 2);
        for css in [
            None,
            Some("p { white-space: pre-wrap }"),
            Some("@import url(other.css)"),
            Some("p { all: revert }"),
        ] {
            assert!(
                locate_in_chapter(&xhtml, &pointer.steps, 2, |_| css.map(str::to_owned)).is_err()
            );
        }
    }

    #[test]
    fn element_points_recover_simple_inline_child_boundaries() {
        let markup = "<p>Alpha <em>βeta</em> gamma <strong>delta</strong> end.</p>";
        for (suffix, expected) in [
            ("/p.1", "βeta gamma delta end."),
            ("/p.2", " gamma delta end."),
            ("/p.3", "delta end."),
            ("/p.4", " end."),
        ] {
            let (text, at) = location(markup, suffix).unwrap();
            assert_eq!(&text[at..], expected, "{suffix}");
        }
        let (text, at) = location("<p>α&amp;β<strong>more</strong></p>", "/p.1").unwrap();
        assert_eq!(&text[at..], "more");
        let (text, at) = location("<p>Hello <b>world</b></p>", "/p.2").unwrap();
        assert_eq!(at, text.len());
    }

    #[test]
    fn element_points_do_not_guess_publisher_or_engine_tree_changes() {
        for markup in [
            "<p>first<!-- hidden by parser -->second</p>",
            "<p>first <span hidden='hidden'>hidden</span> second</p>",
            "<p>first <span style='display:none'>hidden</span> second</p>",
            "<p>first <img src='a.png'/> second</p>",
            "<p><em>first</em> <strong>second</strong></p>",
            "<p><![CDATA[first]]><b>second</b></p>",
        ] {
            assert!(location(markup, "/p.1").is_err(), "{markup}");
        }
        assert!(location("<p>first<b>second</b></p>", "/p.3").is_err());
        assert!(location("<div><p>first</p><p>second</p></div>", "/div.1").is_err());
    }

    #[test]
    fn rejects_ambiguous_unsupported_and_out_of_range_locations() {
        let xml = "<p>Here</p><p>There</p><p hidden='hidden'>Secret</p>";
        for path in [
            "/p/text().0",
            "/p[1].1",
            "/p[3]/text().1",
            "/p[2]/text().88",
            "/p[0].0",
            "/p[01].0",
            "/p[2]/text()[4].0",
            "/p[2]/text()/span.0",
        ] {
            assert!(location(xml, path).is_err(), "{path}");
        }
        let mixed = "<p style='white-space:pre'>A  B</p>";
        assert!(location(mixed, "/p/text().3").is_err());
        assert!(
            location(
                &format!("<p>{}</p>", "a".repeat(TEXT_SPLIT_SIZE)),
                "/p/text().3"
            )
            .is_err()
        );
        assert!(location("<p><![CDATA[one]]></p>", "/p/text().1").is_err());
        assert!(location("<pre>hello</pre>", "/pre/text().2").is_err());
        assert!(
            Pointer::parse(&format!("/body/DocFragment[1]/body{}", "/div".repeat(65))).is_err()
        );
        let mut book = Book::open(&crate::test_fixture("field-notes-epub3.epub")).unwrap();
        for (path, version) in [
            ("/body/DocFragment/body/p[2].0", Some(20240114)),
            ("/body/DocFragment[99]/body/p[2].0", Some(20240114)),
            ("/body/DocFragment[1]/body/p[2].0", Some(20191231)),
            ("/body/DocFragment[1]/body/p[2].0", None),
        ] {
            assert!(resolve(&mut book, path, version).is_err());
        }
    }

    #[cfg(feature = "kindle")]
    #[test]
    fn two_supported_saved_locations_must_agree() {
        use crate::koreader_sidecar::SidecarProgress;
        let mut book = Book::open(&crate::test_fixture("field-notes-epub3.epub")).unwrap();
        let mut sidecar = SidecarProgress {
            last_xpointer: Some("/body/DocFragment[1]/body/p[9]/text().4".into()),
            last_epubcfi: Some("epubcfi(/6/2!/4/20[tide-b]/1:4)".into()),
            cre_dom_version: Some(20240114),
            ..Default::default()
        };
        let verified = sidecar.reflow_position(&mut book).unwrap();
        assert_eq!(verified.anchor.offset, 4);
        let source = Source::parse(&book.chapter(0).unwrap()).unwrap();
        assert_eq!(source.fragments["tide-b"].0, verified.anchor.block);
        assert_ne!(source.fragments["tide-a"].0, verified.anchor.block);
        sidecar.last_epubcfi = Some("epubcfi(/6/2!/4/20[tide-b]/1:6)".into());
        assert!(sidecar.reflow_position(&mut book).is_err());
        sidecar.last_epubcfi = Some("unsupported".into());
        assert_eq!(sidecar.reflow_position(&mut book).unwrap(), verified);
    }
}
