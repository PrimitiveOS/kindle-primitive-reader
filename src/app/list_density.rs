//! Shared geometry and transactional density controls for reading lists.
use primitive_ui::{SECONDARY, TextStyle};

use super::navigation_layout::NavigationLayout;
use super::{
    Action, Canvas, Change, HEIGHT, Input, Item, LIST_X, LIST_Y, ROW_HEIGHT, ROWS_PER_PAGE, Reader,
    Rect, WIDTH,
};
use crate::logical_font::ScaleContext;
use crate::reading_controls::{
    DEFAULT_BOOKMARK_LINES, FooterTextItem, ListDensity, MAX_BOOKMARK_LINES, ReadingControls,
};

const PANEL: Rect = Rect::new(192, 480, 880, 710);
pub(super) const DECREASE: Rect = Rect::new(244, 675, 112, 112);
pub(super) const INCREASE: Rect = Rect::new(908, 675, 112, 112);
pub(super) const PICKER_VALUE: Rect = Rect::new(371, 676, 522, 112);
pub(super) const DEFAULT: Rect = Rect::new(244, 817, 776, 108);
pub(super) const LINE_DEFAULT: Rect = Rect::new(244, 817, 376, 108);
pub(super) const DISABLE_LINES: Rect = Rect::new(644, 817, 376, 108);
pub(super) const CANCEL: Rect = Rect::new(244, 1025, 376, 112);
pub(super) const APPLY: Rect = Rect::new(644, 1025, 376, 112);
pub(super) const LIST_FOOTER_SPACE: i32 = 250;
pub(super) const LIST_HEIGHT: i32 = HEIGHT as i32 - LIST_Y - LIST_FOOTER_SPACE;

pub(super) fn measure_rows<R>(
    first: usize,
    count: usize,
    bottom: i32,
    mut spill: Option<R>,
    mut measure: impl FnMut(usize) -> R,
    bounds: impl Fn(&mut R) -> &mut Rect,
) -> (Vec<R>, Option<R>) {
    let mut y = LIST_Y;
    let mut rows = Vec::new();
    for index in first..count {
        let mut row = spill.take().unwrap_or_else(|| measure(index));
        let rect = bounds(&mut row);
        if y + rect.h > bottom {
            return (rows, Some(row));
        }
        rect.y = y;
        y = rect.bottom();
        rows.push(row);
    }
    (rows, None)
}

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
    width: i32,
}

impl Geometry {
    pub(super) const STANDARD: Self = Self {
        rows: ROWS_PER_PAGE,
        row_height: ROW_HEIGHT,
        width: WIDTH as i32,
    };

    pub(super) fn dense(density: ListDensity) -> Self {
        Self::with_rows(density.rows())
    }

    pub(super) fn with_rows(rows: u8) -> Self {
        Self {
            rows: usize::from(rows),
            row_height: LIST_HEIGHT / i32::from(rows),
            width: WIDTH as i32,
        }
    }

    pub(super) fn in_navigation(self, layout: super::navigation_layout::NavigationLayout) -> Self {
        self.in_navigation_reserving(layout, LIST_FOOTER_SPACE)
    }

    pub(super) fn in_navigation_reserving(
        self,
        layout: NavigationLayout,
        footer_space: i32,
    ) -> Self {
        if layout.height == HEIGHT as i32 {
            return Self {
                width: layout.width,
                ..self
            };
        }
        let available = layout.height - LIST_Y - footer_space;
        let rows = (available / self.row_height).max(1) as usize;
        Self {
            width: layout.width,
            rows: rows.min(self.rows),
            ..self
        }
    }

