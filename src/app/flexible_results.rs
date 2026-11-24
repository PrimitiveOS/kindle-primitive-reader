//! Full-text result rows measured and packed against the reference canvas.
use std::{borrow::Cow, ops::Range};

use primitive_ui::{HAIRLINE, SECONDARY, TextStyle, chevron};

use super::list_density::measure_rows;
use super::result_context::row_excerpt;
use super::*;

const CARD_WIDTH: i32 = WIDTH as i32 - 2 * LIST_X;
const TEXT_WIDTH: i32 = CARD_WIDTH - 116;
const INSET_Y: i32 = 9;
const GAP: i32 = 4;

pub(super) struct FlexibleResults {
    pages: Vec<ResultPage>,
    spill: Option<ResultRow>,
    canvas: Canvas,
    max_lines: usize,
    font: ResultFont,
}

struct ResultPage {
    first: usize,
    rows: Vec<ResultRow>,
}

impl ResultPage {
    fn end(&self) -> usize {
        self.first + self.rows.len()
    }

    fn hit(&self, x: f32, y: f32) -> Option<usize> {
        self.rows
            .iter()
            .position(|row| row.bounds.contains(x, y))
            .map(|offset| self.first + offset)
    }
}

struct ResultRow {
    bounds: Rect,
    lines: usize,
    prefix: usize,
    visible_end: Option<usize>,
}

struct ResultText<'a> {
    snippet: &'a str,
    matched: Range<usize>,
    expand: bool,
}

#[derive(Clone, Copy)]
struct ResultFont {
    title: i32,
    line_height: i32,
    detail: i32,
    detail_height: i32,
}

impl ResultFont {
    fn new(rows: u8) -> Self {
        // KOReader uses the same search-result font as the fixed list, and
        // computes it from the saved result count even in flexible mode.
        let logical = ((24 * 18 - (i32::from(rows) - 6) * 10) / 18) as u8;
        let title = super::list_density::font_pixels(logical);
        let detail = super::list_density::font_pixels(logical - 4).min(28);
        Self {
            title,
            line_height: (title * 13 + 5) / 10,
            detail,
            detail_height: (detail * 13 + 5) / 10,
        }
    }
}

impl ResultRow {
    fn measure(
        canvas: &mut Canvas,
        text: ResultText<'_>,
        max_lines: usize,
        font: ResultFont,
    ) -> Self {
        let glyph_bounds = Rect::new(0, 0, TEXT_WIDTH, font.line_height);
        // Measure with the result's font and a bold upper bound, since Canvas's
        // line measurement API lacks rich spans. Paint bolds only the hit.
        let style = TextStyle::new(font.title).bold().break_long_words();
        let mut prefix = usize::MAX;
        let mut shown = row_excerpt(text.snippet, text.matched.clone(), text.expand, prefix);
        let mut pages = canvas.text_pages(glyph_bounds, &shown.0, style);
        // Long context before the hit should never hide it underneath the
        // ellipsis at the selected line cap.
        if pages
            .get(max_lines.saturating_sub(1))
            .is_some_and(|last| last.end <= shown.1.end)
        {
            for limit in [52, 20, 0] {
                prefix = limit;
                shown = row_excerpt(text.snippet, text.matched.clone(), text.expand, limit);
                pages = canvas.text_pages(glyph_bounds, &shown.0, style);
                if pages
                    .get(max_lines.saturating_sub(1))
                    .is_none_or(|last| last.end > shown.1.end)
                {
                    break;
                }
            }
        }
        let lines = pages.len().max(1).min(max_lines);
        let visible_end = pages.get(lines).map(|next| {
            shown.0[..next.start]
                .trim_end_matches(|ch: char| ch.is_whitespace() || ch == '…')
                .len()
        });
        let height =
            (lines as i32 * font.line_height + font.detail_height + 2 * INSET_Y + GAP).max(100);
        Self {
            bounds: Rect::new(LIST_X, 0, CARD_WIDTH, height),
            lines,
            prefix,
            visible_end,
        }
    }

