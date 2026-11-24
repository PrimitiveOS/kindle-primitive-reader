//! Reader-wide font size for the alternate status header.
use primitive_app::{Change, HEIGHT, WIDTH};
use primitive_ui::{
    CAPTION, Canvas, Editor, Input, Layer, Layout, Press, Rect, TextStyle, keyboard_key,
};

use super::{Action, Reader, step_buttons::StepButtons};
use crate::reading_controls::{ReadingControls, TopHeader};

const SHEET: Rect = Rect::new(
    (WIDTH as i32 - 864) / 2,
    (HEIGHT as i32 - 514) / 2,
    864,
    514,
);
const TITLE: Rect = Rect::new(SHEET.x + 48, SHEET.y + 38, SHEET.w - 96, 64);
const DECREASE: Rect = Rect::new(SHEET.x + 48, SHEET.y + 114, 184, 104);
const INCREASE: Rect = Rect::new(SHEET.x + SHEET.w - 232, DECREASE.y, 184, DECREASE.h);
const VALUE: Rect = Rect::new(DECREASE.x + DECREASE.w + 24, DECREASE.y, 352, DECREASE.h);
const DEFAULT: Rect = Rect::new(SHEET.x + 48, SHEET.y + 234, SHEET.w - 96, 100);
const CLOSE: Rect = Rect::new(SHEET.x + 48, SHEET.y + 362, 372, 104);
const SET: Rect = Rect::new(CLOSE.x + CLOSE.w + 24, CLOSE.y, CLOSE.w, CLOSE.h);
const STEPS: StepButtons = StepButtons::new(DECREASE, INCREASE);
const HOLD_STEP: i16 = 4;

const ENTRY: Rect = Rect::new(SHEET.x, 472, SHEET.w, 423);
const ENTRY_TITLE: Rect = Rect::new(ENTRY.x + 48, ENTRY.y + 32, ENTRY.w - 96, 66);
const ENTRY_FIELD: Rect = Rect::new(ENTRY.x + 48, ENTRY.y + 118, ENTRY.w - 96, 104);
const ENTRY_HINT: Rect = Rect::new(ENTRY.x + 48, ENTRY.y + 236, ENTRY.w - 96, 48);
const ENTRY_CANCEL: Rect = Rect::new(ENTRY.x + 48, ENTRY.y + 288, CLOSE.w, CLOSE.h);
const ENTRY_SET: Rect = Rect::new(
    ENTRY_CANCEL.x + ENTRY_CANCEL.w + 24,
    ENTRY_CANCEL.y,
    CLOSE.w,
    CLOSE.h,
);
const ENTRY_USE: &str = "Use size";

struct Entry {
    editor: Editor,
    invalid: bool,
}

pub(super) struct Picker {
    size: u8,
    entry: Option<Entry>,
}

impl Picker {
    fn enter_size(&mut self) {
        let mut editor = Editor::new(2);
        editor.layer = Layer::Lower;
        self.entry = Some(Entry {
            editor,
            invalid: false,
        });
    }

    fn draw_entry(&self, canvas: &mut Canvas, entry: &Entry) {
        canvas.popup(ENTRY);
        canvas.text(ENTRY_TITLE, "Enter a size", TextStyle::new(50).bold());
        canvas.single_line_field(ENTRY_FIELD, &entry.editor, &self.size.to_string(), false);
        canvas.text(
            ENTRY_HINT,
            if entry.invalid {
                "Enter a whole number from 8 to 36"
            } else {
                "8–36 px"
            },
            CAPTION,
        );
        canvas.button(ENTRY_CANCEL, "Cancel", false, false);
        canvas.button(ENTRY_SET, ENTRY_USE, true, false);
        canvas.keyboard(entry.editor.layer, ENTRY_USE, None);
    }

