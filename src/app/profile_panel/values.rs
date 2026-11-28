//! Route the reading-profile editor through the shared exact-value keyboard.
use super::{field_buttons, *};
use crate::app::step_buttons::StepButtons;
use crate::reading_profiles::{Field, ProfileZoomChoice, ProfileZoomTarget, ValueField};
use primitive_app::with_storage;
use std::fs;

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
        let language = self.controls.interface_language;
        let selection = profile.selection();
        let general = matches!(self.screen, Screen::ReadingProfileValues(_));
        let margins = matches!(self.screen, Screen::ReadingProfileMargins(_));
        let font_rendering = matches!(self.screen, Screen::ReadingProfileFontRendering(_));
        let fields = match self.screen {
            Screen::ReadingProfileMargins(_) => ValueField::MARGINS.as_slice(),
            Screen::ReadingProfileFontRendering(_) => ValueField::FONT_RENDERING.as_slice(),
            _ => profile.editable_values(),
        };
        let mut items: Vec<_> = fields
            .iter()
            .filter_map(|&field| {
                let value = profile.value(field)?;
                let mut detail = if field == ValueField::FontSize {
                    profile.legacy_font_size().map_or_else(
                        || field.display(value),
                        |size| format!("{size} px (legacy)"),
                    )
                } else if let Some(legacy) = profile.legacy_value_label(field) {
                    legacy.into()
                } else {
                    field.display(value)
                };
                if !field.included_in(&selection) {
                    detail.push_str(" · ");
                    detail.push_str(language.reading_profile_text("Excluded"));
                }
                Some(
                    Item::new(
                        language.reading_profile_text(field.label()),
                        Action::Show(Screen::NumberingValue(NumberField::ProfileValue(
                            index, field,
                        ))),
                    )
                    .detail(detail),
                )
            })
            .collect();
        if general && let Profile::Fixed { .. } = profile {
            let fit = profile
                .zoom_fit_axis()
                .unwrap_or(crate::library::FixedContentFit::Full);
            let target = profile
                .zoom_target()
                .map(ProfileZoomTarget::label)
                .unwrap_or("Page");
            let fit_label = match fit {
                crate::library::FixedContentFit::Full => "Full",
                crate::library::FixedContentFit::Width => "Width",
                crate::library::FixedContentFit::Height => "Height",
            };
            items.insert(
                0,
                Item::new(
                    language.reading_profile_text("Zoom and fit"),
                    Action::ShowProfileZoom(index),
                )
                .detail(format!(
                    "{} · {}",
                    language.reading_profile_text(target),
                    language.reading_profile_text(fit_label)
                )),
            );
        }
        if general && let Profile::Text { preferences, .. } = profile {
            items.insert(
                0,
                Item::new(
                    language.reading_profile_text("Reading font"),
                    Action::ShowProfileFonts(index),
                )
                .detail(
                    preferences
                        .font_file
                        .as_deref()
                        .unwrap_or("Device fonts"),
                ),
            );
        }
        if general && let Profile::Text { preferences, .. } = profile {
            let pair = preferences.percent_word_spacing;
            let mut detail = pair.map_or_else(
                || format!("{:+} px (legacy)", preferences.word_spacing),
                |spacing| format!("{}% / {}%", spacing.scaling, spacing.minimum),
            );
            if !selection.contains(&Field::WordSpacing) {
                detail.push_str(" · ");
                detail.push_str(language.reading_profile_text("Excluded"));
            }
            items.push(
                Item::new(
                    language.reading_profile_text("Word spacing"),
                    Action::Show(Screen::NumberingValue(NumberField::WordSpacing(
                        word_spacing::Draft::profile(index, pair),
                    ))),
                )
                .detail(detail),
            );
        }
        if let Some(linked) = profile.margins_linked() {
            if margins {
                items.push(
                    Item::new(
                        language.reading_profile_text("Link top and bottom"),
                        Action::ToggleProfileMarginLink(index),
                    )
                    .detail(language.reading_profile_text(if linked {
                        "Linked"
                    } else {
                        "Independent"
                    })),
                );
            } else if general {
                items.push(
                    Item::new(
                        language.reading_profile_text("Page margins"),
                        Action::Show(Screen::ReadingProfileMargins(index)),
                    )
                    .detail(if selection.contains(&Field::Margins) {
                        language.reading_profile_text("Left, right, top and bottom")
                    } else {
                        language.reading_profile_text("Excluded")
                    }),
                );
                items.push(
                    Item::new(
                        language.reading_profile_text("Font rendering"),
                        Action::Show(Screen::ReadingProfileFontRendering(index)),
                    )
                    .detail(language.reading_profile_text("Weight, contrast, hinting and kerning")),
                );
                items.push(
                    Item::new(
                        language.reading_profile_text("Book rendering"),
                        Action::Show(Screen::ReadingProfileBookRendering(index)),
                    )
                    .detail(
                        language.reading_profile_text("Page mode, alignment, styles and images"),
                    ),
                );
            }
        }
        if font_rendering {
            items.extend(self.profile_rendering_items(index, &super::rendering_choices::FONT));
        }
        items
    }

    pub(super) fn profile_zoom_items(&self, index: usize) -> Vec<Item> {
        let Some(Profile::Fixed { fixed, .. }) = self.profiles.get(index) else {
            return Vec::new();
        };
        let language = self.controls.interface_language;
        let selected_target = self.profiles[index].zoom_target();
        let selected_fit = self.profiles[index].zoom_fit_axis();
        let view = fixed.values();
        let spreads = view.spread
            || (fixed.provider == crate::reading_profiles::FixedProvider::Epub
                && view.epub_spread.unwrap_or(false));
        let mut items = vec![Item::new(
            language.reading_profile_text("Zoom to"),
            Action::Heading,
        )];
        if spreads {
            items.push(
                Item::new(
                    language.reading_profile_text("Facing pages are enabled"),
                    Action::Heading,
                )
                .detail(language.reading_profile_text(
                    "Tile and manual zoom require single-page mode",
                )),
            );
        }
        for target in [
            ProfileZoomTarget::Page,
            ProfileZoomTarget::Content,
            ProfileZoomTarget::Columns,
            ProfileZoomTarget::Rows,
            ProfileZoomTarget::Manual,
        ] {
            if spreads
                && matches!(
                    target,
                    ProfileZoomTarget::Columns
                        | ProfileZoomTarget::Rows
                        | ProfileZoomTarget::Manual
                )
            {
                continue;
            }
            let mut item = Item::new(
                language.reading_profile_text(target.label()),
                Action::SetProfileZoom(index, ProfileZoomChoice::Target(target)),
            );
            if selected_target == Some(target) {
                item = item.detail(language.reading_profile_text("Selected"));
            }
            items.push(item);
        }
        if selected_target == Some(ProfileZoomTarget::Custom) {
            items.push(
                Item::new(
                    language.reading_profile_text("Current custom zoom"),
                    Action::Heading,
                )
                .detail(language.reading_profile_text("Choose a saved zoom mode")),
            );
        }
        items.push(Item::new(
            language.reading_profile_text("Fit axis"),
            Action::Heading,
        ));
        for (fit, label) in [
            (crate::library::FixedContentFit::Full, "Full"),
            (crate::library::FixedContentFit::Width, "Width"),
            (crate::library::FixedContentFit::Height, "Height"),
        ] {
            let mut item = Item::new(
                language.reading_profile_text(label),
                Action::SetProfileZoom(index, ProfileZoomChoice::Fit(fit)),
            );
            if selected_fit == Some(fit) {
                item = item.detail(language.reading_profile_text("Selected"));
            }
            items.push(item);
        }
        items
    }

    pub(in crate::app) fn show_profile_zoom(&mut self, index: usize) -> Result<()> {
        if !matches!(self.profiles.get(index), Some(Profile::Fixed { .. })) {
            return Err("Zoom settings require a fixed-page profile".into());
        }
        self.show(Screen::ReadingProfileZoom(index));
        Ok(())
    }

    pub(in crate::app) fn save_profile_zoom_choice(
        &mut self,
        index: usize,
        choice: ProfileZoomChoice,
    ) -> Result<()> {
        let mut profiles = self.profiles.clone();
        profiles
            .get_mut(index)
            .ok_or("That profile is unavailable")?
            .set_zoom_choice(choice)?;
        self.commit_profiles(profiles)?;
        self.show(Screen::ReadingProfileZoom(index));
        Ok(())
    }

    pub(in crate::app) fn open_profile_font_chooser(&mut self, index: usize) -> Result<()> {
        if !matches!(self.profiles.get(index), Some(Profile::Text { .. })) {
            return Err("Reading fonts are available only in reflowable profiles".into());
        }
        with_storage(|| fs::create_dir_all(&self.fonts_directory))?;
        self.available_fonts = crate::reading_fonts::available(&self.fonts_directory)?;
        self.show(Screen::ReadingProfileFonts(index));
        Ok(())
    }

    pub(in crate::app) fn set_profile_font(&mut self, index: usize, font: Option<String>) -> Result<()> {
        // Resolve the font before changing the profile so a stale or malformed
        // file can never turn a saved profile into an unusable one.
        let fonts = self.fonts_for(font.as_deref())?;
        drop(fonts);
        let mut profiles = self.profiles.clone();
        let Some(Profile::Text { preferences, .. }) = profiles.get_mut(index) else {
            return Err("Reading fonts are available only in reflowable profiles".into());
        };
        preferences.font_file = font;
        self.commit_profiles(profiles)?;
        self.show(Screen::ReadingProfileValues(index));
        Ok(())
    }

    pub(in crate::app) fn save_profile_word_spacing(
        &mut self,
        index: usize,
        value: crate::WordSpacePercent,
    ) -> Result<()> {
        let mut profiles = self.profiles.clone();
        profiles
            .get_mut(index)
            .ok_or("That profile is unavailable")?
            .set_word_spacing(value)?;
        self.commit_profiles(profiles)?;
        self.show(Screen::ReadingProfileValues(index));
        Ok(())
    }

    pub(in crate::app) fn toggle_profile_margin_link(&mut self, index: usize) -> Result<()> {
        let mut profiles = self.profiles.clone();
        profiles
            .get_mut(index)
            .ok_or("That profile is unavailable")?
            .toggle_margin_link()?;
        self.commit_profiles(profiles)?;
        self.notice = None;
        Ok(())
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
            .ok_or_else(|| field.input_error().into())
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
            self.parse_profile_value_draft(field)
                .map(|value| field.stepped(value, delta))
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
        self.show(NumberField::ProfileValue(index, field).parent());
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
        let mut details = if field == ValueField::FontContrast {
            format!(
                "{} gamma levels, {}\nTap -/+ for one level. Hold for five.",
                crate::font_render::GAMMA_LEVELS.len(),
                field.range_label()
            )
        } else {
            format!(
                "{}\nTap -/+ for {}. Hold for {}.",
                field.range_label(),
                field.display(field.step()),
                field.display(f64::from(field.held_steps()) * field.step()),
            )
        };
        if legacy {
            details.push_str("\nSet converts the saved pixel size to a logical size.");
        }
        if self
            .profiles
            .get(index)
            .and_then(|profile| profile.legacy_value_label(field))
            .is_some()
        {
            details.push_str(if field == ValueField::ScanGamma {
                "\nSet replaces adaptive contrast."
            } else {
                "\nSet replaces the legacy setting."
            });
        }
        if matches!(field, ValueField::Margin(edge) if edge.is_vertical())
            && self.profiles.get(index).and_then(Profile::margins_linked) == Some(true)
        {
            details.push_str("\nTop and bottom margins are linked.");
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
