use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::ops::Range;

use parley::{BaseDirection, Language};
use roxmltree::{Document, Node, NodeId, ParsingOptions};

use crate::Result;

pub(crate) const SOURCE_ATTRIBUTE: &str = "data-primos-source";
const RUBY_TEXT_MODEL: u64 = 0xe026_92a4_3415_b12f;
const MAX_RUBY_GROUPS: usize = 1_024;
const MAX_RUBY_ANNOTATION_BYTES: usize = 192;
const MAX_LANGUAGE_BYTES: usize = 128;
const XML_NAMESPACE: &str = "http://www.w3.org/XML/1998/namespace";

#[derive(Clone, Copy, Default, PartialEq, Eq)]
#[cfg(test)]
pub(crate) struct Emphasis {
    pub bold: bool,
    pub italic: bool,
}

pub(crate) struct Block {
    pub key: usize,
    pub text: String,
    pub element: usize,
    pub styled_text: Vec<StyledText>,
    #[cfg(test)]
    pub emphasis: Vec<(Range<usize>, Emphasis)>,
    pub links: Vec<InlineLink>,
    pub direction: BaseDirection,
    pub language: Option<Language>,
    pub images: Vec<EmbeddedImage>,
    pub ruby: Vec<Ruby>,
    legacy: Option<LegacyOffsets>,
}

pub(crate) struct EmbeddedImage {
    pub href: String,
    pub inline_svg: Option<String>,
    pub offset: usize,
    pub space_after: bool,
    pub element: usize,
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub link: Option<String>,
    pub label: String,
}

pub(crate) struct Ruby {
    pub bytes: Range<usize>,
    pub annotation: String,
    pub language: Option<Language>,
}

#[derive(Default)]
struct LegacyOffsets {
    len: usize,
    runs: Vec<OffsetRun>,
}

struct OffsetRun {
    old: Range<usize>,
    current: usize,
}

impl LegacyOffsets {
    fn record(&mut self, old: Range<usize>, current: usize) {
        if let Some(last) = self.runs.last_mut()
            && last.old.end == old.start
            && last.current + last.old.len() == current
        {
            last.old.end = old.end;
            return;
        }
        self.runs.push(OffsetRun { old, current });
    }

    fn project(&self, offset: usize, length: usize) -> Option<usize> {
        if offset >= self.len {
            return (offset == self.len).then_some(length);
        }
        let index = self.runs.partition_point(|run| run.old.end <= offset);
        if let Some(run) = self.runs.get(index)
            && run.old.contains(&offset)
        {
            return Some(run.current + offset - run.old.start);
        }
        // Previously inline pronunciation and fallback punctuation now occupy one caret boundary.
        Some(index.checked_sub(1).map_or(0, |index| {
            let run = &self.runs[index];
            run.current + run.old.len()
        }))
    }
}

pub(crate) struct Element {
    pub tag: String,
    pub classes: String,
    pub id: Option<String>,
    pub inline_style: Option<String>,
    pub parent: Option<usize>,
}

pub(crate) struct StyledText {
    pub bytes: Range<usize>,
    pub element: usize,
    pub language: Option<Language>,
}

pub(crate) enum Stylesheet {
    Linked(String),
    Inline(String),
}

pub(crate) struct InlineLink {
    pub bytes: Range<usize>,
    pub href: String,
}

pub(crate) struct Source {
    pub revision: u64,
    legacy_revision: Option<u64>,
    pub blocks: Vec<Block>,
    pub fragments: BTreeMap<String, (usize, usize)>,
    pub stylesheets: Vec<Stylesheet>,
    pub elements: Vec<Element>,
    insertions: Vec<(usize, usize)>,
    normalized_xhtml: Option<String>,
    ruby_exclusions: Vec<Range<usize>>,
    head_end: usize,
}

#[derive(Default)]
struct LegacyExtraction {
    text: String,
    spaces: bool,
    offsets: LegacyOffsets,
}

impl LegacyExtraction {
    fn push(&mut self, content: &str) {
        for c in content.chars() {
            emit(&mut self.text, &mut self.spaces, c);
        }
    }
}

fn emit(text: &mut String, spaces: &mut bool, c: char) -> Option<Range<usize>> {
    if matches!(c, ' ' | '\t' | '\r' | '\n' | '\u{c}') {
        *spaces = true;
        return None;
    }
    let start = text.len();
    if *spaces && !text.is_empty() && !text.ends_with('\n') {
        text.push(' ');
    }
    text.push(c);
    *spaces = false;
    Some(start..text.len())
}

