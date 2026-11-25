//! Bounded virtual bidi controls for inline HTML direction hints.
//!
//! These events are used only by bidi analysis. They are never inserted into
//! canonical or rendered text, so source offsets and copied text stay intact.

use crate::book_style::Style;
use crate::source::{Element, StyledText};
use parley::BaseDirection;

const MAX_RUNS: usize = 8192;
const MAX_DEPTH: usize = 32;
const MAX_EVENTS: usize = 4096;
const MAX_TEXT_BYTES: usize = 128 * 1024;

#[derive(Clone, Copy, PartialEq, Eq)]
struct Frame {
    element: usize,
    open: char,
    close: char,
}

fn chain(owner: usize, root: usize, elements: &[Element], styles: &[Style]) -> Option<Vec<Frame>> {
    let mut frames = Vec::new();
    let mut cursor = owner;
    let mut depth = 0;
    while cursor != root {
        depth += 1;
        if depth > MAX_DEPTH {
            return None;
        }
        let element = elements.get(cursor)?;
        let style = styles.get(cursor)?;
        let control = match (element.tag.as_str(), element.direction_hint) {
            ("bdo", Some(_)) => match style.direction {
                BaseDirection::Ltr => Some(('\u{202d}', '\u{202c}')),
                BaseDirection::Rtl => Some(('\u{202e}', '\u{202c}')),
                BaseDirection::Auto => None,
            },
            ("bdi", None) => Some(('\u{2068}', '\u{2069}')),
            ("bdi", Some(_)) | (_, Some(_)) => {
                let open = match style.direction {
                    BaseDirection::Ltr => '\u{2066}',
                    BaseDirection::Rtl => '\u{2067}',
                    BaseDirection::Auto => '\u{2068}',
                };
                Some((open, '\u{2069}'))
            }
            _ => None,
        };
        if let Some((open, close)) = control {
            frames.push(Frame {
                element: cursor,
                open,
                close,
            });
        }
        cursor = element.parent?;
    }
    frames.reverse();
    Some(frames)
}

fn push(events: &mut Vec<(usize, char)>, offset: usize, control: char) -> Option<()> {
    if events.len() == MAX_EVENTS {
        return None;
    }
    events.push((offset, control));
    Some(())
}

fn close(events: &mut Vec<(usize, char)>, offset: usize, active: &[Frame]) -> Option<()> {
    for frame in active.iter().rev() {
        push(events, offset, frame.close)?;
    }
    Some(())
}

/// Return no controls when exact text ownership cannot be established.
pub(crate) fn for_text(
    text: &str,
    runs: &[StyledText],
    root: usize,
    elements: &[Element],
    styles: &[Style],
) -> Option<Vec<(usize, char)>> {
    if runs.len() > MAX_RUNS || text.len() > MAX_TEXT_BYTES {
        return None;
    }
    let mut events = Vec::new();
    let mut active: Vec<Frame> = Vec::new();
    let mut previous = 0;
    for run in runs {
        if run.bytes.start < previous || run.bytes.end < run.bytes.start {
            return None;
        }
        let gap = text.get(previous..run.bytes.start)?;
        if !gap.is_empty() {
            // Hard breaks delimit bidi paragraphs. Unknown ownership of any
            // other gap must not be guessed.
            if !gap.chars().all(|ch| ch == '\n') {
                return None;
            }
            close(&mut events, previous, &active)?;
            active.clear();
        }
        let content = text.get(run.bytes.clone())?;
        let next = chain(run.element, root, elements, styles)?;
        let shared = active.iter().zip(&next).take_while(|(a, b)| a == b).count();
        close(&mut events, run.bytes.start, &active[shared..])?;
        for frame in &next[shared..] {
            push(&mut events, run.bytes.start, frame.open)?;
        }
        active = next;
        for (at, ch) in content.char_indices() {
            if ch == '\n' {
                let offset = run.bytes.start + at;
                close(&mut events, offset, &active)?;
                for frame in &active {
                    push(&mut events, offset + ch.len_utf8(), frame.open)?;
                }
            }
        }
        previous = run.bytes.end;
    }
    let tail = text.get(previous..)?;
    if !tail.chars().all(|ch| ch == '\n') {
        return None;
    }
    close(&mut events, previous, &active)?;
    Some(events)
}
