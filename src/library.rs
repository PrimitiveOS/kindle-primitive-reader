//! Local document discovery and reading state. Paths stored in state are relative to the library.

pub(crate) mod custom_contents;
pub(crate) mod hidden_flows;

use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{self, BufReader, Read, Seek, SeekFrom};
use std::path::{Component, Path, PathBuf};

use icu_casemap::CaseMapper;
use icu_normalizer::{ComposingNormalizer, DecomposingNormalizer};
use serde::{Deserialize, Serialize};
use zip::ZipArchive;

use crate::comic::{
    MAX_COMIC_BYTES, MAX_IMAGE_BYTES, MAX_WEBP_BYTES, natural_cmp,
};
use crate::djvu::MAX_DJVU_BYTES;
use crate::epub::{Book, MAX_ARCHIVE, check_zip_directory};
use crate::fixed::MAX_PDF_BYTES;
use crate::fixed_text::PdfTextRect;
use crate::loose::{Format, MAX_FB2_INPUT, MAX_HTMLZ_INPUT, MAX_INPUT};
use crate::mobi::MAX_MOBI_BYTES;
use crate::selection::TextSelection;
use crate::{Anchor, Settings, TextAlignment};

pub(crate) const MAX_PATH_DEPTH: usize = 9;
const MAX_SCAN_ENTRIES: usize = 20_000;
const MAX_BOOKS: usize = 4_096;
const MAX_BROWSE_ENTRIES: usize = 4_096;
const MAX_RELOCATION_CHECKS: usize = 64;
const MAX_RELOCATION_READ_BYTES: u64 = 64 * 1024 * 1024;
const MAX_STATE_BYTES: u64 = 4 * 1024 * 1024;
pub const MAX_BACKUP_BYTES: u64 = MAX_STATE_BYTES + 1024;
pub const MAX_PDF_HIGHLIGHTS: usize = 256;
pub(crate) const MAX_PDF_HIGHLIGHT_BOUNDS: usize = 64;
pub(crate) const MAX_HIGHLIGHT_TEXT_BYTES: usize = 8192;
pub(crate) const MAX_NOTE_BYTES: usize = 4096;
const MAX_FIXED_ZOOM: usize = 4;
const MAX_FIXED_OFFSET: f32 = 32_768.0;
pub const MAX_CONTINUOUS_GAP: u16 = 128;
pub const MIN_SCAN_WHITE_LEVEL: u8 = 192;
pub const MAX_STRAIGHTEN_DEGREES: u8 = 25;
pub const MAX_SCAN_WHITE_LEVEL: u8 = 254;
const MIN_CROP_SIZE: f32 = 0.025;
const BACKUP_VERSION: u32 = 1;
const MAX_BACKUP_BOOKS: usize = 16_384;
const MAX_COLLECTIONS: usize = 128;
const MAX_COLLECTION_MEMBERS: usize = 4096;
const MAX_COLLECTION_NAME_CHARS: usize = 64;
const MAX_IMPORTED_MARKS: usize = 512;
const MAX_ANCHOR_PATH_BYTES: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocumentFormat {
    Epub,
    Text,
    Html,
    Htmlz,
    Docx,
    Chm,
    Markdown,
    Fb2,
    Mobi,
    Rtf,
    Tcr,
    Pdf,
    Djvu,
    Cbz,
    Cbt,
    Cbr,
    Jpeg,
    Jpeg2000,
    Png,
    Gif,
    Bmp,
    Pnm,
    Svg,
    WebP,
    Tiff,
}

impl DocumentFormat {
    pub fn for_path(path: &Path) -> Option<Self> {
        if let Some(format) = Format::for_path(path) {
            return Some(match format {
                Format::Text => Self::Text,
                Format::Html => Self::Html,
                Format::Htmlz => Self::Htmlz,
                Format::Docx => Self::Docx,
                Format::Chm => Self::Chm,
                Format::Markdown => Self::Markdown,
                Format::Fb2 => Self::Fb2,
                Format::Mobi => Self::Mobi,
                Format::Rtf => Self::Rtf,
                Format::Tcr => Self::Tcr,
            });
        }
        if crate::epub_members::is_folder_book(path) {
            return Some(Self::Epub);
        }
        let extension = path.extension()?.to_str()?;
        match extension.to_ascii_lowercase().as_str() {
            "epub" => Some(Self::Epub),
            "pdf" => Some(Self::Pdf),
            "djvu" | "djv" => Some(Self::Djvu),
            "cbz" => Some(Self::Cbz),
            "cbt" => Some(Self::Cbt),
            "cbr" => Some(Self::Cbr),
            "svg" => Some(Self::Svg),
            _ => Self::raster_for_extension(extension),
        }
    }

    pub(crate) fn raster_for_extension(extension: &str) -> Option<Self> {
        match extension.to_ascii_lowercase().as_str() {
            "jpg" | "jpeg" => Some(Self::Jpeg),
            "jp2" | "j2k" => Some(Self::Jpeg2000),
            "png" => Some(Self::Png),
            "gif" => Some(Self::Gif),
            "bmp" => Some(Self::Bmp),
            "pnm" | "pbm" | "pgm" | "ppm" | "pam" => Some(Self::Pnm),
            "webp" => Some(Self::WebP),
            "tif" | "tiff" => Some(Self::Tiff),
            _ => None,
        }
    }

    pub const fn is_still_image(self) -> bool {
        matches!(
            self,
            Self::Jpeg
                | Self::Jpeg2000
                | Self::Png
                | Self::Gif
                | Self::Bmp
                | Self::Pnm
                | Self::Svg
                | Self::WebP
                | Self::Tiff
        )
    }

    pub const fn is_comic(self) -> bool {
        matches!(self, Self::Cbz | Self::Cbt | Self::Cbr)
    }

    pub const fn max_bytes(self) -> u64 {
        match self {
            Self::Epub => MAX_ARCHIVE,
            Self::Text | Self::Html | Self::Markdown => MAX_INPUT as u64,
            Self::Htmlz => MAX_HTMLZ_INPUT as u64,
            Self::Docx => crate::docx::MAX_INPUT as u64,
            Self::Chm => crate::chm::MAX_CHM_BYTES,
            Self::Fb2 => MAX_FB2_INPUT as u64,
            Self::Mobi => MAX_MOBI_BYTES as u64,
            Self::Rtf => crate::rtf::MAX_INPUT as u64,
            Self::Tcr => crate::tcr::MAX_INPUT as u64,
            Self::Pdf => MAX_PDF_BYTES as u64,
            Self::Djvu => MAX_DJVU_BYTES as u64,
            Self::Cbz | Self::Cbt => MAX_COMIC_BYTES,
            Self::Cbr => crate::cbr::MAX_CBR_BYTES,
            Self::Jpeg | Self::Png | Self::Gif | Self::Bmp | Self::Pnm | Self::Tiff => {
                MAX_IMAGE_BYTES
            }
            Self::Svg => crate::svg_image::MAX_SVG_BYTES as u64,
            Self::WebP => MAX_WEBP_BYTES,
            Self::Jpeg2000 => crate::jpeg2000::MAX_INPUT,
        }
    }

    pub(crate) fn tag(self) -> &'static [u8] {
        match self {
            Self::Epub => b"",
            Self::Text => b"reader-txt\0",
            Self::Html => b"reader-html\0",
            Self::Htmlz => b"reader-htmlz\0",
            Self::Docx => b"reader-docx\0",
            Self::Chm => b"reader-chm\0",
            Self::Markdown => b"reader-markdown\0",
            Self::Fb2 => b"reader-fb2\0",
            Self::Mobi => b"reader-mobi\0",
            Self::Rtf => b"reader-rtf\0",
            Self::Tcr => b"reader-tcr\0",
            Self::Pdf => b"reader-pdf\0",
            Self::Djvu => b"reader-djvu\0",
            Self::Cbz => b"reader-cbz\0",
            Self::Cbt => b"reader-cbt\0",
            Self::Cbr => b"reader-cbr\0",
            Self::Jpeg2000 => b"reader-jpeg2000\0",
            Self::Jpeg => b"reader-jpeg\0",
            Self::Png => b"reader-png\0",
            Self::Gif => b"reader-gif\0",
            Self::Bmp => b"reader-bmp\0",
            Self::Pnm => b"reader-pnm\0",
            Self::Svg => b"reader-svg\0",
            Self::WebP => b"reader-webp\0",
            Self::Tiff => b"reader-tiff\0",
        }
    }
}

#[derive(Clone, Debug)]
pub struct LibraryBook {
    pub path: PathBuf,
    pub title: String,
    pub author: Option<String>,
    pub size: u64,
    pub format: DocumentFormat,
    searchable: String,
}

impl LibraryBook {
    pub(crate) fn library_path(&self) -> &Path {
        if crate::epub_members::is_folder_book(&self.path) {
            self.path.parent().expect("a folder EPUB has a marker path")
        } else {
            &self.path
        }
    }

    pub(crate) fn can_delete_file(&self) -> bool {
        self.library_path() == self.path
    }
}

#[derive(Clone, Debug)]
pub struct BrowseList {
    pub entries: Vec<BrowseEntry>,
    pub truncated: bool,
}

#[derive(Clone, Debug)]
pub struct BrowseEntry {
    pub path: PathBuf,
    pub name: String,
    pub kind: BrowseKind,
}

