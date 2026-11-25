//! Plan a native table as one float, including its caption and decorated grid.
#[cfg(test)]
mod tests;

use std::ops::Range;

use super::{Clear, Side, Spec};
use crate::{
    book_style::{Style, WritingMode, flow::Display},
    publisher_padding::BoxArea,
    source::Source,
    table_layout::{TableKey, captions},
};

struct Candidate {
    elements: Range<usize>,
    blocks: Range<usize>,
    side: Side,
    clear: Clear,
    scope_end: usize,
}

pub(crate) struct Candidates(Vec<Candidate>);

impl Candidates {
    pub(crate) fn new(source: &Source, styles: &[Style], spans: &[Option<(usize, usize)>]) -> Self {
        let mut candidates = Vec::new();
        if !source.elements.iter().zip(styles).any(|(node, style)| {
            node.tag == "table" && style.float_side.is_some() && !style.flow.suppressed
        }) {
            return Self(candidates);
        }
        let mut ends = (1..=source.elements.len()).collect::<Vec<_>>();
        for (element, node) in source.elements.iter().enumerate().rev() {
            if let Some(parent) = node.parent {
                ends[parent] = ends[parent].max(ends[element]);
            }
        }
        let mut next_root = 0;
        for (owner, (node, style)) in source.elements.iter().zip(styles).enumerate() {
            let (Some(side), Some((first, last)), Some(parent)) =
                (style.float_side, spans[owner], node.parent)
            else {
                continue;
            };
            if owner < next_root
                || node.tag != "table"
                || style.flow.suppressed
                || style.display != Display::Table
                || style.writing_mode != WritingMode::Horizontal
                || source.elements[owner + 1..ends[owner]]
                    .iter()
                    .any(|node| node.tag == "table")
            {
                continue;
            }
            next_root = ends[owner];
            candidates.push(Candidate {
                elements: owner..next_root,
                blocks: first..last + 1,
                side,
                clear: style.clear,
                scope_end: spans[parent].map_or(last, |span| span.1),
            });
        }
        Self(candidates)
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(crate) fn contains(&self, owner: usize) -> bool {
        self.0
            .binary_search_by_key(&owner, |candidate| candidate.elements.start)
            .is_ok()
    }

    pub(crate) fn resolve(
        &self,
        source: &Source,
        styles: &[Style],
        boxes: &[BoxArea],
        width: u32,
        captions: &captions::Plan,
    ) -> Vec<Table> {
        let mut floats = Vec::new();
        for candidate in &self.0 {
            let owner = candidate.elements.start;
            let root = boxes[owner];
            let parent = boxes[source.elements[owner].parent.unwrap()];
            if root.outer_width == 0
                || root.outer_width > width
                || parent.cell.is_some()
                || parent.x != 0
                || parent.width != width
                || boxes[candidate.elements.clone()].iter().any(|area| {
                    area.x < root.outer_x
                        || area.outer_x < root.outer_x
                        || area.x + area.width > root.outer_x + root.outer_width
                        || area.outer_x + area.outer_width > root.outer_x + root.outer_width
                })
            {
                continue;
            }
            let blocks = candidate.blocks.clone().map(|slot| {
                let index = captions::source_index(captions.order(), slot);
                (index, source.blocks[index].element)
            });
            let Some(cell) = blocks.clone().find_map(|(_, element)| boxes[element].cell) else {
                continue;
            };
            if !matches!(cell.table, TableKey::Native(_))
                || !blocks
                    .filter(|&(_, element)| !styles[element].flow.suppressed)
                    .all(|(index, element)| {
                        candidate.elements.contains(&element)
                            && (boxes[element].cell.map(|cell| cell.table) == Some(cell.table)
                                || captions
                                    .placement(index)
                                    .is_some_and(|caption| caption.table == cell.table))
                    })
            {
                continue;
            }
            floats.push(Table {
                elements: candidate.elements.clone(),
                blocks: candidate.blocks.clone(),
                clear: candidate.clear,
                spec: Spec {
                    owner,
                    side: candidate.side,
                    width: root.outer_width,
                    publisher_height: None,
                    gray: None,
                    scope_end: candidate.scope_end,
                },
            });
        }
        floats.sort_by_key(Table::start);
        floats
    }
}

pub(crate) struct Table {
    elements: Range<usize>,
    blocks: Range<usize>,
    clear: Clear,
    spec: Spec,
}

impl Table {
    pub(crate) fn start(&self) -> usize {
        self.blocks.start
    }

    pub(crate) fn end(&self) -> usize {
        self.blocks.end
    }

    pub(crate) fn place(
        &self,
        flow: &super::Flow,
        boxes: &mut [BoxArea],
        width: u32,
        y: f32,
    ) -> Placement {
        let origin = boxes[self.spec.owner].outer_x;
        let (left, top) = flow.place(self.spec, self.clear, width, y);
        for area in &mut boxes[self.elements.clone()] {
            area.x = left + (area.x - origin);
            area.outer_x = left + (area.outer_x - origin);
        }
        Placement {
            spec: self.spec,
            left,
            top,
            surrounding_y: y,
        }
    }
}

pub(crate) struct Placement {
    spec: Spec,
    left: u32,
    pub top: f32,
    surrounding_y: f32,
}

impl Placement {
    pub(crate) fn finish(
        self,
        flow: &mut super::Flow,
        bottom: f32,
        viewport: f32,
    ) -> crate::Result<f32> {
        if bottom - self.top > viewport {
            return Err(Box::new(crate::PageContentTooTall));
        }
        flow.record(self.spec, self.left, self.top, bottom)?;
        Ok(self.surrounding_y)
    }
}
