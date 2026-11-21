//! Bound KindleGen's sometimes book-length XHTML sections before text and image layout.
use crate::kf8::Kf8Chapter;
use crate::mobi::MobiError;
use roxmltree::{Document, Node, ParsingOptions};
use std::collections::BTreeMap;

const SECTION_BYTES: usize = 80 * 1024;
// Parley lays out and decodes the entire current section up front. Keep image-heavy
// chapters responsive on the Oasis while retaining natural boundaries in small chapters.
const SECTION_IMAGES: usize = 12;
const MAX_SECTIONS: usize = 2048;
const MAX_SECTION_BYTES: usize = 24 * 1024 * 1024;

type Result<T> = std::result::Result<T, MobiError>;

struct Cut<'a, 'input> {
    offset: usize,
    parent: Node<'a, 'input>,
}

fn parse(xml: &str) -> Result<Document<'_>> {
    Document::parse_with_options(
        xml,
        ParsingOptions {
            nodes_limit: 240_000,
            ..Default::default()
        },
    )
    .map_err(|_| MobiError::InvalidText)
}

fn path_for(base: &str, index: usize) -> String {
    if index == 0 {
        return base.into();
    }
    format!(
        "{}-part-{}.xhtml",
        base.trim_end_matches(".xhtml"),
        index + 1
    )
}

fn open_end(raw: &str, node: Node<'_, '_>) -> Result<usize> {
    let start = node.range().start;
    let mut quote = None;
    for (i, c) in raw.as_bytes()[node.range()].iter().copied().enumerate() {
        match (quote, c) {
            (Some(q), c) if q == c => quote = None,
            (None, b'\'' | b'"') => quote = Some(c),
            (None, b'>') => return Ok(start + i + 1),
            _ => {}
        }
    }
    Err(MobiError::InvalidText)
}

fn closing(raw: &str, node: Node<'_, '_>, output: &mut String) {
    let name = raw[node.range().start + 1..]
        .split([' ', '\r', '\n', '\t', '>', '/'])
        .next()
        .unwrap_or("");
    output.push_str("</");
    output.push_str(name);
    output.push('>');
}

fn reopening(raw: &str, node: Node<'_, '_>, output: &mut String) -> Result<()> {
    let mut start = node.range().start;
    let end = open_end(raw, node)?;
    // An anchor belongs to the part where it first appeared.
    for attribute in node.attributes().filter(|a| a.name() == "id") {
        output.push_str(&raw[start..attribute.range().start]);
        start = attribute.range().end;
    }
    output.push_str(&raw[start..end]);
    Ok(())
}

fn split(chapter: Kf8Chapter) -> Result<Vec<Kf8Chapter>> {
    if chapter.xhtml.len() <= SECTION_BYTES
        && chapter
            .xhtml
            .match_indices("<img")
            .take(SECTION_IMAGES + 1)
            .count()
            <= SECTION_IMAGES
    {
        return Ok(vec![chapter]);
    }
    let raw = &chapter.xhtml;
    let document = parse(raw)?;
    let body = document
        .root_element()
        .children()
        .find(|n| n.has_tag_name("body"))
        .ok_or(MobiError::InvalidText)?;
    let mut cuts = Vec::new();
    let mut last = open_end(raw, body)?;
    let mut images = 0;
    for node in body.descendants().skip(1) {
        let offset = node.range().start;
        let image = node.has_tag_name("img") || node.has_tag_name("svg");
        let block = node.is_element()
            && matches!(
                node.tag_name().name(),
                "p" | "div"
                    | "section"
                    | "li"
                    | "h1"
                    | "h2"
                    | "h3"
                    | "h4"
                    | "h5"
                    | "h6"
                    | "table"
                    | "br"
            );
        if (image && images >= SECTION_IMAGES)
            || ((image || block) && offset.saturating_sub(last) >= SECTION_BYTES)
        {
            cuts.push(Cut {
                offset,
                parent: node.parent_element().ok_or(MobiError::InvalidText)?,
            });
            last = offset;
            images = 0;
        }
        if image {
            images += 1;
        }
        if node.is_text() && !raw[node.range()].starts_with("<![CDATA[") {
            // A long preformatted passage or single paragraph can have no block boundaries.
            let end = node.range().end;
            let parent = node.parent_element().ok_or(MobiError::InvalidText)?;
            while end.saturating_sub(last) > 2 * SECTION_BYTES {
                let mut at = (last + SECTION_BYTES).max(offset);
                while !raw.is_char_boundary(at) {
                    at += 1;
                }
                let tail = &raw[at..end];
                if let Some(gap) = tail
                    .char_indices()
                    .take(64)
                    .find_map(|(n, c)| c.is_whitespace().then_some(n))
                {
                    at += gap;
                } else if tail
                    .as_bytes()
                    .iter()
                    .take(32)
                    .position(|&b| b == b';')
                    .is_some_and(|semi| !tail.as_bytes()[..semi].contains(&b'&'))
                {
                    at += tail.find(';').unwrap() + 1;
                }
                if at <= last || at >= end {
                    break;
                }
                cuts.push(Cut { offset: at, parent });
                images = 0;
                last = at;
            }
        }
    }
    if cuts.is_empty() {
        return Ok(vec![chapter]);
    }
    if cuts.len() >= MAX_SECTIONS {
        return Err(MobiError::TextTooLarge);
    }
    let mut result = Vec::new();
    let mut bytes = 0usize;
    for part in 0..=cuts.len() {
        let mut xhtml = String::new();
        let start = if part == 0 {
            0
        } else {
            let cut = &cuts[part - 1];
            xhtml.push_str(&raw[..body.range().start]);
            let chain: Vec<_> = cut
                .parent
                .ancestors()
                .take_while(|n| *n != body.parent().unwrap())
                .collect();
            for node in chain.into_iter().rev() {
                reopening(raw, node, &mut xhtml)?;
            }
            cut.offset
        };
        if let Some(end) = cuts.get(part) {
            xhtml.push_str(&raw[start..end.offset]);
            for node in end.parent.ancestors().filter(Node::is_element) {
                closing(raw, node, &mut xhtml);
            }
        } else {
            xhtml.push_str(&raw[start..]);
        }
        bytes += xhtml.len();
        if bytes > MAX_SECTION_BYTES {
            return Err(MobiError::TextTooLarge);
        }
        result.push(Kf8Chapter {
            continued: part > 0,
            path: path_for(&chapter.path, part),
            label: chapter.label.clone(),
            xhtml,
        });
    }
    Ok(result)
}

