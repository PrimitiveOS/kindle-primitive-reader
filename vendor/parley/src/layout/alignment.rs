// Copyright 2024 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use super::BreakReason;
use alloc::vec::Vec;
use parley_engine::ShapedText;

use crate::data::{LayoutData, LayoutItemKind, LineData, LineItemData};
use crate::style::Brush;

/// Alignment of a layout.
#[derive(Copy, Clone, Default, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Alignment {
    /// This is [`Alignment::Left`] for LTR text and [`Alignment::Right`] for RTL text.
    #[default]
    Start,
    /// This is [`Alignment::Right`] for LTR text and [`Alignment::Left`] for RTL text.
    End,
    /// Align content to the left edge.
    ///
    /// For alignment that should be aware of text direction, use [`Alignment::Start`] or
    /// [`Alignment::End`] instead.
    Left,
    /// Align each line centered within the container.
    Center,
    /// Align content to the right edge.
    ///
    /// For alignment that should be aware of text direction, use [`Alignment::Start`] or
    /// [`Alignment::End`] instead.
    Right,
    /// Justify each line by spacing out content.
    ///
    /// The last line of a paragraph is start-aligned instead, unless
    /// [`AlignmentOptions::last_line_alignment`] is set.
    Justify,
}

/// Additional options to fine tune alignment
#[derive(Debug, Clone, Copy)]
pub struct AlignmentOptions {
    /// If set to `true`, "end" and "center" alignment will apply even if the line contents are
    /// wider than the alignment width. If it is set to `false`, all overflowing lines will be
    /// [`Alignment::Start`] aligned.
    pub align_when_overflowing: bool,
    /// The alignment of the last line of each paragraph: the last line of the layout and every
    /// line that ends in an explicit line break. This corresponds to the CSS `text-align-last`
    /// property.
    ///
    /// If set to `None`, those lines use the alignment passed to [`Layout::align`], except that
    /// [`Alignment::Justify`] falls back to [`Alignment::Start`]. This is the behavior of
    /// `text-align-last: auto`.
    ///
    /// [`Layout::align`]: crate::Layout::align
    pub last_line_alignment: Option<Alignment>,
}

#[expect(
    clippy::derivable_impls,
    reason = "Make default values explicit rather than relying on the implicit default value of bool"
)]
impl Default for AlignmentOptions {
    fn default() -> Self {
        Self {
            align_when_overflowing: false,
            last_line_alignment: None,
        }
    }
}

pub(crate) struct WordExpansion<'a> {
    pub(crate) text: &'a str,
    pub(crate) max_percent: u8,
    pub(crate) eligible: &'a dyn Fn(&str) -> bool,
}

/// Align the layout.
pub(crate) fn align<B: Brush>(
    layout: &mut LayoutData<B>,
    alignment: Alignment,
    options: AlignmentOptions,
    expansion: Option<WordExpansion<'_>>,
) {
    layout.alignment = Some(alignment);
    layout.expanded_gaps.clear();
    layout.punctuation_overflow = [0.0; 2];

    let is_rtl = layout.base_level.is_rtl();

    // Apply alignment to line items
    for (index, line) in layout.lines.iter_mut().enumerate() {
        line.justification.amount_per_opportunity = 0.;
        line.expanded_gaps = 0..0;

        let indent = line.metrics.indent;
        let is_last_line = matches!(line.break_reason, BreakReason::None | BreakReason::Explicit);
        let line_alignment = if !is_last_line {
            alignment
        } else if let Some(last_line_alignment) = options.last_line_alignment {
            last_line_alignment
        } else if alignment == Alignment::Justify {
            Alignment::Start
        } else {
            alignment
        };
        let [left, right] = layout
            .punctuation_shifts
            .get(index)
            .copied()
            .unwrap_or_default();
        let [left, right] = match (line_alignment, is_rtl) {
            (Alignment::Left, _) | (Alignment::Start, false) | (Alignment::End, true) => [left, 0.0],
            (Alignment::Right, _) | (Alignment::Start, true) | (Alignment::End, false) => [0.0, right],
            (Alignment::Justify, _) => [left, right],
            (Alignment::Center, _) => [0.0; 2],
        };
        layout.punctuation_overflow[0] = layout.punctuation_overflow[0].max(left);
        layout.punctuation_overflow[1] = layout.punctuation_overflow[1].max(right);
        line.metrics.offset = if is_rtl {
            -line.metrics.hanging_advance - left
        } else {
            indent - left
        };

        // Optical shifts change alignment and justification, not line breaking.
        let line_width = line.metrics.inline_max_coord - line.metrics.inline_min_coord;
        let free_space = line_width - indent - line.metrics.advance
            + line.metrics.hanging_advance + left + right;

        if !options.align_when_overflowing && free_space <= 0.0 {
            if is_rtl {
                line.metrics.offset += free_space;
            }
            continue;
        }

        match (line_alignment, is_rtl) {
            (Alignment::Left, _) | (Alignment::Start, false) | (Alignment::End, true) => {
                // Do nothing
            }
            (Alignment::Right, _) | (Alignment::Start, true) | (Alignment::End, false) => {
                line.metrics.offset += free_space;
            }
            (Alignment::Center, _) => {
                line.metrics.offset += free_space * 0.5;
            }
            (Alignment::Justify, _) => {
                // Justified alignment doesn't have any effect if free_space is negative or zero
                if free_space <= 0.0 {
                    continue;
                }

                // Justified alignment doesn't apply if there are no whitespace gaps to adjust. In
                // that case, start-align, i.e., left-align for LTR text and right-align for RTL
                // text.
                if line.num_justification_opportunities == 0 {
                    if is_rtl {
                        line.metrics.offset += free_space;
                    }
                    continue;
                }

                let mut space = free_space;
                if let Some(expansion) = &expansion {
                    let usable_width = line_width - indent;
                    if free_space > usable_width * 0.05 {
                        let from = layout.expanded_gaps.len();
                        space -= expand_words(
                            &layout.shaped_text,
                            &layout.line_items[line.item_range.clone()],
                            line,
                            expansion,
                            free_space,
                            usable_width * 0.05,
                            &mut layout.expanded_gaps,
                        );
                        line.expanded_gaps = from..layout.expanded_gaps.len();
                    }
                }
                line.justification.amount_per_opportunity =
                    space / line.num_justification_opportunities as f32;
            }
        }
    }
}

