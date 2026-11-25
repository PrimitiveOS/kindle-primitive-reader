//! Intrinsic sizing and column widths for tables in the horizontal reading view.
use crate::{
    book_style::{ImageLength, Style},
    source::{Block, Source},
    table_layout::{Cell, TableKey},
};
use std::collections::BTreeMap;

// Larger, nested or illustrated grids retain the bounded span-aware layout.
const MAX_MEASURED_BYTES: usize = 256 * 1024;
const MAX_MEASURED_BLOCKS: usize = 2048;

#[derive(Default)]
struct Column {
    min: u32,
    max: u32,
    sum: u64,
    count: u32,
    percent: u32,
    fixed: u32,
}

impl Column {
    fn observe(&mut self, style: &Style, min: u32, max: u32, font: f32) {
        let outer_min = intrinsic_width(min, style, font);
        let outer_max = intrinsic_width(max, style, font);
        self.min = self.min.max(outer_min);
        self.max = self.max.max(outer_max);
        self.sum += u64::from(outer_max);
        self.count += 1;
    }

    fn prefer_hint(&mut self, style: &Style, font: f32, width: u32, same_kind_only: bool) {
        match style.image_width {
            Some(ImageLength::Fraction(ratio)) => {
                if !same_kind_only || self.fixed == 0 {
                    self.percent = self
                        .percent
                        .max(((ratio * 25600.).round() / 256.).clamp(0., 100.) as u32);
                }
            }
            Some(hint) if !same_kind_only || self.percent == 0 => {
                let pixels = hint.pixels(width, font * style.size_scale).unwrap_or(0.);
                self.fixed = self.fixed.max(pixels.clamp(0., width as f32) as u32);
            }
            _ => (),
        }
    }
}

// Percentage padding depends on the final outer width. Invert that relationship
// while measuring a cell with no assigned width yet.
fn intrinsic_width(content: u32, style: &Style, font: f32) -> u32 {
    let mut width = content;
    let mut percent = 0.0;
    let mut terms = 0;
    for edge in &style.padding {
        match edge {
            crate::book_style::CssLength::Fraction(r) if *r > 0. => {
                percent += (r * 25600.).round() / 25600.;
                terms += 1;
            }
            _ => width = width.saturating_add(edge.box_pixels(0, font) as u32),
        }
    }
    if percent > 0. && percent < 1. {
        width
            .saturating_add((width as f64 * f64::from(percent) / (1. - f64::from(percent))) as u32)
            .saturating_add(terms)
    } else {
        width
    }
}

