//! Direct return from a reflow Book Map without changing its reading position.
use super::navigation_layout::NavigationLayout;
use super::{Canvas, Change, Input, Reader, Rect, Screen};

fn button(layout: NavigationLayout) -> Rect {
    layout.horizontal(Rect::new(
        (layout.width - 300) / 2,
        layout.height - 118,
        300,
        100,
    ))
}

impl Reader {
    pub(super) fn draw_reflow_map_return(&self, canvas: &mut Canvas) {
        if self.screen != Screen::BookMap || self.active.is_none() || self.fixed.is_some() {
            return;
        }
        canvas.button(
            button(self.navigation_layout()),
            self.controls
                .interface_language
                .book_map_text("Return to book"),
            false,
            false,
        );
    }

    pub(super) fn reflow_map_return_input(&mut self, input: Input) -> Option<Change> {
        if self.screen != Screen::BookMap || self.active.is_none() || self.fixed.is_some() {
            return None;
        }
        if matches!(input, Input::Tap(x, y) if button(self.navigation_layout()).contains(x, y)) {
            self.show(Screen::Book);
            return Some(Change::Full);
        }
        None
    }
}
