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
                let mut detail = if field == ValueField::FontSize {
                    profile.legacy_font_size().map_or_else(
                        || field.display(value),
                        |size| format!("{size} px (legacy)"),
                    )
                } else {
                    field.display(value)
                };
                if !field.included_in(&selection) {
                    detail.push_str(" · Excluded");
                }
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
        let Some(profile) = self.profiles.get(index) else {
            return;
        };
        let Some(value) = profile.value(field) else {
            return;
        };
        let value = if field == ValueField::FontSize
            && let Some(pixels) = profile.legacy_font_size()
        {
            let scale = self.device_reading_settings().font_scale;
            // Match the nearest half-unit to the old physical size. The
            // renderer rounds logical sizes up to whole device pixels.
            let logical = (f64::from(pixels) - 0.5) / f64::from(scale.physical_delta(1.0));
            ((logical * 2.0).round() * 0.5).clamp(12.0, 255.0)
        } else {
            value
        };
        self.set_profile_value_draft(value);
    }

    fn set_profile_value_draft(&mut self, value: f64) {
        self.editor.text = value.to_string().chars().collect();
        self.editor.caret = self.editor.text.len();
        self.notice = None;
    }

    fn parse_profile_value_draft(&self, field: ValueField) -> Result<f64> {
        field
            .parse(&self.editor.string())
            .ok_or_else(|| format!("Enter a value from {}", field.range_label()).into())
    }

    pub(in crate::app) fn profile_value_input(&mut self, input: Input) -> Option<Change> {
        let Screen::NumberingValue(NumberField::ProfileValue(_, field)) = self.screen else {
            return None;
        };
        let [decrease, default, increase] = self.numeric_layout().steps(STEPS, false);
        let value = if matches!(input, Input::Tap(x, y) if default.contains(x, y)) {
            Ok(field.default())
        } else if let Some(delta) =
            StepButtons::new(decrease, increase).delta(input, field.held_steps())
        {
            let limits = field.limits();
            self.parse_profile_value_draft(field).map(|value| {
                (value + f64::from(delta) * field.step()).clamp(*limits.start(), *limits.end())
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
        let Screen::NumberingValue(NumberField::ProfileValue(index, field)) = self.screen else {
            return;
        };
        let layout = self.numeric_layout();
        let [decrease, default, increase] = layout.steps(STEPS, false);
        StepButtons::new(decrease, increase).draw(canvas);
        canvas.button(
            default,
            &format!("Default: {}", field.display(field.default())),
            false,
            false,
        );
        let legacy = field == ValueField::FontSize
            && self
                .profiles
                .get(index)
                .and_then(Profile::legacy_font_size)
                .is_some();
        let mut details = format!(
            "{}\nTap -/+ for {}. Hold for {}.",
            field.range_label(),
            field.display(field.step()),
            field.display(f64::from(field.held_steps()) * field.step()),
        );
        if legacy {
            details.push_str("\nSet converts the saved pixel size to a logical size.");
        }
        canvas.text(
            layout.detail(Rect::new(48, LIST_Y + 428, 1168, 150), false),
            &details,
            CAPTION.lines(3),
        );
    }
}

#[cfg(test)]
mod tests;
