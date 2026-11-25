//! Page overlap for single zoomed fixed pages.
use primitive_ui::Rect;

use super::{FixedReading, GrayImage, Result, View, view_metrics};
use crate::PageOverlapStyle;
use crate::session::page_overlap::Marker;

impl FixedReading {
    pub(crate) fn allows_page_overlap(&self) -> bool {
        self.supports_tile_zoom()
            && !self.is_continuous()
            && !self.is_spread()
            && !self.view.height_fit
            && (self.view.width_fit || self.view.tile_zoom.is_some() || self.view.zoom > 0)
    }

    pub(crate) fn set_page_overlap(
        &mut self,
        enabled: bool,
        style: PageOverlapStyle,
    ) -> Result<()> {
        if !self.allows_page_overlap() {
            return Err("Page overlap is unavailable in this view".into());
        }
        self.apply_page_overlap(enabled, style);
        Ok(())
    }

    pub(crate) fn apply_page_overlap(&mut self, enabled: bool, style: PageOverlapStyle) {
        self.view.page_overlap_enabled = enabled;
        self.view.page_overlap_style = (style != PageOverlapStyle::default()).then_some(style);
        if !enabled {
            self.overlap_cue = None;
        }
    }

    pub(super) fn present_page_turn(&mut self, page: usize, view: View) -> Result<()> {
        let previous =
            (page == self.page && self.view.page_overlap_enabled && self.allows_page_overlap())
                .then_some(self.view);
        self.present(page, view)?;
        if let Some(previous) = previous {
            self.overlap_cue = self
                .visible
                .iter()
                .find(|visible| visible.index == page)
                .and_then(|visible| {
                    let metrics = view_metrics(visible.size, &self.view, self.frame_size());
                    let visible_width = metrics.width.min(self.frame.width as f32).round() as i32;
                    let visible_height =
                        metrics.height.min(self.frame.height as f32).round() as i32;
                    let dx = (self.view.offset_x - previous.offset_x).round() as i32;
                    let dy = (self.view.offset_y - previous.offset_y).round() as i32;
                    let clip = Rect::new(
                        (self.frame.width as i32 - visible_width) / 2,
                        (self.frame.height as i32 - visible_height) / 2,
                        visible_width,
                        visible_height,
                    );
                    Marker::from_pan(clip, dx, dy)
                });
        }
        Ok(())
    }

    pub(super) fn paint_page_overlap(&self, frame: &mut GrayImage) {
        if let Some(cue) = self.overlap_cue {
            cue.paint(frame, self.view.page_overlap_style.unwrap_or_default());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{
        FixedDocument, FixedReading, PAGE_HEIGHT, Pdf, View, WIDTH, tests::striped_pdf_sized_bytes,
    };
    use crate::PageOverlapStyle;
    use primitive_ui::Input;
    use std::time::Instant;

    fn tall_pdf() -> FixedReading {
        let pdf = Pdf::open(striped_pdf_sized_bytes(2, 240, 690)).unwrap();
        FixedReading::with_source(FixedDocument::Pdf(pdf.into()), 0, View::default()).unwrap()
    }

    fn cue(reading: &FixedReading) -> super::GrayImage {
        let mut image = reading.frame.clone();
        reading.paint_page_overlap(&mut image);
        image
    }

    #[test]
    fn fixed_page_overlap_uses_the_actual_same_page_intersection() {
        let mut reading = tall_pdf();
        assert!(!reading.allows_page_overlap());
        reading.fit(true).unwrap();
        assert!(reading.allows_page_overlap());
        reading
            .set_page_overlap(true, PageOverlapStyle::DashedLine)
            .unwrap();
        assert_eq!(cue(&reading).pixels, reading.frame.pixels);
        let old = reading.view.offset_y;
        reading.turns.next_refresh = Instant::now();
        assert!(reading.input(Input::PageForward).unwrap());
        assert_eq!(reading.page, 0);
        let displacement = (old - reading.view.offset_y).round() as usize;
        let boundary = PAGE_HEIGHT as usize - displacement;
        assert!(displacement > 0 && boundary > 0);
        let row = boundary * WIDTH as usize;
        let overlaid = cue(&reading);
        assert_eq!(&overlaid.pixels[..row], &reading.frame.pixels[..row]);
        assert_eq!(overlaid.pixels[row + 4], 136);
        assert_eq!(overlaid.pixels[row + 17], reading.frame.pixels[row + 17]);
        assert_eq!(overlaid.pixels[row + 24], 136);
        assert_eq!(
            &overlaid.pixels[row + 3 * WIDTH as usize..],
            &reading.frame.pixels[row + 3 * WIDTH as usize..]
        );

        // Change only the indicator while reading the same source rectangle.
        let current = reading.history_point().unwrap();
        reading
            .set_page_overlap(true, PageOverlapStyle::Dim)
            .unwrap();
        assert_eq!(reading.history_point().unwrap(), current);
        let dim = cue(&reading);
        assert_ne!(&dim.pixels[..row], &reading.frame.pixels[..row]);
        assert_eq!(&dim.pixels[row..], &reading.frame.pixels[row..]);
        reading
            .set_page_overlap(true, PageOverlapStyle::None)
            .unwrap();
        assert_eq!(cue(&reading).pixels, reading.frame.pixels);

        // A backward automatic step moves the repeated region to the bottom.
        reading
            .set_page_overlap(true, PageOverlapStyle::Line)
            .unwrap();
        let old = reading.view.offset_y;
        assert!(reading.automatic_turn(-5).unwrap());
        let boundary = (reading.view.offset_y - old).round() as usize;
        assert!(boundary > 0);
        let row = boundary * WIDTH as usize;
        let back = cue(&reading);
        assert_eq!(&back.pixels[..row], &reading.frame.pixels[..row]);
        assert!(
            back.pixels[row..row + WIDTH as usize]
                .iter()
                .all(|&gray| gray == 136)
        );
        reading.jump(1).unwrap();
        assert!(reading.overlap_cue.is_none());
    }

    #[test]
    fn fixed_page_overlap_clears_at_a_page_boundary_and_does_not_mark_free_pans() {
        let mut reading = tall_pdf();
        reading.fit(true).unwrap();
        reading
            .set_page_overlap(true, PageOverlapStyle::Arrow)
            .unwrap();
        reading.turns.next_refresh = Instant::now();
        assert!(reading.input(Input::PageForward).unwrap());
        assert!(reading.overlap_cue.is_some());
        assert!(
            reading
                .input(Input::Swipe {
                    dx: 0.,
                    dy: -50.,
                    start_y: 600.
                })
                .unwrap()
        );
        assert!(reading.overlap_cue.is_none());
        for _ in 0..8 {
            reading.turns.next_refresh = Instant::now();
            assert!(reading.input(Input::PageForward).unwrap());
            if reading.page == 1 {
                break;
            }
        }
        assert_eq!(reading.page, 1);
        assert_eq!(cue(&reading).pixels, reading.frame.pixels);

        reading.fit_height().unwrap();
        assert!(!reading.allows_page_overlap());
        reading.fit(true).unwrap();
        reading.set_continuous(true).unwrap();
        assert!(!reading.allows_page_overlap());
        assert!(reading.preferences().page_overlap_enabled);
    }
}
