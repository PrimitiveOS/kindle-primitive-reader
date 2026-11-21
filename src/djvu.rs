//! Decode one visible DjVu viewport and expose embedded scan text.

use djvu_rs::djvu_render::{RenderRect, RenderRequest as DecodeRequest, UserRotation};
use djvu_rs::{Document, Page, ParseOptions, ResourceLimits, Rotation, TextZone, TextZoneKind};
use std::fmt;
use std::ops::Range;
use std::panic::{AssertUnwindSafe, catch_unwind};

use crate::fixed::{MAX_VIEWPORT_PIXELS, RenderRequest};
use crate::fixed_text::{PdfPageText, PdfTextRect, PdfTextSpan};

#[path = "djvu_contents.rs"]
mod contents;

pub const MAX_DJVU_BYTES: usize = 32 * 1024 * 1024;
const MAX_DJVU_PAGES: u64 = 10_000;
const MAX_COMPONENTS: u64 = 12_000;
const MAX_IFF_DEPTH: usize = 16;
const MAX_IFF_CHUNKS: usize = 64_000;
const MAX_PAGE_PIXELS: u64 = 16 * 1024 * 1024;
const MAX_DECODED_PAGE_BYTES: u64 = 64 * 1024 * 1024;
pub(crate) const MAX_RENDER_CACHE_BYTES: usize = 8 * 1024 * 1024;
const MAX_WAVELET_COEFFICIENT_BYTES: u64 = 48 * 1024 * 1024;
const MAX_TEXT_LAYER_BYTES: usize = 512 * 1024;
const MAX_TEXT_BYTES: usize = 96 * 1024;
const MAX_ZONE_TEXT_BYTES: usize = 1024 * 1024;
const ZONE_BYTES: usize = 17;

pub struct Djvu {
    document: Document,
    contents: contents::Contents,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DjvuError {
    TooLarge,
    UnsupportedOrDamaged,
    MissingPage,
    InvalidPageSize,
    InvalidViewport,
    RenderFailed,
    TextExtractionFailed,
}

impl fmt::Display for DjvuError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::TooLarge => "This DjVu document exceeds Reader's 32 MB limit.",
            Self::UnsupportedOrDamaged => {
                "This DjVu document is damaged, unsupported, or exceeds Reader's limits."
            }
            Self::MissingPage => "That page is outside this DjVu document.",
            Self::InvalidPageSize => "This DjVu page has unsupported dimensions.",
            Self::InvalidViewport => "This DjVu view is too large to draw on the Kindle.",
            Self::RenderFailed => "Reader could not draw this DjVu page.",
            Self::TextExtractionFailed => "Reader could not extract text from this DjVu page.",
        })
    }
}

impl std::error::Error for DjvuError {}

impl Djvu {
    pub fn open(bytes: Vec<u8>) -> Result<Self, DjvuError> {
        if bytes.len() > MAX_DJVU_BYTES {
            return Err(DjvuError::TooLarge);
        }
        let iff = bytes.strip_prefix(b"AT&T").unwrap_or(&bytes);
        let mut chunks = 0;
        validate_iff_chunks(iff, 0, &mut chunks)?;

        let options = ParseOptions {
            limits: Some(ResourceLimits {
                max_file_bytes: Some(MAX_DJVU_BYTES as u64),
                max_pages: Some(MAX_DJVU_PAGES),
                max_components: Some(MAX_COMPONENTS),
                max_page_pixels: Some(MAX_PAGE_PIXELS),
                max_decoded_bytes: Some(MAX_DECODED_PAGE_BYTES),
                max_render_pixels: Some(u64::from(MAX_VIEWPORT_PIXELS)),
                ..ResourceLimits::default()
            }),
        };
        let document = catch_unwind(AssertUnwindSafe(|| {
            // The byte-backed parser has no companion-file resolver.
            Document::from_bytes_with_options(bytes, &options)
        }))
        .map_err(|_| DjvuError::UnsupportedOrDamaged)?
        .map_err(|_| DjvuError::UnsupportedOrDamaged)?;
        if document.page_count() == 0 {
            return Err(DjvuError::UnsupportedOrDamaged);
        }
        let contents = contents::Contents::read(&document);
        Ok(Self { document, contents })
    }

