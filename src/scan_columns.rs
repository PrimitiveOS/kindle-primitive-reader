//! Find clearly separated text columns in an upright, uncropped scan raster.

use std::ops::Range;

use crate::fixed_text::PdfTextRect;
use crate::ocr::{MAX_OCR_PIXELS, MAX_OCR_SIDE};

const MIN_RASTER_SIDE: u32 = 96;
const SAMPLE_WIDTH: usize = 320;
const SAMPLE_HEIGHT: usize = 448;
const VERTICAL_BANDS: usize = 10;

/// Return conservative body-text regions in physical left-to-right order.
/// All coordinates refer to the original raster's top-left-origin unit square.
#[cfg(test)]
pub(crate) fn detect_columns(pixels: &[u8], width: u32, height: u32) -> Option<[PdfTextRect; 2]> {
    if width.min(height) < MIN_RASTER_SIDE
        || width.max(height) > MAX_OCR_SIDE
        || width.checked_mul(height)? > MAX_OCR_PIXELS
        || pixels.len() != (width * height) as usize
    {
        return None;
    }
    let grid = ScanGrid::sample(pixels, width as usize, height as usize)?;
    grid.columns()
}

/// Prefer a clearly separated three-column page, then the existing two-column
/// detector. Returned rectangles are in physical left-to-right order.
pub(crate) fn detect_reading_columns(
    pixels: &[u8],
    width: u32,
    height: u32,
) -> Option<Vec<PdfTextRect>> {
    detect_reading_columns_with_limit(pixels, width, height, 3)
}

/// Find no more than the configured number of text columns. A maximum of one
/// deliberately disables automatic segmentation, matching KOReader's setting.
pub(crate) fn detect_reading_columns_with_limit(
    pixels: &[u8],
    width: u32,
    height: u32,
    maximum: u8,
) -> Option<Vec<PdfTextRect>> {
    if !(1..=3).contains(&maximum) {
        return None;
    }
    if maximum == 1 {
        return None;
    }
    if width.min(height) < MIN_RASTER_SIDE
        || width.max(height) > MAX_OCR_SIDE
        || width.checked_mul(height)? > MAX_OCR_PIXELS
        || pixels.len() != (width * height) as usize
    {
        return None;
    }
    let grid = ScanGrid::sample(pixels, width as usize, height as usize)?;
    match maximum {
        2 => grid.columns().map(Vec::from),
        3 => grid
            .three_columns()
            .map(Vec::from)
            .or_else(|| grid.columns().map(Vec::from)),
        _ => None,
    }
}

struct ScanGrid {
    width: usize,
    height: usize,
    ink: Vec<u8>,
}

impl ScanGrid {
    fn sample(pixels: &[u8], width: usize, height: usize) -> Option<Self> {
        let sample_width = width.min(SAMPLE_WIDTH);
        let sample_height = height.min(SAMPLE_HEIGHT);
        let xs = sample_axis(width, sample_width);
        let ys = sample_axis(height, sample_height);
        let mut dark_samples = Vec::with_capacity(sample_width * sample_height);
        let mut light_samples = Vec::with_capacity(sample_width * sample_height);
        let mut dark_histogram = [0; 256];
        let mut light_histogram = [0; 256];
        let page_x = interior(sample_width);
        let page_y = interior(sample_height);
        for (y, &(y0, y1)) in ys.iter().enumerate() {
            let top = &pixels[y0 * width..][..width];
            let bottom = &pixels[y1 * width..][..width];
            for (x, &(x0, x1)) in xs.iter().enumerate() {
                // Retain both extrema so a thin dark or light stroke survives
                // downsampling without assuming the source paper is white.
                let values = [top[x0], top[x1], bottom[x0], bottom[x1]];
                let dark = *values.iter().min()?;
                let light = *values.iter().max()?;
                dark_samples.push(dark);
                light_samples.push(light);
                if page_x.contains(&x) && page_y.contains(&y) {
                    dark_histogram[usize::from(dark)] += 1;
                    light_histogram[usize::from(light)] += 1;
                }
            }
        }
        let count = page_x.len() * page_y.len();
        let dark = percentile(&dark_histogram, (count * 2).div_ceil(100));
        let paper = percentile(&dark_histogram, (count * 65).div_ceil(100));
        let dark_contrast = paper.saturating_sub(dark);
        let ink = if paper >= 100 && dark_contrast >= 16 {
            let threshold = paper - (dark_contrast / 3).clamp(12, 48);
            for gray in &mut dark_samples {
                *gray = u8::from(*gray <= threshold);
            }
            dark_samples
        } else {
            let paper = percentile(&light_histogram, (count * 65).div_ceil(100));
            let light = percentile(&light_histogram, (count * 98).div_ceil(100));
            let contrast = light.saturating_sub(paper);
            if contrast < 16 || paper >= 160 {
                return None;
            }
            let threshold = paper.saturating_add((contrast / 3).clamp(12, 48));
            for gray in &mut light_samples {
                *gray = u8::from(*gray >= threshold);
            }
            light_samples
        };
        Some(Self {
            width: sample_width,
            height: sample_height,
            ink,
        })
    }

