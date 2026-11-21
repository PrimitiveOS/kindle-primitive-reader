//! Tap a painted annotation without leaving the page or losing its viewport.
use super::*;
use std::cell::Cell;

const PER_PAGE: usize = 4;
const ROW: i32 = 112;
const TEXT: primitive_ui::TextStyle = primitive_ui::TextStyle::new(36).lines(7).break_long_words();

#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    Choices,
    Note(usize),
    Actions(usize),
    RemoveNote(usize),
    RemoveMark(usize),
}

pub(super) struct SavedPopup {
    hits: Vec<usize>,
    view: View,
    anchor_y: i32,
    page: usize,
    pages: Cell<usize>,
}

impl SavedPopup {
    fn rect(&self) -> Rect {
        let height = match self.view {
            View::Choices => {
                130 + self.hits.len().min(PER_PAGE) as i32 * ROW
                    + if self.hits.len() > PER_PAGE { 114 } else { 12 }
            }
            View::Note(_) => 656,
            View::Actions(_) => 556,
            View::RemoveNote(_) | View::RemoveMark(_) => 360,
        };
        let y = if self.anchor_y + 40 + height <= PAGE_HEIGHT as i32 - 28 {
            self.anchor_y + 40
        } else {
            self.anchor_y - height - 40
        };
        Rect::new(
            88,
            y.clamp(28, PAGE_HEIGHT as i32 - height - 28),
            WIDTH as i32 - 176,
            height,
        )
    }

    fn row(&self, row: usize) -> Rect {
        let r = self.rect();
        Rect::new(r.x + 24, r.y + 118 + row as i32 * ROW, r.w - 48, ROW - 4)
    }

    fn close(&self) -> Rect {
        let r = self.rect();
        Rect::new(r.right() - 182, r.y + 12, 158, 100)
    }
    fn button(&self, i: usize, count: usize) -> Rect {
        let r = self.rect();
        let w = (r.w - 48 - 12 * (count as i32 - 1)) / count as i32;
        Rect::new(r.x + 24 + i as i32 * (w + 12), r.bottom() - 116, w, 100)
    }
    fn manage_button(&self, right: bool) -> Rect {
        let row = self.row(1);
        let width = (row.w - 12) / 2;
        Rect::new(
            if right { row.x + width + 12 } else { row.x },
            row.bottom() - 20,
            width,
            100,
        )
    }

    fn body(&self) -> Rect {
        let r = self.rect();
        Rect::new(r.x + 32, r.y + 121, r.w - 64, r.h - 262)
    }
    fn change_view(&mut self, view: View) {
        self.view = view;
        self.page = 0;
        self.pages.set(1);
    }
}

impl Reader {
    fn mark_text(&self, index: usize) -> Option<(&str, Option<&str>)> {
        let state = self.library.state(self.open_book_id()?)?;
        if self.fixed.is_some() {
            let mark = state.pdf_highlights.get(index)?;
            Some((
                &mark.text,
                mark.note.as_deref().filter(|note| !note.trim().is_empty()),
            ))
        } else {
            let mark = state.highlights.get(index)?;
            Some((
                &mark.selection.text,
                mark.note.as_deref().filter(|note| !note.trim().is_empty()),
            ))
        }
    }

