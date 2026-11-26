//! Static SVG illustrations with optional bounded raster resources from the open book.

use std::collections::HashMap;
use std::fmt::Write;
use std::sync::Arc;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use image::{GrayImage, ImageDecoder};
use resvg::{tiny_skia, usvg};
use roxmltree::{Document, ParsingOptions};

use crate::Result;

pub(crate) const MAX_SVG_BYTES: usize = 256 * 1024;
pub(crate) const MAX_SVG_SIDE: u32 = 2_048;
pub(crate) const MAX_SVG_PIXELS: usize = 2 * 1024 * 1024;
const MAX_XML_NODES: u32 = 4_096;
const MAX_DEPTH: u32 = 32;
const MAX_VECTOR_BYTES: usize = 128 * 1024;
const MAX_INTRINSIC_SIDE: f64 = 32_768.0;
const MAX_PAINT_PIXELS: usize = 64 * 1024 * 1024;
const SVG_NAMESPACE: &str = "http://www.w3.org/2000/svg";

pub(crate) fn is_package_svg(path: &str) -> bool {
    path.rsplit_once('.')
        .is_some_and(|(_, extension)| extension.eq_ignore_ascii_case("svg"))
}

pub(crate) fn rasterize(
    bytes: &[u8],
    max_width: u32,
    max_height: u32,
    available_pixels: usize,
) -> Result<GrayImage> {
    rasterize_with_size(bytes, max_width, max_height, available_pixels, None)
}

pub(crate) fn rasterize_with_size(
    bytes: &[u8],
    max_width: u32,
    max_height: u32,
    available_pixels: usize,
    css_size: Option<&dyn Fn(u32, u32) -> (u32, u32)>,
) -> Result<GrayImage> {
    let viewport = standalone_pixmap(bytes, max_width, max_height, available_pixels, css_size)?;
    grayscale_pixmap(viewport)
}

// Preserve vector-only rendering exactly. Only previously rejected standalone
// documents may take the self-contained data-image fallback.
fn standalone_pixmap(
    bytes: &[u8],
    max_width: u32,
    max_height: u32,
    available_pixels: usize,
    css_size: Option<&dyn Fn(u32, u32) -> (u32, u32)>,
) -> Result<tiny_skia::Pixmap> {
    match render_pixmap(
        bytes,
        max_width,
        max_height,
        available_pixels,
        css_size,
        None,
    ) {
        Ok(viewport) => Ok(viewport),
        Err(original) => {
            let fallback = std::str::from_utf8(bytes).ok().and_then(|xml| {
                pixmap_with_images(
                    None,
                    "",
                    xml,
                    max_width,
                    max_height,
                    available_pixels,
                    css_size,
                )
                .ok()
            });
            fallback.map(|(viewport, _)| viewport).ok_or(original)
        }
    }
}

fn grayscale_pixmap(viewport: tiny_skia::Pixmap) -> Result<GrayImage> {
    GrayImage::from_raw(
        viewport.width(),
        viewport.height(),
        crate::grayscale(viewport.data()).collect(),
    )
    .ok_or_else(|| "SVG pixels have unexpected dimensions".into())
}

/// Detach a supported graphic from its XHTML without losing inherited XML
/// namespace bindings. Prefixes on descendants continue to resolve normally.
pub(crate) fn inline_source(node: roxmltree::Node<'_, '_>, xhtml: &str) -> Option<String> {
    if node.range().len() > MAX_SVG_BYTES
        || node.descendants().take(MAX_XML_NODES as usize + 1).count() > MAX_XML_NODES as usize
    {
        return None;
    }
    validate(node, true).ok()?;
    let escape = |value: &str| {
        value
            .replace('&', "&amp;")
            .replace('"', "&quot;")
            .replace('<', "&lt;")
    };
    let mut xml = format!("<svg xmlns=\"{SVG_NAMESPACE}\"");
    for namespace in node.namespaces() {
        if let Some(prefix) = namespace.name()
            && prefix != "xml"
        {
            write!(xml, " xmlns:{prefix}=\"{}\"", escape(namespace.uri())).ok()?;
        }
    }
    for attribute in node.attributes() {
        write!(
            xml,
            " {}=\"{}\"",
            attribute.name(),
            escape(attribute.value())
        )
        .ok()?;
    }
    xml.push('>');
    if let (Some(first), Some(last)) = (node.first_child(), node.last_child()) {
        xml.push_str(&xhtml[first.range().start..last.range().end]);
    }
    xml.push_str("</svg>");
    (xml.len() <= MAX_SVG_BYTES).then_some(xml)
}

