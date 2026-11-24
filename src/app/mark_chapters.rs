//! Snapshot chapter names at the source position where a saved mark begins.
use super::search_chapter_scope::{Destination, source_offsets};
use super::{Reader, ReadingPosition};
use crate::epub::Book;
use crate::generated_contents::OutlineTarget;
use crate::library::clean_mark_chapter_title;
use crate::{Anchor, Probe};
use std::collections::BTreeMap;
use std::io;

type Offset = (usize, usize);
type Headings<'a> = BTreeMap<usize, Vec<(Destination<'a>, &'a str)>>;

const MAX_NEARBY_SPINE_ITEMS: usize = 16;

impl Reader {
    pub(super) fn bookmark_chapter_title(
        &self,
        position: &ReadingPosition,
        probe: Option<&Probe>,
    ) -> Option<String> {
        match probe {
            Some(probe) => self.reflow_chapter_title(
                probe,
                position.chapter,
                position.page,
                position.anchor.as_ref(),
            ),
            None => self.fixed_chapter_title(position.page),
        }
    }

    pub(super) fn highlight_chapter_title(&self, anchor: &Anchor) -> Option<String> {
        let reading = &self.active.as_ref()?.reading;
        let chapter = reading.probe.chapter_index(&anchor.chapter)?;
        let remote;
        let probe = if chapter == reading.chapter {
            &reading.probe
        } else {
            remote = reading.config.chapter(chapter).ok()?;
            &remote
        };
        self.reflow_chapter_title(probe, chapter, probe.page_for(anchor)?, Some(anchor))
    }

    pub(super) fn fixed_chapter_title(&self, page: usize) -> Option<String> {
        let fixed = self.fixed.as_ref()?;
        let state = self.library.state(fixed.id)?;
        let mut previous = None;
        let mut on_page = None;
        let mut consider = |heading, title| {
            if heading == page && on_page.is_none() {
                on_page = Some(title);
            } else if heading < page && previous.is_none_or(|(prior, _)| prior <= heading) {
                previous = Some((heading, title));
            }
        };
        let included =
            |depth| !state.contents_depths.bind_title || state.contents_depths.includes(depth);
        if state.custom_contents.enabled {
            if included(0) {
                for mark in state.custom_contents.entries() {
                    if mark.position.chapter == 0
                        && mark.position.anchor.is_none()
                        && mark.position.page < fixed.reading.count()
                    {
                        consider(mark.position.page, mark.label.as_str());
                    }
                }
            }
        } else {
            for heading in fixed.reading.chapter_outline() {
                if included(heading.depth)
                    && let Some(start) = heading.page.filter(|&start| start < fixed.reading.count())
                {
                    consider(start, heading.title.as_str());
                }
            }
        }
        on_page
            .or_else(|| previous.map(|(_, title)| title))
            .and_then(clean_mark_chapter_title)
    }

