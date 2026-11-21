//! Stable page numbering from the source edition, independent of screen geometry.
use super::{
    Action, ActiveBook, Change, Item, Reader, SEARCH_DELAY, SEARCH_REFRESH, Screen, position,
};
use crate::{Result, epub::Book, library::DocumentFormat, synthetic_pages::CharacterPages};
use primitive_app::with_storage;
use std::{borrow::Cow, io, path::PathBuf, time::Instant};

pub(super) struct Scan {
    map: CharacterPages,
    pub(super) deadline: Instant,
    refresh: Instant,
}

impl ActiveBook {
    pub(super) fn page_labels(&self) -> usize {
        self.numbering
            .as_ref()
            .map_or(self.reading.probe.printed_pages.len(), |map| map.count)
    }

    pub(super) fn page_label(&self, row: usize) -> Option<Cow<'_, str>> {
        if let Some(map) = &self.numbering {
            return (row < map.count).then(|| Cow::Owned((row + 1).to_string()));
        }
        Some(Cow::Borrowed(
            &self.reading.probe.printed_pages.get(row)?.label,
        ))
    }
}

impl Reader {
    pub(super) fn numbering_path(&self, id: u64) -> PathBuf {
        self.numbering_directory.join(format!("{id:016x}.json"))
    }

    pub(super) fn numbering_items(&self) -> Vec<Item> {
        let Some(book) = &self.active else {
            return Vec::new();
        };
        let mut items = Vec::new();
        if let Some(scan) = &self.numbering_scan {
            items.push(
                Item::new("Numbering chapters", Action::Heading).detail(format!(
                    "{} of {} · {} characters per page",
                    scan.map.chapters.len(),
                    book.reading.probe.chapter_count(),
                    scan.map.stride
                )),
            );
        }
        let published = book.reading.probe.printed_pages.len();
        items.push(
            Item::new(
                if published > 0 {
                    "Publisher page numbers"
                } else {
                    "Screen page numbers"
                },
                Action::SetCharacterPages(None),
            )
            .detail(if published > 0 {
                format!("{published} labels supplied by this edition")
            } else {
                "Follow the current text layout".into()
            }),
        );
        let stride = book.numbering.as_ref().map_or(1500, |m| m.stride);
        items.push(
            Item::new(
                "Character-count pages",
                Action::SetCharacterPages(Some(stride)),
            )
            .detail(format!("{stride} characters per stable page")),
        );
        items.push(
            Item::new("Characters per page", Action::Show(Screen::CharacterStride))
                .detail("Choose a value from 500 to 3,000"),
        );
        items
    }

    pub(super) fn restore_character_pages(&mut self) {
        let Some(active) = &self.active else { return };
        let Some(stride) = self
            .library
            .state(active.id)
            .and_then(|s| s.character_page_stride)
            .filter(|n| (500..=3000).contains(n))
        else {
            return;
        };
        if active.entry.format != DocumentFormat::Epub {
            return;
        }
        if let Some(map) = CharacterPages::cached(
            &self.numbering_path(active.id),
            active.reading.probe.book,
            active.reading.probe.chapter_count(),
            stride,
        ) {
            self.active.as_mut().unwrap().numbering = Some(map);
        } else if let Err(error) = self.start_numbering(stride) {
            self.report(error);
        }
    }

    pub(super) fn set_character_pages(&mut self, stride: Option<u16>) -> Result<()> {
        let active = self.active.as_ref().ok_or("Open a reflowable EPUB")?;
        if active.entry.format != DocumentFormat::Epub {
            return Err("Open a reflowable EPUB".into());
        }
        if let Some(stride) = stride {
            if active
                .numbering
                .as_ref()
                .is_some_and(|m| m.stride == stride)
            {
                self.numbering_scan = None;
                self.show(Screen::PublisherPages);
                return Ok(());
            }
            self.start_numbering(stride)?;
            self.show(Screen::PageNumbering);
        } else {
            self.remember_numbering(active.id, None)?;
            self.numbering_scan = None;
            let active = self.active.as_mut().unwrap();
            active.reading.probe.publisher_positions.take();
            active.numbering = None;
            self.show(Screen::Book);
        }
        Ok(())
    }

    fn start_numbering(&mut self, stride: u16) -> Result<()> {
        let active = self.active.as_ref().ok_or("Open a reflowable EPUB")?;
        let map = CharacterPages::new(active.reading.probe.book, stride)?;
        let now = Instant::now();
        self.numbering_scan = Some(Scan {
            map,
            deadline: now + SEARCH_DELAY,
            refresh: now,
        });
        Ok(())
    }

    pub(super) fn step_character_pages(&mut self, now: Instant) -> Result<Change> {
        let Some(scan) = &mut self.numbering_scan else {
            return Ok(Change::None);
        };
        let active = self.active.as_ref().ok_or("Open a reflowable EPUB")?;
        let id = active.id;
        let finished = with_storage(|| {
            // Drop the archive before returning to the event loop so USB can own the volume.
            let mut book = Book::open(&active.reading.config.book).map_err(io::Error::other)?;
            scan.map.step(&mut book).map_err(io::Error::other)
        })?;
        if !finished {
            let now = now.max(Instant::now());
            scan.deadline = now + SEARCH_DELAY;
            if self.screen == Screen::PageNumbering && now >= scan.refresh {
                scan.refresh = now + SEARCH_REFRESH;
                return Ok(Change::Partial);
            }
            return Ok(Change::None);
        }
        let map = self.numbering_scan.take().unwrap().map;
        map.save(&self.numbering_path(id))?;
        self.remember_numbering(id, Some(map.stride))?;
        let active = self.active.as_mut().unwrap();
        active.reading.probe.publisher_positions.take();
        active.numbering = Some(map);
        if self.screen == Screen::PageNumbering {
            self.show(Screen::PublisherPages);
        }
        Ok(Change::Partial)
    }

    fn remember_numbering(&mut self, id: u64, stride: Option<u16>) -> Result<()> {
        let state = self
            .library
            .state_mut(id)
            .ok_or("Reading record unavailable")?;
        let previous = state.character_page_stride;
        state.character_page_stride = stride;
        self.mark_dirty();
        if let Err(err) = self.save_library() {
            self.library.state_mut(id).unwrap().character_page_stride = previous;
            return Err(err.into());
        }
        Ok(())
    }

    pub(super) fn follow_character_page(&mut self, row: usize) -> Result<()> {
        let active = self.active.as_mut().ok_or("No open book")?;
        let map = active
            .numbering
            .as_ref()
            .ok_or("Character-count pages are unavailable")?;
        let (chapter, point) = map
            .locate(row)
            .ok_or("That stable page is outside the edition")?;
        let previous = position(&active.reading);
        if let Some(point) = point {
            let href = &active.reading.probe.chapter_entries()[chapter].href;
            let anchor = map
                .anchor(chapter, point, href)
                .ok_or("Stable page source unavailable")?;
            active.reading.jump_to_anchor(chapter, &anchor)?;
        } else {
            active.reading.jump(chapter, None)?;
        }
        self.finish_link_jump(previous);
        Ok(())
    }
}
