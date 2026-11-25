mod cjk_width;
mod first_line;
#[cfg(test)]
mod hanging_punctuation_tests;
#[cfg(test)]
mod hyphenation_options_tests;
mod inline_blocks;
#[cfg(test)]
mod inline_blocks_tests;
mod note_inline;
mod short_words;
#[cfg(test)]
mod special_glyph_tests;
#[cfg(test)]
mod table_caption_tests;
mod table_vertical;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::ops::Range;
use std::sync::Arc;

use crate::book_style::{
    CssLength, ImageLength, LigatureExtras, RubyPosition, Style, TextDecoration, TextTransform,
    VariantCaps, VariantEastAsian, VariantNumeric, VariantPosition, WhiteSpace,
};
use crate::comic::ImagePage;
use crate::epub::Book;
use crate::flow_projection::OwnedProjection;
use crate::flow_view::{ChapterFlow, LayoutBlock};
use crate::gray_glyph::GlyphCache;
use crate::publisher_padding::{BoxArea, TextBoxCandidate, text_box_candidates};
use crate::source::{Block, EmbeddedImage, Source};
use crate::svg_image;
use crate::{Line, Result, Settings, image_reader};
use image::{DynamicImage, GrayImage, Luma, imageops::FilterType};
use parley::{
    Affinity, Alignment, AlignmentOptions, BoundingBox, Cursor, FontContext, FontFamily,
    FontWeight, GenericFamily, Language, Layout, LayoutContext, LineBreak, LineHeight,
    OverflowWrap, PositionedLayoutItem, Selection, SoftHyphenAction, StyleProperty, TextWrapMode,
    WordBreak,
    setting::{FontFeature, FontVariation, Tag},
};

const MAX_TEXT_BYTES: usize = 512 * 1024;
const MAX_LINES: usize = 30_000;
const MAX_IMAGE_BOXES: usize = 2_048;
const MAX_DISTINCT_IMAGE_READS: usize = 512;
const MAX_IMAGE_PIXELS: usize = 8 * 1024 * 1024;
const MAX_RUBY_BASE_BYTES: usize = 192;

fn declared_box_height(style: &Style, width: u32, font: f32) -> Option<f32> {
    let length = match style.image_height? {
        ImageLength::Pixels(px) => CssLength::Pixels(px),
        ImageLength::Physical(px) => CssLength::Physical(px),
        _ => return None,
    };
    let pixels = length.box_pixels(width, font);
    (pixels.is_finite()
        && pixels > 0.0
        && style.borders.is_none()
        && matches!(style.image_max_height, None | Some(ImageLength::Auto))
        && style.page_breaks == Default::default()
        && style
            .padding
            .iter()
            .chain(&style.vertical_padding)
            .all(|edge| edge.box_pixels(width, font) == 0.0))
    .then_some(pixels)
}

fn keep_fitted_edges(lines: &mut [Line], outer: Range<f32>, settings: Settings) {
    let page_height = settings.text_height() as f32;
    match lines {
        [only] => only.keep_page_extension(
            [
                (only.top - outer.start).max(0.0),
                (outer.end - only.bottom).max(0.0),
            ],
            page_height,
        ),
        [first, .., last] => {
            first.keep_page_extension([(first.top - outer.start).max(0.0), 0.0], page_height);
            last.keep_page_extension([0.0, (outer.end - last.bottom).max(0.0)], page_height);
        }
        [] => {}
    }
}

const KERNING_OFF: [FontFeature; 2] = [
    FontFeature::new(Tag::new(b"kern"), 0),
    FontFeature::new(Tag::new(b"vkrn"), 0),
];
const NO_COMMON_LIGATURES: [FontFeature; 2] = [
    FontFeature::new(Tag::new(b"liga"), 0),
    FontFeature::new(Tag::new(b"clig"), 0),
];
const KERNING_OFF_NO_COMMON_LIGATURES: [FontFeature; 4] = [
    FontFeature::new(Tag::new(b"kern"), 0),
    FontFeature::new(Tag::new(b"vkrn"), 0),
    FontFeature::new(Tag::new(b"liga"), 0),
    FontFeature::new(Tag::new(b"clig"), 0),
];
const KERNING_FAST: [FontFeature; 23] = [
    FontFeature::new(Tag::new(b"kern"), 0),
    FontFeature::new(Tag::new(b"vkrn"), 0),
    FontFeature::new(Tag::new(b"mark"), 0),
    FontFeature::new(Tag::new(b"mkmk"), 0),
    FontFeature::new(Tag::new(b"curs"), 0),
    FontFeature::new(Tag::new(b"locl"), 0),
    FontFeature::new(Tag::new(b"liga"), 0),
    FontFeature::new(Tag::new(b"rlig"), 0),
    FontFeature::new(Tag::new(b"clig"), 0),
    FontFeature::new(Tag::new(b"ccmp"), 0),
    FontFeature::new(Tag::new(b"calt"), 0),
    FontFeature::new(Tag::new(b"rclt"), 0),
    FontFeature::new(Tag::new(b"rvrn"), 0),
    FontFeature::new(Tag::new(b"ltra"), 0),
    FontFeature::new(Tag::new(b"ltrm"), 0),
    FontFeature::new(Tag::new(b"rtla"), 0),
    FontFeature::new(Tag::new(b"rtlm"), 0),
    FontFeature::new(Tag::new(b"frac"), 0),
    FontFeature::new(Tag::new(b"numr"), 0),
    FontFeature::new(Tag::new(b"dnom"), 0),
    FontFeature::new(Tag::new(b"rand"), 0),
    FontFeature::new(Tag::new(b"trak"), 0),
    FontFeature::new(Tag::new(b"vert"), 0),
];
const KERNING_GOOD: [FontFeature; 22] = [
    FontFeature::new(Tag::new(b"kern"), 1),
    FontFeature::new(Tag::new(b"mark"), 1),
    FontFeature::new(Tag::new(b"mkmk"), 1),
    FontFeature::new(Tag::new(b"curs"), 1),
    FontFeature::new(Tag::new(b"locl"), 1),
    FontFeature::new(Tag::new(b"liga"), 0),
    FontFeature::new(Tag::new(b"rlig"), 0),
    FontFeature::new(Tag::new(b"clig"), 0),
    FontFeature::new(Tag::new(b"ccmp"), 0),
    FontFeature::new(Tag::new(b"calt"), 0),
    FontFeature::new(Tag::new(b"rclt"), 0),
    FontFeature::new(Tag::new(b"rvrn"), 0),
    FontFeature::new(Tag::new(b"ltra"), 0),
    FontFeature::new(Tag::new(b"ltrm"), 0),
    FontFeature::new(Tag::new(b"rtla"), 0),
    FontFeature::new(Tag::new(b"rtlm"), 0),
    FontFeature::new(Tag::new(b"frac"), 0),
    FontFeature::new(Tag::new(b"numr"), 0),
    FontFeature::new(Tag::new(b"dnom"), 0),
    FontFeature::new(Tag::new(b"rand"), 0),
    FontFeature::new(Tag::new(b"trak"), 0),
    FontFeature::new(Tag::new(b"vert"), 0),
];

fn kerning_features(mode: crate::FontKerningMode) -> Option<&'static [FontFeature]> {
    match mode {
        crate::FontKerningMode::Off => Some(&KERNING_OFF),
        crate::FontKerningMode::Fast => Some(&KERNING_FAST),
        crate::FontKerningMode::Good => Some(&KERNING_GOOD),
        crate::FontKerningMode::Best => None,
    }
}

fn shaping_features(
    mode: crate::FontKerningMode,
    common_ligatures: bool,
) -> &'static [FontFeature] {
    if common_ligatures {
        return kerning_features(mode).unwrap_or(&[]);
    }
    match mode {
        crate::FontKerningMode::Best => &NO_COMMON_LIGATURES,
        crate::FontKerningMode::Off => &KERNING_OFF_NO_COMMON_LIGATURES,
        // These modes already disable common ligatures; do not reenable any
        // of the other features their kerning policy has deliberately disabled.
        crate::FontKerningMode::Fast => &KERNING_FAST,
        crate::FontKerningMode::Good => &KERNING_GOOD,
    }
}

fn shaping_with_variants(
    mode: crate::FontKerningMode,
    common_ligatures: bool,
    ligature_extras: LigatureExtras,
    caps: VariantCaps,
    numeric: VariantNumeric,
    east_asian: VariantEastAsian,
    position: VariantPosition,
    historical: bool,
) -> Vec<FontFeature> {
    let mut features = shaping_features(mode, common_ligatures).to_vec();
    if let Some(enabled) = ligature_extras.contextual {
        if !matches!(
            mode,
            crate::FontKerningMode::Fast | crate::FontKerningMode::Good
        ) {
            features.push(FontFeature::new(Tag::new(b"calt"), u16::from(enabled)));
        }
    }
    for (enabled, tag) in [
        (ligature_extras.discretionary, b"dlig"),
        (ligature_extras.historical, b"hlig"),
    ] {
        if let Some(enabled) = enabled {
            features.push(FontFeature::new(Tag::new(tag), u16::from(enabled)));
        }
    }
    let tags: &[&[u8; 4]] = match caps {
        VariantCaps::Normal => &[],
        VariantCaps::Small => &[b"smcp"],
        VariantCaps::AllSmall => &[b"smcp", b"c2sc"],
        VariantCaps::Petite => &[b"pcap"],
        VariantCaps::AllPetite => &[b"pcap", b"c2pc"],
        VariantCaps::Unicase => &[b"unic"],
        VariantCaps::Titling => &[b"titl"],
    };
    features.extend(tags.iter().map(|tag| FontFeature::new(Tag::new(*tag), 1)));
    for (flag, tag) in [
        (1, b"lnum"),
        (2, b"onum"),
        (4, b"pnum"),
        (8, b"tnum"),
        (16, b"zero"),
        (32, b"ordn"),
        (64, b"frac"),
        (128, b"afrc"),
    ] {
        // Fast and Good explicitly disable `frac` as part of their existing
        // shaping policy; do not undo that choice with a duplicate feature.
        if numeric.0 & flag != 0
            && !(flag == 64
                && matches!(
                    mode,
                    crate::FontKerningMode::Fast | crate::FontKerningMode::Good
                ))
        {
            features.push(FontFeature::new(Tag::new(tag), 1));
        }
    }
    for (flag, tag) in [
        (1, b"jp78"),
        (2, b"jp83"),
        (4, b"jp04"),
        (8, b"smpl"),
        (16, b"trad"),
        (32, b"fwid"),
        (64, b"pwid"),
        (128, b"ruby"),
    ] {
        if east_asian.0 & flag != 0 {
            features.push(FontFeature::new(Tag::new(tag), 1));
        }
    }
    match position {
        VariantPosition::Normal => {}
        VariantPosition::Super => features.push(FontFeature::new(Tag::new(b"sups"), 1)),
        VariantPosition::Sub => features.push(FontFeature::new(Tag::new(b"subs"), 1)),
    }
    if historical {
        features.push(FontFeature::new(Tag::new(b"hist"), 1));
    }
    features
}