    /// Publisher contents for the current page, including labels without a local destination.
    pub fn outline(&self) -> &[crate::fixed::PdfOutlineItem] {
        &self.contents.rows
    }

    pub(crate) fn stable_outline_destination(&self, index: usize) -> bool {
        self.contents.stable(index)
    }

    pub(crate) fn set_outline_page(&mut self, page: usize) {
        self.contents.set_page(page, self.document.page_count());
    }

    pub fn page_count(&self) -> usize {
        self.document.page_count()
    }

    pub fn page_size(&self, index: usize) -> Result<(f32, f32), DjvuError> {
        display_size(&self.page(index)?)
    }

    /// Return one row-major grayscale viewport, with white outside the page.
    pub fn render(&self, index: usize, request: RenderRequest) -> Result<Vec<u8>, DjvuError> {
        let page = self.page(index)?;
        let (page_width, page_height) = display_size(&page)?;
        self.validate_wavelet_planes(index)?;
        request
            .page_to_viewport((page_width, page_height))
            .map_err(|_| DjvuError::InvalidViewport)?;

        let mut output = vec![255; (request.width * request.height) as usize];
        let width = (page_width * request.scale).round().max(1.0) as u32;
        let height = (page_height * request.scale).round().max(1.0) as u32;
        let turns = request.turns % 4;
        let (turned_width, turned_height) = if turns.is_multiple_of(2) {
            (width, height)
        } else {
            (height, width)
        };
        let (x, y) = (request.x.round() as i32, request.y.round() as i32);
        let (left, top) = (x.max(0), y.max(0));
        let right = (x + turned_width as i32).min(request.width as i32);
        let bottom = (y + turned_height as i32).min(request.height as i32);
        if left >= right || top >= bottom {
            return Ok(output);
        }
        let region = RenderRect {
            x: (left - x) as u32,
            y: (top - y) as u32,
            width: (right - left) as u32,
            height: (bottom - top) as u32,
        };
        let mut options = page.options_for_size(width, height);
        options.rotation = [
            UserRotation::None,
            UserRotation::Cw90,
            UserRotation::Rot180,
            UserRotation::Ccw90,
        ][turns as usize];
        let result = catch_unwind(AssertUnwindSafe(|| {
            page.render_request(&DecodeRequest::new(options).region(region))
        }));
        self.document
            .inner()
            .enforce_cache_budget(MAX_RENDER_CACHE_BYTES, &[]);
        let raster = result
            .map_err(|_| DjvuError::RenderFailed)?
            .map_err(|_| DjvuError::RenderFailed)?;
        if raster.width != region.width
            || raster.height != region.height
            || raster.data.len() != (region.width * region.height * 4) as usize
        {
            return Err(DjvuError::RenderFailed);
        }
        for (row, rgba) in raster
            .data
            .chunks_exact((region.width * 4) as usize)
            .enumerate()
        {
            let start = (top as usize + row) * request.width as usize + left as usize;
            let dest = &mut output[start..start + region.width as usize];
            for (pixel, rgba) in dest.iter_mut().zip(rgba.as_chunks::<4>().0) {
                *pixel = ((54 * u16::from(rgba[0])
                    + 183 * u16::from(rgba[1])
                    + 19 * u16::from(rgba[2])
                    + 128)
                    >> 8) as u8;
            }
        }
        Ok(output)
    }

