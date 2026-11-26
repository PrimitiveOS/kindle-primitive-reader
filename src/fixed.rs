//! Rasterize one visible PDF viewport without retaining a Kindle storage handle.
//! Based on the bounded PDF renderer in PrimitiveOS Viewer.

use hayro::hayro_syntax::object::{
    Array, Dict, MaybeRef, Name, Object, ObjectIdentifier, Rect as PdfRect, String as PdfString,
};
use hayro::hayro_syntax::xref::XRef;
use hayro::hayro_syntax::{DecryptionError, LoadPdfError, Pdf as Document};
use hayro::kurbo::{Affine, Point, Rect};
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::vello_cpu::{Pixmap, RasterizerSettings, RenderContext, Resources, TargetInit};
use hayro::{RenderCache, RenderSettings, render_into};
use hayro_interpret::InterpreterSettings;
use hayro_interpret::util::{RectExt, TransformExt};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::OnceLock;

use crate::fixed_text::{self, PdfPageText, PdfTextError, PdfTextRect};

pub const MAX_PDF_BYTES: usize = 128 * 1024 * 1024;
const MAX_PAGES: usize = 10_000;
pub const MAX_VIEWPORT_PIXELS: u32 = 4 * 1024 * 1024;
pub const MAX_VIEWPORT_SIDE: u32 = 2048;
const MAX_PROJECTED_SIDE: f32 = 32_768.0;
pub(crate) const MAX_OUTLINE_ITEMS: usize = 512;
pub(crate) const MAX_OUTLINE_DEPTH: usize = 16;
const MAX_NAME_TREE_NODES: usize = 512;
const MAX_NAMED_DESTINATIONS: usize = 4096;
const MAX_DESTINATION_NAME: usize = 512;
pub(crate) const MAX_TITLE_BYTES: usize = 1024;
const MAX_PAGE_LABEL_NODES: usize = 512;
const MAX_PAGE_LABEL_DEPTH: usize = 16;
const MAX_PAGE_LABEL_CHARACTERS: u32 = 256;
const MAX_PAGE_ANNOTATIONS: usize = 1024;
const HIDDEN_LINK_FLAGS: u32 = (1 << 1) | (1 << 5);

pub struct Pdf {
    document: Document,
    page_labels: OnceLock<Vec<PageLabelRange>>,
}

struct PageLabelRange {
    first: usize,
    start: u32,
    prefix: String,
    style: PageLabelStyle,
}

enum PageLabelStyle {
    Prefix,
    Decimal,
    Roman { uppercase: bool },
    Alphabetic { uppercase: bool },
}

#[derive(Clone, Debug, PartialEq)]
pub struct PdfOutlineItem {
    pub title: String,
    /// Zero-based page index. A heading may have no destination in this document.
    pub page: Option<usize>,
    pub depth: usize,
    /// Optional destination in the target page's upright, cropped point space.
    pub destination_point: Option<Point>,
    /// Optional `/Fit`, `/XYZ`, `/FitH` or `/FitV` destination.
    pub destination_partial: Option<PartialPdfDestination>,
}

/// A PDF location whose position requires the current view or target page.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PartialPdfDestination {
    /// PDF default-user-space coordinate; absent for a fitted page edge.
    pub left: Option<f64>,
    /// PDF default-user-space coordinate; absent for a fitted page edge.
    pub top: Option<f64>,
    pub fit: PdfDestinationFit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PdfDestinationFit {
    Current,
    Page,
    Width,
    Height,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PdfLinkTarget {
    /// Zero-based page index in this document.
    Page(usize),
    /// A destination outside this document, such as a website or another file.
    External,
    /// A missing destination or unsupported action.
    Unavailable,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PdfLink {
    pub bounds: PdfTextRect,
    pub target: PdfLinkTarget,
    /// Address recorded by a URI link annotation.
    pub uri: Option<String>,
    /// A supported explicit destination, in the target page's upright, cropped point space.
    pub destination_point: Option<Point>,
    /// Optional `/Fit`, `/XYZ`, `/FitH` or `/FitV` destination.
    pub destination_partial: Option<PartialPdfDestination>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RenderRequest {
    pub width: u32,
    pub height: u32,
    pub scale: f32,
    /// Screen position of the rotated page's top-left corner in pixels.
    pub x: f32,
    pub y: f32,
    /// Clockwise quarter turns in addition to the PDF page's recorded orientation.
    pub turns: u8,
}

impl RenderRequest {
    pub(crate) fn page_to_viewport(self, (width, height): (f32, f32)) -> Result<Affine, PdfError> {
        if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
            return Err(PdfError::InvalidPageSize);
        }
        let count = self
            .width
            .checked_mul(self.height)
            .ok_or(PdfError::InvalidViewport)?;
        if self.width == 0
            || self.height == 0
            || count > MAX_VIEWPORT_PIXELS
            || self.width > MAX_VIEWPORT_SIDE
            || self.height > MAX_VIEWPORT_SIDE
            || !self.scale.is_finite()
            || self.scale <= 0.0
            || width.max(height) * self.scale > MAX_PROJECTED_SIDE
            || !self.x.is_finite()
            || !self.y.is_finite()
            || self.x.abs().max(self.y.abs()) > MAX_PROJECTED_SIDE
        {
            return Err(PdfError::InvalidViewport);
        }
        let width = f64::from(width);
        let height = f64::from(height);
        let turn = match self.turns % 4 {
            0 => Affine::IDENTITY,
            1 => Affine::new([0.0, 1.0, -1.0, 0.0, height, 0.0]),
            2 => Affine::new([-1.0, 0.0, 0.0, -1.0, width, height]),
            _ => Affine::new([0.0, -1.0, 1.0, 0.0, 0.0, width]),
        };
        Ok(Affine::translate((f64::from(self.x), f64::from(self.y)))
            * Affine::scale(f64::from(self.scale))
            * turn)
    }
}

impl PdfTextRect {
    /// Project normalized upright page coordinates into visible viewport pixels.
    pub fn in_viewport(self, page_size: (f32, f32), request: RenderRequest) -> Option<Rect> {
        if ![self.x0, self.y0, self.x1, self.y1]
            .into_iter()
            .all(|edge| (0.0..=1.0).contains(&edge))
            || self.x0 >= self.x1
            || self.y0 >= self.y1
        {
            return None;
        }
        let transform = request.page_to_viewport(page_size).ok()?;
        let (width, height) = (f64::from(page_size.0), f64::from(page_size.1));
        let page_rect = Rect::new(
            f64::from(self.x0) * width,
            f64::from(self.y0) * height,
            f64::from(self.x1) * width,
            f64::from(self.y1) * height,
        );
        let viewport = Rect::new(
            0.0,
            0.0,
            f64::from(request.width),
            f64::from(request.height),
        );
        let clipped = transform.transform_rect_bbox(page_rect).intersect(viewport);
        (clipped.width() > 0.0 && clipped.height() > 0.0).then_some(clipped)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PdfError {
    TooLarge,
    TooManyPages,
    Damaged,
    PasswordProtected,
    UnsupportedEncryption,
    MissingPage,
    InvalidPageSize,
    InvalidViewport,
    RenderFailed,
    TextExtractionFailed,
    LinkExtractionFailed,
}

impl fmt::Display for PdfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::TooLarge => "This PDF exceeds Reader's 128 MiB limit.",
            Self::TooManyPages => "This PDF has more than 10,000 pages.",
            Self::Damaged => "This PDF is damaged or has no readable pages.",
            Self::PasswordProtected => "This PDF requires a valid password.",
            Self::UnsupportedEncryption => "Reader does not support this PDF encryption method.",
            Self::MissingPage => "That page is outside this PDF.",
            Self::InvalidPageSize => "This PDF page has unsupported dimensions.",
            Self::InvalidViewport => "This PDF view is too large to draw on the Kindle.",
            Self::RenderFailed => "Reader could not draw this PDF page.",
            Self::TextExtractionFailed => "Reader could not extract text from this PDF page.",
            Self::LinkExtractionFailed => "Reader could not read links on this PDF page.",
        })
    }
}

impl std::error::Error for PdfError {}

/// Profile the real opening path without logging book names, paths, page text or passwords.
pub(crate) fn pdf_open_timing<T>(stage: &str, run: impl FnOnce() -> T) -> T {
    if std::env::var_os("READER_PDF_OPEN_TIMINGS").is_none() {
        return run();
    }
    let started = std::time::Instant::now();
    let result = run();
    eprintln!(
        "reader_pdf_open {stage}_ms={}",
        started.elapsed().as_millis()
    );
    result
}

