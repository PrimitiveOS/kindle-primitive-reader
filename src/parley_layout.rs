use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::sync::Arc;

use crate::book_style::Style;
use crate::comic::ImagePage;
use crate::epub::Book;
use crate::gray_glyph::GlyphCache;
use crate::source::{Block, EmbeddedImage, Source};
use crate::svg_image;
use crate::{Line, Result, Settings, image_reader};
use image::{DynamicImage, GrayImage, Luma, imageops::FilterType};
use parley::{
    Affinity, Alignment, AlignmentOptions, BoundingBox, Cursor, FontContext, FontFamily,
    FontWeight, GenericFamily, Language, Layout, LayoutContext, LineBreak, LineHeight,
    OverflowWrap, PositionedLayoutItem, Selection, SoftHyphenAction, StyleProperty, WordBreak,
    setting::{FontFeature, Tag},
};

const MAX_TEXT_BYTES: usize = 512 * 1024;
const MAX_LINES: usize = 30_000;
const MAX_HEIGHT: f32 = 2_000_000.0;
const MAX_IMAGE_BOXES: usize = 2_048;
const MAX_DISTINCT_IMAGE_READS: usize = 512;
const MAX_IMAGE_PIXELS: usize = 8 * 1024 * 1024;
const MAX_RUBY_BASE_BYTES: usize = 192;
const DISABLE_KERNING: [FontFeature; 1] = [FontFeature::new(Tag::new(b"kern"), 0)];

struct Paragraph {
    key: usize,
    y: f32,
    bottom: f32,
    background: Option<u8>,
    layout: Layout<()>,
    ruby: Vec<RubyOverlay>,
}

struct RubyOverlay {
    line: usize,
    x: f32,
    y: f32,
    top: f32,
    bottom: f32,
    layout: Layout<()>,
}

struct RubyCandidate {
    bytes: Range<usize>,
    layout: Layout<()>,
    line_height: f32,
}

struct Illustration {
    path: Option<String>,
    link: Option<String>,
    label: String,
    // Inline boxes provide x once the paragraph's line breaks and alignment are known.
    x: Option<usize>,
    y: f32,
    pixels: Arc<GrayImage>,
    display: (u32, u32),
}

/// A bounded chapter layout with a limited publisher style cascade and direct
/// grayscale glyph drawing. Source positions are shared with the Blitz probe.
pub(crate) struct ParleyChapter {
    paragraphs: Vec<Paragraph>,
    illustrations: Vec<Illustration>,
    glyphs: GlyphCache,
}

