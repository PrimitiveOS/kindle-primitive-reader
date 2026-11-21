#[cfg(feature = "kindle")]
mod annotation_export;
#[cfg(feature = "kindle")]
pub mod app;
mod blitz;
mod book_menus;
mod book_style;
mod cbr;
mod chm;
pub mod comic;
pub mod dictionary;
#[cfg(feature = "kindle")]
pub mod document_check;
pub mod djvu;
mod docx;
pub mod cre_pointer;
pub mod epub;
pub mod epub_cfi;
mod epub_members;
mod epub_xml;
mod epub_zip;
pub mod fixed;
pub mod fixed_epub;
#[cfg(feature = "kindle")]
mod fixed_session;
pub mod fixed_text;
#[cfg(feature = "kindle")]
mod generated_contents;
mod gray_glyph;
mod gif_image;
#[cfg(feature = "kindle")]
mod footnotes;
mod hyphenation;
mod user_hyphenation;
mod jpeg2000;
mod kf8;
mod kindle_sections;
pub mod koreader_import;
pub mod koreader_sidecar;
pub mod library;
mod list_marker;
mod loose;
pub mod mobi;
pub mod ocr;
#[cfg(feature = "kindle")]
mod ocr_job;
#[cfg(feature = "kindle")]
pub mod online_lookup;
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
mod scan_deskew;
#[cfg(feature = "kindle")]
mod scan_reflow;
pub mod search;
pub mod selection;
#[cfg(feature = "kindle")]
mod session;
mod source;
mod table_layout;
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

/// Grid fitting applied to the native grayscale glyph renderer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FontHinting {
    #[default]
    Off,
    Native,
    Auto,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    pub width: u32,
    pub height: u32,
    pub font_size: f32,
    pub margin: u32,
    pub margin_sides: Option<u32>,
    pub margin_top: Option<u32>,
    pub margin_bottom: Option<u32>,
    pub line_height: f32,
    pub word_spacing: f32,
    pub letter_spacing: f32,
    pub alignment: TextAlignment,
    pub bold_text: bool,
    pub text_contrast: bool,
    pub override_fonts: bool,
    pub disable_soft_hyphens: bool,
    pub automatic_hyphenation: bool,
    /// Load author stylesheets and apply inline CSS during native reflow.
    pub publisher_styles: bool,
    /// Use each font's normal OpenType kerning when shaping text.
    pub font_kerning: bool,
    pub font_hinting: FontHinting,
    pub smooth_images: bool,
    pub invert_night_images: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            width: 1264,
            height: 1680,
            font_size: 42.0,
            margin: 54,
            margin_sides: None,
            margin_top: None,
            margin_bottom: None,
            line_height: 1.35,
            word_spacing: 0.0,
            letter_spacing: 0.0,
            alignment: TextAlignment::Publisher,
            bold_text: false,
            text_contrast: false,
            override_fonts: false,
            disable_soft_hyphens: false,
            automatic_hyphenation: true,
            publisher_styles: true,
            font_kerning: true,
            font_hinting: FontHinting::Off,
            smooth_images: true,
            invert_night_images: false,
        }
    }
}

#[cfg(feature = "kindle")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HyphenationMode {
    Automatic,
    Authored,
    Off,
}

#[cfg(feature = "kindle")]
impl HyphenationMode {
    const ALL: [Self; 3] = [Self::Automatic, Self::Authored, Self::Off];
    fn label(self) -> &'static str {
        match self {
            Self::Automatic => "Automatic",
            Self::Authored => "Authored",
            Self::Off => "Off",
        }
    }
    fn next(self) -> Self {
        match self {
            Self::Automatic => Self::Authored,
            Self::Authored => Self::Off,
            Self::Off => Self::Automatic,
        }
    }
}
impl Settings {
    #[cfg(feature = "kindle")]
    fn hyphenation_mode(self) -> HyphenationMode {
        if self.disable_soft_hyphens {
            HyphenationMode::Off
        } else if self.automatic_hyphenation {
            HyphenationMode::Automatic
        } else {
            HyphenationMode::Authored
        }
    }
    #[cfg(feature = "kindle")]
    fn with_hyphenation(self, mode: HyphenationMode) -> Self {
        Self {
            disable_soft_hyphens: mode == HyphenationMode::Off,
            automatic_hyphenation: mode == HyphenationMode::Automatic,
            ..self
        }
    }
    fn validate(self) -> Result<()> {
        let margin_limit = self.width.min(self.height) / 4;
        if !(160..=4096).contains(&self.width)
            || !(160..=4096).contains(&self.height)
            || self.margin > margin_limit
            || self.side_margin() > margin_limit
            || self.top_margin() > self.height / 4
            || self.bottom_margin() > self.height / 4
            || !(12.0..=144.0).contains(&self.font_size)
            || !(1.0..=3.0).contains(&self.line_height)
            || !(-2.0..=12.0).contains(&self.word_spacing)
            || !(-1.0..=4.0).contains(&self.letter_spacing)
            || self.text_height() as f32 <= self.font_size * self.line_height * 2.0
        {
            return Err("Reading layout settings are out of range".into());
        }
        Ok(())
    }