impl Pdf {
    pub fn open(bytes: Vec<u8>) -> Result<Self, PdfError> {
        Self::open_with_password(bytes, "")
    }

    pub fn open_with_password(bytes: Vec<u8>, password: &str) -> Result<Self, PdfError> {
        if bytes.len() > MAX_PDF_BYTES {
            return Err(PdfError::TooLarge);
        }
        let header = &bytes[..bytes.len().min(1024)];
        if !header.windows(5).any(|word| word == b"%PDF-") {
            return Err(PdfError::Damaged);
        }

        let document = catch_unwind(|| Document::new_with_password(bytes, password))
            .map_err(|_| PdfError::Damaged)?
            .map_err(|error| match error {
                LoadPdfError::Decryption(DecryptionError::PasswordProtected) => {
                    PdfError::PasswordProtected
                }
                LoadPdfError::Decryption(DecryptionError::UnsupportedAlgorithm) => {
                    PdfError::UnsupportedEncryption
                }
                LoadPdfError::Decryption(
                    DecryptionError::InvalidEncryption | DecryptionError::MissingIDEntry,
                )
                | LoadPdfError::Invalid => PdfError::Damaged,
            })?;

        let count = document.pages().len();
        if count == 0 {
            return Err(PdfError::Damaged);
        }
        if count > MAX_PAGES {
            return Err(PdfError::TooManyPages);
        }
        Ok(Self {
            document,
            page_labels: OnceLock::new(),
        })
    }

    pub fn is_encrypted(&self) -> bool {
        self.document.xref().is_encrypted()
    }

    pub fn page_count(&self) -> usize {
        self.document.pages().len()
    }

    #[cfg(feature = "kindle")]
    pub(crate) fn title(&self) -> Option<String> {
        self.document
            .metadata()
            .title
            .as_deref()
            .and_then(pdf_title)
    }

    #[cfg(feature = "kindle")]
    pub(crate) fn author(&self) -> Option<String> {
        self.document
            .metadata()
            .author
            .as_deref()
            .and_then(pdf_title)
    }

    #[cfg(feature = "kindle")]
    pub(crate) fn description_fields(&self) -> (Option<String>, Option<String>, Option<String>) {
        let metadata = self.document.metadata();
        let decode = |bytes: &[u8]| pdf_text(&bytes[..bytes.len().min(16 * 1024)]);
        let xref = self.document.xref();
        let language = xref
            .get::<Dict<'_>>(xref.root_id())
            .and_then(|catalog| catalog.get::<PdfString<'_>>(b"Lang"))
            .map(|language| decode(language.as_bytes()));
        (
            metadata.subject.as_deref().map(decode),
            metadata.keywords.as_deref().map(decode),
            language,
        )
    }

    /// The printed label for a zero-based page, if the PDF provides one.
    pub fn page_label(&self, page: usize) -> Option<String> {
        if page >= self.page_count() {
            return None;
        }
        let ranges = self.page_labels.get_or_init(|| {
            catch_unwind(AssertUnwindSafe(|| read_page_labels(&self.document))).unwrap_or_default()
        });
        let index = ranges.partition_point(|range| range.first <= page);
        ranges.get(index.checked_sub(1)?)?.label(page)
    }

    /// Look up a unique printed label. Prefer an exact match before ASCII case folding.
    pub fn find_page_label(&self, input: &str) -> Option<usize> {
        let input = input.trim();
        if input.is_empty() || input.len() > MAX_TITLE_BYTES + MAX_PAGE_LABEL_CHARACTERS as usize {
            return None;
        }
        let mut exact_match = None;
        let mut folded_match = None;
        let mut folded_ambiguous = false;
        for page in 0..self.page_count() {
            let Some(label) = self.page_label(page) else {
                continue;
            };
            let label = label.trim();
            if label == input {
                if exact_match.replace(page).is_some() {
                    return None;
                }
            } else if label.eq_ignore_ascii_case(input) && folded_match.replace(page).is_some() {
                folded_ambiguous = true;
            }
        }
        exact_match.or_else(|| folded_match.filter(|_| !folded_ambiguous))
    }

    /// Outline items in visual order. Follow only destinations in this document.
    pub fn outline(&self) -> Vec<PdfOutlineItem> {
        catch_unwind(AssertUnwindSafe(|| read_outline(&self.document))).unwrap_or_default()
    }

    /// Read the embedded text and approximate span geometry for a single page.
    pub fn page_text(&self, page: usize) -> Result<PdfPageText, PdfError> {
        self.page_text_with_cache(page, &hayro_interpret::InterpreterCache::new())
    }

    pub(crate) fn page_text_with_cache<'a>(
        &'a self,
        page: usize,
        cache: &hayro_interpret::InterpreterCache<'a>,
    ) -> Result<PdfPageText, PdfError> {
        let page = self
            .document
            .pages()
            .get(page)
            .ok_or(PdfError::MissingPage)?;
        fixed_text::extract_cached(page, fixed_text::MAX_GLYPHS, cache).map_err(|error| match error
        {
            PdfTextError::InvalidPageSize => PdfError::InvalidPageSize,
            PdfTextError::InterpretFailed => PdfError::TextExtractionFailed,
        })
    }

    /// Read the page's visible link rectangles in annotation order.
    pub fn page_links(&self, page: usize) -> Result<Vec<PdfLink>, PdfError> {
        let (width, height) = self.page_size(page)?;
        let size = Rect::new(0.0, 0.0, f64::from(width), f64::from(height));
        catch_unwind(AssertUnwindSafe(|| {
            read_page_links(&self.document, page, size)
        }))
        .map_err(|_| PdfError::LinkExtractionFailed)
    }

