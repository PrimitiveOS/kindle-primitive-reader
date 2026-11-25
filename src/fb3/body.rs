use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use roxmltree::Node;

use super::{
    FB3_BODY_NS, child,
    section_plan::{self, Piece},
};
use crate::Result;
use crate::epub::{Book, Destination, TocEntry};
use crate::loose::{CHAPTER_PATH, check_output, xml_text};

const XLINK_NS: &str = "http://www.w3.org/1999/xlink";
pub(super) const MAX_DEPTH: usize = 48;
const MAX_TOC: usize = 512;
const MAX_ID: usize = 256;

pub(super) struct Converted {
    pub(super) chapters: Vec<(String, String)>,
    pub(super) toc: Vec<TocEntry>,
    pub(super) incomplete_note_ids: BTreeSet<String>,
}

pub(super) fn convert(
    root: Node<'_, '_>,
    title: &str,
    language: Option<&str>,
    cover: Option<&str>,
    images: &BTreeMap<String, String>,
    mut asset: impl FnMut(&str) -> Option<String>,
) -> Result<Converted> {
    // An output-budget retry starts conversion again. Keep each asset lookup
    // stable and avoid repeating the caller's side effects across attempts.
    let mut assets = BTreeMap::<String, Option<String>>::new();
    let mut load = |path: &str| {
        assets
            .entry(path.to_owned())
            .or_insert_with(|| asset(path))
            .clone()
    };
    let groups = section_plan::chapters(root, false)?;
    match convert_groups(groups, root, title, language, cover, images, &mut load) {
        Err(error)
            if error.to_string()
                == "The converted document exceeds the one MiB layout input limit" =>
        {
            let groups = section_plan::chapters(root, true)?;
            convert_groups(groups, root, title, language, cover, images, &mut load)
        }
        result => result,
    }
}

fn convert_groups<'a, 'i>(
    groups: Vec<Vec<Piece<'a, 'i>>>,
    root: Node<'a, 'i>,
    title: &str,
    language: Option<&str>,
    cover: Option<&str>,
    images: &BTreeMap<String, String>,
    asset: impl FnMut(&str) -> Option<String>,
) -> Result<Converted> {
    // Keep this out of converted XHTML: changing markup would invalidate saved
    // Source revisions for FB3s that were already split by an earlier build.
    let incomplete_note_ids = groups
        .iter()
        .flatten()
        .filter_map(|piece| match *piece {
            Piece::Open {
                node,
                continued: true,
                ..
            } if node.tag_name().name() == "notebody" && retained(node) => {
                identifier(node).map(str::to_owned)
            }
            _ => None,
        })
        .collect();
    let mut ids = BTreeMap::new();
    for (chapter, pieces) in groups.iter().enumerate() {
        let mut register = |node| {
            if let Some(id) = identifier(node) {
                ids.entry(id)
                    .and_modify(|entry| *entry = None)
                    .or_insert(Some(chapter));
            }
        };
        for piece in pieces {
            match *piece {
                Piece::Whole(node) => {
                    for child in node.descendants().filter(|child| {
                        child.is_element()
                            && child
                                .ancestors()
                                .take_while(|ancestor| *ancestor != root)
                                .all(retained)
                    }) {
                        register(child);
                    }
                }
                Piece::Open {
                    node,
                    continued: false,
                    ..
                } => register(node),
                Piece::Open { .. } | Piece::Close(_) => (),
            }
        }
    }
    let mut body = Body {
        xhtml: String::from(
            r#"<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops""#,
        ),
        toc: Vec::new(),
        chapter: 0,
        ids,
        images,
        asset,
        generated_id: 0,
    };
    if let Some(language) = language.filter(|value| value.len() <= 64) {
        body.attr("xml:lang", language)?;
    }
    body.xhtml.push_str("><head><title>");
    xml_text(&mut body.xhtml, title)?;
    body.xhtml.push_str("</title></head><body>");
    let header = std::mem::take(&mut body.xhtml);
    let mut chapters = Vec::new();
    for (index, pieces) in groups.into_iter().enumerate() {
        body.chapter = index;
        body.xhtml = header.clone();
        if index == 0
            && let Some(source) = cover
        {
            body.image(source, "Cover", None, true)?;
        }
        let (mut depth, mut section_depth) = (0, 0);
        for part in pieces {
            match part {
                Piece::Whole(node) => body.node(node, section_depth, depth)?,
                Piece::Open {
                    node,
                    tag,
                    continued,
                } => {
                    body.start(node, tag, section_depth, continued)?;
                    depth += 1;
                    section_depth += usize::from(tag == "section");
                }
                Piece::Close(tag) => {
                    write!(body.xhtml, "</{tag}>")?;
                    depth -= 1;
                    section_depth -= usize::from(tag == "section");
                }
            }
        }
        body.xhtml.push_str("</body></html>");
        check_output(&body.xhtml)?;
        chapters.push((chapter_path(index), std::mem::take(&mut body.xhtml)));
    }
    Ok(Converted {
        chapters,
        toc: body.toc,
        incomplete_note_ids,
    })
}

