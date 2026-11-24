//! Reader-wide, transactional controls for the Contents list.
use primitive_ui::{SECONDARY, TextStyle};

use super::{Action, Canvas, Change, Input, Item, Reader, Rect, WIDTH};
use crate::logical_font::ScaleContext;
use crate::reading_controls::ContentsDensity;

const PANEL: Rect = Rect::new(192, 480, 880, 710);
const DECREASE: Rect = Rect::new(244, 675, 112, 112);
const INCREASE: Rect = Rect::new(908, 675, 112, 112);
const DEFAULT: Rect = Rect::new(244, 817, 776, 108);
const CANCEL: Rect = Rect::new(244, 1025, 376, 112);
const APPLY: Rect = Rect::new(644, 1025, 376, 112);

#[derive(Clone, Copy)]
pub(super) enum Field {
    Rows,
    Font,
}

impl Field {
    fn title(self) -> &'static str {
        match self {
            Self::Rows => "Rows per page",
            Self::Font => "Entry font size",
        }
    }

    fn bounds(self) -> (u8, u8) {
        match self {
            Self::Rows => (ContentsDensity::MIN_ROWS, ContentsDensity::MAX_ROWS),
            Self::Font => (ContentsDensity::MIN_FONT, ContentsDensity::MAX_FONT),
        }
    }

    fn default_value(self, density: ContentsDensity) -> u8 {
        match self {
            Self::Rows => ContentsDensity::DEFAULT_ROWS,
            Self::Font => density.automatic_font(),
        }
    }
}

pub(super) struct Picker {
    field: Field,
    value: Option<u8>,
}

impl Picker {
    fn value(&self, density: ContentsDensity) -> u8 {
        self.value
            .unwrap_or_else(|| self.field.default_value(density))
    }
}

impl Reader {
    pub(super) fn contents_settings_items(&self) -> Vec<Item> {
        let density = self.controls.contents_density;
        let font = density.explicit_font().map_or_else(
            || format!("Automatic ({})", density.automatic_font()),
            |value| value.to_string(),
        );
        vec![
            Item::new(
                Field::Rows.title(),
                Action::EditContentsDensity(Field::Rows),
            )
            .detail(density.rows().to_string()),
            Item::new(
                Field::Font.title(),
                Action::EditContentsDensity(Field::Font),
            )
            .detail(font),
        ]
    }

    pub(super) fn open_contents_density(&mut self, field: Field) {
        let density = self.controls.contents_density;
        let value = match field {
            Field::Rows => Some(density.rows()),
            Field::Font => density.explicit_font(),
        };
        self.notice = None;
        self.contents_density_picker = Some(Picker { field, value });
    }

    pub(super) fn draw_contents_density_picker(&self, canvas: &mut Canvas) {
        let Some(picker) = &self.contents_density_picker else {
            return;
        };
        let density = self.controls.contents_density;
        let (min, max) = picker.field.bounds();
        canvas.popup(PANEL);
        canvas.text(
            Rect::new(244, 534, 776, 68),
            picker.field.title(),
            TextStyle::new(50).bold().center(),
        );
        canvas.text(
            Rect::new(244, 607, 776, 49),
            &format!("{min} to {max}"),
            TextStyle::new(30).ink(SECONDARY).center(),
        );
        canvas.button(DECREASE, "-", false, false);
        canvas.button(INCREASE, "+", false, false);
        canvas.text(
            Rect::new(371, 686, 522, 105),
            &picker.value(density).to_string(),
            TextStyle::new(66).bold().center(),
        );
        let default_value = picker.field.default_value(density);
        let default_label = match picker.field {
            Field::Rows => format!("Default ({default_value})"),
            Field::Font => format!("Automatic ({default_value})"),
        };
        canvas.button(DEFAULT, &default_label, false, false);
        if let Some(notice) = &self.notice {
            canvas.text(
                Rect::new(244, 937, 776, 79),
                notice,
                TextStyle::new(28).ink(SECONDARY).lines(2).center(),
            );
        }
        canvas.button(CANCEL, "Cancel", false, false);
        canvas.button(APPLY, "Apply", true, false);
    }

    pub(super) fn contents_density_input(&mut self, input: Input) -> Option<Change> {
        let picker = self.contents_density_picker.as_mut()?;
        let density = self.controls.contents_density;
        let step = match input {
            Input::Tap(x, y) if CANCEL.contains(x, y) || !PANEL.contains(x, y) => {
                self.contents_density_picker = None;
                self.notice = None;
                return Some(Change::Partial);
            }
            Input::Tap(x, y) if APPLY.contains(x, y) => {
                let mut controls = self.controls;
                match picker.field {
                    Field::Rows => controls.contents_density.rows_per_page = picker.value,
                    Field::Font => controls.contents_density.font_size = picker.value,
                }
                return Some(match self.activate(Action::SetReadingControls(controls)) {
                    Ok(()) => {
                        self.contents_density_picker = None;
                        self.notice = None;
                        Change::Partial
                    }
                    Err(error) => self.report(error),
                });
            }
            Input::Tap(x, y) if DEFAULT.contains(x, y) => {
                picker.value = None;
                self.notice = None;
                return Some(Change::Partial);
            }
            Input::Tap(x, y) if DECREASE.contains(x, y) => -1,
            Input::Tap(x, y) if INCREASE.contains(x, y) => 1,
            Input::LongTap(x, y) if DECREASE.contains(x, y) => -4,
            Input::LongTap(x, y) if INCREASE.contains(x, y) => 4,
            Input::PageBack => -1,
            Input::PageForward => 1,
            _ => return Some(Change::None),
        };
        let prior = picker.value(density);
        let (min, max) = picker.field.bounds();
        let adjusted = prior.saturating_add_signed(step).clamp(min, max);
        if prior == adjusted {
            return Some(Change::None);
        }
        picker.value = Some(adjusted);
        self.notice = None;
        Some(Change::Partial)
    }
}

/// The app kit renders into a fixed portrait canvas before device scaling.
fn reference_scale() -> ScaleContext {
    ScaleContext::new(WIDTH, super::HEIGHT)
}

pub(super) fn list_text_pixels(logical: u8, row_height: i32) -> i32 {
    let scale = reference_scale();
    let rule = scale.scaled_pixels(1.) as i32;
    // Like KOReader's MenuItem, reserve two rules and 30% of an em for
    // leading. This caps visible ink without changing the saved preference.
    let available = (row_height - 2 * rule) * 10 / 13;
    (1..=logical)
        .rev()
        .map(|size| scale.scaled_pixels(f64::from(size)) as i32)
        .find(|&size| size <= available)
        .unwrap_or(1)
}

#[cfg(test)]
mod tests;
