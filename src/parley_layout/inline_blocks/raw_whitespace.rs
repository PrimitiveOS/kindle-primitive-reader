//! Verified raw whitespace projection for a future preformatted layout.
//!
//! This plan is intentionally not activated until pagination and persisted
//! anchors can retain raw gap positions without changing existing v1 anchors.
#![allow(dead_code)]

use super::super::{LineIndent, PaintBrush, layout_with_images};
use super::{InlineText, Segment};
use crate::{
    Settings,
    book_style::{Style, TextTransform, WhiteSpace},
    flow_projection::{Projection, SourceBinding},
    flow_view::{LayoutBlock, StyledRuns},
    selection::RawTextSelection,
    source::raw_whitespace::{AnchorContext, CaretAffinity, RawAnchor, RawCaret},
    source::{Block, StyledText},
    user_hyphenation::Rules,
};
use parley::{
    Affinity, BoundingBox, Cluster, Cursor, FontContext, Layout, LayoutContext, Selection,
};
use std::ops::Range;

const MAX_RAW_LINES: usize = 4096;
const MAX_SELECTION_RECTS: usize = 4096;

pub(in crate::parley_layout) struct Plan {
    pub(in crate::parley_layout) text: InlineText,
    pub(in crate::parley_layout) runs: Vec<StyledText>,
    binding: SourceBinding,
    block: usize,
    owner: usize,
    white_space: WhiteSpace,
}

pub(in crate::parley_layout) struct RawLine {
    pub(in crate::parley_layout) bytes: Range<usize>,
    pub(in crate::parley_layout) start: RawCaret,
    pub(in crate::parley_layout) end: RawCaret,
    pub(in crate::parley_layout) top: f32,
    pub(in crate::parley_layout) bottom: f32,
}

pub(in crate::parley_layout) struct Prepared {
    pub(in crate::parley_layout) text: InlineText,
    pub(in crate::parley_layout) layout: Layout<PaintBrush>,
    pub(in crate::parley_layout) lines: Vec<RawLine>,
    binding: SourceBinding,
}

impl Plan {
    pub(in crate::parley_layout) fn new(
        block: &LayoutBlock<'_>,
        view: &Projection<'_>,
        styles: &[Style],
    ) -> Option<Self> {
        if !view.is_identity()
            || !matches!(&block.styled_text, StyledRuns::Original(_))
            || !block.images.is_empty()
            || !block.ruby.is_empty()
            || !block.generated.is_empty()
            || !block.bidi_controls.is_empty()
            || block.first_letter.is_some()
            || styles.get(block.element)?.first_line.is_some()
            || styles.get(block.element)?.generated.is_some()
            || styles.get(block.element)?.text_transform != TextTransform::None
            || !styles.get(block.element)?.preformatted_spaces
            || !matches!(
                styles.get(block.element)?.white_space,
                WhiteSpace::Pre | WhiteSpace::PreWrap | WhiteSpace::BreakSpaces
            )
        {
            return None;
        }
        let [original] = &*block.styled_text else {
            return None;
        };
        if original.element != block.element || original.bytes != (0..block.text.len()) {
            return None;
        }
        let bound = block.raw_whitespace.as_ref()?.bind(block.text)?;
        let raw = bound.raw();
        let mut segments = Vec::new();
        for span in bound.text_spans() {
            segments.push(Segment {
                source: span.canonical.clone(),
                offset: span.raw.start,
            });
        }
        for gap in bound.gaps() {
            if gap.canonical.is_empty() {
                continue;
            }
            // An old normalized space needs an identical raw byte. If none is
            // available, existing v1 anchors cannot be safely projected yet.
            if block.text.get(gap.canonical.clone())? != " " {
                return None;
            }
            let raw_gap = raw.get(gap.raw.clone())?;
            let space = raw_gap.bytes().position(|byte| byte == b' ')?;
            segments.push(Segment {
                source: gap.canonical.clone(),
                offset: gap.raw.start.checked_add(space)?,
            });
        }
        segments.sort_by_key(|segment| segment.source.start);
        let mut source_end = 0;
        let mut raw_end = 0;
        for segment in &segments {
            let length = segment.source.len();
            let end = segment.offset.checked_add(length)?;
            if segment.source.start != source_end
                || segment.offset < raw_end
                || raw.get(segment.offset..end)? != block.text.get(segment.source.clone())?
            {
                return None;
            }
            source_end = segment.source.end;
            raw_end = end;
        }
        if source_end != block.text.len() {
            return None;
        }
        Some(Self {
            text: InlineText {
                text: raw.to_owned(),
                segments,
                boxes: Vec::new(),
                generated: true,
                detached_initial: false,
            },
            runs: vec![StyledText {
                bytes: 0..raw.len(),
                element: block.element,
                language: original.language,
                hyphenation_language: original.hyphenation_language,
                preserve_whitespace: original.preserve_whitespace,
            }],
            binding: SourceBinding::new(block),
            block: block.key,
            owner: block.element,
            white_space: styles.get(block.element)?.white_space,
        })
    }

