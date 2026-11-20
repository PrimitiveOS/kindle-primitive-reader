use std::ops::Range;

use crate::book_style::{BookStyle, Style};
use crate::comic::ImagePage;
use crate::epub::Book;
use crate::gray_glyph::GlyphCache;
use crate::source::{Block, Source};
use crate::svg_image;
use crate::{Line, Result, Settings, image_reader};
use image::{DynamicImage, GrayImage, Luma, imageops::FilterType};
use parley::{
    Alignment, AlignmentOptions, BaseDirection, FontContext, FontFamily, FontWeight, GenericFamily,
    Layout, LayoutContext, LineHeight, OverflowWrap, PositionedLayoutItem, StyleProperty,
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
    path: Option<String>,
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
        let styles =
            BookStyle::load(book, href, &source.stylesheets, &mut fonts).compute(&source.elements);
        let mut lines = Vec::new();
        let mut y = 0.0_f32;
        let mut image_budget = MAX_IMAGE_PIXELS;
        let width = settings.width - 2 * settings.margin;
        let gap = settings.font_size * 0.32;
        for block in &source.blocks {
            if !block.text.is_empty() {
                let style = &styles[block.element];
                let layout =
                    make_paragraph(&mut context, &mut fonts, block, width, settings, &styles);
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
                let fitted = load_image(
                    book,
                    href,
                    image_href,
                    width,
                    settings.text_height(),
                    &mut image_budget,
                );
                let (path, pixels) = fitted.map_or_else(
                    || (None, GrayImage::from_pixel(width.min(64), 36, Luma([225]))),
                    |(path, pixels)| (Some(path), pixels),
                );
                let top = y.ceil();
                let bottom = top + pixels.height() as f32;
                lines.push(Line {
                    block: block.key,
                    bytes: 0..0,
                    top,
                    bottom,
                    missing_glyphs: 0,
                });
                chapter.illustrations.push(Illustration {
                    path,
                    y: top,
                    pixels,
                });
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

    pub(crate) fn illustration_at(
        &self,
        range: Range<f32>,
        settings: Settings,
        x: f32,
        y: f32,
    ) -> Option<&str> {
        let content_y = y - settings.margin as f32;
        if !(0.0..visible_rows(&range, settings) as f32).contains(&content_y) {
            return None;
        }
        self.illustrations.iter().find_map(|illustration| {
            let path = illustration.path.as_deref()?;
            let (left, top) = illustration.origin(
                range.start,
                settings.width as usize,
                settings.margin as usize,
            );
            let (left, top) = (left as f32, top as f32);
            let right = left + illustration.pixels.width() as f32;
            let bottom = top + illustration.pixels.height() as f32;
            ((left..right).contains(&x) && (top..bottom).contains(&content_y)).then_some(path)
        })
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

fn visible_rows(range: &Range<f32>, settings: Settings) -> usize {
    (range.end - range.start)
        .ceil()
        .clamp(1.0, settings.text_height() as f32) as usize
}

fn make_paragraph(
    context: &mut LayoutContext<()>,
    fonts: &mut FontContext,
    block: &Block,
    width: u32,
    settings: Settings,
    styles: &[Style],
) -> Layout<()> {
    let style = &styles[block.element];
    let mut builder = context.ranged_builder(fonts, &block.text, 1.0, true);
    let family: FontFamily<'_> = if settings.override_fonts {
        GenericFamily::Serif.into()
    } else {
        style.font_family.layout()
    };
    builder.push_default(StyleProperty::FontFamily(family));
    let default_spacing = Settings::default().line_height;
    let line_height = |style: &Style| {
        let publisher = style.line_height.unwrap_or(default_spacing);
        LineHeight::FontSizeRelative(
            (publisher * settings.line_height / default_spacing).clamp(1.0, 3.0),
        )
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
    builder.push_default(StyleProperty::FontWeight(weight(style)));
    builder.push_default(StyleProperty::FontStyle(style.font_style));
    if block.rtl {
        builder.set_base_direction(BaseDirection::Rtl);
    }
    for text in &block.styled_text {
        let run = &styles[text.element];
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

fn load_image(
    book: &mut Book,
    chapter_href: &str,
    image_href: &str,
    width: u32,
    height: u32,
    budget: &mut usize,
) -> Option<(String, GrayImage)> {
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
        let image = svg_image::rasterize(&bytes, width, height, *budget).ok()?;
        *budget -= image.width() as usize * image.height() as usize;
        return Some((path, image));
    }
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
    Some((path, grayscale_image(image)))
}

pub(crate) fn original_illustration(bytes: &[u8]) -> Result<ImagePage> {
    let gray = match image_reader(bytes) {
        Ok(reader) => grayscale_image(reader.decode()?),
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
        let left = margin + (stride - 2 * margin - self.pixels.width() as usize) / 2;
        (left, (self.y - start).round() as i32)
    }

    fn draw(&self, page: &mut [u8], range: &Range<f32>, stride: usize, margin: usize) {
        let (x, top) = self.origin(range.start, stride, margin);
        let height = (page.len() / stride) as i32;
        let first = (-top).max(0);
        let last = (height - top).min(self.pixels.height() as i32);
        let width = self.pixels.width() as usize;
        let pixels = self.pixels.as_raw();
        for row in first..last {
            let source = row as usize * width;
            let start = (top + row) as usize * stride + x;
            page[start..start + width].copy_from_slice(&pixels[source..source + width]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        assert_eq!(image.dimensions(), (80, 40));
        assert!(image.get_pixel(4, 4).0[0] < 32);
        assert!(image.get_pixel(76, 4).0[0] > 224);
        let original = probe.open_illustration(&publication.0, package).unwrap();
        assert_eq!(original.page_size(), (80.0, 40.0));
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
        let fonts = crate::font_context(&fonts).unwrap();
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
                ParleyChapter::new(&mut book, &path, &source, fonts.clone(), settings).unwrap();
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
