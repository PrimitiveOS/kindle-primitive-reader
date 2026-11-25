//! Table repair follows computed display while leaving publisher source boxes intact.
use super::{Cell, MAX_COLUMNS, TableKey};
use crate::{
    book_style::{Style, WritingMode, flow::Display},
    source::{Source, is_block_tag},
};
use std::collections::{BTreeMap, HashSet};

const MAX_REPAIRED_CELLS: usize = 2048;

#[derive(Clone, Copy)]
pub(crate) struct Grid {
    pub explicit_root: Option<usize>,
}

pub(super) struct CssTables {
    cells: Vec<(usize, Cell)>,
    grids: BTreeMap<usize, Grid>,
    owners: Vec<Option<usize>>,
}

impl CssTables {
    pub(super) fn plan(source: &Source, styles: &[Style]) -> Option<Self> {
        if !source.elements.iter().zip(styles).any(|(el, style)| {
            available(el, style)
                && style.display == Display::TableCell
                && !matches!(el.tag.as_str(), "td" | "th")
        }) {
            return None;
        }
        let mut children = vec![Vec::new(); source.elements.len()];
        let mut inside_table = Vec::with_capacity(children.len());
        for (index, el) in source.elements.iter().enumerate() {
            if let Some(parent) = el.parent {
                children[parent].push(index);
            }
            inside_table.push(el.parent.is_some_and(|parent| {
                inside_table[parent]
                    || is_table_role(styles[parent].display)
                    || source.elements[parent].tag == "table"
            }));
        }
        let supported = SourceCells::new(source, styles);
        let mut plan = Self {
            cells: Vec::new(),
            grids: BTreeMap::new(),
            owners: vec![None; source.elements.len()],
        };
        for (index, el) in source.elements.iter().enumerate() {
            if !available(el, &styles[index])
                || inside_table[index]
                || !is_block_tag(&el.tag)
                || el.anonymous
            {
                continue;
            }
            match styles[index].display {
                Display::Table if el.tag != "table" => {
                    let items = &children[index];
                    if items.iter().all(|&part| {
                        styles[part].flow.suppressed || table_child(styles[part].display)
                    }) {
                        plan.append(Some(index), items, &supported, &children, styles);
                    }
                }
                Display::Block => {
                    for run in children[index].split(|&part| {
                        !styles[part].flow.suppressed && !table_child(styles[part].display)
                    }) {
                        plan.append(None, run, &supported, &children, styles);
                    }
                }
                _ => (),
            }
        }
        if plan.grids.is_empty() {
            return None;
        }
        let set: HashSet<_> = plan.cells.iter().map(|&(index, _)| index).collect();
        for (index, el) in source.elements.iter().enumerate() {
            plan.owners[index] = if set.contains(&index) {
                Some(index)
            } else {
                el.parent.and_then(|parent| plan.owners[parent])
            };
        }
        Some(plan)
    }

    fn append(
        &mut self,
        explicit_root: Option<usize>,
        items: &[usize],
        supported: &SourceCells,
        children: &[Vec<usize>],
        styles: &[Style],
    ) {
        let Some(rows) = build_rows(items, explicit_root.is_some(), children, styles) else {
            return;
        };
        let Some(&first) = rows.first().and_then(|row| row.first()) else {
            return;
        };
        let total = rows.iter().map(Vec::len).sum::<usize>();
        let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
        if columns < 2
            || columns > MAX_COLUMNS
            || total > MAX_REPAIRED_CELLS.saturating_sub(self.cells.len())
            || rows.iter().flatten().any(|&cell| !supported.eligible(cell))
        {
            return;
        }
        let key = explicit_root.unwrap_or(first);
        let rtl = supported.rtl.contains(&first);
        self.grids.insert(key, Grid { explicit_root });
        self.cells
            .extend(rows.into_iter().enumerate().flat_map(|(row, cells)| {
                cells.into_iter().enumerate().map(move |(column, element)| {
                    (
                        element,
                        Cell {
                            table: TableKey::Css(key),
                            row,
                            after_row: row + 1,
                            fractions: None,
                            css_fractions: None,
                            column: column as u8,
                            span: 1,
                            columns: columns as u8,
                            rtl,
                        },
                    )
                })
            }));
    }