    pub fn page_size(&self, page: usize) -> Result<(f32, f32), PdfError> {
        let page = self
            .document
            .pages()
            .get(page)
            .ok_or(PdfError::MissingPage)?;
        let (width, height) = page.render_dimensions();
        if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
            return Err(PdfError::InvalidPageSize);
        }
        Ok((width, height))
    }

    /// Complete nullable PDF coordinates against the current view or the
    /// target crop edge, then convert into the target's upright crop space.
    pub(crate) fn complete_partial_destination(
        &self,
        current_page: usize,
        current_upright: Point,
        target_page: usize,
        target: PartialPdfDestination,
    ) -> Option<Point> {
        let target_transform = self
            .document
            .pages()
            .get(target_page)?
            .initial_transform(true)
            .to_kurbo();
        let target_determinant = target_transform.determinant();
        if !current_upright.is_finite()
            || !target_determinant.is_finite()
            || target_determinant.abs() < 1e-12
        {
            return None;
        }
        let current_pdf_point = || {
            let current = self
                .document
                .pages()
                .get(current_page)?
                .initial_transform(true)
                .to_kurbo();
            let determinant = current.determinant();
            if !determinant.is_finite() || determinant.abs() < 1e-12 {
                return None;
            }
            let point = current.inverse() * current_upright;
            point.is_finite().then_some(point)
        };
        let old = match target.fit {
            // Whole-page fitting does not require a PDF coordinate.
            PdfDestinationFit::Page => return None,
            PdfDestinationFit::Current => current_pdf_point()?,
            PdfDestinationFit::Width | PdfDestinationFit::Height => {
                let [a, b, c, d, _, _] = target_transform.as_coeffs();
                // Only unrotated target pages have the PDF top/left axes
                // aligned with the upright viewport axes in this candidate.
                if a <= 0.0 || d >= 0.0 || b != 0.0 || c != 0.0 {
                    return None;
                }
                let edge = target_transform.inverse() * Point::ORIGIN;
                match target.fit {
                    PdfDestinationFit::Width if target.top.is_none() => {
                        Point::new(edge.x, current_pdf_point()?.y)
                    }
                    PdfDestinationFit::Height if target.left.is_none() => {
                        Point::new(current_pdf_point()?.x, edge.y)
                    }
                    _ => edge,
                }
            }
        };
        if !old.is_finite() {
            return None;
        }
        let point = target_transform
            * Point::new(target.left.unwrap_or(old.x), target.top.unwrap_or(old.y));
        point.is_finite().then_some(point)
    }

    #[cfg(feature = "kindle")]
    pub(crate) fn page_geometry(
        &self,
        index: usize,
    ) -> Option<crate::pdf_annotations::PageGeometry> {
        let (width, height) = self.page_size(index).ok()?;
        let page = self.document.pages().get(index)?;
        Some(crate::pdf_annotations::PageGeometry {
            width,
            height,
            upright_from_pdf: page.initial_transform(true).to_kurbo(),
        })
    }

    /// Render a centered, aspect-preserving view of the whole page on white.
    pub fn fit(
        &self,
        page: usize,
        width: u32,
        height: u32,
        turns: u8,
    ) -> Result<Vec<u8>, PdfError> {
        let (page_width, page_height) = self.page_size(page)?;
        let (page_width, page_height) = if turns.is_multiple_of(2) {
            (page_width, page_height)
        } else {
            (page_height, page_width)
        };
        let scale = (width as f32 / page_width).min(height as f32 / page_height);
        self.render(
            page,
            RenderRequest {
                width,
                height,
                scale,
                x: (width as f32 - page_width * scale).max(0.0) / 2.0,
                y: (height as f32 - page_height * scale).max(0.0) / 2.0,
                turns,
            },
        )
    }

    /// The returned frame is row-major grayscale, with exactly one byte per viewport pixel.
    pub fn render(&self, page: usize, request: RenderRequest) -> Result<Vec<u8>, PdfError> {
        self.render_transformed(page, request, Affine::IDENTITY)
    }

    pub(crate) fn render_transformed(
        &self,
        page: usize,
        request: RenderRequest,
        correction: Affine,
    ) -> Result<Vec<u8>, PdfError> {
        let page_to_viewport = request.page_to_viewport(self.page_size(page)?)? * correction;
        let page = &self.document.pages()[page];
        let transform = page_to_viewport * page.initial_transform(true).to_kurbo();

        catch_unwind(AssertUnwindSafe(|| {
            let mut context = RenderContext::new(request.width as u16, request.height as u16);
            let mut pixels = Pixmap::new(context.width(), context.height());
            // Per-frame resources keep page-specific glyphs and render caches from accumulating.
            pdf_open_timing("interpret", || {
                render_into(
                    page,
                    &RenderCache::new(),
                    &InterpreterSettings::default(),
                    &RenderSettings::default(),
                    &mut context,
                    transform,
                );
            });
            pdf_open_timing("rasterize", || {
                context.flush();
                context.render_with(
                    &mut pixels,
                    &mut Resources::default(),
                    RasterizerSettings {
                        target_init: TargetInit::Clear(WHITE),
                        ..Default::default()
                    },
                );
            });
            pixels
                .data_as_u8_slice()
                .as_chunks::<4>()
                .0
                .iter()
                .map(|rgba| {
                    // The opaque white target has already composited the document's transparency.
                    ((54 * u16::from(rgba[0])
                        + 183 * u16::from(rgba[1])
                        + 19 * u16::from(rgba[2])
                        + 128)
                        >> 8) as u8
                })
                .collect()
        }))
        .map_err(|_| PdfError::RenderFailed)
    }
}

type PageReferences = HashMap<ObjectIdentifier, usize>;

impl PageLabelRange {
    fn from_dict(first: usize, dict: Dict<'_>) -> Option<Self> {
        let style = match dict.get::<Name<'_>>(b"S").as_deref() {
            None if !dict.contains_key(b"S") => PageLabelStyle::Prefix,
            Some(b"D") => PageLabelStyle::Decimal,
            Some(b"R") => PageLabelStyle::Roman { uppercase: true },
            Some(b"r") => PageLabelStyle::Roman { uppercase: false },
            Some(b"A") => PageLabelStyle::Alphabetic { uppercase: true },
            Some(b"a") => PageLabelStyle::Alphabetic { uppercase: false },
            _ => return None,
        };
        let start = if dict.contains_key(b"St") {
            u32::try_from(dict.get::<i32>(b"St")?)
                .ok()
                .filter(|n| *n > 0)?
        } else {
            1
        };
        let prefix = if dict.contains_key(b"P") {
            let prefix = dict.get::<PdfString<'_>>(b"P")?;
            if prefix.len() > MAX_TITLE_BYTES {
                return None;
            }
            let prefix: String = pdf_text(prefix.as_bytes())
                .chars()
                .filter(|character| !character.is_control())
                .collect();
            if prefix.chars().count() > MAX_PAGE_LABEL_CHARACTERS as usize {
                return None;
            }
            prefix
        } else {
            String::new()
        };
        Some(Self {
            first,
            start,
            prefix,
            style,
        })
    }

    fn label(&self, page: usize) -> Option<String> {
        let number = self
            .start
            .checked_add(u32::try_from(page - self.first).ok()?)?;
        let mut label = self.prefix.clone();
        match self.style {
            PageLabelStyle::Prefix => (),
            PageLabelStyle::Decimal => label.push_str(&number.to_string()),
            PageLabelStyle::Roman { uppercase } => {
                let mut digits = roman_page_number(number)?;
                if !uppercase {
                    digits.make_ascii_lowercase();
                }
                label.push_str(&digits);
            }
            PageLabelStyle::Alphabetic { uppercase } => {
                // PDF labels repeat one letter: A … Z, AA, BB … ZZ, AAA …
                let index = number - 1;
                let repeats = index / 26 + 1;
                if repeats > MAX_PAGE_LABEL_CHARACTERS {
                    return None;
                }
                let base = if uppercase { b'A' } else { b'a' };
                let letter = char::from(base + (index % 26) as u8);
                label.extend(std::iter::repeat_n(letter, repeats as usize));
            }
        }
        (!label.trim().is_empty()).then_some(label)
    }
}

pub(crate) fn roman_page_number(mut value: u32) -> Option<String> {
    const DIGITS: [(u32, &str); 13] = [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    if value / 1000 > MAX_PAGE_LABEL_CHARACTERS {
        return None;
    }
    let mut output = String::new();
    for (unit, digit) in DIGITS {
        for _ in 0..value / unit {
            output.push_str(digit);
        }
        value %= unit;
    }
    (output.len() <= MAX_PAGE_LABEL_CHARACTERS as usize).then_some(output)
}

fn read_page_labels(document: &Document) -> Vec<PageLabelRange> {
    let xref = document.xref();
    let Some(root) = xref
        .get::<Dict<'_>>(xref.root_id())
        .and_then(|catalog| catalog.get_raw::<Object<'_>>(b"PageLabels"))
    else {
        return Vec::new();
    };
    let mut ranges = Vec::new();
    let mut seen = HashSet::new();
    let mut pending = vec![(root, 0)];
    let mut nodes = 0;
    let mut pairs = 0;
    while let Some((node, depth)) = pending.pop() {
        if nodes == MAX_PAGE_LABEL_NODES || pairs == MAX_PAGES {
            break;
        }
        nodes += 1;
        if node.as_obj_ref().is_some_and(|id| !seen.insert(id)) {
            continue;
        }
        let Some(node) = resolve_object(xref, node).and_then(Object::into_dict) else {
            continue;
        };
        if let Some(labels) = node.get::<Array<'_>>(b"Nums") {
            let mut labels = labels.raw_iter();
            while pairs < MAX_PAGES {
                let (Some(first), Some(label)) = (labels.next(), labels.next()) else {
                    break;
                };
                pairs += 1;
                if let Some(first) = resolve_object(xref, first)
                    .and_then(Object::into_i32)
                    .and_then(|first| usize::try_from(first).ok())
                    && first < document.pages().len()
                    && let Some(label) = resolve_object(xref, label).and_then(Object::into_dict)
                    && let Some(range) = PageLabelRange::from_dict(first, label)
                {
                    ranges.push(range);
                }
            }
        }
        if depth + 1 < MAX_PAGE_LABEL_DEPTH
            && let Some(kids) = node.get::<Array<'_>>(b"Kids")
        {
            let first = pending.len();
            let room = MAX_PAGE_LABEL_NODES - nodes - first;
            pending.extend(kids.raw_iter().take(room).map(|kid| (kid, depth + 1)));
            pending[first..].reverse();
        }
    }
    ranges.sort_by_key(|range| range.first);
    ranges.dedup_by_key(|range| range.first);
    ranges
}

fn read_page_links(document: &Document, page: usize, viewport: Rect) -> Vec<PdfLink> {
    let page = &document.pages()[page];
    let Some(annotations) = page.raw().get::<Array<'_>>(b"Annots") else {
        return Vec::new();
    };
    let xref = document.xref();
    let Some(catalog) = xref.get::<Dict<'_>>(xref.root_id()) else {
        return Vec::new();
    };
    let transform = page.initial_transform(true).to_kurbo();
    let mut destinations = DestinationResolver::new(document, catalog);
    annotations
        .raw_iter()
        .take(MAX_PAGE_ANNOTATIONS)
        .filter_map(|item| resolve_object(xref, item).and_then(Object::into_dict))
        .filter_map(|annotation| {
            if annotation.get::<Name<'_>>(b"Subtype")?.as_ref() != b"Link"
                || annotation.get::<u32>(b"F").unwrap_or_default() & HIDDEN_LINK_FLAGS != 0
            {
                return None;
            }
            let rect = annotation.get::<PdfRect>(b"Rect")?.to_kurbo();
            if !rect.is_finite() || rect.width() <= 0.0 || rect.height() <= 0.0 {
                return None;
            }
            let rect = transform.transform_rect_bbox(rect);
            if !rect.is_finite() {
                return None;
            }
            let rect = rect.intersect(viewport);
            if rect.width() <= 0.0 || rect.height() <= 0.0 {
                return None;
            }
            let (target, destination_point, destination_partial) = destinations.target(&annotation);
            Some(PdfLink {
                bounds: PdfTextRect {
                    x0: (rect.x0 / viewport.width()) as f32,
                    y0: (rect.y0 / viewport.height()) as f32,
                    x1: (rect.x1 / viewport.width()) as f32,
                    y1: (rect.y1 / viewport.height()) as f32,
                },
                uri: if target == PdfLinkTarget::External {
                    annotation_uri(&annotation)
                } else {
                    None
                },
                target,
                destination_point,
                destination_partial,
            })
        })
        .collect()
}

