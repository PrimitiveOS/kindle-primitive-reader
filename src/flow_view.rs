//! One rendered-text view shared by layout, selection and reflow search.
//!
//! Canonical source and saved anchors never acquire projected offsets. The
//! default view is empty metadata; only blocks affected by a computed display
//! or paint mask retain an owned projection. This module does not enable user
//! display/visibility syntax or a hiding preset.
#![allow(dead_code)] // Consumer migration is staged while the KO reference builds.

use std::{
    collections::BTreeMap,
    ops::{Deref, Range},
    sync::Arc,
};

use crate::{
    flow_projection::{ElementFlow, OwnedProjection, Projection, ProjectionError, SourceBinding},
    source::{Block, EmbeddedImage, Ruby, Source, StyledText},
};

#[derive(Default)]
pub(crate) struct ChapterFlow {
    source_revision: u64,
    binding: Option<SourceBinding>,
    masks: Vec<ElementFlow>,
    blocks: BTreeMap<usize, Arc<OwnedProjection>>,
    ordinals: BTreeMap<usize, Option<i32>>,
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
                blocks.insert(block.key, Arc::new(projected));
            }
        }
        Ok(Self {
            source_revision: source.revision,
            binding: source.blocks.first().map(SourceBinding::new),
            masks,
            blocks,
            ordinals: BTreeMap::new(),
        })
    }

    pub(crate) fn from_styles(
        source: &Source,
        styles: &[crate::book_style::Style],
    ) -> crate::Result<Self> {
        let mut flow = Self::new(source, styles.iter().map(|style| style.flow).collect())?;
        for group in &source.list_counters {
            let children: Vec<_> = group
                .children
                .iter()
                .map(|child| crate::book_style::flow::ListChild {
                    element: child.element,
                    value: child.value,
                })
                .collect();
            let values = crate::book_style::flow::list_ordinals(
                styles,
                &children,
                group.start,
                group.reversed,
            )?;
            for (child, value) in group.children.iter().zip(values) {
                flow.ordinals.insert(child.element, value);
            }
        }
        Ok(flow)
    }

    pub(crate) fn ordinal(&self, item: crate::list_marker::ListItem) -> Option<i32> {
        self.ordinals
            .get(&item.element)
            .copied()
            .unwrap_or(Some(item.ordinal))
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
        self.blocks.get(&key).map(Arc::as_ref)
    }

    pub(crate) fn shared(&self, key: usize) -> Option<Arc<OwnedProjection>> {
        self.blocks.get(&key).cloned()
    }

    pub(crate) fn layout_block<'a>(
        &'a self,
        source: &'a Source,
        block: &'a Block,
    ) -> crate::Result<LayoutBlock<'a>> {
        let view = self
            .view(source, block)
            .map_err(|error| error.to_string())?;
        let projected = self.owned(block.key);
        let text = projected.map_or(block.text.as_str(), OwnedProjection::text);
        let styled_text = match projected {
            None => StyledRuns::Original(&block.styled_text),
            Some(projected) => StyledRuns::Projected(
                projected
                    .runs()
                    .iter()
                    .map(|run| {
                        let language = block
                            .styled_text
                            .iter()
                            .find(|original| original.element == run.element)
                            .and_then(|original| original.language)
                            .or(block.language);
                        StyledText {
                            bytes: run.layout.clone(),
                            element: run.element,
                            language,
                        }
                    })
                    .collect(),
            ),
        };
        let mut ruby = Vec::new();
        if projected.is_some() {
            for annotation in &block.ruby {
                if self.element(annotation.element)?.suppressed {
                    continue;
                }
                let ranges = view
                    .layout_spans(annotation.bytes.clone())
                    .ok_or("A ruby base has invalid source boundaries")?;
                let Some(first) = ranges.first() else {
                    continue;
                };
                let last = ranges.last().unwrap();
                ruby.push(LayoutRuby {
                    bytes: first.start..last.end,
                    source: annotation,
                    painted: self
                        .element(annotation.element)
                        .map_err(|error| error.to_string())?
                        .painted,
                });
            }
        }
        Ok(LayoutBlock {
            source: block,
            element: block.element,
            text,
            styled_text,
            images: Images {
                source: &block.images,
                projected: projected.map(OwnedProjection::images),
                masks: &self.masks,
            },
            ruby: Rubies {
                source: &block.ruby,
                projected: projected.is_some().then_some(ruby),
                masks: &self.masks,
            },
        })
    }
}

