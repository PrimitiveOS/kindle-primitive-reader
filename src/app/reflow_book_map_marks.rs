//! Read-only chapter-level saved-mark counts for the reflow Book Map.
use crate::Anchor;
use crate::library::BookState;
use crate::session::Reading;
use std::collections::HashMap;

#[derive(Clone, Copy, Default)]
pub(super) struct Counts {
    pub(super) bookmarks: usize,
    pub(super) highlights: usize,
    pub(super) notes: usize,
}

pub(super) fn collect(reading: &Reading, state: Option<&BookState>) -> Vec<Counts> {
    let chapters = reading.probe.chapter_entries();
    let mut counts = vec![Counts::default(); chapters.len()];
    let Some(state) = state else {
        return counts;
    };
    let mut paths = HashMap::with_capacity(chapters.len());
    for (index, chapter) in chapters.iter().enumerate() {
        paths.entry(chapter.href.as_str()).or_insert(index);
    }
    let chapter_of = |anchor: &Anchor| -> Option<usize> {
        if anchor.book != reading.probe.book {
            return None;
        }
        let chapter = *paths.get(anchor.chapter.as_str())?;
        if anchor.raw_caret.is_some()
            && (chapter != reading.chapter || reading.probe.page_for(anchor).is_none())
        {
            return None;
        }
        Some(chapter)
    };
    for mark in &state.bookmarks {
        let chapter = match &mark.position.anchor {
            Some(anchor) => chapter_of(anchor),
            None => Some(mark.position.chapter),
        };
        if let Some(count) = chapter.and_then(|index| counts.get_mut(index)) {
            count.bookmarks = count.bookmarks.saturating_add(1);
            if mark
                .note
                .as_ref()
                .is_some_and(|note| !note.trim().is_empty())
            {
                count.notes = count.notes.saturating_add(1);
            }
        }
    }
    for mark in &state.highlights {
        let Some(chapter) = chapter_of(&mark.selection.start) else {
            continue;
        };
        // A raw selection is counted only when both ends can be verified in
        // the already-open chapter. Do not trust an unverified continuation.
        if (mark.selection.start.raw_caret.is_some() || mark.selection.end.raw_caret.is_some())
            && (chapter_of(&mark.selection.end) != Some(chapter)
                || chapter != reading.chapter
                || reading.probe.page_for(&mark.selection.start).is_none()
                || reading.probe.page_for(&mark.selection.end).is_none())
        {
            continue;
        }
        if let Some(count) = counts.get_mut(chapter) {
            count.highlights = count.highlights.saturating_add(1);
            if mark
                .note
                .as_ref()
                .is_some_and(|note| !note.trim().is_empty())
            {
                count.notes = count.notes.saturating_add(1);
            }
        }
    }
    counts
}
