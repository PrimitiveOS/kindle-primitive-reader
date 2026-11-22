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

/// Named annotation colors retained for interoperable exports on a grayscale screen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HighlightColor {
    Red,
    Orange,
    Yellow,
    Green,
    Olive,
    Cyan,
    Blue,
    Purple,
    #[default]
    #[serde(other)]
    Gray,
}
impl HighlightColor {
    pub const ALL: [Self; 9] = [
        Self::Red,
        Self::Orange,
        Self::Yellow,
        Self::Green,
        Self::Olive,
        Self::Cyan,
        Self::Blue,
        Self::Purple,
        Self::Gray,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Red => "Red",
            Self::Orange => "Orange",
            Self::Yellow => "Yellow",
            Self::Green => "Green",
            Self::Olive => "Olive",
            Self::Cyan => "Cyan",
            Self::Blue => "Blue",
            Self::Purple => "Purple",
            Self::Gray => "Gray",
        }
    }
    pub(crate) fn from_name(name: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|c| c.label().eq_ignore_ascii_case(name))
            .unwrap_or_default()
    }
    pub(crate) fn is_default(&self) -> bool {
        *self == Self::Gray
    }
    pub(crate) fn rgb(self) -> Option<(u32, u32, u32)> {
        Some(match self {
            Self::Red => (255, 51, 0),
            Self::Orange => (255, 136, 0),
            Self::Yellow => (255, 255, 51),
            Self::Green => (0, 170, 102),
            Self::Olive => (136, 255, 119),
            Self::Cyan => (0, 255, 238),
            Self::Blue => (0, 102, 255),
            Self::Purple => (238, 0, 255),
            Self::Gray => return None,
        })
    }
    pub(crate) fn night_gray(self) -> Option<u8> {
        let (r, g, b) = self.rgb()?;
        Some(((4898 * (255 - r) + 9618 * (255 - g) + 1869 * (255 - b)) >> 14) as u8)
    }
    pub(crate) fn gray(self) -> Option<u8> {
        let (r, g, b) = self.rgb()?;
        Some(((4898 * r + 9618 * g + 1869 * b) >> 14) as u8)
    }
}

#[cfg(feature = "kindle")]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum NoteMarker {
    Underline,
    Sideline,
    Sidemark,
    #[default]
    #[serde(other)]
    None,
}
#[cfg(feature = "kindle")]
impl NoteMarker {
    pub const ALL: [Self; 4] = [Self::None, Self::Underline, Self::Sideline, Self::Sidemark];
    pub fn label(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Underline => "Underline",
            Self::Sideline => "Side line",
            Self::Sidemark => "Side mark",
        }
    }
    pub(crate) fn is_default(&self) -> bool {
        *self == Self::None
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
    Annotation {
        style: HighlightStyle,
        color: HighlightColor,
        note: bool,
        first: bool,
        night: bool,
    },
    Selection,
    ReflowSelection,
    SelectionColor { color: HighlightColor, night: bool, reflow: bool },
    Temporary(u8),
}

#[cfg(feature = "kindle")]
impl Paint {
    /// KOReader rounds the filled height down, then centers it with the extra pixel above.
    pub(crate) fn drawer(self) -> Option<HighlightStyle> {
        match self {
            Self::Saved(style) | Self::Annotation { style, .. } => Some(style),
            _ => None,
        }
    }
    pub(crate) fn note(self) -> Option<(HighlightColor, bool, bool)> {
        match self {
            Self::Annotation {
                color,
                note: true,
                first,
                night,
                ..
            } => Some((color, first, night)),
            _ => None,
        }
    }
    pub(crate) fn fill_band(
        self,
        top: i32,
        height: i32,
        percent: u8,
    ) -> Option<std::ops::Range<i32>> {
        if percent >= 100
            || !(matches!(
                self.drawer(),
                Some(HighlightStyle::Lighten | HighlightStyle::Invert)
            ) || matches!(self, Self::Selection | Self::SelectionColor {reflow:false,..}))
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
        match self.drawer() {
            Some(HighlightStyle::Underline) => {
                let thickness = (width * 3).div_ceil(1200).max(1) as i32;
                Some((y + height - 1, thickness))
            }
            Some(HighlightStyle::Strikethrough) => {
                let thickness = width.div_ceil(600).max(1) as i32;
                Some((y + height / 2 + 1 + if fixed { 2 } else { 0 }, thickness))
            }
            _ => None,
        }
    }

