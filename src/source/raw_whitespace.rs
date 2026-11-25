//! Bounded, source-verified raw text for a future preformatted layout view.
//!
//! This metadata does not change canonical text or anchors. Whitespace gaps
//! deliberately remain one-to-many; callers must not guess an interior anchor.
#![allow(dead_code)] // Projection and raw-position consumers are staged separately.

use std::ops::Range;

use ring::digest::{SHA256, digest};
use roxmltree::Node;
use serde::{Deserialize, Serialize};

use crate::flow_projection::Affinity;

const MAX_BLOCK_BYTES: usize = 64 * 1024;
const MAX_RECORDS: usize = 4096;
const PREFERRED_BUDGET: usize = 768 * 1024;
const GENERAL_BUDGET: usize = 256 * 1024;

pub(super) struct Budget {
    preferred: usize,
    general: usize,
}

impl Budget {
    pub(super) fn new() -> Self {
        Self {
            preferred: PREFERRED_BUDGET,
            general: GENERAL_BUDGET,
        }
    }

    fn claim(&mut self, bytes: usize, preferred: bool) -> bool {
        if preferred && bytes <= self.preferred {
            self.preferred -= bytes;
            true
        } else if bytes <= self.general {
            self.general -= bytes;
            true
        } else {
            false
        }
    }

    fn may_fit(&self, bytes: usize, preferred: bool) -> bool {
        (preferred && bytes <= self.preferred) || bytes <= self.general
    }
}

pub(crate) struct TextSpan {
    pub(crate) canonical: Range<usize>,
    pub(crate) raw: Range<usize>,
}

pub(crate) struct WhitespaceGap {
    /// Empty for whitespace omitted at the start or end of a block.
    pub(crate) canonical: Range<usize>,
    pub(crate) raw: Range<usize>,
}

pub(crate) struct Snapshot {
    block_key: usize,
    raw: String,
    text: Vec<TextSpan>,
    gaps: Vec<WhitespaceGap>,
    canonical_sha256: [u8; 32],
    raw_sha256: [u8; 32],
}

/// An offset inside collapsed raw whitespace must remain explicit; choosing
/// either surrounding canonical boundary would lose its precise position.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RawBoundary {
    Exact(usize),
    Gap {
        canonical: Range<usize>,
        raw: Range<usize>,
        offset: usize,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CaretAffinity {
    Before,
    After,
}

/// Not yet attached to persisted anchors. Future consumers must also verify
/// the enclosing book, chapter, revision and block identity.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawCaret {
    pub(crate) raw_offset: usize,
    pub(crate) canonical_offset: usize,
    pub(crate) raw_sha256: [u8; 32],
    pub(crate) affinity: CaretAffinity,
}

/// A proposed v2 anchor envelope. It is not connected to persisted state or
/// rendering; v1 anchors remain unchanged while consumers are being audited.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawAnchor {
    pub(crate) canonical: crate::Anchor,
    pub(crate) caret: RawCaret,
}

/// Must be supplied from the currently open, verified publication and block.
pub(crate) struct AnchorContext<'a> {
    pub(crate) book: u64,
    pub(crate) chapter: &'a str,
    pub(crate) revision: u64,
    pub(crate) block: usize,
}

pub(crate) struct Bound<'a> {
    snapshot: &'a Snapshot,
    canonical: &'a str,
}

impl Snapshot {
    /// Verify the exact canonical block before exposing its raw projection.
    pub(crate) fn bind<'a>(&'a self, canonical: &'a str) -> Option<Bound<'a>> {
        (digest(&SHA256, canonical.as_bytes()).as_ref() == self.canonical_sha256.as_slice())
            .then_some(Bound {
                snapshot: self,
                canonical,
            })
    }
}

