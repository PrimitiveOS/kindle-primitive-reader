use std::fs::{self, File};
use std::io::{self, Read};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::scan_deskew::{DeskewCache, FULL_PAGE, Quad, source_rect};
use hayro::kurbo::{Affine, Point};
use primitive_app::{WIDTH, with_storage};
use primitive_ui::{CAPTION, Canvas, GrayImage, Input, Rect};

use crate::Result;
use crate::comic::{Comic, ImagePage};
use crate::djvu::{Djvu, MAX_DJVU_BYTES};
use crate::fixed::{MAX_PDF_BYTES, Pdf, PdfLink, PdfOutlineItem, RenderRequest, pdf_open_timing};
use crate::fixed_epub::{FixedEpub, SpreadPlan};
use crate::fixed_text::{PdfPageText, PdfTextRect};
use crate::library::{
    DocumentFormat, FixedPreferences as View, MAX_CONTINUOUS_GAP, MAX_SCAN_WHITE_LEVEL,
    MIN_SCAN_WHITE_LEVEL,
};
use crate::session::{
    DONE, PAGE_HEIGHT, PAGE_REFRESH_GAP, PagePaint, PageTurns, draw_document_and_footer,
};

#[path = "fixed_selection_range.rs"]
mod selection_range;

#[path = "comic_panel.rs"]
mod comic_panel;

#[path = "pdf_text_session.rs"]
mod pdf_text_session;
use pdf_text_session::ReadingPdf;

pub(crate) const SMALLER: Rect = Rect::new(840, PAGE_HEIGHT as i32 + 5, 120, 60);
const LARGER: Rect = Rect::new(974, PAGE_HEIGHT as i32 + 5, 120, 60);
const ZOOM: [f32; 5] = [1.0, 1.25, 1.5, 2.0, 3.0];
const MAX_OCR_RASTER_SIDE: f32 = 1800.0;
const MAX_OCR_CACHE_BYTES: u64 = 512 * 1024;
const MAX_OCR_CACHE_PAGES: usize = 32;
const MAX_VISIBLE_PDF_PAGES: usize = 8;
const THUMBNAIL_WIDTH: u32 = 220;
const THUMBNAIL_HEIGHT: u32 = 300;
const SPREAD_GUTTER: u32 = 12;
const MAX_CONTRAST_SAMPLES: usize = 32_768;

pub(crate) struct FixedReading {
    source: FixedDocument,
    page: usize,
    view: View,
    frame: GrayImage,
    deskew_cache: DeskewCache,
    visible: Vec<VisiblePage>,
    epub_spreads: Option<EpubSpreads>,
    turns: PageTurns,
    pending_scroll: (f32, f32),
    end_attempt: bool,
    outline: Vec<PdfOutlineItem>,
    ocr_path: Option<PathBuf>,
    last_ocr: Option<(usize, CachedRecognition)>,
    pub(crate) ocr_language: Option<String>,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct CachedRecognition {
    language: String,
    text: PdfPageText,
}

struct VisiblePage {
    index: usize,
    size: (f32, f32),
    rows: Range<u32>,
    request: RenderRequest,
    correction: Affine,
}

struct EpubSpreads {
    plan: SpreadPlan,
    available: bool,
    right_to_left: bool,
}

pub(crate) enum FixedJump {
    /// Upright source points for PDF XYZ links and scan viewport history.
    LinkPoint(Point),
    /// Source word rectangles in reading order, normalized to the upright page.
    Regions(Vec<PdfTextRect>),
}

impl FixedJump {
    pub(crate) fn regions(bounds: &[PdfTextRect]) -> Option<Self> {
        (!bounds.is_empty()).then(|| Self::Regions(bounds.to_vec()))
    }
}

enum Fit {
    Page,
    Width,
    Height,
}

pub(crate) enum WordAtTouch {
    Ready(Option<PdfSelection>),
    Recognize { page: usize, point: Point },
}

pub(crate) struct PdfSelection {
    page: usize,
    document: PdfPageText,
    first: usize,
    last: usize,
}

impl PdfSelection {
    pub(crate) fn at(page: usize, point: Point, document: PdfPageText) -> Option<Self> {
        let index = document
            .words
            .iter()
            .position(|word| source_rect((1.0, 1.0), word.bounds).contains(point))?;
        Some(Self {
            page,
            document,
            first: index,
            last: index,
        })
    }

    pub(crate) fn text(&self) -> &str {
        let words = &self.document.words;
        &self.document.text[words[self.first].bytes.start..words[self.last].bytes.end]
    }

    pub(crate) fn bounds(&self) -> impl Iterator<Item = PdfTextRect> + '_ {
        self.document.words[self.first..=self.last]
            .iter()
            .map(|word| word.bounds)
    }

    pub(crate) fn page(&self) -> usize {
        self.page
    }
}

enum FixedDocument {
    Epub(Box<FixedEpub>),
    Djvu(Box<Djvu>),
    Pdf(ReadingPdf),
    Comic(Comic),
    Image(ImagePage),
}

/// Read under the caller’s storage guard. The same owned buffer goes to the identity hash and Hayro.
pub(crate) fn pdf_bytes(path: &Path, expected_size: Option<u64>) -> io::Result<Vec<u8>> {
    let mut file = File::open(path)?;
    let metadata = file.metadata()?;
    let size = metadata.len();
    if size > MAX_PDF_BYTES as u64 {
        return Err(io::Error::other(crate::fixed::PdfError::TooLarge));
    }
    if !metadata.is_file() || expected_size.is_some_and(|expected| expected != size) {
        return Err(io::Error::other("The PDF changed while opening"));
    }
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(size as usize).map_err(io::Error::other)?;
    bytes.resize(size as usize, 0);
    file.read_exact(&mut bytes)?;
    if file.read(&mut [0])? != 0 {
        return Err(io::Error::other("The PDF changed while opening"));
    }
    Ok(bytes)
}

impl FixedReading {
    pub(crate) fn open(
        viewport_height: u32,
        format: DocumentFormat,
        path: &Path,
        page: usize,
        fonts: &[PathBuf],
        preferences: View,
        password: Option<&str>,
        prepared_pdf: Option<Vec<u8>>,
    ) -> Result<Option<Self>> {
        let source = match format {
            DocumentFormat::Epub => {
                let source = with_storage(|| {
                    FixedEpub::open_at(path, fonts, (WIDTH, PAGE_HEIGHT)).map_err(io::Error::other)
                })?;
                let Some(book) = source else { return Ok(None) };
                FixedDocument::Epub(Box::new(book))
            }
            DocumentFormat::Pdf => {
                let bytes = match prepared_pdf {
                    Some(bytes) => bytes,
                    None => with_storage(|| pdf_bytes(path, None))?,
                };
                let pdf = pdf_open_timing("parse", || {
                    Pdf::open_with_password(bytes, password.unwrap_or_default())
                })?;
                FixedDocument::Pdf(pdf.into())
            }
            DocumentFormat::Djvu => {
                let bytes = with_storage(|| {
                    let mut bytes = Vec::new();
                    File::open(path)?
                        .take((MAX_DJVU_BYTES + 1) as u64)
                        .read_to_end(&mut bytes)?;
                    Ok(bytes)
                })?;
                FixedDocument::Djvu(Box::new(Djvu::open(bytes)?))
            }
            format if format.is_comic() => FixedDocument::Comic(with_storage(|| {
                Comic::open(path).map_err(io::Error::other)
            })?),
            format if format.is_still_image() => FixedDocument::Image(with_storage(|| {
                ImagePage::open(path).map_err(io::Error::other)
            })?),
            _ => return Ok(None),
        };
        Self::with_source_height(source, page, preferences, viewport_height).map(Some)
    }

    pub(crate) fn inspect_image(image: ImagePage) -> Result<Self> {
        Self::with_source(FixedDocument::Image(image), 0, View::default())
    }

    /// Fit the touched panel in a separate viewer, leaving this reading view intact.
    pub(crate) fn inspect_comic_panel(&self, x: f32, y: f32) -> Result<Option<Self>> {
        let FixedDocument::Comic(comic) = &self.source else {
            return Ok(None);
        };
        if self.is_spread()
            || !(0.0..WIDTH as f32).contains(&x)
            || !(0.0..self.frame.height as f32).contains(&y)
        {
            return Ok(None);
        }
        let panel = {
            let image = with_storage(|| comic.decode_page(self.page).map_err(io::Error::other))?;
            let size = image.page_size();
            let mut view = self.view;
            let request = viewport(size, &mut view, self.frame.height);
            let point = request.page_to_viewport(size)?.inverse()
                * hayro::kurbo::Point::new(f64::from(x), f64::from(y));
            comic_panel::panel_at(&image, point.x as f32, point.y as f32)?
        };
        panel
            .map(|image| {
                Self::with_source(
                    FixedDocument::Image(image),
                    0,
                    View {
                        turns: self.view.turns,
                        ..View::default()
                    },
                )
            })
            .transpose()
    }

    fn with_source(source: FixedDocument, page: usize, preferences: View) -> Result<Self> {
        Self::with_source_height(source, page, preferences, PAGE_HEIGHT)
    }

    fn with_source_height(
        mut source: FixedDocument,
        page: usize,
        preferences: View,
        viewport_height: u32,
    ) -> Result<Self> {
        let page = page.min(source.count() - 1);
        let outline = match &source {
            FixedDocument::Epub(book) => book.outline(),
            FixedDocument::Pdf(pdf) => pdf_open_timing("outline", || pdf.outline()),
            _ => Vec::new(),
        };
        let view = preferences.normalized();
        let epub_spreads = source.epub_spreads(view, viewport_height);
        let mut deskew_cache = DeskewCache::default();
        let mut present = || {
            source.presentation(
                page,
                view,
                epub_spreads.as_ref(),
                &mut deskew_cache,
                viewport_height,
            )
        };
        let (page, view, frame, visible) = if matches!(&source, FixedDocument::Pdf(_)) {
            pdf_open_timing("first_raster", present)
        } else {
            present()
        }?;
        if let FixedDocument::Djvu(book) = &mut source {
            book.set_outline_page(page);
        }
        Ok(Self {
            source,
            page,
            view,
            frame,
            deskew_cache,
            visible,
            epub_spreads,
            turns: PageTurns::new(Instant::now()),
            pending_scroll: (0.0, 0.0),
            end_attempt: false,
            outline,
            ocr_path: None,
            last_ocr: None,
            ocr_language: None,
        })
    }

    pub(crate) fn title(&self) -> Option<String> {
        match &self.source {
            FixedDocument::Epub(book) => Some(book.title().to_owned()),
            FixedDocument::Pdf(pdf) => pdf.title(),
            _ => None,
        }
    }

    pub(crate) fn author(&self) -> Option<String> {
        match &self.source {
            FixedDocument::Epub(book) => book.author().map(str::to_owned),
            FixedDocument::Pdf(pdf) => pdf.author(),
            _ => None,
        }
    }

    pub(crate) fn page(&self) -> usize {
        self.page
    }

    pub(crate) fn count(&self) -> usize {
        self.source.count()
    }

    pub(crate) fn page_label(&self, page: usize) -> Option<String> {
        match &self.source {
            FixedDocument::Epub(book) => book.page_label(page).map(str::to_owned),
            FixedDocument::Pdf(pdf) => pdf.page_label(page),
            _ => None,
        }
    }

    pub(crate) fn find_page_label(&self, label: &str) -> Option<usize> {
        match &self.source {
            FixedDocument::Epub(book) => book.find_page_label(label),
            FixedDocument::Pdf(pdf) => pdf.find_page_label(label),
            _ => None,
        }
    }

    pub(crate) fn is_encrypted_pdf(&self) -> bool {
        matches!(&self.source, FixedDocument::Pdf(pdf) if pdf.is_encrypted())
    }

    pub(crate) fn pdf_geometry(&self, page: usize) -> Option<crate::pdf_annotations::PageGeometry> {
        match &self.source {
            FixedDocument::Pdf(pdf) => pdf.page_geometry(page),
            _ => None,
        }
    }