fn apply_kerning<B: parley::Brush>(
    builder: &mut parley::RangedBuilder<'_, B>,
    mode: crate::FontKerningMode,
) {
    builder.set_fast_kerning(mode == crate::FontKerningMode::Fast);
    if let Some(features) = kerning_features(mode) {
        builder.push_default(StyleProperty::FontFeatures(features.into()));
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PaintBrush {
    pub(crate) decoration: TextDecoration,
    pub(crate) link_ink: Option<u8>,
    pub(crate) painted: bool,
}

impl Default for PaintBrush {
    fn default() -> Self {
        Self {
            decoration: Default::default(),
            link_ink: None,
            painted: true,
        }
    }
}

pub(crate) struct Paragraph {
    pub(crate) column: Range<f32>,
    pub(crate) signed_margin: bool,
    max_bottom: f32,
    pub(crate) key: usize,
    pub(crate) x: f32,
    pub(crate) y: f32,
    text_bottom: f32,
    bottom: f32,
    background: Option<u8>,
    fitted: Option<crate::publisher_padding::fit_content::Placed>,
    pub(crate) layout: Layout<PaintBrush>,
    ruby: Vec<RubyOverlay>,
    flow: Option<Arc<OwnedProjection>>,
    pub(crate) inline_text: Option<inline_blocks::InlineText>,
    pub(crate) joined_note: Option<Arc<note_inline::JoinedNote>>,
}

impl Paragraph {
    pub(crate) fn source_offset(
        &self,
        offset: usize,
        affinity: crate::flow_projection::Affinity,
    ) -> Option<usize> {
        match &self.flow {
            Some(flow) => flow.source_offset(offset, affinity),
            None => Some(offset),
        }
    }

    pub(crate) fn source_location(
        &self,
        offset: usize,
        affinity: crate::flow_projection::Affinity,
    ) -> Option<(usize, usize)> {
        match &self.joined_note {
            Some(note) => note.source_location(offset, affinity),
            None => Some((self.key, self.source_offset(offset, affinity)?)),
        }
    }

    pub(crate) fn visible_text_offset(
        &self,
        offset: usize,
        affinity: crate::flow_projection::Affinity,
    ) -> Option<usize> {
        match &self.inline_text {
            Some(boxed) => boxed.visible_offset(offset, affinity),
            None => Some(offset),
        }
    }

    pub(crate) fn text_box_at(&self, x: f32, y: f32) -> Option<&inline_blocks::TextBox> {
        self.inline_text
            .as_ref()?
            .boxes
            .iter()
            .find(|child| child.hit(x, y))
    }

    fn clip_columns(settings: Settings, columns: Range<f32>) -> Range<usize> {
        let clip =
            |x: f32| (settings.left_margin() as f32 + x).clamp(0.0, settings.width as f32) as usize;
        clip(columns.start)..clip(columns.end)
    }

    fn background_columns(&self, settings: Settings) -> Range<usize> {
        let margin = settings.left_margin() as usize;
        if self.signed_margin {
            Self::clip_columns(settings, self.column.clone())
        } else {
            margin + self.column.start as usize..margin + self.column.end as usize
        }
    }

    pub(crate) fn paint_columns(&self, settings: Settings) -> Range<usize> {
        if self.fitted.is_some() || self.signed_margin {
            Self::clip_columns(settings, self.hit_columns())
        } else {
            let columns = settings.text_columns();
            let [left, right] = self.layout.hanging_punctuation_overflow();
            columns.start.saturating_sub(left.ceil() as usize)
                ..(columns.end + right.ceil() as usize).min(settings.width as usize)
        }
    }

    pub(crate) fn hit_columns(&self) -> Range<f32> {
        self.fitted.map_or_else(
            || {
                let [left, right] = self.layout.hanging_punctuation_overflow();
                (self.column.start - left)..(self.column.end + right)
            },
            |fitted| fitted.text_left as f32..fitted.text_left as f32 + fitted.text_width as f32,
        )
    }

    pub(crate) fn allows_margin_hits(&self) -> bool {
        self.signed_margin || self.layout.hanging_punctuation_overflow() != [0.0; 2]
    }

    fn text_top(&self) -> f32 {
        self.y
            + self
                .inline_text
                .as_ref()
                .and_then(inline_blocks::InlineText::initial_bounds)
                .map_or(0., |bounds| bounds.start.min(0.))
    }

    fn painted_top(&self) -> f32 {
        self.text_top().min(
            self.y
                - self
                    .fitted
                    .map_or(0.0, |fit| fit.vertical_insets()[0] as f32),
        )
    }

    fn paint_fitted_border(&self, page: &mut [u8], settings: Settings, range: &Range<f32>) {
        let Some(boxed) = self.fitted else {
            return;
        };
        let [top, right, bottom, left] = boxed.strokes;
        let column = self.column.clone();
        let y = (self.painted_top() - range.start).floor();
        let end = (self.bottom - range.start).ceil();
        let leading = column.start + left.width as f32;
        let trailing = column.end - right.width as f32;
        let stride = settings.width as usize;
        let height = page.len() / stride;
        let rows = |at: f32| (at.max(0.0) as usize).min(height);
        for (x, vertical, stroke) in [
            (column.clone(), y..y + top.width as f32, top),
            (column.clone(), end - bottom.width as f32..end, bottom),
            (
                column.start..leading,
                y + top.width as f32..end - bottom.width as f32,
                left,
            ),
            (
                trailing..column.end,
                y + top.width as f32..end - bottom.width as f32,
                right,
            ),
        ] {
            if let Some((gray, _)) = stroke.gray
                && stroke.width != 0
            {
                let x = Self::clip_columns(settings, x);
                for row in rows(vertical.start)..rows(vertical.end) {
                    page[row * stride + x.start..row * stride + x.end].fill(gray);
                }
            }
        }
    }
}

struct PaintedMarker {
    x: f32,
    top: f32,
    bottom: f32,
    paint: MarkerPaint,
    painted: bool,
}

enum MarkerPaint {
    Text { y: f32, layout: Layout<PaintBrush> },
    Shape(crate::list_marker::MarkerKind),
}
struct RubyOverlay {
    line: usize,
    x: f32,
    y: f32,
    top: f32,
    bottom: f32,
    layout: Layout<PaintBrush>,
    painted: bool,
}

struct RubyCandidate {
    bytes: Range<usize>,
    layout: Layout<PaintBrush>,
    line_height: f32,
    painted: bool,
    under: bool,
}

/// Find the nearest non-overlapping placement that remains in the column.
/// If shifting is needed, the annotation's center must stay over its base.
fn ruby_position(
    base: BoundingBox,
    annotation_width: f32,
    column_width: f32,
    line: usize,
    painted: bool,
    top: f32,
    bottom: f32,
    overlays: &[RubyOverlay],
) -> Option<f32> {
    let (left, right) = (base.x0 as f32, base.x1 as f32);
    if !left.is_finite()
        || !right.is_finite()
        || !annotation_width.is_finite()
        || annotation_width < 0.0
        || annotation_width > column_width
        || right <= left
    {
        return None;
    }
    let preferred =
        ((left + right - annotation_width) * 0.5).clamp(0.0, column_width - annotation_width);
    if !painted {
        return Some(preferred);
    }
    let mut occupied: Vec<_> = overlays
        .iter()
        .filter(|other| {
            other.painted
                && other.line == line
                && other.top < bottom + 1.0
                && other.bottom + 1.0 > top
        })
        .map(|other| (other.x, other.x + other.layout.width()))
        .collect();
    if occupied
        .iter()
        .all(|&(start, end)| preferred >= end + 1.0 || preferred + annotation_width + 1.0 <= start)
    {
        return Some(preferred);
    }
    occupied.sort_by(|a, b| a.0.total_cmp(&b.0));
    let minimum = (left - annotation_width * 0.5).max(0.0);
    let maximum = (right - annotation_width * 0.5).min(column_width - annotation_width);
    if minimum > maximum {
        return None;
    }
    let mut cursor = minimum;
    let mut best: Option<f32> = None;
    for (start, end) in occupied {
        let upper = maximum.min(start - annotation_width - 1.0);
        if cursor <= upper {
            let x = preferred.clamp(cursor, upper);
            if best.is_none_or(|old| (x - preferred).abs() < (old - preferred).abs()) {
                best = Some(x);
            }
        }
        cursor = cursor.max(end + 1.0);
        if cursor > maximum {
            break;
        }
    }
    if cursor <= maximum {
        let x = preferred.clamp(cursor, maximum);
        if best.is_none_or(|old| (x - preferred).abs() < (old - preferred).abs()) {
            best = Some(x);
        }
    }
    best
}

struct Illustration {
    block: usize,
    source_index: usize,
    painted: bool,
    full_line: bool,
    path: Option<String>,
    link: Option<String>,
    label: String,
    // Inline boxes provide x once the paragraph's line breaks and alignment are known.
    x: Option<usize>,
    y: f32,
    // Layout row ownership can differ from the canonical owner of the image
    // when a footnote joins artwork and text from different source blocks.
    render_line: Option<(usize, f32)>,
    pixels: Arc<GrayImage>,
    display: (u32, u32),
}

/// A bounded chapter layout with a limited publisher style cascade and direct
/// grayscale glyph drawing. Source positions are shared with the Blitz probe.
pub(crate) struct ParleyChapter {
    paragraphs: Vec<Paragraph>,
    illustrations: Vec<Illustration>,
    markers: Vec<PaintedMarker>,
    table_frames: Vec<crate::table_borders::Frame>,
    pub(crate) in_page_table_extents:
        HashMap<crate::table_layout::TableKey, crate::footnotes::in_page::TableCopy>,
    note_table_fit_failed: bool,
    padded_backgrounds: Vec<crate::vertical_padding::Fill>,
    physical_break_runs: Vec<crate::publisher_breaks::PositionedRun>,
    publisher_height_spaces: Vec<Range<f32>>,
    fitted_height_tails: Vec<crate::publisher_padding::fit_content::HeightTail>,
    float_geometry: bool,
    limitations: Vec<String>,
    glyphs: GlyphCache,
}

impl ParleyChapter {
    #[cfg(feature = "kindle")]
    pub(crate) fn for_each_shaped_font(&self, mut visit: impl FnMut(u64)) {
        let mut layout_fonts = |layout: &Layout<PaintBrush>| {
            for line in layout.lines() {
                for run in line.runs() {
                    visit(run.font().data.id());
                }
            }
        };
        for paragraph in &self.paragraphs {
            layout_fonts(&paragraph.layout);
            for ruby in &paragraph.ruby {
                layout_fonts(&ruby.layout);
            }
            if let Some(inline) = &paragraph.inline_text {
                for child in &inline.boxes {
                    layout_fonts(&child.layout);
                }
            }
        }
        for marker in &self.markers {
            if let MarkerPaint::Text { layout, .. } = &marker.paint {
                layout_fonts(layout);
            }
        }
    }

    #[cfg(feature = "kindle")]
    pub(crate) fn release_raster_cache(&mut self) {
        self.glyphs.release_masks();
    }

    pub fn new(
        book: &mut Book,
        href: &str,
        source: &Source,
        fonts: FontContext,
        settings: Settings,
        styles: &[Style],
        user_rules: &crate::user_hyphenation::Rules,
    ) -> Result<(Self, Vec<Line>, f32)> {
        let flow = ChapterFlow::from_styles(source, styles)?;
        Self::with_flow(
            book, href, source, fonts, settings, styles, user_rules, &flow, None,
        )
    }

    pub(crate) fn with_flow(
        book: &mut Book,
        href: &str,
        source: &Source,
        fonts: FontContext,
        settings: Settings,
        styles: &[Style],
        user_rules: &crate::user_hyphenation::Rules,
        flow: &ChapterFlow,
        footnotes: Option<&crate::footnotes::in_page::Targets>,
    ) -> Result<(Self, Vec<Line>, f32)> {
        Self::with_flow_mode(
            book, href, source, fonts, settings, styles, user_rules, flow, false, footnotes,
        )
    }

    pub(crate) fn with_source_order(
        book: &mut Book,
        href: &str,
        source: &Source,
        fonts: FontContext,
        settings: Settings,
        styles: &[Style],
        user_rules: &crate::user_hyphenation::Rules,
        flow: &ChapterFlow,
    ) -> Result<(Self, Vec<Line>, f32)> {
        Self::with_flow_mode(
            book, href, source, fonts, settings, styles, user_rules, flow, true, None,
        )
    }

    fn with_flow_mode(
        book: &mut Book,
        href: &str,
        source: &Source,
        fonts: FontContext,
        settings: Settings,
        styles: &[Style],
        user_rules: &crate::user_hyphenation::Rules,
        flow: &ChapterFlow,
        source_order: bool,
        footnotes: Option<&crate::footnotes::in_page::Targets>,
    ) -> Result<(Self, Vec<Line>, f32)> {
        let float_plan = crate::float_layout::Plan::prepare(source, styles, settings);
        let mut layout = |fonts, float_plan| {
            Self::layout_flow(
                book,
                href,
                source,
                fonts,
                settings,
                styles,
                user_rules,
                flow,
                source_order,
                float_plan,
                footnotes,
            )
        };
        let Some(plan) = &float_plan else {
            return layout(fonts, None);
        };
        let trial = layout(fonts.clone(), Some(plan)).and_then(|(chapter, mut lines, extent)| {
            lines.sort_by(|a, b| a.top.total_cmp(&b.top));
            crate::Pagination::with_breaks_capped(
                &lines,
                settings.text_height() as f32,
                extent,
                &[],
                settings.chapter_height_limit(),
            )?;
            Ok((chapter, lines, extent))
        });
        match trial {
            Ok((chapter, lines, _)) if !chapter.uses_float_geometry() => {
                drop((chapter, lines));
                layout(fonts, None)
            }
            Err(error)
                if error.downcast_ref::<crate::PageContentTooTall>().is_some()
                    || error
                        .downcast_ref::<crate::float_layout::TooManyFloats>()
                        .is_some() =>
            {
                let (mut chapter, lines, extent) = layout(fonts, None)?;
                let reason = if error.is::<crate::float_layout::TooManyFloats>() {
                    "Float geometry exceeded the placement limit; the chapter uses ordinary flow"
                } else {
                    "Float geometry could not fit a reading page; the chapter uses ordinary flow"
                };
                chapter.limitations.push(reason.into());
                Ok((chapter, lines, extent))
            }
            result => result,
        }
    }

    fn layout_flow(
        book: &mut Book,
        href: &str,
        source: &Source,
        mut fonts: FontContext,
        settings: Settings,
        styles: &[Style],
        user_rules: &crate::user_hyphenation::Rules,
        flow: &ChapterFlow,
        source_order: bool,
        float_plan: Option<&crate::float_layout::Plan>,
        footnotes: Option<&crate::footnotes::in_page::Targets>,
    ) -> Result<(Self, Vec<Line>, f32)> {
        if source
            .blocks
            .iter()
            .map(|block| block.text.len())
            .sum::<usize>()
            > MAX_TEXT_BYTES
        {
            return Err("The chapter exceeds the Parley text limit".into());
        }

        let mut chapter = Self {
            paragraphs: Vec::new(),
            illustrations: Vec::new(),
            markers: Vec::new(),
            table_frames: Vec::new(),
            in_page_table_extents: HashMap::new(),
            note_table_fit_failed: false,
            padded_backgrounds: Vec::new(),
            physical_break_runs: Vec::new(),
            publisher_height_spaces: Vec::new(),
            fitted_height_tails: Vec::new(),
            float_geometry: false,
            limitations: Vec::new(),
            glyphs: GlyphCache::with_hinting(
                settings.logical_font_size.is_some(),
                settings.font_hinting,
                styles
                    .first()
                    .map_or_else(Default::default, |style| style.font_weights.clone()),
            ),
        };
        let mut context = LayoutContext::new();
        let has_generated = styles.iter().any(|style| style.generated.is_some());
        let mut lines: Vec<Line> = Vec::new();
        let mut remaining_inline_boxes = inline_blocks::MAX_CHAPTER_BOXES;
        let note_tables = footnotes.map_or_else(HashMap::new, |notes| notes.native_tables(source));
        let in_page_tables: HashSet<_> = footnotes
            .into_iter()
            .flat_map(|_| source.table_cells.iter().map(|(_, cell)| cell.table))
            .collect();
        let mut y = 0.0_f32;
        let mut image_budget = MAX_IMAGE_PIXELS;
        let fallback = Arc::new(GrayImage::from_pixel(
            settings.text_width().min(64),
            36,
            Luma([225]),
        ));
        let group_insets = list_insets(source, styles, settings, flow);
        let outer_width = settings.text_width();
        let table_flow = if source_order {
            chapter.limitations.push(
                "Table rows could not fit a reading page; the table uses readable source order"
                    .into(),
            );
            crate::table_layout::TableFlow::source_order(source)
        } else {
            match crate::table_layout::TableFlow::new(source, styles) {
                Ok(flow) if settings.repair_css_tables => flow.with_repaired_css(source, styles),
                Ok(flow) => flow,
                Err(error)
                    if error.downcast_ref::<crate::table_layout::TableFlowError>()
                        == Some(&crate::table_layout::TableFlowError::HintReprojection) =>
                {
                    chapter.limitations.push(error.to_string());
                    crate::table_layout::TableFlow::source_order(source)
                }
                Err(error) => return Err(error),
            }
        };
        let mut borders = crate::table_borders::Borders::read_with_settings(
            source,
            styles,
            settings,
            &table_flow,
        );
        let mut flow_error = None;
        let mut publisher_boxes = crate::publisher_padding::layout_with_flow(
            source,
            styles,
            outer_width,
            settings.font_size,
            borders.as_ref().filter(|b| b.has_geometry()),
            &table_flow,
            settings,
            float_plan.map(|plan| &plan.tables),
            |block| {
                if styles[block.element].flow.suppressed {
                    return (0, 0);
                }
                let mut input = match flow.layout_block(source, block) {
                    Ok(input) => input,
                    Err(error) => {
                        flow_error = Some(error);
                        return (0, 0);
                    }
                };
                if styles[block.element].first_letter.is_some()
                    && let Ok(view) = flow.view(source, block)
                {
                    input.first_letter = crate::book_style::pseudo::LetterRun::resolve(
                        block,
                        block.element,
                        &view,
                        styles,
                    );
                }
                input.first_line = block.table_cell.is_none();
                let indent = LineIndent::resolve(
                    &styles[block.element],
                    outer_width,
                    outer_width,
                    settings.font_size,
                    0,
                );
                let layout = make_paragraph(
                    &mut context,
                    &mut fonts,
                    &input,
                    None,
                    settings,
                    styles,
                    &[],
                    &[],
                    false,
                    indent,
                    &Default::default(),
                    None,
                );
                let widths = layout.calculate_content_widths();
                (widths.min.ceil() as u32, widths.max.ceil() as u32)
            },
        );
        if let Some(error) = flow_error {
            return Err(error);
        }
        if let (Some(b), Some(boxes)) = (&mut borders, &publisher_boxes) {
            b.resolve_padding(boxes, styles, settings);
        }
        let mut joined = match footnotes {
            Some(targets) if float_plan.is_none() && targets.inlines_all() => {
                note_inline::plan(source, styles, flow, targets, &note_tables)?
            }
            _ => BTreeMap::new(),
        };
        let column_for = |block: &Block, boxes: Option<&[BoxArea]>| {
            if let Some(boxes) = boxes {
                return boxes[block.element].cell;
            }

            table_flow.block_cell(block).filter(|cell| {
                outer_width as f32 / f32::from(cell.columns) >= 2.5 * settings.font_size
            })
        };
        let captions = crate::table_layout::captions::Plan::new(
            source,
            &table_flow,
            styles,
            |block| column_for(block, publisher_boxes.as_deref()),
            &joined
                .iter()
                .map(|(&first, note)| first..note.end_index)
                .collect::<Vec<_>>(),
        );
        let mut floating_tables = float_plan
            .zip(publisher_boxes.as_deref())
            .map_or_else(Vec::new, |(plan, boxes)| {
                plan.tables
                    .resolve(source, styles, boxes, outer_width, &captions)
            })
            .into_iter()
            .peekable();
        let order = captions.order();
        let source_index = |slot| crate::table_layout::captions::source_index(order, slot);
        if let Some(borders) = &mut borders {
            for band in captions.bands() {
                borders.place_caption(band.table, band.bottom);
            }
        }
        let vertical_padding = crate::vertical_padding::layout_with_flow(
            source,
            styles,
            publisher_boxes.as_deref(),
            settings,
            &table_flow,
            order,
        );
        let block_margins = crate::block_margins::before_blocks(
            source,
            styles,
            publisher_boxes.as_deref(),
            settings,
            order,
        );
        let height_candidates = if settings.publisher_block_height {
            text_box_candidates(source, styles, |style| {
                matches!(
                    style.image_height,
                    Some(ImageLength::Pixels(_) | ImageLength::Physical(_))
                )
            })
        } else {
            Vec::new()
        };
        let empty_geometry = crate::publisher_breaks::plan_empty_geometry(source, styles, settings);
        let mut empty_fills = Vec::new();
        chapter.float_geometry = floating_tables.peek().is_some()
            || float_plan.is_some_and(|plan| plan.floats.iter().any(Option::is_some));
        let mut float_flow = crate::float_layout::Flow::default();
        let mut float_fills = Vec::new();
        let bounds_for = |block: &Block, boxes: Option<&[BoxArea]>| {
            if let Some(boxes) = boxes {
                let area = boxes[block.element];
                return (area.x, area.width);
            }

            column_for(block, boxes).map_or((0, outer_width), |cell| {
                cell.bounds(outer_width, (settings.font_size * 0.22).round() as u32)
            })
        };
        let gutter_for = |block: &Block, boxes: Option<&[BoxArea]>| {
            block
                .list_item
                .and_then(|item| group_insets.get(&item.group))
                .copied()
                .unwrap_or(0.0)
                .round()
                .clamp(0.0, bounds_for(block, boxes).1 as f32 * 0.49) as u32
        };
        let signed_margin_candidates = if settings.publisher_signed_horizontal_margins {
            crate::publisher_padding::horizontal_margins::signed_candidates(
                source,
                styles,
                settings,
                outer_width,
            )
        } else {
            Vec::new()
        };
        let fit_content_candidates = crate::publisher_padding::fit_content::candidates(
            source,
            styles,
            settings,
            outer_width,
        );
        // Share the bounded grayscale cache across distinct renderings. Duplicate icons
        // reuse one preview, while large later illustrations still receive space.
        let mut variants = HashSet::new();
        let mut image_boxes = 0;
        for source_block in source
            .blocks
            .iter()
            .filter(|block| !block.images.is_empty())
        {
            if styles[source_block.element].flow.suppressed {
                continue;
            }
            let block = flow.layout_block(source, source_block)?;
            let boxes = publisher_boxes.as_deref();
            let image_width = bounds_for(&block, boxes).1 - gutter_for(&block, boxes);
            for image in block.images.iter() {
                if image_boxes == MAX_IMAGE_BOXES {
                    break;
                }
                let style = &styles[image.element];
                variants.insert(
                    ImageSizing::new(
                        &image,
                        settings,
                        style,
                        !block.text.is_empty() && !style.image_full_line,
                        image_width,
                    )
                    .key(&image.original().href),
                );
                image_boxes += 1;
            }
        }
        // A joined image can use the note container's width instead of its
        // original paragraph's size or list gutter, producing a new variant.
        let joined_images = joined.values().map(|note| note.images.len()).sum::<usize>();
        let image_variants = (variants.len() + joined_images).min(MAX_DISTINCT_IMAGE_READS);
        let mut image_cache = HashMap::new();
        let normal_gap = settings.font_size * 0.32;
        let mut trailing_gap = normal_gap;
        let mut row_flow = crate::table_layout::RowFlow::default();
        let mut positions = crate::table_borders::Positions::default();
        let mut caption_start = (0.0, 0);
        let mut caption_frames = Vec::new();
        let mut caption_fills = Vec::new();
        let mut inline_note_frames = Vec::new();
        let mut inline_note_fills = Vec::new();
        let mut table_alignment = table_vertical::Pass::new(&table_flow, styles, !source_order);
        let wrapper_backgrounds = publisher_boxes.as_ref().is_some_and(|boxes| {
            boxes.iter().zip(styles).any(|(area, style)| {
                area.sized_wrapper && style.flow.painted && style.background.is_some()
            })
        });
        let mut fill_positions = (wrapper_backgrounds
            || vertical_padding.is_some()
                && styles.iter().any(|s| {
                    s.background.is_some()
                        && s.vertical_padding
                            .iter()
                            .any(|v| v.pixels(outer_width, settings.font_size) > 0.)
                }))
        .then(|| Vec::with_capacity(source.blocks.len()));
        let mut fitted_padding_elements = HashSet::new();
        let mut last_cell = None;
        // Marker ownership is the LI, not whichever paragraph happens to be
        // first in canonical extraction. Prefer its first retained content;
        // if all children disappear, keep an empty marker strut at its old host.
        let mut marker_hosts = HashMap::new();
        let mut content_hosts = HashSet::new();
        for block in &source.blocks {
            let Some(item) = block.list_item else {
                continue;
            };
            if flow.ordinal(item).is_none() || styles[item.element].flow.suppressed {
                continue;
            }
            marker_hosts.entry(item.element).or_insert(block.key);
            if !content_hosts.contains(&item.element) && !styles[block.element].flow.suppressed {
                let input = flow.layout_block(source, block)?;
                if !input.text.is_empty() || !input.images.is_empty() {
                    marker_hosts.insert(item.element, block.key);
                    content_hosts.insert(item.element);
                }
            }
        }
        let has_inline_bidi = source.elements.iter().any(|element| {
            matches!(element.tag.as_str(), "bdi" | "bdo")
                || element.direction_hint.is_some() && !crate::source::is_block_tag(&element.tag)
        });
        let mut joined_until = 0;
        let mut floating_table = None;
        for slot in 0..source.blocks.len() {
            let index = source_index(slot);
            let source_block = &source.blocks[index];
            if index < joined_until {
                if let Some(points) = &mut fill_positions {
                    points.push((y, y));
                }
                continue;
            }
            let marker_host = source_block
                .list_item
                .is_some_and(|item| marker_hosts.get(&item.element) == Some(&source_block.key));
            if styles[source_block.element].flow.suppressed && !marker_host {
                if let Some(points) = &mut fill_positions {
                    points.push((y, y));
                }
                continue;
            }
            let projection = flow
                .view(source, source_block)
                .map_err(|error| error.to_string())?;
            let caption = captions.placement(index);
            let mut input = flow.layout_block(source, source_block)?;
            if let Some(caption) = caption
                && source.elements[input.element].anonymous
            {
                input.element = caption.element;
            }
            if has_inline_bidi && input.images.is_empty() {
                input.bidi_controls = crate::bidi_controls::for_text(
                    input.text,
                    &input.styled_text,
                    input.element,
                    &source.elements,
                    styles,
                )
                .unwrap_or_default();
            }
            input.first_letter = crate::book_style::pseudo::LetterRun::resolve(
                source_block,
                input.element,
                &projection,
                styles,
            );
            input.first_line = source_block.table_cell.is_none() && caption.is_none();
            let note = joined.remove(&index);
            let note_layout = note.as_ref().map(|note| Arc::clone(&note.layout));
            let mut note_table = None;
            let note_typography;
            let (flattened, note_images) = match note.zip(note_layout.as_ref()) {
                Some((note, layout)) => {
                    joined_until = note.end_index;
                    note_table = note.table;
                    note_typography = note.generated;
                    let mut fragment = input
                        .text_fragment(note.element, &layout.text, note.runs)
                        .with_joined_ruby(note.ruby);
                    fragment.generated = &note_typography;
                    (Some(fragment), note.images)
                }
                None => (None, Vec::new()),
            };
            let block = flattened.as_ref().unwrap_or(&input);
            let cell = column_for(block, publisher_boxes.as_deref());
            if table_alignment.ends_before(cell) {
                y = table_alignment.finish(
                    row_flow.end(y),
                    &mut chapter,
                    &mut lines,
                    fill_positions.as_deref_mut().unwrap_or_default(),
                    borders.as_ref(),
                    &mut positions,
                );
                row_flow = Default::default();
            }
            if floating_tables
                .peek()
                .is_some_and(|table| table.start() == slot)
            {
                let table = floating_tables.next().unwrap();
                y = float_flow.leave_closed_boxes(slot, row_flow.end(y));
                row_flow = Default::default();
                let placement = table.place(
                    &float_flow,
                    publisher_boxes.as_deref_mut().unwrap(),
                    outer_width,
                    y,
                );
                y = placement.top;
                floating_table = Some((table.end(), placement));
            }
            let boxes = publisher_boxes.as_deref();
            let (natural_column, natural_width) = bounds_for(block, boxes);
            let float = float_plan
                .filter(|_| floating_table.is_none())
                .and_then(|plan| plan.floats[index]);
            if let Some(plan) = float_plan.filter(|_| floating_table.is_none()) {
                use crate::float_layout::{Clear, simple_box};
                y = float_flow.leave_closed_boxes(slot, y);
                let style = &styles[block.element];
                let separate = cell.is_some()
                    || source_block.table_cell.is_some()
                    || block.list_item.is_some()
                    || !block.images.is_empty()
                    || !block.ruby.is_empty()
                    || float.is_none() && style.background.is_some()
                    || natural_column != 0
                    || natural_width != outer_width
                    || style
                        .text_indent
                        .box_pixels(outer_width, settings.font_size)
                        != 0.0
                    || !simple_box(style, outer_width, settings.font_size);
                if float.is_none() {
                    let clear = if separate {
                        Clear::BOTH
                    } else {
                        plan.clears[index]
                    };
                    y = float_flow.clear(clear, y);
                }
            }
            let padding = vertical_padding
                .as_ref()
                .map_or_else(Default::default, |g| g[slot]);
            let margin_space = block_margins
                .as_ref()
                .map_or_else(Default::default, |g| g[slot]);
            let last = slot.max(joined_until.saturating_sub(1));
            let gap = if caption.is_some()
                || block_margins
                    .as_ref()
                    .is_some_and(|spaces| spaces[last].suppress_gap_after)
            {
                0.0
            } else {
                normal_gap
            };
            if float.is_none() && floating_table.is_none() {
                trailing_gap = gap;
            }
            let margin = margin_space.before;
            if padding.before_grid > 0 {
                y = row_flow.end(y) + padding.before_grid as f32;
            }
            if margin > 0.0 && cell.is_some() {
                y = row_flow.end(y) + margin;
            }
            y = row_flow.enter(cell, y);
            if margin != 0.0 && cell.is_none() {
                // A large negative margin may overlap its previous box, but
                // cannot reverse the layout's line and paragraph origins.
                let floor = lines.last().map_or(0.0, |line| line.top + 1.0);
                let floor = chapter.paragraphs.last().map_or(floor, |p| floor.max(p.y));
                y = (y + margin).max(if margin < 0.0 { floor } else { 0.0 });
            }
            if let Some(run) = empty_geometry
                .as_ref()
                .and_then(|runs| runs[index].as_ref())
                && cell.is_none()
                && float.is_none()
                && float_flow.finish(y) <= y
            {
                let (after, positioned) =
                    run.place(block.element, y, outer_width, &mut empty_fills);
                y = after;
                chapter.physical_break_runs.push(positioned);
            }
            let start_of_cell = y;
            let caption_decoration = caption.map(|caption| {
                crate::table_layout::captions::Decoration::new(
                    caption,
                    &styles[caption.element],
                    publisher_boxes.as_deref(),
                    borders.as_ref(),
                    settings,
                )
            });
            if let (Some(caption), Some(decoration)) = (caption, &caption_decoration)
                && caption.first
            {
                let [stroke, inset] = decoration.leading(caption);
                y += stroke;
                caption_start = (y, lines.len());
                y += inset;
            }
            let decorated = cell.and_then(|cell| Some((cell, borders.as_ref()?.cells.get(&cell)?)));
            if let Some((c, _)) = decorated
                && Some(c) != last_cell
            {
                y += borders.as_ref().unwrap().vertical(c).0 as f32;
            }
            last_cell = cell;
            y += padding.before as f32;
            let flow_y = y;
            if let Some(cell) = cell {
                table_alignment.begin(
                    cell,
                    &chapter,
                    lines.len(),
                    fill_positions.as_ref().map_or(0, Vec::len),
                    start_of_cell,
                    flow_y,
                    padding.before_grid > 0,
                );
            }
            let (column_x, column_width) = if let Some(spec) = float {
                let clear = float_plan.unwrap().clears[index];
                let (x, top) = float_flow.place(spec, clear, outer_width, y);
                y = top;
                (x, spec.width)
            } else {
                (natural_column, natural_width)
            };
            let gutter = if note_layout.is_some() {
                0
            } else {
                gutter_for(block, boxes)
            };
            let image_width = column_width - gutter;
            let marker = block
                .list_item
                .filter(|item| {
                    note_layout.is_none() && marker_hosts.get(&item.element) == Some(&block.key)
                })
                .and_then(|item| {
                    let style = &styles[item.element];
                    let size = style.font_pixels(settings);
                    Some((style.marker.label(flow.ordinal(item)?)?, style, size))
                });
            let literal = marker.as_ref().and_then(|(label, style, size)| {
                let prefix = crate::list_marker::authored_prefix(&block.text, label)?;
                Some(
                    marker_layout(&mut context, &mut fonts, prefix, style, settings, *size)
                        .full_width(),
                )
            });
            let hanging = literal.unwrap_or(0.0).round().clamp(0.0, gutter as f32) as u32;
            let width = column_width - gutter + hanging;
            let block_y = y;
            let mut fitted_extent = None;
            let mut rtl = matches!(styles[block.element].direction, parley::BaseDirection::Rtl);
            let mut marker_baseline = None;
            let mut float_height = 0.0_f32;
            let first_image = chapter.illustrations.len();
            let original_images = note_layout
                .is_none()
                .then(|| input.images.iter().map(|image| (source_block.key, image)));
            let image_count = original_images
                .as_ref()
                .map_or(note_images.len(), |_| input.images.len());
            let available = MAX_IMAGE_BOXES.saturating_sub(first_image).min(image_count);
            let mut inline_boxes = Vec::with_capacity(available);
            for (owner, image) in note_images
                .into_iter()
                .chain(original_images.into_iter().flatten())
                .take(available)
            {
                let inline = !block.text.is_empty();
                let style = &styles[image.element];
                let sizing = ImageSizing::new(
                    &image,
                    settings,
                    style,
                    inline && !style.image_full_line,
                    image_width,
                );
                let key = sizing.key(&image.original().href);
                let fallback_preview = || (None, Arc::clone(&fallback), fallback.dimensions());
                let (path, pixels, display) = if let Some(cached) = image_cache.get(&key) {
                    Clone::clone(cached)
                } else if image_cache.len() < image_variants {
                    let share = image_budget / (image_variants - image_cache.len());
                    let result = match load_image(book, href, &image, settings, sizing, share) {
                        Some((path, pixels, display)) => {
                            image_budget -= pixels.as_raw().len();
                            (Some(path), Arc::new(pixels), display)
                        }
                        None => fallback_preview(),
                    };
                    image_cache.insert(key, result.clone());
                    result
                } else {
                    fallback_preview()
                };
                if inline {
                    inline_boxes.push(parley::InlineBox {
                        id: chapter.illustrations.len() as u64,
                        index: image.offset,
                        kind: if style.image_full_line {
                            parley::InlineBoxKind::Block
                        } else {
                            parley::InlineBoxKind::InFlow
                        },
                        width: (display.0 as f32
                            + if image.space_after && !style.image_full_line {
                                settings.font_size * 0.25
                            } else {
                                0.0
                            })
                        .min(width as f32),
                        height: display.1 as f32,
                        baseline: None,
                        vertical_align: if style.image_full_line {
                            parley::VerticalAlign::TOP
                        } else {
                            style.image_align
                        },
                    });
                }
                chapter.illustrations.push(Illustration {
                    block: owner,
                    source_index: image.source_index,
                    painted: image.painted,
                    full_line: style.image_full_line,
                    path,
                    link: image.link.clone(),
                    label: image.label.clone(),
                    x: None,
                    y: 0.0,
                    render_line: None,
                    pixels,
                    display,
                });
            }
            if !block.text.is_empty() {
                let style = &styles[block.element];
                let publication_box = if float.is_none() {
                    publisher_boxes.as_ref().map(|boxes| boxes[block.element])
                } else {
                    None
                };
                let (text_x, text_width, indent_base) =
                    publication_box
                        .as_ref()
                        .map_or((column_x, width, image_width), |area| {
                            let (x, expanded) = area.paragraph_bounds_for_text(
                                style.direction,
                                block.text,
                                style,
                                settings.font_size,
                            );
                            (
                                x,
                                width + expanded - area.width,
                                area.outer_width.saturating_sub(gutter),
                            )
                        });
                let contour = if float.is_none() && floating_table.is_none() {
                    float_flow.contour(y, outer_width, settings.font_size * 4.0)
                } else {
                    None
                };
                let (placed_box, fitted) = if float.is_some() || contour.is_some() {
                    (None, None)
                } else if fit_content_candidates.get(block.element)
                    == Some(&TextBoxCandidate::Simple)
                {
                    // CREngine measures intrinsic percentage indents against one em.
                    let em = style.font_pixels(settings).floor() as u32;
                    let indent =
                        LineIndent::resolve(style, em, column_width, settings.font_size, 0);
                    let intrinsic = make_paragraph(
                        &mut context,
                        &mut fonts,
                        block,
                        None,
                        settings,
                        styles,
                        &[],
                        &[],
                        false,
                        indent,
                        &Default::default(),
                        None,
                    )
                    .calculate_content_widths()
                    .max;
                    let (parent_x, parent_width) = source.elements[block.element]
                        .parent
                        .and_then(|index| publisher_boxes.as_ref().map(|boxes| boxes[index]))
                        .map_or((0, outer_width), |area| (area.x, area.width));
                    let fitted = crate::publisher_padding::fit_content::inset(
                        parent_x,
                        parent_width,
                        style,
                        settings,
                        crate::publisher_padding::paragraph_is_rtl(style.direction, block.text),
                        intrinsic,
                    );
                    (fitted.map(|area| (area.left, area.width)), fitted)
                } else if signed_margin_candidates.get(block.element)
                    == Some(&TextBoxCandidate::Simple)
                {
                    (
                        crate::publisher_padding::horizontal_margins::signed_inset(
                            column_x,
                            column_width,
                            style,
                            settings,
                            crate::publisher_padding::paragraph_is_rtl(style.direction, block.text),
                        ),
                        None,
                    )
                } else {
                    (None, None)
                };
                let fitted_with_padding = fitted.filter(|area| area.vertical_padding != [0; 2]);
                let (text_x, text_width, indent_base) = if let Some(area) = fitted {
                    y += area.vertical_insets()[0] as f32;
                    if fitted_with_padding.is_some() {
                        y -= padding.own[0] as f32;
                    }
                    (area.text_left as f32, area.text_width, area.width)
                } else {
                    placed_box.map_or((text_x as f32, text_width, indent_base), |(left, width)| {
                        (left as f32, width, width)
                    })
                };
                let cjk_grid = if style.cjk_tailored && contour.is_none() && inline_boxes.is_empty()
                {
                    cjk_width::grid(
                        text_width,
                        style.font_pixels(settings),
                        settings.cjk_width_percent,
                    )
                } else {
                    None
                };
                let (text_x, text_width, unit) = cjk_grid
                    .map_or((text_x, text_width, None), |(left, width, unit)| {
                        (text_x + left as f32, width, Some(unit))
                    });
                let indent = LineIndent::with_grid(
                    style,
                    indent_base,
                    text_width,
                    settings.font_size,
                    hanging,
                    unit,
                );
                let mut plan = None;
                if settings.inline_block_mode != crate::InlineBlockMode::Ignore
                    && note_layout.is_none()
                    && cell.is_none()
                    && float.is_none()
                    && contour.is_none()
                    && !style.preformatted_spaces
                {
                    plan = inline_blocks::plan(
                        &mut context,
                        &mut fonts,
                        block,
                        source,
                        styles,
                        text_width - indent.common,
                        settings,
                        user_rules,
                        remaining_inline_boxes,
                    );
                }
                let mut generated = if plan.is_none() && note_layout.is_none() && has_generated {
                    inline_blocks::generated::Plan::new(block, &projection, styles)
                } else {
                    None
                };
                let width = text_width - indent.common;
                let mut tabular = note_table.as_ref().and_then(|table| {
                    inline_blocks::note_table::plan(
                        &mut context,
                        &mut fonts,
                        block,
                        table,
                        width,
                        settings,
                        styles,
                    )
                });
                if note_table.is_some() && tabular.is_none() {
                    chapter.note_table_fit_failed = true;
                }
                let mut initial = if tabular.is_none()
                    && plan.is_none()
                    && generated.is_none()
                    && note_layout.is_none()
                    && contour.is_none()
                    && fitted.is_none()
                    && cell.is_none()
                    && float.is_none()
                    && indent.amount == 0.0
                    && indent.common == 0
                    && inline_boxes.is_empty()
                {
                    inline_blocks::initial::Plan::new(
                        &mut context,
                        &mut fonts,
                        block,
                        styles,
                        settings,
                        width,
                    )
                } else {
                    None
                };
                let (layout, ruby) = if let Some(table) = &mut tabular {
                    let generated: Vec<_> = block
                        .generated
                        .iter()
                        .flat_map(|run| {
                            table.text.layout_spans(&run.bytes).map(|bytes| {
                                crate::book_style::pseudo::Run {
                                    bytes,
                                    style: run.style,
                                    host: run.host,
                                }
                            })
                        })
                        .collect();
                    let mut outer = block.text_fragment(
                        block.element,
                        &table.text.text,
                        std::mem::take(&mut table.runs),
                    );
                    outer.generated = &generated;
                    layout_with_images(
                        &mut context,
                        &mut fonts,
                        &outer,
                        width,
                        settings,
                        styles,
                        std::slice::from_ref(&table.item),
                        indent,
                        user_rules,
                        contour,
                    )
                } else if let Some(boxed) = &mut plan {
                    let mut outer = block.text_fragment(
                        block.element,
                        &boxed.text.text,
                        std::mem::take(&mut boxed.runs),
                    );
                    outer.first_letter = block.first_letter.as_ref().and_then(|run| {
                        boxed.text.layout_spans(&run.bytes).next().map(|bytes| {
                            crate::book_style::pseudo::LetterRun {
                                bytes,
                                ..run.clone()
                            }
                        })
                    });
                    remaining_inline_boxes -= boxed.items.len();
                    layout_with_images(
                        &mut context,
                        &mut fonts,
                        &outer,
                        text_width - indent.common,
                        settings,
                        styles,
                        &boxed.items,
                        indent,
                        user_rules,
                        contour,
                    )
                } else if let Some(generated) = &mut generated {
                    let mut outer = block.text_fragment(
                        block.element,
                        &generated.text.text,
                        std::mem::take(&mut generated.runs),
                    );
                    outer.generated = &generated.typography;
                    outer.first_line = block.first_line;
                    outer.first_letter = block.first_letter.as_ref().and_then(|run| {
                        generated.text.layout_spans(&run.bytes).next().map(|bytes| {
                            crate::book_style::pseudo::LetterRun {
                                bytes,
                                ..run.clone()
                            }
                        })
                    });
                    layout_with_images(
                        &mut context,
                        &mut fonts,
                        &outer,
                        text_width - indent.common,
                        settings,
                        styles,
                        &[],
                        indent,
                        user_rules,
                        contour,
                    )
                } else if let Some(initial) = &mut initial {
                    let outer = block.text_fragment(
                        block.element,
                        &initial.text.text,
                        std::mem::take(&mut initial.runs),
                    );
                    layout_with_images(
                        &mut context,
                        &mut fonts,
                        &outer,
                        width,
                        settings,
                        styles,
                        &[],
                        indent,
                        user_rules,
                        Some(initial.contour),
                    )
                } else {
                    layout_with_images(
                        &mut context,
                        &mut fonts,
                        block,
                        text_width - indent.common,
                        settings,
                        styles,
                        &inline_boxes,
                        indent,
                        user_rules,
                        contour,
                    )
                };
                let mut inline_text = plan
                    .map(|boxed| boxed.text)
                    .or_else(|| generated.map(|generated| generated.text))
                    .or_else(|| initial.map(|initial| initial.text));
                let initial_bounds = inline_text
                    .as_ref()
                    .and_then(inline_blocks::InlineText::initial_bounds);
                if lines.len() + layout.len() > MAX_LINES {
                    return Err("The chapter exceeds the Parley line limit".into());
                }
                if float.is_some() {
                    // The CSS footprint follows line boxes. Glyph overhang in
                    // Parley's ink bounds must not exclude an extra line.
                    float_height = layout.height();
                }
                let rise = initial_bounds
                    .as_ref()
                    .map_or(0., |bounds| -bounds.start.min(0.));
                let origin = y + rise
                    - layout
                        .lines()
                        .next()
                        .map_or(0.0, |line| line.metrics().content_block_min_coord.min(0.0));
                rtl = layout.is_rtl();
                let x = text_x
                    + if rtl {
                        0.0
                    } else {
                        (gutter - hanging + indent.common) as f32
                    };
                marker_baseline = layout.get(0).map(|line| origin + line.metrics().baseline);
                let mut bottom = origin
                    + layout
                        .height()
                        .max(initial_bounds.as_ref().map_or(0., |bounds| bounds.end));
                let mut overlays = ruby.iter().peekable();
                let first_line = lines.len();
                for (index, line) in layout.lines().enumerate() {
                    let metrics = line.metrics();
                    let mut top = metrics.content_block_min_coord;
                    let mut line_bottom = metrics.content_block_max_coord;
                    while let Some(overlay) = overlays.next_if(|overlay| overlay.line == index) {
                        top = top.min(overlay.top);
                        line_bottom = line_bottom.max(overlay.bottom);
                    }
                    if index == 0
                        && let Some(bounds) = &initial_bounds
                    {
                        top = top.min(bounds.start);
                    }
                    let top = origin + top;
                    let line_bottom = origin + line_bottom;
                    bottom = bottom.max(line_bottom);
                    if index == 0
                        && let Some(space) = margin_space.forced_top.filter(|&space| space > 0.0)
                    {
                        chapter
                            .physical_break_runs
                            .push(crate::publisher_breaks::PositionedRun {
                                target: block.element,
                                cuts: Box::new([top - space]),
                            });
                    }
                    let mut source_line = Line {
                        block: block.key,
                        bytes: line.text_range(),
                        spans: None,
                        additional_sources: None,
                        top,
                        bottom: line_bottom,
                        page_extension: [0.0; 2],
                        missing_glyphs: line
                            .items()
                            .filter_map(|item| match item {
                                PositionedLayoutItem::GlyphRun(run) => Some(
                                    run.positioned_glyphs()
                                        .filter(|glyph| glyph.id == 0)
                                        .count(),
                                ),
                                _ => None,
                            })
                            .sum(),
                    };
                    if let (Some(note), Some(table), Some(candidate)) =
                        (&note_layout, &mut tabular, &note_table)
                    {
                        let mut contains_table = false;
                        for item in line.items() {
                            if let PositionedLayoutItem::InlineBox(position) = item {
                                table.place(
                                    &position,
                                    x,
                                    origin,
                                    &mut inline_note_frames,
                                    &mut inline_note_fills,
                                );
                                source_line.top = source_line.top.min(origin + position.y);
                                source_line.bottom = source_line
                                    .bottom
                                    .max(origin + position.y + position.height);
                                bottom = bottom.max(source_line.bottom);
                                chapter.in_page_table_extents.insert(
                                    candidate.key,
                                    crate::footnotes::in_page::TableCopy::Inline,
                                );
                                contains_table = true;
                            }
                        }
                        note.project_ranges(
                            &mut source_line,
                            &table.ranges(line.text_range(), contains_table),
                        );
                        if contains_table {
                            source_line.missing_glyphs += table.missing_glyphs();
                        }
                    } else if let Some(note) = &note_layout {
                        note.project_line(&mut source_line, line.text_range());
                    } else if let Some(boxed) = &mut inline_text {
                        boxed.project_line(&mut source_line, &line, &projection, index == 0)?;
                    } else {
                        source_line.project(&projection, line.text_range())?;
                    }
                    if available > 0 && inline_text.is_none() && tabular.is_none() {
                        for item in line.items() {
                            let PositionedLayoutItem::InlineBox(position) = item else {
                                continue;
                            };
                            let image = &mut chapter.illustrations[position.id as usize];
                            let image_left = if position.kind == parley::InlineBoxKind::Block {
                                let inset = if rtl {
                                    text_width.saturating_sub(image.display.0)
                                } else {
                                    gutter - hanging
                                };
                                text_x + inset as f32
                            } else {
                                let after_gap = if rtl {
                                    position.width - image.display.0 as f32
                                } else {
                                    0.0
                                };
                                x + position.x + after_gap
                            };
                            image.x = Some(image_left.round().max(0.0) as usize);
                            image.y = (origin + position.y).round();
                            image.render_line = Some((source_line.block, source_line.top));
                        }
                    }
                    if index == 0
                        && let Some(bounds) = &initial_bounds
                    {
                        source_line.keep_page_extension(
                            [0., (origin + bounds.end - line_bottom).max(0.)],
                            settings.text_height() as f32,
                        );
                    }
                    lines.push(source_line);
                }
                if let Some(table) = tabular {
                    inline_text = Some(table.text);
                }
                let text_bottom = bottom;
                y = text_bottom + gap;
                if let Some(area) = fitted {
                    let [leading, trailing] = area.vertical_insets().map(|value| value as f32);
                    let top = origin - leading;
                    let natural = text_bottom + trailing - top;
                    let fill = area.desired_height.map_or(0.0, |height| {
                        (height - natural).clamp(0.0, settings.text_height() as f32)
                    });
                    bottom += fill + trailing;
                    y += fill + trailing;
                    if fill > 0.0 {
                        let fill_end = text_bottom + fill;
                        chapter.publisher_height_spaces.push(text_bottom..fill_end);
                        // CRE emits one-pixel splittable filler followed by
                        // the closing edge kept with the last filler pixel.
                        chapter.fitted_height_tails.push(
                            crate::publisher_padding::fit_content::HeightTail {
                                element: block.element,
                                closing: (fill_end - 1.0).max(text_bottom)..bottom,
                            },
                        );
                    }
                    if fitted_with_padding.is_some() {
                        fitted_extent = Some((top, bottom));
                        fitted_padding_elements.insert(block.element);
                    }
                    if fitted_with_padding.is_some() || fill > 0.0 {
                        let text_end = if fill > 0.0 { text_bottom } else { bottom };
                        keep_fitted_edges(&mut lines[first_line..], top..text_end, settings);
                    }
                } else if float.is_none()
                    && contour.is_none()
                    && height_candidates.get(block.element) == Some(&TextBoxCandidate::Simple)
                    && let Some(height) =
                        declared_box_height(style, outer_width, settings.font_size)
                {
                    // Follow the solid background's pixel origin, retain all of the
                    // natural lines, and limit trailing fill to a reading page.
                    let minimum = (origin.floor() + height)
                        .min((bottom + settings.text_height() as f32).floor());
                    if minimum > bottom {
                        chapter.publisher_height_spaces.push(bottom..minimum);
                        bottom = minimum;
                        y = y.max(bottom);
                    }
                }
                chapter.paragraphs.push(Paragraph {
                    key: block.key,
                    column: if let Some((left, width)) = placed_box {
                        left as f32..left as f32 + width as f32
                    } else {
                        publication_box
                            .map_or(column_x as f32..(column_x + column_width) as f32, |area| {
                                area.outer_x as f32..(area.outer_x + area.outer_width) as f32
                            })
                    },
                    signed_margin: placed_box.is_some(),
                    max_bottom: bottom,
                    x,
                    y: origin,
                    text_bottom,
                    bottom,
                    background: style.flow.painted.then_some(style.background).flatten(),
                    fitted: fitted.map(|mut area| {
                        if !style.flow.painted {
                            for stroke in &mut area.strokes {
                                stroke.gray = None;
                            }
                        }
                        area
                    }),
                    layout,
                    ruby,
                    flow: if note_layout.is_none() {
                        flow.shared(block.key)
                    } else {
                        None
                    },
                    inline_text,
                    joined_note: note_layout.as_ref().map(Arc::clone),
                });
            }

            if block.text.is_empty() {
                for image in &mut chapter.illustrations[first_image..] {
                    let top = y.ceil();
                    let bottom = top + image.display.1 as f32;
                    lines.push(Line {
                        block: block.key,
                        bytes: 0..0,
                        spans: None,
                        additional_sources: None,
                        top,
                        bottom,
                        page_extension: [0.0; 2],
                        missing_glyphs: 0,
                    });
                    image.y = top;
                    image.render_line = Some((block.key, top));
                    if image.full_line {
                        let inset = if rtl {
                            width.saturating_sub(image.display.0)
                        } else {
                            gutter
                        };
                        image.x = Some((column_x + inset) as usize);
                    } else if gutter != 0 || cell.is_some() || column_width != outer_width {
                        let inset = if rtl { 0 } else { gutter };
                        image.x = Some(
                            (column_x + inset + width.saturating_sub(image.display.0) / 2) as usize,
                        );
                    }
                    y = bottom + gap;
                }
            }

            if let Some((label, style, size)) = marker
                && literal.is_none()
                && gutter > 0
            {
                let separation = (size * 0.25).min(gutter as f32 * 0.2);
                let limit = gutter as f32 - separation;
                let baseline = marker_baseline.unwrap_or(block_y + size);
                let marker = if matches!(
                    style.marker,
                    crate::list_marker::MarkerKind::Disc
                        | crate::list_marker::MarkerKind::Circle
                        | crate::list_marker::MarkerKind::Square
                ) {
                    let diameter = (size * 0.31).round().clamp(1.0, limit.max(1.0));
                    let top = (baseline - size * 0.35 - diameter / 2.0).round();
                    let x = if rtl {
                        width as f32 + separation
                    } else {
                        limit - diameter
                    };
                    PaintedMarker {
                        x: column_x as f32 + x,
                        top,
                        bottom: top + diameter,
                        paint: MarkerPaint::Shape(style.marker),
                        painted: style.flow.painted,
                    }
                } else {
                    let mut layout =
                        marker_layout(&mut context, &mut fonts, &label, style, settings, size);
                    if layout.width() > limit {
                        let scaled = size * limit / layout.width();
                        layout = marker_layout(
                            &mut context,
                            &mut fonts,
                            &label,
                            style,
                            settings,
                            scaled.max(1.0),
                        );
                    }
                    let ink = *layout.get(0).unwrap().metrics();
                    let y = baseline - ink.baseline;
                    let x = if rtl {
                        width as f32 + separation
                    } else {
                        limit - layout.width()
                    };
                    PaintedMarker {
                        x: column_x as f32 + x,
                        top: y + ink.content_block_min_coord,
                        bottom: y + ink.content_block_max_coord,
                        painted: style.flow.painted,
                        paint: MarkerPaint::Text { y, layout },
                    }
                };
                chapter.markers.push(marker);
                if block.text.is_empty() && block.images.is_empty() {
                    y = y.max(block_y + size * settings.line_height.max(1.0)) + gap;
                }
            }

            if !y.is_finite() || y > settings.chapter_height_limit() {
                return Err("The chapter exceeds the Parley height limit".into());
            }
            if let Some(points) = &mut fill_positions {
                points.push(fitted_extent.unwrap_or((block_y, (y - gap).max(block_y))));
            }
            y += (padding.after - fitted_extent.map_or(0, |_| padding.own[1])) as f32;
            if let (Some(caption), Some(decoration)) = (caption, caption_decoration)
                && caption.last
            {
                y += decoration.trailing();
                let content = caption_start.0..y;
                let outer = decoration.outer(caption, content.clone());
                keep_fitted_edges(&mut lines[caption_start.1..], outer.clone(), settings);
                y = outer.end;
                positions.record_caption(caption.table, outer);
                decoration.paint(caption, content, &mut caption_frames, &mut caption_fills);
            }
            if let Some((c, _)) = decorated {
                let following = (slot + 1..source.blocks.len())
                    .map(|slot| &source.blocks[source_index(slot)])
                    .find(|next| !styles[next.element].flow.suppressed)
                    .and_then(|block| column_for(block, boxes));
                if following != Some(c) {
                    y += borders.as_ref().unwrap().vertical(c).1 as f32;
                }
            }
            if let Some(c) = cell.filter(|&c| {
                in_page_tables.contains(&c.table) || borders.as_ref().is_some_and(|b| b.tracks(c))
            }) {
                positions.record(c, start_of_cell, (y - gap).max(start_of_cell));
            }
            if cell.is_some() {
                table_alignment.end_block(
                    &chapter,
                    lines.len(),
                    fill_positions.as_ref().map_or(0, Vec::len),
                    y,
                    gap,
                    padding.after_grid > 0,
                );
            }
            if padding.after_grid > 0 {
                y = row_flow.end(y) + padding.after_grid as f32;
            }
            if let Some(spec) = float {
                let bottom = block_y + float_height.max(spec.publisher_height.unwrap_or(0.0));
                if bottom - block_y > settings.text_height() as f32 * 0.5 {
                    return Err(Box::new(crate::PageContentTooTall));
                }
                float_flow.record(spec, column_x, block_y, bottom)?;
                if let Some(gray) = spec.gray {
                    float_fills.push(crate::vertical_padding::Fill::rectangle(
                        spec.owner,
                        column_x..column_x + column_width,
                        block_y,
                        bottom,
                        gray,
                    ));
                }
                y = flow_y;
            }
            if floating_table
                .as_ref()
                .is_some_and(|(end, _)| *end == slot + 1)
            {
                let (_, placement) = floating_table.take().unwrap();
                let bottom = if table_alignment.ends_before(None) {
                    table_alignment.finish(
                        row_flow.end(y),
                        &mut chapter,
                        &mut lines,
                        fill_positions.as_deref_mut().unwrap_or_default(),
                        borders.as_ref(),
                        &mut positions,
                    )
                } else {
                    row_flow.end(y)
                };
                y = placement.finish(&mut float_flow, bottom, settings.text_height() as f32)?;
                row_flow = Default::default();
            }
        }

        if table_alignment.ends_before(None) {
            y = table_alignment.finish(
                row_flow.end(y),
                &mut chapter,
                &mut lines,
                fill_positions.as_deref_mut().unwrap_or_default(),
                borders.as_ref(),
                &mut positions,
            );
            row_flow = Default::default();
        }
        if chapter.float_geometry
            || source
                .blocks
                .iter()
                .any(|b| column_for(b, publisher_boxes.as_deref()).is_some())
        {
            chapter.paragraphs.sort_by(|a, b| a.y.total_cmp(&b.y));
        }
        let mut maximum = 0.0_f32;
        for p in &mut chapter.paragraphs {
            maximum = maximum.max(p.bottom);
            p.max_bottom = maximum;
        }
        if let Some(mut points) = fill_positions {
            if let Some(indices) = order {
                let mut canonical = vec![(0.0, 0.0); points.len()];
                for (&index, point) in indices.iter().zip(points) {
                    canonical[index] = point;
                }
                points = canonical;
            }
            chapter.padded_backgrounds = crate::vertical_padding::fills(
                source,
                styles,
                publisher_boxes.as_deref(),
                &points,
                settings,
                |element| fitted_padding_elements.contains(&element),
            );
        }
        chapter
            .in_page_table_extents
            .extend(in_page_tables.into_iter().filter_map(|key| {
                let extent = positions.outer_extent(key)?;
                (extent.start.is_finite() && extent.end.is_finite() && extent.start < extent.end)
                    .then_some((key, crate::footnotes::in_page::TableCopy::Grid(extent)))
            }));
        if let Some(b) = &borders {
            positions.backgrounds(
                b,
                publisher_boxes.as_deref(),
                styles,
                outer_width,
                settings.font_size,
                &mut chapter.padded_backgrounds,
            );
            if let Some(boxes) = &publisher_boxes {
                chapter.table_frames = positions.finish(b, boxes);
            }
        }
        chapter.table_frames.extend(caption_frames);
        chapter.table_frames.extend(inline_note_frames);
        chapter.padded_backgrounds.extend(caption_fills);
        chapter.padded_backgrounds.extend(inline_note_fills);
        if order.is_some() || borders.is_some() {
            chapter.padded_backgrounds.sort_by_key(|f| f.element);
        }
        chapter.padded_backgrounds.extend(float_fills);
        chapter.padded_backgrounds.extend(empty_fills);
        chapter
            .publisher_height_spaces
            .sort_by(|a, b| a.start.total_cmp(&b.start));
        chapter
            .fitted_height_tails
            .sort_by(|a, b| a.closing.start.total_cmp(&b.closing.start));
        let painted = chapter
            .table_frames
            .iter()
            .map(|f| f.bottom())
            .chain(
                chapter
                    .padded_backgrounds
                    .iter()
                    .map(|f| f.painted_bottom()),
            )
            .fold(0., f32::max);
        let extent = float_flow.finish((row_flow.end(y) - trailing_gap).max(painted).max(0.0));
        if !extent.is_finite() || extent > settings.chapter_height_limit() {
            return Err("The chapter exceeds the Parley height limit".into());
        }
        Ok((chapter, lines, extent))
    }

    pub(crate) fn physical_break_runs(&self) -> &[crate::publisher_breaks::PositionedRun] {
        &self.physical_break_runs
    }

    pub(crate) fn publisher_height_spaces(&self) -> &[Range<f32>] {
        &self.publisher_height_spaces
    }

    pub(crate) fn fitted_height_tails(
        &self,
    ) -> &[crate::publisher_padding::fit_content::HeightTail] {
        &self.fitted_height_tails
    }

    /// Last conservative paint bound. Parley's paragraph height also includes leading
    /// and layout lines after a final hard break, which can overflow onto a blank page.
    pub(crate) fn last_paint_bottom(&self, source: &Source) -> f32 {
        let artwork = self
            .illustrations
            .iter()
            .map(|art| art.y + art.display.1 as f32)
            .chain(self.table_frames.iter().map(|frame| frame.bottom()))
            .chain(self.markers.iter().map(|marker| marker.bottom))
            .chain(self.padded_backgrounds.iter().map(|f| f.painted_bottom()))
            .fold(0.0, f32::max);
        for paragraph in self.paragraphs.iter().rev() {
            if paragraph.background.is_some_and(|gray| gray < 255)
                || paragraph.fitted.is_some_and(|fit| {
                    (fit.vertical_padding != [0; 2] || fit.desired_height.is_some())
                        && fit.strokes[1..]
                            .iter()
                            .any(|stroke| stroke.width > 0 && stroke.gray.is_some())
                })
            {
                return artwork.max(paragraph.bottom);
            }
            let Ok(index) = source
                .blocks
                .binary_search_by_key(&paragraph.key, |block| block.key)
            else {
                return artwork.max(paragraph.bottom);
            };
            let text = paragraph
                .flow
                .as_ref()
                .map_or(source.blocks[index].text.as_str(), |flow| flow.text());
            let text = paragraph
                .joined_note
                .as_ref()
                .map_or(text, |note| note.text.as_str());
            let text = paragraph
                .inline_text
                .as_ref()
                .map_or(text, |boxed| boxed.text.as_str());
            let ruby = paragraph
                .ruby
                .iter()
                .map(|reading| paragraph.y + reading.bottom)
                .fold(artwork, f32::max);
            for line in paragraph.layout.lines().rev() {
                if text[line.text_range()].chars().any(|ch| !ch.is_whitespace())
                    || line.items().any(|item| matches!(item,
                        PositionedLayoutItem::GlyphRun(run) if run.advance() > 0.0 && run.style().brush.decoration != TextDecoration::default()))
                    || (paragraph.inline_text.is_some()
                        && line.items().any(|item| matches!(item, PositionedLayoutItem::InlineBox(_))))
                {
                    return ruby.max(paragraph.y + line.metrics().content_block_max_coord);
                }
            }
            if ruby > artwork {
                return ruby;
            }
        }
        artwork
    }

    pub(crate) fn source_at(
        &self,
        x: f32,
        y: f32,
        visible: &Range<f32>,
        inside_columns: bool,
    ) -> Option<(usize, usize)> {
        let start = self.paragraphs.partition_point(|p| p.max_bottom <= y);
        let mut candidates = self.paragraphs[start..]
            .iter()
            .take_while(|p| p.text_top() <= y)
            .filter(|p| {
                p.text_bottom > y
                    && p.hit_columns().contains(&x)
                    && (inside_columns || p.allows_margin_hits())
            });
        let hit = |paragraph: &Paragraph| {
            use crate::flow_projection::Affinity::After;
            let lx = x - paragraph.x;
            let ly = y - paragraph.y;
            let child = paragraph.text_box_at(lx, ly);
            let (layout, origin) = child.map_or((&paragraph.layout, [0.0, 0.0]), |child| {
                (&child.layout, [child.x, child.y])
            });
            let (cluster, _) =
                parley::Cluster::from_point_exact(layout, lx - origin[0], ly - origin[1])?;
            let line = cluster.line();
            let metrics = line.metrics();
            let top = paragraph.y + origin[1];
            let ink = top + metrics.content_block_min_coord..top + metrics.content_block_max_coord;
            // Clearance can postpone a paragraph's first line while its origin
            // remains above a table. Parley snaps hits in that gap to the line.
            if ink.end <= visible.start
                || ink.start >= visible.end
                || self.float_geometry && !ink.contains(&y)
            {
                return None;
            }
            let offset = cluster.text_range().start;
            let offset = match child {
                Some(child) => child.source.start + offset,
                None => paragraph.visible_text_offset(offset, After)?,
            };
            paragraph.source_location(offset, After)
        };
        if self.float_geometry {
            candidates.find_map(hit)
        } else {
            candidates.next().and_then(hit)
        }
    }

    pub(crate) fn uses_float_geometry(&self) -> bool {
        self.float_geometry
    }

    pub(crate) fn uses_joined_notes(&self) -> bool {
        self.paragraphs
            .iter()
            .any(|paragraph| paragraph.joined_note.is_some())
    }

    pub(crate) fn limitations(&self) -> &[String] {
        &self.limitations
    }

    pub(crate) fn pagination_rtl_blocks(&self) -> HashSet<usize> {
        self.paragraphs
            .iter()
            .filter(|paragraph| paragraph.layout.is_rtl())
            .map(|paragraph| paragraph.key)
            .collect()
    }

    /// The bottom edge of artwork assigned to each ordinary source line. In-page
    /// pagination can cut only where the whole illustration belongs to one band.
    pub(crate) fn pagination_artwork_bottoms(
        &self,
        lines: &[Line],
    ) -> Option<std::collections::BTreeMap<usize, f32>> {
        if self.note_table_fit_failed {
            return None;
        }
        let mut bottoms = std::collections::BTreeMap::<_, f32>::new();
        for image in &self.illustrations {
            let (owner, top) = image.render_line?;
            let first = lines.partition_point(|line| line.top < top - 0.01);
            let (offset, line) = lines[first..]
                .iter()
                .enumerate()
                .take_while(|(_, line)| line.top <= top + 0.01)
                .find(|(_, line)| line.block == owner)?;
            // Drawing rounds the image position relative to the page's origin.
            if (image.y - line.top).round() < 0.0 {
                return None;
            }
            let bottom = image.y + image.display.1 as f32;
            bottoms
                .entry(first + offset)
                .and_modify(|extent| *extent = extent.max(bottom))
                .or_insert(bottom);
        }
        Some(bottoms)
    }

    pub(crate) fn image_top(&self, key: usize, ordinal: usize) -> Option<f32> {
        self.illustrations
            .iter()
            .find(|image| image.block == key && image.source_index == ordinal)
            .map(|image| image.y)
    }

    pub(crate) fn image_source_on_line(&self, key: usize, top: f32) -> Option<(usize, usize)> {
        self.illustrations
            .iter()
            .find(|image| {
                image
                    .render_line
                    .is_some_and(|(owner, line_top)| owner == key && (line_top - top).abs() < 0.01)
            })
            .map(|image| (image.block, image.source_index))
    }

    pub(crate) fn illustration_at(
        &self,
        range: Range<f32>,
        settings: Settings,
        x: f32,
        y: f32,
    ) -> Option<&str> {
        self.visible_illustrations(range, settings)
            .find_map(|(path, cols, rows)| {
                (x >= cols.start as f32
                    && x < cols.end as f32
                    && y >= rows.start as f32
                    && y < rows.end as f32)
                    .then_some(path)
            })
    }

    pub(crate) fn visible_illustrations(
        &self,
        range: Range<f32>,
        settings: Settings,
    ) -> impl Iterator<Item = (&str, Range<usize>, Range<usize>)> {
        self.visible_artwork(range, settings)
            .filter_map(|(image, cols, rows)| Some((image.path.as_deref()?, cols, rows)))
    }

    pub(crate) fn image_link_at(
        &self,
        range: Range<f32>,
        settings: Settings,
        x: f32,
        y: f32,
    ) -> Option<&str> {
        self.visible_artwork(range, settings)
            .find_map(|(image, cols, rows)| {
                (x >= cols.start as f32
                    && x < cols.end as f32
                    && y >= rows.start as f32
                    && y < rows.end as f32)
                    .then(|| image.link.as_deref())
                    .flatten()
            })
    }

    pub(crate) fn image_links(
        &self,
        range: Range<f32>,
        settings: Settings,
    ) -> impl Iterator<Item = (&str, &str)> {
        self.visible_artwork(range, settings)
            .filter_map(|(image, _, _)| Some((image.label.as_str(), image.link.as_deref()?)))
    }

    pub(crate) fn image_link_regions(
        &self,
        range: Range<f32>,
        settings: Settings,
    ) -> impl Iterator<Item = (&str, Range<usize>, Range<usize>)> {
        self.visible_artwork(range, settings)
            .filter_map(|(image, cols, rows)| Some((image.link.as_deref()?, cols, rows)))
    }

    fn visible_artwork(
        &self,
        range: Range<f32>,
        settings: Settings,
    ) -> impl Iterator<Item = (&Illustration, Range<usize>, Range<usize>)> {
        let height = visible_rows(&range, settings) as i32;
        self.illustrations.iter().filter_map(move |illustration| {
            let (left, top) = illustration.origin(range.start, &settings.text_columns());
            let rows = top.max(0)..(top + illustration.display.1 as i32).min(height);
            if rows.is_empty() {
                return None;
            }
            let margin = settings.top_margin() as usize;
            Some((
                illustration,
                left..left + illustration.display.0 as usize,
                margin + rows.start as usize..margin + rows.end as usize,
            ))
        })
    }

    #[cfg(feature = "kindle")]
    pub(crate) fn illustration_counts(&self) -> (usize, usize) {
        let decoded = self
            .illustrations
            .iter()
            .filter(|art| art.path.is_some())
            .count();
        (decoded, self.illustrations.len() - decoded)
    }

    pub(crate) fn has_illustration(&self, path: &str) -> bool {
        self.illustrations
            .iter()
            .any(|illustration| illustration.path.as_deref() == Some(path))
    }

    pub(crate) fn paragraphs_in(&self, page: Range<f32>) -> impl Iterator<Item = &Paragraph> {
        let start = self
            .paragraphs
            .partition_point(|paragraph| paragraph.max_bottom <= page.start);
        self.paragraphs[start..]
            .iter()
            .take_while(move |paragraph| paragraph.text_top() < page.end)
            .filter(move |p| p.bottom > page.start)
    }

    pub fn capture(&mut self, range: Range<f32>, settings: Settings) -> Result<Vec<u8>> {
        self.glyphs.begin_paint();
        let visible_height = visible_rows(&range, settings);
        let mut frame = vec![255; (settings.width * settings.height) as usize];
        let stride = settings.width as usize;
        let columns = settings.text_columns();
        let margin = columns.start;
        let top = settings.top_margin() as usize;
        let page = &mut frame[top * stride..(top + visible_height) * stride];
        for fill in &self.padded_backgrounds {
            fill.paint(page, stride, margin, &range);
        }

        let index = self
            .paragraphs
            .partition_point(|paragraph| paragraph.max_bottom <= range.start);
        for paragraph in &self.paragraphs[index..] {
            if paragraph.painted_top() >= range.end {
                break;
            }
            if paragraph.bottom <= range.start {
                continue;
            }
            let paint_columns = paragraph.paint_columns(settings);
            let paint_x = paragraph.x + (margin as f32 - paint_columns.start as f32);
            if let Some(gray) = paragraph.background {
                let top = (paragraph.painted_top() - range.start).floor().max(0.0) as usize;
                let bottom = ((paragraph.bottom - range.start).ceil() as usize).min(visible_height);
                let band = paragraph.background_columns(settings);
                for row in top.min(bottom)..bottom {
                    page[row * stride + band.start..row * stride + band.end].fill(gray);
                }
            }
            paragraph.paint_fitted_border(page, settings, &range);
            for line in paragraph.layout.lines() {
                let metrics = line.metrics();
                if paragraph.y + metrics.content_block_max_coord <= range.start {
                    continue;
                }
                if paragraph.y + metrics.content_block_min_coord >= range.end {
                    break;
                }
                for item in line.items() {
                    if let PositionedLayoutItem::GlyphRun(run) = item {
                        paint_text_run(
                            &mut self.glyphs,
                            page,
                            settings,
                            &paint_columns,
                            (paint_x, paragraph.y - range.start),
                            &run,
                        );
                    }
                }
            }
            if let Some(boxed) = &paragraph.inline_text {
                for child in &boxed.boxes {
                    let (horizontal, vertical) = child.painted_bounds();
                    let top = paragraph.y + vertical.start;
                    let bottom = paragraph.y + vertical.end;
                    if bottom <= range.start || top >= range.end {
                        continue;
                    }
                    if let Some(gray) = child.background() {
                        let width = paint_columns.len() as f32;
                        let left = (paint_x + horizontal.start).round().clamp(0.0, width) as usize;
                        let right = (paint_x + horizontal.end).round().clamp(0.0, width) as usize;
                        let top = (top - range.start)
                            .floor()
                            .clamp(0.0, visible_height as f32)
                            as usize;
                        let bottom = (bottom - range.start)
                            .ceil()
                            .clamp(0.0, visible_height as f32)
                            as usize;
                        for row in top..bottom {
                            let offset = row * stride + paint_columns.start;
                            page[offset + left..offset + right].fill(gray);
                        }
                    }
                    for item in child.layout.lines().flat_map(|line| line.items()) {
                        if let PositionedLayoutItem::GlyphRun(run) = item {
                            paint_text_run(
                                &mut self.glyphs,
                                page,
                                settings,
                                &paint_columns,
                                (paint_x + child.x, paragraph.y + child.y - range.start),
                                &run,
                            );
                        }
                    }
                }
            }
            for overlay in &paragraph.ruby {
                if !overlay.painted {
                    continue;
                }
                if paragraph.y + overlay.top >= range.end
                    || paragraph.y + overlay.bottom <= range.start
                {
                    continue;
                }
                for item in overlay.layout.lines().flat_map(|line| line.items()) {
                    if let PositionedLayoutItem::GlyphRun(run) = item {
                        self.glyphs.draw_run(
                            page,
                            stride,
                            &columns,
                            (
                                paragraph.x + overlay.x,
                                paragraph.y + overlay.y - range.start,
                            ),
                            &run,
                            settings.text_contrast,
                            settings.font_gamma_index,
                        );
                    }
                }
            }
        }

        for marker in &self.markers {
            if !marker.painted {
                continue;
            }
            if marker.top >= range.end || marker.bottom <= range.start {
                continue;
            }
            match &marker.paint {
                MarkerPaint::Text { y, layout } => {
                    for item in layout.lines().flat_map(|line| line.items()) {
                        if let PositionedLayoutItem::GlyphRun(run) = item {
                            self.glyphs.draw_run(
                                page,
                                stride,
                                &columns,
                                (marker.x, *y - range.start),
                                &run,
                                settings.text_contrast,
                                settings.font_gamma_index,
                            );
                        }
                    }
                }
                MarkerPaint::Shape(kind) => draw_bullet(
                    page,
                    stride,
                    margin as f32 + marker.x,
                    marker.top - range.start,
                    marker.bottom - marker.top,
                    *kind,
                ),
            }
        }

        for illustration in &self.illustrations {
            illustration.draw(page, &range, stride, &columns);
        }
        for stroke in &self.table_frames {
            stroke.paint(page, stride, &columns, &range);
        }
        self.glyphs.check_paint()?;
        Ok(frame)
    }
}

fn visible_rows(range: &Range<f32>, settings: Settings) -> usize {
    (range.end - range.start)
        .ceil()
        .clamp(1.0, settings.text_height() as f32) as usize
}

fn paint_text_run(
    glyphs: &mut GlyphCache,
    page: &mut [u8],
    settings: Settings,
    columns: &Range<usize>,
    (x, y): (f32, f32),
    run: &parley::GlyphRun<'_, PaintBrush>,
) {
    let stride = settings.width as usize;
    let visible_height = page.len() / stride;
    if !run.style().brush.painted {
        return;
    }
    glyphs.draw_run(
        page,
        stride,
        columns,
        (x, y),
        run,
        settings.text_contrast,
        settings.font_gamma_index,
    );
    let decoration = run.style().brush.decoration;
    if decoration != TextDecoration::default() {
        let metrics = run.run().font_metrics();
        let thickness = metrics.underline_size.round().clamp(1.0, 12.0);
        let baseline = y + run.baseline();
        let rows = [
            decoration
                .underline
                .then_some(baseline - metrics.underline_offset),
            decoration
                .overline
                .then_some(baseline - metrics.ascent + thickness),
            decoration
                .strike
                .then_some(baseline - run.run().font_size() * 2.0 / 7.0),
        ];
        let limit = columns.len() as f32;
        let x0 = columns.start + (x + run.offset()).floor().clamp(0.0, limit) as usize;
        let x1 =
            columns.start + (x + run.offset() + run.advance()).ceil().clamp(0.0, limit) as usize;
        for y in rows.into_iter().flatten() {
            let start = y.round().clamp(0.0, visible_height as f32) as usize;
            let end = (y.round() + thickness).clamp(0.0, visible_height as f32) as usize;
            for row in start..end {
                page[row * stride + x0..row * stride + x1]
                    .fill(run.style().brush.link_ink.unwrap_or(48));
            }
        }
    }
}

#[derive(Clone, Copy, Default)]
struct LineIndent {
    common: u32,
    amount: f32,
    hanging: bool,
}

impl LineIndent {
    fn resolve(style: &Style, content_width: u32, width: u32, font: f32, marker: u32) -> Self {
        Self::with_grid(style, content_width, width, font, marker, None)
    }

    fn with_grid(
        style: &Style,
        content_width: u32,
        width: u32,
        font: f32,
        marker: u32,
        unit: Option<u32>,
    ) -> Self {
        // Keep enough of the column for a word or glyph, even for malformed
        // publisher lengths. An outdent at the reading margin uses the first
        // available column and shifts the other lines inward (as CREngine does
        // when page overflow is disabled). This also protects adjacent cells.
        let limit = (width as f32 - font.min(width as f32 / 2.0)).max(0.0);
        let indent_font = unit.map_or(font, |unit| unit as f32);
        let requested = style.text_indent.pixels(content_width, indent_font);
        let css = if requested < 0.0 {
            style.text_indent.box_pixels(content_width, indent_font)
        } else {
            requested
        };
        let css = unit.map_or(css, |unit| cjk_width::grid_indent(css, width, unit));
        let css = css.clamp(-limit, limit);
        let (first, rest) = if style.text_indent.hanging {
            (0.0, marker as f32 + css)
        } else {
            (css, marker as f32)
        };
        let outside = first.min(rest).min(0.0);
        let first = (first - outside).clamp(0.0, limit);
        let rest = (rest - outside).clamp(0.0, limit);
        Self {
            common: first.min(rest).round() as u32,
            amount: (first - rest).abs(),
            hanging: rest > first,
        }
    }
}

#[cfg(test)]
fn layout_paragraph(
    context: &mut LayoutContext<PaintBrush>,
    fonts: &mut FontContext,
    block: &Block,
    width: u32,
    settings: Settings,
    styles: &[Style],
) -> (Layout<PaintBrush>, Vec<RubyOverlay>) {
    let input = LayoutBlock::identity(block);
    let indent = LineIndent::resolve(&styles[block.element], width, width, settings.font_size, 0);
    layout_with_images(
        context,
        fonts,
        &input,
        width - indent.common,
        settings,
        styles,
        &[],
        indent,
        &crate::user_hyphenation::Rules::default(),
        None,
    )
}

fn layout_with_images(
    context: &mut LayoutContext<PaintBrush>,
    fonts: &mut FontContext,
    block: &LayoutBlock<'_>,
    width: u32,
    settings: Settings,
    styles: &[Style],
    images: &[parley::InlineBox],
    indent: LineIndent,
    user_rules: &crate::user_hyphenation::Rules,
    contour: Option<crate::float_layout::Contour>,
) -> (Layout<PaintBrush>, Vec<RubyOverlay>) {
    let automatic = crate::hyphenation::with_user_rules_text(
        block.text,
        &block.styled_text,
        styles,
        settings,
        user_rules,
    );
    let mut layout = make_paragraph(
        context,
        fonts,
        block,
        Some(width),
        settings,
        styles,
        &[],
        images,
        false,
        indent,
        &automatic,
        contour,
    );
    // Some embedded fonts have ascenders and descenders taller than the publisher's
    // line advance. A long paragraph can then have no page break between full lines.
    let fit_font_metrics = overlaps_entire_page(&layout, settings.text_height() as f32);
    if fit_font_metrics {
        drop(layout);
        layout = make_paragraph(
            context,
            fonts,
            block,
            Some(width),
            settings,
            styles,
            &[],
            images,
            true,
            indent,
            &automatic,
            contour,
        );
    }
    let mut candidates = Vec::new();
    for ruby in block.ruby.iter() {
        if ruby.bytes.len() > MAX_RUBY_BASE_BYTES {
            continue;
        }
        let Some((_, line, _, _)) = base_geometry(&layout, &ruby.bytes) else {
            continue;
        };
        // The rt element owns its own font declarations; its ID was retained
        // in the source projection without adding anything to canonical text.
        let style = &styles[ruby.element];
        let annotation = make_annotation(
            context,
            fonts,
            &ruby.annotation,
            ruby.language,
            style,
            settings,
        );
        let Some(annotation_line) = annotation.get(0) else {
            continue;
        };
        if annotation.len() != 1
            || !annotation.width().is_finite()
            || annotation.width() > width as f32
        {
            continue;
        }
        if annotation_line.items().any(|item| match item {
            PositionedLayoutItem::GlyphRun(run) => {
                run.positioned_glyphs().any(|glyph| glyph.id == 0)
            }
            PositionedLayoutItem::InlineBox(_) => true,
        }) {
            continue;
        }
        let ink = annotation_line.metrics();
        let height = ink.content_block_max_coord - ink.content_block_min_coord;
        let base = layout.get(line).unwrap();
        let line_height = base.metrics().line_height + 2.0 * (height + 4.0);
        if line_height > settings.text_height() as f32 {
            continue;
        }
        candidates.push(RubyCandidate {
            bytes: ruby.bytes.clone(),
            layout: annotation,
            line_height,
            painted: ruby.painted,
            // Our simple ruby extraction has one annotation row per base;
            // CRE's first alternating row is over the base.
            under: style.ruby_position == RubyPosition::Under,
        });
    }
    if candidates.is_empty() {
        return (layout, Vec::new());
    }
    drop(layout);
    let layout = make_paragraph(
        context,
        fonts,
        block,
        Some(width),
        settings,
        styles,
        &candidates,
        images,
        fit_font_metrics,
        indent,
        &automatic,
        contour,
    );
    let mut overlays: Vec<RubyOverlay> = Vec::new();
    for candidate in candidates {
        let Some((base, index, base_top, base_bottom)) = base_geometry(&layout, &candidate.bytes)
        else {
            continue;
        };
        let ink = *candidate.layout.get(0).unwrap().metrics();
        let annotation_width = candidate.layout.width();
        let line = layout.get(index).unwrap();
        let metrics = line.metrics();
        let height = ink.content_block_max_coord - ink.content_block_min_coord;
        let (top, bottom) = if candidate.under {
            let top = base_bottom + 2.0;
            (top, top + height)
        } else {
            let bottom = base_top - 2.0;
            (bottom - height, bottom)
        };
        if top < metrics.block_min_coord
            || bottom > metrics.block_max_coord
            || annotation_width > width as f32
        {
            continue;
        }
        let Some(x) = ruby_position(
            base,
            annotation_width,
            width as f32,
            index,
            candidate.painted,
            top,
            bottom,
            &overlays,
        ) else {
            continue;
        };
        overlays.push(RubyOverlay {
            line: index,
            x,
            y: top - ink.content_block_min_coord,
            top,
            bottom,
            layout: candidate.layout,
            painted: candidate.painted,
        });
    }
    overlays.sort_by_key(|overlay| overlay.line);
    (layout, overlays)
}

fn base_geometry(
    layout: &Layout<PaintBrush>,
    bytes: &Range<usize>,
) -> Option<(BoundingBox, usize, f32, f32)> {
    let start = Cursor::from_byte_index(layout, bytes.start, Affinity::Downstream);
    let end = Cursor::from_byte_index(layout, bytes.end, Affinity::Upstream);
    if start.index() != bytes.start || end.index() != bytes.end {
        return None;
    }
    let mut result = None;
    let mut multiple = false;
    Selection::new(start, end).geometry_with(layout, |bounds, line| {
        multiple |= result.replace((bounds, line)).is_some();
    });
    let (mut bounds, row) = result?;
    let line = layout.get(row)?;
    let text = line.text_range();
    if multiple || text.start > bytes.start || bytes.end > text.end {
        return None;
    }
    let mut top = line.metrics().content_block_min_coord;
    let mut bottom = line.metrics().content_block_max_coord;
    if layout.inline_boxes().len() > 0 {
        let mut has_artwork = false;
        let mut glyph_top = f32::INFINITY;
        let mut glyph_bottom = f32::NEG_INFINITY;
        // Selection gives a preceding inline box to the first selected cluster.
        // Ruby must clear its base text, so the box cannot set either its
        // center or its vertical clearance.
        for item in line.items() {
            match item {
                PositionedLayoutItem::InlineBox(art) => {
                    has_artwork = true;
                    let left = f64::from(art.x);
                    let right = f64::from(art.x + art.width);
                    if left < bounds.x1 && right > bounds.x0 {
                        if left <= bounds.x0 + 0.25 {
                            bounds.x0 = right;
                        } else if right >= bounds.x1 - 0.25 {
                            bounds.x1 = left;
                        } else {
                            return None;
                        }
                    }
                }
                PositionedLayoutItem::GlyphRun(glyphs) => {
                    let run = glyphs.run();
                    let text = run.text_range();
                    if text.start < bytes.end && bytes.start < text.end {
                        glyph_top = glyph_top.min(glyphs.baseline() - run.font_metrics().ascent);
                        glyph_bottom =
                            glyph_bottom.max(glyphs.baseline() + run.font_metrics().descent);
                    }
                }
            }
        }
        if has_artwork && glyph_top.is_finite() {
            top = glyph_top;
        }
        if has_artwork && glyph_bottom.is_finite() {
            bottom = glyph_bottom;
        }
    }
    (bounds.width() > 0.0).then_some((bounds, row, top, bottom))
}

/// Rasterize typographic bullets geometrically so a book font lacking U+25E6 cannot show tofu.
fn draw_bullet(
    page: &mut [u8],
    stride: usize,
    left: f32,
    top: f32,
    diameter: f32,
    kind: crate::list_marker::MarkerKind,
) {
    use crate::list_marker::MarkerKind;
    let radius = diameter * 0.5;
    let inner = (radius - (diameter * 0.16).max(1.0)).max(0.0);
    for y in
        top.floor().max(0.0) as usize..((top + diameter).ceil() as usize).min(page.len() / stride)
    {
        for x in left.floor().max(0.0) as usize..((left + diameter).ceil() as usize).min(stride) {
            let mut hits = 0;
            for dy in [0.25, 0.75] {
                for dx in [0.25, 0.75] {
                    let px = x as f32 + dx - left;
                    let py = y as f32 + dy - top;
                    let distance = (px - radius).powi(2) + (py - radius).powi(2);
                    let inside = match kind {
                        MarkerKind::Square => {
                            px >= 0.0 && px < diameter && py >= 0.0 && py < diameter
                        }
                        MarkerKind::Circle => {
                            distance <= radius * radius && distance >= inner * inner
                        }
                        _ => distance <= radius * radius,
                    };
                    hits += u16::from(inside);
                }
            }
            let pixel = &mut page[y * stride + x];
            *pixel = (u16::from(*pixel) * (4 - hits) / 4) as u8;
        }
    }
}

/// Element indices are in document order, so a nested list can inherit its parent's complete inset.
fn list_insets(
    source: &Source,
    styles: &[Style],
    settings: Settings,
    flow: &ChapterFlow,
) -> HashMap<usize, f32> {
    let font_size = settings.font_size;
    let mut groups = std::collections::BTreeMap::new();
    for block in &source.blocks {
        if let Some(item) = block.list_item.filter(|item| item.first) {
            let style = &styles[item.element];
            let (_, _, width) =
                groups
                    .entry(item.group)
                    .or_insert((item.parent_group, item.depth, 0.0_f32));
            if let Some(label) = flow
                .ordinal(item)
                .and_then(|ordinal| style.marker.label(ordinal))
            {
                let estimate = if settings.logical_font_size.is_some() {
                    settings.styled_font(style.size_scale)
                } else {
                    font_size * style.size_scale
                } * (0.76 * label.chars().count() as f32 + 0.46);
                *width = width.max(estimate);
            }
        }
    }
    let mut result = HashMap::new();
    for (group, (parent, depth, width)) in groups {
        let inherited = parent.map_or(0.0, |id| {
            result
                .get(&id)
                .copied()
                .unwrap_or(font_size * 1.3 * (depth - 1) as f32)
        });
        result.insert(group, inherited + width);
    }
    result
}

fn marker_layout(
    context: &mut LayoutContext<PaintBrush>,
    fonts: &mut FontContext,
    text: &str,
    style: &Style,
    settings: Settings,
    size: f32,
) -> Layout<PaintBrush> {
    let mut builder = context.ranged_builder(fonts, text, 1.0, true);
    let font_weights = style.font_weights.clone();
    let weight_spacing = move |font: &parley::FontData, requested: f32, size: f32| {
        crate::font_synthesis::strength(font, requested, size, &font_weights)
    };
    builder.set_font_weight_spacing(Some(&weight_spacing));
    let cjk_spacing = cjk_width::policy(text, settings.cjk_width_percent);
    if let Some(policy) = &cjk_spacing {
        builder.set_cluster_spacing(Some(policy));
    }
    let family: FontFamily<'_> = if settings.override_fonts {
        GenericFamily::Serif.into()
    } else {
        style.font_family.layout()
    };
    builder.push_default(StyleProperty::FontFamily(family));
    builder.push_default(StyleProperty::FontSize(size));
    if let Some(axes) = optical_axes(style, settings, size) {
        builder.push_default(StyleProperty::FontVariations(axes.as_slice().into()));
    }
    builder.push_default(StyleProperty::FontWeight(FontWeight::new(
        crate::font_render::font_weight(
            if settings.bold_text && style.font_weight < FontWeight::BOLD {
                FontWeight::BOLD.value()
            } else {
                style.font_weight.value()
            },
            settings.font_weight_delta,
        ),
    )));
    builder.push_default(StyleProperty::FontStyle(style.font_style));
    let mut layout = builder.build(text);
    layout.break_all_lines(None);
    layout
}

fn effective_letter_spacing(style: &Style, settings: Settings) -> f32 {
    let publisher = style.letter_spacing.pixels(0, settings.font_size);
    let publisher = if publisher.is_finite() {
        publisher.clamp(-1.0, 128.0)
    } else {
        0.0
    };
    // Match the existing reader control's safe negative tracking floor.
    (settings.letter_spacing + publisher).max(-1.0)
}

fn optical_axes(style: &Style, settings: Settings, pixels: f32) -> Option<[FontVariation; 1]> {
    if !style.optical_sizing || settings.render_dpi < 100 || !pixels.is_finite() || pixels <= 0.0 {
        return None;
    }
    let points = pixels * 72.0 / f32::from(settings.render_dpi);
    points
        .is_finite()
        .then(|| [FontVariation::new(Tag::new(b"opsz"), points)])
}

fn make_annotation(
    context: &mut LayoutContext<PaintBrush>,
    fonts: &mut FontContext,
    text: &str,
    language: Option<Language>,
    style: &Style,
    settings: Settings,
) -> Layout<PaintBrush> {
    let mut builder = context.ranged_builder(fonts, text, 1.0, true);
    let font_weights = style.font_weights.clone();
    let weight_spacing = move |font: &parley::FontData, requested: f32, size: f32| {
        crate::font_synthesis::strength(font, requested, size, &font_weights)
    };
    builder.set_font_weight_spacing(Some(&weight_spacing));
    let cjk_spacing = cjk_width::policy(text, settings.cjk_width_percent);
    if let Some(policy) = &cjk_spacing {
        builder.set_cluster_spacing(Some(policy));
    }
    apply_kerning(&mut builder, settings.effective_font_kerning_mode());
    if !style.common_ligatures
        || style.ligature_extras != LigatureExtras::default()
        || style.variant_caps != VariantCaps::Normal
        || style.variant_numeric != VariantNumeric::default()
        || style.variant_east_asian != VariantEastAsian::default()
        || style.variant_position != VariantPosition::Normal
        || style.variant_historical
    {
        let features = shaping_with_variants(
            settings.effective_font_kerning_mode(),
            style.common_ligatures,
            style.ligature_extras,
            style.variant_caps,
            style.variant_numeric,
            style.variant_east_asian,
            style.variant_position,
            style.variant_historical,
        );
        builder.push_default(StyleProperty::FontFeatures(features.as_slice().into()));
    }
    let family: FontFamily<'_> = if settings.override_fonts {
        GenericFamily::Serif.into()
    } else {
        style.font_family.layout()
    };
    builder.push_default(StyleProperty::FontFamily(family));
    let spacing = effective_letter_spacing(style, settings);
    if spacing != 0.0 {
        builder.push_default(StyleProperty::LetterSpacing(spacing));
    }
    builder.push_default(StyleProperty::Locale(language));
    builder.push_default(StyleProperty::FontStyle(style.font_style));
    builder.push_default(StyleProperty::FontWeight(FontWeight::new(
        crate::font_render::font_weight(
            if settings.bold_text {
                style.font_weight.value().max(FontWeight::BOLD.value())
            } else {
                style.font_weight.value()
            },
            settings.font_weight_delta,
        ),
    )));
    let scale = if style.ruby_size_explicit {
        style.size_scale
    } else {
        style.size_scale * 0.48
    };
    let annotation_size = if settings.logical_font_size.is_some() {
        settings.styled_font(scale)
    } else {
        (settings.font_size * scale).clamp(8.0, 84.0)
    };
    builder.push_default(StyleProperty::FontSize(annotation_size));
    if let Some(axes) = optical_axes(style, settings, annotation_size) {
        builder.push_default(StyleProperty::FontVariations(axes.as_slice().into()));
    }
    let mut layout = builder.build(text);
    layout.break_all_lines(None);
    layout
}

