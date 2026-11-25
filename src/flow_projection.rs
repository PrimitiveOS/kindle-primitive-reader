//! A layout view of canonical reading text. It is deliberately not enabled in
//! renderers yet: every text consumer must adopt the same view before hiding UI
//! or a custom CSS declaration can promise complete display/visibility support.
//!
//! Source extraction records origins without changing any canonical byte, key,
//! revision or legacy-offset map. Display suppression removes origins here;
//! visibility changes only the paint flag and leaves text/metrics available.
#![allow(dead_code)] // Foundation; consumer migration is a separate acceptance gate.

use std::{
    borrow::Cow,
    fmt,
    ops::Range,
    sync::{Arc, Weak},
};

use ring::digest::{SHA256, digest};

use crate::source::Block;

/// At most about 2 MiB of event payload per chapter, including all its blocks.
/// Contiguous text and repeated whitespace from one element are coalesced.
pub(crate) const MAX_ORIGIN_EVENTS: usize = 65_536;
const MAX_OWNER_RECORDS: usize = 100_000;

/// One identity allocation and one bounded event allowance per parsed chapter.
pub(crate) struct Budget {
    remaining: usize,
    owners_remaining: usize,
    chapter: Arc<()>,
}

/// An opaque parsed-chapter lifetime token. This does not own source text and
/// does not retain a chapter after its canonical blocks have been dropped.
#[derive(Clone)]
pub(crate) struct SourceBinding(Weak<()>);

impl SourceBinding {
    pub(crate) fn new(block: &Block) -> Self {
        Self(Arc::downgrade(&block.flow_origins.chapter))
    }

    pub(crate) fn matches(&self, block: &Block) -> bool {
        Weak::ptr_eq(&self.0, &Arc::downgrade(&block.flow_origins.chapter))
    }
}

