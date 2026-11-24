//! Bounded ECMAScript searches over original UTF-8 source coordinates.
use super::{CASE_MAPPER, FoldedText, MAX_QUERY_BYTES, OffsetEdit, SOFT_HYPHEN};
use crate::Result;
use icu_normalizer::ComposingNormalizer;
use std::{
    ffi::c_void,
    ops::Range,
    ptr::NonNull,
    sync::atomic::{AtomicBool, Ordering},
};

// Budget is shared by every candidate in one source block, including reverse
// scans. Most ordinary searches use only a few byte advances per source byte.
const MAX_BLOCK_BYTES: usize = 1024 * 1024;
const MAX_STEPS: usize = 8 * 1024 * 1024;
const MAX_DEPTH: usize = 24;

unsafe extern "C" {
    fn reader_regex_compile(pattern: *const u8, len: usize, error: *mut i32) -> *mut c_void;
    fn reader_regex_free(compiled: *mut c_void);
    fn reader_regex_next(
        compiled: *const c_void,
        text: *const u8,
        len: usize,
        offset: usize,
        start: *mut usize,
        end: *mut usize,
        remaining: *mut usize,
        cancelled: extern "C" fn(*const AtomicBool) -> bool,
        context: *const AtomicBool,
    ) -> i32;
}

extern "C" fn stopped(flag: *const AtomicBool) -> bool {
    // The caller retains this flag until the synchronous native call returns.
    unsafe { (*flag).load(Ordering::Relaxed) }
}

pub(super) struct Expression {
    compiled: NonNull<c_void>,
    match_case: bool,
}

// The engine owns its compiled automaton. A request moves it to one worker and
// uses it there; it is deliberately not Sync.
unsafe impl Send for Expression {}

impl Drop for Expression {
    fn drop(&mut self) {
        unsafe { reader_regex_free(self.compiled.as_ptr()) }
    }
}

impl Expression {
    pub(super) fn new(pattern: &str, match_case: bool) -> Result<Self> {
        if pattern.is_empty() || pattern.len() > MAX_QUERY_BYTES {
            return Err("Enter a pattern of 1 to 256 UTF-8 bytes".into());
        }
        let normalized = ComposingNormalizer::new_nfc().normalize(pattern);
        let mut cleaned = String::with_capacity(pattern.len());
        let mut escaped = false;
        for ch in normalized.chars().filter(|ch| *ch != SOFT_HYPHEN) {
            cleaned.push(if !match_case && !escaped {
                CASE_MAPPER.simple_lowercase(ch)
            } else {
                ch
            });
            escaped = ch == '\\' && !escaped;
        }
        if cleaned.is_empty() {
            return Err("Enter a visible pattern".into());
        }
        check_repetitions(&cleaned)?;
        let mut error = 0;
        let ptr = unsafe { reader_regex_compile(cleaned.as_ptr(), cleaned.len(), &mut error) };
        let compiled = NonNull::new(ptr).ok_or_else(|| expression_error(error))?;
        Ok(Self {
            compiled,
            match_case,
        })
    }

    /// Finds non-empty matches in publication order, with the original block
    /// retained for ^, $, word boundaries, backreferences and lookarounds.
    pub(super) fn ranges(
        &self,
        source: &str,
        window: Range<usize>,
        limit: usize,
        previous: bool,
        cancel: &AtomicBool,
    ) -> Result<Vec<Range<usize>>> {
        self.ranges_with_budget(source, window, limit, previous, cancel, MAX_STEPS)
    }

