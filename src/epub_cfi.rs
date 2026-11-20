//! Resolve the EPUB CFI point subset emitted by KOReader for simple XHTML.
//!
//! https://w3c.github.io/epub-specs/epub33/epubcfi/
//! https://github.com/koreader/crengine/blob/master/crengine/src/lvtinydom.cpp

use std::collections::HashSet;

use roxmltree::{Document, Node, ParsingOptions};

use crate::epub::Book;
use crate::source::Source;
use crate::{Anchor, Result};

const MAX_CFI_BYTES: usize = 8192;
const MAX_CFI_STEPS: usize = 64;
const MAX_ID_BYTES: usize = 256;
const OPF: &str = "http://www.idpf.org/2007/opf";
const XHTML: &str = "http://www.w3.org/1999/xhtml";
const XML: &str = "http://www.w3.org/XML/1998/namespace";
const ID_ESCAPES: [char; 8] = ['^', '[', ']', '(', ')', ',', ';', '='];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CfiPosition {
    pub chapter: usize,
    pub anchor: Anchor,
}

struct Step {
    index: usize,
    id: Option<String>,
}

struct Point {
    spine: Step,
    item: Step,
    local: Vec<Step>,
    offset: Option<usize>,
}

/// Resolve a point against the opened publication without guessing a different position.
pub fn resolve(book: &mut Book, value: &str) -> Result<CfiPosition> {
    if book.is_fixed_layout() {
        return Err("CFI import requires a reflowable EPUB".into());
    }
    let point = Point::parse(value)?;
    let chapter = {
        let package = book.package_xml()?;
        let opf = Document::parse_with_options(&package, xml_limits())?;
        authored_chapter(opf.root_element(), &point)?
    };
    let path = book
        .chapters
        .get(chapter)
        .ok_or("The CFI chapter is unavailable")?
        .href
        .clone();
    if book
        .chapters
        .iter()
        .filter(|chapter| chapter.href == path)
        .take(2)
        .count()
        != 1
    {
        return Err("The CFI chapter occurs more than once in this spine".into());
    }
    let xhtml = book.chapter(chapter)?;
    let source = Source::parse(&xhtml)?;
    let mut without_doctype;
    let document = match Document::parse_with_options(&xhtml, xml_limits()) {
        Err(roxmltree::Error::DtdDetected) => {
            const PLAIN_DOCTYPE: &str = "<!DOCTYPE html>";
            let start = xhtml
                .find(PLAIN_DOCTYPE)
                .ok_or(roxmltree::Error::DtdDetected)?;
            without_doctype = xhtml.clone();
            without_doctype.replace_range(
                start..start + PLAIN_DOCTYPE.len(),
                &" ".repeat(PLAIN_DOCTYPE.len()),
            );
            Document::parse_with_options(&without_doctype, xml_limits())?
        }
        outcome => outcome?,
    };
    let mut target = authored_body(document.root_element(), &point.local[0])?;
    for step in &point.local[1..] {
        target = if step.index.is_multiple_of(2) {
            element_child(target, step)?
        } else {
            text_chunk(target, step.index)?
        };
    }
    if target.ancestors().any(|node| {
        node.is_element()
            && (node.tag_name().namespace() != Some(XHTML)
                || node.has_attribute("hidden")
                || matches!(
                    node.tag_name().name(),
                    "script" | "style" | "noscript" | "template" | "pre" | "ruby"
                ))
    }) {
        return Err("The CFI points into content that Reader may transform or omit".into());
    }
    let (block, offset) = if target.is_text() {
        exact_text_point(&source, target, point.offset.unwrap_or_default())?
    } else {
        exact_element_start(&source, target)?
    };
    Ok(CfiPosition {
        chapter,
        anchor: Anchor {
            book: book.revision(),
            chapter: path,
            revision: source.revision,
            block,
            offset,
        },
    })
}

