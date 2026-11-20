#[cfg(feature = "kindle")]
mod annotation_export;
#[cfg(feature = "kindle")]
pub mod app;
mod blitz;
mod book_style;
mod cbr;
mod chm;
pub mod comic;
pub mod dictionary;
pub mod djvu;
mod docx;
pub mod epub;
pub mod epub_cfi;
pub mod fixed;
pub mod fixed_epub;
#[cfg(feature = "kindle")]
mod fixed_session;
pub mod fixed_text;
#[cfg(feature = "kindle")]
mod generated_contents;
mod gray_glyph;
mod kf8;
pub mod koreader_import;
pub mod koreader_sidecar;
pub mod library;
mod loose;
pub mod mobi;
pub mod ocr;
#[cfg(feature = "kindle")]
mod ocr_job;
mod parley_layout;
#[cfg(feature = "kindle")]
mod pdf_annotations;
#[cfg(feature = "kindle")]
mod reading_controls;
#[cfg(feature = "kindle")]
mod reading_fonts;
#[cfg(feature = "kindle")]
mod reading_profiles;
#[cfg(feature = "kindle")]
mod reading_stats;
mod rtf;
#[cfg(feature = "kindle")]
mod scan_columns;
#[cfg(feature = "kindle")]
mod scan_reflow;
pub mod search;
pub mod selection;
#[cfg(feature = "kindle")]
mod session;
mod source;
mod svg_image;
mod tcr;
#[cfg(feature = "kindle")]
mod text_flow;
mod tiff_image;
mod vertical_layout;

use std::error::Error;
use std::fs;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use parley::{FontContext, GenericFamily};
use serde::{Deserialize, Serialize};

use crate::epub::{Book, Chapter, Destination, TocEntry};
use crate::source::Source;

pub type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

#[cfg(test)]
fn test_fixture(name: &str) -> PathBuf {
    std::env::var_os("READER_TEST_FIXTURES")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures"))
        .join(name)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Engine {
    Blitz,
    Parley,
}

impl FromStr for Engine {
    type Err = &'static str;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        match value {
            "blitz" => Ok(Self::Blitz),
            "parley" => Ok(Self::Parley),
            _ => Err("Choose either blitz or parley"),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TextAlignment {
    #[default]
    Publisher,
    Start,
    Center,
    Justify,
}

#[derive(Clone, Copy, Debug)]
pub struct Settings {
    pub width: u32,
    pub height: u32,
    pub font_size: f32,
    pub margin: u32,
    pub line_height: f32,
    pub alignment: TextAlignment,
    pub bold_text: bool,
    pub override_fonts: bool,
    pub disable_soft_hyphens: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            width: 1264,
            height: 1680,
            font_size: 42.0,
            margin: 54,
            line_height: 1.35,
            alignment: TextAlignment::Publisher,
            bold_text: false,
            override_fonts: false,
            disable_soft_hyphens: false,
        }
    }
}

impl Settings {
    fn validate(self) -> Result<()> {
        if !(160..=4096).contains(&self.width)
            || !(160..=4096).contains(&self.height)
            || self.margin > self.width.min(self.height) / 4
            || !(12.0..=144.0).contains(&self.font_size)
            || !(1.0..=3.0).contains(&self.line_height)
            || self.text_height() as f32 <= self.font_size * self.line_height * 2.0
        {
            return Err("Reading layout settings are out of range".into());
        }
        Ok(())
    }

    fn text_height(self) -> u32 {
        self.height - 2 * self.margin
    }
}

/// A text location in a specific revision of a chapter. Page numbers are derived.
/// Offsets are UTF-8 bytes in the whitespace-normalized text of a block.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Anchor {
    pub book: u64,
    pub chapter: String,
    pub revision: u64,
    pub block: usize,
    pub offset: usize,
}

struct Line {
    block: usize,
    bytes: Range<usize>,
    top: f32,
    bottom: f32,
    missing_glyphs: usize,
}

struct Pagination {
    bounds: Vec<f32>,
}

impl Pagination {
    fn fixed_stride(height: f32, extent: f32) -> Result<Self> {
        if !extent.is_finite() || !(0.0..=2_000_000.0).contains(&extent) {
            return Err("The chapter exceeds the layout height limit".into());
        }
        let pages = (extent / height).ceil().max(1.0) as usize;
        let mut bounds = (0..pages)
            .map(|page| page as f32 * height)
            .collect::<Vec<_>>();
        bounds.push(extent);
        Ok(Self { bounds })
    }

