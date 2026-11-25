//! Calendar month shortcuts and the exact civil-month editor.
use super::*;
use crate::app::bar_icons::{FIRST_PAGE, LAST_PAGE, NEXT_PAGE, PREVIOUS_PAGE};
use crate::app::{Layout, keyboard_key};
use primitive_ui::{Editor, Press};

const FIRST: Rect = PREVIOUS;
const MONTH_PREVIOUS: Rect = Rect::new(208, 146, 136, 76);
const MONTH: Rect = Rect::new(360, 146, 544, 76);
const MONTH_NEXT: Rect = Rect::new(920, 146, 136, 76);
const LATEST: Rect = NEXT;

pub(super) struct Entry {
    editor: Editor,
    invalid: bool,
}

struct EditorLayout {
    panel: Rect,
    title: Rect,
    cancel: Rect,
    field: Rect,
    clear: Rect,
    hint: Rect,
    keyboard: Layout,
}

fn target_month(text: &str) -> Option<Date> {
    let (year, month) = text.split_once('-')?;
    if year.len() != 4
        || month.len() != 2
        || !year
            .bytes()
            .chain(month.bytes())
            .all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    Date::new(year.parse().ok()?, month.parse().ok()?, 1)
}

impl Reader {
    fn month_editor_layout(&self) -> EditorLayout {
        let (width, height) = self.reading_viewport().dimensions();
        let w = (width as i32 - 80).min(980);
        let x = (width as i32 - w) / 2;
        EditorLayout {
            panel: Rect::new(x, 148, w, 328),
            title: Rect::new(x + 34, 176, w - 286, 78),
            cancel: Rect::new(x + w - 234, 167, 200, 100),
            field: Rect::new(x + 34, 292, w - 260, 100),
            clear: Rect::new(x + w - 210, 292, 176, 100),
            hint: Rect::new(x + 39, 405, w - 78, 52),
            keyboard: Layout::new(width, height, 4, 5),
        }
    }

    fn history_month_edges(&self) -> (Option<Date>, Option<Date>) {
        let visible = self.history_view.month;
        let first = self.history_first_month();
        let latest = Date::today().first();
        (
            (visible > first).then_some(first),
            (visible < latest || self.reading_history.options().future_months).then_some(latest),
        )
    }

    pub(super) fn draw_history_month_navigation(&self, canvas: &mut Canvas) {
        let (first, latest) = self.history_month_edges();
        for (rect, icon, enabled) in [
            (FIRST, FIRST_PAGE, first.is_some()),
            (
                MONTH_PREVIOUS,
                PREVIOUS_PAGE,
                self.history_month_step(-1).is_some(),
            ),
            (MONTH_NEXT, NEXT_PAGE, self.history_month_step(1).is_some()),
            (LATEST, LAST_PAGE, latest.is_some()),
        ] {
            if enabled {
                super::calendar::history_arrow(canvas, rect, icon);
            }
        }
        canvas.button(MONTH, &self.history_view.month.month_label(), false, false);
    }

    pub(super) fn draw_history_month_editor(&self, canvas: &mut Canvas) {
        let Some(entry) = &self.history_view.month_entry else {
            return;
        };
        let language = self.controls.interface_language;
        let face = self.month_editor_layout();
        canvas.popup(face.panel);
        canvas.text(
            face.title,
            language.history_month_text("Enter month"),
            BODY.bold(),
        );
        canvas.button(
            face.cancel,
            language.history_month_text("Cancel"),
            false,
            false,
        );
        canvas.single_line_field(face.field, &entry.editor, "YYYY-MM", false);
        if !entry.editor.text.is_empty() {
            canvas.button(
                face.clear,
                language.history_month_text("Clear"),
                false,
                false,
            );
        }
        if entry.invalid {
            canvas.text(
                face.hint,
                language.history_month_text("Use YYYY-MM with a month from 01 to 12"),
                CAPTION,
            );
        }
        canvas.keyboard(
            entry.editor.layer,
            language.history_month_text("Go to month"),
            None,
        );
    }

    pub(super) fn history_month_navigation_input(&mut self, input: Input) -> Option<Change> {
        if self.history_view.month_entry.is_some() {
            return Some(self.history_month_editor_input(input));
        }
        match input {
            Input::Tap(x, y) | Input::LongTap(x, y) if MONTH.contains(x, y) => {
                let mut editor = Editor::new(7);
                let month = self.history_view.month;
                editor.text = format!("{:04}-{:02}", month.year, month.month)
                    .chars()
                    .collect();
                editor.caret = editor.text.len();
                self.history_view.month_entry = Some(Entry {
                    editor,
                    invalid: false,
                });
                Some(Change::Partial)
            }
            Input::Tap(x, y) if FIRST.contains(x, y) || LATEST.contains(x, y) => {
                let (first, latest) = self.history_month_edges();
                let target = if FIRST.contains(x, y) { first } else { latest }?;
                self.history_view.month = target;
                Some(Change::Partial)
            }
            Input::Tap(x, y) if MONTH_PREVIOUS.contains(x, y) || MONTH_NEXT.contains(x, y) => {
                let direction = if MONTH_PREVIOUS.contains(x, y) { -1 } else { 1 };
                let target = self.history_month_step(direction)?;
                self.history_view.month = target;
                Some(Change::Partial)
            }
            _ => None,
        }
    }

    fn history_month_editor_input(&mut self, input: Input) -> Change {
        if input == Input::PageBack {
            self.history_view.month_entry = None;
            return Change::Partial;
        }
        let Input::Tap(x, y) = input else {
            return Change::None;
        };
        let action_label = self
            .controls
            .interface_language
            .history_month_text("Go to month");
        let face = self.month_editor_layout();
        if face.cancel.contains(x, y)
            || !face.panel.contains(x, y) && !face.keyboard.keyboard_area().contains(x, y)
        {
            self.history_view.month_entry = None;
            return Change::Partial;
        }
        let Some(entry) = self.history_view.month_entry.as_mut() else {
            return Change::None;
        };
        if face.clear.contains(x, y) && !entry.editor.text.is_empty() {
            entry.editor.text.clear();
            entry.editor.caret = 0;
            entry.invalid = false;
            return Change::Partial;
        }
        let Some(key) = keyboard_key(&face.keyboard, &entry.editor, action_label, x, y) else {
            return Change::None;
        };
        if matches!(entry.editor.press(key), Press::Go) {
            if let Some(target) = target_month(&entry.editor.string()) {
                self.history_view.month = target;
                self.history_view.month_entry = None;
            } else {
                entry.invalid = true;
            }
        } else {
            entry.invalid = false;
        }
        Change::Partial
    }
}

#[cfg(test)]
mod tests;
