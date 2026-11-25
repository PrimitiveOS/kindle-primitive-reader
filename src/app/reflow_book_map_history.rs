//! Chapter-level navigation-history cues for the reflow Book Map.
use super::HistoryLocation;
use crate::session::Reading;
use std::collections::HashMap;

#[derive(Clone, Copy, Default)]
pub(super) struct History {
    pub(super) previous: Option<usize>,
    pub(super) next: Option<usize>,
}

pub(super) fn collect(
    reading: &Reading,
    previous: &[HistoryLocation],
    next: &[HistoryLocation],
) -> HashMap<usize, History> {
    let probe = &reading.probe;
    let mut chapters = HashMap::<usize, History>::new();
    for (locations, is_previous) in [(previous, true), (next, false)] {
        for (index, location) in locations.iter().rev().take(3).enumerate() {
            let chapter = match &location.position.anchor {
                Some(anchor) if anchor.book == probe.book => {
                    let Some(chapter) = probe.chapter_index(&anchor.chapter) else {
                        continue;
                    };
                    if anchor.raw_caret.is_some()
                        && (chapter != reading.chapter || probe.page_for(anchor).is_none())
                    {
                        continue;
                    }
                    chapter
                }
                Some(_) => continue,
                None => location.position.chapter,
            };
            if chapter >= probe.chapter_count() {
                continue;
            }
            let history = chapters.entry(chapter).or_default();
            let slot = if is_previous {
                &mut history.previous
            } else {
                &mut history.next
            };
            if slot.is_none() {
                *slot = Some(index + 1);
            }
        }
    }
    chapters
}
