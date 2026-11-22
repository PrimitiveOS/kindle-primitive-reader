//! Document font units from the pinned Screen/CRE contract. UI DPI is separate.
use crate::{Result, Settings};

pub const PRESETS: [u16; 11] = [12, 16, 20, 22, 24, 26, 28, 30, 34, 38, 44];
pub const DEFAULT: f64 = 22.0;

pub fn valid(value: f64) -> bool {
    value.is_finite() && (12.0..=255.0).contains(&value)
}

/// Captured from the whole screen before facing-page geometry is applied.
/// An explicit DPI override differs from the device's ordinary physical DPI.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScaleContext {
    short_side: u32,
    dpi_override: Option<u16>,
}
impl Default for ScaleContext {
    fn default() -> Self {
        Self::new(1264, 1680)
    }
}
impl ScaleContext {
    pub const fn new(width: u32, height: u32) -> Self {
        Self {
            short_side: if width < height { width } else { height },
            dpi_override: None,
        }
    }
    pub fn with_override(width: u32, height: u32, dpi: Option<u16>) -> Result<Self> {
        if !(160..=4096).contains(&width)
            || !(160..=4096).contains(&height)
            || dpi.is_some_and(|value| !(90..=900).contains(&value))
        {
            return Err("Font scale context is out of range".into());
        }
        Ok(Self {
            short_side: width.min(height),
            dpi_override: dpi,
        })
    }
    pub(crate) fn short_side(self) -> u32 {
        self.short_side
    }
    pub(crate) fn dpi_override(self) -> Option<u16> {
        self.dpi_override
    }
    pub fn requested(self, logical: f64) -> Result<u32> {
        if !valid(logical) || !(160..=4096).contains(&self.short_side) {
            return Err("Enter a font size from 12 to 255".into());
        }
        let size_scale = f64::from(self.short_side) / 600.0;
        let dpi_scale = self
            .dpi_override
            .map_or(size_scale, |dpi| f64::from(dpi) / 160.0);
        Ok((logical * (size_scale + dpi_scale) / 2.0).ceil() as u32)
    }
    pub fn effective(self, logical: f64) -> Result<f32> {
        Ok(self.requested(logical)?.clamp(12, 340) as f32)
    }
}
impl Settings {
    pub fn with_logical_font(self, value: f64) -> Result<Self> {
        Ok(Self {
            font_size: self.font_scale.effective(value)?,
            logical_font_size: Some(value),
            ..self
        })
    }
    pub(crate) fn styled_font(self, scale: f32) -> f32 {
        if self.logical_font_size.is_some() {
            (self.font_size * scale).clamp(8.0, 340.0)
        } else {
            (self.font_size * scale).clamp(12.0, 192.0)
        }
    }
    pub(crate) fn font_label(self) -> String {
        self.logical_font_size
            .map_or_else(|| format!("{} px", self.font_size), |n| format!("{n:.1}"))
    }
}

#[cfg(test)]
mod tests;
