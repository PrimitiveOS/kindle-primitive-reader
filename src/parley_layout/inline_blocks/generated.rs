//! Compose generated words into shaped text without giving them source offsets.
use super::{InlineText, Segment, slice_runs};
use crate::{
    book_style::{
        Style,
        pseudo::{Edge, Run, Side},
    },
    flow_projection::{Affinity, Projection},
    flow_view::LayoutBlock,
    source::StyledText,
};
use std::ops::Range;

pub(crate) struct Plan<'a> {
    pub(crate) text: InlineText,
    pub(crate) runs: Vec<StyledText>,
    pub(crate) typography: Vec<Run<'a>>,
}

impl<'a> Plan<'a> {
    pub(crate) fn new(
        block: &LayoutBlock<'_>,
        view: &Projection<'_>,
        styles: &'a [Style],
    ) -> Option<Self> {
        if block.text.is_empty()
            || !block.images.is_empty()
            || !block.ruby.is_empty()
            || styles[block.element].preformatted_spaces
        {
            return None;
        }
        let before = Edge {
            at: 0,
            host: block.element,
            side: Side::Before,
        };
        let after = Edge {
            at: std::ops::Deref::deref(block).text.len(),
            host: block.element,
            side: Side::After,
        };
        let mut result = None;
        let mut cursor = 0;
        for edge in std::iter::once(&before)
            .chain(block.pseudo_edges.iter())
            .chain(std::iter::once(&after))
        {
            let part = styles[edge.host]
                .generated
                .as_ref()
                .and_then(|pair| pair[edge.side as usize].as_ref());
            let Some(part) = part else { continue };
            let at = view.project_nearest(edge.at, Affinity::After)?;
            if at < cursor || !block.text.is_char_boundary(at) {
                return None;
            }
            let result = result.get_or_insert_with(|| Self {
                text: InlineText {
                    text: String::new(),
                    segments: Vec::new(),
                    boxes: Vec::new(),
                    generated: true,
                    detached_initial: false,
                },
                runs: Vec::new(),
                typography: Vec::new(),
            });
            result.append_source(block, cursor..at);
            cursor = at;
            let start = result.text.text.len();
            append_collapsed(&mut result.text.text, &part.text, &block.text[at..]);
            let end = result.text.text.len();
            if end > start {
                result.typography.push(Run {
                    bytes: start..end,
                    style: &part.style,
                });
            }
        }
        let mut result = result?;
        if result.typography.is_empty() {
            return None;
        }
        result.append_source(block, cursor..block.text.len());
        Some(result)
    }

    fn append_source(&mut self, block: &LayoutBlock<'_>, source: Range<usize>) {
        if source.is_empty() {
            return;
        }
        let offset = self.text.text.len();
        self.runs.extend(slice_runs(block, &source, offset));
        self.text.text.push_str(&block.text[source.clone()]);
        self.text.segments.push(Segment { source, offset });
    }
}

fn append_collapsed(text: &mut String, generated: &str, following: &str) {
    let whitespace = |ch: char| ch.is_ascii_whitespace();
    let mut space = false;
    for ch in generated.chars() {
        if ch.is_ascii_whitespace() {
            space = true;
        } else {
            if space && !text.is_empty() && !text.ends_with(whitespace) {
                text.push(' ');
            }
            text.push(ch);
            space = false;
        }
    }
    if space
        && !text.is_empty()
        && !text.ends_with(whitespace)
        && !following.starts_with(whitespace)
    {
        text.push(' ');
    }
}
