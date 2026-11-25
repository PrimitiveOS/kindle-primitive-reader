//! Publisher table strokes belong to cells. Their rectangles share the same
//! column tracks and row starts as text, links and selections.
use crate::{
    book_style::{
        CssLength, Style,
        borders::{Border, Pattern},
    },
    publisher_padding::BoxArea,
    source::Source,
    table_layout::{Cell, TableKey},
    vertical_padding::Fill,
};
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
    pub(crate) fn read(border: Border, font: f32) -> Self {
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
    backgrounds: bool,
    rows: usize,
    collapsed: bool,
    border: [Stroke; 4],
    spacing: [u32; 2],
    padding: [u32; 2],
    painted: bool,
    caption_bottom: Option<bool>,
}

impl Table {
    fn grid_stroke(&self, bottom: bool) -> u32 {
        if self.caption_bottom == Some(bottom) {
            0
        } else {
            self.border[if bottom { 2 } else { 0 }].width
        }
    }
}
pub(crate) struct CellBorder {
    pub element: usize,
    pub edges: [Stroke; 4],
    pub offset: u32,
    pub spacing: [u32; 2],
}
pub(crate) struct Borders {
    pub cells: HashMap<Cell, CellBorder>,
    table: HashMap<TableKey, Table>,
    outer: HashMap<usize, ([Stroke; 4], u32, u32)>,
    shaded_cells: Vec<(usize, Cell, u8)>,
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
        let separate_geometry = |index: usize| {
            !styles[index].border_collapse
                && source.elements[index].tag == "table"
                && (has_padding(index)
                    || (styles[index].has_table_spacing
                        || styles[index].border_spacing != [CssLength::Pixels(1.); 2])
                        && styles[index]
                            .border_spacing
                            .iter()
                            .any(|side| side.pixels(100, font) > 0.))
        };
        let has_background = |index: usize| {
            !styles[index].flow.suppressed
                && styles[index].background.is_some()
                && matches!(
                    source.elements[index].tag.as_str(),
                    "table" | "thead" | "tbody" | "tfoot" | "tr" | "th" | "td"
                )
        };
        if flow.cells().is_empty()
            || !styles.iter().enumerate().any(|(index, s)| {
                s.borders.is_some() || separate_geometry(index) || has_background(index)
            })
        {
            return None;
        }
        let mut table = HashMap::new();
        let mut hierarchy = HashMap::new();
        let mut rows = HashMap::new();
        for &(element, cell) in flow.cells() {
            if matches!(cell.table, TableKey::Css(_)) {
                continue;
            }
            let mut current = source.elements[element].parent;
            let (mut row, mut group) = (None, None);
            while let Some(i) = current {
                match source.elements[i].tag.as_str() {
                    "tr" => row = Some(i),
                    "tbody" | "thead" | "tfoot" => group = Some(i),
                    "table" => {
                        let t = table.entry(cell.table).or_insert_with(|| Table {
                            element: i,
                            active: separate_geometry(i),
                            backgrounds: false,
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
                            caption_bottom: None,
                        });
                        t.rows = t.rows.max(cell.after_row);
                        t.active |= shaded_cell(element);
                        let owners = [Some(i), Some(element), row, group];
                        t.active |= owners
                            .into_iter()
                            .flatten()
                            .any(|elem| styles[elem].borders.is_some());
                        t.backgrounds |= owners.into_iter().flatten().any(has_background);
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
        table.retain(|_, t| t.active || t.backgrounds);
        if table.is_empty() {
            return None;
        }
        // A retained empty row can still be covered by an earlier rowspan.
        // Preserve its own border/group metadata even when no cell starts there.
        let native_tables: HashMap<_, _> = table.iter().map(|(id, t)| (t.element, *id)).collect();
        let mut table_owners = Vec::with_capacity(source.elements.len());
        let mut group_owners = Vec::with_capacity(source.elements.len());
        let mut original_rows = HashMap::<TableKey, usize>::new();
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
        let mut shaded_cells = Vec::new();
        for &(element, cell) in flow.cells() {
            let Some(t) = table.get(&cell.table) else {
                continue;
            };
            let (row, group) = hierarchy[&cell];
            if styles[element].flow.painted
                && let Some(gray) = [Some(element), row, group]
                    .into_iter()
                    .flatten()
                    .find_map(|i| has_background(i).then_some(styles[i].background).flatten())
            {
                shaded_cells.push((element, cell, gray));
            }
            if !t.active {
                continue;
            }
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
            shaded_cells,
        })
    }
    pub(crate) fn has_geometry(&self) -> bool {
        !self.cells.is_empty()
    }
    pub(crate) fn tracks(&self, cell: Cell) -> bool {
        self.table.contains_key(&cell.table)
    }
    pub(crate) fn place_caption(&mut self, table: TableKey, bottom: bool) {
        if let Some(table) = self.table.get_mut(&table) {
            table.caption_bottom = Some(bottom);
        }
    }
    pub(crate) fn caption_stroke(&self, table: TableKey, bottom: bool) -> u32 {
        self.table
            .get(&table)
            .filter(|t| t.active && !t.collapsed)
            .map_or(0, |t| t.border[if bottom { 2 } else { 0 }].width)
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
                    t.grid_stroke(false) + t.padding[0]
                } else {
                    0
                },
            c.edges[2].width
                + if !t.collapsed && cell.after_row == t.rows {
                    c.spacing[1] + t.grid_stroke(true) + t.padding[1]
                } else {
                    0
                },
        )
    }

