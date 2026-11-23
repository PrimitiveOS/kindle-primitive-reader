//! Column positions for simple reflowable XHTML tables. Text stays in its original
//! source block, so a cell's links, selection and saved locations still use book offsets.
use roxmltree::{Node, NodeId};
use std::collections::{BTreeMap, HashMap};

const MAX_COLUMNS: usize = 32;

mod flow;
pub(crate) use flow::{TableFlow, TableFlowError};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(crate) struct Cell {
    pub table: NodeId,
    pub row: usize,
    pub after_row: usize,
    fractions: Option<(u16, u16)>,
    pub column: u8,
    pub span: u8,
    pub columns: u8,
    pub rtl: bool,
}

pub(crate) struct TableCells(HashMap<NodeId, Cell>);

impl TableCells {
    pub fn read(body: Node<'_, '_>, visible_elements: &HashMap<NodeId, usize>) -> Self {
        let mut positions = HashMap::new();
        for table in body.descendants().filter(|node| node.has_tag_name("table")) {
            if !visible_elements.contains_key(&table.id())
                || table
                    .ancestors()
                    .skip(1)
                    .any(|node| node.has_tag_name("table"))
                || table
                    .descendants()
                    .skip(1)
                    .any(|node| node.has_tag_name("table"))
            {
                continue;
            }
            if let Some(cells) = table_cells(table, visible_elements) {
                positions.extend(cells);
            }
        }
        Self(positions)
    }

    pub fn by_element(&self, visible: &HashMap<NodeId, usize>) -> Vec<(usize, Cell)> {
        let mut result: Vec<_> = self
            .0
            .iter()
            .filter_map(|(node, cell)| Some((*visible.get(node)?, *cell)))
            .collect();
        result.sort_unstable_by_key(|(index, _)| *index);
        result
    }
    pub fn containing(&self, block: Node<'_, '_>) -> Option<Cell> {
        if self.0.is_empty() {
            return None;
        }
        block
            .ancestors()
            .find_map(|node| self.0.get(&node.id()).copied())
    }
}

fn table_cells(
    table: Node<'_, '_>,
    visible: &HashMap<NodeId, usize>,
) -> Option<Vec<(NodeId, Cell)>> {
    let mut cells = Vec::new();
    let mut columns = 0;
    let rtl = table
        .ancestors()
        .find_map(|node| node.attribute("dir"))
        .is_some_and(|dir| dir.eq_ignore_ascii_case("rtl"));
    let rows: Vec<_> = table
        .descendants()
        .filter(|node| node.has_tag_name("tr") && visible.contains_key(&node.id()))
        .collect();
    let mut occupied_until = [0usize; MAX_COLUMNS];
    let mut group_end = 0;
    for (index, row) in rows.iter().enumerate() {
        if index == group_end {
            group_end = index
                + rows[index..]
                    .iter()
                    .take_while(|other| other.parent() == row.parent())
                    .count();
        }
        let mut column = 0;
        for cell in row.children().filter(|node| {
            visible.contains_key(&node.id()) && matches!(node.tag_name().name(), "td" | "th")
        }) {
            let span = span_attribute(cell, "colspan").max(1);
            if span > MAX_COLUMNS {
                return None;
            }
            // Reserve all spanned columns, including slots occupied by preceding rows.
            while column + span <= MAX_COLUMNS
                && occupied_until[column..column + span]
                    .iter()
                    .any(|&end| end > index)
            {
                column += 1;
            }
            if column + span > MAX_COLUMNS {
                return None;
            }
            let rows_high = span_attribute(cell, "rowspan");
            let after_row = if rows_high == 0 {
                group_end
            } else {
                index + rows_high.min(group_end - index)
            };
            occupied_until[column..column + span].fill(after_row);
            cells.push((
                cell.id(),
                Cell {
                    table: table.id(),
                    row: index,
                    after_row,
                    column: column as u8,
                    span: span as u8,
                    columns: 0,
                    fractions: None,
                    rtl,
                },
            ));
            column += span;
        }
        columns = columns.max(column);
    }
    if columns < 2 {
        return None;
    }
    let edges = column_edges(table, columns);
    for (_, cell) in &mut cells {
        cell.columns = columns as u8;
        let start = usize::from(cell.column);
        cell.fractions = edges
            .as_ref()
            .map(|edges| (edges[start], edges[start + usize::from(cell.span)]));
    }
    Some(cells)
}

fn span_attribute(node: Node<'_, '_>, key: &str) -> usize {
    node.attribute(key)
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(1)
}

const SCALE: u16 = 10000;

fn percent(width: &str) -> Option<f32> {
    let n = width.trim().strip_suffix('%')?.trim().parse::<f32>().ok()?;
    (n.is_finite() && n > 0.0 && n <= 100.0).then_some(n)
}

