//! Keep the document font resource switch independent of publisher CSS and family overrides.
use super::{ActiveBook, Reader};
use crate::{book_style::BookStyle, epub::Book};
use primitive_app::with_storage;
use std::io;

impl ActiveBook {
    pub(super) fn has_embedded_fonts(&self) -> bool {
        self.reading.probe.registered_embedded_fonts || self.embedded_font_inventory == Some(true)
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
        if active.reading.probe.registered_embedded_fonts {
            active.embedded_font_inventory = Some(true);
        }
        if active.embedded_font_inventory.is_some() {
            return;
        }
        active.embedded_font_inventory = with_storage(|| {
            let mut book = Book::open(&active.reading.config.book)
                .map_err(|error| io::Error::other(error.to_string()))?;
            Ok(BookStyle::publication_has_embedded_fonts(&mut book))
        })
        .ok();
    }
}

#[cfg(test)]
mod tests;
