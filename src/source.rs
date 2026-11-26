use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::ops::Range;

use parley::{BaseDirection, Language};
use roxmltree::{Document, Node, NodeId, ParsingOptions};

use crate::Result;

pub(crate) const SOURCE_ATTRIBUTE: &str = "data-primos-source";
#[cfg(test)]
mod flow_metadata_tests;
mod numeric_references;
pub(crate) mod pre_line_projection;
pub(crate) mod raw_whitespace;
#[cfg(feature = "kindle")]
mod selection_html;

const RUBY_TEXT_MODEL: u64 = 0xe026_92a4_3415_b12f;
const MAX_RUBY_GROUPS: usize = 1_024;
const MAX_RUBY_ANNOTATION_BYTES: usize = 192;
pub(crate) const MAX_LANGUAGE_BYTES: usize = 128;
const XML_NAMESPACE: &str = "http://www.w3.org/XML/1998/namespace";

#[derive(Clone, Copy, Default, PartialEq, Eq)]
#[cfg(test)]
pub(crate) struct Emphasis {
    pub bold: bool,
    pub italic: bool,
}

pub(crate) struct Block {
    pub table_cell: Option<crate::table_layout::Cell>,
    pub key: usize,
    pub text: String,
    pub element: usize,
    pub styled_text: Vec<StyledText>,
    /// Optional raw provenance is layout metadata; canonical bytes are unchanged.
    #[allow(dead_code)]
    pub(crate) raw_whitespace: Option<raw_whitespace::Snapshot>,
    /// Layout-only origins. These never participate in canonical text or revisions.
    pub flow_origins: crate::flow_projection::Origins,
    pub(crate) pseudo_edges: Box<[crate::book_style::pseudo::Edge]>,
    pub list_item: Option<crate::list_marker::ListItem>,
    #[cfg(test)]
    pub emphasis: Vec<(Range<usize>, Emphasis)>,
    pub links: Vec<InlineLink>,
    pub direction: BaseDirection,
    pub language: Option<Language>,
    pub images: Vec<EmbeddedImage>,
    pub ruby: Vec<Ruby>,
    legacy: Option<LegacyOffsets>,
    origin: usize,
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
    /// Annotation paint can differ from the base text's inherited visibility.
    pub element: usize,
}

#[derive(Clone, Default)]
struct LegacyOffsets {
    len: usize,
    runs: Vec<OffsetRun>,
}

#[derive(Clone)]
struct OffsetRun {
    old: Range<usize>,
    current: Range<usize>,
}

impl LegacyOffsets {
    fn record(&mut self, old: Range<usize>, current: usize) {
        let end = current + old.len();
        self.record_span(old, current..end);
    }

    fn record_span(&mut self, old: Range<usize>, current: Range<usize>) {
        if let Some(last) = self.runs.last_mut()
            && last.old.len() == last.current.len()
            && old.len() == current.len()
            && last.old.end == old.start
            && last.current.end == current.start
        {
            last.old.end = old.end;
            last.current.end = current.end;
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
            return Some(run.current.start + offset - run.old.start);
        }
        // Previously inline pronunciation and fallback punctuation occupy one caret boundary.
        Some(
            index
                .checked_sub(1)
                .map_or(0, |index| self.runs[index].current.end),
        )
    }

    fn unproject(&self, offset: usize, length: usize) -> Option<usize> {
        if offset >= length {
            return (offset == length).then_some(self.len);
        }
        let index = self.runs.partition_point(|run| run.current.end <= offset);
        if let Some(run) = self.runs.get(index)
            && run.current.contains(&offset)
        {
            return Some(run.old.start + offset - run.current.start);
        }
        Some(
            index
                .checked_sub(1)
                .map_or(0, |index| self.runs[index].old.end),
        )
    }
}

#[derive(Clone, Copy)]
struct Extraction {
    ruby: bool,
    origins: bool,
    ignore_embedded_language: bool,
    preserve_odt_whitespace: bool,
}

impl Default for Extraction {
    fn default() -> Self {
        Self {
            ruby: true,
            origins: false,
            ignore_embedded_language: false,
            preserve_odt_whitespace: false,
        }
    }
}

