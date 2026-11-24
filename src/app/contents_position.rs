//! Open either reading bar's Contents list at the active section.
use std::collections::HashMap;

use super::Reader;
use super::contents_view::{ContentsEntry, Target};

type Position = (usize, usize);

pub(super) struct ContentsPositions {
    reading: Position,
    headings: Vec<Option<Position>>,
}

impl ContentsPositions {
    pub(super) fn focus(&self, entries: impl Iterator<Item = usize>) -> Option<usize> {
        current_row(
            self.reading,
            entries.enumerate().filter_map(|(row, source)| {
                Some((row, self.headings.get(source).copied().flatten()?))
            }),
        )
    }

    pub(super) fn reveal(&self) -> impl Iterator<Item = usize> + '_ {
        self.focus(0..self.headings.len()).into_iter().chain(
            self.headings
                .iter()
                .enumerate()
                .filter_map(|(index, &position)| (position == Some(self.reading)).then_some(index)),
        )
    }
}

impl Reader {
    pub(super) fn contents_positions(
        &self,
        entries: &[ContentsEntry],
    ) -> Option<ContentsPositions> {
        let position = self.current_position()?;
        let active = self.active.as_ref().map(|active| &active.reading);
        let chapters: HashMap<_, _> = active
            .into_iter()
            .flat_map(|reading| reading.probe.chapter_entries().iter().enumerate())
            .map(|(index, chapter)| (chapter.href.as_str(), index))
            .collect();
        let state = self.library.state(self.open_book_id()?);
        let headings = entries
            .iter()
            .map(|entry| {
                let key = match &entry.target {
                    Target::Fixed(page) => (0, *page),
                    Target::Reflow(target) => {
                        let chapter = *chapters.get(target.path.as_str())?;
                        let reading = active?;
                        let page = match target.fragment.as_deref() {
                            Some(fragment) if chapter == reading.chapter => {
                                reading.probe.page_for(&reading.probe.fragment(fragment)?)?
                            }
                            // Only the open spine item needs laying out. Earlier sections
                            // in another chapter are already in publisher reading order.
                            _ => 0,
                        };
                        (chapter, page)
                    }
                    Target::Custom(index) => {
                        let target = &state?.custom_contents.entries().get(*index)?.position;
                        let page = match (active, target.anchor.as_ref()) {
                            (Some(reading), Some(anchor)) if target.chapter == reading.chapter => {
                                reading.probe.page_for(anchor).unwrap_or(target.page)
                            }
                            _ => target.page,
                        };
                        (target.chapter, page)
                    }
                    Target::None => return None,
                };
                Some(key)
            })
            .collect();
        Some(ContentsPositions {
            reading: (position.chapter, position.page),
            headings,
        })
    }
}

fn current_row(
    current: Position,
    destinations: impl Iterator<Item = (usize, Position)>,
) -> Option<usize> {
    let mut preceding = None;
    for (row, position) in destinations {
        if position == current {
            // KOReader identifies the first heading sharing the current page.
            return Some(row);
        }
        if position < current && preceding.is_none_or(|(_, previous)| position >= previous) {
            preceding = Some((row, position));
        }
    }
    preceding.map(|(row, _)| row)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uses_the_first_heading_on_the_page_and_the_last_preceding_heading_between_pages() {
        let headings = [(3, (0, 1)), (4, (0, 5)), (5, (0, 5)), (6, (0, 12))];
        assert_eq!(current_row((0, 0), headings.into_iter()), None);
        assert_eq!(current_row((0, 5), headings.into_iter()), Some(4));
        assert_eq!(current_row((0, 9), headings.into_iter()), Some(5));
        assert_eq!(current_row((1, 0), headings.into_iter()), Some(6));
    }
}
