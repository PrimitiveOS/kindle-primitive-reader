//! Keep reflow header activation with the book and its presentation in Reader settings.
use std::time::Instant;

use primitive_app::{Change, HEIGHT, WIDTH};
use primitive_ui::{BODY, Canvas, Input, Rect, Sheet, SheetAction, TextStyle};

use super::{Action, Reader, Result};
use crate::reading_controls::{ReadingControls, TopHeader};

const ABOUT_WIDTH: i32 = 940;
const ABOUT_HEIGHT: i32 = 650;
const ABOUT: Rect = Rect::new(
    (WIDTH as i32 - ABOUT_WIDTH) / 2,
    (HEIGHT as i32 - ABOUT_HEIGHT) / 2,
    ABOUT_WIDTH,
    ABOUT_HEIGHT,
);
const ABOUT_TITLE: Rect = Rect::new(ABOUT.x + 48, ABOUT.y + 38, ABOUT.w - 96, 74);
const ABOUT_BEHAVIOR: Rect = Rect::new(ABOUT.x + 48, ABOUT.y + 128, ABOUT.w - 96, 150);
const ABOUT_SETUP: Rect = Rect::new(ABOUT.x + 48, ABOUT.y + 292, ABOUT.w - 96, 194);
const ABOUT_DONE: Rect = Rect::new(ABOUT.x + ABOUT.w - 328, ABOUT.y + 502, 280, 100);
const ABOUT_BEHAVIOR_TEXT: &str = "For reflowable books, the header can appear above the footer in page-turn mode. It hides in continuous scroll.";
const ABOUT_SETUP_TEXT: &str = "Swipe up from the bottom and open Screen. Turn on Top status header for this book or set Default for new books. Choose what it shows here.";

impl Reader {
    pub(super) fn top_header_for_book(&self, id: u64) -> bool {
        let header = self.controls.top_header;
        if let Some(state) = self.library.state(id) {
            if let Some(enabled) = state.top_status_header {
                return enabled;
            }
            if state.position.is_some() {
                return header.legacy_enabled;
            }
        }
        header.default_enabled()
    }

    pub(super) fn set_book_top_header(&mut self, enabled: bool) -> Result<()> {
        let book = self.active.as_mut().ok_or("Open a reflowable book first")?;
        let state = self
            .library
            .state_mut(book.id)
            .ok_or("Reading state unavailable")?;
        book.reading
            .set_status_header_height(TopHeader::height(enabled, book.reading.is_continuous()))?;
        state.top_status_header = Some(enabled);
        self.record_position();
        self.update_footer_clock(Instant::now());
        Ok(())
    }

    pub(super) fn draw_top_header_about(&self, canvas: &mut Canvas) {
        if !self.top_header_about {
            return;
        }
        canvas.popup(ABOUT);
        canvas.text(
            ABOUT_TITLE,
            "About the top status header",
            TextStyle::new(50).bold(),
        );
        canvas.text(ABOUT_BEHAVIOR, ABOUT_BEHAVIOR_TEXT, BODY.lines(4));
        canvas.text(ABOUT_SETUP, ABOUT_SETUP_TEXT, BODY.lines(4));
        canvas.button(ABOUT_DONE, "Done", true, false);
    }

    pub(super) fn top_header_about_input(&mut self, input: Input) -> Option<Change> {
        if !self.top_header_about || matches!(input, Input::Sleep | Input::Wake) {
            return None;
        }
        if matches!(input, Input::PageBack)
            || matches!(input, Input::Tap(x, y) if ABOUT_DONE.contains(x, y) || !ABOUT.contains(x, y))
        {
            self.top_header_about = false;
            return Some(Change::Partial);
        }
        Some(Change::None)
    }

    pub(super) fn top_header_default_sheet(enabled: bool) -> Sheet<'static> {
        Sheet::centered(
            Rect::new(0, 0, WIDTH as i32, HEIGHT as i32),
            "Default top status header",
            if enabled {
                "Show the top status header when you open a new book in page-turn mode?"
            } else {
                "Hide the top status header when you open a new book?"
            },
            "Set as default",
        )
    }

    pub(super) fn draw_top_header_default_prompt(&self, canvas: &mut Canvas) {
        if let Some(enabled) = self.top_header_default_prompt {
            Self::top_header_default_sheet(enabled).draw(canvas, None);
        }
    }

    pub(super) fn top_header_default_input(&mut self, input: Input) -> Option<Change> {
        let enabled = self.top_header_default_prompt?;
        let confirm = match input {
            Input::Sleep | Input::Wake => return None,
            Input::PageBack => false,
            Input::Tap(x, y) => match Self::top_header_default_sheet(enabled).hit(x, y) {
                Some(SheetAction::Cancel) => false,
                Some(SheetAction::Confirm) => true,
                None => return Some(Change::None),
            },
            _ => return Some(Change::None),
        };
        self.top_header_default_prompt = None;
        if confirm {
            let controls = ReadingControls {
                top_header: TopHeader {
                    book_default: Some(enabled),
                    ..self.controls.top_header
                },
                ..self.controls
            };
            if let Err(error) = self.activate(Action::SetReadingControls(controls)) {
                return Some(self.report(error));
            }
        }
        Some(Change::Partial)
    }
}

#[cfg(test)]
mod tests;