struct RubyDraft {
    source: Range<usize>,
    annotation: String,
    annotation_node: NodeId,
    bytes: Option<Range<usize>>,
}

fn ruby_markup(block: Node<'_, '_>, available: &mut usize) -> (Vec<RubyDraft>, Vec<Range<usize>>) {
    let mut drafts = Vec::new();
    let mut exclusions = Vec::new();
    for ruby in block.descendants().filter(|node| {
        node.has_tag_name("ruby")
            && !hidden(*node)
            && !node
                .ancestors()
                .skip(1)
                .any(|parent| parent.has_tag_name("ruby"))
    }) {
        let Some((groups, ranges)) = simple_ruby(ruby, *available) else {
            continue;
        };
        *available -= groups.len();
        drafts.extend(groups);
        exclusions.extend(ranges);
    }
    (drafts, exclusions)
}

fn simple_ruby(node: Node<'_, '_>, limit: usize) -> Option<(Vec<RubyDraft>, Vec<Range<usize>>)> {
    if limit == 0
        || node
            .descendants()
            .skip(1)
            .filter(|child| !hidden(*child))
            .any(|child| {
                is_block(child)
                    || matches!(
                        child.tag_name().name(),
                        "ruby" | "rtc" | "rbc" | "br" | "img"
                    )
                    || (matches!(child.tag_name().name(), "rt" | "rp")
                        && child.parent() != Some(node))
            })
    {
        return None;
    }
    let mut drafts = Vec::new();
    let mut exclusions = Vec::new();
    let mut base = None;
    for child in node.children().filter(|child| !hidden(*child)) {
        if child.has_tag_name("rt") || child.has_tag_name("rp") {
            if child.children().any(|part| part.is_element()) {
                return None;
            }
            exclusions.push(child.range());
            if child.has_tag_name("rt") {
                let start = base.take()?;
                let mut annotation = String::new();
                let mut spaces = false;
                for part in child.children().filter(|part| part.is_text()) {
                    for c in part.text()?.chars() {
                        emit(&mut annotation, &mut spaces, c);
                        if annotation.len() > MAX_RUBY_ANNOTATION_BYTES {
                            return None;
                        }
                    }
                }
                if annotation.is_empty()
                    || annotation.len() > MAX_RUBY_ANNOTATION_BYTES
                    || drafts.len() == limit
                {
                    return None;
                }
                drafts.push(RubyDraft {
                    source: start..child.range().start,
                    annotation,
                    annotation_node: child.id(),
                    bytes: None,
                });
            }
        } else if base.is_none()
            && child.descendants().any(|part| {
                part.is_text()
                    && !hidden(part)
                    && part.text().is_some_and(|text| !text.trim().is_empty())
            })
        {
            base = Some(child.range().start);
        }
    }
    (!drafts.is_empty()).then_some((drafts, exclusions))
}

impl Source {
    pub fn parse(xhtml: &str) -> Result<Self> {
        Self::parse_document(xhtml, crate::revision(xhtml), true)
    }

