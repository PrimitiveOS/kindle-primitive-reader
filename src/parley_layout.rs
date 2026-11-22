mod cjk_width;
mod generated_boxes;
#[cfg(test)]
mod special_glyph_tests;

use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::sync::Arc;

use crate::book_style::{Style, TextDecoration};
use crate::comic::ImagePage;
use crate::epub::Book;
use crate::flow_projection::OwnedProjection;
use crate::flow_view::{ChapterFlow, LayoutBlock};
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

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PaintBrush {
    pub(crate) decoration: TextDecoration,
    pub(crate) painted: bool,
}

impl Default for PaintBrush {
    fn default() -> Self {
        Self {
            decoration: Default::default(),
            painted: true,
        }
    }
}

struct Paragraph {
    column: Range<f32>,
    max_bottom: f32,
    key: usize,
    x: f32,
    y: f32,
    bottom: f32,
    background: Option<u8>,
    layout: Layout<PaintBrush>,
    ruby: Vec<RubyOverlay>,
    flow: Option<Arc<OwnedProjection>>,
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
}

struct Illustration {
    block: usize,
    source_index: usize,
    painted: bool,
    path: Option<String>,
    link: Option<String>,
    label: String,
    // Inline boxes provide x once the paragraph's line breaks and alignment are known.
    x: Option<usize>,
    y: f32,
    // Preserve exact layout-line ownership separately from rounded paint Y.
    line_top: Option<f32>,
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
    padded_backgrounds: Vec<crate::vertical_padding::Fill>,
    limitations: Vec<String>,
    boundaries: std::collections::BTreeMap<usize, generated_boxes::BoundaryGeometry>,
    glyphs: GlyphCache,
}