impl Bound<'_> {
    pub(crate) fn raw(&self) -> &str {
        &self.snapshot.raw
    }

    pub(crate) fn text_spans(&self) -> &[TextSpan] {
        &self.snapshot.text
    }

    pub(crate) fn gaps(&self) -> &[WhitespaceGap] {
        &self.snapshot.gaps
    }

    /// Build an explicit raw caret without losing its position in a gap.
    pub(crate) fn caret_at(&self, raw_offset: usize, affinity: CaretAffinity) -> Option<RawCaret> {
        if !self.is_grapheme_boundary(raw_offset) {
            return None;
        }
        self.caret_at_valid_boundary(raw_offset, affinity)
    }

    fn caret_at_valid_boundary(
        &self,
        raw_offset: usize,
        affinity: CaretAffinity,
    ) -> Option<RawCaret> {
        let canonical_offset = match self.raw_to_canonical(raw_offset)? {
            RawBoundary::Exact(offset) => offset,
            RawBoundary::Gap { canonical, .. } => match affinity {
                CaretAffinity::Before => canonical.start,
                CaretAffinity::After => canonical.end,
            },
        };
        Some(RawCaret {
            raw_offset,
            canonical_offset,
            raw_sha256: self.snapshot.raw_sha256,
            affinity,
        })
    }

    /// Resolve a bounded batch with one grapheme scan, for layout lines.
    pub(crate) fn carets_at(&self, requested: &[(usize, CaretAffinity)]) -> Option<Vec<RawCaret>> {
        if requested.len() > 8192 {
            return None;
        }
        let boundaries: Vec<_> = icu_segmenter::GraphemeClusterSegmenter::new()
            .segment_str(&self.snapshot.raw)
            .collect();
        requested
            .iter()
            .map(|&(offset, affinity)| {
                if boundaries.binary_search(&offset).is_err() {
                    return None;
                }
                self.caret_at_valid_boundary(offset, affinity)
            })
            .collect()
    }

    /// Verify the raw fingerprint and canonical relationship before use.
    pub(crate) fn validate_caret(&self, caret: &RawCaret) -> Option<RawBoundary> {
        if caret.raw_sha256 != self.snapshot.raw_sha256
            || !self.is_grapheme_boundary(caret.raw_offset)
        {
            return None;
        }
        let boundary = self.raw_to_canonical(caret.raw_offset)?;
        let canonical = match &boundary {
            RawBoundary::Exact(offset) => *offset,
            RawBoundary::Gap { canonical, .. } => match caret.affinity {
                CaretAffinity::Before => canonical.start,
                CaretAffinity::After => canonical.end,
            },
        };
        (caret.canonical_offset == canonical).then_some(boundary)
    }

    fn is_grapheme_boundary(&self, offset: usize) -> bool {
        self.snapshot.raw.is_char_boundary(offset)
            && icu_segmenter::GraphemeClusterSegmenter::new()
                .segment_str(&self.snapshot.raw)
                .any(|boundary| boundary == offset)
    }

    /// Attach a verified raw caret to a matching canonical anchor.
    pub(crate) fn make_raw_anchor(
        &self,
        canonical: crate::Anchor,
        raw_offset: usize,
        affinity: CaretAffinity,
        context: &AnchorContext<'_>,
    ) -> Option<RawAnchor> {
        let anchor = RawAnchor {
            canonical,
            caret: self.caret_at(raw_offset, affinity)?,
        };
        self.validate_raw_anchor(&anchor, context)?;
        Some(anchor)
    }

    /// Reject mismatched publication identity, stale raw content and
    /// unsupported illustration anchors before any future use.
    pub(crate) fn validate_raw_anchor(
        &self,
        anchor: &RawAnchor,
        context: &AnchorContext<'_>,
    ) -> Option<RawBoundary> {
        let canonical = &anchor.canonical;
        if canonical.book != context.book
            || canonical.chapter != context.chapter
            || canonical.revision != context.revision
            || canonical.block != context.block
            || context.block != self.snapshot.block_key
            || canonical.illustration.is_some()
            || canonical.raw_caret.is_some()
            || canonical.offset != anchor.caret.canonical_offset
            || canonical.chapter.is_empty()
            || canonical.chapter.len() > 4096
            || canonical.chapter.chars().any(char::is_control)
        {
            return None;
        }
        self.validate_caret(&anchor.caret)
    }

    /// Project a canonical caret to an edge of any collapsed raw gap.
    pub(crate) fn canonical_to_raw(&self, offset: usize, affinity: Affinity) -> Option<usize> {
        if !self.canonical.is_char_boundary(offset) {
            return None;
        }
        let mut result: Option<usize> = None;
        let mut consider = |candidate: usize| {
            if self.snapshot.raw.is_char_boundary(candidate) {
                result = Some(result.map_or(candidate, |previous| match affinity {
                    Affinity::Before => previous.min(candidate),
                    Affinity::After => previous.max(candidate),
                }));
            }
        };
        let text_start = self
            .snapshot
            .text
            .partition_point(|span| span.canonical.end < offset);
        for span in self.snapshot.text[text_start..]
            .iter()
            .take_while(|span| span.canonical.start <= offset)
        {
            if span.canonical.start <= offset && offset <= span.canonical.end {
                consider(span.raw.start + offset - span.canonical.start);
            }
        }
        let gap_start = self
            .snapshot
            .gaps
            .partition_point(|gap| gap.canonical.end < offset);
        for gap in self.snapshot.gaps[gap_start..]
            .iter()
            .take_while(|gap| gap.canonical.start <= offset)
        {
            if offset == gap.canonical.start {
                consider(gap.raw.start);
            }
            if offset == gap.canonical.end {
                consider(gap.raw.end);
            }
        }
        result
    }

    /// Map exact raw boundaries or return an explicit unresolved whitespace
    /// gap. This never invents a canonical position inside a collapsed gap.
    pub(crate) fn raw_to_canonical(&self, offset: usize) -> Option<RawBoundary> {
        if !self.snapshot.raw.is_char_boundary(offset) {
            return None;
        }
        let gap_start = self
            .snapshot
            .gaps
            .partition_point(|gap| gap.raw.end < offset);
        for gap in self.snapshot.gaps[gap_start..]
            .iter()
            .take_while(|gap| gap.raw.start <= offset)
        {
            if gap.raw.start < offset && offset < gap.raw.end {
                return Some(RawBoundary::Gap {
                    canonical: gap.canonical.clone(),
                    raw: gap.raw.clone(),
                    offset,
                });
            }
        }
        let mut result = None;
        let mut consider = |candidate: usize| -> Option<()> {
            if !self.canonical.is_char_boundary(candidate)
                || result.is_some_and(|previous| previous != candidate)
            {
                return None;
            }
            result = Some(candidate);
            Some(())
        };
        let text_start = self
            .snapshot
            .text
            .partition_point(|span| span.raw.end < offset);
        for span in self.snapshot.text[text_start..]
            .iter()
            .take_while(|span| span.raw.start <= offset)
        {
            if span.raw.start <= offset && offset <= span.raw.end {
                consider(span.canonical.start + offset - span.raw.start)?;
            }
        }
        for gap in self.snapshot.gaps[gap_start..]
            .iter()
            .take_while(|gap| gap.raw.start <= offset)
        {
            if offset == gap.raw.start {
                consider(gap.canonical.start)?;
            }
            if offset == gap.raw.end {
                consider(gap.canonical.end)?;
            }
        }
        result.map(RawBoundary::Exact)
    }
}

