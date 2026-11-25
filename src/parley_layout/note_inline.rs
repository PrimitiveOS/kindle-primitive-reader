//! One layout paragraph may reference several immutable source blocks.
use std::{
    collections::{BTreeMap, HashMap},
    ops::Range,
    sync::Arc,
};

use crate::{
    Line, Source,
    book_style::Style,
    flow_projection::{Affinity, OwnedProjection},
    flow_view::{ChapterFlow, LayoutImage, LayoutRuby},
    footnotes::in_page::Targets,
    source::StyledText,
    table_layout::{Cell, TableKey},
};

const MAX_BLOCKS: usize = 64;
const MAX_BYTES: usize = 32 * 1024;

#[cfg(test)]
mod tests;

pub(super) struct Fragment<'a> {
    pub(super) element: usize,
    pub(super) end_index: usize,
    pub(super) runs: Vec<StyledText>,
    // The box uses the joined paragraph's offset and the original image's owner.
    pub(super) images: Vec<(usize, LayoutImage<'a>)>,
    pub(super) ruby: Vec<LayoutRuby<'a>>,
    pub(super) table: Option<Table>,
    pub(super) layout: Arc<JoinedNote>,
}

pub(super) struct Table {
    pub(super) key: TableKey,
    pub(super) element: usize,
    pub(super) cells: Vec<(usize, Range<usize>)>,
}

pub(crate) struct JoinedNote {
    pub(crate) text: String,
    pub(crate) pieces: Vec<Piece>,
}

pub(crate) struct Piece {
    pub(crate) block: usize,
    pub(crate) joined: Range<usize>,
    projected: Option<Arc<OwnedProjection>>,
}

pub(super) fn plan<'a>(
    source: &'a Source,
    styles: &[Style],
    flow: &'a ChapterFlow,
    targets: &Targets,
    note_tables: &HashMap<TableKey, usize>,
) -> crate::Result<BTreeMap<usize, Fragment<'a>>> {
    let owners: Vec<_> = source
        .blocks
        .iter()
        .map(|block| targets.inline_part(source, block.element))
        .collect();
    let mut fragments = BTreeMap::new();
    let mut start = 0;
    for group in owners.chunk_by(|left, right| left == right) {
        let end = start + group.len();
        if let Some(element) = group[0]
            && group.len() <= MAX_BLOCKS
        {
            let marker = source.blocks[start..end]
                .iter()
                .filter_map(|block| block.list_item)
                .find(|item| item.element == element)
                .and_then(|item| styles[element].marker.label(flow.ordinal(item)?));
            if (group.len() > 1 || marker.is_some())
                && let Some(fragment) = join(
                    source,
                    styles,
                    flow,
                    start..end,
                    element,
                    marker,
                    note_tables,
                )?
            {
                fragments.insert(start, fragment);
            }
        }
        start = end;
    }
    Ok(fragments)
}

