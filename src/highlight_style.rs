//! Grayscale annotation drawers and their reading preferences.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum HighlightStyle {
    #[default]
    #[serde(rename = "lighten")]
    Lighten,
    #[serde(rename = "underscore", alias = "underline")]
    Underline,
    #[serde(rename = "strikeout", alias = "strikethrough")]
    Strikethrough,
    #[serde(rename = "invert")]
    Invert,
}

impl HighlightStyle {
    pub const ALL: [Self; 4] = [
        Self::Lighten,
        Self::Underline,
        Self::Strikethrough,
        Self::Invert,
    ];

    pub(crate) fn from_drawer(name: &str) -> Self {
        match name {
            "underscore" | "underline" => Self::Underline,
            "strikeout" | "strikethrough" => Self::Strikethrough,
            "invert" => Self::Invert,
            _ => Self::Lighten,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Lighten => "Lighten",
            Self::Underline => "Underline",
            Self::Strikethrough => "Strikethrough",
            Self::Invert => "Invert",
        }
    }

    pub(crate) fn is_default(&self) -> bool {
        *self == Self::Lighten
    }
}

#[cfg(feature = "kindle")]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Appearance {
    pub highlight_opacity: u8,
    pub selection_opacity: u8,
    pub line_height: u8,
}

#[cfg(feature = "kindle")]
impl Default for Appearance {
    fn default() -> Self {
        Self {
            highlight_opacity: 20,
            selection_opacity: 20,
            line_height: 100,
        }
    }
}

#[cfg(feature = "kindle")]
impl Appearance {
    pub(crate) fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

#[cfg(feature = "kindle")]
#[derive(Clone, Copy)]
pub(crate) enum Paint {
    Saved(HighlightStyle),
    Selection,
    ReflowSelection,
    Temporary(u8),
}

#[cfg(feature = "kindle")]
impl Paint {
    /// KOReader rounds the filled height down, then centers it with the extra pixel above.
    pub(crate) fn fill_band(
        self,
        top: i32,
        height: i32,
        percent: u8,
    ) -> Option<std::ops::Range<i32>> {
        if percent >= 100
            || !matches!(
                self,
                Self::Saved(HighlightStyle::Lighten | HighlightStyle::Invert) | Self::Selection
            )
        {
            return None;
        }
        let fill = height * i32::from(percent) / 100;
        let start = top + (height - fill + 1) / 2;
        Some(start..start + fill)
    }

    pub(crate) fn line_band(
        self,
        y: i32,
        height: i32,
        width: u32,
        fixed: bool,
    ) -> Option<(i32, i32)> {
        match self {
            Self::Saved(HighlightStyle::Underline) => {
                let thickness = (width * 3).div_ceil(1200).max(1) as i32;
                Some((y + height - 1, thickness))
            }
            Self::Saved(HighlightStyle::Strikethrough) => {
                let thickness = width.div_ceil(600).max(1) as i32;
                Some((y + height / 2 + 1 + if fixed { 2 } else { 0 }, thickness))
            }
            _ => None,
        }
    }

    pub(crate) fn draw_row(self, pixels: &mut [u8], appearance: Appearance) {
        match self {
            Self::Saved(HighlightStyle::Lighten) | Self::Selection | Self::ReflowSelection => {
                let multiplier = match self {
                    Self::ReflowSelection => reflow_selection_gray(appearance.selection_opacity),
                    Self::Selection => {
                        255 - u16::from(appearance.selection_opacity.min(100)) * 255 / 100
                    }
                    _ => 255 - u16::from(appearance.highlight_opacity.min(100)) * 255 / 100,
                };
                for pixel in pixels {
                    *pixel = ((u16::from(*pixel) * multiplier + 127) / 255) as u8;
                }
            }
            Self::Saved(HighlightStyle::Underline) => pixels.fill(0x44),
            Self::Saved(HighlightStyle::Strikethrough) => pixels.fill(0),
            Self::Saved(HighlightStyle::Invert) => {
                for pixel in pixels {
                    *pixel = 255 - *pixel;
                }
            }
            Self::Temporary(gray) => {
                for pixel in pixels {
                    *pixel = (*pixel).min(gray);
                }
            }
        }
    }
}

// CREngine receives the hex color emitted by ReaderHighlight::setSelectionColor.
#[cfg(feature = "kindle")]
fn reflow_selection_gray(percent: u8) -> u16 {
    match percent.min(100) {
        0 => 255,
        100 => 0,
        opacity => {
            let value = 256 * (100 - u16::from(opacity)) / 100;
            if value < 16 {
                // Stock repeats a single hex digit three times. CREngine reads it as
                // a 12-bit integer and weights its RGB channels 1:2:1.
                (2 * value + 17 * value) / 4
            } else {
                value
            }
        }
    }
}

#[cfg(all(test, feature = "kindle"))]
mod tests {
    use super::*;