pub(super) struct TableWidths {
    columns: Vec<Column>,
    edges: Vec<u32>,
    has_nowrap: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum NativeMode {
    Book,
    Web,
    Float,
}

impl TableWidths {
    pub(super) fn collect(
        source: &Source,
        styles: &[Style],
        tables: &[Option<usize>],
        owners: &[Option<usize>],
        cells: &[Option<Cell>],
        flow: &crate::table_layout::TableFlow<'_>,
        font: f32,
        width: u32,
        borders: Option<&crate::table_borders::Borders>,
        stock_native: bool,
        mut measure: impl FnMut(&Block) -> (u32, u32),
    ) -> BTreeMap<usize, Self> {
        let mut groups: BTreeMap<usize, Vec<(&Block, Cell)>> = BTreeMap::new();
        for block in &source.blocks {
            if !styles[block.element].flow.suppressed
                && let Some(cell) = flow.block_cell(block)
                && matches!(cell.table, TableKey::Native(_))
                && let Some(table) = tables[block.element]
            {
                groups.entry(table).or_default().push((block, cell));
            }
        }
        let mut column_styles = BTreeMap::<usize, Vec<&Style>>::new();
        if stock_native {
            for (index, element) in source.elements.iter().enumerate() {
                if element.tag == "col"
                    && !styles[index].flow.suppressed
                    && let Some(table) = tables[index]
                {
                    column_styles.entry(table).or_default().push(&styles[index]);
                }
            }
        }
        let mut out = BTreeMap::new();
        let mut bytes = MAX_MEASURED_BYTES;
        let mut count = MAX_MEASURED_BLOCKS;
        for (table, blocks) in groups {
            let nowrap = blocks.iter().any(|(block, _)| {
                styles[block.element].inline_nowrap
                    || block
                        .styled_text
                        .iter()
                        .any(|run| styles[run.element].inline_nowrap)
            });
            if !stock_native
                && !nowrap
                && !styles[table].border_collapse
                && !blocks
                    .iter()
                    .filter_map(|(b, _)| owners[b.element])
                    .any(|i| {
                        styles[i]
                            .image_width
                            .and_then(ImageLength::css_length)
                            .is_some()
                    })
            {
                continue;
            }
            let text_bytes = blocks.iter().map(|(b, _)| b.text.len()).sum::<usize>();
            if blocks.len() > count
                || text_bytes > bytes
                || blocks.iter().any(|(block, cell)| {
                    (!stock_native && cell.has_column_tracks())
                        || (!stock_native && !nowrap && cell.span != 1)
                        || !block.images.is_empty()
                })
            {
                continue;
            }
            count -= blocks.len();
            bytes -= text_bytes;
            let mut demands: BTreeMap<usize, (u32, u32)> = BTreeMap::new();
            let columns = usize::from(blocks[0].1.columns);
            for (block, _) in blocks {
                if let Some(owner) = owners[block.element] {
                    let (min, max) = measure(block);
                    let demand = demands.entry(owner).or_default();
                    demand.0 = demand.0.max(min);
                    demand.1 = demand.1.max(max);
                }
            }
            let mut tracks: Vec<_> = (0..columns).map(|_| Column::default()).collect();
            if let Some(columns) = column_styles.get(&table) {
                for (track, style) in tracks.iter_mut().zip(columns) {
                    track.prefer_hint(style, font, width, true);
                }
            }
            let mut spanning = Vec::new();
            for (owner, (min, max)) in demands {
                let Some(cell) = cells[owner] else { continue };
                let style = &styles[owner];
                let stroke = borders.and_then(|b| b.cells.get(&cell)).map_or_else(
                    || {
                        if stock_native && !styles[table].border_collapse {
                            2 * (font * 0.22).round() as u32
                        } else {
                            0
                        }
                    },
                    |c| c.edges[3].width + c.edges[1].width,
                );
                let min = min.saturating_add(stroke);
                let max = max.saturating_add(stroke);
                if cell.span == 1 {
                    let track = &mut tracks[usize::from(cell.column)];
                    track.observe(style, min, max, font);
                    track.prefer_hint(style, font, width, stock_native);
                } else {
                    spanning.push((cell, style, min, max));
                }
            }
            // A merged cell contributes only the width its existing tracks cannot supply.
            spanning.sort_by_key(|(cell, ..)| cell.span);
            for (cell, style, min, max) in spanning {
                let start = usize::from(cell.column);
                let span = usize::from(cell.span);
                let tracks = &mut tracks[start..start + span];
                let min = intrinsic_width(min, style, font);
                let max = intrinsic_width(max, style, font);
                let occupied_min = tracks.iter().fold(0u32, |sum, c| sum.saturating_add(c.min));
                let occupied_max = tracks.iter().fold(0u32, |sum, c| sum.saturating_add(c.max));
                let mut short_min = min.saturating_sub(occupied_min);
                let mut short_max = max.saturating_sub(occupied_max);
                for (idx, track) in tracks.iter_mut().enumerate() {
                    let remaining = (span - idx) as u32;
                    let extra_min = short_min.div_ceil(remaining);
                    let extra_max = short_max.div_ceil(remaining);
                    short_min -= extra_min;
                    short_max -= extra_max;
                    track.min += extra_min;
                    track.max = (track.max + extra_max).max(track.min);
                }
            }
            out.insert(
                table,
                Self {
                    columns: tracks,
                    edges: Vec::new(),
                    has_nowrap: nowrap,
                },
            );
        }
        out
    }

