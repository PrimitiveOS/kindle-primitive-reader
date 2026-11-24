//! Named presets for Reader's status footer, matching KOReader's reader-footer
//! preset workflow without saving unrelated device or typography settings.

use std::fs;
use std::io::{self, Read};
use std::path::Path;

use primitive_app::with_storage;
use serde::{Deserialize, Serialize};

use super::footer_custom_text::Preference as FooterCustomText;
use crate::reading_controls::ReadingControls;

const MAX_BYTES: u64 = 32 * 1024;
const MAX_PRESETS: usize = 16;
pub(crate) const MAX_NAME: usize = 48;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct Preset {
    pub name: String,
    controls: ReadingControls,
    #[serde(default)]
    footer_font_file: Option<String>,
    #[serde(default)]
    footer_custom_text: FooterCustomText,
}

impl Preset {
    pub(crate) fn capture(
        name: String,
        controls: ReadingControls,
        footer_font_file: Option<String>,
        footer_custom_text: FooterCustomText,
    ) -> Self {
        Self {
            name,
            controls: footer_controls(controls),
            footer_font_file,
            footer_custom_text,
        }
    }

    pub(crate) fn apply(
        &self,
        current: ReadingControls,
    ) -> (ReadingControls, Option<String>, FooterCustomText) {
        (
            apply_footer_controls(current, self.controls),
            self.footer_font_file.clone(),
            self.footer_custom_text.clone(),
        )
    }

    pub(crate) fn update(
        &mut self,
        controls: ReadingControls,
        footer_font_file: Option<String>,
        footer_custom_text: FooterCustomText,
    ) {
        self.controls = footer_controls(controls);
        self.footer_font_file = footer_font_file;
        self.footer_custom_text = footer_custom_text;
    }
}

pub(crate) fn load(path: &Path) -> Vec<Preset> {
    with_storage(|| {
        let info = fs::symlink_metadata(path)?;
        if !info.is_file() || info.len() > MAX_BYTES {
            return Err(invalid());
        }
        let mut bytes = Vec::new();
        fs::File::open(path)?
            .take(MAX_BYTES + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err(invalid());
        }
        let presets: Vec<Preset> = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
        if presets.len() > MAX_PRESETS
            || presets.iter().any(|item| !valid_name(&item.name))
            || has_duplicate_names(&presets)
            || presets
                .iter()
                .any(|item| item.footer_custom_text.validate().is_err())
        {
            return Err(invalid());
        }
        Ok(presets)
    })
    .unwrap_or_default()
}

pub(crate) fn insert(
    presets: &mut Vec<Preset>,
    name: &str,
    controls: ReadingControls,
    footer_font_file: Option<String>,
    footer_custom_text: FooterCustomText,
) -> io::Result<()> {
    validate_name(presets, name, None)?;
    if presets.len() == MAX_PRESETS {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Reader reached its status bar preset limit",
        ));
    }
    footer_custom_text.validate()?;
    presets.push(Preset::capture(
        name.trim().to_owned(),
        controls,
        footer_font_file,
        footer_custom_text,
    ));
    Ok(())
}

pub(crate) fn update(
    presets: &mut [Preset],
    index: usize,
    controls: ReadingControls,
    footer_font_file: Option<String>,
    footer_custom_text: FooterCustomText,
) -> io::Result<()> {
    let preset = presets.get_mut(index).ok_or_else(unavailable)?;
    footer_custom_text.validate()?;
    preset.update(controls, footer_font_file, footer_custom_text);
    Ok(())
}

pub(crate) fn rename(presets: &mut [Preset], index: usize, name: &str) -> io::Result<()> {
    validate_name(presets, name, Some(index))?;
    let preset = presets.get_mut(index).ok_or_else(unavailable)?;
    preset.name = name.trim().to_owned();
    Ok(())
}

pub(crate) fn save(path: &Path, presets: &[Preset]) -> io::Result<()> {
    if presets.len() > MAX_PRESETS
        || presets.iter().any(|item| !valid_name(&item.name))
        || has_duplicate_names(presets)
        || presets
            .iter()
            .any(|item| item.footer_custom_text.validate().is_err())
    {
        return Err(invalid());
    }
    let bytes = serde_json::to_vec(presets).map_err(io::Error::other)?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(invalid());
    }
    primitive_app::save(path, &bytes)
}

