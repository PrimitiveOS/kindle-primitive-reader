//! Text selection and page-local highlight geometry for reflowable books.

use std::ops::Range;

use icu_segmenter::{WordSegmenter, options::WordBreakInvariantOptions};
use parley::{Affinity, BoundingBox, Cluster, Cursor, Layout, Selection};
use serde::{Deserialize, Serialize};

use crate::flow_projection::Affinity as SourceAffinity;
use crate::{Anchor, Probe, Renderer, Settings};

/// A half-open range of source text, ordered in publication reading order.
/// `text` preserves the source wording with the same whitespace normalization
/// used by the renderer. Paragraphs in a selection are separated by newlines.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextSelection {
    pub start: Anchor,
    pub end: Anchor,
    pub text: String,
}

/// A transient selection over a verified raw whitespace snapshot. It is not
/// persisted or used by UI until raw-anchor state and layout are complete.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) struct RawTextSelection {
    pub(crate) start: crate::source::raw_whitespace::RawAnchor,
    pub(crate) end: crate::source::raw_whitespace::RawAnchor,
    pub(crate) text: String,
}

/// A highlight rectangle in the upright page image, including its margins.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HighlightRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl Probe {
    /// Compare two text locations in this chapter without collapsing distinct
    /// raw whitespace offsets. A mixed coordinate system in one block is
    /// ambiguous and is rejected until a source-bound conversion is available.
    pub(crate) fn compare_text_anchors(
        &self,
        first: &Anchor,
        second: &Anchor,
    ) -> Option<std::cmp::Ordering> {
        let locate = |anchor: &Anchor| {
            if anchor.illustration.is_some() {
                return None;
            }
            if let Some(caret) = &anchor.raw_caret {
                let mut canonical = anchor.clone();
                canonical.raw_caret = None;
                self.validate_raw_whitespace_anchor(&crate::source::raw_whitespace::RawAnchor {
                    canonical,
                    caret: caret.clone(),
                })?;
                Some((
                    self.selection_block(anchor.block)?,
                    caret.raw_offset,
                    Some(caret.raw_sha256),
                ))
            } else {
                let (index, offset) = self.project_anchor(anchor)?;
                Some((index, offset, None))
            }
        };
        let (a_block, a_offset, a_hash) = locate(first)?;
        let (b_block, b_offset, b_hash) = locate(second)?;
        if a_block != b_block {
            return Some(a_block.cmp(&b_block));
        }
        if a_hash != b_hash {
            return None;
        }
        Some(a_offset.cmp(&b_offset))
    }

    /// Build an exact selection with at least one raw endpoint. Mixed raw and
    /// ordinary endpoints are supported across distinct verified blocks.
    #[allow(dead_code)]
    pub(crate) fn select_raw_anchors(
        &self,
        first: &Anchor,
        second: &Anchor,
    ) -> Option<TextSelection> {
        let verify = |anchor: &Anchor| {
            if anchor.illustration.is_some() {
                return None;
            }
            if let Some(caret) = &anchor.raw_caret {
                let mut canonical = anchor.clone();
                canonical.raw_caret = None;
                self.validate_raw_whitespace_anchor(&crate::source::raw_whitespace::RawAnchor {
                    canonical,
                    caret: caret.clone(),
                })?;
                Some(anchor.clone())
            } else {
                let (index, offset) = self.project_anchor(anchor)?;
                Some(self.anchor(self.source.blocks[index].key, offset))
            }
        };
        let first = verify(first)?;
        let second = verify(second)?;
        if first.block != second.block {
            return self.select_raw_across_blocks(first, second);
        }
        let wrap = |anchor: Anchor| {
            let mut canonical = anchor;
            let caret = canonical.raw_caret.take()?;
            Some(crate::source::raw_whitespace::RawAnchor { canonical, caret })
        };
        let first_raw = wrap(first)?;
        let second_raw = wrap(second)?;
        let selection = self.select_raw_whitespace(&first_raw, &second_raw)?;
        let unwrap = |raw: crate::source::raw_whitespace::RawAnchor| {
            let mut anchor = raw.canonical;
            anchor.raw_caret = Some(raw.caret);
            anchor
        };
        Some(TextSelection {
            start: unwrap(selection.start),
            end: unwrap(selection.end),
            text: selection.text,
        })
    }

    /// Each endpoint and intermediate block must use its installed raw or
    /// ordinary projection; an ambiguous layout rejects the entire range.
    fn select_raw_across_blocks(&self, first: Anchor, second: Anchor) -> Option<TextSelection> {
        let a = self.selection_block(first.block)?;
        let b = self.selection_block(second.block)?;
        let (first_index, last_index, start, end) = if a < b {
            (a, b, first, second)
        } else {
            (b, a, second, first)
        };
        let raw_blocks = self.verified_raw_blocks()?;
        if raw_blocks.contains(&first_index) != start.raw_caret.is_some()
            || raw_blocks.contains(&last_index) != end.raw_caret.is_some()
        {
            return None;
        }
        let mut quote = String::new();
        for index in first_index..=last_index {
            let block = &self.source.blocks[index];
            let part = if raw_blocks.contains(&index) {
                let raw = block.raw_whitespace.as_ref()?.bind(&block.text)?;
                let from = if index == first_index {
                    start.raw_caret.as_ref()?.raw_offset
                } else {
                    0
                };
                let to = if index == last_index {
                    end.raw_caret.as_ref()?.raw_offset
                } else {
                    raw.raw().len()
                };
                raw.raw().get(from..to)?.to_owned()
            } else {
                let view = self.reading_flow.view(&self.source, block).ok()?;
                let mut text = String::new();
                let from = if index == first_index {
                    start.offset
                } else {
                    0
                };
                let to = if index == last_index {
                    end.offset
                } else {
                    block.text.len()
                };
                for range in view.layout_spans(from..to)? {
                    text.push_str(view.text().get(range)?);
                }
                text
            };
            if part.is_empty() {
                continue;
            }
            let separator = usize::from(!quote.is_empty());
            if quote
                .len()
                .checked_add(separator)?
                .checked_add(part.len())?
                > crate::library::MAX_HIGHLIGHT_TEXT_BYTES
            {
                return None;
            }
            if separator != 0 {
                quote.push('\n');
            }
            quote.push_str(&part);
        }
        Some(TextSelection {
            start,
            end,
            text: quote,
        })
    }

    fn verified_raw_blocks(&self) -> Option<std::collections::BTreeSet<usize>> {
        let Renderer::Parley(renderer) = &self.renderer else {
            return None;
        };
        if renderer
            .paragraphs()
            .iter()
            .any(|paragraph| paragraph.joined_note.is_some())
        {
            return None;
        }
        let mut blocks = std::collections::BTreeSet::new();
        for paragraph in renderer.paragraphs() {
            if paragraph.raw_binding.is_none() {
                continue;
            }
            let index = self.selection_block(paragraph.key)?;
            if !paragraph.is_verified_raw(&self.source.blocks[index]) || !blocks.insert(index) {
                return None;
            }
        }
        Some(blocks)
    }

    /// Select exact raw content inside one verified source block. Reject
    /// cross-block, stale or unrepresentable endpoints rather than snapping.
    #[allow(dead_code)]
    pub(crate) fn select_raw_whitespace(
        &self,
        first: &crate::source::raw_whitespace::RawAnchor,
        second: &crate::source::raw_whitespace::RawAnchor,
    ) -> Option<RawTextSelection> {
        self.validate_raw_whitespace_anchor(first)?;
        self.validate_raw_whitespace_anchor(second)?;
        if first.canonical.block != second.canonical.block
            || first.caret.raw_offset == second.caret.raw_offset
        {
            return None;
        }
        let (start, end) = if first.caret.raw_offset < second.caret.raw_offset {
            (first, second)
        } else {
            (second, first)
        };
        let index = self
            .source
            .blocks
            .binary_search_by_key(&start.canonical.block, |block| block.key)
            .ok()?;
        let block = &self.source.blocks[index];
        let bound = block.raw_whitespace.as_ref()?.bind(&block.text)?;
        let text = bound
            .raw()
            .get(start.caret.raw_offset..end.caret.raw_offset)?;
        Some(RawTextSelection {
            start: start.clone(),
            end: end.clone(),
            text: text.to_owned(),
        })
    }

    /// Up to `words` source words on either side of a selected passage.
    /// It uses the selection anchors so repeated words elsewhere on the page
    /// cannot attach their context to a vocabulary entry.
    pub(crate) fn word_context(
        &self,
        selection: &TextSelection,
        words: usize,
    ) -> Option<(String, String)> {
        if selection.start.raw_caret.is_some() || selection.end.raw_caret.is_some() {
            return self.raw_word_context(selection, words);
        }
        #[cfg(feature = "kindle")]
        if let Some(band) = self.foreign_band_for_anchors(&selection.start, &selection.end) {
            return band.layout.note_word_context(selection, words);
        }
        let spans = self.selection_spans(&selection.start, &selection.end)?;
        let mut before = String::new();
        for (index, block) in self
            .source
            .blocks
            .iter()
            .enumerate()
            .take(spans.first + 1)
            .rev()
        {
            let view = self.reading_flow.view(&self.source, block).ok()?;
            let end = if index == spans.first {
                view.project_nearest(spans.first_byte, SourceAffinity::Before)?
            } else {
                view.text().len()
            };
            let text = view.text().get(..end)?;
            before = bounded_context(
                &format!("{} {before}", bounded_context(text, words, true)),
                words,
                true,
            );
            if word_ranges(&before).count() >= words || before.chars().count() == 512 {
                break;
            }
        }
        let mut after = String::new();
        for (index, block) in self.source.blocks.iter().enumerate().skip(spans.last) {
            let view = self.reading_flow.view(&self.source, block).ok()?;
            let start = if index == spans.last {
                view.project_nearest(spans.last_byte, SourceAffinity::After)?
            } else {
                0
            };
            let text = view.text().get(start..)?;
            after = bounded_context(
                &format!("{after} {}", bounded_context(text, words, false)),
                words,
                false,
            );
            if word_ranges(&after).count() >= words || after.chars().count() == 512 {
                break;
            }
        }
        Some((before, after))
    }

    /// Preserve the displayed text around raw selections for dictionary and
    /// translation context, without normalizing away significant whitespace.
    fn raw_word_context(
        &self,
        selection: &TextSelection,
        words: usize,
    ) -> Option<(String, String)> {
        if self.select_raw_anchors(&selection.start, &selection.end)? != *selection {
            return None;
        }
        let first = self.selection_block(selection.start.block)?;
        let last = self.selection_block(selection.end.block)?;
        if first > last {
            return None;
        }
        let raw_blocks = self.verified_raw_blocks()?;
        if raw_blocks.contains(&first) != selection.start.raw_caret.is_some()
            || raw_blocks.contains(&last) != selection.end.raw_caret.is_some()
        {
            return None;
        }
        let mut before = String::new();
        for index in (0..=first).rev() {
            let block = &self.source.blocks[index];
            let piece = if raw_blocks.contains(&index) {
                let bound = block.raw_whitespace.as_ref()?.bind(&block.text)?;
                let end = if index == first {
                    selection.start.raw_caret.as_ref()?.raw_offset
                } else {
                    bound.raw().len()
                };
                bounded_context(bound.raw().get(..end)?, words, true)
            } else {
                let view = self.reading_flow.view(&self.source, block).ok()?;
                let end = if index == first {
                    view.project_nearest(selection.start.offset, SourceAffinity::Before)?
                } else {
                    view.text().len()
                };
                bounded_context(view.text().get(..end)?, words, true)
            };
            before = bounded_context(&format!("{piece} {before}"), words, true);
            if word_ranges(&before).count() >= words || before.chars().count() == 512 {
                break;
            }
        }
        let mut after = String::new();
        for index in last..self.source.blocks.len() {
            let block = &self.source.blocks[index];
            let piece = if raw_blocks.contains(&index) {
                let bound = block.raw_whitespace.as_ref()?.bind(&block.text)?;
                let start = if index == last {
                    selection.end.raw_caret.as_ref()?.raw_offset
                } else {
                    0
                };
                bounded_context(bound.raw().get(start..)?, words, false)
            } else {
                let view = self.reading_flow.view(&self.source, block).ok()?;
                let start = if index == last {
                    view.project_nearest(selection.end.offset, SourceAffinity::After)?
                } else {
                    0
                };
                bounded_context(view.text().get(start..)?, words, false)
            };
            after = bounded_context(&format!("{after} {piece}"), words, false);
            if word_ranges(&after).count() >= words || after.chars().count() == 512 {
                break;
            }
        }
        Some((before, after))
    }

    /// A bounded automatic-word scan from the original source start. It stops
    /// at punctuation, inline block boundaries and 20 Unicode scalar values.
    /// The returned endpoint is an actual source anchor, never a lookup form.
    pub(crate) fn dictionary_scan(&self, selection: &TextSelection) -> Option<TextSelection> {
        if selection.start.raw_caret.is_some() || selection.end.raw_caret.is_some() {
            if self.select_raw_anchors(&selection.start, &selection.end)? != *selection
                || selection.start.block != selection.end.block
            {
                return None;
            }
            let index = self.selection_block(selection.start.block)?;
            if !self.verified_raw_blocks()?.contains(&index) {
                return None;
            }
            let block = &self.source.blocks[index];
            let bound = block.raw_whitespace.as_ref()?.bind(&block.text)?;
            let start = selection.start.raw_caret.as_ref()?.raw_offset;
            let tail = bound.raw().get(start..)?;
            let length = tail
                .char_indices()
                .take(crate::dictionary::MAX_SCAN_CHARACTERS)
                .take_while(|(_, c)| crate::dictionary::cjk_word_character(*c))
                .map(|(i, c)| i + c.len_utf8())
                .last()?;
            let length = icu_segmenter::GraphemeClusterSegmenter::new()
                .segment_str(tail)
                .take_while(|end| *end <= length)
                .last()
                .filter(|end| *end > 0)?;
            let raw = self.raw_whitespace_anchor(
                block.key,
                start.checked_add(length)?,
                crate::source::raw_whitespace::CaretAffinity::Before,
            )?;
            let mut end = raw.canonical;
            end.raw_caret = Some(raw.caret);
            return self.select_raw_anchors(&selection.start, &end);
        }
        #[cfg(feature = "kindle")]
        if let Some(band) = self.foreign_band_for_anchors(&selection.start, &selection.end) {
            return band.layout.note_dictionary_scan(selection);
        }
        let spans = self.selection_spans(&selection.start, &selection.end)?;
        if spans.first != spans.last {
            return None;
        }
        let block = &self.source.blocks[spans.first];
        let view = self.reading_flow.view(&self.source, block).ok()?;
        if !view.source_is_present(spans.first_byte) {
            return None;
        }
        let start = view.project_offset(spans.first_byte, SourceAffinity::After)?;
        let tail = view.text().get(start..)?;
        let length = tail
            .char_indices()
            .take(crate::dictionary::MAX_SCAN_CHARACTERS)
            .take_while(|(_, c)| crate::dictionary::cjk_word_character(*c))
            .map(|(i, c)| i + c.len_utf8())
            .last()?;
        let length = icu_segmenter::GraphemeClusterSegmenter::new()
            .segment_str(tail)
            .take_while(|end| *end <= length)
            .last()
            .filter(|end| *end > 0)?;
        self.select_range(
            &self.anchor(block.key, view.source_offset(start, SourceAffinity::After)?),
            &self.anchor(
                block.key,
                view.source_offset(start + length, SourceAffinity::Before)?,
            ),
        )
    }

    /// Select the Unicode word directly under a page coordinate.
    pub fn word_at(&self, page: usize, x: f32, y: f32) -> Option<TextSelection> {
        #[cfg(feature = "kindle")]
        if let Some(band) = self.foreign_band_at(page, x, y) {
            let (x, y) = band.coordinates(page, x, y)?;
            return band.layout.note_word_at(x, y);
        }
        let (range, y) = self.page_target(page, y)?;
        self.word_in(range, x, y)
    }

    pub(crate) fn word_in(&self, range: Range<f32>, x: f32, y: f32) -> Option<TextSelection> {
        if !self.is_vertical() {
            let point = self.selection_point(range.clone(), x, y)?;
            if let Renderer::Parley(renderer) = &self.renderer {
                for paragraph in renderer.paragraphs_in(point.page.clone()) {
                    if paragraph.raw_binding.is_none()
                        || !paragraph.hit_columns().contains(&point.x)
                        || !(point.inside_columns || paragraph.allows_margin_hits())
                    {
                        continue;
                    }
                    let local_x = point.x - paragraph.x;
                    let local_y = point.y - paragraph.y;
                    if !(0.0..paragraph.layout.height()).contains(&local_y) {
                        continue;
                    }
                    let block = &self.source.blocks[self.selection_block(paragraph.key)?];
                    if !paragraph.is_verified_raw(block) {
                        return None;
                    }
                    let (cluster, _) =
                        Cluster::from_point_exact(&paragraph.layout, local_x, local_y)?;
                    let line = cluster.line();
                    let metrics = line.metrics();
                    let ink = (paragraph.y + metrics.content_block_min_coord)
                        ..(paragraph.y + metrics.content_block_max_coord);
                    if ink.end <= point.page.start
                        || ink.start >= point.page.end
                        || (renderer.uses_float_geometry() && !ink.contains(&point.y))
                    {
                        return None;
                    }
                    let bound = block.raw_whitespace.as_ref()?.bind(&block.text)?;
                    let word = word_range(bound.raw(), cluster.text_range().start)?;
                    use crate::source::raw_whitespace::CaretAffinity;
                    let start =
                        self.raw_whitespace_anchor(block.key, word.start, CaretAffinity::After)?;
                    let end =
                        self.raw_whitespace_anchor(block.key, word.end, CaretAffinity::Before)?;
                    let attach = |raw: crate::source::raw_whitespace::RawAnchor| {
                        let mut anchor = raw.canonical;
                        anchor.raw_caret = Some(raw.caret);
                        anchor
                    };
                    return self.select_raw_anchors(&attach(start), &attach(end));
                }
            }
        }
        let (key, offset) = self.source_at(range, x, y)?;
        let block = &self.source.blocks[self.selection_block(key)?];
        let view = self.reading_flow.view(&self.source, block).ok()?;
        if !view.source_is_present(offset) {
            return None;
        }
        let offset = view.project_offset(offset, SourceAffinity::After)?;
        let word = word_range(view.text(), offset)?;
        self.select_range(
            &self.anchor(key, view.source_offset(word.start, SourceAffinity::After)?),
            &self.anchor(key, view.source_offset(word.end, SourceAffinity::Before)?),
        )
    }

    /// Find the caret nearest the point within a visible paragraph.
    pub fn text_position_at(&self, page: usize, x: f32, y: f32) -> Option<Anchor> {
        #[cfg(feature = "kindle")]
        if let Some(band) = self.foreign_band_at(page, x, y) {
            let (x, y) = band.coordinates(page, x, y)?;
            return band.layout.note_caret_at(x, y);
        }
        let (range, y) = self.page_target(page, y)?;
        if let Renderer::Vertical(renderer) = &self.renderer {
            let (key, offset) = renderer.caret_at(range, x, y)?;
            return Some(self.anchor(key, offset));
        }
        let point = self.selection_point(range, x, y)?;
        let Renderer::Parley(renderer) = &self.renderer else {
            return None;
        };
        for paragraph in renderer.paragraphs_in(point.page) {
            let y = point.y - paragraph.y;
            let x = point.x - paragraph.x;
            let child = paragraph.text_box_at(x, y);
            let (layout, x, y) = child.map_or((&paragraph.layout, x, y), |child| {
                (&child.layout, x - child.x, y - child.y)
            });
            if (point.inside_columns || paragraph.allows_margin_hits())
                && paragraph.hit_columns().contains(&point.x)
                && (0.0..layout.height()).contains(&y)
                && (!renderer.uses_float_geometry()
                    || layout.lines().any(|line| {
                        let metrics = line.metrics();
                        (metrics.block_min_coord..metrics.block_max_coord).contains(&y)
                            && (metrics.inline_min_coord..metrics.inline_max_coord).contains(&x)
                    }))
                && if point.inside_columns || paragraph.signed_margin {
                    Cluster::from_point(layout, x, y).is_some()
                } else {
                    Cluster::from_point_exact(layout, x, y).is_some()
                }
            {
                let cursor = Cursor::from_point(layout, x, y);
                let affinity = match cursor.affinity() {
                    Affinity::Upstream => SourceAffinity::Before,
                    Affinity::Downstream => SourceAffinity::After,
                };
                if paragraph.raw_binding.is_some() {
                    let block = &self.source.blocks[self.selection_block(paragraph.key)?];
                    if child.is_some() || !paragraph.is_verified_raw(block) {
                        return None;
                    }
                    let raw_affinity = match affinity {
                        SourceAffinity::Before => {
                            crate::source::raw_whitespace::CaretAffinity::Before
                        }
                        SourceAffinity::After => {
                            crate::source::raw_whitespace::CaretAffinity::After
                        }
                    };
                    let raw =
                        self.raw_whitespace_anchor(paragraph.key, cursor.index(), raw_affinity)?;
                    let mut anchor = raw.canonical;
                    anchor.raw_caret = Some(raw.caret);
                    return Some(anchor);
                }
                let visible = match child {
                    Some(child) => child.source.start + cursor.index(),
                    None => paragraph.visible_text_offset(cursor.index(), affinity)?,
                };
                let (key, offset) = paragraph.source_location(visible, affinity)?;
                return Some(self.anchor(key, offset));
            }
        }
        None
    }

    /// Select an anchored range across adjacent blocks and pages of the loaded
    /// chapter. The caller may supply the endpoints in either direction.
    pub fn select_range(&self, start: &Anchor, end: &Anchor) -> Option<TextSelection> {
        if start.raw_caret.is_some() || end.raw_caret.is_some() {
            return self.select_raw_anchors(start, end);
        }
        #[cfg(feature = "kindle")]
        if let Some(band) = self.foreign_band_for_anchors(start, end) {
            return band.layout.note_select_range(start, end);
        }
        self.select_quote(start, end, false)
    }

    /// Historical quote recovery must not become corrupt when current CSS
    /// intentionally omits unchanged canonical text. Never rewrite saved quotes.
    pub(crate) fn select_source_range(
        &self,
        start: &Anchor,
        end: &Anchor,
    ) -> Option<TextSelection> {
        if start.raw_caret.is_some() || end.raw_caret.is_some() {
            return self.select_raw_anchors(start, end);
        }
        #[cfg(feature = "kindle")]
        if let Some(band) = self.foreign_band_for_anchors(start, end) {
            return band.layout.note_select_range(start, end);
        }
        self.select_quote(start, end, true)
    }

    fn select_quote(&self, start: &Anchor, end: &Anchor, canonical: bool) -> Option<TextSelection> {
        let spans = self.selection_spans(start, end)?;
        let mut quote = String::new();
        for (offset, block) in self.source.blocks[spans.first..=spans.last]
            .iter()
            .enumerate()
        {
            let index = spans.first + offset;
            let Some(span) = spans.in_block(index, block.text.len()) else {
                continue;
            };
            let part = if canonical {
                block.text.get(span)?.to_owned()
            } else {
                let view = self.reading_flow.view(&self.source, block).ok()?;
                let mut part = String::new();
                for range in view.layout_spans(span)? {
                    part.push_str(view.text().get(range)?);
                }
                part
            };
            if part.is_empty() {
                continue;
            }
            if !quote.is_empty() {
                quote.push('\n');
            }
            quote.push_str(&part);
        }
        if quote.trim().is_empty() {
            return None;
        }
        Some(TextSelection {
            start: self.anchor(self.source.blocks[spans.first].key, spans.first_byte),
            end: self.anchor(self.source.blocks[spans.last].key, spans.last_byte),
            text: quote,
        })
    }

    #[cfg(feature = "kindle")]
    pub(crate) fn can_inspect_publisher_html(&self, selection: &TextSelection) -> bool {
        self.source.can_inspect_publisher_html()
            && selection.start.chapter == self.chapter
            && selection.end.chapter == self.chapter
            && selection.start.revision == self.source.revision
            && selection.end.revision == self.source.revision
    }

    #[cfg(feature = "kindle")]
    pub(crate) fn inspect_publisher_html(
        &self,
        xhtml: &str,
        selection: &TextSelection,
    ) -> Option<String> {
        if !self.can_inspect_publisher_html(selection)
            || self
                .select_source_range(&selection.start, &selection.end)?
                .text
                != selection.text
        {
            return None;
        }
        let spans = self.selection_spans(&selection.start, &selection.end)?;
        let mut contributing = (spans.first..=spans.last).filter_map(|index| {
            Some((
                index,
                spans.in_block(index, self.source.blocks[index].text.len())?,
            ))
        });
        let first = contributing.next()?;
        let last = contributing.next_back().unwrap_or_else(|| first.clone());
        self.source.publisher_selection_html(xhtml, first, last)
    }

    /// Return the portions of a selection visible on this page. Geometry is
    /// derived anew from source anchors after font, page, or viewport changes.
    pub fn selection_rects(&self, page: usize, selection: &TextSelection) -> Vec<HighlightRect> {
        let mut rects: Vec<_> = self
            .page_parts(page)
            .flat_map(|(range, y)| {
                self.selection_rects_in(range, selection)
                    .into_iter()
                    .map(move |mut rect| {
                        rect.y += y as i32;
                        rect
                    })
            })
            .collect();
        #[cfg(feature = "kindle")]
        for band in self.foreign_bands(page) {
            rects.extend(band.layout.note_selection_rects(selection).into_iter().map(
                |mut rect| {
                    rect.y += band.y_offset(page) as i32;
                    rect
                },
            ));
        }
        rects
    }

    pub(crate) fn selection_rects_in(
        &self,
        page: Range<f32>,
        selection: &TextSelection,
    ) -> Vec<HighlightRect> {
        if selection.start.raw_caret.is_some() || selection.end.raw_caret.is_some() {
            return self.raw_selection_rects_in(page, selection);
        }
        self.anchor_rects_in(page, &selection.start, &selection.end)
    }

    fn raw_selection_rects_in(
        &self,
        page: Range<f32>,
        selection: &TextSelection,
    ) -> Vec<HighlightRect> {
        let Some(verified) = self.select_raw_anchors(&selection.start, &selection.end) else {
            return Vec::new();
        };
        if verified != *selection
            || !page.start.is_finite()
            || !page.end.is_finite()
            || page.start >= page.end
        {
            return Vec::new();
        }
        let Some(first) = self.selection_block(selection.start.block) else {
            return Vec::new();
        };
        let Some(last) = self.selection_block(selection.end.block) else {
            return Vec::new();
        };
        if first > last {
            return Vec::new();
        }
        let Renderer::Parley(renderer) = &self.renderer else {
            return Vec::new();
        };
        let start = selection
            .start
            .raw_caret
            .as_ref()
            .map_or(selection.start.offset, |caret| caret.raw_offset);
        let end = selection
            .end
            .raw_caret
            .as_ref()
            .map_or(selection.end.offset, |caret| caret.raw_offset);
        let mut rects = Vec::new();
        for paragraph in renderer.paragraphs_in(page.clone()) {
            let Some(index) = self.selection_block(paragraph.key) else {
                continue;
            };
            if !(first..=last).contains(&index) {
                continue;
            }
            if paragraph.joined_note.is_some() {
                return Vec::new();
            }
            let block = &self.source.blocks[index];
            let columns = paragraph.paint_columns(self.settings);
            let geometry = Geometry {
                layout: &paragraph.layout,
                x: paragraph.x,
                y: paragraph.y,
                placed_boxes: paragraph.inline_text.is_some(),
            };
            if paragraph.raw_binding.is_some() {
                if !paragraph.is_verified_raw(block) {
                    return Vec::new();
                }
                if (index == first && selection.start.raw_caret.is_none())
                    || (index == last && selection.end.raw_caret.is_none())
                {
                    return Vec::new();
                }
                let Some(raw) = block
                    .raw_whitespace
                    .as_ref()
                    .and_then(|s| s.bind(&block.text))
                else {
                    return Vec::new();
                };
                let from = if index == first { start } else { 0 };
                let to = if index == last { end } else { raw.raw().len() };
                if from < to {
                    geometry.append(from..to, &page, self.settings, &columns, &mut rects);
                }
            } else {
                if (index == first && selection.start.raw_caret.is_some())
                    || (index == last && selection.end.raw_caret.is_some())
                {
                    return Vec::new();
                }
                let Ok(view) = self.reading_flow.view(&self.source, block) else {
                    return Vec::new();
                };
                let from = if index == first { start } else { 0 };
                let to = if index == last { end } else { block.text.len() };
                let Some(spans) = view.layout_spans(from..to) else {
                    return Vec::new();
                };
                for span in spans {
                    geometry.append_flow(
                        paragraph,
                        span,
                        &page,
                        self.settings,
                        &columns,
                        &mut rects,
                    );
                }
            }
        }
        if rects.len() > 4096 {
            Vec::new()
        } else {
            rects
        }
    }

    pub(crate) fn anchor_rects_in(
        &self,
        page: Range<f32>,
        start: &Anchor,
        end: &Anchor,
    ) -> Vec<HighlightRect> {
        if start.raw_caret.is_some() || end.raw_caret.is_some() {
            return self
                .select_raw_anchors(start, end)
                .map_or_else(Vec::new, |selection| {
                    self.raw_selection_rects_in(page, &selection)
                });
        }
        let Some(spans) = self.selection_spans(start, end) else {
            return Vec::new();
        };
        if let Renderer::Vertical(renderer) = &self.renderer {
            return (spans.first..=spans.last)
                .flat_map(|index| {
                    let block = &self.source.blocks[index];
                    spans
                        .in_block(index, block.text.len())
                        .map_or_else(Vec::new, |bytes| {
                            let Ok(view) = self.reading_flow.view(&self.source, block) else {
                                return Vec::new();
                            };
                            view.layout_spans(bytes)
                                .unwrap_or_default()
                                .into_iter()
                                .flat_map(|bytes| {
                                    renderer.selection_rects(page.clone(), block.key, bytes)
                                })
                                .collect()
                        })
                })
                .collect();
        }
        let Renderer::Parley(renderer) = &self.renderer else {
            return Vec::new();
        };
        let mut rects = Vec::new();
        for paragraph in renderer.paragraphs_in(page.clone()) {
            if let Some(note) = &paragraph.joined_note {
                let columns = paragraph.paint_columns(self.settings);
                let geometry = Geometry {
                    layout: &paragraph.layout,
                    x: paragraph.x,
                    y: paragraph.y,
                    placed_boxes: paragraph.inline_text.is_some(),
                };
                for piece in &note.pieces {
                    let Some(index) = self.selection_block(piece.block) else {
                        continue;
                    };
                    let block = &self.source.blocks[index];
                    let Some(span) = spans.in_block(index, block.text.len()) else {
                        continue;
                    };
                    let Ok(view) = self.reading_flow.view(&self.source, block) else {
                        continue;
                    };
                    for local in view
                        .layout_spans(span)
                        .unwrap_or_default()
                        .into_iter()
                        .filter_map(|local| piece.joined_span(local))
                    {
                        geometry.append_flow(
                            paragraph,
                            local,
                            &page,
                            self.settings,
                            &columns,
                            &mut rects,
                        );
                    }
                }
                continue;
            }
            let Some(index) = self.selection_block(paragraph.key) else {
                continue;
            };
            let Some(span) = spans.in_block(index, self.source.blocks[index].text.len()) else {
                continue;
            };
            let Ok(view) = self
                .reading_flow
                .view(&self.source, &self.source.blocks[index])
            else {
                continue;
            };
            let Some(layout_spans) = view.layout_spans(span) else {
                continue;
            };
            let columns = paragraph.paint_columns(self.settings);
            let outer = Geometry {
                layout: &paragraph.layout,
                x: paragraph.x,
                y: paragraph.y,
                placed_boxes: paragraph.inline_text.is_some()
                    || !self.source.blocks[index].images.is_empty(),
            };
            for span in layout_spans {
                outer.append_flow(paragraph, span, &page, self.settings, &columns, &mut rects);
            }
        }
        rects
    }

    fn selection_point(&self, page: Range<f32>, x: f32, y: f32) -> Option<PagePoint> {
        let inside_columns = self.settings.text_hit_column(x)?;
        let local_y = y - self.settings.top_margin() as f32;
        let y = local_y + page.start;
        ((0.0..self.settings.text_height() as f32).contains(&local_y) && page.contains(&y))
            .then_some(PagePoint {
                page,
                x: x - self.settings.left_margin() as f32,
                y,
                inside_columns,
            })
    }

    fn selection_block(&self, key: usize) -> Option<usize> {
        self.source
            .blocks
            .binary_search_by_key(&key, |block| block.key)
            .ok()
    }

    fn selection_spans(&self, start: &Anchor, end: &Anchor) -> Option<Spans> {
        if start.illustration.is_some()
            || end.illustration.is_some()
            || start.raw_caret.is_some()
            || end.raw_caret.is_some()
        {
            return None;
        }
        let (a, b) = if start.chapter == end.chapter {
            (self.project_anchor(start)?, self.project_anchor(end)?)
        } else {
            if start.book != self.book || end.book != self.book {
                return None;
            }
            let start_chapter = self.chapter_index(&start.chapter)?;
            let end_chapter = self.chapter_index(&end.chapter)?;
            let current = self.chapter_index(&self.chapter)?;
            if !(start_chapter..=end_chapter).contains(&current) {
                return None;
            }
            let local_start = if current == start_chapter {
                self.project_anchor(start)?
            } else {
                (
                    self.source
                        .blocks
                        .iter()
                        .position(|block| !block.text.is_empty())?,
                    0,
                )
            };
            let local_end = if current == end_chapter {
                self.project_anchor(end)?
            } else {
                let last = self
                    .source
                    .blocks
                    .iter()
                    .rposition(|block| !block.text.is_empty())?;
                (last, self.source.blocks[last].text.len())
            };
            (local_start, local_end)
        };
        if a == b {
            return None;
        }
        let ((first, first_byte), (last, last_byte)) = if a < b { (a, b) } else { (b, a) };
        Some(Spans {
            first,
            last,
            first_byte,
            last_byte,
        })
    }
}

