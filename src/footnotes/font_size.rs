//! Global footnote size preference and its screen-pixel conversion.
use serde::{Deserialize, Serialize};

use crate::Settings;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FontSize {
    Relative(i16),
    Fixed(i16),
}

impl Default for FontSize {
    fn default() -> Self {
        Self::Relative(-2)
    }
}

impl FontSize {
    pub(crate) const RELATIVE_RANGE: std::ops::RangeInclusive<i16> = -10..=5;
    pub(crate) const FIXED_RANGE: std::ops::RangeInclusive<i16> = 12..=255;

    pub(crate) fn is_default(&self) -> bool {
        *self == Self::default()
    }

    pub(crate) fn valid(self) -> bool {
        match self {
            Self::Relative(offset) => Self::RELATIVE_RANGE.contains(&offset),
            Self::Fixed(size) => Self::FIXED_RANGE.contains(&size),
        }
    }

    pub(crate) fn requested_pixels(self, document: Settings) -> f32 {
        match self {
            // KOReader adds the offset after scaling the active document size.
            Self::Relative(offset) => document.font_size + f32::from(offset),
            Self::Fixed(size) => document.font_scale.scaled_pixels(f64::from(size)) as f32,
        }
    }

    pub(crate) fn description(self) -> String {
        match self {
            Self::Relative(offset) => format!("Relative · {offset:+} px"),
            Self::Fixed(value) => format!("Fixed · {value}"),
        }
    }
}
