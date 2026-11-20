//! Conservative panel detection for comics with light gutters and closed rectangular frames.

use crate::Result;
use crate::comic::ImagePage;
use crate::fixed::{MAX_VIEWPORT_SIDE, RenderRequest};

const SAMPLE_SIDE: f32 = 384.0;
const MAX_PANELS: usize = 12;
const MIN_GUTTER: u32 = 3;
const MIN_PANEL_SIDE: u32 = 24;
const PAPER: u8 = 232;
const INK: u8 = 175;
const EDGE_INK: u8 = 92;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Bounds {
    left: u32,
    top: u32,
    right: u32,
    bottom: u32,
}

impl Bounds {
    fn width(self) -> u32 {
        self.right - self.left
    }

    fn height(self) -> u32 {
        self.bottom - self.top
    }

    fn contains(self, x: u32, y: u32) -> bool {
        (self.left..self.right).contains(&x) && (self.top..self.bottom).contains(&y)
    }
}

struct Sample<'a> {
    width: u32,
    height: u32,
    pixels: &'a [u8],
}

#[derive(Clone, Copy)]
enum Axis {
    Row,
    Column,
}

impl Axis {
    fn coordinates(self, bounds: Bounds) -> (u32, u32, u32, u32) {
        match self {
            Self::Row => (bounds.top, bounds.bottom, bounds.left, bounds.right),
            Self::Column => (bounds.left, bounds.right, bounds.top, bounds.bottom),
        }
    }

    fn pixel(self, sample: &Sample<'_>, line: u32, offset: u32) -> u8 {
        let (x, y) = match self {
            Self::Row => (offset, line),
            Self::Column => (line, offset),
        };
        sample.pixels[(y * sample.width + x) as usize]
    }
}

/// Crop in intrinsic source coordinates. Rotation and zoom belong to the caller's viewport.
pub(super) fn panel_at(image: &ImagePage, x: f32, y: f32) -> Result<Option<ImagePage>> {
    let (source_width, source_height) = image.page_size();
    if !(0.0..source_width).contains(&x) || !(0.0..source_height).contains(&y) {
        return Ok(None);
    }
    let scale = (SAMPLE_SIDE / source_width.max(source_height)).min(1.0);
    let width = (source_width * scale).ceil() as u32;
    let height = (source_height * scale).ceil() as u32;
    if width < MIN_PANEL_SIDE || height < MIN_PANEL_SIDE {
        return Ok(None);
    }
    let pixels = image.render(RenderRequest {
        width,
        height,
        scale,
        x: 0.0,
        y: 0.0,
        turns: 0,
    })?;
    let sample = Sample {
        width,
        height,
        pixels: &pixels,
    };
    let Some(panel) = sample.panels().and_then(|panels| {
        let x = (x * scale) as u32;
        let y = (y * scale) as u32;
        panels.into_iter().find(|panel| panel.contains(x, y))
    }) else {
        return Ok(None);
    };
    let left = (panel.left as f32 / scale).floor();
    let top = (panel.top as f32 / scale).floor();
    let right = (panel.right as f32 / scale).ceil().min(source_width);
    let bottom = (panel.bottom as f32 / scale).ceil().min(source_height);
    let scale = (MAX_VIEWPORT_SIDE as f32 / (right - left).max(bottom - top)).min(1.0);
    let width = (((right - left) * scale).ceil() as u32).min(MAX_VIEWPORT_SIDE);
    let height = (((bottom - top) * scale).ceil() as u32).min(MAX_VIEWPORT_SIDE);
    let pixels = image.render(RenderRequest {
        width,
        height,
        scale,
        x: -left * scale,
        y: -top * scale,
        turns: 0,
    })?;
    ImagePage::from_grayscale(width, height, pixels).map(Some)
}

