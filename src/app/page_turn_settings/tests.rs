use super::*;
use crate::app::{
    App, ReaderRuntime, fixed_book_map,
    page_overview::OverviewLayout,
    selection_viewport_tests::{adopt, capture, tap},
    tests::{Fixture, tap_contents_item},
};
use std::{fs, time::Duration};

const LEFT: Input = Input::Tap(40.0, 940.0);
const RIGHT: Input = Input::Tap(1224.0, 940.0);
const FORWARD_SWIPE: Input = Input::Swipe {
    dx: -350.0,
    dy: 0.0,
    start_y: 940.0,
};

fn turn(app: &mut Reader, input: Input) -> Change {
    let change = if let Input::Swipe { .. } = input {
        app.swipe(input, 650.0)
    } else {
        app.input(input)
    };
    if let Some(due) = app.active.as_ref().and_then(|book| book.reading.deadline()) {
        app.tick(due + Duration::from_secs(1));
    }
    if let Some(due) = app.fixed.as_ref().and_then(|book| book.reading.deadline()) {
        app.tick(due + Duration::from_secs(1));
    }
    change
}

fn settings(app: &mut Reader) {
    app.show(Screen::ReadingControls);
    tap_contents_item(app, "Page turns");
    assert_eq!(app.screen, Screen::PageTurns);
}

fn make_reader(fixture: &Fixture) -> Reader {
    Reader::open(
        &fixture.root,
        &fixture.state,
        [
            "NotoSans-Regular.ttf",
            "NotoSans-Bold.ttf",
            "Silkscreen-Regular.ttf",
        ]
        .into_iter()
        .map(|name| crate::test_fixture(&format!("fonts/{name}")))
        .collect(),
    )
    .unwrap()
}

#[test]
fn page_turn_navigation_zone_edges_match_stock_and_keep_blank_space() {
    let portrait = ReadingViewport::PORTRAIT;
    let height = portrait.page_height();
    let sizes = Sizes {
        backward: 20,
        forward: 50,
    };
    let tap = |zones, x, y, inverted| Zones::tap(zones, portrait, height, sizes, inverted, x, y);
    assert_eq!(
        tap(Zones::Default, 315.0, 900.0, false),
        Some(Input::PageBack)
    );
    assert_eq!(
        tap(Zones::Default, 316.0, 900.0, false),
        Some(Input::PageForward)
    );
    assert_eq!(
        tap(Zones::Default, 948.0, 900.0, true),
        Some(Input::PageBack)
    );
    assert_eq!(tap(Zones::LeftRight, 500.0, 900.0, false), None);
    assert_eq!(
        tap(Zones::LeftRight, 633.0, 900.0, false),
        Some(Input::PageForward)
    );
    assert_eq!(
        tap(Zones::TopBottom, 600.0, 300.0, true),
        Some(Input::PageBack)
    );
    assert_eq!(tap(Zones::TopBottom, 600.0, 600.0, false), None);
    assert_eq!(
        tap(Zones::BottomTop, 600.0, 700.0, true),
        Some(Input::PageForward)
    );
    assert_eq!(
        tap(Zones::BottomTop, 600.0, 1400.0, false),
        Some(Input::PageBack)
    );
    assert_eq!(tap(Zones::BottomTop, 600.0, 1650.0, false), None);
    assert_eq!(
        Sizes {
            backward: 80,
            forward: 53
        }
        .normalize(),
        Sizes {
            backward: 47,
            forward: 53
        }
    );
    for (size, expected) in [(0, None), (100, Some(Input::PageForward))] {
        let sizes = Sizes {
            backward: 0,
            forward: size,
        };
        for x in [0.0, 632.0, 1263.0] {
            assert_eq!(
                Zones::LeftRight.tap(portrait, height, sizes, true, x, 900.0),
                expected
            );
        }
    }
}