fn column_width(node: Node<'_, '_>) -> Option<f32> {
    let style = node.attribute("style").unwrap_or_default();
    style
        .split(';')
        .filter_map(|d| d.split_once(':'))
        .filter(|(name, _)| name.trim().eq_ignore_ascii_case("width"))
        .filter_map(|(_, v)| percent(v))
        .last()
        .or_else(|| node.attribute("width").and_then(percent))
}

fn column_edges(table: Node<'_, '_>, count: usize) -> Option<Vec<u16>> {
    let mut hinted = Vec::with_capacity(count);
    for node in table.children().filter(Node::is_element) {
        if node.has_tag_name("col") {
            add_column(&mut hinted, node, None, count);
        } else if node.has_tag_name("colgroup") {
            let mut children = node.children().filter(|n| n.has_tag_name("col")).peekable();
            if children.peek().is_none() {
                add_column(&mut hinted, node, None, count);
            } else {
                for child in children {
                    add_column(&mut hinted, child, column_width(node), count);
                }
            }
        }
    }
    if hinted.iter().all(Option::is_none) {
        return None;
    }
    hinted.resize(count, None);
    let explicit: f32 = hinted.iter().flatten().sum();
    let unspecified = hinted.iter().filter(|v| v.is_none()).count();
    let default = ((100.0 - explicit) / unspecified.max(1) as f32).max(1.0);
    let total = explicit + default * unspecified as f32;
    let mut edges = vec![0];
    let mut sum = 0.0;
    for width in hinted {
        sum += width.unwrap_or(default);
        edges.push((sum / total * f32::from(SCALE)).round() as u16);
    }
    edges[count] = SCALE;
    Some(edges)
}

fn add_column(
    out: &mut Vec<Option<f32>>,
    node: Node<'_, '_>,
    inherited: Option<f32>,
    count: usize,
) {
    let fill = span_attribute(node, "span").max(1).min(count - out.len());
    out.resize(out.len() + fill, column_width(node).or(inherited));
}

impl Cell {
    pub(crate) fn has_column_tracks(self) -> bool {
        self.fractions.is_some()
    }

