use std::fs::{self, File};
use std::io::{self, Read};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::time::Instant;

use primitive_app::{WIDTH, with_storage};
use primitive_ui::{CAPTION, Canvas, GrayImage, HAIRLINE, Input, Rect};

use crate::Result;
use crate::comic::{Comic, ImagePage};
use crate::djvu::{Djvu, MAX_DJVU_BYTES};
use crate::fixed::{MAX_PDF_BYTES, Pdf, PdfLinkTarget, PdfOutlineItem, RenderRequest};
use crate::fixed_epub::FixedEpub;
use crate::fixed_text::{PdfPageText, PdfTextRect};
use crate::library::{DocumentFormat, FixedPreferences as View};
use crate::session::{DONE, PAGE_HEIGHT, PAGE_REFRESH_GAP, PageTurns, draw_page_picture};

const SMALLER: Rect = Rect::new(840, PAGE_HEIGHT as i32 + 5, 120, 60);
const LARGER: Rect = Rect::new(974, PAGE_HEIGHT as i32 + 5, 120, 60);
const ZOOM: [f32; 5] = [1.0, 1.25, 1.5, 2.0, 3.0];
const MAX_OCR_RASTER_SIDE: f32 = 1800.0;
const MAX_OCR_CACHE_BYTES: u64 = 512 * 1024;
const MAX_OCR_CACHE_PAGES: usize = 32;

pub(crate) struct FixedReading {
    source: FixedDocument,
    page: usize,
    view: View,
    frame: GrayImage,
    turns: PageTurns,
    outline: Vec<PdfOutlineItem>,
    ocr_path: Option<PathBuf>,
    last_ocr: Option<(usize, PdfPageText)>,
}

pub(crate) struct PdfSelection {
    page: usize,
    document: PdfPageText,
    first: usize,
    last: usize,
}

impl PdfSelection {
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
    Pdf(Pdf),
    Comic(Comic),
    Image(ImagePage),
}

