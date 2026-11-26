//! Static rendering for uniformly fixed-layout EPUB XHTML publications.
use std::cell::{Ref, RefCell};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use parley::FontContext;
use roxmltree::{Document, ParsingOptions};

use crate::comic::ImagePage;
use crate::epub::{Book, FixedSpine, ItemPlacement, RenditionSpread, SpreadSide};
use crate::fixed::{MAX_VIEWPORT_PIXELS, MAX_VIEWPORT_SIDE, PdfOutlineItem, RenderRequest};
use crate::{Result, blitz, font_context, loose, svg_image};

const MAX_LAYOUT_DEPTH: usize = 48;

pub struct FixedEpub {
    path: PathBuf,
    revision: u64,
    title: String,
    author: Option<String>,
    spine: FixedSpine,
    content_size: (u32, u32),
    fonts: FontContext,
    outline: Vec<PdfOutlineItem>,
    printed_pages: Vec<(usize, String)>,
    cache: RefCell<Option<(usize, Arc<ImagePage>)>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FixedSpread {
    Single(usize),
    Facing {
        left: Option<usize>,
        right: Option<usize>,
    },
}

impl FixedSpread {
    /// Original spine indices in logical reading order.
    pub fn pages(self) -> [Option<usize>; 2] {
        match self {
            Self::Single(page) => [Some(page), None],
            Self::Facing {
                left: Some(left),
                right: Some(right),
            } => [Some(left.min(right)), Some(left.max(right))],
            Self::Facing { left, right } => [left.or(right), None],
        }
    }

    pub(crate) fn first_page(self) -> usize {
        self.pages()[0].expect("A planned spread contains at least one page")
    }

    fn facing(leading: Option<usize>, trailing: Option<usize>, rtl: bool) -> Self {
        if rtl {
            Self::Facing {
                left: trailing,
                right: leading,
            }
        } else {
            Self::Facing {
                left: leading,
                right: trailing,
            }
        }
    }
}

pub struct SpreadPlan {
    groups: Vec<FixedSpread>,
    page_to_group: Vec<usize>,
    has_facing: bool,
}

impl SpreadPlan {
    fn new(spine: &FixedSpine, user_mode: Option<bool>, rtl: bool, landscape: bool) -> Self {
        let mut plan = Self {
            groups: Vec::new(),
            page_to_group: vec![0; spine.placements.len()],
            has_facing: false,
        };
        let (publisher_leading, publisher_trailing) = spread_sides(spine.right_to_left);
        let (_, trailing) = spread_sides(rtl);
        let side = |placement| effective_side(placement, user_mode, landscape);
        let mut pages = spine.placements.iter().copied().enumerate().peekable();
        let mut pending = None;
        let flush = |plan: &mut Self, pending: &mut Option<usize>| {
            if let Some(page) = pending.take() {
                plan.push(FixedSpread::facing(Some(page), None, rtl));
            }
        };
        while let Some((page, placement)) = pages.next() {
            let slot = side(placement);
            if slot == publisher_leading
                && let Some(&(partner, next)) = pages.peek()
                && side(next) == publisher_trailing
            {
                // A publisher's explicit two-page artwork keeps its physical sides.
                flush(&mut plan, &mut pending);
                plan.push(FixedSpread::facing(
                    Some(page),
                    Some(partner),
                    spine.right_to_left,
                ));
                let _ = pages.next();
            } else if slot == SpreadSide::Center {
                flush(&mut plan, &mut pending);
                plan.push(FixedSpread::Single(page));
            } else if slot == trailing || slot == SpreadSide::Auto && pending.is_some() {
                plan.push(FixedSpread::facing(pending.take(), Some(page), rtl));
            } else {
                flush(&mut plan, &mut pending);
                pending = Some(page);
            }
        }
        flush(&mut plan, &mut pending);
        plan
    }

    fn push(&mut self, spread: FixedSpread) {
        let index = self.groups.len();
        for page in spread.pages().into_iter().flatten() {
            self.page_to_group[page] = index;
        }
        self.has_facing |= matches!(spread, FixedSpread::Facing { .. });
        self.groups.push(spread);
    }

    pub fn has_facing(&self) -> bool {
        self.has_facing
    }