    fn new(lines: &[Line], height: f32, mut content_height: f32) -> Result<Self> {
        let mut intervals: Vec<Range<f32>> = Vec::new();
        for line in lines {
            if !line.top.is_finite() || !line.bottom.is_finite() || line.top > line.bottom {
                return Err("The renderer returned invalid line positions".into());
            }
            content_height = content_height.max(line.bottom);
            match intervals.last_mut() {
                Some(last) if line.top < last.end => last.end = last.end.max(line.bottom),
                _ => intervals.push(line.top..line.bottom),
            }
        }
        if !content_height.is_finite() || !(0.0..=2_000_000.0).contains(&content_height) {
            return Err("The chapter exceeds the layout height limit".into());
        }
        let mut bounds = vec![0.0];
        for interval in intervals {
            if interval.end - interval.start > height {
                return Err("The chapter contains content taller than one reading page".into());
            }
            let origin = *bounds.last().unwrap();
            if interval.end > origin + height && interval.start > origin {
                while interval.start > *bounds.last().unwrap() + height {
                    bounds.push(bounds.last().unwrap() + height);
                }
                if interval.start > *bounds.last().unwrap() {
                    bounds.push(interval.start);
                }
            }
        }
        while content_height > *bounds.last().unwrap() + height {
            bounds.push(bounds.last().unwrap() + height);
        }
        bounds.push(content_height.max(*bounds.last().unwrap()));
        Ok(Self { bounds })
    }

    fn page(&self, y: f32) -> usize {
        self.bounds[..self.len()]
            .partition_point(|&bound| bound <= y)
            .saturating_sub(1)
    }

    fn len(&self) -> usize {
        self.bounds.len() - 1
    }

    fn range(&self, page: usize) -> Option<Range<f32>> {
        (page < self.len()).then(|| self.bounds[page]..self.bounds[page + 1])
    }
}

enum Renderer {
    Blitz(Box<blitz::BlitzChapter>),
    Parley(parley_layout::ParleyChapter),
    Vertical(Box<vertical_layout::VerticalChapter>),
}

/// One bounded chapter layout. The renderer and cache are replaced at a chapter turn.
pub struct Probe {
    pub title: String,
    pub author: Option<String>,
    pub chapter: String,
    pub chapter_label: String,
    book: u64,
    chapters: Vec<Chapter>,
    pub toc: Vec<TocEntry>,
    #[cfg(feature = "kindle")]
    pub(crate) printed_pages: Vec<TocEntry>,
    settings: Settings,
    source: Source,
    lines: Vec<Line>,
    pages: Pagination,
    renderer: Renderer,
}

impl Probe {
    pub fn open(
        path: &Path,
        engine: Engine,
        fonts: &[PathBuf],
        settings: Settings,
        chapter: usize,
    ) -> Result<Self> {
        settings.validate()?;
        let book = Book::open(path).map_err(|e| e.to_string())?;
        Self::from_book(book, engine, fonts, settings, chapter)
    }

    #[cfg(feature = "kindle")]
    pub(crate) fn from_text(
        title: String,
        text: &str,
        fonts: &[PathBuf],
        settings: Settings,
    ) -> Result<Self> {
        settings.validate()?;
        Self::from_book(
            Book::from_text(title, text)?,
            Engine::Parley,
            fonts,
            settings,
            0,
        )
    }