#[cfg(feature = "kindle")]
impl crate::footnotes::cross_part::Layout {
    pub(crate) fn note_word_context(
        &self,
        selection: &TextSelection,
        words: usize,
    ) -> Option<(String, String)> {
        let spans = self.note_spans(&selection.start, &selection.end)?;
        let mut before = String::new();
        for (index, block) in self
            .note
            .source
            .blocks
            .iter()
            .enumerate()
            .take(spans.first + 1)
            .rev()
        {
            if !self.note.blocks.contains(&block.key) {
                break;
            }
            let view = self.flow.view(&self.note.source, block).ok()?;
            let end = if index == spans.first {
                view.project_nearest(spans.first_byte, SourceAffinity::Before)?
            } else {
                view.text().len()
            };
            let text = view.text().get(..end)?;
            before = bounded_context(
                &format!("{} {before}", bounded_context(text, words, true)),
                words,
                true,
            );
            if word_ranges(&before).count() >= words || before.chars().count() == 512 {
                break;
            }
        }
        let mut after = String::new();
        for (index, block) in self.note.source.blocks.iter().enumerate().skip(spans.last) {
            if !self.note.blocks.contains(&block.key) {
                break;
            }
            let view = self.flow.view(&self.note.source, block).ok()?;
            let start = if index == spans.last {
                view.project_nearest(spans.last_byte, SourceAffinity::After)?
            } else {
                0
            };
            let text = view.text().get(start..)?;
            after = bounded_context(
                &format!("{after} {}", bounded_context(text, words, false)),
                words,
                false,
            );
            if word_ranges(&after).count() >= words || after.chars().count() == 512 {
                break;
            }
        }
        Some((before, after))
    }

