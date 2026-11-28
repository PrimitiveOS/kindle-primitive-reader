//! Explicit format defaults, applied only to previously unseen content.
use crate::library::{BookPreferences, FixedPreferences};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
};

pub(crate) const FILE: &str = "document-defaults.json";
pub(crate) const MAX_BYTES: u64 = 64 * 1024;
pub(crate) const MAX_SAVED_MARGIN: u32 = 1024;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Templates {
    version: u32,
    pub(crate) reflow: Option<BookPreferences>,
    pub(crate) fixed: Option<FixedDefaults>,
}
impl Default for Templates {
    fn default() -> Self {
        Self {
            version: 1,
            reflow: None,
            fixed: None,
        }
    }
}

/// Per-format defaults for fixed-page navigation and rendering.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct FixedDefaults {
    zoom: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    zoom_factor_hundredths: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    zoom_columns_tenths: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    zoom_rows_tenths: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    zoom_fit_axis: Option<crate::library::FixedContentFit>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) manual_zoom: Option<bool>,
    turns: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    content_fit: Option<crate::library::FixedContentFit>,
    width_fit: bool,
    height_fit: bool,
    continuous: bool,
    spread: bool,
    scan_contrast: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    scan_gamma_tenths: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    crop_margin_hundredths: Option<u8>,
    straighten_limit: u8,
    force_ocr: bool,
    dither: bool,
    continuous_gap: u16,
    scan_white_level: Option<u8>,
    dewatermark: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    fixed_reflow_spacing: Option<crate::library::FixedReflowSpacing>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) fixed_reflow_columns: Option<crate::library::FixedReflowColumns>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) fixed_reflow_word_gap: Option<crate::library::FixedReflowWordGap>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) fixed_reflow_alignment: Option<crate::library::FixedReflowAlignment>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) fixed_reflow_direction: Option<crate::library::FixedReflowDirection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) fixed_text_reflow_direction: Option<crate::library::FixedTextReflowDirection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) fixed_reflow_text_scale: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) fixed_reflow_text_height: Option<u8>,
    right_to_left: bool,
    zoom_overlap_h: u8,
    zoom_overlap_v: u8,
    page_overlap_enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    page_overlap_style: Option<crate::PageOverlapStyle>,
    zoom_direction_vertical: bool,
    zoom_bottom_to_top: bool,
    epub_spread: Option<bool>,
    epub_right_to_left: Option<bool>,
}

impl Default for FixedDefaults {
    fn default() -> Self {
        Self {
            zoom: 0,
            zoom_factor_hundredths: None,
            zoom_columns_tenths: None,
            zoom_rows_tenths: None,
            zoom_fit_axis: None,
            manual_zoom: None,
            turns: 0,
            content_fit: None,
            // KOReader's pinned default zoom mode is `pagewidth`.
            width_fit: true,
            height_fit: false,
            continuous: false,
            spread: false,
            scan_contrast: false,
            scan_gamma_tenths: None,
            crop_margin_hundredths: None,
            straighten_limit: 0,
            force_ocr: false,
            dither: false,
            continuous_gap: 0,
            scan_white_level: None,
            dewatermark: false,
            fixed_reflow_spacing: None,
            fixed_reflow_columns: None,
            fixed_reflow_word_gap: None,
            fixed_reflow_alignment: None,
            fixed_reflow_direction: None,
            fixed_text_reflow_direction: None,
            fixed_reflow_text_scale: None,
            fixed_reflow_text_height: None,
            right_to_left: false,
            zoom_overlap_h: 40,
            zoom_overlap_v: 40,
            page_overlap_enabled: false,
            page_overlap_style: None,
            zoom_direction_vertical: false,
            zoom_bottom_to_top: false,
            epub_spread: None,
            epub_right_to_left: None,
        }
    }
}

impl FixedDefaults {
    pub(crate) fn migrated(self) -> Self {
        Self {
            fixed_reflow_text_scale: self.fixed_reflow_text_scale.or_else(|| {
                self.fixed_reflow_text_height
                    .and_then(crate::library::legacy_scan_text_height_scale)
            }),
            fixed_reflow_text_height: None,
            ..self
        }
    }

