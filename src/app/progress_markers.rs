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
    pub(super) fn local_contents_pages(&self) -> &[(usize, usize)] {
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
                starts: fixed_marker_pages(reading, state),
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
            starts: local_marker_pages(active, state),
            count: active.reading.probe.page_count(),
        })
    }

    pub(super) fn customized_header_marker_pages(&self, book: &ActiveBook) -> Option<Vec<usize>> {
        let state = self.library.state(book.id)?;
        (state.contents_depths.has_ignored() || state.custom_contents.enabled)
            .then(|| local_marker_pages(book, state))
    }
}

fn fixed_marker_pages(reading: &FixedReading, state: &BookState) -> Vec<usize> {
    if state.custom_contents.enabled {
        return sorted_pages(state.custom_contents.entries().iter().filter_map(|entry| {
            let position = &entry.position;
            (state.contents_depths.includes(0)
                && position.chapter == 0
                && position.anchor.is_none()
                && position.page < reading.count())
            .then_some(position.page)
        }));
    }
    sorted_pages(reading.chapter_outline().filter_map(|heading| {
        heading
            .page
            .filter(|&page| state.contents_depths.includes(heading.depth) && page < reading.count())
    }))
}

fn local_marker_pages(book: &ActiveBook, state: &BookState) -> Vec<usize> {
    let reading = &book.reading;
    let probe = &reading.probe;
    if state.custom_contents.enabled {
        return sorted_pages(state.custom_contents.entries().iter().filter_map(|entry| {
            let position = &entry.position;
            if !state.contents_depths.includes(0) || position.chapter != reading.chapter {
                return None;
            }
            probe.page_for(position.anchor.as_ref()?)
        }));
    }
    sorted_pages(
        probe
            .local_contents_pages()
            .iter()
            .filter_map(|&(index, page)| {
                state
                    .contents_depths
                    .includes(probe.toc[index].depth)
                    .then_some(page)
            }),
    )
}

fn sorted_pages(pages: impl Iterator<Item = usize>) -> Vec<usize> {
    let mut pages: Vec<_> = pages.collect();
    pages.sort_unstable();
    pages.dedup();
    pages
}

#[cfg(test)]
mod tests;