pub(crate) fn partition(chapters: Vec<Kf8Chapter>) -> Result<Vec<Kf8Chapter>> {
    let mut result = Vec::new();
    let mut originals = Vec::new();
    let mut moved = BTreeMap::<String, BTreeMap<String, String>>::new();
    let mut bytes = 0;
    for chapter in chapters {
        let original = chapter.path.clone();
        let parts = split(chapter)?;
        for (index, part) in parts.into_iter().enumerate() {
            if result.len() == MAX_SECTIONS || part.xhtml.len() > MAX_SECTION_BYTES - bytes {
                return Err(MobiError::TextTooLarge);
            }
            bytes += part.xhtml.len();
            if index > 0 {
                let targets = moved.entry(original.clone()).or_default();
                let document = parse(&part.xhtml)?;
                let body = document
                    .root_element()
                    .children()
                    .find(|n| n.has_tag_name("body"))
                    .ok_or(MobiError::InvalidText)?;
                for id in body.descendants().filter_map(|n| n.attribute("id")) {
                    targets
                        .entry(id.into())
                        .or_insert_with(|| part.path.clone());
                }
            }
            originals.push(original.clone());
            result.push(part);
        }
    }
    if moved.is_empty() {
        return Ok(result);
    }
    for (part, original) in result.iter_mut().zip(originals) {
        let document = parse(&part.xhtml)?;
        let mut changes = Vec::new();
        for href in document
            .descendants()
            .filter(|n| n.has_tag_name("a"))
            .filter_map(|n| n.attribute_node("href"))
        {
            let Some((reference, fragment)) = href.value().split_once('#') else {
                continue;
            };
            let target = if reference.is_empty() {
                original.as_str()
            } else {
                reference
            };
            let canonical = moved
                .get(target)
                .and_then(|fragments| fragments.get(fragment))
                .map_or(target, String::as_str);
            let prefix = if canonical == part.path {
                ""
            } else {
                canonical
            };
            if reference != prefix {
                if !(reference.is_empty() || originals_are_local(reference)) {
                    continue;
                }
                let range = href.range_value();
                // Keep the original spelling and XML escaping of the fragment.
                let Some(mark) = part.xhtml[range.clone()].find('#') else {
                    continue;
                };
                changes.push((range.start..range.start + mark, prefix.to_owned()));
            }
        }
        drop(document);
        for (range, text) in changes.into_iter().rev() {
            part.xhtml.replace_range(range, &text);
        }
    }
    Ok(result)
}

fn originals_are_local(path: &str) -> bool {
    !path.contains([':', '/', '?']) && path.ends_with(".xhtml")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_sections_keep_nested_markup_and_retarget_cross_section_links() {
        let paragraph = "<p><img src='assets/one.png'/></p>".repeat(30);
        let chapter = Kf8Chapter {
            continued: false,
            path: "chapter.xhtml".into(),
            label: "Album".into(),
            xhtml: format!(
                "<html><head><title>A</title></head><body><div id='start' dir='rtl'><a href='#end'>Last</a>{paragraph}<p id='end'>End <a href='#start'>Start</a></p></div></body></html>"
            ),
        };
        let parts = partition(vec![chapter]).unwrap();
        assert_eq!(parts.len(), 3);
        let mut count = 0;
        for (part, section) in parts.iter().enumerate() {
            let doc = parse(&section.xhtml).unwrap();
            let n = doc.descendants().filter(|n| n.has_tag_name("img")).count();
            assert!(n <= SECTION_IMAGES);
            count += n;
            assert!(doc.descendants().any(|n| n.attribute("dir") == Some("rtl")));
            assert_eq!(
                doc.descendants()
                    .any(|n| n.attribute("id") == Some("start")),
                part == 0
            );
        }
        assert_eq!(count, 30);
        assert!(parts[0].xhtml.contains("chapter-part-3.xhtml#end"));
        assert!(parts.last().unwrap().xhtml.contains("chapter.xhtml#start"));
    }

    #[test]
    fn long_multibyte_paragraphs_do_not_lose_text_or_break_entities() {
        let text = "Élan &amp; trà. ".repeat(26_000);
        let chapter = Kf8Chapter {
            continued: false,
            path: "document.xhtml".into(),
            label: "Text".into(),
            xhtml: format!("<html><body><p id='p'>{text}</p></body></html>"),
        };
        let parts = partition(vec![chapter]).unwrap();
        assert!(parts.len() > 2);
        let mut recovered = String::new();
        for part in parts {
            let doc = parse(&part.xhtml).unwrap();
            recovered.push_str(
                doc.descendants()
                    .find(|n| n.has_tag_name("p"))
                    .unwrap()
                    .text()
                    .unwrap(),
            );
        }
        assert_eq!(recovered, text.replace("&amp;", "&"));
    }
}