    pub(crate) fn note_dictionary_scan(&self, selection: &TextSelection) -> Option<TextSelection> {
        let spans = self.note_spans(&selection.start, &selection.end)?;
        if spans.first != spans.last {
            return None;
        }
        let block = &self.note.source.blocks[spans.first];
        let view = self.flow.view(&self.note.source, block).ok()?;
        if !view.source_is_present(spans.first_byte) {
            return None;
        }
        let start = view.project_offset(spans.first_byte, SourceAffinity::After)?;
        let tail = view.text().get(start..)?;
        let length = tail
            .char_indices()
            .take(crate::dictionary::MAX_SCAN_CHARACTERS)
            .take_while(|(_, c)| crate::dictionary::cjk_word_character(*c))
            .map(|(i, c)| i + c.len_utf8())
            .last()?;
        let length = icu_segmenter::GraphemeClusterSegmenter::new()
            .segment_str(tail)
            .take_while(|end| *end <= length)
            .last()
            .filter(|end| *end > 0)?;
        self.note_select_range(
            &self.note_anchor(block.key, view.source_offset(start, SourceAffinity::After)?)?,
            &self.note_anchor(
                block.key,
                view.source_offset(start + length, SourceAffinity::Before)?,
            )?,
        )
    }

    /// Select a word at a foreign note's local text-column x and document y.
    /// Every returned endpoint is from the retained canonical foreign chapter.
    pub(crate) fn note_word_at(&self, x: f32, y: f32) -> Option<TextSelection> {
        self.note_point(x, y)?;
        let hit = self.anchor_at(x, y)?;
        let index = self.note_anchor_index(&hit)?;
        let block = &self.note.source.blocks[index];
        let view = self.flow.view(&self.note.source, block).ok()?;
        if !view.source_is_present(hit.offset) {
            return None;
        }
        let offset = view.project_offset(hit.offset, SourceAffinity::After)?;
        let word = word_range(view.text(), offset)?;
        let start = self.note_anchor(
            block.key,
            view.source_offset(word.start, SourceAffinity::After)?,
        )?;
        let end = self.note_anchor(
            block.key,
            view.source_offset(word.end, SourceAffinity::Before)?,
        )?;
        self.note_select_range(&start, &end)
    }