impl FixedReading {
    pub(crate) fn open(
        format: DocumentFormat,
        path: &Path,
        page: usize,
        fonts: &[PathBuf],
        preferences: View,
        password: Option<&str>,
    ) -> Result<Option<Self>> {
        let source = match format {
            DocumentFormat::Epub => {
                let source =
                    with_storage(|| FixedEpub::open(path, fonts).map_err(io::Error::other))?;
                let Some(book) = source else { return Ok(None) };
                FixedDocument::Epub(Box::new(book))
            }
            DocumentFormat::Pdf => {
                let bytes = with_storage(|| {
                    let mut bytes = Vec::new();
                    File::open(path)?
                        .take((MAX_PDF_BYTES + 1) as u64)
                        .read_to_end(&mut bytes)?;
                    Ok(bytes)
                })?;
                FixedDocument::Pdf(Pdf::open_with_password(
                    bytes,
                    password.unwrap_or_default(),
                )?)
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
            DocumentFormat::Cbz => FixedDocument::Comic(with_storage(|| {
                Comic::open(path).map_err(io::Error::other)
            })?),
            DocumentFormat::Jpeg | DocumentFormat::Png => {
                FixedDocument::Image(with_storage(|| {
                    ImagePage::open(path).map_err(io::Error::other)
                })?)
            }
            _ => return Ok(None),
        };
        let page = page.min(source.count() - 1);
        let outline = match &source {
            FixedDocument::Epub(book) => book.outline(),
            FixedDocument::Pdf(pdf) => pdf.outline(),
            _ => Vec::new(),
        };
        let mut view = preferences.normalized();
        let frame = source.frame(page, &mut view)?;
        Ok(Some(Self {
            source,
            page,
            view,
            frame,
            turns: PageTurns::new(Instant::now()),
            outline,
            ocr_path: None,
            last_ocr: None,
        }))
    }

    pub(crate) fn title(&self) -> Option<&str> {
        match &self.source {
            FixedDocument::Epub(book) => Some(book.title()),
            _ => None,
        }
    }

    pub(crate) fn page(&self) -> usize {
        self.page
    }

    pub(crate) fn count(&self) -> usize {
        self.source.count()
    }

    pub(crate) fn outline(&self) -> &[PdfOutlineItem] {
        &self.outline
    }

    pub(crate) fn preferences(&self) -> View {
        self.view
    }

    pub(crate) fn supports_rtl(&self) -> bool {
        matches!(
            self.source,
            FixedDocument::Comic(_) | FixedDocument::Epub(_)
        )
    }

    pub(crate) fn reverse_direction(&mut self) {
        self.view.right_to_left = !self.view.right_to_left;
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
                ..View::default()
            },
        )?;
        Ok(original)
    }

    pub(crate) fn restore_view(&mut self, preferences: View) -> Result<()> {
        self.present(self.page, preferences.normalized())
    }

    pub(crate) fn crop_point(&self, x: f32, y: f32) -> Option<(f32, f32)> {
        let (size, viewport) = self.page_viewport()?;
        let to_page = viewport.page_to_viewport(size).ok()?.inverse();
        let point = to_page * hayro::kurbo::Point::new(f64::from(x), f64::from(y));
        let x = point.x as f32 / size.0;
        let y = point.y as f32 / size.1;
        ((0.0..=1.0).contains(&x) && (0.0..=1.0).contains(&y)).then_some((x, y))
    }

    pub(crate) fn auto_crop(&self) -> Option<PdfTextRect> {
        let mut left = WIDTH;
        let mut right = 0;
        let mut top = PAGE_HEIGHT;
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

    fn cached_ocr(&self, page: usize) -> Option<PdfPageText> {
        if let Some((index, text)) = &self.last_ocr
            && *index == page
        {
            return Some(text.clone());
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
            let text: PdfPageText = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
            if bytes.len() as u64 > MAX_OCR_CACHE_BYTES || !text.valid_ocr_cache() {
                return Err(io::Error::other("Reader OCR cache is invalid"));
            }
            Ok(text)
        })
        .ok()
    }

    pub(crate) fn remember_ocr(&mut self, page: usize, text: PdfPageText) -> Result<()> {
        if !text.valid_ocr_cache() {
            return Err("OCR returned invalid text positions".into());
        }
        self.last_ocr = Some((page, text));
        let Some(directory) = &self.ocr_path else {
            return Ok(());
        };
        let path = directory.join(format!("{page}.json"));
        let bytes = serde_json::to_vec(&self.last_ocr.as_ref().unwrap().1)?;
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

    pub(crate) fn ocr_raster(&self) -> Result<GrayImage> {
        let size = match &self.source {
            FixedDocument::Pdf(pdf) => pdf.page_size(self.page)?,
            FixedDocument::Djvu(book) => book.page_size(self.page)?,
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
            FixedDocument::Pdf(pdf) => pdf.render(self.page, request)?,
            FixedDocument::Djvu(book) => book.render(self.page, request)?,
            _ => unreachable!(),
        };
        Ok(GrayImage {
            width,
            height,
            pixels,
        })
    }

    pub(crate) fn select_word(&self, x: f32, y: f32) -> Result<Option<PdfSelection>> {
        let document = self.page_text(self.page)?;
        Ok(self.word_at(&document, x, y).map(|word| PdfSelection {
            page: self.page,
            document,
            first: word,
            last: word,
        }))
    }

    pub(crate) fn extend_selection(&self, selection: &mut PdfSelection, x: f32, y: f32) {
        if selection.page != self.page {
            return;
        }
        if let Some(word) = self.word_at(&selection.document, x, y) {
            let first = selection.first.min(word);
            let last = selection.last.max(word);
            if last - first < 64 {
                selection.first = first;
                selection.last = last;
            }
        }
    }

    pub(crate) fn link_at(&self, x: f32, y: f32) -> Option<PdfLinkTarget> {
        let FixedDocument::Pdf(pdf) = &self.source else {
            return None;
        };
        let (size, request) = self.page_viewport()?;
        let point = hayro::kurbo::Point::new(f64::from(x), f64::from(y));
        pdf.page_links(self.page)
            .ok()?
            .into_iter()
            .find_map(|link| {
                link.bounds
                    .in_viewport(size, request)
                    .is_some_and(|bounds| bounds.contains(point))
                    .then_some(link.target)
            })
    }

    fn word_at(&self, text: &PdfPageText, x: f32, y: f32) -> Option<usize> {
        let (size, request) = self.page_viewport()?;
        text.words.iter().position(|word| {
            word.bounds.in_viewport(size, request).is_some_and(|rect| {
                rect.contains(hayro::kurbo::Point::new(f64::from(x), f64::from(y)))
            })
        })
    }

    pub(crate) fn page_viewport(&self) -> Option<((f32, f32), RenderRequest)> {
        let size = match &self.source {
            FixedDocument::Pdf(pdf) => pdf.page_size(self.page).ok()?,
            FixedDocument::Djvu(book) => book.page_size(self.page).ok()?,
            _ => return None,
        };
        let mut view = self.view;
        Some((size, viewport(size, &mut view)))
    }

    pub(crate) fn jump(&mut self, page: usize) -> Result<()> {
        self.present(
            page,
            View {
                offset_x: 0.0,
                offset_y: if self.view.width_fit { f32::MAX } else { 0.0 },
                ..self.view
            },
        )
    }

    pub(crate) fn fit(&mut self, width_fit: bool) -> Result<()> {
        self.present(
            self.page,
            View {
                turns: self.view.turns,
                width_fit,
                offset_y: if width_fit { f32::MAX } else { 0.0 },
                right_to_left: self.view.right_to_left,
                crop: self.view.crop,
                ..View::default()
            },
        )
    }

    pub(crate) fn rotate(&mut self) -> Result<()> {
        self.present(
            self.page,
            View {
                turns: (self.view.turns + 1) % 4,
                offset_x: 0.0,
                offset_y: 0.0,
                ..self.view
            },
        )
    }

    pub(crate) fn pause(&mut self) {
        self.turns = PageTurns::new(Instant::now());
    }

    pub(crate) fn deadline(&self) -> Option<Instant> {
        self.turns.deadline()
    }

    pub(crate) fn tick(&mut self, now: Instant) -> Result<bool> {
        let last = self.count() - 1;
        let page = self
            .turns
            .take_due(now)
            .into_iter()
            .fold(self.page, |page, step| {
                page.saturating_add_signed(step).min(last)
            });
        if page == self.page {
            return Ok(false);
        }
        self.jump(page)?;
        Ok(true)
    }

    pub(crate) fn input(&mut self, input: Input) -> Result<bool> {
        let horizontal = if self.view.right_to_left { -1 } else { 1 };
        let direction = match input {
            Input::Tap(x, y) if SMALLER.contains(x, y) => return self.zoom(-1),
            Input::Tap(x, y) if LARGER.contains(x, y) => return self.zoom(1),
            Input::PageForward => 1,
            Input::PageBack => -1,
            Input::Swipe { dx, dy, .. } if self.view.zoom > 0 || self.view.width_fit => {
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
            Input::Tap(x, y) if y < PAGE_HEIGHT as f32 && x < WIDTH as f32 / 3.0 => -horizontal,
            Input::Tap(x, y) if y < PAGE_HEIGHT as f32 && x > WIDTH as f32 * 2.0 / 3.0 => {
                horizontal
            }
            _ => return Ok(false),
        };
        self.turns.pending.push(direction);
        self.tick(Instant::now())
    }

    fn zoom(&mut self, delta: isize) -> Result<bool> {
        let zoom = self
            .view
            .zoom
            .saturating_add_signed(delta)
            .min(ZOOM.len() - 1);
        if zoom == self.view.zoom {
            return Ok(false);
        }
        self.present(self.page, View { zoom, ..self.view })?;
        Ok(true)
    }

    fn present(&mut self, page: usize, mut view: View) -> Result<()> {
        self.frame = self.source.frame(page, &mut view)?;
        self.page = page;
        self.view = view;
        self.turns.next_refresh = Instant::now() + PAGE_REFRESH_GAP;
        Ok(())
    }

    pub(crate) fn draw(
        &self,
        canvas: &mut Canvas,
        notice: Option<&str>,
        marks: &[PdfTextRect],
        selection: &[PdfTextRect],
        visible: Range<u32>,
    ) {
        let mut frame = self.frame.clone();
        if let Some((size, request)) = self.page_viewport() {
            for (regions, gray) in [(marks, 220), (selection, 180)] {
                for rect in regions
                    .iter()
                    .filter_map(|region| region.in_viewport(size, request))
                {
                    let left = rect.x0.floor() as usize;
                    let right = (rect.x1.ceil() as usize).min(WIDTH as usize);
                    let top = rect.y0.floor() as usize;
                    let bottom = (rect.y1.ceil() as usize).min(PAGE_HEIGHT as usize);
                    for row in frame
                        .pixels
                        .as_chunks_mut::<{ WIDTH as usize }>()
                        .0
                        .iter_mut()
                        .skip(top)
                        .take(bottom - top)
                    {
                        for pixel in &mut row[left..right] {
                            *pixel = (*pixel).min(gray);
                        }
                    }
                }
            }
        }
        draw_page_picture(canvas, frame, visible.clone());
        if visible != (0..PAGE_HEIGHT) || !selection.is_empty() {
            return;
        }
        canvas.fill(Rect::new(0, PAGE_HEIGHT as i32, WIDTH as i32, 1), HAIRLINE);
        let label = format!(
            "Page {} of {}  ·  {}%",
            self.page + 1,
            self.count(),
            (ZOOM[self.view.zoom] * 100.0) as u32
        );
        canvas.text(
            Rect::new(38, PAGE_HEIGHT as i32 + 12, SMALLER.x - 60, 48),
            notice.unwrap_or(&label),
            CAPTION,
        );
        canvas.button(SMALLER, "−", false, false);
        canvas.button(LARGER, "+", false, false);
        canvas.button(DONE, "Done", false, false);
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

    fn frame(&self, page: usize, view: &mut View) -> Result<GrayImage> {
        let pixels = match self {
            Self::Epub(book) => with_storage(|| {
                let image = book.decode_page(page).map_err(io::Error::other)?;
                image
                    .render(viewport(image.page_size(), view))
                    .map_err(io::Error::other)
            })?,
            Self::Pdf(pdf) => {
                let size = pdf.page_size(page)?;
                let request = viewport(size, view);
                mask_crop(pdf.render(page, request)?, view.crop, size, request)
            }
            Self::Djvu(book) => {
                let size = book.page_size(page)?;
                let request = viewport(size, view);
                mask_crop(book.render(page, request)?, view.crop, size, request)
            }
            Self::Comic(comic) => {
                let image = with_storage(|| comic.decode_page(page).map_err(io::Error::other))?;
                image.render(viewport(image.page_size(), view))?
            }
            Self::Image(image) => image.render(viewport(image.page_size(), view))?,
        };
        Ok(GrayImage {
            width: WIDTH,
            height: PAGE_HEIGHT,
            pixels,
        })
    }
}

fn mask_crop(
    mut pixels: Vec<u8>,
    crop: Option<PdfTextRect>,
    size: (f32, f32),
    request: RenderRequest,
) -> Vec<u8> {
    if let Some(crop) = crop.and_then(|crop| crop.in_viewport(size, request)) {
        let left = crop.x0.floor() as usize;
        let right = (crop.x1.ceil() as usize).min(WIDTH as usize);
        let top = crop.y0.floor() as usize;
        let bottom = (crop.y1.ceil() as usize).min(PAGE_HEIGHT as usize);
        for (row, line) in pixels
            .as_chunks_mut::<{ WIDTH as usize }>()
            .0
            .iter_mut()
            .enumerate()
        {
            if (top..bottom).contains(&row) {
                line[..left].fill(255);
                line[right..].fill(255);
            } else {
                line.fill(255);
            }
        }
    }
    pixels
}

fn viewport((mut width, mut height): (f32, f32), view: &mut View) -> RenderRequest {
    if !view.turns.is_multiple_of(2) {
        (width, height) = (height, width);
    }
    let viewport_width = WIDTH as f32;
    let viewport_height = PAGE_HEIGHT as f32;
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
    let fit = if view.width_fit {
        width_scale
    } else {
        width_scale.min(viewport_height / crop_height)
    };
    let scale = (fit * ZOOM[view.zoom]).min(24_000.0 / width.max(height));
    let clamp = |offset: f32, projected: f32, viewport: f32| {
        let limit = ((projected - viewport) / 2.0).max(0.0);
        offset.clamp(-limit, limit)
    };
    view.offset_x = clamp(view.offset_x, crop_width * scale, viewport_width);
    view.offset_y = clamp(view.offset_y, crop_height * scale, viewport_height);
    RenderRequest {
        width: WIDTH,
        height: PAGE_HEIGHT,
        scale,
        x: (viewport_width - crop_width * scale) / 2.0 - left * width * scale + view.offset_x,
        y: (viewport_height - crop_height * scale) / 2.0 - top * height * scale + view.offset_y,
        turns: view.turns,
    }
}
