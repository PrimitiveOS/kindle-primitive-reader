//! Contents markers for tracks with known page coordinates.

use super::{ActiveBook, Reader};
use crate::Probe;
use crate::fixed_session::FixedReading;
use crate::generated_contents::{OutlineEntry, OutlineTarget};
use crate::library::BookState;

pub(super) struct MarkerPages {
    pub(super) starts: Vec<usize>,
    pub(super) count: usize,
}

pub(super) struct ChapterProgress {
    pub(super) index: usize,
    pub(super) count: usize,
    pub(super) page: usize,
    pub(super) pages: usize,
}

#[derive(Clone, Copy)]
pub(super) enum MarkerTrack {
    Footer,
    Skim,
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
    pub(super) fn publisher_filtered_marker_demand(&self) -> bool {
        self.active.as_ref().is_some_and(|book| {
            book.entry.format == crate::library::DocumentFormat::Epub
                && book.reading.probe.chapter_count() > 1
                && !book.reading.config.hidden_flows.enabled
                && !book.reading.hides_nonlinear()
                && !self.uses_alternative_contents()
                && self.library.state(book.id).is_some_and(|state| {
                    state.contents_depths.has_ignored() && !state.custom_contents.enabled
                })
        })
    }

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

    pub(super) fn alternative_header_marker_pages(&self, book: &ActiveBook) -> Option<MarkerPages> {
        let entries = self.alternative_entries(book.id)?;
        let state = self.library.state(book.id)?;
        let reading = &book.reading;
        if let Some(markers) = reading.whole_book_alternative_locations(entries, |depth| {
            state.contents_depths.includes(depth)
        }) {
            return Some(MarkerPages {
                starts: sorted_pages(markers.starts.into_iter().map(|heading| heading.absolute)),
                count: markers.count,
            });
        }
        Some(MarkerPages {
            starts: sorted_pages(
                entries
                    .iter()
                    .filter(|entry| state.contents_depths.includes(entry.depth))
                    .filter_map(|entry| local_alternative_page(book, entry)),
            ),
            count: reading.probe.page_count(),
        })
    }

    pub(super) fn alternative_book_marker_fractions(&self, track: MarkerTrack) -> Option<Vec<f64>> {
        let book = self.active.as_ref()?;
        let entries = self.alternative_entries(book.id)?;
        let state = self.library.state(book.id)?;
        let reading = &book.reading;
        if reading.config.hidden_flows.enabled {
            return Some(Vec::new());
        }
        let include = |depth| state.contents_depths.includes(depth);
        let mut starts =
            if let Some(markers) = reading.whole_book_alternative_locations(entries, include) {
                markers
                    .starts
                    .into_iter()
                    .filter_map(|marker| {
                        book_marker_fraction(book, track, marker.chapter, marker.page, marker.pages)
                    })
                    .collect::<Vec<_>>()
            } else {
                entries
                    .iter()
                    .filter(|entry| include(entry.depth))
                    .filter_map(|entry| match entry.target {
                        OutlineTarget::Chapter(chapter) => {
                            book_marker_fraction(book, track, chapter, 0, 1)
                        }
                        OutlineTarget::Heading(..) => book_marker_fraction(
                            book,
                            track,
                            reading.chapter,
                            local_alternative_page(book, entry)?,
                            reading.probe.page_count(),
                        ),
                    })
                    .collect()
            };
        starts.sort_unstable_by(f64::total_cmp);
        starts.dedup();
        Some(starts)
    }

