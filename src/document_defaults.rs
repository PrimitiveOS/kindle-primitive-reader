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
    turns: u8,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    fixed_reflow_spacing: Option<crate::library::FixedReflowSpacing>,
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
            turns: 0,
            width_fit: false,
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
            fixed_reflow_spacing: None,
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
    pub(crate) fn from_view(view: FixedPreferences) -> Self {
        let view = view.normalized();
        Self {
            zoom: view.zoom,
            turns: view.turns,
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
            fixed_reflow_spacing: view.fixed_reflow_spacing,
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
            tile_zoom: None,
            turns: self.turns,
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
            fixed_reflow_spacing: self.fixed_reflow_spacing,
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
    decode(bytes).map(|_| ())
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
    let path = Path::new(name);
    !name.is_empty()
        && name.len() <= 255
        && !name.chars().any(|c| c.is_control() || c == '\\')
        && path.file_name() == Some(std::ffi::OsStr::new(name))
        && path
            .extension()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s.eq_ignore_ascii_case("ttf") || s.eq_ignore_ascii_case("otf"))
}
fn invalid() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "Document defaults contain unsupported or out-of-range settings",
    )
}

#[cfg(test)]
mod tests;
