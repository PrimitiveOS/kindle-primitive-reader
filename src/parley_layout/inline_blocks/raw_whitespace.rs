//! Bounded, source-verified preformatted layout and raw caret projection.
//!
//! Unsupported projections fall back to the established canonical layout.
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
// The bounded source-mapped collapsing projection is active locally;
// unsupported or ambiguous cases retain canonical layout.
const EXTENDED_PRE_LINE_ENABLED: bool = true;

pub(in crate::parley_layout) struct Plan {
    pub(in crate::parley_layout) text: InlineText,
    pub(in crate::parley_layout) runs: Vec<StyledText>,
    binding: SourceBinding,
    block: usize,
    owner: usize,
    white_space: WhiteSpace,
    pre_line_projection:
        Option<std::sync::Arc<crate::source::pre_line_projection::PreLineProjection>>,
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
    pre_line_projection:
        Option<std::sync::Arc<crate::source::pre_line_projection::PreLineProjection>>,
}

fn supported_raw_mode(style: &Style) -> bool {
    match style.white_space {
        WhiteSpace::Pre | WhiteSpace::PreWrap | WhiteSpace::BreakSpaces => {
            style.preformatted_spaces
        }
        WhiteSpace::PreLine => !style.preformatted_spaces,
        WhiteSpace::Normal | WhiteSpace::NoWrap => false,
    }
}