impl ParleyChapter {
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
            book, href, source, fonts, settings, styles, user_rules, &flow,
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
    ) -> Result<(Self, Vec<Line>, f32)> {
        Self::with_flow_mode(
            book, href, source, fonts, settings, styles, user_rules, flow, false,
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
            book, href, source, fonts, settings, styles, user_rules, flow, true,
        )
    }

    fn with_flow_mode(
        book: &mut Book,
        href: &str,
        source: &Source,
        mut fonts: FontContext,
        settings: Settings,
        styles: &[Style],
        user_rules: &crate::user_hyphenation::Rules,
        flow: &ChapterFlow,
        source_order: bool,
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
            padded_backgrounds: Vec::new(),
            limitations: Vec::new(),
            boundaries: Default::default(),
            glyphs: GlyphCache::with_hinting(
                settings.font_hinting,
                styles
                    .first()
                    .map_or_else(Default::default, |style| style.font_weights.clone()),
            ),
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
        let group_insets = list_insets(source, styles, settings.font_size, flow);
        let outer_width = settings.text_width();
        let table_flow = if source_order {
            chapter.limitations.push(
                "Table rows could not fit a reading page; the table uses readable source order"
                    .into(),
            );
            crate::table_layout::TableFlow::source_order(source)
        } else {
            match crate::table_layout::TableFlow::new(source, styles) {
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
        let mut borders = crate::table_borders::Borders::read_with_flow(
            source,
            styles,
            settings.font_size,
            &table_flow,
        );
        let mut flow_error = None;
        let publisher_boxes = crate::publisher_padding::layout_with_flow(
            source,
            styles,
            outer_width,
            settings.font_size,
            borders.as_ref(),
            &table_flow,
            |block| {
                if styles[block.element].flow.suppressed {
                    return (0, 0);
                }
                let input = match flow.layout_block(source, block) {
                    Ok(input) => input,
                    Err(error) => {
                        flow_error = Some(error);
                        return (0, 0);
                    }
                };
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
        let vertical_padding = crate::vertical_padding::layout_with_flow(
            source,
            styles,
            publisher_boxes.as_deref(),
            settings,
            &table_flow,
        );
        let generated = generated_boxes::GeneratedBoxes::new(
            source,
            styles,
            flow,
            publisher_boxes.as_deref(),
            settings,
        )?;
        chapter
            .limitations
            .extend(generated.limitations.iter().cloned());
        let column_for = |block: &Block| {
            if let Some(boxes) = &publisher_boxes {
                return boxes[block.element].cell;
            }

            table_flow.block_cell(block).filter(|cell| {
                outer_width as f32 / f32::from(cell.columns) >= 2.5 * settings.font_size
            })
        };
        let bounds_for = |block: &Block| {
            if let Some(boxes) = &publisher_boxes {
                let area = boxes[block.element];
                return (area.x, area.width);
            }

            column_for(block).map_or((0, outer_width), |cell| {
                cell.bounds(outer_width, (settings.font_size * 0.22).round() as u32)
            })
        };
        let gutter_for = |block: &Block| {
            block
                .list_item
                .and_then(|item| group_insets.get(&item.group))
                .copied()
                .unwrap_or(0.0)
                .round()
                .clamp(0.0, bounds_for(block).1 as f32 * 0.49) as u32
        };
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
            let image_width = bounds_for(&block).1 - gutter_for(&block);
            for image in block.images.iter() {
                if image_boxes == MAX_IMAGE_BOXES {
                    break;
                }
                variants.insert(
                    ImageSizing::new(
                        &image,
                        settings,
                        &styles[image.element],
                        !block.text.is_empty(),
                        image_width,
                    )
                    .key(&image.original().href),
                );
                image_boxes += 1;
            }
        }
        let image_variants = variants.len().min(MAX_DISTINCT_IMAGE_READS);
        let mut image_cache = HashMap::new();
        let gap = settings.font_size * 0.32;
        let mut row_flow = crate::table_layout::RowFlow::default();
        let mut positions = crate::table_borders::Positions::default();
        let mut fill_positions = (vertical_padding.is_some()
            && styles.iter().any(|s| {
                s.background.is_some()
                    && s.vertical_padding
                        .iter()
                        .any(|v| v.pixels(outer_width, settings.font_size) > 0.)
            }))
        .then(|| Vec::with_capacity(source.blocks.len()));
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
        for (index, source_block) in source.blocks.iter().enumerate() {
            if generated.has_slot(index) {
                y = row_flow.end(y);
                generated.emit(
                    index,
                    &mut y,
                    gap,
                    source,
                    styles,
                    settings,
                    &mut chapter.boundaries,
                    &mut chapter.padded_backgrounds,
                    &mut chapter.table_frames,
                )?;
            }
            if generated.excluded[index] {
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
            let input = flow.layout_block(source, source_block)?;
            let block = &input;
            let cell = column_for(block);
            let padding = vertical_padding
                .as_ref()
                .map_or_else(Default::default, |g| g[index]);
            if padding.before_grid > 0 {
                y = row_flow.end(y) + padding.before_grid as f32;
            }
            y = row_flow.enter(cell, y);
            let start_of_cell = y;
            let decorated = cell.and_then(|cell| Some((cell, borders.as_ref()?.cells.get(&cell)?)));
            if let Some((c, _)) = decorated
                && Some(c) != last_cell
            {
                y += borders.as_ref().unwrap().vertical(c).0 as f32;
            }
            last_cell = cell;
            y += padding.before as f32;
            let (column_x, column_width) = bounds_for(block);
            let gutter = gutter_for(block);
            let image_width = column_width - gutter;
            let marker = block
                .list_item
                .filter(|item| marker_hosts.get(&item.element) == Some(&block.key))
                .and_then(|item| {
                    let style = &styles[item.element];
                    let size = (settings.font_size * style.size_scale).clamp(12.0, 192.0);
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
            let mut rtl = matches!(block.direction, parley::BaseDirection::Rtl);
            let mut marker_baseline = None;
            let first_image = chapter.illustrations.len();
            let available = MAX_IMAGE_BOXES
                .saturating_sub(first_image)
                .min(block.images.len());
            let mut inline_boxes = Vec::with_capacity(available);
            for image in block.images.iter().take(available) {
                let inline = !block.text.is_empty();
                let sizing = ImageSizing::new(
                    &image,
                    settings,
                    &styles[image.element],
                    inline,
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
                    block: block.key,
                    source_index: image.source_index,
                    painted: image.painted,
                    path,
                    link: image.link.clone(),
                    label: image.label.clone(),
                    x: None,
                    y: 0.0,
                    line_top: None,
                    pixels,
                    display,
                });
            }
            if !block.text.is_empty() {
                let style = &styles[block.element];
                let (text_x, text_width, indent_base) =
                    publisher_boxes
                        .as_ref()
                        .map_or((column_x, width, image_width), |boxes| {
                            let area = boxes[block.element];
                            let (x, expanded) = area.paragraph_bounds_for_text(
                                block.direction,
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
                let indent = LineIndent::resolve(
                    style,
                    indent_base,
                    text_width,
                    settings.font_size,
                    hanging,
                );
                let (layout, ruby) = layout_with_images(
                    &mut context,
                    &mut fonts,
                    block,
                    text_width - indent.common,
                    settings,
                    styles,
                    &inline_boxes,
                    indent,
                    user_rules,
                );
                if lines.len() + layout.len() > MAX_LINES {
                    return Err("The chapter exceeds the Parley line limit".into());
                }
                let origin = y - layout
                    .lines()
                    .next()
                    .map_or(0.0, |line| line.metrics().content_block_min_coord.min(0.0));
                rtl = layout.is_rtl();
                let x = text_x as f32
                    + if rtl {
                        0.0
                    } else {
                        (gutter - hanging + indent.common) as f32
                    };
                marker_baseline = layout.get(0).map(|line| origin + line.metrics().baseline);
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
                            image.x = Some((x + position.x + after_gap).round().max(0.0) as usize);
                            image.y = (origin + position.y).round();
                            image.line_top = Some(origin + line.metrics().content_block_min_coord);
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
                    let mut source_line = Line {
                        block: block.key,
                        bytes: line.text_range(),
                        spans: None,
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
                    };
                    source_line.project(&projection, line.text_range())?;
                    lines.push(source_line);
                }
                y = bottom + gap;
                chapter.paragraphs.push(Paragraph {
                    key: block.key,
                    column: publisher_boxes.as_ref().map_or(
                        column_x as f32..(column_x + column_width) as f32,
                        |boxes| {
                            let area = boxes[block.element];
                            area.outer_x as f32..(area.outer_x + area.outer_width) as f32
                        },
                    ),
                    max_bottom: bottom,
                    x,
                    y: origin,
                    bottom,
                    background: style.flow.painted.then_some(style.background).flatten(),
                    layout,
                    ruby,
                    flow: flow.shared(block.key),
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
                        top,
                        bottom,
                        missing_glyphs: 0,
                    });
                    image.y = top;
                    image.line_top = Some(top);
                    if gutter != 0 || cell.is_some() || column_width != outer_width {
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
                let marker_top = marker.top.min(block_y);
                let marker_bottom = marker
                    .bottom
                    .max(block_y + size * settings.line_height.max(1.0));
                if let Some(item) = block.list_item {
                    if let Some((key, offset)) = source.boundary_position(item.element) {
                        let origin = source.elements[item.element].source_bytes.start;
                        chapter.boundaries.insert(
                            origin,
                            generated_boxes::BoundaryGeometry {
                                origin,
                                block: key,
                                offset,
                                top: marker_top,
                                bottom: marker_bottom,
                                generated_only: block.text.is_empty() && block.images.is_empty(),
                            },
                        );
                    }
                }
                chapter.markers.push(marker);
                if block.text.is_empty() && block.images.is_empty() {
                    y = y.max(block_y + size * settings.line_height.max(1.0)) + gap;
                }
            }

            if y > MAX_HEIGHT {
                return Err("The chapter exceeds the Parley height limit".into());
            }
            if let Some(points) = &mut fill_positions {
                points.push((block_y, (y - gap).max(block_y)));
            }
            y += padding.after as f32;
            if let Some((c, _)) = decorated {
                let following = source.blocks[index + 1..]
                    .iter()
                    .find(|next| !styles[next.element].flow.suppressed)
                    .and_then(&column_for);
                if following != Some(c) {
                    y += borders.as_ref().unwrap().vertical(c).1 as f32;
                }
                positions.record(c, start_of_cell, (y - gap).max(start_of_cell));
            }
            if padding.after_grid > 0 {
                y = row_flow.end(y) + padding.after_grid as f32;
            }
        }

        if generated.has_slot(source.blocks.len()) {
            y = row_flow.end(y);
            generated.emit(
                source.blocks.len(),
                &mut y,
                gap,
                source,
                styles,
                settings,
                &mut chapter.boundaries,
                &mut chapter.padded_backgrounds,
                &mut chapter.table_frames,
            )?;
        }

        if source.blocks.iter().any(|b| column_for(b).is_some()) {
            chapter.paragraphs.sort_by(|a, b| a.y.total_cmp(&b.y));
        }
        generated.inline_boundaries(source, flow, &chapter.paragraphs, &mut chapter.boundaries)?;
        let mut maximum = 0.0_f32;
        for p in &mut chapter.paragraphs {
            maximum = maximum.max(p.bottom);
            p.max_bottom = maximum;
        }
        if let Some(points) = &fill_positions {
            chapter
                .padded_backgrounds
                .extend(crate::vertical_padding::fills_with_exclusions(
                    source,
                    styles,
                    publisher_boxes.as_deref(),
                    points,
                    settings,
                    &generated.excluded,
                ));
        }
        if let (Some(b), Some(boxes)) = (&borders, &publisher_boxes) {
            chapter.table_frames.extend(positions.finish(
                b,
                boxes,
                styles,
                &mut chapter.padded_backgrounds,
            ));
            chapter.padded_backgrounds.sort_by_key(|f| f.element);
        }
        chapter.padded_backgrounds.sort_by_key(|fill| fill.element);
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
        let extent = (row_flow.end(y) - gap).max(painted).max(0.0);
        Ok((chapter, lines, extent))
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
            .chain(
                self.boundaries
                    .values()
                    .filter(|boundary| boundary.generated_only)
                    .map(|boundary| boundary.bottom),
            )
            .chain(self.padded_backgrounds.iter().map(|f| f.painted_bottom()))
            .fold(0.0, f32::max);
        for paragraph in self.paragraphs.iter().rev() {
            if paragraph.background.is_some_and(|gray| gray < 255) {
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
            let ruby = paragraph
                .ruby
                .iter()
                .map(|reading| paragraph.y + reading.bottom)
                .fold(artwork, f32::max);
            for line in paragraph.layout.lines().rev() {
                if text[line.text_range()].chars().any(|ch| !ch.is_whitespace())
                    || line.items().any(|item| matches!(item,
                        PositionedLayoutItem::GlyphRun(run) if run.advance() > 0.0 && run.style().brush.decoration != TextDecoration::default()))
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

    pub(crate) fn source_at(&self, x: f32, y: f32, visible: &Range<f32>) -> Option<(usize, usize)> {
        let start = self.paragraphs.partition_point(|p| p.max_bottom <= y);
        let paragraph = self.paragraphs[start..]
            .iter()
            .take_while(|p| p.y <= y)
            .find(|p| p.bottom > y && p.column.contains(&x))?;
        let (cluster, _) =
            parley::Cluster::from_point_exact(&paragraph.layout, x - paragraph.x, y - paragraph.y)?;
        let line = cluster.line();
        let metrics = line.metrics();
        if paragraph.y + metrics.content_block_max_coord <= visible.start
            || paragraph.y + metrics.content_block_min_coord >= visible.end
        {
            return None;
        }
        let offset = cluster.text_range().start;
        let offset = match &paragraph.flow {
            Some(flow) => flow.source_offset(offset, crate::flow_projection::Affinity::After)?,
            None => offset,
        };
        Some((paragraph.key, offset))
    }

    pub(crate) fn mapping(&self, key: usize) -> Option<&OwnedProjection> {
        self.paragraphs
            .iter()
            .find(|paragraph| paragraph.key == key)?
            .flow
            .as_deref()
    }

    pub(crate) fn boundary(&self, origin: usize) -> Option<generated_boxes::BoundaryGeometry> {
        self.boundaries.get(&origin).copied()
    }

    pub(crate) fn generated_box_ranges(&self) -> impl Iterator<Item = Range<f32>> + '_ {
        self.boundaries
            .values()
            .filter(|area| area.generated_only && area.bottom > area.top)
            .map(|area| area.top..area.bottom)
    }

    pub(crate) fn boundary_in(
        &self,
        range: &Range<f32>,
    ) -> Option<generated_boxes::BoundaryGeometry> {
        self.boundaries
            .values()
            .filter(|boundary| {
                boundary.generated_only && boundary.bottom > range.start && boundary.top < range.end
            })
            .min_by(|a, b| a.top.total_cmp(&b.top))
            .copied()
    }

    pub(crate) fn limitations(&self) -> &[String] {
        &self.limitations
    }

    pub(crate) fn image_top(&self, key: usize, ordinal: usize) -> Option<f32> {
        self.illustrations
            .iter()
            .find(|image| image.block == key && image.source_index == ordinal)
            .map(|image| image.y)
    }

    pub(crate) fn image_ordinal(&self, key: usize, top: f32) -> Option<usize> {
        self.illustrations
            .iter()
            .find(|image| {
                image.block == key
                    && image
                        .line_top
                        .is_some_and(|line_top| (line_top - top).abs() < 0.01)
            })
            .map(|image| image.source_index)
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

    pub(crate) fn selection_layouts(
        &self,
        page: Range<f32>,
    ) -> impl Iterator<Item = (usize, f32, f32, Range<f32>, &Layout<PaintBrush>)> {
        let start = self
            .paragraphs
            .partition_point(|paragraph| paragraph.max_bottom <= page.start);
        self.paragraphs[start..]
            .iter()
            .take_while(move |paragraph| paragraph.y < page.end)
            .filter(move |p| p.bottom > page.start)
            .map(|p| (p.key, p.x, p.y, p.column.clone(), &p.layout))
    }

    pub fn capture(&mut self, range: Range<f32>, settings: Settings) -> Result<Vec<u8>> {
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
            if paragraph.y >= range.end {
                break;
            }
            if paragraph.bottom <= range.start {
                continue;
            }
            if let Some(gray) = paragraph.background {
                let top = (paragraph.y - range.start).floor().max(0.0) as usize;
                let bottom = ((paragraph.bottom - range.start).ceil() as usize).min(visible_height);
                for row in top.min(bottom)..bottom {
                    let left = margin + paragraph.column.start as usize;
                    let right = margin + paragraph.column.end as usize;
                    page[row * stride + left..row * stride + right].fill(gray);
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
                        if !run.style().brush.painted {
                            continue;
                        }
                        self.glyphs.draw_run(
                            page,
                            stride,
                            &columns,
                            (paragraph.x, paragraph.y - range.start),
                            &run,
                            settings.text_contrast,
                            settings.font_gamma_index,
                        );
                        let decoration = run.style().brush.decoration;
                        if decoration != TextDecoration::default() {
                            let metrics = run.run().font_metrics();
                            let thickness = metrics.underline_size.round().clamp(1.0, 12.0);
                            let baseline = paragraph.y - range.start + run.baseline();
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
                            let x0 = margin
                                + (paragraph.x + run.offset()).floor().clamp(0.0, limit) as usize;
                            let x1 = margin
                                + (paragraph.x + run.offset() + run.advance())
                                    .ceil()
                                    .clamp(0.0, limit) as usize;
                            for y in rows.into_iter().flatten() {
                                let start = y.round().clamp(0.0, visible_height as f32) as usize;
                                let end = (y.round() + thickness).clamp(0.0, visible_height as f32)
                                    as usize;
                                for row in start..end {
                                    page[row * stride + x0..row * stride + x1].fill(48);
                                }
                            }
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
        Ok(frame)
    }
}

fn visible_rows(range: &Range<f32>, settings: Settings) -> usize {
    (range.end - range.start)
        .ceil()
        .clamp(1.0, settings.text_height() as f32) as usize
}

#[derive(Clone, Copy)]
struct LineIndent {
    common: u32,
    amount: f32,
    hanging: bool,
}

impl LineIndent {
    fn resolve(style: &Style, content_width: u32, width: u32, font: f32, marker: u32) -> Self {
        // Keep enough of the column for a word or glyph, even for malformed
        // publisher lengths. An outdent at the reading margin uses the first
        // available column and shifts the other lines inward (as CREngine does
        // when page overflow is disabled). This also protects adjacent cells.
        let limit = (width as f32 - font.min(width as f32 / 2.0)).max(0.0);
        let requested = style.text_indent.pixels(content_width, font);
        let css = if requested < 0.0 {
            style.text_indent.box_pixels(content_width, font)
        } else {
            requested
        }
        .clamp(-limit, limit);
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
        );
    }
    let mut candidates = Vec::new();
    for ruby in block.ruby.iter() {
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
            painted: ruby.painted,
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
            painted: candidate.painted,
        });
    }
    overlays.sort_by_key(|overlay| overlay.line);
    (layout, overlays)
}

fn base_geometry(
    layout: &Layout<PaintBrush>,
    bytes: &Range<usize>,
) -> Option<(BoundingBox, usize)> {
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
    font_size: f32,
    flow: &ChapterFlow,
) -> HashMap<usize, f32> {
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
                let estimate =
                    font_size * style.size_scale * (0.76 * label.chars().count() as f32 + 0.46);
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
    builder.push_default(StyleProperty::FontSize(
        (settings.font_size * style.size_scale * 0.48).clamp(8.0, 84.0),
    ));
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
) -> Layout<PaintBrush> {
    let style = &styles[block.element];
    let language = block.language;
    let mut builder = context.ranged_builder(fonts, &block.text, 1.0, true);
    let font_weights = style.font_weights.clone();
    let weight_spacing = move |font: &parley::FontData, requested: f32, size: f32| {
        crate::font_synthesis::strength(font, requested, size, &font_weights)
    };
    builder.set_font_weight_spacing(Some(&weight_spacing));
    builder.set_automatic_hyphens(&automatic.breaks);
    let cjk_spacing = cjk_width::policy(&block.text, settings.cjk_width_percent);
    if let Some(policy) = &cjk_spacing {
        builder.set_cluster_spacing(Some(policy));
    }
    if !settings.font_kerning {
        builder.push_default(StyleProperty::FontFeatures((&DISABLE_KERNING).into()));
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
    if settings.word_spacing != 0.0 {
        builder.push_default(StyleProperty::WordSpacing(settings.word_spacing));
    }
    if settings.letter_spacing != 0.0 {
        builder.push_default(StyleProperty::LetterSpacing(settings.letter_spacing));
    }
    builder.push_default(StyleProperty::Brush(PaintBrush {
        decoration: style.decoration,
        painted: style.flow.painted,
    }));
    builder.push_default(StyleProperty::FontWeight(weight(style)));
    builder.push_default(StyleProperty::FontStyle(style.font_style));
    builder.push_default(StyleProperty::Locale(language));
    let paragraph_line_break = style.line_break.unwrap_or_else(|| line_break(language));
    builder.push_default(StyleProperty::LineBreak(paragraph_line_break));
    builder.push_default(StyleProperty::WordBreak(style.word_break));
    builder.set_base_direction(block.direction);
    for text in block.styled_text.iter() {
        let run = &styles[text.element];
        if text.language != language {
            builder.push(StyleProperty::Locale(text.language), text.bytes.clone());
        }
        let run_line_break = run.line_break.unwrap_or_else(|| line_break(text.language));
        if run_line_break != paragraph_line_break {
            builder.push(StyleProperty::LineBreak(run_line_break), text.bytes.clone());
        }
        if run.decoration != style.decoration || run.flow.painted != style.flow.painted {
            builder.push(
                StyleProperty::Brush(PaintBrush {
                    decoration: run.decoration,
                    painted: run.flow.painted,
                }),
                text.bytes.clone(),
            );
        }
        if run.text_rise != style.text_rise {
            builder.push(
                StyleProperty::VerticalAlign(run.text_rise.from_block(style)),
                text.bytes.clone(),
            );
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
    layout.break_all_lines(Some(width as f32));
    let alignment = match settings.alignment {
        crate::TextAlignment::Publisher => style.alignment.unwrap_or(Alignment::Start),
        crate::TextAlignment::Start => Alignment::Start,
        crate::TextAlignment::Center => Alignment::Center,
        crate::TextAlignment::Justify => Alignment::Justify,
    };
    layout.align_with_word_expansion(
        alignment,
        AlignmentOptions::default(),
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
    render_dpi: u16,
}

#[cfg(test)]
mod dpi_tests {
    use super::*;

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
    fn key<'a>(&self, path: &'a str) -> (&'a str, Option<u64>, Option<u64>, u32, u32, u16) {
        (
            path,
            self.requested_width.map(f64::to_bits),
            self.requested_height.map(f64::to_bits),
            self.height,
            self.width,
            self.render_dpi,
        )
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
        let font_size = (settings.font_size * style.size_scale).clamp(12.0, 192.0);
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
        Self {
            width,
            height,
            requested_width,
            requested_height,
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
        let w = self.requested_width.unwrap_or_else(|| {
            self.requested_height
                .map_or(f64::from(original_w) * intrinsic_scale, |h| h * ratio)
        });
        let h = self.requested_height.unwrap_or(w / ratio);
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
            path: Some(path),
            link: None,
            label: String::new(),
            x: Some(0),
            y: 0.0,
            line_top: None,
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
mod inline_baseline_tests;

#[cfg(test)]
mod publisher_face_tests;

#[cfg(test)]
#[path = "parley_layout/word_expansion_cases.rs"]
mod word_expansion_cases;

#[cfg(test)]
mod cjk_width_tests;

#[cfg(test)]
mod publisher_padding_tests;

#[cfg(test)]
mod table_border_tests;
#[cfg(test)]
mod vertical_padding_tests;

#[cfg(test)]
mod flow_tests;

#[cfg(test)]
mod media_line_tests;
