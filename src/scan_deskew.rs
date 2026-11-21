//! Detect a dominant text baseline in a bounded, upright preview of a scanned page.
//! The correction is a display transform. Saved rectangles and OCR remain in source space.

use std::collections::HashMap;
use std::ops::Range;

use hayro::kurbo::{Affine, Point, Rect};

use crate::Result;
use crate::fixed::RenderRequest;
use crate::fixed_text::PdfTextRect;

const PREVIEW_SIDE: f32 = 400.0;
const MAX_POINTS: usize = 12_000;

#[derive(Default)]
pub(crate) struct DeskewCache {
    // One small entry per visited page, bounded by the document provider's page limit.
    angles: HashMap<usize, f32>,
}

impl DeskewCache {
    #[cfg(test)]
    pub(crate) fn angles_count(&self) -> usize {
        self.angles.len()
    }

    pub(crate) fn correction(
        &mut self,
        page: usize,
        size: (f32, f32),
        limit: u8,
        crop: Option<PdfTextRect>,
        render: impl FnOnce(RenderRequest) -> Result<Vec<u8>>,
    ) -> Result<Affine> {
        if limit == 0 {
            return Ok(Affine::IDENTITY);
        }
        let angle = match self.angles.entry(page) {
            std::collections::hash_map::Entry::Occupied(entry) => *entry.get(),
            std::collections::hash_map::Entry::Vacant(entry) => {
                let scale = PREVIEW_SIDE / size.0.max(size.1);
                let width = (size.0 * scale).round().max(1.0) as u32;
                let height = (size.1 * scale).round().max(1.0) as u32;
                let pixels = render(RenderRequest {
                    width,
                    height,
                    scale,
                    x: 0.0,
                    y: 0.0,
                    turns: 0,
                })?;
                *entry.insert(estimate(&pixels, width as usize, height as usize))
            }
        };
        if angle == 0.0 || angle.abs() > f32::from(limit) {
            return Ok(Affine::IDENTITY);
        }
        let bounds = source_rect(size, crop.unwrap_or(FULL_PAGE));
        let center = bounds.center().to_vec2();
        let rotate = Affine::rotate(f64::from(-angle).to_radians());
        let rotated = rotate.transform_rect_bbox(bounds);
        // Keep all four corners of the chosen page/crop inside the existing fit or scroll slot.
        let fit = (bounds.width() / rotated.width()).min(bounds.height() / rotated.height());
        Ok(Affine::translate(center) * Affine::scale(fit) * rotate * Affine::translate(-center))
    }
}

fn estimate(pixels: &[u8], width: usize, height: usize) -> f32 {
    if width < 40 || height < 40 || pixels.len() != width * height {
        return 0.0;
    }
    // Omit the outside margins. A dark binding or scanner bed can dominate a sparse page.
    let pad_x = (width / 24).max(1);
    let pad_y = (height / 24).max(1);
    let mut points = Vec::new();
    for y in pad_y..height - pad_y {
        for x in pad_x..width - pad_x {
            let i = y * width + x;
            // Leading dark edges give thin strokes and thick strokes comparable weight.
            if pixels[i] < 195 && i16::from(pixels[i - width]) - i16::from(pixels[i]) > 25 {
                points.push((x as i32 - (width / 2) as i32, y as i32));
            }
        }
    }
    if points.len() < 100 {
        return 0.0;
    }
    let stride = points.len().div_ceil(MAX_POINTS);
    let mut histogram = vec![0_u32; height + width + 4];
    let mut score = |angle: f32| -> u64 {
        histogram.fill(0);
        let slope = (angle.to_radians().tan() * 1024.0).round() as i32;
        for &(x, y) in points.iter().step_by(stride) {
            let row = y + width as i32 / 2 + ((512 - x * slope) >> 10);
            histogram[row as usize] += 1;
        }
        // Aligned baselines concentrate horizontal edges into well-separated row peaks.
        histogram.iter().map(|&n| u64::from(n) * u64::from(n)).sum()
    };
    let level = score(0.0);
    let mut best = (level, 0.0);
    for whole in -26..=26 {
        let angle = whole as f32;
        let candidate = score(angle);
        if candidate > best.0 {
            best = (candidate, angle);
        }
    }
    let center = best.1;
    for quarter in -3..=3 {
        let angle = center + quarter as f32 * 0.25;
        let candidate = score(angle);
        if candidate > best.0 {
            best = (candidate, angle);
        }
    }
    let center = best.1;
    for step in -3..=3 {
        let angle = center + step as f32 * 0.05;
        let candidate = score(angle);
        if candidate > best.0 {
            best = (candidate, angle);
        }
    }
    if best.1.abs() < 0.5 || best.0 * 100 < level * 125 {
        return 0.0;
    }
    best.1
}

pub(crate) const FULL_PAGE: PdfTextRect = PdfTextRect {
    x0: 0.0,
    y0: 0.0,
    x1: 1.0,
    y1: 1.0,
};

pub(crate) fn source_rect(size: (f32, f32), r: PdfTextRect) -> Rect {
    let (w, h) = (f64::from(size.0), f64::from(size.1));
    Rect::new(
        f64::from(r.x0) * w,
        f64::from(r.y0) * h,
        f64::from(r.x1) * w,
        f64::from(r.y1) * h,
    )
}

/// Four projected source corners. Raster masks and highlights use the polygon, never its
/// axis-aligned bounding box, which can include neighboring words on a tilted page.
pub(crate) struct Quad([Point; 4]);

impl Quad {
    pub(crate) fn project(rect: Rect, transform: Affine) -> Self {
        Self([
            transform * Point::new(rect.x0, rect.y0),
            transform * Point::new(rect.x1, rect.y0),
            transform * Point::new(rect.x1, rect.y1),
            transform * Point::new(rect.x0, rect.y1),
        ])
    }

    pub(crate) fn bounds(&self) -> Rect {
        self.0
            .iter()
            .fold(Rect::from_points(self.0[0], self.0[0]), |r, &p| {
                r.union_pt(p)
            })
    }

    pub(crate) fn row(&self, y: usize, width: usize) -> Range<usize> {
        let scan = y as f64 + 0.5;
        let mut left = f64::INFINITY;
        let mut right = f64::NEG_INFINITY;
        for (a, b) in self.0.iter().zip(self.0.iter().cycle().skip(1)) {
            if a.y.min(b.y) <= scan && scan < a.y.max(b.y) {
                let x = a.x + (scan - a.y) / (b.y - a.y) * (b.x - a.x);
                left = left.min(x);
                right = right.max(x);
            }
        }
        if left >= right {
            return 0..0;
        }
        let pixel = |x: f64| ((x - 0.5).ceil() as usize).min(width);
        pixel(left)..pixel(right)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(angle: f32) -> Vec<u8> {
        let mut page = vec![247; 320 * 360];
        let slope = angle.to_radians().tan();
        for top in (62..285).step_by(18) {
            for x in 24..296 {
                if x % 39 > 30 || x % 7 > 4 {
                    continue;
                }
                let y = top + (((x - 160) as f32) * slope).round() as i32;
                for dy in 0..4 {
                    page[((y + dy) * 320 + x) as usize] = 30;
                }
            }
        }
        page
    }

    #[test]
    fn detects_both_slopes_and_leaves_blank_or_upright_scans_alone() {
        for actual in [-14.25, -5.0, 0.0, 3.5, 24.0] {
            assert!((estimate(&sample(actual), 320, 360) - actual).abs() <= 0.5);
        }
        assert_eq!(estimate(&vec![196; 320 * 360], 320, 360), 0.0);
    }
}
