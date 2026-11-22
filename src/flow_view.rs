//! One rendered-text view shared by layout, selection and reflow search.
//!
//! Canonical source and saved anchors never acquire projected offsets. The
//! default view is empty metadata; only blocks affected by a computed display
//! or paint mask retain an owned projection. This module does not enable user
//! display/visibility syntax or a hiding preset.
#![allow(dead_code)] // Consumer migration is staged while the KO reference builds.

use std::collections::BTreeMap;

use crate::{
    flow_projection::{ElementFlow, OwnedProjection, Projection, ProjectionError, SourceBinding},
    source::{Block, Source},
};

#[derive(Default)]
pub(crate) struct ChapterFlow {
    source_revision: u64,
    binding: Option<SourceBinding>,
    masks: Vec<ElementFlow>,
    blocks: BTreeMap<usize, OwnedProjection>,
}

impl ChapterFlow {
    pub(crate) fn new(source: &Source, masks: Vec<ElementFlow>) -> crate::Result<Self> {
        if masks.len() != source.elements.len() {
            return Err("The rendered text mask does not match its source".into());
        }
        if masks.iter().all(|mask| *mask == ElementFlow::default()) {
            return Ok(Self::default());
        }
        let mut blocks = BTreeMap::new();
        for block in &source.blocks {
            if let Some(projected) = Projection::new(block, &masks)
                .map_err(|e| e.to_string())?
                .into_owned()
            {
                blocks.insert(block.key, projected);
            }
        }
        Ok(Self {
            source_revision: source.revision,
            binding: source.blocks.first().map(SourceBinding::new),
            masks,
            blocks,
        })
    }

    pub(crate) fn is_identity(&self) -> bool {
        self.masks.is_empty()
    }

    /// A cache key must separately include book identity, source revision,
    /// publisher-style policy and the immutable effective user-style digest.
    /// Source binding is necessary but does not make a stale cascade current.
    pub(crate) fn view<'a>(
        &'a self,
        source: &'a Source,
        block: &'a Block,
    ) -> Result<Projection<'a>, ProjectionError> {
        if self.is_identity() {
            return Ok(Projection::identity(block));
        }
        if source.revision != self.source_revision
            || self
                .binding
                .as_ref()
                .is_none_or(|binding| !binding.matches(block))
        {
            return Err(ProjectionError::WrongBlock);
        }
        match self.blocks.get(&block.key) {
            Some(projected) => projected.bind(block),
            None => Ok(Projection::identity(block)),
        }
    }

    pub(crate) fn element(&self, index: usize) -> Result<ElementFlow, ProjectionError> {
        if self.is_identity() {
            return Ok(ElementFlow::default());
        }
        self.masks
            .get(index)
            .copied()
            .ok_or(ProjectionError::MissingElement(index))
    }

    pub(crate) fn has_suppression(&self) -> bool {
        self.masks.iter().any(|mask| mask.suppressed)
    }

    pub(crate) fn owned(&self, key: usize) -> Option<&OwnedProjection> {
        self.blocks.get(&key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source() -> Source {
        Source::parse("<html><body><p id='before'>Before.</p><p>A<span id='cut'>TRAP</span>B</p><p id='after'>After.</p></body></html>").unwrap()
    }

    #[test]
    fn chapter_view_keeps_unaffected_blocks_borrowed_and_never_copies_canonical_text() {
        let source = source();
        let mut masks = vec![ElementFlow::default(); source.elements.len()];
        let cut = source
            .elements
            .iter()
            .position(|element| element.id.as_deref() == Some("cut"))
            .unwrap();
        masks[cut].suppressed = true;
        let flow = ChapterFlow::new(&source, masks).unwrap();
        assert_eq!(flow.blocks.len(), 1);
        assert!(flow.has_suppression());
        for index in [0, 2] {
            let view = flow.view(&source, &source.blocks[index]).unwrap();
            assert!(view.is_identity());
            assert_eq!(view.text().as_ptr(), source.blocks[index].text.as_ptr());
        }
        assert_eq!(flow.view(&source, &source.blocks[1]).unwrap().text(), "AB");
        assert_eq!(source.blocks[1].text, "ATRAPB");
    }

    #[test]
    fn changed_chapter_rejects_wrong_source_even_for_an_unaffected_block() {
        let source = source();
        let mut masks = vec![ElementFlow::default(); source.elements.len()];
        masks[source.blocks[1].element].painted = false;
        let flow = ChapterFlow::new(&source, masks).unwrap();
        let another = self::source();
        assert!(matches!(
            flow.view(&another, &another.blocks[0]),
            Err(ProjectionError::WrongBlock)
        ));
    }

    #[test]
    fn default_chapter_has_no_retained_masks_projections_or_source_lifetime() {
        let source = source();
        let flow =
            ChapterFlow::new(&source, vec![ElementFlow::default(); source.elements.len()]).unwrap();
        assert!(flow.is_identity());
        assert!(flow.blocks.is_empty() && flow.binding.is_none());
        assert!(ChapterFlow::new(&source, Vec::new()).is_err());
    }
}
