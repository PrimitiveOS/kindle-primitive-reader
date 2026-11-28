//! Source-space seed selection and constrained PDF margin detection.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::fixed::{Pdf, RenderRequest};
use crate::fixed_text::PdfTextRect;
use crate::scan_deskew::FULL_PAGE;

#[cfg(test)]
pub(crate) mod source_margin_tests;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub(crate) struct Seed {
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
}

impl Seed {
    pub(crate) fn from_normalized(rect: PdfTextRect, (w, h): (f32, f32)) -> Self {
        Self {
            x0: rect.x0 * w,
            y0: rect.y0 * h,
            x1: rect.x1 * w,
            y1: rect.y1 * h,
        }
    }

    pub(crate) fn normalized(self, (w, h): (f32, f32)) -> Option<PdfTextRect> {
        let rect = PdfTextRect {
            x0: (self.x0 / w).clamp(0.0, 1.0),
            y0: (self.y0 / h).clamp(0.0, 1.0),
            x1: (self.x1 / w).clamp(0.0, 1.0),
            y1: (self.y1 / h).clamp(0.0, 1.0),
        };
        (rect.x1 - rect.x0 >= 0.025 && rect.y1 - rect.y0 >= 0.025).then_some(rect)
    }

    pub(crate) fn adjust(self, x: f32, y: f32, size: (f32, f32)) -> Option<Self> {
        let (x, y) = (x * size.0, y * size.1);
        let xs = [self.x0, (self.x0 + self.x1) * 0.5, self.x1];
        let ys = [self.y0, (self.y0 + self.y1) * 0.5, self.y1];
        let nearest = (0..3)
            .flat_map(|row| (0..3).map(move |column| (row, column)))
            .filter(|&(row, column)| (row, column) != (1, 1))
            .min_by(|&(a, b), &(c, d)| {
                let distance =
                    |row: usize, column: usize| (xs[column] - x).powi(2) + (ys[row] - y).powi(2);
                distance(a, b).total_cmp(&distance(c, d))
            })
            .expect("the rectangle has eight anchors");
        let mut next = self;
        match nearest.0 {
            0 => next.y0 = y,
            2 => next.y1 = y,
            _ => {}
        }
        match nearest.1 {
            0 => next.x0 = x,
            2 => next.x1 = x,
            _ => {}
        }
        next.normalized(size).map(|_| next)
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(default)]
pub(crate) struct Seeds {
    exact: BTreeMap<usize, Seed>,
    parity: [Option<Seed>; 2],
}

impl Seeds {
    pub(crate) fn is_empty(&self) -> bool {
        self.exact.is_empty() && self.parity.iter().all(Option::is_none)
    }

    pub(crate) fn record(&mut self, page: usize, seed: Seed) {
        self.exact.insert(page, seed);
        self.parity[page % 2] = Some(seed);
    }

