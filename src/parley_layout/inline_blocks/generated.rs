//! Compose generated words into shaped text without giving them source offsets.
use super::{InlineText, Segment, slice_runs};
use crate::{
    book_style::{
        Style,
        pseudo::{Edge, Run, Side},
    },
    flow_projection::{Affinity, Projection},
    flow_view::LayoutBlock,
    source::{Source, StyledText},
};
use std::ops::Range;

pub(crate) struct Plan<'a> {
    pub(crate) text: InlineText,
    pub(crate) runs: Vec<StyledText>,
    pub(crate) typography: Vec<Run<'a>>,
    artwork_edges: Vec<ArtworkEdge>,
}

struct ArtworkEdge {
    at: usize,
    end: usize,
    host: usize,
    side: Side,
}

impl<'a> Plan<'a> {
    pub(crate) fn new(
        block: &LayoutBlock<'_>,
        view: &Projection<'_>,
        styles: &'a [Style],
    ) -> Option<Self> {
        Self::new_for_hosts(block, view, styles, &[block.element])
    }

    pub(crate) fn new_for_hosts(
        block: &LayoutBlock<'_>,
        view: &Projection<'_>,
        styles: &'a [Style],
        hosts: &[usize],
    ) -> Option<Self> {
        Self::new_with_edges(block, view, styles, hosts, hosts)
    }

    pub(crate) fn new_with_edges(
        block: &LayoutBlock<'_>,
        view: &Projection<'_>,
        styles: &'a [Style],
        before_hosts: &[usize],
        after_hosts: &[usize],
    ) -> Option<Self> {
        if !block.images.is_empty()
            || !block.ruby.is_empty()
            || styles[block.element].preformatted_spaces
        {
            return None;
        }
        Self::compose_with_edges(block, view, styles, before_hosts, after_hosts)
            .ok()
            .flatten()
    }

    pub(in crate::parley_layout) fn compose(
        block: &LayoutBlock<'_>,
        view: &Projection<'_>,
        styles: &'a [Style],
    ) -> Result<Option<Self>, ()> {
        Self::compose_for_hosts(block, view, styles, &[block.element])
    }

    pub(in crate::parley_layout) fn compose_for_hosts(
        block: &LayoutBlock<'_>,
        view: &Projection<'_>,
        styles: &'a [Style],
        hosts: &[usize],
    ) -> Result<Option<Self>, ()> {
        Self::compose_with_edges(block, view, styles, hosts, hosts)
    }

    pub(in crate::parley_layout) fn compose_with_edges(
        block: &LayoutBlock<'_>,
        view: &Projection<'_>,
        styles: &'a [Style],
        before_hosts: &[usize],
        after_hosts: &[usize],
    ) -> Result<Option<Self>, ()> {
        if before_hosts.is_empty() && after_hosts.is_empty() && block.pseudo_edges.is_empty() {
            return Err(());
        }
        let end = std::ops::Deref::deref(block).text.len();
        let mut edges = Vec::with_capacity(
            before_hosts.len() + after_hosts.len() + block.pseudo_edges.len(),
        );
        edges.extend(before_hosts.iter().map(|&host| Edge {
            at: 0,
            host,
            side: Side::Before,
        }));
        edges.extend(block.pseudo_edges.iter().map(|edge| Edge {
            at: edge.at,
            host: edge.host,
            side: edge.side,
        }));
        edges.extend(after_hosts.iter().rev().map(|&host| Edge {
            at: end,
            host,
            side: Side::After,
        }));
        let mut result = None;
        let mut cursor = 0;
        for edge in &edges {
            if styles[edge.host].flow.suppressed {
                continue;
            }
            let part = styles[edge.host]
                .generated
                .as_ref()
                .and_then(|pair| pair[edge.side as usize].as_ref());
            let Some(part) = part else { continue };
            let at = view.project_nearest(edge.at, Affinity::After).ok_or(())?;
            if at < cursor || !block.text.is_char_boundary(at) {
                return Err(());
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
                artwork_edges: Vec::new(),
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
                    host: Some(edge.host),
                });
                if !block.images.is_empty() {
                    result.artwork_edges.push(ArtworkEdge {
                        at,
                        end,
                        host: edge.host,
                        side: edge.side,
                    });
                }
            }
        }
        let Some(mut result) = result.filter(|plan| !plan.typography.is_empty()) else {
            return Ok(None);
        };
        result.append_source(block, cursor..block.text.len());
        Ok(Some(result))
    }

    pub(in crate::parley_layout) fn source_parts(
        &self,
    ) -> impl Iterator<Item = (Range<usize>, Range<usize>)> + '_ {
        self.text
            .segments
            .iter()
            .map(|segment| (segment.source.clone(), segment.layout()))
    }

    pub(in crate::parley_layout) fn artwork_offset(
        &self,
        source: &Source,
        element: usize,
        visible_offset: usize,
    ) -> usize {
        let mut offset = 0;
        for segment in &self.text.segments {
            if visible_offset <= segment.source.start {
                break;
            }
            offset = segment.offset + visible_offset.min(segment.source.end) - segment.source.start;
        }
        let first = self
            .artwork_edges
            .partition_point(|edge| edge.at < visible_offset);
        for edge in &self.artwork_edges[first..] {
            if edge.at != visible_offset {
                break;
            }
            let precedes = match edge.side {
                Side::Before => edge.host <= element,
                Side::After => {
                    edge.host < element
                        && !std::iter::successors(Some(element), |&index| {
                            source.elements[index].parent
                        })
                        .any(|index| index == edge.host)
                }
            };
            if !precedes {
                break;
            }
            offset = edge.end;
        }
        offset
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
