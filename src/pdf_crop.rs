//! Source-space seed selection and constrained PDF margin detection.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::fixed::{Pdf, RenderRequest};
use crate::fixed_text::PdfTextRect;
use crate::scan_deskew::FULL_PAGE;

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
) -> crate::Result<PdfTextRect> {
    let size = pdf.page_size(page)?;
    let area = seed
        .and_then(|seed| seed.normalized(size))
        .unwrap_or(FULL_PAGE);
    let area_width = (area.x1 - area.x0) * size.0;
    let area_height = (area.y1 - area.y0) * size.1;
    let scale = 1536.0 / area_width.max(area_height);
    let width = (area_width * scale).ceil() as u32;
    let height = (area_height * scale).ceil() as u32;
    let pixels = pdf.render(
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
    let crop = bounds(&pixels, width as usize, area)
        .and_then(|ink| padded_native_bounds(ink, size, screen_width));
    Ok(crop.unwrap_or(area))
}

fn bounds(pixels: &[u8], width: usize, area: PdfTextRect) -> Option<PdfTextRect> {
    let height = pixels.len() / width;
    let mut rows = vec![0; height];
    let mut columns = vec![0; width];
    for (y, line) in pixels.chunks_exact(width).enumerate() {
        for (x, ink) in line.iter().enumerate() {
            if *ink < 192 {
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

fn padded_native_bounds(
    ink: PdfTextRect,
    size: (f32, f32),
    screen_width: u32,
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
    // On the 300-dpi reference display, 0.1 inch of reserved screen
    // width fits across both margins after the content scales up.
    const SCREEN_MARGIN: f32 = 30.0;
    let screen_fraction = SCREEN_MARGIN / (screen_width as f32 - SCREEN_MARGIN);
    let half_margin = (points.x1 - points.x0) * screen_fraction * 0.5;
    points.x0 -= half_margin;
    points.x1 += half_margin;
    points.y0 -= half_margin;
    points.y1 += half_margin;
    let rect = points.normalized(size)?;
    let (w, h) = (rect.x1 - rect.x0, rect.y1 - rect.y0);
    (!(w < 0.1 && h < 0.1)).then_some(rect)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) const SIZE: (f32, f32) = (600.0, 800.0);

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
            let bounds = detect(&pdf, page, Some(seed), 1264).unwrap();
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
        let crop = detect(&pdf, 0, Some(seed), 1264)
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
