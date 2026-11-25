//! Collect a bounded table's natural geometry and place its finished tracks.

use std::collections::{HashMap, HashSet};

use crate::{
    Line,
    book_style::Style,
    table_borders::{Borders, Positions},
    table_layout::{
        Cell, TableFlow, TableKey,
        vertical::{self, CellAlignment, Measure},
    },
};

use super::{MarkerPaint, ParleyChapter};

#[cfg(test)]
mod tests;

#[derive(Default)]
struct Grid {
    rows: usize,
    alignments: HashMap<Cell, CellAlignment>,
}

#[derive(Clone, Copy)]
struct Cursor {
    lines: usize,
    paragraphs: usize,
    pictures: usize,
    markers: usize,
    fills: usize,
    breaks: usize,
    height_spaces: usize,
    tails: usize,
}

impl Cursor {
    fn at(chapter: &ParleyChapter, lines: usize, fills: usize) -> Self {
        Self {
            lines,
            paragraphs: chapter.paragraphs.len(),
            pictures: chapter.illustrations.len(),
            markers: chapter.markers.len(),
            fills,
            breaks: chapter.physical_break_runs.len(),
            height_spaces: chapter.publisher_height_spaces.len(),
            tails: chapter.fitted_height_tails.len(),
        }
    }
}

struct Run {
    cell: Cell,
    top: f32,
    content_top: f32,
    end: f32,
    gap: f32,
    before: Cursor,
    after: Cursor,
}

struct Pending {
    key: TableKey,
    runs: Vec<Run>,
    unsupported: bool,
    trailing_wrapper: bool,
}

pub(super) struct Pass {
    grids: HashMap<TableKey, Grid>,
    pending: Option<Pending>,
}

impl Pass {
    pub(super) fn new(flow: &TableFlow<'_>, styles: &[Style], enabled: bool) -> Self {
        let mut grids = HashMap::<_, Grid>::new();
        if enabled {
            for &(element, cell) in flow.cells() {
                if matches!(cell.table, TableKey::Native(_))
                    && styles[element].table_valign_explicit
                {
                    grids.entry(cell.table).or_default();
                }
            }
            for &(element, cell) in flow.cells() {
                if let Some(grid) = grids.get_mut(&cell.table) {
                    let style = &styles[element];
                    let alignment = if style.table_valign_explicit {
                        CellAlignment::from_css(style.image_align)
                    } else {
                        CellAlignment::Top
                    };
                    grid.alignments.insert(cell, alignment);
                    grid.rows = grid.rows.max(cell.after_row);
                }
            }
            grids.retain(|_, grid| grid.alignments.len() <= 4096 && grid.rows <= 8192);
        }
        Self {
            grids,
            pending: None,
        }
    }

    pub(super) fn ends_before(&self, next: Option<Cell>) -> bool {
        self.pending
            .as_ref()
            .is_some_and(|table| Some(table.key) != next.map(|cell| cell.table))
    }

    pub(super) fn begin(
        &mut self,
        cell: Cell,
        chapter: &ParleyChapter,
        lines: usize,
        fills: usize,
        top: f32,
        content_top: f32,
        wrapper_before: bool,
    ) {
        if !self.grids.contains_key(&cell.table) {
            return;
        }
        let pending = self.pending.get_or_insert_with(|| Pending {
            key: cell.table,
            runs: Vec::new(),
            unsupported: cell.row != 0,
            trailing_wrapper: false,
        });
        if pending.trailing_wrapper || wrapper_before && !pending.runs.is_empty() {
            pending.unsupported = true;
        }
        if pending.runs.last().is_some_and(|r| r.cell == cell) {
            return;
        }
        let indices = Cursor::at(chapter, lines, fills);
        pending.runs.push(Run {
            cell,
            top,
            content_top,
            end: top,
            gap: 0.0,
            before: indices,
            after: indices,
        });
    }

    pub(super) fn end_block(
        &mut self,
        chapter: &ParleyChapter,
        lines: usize,
        fills: usize,
        end: f32,
        gap: f32,
        wrapper_after: bool,
    ) {
        if let Some(pending) = &mut self.pending
            && let Some(run) = pending.runs.last_mut()
        {
            run.after = Cursor::at(chapter, lines, fills);
            run.end = end;
            run.gap = gap;
            pending.trailing_wrapper = wrapper_after;
        }
    }

