use super::*;
use crate::app::{
    MenuActivation,
    selection_viewport_tests::{adopt, capture, tap},
    tests::{Fixture, tap_contents_item},
};
use crate::session::ReadingViewport;
use primitive_app::Lifecycle;
use primitive_ui::Input;
use std::fs;

fn canvas_reader(fixture: &Fixture) -> Reader {
    Reader::open(
        &fixture.root,
        &fixture.state,
        vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")],
    )
    .unwrap()
}

fn pan(start: (f32, f32), end: (f32, f32), elapsed: u64, phase: PanPhase) -> TouchPan {
    TouchPan {
        start,
        end,
        elapsed: Duration::from_millis(elapsed),
        phase,
    }
}

const START: (f32, f32) = (550.0, 700.0);
const UP: (f32, f32) = (550.0, 540.0);
const ELBOW: (f32, f32) = (710.0, 540.0);
const CUSTOM: Shape = Shape::new(&[E, S, W, N, E]);
const CUSTOM_POINTS: [(f32, f32); 5] = [
    (700.0, 700.0),
    (700.0, 860.0),
    (550.0, 860.0),
    (550.0, 700.0),
    (710.0, 700.0),
];

fn touch(reader: &mut Reader, positions: &[(f32, f32)], start: (f32, f32)) -> Change {
    let mut life = Lifecycle::default();
    let fail = |error| panic!("unexpected save failure: {error}");
    for (index, &end) in positions.iter().enumerate() {
        life.pan(
            reader,
            pan(start, end, index as u64 * 200, PanPhase::Move),
            fail,
        );
    }
    life.pan(
        reader,
        pan(
            start,
            *positions.last().unwrap(),
            positions.len() as u64 * 160 + 60,
            PanPhase::Release,
        ),
        fail,
    )
}

#[test]
fn multiswipe_can_open_a_saved_book_shortcut() {
    use crate::app::book_shortcuts::{Command as ShortcutCommand, Kind};

    let fixture = Fixture::new();
    let name = "Harbor notes.epub";
    fs::copy(
        crate::test_fixture("night-landing.epub"),
        fixture.root.join(name),
    )
    .unwrap();
    let mut reader = canvas_reader(&fixture);
    reader.open_path(&fixture.book).unwrap();
    reader
        .activate(Action::BookShortcut(ShortcutCommand::Add(
            Kind::Book,
            name.into(),
        )))
        .unwrap();
    reader.show(Screen::Gestures);
    tap_contents_item(&mut reader, "Multiswipes");
    tap_contents_item(&mut reader, "Preset multiswipes");
    tap_contents_item(&mut reader, "↑ →");
    tap_contents_item(&mut reader, "Open Harbor notes.epub");
    reader.show(Screen::Book);
    assert_eq!(touch(&mut reader, &[UP, ELBOW], START), Change::Partial);
    assert_eq!(
        reader.current_document().unwrap().0.library_path(),
        Path::new(name)
    );
}

#[test]
fn groups_in_contact_directions_at_stock_threshold_and_release_window() {
    let fixture = Fixture::new();
    let mut shape = Multiswipes::load(&fixture.state);
    let mut replay = |positions: &[(f32, f32)], release| {
        for (index, &end) in positions.iter().enumerate() {
            assert_eq!(
                shape.observe(pan(START, end, index as u64 * 200, PanPhase::Move)),
                None
            );
        }
        shape.observe(pan(
            START,
            *positions.last().unwrap(),
            release,
            PanPhase::Release,
        ))
    };
    assert_eq!(
        replay(&[UP, (550.0, 425.0), (640.0, 425.0), (670.0, 425.0)], 899),
        Some(Shape::new(&[N, E]))
    );
    assert_eq!(replay(&[UP, ELBOW], 900), None);
    assert_eq!(replay(&[(644.0, 700.0), (644.0, 794.0)], 300), None);
    assert_eq!(
        replay(&[(645.0, 700.0), (645.0, 795.0)], 300),
        Some(Shape::new(&[E, S]))
    );
    assert_eq!(
        replay(&[(700.0, 850.0), (850.0, 700.0)], 300),
        Some(Shape::new(&[SE, NE]))
    );
    assert_eq!(replay(&[UP, START], 330), None);
    assert_eq!(replay(&[(430.0, 700.0)], 250), None);
    assert_eq!(replay(&[(670.0, 700.0)], 250), None);

    assert_eq!(shape.observe(pan(START, UP, 0, PanPhase::Move)), None);
    assert_eq!(
        shape.observe(pan(START, ELBOW, 210, PanPhase::Release)),
        Some(Shape::new(&[N, E]))
    );
    assert_eq!(shape.observe(pan(START, UP, 0, PanPhase::Move)), None);
    assert_eq!(shape.observe(pan(START, UP, 120, PanPhase::Cancel)), None);
    assert_eq!(
        shape.observe(pan(START, ELBOW, 310, PanPhase::Release)),
        None
    );
}

