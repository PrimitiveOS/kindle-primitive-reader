//! Open either reading bar's Contents list at the active section.
use std::collections::HashMap;

use super::custom_contents::CustomAction;
use super::{Action, Reader, Screen};

impl Reader {
    pub(super) fn locate_contents_row(&self) -> Option<usize> {
        if !matches!(self.screen, Screen::Contents | Screen::FixedContents) {
            return None;
        }
        let position = self.current_position()?;
        let active = self.active.as_ref().map(|active| &active.reading);
        let chapters: HashMap<_, _> = active
            .into_iter()
            .flat_map(|reading| reading.probe.chapter_entries().iter().enumerate())
            .map(|(index, chapter)| (chapter.href.as_str(), index))
            .collect();
        let state = self.library.state(self.open_book_id()?);
        let items = self.items();
        let destinations = items.iter().enumerate().filter_map(|(row, item)| {
            let key = match &item.action {
                Action::FixedPage(page) => (0, *page),
                Action::Jump(target) => {
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
                Action::Custom(CustomAction::Open(index)) => {
                    let target = &state?.custom_contents.entries().get(*index)?.position;
                    let page = match (active, target.anchor.as_ref()) {
                        (Some(reading), Some(anchor)) if target.chapter == reading.chapter => {
                            reading.probe.page_for(anchor).unwrap_or(target.page)
                        }
                        _ => target.page,
                    };
                    (target.chapter, page)
                }
                _ => return None,
            };
            Some((row, key))
        });
        current_row((position.chapter, position.page), destinations)
    }
}

fn current_row(
    current: (usize, usize),
    destinations: impl Iterator<Item = (usize, (usize, usize))>,
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