#[derive(Clone, Debug)]
pub enum BrowseKind {
    Folder,
    Readable(LibraryBook),
    Unavailable { size: u64, too_large: bool },
    KindleProtected { size: u64 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BookMetadata {
    pub title: String,
    pub author: Option<String>,
    pub series: Vec<crate::epub::BookSeries>,
    pub has_cover: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BookPreferences {
    pub font_size: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub render_dpi: Option<u16>,
    pub margin: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub margin_sides: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub margin_left: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub margin_right: Option<u32>,
    #[serde(skip_serializing_if = "is_false")]
    pub link_vertical_margins: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub margin_top: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub margin_bottom: Option<u32>,
    pub line_height: Option<f32>,
    pub word_spacing: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub word_expansion: Option<u8>,
    pub letter_spacing: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cjk_width_percent: Option<u8>,
    pub scroll: bool,
    pub facing_pages: bool,
    pub alignment: TextAlignment,
    pub bold_text: bool,
    pub text_contrast: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_gamma_index: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_weight_delta: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publisher_styles: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_kerning: Option<bool>,
    pub font_hinting: crate::FontHinting,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub smooth_images: Option<bool>,
    pub invert_night_images: bool,
    pub override_fonts: bool,
    pub disable_soft_hyphens: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub automatic_hyphenation: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_file: Option<String>,
}

impl BookPreferences {
    pub fn apply_to(&self, settings: Settings) -> Settings {
        let permitted_margin = |margin: &u32| *margin <= settings.width.min(settings.height) / 4;
        let edge = |specific: Option<u32>, inherited: Option<u32>| {
            specific.filter(permitted_margin).or_else(|| {
                if self.margin_sides.is_some_and(|n| permitted_margin(&n))
                    || self.margin.is_some_and(|n| permitted_margin(&n))
                {
                    None
                } else {
                    inherited
                }
            })
        };
        let adjusted = Settings {
            font_size: self.font_size.unwrap_or(settings.font_size),
            render_dpi: self.render_dpi
                .filter(|dpi| crate::RENDER_DPI_PRESETS.contains(dpi))
                .unwrap_or(settings.render_dpi),
            margin: self.margin.unwrap_or(settings.margin),
            margin_sides: self
                .margin_sides
                .filter(permitted_margin)
                .or(settings.margin_sides),
            margin_left: edge(self.margin_left, settings.margin_left),
            margin_right: edge(self.margin_right, settings.margin_right),
            link_vertical_margins: self.link_vertical_margins,
            margin_top: self
                .margin_top
                .filter(|&v| v <= settings.height / 4)
                .or(settings.margin_top),
            margin_bottom: self
                .margin_bottom
                .filter(|&v| v <= settings.height / 4)
                .or(settings.margin_bottom),
            line_height: self.line_height.unwrap_or(settings.line_height),
            word_spacing: if (-2.0..=12.0).contains(&self.word_spacing) {
                self.word_spacing
            } else {
                settings.word_spacing
            },
            word_expansion: self
                .word_expansion
                .filter(|&percent| percent <= 20)
                .unwrap_or(settings.word_expansion),
            cjk_width_percent: self
                .cjk_width_percent
                .filter(|n| (100..=150).contains(n))
                .unwrap_or(settings.cjk_width_percent),
            letter_spacing: if (-1.0..=4.0).contains(&self.letter_spacing) {
                self.letter_spacing
            } else {
                settings.letter_spacing
            },
            alignment: self.alignment,
            bold_text: self.bold_text,
            text_contrast: self.text_contrast,
            font_gamma_index: self.font_gamma_index
                .filter(|&index| usize::from(index) < crate::font_render::GAMMA_LEVELS.len())
                .unwrap_or(settings.font_gamma_index),
            font_weight_delta: self.font_weight_delta
                .filter(|delta| (-3.0..=5.5).contains(delta))
                .unwrap_or(settings.font_weight_delta),
            publisher_styles: self.publisher_styles.unwrap_or(settings.publisher_styles),
            font_kerning: self.font_kerning.unwrap_or(settings.font_kerning),
            font_hinting: self.font_hinting,
            smooth_images: self.smooth_images.unwrap_or(settings.smooth_images),
            invert_night_images: self.invert_night_images,
            override_fonts: self.override_fonts,
            disable_soft_hyphens: self.disable_soft_hyphens,
            automatic_hyphenation: self
                .automatic_hyphenation
                .unwrap_or(settings.automatic_hyphenation),
            ..settings
        };
        if adjusted.validate().is_ok() {
            adjusted
        } else {
            settings
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct FixedPreferences {
    pub zoom: usize,
    pub turns: u8,
    pub width_fit: bool,
    pub height_fit: bool,
    pub continuous: bool,
    pub spread: bool,
    pub scan_contrast: bool,
    pub straighten_limit: u8,
    pub force_ocr: bool,
    pub dither: bool,
    pub continuous_gap: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scan_white_level: Option<u8>,
    pub offset_x: f32,
    pub offset_y: f32,
    pub right_to_left: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub epub_spread: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub epub_right_to_left: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crop: Option<PdfTextRect>,
}

impl FixedPreferences {
    pub fn is_valid(self) -> bool {
        self == self.normalized()
    }

    pub fn normalized(self) -> Self {
        Self {
            zoom: self.zoom.min(MAX_FIXED_ZOOM),
            turns: self.turns % 4,
            height_fit: self.height_fit && !self.width_fit,
            continuous_gap: self.continuous_gap.min(MAX_CONTINUOUS_GAP),
            straighten_limit: self.straighten_limit.min(MAX_STRAIGHTEN_DEGREES),
            scan_white_level: self.scan_white_level.filter(valid_white_level),
            offset_x: normalized_offset(f64::from(self.offset_x)),
            offset_y: normalized_offset(f64::from(self.offset_y)),
            crop: self.crop.filter(valid_crop),
            ..self
        }
    }
}

impl<'de> Deserialize<'de> for FixedPreferences {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // One unusable view field should leave the book's position and annotations readable.
        let values = serde_json::Value::deserialize(deserializer)?;
        let integer = |key| {
            values
                .get(key)
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0)
        };
        let optional_switch = |key| values.get(key).and_then(serde_json::Value::as_bool);
        let switch = |key| optional_switch(key).unwrap_or(false);
        let offset = |key| {
            normalized_offset(
                values
                    .get(key)
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(0.0),
            )
        };
        let width_fit = switch("width_fit");
        Ok(Self {
            zoom: integer("zoom").min(MAX_FIXED_ZOOM as u64) as usize,
            turns: (integer("turns") % 4) as u8,
            width_fit,
            height_fit: !width_fit && switch("height_fit"),
            continuous: switch("continuous"),
            spread: switch("spread"),
            scan_contrast: switch("scan_contrast"),
            force_ocr: switch("force_ocr"),
            straighten_limit: integer("straighten_limit").min(u64::from(MAX_STRAIGHTEN_DEGREES))
                as u8,
            dither: switch("dither"),
            continuous_gap: integer("continuous_gap").min(u64::from(MAX_CONTINUOUS_GAP)) as u16,
            scan_white_level: u8::try_from(integer("scan_white_level"))
                .ok()
                .filter(valid_white_level),
            offset_x: offset("offset_x"),
            offset_y: offset("offset_y"),
            right_to_left: switch("right_to_left"),
            epub_spread: optional_switch("epub_spread"),
            epub_right_to_left: optional_switch("epub_right_to_left"),
            crop: values
                .get("crop")
                .and_then(|value| serde_json::from_value(value.clone()).ok())
                .filter(valid_crop),
        })
    }
}

fn valid_white_level(level: &u8) -> bool {
    (MIN_SCAN_WHITE_LEVEL..=MAX_SCAN_WHITE_LEVEL).contains(level)
}

fn normalized_offset(value: f64) -> f32 {
    let limit = f64::from(MAX_FIXED_OFFSET);
    if value.is_finite() {
        value.clamp(-limit, limit) as f32
    } else {
        0.0
    }
}

fn valid_crop(rect: &PdfTextRect) -> bool {
    valid_page_rect(rect)
        && rect.x1 - rect.x0 >= MIN_CROP_SIZE
        && rect.y1 - rect.y0 >= MIN_CROP_SIZE
}

fn valid_page_rect(rect: &PdfTextRect) -> bool {
    rect.x0 >= 0.0
        && rect.x0 < rect.x1
        && rect.x1 <= 1.0
        && rect.y0 >= 0.0
        && rect.y0 < rect.y1
        && rect.y1 <= 1.0
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadingPosition {
    pub chapter: usize,
    #[serde(default)]
    pub page: usize,
    pub anchor: Option<Anchor>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bookmark {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<i64>,
    pub label: String,
    pub position: ReadingPosition,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Highlight {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<i64>,
    #[serde(
        default,
        skip_serializing_if = "crate::highlight_style::HighlightStyle::is_default"
    )]
    pub style: crate::highlight_style::HighlightStyle,
    #[serde(default, skip_serializing_if = "crate::highlight_style::HighlightColor::is_default")]
    pub color: crate::highlight_style::HighlightColor,
    pub selection: TextSelection,
    pub note: Option<String>,
}

/// Further pages of one saved selection, in reading order.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HighlightPage {
    pub page: usize,
    pub bounds: Vec<PdfTextRect>,
    /// Start of this page in the shared quote, after the separating newline.
    pub text_offset: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PdfHighlight {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<i64>,
    #[serde(
        default,
        skip_serializing_if = "crate::highlight_style::HighlightStyle::is_default"
    )]
    pub style: crate::highlight_style::HighlightStyle,
    #[serde(default, skip_serializing_if = "crate::highlight_style::HighlightColor::is_default")]
    pub color: crate::highlight_style::HighlightColor,
    pub page: usize,
    pub bounds: Vec<PdfTextRect>,
    pub text: String,
    pub note: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub continuation: Vec<HighlightPage>,
}

impl PdfHighlight {
    pub(crate) const MAX_PAGES: usize = 64;

    pub fn is_valid(&self) -> bool {
        let valid_bounds = |bounds: &[PdfTextRect]| {
            !bounds.is_empty()
                && bounds.len() <= MAX_PDF_HIGHLIGHT_BOUNDS
                && bounds.iter().all(valid_page_rect)
        };
        if !valid_bounds(&self.bounds)
            || self.text.trim().is_empty()
            || self.text.len() > MAX_HIGHLIGHT_TEXT_BYTES
            || self
                .note
                .as_ref()
                .is_some_and(|note| note.len() > MAX_NOTE_BYTES)
            || self.continuation.len() >= Self::MAX_PAGES
        {
            return false;
        }
        let mut previous = (self.page, 0);
        for part in &self.continuation {
            if previous.0.checked_add(1) != Some(part.page)
                || part.text_offset <= previous.1 + 1
                || part.text_offset >= self.text.len()
                || !self.text.is_char_boundary(part.text_offset)
                || self.text.as_bytes()[part.text_offset - 1] != b'\n'
                || !valid_bounds(&part.bounds)
            {
                return false;
            }
            previous = (part.page, part.text_offset);
        }
        true
    }

    /// One slice per page, for highlighting and PDF annotation interchange.
    pub(crate) fn parts(&self) -> impl Iterator<Item = (usize, &[PdfTextRect], &str)> {
        std::iter::once((self.page, self.bounds.as_slice(), 0))
            .chain(
                self.continuation
                    .iter()
                    .map(|part| (part.page, part.bounds.as_slice(), part.text_offset)),
            )
            .enumerate()
            .filter_map(|(index, (page, bounds, start))| {
                let end = self
                    .continuation
                    .get(index)
                    .map_or(self.text.len(), |next| next.text_offset.saturating_sub(1));
                Some((page, bounds, self.text.get(start..end)?))
            })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadingStatus {
    #[default]
    Unread,
    Reading,
    OnHold,
    Finished,
}

impl ReadingStatus {
    pub const ALL: [Self; 4] = [Self::Unread, Self::Reading, Self::OnHold, Self::Finished];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Unread => "Unread",
            Self::Reading => "Reading",
            Self::OnHold => "On hold",
            Self::Finished => "Finished",
        }
    }
}

fn read_status<'de, D: serde::Deserializer<'de>>(
    input: D,
) -> Result<Option<ReadingStatus>, D::Error> {
    let value = Option::<serde_json::Value>::deserialize(input)?;
    Ok(value.and_then(|value| serde_json::from_value(value).ok()))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BookState {
    #[serde(default, skip_serializing_if = "hidden_flows::HiddenFlows::is_default")]
    pub(crate) hidden_flows: hidden_flows::HiddenFlows,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hide_nonlinear: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub highlight_style: Option<crate::highlight_style::HighlightStyle>,
    #[serde(default = "legacy_highlight_color", skip_serializing_if = "is_implicit_gray")]
    pub highlight_color: Option<crate::highlight_style::HighlightColor>,
    pub id: u64,
    pub path: PathBuf,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(default)]
    size: u64,
    pub position: Option<ReadingPosition>,
    #[serde(default, skip_serializing_if = "is_false")]
    file_deleted: bool,
    #[serde(
        default,
        deserialize_with = "read_status",
        skip_serializing_if = "Option::is_none"
    )]
    reading_status: Option<ReadingStatus>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub favorite: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub(crate) publisher_page_footer: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub(crate) publisher_page_margins: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) character_page_stride: Option<u16>,
    #[serde(default)]
    pub preferences: BookPreferences,
    #[serde(default)]
    pub fixed_preferences: FixedPreferences,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) preferred_dictionaries: Vec<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) dictionary_word_forms: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "read_ocr_language"
    )]
    pub(crate) ocr_language: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "crate::book_menus::BookMenus::is_empty"
    )]
    pub(crate) menus: crate::book_menus::BookMenus,
    #[serde(default)]
    pub bookmarks: Vec<Bookmark>,
    #[serde(
        default,
        skip_serializing_if = "custom_contents::CustomContents::is_default"
    )]
    pub(crate) custom_contents: custom_contents::CustomContents,
    #[serde(default)]
    pub highlights: Vec<Highlight>,
    #[serde(default)]
    pub pdf_highlights: Vec<PdfHighlight>,
}

fn read_ocr_language<'de, D: serde::Deserializer<'de>>(
    reader: D,
) -> Result<Option<String>, D::Error> {
    let value = serde_json::Value::deserialize(reader)?;
    Ok(value
        .as_str()
        .filter(|code| crate::ocr::valid_language_name(code))
        .map(str::to_owned))
}

impl BookState {
    fn new(entry: &LibraryBook, id: u64) -> Self {
        Self {
            highlight_style: None,
            hide_nonlinear: None,
            hidden_flows: Default::default(),
            highlight_color: None,
            id,
            path: entry.path.clone(),
            title: entry.title.clone(),
            author: entry.author.clone(),
            size: entry.size,
            position: None,
            file_deleted: false,
            reading_status: None,
            favorite: false,
            publisher_page_footer: false,
            publisher_page_margins: false,
            character_page_stride: None,
            preferences: BookPreferences::default(),
            fixed_preferences: FixedPreferences::default(),
            preferred_dictionaries: Vec::new(),
            dictionary_word_forms: None,
            ocr_language: None,
            menus: Default::default(),
            bookmarks: Vec::new(),
            custom_contents: Default::default(),
            highlights: Vec::new(),
            pdf_highlights: Vec::new(),
        }
    }

    pub fn status(&self) -> ReadingStatus {
        self.reading_status.unwrap_or_else(|| {
            if self.position.is_some() {
                ReadingStatus::Reading
            } else {
                ReadingStatus::Unread
            }
        })
    }

    pub fn set_status(&mut self, status: ReadingStatus) {
        self.reading_status = Some(status);
    }

    pub fn add_pdf_highlight(&mut self, highlight: PdfHighlight) -> io::Result<()> {
        if self.pdf_highlights.len() >= MAX_PDF_HIGHLIGHTS {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "This PDF reached its highlight limit",
            ));
        }
        if !highlight.is_valid() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "The PDF highlight is empty or too large",
            ));
        }
        self.pdf_highlights.push(highlight);
        Ok(())
    }
}

fn is_implicit_gray(color: &Option<crate::highlight_style::HighlightColor>) -> bool {
    color.is_some_and(|color| color == crate::highlight_style::HighlightColor::Gray)
}