#[test]
fn records_reassigns_and_removes_custom_shapes_through_visible_controls() {
    let fixture = Fixture::new();
    let mut reader = canvas_reader(&fixture);
    reader.open_path(&fixture.book).unwrap();
    let mut portrait = adopt(&mut reader, ReadingViewport::PORTRAIT, 1);
    reader.show(Screen::Gestures);
    assert!(matches!(reader.items()[1].action, Action::Heading));
    assert!(matches!(reader.items()[2].action, Action::Heading));
    tap_contents_item(&mut reader, "Multiswipes");
    capture(
        &reader,
        &mut portrait,
        "multiswipe-gesture-settings.png",
        false,
    );

    tap_contents_item(&mut reader, "Preset multiswipes");
    assert_eq!(reader.items().len(), 35);
    capture(&reader, &mut portrait, "multiswipe-presets.png", false);
    tap_contents_item(&mut reader, "↑ →");
    assert_eq!(reader.items()[0].detail.as_deref(), Some("Contents"));
    tap_contents_item(&mut reader, "Pass through");
    reader.show(Screen::Book);
    touch(&mut reader, &[UP, ELBOW], START);
    assert_eq!(reader.screen, Screen::Book);
    reader.show(Screen::Multiswipes);
    tap_contents_item(&mut reader, "↑ →");
    tap_contents_item(&mut reader, "Default behavior");
    reader.show(Screen::Book);
    assert_eq!(touch(&mut reader, &[UP, ELBOW], START), Change::Partial);
    assert_eq!(reader.screen, Screen::Contents);
    reader.show(Screen::Multiswipes);
    tap_contents_item(&mut reader, "↑ →");
    tap_contents_item(&mut reader, "Bookmarks");
    assert_eq!(
        reader.gestures.multiswipes.configured(Shape::new(&[N, E])),
        Some(Binding::Bookmarks)
    );
    let back = reader.navigation_layout().back();
    tap(&mut reader, back);
    tap_contents_item(&mut reader, "Custom multiswipes");
    tap_contents_item(&mut reader, "Record a multiswipe");
    assert!(reader.gestures.multiswipes.draft.is_none());
    assert_eq!(touch(&mut reader, &[UP, ELBOW], START), Change::Partial);
    assert!(
        reader.items()[0]
            .detail
            .as_deref()
            .unwrap()
            .starts_with("Already exists")
    );
    assert_eq!(reader.items().len(), 1);
    assert_eq!(touch(&mut reader, &CUSTOM_POINTS, START), Change::Partial);
    assert_eq!(reader.gestures.multiswipes.draft, Some(CUSTOM));
    capture(
        &reader,
        &mut portrait,
        "multiswipe-custom-recorded.png",
        false,
    );
    capture(
        &reader,
        &mut portrait,
        "multiswipe-custom-recorded-dark.png",
        true,
    );
    tap_contents_item(&mut reader, "Save multiswipe");
    assert_eq!(reader.screen, Screen::MultiswipeBinding(CUSTOM));
    tap_contents_item(&mut reader, "Find in book");
    assert_eq!(reader.screen, Screen::CustomMultiswipes);
    let back = reader.navigation_layout().back();
    tap(&mut reader, back);
    tap_contents_item(&mut reader, "Gesture overview");
    assert!(reader.items().into_iter().any(|row| {
        row.title == CUSTOM.arrows() && row.detail.as_deref() == Some("Find in book")
    }));
    capture(
        &reader,
        &mut portrait,
        "multiswipe-gesture-overview.png",
        false,
    );
    reader.save().unwrap();

    let mut reopened = canvas_reader(&fixture);
    reopened.open_path(&fixture.book).unwrap();
    assert_eq!(touch(&mut reopened, &CUSTOM_POINTS, START), Change::Partial);
    assert_eq!(reopened.screen, Screen::Find);
    reopened.show(Screen::CustomMultiswipes);
    tap_contents_item(&mut reopened, &CUSTOM.arrows());
    tap_contents_item(&mut reopened, "Remove multiswipe");
    assert_eq!(
        reopened.gestures.multiswipes.configured(CUSTOM),
        Some(Binding::Find)
    );
    tap_contents_item(&mut reopened, "Remove multiswipe");
    assert_eq!(reopened.screen, Screen::CustomMultiswipes);
    assert_eq!(reopened.gestures.multiswipes.configured(CUSTOM), None);
    reopened.show(Screen::Gestures);
    tap_contents_item(&mut reopened, "Multiswipes");
    let reopened = canvas_reader(&fixture);
    assert!(!reopened.gestures.multiswipes.enabled());
    assert_eq!(reopened.gestures.multiswipes.configured(CUSTOM), None);
    assert_eq!(
        reopened
            .gestures
            .multiswipes
            .configured(Shape::new(&[N, E])),
        Some(Binding::Bookmarks)
    );
}