fn overlaps_entire_page(layout: &Layout<PaintBrush>, page_height: f32) -> bool {
    if layout.len() < 2 {
        return false;
    }
    let mut start = 0.0;
    let mut end = f32::NEG_INFINITY;
    for line in layout.lines() {
        let metrics = line.metrics();
        if metrics.content_block_min_coord >= end {
            start = metrics.content_block_min_coord;
            end = metrics.content_block_max_coord;
        } else {
            end = end.max(metrics.content_block_max_coord);
            if end - start > page_height {
                return true;
            }
        }
    }
    false
}

fn make_paragraph(
    context: &mut LayoutContext<PaintBrush>,
    fonts: &mut FontContext,
    block: &LayoutBlock<'_>,
    width: Option<u32>,
    settings: Settings,
    styles: &[Style],
    ruby: &[RubyCandidate],
    images: &[parley::InlineBox],
    fit_font_metrics: bool,
    indent: LineIndent,
    automatic: &crate::user_hyphenation::Opportunities,
    contour: Option<crate::float_layout::Contour>,
) -> Layout<PaintBrush> {
    first_line::layout(
        block,
        &styles[block.element],
        width.is_some() && images.is_empty(),
        |pass| {
            shape_paragraph(
                context,
                fonts,
                block,
                width,
                settings,
                styles,
                ruby,
                images,
                fit_font_metrics,
                indent,
                automatic,
                contour,
                pass,
            )
        },
    )
}

