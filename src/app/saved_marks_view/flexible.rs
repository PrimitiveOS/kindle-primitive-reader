//! Measured bookmark rows shared by drawing, hit testing and page movement.
use primitive_ui::{BLACK, HAIRLINE, PAPER, SECONDARY, chevron};

use crate::highlight_style::HighlightColor;

#[cfg(test)]
use super::super::list_density::LIST_HEIGHT;
use super::super::list_density::{LIST_FOOTER_SPACE, font_pixels, measure_rows, text_line_height};
use super::super::navigation_layout::NavigationLayout;
use super::super::{Action, BODY, CAPTION, Canvas, Change, Input, Item, LIST_Y, Reader, Rect};
use super::appearance::draw_caption;
use super::fit::TitleFit;

const ROW_PADDING: i32 = 12;
const MIN_ROW_HEIGHT: i32 = 100;

pub(in crate::app) struct FlexibleMarks {
    items: Vec<Item>,
    pages: Vec<MarkPage>,
    measuring: Option<Canvas>,
    spill: Option<MarkRow>,
    font: u8,
    max_lines: usize,
    layout: NavigationLayout,
}

struct MarkPage {
    first: usize,
    rows: Vec<MarkRow>,
}

impl MarkPage {
    fn end(&self) -> usize {
        self.first + self.rows.len()
    }

    fn hit(&self, x: f32, y: f32) -> Option<usize> {
        self.rows
            .iter()
            .position(|row| row.rect.contains(x, y))
            .map(|offset| self.first + offset)
    }
}

struct MarkRow {
    rect: Rect,
    title: TitleFit,
    normalized_title: Option<String>,
    caption_size: i32,
    caption_height: i32,
}

fn is_saved_mark(action: &Action) -> bool {
    matches!(
        action,
        Action::Bookmark(_) | Action::Highlight(_) | Action::PdfHighlight(_)
    )
}

impl MarkRow {
    fn measure(
        canvas: &mut Canvas,
        item: &Item,
        requested_font: u8,
        max_lines: usize,
        layout: NavigationLayout,
    ) -> Self {
        let is_mark = is_saved_mark(&item.action);
        let font = if is_mark { requested_font } else { 18 };
        let normalized_title = item
            .title
            .contains(['\n', '\r'])
            .then(|| item.title.replace(['\n', '\r'], " "));
        let label = normalized_title.as_deref().unwrap_or(&item.title);
        let caption_size = font_pixels(font.saturating_sub(4)).min(28);
        let caption_height = if item.detail.is_some() {
            text_line_height(canvas, caption_size)
        } else {
            0
        };
        // Reader captions include dates and chapter names. Give them a full
        // line below the bookmark rather than reducing the measured title width.
        let available = layout.height - LIST_Y - LIST_FOOTER_SPACE - ROW_PADDING - caption_height;
        let row = layout.body(0, 0);
        let title = TitleFit::measure_limited(
            canvas,
            label,
            Rect::new(0, 0, row.w - 120, available),
            font,
            if is_mark { max_lines } else { 1 },
        );
        let height = title.height() + caption_height;
        Self {
            rect: Rect {
                h: (height + ROW_PADDING).max(MIN_ROW_HEIGHT),
                ..row
            },
            title,
            normalized_title,
            caption_size,
            caption_height,
        }
    }