impl ParleyChapter {
    pub fn new(
        book: &mut Book,
        href: &str,
        source: &Source,
        mut fonts: FontContext,
        settings: Settings,
        styles: &[Style],
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
            glyphs: GlyphCache::with_hinting(settings.font_hinting),
        };
        let mut context = LayoutContext::new();
        let mut lines = Vec::new();
        let mut y = 0.0_f32;
        let mut image_budget = MAX_IMAGE_PIXELS;
        let fallback = Arc::new(GrayImage::from_pixel(
            settings.text_width().min(64),
            36,
            Luma([225]),
        ));
        // Share the bounded grayscale cache across distinct renderings. Duplicate icons
        // reuse one preview, while large later illustrations still receive space.
        let image_variants = source
            .blocks
            .iter()
            .flat_map(|block| {
                block.images.iter().map(move |image| {
                    ImageSizing::new(
                        image,
                        settings,
                        &styles[image.element],
                        !block.text.is_empty(),
                    )
                    .key(&image.href)
                })
            })
            .take(MAX_IMAGE_BOXES)
            .collect::<HashSet<_>>()
            .len()
            .min(MAX_DISTINCT_IMAGE_READS);
        let mut image_cache = HashMap::new();
        let width = settings.text_width();
        let gap = settings.font_size * 0.32;
        for block in &source.blocks {
            let first_image = chapter.illustrations.len();
            let available = MAX_IMAGE_BOXES
                .saturating_sub(first_image)
                .min(block.images.len());
            let mut inline_boxes = Vec::with_capacity(available);
            for image in block.images.iter().take(available) {
                let inline = !block.text.is_empty();
                let sizing = ImageSizing::new(image, settings, &styles[image.element], inline);
                let key = sizing.key(&image.href);
                let fallback_preview = || (None, Arc::clone(&fallback), fallback.dimensions());
                let (path, pixels, display) = if let Some(cached) = image_cache.get(&key) {
                    Clone::clone(cached)
                } else if image_cache.len() < image_variants {
                    let share = image_budget / (image_variants - image_cache.len());
                    let result = match load_image(book, href, image, settings, sizing, share) {
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
                        kind: parley::InlineBoxKind::InFlow,
                        width: (display.0 as f32
                            + if image.space_after {
                                settings.font_size * 0.25
                            } else {
                                0.0
                            })
                        .min(width as f32),
                        height: display.1 as f32,
                        baseline: None,
                        vertical_align: styles[image.element].image_align,
                    });
                }
                chapter.illustrations.push(Illustration {
                    path,
                    link: image.link.clone(),
                    label: image.label.clone(),
                    x: None,
                    y: 0.0,
                    pixels,
                    display,
                });
            }
            if !block.text.is_empty() {
                let style = &styles[block.element];
                let (layout, ruby) = layout_with_images(
                    &mut context,
                    &mut fonts,
                    block,
                    width,
                    settings,
                    styles,
                    &inline_boxes,
                );
                if lines.len() + layout.len() > MAX_LINES {
                    return Err("The chapter exceeds the Parley line limit".into());
                }
                let origin = y - layout
                    .lines()
                    .next()
                    .map_or(0.0, |line| line.metrics().content_block_min_coord.min(0.0));
                let mut bottom = origin + layout.height();
                let mut overlays = ruby.iter().peekable();
                for (index, line) in layout.lines().enumerate() {
                    for item in line.items() {
                        if let PositionedLayoutItem::InlineBox(position) = item {
                            let image = &mut chapter.illustrations[position.id as usize];
                            let after_gap = if layout.is_rtl() {
                                position.width - image.display.0 as f32
                            } else {
                                0.0
                            };
                            image.x = Some((position.x + after_gap).round().max(0.0) as usize);
                            image.y = (origin + position.y).round();
                        }
                    }
                    let metrics = line.metrics();
                    let mut top = metrics.content_block_min_coord;
                    while let Some(overlay) = overlays.next_if(|overlay| overlay.line == index) {
                        top = top.min(overlay.top);
                    }
                    let top = origin + top;
                    let line_bottom = origin + metrics.content_block_max_coord;
                    bottom = bottom.max(line_bottom);
                    lines.push(Line {
                        block: block.key,
                        bytes: line.text_range(),
                        top,
                        bottom: line_bottom,
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
                    });
                }
                y = bottom + gap;
                chapter.paragraphs.push(Paragraph {
                    key: block.key,
                    y: origin,
                    bottom,
                    background: style.background,
                    layout,
                    ruby,
                });
            }

            if block.text.is_empty() {
                for image in &mut chapter.illustrations[first_image..] {
                    let top = y.ceil();
                    let bottom = top + image.display.1 as f32;
                    lines.push(Line {
                        block: block.key,
                        bytes: 0..0,
                        top,
                        bottom,
                        missing_glyphs: 0,
                    });
                    image.y = top;
                    y = bottom + gap;
                }
            }