    /// Decode only the source patch needed for each output tile. A rotated crop at high zoom
    /// can project an entire page to tens of thousands of pixels on either axis.
    #[cfg(feature = "kindle")]
    pub(crate) fn render_transformed(
        &self,
        page: usize,
        request: RenderRequest,
        correction: hayro::kurbo::Affine,
    ) -> Result<Vec<u8>, DjvuError> {
        use hayro::kurbo::{Affine, Point, Rect};
        if correction == Affine::IDENTITY {
            return self.render(page, request);
        }
        let size = self.page_size(page)?;
        let transform = request
            .page_to_viewport(size)
            .map_err(|_| DjvuError::InvalidViewport)?
            * correction;
        let scale = transform.determinant().sqrt() as f32;
        // Measure inverse positions in upright rendered pixels, rather than native page units.
        let inverse = Affine::scale(f64::from(scale)) * transform.inverse();
        let source_width = (size.0 * scale).round().max(1.0) as i32;
        let source_height = (size.1 * scale).round().max(1.0) as i32;
        let mut result = vec![255; (request.width * request.height) as usize];
        const TILE: usize = 900;
        for top in (0..request.height as usize).step_by(TILE) {
            for left in (0..request.width as usize).step_by(TILE) {
                let right = (left + TILE).min(request.width as usize);
                let bottom = (top + TILE).min(request.height as usize);
                let patch = inverse.transform_rect_bbox(Rect::new(
                    left as f64,
                    top as f64,
                    right as f64,
                    bottom as f64,
                ));
                let x0 = (patch.x0.floor() as i32 - 1).max(0);
                let y0 = (patch.y0.floor() as i32 - 1).max(0);
                let x1 = (patch.x1.ceil() as i32 + 1).min(source_width);
                let y1 = (patch.y1.ceil() as i32 + 1).min(source_height);
                if x0 >= x1 || y0 >= y1 {
                    continue;
                }
                let w = (x1 - x0) as usize;
                let h = (y1 - y0) as usize;
                let pixels = self.render(
                    page,
                    RenderRequest {
                        width: w as u32,
                        height: h as u32,
                        x: -(x0 as f32),
                        y: -(y0 as f32),
                        scale,
                        turns: 0,
                    },
                )?;
                let step = inverse.as_coeffs();
                for y in top..bottom {
                    let mut point = inverse * Point::new(left as f64 + 0.5, y as f64 + 0.5);
                    let row = &mut result
                        [y * request.width as usize + left..y * request.width as usize + right];
                    for pixel in row {
                        let x = point.x - f64::from(x0);
                        let y = point.y - f64::from(y0);
                        if (0.0..w as f64).contains(&x) && (0.0..h as f64).contains(&y) {
                            let sx = (x - 0.5).clamp(0.0, (w - 1) as f64);
                            let sy = (y - 0.5).clamp(0.0, (h - 1) as f64);
                            let a = sx as usize;
                            let b = (a + 1).min(w - 1);
                            let upper = sy as usize * w;
                            let lower = ((sy as usize + 1).min(h - 1)) * w;
                            let blend = |i| {
                                f64::from(pixels[i + a]) * (1.0 - sx.fract())
                                    + f64::from(pixels[i + b]) * sx.fract()
                            };
                            *pixel = (blend(upper) * (1.0 - sy.fract()) + blend(lower) * sy.fract())
                                .round() as u8;
                        }
                        point.x += step[0];
                        point.y += step[1];
                    }
                }
            }
        }
        Ok(result)
    }

    /// Read existing text zones in scan order with normalized display bounds.
    pub fn page_text(&self, index: usize) -> Result<PdfPageText, DjvuError> {
        let page = self.page(index)?;
        display_size(&page)?;
        catch_unwind(AssertUnwindSafe(|| read_text(&self.document, &page)))
            .map_err(|_| DjvuError::TextExtractionFailed)?
    }

