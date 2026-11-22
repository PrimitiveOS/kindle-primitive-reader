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
}

#[cfg(feature = "kindle")]
impl Default for Appearance {
    fn default() -> Self {
        Self {
            highlight_opacity: 20,
            selection_opacity: 20,
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
    Temporary(u8),
}

#[cfg(feature = "kindle")]
impl Paint {
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
            Self::Saved(HighlightStyle::Lighten) | Self::Selection => {
                let opacity = match self {
                    Self::Selection => appearance.selection_opacity,
                    _ => appearance.highlight_opacity,
                };
                let multiplier = 255 - u16::from(opacity.min(100)) * 255 / 100;
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

#[cfg(all(test, feature = "kindle"))]
mod tests {
    use super::*;

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
                    },
                    _ => Appearance {
                        highlight_opacity: percent,
                        selection_opacity: 80,
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