    pub(crate) fn get(&self, page: usize) -> Option<Seed> {
        self.exact
            .get(&page)
            .copied()
            .or(self.parity[page % 2])
            .or_else(|| self.exact.range(..page).next_back().map(|(_, seed)| *seed))
    }
}

/// Measure dark content in the seed using a bounded grayscale raster.
/// When isolated marks are too small on both native axes, retain the seed.
pub(crate) fn detect(
    pdf: &Pdf,
    page: usize,
    seed: Option<Seed>,
    screen_width: u32,
    margin_hundredths: u8,
) -> crate::Result<PdfTextRect> {
    let size = pdf.page_size(page)?;
    let area = seed
        .and_then(|seed| seed.normalized(size))
        .unwrap_or(FULL_PAGE);
    Ok(detect_ink(pdf, page, area, size, true)?
        .and_then(|ink| {
            padded_native_bounds(
                ink,
                size,
                screen_width,
                margin_hundredths,
                BoundsMode::Seeded,
            )
        })
        .unwrap_or(area))
}

pub(crate) fn full_auto(
    pdf: &Pdf,
    page: usize,
    seed: Option<Seed>,
    screen_width: u32,
    margin_hundredths: u8,
) -> crate::Result<PdfTextRect> {
    let size = pdf.page_size(page)?;
    let detected = detect_ink(pdf, page, FULL_PAGE, size, true)?
        .and_then(|ink| {
            padded_native_bounds(
                ink,
                size,
                screen_width,
                margin_hundredths,
                BoundsMode::FullPage,
            )
        })
        .filter(|crop| crop.x1 - crop.x0 > 0.1 || crop.y1 - crop.y0 > 0.1);
    match detected {
        Some(area) => Ok(area),
        None => manual(pdf, page, seed),
    }
}

/// Detect used content from a bounded grayscale render supplied by any fixed-page provider.
/// The coordinates returned remain normalized to the original source page.
pub(crate) fn full_auto_rendered(
    pixels: &mut [u8],
    width: u32,
    size: (f32, f32),
    scale: f32,
    screen_width: u32,
    margin_hundredths: u8,
    seed: Option<Seed>,
) -> crate::Result<PdfTextRect> {
    if width == 0
        || pixels.is_empty()
        || pixels.len() % width as usize != 0
        || !scale.is_finite()
        || scale <= 0.0
        || !size.0.is_finite()
        || !size.1.is_finite()
        || size.0 <= 0.0
        || size.1 <= 0.0
        || screen_width <= 1
        || margin_hundredths > crate::library::MAX_CROP_MARGIN
    {
        return Err("The page raster cannot be used to detect content".into());
    }
    normalize_ink_polarity(pixels, width as usize);
    discard_isolated_specks(pixels, width as usize, scale);
    let detected = bounds(pixels, width as usize, FULL_PAGE)
        .and_then(|ink| {
            padded_native_bounds(
                ink,
                size,
                screen_width,
                margin_hundredths,
                BoundsMode::FullPage,
            )
        })
        .filter(|crop| crop.x1 - crop.x0 > 0.1 || crop.y1 - crop.y0 > 0.1);
    Ok(detected
        .or_else(|| seed.and_then(|seed| seed.normalized(size)))
        .unwrap_or(FULL_PAGE))
}

pub(crate) fn manual(pdf: &Pdf, page: usize, seed: Option<Seed>) -> crate::Result<PdfTextRect> {
    let size = pdf.page_size(page)?;
    if let Some(area) = seed.and_then(|seed| seed.normalized(size)) {
        return Ok(area);
    }
    // Estimate the native used bounds from the untrimmed raster when there is no saved box.
    let area = detect_ink(pdf, page, FULL_PAGE, size, false)?.and_then(|ink| {
        let points = Seed::from_normalized(ink, size);
        Seed {
            x0: points.x0.round(),
            y0: points.y0.round(),
            x1: points.x1.round(),
            y1: points.y1.round(),
        }
        .normalized(size)
    });
    Ok(area.unwrap_or(FULL_PAGE))
}

fn detect_ink(
    pdf: &Pdf,
    page: usize,
    area: PdfTextRect,
    size: (f32, f32),
    filter_specks: bool,
) -> crate::Result<Option<PdfTextRect>> {
    let area_width = (area.x1 - area.x0) * size.0;
    let area_height = (area.y1 - area.y0) * size.1;
    let scale = 1536.0 / area_width.max(area_height);
    let width = (area_width * scale).ceil() as u32;
    let height = (area_height * scale).ceil() as u32;
    let mut pixels = pdf.render(
        page,
        RenderRequest {
            width,
            height,
            scale,
            x: -area.x0 * size.0 * scale,
            y: -area.y0 * size.1 * scale,
            turns: 0,
        },
    )?;
    normalize_ink_polarity(&mut pixels, width as usize);
    if filter_specks {
        discard_isolated_specks(&mut pixels, width as usize, scale);
    }
    Ok(bounds(&pixels, width as usize, area))
}

const INK_THRESHOLD: u8 = 192;

/// Normalize the bounded raster so pixels near its border background become
/// white and contrasting content becomes dark. This handles both white-on-dark
/// pages and gray paper without mistaking a large dark illustration on an
/// ordinary light page for a dark page. The work buffer is only used by the
/// detector; the displayed image is unchanged.
fn normalize_ink_polarity(pixels: &mut [u8], width: usize) {
    if width == 0 || pixels.is_empty() || pixels.len() % width != 0 {
        return;
    }
    let height = pixels.len() / width;
    if height == 0 {
        return;
    }

    let mut border = Vec::with_capacity(width.saturating_mul(2) + height.saturating_mul(2));
    border.extend_from_slice(&pixels[..width]);
    border.extend_from_slice(&pixels[(height - 1) * width..height * width]);
    for y in 0..height {
        border.push(pixels[y * width]);
        border.push(pixels[y * width + width - 1]);
    }
    border.sort_unstable();
    let median = border[border.len() / 2];
    let uniform = border
        .iter()
        .filter(|sample| (**sample).abs_diff(median) <= 16)
        .count();
    if uniform * 4 < border.len() * 3 {
        // A mixed border may be a full-bleed illustration rather than a page
        // background. Keep the legacy dark-ink path rather than guessing.
        return;
    }
    if median >= 128 {
        let background = u16::from(median);
        for pixel in pixels {
            let value = u16::from(*pixel);
            *pixel = if value >= background {
                u8::MAX
            } else {
                (255 - (background - value) * 255 / background) as u8
            };
        }
    } else {
        let background = u16::from(median);
        let range = 255 - background;
        for pixel in pixels {
            let value = u16::from(*pixel);
            *pixel = if value <= background {
                u8::MAX
            } else {
                (255 - (value - background) * 255 / range) as u8
            };
        }
    }
}

fn discard_isolated_specks(pixels: &mut [u8], width: usize, scale: f32) {
    // Native PDF points keep the speck cutoff consistent across raster sizes.
    const MIN_SOURCE_EXTENT: f32 = 2.0;
    let minimum = ((MIN_SOURCE_EXTENT * scale).ceil() as usize).clamp(1, 64);
    if minimum == 1 {
        return;
    }
    let height = pixels.len() / width;
    const INK: u8 = 1;
    const IN_QUEUE: u8 = 2;
    const PRESERVE: u8 = 3;
    for pixel in pixels.iter_mut() {
        *pixel = u8::from(*pixel < INK_THRESHOLD);
    }
    let mut queue = Vec::new();
    for start in 0..pixels.len() {
        if pixels[start] != INK {
            continue;
        }
        queue.clear();
        queue.push(start);
        pixels[start] = IN_QUEUE;
        let (mut left, mut top) = (start % width, start / width);
        let (mut right, mut bottom) = (left, top);
        let mut next = 0;
        let mut preserve = false;
        'component: while let Some(&index) = queue.get(next) {
            next += 1;
            let (x, y) = (index % width, index / width);
            for ny in y.saturating_sub(1)..=(y + 1).min(height - 1) {
                for nx in x.saturating_sub(1)..=(x + 1).min(width - 1) {
                    let neighbor = ny * width + nx;
                    if pixels[neighbor] == PRESERVE {
                        preserve = true;
                        break 'component;
                    }
                    if pixels[neighbor] != INK {
                        continue;
                    }
                    pixels[neighbor] = IN_QUEUE;
                    queue.push(neighbor);
                    left = left.min(nx);
                    right = right.max(nx);
                    top = top.min(ny);
                    bottom = bottom.max(ny);
                    // An established component needs no full-page flood fill.
                    // Any remaining pixels will find a confirmed neighbor.
                    if right - left + 1 >= minimum || bottom - top + 1 >= minimum {
                        preserve = true;
                        break 'component;
                    }
                }
            }
        }
        for &index in &queue {
            pixels[index] = if preserve { PRESERVE } else { 0 };
        }
    }
    for pixel in pixels {
        *pixel = if *pixel == PRESERVE { 0 } else { 255 };
    }
}

