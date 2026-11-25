//! Character-count pages are stable under font changes and restart at each spine item.
use std::collections::HashMap;

use roxmltree::{Document, Node, ParsingOptions};
use serde::{Deserialize, Serialize};

use crate::{Anchor, Result, epub::Book, source::Source};

pub(crate) const MAX_LABELS: usize = 32_768;
const MIN_STRIDE: u16 = 500;
const MAX_STRIDE: u16 = 3000;

/// A chapter boundary needs no text. A text point addresses the unchanged publisher source.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct TextPoint {
    pub block: usize,
    pub byte: usize,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct NumberedChapter {
    pub first: usize,
    pub revision: u64,
    pub points: Vec<Option<TextPoint>>,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct CharacterPages {
    pub book: u64,
    pub stride: u16,
    pub chapters: Vec<NumberedChapter>,
    pub count: usize,
}

impl CharacterPages {
    pub(crate) fn new(revision: u64, stride: u16) -> Result<Self> {
        if !(MIN_STRIDE..=MAX_STRIDE).contains(&stride) {
            return Err("Choose 500 to 3,000 characters per page".into());
        }
        Ok(Self {
            book: revision,
            stride,
            chapters: Vec::new(),
            count: 0,
        })
    }

    pub(crate) fn step(&mut self, book: &mut Book) -> Result<bool> {
        if self.book != book.revision() {
            return Err("The publication changed while numbering its pages".into());
        }
        let index = self.chapters.len();
        if index >= book.chapters.len() {
            return Ok(true);
        }
        let mut chapter =
            numbered_chapter(&book.chapter(index)?, self.stride, MAX_LABELS - self.count)?;
        chapter.first = self.count;
        self.count += chapter.points.len();
        self.chapters.push(chapter);
        Ok(self.chapters.len() == book.chapters.len())
    }

    pub(crate) fn locate(&self, row: usize) -> Option<(usize, Option<TextPoint>)> {
        let chapter = self
            .chapters
            .partition_point(|item| item.first <= row)
            .checked_sub(1)?;
        let item = &self.chapters[chapter];
        Some((chapter, *item.points.get(row - item.first)?))
    }

    pub(crate) fn cached(
        path: &std::path::Path,
        revision: u64,
        chapters: usize,
        stride: u16,
    ) -> Option<Self> {
        use std::io::Read;
        const LIMIT: u64 = 4 * 1024 * 1024;
        primitive_app::with_storage(|| {
            let file = std::fs::File::open(path)?;
            if file.metadata()?.len() > LIMIT {
                return Ok(None);
            }
            let mut bytes = Vec::new();
            file.take(LIMIT + 1).read_to_end(&mut bytes)?;
            if bytes.len() as u64 > LIMIT {
                return Ok(None);
            }
            let Ok(map) = serde_json::from_slice::<Self>(&bytes) else {
                return Ok(None);
            };
            let mut total = 0;
            let valid = map.book == revision
                && map.stride == stride
                && (MIN_STRIDE..=MAX_STRIDE).contains(&stride)
                && map.chapters.len() == chapters
                && map.count <= MAX_LABELS
                && map.chapters.iter().all(|c| {
                    let start = total;
                    total += c.points.len();
                    c.first == start
                        && matches!(c.points.first(), Some(None))
                        && c.points[1..].iter().all(Option::is_some)
                })
                && total == map.count;
            Ok(valid.then_some(map))
        })
        .ok()
        .flatten()
    }

    pub(crate) fn save(&self, path: &std::path::Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            primitive_app::with_storage(|| std::fs::create_dir_all(parent))?;
        }
        primitive_app::save(
            path,
            &serde_json::to_vec(self).map_err(std::io::Error::other)?,
        )
    }

    pub(crate) fn anchor(&self, chapter: usize, point: TextPoint, href: &str) -> Option<Anchor> {
        Some(Anchor {
            raw_caret: None,
            book: self.book,
            chapter: href.to_owned(),
            revision: self.chapters.get(chapter)?.revision,
            block: point.block,
            offset: point.byte,
            illustration: None,
        })
    }
}