fn authored_chapter(package: Node<'_, '_>, point: &Point) -> Result<usize> {
    let spine = element_child(package, &point.spine)?;
    if !package.has_tag_name((OPF, "package"))
        || !spine.has_tag_name((OPF, "spine"))
        || package.children().find(|node| node.has_tag_name("spine")) != Some(spine)
        || spine
            .children()
            .any(|child| child.is_element() && !child.has_tag_name((OPF, "itemref")))
    {
        return Err("The CFI does not reference the EPUB's original spine".into());
    }
    let entry = element_child(spine, &point.item)?;
    let manifest = package
        .children()
        .find(|node| node.has_tag_name("manifest"))
        .filter(|node| node.has_tag_name((OPF, "manifest")))
        .ok_or("The CFI publication has no standard EPUB manifest")?;
    let strict_xhtml: HashSet<_> = manifest
        .children()
        .filter(|node| {
            node.has_tag_name((OPF, "item"))
                && node.attribute("media-type") == Some("application/xhtml+xml")
        })
        .filter_map(|node| node.attribute("id"))
        .collect();
    // Older CREngine DOMs can omit non-XHTML items from the ordinal used for the CFI.
    if spine
        .children()
        .filter(Node::is_element)
        .take_while(|item| *item != entry)
        .chain([entry])
        .any(|item| {
            item.attribute("idref")
                .is_none_or(|id| !strict_xhtml.contains(id))
        })
    {
        return Err("The CFI spine ordinal may differ because it includes non-XHTML items".into());
    }
    Ok(point.item.index / 2 - 1)
}

fn authored_body<'a, 'input>(root: Node<'a, 'input>, step: &Step) -> Result<Node<'a, 'input>> {
    let body = element_child(root, step)?;
    let mut children = root.children().filter(Node::is_element);
    if !root.has_tag_name((XHTML, "html"))
        || !children
            .next()
            .is_some_and(|node| node.has_tag_name((XHTML, "head")))
        || !body.has_tag_name((XHTML, "body"))
        || children.next() != Some(body)
        || children.next().is_some()
    {
        return Err("The CFI does not reference an unambiguous XHTML reading body".into());
    }
    Ok(body)
}

impl Point {
    fn parse(value: &str) -> Result<Self> {
        if value.len() > MAX_CFI_BYTES {
            return Err("The EPUB CFI exceeds Reader's import limit".into());
        }
        let value = value.trim();
        let inner = value
            .strip_prefix("epubcfi(")
            .and_then(|value| value.strip_suffix(')'))
            .ok_or("Expected an EPUB CFI point")?;
        let (spine, tail) = parse_step(inner)?;
        let (item, tail) = parse_step(tail)?;
        let mut tail = tail
            .strip_prefix('!')
            .ok_or("The CFI has no chapter indirection")?;
        if spine.index != 6 || !item.index.is_multiple_of(2) {
            return Err("The CFI package path is outside Reader's supported subset".into());
        }
        let mut local = Vec::new();
        let mut reached_text = false;
        while tail.starts_with('/') {
            if reached_text || local.len() == MAX_CFI_STEPS {
                return Err("The CFI contains too many or invalid XHTML steps".into());
            }
            let (step, rest) = parse_step(tail)?;
            reached_text = !step.index.is_multiple_of(2);
            if reached_text && step.id.is_some() {
                return Err("CFI text steps cannot carry an element ID".into());
            }
            local.push(step);
            tail = rest;
        }
        if local.first().is_none_or(|first| first.index != 4) {
            return Err("The CFI does not address an XHTML body".into());
        }
        let offset = if let Some(rest) = tail.strip_prefix(':') {
            let (offset, rest) = take_integer(rest)?;
            if !reached_text || !rest.is_empty() {
                return Err(
                    "This CFI offset or assertion is outside Reader's supported subset".into(),
                );
            }
            Some(offset)
        } else if tail.is_empty() {
            None
        } else {
            return Err("Reader can import a single EPUB CFI point".into());
        };
        Ok(Self {
            spine,
            item,
            local,
            offset,
        })
    }
}

fn parse_step(value: &str) -> Result<(Step, &str)> {
    let value = value.strip_prefix('/').ok_or("The CFI step is invalid")?;
    let (index, tail) = take_integer(value)?;
    if index == 0 {
        return Err("The CFI step does not refer to a document node".into());
    }
    let (id, tail) = match tail.strip_prefix('[') {
        Some(assertion) => {
            let (id, rest) = take_assertion(assertion)?;
            (Some(id), rest)
        }
        None => (None, tail),
    };
    Ok((Step { index, id }, tail))
}