    pub(crate) fn visible_pages(&self) -> impl Iterator<Item = usize> + '_ {
        let fixed = if self.visible.is_empty() {
            self.spread_pages()
        } else {
            [None, None]
        };
        self.visible
            .iter()
            .map(|page| page.index)
            .chain(fixed.into_iter().flatten())
    }

    /// Render one complete page for the overview in the current orientation.
    pub(crate) fn thumbnail(&self, page: usize) -> Result<GrayImage> {
        if page >= self.count() {
            return Err("That page is outside the document".into());
        }
        let (width, height, turns) = (THUMBNAIL_WIDTH, THUMBNAIL_HEIGHT, self.view.turns);
        let pixels = match &self.source {
            FixedDocument::Pdf(pdf) => pdf.fit(page, width, height, turns)?,
            FixedDocument::Epub(book) => with_storage(|| {
                book.decode_page(page)
                    .map_err(io::Error::other)?
                    .fit(width, height, turns)
                    .map_err(io::Error::other)
            })?,
            FixedDocument::Comic(comic) => with_storage(|| {
                comic
                    .decode_page(page)
                    .map_err(io::Error::other)?
                    .fit(width, height, turns)
                    .map_err(io::Error::other)
            })?,
            FixedDocument::Image(image) => image.fit(width, height, turns)?,
            FixedDocument::Djvu(book) => {
                let (mut w, mut h) = book.page_size(page)?;
                if !turns.is_multiple_of(2) {
                    (w, h) = (h, w);
                }
                let scale = (width as f32 / w).min(height as f32 / h);
                book.render(
                    page,
                    RenderRequest {
                        width,
                        height,
                        scale,
                        x: (width as f32 - w * scale).max(0.0) / 2.0,
                        y: (height as f32 - h * scale).max(0.0) / 2.0,
                        turns,
                    },
                )?
            }
        };
        Ok(GrayImage {
            width,
            height,
            pixels,
        })
    }

    pub(crate) fn supports_continuous(&self) -> bool {
        matches!(self.source, FixedDocument::Pdf(_))
    }

    pub(crate) fn is_continuous(&self) -> bool {
        self.view.continuous && self.view.width_fit && self.supports_continuous()
    }

    pub(crate) fn set_continuous(&mut self, enabled: bool) -> Result<()> {
        let FixedDocument::Pdf(pdf) = &self.source else {
            return Err("Continuous scrolling is available for PDF".into());
        };
        if self.is_continuous() == enabled {
            return Ok(());
        }
        let mut view = self.view;
        let size = pdf.page_size(self.page)?;
        let center = ((view_metrics(size, &view, self.frame.height).height
            - self.frame.height as f32)
            / 2.0)
            .max(0.0);
        if enabled {
            view.offset_y = if view.height_fit {
                0.0
            } else {
                (view.offset_y - center).min(0.0)
            };
            view.width_fit = true;
            view.height_fit = false;
        } else {
            view.offset_y += center;
        }
        view.continuous = enabled;
        self.present(self.page, view)?;
        self.pause();
        Ok(())
    }

    pub(crate) fn set_continuous_gap(&mut self, gap: u16) -> Result<()> {
        if !self.supports_continuous() {
            return Err("Page spacing is available in continuous PDF reading".into());
        }
        if gap > MAX_CONTINUOUS_GAP {
            return Err("This page gap exceeds Reader's limit".into());
        }
        if self.view.continuous_gap != gap {
            if self.is_continuous() {
                self.present(
                    self.page,
                    View {
                        continuous_gap: gap,
                        ..self.view
                    },
                )?;
                self.pause();
            } else {
                self.view.continuous_gap = gap;
            }
        }
        Ok(())
    }

    pub(crate) fn chapter_outline(&self) -> impl Iterator<Item = &PdfOutlineItem> {
        self.outline().iter().enumerate().filter_map(|(index, item)| {
            if let FixedDocument::Djvu(book) = &self.source
                && !book.stable_outline_destination(index)
            {
                return None;
            }
            Some(item)
        })
    }

    pub(crate) fn outline(&self) -> &[PdfOutlineItem] {
        match &self.source {
            FixedDocument::Djvu(book) => book.outline(),
            _ => &self.outline,
        }
    }

    pub(crate) fn preferences(&self) -> View {
        self.view
    }

    pub(crate) fn supports_scan_contrast(&self) -> bool {
        self.source.supports_scan_contrast()
    }

    pub(crate) fn scan_contrast(&self) -> bool {
        self.view.scan_contrast
    }

    pub(crate) fn set_scan_contrast(&mut self, higher: bool) -> Result<()> {
        if higher && !self.supports_scan_contrast() {
            return Err("Contrast adjustment is available for PDF, DjVu, and comics".into());
        }
        self.view.scan_contrast = higher;
        Ok(())
    }

    pub(crate) fn set_force_ocr(&mut self, enabled: bool) -> Result<()> {
        if !self.supports_text() {
            return Err("OCR selection is available for PDF and DjVu".into());
        }
        self.view.force_ocr = enabled;
        Ok(())
    }

    pub(crate) fn set_straighten(&mut self, limit: u8) -> Result<()> {
        if !self.supports_text() || limit > crate::library::MAX_STRAIGHTEN_DEGREES {
            return Err("Auto straighten supports PDF and DjVu scans up to 25°".into());
        }
        if self.view.straighten_limit != limit {
            self.present(
                self.page,
                View {
                    straighten_limit: limit,
                    ..self.view
                },
            )?;
            self.pause();
        }
        Ok(())
    }

    pub(crate) fn set_dither(&mut self, enabled: bool) {
        self.view.dither = enabled;
    }

    pub(crate) fn set_scan_white_level(&mut self, level: Option<u8>) -> Result<()> {
        if level
            .is_some_and(|level| !(MIN_SCAN_WHITE_LEVEL..=MAX_SCAN_WHITE_LEVEL).contains(&level))
        {
            return Err("This scan white level exceeds Reader's limits".into());
        }
        if level.is_some() && !self.supports_scan_contrast() {
            return Err("Scan white level is available for PDF, DjVu, and comics".into());
        }
        self.view.scan_white_level = level;
        Ok(())
    }

    pub(crate) fn supports_rtl(&self) -> bool {
        matches!(
            self.source,
            FixedDocument::Comic(_) | FixedDocument::Epub(_)
        )
    }

    pub(crate) fn reverse_direction(&mut self) -> Result<()> {
        let right_to_left = !self.view.right_to_left;
        let epub_right_to_left = self.epub_spreads.as_ref().map(|_| right_to_left);
        if epub_right_to_left.is_some() || self.spread_pages()[1].is_some() {
            self.present(
                self.page,
                View {
                    right_to_left,
                    epub_right_to_left,
                    ..self.view
                },
            )?;
        } else {
            self.view.right_to_left = right_to_left;
        }
        self.pause();
        Ok(())
    }

    pub(crate) fn supports_spreads(&self) -> bool {
        self.epub_spreads.as_ref().map_or_else(
            || self.source.supports_comic_spreads(),
            |epub| epub.available,
        )
    }

    pub(crate) fn is_spread(&self) -> bool {
        self.view.spread
    }

    pub(crate) fn set_spread(&mut self, enabled: bool) -> Result<()> {
        if enabled && !self.supports_spreads() {
            return Err("Spreads are unavailable for this document".into());
        }
        if self.is_spread() != enabled {
            self.present(
                self.page,
                View {
                    spread: enabled,
                    epub_spread: self.epub_spreads.as_ref().map(|_| enabled),
                    ..self.view
                },
            )?;
            self.pause();
        }
        Ok(())
    }

    fn spread_pages(&self) -> [Option<usize>; 2] {
        if self.is_spread() {
            if let Some(epub) = &self.epub_spreads {
                if let Some(spread) = epub.plan.group(self.page) {
                    return spread.pages();
                }
            } else {
                return [
                    Some(self.page),
                    comic_spread_partner(self.page, self.count()),
                ];
            }
        }
        [Some(self.page), None]
    }

    pub(crate) fn supports_crop(&self) -> bool {
        self.supports_text()
    }

    pub(crate) fn start_crop(&mut self) -> Result<View> {
        let original = self.view;
        self.present(
            self.page,
            View {
                turns: original.turns,
                scan_contrast: original.scan_contrast,
                dither: original.dither,
                continuous_gap: original.continuous_gap,
                scan_white_level: original.scan_white_level,
                ..View::default()
            },
        )?;
        Ok(original)
    }

    pub(crate) fn restore_view(&mut self, preferences: View) -> Result<()> {
        self.present(self.page, preferences.normalized())
    }

    pub(crate) fn crop_point(&self, x: f32, y: f32) -> Option<(f32, f32)> {
        let point = self.source_point(self.page, x, y)?;
        Some((point.x as f32, point.y as f32))
    }

    pub(crate) fn auto_crop(&self) -> Option<PdfTextRect> {
        let mut left = WIDTH;
        let mut right = 0;
        let mut top = self.frame.height;
        let mut bottom = 0;
        for (row, pixels) in self
            .frame
            .pixels
            .as_chunks::<{ WIDTH as usize }>()
            .0
            .iter()
            .enumerate()
        {
            let Some(first) = pixels.iter().position(|gray| *gray < 192) else {
                continue;
            };
            let last = pixels.iter().rposition(|gray| *gray < 192)?;
            left = left.min(first as u32);
            right = right.max(last as u32);
            top = top.min(row as u32);
            bottom = bottom.max(row as u32);
        }
        if left >= right || top >= bottom {
            return None;
        }
        let a = self.crop_point(left as f32, top as f32)?;
        let b = self.crop_point(right as f32, bottom as f32)?;
        Some(PdfTextRect {
            x0: (a.0.min(b.0) - 0.015).max(0.0),
            y0: (a.1.min(b.1) - 0.015).max(0.0),
            x1: (a.0.max(b.0) + 0.015).min(1.0),
            y1: (a.1.max(b.1) + 0.015).min(1.0),
        })
    }

    pub(crate) fn supports_text(&self) -> bool {
        matches!(self.source, FixedDocument::Pdf(_) | FixedDocument::Djvu(_))
    }

    pub(crate) fn page_text(&self, page: usize) -> Result<PdfPageText> {
        let extracted = match &self.source {
            FixedDocument::Pdf(pdf) => pdf.page_text(page).map_err(Into::into),
            FixedDocument::Djvu(book) => book.page_text(page).map_err(Into::into),
            _ => Err("This format has no embedded text layer".into()),
        };
        if extracted
            .as_ref()
            .is_ok_and(|text| !text.text.trim().is_empty())
        {
            return extracted;
        }
        self.cached_ocr(page).map_or(extracted, Ok)
    }

    pub(crate) fn set_ocr_path(&mut self, path: PathBuf) {
        self.ocr_path = Some(path);
    }

    pub(crate) fn forget_ocr(&mut self) {
        self.last_ocr = None;
    }

    pub(crate) fn cached_ocr(&self, page: usize) -> Option<PdfPageText> {
        self.cached_ocr_for(page, None)
    }

    fn cached_ocr_for(&self, page: usize, language: Option<&str>) -> Option<PdfPageText> {
        if let Some((index, saved)) = &self.last_ocr
            && *index == page
        {
            return language
                .is_none_or(|lang| saved.language == lang)
                .then(|| saved.text.clone());
        }
        let path = self.ocr_path.as_ref()?.join(format!("{page}.json"));
        with_storage(|| {
            let meta = fs::symlink_metadata(&path)?;
            if !meta.is_file() || meta.len() > MAX_OCR_CACHE_BYTES {
                return Err(io::Error::other("Reader OCR cache is invalid"));
            }
            let mut bytes = Vec::new();
            File::open(path)?
                .take(MAX_OCR_CACHE_BYTES + 1)
                .read_to_end(&mut bytes)?;
            let saved = serde_json::from_slice::<CachedRecognition>(&bytes)
                .or_else(|_| {
                    serde_json::from_slice(&bytes).map(|text| CachedRecognition {
                        language: "eng".to_owned(),
                        text,
                    })
                })
                .map_err(io::Error::other)?;
            if bytes.len() as u64 > MAX_OCR_CACHE_BYTES || !saved.text.valid_ocr_cache() {
                return Err(io::Error::other("Reader OCR cache is invalid"));
            }
            Ok(language
                .is_none_or(|lang| lang == saved.language)
                .then_some(saved.text))
        })
        .ok()
        .flatten()
    }

    pub(crate) fn remember_ocr(
        &mut self,
        page: usize,
        language: &str,
        text: PdfPageText,
    ) -> Result<()> {
        if !text.valid_ocr_cache() {
            return Err("OCR returned invalid text positions".into());
        }
        let saved = CachedRecognition {
            language: language.to_owned(),
            text,
        };
        let Some(directory) = &self.ocr_path else {
            self.last_ocr = Some((page, saved));
            return Ok(());
        };
        let path = directory.join(format!("{page}.json"));
        let bytes = serde_json::to_vec(&saved)?;
        self.last_ocr = Some((page, saved));
        if bytes.len() as u64 > MAX_OCR_CACHE_BYTES {
            return Ok(());
        }
        let save = with_storage(|| {
            fs::create_dir_all(directory)?;
            Ok(path.is_file()
                || fs::read_dir(directory)?.take(MAX_OCR_CACHE_PAGES).count() < MAX_OCR_CACHE_PAGES)
        })?;
        if save {
            primitive_app::save(&path, &bytes)?;
        }
        Ok(())
    }

    pub(crate) fn ocr_raster(&self, page: usize) -> Result<GrayImage> {
        let size = match &self.source {
            FixedDocument::Pdf(pdf) => pdf.page_size(page)?,
            FixedDocument::Djvu(book) => book.page_size(page)?,
            _ => return Err("OCR is available for PDF and DjVu".into()),
        };
        let scale = MAX_OCR_RASTER_SIDE / size.0.max(size.1);
        let width = (size.0 * scale).round().max(1.0) as u32;
        let height = (size.1 * scale).round().max(1.0) as u32;
        let request = RenderRequest {
            width,
            height,
            scale,
            x: 0.0,
            y: 0.0,
            turns: 0,
        };
        let pixels = match &self.source {
            FixedDocument::Pdf(pdf) => pdf.render(page, request)?,
            FixedDocument::Djvu(book) => book.render(page, request)?,
            _ => unreachable!(),
        };
        Ok(GrayImage {
            width,
            height,
            pixels,
        })
    }

    pub(crate) fn select_word(&self, x: f32, y: f32) -> Result<WordAtTouch> {
        let Some((page, point)) = self
            .page_at(y)
            .and_then(|page| Some((page, self.source_point(page, x, y)?)))
        else {
            return Ok(WordAtTouch::Ready(None));
        };
        let document = if self.view.force_ocr {
            let Some(text) = self.cached_ocr_for(page, self.ocr_language.as_deref()) else {
                return Ok(WordAtTouch::Recognize { page, point });
            };
            text
        } else {
            self.page_text(page)?
        };
        Ok(WordAtTouch::Ready(PdfSelection::at(page, point, document)))
    }

    /// Use the same transformed, visible word rectangles as fixed-page highlight drawing.
    pub(crate) fn selection_bounds(&self, selection: &PdfSelection) -> Option<Rect> {
        let viewport =
            hayro::kurbo::Rect::new(0.0, 0.0, f64::from(WIDTH), f64::from(self.frame.height));
        let bounds = selection
            .bounds()
            .filter_map(|bounds| self.visible_rectangle(selection.page(), bounds))
            .map(|bounds| bounds.intersect(viewport))
            .filter(|bounds| bounds.width() > 0.0 && bounds.height() > 0.0)
            .reduce(|left, right| left.union(right))?;
        let x = bounds.x0.floor() as i32;
        let y = bounds.y0.floor() as i32;
        Some(Rect::new(
            x,
            y,
            bounds.x1.ceil() as i32 - x,
            bounds.y1.ceil() as i32 - y,
        ))
    }

    pub(crate) fn extend_selection(&self, selection: &mut PdfSelection, x: f32, y: f32) {
        if self.page_at(y) != Some(selection.page) {
            return;
        }
        if let Some(word) = self.word_at(selection.page, &selection.document, x, y) {
            let first = selection.first.min(word);
            let last = selection.last.max(word);
            if last - first < 64 {
                selection.first = first;
                selection.last = last;
            }
        }
    }

    pub(crate) fn link_at(&self, x: f32, y: f32) -> Option<PdfLink> {
        let FixedDocument::Pdf(pdf) = &self.source else {
            return None;
        };
        let page = self.page_at(y)?;
        let point = self.source_point(page, x, y)?;
        pdf.page_links(page)
            .ok()?
            .into_iter()
            .find(|link| source_rect((1.0, 1.0), link.bounds).contains(point))
    }

    fn word_at(&self, page: usize, text: &PdfPageText, x: f32, y: f32) -> Option<usize> {
        let point = self.source_point(page, x, y)?;
        text.words
            .iter()
            .position(|word| source_rect((1.0, 1.0), word.bounds).contains(point))
    }

    fn source_point(&self, index: usize, x: f32, y: f32) -> Option<Point> {
        if !(0.0..WIDTH as f32).contains(&x) || self.page_at(y) != Some(index) {
            return None;
        }
        let page = self.visible.iter().find(|p| p.index == index)?;
        let transform = page.request.page_to_viewport(page.size).ok()? * page.correction;
        let source = transform.inverse() * Point::new(f64::from(x), f64::from(y));
        let normalized = Point::new(
            source.x / f64::from(page.size.0),
            source.y / f64::from(page.size.1),
        );
        source_rect((1.0, 1.0), self.view.crop.unwrap_or(FULL_PAGE))
            .contains(normalized)
            .then_some(normalized)
    }

    fn page_at(&self, y: f32) -> Option<usize> {
        if !(0.0..self.frame.height as f32).contains(&y) {
            return None;
        }
        if self.visible.is_empty() {
            return Some(self.page);
        }
        let row = y as u32;
        self.visible
            .iter()
            .find(|page| page.rows.contains(&row))
            .map(|page| page.index)
    }

    fn projected_region(
        &self,
        index: usize,
        bounds: PdfTextRect,
    ) -> Option<(Quad, hayro::kurbo::Rect)> {
        let page = self.visible.iter().find(|p| p.index == index)?;
        let transform = page.request.page_to_viewport(page.size).ok()? * page.correction;
        let cropped = source_rect(page.size, bounds)
            .intersect(source_rect(page.size, self.view.crop.unwrap_or(FULL_PAGE)));
        if cropped.is_zero_area() {
            return None;
        }
        let quad = Quad::project(cropped, transform);
        let clip = hayro::kurbo::Rect::new(
            0.0,
            f64::from(page.rows.start),
            f64::from(WIDTH),
            f64::from(page.rows.end),
        );
        let rect = quad.bounds().intersect(clip);
        (!rect.is_zero_area()).then_some((quad, rect))
    }

    pub(crate) fn visible_rectangle(
        &self,
        index: usize,
        bounds: PdfTextRect,
    ) -> Option<hayro::kurbo::Rect> {
        self.projected_region(index, bounds).map(|(_, rect)| rect)
    }

    #[cfg(test)]
    pub(crate) fn page_viewport(&self) -> Option<((f32, f32), RenderRequest)> {
        self.visible
            .iter()
            .find(|p| p.index == self.page)
            .map(|p| (p.size, p.request))
    }

    /// The upper-left viewport position in upright scan coordinates, including any page margin.
    pub(crate) fn history_point(&self) -> Option<Point> {
        ScanPage::from_document(&self.source)?;
        let visible = self.visible.iter().find(|entry| entry.index == self.page)?;
        let transform = visible.request.page_to_viewport(visible.size).ok()? * visible.correction;
        let point = transform.inverse() * Point::ORIGIN;
        point.is_finite().then_some(point)
    }

    pub(crate) fn jump(&mut self, page: usize) -> Result<()> {
        self.jump_to(page, None)
    }

    pub(crate) fn jump_to(&mut self, page: usize, target: Option<&FixedJump>) -> Result<()> {
        if self.page == page
            && let Some(FixedJump::Regions(regions)) = target
            && regions.iter().all(|&region| {
                self.projected_region(page, region)
                    .is_some_and(|(quad, visible)| quad.bounds() == visible)
            })
        {
            return Ok(());
        }
        let mut view = View {
            offset_x: 0.0,
            offset_y: 0.0,
            ..self.view
        };
        if let (Some(target), Some(scan)) = (target, ScanPage::from_document(&self.source)) {
            let size = scan.page_size(page)?;
            let metrics = view_metrics(size, &view, self.frame.height);
            let mut request = viewport(size, &mut view, self.frame.height);
            if self.is_continuous() {
                request.y = -metrics.top;
            }
            let correction = scan.correction(page, size, &view, &mut self.deskew_cache)?;
            let transform = request.page_to_viewport(size)? * correction;
            let offset = match target {
                FixedJump::LinkPoint(point) => Point::ORIGIN - transform * *point,
                FixedJump::Regions(bounds) => {
                    let crop = source_rect(size, view.crop.unwrap_or(FULL_PAGE));
                    let mut visible = bounds.iter().filter_map(|&r| {
                        let clipped = source_rect(size, r).intersect(crop);
                        (!clipped.is_zero_area()).then(|| transform.transform_rect_bbox(clipped))
                    });
                    let first = visible.next().ok_or(
                        "This passage is outside the current crop. Reset the crop to view it.",
                    )?;
                    let all = visible.fold(first, |all, rect| all.union(rect));
                    // A long or multi-column passage can exceed the viewport on one axis.
                    let center = Point::new(
                        if all.width() <= f64::from(WIDTH) {
                            all.center().x
                        } else {
                            first.center().x
                        },
                        if all.height() <= f64::from(self.frame.height) {
                            all.center().y
                        } else {
                            first.center().y
                        },
                    );
                    Point::new(f64::from(WIDTH) * 0.5, f64::from(self.frame.height) * 0.5) - center
                }
            };
            view.offset_x = offset.x as f32;
            view.offset_y = offset.y as f32;
            if self.is_continuous() {
                let last_pixel = (metrics.height.round() - 1.0).max(0.0);
                view.offset_y = view.offset_y.clamp(-last_pixel, 0.0);
            }
        } else if view.width_fit && !self.is_continuous() {
            view.offset_y = f32::MAX;
        }
        self.present(page, view)
    }

    pub(crate) fn fit(&mut self, width_fit: bool) -> Result<()> {
        self.apply_fit(if width_fit { Fit::Width } else { Fit::Page })
    }

    pub(crate) fn fit_height(&mut self) -> Result<()> {
        self.apply_fit(Fit::Height)
    }

    fn apply_fit(&mut self, fit: Fit) -> Result<()> {
        let width_fit = matches!(fit, Fit::Width);
        self.present(
            self.page,
            View {
                turns: self.view.turns,
                width_fit,
                height_fit: matches!(fit, Fit::Height),
                continuous: width_fit && self.view.continuous,
                spread: self.view.spread,
                scan_contrast: self.view.scan_contrast,
                dither: self.view.dither,
                continuous_gap: self.view.continuous_gap,
                scan_white_level: self.view.scan_white_level,
                straighten_limit: self.view.straighten_limit,
                force_ocr: self.view.force_ocr,
                offset_y: if width_fit && !self.is_continuous() {
                    f32::MAX
                } else {
                    0.0
                },
                right_to_left: self.view.right_to_left,
                epub_spread: self.view.epub_spread,
                epub_right_to_left: self.view.epub_right_to_left,
                crop: self.view.crop,
                ..View::default()
            },
        )?;
        self.pause();
        Ok(())
    }

    pub(crate) fn rotate(&mut self) -> Result<()> {
        self.set_rotation((self.view.turns + 1) % 4)
    }

    pub(crate) fn set_rotation(&mut self, turns: u8) -> Result<()> {
        self.present(
            self.page,
            View {
                turns: turns % 4,
                offset_x: 0.0,
                offset_y: 0.0,
                ..self.view
            },
        )
    }

    pub(crate) fn pause(&mut self) {
        self.turns = PageTurns::new(Instant::now());
        self.pending_scroll = (0.0, 0.0);
        self.end_attempt = false;
    }

    pub(crate) fn take_end_attempt(&mut self) -> bool {
        std::mem::take(&mut self.end_attempt)
    }

    pub(crate) fn deadline(&self) -> Option<Instant> {
        if self.is_continuous() && self.pending_scroll != (0.0, 0.0) {
            Some(self.turns.next_refresh)
        } else {
            self.turns.deadline()
        }
    }

    pub(crate) fn tick(&mut self, now: Instant) -> Result<bool> {
        if self.is_continuous() {
            if now < self.turns.next_refresh || self.pending_scroll == (0.0, 0.0) {
                return Ok(false);
            }
            let (dx, dy) = std::mem::take(&mut self.pending_scroll);
            let previous = (self.page, self.view);
            let mut view = self.view;
            view.offset_x += dx;
            view.offset_y += dy;
            self.present(self.page, view)?;
            self.end_attempt = dy < -0.5
                && previous.0 == self.page
                && previous.1.offset_y == self.view.offset_y
                && self.visible.last().is_some_and(|page| page.index + 1 == self.count());
            return Ok(previous != (self.page, self.view));
        }
        let count = self.count();
        let mut page = self.page;
        let mut end_attempt = false;
        for step in self.turns.take_due(now) {
            let next = if self.is_spread() {
                self.epub_spreads.as_ref().map_or_else(
                    || comic_spread_step(page, count, step),
                    |epub| epub.plan.step(page, step),
                )
            } else {
                page.saturating_add_signed(step).min(count - 1)
            };
            end_attempt = step > 0 && next == page;
            page = next;
        }
        if page == self.page {
            self.end_attempt = end_attempt;
            return Ok(false);
        }
        self.jump(page)?;
        self.end_attempt = end_attempt;
        Ok(true)
    }

    pub(crate) fn input(&mut self, input: Input) -> Result<bool> {
        self.input_with_status(input, true)
    }

    pub(crate) fn input_with_status(&mut self, input: Input, show_zoom: bool) -> Result<bool> {
        let horizontal = if self.view.right_to_left { -1 } else { 1 };
        let direction = match input {
            Input::Tap(x, y) if show_zoom && SMALLER.contains(x, y) => return self.zoom(-1),
            Input::Tap(x, y) if show_zoom && LARGER.contains(x, y) => return self.zoom(1),
            Input::PageForward => 1,
            Input::PageBack => -1,
            Input::Swipe { dx, dy, .. } if self.is_continuous() => {
                return self.queue_scroll(dx, dy);
            }
            Input::Swipe { dx, dy, .. }
                if !self.is_spread()
                    && (self.view.zoom > 0
                        || self.view.width_fit
                        || (self.view.height_fit && dx.abs() >= dy.abs())) =>
            {
                let (x, y) = (self.view.offset_x, self.view.offset_y);
                self.present(
                    self.page,
                    View {
                        offset_x: x + dx,
                        offset_y: y + dy,
                        ..self.view
                    },
                )?;
                return Ok(true);
            }
            Input::Swipe { dx, dy, .. } => {
                let displacement = if dx.abs() >= dy.abs() {
                    dx * horizontal as f32
                } else {
                    dy
                };
                if displacement < 0.0 { 1 } else { -1 }
            }
            Input::Tap(x, y) if y < self.frame.height as f32 && x < WIDTH as f32 / 3.0 => {
                -horizontal
            }
            Input::Tap(x, y) if y < self.frame.height as f32 && x > WIDTH as f32 * 2.0 / 3.0 => {
                horizontal
            }
            _ => return Ok(false),
        };
        if self.is_continuous() {
            return self.queue_scroll(0.0, -(direction as f32) * (self.frame.height as f32 * 0.9));
        }
        self.turns.pending.push(direction);
        self.tick(Instant::now())
    }

    fn queue_scroll(&mut self, dx: f32, dy: f32) -> Result<bool> {
        if dx.is_finite() && dy.is_finite() {
            let limit = self.frame.height as f32 * 8.0;
            self.pending_scroll.0 = (self.pending_scroll.0 + dx).clamp(-limit, limit);
            self.pending_scroll.1 = (self.pending_scroll.1 + dy).clamp(-limit, limit);
        }
        self.tick(Instant::now())
    }

    fn zoom(&mut self, delta: isize) -> Result<bool> {
        self.set_zoom(
            self.view
                .zoom
                .saturating_add_signed(delta)
                .min(ZOOM.len() - 1),
        )
    }

    pub(crate) fn set_zoom(&mut self, zoom: usize) -> Result<bool> {
        if self.is_spread() || zoom == self.view.zoom {
            return Ok(false);
        }
        if zoom >= ZOOM.len() {
            return Err("That zoom level is unavailable".into());
        }
        let mut view = View { zoom, ..self.view };
        if self.is_continuous()
            && let FixedDocument::Pdf(pdf) = &self.source
        {
            let size = pdf.page_size(self.page)?;
            let old_height = view_metrics(size, &self.view, self.frame.height).height;
            let new_height = view_metrics(size, &view, self.frame.height).height;
            let old_boundary = old_height.round().max(1.0);
            if self.view.continuous_gap != 0
                && -view.offset_y >= old_boundary
                && self.page + 1 < self.count()
            {
                // The gap is measured in display pixels and does not scale with the document.
                view.offset_y += old_boundary - new_height.round().max(1.0);
            } else {
                view.offset_y *= new_height / old_height;
            }
        }
        self.present(self.page, view)?;
        Ok(true)
    }

    fn present(&mut self, page: usize, view: View) -> Result<()> {
        self.present_in(page, view, self.frame.height, || Ok(()))
    }

    pub(crate) fn resize(
        &mut self,
        viewport_height: u32,
        save: impl FnOnce() -> io::Result<()>,
    ) -> Result<()> {
        let mut view = self.view;
        if !view.spread && (view.offset_x != 0.0 || view.offset_y != 0.0) {
            let size = match &self.source {
                FixedDocument::Pdf(pdf) => pdf.page_size(self.page)?,
                FixedDocument::Djvu(book) => book.page_size(self.page)?,
                FixedDocument::Epub(book) => with_storage(|| {
                    book.decode_page(self.page)
                        .map(|image| image.page_size())
                        .map_err(io::Error::other)
                })?,
                FixedDocument::Comic(book) => with_storage(|| {
                    book.decode_page(self.page)
                        .map(|image| image.page_size())
                        .map_err(io::Error::other)
                })?,
                FixedDocument::Image(image) => image.page_size(),
            };
            let old = view_metrics(size, &view, self.frame.height);
            let new = view_metrics(size, &view, viewport_height);
            let ratio = new.scale / old.scale;
            view.offset_x *= ratio;
            if view.continuous
                && view.continuous_gap > 0
                && -view.offset_y >= old.height.round()
                && self.page + 1 < self.count()
            {
                view.offset_y += old.height.round() - new.height.round();
            } else {
                view.offset_y *= ratio;
            }
        }
        self.present_in(self.page, view, viewport_height, save)
    }

    fn present_in(
        &mut self,
        page: usize,
        view: View,
        viewport_height: u32,
        save: impl FnOnce() -> io::Result<()>,
    ) -> Result<()> {
        let new_epub_spreads = if viewport_height != self.frame.height
            || view.epub_spread != self.view.epub_spread
            || view.epub_right_to_left != self.view.epub_right_to_left
        {
            self.source.epub_spreads(view, viewport_height)
        } else {
            None
        };
        let spreads = new_epub_spreads.as_ref().or(self.epub_spreads.as_ref());
        let (page, view, frame, visible) = self.source.presentation(
            page,
            view,
            spreads,
            &mut self.deskew_cache,
            viewport_height,
        )?;
        save()?;
        if new_epub_spreads.is_some() {
            self.epub_spreads = new_epub_spreads;
        }
        self.frame = frame;
        self.visible = visible;
        if self.page != page && let FixedDocument::Djvu(book) = &mut self.source {
            book.set_outline_page(page);
        }
        self.page = page;
        self.view = view;
        self.turns.next_refresh = Instant::now() + PAGE_REFRESH_GAP;
        Ok(())
    }

    fn display_frame(&self, rows: Range<u32>) -> GrayImage {
        let mut frame = self.frame.clone();
        let contrast = self
            .scan_contrast()
            .then(|| scan_contrast_table(&self.frame.pixels))
            .flatten();
        let white_level = self.view.scan_white_level;
        if contrast.is_none() && white_level.is_none() && !self.view.dither {
            return frame;
        }
        let tones: [u8; 256] = std::array::from_fn(|gray| {
            let gray = contrast.as_ref().map_or(gray as u8, |table| table[gray]);
            white_level.map_or(gray, |white| scale_scan_white(gray, white))
        });
        let width = WIDTH as usize;
        let start = rows.start as usize;
        let pixels = &mut frame.pixels[start * width..rows.end as usize * width];
        if self.view.dither {
            for (y, row) in pixels.chunks_exact_mut(width).enumerate() {
                for (x, pixel) in row.iter_mut().enumerate() {
                    *pixel = ordered_gray(tones[usize::from(*pixel)], x, start + y);
                }
            }
        } else {
            for pixel in pixels {
                *pixel = tones[usize::from(*pixel)];
            }
        }
        frame
    }

    pub(crate) fn draw(
        &self,
        canvas: &mut Canvas,
        notice: Option<&str>,
        marks: &[(usize, PdfTextRect)],
        selection: &[(usize, PdfTextRect)],
        paint: PagePaint,
    ) {
        let transient: Vec<_> = marks
            .iter()
            .copied()
            .map(|r| (r, crate::highlight_style::Paint::Temporary(220)))
            .collect();
        self.draw_with_footer(canvas, notice, &transient, selection, paint, false);
    }

    pub(crate) fn page_status(&self) -> String {
        let physical = self.page + 1;
        let zoom = (ZOOM[self.view.zoom] * 100.0) as u32;
        if self.is_spread() {
            match self.spread_pages() {
                [Some(first), Some(last)] => {
                    format!("Pages {}–{} of {}", first + 1, last + 1, self.count())
                }
                _ => format!("Page {physical} of {}", self.count()),
            }
        } else {
            match self.page_label(self.page) {
                Some(label) if label != physical.to_string() => {
                    format!("Page {label}  ·  {physical}/{}  ·  {zoom}%", self.count())
                }
                _ => format!("Page {physical} of {}  ·  {zoom}%", self.count()),
            }
        }
    }

    pub(crate) fn draw_with_footer(
        &self,
        canvas: &mut Canvas,
        notice: Option<&str>,
        marks: &[((usize, PdfTextRect), crate::highlight_style::Paint)],
        selection: &[(usize, PdfTextRect)],
        paint: PagePaint,
        hidden_status: bool,
    ) {
        let mut frame = self.display_frame(paint.visible.clone());
        let ink = marks.iter().copied().chain(
            selection
                .iter()
                .copied()
                .map(|r| (r, crate::highlight_style::Paint::Temporary(180))),
        );
        for ((index, region), paint_style) in ink {
            let Some((quad, clip)) = self.projected_region(index, region) else {
                continue;
            };
            let bounds = quad.bounds();
            if let Some((y, height)) = paint_style.line_band(
                bounds.y0.floor() as i32,
                bounds.height().ceil() as i32,
                WIDTH,
                true,
            ) {
                let page = self
                    .visible
                    .iter()
                    .find(|page| page.index == index)
                    .unwrap();
                let x0 = clip.x0.floor().max(0.) as usize;
                let x1 = clip.x1.ceil().min(f64::from(WIDTH)) as usize;
                let first = y.max(page.rows.start as i32).max(0) as usize;
                let end = (y + height).min(page.rows.end as i32).max(0) as usize;
                for row in first..end {
                    let start = row * WIDTH as usize;
                    paint_style.draw_row(&mut frame.pixels[start + x0..start + x1]);
                }
            } else {
                for y in clip.y0.floor() as usize..clip.y1.ceil() as usize {
                    let span = quad.row(y, WIDTH as usize);
                    let start = y * WIDTH as usize;
                    paint_style.draw_row(&mut frame.pixels[start + span.start..start + span.end]);
                }
            }
        }
        let Some(full) =
            draw_document_and_footer(canvas, frame, paint, hidden_status && selection.is_empty())
        else {
            return;
        };
        if !selection.is_empty() {
            return;
        }
        let label = self.page_status();
        canvas.text(
            Rect::new(38, PAGE_HEIGHT as i32 + 12, SMALLER.x - 60, 48),
            notice.unwrap_or(&label),
            CAPTION,
        );
        if !full {
            return;
        }
        if !self.is_spread() {
            canvas.button(SMALLER, "-", false, false);
            canvas.button(LARGER, "+", false, false);
        }
        canvas.button(DONE, "Exit", false, false);
    }
}

