//! Static, single-page rendering for uniformly fixed-layout EPUB XHTML publications.
use std::cell::{Ref, RefCell};
use std::path::{Path, PathBuf};

use parley::FontContext;
use roxmltree::{Document, ParsingOptions};

use crate::comic::ImagePage;
use crate::epub::Book;
use crate::fixed::{MAX_VIEWPORT_PIXELS, MAX_VIEWPORT_SIDE, PdfOutlineItem};
use crate::{Result, blitz, font_context, loose};

const MAX_LAYOUT_DEPTH: usize = 48;

pub struct FixedEpub {
    path: PathBuf,
    revision: u64,
    title: String,
    count: usize,
    fonts: FontContext,
    outline: Vec<PdfOutlineItem>,
    cache: RefCell<Option<(usize, ImagePage)>>,
}

impl FixedEpub {
    /// Return None for a reflowable book. No archive handle is kept between pages.
    pub fn open(path: &Path, fonts: &[PathBuf]) -> Result<Option<Self>> {
        let book = Book::open(path)?;
        if !book.is_fixed_layout() {
            return Ok(None);
        }
        let outline = book
            .toc()
            .iter()
            .map(|item| PdfOutlineItem {
                title: item.label.clone(),
                page: item
                    .destination
                    .as_ref()
                    .and_then(|destination| book.chapter_index(&destination.path)),
                depth: item.depth,
            })
            .collect();
        Ok(Some(Self {
            path: path.to_owned(),
            revision: book.revision(),
            count: book.chapters.len(),
            title: book.title,
            fonts: font_context(fonts)?,
            outline,
            cache: RefCell::new(None),
        }))
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn page_count(&self) -> usize {
        self.count
    }

    pub fn outline(&self) -> Vec<PdfOutlineItem> {
        self.outline.clone()
    }

    /// Load just the requested page and cache its source-size pixels for zoom and pan.
    pub fn decode_page(&self, page: usize) -> Result<Ref<'_, ImagePage>> {
        if page >= self.count {
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
            let mut book = Book::open(&self.path)?;
            if !book.is_fixed_layout() || book.revision() != self.revision {
                return Err("The fixed-layout EPUB changed after it was opened".into());
            }
            let href = book.chapters[page].href.clone();
            let xhtml = book.chapter(page)?;
            let xhtml = loose::without_plain_html_doctype(&xhtml);
            let (width, height) = viewport_size(&xhtml)?;
            let gray = blitz::fixed_page(book, &href, &xhtml, self.fonts.clone(), (width, height))?;
            *cache = Some((page, ImagePage::from_grayscale(width, height, gray)?));
        }
        Ok(Ref::map(self.cache.borrow(), |cache| {
            &cache.as_ref().unwrap().1
        }))
    }
}

// EPUB 3.3 §8.2.2.6: https://www.w3.org/TR/epub-33/#sec-fxl-dimensions
fn viewport_size(xhtml: &str) -> Result<(u32, u32)> {
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
        let dimension = if key.eq_ignore_ascii_case("width") {
            &mut width
        } else if key.eq_ignore_ascii_case("height") {
            &mut height
        } else {
            continue;
        };
        let number = value
            .and_then(|value| value.parse::<f32>().ok())
            .filter(|value| (1.0..=MAX_VIEWPORT_SIDE as f32).contains(value))
            .ok_or("The fixed-layout EPUB viewport needs bounded numeric width and height")?;
        if dimension.replace(number.ceil() as u32).is_some() {
            return Err("The fixed-layout EPUB viewport repeats a dimension".into());
        }
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
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};
    use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

    struct TestBook(PathBuf);

    impl Drop for TestBook {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }

    fn write_book(layout: &str, first_override: &str, second_override: &str) -> TestBook {
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
                </metadata>
                <manifest>
                    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
                    <item id="first" href="pages/first.xhtml" media-type="application/xhtml+xml"/>
                    <item id="second" href="pages/second.xhtml" media-type="application/xhtml+xml"/>
                    <item id="css" href="style.css" media-type="text/css"/>
                </manifest>
                <spine><itemref idref="first" properties="{first_override}"/><itemref idref="second" properties="{second_override}"/></spine>
            </package>"#,
        );
        let container = r#"<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container" version="1.0">
                <rootfiles><rootfile full-path="EPUB/book.opf" media-type="application/oebps-package+xml"/></rootfiles>
            </container>"#;
        let nav = r##"<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><body>
            <nav epub:type="toc"><ol><li><a href="pages/first.xhtml">Opening</a><ol>
                <li><a href="pages/second.xhtml#target">Second</a></li>
            </ol></li></ol></nav>
        </body></html>"##;
        let first = r#"<html xmlns="http://www.w3.org/1999/xhtml">
            <head><meta name="viewport" content="width=224, height=320"/><link rel="stylesheet" href="../style.css"/></head>
            <body><div class="mark"></div></body>
        </html>"#;
        let second = r#"<html xmlns="http://www.w3.org/1999/xhtml">
            <head><meta name="viewport" content="height = 200 width = 290; initial-scale = 2"/></head>
            <body><div id="target" style="position:absolute; left:130px; top:20px; width:55px; height:40px; background:black"></div></body>
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
    fn renders_publisher_geometry_locally_and_caches_one_page() {
        let fixture = write_book("pre-paginated", "", "");
        let fonts = [crate::test_fixture("fonts/Silkscreen-Regular.ttf")];
        let book = FixedEpub::open(&fixture.0, &fonts).unwrap().unwrap();
        assert_eq!(book.title(), "Positioned Pages");
        assert_eq!(book.page_count(), 2);
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
    fn rejects_unsupported_or_ambiguous_viewport_values() {
        for declaration in [
            "width=100",
            "width=device-width,height=100",
            "width=100,height=0",
            "width=4096,height=4096",
            "width=100,height=100;width=200",
        ] {
            let page = format!(
                r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><meta name="viewport" content="{declaration}"/></head></html>"#
            );
            assert!(viewport_size(&page).is_err(), "{declaration}");
        }
        let page = r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><meta name="viewport" content="width=100,height=100"/><script/></head></html>"#;
        assert!(viewport_size(page).is_err());
    }
}