    pub fn group(&self, page: usize) -> Option<FixedSpread> {
        self.page_to_group
            .get(page)
            .map(|&index| self.groups[index])
    }

    pub fn step(&self, page: usize, direction: isize) -> usize {
        if direction == 0 {
            return page;
        }
        self.page_to_group
            .get(page)
            .and_then(|index| index.checked_add_signed(direction))
            .and_then(|index| self.groups.get(index))
            .map_or(page, |group| group.first_page())
    }
}

fn spread_sides(rtl: bool) -> (SpreadSide, SpreadSide) {
    if rtl {
        (SpreadSide::Right, SpreadSide::Left)
    } else {
        (SpreadSide::Left, SpreadSide::Right)
    }
}

fn effective_side(
    placement: ItemPlacement,
    user_mode: Option<bool>,
    landscape: bool,
) -> SpreadSide {
    let paired = match placement.spread {
        RenditionSpread::None => false,
        RenditionSpread::Auto => user_mode.unwrap_or(false),
        RenditionSpread::Landscape => landscape && user_mode.unwrap_or(true),
        RenditionSpread::Both => user_mode.unwrap_or(true),
    };
    if paired {
        placement.side
    } else {
        SpreadSide::Center
    }
}

impl FixedEpub {
    /// Return None for a reflowable book. No archive handle is kept between pages.
    pub fn open(path: &Path, fonts: &[PathBuf]) -> Result<Option<Self>> {
        let settings = crate::Settings::default();
        Self::open_at(path, fonts, (settings.width, settings.height))
    }