// Some scripts and languages need contextual case mapping (for example Greek
// final sigma). Only use a whole-paragraph result when every source scalar and
// grapheme boundary remains exactly representable in the display text.
fn contextual_transformed(block: &LayoutBlock<'_>, styles: &[Style]) -> Option<Option<String>> {
    let transform = styles.get(block.element)?.text_transform;
    if !matches!(
        transform,
        TextTransform::Uppercase | TextTransform::Lowercase
    ) {
        return None;
    }
    let mut previous_end = 0;
    for run in block.styled_text.iter() {
        if run.bytes.start < previous_end
            || run.bytes.end > block.text.len()
            || run.bytes.start > run.bytes.end
            || !block.text.is_char_boundary(run.bytes.start)
            || !block.text.is_char_boundary(run.bytes.end)
            || styles.get(run.element)?.text_transform != transform
            || run.language.or(block.language) != block.language
        {
            return None;
        }
        previous_end = run.bytes.end;
    }
    let parsed_language = match block.language {
        Some(language) => language.as_str().parse(),
        None => "und".parse(),
    };
    let Ok(language) = parsed_language else {
        return Some(None);
    };
    let mapper = icu_casemap::CaseMapper::new();
    let visual = match transform {
        TextTransform::Uppercase => mapper.uppercase_to_string(block.text, &language),
        TextTransform::Lowercase => mapper.lowercase_to_string(block.text, &language),
        _ => return None,
    };
    if visual == block.text {
        return Some(None);
    }
    if visual.len() != block.text.len()
        || !block
            .text
            .char_indices()
            .map(|(offset, _)| offset)
            .eq(visual.char_indices().map(|(offset, _)| offset))
    {
        return Some(None);
    }
    let graphemes = icu_segmenter::GraphemeClusterSegmenter::new();
    if !graphemes
        .segment_str(block.text)
        .eq(graphemes.segment_str(&visual))
    {
        return Some(None);
    }
    Some(Some(visual.into_owned()))
}