    pub(crate) fn vertical_track_insets(
        &self,
        table: TableKey,
    ) -> crate::table_layout::vertical::Insets {
        let Some(t) = self.table.get(&table).filter(|t| t.active && !t.collapsed) else {
            return crate::table_layout::vertical::Insets::default();
        };
        crate::table_layout::vertical::Insets {
            head: t.spacing[1] + t.grid_stroke(false) + t.padding[0],
            spacing: t.spacing[1],
            tail: t.spacing[1] + t.grid_stroke(true) + t.padding[1],
        }
    }
}

#[derive(Default)]
pub(crate) struct Positions {
    rows: HashMap<TableKey, BTreeMap<usize, f32>>,
    bottom: HashMap<TableKey, f32>,
    captions: HashMap<TableKey, Range<f32>>,
}
impl Positions {
    pub(crate) fn record_caption(&mut self, table: TableKey, span: Range<f32>) {
        self.captions.insert(table, span);
    }

    pub(crate) fn outer_extent(&self, table: TableKey) -> Option<Range<f32>> {
        let top = *self.rows.get(&table)?.first_key_value()?.1;
        let bottom = self.bottom[&table];
        Some(self.captions.get(&table).map_or(top..bottom, |caption| {
            top.min(caption.start)..bottom.max(caption.end)
        }))
    }

    pub(crate) fn record_tracks(
        &mut self,
        table: TableKey,
        start: f32,
        rows: &[Range<f32>],
        bottom: f32,
    ) {
        self.rows.insert(
            table,
            rows.iter()
                .enumerate()
                .map(|(index, row)| (index, start + row.start))
                .collect(),
        );
        self.bottom.insert(table, bottom);
    }

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
    fn extent(
        &self,
        table: TableKey,
        first: usize,
        after: usize,
        spec: &Table,
    ) -> Option<Range<f32>> {
        let rows = self.rows.get(&table)?;
        let top = *rows.get(&first)?;
        let bottom = rows
            .range(after..)
            .next()
            .map_or(self.bottom[&table], |(_, v)| *v);
        if spec.active && !spec.collapsed {
            let before = spec.spacing[1]
                + if first == 0 {
                    spec.grid_stroke(false) + spec.padding[0]
                } else {
                    0
                };
            let after = if after == spec.rows {
                spec.spacing[1] + spec.grid_stroke(true) + spec.padding[1]
            } else {
                0
            };
            Some(top + before as f32..bottom - after as f32)
        } else {
            Some(top..bottom)
        }
    }