#[test]
fn release_runs_the_action_once_and_restores_the_continuous_view_origin() {
    let fixture = Fixture::new();
    let path = fixture.root.join("Coastal dispatches.epub");
    fs::copy(crate::test_fixture("long-chapter.epub"), &path).unwrap();
    let mut reader = canvas_reader(&fixture);
    reader.open_path(&path).unwrap();
    touch(&mut reader, &[UP, ELBOW], START);
    assert_eq!(reader.screen, Screen::Book);
    reader.activate_multiswipe(Command::Toggle).unwrap();
    reader.show(Screen::Book);
    reader.activate(Action::ReadingMode(true)).unwrap();
    reader.controls.menu_activation = MenuActivation::Tap;
    let position = reader.current_history_location();
    assert_eq!(
        reader.touch_pan(pan(START, UP, 0, PanPhase::Move)),
        Change::Partial
    );
    assert_eq!(reader.screen, Screen::Book);
    assert_ne!(reader.current_history_location(), position);
    assert_eq!(
        reader.touch_pan(pan(START, ELBOW, 230, PanPhase::Release)),
        Change::Partial
    );
    assert_eq!(reader.screen, Screen::Contents);
    assert_eq!(reader.current_history_location(), position);
    assert!(!reader.scroll_pan.busy());
    reader.touch_pan(pan(START, ELBOW, 250, PanPhase::Release));
    assert_eq!(reader.screen, Screen::Contents);
    assert_eq!(reader.current_history_location(), position);
}

#[test]
fn landscape_pdf_reserves_edges_and_drops_interrupted_shapes() {
    let fixture = Fixture::new();
    let book = fixture.root.join("Landscape chart.pdf");
    let pdf = crate::fixed::tests::pdf_objects(&[
        "<< /Type /Catalog /Pages 2 0 R /Outlines 5 0 R >>",
        "<< /Type /Pages /Count 2 /Kids [3 0 R 4 0 R] >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 120 80] >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 120 80] >>",
        "<< /Type /Outlines /First 6 0 R /Last 7 0 R >>",
        "<< /Title (Coastal chart) /Dest [3 0 R /Fit] /Next 7 0 R >>",
        "<< /Title (Soundings) /Dest [4 0 R /Fit] >>",
    ]);
    fs::write(&book, pdf).unwrap();
    let mut reader = canvas_reader(&fixture);
    reader.open_path(&book).unwrap();
    let mut landscape = adopt(&mut reader, ReadingViewport::LANDSCAPE, 1);
    reader.activate_multiswipe(Command::Toggle).unwrap();
    reader.show(Screen::Fixed);
    let origin = reader.current_history_location();
    assert_eq!(touch(&mut reader, &[UP, ELBOW], START), Change::Partial);
    assert_eq!(reader.screen, Screen::FixedContents);
    assert_eq!(reader.current_history_location(), origin);
    capture(
        &reader,
        &mut landscape,
        "multiswipe-landscape-pdf-result.png",
        false,
    );
    tap_contents_item(&mut reader, "Soundings");
    assert_eq!(reader.current_position().unwrap().page, 1);
    reader.controls.menu_activation = MenuActivation::Swipe;
    touch(
        &mut reader,
        &[(840.0, 290.0), (970.0, 580.0)],
        (840.0, 30.0),
    );
    assert_eq!(reader.screen, Screen::FixedTools);
    reader.show(Screen::Fixed);
    reader.touch_pan(pan(START, UP, 0, PanPhase::Move));
    reader.touch_pan(pan(START, UP, 150, PanPhase::Cancel));
    reader.touch_pan(pan(START, ELBOW, 310, PanPhase::Release));
    assert_eq!(reader.screen, Screen::Fixed);
    reader.touch_pan(pan(START, UP, 0, PanPhase::Move));
    reader.input(Input::Sleep);
    reader.input(Input::Wake);
    reader.touch_pan(pan(START, ELBOW, 310, PanPhase::Release));
    assert_eq!(reader.screen, Screen::Fixed);
}