    fn ranges_with_budget(
        &self,
        source: &str,
        window: Range<usize>,
        limit: usize,
        previous: bool,
        cancel: &AtomicBool,
        mut remaining: usize,
    ) -> Result<Vec<Range<usize>>> {
        if limit == 0 || window.is_empty() || cancel.load(Ordering::Relaxed) {
            return Ok(Vec::new());
        }
        if source.len() > MAX_BLOCK_BYTES {
            return Err("A paragraph exceeds the pattern search limit".into());
        }
        let Some(projected) = project(source, self.match_case, cancel) else {
            return Ok(Vec::new());
        };
        let byte = |pos: usize| {
            let n = projected.edits.partition_point(|e| e.source_end <= pos);
            n.checked_sub(1).map_or(pos, |i| {
                let e = &projected.edits[i];
                e.folded_end + (pos - e.source_end)
            })
        };
        let mut offset = byte(window.start);
        let boundary = byte(window.end);
        let mut hits = Vec::new();
        while offset < boundary && !cancel.load(Ordering::Relaxed) {
            let (mut start, mut end) = (0usize, 0usize);
            let rc = unsafe {
                reader_regex_next(
                    self.compiled.as_ptr(),
                    projected.text.as_ptr(),
                    projected.text.len(),
                    offset,
                    &mut start,
                    &mut end,
                    &mut remaining,
                    stopped,
                    cancel,
                )
            };
            if rc < 0 {
                return Err(expression_error(-rc).into());
            }
            if rc == 0 || start >= boundary {
                break;
            }
            if start < offset
                || end <= start
                || !projected.text.is_char_boundary(start)
                || !projected.text.is_char_boundary(end)
            {
                return Err("Invalid pattern result".into());
            }
            if end <= boundary {
                let from = projected
                    .source_offset(start, true)
                    .ok_or("Invalid pattern start")?;
                let to = projected
                    .source_offset(end, false)
                    .ok_or("Invalid pattern end")?;
                if previous {
                    hits.clear();
                }
                hits.push(from..to);
                if !previous && hits.len() == limit {
                    break;
                }
            }
            offset = end;
        }
        Ok(hits)
    }
}

fn project(text: &str, match_case: bool, cancel: &AtomicBool) -> Option<FoldedText> {
    if text.is_ascii() {
        return Some(FoldedText {
            text: if match_case {
                text.to_owned()
            } else {
                text.to_ascii_lowercase()
            },
            edits: Vec::new(),
        });
    }
    let mut out = String::with_capacity(text.len());
    let mut edits = Vec::new();
    for (index, (pos, ch)) in text.char_indices().enumerate() {
        if index % 2048 == 0 && cancel.load(Ordering::Relaxed) {
            return None;
        }
        let start = out.len();
        if ch != SOFT_HYPHEN {
            out.push(if match_case {
                ch
            } else {
                CASE_MAPPER.simple_lowercase(ch)
            });
        }
        if out.len() - start != ch.len_utf8() {
            edits.push(OffsetEdit {
                source_end: pos + ch.len_utf8(),
                folded_start: start,
                folded_end: out.len(),
            });
        }
    }
    Some(FoldedText { text: out, edits })
}

// Reject repeated, already variable groups before compiling them, as the stock
// reflow search does. Bound counted repetitions and nesting during compilation.
fn check_repetitions(pattern: &str) -> Result<()> {
    let mut groups = Vec::new();
    let mut escaped = false;
    let mut class = false;
    let mut just_closed = false;
    for (at, ch) in pattern.char_indices() {
        if escaped {
            escaped = false;
            just_closed = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            just_closed = false;
            continue;
        }
        if class {
            if ch == ']' {
                class = false;
            }
            continue;
        }
        match ch {
            '[' => class = true,
            '(' => {
                if groups.len() >= MAX_DEPTH {
                    return Err("Pattern nesting is too deep".into());
                }
                groups.push(false);
            }
            ')' => {
                just_closed = groups.pop().unwrap_or(false);
                if just_closed && let Some(parent) = groups.last_mut() {
                    *parent = true;
                }
                continue;
            }
            '*' | '+' | '{' => {
                if just_closed && ch != '{' {
                    return Err("Nested repetition is too complex".into());
                }
                if ch == '{' {
                    let counts = &pattern[at + 1..];
                    if let Some((counts, _)) = counts.split_once('}')
                        && !counts.is_empty()
                        && counts.chars().all(|c| c.is_ascii_digit() || c == ',')
                        && counts
                            .split(',')
                            .filter(|s| !s.is_empty())
                            .any(|s| s.parse::<u32>().unwrap_or(u32::MAX) > 4096)
                    {
                        return Err("Pattern repetition exceeds 4096".into());
                    }
                }
                if ch != '{'
                    && let Some(parent) = groups.last_mut()
                {
                    *parent = true;
                }
            }
            _ => {}
        }
        just_closed = false;
    }
    Ok(())
}