    pub(super) fn cells(&self) -> impl Iterator<Item = (usize, Cell)> + '_ {
        self.cells.iter().copied()
    }
    pub(super) fn grid(&self, key: usize) -> Option<Grid> {
        self.grids.get(&key).copied()
    }
    pub(super) fn explicit_root(&self, index: usize) -> bool {
        self.grids
            .get(&index)
            .is_some_and(|grid| grid.explicit_root == Some(index))
    }
    pub(super) fn owner(&self, element: usize) -> Option<usize> {
        self.owners[element]
    }
}

fn available(element: &crate::source::Element, style: &Style) -> bool {
    !element.selector_only
        && !style.flow.suppressed
        && style.writing_mode == WritingMode::Horizontal
        && style.float_side.is_none()
}

fn table_child(display: Display) -> bool {
    matches!(
        display,
        Display::TableCell
            | Display::TableRow
            | Display::TableRowGroup
            | Display::TableHeaderGroup
            | Display::TableFooterGroup
    )
}
fn is_table_role(display: Display) -> bool {
    table_child(display) || matches!(display, Display::Table | Display::InlineTable)
}

fn build_rows(
    items: &[usize],
    explicit: bool,
    children: &[Vec<usize>],
    styles: &[Style],
) -> Option<Vec<Vec<usize>>> {
    let mut rows = Vec::new();
    let mut implicit_row = Vec::new();
    let flush = |implicit: &mut Vec<_>, rows: &mut Vec<_>| {
        if !implicit.is_empty() {
            rows.push(std::mem::take(implicit));
        }
    };
    for &index in items {
        if styles[index].flow.suppressed {
            if explicit {
                flush(&mut implicit_row, &mut rows);
            }
            continue;
        }
        match styles[index].display {
            Display::TableCell => implicit_row.push(index),
            Display::TableRow => {
                flush(&mut implicit_row, &mut rows);
                rows.push(row_cells(index, children, styles)?);
            }
            Display::TableRowGroup | Display::TableHeaderGroup | Display::TableFooterGroup => {
                flush(&mut implicit_row, &mut rows);
                for &row in &children[index] {
                    if !styles[row].flow.suppressed {
                        rows.push(row_cells(row, children, styles)?);
                    }
                }
            }
            _ => return None,
        }
    }
    flush(&mut implicit_row, &mut rows);
    rows.retain(|row| !row.is_empty());
    Some(rows)
}

fn row_cells(root: usize, children: &[Vec<usize>], styles: &[Style]) -> Option<Vec<usize>> {
    if styles[root].display != Display::TableRow {
        return None;
    }
    let retained: Vec<_> = children[root]
        .iter()
        .copied()
        .filter(|&child| !styles[child].flow.suppressed)
        .collect();
    retained
        .iter()
        .all(|&child| styles[child].display == Display::TableCell)
        .then_some(retained)
}

// Reject shapes whose text cannot be attributed to a single canonical cell,
// and grids containing nested CSS tables, media, floats or unsupported boxes.
struct SourceCells<'a> {
    source: &'a Source,
    styles: &'a [Style],
    unsupported: HashSet<usize>,
    rtl: HashSet<usize>,
}

impl<'a> SourceCells<'a> {
    fn new(source: &'a Source, styles: &'a [Style]) -> Self {
        let mut hosts = Vec::with_capacity(styles.len());
        let mut unsupported = HashSet::new();
        for (index, el) in source.elements.iter().enumerate() {
            let inherited = el.parent.and_then(|parent| hosts[parent]);
            if let Some(owner) = inherited
                && !styles[index].flow.suppressed
                && (is_table_role(styles[index].display) || styles[index].float_side.is_some())
            {
                unsupported.insert(owner);
            }
            hosts.push(if styles[index].display == Display::TableCell {
                Some(index)
            } else {
                inherited
            });
        }
        let mut rtl = HashSet::new();
        for block in &source.blocks {
            if styles[block.element].flow.suppressed {
                continue;
            }
            if let Some(host) = hosts[block.element] {
                if !block.images.is_empty()
                    || !block.ruby.is_empty()
                    || !available(&source.elements[block.element], &styles[block.element])
                    || block.list_item.is_some()
                    || block.table_cell.is_some()
                {
                    unsupported.insert(host);
                }
                if matches!(styles[block.element].direction, parley::BaseDirection::Rtl) {
                    rtl.insert(host);
                }
            }
        }
        Self {
            source,
            styles,
            unsupported,
            rtl,
        }
    }
    fn eligible(&self, cell: usize) -> bool {
        let el = &self.source.elements[cell];
        is_block_tag(&el.tag)
            && available(el, &self.styles[cell])
            && !self.unsupported.contains(&cell)
    }
}

#[cfg(test)]
mod tests;