    /// Shape a detached candidate and verify every line before any future
    /// caller may choose to install it in a chapter.
    pub(in crate::parley_layout) fn shape(
        mut self,
        context: &mut LayoutContext<PaintBrush>,
        fonts: &mut FontContext,
        block: &LayoutBlock<'_>,
        width: u32,
        settings: Settings,
        styles: &[Style],
        indent: LineIndent,
        rules: &Rules,
    ) -> Option<Prepared> {
        let bound = block.raw_whitespace.as_ref()?.bind(block.text)?;
        let style = styles.get(block.element)?;
        if width == 0
            || !self.binding.matches(block)
            || block.key != self.block
            || block.element != self.owner
            || !style.preformatted_spaces
            || style.white_space != self.white_space
            || style.generated.is_some()
            || style.text_transform != TextTransform::None
            || bound.raw() != self.text.text.as_str()
        {
            return None;
        }
        let fragment = block.text_fragment(
            block.element,
            &self.text.text,
            std::mem::take(&mut self.runs),
        );
        let (layout, ruby) = layout_with_images(
            context,
            fonts,
            &fragment,
            width,
            settings,
            styles,
            &[],
            indent,
            rules,
            None,
        );
        if !ruby.is_empty()
            || layout.text_len() != bound.raw().len()
            || layout.len() > MAX_RAW_LINES
        {
            return None;
        }
        let mut ranges = Vec::with_capacity(layout.len());
        let mut requests = Vec::with_capacity(layout.len() * 2);
        let mut previous_end = 0;
        let mut previous_start = None;
        let mut previous_top = None;
        for line in layout.lines() {
            let range = line.text_range();
            let metrics = line.metrics();
            if range.start != previous_end
                || range.end < range.start
                || range.end > bound.raw().len()
                || previous_start == Some(range.start)
                || !metrics.block_min_coord.is_finite()
                || !metrics.block_max_coord.is_finite()
                || previous_top.is_some_and(|top| metrics.block_min_coord < top)
                || metrics.block_max_coord < metrics.block_min_coord
                || metrics.block_max_coord - metrics.block_min_coord > settings.text_height() as f32
            {
                return None;
            }
            previous_start = Some(range.start);
            previous_top = Some(metrics.block_min_coord);
            previous_end = range.end;
            requests.push((range.start, CaretAffinity::After));
            requests.push((range.end, CaretAffinity::Before));
            ranges.push((range, metrics.block_min_coord, metrics.block_max_coord));
        }
        if previous_end != bound.raw().len() {
            return None;
        }
        let mut carets = bound.carets_at(&requests)?.into_iter();
        let mut lines = Vec::with_capacity(ranges.len());
        for (bytes, top, bottom) in ranges {
            lines.push(RawLine {
                bytes,
                start: carets.next()?,
                end: carets.next()?,
                top,
                bottom,
            });
        }
        Some(Prepared {
            text: self.text,
            layout,
            lines,
            binding: self.binding,
        })
    }
}

impl Prepared {
    /// Resolve a point in paragraph-local layout coordinates without snapping
    /// extra whitespace back to its collapsed canonical position. Empty space
    /// without a shaped cluster does not produce a caret.
    pub(in crate::parley_layout) fn anchor_at_point(
        &self,
        block: &Block,
        context: &AnchorContext<'_>,
        x: f32,
        y: f32,
    ) -> Option<RawAnchor> {
        if !self.binding.matches(block)
            || block.key != context.block
            || !x.is_finite()
            || !y.is_finite()
            || !(0.0..self.layout.height()).contains(&y)
        {
            return None;
        }
        let bound = block.raw_whitespace.as_ref()?.bind(&block.text)?;
        if self.text.text.as_str() != bound.raw() || self.layout.text_len() != bound.raw().len() {
            return None;
        }
        Cluster::from_point_exact(&self.layout, x, y)?;
        let cursor = Cursor::from_point(&self.layout, x, y);
        let affinity = match cursor.affinity() {
            Affinity::Upstream => CaretAffinity::Before,
            Affinity::Downstream => CaretAffinity::After,
        };
        let caret = bound.caret_at(cursor.index(), affinity)?;
        let anchor = RawAnchor {
            canonical: crate::Anchor {
                book: context.book,
                chapter: context.chapter.to_owned(),
                revision: context.revision,
                block: context.block,
                offset: caret.canonical_offset,
                illustration: None,
            },
            caret,
        };
        bound.validate_raw_anchor(&anchor, context)?;
        Some(anchor)
    }

