//! Edition-level alternative contents. Only bounded source labels and anchors are kept.
use super::{Action, CAPTION, Canvas, Change, Input, Item, Reader, Rect, SEARCH_DELAY, Screen};
use crate::generated_contents::{GeneratedContents, OutlineEntry};
use crate::{Result, epub::Book};
use primitive_app::with_storage;
use primitive_ui::TextStyle;
use std::{io, time::Instant};

const PANEL: Rect = Rect::new(152, 622, 960, 426);
const CANCEL: Rect = Rect::new(216, 898, 376, 100);
const APPLY: Rect = Rect::new(672, 898, 376, 100);

#[derive(Default)]
pub(super) struct AlternativeContents {
    book: Option<u64>,
    outline: GeneratedContents,
    deadline: Option<Instant>,
    pub(super) confirm: Option<bool>,
    failed: bool,
}

impl Reader {
    pub(super) fn alternative_contents_setting(&self) -> Option<Item> {
        self.active.as_ref()?;
        let custom = self.custom_contents_enabled();
        Some(
            Item::new(
                "Alternative table of contents",
                if custom {
                    Action::Heading
                } else {
                    Action::ToggleAlternativeContents
                },
            )
            .detail(if custom {
                "Disable custom contents to switch"
            } else if self.uses_alternative_contents() {
                "Document headings · On"
            } else {
                "Publisher contents · Off"
            }),
        )
    }

    pub(super) fn uses_alternative_contents(&self) -> bool {
        self.active
            .as_ref()
            .and_then(|active| self.library.state(active.id))
            .is_some_and(|state| state.alternative_toc && !state.custom_contents.enabled)
    }

    pub(super) fn pending_alternative_contents(&self) -> bool {
        self.uses_alternative_contents() && self.alternative_contents.deadline.is_some()
    }

    pub(super) fn failed_alternative_contents(&self) -> bool {
        self.uses_alternative_contents() && self.alternative_contents.failed
    }

    pub(super) fn alternative_entries(&self, book: u64) -> Option<&[OutlineEntry]> {
        let state = &self.alternative_contents;
        (self.uses_alternative_contents() && state.book == Some(book) && !state.failed).then_some(
            if state.deadline.is_some() {
                &[]
            } else {
                state.outline.entries.as_slice()
            },
        )
    }

    pub(super) fn sync_alternative_contents(&mut self) {
        let book = self.active.as_ref().map(|active| active.id).filter(|id| {
            self.library
                .state(*id)
                .is_some_and(|state| state.alternative_toc)
        });
        let want_markers = self.uses_alternative_contents()
            || self.publisher_filtered_marker_demand()
            || self.contents_chapter_lengths_demand();
        if let Some(active) = &mut self.active {
            active
                .reading
                .request_alternative_marker_pages(want_markers);
        }
        if self.alternative_contents.book != book {
            self.alternative_contents = AlternativeContents {
                book,
                outline: GeneratedContents::alternative(),
                deadline: book.map(|_| Instant::now() + SEARCH_DELAY),
                ..AlternativeContents::default()
            };
        }
    }

    pub(super) fn alternative_contents_deadline(&self) -> Option<Instant> {
        self.alternative_contents.deadline
    }

    pub(super) fn poll_alternative_contents(&mut self, now: Instant) -> Option<Change> {
        let scan = &mut self.alternative_contents;
        if scan.deadline.is_none_or(|at| at > now) {
            return None;
        }
        let active = self.active.as_ref()?;
        let result = with_storage(|| {
            let mut publication =
                Book::open(&active.reading.config.book).map_err(io::Error::other)?;
            scan.outline
                .step(&mut publication)
                .map_err(io::Error::other)
        });
        if matches!(result, Ok(false)) {
            scan.deadline = Some(now.max(Instant::now()) + SEARCH_DELAY);
            return Some(Change::None);
        }
        scan.deadline = None;
        scan.failed = result.is_err() || scan.outline.entries.is_empty();
        if self.screen.is_contents() {
            self.prepare_contents(true);
            self.list_page = self.current_contents_row.unwrap_or_default() / self.contents_rows();
            return Some(Change::Partial);
        }
        Some(if self.screen == Screen::Book {
            Change::Partial
        } else {
            Change::None
        })
    }

    pub(super) fn draw_alternative_contents_confirmation(&self, canvas: &mut Canvas) {
        let Some(enable) = self.alternative_contents.confirm else {
            return;
        };
        let layout = self.navigation_layout();
        let at = |rect| layout.centered(rect);
        canvas.popup(at(PANEL));
        canvas.text(
            at(Rect::new(218, 668, 824, 86)),
            if enable {
                "Use document headings?"
            } else {
                "Restore publisher contents?"
            },
            TextStyle::new(46).bold(),
        );
        canvas.text(
            at(Rect::new(218, 776, 824, 88)),
            self.notice.as_deref().unwrap_or(if enable {
                "Build the table of contents from this book's headings."
            } else {
                "Switch back to the table of contents supplied with this book."
            }),
            CAPTION.lines(2),
        );
        canvas.button(at(CANCEL), "Cancel", false, false);
        canvas.button(
            at(APPLY),
            if enable { "Use headings" } else { "Restore" },
            true,
            false,
        );
    }

    pub(super) fn alternative_contents_confirmation(&mut self, input: Input) -> Option<Change> {
        let enable = self.alternative_contents.confirm?;
        let layout = self.navigation_layout();
        let at = |rect| layout.centered(rect);
        if matches!(input, Input::Sleep | Input::Wake) {
            return None;
        }
        if !self.stats_awake {
            return Some(Change::None);
        }
        if matches!(input, Input::PageBack)
            || matches!(input, Input::Tap(x, y) if at(CANCEL).contains(x, y) || !at(PANEL).contains(x, y))
        {
            self.alternative_contents.confirm = None;
            return Some(Change::Partial);
        }
        if matches!(input, Input::Tap(x, y) if at(APPLY).contains(x, y)) {
            return Some(match self.switch_alternative_contents(enable) {
                Ok(()) => Change::Partial,
                Err(error) => self.report(error),
            });
        }
        Some(Change::None)
    }

    fn switch_alternative_contents(&mut self, enable: bool) -> Result<()> {
        let book = self
            .active
            .as_ref()
            .ok_or("Open a reflowable book first")?
            .id;
        self.library.set_alternative_contents(book, enable)?;
        self.alternative_contents.confirm = None;
        self.show(Screen::Contents);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