            if y > MAX_HEIGHT {
                return Err("The chapter exceeds the Parley height limit".into());
            }
        }

        let extent = (y - gap).max(0.0);
        Ok((chapter, lines, extent))
    }

    pub(crate) fn source_at(&self, x: f32, y: f32, visible: &Range<f32>) -> Option<(usize, usize)> {
        let paragraph = self.paragraphs.get(
            self.paragraphs
                .partition_point(|paragraph| paragraph.bottom <= y),
        )?;
        let (cluster, _) =
            parley::Cluster::from_point_exact(&paragraph.layout, x, y - paragraph.y)?;
        let line = cluster.line();
        let metrics = line.metrics();
        if paragraph.y + metrics.content_block_max_coord <= visible.start
            || paragraph.y + metrics.content_block_min_coord >= visible.end
        {
            return None;
        }
        Some((paragraph.key, cluster.text_range().start))
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

    fn visible_artwork(
        &self,
        range: Range<f32>,
        settings: Settings,
    ) -> impl Iterator<Item = (&Illustration, Range<usize>, Range<usize>)> {
        let height = visible_rows(&range, settings) as i32;
        self.illustrations.iter().filter_map(move |illustration| {
            let (left, top) = illustration.origin(
                range.start,
                settings.width as usize,
                settings.side_margin() as usize,
            );
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

    pub(crate) fn selection_layouts(
        &self,
        page: Range<f32>,
    ) -> impl Iterator<Item = (usize, f32, &Layout<()>)> {
        let start = self
            .paragraphs
            .partition_point(|paragraph| paragraph.bottom <= page.start);
        self.paragraphs[start..]
            .iter()
            .take_while(move |paragraph| paragraph.y < page.end)
            .map(|paragraph| (paragraph.key, paragraph.y, &paragraph.layout))
    }

    pub fn capture(&mut self, range: Range<f32>, settings: Settings) -> Result<Vec<u8>> {
        let visible_height = visible_rows(&range, settings);
        let mut frame = vec![255; (settings.width * settings.height) as usize];
        let stride = settings.width as usize;
        let margin = settings.side_margin() as usize;
        let top = settings.top_margin() as usize;
        let page = &mut frame[top * stride..(top + visible_height) * stride];

        let index = self
            .paragraphs
            .partition_point(|paragraph| paragraph.bottom <= range.start);
        for paragraph in &self.paragraphs[index..] {
            if paragraph.y >= range.end {
                break;
            }
            if let Some(gray) = paragraph.background {
                let top = (paragraph.y - range.start).floor().max(0.0) as usize;
                let bottom = ((paragraph.bottom - range.start).ceil() as usize).min(visible_height);
                for row in top.min(bottom)..bottom {
                    page[row * stride + margin..(row + 1) * stride - margin].fill(gray);
                }
            }
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
                        self.glyphs.draw_run(
                            page,
                            stride,
                            margin,
                            (0.0, paragraph.y - range.start),
                            &run,
                            settings.text_contrast,
                        );
                        if run.style().underline.is_some() {
                            let y =
                                (paragraph.y - range.start + run.baseline() + 3.0).round() as isize;
                            let x0 = margin
                                + (run.offset().floor().max(0.0) as usize).min(stride - 2 * margin);
                            let x1 = margin
                                + ((run.offset() + run.advance()).ceil().max(0.0) as usize)
                                    .min(stride - 2 * margin);
                            for row in y.max(0) as usize..(y + 2).max(0) as usize {
                                if row < visible_height && x0 < x1 {
                                    page[row * stride + x0..row * stride + x1].fill(48);
                                }
                            }
                        }
                    }
                }
            }
            for overlay in &paragraph.ruby {
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
                            margin,
                            (overlay.x, paragraph.y + overlay.y - range.start),
                            &run,
                            settings.text_contrast,
                        );
                    }
                }
            }
        }

        for illustration in &self.illustrations {
            illustration.draw(page, &range, stride, margin);
        }
        Ok(frame)
    }
}

fn visible_rows(range: &Range<f32>, settings: Settings) -> usize {
    (range.end - range.start)
        .ceil()
        .clamp(1.0, settings.text_height() as f32) as usize
}

#[cfg(test)]
fn layout_paragraph(
    context: &mut LayoutContext<()>,
    fonts: &mut FontContext,
    block: &Block,
    width: u32,
    settings: Settings,
    styles: &[Style],
) -> (Layout<()>, Vec<RubyOverlay>) {
    layout_with_images(context, fonts, block, width, settings, styles, &[])
}