    /// Size the whole native table before applying cell spacing and track edges.
    /// Book keeps normal explicitly sized tables at container width. The
    /// surrounding float establishes an explicit width even in Book mode.
    pub(super) fn native_width(
        tracks: Option<&Self>,
        style: &Style,
        available: u32,
        font: f32,
        border_width: u32,
        spacing: u32,
        mode: NativeMode,
    ) -> u32 {
        let outer = |width: u32| {
            let width = width.saturating_add(border_width);
            if style.border_collapse {
                width
            } else {
                intrinsic_width(width, style, font)
            }
        };
        let min = tracks.map_or(1, |t| {
            let text = t
                .columns
                .iter()
                .fold(0u32, |sum, c| sum.saturating_add(c.min.max(1)));
            outer(text.saturating_add(spacing))
        });
        let declared = style
            .image_width
            .and_then(|width| width.pixels(available, font));
        let enforce_width = mode != NativeMode::Book;
        let preferred = match declared {
            Some(width) if enforce_width => {
                let width = width.max(0.0) as u32;
                if style.box_sizing == crate::book_style::BoxSizing::BorderBox
                    || (mode == NativeMode::Float && !style.box_sizing_explicit)
                {
                    width
                } else {
                    outer(width)
                }
            }
            Some(_) => available,
            None => tracks.map_or(available, |t| {
                let content = available.saturating_sub(border_width.saturating_add(spacing));
                let natural = t.columns.iter().fold(spacing, |sum, col| {
                    let percent = (u64::from(content) * u64::from(col.percent) / 100) as u32;
                    sum.saturating_add(col.max.max(col.fixed).max(percent).max(1))
                });
                outer(natural)
            }),
        };
        let bound = if enforce_width {
            let maximum = style
                .image_max_width
                .and_then(|width| width.pixels(available, font))
                .map_or(available, |width| width.max(0.0) as u32);
            let minimum = style
                .min_width
                .map_or(0, |width| width.box_pixels(available, font) as u32);
            preferred.min(maximum).max(minimum)
        } else {
            preferred
        };
        if enforce_width
            && declared.is_some_and(|width| width > 0.)
            && tracks.is_some_and(|t| t.has_nowrap)
        {
            // A publisher-sized table stays constrained; a nowrap cell can
            // fall back to natural breaks when the tracks cannot honor its min.
            bound.min(available)
        } else {
            bound.max(min).min(available)
        }
    }

    /// Simple CSS grids use the same measured column tracks as HTML tables.
    /// The elements are already classified, including repaired anonymous rows.
    pub(super) fn collect_css(
        source: &Source,
        styles: &[Style],
        flow: &crate::table_layout::TableFlow<'_>,
        font: f32,
        width: u32,
        mut measure: impl FnMut(&Block) -> (u32, u32),
    ) -> BTreeMap<usize, Self> {
        if !flow.has_repaired_css() {
            return BTreeMap::new();
        }
        let mut groups = BTreeMap::<usize, Vec<(&Block, Cell, usize)>>::new();
        for block in &source.blocks {
            if styles[block.element].flow.suppressed {
                continue;
            }
            let Some(owner) = flow.css_owner(block.element) else {
                continue;
            };
            let Some(cell) = flow.cell(owner) else {
                continue;
            };
            if let TableKey::Css(key) = cell.table {
                groups.entry(key).or_default().push((block, cell, owner));
            }
        }
        let mut out = BTreeMap::new();
        let mut remaining_bytes = MAX_MEASURED_BYTES;
        let mut remaining_blocks = MAX_MEASURED_BLOCKS;
        for (key, blocks) in groups {
            let bytes = blocks
                .iter()
                .map(|(block, _, _)| block.text.len())
                .sum::<usize>();
            if blocks.len() > remaining_blocks || bytes > remaining_bytes {
                continue;
            }
            remaining_bytes -= bytes;
            remaining_blocks -= blocks.len();
            let columns = usize::from(blocks[0].1.columns);
            let mut demands: BTreeMap<usize, (Cell, u32, u32)> = BTreeMap::new();
            for (block, cell, owner) in blocks {
                let (min, max) = measure(block);
                let demand = demands.entry(owner).or_insert((cell, 0, 0));
                demand.1 = demand.1.max(min);
                demand.2 = demand.2.max(max);
            }
            let mut tracks: Vec<_> = (0..columns).map(|_| Column::default()).collect();
            for (owner, (cell, min, max)) in demands {
                let style = &styles[owner];
                let track = &mut tracks[usize::from(cell.column)];
                track.observe(style, min, max, font);
                track.prefer_hint(style, font, width, false);
            }
            out.insert(
                key,
                Self {
                    columns: tracks,
                    edges: Vec::new(),
                    has_nowrap: false,
                },
            );
        }
        out
    }