/// Only install pre-line when its authored text is already equal to the
/// displayed whitespace. More complex collapsing needs its own projection.
fn pre_line_identity(raw: &str) -> bool {
    !raw.ends_with('\n')
        && !raw.chars().any(|ch| matches!(ch, '\t' | '\r' | '\u{c}'))
        && raw
            .split('\n')
            .all(|line| !line.starts_with(' ') && !line.ends_with(' ') && !line.contains("  "))
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
            || !supported_raw_mode(styles.get(block.element)?)
        {
            return None;
        }
        let originals = &*block.styled_text;
        if (originals.is_empty() && !block.text.chars().all(char::is_whitespace))
            || originals.iter().any(|run| {
                run.bytes.start >= run.bytes.end
                    || run.bytes.end > block.text.len()
                    || !block.text.is_char_boundary(run.bytes.start)
                    || !block.text.is_char_boundary(run.bytes.end)
            })
            || originals
                .windows(2)
                .any(|pair| pair[0].bytes.end > pair[1].bytes.start)
        {
            return None;
        }
        let owner_style = styles.get(block.element)?;
        for run in originals {
            let style = styles.get(run.element)?;
            if style.flow.suppressed
                || !style.flow.painted
                || (run.element != block.element
                    && style.display != crate::book_style::flow::Display::Inline)
                || !supported_raw_mode(style)
                || style.white_space != owner_style.white_space
                || style.text_transform != TextTransform::None
                || style.generated.is_some()
                || style.first_letter.is_some()
                || style.first_line.is_some()
            {
                return None;
            }
        }
        let bound = block.raw_whitespace.as_ref()?.bind(block.text)?;
        for &element in bound.elements() {
            let style = styles.get(element)?;
            if style.flow.suppressed
                || !style.flow.painted
                || style.display != crate::book_style::flow::Display::Inline
                || style.white_space != owner_style.white_space
                || style.writing_mode != owner_style.writing_mode
                || style.direction != owner_style.direction
                || style.text_transform != TextTransform::None
                || style.generated.is_some()
                || style.first_letter.is_some()
                || style.first_line.is_some()
            {
                return None;
            }
        }
        let raw = bound.raw();
        if raw.contains('\u{c}') {
            return None;
        }
        let pre_line_projection = if owner_style.white_space == WhiteSpace::PreLine
            && !pre_line_identity(raw)
        {
            if !EXTENDED_PRE_LINE_ENABLED {
                return None;
            }
            let projection = crate::source::pre_line_projection::PreLineProjection::build(
                raw,
                bound.hard_breaks(),
            )?;
            if projection.text() == block.text {
                return None;
            }
            Some(std::sync::Arc::new(projection))
        } else if owner_style.white_space != WhiteSpace::PreLine && raw.contains('\r') {
            let projection = crate::source::pre_line_projection::PreLineProjection::preserved(raw)?;
            if projection.text() == block.text {
                return None;
            }
            Some(std::sync::Arc::new(projection))
        } else {
            None
        };
        let display = pre_line_projection
            .as_ref()
            .map_or(raw, |projection| projection.text());
        let mut runs = Vec::with_capacity(bound.style_runs().len());
        let mut previous_raw = 0;
        for raw_run in bound.style_runs() {
            if raw_run.raw.start != previous_raw
                || raw_run.raw.start >= raw_run.raw.end
                || !raw.is_char_boundary(raw_run.raw.start)
                || !raw.is_char_boundary(raw_run.raw.end)
                || raw_run.raw.end > raw.len()
            {
                return None;
            }
            let style = styles.get(raw_run.element)?;
            if style.flow.suppressed
                || !style.flow.painted
                || (raw_run.element != block.element
                    && style.display != crate::book_style::flow::Display::Inline)
                || !supported_raw_mode(style)
                || style.white_space != owner_style.white_space
                || style.text_transform != TextTransform::None
                || style.generated.is_some()
                || style.first_letter.is_some()
                || style.first_line.is_some()
            {
                return None;
            }
            let existing = originals.iter().find(|run| run.element == raw_run.element);
            // A whitespace-only node can have no canonical run; its font and
            // language are harmless to shaping, but its owner style still
            // controls decoration and whitespace painting.
            if existing.is_none()
                && !raw
                    .get(raw_run.raw.clone())?
                    .chars()
                    .all(char::is_whitespace)
            {
                return None;
            }
            runs.push(StyledText {
                bytes: raw_run.raw.clone(),
                element: raw_run.element,
                language: existing.and_then(|run| run.language).or(block.language),
                hyphenation_language: existing
                    .and_then(|run| run.hyphenation_language)
                    .or(block.language.map(crate::TypographyLocale::publication)),
                preserve_whitespace: existing.map_or(true, |run| run.preserve_whitespace),
            });
            previous_raw = raw_run.raw.end;
        }
        if previous_raw != raw.len() || runs.is_empty() {
            return None;
        }
        if let Some(projection) = &pre_line_projection {
            let mut projected_runs = Vec::new();
            for (bytes, element) in projection.style_spans(bound.style_runs())? {
                let template = runs.iter().find(|run| run.element == element)?;
                projected_runs.push(StyledText {
                    bytes,
                    element,
                    language: template.language,
                    hyphenation_language: template.hyphenation_language,
                    preserve_whitespace: template.preserve_whitespace,
                });
            }
            runs = projected_runs;
        }
        let mut segments = Vec::new();
        for span in bound.text_spans() {
            let offset = if let Some(projection) = &pre_line_projection {
                let fragments = projection.visible_fragments(span.raw.clone())?;
                if fragments.len() != 1 || fragments[0].len() != span.canonical.len() {
                    return None;
                }
                fragments[0].start
            } else {
                span.raw.start
            };
            segments.push(Segment {
                source: span.canonical.clone(),
                offset,
            });
        }
        for gap in bound.gaps() {
            if gap.canonical.is_empty() {
                continue;
            }
            // The normalized space is a one-byte projection of this ASCII
            // whitespace gap. It may represent a tab or newline rather than a
            // literal space; exact locations still use the bound raw mapper.
            if block.text.get(gap.canonical.clone())? != " " {
                return None;
            }
            let offset = if let Some(projection) = &pre_line_projection {
                projection
                    .visible_fragments(gap.raw.clone())?
                    .first()?
                    .start
            } else {
                let raw_gap = raw.get(gap.raw.clone())?;
                let whitespace = raw_gap
                    .bytes()
                    .position(|byte| matches!(byte, b' ' | b'\t' | b'\r' | b'\n' | 0x0c))?;
                gap.raw.start.checked_add(whitespace)?
            };
            segments.push(Segment {
                source: gap.canonical.clone(),
                offset,
            });
        }
        segments.sort_by_key(|segment| segment.source.start);
        let mut source_end = 0;
        let mut raw_end = 0;
        for segment in &segments {
            let length = segment.source.len();
            let end = segment.offset.checked_add(length)?;
            let projected = display.get(segment.offset..end)?;
            let canonical = block.text.get(segment.source.clone())?;
            let whitespace_alias = canonical == " "
                && projected.len() == 1
                && matches!(projected.as_bytes()[0], b' ' | b'\t' | b'\r' | b'\n' | 0x0c);
            if segment.source.start != source_end
                || segment.offset < raw_end
                || (projected != canonical && !whitespace_alias)
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
                text: display.to_owned(),
                segments,
                boxes: Vec::new(),
                generated: true,
                detached_initial: false,
            },
            runs,
            binding: SourceBinding::new(block),
            block: block.key,
            owner: block.element,
            white_space: styles.get(block.element)?.white_space,
            pre_line_projection,
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
            || !supported_raw_mode(style)
            || style.white_space != self.white_space
            || style.generated.is_some()
            || style.text_transform != TextTransform::None
            || self
                .pre_line_projection
                .as_ref()
                .map_or(bound.raw(), |projection| projection.text())
                != self.text.text.as_str()
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
            || layout.text_len() != self.text.text.len()
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
                || range.end > self.text.text.len()
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
            ranges.push((range, metrics.block_min_coord, metrics.block_max_coord));
        }
        if previous_end != self.text.text.len() {
            return None;
        }
        let visible_requests: Vec<_> = ranges
            .iter()
            .flat_map(|(range, _, _)| {
                [
                    (range.start, crate::flow_projection::Affinity::After),
                    (range.end, crate::flow_projection::Affinity::Before),
                ]
            })
            .collect();
        let offsets = if let Some(projection) = &self.pre_line_projection {
            projection.visible_carets_at(&visible_requests)?
        } else {
            visible_requests.iter().map(|(offset, _)| *offset).collect()
        };
        for pair in offsets.chunks_exact(2) {
            if pair[0] > pair[1] {
                return None;
            }
            requests.push((pair[0], CaretAffinity::After));
            requests.push((pair[1], CaretAffinity::Before));
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
            pre_line_projection: self.pre_line_projection,
        })
    }
}