fn take_integer(value: &str) -> Result<(usize, &str)> {
    let length = value.bytes().take_while(u8::is_ascii_digit).count();
    if length == 0 || length > 1 && value.starts_with('0') {
        return Err("The CFI has an invalid integer".into());
    }
    let (number, tail) = value.split_at(length);
    Ok((number.parse()?, tail))
}

fn take_assertion(value: &str) -> Result<(String, &str)> {
    let mut characters = value.char_indices();
    let mut id = String::new();
    while let Some((offset, character)) = characters.next() {
        match character {
            ']' if !id.is_empty() => return Ok((id, &value[offset + 1..])),
            '^' => {
                let (_, escaped) = characters
                    .next()
                    .filter(|(_, escaped)| ID_ESCAPES.contains(escaped))
                    .ok_or("The CFI has an invalid assertion escape")?;
                id.push(escaped);
            }
            ch if !ID_ESCAPES.contains(&ch) && !ch.is_control() && !ch.is_whitespace() => {
                id.push(ch)
            }
            _ => return Err("The CFI does not contain a simple element ID assertion".into()),
        }
        if id.len() > MAX_ID_BYTES {
            return Err("A CFI element ID exceeds Reader's import limit".into());
        }
    }
    Err("The CFI element ID was not closed".into())
}

fn element_child<'a, 'input>(parent: Node<'a, 'input>, step: &Step) -> Result<Node<'a, 'input>> {
    if !step.index.is_multiple_of(2) {
        return Err("The CFI expected an element step".into());
    }
    let child = parent
        .children()
        .filter(Node::is_element)
        .nth(step.index / 2 - 1)
        .ok_or("A CFI element step does not exist")?;
    if let Some(id) = step.id.as_deref()
        && !unique_id_at(child, id)
    {
        return Err("The CFI element ID is missing, changed, or ambiguous".into());
    }
    Ok(child)
}

fn node_id<'a>(node: Node<'a, '_>) -> Option<&'a str> {
    node.attribute("id").or_else(|| node.attribute((XML, "id")))
}

fn unique_id_at(node: Node<'_, '_>, id: &str) -> bool {
    node_id(node) == Some(id)
        && node
            .document()
            .descendants()
            .filter(|child| node_id(*child) == Some(id))
            .take(2)
            .count()
            == 1
}

fn text_chunk<'a, 'input>(parent: Node<'a, 'input>, step: usize) -> Result<Node<'a, 'input>> {
    // CFI odd indices count gaps between elements. Comments and processing instructions have no step.
    let gap = (step - 1) / 2;
    let mut seen = 0;
    let mut text = None;
    for child in parent.children() {
        if child.is_element() {
            if seen == gap {
                break;
            }
            seen += 1;
        } else if child.is_text() && seen == gap && text.replace(child).is_some() {
            return Err("The CFI text chunk contains an ambiguous XML boundary".into());
        }
    }
    text.ok_or_else(|| "The CFI text chunk is unavailable".into())
}

fn exact_text_point(source: &Source, target: Node<'_, '_>, utf16: usize) -> Result<(usize, usize)> {
    let text = target.text().ok_or("The CFI does not address text")?;
    if text.is_empty()
        || text.trim() != text
        || text.contains("  ")
        || text
            .chars()
            .any(|ch| ch.is_control() || ch == '\u{ad}' || ch != ' ' && ch.is_whitespace())
    {
        return Err("The CFI text may have different whitespace in KOReader".into());
    }
    let byte_offset =
        utf16_boundary(text, utf16).ok_or("The CFI offset is not a valid text boundary")?;
    let (element, block) = target
        .ancestors()
        .skip(1)
        .find_map(|node| {
            source
                .blocks
                .iter()
                .find(|block| block.key == node.range().start)
                .map(|block| (node, block))
        })
        .ok_or("The CFI text has no Reader block")?;
    if element
        .descendants()
        .any(|node| node.has_tag_name("ruby") || node.has_tag_name("br"))
    {
        return Err("The CFI block contains unsupported text transformations".into());
    }
    let mut offset = None;
    let mut at = 0;
    for node in element.descendants().filter(Node::is_text) {
        let raw = node.text().unwrap_or_default();
        if block.text.get(at..at + raw.len()) != Some(raw) {
            return Err("Reader and KOReader may represent this CFI block differently".into());
        }
        if node == target {
            offset = Some(at + byte_offset);
        }
        at += raw.len();
    }
    if at != block.text.len() {
        return Err("The CFI block has no exact Reader text match".into());
    }
    Ok((
        block.key,
        offset.ok_or("The CFI text boundary is outside the Reader block")?,
    ))
}