    /// Resolve a drag caret in local text-column and foreign-document coordinates.
    pub(crate) fn note_caret_at(&self, x: f32, y: f32) -> Option<Anchor> {
        self.note_point(x, y)?;
        let range = 0.0..self.extent;
        for paragraph in self.renderer.paragraphs_in(range) {
            let local_y = y - paragraph.y;
            let local_x = x - paragraph.x;
            let child = paragraph.text_box_at(local_x, local_y);
            let (layout, x, y) = child.map_or((&paragraph.layout, local_x, local_y), |child| {
                (&child.layout, local_x - child.x, local_y - child.y)
            });
            if !paragraph.hit_columns().contains(&(local_x + paragraph.x))
                || !(0.0..layout.height()).contains(&y)
                || Cluster::from_point(layout, x, y).is_none()
            {
                continue;
            }
            let cursor = Cursor::from_point(layout, x, y);
            let affinity = match cursor.affinity() {
                Affinity::Upstream => SourceAffinity::Before,
                Affinity::Downstream => SourceAffinity::After,
            };
            let visible = match child {
                Some(child) => child.source.start + cursor.index(),
                None => paragraph.visible_text_offset(cursor.index(), affinity)?,
            };
            let (key, offset) = paragraph.source_location(visible, affinity)?;
            return self.note_anchor(key, offset);
        }
        None
    }

