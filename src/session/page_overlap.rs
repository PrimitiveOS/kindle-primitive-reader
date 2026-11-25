//! Paint the repeated source area after a view-sized move.
use primitive_ui::{GrayImage, Rect};

use crate::PageOverlapStyle;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Marker {
    region: Rect,
    boundary: i32,
    marker_left: i32,
}

impl Marker {
    pub(crate) fn from_pan(clip: Rect, dx: i32, dy: i32) -> Option<Self> {
        let width = clip.w.saturating_sub(dx.saturating_abs());
        let height = clip.h.saturating_sub(dy.saturating_abs());
        if (dx == 0 && dy == 0) || width <= 0 || height <= 0 {
            return None;
        }
        let region = Rect::new(clip.x + dx.max(0), clip.y + dy.max(0), width, height);
        Some(Self {
            region,
            boundary: if dy > 0 { region.y } else { region.bottom() },
            marker_left: clip.x,
        })
    }

    pub(super) fn scroll(frame: &GrayImage, forward: bool, requested_height: u32) -> Option<Self> {
        let height = requested_height.min(frame.height) as i32;
        if height == 0 || frame.width == 0 {
            return None;
        }
        let top = if forward {
            0
        } else {
            frame.height as i32 - height
        };
        Some(Self {
            region: Rect::new(0, top, frame.width as i32, height),
            boundary: if forward { height } else { top }.min(frame.height as i32 - 1),
            marker_left: 0,
        })
    }

    pub(crate) fn paint(self, frame: &mut GrayImage, style: PageOverlapStyle) {
        const LINE_HEIGHT: i32 = 3;
        const LINE_GRAY: u8 = 136;
        let width = frame.width as usize;
        match style {
            PageOverlapStyle::None => {}
            PageOverlapStyle::Dim => {
                let first = self.region.x.max(0) as usize;
                let end = (self.region.right().max(0) as usize).min(width);
                if first >= end {
                    return;
                }
                for row in self.region.y.max(0)..self.region.bottom().min(frame.height as i32) {
                    let at = row as usize * width;
                    for pixel in &mut frame.pixels[at + first..at + end] {
                        *pixel = ((u16::from(*pixel) + 255) / 2) as u8;
                    }
                }
            }
            PageOverlapStyle::Arrow => {
                paint_arrow(frame, self.marker_left, self.boundary);
            }
            PageOverlapStyle::Line | PageOverlapStyle::DashedLine => {
                let first = self.marker_left.max(0) as usize;
                let end = ((self.marker_left + self.region.w).max(0) as usize).min(width);
                if first >= end {
                    return;
                }
                for row in
                    self.boundary.max(0)..(self.boundary + LINE_HEIGHT).min(frame.height as i32)
                {
                    let at = row as usize * width;
                    let line = &mut frame.pixels[at + first..at + end];
                    if style == PageOverlapStyle::Line {
                        line.fill(LINE_GRAY);
                    } else {
                        for dash in line.chunks_exact_mut(20) {
                            dash[..14].fill(LINE_GRAY);
                        }
                    }
                }
            }
        }
    }
}

fn paint_arrow(frame: &mut GrayImage, left: i32, boundary: i32) {
    if frame.width == 0 || frame.height == 0 || left >= frame.width as i32 {
        return;
    }

    // The arrow points from the left edge to the boundary in both directions.
    let half_height = 8;
    for offset in -half_height..=half_height {
        let y = boundary + offset;
        if !(0..frame.height as i32).contains(&y) {
            continue;
        }
        let extent = 3 + (half_height - i32::abs(offset)) * 10 / half_height;
        let first = (left + 3).clamp(0, frame.width as i32 - 1) as usize;
        let last = (left + extent).clamp(first as i32, frame.width as i32 - 1) as usize;
        let row = y as usize * frame.width as usize;
        frame.pixels[row + first..=row + last].fill(64);
    }
}
