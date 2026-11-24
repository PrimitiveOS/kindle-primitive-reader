//! Values editable in a saved profile without applying it to a book.
use super::{Field, Profile};
use std::{io, ops::RangeInclusive};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ValueField {
    LineSpacing,
    HorizontalOverlap,
    VerticalOverlap,
}

impl ValueField {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::LineSpacing => "Line spacing",
            Self::HorizontalOverlap => "Horizontal overlap",
            Self::VerticalOverlap => "Vertical overlap",
        }
    }

    pub(crate) fn limits(self) -> RangeInclusive<u16> {
        match self {
            Self::LineSpacing => 50..=200,
            Self::HorizontalOverlap | Self::VerticalOverlap => 0..=90,
        }
    }

    pub(crate) fn default(self) -> u16 {
        match self {
            Self::LineSpacing => crate::DEFAULT_LINE_SPACING_PERCENT,
            Self::HorizontalOverlap | Self::VerticalOverlap => 36,
        }
    }

    pub(crate) fn included_in(self, selection: &[Field]) -> bool {
        let field = match self {
            Self::LineSpacing => Field::LineSpacing,
            Self::HorizontalOverlap | Self::VerticalOverlap => Field::PanOverlap,
        };
        selection.contains(&field)
    }
}

impl Profile {
    pub(crate) fn editable_values(&self) -> &'static [ValueField] {
        match self {
            Self::Text { .. } => &[ValueField::LineSpacing],
            Self::Fixed { .. } => &[ValueField::HorizontalOverlap, ValueField::VerticalOverlap],
        }
    }

    pub(crate) fn value(&self, field: ValueField) -> Option<u16> {
        match (self, field) {
            (Self::Text { preferences, .. }, ValueField::LineSpacing) => Some(
                preferences
                    .apply_to(crate::Settings::default())
                    .effective_line_spacing_percent(),
            ),
            (Self::Fixed { fixed, .. }, ValueField::HorizontalOverlap) => {
                Some(u16::from(fixed.values().zoom_overlap_h()))
            }
            (Self::Fixed { fixed, .. }, ValueField::VerticalOverlap) => {
                Some(u16::from(fixed.values().zoom_overlap_v()))
            }
            _ => None,
        }
    }

    pub(crate) fn set_value(&mut self, field: ValueField, value: u16) -> io::Result<()> {
        let limits = field.limits();
        if !limits.contains(&value) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "Enter a value from {}% to {}%",
                    limits.start(),
                    limits.end()
                ),
            ));
        }
        match (self, field) {
            (Self::Text { preferences, .. }, ValueField::LineSpacing) => {
                preferences.line_spacing_percent = Some(value);
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