    fn columns(&self) -> Option<[PdfTextRect; 2]> {
        let vertical = interior(self.height);
        let mut projection = [0; SAMPLE_WIDTH];
        for y in vertical.clone() {
            for (count, &ink) in projection.iter_mut().zip(self.row(y)) {
                *count += usize::from(ink);
            }
        }
        let low_ink = vertical.len().div_ceil(20);
        let min_width = (self.width * 3).div_ceil(100);
        let max_width = self.width / 5;
        // A central gutter may be off-center, but both body columns must have room.
        let search = self.width * 29 / 100..self.width * 71 / 100;
        let ends = projection[search.clone()]
            .iter()
            .enumerate()
            .filter_map(|(offset, &ink)| (ink > low_ink).then_some(search.start + offset))
            .chain(std::iter::once(search.end));
        let mut found = None;
        let mut start = search.start;
        for x in ends {
            if (min_width..=max_width).contains(&(x - start))
                && let Some(columns) = self.evaluate(start..x, vertical.clone())
            {
                if found.is_some() {
                    return None;
                }
                found = Some(columns);
            }
            start = x + 1;
        }
        found
    }

    fn three_columns(&self) -> Option<[PdfTextRect; 3]> {
        let vertical = interior(self.height);
        let mut projection = [0; SAMPLE_WIDTH];
        for y in vertical.clone() {
            for (count, &ink) in projection.iter_mut().zip(self.row(y)) {
                *count += usize::from(ink);
            }
        }
        let low_ink = vertical.len().div_ceil(20);
        let min_width = (self.width * 3).div_ceil(100);
        let max_width = self.width / 5;
        let search = self.width / 5..self.width * 4 / 5;
        let mut gutters = Vec::with_capacity(2);
        let mut start = search.start;
        for x in projection[search.clone()]
            .iter()
            .enumerate()
            .filter_map(|(offset, &ink)| (ink > low_ink).then_some(search.start + offset))
            .chain(std::iter::once(search.end))
        {
            if (min_width..=max_width).contains(&(x - start)) {
                gutters.push(start..x);
                if gutters.len() > 2 {
                    return None;
                }
            }
            start = x + 1;
        }
        let [left_gutter, right_gutter] = gutters.as_slice() else {
            return None;
        };
        let horizontal = interior(self.width);
        let columns = [
            horizontal.start..left_gutter.start,
            left_gutter.end..right_gutter.start,
            right_gutter.end..horizontal.end,
        ];
        if columns.iter().any(|column| column.len() < self.width / 5) {
            return None;
        }
        let minimum_ink = (self.width / 100).max(2);
        let mut bodies = [Body::default(), Body::default(), Body::default()];
        let mut dirty_rows = 0;
        let mut active_rows = 0;
        for y in vertical {
            let row = self.row(y);
            let active = columns
                .each_ref()
                .map(|column| ink_count(&row[(*column).clone()]) >= minimum_ink);
            let any = active.iter().any(|&value| value);
            let dirty = any
                && gutters
                    .iter()
                    .any(|gutter| ink_count(&row[(*gutter).clone()]) >= (gutter.len() / 9).max(2));
            active_rows += usize::from(any);
            dirty_rows += usize::from(dirty);
            let band = y * VERTICAL_BANDS / self.height;
            for ((body, column), is_active) in bodies.iter_mut().zip(&columns).zip(active) {
                body.observe(y, band, &row[(*column).clone()], is_active && !dirty);
            }
        }
        if dirty_rows * 8 > active_rows
            || (bodies[0].bands & bodies[1].bands & bodies[2].bands).count_ones() < 3
        {
            return None;
        }
        Some([
            bodies[0].rectangle(self, columns[0].clone(), 0..left_gutter.start)?,
            bodies[1].rectangle(
                self,
                columns[1].clone(),
                left_gutter.end..right_gutter.start,
            )?,
            bodies[2].rectangle(self, columns[2].clone(), right_gutter.end..self.width)?,
        ])
    }

