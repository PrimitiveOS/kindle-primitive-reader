//! Fit bookmark text into a fixed row using the same wrapped lines as Canvas.
use std::borrow::Cow;

use primitive_ui::{Canvas, Rect, TextStyle};

use super::super::list_density::font_pixels;

pub(super) struct TitleFit {
    pub(super) logical_font: u8,
    pixels: i32,
    line_height: i32,
    lines: usize,
    visible_end: usize,
}

impl TitleFit {
    pub(super) fn measure(canvas: &mut Canvas, title: &str, area: Rect, logical_font: u8) -> Self {
        let pixels = font_pixels(logical_font);
        let line_height = (pixels * 13 + 5) / 10;
        let capacity = (area.h / line_height).max(1) as usize;
        let ranges = canvas.text_pages(area, title, title_style(pixels));
        let lines = ranges.len().min(capacity).max(1);
        let visible_end = ranges.get(lines - 1).map_or(title.len(), |range| range.end);
        Self {
            logical_font,
            pixels,
            line_height,
            lines,
            visible_end,
        }
    }

    pub(super) fn complete(&self, title: &str) -> bool {
        self.visible_end == title.len()
    }

    pub(super) fn shrink(self, canvas: &mut Canvas, title: &str, area: Rect) -> Self {
        // KOReader MenuItem keeps the largest size that fits and uses the
        // smallest allowed size with an ellipsis when the entry still overflows.
        let floor = self
            .logical_font
            .min(self.logical_font.saturating_sub(8).max(12));
        if self.complete(title) || self.logical_font == floor {
            return self;
        }

        let mut largest_fit = Self::measure(canvas, title, area, floor);
        if !largest_fit.complete(title) {
            return largest_fit;
        }
        let mut first_excluded = self.logical_font;
        while largest_fit.logical_font + 1 < first_excluded {
            let size = largest_fit.logical_font + (first_excluded - largest_fit.logical_font) / 2;
            let candidate = Self::measure(canvas, title, area, size);
            if candidate.complete(title) {
                largest_fit = candidate;
            } else {
                first_excluded = size;
            }
        }
        largest_fit
    }

    pub(super) fn height(&self) -> i32 {
        self.line_height * self.lines as i32
    }

    pub(super) fn displayed<'a>(&self, title: &'a str) -> Cow<'a, str> {
        if self.complete(title) {
            Cow::Borrowed(title)
        } else {
            Cow::Owned(format!("{}…", title[..self.visible_end].trim_end()))
        }
    }

    pub(super) fn draw(&self, canvas: &mut Canvas, bounds: Rect, title: &str, ink: u8) {
        canvas.text(
            bounds,
            &self.displayed(title),
            title_style(self.pixels).lines(self.lines).ink(ink),
        );
    }
}

fn title_style(pixels: i32) -> TextStyle {
    TextStyle::new(pixels).bold().break_long_words()
}