    fn page(&self, index: usize) -> Result<Page<'_>, DjvuError> {
        if index >= self.page_count() {
            return Err(DjvuError::MissingPage);
        }
        self.document
            .page(index)
            .map_err(|_| DjvuError::UnsupportedOrDamaged)
    }

    fn validate_wavelet_planes(&self, index: usize) -> Result<(), DjvuError> {
        let page = self
            .document
            .inner()
            .page(index)
            .map_err(|_| DjvuError::UnsupportedOrDamaged)?;
        let mut total = 0;
        for id in [b"BG44", b"FG44", b"BM44", b"PM44"] {
            if let Some(header) = page.raw_chunk(id) {
                total += wavelet_coefficients(header)?;
            }
        }
        if total > MAX_WAVELET_COEFFICIENT_BYTES {
            return Err(DjvuError::InvalidPageSize);
        }
        Ok(())
    }
}

fn wavelet_coefficients(header: &[u8]) -> Result<u64, DjvuError> {
    let [0, _, major, _, w0, w1, h0, h1, _, ..] = header else {
        return Err(DjvuError::UnsupportedOrDamaged);
    };
    let width = u64::from(u16::from_be_bytes([*w0, *w1]));
    let height = u64::from(u16::from_be_bytes([*h0, *h1]));
    if width == 0 || height == 0 {
        return Err(DjvuError::InvalidPageSize);
    }
    // IW44 allocates one 32 × 32 block of i16 coefficients per plane.
    let planes = if major & 0x80 == 0 { 3 } else { 1 };
    Ok(width.div_ceil(32) * height.div_ceil(32) * 1024 * 2 * planes)
}

fn validate_iff_chunks(mut bytes: &[u8], depth: usize, count: &mut usize) -> Result<(), DjvuError> {
    if depth > MAX_IFF_DEPTH {
        return Err(DjvuError::UnsupportedOrDamaged);
    }
    while !bytes.is_empty() {
        let [a, b, c, d, l0, l1, l2, l3, ..] = bytes else {
            return Err(DjvuError::UnsupportedOrDamaged);
        };
        *count += 1;
        if *count > MAX_IFF_CHUNKS {
            return Err(DjvuError::UnsupportedOrDamaged);
        }
        let size = u32::from_be_bytes([*l0, *l1, *l2, *l3]) as usize;
        let end = 8_usize
            .checked_add(size)
            .ok_or(DjvuError::UnsupportedOrDamaged)?;
        let payload = bytes.get(8..end).ok_or(DjvuError::UnsupportedOrDamaged)?;
        if matches!(&[*a, *b, *c, *d], b"FORM" | b"LIST" | b"CAT " | b"PROP") {
            let children = payload.get(4..).ok_or(DjvuError::UnsupportedOrDamaged)?;
            validate_iff_chunks(children, depth + 1, count)?;
        }
        if end == bytes.len() {
            break;
        }
        bytes = bytes
            .get(end + size % 2..)
            .ok_or(DjvuError::UnsupportedOrDamaged)?;
    }
    Ok(())
}

fn display_size(page: &Page<'_>) -> Result<(f32, f32), DjvuError> {
    let (width, height) = (page.display_width(), page.display_height());
    if width == 0 || height == 0 {
        return Err(DjvuError::InvalidPageSize);
    }
    Ok((width as f32, height as f32))
}

