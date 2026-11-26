//! Full-text result rows measured and packed against the active viewport.
use std::{borrow::Cow, ops::Range};

use primitive_ui::{HAIRLINE, SECONDARY, TextStyle, chevron};

use super::list_density::{LIST_FOOTER_SPACE, measure_rows};
use super::result_context::row_excerpt;
use super::search_layout::SearchLayout;
use super::*;
use crate::session::ReadingViewport;

const INSET_Y: i32 = 9;
const GAP: i32 = 4;

pub(super) struct FlexibleResults {
    pages: Vec<ResultPage>,
    spill: Option<ResultRow>,
    canvas: Canvas,
    viewport: ReadingViewport,
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
        card_width: i32,
    ) -> Self {
        let glyph_bounds = Rect::new(0, 0, card_width - 116, font.line_height);
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
            bounds: Rect::new(LIST_X, 0, card_width, height),
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
            Rect::new(left, row.y + INSET_Y, row.w - 116, height),
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
                row.w - 116,
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
    fn new(
        fonts: &[PathBuf],
        max_lines: u8,
        font: ResultFont,
        viewport: ReadingViewport,
    ) -> Option<Self> {
        let fonts: Vec<_> = fonts
            .iter()
            .map(|font| font.display().to_string())
            .collect();
        Some(Self {
            pages: Vec::new(),
            spill: None,
            canvas: Canvas::new_viewport(viewport.dimensions().0, 1, WIDTH, &fonts).ok()?,
            viewport,
            max_lines: usize::from(max_lines),
            font,
        })
    }

    fn measured_count(&self) -> usize {
        self.pages.last().map_or(0, ResultPage::end)
    }

    fn ensure_page(&mut self, app: &Reader, page: usize) {
        while self.pages.len() <= page
            && self.measured_count() < app.fulltext_result_count()
            && self.append(app)
        {}
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
        let layout = SearchLayout {
            viewport: self.viewport,
        };
        let (rows, spill) = measure_rows(
            first,
            count,
            layout.height() - LIST_FOOTER_SPACE,
            self.spill.take(),
            |index| {
                ResultRow::measure(
                    &mut self.canvas,
                    app.result_text(order.provider_index(index).expect("measured result index")),
                    self.max_lines,
                    self.font,
                    layout.width() - 2 * LIST_X,
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
        let viewport = self.reading_viewport();
        if view.as_ref().is_none_or(|view| view.viewport != viewport) {
            *view = FlexibleResults::new(
                &self.fonts,
                lines,
                ResultFont::new(self.configured_search_rows()),
                viewport,
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
        self.flexible_result_range_at(self.list_page)
    }

    pub(super) fn flexible_result_range_at(&self, page: usize) -> Option<Range<usize>> {
        self.with_flexible_results(|view| {
            view.ensure_page(self, page);
            view.pages
                .get(page)
                .map_or(0..0, |page| page.first..page.end())
        })
    }

    pub(super) fn draw_flexible_results(&self, canvas: &mut Canvas) -> bool {
        let layout = self.search_layout();
        self.with_flexible_results(|view| {
            view.ensure_page(self, self.list_page);
            let order = self.search_result_order();
            let count = order.len();
            let Some(page) = view.pages.get(self.list_page) else {
                canvas.text(
                    layout.body(LIST_Y, 110),
                    self.search_results_empty_label(),
                    BODY,
                );
                self.draw_flexible_result_notice(canvas);
                return;
            };
            if let Some(last) = page.rows.last() {
                canvas.card(layout.body(LIST_Y, last.bounds.bottom() - LIST_Y));
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
                    (
                        layout.list_button(FIRST_MATCH_PAGE),
                        bar_icons::FIRST_PAGE,
                        self.list_page > 0,
                    ),
                    (
                        layout.list_button(PREV_MATCH_PAGE),
                        bar_icons::PREVIOUS_PAGE,
                        self.list_page > 0,
                    ),
                    (
                        layout.list_button(NEXT_MATCH_PAGE),
                        bar_icons::NEXT_PAGE,
                        page.end() < count,
                    ),
                    (
                        layout.list_button(LAST_MATCH_PAGE),
                        bar_icons::LAST_PAGE,
                        page.end() < count,
                    ),
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
                    layout.list_caption(435, 104, 72),
                    &format!("{}–{} of {count}{more}", page.first + 1, page.end()),
                    layout.counter_style(),
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
        } else if self.reflow_results_completeness().is_some() {
            self.controls
                .interface_language
                .document_search_text("Search incomplete")
        } else {
            "No matches"
        }
    }

    fn draw_flexible_result_notice(&self, canvas: &mut Canvas) {
        if !self.draw_reflow_completeness_indicator(canvas)
            && let Some(notice) = self.notice.as_deref()
        {
            canvas.text(
                self.search_layout().list_caption(LIST_X, 179, 64),
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
        if matches!(input, Input::Tap(x, y) if self.reading_editor_back().contains(x, y)) {
            return None;
        }
        let page = self.list_page;
        let layout = self.search_layout();
        let target = self.with_flexible_results(|view| {
            view.ensure_page(self, page);
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
                Input::Tap(x, y)
                    if pager_hit(layout.list_button(FIRST_MATCH_PAGE)).contains(x, y) =>
                {
                    Some(0)
                }
                Input::Tap(x, y)
                    if pager_hit(layout.list_button(PREV_MATCH_PAGE)).contains(x, y) =>
                {
                    move_page(false)
                }
                Input::Tap(x, y)
                    if pager_hit(layout.list_button(NEXT_MATCH_PAGE)).contains(x, y) =>
                {
                    move_page(true)
                }
                Input::Tap(x, y)
                    if pager_hit(layout.list_button(LAST_MATCH_PAGE)).contains(x, y) =>
                {
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
                self.navigate_list_page(page);
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
