//! A native table grid over retained CSS boxes; canonical cells never change.
use super::{
    Cell, MAX_COLUMNS,
    css::{self, CssTables},
};
use crate::{
    book_style::Style,
    source::{Block, Source},
};
use std::{
    borrow::Cow,
    collections::{BTreeMap, HashMap, HashSet},
    fmt,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TableFlowError {
    HintReprojection,
}
impl fmt::Display for TableFlowError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("CSS-hidden cells change publisher column hints; a readable source-order table fallback is required")
    }
}
impl std::error::Error for TableFlowError {}

pub(crate) struct TableFlow<'a> {
    cells: Cow<'a, [(usize, Cell)]>,
    // Only a transformed grid needs this sparse lookup. In particular, normal
    // chapters allocate neither a dense element index nor a copied cell array.
    remapped: Option<HashMap<Cell, Option<Cell>>>,
    rows: Option<HashMap<usize, Option<usize>>>,
    css: Option<CssTables>,
}

impl<'a> TableFlow<'a> {
    pub(crate) fn identity(source: &'a Source) -> Self {
        Self {
            cells: Cow::Borrowed(&source.table_cells),
            remapped: None,
            rows: None,
            css: None,
        }
    }
    pub(crate) fn is_identity(&self) -> bool {
        self.remapped.is_none() && self.css.is_none()
    }
    /// An explicit degraded table view. The caller reports the narrowly typed
    /// hint limitation; CSS text masks and canonical positions stay unchanged.
    pub(crate) fn source_order(source: &'a Source) -> Self {
        Self {
            cells: Cow::Owned(Vec::new()),
            remapped: Some(
                source
                    .table_cells
                    .iter()
                    .map(|(_, cell)| (*cell, None))
                    .collect(),
            ),
            rows: Some(HashMap::new()),
            css: None,
        }
    }
    pub(crate) fn with_repaired_css(mut self, source: &Source, styles: &[Style]) -> Self {
        if let Some(repair) = CssTables::plan(source, styles) {
            let cells = self.cells.to_mut();
            cells.extend(repair.cells());
            cells.sort_unstable_by_key(|entry| entry.0);
            self.css = Some(repair);
        }
        self
    }
    pub(crate) fn has_repaired_css(&self) -> bool {
        self.css.is_some()
    }
    pub(crate) fn css_grid(&self, key: usize) -> Option<css::Grid> {
        self.css.as_ref()?.grid(key)
    }
    pub(crate) fn css_root(&self, element: usize) -> bool {
        self.css
            .as_ref()
            .is_some_and(|css| css.explicit_root(element))
    }
    pub(crate) fn css_owner(&self, element: usize) -> Option<usize> {
        self.css.as_ref()?.owner(element)
    }
    pub(crate) fn cells(&self) -> &[(usize, Cell)] {
        &self.cells
    }
    pub(crate) fn cell(&self, element: usize) -> Option<Cell> {
        self.cells
            .binary_search_by_key(&element, |entry| entry.0)
            .ok()
            .map(|i| self.cells[i].1)
    }
    pub(crate) fn block_cell(&self, block: &Block) -> Option<Cell> {
        if let Some(owner) = self.css_owner(block.element) {
            return self.cell(owner);
        }
        let old = block.table_cell?;
        self.remapped
            .as_ref()
            .map_or(Some(old), |map| map.get(&old).copied().flatten())
    }
    /// A retained empty row can be spanned by an earlier cell and still own
    /// collapsed-edge metadata, even though no cell starts on this row.
    pub(crate) fn row_index(&self, element: usize, original_row: usize) -> Option<usize> {
        self.rows.as_ref().map_or(Some(original_row), |rows| {
            rows.get(&element).copied().flatten()
        })
    }