    fn side_margin(self) -> u32 {
        self.margin_sides.unwrap_or(self.margin)
    }

    fn top_margin(self) -> u32 {
        self.margin_top.unwrap_or(self.margin)
    }

    fn bottom_margin(self) -> u32 {
        self.margin_bottom.unwrap_or(self.margin)
    }

    fn text_width(self) -> u32 {
        self.width - 2 * self.side_margin()
    }

    fn text_height(self) -> u32 {
        self.height - self.top_margin() - self.bottom_margin()
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
    /// Zero-based picture within a text-empty block holding several illustrations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub illustration: Option<usize>,
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
                // Keep any full blank views from intentional large gaps. A short
                // remainder before the next line belongs to the preceding view's
                // trailing whitespace, rather than a separate empty page.
                while interval.start >= *bounds.last().unwrap() + 2.0 * height {
                    bounds.push(bounds.last().unwrap() + height);
                }
                bounds.push(interval.start);
            }
        }
        while content_height > *bounds.last().unwrap() + height {
            bounds.push(bounds.last().unwrap() + height);
        }
        bounds.push(content_height.max(*bounds.last().unwrap()));
        Ok(Self { bounds })
    }

    fn trim_unpainted_tail(&mut self, painted_bottom: f32) {
        if self.len() < 2 || self.bounds[self.len() - 1] < painted_bottom {
            return;
        }
        // Keep at least one view for a deliberately empty spine entry. The source
        // and all of its line positions remain available for links and saved anchors.
        let pages = self.bounds[..self.len()]
            .partition_point(|&start| start < painted_bottom)
            .max(1);
        self.bounds.truncate(pages + 1);
        self.bounds[pages] = painted_bottom;
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
    #[cfg(feature = "kindle")]
    pub(crate) printed_source: Option<String>,
    #[cfg(feature = "kindle")]
    footer_chapters: std::cell::OnceCell<Vec<(usize, String)>>,
    #[cfg(feature = "kindle")]
    publisher_positions: std::cell::OnceCell<app::publisher_pages::PublisherPositions>,
    settings: Settings,
    spine_rtl: bool,
    source: Source,
    #[cfg(feature = "kindle")]
    custom_hyphenation_allowed: Vec<bool>,
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
        book: Book,
        engine: Engine,
        fonts: &[PathBuf],
        settings: Settings,
        chapter: usize,
    ) -> Result<Self> {
        Self::from_book_content(book, engine, fonts, settings, chapter, None)
    }

    fn from_book_content(
        book: Book,
        engine: Engine,
        fonts: &[PathBuf],
        settings: Settings,
        chapter: usize,
        excerpt: Option<String>,
    ) -> Result<Self> {
        Self::from_book_with_rules(
            book, engine, fonts, settings, chapter, excerpt, &user_hyphenation::Rules::default(),
        )
    }

    fn from_book_with_rules(
        mut book: Book,
        engine: Engine,
        fonts: &[PathBuf],
        settings: Settings,
        chapter: usize,
        excerpt: Option<String>,
        user_rules: &user_hyphenation::Rules,
    ) -> Result<Self> {
        if book.is_fixed_layout() {
            return Err("Open this fixed-layout EPUB in Reader".into());
        }
        if engine == Engine::Blitz
            && (!settings.publisher_styles
                || !settings.font_kerning
                || settings.font_hinting != FontHinting::Off)
        {
            return Err("Publisher style, kerning and hinting controls require Parley".into());
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
        let spine_rtl = book.spine_rtl;
        let chapters = book.chapters.clone();
        let toc = book.toc.clone();
        #[cfg(feature = "kindle")]
        let printed_pages = book.printed_pages().to_vec();
        #[cfg(feature = "kindle")]
        let printed_source = book.printed_page_source().map(str::to_owned);
        let xhtml = match excerpt {
            Some(value) => value,
            None => book.chapter(chapter).map_err(|e| e.to_string())?,
        };
        let mut source = Source::parse_with_language(&xhtml, book.language)?;
        let mut fonts = font_context(fonts)?;
        #[cfg(feature = "kindle")]
        let mut custom_hyphenation_allowed = Vec::new();
        let (renderer, mut lines, content_height) = match engine {
            Engine::Blitz => {
                let (layout, lines, extent) =
                    blitz::BlitzChapter::new(book, &href, &xhtml, &source, fonts, settings)?;
                (Renderer::Blitz(Box::new(layout)), lines, extent)
            }
            Engine::Parley => {
                let author = settings
                    .publisher_styles
                    .then(|| {
                        book_style::BookStyle::load(
                            &mut book,
                            &href,
                            &source,
                            &mut fonts,
                        )
                    })
                    .unwrap_or_default();
                let styles = author.compute(&source.elements);
                #[cfg(feature = "kindle")]
                {
                    custom_hyphenation_allowed = styles.iter().map(hyphenation::allows_user_rules).collect();
                }
                let mut text = source
                    .blocks
                    .iter()
                    .filter(|block| !block.text.is_empty())
                    .peekable();
                let vertical = text.peek().is_some()
                    && source.blocks.iter().all(|block| block.images.is_empty())
                    && !source.elements.iter().any(|element| {
                        !element.selector_only && element.tag == "table"
                    })
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
                        let has_table = source.blocks.iter().any(|b| b.table_cell.is_some());
                        let mut lay_out = |source: &Source, faces| {
                            parley_layout::ParleyChapter::new(
                                &mut book, &href, source, faces, settings, &styles, user_rules,
                            )
                        };
                        let (layout, lines, extent) = if has_table {
                            let grid = lay_out(&source, fonts.clone()).and_then(
                                |(layout, mut lines, extent)| {
                                    lines.sort_by(|a, b| a.top.total_cmp(&b.top));
                                    Pagination::new(&lines, settings.text_height() as f32, extent)?;
                                    Ok((layout, lines, extent))
                                },
                            );
                            match grid {
                                Ok(result) => result,
                                Err(_) => {
                                    // A tall row can have no safe horizontal break across
                                    // all its cells. Keep the chapter readable in source order.
                                    for block in &mut source.blocks {
                                        block.table_cell = None;
                                    }
                                    lay_out(&source, fonts)?
                                }
                            }
                        } else {
                            lay_out(&source, fonts)?
                        };
                        (Renderer::Parley(layout), lines, extent)
                    }
                }
            }
        };
        lines.sort_by(|a, b| a.top.total_cmp(&b.top));
        let height = settings.text_height() as f32;
        let mut pages = if matches!(&renderer, Renderer::Vertical(_)) {
            Pagination::fixed_stride(height, content_height)?
        } else {
            Pagination::new(&lines, height, content_height)?
        };
        if pages.len() > 1 && let Renderer::Parley(chapter) = &renderer {
            pages.trim_unpainted_tail(chapter.last_paint_bottom(&source));
        }
        Ok(Self {
            title,
            author,
            chapter: href,
            chapter_label,
            book: publication,
            spine_rtl,
            chapters,
            toc,
            #[cfg(feature = "kindle")]
            printed_pages,
            #[cfg(feature = "kindle")]
            printed_source,
            #[cfg(feature = "kindle")]
            footer_chapters: Default::default(),
            #[cfg(feature = "kindle")]
            publisher_positions: Default::default(),
            settings,
            #[cfg(feature = "kindle")]
            custom_hyphenation_allowed,
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

    /// The visual direction of forward page turns for this chapter.
    pub fn pages_right_to_left(&self) -> bool {
        self.spine_rtl || self.is_vertical()
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

    fn links_to_image(&self, path: &str) -> bool {
        let supported = path.rsplit_once('.').is_some_and(|(_, ext)| {
            ["png", "jpg", "jpeg", "gif", "svg"].iter().any(|known| ext.eq_ignore_ascii_case(known))
        });
        supported && self.source.blocks.iter().any(|block| {
            block.links.iter().map(|link| link.href.as_str())
                .chain(block.images.iter().filter_map(|image| image.link.as_deref()))
                .any(|href| Book::resolve_link(&self.chapter, href)
                    .is_ok_and(|target| target.fragment.is_none() && target.path == path))
        })
    }

    /// Load original pixels for a displayed illustration, reopening the local book as needed.
    pub fn open_illustration(&self, path: &Path, package_path: &str) -> Result<comic::ImagePage> {
        if !matches!(&self.renderer, Renderer::Parley(layout) if layout.has_illustration(package_path)) && !self.links_to_image(package_path)
        {
            return Err("This illustration is unavailable in the current chapter".into());
        }
        let mut book = Book::open(path)?;
        if book.revision() != self.book || book.chapter_index(&self.chapter).is_none() {
            return Err("This book changed after it was opened".into());
        }
        if let Some(xml) = self.source.blocks.iter().flat_map(|block| &block.images)
            .find(|image| image.href == package_path).and_then(|image| image.inline_svg.as_deref()) {
            let (gray, _) = svg_image::render_inline(&mut book, &self.chapter, xml, svg_image::MAX_SVG_SIDE,
                svg_image::MAX_SVG_SIDE, 8 * 1024 * 1024, None)?;
            return comic::ImagePage::from_grayscale(gray.width(), gray.height(), gray.into_raw());
        }
        if svg_image::is_package_svg(package_path) {
            let bytes = book.resource_limited(package_path, svg_image::MAX_SVG_BYTES)?;
            return svg_image::original_package_image(&mut book, package_path, &bytes);
        }
        let bytes = book.resource(package_path)?;
        drop(book);
        parley_layout::original_illustration(&bytes)
    }

    pub fn link_at(&self, page: usize, x: f32, y: f32) -> Option<Destination> {
        let range = self.pages.range(page)?;
        self.link_in(range, x, y)
    }

    pub(crate) fn link_in(&self, range: Range<f32>, x: f32, y: f32) -> Option<Destination> {
        if let Renderer::Parley(layout) = &self.renderer
            && let Some(href) = layout.image_link_at(range.clone(), self.settings, x, y)
        {
            return Book::resolve_link(&self.chapter, href).ok();
        }
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
                let sides = self.settings.side_margin() as f32;
                let local_y = y - self.settings.top_margin() as f32;
                let y = range.start + local_y;
                if !(0.0..self.settings.text_height() as f32).contains(&local_y)
                    || !range.contains(&y)
                    || !(sides..self.settings.width as f32 - sides).contains(&x)
                {
                    return None;
                }
                renderer.source_at(x - sides, y, &range)
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
        // Several cells may have lines crossing the top of a scrolled view.
        // A paginated line can extend upward by at most one reading viewport.
        let earliest = bounds.start - self.settings.text_height() as f32;
        let start = self.lines.partition_point(|line| line.top < earliest);
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
        if let Renderer::Parley(layout) = &self.renderer {
            for (label, href) in layout.image_links(bounds, self.settings) {
                if let Ok(target) = Book::resolve_link(&self.chapter, href)
                    && !links.iter().any(|(_, existing)| existing == &target)
                {
                    links.push((label.to_owned(), target));
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
        if let Some(ordinal) = anchor.illustration {
            if offset != 0 || !block.text.is_empty() || ordinal >= block.images.len() {
                return None;
            }
            return self
                .lines
                .iter()
                .filter(|line| line.block == block.key && line.bytes.is_empty())
                .nth(ordinal)
                .map(|line| line.top);
        }
        let exact = self
            .lines
            .iter()
            .find(|line| line.block == anchor.block && line.bytes.contains(&offset))
            .map(|line| line.top);
        exact.or_else(|| {
            if offset != block.text.len() {
                return None;
            }
            if block.text.is_empty() {
                self.lines
                    .iter()
                    .find(|line| line.block == block.key)
                    .map(|line| line.top)
            } else {
                self.lines
                    .iter()
                    .rev()
                    .find(|line| line.block == block.key && !line.bytes.is_empty())
                    .map(|line| line.top)
            }
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
            .or_else(|| {
                self.lines.iter().find(|line| {
                    line.bottom > range.start
                        && line.top < range.end
                        && self
                            .source
                            .blocks
                            .binary_search_by_key(&line.block, |block| block.key)
                            .is_ok_and(|index| self.source.blocks[index].text.is_empty())
                })
            })
            .map(|line| {
                let mut anchor = self.anchor(line.block, line.bytes.start);
                if line.bytes.is_empty()
                    && let Ok(index) = self
                        .source
                        .blocks
                        .binary_search_by_key(&line.block, |b| b.key)
                    && self.source.blocks[index].text.is_empty()
                    && self.source.blocks[index].images.len() > 1
                {
                    let ordinal = self
                        .lines
                        .iter()
                        .take_while(|entry| entry.top < line.top)
                        .filter(|entry| entry.block == line.block && entry.bytes.is_empty())
                        .count();
                    if ordinal > 0 {
                        anchor.illustration = Some(ordinal);
                    }
                }
                anchor
            })
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
            illustration: None,
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
    font_context_with_firmware(paths, Path::new("/usr/java/lib/fonts"))
}

fn font_context_with_firmware(paths: &[PathBuf], firmware: &Path) -> Result<FontContext> {
    if paths.is_empty() {
        return Err("Provide at least one reading font".into());
    }
    let mut fonts = FontContext::default();
    let mut families = Vec::new();
    for path in paths {
        let metadata = fs::metadata(path)?;
        if metadata.len() > 64 * 1024 * 1024 {
            return Err("A reading font exceeds the size limit".into());
        }
        if path.parent() == Some(firmware) && metadata.is_file() {
            // The firmware volume stays mounted when books and user fonts are removed
            // over USB. Fontique can safely map these fonts on demand without a heap
            // copy of the whole fallback stack on every chapter open.
            let known = fonts.collection.family_ids().collect::<Vec<_>>();
            fonts.collection.load_fonts_from_paths([path]);
            families.extend(
                fonts
                    .collection
                    .family_ids()
                    .filter(|id| !known.contains(id)),
            );
            continue;
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

#[cfg(test)]
mod firmware_font_tests {
    use super::*;
    use parley::{
        Layout, LayoutContext, PositionedLayoutItem, StyleProperty, fontique::SourceKind,
    };

    fn missing(fonts: &mut FontContext, text: &str) -> usize {
        let mut context = LayoutContext::new();
        let mut builder = context.ranged_builder(fonts, text, 1.0, true);
        builder.push_default(StyleProperty::FontFamily(GenericFamily::Serif.into()));
        let mut layout: Layout<()> = builder.build(text);
        layout.break_all_lines(None);
        layout
            .lines()
            .flat_map(|line| line.items())
            .map(|item| match item {
                PositionedLayoutItem::GlyphRun(run) => {
                    run.positioned_glyphs().filter(|g| g.id == 0).count()
                }
                PositionedLayoutItem::InlineBox(_) => 0,
            })
            .sum()
    }

    #[test]
    fn file_backed_fallback_preserves_priority_and_shapes_missing_characters() {
        let first = test_fixture("fonts/Silkscreen-Regular.ttf");
        let fallback = test_fixture("fonts/NotoSans-Regular.ttf");
        let firmware = fallback.parent().unwrap();
        let paths = [first, fallback.clone()];
        let mut primary = font_context_with_firmware(&paths[..1], firmware).unwrap();
        assert_eq!(missing(&mut primary, "AΩ"), 1);
        let mut fonts = font_context_with_firmware(&paths, firmware).unwrap();
        let stack: Vec<_> = fonts
            .collection
            .generic_families(GenericFamily::Serif)
            .collect();
        assert_eq!(stack.len(), 2);
        for (id, expected_path) in stack.into_iter().zip(&paths) {
            let family = fonts.collection.family(id).unwrap();
            assert!(family.fonts().iter().any(|font|
                matches!(font.source().kind(), SourceKind::Path(path) if path.as_ref() == expected_path)));
        }
        assert_eq!(missing(&mut fonts, "AΩ"), 0);
        // Neither test font assigns a glyph to this private-use code point.
        assert_eq!(missing(&mut fonts, "\u{F0000}"), 1);
    }
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
    limits.max_image_width = Some(comic::MAX_IMAGE_SIDE);
    limits.max_image_height = Some(comic::MAX_IMAGE_SIDE);
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

fn epub_raster_dimensions(bytes: &[u8]) -> Result<(u32, u32)> {
    if gif_image::has_signature(bytes) { return gif_image::dimensions(bytes); }
    let (width, height) = image_reader(bytes)?.into_dimensions()?;
    if !comic::image_dimensions_valid(width, height) {
        return Err("The EPUB image exceeds Reader's decoded image limit".into());
    }
    Ok((width, height))
}
