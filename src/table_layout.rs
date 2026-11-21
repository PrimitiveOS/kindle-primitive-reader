//! Column positions for simple reflowable XHTML tables. Text stays in its original
//! source block, so a cell's links, selection and saved locations still use book offsets.
use roxmltree::{Node, NodeId};
use std::collections::HashMap;

const MAX_COLUMNS: usize = 32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Cell {
    pub row: NodeId,
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
    for row in table
        .descendants()
        .filter(|node| node.has_tag_name("tr") && visible.contains_key(&node.id()))
    {
        let mut column = 0;
        for cell in row.children().filter(|node| {
            visible.contains_key(&node.id()) && matches!(node.tag_name().name(), "td" | "th")
        }) {
            // A vertically merged cell needs coordinated row heights. Keep such a
            // table in source reading order until it can be laid out as a complete grid.
            if cell
                .attribute("rowspan")
                .is_some_and(|value| value.trim().parse::<usize>().ok() != Some(1))
            {
                return None;
            }
            let span = cell
                .attribute("colspan")
                .unwrap_or("1")
                .trim()
                .parse::<usize>()
                .unwrap_or(1)
                .max(1);
            if span > MAX_COLUMNS - column {
                return None;
            }
            cells.push((
                cell.id(),
                Cell {
                    row: row.id(),
                    column: column as u8,
                    span: span as u8,
                    columns: 0,
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
    for (_, cell) in &mut cells {
        cell.columns = columns as u8;
    }
    Some(cells)
}

impl Cell {
    pub fn bounds(self, width: u32, padding: u32) -> (u32, u32) {
        let n = u32::from(self.columns);
        let start = u32::from(if self.rtl {
            self.columns - self.column - self.span
        } else {
            self.column
        });
        let left = width * start / n;
        let right = width * (start + u32::from(self.span)) / n;
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
    cell: Option<(NodeId, u8)>,
    top: f32,
    bottom: f32,
}

impl RowFlow {
    pub fn enter(&mut self, next: Option<Cell>, end: f32) -> f32 {
        let next = next.map(|c| (c.row, c.column));
        self.bottom = self.bottom.max(end);
        if self.cell == next {
            return end;
        }
        if self.cell.map(|(row, _)| row) != next.map(|(row, _)| row) {
            self.top = self.bottom;
        }
        self.cell = next;
        self.top
    }

    pub fn end(&self, y: f32) -> f32 {
        self.bottom.max(y)
    }
}
