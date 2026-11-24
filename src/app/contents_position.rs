//! Locate Contents targets in the open layout or the document's stable page map.
use std::collections::HashMap;

use super::contents_view::{ContentsEntry, Target};
use super::{ActiveBook, Reader};
use crate::Anchor;
use crate::epub::Destination;

type Position = (usize, usize);

pub(super) struct ContentsPositions {
    reading: Position,
    headings: Vec<Option<Position>>,
    numbers: Vec<ContentsNumbers>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(in crate::app) struct ContentsNumbers {
    pub page_label: Option<String>,
    pub chapter_length: Option<usize>,
}

#[derive(Clone, Copy)]
enum Page {
    Fixed(usize),
    Layout(Position),
    Reference { label: usize, starts_page: bool },
}

impl Page {
    fn length_until(self, end: Self, open_layout: Option<(usize, usize)>) -> Option<usize> {
        match (self, end) {
            (Self::Fixed(start), Self::Fixed(end)) => end.checked_sub(start),
            (Self::Layout((chapter, page)), Self::Layout((end_chapter, end_page)))
                if chapter == end_chapter =>
            {
                end_page.checked_sub(page)
            }
            (Self::Layout((chapter, page)), Self::Layout((end_chapter, 0)))
                if chapter.checked_add(1) == Some(end_chapter) =>
            {
                open_layout
                    .filter(|(open, _)| *open == chapter)?
                    .1
                    .checked_sub(page)
            }
            (
                Self::Reference { label: start, .. },
                Self::Reference {
                    label: end,
                    starts_page,
                },
            ) => end
                .checked_sub(start)?
                .checked_add(usize::from(!starts_page)),
            _ => None,
        }
    }
}

impl ContentsPositions {
    pub(super) fn numbers(&self, entry: usize) -> Option<&ContentsNumbers> {
        self.numbers.get(entry)
    }

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
        let active = self.active.as_ref();
        let reading = active.map(|book| &book.reading);
        let chapters: HashMap<_, _> = active
            .into_iter()
            .flat_map(|book| book.reading.probe.chapter_entries().iter().enumerate())
            .map(|(index, chapter)| (chapter.href.as_str(), index))
            .collect();
        let state = self.library.state(self.open_book_id()?);
        let references = active.and_then(|book| {
            if book.numbering.is_some() {
                return Some(References::Character);
            }
            (book.page_labels() > 0
                && (self.publisher_footer_enabled() || self.publisher_margins_enabled()))
            .then(|| References::Print(PrintedMap::new(book, &chapters)))
        });
        let fixed = self.fixed.as_ref().map(|book| &book.reading);
        let resolved: Vec<_> = entries
            .iter()
            .map(|entry| {
                let custom = match entry.target {
                    Target::Custom(index) => {
                        Some(&state?.custom_contents.entries().get(index)?.position)
                    }
                    _ => None,
                };
                if let Some(reading) = fixed {
                    let page = match entry.target {
                        Target::Fixed(page) => page,
                        Target::Custom(_) => custom?.page,
                        _ => return None,
                    };
                    return (page < reading.count()).then(|| {
                        (
                            (0, page),
                            Some(Page::Fixed(page)),
                            Some(
                                reading
                                    .page_label(page)
                                    .unwrap_or_else(|| (page + 1).to_string()),
                            ),
                        )
                    });
                }
                let book = active?;
                let reading = &book.reading;
                let (chapter, starts_chapter, anchor, destination) = match &entry.target {
                    Target::Reflow(target) => {
                        let chapter = *chapters.get(target.path.as_str())?;
                        let anchor = if chapter == reading.chapter {
                            match &target.fragment {
                                Some(fragment) => Some(reading.probe.fragment(fragment)?),
                                None => None,
                            }
                        } else {
                            None
                        };
                        (chapter, target.fragment.is_none(), anchor, Some(target))
                    }
                    Target::Custom(_) => {
                        let target = custom?;
                        (
                            target.chapter,
                            target.page == 0 && target.anchor.is_none(),
                            target.anchor.clone(),
                            None,
                        )
                    }
                    _ => return None,
                };
                let page = if starts_chapter {
                    Some(0)
                } else if chapter == reading.chapter {
                    let page = anchor
                        .as_ref()
                        .and_then(|anchor| reading.probe.page_for(anchor));
                    if destination.is_some() && page.is_none() {
                        return None;
                    }
                    page.or_else(|| {
                        let target = custom.filter(|target| target.anchor.is_none())?;
                        (target.page < reading.probe.page_count()).then_some(target.page)
                    })
                } else {
                    None
                };
                // An unopened fragment can still place the current-section highlight in
                // spine order. Its estimated page is never used for visible metadata.
                let focus = (
                    chapter,
                    page.unwrap_or_else(|| custom.map_or(0, |target| target.page)),
                );
                let location = if let Some(references) = &references {
                    references.locate(book, chapter, starts_chapter, anchor.as_ref(), destination)
                } else {
                    page.map(|page| Page::Layout((chapter, page)))
                };
                let label = match location {
                    Some(Page::Layout((chapter, page))) => {
                        Some(format!("{}:{}", chapter + 1, page + 1))
                    }
                    Some(Page::Reference { label, .. }) => {
                        book.page_label(label).map(|label| label.into_owned())
                    }
                    _ => None,
                };
                Some((focus, location, label))
            })
            .collect();
        let open_layout = reading.map(|reader| (reader.chapter, reader.probe.page_count()));
        let end = if let Some(fixed) = fixed {
            Some(Page::Fixed(fixed.count()))
        } else if let Some(references) = &references {
            references.total(active?).map(|label| Page::Reference {
                label,
                starts_page: true,
            })
        } else {
            reading.map(|reader| Page::Layout((reader.probe.chapter_count(), 0)))
        };
        let lengths = if references.is_some() && end.is_none() {
            // A partially resolved print list can still identify individual
            // labels, but unverified entries make its lengths unreliable.
            vec![None; entries.len()]
        } else {
            chapter_lengths(
                entries.iter().map(|entry| entry.depth),
                resolved
                    .iter()
                    .map(|row| row.as_ref().and_then(|(_, page, _)| *page)),
                end,
                open_layout,
            )
        };
        let headings = resolved
            .iter()
            .map(|row| row.as_ref().map(|(focus, _, _)| *focus))
            .collect();
        let numbers = resolved
            .into_iter()
            .zip(lengths)
            .map(|(row, chapter_length)| ContentsNumbers {
                page_label: row.and_then(|(_, _, label)| label),
                chapter_length,
            })
            .collect();
        Some(ContentsPositions {
            reading: (position.chapter, position.page),
            headings,
            numbers,
        })
    }
}

