//! Extract PDF text with approximate geometry in content-stream order.

use hayro::hayro_interpret::font::{Glyph, GlyphRun, PositionedGlyph};
use hayro::hayro_interpret::hayro_cmap::{BfString, WritingMode};
use hayro::hayro_interpret::util::TransformExt;
use hayro::hayro_interpret::{
    BlendMode, ClipPath, Context, Device, DrawMode, DrawProps, Image, ImageDrawProps,
    InterpreterCache, InterpreterSettings, RenderContent, SoftMask, interpret_page,
};
use hayro::hayro_syntax::page::Page;
use hayro::kurbo::{Affine, BezPath, Point, Rect, Vec2};
use std::fmt;
use std::ops::Range;
use std::panic::{AssertUnwindSafe, catch_unwind};

pub(crate) const MAX_GLYPHS: usize = 24_000;
const MAX_TEXT_BYTES: usize = 96 * 1024;
const MAX_GLYPH_TEXT_BYTES: usize = 512;

/// Unit coordinates in the upright, cropped PDF page, with a top-left origin.
#[derive(Clone, Copy, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct PdfTextRect {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct PdfTextSpan {
    /// A UTF-8 byte range in [`PdfPageText::text`].
    pub bytes: Range<usize>,
    pub bounds: PdfTextRect,
}

#[derive(Clone, Debug, Default, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct PdfPageText {
    pub text: String,
    pub lines: Vec<PdfTextSpan>,
    pub words: Vec<PdfTextSpan>,
    pub truncated: bool,
}

#[cfg(feature = "kindle")]
impl PdfPageText {
    pub(crate) fn valid_ocr_cache(&self) -> bool {
        let valid = |spans: &[PdfTextSpan]| {
            spans.iter().all(|span| {
                let rect = span.bounds;
                span.bytes.start < span.bytes.end
                    && self.text.is_char_boundary(span.bytes.start)
                    && self.text.is_char_boundary(span.bytes.end)
                    && rect.x0 >= 0.0
                    && rect.y0 >= 0.0
                    && rect.x0 < rect.x1
                    && rect.y0 < rect.y1
                    && rect.x1 <= 1.0
                    && rect.y1 <= 1.0
            }) && spans
                .windows(2)
                .all(|pair| pair[0].bytes.end <= pair[1].bytes.start)
        };
        self.text.len() <= MAX_TEXT_BYTES
            && self.words.len() <= MAX_GLYPHS
            && self.lines.len() <= MAX_GLYPHS
            && valid(&self.words)
            && valid(&self.lines)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PdfTextError {
    InvalidPageSize,
    InterpretFailed,
}

impl fmt::Display for PdfTextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidPageSize => "This PDF page has unsupported dimensions.",
            Self::InterpretFailed => "Reader could not extract text from this PDF page.",
        })
    }
}

impl std::error::Error for PdfTextError {}

/// Collect a page's embedded text, including any invisible text layer.
/// Span geometry approximates the advance and line height rather than ink outlines.
pub fn extract(page: &Page<'_>) -> Result<PdfPageText, PdfTextError> {
    extract_with_glyph_limit(page, MAX_GLYPHS)
}

fn extract_with_glyph_limit(page: &Page<'_>, limit: usize) -> Result<PdfPageText, PdfTextError> {
    extract_cached(page, limit, &InterpreterCache::new())
}

pub(crate) fn extract_cached<'a>(
    page: &Page<'a>,
    limit: usize,
    cache: &InterpreterCache<'a>,
) -> Result<PdfPageText, PdfTextError> {
    let (width, height) = page.render_dimensions();
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
        return Err(PdfTextError::InvalidPageSize);
    }
    let viewport = Rect::new(0.0, 0.0, f64::from(width), f64::from(height));
    catch_unwind(AssertUnwindSafe(|| {
        let mut collector = Collector::new(viewport, limit);
        let mut context = Context::new(
            page.initial_transform(true).to_kurbo(),
            viewport,
            cache,
            page.xref(),
            InterpreterSettings {
                render_content: RenderContent::PAGE,
                ..InterpreterSettings::default()
            },
        );
        interpret_page(page, &mut context, &mut collector);
        collector.finish()
    }))
    .map_err(|_| PdfTextError::InterpretFailed)
}

struct Placement {
    origin: Point,
    tangent: Vec2,
    em: f64,
    advance: f64,
    bounds: Rect,
}