    pub(crate) fn new(source: &'a Source, styles: &[Style]) -> crate::Result<Self> {
        if styles.len() != source.elements.len() {
            return Err("The table styles do not match the chapter".into());
        }
        let use_css_columns = styles.first().is_some_and(|style| style.ignore_table_hints);
        let effective = |cell: Cell| {
            if use_css_columns {
                cell.css_columns()
            } else {
                cell
            }
        };
        if source.table_cells.is_empty()
            || !styles.iter().zip(&source.elements).any(|(style, element)| {
                style.flow.suppressed
                    && matches!(
                        element.tag.as_str(),
                        "table"
                            | "thead"
                            | "tbody"
                            | "tfoot"
                            | "tr"
                            | "td"
                            | "th"
                            | "col"
                            | "colgroup"
                    )
            })
        {
            if !use_css_columns
                || source
                    .table_cells
                    .iter()
                    .all(|&(_, cell)| cell == effective(cell))
            {
                return Ok(Self::identity(source));
            }
            let mut flow = Self::identity(source);
            flow.remapped = Some(
                source
                    .table_cells
                    .iter()
                    .map(|&(_, cell)| (cell, Some(effective(cell))))
                    .collect(),
            );
            flow.cells = Cow::Owned(
                source
                    .table_cells
                    .iter()
                    .map(|&(element, cell)| (element, effective(cell)))
                    .collect(),
            );
            return Ok(flow);
        }
        let mut groups: HashMap<_, Vec<_>> = HashMap::new();
        for &(element, cell) in &source.table_cells {
            groups.entry(cell.table).or_default().push((element, cell));
        }
        let mut table_owner = Vec::with_capacity(source.elements.len());
        let mut table_rows = HashMap::<usize, Vec<usize>>::new();
        let mut hidden_columns = HashSet::new();
        for (i, element) in source.elements.iter().enumerate() {
            let table = if element.tag == "table" {
                Some(i)
            } else {
                element.parent.and_then(|p| table_owner[p])
            };
            table_owner.push(table);
            if let Some(table) = table {
                if element.tag == "tr" && !element.selector_only {
                    table_rows.entry(table).or_default().push(i);
                }
                if matches!(element.tag.as_str(), "col" | "colgroup") && styles[i].flow.suppressed {
                    hidden_columns.insert(table);
                }
            }
        }
        let mut out = Vec::with_capacity(source.table_cells.len());
        let mut remapped = HashMap::with_capacity(source.table_cells.len());
        let mut row_indices = HashMap::new();
        for entries in groups.into_values() {
            let table =
                table_owner[entries[0].0].ok_or("The table cell has no native table owner")?;
            let rows = table_rows.remove(&table).unwrap_or_default();
            let mut row_map = Vec::with_capacity(rows.len());
            let mut row_count = 0;
            for &row in &rows {
                let mapped = (!styles[row].flow.suppressed && !styles[table].flow.suppressed)
                    .then_some(row_count);
                row_map.push(mapped);
                row_indices.insert(row, mapped);
                if mapped.is_some() {
                    row_count += 1;
                }
            }
            if styles[table].flow.suppressed
                || entries.iter().all(|&(i, _)| styles[i].flow.suppressed)
            {
                for (_, cell) in entries {
                    remapped.insert(cell, None);
                }
                continue;
            }
            let removed = styles[table].flow.suppressed
                || rows.iter().any(|&i| styles[i].flow.suppressed)
                || entries.iter().any(|&(i, _)| styles[i].flow.suppressed);
            let columns_hidden = hidden_columns.contains(&table);
            if !removed && !columns_hidden {
                for (element, cell) in entries {
                    let resolved = effective(cell);
                    out.push((element, resolved));
                    remapped.insert(cell, Some(resolved));
                }
                continue;
            }
            if columns_hidden
                && entries
                    .iter()
                    .any(|&(_, cell)| effective(cell).fractions.is_some())
            {
                return Err(TableFlowError::HintReprojection.into());
            }
            let mut grouped = BTreeMap::<usize, Vec<_>>::new();
            for (element, cell) in entries {
                remapped.insert(cell, None);
                if !styles[element].flow.suppressed && !styles[table].flow.suppressed {
                    let row = row_map.get(cell.row).copied().flatten();
                    if let Some(row) = row {
                        grouped.entry(row).or_default().push((element, cell));
                    }
                }
            }
            let mut occupied = [0usize; MAX_COLUMNS];
            let mut projected = Vec::new();
            let mut columns = 0;
            for (row, entries) in grouped {
                let mut column = 0;
                for (element, old) in entries {
                    let span = usize::from(old.span);
                    while column + span <= MAX_COLUMNS
                        && occupied[column..column + span].iter().any(|&end| end > row)
                    {
                        column += 1;
                    }
                    if span == 0 || column + span > MAX_COLUMNS {
                        return Err(
                            "The CSS-projected table exceeds the 32-column grid limit".into()
                        );
                    }
                    // Pinned CRE reads rowspan after removing display:none rows;
                    // its duration counts remaining rows, not old DOM row indices.
                    // Preserve Reader's pre-existing bounded HTML duration here.
                    let after_row = row
                        + old
                            .after_row
                            .saturating_sub(old.row)
                            .max(1)
                            .min(row_count - row);
                    occupied[column..column + span].fill(after_row);
                    let cell = Cell {
                        row,
                        after_row,
                        column: column as u8,
                        fractions: None,
                        ..effective(old)
                    };
                    projected.push((element, old, cell));
                    column += span;
                    columns = columns.max(column);
                }
            }
            for (element, old, mut cell) in projected {
                cell.columns = columns as u8;
                if effective(old).fractions.is_some() {
                    if (old.column, old.span, old.columns) != (cell.column, cell.span, cell.columns)
                    {
                        return Err(TableFlowError::HintReprojection.into());
                    }
                    cell.fractions = effective(old).fractions;
                }
                remapped.insert(old, Some(cell));
                out.push((element, cell));
            }
        }
        out.sort_unstable_by_key(|entry| entry.0);
        Ok(Self {
            cells: Cow::Owned(out),
            remapped: Some(remapped),
            rows: Some(row_indices),
            css: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::book_style::BookStyle;
    fn styled(body: &str) -> (Source, Vec<Style>) {
        let source = Source::parse(&format!("<html><body>{body}</body></html>")).unwrap();
        let mut book = crate::Book::open(&crate::test_fixture("field-notes-epub3.epub")).unwrap();
        let href = book.chapters[0].href.clone();
        let styles = BookStyle::load_for_flow(&mut book, &href, &source, true, &Default::default())
            .unwrap()
            .compute(&source.elements);
        (source, styles)
    }
    fn id(source: &Source, id: &str) -> usize {
        source
            .elements
            .iter()
            .position(|e| e.id.as_deref() == Some(id))
            .unwrap()
    }
    #[test]
    fn identity_is_borrowed_and_visibility_cannot_change_any_cell_geometry() {
        let (source, styles) = styled(
            "<table><tr><td style='visibility:hidden'>Hidden</td><td>Visible</td></tr></table>",
        );
        let plan = TableFlow::new(&source, &styles).unwrap();
        assert!(plan.is_identity());
        assert!(matches!(plan.cells, Cow::Borrowed(_)));
        assert_eq!(plan.cells(), source.table_cells.as_slice());
        for block in &source.blocks {
            assert_eq!(plan.block_cell(block), block.table_cell);
        }
    }
    #[test]
    fn removed_rows_and_cells_repack_and_keep_rowspan_duration_in_retained_rows() {
        let (source, styles) = styled(
            "<table><tr><td id='a' rowspan='2'>A</td><td style='display:none'>Gone</td><td id='b'>B</td></tr><tr style='display:none'><td>Gone row</td></tr><tr><td id='c' colspan='2'>C</td></tr><tr><td id='d'>D</td><td>E</td></tr></table>",
        );
        let canonical = source.table_cells.clone();
        let plan = TableFlow::new(&source, &styles).unwrap();
        let a = plan.cell(id(&source, "a")).unwrap();
        let b = plan.cell(id(&source, "b")).unwrap();
        let c = plan.cell(id(&source, "c")).unwrap();
        let d = plan.cell(id(&source, "d")).unwrap();
        assert_eq!((a.row, a.after_row, a.column, a.columns), (0, 2, 0, 3));
        assert_eq!((b.row, b.column), (0, 1));
        assert_eq!((c.row, c.column, c.span), (1, 1, 2));
        assert_eq!((d.row, d.column), (2, 0));
        assert_eq!(source.table_cells, canonical);
        for block in &source.blocks {
            assert_eq!(
                plan.block_cell(block).is_none(),
                styles[block.element].flow.suppressed
            );
        }
    }
    #[test]
    fn a_single_surviving_column_keeps_its_native_cell_and_fills_available_width() {
        let (source, styles) = styled(
            "<table dir='rtl'><tr><td style='display:none'>Gone</td><td id='kept'>Kept</td></tr></table>",
        );
        let plan = TableFlow::new(&source, &styles).unwrap();
        let cell = plan.cell(id(&source, "kept")).unwrap();
        assert_eq!(
            (cell.column, cell.span, cell.columns, cell.rtl),
            (0, 1, 1, true)
        );
        assert_eq!(cell.bounds(600, 0), (0, 600));
    }
    #[test]
    fn removing_a_whole_table_is_empty_without_changing_canonical_cells() {
        let (source, styles) =
            styled("<table style='display:none'><tr><td>A</td><td>B</td></tr></table>");
        let plan = TableFlow::new(&source, &styles).unwrap();
        assert!(!source.table_cells.is_empty());
        assert!(plan.cells().is_empty());
        assert!(
            source
                .blocks
                .iter()
                .all(|block| plan.block_cell(block).is_none())
        );
    }
    #[test]
    fn ambiguous_changed_column_hints_are_explicit_errors_not_invented_widths() {
        let (source, styles) = styled(
            "<table><colgroup><col width='25%'/><col width='75%'/></colgroup><tr><td style='display:none'>Gone</td><td>Kept</td></tr></table>",
        );
        let canonical = source.table_cells.clone();
        let error = TableFlow::new(&source, &styles).err().unwrap();
        assert_eq!(
            error.downcast_ref::<TableFlowError>(),
            Some(&TableFlowError::HintReprojection)
        );
        let fallback = TableFlow::source_order(&source);
        assert!(fallback.cells().is_empty());
        assert!(
            source
                .blocks
                .iter()
                .all(|block| fallback.block_cell(block).is_none())
        );
        assert_eq!(source.table_cells, canonical);
        let (source, styles) = styled(
            "<table><col width='25%'/><col width='75%'/><tr style='display:none'><td>Gone</td><td>Gone</td></tr><tr><td>A</td><td>B</td></tr></table>",
        );
        let plan = TableFlow::new(&source, &styles).unwrap();
        assert!(
            plan.cells()
                .iter()
                .all(|(_, cell)| cell.row == 0 && cell.fractions.is_some())
        );
    }
    #[test]
    fn projected_overflow_and_style_mismatch_are_not_hint_fallback_errors() {
        let (source, styles) = styled(
            "<table><tr><td rowspan='2' colspan='16'>A</td><td colspan='16'>B</td></tr><tr style='display:none'><td colspan='16'>Removed row</td></tr><tr><td colspan='32'>Wide after row</td></tr></table>",
        );
        assert!(!source.table_cells.is_empty());
        let error = TableFlow::new(&source, &styles).err().unwrap();
        assert!(error.to_string().contains("32-column"));
        assert!(error.downcast_ref::<TableFlowError>().is_none());
        assert!(TableFlow::new(&source, &styles[..styles.len() - 1]).is_err());
    }
}