pub(crate) struct Element {
    pub tag: String,
    pub classes: String,
    pub has_href: bool,
    /// Pinned CRE selectors identify attributes by local name, ignoring prefixes.
    pub type_attribute: Option<String>,
    pub list_kind: Option<crate::list_marker::MarkerKind>,
    pub id: Option<String>,
    pub inline_style: Option<String>,
    pub(crate) pseudo_attributes: Box<[(String, String)]>,
    pub(crate) quote_language: Option<Box<str>>,
    /// Outer option records an explicit attribute, including invalid/empty tags.
    pub(crate) hyphenation_language: Option<Option<crate::TypographyLocale>>,
    pub(crate) resolved_hyphenation_language: Option<crate::TypographyLocale>,
    /// This element's own HTML direction hint, independent of inherited style.
    pub(crate) direction_hint: Option<BaseDirection>,
    pub(crate) table_hints: Option<crate::book_style::table_hints::TableHints>,
    pub parent: Option<usize>,
    pub previous_sibling: Option<usize>,
    pub odd_sibling: bool,
    pub odd_sibling_from_end: bool,
    pub sibling_index: usize,
    pub sibling_index_from_end: usize,
    pub last_child: bool,
    pub first_of_type: bool,
    pub last_of_type: bool,
    pub selector_only: bool,
    pub anonymous: bool,
}

pub(crate) struct StyledText {
    pub bytes: Range<usize>,
    pub element: usize,
    pub language: Option<Language>,
    pub(crate) hyphenation_language: Option<crate::TypographyLocale>,
    pub(crate) preserve_whitespace: bool,
}

pub(crate) enum Stylesheet {
    Linked(String),
    Inline(String),
}

pub(crate) struct InlineLink {
    #[cfg(feature = "kindle")]
    pub note_reference: crate::footnotes::Reference,
    pub bytes: Range<usize>,
    pub href: String,
}

pub(crate) struct Source {
    pub revision: u64,
    /// Raw publication identity is independent of Ruby/numeric text-model
    /// revisions. Internal parse stages are unbound until the outer entry sets it.
    original_input_sha256: Option<[u8; 32]>,
    legacy_revision: Option<u64>,
    previous_mobi: BTreeMap<u64, BTreeMap<usize, usize>>,
    numeric_history: Vec<numeric_references::History>,
    origins: Vec<usize>,
    // Only populated by the existing numeric-history parse, where normalized
    // XHTML can have different anonymous wrappers. Not a canonical text model.
    element_origins: Vec<Option<usize>>,
    pub blocks: Vec<Block>,
    /// Separate, unrendered candidates; canonical block order is unchanged.
    pub(crate) raw_only_blocks: Vec<Block>,
    pub fragments: BTreeMap<String, (usize, usize)>,
    fragment_owners: BTreeMap<String, usize>,
    pub list_counters: Vec<crate::list_marker::CounterGroup>,
    pub stylesheets: Vec<Stylesheet>,
    pub elements: Vec<Element>,
    pub table_cells: Vec<(usize, crate::table_layout::Cell)>,
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

fn emit_preserved(text: &mut String, spaces: &mut bool, c: char) -> Option<Range<usize>> {
    let start = text.len();
    if *spaces && !text.is_empty() && !text.ends_with('\n') {
        text.push(' ');
    }
    *spaces = false;
    text.push(c);
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

fn fragment_ids<'a>(
    node: Node<'a, '_>,
    navigation: &'a HashMap<NodeId, String>,
) -> impl Iterator<Item = &'a str> {
    node.attribute("id")
        .into_iter()
        .chain(navigation.get(&node.id()).map(String::as_str))
}

