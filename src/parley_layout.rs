use std::ops::Range;

use crate::book_style::{BookStyle, Style};
use crate::epub::Book;
use crate::gray_glyph::GlyphCache;
use crate::source::{Block, Source};
use crate::{Line, Result, Settings, image_reader};
use image::{GrayImage, Luma, imageops::FilterType};
use parley::{
    Alignment, AlignmentOptions, BaseDirection, FontContext, FontFamily, FontStyle, FontWeight,
    GenericFamily, Layout, LayoutContext, LineHeight, OverflowWrap, PositionedLayoutItem,
    StyleProperty,
};

const MAX_TEXT_BYTES: usize = 512 * 1024;
const MAX_LINES: usize = 30_000;
const MAX_HEIGHT: f32 = 2_000_000.0;
const MAX_IMAGES: usize = 48;
const MAX_IMAGE_PIXELS: usize = 8 * 1024 * 1024;

struct Paragraph {
    key: usize,
    y: f32,
    bottom: f32,
    background: Option<u8>,
    layout: Layout<()>,
}

struct Illustration {
    y: f32,
    pixels: GrayImage,
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
            glyphs: GlyphCache::default(),
        };
        let mut context = LayoutContext::new();
        let styles = BookStyle::load(
            book,
            href,
            &source.stylesheets,
            &source.inline_sheets,
            &mut fonts,
        );
        let body = styles.properties("body", "", None, None);
        let mut lines = Vec::new();
        let mut y = 0.0_f32;
        let mut image_budget = MAX_IMAGE_PIXELS;
        let width = settings.width - 2 * settings.margin;
        let gap = settings.font_size * 0.32;
        for block in &source.blocks {
            if !block.text.is_empty() {
                let style = styles.properties(
                    &block.tag,
                    &block.classes,
                    block.id.as_deref(),
                    block.inline_style.as_deref(),
                );
                let layout = make_paragraph(
                    &mut context,
                    &mut fonts,
                    block,
                    width,
                    settings,
                    &style,
                    &body,
                );
                if lines.len() + layout.len() > MAX_LINES {
                    return Err("The chapter exceeds the Parley line limit".into());
                }
                let origin = y - layout
                    .lines()
                    .next()
                    .map_or(0.0, |line| line.metrics().content_block_min_coord.min(0.0));
                let mut bottom = origin + layout.height();
                for line in layout.lines() {
                    let metrics = line.metrics();
                    let top = origin + metrics.content_block_min_coord;
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
                });
            }

            for image_href in &block.images {
                if chapter.illustrations.len() == MAX_IMAGES {
                    return Err("The chapter exceeds the Parley image limit".into());
                }
                let pixels = load_image(
                    book,
                    href,
                    image_href,
                    width,
                    settings.text_height(),
                    &mut image_budget,
                )
                .unwrap_or_else(|| GrayImage::from_pixel(width.min(64), 36, Luma([225])));
                let top = y.ceil();
                let bottom = top + pixels.height() as f32;
                lines.push(Line {
                    block: block.key,
                    bytes: 0..0,
                    top,
                    bottom,
                    missing_glyphs: 0,
                });
                chapter.illustrations.push(Illustration { y: top, pixels });
                y = bottom + gap;
            }

            if y > MAX_HEIGHT {
                return Err("The chapter exceeds the Parley height limit".into());
            }
        }

        let extent = (y - gap).max(0.0);
        Ok((chapter, lines, extent))
    }

    pub(crate) fn source_at(&self, x: f32, y: f32) -> Option<(usize, usize)> {
        let paragraph = self.paragraphs.get(
            self.paragraphs
                .partition_point(|paragraph| paragraph.bottom <= y),
        )?;
        let (cluster, _) =
            parley::Cluster::from_point_exact(&paragraph.layout, x, y - paragraph.y)?;
        Some((paragraph.key, cluster.text_range().start))
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
        let visible_height = (range.end - range.start)
            .ceil()
            .clamp(1.0, settings.text_height() as f32) as usize;
        let mut frame = vec![255; (settings.width * settings.height) as usize];
        let stride = settings.width as usize;
        let margin = settings.margin as usize;
        let page = &mut frame[margin * stride..(margin + visible_height) * stride];

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
                        self.glyphs
                            .draw_run(page, stride, margin, paragraph.y - range.start, &run);
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
        }

        for illustration in &self.illustrations {
            illustration.draw(page, &range, stride, margin);
        }
        Ok(frame)
    }
}

