//! Bounded analogue of CREngine's inclusive `getFinalNode` walk and note range extension.
//!
//! Source already identifies native paragraph/inline-flow owners. Publisher flow
//! removes suppressed owners; visibility affects paint, not final ownership.
//! Arbitrary CSS box reclassification and anonymous final boxes are deliberately
//! refused rather than guessed from semantic note tags or hidden ancestors.

use super::{MAX_CHARS, MAX_MARKUP};
use crate::{
    book_style::Style,
    publisher_breaks::Edge,
    source::Source,
};
use roxmltree::Node;

#[cfg(test)]
mod tests;

const MAX_ELEMENTS: usize = 100_000;
const MAX_DEPTH: usize = 64;

pub(crate) struct LayoutRoots<'a> {
    source: &'a Source,
    styles: &'a [Style],
    final_elements: Vec<bool>,
    reclassified: Vec<bool>,
    affected_subtree: Vec<bool>,
    sibling_ordinals: Vec<usize>,
}

impl<'a> LayoutRoots<'a> {
    pub(crate) fn new(source: &'a Source, styles: &'a [Style]) -> Option<Self> {
        let count = source.elements.len();
        if count > MAX_ELEMENTS || styles.len() != count {
            return None;
        }
        let mut final_elements = vec![false; count];
        let mut reclassified = vec![false; count];
        let mut affected_subtree = vec![false; count];
        let mut sibling_ordinals = vec![0usize; count];
        for (index, (element, style)) in source.elements.iter().zip(styles).enumerate() {
            if element.parent.is_some_and(|parent| parent >= index) {
                return None;
            }
            if let Some(previous) = element.previous_sibling {
                if previous >= index || source.elements[previous].parent != element.parent {
                    return None;
                }
                sibling_ordinals[index] = sibling_ordinals[previous].checked_add(1)?;
            }
            reclassified[index] = style.display.requires_reclassification(&element.tag);
            affected_subtree[index] = reclassified[index] || element.anonymous;
        }
        // A changed sibling inside the candidate can alter final/block ownership
        // even when it is not an ancestor of the selected inline target.
        for index in (0..count).rev() {
            if affected_subtree[index]
                && let Some(parent) = source.elements[index].parent
            {
                affected_subtree[parent] = true;
            }
        }
        for block in &source.blocks {
            let element = source.elements.get(block.element)?;
            let style = styles.get(block.element)?;
            if !element.anonymous && !element.selector_only && !style.flow.suppressed {
                final_elements[block.element] = true;
            }
        }
        Some(Self {
            source,
            styles,
            final_elements,
            reclassified,
            affected_subtree,
            sibling_ordinals,
        })
    }

    pub(super) fn matches(&self, xhtml: &str) -> bool {
        self.source.matches_input(xhtml)
    }

    pub(super) fn footnote_hint(
        &self,
        fragment: &str,
    ) -> Option<crate::book_style::CrFootnoteHint> {
        self.source
            .fragment_owner(fragment)
            .and_then(|index| self.styles.get(index))
            .and_then(|style| style.cr_footnote_hint)
    }

    pub(super) fn root<'d, 'i>(
        &self,
        target: Node<'d, 'i>,
        fragment: &str,
    ) -> Option<Node<'d, 'i>> {
        self.root_with_index(target, fragment).map(|(_, node)| node)
    }