    pub(crate) fn from_view(view: FixedPreferences) -> Self {
        let view = view.normalized();
        Self {
            zoom: view.zoom,
            zoom_factor_hundredths: view.zoom_factor_hundredths,
            zoom_columns_tenths: view.zoom_columns_tenths,
            zoom_rows_tenths: view.zoom_rows_tenths,
            zoom_fit_axis: view.zoom_fit_axis,
            manual_zoom: Some(view.manual_zoom),
            turns: view.turns,
            content_fit: view.content_fit,
            width_fit: view.width_fit,
            height_fit: view.height_fit,
            continuous: view.continuous,
            spread: view.spread,
            scan_contrast: view.scan_contrast,
            scan_gamma_tenths: view.scan_gamma_tenths,
            crop_margin_hundredths: view.crop_margin_hundredths,
            straighten_limit: view.straighten_limit,
            force_ocr: view.force_ocr,
            dither: view.dither,
            continuous_gap: view.continuous_gap,
            scan_white_level: view.scan_white_level,
            dewatermark: view.dewatermark,
            fixed_reflow_spacing: view.fixed_reflow_spacing,
            fixed_reflow_columns: Some(view.fixed_reflow_columns.unwrap_or_default()),
            fixed_reflow_word_gap: Some(view.fixed_reflow_word_gap.unwrap_or_default()),
            fixed_reflow_alignment: Some(view.fixed_reflow_alignment.unwrap_or_default()),
            fixed_reflow_direction: Some(view.fixed_reflow_direction.unwrap_or_default()),
            fixed_text_reflow_direction: Some(
                view.fixed_text_reflow_direction
                    .or_else(|| view.fixed_reflow_direction.map(Into::into))
                    .unwrap_or_default(),
            ),
            fixed_reflow_text_scale: Some(
                view.fixed_reflow_text_scale
                    .unwrap_or(crate::library::DEFAULT_SCAN_REFLOW_TEXT_SCALE),
            ),
            fixed_reflow_text_height: None,
            right_to_left: view.right_to_left,
            zoom_overlap_h: view.zoom_overlap_h(),
            zoom_overlap_v: view.zoom_overlap_v(),
            page_overlap_enabled: view.page_overlap_enabled,
            page_overlap_style: view.page_overlap_style,
            zoom_direction_vertical: view.zoom_direction_vertical(),
            zoom_bottom_to_top: view.zoom_bottom_to_top(),
            epub_spread: view.epub_spread,
            epub_right_to_left: view.epub_right_to_left,
        }
    }
    pub(crate) fn apply(self, geometry: FixedPreferences) -> FixedPreferences {
        FixedPreferences {
            zoom: self.zoom,
            zoom_factor_hundredths: self.zoom_factor_hundredths,
            zoom_columns_tenths: self.zoom_columns_tenths,
            zoom_rows_tenths: self.zoom_rows_tenths,
            zoom_fit_axis: self.zoom_fit_axis,
            // Older candidate defaults had a factor but no explicit target. Treat that saved
            // factor as Manual; current defaults write Some(false) when Manual is inactive.
            manual_zoom: self
                .manual_zoom
                .unwrap_or(self.zoom_factor_hundredths.is_some()),
            precise_scale: None,
            tile_zoom: None,
            turns: self.turns,
            content_fit: self.content_fit,
            width_fit: self.width_fit,
            height_fit: self.height_fit,
            continuous: self.continuous,
            spread: self.spread,
            scan_contrast: self.scan_contrast,
            scan_gamma_tenths: self.scan_gamma_tenths,
            crop_margin_hundredths: self.crop_margin_hundredths,
            straighten_limit: self.straighten_limit,
            force_ocr: self.force_ocr,
            dither: self.dither,
            continuous_gap: self.continuous_gap,
            scan_white_level: self.scan_white_level,
            dewatermark: self.dewatermark,
            fixed_reflow_spacing: self.fixed_reflow_spacing,
            fixed_reflow_columns: self.fixed_reflow_columns,
            fixed_reflow_word_gap: self.fixed_reflow_word_gap,
            fixed_reflow_alignment: self.fixed_reflow_alignment,
            fixed_reflow_direction: self.fixed_reflow_direction,
            fixed_text_reflow_direction: self.fixed_text_reflow_direction,
            fixed_reflow_text_scale: self.migrated().fixed_reflow_text_scale,
            fixed_reflow_text_height: None,
            right_to_left: self.right_to_left,
            zoom_overlap_h: Some(self.zoom_overlap_h),
            zoom_overlap_v: Some(self.zoom_overlap_v),
            page_overlap_enabled: self.page_overlap_enabled,
            page_overlap_style: self.page_overlap_style,
            zoom_direction_vertical: Some(self.zoom_direction_vertical),
            zoom_bottom_to_top: Some(self.zoom_bottom_to_top),
            epub_spread: self.epub_spread,
            epub_right_to_left: self.epub_right_to_left,
            crop: geometry.crop,
            semi_auto_crop: geometry.semi_auto_crop,
            continuous_crop: geometry.continuous_crop,
            offset_x: geometry.offset_x,
            offset_y: geometry.offset_y,
        }
        .normalized()
    }
}

