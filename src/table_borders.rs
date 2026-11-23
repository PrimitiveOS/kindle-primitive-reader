//! Publisher table strokes belong to cells. Their rectangles share the same
//! column tracks and row starts as text, links and selections.
use crate::{
    book_style::{
        Style,
        borders::{Border, Pattern},
    },
    source::Source,
    table_layout::Cell,
};
use roxmltree::NodeId;
use std::{
    collections::{BTreeMap, HashMap},
    ops::Range,
};

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub(crate) struct Stroke {
    pub width: u32,
    pub pattern: Pattern,
    pub gray: Option<(u8, u8)>,
}
impl Stroke {
    fn read(border: Border, font: f32) -> Self {
        if matches!(border.pattern, Pattern::None | Pattern::Hidden) {
            return Self::default();
        }
        Self {
            width: (border.width.box_pixels(0, font) as u32).min(64),
            pattern: border.pattern,
            gray: border.gray,
        }
    }
    fn prefer(&mut self, other: Self) {
        if other.width > self.width {
            *self = other;
        }
    }
}
fn edges(style: &Style, font: f32) -> [Stroke; 4] {
    style
        .borders
        .as_ref()
        .map(|s| s.map(|edge| Stroke::read(edge, font)))
        .unwrap_or_default()
}