/// Spend part of the excess width on tracking while retaining the shaped glyphs.
fn expand_words(
    shaped: &ShapedText,
    items: &[LineItemData],
    line: &LineData,
    expansion: &WordExpansion<'_>,
    free_space: f32,
    tolerance: f32,
    all: &mut Vec<(u32, f32)>,
) -> f32 {
    let target = free_space - tolerance;
    let end = line.justification.justification_end_cluster;
    let start = all.len();
    let mut largest: f32 = 0.0;
    for item in items
        .iter()
        .filter(|item| item.kind == LayoutItemKind::TextRun)
    {
        let font_size = shaped.runs()[item.index].font_size;
        let atoms = shaped
            .run_slice(item.index as u32)
            .narrow(item.shaped_cluster_range.clone())
            .atoms_start();
        for atom in atoms {
            let cluster = atom.shaped_clusters_range();
            if cluster.end > end || (cluster.end == end && line.soft_hyphen.is_none()) {
                continue;
            }
            let characters = atom.characters();
            if characters[0].is_whitespace() || atom.advance() <= 0.0 {
                continue;
            }
            let bytes = characters[0].text_byte_range().start
                ..characters.last().unwrap().text_byte_range().end;
            if expansion.text.get(bytes).is_some_and(expansion.eligible) {
                largest = largest.max(font_size);
                all.push((cluster.start, font_size));
            }
        }
    }
    let candidates = &mut all[start..];
    let cap = (largest * f32::from(expansion.max_percent) / 100.0) as u32;
    let cap = cap.min(u32::MAX - 1);
    if cap == 0 || candidates.is_empty() {
        all.truncate(start);
        return 0.0;
    }
    // Test pixel increments in the largest font, proportionally smaller in
    // other runs. Respect the maximum and leave enough width for spaces.
    let per_atom = |step: u32, size: f32| (step as f32 * size / largest) as u32 as f32;
    let total = |step| {
        candidates
            .iter()
            .map(|&(_, size)| per_atom(step, size))
            .sum::<f32>()
    };
    let mut low = 1;
    let mut high = cap;
    let (mut below, mut meets) = (0, None);
    while low <= high {
        let step = low + (high - low) / 2;
        let width = total(step);
        if width >= target {
            if width <= free_space {
                meets = Some(step);
            }
            high = step - 1;
        } else {
            below = step;
            low = step + 1;
        }
    }
    let step = meets.unwrap_or(below);
    if step == 0 {
        all.truncate(start);
        return 0.0;
    }
    let spent = total(step);
    for (_, size) in &mut *candidates {
        *size = per_atom(step, *size);
    }
    candidates.sort_unstable_by_key(|gap| gap.0);
    spent
}
