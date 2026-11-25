//! Table-local vertical tracks, measured before cell content is positioned.

use std::ops::Range;

use super::Cell;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CellAlignment {
    Baseline,
    Top,
    Middle,
    Bottom,
}

impl CellAlignment {
    pub(crate) fn from_css(value: parley::VerticalAlign) -> Self {
        match value {
            parley::VerticalAlign::TOP => Self::Top,
            parley::VerticalAlign::MIDDLE => Self::Middle,
            parley::VerticalAlign::BOTTOM => Self::Bottom,
            _ => Self::Baseline,
        }
    }
}

#[derive(Clone, Copy, Default)]
pub(crate) struct Insets {
    pub(crate) head: u32,
    pub(crate) spacing: u32,
    pub(crate) tail: u32,
}

impl Insets {
    pub(crate) fn before(self, row: usize) -> f32 {
        (if row == 0 { self.head } else { self.spacing }) as f32
    }
}

pub(crate) struct Measure {
    pub(crate) cell: Cell,
    pub(crate) alignment: CellAlignment,
    pub(crate) height: f32,
    pub(crate) baseline: f32,
}

pub(crate) struct Tracks {
    /// Each slot starts before its leading table padding or inter-row spacing.
    pub(crate) rows: Vec<Range<f32>>,
    pub(crate) height: f32,
    pub(crate) padding: Vec<f32>,
}

pub(crate) fn solve(cells: &[Measure], count: usize, insets: Insets) -> Option<Tracks> {
    if !(1..=8192).contains(&count)
        || cells.iter().any(|c| {
            c.cell.row >= c.cell.after_row
                || c.cell.after_row > count
                || !c.height.is_finite()
                || !c.baseline.is_finite()
                || c.height < 0.0
                || c.baseline < 0.0
                || c.baseline > c.height
        })
    {
        return None;
    }
    let mut baselines = vec![0.0_f32; count];
    for cell in cells
        .iter()
        .filter(|c| c.alignment == CellAlignment::Baseline)
    {
        let row = &mut baselines[cell.cell.row];
        *row = row.max(cell.baseline);
    }
    let baseline_pad = |c: &Measure| baselines[c.cell.row] - c.baseline;
    let mut heights = vec![0.0_f32; count];
    for c in cells.iter().filter(|c| c.cell.after_row == c.cell.row + 1) {
        let pad = if c.alignment == CellAlignment::Baseline {
            baseline_pad(c)
        } else {
            0.0
        };
        let row = &mut heights[c.cell.row];
        *row = row.max(c.height + pad);
    }
    // Like CRE, fill spanning deficits using natural heights. Inter-row
    // border spacing is applied later. The earlier rows get any remainder.
    for c in cells.iter().filter(|c| c.cell.after_row > c.cell.row + 1) {
        let tracks = &mut heights[c.cell.row..c.cell.after_row];
        let extra = c.height - tracks.iter().sum::<f32>();
        if extra > 0.0 {
            let share = (extra / tracks.len() as f32).floor();
            let mut remainder = extra - share * tracks.len() as f32;
            for row in tracks {
                let first = remainder.min(1.0);
                *row += share + first;
                remainder -= first;
            }
        }
    }
    let mut y = 0.0;
    let rows: Vec<_> = heights
        .into_iter()
        .enumerate()
        .map(|(row, size)| {
            let start = y;
            y += insets.before(row) + size;
            start..y
        })
        .collect();
    let mut padding = Vec::with_capacity(cells.len());
    for c in cells {
        let row = c.cell.row;
        let available = rows[c.cell.after_row - 1].end - rows[row].start - insets.before(row);
        let free = available - c.height;
        let pad = if free <= 0.01 {
            0.0
        } else {
            match c.alignment {
                CellAlignment::Top => 0.0,
                CellAlignment::Middle => (free / 2.0).floor(),
                CellAlignment::Bottom => free,
                CellAlignment::Baseline => baseline_pad(c),
            }
        };
        // A baseline shift can outgrow an authored spanning box. Retain the
        // ordinary layout rather than placing content across its bottom edge.
        if pad > free + 0.01 {
            return None;
        }
        padding.push(pad);
    }
    Some(Tracks {
        rows,
        height: y + insets.tail as f32,
        padding,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(
        row: usize,
        after: usize,
        alignment: CellAlignment,
        height: f32,
        baseline: f32,
    ) -> Measure {
        Measure {
            cell: Cell {
                table: super::super::TableKey::Css(0),
                row,
                after_row: after,
                fractions: None,
                css_fractions: None,
                column: 0,
                span: 1,
                columns: 1,
                rtl: false,
            },
            alignment,
            height,
            baseline,
        }
    }

    #[test]
    fn table_vertical_baseline_grows_a_row_and_middle_and_bottom_use_its_full_height() {
        use CellAlignment::*;
        let tracks = solve(
            &[
                cell(0, 1, Baseline, 30., 8.),
                cell(0, 1, Baseline, 22., 16.),
                cell(0, 1, Top, 16., 10.),
                cell(0, 1, Middle, 15., 10.),
                cell(0, 1, Bottom, 14., 10.),
            ],
            1,
            Insets::default(),
        )
        .unwrap();
        assert_eq!(tracks.rows, [0.0..38.0]);
        assert_eq!(tracks.padding, [8., 0., 0., 11., 24.]);
    }

    #[test]
    fn table_vertical_spans_distribute_deficits_and_include_spacing_in_their_box() {
        use CellAlignment::*;
        let cells = [
            cell(0, 1, Top, 20., 13.),
            cell(1, 2, Top, 10., 7.),
            cell(0, 2, Top, 45., 20.),
            cell(0, 2, Bottom, 10., 7.),
        ];
        let tracks = solve(
            &cells,
            2,
            Insets {
                head: 3,
                spacing: 4,
                tail: 2,
            },
        )
        .unwrap();
        assert_eq!(tracks.rows, [0.0..31.0, 31.0..52.0]);
        assert_eq!(tracks.height, 54.);
        assert_eq!(tracks.padding, [0., 0., 0., 39.]);

        let exact = solve(
            &[
                cell(0, 1, Baseline, 22., 16.),
                cell(0, 2, Baseline, 45., 8.),
            ],
            2,
            Insets::default(),
        )
        .unwrap();
        assert_eq!(exact.height, 45.);
        assert_eq!(exact.padding, [0., 0.]);
    }
}
