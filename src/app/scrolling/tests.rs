use super::*;
use crate::app::selection_viewport_tests::{adopt, capture, key, tap};
use crate::app::tests::{Fixture, tap_contents_item};
use crate::session::ReadingViewport;
use primitive_app::Lifecycle;
use primitive_ui::Key;
use std::fs;

fn app(fixture: &Fixture) -> Reader {
    Reader::open(
        &fixture.root,
        &fixture.state,
        vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")],
    )
    .unwrap()
}

fn book(fixture: &Fixture) -> Reader {
    let path = fixture.root.join("Coastal dispatches.epub");
    fs::copy(crate::test_fixture("long-chapter.epub"), &path).unwrap();
    let mut reader = app(fixture);
    reader.open_path(&path).unwrap();
    reader
}

fn movement(start: (f32, f32), displacement: f32, ms: u64, phase: PanPhase) -> TouchPan {
    TouchPan {
        start,
        end: (start.0, start.1 + displacement),
        elapsed: Duration::from_millis(ms),
        phase,
    }
}

fn scroll_top(reader: &Reader) -> f32 {
    reader
        .active
        .as_ref()
        .unwrap()
        .reading
        .scroll_history_top()
        .unwrap()
}

#[test]
fn scrolling_methods_use_incremental_accumulated_and_release_motion_in_epub() {
    for (method, expected) in [
        (Method::Classic, [80.0, 220.0, 280.0]),
        (Method::Turbo, [80.0, 300.0, 300.0]),
        (Method::OnRelease, [0.0, 0.0, 280.0]),
    ] {
        let fixture = Fixture::new();
        let mut reader = book(&fixture);
        assert!(reader.live_touch_pan());
        assert!(matches!(
            reader.scroll_settings_item().action,
            Action::Heading
        ));
        reader.activate(Action::ReadingMode(true)).unwrap();
        reader.controls.scroll_method = method;
        reader.controls.menu_activation = MenuActivation::Tap;
        assert_eq!(reader.scroll_delay_ms(), 0);
        let original_position = reader.current_history_location();
        let original_saved = reader
            .library
            .state(reader.open_book_id().unwrap())
            .unwrap()
            .position
            .clone();
        let mut life = Lifecycle::default();
        let fail = |error| panic!("unexpected save error: {error}");
        let middle = (620.0, 750.0);
        for (i, (dy, ms, phase)) in [
            (-80.0, 0, PanPhase::Move),
            (-220.0, 220, PanPhase::Move),
            (-280.0, 440, PanPhase::Release),
        ]
        .into_iter()
        .enumerate()
        {
            let before = scroll_top(&reader);
            let change = life.pan(&mut reader, movement(middle, dy, ms, phase), fail);
            assert_eq!(scroll_top(&reader), expected[i], "method {method:?}");
            assert_eq!(change != Change::None, expected[i] != before);
            if phase == PanPhase::Move {
                assert_eq!(
                    reader
                        .library
                        .state(reader.open_book_id().unwrap())
                        .unwrap()
                        .position,
                    original_saved
                );
            }
        }
        assert!(!reader.scroll_pan.busy());
        assert_ne!(reader.current_history_location(), original_position);
        assert!(reader.back.is_empty());
        if method == Method::Classic {
            let mut portrait = adopt(&mut reader, ReadingViewport::PORTRAIT, 1);
            capture(&reader, &mut portrait, "classic-epub-scrolling.png", false);
        }
    }
}

