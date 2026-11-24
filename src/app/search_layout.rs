//! Shared drawing and touch bounds for document search.
use super::{HEIGHT, LIST_X, LIST_Y, Reader, Rect, WIDTH};
use crate::session::ReadingViewport;

#[derive(Clone, Copy)]
pub(super) struct SearchLayout {
    pub(super) viewport: ReadingViewport,
}

impl SearchLayout {
    pub(super) const PORTRAIT: Self = Self {
        viewport: ReadingViewport::PORTRAIT,
    };

    pub(super) fn width(self) -> i32 {
        self.viewport.dimensions().0 as i32
    }

    pub(super) fn height(self) -> i32 {
        self.viewport.dimensions().1 as i32
    }

    pub(super) fn body(self, y: i32, height: i32) -> Rect {
        Rect::new(LIST_X, y, self.width() - 2 * LIST_X, height)
    }

    pub(super) fn list_button(self, rect: Rect) -> Rect {
        let x = if rect.x >= WIDTH as i32 / 2 {
            rect.x + self.width() - WIDTH as i32
        } else {
            rect.x
        };
        Rect::new(x, rect.y + self.height() - HEIGHT as i32, rect.w, rect.h)
    }

    pub(super) fn list_caption(self, margin: i32, inset: i32, height: i32) -> Rect {
        Rect::new(
            margin,
            self.height() - inset,
            self.width() - margin * 2,
            height,
        )
    }

    pub(super) fn counter_style(self) -> primitive_ui::TextStyle {
        if self.viewport == ReadingViewport::PORTRAIT {
            primitive_ui::CAPTION
        } else {
            primitive_ui::CAPTION.center()
        }
    }

    pub(super) fn options_button(self, rect: Rect) -> Rect {
        Rect::new(rect.x + self.width() - WIDTH as i32, rect.y, rect.w, rect.h)
    }

    pub(super) fn centered(self, rect: Rect) -> Rect {
        Rect::new(
            rect.x + (self.width() - WIDTH as i32) / 2,
            rect.y + (self.height() - HEIGHT as i32) / 2,
            rect.w,
            rect.h,
        )
    }

    pub(super) fn reading_footer(self, rect: Rect) -> Rect {
        Rect::new(
            rect.x + (self.width() - WIDTH as i32) / 2,
            rect.y + self.height() - HEIGHT as i32,
            rect.w,
            rect.h,
        )
    }

    pub(super) fn find_option(self, rect: Rect) -> Rect {
        self.find_button(rect, 337)
    }

    pub(super) fn find_direction(self, rect: Rect) -> Rect {
        self.find_button(rect, 419)
    }

    fn find_button(self, rect: Rect, top: i32) -> Rect {
        if self.viewport == ReadingViewport::PORTRAIT {
            return rect;
        }
        // Reserve the same physical key area as the shared keyboard. Expanding
        // the controls horizontally keeps all search choices above it.
        let scale = |x: i32| {
            LIST_X + (x - LIST_X) * (self.width() - 2 * LIST_X) / (WIDTH as i32 - 2 * LIST_X)
        };
        let x = scale(rect.x);
        Rect::new(x, top, scale(rect.right()) - x, rect.h)
    }

    pub(super) fn find_field(self, paste: bool) -> Rect {
        let mut field = self.body(LIST_Y, 130);
        if paste && self.viewport != ReadingViewport::PORTRAIT {
            field.w -= 336;
        }
        field
    }

    pub(super) fn find_paste(self, portrait: Rect) -> Rect {
        if self.viewport == ReadingViewport::PORTRAIT {
            portrait
        } else {
            Rect::new(
                self.width() - LIST_X - portrait.w,
                LIST_Y + 12,
                portrait.w,
                106,
            )
        }
    }

    pub(super) fn find_notice(self) -> Rect {
        if self.viewport == ReadingViewport::PORTRAIT {
            self.body(LIST_Y + 158, 75)
        } else {
            self.body(285, 50)
        }
    }
}

pub(super) struct MatchWindow {
    viewport: ReadingViewport,
    ordinal: usize,
}

impl Reader {
    pub(super) fn search_layout(&self) -> SearchLayout {
        SearchLayout {
            viewport: self.reading_viewport(),
        }
    }

    pub(super) fn remember_match_window(&mut self, page: usize) {
        self.matches_page = page;
        let first = self
            .flexible_result_range_at(page)
            .map_or_else(|| page * self.match_geometry().rows, |range| range.start);
        self.match_window = Some(MatchWindow {
            viewport: self.reading_viewport(),
            ordinal: first,
        });
    }

    pub(super) fn reopen_match_window(&self) -> usize {
        if self.matches_page == 0 {
            return 0;
        }
        self.match_window
            .as_ref()
            .filter(|window| window.viewport != self.reading_viewport())
            .and_then(|window| self.search_result_order().provider_index(window.ordinal))
            .map_or(self.matches_page, |index| self.match_page_for_index(index))
    }
}
