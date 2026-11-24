//! The saved font size applies to each new dictionary popup.
use super::*;
use crate::app::step_buttons::StepButtons;
use primitive_ui::TextStyle;

pub(in crate::app) const DEFAULT: i32 = 20;
pub(in crate::app) const RANGE: std::ops::RangeInclusive<i32> = 8..=32;

const PANEL: Rect = Rect::new(162, 570, 940, 540);
pub(super) const RESET: Rect = Rect::new(850, 607, 204, 100);
pub(super) const DECREASE: Rect = Rect::new(220, 750, 122, 104);
pub(super) const INCREASE: Rect = Rect::new(922, 750, 122, 104);
pub(super) const CANCEL: Rect = Rect::new(220, 948, 390, 106);
pub(super) const APPLY: Rect = Rect::new(654, 948, 390, 106);

pub(in crate::app) fn is_default(value: &i32) -> bool {
    *value == DEFAULT
}

impl Reader {
    pub(in crate::app) fn open_dictionary_font_size(&mut self) {
        self.dictionary_navigation.font_size_draft =
            Some(self.dictionary_settings.definition_font_size);
        self.notice = None;
    }

    pub(in crate::app) fn close_dictionary_font_size(&mut self) {
        self.dictionary_navigation.font_size_draft = None;
    }

    pub(in crate::app) fn draw_dictionary_font_size(&self, canvas: &mut Canvas) {
        let Some(value) = self.dictionary_navigation.font_size_draft else {
            return;
        };
        let layout = self.navigation_layout();
        let at = |rect| layout.centered(rect);
        canvas.popup(at(PANEL));
        canvas.text(
            at(Rect::new(220, 617, 605, 76)),
            "Dictionary font size",
            TextStyle::new(46).bold(),
        );
        canvas.button(at(RESET), "Default", false, false);
        StepButtons::new(at(DECREASE), at(INCREASE)).draw(canvas);
        canvas.text(
            at(Rect::new(372, 750, 520, 104)),
            &value.to_string(),
            TextStyle::new(66).bold().center(),
        );
        canvas.text(
            at(Rect::new(220, 870, 824, 60)),
            self.notice.as_deref().unwrap_or("8–32"),
            CAPTION.center(),
        );
        canvas.button(at(CANCEL), "Cancel", false, false);
        canvas.button(at(APPLY), "Apply", true, false);
    }

    pub(in crate::app) fn dictionary_font_size_press(
        &self,
        input: Input,
    ) -> Option<Option<(f32, f32)>> {
        self.dictionary_navigation.font_size_draft?;
        let layout = self.navigation_layout();
        Some(match input {
            Input::Tap(x, y)
                if [RESET, DECREASE, INCREASE, CANCEL, APPLY]
                    .into_iter()
                    .any(|choice| layout.centered(choice).contains(x, y)) =>
            {
                Some((x, y))
            }
            _ => None,
        })
    }

    pub(in crate::app) fn dictionary_font_size_input(&mut self, input: Input) -> Option<Change> {
        let value = self.dictionary_navigation.font_size_draft?;
        if matches!(input, Input::Sleep | Input::Wake) {
            return None;
        }
        if !self.stats_awake {
            return Some(Change::None);
        }
        let layout = self.navigation_layout();
        let at = |rect: Rect| layout.centered(rect);
        if let Some(delta) = StepButtons::new(at(DECREASE), at(INCREASE)).delta(input, 4) {
            let next = (value + i32::from(delta)).clamp(*RANGE.start(), *RANGE.end());
            if next == value {
                return Some(Change::None);
            }
            self.dictionary_navigation.font_size_draft = Some(next);
            self.notice = None;
            return Some(Change::Partial);
        }
        if matches!(input, Input::PageBack)
            || matches!(input, Input::Tap(x,y) if at(CANCEL).contains(x, y) || !at(PANEL).contains(x, y))
        {
            self.close_dictionary_font_size();
            self.notice = None;
            return Some(Change::Partial);
        }
        let Input::Tap(x, y) = input else {
            return Some(Change::None);
        };
        if at(RESET).contains(x, y) {
            self.dictionary_navigation.font_size_draft = Some(DEFAULT);
            self.notice = None;
            return Some(Change::Partial);
        }
        if !at(APPLY).contains(x, y) {
            return Some(Change::None);
        }
        if value != self.dictionary_settings.definition_font_size {
            let mut next = self.dictionary_settings.clone();
            next.definition_font_size = value;
            if let Err(error) = self.save_source_preferences(next) {
                return Some(self.report(error));
            }
        }
        self.close_dictionary_font_size();
        self.notice = None;
        Some(Change::Partial)
    }
}
