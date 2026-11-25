//! Verified raw whitespace projection for a future preformatted layout.
//!
//! This plan is intentionally not activated until pagination and persisted
//! anchors can retain raw gap positions without changing existing v1 anchors.
#![allow(dead_code)]

use super::{InlineText, Segment};
use crate::{
    book_style::{Style, WhiteSpace},
    flow_projection::Projection,
    flow_view::{LayoutBlock, StyledRuns},
    source::StyledText,
};

pub(super) struct Plan {
    pub(super) text: InlineText,
    pub(super) runs: Vec<StyledText>,
}

impl Plan {
    pub(super) fn new(
        block: &LayoutBlock<'_>,
        view: &Projection<'_>,
        styles: &[Style],
    ) -> Option<Self> {
        if !view.is_identity()
            || !matches!(&block.styled_text, StyledRuns::Original(_))
            || !block.images.is_empty()
            || !block.ruby.is_empty()
            || !block.generated.is_empty()
            || !block.bidi_controls.is_empty()
            || block.first_letter.is_some()
            || styles.get(block.element)?.first_line.is_some()
            || !styles.get(block.element)?.preformatted_spaces
            || !matches!(
                styles.get(block.element)?.white_space,
                WhiteSpace::Pre | WhiteSpace::PreWrap | WhiteSpace::BreakSpaces
            )
        {
            return None;
        }
        let [original] = &*block.styled_text else {
            return None;
        };
        if original.element != block.element || original.bytes != (0..block.text.len()) {
            return None;
        }
        let bound = block.raw_whitespace.as_ref()?.bind(block.text)?;
        let raw = bound.raw();
        let mut segments = Vec::new();
        for span in bound.text_spans() {
            segments.push(Segment {
                source: span.canonical.clone(),
                offset: span.raw.start,
            });
        }
        for gap in bound.gaps() {
            if gap.canonical.is_empty() {
                continue;
            }
            // An old normalized space needs an identical raw byte. If none is
            // available, existing v1 anchors cannot be safely projected yet.
            if block.text.get(gap.canonical.clone())? != " " {
                return None;
            }
            let raw_gap = raw.get(gap.raw.clone())?;
            let space = raw_gap.bytes().position(|byte| byte == b' ')?;
            segments.push(Segment {
                source: gap.canonical.clone(),
                offset: gap.raw.start.checked_add(space)?,
            });
        }
        segments.sort_by_key(|segment| segment.source.start);
        let mut source_end = 0;
        let mut raw_end = 0;
        for segment in &segments {
            let length = segment.source.len();
            let end = segment.offset.checked_add(length)?;
            if segment.source.start != source_end
                || segment.offset < raw_end
                || raw.get(segment.offset..end)? != block.text.get(segment.source.clone())?
            {
                return None;
            }
            source_end = segment.source.end;
            raw_end = end;
        }
        if source_end != block.text.len() {
            return None;
        }
        Some(Self {
            text: InlineText {
                text: raw.to_owned(),
                segments,
                boxes: Vec::new(),
                generated: true,
                detached_initial: false,
            },
            runs: vec![StyledText {
                bytes: 0..raw.len(),
                element: block.element,
                language: original.language,
                hyphenation_language: original.hyphenation_language,
                preserve_whitespace: original.preserve_whitespace,
            }],
        })
    }
}