struct Table {
    element: usize,
    active: bool,
    rows: usize,
    collapsed: bool,
    border: [Stroke; 4],
    spacing: [u32; 2],
    padding: [u32; 2],
    painted: bool,
}
pub(crate) struct CellBorder {
    pub element: usize,
    pub edges: [Stroke; 4],
    pub offset: u32,
    pub spacing: [u32; 2],
}
pub(crate) struct Borders {
    pub cells: HashMap<Cell, CellBorder>,
    table: HashMap<NodeId, Table>,
    outer: HashMap<usize, ([Stroke; 4], u32, u32)>,
}
impl Borders {
    pub(crate) fn read(source: &Source, styles: &[Style], font: f32) -> Option<Self> {
        Self::read_with_flow(
            source,
            styles,
            font,
            &crate::table_layout::TableFlow::identity(source),
        )
    }
    pub(crate) fn read_with_flow(
        source: &Source,
        styles: &[Style],
        font: f32,
        flow: &crate::table_layout::TableFlow<'_>,
    ) -> Option<Self> {
        Self::read_impl(source, styles, font, flow, None)
    }
    pub(crate) fn read_with_settings(
        source: &Source,
        styles: &[Style],
        settings: crate::Settings,
        flow: &crate::table_layout::TableFlow<'_>,
    ) -> Option<Self> {
        Self::read_impl(source, styles, settings.font_size, flow, Some(settings))
    }
    fn read_impl(
        source: &Source,
        styles: &[Style],
        font: f32,
        flow: &crate::table_layout::TableFlow<'_>,
        settings: Option<crate::Settings>,
    ) -> Option<Self> {
        let has_padding = |index: usize| {
            styles[index]
                .vertical_padding
                .iter()
                .any(|side| side.pixels(100, font) > 0.)
        };
        let shaded_cell = |idx: usize| {
            matches!(source.elements[idx].tag.as_str(), "td" | "th")
                && styles[idx].background.is_some()
                && has_padding(idx)
        };
        let separate_padding = |index: usize| {
            !styles[index].border_collapse
                && source.elements[index].tag == "table"
                && has_padding(index)
        };
        if flow.cells().is_empty()
            || !styles.iter().enumerate().any(|(index, s)| {
                s.borders.is_some() || separate_padding(index) || shaded_cell(index)
            })
        {
            return None;
        }
        let mut table = HashMap::new();
        let mut hierarchy = HashMap::new();
        let mut rows = HashMap::new();
        for &(element, cell) in flow.cells() {
            let mut current = source.elements[element].parent;
            let (mut row, mut group) = (None, None);
            while let Some(i) = current {
                match source.elements[i].tag.as_str() {
                    "tr" => row = Some(i),
                    "tbody" | "thead" | "tfoot" => group = Some(i),
                    "table" => {
                        let t = table.entry(cell.table).or_insert_with(|| Table {
                            element: i,
                            active: separate_padding(i),
                            padding: [0, 0],
                            rows: 0,
                            collapsed: styles[i].border_collapse,
                            border: edges(&styles[i], font),
                            spacing: styles[i].border_spacing.map(|s| {
                                (s.box_pixels(
                                    0,
                                    settings.filter(|s| s.logical_font_size.is_some()).map_or(
                                        font * styles[i].size_scale,
                                        |cfg| {
                                            if matches!(
                                                s,
                                                crate::book_style::CssLength::ComputedEm { .. }
                                            ) {
                                                font
                                            } else {
                                                cfg.styled_font(styles[i].size_scale)
                                            }
                                        },
                                    ),
                                ) as u32)
                                    .min(64)
                            }),
                            painted: styles[i].flow.painted,
                        });
                        t.rows = t.rows.max(cell.after_row);
                        t.active |= shaded_cell(element);
                        t.active |= [Some(i), Some(element), row, group]
                            .into_iter()
                            .flatten()
                            .any(|elem| styles[elem].borders.is_some());
                        if let Some(r) = row {
                            rows.insert((cell.table, cell.row), (r, group));
                        }
                        hierarchy.insert(cell, (row, group));
                        break;
                    }
                    _ => (),
                }
                current = source.elements[i].parent;
            }
        }
        table.retain(|_, t| t.active);
        if table.is_empty() {
            return None;
        }
        // A retained empty row can still be covered by an earlier rowspan.
        // Preserve its own border/group metadata even when no cell starts there.
        let native_tables: HashMap<_, _> = table.iter().map(|(id, t)| (t.element, *id)).collect();
        let mut table_owners = Vec::with_capacity(source.elements.len());
        let mut group_owners = Vec::with_capacity(source.elements.len());
        let mut original_rows = HashMap::<NodeId, usize>::new();
        for (index, element) in source.elements.iter().enumerate() {
            let owner = if element.tag == "table" {
                native_tables.get(&index).copied()
            } else {
                element.parent.and_then(|parent| table_owners[parent])
            };
            let group = if element.tag == "table" {
                None
            } else if matches!(element.tag.as_str(), "thead" | "tbody" | "tfoot") {
                Some(index)
            } else {
                element.parent.and_then(|parent| group_owners[parent])
            };
            table_owners.push(owner);
            group_owners.push(group);
            if element.tag == "tr"
                && !element.selector_only
                && let Some(owner) = owner
            {
                let original = original_rows.entry(owner).or_default();
                if let Some(row) = flow.row_index(index, *original) {
                    rows.insert((owner, row), (index, group));
                }
                *original += 1;
            }
        }
        let mut cells = HashMap::new();
        let mut outer = HashMap::new();
        for &(element, cell) in flow.cells() {
            let Some(t) = table.get(&cell.table) else {
                continue;
            };
            let mut border = edges(&styles[element], font);
            let leading = if cell.rtl {
                cell.columns - cell.column - cell.span
            } else {
                cell.column
            };
            let first = leading == 0;
            let last = leading + cell.span == cell.columns;
            if t.collapsed {
                let (r, g) = hierarchy[&cell];
                let end = rows
                    .get(&(cell.table, cell.after_row - 1))
                    .copied()
                    .unwrap_or((r?, g));
                let start_group = cell.row == 0
                    || rows.get(&(cell.table, cell.row - 1)).and_then(|(_, g)| *g) != g;
                let end_group = cell.after_row == t.rows
                    || rows
                        .get(&(cell.table, cell.after_row))
                        .and_then(|(_, g)| *g)
                        != end.1;
                let row_edges = [r, r, Some(end.0), r];
                let group_edges = [g, g, end.1, g];
                for side in 0..4 {
                    if [true, last, true, first][side]
                        && let Some(e) = row_edges[side]
                    {
                        border[side].prefer(edges(&styles[e], font)[side]);
                    }
                    if [start_group, last, end_group, first][side]
                        && let Some(e) = group_edges[side]
                    {
                        border[side].prefer(edges(&styles[e], font)[side]);
                    }
                    if [cell.row == 0, last, cell.after_row == t.rows, first][side] {
                        border[side].prefer(t.border[side]);
                    }
                }
                // CREngine owns an interior boundary on the cell below/right.
                // It drops an interior right/bottom declaration on the other cell.
                if !last {
                    border[1] = Stroke::default();
                }
                if cell.after_row < t.rows {
                    border[2] = Stroke::default();
                }
            } else {
                outer.insert(
                    t.element,
                    (t.border, t.spacing[0], u32::from(cell.columns) + 1),
                );
            }
            // Collapsed donors contribute geometry first. The final cell owns
            // painting, matching CRE's post-conflict cell-style draw gate.
            if !styles[element].flow.painted {
                for edge in &mut border {
                    edge.gray = None;
                }
            }
            cells.insert(
                cell,
                CellBorder {
                    element,
                    edges: border,
                    offset: u32::from(leading) + 1,
                    spacing: if t.collapsed { [0, 0] } else { t.spacing },
                },
            );
        }
        Some(Self {
            cells,
            table,
            outer,
        })
    }
    pub(crate) fn resolve_padding(
        &mut self,
        boxes: &[crate::publisher_padding::BoxArea],
        styles: &[Style],
        cfg: crate::Settings,
    ) {
        for table in self.table.values_mut().filter(|t| !t.collapsed) {
            let width = boxes[table.element].outer_width;
            table.padding = styles[table.element].vertical_padding.map(|edge| {
                (edge.box_pixels(width, cfg.font_size) as u32).min(cfg.text_height() / 2)
            });
        }
    }
    pub(crate) fn outer_edges(&self, elem: usize) -> Option<[Stroke; 4]> {
        self.outer.get(&elem).map(|s| s.0)
    }
    pub(crate) fn spacing_for_table(&self, element: usize) -> (u32, u32) {
        self.outer
            .get(&element)
            .map(|a| (a.1, a.2))
            .unwrap_or_default()
    }
    pub(crate) fn horizontal(&self, cell: Cell, width: u32) -> (u32, u32) {
        let c = &self.cells[&cell];
        let spaces = u32::from(cell.columns) + 1;
        let spacing = c.spacing[0].min(width.saturating_sub(u32::from(cell.columns)) / spaces);
        (spacing * c.offset, width - spacing * spaces)
    }
    pub(crate) fn vertical(&self, cell: Cell) -> (u32, u32) {
        let c = &self.cells[&cell];
        let t = &self.table[&cell.table];
        (
            c.edges[0].width
                + c.spacing[1]
                + if !t.collapsed && cell.row == 0 {
                    t.border[0].width + t.padding[0]
                } else {
                    0
                },
            c.edges[2].width
                + if !t.collapsed && cell.after_row == t.rows {
                    c.spacing[1] + t.border[2].width + t.padding[1]
                } else {
                    0
                },
        )
    }
}

