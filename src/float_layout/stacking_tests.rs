use super::tests::{marker, open, settings};
use super::*;
use crate::{Probe, Renderer};
use std::path::PathBuf;

const BOOK: &str = "render-floats.epub";
const WEB: FloatMode = FloatMode::PublisherHeight;
const CONTENT: FloatMode = FloatMode::ContentHeight;

fn layout<'a>(
    probe: &'a Probe,
    phrase: &str,
) -> &'a parley::Layout<crate::parley_layout::PaintBrush> {
    let Renderer::Parley(renderer) = &probe.renderer else {
        panic!("Parley required")
    };
    let (anchor, _) = marker(probe, phrase);
    let page = probe.page_for(&anchor).unwrap();
    &renderer
        .paragraphs_in(probe.page_range(page))
        .find(|paragraph| paragraph.key == anchor.block)
        .unwrap()
        .layout
}

fn capture(probe: &mut Probe, mode: FloatMode, case: &str) -> Vec<u8> {
    let pixels = probe.capture(0).unwrap();
    if let Some(path) = std::env::var_os("READER_FLOAT_STACKING_CAPTURES") {
        let mode = if mode == WEB { "web" } else { "book" };
        let file =
            PathBuf::from(path).join(format!("render-float-stacking-reader-{mode}-{case}.png"));
        let settings = settings(WEB);
        image::GrayImage::from_raw(settings.width, settings.height, pixels.clone())
            .unwrap()
            .save(file)
            .unwrap();
    }
    pixels
}

#[test]
fn stacked_floats_share_their_physical_edges_and_keep_distinct_source_targets() {
    for mode in [CONTENT, WEB] {
        let mut probe = open(BOOK, mode, 6);
        let mut ordinary = open(BOOK, FloatMode::Ignore, 6);
        for (phrase, x) in [
            ("Long edge", 54),
            ("Inner edge", 254),
            ("Opposite bank", 970),
            ("Upper shelf", 554),
        ] {
            let (anchor, target) = marker(&probe, phrase);
            assert_eq!((target.x, target.y), (x, 54), "{mode:?} {phrase}");
            assert_eq!(anchor, marker(&ordinary, phrase).0);
            let x = (target.x + 4) as f32;
            let y = (target.y + target.height / 2) as f32;
            assert_eq!(probe.word_at(0, x, y).unwrap().start, anchor, "{phrase}");
        }
        let upper = marker(&probe, "Upper shelf").1;
        let y = (upper.y + upper.height / 2) as f32;
        let link = probe.link_at(0, (upper.x + 10) as f32, y).unwrap();
        assert_eq!(link.fragment.as_deref(), Some("shelf-end"));
        let target = probe.fragment("shelf-end").unwrap();
        assert_eq!(target, marker(&probe, "Open bank").0);

        if mode == WEB {
            let (middle, lower) = (
                marker(&probe, "Middle shelf").1,
                marker(&probe, "Lower shelf").1,
            );
            assert_eq!((middle.x, lower.x), (554, 254));
            let cleared = marker(&probe, "Open bank").1;
            assert_eq!(cleared.x, 54);
            assert_eq!(
                layout(&probe, "Open bank")
                    .get(0)
                    .unwrap()
                    .metrics()
                    .inline_max_coord,
                916.0
            );
        }
        let frame = capture(&mut probe, mode, "shelf");
        assert_ne!(frame, ordinary.capture(0).unwrap());
        let pixel = |x, y| frame[y * settings(WEB).width as usize + x];
        assert_eq!(
            (pixel(236, 60), pixel(532, 60), pixel(1200, 60)),
            (204, 187, 221)
        );
        let mut reopened = open(BOOK, mode, 6);
        assert_eq!(reopened.capture(0).unwrap(), frame);
        assert_eq!(
            marker(&reopened, "Upper shelf"),
            marker(&probe, "Upper shelf")
        );

        let right = open(BOOK, mode, 7);
        assert_eq!(
            (
                marker(&right, "Outer quay").1.x,
                marker(&right, "Inner quay").1.x
            ),
            (970, 670)
        );
        assert_eq!(
            layout(&right, "Below the crane")
                .get(0)
                .unwrap()
                .metrics()
                .inline_max_coord,
            616.0
        );
        assert_eq!(
            layout(&right, "Clear quay")
                .get(0)
                .unwrap()
                .metrics()
                .inline_max_coord,
            1156.0
        );
    }
}