    pub(super) fn tap_saved_mark(&mut self, x: f32, y: f32) -> Option<Change> {
        if !(0.0..PAGE_HEIGHT as f32).contains(&y)
            || self.selection.is_some()
            || self.pdf_selection.is_some()
            || self.footnote.is_some()
        {
            return None;
        }
        let Some(book) = self.open_book_id().and_then(|id| self.library.state(id)) else {
            return None;
        };
        // Both engines use the same visible rectangles used to paint each saved mark.
        let hits: Vec<usize> = if self.screen == Screen::Book
            && let Some(active) = &self.active
        {
            if book.highlights.is_empty() {
                return None;
            }
            let hits = book
                .highlights
                .iter()
                .enumerate()
                .filter(|(_, mark)| {
                    active
                        .reading
                        .highlight_rects(&mark.selection)
                        .into_iter()
                        .any(|r| Rect::new(r.x, r.y, r.width, r.height).contains(x, y))
                })
                .map(|(i, _)| i)
                .collect::<Vec<_>>();
            if !hits.is_empty() && active.reading.link_at(x, y).is_some() {
                return Some(self.handle_book(Input::Tap(x, y)));
            }
            hits
        } else if self.screen == Screen::Fixed
            && let Some(fixed) = &self.fixed
        {
            if book.pdf_highlights.is_empty() {
                return None;
            }
            let p = hayro::kurbo::Point::new(x as f64, y as f64);
            let hits = book
                .pdf_highlights
                .iter()
                .enumerate()
                .filter(|(_, mark)| {
                    mark.parts().any(|(page, rects, _)| {
                        rects.iter().any(|r| {
                            fixed
                                .reading
                                .visible_rectangle(page, *r)
                                .is_some_and(|r| r.contains(p))
                        })
                    })
                })
                .map(|(i, _)| i)
                .collect::<Vec<_>>();
            if !hits.is_empty() && fixed.reading.link_at(x, y).is_some() {
                return Some(self.handle_fixed(Input::Tap(x, y)));
            }
            hits
        } else {
            return None;
        };
        if hits.is_empty() {
            return None;
        }
        let view = if hits.len() == 1 {
            self.saved_view(hits[0])
        } else {
            View::Choices
        };
        self.saved_popup = Some(SavedPopup {
            hits,
            view,
            anchor_y: y as i32,
            page: 0,
            pages: Cell::new(1),
        });
        self.sync_auto_turn(Instant::now());
        Some(Change::Partial)
    }

    fn saved_view(&self, index: usize) -> View {
        if self
            .mark_text(index)
            .is_some_and(|(_, note)| note.is_some())
        {
            View::Note(index)
        } else {
            View::Actions(index)
        }
    }

    pub(super) fn saved_popup_over_book(&self) -> bool {
        self.saved_popup.is_some()
            && self.range_edit.is_none()
            && matches!(self.screen, Screen::Book | Screen::Fixed)
    }

    pub(super) fn return_to_saved_mark(&mut self) {
        self.show(if self.fixed.is_some() {
            Screen::Fixed
        } else {
            Screen::Book
        });
    }