    /// Extract the visible wording between canonical foreign-note anchors.
    /// Endpoints may be reversed, but must identify this exact book, chapter,
    /// source revision, and retained note subtree.
    pub(crate) fn note_select_range(&self, start: &Anchor, end: &Anchor) -> Option<TextSelection> {
        let spans = self.note_spans(start, end)?;
        let mut quote = String::new();
        for (offset, block) in self.note.source.blocks[spans.first..=spans.last]
            .iter()
            .enumerate()
        {
            let index = spans.first + offset;
            let Some(span) = spans.in_block(index, block.text.len()) else {
                continue;
            };
            let view = self.flow.view(&self.note.source, block).ok()?;
            let mut part = String::new();
            for range in view.layout_spans(span)? {
                part.push_str(view.text().get(range)?);
            }
            if part.is_empty() {
                continue;
            }
            if !quote.is_empty() {
                quote.push('\n');
            }
            quote.push_str(&part);
        }
        if quote.trim().is_empty() {
            return None;
        }
        Some(TextSelection {
            start: self.note_anchor(self.note.source.blocks[spans.first].key, spans.first_byte)?,
            end: self.note_anchor(self.note.source.blocks[spans.last].key, spans.last_byte)?,
            text: quote,
        })
    }

    /// The complete visible note text, in canonical source order. The initial
    /// cross-part layout only permits a complete note that fits in its band.
    pub(crate) fn note_visible_text(&self) -> Option<String> {
        let mut lines = self.lines.iter().filter(|line| !line.bytes.is_empty());
        let first = lines.next()?;
        let last = lines.last().unwrap_or(first);
        let (first_key, first_span) = first.sources().next()?;
        let (last_key, last_span) = last.sources().last()?;
        let start = self.note_anchor(first_key, first_span.start)?;
        let end = self.note_anchor(last_key, last_span.end)?;
        self.note_select_range(&start, &end)
            .map(|selection| selection.text)
    }