    pub(super) fn row(self, offset: usize) -> Rect {
        Rect::new(
            LIST_X,
            LIST_Y + offset as i32 * self.row_height,
            self.width - 2 * LIST_X,
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

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Field {
    Rows,
    Font,
    Lines,
}

impl Field {
    fn title(self, scope: Scope) -> &'static str {
        match (self, scope) {
            (Self::Rows, _) => "Rows per page",
            (Self::Font, Scope::Contents) => "Entry font size",
            (Self::Font, Scope::Bookmarks) => "Bookmark font size",
            (Self::Lines, _) => "Max lines per bookmark",
        }
    }

    fn bounds(self) -> (u8, u8) {
        match self {
            Self::Rows => (ListDensity::MIN_ROWS, ListDensity::MAX_ROWS),
            Self::Font => (ListDensity::MIN_FONT, ListDensity::MAX_FONT),
            Self::Lines => (1, MAX_BOOKMARK_LINES),
        }
    }

    fn default_value(self, density: ListDensity) -> u8 {
        match self {
            Self::Rows => ListDensity::DEFAULT_ROWS,
            Self::Font => density.automatic_font(),
            Self::Lines => DEFAULT_BOOKMARK_LINES,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum PickerTarget {
    Density(Scope, Field),
    Search(super::search_presentation::Field),
    FooterWidth(FooterTextItem),
}

impl PickerTarget {
    fn title(self) -> &'static str {
        match self {
            Self::Density(scope, field) => field.title(scope),
            Self::Search(field) => field.title(),
            Self::FooterWidth(item) => item.width_label(),
        }
    }

    fn bounds(self) -> (u8, u8) {
        match self {
            Self::Density(_, field) => field.bounds(),
            Self::Search(field) => field.bounds(),
            Self::FooterWidth(_) => (FooterTextItem::MIN, FooterTextItem::MAX),
        }
    }

    fn default_value(self, controls: &ReadingControls) -> u8 {
        match self {
            Self::Density(scope, field) => field.default_value(scope.preferences(controls)),
            Self::Search(field) => field.default_value(),
            Self::FooterWidth(_) => FooterTextItem::DEFAULT,
        }
    }

    fn is_lines(self) -> bool {
        matches!(
            self,
            Self::Density(_, Field::Lines) | Self::Search(super::search_presentation::Field::Lines)
        )
    }

    fn is_font(self) -> bool {
        matches!(self, Self::Density(_, Field::Font))
    }

    fn hold_step(self) -> i8 {
        match self {
            Self::Search(super::search_presentation::Field::Words) => 5,
            Self::FooterWidth(_) => 20,
            _ => 4,
        }
    }

    fn step(self) -> i8 {
        if matches!(self, Self::FooterWidth(_)) {
            5
        } else {
            1
        }
    }

    fn format(self, value: u8) -> String {
        if matches!(self, Self::FooterWidth(_)) {
            format!("{value}%")
        } else {
            value.to_string()
        }
    }
}

pub(super) struct Picker {
    pub(super) target: PickerTarget,
    pub(super) value: Option<u8>,
}

impl Picker {
    fn value(&self, controls: &ReadingControls) -> u8 {
        self.value
            .unwrap_or_else(|| self.target.default_value(controls))
    }
}

impl Reader {
    pub(super) fn bookmark_lines_item(&self) -> Item {
        Item::new(
            Field::Lines.title(Scope::Bookmarks),
            Action::EditListDensity(Scope::Bookmarks, Field::Lines),
        )
        .detail(
            self.controls
                .bookmark_max_lines()
                .map_or_else(|| "Disabled".to_owned(), |value| value.to_string()),
        )
    }

    pub(super) fn list_density_items(&self, scope: Scope) -> [Item; 2] {
        let density = scope.preferences(&self.controls);
        let font = density.explicit_font().map_or_else(
            || format!("Automatic ({})", density.automatic_font()),
            |value| value.to_string(),
        );
        let rows =
            if matches!(scope, Scope::Bookmarks) && self.controls.bookmark_max_lines().is_some() {
                Item::new(Field::Rows.title(scope), Action::Heading).detail("Flexible")
            } else {
                Item::new(
                    Field::Rows.title(scope),
                    Action::EditListDensity(scope, Field::Rows),
                )
                .detail(density.rows().to_string())
            };
        [
            rows,
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
            Field::Lines => Some(
                self.controls
                    .bookmark_max_lines()
                    .unwrap_or(DEFAULT_BOOKMARK_LINES),
            ),
        };
        self.notice = None;
        self.list_density_picker = Some(Picker {
            target: PickerTarget::Density(scope, field),
            value,
        });
    }

    pub(super) fn open_footer_width(&mut self, item: FooterTextItem, draft: Option<u8>) {
        self.notice = None;
        self.list_density_picker = Some(Picker {
            target: PickerTarget::FooterWidth(item),
            value: Some(draft.unwrap_or_else(|| self.controls.footer_text_widths.get(item))),
        });
    }

    fn density_picker_layout(&self, target: PickerTarget) -> NavigationLayout {
        if matches!(
            target,
            PickerTarget::Density(Scope::Contents, _) | PickerTarget::FooterWidth(_)
        ) {
            self.navigation_layout()
        } else {
            NavigationLayout::PORTRAIT
        }
    }

    pub(super) fn draw_list_density_picker(&self, canvas: &mut Canvas) {
        let Some(picker) = &self.list_density_picker else {
            return;
        };
        let target = picker.target;
        let footer_width = matches!(target, PickerTarget::FooterWidth(_));
        let layout = self.density_picker_layout(target);
        let place = |rect| layout.centered(rect);
        let (min, max) = target.bounds();
        canvas.popup(place(PANEL));
        canvas.text(
            place(Rect::new(244, 534, 776, 68)),
            target.title(),
            TextStyle::new(50).bold().center(),
        );
        let bounds = format!("{} to {}", target.format(min), target.format(max));
        let hint = if footer_width {
            format!("{bounds} of screen width")
        } else {
            bounds
        };
        canvas.text(
            place(Rect::new(244, 607, 776, 49)),
            &hint,
            TextStyle::new(30).ink(SECONDARY).center(),
        );
        canvas.button(place(DECREASE), "-", false, false);
        canvas.button(place(INCREASE), "+", false, false);
        let value = if target.is_lines() && picker.value.is_none() {
            "Disabled".to_owned()
        } else {
            target.format(picker.value(&self.controls))
        };
        if footer_width {
            canvas.button(place(PICKER_VALUE), &value, false, false);
        } else {
            canvas.text(
                place(Rect::new(371, 686, 522, 105)),
                &value,
                TextStyle::new(66).bold().center(),
            );
        }
        let default_value = target.default_value(&self.controls);
        if target.is_lines() {
            canvas.button(
                place(LINE_DEFAULT),
                &format!("Default ({default_value})"),
                false,
                false,
            );
            canvas.button(place(DISABLE_LINES), "Disable", false, false);
        } else {
            let label = if target.is_font() {
                "Automatic"
            } else {
                "Default"
            };
            canvas.button(
                place(DEFAULT),
                &format!("{label} ({})", target.format(default_value)),
                false,
                false,
            );
        }
        if let Some(notice) = &self.notice {
            canvas.text(
                place(Rect::new(244, 937, 776, 79)),
                notice,
                TextStyle::new(28).ink(SECONDARY).lines(2).center(),
            );
        }
        canvas.button(
            place(CANCEL),
            if footer_width { "Close" } else { "Cancel" },
            false,
            false,
        );
        canvas.button(place(APPLY), "Apply", true, false);
    }

    pub(super) fn list_density_input(&mut self, input: Input) -> Option<Change> {
        let target = self.list_density_picker.as_ref()?.target;
        let layout = self.density_picker_layout(target);
        let place = |rect| layout.centered(rect);
        let picker = self.list_density_picker.as_mut()?;
        if let PickerTarget::FooterWidth(item) = target
            && matches!(input, Input::Tap(x, y) if place(PICKER_VALUE).contains(x, y))
        {
            let value = picker.value(&self.controls);
            self.show(super::Screen::NumberingValue(
                super::NumberField::FooterItemWidth(item, value),
            ));
            return Some(Change::Partial);
        }
        let clear_button = if target.is_lines() {
            DISABLE_LINES
        } else {
            DEFAULT
        };
        let step = match input {
            Input::Tap(x, y) if place(CANCEL).contains(x, y) || !place(PANEL).contains(x, y) => {
                self.list_density_picker = None;
                self.notice = None;
                return Some(Change::Partial);
            }
            Input::Tap(x, y) if place(APPLY).contains(x, y) => {
                let mut controls = self.controls;
                match target {
                    PickerTarget::Density(scope, Field::Rows) => {
                        scope.preferences_mut(&mut controls).rows_per_page = picker.value
                    }
                    PickerTarget::Density(scope, Field::Font) => {
                        scope.preferences_mut(&mut controls).font_size = picker.value
                    }
                    PickerTarget::Density(_, Field::Lines) => {
                        controls.bookmark_max_lines = picker.value
                    }
                    PickerTarget::Search(field) => field.set(&mut controls, picker.value),
                    PickerTarget::FooterWidth(item) => {
                        let width = picker.value(&controls);
                        controls.footer_text_widths.set(item, width);
                    }
                }
                return Some(match self.activate(Action::SetReadingControls(controls)) {
                    Ok(()) => {
                        if !matches!(target, PickerTarget::FooterWidth(_)) {
                            self.list_density_picker = None;
                        }
                        self.notice = None;
                        Change::Partial
                    }
                    Err(error) => self.report(error),
                });
            }
            Input::Tap(x, y) if target.is_lines() && place(LINE_DEFAULT).contains(x, y) => {
                picker.value = Some(target.default_value(&self.controls));
                self.notice = None;
                return Some(Change::Partial);
            }
            Input::Tap(x, y) if place(clear_button).contains(x, y) => {
                picker.value = None;
                self.notice = None;
                return Some(Change::Partial);
            }
            Input::Tap(x, y) if place(DECREASE).contains(x, y) => -target.step(),
            Input::Tap(x, y) if place(INCREASE).contains(x, y) => target.step(),
            Input::LongTap(x, y) if place(DECREASE).contains(x, y) => -target.hold_step(),
            Input::LongTap(x, y) if place(INCREASE).contains(x, y) => target.hold_step(),
            Input::PageBack => -target.step(),
            Input::PageForward => target.step(),
            _ => return Some(Change::None),
        };
        let prior = picker.value(&self.controls);
        let (min, max) = target.bounds();
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

pub(super) fn nominal_height(canvas: &Canvas, rendered: i32) -> i32 {
    rendered * 1000 / canvas.px(1000).max(1)
}

pub(super) fn text_line_height(canvas: &Canvas, size: i32) -> i32 {
    (canvas.px(size) * 13 + 5) / 10
}

pub(super) fn list_text_pixels(logical: u8, row_height: i32) -> i32 {
    font_pixels(fit_logical_font(logical, row_height))
}

pub(super) fn font_pixels(logical: u8) -> i32 {
    (reference_scale().scaled_pixels(f64::from(logical)) as i32).max(1)
}

pub(super) fn fit_logical_font(logical: u8, row_height: i32) -> u8 {
    let scale = reference_scale();
    let rule = scale.scaled_pixels(1.) as i32;
    // Like KOReader's MenuItem, reserve two rules and 30% of an em for
    // leading. This caps visible ink without changing the saved preference.
    let available = (row_height - 2 * rule) * 10 / 13;
    (1..=logical)
        .rev()
        .find(|&size| (scale.scaled_pixels(f64::from(size)) as i32) <= available)
        .unwrap_or(0)
}
