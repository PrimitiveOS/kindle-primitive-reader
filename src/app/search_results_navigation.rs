//! Navigate retained document search results without changing the query or match ranges.
use std::cell::{Ref, RefCell};
use std::cmp::Ordering;

use primitive_app::{Change, HEIGHT, WIDTH};
use primitive_ui::{CAPTION, Canvas, Input, Rect, TextStyle, icons};

use super::search_chapter_scope::SearchChapterScope;
use super::{HistoryLocation, Reader, Result, Screen, fixed_page_label};
use crate::library::FixedPreferences;

const OPTIONS: Rect = Rect::new(WIDTH as i32 - 384, 40, 100, 100);
const SHEET: Rect = Rect::new(
    (WIDTH as i32 - 864) / 2,
    (HEIGHT as i32 - 864) / 2,
    864,
    864,
);
const TITLE: Rect = Rect::new(SHEET.x + 48, SHEET.y + 38, SHEET.w - 96, 72);
const ALL: Rect = Rect::new(SHEET.x + 48, SHEET.y + 130, SHEET.w - 96, 104);
const CHAPTER: Rect = Rect::new(SHEET.x + 48, SHEET.y + 250, SHEET.w - 96, 104);
const CURRENT: Rect = Rect::new(SHEET.x + 48, SHEET.y + 370, SHEET.w - 96, 104);
const CURRENT_LABEL: Rect = Rect::new(CURRENT.x, CURRENT.y + CURRENT.h + 6, CURRENT.w, 46);
const ORIGINAL: Rect = Rect::new(SHEET.x + 48, SHEET.y + 542, SHEET.w - 96, 104);
const ORIGINAL_LABEL: Rect = Rect::new(ORIGINAL.x, ORIGINAL.y + ORIGINAL.h + 6, ORIGINAL.w, 46);
const CLOSE: Rect = Rect::new(SHEET.x + SHEET.w - 278, SHEET.y + 716, 230, 100);

#[derive(Default)]
pub(super) struct Navigation {
    origin: Option<Origin>,
    chapter: Option<ChapterResults>,
    pub(super) menu_open: bool,
}

struct ChapterResults {
    scope: SearchChapterScope,
    projection: RefCell<Projection>,
}

#[derive(Default)]
struct Projection {
    seen: usize,
    hits: Vec<usize>,
}

pub(super) enum ResultOrder<'a> {
    All(usize),
    Chapter(Ref<'a, Vec<usize>>),
}