fn chapter_lengths(
    depths: impl Iterator<Item = usize>,
    pages: impl Iterator<Item = Option<Page>>,
    end: Option<Page>,
    open_layout: Option<(usize, usize)>,
) -> Vec<Option<usize>> {
    // KOReader closes every open heading at or below the incoming depth.
    // Use the full outline, so filtering/collapsing never changes the lengths.
    let mut open = Vec::<(usize, usize, Option<Page>)>::new();
    let mut lengths = Vec::new();
    let length = |start: Option<Page>, end: Option<Page>| start?.length_until(end?, open_layout);
    for (depth, page) in depths.zip(pages) {
        while open.last().is_some_and(|(_, prior, _)| *prior >= depth) {
            let (index, _, start) = open.pop().unwrap();
            lengths[index] = length(start, page);
        }
        open.push((lengths.len(), depth, page));
        lengths.push(None);
    }
    for (index, _, start) in open {
        lengths[index] = length(start, end);
    }
    lengths
}

enum References<'a> {
    Character,
    Print(PrintedMap<'a>),
}

impl References<'_> {
    fn locate(
        &self,
        book: &ActiveBook,
        chapter: usize,
        starts_chapter: bool,
        anchor: Option<&Anchor>,
        destination: Option<&Destination>,
    ) -> Option<Page> {
        let (label, starts_page) = match self {
            Self::Print(map) => map.locate(book, chapter, starts_chapter, anchor, destination)?,
            Self::Character => {
                let map = book.numbering.as_ref()?;
                let numbered = map.chapters.get(chapter)?;
                if starts_chapter {
                    (numbered.first, true)
                } else {
                    let anchor = anchor.filter(|anchor| {
                        anchor.book == map.book
                            && anchor.revision == numbered.revision
                            && anchor.chapter == book.reading.probe.chapter_entries()[chapter].href
                    })?;
                    let target = (anchor.block, anchor.offset);
                    let point = numbered
                        .points
                        .partition_point(|point| {
                            point.is_none_or(|point| (point.block, point.byte) <= target)
                        })
                        .checked_sub(1)?;
                    (
                        numbered.first + point,
                        numbered.points[point]
                            .is_some_and(|start| (start.block, start.byte) == target),
                    )
                }
            }
        };
        Some(Page::Reference { label, starts_page })
    }

    fn total(&self, book: &ActiveBook) -> Option<usize> {
        match self {
            Self::Character => Some(book.numbering.as_ref()?.count),
            Self::Print(map) => map.complete.then_some(book.page_labels()),
        }
    }
}