pub(crate) fn original_package_image(
    book: &mut crate::epub::Book,
    path: &str,
    bytes: &[u8],
) -> Result<crate::comic::ImagePage> {
    let (gray, _) = render_inline(
        book,
        path,
        std::str::from_utf8(bytes)?,
        MAX_SVG_SIDE,
        MAX_SVG_SIDE,
        8 * 1024 * 1024,
        None,
    )?;
    crate::comic::ImagePage::from_grayscale(gray.width(), gray.height(), gray.into_raw())
}

pub(crate) fn render_inline(
    book: &mut crate::epub::Book,
    chapter: &str,
    xml: &str,
    width: u32,
    height: u32,
    budget: usize,
    css_size: Option<&dyn Fn(u32, u32) -> (u32, u32)>,
) -> Result<(GrayImage, usize)> {
    let (viewport, cost) = inline_pixmap(book, chapter, xml, width, height, budget, css_size)?;
    Ok((grayscale_pixmap(viewport)?, cost))
}

pub(crate) fn inline_png_resource(
    book: &mut crate::epub::Book,
    chapter: &str,
    xml: &str,
    budget: usize,
) -> Result<(Vec<u8>, usize)> {
    let (viewport, cost) =
        inline_pixmap(book, chapter, xml, MAX_SVG_SIDE, MAX_SVG_SIDE, budget, None)?;
    Ok((viewport.encode_png()?, cost))
}

fn inline_pixmap(
    book: &mut crate::epub::Book,
    chapter: &str,
    xml: &str,
    width: u32,
    height: u32,
    budget: usize,
    css_size: Option<&dyn Fn(u32, u32) -> (u32, u32)>,
) -> Result<(tiny_skia::Pixmap, usize)> {
    pixmap_with_images(Some(book), chapter, xml, width, height, budget, css_size)
}

fn pixmap_with_images(
    mut book: Option<&mut crate::epub::Book>,
    chapter: &str,
    xml: &str,
    width: u32,
    height: u32,
    budget: usize,
    css_size: Option<&dyn Fn(u32, u32) -> (u32, u32)>,
) -> Result<(tiny_skia::Pixmap, usize)> {
    let standalone = book.is_none();
    if xml.len() > MAX_SVG_BYTES || xml.contains("<!DOCTYPE") {
        return Err("The SVG must be static UTF-8 within 256 KiB".into());
    }
    let doc = Document::parse_with_options(
        xml,
        ParsingOptions {
            nodes_limit: MAX_XML_NODES,
            ..Default::default()
        },
    )?;
    validate(doc.root_element(), true)?;
    let mut images = HashMap::new();
    let mut source_pixels = 0usize;
    let mut source_bytes = 0usize;
    for node in doc
        .descendants()
        .filter(|node| node.has_tag_name((SVG_NAMESPACE, "image")))
    {
        let href = node
            .attribute("href")
            .or_else(|| node.attribute(("http://www.w3.org/1999/xlink", "href")))
            .ok_or("The SVG image reference is missing")?;
        if images.contains_key(href) {
            continue;
        }
        if images.len() >= 8 {
            return Err("The inline SVG has too many image resources".into());
        }
        let remaining = (8 * 1024 * 1024usize).saturating_sub(source_bytes);
        let data = if href.starts_with("data:") {
            svg_data_image(href, remaining)?
        } else {
            let path = crate::epub::Book::resolve(chapter, href)?;
            book.as_deref_mut()
                .ok_or("Standalone SVG images must use embedded data")?
                .resource_limited(&path, remaining)?
        };
        source_bytes = source_bytes
            .checked_add(data.len())
            .ok_or("The SVG raster resource budget overflowed")?;
        let (w, h) = crate::epub_raster_dimensions(&data)?;
        source_pixels += w as usize * h as usize;
        if source_pixels > budget {
            return Err("The inline SVG exceeds its image budget".into());
        }
        let kind = if crate::gif_image::has_signature(&data) {
            let png = crate::gif_image::transparent_png(&data)?;
            source_bytes += png.len();
            if source_bytes > 8 * 1024 * 1024 {
                return Err("The SVG exceeds its raster resource budget".into());
            }
            usvg::ImageKind::PNG(Arc::new(png))
        } else {
            let reader = crate::image_reader(&data)?;
            let format = reader.format();
            let decoded_bytes = reader.into_decoder()?.total_bytes();
            if decoded_bytes > 24 * 1024 * 1024 {
                return Err("The inline SVG image is too large to decode".into());
            }
            match format {
                Some(image::ImageFormat::Png) => usvg::ImageKind::PNG(Arc::new(data)),
                Some(image::ImageFormat::Jpeg) => usvg::ImageKind::JPEG(Arc::new(data)),
                Some(image::ImageFormat::Bmp | image::ImageFormat::Pnm) => {
                    let remaining = (8 * 1024 * 1024usize)
                        .checked_sub(source_bytes)
                        .ok_or("The SVG exceeds its raster resource budget")?;
                    let rgba = usize::try_from(u64::from(w) * u64::from(h) * 4)
                        .map_err(|_| "The SVG image dimensions are too large")?;
                    let declared = usize::try_from(decoded_bytes)
                        .map_err(|_| "The SVG image is too large to decode")?;
                    let combined = source_bytes
                        .checked_add(declared)
                        .and_then(|value| value.checked_add(rgba))
                        // Allow for growth of the capped PNG Vec as it encodes.
                        .and_then(|value| value.checked_add(remaining.checked_mul(2)?))
                        .ok_or("The SVG image memory budget overflowed")?;
                    if combined > 32 * 1024 * 1024 {
                        return Err("The SVG image exceeds its conversion memory budget".into());
                    }
                    let png = svg_extra_raster_png(&data, remaining)?;
                    source_bytes = source_bytes
                        .checked_add(png.len())
                        .ok_or("The SVG raster resource budget overflowed")?;
                    usvg::ImageKind::PNG(Arc::new(png))
                }
                _ => return Err("The SVG image format is unsupported".into()),
            }
        };
        images.insert(href.to_owned(), kind);
    }
    if standalone && images.is_empty() {
        return Err("The SVG has no embedded images".into());
    }
    let viewport = render_pixmap(
        xml.as_bytes(),
        width,
        height,
        budget,
        css_size,
        Some(&images),
    )?;
    let cost = source_pixels.max(viewport.width() as usize * viewport.height() as usize);
    Ok((viewport, cost))
}