pub(crate) struct Store {
    path: PathBuf,
    pub(crate) templates: Templates,
    pub(crate) problem: Option<String>,
}
impl Store {
    pub(crate) fn load(path: PathBuf) -> Self {
        let result = primitive_app::with_storage(|| {
            let metadata = fs::symlink_metadata(&path)?;
            if !metadata.is_file() || metadata.len() > MAX_BYTES {
                return Err(invalid());
            }
            let mut bytes = Vec::new();
            fs::File::open(&path)?
                .take(MAX_BYTES + 1)
                .read_to_end(&mut bytes)?;
            decode(&bytes)
        });
        match result {
            Ok(templates) => Self { path, templates, problem: None },
            Err(error) if error.kind() == io::ErrorKind::NotFound => Self { path, templates: Templates::default(), problem: None },
            Err(_) => Self { path, templates: Templates::default(), problem: Some("Saved document defaults are unreadable or use another version. The file was preserved.".into()) },
        }
    }
    pub(crate) fn reload(&mut self) {
        *self = Self::load(self.path.clone());
    }
    pub(crate) fn save(&mut self, templates: Templates) -> io::Result<()> {
        if self.problem.is_some() {
            return Err(io::Error::other(
                "The existing document defaults file is read-only",
            ));
        }
        validate_templates(&templates)?;
        let bytes = serde_json::to_vec(&templates).map_err(io::Error::other)?;
        validate_backup(&bytes)?;
        primitive_app::save(&self.path, &bytes)?;
        self.templates = templates;
        Ok(())
    }
}

fn decode(bytes: &[u8]) -> io::Result<Templates> {
    if bytes.len() as u64 > MAX_BYTES {
        return Err(invalid());
    }
    let templates: Templates = serde_json::from_slice(bytes).map_err(io::Error::other)?;
    validate_templates(&templates)?;
    Ok(templates)
}
fn validate_templates(templates: &Templates) -> io::Result<()> {
    if templates.version != 1 {
        return Err(invalid());
    }
    if let Some(prefs) = &templates.reflow {
        validate_preferences(prefs)?;
    }
    if let Some(prefs) = templates.fixed
        && !prefs.apply(FixedPreferences::default()).is_valid()
    {
        return Err(invalid());
    }
    Ok(())
}
/// Shared with the user-data backup importer. It performs no filesystem work.
pub(crate) fn validate_backup(bytes: &[u8]) -> io::Result<()> {
    decode(bytes)?;
    // Local saved files clamp an oversized legacy zoom. An imported backup is
    // rejected instead, so out-of-range values never enter the library silently.
    let value: serde_json::Value = serde_json::from_slice(bytes).map_err(|_| invalid())?;
    use crate::library::{MAX_CONTINUOUS_GAP, MAX_FIXED_ZOOM, MAX_STRAIGHTEN_DEGREES};
    let limits = [
        ("zoom", MAX_FIXED_ZOOM as u64),
        ("turns", 3),
        ("straighten_limit", u64::from(MAX_STRAIGHTEN_DEGREES)),
        ("continuous_gap", u64::from(MAX_CONTINUOUS_GAP)),
    ];
    let fit = |key| {
        value
            .pointer(&format!("/fixed/{key}"))
            .and_then(serde_json::Value::as_bool)
            == Some(true)
    };
    if fit("width_fit") && fit("height_fit") {
        return Err(invalid());
    }
    for (key, max) in limits {
        let value = value
            .pointer(&format!("/fixed/{key}"))
            .and_then(serde_json::Value::as_u64);
        if value.is_some_and(|value| value > max) {
            return Err(invalid());
        }
    }
    Ok(())
}
/// Validate profile/default data independently of any device viewport or font file.
pub(crate) fn validate_preferences(p: &BookPreferences) -> io::Result<()> {
    let margins = [
        p.margin,
        p.margin_sides,
        p.margin_left,
        p.margin_right,
        p.margin_top,
        p.margin_bottom,
    ];
    if p.logical_font_size
        .is_some_and(|n| !crate::logical_font::valid(n))
        || p.render_dpi.is_some_and(|n| !crate::valid_render_dpi(n))
        || p.font_size.is_some_and(|n| !(12.0..=144.0).contains(&n))
        || p.line_height.is_some_and(|n| !(1.0..=3.0).contains(&n))
        || p.line_spacing_percent
            .is_some_and(|n| !crate::valid_line_spacing_percent(n))
        || !(-2.0..=12.0).contains(&p.word_spacing)
        || p.percent_word_spacing.is_some_and(|pair| !pair.valid())
        || !(-1.0..=4.0).contains(&p.letter_spacing)
        || margins.into_iter().flatten().any(|n| n > MAX_SAVED_MARGIN)
        || p.word_expansion.is_some_and(|n| n > 20)
        || p.cjk_width_percent
            .is_some_and(|n| !(100..=150).contains(&n))
        || p.font_gamma_index
            .is_some_and(|n| usize::from(n) >= crate::font_render::GAMMA_LEVELS.len())
        || p.font_weight_delta
            .is_some_and(|n| !(-3.0..=5.5).contains(&n))
        || p.font_file
            .as_deref()
            .is_some_and(|name| !valid_font_name(name))
    {
        return Err(invalid());
    }
    Ok(())
}
fn valid_font_name(name: &str) -> bool {
    crate::reading_fonts::valid_name(name)
}

fn invalid() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "Document defaults contain unsupported or out-of-range settings",
    )
}

#[cfg(test)]
mod tests;