    /// Rectangles use the foreign note's own screen coordinates, including
    /// its left/top margins. An overlay may add its y offset directly.
    pub(crate) fn note_selection_rects(&self, selection: &TextSelection) -> Vec<HighlightRect> {
        let Some(spans) = self.note_spans(&selection.start, &selection.end) else {
            return Vec::new();
        };
        let page = 0.0..self.extent;
        let mut rects = Vec::new();
        for paragraph in self.renderer.paragraphs_in(page.clone()) {
            // Joined notes are explicitly rejected by cross_part::Note::layout.
            if paragraph.joined_note.is_some() {
                return Vec::new();
            }
            let Some(index) = self.note_block_index(paragraph.key) else {
                continue;
            };
            let block = &self.note.source.blocks[index];
            let Some(span) = spans.in_block(index, block.text.len()) else {
                continue;
            };
            let Ok(view) = self.flow.view(&self.note.source, block) else {
                continue;
            };
            let Some(layout_spans) = view.layout_spans(span) else {
                continue;
            };
            let columns = paragraph.paint_columns(self.settings);
            let geometry = Geometry {
                layout: &paragraph.layout,
                x: paragraph.x,
                y: paragraph.y,
                placed_boxes: paragraph.inline_text.is_some() || !block.images.is_empty(),
            };
            for span in layout_spans {
                geometry.append_flow(paragraph, span, &page, self.settings, &columns, &mut rects);
            }
        }
        rects
    }

    /// Find a link under a foreign note point and retain its canonical source.
    pub(crate) fn note_reference_at(
        &self,
        x: f32,
        y: f32,
    ) -> Option<(crate::footnotes::ReferenceContext, TextSelection)> {
        self.note_point(x, y)?;
        let hit = self.anchor_at(x, y)?;
        let block = &self.note.source.blocks[self.note_anchor_index(&hit)?];
        let link = block
            .links
            .iter()
            .find(|link| link.bytes.contains(&hit.offset))?;
        let linked = block.text.get(link.bytes.clone())?;
        let label = linked.trim();
        if label.is_empty() {
            return None;
        }
        let first = link.bytes.start + linked.len() - linked.trim_start().len();
        let selection = self.note_select_range(
            &self.note_anchor(block.key, first)?,
            &self.note_anchor(block.key, first.checked_add(label.len())?)?,
        )?;
        Some((
            crate::footnotes::ReferenceContext {
                kind: link.note_reference,
                label: label.chars().take(64).collect(),
                in_prose: block
                    .text
                    .get(..link.bytes.start)?
                    .chars()
                    .chain(block.text.get(link.bytes.end..)?.chars())
                    .any(char::is_alphanumeric),
                source_block: block.key,
            },
            selection,
        ))
    }