/// Capture only a plain leaf block; markup, multiple text nodes and empty or
/// nonmatching extraction are left on the existing canonical path.
pub(super) fn capture(
    node: Node<'_, '_>,
    canonical: &str,
    budget: &mut Budget,
) -> Option<Snapshot> {
    let mut children = node.children();
    let child = children.next()?;
    if !child.is_text() || children.next().is_some() || canonical.is_empty() {
        return None;
    }
    let raw = child.text()?;
    let preferred = node.has_tag_name("pre");
    if raw == canonical || raw.len() > MAX_BLOCK_BYTES || !budget.may_fit(raw.len(), preferred) {
        return None;
    }

    let mut normalized = String::new();
    let mut pending = false;
    let mut gap_start = None;
    let mut text: Vec<TextSpan> = Vec::new();
    let mut gaps: Vec<WhitespaceGap> = Vec::new();
    for (at, ch) in raw.char_indices() {
        if matches!(ch, ' ' | '\t' | '\r' | '\n' | '\u{c}') {
            gap_start.get_or_insert(at);
            super::emit(&mut normalized, &mut pending, ch);
            continue;
        }
        let before = normalized.len();
        super::emit(&mut normalized, &mut pending, ch)?;
        let end = normalized.len();
        let char_start = end.checked_sub(ch.len_utf8())?;
        if let Some(start) = gap_start.take() {
            gaps.push(WhitespaceGap {
                canonical: before..char_start,
                raw: start..at,
            });
        }
        let raw_end = at + ch.len_utf8();
        if let Some(last) = text.last_mut()
            && last.canonical.end == char_start
            && last.raw.end == at
        {
            last.canonical.end = end;
            last.raw.end = raw_end;
        } else {
            text.push(TextSpan {
                canonical: char_start..end,
                raw: at..raw_end,
            });
        }
        if text.len().saturating_add(gaps.len()) > MAX_RECORDS {
            return None;
        }
    }
    if let Some(start) = gap_start {
        gaps.push(WhitespaceGap {
            canonical: normalized.len()..normalized.len(),
            raw: start..raw.len(),
        });
    }
    if text.len().saturating_add(gaps.len()) > MAX_RECORDS || normalized != canonical {
        return None;
    }
    let bytes = std::mem::size_of::<Snapshot>()
        .checked_add(raw.len())?
        .checked_add(
            text.capacity()
                .checked_mul(std::mem::size_of::<TextSpan>())?,
        )?
        .checked_add(
            gaps.capacity()
                .checked_mul(std::mem::size_of::<WhitespaceGap>())?,
        )?;
    if !budget.claim(bytes, preferred) {
        return None;
    }
    Some(Snapshot {
        block_key: node.range().start,
        raw: raw.to_owned(),
        text,
        gaps,
        canonical_sha256: digest(&SHA256, canonical.as_bytes())
            .as_ref()
            .try_into()
            .ok()?,
        raw_sha256: digest(&SHA256, raw.as_bytes()).as_ref().try_into().ok()?,
    })
}