    pub(super) fn finish(
        &mut self,
        provisional_bottom: f32,
        chapter: &mut ParleyChapter,
        lines: &mut [Line],
        fills: &mut [(f32, f32)],
        borders: Option<&Borders>,
        positions: &mut Positions,
    ) -> f32 {
        let Some(table) = self.pending.take().filter(|table| !table.unsupported) else {
            return provisional_bottom;
        };
        let grid = &self.grids[&table.key];
        let insets = borders.map_or_else(Default::default, |b| b.vertical_track_insets(table.key));
        let mut raw_bottom = 0.0_f32;
        let mut raw_paint_bottom = 0.0_f32;
        let mut measures = Vec::with_capacity(table.runs.len());
        let mut seen = HashSet::new();
        for run in &table.runs {
            let Some(&alignment) = grid.alignments.get(&run.cell) else {
                return provisional_bottom;
            };
            if !seen.insert(run.cell) {
                return provisional_bottom;
            }
            let origin = run.top + insets.before(run.cell.row);
            let tail = if run.cell.after_row == grid.rows {
                insets.tail as f32
            } else {
                0.0
            };
            let height = (run.end - origin - tail).max(0.0);
            let baseline = chapter.paragraphs[run.before.paragraphs..run.after.paragraphs]
                .iter()
                .find_map(|p| p.layout.first_baseline().map(|baseline| p.y + baseline))
                .unwrap_or_else(|| {
                    chapter.illustrations[run.before.pictures..run.after.pictures]
                        .iter()
                        .map(|image| image.y + image.display.1 as f32)
                        .fold(run.content_top, f32::max)
                });
            measures.push(Measure {
                cell: run.cell,
                alignment,
                height,
                baseline: (baseline - origin).clamp(0.0, height),
            });
            raw_bottom = raw_bottom.max(run.end);
            raw_paint_bottom = raw_paint_bottom.max(run.end - run.gap);
        }
        let Some(tracks) = vertical::solve(&measures, grid.rows, insets) else {
            return provisional_bottom;
        };
        let start = table.runs[0].top;
        let bottom = start + tracks.height;
        let paint_bottom = bottom - (raw_bottom - raw_paint_bottom);
        if borders.is_some_and(|b| b.tracks(table.runs[0].cell)) {
            positions.record_tracks(table.key, start, &tracks.rows, paint_bottom);
        }
        for (run, pad) in table.runs.into_iter().zip(tracks.padding) {
            let delta = start + tracks.rows[run.cell.row].start + pad - run.top;
            if delta != 0.0 {
                run.shift(chapter, lines, fills, delta);
            }
        }
        bottom + (provisional_bottom - raw_bottom).max(0.0)
    }
}

impl Run {
    fn shift(
        &self,
        chapter: &mut ParleyChapter,
        lines: &mut [Line],
        fills: &mut [(f32, f32)],
        delta: f32,
    ) {
        let start = self.before;
        let end = self.after;
        for line in &mut lines[start.lines..end.lines] {
            line.top += delta;
            line.bottom += delta;
        }
        for p in &mut chapter.paragraphs[start.paragraphs..end.paragraphs] {
            p.y += delta;
            p.text_bottom += delta;
            p.bottom += delta;
        }
        for image in &mut chapter.illustrations[start.pictures..end.pictures] {
            image.y += delta;
            if let Some((_, top)) = &mut image.render_line {
                *top += delta;
            }
        }
        for marker in &mut chapter.markers[start.markers..end.markers] {
            marker.top += delta;
            marker.bottom += delta;
            if let MarkerPaint::Text { y, .. } = &mut marker.paint {
                *y += delta;
            }
        }
        for (top, bottom) in &mut fills[start.fills..end.fills] {
            *top += delta;
            *bottom += delta;
        }
        for positioned in &mut chapter.physical_break_runs[start.breaks..end.breaks] {
            for cut in &mut positioned.cuts {
                *cut += delta;
            }
        }
        for space in &mut chapter.publisher_height_spaces[start.height_spaces..end.height_spaces] {
            space.start += delta;
            space.end += delta;
        }
        for tail in &mut chapter.fitted_height_tails[start.tails..end.tails] {
            tail.closing.start += delta;
            tail.closing.end += delta;
        }
    }
}