fn legacy_highlight_color() -> Option<crate::highlight_style::HighlightColor> {
    Some(crate::highlight_style::HighlightColor::Gray)
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Collection {
    pub name: String,
    #[serde(default)]
    book_ids: Vec<u64>,
}

impl Collection {
    pub fn book_ids(&self) -> &[u64] {
        &self.book_ids
    }

    pub fn contains(&self, id: u64) -> bool {
        self.book_ids.contains(&id)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BackupSummary {
    pub new_books: usize,
    pub existing_books: usize,
    pub bookmarks_added: usize,
    pub custom_chapters_added: usize,
    pub custom_flow_points_added: usize,
    pub highlights_added: usize,
    pub pdf_highlights_added: usize,
    pub notes_added: usize,
    pub timestamps_added: usize,
    pub duplicates: usize,
    pub skipped: usize,
    pub note_conflicts: usize,
    pub collections_added: usize,
    pub memberships_added: usize,
}

pub struct BackupImport {
    books: Vec<BookState>,
    collections: Vec<Collection>,
    summary: BackupSummary,
}

impl BackupImport {
    pub fn summary(&self) -> BackupSummary {
        self.summary
    }
}

#[derive(Deserialize)]
struct SavedLibrary {
    version: u32,
    books: Vec<BookState>,
    #[serde(default)]
    collections: Vec<Collection>,
}

#[derive(Serialize)]
struct SavedLibraryRef<'a> {
    version: u32,
    books: &'a [BookState],
    collections: &'a [Collection],
}

pub struct Library {
    root: PathBuf,
    original_root: PathBuf,
    state_path: PathBuf,
    books: Vec<LibraryBook>,
    recent: Vec<BookState>,
    collections: Vec<Collection>,
    truncated: bool,
}

impl Library {
    pub fn open(root: &Path, state_path: &Path) -> io::Result<Self> {
        let original_root = if root.is_absolute() {
            root.to_owned()
        } else {
            std::env::current_dir()?.join(root)
        };
        if !fs::symlink_metadata(&original_root)?.is_dir() {
            return Err(denied());
        }
        let (recent, collections) = load_library_state(state_path)?;
        Ok(Self {
            root: original_root.canonicalize()?,
            original_root,
            state_path: state_path.to_owned(),
            recent,
            collections,
            books: Vec::new(),
            truncated: false,
        })
    }

    pub fn scan(&mut self) -> io::Result<()> {
        if !fs::symlink_metadata(&self.root)?.is_dir() {
            return Err(denied());
        }
        let root_entries = fs::read_dir(&self.root)?;
        let mut dirs = vec![(PathBuf::new(), root_entries)];
        let mut seen = 0;
        let mut books = Vec::new();
        let mut truncated = false;
        let known_metadata = self.known_metadata();
        while let Some((relative, entries)) = dirs.last_mut() {
            let Some(entry) = entries.next() else {
                dirs.pop();
                continue;
            };
            if seen == MAX_SCAN_ENTRIES || books.len() == MAX_BOOKS {
                truncated = true;
                break;
            }
            seen += 1;
            let Ok(entry) = entry else { continue };
            let path = relative.join(entry.file_name());
            if !safe_path(&path) {
                continue;
            }
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                if has_extension(&path, "sdr") {
                    continue;
                }
                if path.components().count() < MAX_PATH_DEPTH {
                    if let Ok(entries) = fs::read_dir(entry.path()) {
                        dirs.push((path, entries));
                    }
                } else {
                    truncated = true;
                }
            } else if kind.is_file()
                && let Some(format) = DocumentFormat::for_path(&path)
            {
                let Ok(metadata) = entry.metadata() else {
                    continue;
                };
                if metadata.len() <= format.max_bytes() {
                    let size = metadata.len();
                    let saved = known_metadata
                        .get(path.as_path())
                        .filter(|state| state.size == size);
                    books.push(library_book(path, size, format, saved.copied()));
                }
            }
        }
        books.sort_by_cached_key(|book| (book.title.to_lowercase(), book.path.clone()));
        self.books = books;
        self.truncated = truncated;
        Ok(())
    }

    /// List one folder at a time. An empty Documents-relative path selects the library root.
    pub fn browse(&self, folder: &Path) -> io::Result<BrowseList> {
        let directory = fs::read_dir(self.browse_folder(folder)?)?;
        let known_metadata = self.known_metadata();
        let mut entries = Vec::new();
        let mut truncated = false;
        for (index, entry) in directory.enumerate() {
            if index == MAX_SCAN_ENTRIES {
                truncated = true;
                break;
            }
            let Ok(entry) = entry else { continue };
            let filename = entry.file_name();
            let Some(name) = filename.to_str() else {
                continue;
            };
            let path = folder.join(&filename);
            if !safe_path(&path) {
                continue;
            }
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            let kind = if file_type.is_dir() && !has_extension(&path, "sdr") {
                if path.components().count() == MAX_PATH_DEPTH {
                    truncated = true;
                    continue;
                }
                BrowseKind::Folder
            } else if file_type.is_file() && !has_extension(&path, "sh") {
                let Ok(metadata) = entry.metadata() else {
                    continue;
                };
                if !metadata.is_file() {
                    continue;
                }
                let size = metadata.len();
                match DocumentFormat::for_path(&path) {
                    Some(DocumentFormat::Mobi)
                        if has_extension(&path, "azw")
                            && crate::mobi::protected_kindle_header(&entry.path())
                                .unwrap_or(false) =>
                    {
                        BrowseKind::KindleProtected { size }
                    }
                    Some(format) if size <= format.max_bytes() => {
                        let saved = known_metadata
                            .get(path.as_path())
                            .filter(|state| state.size == size);
                        BrowseKind::Readable(library_book(
                            path.clone(),
                            size,
                            format,
                            saved.copied(),
                        ))
                    }
                    format => BrowseKind::Unavailable {
                        size,
                        too_large: format.is_some(),
                    },
                }
            } else {
                continue;
            };
            if entries.len() == MAX_BROWSE_ENTRIES {
                truncated = true;
                break;
            }
            entries.push(BrowseEntry {
                path,
                name: display_filename(name),
                kind,
            });
        }
        entries.sort_by(|left, right| {
            let folder = |entry: &BrowseEntry| matches!(entry.kind, BrowseKind::Folder);
            let book = |entry: &BrowseEntry| crate::epub_members::is_folder_book(&entry.path);
            book(right)
                .cmp(&book(left))
                .then_with(|| folder(right).cmp(&folder(left)))
                .then_with(|| natural_cmp(&left.name, &right.name))
        });
        Ok(BrowseList { entries, truncated })
    }

    fn browse_folder(&self, relative: &Path) -> io::Result<PathBuf> {
        let denied = || {
            io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Choose a folder from the library",
            )
        };
        if (!relative.as_os_str().is_empty()
            && (!safe_path(relative) || relative.components().count() >= MAX_PATH_DEPTH))
            || !fs::symlink_metadata(&self.root)?.is_dir()
        {
            return Err(denied());
        }
        let mut absolute = self.root.clone();
        for component in relative.components() {
            let part = Path::new(component.as_os_str());
            if has_extension(part, "sdr") || part.to_string_lossy().chars().any(char::is_control) {
                return Err(denied());
            }
            absolute.push(part);
            if !fs::symlink_metadata(&absolute)?.is_dir() {
                return Err(denied());
            }
        }
        let verified = absolute.canonicalize()?;
        if !verified.starts_with(&self.root) {
            return Err(denied());
        }
        Ok(verified)
    }

    fn known_metadata(&self) -> HashMap<&Path, &BookState> {
        self.recent
            .iter()
            .rev()
            .map(|state| (state.path.as_path(), state))
            .collect()
    }

    pub fn entries(&self) -> &[LibraryBook] {
        &self.books
    }

    /// Use the same natural filename order and document availability as the folder picker.
    pub(crate) fn next_book_in_folder(&self, current: &LibraryBook) -> io::Result<Option<LibraryBook>> {
        let path = current.library_path();
        let folder = path.parent().unwrap_or_else(|| Path::new(""));
        let listing = self.browse(folder)?;
        let next = listing.entries.into_iter()
            .skip_while(|entry| entry.path != path)
            .skip(1)
            .find_map(|entry| match entry.kind {
                BrowseKind::Readable(book) => Some(book),
                BrowseKind::Folder => {
                    let marker = entry.path.join("mimetype");
                    if crate::epub_members::is_folder_book(&marker) {
                        self.resolve(&marker).ok()
                    } else {
                        None
                    }
                }
                _ => None,
            });
        Ok(next)
    }

    pub(crate) fn delete_book_file(&mut self, current: &LibraryBook, id: u64) -> io::Result<()> {
        if !current.can_delete_file() {
            return Err(io::Error::other("This book is a folder. Manage it in Files."));
        }
        let path = self.absolute_path(&current.path)?;
        if self.identify(current)? != id {
            return Err(io::Error::other("The file changed. Reopen it before deleting."));
        }
        fs::remove_file(path)?;
        self.books.retain(|entry| entry.path != current.path);
        if let Some(state) = self.recent.iter_mut().find(|state| state.id == id) {
            state.file_deleted = true;
        }
        Ok(())
    }

    /// Find the next numbered or named comic in the current folder's scanned entries.
    pub fn next_comic_in_folder(&self, current: &LibraryBook) -> Option<&LibraryBook> {
        if !current.format.is_comic() {
            return None;
        }
        let folder = current.path.parent()?;
        let current_name = current.path.file_name()?.to_str()?;
        self.books
            .iter()
            .filter(|book| book.format.is_comic() && book.path.parent() == Some(folder))
            .filter_map(|book| {
                let name = book.path.file_name()?.to_str()?;
                natural_cmp(name, current_name)
                    .is_gt()
                    .then_some((book, name))
            })
            .min_by(|(_, left), (_, right)| natural_cmp(left, right))
            .map(|(book, _)| book)
    }

    /// Case-folded substring filtering across titles, known authors, and file paths.
    pub fn matching<'a>(&'a self, query: &str) -> impl Iterator<Item = &'a LibraryBook> + use<'a> {
        let query = fold_search(query);
        self.books.iter().filter(move |book| {
            query
                .split_whitespace()
                .all(|word| book.searchable.contains(word))
        })
    }

    /// Parse metadata for one selected reflow document, leaving no book or storage handle open.
    pub fn metadata(&self, entry: &LibraryBook) -> io::Result<BookMetadata> {
        if !matches!(
            entry.format,
            DocumentFormat::Epub
                | DocumentFormat::Text
                | DocumentFormat::Html
                | DocumentFormat::Htmlz
                | DocumentFormat::Docx
                | DocumentFormat::Chm
                | DocumentFormat::Markdown
                | DocumentFormat::Fb2
                | DocumentFormat::Mobi
                | DocumentFormat::Rtf
                | DocumentFormat::Tcr
        ) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Choose a supported reflow document",
            ));
        }
        let path = self.absolute_path(&entry.path)?;
        let book = Book::open(&path).map_err(io::Error::other)?;
        Ok(BookMetadata {
            title: clean_metadata(&book.title),
            author: book
                .author
                .as_deref()
                .map(clean_metadata)
                .filter(|author| !author.is_empty()),
            series: book.series().to_vec(),
            has_cover: book.cover().is_some(),
        })
    }

    pub fn scan_truncated(&self) -> bool {
        self.truncated
    }

    pub fn resolve(&self, path: &Path) -> io::Result<LibraryBook> {
        let absolute = self.absolute_path(path)?;
        let relative = absolute.strip_prefix(&self.root).map_err(|_| denied())?;
        let format = DocumentFormat::for_path(relative).ok_or_else(denied)?;
        let size = fs::metadata(&absolute)?.len();
        Ok(library_book(relative.to_owned(), size, format, None))
    }

    /// A chapter selected inside an extracted publication shares the full book's
    /// illustrations, links, bookmarks and position. Unlisted HTML stays standalone.
    pub fn publication_for_chapter(
        &self,
        entry: &LibraryBook,
    ) -> Option<(LibraryBook, usize, String)> {
        if entry.format != DocumentFormat::Html {
            return None;
        }
        let folder = entry
            .path
            .ancestors()
            .skip(1)
            .find(|p| has_extension(p, "epub"))?;
        let name = entry.path.strip_prefix(folder).ok()?.to_str()?;
        let marker = self.resolve(&folder.join("mimetype")).ok()?;
        let book = Book::open(&self.absolute_path(&marker.path).ok()?).ok()?;
        let chapter = book.chapter_index(name)?;
        Some((marker, chapter, name.to_owned()))
    }

    pub fn absolute_path(&self, path: &Path) -> io::Result<PathBuf> {
        let relative = if path.is_absolute() {
            path.strip_prefix(&self.root)
                .or_else(|_| path.strip_prefix(&self.original_root))
                .map_err(|_| denied())?
        } else {
            path
        };
        if !safe_path(relative) {
            return Err(denied());
        }
        let format = DocumentFormat::for_path(relative).ok_or_else(|| {
            if has_extension(relative, "kfx") {
                io::Error::new(
                    io::ErrorKind::Unsupported,
                    "Kindle KFX files open in the Kindle Library.",
                )
            } else {
                denied()
            }
        })?;
        let mut absolute = self.root.clone();
        let mut components = relative.components().peekable();
        while let Some(component) = components.next() {
            absolute.push(component.as_os_str());
            let metadata = fs::symlink_metadata(&absolute)?;
            if components.peek().is_some() {
                if !metadata.is_dir() {
                    return Err(denied());
                }
            } else if !metadata.is_file() {
                return Err(denied());
            } else if metadata.len() > format.max_bytes() {
                if format == DocumentFormat::Mobi
                    && crate::mobi::protected_kindle_header(&absolute).unwrap_or(false)
                {
                    return Err(io::Error::other(crate::mobi::MobiError::Protected));
                }
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "This document exceeds the {} MiB limit for its format",
                        format.max_bytes() / (1024 * 1024)
                    ),
                ));
            }
        }
        let verified = absolute.canonicalize()?;
        if !verified.starts_with(&self.root) {
            return Err(denied());
        }
        Ok(verified)
    }

    /// KOReader keeps metadata for books sharing a stem in one directory, with one file per format.
    pub(crate) fn nearby_koreader_metadata(&self, path: &Path) -> io::Result<Option<PathBuf>> {
        let book = self.absolute_path(path)?;
        let Some(extension) = book.extension().and_then(|ext| ext.to_str()) else {
            return Ok(None);
        };
        let directory = book.with_extension("sdr");
        match fs::symlink_metadata(&directory) {
            Ok(dir) if dir.is_dir() => {}
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(err) => return Err(err),
            Ok(_) => return Err(denied()),
        }
        let metadata = directory.join(format!("metadata.{extension}.lua"));
        match fs::symlink_metadata(&metadata) {
            Ok(file) if file.is_file() => Ok(Some(metadata)),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(err),
            Ok(_) => Err(denied()),
        }
    }

    /// Fingerprint a file or a validated archive index, independent of its library path.
    pub fn identify(&self, entry: &LibraryBook) -> io::Result<u64> {
        let path = self.absolute_path(&entry.path)?;
        let file = File::open(&path)?;
        let size = file.metadata()?.len();
        let format = DocumentFormat::for_path(&entry.path)
            .filter(|format| *format == entry.format)
            .ok_or_else(denied)?;
        if size != entry.size || size > format.max_bytes() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "The document changed while opening",
            ));
        }
        if format == DocumentFormat::Cbr {
            return crate::cbr::Cbr::open(&path)
                .map(|comic| comic.identity())
                .map_err(io::Error::other);
        }
        if crate::epub_members::is_folder_book(&entry.path) {
            return crate::epub_members::folder_identity(&path);
        }
        if format == DocumentFormat::Epub {
            let archive = crate::epub_zip::open(file, size).map_err(io::Error::other)?;
            let (input, offset) = archive.zip.into_inner().original_directory()?;
            return fingerprint(input, size, format.tag(), size - offset);
        }
        if format != DocumentFormat::Cbz {
            return fingerprint(file, size, format.tag(), size);
        }
        let mut input = BufReader::new(file);
        let entries = check_zip_directory(&mut input, size).map_err(io::Error::other)?;
        let archive = ZipArchive::new(input).map_err(io::Error::other)?;
        if archive.len() != entries {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "The EPUB archive has duplicate entries",
            ));
        }
        let directory = archive.central_directory_start();
        let bytes_to_hash = size.checked_sub(directory).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "The EPUB ZIP directory moved while opening",
            )
        })?;
        let mut input = archive.into_inner();
        input.seek(SeekFrom::Start(directory))?;
        fingerprint(input, size, format.tag(), bytes_to_hash)
    }

    pub fn state(&self, id: u64) -> Option<&BookState> {
        self.recent.iter().find(|state| state.id == id)
    }

    #[cfg(feature = "kindle")]
    pub(crate) fn state_mut(&mut self, id: u64) -> Option<&mut BookState> {
        self.recent.iter_mut().find(|state| state.id == id)
    }

    pub fn recents(&self) -> &[BookState] {
        &self.recent
    }

    /// Filter out organization-only records that have never recorded a reading position.
    pub fn recently_read(&self) -> impl Iterator<Item = &BookState> {
        self.recent.iter().filter(|book| book.position.is_some() && !book.file_deleted)
    }

    pub fn favorites(&self) -> impl Iterator<Item = &BookState> {
        self.recent.iter().filter(|book| book.favorite)
    }

    /// Create or relocate metadata for a book without changing the reading order in Recents.
    pub fn track(&mut self, entry: &LibraryBook, id: u64) -> &mut BookState {
        if let Some(index) = self.recent.iter().position(|book| book.id == id) {
            let book = &mut self.recent[index];
            book.path.clone_from(&entry.path);
            book.size = entry.size;
            book.file_deleted = false;
            return book;
        }
        self.recent.push(BookState::new(entry, id));
        self.recent.last_mut().unwrap()
    }

    pub fn remember(&mut self, entry: &LibraryBook, id: u64, title: &str) -> &mut BookState {
        let title = clean_metadata(title);
        let title = if title.is_empty() {
            entry.title.clone()
        } else {
            title
        };
        let prior = self.recent.iter().position(|item| item.id == id);
        let mut state = prior
            .map(|index| self.recent.remove(index))
            .unwrap_or_else(|| BookState::new(entry, id));
        if (prior.is_none() || state.path != entry.path || state.title != title)
            && let Some(book) = self.books.iter_mut().find(|book| book.path == entry.path)
        {
            book.title.clone_from(&title);
            book.searchable = search_key(&book.title, book.author.as_deref(), &book.path);
        }
        state.path.clone_from(&entry.path);
        state.title = title;
        state.size = entry.size;
        state.file_deleted = false;
        self.recent.insert(0, state);
        &mut self.recent[0]
    }

    #[cfg(feature = "kindle")]
    pub(crate) fn cache_author(&mut self, id: u64, author: Option<&str>) {
        let Some(state) = self.recent.iter_mut().find(|book| book.id == id) else {
            return;
        };
        let author = author.map(clean_metadata).filter(|value| !value.is_empty());
        if state.author != author {
            state.author.clone_from(&author);
        }
        if let Some(entry) = self.books.iter_mut().find(|entry| entry.path == state.path)
            && entry.author != author
        {
            entry.author = author;
            entry.searchable = search_key(&entry.title, entry.author.as_deref(), &entry.path);
        }
    }

    pub fn collections(&self) -> &[Collection] {
        &self.collections
    }

    pub fn collection(&self, name: &str) -> Option<&Collection> {
        collection_index(&self.collections, name).map(|index| &self.collections[index])
    }

    pub fn create_collection(&mut self, name: &str) -> io::Result<()> {
        let name = collection_name(name)?;
        if self.collection(name).is_some() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "A collection already uses that name",
            ));
        }
        if self.collections.len() >= MAX_COLLECTIONS {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Reader reached its collection limit",
            ));
        }
        self.collections.push(Collection {
            name: name.to_owned(),
            book_ids: Vec::new(),
        });
        Ok(())
    }

    pub fn rename_collection(&mut self, old: &str, new: &str) -> io::Result<bool> {
        let name = collection_name(new)?;
        let index = collection_index(&self.collections, old).ok_or_else(collection_missing)?;
        if collection_index(&self.collections, name).is_some_and(|existing| existing != index) {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "A collection already uses that name",
            ));
        }
        let collection = &mut self.collections[index];
        if collection.name == name {
            return Ok(false);
        }
        collection.name = name.to_owned();
        Ok(true)
    }

    pub fn remove_collection(&mut self, name: &str) -> bool {
        let Some(index) = collection_index(&self.collections, name) else {
            return false;
        };
        self.collections.remove(index);
        true
    }

    pub fn set_collection_membership(
        &mut self,
        name: &str,
        id: u64,
        included: bool,
    ) -> io::Result<bool> {
        let index = collection_index(&self.collections, name).ok_or_else(collection_missing)?;
        if included && self.state(id).is_none() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "That book is absent from Reader's tracked library",
            ));
        }
        let collection = &mut self.collections[index];
        match (
            collection.book_ids.iter().position(|&member| member == id),
            included,
        ) {
            (Some(member), false) => {
                collection.book_ids.remove(member);
                Ok(true)
            }
            (None, true) if collection.book_ids.len() >= MAX_COLLECTION_MEMBERS => {
                Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "This collection reached its book limit",
                ))
            }
            (None, true) => {
                collection.book_ids.push(id);
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    pub fn locate(&self, state: &BookState) -> io::Result<LibraryBook> {
        let matches = |book: &LibraryBook| self.identify(book).is_ok_and(|id| id == state.id);
        if let Ok(book) = self.resolve(&state.path)
            && matches(&book)
        {
            return Ok(book);
        }
        let format = DocumentFormat::for_path(&state.path);
        let mut bytes_left = MAX_RELOCATION_READ_BYTES;
        for book in self
            .books
            .iter()
            .filter(|book| {
                Some(book.format) == format && book.size == state.size && book.path != state.path
            })
            .take(MAX_RELOCATION_CHECKS)
        {
            let cost = match book.format {
                DocumentFormat::Epub | DocumentFormat::Cbz => book.size.min(9 * 1024 * 1024),
                DocumentFormat::Cbr => book.size.min(4 * 1024 * 1024),
                _ => book.size,
            };
            let Some(remaining) = bytes_left.checked_sub(cost) else {
                break;
            };
            bytes_left = remaining;
            if matches(book) {
                return Ok(book.clone());
            }
        }
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            "This book is missing. Rescan the library or open it in its new folder.",
        ))
    }

    pub fn bytes(&self) -> io::Result<Vec<u8>> {
        encode_library(&self.recent, &self.collections, MAX_STATE_BYTES)
    }

    /// Export Reader metadata as a versioned JSON document.
    pub fn backup_bytes(&self) -> io::Result<Vec<u8>> {
        encode_library(&self.recent, &self.collections, MAX_BACKUP_BYTES)
    }

    /// Calculate the effect of an import without touching the library, books, or state file.
    pub fn preview_backup(&self, bytes: &[u8]) -> io::Result<BackupImport> {
        if bytes.len() as u64 > MAX_BACKUP_BYTES {
            return Err(invalid_state("Reader backup exceeds its size limit"));
        }
        let backup: SavedLibrary = serde_json::from_slice(bytes).map_err(io::Error::other)?;
        if backup.version != BACKUP_VERSION || backup.books.len() > MAX_BACKUP_BOOKS {
            return Err(invalid_state(
                "Reader backup version or book count is unsupported",
            ));
        }
        let (merged, mut summary) = merge_states(&self.recent, &backup.books);
        let collections = merge_collections(
            &self.collections,
            &backup.collections,
            &merged,
            &mut summary,
        );
        encode_library(&merged, &collections, MAX_STATE_BYTES)?;
        Ok(BackupImport {
            books: backup.books,
            collections: backup.collections,
            summary,
        })
    }

    /// Revalidate the proposed merge against current state, then apply it in memory.
    pub fn merge_backup(&mut self, import: BackupImport) -> io::Result<BackupSummary> {
        let (merged, mut summary) = merge_states(&self.recent, &import.books);
        let collections = merge_collections(
            &self.collections,
            &import.collections,
            &merged,
            &mut summary,
        );
        encode_library(&merged, &collections, MAX_STATE_BYTES)?;
        self.recent = merged;
        self.collections = collections;
        Ok(summary)
    }

    pub fn save_with(&self, write: impl FnOnce(&Path, &[u8]) -> io::Result<()>) -> io::Result<()> {
        write(&self.state_path, &self.bytes()?)
    }

    #[cfg(feature = "kindle")]
    pub fn persist(&self) -> io::Result<()> {
        self.save_with(primitive_app::save)
    }
}