fn block(name: &str) -> bool {
    matches!(
        name,
        "body"
            | "article"
            | "section"
            | "main"
            | "header"
            | "footer"
            | "aside"
            | "nav"
            | "div"
            | "p"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "ul"
            | "ol"
            | "li"
            | "blockquote"
            | "figure"
            | "figcaption"
            | "table"
            | "thead"
            | "tbody"
            | "tfoot"
            | "tr"
            | "td"
            | "th"
            | "dl"
            | "dt"
            | "dd"
            | "pre"
            | "hr"
    )
}

fn xml_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\r' | '\n')
}

fn excluded(n: Node<'_, '_>) -> bool {
    n.ancestors().any(|a| {
        matches!(
            a.tag_name().name(),
            "head" | "script" | "style" | "title" | "binary" | "annotation-xml"
        )
    })
}

fn retained(n: Node<'_, '_>) -> bool {
    let value = n.text().unwrap_or_default();
    if value.is_empty() || excluded(n) {
        return false;
    }
    if !value.chars().all(xml_space) || n.ancestors().any(|a| a.has_tag_name("pre")) {
        return true;
    }
    let Some(parent) = n.parent_element() else {
        return false;
    };
    if !block(parent.tag_name().name()) {
        return true;
    }
    // Reflow drops anonymous whitespace between block boxes; whitespace between inline
    // runs in the same paragraph can be a meaningful character. Standalone images in
    // the body form their own boxes.
    let is_block = |node: Node<'_, '_>| {
        block(node.tag_name().name()) || (parent.has_tag_name("body") && node.has_tag_name("img"))
    };
    n.prev_siblings()
        .skip(1)
        .find(Node::is_element)
        .is_some_and(|p| !is_block(p))
        && !n
            .next_siblings()
            .skip(1)
            .find(Node::is_element)
            .is_some_and(is_block)
}

struct Boundary {
    raw: usize,
    cdata: bool,
    #[cfg(test)]
    node: roxmltree::NodeId,
    #[cfg(test)]
    scalar: usize,
}

fn boundaries(xhtml: &str, body: Node<'_, '_>, stride: u16, limit: usize) -> Result<Vec<Boundary>> {
    if limit == 0 {
        return Err("The character page list has reached its size limit".into());
    }
    let mut points = Vec::new();
    let mut remaining = stride;
    for node in body
        .descendants()
        .filter(|node| node.is_text() && retained(*node))
    {
        let range = node.range();
        let raw = &xhtml[range.clone()];
        let pre = node.ancestors().any(|element| element.has_tag_name("pre"));
        let cdata = raw.starts_with("<![CDATA[");
        let mut at = if cdata { 9 } else { 0 };
        let end = if cdata {
            raw.len().saturating_sub(3)
        } else {
            raw.len()
        };
        let mut previous_space = false;
        #[cfg(test)]
        let mut scalar = 0;
        while at < end {
            let start = at;
            let mut c = raw[at..].chars().next().unwrap();
            at += c.len_utf8();
            // An XML reference contributes a single decoded scalar. A literal CRLF
            // likewise contributes one newline to the XML text node.
            if c == '&' && !cdata {
                let length = raw[at..end]
                    .find(';')
                    .ok_or("Invalid text reference in a numbered page")?;
                let name = &raw[at..at + length];
                if let Some(hex) = name.strip_prefix("#x") {
                    c = u32::from_str_radix(hex, 16)
                        .ok()
                        .and_then(char::from_u32)
                        .ok_or("Invalid XML scalar")?;
                } else if let Some(decimal) = name.strip_prefix('#') {
                    c = decimal
                        .parse::<u32>()
                        .ok()
                        .and_then(char::from_u32)
                        .ok_or("Invalid XML scalar")?;
                }
                at += length + 1;
            } else if c == '\r' && raw.as_bytes().get(at) == Some(&b'\n') {
                at += 1;
            }
            let space = xml_space(c) && !pre;
            if space && previous_space {
                continue;
            }
            previous_space = space;
            if remaining == 0 {
                if points.len() + 1 >= limit {
                    return Err("The character page list has reached its size limit".into());
                }
                points.push(Boundary {
                    raw: range.start + start,
                    cdata,
                    #[cfg(test)]
                    node: node.id(),
                    #[cfg(test)]
                    scalar,
                });
                remaining = stride;
            }
            remaining -= 1;
            #[cfg(test)]
            {
                scalar += 1;
            }
        }
    }
    Ok(points)
}