    pub fn bounds(self, width: u32, padding: u32) -> (u32, u32) {
        let (left, right) = match self.fractions {
            Some((mut left, mut right)) => {
                if self.rtl {
                    (left, right) = (SCALE - right, SCALE - left);
                }
                let edge =
                    |p: u16| (width * u32::from(p) + u32::from(SCALE) / 2) / u32::from(SCALE);
                (edge(left), edge(right))
            }
            None => {
                let start = if self.rtl {
                    self.columns - self.column - self.span
                } else {
                    self.column
                };
                let edge = |i: u8| width * u32::from(i) / u32::from(self.columns);
                (edge(start), edge(start + self.span))
            }
        };
        let inset = padding.min((right - left).saturating_sub(1) / 2);
        (
            left + inset,
            (right - left).saturating_sub(2 * inset).max(1),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn merged_cells_keep_their_columns_until_they_end_and_honor_publisher_widths() {
        let doc=roxmltree::Document::parse("<table><colgroup><col style='width: 20%'/><col width='50%'/><col/></colgroup><tbody><tr><th id='label' rowspan='0'>Period</th><td id='name'>Morning</td><td>4.2</td></tr><tr><td id='name2'>Sunday</td><td>4.9</td></tr></tbody><tbody><tr><td id='footer' colspan='3'>End</td></tr></tbody></table>").unwrap();
        let root = doc.root_element();
        let visible = root
            .descendants()
            .filter(Node::is_element)
            .enumerate()
            .map(|(i, n)| (n.id(), i))
            .collect();
        let cells = TableCells::read(root, &visible);
        let named = |id| {
            cells
                .containing(
                    root.descendants()
                        .find(|n| n.attribute("id") == Some(id))
                        .unwrap(),
                )
                .unwrap()
        };
        let (label, first, second, footer) = (
            named("label"),
            named("name"),
            named("name2"),
            named("footer"),
        );
        assert_eq!(label.bounds(1000, 0), (0, 200));
        assert_eq!(first.bounds(1000, 0), (200, 500));
        assert_eq!(second.bounds(1000, 0), (200, 500));
        assert_eq!(footer.bounds(1000, 0), (0, 1000));
        let mut flow = RowFlow::default();
        assert_eq!(flow.enter(Some(label), 100.0), 100.0);
        assert_eq!(flow.enter(Some(first), 400.0), 100.0);
        assert_eq!(flow.enter(Some(second), 175.0), 175.0);
        assert_eq!(flow.enter(Some(footer), 225.0), 400.0);
        assert_eq!(flow.enter(None, 460.0), 460.0);
    }

    #[test]
    fn headers_form_a_row_and_each_cell_keeps_its_own_touch_target() {
        let path = crate::test_fixture("field-notes-epub3.epub");
        let html = "<html><body><p>Morning readings</p><table><thead><tr><th id='a'>Station</th><th id='b'>Depth</th><th id='c'>Tide</th></tr></thead><tbody><tr><td><p>Basin</p><p>east</p></td><td>Four fathoms</td><td><a href='#b'>Falling</a></td></tr><tr><td colspan='3'>End of survey</td></tr></tbody></table><p id='post'>Afterward</p></body></html>";
        let fonts = [crate::test_fixture("fonts/NotoSans-Regular.ttf")];
        for width in [680, 1264] {
            let settings = crate::Settings {
                width,
                height: 920,
                font_size: 32.0,
                margin: 24,
                ..Default::default()
            };
            let mut probe = crate::Probe::from_book_content(
                crate::epub::Book::open(&path).unwrap(),
                crate::Engine::Parley,
                &fonts,
                settings,
                0,
                Some(html.to_string()),
            )
            .unwrap();
            let mut headers = Vec::new();
            for (id, label) in [("a", "Station"), ("b", "Depth"), ("c", "Tide")] {
                let start = probe.fragment(id).unwrap();
                let mut end = start.clone();
                end.offset += label.len();
                let selected = probe.select_range(&start, &end).unwrap();
                let page = probe.page_for(&start).unwrap();
                let rect = probe.selection_rects(page, &selected)[0];
                let x = (rect.x + rect.width / 2) as f32;
                let y = (rect.y + rect.height / 2) as f32;
                assert_eq!(probe.word_at(page, x, y).unwrap().text, label);
                assert_eq!(
                    probe.text_position_at(page, x, y).unwrap().block,
                    start.block
                );
                headers.push(rect);
            }
            assert_eq!(headers[0].y, headers[1].y);
            assert_eq!(headers[1].y, headers[2].y);
            assert!(headers[1].x - headers[0].x > (width / 5) as i32);
            assert!(headers[2].x - headers[1].x > (width / 5) as i32);
            let post = probe.fragment("post").unwrap();
            let mut end = post.clone();
            end.offset += "Afterward".len();
            let post_rect = probe.selection_rects(0, &probe.select_range(&post, &end).unwrap())[0];
            assert!(post_rect.y > headers[0].y + 100);
            assert_eq!(probe.coverage().0, probe.coverage().1);
            let frame = probe.capture(0).unwrap();
            assert!(frame.iter().any(|v| *v < 150));
        }
    }
    #[test]
    fn spanned_headings_nested_blocks_empty_cells_and_rtl_share_column_boundaries() {
        let doc = roxmltree::Document::parse("<table dir='rtl'><thead><tr><th colspan='2'>Pair</th><th>Unit</th></tr></thead><tbody><tr><td/><td><p>Value</p></td><td>kg</td></tr></tbody></table>").unwrap();
        let table = doc.root_element();
        let visible = table
            .descendants()
            .filter(Node::is_element)
            .enumerate()
            .map(|(i, n)| (n.id(), i))
            .collect();
        let layout = TableCells::read(table, &visible);
        let cells: Vec<_> = table
            .descendants()
            .filter(|n| matches!(n.tag_name().name(), "th" | "td"))
            .map(|n| layout.containing(n).unwrap())
            .collect();
        assert_eq!(cells[0].bounds(900, 4), (304, 592));
        assert_eq!(cells[1].bounds(900, 4), (4, 292));
        assert_eq!(cells[4].bounds(900, 4), cells[1].bounds(900, 4));
        assert_eq!(
            layout.containing(table.descendants().find(|n| n.has_tag_name("p")).unwrap()),
            Some(cells[3])
        );
        assert_ne!(cells[0].row, cells[3].row);
    }
}

/// The next block in a cell follows the previous block vertically. Neighboring
/// cells begin together, and subsequent prose follows the tallest cell.
#[derive(Default)]
pub(crate) struct RowFlow {
    cell: Option<Cell>,
    row_ends: BTreeMap<usize, f32>,
    top: f32,
    bottom: f32,
}

impl RowFlow {
    pub fn enter(&mut self, next: Option<Cell>, end: f32) -> f32 {
        self.bottom = self.bottom.max(end);
        if self.cell == next {
            return end;
        }
        if let Some(previous) = self.cell {
            let required = self.row_ends.entry(previous.after_row).or_insert(self.top);
            *required = required.max(end);
        }
        if self.cell.map(|c| c.table) != next.map(|c| c.table) {
            self.top = self.bottom;
            self.row_ends.clear();
        } else if let Some(cell) = next {
            while self
                .row_ends
                .first_key_value()
                .is_some_and(|(&after, _)| after <= cell.row)
            {
                if let Some((_, bottom)) = self.row_ends.pop_first() {
                    self.top = self.top.max(bottom);
                }
            }
        }
        self.cell = next;
        self.top
    }

    pub fn end(&self, y: f32) -> f32 {
        self.bottom.max(y)
    }
}
