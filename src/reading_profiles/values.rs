//! Values editable in a saved profile without applying it to a book.
use super::{Field, FixedProvider, Profile};
use crate::document_defaults::FixedDefaults;
use crate::font_render::{GAMMA_LEVELS, NORMAL_GAMMA};
use crate::library::{
    BookPreferences, DEFAULT_CROP_MARGIN, FixedContentFit, FixedTileAxis, MAX_CROP_MARGIN,
    MAX_STRAIGHTEN_DEGREES, SCAN_GAMMA_DEFAULT, SCAN_GAMMA_MAX, SCAN_GAMMA_MIN,
};
use std::{io, ops::RangeInclusive};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ValueField {
    FontSize,
    ContentZoom,
    LineSpacing,
    WordExpansion,
    LetterSpacing,
    CjkWidth,
    ManualZoomFactor,
    TileCount(FixedTileAxis),
    FontWeight,
    FontContrast,
    Margin(MarginEdge),
    CropMargin,
    PageRotation,
    PageGap,
    ScanGamma,
    ScanWhiteLevel,
    AutoStraighten,
    HorizontalOverlap,
    VerticalOverlap,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ProfileZoomTarget {
    Page,
    Content,
    Columns,
    Rows,
    Manual,
    Custom,
}

impl ProfileZoomTarget {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Page => "Page",
            Self::Content => "Content",
            Self::Columns => "Columns",
            Self::Rows => "Rows",
            Self::Manual => "Manual",
            Self::Custom => "Custom zoom",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ProfileZoomChoice {
    Target(ProfileZoomTarget),
    Fit(FixedContentFit),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MarginEdge {
    Left,
    Right,
    Top,
    Bottom,
}

impl MarginEdge {
    fn label(self) -> &'static str {
        match self {
            Self::Left => "Left margin",
            Self::Right => "Right margin",
            Self::Top => "Top margin",
            Self::Bottom => "Bottom margin",
        }
    }

    pub(crate) fn is_vertical(self) -> bool {
        matches!(self, Self::Top | Self::Bottom)
    }

    fn value(self, preferences: &BookPreferences) -> u32 {
        let inherited = preferences
            .margin
            .unwrap_or(crate::Settings::default().margin);
        match self {
            Self::Left => preferences.margin_left.or(preferences.margin_sides),
            Self::Right => preferences.margin_right.or(preferences.margin_sides),
            Self::Top => preferences.margin_top,
            Self::Bottom => preferences.margin_bottom,
        }
        .unwrap_or(inherited)
    }

    fn set(self, preferences: &mut BookPreferences, value: u32) {
        match self {
            Self::Left => preferences.margin_left = Some(value),
            Self::Right => preferences.margin_right = Some(value),
            _ if preferences.link_vertical_margins => {
                preferences.margin_top = Some(value);
                preferences.margin_bottom = Some(value);
            }
            Self::Top => preferences.margin_top = Some(value),
            Self::Bottom => preferences.margin_bottom = Some(value),
        }
    }
}

impl ValueField {
    pub(crate) const MARGINS: [Self; 4] = [
        Self::Margin(MarginEdge::Left),
        Self::Margin(MarginEdge::Right),
        Self::Margin(MarginEdge::Top),
        Self::Margin(MarginEdge::Bottom),
    ];

    pub(crate) const FONT_RENDERING: [Self; 2] = [Self::FontWeight, Self::FontContrast];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::FontSize => "Font size",
            Self::ContentZoom => "Content zoom",
            Self::LineSpacing => "Line spacing",
            Self::WordExpansion => "Word expansion",
            Self::LetterSpacing => "Letter spacing",
            Self::CjkWidth => "CJK character width",
            Self::ManualZoomFactor => "Manual zoom factor",
            Self::TileCount(FixedTileAxis::Columns) => "Zoom columns",
            Self::TileCount(FixedTileAxis::Rows) => "Zoom rows",
            Self::FontWeight => "Font weight",
            Self::FontContrast => "Font contrast",
            Self::Margin(edge) => edge.label(),
            Self::CropMargin => "Crop margin",
            Self::PageRotation => "Page rotation",
            Self::PageGap => "Page gap",
            Self::ScanGamma => "Scan contrast",
            Self::ScanWhiteLevel => "Scan white level",
            Self::AutoStraighten => "Auto straighten",
            Self::HorizontalOverlap => "Horizontal overlap",
            Self::VerticalOverlap => "Vertical overlap",
        }
    }

    pub(crate) fn limits(self) -> RangeInclusive<f64> {
        match self {
            Self::FontSize => 12.0..=255.0,
            Self::ContentZoom => 0.0..=300.0,
            Self::LineSpacing => 50.0..=200.0,
            Self::WordExpansion => 0.0..=20.0,
            Self::LetterSpacing => -1.0..=4.0,
            Self::CjkWidth => 100.0..=150.0,
            Self::ManualZoomFactor => 0.1..=20.0,
            Self::TileCount(_) => 0.1..=8.0,
            Self::FontWeight => -3.0..=5.5,
            Self::FontContrast => GAMMA_LEVELS[0]..=GAMMA_LEVELS[GAMMA_LEVELS.len() - 1],
            Self::Margin(_) => 0.0..=f64::from(crate::document_defaults::MAX_SAVED_MARGIN),
            Self::CropMargin => 0.0..=f64::from(MAX_CROP_MARGIN) / 100.0,
            Self::PageRotation => 0.0..=270.0,
            Self::PageGap => 0.0..=f64::from(crate::library::MAX_CONTINUOUS_GAP),
            Self::ScanGamma => f64::from(SCAN_GAMMA_MIN) / 10.0..=f64::from(SCAN_GAMMA_MAX) / 10.0,
            Self::ScanWhiteLevel => 0.0..=255.0,
            Self::AutoStraighten => 0.0..=f64::from(MAX_STRAIGHTEN_DEGREES),
            Self::HorizontalOverlap | Self::VerticalOverlap => 0.0..=90.0,
        }
    }

    pub(crate) fn default(self) -> f64 {
        match self {
            Self::FontSize => crate::logical_font::DEFAULT,
            Self::ContentZoom => f64::from(crate::Settings::default().render_dpi),
            Self::LineSpacing => f64::from(crate::DEFAULT_LINE_SPACING_PERCENT),
            Self::WordExpansion => f64::from(crate::Settings::default().word_expansion),
            Self::LetterSpacing => 0.0,
            Self::CjkWidth => 100.0,
            Self::ManualZoomFactor => 1.5,
            Self::TileCount(_) => 2.0,
            Self::FontWeight => 0.0,
            Self::FontContrast => GAMMA_LEVELS[usize::from(NORMAL_GAMMA)],
            Self::Margin(_) => f64::from(crate::Settings::default().margin),
            Self::CropMargin => f64::from(DEFAULT_CROP_MARGIN) / 100.0,
            Self::PageRotation | Self::PageGap => 0.0,
            Self::ScanGamma => f64::from(SCAN_GAMMA_DEFAULT) / 10.0,
            Self::ScanWhiteLevel => 255.0,
            Self::AutoStraighten => 0.0,
            Self::HorizontalOverlap | Self::VerticalOverlap => 36.0,
        }
    }

    pub(crate) fn step(self) -> f64 {
        match self {
            Self::FontSize => 0.5,
            Self::FontWeight => 0.25,
            Self::LetterSpacing => 0.25,
            Self::ManualZoomFactor | Self::TileCount(_) => 0.1,
            Self::CropMargin => 0.01,
            Self::PageRotation => 90.0,
            Self::ScanGamma => 0.1,
            _ => 1.0,
        }
    }

    pub(crate) fn held_steps(self) -> i16 {
        match self {
            Self::FontSize => 10,
            Self::ContentZoom => 20,
            Self::PageRotation => 1,
            Self::ManualZoomFactor => 5,
            Self::TileCount(_) => 5,
            Self::WordExpansion | Self::LetterSpacing | Self::FontWeight => 4,
            Self::CropMargin | Self::ScanGamma => 10,
            _ => 5,
        }
    }

    pub(crate) fn display(self, value: f64) -> String {
        match self {
            Self::FontSize => value.to_string(),
            Self::FontWeight => format!("{value:+.2}"),
            Self::FontContrast => format!("{value:.2}"),
            Self::LetterSpacing => format!("{value:+.2} em"),
            Self::ContentZoom => format!("{value} dpi"),
            Self::Margin(_) => format!("{value} px"),
            Self::CropMargin => format!("{value:.2} in"),
            Self::ManualZoomFactor => format_zoom_factor(value),
            Self::TileCount(axis) => format!("{value:.1} {}", axis.label().to_lowercase()),
            Self::PageRotation => format!("{value:.0}°"),
            Self::PageGap => format!("{value:.0} px"),
            Self::ScanGamma => format!("{value:.1}"),
            Self::ScanWhiteLevel => format!("Level {value:.0}"),
            Self::AutoStraighten => format!("{value}°"),
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
        let number = if self.supports_decimals() {
            value.parse().ok()?
        } else {
            f64::from(value.parse::<u16>().ok()?)
        };
        self.accepts(number).then_some(number)
    }

    fn accepts(self, value: f64) -> bool {
        if self == Self::FontContrast {
            return gamma_index(value).is_some();
        }
        if self == Self::PageRotation {
            return self.limits().contains(&value) && value % 90.0 == 0.0;
        }
        self.limits().contains(&value)
            && if let Some(scale) = self.decimal_scale() {
                (value * scale).round() / scale == value
            } else {
                self.supports_decimals() || value.fract() == 0.0
            }
    }

    fn supports_decimals(self) -> bool {
        matches!(
            self,
            Self::FontSize
                | Self::ManualZoomFactor
                | Self::TileCount(_)
                | Self::FontWeight
                | Self::FontContrast
                | Self::LetterSpacing
        )
            || self.decimal_scale().is_some()
    }

    fn decimal_scale(self) -> Option<f64> {
        match self {
            Self::CropMargin => Some(100.0),
            Self::ManualZoomFactor => Some(100.0),
            Self::TileCount(_) => Some(10.0),
            Self::ScanGamma => Some(10.0),
            Self::LetterSpacing => Some(4.0),
            _ => None,
        }
    }

    pub(crate) fn stepped(self, value: f64, steps: i16) -> f64 {
        if self == Self::FontContrast {
            let index = gamma_index(value).unwrap_or(NORMAL_GAMMA);
            let next = (i16::from(index) + steps).clamp(0, (GAMMA_LEVELS.len() - 1) as i16);
            return GAMMA_LEVELS[next as usize];
        }
        let limits = self.limits();
        let next = (value + f64::from(steps) * self.step()).clamp(*limits.start(), *limits.end());
        self.decimal_scale()
            .map_or(next, |scale| (next * scale).round() / scale)
    }

    pub(crate) fn input_error(self) -> String {
        if self == Self::FontContrast {
            return "Use -/+ to select a supported gamma level".into();
        }
        format!("Enter a value from {}", self.range_label())
    }

    pub(crate) fn included_in(self, selection: &[Field]) -> bool {
        let field = match self {
            Self::FontSize => Field::FontSize,
            Self::ContentZoom => Field::ContentZoom,
            Self::LineSpacing => Field::LineSpacing,
            Self::WordExpansion => Field::WordExpansion,
            Self::LetterSpacing => Field::LetterSpacing,
            Self::CjkWidth => Field::CjkWidth,
            Self::ManualZoomFactor => Field::ManualZoomFactor,
            Self::TileCount(FixedTileAxis::Columns) => Field::ZoomColumns,
            Self::TileCount(FixedTileAxis::Rows) => Field::ZoomRows,
            Self::FontWeight => Field::FontWeight,
            Self::FontContrast => Field::FontContrast,
            Self::Margin(_) => Field::Margins,
            Self::CropMargin => Field::CropMargin,
            Self::PageRotation => Field::PageRotation,
            Self::PageGap => Field::ScrollGap,
            Self::ScanGamma => Field::ScanContrast,
            Self::ScanWhiteLevel => Field::ScanWhites,
            Self::AutoStraighten => Field::Straighten,
            Self::HorizontalOverlap | Self::VerticalOverlap => Field::PanOverlap,
        };
        selection.contains(&field)
    }
}

fn gamma_index(value: f64) -> Option<u8> {
    GAMMA_LEVELS
        .iter()
        .position(|&level| level == value)
        .map(|index| index as u8)
}

fn format_zoom_factor(value: f64) -> String {
    let hundredths = (value * 100.0).round() as u16;
    let whole = hundredths / 100;
    let fraction = hundredths % 100;
    let number = if fraction == 0 {
        whole.to_string()
    } else if fraction % 10 == 0 {
        format!("{whole}.{}", fraction / 10)
    } else {
        format!("{whole}.{fraction:02}")
    };
    format!("{number}×")
}

impl Profile {
    pub(crate) fn zoom_target(&self) -> Option<ProfileZoomTarget> {
        let Self::Fixed { fixed, .. } = self else {
            return None;
        };
        let view = fixed.values();
        Some(if let Some(tile) = fixed.tile {
            match tile.axis {
                FixedTileAxis::Columns => ProfileZoomTarget::Columns,
                FixedTileAxis::Rows => ProfileZoomTarget::Rows,
            }
        } else if view.manual_zoom {
            ProfileZoomTarget::Manual
        } else if view.content_fit.is_some() {
            ProfileZoomTarget::Content
        } else if view.zoom > 0 || view.precise_scale.is_some() {
            ProfileZoomTarget::Custom
        } else {
            ProfileZoomTarget::Page
        })
    }

    pub(crate) fn zoom_fit_axis(&self) -> Option<FixedContentFit> {
        let Self::Fixed { fixed, .. } = self else {
            return None;
        };
        let view = fixed.values();
        Some(view.zoom_fit_axis.unwrap_or_else(|| {
            view.content_fit.unwrap_or_else(|| {
                if view.width_fit {
                    FixedContentFit::Width
                } else if view.height_fit {
                    FixedContentFit::Height
                } else {
                    FixedContentFit::Full
                }
            })
        }))
    }

    pub(crate) fn set_zoom_choice(&mut self, choice: ProfileZoomChoice) -> io::Result<()> {
        let Self::Fixed { fixed, .. } = self else {
            return Err(io::Error::other(
                "Zoom targets require a fixed-page profile",
            ));
        };
        let mut view = fixed.values();
        let fit_axis = view.zoom_fit_axis.unwrap_or_else(|| {
            view.content_fit.unwrap_or_else(|| {
                if view.width_fit {
                    FixedContentFit::Width
                } else if view.height_fit {
                    FixedContentFit::Height
                } else {
                    FixedContentFit::Full
                }
            })
        });
        match choice {
            ProfileZoomChoice::Fit(fit) => {
                view.zoom_fit_axis = Some(fit);
                if view.content_fit.is_some() {
                    view.content_fit = Some(fit);
                } else if view.zoom == 0 && !view.manual_zoom && view.precise_scale.is_none() {
                    view.width_fit = fit == FixedContentFit::Width;
                    view.height_fit = fit == FixedContentFit::Height;
                    view.continuous = view.width_fit && view.continuous;
                }
            }
            ProfileZoomChoice::Target(ProfileZoomTarget::Page) => {
                view.zoom = 0;
                view.tile_zoom = None;
                view.precise_scale = None;
                view.manual_zoom = false;
                view.content_fit = None;
                view.zoom_fit_axis = Some(fit_axis);
                view.width_fit = fit_axis == FixedContentFit::Width;
                view.height_fit = fit_axis == FixedContentFit::Height;
                view.continuous = view.width_fit && view.continuous;
                fixed.tile = None;
            }
            ProfileZoomChoice::Target(ProfileZoomTarget::Content) => {
                view.zoom = 0;
                view.tile_zoom = None;
                view.precise_scale = None;
                view.manual_zoom = false;
                view.content_fit = Some(fit_axis);
                view.zoom_fit_axis = Some(fit_axis);
                view.width_fit = false;
                view.height_fit = false;
                view.continuous = false;
                fixed.tile = None;
            }
            ProfileZoomChoice::Target(ProfileZoomTarget::Columns | ProfileZoomTarget::Rows) => {
                if view.spread
                    || (fixed.provider == FixedProvider::Epub
                        && view.epub_spread.unwrap_or(false))
                {
                    return Err(io::Error::other(
                        "Tile zoom is unavailable while facing pages are enabled",
                    ));
                }
                let axis = match choice {
                    ProfileZoomChoice::Target(ProfileZoomTarget::Columns) => {
                        FixedTileAxis::Columns
                    }
                    ProfileZoomChoice::Target(ProfileZoomTarget::Rows) => FixedTileAxis::Rows,
                    _ => unreachable!(),
                };
                let stored = match axis {
                    FixedTileAxis::Columns => view.zoom_columns_tenths,
                    FixedTileAxis::Rows => view.zoom_rows_tenths,
                };
                let count_tenths = fixed
                    .tile
                    .filter(|tile| tile.axis == axis)
                    .map(|tile| tile.count_tenths)
                    .or(stored)
                    .unwrap_or(20);
                view.zoom = 0;
                view.tile_zoom = None;
                view.precise_scale = None;
                view.manual_zoom = false;
                view.content_fit = None;
                view.zoom_fit_axis = Some(fit_axis);
                view.width_fit = false;
                view.height_fit = false;
                view.continuous = false;
                fixed.tile = Some(crate::reading_profiles::Tile {
                    axis,
                    count_tenths,
                });
            }
            ProfileZoomChoice::Target(ProfileZoomTarget::Manual) => {
                if view.spread
                    || (fixed.provider == FixedProvider::Epub
                        && view.epub_spread.unwrap_or(false))
                {
                    return Err(io::Error::other(
                        "Manual zoom is unavailable while facing pages are enabled",
                    ));
                }
                view.zoom = 0;
                view.tile_zoom = None;
                view.precise_scale = None;
                view.manual_zoom = true;
                view.content_fit = None;
                view.zoom_fit_axis = Some(fit_axis);
                view.width_fit = false;
                view.height_fit = false;
                view.continuous = false;
                view.zoom_factor_hundredths.get_or_insert(150);
                fixed.tile = None;
            }
            ProfileZoomChoice::Target(ProfileZoomTarget::Custom) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Custom zoom is set by the current book view",
                ));
            }
        }
        fixed.settings = FixedDefaults::from_view(view);
        Ok(())
    }

    pub(crate) fn editable_values(&self) -> &'static [ValueField] {
        match self {
            Self::Text { .. } => &[
                ValueField::FontSize,
                ValueField::ContentZoom,
                ValueField::LineSpacing,
                ValueField::WordExpansion,
                ValueField::LetterSpacing,
                ValueField::CjkWidth,
            ],
            Self::Fixed { fixed, .. } => match fixed.provider {
                FixedProvider::Pdf => &[
                    ValueField::PageRotation,
                    ValueField::PageGap,
                    ValueField::ManualZoomFactor,
                    ValueField::TileCount(FixedTileAxis::Columns),
                    ValueField::TileCount(FixedTileAxis::Rows),
                    ValueField::HorizontalOverlap,
                    ValueField::VerticalOverlap,
                    ValueField::CropMargin,
                    ValueField::ScanGamma,
                    ValueField::ScanWhiteLevel,
                    ValueField::AutoStraighten,
                ],
                FixedProvider::Comic => &[
                    ValueField::PageRotation,
                    ValueField::ManualZoomFactor,
                    ValueField::TileCount(FixedTileAxis::Columns),
                    ValueField::TileCount(FixedTileAxis::Rows),
                    ValueField::HorizontalOverlap,
                    ValueField::VerticalOverlap,
                    ValueField::ScanGamma,
                    ValueField::ScanWhiteLevel,
                ],
                FixedProvider::Djvu => &[
                    ValueField::PageRotation,
                    ValueField::ManualZoomFactor,
                    ValueField::TileCount(FixedTileAxis::Columns),
                    ValueField::TileCount(FixedTileAxis::Rows),
                    ValueField::HorizontalOverlap,
                    ValueField::VerticalOverlap,
                    ValueField::ScanWhiteLevel,
                    ValueField::AutoStraighten,
                ],
                FixedProvider::Epub | FixedProvider::Image => &[
                    ValueField::PageRotation,
                    ValueField::ManualZoomFactor,
                    ValueField::TileCount(FixedTileAxis::Columns),
                    ValueField::TileCount(FixedTileAxis::Rows),
                    ValueField::HorizontalOverlap,
                    ValueField::VerticalOverlap,
                    ValueField::AutoStraighten,
                ],
            },
        }
    }

    pub(crate) fn set_word_spacing(&mut self, value: crate::WordSpacePercent) -> io::Result<()> {
        if !value.valid() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "The spacing percentages are out of range",
            ));
        }
        let Self::Text { preferences, .. } = self else {
            return Err(io::Error::other(
                "Word spacing requires a reflowable profile",
            ));
        };
        preferences.percent_word_spacing = Some(value);
        Ok(())
    }

    pub(crate) fn margins_linked(&self) -> Option<bool> {
        match self {
            Self::Text { preferences, .. } => Some(preferences.link_vertical_margins),
            _ => None,
        }
    }

    pub(crate) fn toggle_margin_link(&mut self) -> io::Result<()> {
        let Self::Text { preferences, .. } = self else {
            return Err(io::Error::other(
                "Page margins require a reflowable profile",
            ));
        };
        preferences.link_vertical_margins = !preferences.link_vertical_margins;
        if preferences.link_vertical_margins {
            let top = MarginEdge::Top.value(preferences);
            let bottom = MarginEdge::Bottom.value(preferences);
            MarginEdge::Top.set(preferences, (top + bottom).div_ceil(2));
        }
        Ok(())
    }

    pub(crate) fn legacy_value_label(&self, field: ValueField) -> Option<&'static str> {
        match (self, field) {
            (Self::Text { preferences, .. }, ValueField::FontWeight) if preferences.bold_text => {
                Some("Bold (legacy)")
            }
            (Self::Text { preferences, .. }, ValueField::FontContrast)
                if preferences.text_contrast =>
            {
                Some("High contrast (legacy)")
            }
            (Self::Fixed { fixed, .. }, ValueField::ScanGamma) if fixed.values().scan_contrast => {
                Some("Adaptive")
            }
            (Self::Fixed { fixed, .. }, ValueField::ScanWhiteLevel)
                if fixed.values().scan_white_level.is_none() =>
            {
                Some("Original")
            }
            _ => None,
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
            (Self::Text { preferences, .. }, ValueField::WordExpansion) => Some(
                preferences
                    .word_expansion
                    .map_or_else(|| field.default(), f64::from),
            ),
            (Self::Text { preferences, .. }, ValueField::LetterSpacing) => {
                Some(f64::from(preferences.letter_spacing))
            }
            (Self::Text { preferences, .. }, ValueField::CjkWidth) => Some(
                preferences
                    .cjk_width_percent
                    .map_or_else(|| field.default(), f64::from),
            ),
            (Self::Text { preferences, .. }, ValueField::FontWeight) => Some(
                preferences
                    .font_weight_delta
                    .map_or_else(|| field.default(), f64::from),
            ),
            (Self::Text { preferences, .. }, ValueField::FontContrast) => Some(
                preferences
                    .font_gamma_index
                    .and_then(|index| GAMMA_LEVELS.get(usize::from(index)).copied())
                    .unwrap_or_else(|| field.default()),
            ),
            (Self::Text { preferences, .. }, ValueField::Margin(edge)) => {
                Some(f64::from(edge.value(preferences)))
            }
            (Self::Fixed { fixed, .. }, ValueField::PageRotation) => {
                Some(f64::from(fixed.values().turns) * 90.0)
            }
            (Self::Fixed { fixed, .. }, ValueField::PageGap)
                if fixed.provider == FixedProvider::Pdf =>
            {
                Some(f64::from(fixed.values().continuous_gap))
            }
            (Self::Fixed { fixed, .. }, ValueField::ManualZoomFactor) => Some(
                f64::from(fixed.values().zoom_factor_hundredths.unwrap_or(150)) / 100.0,
            ),
            (Self::Fixed { fixed, .. }, ValueField::TileCount(axis)) => {
                let view = fixed.values();
                let stored = match axis {
                    FixedTileAxis::Columns => view.zoom_columns_tenths,
                    FixedTileAxis::Rows => view.zoom_rows_tenths,
                };
                let count = fixed
                    .tile
                    .filter(|tile| tile.axis == axis)
                    .map(|tile| tile.count_tenths)
                    .or(stored)
                    .unwrap_or(20);
                Some(f64::from(count) / 10.0)
            }
            (Self::Fixed { fixed, .. }, ValueField::HorizontalOverlap) => {
                Some(f64::from(fixed.values().zoom_overlap_h()))
            }
            (Self::Fixed { fixed, .. }, ValueField::VerticalOverlap) => {
                Some(f64::from(fixed.values().zoom_overlap_v()))
            }
            (Self::Fixed { fixed, .. }, ValueField::CropMargin)
                if fixed.provider == FixedProvider::Pdf =>
            {
                Some(f64::from(fixed.values().crop_margin()) / 100.0)
            }
            (Self::Fixed { fixed, .. }, ValueField::ScanGamma)
                if matches!(fixed.provider, FixedProvider::Pdf | FixedProvider::Comic) =>
            {
                let gamma = fixed
                    .values()
                    .scan_gamma_tenths
                    .unwrap_or(SCAN_GAMMA_DEFAULT);
                Some(f64::from(gamma) / 10.0)
            }
            (Self::Fixed { fixed, .. }, ValueField::ScanWhiteLevel)
                if matches!(
                    fixed.provider,
                    FixedProvider::Pdf | FixedProvider::Djvu | FixedProvider::Comic
                ) =>
            {
                Some(
                    fixed
                        .values()
                        .scan_white_level
                        .map_or_else(|| field.default(), f64::from),
                )
            }
            (Self::Fixed { fixed, .. }, ValueField::AutoStraighten)
                if fixed.provider != FixedProvider::Comic =>
            {
                Some(f64::from(fixed.values().straighten_limit))
            }
            _ => None,
        }
    }

    pub(crate) fn set_value(&mut self, field: ValueField, value: f64) -> io::Result<()> {
        if !field.accepts(value) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                field.input_error(),
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
            (Self::Text { preferences, .. }, ValueField::WordExpansion) => {
                preferences.word_expansion = Some(value as u8);
            }
            (Self::Text { preferences, .. }, ValueField::LetterSpacing) => {
                preferences.letter_spacing = value as f32;
            }
            (Self::Text { preferences, .. }, ValueField::CjkWidth) => {
                preferences.cjk_width_percent = Some(value as u8);
            }
            (Self::Text { preferences, .. }, ValueField::FontWeight) => {
                preferences.font_weight_delta = Some(value as f32);
                preferences.bold_text = false;
            }
            (Self::Text { preferences, .. }, ValueField::FontContrast) => {
                preferences.font_gamma_index = gamma_index(value);
                preferences.text_contrast = false;
            }
            (Self::Text { preferences, .. }, ValueField::Margin(edge)) => {
                edge.set(preferences, value as u32);
            }
            (Self::Fixed { fixed, .. }, field) => {
                let mut view = fixed.values();
                match field {
                    ValueField::PageRotation => view.turns = (value / 90.0) as u8,
                    ValueField::PageGap if fixed.provider == FixedProvider::Pdf => {
                        view.continuous_gap = value as u16;
                    }
                    ValueField::ManualZoomFactor => {
                        view.zoom_factor_hundredths = Some((value * 100.0).round() as u16);
                    }
                    ValueField::TileCount(axis) => {
                        let count_tenths = (value * 10.0).round() as u8;
                        match axis {
                            FixedTileAxis::Columns => view.zoom_columns_tenths = Some(count_tenths),
                            FixedTileAxis::Rows => view.zoom_rows_tenths = Some(count_tenths),
                        }
                        if let Some(tile) = fixed.tile.as_mut()
                            && tile.axis == axis
                        {
                            tile.count_tenths = count_tenths;
                        }
                    }
                    ValueField::HorizontalOverlap => view.zoom_overlap_h = Some(value as u8),
                    ValueField::VerticalOverlap => view.zoom_overlap_v = Some(value as u8),
                    ValueField::CropMargin if fixed.provider == FixedProvider::Pdf => {
                        let hundredths = (value * 100.0).round() as u8;
                        view.crop_margin_hundredths =
                            (hundredths != DEFAULT_CROP_MARGIN).then_some(hundredths);
                    }
                    ValueField::ScanGamma
                        if matches!(fixed.provider, FixedProvider::Pdf | FixedProvider::Comic) =>
                    {
                        let tenths = (value * 10.0).round() as u16;
                        view.scan_contrast = false;
                        view.scan_gamma_tenths = (tenths != SCAN_GAMMA_DEFAULT).then_some(tenths);
                    }
                    ValueField::ScanWhiteLevel
                        if matches!(
                            fixed.provider,
                            FixedProvider::Pdf | FixedProvider::Djvu | FixedProvider::Comic
                        ) =>
                    {
                        view.scan_white_level = (value != 255.0).then_some(value as u8);
                    }
                    ValueField::AutoStraighten if fixed.provider != FixedProvider::Comic => {
                        view.straighten_limit = value as u8;
                    }
                    _ => return Err(unavailable()),
                }
                fixed.settings = FixedDefaults::from_view(view);
            }
            _ => return Err(unavailable()),
        }
        Ok(())
    }
}

fn unavailable() -> io::Error {
    io::Error::other("That setting is unavailable in this profile")
}