    fn from_book(
        mut book: Book,
        engine: Engine,
        fonts: &[PathBuf],
        settings: Settings,
        chapter: usize,
    ) -> Result<Self> {
        if book.is_fixed_layout() {
            return Err("Open this fixed-layout EPUB in Reader".into());
        }
        let entry = book
            .chapters
            .get(chapter)
            .ok_or("That chapter is unavailable")?;
        let href = entry.href.clone();
        let chapter_label = entry.label.clone();
        let title = book.title.clone();
        let author = book.author.clone();
        let publication = book.revision();
        let chapters = book.chapters.clone();
        let toc = book.toc.clone();
        #[cfg(feature = "kindle")]
        let printed_pages = book.printed_pages().to_vec();
        let xhtml = book.chapter(chapter).map_err(|e| e.to_string())?;
        let source = Source::parse(&xhtml)?;
        let mut fonts = font_context(fonts)?;
        let (renderer, mut lines, content_height) = match engine {
            Engine::Blitz => {
                let (layout, lines, extent) =
                    blitz::BlitzChapter::new(book, &href, &xhtml, &source, fonts, settings)?;
                (Renderer::Blitz(Box::new(layout)), lines, extent)
            }
            Engine::Parley => {
                let styles =
                    book_style::BookStyle::load(&mut book, &href, &source.stylesheets, &mut fonts)
                        .compute(&source.elements);
                let mut text = source
                    .blocks
                    .iter()
                    .filter(|block| !block.text.is_empty())
                    .peekable();
                let vertical = text.peek().is_some()
                    && source.blocks.iter().all(|block| block.images.is_empty())
                    && !source.elements.iter().any(|element| element.tag == "table")
                    && text.all(|block| {
                        styles[block.element].writing_mode == book_style::WritingMode::VerticalRl
                            && block.styled_text.iter().all(|run| {
                                styles[run.element].writing_mode
                                    == book_style::WritingMode::VerticalRl
                            })
                    });
                let vertical = vertical.then(|| {
                    vertical_layout::VerticalChapter::new(&source, &styles, fonts.clone(), settings)
                });
                match vertical {
                    Some(Ok((layout, lines, extent))) => {
                        (Renderer::Vertical(Box::new(layout)), lines, extent)
                    }
                    fallback => {
                        if let Some(Err(error)) = fallback {
                            eprintln!("reader vertical layout unavailable: {error}");
                        }
                        let (layout, lines, extent) = parley_layout::ParleyChapter::new(
                            &mut book, &href, &source, fonts, settings, &styles,
                        )?;
                        (Renderer::Parley(layout), lines, extent)
                    }
                }
            }
        };
        lines.sort_by(|a, b| a.top.total_cmp(&b.top));
        let height = settings.text_height() as f32;
        let pages = if matches!(&renderer, Renderer::Vertical(_)) {
            Pagination::fixed_stride(height, content_height)?
        } else {
            Pagination::new(&lines, height, content_height)?
        };
        Ok(Self {
            title,
            author,
            chapter: href,
            chapter_label,
            book: publication,
            chapters,
            toc,
            #[cfg(feature = "kindle")]
            printed_pages,
            settings,
            source,
            lines,
            pages,
            renderer,
        })
    }

    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    pub(crate) fn is_vertical(&self) -> bool {
        matches!(&self.renderer, Renderer::Vertical(_))
    }

    pub fn chapter_count(&self) -> usize {
        self.chapters.len()
    }

    #[cfg(feature = "kindle")]
    pub(crate) fn proportional_anchor(&self, fraction: f64) -> Option<Anchor> {
        let total: usize = self
            .source
            .blocks
            .iter()
            .map(|block| block.text.len())
            .sum();
        let mut remaining = (total.saturating_sub(1) as f64 * fraction).round() as usize;
        for block in &self.source.blocks {
            if remaining < block.text.len() {
                return Some(self.anchor(block.key, block.text.floor_char_boundary(remaining)));
            }
            remaining -= block.text.len();
        }
        None
    }

    pub fn chapter_index(&self, path: &str) -> Option<usize> {
        self.chapters
            .iter()
            .position(|chapter| chapter.href == path)
    }

    pub fn chapter_entries(&self) -> &[Chapter] {
        &self.chapters
    }

    /// Resolve a successfully displayed illustration at a page's viewport coordinates.
    pub fn illustration_at(&self, page: usize, x: f32, y: f32) -> Option<&str> {
        self.illustration_in(self.pages.range(page)?, x, y)
    }

    pub(crate) fn illustration_in(&self, range: Range<f32>, x: f32, y: f32) -> Option<&str> {
        let Renderer::Parley(layout) = &self.renderer else {
            return None;
        };
        layout.illustration_at(range, self.settings, x, y)
    }

    /// Load original pixels for a displayed illustration, reopening the local book as needed.
    pub fn open_illustration(&self, path: &Path, package_path: &str) -> Result<comic::ImagePage> {
        if !matches!(&self.renderer, Renderer::Parley(layout) if layout.has_illustration(package_path))
        {
            return Err("This illustration is unavailable in the current chapter".into());
        }
        let mut book = Book::open(path)?;
        if book.revision() != self.book || book.chapter_index(&self.chapter).is_none() {
            return Err("This book changed after it was opened".into());
        }
        let bytes = if svg_image::is_package_svg(package_path) {
            book.resource_limited(package_path, svg_image::MAX_SVG_BYTES)?
        } else {
            book.resource(package_path)?
        };
        drop(book);
        parley_layout::original_illustration(&bytes)
    }

    pub fn link_at(&self, page: usize, x: f32, y: f32) -> Option<Destination> {
        let range = self.pages.range(page)?;
        self.link_in(range, x, y)
    }

