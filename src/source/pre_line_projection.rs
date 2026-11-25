//! Bounded source mapping for CSS pre-line's collapsed display text.
//!
//! This is deliberately separate from canonical source and is not installed
//! into layout until selection, line mapping and search use it consistently.
#![allow(dead_code)]

use std::ops::Range;

use crate::flow_projection::Affinity;

const MAX_BYTES: usize = 64 * 1024;
const MAX_SEGMENTS: usize = 4096;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Segment {
    visible: Range<usize>,
    raw: Range<usize>,
    exact: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PreLineProjection {
    display: String,
    segments: Vec<Segment>,
    raw_len: usize,
}

impl PreLineProjection {
    /// Preserve segment breaks, collapse horizontal CSS whitespace, remove
    /// line-edge spaces and discard the final segment break. Unsupported or
    /// over-budget input returns None without modifying source text.
    pub(crate) fn build(raw: &str) -> Option<Self> {
        if raw.len() > MAX_BYTES {
            return None;
        }
        let mut display = String::with_capacity(raw.len());
        let mut segments: Vec<Segment> = Vec::new();
        let mut pending: Option<Range<usize>> = None;
        let mut chars = raw.char_indices().peekable();
        while let Some((at, ch)) = chars.next() {
            let end = at.checked_add(ch.len_utf8())?;
            match ch {
                ' ' | '\t' | '\u{c}' => {
                    pending = Some(match pending {
                        Some(span) => span.start..end,
                        None => at..end,
                    });
                }
                '\r' | '\n' => {
                    pending = None;
                    let raw_end =
                        if ch == '\r' && chars.peek().is_some_and(|(_, next)| *next == '\n') {
                            chars.next().map(|(i, c)| i + c.len_utf8())?
                        } else {
                            end
                        };
                    let start = display.len();
                    display.push('\n');
                    segments.push(Segment {
                        visible: start..display.len(),
                        raw: at..raw_end,
                        exact: ch == '\n',
                    });
                }
                _ => {
                    if let Some(spaces) = pending.take()
                        && !display.is_empty()
                        && !display.ends_with('\n')
                    {
                        let start = display.len();
                        display.push(' ');
                        segments.push(Segment {
                            visible: start..display.len(),
                            raw: spaces,
                            exact: false,
                        });
                    }
                    let start = display.len();
                    display.push(ch);
                    if let Some(previous) = segments.last_mut()
                        && previous.exact
                        && previous.visible.end == start
                        && previous.raw.end == at
                    {
                        previous.visible.end = display.len();
                        previous.raw.end = end;
                    } else {
                        segments.push(Segment {
                            visible: start..display.len(),
                            raw: at..end,
                            exact: true,
                        });
                    }
                }
            }
            if segments.len() > MAX_SEGMENTS {
                return None;
            }
        }
        if display.ends_with('\n') {
            let last = segments.pop()?;
            if last.visible.end != display.len() || last.visible.len() != 1 {
                return None;
            }
            display.pop();
        }
        if display.is_empty() || segments.is_empty() {
            return None;
        }
        Some(Self {
            display,
            segments,
            raw_len: raw.len(),
        })
    }

    /// Project text ownership without inventing a style across a collapsed
    /// run. If collapsed bytes have different owners, use canonical fallback.
    pub(crate) fn style_spans(
        &self,
        owners: &[super::raw_whitespace::RawStyleRun],
    ) -> Option<Vec<(Range<usize>, usize)>> {
        let mut result: Vec<(Range<usize>, usize)> = Vec::new();
        for segment in &self.segments {
            let mut covered = segment.raw.start;
            let first = owners.partition_point(|run| run.raw.end <= covered);
            let mut pieces = 0usize;
            for owner in owners[first..]
                .iter()
                .take_while(|run| run.raw.start < segment.raw.end)
            {
                let start = owner.raw.start.max(segment.raw.start);
                let end = owner.raw.end.min(segment.raw.end);
                if start != covered || start >= end {
                    return None;
                }
                let visible = if segment.exact {
                    (segment.visible.start + start - segment.raw.start)
                        ..(segment.visible.start + end - segment.raw.start)
                } else {
                    if start != segment.raw.start || end != segment.raw.end {
                        return None;
                    }
                    segment.visible.clone()
                };
                if let Some((prior, element)) = result.last_mut()
                    && *element == owner.element
                    && prior.end == visible.start
                {
                    prior.end = visible.end;
                } else {
                    result.push((visible, owner.element));
                }
                covered = end;
                pieces += 1;
                if result.len() > MAX_SEGMENTS {
                    return None;
                }
            }
            if covered != segment.raw.end || pieces == 0 {
                return None;
            }
        }
        Some(result)
    }

    /// Map a raw source range to its visible fragments. Removed edge spaces
    /// contribute nothing; collapsed runs contribute their visible space only
    /// when the raw range covers the whole run.
    pub(crate) fn visible_fragments(&self, raw: Range<usize>) -> Option<Vec<Range<usize>>> {
        if raw.start > raw.end || raw.end > self.raw_len {
            return None;
        }
        let first = self
            .segments
            .partition_point(|segment| segment.raw.end <= raw.start);
        let mut result = Vec::new();
        for segment in self.segments[first..]
            .iter()
            .take_while(|segment| segment.raw.start < raw.end)
        {
            let start = segment.raw.start.max(raw.start);
            let end = segment.raw.end.min(raw.end);
            if start >= end {
                continue;
            }
            let visible = if segment.exact {
                (segment.visible.start + start - segment.raw.start)
                    ..(segment.visible.start + end - segment.raw.start)
            } else {
                if start != segment.raw.start || end != segment.raw.end {
                    return None;
                }
                segment.visible.clone()
            };
            if !self.display.is_char_boundary(visible.start)
                || !self.display.is_char_boundary(visible.end)
            {
                return None;
            }
            result.push(visible);
            if result.len() > MAX_SEGMENTS {
                return None;
            }
        }
        Some(result)
    }

    pub(crate) fn text(&self) -> &str {
        &self.display
    }

    /// Convert a visible caret to a raw source boundary. At removed space,
    /// affinity selects the corresponding edge instead of guessing inside it.
    pub(crate) fn visible_to_raw(&self, offset: usize, affinity: Affinity) -> Option<usize> {
        self.visible_carets_at(&[(offset, affinity)])?
            .into_iter()
            .next()
    }

    /// Verify a bounded batch with one grapheme scan before mapping offsets.
    pub(crate) fn visible_carets_at(&self, requests: &[(usize, Affinity)]) -> Option<Vec<usize>> {
        if requests.len() > 8192 {
            return None;
        }
        let boundaries: Vec<_> = icu_segmenter::GraphemeClusterSegmenter::new()
            .segment_str(&self.display)
            .collect();
        requests
            .iter()
            .map(|&(offset, affinity)| {
                if boundaries.binary_search(&offset).is_err() {
                    return None;
                }
                self.visible_to_raw_unchecked(offset, affinity)
            })
            .collect()
    }

    fn visible_to_raw_unchecked(&self, offset: usize, affinity: Affinity) -> Option<usize> {
        if !self.display.is_char_boundary(offset) {
            return None;
        }
        let mut result: Option<usize> = None;
        let first = self
            .segments
            .partition_point(|segment| segment.visible.end < offset);
        for segment in self.segments[first..]
            .iter()
            .take_while(|segment| segment.visible.start <= offset)
        {
            let candidate = if segment.exact
                && segment.visible.start <= offset
                && offset <= segment.visible.end
            {
                Some(segment.raw.start + offset - segment.visible.start)
            } else if !segment.exact && offset == segment.visible.start {
                Some(segment.raw.start)
            } else if !segment.exact && offset == segment.visible.end {
                Some(segment.raw.end)
            } else {
                None
            };
            if let Some(candidate) = candidate {
                result = Some(result.map_or(candidate, |old| match affinity {
                    Affinity::Before => old.min(candidate),
                    Affinity::After => old.max(candidate),
                }));
            }
        }
        result.filter(|&at| at <= self.raw_len)
    }

    /// A raw position inside collapsed or omitted whitespace is ambiguous and
    /// is rejected. Exact visible boundaries remain independently reversible.
    pub(crate) fn raw_to_visible(&self, offset: usize, affinity: Affinity) -> Option<usize> {
        if offset > self.raw_len {
            return None;
        }
        let mut result: Option<usize> = None;
        let first = self
            .segments
            .partition_point(|segment| segment.raw.end < offset);
        for segment in self.segments[first..]
            .iter()
            .take_while(|segment| segment.raw.start <= offset)
        {
            let candidate =
                if segment.exact && segment.raw.start <= offset && offset <= segment.raw.end {
                    Some(segment.visible.start + offset - segment.raw.start)
                } else if !segment.exact && offset == segment.raw.start {
                    Some(segment.visible.start)
                } else if !segment.exact && offset == segment.raw.end {
                    Some(segment.visible.end)
                } else {
                    None
                };
            if let Some(candidate) = candidate {
                if self.display.is_char_boundary(candidate) {
                    result = Some(result.map_or(candidate, |old| match affinity {
                        Affinity::Before => old.min(candidate),
                        Affinity::After => old.max(candidate),
                    }));
                }
            }
        }
        result.filter(|&visible| {
            icu_segmenter::GraphemeClusterSegmenter::new()
                .segment_str(&self.display)
                .any(|boundary| boundary == visible)
        })
    }
}