    /// Bounded foreign link labels, hrefs and note-local screen rectangles.
    pub(crate) fn note_links(&self) -> Vec<(String, &str, Vec<HighlightRect>)> {
        const MAX_FOREIGN_LINKS: usize = 128;
        let mut result = Vec::new();
        for block in &self.note.source.blocks {
            if !self.note.blocks.contains(&block.key) {
                continue;
            }
            for link in &block.links {
                if result.len() == MAX_FOREIGN_LINKS {
                    return Vec::new();
                }
                let Some(label) = block.text.get(link.bytes.clone()).map(str::trim) else {
                    return Vec::new();
                };
                if label.is_empty() {
                    continue;
                }
                let (Some(start), Some(end)) = (
                    self.note_anchor(block.key, link.bytes.start),
                    self.note_anchor(block.key, link.bytes.end),
                ) else {
                    continue;
                };
                let Some(selection) = self.note_select_range(&start, &end) else {
                    continue;
                };
                let rects = self.note_selection_rects(&selection);
                if !rects.is_empty() {
                    result.push((label.to_owned(), link.href.as_str(), rects));
                }
            }
        }
        result
    }

    pub(crate) fn note_contains_anchor(&self, anchor: &Anchor) -> bool {
        self.note_anchor_index(anchor).is_some()
    }

    fn note_point(&self, x: f32, y: f32) -> Option<()> {
        (x.is_finite()
            && y.is_finite()
            && (0.0..self.settings.text_width() as f32).contains(&x)
            && (0.0..self.extent).contains(&y))
        .then_some(())
    }

    fn note_block_index(&self, key: usize) -> Option<usize> {
        self.note.blocks.contains(&key).then_some(())?;
        self.note
            .source
            .blocks
            .binary_search_by_key(&key, |block| block.key)
            .ok()
    }

    fn note_anchor(&self, key: usize, offset: usize) -> Option<Anchor> {
        let block = &self.note.source.blocks[self.note_block_index(key)?];
        if offset > block.text.len() || !block.text.is_char_boundary(offset) {
            return None;
        }
        Some(Anchor {
            raw_caret: None,
            book: self.note.book,
            chapter: self.note.chapter.clone(),
            revision: self.note.source.revision,
            block: key,
            offset,
            illustration: None,
        })
    }

    fn note_anchor_index(&self, anchor: &Anchor) -> Option<usize> {
        if anchor.book != self.note.book
            || anchor.chapter != self.note.chapter
            || anchor.revision != self.note.source.revision
            || anchor.illustration.is_some()
        {
            return None;
        }
        let index = self.note_block_index(anchor.block)?;
        let block = &self.note.source.blocks[index];
        (anchor.offset <= block.text.len() && block.text.is_char_boundary(anchor.offset))
            .then_some(index)
    }

    fn note_spans(&self, start: &Anchor, end: &Anchor) -> Option<Spans> {
        let a = (self.note_anchor_index(start)?, start.offset);
        let b = (self.note_anchor_index(end)?, end.offset);
        if a == b {
            return None;
        }
        let ((first, first_byte), (last, last_byte)) = if a < b { (a, b) } else { (b, a) };
        if self.note.source.blocks[first..=last]
            .iter()
            .any(|block| !self.note.blocks.contains(&block.key))
        {
            return None;
        }
        Some(Spans {
            first,
            last,
            first_byte,
            last_byte,
        })
    }
}

struct Geometry<'a> {
    layout: &'a Layout<crate::parley_layout::PaintBrush>,
    x: f32,
    y: f32,
    placed_boxes: bool,
}

impl Geometry<'_> {
    fn append_flow(
        &self,
        paragraph: &crate::parley_layout::Paragraph,
        span: Range<usize>,
        page: &Range<f32>,
        settings: Settings,
        columns: &Range<usize>,
        rects: &mut Vec<HighlightRect>,
    ) {
        let Some(boxed) = &paragraph.inline_text else {
            self.append(span, page, settings, columns, rects);
            return;
        };
        for part in boxed.layout_spans(&span) {
            self.append(part, page, settings, columns, rects);
        }
        for child in &boxed.boxes {
            if let Some(part) = child.local_span(&span) {
                Geometry {
                    layout: &child.layout,
                    x: self.x + child.x,
                    y: self.y + child.y,
                    placed_boxes: false,
                }
                .append(part, page, settings, columns, rects);
            }
        }
    }

    fn append(
        &self,
        span: Range<usize>,
        page: &Range<f32>,
        settings: Settings,
        columns: &Range<usize>,
        rects: &mut Vec<HighlightRect>,
    ) {
        let Self {
            layout,
            x: left,
            y: origin,
            placed_boxes,
        } = *self;
        let Some(span) = visible_span(layout, origin, page, span) else {
            return;
        };
        let start = Cursor::from_byte_index(layout, span.start, Affinity::Downstream);
        let boundary = Cluster::from_byte_index(layout, span.end - 1)
            .map_or(span.end, |cluster| cluster.text_range().end);
        let end = Cursor::from_byte_index(layout, boundary, Affinity::Upstream);
        Selection::new(start, end).geometry_with(layout, |mut rect, row| {
            rect.x0 += f64::from(left);
            rect.x1 += f64::from(left);
            // Parley includes replaced elements at the same caret position
            // in selection rectangles. Highlight the surrounding text runs.
            if placed_boxes && let Some(line) = layout.get(row) {
                for item in line.items() {
                    let parley::PositionedLayoutItem::InlineBox(image) = item else {
                        continue;
                    };
                    let right = f64::from(left + image.x + image.width);
                    let left = f64::from(left + image.x);
                    if left >= rect.x1 || right <= rect.x0 {
                        continue;
                    }
                    if rect.x0 < left
                        && let Some(left_run) = page_rect(
                            BoundingBox { x1: left, ..rect },
                            origin,
                            page,
                            settings,
                            columns,
                        )
                    {
                        rects.push(left_run);
                    }
                    rect.x0 = right;
                    if rect.x0 >= rect.x1 {
                        return;
                    }
                }
            }
            if let Some(rect) = page_rect(rect, origin, page, settings, columns) {
                rects.push(rect);
            }
        });
    }
}

struct PagePoint {
    page: Range<f32>,
    x: f32,
    y: f32,
    inside_columns: bool,
}

struct Spans {
    first: usize,
    last: usize,
    first_byte: usize,
    last_byte: usize,
}

impl Spans {
    fn in_block(&self, index: usize, length: usize) -> Option<Range<usize>> {
        if !(self.first..=self.last).contains(&index) {
            return None;
        }
        let start = if index == self.first {
            self.first_byte
        } else {
            0
        };
        let end = if index == self.last {
            self.last_byte
        } else {
            length
        };
        (start < end).then_some(start..end)
    }
}

pub(crate) fn word_ranges(text: &str) -> impl Iterator<Item = Range<usize>> + '_ {
    let words = WordSegmenter::new_auto(WordBreakInvariantOptions::default());
    let mut start = 0;
    words
        .segment_str(text)
        .iter_with_word_type()
        .filter_map(move |(end, word_type)| {
            let range = start..end;
            start = end;
            // Combining marks can end a word segment that ICU does not tag as word-like.
            (word_type.is_word_like() || text[range.clone()].chars().any(char::is_alphanumeric))
                .then_some(range)
        })
}

/// Unicode word context with a scalar cap for space-free text and long tokens.
pub(crate) fn bounded_context(text: &str, words: usize, from_end: bool) -> String {
    bounded_context_range(text, words, from_end)
        .map_or_else(String::new, |range| text[range].to_owned())
}