#[test]
fn scrolling_delay_editor_keeps_draft_bounds_and_commits_from_continuous_view() {
    let fixture = Fixture::new();
    let mut reader = book(&fixture);
    assert_eq!(reader.scroll_delay_ms(), 500);
    let mut landscape = adopt(&mut reader, ReadingViewport::LANDSCAPE, 1);
    reader.activate(Action::ReadingMode(true)).unwrap();
    reader.show(Screen::Gestures);
    tap_contents_item(&mut reader, "Scrolling");
    assert_eq!(reader.screen, Screen::Scrolling);
    tap_contents_item(&mut reader, "On-release scrolling");
    assert_eq!(reader.controls.scroll_method, Method::OnRelease);
    capture(&reader, &mut landscape, "scrolling-controls.png", false);
    capture(&reader, &mut landscape, "scrolling-controls-dark.png", true);
    tap_contents_item(&mut reader, "Activation delay");
    assert_eq!(reader.editor.string(), "500");
    let [minus, default, plus] = reader.scroll_delay_buttons();
    tap(&mut reader, plus);
    assert_eq!(reader.editor.string(), "600");
    assert_eq!(
        reader.input(Input::LongTap(
            (plus.x + plus.w / 2) as f32,
            (plus.y + plus.h / 2) as f32,
        )),
        Change::Partial
    );
    assert_eq!(reader.editor.string(), "1100");
    tap(&mut reader, minus);
    assert_eq!(reader.editor.string(), "1000");
    tap(&mut reader, default);
    assert_eq!(reader.editor.string(), "500");
    assert_eq!(reader.input(Input::PageBack), Change::Partial);
    assert_eq!(reader.controls.scroll_delay_ms, None);
    tap_contents_item(&mut reader, "Activation delay");
    let set = key(&reader, Key::Go, "Set");
    reader.set_scroll_delay_draft(2001);
    tap(&mut reader, set);
    assert_eq!(
        reader.screen,
        Screen::NumberingValue(NumberField::ScrollDelay)
    );
    assert_eq!(reader.controls.scroll_delay_ms, None);
    reader.set_scroll_delay_draft(450);
    capture(&reader, &mut landscape, "scrolling-delay.png", false);
    tap(&mut reader, set);
    assert_eq!(reader.screen, Screen::Scrolling);
    let reopened = app(&fixture);
    assert_eq!(reopened.controls.scroll_method, Method::OnRelease);
    assert_eq!(reopened.scroll_delay_ms(), 450);
}

#[test]
fn scrolling_activation_and_competing_menu_swipe_preserve_the_original_epub_view() {
    let fixture = Fixture::new();
    let mut reader = book(&fixture);
    reader.activate(Action::ReadingMode(true)).unwrap();
    let origin = reader.current_history_location();
    let middle = (620.0, 750.0);
    assert_eq!(
        reader.touch_pan(movement(middle, -80.0, 0, PanPhase::Move)),
        Change::None
    );
    assert_eq!(
        reader.touch_pan(movement(middle, -130.0, 499, PanPhase::Move)),
        Change::None
    );
    assert_eq!(
        reader.touch_pan(movement(middle, -190.0, 499, PanPhase::Release)),
        Change::None
    );
    assert_eq!(scroll_top(&reader), 0.0);
    assert_eq!(reader.current_history_location(), origin);
    reader.controls.scroll_method = Method::OnRelease;
    assert_eq!(
        reader.touch_pan(movement(middle, -80.0, 0, PanPhase::Move)),
        Change::None
    );
    assert_eq!(
        reader.touch_pan(movement(middle, -80.0, 900, PanPhase::Release)),
        Change::None
    );
    assert_eq!(scroll_top(&reader), 0.0);
    assert_eq!(reader.current_history_location(), origin);
    reader.controls.scroll_method = Method::Classic;
    assert_eq!(
        reader.touch_pan(movement(middle, -80.0, 0, PanPhase::Move)),
        Change::None
    );
    assert_eq!(
        reader.touch_pan(movement(middle, -130.0, 500, PanPhase::Move)),
        Change::Partial
    );
    assert_eq!(scroll_top(&reader), 130.0);
    assert_eq!(
        reader.touch_pan(movement(middle, -140.0, 530, PanPhase::Cancel)),
        Change::Partial
    );
    assert_eq!(scroll_top(&reader), 0.0);

    reader.controls.scroll_delay_ms = Some(0);
    let edge = (620.0, 180.0);
    assert_eq!(
        reader.touch_pan(movement(edge, -130.0, 0, PanPhase::Move)),
        Change::Partial
    );
    assert_ne!(reader.current_history_location(), origin);
    assert_eq!(
        reader.touch_pan(movement(edge, 180.0, 800, PanPhase::Release)),
        Change::Partial
    );
    assert_eq!(reader.current_history_location(), origin);
    assert!(reader.reading_top);
    assert!(!reader.scroll_pan.busy());
}