impl Placement {
    fn new(
        glyph: &PositionedGlyph<'_>,
        transform: Affine,
        vertical: bool,
        crop: Rect,
    ) -> Option<Self> {
        let transform = transform * glyph.transform();
        if !transform.is_finite() {
            return None;
        }
        let width = match &**glyph {
            Glyph::Outline(glyph) => glyph.advance_width(),
            Glyph::Type3(glyph) => glyph.advance_width(),
        };
        let width =
            f64::from(width.filter(|width| width.is_finite()).unwrap_or(500.0)).clamp(80.0, 2000.0);
        let origin = transform * Point::ZERO;
        let x_axis = transform * Point::new(1000.0, 0.0) - origin;
        let y_axis = transform * Point::new(0.0, 1000.0) - origin;
        let (local_bounds, direction, em, advance) = if vertical {
            (
                Rect::new(-500.0, -700.0, 500.0, 250.0),
                -y_axis,
                x_axis.hypot(),
                y_axis.hypot(),
            )
        } else {
            (
                Rect::new(0.0, -200.0, width, 800.0),
                x_axis,
                y_axis.hypot(),
                x_axis.hypot() * width / 1000.0,
            )
        };
        let length = direction.hypot();
        let mapped = transform.transform_rect_bbox(local_bounds);
        if !mapped.is_finite()
            || !length.is_finite()
            || !em.is_finite()
            || length == 0.0
            || em == 0.0
        {
            return None;
        }
        let bounds = mapped.intersect(crop);
        (bounds.width() > 0.0 && bounds.height() > 0.0).then_some(Self {
            origin,
            tangent: direction / length,
            em,
            advance,
            bounds,
        })
    }

    fn separation(&self, previous: &Self) -> Option<char> {
        let displacement = self.origin - previous.origin;
        let em = self.em.max(previous.em);
        if self.tangent.dot(previous.tangent).abs() < 0.75
            || displacement.cross(previous.tangent).abs() > em * 0.48
        {
            Some('\n')
        } else if displacement.dot(previous.tangent).abs() > previous.advance + em * 0.21 {
            Some(' ')
        } else {
            None
        }
    }
}

#[derive(Clone, Copy)]
struct Span {
    start: usize,
    bounds: Rect,
}

impl Span {
    fn append(target: &mut Option<Self>, start: usize, bounds: Rect) {
        match target {
            Some(span) => span.bounds = span.bounds.union(bounds),
            None => *target = Some(Self { start, bounds }),
        }
    }

    fn into_public(self, end: usize, crop: Rect) -> PdfTextSpan {
        PdfTextSpan {
            bytes: self.start..end,
            bounds: PdfTextRect {
                x0: (self.bounds.x0 / crop.width()) as f32,
                y0: (self.bounds.y0 / crop.height()) as f32,
                x1: (self.bounds.x1 / crop.width()) as f32,
                y1: (self.bounds.y1 / crop.height()) as f32,
            },
        }
    }
}

#[derive(PartialEq)]
struct RunMark {
    len: usize,
    start: (u32, Affine),
    end: (u32, Affine),
}

impl RunMark {
    fn new(glyphs: &[PositionedGlyph<'_>], transform: Affine) -> Option<Self> {
        let mark = |glyph: &PositionedGlyph<'_>| (glyph.char_code(), transform * glyph.transform());
        Some(Self {
            len: glyphs.len(),
            start: mark(glyphs.first()?),
            end: mark(glyphs.last()?),
        })
    }
}

struct Collector {
    page: PdfPageText,
    crop: Rect,
    remaining_glyphs: usize,
    has_mapped_text: bool,
    last_fill: Option<RunMark>,
    previous: Option<Placement>,
    pending_space: bool,
    line: Option<Span>,
    word: Option<Span>,
}

impl Collector {
    fn new(crop: Rect, limit: usize) -> Self {
        Self {
            page: PdfPageText::default(),
            crop,
            remaining_glyphs: limit,
            has_mapped_text: false,
            last_fill: None,
            previous: None,
            pending_space: false,
            line: None,
            word: None,
        }
    }