/// A transient layout input that shadows text/ranges without modifying Block.
/// Source-only consumers must continue to use the original canonical Block.
pub(crate) struct LayoutBlock<'a> {
    source: &'a Block,
    pub element: usize,
    pub text: &'a str,
    pub styled_text: StyledRuns<'a>,
    pub images: Images<'a>,
    pub ruby: Rubies<'a>,
}

impl Deref for LayoutBlock<'_> {
    type Target = Block;
    fn deref(&self) -> &Self::Target {
        self.source
    }
}

impl<'a> LayoutBlock<'a> {
    pub(crate) fn identity(source: &'a Block) -> Self {
        Self {
            source,
            element: source.element,
            text: &source.text,
            styled_text: StyledRuns::Original(&source.styled_text),
            images: Images {
                source: &source.images,
                projected: None,
                masks: &[],
            },
            ruby: Rubies {
                source: &source.ruby,
                projected: None,
                masks: &[],
            },
        }
    }
    pub(crate) fn text_fragment<'b>(
        &self,
        element: usize,
        text: &'b str,
        runs: Vec<StyledText>,
    ) -> LayoutBlock<'b>
    where
        'a: 'b,
    {
        LayoutBlock {
            source: self.source,
            element,
            text,
            styled_text: StyledRuns::Projected(runs),
            images: Images {
                source: &[],
                projected: None,
                masks: &[],
            },
            ruby: Rubies {
                source: &[],
                projected: None,
                masks: &[],
            },
        }
    }
}

pub(crate) enum StyledRuns<'a> {
    Original(&'a [StyledText]),
    Projected(Vec<StyledText>),
}

impl Deref for StyledRuns<'_> {
    type Target = [StyledText];
    fn deref(&self) -> &Self::Target {
        match self {
            Self::Original(runs) => runs,
            Self::Projected(runs) => runs,
        }
    }
}

pub(crate) struct Images<'a> {
    source: &'a [EmbeddedImage],
    projected: Option<&'a [crate::flow_projection::Image]>,
    masks: &'a [ElementFlow],
}

#[derive(Clone, Copy)]
pub(crate) struct LayoutImage<'a> {
    pub source_index: usize,
    source: &'a EmbeddedImage,
    pub offset: usize,
    pub space_after: bool,
    pub painted: bool,
}

impl Deref for LayoutImage<'_> {
    type Target = EmbeddedImage;
    fn deref(&self) -> &Self::Target {
        self.source
    }
}

impl<'a> LayoutImage<'a> {
    pub(crate) fn original(&self) -> &'a EmbeddedImage {
        self.source
    }
}

impl<'a> Images<'a> {
    pub(crate) fn len(&self) -> usize {
        self.projected.map_or(self.source.len(), <[_]>::len)
    }
    pub(crate) fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub(crate) fn iter(&self) -> impl Iterator<Item = LayoutImage<'a>> + '_ {
        (0..self.len()).map(|index| {
            let projected = self.projected.map(|images| &images[index]);
            let source_index = projected.map_or(index, |image| image.source_index);
            let source = &self.source[source_index];
            LayoutImage {
                source_index,
                source,
                offset: projected.map_or(source.offset, |image| image.offset),
                space_after: projected.map_or(source.space_after, |image| image.space_after),
                painted: projected.map_or_else(
                    || {
                        self.masks
                            .get(source.element)
                            .is_none_or(|flow| flow.painted)
                    },
                    |image| image.painted,
                ),
            }
        })
    }
}

pub(crate) struct LayoutRuby<'a> {
    pub bytes: Range<usize>,
    source: &'a Ruby,
    pub painted: bool,
}

impl Deref for LayoutRuby<'_> {
    type Target = Ruby;
    fn deref(&self) -> &Self::Target {
        self.source
    }
}

pub(crate) struct Rubies<'a> {
    source: &'a [Ruby],
    projected: Option<Vec<LayoutRuby<'a>>>,
    masks: &'a [ElementFlow],
}