    fn draw(
        &self,
        canvas: &mut Canvas,
        item: &Item,
        has_separator: bool,
        color: Option<HighlightColor>,
    ) {
        let row = self.rect;
        let pressed = canvas.control_pressed(row, false);
        if pressed {
            canvas.panel(row.inset(6), 16, 2, BLACK);
        }
        let ink = if pressed { PAPER } else { BLACK };
        let detail_ink = if pressed {
            PAPER - SECONDARY
        } else {
            SECONDARY
        };
        let text_x = row.x + 32;
        let title_height = self.title.height();
        let title_y = row.y + (row.h - title_height - self.caption_height) / 2;
        self.title.draw(
            canvas,
            Rect::new(text_x, title_y, row.w - 120, title_height),
            self.normalized_title.as_deref().unwrap_or(&item.title),
            ink,
        );
        if let Some(caption) = &item.detail {
            draw_caption(
                canvas,
                Rect::new(
                    text_x,
                    title_y + title_height,
                    row.w - 120,
                    self.caption_height,
                ),
                caption,
                self.caption_size,
                detail_ink,
                color,
            );
        }
        chevron(
            canvas,
            Rect::new(row.right() - 72, row.y + (row.h - 40) / 2, 40, 40),
            ink,
        );
        if has_separator {
            canvas.fill(Rect::new(text_x, row.bottom() - 2, row.w - 64, 2), HAIRLINE);
        }
    }
}

impl FlexibleMarks {
    fn new(
        items: Vec<Item>,
        fonts: &[String],
        font: u8,
        max_lines: u8,
        layout: NavigationLayout,
    ) -> Result<Self, String> {
        // Only a line of backing pixels is needed to shape text at the real
        // canvas width. No full-screen pixel buffer is retained for this view.
        let measuring = if items.is_empty() {
            None
        } else {
            Some(Canvas::new(layout.width as u32, 1, fonts)?)
        };
        let mut view = Self {
            items,
            pages: Vec::new(),
            measuring,
            spill: None,
            font,
            max_lines: usize::from(max_lines),
            layout,
        };
        view.append_page();
        Ok(view)
    }

    fn append_page(&mut self) -> bool {
        let Some(canvas) = self.measuring.as_mut() else {
            return false;
        };
        let first = self.pages.last().map_or(0, MarkPage::end);
        let (rows, spill) = measure_rows(
            first,
            self.items.len(),
            self.layout.height - LIST_FOOTER_SPACE,
            self.spill.take(),
            |index| {
                MarkRow::measure(
                    canvas,
                    &self.items[index],
                    self.font,
                    self.max_lines,
                    self.layout,
                )
            },
            |row| &mut row.rect,
        );
        self.spill = spill;
        if rows.is_empty() {
            return false;
        }
        let page = MarkPage { first, rows };
        if page.end() == self.items.len() {
            self.measuring = None;
        }
        self.pages.push(page);
        true
    }

    fn has_next(&self, page: usize) -> bool {
        self.pages
            .get(page)
            .is_some_and(|plan| plan.end() < self.items.len())
    }
}

impl Reader {
    pub(in crate::app) fn prepare_flexible_marks(&mut self) {
        self.flexible_marks = None;
        if !self.screen.is_bookmark_list() {
            return;
        }
        let Some(max_lines) = self.controls.bookmark_max_lines() else {
            return;
        };
        let fonts = self
            .fonts
            .iter()
            .map(|font| font.display().to_string())
            .collect::<Vec<_>>();
        match FlexibleMarks::new(
            self.items(),
            &fonts,
            self.controls.bookmark_density.font(),
            max_lines,
            self.navigation_layout(),
        ) {
            Ok(view) => self.flexible_marks = Some(view),
            Err(error) => {
                self.report(error);
            }
        }
    }

    pub(in crate::app) fn draw_flexible_marks(&self, canvas: &mut Canvas) -> bool {
        let Some(view) = &self.flexible_marks else {
            return false;
        };
        let Some(page) = view.pages.get(self.list_page) else {
            canvas.text(
                view.layout.body(LIST_Y, 110),
                "No saved marks in this book",
                BODY,
            );
            return true;
        };
        if let Some(last) = page.rows.last() {
            canvas.card(view.layout.body(LIST_Y, last.rect.bottom() - LIST_Y));
        }
        for (index, row) in page.rows.iter().enumerate() {
            let item = &view.items[page.first + index];
            row.draw(
                canvas,
                item,
                self.controls.bookmark_list_separators && index + 1 < page.rows.len(),
                self.bookmark_row_color(&item.action),
            );
        }
        let previous = self.list_page > 0;
        let next = view.has_next(self.list_page);
        if previous {
            pager_button(canvas, view.layout.previous(), "Previous");
        }
        if next {
            pager_button(canvas, view.layout.next(), "Next");
        }
        let label = self.notice.clone().or_else(|| {
            if view.measuring.is_some() {
                Some(format!("Page {}", self.list_page + 1))
            } else if view.pages.len() > 1 {
                Some(format!("{} / {}", self.list_page + 1, view.pages.len()))
            } else {
                None
            }
        });
        if let Some(label) = label {
            canvas.text(
                view.layout.footer_text(310, 93, 50),
                &label,
                CAPTION.center(),
            );
        }
        true
    }

