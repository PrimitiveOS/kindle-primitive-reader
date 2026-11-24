//! Inspect a source image independently of the book's page and reading controls.
use super::{CAPTION, Canvas, Change, Input, Reader, Result, Screen, bookmark_browser};
use crate::{epub::Book, fixed_session::FixedReading};
use primitive_app::with_storage;
use std::io;

impl Reader {
    pub(super) fn open_illustration_view(&mut self, mut view: FixedReading) -> Result<()> {
        let target = self.reading_viewport();
        if view.viewport() != target {
            let prepared = view.prepare_viewport(target)?;
            view.commit_viewport(prepared)?;
        }
        self.show(Screen::Illustration);
        self.illustration = Some(view);
        Ok(())
    }

    pub(super) fn view_cover(&mut self) -> Result<()> {
        let details = self.book_details.as_ref().ok_or("Choose a book first")?;
        let image = with_storage(|| {
            if self.library.identify(&details.entry)? != details.id {
                return Err(io::Error::other(
                    "The publication changed. Reopen its book details.",
                ));
            }
            let path = self.library.absolute_path(&details.entry.path)?;
            let mut book = Book::open(&path).map_err(io::Error::other)?;
            let cover = book
                .cover()
                .cloned()
                .ok_or_else(|| io::Error::other("The book has no supported cover"))?;
            let (_, bytes) = book
                .read_cover()
                .map_err(io::Error::other)?
                .ok_or_else(|| io::Error::other("The book has no supported cover"))?;
            if cover.kind == crate::epub::CoverKind::Svg {
                return crate::svg_image::original_package_image(&mut book, &cover.path, &bytes)
                    .map_err(io::Error::other);
            }
            crate::parley_layout::original_illustration(&bytes).map_err(io::Error::other)
        })?;
        self.open_illustration_view(FixedReading::inspect_image(image)?)
    }

    pub(super) fn inspect_illustration(&mut self, x: f32, y: f32) -> Result<bool> {
        let Some(book) = &self.active else {
            return Ok(false);
        };
        let Some(path) = book.reading.illustration_at(x, y) else {
            return Ok(false);
        };
        let image = book.reading.open_illustration(path)?;
        self.open_illustration_view(FixedReading::inspect_image(image)?)?;
        Ok(true)
    }

    pub(super) fn draw_illustration(&self, canvas: &mut Canvas) -> bool {
        let Some(illustration) = &self.illustration else {
            return false;
        };
        let title = if self.book_details.is_some() {
            "Book cover"
        } else if self.fixed.is_some() {
            "Comic panel"
        } else {
            "Illustration"
        };
        let notice = self.notice.as_deref().unwrap_or(title);
        let mut paint = self.page_paint();
        if self.book_details.is_none()
            && self
                .active
                .as_ref()
                .is_some_and(|active| !active.reading.config.settings.invert_night_images)
        {
            paint.inverted = false;
        }
        let status = illustration
            .viewport()
            .caption(illustration.zoom_buttons().0.x.saturating_sub(60), 28);
        illustration.draw_with_footer(
            canvas,
            Some(notice),
            &[],
            &[],
            paint,
            false,
            status,
            CAPTION,
            false,
        );
        true
    }

    pub(super) fn illustration_input(&mut self, input: Input) -> Change {
        let Some(view) = &mut self.illustration else {
            return Change::None;
        };
        if matches!(input, Input::Tap(x, y) if view.viewport().done().contains(x, y)) {
            let destination = match &self.book_details {
                Some(details)
                    if details.back == Screen::BookmarkBrowser(bookmark_browser::Page::Book) =>
                {
                    details.back
                }
                Some(_) => Screen::BookDetails,
                None => self.surface_screen(),
            };
            self.show(destination);
            return Change::Partial;
        }
        let (smaller, larger) = view.zoom_buttons();
        let result = match input {
            Input::Tap(x, y) if smaller.contains(x, y) => view.zoom_inspection(-1),
            Input::Tap(x, y) if larger.contains(x, y) => view.zoom_inspection(1),
            _ => view.input(input),
        };
        match result {
            Ok(true) => Change::Partial,
            Ok(false) => Change::None,
            Err(error) => self.report(error),
        }
    }
}

#[cfg(test)]
mod tests;
