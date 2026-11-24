//! Previous and next chapter use the active Contents and rendered page order.

use std::collections::{BTreeMap, HashMap};
use std::ops::Range;

use super::contents_view::{ContentsEntry, Target};
use super::{Action, Item, Reader, Screen, position};
use crate::fixed_session::FixedReading;
use crate::generated_contents::OutlineTarget;
use crate::library::Bookmark;
use crate::session::Reading;
use crate::{Anchor, Probe, Result};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Direction {
    Previous,
    Next,
}

pub(super) enum LevelSetting {
    BindNavigation,
    BindTitle,
    ToggleLevel(usize),
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
        let state = self.library.state(book);
        let custom = state.map_or(&[][..], |state| state.custom_contents.entries());
        let headings =
            self.contents_entries(book)
                .into_iter()
                .enumerate()
                .filter(|(_, heading)| {
                    state
                        .is_none_or(|state| state.contents_depths.use_for_navigation(heading.depth))
                });
        if let Some(fixed) = &mut self.fixed {
            if let Some(page) = fixed_page(&fixed.reading, headings, custom, direction) {
                if self.screen == Screen::Skim {
                    fixed.reading.jump(page)?;
                    return Ok(());
                }
                let previous = self.jump_fixed(page, None)?;
                if self.current_history_location().as_ref() != Some(&previous) {
                    self.push_history(previous);
                    return Ok(());
                }
            }
        } else if let Some(active) = &mut self.active
            && let Some((chapter, page, probe)) = reflow_page(
                &active.reading,
                headings.map(|(_, heading)| heading),
                custom,
                direction,
            )
        {
            let previous = position(&active.reading);
            active.reading.go_to_laid_out_page(chapter, page, probe)?;
            if self.screen != Screen::Skim {
                self.finish_reflow_jump(previous);
            }
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

    pub(super) fn chapter_level_items(&self) -> Vec<Item> {
        let Some(book) = self.open_book_id() else {
            return Vec::new();
        };
        let Some(state) = self.library.state(book) else {
            return Vec::new();
        };
        let levels = &state.contents_depths;
        let counts = self.chapter_level_counts(book);
        if counts.is_empty() {
            return vec![
                Item::new("No chapter levels", Action::Heading)
                    .detail("The active Contents has no chapter destinations"),
            ];
        }
        let mut rows = vec![
            Item::new(
                "Use levels for chapter jumps",
                if levels.has_ignored() {
                    Action::ChapterLevel(LevelSetting::BindNavigation)
                } else {
                    Action::Heading
                },
            )
            .detail(if !levels.has_ignored() {
                "Turn off a level to enable this option"
            } else if self.uses_alternative_contents() {
                "Chapter jumps and indexed chapter progress use the included levels"
            } else if self.has_whole_book_marker_coordinates() {
                if self.chapter_pages_use_edition_labels() {
                    "Chapter jumps and reading time use the included levels"
                } else {
                    "Chapter jumps, pages left, and reading time use the included levels"
                }
            } else {
                "Previous and next chapter use the included levels"
            }),
        ];
        rows.extend(counts.into_iter().map(|(depth, count)| {
            let unit = if count == 1 { "heading" } else { "headings" };
            Item::new(
                format!("Contents level {}", depth.saturating_add(1)),
                Action::ChapterLevel(LevelSetting::ToggleLevel(depth)),
            )
            .detail(format!("{count} {unit}"))
        }));
        rows.push(Item::new("Progress markers", Action::Heading).detail(
            if self.uses_alternative_contents() {
                "The reading header, footer and Skim use the included levels"
            } else if self.has_whole_book_marker_coordinates() {
                "The footer and Skim use the included levels"
            } else {
                "The reading header uses the included levels for this book"
            },
        ));
        if self.has_chapter_title_levels() {
            rows.push(
                Item::new(
                    "Chapter titles from ticks only",
                    if levels.has_ignored() {
                        Action::ChapterLevel(LevelSetting::BindTitle)
                    } else {
                        Action::Heading
                    },
                )
                .detail(if levels.has_ignored() {
                    "Use included levels for the footer and new saved marks"
                } else {
                    "Turn off a level to enable this option"
                }),
            );
        }
        rows
    }

    pub(super) fn chapter_level_checked(&self, setting: &LevelSetting) -> bool {
        self.open_book_id()
            .and_then(|book| self.library.state(book))
            .is_some_and(|state| match *setting {
                LevelSetting::BindNavigation => state.contents_depths.bind_navigation,
                LevelSetting::BindTitle => state.contents_depths.bind_title,
                LevelSetting::ToggleLevel(depth) => state.contents_depths.includes(depth),
            })
    }

    fn chapter_level_counts(&self, book: u64) -> BTreeMap<usize, usize> {
        let mut counts = BTreeMap::new();
        for (index, heading) in self.contents_entries(book).into_iter().enumerate() {
            let available = match heading.target {
                Target::Fixed(page) => self.fixed.as_ref().is_some_and(|fixed| {
                    page < fixed.reading.count() && fixed.reading.stable_outline_destination(index)
                }),
                Target::None => false,
                _ => true,
            };
            if available {
                *counts.entry(heading.depth).or_default() += 1;
            }
        }
        counts
    }

    fn has_chapter_title_levels(&self) -> bool {
        self.uses_alternative_contents() || self.has_whole_book_marker_coordinates()
    }

    pub(super) fn set_chapter_level(&mut self, setting: LevelSetting) -> Result<()> {
        if matches!(setting, LevelSetting::BindTitle) && !self.has_chapter_title_levels() {
            return Ok(());
        }
        let book = self.open_book_id().ok_or("Open a document first")?;
        let levels = &mut self
            .library
            .state_mut(book)
            .ok_or("This book is unavailable")?
            .contents_depths;
        match setting {
            LevelSetting::BindNavigation if levels.has_ignored() => {
                levels.bind_navigation = !levels.bind_navigation;
            }
            LevelSetting::BindTitle if levels.has_ignored() => {
                levels.bind_title = !levels.bind_title;
            }
            LevelSetting::BindNavigation | LevelSetting::BindTitle => return Ok(()),
            LevelSetting::ToggleLevel(depth) => levels.toggle(depth),
        }
        self.mark_book_changed();
        Ok(())
    }
}

fn fixed_page(
    reading: &FixedReading,
    headings: impl Iterator<Item = (usize, ContentsEntry)>,
    custom: &[Bookmark],
    direction: Direction,
) -> Option<usize> {
    let current = reading.page();
    let flows = reading.hidden_flow_map();
    let current_flow = flows.map_or(0, |map| map.flow(current));
    direction.closest(
        headings
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
    Alternative(Anchor),
}

impl AnchorTarget<'_> {
    fn page(&self, probe: &Probe) -> Option<usize> {
        match self {
            Self::Start => Some(0),
            Self::Fragment(fragment) => probe.page_for(&probe.fragment(fragment)?),
            Self::Custom(anchor) => probe.page_for(anchor),
            Self::Alternative(anchor) => probe.page_for(anchor),
        }
    }
}

fn reflow_page(
    reading: &Reading,
    headings: impl Iterator<Item = ContentsEntry>,
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
            Target::Alternative(OutlineTarget::Chapter(chapter)) => (chapter, AnchorTarget::Start),
            Target::Alternative(OutlineTarget::Heading(chapter, anchor)) => {
                (chapter, AnchorTarget::Alternative(anchor))
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