    fn mark_headings(&self) -> Option<Headings<'_>> {
        let active = self.active.as_ref()?;
        let state = self.library.state(active.id)?;
        let included =
            |depth| !state.contents_depths.bind_title || state.contents_depths.includes(depth);
        let mut headings = Headings::new();
        let mut add = |chapter, destination, title| {
            if chapter < active.reading.probe.chapter_count() {
                headings
                    .entry(chapter)
                    .or_default()
                    .push((destination, title));
            }
        };
        if state.custom_contents.enabled {
            if included(0) {
                for mark in state.custom_contents.entries() {
                    add(
                        mark.position.chapter,
                        Destination::Custom(&mark.position),
                        &mark.label,
                    );
                }
            }
        } else if let Some(entries) = self.alternative_entries(active.id) {
            for entry in entries.iter().filter(|entry| included(entry.depth)) {
                let (chapter, target) = match &entry.target {
                    OutlineTarget::Chapter(chapter) => (*chapter, Destination::Publisher(None)),
                    OutlineTarget::Heading(chapter, anchor) => {
                        (*chapter, Destination::Alternative(anchor))
                    }
                };
                add(chapter, target, &entry.title);
            }
        } else {
            let probe = &active.reading.probe;
            for entry in probe.toc.iter().filter(|entry| included(entry.depth)) {
                if let Some(target) = &entry.destination
                    && let Some(chapter) = probe.chapter_index(&target.path)
                {
                    add(
                        chapter,
                        Destination::Publisher(target.fragment.as_deref()),
                        &entry.label,
                    );
                }
            }
        }
        Some(headings)
    }

    fn reflow_chapter_title(
        &self,
        probe: &Probe,
        chapter: usize,
        page: usize,
        mark: Option<&Anchor>,
    ) -> Option<String> {
        let headings = self.mark_headings()?;
        let local: Vec<_> = headings
            .get(&chapter)
            .into_iter()
            .flatten()
            .filter_map(|&(target, title)| {
                let (page, offset) = layout_position(probe, target)?;
                Some((page, offset, title))
            })
            .collect();
        let initial = local
            .iter()
            .find(|&&(start, _, _)| start == page)
            .or_else(|| {
                local
                    .iter()
                    .filter(|&&(start, _, _)| start < page)
                    .max_by_key(|&&(start, _, _)| start)
            });
        let previous_spine = || self.remote_mark_heading(&headings, chapter, true);
        let Some(&(_, initial_offset, initial_title)) = initial else {
            return previous_spine().and_then(clean_mark_chapter_title);
        };
        let Some(mark) = mark.and_then(|anchor| source_order(probe, anchor)) else {
            return clean_mark_chapter_title(initial_title);
        };
        let nearest = local
            .iter()
            .filter(|&&(_, offset, _)| offset <= mark)
            .max_by_key(|&&(_, offset, _)| offset)
            .map(|&(_, _, title)| title);

        // Stock begins with the first heading on the marked page, or its
        // closest preceding page heading. Its XPointer scan returns the
        // nearest source title if it finds a heading after the mark. At
        // the end of the outline it retains that initial page-based title.
        let title = if mark < initial_offset {
            nearest.or_else(previous_spine).unwrap_or(initial_title)
        } else if nearest.is_some_and(|title| title != initial_title)
            && (local.iter().any(|&(_, offset, _)| offset > mark)
                || self
                    .remote_mark_heading(&headings, chapter, false)
                    .is_some())
        {
            nearest.unwrap_or(initial_title)
        } else {
            initial_title
        };
        clean_mark_chapter_title(title)
    }

    fn remote_mark_heading<'a>(
        &self,
        headings: &Headings<'a>,
        chapter: usize,
        before: bool,
    ) -> Option<&'a str> {
        let nearby: Vec<_> = if before {
            headings
                .range(..chapter)
                .rev()
                .take(MAX_NEARBY_SPINE_ITEMS)
                .collect()
        } else {
            headings
                .range(chapter.saturating_add(1)..)
                .take(MAX_NEARBY_SPINE_ITEMS)
                .collect()
        };
        if nearby.is_empty() {
            return None;
        }
        let reading = &self.active.as_ref()?.reading;
        primitive_app::with_storage(|| {
            let mut publication = Book::open(&reading.config.book).map_err(io::Error::other)?;
            for (&chapter, entries) in nearby {
                let targets: Vec<_> = entries.iter().map(|&(target, _)| target).collect();
                let offsets = source_offsets(reading, &mut publication, chapter, &targets)?;
                let chosen = if before {
                    offsets.max_by_key(|&(_, offset)| offset)
                } else {
                    offsets.min_by_key(|&(_, offset)| offset)
                };
                if let Some((index, _)) = chosen {
                    return Ok(Some(entries[index].1));
                }
            }
            Ok(None)
        })
        .ok()
        .flatten()
    }
}

pub(super) fn location_caption(mut location: String, title: Option<&str>) -> String {
    if let Some(title) = title {
        location.push_str(" · ");
        location.push_str(title);
    }
    location
}

fn source_order(probe: &Probe, anchor: &Anchor) -> Option<Offset> {
    let (index, offset) = probe.project_anchor(anchor)?;
    Some((probe.source.blocks[index].key, offset))
}

fn layout_position(probe: &Probe, destination: Destination<'_>) -> Option<(usize, Offset)> {
    let anchor = match destination {
        Destination::Publisher(None) => return Some((0, (0, 0))),
        Destination::Publisher(Some(fragment)) => probe.fragment(fragment)?,
        Destination::Alternative(anchor) => anchor.clone(),
        Destination::Custom(position) => match &position.anchor {
            Some(anchor) => anchor.clone(),
            None if position.page == 0 => return Some((0, (0, 0))),
            None => probe.page_anchor(position.page)?,
        },
    };
    Some((probe.page_for(&anchor)?, source_order(probe, &anchor)?))
}

#[cfg(test)]
mod tests;
