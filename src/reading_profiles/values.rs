//! Values editable in a saved profile without applying it to a book.
use super::{Field, Profile};
use std::{io, ops::RangeInclusive};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ValueField {
    FontSize,
    ContentZoom,
    LineSpacing,
    HorizontalOverlap,
    VerticalOverlap,
}

impl ValueField {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::FontSize => "Font size",
            Self::ContentZoom => "Content zoom",
            Self::LineSpacing => "Line spacing",
            Self::HorizontalOverlap => "Horizontal overlap",
            Self::VerticalOverlap => "Vertical overlap",
        }
    }

    pub(crate) fn limits(self) -> RangeInclusive<f64> {
        match self {
            Self::FontSize => 12.0..=255.0,
            Self::ContentZoom => 0.0..=300.0,
            Self::LineSpacing => 50.0..=200.0,
            Self::HorizontalOverlap | Self::VerticalOverlap => 0.0..=90.0,
        }
    }

    pub(crate) fn default(self) -> f64 {
        match self {
            Self::FontSize => crate::logical_font::DEFAULT,
            Self::ContentZoom => f64::from(crate::Settings::default().render_dpi),
            Self::LineSpacing => f64::from(crate::DEFAULT_LINE_SPACING_PERCENT),
            Self::HorizontalOverlap | Self::VerticalOverlap => 36.0,
        }
    }

    pub(crate) fn step(self) -> f64 {
        if self == Self::FontSize { 0.5 } else { 1.0 }
    }

    pub(crate) fn held_steps(self) -> i16 {
        match self {
            Self::FontSize => 10,
            Self::ContentZoom => 20,
            _ => 5,
        }
    }

    pub(crate) fn display(self, value: f64) -> String {
        match self {
            Self::FontSize => value.to_string(),
            Self::ContentZoom => format!("{value} dpi"),
            _ => format!("{value}%"),
        }
    }

    pub(crate) fn range_label(self) -> String {
        let limits = self.limits();
        format!(
            "{} to {}",
            self.display(*limits.start()),
            self.display(*limits.end())
        )
    }

    pub(crate) fn parse(self, value: &str) -> Option<f64> {
        let value = value.trim();
        let number = if self == Self::FontSize {
            value.parse().ok()?
        } else {
            f64::from(value.parse::<u16>().ok()?)
        };
        self.accepts(number).then_some(number)
    }

    fn accepts(self, value: f64) -> bool {
        self.limits().contains(&value) && (self == Self::FontSize || value.fract() == 0.0)
    }

    pub(crate) fn included_in(self, selection: &[Field]) -> bool {
        let field = match self {
            Self::FontSize => Field::FontSize,
            Self::ContentZoom => Field::ContentZoom,
            Self::LineSpacing => Field::LineSpacing,
            Self::HorizontalOverlap | Self::VerticalOverlap => Field::PanOverlap,
        };
        selection.contains(&field)
    }
}

impl Profile {
    pub(crate) fn editable_values(&self) -> &'static [ValueField] {
        match self {
            Self::Text { .. } => &[
                ValueField::FontSize,
                ValueField::ContentZoom,
                ValueField::LineSpacing,
            ],
            Self::Fixed { .. } => &[ValueField::HorizontalOverlap, ValueField::VerticalOverlap],
        }
    }

    pub(crate) fn legacy_font_size(&self) -> Option<f32> {
        match self {
            Self::Text { preferences, .. } if preferences.logical_font_size.is_none() => {
                preferences.font_size
            }
            _ => None,
        }
    }

    pub(crate) fn value(&self, field: ValueField) -> Option<f64> {
        match (self, field) {
            (Self::Text { preferences, .. }, ValueField::FontSize) => Some(
                preferences
                    .logical_font_size
                    .unwrap_or_else(|| field.default()),
            ),
            (Self::Text { preferences, .. }, ValueField::ContentZoom) => Some(
                preferences
                    .render_dpi
                    .map_or_else(|| field.default(), f64::from),
            ),
            (Self::Text { preferences, .. }, ValueField::LineSpacing) => Some(f64::from(
                preferences
                    .apply_to(crate::Settings::default())
                    .effective_line_spacing_percent(),
            )),
            (Self::Fixed { fixed, .. }, ValueField::HorizontalOverlap) => {
                Some(f64::from(fixed.values().zoom_overlap_h()))
            }
            (Self::Fixed { fixed, .. }, ValueField::VerticalOverlap) => {
                Some(f64::from(fixed.values().zoom_overlap_v()))
            }
            _ => None,
        }
    }

    pub(crate) fn set_value(&mut self, field: ValueField, value: f64) -> io::Result<()> {
        if !field.accepts(value) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("Enter a value from {}", field.range_label()),
            ));
        }
        match (self, field) {
            (Self::Text { preferences, .. }, ValueField::FontSize) => {
                preferences.logical_font_size = Some(value);
            }
            (Self::Text { preferences, .. }, ValueField::ContentZoom) => {
                preferences.render_dpi = Some(value as u16);
            }
            (Self::Text { preferences, .. }, ValueField::LineSpacing) => {
                preferences.line_spacing_percent = Some(value as u16);
            }
            (
                Self::Fixed { fixed, .. },
                field @ (ValueField::HorizontalOverlap | ValueField::VerticalOverlap),
            ) => {
                fixed.set_overlap(field == ValueField::HorizontalOverlap, value as u8);
            }
            _ => {
                return Err(io::Error::other(
                    "That setting is unavailable in this profile",
                ));
            }
        }
        Ok(())
    }
}