fn footer_controls(controls: ReadingControls) -> ReadingControls {
    ReadingControls {
        status_bar: controls.status_bar,
        footer_items: controls.footer_items,
        footer_order: controls.footer_order,
        footer_item_cycle: controls.footer_item_cycle,
        footer_separator: controls.footer_separator,
        footer_horizontal_separator: controls.footer_horizontal_separator,
        footer_symbol_style: controls.footer_symbol_style,
        footer_alignment: controls.footer_alignment,
        footer_pages_left_include_current: controls.footer_pages_left_include_current,
        footer_percentage_digits: controls.footer_percentage_digits,
        footer_track_position: controls.footer_track_position,
        footer_track_thickness: controls.footer_track_thickness,
        footer_track_min_width_pct: controls.footer_track_min_width_pct,
        footer_track_lock_min_width: controls.footer_track_lock_min_width,
        footer_track_margin_width: controls.footer_track_margin_width,
        footer_initial_marker: controls.footer_initial_marker,
        footer_auto_refresh: controls.footer_auto_refresh,
        footer_battery_hide_threshold: controls.footer_battery_hide_threshold,
        footer_item_font_size: controls.footer_item_font_size,
        footer_item_bold: controls.footer_item_bold,
        footer_container_height: controls.footer_container_height,
        footer_container_bottom_margin: controls.footer_container_bottom_margin,
        top_header: crate::reading_controls::TopHeader {
            legacy_enabled: false,
            book_default: None,
            ..controls.top_header
        },
        chapter_marker_width: controls.chapter_marker_width,
        overlap_footer: controls.overlap_footer,
        footer_scope: controls.footer_scope,
        hide_chapter_markers: controls.hide_chapter_markers,
        footer_all_at_once: controls.footer_all_at_once,
        footer_combined_time: controls.footer_combined_time,
        footer_tap_locked: controls.footer_tap_locked,
        skim_on_footer_hold: controls.skim_on_footer_hold,
        ..ReadingControls::default()
    }
}

fn apply_footer_controls(current: ReadingControls, preset: ReadingControls) -> ReadingControls {
    ReadingControls {
        status_bar: preset.status_bar,
        footer_items: preset.footer_items,
        footer_order: preset.footer_order,
        footer_item_cycle: preset.footer_item_cycle,
        footer_separator: preset.footer_separator,
        footer_horizontal_separator: preset.footer_horizontal_separator,
        footer_symbol_style: preset.footer_symbol_style,
        footer_alignment: preset.footer_alignment,
        footer_pages_left_include_current: preset.footer_pages_left_include_current,
        footer_percentage_digits: preset.footer_percentage_digits,
        footer_track_position: preset.footer_track_position,
        footer_track_thickness: preset.footer_track_thickness,
        footer_track_min_width_pct: preset.footer_track_min_width_pct,
        footer_track_lock_min_width: preset.footer_track_lock_min_width,
        footer_track_margin_width: preset.footer_track_margin_width,
        footer_initial_marker: preset.footer_initial_marker,
        footer_auto_refresh: preset.footer_auto_refresh,
        footer_battery_hide_threshold: preset.footer_battery_hide_threshold,
        footer_item_font_size: preset.footer_item_font_size,
        footer_item_bold: preset.footer_item_bold,
        footer_container_height: preset.footer_container_height,
        footer_container_bottom_margin: preset.footer_container_bottom_margin,
        top_header: crate::reading_controls::TopHeader {
            legacy_enabled: current.top_header.legacy_enabled,
            book_default: current.top_header.book_default,
            ..preset.top_header
        },
        chapter_marker_width: preset.chapter_marker_width,
        overlap_footer: preset.overlap_footer,
        footer_scope: preset.footer_scope,
        hide_chapter_markers: preset.hide_chapter_markers,
        footer_all_at_once: preset.footer_all_at_once,
        footer_combined_time: preset.footer_combined_time,
        footer_tap_locked: preset.footer_tap_locked,
        skim_on_footer_hold: preset.skim_on_footer_hold,
        ..current
    }
}

fn validate_name(presets: &[Preset], name: &str, except: Option<usize>) -> io::Result<()> {
    if !valid_name(name) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Enter a status bar preset name up to 48 characters",
        ));
    }
    let folded = name.trim().to_lowercase();
    if presets
        .iter()
        .enumerate()
        .any(|(index, preset)| Some(index) != except && preset.name.to_lowercase() == folded)
    {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "A status bar preset already uses that name",
        ));
    }
    Ok(())
}