    /// Resolve the first intersecting line, including whitespace-only lines.
    /// `page` is in paragraph-local block coordinates. The caller supplies
    /// the identity of the verified, currently open publication.
    pub(in crate::parley_layout) fn anchor_in_page(
        &self,
        block: &Block,
        context: &AnchorContext<'_>,
        page: Range<f32>,
    ) -> Option<RawAnchor> {
        if !self.binding.matches(block)
            || block.key != context.block
            || !page.start.is_finite()
            || !page.end.is_finite()
            || page.start >= page.end
        {
            return None;
        }
        let bound = block.raw_whitespace.as_ref()?.bind(&block.text)?;
        if self.text.text.as_str() != bound.raw() || self.layout.text_len() != bound.raw().len() {
            return None;
        }
        let line = self
            .lines
            .iter()
            .find(|line| line.bottom > page.start && line.top < page.end)?;
        let anchor = RawAnchor {
            canonical: crate::Anchor {
                book: context.book,
                chapter: context.chapter.to_owned(),
                revision: context.revision,
                block: context.block,
                offset: line.start.canonical_offset,
                illustration: None,
            },
            caret: line.start.clone(),
        };
        bound.validate_raw_anchor(&anchor, context)?;
        Some(anchor)
    }

    /// Map a verified raw anchor back to a line. Affinity selects the prior
    /// or next line at a shared boundary without changing the source offset.
    pub(in crate::parley_layout) fn line_for_anchor(
        &self,
        block: &Block,
        context: &AnchorContext<'_>,
        anchor: &RawAnchor,
    ) -> Option<usize> {
        if !self.binding.matches(block) || block.key != context.block {
            return None;
        }
        let bound = block.raw_whitespace.as_ref()?.bind(&block.text)?;
        if self.text.text.as_str() != bound.raw() || self.layout.text_len() != bound.raw().len() {
            return None;
        }
        bound.validate_raw_anchor(anchor, context)?;
        let offset = anchor.caret.raw_offset;
        for (index, line) in self.lines.iter().enumerate() {
            if offset > line.bytes.start && offset < line.bytes.end {
                return Some(index);
            }
            if offset == line.bytes.start {
                return Some(
                    if anchor.caret.affinity == CaretAffinity::Before && index > 0 {
                        index - 1
                    } else {
                        index
                    },
                );
            }
            if offset == line.bytes.end {
                return Some(
                    if anchor.caret.affinity == CaretAffinity::After
                        && self
                            .lines
                            .get(index + 1)
                            .is_some_and(|next| next.bytes.start == offset)
                    {
                        index + 1
                    } else {
                        index
                    },
                );
            }
        }
        None
    }

    /// Compute raw selection geometry only after revalidating both anchors and
    /// the exact text/quote against the bound source snapshot.
    pub(in crate::parley_layout) fn selection_geometry(
        &self,
        block: &Block,
        context: &AnchorContext<'_>,
        selection: &RawTextSelection,
    ) -> Option<Vec<(usize, BoundingBox)>> {
        if !self.binding.matches(block) || block.key != context.block {
            return None;
        }
        let bound = block.raw_whitespace.as_ref()?.bind(&block.text)?;
        bound.validate_raw_anchor(&selection.start, context)?;
        bound.validate_raw_anchor(&selection.end, context)?;
        let start = selection.start.caret.raw_offset;
        let end = selection.end.caret.raw_offset;
        if start >= end
            || self.text.text.as_str() != bound.raw()
            || self.layout.text_len() != bound.raw().len()
            || bound.raw().get(start..end)? != selection.text.as_str()
        {
            return None;
        }
        let first = Cursor::from_byte_index(&self.layout, start, Affinity::Downstream);
        let last = Cursor::from_byte_index(&self.layout, end, Affinity::Upstream);
        if first.index() != start || last.index() != end {
            return None;
        }
        let mut result = Vec::new();
        let mut invalid = false;
        Selection::new(first, last).geometry_with(&self.layout, |bounds, line| {
            if result.len() >= MAX_SELECTION_RECTS
                || !bounds.x0.is_finite()
                || !bounds.x1.is_finite()
                || !bounds.y0.is_finite()
                || !bounds.y1.is_finite()
                || bounds.x1 < bounds.x0
                || bounds.y1 < bounds.y0
            {
                invalid = true;
            } else {
                result.push((line, bounds));
            }
        });
        (!invalid && !result.is_empty()).then_some(result)
    }
}