// Restrict embedded images to canonical base64 of the three existing raster
// types. The same image and aggregate budgets apply as for local book paths.
fn svg_data_image(href: &str, limit: usize) -> Result<Vec<u8>> {
    let (mime, encoded) = href
        .strip_prefix("data:")
        .and_then(|value| value.split_once(";base64,"))
        .ok_or("The SVG data image must use canonical base64")?;
    if !matches!(mime, "image/png" | "image/jpeg" | "image/gif") || encoded.len() > MAX_SVG_BYTES {
        return Err("The SVG data image type or size is unsupported".into());
    }
    let bytes = STANDARD.decode(encoded)?;
    if bytes.len() > limit || STANDARD.encode(&bytes) != encoded {
        return Err("The SVG data image is noncanonical or exceeds its resource budget".into());
    }
    let valid_signature = match mime {
        "image/png" => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        "image/jpeg" => bytes.starts_with(b"\xff\xd8\xff"),
        "image/gif" => crate::gif_image::has_signature(&bytes),
        _ => false,
    };
    if !valid_signature {
        return Err("The SVG data image does not match its declared type".into());
    }
    Ok(bytes)
}

// Limit PNG output while encoding, rather than allocating an unbounded result
// and checking only after it has already been produced.
struct CappedPng {
    bytes: Vec<u8>,
    limit: usize,
}