impl Source {
    /// Project verified MOBI shadow links without changing canonical XHTML,
    /// revisions, keys, styles or the saved anchor model.
    pub(crate) fn apply_mobi_shadow(
        &mut self,
        shadow: &Source,
        ids: &BTreeSet<String>,
    ) -> BTreeSet<String> {
        if self.blocks.len() != shadow.blocks.len()
            || self.blocks.iter().zip(&shadow.blocks).any(|(a, b)| {
                a.text != b.text
                    || !a
                        .images
                        .iter()
                        .map(|im| (&im.href, im.offset))
                        .eq(b.images.iter().map(|im| (&im.href, im.offset)))
            })
        {
            return BTreeSet::new();
        }
        let mut mapped = vec![None; shadow.elements.len()];
        let mut next = 0usize;
        for (index, element) in shadow.elements.iter().enumerate() {
            if element.tag == "span" && element.id.as_ref().is_some_and(|id| ids.contains(id)) {
                continue;
            }
            let Some(original) = self.elements.get(next) else {
                return BTreeSet::new();
            };
            let parent = element
                .parent
                .and_then(|parent| mapped.get(parent).copied().flatten());
            if element.tag != original.tag
                || element.classes != original.classes
                || element.id != original.id
                || element.inline_style != original.inline_style
                || element.direction_hint != original.direction_hint
                || element.anonymous != original.anonymous
                || element.selector_only != original.selector_only
                || parent != original.parent
            {
                return BTreeSet::new();
            }
            mapped[index] = Some(next);
            next += 1;
        }
        if next != self.elements.len() {
            return BTreeSet::new();
        }
        let block_indices: BTreeMap<_, _> = shadow
            .blocks
            .iter()
            .enumerate()
            .map(|(index, block)| (block.key, index))
            .collect();
        let mut resolved = BTreeSet::new();
        for id in ids {
            let Some(&(shadow_key, offset)) = shadow.fragments.get(id) else {
                continue;
            };
            let Some(&index) = block_indices.get(&shadow_key) else {
                continue;
            };
            let Some(&owner) = shadow.fragment_owners.get(id) else {
                continue;
            };
            let Some(element) = shadow.elements.get(owner) else {
                continue;
            };
            let Some(parent) = element
                .parent
                .and_then(|parent| mapped.get(parent).copied().flatten())
            else {
                continue;
            };
            let block = &self.blocks[index];
            if offset > block.text.len()
                || !block.text.is_char_boundary(offset)
                || self.fragments.contains_key(id)
            {
                continue;
            }
            self.fragments.insert(id.clone(), (block.key, offset));
            self.fragment_owners.insert(id.clone(), parent);
            resolved.insert(id.clone());
        }
        for (actual, other) in self.blocks.iter_mut().zip(&shadow.blocks) {
            for link in &other.links {
                let Some(id) = link.href.strip_prefix('#') else {
                    continue;
                };
                if !resolved.contains(id)
                    || link.bytes.start >= link.bytes.end
                    || link.bytes.end > actual.text.len()
                    || !actual.text.is_char_boundary(link.bytes.start)
                    || !actual.text.is_char_boundary(link.bytes.end)
                    || actual.links.iter().any(|previous| {
                        previous.bytes.start < link.bytes.end
                            && previous.bytes.end > link.bytes.start
                    })
                {
                    continue;
                }
                actual.links.push(InlineLink {
                    #[cfg(feature = "kindle")]
                    note_reference: link.note_reference,
                    bytes: link.bytes.clone(),
                    href: link.href.clone(),
                });
            }
        }
        resolved
    }
    /// Apply the selected typography locale without changing canonical text or
    /// shaping language. Embedded invalid or empty tags explicitly clear it.
    pub(crate) fn resolve_hyphenation_languages(
        &mut self,
        main: Option<crate::TypographyLocale>,
        respect_embedded: bool,
    ) {
        let mut locales = Vec::with_capacity(self.elements.len());
        for element in &self.elements {
            let inherited = match element.parent {
                Some(id) => locales.get(id).copied().flatten(),
                None => main,
            };
            let locale = if respect_embedded {
                element.hyphenation_language.unwrap_or(inherited)
            } else {
                main
            };
            locales.push(locale);
        }
        for (element, locale) in self.elements.iter_mut().zip(&locales) {
            element.resolved_hyphenation_language = *locale;
        }
        for block in &mut self.blocks {
            for run in &mut block.styled_text {
                run.hyphenation_language = locales.get(run.element).copied().flatten();
            }
        }
    }

    pub fn parse(xhtml: &str) -> Result<Self> {
        Self::parse_with_language(xhtml, None)
    }

    pub(crate) fn parse_with_language(
        xhtml: &str,
        publication_language: Option<Language>,
    ) -> Result<Self> {
        Self::parse_with_kindle_elements(xhtml, publication_language, &BTreeSet::new())
    }

    pub(crate) fn parse_with_kindle_elements(
        xhtml: &str,
        publication_language: Option<Language>,
        kindle_elements: &BTreeSet<usize>,
    ) -> Result<Self> {
        Self::parse_with_typography(xhtml, publication_language, true, kindle_elements)
    }

    pub(crate) fn parse_with_typography(
        xhtml: &str,
        publication_language: Option<Language>,
        respect_embedded: bool,
        kindle_elements: &BTreeSet<usize>,
    ) -> Result<Self> {
        Self::parse_with_typography_flags(
            xhtml,
            publication_language,
            respect_embedded,
            kindle_elements,
            false,
        )
    }

    pub(crate) fn parse_odt_with_typography(
        xhtml: &str,
        publication_language: Option<Language>,
        respect_embedded: bool,
        kindle_elements: &BTreeSet<usize>,
    ) -> Result<Self> {
        Self::parse_with_typography_flags(
            xhtml,
            publication_language,
            respect_embedded,
            kindle_elements,
            true,
        )
    }