    /// Add subsequent final blocks in document order, including blocks nested in
    /// new wrappers. Stop before headings, new anchors, forced breaks, or CSS
    /// boxes whose final ownership is uncertain.
    pub(super) fn extend_after<'d, 'i>(
        &self,
        target: Node<'d, 'i>,
        fragment: &str,
    ) -> Option<Vec<Node<'d, 'i>>> {
        let (element, raw) = self.root_with_index(target, fragment)?;
        if !self.final_elements[element] {
            return Some(Vec::new());
        }

        let Some(block_index) = self.source.blocks.iter().position(|block| {
            block.element == element && block.key == raw.range().start
        }) else {
            return Some(Vec::new());
        };
        let raw_elements: Vec<_> = target
            .document()
            .descendants()
            .filter(Node::is_element)
            .collect();
        let start = raw.range().start;
        let mut previous_end = raw.range().end;
        let mut text_chars = text_char_count(raw);
        if raw.range().end.saturating_sub(start) > MAX_MARKUP || text_chars > MAX_CHARS {
            return None;
        }
        let mut result = Vec::new();
        let mut stop_after = self.subtree_break_after_end(element)?;
        if stop_after.is_some_and(|end| end <= raw.range().end) {
            return Some(result);
        }
        let mut element_cursor = element + 1;
        let mut raw_cursor = raw_elements.partition_point(|node| node.range().start < previous_end);
        for block in self.source.blocks.iter().skip(block_index + 1) {
            let next_element = block.element;
            let metadata = self.source.elements.get(next_element)?;
            if metadata.raw_range.start < previous_end {
                continue;
            }
            if stop_after.is_some_and(|end| metadata.raw_range.start >= end) {
                break;
            }
            let Ok(raw_index) = raw_elements
                .binary_search_by_key(&metadata.raw_range.start, |node| node.range().start)
            else {
                break;
            };
            let next_raw = raw_elements[raw_index];
            if metadata.tag != next_raw.tag_name().name()
                || metadata.raw_range.start != block.key
                || next_raw.range().end <= previous_end
            {
                break;
            }

            let gap_end = raw_elements.partition_point(|node| {
                node.range().start < metadata.raw_range.start
            });
            if raw_elements[raw_cursor..gap_end]
                .iter()
                .any(|node| heading(*node) || has_anchor_id(*node))
            {
                break;
            }

            let mut next_stop_after = stop_after;
            let mut stop_before = false;
            for index in element_cursor..next_element {
                let element = &self.source.elements[index];
                if element.raw_range.start < previous_end
                    || element.raw_range.start >= metadata.raw_range.start
                {
                    continue;
                }
                let style = &self.styles[index];
                if self.affected_subtree[index]
                    || document_boundary_tag(&element.tag)
                    || heading_tag(&element.tag)
                    || element.id.is_some()
                    || element.tag == "a"
                        && element
                            .selector_other_attributes
                            .iter()
                            .any(|(name, _)| name == "name")
                    || style.page_breaks.before == Edge::Force
                {
                    stop_before = true;
                    break;
                }
                if style.page_breaks.after == Edge::Force {
                    if element.raw_range.end <= metadata.raw_range.start {
                        stop_before = true;
                        break;
                    }
                    next_stop_after = Some(
                        next_stop_after.map_or(element.raw_range.end, |end| {
                            end.max(element.raw_range.end)
                        }),
                    );
                }
            }
            if stop_before
                || self.affected_subtree[next_element]
                || self.subtree_has_stop_marker(next_element, next_raw)
            {
                break;
            }
            if !self.final_elements[next_element] {
                if let Some(end) = self.subtree_break_after_end(next_element)? {
                    next_stop_after = Some(next_stop_after.map_or(end, |previous| previous.max(end)));
                }
                stop_after = next_stop_after;
                element_cursor = next_element + 1;
                continue;
            }
            if next_raw.range().end.saturating_sub(start) > MAX_MARKUP {
                return None;
            }
            text_chars = text_chars.saturating_add(text_char_count(next_raw));
            if text_chars > MAX_CHARS {
                return None;
            }
            result.push(next_raw);
            element_cursor = next_element + 1;
            previous_end = next_raw.range().end;
            raw_cursor = raw_elements.partition_point(|node| node.range().start < previous_end);
            stop_after = next_stop_after;
            if let Some(end) = self.subtree_break_after_end(next_element)? {
                stop_after = Some(stop_after.map_or(end, |previous| previous.max(end)));
            }
            if stop_after.is_some_and(|end| next_raw.range().end >= end) {
                break;
            }
        }
        Some(result)
    }