    pub(crate) fn backgrounds(
        &self,
        borders: &Borders,
        boxes: Option<&[BoxArea]>,
        styles: &[Style],
        width: u32,
        font: f32,
        out: &mut Vec<Fill>,
    ) {
        for (&id, spec) in &borders.table {
            let style = &styles[spec.element];
            let Some(color) = style.background.filter(|_| style.flow.painted) else {
                continue;
            };
            let Some(extent) = self.outer_extent(id) else {
                continue;
            };
            let outer = boxes.map_or(0..width, |b| {
                let root = b[spec.element];
                root.outer_x..root.outer_x + root.outer_width
            });
            out.push(Fill::rectangle(
                spec.element,
                outer,
                extent.start,
                extent.end,
                color,
            ));
        }
        // Rows and groups paint through their cells, leaving separate-border
        // spacing clear. A row-spanning cell keeps its originating fill.
        for &(element, cell, color) in &borders.shaded_cells {
            let Some(y) = self.extent(
                cell.table,
                cell.row,
                cell.after_row,
                &borders.table[&cell.table],
            ) else {
                continue;
            };
            match boxes {
                Some(b) => out.push(Fill::cell(element, b[element], y.start, y.end, color)),
                None => {
                    let (x, w) = cell.bounds(width, (font * 0.22).round() as u32);
                    out.push(Fill::rectangle(element, x..x + w, y.start, y.end, color));
                }
            }
        }
    }