    fn entry_input(&mut self, input: Input) -> Change {
        let Some(entry) = &mut self.entry else {
            return Change::None;
        };
        if matches!(input, Input::PageBack)
            || matches!(input, Input::Tap(x, y) if ENTRY_CANCEL.contains(x, y))
        {
            self.entry = None;
            return Change::Partial;
        }
        let Input::Tap(x, y) = input else {
            return Change::None;
        };
        let layout = Layout::new(WIDTH, HEIGHT, 4, 5);
        if !ENTRY.contains(x, y) && !layout.keyboard_area().contains(x, y) {
            self.entry = None;
            return Change::Partial;
        }
        let submit = if ENTRY_SET.contains(x, y) {
            true
        } else if let Some(key) = keyboard_key(&layout, &entry.editor, ENTRY_USE, x, y) {
            matches!(entry.editor.press(key), Press::Go)
        } else {
            return Change::None;
        };
        if submit {
            if let Some(size) = entry
                .editor
                .string()
                .trim()
                .parse::<u8>()
                .ok()
                .filter(|size| (TopHeader::FONT_MIN..=TopHeader::FONT_MAX).contains(size))
            {
                self.size = size;
                self.entry = None;
            } else {
                entry.invalid = true;
            }
        } else {
            entry.invalid = false;
        }
        Change::Partial
    }
}

impl Reader {
    pub(super) fn open_header_font_picker(&mut self) {
        self.notice = None;
        self.top_header_font_picker = Some(Picker {
            size: self.controls.top_header.font_size(),
            entry: None,
        });
    }

    pub(super) fn draw_header_font_picker(&self, canvas: &mut Canvas) {
        let Some(picker) = &self.top_header_font_picker else {
            return;
        };
        if let Some(entry) = &picker.entry {
            picker.draw_entry(canvas, entry);
            return;
        }
        canvas.popup(SHEET);
        canvas.text(TITLE, "Header font size", TextStyle::new(50).bold());
        STEPS.draw(canvas);
        canvas.button(VALUE, &format!("{} px", picker.size), false, false);
        canvas.button(
            DEFAULT,
            &format!("Default: {} px", TopHeader::FONT_PICKER_DEFAULT),
            false,
            false,
        );
        canvas.button(CLOSE, "Close", false, false);
        if picker.size != self.controls.top_header.font_size() {
            canvas.button(SET, "Set size", true, false);
        } else {
            canvas.panel(SET, 16, 2, 247);
            canvas.text(
                Rect::new(SET.x, SET.y + 28, SET.w, 55),
                "Set size",
                TextStyle::new(36).bold().center().ink(85),
            );
        }
    }

    pub(super) fn header_font_picker_input(&mut self, input: Input) -> Option<Change> {
        let picker = self.top_header_font_picker.as_mut()?;
        if matches!(input, Input::Sleep | Input::Wake) {
            return None;
        }
        if picker.entry.is_some() {
            return Some(picker.entry_input(input));
        }
        if let Input::Tap(x, y) = input {
            if CLOSE.contains(x, y) || !SHEET.contains(x, y) {
                self.top_header_font_picker = None;
                return Some(Change::Partial);
            }
            if VALUE.contains(x, y) {
                picker.enter_size();
                return Some(Change::Partial);
            }
            if DEFAULT.contains(x, y) {
                let changed = picker.size != TopHeader::FONT_PICKER_DEFAULT;
                picker.size = TopHeader::FONT_PICKER_DEFAULT;
                return Some(if changed {
                    Change::Partial
                } else {
                    Change::None
                });
            }
            if SET.contains(x, y) {
                let font_size = picker.size;
                if font_size == self.controls.top_header.font_size() {
                    return Some(Change::None);
                }
                let controls = ReadingControls {
                    top_header: TopHeader {
                        font_size,
                        ..self.controls.top_header
                    },
                    ..self.controls
                };
                return Some(match self.activate(Action::SetReadingControls(controls)) {
                    Ok(()) => {
                        self.notice = None;
                        Change::Partial
                    }
                    Err(error) => self.report(error),
                });
            }
        }
        let delta = match input {
            Input::PageBack => Some(-1),
            Input::PageForward => Some(1),
            _ => STEPS.delta(input, HOLD_STEP),
        };
        let Some(delta) = delta else {
            return Some(Change::None);
        };
        let size = (i16::from(picker.size) + delta).clamp(
            i16::from(TopHeader::FONT_MIN),
            i16::from(TopHeader::FONT_MAX),
        ) as u8;
        if size == picker.size {
            return Some(Change::None);
        }
        picker.size = size;
        Some(Change::Partial)
    }
}

#[cfg(test)]
mod tests;
