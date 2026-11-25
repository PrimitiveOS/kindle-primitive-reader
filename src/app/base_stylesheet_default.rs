//! Per-format defaults for the bottom bar's base stylesheet choices.
use primitive_app::{Change, HEIGHT, WIDTH};
use primitive_ui::{Canvas, Input, Rect, Sheet, SheetAction};

use super::{Action, Reader};
use crate::BaseStylesheet;

fn message(style: BaseStylesheet, fictionbook: bool) -> String {
    let scope = if fictionbook {
        "FictionBook documents"
    } else {
        "new documents"
    };
    let label = if fictionbook && style == BaseStylesheet::FictionBook {
        "FB2 / FB3"
    } else {
        style.label()
    };
    format!("Set {label} as the default style for {scope}?")
}

impl Reader {
    pub(super) fn draw_base_stylesheet_default_prompt(&self, canvas: &mut Canvas) {
        if let Some((style, fictionbook)) = self.base_stylesheet_default_prompt {
            let detail = message(style, fictionbook);
            Sheet::centered(
                Rect::new(0, 0, WIDTH as i32, HEIGHT as i32),
                "Default base stylesheet",
                &detail,
                "Set as default",
            )
            .draw(canvas, None);
        }
    }

    pub(super) fn base_stylesheet_default_input(&mut self, input: Input) -> Option<Change> {
        let (style, fictionbook) = self.base_stylesheet_default_prompt?;
        let detail = message(style, fictionbook);
        let sheet = Sheet::centered(
            Rect::new(0, 0, WIDTH as i32, HEIGHT as i32),
            "Default base stylesheet",
            &detail,
            "Set as default",
        );
        let confirm = match input {
            Input::Sleep | Input::Wake => return None,
            Input::PageBack => false,
            Input::Tap(x, y) => match sheet.hit(x, y) {
                Some(SheetAction::Cancel) => false,
                Some(SheetAction::Confirm) => true,
                None => return Some(Change::None),
            },
            _ => return Some(Change::None),
        };
        self.base_stylesheet_default_prompt = None;
        if confirm {
            let mut controls = self.controls;
            let selected = (style != BaseStylesheet::Auto).then_some(style);
            if fictionbook {
                controls.fictionbook_stylesheet_default = selected;
            } else {
                controls.base_stylesheet_default = selected;
            }
            if let Err(error) = self.activate(Action::SetReadingControls(controls)) {
                return Some(self.report(error));
            }
        }
        Some(Change::Partial)
    }
}
