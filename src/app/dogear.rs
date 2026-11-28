//! KOReader's dog-ear: a corner mark on a bookmarked page (`frontend/apps/reader/modules/readerdogear.lua`,
//! painted by `readerview.lua` when `dogear_visible` is set). Reader paints a solid right-angled wedge into the
//! top-right corner of the page raster, so it is drawn with the page text and is not covered by the picture.
use primitive_ui::GrayImage;

/// KOReader sizes the dog-ear between 1/40 and 1/32 of the shorter screen side and uses the largest size
/// that fits the top and right margins. Reader uses the largest size: its default margins are wider than it.
fn size(width: u32, height: u32) -> u32 {
    width.min(height) / 32
}

/// Paint the wedge into `page`: row `y` of the corner box is black from `side - y` columns before the right edge
/// to the edge, so the box's top edge is solid and the wedge tapers to one pixel on its last row.
pub(crate) fn paint_wedge(page: &mut GrayImage) {
    let side = size(page.width, page.height).min(page.height);
    let width = page.width as usize;
    for y in 0..side as usize {
        let start = width - side as usize + y;
        page.pixels[y * width + start..(y + 1) * width].fill(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(width: u32, height: u32) -> GrayImage {
        GrayImage {
            width,
            height,
            pixels: vec![255; (width * height) as usize],
        }
    }

    #[test]
    fn size_follows_the_shorter_side() {
        assert_eq!(size(1264, 1680), 39);
        assert_eq!(size(1680, 1264), 39);
        assert_eq!(size(600, 900), 18);
    }

    #[test]
    fn wedge_covers_the_top_right_corner_and_tapers_to_one_pixel() {
        let mut image = page(1264, 1680);
        paint_wedge(&mut image);
        let side = size(1264, 1680) as usize;
        let at = |x: usize, y: usize| image.pixels[y * 1264 + x];
        // The top row is black across the whole box; the bottom row only at the right edge.
        assert!((1264 - side..1264).all(|x| at(x, 0) == 0));
        assert_eq!(at(1263, side - 1), 0);
        assert_eq!(at(1264 - side, side - 1), 255);
        // Outside the box nothing changes.
        assert_eq!(at(1264 - side - 1, 0), 255);
        assert_eq!(at(1263, side), 255);
    }

    #[test]
    fn bookmarked_page_shows_the_corner_mark_and_clears_when_unmarked() {
        use crate::app::Action;
        let fixture = crate::app::tests::Fixture::new();
        let mut app = fixture.app();
        app.open_path(&fixture.book).unwrap();
        let fonts = [crate::test_fixture("fonts/NotoSans-Regular.ttf")];
        let font_paths: Vec<_> = fonts.iter().map(|path| path.display().to_string()).collect();
        let mut canvas =
            primitive_ui::Canvas::new(primitive_app::WIDTH, primitive_app::HEIGHT, &font_paths).unwrap();
        // Two pixels inside the wedge's top-right corner.
        let corner = (primitive_app::WIDTH as usize - 2) + 2 * primitive_app::WIDTH as usize;
        assert!(primitive_app::frame(&app, &mut canvas, false)[corner] > 200);
        app.activate(Action::ToggleBookmark).unwrap();
        assert!(primitive_app::frame(&app, &mut canvas, false)[corner] < 50);
        app.activate(Action::ToggleBookmark).unwrap();
        assert!(primitive_app::frame(&app, &mut canvas, false)[corner] > 200);
    }

    #[test]
    fn wedge_is_clamped_to_a_tiny_page() {
        let mut image = page(10, 4);
        paint_wedge(&mut image);
        assert_eq!(image.pixels.iter().filter(|&&pixel| pixel == 0).count(), 0);
    }
}
