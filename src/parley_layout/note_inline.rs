//! One layout paragraph may reference several immutable source blocks.
use std::{collections::BTreeMap, ops::Range, sync::Arc};

use crate::{
    Line, Source,
    book_style::Style,
    flow_projection::{Affinity, OwnedProjection},
    flow_view::ChapterFlow,
    footnotes::in_page::Targets,
    source::StyledText,
};

const MAX_BLOCKS: usize = 64;
const MAX_BYTES: usize = 32 * 1024;

pub(super) struct Fragment {
    pub(super) element: usize,
    pub(super) end_index: usize,
    pub(super) runs: Vec<StyledText>,
    pub(super) layout: Arc<JoinedNote>,
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

pub(super) fn plan(
    source: &Source,
    styles: &[Style],
    flow: &ChapterFlow,
    targets: &Targets,
) -> crate::Result<BTreeMap<usize, Fragment>> {
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
            && (2..=MAX_BLOCKS).contains(&group.len())
            && let Some(fragment) = join(source, styles, flow, start..end, element)?
        {
            fragments.insert(start, fragment);
        }
        start = end;
    }
    Ok(fragments)
}

fn join(
    source: &Source,
    styles: &[Style],
    flow: &ChapterFlow,
    indices: Range<usize>,
    element: usize,
) -> crate::Result<Option<Fragment>> {
    let end_index = indices.end;
    if styles[source.blocks[indices.start].element].flow.suppressed {
        return Ok(None);
    }
    let mut joined = JoinedNote {
        text: String::new(),
        pieces: Vec::new(),
    };
    let mut runs = Vec::new();
    for block in &source.blocks[indices] {
        let style = &styles[block.element];
        // A direct, single-block item becomes inline text without its generated
        // list marker. Keep nested lists and multi-block items in ordinary flow.
        let complex_list = block.list_item.is_some_and(|item| {
            item.parent_group.is_some() || !item.first || item.element == element
        });
        if block.table_cell.is_some()
            || complex_list
            || !block.images.is_empty()
            || !block.ruby.is_empty()
            || style.preformatted_spaces
            || style.page_breaks != Default::default()
            || style.writing_mode == crate::book_style::WritingMode::VerticalRl
            || crate::publisher_padding::paragraph_is_rtl(block.direction, &block.text)
        {
            return Ok(None);
        }
        let input = flow.layout_block(source, block)?;
        if style.flow.suppressed || input.text.is_empty() {
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
        if joined.text.len() + 1 + input.text.len() > MAX_BYTES {
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
        runs.extend(input.styled_text.iter().map(|run| StyledText {
            bytes: offset + run.bytes.start..offset + run.bytes.end,
            element: run.element,
            language: run.language,
        }));
        joined.pieces.push(Piece {
            block: block.key,
            joined: offset..joined.text.len(),
            projected: flow.shared(block.key),
        });
    }
    Ok((joined.pieces.len() > 1).then_some(Fragment {
        element,
        end_index,
        runs,
        layout: Arc::new(joined),
    }))
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
        let mut spans = Vec::new();
        for piece in &self.pieces {
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
        line.set_joined_sources(spans);
    }
}