    fn draw(
        &self,
        canvas: &mut Canvas,
        text: ResultText<'_>,
        caption: &str,
        font: ResultFont,
        focused: bool,
        divider: bool,
    ) {
        let row = self.bounds;
        let pressed = canvas.control_pressed(row, false);
        if pressed {
            canvas.panel(row.inset(4), 16, 2, 0);
        }
        let ink = if pressed { 255 } else { 20 };
        let detail_ink = if pressed { 255 - SECONDARY } else { SECONDARY };
        let left = row.x + 32;
        let height = self.lines as i32 * font.line_height;
        let (mut text, mut emphasis) =
            row_excerpt(text.snippet, text.matched, text.expand, self.prefix);
        if let Some(end) = self.visible_end {
            text = Cow::Owned(format!("{}…", &text[..end]));
            emphasis.end = emphasis.end.min(end);
            if emphasis.start > emphasis.end {
                emphasis = 0..0;
            }
        }
        canvas.emphasized_text(
            Rect::new(left, row.y + INSET_Y, TEXT_WIDTH, height),
            &text,
            TextStyle::new(font.title)
                .break_long_words()
                .ink(ink)
                .lines(self.lines),
            emphasis,
        );
        canvas.text(
            Rect::new(
                left,
                row.y + INSET_Y + height + GAP,
                TEXT_WIDTH,
                font.detail_height,
            ),
            caption,
            TextStyle::new(font.detail).ink(detail_ink),
        );
        chevron(
            canvas,
            Rect::new(row.right() - 72, row.y + (row.h - 40) / 2, 40, 40),
            ink,
        );
        if focused {
            canvas.fill(Rect::new(row.x + 10, row.y + 8, 4, row.h - 16), ink);
        }
        if divider {
            canvas.fill(Rect::new(left, row.bottom() - 2, row.w - 64, 2), HAIRLINE);
        }
    }
}

impl FlexibleResults {
    fn new(fonts: &[PathBuf], max_lines: u8, font: ResultFont) -> Option<Self> {
        let fonts: Vec<_> = fonts
            .iter()
            .map(|font| font.display().to_string())
            .collect();
        Some(Self {
            pages: Vec::new(),
            spill: None,
            canvas: Canvas::new(WIDTH, 1, &fonts).ok()?,
            max_lines: usize::from(max_lines),
            font,
        })
    }

    fn measured_count(&self) -> usize {
        self.pages.last().map_or(0, ResultPage::end)
    }

    fn append(&mut self, app: &Reader) -> bool {
        let order = app.search_result_order();
        let count = order.len();
        let first = if self.spill.is_some() {
            self.measured_count()
        } else {
            // A streaming batch may have ended halfway through the last page.
            // Remeasure only that page when new hits arrive.
            self.pages.last().map_or(0, |page| page.first)
        };
        if first >= count {
            return false;
        }
        if let Some(previous) = self.pages.last()
            && previous.end() > first
        {
            self.pages.pop();
        }
        let (rows, spill) = measure_rows(
            first,
            count,
            self.spill.take(),
            |index| {
                ResultRow::measure(
                    &mut self.canvas,
                    app.result_text(order.provider_index(index).expect("measured result index")),
                    self.max_lines,
                    self.font,
                )
            },
            |row| &mut row.bounds,
        );
        self.spill = spill;
        self.pages.push(ResultPage { first, rows });
        true
    }
}

impl Reader {
    pub(super) fn clear_flexible_results(&self) {
        self.flexible_results.borrow_mut().take();
    }

    fn fulltext_result_count(&self) -> usize {
        self.search_result_order().len()
    }