    fn parse_document(
        xhtml: &str,
        original_revision: u64,
        allow_normalization: bool,
    ) -> Result<Self> {
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
        if let Some(head) = head {
            for node in head.children() {
                if node.has_tag_name("style")
                    && let Some(css) = node.text()
                {
                    stylesheets.push(Stylesheet::Inline(css.to_owned()));
                }
                if node.has_tag_name("link")
                    && node.attribute("rel").is_some_and(|rel| {
                        rel.split_ascii_whitespace()
                            .any(|name| name.eq_ignore_ascii_case("stylesheet"))
                    })
                    && let Some(href) = node.attribute("href")
                {
                    stylesheets.push(Stylesheet::Linked(href.to_owned()));
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
        let mut elements = Vec::new();
        let mut element_ids = HashMap::new();
        let mut languages = Vec::new();
        for node in std::iter::once(root)
            .chain(body.descendants())
            .filter(Node::is_element)
        {
            let parent = node
                .parent()
                .and_then(|parent| element_ids.get(&parent.id()).copied());
            if (node != root && parent.is_none()) || hidden_element(node) {
                continue;
            }
            element_ids.insert(node.id(), elements.len());
            languages.push(content_language(node, parent.and_then(|id| languages[id])));
            elements.push(Element {
                tag: node.tag_name().name().to_owned(),
                classes: node.attribute("class").unwrap_or_default().to_owned(),
                id: node.attribute("id").map(str::to_owned),
                inline_style: node.attribute("style").map(str::to_owned),
                parent,
            });
        }
        let mut blocks = Vec::new();
        let mut fragments = BTreeMap::new();
        let mut insertions = Vec::new();
        let mut ruby_exclusions = Vec::new();
        let mut available_ruby = MAX_RUBY_GROUPS;
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
            let mut styled_text: Vec<StyledText> = Vec::new();
            #[cfg(test)]
            let mut emphasis: Vec<(Range<usize>, Emphasis)> = Vec::new();
            let mut links: Vec<InlineLink> = Vec::new();
            let mut last_link = None;
            let mut spaces = false;
            let mut images: Vec<EmbeddedImage> = Vec::new();
            let mut image_after_space: Option<usize> = None;
            let mut ids = Vec::new();
            let (mut ruby, exclusions) = ruby_markup(node, &mut available_ruby);
            let mut legacy = (!ruby.is_empty()).then(LegacyExtraction::default);
            for descendant in node.descendants().filter(|d| !hidden(*d)) {
                let raw = descendant.range();
                let excluded = exclusions.partition_point(|span| span.end <= raw.start);
                if exclusions
                    .get(excluded)
                    .is_some_and(|span| span.contains(&raw.start))
                {
                    if let Some(legacy) = &mut legacy
                        && let Some(content) = descendant.text().filter(|_| descendant.is_text())
                    {
                        legacy.push(content);
                    }
                    continue;
                }
                if let Some(id) = descendant.attribute("id") {
                    ids.push((
                        id,
                        text.len()
                            + usize::from(spaces && !text.is_empty() && !text.ends_with('\n')),
                    ));
                }
                if descendant.has_tag_name("br") {
                    image_after_space = None;
                    if let Some(legacy) = &mut legacy {
                        let at = legacy.text.len();
                        legacy.text.push('\n');
                        legacy.spaces = false;
                        legacy.offsets.record(at..at + 1, text.len());
                    }
                    text.push('\n');
                    spaces = false;
                }
                let external = illustration_href(descendant);
                let inline_svg = (external.is_none()
                    && descendant.has_tag_name(("http://www.w3.org/2000/svg", "svg")))
                .then(|| crate::svg_image::inline_source(descendant, xhtml))
                .flatten();
                if external.is_some() || inline_svg.is_some() {
                    let pixels = |name| {
                        descendant.attribute(name).and_then(|v| {
                            v.trim()
                                .parse::<f32>()
                                .ok()
                                .filter(|v| v.is_finite() && *v > 0.0)
                        })
                    };
                    image_after_space = (spaces && !text.is_empty() && !text.ends_with('\n'))
                        .then_some(images.len());
                    images.push(EmbeddedImage {
                        href: external.map(str::to_owned).unwrap_or_else(|| {
                            format!("reader-inline-svg:{}", descendant.range().start)
                        }),
                        inline_svg,
                        space_after: false,
                        offset: text.len()
                            + usize::from(spaces && !text.is_empty() && !text.ends_with('\n')),
                        element: element_ids[&descendant.id()],
                        width: pixels("width"),
                        height: pixels("height"),
                        link: descendant
                            .ancestors()
                            .find(|ancestor| ancestor.has_tag_name("a"))
                            .and_then(|anchor| anchor.attribute("href"))
                            .map(str::to_owned),
                        label: descendant
                            .attribute("alt")
                            .map(str::trim)
                            .filter(|label| !label.is_empty())
                            .unwrap_or("Illustration")
                            .chars()
                            .take(128)
                            .collect(),
                    });
                }
                let Some(content) = descendant.text().filter(|_| descendant.is_text()) else {
                    continue;
                };
                if let Some(index) = image_after_space.take() {
                    // Extracted text collapses whitespace across an illustration.
                    // The picture still separates the two rendered word spaces.
                    images[index].space_after = content.starts_with(char::is_whitespace);
                }
                let start = text.len();
                if let Some(legacy) = &mut legacy {
                    for c in content.chars() {
                        let old = emit(&mut legacy.text, &mut legacy.spaces, c);
                        let current = emit(&mut text, &mut spaces, c);
                        if let (Some(old), Some(current)) = (old, current) {
                            let length = old.len().min(current.len());
                            legacy
                                .offsets
                                .record(old.end - length..old.end, current.end - length);
                        }
                    }
                    let group = ruby.partition_point(|draft| draft.source.end <= raw.start);
                    if let Some(draft) = ruby.get_mut(group)
                        && draft.source.contains(&raw.start)
                        && let Some(first) = text[start..].find(|c| c != ' ')
                    {
                        let range = draft.bytes.get_or_insert_with(|| start + first..text.len());
                        range.end = text.len();
                    }
                } else {
                    content.chars().for_each(|c| {
                        emit(&mut text, &mut spaces, c);
                    });
                }
                if start < text.len() {
                    let element = descendant
                        .parent()
                        .and_then(|parent| element_ids.get(&parent.id()).copied())
                        .ok_or("The chapter's text has no style element")?;
                    if let Some(previous) = styled_text.last_mut()
                        && previous.element == element
                        && previous.bytes.end == start
                    {
                        previous.bytes.end = text.len();
                    } else {
                        styled_text.push(StyledText {
                            bytes: start..text.len(),
                            element,
                            language: languages[element],
                        });
                    }
                    #[cfg(test)]
                    emphasis.push((
                        start..text.len(),
                        Emphasis {
                            bold: descendant
                                .ancestors()
                                .any(|a| a.has_tag_name("strong") || a.has_tag_name("b")),
                            italic: descendant
                                .ancestors()
                                .any(|a| a.has_tag_name("em") || a.has_tag_name("i")),
                        },
                    ));
                    if let Some(anchor) = descendant
                        .ancestors()
                        .take_while(|a| *a != node)
                        .find(|a| a.has_tag_name("a") && a.attribute("href").is_some())
                    {
                        if last_link == Some(anchor.id()) {
                            links.last_mut().unwrap().bytes.end = text.len();
                        } else {
                            links.push(InlineLink {
                                bytes: start..text.len(),
                                href: anchor.attribute("href").unwrap().to_owned(),
                            });
                            last_link = Some(anchor.id());
                        }
                    } else {
                        last_link = None;
                    }
                }
            }
            if text.is_empty() && images.is_empty() {
                continue;
            }
            for image in &mut images {
                image.offset = image.offset.min(text.len());
            }
            for (id, offset) in ids {
                fragments
                    .entry(id.to_owned())
                    .or_insert((key, offset.min(text.len())));
            }
            if !text.is_empty() || !images.is_empty() {
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
            ruby_exclusions.extend(exclusions);
            blocks.push(Block {
                key,
                text,
                element: element_ids[&node.id()],
                styled_text,
                #[cfg(test)]
                emphasis,
                links,
                images,
                direction: base_direction(node),
                language: languages[element_ids[&node.id()]],
                ruby: ruby
                    .into_iter()
                    .filter_map(|draft| {
                        Some(Ruby {
                            bytes: draft.bytes?,
                            annotation: draft.annotation,
                            language: languages[element_ids[&draft.annotation_node]],
                        })
                    })
                    .collect(),
                legacy: legacy.map(|mut legacy| {
                    legacy.offsets.len = legacy.text.len();
                    legacy.offsets
                }),
            });
        }
        let selected = blocks
            .iter()
            .map(|block| block.key)
            .collect::<BTreeSet<_>>();
        if body.descendants().any(|node| {
            let visible = may_be_illustration(node)
                || (node.is_text() && node.text().is_some_and(|text| !text.trim().is_empty()));
            visible
                && !hidden(node)
                && !node
                    .ancestors()
                    .any(|parent| selected.contains(&parent.range().start))
        }) {
            if allow_normalization && let Some(normalized) = wrap_inline_runs(xhtml, body) {
                let mut parsed = Self::parse_document(&normalized, original_revision, false)?;
                parsed.normalized_xhtml = Some(normalized);
                return Ok(parsed);
            }
            return Err("The chapter's inline content could not be laid out".into());
        }
        // Some publishers insert an intentionally blank side of a leaf. Preserve
        // its spine location while continuing to reject unrenderable artwork.
        if blocks.is_empty()
            && body.descendants().any(|node| {
                !hidden(node)
                    && matches!(
                        node.tag_name().name(),
                        "svg"
                            | "img"
                            | "object"
                            | "iframe"
                            | "canvas"
                            | "math"
                            | "video"
                            | "audio"
                            | "embed"
                    )
            })
        {
            return Err("The chapter has no supported reading content".into());
        }
        let legacy_revision = blocks
            .iter()
            .any(|block| block.legacy.is_some())
            .then_some(original_revision);
        let revision = if legacy_revision.is_some() {
            original_revision ^ RUBY_TEXT_MODEL
        } else {
            original_revision
        };
        let head_end = head
            .and_then(|node| {
                xhtml[node.range()]
                    .rfind("</")
                    .map(|offset| node.range().start + offset)
            })
            .unwrap_or(body.range().start);
        Ok(Self {
            revision,
            legacy_revision,
            blocks,
            fragments,
            stylesheets,
            elements,
            insertions,
            normalized_xhtml: None,
            ruby_exclusions,
            head_end,
        })
    }

    pub(crate) fn project_offset(
        &self,
        block: &Block,
        revision: u64,
        offset: usize,
    ) -> Option<usize> {
        let current = if revision == self.revision {
            offset
        } else if self.legacy_revision == Some(revision) {
            match &block.legacy {
                Some(legacy) => legacy.project(offset, block.text.len())?,
                None => offset,
            }
        } else {
            return None;
        };
        block.text.is_char_boundary(current).then_some(current)
    }

    pub fn with_styles(&self, xhtml: &str, stylesheet: &str) -> String {
        let xhtml = self.normalized_xhtml.as_deref().unwrap_or(xhtml);
        let mut html = String::new();
        html.push_str(&xhtml[..self.head_end]);
        html.push_str("<style>");
        html.push_str(stylesheet);
        html.push_str("</style>");
        let mut cursor = self.head_end;
        let mut exclusions = self.ruby_exclusions.iter().peekable();
        for &(offset, key) in &self.insertions {
            use std::fmt::Write;
            while let Some(span) = exclusions.next_if(|span| span.end <= offset) {
                html.push_str(&xhtml[cursor..span.start]);
                cursor = span.end;
            }
            html.push_str(&xhtml[cursor..offset]);
            write!(html, " {SOURCE_ATTRIBUTE}=\"{key}\"").unwrap();
            cursor = offset;
        }
        for span in exclusions {
            html.push_str(&xhtml[cursor..span.start]);
            cursor = span.end;
        }
        html.push_str(&xhtml[cursor..]);
        html
    }
}

// XHTML permits inline runs alongside block children. Give each such run a
// layout box in place so its text, images and links stay in document order.
// The same normalized source feeds both layout engines and every anchor.
fn wrap_inline_runs(xhtml: &str, body: Node<'_, '_>) -> Option<String> {
    let mut parents = std::collections::HashSet::new();
    for block in body
        .descendants()
        .filter(|node| is_block(*node) && !hidden(*node))
    {
        for parent in block.ancestors().skip(1) {
            if !parents.insert(parent.id()) || parent == body {
                break;
            }
        }
    }
    let mut ranges = Vec::new();
    for parent in body
        .descendants()
        .filter(|node| parents.contains(&node.id()))
    {
        let mut pending: Option<Range<usize>> = None;
        let mut visible = false;
        for child in parent.children() {
            if is_block(child) || parents.contains(&child.id()) {
                if let Some(run) = pending.take()
                    && visible
                {
                    ranges.push(run);
                }
                visible = false;
                continue;
            }
            let bounds = child.range();
            pending.get_or_insert(bounds.clone()).end = bounds.end;
            visible |= child.descendants().any(|node| {
                !hidden(node)
                    && (may_be_illustration(node)
                        || node.has_tag_name("br")
                        || node.is_text()
                            && node.text().is_some_and(|text| !text.trim().is_empty()))
            });
        }
        if let Some(run) = pending
            && visible
        {
            ranges.push(run);
        }
    }
    if ranges.is_empty() {
        return None;
    }
    ranges.sort_unstable_by_key(|range| range.start);
    let mut normalized = String::with_capacity(xhtml.len() + 11 * ranges.len());
    let mut cursor = 0;
    for run in ranges {
        normalized.push_str(&xhtml[cursor..run.start]);
        normalized.push_str("<div>");
        normalized.push_str(&xhtml[run.clone()]);
        normalized.push_str("</div>");
        cursor = run.end;
    }
    normalized.push_str(&xhtml[cursor..]);
    Some(normalized)
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

fn content_language(node: Node<'_, '_>, inherited: Option<Language>) -> Option<Language> {
    match node
        .attribute((XML_NAMESPACE, "lang"))
        .or_else(|| node.attribute("lang"))
    {
        Some(tag) if tag.len() <= MAX_LANGUAGE_BYTES => Language::parse(tag).ok(),
        Some(_) => None,
        None => inherited,
    }
}

fn base_direction(node: Node<'_, '_>) -> BaseDirection {
    node.ancestors()
        .filter_map(|ancestor| ancestor.attribute("dir"))
        .find_map(|direction| {
            if direction.eq_ignore_ascii_case("ltr") {
                Some(BaseDirection::Ltr)
            } else if direction.eq_ignore_ascii_case("rtl") {
                Some(BaseDirection::Rtl)
            } else if direction.eq_ignore_ascii_case("auto") {
                Some(BaseDirection::Auto)
            } else {
                None
            }
        })
        .unwrap_or_default()
}

fn may_be_illustration(node: Node<'_, '_>) -> bool {
    node.has_tag_name("img") && node.has_attribute("src")
        || node.has_tag_name(("http://www.w3.org/2000/svg", "svg"))
}

// Common EPUB cover pages fit a single archive image to an SVG viewport. Keep
// the wrapper atomic so SVG metadata never becomes selectable reading text.
pub(crate) fn illustration_href<'a>(node: Node<'a, '_>) -> Option<&'a str> {
    if node.has_tag_name("img") {
        return node.attribute("src");
    }
    if !node.has_tag_name(("http://www.w3.org/2000/svg", "svg")) {
        return None;
    }
    let mut artwork = node.children().filter(|child| {
        child.is_element() && !child.has_tag_name("title") && !child.has_tag_name("desc")
    });
    let image = artwork.next()?;
    if !image.has_tag_name(("http://www.w3.org/2000/svg", "image"))
        || artwork.next().is_some()
        || ["transform", "x", "y", "opacity", "preserveAspectRatio"]
            .iter()
            .any(|attr| image.has_attribute(*attr))
    {
        return None;
    }
    image
        .attribute("href")
        .or_else(|| image.attribute(("http://www.w3.org/1999/xlink", "href")))
}

fn hidden(node: Node<'_, '_>) -> bool {
    node.ancestors().any(hidden_element)
}

fn hidden_element(node: Node<'_, '_>) -> bool {
    node.parent_element()
        .is_some_and(|parent| parent.has_tag_name("svg"))
        || node.is_element()
            && (matches!(
                node.tag_name().name(),
                "script" | "style" | "noscript" | "template"
            ) || node.has_attribute("hidden"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_mixed_text_and_images_without_losing_document_order_or_markup() {
        let xml = r##"<html><head></head><body><div id="group">Before <a href="#last">jump</a>.
          <p>Nested paragraph.</p><img src="photo.jpg"/><section><p>Middle.</p>
          Trailing <em id="last">words.</em></section>End.</div></body></html>"##;
        let source = Source::parse(xml).unwrap();
        assert_eq!(
            source
                .blocks
                .iter()
                .map(|b| b.text.as_str())
                .collect::<Vec<_>>(),
            [
                "Before jump.",
                "Nested paragraph.",
                "",
                "Middle.",
                "Trailing words.",
                "End."
            ]
        );
        assert_eq!(source.blocks[2].images[0].href, "photo.jpg");
        assert_eq!(source.blocks[0].links[0].href, "#last");
        assert_eq!(source.fragments["group"], (source.blocks[0].key, 0));
        assert_eq!(
            source.fragments["last"],
            (source.blocks[4].key, "Trailing ".len())
        );
        assert_eq!(source.revision, crate::revision(xml));
        let annotated = source.with_styles(xml, "body{color:black}");
        let dom = Document::parse(&annotated).unwrap();
        assert_eq!(
            dom.descendants()
                .filter(|node| node.has_attribute(SOURCE_ATTRIBUTE))
                .count(),
            6
        );
        assert!(
            Source::parse("<html><body><p>Single.</p></body></html>")
                .unwrap()
                .normalized_xhtml
                .is_none()
        );
    }

    #[test]
    fn svg_image_wrappers_support_href_and_xlink_and_preserve_intentional_blank_pages() {
        for attr in ["href", "xlink:href"] {
            let xml = format!(
                r#"<html><body id="outer"><div><svg xmlns="http://www.w3.org/2000/svg"
                xmlns:xlink="http://www.w3.org/1999/xlink"><title>Cover description</title>
                <image {attr}="../images/art.jpg"/></svg></div></body></html>"#
            );
            let source = Source::parse(&xml).unwrap();
            assert_eq!(source.blocks.len(), 1);
            assert!(source.blocks[0].text.is_empty());
            assert_eq!(source.blocks[0].images[0].href, "../images/art.jpg");
            assert_eq!(source.fragments["outer"], (source.blocks[0].key, 0));
        }
        assert!(
            Source::parse("<html><body><div><p>  </p></div></body></html>")
                .unwrap()
                .blocks
                .is_empty()
        );
        assert!(
            Source::parse(
                r#"<html><body><svg xmlns="http://www.w3.org/2000/svg"><foreignObject/></svg></body></html>"#
            )
            .is_err()
        );
    }

    #[test]
    fn inherits_publisher_language_and_direction_with_inline_resets() {
        let xhtml = r#"<html xml:lang="zh-hant-TW" dir="RTL"><body><section>
            <p>Here <span lang="en-us"><em>English</em></span> <span xml:lang="">unknown</span> <span lang="bad tag">invalid</span>.</p>
            <p dir="lTr" id="later">Later.</p><p dir="auto">Automatic.</p>
            <p dir="invalid">Inherited.</p><p><ruby>字<rt xml:lang="ja">じ</rt></ruby></p>
            </section></body></html>"#;
        let source = Source::parse(xhtml).unwrap();
        let first = &source.blocks[0];
        assert_eq!(first.text, "Here English unknown invalid.");
        let locale = |word: &str| {
            let byte = first.text.find(word).unwrap();
            first
                .styled_text
                .iter()
                .find(|run| run.bytes.contains(&byte))
                .unwrap()
                .language
        };
        assert_eq!(first.language.unwrap().as_str(), "zh-Hant-TW");
        assert_eq!(locale("English").unwrap().as_str(), "en-US");
        assert_eq!(locale("unknown"), None);
        assert_eq!(locale("invalid"), None);
        assert_eq!(first.direction, BaseDirection::Rtl);
        assert_eq!(source.blocks[1].direction, BaseDirection::Ltr);
        assert_eq!(source.blocks[2].direction, BaseDirection::Auto);
        assert_eq!(source.blocks[3].direction, BaseDirection::Rtl);
        assert_eq!(source.blocks[4].ruby[0].language.unwrap().as_str(), "ja");
        assert_eq!(source.fragments["later"], (source.blocks[1].key, 0));
    }

    #[test]
    fn ruby_projects_legacy_utf8_offsets_and_preserves_other_blocks() {
        let xhtml = "<html><body><p>A <ruby>日<rp>(</rp><rt>にち</rt><rp>)</rp>本<rt>ほん</rt></ruby> Z</p><p>Another paragraph.</p></body></html>";
        let source = Source::parse(xhtml).unwrap();
        let old = crate::revision(xhtml);
        assert_ne!(old, source.revision);
        let block = &source.blocks[0];
        assert_eq!(block.text, "A 日本 Z");
        assert_eq!(&block.text[block.ruby[0].bytes.clone()], "日");
        assert_eq!(&block.text[block.ruby[1].bytes.clone()], "本");
        assert_eq!(block.ruby[1].annotation, "ほん");
        let legacy_text = "A 日(にち)本ほん Z";
        for word in ["日", "本", "Z"] {
            assert_eq!(
                source.project_offset(block, old, legacy_text.find(word).unwrap()),
                block.text.find(word)
            );
        }
        assert_eq!(
            source.project_offset(block, old, legacy_text.len()),
            Some(block.text.len())
        );
        assert_eq!(
            source.project_offset(block, old, legacy_text.len() + 1),
            None
        );
        assert_eq!(source.project_offset(&source.blocks[1], old, 8), Some(8));
        assert!(source.with_styles(xhtml, "").contains("<ruby>日本</ruby>"));

        let unsupported =
            "<html><body><p><ruby><rt>Missing base</rt></ruby> remains.</p></body></html>";
        let source = Source::parse(unsupported).unwrap();
        assert_eq!(source.revision, crate::revision(unsupported));
        assert!(source.blocks[0].ruby.is_empty());
        assert_eq!(source.blocks[0].text, "Missing base remains.");
    }
}