    fn push(&mut self, ch: char, bounds: Rect, mapped: bool) {
        if ch.is_whitespace() {
            self.pending_space = true;
            return;
        }
        if ch.is_control() {
            return;
        }
        let separator = if self.page.text.is_empty() {
            None
        } else if self.line.is_none() {
            Some('\n')
        } else if self.pending_space {
            Some(' ')
        } else {
            None
        };
        if self.page.text.len() + ch.len_utf8() + usize::from(separator.is_some()) > MAX_TEXT_BYTES
        {
            self.page.truncated = true;
            return;
        }
        if let Some(separator) = separator {
            self.finish_word();
            self.page.text.push(separator);
        }
        self.pending_space = false;
        self.has_mapped_text |= mapped;
        let start = self.page.text.len();
        Span::append(&mut self.word, start, bounds);
        Span::append(&mut self.line, start, bounds);
        self.page.text.push(ch);
        // KOReader's MuPDF text provider appends these characters to the
        // current word, then starts a fresh rectangle at the following glyph.
        // Keep adjoining source bytes intact even when their boxes differ.
        if matches!(ch as u32, 0x2000..=0x206f | 0x3000..=0x303f | 0x3400..=0x4dbf
            | 0x4e00..=0x9fff | 0xf900..=0xfaff | 0xff01..=0xffee | 0x20000..=0x2a6df)
        {
            self.finish_word();
        }
    }

    fn finish_word(&mut self) {
        if let Some(word) = self.word.take() {
            self.page
                .words
                .push(word.into_public(self.page.text.len(), self.crop));
        }
    }

    fn finish_line(&mut self) {
        self.finish_word();
        if let Some(line) = self.line.take() {
            self.page
                .lines
                .push(line.into_public(self.page.text.len(), self.crop));
        }
        self.pending_space = false;
    }

