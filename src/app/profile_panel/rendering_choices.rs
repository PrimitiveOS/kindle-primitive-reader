//! Discrete rendering settings saved to a reflowable profile.
use super::{Action, BookPreferences, Field, Item, Profile, Reader, Result, Screen};
use crate::{FontHinting, FontKerningMode};

pub(super) const FONT: [Field; 2] = [Field::Hinting, Field::Kerning];
pub(super) const BOOK: [Field; 4] = [
    Field::PublisherStyles,
    Field::PublisherFonts,
    Field::ImageScaling,
    Field::NightImages,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::app) enum Value {
    Hinting(FontHinting),
    Kerning(FontKerningMode),
    Styles(bool),
    Fonts(bool),
    SmoothImages(bool),
    InvertImages(bool),
}

impl Value {
    fn field(self) -> Field {
        match self {
            Self::Hinting(_) => Field::Hinting,
            Self::Kerning(_) => Field::Kerning,
            Self::Styles(_) => Field::PublisherStyles,
            Self::Fonts(_) => Field::PublisherFonts,
            Self::SmoothImages(_) => Field::ImageScaling,
            Self::InvertImages(_) => Field::NightImages,
        }
    }

    fn store(self, preferences: &mut BookPreferences) {
        match self {
            Self::Hinting(hinting) => preferences.font_hinting = hinting,
            Self::Kerning(mode) => {
                preferences.font_kerning_mode = Some(mode);
                preferences.font_kerning = Some(mode != FontKerningMode::Off);
            }
            Self::Styles(enabled) => preferences.publisher_styles = Some(enabled),
            Self::Fonts(enabled) => preferences.override_fonts = !enabled,
            Self::SmoothImages(smooth) => preferences.smooth_images = Some(smooth),
            Self::InvertImages(invert) => preferences.invert_night_images = invert,
        }
    }
}

fn choices(field: Field) -> &'static [(&'static str, Value)] {
    match field {
        Field::Hinting => &[
            ("Off", Value::Hinting(FontHinting::Off)),
            ("Native", Value::Hinting(FontHinting::Native)),
            ("Auto", Value::Hinting(FontHinting::Auto)),
        ],
        Field::Kerning => &[
            ("Off", Value::Kerning(FontKerningMode::Off)),
            ("Fast", Value::Kerning(FontKerningMode::Fast)),
            ("Good", Value::Kerning(FontKerningMode::Good)),
            ("Best", Value::Kerning(FontKerningMode::Best)),
        ],
        Field::PublisherStyles => &[("Off", Value::Styles(false)), ("On", Value::Styles(true))],
        Field::PublisherFonts => &[("Off", Value::Fonts(false)), ("On", Value::Fonts(true))],
        Field::ImageScaling => &[
            ("Fast", Value::SmoothImages(false)),
            ("Smooth", Value::SmoothImages(true)),
        ],
        Field::NightImages => &[
            ("Original", Value::InvertImages(false)),
            ("Inverted", Value::InvertImages(true)),
        ],
        _ => &[],
    }
}

fn saved(field: Field, preferences: &BookPreferences) -> Option<Value> {
    match field {
        Field::Hinting => Some(Value::Hinting(preferences.font_hinting)),
        Field::Kerning => preferences.saved_font_kerning_mode().map(Value::Kerning),
        Field::PublisherStyles => preferences.publisher_styles.map(Value::Styles),
        Field::PublisherFonts => Some(Value::Fonts(!preferences.override_fonts)),
        Field::ImageScaling => preferences.smooth_images.map(Value::SmoothImages),
        Field::NightImages => Some(Value::InvertImages(preferences.invert_night_images)),
        _ => None,
    }
}

pub(in crate::app) fn parent(index: usize, field: Field) -> Screen {
    if FONT.contains(&field) {
        Screen::ReadingProfileFontRendering(index)
    } else {
        Screen::ReadingProfileBookRendering(index)
    }
}

impl Reader {
    pub(super) fn profile_rendering_items(&self, index: usize, fields: &[Field]) -> Vec<Item> {
        let Some(profile @ Profile::Text { preferences, .. }) = self.profiles.get(index) else {
            return Vec::new();
        };
        let selection = profile.selection();
        fields
            .iter()
            .map(|&field| {
                let value = saved(field, preferences);
                let label = choices(field)
                    .iter()
                    .find_map(|&(label, option)| (value == Some(option)).then_some(label))
                    .unwrap_or("Default");
                Item::new(
                    field.label(),
                    Action::Show(Screen::ReadingProfileChoice(index, field)),
                )
                .detail(if selection.contains(&field) {
                    label.into()
                } else {
                    format!("{label} · Excluded")
                })
            })
            .collect()
    }

    pub(super) fn profile_rendering_choices(&self, index: usize, field: Field) -> Vec<Item> {
        let Some(Profile::Text { preferences, .. }) = self.profiles.get(index) else {
            return Vec::new();
        };
        let current = saved(field, preferences);
        choices(field)
            .iter()
            .map(|&(label, value)| {
                let item = Item::new(label, Action::SetProfileRendering(index, value));
                if current == Some(value) {
                    item.detail("Selected")
                } else {
                    item
                }
            })
            .collect()
    }

    pub(in crate::app) fn save_profile_rendering_choice(
        &mut self,
        index: usize,
        value: Value,
    ) -> Result<()> {
        let mut profiles = self.profiles.clone();
        let Some(Profile::Text { preferences, .. }) = profiles.get_mut(index) else {
            return Err("Open a reflowable profile to edit these settings".into());
        };
        if saved(value.field(), preferences) != Some(value) {
            value.store(preferences);
            self.commit_profiles(profiles)?;
        }
        self.show(parent(index, value.field()));
        Ok(())
    }
}

#[cfg(test)]
mod tests;
