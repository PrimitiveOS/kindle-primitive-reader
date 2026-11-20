//! Static SVG illustrations from EPUB archive bytes with no external resource resolution.

use image::GrayImage;
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
    let viewport = render_pixmap(bytes, max_width, max_height, available_pixels)?;
    GrayImage::from_raw(
        viewport.width(),
        viewport.height(),
        crate::grayscale(viewport.data()).collect(),
    )
    .ok_or_else(|| "SVG pixels have unexpected dimensions".into())
}

pub(crate) fn png_resource(bytes: &[u8], available_pixels: usize) -> Result<(Vec<u8>, usize)> {
    let viewport = render_pixmap(bytes, MAX_SVG_SIDE, MAX_SVG_SIDE, available_pixels)?;
    let pixels = viewport.width() as usize * viewport.height() as usize;
    Ok((viewport.encode_png()?, pixels))
}

fn render_pixmap(
    bytes: &[u8],
    max_width: u32,
    max_height: u32,
    available_pixels: usize,
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
    validate(&document)?;
    let options = usvg::Options {
        max_depth: MAX_DEPTH,
        image_href_resolver: usvg::ImageHrefResolver {
            resolve_data: Box::new(|_, _, _| None),
            resolve_string: Box::new(|_, _| None),
        },
        ..Default::default()
    };
    let tree = usvg::Tree::from_xmltree(&document, &options)?;
    let natural_width = f64::from(tree.size().width());
    let natural_height = f64::from(tree.size().height());
    let (width, height) = fitted_size(natural_width, natural_height, max_width, max_height)?;
    if width as usize * height as usize > available_pixels {
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

fn validate(document: &Document<'_>) -> Result<()> {
    let root = document.root_element();
    if root.tag_name().name() != "svg" || root.tag_name().namespace() != Some(SVG_NAMESPACE) {
        return Err("The illustration needs a standalone SVG root".into());
    }
    let mut vector_bytes = 0;
    for node in root.descendants().filter(roxmltree::Node::is_element) {
        if node.ancestors().take(MAX_DEPTH as usize + 2).count() > MAX_DEPTH as usize + 1 {
            return Err("The SVG exceeds the nesting limit".into());
        }
        let element = node.tag_name();
        if element.namespace() != Some(SVG_NAMESPACE)
            || (node != root
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