    fn root_with_index<'d, 'i>(
        &self,
        target: Node<'d, 'i>,
        fragment: &str,
    ) -> Option<(usize, Node<'d, 'i>)> {
        let mut element = self.source.fragment_owner(fragment)?;
        let fragment_element = element;
        if self.source.elements.get(element)?.selector_only {
            return None;
        }
        // Canonical tuples can point an empty anchor at a following paragraph.
        // Their true element owner above is authoritative for ancestry. A loose
        // inline run inside a synthetic layout box has no raw XML slice to export.
        if let Some((key, _)) = self.source.fragments.get(fragment)
            && self
                .source
                .blocks
                .iter()
                .any(|block| block.key == *key && self.source.elements[block.element].anonymous)
        {
            return None;
        }
        let mut raw = target;
        let mut ancestors = Vec::new();
        loop {
            if ancestors.len() == MAX_DEPTH {
                return None;
            }
            let metadata = self.source.elements.get(element)?;
            // A whole ancestry check prevents duplicate IDs from binding raw
            // target1 to target2's cascade. Byte ranges need not match: numeric
            // reference compatibility can change UTF-8 lengths without changing
            // publisher element identity or sibling relationships.
            if metadata.anonymous
                || metadata.tag != raw.tag_name().name()
                || self.sibling_ordinals[element]
                    != raw.prev_siblings().skip(1).filter(Node::is_element).count()
                || self.reclassified[element]
            {
                return None;
            }
            ancestors.push((element, raw));
            match (metadata.parent, raw.parent_element()) {
                (Some(parent), Some(parent_raw)) => {
                    element = parent;
                    raw = parent_raw;
                }
                (None, None) => break,
                _ => return None,
            }
        }
        // Do not stop at a suppressed inline target/ancestor: a higher visible P
        // may still own this final block (native oracleG). If no final owner
        // exists, keep the original target (oracleF), rather than a nearest tag.
        for (element, raw) in ancestors {
            if self.final_elements[element] {
                return (!self.affected_subtree[element]).then_some((element, raw));
            }
        }
        Some((fragment_element, target))
    }

    fn subtree_range(&self, root: usize) -> Option<std::ops::Range<usize>> {
        let mut end = root + 1;
        while end < self.source.elements.len() {
            let mut parent = self.source.elements[end].parent;
            let mut descendant = false;
            let mut depth = 0;
            while let Some(index) = parent {
                depth += 1;
                if depth > MAX_DEPTH {
                    return None;
                }
                if index == root {
                    descendant = true;
                    break;
                }
                parent = self.source.elements[index].parent;
            }
            if !descendant {
                break;
            }
            end += 1;
        }
        Some(root..end)
    }

    fn subtree_has_stop_marker<'d, 'i>(&self, root: usize, raw: Node<'d, 'i>) -> bool {
        if heading(raw) || has_anchor_id(raw) {
            return true;
        }
        let Some(mut range) = self.subtree_range(root) else {
            return true;
        };
        range.any(|index| {
            let element = &self.source.elements[index];
            let style = &self.styles[index];
            document_boundary_tag(&element.tag)
                || heading_tag(&element.tag)
                || element.id.is_some()
                || element.tag == "a"
                    && element
                        .selector_other_attributes
                        .iter()
                        .any(|(name, _)| name == "name")
                || style.page_breaks.before == Edge::Force
        }) || raw.descendants().any(heading)
    }

    fn subtree_break_after_end(&self, root: usize) -> Option<Option<usize>> {
        let range = self.subtree_range(root)?;
        Some(
            range
                .filter(|&index| self.styles[index].page_breaks.after == Edge::Force)
                .map(|index| self.source.elements[index].raw_range.end)
                .max(),
        )
    }
}

fn heading(node: Node<'_, '_>) -> bool {
    heading_tag(node.tag_name().name())
}
fn document_boundary_tag(tag: &str) -> bool {
    tag.eq_ignore_ascii_case("body") || tag.eq_ignore_ascii_case("docfragment")
}

fn heading_tag(tag: &str) -> bool {
    matches!(tag, "h1" | "h2" | "h3" | "h4" | "h5" | "h6")
}

fn has_anchor_id(node: Node<'_, '_>) -> bool {
    std::iter::once(node).chain(node.descendants()).any(|node| {
        matches!(node.tag_name().name(), "body" | "DocFragment")
            || node.attribute("id").is_some()
            || node.has_tag_name("a") && node.attribute("name").is_some()
    })
}

fn text_char_count(node: Node<'_, '_>) -> usize {
    node.descendants()
        .filter(Node::is_text)
        .filter_map(|node| node.text())
        .map(|text| text.chars().count())
        .sum()
}