fn bounds(pixels: &[u8], width: usize, area: PdfTextRect) -> Option<PdfTextRect> {
    let height = pixels.len() / width;
    let mut rows = vec![0; height];
    let mut columns = vec![0; width];
    for (y, line) in pixels.chunks_exact(width).enumerate() {
        for (x, ink) in line.iter().enumerate() {
            if *ink < INK_THRESHOLD {
                rows[y] += 1;
                columns[x] += 1;
            }
        }
    }
    let ink_range = |hits: &[usize]| {
        let start = hits.iter().position(|&n| n >= 2)?;
        let end = hits.iter().rposition(|&n| n >= 2)? + 1;
        Some((
            start as f32 / hits.len() as f32,
            end as f32 / hits.len() as f32,
        ))
    };
    let (left, right) = ink_range(&columns)?;
    let (top, bottom) = ink_range(&rows)?;
    let w = area.x1 - area.x0;
    let h = area.y1 - area.y0;
    Some(PdfTextRect {
        x0: area.x0 + left * w,
        x1: area.x0 + right * w,
        y0: area.y0 + top * h,
        y1: area.y0 + bottom * h,
    })
}

enum BoundsMode {
    Seeded,
    FullPage,
}

fn padded_native_bounds(
    ink: PdfTextRect,
    size: (f32, f32),
    screen_width: u32,
    margin_hundredths: u8,
    mode: BoundsMode,
) -> Option<PdfTextRect> {
    let mut points = Seed::from_normalized(ink, size);
    // Use the centers of the first and last native samples.
    points.x0 = points.x0.round();
    points.y0 = points.y0.round();
    points.x1 = points.x1.round() - 1.0;
    points.y1 = points.y1.round() - 1.0;
    if points.x0 >= points.x1 || points.y0 >= points.y1 {
        return None;
    }
    // The Oasis 3 has a 300-dpi display. Convert inches to the total width
    // reserved across both sides, then project that width back to source points.
    const OASIS_DPI: f32 = 300.0;
    let margin = f32::from(margin_hundredths) * OASIS_DPI / 100.0;
    let screen_fraction = margin / (screen_width as f32 - margin).max(1.0);
    let half_margin = (points.x1 - points.x0) * screen_fraction * 0.5;
    points.x0 -= half_margin;
    points.x1 += half_margin;
    points.y0 -= half_margin;
    points.y1 += half_margin;
    match mode {
        BoundsMode::FullPage => {
            points.x1 = points.x1.min(size.0 - 1.0);
            points.y1 = points.y1.min(size.1 - 1.0);
        }
        BoundsMode::Seeded => {
            let width = points.x1 - points.x0.max(0.0);
            let height = points.y1 - points.y0.max(0.0);
            if width > size.0 || height > size.1 {
                return Some(FULL_PAGE);
            }
        }
    }
    let rect = points.normalized(size)?;
    let (w, h) = (rect.x1 - rect.x0, rect.y1 - rect.y0);
    (!(w < 0.1 && h < 0.1)).then_some(rect)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) const SIZE: (f32, f32) = (600.0, 800.0);

    #[test]
    fn content_detection_handles_dark_and_light_page_backgrounds() {
        let width = 64usize;
        let height = 64usize;
        let expected = PdfTextRect {
            x0: 12.0 / width as f32,
            y0: 8.0 / height as f32,
            x1: 52.0 / width as f32,
            y1: 56.0 / height as f32,
        };

        for (background, foreground) in [
            (u8::MAX, 0),
            (220, 0),
            (128, 0),
            (127, u8::MAX),
            (64, u8::MAX),
            (0, u8::MAX),
        ] {
            let mut pixels = vec![background; width * height];
            for y in 8..56 {
                for x in 12..52 {
                    pixels[y * width + x] = foreground;
                }
            }
            // A single border speck must not change the inferred polarity or
            // enlarge the detected content bounds.
            pixels[0] = foreground;
            normalize_ink_polarity(&mut pixels, width);
            discard_isolated_specks(&mut pixels, width, 1.0);
            let found = bounds(&pixels, width, FULL_PAGE).unwrap();
            for (actual, expected) in [found.x0, found.y0, found.x1, found.y1]
                .into_iter()
                .zip([expected.x0, expected.y0, expected.x1, expected.y1])
            {
                assert!((actual - expected).abs() < 0.001, "{actual} != {expected}");
            }
        }

        let mut mixed_border = vec![u8::MAX; width * height];
        mixed_border[..width].fill(0);
        mixed_border[(height - 1) * width..].fill(0);
        let original = mixed_border.clone();
        normalize_ink_polarity(&mut mixed_border, width);
        assert_eq!(mixed_border, original);
    }

    pub(crate) fn fixture() -> Vec<u8> {
        let stream = |contents: &str| {
            format!(
                "<< /Length {} >>\nstream\n{contents}endstream",
                contents.len()
            )
        };
        crate::fixed::tests::pdf_objects(&[
            "<< /Type /Catalog /Pages 2 0 R >>".into(),
            "<< /Type /Pages /Kids [3 0 R 4 0 R 5 0 R 6 0 R] /Count 4 /MediaBox [0 0 600 800] >>"
                .into(),
            "<< /Type /Page /Parent 2 0 R /Contents 7 0 R /Annots [11 0 R 12 0 R] >>".into(),
            "<< /Type /Page /Parent 2 0 R /Contents 8 0 R >>".into(),
            "<< /Type /Page /Parent 2 0 R /Contents 9 0 R >>".into(),
            "<< /Type /Page /Parent 2 0 R /Contents 10 0 R >>".into(),
            stream("0 g 170 150 240 500 re f 14 660 22 32 re f\n"),
            stream("0 g 120 260 320 420 re f 567 114 23 26 re f\n"),
            stream("0 g 200 220 190 360 re f 16 210 22 42 re f\n"),
            stream("0 g 218 402 10 10 re f\n"),
            "<< /Subtype /Link /Rect [200 400 250 445] /Dest [4 0 R /Fit] >>".into(),
            "<< /Subtype /Link /Rect [14 660 36 692] /Dest [6 0 R /Fit] >>".into(),
        ])
    }

    pub(crate) fn odd_seed() -> Seed {
        Seed {
            x0: 60.0,
            y0: 70.0,
            x1: 550.0,
            y1: 740.0,
        }
    }

    pub(crate) fn even_seed() -> Seed {
        Seed {
            x0: 80.0,
            y0: 80.0,
            x1: 530.0,
            y1: 745.0,
        }
    }

    #[test]
    fn configurable_crop_margin_matches_stock_padding_and_clipped_fit() {
        let pdf = Pdf::open(fixture()).unwrap();
        let assert_points = |rect: PdfTextRect, expected: [f32; 4]| {
            let b = Seed::from_normalized(rect, SIZE);
            for (actual, expected) in [b.x0, b.y0, b.x1, b.y1].into_iter().zip(expected) {
                assert!((actual - expected).abs() < 0.002, "{actual} != {expected}");
            }
        };
        let tight = Seed {
            x0: 165.0,
            y0: 145.0,
            x1: 412.0,
            y1: 655.0,
        };
        for (margin, expected) in [
            (0, [170.0, 150.0, 409.0, 649.0]),
            (10, [167.09481, 147.09481, 411.90518, 651.90521]),
            (55, [152.05869, 132.05869, 426.94131, 666.94128]),
            (150, [103.93735, 83.93735, 475.06265, 715.06262]),
        ] {
            assert_points(
                detect(&pdf, 0, Some(odd_seed()), 1264, margin).unwrap(),
                expected,
            );
            assert_points(
                detect(&pdf, 0, Some(tight), 1264, margin).unwrap(),
                expected,
            );
        }
        for (margin, expected) in [
            (0, [120.0, 120.0, 589.0, 685.0]),
            (10, [114.29903, 114.29903, 594.70099, 690.70099]),
            (55, [84.79300, 84.79300, 599.0, 720.20703]),
            (150, [0.0, 0.0, 599.0, 799.0]),
        ] {
            assert_points(
                full_auto(&pdf, 1, Some(even_seed()), 1264, margin).unwrap(),
                expected,
            );
        }
        let full_seed = Seed::from_normalized(FULL_PAGE, SIZE);
        assert_eq!(
            detect(&pdf, 1, Some(full_seed), 1264, 150).unwrap(),
            FULL_PAGE
        );
    }

    #[test]
    fn continuous_manual_and_auto_match_stock_native_bounds() {
        let pdf = Pdf::open(fixture()).unwrap();
        let mut seeds = Seeds::default();
        seeds.record(0, odd_seed());
        seeds.record(1, even_seed());
        let check = |crop: PdfTextRect, expected: [f32; 4]| {
            let points = Seed::from_normalized(crop, SIZE);
            for (found, expected) in [points.x0, points.y0, points.x1, points.y1]
                .into_iter()
                .zip(expected)
            {
                assert!((found - expected).abs() < 0.002, "{found} != {expected}");
            }
        };
        // Pinned KOReader's getPageBBox with fixed-page wrapping and deskew disabled.
        check(
            manual(&pdf, 0, seeds.get(0)).unwrap(),
            [60.0, 70.0, 550.0, 740.0],
        );
        check(
            manual(&pdf, 2, seeds.get(2)).unwrap(),
            [60.0, 70.0, 550.0, 740.0],
        );
        check(
            full_auto(&pdf, 0, seeds.get(0), 1264, 10).unwrap(),
            [9.199, 103.199, 413.801, 653.801],
        );
        check(
            full_auto(&pdf, 1, seeds.get(1), 1264, 10).unwrap(),
            [114.299, 114.299, 594.701, 690.701],
        );
        check(
            full_auto(&pdf, 3, seeds.get(3), 1264, 10).unwrap(),
            [80.0, 80.0, 530.0, 745.0],
        );
        check(manual(&pdf, 1, None).unwrap(), [120.0, 120.0, 590.0, 686.0]);
    }

    #[test]
    fn semi_auto_crop_seed_precedence_uses_exact_parity_then_preceding_page() {
        let mut seeds = Seeds::default();
        let odd = odd_seed();
        let even = even_seed();
        assert_eq!(seeds.get(0), None);
        seeds.record(2, odd);
        assert_eq!(seeds.get(0), Some(odd)); // Physical pages 1 and 3 are odd.
        assert_eq!(seeds.get(1), None); // No seed preceding physical page 2.
        assert_eq!(seeds.get(3), Some(odd)); // Uses the preceding exact seed.
        seeds.record(1, even);
        assert_eq!(seeds.get(3), Some(even));
        let other = Seed { x0: 100.0, ..odd };
        seeds.record(4, other);
        assert_eq!(seeds.get(2), Some(odd));
        assert_eq!(seeds.get(0), Some(other));
        assert_eq!(seeds.get(6), Some(other));
        let encoded = serde_json::to_vec(&seeds).unwrap();
        assert_eq!(serde_json::from_slice::<Seeds>(&encoded).unwrap(), seeds);
        let reused = odd.normalized((400.0, 600.0)).unwrap();
        assert_eq!(reused.x0, 0.15); // The reused seed holds original points.
        assert_eq!(reused.x1, 1.0);
    }

    #[test]
    fn semi_auto_crop_detects_each_page_inside_its_seed_and_keeps_small_area() {
        let bytes = fixture();
        if let Some(path) = std::env::var_os("READER_SEMI_AUTO_ORACLE_FIXTURE") {
            std::fs::write(path, &bytes).unwrap();
        }
        let pdf = Pdf::open(bytes).unwrap();
        for (page, seed, expected) in [
            (0, odd_seed(), [167.09481, 147.09481, 411.90518, 651.9052]),
            (1, even_seed(), [116.12237, 116.12237, 442.87762, 542.8776]),
            (2, odd_seed(), [197.70259, 217.70259, 391.2974, 581.2974]),
            (3, even_seed(), [80.0, 80.0, 530.0, 745.0]),
        ] {
            let bounds = detect(&pdf, page, Some(seed), 1264, 10).unwrap();
            let bounds = Seed::from_normalized(bounds, SIZE);
            let edges = [bounds.x0, bounds.y0, bounds.x1, bounds.y1];
            eprintln!("semi-auto-source-bounds page={} {edges:?}", page + 1);
            for (actual, expected) in edges.into_iter().zip(expected) {
                assert!(
                    (actual - expected).abs() < 0.001,
                    "page {page} edge {actual} versus {expected}"
                );
            }
        }
    }

    #[test]
    #[ignore = "Set READER_SEMI_AUTO_ORIGINAL_PDF to an authorized unchanged PDF"]
    fn original_pdf_first_page_semi_auto_crop_verifies_source_without_content_output() {
        use ring::digest::{Context, SHA256, digest};
        use std::fs::File;
        use std::io::Read;
        use std::path::PathBuf;

        let path = PathBuf::from(
            std::env::var_os("READER_SEMI_AUTO_ORIGINAL_PDF")
                .expect("Set READER_SEMI_AUTO_ORIGINAL_PDF"),
        );
        assert!(
            std::fs::metadata(&path).is_ok_and(|entry| entry.is_file()),
            "Select a regular PDF"
        );
        let open =
            || File::open(&path).unwrap_or_else(|_| panic!("Could not read the selected PDF"));
        let mut bytes = Vec::new();
        open()
            .take(crate::fixed::MAX_PDF_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .unwrap_or_else(|_| panic!("Could not read the selected PDF"));
        assert!(
            bytes.len() <= crate::fixed::MAX_PDF_BYTES,
            "The selected PDF exceeds Reader's size limit"
        );
        let original = digest(&SHA256, &bytes);
        let pdf = Pdf::open(bytes).unwrap_or_else(|_| panic!("Could not open the selected PDF"));
        let size = pdf
            .page_size(0)
            .unwrap_or_else(|_| panic!("The selected PDF has no usable first page"));

        let coords = std::env::var("READER_SEMI_AUTO_ORIGINAL_SEED").ok().map_or(
            [0.05, 0.05, 0.95, 0.95],
            |setting| {
                setting
                    .split(',')
                    .map(|value| value.trim().parse::<f32>())
                    .collect::<Result<Vec<_>, _>>()
                    .ok()
                    .and_then(|values| values.try_into().ok())
                    .unwrap_or_else(|| {
                        panic!("Provide four comma-separated normalized seed coordinates")
                    })
            },
        );
        let [x0, y0, x1, y1] = coords;
        assert!(coords.into_iter().all(|value| (0.0..=1.0).contains(&value)));
        let seed = Seed::from_normalized(PdfTextRect { x0, y0, x1, y1 }, size);
        let area = seed
            .normalized(size)
            .expect("The seed needs a usable width and height");
        let crop = detect(
            &pdf,
            0,
            Some(seed),
            1264,
            crate::library::DEFAULT_CROP_MARGIN,
        )
        .unwrap_or_else(|_| panic!("Could not crop the selected PDF's first page"));
        let width = crop.x1 - crop.x0;
        let height = crop.y1 - crop.y0;
        assert!((0.025..=1.0).contains(&width) && (0.025..=1.0).contains(&height));
        assert!(crop.x0 >= 0.0 && crop.y0 >= 0.0 && crop.x1 <= 1.0 && crop.y1 <= 1.0);

        let mut verifier = Context::new(&SHA256);
        let mut source = open().take(crate::fixed::MAX_PDF_BYTES as u64 + 1);
        let mut buffer = [0; 64 * 1024];
        loop {
            let count = source
                .read(&mut buffer)
                .unwrap_or_else(|_| panic!("Could not re-read the selected PDF"));
            if count == 0 {
                break;
            }
            verifier.update(&buffer[..count]);
        }
        assert!(
            verifier.finish().as_ref() == original.as_ref(),
            "The source PDF bytes changed"
        );
        println!(
            "source_unchanged=true page=1 width={width:.4} height={height:.4} area={:.4} seed_fallback={}",
            width * height,
            crop == area
        );
    }
}
