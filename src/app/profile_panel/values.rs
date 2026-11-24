//! Route the reading-profile editor through the shared exact-value keyboard.
use super::{field_buttons, *};
use crate::app::step_buttons::StepButtons;
use crate::reading_profiles::ValueField;

const STEPS: [Rect; 3] = [
    Rect::new(48, LIST_Y + 286, 352, 112),
    Rect::new(424, LIST_Y + 286, 416, 112),
    Rect::new(864, LIST_Y + 286, 352, 112),
];

impl Reader {
    pub(in crate::app) fn profile_value_items(&self, index: usize) -> Vec<Item> {
        let Some(profile) = self.profiles.get(index) else {
            return Vec::new();
        };
        let selection = profile.selection();
        profile
            .editable_values()
            .iter()
            .filter_map(|&field| {
                let value = profile.value(field)?;
                let detail = if field.included_in(&selection) {
                    format!("{value}%")
                } else {
                    format!("{value}% · Excluded")
                };
                Some(
                    Item::new(
                        field.label(),
                        Action::Show(Screen::NumberingValue(NumberField::ProfileValue(
                            index, field,
                        ))),
                    )
                    .detail(detail),
                )
            })
            .collect()
    }

    pub(in crate::app) fn draw_profile_values_button(&self, canvas: &mut Canvas) {
        if let Screen::ReadingProfile(index) = self.screen
            && self.profiles.get(index).is_some()
        {
            canvas.button(
                field_buttons(self.navigation_layout()).1,
                "Edit values",
                false,
                false,
            );
        }
    }

    pub(in crate::app) fn profile_values_button_input(&mut self, input: Input) -> Option<Change> {
        let Screen::ReadingProfile(index) = self.screen else {
            return None;
        };
        let button = field_buttons(self.navigation_layout()).1;
        if matches!(input, Input::Tap(x, y) if button.contains(x, y))
            && self.profiles.get(index).is_some()
        {
            self.show(Screen::ReadingProfileValues(index));
            return Some(Change::Partial);
        }
        None
    }

    pub(in crate::app) fn prepare_profile_value_editor(&mut self) {
        let Screen::NumberingValue(NumberField::ProfileValue(index, field)) = self.screen else {
            return;
        };
        if let Some(value) = self
            .profiles
            .get(index)
            .and_then(|profile| profile.value(field))
        {
            self.set_profile_value_draft(value);
        }
    }

    fn set_profile_value_draft(&mut self, value: u16) {
        self.editor.text = value.to_string().chars().collect();
        self.editor.caret = self.editor.text.len();
        self.notice = None;
    }

    fn parse_profile_value_draft(&self, field: ValueField) -> Result<u16> {
        let limits = field.limits();
        self.editor
            .string()
            .trim()
            .parse::<u16>()
            .ok()
            .filter(|value| limits.contains(value))
            .ok_or_else(|| {
                format!(
                    "Enter a value from {}% to {}%",
                    limits.start(),
                    limits.end()
                )
                .into()
            })
    }

    pub(in crate::app) fn profile_value_input(&mut self, input: Input) -> Option<Change> {
        let Screen::NumberingValue(NumberField::ProfileValue(_, field)) = self.screen else {
            return None;
        };
        let [decrease, default, increase] = self.numeric_layout().steps(STEPS, false);
        let value = if matches!(input, Input::Tap(x, y) if default.contains(x, y)) {
            Ok(field.default())
        } else if let Some(delta) = StepButtons::new(decrease, increase).delta(input, 5) {
            let limits = field.limits();
            self.parse_profile_value_draft(field).map(|value| {
                value
                    .saturating_add_signed(delta)
                    .clamp(*limits.start(), *limits.end())
            })
        } else {
            return None;
        };
        Some(match value {
            Ok(value) => {
                self.set_profile_value_draft(value);
                Change::Partial
            }
            Err(error) => self.report(error),
        })
    }

    pub(in crate::app) fn apply_profile_value(
        &mut self,
        index: usize,
        field: ValueField,
    ) -> Result<()> {
        let value = self.parse_profile_value_draft(field)?;
        let mut profiles = self.profiles.clone();
        profiles
            .get_mut(index)
            .ok_or("That profile is unavailable")?
            .set_value(field, value)?;
        self.commit_profiles(profiles)?;
        self.show(Screen::ReadingProfileValues(index));
        Ok(())
    }

    pub(in crate::app) fn draw_profile_value_editor(&self, canvas: &mut Canvas) {
        let Screen::NumberingValue(NumberField::ProfileValue(_, field)) = self.screen else {
            return;
        };
        let layout = self.numeric_layout();
        let [decrease, default, increase] = layout.steps(STEPS, false);
        StepButtons::new(decrease, increase).draw(canvas);
        canvas.button(
            default,
            &format!("Default: {}%", field.default()),
            false,
            false,
        );
        let limits = field.limits();
        canvas.text(
            layout.detail(Rect::new(48, LIST_Y + 428, 1168, 150), false),
            &format!(
                "{}% to {}%\nTap -/+ for 1%. Hold for 5%.",
                limits.start(),
                limits.end()
            ),
            CAPTION.lines(2),
        );
    }
}

#[cfg(test)]
mod tests;
