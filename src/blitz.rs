use std::ops::Range;
use std::sync::{Arc, LazyLock, Mutex};

use anyrender_vello_cpu::VelloCpuScenePainter;
use blitz_dom::{DocumentConfig, Point, UserAgentStylesheet};
use blitz_html::HtmlDocument;
use blitz_traits::net::{Bytes, Method, NetHandler, NetProvider, Request, Url};
use blitz_traits::shell::{ColorScheme, Viewport};
use parley::FontContext;

use crate::epub::Book;
use crate::source::{SOURCE_ATTRIBUTE, Source};
use crate::{Line, Result, Settings, grayscale, svg_image};

const RESOURCE_HOST: &str = "epub.invalid";
const MAX_RESOURCES: usize = 64;
const MAX_RESOURCE_BYTES: usize = 20 * 1024 * 1024;
const MAX_IMAGE_PIXELS: usize = 8 * 1024 * 1024;

struct ArchiveResources {
    book: Option<Book>,
    requests: usize,
    bytes: usize,
    pixels: usize,
}

impl ArchiveResources {
    fn get(&mut self, request: &Request) -> Option<Vec<u8>> {
        self.requests += 1;
        if self.requests > MAX_RESOURCES
            || self.bytes >= MAX_RESOURCE_BYTES
            || request.method != Method::GET
            || request.url.scheme() != "https"
            || request.url.host_str() != Some(RESOURCE_HOST)
            || !request.url.username().is_empty()
            || request.url.port().is_some()
            || request.url.password().is_some()
        {
            return None;
        }
        let path = request.url.path().strip_prefix('/')?;
        let extension = path.rsplit_once('.')?.1.to_ascii_lowercase();
        if !matches!(
            extension.as_str(),
            "css" | "png" | "jpg" | "jpeg" | "svg" | "ttf" | "otf"
        ) {
            return None;
        }
        let path = Book::resolve("index.xhtml", path).ok()?;
        let available = MAX_RESOURCE_BYTES - self.bytes;
        let limit = if extension == "svg" {
            available.min(svg_image::MAX_SVG_BYTES)
        } else {
            available
        };
        let bytes = self.book.as_mut()?.resource_limited(&path, limit).ok()?;
        self.bytes += bytes.len();
        if extension == "svg" {
            let (data, pixels) =
                svg_image::png_resource(&bytes, MAX_IMAGE_PIXELS.saturating_sub(self.pixels))
                    .ok()?;
            if data.len() > MAX_RESOURCE_BYTES - self.bytes {
                return None;
            }
            self.bytes += data.len();
            self.pixels += pixels;
            return Some(data);
        }
        if matches!(extension.as_str(), "png" | "jpg" | "jpeg") {
            let (w, h) = crate::epub_raster_dimensions(&bytes).ok()?;
            self.pixels += w as usize * h as usize;
            if self.pixels > MAX_IMAGE_PIXELS {
                return None;
            }
        }
        Some(bytes)
    }
}

struct OfflineResources(Mutex<ArchiveResources>);

impl NetProvider for OfflineResources {
    fn fetch(&self, _doc_id: usize, request: Request, handler: Box<dyn NetHandler>) {
        // Release the archive before the callback. CSS imports can request another resource.
        let response = self
            .0
            .lock()
            .ok()
            .and_then(|mut archive| archive.get(&request));
        handler.bytes(
            request.url.to_string(),
            response.map(Bytes::from).unwrap_or_default(),
        );
    }
}

fn layout_document(
    book: Book,
    href: &str,
    html: &str,
    fonts: FontContext,
    size: (u32, u32),
    fixed: bool,
) -> Result<HtmlDocument> {
    let mut url = Url::parse("https://epub.invalid/")?;
    url.path_segments_mut()
        .map_err(|_| "The chapter URL is invalid")?
        .pop_if_empty()
        .extend(href.split('/'));
    let resources = Arc::new(OfflineResources(Mutex::new(ArchiveResources {
        book: Some(book),
        requests: 0,
        bytes: 0,
        pixels: 0,
    })));
    let mut config = DocumentConfig {
        viewport: Some(Viewport::new(size.0, size.1, 1.0, ColorScheme::Light)),
        base_url: Some(url.into()),
        font_ctx: Some(fonts),
        net_provider: Some(resources.clone()),
        ..DocumentConfig::default()
    };
    let mut doc = if fixed {
        static FIXED_DEFAULTS: LazyLock<UserAgentStylesheet> =
            LazyLock::new(|| UserAgentStylesheet::parse("html, body { margin: 0; padding: 0; }"));
        config.ua_stylesheets.push(FIXED_DEFAULTS.clone());
        HtmlDocument::from_xml(html, config)
    } else {
        HtmlDocument::from_html(html, config)
    };
    for _ in 0..16 {
        doc.handle_messages();
        doc.resolve(0.0);
        if !doc.has_pending_critical_resources() {
            doc.handle_messages();
            doc.resolve(0.0);
            resources
                .0
                .lock()
                .map_err(|_| "The EPUB resource reader failed")?
                .book = None;
            return Ok(doc);
        }
    }
    Err("The chapter exceeded the resource processing limit".into())
}