    pub fn finish(self, borders: &Borders, boxes: &[BoxArea]) -> Vec<Frame> {
        let mut frames = Vec::new();
        for (cell, spec) in &borders.cells {
            let Some(y) = self.extent(
                cell.table,
                cell.row,
                cell.after_row,
                &borders.table[&cell.table],
            ) else {
                continue;
            };
            let bounds = boxes[spec.element];
            Frame::push(
                &mut frames,
                bounds.outer_x,
                bounds.outer_width,
                y.start,
                y.end,
                spec.edges,
            );
        }
        for (id, t) in &borders.table {
            if t.active
                && !t.collapsed
                && let Some(extent) = self.outer_extent(*id)
            {
                let b = boxes[t.element];
                Frame::push(
                    &mut frames,
                    b.outer_x,
                    b.outer_width,
                    extent.start,
                    extent.end,
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
    pub(crate) fn push(
        out: &mut Vec<Self>,
        x: u32,
        width: u32,
        top: f32,
        bottom: f32,
        edges: [Stroke; 4],
    ) {
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
            crate::Settings::default(),
            None,
            |_| (40, 80),
        )
        .unwrap();
        let mut positions = Positions::default();
        for &(_, cell) in flow.cells() {
            positions.record(cell, 0., 100.);
        }
        let mut backgrounds = Vec::new();
        positions.backgrounds(&borders, Some(&boxes), &styles, 600, 40., &mut backgrounds);
        let frames = positions.finish(&borders, &boxes);
        assert!(backgrounds.is_empty());
        assert_eq!(frames.len(), 1);
        let mut pixels = vec![255; 600 * 100];
        frames[0].paint(&mut pixels, 600, &(0..600), &(0.0..100.0));
        let edge = boxes[visible].outer_x as usize;
        assert!(pixels[..edge].iter().all(|&value| value == 255));
        assert_eq!(pixels[edge], 0);
    }

    #[test]
    fn reading_style_table_stripes_and_borders_preserve_anchors_and_cascade() {
        use crate::{Anchor, Book, Engine, Probe, Settings, user_styles};

        const ROWS: [(&str, u8); 13] = [
            ("Head row", 221),
            ("Body first", 238),
            ("Own background", 170),
            ("Body third", 238),
            ("Body fourth", 204),
            ("New group", 238),
            ("Foot row", 238),
            ("Direct first", 238),
            ("Direct second", 204),
            ("Publisher left", 238),
            ("Publisher right", 238),
            ("Single column", 238),
            ("Second solo", 204),
        ];
        let settings = Settings {
            width: 640,
            height: 880,
            margin: 40,
            font_size: 22.,
            ..Settings::default()
        };
        let open = |data: &user_styles::Data| {
            Probe::from_book_with_rules(
                Book::open(&crate::test_fixture("table-style-presets.xhtml")).unwrap(),
                Engine::Parley,
                &[
                    crate::test_fixture("fonts/NotoSans-Regular.ttf"),
                    crate::test_fixture("fonts/NotoSans-Bold.ttf"),
                ],
                settings,
                0,
                None,
                &crate::user_hyphenation::Rules::default(),
                &data.snapshot(1).unwrap(),
            )
            .unwrap()
        };
        let area = |probe: &Probe, word: &str| {
            let mut matches = probe.matches(word);
            assert_eq!(matches.len(), 1, "{word}");
            let start = matches.pop().unwrap();
            let end = Anchor {
                raw_caret: None,
                offset: start.offset + word.len(),
                ..start.clone()
            };
            let text = probe.select_range(&start, &end).unwrap();
            assert_eq!(text.text, word);
            assert_eq!(probe.page_for(&start), Some(0));
            let rectangles = probe.selection_rects(0, &text);
            assert_eq!(rectangles.len(), 1, "{word}");
            rectangles[0]
        };
        let sample =
            |frame: &[u8], x: i32, y: i32| frame[y as usize * settings.width as usize + x as usize];
        let mut data = user_styles::Data::default();
        let original = open(&data);
        let mut frames = Vec::new();
        for (name, stripe, force) in [
            ("original", false, false),
            ("striped", true, false),
            ("combined", true, true),
            ("borders", false, true),
        ] {
            data.choose(1, "table_row_odd_even", Some(stripe)).unwrap();
            data.choose(1, "table_force_border", Some(force)).unwrap();
            let mut probe = open(&data);
            let frame = probe.capture(0).unwrap();
            assert!(probe.matches("Hidden row").is_empty());
            assert_eq!(probe.coverage(), original.coverage());
            for (word, expected) in ROWS {
                assert_eq!(probe.matches(word), original.matches(word));
                let rect = area(&probe, word);
                let y = rect.y + rect.height / 2;
                let x = rect.x + rect.width + 8;
                if stripe {
                    assert_eq!(sample(&frame, x, y), expected, "{name} {word}");
                }
                if !force {
                    let original = area(&original, word);
                    assert_eq!((rect.x, rect.y), (original.x, original.y), "{name} {word}");
                }
                if matches!(word, "Publisher left" | "Single column") {
                    let edge = if word == "Publisher left" {
                        102
                    } else if force {
                        0
                    } else {
                        255
                    };
                    assert_eq!(sample(&frame, 40, y), edge, "{name} {word}");
                }
            }
            let right = area(&probe, "Publisher right");
            assert_eq!(
                sample(&frame, right.x - 11, right.y + right.height / 2),
                255
            );
            #[cfg(feature = "kindle")]
            if let Some(dir) = std::env::var_os("READER_TABLE_STYLE_CAPTURES") {
                let dir = std::path::PathBuf::from(dir);
                std::fs::create_dir_all(&dir).unwrap();
                primitive_app::write_png(
                    &dir.join(format!("reader-table-style-{name}.png")),
                    settings.width,
                    settings.height,
                    &frame,
                )
                .unwrap();
            }
            frames.push(frame);
        }
        assert!(frames.windows(2).all(|pair| pair[0] != pair[1]));

        let legacy: user_styles::Data =
            serde_json::from_str(r#"{"version":1,"global":["table-visible-borders"],"books":{}}"#)
                .unwrap();
        let mut probe = open(&legacy);
        let row = area(&probe, "Publisher left");
        assert_eq!(
            sample(&probe.capture(0).unwrap(), 40, row.y + row.height / 2),
            0
        );
    }
}