fn layout_with_images(
    context: &mut LayoutContext<()>,
    fonts: &mut FontContext,
    block: &Block,
    width: u32,
    settings: Settings,
    styles: &[Style],
    images: &[parley::InlineBox],
) -> (Layout<()>, Vec<RubyOverlay>) {
    let mut layout = make_paragraph(
        context, fonts, block, width, settings, styles, &[], images, false,
    );
    // Some embedded fonts have ascenders and descenders taller than the publisher's
    // line advance. A long paragraph can then have no page break between full lines.
    let fit_font_metrics = overlaps_entire_page(&layout, settings.text_height() as f32);
    if fit_font_metrics {
        drop(layout);
        layout = make_paragraph(
            context, fonts, block, width, settings, styles, &[], images, true,
        );
    }
    let mut candidates = Vec::new();
    for ruby in &block.ruby {
        if ruby.bytes.len() > MAX_RUBY_BASE_BYTES {
            continue;
        }
        let Some((base, line)) = base_geometry(&layout, &ruby.bytes) else {
            continue;
        };
        let style = block
            .styled_text
            .iter()
            .find(|run| run.bytes.contains(&ruby.bytes.start))
            .map_or(&styles[block.element], |run| &styles[run.element]);
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
        if annotation.len() != 1 || annotation.width() > base.width() as f32 + 0.25 {
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
        width,
        settings,
        styles,
        &candidates,
        images,
        fit_font_metrics,
    );
    let mut overlays = Vec::new();
    for candidate in candidates {
        let Some((base, index)) = base_geometry(&layout, &candidate.bytes) else {
            continue;
        };
        let ink = *candidate.layout.get(0).unwrap().metrics();
        let width = candidate.layout.width();
        let line = layout.get(index).unwrap();
        let metrics = line.metrics();
        let height = ink.content_block_max_coord - ink.content_block_min_coord;
        let bottom = metrics.content_block_min_coord - 2.0;
        let top = bottom - height;
        if top < metrics.block_min_coord || width > base.width() as f32 + 0.25 {
            continue;
        }
        overlays.push(RubyOverlay {
            line: index,
            x: (base.x0 as f32 + (base.width() as f32 - width) * 0.5).max(0.0),
            y: top - ink.content_block_min_coord,
            top,
            bottom,
            layout: candidate.layout,
        });
    }
    overlays.sort_by_key(|overlay| overlay.line);
    (layout, overlays)
}

fn base_geometry(layout: &Layout<()>, bytes: &Range<usize>) -> Option<(BoundingBox, usize)> {
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
    let (bounds, row) = result?;
    let line = layout.get(row)?.text_range();
    (!multiple && line.start <= bytes.start && bytes.end <= line.end && bounds.width() > 0.0)
        .then_some((bounds, row))
}

fn make_annotation(
    context: &mut LayoutContext<()>,
    fonts: &mut FontContext,
    text: &str,
    language: Option<Language>,
    style: &Style,
    settings: Settings,
) -> Layout<()> {
    let mut builder = context.ranged_builder(fonts, text, 1.0, true);
    if !settings.font_kerning {
        builder.push_default(StyleProperty::FontFeatures((&DISABLE_KERNING).into()));
    }
    let family: FontFamily<'_> = if settings.override_fonts {
        GenericFamily::Serif.into()
    } else {
        style.font_family.layout()
    };
    builder.push_default(StyleProperty::FontFamily(family));
    builder.push_default(StyleProperty::Locale(language));
    builder.push_default(StyleProperty::FontSize(
        (settings.font_size * style.size_scale * 0.48).clamp(8.0, 84.0),
    ));
    let mut layout = builder.build(text);
    layout.break_all_lines(None);
    layout
}

