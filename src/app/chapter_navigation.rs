//! Previous and next chapter use the active Contents and rendered page order.

use std::collections::{BTreeMap, HashMap};
use std::ops::Range;

use super::contents_view::{ContentsEntry, Target};
use super::{Reader, position};
use crate::fixed_session::FixedReading;
use crate::library::Bookmark;
use crate::session::Reading;
use crate::{Anchor, Probe, Result};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Direction {
    Previous,
    Next,
}

impl Direction {
    fn beyond(self, value: usize, reference: usize) -> bool {
        match self {
            Self::Previous => value < reference,
            Self::Next => value > reference,
        }
    }

    fn closest(self, candidates: impl Iterator<Item = usize>) -> Option<usize> {
        match self {
            Self::Previous => candidates.max(),
            Self::Next => candidates.min(),
        }
    }
}

impl Reader {
    pub(super) fn jump_chapter(&mut self, direction: Direction) -> Result<()> {
        let book = self.open_book_id().ok_or("Open a document first")?;
        let headings = self.contents_entries(book);
        let custom = self
            .library
            .state(book)
            .map_or(&[][..], |state| state.custom_contents.entries());
        if let Some(fixed) = &self.fixed {
            if let Some(page) = fixed_page(&fixed.reading, &headings, custom, direction) {
                let previous = self.jump_fixed(page, None)?;
                if self.current_history_location().as_ref() != Some(&previous) {
                    self.push_history(previous);
                    return Ok(());
                }
            }
        } else if let Some(active) = &mut self.active
            && let Some((chapter, page, probe)) =
                reflow_page(&active.reading, headings, custom, direction)
        {
            let previous = position(&active.reading);
            active.reading.go_to_laid_out_page(chapter, page, probe)?;
            self.finish_reflow_jump(previous);
            return Ok(());
        }
        self.notice = Some(
            match direction {
                Direction::Previous => "No previous chapter",
                Direction::Next => "No next chapter",
            }
            .into(),
        );
        Ok(())
    }
}

fn fixed_page(
    reading: &FixedReading,
    headings: &[ContentsEntry],
    custom: &[Bookmark],
    direction: Direction,
) -> Option<usize> {
    let current = reading.page();
    let flows = reading.hidden_flow_map();
    let current_flow = flows.map_or(0, |map| map.flow(current));
    direction.closest(
        headings
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| match entry.target {
                Target::Fixed(page) if reading.stable_outline_destination(index) => Some(page),
                Target::Custom(index) => {
                    let position = &custom.get(index)?.position;
                    (position.chapter == 0 && position.anchor.is_none()).then_some(position.page)
                }
                _ => None,
            })
            .filter(|&page| {
                page < reading.count()
                    && direction.beyond(page, current)
                    && flows.is_none_or(|map| matches_flow(map.flow(page), current_flow))
            }),
    )
}

enum AnchorTarget<'a> {
    Start,
    Fragment(String),
    Custom(&'a Anchor),
}

impl AnchorTarget<'_> {
    fn page(&self, probe: &Probe) -> Option<usize> {
        match self {
            Self::Start => Some(0),
            Self::Fragment(fragment) => probe.page_for(&probe.fragment(fragment)?),
            Self::Custom(anchor) => probe.page_for(anchor),
        }
    }
}

fn reflow_page(
    reading: &Reading,
    headings: Vec<ContentsEntry>,
    custom: &[Bookmark],
    direction: Direction,
) -> Option<(usize, usize, Option<Probe>)> {
    let current = reading.chapter;
    // The start of a heading already visible on the second page of a spread
    // does not count as the next chapter. Previous starts from the first page.
    let reference = match direction {
        Direction::Previous => reading.first_visible_page(),
        Direction::Next => reading.last_visible_page(),
    };
    let mut by_path = HashMap::new();
    for (index, chapter) in reading.probe.chapter_entries().iter().enumerate() {
        by_path.entry(chapter.href.as_str()).or_insert(index);
    }
    let mut by_chapter = BTreeMap::<_, Vec<_>>::new();
    for heading in headings {
        let (chapter, target) = match heading.target {
            Target::Reflow(destination) => {
                let Some(&chapter) = by_path.get(destination.path.as_str()) else {
                    continue;
                };
                let target = destination
                    .fragment
                    .map_or(AnchorTarget::Start, AnchorTarget::Fragment);
                (chapter, target)
            }
            Target::Custom(index) => {
                let Some(position) = custom.get(index).map(|mark| &mark.position) else {
                    continue;
                };
                let Some(anchor) = &position.anchor else {
                    continue;
                };
                if by_path.get(anchor.chapter.as_str()) != Some(&position.chapter) {
                    continue;
                }
                (position.chapter, AnchorTarget::Custom(anchor))
            }
            _ => continue,
        };
        if chapter == current || direction.beyond(chapter, current) {
            by_chapter.entry(chapter).or_default().push(target);
        }
    }
    let active_nonlinear = visible_nonlinear_run(reading);
    let current_flow = reading
        .custom_flow_map()
        .map_or(0, |map| map.flow(reference));
    // Resolve only spines with heading targets on the requested side. Keep
    // at most one extra layout and render it directly once a target is found.
    while let Some((chapter, targets)) = match direction {
        Direction::Previous => by_chapter.pop_last(),
        Direction::Next => by_chapter.pop_first(),
    } {
        if active_nonlinear.as_ref().is_some_and(|run| {
            reading.probe.chapter_is_nonlinear(chapter) && !run.contains(&chapter)
        }) {
            continue;
        }
        let loaded = if chapter == current {
            None
        } else if let Ok(probe) = reading.config.chapter(chapter) {
            Some(probe)
        } else {
            continue;
        };
        let probe = loaded.as_ref().unwrap_or(&reading.probe);
        let flows = reading.custom_flow_map_for_chapter(chapter, probe.page_count());
        let page = direction.closest(
            targets
                .iter()
                .filter_map(|target| target.page(probe))
                .filter(|&page| {
                    (chapter != current || direction.beyond(page, reference))
                        && flows
                            .as_ref()
                            .is_none_or(|map| matches_flow(map.flow(page), current_flow))
                }),
        );
        if let Some(page) = page {
            return Some((chapter, page, loaded));
        }
    }
    None
}

fn matches_flow(flow: usize, current: usize) -> bool {
    flow == 0 || flow == current
}

fn visible_nonlinear_run(reading: &Reading) -> Option<Range<usize>> {
    let probe = &reading.probe;
    let current = reading.chapter;
    if !reading.hides_nonlinear() {
        return None;
    }
    if !probe.chapter_is_nonlinear(current) {
        return Some(0..0);
    }
    let before = (0..current)
        .rev()
        .take_while(|&chapter| probe.chapter_is_nonlinear(chapter))
        .count();
    let after = (current + 1..probe.chapter_count())
        .take_while(|&chapter| probe.chapter_is_nonlinear(chapter))
        .count();
    Some(current - before..current + 1 + after)
}

#[cfg(test)]
mod tests;