impl ResultOrder<'_> {
    pub(super) fn len(&self) -> usize {
        match self {
            Self::All(count) => *count,
            Self::Chapter(indices) => indices.len(),
        }
    }

    pub(super) fn provider_index(&self, ordinal: usize) -> Option<usize> {
        match self {
            Self::All(count) => (ordinal < *count).then_some(ordinal),
            Self::Chapter(indices) => indices.get(ordinal).copied(),
        }
    }

    pub(super) fn ordinal(&self, index: usize) -> Option<usize> {
        match self {
            Self::All(count) => (index < *count).then_some(index),
            Self::Chapter(indices) => indices.binary_search(&index).ok(),
        }
    }

    pub(super) fn step(&self, index: usize, direction: isize) -> Option<usize> {
        self.provider_index(self.ordinal(index)?.checked_add_signed(direction)?)
    }
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
            ..Navigation::default()
        };
    }

    pub(super) fn search_result_order(&self) -> ResultOrder<'_> {
        let count = if self.fixed.is_some() {
            self.pdf_hits.len()
        } else {
            self.hits.len()
        };
        let Some(chapter) = &self.search_results_navigation.chapter else {
            return ResultOrder::All(count);
        };
        if chapter.projection.borrow().seen != count {
            let mut projection = chapter.projection.borrow_mut();
            // Query replacement resets the scope. An existing query only
            // appends complete batches, leaving earlier provider indices intact.
            if projection.seen > count {
                *projection = Projection::default();
            }
            let new_matches = (projection.seen..count)
                .filter(|&index| chapter.scope.includes_result(self, index));
            projection.hits.extend(new_matches);
            projection.seen = count;
        }
        ResultOrder::Chapter(Ref::map(chapter.projection.borrow(), |state| &state.hits))
    }

    pub(super) fn search_chapter_active(&self) -> bool {
        self.search_results_navigation.chapter.is_some()
    }

    pub(super) fn reset_search_chapter(&mut self) {
        if self.search_chapter_active() {
            self.set_search_chapter(None);
        }
    }

    fn set_search_chapter(&mut self, scope: Option<SearchChapterScope>) {
        self.search_results_navigation.chapter = scope.map(|scope| ChapterResults {
            scope,
            projection: RefCell::new(Projection::default()),
        });
        self.clear_flexible_results();
        if self.screen == Screen::Matches {
            self.list_page = 0;
        }
        self.reset_match_window();
        self.focused_result = None;
        self.result_context = None;
    }

    fn focus_on_current_chapter(&mut self) -> Result<()> {
        if self.active.is_some() {
            self.ensure_reflow_search_current()?;
        }
        let scope = self.current_search_chapter()?;
        self.set_search_chapter(Some(scope));
        Ok(())
    }

    fn results_origin(&self) -> Option<&Origin> {
        let origin = self.search_results_navigation.origin.as_ref()?;
        (self.open_book_id() == Some(origin.book)).then_some(origin)
    }

    fn has_search_result_navigation(&self) -> bool {
        self.screen == Screen::Matches && self.results_origin().is_some()
    }

    pub(super) fn draw_search_results_options(&self, canvas: &mut Canvas) {
        if !self.has_search_result_navigation() {
            return;
        }
        let options = self.search_layout().options_button(OPTIONS);
        canvas.button(options, "", false, false);
        let ink = if canvas.is_control_pressed(options) {
            255
        } else {
            0
        };
        canvas.icon(
            Rect::new(options.x + 27, options.y + 27, 46, 46),
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
        let layout = self.search_layout();
        canvas.popup(layout.centered(SHEET));
        canvas.text(
            layout.centered(TITLE),
            "Search result options",
            TextStyle::new(50).bold(),
        );
        let scoped = self.search_chapter_active();
        canvas.button(layout.centered(ALL), "All results", !scoped, false);
        canvas.button(
            layout.centered(CHAPTER),
            "Results in current chapter",
            scoped,
            false,
        );
        canvas.button(
            layout.centered(CURRENT),
            "Focus results near current page",
            false,
            false,
        );
        canvas.text(
            layout.centered(CURRENT_LABEL),
            &self.search_result_page_label(),
            CAPTION.center(),
        );
        canvas.button(
            layout.centered(ORIGINAL),
            "Return to original page",
            false,
            false,
        );
        canvas.text(
            layout.centered(ORIGINAL_LABEL),
            &origin.label,
            CAPTION.center(),
        );
        canvas.button(layout.centered(CLOSE), "Done", true, false);
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
        let layout = self.search_layout();
        if !self.search_results_navigation.menu_open {
            if matches!(input, Input::Tap(x, y) if layout.options_button(OPTIONS).contains(x, y)) {
                self.result_context = None;
                self.search_results_navigation.menu_open = true;
                return Some(Change::Partial);
            }
            return None;
        }
        let outcome = match input {
            Input::Tap(x, y) if layout.centered(ALL).contains(x, y) => {
                self.reset_search_chapter();
                Ok(())
            }
            Input::Tap(x, y) if layout.centered(CHAPTER).contains(x, y) => {
                self.focus_on_current_chapter()
            }
            Input::Tap(x, y) if layout.centered(CURRENT).contains(x, y) => {
                self.focus_results_at_current_page()
            }
            Input::Tap(x, y) if layout.centered(ORIGINAL).contains(x, y) => {
                self.go_to_search_origin()
            }
            Input::Tap(x, y)
                if layout.centered(CLOSE).contains(x, y)
                    || !layout.centered(SHEET).contains(x, y) =>
            {
                Ok(())
            }
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
        if self.active.is_some() {
            self.ensure_reflow_search_current()?;
        }
        let order = self.search_result_order();
        let indices = (0..order.len()).filter_map(|ordinal| order.provider_index(ordinal));
        let index = if let Some(fixed) = &self.fixed {
            let page = fixed.reading.page();
            at_page(indices.map(|index| (index, self.pdf_hits[index].page.cmp(&page))))
        } else {
            let reading = &self.active.as_ref().ok_or("Open a book first")?.reading;
            at_page(indices.filter_map(|index| {
                let hit = &self.hits[index];
                let relation = match hit.chapter.cmp(&reading.chapter) {
                    Ordering::Equal => reading.probe.page_for(&hit.anchor)?.cmp(&reading.page),
                    order => order,
                };
                Some((index, relation))
            }))
        }
        .ok_or("No results near this page")?;
        drop(order);
        self.go_to_match_list_index(index);
        Ok(())
    }

    fn go_to_match_list_index(&mut self, index: usize) {
        self.list_page = self.match_page_for_index(index);
        self.remember_match_window(self.list_page);
        self.focused_result = Some(index);
    }
}

#[cfg(test)]
mod tests;