impl Sample<'_> {
    fn panels(&self) -> Option<Vec<Bounds>> {
        let canvas = Bounds {
            left: 0,
            top: 0,
            right: self.width,
            bottom: self.height,
        };
        let mut found = Vec::new();
        for (top, bottom) in self.bands(canvas, Axis::Row)? {
            let row = Bounds {
                top,
                bottom,
                ..canvas
            };
            for (left, right) in self.bands(row, Axis::Column)? {
                let panel = self.ink_bounds(Bounds { left, right, ..row })?;
                if found.len() == MAX_PANELS || !self.is_framed(panel) {
                    return None;
                }
                found.push(panel);
            }
        }
        (found.len() >= 2).then_some(found)
    }

    fn bands(&self, bounds: Bounds, axis: Axis) -> Option<Vec<(u32, u32)>> {
        let (begin, end, first, last) = axis.coordinates(bounds);
        let noise = (last - first) / 50;
        let mut bands = Vec::new();
        let mut content = None::<(u32, u32)>;
        for line in begin..end {
            let occupied = (first..last)
                .filter(|&i| axis.pixel(self, line, i) < PAPER)
                .take(noise as usize + 1)
                .count()
                > noise as usize;
            if !occupied {
                continue;
            }
            if let Some((start, stop)) = content {
                if line - stop < MIN_GUTTER {
                    content = Some((start, line + 1));
                    continue;
                }
                bands.push((start, stop));
                if bands.len() == MAX_PANELS {
                    return None;
                }
            }
            content = Some((line, line + 1));
        }
        bands.extend(content);
        Some(bands)
    }

    fn ink_bounds(&self, area: Bounds) -> Option<Bounds> {
        let mut ink = Bounds {
            left: area.right,
            top: area.bottom,
            right: 0,
            bottom: 0,
        };
        for y in area.top..area.bottom {
            for x in area.left..area.right {
                if Axis::Row.pixel(self, y, x) <= INK {
                    ink.left = ink.left.min(x);
                    ink.top = ink.top.min(y);
                    ink.right = ink.right.max(x + 1);
                    ink.bottom = ink.bottom.max(y + 1);
                }
            }
        }
        (ink.right > ink.left && ink.bottom > ink.top).then_some(ink)
    }

    fn is_framed(&self, panel: Bounds) -> bool {
        let (width, height) = (panel.width(), panel.height());
        if width < MIN_PANEL_SIDE
            || height < MIN_PANEL_SIDE
            || width * height * 24 < self.width * self.height
        {
            return false;
        }
        // An isolated dark block without any lighter interior is not a panel frame.
        let inset = (width.min(height) / 8).clamp(2, 8);
        let interior = (width - 2 * inset) * (height - 2 * inset);
        let light_needed = interior.div_ceil(32) as usize;
        let light = (panel.top + inset..panel.bottom - inset)
            .flat_map(|y| (panel.left + inset..panel.right - inset).map(move |x| (x, y)))
            .filter(|&(x, y)| Axis::Row.pixel(self, y, x) > EDGE_INK)
            .take(light_needed)
            .count();
        light >= light_needed
            && [Axis::Row, Axis::Column].into_iter().all(|axis| {
                let (begin, end, first, last) = axis.coordinates(panel);
                let inset = (end - begin).div_ceil(32).clamp(2, 6);
                let needed = ((last - first) * 4).div_ceil(5) as usize;
                let edge = |lines: std::ops::Range<u32>| {
                    lines.into_iter().any(|line| {
                        (first..last)
                            .filter(|&offset| axis.pixel(self, line, offset) <= EDGE_INK)
                            .take(needed)
                            .count()
                            >= needed
                    })
                };
                edge(begin..begin + inset) && edge(end - inset..end)
            })
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    pub(crate) const WIDTH: u32 = 320;
    pub(crate) const HEIGHT: u32 = 480;

    pub(crate) fn grid() -> Vec<u8> {
        let mut page = vec![255; (WIDTH * HEIGHT) as usize];
        for (index, (left, top)) in [(16, 16), (166, 16), (16, 246), (166, 246)]
            .into_iter()
            .enumerate()
        {
            let color = [197, 131, 95, 220][index];
            for y in top..top + 218 {
                for x in left..left + 138 {
                    let border = x < left + 4 || x >= left + 134 || y < top + 4 || y >= top + 214;
                    page[(y * WIDTH + x) as usize] = if border { 24 } else { color };
                }
            }
        }
        page
    }

    #[test]
    fn distinguishes_crops_and_leaves_gutters_alone() {
        let image = ImagePage::from_grayscale(WIDTH, HEIGHT, grid()).unwrap();
        for ((x, y), value) in [
            ((60.0, 60.0), 197),
            ((230.0, 60.0), 131),
            ((60.0, 310.0), 95),
            ((230.0, 310.0), 220),
        ] {
            let panel = panel_at(&image, x, y).unwrap().unwrap();
            assert!(panel.page_size().0 < WIDTH as f32 / 2.0);
            assert!(panel.page_size().1 < HEIGHT as f32 / 2.0);
            assert_eq!(panel.fit(100, 100, 0).unwrap()[50 * 100 + 50], value);
        }
        for (x, y) in [(0.0, 0.0), (160.0, 60.0), (60.0, 240.0), (f32::NAN, 60.0)] {
            assert!(panel_at(&image, x, y).unwrap().is_none());
        }
    }

    #[test]
    fn rejects_full_bleed_or_an_ambiguous_partial_frame() {
        let full =
            ImagePage::from_grayscale(WIDTH, HEIGHT, vec![80; (WIDTH * HEIGHT) as usize]).unwrap();
        assert!(panel_at(&full, 60.0, 60.0).unwrap().is_none());
        let mut uncertain = grid();
        for y in 16..234 {
            for x in 166..304 {
                uncertain[(y * WIDTH + x) as usize] = if x < 210 { 120 } else { 250 };
            }
        }
        // A single irregular object cannot certify a multi-panel page.
        let image = ImagePage::from_grayscale(WIDTH, HEIGHT, uncertain).unwrap();
        assert!(panel_at(&image, 60.0, 60.0).unwrap().is_none());
    }
}
