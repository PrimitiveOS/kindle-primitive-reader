//! Resolve EPUB CFI points and simple range starts for supported XHTML.
//!
//! https://w3c.github.io/epub-specs/epub33/epubcfi/
//! https://github.com/koreader/crengine/blob/master/crengine/src/lvtinydom.cpp

use std::collections::HashSet;

use roxmltree::{Document, Node, ParsingOptions};

use crate::epub::Book;
use crate::source::Source;
use crate::{Anchor, Result, SideBias};

const MAX_CFI_BYTES: usize = 8192;
const MAX_CFI_STEPS: usize = 64;
const MAX_ID_BYTES: usize = 256;
const MAX_TEXT_ASSERTION_BYTES: usize = 4096;
const MAX_CFI_CORRECTION_TEXT_BYTES: usize = 128 * 1024;
const OPF: &str = "http://www.idpf.org/2007/opf";
const XHTML: &str = "http://www.w3.org/1999/xhtml";
const XML: &str = "http://www.w3.org/XML/1998/namespace";
const ID_ESCAPES: [char; 8] = ['^', '[', ']', '(', ')', ',', ';', '='];

pub use crate::epub::EpubPosition as CfiPosition;

struct Step {
    index: usize,
    id: Option<String>,
}

struct Point {
    spine: Step,
    item: Step,
    local: Vec<Step>,
    offset: Option<usize>,
    assertion: Option<TextAssertion>,
    side_bias: Option<SideBias>,
}

struct TextAssertion {
    before: String,
    after: String,
}

/// Resolve a point against the opened publication without guessing a different position.
pub fn resolve(book: &mut Book, value: &str) -> Result<CfiPosition> {
    if !book.is_reflowable_epub() {
        return Err("CFI import requires a reflowable EPUB".into());
    }
    let point = Point::parse(value)?;
    let chapter = {
        let package = book.package_xml()?;
        let opf = Document::parse_with_options(&package, xml_limits())?;
        authored_chapter(opf.root_element(), &point)?
    };
    let entry = book
        .chapters
        .get(chapter)
        .ok_or("The CFI chapter is unavailable")?;
    if book.recovered_placeholder(entry) || book.is_manifest_fallback(entry) {
        return Err("The CFI chapter is unavailable in this edition".into());
    }
    let path = entry.href.clone();
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
    let xml = crate::epub_xml::without_plain_doctype(&xhtml);
    let document = Document::parse_with_options(&xml, xml_limits())?;
    let mut target = authored_body(document.root_element(), &point.local[0])?;
    for step in &point.local[1..] {
        target = if step.index.is_multiple_of(2) {
            element_child(target, step)?
        } else {
            text_chunk(target, step.index)?
        };
    }
    if unsupported_target(target) {
        return Err("The CFI points into content that Reader may transform or omit".into());
    }
    let (block, offset) = if target.is_text() {
        let (target, text_offset) = if let Some(assertion) = &point.assertion {
            let original_offset = point.offset.unwrap_or_default();
            match validate_text_assertion(
                document.root_element(),
                target,
                original_offset,
                assertion,
            ) {
                Ok(()) => (target, original_offset),
                Err(_) => corrected_text_location(document.root_element(), assertion)?,
            }
        } else {
            (target, point.offset.unwrap_or_default())
        };
        if unsupported_target(target) {
            return Err(
                "The corrected CFI points into content that Reader may transform or omit".into(),
            );
        }
        exact_text_point(&source, target, text_offset)?
    } else {
        exact_element_start(&source, target)?
    };
    Ok(CfiPosition {
        chapter,
        anchor: Anchor {
            raw_caret: None,
            illustration: None,
            side_bias: point.side_bias,
            book: book.revision(),
            chapter: path,
            revision: source.revision,
            block,
            offset,
        },
    })
}

fn unsupported_target(target: Node<'_, '_>) -> bool {
    target.ancestors().any(|node| {
        node.is_element()
            && (node.tag_name().namespace() != Some(XHTML)
                || node.has_attribute("hidden")
                || matches!(
                    node.tag_name().name(),
                    "script" | "style" | "noscript" | "template" | "pre" | "ruby"
                ))
    })
}

