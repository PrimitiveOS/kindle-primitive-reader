//! The path editor's folder shortcut guide and expanded preview.
use super::{Canvas, Change, Input, LIST_X, LIST_Y, Name, Reader, Rect, Screen};
use crate::reading_profiles::opening::path::Folders;
use primitive_ui::TextStyle;

pub(in crate::app) enum Overlay {
    Shortcuts,
    Preview(String),
}

impl Reader {
    pub(super) fn opening_rule_path_detail(&self, pattern: &str) -> String {
        Folders::new(&self.library, self.browser.directory())
            .expand(pattern)
            .map_or_else(
                || "Expanded path is too long".into(),
                |path| path.into_owned(),
            )
    }

    fn opening_path_buttons(&self) -> Option<(Rect, Rect)> {
        if !matches!(
            self.screen,
            Screen::ProfileName(Name::OpeningPath(_) | Name::ClosingPath(_))
        ) {
            return None;
        }
        let x = self
            .current_opening_book_button()
            .map_or(LIST_X, |button| button.right() + 24);
        let help = Rect::new(x, LIST_Y + 248, 260, 100);
        let preview = Rect::new(help.right() + 24, help.y, 260, help.h);
        Some((help, preview))
    }

    fn opening_path_overlay_rects(&self) -> (Rect, Rect) {
        let view = self.navigation_layout();
        let width = (view.width - 80).min(1056);
        let panel = Rect::new(
            (view.width - width) / 2,
            (view.height - 520) / 2,
            width,
            520,
        );
        let done = Rect::new(panel.right() - 268, panel.bottom() - 144, 220, 100);
        (panel, done)
    }

    pub(in crate::app) fn opening_path_shortcut_input(&mut self, input: Input) -> Option<Change> {
        let (help, preview) = self.opening_path_buttons()?;
        if self.opening_path_overlay.is_some() {
            let (panel, done) = self.opening_path_overlay_rects();
            let close = match input {
                Input::PageBack => true,
                Input::Tap(x, y) => done.contains(x, y) || !panel.contains(x, y),
                _ => false,
            };
            if close {
                self.opening_path_overlay = None;
                return Some(Change::Partial);
            }
            return Some(Change::None);
        }
        let Input::Tap(x, y) = input else {
            return None;
        };
        self.opening_path_overlay = Some(if help.contains(x, y) {
            Overlay::Shortcuts
        } else if preview.contains(x, y) {
            let draft = self.editor.string();
            let detail = if draft.trim().is_empty() {
                "Enter a phrase to preview its expanded path.".into()
            } else {
                self.opening_rule_path_detail(draft.trim())
            };
            Overlay::Preview(detail)
        } else {
            return None;
        });
        Some(Change::Partial)
    }

    pub(in crate::app) fn draw_opening_path_shortcuts(&self, canvas: &mut Canvas) {
        let Some((help, preview)) = self.opening_path_buttons() else {
            return;
        };
        canvas.button(help, "Shortcuts", false, false);
        canvas.button(preview, "Preview", false, false);
        let Some(overlay) = &self.opening_path_overlay else {
            return;
        };
        let (title, detail) = match overlay {
            Overlay::Shortcuts => (
                "Folder shortcuts",
                "%H%  Documents\n%D%  Last browsed folder\n%S%  Reader screenshots\n%W%  Wikipedia folder",
            ),
            Overlay::Preview(detail) => ("Expanded path phrases", detail.as_str()),
        };
        let (panel, done) = self.opening_path_overlay_rects();
        canvas.popup(panel);
        canvas.text(
            Rect::new(panel.x + 48, panel.y + 35, panel.w - 96, 65),
            title,
            TextStyle::new(50).bold(),
        );
        canvas.text(
            Rect::new(panel.x + 48, panel.y + 120, panel.w - 96, 225),
            detail,
            TextStyle::new(36).ink(85).lines(5),
        );
        canvas.button(done, "Done", true, false);
    }
}

#[cfg(test)]
#[path = "opening_shortcuts_tests.rs"]
mod tests;