impl std::io::Write for CappedPng {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        if data.len() > self.limit.saturating_sub(self.bytes.len()) {
            return Err(std::io::Error::other(
                "The SVG PNG conversion exceeds its resource budget",
            ));
        }
        self.bytes.extend_from_slice(data);
        Ok(data.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn svg_extra_raster_png(data: &[u8], limit: usize) -> Result<Vec<u8>> {
    use image::ImageEncoder;
    let image = crate::image_reader(data)?.decode()?.into_rgba8();
    let mut output = CappedPng {
        bytes: Vec::new(),
        limit,
    };
    image::codecs::png::PngEncoder::new(&mut output).write_image(
        &image,
        image.width(),
        image.height(),
        image::ExtendedColorType::Rgba8,
    )?;
    Ok(output.bytes)
}

pub(crate) fn png_resource(bytes: &[u8], available_pixels: usize) -> Result<(Vec<u8>, usize)> {
    let viewport = standalone_pixmap(bytes, MAX_SVG_SIDE, MAX_SVG_SIDE, available_pixels, None)?;
    let pixels = viewport.width() as usize * viewport.height() as usize;
    Ok((viewport.encode_png()?, pixels))
}

fn render_pixmap(
    bytes: &[u8],
    max_width: u32,
    max_height: u32,
    available_pixels: usize,
    css_size: Option<&dyn Fn(u32, u32) -> (u32, u32)>,
    images: Option<&HashMap<String, usvg::ImageKind>>,
) -> Result<tiny_skia::Pixmap> {
    if bytes.len() > MAX_SVG_BYTES || bytes.starts_with(&[0x1f, 0x8b]) {
        return Err("SVG must be uncompressed UTF-8 within 256 KiB".into());
    }
    let xml = std::str::from_utf8(bytes)?;
    if xml.contains("<!DOCTYPE") {
        return Err("SVG document type declarations are unsupported".into());
    }
    let document = Document::parse_with_options(
        xml,
        ParsingOptions {
            nodes_limit: MAX_XML_NODES,
            ..Default::default()
        },
    )?;
    validate(document.root_element(), images.is_some())?;
    let options = usvg::Options {
        max_depth: MAX_DEPTH,
        image_href_resolver: usvg::ImageHrefResolver {
            resolve_data: Box::new(|mime, data, _| {
                // usvg decodes data URLs itself; allow only the exact canonical
                // URL already parsed, budgeted and validated by inline_pixmap.
                if data.len() > MAX_SVG_BYTES
                    || !matches!(mime, "image/png" | "image/jpeg" | "image/gif")
                {
                    return None;
                }
                let key = format!("data:{mime};base64,{}", STANDARD.encode(data.as_ref()));
                images.and_then(|images| images.get(&key).cloned())
            }),
            resolve_string: Box::new(|href, _| images.and_then(|images| images.get(href).cloned())),
        },
        ..Default::default()
    };
    let tree = usvg::Tree::from_xmltree(&document, &options)?;
    let natural_width = f64::from(tree.size().width());
    let natural_height = f64::from(tree.size().height());
    let (width, height) = fitted_size(natural_width, natural_height, max_width, max_height)?;
    // Render vectors directly at their final CSS size on the reading page.
    // The natural viewport and the requested drawing retain their own limits.
    let (width, height) = css_size.map_or((width, height), |size| {
        size(natural_width.ceil() as u32, natural_height.ceil() as u32)
    });
    if width == 0
        || height == 0
        || width > max_width.min(MAX_SVG_SIDE)
        || height > max_height.min(MAX_SVG_SIDE)
        || width as usize * height as usize > available_pixels.min(MAX_SVG_PIXELS)
    {
        return Err("The SVG exceeds the chapter image budget".into());
    }
    let scale = (
        f64::from(width) / natural_width,
        f64::from(height) / natural_height,
    );
    let mut remaining_pixels = MAX_PAINT_PIXELS;
    charge_render(tree.root(), scale, (width, height), &mut remaining_pixels)?;
    let mut viewport = tiny_skia::Pixmap::new(width, height).ok_or("Cannot allocate SVG pixels")?;
    let transform = tiny_skia::Transform::from_scale(scale.0 as f32, scale.1 as f32);
    resvg::render(&tree, transform, &mut viewport.as_mut());
    Ok(viewport)
}

fn fitted_size(
    natural_width: f64,
    natural_height: f64,
    width: u32,
    height: u32,
) -> Result<(u32, u32)> {
    if width == 0
        || height == 0
        || !(1.0..=MAX_INTRINSIC_SIDE).contains(&natural_width)
        || !(1.0..=MAX_INTRINSIC_SIDE).contains(&natural_height)
    {
        return Err("SVG viewport dimensions are outside the supported range".into());
    }
    let scale = 1.0_f64
        .min(f64::from(width.min(MAX_SVG_SIDE)) / natural_width)
        .min(f64::from(height.min(MAX_SVG_SIDE)) / natural_height)
        .min((MAX_SVG_PIXELS as f64 / (natural_width * natural_height)).sqrt());
    let width = (natural_width * scale).floor().max(1.0) as u32;
    let height = (natural_height * scale).floor().max(1.0) as u32;
    Ok((width, height))
}

fn validate(root: roxmltree::Node<'_, '_>, local_images: bool) -> Result<()> {
    if root.tag_name().name() != "svg" || root.tag_name().namespace() != Some(SVG_NAMESPACE) {
        return Err("The illustration needs a standalone SVG root".into());
    }
    let mut vector_bytes = 0;
    for node in root.descendants().filter(roxmltree::Node::is_element) {
        if node
            .ancestors()
            .take_while(|ancestor| *ancestor != root)
            .take(MAX_DEPTH as usize + 1)
            .count()
            > MAX_DEPTH as usize
        {
            return Err("The SVG exceeds the nesting limit".into());
        }
        let element = node.tag_name();
        if element.namespace() != Some(SVG_NAMESPACE)
            || (node != root
                && !(local_images && element.name() == "image")
                && !matches!(
                    element.name(),
                    "g" | "defs"
                        | "path"
                        | "rect"
                        | "circle"
                        | "ellipse"
                        | "polygon"
                        | "polyline"
                        | "line"
                        | "linearGradient"
                        | "radialGradient"
                        | "stop"
                        | "title"
                        | "desc"
                ))
        {
            return Err(format!(
                "SVG element {} is outside the static illustration subset",
                element.name()
            )
            .into());
        }
        for attribute in node.attributes() {
            let name = attribute.name();
            let value = attribute.value();
            if local_images
                && element.name() == "image"
                && name == "href"
                && matches!(
                    attribute.namespace(),
                    None | Some("http://www.w3.org/1999/xlink")
                )
            {
                continue;
            }
            if attribute.namespace().is_some() || !is_static_attribute(name) {
                return Err(format!(
                    "SVG attribute {name} is outside the static illustration subset"
                )
                .into());
            }
            if matches!(name, "fill" | "stroke") && !local_paint(value) {
                return Err("SVG paint must use an inline color or local gradient".into());
            }
            if matches!(name, "d" | "points") {
                vector_bytes += value.len();
                if vector_bytes > MAX_VECTOR_BYTES {
                    return Err("The SVG exceeds the path and point data limit".into());
                }
            }
        }
    }
    Ok(())
}

fn is_static_attribute(name: &str) -> bool {
    matches!(
        name,
        "id" | "version"
            | "viewBox"
            | "preserveAspectRatio"
            | "width"
            | "height"
            | "x"
            | "y"
            | "x1"
            | "x2"
            | "y1"
            | "y2"
            | "cx"
            | "cy"
            | "r"
            | "rx"
            | "ry"
            | "fx"
            | "fy"
            | "fr"
            | "d"
            | "points"
            | "transform"
            | "gradientTransform"
            | "gradientUnits"
            | "spreadMethod"
            | "offset"
            | "fill"
            | "fill-opacity"
            | "fill-rule"
            | "stroke"
            | "stroke-opacity"
            | "stroke-width"
            | "stroke-linecap"
            | "stroke-linejoin"
            | "stroke-miterlimit"
            | "stop-color"
            | "stop-opacity"
            | "color"
            | "display"
            | "visibility"
            | "shape-rendering"
    )
}

fn local_paint(value: &str) -> bool {
    let value = value.trim();
    if !value
        .get(..3)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("url"))
    {
        return true;
    }
    value
        .strip_prefix("url(#")
        .and_then(|name| name.strip_suffix(')'))
        .is_some_and(|name| {
            !name.is_empty()
                && name
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'.'))
        })
}

