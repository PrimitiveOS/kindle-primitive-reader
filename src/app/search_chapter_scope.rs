//! Filter a full-text result list by the active chapter without discarding
//! provider indices or laying out the rest of the book.
use std::collections::{BTreeMap, HashMap};
use std::io;
use std::ops::Range;

use super::contents_view::Target;
use super::{Reader, ReadingPosition, Result};
use crate::book_style::BookStyle;
use crate::epub::Book;
use crate::generated_contents::OutlineTarget;
use crate::session::Reading;

type Offset = (usize, usize);
type Location = (usize, Offset);

pub(super) struct SearchChapterScope {
    book_id: u64,
    bounds: Bounds,
}

enum Bounds {
    Fixed(Range<usize>),
    Reflow {
        start: Option<Location>,
        end: Option<Location>,
    },
}

impl SearchChapterScope {
    /// The index is into the full retained search results, including batches
    /// that arrive after the chapter filter was applied.
    pub(super) fn includes_result(&self, reader: &Reader, index: usize) -> bool {
        if reader.open_book_id() != Some(self.book_id) {
            return false;
        }
        match &self.bounds {
            Bounds::Fixed(pages) => reader
                .pdf_hits
                .get(index)
                .is_some_and(|hit| pages.contains(&hit.page)),
            Bounds::Reflow { start, end } => reader.hits.get(index).is_some_and(|hit| {
                let location = (hit.chapter, (hit.anchor.block, hit.anchor.offset));
                start.is_none_or(|start| location >= start) && end.is_none_or(|end| location < end)
            }),
        }
    }
}

impl Reader {
    pub(super) fn current_search_chapter(&self) -> Result<SearchChapterScope> {
        let book_id = self.open_book_id().ok_or("Open a document first")?;
        let state = self.library.state(book_id);
        let custom = state.map(|state| state.custom_contents.entries());
        let use_level = |depth| {
            state.is_none_or(|state| {
                !state.contents_depths.bind_title || state.contents_depths.includes(depth)
            })
        };
        let bounds = if let Some(fixed) = &self.fixed {
            let pages = self
                .contents_entries(book_id)
                .iter()
                .enumerate()
                .filter(|(_, entry)| use_level(entry.depth))
                .filter_map(|(index, entry)| match entry.target {
                    Target::Fixed(page) if fixed.reading.stable_outline_destination(index) => {
                        Some(page)
                    }
                    Target::Custom(index) => {
                        let position = &custom?.get(index)?.position;
                        (position.chapter == 0 && position.anchor.is_none())
                            .then_some(position.page)
                    }
                    _ => None,
                })
                .collect();
            Bounds::Fixed(fixed_range(
                fixed.reading.page(),
                fixed.reading.count(),
                pages,
            ))
        } else if let Some(active) = &self.active {
            let reading = &active.reading;
            // KOReader applies an empty title to every result when there is
            // no outline. The Contents screen's generated spine labels are
            // navigation fallbacks, not publisher or custom boundaries.
            let has_contents = self.alternative_entries(book_id).is_some()
                || !reading.probe.toc.is_empty()
                || state.is_some_and(|state| state.custom_contents.enabled);
            let entries = has_contents
                .then(|| self.contents_entries(book_id))
                .unwrap_or_default();
            let mut chapters = HashMap::new();
            for (index, entry) in reading.probe.chapter_entries().iter().enumerate() {
                chapters.entry(entry.href.as_str()).or_insert(index);
            }
            let mut local = Vec::new();
            let mut remote = BTreeMap::<usize, Vec<Destination<'_>>>::new();
            for entry in entries.iter().filter(|entry| use_level(entry.depth)) {
                let Some((chapter, destination)) = (match &entry.target {
                    Target::Reflow(target) => chapters.get(target.path.as_str()).map(|&chapter| {
                        (chapter, Destination::Publisher(target.fragment.as_deref()))
                    }),
                    Target::Alternative(OutlineTarget::Chapter(chapter)) => {
                        Some((*chapter, Destination::Publisher(None)))
                    }
                    Target::Alternative(OutlineTarget::Heading(chapter, anchor)) => {
                        Some((*chapter, Destination::Alternative(anchor)))
                    }
                    Target::Custom(index) => {
                        custom.and_then(|entries| entries.get(*index)).map(|entry| {
                            (entry.position.chapter, Destination::Custom(&entry.position))
                        })
                    }
                    _ => None,
                }) else {
                    continue;
                };
                if chapter == reading.chapter {
                    local.extend(local_position(reading, destination));
                } else if chapter < reading.probe.chapter_count() {
                    remote.entry(chapter).or_default().push(destination);
                }
            }

            let current = reading.page;
            let local_start = local
                .iter()
                .find(|(page, _)| *page == current)
                .or_else(|| {
                    local
                        .iter()
                        .filter(|(page, _)| *page < current)
                        .max_by_key(|(page, _)| *page)
                })
                .map(|(_, offset)| *offset);
            let local_end = local
                .into_iter()
                .filter_map(|(_, offset)| {
                    local_start
                        .is_none_or(|start| offset > start)
                        .then_some(offset)
                })
                .min();
            let start = local_start.map(|offset| (reading.chapter, offset));
            let end = local_end.map(|offset| (reading.chapter, offset));
            let (start, end) = remote_bounds(reading, &remote, start, end)?;
            Bounds::Reflow { start, end }
        } else {
            return Err("Open a document first".into());
        };
        Ok(SearchChapterScope { book_id, bounds })
    }
}