    pub(crate) fn link_in(&self, range: Range<f32>, x: f32, y: f32) -> Option<Destination> {
        let (key, offset) = self.source_at(range, x, y)?;
        let block = self.source.blocks.iter().find(|block| block.key == key)?;
        let link = block
            .links
            .iter()
            .find(|link| link.bytes.contains(&offset))?;
        Book::resolve_link(&self.chapter, &link.href).ok()
    }

    fn source_at(&self, range: Range<f32>, x: f32, y: f32) -> Option<(usize, usize)> {
        match &self.renderer {
            Renderer::Vertical(renderer) => renderer.source_at(range, x, y),
            Renderer::Parley(renderer) => {
                let margin = self.settings.margin as f32;
                let y = y - margin + range.start;
                if !range.contains(&y) {
                    return None;
                }
                renderer.source_at(x - margin, y)
            }
            Renderer::Blitz(_) => None,
        }
    }

    pub fn links_on_page(&self, page: usize) -> Vec<(String, Destination)> {
        let Some(bounds) = self.pages.range(page) else {
            return Vec::new();
        };
        self.links_in(bounds)
    }

    pub(crate) fn links_in(&self, bounds: Range<f32>) -> Vec<(String, Destination)> {
        let start = self
            .lines
            .partition_point(|line| line.top < bounds.start)
            .saturating_sub(1);
        let end = self.lines.partition_point(|line| line.top < bounds.end);
        let visible_lines = &self.lines[start..end];
        let mut links = Vec::new();
        for block in &self.source.blocks {
            for link in &block.links {
                let visible = visible_lines.iter().any(|line| {
                    line.bottom > bounds.start
                        && line.block == block.key
                        && line.bytes.start < link.bytes.end
                        && line.bytes.end > link.bytes.start
                });
                if visible && let Ok(destination) = Book::resolve_link(&self.chapter, &link.href) {
                    let label = block.text[link.bytes.clone()].trim();
                    if !label.is_empty() {
                        links.push((label.to_owned(), destination));
                    }
                }
            }
        }
        links
    }

    pub fn matches(&self, text: &str) -> Vec<Anchor> {
        if text.is_empty() {
            return Vec::new();
        }
        self.source
            .blocks
            .iter()
            .flat_map(|block| {
                block
                    .text
                    .match_indices(text)
                    .map(move |(offset, _)| self.anchor(block.key, offset))
            })
            .collect()
    }

    pub fn fragment(&self, name: &str) -> Option<Anchor> {
        self.source
            .fragments
            .get(name)
            .map(|&(block, offset)| self.anchor(block, offset))
    }

    pub fn page_for(&self, anchor: &Anchor) -> Option<usize> {
        self.anchor_top(anchor).map(|top| self.pages.page(top))
    }

    pub(crate) fn anchor_top(&self, anchor: &Anchor) -> Option<f32> {
        let (index, offset) = self.project_anchor(anchor)?;
        let block = &self.source.blocks[index];
        let exact = self
            .lines
            .iter()
            .find(|line| line.block == anchor.block && line.bytes.contains(&offset))
            .map(|line| line.top);
        exact.or_else(|| {
            if offset != block.text.len() {
                return None;
            }
            self.lines
                .iter()
                .rev()
                .find(|line| line.block == anchor.block && !line.bytes.is_empty())
                .map(|line| line.top)
        })
    }

    fn project_anchor(&self, anchor: &Anchor) -> Option<(usize, usize)> {
        if self.book != anchor.book || self.chapter != anchor.chapter {
            return None;
        }
        let blocks = &self.source.blocks;
        let index = blocks
            .binary_search_by_key(&anchor.block, |block| block.key)
            .ok()?;
        let offset = self
            .source
            .project_offset(&blocks[index], anchor.revision, anchor.offset)?;
        Some((index, offset))
    }

    pub fn page_anchor(&self, page: usize) -> Option<Anchor> {
        self.anchor_in(self.pages.range(page)?)
    }

    pub(crate) fn anchor_in(&self, range: Range<f32>) -> Option<Anchor> {
        self.lines
            .iter()
            .find(|line| {
                !line.bytes.is_empty() && line.bottom > range.start && line.top < range.end
            })
            .map(|line| self.anchor(line.block, line.bytes.start))
    }

    #[cfg(feature = "kindle")]
    pub(crate) fn page_range(&self, page: usize) -> Range<f32> {
        self.pages.range(page).unwrap_or_default()
    }

    #[cfg(feature = "kindle")]
    pub(crate) fn scroll_end(&self) -> f32 {
        (*self.pages.bounds.last().unwrap_or(&0.0) - self.settings.text_height() as f32).max(0.0)
    }

