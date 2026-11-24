//! Publisher geometry choices for the open EPUB and future books.
use super::{Action, Item, Reader, Result, Screen};
use crate::PublisherRenderMode;

impl Reader {
    pub(super) fn publisher_render_mode_unavailable(&self) -> Option<&'static str> {
        let Some(book) = &self.active else {
            return Some("Open a reflowable EPUB");
        };
        let reading = &book.reading;
        if book.entry.format != crate::library::DocumentFormat::Epub
            || reading.probe.pages_right_to_left()
        {
            Some("Requires a horizontal left-to-right EPUB")
        } else if reading.config.scroll
            || reading.config.facing_pages
            || reading.config.hidden_flows.enabled
        {
            Some("Requires one-page reading")
        } else if !reading.config.settings.publisher_styles {
            Some("Turn on Publisher CSS")
        } else {
            None
        }
    }

    pub(super) fn book_publisher_render_mode(&self) -> PublisherRenderMode {
        self.active
            .as_ref()
            .and_then(|book| self.library.state(book.id))
            .and_then(|book| book.preferences.publisher_render_mode)
            .unwrap_or_default()
    }

    pub(super) fn publisher_render_default_items(&self) -> Vec<Item> {
        let current = self
            .controls
            .publisher_render_mode_default
            .unwrap_or_default();
        let mut items = vec![
            Item::new("For new horizontal EPUBs", Action::Heading)
                .detail("Complex layouts may use Reader’s established layout"),
        ];
        items.extend(PublisherRenderMode::CHOICES.map(|mode| {
            let detail = if mode == current {
                format!("Selected · {}", mode.description())
            } else {
                mode.description().into()
            };
            Item::new(mode.label(), Action::SetPublisherRenderDefault(mode)).detail(detail)
        }));
        items
    }

    pub(super) fn set_book_publisher_render_mode(
        &mut self,
        mode: PublisherRenderMode,
    ) -> Result<()> {
        if let Some(reason) = self.publisher_render_mode_unavailable() {
            return Err(reason.into());
        }
        let mut preferences = self.current_text_preferences()?;
        preferences.publisher_render_mode = Some(mode);
        self.apply_text_profile(preferences)
    }

    pub(super) fn set_publisher_render_default(&mut self, mode: PublisherRenderMode) -> Result<()> {
        self.activate(Action::SetReadingControls(
            crate::reading_controls::ReadingControls {
                publisher_render_mode_default: (mode != PublisherRenderMode::Reader)
                    .then_some(mode),
                ..self.controls
            },
        ))?;
        self.show(Screen::PublisherRenderDefaults);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