fn annotation_uri(annotation: &Dict<'_>) -> Option<String> {
    let action = annotation.get::<Dict<'_>>(b"A")?;
    if action.get::<Name<'_>>(b"S")?.as_ref() != b"URI" {
        return None;
    }
    let bytes = action.get::<PdfString<'_>>(b"URI")?;
    if bytes.is_empty() || bytes.len() > 8192 {
        return None;
    }
    let address = if bytes.starts_with(b"\xfe\xff") || bytes.starts_with(b"\xef\xbb\xbf") {
        pdf_text(&bytes)
    } else {
        std::str::from_utf8(&bytes)
            .map(str::to_owned)
            .unwrap_or_else(|_| pdf_text(&bytes))
    };
    if address.trim().is_empty() || address.chars().any(char::is_control) {
        return None;
    }
    Some(address)
}

fn read_outline(document: &Document) -> Vec<PdfOutlineItem> {
    let xref = document.xref();
    let Some(catalog) = xref.get::<Dict<'_>>(xref.root_id()) else {
        return Vec::new();
    };
    let Some(first) = catalog
        .get::<Dict<'_>>(b"Outlines")
        .and_then(|outline| outline.get_ref(b"First"))
    else {
        return Vec::new();
    };
    let mut destinations = DestinationResolver::new(document, catalog);
    let mut visited = HashSet::new();
    let mut pending = vec![(first.into(), 0)];
    let mut rows = Vec::new();
    while let Some((id, depth)) = pending.pop() {
        if visited.len() == MAX_OUTLINE_ITEMS {
            break;
        }
        if !visited.insert(id) {
            continue;
        }
        let Some(item) = xref.get::<Dict<'_>>(id) else {
            continue;
        };
        if let Some(next) = item.get_ref(b"Next") {
            pending.push((next.into(), depth));
        }
        if depth + 1 < MAX_OUTLINE_DEPTH
            && let Some(child) = item.get_ref(b"First")
        {
            pending.push((child.into(), depth + 1));
        }
        let Some(title) = item
            .get::<PdfString<'_>>(b"Title")
            .and_then(|text| pdf_title(&text))
        else {
            continue;
        };
        let (target, destination_point, destination_partial) = destinations.target(&item);
        let page = match target {
            PdfLinkTarget::Page(page) => Some(page),
            PdfLinkTarget::External | PdfLinkTarget::Unavailable => None,
        };
        rows.push(PdfOutlineItem {
            title,
            page,
            depth,
            destination_point,
            destination_partial,
        });
    }
    rows
}

#[derive(Clone, Copy)]
enum DestinationPosition {
    Point(Point),
    Rectangle(Rect),
    Partial(PartialPdfDestination),
}

#[derive(Clone, Copy)]
struct PageDestination {
    page: usize,
    pdf_position: Option<DestinationPosition>,
}

struct DestinationResolver<'a> {
    catalog: Dict<'a>,
    legacy_names: Option<Dict<'a>>,
    document: &'a Document,
    pages: Option<PageReferences>,
    tree_names: Option<HashMap<Vec<u8>, PageDestination>>,
}

impl<'a> DestinationResolver<'a> {
    fn new(document: &'a Document, catalog: Dict<'a>) -> Self {
        Self {
            legacy_names: catalog.get(b"Dests"),
            catalog,
            document,
            pages: None,
            tree_names: None,
        }
    }

    fn target(
        &mut self,
        item: &Dict<'a>,
    ) -> (PdfLinkTarget, Option<Point>, Option<PartialPdfDestination>) {
        let destination = if let Some(destination) = item.get(b"Dest") {
            Some(destination)
        } else if let Some(action) = item.get::<Dict<'_>>(b"A") {
            match action.get::<Name<'_>>(b"S").as_deref() {
                Some(b"GoTo") => action.get(b"D"),
                Some(b"URI" | b"GoToR" | b"GoToE" | b"Launch") => {
                    return (PdfLinkTarget::External, None, None);
                }
                _ => None,
            }
        } else {
            None
        };
        let Some(target) = destination.and_then(|destination| self.resolve(&destination)) else {
            return (PdfLinkTarget::Unavailable, None, None);
        };
        let partial = match target.pdf_position {
            Some(DestinationPosition::Partial(value)) => Some(value),
            _ => None,
        };
        let upright = target.pdf_position.and_then(|position| {
            let page = self.document.pages().get(target.page)?;
            let transform = page.initial_transform(true).to_kurbo();
            let point = match position {
                DestinationPosition::Point(point) => transform * point,
                DestinationPosition::Rectangle(rect) => {
                    let rect = transform.transform_rect_bbox(rect);
                    Point::new(rect.x0, rect.y0)
                }
                DestinationPosition::Partial(_) => return None,
            };
            point.is_finite().then_some(point)
        });
        (PdfLinkTarget::Page(target.page), upright, partial)
    }

    fn resolve(&mut self, destination: &Object<'a>) -> Option<PageDestination> {
        let pages = self.pages.get_or_insert_with(|| {
            self.document
                .pages()
                .iter()
                .enumerate()
                .filter_map(|(index, page)| page.raw().obj_id().map(|id| (id, index)))
                .collect()
        });
        let name = match destination {
            Object::Name(name) => name.as_ref(),
            Object::String(name) => name.as_ref(),
            _ => {
                return explicit_destination(
                    destination,
                    pages,
                    self.document.pages().len(),
                    self.document.xref(),
                );
            }
        };
        if name.len() > MAX_DESTINATION_NAME {
            return None;
        }
        let legacy_page = || {
            self.legacy_names
                .as_ref()?
                .get::<Object<'_>>(name)
                .and_then(|value| {
                    explicit_destination(
                        &value,
                        pages,
                        self.document.pages().len(),
                        self.document.xref(),
                    )
                })
        };
        let mut tree_page = || {
            self.tree_names
                .get_or_insert_with(|| {
                    read_named_destinations(
                        &self.catalog,
                        self.document.xref(),
                        pages,
                        self.document.pages().len(),
                    )
                })
                .get(name)
                .copied()
        };
        if matches!(destination, Object::Name(_)) {
            legacy_page().or_else(tree_page)
        } else {
            tree_page().or_else(legacy_page)
        }
    }
}