impl Rubies<'_> {
    pub(crate) fn is_empty(&self) -> bool {
        self.iter().next().is_none()
    }
    pub(crate) fn len(&self) -> usize {
        self.iter().count()
    }
    pub(crate) fn iter(&self) -> impl Iterator<Item = LayoutRuby<'_>> + '_ {
        let count = self.projected.as_ref().map_or(self.source.len(), Vec::len);
        (0..count).filter_map(|index| {
            let owner = self
                .projected
                .as_ref()
                .map_or(self.source[index].element, |items| items[index].element);
            if self.masks.get(owner).is_some_and(|flow| flow.suppressed) {
                return None;
            }
            Some(match &self.projected {
                Some(ruby) => LayoutRuby {
                    bytes: ruby[index].bytes.clone(),
                    source: ruby[index].source,
                    painted: ruby[index].painted,
                },
                None => {
                    let source = &self.source[index];
                    LayoutRuby {
                        bytes: source.bytes.clone(),
                        source,
                        painted: self
                            .masks
                            .get(source.element)
                            .is_none_or(|flow| flow.painted),
                    }
                }
            })
        })
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

    #[test]
    fn layout_input_maps_text_ruby_and_image_slots_without_mutating_source() {
        let source = Source::parse("<html><body><p>A<span id='cut'>TRAP</span><ruby>日<rt id='note'>にち</rt></ruby><img id='gone' src='one'/><img src='two'/>B</p></body></html>").unwrap();
        let mut masks = vec![ElementFlow::default(); source.elements.len()];
        for (index, element) in source.elements.iter().enumerate() {
            if matches!(element.id.as_deref(), Some("cut" | "gone")) {
                masks[index].suppressed = true;
            }
            if element.id.as_deref() == Some("note") {
                masks[index].painted = false;
            }
        }
        let flow = ChapterFlow::new(&source, masks).unwrap();
        let input = flow.layout_block(&source, &source.blocks[0]).unwrap();
        assert_eq!(input.text, "A日B");
        assert_eq!(source.blocks[0].text, "ATRAP日B");
        let images: Vec<_> = input.images.iter().collect();
        assert_eq!(images.len(), 1);
        assert_eq!(images[0].source_index, 1);
        assert_eq!(images[0].offset, "A日".len());
        let ruby: Vec<_> = input.ruby.iter().collect();
        assert_eq!(ruby.len(), 1);
        assert_eq!(&input.text[ruby[0].bytes.clone()], "日");
        assert!(!ruby[0].painted);
        assert_eq!(ruby[0].annotation, "にち");
    }

    #[test]
    fn identity_layout_input_borrows_text_runs_and_original_media() {
        let source = Source::parse(
            "<html><body><p>Word <ruby>日<rt>にち</rt></ruby><img src='photo'/></p></body></html>",
        )
        .unwrap();
        let flow = ChapterFlow::default();
        let block = &source.blocks[0];
        let input = flow.layout_block(&source, block).unwrap();
        assert_eq!(input.text.as_ptr(), block.text.as_ptr());
        assert_eq!(input.styled_text.as_ptr(), block.styled_text.as_ptr());
        assert_eq!(input.images.iter().next().unwrap().source_index, 0);
        assert_eq!(input.ruby.iter().next().unwrap().bytes, block.ruby[0].bytes);
    }

    #[test]
    fn projected_hyphenation_uses_joined_visible_word_and_projected_style_ranges() {
        let source = Source::parse(
            "<html><body><p lang='en'>rep<span id='cut'>TRAP</span>resentation</p></body></html>",
        )
        .unwrap();
        let mut masks = vec![ElementFlow::default(); source.elements.len()];
        masks[source
            .elements
            .iter()
            .position(|element| element.id.as_deref() == Some("cut"))
            .unwrap()]
        .suppressed = true;
        let flow = ChapterFlow::new(&source, masks).unwrap();
        let input = flow.layout_block(&source, &source.blocks[0]).unwrap();
        let plain =
            Source::parse("<html><body><p lang='en'>representation</p></body></html>").unwrap();
        let styles = vec![crate::book_style::Style::default(); source.elements.len()];
        let settings = crate::Settings {
            automatic_hyphenation: true,
            disable_soft_hyphens: false,
            ..Default::default()
        };
        let expected = crate::hyphenation::opportunities(&plain.blocks[0], &styles, settings);
        assert!(!expected.is_empty());
        assert_eq!(
            crate::hyphenation::opportunities_text(
                input.text,
                &input.styled_text,
                &styles,
                settings
            ),
            expected
        );
        assert_eq!(input.text, "representation");
    }
}