// Pseudo and generated styles use the same layout byte ranges as the existing
// first-line passes and source projection. ASCII keeps casing one-to-one.
fn pseudo_ascii_transformed(
    block: &LayoutBlock<'_>,
    styles: &[Style],
    pass: first_line::Pass,
) -> Option<String> {
    if !block.text.is_ascii() {
        return None;
    }
    let locale_sensitive = |language: Option<Language>| {
        language.is_some_and(|language| {
            [
                "tr", "tur", "az", "aze", "crh", "lt", "lit", "nl", "nld", "dut",
            ]
            .iter()
            .any(|tag| language.language().eq_ignore_ascii_case(tag))
        })
    };
    if locale_sensitive(block.language)
        || block.styled_text.iter().any(|run| {
            locale_sensitive(run.language) || run.language.or(block.language) != block.language
        })
    {
        return None;
    }
    let base = styles.get(block.element)?;
    let first_end = pass.styled_end(block.text);
    if first_end > block.text.len() || !block.text.is_char_boundary(first_end) {
        return None;
    }
    let mut modes = vec![base.text_transform; block.text.len()];
    let mut covered = vec![false; block.text.len()];
    if let Some(line) = base.first_line.as_ref().filter(|_| first_end > 0) {
        modes[..first_end].fill(line.apply(base).text_transform);
    }
    let first_style = |style: &Style| {
        style
            .first_line
            .as_deref()
            .filter(|line| line.host == block.element)
            .map(|line| line.apply(style))
    };
    let mut previous_end = 0;
    for run in block.styled_text.iter() {
        if run.bytes.start < previous_end
            || run.bytes.start > run.bytes.end
            || run.bytes.end > modes.len()
        {
            return None;
        }
        let style = styles.get(run.element)?;
        modes[run.bytes.clone()].fill(style.text_transform);
        covered[run.bytes.clone()].fill(true);
        let first_part = run.bytes.start..run.bytes.end.min(first_end);
        if !first_part.is_empty() {
            let transformed = first_style(style).map_or(style.text_transform, |s| s.text_transform);
            modes[first_part].fill(transformed);
        }
        previous_end = run.bytes.end;
    }
    let mut generated_end = 0;
    let mut source_run = 0;
    for generated in block.generated {
        if generated.bytes.start < generated_end
            || generated.bytes.start > generated.bytes.end
            || generated.bytes.end > modes.len()
        {
            return None;
        }
        while block
            .styled_text
            .get(source_run)
            .is_some_and(|run| run.bytes.end <= generated.bytes.start)
        {
            source_run += 1;
        }
        if block
            .styled_text
            .get(source_run)
            .is_some_and(|run| run.bytes.start < generated.bytes.end)
        {
            return None;
        }
        modes[generated.bytes.clone()].fill(generated.style.text_transform);
        covered[generated.bytes.clone()].fill(true);
        let first_part = generated.bytes.start..generated.bytes.end.min(first_end);
        if !first_part.is_empty() {
            let transformed = generated
                .host
                .and_then(|owner| styles.get(owner))
                .and_then(|owner| owner.first_line.as_deref())
                .filter(|line| line.host == block.element)
                .map(|line| line.apply(generated.style).text_transform)
                .unwrap_or(generated.style.text_transform);
            modes[first_part].fill(transformed);
        }
        generated_end = generated.bytes.end;
    }
    if block
        .text
        .bytes()
        .zip(&covered)
        .any(|(byte, covered)| byte.is_ascii_alphabetic() && !*covered)
    {
        return None;
    }
    if let Some(letter) = &block.first_letter {
        if letter.bytes.start > letter.bytes.end || letter.bytes.end > modes.len() {
            return None;
        }
        if block.generated.iter().any(|generated| {
            generated.bytes.start < letter.bytes.end && letter.bytes.start < generated.bytes.end
        }) {
            return None;
        }
        let style = styles.get(letter.owner)?;
        modes[letter.bytes.clone()].fill(letter.rule.apply(style).text_transform);
        let first_part = letter.bytes.start..letter.bytes.end.min(first_end);
        if !first_part.is_empty() {
            let first = first_style(style);
            modes[first_part].fill(
                letter
                    .rule
                    .apply(first.as_ref().unwrap_or(style))
                    .text_transform,
            );
        }
    }
    if modes.iter().all(|mode| *mode == TextTransform::None) {
        return None;
    }
    let mut starts = vec![false; modes.len()];
    if modes.contains(&TextTransform::Capitalize) {
        for range in crate::selection::word_ranges(block.text) {
            if let Some(start) = starts.get_mut(range.start) {
                *start = true;
            }
        }
    }
    let mut text = block.text.as_bytes().to_vec();
    for (index, byte) in text.iter_mut().enumerate() {
        *byte = match modes[index] {
            TextTransform::None => *byte,
            TextTransform::Uppercase => byte.to_ascii_uppercase(),
            TextTransform::Lowercase => byte.to_ascii_lowercase(),
            TextTransform::Capitalize if starts[index] => byte.to_ascii_uppercase(),
            TextTransform::Capitalize => *byte,
        };
    }
    String::from_utf8(text).ok()
}

// A same-length visual copy keeps every canonical byte boundary intact. Avoid
// synthetic/projection paths until a general many-to-many mapping is available.
fn bounded_transformed(
    block: &LayoutBlock<'_>,
    styles: &[Style],
    pass: first_line::Pass,
) -> Option<String> {
    if block.text.len() > MAX_TEXT_BYTES
        || (!matches!(
            &block.styled_text,
            crate::flow_view::StyledRuns::Original(_)
        ) && block.generated.is_empty())
        || !block.images.is_empty()
        || !block.ruby.is_empty()
        || !block.bidi_controls.is_empty()
    {
        return None;
    }
    if !block.generated.is_empty()
        || block.first_letter.is_some()
        || styles.get(block.element)?.first_line.is_some()
    {
        return pseudo_ascii_transformed(block, styles, pass);
    }
    if let Some(result) = contextual_transformed(block, styles) {
        return result;
    }
    let locale_sensitive = |language: Option<Language>| {
        language.is_some_and(|language| {
            [
                "tr", "tur", "az", "aze", "crh", "lt", "lit", "nl", "nld", "dut",
            ]
            .iter()
            .any(|tag| language.language().eq_ignore_ascii_case(tag))
        })
    };
    if locale_sensitive(block.language)
        || block
            .styled_text
            .iter()
            .any(|run| locale_sensitive(run.language))
    {
        return None;
    }
    let default = styles.get(block.element)?.text_transform;
    if default == TextTransform::None
        && block.styled_text.iter().all(|run| {
            styles
                .get(run.element)
                .is_some_and(|style| style.text_transform == TextTransform::None)
        })
    {
        return None;
    }
    if !block.text.is_ascii() {
        use icu_properties::{CodePointMapData, props::GeneralCategory};
        let category = CodePointMapData::<GeneralCategory>::new();
        if block.text.chars().any(|ch| {
            matches!(
                category.get(ch),
                GeneralCategory::NonspacingMark
                    | GeneralCategory::SpacingMark
                    | GeneralCategory::EnclosingMark
            )
        }) {
            return None;
        }
    }
    let needs_words = default == TextTransform::Capitalize
        || block.styled_text.iter().any(|run| {
            styles
                .get(run.element)
                .is_some_and(|style| style.text_transform == TextTransform::Capitalize)
        });
    let mut starts = vec![false; if needs_words { block.text.len() } else { 0 }];
    if needs_words {
        for range in crate::selection::word_ranges(block.text) {
            if let Some(start) = starts.get_mut(range.start) {
                *start = true;
            }
        }
    }
    let original = block.text.as_bytes();
    let mut previous_end = 0;
    for run in block.styled_text.iter() {
        if run.bytes.start < previous_end
            || run.bytes.end > original.len()
            || run.bytes.start > run.bytes.end
            || !block.text.is_char_boundary(run.bytes.start)
            || !block.text.is_char_boundary(run.bytes.end)
            || styles.get(run.element).is_none()
        {
            return None;
        }
        previous_end = run.bytes.end;
    }
    let mut result = original.to_vec();
    let mut run_index = 0;
    for (index, ch) in block.text.char_indices() {
        while block
            .styled_text
            .get(run_index)
            .is_some_and(|run| run.bytes.end <= index)
        {
            run_index += 1;
        }
        let transform = match block.styled_text.get(run_index) {
            Some(run) if run.bytes.contains(&index) => styles.get(run.element)?.text_transform,
            _ => default,
        };
        if transform == TextTransform::None {
            continue;
        }
        // Keep each changed scalar in its supported script repertoire. Other
        // scripts, combining sequences and locale-sensitive forms need richer
        // mapping than this one-to-one visual copy can provide.
        let repertoire = |value: char| {
            if value.is_ascii() {
                Some(0)
            } else if ('\u{00a0}'..='\u{024f}').contains(&value) {
                Some(1)
            } else if ('\u{0400}'..='\u{052f}').contains(&value) {
                Some(2)
            } else {
                None
            }
        };
        if !ch.is_ascii()
            && repertoire(ch).is_none()
            && !ch.is_whitespace()
            && !matches!(
                ch,
                '\u{2013}'
                    | '\u{2014}'
                    | '\u{2018}'
                    | '\u{2019}'
                    | '\u{201c}'
                    | '\u{201d}'
                    | '\u{2026}'
            )
        {
            return None;
        }
        let uppercase = match transform {
            TextTransform::None => continue,
            TextTransform::Uppercase => true,
            TextTransform::Lowercase => false,
            TextTransform::Capitalize if starts[index] => true,
            TextTransform::Capitalize => continue,
        };
        let first = if uppercase {
            let mut converted = ch.to_uppercase();
            let first = converted.next()?;
            if converted.next().is_some() {
                return None;
            }
            first
        } else {
            let mut converted = ch.to_lowercase();
            let first = converted.next()?;
            if converted.next().is_some() {
                return None;
            }
            first
        };
        if first.len_utf8() != ch.len_utf8()
            || (first != ch && (repertoire(ch).is_none() || repertoire(ch) != repertoire(first)))
        {
            return None;
        }
        let mut buf = [0; 4];
        let encoded = first.encode_utf8(&mut buf).as_bytes();
        result[index..index + encoded.len()].copy_from_slice(encoded);
    }

    String::from_utf8(result).ok()
}

fn shape_paragraph(
    context: &mut LayoutContext<PaintBrush>,
    fonts: &mut FontContext,
    block: &LayoutBlock<'_>,
    width: Option<u32>,
    settings: Settings,
    styles: &[Style],
    ruby: &[RubyCandidate],
    images: &[parley::InlineBox],
    fit_font_metrics: bool,
    indent: LineIndent,
    automatic: &crate::user_hyphenation::Opportunities,
    contour: Option<crate::float_layout::Contour>,
    pass: first_line::Pass,
) -> Layout<PaintBrush> {
    let style = &styles[block.element];
    let language = block.language;
    let trusted_words = if settings.trust_soft_hyphens && block.text.contains('\u{ad}') {
        crate::selection::word_ranges(&block.text)
            .filter(|range| block.text[range.clone()].contains('\u{ad}'))
            .take(4096)
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    // Build only visual break boundaries; never replace source characters.
    // Runs that cannot be mapped exactly are left on the normal line rules.
    let mut relaxed_nonbreaking = Vec::new();
    'runs: for run in block.styled_text.iter() {
        let Some(run_style) = styles.get(run.element) else {
            continue;
        };
        if !run_style.loose_nonbreaking || run_style.inline_nowrap {
            continue;
        }
        let Some(text) = block.text.get(run.bytes.clone()) else {
            continue;
        };
        for (offset, ch) in text.char_indices() {
            if matches!(ch, '\u{00a0}' | '\u{2011}') {
                let boundary = run.bytes.start + offset + ch.len_utf8();
                if boundary < block.text.len() {
                    relaxed_nonbreaking.push(boundary);
                    if relaxed_nonbreaking.len() > 4096 {
                        relaxed_nonbreaking.clear();
                        break 'runs;
                    }
                }
            }
        }
    }
    relaxed_nonbreaking.sort_unstable();
    relaxed_nonbreaking.dedup();
    let visual_text = if images.is_empty() {
        bounded_transformed(block, styles, pass)
    } else {
        None
    };
    let layout_text = visual_text.as_deref().unwrap_or(block.text);
    // Transformations may change byte positions, so only pass boundaries when
    // the layout is using the unchanged visible source text.
    let suppressed_soft_breaks = if visual_text.is_none() {
        short_words::suppressed(block, styles)
    } else {
        Vec::new()
    };
    let mut builder = context.ranged_builder(fonts, layout_text, 1.0, true);
    builder.set_bidi_controls(&block.bidi_controls);
    builder.set_relaxed_nonbreaking(&relaxed_nonbreaking);
    builder.set_suppressed_soft_breaks(&suppressed_soft_breaks);
    let font_weights = style.font_weights.clone();
    let weight_spacing = move |font: &parley::FontData, requested: f32, size: f32| {
        crate::font_synthesis::strength(font, requested, size, &font_weights)
    };
    builder.set_font_weight_spacing(Some(&weight_spacing));
    builder.set_automatic_hyphens(&automatic.breaks);
    builder.set_trusted_soft_hyphen_words(&trusted_words);
    let cjk_spacing = cjk_width::policy(&block.text, settings.cjk_width_percent);
    if let Some(policy) = &cjk_spacing {
        builder.set_cluster_spacing(Some(policy));
    }
    apply_kerning(&mut builder, settings.effective_font_kerning_mode());
    if !style.common_ligatures
        || style.ligature_extras != LigatureExtras::default()
        || style.variant_caps != VariantCaps::Normal
        || style.variant_numeric != VariantNumeric::default()
        || style.variant_east_asian != VariantEastAsian::default()
        || style.variant_position != VariantPosition::Normal
        || style.variant_historical
    {
        let features = shaping_with_variants(
            settings.effective_font_kerning_mode(),
            style.common_ligatures,
            style.ligature_extras,
            style.variant_caps,
            style.variant_numeric,
            style.variant_east_asian,
            style.variant_position,
            style.variant_historical,
        );
        builder.push_default(StyleProperty::FontFeatures(features.as_slice().into()));
    }
    let soft_hyphen = |offset| {
        let index = automatic
            .custom_words
            .partition_point(|range| range.end <= offset);
        if automatic
            .custom_words
            .get(index)
            .is_some_and(|range| range.contains(&offset))
        {
            return if automatic
                .matching_soft_hyphens
                .binary_search(&offset)
                .is_ok()
            {
                SoftHyphenAction::Draw
            } else {
                SoftHyphenAction::Suppress
            };
        }
        let index = block
            .styled_text
            .partition_point(|text| text.bytes.end <= offset);
        let style = block
            .styled_text
            .get(index)
            .filter(|text| text.bytes.contains(&offset))
            .map_or(style, |text| &styles[text.element]);
        if style.word_break == WordBreak::BreakAll || style.line_break == Some(LineBreak::Anywhere)
        {
            SoftHyphenAction::LeaveBoundary
        } else if style.hyphens != crate::book_style::Hyphens::None
            && !settings.disable_soft_hyphens
        {
            SoftHyphenAction::Draw
        } else {
            SoftHyphenAction::Suppress
        }
    };
    if block.text.contains('\u{ad}') {
        builder.set_soft_hyphen_policy(Some(&soft_hyphen));
    }
    let family: FontFamily<'_> = if settings.override_fonts {
        GenericFamily::Serif.into()
    } else {
        style.font_family.layout()
    };
    builder.push_default(StyleProperty::FontFamily(family));
    let line_height = |style: &Style| {
        let default_spacing = Settings::default().line_height;
        let publisher = style.line_height.unwrap_or_else(|| {
            style
                .normal_line_height_percent
                .map_or(default_spacing, |percent| f32::from(percent) / 100.0)
        });
        let minimum_spacing = if settings.line_spacing_percent.is_some()
            || style.line_height.is_none() && style.normal_line_height_percent.is_some()
        {
            0.5
        } else {
            1.0
        };
        let spacing = (publisher * settings.line_height_scale()).clamp(minimum_spacing, 3.0);
        if fit_font_metrics {
            LineHeight::MetricsRelative((spacing / default_spacing).max(1.05))
        } else {
            LineHeight::FontSizeRelative(spacing)
        }
    };
    let font_size = |style: &Style| style.font_pixels(settings);
    let letter_spacing = |style: &Style| effective_letter_spacing(style, settings);
    let weight = |style: &Style| {
        FontWeight::new(crate::font_render::font_weight(
            if settings.bold_text && style.font_weight < FontWeight::BOLD {
                FontWeight::BOLD.value()
            } else {
                style.font_weight.value()
            },
            settings.font_weight_delta,
        ))
    };
    builder.push_default(StyleProperty::LineHeight(line_height(style)));
    // An emergency break keeps a placed word within a narrow reading column.
    // Intrinsic table sizing must still see the word's whole natural width.
    if width.is_some() {
        builder.push_default(StyleProperty::OverflowWrap(OverflowWrap::Anywhere));
    }
    builder.push_default(StyleProperty::FontSize(font_size(style)));
    let base_optical = optical_axes(style, settings, font_size(style));
    if let Some(axes) = base_optical {
        builder.push_default(StyleProperty::FontVariations(axes.as_slice().into()));
    }
    if settings.percent_word_spacing.is_none() && settings.word_spacing != 0.0 {
        builder.push_default(StyleProperty::WordSpacing(settings.word_spacing));
    }
    if letter_spacing(style) != 0.0 {
        builder.push_default(StyleProperty::LetterSpacing(letter_spacing(style)));
    }
    builder.push_default(StyleProperty::Brush(PaintBrush {
        decoration: style.decoration,
        link_ink: style.link_ink,
        painted: style.flow.painted,
    }));
    builder.push_default(StyleProperty::FontWeight(weight(style)));
    builder.push_default(StyleProperty::FontStyle(style.font_style));
    builder.push_default(StyleProperty::Locale(language));
    // This cannot restore whitespace removed during source extraction. ODT's
    // explicit whitespace runs keep their existing break-spaces behavior.
    let base_break_spaces = style.white_space == WhiteSpace::BreakSpaces;
    if base_break_spaces {
        builder.push_default(StyleProperty::WhiteSpaceCollapse(
            parley::WhiteSpaceCollapse::BreakSpaces,
        ));
    }
    for run in block.styled_text.iter() {
        let break_spaces =
            run.preserve_whitespace || styles[run.element].white_space == WhiteSpace::BreakSpaces;
        if break_spaces != base_break_spaces {
            builder.push(
                StyleProperty::WhiteSpaceCollapse(if break_spaces {
                    parley::WhiteSpaceCollapse::BreakSpaces
                } else {
                    parley::WhiteSpaceCollapse::Preserve
                }),
                run.bytes.clone(),
            );
        }
    }

    let line_break_for = |style: &Style, language| {
        style
            .line_break
            .filter(|_| !style.inline_nowrap)
            .unwrap_or_else(|| line_break(language))
    };
    let word_break_for = |style: &Style| {
        if style.inline_nowrap {
            WordBreak::Normal
        } else {
            style.word_break
        }
    };
    let paragraph_line_break = line_break_for(style, language);
    let paragraph_word_break = word_break_for(style);
    builder.push_default(StyleProperty::LineBreak(paragraph_line_break));
    builder.push_default(StyleProperty::WordBreak(paragraph_word_break));
    if style.inline_nowrap {
        builder.push_default(StyleProperty::TextWrapMode(TextWrapMode::NoWrap));
    }
    let mut has_nowrap = style.inline_nowrap;
    builder.set_base_direction(style.direction);
    let letter = block.first_letter.as_ref();
    let letter_run = letter.map(|run| {
        let language = block
            .styled_text
            .iter()
            .find(|text| text.bytes.contains(&run.bytes.start))
            .and_then(|text| text.language)
            .or(language);
        (
            run.bytes.clone(),
            &styles[run.owner],
            language,
            Some(run.owner),
            Some(run.rule),
        )
    });
    let ordinary = block.styled_text.iter().flat_map(|text| {
        let style = &styles[text.element];
        let first = letter.map_or(text.bytes.end, |run| text.bytes.end.min(run.bytes.start));
        let after = letter.map_or(text.bytes.end, |run| text.bytes.start.max(run.bytes.end));
        [text.bytes.start..first, after..text.bytes.end]
            .into_iter()
            .filter(|range| !range.is_empty())
            .map(move |range| (range, style, text.language, Some(text.element), None))
    });
    let runs = ordinary
        .chain(
            block
                .generated
                .iter()
                .map(|run| (run.bytes.clone(), run.style, language, run.host, None)),
        )
        .chain(letter_run);
    let mut emit_run = |bytes: Range<usize>, run: &Style, run_language, base: &Style| {
        if run_language != language {
            builder.push(StyleProperty::Locale(run_language), bytes.clone());
        }
        let run_line_break = line_break_for(run, run_language);
        if run_line_break != paragraph_line_break {
            builder.push(StyleProperty::LineBreak(run_line_break), bytes.clone());
        }
        if run.decoration != base.decoration
            || run.link_ink != base.link_ink
            || run.flow.painted != base.flow.painted
        {
            builder.push(
                StyleProperty::Brush(PaintBrush {
                    decoration: run.decoration,
                    link_ink: run.link_ink,
                    painted: run.flow.painted,
                }),
                bytes.clone(),
            );
        }
        if run.text_rise != style.text_rise {
            builder.push(
                StyleProperty::VerticalAlign(run.text_rise.from_block_at(style, settings)),
                bytes.clone(),
            );
        }
        let run_word_break = word_break_for(run);
        if run_word_break != paragraph_word_break {
            builder.push(StyleProperty::WordBreak(run_word_break), bytes.clone());
        }
        if run.inline_nowrap != style.inline_nowrap {
            let mode = if run.inline_nowrap {
                TextWrapMode::NoWrap
            } else {
                TextWrapMode::Wrap
            };
            builder.push(StyleProperty::TextWrapMode(mode), bytes.clone());
            has_nowrap |= run.inline_nowrap;
        }
        if !settings.override_fonts && run.font_family != base.font_family {
            builder.push(
                StyleProperty::FontFamily(run.font_family.layout()),
                bytes.clone(),
            );
        }
        if run.size_scale != base.size_scale {
            builder.push(StyleProperty::FontSize(font_size(run)), bytes.clone());
        }
        if optical_axes(run, settings, font_size(run)).map(|axes| axes[0].value)
            != optical_axes(base, settings, font_size(base)).map(|axes| axes[0].value)
        {
            match optical_axes(run, settings, font_size(run)) {
                Some(axes) => builder.push(
                    StyleProperty::FontVariations(axes.as_slice().into()),
                    bytes.clone(),
                ),
                None => builder.push(
                    StyleProperty::FontVariations(parley::FontVariations::empty()),
                    bytes.clone(),
                ),
            }
        }
        if run.line_height != base.line_height {
            builder.push(StyleProperty::LineHeight(line_height(run)), bytes.clone());
        }
        if letter_spacing(run) != letter_spacing(base) {
            builder.push(
                StyleProperty::LetterSpacing(letter_spacing(run)),
                bytes.clone(),
            );
        }
        if run.font_weight != base.font_weight {
            builder.push(StyleProperty::FontWeight(weight(run)), bytes.clone());
        }
        if run.font_style != base.font_style {
            builder.push(StyleProperty::FontStyle(run.font_style), bytes.clone());
        }
        if run.common_ligatures != base.common_ligatures
            || run.ligature_extras != base.ligature_extras
            || run.variant_caps != base.variant_caps
            || run.variant_numeric != base.variant_numeric
            || run.variant_east_asian != base.variant_east_asian
            || run.variant_position != base.variant_position
            || run.variant_historical != base.variant_historical
        {
            let features = shaping_with_variants(
                settings.effective_font_kerning_mode(),
                run.common_ligatures,
                run.ligature_extras,
                run.variant_caps,
                run.variant_numeric,
                run.variant_east_asian,
                run.variant_position,
                run.variant_historical,
            );
            builder.push(
                StyleProperty::FontFeatures(features.as_slice().into()),
                bytes.clone(),
            );
        }
    };
    let first_end = pass.styled_end(block.text);
    let first_base = style
        .first_line
        .as_ref()
        .filter(|_| first_end > 0)
        .map(|line| line.apply(style));
    if let Some(first) = &first_base {
        emit_run(0..first_end, first, language, style);
    }
    for (mut bytes, run, run_language, owner, letter_rule) in runs {
        if let Some(first) = &first_base
            && bytes.start < first_end
        {
            let line = owner
                .and_then(|element| styles[element].first_line.as_deref())
                .filter(|line| line.host == block.element);
            let alternate = line.map(|line| line.apply(run));
            let alternate = alternate.as_ref().unwrap_or(run);
            let letter = letter_rule.map(|rule| rule.apply(alternate));
            let end = bytes.end.min(first_end);
            emit_run(
                bytes.start..end,
                letter.as_ref().unwrap_or(alternate),
                run_language,
                first,
            );
            bytes.start = end;
        }
        if !bytes.is_empty() {
            let letter = letter_rule.map(|rule| rule.apply(run));
            emit_run(bytes, letter.as_ref().unwrap_or(run), run_language, style);
        }
    }
    for annotation in ruby {
        builder.push(
            StyleProperty::LineHeight(LineHeight::Absolute(annotation.line_height)),
            annotation.bytes.clone(),
        );
    }
    if block.ruby.keep_bases_together {
        for annotation in block
            .ruby
            .iter()
            .filter(|annotation| annotation.bytes.len() <= MAX_RUBY_BASE_BYTES)
        {
            // A joined note can move the whole ruby base to the next line.
            // Preserve the emergency breaks for a base wider than the page.
            builder.push(
                StyleProperty::TextWrapMode(TextWrapMode::NoWrap),
                annotation.bytes,
            );
            has_nowrap = true;
        }
    }
    for image in images {
        builder.push_inline_box(image.clone());
    }
    let mut layout = builder.build(layout_text);
    if style.steady_line_heights
        && !fit_font_metrics
        && images.is_empty()
        && block.images.is_empty()
        && ruby.is_empty()
        && block.ruby.is_empty()
        && block.generated.is_empty()
        && block.first_letter.is_none()
        && style.first_line.is_none()
    {
        layout.set_confine_shifted_text(true);
    }
    if has_nowrap {
        layout.set_wrap_nowrap_on_overflow(true);
    }
    if let Some(pair) = settings.percent_word_spacing {
        let preserved: Vec<_> = block
            .styled_text
            .iter()
            .filter(|run| styles[run.element].preformatted_spaces)
            .map(|run| run.bytes.clone())
            .collect();
        layout.set_word_spacing_percent(&block.text, pair.scaling, pair.minimum, &preserved);
    }
    if indent.amount > 0.0 {
        layout.set_text_indent(
            indent.amount,
            parley::IndentOptions {
                hanging: indent.hanging,
                each_line: false,
            },
        );
    }
    let Some(width) = width else { return layout };
    let exact = pass.exact_break();
    if contour.is_some() || exact.is_some() || matches!(pass, first_line::Pass::Measure) {
        let mut breaker = layout.break_lines();
        breaker.state_mut().set_layout_max_advance(width as f32);
        if let Some(boundary) = exact {
            breaker.require_first_break(boundary.end, boundary.reason, boundary.hyphen);
        }
        let strut = match line_height(style) {
            LineHeight::FontSizeRelative(ratio) => ratio * font_size(style),
            LineHeight::Absolute(pixels) => pixels,
            // A metrics-relative retry uses the same nominal font sizes.
            LineHeight::MetricsRelative(ratio) => {
                ratio * font_size(style) * Settings::default().line_height
            }
        };
        while !breaker.is_done() {
            let start = breaker.committed_y();
            let (y, x, advance) = contour.map_or((start, 0., width as f32), |contour| {
                contour.for_line(start, strut)
            });
            let state = breaker.state_mut();
            state.set_line_y(y);
            state.set_line_x(x);
            state.set_line_max_advance(advance);
            breaker.break_next();
            if matches!(pass, first_line::Pass::Measure) {
                break;
            }
        }
        breaker.finish();
    } else {
        layout.break_all_lines(Some(width as f32));
    }
    if matches!(pass, first_line::Pass::Measure) {
        return layout;
    }
    if settings.hanging_punctuation
        && contour.is_none()
        && images.is_empty()
        && !style.preformatted_spaces
        && style.flow.painted
        && width.saturating_add(indent.common) == settings.text_width()
    {
        let rtl = layout.is_rtl();
        let cap = |px| (px as f32 * 0.8).floor();
        layout.set_hanging_punctuation(
            &block.text,
            [cap(settings.left_margin()), cap(settings.right_margin())],
            &|range, edge, discretionary| {
                let index = block
                    .styled_text
                    .partition_point(|run| run.bytes.end <= range.start);
                let unavailable = block.styled_text.get(index).is_some_and(|run| {
                    let style = &styles[run.element];
                    run.bytes.contains(&range.start)
                        && (style.preformatted_spaces || !style.flow.painted)
                });
                if unavailable {
                    return (0, false);
                }
                let Some(char) = block.text[range].chars().next() else {
                    return (0, false);
                };
                crate::hanging_punctuation::edge_rule(char, edge, rtl, discretionary)
            },
        );
    }
    let alignment = match settings.alignment {
        crate::TextAlignment::Publisher => style.alignment.unwrap_or(Alignment::Start),
        crate::TextAlignment::Start => Alignment::Start,
        crate::TextAlignment::Center => Alignment::Center,
        crate::TextAlignment::Justify => Alignment::Justify,
        crate::TextAlignment::FootnoteJustify => style.alignment.unwrap_or(if layout.is_rtl() {
            Alignment::Start
        } else {
            Alignment::Justify
        }),
    };
    layout.align_with_word_expansion(
        alignment,
        AlignmentOptions {
            last_line_alignment: if matches!(settings.alignment, crate::TextAlignment::Publisher) {
                style.last_line_alignment
            } else {
                None
            },
            ..AlignmentOptions::default()
        },
        &block.text,
        settings.word_expansion,
        &can_expand_word_atom,
    );
    layout
}