struct PrintedMap<'a> {
    exact: HashMap<(&'a str, Option<&'a str>), usize>,
    preceding: Vec<Option<usize>>,
    has_markers: Vec<bool>,
    current: Vec<(f32, usize)>,
    complete: bool,
}

impl<'a> PrintedMap<'a> {
    fn new(book: &'a ActiveBook, chapters: &HashMap<&str, usize>) -> Self {
        let reading = &book.reading;
        let mut exact = HashMap::new();
        let mut preceding = vec![None; chapters.len()];
        let mut has_markers = vec![false; chapters.len()];
        let mut current = Vec::new();
        let mut complete = true;
        for (label, entry) in reading.probe.printed_pages.iter().enumerate() {
            let Some(target) = entry.destination.as_ref() else {
                complete = false;
                continue;
            };
            let Some(&chapter) = chapters.get(target.path.as_str()) else {
                complete = false;
                continue;
            };
            has_markers[chapter] = true;
            if chapter == reading.chapter {
                let top = match target.fragment.as_deref() {
                    Some(fragment) => reading
                        .probe
                        .fragment(fragment)
                        .and_then(|anchor| reading.probe.anchor_top(&anchor)),
                    None => Some(0.0),
                };
                let Some(top) = top else {
                    complete = false;
                    continue;
                };
                current.push((top, label));
            }
            exact.insert((target.path.as_str(), target.fragment.as_deref()), label);
            preceding[chapter] = Some(label);
        }
        let mut prior = None;
        for chapter in &mut preceding {
            let ending = *chapter;
            *chapter = prior;
            prior = ending.or(prior);
        }
        current.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        Self {
            exact,
            preceding,
            has_markers,
            current,
            complete,
        }
    }

    fn locate(
        &self,
        book: &ActiveBook,
        chapter: usize,
        starts_chapter: bool,
        anchor: Option<&Anchor>,
        destination: Option<&Destination>,
    ) -> Option<(usize, bool)> {
        if let Some(target) = destination
            && let Some(label) = self
                .exact
                .get(&(target.path.as_str(), target.fragment.as_deref()))
        {
            return Some((*label, true));
        }
        if chapter == book.reading.chapter {
            let top = if starts_chapter {
                0.0
            } else {
                book.reading.probe.anchor_top(anchor?)?
            };
            if let Some(index) = self
                .current
                .partition_point(|&(y, _)| y <= top)
                .checked_sub(1)
            {
                let (start, label) = self.current[index];
                return Some((label, start == top));
            }
        } else if !starts_chapter || *self.has_markers.get(chapter)? {
            // The order of this unopened fragment relative to local print
            // boundaries is unknown. An unopened chapter's marker may also
            // share its opening line, so require an exact match in that case.
            return None;
        }
        Some((self.preceding.get(chapter).copied().flatten()?, false))
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
