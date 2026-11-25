//! Comparison helpers for verified, optionally raw reading positions.
//!
//! Raw carets are not persisted or installed in the UI yet. Callers that
//! eventually supply one must first validate it against the open source and
//! its canonical anchor; these helpers do not validate untrusted data.

use super::ReadingPosition;
use crate::source::raw_whitespace::{CaretAffinity, RawCaret};

/// Keep legacy positions distinct from raw positions until a source-bound
/// mapping proves that they represent the same location.
pub(super) fn same(
    left: &ReadingPosition,
    left_raw: Option<&RawCaret>,
    right: &ReadingPosition,
    right_raw: Option<&RawCaret>,
) -> bool {
    let canonical = match (&left.anchor, &right.anchor) {
        (Some(left), Some(right)) => left == right,
        (None, None) => left.chapter == right.chapter && left.page == right.page,
        _ => false,
    };
    canonical && left_raw == right_raw
}

/// A tie-break for canonical positions with equal legacy ordering keys. The
/// raw byte offset precedes the fingerprint and affinity, so distinct raw
/// offsets within the same normalized gap remain ordered and distinct.
type RawOrder = (u8, usize, [u8; 32], u8);

fn raw_order(raw: Option<&RawCaret>) -> RawOrder {
    raw.map_or((0, 0, [0; 32], 0), |raw| {
        (
            1,
            raw.raw_offset,
            raw.raw_sha256,
            match raw.affinity {
                CaretAffinity::Before => 0,
                CaretAffinity::After => 1,
            },
        )
    })
}

pub(super) fn contents_order(
    position: &ReadingPosition,
    raw: Option<&RawCaret>,
) -> ((usize, usize, usize, usize), RawOrder) {
    let canonical = position
        .anchor
        .as_ref()
        .map_or((position.chapter, 0, position.page, 0), |anchor| {
            (position.chapter, 1, anchor.block, anchor.offset)
        });
    (canonical, raw_order(raw))
}

pub(super) fn flow_order(
    position: &ReadingPosition,
    raw: Option<&RawCaret>,
) -> ((usize, usize, usize), RawOrder) {
    let canonical = position
        .anchor
        .as_ref()
        .map_or((position.chapter, position.page, 0), |anchor| {
            (position.chapter, anchor.block, anchor.offset)
        });
    (canonical, raw_order(raw))
}