    fn evaluate(&self, gutter: Range<usize>, vertical: Range<usize>) -> Option<[PdfTextRect; 2]> {
        let horizontal = interior(self.width);
        let left = horizontal.start..gutter.start;
        let right = gutter.end..horizontal.end;
        let minimum_ink = (self.width / 100).max(2);
        let gutter_noise = (gutter.len() / 9).max(2);
        let mut left_body = Body::default();
        let mut right_body = Body::default();
        let mut dirty_rows = 0;
        let mut active_rows = 0;
        for y in vertical {
            let row = self.row(y);
            let left_active = ink_count(&row[left.clone()]) >= minimum_ink;
            let right_active = ink_count(&row[right.clone()]) >= minimum_ink;
            let active = left_active || right_active;
            let dirty = active && ink_count(&row[gutter.clone()]) >= gutter_noise;
            active_rows += usize::from(active);
            dirty_rows += usize::from(dirty);

            // A page-wide title or footer can cross the gutter once in a while.
            let band = y * VERTICAL_BANDS / self.height;
            left_body.observe(y, band, &row[left.clone()], left_active && !dirty);
            right_body.observe(y, band, &row[right.clone()], right_active && !dirty);
        }
        if dirty_rows * 8 > active_rows || (left_body.bands & right_body.bands).count_ones() < 3 {
            return None;
        }
        Some([
            left_body.rectangle(self, left, 0..gutter.start)?,
            right_body.rectangle(self, right, gutter.end..self.width)?,
        ])
    }

    fn row(&self, y: usize) -> &[u8] {
        &self.ink[y * self.width..][..self.width]
    }
}

struct Body {
    column_ink: [u16; SAMPLE_WIDTH],
    active_rows: usize,
    line_runs: usize,
    in_run: bool,
    bands: u16,
    top: usize,
    bottom: usize,
}

impl Default for Body {
    fn default() -> Self {
        Self {
            column_ink: [0; SAMPLE_WIDTH],
            active_rows: 0,
            line_runs: 0,
            in_run: false,
            bands: 0,
            top: 0,
            bottom: 0,
        }
    }
}

impl Body {
    fn observe(&mut self, y: usize, band: usize, row: &[u8], active: bool) {
        if active {
            if self.active_rows == 0 {
                self.top = y;
            }
            self.bottom = y + 1;
            self.active_rows += 1;
            self.line_runs += usize::from(!self.in_run);
            self.bands |= 1 << band;
            for (count, &ink) in self.column_ink.iter_mut().zip(row) {
                *count += u16::from(ink);
            }
        }
        self.in_run = active;
    }

    fn rectangle(
        &self,
        grid: &ScanGrid,
        sampled: Range<usize>,
        limit: Range<usize>,
    ) -> Option<PdfTextRect> {
        if self.active_rows < grid.height.div_ceil(40) || self.line_runs < 4 {
            return None;
        }
        let repeat = (self.active_rows.div_ceil(50) as u16).max(2);
        let ink = &self.column_ink[..sampled.len()];
        let left = sampled.start + ink.iter().position(|&count| count >= repeat)?;
        let right = sampled.start + ink.iter().rposition(|&count| count >= repeat)? + 1;
        if right - left < grid.width / 5 {
            return None;
        }

        let horizontal_pad = grid.width.div_ceil(64);
        let vertical_pad = grid.height.div_ceil(72);
        Some(PdfTextRect {
            x0: left.saturating_sub(horizontal_pad).max(limit.start) as f32 / grid.width as f32,
            x1: (right + horizontal_pad).min(limit.end) as f32 / grid.width as f32,
            y0: self.top.saturating_sub(vertical_pad) as f32 / grid.height as f32,
            y1: (self.bottom + vertical_pad).min(grid.height) as f32 / grid.height as f32,
        })
    }
}

fn sample_axis(original: usize, sampled: usize) -> Vec<(usize, usize)> {
    (0..sampled)
        .map(|index| {
            let first = index * original / sampled;
            let last = (index + 1) * original / sampled - 1;
            let inset = (last - first) / 4;
            (first + inset, last - inset)
        })
        .collect()
}

fn interior(side: usize) -> Range<usize> {
    let margin = side / 25;
    margin..side - margin
}

fn percentile(histogram: &[usize; 256], rank: usize) -> u8 {
    let mut count = 0;
    histogram
        .iter()
        .position(|bin| {
            count += bin;
            count >= rank
        })
        .unwrap_or(255) as u8
}