impl FixedDocument {
    fn count(&self) -> usize {
        match self {
            Self::Epub(book) => book.page_count(),
            Self::Djvu(book) => book.page_count(),
            Self::Pdf(pdf) => pdf.page_count(),
            Self::Comic(comic) => comic.page_count(),
            Self::Image(_) => 1,
        }
    }

    fn supports_comic_spreads(&self) -> bool {
        matches!(self, Self::Comic(comic) if comic.page_count() > 1)
    }

    fn epub_spreads(&self, view: View, viewport_height: u32) -> Option<EpubSpreads> {
        let Self::Epub(book) = self else { return None };
        // Legacy explicit RTL selections still apply. An unset override otherwise follows the book.
        let right_to_left = view
            .epub_right_to_left
            .unwrap_or_else(|| view.right_to_left || book.right_to_left());
        let landscape = WIDTH > viewport_height;
        let plan = book.spread_plan(view.epub_spread, right_to_left, landscape);
        let available = plan.has_facing()
            || (view.epub_spread != Some(true)
                && book
                    .spread_plan(Some(true), right_to_left, landscape)
                    .has_facing());
        Some(EpubSpreads {
            plan,
            available,
            right_to_left,
        })
    }

    fn supports_scan_contrast(&self) -> bool {
        matches!(self, Self::Pdf(_) | Self::Djvu(_) | Self::Comic(_))
    }

