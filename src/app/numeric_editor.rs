//! The exact-value field and inline controls share space above the keyboard.
use super::navigation_layout::NavigationLayout;
use super::{LIST_X, LIST_Y, NumberField, Reader, Rect, Screen};

const GAP: i32 = 24;
const CHOICE_HEIGHT: i32 = 96;
const STEP_HEIGHT: i32 = 92;

#[derive(Clone, Copy)]
pub(super) struct NumericLayout {
    navigation: NavigationLayout,
    inline: bool,
}

impl NumericLayout {
    pub(super) fn is_landscape(self) -> bool {
        self.navigation.width > self.navigation.height
    }

    pub(super) fn field(self) -> Rect {
        let mut rect = self.navigation.body(LIST_Y, 130);
        if self.is_landscape() && self.inline {
            rect.w = self.inline_x() - GAP - rect.x;
        }
        rect
    }

    pub(super) fn hint(self) -> Rect {
        let field = self.field();
        Rect::new(field.x, field.bottom() + 12, field.w, 58)
    }

    pub(super) fn notice(self) -> Rect {
        if !self.is_landscape() {
            return self.navigation.body(LIST_Y + 158, 75);
        }
        let hint = self.hint();
        let top = hint.bottom() + 8;
        Rect::new(hint.x, top, hint.w, self.keyboard_top() - top - 12)
    }

    pub(super) fn choices(self, portrait: [Rect; 2]) -> [Rect; 2] {
        if !self.is_landscape() {
            return portrait;
        }
        let x = self.inline_x();
        let width = (self.inline_width() - GAP) / 2;
        [
            Rect::new(x, LIST_Y, width, CHOICE_HEIGHT),
            Rect::new(x + width + GAP, LIST_Y, width, CHOICE_HEIGHT),
        ]
    }

    pub(super) fn steps(self, portrait: [Rect; 3], with_choices: bool) -> [Rect; 3] {
        if !self.is_landscape() {
            return portrait;
        }
        let x = self.inline_x();
        let y = LIST_Y + if with_choices { CHOICE_HEIGHT + 16 } else { 0 };
        let edge = 156;
        let middle = self.inline_width() - 2 * (edge + GAP);
        [
            Rect::new(x, y, edge, STEP_HEIGHT),
            Rect::new(x + edge + GAP, y, middle, STEP_HEIGHT),
            Rect::new(x + edge + middle + 2 * GAP, y, edge, STEP_HEIGHT),
        ]
    }

    pub(super) fn detail(self, portrait: Rect, with_choices: bool) -> Rect {
        if !self.is_landscape() {
            return portrait;
        }
        let top = LIST_Y + STEP_HEIGHT + 16 + if with_choices { CHOICE_HEIGHT + 16 } else { 0 };
        Rect::new(
            self.inline_x(),
            top,
            self.inline_width(),
            self.keyboard_top() - top - 12,
        )
    }

    fn inline_x(self) -> i32 {
        self.navigation.width * 9 / 20
    }

    fn inline_width(self) -> i32 {
        self.navigation.width - self.inline_x() - LIST_X
    }

    fn keyboard_top(self) -> i32 {
        self.navigation.keyboard().keyboard_area().y
    }
}

impl Screen {
    pub(super) fn is_numeric_editor(self) -> bool {
        matches!(self, Self::NumberingValue(_) | Self::FootnoteFontSize(_))
    }

    pub(super) fn uses_numeric_settings_list(self) -> bool {
        matches!(
            self,
            Self::ReadingControls
                | Self::LinkSettings
                | Self::PageNumbering
                | Self::NumberingDefaults
                | Self::AutoTurn
                | Self::AutoTurnPan
                | Self::ReadingGuides
                | Self::FooterSettings
        )
    }
}

impl Reader {
    pub(super) fn numeric_layout(&self) -> NumericLayout {
        let inline = matches!(
            self.screen,
            Screen::NumberingValue(
                NumberField::ContentDpi(_)
                    | NumberField::LogicalFont(_)
                    | NumberField::LineSpacing(_)
                    | NumberField::ProfileValue(_, _)
                    | NumberField::WordSpacing(_)
            ) | Screen::FootnoteFontSize(_)
        );
        NumericLayout {
            navigation: self.navigation_layout(),
            inline,
        }
    }
}

#[cfg(test)]
mod tests;
