//! Shared geometry and transactional density controls for reading lists.
use primitive_ui::{SECONDARY, TextStyle};

use super::{
    Action, Canvas, Change, HEIGHT, Input, Item, LIST_X, LIST_Y, ROW_HEIGHT, ROWS_PER_PAGE, Reader,
    Rect, WIDTH,
};
use crate::logical_font::ScaleContext;
use crate::reading_controls::{ListDensity, ReadingControls};

const PANEL: Rect = Rect::new(192, 480, 880, 710);
pub(super) const DECREASE: Rect = Rect::new(244, 675, 112, 112);
pub(super) const INCREASE: Rect = Rect::new(908, 675, 112, 112);
pub(super) const DEFAULT: Rect = Rect::new(244, 817, 776, 108);
pub(super) const CANCEL: Rect = Rect::new(244, 1025, 376, 112);
pub(super) const APPLY: Rect = Rect::new(644, 1025, 376, 112);

#[derive(Clone, Copy)]
pub(super) enum Scope {
    Contents,
    Bookmarks,
}

impl Scope {
    fn preferences(self, controls: &ReadingControls) -> ListDensity {
        match self {
            Self::Contents => controls.contents_density,
            Self::Bookmarks => controls.bookmark_density,
        }
    }

    fn preferences_mut(self, controls: &mut ReadingControls) -> &mut ListDensity {
        match self {
            Self::Contents => &mut controls.contents_density,
            Self::Bookmarks => &mut controls.bookmark_density,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct Geometry {
    pub(super) rows: usize,
    pub(super) row_height: i32,
}

impl Geometry {
    pub(super) const STANDARD: Self = Self {
        rows: ROWS_PER_PAGE,
        row_height: ROW_HEIGHT,
    };

    pub(super) fn dense(density: ListDensity) -> Self {
        let rows = density.rows();
        let available_height = HEIGHT as i32 - LIST_Y - 250;
        Self {
            rows: usize::from(rows),
            row_height: available_height / i32::from(rows),
        }
    }

    pub(super) fn row(self, offset: usize) -> Rect {
        Rect::new(
            LIST_X,
            LIST_Y + offset as i32 * self.row_height,
            WIDTH as i32 - 2 * LIST_X,
            self.row_height,
        )
    }

    pub(super) fn bounds(self, shown: usize) -> Rect {
        let mut rect = self.row(0);
        rect.h *= shown as i32;
        rect
    }

    pub(super) fn item_at(self, page: usize, count: usize, x: f32, y: f32) -> Option<usize> {
        let first = page * self.rows;
        let shown = count.saturating_sub(first).min(self.rows);
        self.offset_at(shown, x, y).map(|offset| first + offset)
    }

    pub(super) fn offset_at(self, shown: usize, x: f32, y: f32) -> Option<usize> {
        self.bounds(shown)
            .contains(x, y)
            .then(|| ((y as i32 - LIST_Y) / self.row_height) as usize)
    }
}

#[derive(Clone, Copy)]
pub(super) enum Field {
    Rows,
    Font,
}

impl Field {
    fn title(self, scope: Scope) -> &'static str {
        match (self, scope) {
            (Self::Rows, _) => "Rows per page",
            (Self::Font, Scope::Contents) => "Entry font size",
            (Self::Font, Scope::Bookmarks) => "Bookmark font size",
        }
    }

    fn bounds(self) -> (u8, u8) {
        match self {
            Self::Rows => (ListDensity::MIN_ROWS, ListDensity::MAX_ROWS),
            Self::Font => (ListDensity::MIN_FONT, ListDensity::MAX_FONT),
        }
    }

    fn default_value(self, density: ListDensity) -> u8 {
        match self {
            Self::Rows => ListDensity::DEFAULT_ROWS,
            Self::Font => density.automatic_font(),
        }
    }
}

pub(super) struct Picker {
    scope: Scope,
    field: Field,
    pub(super) value: Option<u8>,
}

impl Picker {
    fn value(&self, density: ListDensity) -> u8 {
        self.value
            .unwrap_or_else(|| self.field.default_value(density))
    }
}

impl Reader {
    pub(super) fn list_density_items(&self, scope: Scope) -> [Item; 2] {
        let density = scope.preferences(&self.controls);
        let font = density.explicit_font().map_or_else(
            || format!("Automatic ({})", density.automatic_font()),
            |value| value.to_string(),
        );
        [
            Item::new(
                Field::Rows.title(scope),
                Action::EditListDensity(scope, Field::Rows),
            )
            .detail(density.rows().to_string()),
            Item::new(
                Field::Font.title(scope),
                Action::EditListDensity(scope, Field::Font),
            )
            .detail(font),
        ]
    }

    pub(super) fn open_list_density(&mut self, scope: Scope, field: Field) {
        let density = scope.preferences(&self.controls);
        let value = match field {
            Field::Rows => Some(density.rows()),
            Field::Font => density.explicit_font(),
        };
        self.notice = None;
        self.list_density_picker = Some(Picker {
            scope,
            field,
            value,
        });
    }

    pub(super) fn draw_list_density_picker(&self, canvas: &mut Canvas) {
        let Some(picker) = &self.list_density_picker else {
            return;
        };
        let density = picker.scope.preferences(&self.controls);
        let (min, max) = picker.field.bounds();
        canvas.popup(PANEL);
        canvas.text(
            Rect::new(244, 534, 776, 68),
            picker.field.title(picker.scope),
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

    pub(super) fn list_density_input(&mut self, input: Input) -> Option<Change> {
        let picker = self.list_density_picker.as_mut()?;
        let density = picker.scope.preferences(&self.controls);
        let step = match input {
            Input::Tap(x, y) if CANCEL.contains(x, y) || !PANEL.contains(x, y) => {
                self.list_density_picker = None;
                self.notice = None;
                return Some(Change::Partial);
            }
            Input::Tap(x, y) if APPLY.contains(x, y) => {
                let mut controls = self.controls;
                let preference = picker.scope.preferences_mut(&mut controls);
                match picker.field {
                    Field::Rows => preference.rows_per_page = picker.value,
                    Field::Font => preference.font_size = picker.value,
                }
                return Some(match self.activate(Action::SetReadingControls(controls)) {
                    Ok(()) => {
                        self.list_density_picker = None;
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
pub(super) fn reference_scale() -> ScaleContext {
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
