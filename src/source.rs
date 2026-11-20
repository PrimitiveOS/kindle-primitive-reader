use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use roxmltree::{Document, Node, ParsingOptions};

use crate::Result;

pub(crate) const SOURCE_ATTRIBUTE: &str = "data-primos-source";

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Emphasis {
    pub bold: bool,
    pub italic: bool,
}

pub(crate) struct Block {
    pub key: usize,
    pub text: String,
    pub emphasis: Vec<(Range<usize>, Emphasis)>,
    pub heading: bool,
    pub rtl: bool,
    pub images: Vec<String>,
    pub tag: String,
    pub classes: String,
    pub id: Option<String>,
    pub inline_style: Option<String>,
}

pub(crate) struct Source {
    pub revision: u64,
    pub blocks: Vec<Block>,
    pub fragments: BTreeMap<String, (usize, usize)>,
    pub stylesheets: Vec<String>,
    pub inline_sheets: Vec<String>,
    insertions: Vec<(usize, usize)>,
    head_end: usize,
}

impl Source {
    pub fn parse(xhtml: &str) -> Result<Self> {
        let options = || ParsingOptions {
            nodes_limit: 100_000,
            ..ParsingOptions::default()
        };
        let mut without_doctype;
        let html = match Document::parse_with_options(xhtml, options()) {
            Err(roxmltree::Error::DtdDetected) => {
                const PLAIN_DOCTYPE: &str = "<!DOCTYPE html>";
                let start = xhtml
                    .find(PLAIN_DOCTYPE)
                    .ok_or(roxmltree::Error::DtdDetected)?;
                without_doctype = xhtml.to_owned();
                without_doctype.replace_range(
                    start..start + PLAIN_DOCTYPE.len(),
                    &" ".repeat(PLAIN_DOCTYPE.len()),
                );
                Document::parse_with_options(&without_doctype, options())?
            }
            outcome => outcome?,
        };
        let root = html.root_element();
        if root.descendants().any(|node| {
            node.attributes()
                .any(|attr| attr.name().eq_ignore_ascii_case(SOURCE_ATTRIBUTE))
        }) {
            return Err("The chapter uses a reserved source attribute".into());
        }
        let head = root.children().find(|node| node.has_tag_name("head"));
        let mut stylesheets = Vec::new();
        let mut inline_sheets = Vec::new();
        if let Some(head) = head {
            for node in head.children() {
                if node.has_tag_name("style")
                    && let Some(css) = node.text()
                {
                    inline_sheets.push(css.to_owned());
                }
                if node.has_tag_name("link")
                    && node.attribute("rel").is_some_and(|rel| {
                        rel.split_ascii_whitespace()
                            .any(|name| name.eq_ignore_ascii_case("stylesheet"))
                    })
                    && let Some(href) = node.attribute("href")
                {
                    stylesheets.push(href.to_owned());
                }
            }
        }
        let body = root
            .children()
            .find(|node| node.has_tag_name("body"))
            .ok_or("The chapter has no body")?;
        if body
            .descendants()
            .any(|node| node.ancestors().take(65).count() == 65)
        {
            return Err("The chapter exceeds the nesting limit".into());
        }
        let mut blocks = Vec::new();
        let mut fragments = BTreeMap::new();
        let mut insertions = Vec::new();
        for node in body.descendants().filter(Node::is_element) {
            if !is_block(node)
                || hidden(node)
                || node
                    .descendants()
                    .skip(1)
                    .any(|child| is_block(child) && !hidden(child))
            {
                continue;
            }
            let key = node.range().start;
            let mut text = String::new();
            let mut emphasis: Vec<(Range<usize>, Emphasis)> = Vec::new();
            let mut spaces = false;
            let mut images = Vec::new();
            let mut ids = Vec::new();
            for descendant in node.descendants().filter(|d| !hidden(*d)) {
                if let Some(id) = descendant.attribute("id") {
                    ids.push((
                        id,
                        text.len()
                            + usize::from(spaces && !text.is_empty() && !text.ends_with('\n')),
                    ));
                }
                if descendant.has_tag_name("br") {
                    text.push('\n');
                    spaces = false;
                }
                if descendant.has_tag_name("img")
                    && let Some(src) = descendant.attribute("src")
                {
                    images.push(src.to_owned());
                }
                let Some(content) = descendant.text().filter(|_| descendant.is_text()) else {
                    continue;
                };
                let start = text.len();
                for c in content.chars() {
                    if matches!(c, ' ' | '\t' | '\r' | '\n' | '\u{c}') {
                        spaces = true;
                    } else {
                        if spaces && !text.is_empty() && !text.ends_with('\n') {
                            text.push(' ');
                        }
                        text.push(c);
                        spaces = false;
                    }
                }
                if start < text.len() {
                    let flags = Emphasis {
                        bold: descendant
                            .ancestors()
                            .any(|a| a.has_tag_name("strong") || a.has_tag_name("b")),
                        italic: descendant
                            .ancestors()
                            .any(|a| a.has_tag_name("em") || a.has_tag_name("i")),
                    };
                    emphasis.push((start..text.len(), flags));
                }
            }
            if text.is_empty() && images.is_empty() {
                continue;
            }
            for (id, offset) in ids {
                fragments
                    .entry(id.to_owned())
                    .or_insert((key, offset.min(text.len())));
            }
            if !text.is_empty() {
                for parent in node.ancestors().skip(1) {
                    if let Some(id) = parent.attribute("id") {
                        fragments.entry(id.to_owned()).or_insert((key, 0));
                    }
                }
            }
            let tag_end = xhtml[key..]
                .find(|c: char| c.is_ascii_whitespace() || matches!(c, '>' | '/'))
                .ok_or("The chapter has an incomplete tag")?;
            insertions.push((key + tag_end, key));
            blocks.push(Block {
                key,
                text,
                emphasis,
                images,
                heading: matches!(
                    node.tag_name().name(),
                    "h1" | "h2" | "h3" | "h4" | "h5" | "h6"
                ),
                rtl: node.ancestors().find_map(|a| a.attribute("dir")) == Some("rtl"),
                tag: node.tag_name().name().to_owned(),
                classes: node.attribute("class").unwrap_or_default().to_owned(),
                id: node.attribute("id").map(str::to_owned),
                inline_style: node.attribute("style").map(str::to_owned),
            });
        }
        if blocks.is_empty() {
            return Err("The chapter has no supported reading content".into());
        }
        let selected = blocks
            .iter()
            .map(|block| block.key)
            .collect::<BTreeSet<_>>();
        if body.descendants().any(|node| {
            let visible = node.has_tag_name("img")
                || (node.is_text() && node.text().is_some_and(|text| !text.trim().is_empty()));
            visible
                && !hidden(node)
                && !node
                    .ancestors()
                    .any(|parent| selected.contains(&parent.range().start))
        }) {
            return Err("The probe cannot paginate text between nested blocks".into());
        }
        let revision = crate::revision(xhtml);
        let head_end = head
            .and_then(|node| {
                xhtml[node.range()]
                    .rfind("</")
                    .map(|offset| node.range().start + offset)
            })
            .unwrap_or(body.range().start);
        Ok(Self {
            revision,
            blocks,
            fragments,
            stylesheets,
            inline_sheets,
            insertions,
            head_end,
        })
    }