#[derive(Default)]
pub(crate) struct Positions {
    rows: HashMap<NodeId, BTreeMap<usize, f32>>,
    bottom: HashMap<NodeId, f32>,
}
impl Positions {
    pub fn record(&mut self, cell: Cell, start: f32, end: f32) {
        self.rows
            .entry(cell.table)
            .or_default()
            .entry(cell.row)
            .and_modify(|y| *y = y.min(start))
            .or_insert(start);
        self.bottom
            .entry(cell.table)
            .and_modify(|y| *y = y.max(end))
            .or_insert(end);
    }
    pub fn finish(
        self,
        borders: &Borders,
        boxes: &[crate::publisher_padding::BoxArea],
        styles: &[Style],
        backgrounds: &mut Vec<crate::vertical_padding::Fill>,
    ) -> Vec<Frame> {
        let mut frames = Vec::new();
        for (cell, spec) in &borders.cells {
            let Some(rows) = self.rows.get(&cell.table) else {
                continue;
            };
            let Some(&top) = rows.get(&cell.row) else {
                continue;
            };
            let end = rows
                .range(cell.after_row..)
                .next()
                .map_or(self.bottom[&cell.table], |(_, v)| *v);
            let t = &borders.table[&cell.table];
            let above = spec.spacing[1]
                + if cell.row == 0 && !t.collapsed {
                    t.border[0].width + t.padding[0]
                } else {
                    0
                };
            let below = if cell.after_row == t.rows && !t.collapsed {
                t.spacing[1] + t.border[2].width + t.padding[1]
            } else {
                0
            };
            let bounds = boxes[spec.element];
            if styles[spec.element].flow.painted
                && let Some(gray) = styles[spec.element].background
                && styles[spec.element]
                    .vertical_padding
                    .iter()
                    .any(|side| *side != crate::book_style::CssLength::default())
            {
                backgrounds.push(crate::vertical_padding::Fill::cell(
                    spec.element,
                    bounds,
                    top + above as f32,
                    end - below as f32,
                    gray,
                ));
            }
            Frame::push(
                &mut frames,
                bounds.outer_x,
                bounds.outer_width,
                top + above as f32,
                end - below as f32,
                spec.edges,
            );
        }
        for (id, t) in &borders.table {
            if !t.collapsed
                && let Some(rows) = self.rows.get(id)
                && let Some((_, &top)) = rows.first_key_value()
            {
                let b = boxes[t.element];
                Frame::push(
                    &mut frames,
                    b.outer_x,
                    b.outer_width,
                    top,
                    self.bottom[id],
                    if t.painted {
                        t.border
                    } else {
                        t.border.map(|edge| Stroke { gray: None, ..edge })
                    },
                );
            }
        }
        frames.sort_by(|a, b| a.top.total_cmp(&b.top).then(a.x.cmp(&b.x)));
        frames
    }
}
pub(crate) struct Frame {
    x: u32,
    width: u32,
    top: f32,
    bottom: f32,
    edges: [Stroke; 4],
}
impl Frame {
    fn push(out: &mut Vec<Self>, x: u32, width: u32, top: f32, bottom: f32, edges: [Stroke; 4]) {
        if bottom > top && edges.iter().any(|s| s.width > 0 && s.gray.is_some()) {
            out.push(Self {
                x,
                width,
                top,
                bottom,
                edges,
            });
        }
    }
    pub(crate) fn bottom(&self) -> f32 {
        self.bottom
    }
    pub(crate) fn paint(
        &self,
        page: &mut [u8],
        stride: usize,
        columns: &Range<usize>,
        range: &Range<f32>,
    ) {
        if self.top >= range.end || self.bottom <= range.start {
            return;
        }
        let rows = (page.len() / stride) as i64;
        let top = self.top.round() as i64;
        let bottom = self.bottom.round() as i64;
        let offset = range.start.round() as i64;
        let l = self.x as i64;
        let r = l + i64::from(self.width);
        for (side, rule) in self.edges.iter().enumerate() {
            let Some((gray, alpha)) = rule.gray else {
                continue;
            };
            let w = i64::from(rule.width);
            if w == 0 {
                continue;
            }
            let (x0, x1, y0, y1) = match side {
                0 => (l, r, top, (top + w).min(bottom)),
                1 => ((r - w).max(l), r, top, bottom),
                2 => (l, r, (bottom - w).max(top), bottom),
                _ => (l, (l + w).min(r), top, bottom),
            };
            let (x0, x1) = (
                x0.clamp(0, columns.len() as i64),
                x1.clamp(0, columns.len() as i64),
            );
            for y in y0.max(offset)..y1.min(offset + rows) {
                let row = (y - offset) as usize * stride + columns.start;
                if rule.pattern == Pattern::Solid && alpha == 255 {
                    page[row + x0 as usize..row + x1 as usize].fill(gray);
                    continue;
                }
                for x in x0..x1 {
                    let (across, along) = if side == 0 || side == 2 {
                        (y - y0, x - x0)
                    } else {
                        (x - x0, y - y0)
                    };
                    let paint = match rule.pattern {
                        Pattern::Double if w >= 3 => {
                            across < (w + 1) / 3 || across >= w - (w + 1) / 3
                        }
                        Pattern::Dotted => along % (2 * w) < w,
                        Pattern::Dashed => along % (6 * w) < 3 * w,
                        _ => true,
                    };
                    if paint {
                        let pixel = &mut page[row + x as usize];
                        *pixel = ((u32::from(gray) * u32::from(alpha)
                            + u32::from(*pixel) * u32::from(255 - alpha)
                            + 127)
                            / 255) as u8;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod flow_tests {
    use super::*;

    #[test]
    fn a_retained_empty_row_still_donates_the_ending_edge_to_a_visible_rowspan() {
        let source=Source::parse("<html><body><table style='border-collapse:collapse;border:1px solid #000'><tr><td id='span' rowspan='2' style='border-bottom:2px solid #000'>Spanning</td><td>B</td></tr><tr style='visibility:hidden;border-bottom:9px solid #000'><td style='display:none'>Removed</td></tr></table></body></html>").unwrap();
        let mut book = crate::Book::open(&crate::test_fixture("field-notes-epub3.epub")).unwrap();
        let href = book.chapters[0].href.clone();
        let styles = crate::book_style::BookStyle::load_for_flow(
            &mut book,
            &href,
            &source,
            true,
            &Default::default(),
        )
        .unwrap()
        .compute(&source.elements);
        let flow = crate::table_layout::TableFlow::new(&source, &styles).unwrap();
        let owner = source
            .elements
            .iter()
            .position(|e| e.id.as_deref() == Some("span"))
            .unwrap();
        let cell = flow.cell(owner).unwrap();
        assert_eq!((cell.row, cell.after_row), (0, 2));
        let borders = Borders::read_with_flow(&source, &styles, 40., &flow).unwrap();
        assert_eq!(borders.cells[&cell].edges[2].width, 9);
        assert!(borders.cells[&cell].edges[2].gray.is_some());
    }

    #[test]
    fn hidden_cells_keep_collapsed_geometry_but_visible_cells_paint_donated_edges() {
        let source=Source::parse("<html><body><table style='visibility:hidden;border-collapse:collapse;border:12px solid #000'><tr><td id='hidden' style='padding-top:5px;background:#888'>Hidden</td><td id='visible' style='visibility:visible'>Visible</td></tr></table></body></html>").unwrap();
        let mut book = crate::Book::open(&crate::test_fixture("field-notes-epub3.epub")).unwrap();
        let href = book.chapters[0].href.clone();
        let styles = crate::book_style::BookStyle::load_for_flow(
            &mut book,
            &href,
            &source,
            true,
            &Default::default(),
        )
        .unwrap()
        .compute(&source.elements);
        let flow = crate::table_layout::TableFlow::new(&source, &styles).unwrap();
        let hidden = source
            .elements
            .iter()
            .position(|e| e.id.as_deref() == Some("hidden"))
            .unwrap();
        let visible = source
            .elements
            .iter()
            .position(|e| e.id.as_deref() == Some("visible"))
            .unwrap();
        let hidden_cell = flow.cell(hidden).unwrap();
        let visible_cell = flow.cell(visible).unwrap();
        let borders = Borders::read_with_flow(&source, &styles, 40., &flow).unwrap();
        assert!(
            borders.cells[&hidden_cell]
                .edges
                .iter()
                .all(|edge| edge.gray.is_none())
        );
        assert_eq!(borders.cells[&hidden_cell].edges[0].width, 12);
        assert_eq!(borders.cells[&visible_cell].edges[0].width, 12);
        assert!(borders.cells[&visible_cell].edges[0].gray.is_some());
        let mut shown = styles.clone();
        for style in &mut shown {
            style.flow.painted = true;
        }
        let control = Borders::read_with_flow(&source, &shown, 40., &flow).unwrap();
        for &(_, cell) in flow.cells() {
            assert_eq!(
                borders.cells[&cell].edges.map(|e| e.width),
                control.cells[&cell].edges.map(|e| e.width)
            );
            assert_eq!(borders.vertical(cell), control.vertical(cell));
            assert_eq!(borders.horizontal(cell, 600), control.horizontal(cell, 600));
        }
        let boxes = crate::publisher_padding::layout_with_flow(
            &source,
            &styles,
            600,
            40.,
            Some(&borders),
            &flow,
            |_| (40, 80),
        )
        .unwrap();
        let mut positions = Positions::default();
        for &(_, cell) in flow.cells() {
            positions.record(cell, 0., 100.);
        }
        let mut backgrounds = Vec::new();
        let frames = positions.finish(&borders, &boxes, &styles, &mut backgrounds);
        assert!(backgrounds.is_empty());
        assert_eq!(frames.len(), 1);
        let mut pixels = vec![255; 600 * 100];
        frames[0].paint(&mut pixels, 600, &(0..600), &(0.0..100.0));
        assert!(pixels[..300].iter().all(|&value| value == 255));
        assert!(pixels[300..600].contains(&0));
    }
}
