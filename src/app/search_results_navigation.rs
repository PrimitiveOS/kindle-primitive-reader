//! Navigate retained document search results without changing the query or match ranges.
use std::cmp::Ordering;

use primitive_app::{Change, HEIGHT, WIDTH};
use primitive_ui::{CAPTION, Canvas, Input, Rect, TextStyle, icons};

use super::{HistoryLocation, Reader, Result, Screen, fixed_page_label};
use crate::library::FixedPreferences;

const OPTIONS: Rect = Rect::new(WIDTH as i32 - 384, 40, 100, 100);
const SHEET: Rect = Rect::new(
    (WIDTH as i32 - 864) / 2,
    (HEIGHT as i32 - 624) / 2,
    864,
    624,
);
const TITLE: Rect = Rect::new(SHEET.x + 48, SHEET.y + 38, SHEET.w - 96, 72);
const CURRENT: Rect = Rect::new(SHEET.x + 48, SHEET.y + 130, SHEET.w - 96, 104);
const CURRENT_LABEL: Rect = Rect::new(CURRENT.x, CURRENT.y + CURRENT.h + 6, CURRENT.w, 46);
const ORIGINAL: Rect = Rect::new(SHEET.x + 48, SHEET.y + 302, SHEET.w - 96, 104);
const ORIGINAL_LABEL: Rect = Rect::new(ORIGINAL.x, ORIGINAL.y + ORIGINAL.h + 6, ORIGINAL.w, 46);
const CLOSE: Rect = Rect::new(SHEET.x + SHEET.w - 278, SHEET.y + 476, 230, 100);

#[derive(Default)]
pub(super) struct Navigation {
    origin: Option<Origin>,
    pub(super) menu_open: bool,
}

struct Origin {
    book: u64,
    location: HistoryLocation,
    fixed_view: Option<FixedPreferences>,
    label: String,
}

fn at_page(order: impl Iterator<Item = (usize, Ordering)>) -> Option<usize> {
    let mut previous = None;
    for (index, relation) in order {
        match relation {
            Ordering::Equal => return Some(index),
            Ordering::Less => previous = Some(index),
            Ordering::Greater => return previous.or(Some(index)),
        }
    }
    previous
}

impl Reader {
    pub(super) fn remember_all_results_origin(&mut self) {
        let origin = self
            .open_book_id()
            .zip(self.current_history_location())
            .map(|(book, location)| Origin {
                book,
                location,
                fixed_view: self.fixed.as_ref().map(|fixed| fixed.reading.preferences()),
                label: self.search_result_page_label(),
            });
        self.search_results_navigation = Navigation {
            origin,
            menu_open: false,
        };
    }

    fn results_origin(&self) -> Option<&Origin> {
        let origin = self.search_results_navigation.origin.as_ref()?;
        (self.open_book_id() == Some(origin.book)).then_some(origin)
    }

    fn has_search_result_navigation(&self) -> bool {
        self.screen == Screen::Matches
            && self.results_origin().is_some()
            && (!self.hits.is_empty() || !self.pdf_hits.is_empty())
    }

    pub(super) fn draw_search_results_options(&self, canvas: &mut Canvas) {
        if !self.has_search_result_navigation() {
            return;
        }
        canvas.button(OPTIONS, "", false, false);
        let ink = if canvas.is_control_pressed(OPTIONS) {
            255
        } else {
            0
        };
        canvas.icon(
            Rect::new(OPTIONS.x + 27, OPTIONS.y + 27, 46, 46),
            icons::SETTINGS,
            ink,
        );
    }

    pub(super) fn draw_search_results_navigation(&self, canvas: &mut Canvas) {
        if !self.search_results_navigation.menu_open || self.screen != Screen::Matches {
            return;
        }
        let Some(origin) = self.results_origin() else {
            return;
        };
        canvas.popup(SHEET);
        canvas.text(TITLE, "Search result options", TextStyle::new(50).bold());
        canvas.button(CURRENT, "Focus results near current page", false, false);
        canvas.text(
            CURRENT_LABEL,
            &self.search_result_page_label(),
            CAPTION.center(),
        );
        canvas.button(ORIGINAL, "Return to original page", false, false);
        canvas.text(ORIGINAL_LABEL, &origin.label, CAPTION.center());
        canvas.button(CLOSE, "Done", true, false);
    }

    fn search_result_page_label(&self) -> String {
        if let Some(fixed) = &self.fixed {
            fixed_page_label(&fixed.reading, fixed.reading.page())
        } else if let Some((current, _)) = self.top_header_page_labels() {
            current
        } else if let Some(book) = &self.active {
            format!(
                "Chapter {} · Page {}",
                book.reading.chapter + 1,
                book.reading.page + 1
            )
        } else {
            String::new()
        }
    }

    pub(super) fn search_results_navigation_input(&mut self, input: Input) -> Option<Change> {
        if matches!(input, Input::Sleep | Input::Wake) {
            return None;
        }
        if !self.has_search_result_navigation() {
            self.search_results_navigation.menu_open = false;
            return None;
        }
        if !self.search_results_navigation.menu_open {
            if matches!(input, Input::Tap(x, y) if OPTIONS.contains(x, y)) {
                self.result_context = None;
                self.search_results_navigation.menu_open = true;
                return Some(Change::Partial);
            }
            return None;
        }
        let outcome = match input {
            Input::Tap(x, y) if CURRENT.contains(x, y) => self.focus_results_at_current_page(),
            Input::Tap(x, y) if ORIGINAL.contains(x, y) => self.go_to_search_origin(),
            Input::Tap(x, y) if CLOSE.contains(x, y) || !SHEET.contains(x, y) => Ok(()),
            Input::PageBack => Ok(()),
            _ => return Some(Change::None),
        };
        self.search_results_navigation.menu_open = false;
        Some(match outcome {
            Ok(()) => Change::Partial,
            Err(error) => self.report(error),
        })
    }

    fn go_to_search_origin(&mut self) -> Result<()> {
        let origin = self
            .results_origin()
            .ok_or("The original reading location is unavailable")?;
        let location = origin.location.clone();
        if let Some(view) = origin.fixed_view {
            self.close_fixed_search_fit()?;
            let reading = &mut self.fixed.as_mut().ok_or("No open document")?.reading;
            let page = location.position.page;
            if reading.page() != page || reading.preferences() != view {
                reading.restore_view_at(page, view)?;
            }
            self.finish_fixed_jump(page, None);
            Ok(())
        } else {
            self.go_to_location(&location)
        }
    }

    fn focus_results_at_current_page(&mut self) -> Result<()> {
        let index = if let Some(fixed) = &self.fixed {
            let page = fixed.reading.page();
            at_page(
                self.pdf_hits
                    .iter()
                    .enumerate()
                    .map(|(index, hit)| (index, hit.page.cmp(&page))),
            )
        } else {
            self.ensure_reflow_search_current()?;
            let reading = &self.active.as_ref().ok_or("Open a book first")?.reading;
            at_page(self.hits.iter().enumerate().filter_map(|(index, hit)| {
                let relation = match hit.chapter.cmp(&reading.chapter) {
                    Ordering::Equal => reading.probe.page_for(&hit.anchor)?.cmp(&reading.page),
                    order => order,
                };
                Some((index, relation))
            }))
        }
        .ok_or("No results near this page")?;
        self.go_to_match_list_index(index);
        Ok(())
    }

    fn go_to_match_list_index(&mut self, index: usize) {
        self.list_page = index / self.match_geometry().rows;
        self.matches_page = self.list_page;
        self.focused_result = Some(index);
    }
}

#[cfg(test)]
mod tests;
