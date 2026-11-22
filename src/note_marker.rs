//! Note indicators share the document's projected highlight geometry.
use crate::{
    highlight_style::{Appearance, NoteMarker, Paint},
    selection::HighlightRect,
};
use primitive_ui::GrayImage;

const GAP: i32 = 11;
const SYMBOL: usize = 32;

pub(crate) fn bounds(
    rect: HighlightRect,
    marker: NoteMarker,
    margin: i32,
    first: bool,
    width: u32,
) -> Option<HighlightRect> {
    let stroke = 3;
    let (x, y, w, h) = match marker {
        NoteMarker::None => return None,
        NoteMarker::Underline => (
            rect.x,
            rect.y + rect.height - 1,
            rect.width,
            width.div_ceil(600).max(1) as i32,
        ),
        NoteMarker::Sideline => (margin, rect.y, stroke, rect.height),
        NoteMarker::Sidemark if first => (margin, rect.y, SYMBOL as i32, SYMBOL as i32),
        NoteMarker::Sidemark => return None,
    };
    Some(HighlightRect {
        x,
        y,
        width: w,
        height: h,
    })
}

pub(crate) fn margin(
    width: u32,
    right_margin: u32,
    leaf_x: u32,
    marker: NoteMarker,
    fixed: bool,
) -> i32 {
    let mark_width = if marker == NoteMarker::Sidemark {
        SYMBOL as i32
    } else {
        3
    };
    if fixed {
        width as i32 - GAP - mark_width
    } else {
        (leaf_x + width - right_margin) as i32 + GAP
    }
}

pub(crate) fn hit(
    rect: HighlightRect,
    marker: NoteMarker,
    margin: i32,
    first: bool,
    width: u32,
    x: f32,
    y: f32,
) -> bool {
    if !matches!(marker, NoteMarker::Sideline | NoteMarker::Sidemark) {
        return false;
    }
    bounds(rect, marker, margin, first, width).is_some_and(|r| {
        let w = r.width.max(100);
        let h = r.height.max(100);
        primitive_ui::Rect::new(r.x - (w - r.width) / 2, r.y - (h - r.height) / 2, w, h)
            .contains(x, y)
    })
}

pub(crate) fn paint(
    page: &mut GrayImage,
    rect: HighlightRect,
    ink: Paint,
    marker: NoteMarker,
    margin: i32,
    appearance: Appearance,
) {
    let original_height = rect.height;
    let mut rect = rect;
    if let Some(band) = ink.fill_band(rect.y, rect.height, appearance.line_height) {
        rect.y = band.start;
        if marker == NoteMarker::Underline {
            rect.height = band.end - band.start;
        }
    }
    if marker == NoteMarker::Sideline {
        rect.height = original_height;
    }
    let Some((color, first, night)) = ink.note() else {
        return;
    };
    let Some(mark) = bounds(rect, marker, margin, first, page.width) else {
        return;
    };
    let tone = if marker == NoteMarker::Sideline {
        (if night {
            color.night_gray()
        } else {
            color.gray()
        })
        .unwrap_or(0)
    } else {
        0
    };
    let mask = (marker == NoteMarker::Sidemark).then(pencil);
    for y in mark.y.max(0)..(mark.y + mark.height).min(page.height as i32) {
        for x in mark.x.max(0)..(mark.x + mark.width).min(page.width as i32) {
            let target = &mut page.pixels[y as usize * page.width as usize + x as usize];
            if let Some(mask) = mask {
                let alpha = mask[(y - mark.y) as usize * SYMBOL + (x - mark.x) as usize];
                *target = ((u16::from(*target) * (255 - u16::from(alpha)) + 127) / 255) as u8;
            } else {
                *target = tone;
            }
        }
    }
}

fn pencil() -> &'static Vec<u8> {
    static MASK: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
    MASK.get_or_init(|| {
        use kurbo::{Affine, BezPath, Join, Stroke, StrokeOpts};
        let icon = crate::app::NOTE_MARK_ICON;
        let mut paths = BezPath::new();
        for source in icon.stroke {
            if let Ok(path) = BezPath::from_svg(source) {
                paths.extend(kurbo::stroke(
                    path.elements().iter().copied(),
                    &Stroke::new(2.0).with_join(Join::Miter),
                    &StrokeOpts::default(),
                    0.05,
                ));
            }
        }
        let paths = Affine::scale(SYMBOL as f64 / 24.0) * paths;
        crate::gray_glyph::curve_mask(paths.elements(), 0.0, 0.0, SYMBOL, SYMBOL)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::highlight_style::{HighlightColor, HighlightStyle};
    fn paper() -> GrayImage {
        GrayImage {
            width: 120,
            height: 100,
            pixels: vec![255; 12000],
        }
    }
    fn saved(first: bool) -> Paint {
        Paint::Annotation {
            style: HighlightStyle::Lighten,
            color: HighlightColor::Blue,
            note: true,
            first,
            night: false,
        }
    }
    #[test]
    fn note_indicators_use_the_saved_line_projection_and_one_first_line_symbol() {
        let rect = HighlightRect {
            x: 12,
            y: 15,
            width: 50,
            height: 13,
        };
        let appearance = Appearance {
            line_height: 40,
            ..Default::default()
        };
        let mut page = paper();
        paint(
            &mut page,
            rect,
            saved(true),
            NoteMarker::None,
            100,
            appearance,
        );
        assert!(page.pixels.iter().all(|&p| p == 255));
        paint(
            &mut page,
            rect,
            saved(true),
            NoteMarker::Sideline,
            100,
            appearance,
        );
        // Stock centers the five-pixel filled band, then starts the original
        // thirteen-pixel side stroke from the resulting top.
        for y in 0..100 {
            for x in 0..120 {
                assert_eq!(
                    page.pixels[y * 120 + x],
                    if (19..32).contains(&y) && (100..103).contains(&x) {
                        88
                    } else {
                        255
                    }
                );
            }
        }
        let mut page = paper();
        paint(
            &mut page,
            rect,
            saved(true),
            NoteMarker::Underline,
            100,
            appearance,
        );
        assert!(
            page.pixels[23 * 120 + 12..23 * 120 + 62]
                .iter()
                .all(|&p| p == 0)
        );
        assert!(
            page.pixels[22 * 120 + 12..22 * 120 + 62]
                .iter()
                .all(|&p| p == 255)
        );
        let mut page = paper();
        paint(
            &mut page,
            rect,
            saved(true),
            NoteMarker::Sidemark,
            80,
            appearance,
        );
        assert!(page.pixels.iter().any(|&p| p < 255));
        let prior = page.pixels.clone();
        paint(
            &mut page,
            HighlightRect { y: 60, ..rect },
            saved(false),
            NoteMarker::Sidemark,
            80,
            appearance,
        );
        assert_eq!(page.pixels, prior);
    }
    #[test]
    fn reflow_leaves_keep_separate_margin_origins_and_fixed_pages_use_the_screen_edge() {
        assert_eq!(margin(616, 54, 0, NoteMarker::Sideline, false), 573);
        assert_eq!(margin(616, 54, 648, NoteMarker::Sideline, false), 1221);
        assert_eq!(margin(1264, 0, 0, NoteMarker::Sidemark, true), 1221);
    }
}