    pub(in crate::app) fn flexible_mark_input(&mut self, input: Input) -> Change {
        let layout = self.navigation_layout();
        match input {
            Input::PageBack => return self.turn_flexible_bookmark_page(-1),
            Input::PageForward => return self.turn_flexible_bookmark_page(1),
            Input::Swipe { dx, dy, .. } if dy.abs() > dx.abs() => {
                return self.turn_flexible_bookmark_page(if dy < 0.0 { 1 } else { -1 });
            }
            Input::Tap(x, y) if layout.back().contains(x, y) => {
                self.show(self.tools_screen());
                return Change::Partial;
            }
            Input::Tap(x, y) if pager_hit(layout.previous()).contains(x, y) => {
                return self.turn_flexible_bookmark_page(-1);
            }
            Input::Tap(x, y) if pager_hit(layout.next()).contains(x, y) => {
                return self.turn_flexible_bookmark_page(1);
            }
            _ => {}
        }
        let (x, y, hold) = match input {
            Input::Tap(x, y) => (x, y, false),
            Input::LongTap(x, y) => (x, y, true),
            _ => return Change::None,
        };
        let Some(view) = &mut self.flexible_marks else {
            return Change::None;
        };
        let Some(index) = view
            .pages
            .get(self.list_page)
            .and_then(|page| page.hit(x, y))
        else {
            return Change::None;
        };
        let action = if hold {
            match view.items[index].action {
                Action::Highlight(index) | Action::PdfHighlight(index) => {
                    Action::ManageHighlight(index)
                }
                _ => return Change::None,
            }
        } else {
            std::mem::replace(&mut view.items[index].action, Action::Heading)
        };
        let result = self.activate(action);
        if !hold {
            self.restore_flexible_action(index);
        }
        match result {
            Ok(()) => self.applied_change(),
            Err(error) => self.report(error),
        }
    }

    fn restore_flexible_action(&mut self, index: usize) {
        // Exports and failed actions can stay on the same screen. Restore the
        // consumed action while keeping the cached page if its rows still match.
        if !self.flexible_marks.as_ref().is_some_and(|view| {
            view.items
                .get(index)
                .is_some_and(|item| matches!(item.action, Action::Heading))
        }) {
            return;
        }
        let items = self.items();
        let view = self.flexible_marks.as_mut().unwrap();
        let same_rows = items.len() == view.items.len()
            && items
                .iter()
                .zip(&view.items)
                .all(|(a, b)| a.title == b.title && a.detail == b.detail);
        if same_rows {
            view.items = items;
        } else {
            self.list_page = 0;
            self.prepare_flexible_marks();
        }
    }

    fn turn_flexible_bookmark_page(&mut self, direction: isize) -> Change {
        let Some(page) = self.list_page.checked_add_signed(direction) else {
            return Change::None;
        };
        let Some(view) = &mut self.flexible_marks else {
            return Change::None;
        };
        if page < view.pages.len() || (view.has_next(self.list_page) && view.append_page()) {
            self.list_page = page;
            Change::Partial
        } else {
            Change::None
        }
    }
}

fn pager_hit(face: Rect) -> Rect {
    face.inset(-14)
}

fn pager_button(canvas: &mut Canvas, face: Rect, label: &str) {
    let pressed = canvas.control_pressed(pager_hit(face), false);
    canvas.button(face, label, false, pressed);
}

#[cfg(test)]
mod appearance_tests;
#[cfg(test)]
mod tests;