fn make_paragraph(
    context: &mut LayoutContext<()>,
    fonts: &mut FontContext,
    block: &Block,
    width: u32,
    settings: Settings,
    style: &Style,
    body: &Style,
) -> Layout<()> {
    let mut builder = context.ranged_builder(fonts, &block.text, 1.0, true);
    let font = if settings.override_fonts {
        None
    } else {
        style.font_family.as_deref().or(body.font_family.as_deref())
    };
    let family = font.map_or_else(|| GenericFamily::Serif.into(), FontFamily::named);
    builder.push_default(StyleProperty::FontFamily(family));
    let default_spacing = Settings::default().line_height;
    let publisher_spacing = style
        .line_height
        .or(body.line_height)
        .unwrap_or(default_spacing);
    let line_height = (publisher_spacing * settings.line_height / default_spacing).clamp(1.0, 3.0);
    builder.push_default(StyleProperty::LineHeight(LineHeight::FontSizeRelative(
        line_height,
    )));
    builder.push_default(StyleProperty::OverflowWrap(OverflowWrap::Anywhere));
    let block_scale = style
        .size_scale
        .unwrap_or(if block.heading { 1.4 } else { 1.0 });
    let size =
        (settings.font_size * body.size_scale.unwrap_or(1.0) * block_scale).clamp(12.0, 192.0);
    builder.push_default(StyleProperty::FontSize(size));
    if block.heading || settings.bold_text {
        builder.push_default(StyleProperty::FontWeight(FontWeight::BOLD));
    }
    if block.rtl {
        builder.set_base_direction(BaseDirection::Rtl);
    }
    for (bytes, emphasis) in &block.emphasis {
        if emphasis.bold {
            builder.push(StyleProperty::FontWeight(FontWeight::BOLD), bytes.clone());
        }
        if emphasis.italic {
            builder.push(StyleProperty::FontStyle(FontStyle::Italic), bytes.clone());
        }
    }
    for link in &block.links {
        builder.push(StyleProperty::Underline(true), link.bytes.clone());
    }
    let mut layout = builder.build(&block.text);
    layout.break_all_lines(Some(width as f32));
    let alignment = match settings.alignment {
        crate::TextAlignment::Publisher => style
            .alignment
            .or(body.alignment)
            .unwrap_or(Alignment::Start),
        crate::TextAlignment::Start => Alignment::Start,
        crate::TextAlignment::Center => Alignment::Center,
        crate::TextAlignment::Justify => Alignment::Justify,
    };
    layout.align(alignment, AlignmentOptions::default());
    layout
}

fn load_image(
    book: &mut Book,
    chapter_href: &str,
    image_href: &str,
    width: u32,
    height: u32,
    budget: &mut usize,
) -> Option<GrayImage> {
    let path = Book::resolve(chapter_href, image_href).ok()?;
    let bytes = book.resource(&path).ok()?;
    let (original_w, original_h) = image_reader(&bytes).ok()?.into_dimensions().ok()?;
    if original_w == 0 || original_h == 0 || original_w > 2048 || original_h > 2048 {
        return None;
    }
    *budget = budget.checked_sub(original_w as usize * original_h as usize)?;
    let mut image = image_reader(&bytes).ok()?.decode().ok()?;
    if original_w > width || original_h > height {
        image = image.resize(width, height, FilterType::Triangle);
    }
    let luma = image.into_luma_alpha8();
    Some(GrayImage::from_fn(luma.width(), luma.height(), |x, y| {
        let [tone, opacity] = luma.get_pixel(x, y).0.map(u16::from);
        Luma([((tone * opacity + 255 * (255 - opacity) + 127) / 255) as u8])
    }))
}

impl Illustration {
    fn draw(&self, page: &mut [u8], range: &Range<f32>, stride: usize, margin: usize) {
        let top = (self.y - range.start).round() as i32;
        let height = (page.len() / stride) as i32;
        let first = (-top).max(0);
        let last = (height - top).min(self.pixels.height() as i32);
        let width = self.pixels.width() as usize;
        let x = margin + (stride - 2 * margin - width) / 2;
        let pixels = self.pixels.as_raw();
        for row in first..last {
            let source = row as usize * width;
            let start = (top + row) as usize * stride + x;
            page[start..start + width].copy_from_slice(&pixels[source..source + width]);
        }
    }
}