fn charge_render(
    group: &usvg::Group,
    scale: (f64, f64),
    viewport: (u32, u32),
    remaining_pixels: &mut usize,
) -> Result<()> {
    let unsupported = "SVG requires an unsupported image or compositing effect";
    if group.should_isolate() {
        return Err(unsupported.into());
    }
    for child in group.children() {
        match child {
            usvg::Node::Group(group) => charge_render(group, scale, viewport, remaining_pixels)?,
            usvg::Node::Path(path) => {
                if let Some(bounds) = path.abs_stroke_bounding_box() {
                    let width = paint_span(bounds.left(), bounds.right(), scale.0, viewport.0);
                    let height = paint_span(bounds.top(), bounds.bottom(), scale.1, viewport.1);
                    let passes =
                        usize::from(path.fill().is_some()) + usize::from(path.stroke().is_some());
                    *remaining_pixels = remaining_pixels
                        .checked_sub(width * height * passes)
                        .ok_or("The SVG exceeds the paint work limit")?;
                }
            }
            usvg::Node::Image(image) => {
                let bounds = image.abs_bounding_box();
                let width = paint_span(bounds.left(), bounds.right(), scale.0, viewport.0);
                let height = paint_span(bounds.top(), bounds.bottom(), scale.1, viewport.1);
                *remaining_pixels = remaining_pixels
                    .checked_sub(width * height)
                    .ok_or("The SVG exceeds its paint budget")?;
            }
            _ => return Err(unsupported.into()),
        }
    }
    Ok(())
}