    pub(crate) fn draw_row(self, pixels: &mut [u8], appearance: Appearance) {
        if let Self::SelectionColor {color,night,reflow} = self {
            if reflow {
                // CREngine's native selection background uses rgbToGray,
                // (r+2g+b)/4, rather than the blitbuffer's saved-mark conversion.
                let tone = color.rgb().map(|(r,g,b)| {
                    if night {((255-r)+2*(255-g)+(255-b))/4} else {(r+2*g+b)/4}
                }).unwrap_or_else(||u32::from(reflow_selection_gray(appearance.selection_opacity)));
                for pixel in pixels { *pixel=((u32::from(*pixel)*tone+127)/255) as u8; }
            } else {
                Self::Annotation {style:HighlightStyle::Lighten,color,note:false,first:false,night}
                    .draw_row(pixels,Appearance {highlight_opacity:appearance.selection_opacity,..appearance});
            }
            return;
        }
        if let Self::Annotation {
            style,
            color,
            night,
            ..
        } = self
        {
            if let Some(tone) = color.gray() {
                let stroke = if night {
                    color.night_gray().unwrap()
                } else {
                    tone
                };
                match style {
                    HighlightStyle::Lighten => {
                        for pixel in pixels {
                            *pixel = if night {
                                let alpha =
                                    u16::from(appearance.highlight_opacity.min(100)) * 255 / 100;
                                ((u16::from(*pixel) * (255 - alpha)
                                    + u16::from(stroke) * alpha
                                    + 127)
                                    / 255) as u8
                            } else {
                                ((u16::from(*pixel) * u16::from(tone) + 127) / 255) as u8
                            };
                        }
                    }
                    HighlightStyle::Underline | HighlightStyle::Strikethrough => {
                        pixels.fill(stroke)
                    }
                    HighlightStyle::Invert => {
                        for pixel in pixels {
                            *pixel = 255 - *pixel;
                        }
                    }
                }
                return;
            }
            Self::Saved(style).draw_row(pixels, appearance);
            return;
        }
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
            Self::Annotation { .. } | Self::SelectionColor { .. } => unreachable!(),
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

#[cfg(all(test, feature = "kindle"))]
mod color_tests {
    use super::*;
    #[test]
    fn grayscale_colors_keep_legacy_gray_and_unknown_names_readable() {
        let values: Vec<_> = HighlightColor::ALL.into_iter().map(|c| c.gray()).collect();
        assert_eq!(
            values,
            [
                Some(106),
                Some(156),
                Some(231),
                Some(111),
                Some(203),
                Some(176),
                Some(88),
                Some(100),
                None
            ]
        );
        assert_eq!(
            serde_json::from_str::<HighlightColor>("\"future-color\"").unwrap(),
            HighlightColor::Gray
        );
        for style in HighlightStyle::ALL {
            let mut legacy = [0, 51, 102, 153, 204, 255];
            let mut modern = legacy;
            Paint::Saved(style).draw_row(&mut legacy, Appearance::default());
            Paint::Annotation {
                style,
                color: HighlightColor::Gray,
                note: false,
                first: false,
                night: false,
            }
            .draw_row(&mut modern, Appearance::default());
            assert_eq!(legacy, modern);
        }
    }
    #[test]
    #[ignore = "requires coverage generated by pinned stock C blitbuffer"]
    fn named_color_blending_matches_the_pinned_stock_blitter() {
        let raw =
            std::fs::read(std::env::var_os("READER_COLOR_ORACLE").expect("READER_COLOR_ORACLE"))
                .unwrap();
        assert_eq!(raw.len(), 8 * 23 * 256);
        for (i, color) in HighlightColor::ALL[..8].iter().copied().enumerate() {
            for (n, style, night, opacity) in
                std::iter::once((0, HighlightStyle::Lighten, false, 75))
                    .chain(std::iter::once((1, HighlightStyle::Underline, false, 75)))
                    .chain(
                        (0..=20)
                            .map(|step| (step + 2, HighlightStyle::Lighten, true, step as u8 * 5)),
                    )
            {
                let mut input: Vec<_> = (0..=255).collect();
                Paint::Annotation {
                    style,
                    color,
                    note: false,
                    first: false,
                    night,
                }
                .draw_row(
                    &mut input,
                    Appearance {
                        highlight_opacity: opacity,
                        ..Default::default()
                    },
                );
                assert_eq!(
                    input.as_slice(),
                    &raw[(i * 23 + n) * 256..(i * 23 + n + 1) * 256],
                    "{color:?} night={night} opacity={opacity}"
                );
            }
        }
    }
}