    fn presentation(
        &self,
        page: usize,
        mut view: View,
        epub_spreads: Option<&EpubSpreads>,
        cache: &mut DeskewCache,
        viewport_height: u32,
    ) -> Result<(usize, View, GrayImage, Vec<VisiblePage>)> {
        if let Some(epub) = epub_spreads {
            view.spread = epub.plan.has_facing();
            view.right_to_left = epub.right_to_left;
        } else {
            view.spread &= self.supports_comic_spreads();
        }
        if !self.supports_scan_contrast() {
            view.scan_contrast = false;
            view.scan_white_level = None;
        }
        if view.continuous
            && view.width_fit
            && let Self::Pdf(pdf) = self
        {
            return continuous_frame(pdf, page, view, cache, viewport_height);
        }
        view.continuous = false;
        let page = if view.spread && epub_spreads.is_none() {
            comic_spread_start(page)
        } else {
            page
        };
        if let Some(scan) = ScanPage::from_document(self) {
            let size = scan.page_size(page)?;
            let request = viewport(size, &mut view, viewport_height);
            let correction = scan.correction(page, size, &view, cache)?;
            let pixels = scan.render(page, request, correction)?;
            let pixels = mask_crop(pixels, view.crop, size, request, correction)?;
            let frame = GrayImage {
                width: WIDTH,
                height: viewport_height,
                pixels,
            };
            let visible = VisiblePage {
                index: page,
                size,
                rows: 0..viewport_height,
                request,
                correction,
            };
            return Ok((page, view, frame, vec![visible]));
        }
        view.straighten_limit = 0;
        view.force_ocr = false;
        let frame = self.frame(page, &mut view, epub_spreads, viewport_height)?;
        Ok((page, view, frame, Vec::new()))
    }

    fn frame(
        &self,
        page: usize,
        view: &mut View,
        epub_spreads: Option<&EpubSpreads>,
        viewport_height: u32,
    ) -> Result<GrayImage> {
        let pixels = match self {
            Self::Epub(book) => with_storage(|| {
                if view.spread
                    && let Some(spread) = epub_spreads.and_then(|epub| epub.plan.group(page))
                {
                    return book
                        .compose_spread(spread, WIDTH, viewport_height, view.turns)
                        .map_err(io::Error::other);
                }
                let image = book.decode_page(page).map_err(io::Error::other)?;
                image
                    .render(viewport(image.page_size(), view, viewport_height))
                    .map_err(io::Error::other)
            })?,
            Self::Pdf(_) | Self::Djvu(_) => unreachable!("scans use their page transform"),
            Self::Comic(comic) if view.spread => {
                comic_spread_pixels(comic, page, *view, viewport_height)?
            }
            Self::Comic(comic) => {
                let image = with_storage(|| comic.decode_page(page).map_err(io::Error::other))?;
                image.render(viewport(image.page_size(), view, viewport_height))?
            }
            Self::Image(image) => {
                image.render(viewport(image.page_size(), view, viewport_height))?
            }
        };
        Ok(GrayImage {
            width: WIDTH,
            height: viewport_height,
            pixels,
        })
    }
}

fn scale_scan_white(gray: u8, white: u8) -> u8 {
    let white = u16::from(white);
    ((u16::from(gray) * 255 + white / 2) / white).min(255) as u8
}

fn ordered_gray(gray: u8, x: usize, y: usize) -> u8 {
    const BAYER4: [[u16; 4]; 4] = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];
    let level = gray / 17;
    let residual = u16::from(gray % 17) * 32;
    let threshold = (BAYER4[y % 4][x % 4] * 2 + 1) * 17;
    (level + u8::from(residual > threshold)) * 17
}

