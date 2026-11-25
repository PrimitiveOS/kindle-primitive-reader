//! Keep reflow header activation with the book and its presentation in Reader settings.
use std::{path::Path, time::Instant};

use primitive_app::{Change, HEIGHT, WIDTH};
use primitive_ui::{BODY, Canvas, Input, Rect, Sheet, SheetAction, TextStyle};

use super::{Action, Item, Reader, Result};
use crate::koreader_sidecar::custom_metadata;
use crate::reading_controls::{HeaderBattery, ReadingControls, TopHeader};

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

mod identity;
pub(super) use identity::IdentityField;

pub(super) struct Metadata {
    pub(super) title: Option<String>,
    pub(super) author: Option<String>,
}

impl Reader {
    pub(super) fn load_header_metadata(&self, path: &Path) -> Metadata {
        let custom = primitive_app::with_storage(|| self.library.koreader_custom_metadata(path))
            .ok()
            .flatten()
            .and_then(|bytes| custom_metadata::parse(&bytes).ok())
            .unwrap_or_default();
        Metadata {
            title: custom.title.map(|value| value.replace(['\n', '\r'], " ")),
            author: custom.authors.map(|value| value.replace(['\n', '\r'], " ")),
        }
    }

    pub(super) fn header_battery_items(&self) -> Vec<Item> {
        let header = self.controls.top_header;
        let language = self.controls.interface_language;
        [HeaderBattery::Icon, HeaderBattery::Percentage]
            .into_iter()
            .map(|choice| {
                let selected = header.battery == choice;
                Item::new(
                    language.status_bar_settings_text(choice.label()),
                    Action::SetReadingControls(ReadingControls {
                        top_header: TopHeader {
                            battery: if selected { HeaderBattery::Off } else { choice },
                            ..header
                        },
                        ..self.controls
                    }),
                )
                .detail(language.status_bar_settings_text(if selected {
                    "On"
                } else {
                    "Off"
                }))
            })
            .collect()
    }

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
        let current = self.active.as_ref().ok_or("Open a reflowable book first")?;
        let metadata = enabled.then(|| self.load_header_metadata(current.entry.library_path()));
        let book = self.active.as_mut().ok_or("Open a reflowable book first")?;
        let state = self
            .library
            .state_mut(book.id)
            .ok_or("Reading state unavailable")?;
        book.reading
            .set_status_header_height(TopHeader::height(enabled, book.reading.is_continuous()))?;
        book.header_metadata = metadata;
        state.top_status_header = Some(enabled);
        self.record_position();
        self.update_footer_clock(Instant::now());
        Ok(())
    }

    pub(super) fn draw_top_header_about(&self, canvas: &mut Canvas) {
        if !self.top_header_about {
            return;
        }
        let language = self.controls.interface_language;
        canvas.popup(ABOUT);
        canvas.text(
            ABOUT_TITLE,
            language.status_bar_settings_text("About the top status header"),
            TextStyle::new(50).bold(),
        );
        canvas.text(
            ABOUT_BEHAVIOR,
            language.status_bar_settings_text(ABOUT_BEHAVIOR_TEXT),
            BODY.lines(4),
        );
        canvas.text(
            ABOUT_SETUP,
            language.status_bar_settings_text(ABOUT_SETUP_TEXT),
            BODY.lines(4),
        );
        canvas.button(
            ABOUT_DONE,
            language.status_bar_settings_text("Done"),
            true,
            false,
        );
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
            let language = self.controls.interface_language;
            let sheet = Sheet::centered(
                Rect::new(0, 0, WIDTH as i32, HEIGHT as i32),
                language.status_bar_settings_text("Default top status header"),
                language.status_bar_settings_text(if enabled {
                    "Show the top status header when you open a new book in page-turn mode?"
                } else {
                    "Hide the top status header when you open a new book?"
                }),
                language.status_bar_settings_text("Set as default"),
            );
            sheet.draw(canvas, None);
            if !language.is_english() {
                canvas.button(
                    sheet.button(SheetAction::Cancel),
                    language.status_bar_settings_text("Cancel"),
                    false,
                    false,
                );
            }
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
