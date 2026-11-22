//! Tap a painted annotation without leaving the page or losing its viewport.
use super::*;
use crate::highlight_style::{HighlightColor, HighlightStyle, NoteMarker};
use std::cell::Cell;

const PER_PAGE: usize = 4;
const ROW: i32 = 112;
const TEXT: primitive_ui::TextStyle = primitive_ui::TextStyle::new(36).lines(7).break_long_words();

#[derive(Clone, Copy, PartialEq, Eq)]
enum Amount {
    HighlightOpacity,
    SelectionOpacity,
    LineHeight,
}
impl Amount {
    const ALL: [Self; 3] = [
        Self::HighlightOpacity,
        Self::SelectionOpacity,
        Self::LineHeight,
    ];

    fn enabled(self, drawer: HighlightStyle) -> bool {
        match self {
            Self::HighlightOpacity => drawer == HighlightStyle::Lighten,
            Self::SelectionOpacity => true,
            Self::LineHeight => matches!(drawer, HighlightStyle::Lighten | HighlightStyle::Invert),
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::HighlightOpacity => "Gray highlight opacity",
            Self::SelectionOpacity => "Gray selection opacity",
            Self::LineHeight => "Highlight line height",
        }
    }
    fn percent(self, appearance: crate::highlight_style::Appearance) -> u8 {
        match self {
            Self::HighlightOpacity => appearance.highlight_opacity.min(100),
            Self::SelectionOpacity => appearance.selection_opacity.min(100),
            Self::LineHeight => appearance.line_height.min(100),
        }
    }
    fn write(self, appearance: &mut crate::highlight_style::Appearance, percent: u8) {
        match self {
            Self::HighlightOpacity => appearance.highlight_opacity = percent,
            Self::SelectionOpacity => appearance.selection_opacity = percent,
            Self::LineHeight => appearance.line_height = percent,
        }
    }
    fn step(self, held: bool) -> i16 {
        match (self, held) {
            (Self::LineHeight, true) => 10,
            (Self::LineHeight, false) => 1,
            (_, true) => 5,
            (_, false) => 10,
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    Choices,
    Note(usize),
    Actions(usize),
    Styles(Option<usize>),
    Adjust(Amount),
    Colors(Option<usize>),
    NoteMarkers,
    ApplyAll {
        book: u64,
        style: HighlightStyle,
        color: HighlightColor,
    },
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
            View::Actions(_) => 664,
            View::Styles(Some(_)) => 504,
            View::Styles(None) => 872,
            View::Colors(Some(_)) => 504,
            View::Colors(None) => 632,
            View::NoteMarkers => 380,
            View::ApplyAll { .. } => 480,
            View::Adjust(_) => 395,
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

    fn style_action(&self) -> Rect {
        let r = self.manage_button(false);
        Rect::new(r.x, r.bottom() + 12, self.rect().w - 48, r.h)
    }

    fn style_cell(&self, index: usize) -> Rect {
        let r = self.rect();
        let w = (r.w - 60) / 2;
        Rect::new(
            r.x + 24 + (index % 2) as i32 * (w + 12),
            r.y + 124 + (index / 2) as i32 * 116,
            w,
            104,
        )
    }

    fn color_entry(&self, index: Option<usize>, right: bool) -> Rect {
        let r = self.rect();
        let w = (r.w - 60) / 2;
        Rect::new(
            r.x + 24 + if right { w + 12 } else { 0 },
            r.y + if index.is_some() { 375 } else { 497 },
            w,
            100,
        )
    }
    fn color_cell(&self, i: usize) -> Rect {
        let r = self.rect();
        let w = (r.w - 72) / 3;
        Rect::new(
            r.x + 24 + (i % 3) as i32 * (w + 12),
            r.y + 124 + (i / 3) as i32 * 116,
            w,
            104,
        )
    }

    fn amount_entry(&self, i: usize) -> Rect {
        let r = self.rect();
        let width = (r.w - 72) / 3;
        Rect::new(r.x + 24 + i as i32 * (width + 12), r.y + 375, width, 105)
    }

    fn workflow_entry(&self, right: bool) -> Rect {
        let r = self.rect();
        let w = (r.w - 60) / 2;
        Rect::new(r.x + 24 + if right { w + 12 } else { 0 }, r.y + 609, w, 100)
    }

    fn adjust(&self, right: bool) -> Rect {
        let r = self.rect();
        Rect::new(
            if right { r.right() - 198 } else { r.x + 32 },
            r.y + 127,
            166,
            116,
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
    fn apply_all_highlight_appearance(
        &mut self,
        book: u64,
        style: HighlightStyle,
        color: HighlightColor,
    ) -> Result<usize> {
        if self.open_book_id() != Some(book) {
            return Err("The book changed. Open highlight settings again.".into());
        }
        let prior = self
            .library
            .state(book)
            .ok_or("This book is no longer tracked")?
            .clone();
        let state = self.library.state_mut(book).unwrap();
        let count = state.highlights.len() + state.pdf_highlights.len();
        for mark in &mut state.highlights {
            mark.style = style;
            mark.color = color;
        }
        for mark in &mut state.pdf_highlights {
            mark.style = style;
            mark.color = color;
        }
        // Commit the whole library through its atomic save before acknowledging
        // a bulk edit. Restore every field if storage rejects the transaction.
        if let Err(error) = self.library.persist() {
            *self.library.state_mut(book).unwrap() = prior;
            return Err(error.into());
        }
        self.mark_book_changed();
        Ok(count)
    }

    pub(super) fn effective_highlight_style(&self) -> HighlightStyle {
        self.open_book_id()
            .and_then(|id| self.library.state(id))
            .and_then(|s| s.highlight_style)
            .unwrap_or(self.controls.highlight_style)
    }

    pub(super) fn effective_highlight_color(&self) -> HighlightColor {
        self.open_book_id()
            .and_then(|id| self.library.state(id))
            .and_then(|state| state.highlight_color)
            .unwrap_or(self.controls.highlight_color)
    }
    fn mark_color(&self, index: usize) -> Option<HighlightColor> {
        let state = self.library.state(self.open_book_id()?)?;
        if self.fixed.is_some() {
            state.pdf_highlights.get(index).map(|mark| mark.color)
        } else {
            state.highlights.get(index).map(|mark| mark.color)
        }
    }
    fn save_highlight_color(
        &mut self,
        index: Option<usize>,
        color: Option<HighlightColor>,
    ) -> Change {
        let Some(id) = self.open_book_id() else {
            return Change::None;
        };
        let Some(state) = self.library.state_mut(id) else {
            return Change::None;
        };
        if let Some(index) = index {
            let target = if self.fixed.is_some() {
                state
                    .pdf_highlights
                    .get_mut(index)
                    .map(|mark| &mut mark.color)
            } else {
                state.highlights.get_mut(index).map(|mark| &mut mark.color)
            };
            let (Some(target), Some(color)) = (target, color) else {
                return Change::None;
            };
            *target = color;
            self.close_saved_popup();
        } else {
            state.highlight_color = Some(color.unwrap_or(self.controls.highlight_color));
        }
        self.mark_book_changed();
        Change::Partial
    }

    fn mark_style(&self, index: usize) -> Option<HighlightStyle> {
        let book = self.library.state(self.open_book_id()?)?;
        if self.fixed.is_some() {
            book.pdf_highlights.get(index).map(|m| m.style)
        } else {
            book.highlights.get(index).map(|m| m.style)
        }
    }

    pub(super) fn open_highlight_styles(&mut self, index: Option<usize>) {
        self.show(self.surface_screen());
        self.saved_popup = Some(SavedPopup {
            hits: index.into_iter().collect(),
            view: View::Styles(index),
            anchor_y: 380,
            page: 0,
            pages: Cell::new(1),
        });
        self.sync_auto_turn(Instant::now());
    }

    fn save_highlight_style(
        &mut self,
        index: Option<usize>,
        style: Option<HighlightStyle>,
    ) -> Change {
        let Some(id) = self.open_book_id() else {
            return Change::None;
        };
        let Some(state) = self.library.state_mut(id) else {
            return Change::None;
        };
        if let Some(index) = index {
            let stored = if self.fixed.is_some() {
                state.pdf_highlights.get_mut(index).map(|m| &mut m.style)
            } else {
                state.highlights.get_mut(index).map(|m| &mut m.style)
            };
            let (Some(stored), Some(style)) = (stored, style) else {
                return Change::None;
            };
            *stored = style;
            self.close_saved_popup();
        } else {
            state.highlight_style = style;
        }
        self.mark_book_changed();
        Change::Partial
    }

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
        let bottom = if self.hidden_overlap_footer() {
            primitive_app::HEIGHT
        } else {
            PAGE_HEIGHT
        };
        if !(0.0..bottom as f32).contains(&y)
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
                        .enumerate()
                        .any(|(i, r)| {
                            let width = active.reading.probe.settings.width;
                            let leaf_x = if active.reading.uses_facing_pages()
                                && r.x >= (WIDTH / 2) as i32
                            {
                                WIDTH - width
                            } else {
                                0
                            };
                            let margin = crate::note_marker::margin(
                                width,
                                active.reading.probe.settings.right_margin(),
                                leaf_x,
                                self.controls.note_marker,
                                false,
                            );
                            Rect::new(r.x, r.y, r.width, r.height).contains(x, y)
                                || (mark.note.is_some()
                                    && crate::note_marker::hit(
                                        r,
                                        self.controls.note_marker,
                                        margin,
                                        i == 0,
                                        WIDTH,
                                        x,
                                        y,
                                    ))
                        })
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
            let hits =
                book.pdf_highlights
                    .iter()
                    .enumerate()
                    .filter(|(_, mark)| {
                        mark.parts().any(|(page, rects, _)| {
                            rects
                                .iter()
                                .filter(|r| fixed.reading.visible_rectangle(page, **r).is_some())
                                .enumerate()
                                .any(|(i, r)| {
                                    fixed.reading.visible_rectangle(page, *r).is_some_and(
                                        |visible| {
                                            let raw = fixed
                                                .reading
                                                .annotation_rectangle(page, *r)
                                                .unwrap_or(visible);
                                            let rect = crate::selection::HighlightRect {
                                                x: raw.x0.floor() as i32,
                                                y: raw.y0.floor() as i32,
                                                width: raw.width().ceil() as i32,
                                                height: raw.height().ceil() as i32,
                                            };
                                            let margin = crate::note_marker::margin(
                                                WIDTH,
                                                0,
                                                0,
                                                self.controls.note_marker,
                                                true,
                                            );
                                            visible.contains(p)
                                                || (mark.note.is_some()
                                                    && crate::note_marker::hit(
                                                        rect,
                                                        self.controls.note_marker,
                                                        margin,
                                                        i == 0,
                                                        WIDTH,
                                                        x,
                                                        y,
                                                    ))
                                        },
                                    )
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
            View::Styles(Some(_)) => ("Highlight style", bar_icons::HIGHLIGHT),
            View::Styles(None) => ("New highlights in this book", bar_icons::HIGHLIGHT),
            View::Adjust(amount) => (amount.title(), bar_icons::HIGHLIGHT),
            View::Colors(_) => ("Highlight color", bar_icons::HIGHLIGHT),
            View::NoteMarkers => ("Note marker", bar_icons::HIGHLIGHT),
            View::ApplyAll { .. } => ("Update all highlights?", bar_icons::HIGHLIGHT),
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
                canvas.button(
                    popup.style_action(),
                    &format!(
                        "Style · {}",
                        self.mark_style(index).unwrap_or_default().label()
                    ),
                    false,
                    false,
                );
                buttons(
                    canvas,
                    &[
                        if note.is_some() { "Remove note" } else { "" },
                        "Remove highlight",
                    ],
                );
            }
            View::Styles(index) => {
                let selected = index
                    .and_then(|i| self.mark_style(i))
                    .unwrap_or_else(|| self.effective_highlight_style());
                for (i, style) in HighlightStyle::ALL.into_iter().enumerate() {
                    let cell = popup.style_cell(i);
                    let chosen = (style == selected) != canvas.control_pressed(cell, false);
                    canvas.panel(cell, 16, 4, if chosen { 0 } else { 255 });
                    let sample = Rect::new(cell.x + 16, cell.y + 20, 80, 64);
                    let (paper, ink) = match style {
                        HighlightStyle::Lighten => (204, 0),
                        HighlightStyle::Invert => (0, 255),
                        _ => (255, 0),
                    };
                    canvas.fill(sample, paper);
                    canvas.text(
                        Rect::new(sample.x, sample.y + 5, sample.w, sample.h - 5),
                        "Aa",
                        BODY.center().ink(ink),
                    );
                    if style == HighlightStyle::Underline {
                        canvas.fill(
                            Rect::new(sample.x + 13, sample.bottom() - 8, sample.w - 26, 4),
                            68,
                        );
                    }
                    if style == HighlightStyle::Strikethrough {
                        canvas.fill(Rect::new(sample.x + 13, sample.y + 32, sample.w - 26, 3), 0);
                    }
                    canvas.text(
                        Rect::new(sample.right() + 18, cell.y + 26, cell.w - 124, 64),
                        style.label(),
                        BODY.bold().ink(if chosen { 255 } else { 0 }),
                    );
                }
                let color_label = format!(
                    "Color · {}",
                    index
                        .and_then(|i| self.mark_color(i))
                        .unwrap_or_else(|| self.effective_highlight_color())
                        .label()
                );
                if selected == HighlightStyle::Invert {
                    let r = popup.color_entry(index, false);
                    canvas.panel(r, 16, 3, 255);
                    canvas.text(
                        Rect::new(r.x + 10, r.y + 25, r.w - 20, 60),
                        &color_label,
                        CAPTION.center().ink(145),
                    );
                } else {
                    canvas.button(popup.color_entry(index, false), &color_label, false, false);
                }
                if index.is_none() {
                    canvas.button(
                        popup.color_entry(index, true),
                        &format!("Note · {}", self.controls.note_marker.label()),
                        false,
                        false,
                    );
                    for (i, amount) in Amount::ALL.into_iter().enumerate() {
                        let r = popup.amount_entry(i);
                        let active = amount.enabled(selected);
                        let ink = if active { 0 } else { 145 };
                        if active {
                            canvas.button(r, "", false, false);
                        } else {
                            canvas.panel(r, 16, 3, 255);
                        }
                        let ink = if active && canvas.is_control_pressed(r) {
                            255
                        } else {
                            ink
                        };
                        canvas.text(
                            Rect::new(r.x + 10, r.y + 9, r.w - 20, 50),
                            amount.title(),
                            CAPTION.center().ink(ink),
                        );
                        canvas.text(
                            Rect::new(r.x + 10, r.y + 49, r.w - 20, 55),
                            &format!("{}%", amount.percent(self.controls.highlight_appearance)),
                            BODY.bold().center().ink(ink),
                        );
                    }
                    let selection_active = selected != HighlightStyle::Invert;
                    let label = if selection_active && self.controls.selection_uses_highlight_color
                    {
                        "Selection color · On"
                    } else {
                        "Selection color · Off"
                    };
                    let r = popup.workflow_entry(false);
                    if selection_active {
                        canvas.button(r, label, false, false);
                    } else {
                        canvas.panel(r, 16, 3, 255);
                        canvas.text(
                            Rect::new(r.x + 10, r.y + 25, r.w - 20, 60),
                            label,
                            CAPTION.center().ink(145),
                        );
                    }
                    canvas.button(
                        popup.workflow_entry(true),
                        "Apply to all highlights",
                        false,
                        false,
                    );
                    buttons(canvas, &["Use reader default", "Set reader default"]);
                }
            }
            View::Colors(index) => {
                let selected = index
                    .and_then(|i| self.mark_color(i))
                    .unwrap_or_else(|| self.effective_highlight_color());
                for (i, color) in HighlightColor::ALL.into_iter().enumerate() {
                    let r = popup.color_cell(i);
                    let chosen = (color == selected) != canvas.control_pressed(r, false);
                    canvas.panel(r, 16, 4, if chosen { 0 } else { 255 });
                    let sample = Rect::new(r.x + 18, r.y + 26, 48, 48);
                    canvas.panel(sample, 4, 2, color.gray().unwrap_or(204));
                    canvas.text(
                        Rect::new(sample.right() + 14, r.y + 28, r.w - 90, 60),
                        color.label(),
                        BODY.bold().ink(if chosen { 255 } else { 0 }),
                    );
                }
                if index.is_none() {
                    buttons(canvas, &["Use reader default", "Set reader default"]);
                }
            }
            View::NoteMarkers => {
                for (i, marker) in NoteMarker::ALL.into_iter().enumerate() {
                    let r = popup.style_cell(i);
                    let selected =
                        (self.controls.note_marker == marker) != canvas.control_pressed(r, false);
                    canvas.panel(r, 16, 4, if selected { 0 } else { 255 });
                    let ink = if selected { 255 } else { 0 };
                    let sample = Rect::new(r.x + 20, r.y + 28, 48, 48);
                    if marker == NoteMarker::Sidemark {
                        canvas.icon(sample, bar_icons::HIGHLIGHT, ink);
                    } else {
                        canvas.text(sample, "Aa", BODY.center().ink(ink));
                        if marker == NoteMarker::Underline {
                            canvas.fill(Rect::new(sample.x, sample.bottom() - 3, sample.w, 3), ink);
                        }
                        if marker == NoteMarker::Sideline {
                            canvas.fill(Rect::new(sample.right() + 5, sample.y, 3, sample.h), ink);
                        }
                    }
                    canvas.text(
                        Rect::new(sample.right() + 22, r.y + 29, r.w - 112, 60),
                        marker.label(),
                        BODY.bold().ink(ink),
                    );
                }
            }
            View::ApplyAll { book, style, color } => {
                let count = self
                    .library
                    .state(book)
                    .map_or(0, |s| s.highlights.len() + s.pdf_highlights.len());
                let body = popup.body();
                canvas.text(Rect::new(body.x,body.y,body.w,135),&format!("Apply {} and {} to all {count} highlights? Notes, positions and creation dates are kept.",style.label(),color.label()),BODY.lines(3));
                if let Some(notice) = &self.notice {
                    canvas.text(
                        Rect::new(body.x, body.y + 141, body.w, 76),
                        notice,
                        CAPTION.lines(2),
                    );
                }
                canvas.button(popup.button(0, 2), "Cancel", false, false);
                canvas.button(popup.button(1, 2), "Apply", true, false);
            }
            View::Adjust(amount) => {
                let percent = amount.percent(self.controls.highlight_appearance);
                let left = popup.adjust(false);
                let right = popup.adjust(true);
                for (button, plus) in [(left, false), (right, true)] {
                    canvas.button(button, "", false, false);
                    let (x, y) = (button.x + button.w / 2, button.y + button.h / 2);
                    let ink = if canvas.is_control_pressed(button) {
                        255
                    } else {
                        0
                    };
                    canvas.fill(Rect::new(x - 17, y - 2, 34, 5), ink);
                    if plus {
                        canvas.fill(Rect::new(x - 2, y - 17, 5, 34), ink);
                    }
                }
                canvas.text(
                    Rect::new(
                        left.right() + 20,
                        left.y + 23,
                        right.x - left.right() - 40,
                        74,
                    ),
                    &format!("{percent}%"),
                    primitive_ui::TextStyle::new(48).center().bold(),
                );
                buttons(canvas, &["Default", "Back"]);
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
        if matches!(p.view, View::ApplyAll { .. }) && input == Input::PageBack {
            self.saved_popup
                .as_mut()
                .unwrap()
                .change_view(View::Styles(None));
            return Change::Partial;
        }
        if let View::Adjust(amount) = p.view {
            let held = matches!(input, Input::LongTap(..));
            if let Input::Tap(x, y) | Input::LongTap(x, y) = input {
                let delta = if p.adjust(false).contains(x, y) {
                    Some(-amount.step(held))
                } else if p.adjust(true).contains(x, y) {
                    Some(amount.step(held))
                } else {
                    None
                };
                let initial = amount.percent(self.controls.highlight_appearance);
                let percent = if let Some(delta) = delta {
                    Some((i16::from(initial) + delta).clamp(0, 100) as u8)
                } else if !held && p.button(0, 2).contains(x, y) {
                    Some(amount.percent(Default::default()))
                } else {
                    None
                };
                if let Some(percent) = percent {
                    let mut controls = self.controls;
                    amount.write(&mut controls.highlight_appearance, percent);
                    if controls != self.controls {
                        if let Err(error) = controls.save(&self.controls_path) {
                            return self.report(error);
                        }
                        self.controls = controls;
                    }
                    return Change::Partial;
                }
            }
        }
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
                if p.style_action().contains(x, y) {
                    self.saved_popup
                        .as_mut()
                        .unwrap()
                        .change_view(View::Styles(Some(index)));
                    return Change::Partial;
                }
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
            View::Styles(index) => {
                if p.color_entry(index, false).contains(x, y) {
                    if index
                        .and_then(|i| self.mark_style(i))
                        .unwrap_or_else(|| self.effective_highlight_style())
                        == HighlightStyle::Invert
                    {
                        return Change::None;
                    }
                    self.saved_popup
                        .as_mut()
                        .unwrap()
                        .change_view(View::Colors(index));
                    return Change::Partial;
                }
                if index.is_none() && p.color_entry(index, true).contains(x, y) {
                    self.saved_popup
                        .as_mut()
                        .unwrap()
                        .change_view(View::NoteMarkers);
                    return Change::Partial;
                }
                if let Some((_, style)) = HighlightStyle::ALL
                    .iter()
                    .enumerate()
                    .find(|(i, _)| p.style_cell(*i).contains(x, y))
                {
                    return self.save_highlight_style(index, Some(*style));
                }
                if index.is_none() {
                    if p.workflow_entry(false).contains(x, y) {
                        if self.effective_highlight_style() == HighlightStyle::Invert {
                            return Change::None;
                        }
                        let controls = ReadingControls {
                            selection_uses_highlight_color: !self
                                .controls
                                .selection_uses_highlight_color,
                            ..self.controls
                        };
                        if let Err(error) = controls.save(&self.controls_path) {
                            return self.report(error);
                        }
                        self.controls = controls;
                        return Change::Partial;
                    }
                    if p.workflow_entry(true).contains(x, y) {
                        let Some(book) = self.open_book_id() else {
                            return Change::None;
                        };
                        let style = self.effective_highlight_style();
                        let color = self.effective_highlight_color();
                        self.notice = None;
                        self.saved_popup
                            .as_mut()
                            .unwrap()
                            .change_view(View::ApplyAll { book, style, color });
                        return Change::Partial;
                    }
                    for (i, amount) in Amount::ALL.into_iter().enumerate() {
                        if p.amount_entry(i).contains(x, y)
                            && amount.enabled(self.effective_highlight_style())
                        {
                            self.saved_popup
                                .as_mut()
                                .unwrap()
                                .change_view(View::Adjust(amount));
                            return Change::Partial;
                        }
                    }
                }
                if index.is_none() && button(0, 2) {
                    return self.save_highlight_style(None, None);
                }
                if index.is_none() && button(1, 2) {
                    let controls = ReadingControls {
                        highlight_style: self.effective_highlight_style(),
                        ..self.controls
                    };
                    if let Err(e) = controls.save(&self.controls_path) {
                        return self.report(e);
                    }
                    self.controls = controls;
                    self.close_saved_popup();
                    return Change::Partial;
                }
                return Change::None;
            }
            View::Colors(index) => {
                if let Some((_, color)) = HighlightColor::ALL
                    .iter()
                    .enumerate()
                    .find(|(i, _)| p.color_cell(*i).contains(x, y))
                {
                    return self.save_highlight_color(index, Some(*color));
                }
                if index.is_none() && button(0, 2) {
                    return self.save_highlight_color(None, None);
                }
                if index.is_none() && button(1, 2) {
                    let controls = ReadingControls {
                        highlight_color: self.effective_highlight_color(),
                        ..self.controls
                    };
                    if let Err(error) = controls.save(&self.controls_path) {
                        return self.report(error);
                    }
                    self.controls = controls;
                    self.close_saved_popup();
                    return Change::Partial;
                }
                return Change::None;
            }
            View::NoteMarkers => {
                if let Some((_, marker)) = NoteMarker::ALL
                    .iter()
                    .enumerate()
                    .find(|(i, _)| p.style_cell(*i).contains(x, y))
                {
                    let controls = ReadingControls {
                        note_marker: *marker,
                        ..self.controls
                    };
                    if let Err(error) = controls.save(&self.controls_path) {
                        return self.report(error);
                    }
                    self.controls = controls;
                    return Change::Partial;
                }
                return Change::None;
            }
            View::ApplyAll { book, style, color } => {
                if button(0, 2) {
                    View::Styles(None)
                } else if button(1, 2) {
                    match self.apply_all_highlight_appearance(book, style, color) {
                        Ok(count) => {
                            self.close_saved_popup();
                            self.notice = Some(format!("Updated {count} highlights"));
                            return Change::Partial;
                        }
                        Err(error) => return self.report(error),
                    }
                } else {
                    return Change::None;
                }
            }
            View::Adjust(_) => {
                if button(1, 2) {
                    View::Styles(None)
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

#[cfg(test)]
#[path = "highlight_style_tests.rs"]
mod style_tests;

#[cfg(test)]
#[path = "highlight_appearance_tests.rs"]
mod appearance_tests;

#[cfg(test)]
#[path = "highlight_color_tests.rs"]
mod color_tests;

#[cfg(test)]
#[path = "highlight_workflow_tests.rs"]
mod workflow_tests;