fn collection_name(value: &str) -> io::Result<&str> {
    let name = value.trim();
    if name.is_empty()
        || name.chars().any(char::is_control)
        || name.chars().count() > MAX_COLLECTION_NAME_CHARS
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Enter a collection name of 1 to 64 characters",
        ));
    }
    Ok(name)
}

fn collection_index(collections: &[Collection], name: &str) -> Option<usize> {
    let key = fold_search(name.trim());
    collections
        .iter()
        .position(|collection| fold_search(&collection.name) == key)
}

fn collection_missing() -> io::Error {
    io::Error::new(
        io::ErrorKind::NotFound,
        "That collection is absent from Reader",
    )
}

fn merge_collections(
    existing: &[Collection],
    imported: &[Collection],
    books: &[BookState],
    summary: &mut BackupSummary,
) -> Vec<Collection> {
    let valid_ids: HashSet<_> = books.iter().map(|book| book.id).collect();
    let mut collections = existing.to_vec();
    let mut names: HashMap<_, _> = collections
        .iter()
        .enumerate()
        .map(|(index, collection)| (fold_search(&collection.name), index))
        .collect();
    let mut members = HashMap::<usize, HashSet<u64>>::new();
    for source in imported {
        let Ok(name) = collection_name(&source.name) else {
            summary.skipped += 1;
            continue;
        };
        let key = fold_search(name);
        let index = if let Some(index) = names.get(&key) {
            *index
        } else if collections.len() < MAX_COLLECTIONS {
            let index = collections.len();
            collections.push(Collection {
                name: name.to_owned(),
                book_ids: Vec::new(),
            });
            names.insert(key, index);
            summary.collections_added += 1;
            index
        } else {
            summary.skipped += 1;
            continue;
        };
        let ids = &mut collections[index].book_ids;
        let seen = members
            .entry(index)
            .or_insert_with(|| ids.iter().copied().collect());
        for &id in &source.book_ids {
            if seen.contains(&id) {
                continue;
            }
            if ids.len() == MAX_COLLECTION_MEMBERS || !valid_ids.contains(&id) {
                summary.skipped += 1;
                continue;
            }
            seen.insert(id);
            ids.push(id);
            summary.memberships_added += 1;
        }
    }
    collections
}

fn encode_library(
    books: &[BookState],
    collections: &[Collection],
    limit: u64,
) -> io::Result<Vec<u8>> {
    validate_state(books)?;
    state_json(
        &SavedLibraryRef {
            version: BACKUP_VERSION,
            books,
            collections,
        },
        limit,
    )
}

fn validate_state(states: &[BookState]) -> io::Result<()> {
    if states
        .iter()
        .any(|state| !state.fixed_preferences.is_valid())
    {
        return Err(invalid_state(
            "Reader state contains an invalid fixed-page view",
        ));
    }
    if states.iter().any(|state| {
        state.pdf_highlights.len() > MAX_PDF_HIGHLIGHTS
            || state
                .pdf_highlights
                .iter()
                .any(|highlight| !highlight.is_valid())
    }) {
        return Err(invalid_state(
            "Reader state contains an invalid PDF highlight",
        ));
    }
    Ok(())
}

fn state_json(value: &impl Serialize, limit: u64) -> io::Result<Vec<u8>> {
    let bytes = serde_json::to_vec(value).map_err(io::Error::other)?;
    if bytes.len() as u64 > limit {
        return Err(invalid_state("Reader state reached its size limit"));
    }
    Ok(bytes)
}

fn invalid_state(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.to_owned())
}

fn merge_states(current: &[BookState], backup: &[BookState]) -> (Vec<BookState>, BackupSummary) {
    let mut merged = current.to_vec();
    let mut indices: HashMap<u64, usize> = current
        .iter()
        .enumerate()
        .map(|(index, book)| (book.id, index))
        .collect();
    let mut summary = BackupSummary::default();
    for source in backup {
        let Some(format) = DocumentFormat::for_path(&source.path)
            .filter(|format| safe_path(&source.path) && source.size <= format.max_bytes())
        else {
            summary.skipped += 1;
            continue;
        };
        let target = if let Some(&index) = indices.get(&source.id) {
            if DocumentFormat::for_path(&merged[index].path) != Some(format) {
                summary.skipped += 1;
                continue;
            }
            summary.existing_books += 1;
            &mut merged[index]
        } else if merged.len() < MAX_BACKUP_BOOKS {
            let mut added = source.clone();
            added.bookmarks = Vec::new();
            added.custom_contents = custom_contents::CustomContents::default();
            added.custom_contents.enabled = source.custom_contents.enabled;
            added.hidden_flows = hidden_flows::HiddenFlows::default();
            added.hidden_flows.enabled = source.hidden_flows.enabled;
            added.hidden_flows.editing = source.hidden_flows.editing;
            added.highlights = Vec::new();
            added.pdf_highlights = Vec::new();
            added.title = clean_metadata(&added.title);
            added.author = added
                .author
                .as_deref()
                .map(clean_metadata)
                .filter(|author| !author.is_empty());
            if added
                .position
                .as_ref()
                .is_some_and(|position| !valid_position(position))
            {
                added.position = None;
                summary.skipped += 1;
            }
            let index = merged.len();
            indices.insert(added.id, index);
            merged.push(added);
            summary.new_books += 1;
            &mut merged[index]
        } else {
            summary.skipped += 1;
            continue;
        };
        for entry in source.custom_contents.entries() {
            if target.custom_contents.find(&entry.position).is_some() {
                summary.duplicates += 1;
            } else if target
                .custom_contents
                .save(entry.position.clone(), &entry.label)
                .is_ok()
            {
                summary.custom_chapters_added += 1;
            } else {
                summary.skipped += 1;
            }
        }
        for point in source.hidden_flows.points() {
            match target.hidden_flows.insert(point.clone()) {
                Ok(true) => summary.custom_flow_points_added += 1,
                Ok(false) => summary.duplicates += 1,
                Err(_) => summary.skipped += 1,
            }
        }
        if target.hide_nonlinear.is_none() { target.hide_nonlinear = source.hide_nonlinear; }
        merge_text_marks(target, &source.bookmarks, &source.highlights, &mut summary);
        for mark in &source.pdf_highlights {
            if !mark.is_valid() {
                summary.skipped += 1;
            } else if let Some(existing) = target
                .pdf_highlights
                .iter_mut()
                .find(|existing| existing.page == mark.page && existing.bounds == mark.bounds && existing.continuation == mark.continuation)
            {
                merge_created_at(&mut existing.created_at, mark.created_at, &mut summary);
                merge_note(&mut existing.note, &mark.note, &mut summary);
            } else if target.pdf_highlights.len() < MAX_PDF_HIGHLIGHTS {
                target.pdf_highlights.push(mark.clone());
                summary.pdf_highlights_added += 1;
                summary.notes_added += usize::from(mark.note.is_some());
            } else {
                summary.skipped += 1;
            }
        }
    }
    (merged, summary)
}