// Track alphabetic and numeric atoms in supported nonjoining scripts. Keep
// ligatures together and leave connected writing, spaces and ideographs alone.
fn can_expand_word_atom(atom: &str) -> bool {
    use icu_properties::CodePointMapData;
    use icu_properties::props::Script;
    atom.chars().next().is_some_and(|first| {
        if first.is_ascii() {
            return first.is_ascii_alphanumeric();
        }
        first.is_numeric()
            || (first.is_alphabetic()
                && matches!(
                    CodePointMapData::<Script>::new().get(first),
                    Script::Latin | Script::Greek | Script::Cyrillic | Script::Hebrew
                ))
    })
}

fn line_break(language: Option<Language>) -> LineBreak {
    // Parley's normal breaks enable locale-tailored CJK punctuation. Preserve its strict default
    // for undeclared language and other scripts.
    if language.is_some_and(|language| matches!(language.language(), "ja" | "zh")) {
        LineBreak::Normal
    } else {
        LineBreak::Strict
    }
}

struct ImageSizing {
    width: u32,
    height: u32,
    requested_width: Option<f64>,
    requested_height: Option<f64>,
    max_width: Option<f64>,
    max_height: Option<f64>,
    render_dpi: u16,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ImageKey<'a> {
    path: &'a str,
    requested: [Option<u64>; 2],
    maximum: [Option<u64>; 2],
    viewport: (u32, u32),
    render_dpi: u16,
}

#[cfg(test)]
mod dpi_tests {
    use super::*;

    #[test]
    fn image_maxima_match_stock_auto_and_definite_axis_rules() {
        use crate::book_style::ImageLength;

        let source = Source::parse(r#"<html><body><p><img src="x"/></p></body></html>"#).unwrap();
        let image = &source.blocks[0].images[0];
        let style = Style {
            image_max_width: Some(ImageLength::Pixels(160.0)),
            image_max_height: Some(ImageLength::Pixels(75.0)),
            ..Style::default()
        };
        let fit = |style: &Style| ImageSizing::new(image, Settings::default(), style, false, 1000);
        assert_eq!(fit(&style).target(400, 200), (150, 75));
        assert_eq!(fit(&style).target(60, 30), (60, 30));
        assert_eq!(
            fit(&Style {
                image_width: Some(ImageLength::Pixels(300.0)),
                ..style.clone()
            })
            .target(400, 200),
            (160, 75)
        );
        assert_eq!(
            fit(&Style {
                image_height: Some(ImageLength::Pixels(200.0)),
                image_max_height: None,
                ..style.clone()
            })
            .target(400, 200),
            (160, 200)
        );
        assert_eq!(
            fit(&Style {
                image_height: Some(ImageLength::Pixels(200.0)),
                ..style.clone()
            })
            .target(400, 200),
            (150, 75)
        );
        assert_eq!(
            fit(&Style {
                image_width: Some(ImageLength::Pixels(300.0)),
                image_height: Some(ImageLength::Pixels(200.0)),
                ..style.clone()
            })
            .target(400, 200),
            (160, 75)
        );
        assert_eq!(
            fit(&Style {
                image_max_height: Some(ImageLength::Fraction(0.03)),
                ..style.clone()
            })
            .target(400, 200),
            (160, 80)
        );
        assert_ne!(fit(&style).key("x"), fit(&Style::default()).key("x"));
        let inline = ImageSizing::new(
            image,
            Settings {
                height: 300,
                ..Settings::default()
            },
            &style,
            true,
            70,
        );
        assert_eq!(inline.target(400, 200), (70, 35));
    }

    #[test]
    fn dpi_scales_intrinsic_and_html_images_but_not_relative_constraints_and_bounds_every_target() {
        let source = Source::parse(
            r#"<html><body><p><img src="x" width="144" height="96"/></p></body></html>"#,
        )
        .unwrap();
        let image = &source.blocks[0].images[0];
        for dpi in crate::RENDER_DPI_PRESETS {
            let settings = Settings {
                render_dpi: dpi,
                ..Settings::default()
            };
            let scale = if dpi == 0 { 1. } else { f64::from(dpi) / 96. };
            let mut sizing = ImageSizing::new(image, settings, &Style::default(), false, 1000);
            assert_eq!(
                sizing.target(144, 96),
                ((144. * scale).round() as u32, (96. * scale).round() as u32)
            );
            let previous_key = sizing.key("x");
            sizing.requested_width = None;
            sizing.requested_height = None;
            assert_eq!(
                sizing.target(144, 96),
                ((144. * scale).round() as u32, (96. * scale).round() as u32)
            );
            assert_ne!(sizing.key("x"), previous_key);
            assert!(sizing.target(100_000, 100_000).0 <= 1000);
            assert!(sizing.target(1, 100_000).0 >= 1);
            let style = Style {
                image_width: Some(crate::book_style::ImageLength::Fraction(0.25)),
                image_height: Some(crate::book_style::ImageLength::Auto),
                ..Style::default()
            };
            let relative = ImageSizing::new(image, settings, &style, false, 1000);
            assert_eq!(relative.target(144, 96), (250, 167));
            let style = Style {
                image_width: Some(crate::book_style::ImageLength::Em(2.)),
                image_height: Some(crate::book_style::ImageLength::Auto),
                ..Style::default()
            };
            assert_eq!(
                ImageSizing::new(image, settings, &style, false, 1000).target(144, 96),
                (84, 56)
            );
        }
        let old = ImageSizing::new(image, Settings::default(), &Style::default(), false, 1000);
        let scaled = ImageSizing::new(
            image,
            Settings {
                render_dpi: 212,
                ..Settings::default()
            },
            &Style::default(),
            false,
            1000,
        );
        assert_ne!(old.key("x"), scaled.key("x"));
    }
}

impl ImageSizing {
    fn key<'a>(&self, path: &'a str) -> ImageKey<'a> {
        ImageKey {
            path,
            requested: [
                self.requested_width.map(f64::to_bits),
                self.requested_height.map(f64::to_bits),
            ],
            maximum: [
                self.max_width.map(f64::to_bits),
                self.max_height.map(f64::to_bits),
            ],
            viewport: (self.width, self.height),
            render_dpi: self.render_dpi,
        }
    }

    fn new(
        image: &EmbeddedImage,
        settings: Settings,
        style: &Style,
        inline: bool,
        width: u32,
    ) -> Self {
        // Inline artwork shares its line with a font strut below the baseline.
        let height = if inline {
            settings
                .text_height()
                .saturating_sub((settings.font_size * 2.0).ceil() as u32)
                .max(1)
        } else {
            settings.text_height()
        };
        let font_size = style.font_pixels(settings);
        let html_scale = if settings.render_dpi == 0 {
            1.
        } else {
            f32::from(settings.render_dpi) / 96.
        };
        let requested_width = style
            .image_width
            .map_or_else(
                || image.width.map(|px| px * html_scale),
                |value| value.pixels(width, font_size),
            )
            .filter(|v| v.is_finite())
            .map(f64::from);
        let requested_height = style
            .image_height
            .map_or_else(
                || image.height.map(|px| px * html_scale),
                |value| value.pixels(height, font_size),
            )
            .filter(|v| v.is_finite())
            .map(f64::from);
        let max_width = style
            .image_max_width
            .and_then(|value| value.pixels(width, font_size))
            .filter(|v| v.is_finite() && *v > 0.0)
            .map(f64::from);
        // The reflow body has no definite containing-block height. Its safe
        // viewport limit is independent of a percentage CSS height constraint.
        let max_height = style
            .image_max_height
            .filter(|value| !matches!(value, crate::book_style::ImageLength::Fraction(_)))
            .and_then(|value| value.pixels(height, font_size))
            .filter(|v| v.is_finite() && *v > 0.0)
            .map(f64::from);
        Self {
            width,
            height,
            requested_width,
            requested_height,
            max_width,
            max_height,
            render_dpi: settings.render_dpi,
        }
    }

    fn target(&self, original_w: u32, original_h: u32) -> (u32, u32) {
        let intrinsic_scale = if self.render_dpi == 0 {
            1.
        } else {
            f64::from(self.render_dpi) / 96.
        };
        let ratio = f64::from(original_w) / f64::from(original_h);
        let max_width = self.max_width.unwrap_or(f64::INFINITY);
        let max_height = self.max_height.unwrap_or(f64::INFINITY);
        let (w, h) = match (self.requested_width, self.requested_height) {
            // If the width is definite, the height cap does not change it.
            (Some(width), height) => {
                let width = width.min(max_width);
                let height = height.unwrap_or(width / ratio).min(max_height);
                (width, height)
            }
            (None, Some(height)) => {
                let height = height.min(max_height);
                ((height * ratio).min(max_width), height)
            }
            (None, None) => {
                let width = f64::from(original_w) * intrinsic_scale;
                let height = width / ratio;
                let scale = (max_width / width).min(max_height / height).min(1.0);
                (width * scale, height * scale)
            }
        };
        let scale = (f64::from(self.width) / w)
            .min(f64::from(self.height) / h)
            .min(1.0);
        (
            (w * scale).round().clamp(1.0, f64::from(self.width)) as u32,
            (h * scale).round().clamp(1.0, f64::from(self.height)) as u32,
        )
    }
}

fn load_image(
    book: &mut Book,
    chapter_href: &str,
    image: &EmbeddedImage,
    settings: Settings,
    sizing: ImageSizing,
    cache_pixels: usize,
) -> Option<(String, GrayImage, (u32, u32))> {
    let width = sizing.width;
    let height = sizing.height;
    let target = |original_w, original_h| sizing.target(original_w, original_h);
    let styled_size = (sizing.requested_width.is_some()
        || sizing.requested_height.is_some()
        || sizing.max_width.is_some()
        || sizing.max_height.is_some()
        || !matches!(sizing.render_dpi, 0 | 96))
    .then_some(&target as &dyn Fn(u32, u32) -> (u32, u32));
    let filter = if settings.smooth_images {
        FilterType::Triangle
    } else {
        FilterType::Nearest
    };
    let preview_size = |w: u32, h: u32| {
        let scale = ((cache_pixels as f64) / (u64::from(w) * u64::from(h)) as f64)
            .sqrt()
            .min(1.0);
        let cw = ((w as f64 * scale) as u32)
            .clamp(1, w)
            .min(cache_pixels as u32);
        let ch = ((h as f64 * scale) as u32)
            .clamp(1, h)
            .min((cache_pixels / cw as usize) as u32);
        (cw, ch)
    };
    let cache_vector = |path, pixels: GrayImage| {
        let display = pixels.dimensions();
        let (cw, ch) = preview_size(display.0, display.1);
        let preview = if (cw, ch) == display {
            pixels
        } else {
            image::imageops::resize(&pixels, cw, ch, filter)
        };
        Some((path, preview, display))
    };
    if let Some(xml) = &image.inline_svg {
        let (art, _) = svg_image::render_inline(
            book,
            chapter_href,
            xml,
            width,
            height,
            MAX_IMAGE_PIXELS,
            styled_size,
        )
        .ok()?;
        return cache_vector(image.href.clone(), art);
    }
    let image_href = &image.href;
    let path = Book::resolve(chapter_href, image_href).ok()?;
    if svg_image::is_package_svg(&path) {
        if image_href
            .split_once('#')
            .is_some_and(|(_, fragment)| !fragment.is_empty())
        {
            return None;
        }
        let bytes = book
            .resource_limited(&path, svg_image::MAX_SVG_BYTES)
            .ok()?;
        let (art, _) = svg_image::render_inline(
            book,
            &path,
            std::str::from_utf8(&bytes).ok()?,
            width,
            height,
            MAX_IMAGE_PIXELS,
            styled_size,
        )
        .ok()?;
        return cache_vector(path, art);
    }
    let bytes = book.resource(&path).ok()?;
    let (w, h) = crate::epub_raster_dimensions(&bytes).ok()?;
    // A decode is transient. The cache retains only the display-sized grayscale preview.
    if u64::from(w) * u64::from(h) > MAX_IMAGE_PIXELS as u64 {
        return None;
    }
    let display = target(w, h);
    let stored = preview_size(display.0, display.1);
    let mut image = if crate::gif_image::has_signature(&bytes) {
        DynamicImage::ImageLuma8(crate::gif_image::gray(&bytes).ok()?)
    } else {
        image_reader(&bytes).ok()?.decode().ok()?
    };
    if stored != (w, h) {
        image = image.resize_exact(stored.0, stored.1, filter);
    }
    Some((path, grayscale_image(image), display))
}

pub(crate) fn original_illustration(bytes: &[u8]) -> Result<ImagePage> {
    if crate::gif_image::has_signature(bytes) {
        return ImagePage::decode_gif(bytes);
    }
    let gray = match image_reader(bytes) {
        Ok(reader) => {
            crate::epub_raster_dimensions(bytes)?;
            grayscale_image(reader.decode()?)
        }
        Err(_) => svg_image::rasterize(
            bytes,
            svg_image::MAX_SVG_SIDE,
            svg_image::MAX_SVG_SIDE,
            svg_image::MAX_SVG_PIXELS,
        )?,
    };
    ImagePage::from_grayscale(gray.width(), gray.height(), gray.into_raw())
}

fn grayscale_image(image: DynamicImage) -> GrayImage {
    if let DynamicImage::ImageLuma8(gray) = image {
        return gray;
    }
    let luma = image.into_luma_alpha8();
    GrayImage::from_fn(luma.width(), luma.height(), |x, y| {
        let [tone, opacity] = luma.get_pixel(x, y).0.map(u16::from);
        Luma([((tone * opacity + 255 * (255 - opacity) + 127) / 255) as u8])
    })
}

impl Illustration {
    fn origin(&self, start: f32, columns: &Range<usize>) -> (usize, i32) {
        let remaining = columns.len() - self.display.0 as usize;
        let left = columns.start + self.x.map_or(remaining / 2, |x| x.min(remaining));
        (left, (self.y - start).round() as i32)
    }