#[test]
fn page_turn_navigation_touch_switches_keep_physical_turns_and_respect_lock() {
    let fixture = Fixture::new();
    fs::copy(crate::test_fixture("many-chapters.epub"), &fixture.book).unwrap();
    let mut app = make_reader(&fixture);
    app.open_path(&fixture.book).unwrap();
    let start = app.current_history_location().unwrap();
    settings(&mut app);
    tap_contents_item(&mut app, "With taps");
    tap_contents_item(&mut app, "With swipes");
    app.show(Screen::Book);
    assert_eq!(turn(&mut app, RIGHT), Change::None);
    assert_eq!(turn(&mut app, FORWARD_SWIPE), Change::None);
    assert_eq!(app.current_history_location().unwrap(), start);
    let mut runtime = ReaderRuntime(app);
    runtime.input(Input::PageForward);
    if let Some(due) = runtime.0.active.as_ref().unwrap().reading.deadline() {
        runtime.tick(due + Duration::from_secs(1));
    }
    let mut app = runtime.0;
    assert_ne!(app.current_history_location().unwrap(), start);
    turn(&mut app, Input::PageBack);

    settings(&mut app);
    tap_contents_item(&mut app, "With swipes");
    app.show(Screen::Book);
    app.activate(Action::LockTouch).unwrap();
    assert!(app.touch_locked);
    assert_eq!(turn(&mut app, FORWARD_SWIPE), Change::None);
    app.touch_locked = false;
    settings(&mut app);
    tap_contents_item(&mut app, "Swipes always active");
    tap_contents_item(&mut app, "With swipes");
    tap_contents_item(&mut app, "With swipes");
    assert!(app.controls.page_swipes_while_locked);
    app.show(Screen::Book);
    app.activate(Action::LockTouch).unwrap();
    let before = app.current_history_location().unwrap();
    turn(&mut app, FORWARD_SWIPE);
    assert_ne!(app.current_history_location().unwrap(), before);
    assert!(app.touch_locked);
    assert_eq!(turn(&mut app, RIGHT), Change::None);
    let vertical = Input::Swipe {
        dx: 0.0,
        dy: 500.0,
        start_y: 90.0,
    };
    assert_eq!(turn(&mut app, vertical), Change::None);
    assert_eq!(app.screen, Screen::Book);

    let mut reopened = make_reader(&fixture);
    reopened.open_path(&fixture.book).unwrap();
    assert!(reopened.controls.disable_page_taps && reopened.controls.page_swipes_while_locked);
    assert!(!reopened.controls.disable_page_swipes);
    assert_eq!(
        fs::read(&fixture.book).unwrap(),
        fs::read(crate::test_fixture("many-chapters.epub")).unwrap()
    );
}

#[test]
fn page_turn_navigation_visible_zone_editor_applies_a_bounded_pair_in_landscape() {
    let fixture = Fixture::new();
    let mut app = make_reader(&fixture);
    app.open_path(&fixture.book).unwrap();
    settings(&mut app);
    let mut portrait =
        super::super::selection_viewport_tests::canvas(&app, ReadingViewport::PORTRAIT);
    capture(&app, &mut portrait, "page-turn-controls.png", false);
    tap_contents_item(&mut app, "Tap zones");
    tap_contents_item(&mut app, "Top / bottom");
    tap_contents_item(&mut app, "Backward and forward zone sizes");
    let mut landscape = adopt(&mut app, ReadingViewport::LANDSCAPE, 1);
    let width = app.navigation_layout().width;
    let apply = zone_size_actions(width).1;
    let decrease = width - 260;
    let increase = width - 108;
    for _ in 0..5 {
        assert_eq!(
            app.input(Input::LongTap(decrease as f32, 503.0)),
            Change::Partial
        );
    }
    assert_eq!(
        app.input(Input::Tap(increase as f32, 366.0)),
        Change::Partial
    );
    capture(&app, &mut landscape, "page-turn-zone-sizes.png", true);
    assert_eq!(app.controls.page_turn_sizes, Sizes::default());
    tap(&mut app, apply);
    assert_eq!(app.screen, Screen::PageTurnZones);
    assert_eq!(
        app.controls.page_turn_sizes,
        Sizes {
            backward: 26,
            forward: 50
        }
    );
    assert_eq!(
        app.tap_turn(100.0, 450.0, app.reading_viewport().page_height()),
        None
    );
    assert_eq!(
        app.tap_turn(100.0, 870.0, app.reading_viewport().page_height()),
        Some(Input::PageForward)
    );
    tap_contents_item(&mut app, "Backward and forward zone sizes");
    tap(&mut app, zone_size_actions(width).0);
    let back = app.navigation_layout().back();
    tap(&mut app, back);
    assert_eq!(
        app.controls.page_turn_sizes,
        Sizes {
            backward: 26,
            forward: 50
        }
    );
    assert_eq!(
        make_reader(&fixture).controls.page_turn_zones,
        Zones::TopBottom
    );
}

