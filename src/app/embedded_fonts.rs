//! Keep the document font resource switch independent of publisher CSS and family overrides.
use super::{ActiveBook, Reader};
use crate::{book_style::BookStyle, epub::Book};
use primitive_app::with_storage;
use std::io;

mod report;
pub(super) use report::Inspection;

#[derive(Clone, Copy)]
pub(super) enum Availability {
    Found,
    MissingAt(usize),
}

impl ActiveBook {
    pub(super) fn has_embedded_fonts(&self) -> bool {
        !self.reading.probe.embedded_font_usage.is_empty()
            || matches!(self.embedded_font_inventory, Some(Availability::Found))
    }
}

impl Reader {
    pub(super) fn prepare_embedded_font_inventory(&mut self) {
        let Some(active) = &mut self.active else {
            return;
        };
        if !active.reading.config.settings.publisher_styles {
            return;
        }
        if active.has_embedded_fonts() {
            active.embedded_font_inventory = Some(Availability::Found);
            return;
        }
        let chapter = active.reading.chapter;
        if matches!(active.embedded_font_inventory, Some(Availability::MissingAt(previous)) if previous == chapter)
        {
            return;
        }
        let initial_check = active.embedded_font_inventory.is_none();
        if let Ok(available) = with_storage(|| {
            let mut book = Book::open(&active.reading.config.book)
                .map_err(|error| io::Error::other(error.to_string()))?;
            Ok(if initial_check {
                BookStyle::publication_has_embedded_fonts(&mut book, chapter)
            } else {
                BookStyle::chapter_has_embedded_fonts(&mut book, chapter)
            })
        }) {
            active.embedded_font_inventory = Some(if available {
                Availability::Found
            } else {
                Availability::MissingAt(chapter)
            });
        }
    }
}

#[cfg(test)]
mod tests;