#[test]
fn dropped_floats_keep_the_text_above_the_first_available_gap() {
    for (mode, inner_right) in [(CONTENT, 1050), (WEB, 810)] {
        let mut probe = open(BOOK, mode, 8);
        let outer = marker(&probe, "Outer card").1;
        let lower = marker(&probe, "Lower card").1;
        let small = marker(&probe, "Small card").1;
        let water = marker(&probe, "Water edge").1;
        assert_eq!(
            (outer.x, lower.x, small.x, water.x),
            (54, 54, inner_right, 734),
            "{mode:?}"
        );
        assert_eq!((outer.y, water.y), (54, 54));
        assert_eq!(lower.y, small.y);
        if mode == WEB {
            assert_eq!(lower.y, 150);
        } else {
            assert!(lower.y > outer.y);
        }
        assert_eq!(
            layout(&probe, "Water edge")
                .get(0)
                .unwrap()
                .metrics()
                .inline_max_coord,
            916.0
        );
        capture(&mut probe, mode, "dropped");

        let probe = open(BOOK, mode, 9);
        let first = marker(&probe, "First beam").1;
        let next = marker(&probe, "Next beam").1;
        let beside = marker(&probe, "Along beam").1;
        assert_eq!(first.y, beside.y);
        assert!(next.y > first.y);
        if mode == WEB {
            assert_eq!((next.y, beside.x), (214, 254));
        }
    }
}

#[test]
fn stacked_floats_merge_only_the_new_paragraphs_with_six_active_rectangles() {
    let mut probe = open(BOOK, WEB, 10);
    for (phrase, x) in [
        ("Far step", 804),
        ("High ledge", 954),
        ("Mid ledge", 954),
        ("Low ledge", 804),
        ("Clear ledge", 54),
    ] {
        assert_eq!(marker(&probe, phrase).1.x, x, "{phrase}");
    }
    capture(&mut probe, WEB, "dense");

    let mut flow = Flow::default();
    let spec = Spec {
        owner: 0,
        side: Side::Left,
        width: 150,
        publisher_height: None,
        gray: None,
        scope_end: 10,
    };
    for index in 0..6 {
        let height = if index == 5 { 80.0 } else { 160.0 };
        flow.record(spec, index * spec.width, 0.0, height).unwrap();
    }
    // A single paragraph keeps the conservative side rectangle selected at its
    // beginning. Only a later paragraph can rebuild the five exact rectangles.
    let six = flow.contour(0.0, 1156, 160.0).unwrap();
    assert_eq!(six.for_line(95.0, 48.0), (95.0, 900.0, 256.0));
    let five = flow.contour(95.0, 1156, 160.0).unwrap();
    assert_eq!(five.for_line(0.0, 48.0), (0.0, 750.0, 406.0));
}

#[test]
fn stacked_float_limit_retries_normal_flow_and_keeps_the_later_source() {
    let mut probe = open(BOOK, WEB, 11);
    let mut ordinary = open(BOOK, FloatMode::Ignore, 11);
    let Renderer::Parley(renderer) = &probe.renderer else {
        panic!("Parley required");
    };
    assert!(!renderer.uses_float_geometry());
    assert!(
        renderer
            .limitations()
            .iter()
            .any(|message| message.contains("placement limit"))
    );
    assert_eq!(
        marker(&ordinary, "Anchor beyond the float budget"),
        marker(&probe, "Anchor beyond the float budget")
    );
    assert_eq!(probe.capture(0).unwrap(), ordinary.capture(0).unwrap());
}
