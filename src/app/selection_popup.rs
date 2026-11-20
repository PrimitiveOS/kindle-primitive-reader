//! Small, anchored actions for the selection already painted on the reading page.

use primitive_app::{Change, WIDTH};
use primitive_ui::{CAPTION, Canvas, Editor, HAIRLINE, Rect, TextStyle};

use super::{Reader, Screen};
use crate::session::{DONE, PAGE_HEIGHT};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SelectionAction {
    Highlight,
    Note,
    Dictionary,
    Cancel,
}

impl SelectionAction {
    const ALL: [Self; 4] = [Self::Highlight, Self::Note, Self::Dictionary, Self::Cancel];

    fn label(self) -> &'static str {
        match self {
            Self::Highlight => "Highlight",
            Self::Note => "Note",
            Self::Dictionary => "Dictionary",
            Self::Cancel => "Cancel",
        }
    }
}

pub(super) struct SelectionPopup {
    rect: Rect,
    large: bool,
}

impl SelectionPopup {
    fn new(bounds: Option<Rect>, large: bool) -> Self {
        let height = if large { 80 } else { 68 };
        let width = (WIDTH as i32 - 48).min(864);
        let page_bottom = PAGE_HEIGHT as i32;
        // Leave space for the next line and for the canvas's popup border.
        let gap = 56;
        let edge = 16;
        if let Some(bounds) = bounds {
            let above = bounds.y - height - gap;
            let below = bounds.bottom() + gap;
            let y = if above >= edge {
                Some(above)
            } else if below + height <= page_bottom - edge {
                Some(below)
            } else {
                None
            };
            if let Some(y) = y {
                let x =
                    (bounds.x + bounds.w / 2 - width / 2).clamp(edge, WIDTH as i32 - width - edge);
                return Self {
                    rect: Rect::new(x, y, width, height),
                    large,
                };
            }
        }

        // A page-spanning selection can leave no clear location for the popup in the text.
        // Put its actions in the footer alongside Done and leave every selected word visible.
        let width = width.min(DONE.x - 36);
        Self {
            rect: Rect::new(16, page_bottom + 12, width, 50),
            large,
        }
    }

    fn buttons(&self) -> impl Iterator<Item = (SelectionAction, Rect)> + '_ {
        let inner = self.rect.inset(5);
        let count = SelectionAction::ALL.len() as i32;
        SelectionAction::ALL
            .into_iter()
            .enumerate()
            .map(move |(index, action)| {
                let index = index as i32;
                let left = inner.x + inner.w * index / count;
                let right = inner.x + inner.w * (index + 1) / count;
                (action, Rect::new(left, inner.y, right - left, inner.h))
            })
    }

    fn action_at(&self, x: f32, y: f32) -> Option<SelectionAction> {
        self.buttons()
            .find_map(|(action, rect)| rect.contains(x, y).then_some(action))
    }

    fn draw(&self, canvas: &mut Canvas, notice: Option<&str>) {
        let floating = self.rect.y < PAGE_HEIGHT as i32;
        canvas.fill(Rect::new(0, PAGE_HEIGHT as i32, WIDTH as i32, 1), HAIRLINE);
        if floating {
            canvas.text(
                Rect::new(38, PAGE_HEIGHT as i32 + 12, DONE.x - 62, 48),
                notice.unwrap_or("Tap another word to extend"),
                CAPTION,
            );
        }
        // The balanced border also tells Canvas to cover the page raster under this popup.
        canvas.popup(self.rect);
        let size = if self.large && floating { 34 } else { 28 };
        for (index, (action, rect)) in self.buttons().enumerate() {
            if index != 0 {
                canvas.fill(Rect::new(rect.x, rect.y + 10, 1, rect.h - 20), HAIRLINE);
            }
            if canvas.control_pressed(rect, false) {
                canvas.fill(rect.inset(2), 218);
            }
            canvas.text(
                Rect::new(rect.x, rect.y + (rect.h - size) / 2, rect.w, size + 9),
                action.label(),
                TextStyle::new(size).bold().center(),
            );
        }
        canvas.button(DONE, "Done", false, false);
    }
}

impl Reader {
    pub(super) fn selection_bounds(&self) -> Option<Rect> {
        if let (Some(fixed), Some(selection)) = (&self.fixed, &self.pdf_selection) {
            fixed.reading.selection_bounds(selection)
        } else if let (Some(active), Some(selection)) = (&self.active, &self.selection) {
            active
                .reading
                .highlight_rects(selection)
                .into_iter()
                .map(|rect| Rect::new(rect.x, rect.y, rect.width, rect.height))
                .reduce(|left, right| {
                    let x = left.x.min(right.x);
                    let y = left.y.min(right.y);
                    Rect::new(
                        x,
                        y,
                        left.right().max(right.right()) - x,
                        left.bottom().max(right.bottom()) - y,
                    )
                })
        } else {
            None
        }
    }

    fn selection_popup(&self) -> Option<SelectionPopup> {
        (self.selection.is_some() || self.pdf_selection.is_some()).then(|| {
            SelectionPopup::new(self.selection_bounds(), self.controls.large_toolbar)
        })
    }

    pub(super) fn selection_popup_input(&mut self, x: f32, y: f32) -> Option<Change> {
        if DONE.contains(x, y) {
            return Some(self.exit());
        }
        let popup = self.selection_popup()?;
        if !popup.rect.contains(x, y) {
            return None;
        }
        let result = match popup.action_at(x, y) {
            Some(SelectionAction::Highlight) if self.pdf_selection.is_some() => {
                self.save_pdf_highlight(None)
            }
            Some(SelectionAction::Highlight) => self.save_highlight(None),
            Some(SelectionAction::Note) => {
                self.editor = Editor::new(128);
                self.show(Screen::Note);
                Ok(())
            }
            Some(SelectionAction::Dictionary) => self.look_up_selection(),
            Some(SelectionAction::Cancel) => {
                self.selection = None;
                self.pdf_selection = None;
                self.notice = None;
                Ok(())
            }
            None => return Some(Change::None),
        };
        Some(match result {
            Ok(()) => Change::Partial,
            Err(error) => self.report(error),
        })
    }

    pub(super) fn draw_selection_popup(&self, canvas: &mut Canvas) {
        if let Some(popup) = self.selection_popup() {
            popup.draw(canvas, self.notice.as_deref());
        }
    }

    #[cfg(test)]
    pub(super) fn selection_button(&self, action: SelectionAction) -> Rect {
        self.selection_popup()
            .unwrap()
            .buttons()
            .find_map(|(candidate, rect)| (candidate == action).then_some(rect))
            .unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn popup_avoids_selected_text_and_the_done_control() {
        for (bounds, large) in [
            (Rect::new(0, 12, 80, 45), false),
            (Rect::new(WIDTH as i32 - 100, 760, 100, 45), true),
            (
                Rect::new(0, 8, WIDTH as i32, PAGE_HEIGHT as i32 - 16),
                false,
            ),
        ] {
            let popup = SelectionPopup::new(Some(bounds), large);
            let rect = popup.rect;
            assert!(rect.x >= 16 && rect.right() <= WIDTH as i32 - 16);
            assert!(rect.bottom() <= DONE.y || rect.right() < DONE.x);
            assert!(rect.y >= bounds.bottom() || rect.bottom() <= bounds.y);
            assert!(rect.bottom() <= primitive_app::HEIGHT as i32 - 10);
            for (action, button) in popup.buttons() {
                assert_eq!(
                    popup.action_at(
                        (button.x + button.w / 2) as f32,
                        (button.y + button.h / 2) as f32
                    ),
                    Some(action),
                );
            }
        }
    }
}