    fn result_text(&self, index: usize) -> ResultText<'_> {
        if self.fixed.is_some() {
            let hit = &self.pdf_hits[index];
            ResultText {
                snippet: &hit.snippet,
                matched: hit.matched.clone(),
                expand: false,
            }
        } else {
            let hit = &self.hits[index];
            ResultText {
                snippet: &hit.snippet,
                matched: hit.matched.clone(),
                expand: true,
            }
        }
    }

    fn result_detail(&self, index: usize) -> Cow<'_, str> {
        if let Some(fixed) = &self.fixed {
            Cow::Owned(fixed_page_label(&fixed.reading, self.pdf_hits[index].page))
        } else {
            Cow::Borrowed(&self.hits[index].chapter_title)
        }
    }

    fn with_flexible_results<T>(
        &self,
        use_view: impl FnOnce(&mut FlexibleResults) -> T,
    ) -> Option<T> {
        let lines = self.search_max_lines()?;
        let mut view = self.flexible_results.borrow_mut();
        if view.is_none() {
            *view = FlexibleResults::new(
                &self.fonts,
                lines,
                ResultFont::new(self.match_geometry().rows as u8),
            );
        }
        let view = view.as_mut()?;
        if view.spill.is_none() && view.measured_count() < self.fulltext_result_count() {
            view.append(self);
        }
        Some(use_view(view))
    }

    pub(super) fn match_page_for_index(&self, index: usize) -> usize {
        let Some(ordinal) = self.search_result_order().ordinal(index) else {
            return 0;
        };
        self.with_flexible_results(|view| {
            while view.measured_count() <= ordinal
                && view.measured_count() < self.fulltext_result_count()
            {
                if !view.append(self) {
                    break;
                }
            }
            view.pages
                .partition_point(|page| page.end() <= ordinal)
                .min(view.pages.len().saturating_sub(1))
        })
        .unwrap_or_else(|| ordinal / self.match_geometry().rows)
    }

    pub(super) fn flexible_result_range(&self) -> Option<Range<usize>> {
        self.with_flexible_results(|view| {
            view.pages
                .get(self.list_page)
                .map_or(0..0, |page| page.first..page.end())
        })
    }

    pub(super) fn draw_flexible_results(&self, canvas: &mut Canvas) -> bool {
        self.with_flexible_results(|view| {
            let order = self.search_result_order();
            let count = order.len();
            let Some(page) = view.pages.get(self.list_page) else {
                canvas.text(
                    Rect::new(LIST_X, LIST_Y, CARD_WIDTH, 110),
                    self.search_results_empty_label(),
                    BODY,
                );
                self.draw_flexible_result_notice(canvas);
                return;
            };
            if let Some(last) = page.rows.last() {
                canvas.card(Rect::new(
                    LIST_X,
                    LIST_Y,
                    CARD_WIDTH,
                    last.bounds.bottom() - LIST_Y,
                ));
            }
            for (offset, row) in page.rows.iter().enumerate() {
                let Some(index) = order.provider_index(page.first + offset) else {
                    continue;
                };
                row.draw(
                    canvas,
                    self.result_text(index),
                    &self.result_detail(index),
                    view.font,
                    self.focused_result == Some(index),
                    offset + 1 < page.rows.len(),
                );
            }
            if page.end() < count || self.list_page > 0 {
                for (rect, icon, available) in [
                    (FIRST_MATCH_PAGE, bar_icons::FIRST_PAGE, self.list_page > 0),
                    (
                        PREV_MATCH_PAGE,
                        bar_icons::PREVIOUS_PAGE,
                        self.list_page > 0,
                    ),
                    (NEXT_MATCH_PAGE, bar_icons::NEXT_PAGE, page.end() < count),
                    (LAST_MATCH_PAGE, bar_icons::LAST_PAGE, page.end() < count),
                ] {
                    if available {
                        let pressed = canvas.control_pressed(pager_hit(rect), false);
                        canvas.button(rect, "", false, pressed);
                    }
                    canvas.icon(
                        Rect::new(
                            rect.x + (rect.w - 42) / 2,
                            rect.y + (rect.h - 42) / 2,
                            42,
                            42,
                        ),
                        icon,
                        if canvas.is_control_pressed(rect)
                            || canvas.is_control_pressed(pager_hit(rect))
                        {
                            255
                        } else if available {
                            20
                        } else {
                            178
                        },
                    );
                }
                let more = if self.nearby.searching_all() || self.search_at.is_some() {
                    "+"
                } else {
                    ""
                };
                canvas.text(
                    Rect::new(435, HEIGHT as i32 - 104, WIDTH as i32 - 870, 72),
                    &format!("{}–{} of {count}{more}", page.first + 1, page.end()),
                    CAPTION,
                );
            }
            self.draw_flexible_result_notice(canvas);
        })
        .is_some()
    }

    pub(super) fn search_results_empty_label(&self) -> &'static str {
        if self.search_at.is_some() && self.fixed.is_some() {
            "Searching pages"
        } else if self.search_at.is_some() || self.nearby.searching_all() {
            "Searching chapters"
        } else {
            "No matches"
        }
    }

    fn draw_flexible_result_notice(&self, canvas: &mut Canvas) {
        if let Some(notice) = &self.notice {
            canvas.text(
                Rect::new(LIST_X, HEIGHT as i32 - 179, CARD_WIDTH, 64),
                notice,
                CAPTION,
            );
        }
        if let Some(popup) = &self.result_context {
            popup.draw(canvas);
        }
    }

    pub(super) fn flexible_result_input(&mut self, input: Input) -> Option<Change> {
        if self.search_max_lines().is_none() || self.screen != Screen::Matches {
            return None;
        }
        if matches!(input, Input::Tap(x, y) if primitive_ui::back_button(WIDTH as i32).contains(x, y))
        {
            return None;
        }
        let page = self.list_page;
        let target = self.with_flexible_results(|view| {
            let move_page = |next: bool| {
                if next {
                    page.checked_add(1)
                } else {
                    page.checked_sub(1)
                }
            };
            let target_page = match input {
                Input::PageBack => move_page(false),
                Input::PageForward => move_page(true),
                Input::Swipe { dx, dy, .. } if dy.abs() > dx.abs() => move_page(dy < 0.0),
                Input::Tap(x, y) if pager_hit(FIRST_MATCH_PAGE).contains(x, y) => Some(0),
                Input::Tap(x, y) if pager_hit(PREV_MATCH_PAGE).contains(x, y) => move_page(false),
                Input::Tap(x, y) if pager_hit(NEXT_MATCH_PAGE).contains(x, y) => move_page(true),
                Input::Tap(x, y) if pager_hit(LAST_MATCH_PAGE).contains(x, y) => {
                    while view.measured_count() < self.fulltext_result_count() && view.append(self)
                    {
                    }
                    view.pages.len().checked_sub(1)
                }
                _ => None,
            };
            if let Some(target) = target_page {
                if target == view.pages.len()
                    && view.measured_count() < self.fulltext_result_count()
                {
                    view.append(self);
                }
                return (target < view.pages.len() && target != page)
                    .then_some(Target::Page(target));
            }
            let provider = |x, y| {
                let ordinal = view.pages.get(page)?.hit(x, y)?;
                self.search_result_order().provider_index(ordinal)
            };
            match input {
                Input::Tap(x, y) => provider(x, y).map(Target::Open),
                Input::LongTap(x, y) => provider(x, y).map(Target::Preview),
                _ => None,
            }
        })?;
        Some(match target {
            Some(Target::Page(page)) => {
                self.list_page = page;
                Change::Partial
            }
            Some(Target::Open(index)) => {
                let action = if self.fixed.is_some() {
                    Action::PdfSearchHit(index)
                } else {
                    Action::SearchHit(index)
                };
                match self.activate(action) {
                    Ok(()) => self.applied_change(),
                    Err(error) => self.report(error),
                }
            }
            Some(Target::Preview(index)) => match self.preview_result(index) {
                Ok(()) => Change::Partial,
                Err(error) => self.report(error),
            },
            None => Change::None,
        })
    }
}

enum Target {
    Page(usize),
    Open(usize),
    Preview(usize),
}

fn pager_hit(face: Rect) -> Rect {
    Rect::new(face.x, face.y - 14, face.w, face.h + 28)
}

#[cfg(test)]
mod tests;