fn fixed_range(current: usize, count: usize, mut headings: Vec<usize>) -> Range<usize> {
    headings.retain(|&page| page < count);
    // Stable sorting preserves the outline order for headings sharing a page.
    headings.sort();
    let first = headings.partition_point(|&page| page < current);
    let selected = if headings.get(first) == Some(&current) {
        Some(first)
    } else {
        first.checked_sub(1)
    };
    let Some(index) = selected else {
        return 0..headings.first().copied().unwrap_or(count);
    };
    let page = headings[index];
    // The first heading owns its start page. The last heading at that page
    // owns subsequent pages up to the next distinct heading.
    let start = page + usize::from(index > 0 && headings[index - 1] == page);
    let end = match headings.get(index + 1) {
        Some(&next) if next == page => page + 1,
        Some(&next) => next,
        None => count,
    };
    start..end
}

#[derive(Clone, Copy)]
pub(super) enum Destination<'a> {
    Publisher(Option<&'a str>),
    Custom(&'a ReadingPosition),
    Alternative(&'a crate::Anchor),
}

fn local_position(reading: &Reading, destination: Destination<'_>) -> Option<(usize, Offset)> {
    let anchor = match destination {
        Destination::Publisher(None) => return Some((0, (0, 0))),
        Destination::Publisher(Some(fragment)) => reading.probe.fragment(fragment)?,
        Destination::Alternative(anchor) => anchor.clone(),
        Destination::Custom(position) => match &position.anchor {
            Some(anchor) => anchor.clone(),
            None if position.page == 0 => return Some((0, (0, 0))),
            None => reading.probe.page_anchor(position.page)?,
        },
    };
    let page = reading.probe.page_for(&anchor)?;
    Some((page, (anchor.block, anchor.offset)))
}

fn remote_bounds(
    reading: &Reading,
    destinations: &BTreeMap<usize, Vec<Destination<'_>>>,
    mut start: Option<Location>,
    mut end: Option<Location>,
) -> Result<(Option<Location>, Option<Location>)> {
    let before = destinations.range(..reading.chapter).rev();
    let after = destinations.range(reading.chapter..);
    if (start.is_some() || before.clone().next().is_none())
        && (end.is_some() || after.clone().next().is_none())
    {
        return Ok((start, end));
    }
    primitive_app::with_storage(|| {
        let mut publication = reading.config.open_book().map_err(io::Error::other)?;
        if start.is_none() {
            for (&chapter, entries) in before {
                if let Some(offset) = source_offsets(reading, &mut publication, chapter, entries)?
                    .map(|(_, offset)| offset)
                    .max()
                {
                    start = Some((chapter, offset));
                    break;
                }
            }
        }
        if end.is_none() {
            for (&chapter, entries) in after {
                if let Some(offset) = source_offsets(reading, &mut publication, chapter, entries)?
                    .map(|(_, offset)| offset)
                    .min()
                {
                    end = Some((chapter, offset));
                    break;
                }
            }
        }
        Ok((start, end))
    })
    .map_err(Into::into)
}

pub(super) fn source_offsets(
    reading: &Reading,
    publication: &mut Book,
    chapter: usize,
    entries: &[Destination<'_>],
) -> io::Result<impl Iterator<Item = (usize, Offset)>> {
    let needs_source = entries.iter().any(|entry| {
        matches!(
            entry,
            Destination::Publisher(Some(_)) | Destination::Alternative(_)
        ) || matches!(entry, Destination::Custom(position) if position.anchor.is_some())
    });
    // The same font-free cascade as the full-text worker excludes hidden
    // fragment owners. This never registers fonts or paginates the chapter.
    let source = if needs_source {
        let href = publication
            .chapters
            .get(chapter)
            .ok_or_else(|| io::Error::other("The chapter outline has changed"))?
            .href
            .clone();
        let xml = publication.chapter(chapter).map_err(io::Error::other)?;
        let source = publication
            .chapter_source(&href, &xml)
            .map_err(io::Error::other)?;
        let config = &reading.config;
        let styles = BookStyle::load_for_flow_with_base(
            publication,
            &href,
            &source,
            config.settings.publisher_styles,
            &config.user_styles,
            config.settings.base_stylesheet,
        )
        .map_err(io::Error::other)?
        .compute(&source.elements);
        Some((source, styles))
    } else {
        None
    };
    let offset = |entry: &Destination<'_>| match entry {
        Destination::Publisher(None) => Some((0, 0)),
        Destination::Publisher(Some(fragment)) => {
            let (source, styles) = source.as_ref()?;
            let owner = source.fragment_owner(fragment)?;
            if styles.get(owner)?.flow.suppressed {
                return None;
            }
            source.fragments.get(*fragment).copied()
        }
        Destination::Alternative(anchor) => {
            let (source, styles) = source.as_ref()?;
            let block = source
                .blocks
                .iter()
                .find(|block| block.key == anchor.block)?;
            (anchor.book == publication.revision()
                && anchor.revision == source.revision
                && !styles.get(block.element)?.flow.suppressed)
                .then_some((anchor.block, anchor.offset))
        }
        Destination::Custom(position) => match &position.anchor {
            Some(anchor) => {
                let (source, _) = source.as_ref()?;
                let valid = source.revision == anchor.revision
                    && source.blocks.iter().any(|block| {
                        block.key == anchor.block && block.text.is_char_boundary(anchor.offset)
                    });
                valid.then_some((anchor.block, anchor.offset))
            }
            None => (position.page == 0).then_some((0, 0)),
        },
    };
    let offsets: Vec<_> = entries
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| Some((index, offset(entry)?)))
        .collect();
    Ok(offsets.into_iter())
}

#[cfg(test)]
pub(super) mod tests;