fn authored_chapter(package: Node<'_, '_>, point: &Point) -> Result<usize> {
    let spine = element_child(package, &point.spine)?;
    if !package.has_tag_name((OPF, "package"))
        || (point.spine.index != 6 && point.spine.id.is_none())
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
    let itemrefs: Vec<_> = spine.children().filter(Node::is_element).collect();
    let ordinal = itemrefs
        .iter()
        .position(|item| *item == entry)
        .ok_or("The CFI item is not a direct child of the original spine")?;
    if entry
        .attribute("idref")
        .is_none_or(|id| !strict_xhtml.contains(id))
    {
        return Err("The CFI target is not an XHTML reading document".into());
    }
    Ok(ordinal)
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
        let (normalized, side_bias) = strip_side_bias(inner)?;
        let (spine, tail) = parse_step(&normalized)?;
        let (item, tail) = parse_step(tail)?;
        let mut tail = tail
            .strip_prefix('!')
            .ok_or("The CFI has no chapter indirection")?;
        if (spine.index != 6 && spine.id.is_none()) || !item.index.is_multiple_of(2) {
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
        let (offset, assertion) = if let Some(rest) = tail.strip_prefix(':') {
            let (offset, rest) = take_integer(rest)?;
            let (assertion, rest) = if rest.starts_with('[') {
                let (assertion, rest) = parse_text_assertion(rest)?;
                (Some(assertion), rest)
            } else {
                (None, rest)
            };
            if !reached_text || !rest.is_empty() {
                return Err(
                    "This CFI offset or assertion is outside Reader's supported subset".into(),
                );
            }
            (Some(offset), assertion)
        } else if let Some(range) = tail.strip_prefix(',') {
            if side_bias.is_some() {
                return Err("EPUB CFI side bias is not valid on a range".into());
            }
            let (start, end) = split_cfi_range(range)?;
            let (mut suffix, offset, assertion) = parse_relative_local_path(start, reached_text)?;
            // A range must begin at an element step; a text node directly under the parent is not a supported range start.
            if suffix.first().is_some_and(|step| !step.index.is_multiple_of(2)) {
                return Err("An EPUB CFI range must start at an element step".into());
            }
            let (end_suffix, _, _) = parse_relative_local_path(end, reached_text)?;
            if reached_text && (!suffix.is_empty() || !end_suffix.is_empty()) {
                return Err("An EPUB CFI range cannot continue below a text node".into());
            }
            if local.len().saturating_add(suffix.len()) > MAX_CFI_STEPS {
                return Err("The CFI contains too many XHTML steps".into());
            }
            local.append(&mut suffix);
            (offset, assertion)
        } else if tail.is_empty() {
            (None, None)
        } else {
            return Err("The EPUB CFI point or range is outside Reader's supported subset".into());
        };
        if local.first().is_none_or(|first| first.index != 4) {
            return Err("The CFI does not address an XHTML body".into());
        }
        if offset.is_some() && local.last().is_none_or(|step| step.index.is_multiple_of(2)) {
            return Err("The CFI character offset does not address a text node".into());
        }
        Ok(Self {
            spine,
            item,
            local,
            offset,
            assertion,
            side_bias,
        })
    }
}

fn strip_side_bias(value: &str) -> Result<(String, Option<SideBias>)> {
    let mut marker = None;
    for (offset, character) in value.char_indices() {
        if character != ';' || !value[offset..].starts_with(";s=") {
            continue;
        }
        let mut carets = 0;
        let mut previous = offset;
        while previous > 0 {
            let Some((at, '^')) = value[..previous].char_indices().next_back() else {
                break;
            };
            carets += 1;
            previous = at;
        }
        if carets % 2 == 0 {
            marker = Some(offset);
        }
    }
    let Some(marker) = marker else {
        return Ok((value.to_owned(), None));
    };
    let bias = match &value[marker + 3..] {
        "a]" => SideBias::After,
        "b]" => SideBias::Before,
        _ => return Err("The EPUB CFI has an invalid side-bias value".into()),
    };
    let prefix = &value[..marker];
    let mut assertion_open = false;
    let mut escaped = false;
    for character in prefix.chars() {
        if escaped {
            escaped = false;
        } else if character == '^' {
            escaped = true;
        } else if character == '[' {
            if assertion_open {
                return Err("The EPUB CFI side-bias assertion is malformed".into());
            }
            assertion_open = true;
        } else if character == ']' {
            if !assertion_open {
                return Err("The EPUB CFI side-bias assertion is malformed".into());
            }
            assertion_open = false;
        }
    }
    if !assertion_open {
        return Err("The EPUB CFI side bias is outside its final assertion".into());
    }
    let mut normalized = prefix.to_owned();
    if normalized.ends_with('[') {
        normalized.pop();
    } else {
        normalized.push(']');
    }
    Ok((normalized, Some(bias)))
}