    fn finish(mut self) -> PdfPageText {
        if !self.has_mapped_text {
            return PdfPageText {
                truncated: self.page.truncated,
                ..PdfPageText::default()
            };
        }
        self.finish_line();
        self.page
    }
}

impl Device<'_> for Collector {
    fn draw_path(&mut self, _: &BezPath, _: DrawProps<'_>, _: &DrawMode) {}
    fn push_clip_path(&mut self, _: &ClipPath) {}
    fn push_clip_rect(&mut self, _: &Rect) {}
    fn push_transparency_group(&mut self, _: f32, _: Option<SoftMask<'_>>, _: BlendMode) {}
    fn draw_image(&mut self, _: Image<'_, '_>, _: ImageDrawProps<'_>) {}
    fn pop_clip(&mut self) {}
    fn pop_transparency_group(&mut self) {}

    fn draw_glyph_run(&mut self, run: &GlyphRun<'_, '_>, props: DrawProps<'_>, mode: &DrawMode) {
        if self.page.truncated {
            return;
        }
        let glyphs = run.glyphs();
        let mark = RunMark::new(glyphs, props.transform);
        let last_fill = self.last_fill.take();
        if matches!(mode, DrawMode::Stroke(_)) && last_fill.is_some() && last_fill == mark {
            return;
        }
        if matches!(mode, DrawMode::Fill(_)) {
            self.last_fill = mark;
        }
        let vertical = matches!(run.writing_mode(), WritingMode::Vertical);
        for glyph in glyphs {
            if self.remaining_glyphs == 0 || self.page.truncated {
                self.page.truncated = true;
                break;
            }
            self.remaining_glyphs -= 1;
            let Some(placement) = Placement::new(glyph, props.transform, vertical, self.crop)
            else {
                continue;
            };
            match self
                .previous
                .as_ref()
                .and_then(|previous| placement.separation(previous))
            {
                Some('\n') => self.finish_line(),
                Some(' ') => self.pending_space = true,
                _ => {}
            }
            match glyph.as_unicode() {
                Some(BfString::Char(ch)) => self.push(ch, placement.bounds, true),
                Some(BfString::String(text)) => {
                    let end = text.floor_char_boundary(MAX_GLYPH_TEXT_BYTES);
                    for ch in text[..end].chars() {
                        self.push(ch, placement.bounds, true);
                        if self.page.truncated {
                            break;
                        }
                    }
                    self.page.truncated |= text.len() > end;
                }
                None => self.push('\u{fffd}', placement.bounds, false),
            }
            self.previous = Some(placement);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hayro::hayro_syntax::Pdf;
    use std::io::Write;

    #[test]
    #[ignore = "Supply an original PDF to compare consecutive extraction timings"]
    fn compare_shared_pdf_text_cache() {
        let bytes = std::fs::read(std::env::var_os("READER_CACHE_ORIGINAL").unwrap()).unwrap();
        let mut reference = None;
        for group in [1, 4, 16, 64] {
            let pdf = Pdf::new(bytes.clone()).unwrap();
            let start = std::time::Instant::now();
            let mut digest = md5::Context::new();
            let pages = pdf.pages();
            let first = std::env::var("READER_CACHE_START")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            let last = (first + 64).min(pages.len());
            for chunk in (first..last).step_by(group) {
                let cache = InterpreterCache::new();
                for i in chunk..(chunk + group).min(last) {
                    let t = extract_cached(&pages[i], MAX_GLYPHS, &cache).unwrap();
                    digest.consume(serde_json::to_vec(&t).unwrap());
                }
            }
            let digest = format!("{:x}", digest.finalize());
            if let Some(r) = &reference {
                assert_eq!(r, &digest);
            } else {
                reference = Some(digest.clone());
            }
            eprintln!(
                "text-cache group={group} pages={} ms={} fingerprint={digest}",
                last - first,
                start.elapsed().as_millis()
            );
        }
    }

    fn pdf(content: &str, page_properties: &str, unicode_map: Option<&str>) -> Pdf {
        let stream =
            |data: &str| format!("<< /Length {} >>\nstream\n{data}\nendstream", data.len());
        let objects = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_owned(),
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 240 300] {page_properties} /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>"
            ),
            format!(
                "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica {} >>",
                unicode_map.map_or("", |_| "/ToUnicode 6 0 R")
            ),
            stream(content),
            stream(unicode_map.unwrap_or_default()),
        ];
        let mut bytes = b"%PDF-1.4\n".to_vec();
        let mut offsets = Vec::new();
        for (index, object) in objects.iter().enumerate() {
            offsets.push(bytes.len());
            writeln!(bytes, "{} 0 obj\n{object}\nendobj", index + 1).unwrap();
        }
        let xref = bytes.len();
        writeln!(bytes, "xref\n0 {}\n0000000000 65535 f ", offsets.len() + 1).unwrap();
        for offset in offsets {
            writeln!(bytes, "{offset:010} 00000 n ").unwrap();
        }
        write!(
            bytes,
            "trailer\n<< /Size 7 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n"
        )
        .unwrap();
        Pdf::new(bytes).unwrap()
    }

    #[test]
    fn extracts_normal_and_invisible_text_with_spans() {
        let pdf = pdf(
            "BT /F1 12 Tf 22 270 Td (Warm tide) Tj 0 -24 Td 3 Tr (Hidden words) Tj ET",
            "",
            None,
        );
        let result = extract(&pdf.pages()[0]).unwrap();
        assert_eq!(result.text, "Warm tide\nHidden words");
        assert_eq!(result.lines[0].bytes, 0..9);
        assert_eq!(result.lines[1].bytes, 10..22);
        let words: Vec<_> = result
            .words
            .iter()
            .map(|span| &result.text[span.bytes.clone()])
            .collect();
        assert_eq!(words, ["Warm", "tide", "Hidden", "words"]);
        assert!(result.lines[0].bounds.y1 < result.lines[1].bounds.y0);
        assert!(result.words[0].bounds.x0 < result.words[1].bounds.x0);
    }

    #[test]
    fn uses_tounicode_and_normalizes_the_rotated_crop_box() {
        let cmap = "/CIDInit /ProcSet findresource begin 12 dict begin begincmap \
            /CMapType 2 def 1 begincodespacerange <00> <FF> endcodespacerange \
            1 beginbfchar <58> <00df> endbfchar endcmap \
            CMapName currentdict /CMap defineresource pop end end";
        let pdf = pdf(
            "BT /F1 12 Tf 20 240 Td (StraXe) Tj ET",
            "/Rotate 90 /CropBox [10 20 210 270]",
            Some(cmap),
        );
        let result = extract(&pdf.pages()[0]).unwrap();
        assert_eq!(result.text, "Straße");
        let word = &result.words[0];
        assert_eq!(word.bytes, 0..7);
        assert!((0.8..0.95).contains(&word.bounds.x0), "{:?}", word.bounds);
        assert!((0.04..0.06).contains(&word.bounds.y0), "{:?}", word.bounds);
        assert!(word.bounds.y1 > word.bounds.y0);
    }

    #[test]
    fn counts_fill_and_stroke_once_and_caps_output() {
        let pdf = pdf("BT /F1 12 Tf 22 270 Td 2 Tr (ABCD) Tj ET", "", None);
        let page = &pdf.pages()[0];
        assert_eq!(extract(page).unwrap().text, "ABCD");
        let result = extract_with_glyph_limit(page, 3).unwrap();
        assert_eq!(result.text, "ABC");
        assert!(result.truncated);
        assert_eq!(result.words[0].bytes, 0..3);
    }

    #[test]
    fn returns_empty_text_for_a_page_without_glyphs() {
        let pdf = pdf("0 0 100 100 re f", "", None);
        assert_eq!(extract(&pdf.pages()[0]).unwrap(), PdfPageText::default());
    }
}