    #[cfg(feature = "kindle")]
    pub(crate) fn scroll_range(&self, top: f32) -> Range<f32> {
        top..(top + self.settings.text_height() as f32)
    }

    #[cfg(feature = "kindle")]
    pub(crate) fn page_at(&self, top: f32) -> usize {
        self.pages.page(top)
    }

    /// Number of non-whitespace source characters represented by a rendered line.
    pub fn coverage(&self) -> (usize, usize) {
        let mut seen = std::collections::BTreeMap::<usize, Vec<Range<usize>>>::new();
        for line in &self.lines {
            seen.entry(line.block).or_default().push(line.bytes.clone());
        }
        self.source
            .blocks
            .iter()
            .fold((0, 0), |(covered, total), block| {
                let spans = seen.entry(block.key).or_default();
                spans.sort_unstable_by_key(|span| span.start);
                let mut visible = spans.iter().peekable();
                let mut visible_end = 0;
                block
                    .text
                    .char_indices()
                    .filter(|(_, c)| !c.is_whitespace())
                    .fold((covered, total), |(covered, total), (index, _)| {
                        while visible.peek().is_some_and(|span| span.start <= index) {
                            visible_end = visible_end.max(visible.next().unwrap().end);
                        }
                        (covered + usize::from(index < visible_end), total + 1)
                    })
            })
    }

    pub fn missing_glyphs(&self) -> usize {
        self.lines.iter().map(|line| line.missing_glyphs).sum()
    }

    pub fn capture(&mut self, page: usize) -> Result<Vec<u8>> {
        let range = self.pages.range(page).ok_or("That page is unavailable")?;
        self.capture_range(range)
    }

    pub(crate) fn capture_range(&mut self, range: Range<f32>) -> Result<Vec<u8>> {
        match &mut self.renderer {
            Renderer::Blitz(layout) => layout.capture(range, self.settings),
            Renderer::Parley(layout) => layout.capture(range, self.settings),
            Renderer::Vertical(layout) => layout.capture(range, self.settings),
        }
    }

    fn anchor(&self, block: usize, offset: usize) -> Anchor {
        Anchor {
            book: self.book,
            chapter: self.chapter.clone(),
            revision: self.source.revision,
            block,
            offset,
        }
    }
}

fn revision(text: &str) -> u64 {
    revision_bytes(text.as_bytes())
}

fn revision_bytes(bytes: &[u8]) -> u64 {
    bytes
        .iter()
        .copied()
        .fold(0xcbf29ce484222325_u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
        })
}

fn font_context(paths: &[PathBuf]) -> Result<FontContext> {
    if paths.is_empty() {
        return Err("Provide at least one reading font".into());
    }
    let mut fonts = FontContext::default();
    let mut families = Vec::new();
    for path in paths {
        if fs::metadata(path)?.len() > 64 * 1024 * 1024 {
            return Err("A reading font exceeds the size limit".into());
        }
        let data = parley::fontique::Blob::new(std::sync::Arc::new(fs::read(path)?));
        families.extend(
            fonts
                .collection
                .register_fonts(data, None)
                .into_iter()
                .map(|(id, _)| id),
        );
    }
    if families.is_empty() {
        return Err("No valid font was loaded".into());
    }
    fonts
        .collection
        .set_generic_families(GenericFamily::Serif, families.iter().copied());
    fonts
        .collection
        .set_generic_families(GenericFamily::SansSerif, families.iter().copied());
    Ok(fonts)
}

fn grayscale(rgba: &[u8]) -> impl Iterator<Item = u8> + '_ {
    rgba.as_chunks::<4>().0.iter().map(|p| {
        if p[3] == 0 {
            return 255;
        }
        let white = 255 - u32::from(p[3]);
        if p[0] == p[1] && p[1] == p[2] {
            return (white + u32::from(p[0])).min(255) as u8;
        }
        let value = 77 * u32::from(p[0]) + 150 * u32::from(p[1]) + 29 * u32::from(p[2]);
        (white + ((value + 128) >> 8)).min(255) as u8
    })
}

fn image_reader(bytes: &[u8]) -> Result<image::ImageReader<std::io::Cursor<&[u8]>>> {
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(2048);
    limits.max_image_height = Some(2048);
    limits.max_alloc = Some(24 * 1024 * 1024);
    reader.limits(limits);
    if !matches!(
        reader.format(),
        Some(image::ImageFormat::Png | image::ImageFormat::Jpeg)
    ) {
        return Err("The EPUB image format is outside the probe subset".into());
    }
    Ok(reader)
}