    pub(super) fn draw_saved_popup(&self, canvas: &mut Canvas) {
        let Some(popup) = &self.saved_popup else {
            return;
        };
        let rect = popup.rect();
        canvas.popup(rect);
        let (title, icon) = match popup.view {
            View::Choices => ("Overlapping highlights", bar_icons::HIGHLIGHT),
            View::Note(_) => ("Note", bar_icons::action_icon(&Action::EditNote)),
            View::Actions(_) => ("Highlight", bar_icons::HIGHLIGHT),
            View::RemoveNote(_) => (
                "Remove note?",
                bar_icons::action_icon(&Action::RemoveHighlight),
            ),
            View::RemoveMark(_) => (
                "Remove highlight?",
                bar_icons::action_icon(&Action::RemoveHighlight),
            ),
        };
        canvas.icon(Rect::new(rect.x + 30, rect.y + 40, 44, 44), icon, 0);
        canvas.text(
            Rect::new(rect.x + 98, rect.y + 31, rect.w - 318, 69),
            title,
            BODY.bold(),
        );
        canvas.button(popup.close(), "Close", false, false);
        let buttons = |canvas: &mut Canvas, labels: &[&str]| {
            for (i, label) in labels.iter().enumerate() {
                if !label.is_empty() {
                    canvas.button(popup.button(i, labels.len()), label, false, false);
                }
            }
        };
        match popup.view {
            View::Choices => {
                let count = popup.hits.len().div_ceil(PER_PAGE);
                popup.pages.set(count);
                for (slot, &i) in popup
                    .hits
                    .iter()
                    .skip(popup.page * PER_PAGE)
                    .take(PER_PAGE)
                    .enumerate()
                {
                    let Some((quote, note)) = self.mark_text(i) else {
                        continue;
                    };
                    let row = popup.row(slot);
                    canvas.icon(
                        Rect::new(row.x + 14, row.y + 27, 42, 42),
                        if note.is_some() {
                            bar_icons::action_icon(&Action::EditNote)
                        } else {
                            bar_icons::HIGHLIGHT
                        },
                        0,
                    );
                    canvas.text(
                        Rect::new(row.x + 78, row.y + 11, row.w - 94, 49),
                        quote.lines().next().unwrap_or(quote),
                        BODY,
                    );
                    canvas.text(
                        Rect::new(row.x + 78, row.y + 64, row.w - 94, 40),
                        note.unwrap_or("Highlight"),
                        CAPTION,
                    );
                    canvas.fill(Rect::new(row.x, row.bottom() - 2, row.w, 2), 180);
                }
                if count > 1 {
                    buttons(
                        canvas,
                        &[
                            if popup.page > 0 { "Previous" } else { "" },
                            if popup.page + 1 < count { "Next" } else { "" },
                        ],
                    );
                }
            }
            View::Note(index) => {
                let Some((_, Some(note))) = self.mark_text(index) else {
                    return;
                };
                let pages = canvas.text_pages(popup.body(), note, TEXT);
                popup.pages.set(pages.len().max(1));
                if let Some(range) = pages.get(popup.page) {
                    canvas.text(popup.body(), &note[range.clone()], TEXT);
                }
                if pages.len() > 1 {
                    buttons(
                        canvas,
                        &[
                            if popup.page > 0 { "Previous" } else { "" },
                            "Edit",
                            "Actions",
                            if popup.page + 1 < pages.len() {
                                "Next"
                            } else {
                                ""
                            },
                        ],
                    );
                } else {
                    buttons(canvas, &["Edit note", "Actions"]);
                }
            }
            View::Actions(index) => {
                let Some((quote, note)) = self.mark_text(index) else {
                    return;
                };
                canvas.text(
                    Rect::new(rect.x + 32, rect.y + 117, rect.w - 64, 135),
                    quote,
                    BODY.lines(2).break_long_words(),
                );
                canvas.button(
                    popup.manage_button(false),
                    if note.is_some() {
                        "Edit note"
                    } else {
                        "Add note"
                    },
                    false,
                    false,
                );
                canvas.button(popup.manage_button(true), "Extend", false, false);
                buttons(
                    canvas,
                    &[
                        if note.is_some() { "Remove note" } else { "" },
                        "Remove highlight",
                    ],
                );
            }
            View::RemoveNote(_) => {
                canvas.text(
                    popup.body(),
                    "The highlighted passage will stay in your book.",
                    BODY.lines(2),
                );
                buttons(canvas, &["Cancel", "Remove note"]);
            }
            View::RemoveMark(_) => {
                canvas.text(
                    popup.body(),
                    "The passage highlight and its note will be removed.",
                    BODY.lines(2),
                );
                buttons(canvas, &["Cancel", "Remove highlight"]);
            }
        }
    }

    fn close_saved_popup(&mut self) {
        self.saved_popup = None;
        self.sync_auto_turn(Instant::now());
    }
    fn step_saved_popup(&mut self, dir: isize) -> Change {
        let Some(p) = self.saved_popup.as_mut() else {
            return Change::None;
        };
        if let Some(i) = p
            .page
            .checked_add_signed(dir)
            .filter(|&i| i < p.pages.get())
        {
            p.page = i;
            Change::Partial
        } else {
            Change::None
        }
    }