fn expression_error(code: i32) -> &'static str {
    match code {
        102 => "Invalid escape in pattern",
        103 => "Invalid backreference in pattern",
        104 => "Unmatched square bracket in pattern",
        105 => "Unmatched parenthesis in pattern",
        106 | 107 => "Invalid repetition in pattern",
        108 => "Invalid character range in pattern",
        109 => "Insufficient memory for this pattern",
        110 => "Pattern repetition needs an expression",
        111 | 112 => "Pattern is too complex for this passage",
        200 => "Invalid lookbehind in pattern",
        _ => "Invalid regular expression",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spans<'a>(pattern: &str, text: &'a str, case: bool) -> Vec<&'a str> {
        Expression::new(pattern, case)
            .unwrap()
            .ranges(text, 0..text.len(), 100, false, &AtomicBool::new(false))
            .unwrap()
            .into_iter()
            .map(|r| &text[r])
            .collect()
    }

    #[test]
    fn ecmascript_preserves_captures_lookaround_escapes_and_original_unicode_ranges() {
        assert_eq!(
            spans(r"\b([A-Z]+)\s+\1\b", "Storm storm. Sun sea", false),
            ["Storm storm"]
        );
        assert_eq!(
            spans(r"(?<=at )\d{2}(?= knots)", "12 at 18 knots; 40 knots", true),
            ["18"]
        );
        assert_eq!(
            spans("VA\u{300}|ĐẸP", "và · đẹp · va · đ\u{ad}ẹp", false),
            ["và", "đẹp", "đ\u{ad}ẹp"]
        );
        assert_eq!(spans("VÀ", "và VÀ", true), ["VÀ"]);
        assert_eq!(spans(r"a.💧", "a中💧", true), ["a中💧"]);
        assert_eq!(spans(r"(?:[0-9]{2}){2}", "00 1234", true), ["1234"]);
        assert_eq!(spans(r"\D+", "12 Ab 45", false), [" Ab "]);
        assert!(spans(r"(?=wind)", "wind", true).is_empty());
        assert_eq!(
            spans(r"^tide|time$", "tide tide time", true),
            ["tide", "time"]
        );
    }

    #[test]
    fn selecting_a_long_pattern_keeps_its_source_range_and_limits_the_result_preview() {
        let source = "đ".repeat(8000);
        let range = Expression::new(".+", true)
            .unwrap()
            .ranges(&source, 0..source.len(), 1, false, &AtomicBool::new(false))
            .unwrap()
            .remove(0);
        assert_eq!(range, 0..source.len());
        let (snippet, shown) = super::super::snippet(&source, range, None);
        assert_eq!(snippet[shown].chars().count(), 256);
        assert!(snippet.ends_with('…'));
        assert!(snippet.len() < 1024);
    }

    #[test]
    fn pattern_windows_retain_true_boundaries_and_backwards_restores_the_source_word() {
        let text = "the tide; the tide; the tide";
        let cancelled = AtomicBool::new(false);
        let find = |p: &str, r, previous| {
            Expression::new(p, true)
                .unwrap()
                .ranges(text, r, 1, previous, &cancelled)
                .unwrap()
        };
        assert!(find("^the", 10..text.len(), false).is_empty());
        assert!(find("tide$", 0..18, true).is_empty());
        assert_eq!(find("(?<=the )tide", 12..text.len(), false), [14..18]);
        assert_eq!(find("the tide", 0..19, true), [10..18]);
        assert_eq!(find("the tide", 18..text.len(), false), [20..28]);
    }

    #[test]
    fn pattern_reports_errors_bounds_and_interrupts_engine_work() {
        for invalid in ["(", "[z-a]", r"\", "(a+)+", "a{9999999}"] {
            assert!(Expression::new(invalid, false).is_err(), "{invalid}");
        }
        let e = Expression::new("a+z", true).unwrap();
        let source = "a".repeat(100_000);
        assert!(
            e.ranges_with_budget(
                &source,
                0..source.len(),
                1,
                false,
                &AtomicBool::new(false),
                1024
            )
            .is_err()
        );
        assert!(
            e.ranges(&source, 0..source.len(), 1, false, &AtomicBool::new(true))
                .unwrap()
                .is_empty()
        );
        let flag = AtomicBool::new(false);
        std::thread::scope(|scope| {
            let flag = &flag;
            scope.spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(1));
                flag.store(true, Ordering::Relaxed)
            });
            assert!(
                e.ranges(&source, 0..source.len(), 1, false, flag)
                    .unwrap()
                    .is_empty()
            );
        });
    }
}