    pub(crate) fn open_at(
        path: &Path,
        fonts: &[PathBuf],
        content_size: (u32, u32),
    ) -> Result<Option<Self>> {
        let mut book = Book::open(path)?;
        let Some(spine) = book.take_fixed_spine() else {
            return Ok(None);
        };
        let outline = book
            .toc()
            .iter()
            .map(|item| PdfOutlineItem {
                title: item.label.clone(),
                destination_point: None,
                page: item
                    .destination
                    .as_ref()
                    .and_then(|destination| book.chapter_index(&destination.path)),
                depth: item.depth,
            })
            .collect();
        let paths: BTreeMap<_, _> = book
            .chapters
            .iter()
            .enumerate()
            .map(|(index, chapter)| (chapter.href.as_str(), index))
            .collect();
        let printed_pages = book
            .printed_pages()
            .iter()
            .filter_map(|entry| {
                let destination = entry.destination.as_ref()?;
                Some((*paths.get(destination.path.as_str())?, entry.label.clone()))
            })
            .collect();
        Ok(Some(Self {
            path: path.to_owned(),
            revision: book.revision(),
            spine,
            content_size,
            title: book.title,
            author: book.author,
            fonts: font_context(fonts)?,
            outline,
            printed_pages,
            cache: RefCell::new(None),
        }))
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn author(&self) -> Option<&str> {
        self.author.as_deref()
    }

    pub fn page_count(&self) -> usize {
        self.spine.placements.len()
    }

    pub fn right_to_left(&self) -> bool {
        self.spine.right_to_left
    }

    pub fn spread_plan(&self, user_mode: Option<bool>, rtl: bool, landscape: bool) -> SpreadPlan {
        SpreadPlan::new(&self.spine, user_mode, rtl, landscape)
    }

    pub fn compose_spread(
        &self,
        spread: FixedSpread,
        width: u32,
        height: u32,
        turns: u8,
    ) -> Result<Vec<u8>> {
        if !(1..=MAX_VIEWPORT_SIDE).contains(&width)
            || !(1..=MAX_VIEWPORT_SIDE).contains(&height)
            || width * height > MAX_VIEWPORT_PIXELS
        {
            return Err("The EPUB spread exceeds Reader's viewport limit".into());
        }
        let FixedSpread::Facing { left, right } = spread else {
            return self
                .decode_page(spread.first_page())?
                .fit(width, height, turns);
        };
        if width < 2 || left.is_none() && right.is_none() {
            return Err("The EPUB spread has no usable viewport slots".into());
        }
        let seam = width / 2;
        let mut frame = vec![255; (width * height) as usize];
        for (page, x, slot_width) in [(left, 0, seam), (right, seam, width - seam)] {
            let Some(page) = page else { continue };
            let image = self.decode_page(page)?;
            let (mut natural_width, mut natural_height) = image.page_size();
            if !turns.is_multiple_of(2) {
                std::mem::swap(&mut natural_width, &mut natural_height);
            }
            let scale = (slot_width as f32 / natural_width).min(height as f32 / natural_height);
            let half = image.render(RenderRequest {
                width: slot_width,
                height,
                scale,
                x: if x == 0 {
                    (slot_width as f32 - natural_width * scale).max(0.0)
                } else {
                    0.0
                },
                y: (height as f32 - natural_height * scale).max(0.0) / 2.0,
                turns,
            })?;
            for (row, piece) in frame
                .chunks_exact_mut(width as usize)
                .zip(half.chunks_exact(slot_width as usize))
            {
                row[x as usize..(x + slot_width) as usize].copy_from_slice(piece);
            }
        }
        Ok(frame)
    }

    pub fn outline(&self) -> Vec<PdfOutlineItem> {
        self.outline.clone()
    }

    pub fn page_label(&self, page: usize) -> Option<&str> {
        self.printed_pages
            .iter()
            .find_map(|(index, label)| (*index == page).then_some(label.as_str()))
    }

    pub fn find_page_label(&self, label: &str) -> Option<usize> {
        let mut matches = self
            .printed_pages
            .iter()
            .filter_map(|(page, value)| (value == label).then_some(*page));
        let page = matches.next()?;
        matches.all(|other| other == page).then_some(page)
    }

    /// Keep the displayed page's decoded pixels while composing a candidate frame.
    pub(crate) fn with_preserved_page_cache<T>(
        &self,
        compose: impl FnOnce() -> Result<T>,
    ) -> Result<T> {
        let saved = self
            .cache
            .try_borrow_mut()
            .map_err(|_| "The current fixed-layout EPUB page is still in use")?
            .clone();
        let frame = compose();
        self.cache.replace(saved);
        frame
    }

    /// Load just the requested page and cache its source-size pixels for zoom and pan.
    pub fn decode_page(&self, page: usize) -> Result<Ref<'_, ImagePage>> {
        if page >= self.page_count() {
            return Err("That page is outside this fixed-layout EPUB".into());
        }
        let cached = self
            .cache
            .borrow()
            .as_ref()
            .is_some_and(|(index, _)| *index == page);
        if !cached {
            let mut cache = self
                .cache
                .try_borrow_mut()
                .map_err(|_| "The current fixed-layout EPUB page is still in use")?;
            *cache = None;
            let mut book = Book::open(&self.path)?;
            if !book.is_fixed_layout() || book.revision() != self.revision {
                return Err("The fixed-layout EPUB changed after it was opened".into());
            }
            let href = book.chapters[page].href.clone();
            let image = if book.chapters[page].is_svg {
                let bytes = book.resource_limited(&href, svg_image::MAX_SVG_BYTES)?;
                let picture = svg_image::rasterize(
                    &bytes,
                    svg_image::MAX_SVG_SIDE,
                    svg_image::MAX_SVG_SIDE,
                    svg_image::MAX_SVG_PIXELS,
                )?;
                ImagePage::from_grayscale(picture.width(), picture.height(), picture.into_raw())?
            } else {
                let xhtml = book.chapter(page)?;
                let xhtml = loose::without_plain_html_doctype(&xhtml);
                let (xhtml, inline_svg) = prepare_inline_svg(&mut book, &href, &xhtml)?;
                let (width, height) = viewport_size(&xhtml, self.content_size)?;
                let gray = blitz::fixed_page(
                    book,
                    &href,
                    &xhtml,
                    self.fonts.clone(),
                    (width, height),
                    inline_svg,
                )?;
                ImagePage::from_grayscale(width, height, gray)?
            };
            *cache = Some((page, Arc::new(image)));
        }
        Ok(Ref::map(self.cache.borrow(), |cache| {
            cache.as_ref().unwrap().1.as_ref()
        }))
    }
}

/// Rasterize supported inline vectors into temporary, page-local images. This
/// transient layout copy is never stored as the chapter or reading identity.
fn prepare_inline_svg(
    book: &mut Book,
    href: &str,
    xhtml: &str,
) -> Result<(String, BTreeMap<String, Vec<u8>>)> {
    const SVG_NS: &str = "http://www.w3.org/2000/svg";
    const MAX_GRAPHICS: usize = 32;
    const MAX_TEMPORARY_BYTES: usize = 8 * 1024 * 1024;
    let document = Document::parse_with_options(
        xhtml,
        ParsingOptions {
            nodes_limit: 80_000,
            ..ParsingOptions::default()
        },
    )?;
    let mut output = String::with_capacity(xhtml.len());
    let mut images = BTreeMap::new();
    let mut cursor = 0;
    let mut pixels = 0usize;
    let mut bytes = 0usize;
    for node in document
        .descendants()
        .filter(|node| node.has_tag_name((SVG_NS, "svg")))
    {
        if node
            .ancestors()
            .skip(1)
            .any(|parent| parent.has_tag_name((SVG_NS, "svg")))
        {
            continue;
        }
        if images.len() >= MAX_GRAPHICS {
            return Err("The fixed-layout page has too many inline SVG graphics".into());
        }
        let svg = svg_image::inline_source(node, xhtml)
            .ok_or("The fixed-layout page contains an unsupported inline SVG graphic")?;
        let (png, used) = svg_image::inline_png_resource(
            book,
            href,
            &svg,
            svg_image::MAX_SVG_PIXELS.saturating_sub(pixels),
        )?;
        pixels = pixels
            .checked_add(used)
            .ok_or("The inline SVG pixel limit was exceeded")?;
        bytes = bytes
            .checked_add(png.len())
            .filter(|&n| n <= MAX_TEMPORARY_BYTES)
            .ok_or("The inline SVG resource limit was exceeded")?;
        let path = format!("{}.png", images.len());
        output.push_str(&xhtml[cursor..node.range().start]);
        output.push_str("<img src=\"https://");
        output.push_str(blitz::INLINE_SVG_HOST);
        output.push('/');
        output.push_str(&path);
        output.push_str("\" alt=\"\"");
        for name in ["id", "class", "style", "width", "height"] {
            if let Some(value) = node.attribute(name) {
                output.push(' ');
                output.push_str(name);
                output.push_str("=\"");
                for character in value.chars() {
                    match character {
                        '&' => output.push_str("&amp;"),
                        '<' => output.push_str("&lt;"),
                        '"' => output.push_str("&quot;"),
                        _ => output.push(character),
                    }
                }
                output.push('"');
            }
        }
        output.push_str("/>");
        images.insert(path, png);
        cursor = node.range().end;
    }
    output.push_str(&xhtml[cursor..]);
    Ok((output, images))
}

// EPUB 3.3 §8.2.2.6: https://www.w3.org/TR/epub-33/#sec-fxl-dimensions
fn viewport_size(xhtml: &str, content_size: (u32, u32)) -> Result<(u32, u32)> {
    let doc = Document::parse_with_options(
        xhtml,
        ParsingOptions {
            nodes_limit: 80_000,
            ..ParsingOptions::default()
        },
    )?;
    let root = doc.root_element();
    if !root.has_tag_name(("http://www.w3.org/1999/xhtml", "html")) {
        return Err("The fixed-layout EPUB page does not contain XHTML".into());
    }
    if root
        .descendants()
        .any(|node| node.ancestors().take(MAX_LAYOUT_DEPTH + 1).count() > MAX_LAYOUT_DEPTH)
    {
        return Err("The fixed-layout EPUB page is nested too deeply".into());
    }
    if root.descendants().any(|node| {
        node.is_element()
            && matches!(
                node.tag_name().name(),
                "script"
                    | "svg"
                    | "iframe"
                    | "frame"
                    | "object"
                    | "embed"
                    | "canvas"
                    | "audio"
                    | "video"
            )
    }) {
        return Err(
            "The fixed-layout EPUB page requires unsupported scripted, SVG, or embedded content"
                .into(),
        );
    }
    let meta = root
        .children()
        .find(|node| node.has_tag_name("head"))
        .and_then(|head| {
            head.children().find(|node| {
                node.has_tag_name("meta")
                    && node
                        .attribute("name")
                        .is_some_and(|value| value.trim().eq_ignore_ascii_case("viewport"))
            })
        })
        .and_then(|node| node.attribute("content"))
        .ok_or("The fixed-layout EPUB page has no viewport dimensions")?;
    let mut width = None;
    let mut height = None;
    let separator = |c: char| c == ',' || c == ';' || c.is_ascii_whitespace();
    let mut rest = meta.trim_start_matches(separator);
    while !rest.is_empty() {
        let end = rest
            .find(|c| c == '=' || separator(c))
            .unwrap_or(rest.len());
        let key = &rest[..end];
        let tail = rest[end..].trim_start_matches(char::is_whitespace);
        let (value, tail) = if let Some(tail) = tail.strip_prefix('=') {
            let tail = tail.trim_start_matches(char::is_whitespace);
            let end = tail.find(separator).unwrap_or(tail.len());
            (Some(&tail[..end]), &tail[end..])
        } else {
            (None, tail)
        };
        rest = tail.trim_start_matches(separator);
        let (dimension, keyword, device_side) = if key.eq_ignore_ascii_case("width") {
            (&mut width, "device-width", content_size.0)
        } else if key.eq_ignore_ascii_case("height") {
            (&mut height, "device-height", content_size.1)
        } else {
            continue;
        };
        if dimension.is_some() {
            continue;
        }
        let value = value.ok_or("The fixed-layout EPUB viewport is missing a dimension value")?;
        let side = if value.eq_ignore_ascii_case(keyword) {
            device_side as f32
        } else {
            value
                .trim_end_matches(|ch: char| ch.is_ascii_alphabetic() || ch == '%')
                .parse::<f32>()
                .map_err(|_| "The fixed-layout EPUB viewport has an invalid dimension")?
        };
        if !(1.0..=MAX_VIEWPORT_SIDE as f32).contains(&side) {
            return Err("The fixed-layout EPUB viewport exceeds Reader's size limits".into());
        }
        *dimension = Some(side.ceil() as u32);
    }
    let width = width.ok_or("The fixed-layout EPUB viewport has no width")?;
    let height = height.ok_or("The fixed-layout EPUB viewport has no height")?;
    if width * height > MAX_VIEWPORT_PIXELS {
        return Err("The fixed-layout EPUB viewport exceeds Reader's raster budget".into());
    }
    Ok((width, height))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixed::RenderRequest;
    use std::fs::{self, File};
    use std::io::{Read, Write};
    use std::time::{SystemTime, UNIX_EPOCH};
    use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

    struct TestBook(PathBuf);

    impl Drop for TestBook {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }

    fn write_book(layout: &str, first_override: &str, second_override: &str) -> TestBook {
        write_book_rendition(layout, "auto", "default", first_override, second_override)
    }

    #[test]
    fn opens_static_svg_as_a_fixed_layout_spine_page() {
        let original = write_book("pre-paginated", "", "");
        let target = TestBook(original.0.with_extension("svg-spine.epub"));
        let mut source = zip::ZipArchive::new(File::open(&original.0).unwrap()).unwrap();
        let mut output = ZipWriter::new(File::create(&target.0).unwrap());
        for index in 0..source.len() {
            let mut file = source.by_index(index).unwrap();
            let mut name = file.name().to_owned();
            let mut contents = String::new();
            file.read_to_string(&mut contents).unwrap();
            if name == "EPUB/book.opf" {
                contents = contents.replace(
                    "href=\"pages/first.xhtml\" media-type=\"application/xhtml+xml\"",
                    "href=\"pages/first.svg\" media-type=\"image/svg+xml\"",
                );
            } else if name == "EPUB/nav.xhtml" {
                contents = contents.replace("pages/first.xhtml", "pages/first.svg");
            } else if name == "EPUB/pages/first.xhtml" {
                name = "EPUB/pages/first.svg".into();
                contents = r#"<svg xmlns="http://www.w3.org/2000/svg" width="90" height="140"><rect width="45" height="140" fill="black"/></svg>"#.into();
            }
            output
                .start_file(
                    name,
                    SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
                )
                .unwrap();
            output.write_all(contents.as_bytes()).unwrap();
        }
        output.finish().unwrap();
        let book = FixedEpub::open(
            &target.0,
            &[crate::test_fixture("fonts/Silkscreen-Regular.ttf")],
        )
        .unwrap()
        .unwrap();
        assert_eq!(book.page_count(), 2);
        assert_eq!(book.outline()[0].page, Some(0));
        let svg = book.decode_page(0).unwrap();
        assert_eq!(svg.page_size(), (90.0, 140.0));
        let image = render(&svg, 90, 140);
        assert_eq!(image[90 * 30 + 5], 0);
        assert_eq!(image[90 * 30 + 80], 255);
    }

    fn write_book_rendition(
        layout: &str,
        spread: &str,
        progression: &str,
        first_override: &str,
        second_override: &str,
    ) -> TestBook {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "primitiveos-fixed-epub-{}-{unique}.epub",
            std::process::id()
        ));
        let file = File::options()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        let book = TestBook(path);
        let mut zip = ZipWriter::new(file);
        let package = format!(
            r#"<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
                <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
                    <dc:title>Positioned Pages</dc:title>
                    <meta property="rendition:layout">{layout}</meta>
                    <meta property="rendition:spread">{spread}</meta>
                </metadata>
                <manifest>
                    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
                    <item id="first" href="pages/first.xhtml" media-type="application/xhtml+xml"/>
                    <item id="second" href="pages/second.xhtml" media-type="application/xhtml+xml"/>
                    <item id="css" href="style.css" media-type="text/css"/>
                    <item id="shape" href="shape.svg" media-type="image/svg+xml"/>
                </manifest>
                <spine page-progression-direction="{progression}"><itemref idref="first" properties="{first_override}"/><itemref idref="second" properties="{second_override}"/></spine>
            </package>"#,
        );
        let container = r#"<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container" version="1.0">
                <rootfiles><rootfile full-path="EPUB/book.opf" media-type="application/oebps-package+xml"/></rootfiles>
            </container>"#;
        let nav = r##"<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><body>
            <nav epub:type="toc"><ol><li><a href="pages/first.xhtml">Opening</a><ol>
                <li><a href="pages/second.xhtml#target">Second</a></li>
            </ol></li></ol></nav>
            <nav epub:type="page-list"><ol><li><a href="pages/first.xhtml">iv</a></li>
                <li><a href="pages/second.xhtml#target">12</a></li></ol></nav>
        </body></html>"##;
        let first = r#"<html xmlns="http://www.w3.org/1999/xhtml">
            <head><meta name="viewport" content="width=224, height=320"/><link rel="stylesheet" href="../style.css"/></head>
            <body><div class="mark"></div>
            <img src="../shape.svg" style="position:absolute;left:90px;top:100px;width:24px;height:24px"/>
            <div style="position:absolute;left:208px;top:0;width:16px;height:320px;background:rgb(32,32,32)"></div></body>
        </html>"#;
        let second = r#"<html xmlns="http://www.w3.org/1999/xhtml">
            <head><meta name="viewport" content="height = 200 width = 290; initial-scale = 2"/></head>
            <body><div id="target" style="position:absolute; left:130px; top:20px; width:55px; height:40px; background:black"></div>
            <div style="position:absolute;left:0;top:0;width:16px;height:200px;background:rgb(96,96,96)"></div></body>
        </html>"#;
        let style = ".mark { position:absolute; left:21px; top:41px; width:38px; height:28px; background:#000; }";
        for (path, data) in [
            ("mimetype", "application/epub+zip"),
            ("META-INF/container.xml", container),
            ("EPUB/book.opf", &package),
            ("EPUB/nav.xhtml", nav),
            ("EPUB/pages/first.xhtml", first),
            ("EPUB/pages/second.xhtml", second),
            ("EPUB/style.css", style),
            (
                "EPUB/shape.svg",
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24"><rect width="24" height="24" fill="black"/></svg>"#,
            ),
        ] {
            zip.start_file(
                path,
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .unwrap();
            zip.write_all(data.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
        book
    }

    fn render(page: &ImagePage, width: u32, height: u32) -> Vec<u8> {
        page.render(RenderRequest {
            width,
            height,
            scale: 1.0,
            x: 0.0,
            y: 0.0,
            turns: 0,
        })
        .unwrap()
    }

    #[test]
    fn publisher_pair_joins_its_inner_edges_even_at_mixed_intrinsic_sizes() {
        let fixture = write_book_rendition(
            "pre-paginated",
            "both",
            "ltr",
            "page-spread-left rendition:page-spread-left",
            "rendition:page-spread-right",
        );
        let fonts = [crate::test_fixture("fonts/Silkscreen-Regular.ttf")];
        let book = FixedEpub::open(&fixture.0, &fonts).unwrap().unwrap();
        assert!(!book.right_to_left());
        let plan = book.spread_plan(None, true, false);
        assert!(plan.has_facing());
        let group = plan.group(1).unwrap();
        assert_eq!(
            group,
            FixedSpread::Facing {
                left: Some(0),
                right: Some(1)
            }
        );
        assert_eq!(group.pages(), [Some(0), Some(1)]);
        assert_eq!(plan.step(1, 1), 1);
        assert_eq!(plan.step(1, 0), 1);
        let frame = book.compose_spread(group, 400, 100, 0).unwrap();
        assert_eq!(frame.len(), 40_000);
        assert!(
            frame[50 * 400 + 199] < 50,
            "The left content should end at the seam"
        );
        assert!(
            (64..128).contains(&frame[50 * 400 + 200]),
            "The right content should start at the seam"
        );
        assert_eq!(frame[50 * 400], 255);
        let single = book.spread_plan(Some(false), true, false);
        assert!(!single.has_facing());
        assert_eq!(single.group(1), Some(FixedSpread::Single(1)));
        assert!(book.compose_spread(group, 2049, 200, 0).is_err());
    }

    #[test]
    fn respects_landscape_global_and_item_spread_overrides() {
        let fixture = write_book_rendition(
            "pre-paginated",
            "landscape",
            "rtl",
            "rendition:spread-both rendition:page-spread-left",
            "rendition:page-spread-center",
        );
        let fonts = [crate::test_fixture("fonts/Silkscreen-Regular.ttf")];
        let book = FixedEpub::open(&fixture.0, &fonts).unwrap().unwrap();
        assert!(book.right_to_left());
        let plan = book.spread_plan(None, book.right_to_left(), false);
        let first = FixedSpread::Facing {
            left: Some(0),
            right: None,
        };
        assert_eq!(plan.group(0), Some(first));
        assert_eq!(plan.group(1), Some(FixedSpread::Single(1)));
        assert_eq!(plan.step(0, 1), 1);
        let frame = book.compose_spread(first, 224, 120, 0).unwrap();
        assert!(
            frame
                .as_chunks::<224>()
                .0
                .iter()
                .all(|row| row[112..].iter().all(|&tone| tone == 255))
        );
        assert!(frame.iter().any(|&tone| tone != 255));

        let landscape_fixture = write_book_rendition("pre-paginated", "landscape", "rtl", "", "");
        let landscape = FixedEpub::open(&landscape_fixture.0, &fonts)
            .unwrap()
            .unwrap();
        assert!(!landscape.spread_plan(None, true, false).has_facing());
        assert_eq!(
            landscape.spread_plan(None, true, true).group(0),
            Some(FixedSpread::Facing {
                left: Some(1),
                right: Some(0)
            })
        );
    }

    #[test]
    fn keeps_original_spine_order_through_blanks_center_and_automatic_pages() {
        let spine = FixedSpine {
            right_to_left: true,
            placements: [
                SpreadSide::Left,
                SpreadSide::Auto,
                SpreadSide::Left,
                SpreadSide::Center,
                SpreadSide::Auto,
                SpreadSide::Auto,
                SpreadSide::Right,
            ]
            .into_iter()
            .map(|side| ItemPlacement {
                side,
                spread: RenditionSpread::Both,
            })
            .collect(),
        };
        let plan = SpreadPlan::new(&spine, None, true, false);
        let expected = [
            FixedSpread::Facing {
                left: Some(0),
                right: None,
            },
            FixedSpread::Facing {
                left: Some(2),
                right: Some(1),
            },
            FixedSpread::Single(3),
            FixedSpread::Facing {
                left: Some(5),
                right: Some(4),
            },
            FixedSpread::Facing {
                left: None,
                right: Some(6),
            },
        ];
        assert_eq!(plan.groups, expected);
        assert_eq!(plan.group(2), Some(expected[1]));
        assert_eq!(plan.step(2, 1), 3);
        assert_eq!(plan.step(5, -1), 3);
        assert_eq!(plan.step(6, 1), 6);
        assert_eq!(plan.group(7), None);
    }

    #[test]
    fn renders_publisher_geometry_locally_and_caches_one_page() {
        let fixture = write_book("pre-paginated", "", "");
        let fonts = [crate::test_fixture("fonts/Silkscreen-Regular.ttf")];
        let book = FixedEpub::open(&fixture.0, &fonts).unwrap().unwrap();
        assert_eq!(book.title(), "Positioned Pages");
        assert_eq!(book.page_count(), 2);
        assert_eq!(book.page_label(0), Some("iv"));
        assert_eq!(book.find_page_label("12"), Some(1));
        let outline = book.outline();
        assert_eq!(
            (&outline[0].title[..], outline[0].depth, outline[0].page),
            ("Opening", 0, Some(0))
        );
        assert_eq!(
            (&outline[1].title[..], outline[1].depth, outline[1].page),
            ("Second", 1, Some(1))
        );
        let first = book.decode_page(0).unwrap();
        assert_eq!(first.page_size(), (224.0, 320.0));
        let pixels = render(&first, 224, 320);
        assert!(
            pixels[45 * 224 + 25] < 20,
            "local archive CSS did not draw its authored rectangle"
        );
        assert_eq!(pixels[45 * 224 + 18], 255);
        assert!(
            pixels[104 * 224 + 95] < 32,
            "local SVG should draw at its authored position"
        );
        assert!(book.decode_page(1).is_err());
        drop(first);
        let second = book.decode_page(1).unwrap();
        assert_eq!(second.page_size(), (290.0, 200.0));
        let pixels = render(&second, 290, 200);
        assert!(pixels[30 * 290 + 150] < 20);
        assert_eq!(pixels[30 * 290 + 50], 255);
        assert!(book.decode_page(2).is_err());
        drop(second);
        assert_eq!(book.decode_page(0).unwrap().page_size(), (224.0, 320.0));
    }

    #[test]
    fn handles_uniform_spine_overrides_and_rejects_mixed_layout() {
        let fonts = [crate::test_fixture("fonts/Silkscreen-Regular.ttf")];
        let pure = write_book(
            "reflowable",
            "rendition:layout-pre-paginated",
            "rendition:layout-pre-paginated",
        );
        assert!(FixedEpub::open(&pure.0, &fonts).unwrap().is_some());
        let mixed = write_book("pre-paginated", "", "rendition:layout-reflowable");
        let error = FixedEpub::open(&mixed.0, &fonts).err().unwrap().to_string();
        assert!(error.contains("mixes fixed-layout and reflowable"));
        assert!(
            FixedEpub::open(&crate::test_fixture("field-notes-epub3.epub"), &fonts)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn applies_the_first_viewport_dimensions_and_the_available_device_size() {
        for (declaration, size) in [
            ("width=device-width,height=device-height", (1264, 1606)),
            ("width=100px,height=180mm,width=500,height=640", (100, 180)),
            ("width=100.5,height=159.1,initial-scale=3", (101, 160)),
        ] {
            let page = format!(
                r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><meta name="viewport" content="{declaration}"/><meta name="viewport" content="width=5,height=5"/></head></html>"#
            );
            assert_eq!(viewport_size(&page, (1264, 1606)).unwrap(), size);
        }
    }

    #[test]
    fn rejects_invalid_or_oversized_viewport_dimensions() {
        for declaration in [
            "width=100",
            "width=device-height,height=100",
            "width=100,height=0",
            "width=4096,height=4096",
            "width=0,height=100;width=200",
        ] {
            let page = format!(
                r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><meta name="viewport" content="{declaration}"/></head></html>"#
            );
            assert!(viewport_size(&page, (1264, 1606)).is_err(), "{declaration}");
        }
        let page = r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><meta name="viewport" content="width=100,height=100"/><script/></head></html>"#;
        assert!(viewport_size(page, (1264, 1606)).is_err());
    }
}