#[test]
fn page_turn_navigation_direction_and_dialog_mirror_are_saved_per_edition() {
    let fixture = Fixture::new();
    let pdf = fixture.root.join("Tilted Scan.pdf");
    fs::copy(crate::test_fixture("tilted-scan.pdf"), &pdf).unwrap();
    let mut app = make_reader(&fixture);
    app.open_path(&pdf).unwrap();
    let origin = app.current_history_location().unwrap();
    settings(&mut app);
    tap_contents_item(&mut app, "Invert page turn taps and swipes");
    assert_eq!(app.current_history_location().unwrap(), origin);
    app.show(Screen::Fixed);
    turn(&mut app, LEFT);
    assert_eq!(app.current_position().unwrap().page, 1);
    turn(&mut app, FORWARD_SWIPE);
    assert_eq!(app.current_position().unwrap().page, 0);
    turn(&mut app, Input::PageForward);
    assert_eq!(app.current_position().unwrap().page, 1);
    settings(&mut app);
    tap_contents_item(&mut app, "Invert document-related dialogs");
    tap_contents_item(&mut app, "Defaults for new books");
    tap_contents_item(&mut app, "Right to left");
    assert!(app.document_dialogs_mirrored());
    app.show(Screen::BookMap);
    let navigation = app.navigation_layout();
    assert!(navigation.mirrored && navigation.back().x < 100);
    let tile = fixed_book_map::projected_cell(navigation, 0);
    assert!(tile.x > navigation.width / 2);
    tap(&mut app, tile);
    assert_eq!(app.screen, Screen::PageOverview);
    let navigation = app.navigation_layout();
    let overview = OverviewLayout::new(navigation);
    assert!(overview.tile(0).x > overview.tile(1).x);
    let mut canvas =
        super::super::selection_viewport_tests::canvas(&app, ReadingViewport::PORTRAIT);
    capture(&app, &mut canvas, "page-turn-mirrored-browser.png", false);
    tap(&mut app, overview.tile(0));
    assert_eq!(app.current_history_location().unwrap(), origin);

    app.open_path(&fixture.book).unwrap();
    assert!(app.touch_turn_inverted(false));
    assert!(!app.document_dialogs_mirrored());
    settings(&mut app);
    tap_contents_item(&mut app, "Invert document-related dialogs");
    app.show(Screen::Contents);
    assert!(app.navigation_layout().mirrored);
    capture(&app, &mut canvas, "page-turn-mirrored-contents.png", false);
    let back = app.navigation_layout().back();
    tap(&mut app, back);
    assert_eq!(app.screen, Screen::BookTools);

    let mut reopened = make_reader(&fixture);
    reopened.open_path(&pdf).unwrap();
    assert!(reopened.touch_turn_inverted(false) && reopened.document_dialogs_mirrored());
    assert_eq!(reopened.current_history_location().unwrap(), origin);
}

#[test]
fn page_turn_navigation_swipe_switches_preserve_reflow_scroll_and_menu_actions() {
    let fixture = Fixture::new();
    fs::copy(crate::test_fixture("many-chapters.epub"), &fixture.book).unwrap();
    let mut app = make_reader(&fixture);
    app.open_path(&fixture.book).unwrap();
    app.activate(Action::ReadingMode(true)).unwrap();
    settings(&mut app);
    tap_contents_item(&mut app, "With swipes");
    app.show(Screen::Book);
    let top = |app: &Reader| {
        let reading = &app.active.as_ref().unwrap().reading;
        (reading.chapter, reading.scroll_history_top())
    };
    let origin = top(&app);
    assert!(origin.1.is_some());
    assert_eq!(turn(&mut app, FORWARD_SWIPE), Change::None);
    assert_eq!(top(&app), origin);
    let vertical = Input::Swipe {
        dx: 0.0,
        dy: -360.0,
        start_y: 940.0,
    };
    turn(&mut app, vertical);
    assert_ne!(top(&app), origin);
    let menu = Input::Swipe {
        dx: 0.0,
        dy: 260.0,
        start_y: 45.0,
    };
    assert_eq!(turn(&mut app, menu), Change::Partial);
    assert_eq!(app.screen, Screen::BookTools);
}

#[test]
fn page_turn_navigation_mirrored_skim_keeps_cancel_at_the_same_passage() {
    let fixture = Fixture::new();
    let mut app = make_reader(&fixture);
    fs::copy(crate::test_fixture("many-chapters.epub"), &fixture.book).unwrap();
    app.open_path(&fixture.book).unwrap();
    let origin = app.current_history_location().unwrap();
    settings(&mut app);
    tap_contents_item(&mut app, "Invert document-related dialogs");
    assert_eq!(origin, app.current_history_location().unwrap());
    app.activate(Action::Skim).unwrap();
    let layout = app.skim_layout();
    assert!(layout.commit.x < layout.cancel.x);
    assert_eq!(
        app.input(Input::Tap(
            (layout.track.x + 30) as f32,
            (layout.track.y + 25) as f32
        )),
        Change::Partial
    );
    assert_ne!(origin, app.current_history_location().unwrap());
    tap(&mut app, layout.cancel);
    assert_eq!(origin, app.current_history_location().unwrap());
    assert!(app.back.is_empty());
}