    pub(super) fn saved_popup_input(&mut self, input: Input) -> Change {
        let Some(p) = self.saved_popup.as_ref() else {
            return Change::None;
        };
        match input {
            Input::PageForward => return self.step_saved_popup(1),
            Input::PageBack => return self.step_saved_popup(-1),
            Input::Swipe { dx, dy, .. } if dx > dy.abs() && dx > 0.0 => {
                self.close_saved_popup();
                return Change::Partial;
            }
            Input::Swipe { dx, dy, .. } if -dx > dy.abs() => return self.step_saved_popup(1),
            Input::Swipe { dy, .. } => return self.step_saved_popup(if dy < 0.0 { 1 } else { -1 }),
            _ => {}
        }
        let Input::Tap(x, y) = input else {
            return Change::None;
        };
        if p.close().contains(x, y) || !p.rect().contains(x, y) {
            self.close_saved_popup();
            return Change::Partial;
        }
        let button = |i, n| p.button(i, n).contains(x, y);
        let next = match p.view {
            View::Choices => {
                if p.pages.get() > 1 {
                    if button(0, 2) {
                        return self.step_saved_popup(-1);
                    }
                    if button(1, 2) {
                        return self.step_saved_popup(1);
                    }
                }
                let Some(&index) = p
                    .hits
                    .iter()
                    .skip(p.page * PER_PAGE)
                    .take(PER_PAGE)
                    .enumerate()
                    .find_map(|(slot, index)| p.row(slot).contains(x, y).then_some(index))
                else {
                    return Change::None;
                };
                self.saved_view(index)
            }
            View::Note(index) => {
                let paged = p.pages.get() > 1;
                let n = if paged { 4 } else { 2 };
                if paged && button(0, n) {
                    return self.step_saved_popup(-1);
                }
                if paged && button(3, n) {
                    return self.step_saved_popup(1);
                }
                if button(usize::from(paged), n) {
                    return self.edit_tapped_note(index);
                }
                if !button(usize::from(paged) + 1, n) {
                    return Change::None;
                }
                View::Actions(index)
            }
            View::Actions(index) => {
                if p.manage_button(false).contains(x, y) {
                    return self.edit_tapped_note(index);
                }
                if p.manage_button(true).contains(x, y) {
                    self.highlight_index = index;
                    return match self.begin_extending_highlight() {
                        Ok(()) => Change::Partial,
                        Err(e) => self.report(e),
                    };
                }
                if button(1, 2) {
                    View::RemoveMark(index)
                } else if button(0, 2) && self.mark_text(index).is_some_and(|(_, n)| n.is_some()) {
                    View::RemoveNote(index)
                } else {
                    return Change::None;
                }
            }
            View::RemoveMark(index) | View::RemoveNote(index) => {
                if button(0, 2) {
                    View::Actions(index)
                } else if button(1, 2) {
                    let remove = matches!(p.view, View::RemoveMark(_));
                    self.highlight_index = index;
                    if remove {
                        self.saved_popup = None;
                        match self.activate(Action::RemoveHighlight) {
                            Ok(()) => self.return_to_saved_mark(),
                            Err(e) => return self.report(e),
                        }
                        return Change::Partial;
                    }
                    if let Err(e) = self.update_note(String::new()) {
                        return self.report(e);
                    }
                    View::Actions(index)
                } else {
                    return Change::None;
                }
            }
        };
        if let Some(p) = self.saved_popup.as_mut() {
            p.change_view(next);
        }
        Change::Partial
    }

    fn edit_tapped_note(&mut self, index: usize) -> Change {
        self.highlight_index = index;
        match self.activate(Action::EditNote) {
            Ok(()) => Change::Partial,
            Err(e) => self.report(e),
        }
    }

    pub(super) fn saved_note_updated(&mut self, index: usize) {
        let view = self.saved_view(index);
        if let Some(p) = self.saved_popup.as_mut() {
            p.change_view(view);
        }
        self.return_to_saved_mark();
    }
}

#[cfg(test)]
#[path = "saved_highlight_popup_tests.rs"]
mod tests;
