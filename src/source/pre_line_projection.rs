//! Bounded source mapping for CSS pre-line's collapsed display text.
//!
//! This is deliberately separate from canonical source and is not installed
//! into layout until selection, line mapping and search use it consistently.
#![allow(dead_code)]

use std::ops::Range;

use crate::flow_projection::Affinity;

const MAX_BYTES: usize = 64 * 1024;
const MAX_SEGMENTS: usize = 4096;

#[derive(Clone, Debug)]
struct Segment {
    visible: Range<usize>,
    raw: Range<usize>,
    exact: bool,
}

#[derive(Clone, Debug)]
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

    pub(crate) fn text(&self) -> &str {
        &self.display
    }

    /// Convert a visible caret to a raw source boundary. At removed space,
    /// affinity selects the corresponding edge instead of guessing inside it.
    pub(crate) fn visible_to_raw(&self, offset: usize, affinity: Affinity) -> Option<usize> {
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
        result
    }
}