fn exact_element_start(source: &Source, element: Node<'_, '_>) -> Result<(usize, usize)> {
    let range = element.range();
    if let Some(block) = source
        .blocks
        .iter()
        .find(|block| range.contains(&block.key))
    {
        if block.text.is_empty() || !block.images.is_empty() {
            return Err("The CFI element starts at a non-text location".into());
        }
        return Ok((block.key, 0));
    }
    if let Some(id) = node_id(element)
        && unique_id_at(element, id)
        && let Some(&(key, offset)) = source.fragments.get(id)
        && source.blocks.iter().any(|block| {
            block.key == key && !block.text.is_empty() && block.text.is_char_boundary(offset)
        })
    {
        return Ok((key, offset));
    }
    Err("The CFI element has no exact Reader text position".into())
}

fn utf16_boundary(text: &str, requested: usize) -> Option<usize> {
    let mut units = 0;
    for (byte, character) in text.char_indices() {
        if units == requested {
            return Some(byte);
        }
        units += character.len_utf16();
        if units > requested {
            return None;
        }
    }
    (units == requested).then_some(text.len())
}

fn xml_limits<'input>() -> ParsingOptions<'input> {
    ParsingOptions {
        nodes_limit: 100_000,
        ..ParsingOptions::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_the_authored_spine_and_repeated_text_exactly() {
        let mut book = Book::open(&crate::test_fixture("field-notes-epub3.epub")).unwrap();
        let shore = Source::parse(&book.chapter(0).unwrap()).unwrap();
        let result = resolve(&mut book, "epubcfi(/6/2!/4/20[tide-b]/1:4)").unwrap();
        assert_eq!(result.chapter, 0);
        assert_eq!(result.anchor.chapter, "EPUB/chapters/shore.xhtml");
        assert_eq!(result.anchor.revision, shore.revision);
        assert_eq!(result.anchor.book, book.revision());
        assert_eq!(result.anchor.block, shore.fragments["tide-b"].0);
        assert_ne!(result.anchor.block, shore.fragments["tide-a"].0);
        assert_eq!(result.anchor.offset, 4);
        assert!(resolve(&mut book, "epubcfi(/6/2!/4/20[tide-a]/1:4)").is_err());
        assert!(resolve(&mut book, "epubcfi(/6/98!/4/2)").is_err());

        let dusk = Source::parse(&book.chapter(1).unwrap()).unwrap();
        let result = resolve(&mut book, "epubcfi(/6/4!/4/4[door])").unwrap();
        assert_eq!(result.chapter, 1);
        assert_eq!(result.anchor.chapter, "EPUB/chapters/dusk.xhtml");
        assert_eq!(
            (result.anchor.block, result.anchor.offset),
            dusk.fragments["door"]
        );
    }

    #[test]
    fn maps_utf16_offsets_to_utf8_and_checks_text_boundaries() {
        let xhtml = "<html xmlns='http://www.w3.org/1999/xhtml'><head/><body><p>Foo <em id='a'>😀é</em> end</p><p>foo  bar</p></body></html>";
        let source = Source::parse(xhtml).unwrap();
        let html = Document::parse_with_options(xhtml, xml_limits()).unwrap();
        let em = html
            .descendants()
            .find(|node| node.has_tag_name("em"))
            .unwrap();
        let text = text_chunk(em, 1).unwrap();
        assert_eq!(
            exact_text_point(&source, text, 2).unwrap(),
            (source.blocks[0].key, 8)
        );
        assert!(exact_text_point(&source, text, 1).is_err());
        assert!(exact_text_point(&source, text, 4).is_err());
        let paragraph = em.parent().unwrap();
        let trailing = text_chunk(paragraph, 3).unwrap();
        assert_eq!(trailing.text(), Some(" end"));
        assert!(exact_text_point(&source, trailing, 1).is_err());
        assert!(
            exact_text_point(
                &source,
                text_chunk(paragraph.next_sibling_element().unwrap(), 1).unwrap(),
                3
            )
            .is_err()
        );
    }

    #[test]
    fn validates_escape_assertions_and_rejects_duplicate_ids() {
        let doc =
            Document::parse("<body><p id='left]^'/><p id='same'/><p id='same'/></body>").unwrap();
        let (step, remainder) = parse_step("/2[left^]^^]/1").unwrap();
        assert_eq!(remainder, "/1");
        assert!(element_child(doc.root_element(), &step).is_ok());
        let (duplicate, _) = parse_step("/4[same]").unwrap();
        assert!(element_child(doc.root_element(), &duplicate).is_err());
    }

    #[test]
    fn declines_spine_and_body_ordinals_that_can_drift() {
        let opf = "<package xmlns='http://www.idpf.org/2007/opf'><metadata/><manifest>\
            <item id='first' media-type='text/html'/><item id='next' media-type='application/xhtml+xml'/>\
            </manifest><spine><itemref idref='first'/><itemref idref='next'/></spine></package>";
        let point = Point::parse("epubcfi(/6/4!/4/2/1:4)").unwrap();
        let document = Document::parse(opf).unwrap();
        assert!(authored_chapter(document.root_element(), &point).is_err());

        for html in [
            "<html xmlns='http://www.w3.org/1999/xhtml'><head/><body/><body/></html>",
            "<html xmlns='http://www.w3.org/1999/xhtml'><extra/><body/></html>",
            "<html xmlns='http://www.w3.org/1999/xhtml'><body/><head/></html>",
        ] {
            let document = Document::parse(html).unwrap();
            assert!(authored_body(document.root_element(), &point.local[0]).is_err());
        }
    }

    #[test]
    fn declines_an_element_start_that_would_skip_an_illustration() {
        let xhtml = "<html xmlns='http://www.w3.org/1999/xhtml'><head/><body>\
            <p><img src='cover.png'/></p><p>First text</p></body></html>";
        let source = Source::parse(xhtml).unwrap();
        let document = Document::parse(xhtml).unwrap();
        let body = document.root_element().last_element_child().unwrap();
        assert!(exact_element_start(&source, body).is_err());
        let paragraph = body.last_element_child().unwrap();
        assert_eq!(
            exact_element_start(&source, paragraph).unwrap(),
            (source.blocks[1].key, 0)
        );
    }

    #[test]
    fn bounds_positions_and_declines_unsupported_location_types() {
        for value in [
            "/body/DocFragment[1]/body/p[4].17",
            "epubcfi(/6/2!/4/2/1:4[before,after])",
            "epubcfi(/6/2!/4/2,/1:2,/1:5)",
            "epubcfi(/6/2!/4/2~5.2)",
            "epubcfi(/6/2!/4/2@40:60)",
            "epubcfi(/6/2!/4/2!/4/2)",
            "epubcfi(/6/2!/4/2:4)",
            "epubcfi(/6/2!/4/2/1[wrong]:4)",
            "epubcfi(/6/2!/4/2[wrong^a]/1:4)",
            "epubcfi(/6/2!/4/2/1:0004)",
        ] {
            assert!(Point::parse(value).is_err(), "accepted {value}");
        }
        let excess_depth = format!("epubcfi(/6/2!/4{})", "/2".repeat(MAX_CFI_STEPS));
        assert!(Point::parse(&excess_depth).is_err());
        assert!(Point::parse(&" ".repeat(MAX_CFI_BYTES + 1)).is_err());
    }
}