    /// Project measured publisher headings onto the same source-weighted
    /// whole-book track as the footer or Skim thumb. Only activate this
    /// path for an explicit level filter; the unfiltered track keeps its
    /// established spine ticks while the complete index is unavailable.
    pub(super) fn publisher_book_marker_fractions(&self, track: MarkerTrack) -> Option<Vec<f64>> {
        let book = self.active.as_ref()?;
        let state = self.library.state(book.id)?;
        let reading = &book.reading;
        if reading.probe.chapter_count() <= 1
            || !state.contents_depths.has_ignored()
            || state.custom_contents.enabled
            || reading.config.hidden_flows.enabled
            || reading.hides_nonlinear()
        {
            return None;
        }
        let locations = reading
            .whole_book_publisher_locations(|depth| state.contents_depths.includes(depth))?;
        let mut starts: Vec<_> = locations
            .starts
            .into_iter()
            .filter_map(|location| {
                book_marker_fraction(book, track, location.chapter, location.page, location.pages)
            })
            .collect();
        starts.sort_unstable_by(f64::total_cmp);
        starts.dedup();
        Some(starts)
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

    pub(super) fn active_alternative_chapter_title(&self) -> Option<&str> {
        let book = self.active.as_ref()?;
        let entries = self.alternative_entries(book.id)?;
        if entries.is_empty() {
            return None;
        }
        let levels = &self.library.state(book.id)?.contents_depths;
        let include = |depth| !levels.bind_title || levels.includes(depth);
        let reading = &book.reading;
        let first = entries.partition_point(|entry| alternative_spine(entry) < reading.chapter);
        let title = chapter_title_at(
            entries[first..]
                .iter()
                .take_while(|entry| alternative_spine(entry) == reading.chapter)
                .filter(|entry| include(entry.depth))
                .filter_map(|entry| {
                    Some((local_alternative_page(book, entry)?, entry.title.as_str()))
                }),
            reading.first_visible_page(),
        );
        if title.is_some() {
            return title;
        }
        if !entries[..first].iter().any(|entry| include(entry.depth)) {
            return Some("");
        }

        // A preceding spine may contain the current title. Use the measured
        // layout and never load a remote chapter solely to paint the footer.
        let locations = reading.whole_book_alternative_locations(entries, include)?;
        Some(
            chapter_title_at(
                locations
                    .starts
                    .iter()
                    .map(|heading| (heading.absolute, entries[heading.index].title.as_str())),
                locations.current,
            )
            .unwrap_or_default(),
        )
    }

    pub(super) fn alternative_chapter_progress(&self) -> Option<ChapterProgress> {
        let book = self.active.as_ref()?;
        let entries = self.alternative_entries(book.id)?;
        let reading = &book.reading;
        if entries.is_empty()
            || !reading.has_exact_local_screen_pages()
            || reading.hides_nonlinear()
        {
            return None;
        }
        let levels = &self.library.state(book.id)?.contents_depths;
        let include = |depth| levels.use_for_navigation(depth);
        let (mut starts, current, total) = if reading.probe.chapter_count() == 1 {
            (
                sorted_pages(
                    entries
                        .iter()
                        .filter(|entry| include(entry.depth))
                        .filter_map(|entry| local_alternative_page(book, entry)),
                ),
                reading.page,
                reading.probe.page_count(),
            )
        } else {
            let locations = reading.whole_book_alternative_locations(entries, include)?;
            (
                sorted_pages(locations.starts.into_iter().map(|heading| heading.absolute)),
                locations.current,
                locations.count,
            )
        };
        // Front matter has a span of its own before the first chapter tick.
        if starts.first() != Some(&0) {
            starts.insert(0, 0);
        }
        let index = starts.partition_point(|&page| page <= current) - 1;
        let start = starts[index];
        let end = starts.get(index + 1).copied().unwrap_or(total);
        Some(ChapterProgress {
            index: index + 1,
            count: starts.len(),
            page: current - start + 1,
            pages: end - start,
        })
    }

    pub(super) fn bound_footer_chapter_title(&self) -> Option<&str> {
        if self.uses_alternative_contents() {
            return self.active_alternative_chapter_title();
        }
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

fn alternative_spine(entry: &OutlineEntry) -> usize {
    match &entry.target {
        OutlineTarget::Chapter(chapter) | OutlineTarget::Heading(chapter, _) => *chapter,
    }
}

fn local_alternative_page(book: &ActiveBook, entry: &OutlineEntry) -> Option<usize> {
    let reading = &book.reading;
    match &entry.target {
        OutlineTarget::Chapter(chapter) if *chapter == reading.chapter => Some(0),
        OutlineTarget::Heading(chapter, anchor) if *chapter == reading.chapter => {
            reading.probe.page_for(anchor)
        }
        _ => None,
    }
}

// Retain the units of each existing progress track. Footer measures page
// boundaries. Skim measures the first-to-last position used by its thumb.
fn book_marker_fraction(
    book: &ActiveBook,
    track: MarkerTrack,
    chapter: usize,
    page: usize,
    pages: usize,
) -> Option<f64> {
    let reading = &book.reading;
    let chapters = reading.probe.chapter_count();
    if chapter >= chapters {
        return None;
    }
    let denominator = match track {
        MarkerTrack::Footer => pages,
        MarkerTrack::Skim => pages.saturating_sub(1),
    }
    .max(1);
    let local = page as f64 / denominator as f64;
    if chapters == 1 {
        return Some(local);
    }
    let weights = reading.progress_weights();
    if let (Some([start, end]), Some(&total)) = (weights.get(chapter..=chapter + 1), weights.last())
        && total > 0
    {
        return (end > start)
            .then(|| (*start as f64 + (end - start) as f64 * local) / total as f64);
    }
    Some(match track {
        MarkerTrack::Footer => (chapter as f64 + local) / chapters as f64,
        MarkerTrack::Skim => chapter as f64 / (chapters - 1) as f64,
    })
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
pub(super) mod tests;