pub(crate) fn merge_text_marks(
    target: &mut BookState,
    bookmarks: &[Bookmark],
    highlights: &[Highlight],
    summary: &mut BackupSummary,
) {
    for bookmark in bookmarks {
        let label = clean_metadata(&bookmark.label);
        if !valid_position(&bookmark.position) || label.is_empty() {
            summary.skipped += 1;
        } else if let Some(existing) = target
            .bookmarks
            .iter_mut()
            .find(|existing| same_position(&existing.position, &bookmark.position))
        {
            merge_created_at(&mut existing.created_at, bookmark.created_at, summary);
            summary.duplicates += 1;
        } else if target.bookmarks.len() < MAX_IMPORTED_MARKS {
            target.bookmarks.push(Bookmark {
                created_at: bookmark.created_at,
                label,
                position: bookmark.position.clone(),
            });
            summary.bookmarks_added += 1;
        } else {
            summary.skipped += 1;
        }
    }
    for mark in highlights {
        if !valid_highlight(mark) {
            summary.skipped += 1;
        } else if let Some(existing) = target.highlights.iter_mut().find(|existing| {
            existing.selection.start == mark.selection.start
                && existing.selection.end == mark.selection.end
        }) {
            merge_created_at(&mut existing.created_at, mark.created_at, summary);
            merge_note(&mut existing.note, &mark.note, summary);
        } else if target.highlights.len() < MAX_IMPORTED_MARKS {
            target.highlights.push(mark.clone());
            summary.highlights_added += 1;
            summary.notes_added += usize::from(mark.note.is_some());
        } else {
            summary.skipped += 1;
        }
    }
}

fn merge_created_at(current: &mut Option<i64>, imported: Option<i64>, summary: &mut BackupSummary) {
    if current.is_none() && imported.is_some() {
        *current = imported;
        summary.timestamps_added += 1;
    }
}

fn merge_note(
    current: &mut Option<String>,
    imported: &Option<String>,
    summary: &mut BackupSummary,
) {
    summary.duplicates += 1;
    if let Some(note) = imported.as_ref().filter(|note| !note.trim().is_empty()) {
        match current {
            Some(current) if current == note => {}
            Some(current) if !current.trim().is_empty() => summary.note_conflicts += 1,
            _ => {
                *current = Some(note.clone());
                summary.notes_added += 1;
            }
        }
    }
}

fn valid_position(position: &ReadingPosition) -> bool {
    position.anchor.as_ref().is_none_or(valid_anchor)
}

fn valid_anchor(anchor: &Anchor) -> bool {
    !anchor.chapter.is_empty()
        && anchor.chapter.len() <= MAX_ANCHOR_PATH_BYTES
        && !anchor.chapter.chars().any(char::is_control)
}

fn same_position(left: &ReadingPosition, right: &ReadingPosition) -> bool {
    match (&left.anchor, &right.anchor) {
        (Some(left), Some(right)) => left == right,
        (None, None) => left.chapter == right.chapter && left.page == right.page,
        _ => false,
    }
}

fn valid_highlight(mark: &Highlight) -> bool {
    let selection = &mark.selection;
    valid_anchor(&selection.start)
        && valid_anchor(&selection.end)
        && selection.start.book == selection.end.book
        && (selection.start.chapter != selection.end.chapter
            || (selection.start.revision == selection.end.revision
                && (selection.start.block, selection.start.offset)
                    < (selection.end.block, selection.end.offset)))
        && !selection.text.trim().is_empty()
        && selection.text.len() <= MAX_HIGHLIGHT_TEXT_BYTES
        && mark
            .note
            .as_ref()
            .is_none_or(|note| note.len() <= MAX_NOTE_BYTES)
}

fn load_library_state(path: &Path) -> io::Result<(Vec<BookState>, Vec<Collection>)> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() && metadata.len() <= MAX_STATE_BYTES => {}
        Ok(_) => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Reader state is unavailable or too large",
            ));
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok((Vec::new(), Vec::new()));
        }
        Err(error) => return Err(error),
    }
    let mut bytes = Vec::new();
    File::open(path)?
        .take(MAX_STATE_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_STATE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Reader state exceeds its size limit",
        ));
    }
    let (mut recent, collections) =
        if bytes.iter().find(|byte| !byte.is_ascii_whitespace()) == Some(&b'[') {
            let books: Vec<BookState> = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
            (books, Vec::new())
        } else {
            let state: SavedLibrary = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
            if state.version != BACKUP_VERSION {
                return Err(invalid_state("Reader state version is unsupported"));
            }
            (state.books, state.collections)
        };
    let mut ids = HashSet::new();
    recent.retain(|state| {
        safe_path(&state.path)
            && DocumentFormat::for_path(&state.path).is_some()
            && ids.insert(state.id)
    });
    for state in &mut recent {
        state.custom_contents.sanitize();
        state.hidden_flows.sanitize();
        state.pdf_highlights.retain(PdfHighlight::is_valid);
        state.pdf_highlights.truncate(MAX_PDF_HIGHLIGHTS);
    }
    let collections = merge_collections(&[], &collections, &recent, &mut BackupSummary::default());
    Ok((recent, collections))
}

fn fingerprint_update(hash: u64, bytes: &[u8]) -> u64 {
    bytes.iter().fold(hash, |hash, &byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    })
}

fn fingerprint_prefix(size: u64, tag: &[u8]) -> u64 {
    fingerprint_update(
        fingerprint_update(0xcbf29ce484222325, &size.to_le_bytes()),
        tag,
    )
}

#[cfg(feature = "kindle")]
pub(crate) fn fingerprint_bytes(bytes: &[u8], tag: &[u8]) -> u64 {
    fingerprint_update(fingerprint_prefix(bytes.len() as u64, tag), bytes)
}

pub(crate) fn fingerprint(
    mut input: impl Read,
    size: u64,
    tag: &[u8],
    mut length: u64,
) -> io::Result<u64> {
    let mut hash = fingerprint_prefix(size, tag);
    let mut buffer = [0; 32 * 1024];
    while length > 0 {
        let count = length.min(buffer.len() as u64) as usize;
        input.read_exact(&mut buffer[..count])?;
        hash = fingerprint_update(hash, &buffer[..count]);
        length -= count as u64;
    }
    if input.read(&mut buffer[..1])? != 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "The document changed while opening",
        ));
    }
    Ok(hash)
}

fn library_book(
    path: PathBuf,
    size: u64,
    format: DocumentFormat,
    known: Option<&BookState>,
) -> LibraryBook {
    let title = known.map(|state| state.title.clone()).unwrap_or_else(|| {
        let source = if crate::epub_members::is_folder_book(&path) {
            path.parent().unwrap_or(&path)
        } else {
            &path
        };
        source.file_stem().map_or_else(
            || "Untitled book".to_owned(),
            |name| display_filename(&name.to_string_lossy()),
        )
    });
    let author = known
        .and_then(|state| state.author.as_deref())
        .map(clean_metadata)
        .filter(|author| !author.is_empty());
    let searchable = search_key(&title, author.as_deref(), &path);
    LibraryBook {
        path,
        title,
        author,
        size,
        format,
        searchable,
    }
}

fn search_key(title: &str, author: Option<&str>, path: &Path) -> String {
    fold_search(&format!(
        "{title}\n{}\n{}",
        author.unwrap_or_default(),
        path.to_string_lossy()
    ))
}

pub(crate) fn fold_search(text: &str) -> String {
    if text.is_ascii() {
        return text.to_ascii_lowercase();
    }
    let decomposed = DecomposingNormalizer::new_nfd().normalize(text);
    let folded = CaseMapper::new().fold_string(&decomposed);
    ComposingNormalizer::new_nfc()
        .normalize(&folded)
        .into_owned()
}

/// Keep the file's actual path for opening and persistence. Some Kindle downloads
/// have C1 controls in their filename that should never enter a visible label.
pub(crate) fn display_filename(value: &str) -> String {
    value
        .chars()
        .map(|ch| if ch.is_control() { '�' } else { ch })
        .collect()
}

fn clean_metadata(value: &str) -> String {
    value
        .trim()
        .chars()
        .filter(|char| !char.is_control())
        .take(256)
        .collect()
}

fn safe_path(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && path.components().count() <= MAX_PATH_DEPTH
        && path.components().all(|component| matches!(component, Component::Normal(name) if name.to_str().is_some_and(|name| !name.starts_with('.'))))
}

fn has_extension(path: &Path, extension: &str) -> bool {
    path.extension()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case(extension))
}

