//! Bounded analogue of CREngine's inclusive `getFinalNode` ancestor walk.
//!
//! Source already identifies native paragraph/inline-flow owners. Publisher flow
//! removes suppressed owners; visibility affects paint, not final ownership.
//! Arbitrary CSS box reclassification and anonymous final boxes are deliberately
//! refused rather than guessed from semantic note tags or hidden ancestors.

use crate::{book_style::Style, source::Source};
use roxmltree::Node;

#[cfg(test)]
mod tests;

const MAX_ELEMENTS: usize = 100_000;
const MAX_DEPTH: usize = 64;

pub(crate) struct LayoutRoots<'a> {
    source: &'a Source,
    final_elements: Vec<bool>,
    reclassified: Vec<bool>,
    affected_subtree: Vec<bool>,
    sibling_ordinals: Vec<usize>,
}

impl<'a> LayoutRoots<'a> {
    pub(crate) fn new(source: &'a Source, styles: &[Style]) -> Option<Self> {
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
            final_elements,
            reclassified,
            affected_subtree,
            sibling_ordinals,
        })
    }

    pub(super) fn matches(&self, xhtml: &str) -> bool {
        self.source.matches_input(xhtml)
    }

    pub(super) fn root<'d, 'i>(
        &self,
        target: Node<'d, 'i>,
        fragment: &str,
    ) -> Option<Node<'d, 'i>> {
        let mut element = self.source.fragment_owner(fragment)?;
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
                return (!self.affected_subtree[element]).then_some(raw);
            }
        }
        Some(target)
    }
}