fn numbered_chapter(xhtml: &str, stride: u16, limit: usize) -> Result<NumberedChapter> {
    let xml = crate::epub_xml::without_plain_doctype(xhtml);
    let dom = Document::parse_with_options(
        &xml,
        ParsingOptions {
            nodes_limit: 100_000,
            ..Default::default()
        },
    )?;
    let body = dom
        .root_element()
        .children()
        .find(|n| n.has_tag_name("body"))
        .ok_or("The chapter has no reading body")?;
    let boundaries = boundaries(&xml, body, stride, limit)?;
    let source = Source::parse(xhtml)?;
    let mut points = vec![None];
    if !boundaries.is_empty() {
        let reserved = "reader-number-boundary-";
        let used: std::collections::HashSet<usize> = dom
            .descendants()
            .filter_map(|n| {
                n.attribute("id")?
                    .strip_prefix(reserved)?
                    .split_once('-')?
                    .0
                    .parse()
                    .ok()
            })
            .collect();
        let batch = (0..=used.len()).find(|n| !used.contains(n)).unwrap();
        let prefix = format!("{reserved}{batch}-");
        let mut marked = String::with_capacity(xhtml.len() + boundaries.len() * 48);
        let mut cursor = 0;
        for (i, b) in boundaries.iter().enumerate() {
            use std::fmt::Write as _;
            marked.push_str(&xhtml[cursor..b.raw]);
            if b.cdata {
                marked.push_str("]]>");
            }
            write!(marked, "<span id=\"{prefix}{i}\"/>")?;
            if b.cdata {
                marked.push_str("<![CDATA[");
            }
            cursor = b.raw;
        }
        marked.push_str(&xhtml[cursor..]);
        let located = Source::parse(&marked)?;
        if source.blocks.len() != located.blocks.len()
            || source
                .blocks
                .iter()
                .zip(&located.blocks)
                .any(|(a, b)| a.text != b.text)
        {
            return Err("A character boundary changed the chapter structure".into());
        }
        let originals: HashMap<_, _> = located
            .blocks
            .iter()
            .zip(&source.blocks)
            .map(|(a, b)| (a.key, b))
            .collect();
        for i in 0..boundaries.len() {
            let (marked_key, byte) = located
                .fragments
                .get(&format!("{prefix}{i}"))
                .copied()
                .ok_or("A numbered boundary has no readable source")?;
            let block = originals
                .get(&marked_key)
                .ok_or("Numbered source block unavailable")?;
            if !block.text.is_char_boundary(byte) {
                return Err("Invalid numbered text position".into());
            }
            points.push(Some(TextPoint {
                block: block.key,
                byte,
            }));
        }
    }
    Ok(NumberedChapter {
        first: 0,
        revision: source.revision,
        points,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unicode_and_mixed_inline_nodes_keep_original_positions_and_exact_ends() {
        let before = "é".repeat(499);
        let xhtml = format!("<html><body><p>{before}<em>&amp;</em><b>語tail</b></p></body></html>");
        let pages = numbered_chapter(&xhtml, 500, 20).unwrap();
        assert_eq!(pages.points.len(), 2);
        let point = pages.points[1].unwrap();
        let source = Source::parse(&xhtml).unwrap();
        let block = source.blocks.iter().find(|b| b.key == point.block).unwrap();
        assert_eq!(&block.text[point.byte..], "語tail");
        assert_eq!(pages.revision, source.revision);
        let exact = format!("<html><body><p>{before}α</p></body></html>");
        assert_eq!(
            numbered_chapter(&exact, 500, 20).unwrap().points,
            vec![None]
        );
        assert!(numbered_chapter(&xhtml, 500, 1).is_err());
    }

    #[test]
    fn numbered_cdata_keeps_literal_markup_and_uses_unique_marker_names() {
        let content = format!("{}<x>", "s".repeat(500));
        let xml = format!(
            "<html><body><p id=\"reader-number-boundary-0-0\"><![CDATA[{content}]]></p></body></html>"
        );
        let pages = numbered_chapter(&xml, 500, 10).unwrap();
        let p = pages.points[1].unwrap();
        let src = Source::parse(&xml).unwrap();
        assert_eq!(&src.blocks[0].text[p.byte..], "<x>");
    }

    #[test]
    fn numeric_xml_whitespace_counts_like_its_decoded_character() {
        let xhtml = format!(
            "<html><body><p>{} &#32;&#x20;z</p></body></html>",
            "a".repeat(499)
        );
        let source = Source::parse(&xhtml).unwrap();
        let pages = numbered_chapter(&xhtml, 500, 8).unwrap();
        let point = pages.points[1].unwrap();
        assert_eq!(&source.blocks[0].text[point.byte..], "z");
    }

    fn stock_path(node: Node<'_, '_>, chapter: usize, scalar: usize) -> String {
        let mut parts = Vec::new();
        for n in node.ancestors() {
            if n.has_tag_name("body") {
                break;
            }
            let parent = n.parent_element().unwrap();
            let matches: Vec<_> = parent
                .children()
                .filter(|c| {
                    if n.is_text() {
                        c.is_text() && retained(*c)
                    } else {
                        c.is_element() && c.tag_name() == n.tag_name()
                    }
                })
                .collect();
            let name = if n.is_text() {
                "text()"
            } else {
                n.tag_name().name()
            };
            parts.push(if matches.len() == 1 {
                name.to_owned()
            } else {
                format!(
                    "{name}[{}]",
                    1 + matches.iter().position(|c| *c == n).unwrap()
                )
            });
        }
        parts.reverse();
        format!(
            "/body/DocFragment[{}]/body/{}.{scalar}",
            chapter + 1,
            parts.join("/")
        )
    }

    #[test]
    #[ignore = "requires a Kindle original and its installed CREngine reference"]
    fn original_character_pages_match_stock_source_paths_and_offsets() {
        use std::path::PathBuf;
        let path = PathBuf::from(std::env::var_os("READER_SYNTHETIC_BOOK").unwrap());
        let expected: Vec<String> = serde_json::from_slice(
            &std::fs::read(std::env::var_os("READER_SYNTHETIC_REFERENCE").unwrap()).unwrap(),
        )
        .unwrap();
        let stride: u16 = std::env::var("READER_SYNTHETIC_STRIDE")
            .unwrap()
            .parse()
            .unwrap();
        let mut book = Book::open(&path).unwrap();
        let mut map = CharacterPages::new(book.revision(), stride).unwrap();
        let mut row = 0;
        for index in 0..book.chapters.len() {
            let xhtml = book.chapter(index).unwrap();
            let xml = crate::epub_xml::without_plain_doctype(&xhtml);
            let dom = Document::parse_with_options(
                &xml,
                ParsingOptions {
                    nodes_limit: 100_000,
                    ..Default::default()
                },
            )
            .unwrap();
            let body = dom
                .root_element()
                .children()
                .find(|n| n.has_tag_name("body"))
                .unwrap();
            assert_eq!(expected[row], format!("/body/DocFragment[{}].0", index + 1));
            row += 1;
            let bounds = boundaries(&xml, body, stride, MAX_LABELS).unwrap();
            for b in &bounds {
                assert_eq!(
                    stock_path(dom.get_node(b.node).unwrap(), index, b.scalar),
                    expected[row],
                    "chapter {} published character boundary {}",
                    index + 1,
                    row + 1
                );
                row += 1;
            }
            let done = map.step(&mut book).unwrap();
            assert_eq!(map.count, row);
            assert_eq!(done, index + 1 == book.chapters.len());
            let chap = &map.chapters[index];
            let original = Source::parse(&xhtml).unwrap();
            for loc in chap.points.iter().flatten() {
                let anchor = map.anchor(index, *loc, &book.chapters[index].href).unwrap();
                assert_eq!(anchor.revision, original.revision);
                let b = original
                    .blocks
                    .iter()
                    .find(|b| b.key == anchor.block)
                    .unwrap();
                assert!(b.text.is_char_boundary(anchor.offset));
            }
        }
        assert_eq!(expected.len(), row);
        println!("Verified {row} stock boundaries and source mappings at {stride} characters");
    }
}