struct Body<'a, F> {
    xhtml: String,
    toc: Vec<TocEntry>,
    chapter: usize,
    ids: BTreeMap<&'a str, Option<usize>>,
    images: &'a BTreeMap<String, String>,
    asset: F,
    generated_id: usize,
}

fn chapter_path(index: usize) -> String {
    if index == 0 {
        CHAPTER_PATH.into()
    } else {
        format!("fb3-section-{}.xhtml", index + 1)
    }
}

pub(super) fn retained(node: Node<'_, '_>) -> bool {
    node.tag_name().namespace() == Some(FB3_BODY_NS)
        && !matches!(
            node.tag_name().name(),
            "script" | "style" | "object" | "iframe" | "audio" | "video"
        )
}

impl<F: FnMut(&str) -> Option<String>> Body<'_, F> {
    fn node(&mut self, node: Node<'_, '_>, section_depth: usize, depth: usize) -> Result<()> {
        if depth > MAX_DEPTH {
            return Err("The FB3 exceeds its section nesting limit".into());
        }
        if node.is_text() {
            return xml_text(&mut self.xhtml, node.text().unwrap_or_default());
        }
        if !retained(node) {
            return Ok(());
        }
        let name = node.tag_name().name();
        let id = identifier(node).filter(|id| self.ids.get(id) == Some(&Some(self.chapter)));
        let tag = if let Some(tag) = section_plan::wrappers(name) {
            tag
        } else {
            match name {
                "title" => "div",
                "p" if node
                    .parent()
                    .is_some_and(|parent| parent.has_tag_name((FB3_BODY_NS, "title"))) =>
                {
                    ["h1", "h2", "h3", "h4", "h5", "h6"][section_depth.min(5)]
                }
                "p" | "v" | "subscription" => "p",
                "subtitle" => "h3",
                "strong" | "em" | "sub" | "sup" | "code" | "pre" | "span" | "ol" | "ul" | "li"
                | "table" | "thead" | "tbody" | "tfoot" | "th" | "tr" | "td" => name,
                "underline" => "u",
                "strikethrough" => "s",
                "spacing" | "smallcaps" | "marker" => "span",
                "a" | "note" => "a",
                "img" => return self.element_image(node),
                "br" => {
                    self.xhtml.push_str("<br");
                    if let Some(id) = id {
                        self.attr("id", id)?;
                    }
                    self.xhtml.push_str(" />");
                    return check_output(&self.xhtml);
                }
                _ => {
                    if let Some(id) = id {
                        self.xhtml.push_str("<span");
                        self.attr("id", id)?;
                        self.xhtml.push_str("></span>");
                    }
                    for child in node.children() {
                        self.node(child, section_depth, depth + 1)?;
                    }
                    return Ok(());
                }
            }
        };
        self.start(node, tag, section_depth, false)?;
        let next_section = section_depth + usize::from(name == "section");
        for child in node.children() {
            self.node(child, next_section, depth + 1)?;
        }
        write!(self.xhtml, "</{tag}>")?;
        check_output(&self.xhtml)
    }

    fn start(
        &mut self,
        node: Node<'_, '_>,
        tag: &str,
        section_depth: usize,
        continued: bool,
    ) -> Result<()> {
        let name = node.tag_name().name();
        let id = identifier(node)
            .filter(|id| !continued && self.ids.get(id) == Some(&Some(self.chapter)));
        let mut generated = None;
        if !continued
            && name == "section"
            && self.toc.len() < MAX_TOC
            && let Some(label) = section_title(node)
        {
            if id.is_none() {
                generated = Some(self.next_id());
            }
            self.toc.push(TocEntry {
                label,
                depth: section_depth,
                destination: Some(Destination {
                    path: chapter_path(self.chapter),
                    fragment: id.or(generated.as_deref()).map(str::to_owned),
                }),
            });
        }
        write!(self.xhtml, "<{tag}")?;
        if let Some(id) = id.or(generated.as_deref()) {
            self.attr("id", id)?;
        }
        if let Some(language) = node
            .attribute(("http://www.w3.org/XML/1998/namespace", "lang"))
            .filter(|value| value.len() <= 64)
        {
            self.attr("xml:lang", language)?;
        }
        if name == "notebody" {
            self.attr("epub:type", "footnote")?;
        }
        if tag == "a" {
            let href = node
                .attribute((XLINK_NS, "href"))
                .or_else(|| node.attribute("href"));
            if let Some(link) = href.and_then(|href| self.link(href)) {
                self.attr("href", &link)?;
                if name == "note" && !link.contains(':') {
                    self.attr("epub:type", "noteref")?;
                }
            }
        }
        if matches!(tag, "td" | "th") {
            for attr in ["rowspan", "colspan"] {
                if let Some(value) = node
                    .attribute(attr)
                    .filter(|value| value.parse::<u16>().is_ok_and(|n| (1..=64).contains(&n)))
                {
                    self.attr(attr, value)?;
                }
            }
        }
        if tag == "ol"
            && let Some(start) = node
                .attribute("start")
                .filter(|value| value.parse::<i32>().is_ok())
        {
            self.attr("start", start)?;
        }
        let class = match name {
            "poem" => Some("fb2-poem"),
            "stanza" => Some("fb2-stanza"),
            "v" => Some("fb2-verse"),
            "epigraph" => Some("fb2-epigraph"),
            "annotation" => Some("fb2-annotation"),
            _ => None,
        };
        if let Some(class) = class {
            self.attr("class", class)?;
        }
        self.xhtml.push('>');
        Ok(())
    }

    fn next_id(&mut self) -> String {
        loop {
            self.generated_id += 1;
            let candidate = format!("primitiveos-fb3-section-{}", self.generated_id);
            if !self.ids.contains_key(candidate.as_str()) {
                return candidate;
            }
        }
    }

    fn attr(&mut self, name: &str, value: &str) -> Result<()> {
        write!(self.xhtml, " {name}=\"")?;
        xml_text(&mut self.xhtml, value)?;
        self.xhtml.push('"');
        Ok(())
    }

    fn link(&self, raw: &str) -> Option<String> {
        if raw.is_empty() || raw.len() > 2 * MAX_ID || raw.chars().any(char::is_control) {
            return None;
        }
        if raw.starts_with("https://") || raw.starts_with("http://") {
            return Some(raw.into());
        }
        let href = if raw.starts_with('#') {
            raw.to_owned()
        } else {
            format!("#{raw}")
        };
        let id = Book::resolve_link(CHAPTER_PATH, &href).ok()?.fragment?;
        let chapter = self.ids.get(id.as_str()).copied().flatten()?;
        if chapter == self.chapter {
            Some(href)
        } else {
            Some(format!("{}{href}", chapter_path(chapter)))
        }
    }

    fn element_image(&mut self, node: Node<'_, '_>) -> Result<()> {
        let alt = node
            .attribute("alt")
            .or_else(|| node.attribute("title"))
            .unwrap_or_default();
        let id = identifier(node).filter(|id| self.ids.get(id) == Some(&Some(self.chapter)));
        let block = !node.ancestors().skip(1).any(|parent| {
            parent.tag_name().namespace() == Some(FB3_BODY_NS)
                && matches!(parent.tag_name().name(), "p" | "v" | "subtitle" | "span")
        });
        if let Some(path) = node.attribute("src").and_then(|id| self.images.get(id))
            && let Some(source) = (self.asset)(path)
        {
            return self.image(&source, alt, id, block);
        }
        if !alt.trim().is_empty() || id.is_some() {
            let tag = if block { "p" } else { "span" };
            write!(self.xhtml, "<{tag}")?;
            if let Some(id) = id {
                self.attr("id", id)?;
            }
            self.xhtml.push('>');
            xml_text(&mut self.xhtml, alt)?;
            write!(self.xhtml, "</{tag}>")?;
        }
        check_output(&self.xhtml)
    }

    fn image(&mut self, source: &str, alt: &str, id: Option<&str>, block: bool) -> Result<()> {
        if block {
            self.xhtml.push_str("<figure>");
        }
        self.xhtml.push_str("<img");
        self.attr("src", source)?;
        self.attr("alt", alt)?;
        if let Some(id) = id {
            self.attr("id", id)?;
        }
        self.xhtml.push_str(" />");
        if block {
            self.xhtml.push_str("</figure>");
        }
        check_output(&self.xhtml)
    }
}

fn identifier<'a>(node: Node<'a, '_>) -> Option<&'a str> {
    node.attribute("id")
        .filter(|id| !id.is_empty() && id.len() <= MAX_ID && !id.chars().any(char::is_control))
}

fn section_title(node: Node<'_, '_>) -> Option<String> {
    let mut output = String::new();
    for word in child(node, FB3_BODY_NS, "title")?
        .descendants()
        .filter(|node| node.is_text())
        .filter_map(|node| node.text())
        .flat_map(str::split_whitespace)
    {
        if output.len() + word.len() + 1 > 512 {
            break;
        }
        if !output.is_empty() {
            output.push(' ');
        }
        output.push_str(word);
    }
    (!output.is_empty()).then_some(output)
}
