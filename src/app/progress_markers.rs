//! Contents markers for tracks with known page coordinates.

use super::{ActiveBook, Reader};
use crate::Probe;
use crate::fixed_session::FixedReading;
use crate::library::BookState;

pub(super) struct MarkerPages {
    pub(super) starts: Vec<usize>,
    pub(super) count: usize,
}

impl Probe {
    // Keep every source heading until after depth selection, including two
    // headings at different depths on the same rendered page.
    pub(crate) fn local_contents_pages(&self) -> &[(usize, usize)] {
        self.local_contents_pages.get_or_init(|| {
            self.toc
                .iter()
                .enumerate()
                .filter_map(|(index, entry)| {
                    let destination = entry.destination.as_ref()?;
                    if destination.path != self.chapter {
                        return None;
                    }
                    let page = match &destination.fragment {
                        Some(fragment) => self.page_for(&self.fragment(fragment)?)?,
                        None => 0,
                    };
                    Some((index, page))
                })
                .collect()
        })
    }
}

impl Reader {
    pub(super) fn has_whole_book_marker_coordinates(&self) -> bool {
        self.fixed.is_some()
            || self
                .active
                .as_ref()
                .is_some_and(|active| active.reading.probe.chapter_count() == 1)
    }

    pub(super) fn whole_book_marker_pages(&self, only_if_customized: bool) -> Option<MarkerPages> {
        let state = self.library.state(self.open_book_id()?)?;
        if only_if_customized
            && !state.contents_depths.has_ignored()
            && !state.custom_contents.enabled
        {
            return None;
        }
        if let Some(fixed) = &self.fixed {
            let reading = &fixed.reading;
            return Some(MarkerPages {
                starts: sorted_pages(fixed_headings(reading, state).map(|(page, _)| page)),
                count: reading.count(),
            });
        }
        let active = self.active.as_ref()?;
        // Off-spine fragment page numbers would require loading and laying
        // out other chapters. The existing whole-book track uses source sizes.
        if active.reading.probe.chapter_count() != 1 {
            return None;
        }
        Some(MarkerPages {
            starts: sorted_pages(local_headings(active, state).map(|(page, _)| page)),
            count: active.reading.probe.page_count(),
        })
    }

    pub(super) fn publisher_header_marker_pages(&self, book: &ActiveBook) -> Option<MarkerPages> {
        let state = self.library.state(book.id)?;
        if state.custom_contents.enabled {
            return None;
        }
        let (starts, count) = book
            .reading
            .whole_book_publisher_markers(|depth| state.contents_depths.includes(depth))?;
        Some(MarkerPages { starts, count })
    }

    pub(super) fn customized_header_marker_pages(&self, book: &ActiveBook) -> Option<Vec<usize>> {
        let state = self.library.state(book.id)?;
        (state.contents_depths.has_ignored() || state.custom_contents.enabled)
            .then(|| sorted_pages(local_headings(book, state).map(|(page, _)| page)))
    }

    pub(super) fn bound_footer_chapter_title(&self) -> Option<&str> {
        let state = self.library.state(self.open_book_id()?)?;
        if !state.contents_depths.bind_title || !state.contents_depths.has_ignored() {
            return None;
        }
        if let Some(fixed) = &self.fixed {
            return Some(
                chapter_title_at(fixed_headings(&fixed.reading, state), fixed.reading.page())
                    .unwrap_or_default(),
            );
        }
        let active = self.active.as_ref()?;
        (active.reading.probe.chapter_count() == 1).then(|| {
            chapter_title_at(local_headings(active, state), active.reading.page).unwrap_or_default()
        })
    }
}

fn fixed_headings<'a>(
    reading: &'a FixedReading,
    state: &'a BookState,
) -> impl Iterator<Item = (usize, &'a str)> {
    let publisher = (!state.custom_contents.enabled)
        .then_some(reading)
        .into_iter()
        .flat_map(FixedReading::chapter_outline)
        .filter_map(move |heading| {
            let page = heading.page.filter(|&page| {
                state.contents_depths.includes(heading.depth) && page < reading.count()
            })?;
            Some((page, heading.title.as_str()))
        });
    let custom = selected_custom_headings(state).filter_map(move |entry| {
        let position = &entry.position;
        (position.chapter == 0 && position.anchor.is_none() && position.page < reading.count())
            .then_some((position.page, entry.label.as_str()))
    });
    publisher.chain(custom)
}

fn local_headings<'a>(
    book: &'a ActiveBook,
    state: &'a BookState,
) -> impl Iterator<Item = (usize, &'a str)> {
    let reading = &book.reading;
    let probe = &reading.probe;
    let publisher = (!state.custom_contents.enabled)
        .then(|| probe.local_contents_pages())
        .into_iter()
        .flatten()
        .filter_map(move |&(index, page)| {
            let entry = &probe.toc[index];
            state
                .contents_depths
                .includes(entry.depth)
                .then_some((page, entry.label.as_str()))
        });
    let custom = selected_custom_headings(state).filter_map(move |entry| {
        let position = &entry.position;
        if position.chapter != reading.chapter {
            return None;
        }
        let page = probe.page_for(position.anchor.as_ref()?)?;
        Some((page, entry.label.as_str()))
    });
    publisher.chain(custom)
}

fn selected_custom_headings(state: &BookState) -> impl Iterator<Item = &crate::library::Bookmark> {
    (state.custom_contents.enabled && state.contents_depths.includes(0))
        .then(|| state.custom_contents.entries())
        .into_iter()
        .flatten()
}

fn chapter_title_at<'a>(
    headings: impl Iterator<Item = (usize, &'a str)>,
    current: usize,
) -> Option<&'a str> {
    // For a page with several eligible headings, use its first title while
    // reading that page and its last title after leaving it.
    let mut previous = None;
    for (page, title) in headings {
        if page == current {
            return Some(title);
        }
        if page < current && previous.is_none_or(|(prior_page, _)| prior_page <= page) {
            previous = Some((page, title));
        }
    }
    previous.map(|(_, title)| title)
}

fn sorted_pages(pages: impl Iterator<Item = usize>) -> Vec<usize> {
    let mut pages: Vec<_> = pages.collect();
    pages.sort_unstable();
    pages.dedup();
    pages
}

#[cfg(test)]
mod tests;