impl Prepared {
    /// Move a verified raw layout and its page metadata into the chapter
    /// builder. This remains unused while raw reading is disabled.
    pub(in crate::parley_layout) fn into_page_parts(
        self,
        block: &Block,
    ) -> Option<(
        Layout<PaintBrush>,
        InlineText,
        Vec<crate::RawLinePosition>,
        SourceBinding,
        Option<std::sync::Arc<crate::source::pre_line_projection::PreLineProjection>>,
    )> {
        let lines = self.page_lines(block)?;
        Some((
            self.layout,
            self.text,
            lines,
            self.binding,
            self.pre_line_projection,
        ))
    }

    /// Search a verified raw-shaped block using the same folded literal query
    /// as ordinary reflow search. Returned quotes and both endpoints retain
    /// raw whitespace; callers must still validate them through the open Probe.
    pub(in crate::parley_layout) fn search_raw(
        &self,
        block: &Block,
        context: &AnchorContext<'_>,
        query: &crate::search::Query,
    ) -> Option<Vec<RawTextSelection>> {
        if !self.binding.matches(block) || block.key != context.block {
            return None;
        }
        let bound = block.raw_whitespace.as_ref()?.bind(&block.text)?;
        if self.text.text.as_str() != bound.raw() || self.layout.text_len() != bound.raw().len() {
            return None;
        }
        let matches = query.matches(bound.raw(), crate::search::MAX_SEARCH_HITS);
        let mut requests = Vec::with_capacity(matches.len().checked_mul(2)?);
        for range in &matches {
            if range.start >= range.end || bound.raw().get(range.clone()).is_none() {
                return None;
            }
            requests.push((range.start, CaretAffinity::After));
            requests.push((range.end, CaretAffinity::Before));
        }
        let mut carets = bound.carets_at(&requests)?.into_iter();
        let mut results = Vec::with_capacity(matches.len());
        for range in matches {
            let start = carets.next()?;
            let end = carets.next()?;
            let anchor = |caret: RawCaret| RawAnchor {
                canonical: crate::Anchor {
                    raw_caret: None,
                    book: context.book,
                    chapter: context.chapter.to_owned(),
                    revision: context.revision,
                    block: context.block,
                    offset: caret.canonical_offset,
                    illustration: None,
                },
                caret,
            };
            results.push(RawTextSelection {
                start: anchor(start),
                end: anchor(end),
                text: bound.raw().get(range)?.to_owned(),
            });
        }
        Some(results)
    }

    /// Transfer all verified raw line boundaries in layout order. The caller
    /// must retain the same source block; this does not install the detached
    /// layout into the renderer. Grapheme segmentation is performed once.
    pub(in crate::parley_layout) fn page_lines(
        &self,
        block: &Block,
    ) -> Option<Vec<crate::RawLinePosition>> {
        if !self.binding.matches(block) {
            return None;
        }
        let bound = block.raw_whitespace.as_ref()?.bind(&block.text)?;
        let visible = self
            .pre_line_projection
            .as_ref()
            .map_or(bound.raw(), |projection| projection.text());
        if self.text.text.as_str() != visible || self.layout.text_len() != visible.len() {
            return None;
        }
        let visible_requests: Vec<_> = self
            .lines
            .iter()
            .flat_map(|line| {
                [
                    (line.bytes.start, crate::flow_projection::Affinity::After),
                    (line.bytes.end, crate::flow_projection::Affinity::Before),
                ]
            })
            .collect();
        let offsets = if let Some(projection) = &self.pre_line_projection {
            projection.visible_carets_at(&visible_requests)?
        } else {
            visible_requests.iter().map(|(offset, _)| *offset).collect()
        };
        let mut requests = Vec::with_capacity(offsets.len());
        for (line, pair) in self.lines.iter().zip(offsets.chunks_exact(2)) {
            if line.start.raw_offset != pair[0] || line.end.raw_offset != pair[1] {
                return None;
            }
            requests.push((pair[0], CaretAffinity::After));
            requests.push((pair[1], CaretAffinity::Before));
        }
        let mut verified = bound.carets_at(&requests)?.into_iter();
        let mut positions = Vec::with_capacity(self.lines.len());
        for line in &self.lines {
            let start = verified.next()?;
            let end = verified.next()?;
            if start != line.start || end != line.end {
                return None;
            }
            positions.push(crate::RawLinePosition { start, end });
        }
        Some(positions)
    }

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
                raw_caret: None,
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
                raw_caret: None,
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