fn scan_contrast_table(pixels: &[u8]) -> Option<[u8; 256]> {
    let stride = pixels.len().div_ceil(MAX_CONTRAST_SAMPLES).max(1);
    let mut histogram = [0; 256];
    for &gray in pixels.iter().step_by(stride) {
        histogram[usize::from(gray)] += 1;
    }
    // Treat any white margins separately, since they can surround a gray scan.
    let white_pixels = std::mem::take(&mut histogram[255]);
    let samples = histogram.iter().sum::<usize>();
    if samples == 0 {
        return None;
    }
    let ink = gray_percentile(&histogram, (samples * 2).div_ceil(100));
    let paper = gray_percentile(&histogram, (samples * 98).div_ceil(100));
    let paper = if paper - ink < 8 && white_pixels > 0 {
        255
    } else {
        paper
    };
    if paper - ink < 8 {
        return None;
    }

    let black = u16::from(ink.min(112));
    let white = u16::from(paper.max(216));
    let range = white - black;
    Some(std::array::from_fn(|gray| {
        let gray = gray as u16;
        let relative = gray.saturating_sub(black).min(range);
        let stretched = (relative * 255 + range / 2) / range;
        // Preserve most of the original tone so pale ink retains visible separation.
        ((gray * 3 + stretched * 2 + 2) / 5) as u8
    }))
}

fn gray_percentile(histogram: &[usize; 256], rank: usize) -> u8 {
    let mut cumulative = 0;
    histogram
        .iter()
        .position(|count| {
            cumulative += count;
            cumulative >= rank
        })
        .unwrap_or(255) as u8
}

fn comic_spread_start(page: usize) -> usize {
    if page == 0 { 0 } else { (page - 1) / 2 * 2 + 1 }
}

fn comic_spread_partner(page: usize, count: usize) -> Option<usize> {
    (page > 0 && page + 1 < count).then_some(page + 1)
}

fn comic_spread_step(page: usize, count: usize, direction: isize) -> usize {
    if direction < 0 {
        page.saturating_sub(2)
    } else if page == 0 {
        1
    } else {
        (page + 2).min(comic_spread_start(count - 1))
    }
}

fn comic_spread_pixels(
    comic: &Comic,
    page: usize,
    view: View,
    viewport_height: u32,
) -> Result<Vec<u8>> {
    Ok(with_storage(|| {
        let fit = |index, width| {
            comic
                .decode_page(index)
                .and_then(|image| image.fit(width, viewport_height, view.turns))
                .map_err(io::Error::other)
        };
        let Some(partner) = comic_spread_partner(page, comic.page_count()) else {
            return fit(page, WIDTH);
        };
        let halves = if view.right_to_left {
            [partner, page]
        } else {
            [page, partner]
        };
        let first_width = (WIDTH - SPREAD_GUTTER) / 2;
        let second_x = first_width + SPREAD_GUTTER;
        let mut pixels = vec![255; (WIDTH * viewport_height) as usize];
        for (index, x, width) in [
            (halves[0], 0, first_width),
            (halves[1], second_x, WIDTH - second_x),
        ] {
            let raster = fit(index, width)?;
            let occupied = x as usize..(x + width) as usize;
            for (row, half_row) in pixels
                .as_chunks_mut::<{ WIDTH as usize }>()
                .0
                .iter_mut()
                .zip(raster.chunks_exact(width as usize))
            {
                row[occupied.clone()].copy_from_slice(half_row);
            }
        }
        Ok(pixels)
    })?)
}

fn continuous_frame(
    pdf: &Pdf,
    page: usize,
    view: View,
    cache: &mut DeskewCache,
    viewport_height: u32,
) -> Result<(usize, View, GrayImage, Vec<VisiblePage>)> {
    let (page, view) = continuous_position(pdf, page, view, viewport_height)?;
    let mut pixels = vec![255; (WIDTH * viewport_height) as usize];
    let mut visible = Vec::new();
    let mut top = view.offset_y as i32;
    let last = pdf.page_count() - 1;
    for index in page..=last {
        if top >= viewport_height as i32 || visible.len() == MAX_VISIBLE_PDF_PAGES {
            break;
        }
        let (size, metrics, height) = continuous_metrics(pdf, index, &view, viewport_height)?;
        let bottom = top + height;
        if bottom <= 0 {
            top = bottom + continuous_gap_after(index, last, &view);
            continue;
        }
        let rows = top.max(0) as u32..bottom.min(viewport_height as i32) as u32;
        let offset = clamp_offset(view.offset_x, metrics.width, WIDTH as f32);
        let request = RenderRequest {
            width: WIDTH,
            height: viewport_height,
            scale: metrics.scale,
            x: (WIDTH as f32 - metrics.width) / 2.0 - metrics.left + offset,
            y: top as f32 - metrics.top,
            turns: view.turns,
        };
        let slice = RenderRequest {
            height: rows.end - rows.start,
            y: request.y - rows.start as f32,
            ..request
        };
        let scan = ScanPage::Pdf(pdf);
        let correction = scan.correction(index, size, &view, cache)?;
        let raster = mask_crop(
            scan.render(index, slice, correction)?,
            view.crop,
            size,
            slice,
            correction,
        )?;
        pixels[(rows.start * WIDTH) as usize..(rows.end * WIDTH) as usize].copy_from_slice(&raster);
        visible.push(VisiblePage {
            index,
            size,
            rows,
            request,
            correction,
        });
        top = bottom + continuous_gap_after(index, last, &view);
    }
    let image = GrayImage {
        width: WIDTH,
        height: viewport_height,
        pixels,
    };
    Ok((page, view, image, visible))
}

fn continuous_position(
    pdf: &Pdf,
    mut page: usize,
    mut view: View,
    viewport_height: u32,
) -> Result<(usize, View)> {
    let last = pdf.page_count() - 1;
    let length = |index| -> Result<i32> {
        let (_, _, height) = continuous_metrics(pdf, index, &view, viewport_height)?;
        Ok(height + continuous_gap_after(index, last, &view))
    };
    let mut offset = (-view.offset_y.round()) as i32;
    while offset < 0 && page > 0 {
        page -= 1;
        offset += length(page)?;
    }
    let mut span = length(page)?;
    while page < last && offset >= span {
        offset -= span;
        page += 1;
        span = length(page)?;
    }
    offset = offset.clamp(0, span);

    // Keep the last page flush with the bottom when a full viewport remains.
    let mut available = span - offset;
    for index in page + 1..=last {
        if available >= viewport_height as i32 {
            break;
        }
        available += length(index)?;
    }
    let mut deficit = (viewport_height as i32 - available).max(0);
    while deficit > offset && page > 0 {
        deficit -= offset;
        page -= 1;
        offset = length(page)?;
    }
    let (_, metrics, _) = continuous_metrics(pdf, page, &view, viewport_height)?;
    view.offset_y = -((offset - deficit).max(0) as f32);
    view.offset_x = clamp_offset(view.offset_x, metrics.width, WIDTH as f32);
    Ok((page, view))
}

fn continuous_gap_after(page: usize, last: usize, view: &View) -> i32 {
    if page < last {
        i32::from(view.continuous_gap)
    } else {
        0
    }
}

fn continuous_metrics(
    pdf: &Pdf,
    page: usize,
    view: &View,
    viewport_height: u32,
) -> Result<((f32, f32), ViewMetrics, i32)> {
    let size = pdf.page_size(page)?;
    let metrics = view_metrics(size, view, viewport_height);
    if !metrics.height.is_finite() || metrics.height <= 0.0 {
        return Err("This PDF page has unsupported dimensions".into());
    }
    let height = metrics.height.round().max(1.0) as i32;
    Ok((size, metrics, height))
}