fn read_text(document: &Document, page: &Page<'_>) -> Result<PdfPageText, DjvuError> {
    let inner = document
        .inner()
        .page(page.index())
        .map_err(|_| DjvuError::TextExtractionFailed)?;
    let truncated = || PdfPageText {
        truncated: true,
        ..PdfPageText::default()
    };
    if [b"TXTz", b"TXTa"].into_iter().any(|id| {
        inner
            .raw_chunk(id)
            .is_some_and(|data| data.len() > MAX_TEXT_LAYER_BYTES)
    }) {
        return Ok(truncated());
    }
    // djvu-bzz applies its own decoded-output limit before this text-specific check.
    let Some(payload) = inner
        .chunk_payload(b"TXTz", b"TXTa")
        .map_err(|_| DjvuError::TextExtractionFailed)?
    else {
        return Ok(PdfPageText::default());
    };
    if !text_layer_within_budget(&payload)? {
        return Ok(truncated());
    }
    let mut layer = djvu_rs::text::parse_text_layer(&payload, page.height())
        .map_err(|_| DjvuError::TextExtractionFailed)?;
    let (width, height) = (page.display_width(), page.display_height());
    if page.rotation() != Rotation::None {
        layer = layer.transform(page.width(), page.height(), page.rotation(), width, height);
    }

    let mut text = layer.text;
    let cut = text.floor_char_boundary(MAX_TEXT_BYTES);
    let is_truncated = cut != text.len();
    text.truncate(cut);
    let mut result = PdfPageText {
        text,
        truncated: is_truncated,
        ..PdfPageText::default()
    };
    let mut lines = SpanCursor::default();
    let mut words = SpanCursor::default();
    let mut pending: Vec<&TextZone> = layer.zones.iter().rev().collect();
    while let Some(zone) = pending.pop() {
        let (cursor, spans) = match zone.kind {
            TextZoneKind::Line => (&mut lines, &mut result.lines),
            TextZoneKind::Word | TextZoneKind::Character => (&mut words, &mut result.words),
            _ => {
                pending.extend(zone.children.iter().rev());
                continue;
            }
        };
        let content = zone
            .text
            .trim_matches(|ch: char| ch.is_whitespace() || ch.is_ascii_control());
        if let Some(bytes) = cursor.next(&result.text, content)
            && let Some(bounds) = zone_bounds(zone, width, height)
        {
            spans.push(PdfTextSpan { bytes, bounds });
        }
        if zone.kind == TextZoneKind::Line {
            pending.extend(zone.children.iter().rev());
        }
    }
    // DjVu paragraph and region separators are single-byte controls. Preserve range offsets.
    result.text = result
        .text
        .chars()
        .map(|ch| match ch {
            '\0' | '\u{b}' | '\u{1d}' | '\u{1f}' => '\n',
            ch if ch.is_ascii_control() && ch != '\n' && ch != '\t' => ' ',
            ch => ch,
        })
        .collect();
    Ok(result)
}