fn denied() -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        "Choose a supported document from the library",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const EPUB2: &[u8] = include_bytes!("../tests/fixtures/field-notes-epub2.epub");
    const EPUB3: &[u8] = include_bytes!("../tests/fixtures/field-notes-epub3.epub");

    #[cfg(feature = "kindle")]
    #[test]
    fn single_read_pdf_preserves_known_book_identity_and_detects_a_size_change() {
        let scratch = Scratch::new();
        let path = scratch.0.join("single-read.pdf");
        let expected = (0..=255u8).cycle().take(102_400).collect::<Vec<_>>();
        fs::write(&path, &expected).unwrap();
        let library = Library::open(&scratch.0, &scratch.0.join("state.json")).unwrap();
        let entry = library.resolve(&path).unwrap();
        let bytes = crate::fixed_session::pdf_bytes(&path, Some(entry.size)).unwrap();
        let memory_id = fingerprint_bytes(&bytes, DocumentFormat::Pdf.tag());
        assert_eq!(bytes, expected);
        assert_eq!(memory_id, 0x12fc66d5b554deac);
        assert_eq!(library.identify(&entry).unwrap(), memory_id);

        fs::write(&path, &expected[..1000]).unwrap();
        assert!(crate::fixed_session::pdf_bytes(&path, Some(entry.size)).is_err());
        assert!(library.identify(&entry).is_err());
    }

    struct Scratch(PathBuf);

    impl Scratch {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!(
                "primitive-reader-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed),
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn reads_and_identifies_an_extracted_epub_as_a_complete_book() {
        let scratch = Scratch::new();
        let root = scratch.0.join("documents");
        let extracted = root.join("Field notes.epub");
        fs::create_dir_all(&extracted).unwrap();
        let mut zip = ZipArchive::new(io::Cursor::new(EPUB3)).unwrap();
        zip.extract(&extracted).unwrap();
        let library = Library::open(&root, &scratch.0.join("state.json")).unwrap();
        let root_rows = library.browse(Path::new("")).unwrap();
        assert!(matches!(root_rows.entries[0].kind, BrowseKind::Folder));
        let inside = library.browse(&root_rows.entries[0].path).unwrap();
        let marker = inside
            .entries
            .iter()
            .find(|entry| crate::epub_members::is_folder_book(&entry.path))
            .unwrap();
        let BrowseKind::Readable(book) = &marker.kind else {
            panic!("Missing extracted book")
        };
        assert_eq!(book.title, "Field notes");
        let identity = library.identify(book).unwrap();
        let mut reading = Book::open(&extracted.join("mimetype")).unwrap();
        assert!(reading.chapters.len() >= 2);
        assert!(reading.chapter(1).unwrap().contains("<body"));
        let chapter = extracted.join(&reading.chapters[1].href);
        let chapter_entry = library.resolve(&chapter).unwrap();
        let (publication, index, href) = library.publication_for_chapter(&chapter_entry).unwrap();
        assert_eq!(publication.path, book.path);
        assert_eq!(index, 1);
        assert_eq!(href, reading.chapters[1].href);
        File::options()
            .append(true)
            .open(&chapter)
            .unwrap()
            .write_all(b" ")
            .unwrap();
        assert_ne!(identity, library.identify(book).unwrap());
        #[cfg(unix)]
        {
            let outside = scratch.0.join("private.txt");
            fs::write(&outside, b"outside publication").unwrap();
            std::os::unix::fs::symlink(&outside, extracted.join("escape.txt")).unwrap();
            assert!(reading.resource("escape.txt").is_err());
            assert!(reading.resource("../private.txt").is_err());
        }
    }

    #[test]
    fn picker_labels_an_encrypted_kindle_book_even_when_it_is_large() {
        let scratch = Scratch::new();
        let path = scratch.0.join("Dictionary.azw");
        let mut header = vec![0; 108];
        header[60..68].copy_from_slice(b"BOOKMOBI");
        header[77] = 1;
        header[81] = 88;
        header[101] = 2;
        header[104..108].copy_from_slice(b"MOBI");
        let mut book = File::create(&path).unwrap();
        book.write_all(&header).unwrap();
        book.set_len(DocumentFormat::Mobi.max_bytes() + 1).unwrap();
        let library = Library::open(&scratch.0, &scratch.0.join("reading.json")).unwrap();
        let items = library.browse(Path::new("")).unwrap();
        assert!(matches!(
            items.entries[0].kind,
            BrowseKind::KindleProtected { .. }
        ));
        assert!(
            library
                .resolve(&path)
                .unwrap_err()
                .to_string()
                .contains("encrypted")
        );
    }

    #[test]
    fn picker_can_open_a_filename_with_an_invisible_character() {
        let scratch = Scratch::new();
        let root = scratch.0.join("documents");
        fs::create_dir(&root).unwrap();
        let actual = Path::new("Field\u{0080}notes.epub");
        fs::write(root.join(actual), EPUB3).unwrap();
        let library = Library::open(&root, &scratch.0.join("state.json")).unwrap();
        let rows = library.browse(Path::new("")).unwrap();
        assert_eq!(rows.entries.len(), 1);
        assert_eq!(rows.entries[0].name, "Field�notes.epub");
        assert_eq!(rows.entries[0].path, actual);
        let book = library.resolve(&rows.entries[0].path).unwrap();
        assert_eq!(book.title, "Field�notes");
        assert!(library.identify(&book).is_ok());
    }

    #[test]
    fn old_margin_values_and_new_overrides_decode_independently() {
        let original: BookPreferences = serde_json::from_str(r#"{"margin":40}"#).unwrap();
        let legacy = original.apply_to(Settings::default());
        assert_eq!(
            (
                legacy.side_margin(),
                legacy.top_margin(),
                legacy.bottom_margin()
            ),
            (40, 40, 40)
        );
        let separate = BookPreferences {
            margin_sides: Some(100),
            margin_top: Some(12),
            margin_bottom: Some(75),
            ..original
        };
        let bytes = serde_json::to_vec(&separate).unwrap();
        let restored: BookPreferences = serde_json::from_slice(&bytes).unwrap();
        let settings = restored.apply_to(Settings::default());
        assert_eq!(
            (
                settings.side_margin(),
                settings.top_margin(),
                settings.bottom_margin()
            ),
            (100, 12, 75)
        );
    }

    #[test]
    fn browses_one_folder_with_known_metadata_and_unavailable_files() {
        let scratch = Scratch::new();
        let root = scratch.0.join("documents");
        for folder in ["Zines", "Books/Series", "Field Notes.epub.SDR", ".hidden"] {
            fs::create_dir_all(root.join(folder)).unwrap();
        }
        fs::write(root.join("Books/Series/Volume 10.txt"), b"ten").unwrap();
        fs::write(root.join("Books/Series/volume 2.txt"), b"two").unwrap();
        fs::write(root.join("A document.xps"), b"xps").unwrap();
        fs::write(root.join("Notes.txt"), b"notes").unwrap();
        fs::write(root.join("Launcher.SH"), b"scriptlet").unwrap();
        fs::write(root.join(".private.txt"), b"hidden").unwrap();
        File::create(root.join("Too big.TXT"))
            .unwrap()
            .set_len(MAX_INPUT as u64 + 1)
            .unwrap();

        let mut library = Library::open(&root, &scratch.0.join("reader.json")).unwrap();
        let book = library.resolve(Path::new("Notes.txt")).unwrap();
        let saved = library.remember(&book, 1, "Field notes");
        saved.author = Some("Author".into());

        let browse = library.browse(Path::new("")).unwrap();
        assert!(!browse.truncated);
        assert_eq!(
            browse
                .entries
                .iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>(),
            [
                "Books",
                "Zines",
                "A document.xps",
                "Notes.txt",
                "Too big.TXT"
            ],
        );
        assert_eq!(browse.entries[0].path, Path::new("Books"));
        assert!(matches!(browse.entries[0].kind, BrowseKind::Folder));
        assert!(matches!(
            browse.entries[2].kind,
            BrowseKind::Unavailable {
                size: 3,
                too_large: false
            },
        ));
        let BrowseKind::Readable(book) = &browse.entries[3].kind else {
            panic!("Text file is missing from the picker");
        };
        assert_eq!(book.title, "Field notes");
        assert_eq!(book.author.as_deref(), Some("Author"));
        assert!(matches!(
            browse.entries[4].kind,
            BrowseKind::Unavailable { size, too_large: true } if size == MAX_INPUT as u64 + 1
        ));

        let books = library.browse(Path::new("Books")).unwrap();
        assert_eq!(books.entries.len(), 1);
        assert_eq!(books.entries[0].path, Path::new("Books/Series"));
        let series = library.browse(&books.entries[0].path).unwrap();
        assert_eq!(series.entries.len(), 2);
        assert_eq!(series.entries[0].name, "volume 2.txt");
        assert_eq!(series.entries[1].name, "Volume 10.txt");
        assert!(library.entries().is_empty());
        library.scan().unwrap();
        assert_eq!(library.matching("volume").count(), 2);
    }

    #[test]
    fn rejects_hidden_and_escaping_browse_folders() {
        let scratch = Scratch::new();
        let root = scratch.0.join("documents");
        let outside = scratch.0.join("outside");
        fs::create_dir(&outside).unwrap();
        fs::create_dir_all(root.join("Books")).unwrap();
        fs::create_dir(root.join(".private")).unwrap();
        fs::create_dir(root.join("Notes.epub.sdr")).unwrap();
        fs::write(root.join("Notes.txt"), b"notes").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            symlink(&outside, root.join("outside")).unwrap();
            symlink(root.join("Books"), root.join("alias")).unwrap();
            symlink(root.join("Notes.txt"), root.join("alias.txt")).unwrap();
        }
        let library = Library::open(&root, &scratch.0.join("reader.json")).unwrap();
        let browse = library.browse(Path::new("")).unwrap();
        assert_eq!(browse.entries.len(), 2);
        for path in [
            ".private",
            "../outside",
            "Notes.epub.sdr",
            "Notes.txt",
            "Books/../Books",
        ] {
            let error = library.browse(Path::new(path)).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::PermissionDenied, "{path}");
        }
        assert!(library.browse(&outside).is_err());
        #[cfg(unix)]
        for path in ["outside", "alias", "alias.txt"] {
            let error = library.browse(Path::new(path)).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::PermissionDenied, "{path}");
        }
    }

    #[test]
    fn folder_browse_reports_truncation_at_the_depth_limit() {
        let scratch = Scratch::new();
        let root = scratch.0.join("documents");
        let folder = PathBuf::from_iter(std::iter::repeat_n("folder", MAX_PATH_DEPTH - 1));
        fs::create_dir_all(root.join(&folder).join("deeper")).unwrap();
        fs::write(root.join(&folder).join("Notes.txt"), b"notes").unwrap();
        let library = Library::open(&root, &scratch.0.join("reader.json")).unwrap();
        let browse = library.browse(&folder).unwrap();
        assert!(browse.truncated);
        assert_eq!(browse.entries.len(), 1);
        assert_eq!(browse.entries[0].name, "Notes.txt");
        assert!(library.browse(&folder.join("deeper")).is_err());
    }

    #[test]
    fn next_comic_uses_natural_filename_order_within_the_same_folder() {
        let scratch = Scratch::new();
        let root = scratch.0.join("documents");
        fs::create_dir_all(root.join("Manga/Other")).unwrap();
        for path in [
            "Manga/Volume 10.cbz",
            "Manga/Volume 2.cbt",
            "Manga/Volume 02.cbt",
            "Manga/Volume 1.cbz",
            "Manga/Volume 3.pdf",
            "Manga/Other/Volume 4.cbt",
            "Volume 8.cbz",
            "Volume 11.cbt",
        ] {
            fs::write(root.join(path), b"fixture").unwrap();
        }
        let mut library = Library::open(&root, &scratch.0.join("state.json")).unwrap();
        library.scan().unwrap();
        let entry = |path: &str| {
            library
                .entries()
                .iter()
                .find(|book| book.path == Path::new(path))
                .unwrap()
        };
        let next = |path| {
            library
                .next_comic_in_folder(entry(path))
                .map(|book| book.path.as_path())
        };

        assert_eq!(
            next("Manga/Volume 1.cbz"),
            Some(Path::new("Manga/Volume 02.cbt"))
        );
        assert_eq!(
            next("Manga/Volume 02.cbt"),
            Some(Path::new("Manga/Volume 2.cbt"))
        );
        assert_eq!(
            next("Manga/Volume 2.cbt"),
            Some(Path::new("Manga/Volume 10.cbz"))
        );
        assert_eq!(next("Volume 8.cbz"), Some(Path::new("Volume 11.cbt")));
        assert_eq!(next("Manga/Volume 10.cbz"), None);
        assert_eq!(next("Manga/Volume 3.pdf"), None);
        assert_eq!(next("Manga/Other/Volume 4.cbt"), None);
    }

    #[test]
    fn same_title_books_keep_separate_state_after_rename() {
        let scratch = Scratch::new();
        let root = scratch.0.join("documents");
        let state_path = scratch.0.join("reader.json");
        fs::create_dir_all(root.join("other")).unwrap();
        fs::write(root.join("Field Notes.epub"), EPUB3).unwrap();
        fs::write(root.join("other/Field Notes.EPUB"), EPUB2).unwrap();

        let mut library = Library::open(&root, &state_path).unwrap();
        library.scan().unwrap();
        assert_eq!(library.entries().len(), 2);
        let first = library.resolve(&root.join("Field Notes.epub")).unwrap();
        let second = library
            .resolve(Path::new("other/Field Notes.EPUB"))
            .unwrap();
        let id = library.identify(&first).unwrap();
        let other_id = library.identify(&second).unwrap();
        assert_eq!(id, 0xfb0c855bfa08f86a);
        assert_ne!(id, other_id);

        let anchor = Anchor {
            illustration: None,
            book: 1,
            chapter: "chapter.xhtml".into(),
            revision: 2,
            block: 3,
            offset: 4,
        };
        let position = ReadingPosition {
            chapter: 1,
            page: 3,
            anchor: Some(anchor.clone()),
        };
        let first_state = library.remember(&first, id, "Field Notes");
        first_state.position = Some(position.clone());
        first_state.preferences.font_size = Some(54.0);
        first_state.bookmarks.push(Bookmark {
            created_at: None,
            label: "Beginning".into(),
            position: position.clone(),
        });
        first_state.highlights.push(Highlight {
            color: Default::default(),
            created_at: None,
            style: Default::default(),
            selection: TextSelection {
                start: anchor.clone(),
                end: Anchor {
                    offset: 8,
                    ..anchor
                },
                text: "tide".into(),
            },
            note: Some("A passage to revisit".into()),
        });
        library
            .remember(&second, other_id, "Field Notes")
            .preferences
            .margin = Some(48);
        library
            .save_with(|path, bytes| {
                let temporary = path.with_extension("tmp");
                fs::write(&temporary, bytes)?;
                fs::rename(temporary, path)
            })
            .unwrap();

        fs::rename(root.join(&first.path), root.join("other/Renamed.epub")).unwrap();
        let mut library = Library::open(&root, &state_path).unwrap();
        library.scan().unwrap();
        assert_eq!(library.recents()[0].id, other_id);
        let saved = library.state(id).unwrap();
        let relocated = library.locate(saved).unwrap();
        assert_eq!(relocated.path, Path::new("other/Renamed.epub"));
        assert_eq!(library.identify(&relocated).unwrap(), id);
        assert_eq!(saved.position.as_ref(), Some(&position));
        assert_eq!(
            saved.preferences.apply_to(Settings::default()).font_size,
            54.0
        );
        assert_eq!(saved.bookmarks[0].position, position);
        assert_eq!(saved.highlights[0].selection.text, "tide");
        assert_eq!(
            saved.highlights[0].note.as_deref(),
            Some("A passage to revisit")
        );
        assert_eq!(
            library.state(other_id).unwrap().preferences.margin,
            Some(48)
        );
        assert!(library.state(other_id).unwrap().position.is_none());
        assert_eq!(
            library.remember(&relocated, id, "Field Notes").path,
            relocated.path
        );
        assert_eq!(library.recents()[0].id, id);
    }

    #[test]
    fn scans_and_identifies_loose_formats_with_the_reader_limits() {
        let scratch = Scratch::new();
        let root = scratch.0.join("documents");
        fs::create_dir(&root).unwrap();
        let text = b"<html><body><p>A tide.</p></body></html>";
        fs::write(root.join("Notes.TXT"), text).unwrap();
        fs::write(root.join("Notes.XHTML"), text).unwrap();
        fs::write(
            root.join("Notes.FB2"),
            b"<FictionBook><body><section><p>A tide.</p></section></body></FictionBook>",
        )
        .unwrap();
        File::create(root.join("large.txt"))
            .unwrap()
            .set_len(MAX_INPUT as u64 + 1)
            .unwrap();
        fs::write(root.join("other.dat"), text).unwrap();
        let mut library = Library::open(&root, &scratch.0.join("reader.json")).unwrap();
        library.scan().unwrap();
        assert_eq!(library.entries().len(), 3);
        let plain = library.resolve(Path::new("Notes.TXT")).unwrap();
        let xhtml = library.resolve(Path::new("Notes.XHTML")).unwrap();
        assert!(library.resolve(Path::new("large.txt")).is_err());
        let id = library.identify(&xhtml).unwrap();
        assert_ne!(id, library.identify(&plain).unwrap());
        let fb2 = library.resolve(Path::new("Notes.FB2")).unwrap();
        assert_ne!(id, library.identify(&fb2).unwrap());
        library.remember(&xhtml, id, "Story");

        fs::rename(root.join("Notes.XHTML"), root.join("Renamed.HTML")).unwrap();
        library.scan().unwrap();
        let moved = library.locate(library.state(id).unwrap()).unwrap();
        assert_eq!(moved.path, Path::new("Renamed.HTML"));
        assert_eq!(library.identify(&moved).unwrap(), id);
    }

    #[test]
    fn scans_fixed_formats_and_identifies_their_content_across_renames() {
        let scratch = Scratch::new();
        let root = scratch.0.join("documents");
        fs::create_dir(&root).unwrap();
        let input = b"fixed-page test data";
        for name in [
            "sample.PDF",
            "sample.JPG",
            "sample.PNG",
            "sample.DJV",
            "sample.GIF",
            "sample.BMP",
            "sample.PPM",
        ] {
            fs::write(root.join(name), input).unwrap();
        }
        fs::write(root.join("archive.CBZ"), EPUB3).unwrap();
        File::create(root.join("oversized.pdf"))
            .unwrap()
            .set_len(DocumentFormat::Pdf.max_bytes() + 1)
            .unwrap();

        let mut library = Library::open(&root, &scratch.0.join("reader.json")).unwrap();
        library.scan().unwrap();
        assert_eq!(library.entries().len(), 8);
        let mut ids = HashSet::new();
        for (name, format) in [
            ("sample.PDF", DocumentFormat::Pdf),
            ("sample.DJV", DocumentFormat::Djvu),
            ("sample.JPG", DocumentFormat::Jpeg),
            ("sample.PNG", DocumentFormat::Png),
            ("sample.GIF", DocumentFormat::Gif),
            ("sample.BMP", DocumentFormat::Bmp),
            ("sample.PPM", DocumentFormat::Pnm),
            ("archive.CBZ", DocumentFormat::Cbz),
        ] {
            let book = library.resolve(Path::new(name)).unwrap();
            assert_eq!(book.format, format);
            assert!(ids.insert(library.identify(&book).unwrap()));
        }
        for ext in ["PNM", "PBM", "PGM", "PPM", "PAM"] {
            let name = format!("image.{ext}");
            let format = DocumentFormat::for_path(Path::new(&name)).unwrap();
            assert_eq!(format, DocumentFormat::Pnm);
            assert!(format.is_still_image());
            assert_eq!(format.max_bytes(), MAX_IMAGE_BYTES);
        }
        let jpeg = library.resolve(Path::new("sample.JPG")).unwrap();
        let id = library.identify(&jpeg).unwrap();
        library.remember(&jpeg, id, "A photograph");
        fs::rename(root.join("sample.JPG"), root.join("renamed.jpeg")).unwrap();
        library.scan().unwrap();
        let moved = library.locate(library.state(id).unwrap()).unwrap();
        assert_eq!(moved.path, Path::new("renamed.jpeg"));
        assert_eq!(library.identify(&moved).unwrap(), id);
        let djvu = library.resolve(Path::new("sample.DJV")).unwrap();
        let djvu_id = library.identify(&djvu).unwrap();
        library.remember(&djvu, djvu_id, "A scan");
        fs::rename(root.join("sample.DJV"), root.join("renamed.djvu")).unwrap();
        library.scan().unwrap();
        let moved = library.locate(library.state(djvu_id).unwrap()).unwrap();
        assert_eq!(moved.path, Path::new("renamed.djvu"));
        assert_eq!(library.identify(&moved).unwrap(), djvu_id);
    }

    #[test]
    fn filtering_folds_titles_and_paths_and_reads_selected_metadata() {
        let scratch = Scratch::new();
        let root = scratch.0.join("docs");
        let state_path = scratch.0.join("reader.json");
        let folder = root.join("Schriften");
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("Đe\u{302}m Straße Σ.epub"), EPUB3).unwrap();
        fs::write(root.join("Other.epub"), EPUB2).unwrap();
        let mut library = Library::open(&root, &state_path).unwrap();
        library.scan().unwrap();
        assert_eq!(library.matching("   ").count(), 2);
        assert_eq!(library.matching("other Straße").count(), 0);
        let entry = library
            .matching("SCHRIFTEN ĐÊM STRASSE ς")
            .next()
            .unwrap()
            .clone();
        let meta = library.metadata(&entry).unwrap();
        assert_eq!(meta.title, "Field Notes at Dusk (EPUB 3)");
        assert_eq!(meta.author.as_deref(), Some("PrimitiveOS"));

        let id = library.identify(&entry).unwrap();
        library.remember(&entry, id, &meta.title);
        assert_eq!(library.matching("DUSK Schriften").count(), 1);
        library
            .save_with(|path, bytes| fs::write(path, bytes))
            .unwrap();
        let mut reopened = Library::open(&root, &state_path).unwrap();
        reopened.scan().unwrap();
        assert_eq!(reopened.matching("DUSK Schriften").count(), 1);
    }

    #[test]
    fn discovers_packed_fictionbook_and_uses_its_shared_size_limit() {
        let scratch = Scratch::new();
        let root = scratch.0.join("docs");
        fs::create_dir(&root).unwrap();
        let path = root.join("Shore.FB2.ZIP");
        let mut archive = zip::ZipWriter::new(File::create(&path).unwrap());
        archive
            .start_file("story.fb2", zip::write::SimpleFileOptions::default())
            .unwrap();
        archive.write_all(br#"<FictionBook xmlns="http://www.gribuser.ru/xml/fictionbook/2.0"><description><title-info><book-title>Shore Notes</book-title><author><first-name>Linh</first-name><last-name>Tran</last-name></author></title-info></description><body><section><p>A tide.</p></section></body></FictionBook>"#).unwrap();
        archive.finish().unwrap();
        let mut library = Library::open(&root, &scratch.0.join("reader.json")).unwrap();
        library.scan().unwrap();
        let entry = library.entries().first().unwrap();
        assert_eq!(entry.format, DocumentFormat::Fb2);
        assert_eq!(entry.format.max_bytes(), MAX_FB2_INPUT as u64);
        let metadata = library.metadata(entry).unwrap();
        assert_eq!(metadata.title, "Shore Notes");
        assert_eq!(metadata.author.as_deref(), Some("Linh Tran"));
        assert!(library.identify(entry).is_ok());
    }

    #[test]
    fn discovers_markdown_and_checks_kindle_container_contents_when_opened() {
        let scratch = Scratch::new();
        fs::write(scratch.0.join("Notebook.MARKDOWN"), b"# Tides\n\nA note.\n").unwrap();
        fs::write(scratch.0.join("Database.PDB"), b"not a Palm document").unwrap();
        fs::write(scratch.0.join("Invalid.AZW3"), b"invalid Kindle container").unwrap();
        let mut library = Library::open(&scratch.0, &scratch.0.join("reader.json")).unwrap();
        library.scan().unwrap();
        assert_eq!(library.entries().len(), 3);
        let markdown = library.resolve(Path::new("Notebook.MARKDOWN")).unwrap();
        let mobi = library.resolve(Path::new("Database.PDB")).unwrap();
        assert_eq!(markdown.format, DocumentFormat::Markdown);
        assert_eq!(mobi.format, DocumentFormat::Mobi);
        assert_eq!(mobi.format.max_bytes(), MAX_MOBI_BYTES as u64);
        assert!(library.metadata(&markdown).is_ok());
        assert!(library.metadata(&mobi).is_err());
        let id = library.identify(&markdown).unwrap();
        library.remember(&markdown, id, "Notebook");
        fs::rename(
            scratch.0.join("Notebook.MARKDOWN"),
            scratch.0.join("Tides.md"),
        )
        .unwrap();
        library.scan().unwrap();
        assert_eq!(
            library.locate(library.state(id).unwrap()).unwrap().path,
            Path::new("Tides.md")
        );
        let kf8 = library.resolve(Path::new("Invalid.AZW3")).unwrap();
        assert_eq!(kf8.format, DocumentFormat::Mobi);
        assert!(library.metadata(&kf8).is_err());
    }

    #[test]
    fn pdf_highlight_round_trips_and_rejects_invalid_or_oversized_marks() {
        let scratch = Scratch::new();
        let root = scratch.0.join("docs");
        let state_path = scratch.0.join("reader.json");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("Book.pdf"), b"PDF fixture").unwrap();
        let mut library = Library::open(&root, &state_path).unwrap();
        let entry = library.resolve(Path::new("Book.pdf")).unwrap();
        let id = library.identify(&entry).unwrap();
        let saved = library.remember(&entry, id, "Book");
        let bounds = PdfTextRect {
            x0: 0.2,
            y0: 0.3,
            x1: 0.5,
            y1: 0.4,
        };
        let mark = PdfHighlight {
            color: Default::default(),
            created_at: None,
            style: Default::default(),
            continuation: Vec::new(),
            page: 2,
            bounds: vec![bounds],
            text: "a passage".into(),
            note: Some("One note".into()),
        };
        saved.add_pdf_highlight(mark.clone()).unwrap();
        assert!(
            saved
                .add_pdf_highlight(PdfHighlight {
                    color: Default::default(),
                    bounds: vec![PdfTextRect {
                        x0: f32::NAN,
                        ..bounds
                    }],
                    ..mark.clone()
                })
                .is_err()
        );
        assert!(
            saved
                .add_pdf_highlight(PdfHighlight {
                    color: Default::default(),
                    bounds: vec![bounds; MAX_PDF_HIGHLIGHT_BOUNDS + 1],
                    ..mark.clone()
                })
                .is_err()
        );
        assert!(
            saved
                .add_pdf_highlight(PdfHighlight {
                    color: Default::default(),
                    note: Some("x".repeat(MAX_NOTE_BYTES + 1)),
                    ..mark.clone()
                })
                .is_err()
        );
        library
            .save_with(|path, bytes| fs::write(path, bytes))
            .unwrap();
        let reopened = Library::open(&root, &state_path).unwrap();
        assert_eq!(
            reopened.state(id).unwrap().pdf_highlights,
            vec![mark.clone()]
        );

        let saved = library.remember(&entry, id, "Book");
        saved
            .pdf_highlights
            .resize(MAX_PDF_HIGHLIGHTS, mark.clone());
        assert!(saved.add_pdf_highlight(mark).is_err());
        let old: BookState =
            serde_json::from_str(r#"{"id":7,"path":"old.pdf","title":"Old"}"#).unwrap();
        assert!(old.pdf_highlights.is_empty());
    }

    #[test]
    fn continued_pdf_highlights_validate_reading_order_quote_boundaries_and_old_records() {
        let bounds = vec![PdfTextRect {
            x0: 0.1,
            y0: 0.1,
            x1: 0.2,
            y1: 0.2,
        }];
        let mut single = PdfHighlight {
            color: Default::default(),
            created_at: None,
            style: Default::default(),
            page: 5,
            bounds: bounds.clone(),
            text: "First".into(),
            note: None,
            continuation: vec![],
        };
        let old = serde_json::to_value(&single).unwrap();
        assert!(old.get("continuation").is_none());
        assert_eq!(serde_json::from_value::<PdfHighlight>(old).unwrap(), single);
        single.text.push_str("\nÉcluse");
        single.continuation.push(HighlightPage {
            page: 6,
            bounds: bounds.clone(),
            text_offset: 6,
        });
        assert!(single.is_valid());
        assert_eq!(
            single.parts().map(|(_, _, part)| part).collect::<Vec<_>>(),
            ["First", "Écluse"]
        );
        for (page, offset) in [(5, 6), (7, 6), (6, 0), (6, 7), (6, usize::MAX)] {
            let mut invalid = single.clone();
            invalid.continuation[0].page = page;
            invalid.continuation[0].text_offset = offset;
            assert!(!invalid.is_valid());
        }
        single.continuation[0].bounds[0].x1 = 1.2;
        assert!(!single.is_valid());
    }

    #[test]
    fn fixed_display_options_default_for_old_books_and_validate_independently() {
        let legacy: FixedPreferences = serde_json::from_str(r#"{"width_fit":true}"#).unwrap();
        assert!(legacy.width_fit);
        assert!(!legacy.dither);
        assert_eq!(legacy.continuous_gap, 0);
        assert_eq!(legacy.scan_white_level, None);

        let options: FixedPreferences = serde_json::from_str(
            r#"{"dither":true,"scan_white_level":900,"continuous_gap":999999}"#,
        )
        .unwrap();
        assert!(options.dither);
        assert_eq!(options.continuous_gap, MAX_CONTINUOUS_GAP);
        assert_eq!(options.scan_white_level, None);
        let saved = FixedPreferences {
            scan_white_level: Some(235),
            ..options
        };
        let reopened: FixedPreferences =
            serde_json::from_slice(&serde_json::to_vec(&saved).unwrap()).unwrap();
        assert_eq!(reopened, saved);
        assert_eq!(
            FixedPreferences {
                scan_white_level: Some(120),
                ..saved
            }
            .normalized()
            .scan_white_level,
            None
        );
    }

    #[test]
    fn fixed_view_recovers_unusable_fields_and_preserves_progress() {
        let state: BookState = serde_json::from_str(
            r#"{
            "id":7,"path":"old.cbz","title":"Comic","position":{"chapter":0,"page":3},
            "fixed_preferences":{"zoom":99,"turns":259,"width_fit":true,"offset_x":1e40,
                                 "offset_y":null,"right_to_left":true,"scan_contrast":"higher",
                                 "crop":{"x0":0.1,"y0":0.2,"x1":0.11,"y1":0.8}}
        }"#,
        )
        .unwrap();
        assert_eq!(state.position.unwrap().page, 3);
        assert_eq!(
            state.fixed_preferences,
            FixedPreferences {
                zoom: 4,
                turns: 3,
                width_fit: true,
                height_fit: false,
                continuous: false,
                spread: false,
                scan_contrast: false,
                dither: false,
                continuous_gap: 0,
                scan_white_level: None,
                straighten_limit: 0,
                force_ocr: false,
                offset_x: MAX_FIXED_OFFSET,
                offset_y: 0.0,
                right_to_left: true,
                epub_spread: None,
                epub_right_to_left: None,
                crop: None,
            }
        );
        assert!(state.fixed_preferences.is_valid());
        let invalid = FixedPreferences {
            offset_y: f32::NAN,
            height_fit: true,
            ..state.fixed_preferences
        };
        assert!(!invalid.is_valid());
        assert_eq!(invalid.normalized(), state.fixed_preferences);
        let conflicting: FixedPreferences =
            serde_json::from_str(r#"{"width_fit":true,"height_fit":true}"#).unwrap();
        assert!(conflicting.width_fit);
        assert!(!conflicting.height_fit);
        let crop: FixedPreferences =
            serde_json::from_str(r#"{"crop":{"x0":0.1,"y0":0.2,"x1":0.9,"y1":0.8}}"#).unwrap();
        assert_eq!(crop.crop.unwrap().x1, 0.9);
        assert!(crop.is_valid());
        assert_eq!(
            serde_json::from_slice::<FixedPreferences>(&serde_json::to_vec(&crop).unwrap())
                .unwrap(),
            crop
        );
        let old: BookState =
            serde_json::from_str(r#"{"id":7,"path":"old.cbz","title":"Comic"}"#).unwrap();
        assert_eq!(old.fixed_preferences, FixedPreferences::default());
    }

    #[test]
    fn backup_preview_merge_and_repeat_preserve_current_progress_and_distinct_annotations() {
        let scratch = Scratch::new();
        let current = scratch.0.join("current");
        let imported = scratch.0.join("source");
        fs::create_dir(&current).unwrap();
        fs::create_dir(&imported).unwrap();
        fs::write(current.join("Current.epub"), EPUB3).unwrap();
        fs::write(imported.join("Old.epub"), EPUB3).unwrap();
        fs::write(imported.join("Another.pdf"), b"PDF for identity only").unwrap();
        let current_state = scratch.0.join("current.json");
        let mut target = Library::open(&current, &current_state).unwrap();
        let mut source = Library::open(&imported, &scratch.0.join("source.json")).unwrap();
        let target_entry = target.resolve(Path::new("Current.epub")).unwrap();
        let source_entry = source.resolve(Path::new("Old.epub")).unwrap();
        let id = target.identify(&target_entry).unwrap();
        assert_eq!(id, source.identify(&source_entry).unwrap());
        let anchor = |offset| Anchor {
            illustration: None,
            book: 1,
            chapter: "chapter.xhtml".into(),
            revision: 1,
            block: 0,
            offset,
        };
        let position = |page, offset| ReadingPosition {
            chapter: 0,
            page,
            anchor: Some(anchor(offset)),
        };
        let bookmark = |page, offset| Bookmark {
            created_at: None,
            label: "A bookmark".into(),
            position: position(page, offset),
        };
        let mark = |offset, note: Option<&str>| Highlight {
            color: Default::default(),
            created_at: None,
            style: Default::default(),
            selection: TextSelection {
                start: anchor(offset),
                end: anchor(offset + 1),
                text: "x".into(),
            },
            note: note.map(str::to_owned),
        };

        let saved = target.remember(&target_entry, id, "Current title");
        saved.position = Some(position(4, 40));
        saved.preferences.font_size = Some(54.0);
        saved.bookmarks.push(bookmark(1, 2));
        saved.bookmarks[0].created_at = Some(11);
        saved
            .highlights
            .extend([mark(2, Some("Keep this")), mark(4, None)]);

        let old = source.remember(&source_entry, id, "Imported title");
        old.position = Some(position(1, 10));
        old.preferences.font_size = Some(30.0);
        old.bookmarks.extend([bookmark(0, 2), bookmark(2, 8)]);
        old.highlights.extend([
            mark(2, Some("Conflict")),
            mark(4, Some("Fill blank")),
            mark(8, None),
        ]);
        old.bookmarks[0].created_at = Some(12);
        old.bookmarks[1].created_at = Some(13);
        old.highlights[1].created_at = Some(14);
        let another = source.resolve(Path::new("Another.pdf")).unwrap();
        let another_id = source.identify(&another).unwrap();
        let added = source.remember(&another, another_id, "Another document");
        added.position = Some(ReadingPosition {
            chapter: 0,
            page: 2,
            anchor: None,
        });
        added.fixed_preferences.width_fit = true;
        added
            .add_pdf_highlight(PdfHighlight {
                color: Default::default(),
                created_at: Some(15),
                style: Default::default(),
                continuation: Vec::new(),
                page: 2,
                bounds: vec![PdfTextRect {
                    x0: 0.1,
                    y0: 0.2,
                    x1: 0.3,
                    y1: 0.4,
                }],
                text: "Another quote".into(),
                note: Some("PDF note".into()),
            })
            .unwrap();

        let backup = source.backup_bytes().unwrap();
        let preview = target.preview_backup(&backup).unwrap();
        assert_eq!(
            preview.summary(),
            BackupSummary {
                new_books: 1,
                existing_books: 1,
                bookmarks_added: 1,
                highlights_added: 1,
                pdf_highlights_added: 1,
                notes_added: 2,
                timestamps_added: 1,
                duplicates: 3,
                skipped: 0,
                note_conflicts: 1,
                ..BackupSummary::default()
            }
        );
        assert_eq!(target.recents().len(), 1);
        assert!(!current_state.exists());
        let expected = preview.summary();
        assert_eq!(target.merge_backup(preview).unwrap(), expected);
        let saved = target.state(id).unwrap();
        assert_eq!(saved.title, "Current title");
        assert_eq!(saved.path, Path::new("Current.epub"));
        assert_eq!(saved.position, Some(position(4, 40)));
        assert_eq!(saved.preferences.font_size, Some(54.0));
        assert_eq!(saved.bookmarks.len(), 2);
        assert_eq!(saved.bookmarks.iter().map(|m| m.created_at).collect::<Vec<_>>(), [Some(11), Some(13)]);
        assert_eq!(saved.highlights.iter().map(|m| m.created_at).collect::<Vec<_>>(), [None, Some(14), None]);
        assert_eq!(target.state(another_id).unwrap().pdf_highlights[0].created_at, Some(15));
        assert_eq!(saved.highlights[0].note.as_deref(), Some("Keep this"));
        assert_eq!(saved.highlights[1].note.as_deref(), Some("Fill blank"));
        assert!(!current.join("Another.pdf").exists());
        assert_eq!(
            target
                .state(another_id)
                .unwrap()
                .position
                .as_ref()
                .unwrap()
                .page,
            2
        );

        let before = target.bytes().unwrap();
        let repeated = target.preview_backup(&backup).unwrap();
        assert_eq!(repeated.summary().new_books, 0);
        assert_eq!(repeated.summary().notes_added, 0);
        assert_eq!(repeated.summary().timestamps_added, 0);
        target.merge_backup(repeated).unwrap();
        assert_eq!(target.bytes().unwrap(), before);
        target
            .save_with(|path, bytes| fs::write(path, bytes))
            .unwrap();
        let reopened = Library::open(&current, &current_state).unwrap();
        assert_eq!(reopened.bytes().unwrap(), before);
    }

    #[test]
    fn backup_rejects_bad_versions_and_ignores_paths_outside_the_library() {
        let scratch = Scratch::new();
        let mut library = Library::open(&scratch.0, &scratch.0.join("state.json")).unwrap();
        assert!(
            library
                .preview_backup(br#"{"version":2,"books":[]}"#)
                .is_err()
        );
        assert!(
            library
                .preview_backup(&vec![0; MAX_BACKUP_BYTES as usize + 1])
                .is_err()
        );
        let preview = library
            .preview_backup(
                br#"{"version":1,"books":[{"id":7,"path":"../unsafe.pdf","title":"Unsafe"}]}"#,
            )
            .unwrap();
        assert_eq!(preview.summary().skipped, 1);
        library.merge_backup(preview).unwrap();
        assert!(library.recents().is_empty());
    }

    #[test]
    fn older_state_infers_reading_status_and_initializes_organization() {
        let scratch = Scratch::new();
        let state_path = scratch.0.join("library.json");
        fs::write(&state_path, br#"[
            {"id":1,"path":"Positioned.epub","title":"Positioned","position":{"chapter":2,"page":3}},
            {"id":2,"path":"Unopened.epub","title":"Unopened"},
            {"id":3,"path":"Future.epub","title":"Future","reading_status":"future_status","position":{"chapter":0}}
        ]"#).unwrap();

        let library = Library::open(&scratch.0, &state_path).unwrap();
        assert_eq!(library.state(1).unwrap().status(), ReadingStatus::Reading);
        assert_eq!(library.state(2).unwrap().status(), ReadingStatus::Unread);
        assert_eq!(library.state(3).unwrap().status(), ReadingStatus::Reading);
        assert_eq!(library.recently_read().count(), 2);
        assert_eq!(library.favorites().count(), 0);
        assert!(library.collections().is_empty());
        library
            .save_with(|path, bytes| fs::write(path, bytes))
            .unwrap();

        let reopened = Library::open(&scratch.0, &state_path).unwrap();
        assert_eq!(
            reopened.state(1).unwrap().position.as_ref().unwrap().page,
            3
        );
        assert_eq!(reopened.state(2).unwrap().status(), ReadingStatus::Unread);
    }

    #[test]
    fn tracks_unopened_favorites_and_persists_overlapping_collections_through_rename() {
        let scratch = Scratch::new();
        let state_path = scratch.0.join("library.json");
        fs::write(scratch.0.join("Started.epub"), EPUB2).unwrap();
        fs::write(scratch.0.join("Unopened.epub"), EPUB3).unwrap();
        let mut library = Library::open(&scratch.0, &state_path).unwrap();
        let started = library.resolve(Path::new("Started.epub")).unwrap();
        let unopened = library.resolve(Path::new("Unopened.epub")).unwrap();
        let started_id = library.identify(&started).unwrap();
        let unopened_id = library.identify(&unopened).unwrap();
        let book = library.remember(&started, started_id, "First book");
        book.position = Some(ReadingPosition {
            chapter: 0,
            page: 2,
            anchor: None,
        });
        book.set_status(ReadingStatus::Finished);
        let book = library.track(&unopened, unopened_id);
        book.favorite = true;
        book.set_status(ReadingStatus::OnHold);

        assert_eq!(
            library
                .recently_read()
                .map(|book| book.id)
                .collect::<Vec<_>>(),
            [started_id]
        );
        assert_eq!(
            library.favorites().map(|book| book.id).collect::<Vec<_>>(),
            [unopened_id]
        );
        library.create_collection(" Straße ").unwrap();
        assert_eq!(
            library.create_collection("STRASSE").unwrap_err().kind(),
            io::ErrorKind::AlreadyExists
        );
        assert!(library.create_collection(" ").is_err());
        assert!(library.create_collection("An\ninvalid name").is_err());
        assert!(
            library
                .create_collection(&"a".repeat(MAX_COLLECTION_NAME_CHARS + 1))
                .is_err()
        );
        library.create_collection("Weekend").unwrap();
        assert!(
            library
                .set_collection_membership("strasse", unopened_id, true)
                .unwrap()
        );
        assert!(
            !library
                .set_collection_membership("Straße", unopened_id, true)
                .unwrap()
        );
        assert!(
            library
                .set_collection_membership("Weekend", unopened_id, true)
                .unwrap()
        );
        assert!(
            library
                .set_collection_membership("Weekend", started_id, true)
                .unwrap()
        );
        assert!(
            library
                .set_collection_membership("Weekend", 7, true)
                .is_err()
        );
        assert!(library.rename_collection("strasse", "Weekend").is_err());
        assert!(
            library
                .rename_collection("strasse", "Favorites to revisit")
                .unwrap()
        );
        library
            .save_with(|path, bytes| fs::write(path, bytes))
            .unwrap();

        fs::rename(
            scratch.0.join("Unopened.epub"),
            scratch.0.join("Moved.epub"),
        )
        .unwrap();
        let mut reopened = Library::open(&scratch.0, &state_path).unwrap();
        reopened.scan().unwrap();
        let new_path = reopened
            .locate(reopened.state(unopened_id).unwrap())
            .unwrap();
        assert_eq!(new_path.path, Path::new("Moved.epub"));
        assert_eq!(
            reopened.track(&new_path, unopened_id).status(),
            ReadingStatus::OnHold
        );
        assert!(reopened.state(unopened_id).unwrap().favorite);
        assert!(
            reopened
                .collection("Favorites to revisit")
                .unwrap()
                .contains(unopened_id)
        );
        assert_eq!(
            reopened.collection("weekend").unwrap().book_ids(),
            [unopened_id, started_id]
        );
        assert!(reopened.remove_collection("Weekend"));
        assert!(scratch.0.join("Moved.epub").is_file());
        assert!(
            reopened
                .collection("Favorites to revisit")
                .unwrap()
                .contains(unopened_id)
        );
        assert_eq!(
            reopened.state(started_id).unwrap().status(),
            ReadingStatus::Finished
        );
    }

    #[test]
    fn backup_combines_collection_members_without_replacing_current_status() {
        let scratch = Scratch::new();
        let state_path = scratch.0.join("library.json");
        fs::write(
            &state_path,
            br#"{
           "version":1,
           "books":[{"id":1,"path":"First.epub","title":"First","reading_status":"finished"}],
           "collections":[{"name":"Trips","book_ids":[1,1,47]},{"name":"TRIPS","book_ids":[1]},
                          {"name":"  ","book_ids":[1]}]
        }"#,
        )
        .unwrap();
        let mut library = Library::open(&scratch.0, &state_path).unwrap();
        assert_eq!(library.collections().len(), 1);
        assert_eq!(library.collections()[0].book_ids(), [1]);
        let bytes = br#"{
           "version":1,
           "books":[{"id":1,"path":"First.epub","title":"Old","favorite":true,"reading_status":"reading"},
                    {"id":2,"path":"Second.epub","title":"Second","favorite":true,"reading_status":"on_hold"}],
           "collections":[{"name":"TRIPS","book_ids":[2,2,99]},{"name":"Empty","book_ids":[]}]
        }"#;
        let preview = library.preview_backup(bytes).unwrap();
        assert_eq!(preview.summary().collections_added, 1);
        assert_eq!(preview.summary().memberships_added, 1);
        assert_eq!(preview.summary().skipped, 1);
        assert_eq!(library.collections().len(), 1);
        let expected = preview.summary();
        assert_eq!(library.merge_backup(preview).unwrap(), expected);
        assert_eq!(library.collection("Trips").unwrap().name, "Trips");
        assert_eq!(library.collection("Trips").unwrap().book_ids(), [1, 2]);
        assert_eq!(library.state(1).unwrap().status(), ReadingStatus::Finished);
        assert!(!library.state(1).unwrap().favorite);
        assert_eq!(library.state(2).unwrap().status(), ReadingStatus::OnHold);
        assert!(library.state(2).unwrap().favorite);
        let exported = library.backup_bytes().unwrap();
        let repeated = library.preview_backup(&exported).unwrap();
        assert_eq!(repeated.summary().collections_added, 0);
        assert_eq!(repeated.summary().memberships_added, 0);
    }

    #[cfg(unix)]
    #[test]
    fn discovery_and_handoff_reject_hidden_files_and_symlink_escapes() {
        use std::os::unix::fs::symlink;

        let scratch = Scratch::new();
        let root = scratch.0.join("documents");
        let outside = scratch.0.join("private");
        fs::create_dir_all(root.join(".hidden")).unwrap();
        fs::create_dir(&outside).unwrap();
        fs::write(root.join("visible.EPUB"), EPUB3).unwrap();
        fs::write(root.join(".hidden/story.epub"), EPUB3).unwrap();
        fs::write(outside.join("story.epub"), EPUB3).unwrap();
        symlink(&outside, root.join("external")).unwrap();
        symlink(outside.join("story.epub"), root.join("linked.epub")).unwrap();

        let mut library = Library::open(&root, &scratch.0.join("reader.json")).unwrap();
        library.scan().unwrap();
        assert_eq!(library.entries().len(), 1);
        assert_eq!(library.entries()[0].path, Path::new("visible.EPUB"));
        for bad_path in [
            ".hidden/story.epub",
            "external/story.epub",
            "linked.epub",
            "../private/story.epub",
        ] {
            assert!(library.resolve(Path::new(bad_path)).is_err());
        }
        assert!(library.resolve(&outside.join("story.epub")).is_err());
    }
}