fn join<'a>(
    source: &'a Source,
    styles: &[Style],
    flow: &'a ChapterFlow,
    indices: Range<usize>,
    element: usize,
    mut marker: Option<String>,
    note_tables: &HashMap<TableKey, usize>,
) -> crate::Result<Option<Fragment<'a>>> {
    let end_index = indices.end;
    if styles[source.blocks[indices.start].element].flow.suppressed {
        return Ok(None);
    }
    let mut joined = JoinedNote {
        text: String::new(),
        pieces: Vec::new(),
    };
    let mut runs = Vec::new();
    let mut images = Vec::new();
    let mut ruby = Vec::new();
    let mut artwork_blocks = 0;
    let mut inside_marker = false;
    let has_table = source.blocks[indices.clone()]
        .iter()
        .any(|block| block.table_cell.is_some());
    let mut table: Option<Table> = None;
    let mut closed_table = false;
    let mut seen_cells = Vec::<Cell>::new();
    for block in &source.blocks[indices] {
        let style = &styles[block.element];
        let table_root =
            std::iter::successors(Some(block.element), |&index| source.elements[index].parent)
                .find(|&index| source.elements[index].tag == "table");
        if style.preformatted_spaces
            || style.page_breaks != Default::default()
            || style.writing_mode == crate::book_style::WritingMode::VerticalRl
            || crate::publisher_padding::paragraph_is_rtl(block.direction, &block.text)
            || table_root.is_some() && block.table_cell.is_none()
            || has_table
                && (!block.images.is_empty()
                    || !block.ruby.is_empty()
                    || source.elements[block.element].tag == "caption")
        {
            return Ok(None);
        }
        let input = flow.layout_block(source, block)?;
        if style.flow.suppressed || (input.text.is_empty() && input.images.is_empty()) {
            continue;
        }
        if input.styled_text.iter().any(|run| {
            matches!(
                styles[run.element].display,
                crate::book_style::flow::Display::InlineBlock
                    | crate::book_style::flow::Display::InlineTable
            )
        }) {
            return Ok(None);
        }
        // Keep the note container's marker once, at the start of the shaped
        // paragraph. It doesn't own a canonical span or add searchable text.
        if let Some(label) = marker.take()
            && crate::list_marker::authored_prefix(input.text, &label).is_none()
        {
            joined.text.push_str(&label);
            joined.text.push('\u{a0}');
            runs.push(StyledText {
                bytes: 0..joined.text.len(),
                element,
                language: block.language,
            });
            inside_marker = true;
        }
        if joined.text.len() + 1 + input.text.len().max(1) > MAX_BYTES {
            return Ok(None);
        }
        if !joined.text.is_empty()
            && !joined.text.ends_with(char::is_whitespace)
            && !input.text.starts_with(char::is_whitespace)
        {
            joined.text.push(' ');
        }
        let offset = joined.text.len();
        joined.text.push_str(input.text);
        if let Some(cell) = block.table_cell {
            if closed_table
                || !note_tables.contains_key(&cell.table)
                || input.text.is_empty()
                || !matches!(source.elements[block.element].tag.as_str(), "td" | "th")
                || cell.after_row > cell.row + 1
                || seen_cells.len() >= 12
                || seen_cells.contains(&cell)
            {
                return Ok(None);
            }
            if table.is_none() {
                let Some(root) = table_root else {
                    return Ok(None);
                };
                table = Some(Table {
                    key: cell.table,
                    element: root,
                    cells: Vec::new(),
                });
            }
            let group = table.as_mut().unwrap();
            if group.key != cell.table {
                return Ok(None);
            }
            seen_cells.push(cell);
            group.cells.push((block.element, offset..joined.text.len()));
        } else if table.is_some() {
            closed_table = true;
        }
        runs.extend(input.styled_text.iter().map(|run| StyledText {
            bytes: offset + run.bytes.start..offset + run.bytes.end,
            element: run.element,
            language: run.language,
        }));
        images.extend(input.images.iter().map(|mut image| {
            image.offset += offset;
            (block.key, image)
        }));
        ruby.extend(input.ruby.iter().map(|mut annotation| {
            annotation.bytes = offset + annotation.bytes.start..offset + annotation.bytes.end;
            annotation
        }));
        if input.text.is_empty() {
            // Keep the inline box between its neighboring source blocks.
            // The separating space has no canonical text offset.
            joined.text.push(' ');
            artwork_blocks += 1;
        } else {
            joined.pieces.push(Piece {
                block: block.key,
                joined: offset..joined.text.len(),
                projected: flow.shared(block.key),
            });
        }
    }
    if let Some(table) = &table
        && source
            .table_cells
            .iter()
            .filter(|(_, cell)| cell.table == table.key)
            .count()
            != table.cells.len()
    {
        return Ok(None);
    }
    Ok(
        (!joined.pieces.is_empty() && (inside_marker || joined.pieces.len() + artwork_blocks > 1))
            .then_some(Fragment {
                element,
                end_index,
                runs,
                images,
                ruby,
                table,
                layout: Arc::new(joined),
            }),
    )
}

impl JoinedNote {
    pub(crate) fn source_location(
        &self,
        offset: usize,
        affinity: Affinity,
    ) -> Option<(usize, usize)> {
        let piece = match affinity {
            Affinity::Before => self
                .pieces
                .iter()
                .find(|piece| piece.joined.start < offset && offset <= piece.joined.end)
                .or_else(|| {
                    self.pieces
                        .iter()
                        .find(|piece| piece.joined.start == offset)
                }),
            Affinity::After => self
                .pieces
                .iter()
                .find(|piece| piece.joined.contains(&offset))
                .or_else(|| self.pieces.iter().find(|piece| piece.joined.end == offset)),
        }?;
        let offset = offset - piece.joined.start;
        let source = match &piece.projected {
            Some(view) => view.source_offset(offset, affinity)?,
            None => offset,
        };
        Some((piece.block, source))
    }

    pub(super) fn project_line(&self, line: &mut Line, bytes: Range<usize>) {
        self.project_ranges(line, &[bytes]);
    }

    pub(super) fn project_ranges(&self, line: &mut Line, ranges: &[Range<usize>]) {
        let mut spans = Vec::new();
        for piece in &self.pieces {
            for bytes in ranges {
                let start = bytes.start.max(piece.joined.start);
                let end = bytes.end.min(piece.joined.end);
                if start >= end {
                    continue;
                }
                let origin = piece.joined.start;
                let local = start - origin..end - origin;
                if let Some(projected) = &piece.projected {
                    spans.extend(projected.runs().iter().filter_map(|run| {
                        let start = local.start.max(run.layout.start);
                        let end = local.end.min(run.layout.end);
                        (start < end && !run.source.is_empty()).then(|| {
                            let bias = run.source.start;
                            (
                                piece.block,
                                bias + (start - run.layout.start)..bias + (end - run.layout.start),
                            )
                        })
                    }));
                } else {
                    spans.push((piece.block, local));
                }
            }
        }
        line.set_joined_sources(spans);
    }
}
