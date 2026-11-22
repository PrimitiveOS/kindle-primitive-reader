//! Saved annotation drawers. Selection and search keep their own temporary tones.
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
#[derive(Clone, Copy)]
pub(crate) enum Paint {
    Saved(HighlightStyle),
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

    pub(crate) fn draw_row(self, pixels: &mut [u8]) {
        match self {
            Self::Saved(HighlightStyle::Lighten) => {
                for pixel in pixels {
                    *pixel = ((u16::from(*pixel) * 204 + 127) / 255) as u8;
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
                        paint.draw_row(&mut pixels);
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