    fn draw(&self, page: &mut [u8], range: &Range<f32>, stride: usize, columns: &Range<usize>) {
        if !self.painted {
            return;
        }
        let (x, top) = self.origin(range.start, columns);
        let height = (page.len() / stride) as i32;
        let first = (-top).max(0);
        let last = (height - top).min(self.display.1 as i32);
        let width = self.display.0 as usize;
        let stored = self.pixels.width() as usize;
        for row in first..last {
            let source_row = row as u32 * self.pixels.height() / self.display.1;
            let source = &self.pixels.as_raw()[source_row as usize * stored..][..stored];
            let start = (top + row) as usize * stride + x;
            let dest = &mut page[start..start + width];
            if width == stored {
                dest.copy_from_slice(source);
            } else {
                for (x, out) in dest.iter_mut().enumerate() {
                    *out = source[x * stored / width];
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::book_style::BookStyle;

    #[test]
    fn authored_lists_paint_marker_gutters_and_keep_search_selection_and_source_anchors() {
        let original = include_str!("../tests/fixtures/publisher-lists.xhtml");
        let fonts = [crate::test_fixture("fonts/NotoSans-Regular.ttf")];
        let settings = Settings {
            width: 610,
            height: 900,
            font_size: 29.0,
            margin: 30,
            ..Settings::default()
        };
        let fixture = crate::test_fixture("field-notes-epub3.epub");
        let open = |s: &str| {
            crate::Probe::from_book_content(
                Book::open(&fixture).unwrap(),
                crate::Engine::Parley,
                &fonts,
                settings,
                0,
                Some(s.to_owned()),
            )
            .unwrap()
        };
        let mut probe = open(original);
        let altered = original.replace("start=\"8\"", "start=\"7\"");
        let mut alternate = open(&altered);
        assert_ne!(probe.capture(0).unwrap(), alternate.capture(0).unwrap());
        assert_eq!(probe.coverage().0, probe.coverage().1);
        assert_eq!(probe.matches("13. Record").len(), 1);
        assert!(probe.matches("8. Carry").is_empty());
        let crate::Renderer::Parley(engine) = &probe.renderer else {
            panic!()
        };
        // First item, next item, two inner bullets and the two reversed alphabetical items.
        // Continued paragraph, author-supplied ordinal and CSS-suppressed list are unmarked.
        assert_eq!(engine.markers.len(), 6);
        let first = &engine.paragraphs[1];
        let nested = engine
            .paragraphs
            .iter()
            .find(|p| p.key == probe.matches("Yellow flag")[0].block)
            .unwrap();
        assert!(first.x > 25.0 && nested.x > first.x);
        assert_eq!(engine.paragraphs[2].x, first.x);
        let start = probe.matches("lantern")[0].clone();
        let end = crate::Anchor {
            raw_caret: None,
            offset: start.offset + "lantern".len(),
            ..start.clone()
        };
        let selection = probe.select_range(&start, &end).unwrap();
        let page = probe.page_for(&start).unwrap();
        let rect = probe.selection_rects(page, &selection)[0];
        assert_eq!(
            probe
                .word_at(
                    page,
                    (rect.x + rect.width / 2) as f32,
                    (rect.y + rect.height / 2) as f32
                )
                .unwrap()
                .text,
            "lantern"
        );
        assert!(
            probe
                .word_at(
                    page,
                    settings.side_margin() as f32 + first.x / 3.0,
                    (rect.y + rect.height / 2) as f32
                )
                .is_none()
        );
        let moved = crate::Probe::from_book_content(
            Book::open(&fixture).unwrap(),
            crate::Engine::Parley,
            &fonts,
            Settings {
                font_size: 40.0,
                ..settings
            },
            0,
            Some(original.to_owned()),
        )
        .unwrap();
        assert!(moved.page_for(&selection.start).is_some());
        assert_eq!(
            moved
                .select_range(&selection.start, &selection.end)
                .unwrap()
                .text,
            selection.text
        );
    }

    #[test]
    fn rtl_list_markers_remain_outside_source_text_and_bullets_survive_missing_font_symbols() {
        let markup = r#"<html><body><ol dir="rtl"><li>East wind</li></ol><ul><li>North</li></ul></body></html>"#;
        let fonts = [crate::test_fixture("fonts/NotoSans-Regular.ttf")];
        let settings = Settings {
            width: 580,
            height: 580,
            font_size: 30.0,
            margin: 25,
            base_stylesheet: crate::BaseStylesheet::None,
            ..Settings::default()
        };
        let fixture = crate::test_fixture("field-notes-epub3.epub");
        let probe = crate::Probe::from_book_content(
            Book::open(&fixture).unwrap(),
            crate::Engine::Parley,
            &fonts,
            settings,
            0,
            Some(markup.to_owned()),
        )
        .unwrap();
        let crate::Renderer::Parley(engine) = &probe.renderer else {
            panic!()
        };
        let text = &engine.paragraphs[0];
        assert!(text.layout.is_rtl());
        assert_eq!(text.x, 0.0);
        assert!(engine.markers[0].x > settings.text_width() as f32 * 0.75);
        for (kind, center, corner) in [
            (crate::list_marker::MarkerKind::Disc, 0, 255),
            (crate::list_marker::MarkerKind::Circle, 255, 255),
            (crate::list_marker::MarkerKind::Square, 0, 0),
        ] {
            let mut image = vec![255; 14 * 14];
            draw_bullet(&mut image, 14, 2.0, 2.0, 10.0, kind);
            assert_eq!(image[7 * 14 + 7], center);
            assert_eq!(image[2 * 14 + 2], corner);
            assert!(image[2 * 14 + 7] < 128);
        }
    }

    #[test]
    fn publisher_paragraph_indents_preserve_source_hits_search_and_saved_positions() {
        let original = include_str!("../tests/fixtures/paragraph-indents.xhtml");
        let settings = Settings {
            width: 600,
            height: 800,
            font_size: 32.0,
            margin: 24,
            base_stylesheet: crate::BaseStylesheet::None,
            ..Settings::default()
        };
        let fonts = [crate::test_fixture("fonts/NotoSans-Regular.ttf")];
        let book = crate::test_fixture("field-notes-epub3.epub");
        let open = |publisher_styles| {
            crate::Probe::from_book_content(
                Book::open(&book).unwrap(),
                crate::Engine::Parley,
                &fonts,
                Settings {
                    publisher_styles,
                    ..settings
                },
                0,
                Some(original.to_owned()),
            )
            .unwrap()
        };
        let mut styled = open(true);
        let mut plain = open(false);
        assert_ne!(styled.capture(0).unwrap(), plain.capture(0).unwrap());
        assert_eq!(styled.coverage(), plain.coverage());
        assert_eq!(styled.coverage().0, styled.coverage().1);
        for (id, word, first, rest) in [
            ("tide", "tide", 48.0, 0.0),
            ("pier", "pier", 0.0, 0.0),
            ("signal", "signal", 55.2, 0.0),
            ("weather", "Weather", 0.0, 32.0),
        ] {
            let saved = plain.fragment(id).unwrap();
            assert_eq!(styled.fragment(id).unwrap(), saved);
            assert!(styled.page_for(&saved).is_some());
            assert_eq!(plain.matches(word), styled.matches(word));
            let start = styled.matches(word).remove(0);
            let end = crate::Anchor {
                raw_caret: None,
                offset: start.offset + word.len(),
                ..start.clone()
            };
            let selection = styled.select_range(&start, &end).unwrap();
            assert_eq!(selection.text, word);
            let page = styled.page_for(&start).unwrap();
            let rect = styled.selection_rects(page, &selection)[0];
            assert_eq!(
                styled
                    .word_at(
                        page,
                        (rect.x + rect.width / 2) as f32,
                        (rect.y + rect.height / 2) as f32
                    )
                    .unwrap()
                    .text,
                word
            );
            let crate::Renderer::Parley(engine) = &styled.renderer else {
                panic!()
            };
            let p = engine
                .paragraphs
                .iter()
                .find(|p| p.key == start.block)
                .unwrap();
            for (line, expected) in [(0, first), (1, rest), (2, rest)] {
                let x = p.x + p.layout.get(line).unwrap().metrics().offset;
                assert!(
                    (x - expected).abs() <= 0.6,
                    "{id} line {line} starts at {x}, expected {expected}"
                );
            }
        }
    }

    #[test]
    fn signed_paragraph_indents_follow_the_cascade_and_stay_inside_the_column() {
        let filler =
            "Harbour lights reflected off the quiet water while the boat was waiting. ".repeat(6);
        let markup = format!(
            r#"<html style="font-size:1.5em"><body style="font-size:1em; text-indent:2em">
            <p style="font-size:0.5em">{filler}</p>
            <p style="text-indent:initial">{filler}</p>
            <p style="text-indent:1rem">{filler}</p>
            <p style="text-indent:0; text-indent:inherit">{filler}</p>
            <p style="text-indent:-1em">{filler}</p>
            <p style="text-indent:hanging -1em">{filler}</p>
            <p style="text-indent:1pt !important; text-indent:0">{filler}</p>
            <p style="text-indent:12px; text-indent:3foo">{filler}</p>
            <p style="text-indent:40000%">{filler}</p>
            <p dir="rtl" style="text-indent:24px">{filler}</p>
        </body></html>"#
        );
        let fonts = [crate::test_fixture("fonts/NotoSans-Regular.ttf")];
        let book = crate::test_fixture("field-notes-epub3.epub");
        let settings = Settings {
            width: 600,
            height: 800,
            font_size: 32.0,
            margin: 24,
            base_stylesheet: crate::BaseStylesheet::None,
            ..Settings::default()
        };
        let p = crate::Probe::from_book_content(
            Book::open(&book).unwrap(),
            crate::Engine::Parley,
            &fonts,
            settings,
            0,
            Some(markup),
        )
        .unwrap();
        assert_eq!(p.coverage().0, p.coverage().1);
        let crate::Renderer::Parley(engine) = &p.renderer else {
            panic!()
        };
        assert_eq!(engine.paragraphs.len(), 10);
        for (p, (first, rest)) in engine.paragraphs.iter().zip([
            (96.0, 0.0),
            (0.0, 0.0),
            (48.0, 0.0),
            (96.0, 0.0),
            (0.0, 48.0),
            (48.0, 0.0),
            (96.0 / 72.0, 0.0),
            (12.0, 0.0),
            (520.0, 0.0),
        ]) {
            for (line, expected) in [(0, first), (1, rest)] {
                assert!(
                    (p.layout.get(line).unwrap().metrics().offset + p.x - expected).abs() <= 0.6,
                    "line {line} expected {expected}"
                );
            }
        }
        let rtl = &engine.paragraphs[9];
        assert!(rtl.layout.is_rtl());
        for (line, inset) in [(0, 24.0), (1, 0.0)] {
            let m = rtl.layout.get(line).unwrap();
            let right = m.metrics().offset + m.metrics().advance;
            assert!(
                (right + inset - settings.text_width() as f32).abs() < 1.0,
                "rtl line {line} ends at {right}"
            );
        }
    }

    #[test]
    fn tight_publisher_leading_preserves_long_paragraphs_and_every_page_anchor() {
        let words = "Light from the harbour reached the shore. ".repeat(90);
        let markup = format!("<html><body><p style='line-height: 1'>{words}</p></body></html>");
        let settings = Settings {
            width: 540,
            height: 490,
            font_size: 38.0,
            margin: 28,
            ..Settings::default()
        };
        let fonts = [crate::test_fixture("fonts/NotoSans-Regular.ttf")];
        let path = crate::test_fixture("field-notes-epub3.epub");
        let mut probe = crate::Probe::from_book_content(
            Book::open(&path).unwrap(),
            crate::Engine::Parley,
            &fonts,
            settings,
            0,
            Some(markup),
        )
        .unwrap();
        assert!(probe.page_count() > 5);
        let (visible, total) = probe.coverage();
        assert!(total > 2_000);
        assert_eq!(visible, total);
        for page in 0..probe.page_count() {
            let anchor = probe.page_anchor(page).unwrap();
            assert_eq!(probe.page_for(&anchor), Some(page));
            assert!(probe.capture(page).unwrap().iter().any(|&gray| gray < 180));
        }
    }

    #[test]
    fn terminal_breaks_keep_end_anchors_on_the_last_painted_page() {
        let path = crate::test_fixture("field-notes-epub3.epub");
        let fonts = [crate::test_fixture("fonts/NotoSans-Regular.ttf")];
        let settings = Settings {
            width: 240,
            height: 200,
            font_size: 22.,
            margin: 10,
            ..Settings::default()
        };
        let markup = format!(
            "<html><body><p>First.<br/>{}<span id='last'>Last.</span>{}<span id='end'/></p></body></html>",
            "Body line.<br/>".repeat(8),
            "<br/>".repeat(17)
        );
        let mut probe = crate::Probe::from_book_content(
            Book::open(&path).unwrap(),
            crate::Engine::Parley,
            &fonts,
            settings,
            0,
            Some(markup),
        )
        .unwrap();
        let last = probe.page_count() - 1;
        assert!(last > 0);
        assert_eq!(probe.page_for(&probe.fragment("last").unwrap()), Some(last));
        assert_eq!(probe.page_for(&probe.fragment("end").unwrap()), Some(last));
        for page in 0..=last {
            assert!(probe.capture(page).unwrap().iter().any(|&gray| gray < 180));
            assert_eq!(
                probe.page_for(&probe.page_anchor(page).unwrap()),
                Some(page)
            );
        }
        let blank = crate::Probe::from_book_content(
            Book::open(&path).unwrap(),
            crate::Engine::Parley,
            &fonts,
            settings,
            0,
            Some("<html><body><p> </p></body></html>".into()),
        )
        .unwrap();
        assert_eq!(blank.page_count(), 1);
    }

    #[test]
    fn word_expansion_keeps_line_breaks_and_selection_geometry() {
        use parley::Cluster;

        let path = crate::test_fixture("field-notes-epub3.epub");
        let fonts = [crate::test_fixture("fonts/NotoSans-Regular.ttf")];
        let markup = "<html><body><p>Tide pools drift. Longshorewatch observes the moon. Sparse notes.</p></body></html>";
        let run = |word_expansion| {
            crate::Probe::from_book_content(
                Book::open(&path).unwrap(),
                crate::Engine::Parley,
                &fonts,
                Settings {
                    width: 420,
                    height: 460,
                    margin: 0,
                    font_size: 38.0,
                    alignment: crate::TextAlignment::Justify,
                    word_expansion,
                    disable_soft_hyphens: true,
                    automatic_hyphenation: false,
                    ..Settings::default()
                },
                0,
                Some(markup.into()),
            )
            .unwrap()
        };
        let compare = |probe: &crate::Probe| {
            let Renderer::Parley(engine) = &probe.renderer else {
                unreachable!()
            };
            let paragraph = &engine.paragraphs[0];
            let layout = &paragraph.layout;
            let text = &probe.source.blocks[0].text;
            let first_word = 0.."Tide".len();
            let cluster = |at| Cluster::from_byte_index(layout, at).unwrap().advance();
            let gaps = (cluster(1), cluster(first_word.end));
            let lines: Vec<_> = layout.lines().map(|line| line.text_range()).collect();
            let last = lines.last().unwrap().clone();
            let last_advances: Vec<_> = text[last.clone()]
                .char_indices()
                .map(|(offset, _)| cluster(last.start + offset))
                .collect();
            let choice = probe
                .select_range(
                    &probe.anchor(paragraph.key, first_word.start),
                    &probe.anchor(paragraph.key, first_word.end),
                )
                .unwrap();
            let rect = probe.selection_rects(0, &choice)[0];
            let selected = probe
                .word_at(
                    0,
                    (rect.x + rect.width / 2) as f32,
                    (rect.y + rect.height / 2) as f32,
                )
                .unwrap();
            assert_eq!(selected.text, "Tide");
            (gaps, lines, last_advances, choice, rect.width)
        };
        let mut spaced = run(15);
        let mut baseline = run(0);
        let ((base_letter, base_space), base_lines, base_last, before, base_width) =
            compare(&baseline);
        let ((wide_letter, narrow_space), expanded_lines, expanded_last, after, expanded_width) =
            compare(&spaced);
        assert_eq!(base_lines, expanded_lines);
        assert_eq!(base_last, expanded_last);
        assert_eq!(before, after);
        assert!(wide_letter > base_letter && wide_letter - base_letter <= 5.0);
        assert!(narrow_space < base_space && expanded_width > base_width);
        assert_ne!(baseline.capture(0).unwrap(), spaced.capture(0).unwrap());
        assert!(can_expand_word_atom("fí"));
        assert!(!can_expand_word_atom("ن") && !can_expand_word_atom("天"));
    }

    #[test]
    fn fractional_leading_extending_beyond_the_last_page_restores_the_last_visible_line() {
        let path = crate::test_fixture("field-notes-epub3.epub");
        let fonts = [crate::test_fixture("fonts/NotoSans-Regular.ttf")];
        let markup = "<html><body><p>First line.<br/>Second line.<br/>Third line.<br/><span id='last'>Fourth line.</span></p></body></html>";
        let settings = Settings {
            width: 280,
            height: 240,
            margin: 0,
            font_size: 25.,
            line_height: 1.8,
            ..Settings::default()
        };
        let mut probe = crate::Probe::from_book_content(
            Book::open(&path).unwrap(),
            crate::Engine::Parley,
            &fonts,
            settings,
            0,
            Some(markup.into()),
        )
        .unwrap();
        let crate::Renderer::Parley(chapter) = &probe.renderer else {
            unreachable!()
        };
        let paint_end = chapter.last_paint_bottom(&probe.source);
        let nominal_end = chapter.paragraphs.last().unwrap().bottom;
        let height = paint_end.ceil();
        assert!(height < nominal_end);
        let mut pages = crate::Pagination::new(&probe.lines, height, nominal_end).unwrap();
        assert_eq!(pages.len(), 2);
        pages.trim_unpainted_tail(paint_end);
        assert_eq!(pages.len(), 1);
        probe.pages = pages;
        assert_eq!(probe.page_for(&probe.fragment("last").unwrap()), Some(0));
        assert!(probe.capture(0).unwrap().iter().any(|&gray| gray < 180));
    }

    #[test]
    fn trailing_shaded_breaks_remain_visible() {
        let path = crate::test_fixture("field-notes-epub3.epub");
        let fonts = [crate::test_fixture("fonts/NotoSans-Regular.ttf")];
        let settings = Settings {
            width: 240,
            height: 200,
            font_size: 22.,
            margin: 10,
            ..Settings::default()
        };
        let markup = format!(
            "<html><body><p style='background-color:#aaa'>Start.{}<span id='end'/></p></body></html>",
            "<br/>".repeat(17)
        );
        let mut probe = crate::Probe::from_book_content(
            Book::open(&path).unwrap(),
            crate::Engine::Parley,
            &fonts,
            settings,
            0,
            Some(markup),
        )
        .unwrap();
        let last = probe.page_count() - 1;
        assert!(last > 0);
        assert!(probe.capture(last).unwrap().iter().any(|&gray| gray < 220));
    }

    #[test]
    fn text_contrast_darkens_glyphs_without_reflowing_the_page() {
        let book = crate::test_fixture("long-chapter.epub");
        let fonts = [crate::test_fixture("fonts/Silkscreen-Regular.ttf")];
        let settings = Settings::default();
        let mut normal =
            crate::Probe::open(&book, crate::Engine::Parley, &fonts, settings, 0).unwrap();
        let mut darker = crate::Probe::open(
            &book,
            crate::Engine::Parley,
            &fonts,
            Settings {
                text_contrast: true,
                ..settings
            },
            0,
        )
        .unwrap();
        assert_eq!(normal.page_count(), darker.page_count());
        assert_eq!(normal.page_anchor(0), darker.page_anchor(0));
        let normal = normal.capture(0).unwrap();
        let darker = darker.capture(0).unwrap();
        assert!(darker.iter().zip(&normal).all(|(new, old)| new <= old));
        assert!(darker.iter().zip(&normal).any(|(new, old)| new < old));
    }

    #[test]
    fn new_spacing_presets_change_text_advances_with_the_same_source_offsets() {
        let source =
            Source::parse("<html><body><p>Two words for spacing.</p></body></html>").unwrap();
        let mut book = Book::open(&crate::test_fixture("field-notes-epub3.epub")).unwrap();
        let href = book.chapters[0].href.clone();
        let mut fonts =
            crate::font_context(&[crate::test_fixture("fonts/Silkscreen-Regular.ttf")]).unwrap();
        let styles =
            BookStyle::load(&mut book, &href, &source, &mut fonts).compute(&source.elements);
        let mut context = LayoutContext::new();
        let mut sample = |settings| {
            let (layout, _) = layout_paragraph(
                &mut context,
                &mut fonts,
                &source.blocks[0],
                1100,
                settings,
                &styles,
            );
            let line = layout.get(0).unwrap();
            (line.metrics().advance, line.text_range())
        };
        let settings = Settings::default();
        let (normal, source_bytes) = sample(settings);
        for wider in [
            Settings {
                word_spacing: 8.0,
                ..settings
            },
            Settings {
                letter_spacing: 2.0,
                ..settings
            },
        ] {
            let (advance, bytes) = sample(wider);
            assert!(advance > normal);
            assert_eq!(bytes, source_bytes);
        }
    }

    #[test]
    fn dictionary_break_paints_both_the_last_letter_and_hyphen_and_preserves_anchors() {
        use skrifa::{FontRef, MetadataProvider as _};

        let book = crate::test_fixture("english-hyphenation.epub");
        let fonts = [crate::test_fixture("fonts/NotoSans-Regular.ttf")];
        let open = |automatic_hyphenation, width| {
            Probe::open(
                &book,
                Engine::Parley,
                &fonts,
                Settings {
                    width,
                    height: 480,
                    margin: 15,
                    font_size: 26.0,
                    automatic_hyphenation,
                    ..Settings::default()
                },
                0,
            )
            .unwrap()
        };
        let inspect = |probe: &Probe, word: &crate::Anchor| {
            let Renderer::Parley(engine) = &probe.renderer else {
                unreachable!()
            };
            let paragraph = engine
                .paragraphs
                .iter()
                .find(|p| p.key == word.block)
                .unwrap();
            let mut spans = Vec::new();
            for line in paragraph.layout.lines() {
                let end = line.text_range().end;
                let mut glyphs = Vec::new();
                for item in line.items() {
                    let PositionedLayoutItem::GlyphRun(run) = item else {
                        continue;
                    };
                    let font = run.run().font();
                    let face = FontRef::from_index(font.data.data(), font.index).unwrap();
                    let map = face.charmap();
                    let hyphen = map.map('‐').or_else(|| map.map('-')).unwrap().to_u32();
                    let preceding = match end.checked_sub(word.offset) {
                        Some(2) => 'y',
                        Some(7) => 'a',
                        _ => 'n',
                    };
                    let expected = map.map(preceding).unwrap().to_u32();
                    glyphs.extend(
                        run.positioned_glyphs()
                            .map(|g| (g.id == expected, g.id == hyphen)),
                    );
                }
                if [word.offset + 2, word.offset + 6, word.offset + 7].contains(&end) {
                    assert!(line.metrics().advance <= probe.settings.text_width() as f32 + 0.1);
                    assert!(
                        glyphs.len() >= 2 && glyphs[glyphs.len() - 2].0 && glyphs.last().unwrap().1
                    );
                    spans.push(end);
                }
            }
            spans
        };
        let mut auto = open(true, 234);
        let mut authored = open(false, 234);
        let start = auto.matches("hyphenation").remove(0);
        let end = crate::Anchor {
            raw_caret: None,
            offset: start.offset + "hyphenation".len(),
            ..start.clone()
        };
        assert!(!inspect(&auto, &start).is_empty());
        assert!(inspect(&authored, &start).is_empty());
        assert_ne!(auto.capture(0).unwrap(), authored.capture(0).unwrap());
        let wide = open(true, 720);
        assert!(inspect(&wide, &start).is_empty());
        let saved = serde_json::to_vec(&start).unwrap();
        for probe in [&auto, &authored, &wide] {
            assert_eq!(probe.coverage().0, probe.coverage().1);
            assert_eq!(probe.matches("hyphenation")[0], start);
            let page = probe
                .page_for(&serde_json::from_slice(&saved).unwrap())
                .unwrap();
            let selection = probe.select_range(&start, &end).unwrap();
            assert_eq!(selection.text, "hyphenation");
            let rects = probe.selection_rects(page, &selection);
            assert!(!rects.is_empty());
            for rect in rects {
                let hit = probe
                    .word_at(
                        page,
                        (rect.x + rect.width / 2) as f32,
                        (rect.y + rect.height / 2) as f32,
                    )
                    .unwrap();
                assert_eq!(hit, selection);
            }
        }
    }

    #[test]
    fn publisher_soft_hyphens_respect_inherited_and_inline_policy() {
        use skrifa::{FontRef, MetadataProvider as _};

        let source = Source::parse(
            r#"<html><head><style>.disabled { hyphens: none; }</style></head><body>
                <p>Lead interna&#173;tional travel.</p>
                <section class="disabled">
                <p>Lead interna&#173;tional travel.</p>
                <p>Lead <span style="hyphens: manual">interna&#173;tional</span> travel.</p>
                </section></body></html>"#,
        )
        .unwrap();
        let mut book = Book::open(&crate::test_fixture("field-notes-epub3.epub")).unwrap();
        let href = book.chapters[0].href.clone();
        let fonts = [crate::test_fixture("fonts/Silkscreen-Regular.ttf")];
        let mut fonts = crate::font_context(&fonts).unwrap();
        let styles =
            BookStyle::load(&mut book, &href, &source, &mut fonts).compute(&source.elements);
        let mut context = LayoutContext::new();
        let settings = Settings {
            font_size: 24.0,
            ..Settings::default()
        };
        let mut first_line_hyphen = |index: usize, width, disable_soft_hyphens| {
            let block = &source.blocks[index];
            let settings = Settings {
                disable_soft_hyphens,
                ..settings
            };
            let (layout, _) =
                layout_paragraph(&mut context, &mut fonts, block, width, settings, &styles);
            let line = layout.get(0).unwrap();
            let end = line.text_range().end;
            let painted = line.items().any(|item| {
                let PositionedLayoutItem::GlyphRun(run) = item else {
                    return false;
                };
                let font = run.run().font();
                let face = FontRef::from_index(font.data.data(), font.index).unwrap();
                let hyphen = face
                    .charmap()
                    .map('\u{2010}')
                    .or_else(|| face.charmap().map('-'))
                    .unwrap();
                run.positioned_glyphs()
                    .any(|glyph| glyph.id == hyphen.to_u32())
            });
            (end, painted)
        };
        let expected = ("Lead interna\u{ad}".len(), true);
        assert!((60..275).any(|width| {
            first_line_hyphen(0, width, false) == expected
                && !first_line_hyphen(1, width, false).1
                && first_line_hyphen(2, width, false) == expected
                && !first_line_hyphen(0, width, true).1
        }));
        for block in source.blocks {
            assert_eq!(block.text, "Lead interna\u{ad}tional travel.");
        }
    }
    use crate::{Engine, Probe, Renderer};
    use std::fs::{self, File, OpenOptions};
    use std::io::{Read, Write};
    use std::path::PathBuf;

    struct TemporaryEpub(PathBuf);

    impl Drop for TemporaryEpub {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }

    #[test]
    fn publisher_language_changes_line_breaks_without_changing_source_offsets() {
        let source = Source::parse(
            r#"<html lang="en"><body>
            <p>Lead 文〜文.</p>
            <p xml:lang="ja">Lead 文〜文.</p>
            <p>Lead <span xml:lang="zh-Hant">文〜文</span>.</p>
            <p xml:lang="ja">Lead <span lang="">文〜文</span>.</p>
            <p dir="ltr">سلام 12</p><p dir="rtl">سلام 12</p>
        </body></html>"#,
        )
        .unwrap();
        let fixture = crate::test_fixture("field-notes-epub3.epub");
        let mut book = Book::open(&fixture).unwrap();
        let href = book.chapters[0].href.clone();
        let fonts = [crate::test_fixture("fonts/Silkscreen-Regular.ttf")];
        let mut fonts = crate::font_context(&fonts).unwrap();
        let styles =
            BookStyle::load(&mut book, &href, &source, &mut fonts).compute(&source.elements);
        let mut context = LayoutContext::new();
        let settings = Settings {
            font_size: 24.0,
            ..Settings::default()
        };
        let expected = "Lead 文".len();
        let witness = (20..160).find(|&width| {
            let first_line_end =
                |context: &mut LayoutContext<PaintBrush>, fonts: &mut FontContext, block: usize| {
                    let (layout, _) = layout_paragraph(
                        context,
                        fonts,
                        &source.blocks[block],
                        width,
                        settings,
                        &styles,
                    );
                    let first = layout.get(0).unwrap().text_range();
                    assert!(source.blocks[block].text.is_char_boundary(first.end));
                    first.end
                };
            let english = first_line_end(&mut context, &mut fonts, 0);
            english < expected
                && first_line_end(&mut context, &mut fonts, 1) == expected
                && first_line_end(&mut context, &mut fonts, 2) == expected
                && first_line_end(&mut context, &mut fonts, 3) == english
        });
        assert!(
            witness.is_some(),
            "An inherited or inline CJK locale must permit a break before the wave dash"
        );
        for block in source.blocks.iter().take(4) {
            assert_eq!(block.text, "Lead 文〜文.");
        }
        let layout =
            |context: &mut LayoutContext<PaintBrush>, fonts: &mut FontContext, block: usize| {
                layout_paragraph(
                    context,
                    fonts,
                    &source.blocks[block],
                    300,
                    settings,
                    &styles,
                )
                .0
            };
        assert!(!layout(&mut context, &mut fonts, 4).is_rtl());
        assert!(layout(&mut context, &mut fonts, 5).is_rtl());
    }

    #[test]
    fn publisher_break_rules_change_wrapping_without_moving_source_anchors() {
        let source = Source::parse(
            r#"<html lang="ja"><head><style>
                .strict { line-break: strict }
                .split { word-break: break-all }
            </style></head><body>
                <p>Lead 文〜文.</p>
                <section class="strict">
                    <p>Lead 文〜文.</p>
                    <p>Lead <span style="line-break: auto">文<span id="anchor">〜</span>文</span>.</p>
                </section>
                <p>Lead abcdefghi.</p>
                <p class="split">Lead abcdefghi.</p>
            </body></html>"#,
        )
        .unwrap();
        let mut book = Book::open(&crate::test_fixture("field-notes-epub3.epub")).unwrap();
        let href = book.chapters[0].href.clone();
        let fonts = [crate::test_fixture("fonts/Silkscreen-Regular.ttf")];
        let mut fonts = crate::font_context(&fonts).unwrap();
        let styles =
            BookStyle::load(&mut book, &href, &source, &mut fonts).compute(&source.elements);
        let mut context = LayoutContext::new();
        let settings = Settings {
            font_size: 24.0,
            ..Settings::default()
        };
        let mut first_line_end = |block: usize, width| {
            let block = &source.blocks[block];
            let (layout, _) =
                layout_paragraph(&mut context, &mut fonts, block, width, settings, &styles);
            let end = layout.get(0).unwrap().text_range().end;
            assert!(block.text.is_char_boundary(end));
            end
        };
        let (block, anchor) = source.fragments["anchor"];
        assert_eq!(block, source.blocks[2].key);
        assert_eq!(anchor, "Lead 文".len());
        assert_eq!(&source.blocks[2].text[anchor..], "〜文.");
        assert!(
            (20..160).any(|width| {
                first_line_end(0, width) == anchor
                    && first_line_end(1, width) < anchor
                    && first_line_end(2, width) == anchor
            }),
            "Inherited strict wrapping should suppress the Japanese punctuation break that inline auto restores"
        );
        assert!(
            (40..160).any(|width| {
                first_line_end(3, width) == "Lead ".len()
                    && first_line_end(4, width) > "Lead a".len()
                    && first_line_end(4, width) < "Lead abcdefghi.".len()
            }),
            "Publisher break-all should fill the line with part of the next word"
        );
    }

    #[test]
    fn rasterizes_ruby_above_the_base_and_reopens_legacy_selections() {
        let path = std::env::temp_dir().join(format!("reader-ruby-{}.epub", std::process::id()));
        let file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)
            .unwrap();
        let publication = TemporaryEpub(path);
        let original = File::open(crate::test_fixture("field-notes-epub3.epub")).unwrap();
        let mut archive = zip::ZipArchive::new(original).unwrap();
        let mut changed = zip::ZipWriter::new(file);
        let chapter = r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Ruby</title></head><body>
            <p id="sentence">Before <ruby>DAYLIGHT<rp>(</rp><rt>day</rt><rp>)</rp></ruby> <span id="after">after</span>.</p>
            </body></html>"#;
        for index in 0..archive.len() {
            let entry = archive.by_index(index).unwrap();
            if entry.name() == "EPUB/chapters/shore.xhtml" {
                changed
                    .start_file(entry.name(), zip::write::SimpleFileOptions::default())
                    .unwrap();
                changed.write_all(chapter.as_bytes()).unwrap();
            } else {
                changed.raw_copy_file(entry).unwrap();
            }
        }
        changed.finish().unwrap();

        let fonts = [crate::test_fixture("fonts/Silkscreen-Regular.ttf")];
        let settings = Settings {
            width: 440,
            height: 416,
            margin: 36,
            font_size: 24.0,
            ..Settings::default()
        };
        let mut probe = Probe::open(&publication.0, Engine::Parley, &fonts, settings, 0).unwrap();
        let modern_after = probe.fragment("after").unwrap();
        let mut old_after = modern_after.clone();
        old_after.revision = crate::revision(chapter);
        old_after.offset = "Before DAYLIGHT(day) ".len();
        let mut old_end = old_after.clone();
        old_end.offset += "after".len();
        let former = crate::selection::TextSelection {
            start: old_after,
            end: old_end,
            text: "after".into(),
        };
        let saved = serde_json::to_vec(&former).unwrap();
        assert_eq!(probe.source.blocks[0].text, "Before DAYLIGHT after.");
        assert!(probe.matches("(day)").is_empty());
        let Renderer::Parley(layout) = &probe.renderer else {
            unreachable!()
        };
        let paragraph = &layout.paragraphs[0];
        let overlay = &paragraph.ruby[0];
        let base = paragraph.layout.get(overlay.line).unwrap();
        assert!(overlay.bottom < base.metrics().content_block_min_coord);
        let (left, right) = (
            settings.margin as usize + overlay.x.floor() as usize,
            settings.margin as usize + (overlay.x + overlay.layout.width()).ceil() as usize,
        );
        let page = probe.pages.page(paragraph.y + overlay.top);
        let range = probe.pages.range(page).unwrap();
        let (top, bottom) = (
            settings.margin as usize + (paragraph.y + overlay.top - range.start).floor() as usize,
            settings.margin as usize + (paragraph.y + overlay.bottom - range.start).ceil() as usize,
        );
        let pixels = probe.capture(page).unwrap();
        assert!((top..bottom).any(|row| {
            pixels[row * settings.width as usize + left..row * settings.width as usize + right]
                .iter()
                .any(|&pixel| pixel < 180)
        }));

        let settings = Settings {
            font_size: 30.0,
            ..settings
        };
        let reopened = Probe::open(&publication.0, Engine::Parley, &fonts, settings, 0).unwrap();
        let saved: crate::selection::TextSelection = serde_json::from_slice(&saved).unwrap();
        let projected = reopened.select_range(&saved.start, &saved.end).unwrap();
        assert_eq!(projected.text, "after");
        assert_eq!(projected.start, modern_after);
        let page = reopened.page_for(&saved.start).unwrap();
        assert_eq!(Some(page), reopened.page_for(&modern_after));
        assert!(!reopened.selection_rects(page, &saved).is_empty());
    }

    fn replace_opening(name: &str, chapter: &str) -> TemporaryEpub {
        let original = File::open(crate::test_fixture("field-notes-epub3.epub")).unwrap();
        let mut archive = zip::ZipArchive::new(original).unwrap();
        let target = TemporaryEpub(
            std::env::temp_dir().join(format!("reader-{name}-{}.epub", std::process::id())),
        );
        let mut changed = zip::ZipWriter::new(File::create(&target.0).unwrap());
        for index in 0..archive.len() {
            let entry = archive.by_index(index).unwrap();
            if entry.name() == "EPUB/chapters/shore.xhtml" {
                let options = zip::write::SimpleFileOptions::default()
                    .compression_method(entry.compression());
                changed.start_file(entry.name(), options).unwrap();
                changed.write_all(chapter.as_bytes()).unwrap();
            } else {
                changed.raw_copy_file(entry).unwrap();
            }
        }
        changed.finish().unwrap();
        target
    }

    #[test]
    fn publisher_image_maxima_keep_landscapes_inline_positions_and_originals() {
        let book = replace_opening(
            "publisher-image-maxima",
            r##"<html xmlns="http://www.w3.org/1999/xhtml">
          <head><style>
            img.plate { width: 640px; height: auto; max-width: 50%; }
            img.detail { max-width: 25%; }
            img.mark { max-height: 0.5em; }
            svg { max-height: 45px; }
          </style></head><body><h1>Harbour field guide</h1>
          <p>Plate and detail</p>
          <p><img class="plate" src="../images/shore.png" alt="Shore plate"/></p>
          <p><img class="plate detail" src="../images/shore.png" alt="Shore detail"/></p>
          <p><span id="before">An inline</span> <img class="mark" src="../images/shore.png" alt="mark"/> <span id="after">mark stays within the sentence.</span></p>
          <p><svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 320 160"><rect x="3" y="3" width="314" height="154" fill="#eeeeee" stroke="#000000" stroke-width="6"/><path d="M0 158L110 30L180 125L250 70L320 148" fill="none" stroke="#121212" stroke-width="9"/></svg></p>
          <p id="passage">The trail continues below the figures.</p>
          </body></html>"##,
        );
        let original = fs::read(&book.0).unwrap();
        let fonts = [
            crate::test_fixture("fonts/NotoSans-Regular.ttf"),
            crate::test_fixture("fonts/NotoSans-Bold.ttf"),
        ];
        let settings = Settings {
            width: 640,
            height: 880,
            margin: 40,
            font_size: 40.0,
            ..Settings::default()
        };
        let open = |settings| Probe::open(&book.0, Engine::Parley, &fonts, settings, 0).unwrap();
        let dimensions = |probe: &Probe| {
            let Renderer::Parley(layout) = &probe.renderer else {
                unreachable!()
            };
            layout
                .illustrations
                .iter()
                .map(|image| image.display)
                .collect::<Vec<_>>()
        };
        let mut narrow = open(settings);
        assert_eq!(
            dimensions(&narrow),
            [(280, 187), (140, 93), (30, 20), (90, 45)]
        );
        let landscape = open(Settings {
            width: 880,
            height: 640,
            ..settings
        });
        assert_eq!(
            dimensions(&landscape),
            [(400, 267), (200, 133), (30, 20), (90, 45)]
        );
        let without_css = open(Settings {
            publisher_styles: false,
            ..settings
        });
        assert_eq!(&dimensions(&without_css)[..3], [(144, 96); 3]);

        let passage = narrow.fragment("passage").unwrap();
        assert_eq!(landscape.fragment("passage"), Some(passage.clone()));
        assert_eq!(without_css.fragment("passage"), Some(passage.clone()));
        assert!(landscape.page_for(&passage).is_some());
        let before = narrow.fragment("before").unwrap();
        let after = narrow.fragment("after").unwrap();
        let page = narrow.page_for(&after).unwrap();
        assert_eq!(narrow.page_for(&before), Some(page));
        let Renderer::Parley(layout) = &narrow.renderer else {
            unreachable!()
        };
        let (resource, cols, rows) = layout
            .visible_illustrations(narrow.pages.range(page).unwrap(), settings)
            .find(|(_, cols, rows)| cols.len() == 30 && rows.len() == 20)
            .unwrap();
        let first = |anchor: &crate::Anchor| {
            let mut end = anchor.clone();
            end.offset += 1;
            narrow.selection_rects(page, &narrow.select_range(anchor, &end).unwrap())[0]
        };
        assert!((first(&before).x as usize) < cols.start);
        assert!(first(&after).x as usize >= cols.end);
        assert_eq!(
            narrow.illustration_at(page, cols.start as f32 + 1.0, rows.start as f32 + 1.0),
            Some(resource)
        );
        assert_eq!(
            narrow
                .open_illustration(&book.0, resource)
                .unwrap()
                .page_size(),
            (144.0, 96.0)
        );
        assert!(
            narrow
                .capture(page)
                .unwrap()
                .iter()
                .any(|&pixel| pixel < 100)
        );
        assert_eq!(fs::read(&book.0).unwrap(), original);
        if let Some(folder) = std::env::var_os("READER_PUBLISHER_IMAGE_SCALE_CAPTURES") {
            let folder = PathBuf::from(folder);
            for (name, width, height) in [
                ("publisher-image-maxima.png", 1264, 1680),
                ("publisher-image-maxima-landscape.png", 1680, 1264),
            ] {
                let mut probe = open(Settings {
                    width,
                    height,
                    ..settings
                });
                let frame = probe.capture(0).unwrap();
                image::save_buffer(
                    folder.join(name),
                    &frame,
                    width,
                    height,
                    image::ColorType::L8,
                )
                .unwrap();
            }
        }
    }

    #[test]
    fn dpi_decodes_actual_archive_artwork_with_intrinsic_html_css_and_relative_sizes() {
        let markup = r#"<html><body style="font-size:150%">
          <p><img src="../images/shore.png"/></p>
          <p><img src="../images/shore.png" width="72" height="48"/></p>
          <p><img src="../images/shore.png" style="width:96px;height:auto"/></p>
          <p><img src="../images/shore.png" style="width:1in;height:auto"/></p>
          <p><img src="../images/shore.png" style="width:25%;height:auto"/></p>
          <p><img src="../images/shore.png" style="width:2em;height:auto"/></p>
          <p><img src="../images/shore.png" style="width:2rem;height:auto"/></p>
          <p id="location">The original text anchor follows the illustrations.</p>
          </body></html>"#;
        let book = replace_opening("dpi-images", markup);
        let original = std::fs::read(&book.0).unwrap();
        let fonts = [crate::test_fixture("fonts/NotoSans-Regular.ttf")];
        for dpi in crate::RENDER_DPI_PRESETS {
            let settings = Settings {
                render_dpi: dpi,
                ..Settings::default()
            };
            let mut probe = Probe::open(&book.0, Engine::Parley, &fonts, settings, 0).unwrap();
            let Renderer::Parley(layout) = &probe.renderer else {
                unreachable!()
            };
            let images = &layout.illustrations;
            assert_eq!(images.len(), 7);
            let scale = if dpi == 0 { 1. } else { f32::from(dpi) / 96. };
            let widths = [
                144. * scale,
                72. * scale,
                96. * scale,
                if dpi == 0 { 144. } else { 96. * scale },
                settings.text_width() as f32 / 4.,
                126.,
                84.,
            ];
            for (image, width) in images.iter().zip(widths) {
                assert_eq!(image.display.0, width.round() as u32, "DPI {dpi}");
                assert_eq!(
                    image.display.1,
                    (width * 2. / 3.).round() as u32,
                    "DPI {dpi}"
                );
                assert!(image.pixels.as_raw().iter().any(|v| *v < 128));
            }
            let anchor = probe.fragment("location").unwrap();
            let page = probe.page_for(&anchor).unwrap();
            assert!(probe.capture(page).unwrap().iter().any(|v| *v < 128));
        }
        assert_eq!(std::fs::read(&book.0).unwrap(), original);
    }

    #[test]
    fn composited_inline_svg_draws_an_archive_picture_in_its_transformed_viewport_and_opens_original()
     {
        let xhtml = r##"<html xmlns="http://www.w3.org/1999/xhtml" xmlns:xlink="http://www.w3.org/1999/xlink"><body>
            <p id="cover"><svg xmlns="http://www.w3.org/2000/svg" version="1.1" width="100%" height="100%" viewBox="0 0 144 96">
            <rect width="100%" height="96" fill="#111111"/><g transform="translate(36 24) scale(.5)">
            <image xlink:href="../images/shore.png" width="144" height="96"/></g>
            </svg></p></body></html>"##;
        let book = replace_opening("composited-artwork", xhtml);
        let fonts = [crate::test_fixture("fonts/Silkscreen-Regular.ttf")];
        let settings = Settings::default();
        let probe = Probe::open(&book.0, Engine::Parley, &fonts, settings, 0).unwrap();
        let Renderer::Parley(chapter) = &probe.renderer else {
            unreachable!()
        };
        let art = &chapter.illustrations[0];
        assert_eq!(art.pixels.dimensions(), (144, 96));
        assert_eq!(art.pixels.get_pixel(2, 2).0, [17]);
        assert!(art.pixels.get_pixel(42, 30).0[0] > 210);
        assert_eq!(art.pixels.get_pixel(134, 80).0, [17]);
        let bounded = art.path.as_deref().unwrap();
        assert!(bounded.starts_with("reader-inline-svg:"));
        let original = probe.open_illustration(&book.0, bounded).unwrap();
        assert_eq!(original.page_size(), (144.0, 96.0));
        assert_eq!(probe.page_for(&probe.fragment("cover").unwrap()), Some(0));
        let (x, y) = art.origin(0.0, &settings.text_columns());
        assert_eq!(
            probe.illustration_at(
                0,
                x as f32 + 1.0,
                y as f32 + settings.top_margin() as f32 + 1.0
            ),
            Some(bounded)
        );
        let mut source = Book::open(&book.0).unwrap();
        let blocked = xhtml.replace("../images/shore.png", "https://example.invalid/never.png");
        let src = Source::parse(&blocked).unwrap();
        assert!(
            svg_image::render_inline(
                &mut source,
                &probe.chapter,
                src.blocks[0].images[0].inline_svg.as_ref().unwrap(),
                144,
                96,
                MAX_IMAGE_PIXELS,
                None
            )
            .is_err()
        );
    }

    #[test]
    fn images_in_one_paragraph_have_independent_resume_anchors() {
        let image = r#"<img src="../images/shore.png" width="700" height="1100"/>"#;
        let book = replace_opening(
            "image-anchors",
            &format!("<html><body><p>{}</p></body></html>", image.repeat(3)),
        );
        let fonts = [crate::test_fixture("fonts/NotoSans-Regular.ttf")];
        let mut probe =
            Probe::open(&book.0, Engine::Parley, &fonts, Settings::default(), 0).unwrap();
        assert_eq!(probe.page_count(), 3);
        for page in 0..3 {
            let anchor = probe.page_anchor(page).unwrap();
            let bytes = serde_json::to_vec(&anchor).unwrap();
            let restored = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(probe.page_for(&restored), Some(page));
            assert!(probe.capture(page).unwrap().iter().any(|&v| v < 128));
            if page == 0 {
                assert!(!String::from_utf8(bytes).unwrap().contains("illustration"));
            } else {
                assert_eq!(anchor.illustration, Some(page));
            }
        }
    }

    #[test]
    fn a_small_cache_share_keeps_image_geometry_and_visible_pixels() {
        let path = crate::test_fixture("field-notes-epub3.epub");
        let mut book = Book::open(&path).unwrap();
        let href = book.chapters[0].href.clone();
        let source=Source::parse(r#"<html><body><p><img src="../images/shore.png" width="144" height="96"/></p></body></html>"#).unwrap();
        let image = &source.blocks[0].images[0];
        let sizing = ImageSizing {
            width: 1024,
            height: 1200,
            requested_width: Some(144.0),
            requested_height: Some(96.0),
            max_width: None,
            max_height: None,
            render_dpi: 96,
        };
        let (path, preview, display) =
            load_image(&mut book, &href, image, Settings::default(), sizing, 100).unwrap();
        assert!(preview.as_raw().len() <= 100);
        assert_eq!(display, (144, 96));
        let art = Illustration {
            block: source.blocks[0].key,
            source_index: 0,
            painted: true,
            full_line: false,
            path: Some(path),
            link: None,
            label: String::new(),
            x: Some(0),
            y: 0.0,
            render_line: None,
            pixels: Arc::new(preview),
            display,
        };
        let mut page = vec![255; 150 * 100];
        art.draw(&mut page, &(0.0..100.0), 150, &(0..150));
        assert!(page.iter().any(|&v| v < 180));
        for (x, y) in [(0, 0), (52, 72), (143, 95)] {
            assert_eq!(
                page[y * 150 + x],
                art.pixels
                    .get_pixel(
                        x as u32 * art.pixels.width() / 144,
                        y as u32 * art.pixels.height() / 96
                    )
                    .0[0]
            );
        }
        assert!(page[96 * 150..].iter().all(|&v| v == 255));
    }

    #[test]
    fn list_column_width_is_part_of_the_shared_image_cache_key() {
        let artwork = r#"<img src="../images/shore.png" style="width:100%;height:auto"/>"#;
        let markup = format!(
            "<html><body><p>{artwork}</p><ol><li>{artwork}</li><li>{artwork}</li></ol></body></html>"
        );
        let book = replace_opening("list-image-cache", &markup);
        let fonts = [crate::test_fixture("fonts/NotoSans-Regular.ttf")];
        let probe = Probe::open(&book.0, Engine::Parley, &fonts, Settings::default(), 0).unwrap();
        let Renderer::Parley(chapter) = &probe.renderer else {
            unreachable!()
        };
        let [outer, first, second] = chapter.illustrations.as_slice() else {
            panic!()
        };
        assert!(outer.display.0 > first.display.0);
        assert_eq!(first.display, second.display);
        assert!(Arc::ptr_eq(&first.pixels, &second.pixels));
        assert!(!Arc::ptr_eq(&outer.pixels, &first.pixels));
        assert!(first.x.unwrap() > 0);
        assert_eq!(first.x, second.x);
        assert!(
            chapter
                .illustrations
                .iter()
                .all(|image| image.path.is_some())
        );
    }

    #[test]
    fn repeated_image_heavy_spine_shares_pixels_and_keeps_text_after_the_graphics_limit() {
        let icon = r#"<img src="../images/shore.png" width="3" height="2"/>"#;
        let markup = format!(
            "<html><body><p>{}</p><p id='end'>Final field note.</p></body></html>",
            icon.repeat(MAX_IMAGE_BOXES + 3)
        );
        let book = replace_opening("image-heavy", &markup);
        let fonts = [crate::test_fixture("fonts/Silkscreen-Regular.ttf")];
        let mut probe =
            Probe::open(&book.0, Engine::Parley, &fonts, Settings::default(), 0).unwrap();
        let Renderer::Parley(chapter) = &probe.renderer else {
            unreachable!()
        };
        assert_eq!(chapter.illustrations.len(), MAX_IMAGE_BOXES);
        assert!(
            chapter
                .illustrations
                .iter()
                .all(|art| Arc::ptr_eq(&chapter.illustrations[0].pixels, &art.pixels))
        );
        assert!(chapter.illustrations.last().unwrap().path.is_some());
        let end = probe.fragment("end").unwrap();
        let last = probe.page_for(&end).unwrap();
        assert_eq!(last, probe.page_count() - 1);
        assert_eq!(probe.matches("Final field note.").as_slice(), [end]);
        assert!(probe.capture(last).unwrap().iter().any(|&p| p < 200));
    }

    #[test]
    fn inline_image_preserves_text_geometry_css_sizing_and_link_destinations_after_reflow() {
        let path = crate::test_fixture("signal-guide.epub");
        let fonts = [crate::test_fixture("fonts/Silkscreen-Regular.ttf")];
        let read = |settings| Probe::open(&path, Engine::Parley, &fonts, settings, 0).unwrap();
        let settings = Settings {
            width: 1000,
            height: 1280,
            font_size: 30.0,
            base_stylesheet: crate::BaseStylesheet::None,
            ..Settings::default()
        };
        let mut probe = read(settings);
        let marker = probe.matches("marker")[0].clone();
        let position = probe.matches("comes")[0].clone();
        let page = probe.page_for(&position).unwrap();
        assert_eq!(probe.coverage().0, probe.coverage().1);
        let Renderer::Parley(layout) = &probe.renderer else {
            unreachable!()
        };
        let (path_on_page, cols, rows) = layout
            .visible_illustrations(probe.pages.range(page).unwrap(), settings)
            .find(|(path, _, _)| path.ends_with("beacon.svg"))
            .unwrap();
        assert_eq!(path_on_page, "EPUB/images/beacon.svg");
        assert_eq!(cols.len(), 35);
        assert_eq!(rows.len(), 35);
        let x = (cols.start + cols.len() / 2) as f32;
        let y = (rows.start + rows.len() / 2) as f32;
        let target = probe.link_at(page, x, y).unwrap();
        assert_eq!(target.path, "EPUB/chapters/legend.xhtml");
        assert_eq!(target.fragment.as_deref(), Some("beacon"));
        assert!(
            probe
                .links_on_page(page)
                .iter()
                .any(|(label, dest)| label == "Beacon key" && dest == &target)
        );
        assert_eq!(probe.illustration_at(page, x, y), Some(path_on_page));
        assert_eq!(
            probe
                .open_illustration(&path, path_on_page)
                .unwrap()
                .page_size(),
            (96.0, 96.0)
        );
        let ink = probe.capture(page).unwrap();
        assert!(
            rows.clone()
                .flat_map(|y| (cols.clone()).map(move |x| y * settings.width as usize + x))
                .any(|i| ink[i] < 55)
        );
        let first = |probe: &Probe, start: &crate::Anchor| {
            let mut end = start.clone();
            end.offset += 1;
            probe.selection_rects(page, &probe.select_range(start, &end).unwrap())[0]
        };
        let prior = first(&probe, &marker);
        let after = first(&probe, &position);
        // A line break is valid on either side of the illustration; within
        // a line its horizontal order must follow the original sentence.
        assert!(prior.y < rows.start as i32 || prior.x < cols.start as i32);
        assert!(
            after.y >= rows.end as i32 || after.x >= cols.end as i32,
            "before={prior:?} after={after:?} image={cols:?},{rows:?}"
        );
        assert_ne!(prior, after);
        let enlarged = read(Settings {
            width: 560,
            font_size: 48.0,
            ..settings
        });
        let page = enlarged.page_for(&position).unwrap();
        let Renderer::Parley(layout) = &enlarged.renderer else {
            unreachable!()
        };
        let (_, cols, _) = layout
            .visible_illustrations(enlarged.pages.range(page).unwrap(), enlarged.settings)
            .find(|(path, _, _)| path.ends_with("beacon.svg"))
            .unwrap();
        assert_eq!(cols.len(), 55);
        let sketch = layout
            .illustrations
            .iter()
            .find(|i| i.path.as_deref() == Some("EPUB/images/shore.png"))
            .unwrap();
        assert_eq!(
            sketch.pixels.width(),
            (enlarged.settings.text_width() as f32 * 0.68).round() as u32
        );
    }

    #[test]
    fn opens_a_linked_svg_with_the_existing_illustration_target() {
        let path =
            std::env::temp_dir().join(format!("reader-linked-svg-{}.epub", std::process::id()));
        let file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)
            .unwrap();
        let publication = TemporaryEpub(path);
        let original = File::open(crate::test_fixture("field-notes-epub3.epub")).unwrap();
        let mut archive = zip::ZipArchive::new(original).unwrap();
        let mut changed = zip::ZipWriter::new(file);
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index).unwrap();
            let name = entry.name().replace("shore.png", "shore.svg");
            let options =
                zip::write::SimpleFileOptions::default().compression_method(entry.compression());
            changed.start_file(&name, options).unwrap();
            match name.as_str() {
                "EPUB/images/shore.svg" => changed.write_all(br##"<svg xmlns="http://www.w3.org/2000/svg" width="80" height="40" viewBox="0 0 80 40">
                    <defs><linearGradient id="coast"><stop offset="0" stop-color="black"/><stop offset="1" stop-color="white"/></linearGradient></defs>
                    <rect width="80" height="40" fill="url(#coast)"/>
                </svg>"##).unwrap(),
                "EPUB/chapters/shore.xhtml" | "EPUB/package.opf" => {
                    let mut text = String::new();
                    entry.read_to_string(&mut text).unwrap();
                    let text = text.replace("shore.png", "shore.svg").replace("image/png", "image/svg+xml");
                    changed.write_all(text.as_bytes()).unwrap();
                }
                _ => { std::io::copy(&mut entry, &mut changed).unwrap(); }
            }
        }
        changed.finish().unwrap();

        let fonts = [crate::test_fixture("fonts/Silkscreen-Regular.ttf")];
        let probe = Probe::open(
            &publication.0,
            Engine::Parley,
            &fonts,
            Settings::default(),
            0,
        )
        .unwrap();
        let Renderer::Parley(layout) = &probe.renderer else {
            unreachable!()
        };
        let package = "EPUB/images/shore.svg";
        assert!(layout.has_illustration(package));
        let image = &layout.illustrations[0].pixels;
        // XHTML requests a width of 144px; CSS height:auto preserves the SVG aspect.
        assert_eq!(image.dimensions(), (144, 72));
        assert!(image.get_pixel(4, 4).0[0] < 32);
        assert!(image.get_pixel(140, 4).0[0] > 224);
        let original = probe.open_illustration(&publication.0, package).unwrap();
        assert_eq!(original.page_size(), (80.0, 40.0));
    }

    #[test]
    fn image_scaling_changes_raster_samples_without_changing_the_page_or_anchor() {
        let path = crate::test_fixture("field-notes-epub3.epub");
        let fonts = [crate::test_fixture("fonts/Silkscreen-Regular.ttf")];
        let settings = Settings {
            width: 180,
            height: 440,
            margin: 40,
            font_size: 20.0,
            ..Settings::default()
        };
        let make = |smooth_images| {
            Probe::open(
                &path,
                Engine::Parley,
                &fonts,
                Settings {
                    smooth_images,
                    ..settings
                },
                0,
            )
            .unwrap()
        };
        let smooth = make(true);
        let fast = make(false);
        let sample = |probe: &Probe| {
            let Renderer::Parley(layout) = &probe.renderer else {
                unreachable!()
            };
            layout.illustrations[0].pixels.clone()
        };
        let (smooth_image, fast_image) = (sample(&smooth), sample(&fast));
        assert_eq!(smooth_image.dimensions(), fast_image.dimensions());
        assert!(smooth_image.width() < 144);
        assert_ne!(smooth_image.as_raw(), fast_image.as_raw());
        let a = smooth.fragment("tide-b").unwrap();
        assert_eq!(smooth.page_for(&a), fast.page_for(&a));
        assert_eq!(smooth.page_count(), fast.page_count());
    }

    #[test]
    fn opens_full_local_illustration_at_its_drawn_page_and_scroll_coordinates() {
        let publication = crate::test_fixture("field-notes-epub3.epub");
        let fonts = [crate::test_fixture("fonts/Silkscreen-Regular.ttf")];
        let settings = Settings {
            width: 180,
            height: 440,
            margin: 30,
            font_size: 20.0,
            ..Settings::default()
        };
        let mut probe = Probe::open(&publication, Engine::Parley, &fonts, settings, 0).unwrap();
        let Renderer::Parley(layout) = &probe.renderer else {
            unreachable!()
        };
        let illustration = &layout.illustrations[0];
        let page = probe.pages.page(illustration.y);
        let range = probe.pages.range(page).unwrap();
        let (left, top) = illustration.origin(range.start, &settings.text_columns());
        let right = left + illustration.pixels.width() as usize;
        assert!(illustration.pixels.width() < 144);
        let (px, py, color) = illustration
            .pixels
            .enumerate_pixels()
            .find(|(_, _, color)| color[0] < 128)
            .unwrap();
        let (x, y) = (
            left + px as usize,
            (settings.margin as i32 + top + py as i32) as usize,
        );
        let color = color[0];
        let package = "EPUB/images/shore.png";
        assert_eq!(
            probe.illustration_at(page, x as f32, y as f32),
            Some(package)
        );
        assert!(
            probe
                .illustration_at(page, (left - 1) as f32, y as f32)
                .is_none()
        );
        assert!(
            probe
                .illustration_at(page, right as f32, y as f32)
                .is_none()
        );

        let scroll_start = illustration.y + 12.6;
        let scroll_range = scroll_start..scroll_start + settings.text_height() as f32;
        assert_eq!(
            probe.illustration_in(scroll_range.clone(), x as f32, settings.margin as f32),
            Some(package)
        );
        assert!(
            probe
                .illustration_in(scroll_range, x as f32, settings.margin as f32 - 0.5)
                .is_none()
        );
        let before = probe.fragment("tide-a").unwrap();
        let links = |probe: &Probe| {
            (0..probe.page_count())
                .flat_map(|page| probe.links_on_page(page))
                .collect::<Vec<_>>()
        };
        let before_links = links(&probe);
        assert!(
            before_links
                .iter()
                .any(|(_, destination)| destination.fragment.as_deref() == Some("tide-note"))
        );

        let original = probe.open_illustration(&publication, package).unwrap();
        assert_eq!(original.page_size(), (144.0, 96.0));
        assert!(
            probe
                .open_illustration(&publication, "EPUB/styles/reader.css")
                .is_err()
        );
        assert!(
            probe
                .open_illustration(&publication, "https://example.invalid/shore.png")
                .is_err()
        );
        assert_eq!(probe.fragment("tide-a"), Some(before));
        assert_eq!(links(&probe), before_links);
        assert_eq!(
            probe.capture(page).unwrap()[y * settings.width as usize + x],
            color
        );
    }

    #[test]
    fn a_missing_illustration_keeps_the_reading_placeholder_without_an_open_target() {
        let publication = crate::test_fixture("missing-image.epub");
        let fonts = [crate::test_fixture("fonts/Silkscreen-Regular.ttf")];
        let settings = Settings::default();
        let probe = Probe::open(&publication, Engine::Parley, &fonts, settings, 0).unwrap();
        let Renderer::Parley(layout) = &probe.renderer else {
            unreachable!()
        };
        let illustration = &layout.illustrations[0];
        let page = probe.pages.page(illustration.y);
        let range = probe.pages.range(page).unwrap();
        let (left, top) = illustration.origin(range.start, &settings.text_columns());
        assert_eq!(illustration.pixels.dimensions(), (64, 36));
        assert!(
            probe
                .illustration_at(
                    page,
                    (left + 10) as f32,
                    settings.margin as f32 + top as f32 + 10.0
                )
                .is_none()
        );
    }

    #[test]
    fn lays_out_scoped_font_sizes_without_changing_fragment_byte_positions() {
        let source = Source::parse(
            r#"<html><head><style>
                .chapter {font-size: 125%;}
                section.chapter .enlarged {font-size: 150%; font-weight: bold;}
            </style></head><body><section class="chapter">
                <p>Ordinary <span class="enlarged" id="anchor">Large</span> text.</p>
            </section></body></html>"#,
        )
        .unwrap();
        let mut book = Book::open(&crate::test_fixture("field-notes-epub3.epub")).unwrap();
        let path = book.chapters[0].href.clone();
        let fonts = [crate::test_fixture("fonts/Silkscreen-Regular.ttf")];
        let mut fonts = crate::font_context(&fonts).unwrap();
        let styles = crate::book_style::BookStyle::load(&mut book, &path, &source, &mut fonts)
            .compute(&source.elements);
        let (block, anchor) = source.fragments["anchor"];
        let text = &source.blocks[0].text;
        assert_eq!(block, source.blocks[0].key);
        assert_eq!(&text[anchor..anchor + 5], "Large");

        for size in [32.0, 40.0] {
            let settings = Settings {
                width: 700,
                height: 600,
                margin: 40,
                font_size: size,
                ..Settings::default()
            };
            let (mut chapter, source_lines, height) = ParleyChapter::new(
                &mut book,
                &path,
                &source,
                fonts.clone(),
                settings,
                &styles,
                &crate::user_hyphenation::Rules::default(),
            )
            .unwrap();
            let glyph_runs: Vec<_> = chapter.paragraphs[0]
                .layout
                .lines()
                .flat_map(|line| line.items())
                .filter_map(|item| match item {
                    PositionedLayoutItem::GlyphRun(run) => {
                        Some((run.run().text_range(), run.run().font_size()))
                    }
                    _ => None,
                })
                .collect();
            assert!(
                glyph_runs
                    .iter()
                    .any(|(range, value)| range.contains(&anchor)
                        && (*value - size * 1.875).abs() < 0.001)
            );
            assert!(
                glyph_runs.iter().any(
                    |(range, value)| range.contains(&0) && (*value - size * 1.25).abs() < 0.001
                )
            );
            assert!(
                source_lines
                    .iter()
                    .any(|line| line.block == block && line.bytes.contains(&anchor))
            );
            assert!(
                chapter
                    .capture(0.0..height, settings)
                    .unwrap()
                    .iter()
                    .any(|&gray| gray < 128)
            );
        }
    }
}

#[cfg(test)]
mod decoration_tests;

#[cfg(test)]
mod link_styles_tests;

#[cfg(test)]
mod inline_baseline_tests;

#[cfg(test)]
mod publisher_face_tests;

#[cfg(test)]
#[path = "parley_layout/word_expansion_cases.rs"]
mod word_expansion_cases;

#[cfg(test)]
mod cjk_width_tests;
#[cfg(test)]
mod pseudo_first_letter_tests;
#[cfg(test)]
mod pseudo_first_line_tests;
#[cfg(test)]
mod pseudo_generated_tests;
#[cfg(test)]
mod style_presets_tests;

#[cfg(test)]
mod table_presentation_tests;

#[cfg(test)]
mod publisher_padding_tests;

#[cfg(test)]
mod table_border_tests;
#[cfg(test)]
mod table_nowrap_tests;
#[cfg(test)]
mod vertical_padding_tests;

#[cfg(test)]
mod flow_tests;

#[cfg(test)]
mod image_style_tests;
#[cfg(test)]
mod media_line_tests;

#[cfg(test)]
#[path = "parley_layout/word_spacing_cases.rs"]
mod word_spacing_cases;

#[cfg(test)]
#[path = "parley_layout/kerning_mode_tests.rs"]
mod kerning_mode_tests;