#[derive(Clone, Copy)]
enum ScanPage<'a> {
    Pdf(&'a Pdf),
    Djvu(&'a Djvu),
}

impl<'a> ScanPage<'a> {
    fn from_document(source: &'a FixedDocument) -> Option<Self> {
        match source {
            FixedDocument::Pdf(pdf) => Some(Self::Pdf(pdf)),
            FixedDocument::Djvu(book) => Some(Self::Djvu(book)),
            _ => None,
        }
    }

    fn page_size(self, page: usize) -> Result<(f32, f32)> {
        match self {
            Self::Pdf(pdf) => Ok(pdf.page_size(page)?),
            Self::Djvu(book) => Ok(book.page_size(page)?),
        }
    }

    fn render(self, page: usize, request: RenderRequest, correction: Affine) -> Result<Vec<u8>> {
        match self {
            Self::Pdf(pdf) => Ok(pdf.render_transformed(page, request, correction)?),
            Self::Djvu(book) => Ok(book.render_transformed(page, request, correction)?),
        }
    }

    fn correction(
        self,
        page: usize,
        size: (f32, f32),
        view: &View,
        cache: &mut DeskewCache,
    ) -> Result<Affine> {
        cache.correction(page, size, view.straighten_limit, view.crop, |request| {
            self.render(page, request, Affine::IDENTITY)
        })
    }
}

fn mask_crop(
    mut pixels: Vec<u8>,
    crop: Option<PdfTextRect>,
    size: (f32, f32),
    request: RenderRequest,
    correction: Affine,
) -> Result<Vec<u8>> {
    if let Some(crop) = crop {
        let quad = Quad::project(
            source_rect(size, crop),
            request.page_to_viewport(size)? * correction,
        );
        for (y, row) in pixels.chunks_exact_mut(request.width as usize).enumerate() {
            let span = quad.row(y, row.len());
            row[..span.start].fill(255);
            row[span.end..].fill(255);
        }
    }
    Ok(pixels)
}

struct ViewMetrics {
    scale: f32,
    width: f32,
    height: f32,
    left: f32,
    top: f32,
}

fn view_metrics(
    (mut width, mut height): (f32, f32),
    view: &View,
    viewport_height: u32,
) -> ViewMetrics {
    if !view.turns.is_multiple_of(2) {
        (width, height) = (height, width);
    }
    let viewport_width = WIDTH as f32;
    let height_px = viewport_height as f32;
    let (left, top, right, bottom) = view
        .crop
        .map(|r| match view.turns % 4 {
            0 => (r.x0, r.y0, r.x1, r.y1),
            1 => (1.0 - r.y1, r.x0, 1.0 - r.y0, r.x1),
            2 => (1.0 - r.x1, 1.0 - r.y1, 1.0 - r.x0, 1.0 - r.y0),
            _ => (r.y0, 1.0 - r.x1, r.y1, 1.0 - r.x0),
        })
        .unwrap_or((0.0, 0.0, 1.0, 1.0));
    let crop_width = width * (right - left);
    let crop_height = height * (bottom - top);
    let width_scale = viewport_width / crop_width;
    let height_scale = height_px / crop_height;
    let fit = if view.width_fit {
        width_scale
    } else if view.height_fit {
        height_scale
    } else {
        width_scale.min(height_scale)
    };
    let scale = (fit * ZOOM[view.zoom]).min(24_000.0 / width.max(height));
    ViewMetrics {
        scale,
        width: crop_width * scale,
        height: crop_height * scale,
        left: left * width * scale,
        top: top * height * scale,
    }
}

fn clamp_offset(offset: f32, projected: f32, viewport: f32) -> f32 {
    let limit = ((projected - viewport) / 2.0).max(0.0);
    offset.clamp(-limit, limit)
}

fn viewport(size: (f32, f32), view: &mut View, viewport_height: u32) -> RenderRequest {
    let metrics = view_metrics(size, view, viewport_height);
    let viewport_width = WIDTH as f32;
    let height_px = viewport_height as f32;
    view.offset_x = clamp_offset(view.offset_x, metrics.width, viewport_width);
    view.offset_y = clamp_offset(view.offset_y, metrics.height, height_px);
    RenderRequest {
        width: WIDTH,
        height: viewport_height,
        scale: metrics.scale,
        x: (viewport_width - metrics.width) / 2.0 - metrics.left + view.offset_x,
        y: (height_px - metrics.height) / 2.0 - metrics.top + view.offset_y,
        turns: view.turns,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixed::PdfLinkTarget;
    use std::io::Write;
    use std::sync::atomic::{AtomicUsize, Ordering};

    impl WordAtTouch {
        fn ready(self) -> Option<PdfSelection> {
            match self {
                Self::Ready(word) => word,
                Self::Recognize { .. } => panic!("unexpected OCR request"),
            }
        }
    }

    struct ComicFixture(PathBuf);

    impl ComicFixture {
        fn panels() -> Self {
            use comic_panel::tests::{HEIGHT, WIDTH, grid};

            let fixture = Self(
                std::env::temp_dir().join(format!("reader-panel-test-{}.cbz", std::process::id(),)),
            );
            let mut archive = zip::ZipWriter::new(File::create(&fixture.0).unwrap());
            let pixels = grid();
            for index in 0..2 {
                archive
                    .start_file(
                        format!("{index}.pgm"),
                        zip::write::SimpleFileOptions::default(),
                    )
                    .unwrap();
                write!(archive, "P5\n{WIDTH} {HEIGHT}\n255\n").unwrap();
                archive.write_all(&pixels).unwrap();
            }
            archive.finish().unwrap();
            fixture
        }

        fn striped() -> Self {
            let fixture = Self(
                std::env::temp_dir().join(format!("reader-spread-test-{}.cbz", std::process::id())),
            );
            let mut archive = zip::ZipWriter::new(File::create(&fixture.0).unwrap());
            for (index, intensity) in [32, 64, 128, 192].into_iter().enumerate() {
                archive
                    .start_file(
                        format!("{index}.pgm"),
                        zip::write::SimpleFileOptions::default(),
                    )
                    .unwrap();
                archive.write_all(b"P5\n8 16\n255\n").unwrap();
                archive.write_all(&[intensity; 128]).unwrap();
            }
            archive.finish().unwrap();
            fixture
        }
    }

    impl Drop for ComicFixture {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }

    #[test]
    fn panel_zoom_maps_a_rotated_and_panned_touch_without_changing_the_comic_pose() {
        use comic_panel::tests::{HEIGHT as SOURCE_HEIGHT, WIDTH as SOURCE_WIDTH};

        let fixture = ComicFixture::panels();
        let comic = Comic::open(&fixture.0).unwrap();
        let original = FixedReading::with_source(
            FixedDocument::Comic(comic),
            1,
            View {
                zoom: 1,
                turns: 1,
                offset_x: 85.0,
                ..View::default()
            },
        )
        .unwrap();
        let size = (SOURCE_WIDTH as f32, SOURCE_HEIGHT as f32);
        let mut preferences = original.preferences();
        let transform = viewport(size, &mut preferences, PAGE_HEIGHT)
            .page_to_viewport(size)
            .unwrap();
        let touch = transform * hayro::kurbo::Point::new(90.0, 106.0);
        let before = original.frame.pixels.clone();
        let panel = original
            .inspect_comic_panel(touch.x as f32, touch.y as f32)
            .unwrap()
            .unwrap();
        let gray = panel.frame.pixels[((PAGE_HEIGHT / 2) * WIDTH + WIDTH / 2) as usize];
        assert_eq!(gray, 197);
        assert_eq!(panel.preferences().turns, 1);
        assert_eq!(original.page(), 1);
        assert_eq!(original.preferences(), preferences);
        assert_eq!(original.frame.pixels, before);

        let gutter = transform * hayro::kurbo::Point::new(160.0, 220.0);
        assert!(
            original
                .inspect_comic_panel(gutter.x as f32, gutter.y as f32)
                .unwrap()
                .is_none()
        );
    }

    struct EpubFixture(PathBuf);

    impl EpubFixture {
        fn striped(spread: &str) -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!(
                "reader-fixed-spread-{}-{}.epub",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let file = File::options()
                .create_new(true)
                .write(true)
                .open(&path)
                .unwrap();
            let fixture = Self(path);
            let mut archive = zip::ZipWriter::new(file);
            let mut add = |name: &str, data: &str| {
                archive
                    .start_file(
                        name,
                        zip::write::SimpleFileOptions::default()
                            .compression_method(zip::CompressionMethod::Stored),
                    )
                    .unwrap();
                archive.write_all(data.as_bytes()).unwrap();
            };
            add("mimetype", "application/epub+zip");
            add(
                "META-INF/container.xml",
                r#"<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container" version="1.0">
                    <rootfiles><rootfile full-path="book.opf" media-type="application/oebps-package+xml"/></rootfiles>
                   </container>"#,
            );
            let mut manifest = String::new();
            let mut spine = String::new();
            for (index, (properties, tone)) in [
                ("page-spread-left", 32),
                ("rendition:page-spread-right", 64),
                ("page-spread-left", 128),
                ("rendition:page-spread-center", 96),
                ("", 160),
                ("", 192),
            ]
            .into_iter()
            .enumerate()
            {
                manifest.push_str(&format!(
                    r#"<item id="p{index}" href="p{index}.xhtml" media-type="application/xhtml+xml"/>"#,
                ));
                spine.push_str(&format!(
                    r#"<itemref idref="p{index}" properties="{properties}"/>"#
                ));
                add(
                    &format!("p{index}.xhtml"),
                    &format!(
                        r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Page</title>
                        <meta name="viewport" content="width=80,height=120"/></head><body>
                        <div style="position:absolute;left:0;top:0;width:80px;height:120px;background:rgb({tone},{tone},{tone})"/>
                        </body></html>"#,
                    ),
                );
            }
            add(
                "book.opf",
                &format!(
                    r#"<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
                    <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
                    <dc:title>Spread Tests</dc:title><meta property="rendition:layout">pre-paginated</meta>
                    <meta property="rendition:spread">{spread}</meta></metadata>
                    <manifest>{manifest}</manifest><spine page-progression-direction="rtl">{spine}</spine></package>"#,
                ),
            );
            archive.finish().unwrap();
            fixture
        }

        fn open(&self, page: usize, view: View) -> FixedReading {
            let fonts = [crate::test_fixture("fonts/Silkscreen-Regular.ttf")];
            FixedReading::open(
                PAGE_HEIGHT,
                DocumentFormat::Epub,
                &self.0,
                page,
                &fonts,
                view,
                None,
                None,
            )
            .unwrap()
            .unwrap()
        }
    }

    impl Drop for EpubFixture {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }

    fn striped_pdf(pages: usize) -> Pdf {
        let kids = (0..pages)
            .map(|index| format!("{} 0 R", 3 + index * 2))
            .collect::<Vec<_>>()
            .join(" ");
        let font = 3 + pages * 2;
        let link = font + 1;
        let mut objects = vec![
            "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
            format!(
                "<< /Type /Pages /Kids [{kids}] /Count {pages} /Resources << /Font << /F1 {font} 0 R >> >> >>"
            ),
        ];
        for index in 0..pages {
            let contents = 4 + index * 2;
            let mut stream = format!("{:.2} g 0 0 120 80 re f\n", (index + 1) as f32 * 0.1);
            let annotation = if index == 1 {
                stream.push_str("0 g BT /F1 12 Tf 10 63 Td (Second) Tj ET\n");
                format!("/Annots [{link} 0 R]")
            } else {
                String::new()
            };
            objects.push(format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 120 80] /Contents {contents} 0 R {annotation} >>"
            ));
            objects.push(format!(
                "<< /Length {} >>\nstream\n{stream}endstream",
                stream.len()
            ));
        }
        objects.push("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_owned());
        objects.push("<< /Subtype /Link /Rect [8 60 60 76] /Dest [3 0 R /Fit] >>".to_owned());
        let mut pdf = b"%PDF-1.4\n".to_vec();
        let mut offsets = Vec::new();
        for (index, object) in objects.iter().enumerate() {
            offsets.push(pdf.len());
            writeln!(pdf, "{} 0 obj\n{object}\nendobj", index + 1).unwrap();
        }
        let xref = pdf.len();
        writeln!(pdf, "xref\n0 {}\n0000000000 65535 f ", offsets.len() + 1).unwrap();
        for offset in offsets {
            writeln!(pdf, "{offset:010} 00000 n ").unwrap();
        }
        writeln!(
            pdf,
            "trailer\n<< /Root 1 0 R /Size {} >>\nstartxref\n{xref}\n%%EOF",
            objects.len() + 1
        )
        .unwrap();
        Pdf::open(pdf).unwrap()
    }

    fn reading(pdf: Pdf, view: View) -> FixedReading {
        FixedReading::with_source(FixedDocument::Pdf(pdf.into()), 0, view).unwrap()
    }

    #[test]
    fn forced_ocr_reuses_only_the_chosen_language_and_keeps_legacy_english() {
        let pdf = Pdf::open(include_bytes!("../tests/fixtures/tilted-scan.pdf").to_vec()).unwrap();
        let mut reader = reading(pdf, View::default());
        let tmp = std::env::temp_dir().join(format!("reader-ocr-language-{}", std::process::id()));
        let mut words = reader.page_text(0).unwrap();
        words.words.truncate(1);
        words.lines.clear();
        words.text.truncate(words.words[0].bytes.end);
        reader.set_ocr_path(tmp.clone());
        reader.remember_ocr(0, "fra", words.clone()).unwrap();
        assert!(reader.cached_ocr_for(0, Some("eng")).is_none());
        assert!(reader.cached_ocr_for(0, Some("fra")).is_some());
        reader.forget_ocr();
        assert!(reader.cached_ocr_for(0, Some("eng")).is_none());
        assert_eq!(reader.cached_ocr_for(0, Some("fra")).unwrap(), words);
        fs::write(tmp.join("0.json"), serde_json::to_vec(&words).unwrap()).unwrap();
        assert!(reader.cached_ocr_for(0, Some("fra")).is_none());
        assert_eq!(reader.cached_ocr_for(0, Some("eng")).unwrap(), words);
        fs::remove_file(tmp.join("0.json")).unwrap();
        fs::remove_dir(tmp).unwrap();
    }

    #[test]
    fn passage_navigation_keeps_visible_words_and_reveals_cropped_rotated_regions() {
        let first = PdfTextRect {
            x0: 0.10,
            y0: 0.20,
            x1: 0.15,
            y1: 0.25,
        };
        let second = PdfTextRect {
            x0: 0.16,
            y0: 0.20,
            x1: 0.21,
            y1: 0.25,
        };
        let crop = PdfTextRect {
            x0: 0.08,
            y0: 0.10,
            x1: 0.93,
            y1: 0.90,
        };
        let target = FixedJump::regions(&[first, second]).unwrap();
        for (continuous, turns) in [(false, 0), (false, 1), (true, 0), (true, 3)] {
            let view = View {
                zoom: 4,
                continuous,
                width_fit: continuous,
                turns,
                crop: Some(crop),
                ..View::default()
            };
            let mut reader =
                FixedReading::with_source(FixedDocument::Pdf(striped_pdf(4).into()), 0, view).unwrap();
            reader.jump_to(1, Some(&target)).unwrap();
            assert_eq!(reader.page(), 1);
            assert_eq!(reader.preferences().crop, Some(crop));
            assert_eq!(reader.preferences().zoom, 4);
            for region in [first, second] {
                let (quad, clipped) = reader.projected_region(1, region).unwrap();
                assert_eq!(quad.bounds(), clipped);
            }
            let before = reader.preferences();
            reader.jump_to(1, Some(&target)).unwrap();
            assert_eq!(reader.preferences(), before);
            let outside = PdfTextRect {
                x0: 0.0,
                y0: 0.02,
                x1: 0.03,
                y1: 0.04,
            };
            assert!(
                reader
                    .jump_to(2, FixedJump::regions(&[outside]).as_ref())
                    .is_err()
            );
            assert_eq!(reader.page(), 1);
            assert_eq!(reader.preferences(), before);
            // The first word remains the anchor when the full passage is wider than the viewport.
            let distant = PdfTextRect {
                x0: 0.83,
                y0: 0.75,
                x1: 0.90,
                y1: 0.84,
            };
            reader
                .jump_to(2, FixedJump::regions(&[first, distant]).as_ref())
                .unwrap();
            let (quad, clipped) = reader.projected_region(2, first).unwrap();
            assert_eq!(quad.bounds(), clipped);
        }
    }

    #[test]
    fn following_an_xyz_link_positions_the_painted_target_in_paged_and_scrolling_views() {
        let content = "0 g 174 586 12 12 re f";
        let objects = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
            "<< /Type /Pages /Count 2 /Kids [3 0 R 4 0 R] >>".to_owned(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 800 400] /Annots [5 0 R] >>".to_owned(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 600 800] /Contents 6 0 R >>".to_owned(),
            "<< /Subtype /Link /Rect [30 30 70 70] /Dest [4 0 R /XYZ 180 592 null] >>".to_owned(),
            format!(
                "<< /Length {} >>\nstream\n{content}\nendstream",
                content.len()
            ),
        ];
        let crop = PdfTextRect {
            x0: 0.05,
            y0: 0.10,
            x1: 0.95,
            y1: 0.90,
        };
        for (continuous, turns) in [(false, 0), (true, 0), (false, 1), (true, 3)] {
            let pdf = Pdf::open(crate::fixed::tests::pdf_objects(&objects)).unwrap();
            let link = &pdf.page_links(0).unwrap()[0];
            assert_eq!(link.target, PdfLinkTarget::Page(1));
            let preferences = View {
                zoom: 4,
                width_fit: continuous,
                continuous,
                turns,
                crop: Some(crop),
                ..View::default()
            };
            let mut reader =
                FixedReading::with_source(FixedDocument::Pdf(pdf.into()), 0, preferences).unwrap();
            reader
                .jump_to(1, link.destination_point.map(FixedJump::LinkPoint).as_ref())
                .unwrap();
            assert_eq!(reader.page(), 1);
            assert_eq!(reader.preferences().zoom, 4);
            assert_eq!(reader.preferences().crop, Some(crop));
            let visible = &reader.visible[0];
            let point = visible.request.page_to_viewport(visible.size).unwrap()
                * link.destination_point.unwrap();
            assert!((-0.5..f64::from(WIDTH)).contains(&point.x));
            assert!((-0.5..f64::from(PAGE_HEIGHT)).contains(&point.y));
            let pixel = point.y.round().max(0.0) as usize * WIDTH as usize
                + point.x.round().max(0.0) as usize;
            assert!(reader.frame.pixels[pixel] < 32);
        }
    }

    #[test]
    fn straightening_keeps_source_words_links_and_highlights_aligned_in_each_view() {
        let pdf = Pdf::open(include_bytes!("../tests/fixtures/tilted-scan.pdf").to_vec()).unwrap();
        let mut reader = reading(pdf, View::default());
        let original = reader.frame.pixels.clone();
        reader.set_straighten(5).unwrap();
        assert!(
            original == reader.frame.pixels,
            "8° is over the user's 5° limit"
        );
        reader.set_straighten(10).unwrap();
        assert!(original != reader.frame.pixels);
        let angle = |page: &VisiblePage| {
            let c = page.correction.as_coeffs();
            c[1].atan2(c[0]).to_degrees()
        };
        assert!(
            (angle(&reader.visible[0]) - 8.0).abs() < 1.0,
            "{}",
            angle(&reader.visible[0])
        );
        let words = reader.page_text(0).unwrap();
        let trail = words
            .words
            .iter()
            .find(|w| &words.text[w.bytes.clone()] == "Trail")
            .unwrap()
            .bounds;
        let link = match &reader.source {
            FixedDocument::Pdf(pdf) => pdf.page_links(0).unwrap().into_iter().next().unwrap(),
            _ => unreachable!(),
        };
        let verify = |reader: &FixedReading| {
            let point = reader.visible_rectangle(0, trail).unwrap().center();
            let word = reader
                .select_word(point.x as f32, point.y as f32)
                .unwrap()
                .ready()
                .unwrap();
            assert_eq!(word.text(), "Trail");
            assert_eq!(word.bounds().next(), Some(trail));
            let (quad, rect) = reader.projected_region(0, link.bounds).unwrap();
            let center = rect.center();
            assert_eq!(
                reader
                    .link_at(center.x as f32, center.y as f32)
                    .map(|link| link.target),
                Some(PdfLinkTarget::Page(1))
            );
            // A projected box corner is outside this rotated annotation parallelogram.
            let y = rect.y0.ceil() as usize + 1;
            let span = quad.row(y, WIDTH as usize);
            let x = if span.start as f64 - rect.x0 > 2.0 {
                rect.x0 + 0.5
            } else {
                rect.x1 - 1.0
            };
            assert_eq!(reader.link_at(x as f32, y as f32 + 0.5), None);
        };
        verify(&reader);
        reader.rotate().unwrap();
        verify(&reader);
        reader
            .restore_view(View {
                straighten_limit: 10,
                crop: Some(PdfTextRect {
                    x0: 0.04,
                    y0: 0.04,
                    x1: 0.96,
                    y1: 0.96,
                }),
                ..View::default()
            })
            .unwrap();
        verify(&reader);
        reader.set_continuous(true).unwrap();
        verify(&reader);
        assert_eq!(reader.visible.len(), 2);
        assert!((angle(&reader.visible[1]) + 8.0).abs() < 1.0);
        let prefs = reader.preferences();
        let restored: View = serde_json::from_slice(&serde_json::to_vec(&prefs).unwrap()).unwrap();
        assert_eq!(restored.straighten_limit, 10);
        let count = reader.deskew_cache.angles_count();
        reader.set_straighten(0).unwrap();
        assert_eq!(reader.deskew_cache.angles_count(), count);
        reader.restore_view(View::default()).unwrap();
        assert!(reader.frame.pixels == original);
    }

    #[test]
    fn continuous_composites_adjacent_pages_and_projects_each_page_overlay() {
        let mut reader = reading(
            striped_pdf(4),
            View {
                width_fit: true,
                continuous: true,
                ..View::default()
            },
        );
        assert_eq!(reader.visible_pages().collect::<Vec<_>>(), [0, 1]);
        let divider = reader.visible[0].rows.end;
        assert_eq!(reader.visible[1].rows.start, divider);
        let pixel = |y: u32| reader.frame.pixels[(y * WIDTH + WIDTH / 2) as usize];
        assert!(pixel(divider - 12) < pixel(divider + 12));
        assert_eq!(reader.page_at(divider as f32 - 1.0), Some(0));
        assert_eq!(reader.page_at(divider as f32), Some(1));
        let second = reader
            .visible_rectangle(
                1,
                PdfTextRect {
                    x0: 0.1,
                    y0: 0.0,
                    x1: 0.9,
                    y1: 1.0,
                },
            )
            .unwrap();
        assert_eq!(second.y0, f64::from(divider));
        assert!(second.y1 <= f64::from(PAGE_HEIGHT));
        let text = reader.page_text(1).unwrap();
        let word = reader
            .visible_rectangle(1, text.words[0].bounds)
            .unwrap()
            .center();
        let selection = reader
            .select_word(word.x as f32, word.y as f32)
            .unwrap()
            .ready()
            .unwrap();
        assert_eq!(selection.page(), 1);
        assert_eq!(selection.text(), "Second");
        assert_eq!(
            reader
                .link_at(word.x as f32, word.y as f32)
                .map(|link| link.target),
            Some(PdfLinkTarget::Page(0))
        );

        let now = Instant::now();
        reader.turns.next_refresh = now + PAGE_REFRESH_GAP;
        reader.pending_scroll = (0.0, -(divider as f32 + 30.0));
        assert!(!reader.tick(now).unwrap());
        assert_eq!(reader.deadline(), Some(now + PAGE_REFRESH_GAP));
        assert!(reader.tick(now + PAGE_REFRESH_GAP).unwrap());
        assert_eq!(reader.page(), 1);
        assert_eq!(reader.preferences().offset_y, -30.0);
        assert_eq!(reader.page_at(0.0), Some(1));
    }

    #[test]
    fn continuous_page_gap_preserves_transformed_selection_and_empty_hits() {
        let mut reader = reading(
            striped_pdf(4),
            View {
                width_fit: true,
                continuous: true,
                ..View::default()
            },
        );
        let divider = reader.visible[0].rows.end;
        let text = reader.page_text(1).unwrap();
        let bounds = text.words[0].bounds;
        let word_before = reader.visible_rectangle(1, bounds).unwrap().center();

        reader.set_continuous_gap(48).unwrap();
        assert_eq!(reader.page(), 0);
        assert_eq!(reader.visible[0].rows.end, divider);
        assert_eq!(reader.visible[1].rows.start, divider + 48);
        let gap = (divider * WIDTH) as usize..((divider + 48) * WIDTH) as usize;
        assert!(reader.frame.pixels[gap].iter().all(|&tone| tone == 255));
        assert_eq!(reader.page_at((divider + 12) as f32), None);
        assert!(
            reader
                .link_at(word_before.x as f32, (divider + 12) as f32)
                .is_none()
        );
        assert!(
            reader
                .select_word(word_before.x as f32, (divider + 12) as f32)
                .unwrap()
                .ready()
                .is_none()
        );
        let word = reader.visible_rectangle(1, bounds).unwrap().center();
        assert_eq!(word.x, word_before.x);
        assert_eq!(word.y - word_before.y, 48.0);
        let chosen = reader
            .select_word(word.x as f32, word.y as f32)
            .unwrap()
            .ready()
            .unwrap();
        assert_eq!(chosen.text(), "Second");
        assert_eq!(
            reader
                .link_at(word.x as f32, word.y as f32)
                .map(|link| link.target),
            Some(PdfLinkTarget::Page(0))
        );

        // A viewport whose origin falls inside a gap starts the next page below it.
        let in_gap = View {
            offset_y: -((divider + 12) as f32),
            ..reader.preferences()
        };
        reader.present(0, in_gap).unwrap();
        assert_eq!(reader.page(), 0);
        assert_eq!(reader.visible[0].rows.start, 36);
        assert_eq!(reader.page_at(35.0), None);
        assert_eq!(reader.page_at(36.0), Some(1));
        reader.set_zoom(1).unwrap();
        assert_eq!(reader.visible[0].rows.start, 36);
        assert_eq!(reader.page_at(36.0), Some(1));

        reader.jump(reader.count() - 1).unwrap();
        assert_eq!(reader.visible.last().unwrap().rows.end, PAGE_HEIGHT);
        assert!(reader.set_continuous_gap(MAX_CONTINUOUS_GAP + 1).is_err());
        assert_eq!(reader.preferences().continuous_gap, 48);
        reader.set_continuous(false).unwrap();
        reader.set_continuous(true).unwrap();
        assert_eq!(reader.preferences().continuous_gap, 48);
    }

    #[test]
    fn continuous_clamps_document_bottom_and_toggles_back_to_paged_with_crop() {
        let pdf = striped_pdf(3);
        let crop = PdfTextRect {
            x0: 0.05,
            y0: 0.15,
            x1: 0.95,
            y1: 0.85,
        };
        let view = View {
            width_fit: true,
            continuous: true,
            crop: Some(crop),
            ..View::default()
        };
        let mut reader = reading(pdf, view);
        let end = reader.count() - 1;
        reader
            .present(
                end,
                View {
                    offset_y: -800.0,
                    ..view
                },
            )
            .unwrap();
        assert_eq!(reader.visible.last().unwrap().rows.end, PAGE_HEIGHT);
        assert_eq!(reader.visible.last().unwrap().index, end);
        assert!(reader.page() < end);
        let saved = reader.preferences();
        let roundtrip: View = serde_json::from_slice(&serde_json::to_vec(&saved).unwrap()).unwrap();
        assert_eq!(saved, roundtrip);
        reader.set_continuous(false).unwrap();
        assert!(!reader.is_continuous());
        assert!(reader.preferences().width_fit);
        assert_eq!(reader.preferences().crop, Some(crop));
        assert_eq!(reader.visible_pages().collect::<Vec<_>>(), [reader.page()]);
    }

    #[test]
    fn thumbnail_is_bounded_and_keeps_the_page_aspect_ratio() {
        let mut reader = reading(striped_pdf(2), View::default());
        let thumbnail = reader.thumbnail(1).unwrap();
        assert_eq!((thumbnail.width, thumbnail.height), (220, 300));
        assert_eq!(thumbnail.pixels.len(), 220 * 300);
        assert_eq!(thumbnail.pixels[0], 255);
        assert!(thumbnail.pixels[150 * 220 + 110] < 128);
        assert!(reader.thumbnail(2).is_err());
        reader.rotate().unwrap();
        let rotated = reader.thumbnail(1).unwrap();
        assert_ne!(rotated.pixels, thumbnail.pixels);
    }

    #[test]
    fn fit_height_pans_landscape_pages_and_round_trips_the_saved_view() {
        let mut reader = reading(striped_pdf(3), View::default());
        let (size, whole_page) = reader.page_viewport().unwrap();
        reader.fit_height().unwrap();
        let (_, fit_height) = reader.page_viewport().unwrap();
        assert!(fit_height.scale > whole_page.scale);
        assert!((size.1 * fit_height.scale - PAGE_HEIGHT as f32).abs() < 0.01);
        assert!(size.0 * fit_height.scale > WIDTH as f32);

        let swipe = |reader: &mut FixedReading, dx, dy| {
            reader
                .input(Input::Swipe {
                    dx,
                    dy,
                    start_y: PAGE_HEIGHT as f32 / 2.0,
                })
                .unwrap()
        };
        assert!(swipe(&mut reader, 10_000.0, 10.0));
        let positive = reader.preferences().offset_x;
        assert!(positive > 0.0);
        assert_eq!(reader.page(), 0);
        assert!(reader.preferences().offset_y.abs() < 0.01);
        assert!(swipe(&mut reader, -10_000.0, -10.0));
        assert_eq!(reader.preferences().offset_x, -positive);
        assert_eq!(reader.page(), 0);

        let saved = reader.preferences();
        let roundtrip: View = serde_json::from_slice(&serde_json::to_vec(&saved).unwrap()).unwrap();
        assert!(roundtrip.height_fit && !roundtrip.width_fit);
        assert_eq!(saved, roundtrip);
        assert_eq!(reader.start_crop().unwrap(), saved);
        assert!(!reader.preferences().height_fit);
        reader.restore_view(roundtrip).unwrap();
        assert_eq!(reader.preferences(), saved);
        reader.turns.next_refresh = Instant::now();
        assert!(swipe(&mut reader, 2.0, -100.0));
        assert_eq!(reader.page(), 1);
        assert_eq!(reader.preferences().offset_x, 0.0);

        assert!(reader.zoom(1).unwrap());
        assert_eq!(
            reader.page_viewport().unwrap().1.scale,
            fit_height.scale * ZOOM[1]
        );
        reader.fit(true).unwrap();
        assert!(reader.preferences().width_fit && !reader.preferences().height_fit);
        reader.fit(false).unwrap();
        assert!(!reader.preferences().width_fit && !reader.preferences().height_fit);
        reader.set_continuous(true).unwrap();
        assert!(reader.is_continuous());
        reader.fit_height().unwrap();
        assert!(!reader.is_continuous());
        assert!(reader.preferences().height_fit);
        reader.set_continuous(true).unwrap();
        assert!(reader.is_continuous() && !reader.preferences().height_fit);
    }

    #[test]
    fn ordered_dither_quantizes_every_tone_and_preserves_its_tile_average() {
        for gray in 0..=255 {
            let values = (0..4)
                .flat_map(|y| (0..4).map(move |x| ordered_gray(gray, x, y)))
                .collect::<Vec<_>>();
            assert!(values.iter().all(|tone| tone % 17 == 0));
            let total = values.iter().map(|&tone| i32::from(tone)).sum::<i32>();
            assert!((total - 16 * i32::from(gray)).abs() <= 8);
            assert_eq!(ordered_gray(gray, 0, 0), ordered_gray(gray, 4, 8));
        }
    }

    #[test]
    fn dither_and_scan_white_only_adjust_displayed_rows_without_changing_geometry() {
        let mut reader = reading(striped_pdf(2), View::default());
        let page = reader.page();
        let source = reader.frame.pixels.clone();
        let original_viewport = reader.page_viewport().unwrap();
        assert_eq!(scale_scan_white(0, 220), 0);
        assert_eq!(scale_scan_white(220, 220), 255);
        assert_eq!(scale_scan_white(255, 220), 255);
        assert!(scale_scan_white(155, 220) > 155);
        assert!(reader.set_scan_white_level(Some(191)).is_err());

        reader.set_scan_white_level(Some(220)).unwrap();
        let lighter = reader.display_frame(0..PAGE_HEIGHT).pixels;
        assert!(
            lighter
                .iter()
                .zip(&source)
                .any(|(&pixel, &original)| pixel > original)
        );
        assert!(
            lighter
                .iter()
                .zip(&source)
                .all(|(&pixel, &original)| pixel >= original)
        );
        reader.set_dither(true);
        let row = PAGE_HEIGHT / 2 + 1;
        let range = (row * WIDTH) as usize..((row + 1) * WIDTH) as usize;
        let partial = reader.display_frame(row..row + 1).pixels;
        let full = reader.display_frame(0..PAGE_HEIGHT).pixels;
        assert_ne!(&full[range.clone()], &lighter[range.clone()]);
        assert!(full[range.clone()].iter().all(|tone| tone % 17 == 0));
        assert_eq!(&partial[range.clone()], &full[range.clone()]);
        assert_eq!(&partial[..range.start], &source[..range.start]);
        assert_eq!(&partial[range.end..], &source[range.end..]);
        assert_eq!(reader.page(), page);
        let viewport = reader.page_viewport().unwrap();
        assert_eq!(viewport.0, original_viewport.0);
        assert_eq!(viewport.1.y, original_viewport.1.y);
        assert_eq!(viewport.1.scale, original_viewport.1.scale);
        assert_eq!(reader.frame.pixels, source);

        reader.fit(true).unwrap();
        let saved: View =
            serde_json::from_slice(&serde_json::to_vec(&reader.preferences()).unwrap()).unwrap();
        assert!(saved.dither);
        assert_eq!(saved.scan_white_level, Some(220));
        reader.set_dither(false);
        reader.set_scan_white_level(None).unwrap();
        assert_eq!(
            reader.display_frame(0..PAGE_HEIGHT).pixels,
            reader.frame.pixels
        );
    }

    #[test]
    fn scan_contrast_stretches_gray_paper_and_keeps_light_ink_visible() {
        let mut scan = vec![255; 120];
        scan.extend([220; 80]);
        scan.extend([155; 25]);
        scan.extend([230; 15]);
        let adjusted = scan_contrast_table(&scan).unwrap();
        assert_eq!((adjusted[0], adjusted[255]), (0, 255));
        assert!(adjusted.windows(2).all(|pair| pair[0] <= pair[1]));
        assert!(adjusted[220] > 220);
        assert!(adjusted[155] < 155);

        let mut faint = vec![242; 500];
        faint.extend([226; 20]);
        let adjusted = scan_contrast_table(&faint).unwrap();
        assert!(adjusted[242] - adjusted[226] >= 16);
        assert!(adjusted[254] < 255);
        assert!(scan_contrast_table(&[200; 64]).is_none());
        assert!(scan_contrast_table(&[255; 64]).is_none());
    }

    #[test]
    fn scan_contrast_only_changes_visible_paint_and_persists_through_fit() {
        let mut reader = reading(striped_pdf(2), View::default());
        assert!(reader.supports_scan_contrast());
        let original = reader.frame.pixels.clone();
        let point = reader.crop_point(WIDTH as f32 / 2.0, PAGE_HEIGHT as f32 / 2.0);
        assert_eq!(reader.display_frame(0..PAGE_HEIGHT).pixels, original);

        reader.set_scan_contrast(true).unwrap();
        let row = PAGE_HEIGHT / 2;
        let adjusted = reader.display_frame(row..row + 1).pixels;
        let selected = (row * WIDTH) as usize..((row + 1) * WIDTH) as usize;
        assert_ne!(adjusted[selected.clone()], original[selected.clone()]);
        assert_eq!(adjusted[..selected.start], original[..selected.start]);
        assert_eq!(adjusted[selected.end..], original[selected.end..]);
        assert_eq!(reader.frame.pixels, original);
        assert_eq!(
            reader.crop_point(WIDTH as f32 / 2.0, PAGE_HEIGHT as f32 / 2.0),
            point
        );

        reader.fit(true).unwrap();
        let saved: View =
            serde_json::from_slice(&serde_json::to_vec(&reader.preferences()).unwrap()).unwrap();
        assert!(saved.scan_contrast);
        reader.set_scan_contrast(false).unwrap();
        assert_eq!(
            reader.display_frame(0..PAGE_HEIGHT).pixels,
            reader.frame.pixels
        );
    }

    #[test]
    fn full_height_preserves_a_zoomed_source_point_continuous_gaps_and_spreads() {
        use primitive_app::HEIGHT;
        let mut r = reading(
            striped_pdf(7),
            View {
                zoom: 2,
                height_fit: true,
                offset_x: 68.0,
                offset_y: -94.0,
                ..View::default()
            },
        );
        let center = |r: &FixedReading| {
            r.crop_point(WIDTH as f32 * 0.5, r.frame.height as f32 * 0.5)
                .unwrap()
        };
        let p = center(&r);
        let first = r.frame.clone();
        assert!(
            r.resize(HEIGHT, || Err(io::Error::other("storage unavailable")))
                .is_err()
        );
        assert_eq!(r.frame, first);
        r.resize(HEIGHT, || Ok(())).unwrap();
        let q = center(&r);
        assert!((p.0 - q.0).abs() < 0.01 && (p.1 - q.1).abs() < 0.01);
        r.resize(PAGE_HEIGHT, || Ok(())).unwrap();
        assert_eq!(r.frame, first);
        r.fit(true).unwrap();
        r.set_continuous(true).unwrap();
        r.set_continuous_gap(74).unwrap();
        r.jump(2).unwrap();
        let start = r.source_point(r.page, WIDTH as f32 / 2.0, 10.0);
        r.resize(HEIGHT, || Ok(())).unwrap();
        assert_eq!(r.frame.height, HEIGHT);
        assert_eq!(r.source_point(r.page, WIDTH as f32 / 2.0, 10.0), start);
        assert!(r.visible.len() >= 2 && r.visible.last().unwrap().rows.end == HEIGHT);
        r.resize(PAGE_HEIGHT, || Ok(())).unwrap();
        assert_eq!(r.visible.last().unwrap().rows.end, PAGE_HEIGHT);

        let fixture = ComicFixture::striped();
        let mut comic = FixedReading::open(
            PAGE_HEIGHT,
            DocumentFormat::Cbz,
            &fixture.0,
            0,
            &[],
            View::default(),
            None,
            None,
        )
        .unwrap()
        .unwrap();
        comic.set_spread(true).unwrap();
        comic.jump(1).unwrap();
        let before = comic.frame.clone();
        comic.resize(HEIGHT, || Ok(())).unwrap();
        assert_eq!(comic.visible_pages().collect::<Vec<_>>(), [1, 2]);
        assert_eq!(comic.frame.pixels.len(), (WIDTH * HEIGHT) as usize);
        comic.resize(PAGE_HEIGHT, || Ok(())).unwrap();
        assert_eq!(comic.frame, before);
    }

    #[test]
    fn comic_spread_keeps_cover_and_tail_solo_and_pairs_direct_jumps() {
        let fixture = ComicFixture::striped();
        let mut reader = FixedReading::open(
            PAGE_HEIGHT,
            DocumentFormat::Cbz,
            &fixture.0,
            0,
            &[],
            View {
                zoom: 1,
                ..View::default()
            },
            None,
            None,
        )
        .unwrap()
        .unwrap();

        let center = |reader: &FixedReading, x: u32| {
            reader.frame.pixels[(PAGE_HEIGHT / 2 * WIDTH + x) as usize]
        };
        assert!(reader.supports_spreads());
        reader.set_spread(true).unwrap();
        assert_eq!(reader.visible_pages().collect::<Vec<_>>(), [0]);
        assert_eq!(center(&reader, WIDTH / 2), 32);

        let turn = |reader: &mut FixedReading, input| {
            reader.turns.next_refresh = Instant::now();
            reader.input(input).unwrap()
        };
        assert!(turn(&mut reader, Input::PageForward));
        assert_eq!(reader.visible_pages().collect::<Vec<_>>(), [1, 2]);
        assert_eq!(center(&reader, WIDTH / 4), 64);
        assert_eq!(center(&reader, WIDTH / 2), 255);
        assert_eq!(center(&reader, WIDTH * 3 / 4), 128);

        reader.reverse_direction().unwrap();
        assert_eq!(center(&reader, WIDTH / 4), 128);
        assert_eq!(center(&reader, WIDTH * 3 / 4), 64);
        assert!(turn(&mut reader, Input::PageForward));
        assert_eq!(reader.visible_pages().collect::<Vec<_>>(), [3]);
        assert_eq!(center(&reader, WIDTH / 2), 192);
        assert!(!turn(&mut reader, Input::PageForward));
        assert!(turn(&mut reader, Input::PageBack));
        reader.jump(2).unwrap();
        assert_eq!(reader.visible_pages().collect::<Vec<_>>(), [1, 2]);

        let preferences: View =
            serde_json::from_slice(&serde_json::to_vec(&reader.preferences()).unwrap()).unwrap();
        assert!(preferences.spread);
        assert!(preferences.right_to_left);
        assert!(turn(&mut reader, Input::PageBack));
        assert_eq!(reader.page(), 0);
        reader.set_spread(false).unwrap();
        assert!(!reader.preferences().spread);
        assert_eq!(reader.preferences().zoom, 1);
        assert!(turn(&mut reader, Input::PageForward));
        assert_eq!(reader.visible_pages().collect::<Vec<_>>(), [1]);
        assert!(turn(&mut reader, Input::PageForward));
        assert_eq!(reader.visible_pages().collect::<Vec<_>>(), [2]);
    }

    #[test]
    fn epub_spreads_honor_physical_slots_and_keep_a_selected_spine_page() {
        let fixture = EpubFixture::striped("both");
        let mut reader = fixture.open(0, View::default());
        let pixel = |reader: &FixedReading, x: u32| {
            reader.frame.pixels[(PAGE_HEIGHT / 2 * WIDTH + x) as usize]
        };
        assert!(reader.supports_spreads());
        assert!(reader.is_spread());
        assert!(reader.preferences().right_to_left);
        assert_eq!(reader.visible_pages().collect::<Vec<_>>(), [0]);
        assert_eq!(pixel(&reader, WIDTH / 4), 32);
        assert_eq!(pixel(&reader, WIDTH * 3 / 4), 255);

        reader.jump(2).unwrap();
        assert_eq!(reader.page(), 2);
        assert_eq!(reader.visible_pages().collect::<Vec<_>>(), [1, 2]);
        assert_eq!(pixel(&reader, WIDTH / 2 - 1), 128);
        assert_eq!(pixel(&reader, WIDTH / 2), 64);
        let saved: View =
            serde_json::from_slice(&serde_json::to_vec(&reader.preferences()).unwrap()).unwrap();
        let mut reopened = fixture.open(reader.page(), saved);
        assert_eq!(reopened.page(), 2);
        assert_eq!(reopened.visible_pages().collect::<Vec<_>>(), [1, 2]);

        reopened.set_spread(false).unwrap();
        assert_eq!(reopened.page(), 2);
        assert_eq!(reopened.visible_pages().collect::<Vec<_>>(), [2]);
        reopened.fit_height().unwrap();
        assert_eq!(reopened.preferences().epub_spread, Some(false));
        reopened.set_spread(true).unwrap();
        assert_eq!(reopened.page(), 2);
        assert_eq!(reopened.visible_pages().collect::<Vec<_>>(), [1, 2]);
        reopened.turns.next_refresh = Instant::now();
        assert!(reopened.input(Input::PageForward).unwrap());
        assert_eq!(reopened.page(), 3);
        assert_eq!(reopened.visible_pages().collect::<Vec<_>>(), [3]);
        assert_eq!(pixel(&reopened, WIDTH / 2), 96);

        reopened.reverse_direction().unwrap();
        assert_eq!(reopened.preferences().epub_right_to_left, Some(false));
        assert!(!reopened.preferences().right_to_left);
        let preferences: View =
            serde_json::from_slice(&serde_json::to_vec(&reopened.preferences()).unwrap()).unwrap();
        let mut reopened = fixture.open(reopened.page(), preferences);
        assert!(!reopened.preferences().right_to_left);
        assert_eq!(reopened.preferences().epub_spread, Some(true));
        assert_eq!(reopened.page(), 3);

        reopened.jump(2).unwrap();
        assert_eq!(reopened.page(), 2);
        assert_eq!(pixel(&reopened, WIDTH / 2 - 1), 128);
        assert_eq!(pixel(&reopened, WIDTH / 2), 64);
        reopened.jump(5).unwrap();
        assert_eq!(reopened.visible_pages().collect::<Vec<_>>(), [4, 5]);
        assert_eq!(pixel(&reopened, WIDTH / 2 - 1), 160);
        assert_eq!(pixel(&reopened, WIDTH / 2), 192);
        reopened.turns.next_refresh = Instant::now();
        assert!(!reopened.input(Input::PageForward).unwrap());
        assert_eq!(reopened.page(), 5);
    }

    #[test]
    fn epub_spread_none_keeps_all_spine_items_centered() {
        let fixture = EpubFixture::striped("none");
        let mut reader = fixture.open(1, View::default());
        assert_eq!(reader.page(), 1);
        assert_eq!(reader.visible_pages().collect::<Vec<_>>(), [1]);
        assert!(!reader.supports_spreads());
        assert!(!reader.is_spread());
        assert!(reader.set_spread(true).is_err());
    }
}