    pub fn with_styles(&self, xhtml: &str, stylesheet: &str) -> String {
        let mut html = String::new();
        html.push_str(&xhtml[..self.head_end]);
        html.push_str("<style>");
        html.push_str(stylesheet);
        html.push_str("</style>");
        let mut cursor = self.head_end;
        for &(offset, key) in &self.insertions {
            use std::fmt::Write;
            html.push_str(&xhtml[cursor..offset]);
            write!(html, " {SOURCE_ATTRIBUTE}=\"{key}\"").unwrap();
            cursor = offset;
        }
        html.push_str(&xhtml[cursor..]);
        html
    }
}

fn is_block(node: Node<'_, '_>) -> bool {
    node.is_element()
        && matches!(
            node.tag_name().name(),
            "body"
                | "article"
                | "section"
                | "div"
                | "aside"
                | "p"
                | "li"
                | "dd"
                | "dt"
                | "h1"
                | "h2"
                | "h3"
                | "h4"
                | "h5"
                | "h6"
                | "blockquote"
                | "figure"
                | "figcaption"
                | "pre"
                | "td"
                | "th"
        )
}

fn hidden(node: Node<'_, '_>) -> bool {
    node.ancestors().any(|ancestor| {
        ancestor.is_element()
            && (matches!(
                ancestor.tag_name().name(),
                "script" | "style" | "noscript" | "template" | "svg"
            ) || ancestor.has_attribute("hidden"))
    })
}