fn valid_name(name: &str) -> bool {
    !name.trim().is_empty()
        && name == name.trim()
        && name.chars().count() <= MAX_NAME
        && !name.chars().any(char::is_control)
}

fn has_duplicate_names(presets: &[Preset]) -> bool {
    let mut names = std::collections::BTreeSet::new();
    presets
        .iter()
        .any(|preset| !names.insert(preset.name.to_lowercase()))
}

fn unavailable() -> io::Error {
    io::Error::new(
        io::ErrorKind::NotFound,
        "That status bar preset is unavailable",
    )
}

fn invalid() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "Status bar presets exceed Reader's supported format or limits",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reading_controls::{FooterAlignment, FooterSeparator, FooterTrackPosition};

    #[test]
    fn presets_round_trip_without_overwriting_unrelated_controls() {
        let path = std::env::temp_dir().join(format!(
            "primitive-reader-footer-presets-{}-{}.json",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        let source = ReadingControls {
            footer_alignment: FooterAlignment::Right,
            footer_separator: FooterSeparator::Hyphen,
            footer_horizontal_separator: true,
            footer_track_position: FooterTrackPosition::Above,
            footer_item_bold: true,
            footer_container_height: Some(36),
            footer_container_bottom_margin: Some(8),
            invert_page_turn_buttons: true,
            night_pages: true,
            top_header: crate::reading_controls::TopHeader {
                book_default: Some(false),
                clock: false,
                ..Default::default()
            },
            ..ReadingControls::default()
        };
        let mut presets = Vec::new();
        insert(
            &mut presets,
            "Compact",
            source,
            Some("Silkscreen-Regular.ttf".into()),
            FooterCustomText::default(),
        )
        .unwrap();
        save(&path, &presets).unwrap();
        let reopened = load(&path);
        assert_eq!(reopened.len(), 1);

        let current = ReadingControls {
            footer_alignment: FooterAlignment::Left,
            footer_horizontal_separator: false,
            invert_page_turn_buttons: false,
            night_pages: false,
            top_header: crate::reading_controls::TopHeader {
                legacy_enabled: true,
                book_default: Some(true),
                ..Default::default()
            },
            ..ReadingControls::default()
        };
        let (applied, font_file, custom_text) = reopened[0].apply(current);
        assert_eq!(font_file.as_deref(), Some("Silkscreen-Regular.ttf"));
        assert_eq!(custom_text, FooterCustomText::default());
        assert_eq!(applied.footer_alignment, FooterAlignment::Right);
        assert_eq!(applied.footer_separator, FooterSeparator::Hyphen);
        assert!(applied.footer_horizontal_separator);
        assert_eq!(applied.footer_track_position, FooterTrackPosition::Above);
        assert!(applied.footer_item_bold);
        assert_eq!(applied.footer_container_height, Some(36));
        assert_eq!(applied.footer_container_bottom_margin, Some(8));
        assert!(!applied.invert_page_turn_buttons);
        assert!(!applied.night_pages);
        assert!(!applied.top_header.clock);
        assert!(applied.top_header.legacy_enabled);
        assert_eq!(applied.top_header.book_default, Some(true));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn names_support_rename_update_and_reject_duplicates() {
        let mut presets = Vec::new();
        insert(
            &mut presets,
            "Default",
            ReadingControls::default(),
            None,
            FooterCustomText::default(),
        )
        .unwrap();
        assert_eq!(
            insert(
                &mut presets,
                "default",
                ReadingControls::default(),
                None,
                FooterCustomText::default(),
            )
            .unwrap_err()
            .kind(),
            io::ErrorKind::AlreadyExists
        );
        assert!(
            insert(
                &mut presets,
                "Default ",
                ReadingControls::default(),
                None,
                FooterCustomText::default(),
            )
            .is_err()
        );
        rename(&mut presets, 0, "Evening").unwrap();
        update(
            &mut presets,
            0,
            ReadingControls {
                footer_horizontal_separator: true,
                ..ReadingControls::default()
            },
            Some("Serif-Regular.ttf".into()),
            FooterCustomText::default(),
        )
        .unwrap();
        assert_eq!(presets[0].name, "Evening");
        let (applied, font_file, custom_text) = presets[0].apply(ReadingControls::default());
        assert!(applied.footer_horizontal_separator);
        assert_eq!(font_file.as_deref(), Some("Serif-Regular.ttf"));
        assert_eq!(custom_text, FooterCustomText::default());
    }
}