fn paint_span(start: f32, end: f32, scale: f64, edge: u32) -> usize {
    let first = (f64::from(start) * scale).floor().max(0.0);
    let last = (f64::from(end) * scale).ceil().min(f64::from(edge));
    (last - first).max(0.0) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_non_square_vectors_and_composites_transparency_on_white() {
        let svg =
            br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 80 40" width="80" height="40">
            <rect width="40" height="40" fill="black" fill-opacity="0.5"/>
        </svg>"#;
        let image = rasterize(svg, 40, 100, MAX_SVG_PIXELS).unwrap();
        assert_eq!(image.dimensions(), (40, 20));
        assert!((127..=128).contains(&image.get_pixel(4, 4).0[0]));
        assert_eq!(image.get_pixel(35, 15).0[0], 255);
        assert!(rasterize(svg, 40, 100, 799).is_err());
        let size = fitted_size(16_000.0, 16_000.0, 4000, 4000).unwrap();
        assert!(size.0 as usize * size.1 as usize <= MAX_SVG_PIXELS);
    }

    #[test]
    fn rejects_external_resources_dynamic_elements_and_excessive_input() {
        for body in [
            r#"<image href="file:///tmp/picture.png"/>"#,
            r#"<image href="https://example.invalid/picture.png"/>"#,
            r#"<image href="data:image/png;base64,AA=="/>"#,
            r#"<rect fill="url(https://example.invalid/paint.svg#f)"/>"#,
            r#"<rect style="filter: blur(100px)"/>"#,
            r#"<script>void(0)</script>"#,
            r##"<use href="#item"/>"##,
        ] {
            let svg =
                format!(r#"<svg xmlns="{SVG_NAMESPACE}" width="10" height="10">{body}</svg>"#);
            assert!(
                rasterize(svg.as_bytes(), 10, 10, MAX_SVG_PIXELS).is_err(),
                "{body}"
            );
        }
        let excessive = vec![b' '; MAX_SVG_BYTES + 1];
        assert!(rasterize(&excessive, 10, 10, MAX_SVG_PIXELS).is_err());
        let dtd =
            format!(r#"<!DOCTYPE svg []><svg xmlns="{SVG_NAMESPACE}" width="10" height="10"/>"#);
        assert!(rasterize(dtd.as_bytes(), 10, 10, MAX_SVG_PIXELS).is_err());
        let nested = "<g>".repeat(MAX_DEPTH as usize + 1) + &"</g>".repeat(MAX_DEPTH as usize + 1);
        let svg = format!(r#"<svg xmlns="{SVG_NAMESPACE}" width="10" height="10">{nested}</svg>"#);
        assert!(rasterize(svg.as_bytes(), 10, 10, MAX_SVG_PIXELS).is_err());
        let nodes = "<g/>".repeat(MAX_XML_NODES as usize);
        let svg = format!(r#"<svg xmlns="{SVG_NAMESPACE}" width="10" height="10">{nodes}</svg>"#);
        assert!(rasterize(svg.as_bytes(), 10, 10, MAX_SVG_PIXELS).is_err());
    }

    #[test]
    fn rejects_heavy_overlap_before_rasterizing() {
        let shapes = r#"<rect width="2048" height="2048" fill="black"/>"#.repeat(40);
        let svg =
            format!(r#"<svg xmlns="{SVG_NAMESPACE}" width="2048" height="2048">{shapes}</svg>"#);
        assert!(
            rasterize(svg.as_bytes(), 2048, 2048, MAX_SVG_PIXELS)
                .unwrap_err()
                .to_string()
                .contains("paint work limit")
        );
    }
}