fn text_layer_within_budget(payload: &[u8]) -> Result<bool, DjvuError> {
    let length = payload
        .first_chunk::<3>()
        .ok_or(DjvuError::TextExtractionFailed)?;
    let text_len = u24(length);
    let text_end = 3 + text_len;
    if text_end > payload.len() {
        return Err(DjvuError::TextExtractionFailed);
    }
    if payload.len() > MAX_TEXT_LAYER_BYTES || text_len > MAX_TEXT_BYTES {
        return Ok(false);
    }
    // A zone record has a signed text length at byte 11. The upstream parser
    // copies each zone's slice, so bound duplicate claims before it allocates.
    let records = payload.get(text_end + 1..).unwrap_or_default();
    let mut claims = 0usize;
    for record in records.as_chunks::<ZONE_BYTES>().0 {
        if record[11] & 0x80 == 0 {
            claims += u24(&record[11..14]);
            if claims > MAX_ZONE_TEXT_BYTES {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

fn u24(bytes: &[u8]) -> usize {
    usize::from(bytes[0]) << 16 | usize::from(bytes[1]) << 8 | usize::from(bytes[2])
}

struct SpanCursor {
    offset: usize,
    budget: usize,
}

impl Default for SpanCursor {
    fn default() -> Self {
        Self {
            offset: 0,
            budget: MAX_TEXT_BYTES * 2,
        }
    }
}

impl SpanCursor {
    fn next(&mut self, text: &str, zone: &str) -> Option<Range<usize>> {
        if zone.is_empty() {
            return None;
        }
        let remaining = &text[self.offset..];
        let search = &remaining[..remaining.floor_char_boundary(self.budget)];
        let Some(index) = search.find(zone) else {
            self.budget -= search.len();
            return None;
        };
        let start = self.offset + index;
        self.offset = start + zone.len();
        self.budget -= index + zone.len();
        Some(start..self.offset)
    }
}

fn zone_bounds(zone: &TextZone, width: u32, height: u32) -> Option<PdfTextRect> {
    let rect = &zone.rect;
    let x1 = rect.x.saturating_add(rect.width).min(width);
    let y1 = rect.y.saturating_add(rect.height).min(height);
    (x1 > rect.x && y1 > rect.y).then(|| PdfTextRect {
        x0: rect.x as f32 / width as f32,
        y0: rect.y as f32 / height as f32,
        x1: x1 as f32 / width as f32,
        y1: y1 as f32 / height as f32,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn estimates_wavelet_planes_before_rendering() {
        let small = [0, 1, 0x81, 2, 0, 33, 0, 1, 0];
        assert_eq!(wavelet_coefficients(&small), Ok(2 * 1024 * 2));
        let oversized = [0, 1, 1, 2, 0xff, 0xff, 0xff, 0xff, 0];
        assert!(wavelet_coefficients(&oversized).unwrap() > MAX_WAVELET_COEFFICIENT_BYTES);
        assert!(wavelet_coefficients(&[0, 1, 1]).is_err());
    }

    #[test]
    fn checks_iff_chunk_lengths_nesting_and_count_before_decoding() {
        let empty_chunk = b"INFO\0\0\0\0";
        assert!(validate_iff_chunks(empty_chunk, MAX_IFF_DEPTH, &mut 0).is_ok());
        assert!(validate_iff_chunks(empty_chunk, MAX_IFF_DEPTH + 1, &mut 0).is_err());
        let mut count = MAX_IFF_CHUNKS;
        assert!(validate_iff_chunks(empty_chunk, 0, &mut count).is_err());
        assert!(validate_iff_chunks(b"INFO\xff\xff\xff\xff", 0, &mut 0).is_err());
    }
    use djvu_rs::Bitmap;
    use djvu_rs::TextLayer;
    use djvu_rs::text::Rect;

    fn chunk(output: &mut Vec<u8>, id: &[u8; 4], payload: &[u8]) {
        output.extend_from_slice(id);
        output.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        output.extend_from_slice(payload);
        if !payload.len().is_multiple_of(2) {
            output.push(0);
        }
    }

    #[cfg(feature = "kindle")]
    #[test]
    fn arbitrary_djvu_rotation_places_ink_under_transformed_word() {
        use hayro::kurbo::{Affine, Point};
        let book = Djvu::open(document(0, false)).unwrap();
        let size = book.page_size(0).unwrap();
        let word = book.page_text(0).unwrap().words[0].bounds;
        let center = Point::new(
            f64::from((word.x0 + word.x1) * size.0 / 2.0),
            f64::from((word.y0 + word.y1) * size.1 / 2.0),
        );
        let correction = Affine::translate((32.0, 20.0))
            * Affine::rotate(13.0_f64.to_radians())
            * Affine::translate((-32.0, -20.0));
        for turns in 0..4 {
            let request = RenderRequest {
                width: 1050,
                height: 1070,
                scale: 14.0,
                x: 90.0,
                y: 210.0,
                turns,
            };
            let target = request.page_to_viewport(size).unwrap() * correction * center;
            let pixels = book.render_transformed(0, request, correction).unwrap();
            assert!(pixels[target.y as usize * 1050 + target.x as usize] < 64);
        }
    }

    pub(crate) fn document(rotation: u8, compressed_text: bool) -> Vec<u8> {
        // Original synthetic page with a dark region under a two-word text layer.
        let (width, height) = (64, 40);
        let mut bitmap = Bitmap::new(width, height);
        for y in 8..15 {
            for x in 12..25 {
                bitmap.set_black(x, y);
            }
        }
        let word = |text: &str, x| TextZone {
            kind: TextZoneKind::Word,
            rect: Rect {
                x,
                y: 8,
                width: 13,
                height: 7,
            },
            text: text.to_owned(),
            children: Vec::new(),
        };
        let line = TextZone {
            kind: TextZoneKind::Line,
            rect: Rect {
                x: 12,
                y: 8,
                width: 32,
                height: 7,
            },
            text: "Tide café".to_owned(),
            children: vec![word("Tide", 12), word("café", 31)],
        };
        let layer = TextLayer {
            text: line.text.clone(),
            zones: vec![TextZone {
                kind: TextZoneKind::Page,
                rect: Rect {
                    x: 0,
                    y: 0,
                    width,
                    height,
                },
                text: line.text.clone(),
                children: vec![line],
            }],
        };
        let text = djvu_rs::text_encode::encode_text_layer(&layer, height);
        let mask = djvu_rs::jb2_encode::encode_jb2(&bitmap);
        let mut form = b"DJVU".to_vec();
        chunk(
            &mut form,
            b"INFO",
            &[0, width as u8, 0, height as u8, 24, 0, 44, 1, 22, rotation],
        );
        chunk(&mut form, b"Sjbz", &mask);
        if compressed_text {
            chunk(&mut form, b"TXTz", &djvu_rs::bzz_encode::bzz_encode(&text));
        } else {
            chunk(&mut form, b"TXTa", &text);
        }
        let mut output = b"AT&T".to_vec();
        chunk(&mut output, b"FORM", &form);
        output
    }

    #[test]
    fn maps_text_geometry_to_the_same_panned_rotated_raster() {
        let normal = Djvu::open(document(1, true)).unwrap();
        let rotated = Djvu::open(document(5, false)).unwrap();
        assert_eq!(normal.page_count(), 1);
        assert_eq!(normal.page_size(0).unwrap(), (64.0, 40.0));
        assert_eq!(rotated.page_size(0).unwrap(), (40.0, 64.0));
        let mut frames = Vec::new();
        for (document, turns) in [(&normal, 1), (&rotated, 0)] {
            let request = RenderRequest {
                width: 53,
                height: 66,
                scale: 1.0,
                x: 4.0,
                y: -6.0,
                turns,
            };
            let text = document.page_text(0).unwrap();
            assert_eq!(text.text, "Tide café");
            assert_eq!(text.lines[0].bytes, 0..10);
            assert_eq!(
                text.words
                    .iter()
                    .map(|span| &text.text[span.bytes.clone()])
                    .collect::<Vec<_>>(),
                ["Tide", "café"]
            );
            let area = text.words[0]
                .bounds
                .in_viewport(document.page_size(0).unwrap(), request)
                .unwrap();
            let frame = document.render(0, request).unwrap();
            let width = request.width as usize;
            assert_eq!(frame.len(), width * request.height as usize);
            assert!(
                (area.y0.ceil() as usize..area.y1.floor() as usize).any(|y| {
                    (area.x0.ceil() as usize..area.x1.floor() as usize)
                        .any(|x| frame[y * width + x] < 64)
                })
            );
            assert!(frame.chunks_exact(width).all(|row| row[0] == 255));
            frames.push(frame);
        }
        assert_eq!(frames[0], frames[1]);
    }

    #[test]
    fn validates_viewport_and_bounds_duplicate_text_slices() {
        let doc = Djvu::open(document(1, false)).unwrap();
        assert_eq!(doc.page_text(1), Err(DjvuError::MissingPage));
        let request = RenderRequest {
            width: 80,
            height: 80,
            scale: 1.0,
            x: 100.0,
            y: 100.0,
            turns: 2,
        };
        assert_eq!(doc.render(0, request), Ok(vec![255; 80 * 80]));
        assert_eq!(
            doc.render(
                0,
                RenderRequest {
                    width: u32::MAX,
                    ..request
                }
            ),
            Err(DjvuError::InvalidViewport)
        );
        let mut oversized_zone = vec![0u8; 4 + ZONE_BYTES];
        oversized_zone[4 + 11] = 0x20;
        assert_eq!(text_layer_within_budget(&oversized_zone), Ok(false));
        assert_eq!(
            text_layer_within_budget(b"\0\0\x09"),
            Err(DjvuError::TextExtractionFailed)
        );
    }
}