fn parse_relative_local_path(
    value: &str,
    parent_is_text: bool,
) -> Result<(Vec<Step>, Option<usize>, Option<TextAssertion>)> {
    let mut tail = value;
    let mut local = Vec::new();
    let mut reached_text = false;
    while tail.starts_with('/') {
        if reached_text || local.len() == MAX_CFI_STEPS {
            return Err("The CFI range contains too many or invalid XHTML steps".into());
        }
        let (step, rest) = parse_step(tail)?;
        reached_text = !step.index.is_multiple_of(2);
        if reached_text && step.id.is_some() {
            return Err("CFI text steps cannot carry an element ID".into());
        }
        local.push(step);
        tail = rest;
    }
    let (offset, assertion) = if let Some(rest) = tail.strip_prefix(':') {
        let (offset, rest) = take_integer(rest)?;
        let (assertion, rest) = if rest.starts_with('[') {
            let (assertion, rest) = parse_text_assertion(rest)?;
            (Some(assertion), rest)
        } else {
            (None, rest)
        };
        if !rest.is_empty() || !reached_text && !parent_is_text {
            return Err("The CFI range offset is outside Reader's supported subset".into());
        }
        (Some(offset), assertion)
    } else if tail.is_empty() {
        (None, None)
    } else {
        return Err("The CFI range path is invalid".into());
    };
    Ok((local, offset, assertion))
}

fn split_cfi_range(value: &str) -> Result<(&str, &str)> {
    let mut delimiters = Vec::with_capacity(2);
    let mut assertion = false;
    let mut escaped = false;
    for (offset, character) in value.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match character {
            '^' => escaped = true,
            '[' if !assertion => assertion = true,
            ']' if assertion => assertion = false,
            '[' | ']' => return Err("The CFI range assertion is malformed".into()),
            ',' if !assertion => delimiters.push(offset),
            _ => {}
        }
    }
    // `value` is the text after the parent path's comma: `S,E)` with the closing parenthesis removed.
    // Exactly one top-level comma separates the start and end paths.
    if escaped || assertion || delimiters.len() != 1 {
        return Err("The EPUB CFI range must contain two local paths".into());
    }
    let start = &value[..delimiters[0]];
    let end = &value[delimiters[0] + 1..];
    Ok((start, end))
}