pub(crate) fn bounded_context_range(
    text: &str,
    words: usize,
    from_end: bool,
) -> Option<Range<usize>> {
    if words == 0 {
        return None;
    }
    let offset = if from_end {
        text.char_indices()
            .rev()
            .nth(2047)
            .map_or(0, |(index, _)| index)
    } else {
        0
    };
    let bounded = if from_end {
        &text[offset..]
    } else {
        &text[..text
            .char_indices()
            .nth(2048)
            .map_or(text.len(), |(index, _)| index)]
    };
    let ranges: Vec<_> = word_ranges(bounded).collect();
    let range = if from_end {
        ranges.get(ranges.len().saturating_sub(words))?.start..ranges.last()?.end
    } else {
        ranges.first()?.start
            ..ranges
                .get((words - 1).min(ranges.len().saturating_sub(1)))?
                .end
    };
    let selected = &bounded[range.clone()];
    let clipped = if from_end {
        range.start
            + selected
                .char_indices()
                .rev()
                .nth(511)
                .map_or(0, |(index, _)| index)..range.end
    } else {
        range.start
            ..range.start
                + selected
                    .char_indices()
                    .nth(512)
                    .map_or(selected.len(), |(index, _)| index)
    };
    Some(offset + clipped.start..offset + clipped.end)
}

pub(crate) fn word_range(text: &str, offset: usize) -> Option<Range<usize>> {
    word_ranges(text)
        .find(|range| range.end > offset)
        .filter(|range| range.start <= offset)
}

fn visible_span(
    layout: &Layout<impl parley::style::Brush>,
    origin: f32,
    page: &Range<f32>,
    span: Range<usize>,
) -> Option<Range<usize>> {
    let first = layout
        .lines()
        .find(|line| origin + line.metrics().content_block_max_coord > page.start)?;
    let last = layout
        .lines()
        .rev()
        .find(|line| origin + line.metrics().content_block_min_coord < page.end)?;
    let start = span.start.max(first.text_range().start);
    let end = span.end.min(last.text_range().end);
    (start < end).then_some(start..end)
}

fn page_rect(
    rect: BoundingBox,
    origin: f32,
    page: &Range<f32>,
    settings: Settings,
    columns: &Range<usize>,
) -> Option<HighlightRect> {
    if ![rect.x0, rect.y0, rect.x1, rect.y1]
        .iter()
        .all(|edge| edge.is_finite())
    {
        return None;
    }
    let left_margin = f64::from(settings.left_margin());
    let top_margin = f64::from(settings.top_margin());
    let left_edge = columns.start as f64;
    let right_edge = columns.end as f64;
    let bottom = top_margin
        + f64::from(
            (page.end - page.start)
                .ceil()
                .min(settings.text_height() as f32),
        );
    let y = top_margin + f64::from(origin - page.start);
    let left = (rect.x0 + left_margin).floor().clamp(left_edge, right_edge) as i32;
    let top = (rect.y0 + y).floor().clamp(top_margin, bottom) as i32;
    let right = (rect.x1 + left_margin).ceil().clamp(left_edge, right_edge) as i32;
    let bottom = (rect.y1 + y).ceil().clamp(top_margin, bottom) as i32;
    (right > left && bottom > top).then_some(HighlightRect {
        x: left,
        y: top,
        width: right - left,
        height: bottom - top,
    })
}

#[cfg(test)]
#[path = "selection/cross_part_tests.rs"]
mod cross_part_tests;

#[cfg(test)]
mod tests {

    use crate::Engine;

    use super::*;

    #[test]
    fn selects_unicode_words_and_skips_punctuation() {
        let text = "A cafe\u{301}, العربية. こんにちは世界";
        for expected in ["cafe\u{301}", "العربية", "こんにちは", "世界"] {
            let position = text.find(expected).unwrap();
            let word = word_range(text, position).unwrap();
            assert_eq!(&text[word], expected);
        }
        assert!(word_range(text, text.find(',').unwrap()).is_none());
    }

    #[test]
    fn independent_margins_keep_words_and_highlights_in_the_painted_viewport() {
        let fonts = [crate::test_fixture("fonts/Silkscreen-Regular.ttf")];
        let book = crate::test_fixture("field-notes-epub3.epub");
        let settings = Settings {
            width: 460,
            height: 700,
            margin_sides: Some(68),
            margin_top: Some(86),
            margin_bottom: Some(33),
            ..Settings::default()
        };
        let mut probe = Probe::open(&book, Engine::Parley, &fonts, settings, 0).unwrap();
        let start = probe.matches("The tide keeps its own time.")[0].clone();
        let mut end = start.clone();
        end.offset += "The".len();
        let selection = probe.select_range(&start, &end).unwrap();
        let page = probe.page_for(&start).unwrap();
        let rect = probe.selection_rects(page, &selection)[0];
        assert!(rect.x >= 68 && rect.x + rect.width <= 392);
        assert!(rect.y >= 86 && rect.y + rect.height <= 667);
        assert_eq!(
            probe
                .word_at(
                    page,
                    rect.x as f32 + 5.0,
                    rect.y as f32 + rect.height as f32 / 2.0
                )
                .unwrap()
                .text,
            "The",
        );
        let pixels = probe.capture(page).unwrap();
        let stride = settings.width as usize;
        assert!(pixels[..86 * stride].iter().all(|&pixel| pixel == 255));
        assert!(pixels[667 * stride..].iter().all(|&pixel| pixel == 255));
    }

    #[test]
    fn locates_and_draws_a_highlight_across_blocks_and_pages() {
        let font = crate::test_fixture("fonts/Silkscreen-Regular.ttf");
        let book = crate::test_fixture("field-notes-epub3.epub");
        let settings = Settings {
            width: 360,
            height: 420,
            font_size: 32.0,
            margin: 18,
            line_height: 1.4,
            ..Settings::default()
        };
        let probe = Probe::open(
            &book,
            Engine::Parley,
            std::slice::from_ref(&font),
            settings,
            0,
        )
        .unwrap();
        let repeats = probe.matches("The tide keeps its own time.");
        let first = &repeats[0];
        let finish = probe.fragment("lantern").unwrap();
        let span = probe.select_range(&finish, first).unwrap();
        assert_eq!(&span.start, first);
        assert_eq!(span.end, finish);
        assert!(span.text.contains("Morning observation"));
        assert!(span.text.matches("The tide keeps its own time.").count() >= 2);
        let serialized = serde_json::to_vec(&span).unwrap();
        let restored: TextSelection = serde_json::from_slice(&serialized).unwrap();
        assert_eq!(restored, span);

        let second_layout = Settings {
            width: 420,
            height: 360,
            font_size: 37.0,
            ..settings
        };
        for layout in [settings, second_layout] {
            let probe = Probe::open(
                &book,
                Engine::Parley,
                std::slice::from_ref(&font),
                layout,
                0,
            )
            .unwrap();
            let pages: Vec<_> = (0..probe.page_count())
                .map(|page| probe.selection_rects(page, &span))
                .collect();
            assert!(pages.iter().filter(|rects| !rects.is_empty()).count() >= 2);
            assert_eq!(
                probe.select_range(&span.start, &span.end).as_ref(),
                Some(&span)
            );
            let rectangle = probe.selection_rects(probe.page_for(&span.start).unwrap(), &span)[0];
            let x = rectangle.x as f32 + 5.0;
            let y = rectangle.y as f32 + rectangle.height as f32 / 2.0;
            let page = probe.page_for(&span.start).unwrap();
            assert_eq!(probe.word_at(page, x, y).unwrap().text, "The");
            assert_eq!(
                probe.text_position_at(page, x, y).unwrap().block,
                span.start.block
            );
        }

        let mut stale = span.clone();
        stale.start.revision ^= 1;
        assert!(probe.selection_rects(0, &stale).is_empty());
    }
}