/// Render a single author's fixed viewport without reflow or reader typography.
pub(crate) fn fixed_page(
    book: Book,
    href: &str,
    xhtml: &str,
    fonts: FontContext,
    (width, height): (u32, u32),
) -> Result<Vec<u8>> {
    let mut doc = layout_document(book, href, xhtml, fonts, (width, height), true)?;
    let mut painter = VelloCpuScenePainter::new(width as u16, height as u16);
    blitz_paint::paint_scene(&mut painter, &mut doc, 1.0, width, height, 0, 0);
    let image = painter.finish();
    Ok(grayscale(image.data_as_u8_slice()).collect())
}

pub(crate) struct BlitzChapter {
    doc: HtmlDocument,
}

impl BlitzChapter {
    pub fn new(
        book: Book,
        href: &str,
        xhtml: &str,
        source: &Source,
        fonts: FontContext,
        settings: Settings,
    ) -> Result<(Self, Vec<Line>, f32)> {
        let css = format!(
            "html, body {{ padding: 0; }} body {{ margin: 0 {}px; font: {}px/{} serif; }}
             img {{ max-width: 100%; max-height: {}px; object-fit: contain; }}
             iframe, frame, embed, object {{ display: none !important; }}",
            settings.side_margin(),
            settings.font_size,
            settings.line_height,
            settings.text_height(),
        );
        let html = source.with_styles(xhtml, &css);
        let doc = layout_document(
            book,
            href,
            &html,
            fonts,
            (settings.width, settings.text_height()),
            false,
        )?;

        let mut lines = Vec::new();
        let mut bottom = 0.0_f32;
        let attr = blitz_dom::LocalName::from(SOURCE_ATTRIBUTE);
        let texts: std::collections::BTreeMap<_, _> = source
            .blocks
            .iter()
            .map(|b| (b.key, b.text.as_str()))
            .collect();
        let mut nodes = vec![(doc.root_element().id, 0.0_f32)];
        while let Some((id, parent_y)) = nodes.pop() {
            let Some(node) = doc.get_node(id) else {
                continue;
            };
            let layout = node.final_layout();
            let y = parent_y + layout.location.y;
            bottom = bottom.max(y + layout.size.height);
            let children = node.layout_children.borrow();
            let children: &[blitz_dom::NodeId] = children.as_deref().unwrap_or(&node.children);
            nodes.extend(children.iter().rev().map(|&child| (child, y)));

            if layout.size.height <= 0.0 {
                continue;
            }
            let key = node
                .attr(attr.clone())
                .and_then(|value| value.parse::<usize>().ok());
            let inline = node
                .element_data()
                .and_then(|e| e.inline_layout_data.as_ref());
            if let (Some(key), Some(inline)) = (key, inline) {
                let Some(&text) = texts.get(&key) else {
                    continue;
                };
                let leading = inline.text.len() - inline.text.trim_start_matches(' ').len();
                let consistent = inline.text.trim_matches(' ') == text;
                let origin = y + layout.padding.top + layout.border.top + inline.block_offset;
                for line in inline.layout.lines() {
                    let metrics = line.metrics();
                    let bytes = line.text_range();
                    lines.push(Line {
                        block: key,
                        bytes: if consistent {
                            bytes.start.saturating_sub(leading)
                                ..bytes.end.saturating_sub(leading).min(text.len())
                        } else {
                            0..0
                        },
                        top: origin + metrics.content_block_min_coord,
                        bottom: origin + metrics.content_block_max_coord,
                        missing_glyphs: line
                            .items()
                            .filter_map(|item| match item {
                                parley::PositionedLayoutItem::GlyphRun(run) => Some(
                                    run.positioned_glyphs()
                                        .filter(|glyph| glyph.id == 0)
                                        .count(),
                                ),
                                _ => None,
                            })
                            .sum(),
                    });
                }
            }
            if node
                .element_data()
                .is_some_and(|element| &*element.name.local == "img")
            {
                lines.push(Line {
                    block: 0,
                    bytes: 0..0,
                    top: y,
                    bottom: y + layout.size.height,
                    missing_glyphs: 0,
                });
            }
        }
        Ok((Self { doc }, lines, bottom))
    }

    pub fn capture(&mut self, range: Range<f32>, settings: Settings) -> Result<Vec<u8>> {
        let height = settings.text_height();
        self.doc.set_viewport_scroll(Point {
            x: 0.0,
            y: f64::from(range.start.floor()),
        });
        let mut painter = VelloCpuScenePainter::new(settings.width as u16, height as u16);
        blitz_paint::paint_scene(
            &mut painter,
            &mut self.doc,
            1.0,
            settings.width,
            height,
            0,
            0,
        );
        let image = painter.finish();
        let mut gray = vec![255; (settings.width * settings.height) as usize];
        let visible = (range.end - range.start.floor())
            .floor()
            .clamp(0.0, height as f32) as usize;
        let start = (settings.top_margin() * settings.width) as usize;
        let count = visible * settings.width as usize;
        gray[start..start + count]
            .iter_mut()
            .zip(grayscale(image.data_as_u8_slice()))
            .for_each(|(pixel, v)| *pixel = v);
        Ok(gray)
    }
}