    fn parse_with_typography_flags(
        xhtml: &str,
        publication_language: Option<Language>,
        respect_embedded: bool,
        kindle_elements: &BTreeSet<usize>,
        preserve_odt_whitespace: bool,
    ) -> Result<Self> {
        let input = ring::digest::digest(&ring::digest::SHA256, xhtml.as_bytes());
        let mut fingerprint = [0; 32];
        fingerprint.copy_from_slice(input.as_ref());
        let mut parsed = if let Some(normalized) = numeric_references::normalize(xhtml) {
            numeric_references::parse(
                xhtml,
                normalized,
                publication_language,
                respect_embedded,
                kindle_elements,
                preserve_odt_whitespace,
            )?
        } else {
            Self::parse_document(
                xhtml,
                crate::revision(xhtml),
                None,
                publication_language,
                kindle_elements,
                Extraction {
                    ignore_embedded_language: !respect_embedded,
                    preserve_odt_whitespace,
                    ..Extraction::default()
                },
            )?
        };
        // Bind the original bytes after all numeric/anonymous transformations;
        // their canonical and historical revisions retain their existing values.
        parsed.original_input_sha256 = Some(fingerprint);
        Ok(parsed)
    }

    fn parse_document(
        xhtml: &str,
        original_revision: u64,
        anonymous_boxes: Option<&[usize]>,
        publication_language: Option<Language>,
        kindle_elements: &BTreeSet<usize>,
        extraction: Extraction,
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
        let mut origins = Vec::new();
        let mut node_origins = HashMap::new();
        if extraction.origins {
            for node in root.descendants().filter(|node| {
                !anonymous_boxes
                    .is_some_and(|starts| starts.binary_search(&node.range().start).is_ok())
            }) {
                node_origins.insert(node.id(), origins.len());
                origins.push(node.range().start);
            }
        }
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
        let mut navigation_ids = HashMap::new();
        if !kindle_elements.is_empty() {
            for (i, node) in root
                .descendants()
                .filter(crate::kf8::publisher_contents::html_node)
                .filter(|node| {
                    !anonymous_boxes
                        .is_some_and(|starts| starts.binary_search(&node.range().start).is_ok())
                })
                .enumerate()
            {
                if kindle_elements.contains(&i) {
                    navigation_ids
                        .insert(node.id(), crate::kf8::publisher_contents::element_name(i));
                }
            }
        }
        let mut elements: Vec<Element> = Vec::new();
        let mut element_ids = HashMap::new();
        let mut element_origins = Vec::new();
        let mut languages = Vec::new();
        let mut selector_only_nodes = Vec::new();
        let mut seen_types = HashSet::new();
        let mut last_types = HashMap::new();
        let mut last_children = HashMap::new();
        let mut pseudo_attribute_bytes = crate::book_style::pseudo::MAX_ATTRIBUTE_BYTES;
        for node in root.descendants().filter(Node::is_element) {
            let parent: Option<usize> = node
                .parent()
                .and_then(|p| element_ids.get(&p.id()).copied());
            if node != root && parent.is_none_or(|index| elements[index].selector_only) {
                continue;
            }
            let anonymous = anonymous_boxes
                .is_some_and(|starts| starts.binary_search(&node.range().start).is_ok());
            // A layout box generated for a loose inline run has no counterpart in
            // the publisher's DOM. Its children keep their original CSS relationships.
            let parent = parent.and_then(|i| {
                if elements[i].anonymous {
                    elements[i].parent
                } else {
                    Some(i)
                }
            });
            // Hidden elements still participate in sibling selectors. Retain their
            // identity but never expose their text, IDs, list items or cells to Reader.
            let selector_only = (parent == Some(0) && node != body) || hidden_element(node);
            if selector_only {
                selector_only_nodes.push(node.id());
            }
            // User-origin CSS can be enabled later, even when the publisher
            // supplies no sheet or publisher CSS is disabled. Keep real sibling
            // metadata independently of stylesheet presence. Anonymous layout
            // wrappers still do not acquire publisher DOM relationships.
            let structural_element = !anonymous;
            let previous_sibling = if structural_element {
                parent.and_then(|i| last_children.insert(i, elements.len()))
            } else {
                None
            };
            if let Some(previous) = previous_sibling {
                elements[previous].last_child = false;
            }
            let tag = node.tag_name();
            let first_of_type =
                structural_element && seen_types.insert((parent, tag.namespace(), tag.name()));
            if structural_element
                && let Some(previous) =
                    last_types.insert((parent, tag.namespace(), tag.name()), elements.len())
            {
                elements[previous].last_of_type = false;
            }
            // CRE inserts an implicit tbody for consecutive direct table rows.
            let direct_row =
                tag.name() == "tr" && parent.is_some_and(|i| elements[i].tag == "table");
            let odd_sibling = previous_sibling
                .filter(|&i| !direct_row || elements[i].tag == "tr")
                .is_none_or(|i| !elements[i].odd_sibling);
            let sibling_index = previous_sibling
                .filter(|&i| !direct_row || elements[i].tag == "tr")
                .map_or(1, |i| elements[i].sibling_index.saturating_add(1));
            element_ids.insert(node.id(), elements.len());
            if extraction.origins {
                element_origins.push(node_origins.get(&node.id()).copied());
            }
            let inherited = if node == root {
                publication_language
            } else {
                parent.and_then(|id| languages[id])
            };
            languages.push(if extraction.ignore_embedded_language {
                publication_language
            } else {
                content_language(node, inherited)
            });
            elements.push(Element {
                tag: node.tag_name().name().to_owned(),
                list_kind: crate::list_marker::MarkerKind::html(node),
                has_href: node.has_attribute("href"),
                type_attribute: node
                    .attributes()
                    .find(|a| a.name() == "type")
                    .map(|a| a.value().to_owned()),
                classes: node.attribute("class").unwrap_or_default().to_owned(),
                id: node.attribute("id").map(str::to_owned),
                inline_style: if selector_only {
                    None
                } else {
                    node.attribute("style").map(str::to_owned)
                },
                pseudo_attributes: if selector_only {
                    Default::default()
                } else {
                    crate::book_style::pseudo::read_attributes(node, &mut pseudo_attribute_bytes)
                },
                quote_language: language_attribute(node)
                    .filter(|tag| !tag.is_empty() && tag.len() <= MAX_LANGUAGE_BYTES)
                    .map(Box::from),
                hyphenation_language: language_attribute(node).map(|tag| {
                    (tag.len() <= MAX_LANGUAGE_BYTES)
                        .then(|| crate::TypographyLocale::parse(tag))
                        .flatten()
                }),
                resolved_hyphenation_language: None,
                direction_hint: node.attribute("dir").and_then(parse_direction),
                table_hints: if selector_only {
                    None
                } else {
                    crate::book_style::table_hints::TableHints::read(node)
                },
                parent,
                previous_sibling,
                odd_sibling,
                odd_sibling_from_end: true,
                sibling_index,
                sibling_index_from_end: 1,
                last_child: structural_element,
                first_of_type,
                last_of_type: structural_element,
                selector_only,
                anonymous,
            });
        }
        // Walk each real sibling chain once from its final child. Anonymous
        // wrappers have no structural sibling link and cannot match selectors.
        for &last in last_children.values() {
            let mut current = Some(last);
            let mut next: Option<usize> = None;
            while let Some(index) = current {
                let direct_row = elements[index].tag == "tr"
                    && elements[index]
                        .parent
                        .is_some_and(|parent| elements[parent].tag == "table");
                let odd_from_end = next
                    .filter(|&i| !direct_row || elements[i].tag == "tr")
                    .is_none_or(|i| !elements[i].odd_sibling_from_end);
                let index_from_end = next
                    .filter(|&i| !direct_row || elements[i].tag == "tr")
                    .map_or(1, |i| elements[i].sibling_index_from_end.saturating_add(1));
                elements[index].odd_sibling_from_end = odd_from_end;
                elements[index].sibling_index_from_end = index_from_end;
                next = Some(index);
                current = elements[index].previous_sibling;
            }
        }
        drop(seen_types);
        drop(last_types);
        drop(last_children);
        for id in selector_only_nodes {
            element_ids.remove(&id);
        }
        let mut list_items = crate::list_marker::Items::read(body, &element_ids);
        let tables = crate::table_layout::TableCells::read(body, &element_ids);
        let mut blocks = Vec::new();
        let mut raw_only_blocks = Vec::new();
        let mut pseudo_edges_remaining = 32 * 1024;
        let mut fragments = BTreeMap::new();
        let mut fragment_owners = BTreeMap::new();
        let mut insertions = Vec::new();
        let mut ruby_exclusions = Vec::new();
        let mut available_ruby = MAX_RUBY_GROUPS;
        let mut flow_budget = crate::flow_projection::Budget::new();
        let mut raw_whitespace_budget = raw_whitespace::Budget::new();
        let mut raw_only_budget = raw_whitespace::Budget::new();
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
            let mut flow_origins = crate::flow_projection::Origins::new(&flow_budget);
            let mut pseudo_edges = crate::book_style::pseudo::Edges::default();
            #[cfg(test)]
            let mut emphasis: Vec<(Range<usize>, Emphasis)> = Vec::new();
            let mut links: Vec<InlineLink> = Vec::new();
            let mut last_link = None;
            let mut spaces = false;
            let mut images: Vec<EmbeddedImage> = Vec::new();
            let mut image_after_space: Option<usize> = None;
            let mut ids = Vec::new();
            let (mut ruby, exclusions) = if extraction.ruby {
                ruby_markup(node, &mut available_ruby)
            } else {
                (Vec::new(), Vec::new())
            };
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
                if pseudo_edges_remaining > 0 {
                    pseudo_edges.close_before(raw.start, text.len());
                    if descendant != node
                        && descendant.is_element()
                        && !matches!(descendant.tag_name().name(), "br" | "img")
                        && let Some(&host) = element_ids.get(&descendant.id())
                        && !elements[host].anonymous
                    {
                        let at = text.len()
                            + usize::from(spaces && !text.is_empty() && !text.ends_with('\n'));
                        pseudo_edges.open(raw.end, at, host);
                    }
                }
                for id in fragment_ids(descendant, &navigation_ids) {
                    ids.push((
                        id,
                        text.len()
                            + usize::from(spaces && !text.is_empty() && !text.ends_with('\n')),
                        element_ids[&descendant.id()],
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
                    let at = text.len();
                    text.push('\n');
                    flow_origins.hard_break(at, element_ids[&descendant.id()], &mut flow_budget);
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
                    flow_origins.image(
                        images.len(),
                        element_ids[&descendant.id()],
                        &mut flow_budget,
                    );
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
                // Only the ODT adapter can opt into this private marker. Other
                // XHTML and EPUB documents retain their established collapsing.
                let preserve_whitespace = extraction.preserve_odt_whitespace
                    && descendant.parent().is_some_and(|parent| {
                        parent.has_tag_name("span")
                            && parent.attribute("data-primitive-odt-space") == Some("1")
                    });
                let flow_element = descendant
                    .parent()
                    .and_then(|parent| element_ids.get(&parent.id()).copied())
                    .ok_or("The chapter's text has no style element")?;
                flow_origins.text_node(
                    content.starts_with(char::is_whitespace),
                    flow_element,
                    &mut flow_budget,
                );
                if let Some(legacy) = &mut legacy {
                    for c in content.chars() {
                        let old = emit(&mut legacy.text, &mut legacy.spaces, c);
                        let before = text.len();
                        let current = if preserve_whitespace {
                            emit_preserved(&mut text, &mut spaces, c)
                        } else {
                            emit(&mut text, &mut spaces, c)
                        };
                        if preserve_whitespace {
                            flow_origins.literal_character(
                                c,
                                flow_element,
                                text.len(),
                                &mut flow_budget,
                            );
                        } else {
                            flow_origins.character(
                                c,
                                flow_element,
                                before,
                                text.len(),
                                &mut flow_budget,
                            );
                        }
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
                        let before = text.len();
                        if preserve_whitespace {
                            emit_preserved(&mut text, &mut spaces, c);
                            flow_origins.literal_character(
                                c,
                                flow_element,
                                text.len(),
                                &mut flow_budget,
                            );
                        } else {
                            emit(&mut text, &mut spaces, c);
                            flow_origins.character(
                                c,
                                flow_element,
                                before,
                                text.len(),
                                &mut flow_budget,
                            );
                        }
                    });
                }
                if start < text.len() {
                    let element = descendant
                        .parent()
                        .and_then(|parent| element_ids.get(&parent.id()).copied())
                        .ok_or("The chapter's text has no style element")?;
                    if let Some(previous) = styled_text.last_mut()
                        && previous.element == element
                        && previous.preserve_whitespace == preserve_whitespace
                        && previous.bytes.end == start
                    {
                        previous.bytes.end = text.len();
                    } else {
                        styled_text.push(StyledText {
                            bytes: start..text.len(),
                            element,
                            language: languages[element],
                            hyphenation_language: languages[element]
                                .map(crate::TypographyLocale::publication),
                            preserve_whitespace,
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
                                #[cfg(feature = "kindle")]
                                note_reference: crate::footnotes::Reference::from_node(anchor),
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
                // Keep whitespace-only preformatted candidates out of the
                // canonical source vector and navigation/legacy maps.
                if crate::parley_layout::RAW_LAYOUT_ENABLED
                    && node.has_tag_name("pre")
                    && let Some(raw_whitespace) =
                        raw_whitespace::capture(node, &text, &mut raw_only_budget, &element_ids)
                {
                    raw_only_blocks.push(Block {
                        table_cell: None,
                        key,
                        origin: 0,
                        text,
                        element: element_ids[&node.id()],
                        styled_text,
                        raw_whitespace: Some(raw_whitespace),
                        flow_origins,
                        pseudo_edges: Box::default(),
                        list_item: None,
                        #[cfg(test)]
                        emphasis,
                        links,
                        images,
                        direction: base_direction(node),
                        language: languages[element_ids[&node.id()]],
                        ruby: Vec::new(),
                        legacy: None,
                    });
                }
                continue;
            }
            for image in &mut images {
                image.offset = image.offset.min(text.len());
            }
            for (id, offset, owner) in ids {
                if let std::collections::btree_map::Entry::Vacant(entry) =
                    fragments.entry(id.to_owned())
                {
                    fragment_owners.insert(id.to_owned(), owner);
                    entry.insert((key, offset.min(text.len())));
                }
            }
            if !text.is_empty() || !images.is_empty() {
                for parent in node.ancestors().skip(1) {
                    for id in fragment_ids(parent, &navigation_ids) {
                        if let std::collections::btree_map::Entry::Vacant(entry) =
                            fragments.entry(id.to_owned())
                        {
                            fragment_owners.insert(id.to_owned(), element_ids[&parent.id()]);
                            entry.insert((key, 0));
                        }
                    }
                }
            }
            let tag_end = xhtml[key..]
                .find(|c: char| c.is_ascii_whitespace() || matches!(c, '>' | '/'))
                .ok_or("The chapter has an incomplete tag")?;
            insertions.push((key + tag_end, key));
            ruby_exclusions.extend(exclusions);
            let pseudo_edges = pseudo_edges.finish(text.len(), &mut pseudo_edges_remaining);
            let raw_whitespace =
                raw_whitespace::capture(node, &text, &mut raw_whitespace_budget, &element_ids);
            blocks.push(Block {
                table_cell: tables.containing(node),
                key,
                origin: if extraction.origins {
                    node.descendants()
                        .find_map(|part| node_origins.get(&part.id()).copied())
                        .ok_or("A chapter block has no publisher origin")?
                } else {
                    0
                },
                text,
                element: element_ids[&node.id()],
                styled_text,
                raw_whitespace,
                flow_origins,
                pseudo_edges,
                list_item: list_items.block(node),
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
                            element: element_ids[&draft.annotation_node],
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
            if anonymous_boxes.is_none()
                && let Some((normalized, starts)) = wrap_inline_runs(xhtml, body)
            {
                let mut parsed = Self::parse_document(
                    &normalized,
                    original_revision,
                    Some(&starts),
                    publication_language,
                    kindle_elements,
                    extraction,
                )?;
                parsed.normalized_xhtml = Some(normalized);
                return Ok(parsed);
            }
            return Err("The chapter's inline content could not be laid out".into());
        }
        // A named anchor between paragraphs has no visible glyph of its own. Move to the
        // next rendered block, or to the last visible character for a trailing anchor.
        for anchor in body.descendants().filter(Node::is_element) {
            if !element_ids.contains_key(&anchor.id()) {
                continue;
            }
            for id in fragment_ids(anchor, &navigation_ids) {
                if fragments.contains_key(id) {
                    continue;
                }
                let next = blocks.partition_point(|block| block.key < anchor.range().start);
                let position = blocks
                    .get(next)
                    .map(|block| (block.key, 0))
                    .or_else(|| blocks.last().map(|block| (block.key, block.text.len())));
                if let Some(position) = position {
                    fragments.insert(id.into(), position);
                    fragment_owners.insert(id.into(), element_ids[&anchor.id()]);
                }
            }
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
        let mut source = Self {
            revision,
            original_input_sha256: None,
            legacy_revision,
            previous_mobi: BTreeMap::new(),
            numeric_history: Vec::new(),
            origins,
            element_origins,
            blocks,
            raw_only_blocks,
            fragments,
            fragment_owners,
            list_counters: list_items.counters,
            stylesheets,
            table_cells: tables.by_element(&element_ids),
            elements,
            insertions,
            normalized_xhtml: None,
            ruby_exclusions,
            head_end,
        };
        source.resolve_hyphenation_languages(
            publication_language.map(crate::TypographyLocale::publication),
            !extraction.ignore_embedded_language,
        );
        Ok(source)
    }

    /// The DOM element that supplied the existing destination, not the element
    /// of a later paragraph chosen for an empty anchor's canonical position.
    /// This permits CSS-hidden-target checks without changing saved tuples.
    pub(crate) fn fragment_owner(&self, id: &str) -> Option<usize> {
        self.fragment_owners.get(id).copied()
    }

    pub(crate) fn matches_input(&self, xhtml: &str) -> bool {
        self.original_input_sha256.is_some_and(|expected| {
            expected.as_slice()
                == ring::digest::digest(&ring::digest::SHA256, xhtml.as_bytes()).as_ref()
        })
    }

    /// Legacy MOBI block keys were byte offsets in the image/link-stripped XHTML.
    /// Retain them only when every historical text block matches in order; this
    /// also disambiguates repeated paragraphs without matching on quotations.
    pub(crate) fn accept_previous_mobi(&mut self, xml: &str) -> Result<()> {
        let previous = Self::parse(xml)?;
        let with_images = previous.blocks.iter().any(|block| !block.images.is_empty());
        let relevant =
            |block: &&Block| !block.text.is_empty() || (with_images && !block.images.is_empty());
        let mut current = self.blocks.iter().filter(relevant);
        let mut keys = BTreeMap::new();
        for old in previous.blocks.iter().filter(relevant) {
            let Some(now) = current.next() else {
                return Ok(());
            };
            if old.text != now.text
                || with_images
                    && !old
                        .images
                        .iter()
                        .map(|im| (&im.href, im.offset))
                        .eq(now.images.iter().map(|im| (&im.href, im.offset)))
            {
                return Ok(());
            }
            keys.insert(old.key, now.key);
        }
        if current.next().is_none() {
            for history in previous.numeric_history {
                for revision in history.revisions() {
                    self.previous_mobi.insert(revision, keys.clone());
                }
                self.numeric_history.push(history.remap(&keys));
            }
            self.previous_mobi.insert(previous.revision, keys);
        }
        Ok(())
    }
    pub(crate) fn current_key(&self, key: usize, revision: u64) -> Option<usize> {
        if let Some(keys) = self.previous_mobi.get(&revision)
            && revision != self.revision
        {
            return keys.get(&key).copied();
        }
        Some(key)
    }
    pub(crate) fn project_offset(
        &self,
        block: &Block,
        revision: u64,
        offset: usize,
    ) -> Option<usize> {
        let current = if revision == self.revision {
            offset
        } else if let Some(history) = self.numeric_history.iter().find(|h| h.accepts(revision)) {
            history.project(block, revision, offset)?
        } else if self.previous_mobi.contains_key(&revision) {
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
fn wrap_inline_runs(xhtml: &str, body: Node<'_, '_>) -> Option<(String, Vec<usize>)> {
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
    let mut starts = Vec::with_capacity(ranges.len());
    let mut cursor = 0;
    for run in ranges {
        normalized.push_str(&xhtml[cursor..run.start]);
        starts.push(normalized.len());
        normalized.push_str("<div>");
        normalized.push_str(&xhtml[run.clone()]);
        normalized.push_str("</div>");
        cursor = run.end;
    }
    normalized.push_str(&xhtml[cursor..]);
    Some((normalized, starts))
}

fn is_block(node: Node<'_, '_>) -> bool {
    node.is_element() && is_block_tag(node.tag_name().name())
}

pub(crate) fn is_block_tag(tag: &str) -> bool {
    matches!(
        tag,
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
    match language_attribute(node) {
        Some(tag) if tag.len() <= MAX_LANGUAGE_BYTES => {
            crate::TypographyLocale::parse(tag).map(crate::TypographyLocale::shaping_language)
        }
        Some(_) => None,
        None => inherited,
    }
}

fn language_attribute<'a>(node: Node<'a, '_>) -> Option<&'a str> {
    node.attribute((XML_NAMESPACE, "lang"))
        .or_else(|| node.attribute("lang"))
}

fn base_direction(node: Node<'_, '_>) -> BaseDirection {
    node.ancestors()
        .filter_map(|ancestor| ancestor.attribute("dir"))
        .find_map(parse_direction)
        .unwrap_or_default()
}

fn parse_direction(direction: &str) -> Option<BaseDirection> {
    if direction.eq_ignore_ascii_case("ltr") {
        Some(BaseDirection::Ltr)
    } else if direction.eq_ignore_ascii_case("rtl") {
        Some(BaseDirection::Rtl)
    } else if direction.eq_ignore_ascii_case("auto") {
        Some(BaseDirection::Auto)
    } else {
        None
    }
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
    fn standalone_empty_anchors_reach_visible_paragraphs_and_the_chapter_end() {
        let source = Source::parse("<html><body><p>North.</p><p id='marker'><a id='middle'/></p><a id='before'/><p>South.</p><a id='end'/></body></html>").unwrap();
        assert_eq!(source.fragments["middle"], (source.blocks[1].key, 0));
        assert_eq!(source.fragments["before"], (source.blocks[1].key, 0));
        assert_eq!(source.fragments["marker"], (source.blocks[1].key, 0));
        assert_eq!(
            source.fragments["end"],
            (source.blocks[1].key, "South.".len())
        );
    }

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