fn explicit_destination(
    destination: &Object<'_>,
    pages: &PageReferences,
    count: usize,
    xref: &XRef,
) -> Option<PageDestination> {
    let stored;
    let array = match destination {
        Object::Array(array) => array,
        Object::Dict(dict) => {
            stored = dict.get::<Array<'_>>(b"D")?;
            &stored
        }
        _ => return None,
    };
    let mut values = array.raw_iter();
    let first = values.next()?;
    let page = if let Some(reference) = first.as_obj_ref() {
        *pages.get(&reference.into())?
    } else if let MaybeRef::NotRef(object) = first {
        // Hayro represents integer and real PDF values in a shared Number.
        // Keep page indices restricted to integer objects without truncating a real.
        let number = object.into_number()?;
        let index = usize::try_from(number.as_i64()).ok()?;
        if index >= count
            || number != hayro::hayro_syntax::object::Number::from_i32(i32::try_from(index).ok()?)
        {
            return None;
        }
        index
    } else {
        return None;
    };
    let mut read = || resolve_object(xref, values.next()?);
    let pdf_position = (|| {
        let kind = read()?.into_name()?;
        match kind.as_ref() {
            b"XYZ" => {
                let left = read()?;
                let top = read()?;
                // Keep the previously supported complete-point path unchanged.
                if let (Some(left), Some(top)) = (left.clone().into_f32(), top.clone().into_f32()) {
                    let point = Point::new(f64::from(left), f64::from(top));
                    point
                        .is_finite()
                        .then_some(DestinationPosition::Point(point))
                } else {
                    let coordinate = |value: Object<'_>| -> Option<Option<f64>> {
                        match value {
                            Object::Null(_) => Some(None),
                            value => {
                                let number = f64::from(value.into_f32()?);
                                number.is_finite().then_some(Some(number))
                            }
                        }
                    };
                    let left = coordinate(left)?;
                    let top = coordinate(top)?;
                    // Only null/zero zoom preserves the current zoom. Other
                    // zooms need a separate viewport policy and stay page-only.
                    let zoom = read()?;
                    let same_zoom = match zoom {
                        Object::Null(_) => true,
                        value => value.into_f32().is_some_and(|value| value == 0.0),
                    };
                    same_zoom.then_some(DestinationPosition::Partial(PartialPdfDestination {
                        left,
                        top,
                        fit: PdfDestinationFit::Current,
                    }))
                }
            }
            b"Fit" => Some(DestinationPosition::Partial(PartialPdfDestination {
                left: None,
                top: None,
                fit: PdfDestinationFit::Page,
            })),
            b"FitH" | b"FitV" => {
                let value = match read()? {
                    Object::Null(_) => None,
                    value => {
                        let value = f64::from(value.into_f32()?);
                        if !value.is_finite() {
                            return None;
                        }
                        Some(value)
                    }
                };
                let horizontal = kind.as_ref() == b"FitH";
                Some(DestinationPosition::Partial(PartialPdfDestination {
                    left: if horizontal { None } else { value },
                    top: if horizontal { value } else { None },
                    fit: if horizontal {
                        PdfDestinationFit::Width
                    } else {
                        PdfDestinationFit::Height
                    },
                }))
            }
            b"FitR" => {
                let left = f64::from(read()?.into_f32()?);
                let bottom = f64::from(read()?.into_f32()?);
                let right = f64::from(read()?.into_f32()?);
                let top = f64::from(read()?.into_f32()?);
                let rect = Rect::new(left, bottom, right, top);
                (rect.is_finite() && left < right && bottom < top)
                    .then_some(DestinationPosition::Rectangle(rect))
            }
            _ => None,
        }
    })();
    Some(PageDestination { page, pdf_position })
}

fn read_named_destinations<'a>(
    catalog: &Dict<'a>,
    xref: &'a XRef,
    pages: &PageReferences,
    count: usize,
) -> HashMap<Vec<u8>, PageDestination> {
    let Some(root) = catalog
        .get::<Dict<'_>>(b"Names")
        .and_then(|names| names.get::<Dict<'_>>(b"Dests"))
    else {
        return HashMap::new();
    };
    let mut output = HashMap::new();
    let mut nodes = 0;
    let mut pairs = 0;
    let mut visited = HashSet::new();
    let mut stack = vec![(root, 0)];
    while let Some((node, depth)) = stack.pop() {
        if nodes == MAX_NAME_TREE_NODES || pairs == MAX_NAMED_DESTINATIONS {
            break;
        }
        nodes += 1;
        if node.obj_id().is_some_and(|id| !visited.insert(id)) {
            continue;
        }
        if let Some(items) = node.get::<Array<'_>>(b"Names") {
            let mut items = items.raw_iter();
            while pairs < MAX_NAMED_DESTINATIONS {
                let (Some(key), Some(value)) = (items.next(), items.next()) else {
                    break;
                };
                pairs += 1;
                if let Some(key) = resolve_object(xref, key).and_then(Object::into_string)
                    && key.len() <= MAX_DESTINATION_NAME
                    && let Some(page) = resolve_object(xref, value)
                        .and_then(|object| explicit_destination(&object, pages, count, xref))
                {
                    output.entry(key.as_bytes().to_vec()).or_insert(page);
                }
            }
        }
        if depth + 1 < MAX_OUTLINE_DEPTH
            && let Some(kids) = node.get::<Array<'_>>(b"Kids")
        {
            let room = MAX_NAME_TREE_NODES.saturating_sub(nodes + stack.len());
            stack.extend(kids.raw_iter().take(room).filter_map(|child| {
                resolve_object(xref, child)
                    .and_then(Object::into_dict)
                    .map(|child| (child, depth + 1))
            }));
        }
    }
    output
}

fn resolve_object<'a>(xref: &'a XRef, object: MaybeRef<Object<'a>>) -> Option<Object<'a>> {
    match object {
        MaybeRef::Ref(id) => xref.get(id.into()),
        MaybeRef::NotRef(object) => Some(object),
    }
}

fn pdf_title(bytes: &[u8]) -> Option<String> {
    let bytes = &bytes[..bytes.len().min(MAX_TITLE_BYTES)];
    let title: String = pdf_text(bytes)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .filter(|char| !char.is_control())
        .take(256)
        .collect();
    (!title.is_empty()).then_some(title)
}