fn overlaps_entire_page(layout: &Layout<()>, page_height: f32) -> bool {
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
    context: &mut LayoutContext<()>,
    fonts: &mut FontContext,
    block: &Block,
    width: u32,
    settings: Settings,
    styles: &[Style],
    ruby: &[RubyCandidate],
    images: &[parley::InlineBox],
    fit_font_metrics: bool,
) -> Layout<()> {
    let style = &styles[block.element];
    let language = block.language;
    let mut builder = context.ranged_builder(fonts, &block.text, 1.0, true);
    if !settings.font_kerning {
        builder.push_default(StyleProperty::FontFeatures((&DISABLE_KERNING).into()));
    }
    let soft_hyphen = |offset| {
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
        } else if style.soft_hyphens && !settings.disable_soft_hyphens {
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
    let default_spacing = Settings::default().line_height;
    let line_height = |style: &Style| {
        let publisher = style.line_height.unwrap_or(default_spacing);
        let spacing = (publisher * settings.line_height / default_spacing).clamp(1.0, 3.0);
        if fit_font_metrics {
            LineHeight::MetricsRelative((spacing / default_spacing).max(1.05))
        } else {
            LineHeight::FontSizeRelative(spacing)
        }
    };
    let font_size = |style: &Style| (settings.font_size * style.size_scale).clamp(12.0, 192.0);
    let weight = |style: &Style| {
        if settings.bold_text && style.font_weight < FontWeight::BOLD {
            FontWeight::BOLD
        } else {
            style.font_weight
        }
    };
    builder.push_default(StyleProperty::LineHeight(line_height(style)));
    builder.push_default(StyleProperty::OverflowWrap(OverflowWrap::Anywhere));
    builder.push_default(StyleProperty::FontSize(font_size(style)));
    if settings.word_spacing != 0.0 {
        builder.push_default(StyleProperty::WordSpacing(settings.word_spacing));
    }
    if settings.letter_spacing != 0.0 {
        builder.push_default(StyleProperty::LetterSpacing(settings.letter_spacing));
    }
    builder.push_default(StyleProperty::FontWeight(weight(style)));
    builder.push_default(StyleProperty::FontStyle(style.font_style));
    builder.push_default(StyleProperty::Locale(language));
    let paragraph_line_break = style.line_break.unwrap_or_else(|| line_break(language));
    builder.push_default(StyleProperty::LineBreak(paragraph_line_break));
    builder.push_default(StyleProperty::WordBreak(style.word_break));
    builder.set_base_direction(block.direction);
    for text in &block.styled_text {
        let run = &styles[text.element];
        if text.language != language {
            builder.push(StyleProperty::Locale(text.language), text.bytes.clone());
        }
        let run_line_break = run.line_break.unwrap_or_else(|| line_break(text.language));
        if run_line_break != paragraph_line_break {
            builder.push(StyleProperty::LineBreak(run_line_break), text.bytes.clone());
        }
        if run.word_break != style.word_break {
            builder.push(StyleProperty::WordBreak(run.word_break), text.bytes.clone());
        }
        if !settings.override_fonts && run.font_family != style.font_family {
            builder.push(
                StyleProperty::FontFamily(run.font_family.layout()),
                text.bytes.clone(),
            );
        }
        if run.size_scale != style.size_scale {
            builder.push(StyleProperty::FontSize(font_size(run)), text.bytes.clone());
        }
        if run.line_height != style.line_height {
            builder.push(
                StyleProperty::LineHeight(line_height(run)),
                text.bytes.clone(),
            );
        }
        if run.font_weight != style.font_weight {
            builder.push(StyleProperty::FontWeight(weight(run)), text.bytes.clone());
        }
        if run.font_style != style.font_style {
            builder.push(StyleProperty::FontStyle(run.font_style), text.bytes.clone());
        }
    }
    for link in &block.links {
        builder.push(StyleProperty::Underline(true), link.bytes.clone());
    }
    for annotation in ruby {
        builder.push(
            StyleProperty::LineHeight(LineHeight::Absolute(annotation.line_height)),
            annotation.bytes.clone(),
        );
    }
    for image in images {
        builder.push_inline_box(image.clone());
    }
    let mut layout = builder.build(&block.text);
    layout.break_all_lines(Some(width as f32));
    let alignment = match settings.alignment {
        crate::TextAlignment::Publisher => style.alignment.unwrap_or(Alignment::Start),
        crate::TextAlignment::Start => Alignment::Start,
        crate::TextAlignment::Center => Alignment::Center,
        crate::TextAlignment::Justify => Alignment::Justify,
    };
    layout.align(alignment, AlignmentOptions::default());
    layout
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
}

impl ImageSizing {
    fn key<'a>(&self, path: &'a str) -> (&'a str, Option<u64>, Option<u64>, u32) {
        (
            path,
            self.requested_width.map(f64::to_bits),
            self.requested_height.map(f64::to_bits),
            self.height,
        )
    }
    fn new(image: &EmbeddedImage, settings: Settings, style: &Style, inline: bool) -> Self {
        let width = settings.text_width();
        // Inline artwork shares its line with a font strut below the baseline.
        let height = if inline {
            settings
                .text_height()
                .saturating_sub((settings.font_size * 2.0).ceil() as u32)
                .max(1)
        } else {
            settings.text_height()
        };
        let font_size = (settings.font_size * style.size_scale).clamp(12.0, 192.0);
        let requested_width = style
            .image_width
            .map_or(image.width, |value| value.pixels(width, font_size))
            .filter(|v| v.is_finite())
            .map(f64::from);
        let requested_height = style
            .image_height
            .map_or(image.height, |value| value.pixels(height, font_size))
            .filter(|v| v.is_finite())
            .map(f64::from);
        Self {
            width,
            height,
            requested_width,
            requested_height,
        }
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
    let ImageSizing {
        width,
        height,
        requested_width,
        requested_height,
    } = sizing;
    let target = |original_w: u32, original_h: u32| {
        let ratio = f64::from(original_w) / f64::from(original_h);
        let w = requested_width
            .unwrap_or_else(|| requested_height.map_or(f64::from(original_w), |h| h * ratio));
        let h = requested_height.unwrap_or(w / ratio);
        let scale = (f64::from(width) / w).min(f64::from(height) / h).min(1.0);
        (
            (w * scale).round().clamp(1.0, f64::from(width)) as u32,
            (h * scale).round().clamp(1.0, f64::from(height)) as u32,
        )
    };
    let styled_size = (requested_width.is_some() || requested_height.is_some())
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
        let art =
            svg_image::rasterize_with_size(&bytes, width, height, MAX_IMAGE_PIXELS, styled_size)
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
    let mut image = image_reader(&bytes).ok()?.decode().ok()?;
    if stored != (w, h) {
        image = image.resize_exact(stored.0, stored.1, filter);
    }
    Some((path, grayscale_image(image), display))
}

pub(crate) fn original_illustration(bytes: &[u8]) -> Result<ImagePage> {
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
    fn origin(&self, start: f32, stride: usize, margin: usize) -> (usize, i32) {
        let remaining = stride - 2 * margin - self.display.0 as usize;
        let left = margin + self.x.map_or(remaining / 2, |x| x.min(remaining));
        (left, (self.y - start).round() as i32)
    }

    fn draw(&self, page: &mut [u8], range: &Range<f32>, stride: usize, margin: usize) {
        let (x, top) = self.origin(range.start, stride, margin);
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
        let styles = BookStyle::load(&mut book, &href, &source.stylesheets, &mut fonts)
            .compute(&source.elements);
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
        let styles = BookStyle::load(&mut book, &href, &source.stylesheets, &mut fonts)
            .compute(&source.elements);
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
        let styles = BookStyle::load(&mut book, &href, &source.stylesheets, &mut fonts)
            .compute(&source.elements);
        let mut context = LayoutContext::new();
        let settings = Settings {
            font_size: 24.0,
            ..Settings::default()
        };
        let expected = "Lead 文".len();
        let witness = (20..160).find(|&width| {
            let first_line_end =
                |context: &mut LayoutContext<()>, fonts: &mut FontContext, block: usize| {
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
        let layout = |context: &mut LayoutContext<()>, fonts: &mut FontContext, block: usize| {
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
        let styles = BookStyle::load(&mut book, &href, &source.stylesheets, &mut fonts)
            .compute(&source.elements);
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
        let (x, y) = art.origin(
            0.0,
            settings.width as usize,
            settings.side_margin() as usize,
        );
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
        };
        let (path, preview, display) =
            load_image(&mut book, &href, image, Settings::default(), sizing, 100).unwrap();
        assert!(preview.as_raw().len() <= 100);
        assert_eq!(display, (144, 96));
        let art = Illustration {
            path: Some(path),
            link: None,
            label: String::new(),
            x: Some(0),
            y: 0.0,
            pixels: Arc::new(preview),
            display,
        };
        let mut page = vec![255; 150 * 100];
        art.draw(&mut page, &(0.0..100.0), 150, 0);
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
        let (left, top) = illustration.origin(
            range.start,
            settings.width as usize,
            settings.margin as usize,
        );
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
        let (left, top) = illustration.origin(
            range.start,
            settings.width as usize,
            settings.margin as usize,
        );
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
        let styles =
            crate::book_style::BookStyle::load(&mut book, &path, &source.stylesheets, &mut fonts)
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
            let (mut chapter, source_lines, height) =
                ParleyChapter::new(&mut book, &path, &source, fonts.clone(), settings, &styles)
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