impl Budget {
    pub(crate) fn new() -> Self {
        Self {
            remaining: MAX_ORIGIN_EVENTS,
            owners_remaining: MAX_OWNER_RECORDS,
            chapter: Arc::new(()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Event {
    TextNode {
        starts_whitespace: bool,
        element: usize,
    },
    Text {
        bytes: Range<usize>,
        element: usize,
    },
    Space {
        at: usize,
        element: usize,
    },
    Break {
        at: usize,
        element: usize,
    },
    Image {
        index: usize,
        element: usize,
    },
}

impl Event {
    fn element(&self) -> usize {
        match *self {
            Self::TextNode { element, .. }
            | Self::Text { element, .. }
            | Self::Space { element, .. }
            | Self::Break { element, .. }
            | Self::Image { element, .. } => element,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Origins {
    events: Vec<Event>,
    complete: bool,
    owners: Vec<usize>,
    owners_complete: bool,
    chapter: Arc<()>,
}

impl Origins {
    pub(crate) fn new(budget: &Budget) -> Self {
        Self {
            events: Vec::new(),
            complete: true,
            owners: Vec::new(),
            owners_complete: true,
            chapter: Arc::clone(&budget.chapter),
        }
    }
    pub(crate) fn complete(&self) -> bool {
        self.complete
    }

    /// The first retained, nonempty original text node, stopping at a hard break.
    /// Keep the original node boundary even when neighboring style runs can merge.
    pub(crate) fn first_text(
        &self,
        mut suppressed: impl FnMut(usize) -> bool,
    ) -> Option<(usize, Range<usize>)> {
        let mut first: Option<(usize, Range<usize>)> = None;
        for event in &self.events {
            if suppressed(event.element()) {
                continue;
            }
            match event {
                Event::TextNode { .. } if first.is_some() => break,
                Event::Break { .. } => break,
                Event::Text { bytes, element } => {
                    if let Some((_, range)) = &mut first {
                        range.end = bytes.end;
                    } else {
                        first = Some((*element, bytes.clone()));
                    }
                }
                _ => (),
            }
        }
        first
    }

    fn push(&mut self, event: Event, budget: &mut Budget) {
        if !self.complete {
            return;
        }
        // Internal whitespace within one raw text node has the same owner on
        // both sides, so its normalized byte belongs in the same Text range.
        // TextNode/BR/image boundaries prevent crossing a different source
        // context. Leading/trailing pending spaces remain distinct events.
        if let Event::Text {
            bytes: next,
            element: owner,
        } = &event
            && self.events.len() >= 2
        {
            let count = self.events.len();
            let merge = matches!((&self.events[count-2], &self.events[count-1]),
                (Event::Text { bytes, element }, Event::Space { at, element: space_owner })
                    if element == owner && space_owner == owner && bytes.end == *at && *at + 1 == next.start);
            if merge {
                self.events.pop();
                if let Some(Event::Text { bytes, .. }) = self.events.last_mut() {
                    bytes.end = next.end;
                }
                budget.remaining += 1;
                return;
            }
        }
        match (self.events.last_mut(), &event) {
            (
                Some(Event::Text { bytes, element }),
                Event::Text {
                    bytes: next,
                    element: owner,
                },
            ) if element == owner && bytes.end == next.start => {
                bytes.end = next.end;
                return;
            }
            (
                Some(Event::Space { at, element }),
                Event::Space {
                    at: next,
                    element: owner,
                },
            ) if at == next && element == owner => return,
            _ => {}
        }
        if budget.remaining == 0 {
            // Normal extraction stays usable. A future styled candidate must
            // reject this explicit incomplete state rather than guess origins.
            self.complete = false;
            self.events.clear();
            return;
        }
        budget.remaining -= 1;
        self.events.push(event);
    }

    pub(crate) fn character(
        &mut self,
        c: char,
        element: usize,
        before: usize,
        after: usize,
        budget: &mut Budget,
    ) {
        if matches!(c, ' ' | '\t' | '\r' | '\n' | '\u{c}') {
            self.push(
                Event::Space {
                    at: before,
                    element,
                },
                budget,
            );
        } else {
            self.push(
                Event::Text {
                    bytes: after - c.len_utf8()..after,
                    element,
                },
                budget,
            );
        }
    }

    pub(crate) fn hard_break(&mut self, at: usize, element: usize, budget: &mut Budget) {
        self.owner(element, budget);
        self.push(Event::Break { at, element }, budget);
    }

    pub(crate) fn text_node(
        &mut self,
        starts_whitespace: bool,
        element: usize,
        budget: &mut Budget,
    ) {
        self.owner(element, budget);
        self.push(
            Event::TextNode {
                starts_whitespace,
                element,
            },
            budget,
        );
    }

    pub(crate) fn image(&mut self, index: usize, element: usize, budget: &mut Budget) {
        self.owner(element, budget);
        self.push(Event::Image { index, element }, budget);
    }

    fn owner(&mut self, element: usize, budget: &mut Budget) {
        if !self.owners_complete || self.owners.last() == Some(&element) {
            return;
        }
        if budget.owners_remaining == 0 {
            self.owners_complete = false;
            self.owners.clear();
            return;
        }
        budget.owners_remaining -= 1;
        self.owners.push(element);
    }
}

/// Both fields are effective computed values. Ancestor display:none always
/// suppresses descendants; a child's computed visibility may become visible.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ElementFlow {
    pub suppressed: bool,
    pub painted: bool,
}

impl Default for ElementFlow {
    fn default() -> Self {
        Self {
            suppressed: false,
            painted: true,
        }
    }
}

/// Which side owns a caret at the join left by removed source content.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Affinity {
    Before,
    After,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Run {
    /// Empty only for a projected space whose raw whitespace was not emitted
    /// into canonical text (for example, a space before a now-hidden <br/>).
    pub source: Range<usize>,
    pub layout: Range<usize>,
    pub element: usize,
    pub painted: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Image {
    pub source_index: usize,
    pub offset: usize,
    pub space_after: bool,
    pub painted: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ProjectionError {
    IncompleteOrigins,
    MissingElement(usize),
    InvalidOrigin,
    WrongBlock,
}

impl fmt::Display for ProjectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IncompleteOrigins => {
                f.write_str("The chapter exceeds the styled-flow origin limit")
            }
            Self::MissingElement(index) => {
                write!(f, "The styled-flow element {index} is unavailable")
            }
            Self::InvalidOrigin => f.write_str("The styled-flow source origin is invalid"),
            Self::WrongBlock => {
                f.write_str("The styled-flow payload belongs to another source block")
            }
        }
    }
}

impl std::error::Error for ProjectionError {}

#[derive(Clone)]
pub(crate) struct OwnedProjection {
    text: String,
    runs: Vec<Run>,
    images: Vec<Image>,
    chapter: Weak<()>,
    source_key: usize,
    source_len: usize,
    source_hash: [u8; 32],
}

/// The identity view borrows its block and allocates nothing. Consumers should
/// retain their existing fast path when `is_identity()` is true. A non-identity
/// view owns only bounded projected text/runs/image positions, never Source.
pub(crate) struct Projection<'a> {
    block: &'a Block,
    data: Option<Cow<'a, OwnedProjection>>,
}

impl<'a> Projection<'a> {
    pub(crate) fn identity(block: &'a Block) -> Self {
        Self { block, data: None }
    }

    pub(crate) fn new(block: &'a Block, flows: &[ElementFlow]) -> Result<Self, ProjectionError> {
        if flows.is_empty() {
            return Ok(Self::identity(block));
        }
        let block_flow = flows
            .get(block.element)
            .ok_or(ProjectionError::MissingElement(block.element))?;
        if block_flow.suppressed {
            return Ok(Self {
                block,
                data: Some(Cow::Owned(OwnedProjection::empty(block))),
            });
        }
        if !block.flow_origins.owners_complete {
            if flows.iter().all(|flow| *flow == ElementFlow::default()) {
                return Ok(Self::identity(block));
            }
            return Err(ProjectionError::IncompleteOrigins);
        }
        // A chapter can contain many unaffected blocks under one small hidden
        // subtree. Inspect local origins once instead of rescanning the entire
        // element mask vector for every paragraph (quadratic in DOM size).
        let mut changed = false;
        for &element in &block.flow_origins.owners {
            let flow = flows
                .get(element)
                .ok_or(ProjectionError::MissingElement(element))?;
            changed |= *flow != ElementFlow::default();
        }
        if !changed {
            return Ok(Self::identity(block));
        }
        if !block.flow_origins.complete {
            return Err(ProjectionError::IncompleteOrigins);
        }
        let mut data = OwnedProjection::empty(block);
        let mut pending = None;
        // Image positions after pending whitespace are finalized only when the
        // next text/break decides whether that whitespace becomes a real space.
        let mut unresolved: Vec<(usize, bool)> = Vec::new();
        let mut last_image: Option<usize> = None;
        for event in &block.flow_origins.events {
            let element = event.element();
            let flow = *flows
                .get(element)
                .ok_or(ProjectionError::MissingElement(element))?;
            if flow.suppressed {
                continue;
            }
            match event {
                Event::TextNode {
                    starts_whitespace, ..
                } => {
                    // Source consumes image_after_space at a text-node boundary,
                    // including whitespace-only nodes and Unicode whitespace.
                    // Only the most recent image is eligible for that extra gap.
                    if let Some(index) = last_image.take() {
                        data.images[index].space_after = *starts_whitespace;
                    }
                }
                Event::Space { at, .. } => {
                    if !block.text.is_char_boundary(*at) {
                        return Err(ProjectionError::InvalidOrigin);
                    }
                    pending.get_or_insert((*at, element, flow.painted));
                }
                Event::Text { bytes, .. } => {
                    let text = block
                        .text
                        .get(bytes.clone())
                        .ok_or(ProjectionError::InvalidOrigin)?;
                    if text.is_empty() {
                        return Err(ProjectionError::InvalidOrigin);
                    }
                    let space = pending
                        .take()
                        .filter(|_| !data.text.is_empty() && !data.text.ends_with('\n'));
                    if let Some((at, owner, painted)) = space {
                        let source = if block.text.as_bytes().get(at) == Some(&b' ') {
                            at..at + 1
                        } else {
                            at..at
                        };
                        data.push(" ", source, owner, painted);
                    }
                    for (index, before) in unresolved.drain(..) {
                        let image = &mut data.images[index];
                        image.offset += usize::from(before && space.is_some());
                    }
                    data.push(text, bytes.clone(), element, flow.painted);
                }
                Event::Break { at, .. } => {
                    if block.text.as_bytes().get(*at) != Some(&b'\n') {
                        return Err(ProjectionError::InvalidOrigin);
                    }
                    pending = None;
                    unresolved.clear();
                    last_image = None;
                    data.push("\n", *at..at + 1, element, flow.painted);
                }
                Event::Image { index, .. } => {
                    if block.images.get(*index).is_none() {
                        return Err(ProjectionError::InvalidOrigin);
                    }
                    let before =
                        pending.is_some() && !data.text.is_empty() && !data.text.ends_with('\n');
                    unresolved.push((data.images.len(), before));
                    last_image = before.then_some(data.images.len());
                    data.images.push(Image {
                        source_index: *index,
                        offset: data.text.len(),
                        space_after: false,
                        painted: flow.painted,
                    });
                }
            }
        }
        Ok(Self {
            block,
            data: Some(Cow::Owned(data)),
        })
    }

    pub(crate) fn is_identity(&self) -> bool {
        self.data.is_none()
    }
    /// Transfer to an owning renderer without borrowing Source or copying its
    /// canonical text. Identity has no payload. An already-borrowed payload
    /// clones only if the caller explicitly requests another owner.
    pub(crate) fn into_owned(self) -> Option<OwnedProjection> {
        self.data.map(Cow::into_owned)
    }
    pub(crate) fn text(&self) -> &str {
        self.data
            .as_ref()
            .map_or(&self.block.text, |data| &data.text)
    }
    /// None means use canonical style runs/illustrations directly, with no copy.
    pub(crate) fn runs(&self) -> Option<&[Run]> {
        self.data.as_ref().map(|data| data.runs.as_slice())
    }
    pub(crate) fn images(&self) -> Option<&[Image]> {
        self.data.as_ref().map(|data| data.images.as_slice())
    }

    pub(crate) fn source_is_present(&self, offset: usize) -> bool {
        if !self.block.text.is_char_boundary(offset) || offset == self.block.text.len() {
            return false;
        }
        self.data
            .as_ref()
            .is_none_or(|data| data.runs.iter().any(|run| run.source.contains(&offset)))
    }

    /// Exact mapping for retained source characters/caret boundaries. An anchor
    /// *inside* removed content has no result; explicit jumps must additionally
    /// check source_is_present when targeting a source character.
    pub(crate) fn project_offset(&self, offset: usize, affinity: Affinity) -> Option<usize> {
        if !self.block.text.is_char_boundary(offset) {
            return None;
        }
        let Some(data) = &self.data else {
            return Some(offset);
        };
        if let Some(space) = data
            .runs
            .iter()
            .find(|run| run.source.is_empty() && run.source.start == offset)
        {
            return Some(match affinity {
                Affinity::Before => space.layout.start,
                Affinity::After => space.layout.end,
            });
        }
        let mut left = None;
        let mut right = None;
        for run in &data.runs {
            if run.source.start < offset && offset < run.source.end {
                return Some(run.layout.start + offset - run.source.start);
            }
            if run.source.end == offset {
                left = Some(run.layout.end);
            }
            if run.source.start == offset && right.is_none() {
                right = Some(run.layout.start);
            }
        }
        match affinity {
            Affinity::Before => left.or(right),
            Affinity::After => right.or(left),
        }
    }

    /// Explicit snapping is separate from exact target resolution. This is for
    /// restore/caret fallback, never permission to reveal a suppressed target.
    pub(crate) fn project_nearest(&self, offset: usize, affinity: Affinity) -> Option<usize> {
        if !self.block.text.is_char_boundary(offset) {
            return None;
        }
        if let Some(exact) = self.project_offset(offset, affinity) {
            return Some(exact);
        }
        let data = self.data.as_ref()?;
        Some(match affinity {
            Affinity::Before => data
                .runs
                .iter()
                .rev()
                .find(|run| run.source.end < offset)
                .map_or(0, |run| run.layout.end),
            Affinity::After => data
                .runs
                .iter()
                .find(|run| run.source.start > offset)
                .map_or(data.text.len(), |run| run.layout.start),
        })
    }

    pub(crate) fn source_offset(&self, offset: usize, affinity: Affinity) -> Option<usize> {
        match &self.data {
            Some(data) => data.source_offset(offset, affinity),
            None => self.block.text.is_char_boundary(offset).then_some(offset),
        }
    }

    /// Preserve separate spans across suppression gaps. A consumer must not
    /// replace these with the envelope from the first start to the last end.
    pub(crate) fn source_spans(&self, bytes: Range<usize>) -> Option<Vec<Range<usize>>> {
        if bytes.start > bytes.end
            || !self.text().is_char_boundary(bytes.start)
            || !self.text().is_char_boundary(bytes.end)
        {
            return None;
        }
        let Some(data) = &self.data else {
            return Some((!bytes.is_empty()).then_some(bytes).into_iter().collect());
        };
        Some(
            data.runs
                .iter()
                .filter_map(|run| {
                    let start = bytes.start.max(run.layout.start);
                    let end = bytes.end.min(run.layout.end);
                    (start < end && !run.source.is_empty()).then(|| {
                        run.source.start + start - run.layout.start
                            ..run.source.start + end - run.layout.start
                    })
                })
                .collect(),
        )
    }

    pub(crate) fn layout_spans(&self, bytes: Range<usize>) -> Option<Vec<Range<usize>>> {
        if bytes.start > bytes.end
            || !self.block.text.is_char_boundary(bytes.start)
            || !self.block.text.is_char_boundary(bytes.end)
        {
            return None;
        }
        let Some(data) = &self.data else {
            return Some((!bytes.is_empty()).then_some(bytes).into_iter().collect());
        };
        Some(
            data.runs
                .iter()
                .filter_map(|run| {
                    // A re-collapsed space between selected canonical content
                    // has no source byte of its own, but still belongs to the
                    // selection's layout geometry. Endpoint-only contact does
                    // not claim it (start/end affinity decides that separately).
                    if run.source.is_empty() {
                        return (bytes.start < run.source.start && run.source.start < bytes.end)
                            .then(|| run.layout.clone());
                    }
                    let start = bytes.start.max(run.source.start);
                    let end = bytes.end.min(run.source.end);
                    (start < end).then(|| {
                        run.layout.start + start - run.source.start
                            ..run.layout.start + end - run.source.start
                    })
                })
                .collect(),
        )
    }
}

impl OwnedProjection {
    fn empty(block: &Block) -> Self {
        Self {
            text: String::new(),
            runs: Vec::new(),
            images: Vec::new(),
            chapter: Arc::downgrade(&block.flow_origins.chapter),
            source_key: block.key,
            source_len: block.text.len(),
            source_hash: digest(&SHA256, block.text.as_bytes())
                .as_ref()
                .try_into()
                .unwrap(),
        }
    }
    /// Rebind only to the same immutable parsed chapter/block. A reparse (even
    /// byte-identical), book switch or source revision change needs a fresh
    /// payload. The Weak identity neither keeps Source alive nor can be reused
    /// while a stale payload exists. SHA-256 rejects in-place text mutation.
    pub(crate) fn bind<'a>(&'a self, block: &'a Block) -> Result<Projection<'a>, ProjectionError> {
        if !Weak::ptr_eq(&self.chapter, &Arc::downgrade(&block.flow_origins.chapter))
            || self.source_key != block.key
            || self.source_len != block.text.len()
            || digest(&SHA256, block.text.as_bytes()).as_ref() != self.source_hash
        {
            return Err(ProjectionError::WrongBlock);
        }
        Ok(Projection {
            block,
            data: Some(Cow::Borrowed(self)),
        })
    }

    pub(crate) fn text(&self) -> &str {
        &self.text
    }
    pub(crate) fn runs(&self) -> &[Run] {
        &self.runs
    }
    pub(crate) fn images(&self) -> &[Image] {
        &self.images
    }

    pub(crate) fn source_offset(&self, offset: usize, affinity: Affinity) -> Option<usize> {
        if !self.text.is_char_boundary(offset) {
            return None;
        }
        let mut left = None;
        let mut right = None;
        for run in &self.runs {
            if run.layout.start < offset && offset < run.layout.end {
                return Some(run.source.start + offset - run.layout.start);
            }
            if run.layout.end == offset {
                left = Some(run.source.end);
            }
            if run.layout.start == offset && right.is_none() {
                right = Some(run.source.start);
            }
        }
        match affinity {
            Affinity::Before => left.or(right),
            Affinity::After => right.or(left),
        }
    }

    fn push(&mut self, text: &str, source: Range<usize>, element: usize, painted: bool) {
        let start = self.text.len();
        self.text.push_str(text);
        if let Some(last) = self.runs.last_mut()
            && last.element == element
            && last.painted == painted
            && !last.source.is_empty()
            && !source.is_empty()
            && last.source.end == source.start
        {
            last.source.end = source.end;
            last.layout.end = self.text.len();
        } else {
            self.runs.push(Run {
                source,
                layout: start..self.text.len(),
                element,
                painted,
            });
        }
    }
}

#[cfg(test)]
mod tests;