    pub(super) fn resolve_css(&mut self, available: u32, explicit: bool) -> u32 {
        let natural = self
            .columns
            .iter()
            .fold(0u32, |width, col| width.saturating_add(col.max));
        let hinted = self.columns.iter().any(|c| c.percent > 0 || c.fixed > 0);
        let width = if explicit || hinted {
            available
        } else {
            natural.max(self.columns.len() as u32).min(available)
        };
        if self.edges.last() != Some(&width) {
            self.resolve(width);
        }
        width
    }

    /// Allocate the publisher's fixed and percentage widths first. Unspecified
    /// columns share the available space in proportion to their average cell
    /// width. A wide unbreakable word may displace even a specified column.
    pub(super) fn resolve(&mut self, width: u32) {
        let tracks = &self.columns;
        let autos = tracks
            .iter()
            .filter(|c| c.fixed == 0 && c.percent == 0)
            .count() as u32;
        let total_pct = tracks.iter().map(|c| c.percent).sum::<u32>();
        let total_fixed = tracks
            .iter()
            .filter(|c| c.percent == 0)
            .map(|c| c.fixed)
            .sum::<u32>();
        let scale_fixed = total_pct + 100 * total_fixed / width + 5 * autos > 100;
        let percents: Vec<_> = tracks
            .iter()
            .map(|c| {
                if c.percent > 0 {
                    c.percent
                } else if scale_fixed {
                    100 * c.fixed / width
                } else {
                    0
                }
            })
            .collect();
        let total = percents.iter().sum::<u32>();
        let limit = 100 - 3 * autos;
        let mut sizes: Vec<_> = tracks
            .iter()
            .zip(percents)
            .map(|(c, mut p)| {
                if total > limit && total > 0 {
                    p = p * limit / total;
                }
                if p > 0 {
                    width * p / 100
                } else if !scale_fixed {
                    c.fixed
                } else {
                    0
                }
            })
            .collect();
        let remaining = width.saturating_sub(sizes.iter().sum());
        let mut sum_max = 0u64;
        let mut sum_weight = 0u64;
        for (c, &size) in tracks.iter().zip(&sizes) {
            if size == 0 {
                sum_max += u64::from(c.max);
                sum_weight += c.sum / u64::from(c.count.max(1));
            }
        }
        for (c, size) in tracks.iter().zip(&mut sizes) {
            if *size == 0 {
                *size = if sum_max <= u64::from(remaining) {
                    c.max
                } else if sum_weight > 0 {
                    ((c.sum / u64::from(c.count.max(1))) * u64::from(remaining) / sum_weight) as u32
                } else {
                    0
                };
            }
            *size = (*size).max(c.min);
        }
        let total = sizes.iter().map(|&size| u64::from(size)).sum::<u64>();
        if total > u64::from(width) {
            for size in &mut sizes {
                *size = (u64::from(*size) * u64::from(width) / total) as u32;
            }
        }
        let fit_min = tracks.iter().map(|c| u64::from(c.min)).sum::<u64>() <= u64::from(width);
        let mut eligible = vec![false; sizes.len()];
        for (i, (c, size)) in tracks.iter().zip(&mut sizes).enumerate() {
            let auto = c.percent == 0 && c.fixed == 0;
            if auto && *size > c.max {
                *size = c.max;
            } else if fit_min && *size < c.min {
                *size = c.min;
            } else if auto && c.min > 0 {
                eligible[i] = true;
            }
        }
        if !eligible.iter().any(|e| *e) {
            for (idx, c) in tracks.iter().enumerate() {
                eligible[idx] = c.count > 0;
            }
        }
        // When preferred widths exceed the available width, take the deficit
        // only from columns with spare space above their unbreakable minimum.
        // Repeat if a narrow column hits its minimum before its share is spent.
        if fit_min {
            let mut excess = sizes.iter().sum::<u32>().saturating_sub(width);
            while excess > 0 {
                let available = tracks
                    .iter()
                    .zip(&sizes)
                    .filter(|(c, size)| **size > c.min.max(1))
                    .count() as u32;
                if available == 0 {
                    break;
                }
                let share = excess.div_ceil(available);
                for (c, size) in tracks.iter().zip(&mut sizes) {
                    let take = size.saturating_sub(c.min.max(1)).min(share).min(excess);
                    *size -= take;
                    excess -= take;
                }
            }
        }
        let mut delta = i64::from(width) - sizes.iter().map(|&n| i64::from(n)).sum::<i64>();
        let mut n = eligible.iter().filter(|&&e| e).count() as i64;
        for (size, eligible) in sizes.iter_mut().zip(eligible) {
            if eligible {
                let adjustment = (delta / n + (delta % n).signum()).max(1 - i64::from(*size));
                *size = (i64::from(*size) + adjustment) as u32;
                delta -= adjustment;
                n -= 1;
            }
        }
        self.edges.clear();
        self.edges.push(0);
        let mut x = 0;
        let columns = sizes.len();
        for (i, w) in sizes.into_iter().enumerate() {
            // Keep every column selectable even if the source gives one of them
            // an extreme width. The layout caller already verifies width >= columns.
            x = (x + w).clamp(x + 1, width - (columns - i - 1) as u32);
            self.edges.push(x);
        }
        *self.edges.last_mut().unwrap() = width;
    }
    pub(super) fn bounds(&self, cell: Cell, width: u32, gutter: u32) -> Option<(u32, u32)> {
        if self.edges.len() != usize::from(cell.columns) + 1 || self.edges.last() != Some(&width) {
            return None;
        }
        let index = usize::from(cell.column);
        let (l, r) = (
            self.edges[index],
            self.edges[index + usize::from(cell.span)],
        );
        let left = if cell.rtl { width - r } else { l };
        let inset = gutter.min((r - l - 1) / 2);
        Some((left + inset, r - l - 2 * inset))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn column(min: u32, max: u32, percent: u32, fixed: u32) -> Column {
        Column {
            min,
            max,
            sum: u64::from(max),
            count: 1,
            percent,
            fixed,
        }
    }

    #[test]
    fn automatic_columns_use_content_and_honor_an_unbreakable_neighbor() {
        let mut table = TableWidths {
            columns: vec![column(25, 50, 0, 0), column(40, 220, 0, 0)],
            edges: vec![],
            has_nowrap: false,
        };
        table.resolve(200);
        assert_eq!(&table.edges, [0, 38, 200].as_slice());
        table.columns[0].percent = 50;
        table.columns[1].percent = 50;
        table.columns[1].min = 125;
        table.resolve(200);
        assert_eq!(&table.edges, [0, 75, 200].as_slice());
    }

    #[test]
    fn constrained_fixed_and_automatic_columns_preserve_the_readable_width() {
        let mut table = TableWidths {
            columns: vec![
                column(20, 90, 0, 730),
                column(10, 80, 67, 0),
                column(25, 140, 0, 0),
            ],
            edges: vec![],
            has_nowrap: false,
        };
        for width in [81, 270, 840, 1264] {
            table.resolve(width);
            assert_eq!(table.edges[0], 0);
            assert_eq!(table.edges.last(), Some(&width));
            assert!(table.edges.windows(2).all(|pair| pair[1] > pair[0]));
        }
    }

    #[test]
    fn percentage_cell_padding_is_measured_against_its_own_outer_width() {
        let style = Style {
            padding: [
                crate::book_style::CssLength::Fraction(0.1),
                crate::book_style::CssLength::Fraction(0.1),
            ],
            ..Default::default()
        };
        assert_eq!(intrinsic_width(100, &style, 42.), 127);
        assert_eq!(intrinsic_width(200, &style, 42.), 252);
    }
}