    #[test]
    fn reflow_selection_preserves_the_installed_engine_tones_at_full_line_height() {
        for (p, expected) in [
            (0, 255),
            (5, 243),
            (10, 230),
            (15, 217),
            (20, 204),
            (30, 179),
            (35, 166),
            (50, 128),
            (60, 102),
            (65, 89),
            (90, 25),
            (95, 57),
            (100, 0),
        ] {
            assert_eq!(reflow_selection_gray(p), expected);
            for h in [0, 49, 100] {
                assert_eq!(Paint::ReflowSelection.fill_band(0, 93, h), None);
            }
        }
    }

    #[test]
    fn height_centers_the_stock_band_for_odd_and_even_lines_without_moving_strokes() {
        for h in [13, 14] {
            for pct in [0, 1, 9, 33, 49, 50, 67, 99, 100] {
                for style in HighlightStyle::ALL {
                    let p = Paint::Saved(style);
                    let band = p
                        .line_band(3, h, 1264, false)
                        .map(|(y, n)| y..y + n)
                        .or_else(|| p.fill_band(3, h, pct))
                        .unwrap_or(3..3 + h);
                    let (from, length) = match style {
                        HighlightStyle::Underline => (3 + h - 1, 4),
                        HighlightStyle::Strikethrough => (3 + h / 2 + 1, 3),
                        _ => {
                            let n = (f64::from(h) * f64::from(pct) / 100.).floor() as i32;
                            (3 + (f64::from(h - n) / 2.).ceil() as i32, n)
                        }
                    };
                    assert_eq!(band, from..from + length);
                }
            }
        }
    }

    #[test]
    fn saved_and_selection_opacities_match_the_installed_256_tone_blending() {
        for percent in [0, 5, 10, 15, 20, 30, 35, 50, 65, 90, 95, 100] {
            let gray: Vec<_> = (0..=255u8).collect();
            let expected: Vec<_> = gray
                .iter()
                .map(|&v| {
                    let alpha = (f64::from(percent) * 255.0 / 100.0).floor();
                    (f64::from(v) * (255. - alpha) / 255.).round() as u8
                })
                .collect();
            for p in [Paint::Saved(HighlightStyle::Lighten), Paint::Selection] {
                let a = match p {
                    Paint::Selection => Appearance {
                        highlight_opacity: 80,
                        selection_opacity: percent,
                        ..Default::default()
                    },
                    _ => Appearance {
                        highlight_opacity: percent,
                        selection_opacity: 80,
                        ..Default::default()
                    },
                };
                let mut pixels = gray.clone();
                p.draw_row(&mut pixels, a);
                assert_eq!(pixels, expected);
            }
        }
    }

    #[test]
    fn six_tones_and_line_positions_agree_with_installed_oasis_drawers() {
        let input = [0, 51, 102, 153, 204, 255];
        for fixed in [false, true] {
            for style in HighlightStyle::ALL {
                let paint = Paint::Saved(style);
                let (y, h) = paint.line_band(5, 14, 1264, fixed).unwrap_or((5, 14));
                let drawn = match style {
                    HighlightStyle::Lighten => (5..19, [0, 41, 82, 122, 163, 204]),
                    HighlightStyle::Underline => (18..22, [68; 6]),
                    HighlightStyle::Strikethrough => (if fixed { 15..18 } else { 13..16 }, [0; 6]),
                    HighlightStyle::Invert => (5..19, [255, 204, 153, 102, 51, 0]),
                };
                for row in 4..24 {
                    let mut pixels = input;
                    if (y..y + h).contains(&row) {
                        paint.draw_row(&mut pixels, Appearance::default());
                    }
                    assert_eq!(
                        pixels,
                        if drawn.0.contains(&row) {
                            drawn.1
                        } else {
                            input
                        }
                    );
                }
            }
        }
    }
}