fn ink_count(row: &[u8]) -> usize {
    row.iter().map(|&ink| usize::from(ink)).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: usize = 240;
    const H: usize = 320;

    fn text(pixels: &mut [u8], x: Range<usize>, y: Range<usize>, lines: usize, tone: u8) {
        for line in 0..lines {
            let top = y.start + line * y.len() / lines;
            let mut left = x.start + line % 3;
            let mut word = 0;
            while left < x.end {
                let length = 5 + (line * 5 + word * 7) % 13;
                let right = (left + length).min(x.end);
                for row in top..top + 5 {
                    pixels[row * W + left..row * W + right].fill(tone);
                }
                left = right + 3;
                word += 1;
            }
        }
    }

    fn detect(pixels: &[u8]) -> Option<[PdfTextRect; 2]> {
        detect_columns(pixels, W as u32, H as u32)
    }

    fn paired(paper: u8, ink: u8) -> Vec<u8> {
        let mut page = vec![paper; W * H];
        text(&mut page, 27..111, 61..276, 13, ink);
        text(&mut page, 132..223, 65..274, 12, ink);
        page
    }

    #[test]
    fn detects_paired_columns_on_pale_scans_with_staggered_baselines() {
        let original = paired(230, 207);
        let [left, right] = detect(&original).unwrap();
        assert!((0.08..0.13).contains(&left.x0));
        assert!((0.45..0.5).contains(&left.x1));
        assert!((0.52..0.56).contains(&right.x0));
        assert!((0.91..0.97).contains(&right.x1));
        assert!(left.y0 < 0.2 && right.y1 > 0.8);
        assert!(left.x1 < right.x0);

        let scale = 4;
        let larger: Vec<_> = (0..H * scale)
            .flat_map(|y| (0..W * scale).map(move |x| (y / scale) * W + x / scale))
            .map(|index| original[index])
            .collect();
        let [scaled_left, scaled_right] =
            detect_columns(&larger, (W * scale) as u32, (H * scale) as u32).unwrap();
        assert!((left.x0 - scaled_left.x0).abs() < 0.02);
        assert!((right.x1 - scaled_right.x1).abs() < 0.02);
    }

    #[test]
    fn detects_light_text_on_dark_scans_without_reversing_page_geometry() {
        let ordinary = detect(&paired(240, 35)).unwrap();
        for (paper, ink) in [(0, 255), (32, 220), (75, 145)] {
            let inverted = detect(&paired(paper, ink)).unwrap();
            assert_eq!(inverted, ordinary, "paper {paper}, ink {ink}");
        }
        // Blank and single-column dark pages do not acquire false gutters.
        assert!(detect(&vec![0; W * H]).is_none());
        let mut single = vec![24; W * H];
        text(&mut single, 25..212, 54..283, 16, 225);
        assert!(detect(&single).is_none());
        let mut three = vec![20; W * H];
        for column in [14..70, 84..151, 166..225] {
            text(&mut three, column, 56..284, 13, 225);
        }
        assert!(detect(&three).is_none());
    }

    #[test]
    fn dark_scan_sampling_preserves_fine_light_strokes_when_scaled() {
        let original = paired(22, 230);
        let [left, right] = detect(&original).unwrap();
        let scale = 4;
        let larger: Vec<_> = (0..H * scale)
            .flat_map(|y| (0..W * scale).map(move |x| (y / scale) * W + x / scale))
            .map(|index| original[index])
            .collect();
        let [scaled_left, scaled_right] =
            detect_columns(&larger, (W * scale) as u32, (H * scale) as u32).unwrap();
        assert!((left.x0 - scaled_left.x0).abs() < 0.02);
        assert!((right.x1 - scaled_right.x1).abs() < 0.02);
    }

    #[test]
    fn allows_a_spanning_heading_and_returns_only_body_columns() {
        let mut page = paired(245, 54);
        text(&mut page, 26..215, 24..40, 1, 12);
        let [left, right] = detect(&page).unwrap();
        assert!(left.y0 > 40.0 / H as f32);
        assert!(right.y0 > 40.0 / H as f32);
        assert!(left.x1 < right.x0);
    }

    #[test]
    fn rejects_blank_single_column_and_narrow_margin_notes() {
        assert!(detect(&vec![255; W * H]).is_none());
        let mut single = vec![246; W * H];
        text(&mut single, 25..212, 54..283, 16, 62);
        assert!(detect(&single).is_none());
        let mut margin_notes = vec![246; W * H];
        text(&mut margin_notes, 27..170, 54..283, 16, 62);
        text(&mut margin_notes, 189..216, 56..268, 12, 62);
        assert!(detect(&margin_notes).is_none());
    }

    #[test]
    fn rejects_multiple_credible_gutters_and_sparse_horizontal_bands() {
        let mut three = vec![250; W * H];
        for column in [14..70, 84..151, 166..225] {
            text(&mut three, column, 56..284, 13, 60);
        }
        assert!(detect(&three).is_none());

        let mut few = vec![250; W * H];
        text(&mut few, 20..108, 80..103, 2, 42);
        text(&mut few, 134..220, 83..106, 2, 42);
        assert!(detect(&few).is_none());
    }

    #[test]
    fn rejects_invalid_raster_sizes_before_decoding() {
        assert!(detect(&[0]).is_none());
        assert!(detect_columns(&[], u32::MAX, u32::MAX).is_none());
        assert!(detect_columns(&[], 4096, 4096).is_none());
    }
}
