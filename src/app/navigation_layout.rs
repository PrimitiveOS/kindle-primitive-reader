//! Geometry shared by the in-book destination editor and navigation lists.
use super::{Canvas, HEIGHT, LIST_X, Layout, NEXT, PREV, Reader, Rect, Screen, WIDTH};

#[derive(Clone, Copy)]
pub(super) struct NavigationLayout {
    pub(super) width: i32,
    pub(super) height: i32,
}

impl NavigationLayout {
    pub(super) const PORTRAIT: Self = Self {
        width: WIDTH as i32,
        height: HEIGHT as i32,
    };

    pub(super) fn keyboard(self) -> Layout {
        Layout::new(self.width as u32, self.height as u32, 4, 5)
    }

    pub(super) fn back(self) -> Rect {
        Rect::new(self.width - 260, 40, 220, 100)
    }

    pub(super) fn header(self, canvas: &mut Canvas, title: &str) {
        self.header_action(canvas, title, "Back");
    }

    pub(super) fn header_action(self, canvas: &mut Canvas, title: &str, action: &str) {
        canvas.text(
            Rect::new(40, 47, self.width - 354, 110),
            title,
            primitive_ui::TITLE,
        );
        canvas.button(self.back(), action, true, false);
    }

    pub(super) fn body(self, y: i32, height: i32) -> Rect {
        Rect::new(LIST_X, y, self.width - 2 * LIST_X, height)
    }

    pub(super) fn previous(self) -> Rect {
        Rect::new(PREV.x, self.bottom_y(PREV.y), PREV.w, PREV.h)
    }

    pub(super) fn next(self) -> Rect {
        Rect::new(
            self.width - (WIDTH as i32 - NEXT.x),
            self.bottom_y(NEXT.y),
            NEXT.w,
            NEXT.h,
        )
    }

    pub(super) fn centered_footer(self, face: Rect) -> Rect {
        Rect::new(
            face.x + (self.width - WIDTH as i32) / 2,
            self.bottom_y(face.y),
            face.w,
            face.h,
        )
    }

    pub(super) fn footer_text(self, margin: i32, inset: i32, height: i32) -> Rect {
        Rect::new(margin, self.height - inset, self.width - 2 * margin, height)
    }

    fn bottom_y(self, portrait_y: i32) -> i32 {
        self.height - (HEIGHT as i32 - portrait_y)
    }
}

impl Screen {
    pub(super) fn uses_navigation_viewport(self) -> bool {
        self.is_contents()
            || self.uses_reading_text_editor()
            || self.is_bookmark_list()
            || matches!(
                self,
                Self::GoToPage
                    | Self::ContentsSearch
                    | Self::LibraryFind
                    | Self::Find
                    | Self::Matches
                    | Self::BookmarkFind
                    | Self::BookmarkMatches
                    | Self::BookMap
                    | Self::PageOverview
                    | Self::BookmarkBrowser(_)
            )
    }
}

impl Reader {
    pub(super) fn navigation_layout(&self) -> NavigationLayout {
        let (width, height) = self.reading_viewport().dimensions();
        NavigationLayout {
            width: width as i32,
            height: height as i32,
        }
    }
}

#[cfg(test)]
mod tests;