#[test]
fn scrolling_on_release_pdf_retains_zoom_and_drops_an_interrupted_contact() {
    let fixture = Fixture::new();
    let path = fixture.root.join("Port survey.pdf");
    let mut page = String::from(
        "BT /F1 28 Tf 56 680 Td (Harbor survey) Tj ET\n\
         BT /F1 16 Tf 56 640 Td (Coastal observations) Tj ET\n",
    );
    for (index, note) in [
        "Read the tide gauge at the southern pier.",
        "Mark the channel entrance on the chart.",
        "Check the two lights before entering.",
        "The lee of the breakwater is sheltered.",
        "Record changes after the next tide.",
        "Close the survey at the north pier.",
    ]
    .into_iter()
    .enumerate()
    {
        let y = 556 - index * 68;
        page.push_str(&format!("BT /F1 13 Tf 56 {y} Td ({note}) Tj ET\n"));
    }
    let source = crate::fixed::tests::pdf_objects(&[
        "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
        "<< /Type /Pages /Count 2 /Kids [3 0 R 4 0 R] /Resources << /Font << /F1 6 0 R >> >> >>"
            .to_owned(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 450 710] /Contents 5 0 R >>".to_owned(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 450 710] /Contents 5 0 R >>".to_owned(),
        format!("<< /Length {} >>\nstream\n{page}endstream", page.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_owned(),
    ]);
    fs::write(&path, &source).unwrap();
    let mut reader = app(&fixture);
    reader.open_path(&path).unwrap();
    reader.activate(Action::FixedScroll(true)).unwrap();
    reader.controls.scroll_method = Method::OnRelease;
    reader.controls.menu_activation = MenuActivation::Tap;
    let start = (625.0, 720.0);
    let initial = reader.fixed.as_ref().unwrap().reading.preferences();
    let mut life = Lifecycle::default();
    let fail = |error| panic!("unexpected save error: {error}");
    assert_eq!(
        life.pan(
            &mut reader,
            movement(start, -100.0, 0, PanPhase::Move),
            fail
        ),
        Change::None
    );
    assert_eq!(
        reader.fixed.as_ref().unwrap().reading.preferences(),
        initial
    );
    assert_eq!(
        life.pan(
            &mut reader,
            movement(start, -210.0, 210, PanPhase::Release),
            fail,
        ),
        Change::Partial
    );
    let moved = reader.fixed.as_ref().unwrap().reading.preferences();
    assert_eq!(moved.offset_y, initial.offset_y - 210.0);
    assert_eq!(moved.zoom, initial.zoom);
    assert_eq!(moved.width_fit, initial.width_fit);
    let mut portrait = adopt(&mut reader, ReadingViewport::PORTRAIT, 1);
    capture(
        &reader,
        &mut portrait,
        "on-release-pdf-scrolling.png",
        false,
    );

    let viewport_view = reader.fixed.as_ref().unwrap().reading.preferences();
    life.pan(&mut reader, movement(start, -95.0, 0, PanPhase::Move), fail);
    life.input(&mut reader, Input::Sleep, fail);
    life.input(&mut reader, Input::Wake, fail);
    assert_eq!(
        life.pan(
            &mut reader,
            movement(start, -180.0, 700, PanPhase::Release),
            fail,
        ),
        Change::None
    );
    assert_eq!(
        reader.fixed.as_ref().unwrap().reading.preferences(),
        viewport_view
    );
    assert_eq!(fs::read(path).unwrap(), source);
}