fn parse_text_assertion(value: &str) -> Result<(TextAssertion, &str)> {
    let body = value
        .strip_prefix('[')
        .ok_or("The CFI text assertion is invalid")?;
    let mut before = String::new();
    let mut after = String::new();
    let mut after_comma = false;
    let mut characters = body.char_indices();
    while let Some((offset, character)) = characters.next() {
        match character {
            ']' => {
                return Ok((TextAssertion { before, after }, &body[offset + 1..]));
            }
            '^' => {
                let (_, escaped) = characters
                    .next()
                    .filter(|(_, escaped)| ID_ESCAPES.contains(escaped))
                    .ok_or("The CFI text assertion has an invalid escape")?;
                if after_comma {
                    after.push(escaped);
                } else {
                    before.push(escaped);
                }
            }
            ',' if !after_comma => after_comma = true,
            ',' => return Err("A CFI comma inside a text assertion must be escaped".into()),
            ch if ID_ESCAPES.contains(&ch) || ch.is_control() && !cfi_whitespace(ch) => {
                return Err("The CFI text assertion has an unescaped character".into());
            }
            ch if after_comma => after.push(ch),
            ch => before.push(ch),
        }
        if before.len().saturating_add(after.len()) > MAX_TEXT_ASSERTION_BYTES {
            return Err("The CFI text assertion exceeds Reader's import limit".into());
        }
    }
    Err("The CFI text assertion was not closed".into())
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
        .nth(step.index / 2 - 1);
    if let Some(id) = step.id.as_deref() {
        if let Some(child) = child.filter(|child| unique_id_at(*child, id)) {
            return Ok(child);
        }
        // EPUB CFI correction uses an ID assertion to recover the intended
        // element when edits have shifted its structural ordinal. Require one
        // unique ID in the parsed document; never guess among duplicate IDs.
        let mut matches = parent
            .document()
            .descendants()
            .filter(|node| node.is_element() && node_id(*node) == Some(id));
        let corrected = matches
            .next()
            .ok_or("The CFI element ID is missing or ambiguous")?;
        if matches.next().is_some() {
            return Err("The CFI element ID is missing or ambiguous".into());
        }
        return Ok(corrected);
    }
    child.ok_or_else(|| "A CFI element step does not exist".into())
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

fn validate_text_assertion(
    root: Node<'_, '_>,
    target: Node<'_, '_>,
    utf16: usize,
    assertion: &TextAssertion,
) -> Result<()> {
    let text = target
        .text()
        .ok_or("The CFI assertion does not address text")?;
    let byte = utf16_boundary(text, utf16)
        .ok_or("The CFI assertion offset is not a valid text boundary")?;
    let mut stream = Vec::new();
    let mut point = None;
    let mut previous_space = false;
    for node in root.descendants().filter(Node::is_text) {
        let raw = node.text().unwrap_or_default();
        if node == target && byte == 0 {
            point = Some(stream.len());
        }
        let mut local_byte = 0usize;
        for character in raw.chars() {
            if node == target && local_byte == byte {
                point = Some(stream.len());
            }
            local_byte += character.len_utf8();
            if cfi_whitespace(character) {
                if !previous_space {
                    stream.push(' ');
                    previous_space = true;
                }
            } else {
                stream.push(character);
                previous_space = false;
            }
        }
        if node == target && local_byte == byte {
            point = Some(stream.len());
        }
    }
    let point = point.ok_or("The CFI assertion target is outside the XHTML text")?;
    let before = collapse_cfi_whitespace(&assertion.before);
    let after = collapse_cfi_whitespace(&assertion.after);
    if point < before.len()
        || stream.get(point - before.len()..point) != Some(before.as_slice())
        || stream.get(point..point.saturating_add(after.len())) != Some(after.as_slice())
    {
        return Err("The CFI text assertion does not match the publication".into());
    }
    Ok(())
}

fn corrected_text_location<'a, 'input>(
    root: Node<'a, 'input>,
    assertion: &TextAssertion,
) -> Result<(Node<'a, 'input>, usize)> {
    let before = collapse_cfi_whitespace(&assertion.before);
    let after = collapse_cfi_whitespace(&assertion.after);
    if before.is_empty() && after.is_empty() {
        return Err("The CFI text assertion has no context for correction".into());
    }

    let mut text = Vec::new();
    let mut ends = Vec::new();
    let mut first_start = None;
    let mut previous_space = false;
    let mut text_bytes = 0usize;
    for node in root.descendants().filter(Node::is_text) {
        let node_text = node.text().unwrap_or_default();
        text_bytes = text_bytes.saturating_add(node_text.len());
        if text_bytes > MAX_CFI_CORRECTION_TEXT_BYTES {
            return Err("The CFI correction text exceeds Reader's import limit".into());
        }
        let mut units = 0;
        for character in node_text.chars() {
            let start = CfiTextLocation { node, utf16: units };
            units += character.len_utf16();
            let end = CfiTextLocation { node, utf16: units };
            if cfi_whitespace(character) {
                if previous_space {
                    let last_end = ends
                        .last_mut()
                        .ok_or("The CFI text stream is inconsistent")?;
                    *last_end = end;
                } else {
                    text.push(' ');
                    if first_start.is_none() {
                        first_start = Some(start);
                    }
                    ends.push(end);
                }
                previous_space = true;
            } else {
                text.push(character);
                if first_start.is_none() {
                    first_start = Some(start);
                }
                ends.push(end);
                previous_space = false;
            }
        }
    }

    let mut matches = (0..=text.len()).filter(|&point| {
        point >= before.len()
            && point + after.len() <= text.len()
            && text.get(point - before.len()..point) == Some(before.as_slice())
            && text.get(point..point + after.len()) == Some(after.as_slice())
    });
    let point = matches
        .next()
        .ok_or("The CFI text assertion does not identify one unique target")?;
    if matches.next().is_some() {
        return Err("The CFI text assertion does not identify one unique target".into());
    }
    let location = if point == 0 {
        first_start
    } else {
        ends.get(point - 1).copied()
    }
    .ok_or("The CFI text assertion resolves to an empty document")?;
    Ok((location.node, location.utf16))
}

#[derive(Clone, Copy)]
struct CfiTextLocation<'a, 'input> {
    node: Node<'a, 'input>,
    utf16: usize,
}

fn cfi_whitespace(character: char) -> bool {
    matches!(character, ' ' | '\t' | '\r' | '\n')
}

fn collapse_cfi_whitespace(text: &str) -> Vec<char> {
    let mut collapsed = Vec::new();
    let mut previous_space = false;
    for character in text.chars() {
        if cfi_whitespace(character) {
            if !previous_space {
                collapsed.push(' ');
                previous_space = true;
            }
        } else {
            collapsed.push(character);
            previous_space = false;
        }
    }
    collapsed
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
        let biased = resolve(&mut book, "epubcfi(/6/2!/4/20[tide-b]/1:4[;s=b])").unwrap();
        assert_eq!(biased.anchor.side_bias, Some(SideBias::Before));
        // The ordinal /20 holds tide-b, so the id assertion fails and the CFI is recovered by the
        // unique id `tide-a` (EPUB CFI correction; see `corrects_shifted_element_ordinals_only_with_a_unique_id`).
        let corrected = resolve(&mut book, "epubcfi(/6/2!/4/20[tide-a]/1:4)").unwrap();
        assert_eq!(corrected.anchor.block, shore.fragments["tide-a"].0);
        assert_eq!(corrected.anchor.offset, 4);
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
    fn corrects_shifted_element_ordinals_only_with_a_unique_id() {
        let doc = Document::parse(
            "<root><inserted/><section id='target'/><section id='duplicate'/><section id='duplicate'/></root>",
        )
        .unwrap();
        let (target, _) = parse_step("/2[target]").unwrap();
        let recovered = element_child(doc.root_element(), &target).unwrap();
        assert_eq!(recovered.tag_name().name(), "section");
        assert_eq!(node_id(recovered), Some("target"));

        let (ambiguous, _) = parse_step("/2[duplicate]").unwrap();
        assert!(element_child(doc.root_element(), &ambiguous).is_err());
        let (unasserted, _) = parse_step("/2").unwrap();
        assert_eq!(
            element_child(doc.root_element(), &unasserted)
                .unwrap()
                .tag_name()
                .name(),
            "inserted"
        );
    }

    #[test]
    fn corrects_spine_item_ordinal_with_authored_ids() {
        let opf = "<package xmlns='http://www.idpf.org/2007/opf'><metadata/><manifest>\
            <item id='a' media-type='application/xhtml+xml'/><item id='b' media-type='application/xhtml+xml'/>\
            </manifest><guide/><spine id='spine'><itemref id='new' idref='a'/><itemref id='target' idref='b'/></spine></package>";
        let point = Point::parse("epubcfi(/8[spine]/2[target]!/4/2/1:0)").unwrap();
        let document = Document::parse(opf).unwrap();
        assert_eq!(
            authored_chapter(document.root_element(), &point).unwrap(),
            1
        );
    }

    #[test]
    fn parses_text_location_assertions_with_escaped_punctuation() {
        let point = Point::parse("epubcfi(/6/2!/4/2/1:3[xx^,,y])").unwrap();
        let assertion = point.assertion.unwrap();
        assert_eq!(assertion.before, "xx,");
        assert_eq!(assertion.after, "y");
        assert!(Point::parse("epubcfi(/6/2!/4/2/1:3[x,y,z])").is_err());
    }

    #[test]
    fn parses_side_bias_on_element_and_text_locations() {
        let before = Point::parse("epubcfi(/6/2!/4/2[;s=b])").unwrap();
        assert_eq!(before.side_bias, Some(SideBias::Before));

        let after = Point::parse("epubcfi(/6/2!/4/2/1:3[before,after;s=a])").unwrap();
        assert_eq!(after.side_bias, Some(SideBias::After));
        let assertion = after.assertion.unwrap();
        assert_eq!(assertion.before, "before");
        assert_eq!(assertion.after, "after");

        let escaped = Point::parse("epubcfi(/6/2!/4/2[literal^;s^=b])").unwrap();
        assert_eq!(escaped.side_bias, None);
        assert_eq!(escaped.local[1].id.as_deref(), Some("literal;s=b"));

        for value in [
            "epubcfi(/6/2!/4/2[;s=x])",
            "epubcfi(/6/2!/4/2,/1:1,/3:4[;s=b])",
            "epubcfi(/6/2!/4/2];s=b])",
        ] {
            assert!(Point::parse(value).is_err(), "accepted {value}");
        }
    }

    #[test]
    fn uses_the_start_of_a_well_formed_cfi_range() {
        let point = Point::parse(
            "epubcfi(/6/4[chapter]!/4[body]/10[paragraph],/2/1:1[near^,,target],/3:4)",
        )
        .unwrap();
        assert_eq!(
            point
                .local
                .iter()
                .map(|step| step.index)
                .collect::<Vec<_>>(),
            [4, 10, 2, 1]
        );
        assert_eq!(point.offset, Some(1));
        let assertion = point.assertion.unwrap();
        assert_eq!(assertion.before, "near,");
        assert_eq!(assertion.after, "target");

        assert!(Point::parse("epubcfi(/6/4!/4/2,/1:1,/3:4,/5:6)").is_err());
    }

    #[test]
    fn validates_text_assertions_across_inline_elements_and_collapsed_whitespace() {
        let xhtml = "<html xmlns='http://www.w3.org/1999/xhtml'><head/><body>\
            <p>xxx<em>yyy</em>\n  0123456789</p></body></html>";
        let document = Document::parse(xhtml).unwrap();
        let target = document
            .descendants()
            .find(|node| node.is_text() && node.text() == Some("yyy"))
            .unwrap();
        validate_text_assertion(
            document.root_element(),
            target,
            3,
            &TextAssertion {
                before: "yy".into(),
                after: "  01".into(),
            },
        )
        .unwrap();
        assert!(
            validate_text_assertion(
                document.root_element(),
                target,
                3,
                &TextAssertion {
                    before: "xx".into(),
                    after: "wrong".into(),
                },
            )
            .is_err()
        );
    }

    #[test]
    fn corrects_a_stale_text_offset_from_unique_collapsed_context() {
        let xhtml = "<html xmlns='http://www.w3.org/1999/xhtml'><head/><body>\
            <p>near <em>target</em> word</p></body></html>";
        let document = Document::parse(xhtml).unwrap();
        let assertion = TextAssertion {
            before: "near target".into(),
            after: " word".into(),
        };
        let (node, utf16) = corrected_text_location(document.root_element(), &assertion).unwrap();
        assert_eq!(node.text(), Some("target"));
        assert_eq!(utf16, 6);

        let source = Source::parse(xhtml).unwrap();
        assert_eq!(
            exact_text_point(&source, node, utf16).unwrap(),
            (source.blocks[0].key, 11)
        );
    }

    #[test]
    fn stale_text_context_must_identify_one_nonempty_target() {
        let xhtml = "<html xmlns='http://www.w3.org/1999/xhtml'><head/><body>\
            <p>near target word</p><p>near target word</p></body></html>";
        let document = Document::parse(xhtml).unwrap();
        let repeated = TextAssertion {
            before: "near target".into(),
            after: " word".into(),
        };
        assert!(corrected_text_location(document.root_element(), &repeated).is_err());
        assert!(
            corrected_text_location(
                document.root_element(),
                &TextAssertion {
                    before: String::new(),
                    after: String::new(),
                }
            )
            .is_err()
        );
    }

    #[test]
    fn maps_the_original_spine_ordinal_across_unsupported_items() {
        let opf = "<package xmlns='http://www.idpf.org/2007/opf'><metadata/><manifest>\
            <item id='first' media-type='text/html'/><item id='next' media-type='application/xhtml+xml'/>\
            </manifest><spine><itemref idref='first'/><itemref idref='next'/></spine></package>";
        let point = Point::parse("epubcfi(/6/4!/4/2/1:4)").unwrap();
        let document = Document::parse(opf).unwrap();
        assert_eq!(
            authored_chapter(document.root_element(), &point).unwrap(),
            1
        );

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
            "epubcfi(/6/2!/4/2,/1:2,/1:5)",
            "epubcfi(/6/2!/4/2~5.2)",
            "epubcfi(/6/2!/4/2@40:60)",
            "epubcfi(/6/2!/4/2!/4/2)",
            "epubcfi(/6/2!/4/2:4)",
            "epubcfi(/6/2!/4/2/1[wrong]:4)",
            "epubcfi(/6/2!/4/2[wrong^a]/1:4)",
            "epubcfi(/6/2!/4/2/1:0004)",
            "epubcfi(/6/2!/4/2/1:4[before,after]tail)",
        ] {
            assert!(Point::parse(value).is_err(), "accepted {value}");
        }
        let excess_depth = format!("epubcfi(/6/2!/4{})", "/2".repeat(MAX_CFI_STEPS));
        assert!(Point::parse(&excess_depth).is_err());
        assert!(Point::parse(&" ".repeat(MAX_CFI_BYTES + 1)).is_err());
    }
}