fn pdf_text(bytes: &[u8]) -> String {
    if let Some(bytes) = bytes.strip_prefix(b"\xfe\xff") {
        char::decode_utf16(
            bytes
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| u16::from_be_bytes([pair[0], pair[1]])),
        )
        .map(|value| value.unwrap_or(char::REPLACEMENT_CHARACTER))
        .collect()
    } else if let Some(bytes) = bytes.strip_prefix(b"\xef\xbb\xbf") {
        String::from_utf8_lossy(bytes).into_owned()
    } else {
        bytes
            .iter()
            .copied()
            .map(hayro::hayro_syntax::object::pdf_doc_char)
            .collect()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn protected_pdf_authenticates_standard_roles_and_renders_the_plain_reference() {
        let plain_bytes = include_bytes!("../tests/fixtures/pdf-password/plain.pdf");
        let plain = Pdf::open(plain_bytes.to_vec()).unwrap();
        let frame = RenderRequest {
            width: 320,
            height: 480,
            scale: 1.0,
            x: 0.0,
            y: 0.0,
            turns: 0,
        };
        let pixels = (0..3)
            .map(|page| plain.render(page, frame).unwrap())
            .collect::<Vec<_>>();
        let secret = format!("estuary{}river", "n".repeat(88));
        for (bytes, user, owner) in [
            (
                include_bytes!("../tests/fixtures/pdf-password/r2.pdf").as_slice(),
                "survey-entry",
                "survey-editor",
            ),
            (
                include_bytes!("../tests/fixtures/pdf-password/r3.pdf").as_slice(),
                "survey-entry",
                "survey-editor",
            ),
            (
                include_bytes!("../tests/fixtures/pdf-password/r4.pdf").as_slice(),
                "survey-entry",
                "survey-editor",
            ),
            (
                include_bytes!("../tests/fixtures/pdf-password/r5.pdf").as_slice(),
                "survey-entry",
                "survey-editor",
            ),
            (
                include_bytes!("../tests/fixtures/pdf-password/r6.pdf").as_slice(),
                "survey-entry",
                "survey-editor",
            ),
            (
                include_bytes!("../tests/fixtures/pdf-password/r6-long.pdf").as_slice(),
                secret.as_str(),
                "survey-editor",
            ),
            (
                include_bytes!("../tests/fixtures/pdf-password/r4-owner-only.pdf").as_slice(),
                "",
                "survey-editor",
            ),
            (
                include_bytes!("../tests/fixtures/pdf-password/r4-latin.pdf").as_slice(),
                "été",
                "côté",
            ),
            (
                include_bytes!("../tests/fixtures/pdf-password/r6-latin.pdf").as_slice(),
                "été",
                "côté",
            ),
            (plain_bytes.as_slice(), "survey-entry", "survey-editor"),
        ] {
            let encrypted = bytes != plain_bytes;
            for (index, password) in ["", "wrong", user, owner].iter().enumerate() {
                let result = Pdf::open_with_password(bytes.to_vec(), password);
                if encrypted && (index == 1 || index == 0 && !user.is_empty()) {
                    assert!(matches!(result, Err(PdfError::PasswordProtected)));
                    continue;
                }
                let pdf = result.unwrap();
                assert_eq!(pdf.is_encrypted(), encrypted);
                assert_eq!(pdf.page_count(), 3);
                assert_eq!(pdf.outline(), plain.outline());
                for (page, expected) in pixels.iter().enumerate() {
                    assert_eq!(pdf.page_text(page).unwrap(), plain.page_text(page).unwrap());
                    assert!(pdf.render(page, frame).unwrap() == *expected);
                }
            }
        }
    }

    #[test]
    fn protected_pdf_reports_an_unsupported_handler_without_a_password_retry() {
        let mut bytes = include_bytes!("../tests/fixtures/pdf-password/r4.pdf").to_vec();
        let filter = b"/Filter /Standard";
        let start = bytes
            .windows(filter.len())
            .position(|part| part == filter)
            .unwrap();
        bytes[start..start + filter.len()].copy_from_slice(b"/Filter /ForeignX");
        assert!(matches!(
            Pdf::open(bytes),
            Err(PdfError::UnsupportedEncryption)
        ));
    }

    pub(crate) fn document() -> Vec<u8> {
        let content = "0 g\n10 60 20 15 re f\nBT /F1 14 Tf 55 60 Td (PDF) Tj ET\n";
        let objects = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
            "<< /Type /Pages /Count 2 /Kids [3 0 R 4 0 R] /Resources << /Font << /F1 6 0 R >> >> >>"
                .to_owned(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 120 80] /Contents 5 0 R >>".to_owned(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 120 80] /Rotate 90 /Contents 5 0 R >>"
                .to_owned(),
            format!(
                "<< /Length {} >>\nstream\n{}endstream",
                content.len(),
                content
            ),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_owned(),
        ];
        pdf_objects(&objects)
    }

    pub(crate) fn pdf_objects(objects: &[impl AsRef<str>]) -> Vec<u8> {
        let mut pdf = b"%PDF-1.4\n".to_vec();
        let mut offsets = Vec::new();
        for (i, object) in objects.iter().enumerate() {
            offsets.push(pdf.len());
            writeln!(pdf, "{} 0 obj\n{}\nendobj", i + 1, object.as_ref()).unwrap();
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
        pdf
    }

    fn labeled_document(pages: usize, labels: &[&str]) -> Pdf {
        let root = pages + 3;
        let kids = (3..root)
            .map(|number| format!("{number} 0 R"))
            .collect::<Vec<_>>()
            .join(" ");
        let mut objects = vec![
            format!("<< /Type /Catalog /Pages 2 0 R /PageLabels {root} 0 R >>"),
            format!("<< /Type /Pages /Count {pages} /Kids [{kids}] >>"),
        ];
        objects.extend(std::iter::repeat_n(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 120 80] >>".to_owned(),
            pages,
        ));
        objects.extend(labels.iter().map(|label| (*label).to_owned()));
        Pdf::open(pdf_objects(&objects)).unwrap()
    }

    #[test]
    fn printer_color_chart_keeps_its_swatches_and_vector_registration_line() {
        let bytes = include_bytes!("../tests/fixtures/cmyk-print-chart.pdf").to_vec();
        let document = Pdf::open(bytes).unwrap();
        assert_eq!(document.page_size(0).unwrap(), (544.0, 416.0));
        let frame = document.fit(0, 544, 416, 0).unwrap();
        let gray = |x: usize, y: usize| frame[y * 544 + x];
        assert!(gray(37, 33) > 235, "paper");
        assert!(gray(105, 33) < 100, "black ink");
        assert!(
            gray(170, 33).saturating_add(5) < gray(300, 33),
            "cyan and yellow"
        );
        assert_eq!(gray(272, 208), 255, "white vector overprint");
        assert_eq!(gray(4, 4), 255, "paper margin");
    }

    #[test]
    fn renders_printed_page_labels_from_nested_number_tree() {
        let pdf = labeled_document(
            14,
            &[
                "<< /Kids [18 0 R 19 0 R] >>",
                "<< /Limits [0 7] /Nums [0 << /S /r /St 3 >> 3 << /S /A /St 26 >> 6 << /S /a /St 52 >> 7 << /S /D /P ( Ch- ) /St 7 >>] >>",
                "<< /Limits [8 13] /Kids [17 0 R 20 0 R] >>",
                "<< /Limits [8 13] /Nums [8 << /S /R /St 9 >> 9 << /S /r /St 9 >> 10 21 0 R 12 << /S /D >> 13 << /S /R /St 2147483647 >>] >>",
                "<< /P <FEFF03A9> >>",
            ],
        );
        let labels: Vec<_> = (0..pdf.page_count())
            .map(|page| pdf.page_label(page))
            .collect();
        let expected = [
            Some("iii"),
            Some("iv"),
            Some("v"),
            Some("Z"),
            Some("AA"),
            Some("BB"),
            Some("zz"),
            Some(" Ch- 7"),
            Some("IX"),
            Some("ix"),
            Some("Ω"),
            Some("Ω"),
            Some("1"),
            None,
        ];
        assert_eq!(
            labels.iter().map(Option::as_deref).collect::<Vec<_>>(),
            expected
        );
        assert_eq!(pdf.page_label(14), None);
        assert_eq!(pdf.find_page_label("  ch- 7  "), Some(7));
        assert_eq!(pdf.find_page_label("IX"), Some(8));
        assert_eq!(pdf.find_page_label("ix"), Some(9));
        assert_eq!(pdf.find_page_label("Ix"), None);
        assert_eq!(pdf.find_page_label("Ω"), None);
    }

    #[test]
    fn invalid_or_missing_page_labels_leave_physical_navigation_available() {
        let ordinary_pdf = Pdf::open(document()).unwrap();
        assert_eq!(ordinary_pdf.page_label(0), None);
        assert_eq!(ordinary_pdf.find_page_label("1"), None);

        let malformed_pdf = labeled_document(
            4,
            &[
                "<< /Nums [-1 << /S /D >> 0 << /S /d >> 1 << /S /D /St 0 >> 2 << /P ( ) >> 3 << /P (End) >> 4 << /S /D >> 5] >>",
            ],
        );
        assert_eq!(malformed_pdf.page_label(0), None);
        assert_eq!(malformed_pdf.page_label(1), None);
        assert_eq!(malformed_pdf.page_label(2), None);
        assert_eq!(malformed_pdf.page_label(3).as_deref(), Some("End"));
        assert_eq!(malformed_pdf.find_page_label("end"), Some(3));
    }

    #[test]
    fn outline_resolves_nested_explicit_and_named_local_pages() {
        let pdf = Pdf::open(pdf_objects(&[
            "<< /Type /Catalog /Pages 2 0 R /Outlines 5 0 R /Dests << /legacy << /D [3 0 R /Fit] >> >> /Names << /Dests 12 0 R >> >>",
            "<< /Type /Pages /Count 2 /Kids [4 0 R 3 0 R] >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 120 80] >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 120 80] >>",
            "<< /Type /Outlines /First 6 0 R /Last 9 0 R >>",
            "<< /Title (Opening \\205 Introduction) /Dest [4 0 R /Fit] /First 7 0 R /Next 9 0 R >>",
            "<< /Title <FEFF0054006900640065002003A9> /A << /S /GoTo /D (child) >> /Next 8 0 R >>",
            "<< /Title (Appendix) /Dest /legacy >>",
            "<< /Title (More) /Next 6 0 R /First 10 0 R >>",
            "<< /Title (Remote) /A << /S /GoToR /D [3 0 R /Fit] /F (remote.pdf) >> /Next 11 0 R >>",
            "<< /Title (Missing) /Dest [40 0 R /Fit] >>",
            "<< /Kids [13 0 R 12 0 R] >>",
            "<< /Limits [(child) (child)] /Names [(child) 14 0 R] >>",
            "<< /D [3 0 R /XYZ 0 100 null] >>",
        ])).unwrap();
        let outline = pdf.outline();
        let rows = outline
            .iter()
            .map(|item| (item.title.as_str(), item.page, item.depth))
            .collect::<Vec<_>>();
        assert_eq!(
            rows,
            [
                ("Opening – Introduction", Some(0), 0),
                ("Tide Ω", Some(1), 1),
                ("Appendix", Some(1), 1),
                ("More", None, 0),
                ("Remote", None, 1),
                ("Missing", None, 1),
            ]
        );
    }

    #[test]
    fn outline_caps_depth_and_item_count() {
        let build = |link: &str, count: usize| {
            let mut objects = vec![
                "<< /Type /Catalog /Pages 2 0 R /Outlines 4 0 R >>".into(),
                "<< /Type /Pages /Count 1 /Kids [3 0 R] >>".into(),
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 120 80] >>".into(),
                "<< /Type /Outlines /First 5 0 R >>".into(),
            ];
            for index in 0..count {
                objects.push(format!(
                    "<< /Title (Node {index}) /Dest [3 0 R /Fit] /{link} {} 0 R >>",
                    index + 6
                ));
            }
            Pdf::open(pdf_objects(&objects)).unwrap()
        };
        let deep = build("First", MAX_OUTLINE_DEPTH + 3).outline();
        assert_eq!(deep.len(), MAX_OUTLINE_DEPTH);
        assert_eq!(deep.last().unwrap().depth, MAX_OUTLINE_DEPTH - 1);
        let broad = build("Next", MAX_OUTLINE_ITEMS + 3).outline();
        assert_eq!(broad.len(), MAX_OUTLINE_ITEMS);
        assert!(
            broad
                .iter()
                .all(|item| item.depth == 0 && item.page == Some(0))
        );
    }

    #[test]
    fn page_links_classify_local_and_external_targets_and_align_to_the_cropped_page() {
        let pdf = Pdf::open(pdf_objects(&[
            "<< /Type /Catalog /Pages 2 0 R /Dests << /legacy [4 0 R /Fit] >> /Names << /Dests 6 0 R >> >>",
            "<< /Type /Pages /Count 2 /Kids [3 0 R 4 0 R] >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 120 80] /CropBox [10 20 110 70] /Rotate 90 /Annots 5 0 R >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 120 80] >>",
            "[7 0 R
                << /Subtype /Link /Rect [30 30 36 36] /Dest /legacy >>
                << /Subtype /Link /Rect [36 36 42 42] /A << /S /GoTo /D (tree) >> >>
                << /Subtype /Link /Rect [42 42 48 48] /A << /S /URI /URI (https://example.invalid/) >> >>
                << /Subtype /Link /Rect [48 48 54 54] /A << /S /GoToR /D [4 0 R /Fit] /F (other.pdf) >> >>
                << /Subtype /Link /Rect [54 54 60 60] /A << /S /JavaScript >> >>
                << /Subtype /Link /Rect [60 60 66 66] /Dest [40 0 R /Fit] >>
                << /Subtype /Link /Rect [66 60 70 66] /Dest /missing >>
                << /Subtype /Link /Rect [20 20 24 24] /F 2 /Dest [4 0 R /Fit] >>
                << /Subtype /Link /Rect [24 24 28 28] /F 32 /Dest [4 0 R /Fit] >>
                << /Subtype /Link /Rect [0 0 5 5] /Dest [4 0 R /Fit] >>
                << /Subtype /Link /Rect [30 30 30 34] /Dest [4 0 R /Fit] >>
                << /Subtype /Text /Rect [40 40 44 44] /Dest [4 0 R /Fit] >>
                42
            ]",
            "<< /Names [(tree) [3 0 R /Fit]] >>",
            "<< /Subtype /Link /Rect [5 15 30 40] /Dest [4 0 R /Fit] >>",
        ])).unwrap();
        let links = pdf.page_links(0).unwrap();
        let targets: Vec<_> = links.iter().map(|link| link.target).collect();
        assert_eq!(
            targets,
            [
                PdfLinkTarget::Page(1),
                PdfLinkTarget::Page(1),
                PdfLinkTarget::Page(0),
                PdfLinkTarget::External,
                PdfLinkTarget::External,
                PdfLinkTarget::Unavailable,
                PdfLinkTarget::Unavailable,
                PdfLinkTarget::Unavailable,
            ]
        );
        assert_eq!(links[3].uri.as_deref(), Some("https://example.invalid/"));
        assert_eq!(links[4].uri, None);
        assert!(pdf.page_links(1).unwrap().is_empty());
        assert_eq!(pdf.page_links(2), Err(PdfError::MissingPage));

        assert_eq!(
            links[0].bounds,
            PdfTextRect {
                x0: 0.0,
                y0: 0.0,
                x1: 0.4,
                y1: 0.2
            }
        );
        let clipped = links[0]
            .bounds
            .in_viewport(
                pdf.page_size(0).unwrap(),
                RenderRequest {
                    width: 24,
                    height: 26,
                    scale: 2.0,
                    x: -10.0,
                    y: -70.0,
                    turns: 3,
                },
            )
            .unwrap();
        assert_eq!(clipped, Rect::new(0.0, 0.0, 24.0, 26.0));
    }

    #[test]
    fn xyz_links_resolve_points_in_the_target_crop_and_orientation() {
        let pdf = Pdf::open(pdf_objects(&[
            "<< /Type /Catalog /Pages 2 0 R /Names << /Dests << /Names [(named) << /D [4 0 R /XYZ 120 500 0] >>] >> >> >>",
            "<< /Type /Pages /Count 2 /Kids [3 0 R 4 0 R] >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Annots 5 0 R >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 500 800] /CropBox [50 70 450 670] /Rotate 90 >>",
            "[<< /Subtype /Link /Rect [5 5 10 10] /Dest [4 0 R /XYZ 120 500 null] >>
              << /Subtype /Link /Rect [15 5 20 10] /A << /S /GoTo /D (named) >> >>
              << /Subtype /Link /Rect [25 5 30 10] /Dest [null /XYZ 120 500 null] >>
              << /Subtype /Link /Rect [35 5 40 10] /Dest [4 0 R /XYZ null 500 null] >>
              << /Subtype /Link /Rect [45 5 50 10] /Dest [4 0 R /Fit] >>]",
        ])).unwrap();
        let links = pdf.page_links(0).unwrap();
        assert_eq!(links.len(), 5);
        for link in &links[..2] {
            assert_eq!(link.target, PdfLinkTarget::Page(1));
            assert_eq!(link.destination_point, Some(Point::new(430.0, 70.0)));
        }
        assert_eq!(links[2].target, PdfLinkTarget::Unavailable);
        assert_eq!(links[2].destination_point, None);
        for link in &links[3..] {
            assert_eq!(link.target, PdfLinkTarget::Page(1));
            assert_eq!(link.destination_point, None);
        }
    }

    #[test]
    fn local_page_indices_and_fitr_links_keep_cropped_rotated_destination() {
        let pdf = Pdf::open(pdf_objects(&[
            "<< /Type /Catalog /Pages 2 0 R /Names << /Dests << /Names [(region) [1 /FitR 120 400 220 500]] >> >> /Outlines 6 0 R >>",
            "<< /Type /Pages /Count 2 /Kids [3 0 R 4 0 R] >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Annots 5 0 R >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 500 800] /CropBox [50 70 450 670] /Rotate 90 >>",
            "[<< /Subtype /Link /Rect [5 5 10 10] /Dest [1 /FitR 120 400 220 500] >>
              << /Subtype /Link /Rect [15 5 20 10] /Dest [4 0 R /FitR 120 400 220 500] >>
              << /Subtype /Link /Rect [25 5 30 10] /Dest (region) >>
              << /Subtype /Link /Rect [35 5 40 10] /Dest [2 /Fit] >>
              << /Subtype /Link /Rect [45 5 50 10] /Dest [-1 /Fit] >>
              << /Subtype /Link /Rect [55 5 60 10] /Dest [1 /FitR 220 400 120 500] >>]",
            "<< /Type /Outlines /First 7 0 R >>",
            "<< /Title (Indexed) /Dest [1 /Fit] >>",
        ])).unwrap();
        let links = pdf.page_links(0).unwrap();
        assert_eq!(links.len(), 6);
        for link in &links[..3] {
            assert_eq!(link.target, PdfLinkTarget::Page(1));
            assert_eq!(link.destination_point, Some(Point::new(330.0, 70.0)));
        }
        assert_eq!(links[3].target, PdfLinkTarget::Unavailable);
        assert_eq!(links[4].target, PdfLinkTarget::Unavailable);
        assert_eq!(links[5].target, PdfLinkTarget::Page(1));
        assert_eq!(links[5].destination_point, None);
        assert_eq!(pdf.outline()[0].page, Some(1));
    }

    #[test]
    fn page_links_cap_annotation_entries_before_resolving_them() {
        let ignored = "0 ".repeat(MAX_PAGE_ANNOTATIONS - 1);
        let link = "<< /Subtype /Link /Rect [1 1 10 10] /Dest [3 0 R /Fit] >>";
        let pdf = Pdf::open(pdf_objects(&[
            "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
            "<< /Type /Pages /Count 1 /Kids [3 0 R] >>".to_owned(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 120 80] /Annots 4 0 R >>".to_owned(),
            format!("[{ignored} {link} {link}]"),
        ]))
        .unwrap();
        let links = pdf.page_links(0).unwrap();
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, PdfLinkTarget::Page(0));
    }

    #[test]
    fn renders_vector_content_with_document_rotation_and_viewport_panning() {
        let pdf = Pdf::open(document()).unwrap();
        assert_eq!(pdf.page_count(), 2);
        assert_eq!(pdf.page_size(0), Ok((120.0, 80.0)));
        assert_eq!(pdf.page_size(1), Ok((80.0, 120.0)));
        let frame = pdf
            .render(
                0,
                RenderRequest {
                    width: 120,
                    height: 80,
                    scale: 1.0,
                    x: 0.0,
                    y: 0.0,
                    turns: 0,
                },
            )
            .unwrap();
        assert_eq!(frame.len(), 120 * 80);
        assert!(frame[10 * 120 + 20] < 32);
        assert_eq!(frame[40 * 120 + 40], 255);
        assert!((8..20).any(|y| {
            frame[y * 120 + 56..y * 120 + 85]
                .iter()
                .any(|&pixel| pixel < 96)
        }));

        let frame = pdf
            .render(
                0,
                RenderRequest {
                    width: 80,
                    height: 120,
                    scale: 1.0,
                    x: 0.0,
                    y: 0.0,
                    turns: 1,
                },
            )
            .unwrap();
        assert!(frame[20 * 80 + 70] < 32);
        assert_eq!(frame[20 * 80 + 10], 255);
        let rotated_by_document = pdf
            .render(
                1,
                RenderRequest {
                    width: 80,
                    height: 120,
                    scale: 1.0,
                    x: 0.0,
                    y: 0.0,
                    turns: 0,
                },
            )
            .unwrap();
        assert_eq!(frame, rotated_by_document);

        let frame = pdf
            .render(
                0,
                RenderRequest {
                    width: 24,
                    height: 24,
                    scale: 1.0,
                    x: -10.0,
                    y: -5.0,
                    turns: 0,
                },
            )
            .unwrap();
        assert_eq!(frame.len(), 24 * 24);
        assert!(frame[6 * 24 + 4] < 32);
        assert_eq!(frame[18 * 24 + 4], 255);
    }

    #[test]
    fn fit_preserves_aspect_ratio_and_centers_the_rotated_page() {
        let pdf = Pdf::open(document()).unwrap();
        let pixels = pdf.fit(0, 120, 120, 0).unwrap();
        assert_eq!(pixels.len(), 120 * 120);
        assert!(pixels[0..20 * 120].iter().all(|&pixel| pixel == 255));
        assert!(pixels[30 * 120 + 20] < 32);
        assert_eq!(pdf.fit(0, 120, 120, 1), pdf.fit(1, 120, 120, 0));
        assert_eq!(pdf.fit(0, 0, 120, 0), Err(PdfError::InvalidViewport));
    }

    #[test]
    fn embedded_text_projects_to_the_rendered_text_after_rotation_and_panning() {
        let pdf = Pdf::open(document()).unwrap();
        assert_eq!(pdf.page_text(2), Err(PdfError::MissingPage));
        let mut rectangles = Vec::new();
        for (page, turns) in [(0, 1), (1, 0)] {
            let text = pdf.page_text(page).unwrap();
            assert_eq!(text.text, "PDF");
            assert_eq!(text.words.len(), 1);
            assert_eq!(text.words[0].bytes, 0..3);
            assert!(!text.truncated);
            let request = RenderRequest {
                width: 160,
                height: 100,
                scale: 2.0,
                x: -10.0,
                y: -100.0,
                turns,
            };
            let rect = text.words[0]
                .bounds
                .in_viewport(pdf.page_size(page).unwrap(), request)
                .unwrap();
            let pixels = pdf.render(page, request).unwrap();
            assert!(
                (rect.y0.ceil() as usize..rect.y1.floor() as usize).any(|y| {
                    (rect.x0.ceil() as usize..rect.x1.floor() as usize)
                        .any(|x| pixels[y * request.width as usize + x] < 96)
                })
            );
            rectangles.push(rect);
        }
        let [first, second] = rectangles.as_slice() else {
            unreachable!()
        };
        assert!((first.x0 - second.x0).abs() < 0.001);
        assert!((first.y0 - second.y0).abs() < 0.001);
        assert!((first.x1 - second.x1).abs() < 0.001);
        assert!((first.y1 - second.y1).abs() < 0.001);
    }

    #[test]
    fn text_projection_handles_quarter_turns_zoom_clipping_and_invalid_input() {
        let rect = PdfTextRect {
            x0: 0.125,
            y0: 0.25,
            x1: 0.5,
            y1: 0.75,
        };
        let page_size = (80.0, 40.0);
        let request = RenderRequest {
            width: 200,
            height: 200,
            scale: 2.0,
            x: 0.0,
            y: 0.0,
            turns: 0,
        };
        for (turns, expected) in [
            (0, Rect::new(20.0, 20.0, 80.0, 60.0)),
            (1, Rect::new(20.0, 20.0, 60.0, 80.0)),
            (2, Rect::new(80.0, 20.0, 140.0, 60.0)),
            (3, Rect::new(20.0, 80.0, 60.0, 140.0)),
        ] {
            assert_eq!(
                rect.in_viewport(page_size, RenderRequest { turns, ..request }),
                Some(expected)
            );
        }
        assert_eq!(
            rect.in_viewport(
                page_size,
                RenderRequest {
                    width: 45,
                    height: 30,
                    x: -25.0,
                    y: -40.0,
                    turns: 1,
                    ..request
                }
            ),
            Some(Rect::new(0.0, 0.0, 35.0, 30.0))
        );
        assert_eq!(
            rect.in_viewport(
                page_size,
                RenderRequest {
                    x: -100.0,
                    ..request
                }
            ),
            None
        );
        assert_eq!(
            rect.in_viewport(
                page_size,
                RenderRequest {
                    scale: f32::NAN,
                    ..request
                }
            ),
            None
        );
        assert_eq!(
            PdfTextRect { x1: 0.0, ..rect }.in_viewport(page_size, request),
            None
        );
    }

    #[test]
    fn rejects_invalid_documents_and_viewports() {
        assert!(matches!(
            Pdf::open(b"%PDF-1.4 damaged".to_vec()),
            Err(PdfError::Damaged)
        ));
        let pdf = Pdf::open(document()).unwrap();
        let request = RenderRequest {
            width: 640,
            height: 800,
            scale: 1.0,
            x: 0.0,
            y: 0.0,
            turns: 0,
        };
        assert_eq!(pdf.render(2, request), Err(PdfError::MissingPage));
        assert_eq!(
            pdf.render(
                0,
                RenderRequest {
                    width: u32::MAX,
                    ..request
                }
            ),
            Err(PdfError::InvalidViewport)
        );
        assert_eq!(
            pdf.render(
                0,
                RenderRequest {
                    scale: f32::NAN,
                    ..request
                }
            ),
            Err(PdfError::InvalidViewport)
        );
    }
}
