use super::*;
use crate::app::{
    Action, Screen,
    footer_custom_text::clock_at,
    tests::{Fixture, capture_test_screen, tap_contents_item},
};
use primitive_app::{App, Change, HEIGHT, WIDTH};
use primitive_ui::Input;
use std::{fs, path::Path};

#[test]
fn top_header_flow_values_match_the_selected_stock_fields() {
    let pages = Pagination {
        current: 5,
        count: 9,
        flow: None,
    };
    let header = TopHeader::default();
    assert_eq!(pages.info(header, None).as_deref(), Some("5 / 9"));
    assert_eq!(
        pages
            .info(
                TopHeader {
                    page_number: false,
                    ..header
                },
                None
            )
            .as_deref(),
        Some("9")
    );
    assert_eq!(
        pages
            .info(
                TopHeader {
                    page_count: false,
                    ..header
                },
                None
            )
            .as_deref(),
        Some("5")
    );
    assert_eq!(pages.percentage(2), "55.56%");
    let regular = Pagination {
        flow: Some(0),
        ..pages
    };
    assert_eq!(regular.info(header, None).as_deref(), Some("5 // 9"));
    let note = Pagination {
        current: 2,
        count: 3,
        flow: Some(1),
    };
    assert_eq!(note.info(header, None).as_deref(), Some("[2 / 3]1"));
    assert_eq!(note.percentage(1), "[66.7%]");
    assert_eq!(
        note.info(header, Some(("iii".into(), "A-19".into())))
            .as_deref(),
        Some("iii / A-19")
    );
}

#[test]
fn top_header_alternates_long_identity_without_clipping_graphemes() {
    let parts = HeaderParts {
        title: "Bound editions".into(),
        author: "Harbor keepers".into(),
        status: String::new(),
    };
    let measure = |text: &str| text.chars().count() as i32 * 10;
    assert_eq!(
        parts.fitted_identity(400, 1, measure),
        "Harbor keepers.  Bound editions"
    );
    assert_eq!(parts.fitted_identity(140, 1, measure), "Harbor keepers");
    assert_eq!(parts.fitted_identity(140, 2, measure), "Bound editions");
    assert_eq!(parts.fitted_identity(80, 2, measure), "Bound e…");
}

#[test]
fn top_header_track_has_stock_progress_weight_and_notches() {
    let fonts = vec![
        crate::test_fixture("fonts/NotoSans-Regular.ttf")
            .display()
            .to_string(),
    ];
    let mut canvas = Canvas::new(WIDTH, HEIGHT, &fonts).unwrap();
    let y = TopHeader::HEIGHT as usize - 4;
    let offset = |x, row| WIDTH as usize * row + x;
    let draw = |canvas: &mut Canvas, marks| {
        canvas.begin();
        draw_header_track(
            canvas,
            WIDTH as i32,
            0.5,
            marks,
            [0.25, 0.75].into_iter(),
            0,
            255,
        );
        canvas.finish(false)
    };
    let marked = draw(&mut canvas, true);
    let plain = draw(&mut canvas, false);
    assert_eq!(marked[offset(30, y - 1)], 0);
    assert_eq!(marked[offset(900, y - 1)], 255);
    assert_eq!(marked[offset(321, y)], 255);
    assert_eq!(plain[offset(321, y)], 0);
    assert_eq!(marked[offset(940, y - 3)], 0);
    assert_eq!(plain[offset(940, y - 3)], 255);
}

#[test]
fn top_header_ui_uses_alternate_identity_battery_choices_and_keeps_the_default_footer() {
    let fixture = Fixture::new();
    let original = fs::read(&fixture.book).unwrap();
    let sidecar = fixture.book.with_extension("sdr");
    fs::create_dir(&sidecar).unwrap();
    let path = sidecar.join("custom_metadata.lua");
    let custom = b"return {custom_props = {title = 'Coastal Ledger', authors = 'Ada Scout'}}";
    fs::write(&path, custom).unwrap();
    let fonts = vec![
        crate::test_fixture("fonts/NotoSans-Regular.ttf"),
        crate::test_fixture("fonts/NotoSans-Bold.ttf"),
        crate::test_fixture("fonts/NotoSansArabic-Regular.ttf"),
    ];
    let font_paths = fonts
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>();
    let mut canvas = Canvas::new(WIDTH, HEIGHT, &font_paths).unwrap();
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
    app.open_path(&fixture.book).unwrap();
    app.show(Screen::Book);
    let origin = app.current_history_location();
    let default = primitive_app::frame(&app, &mut canvas, false);
    let default_text = app.status_caption();
    assert!(default_text.as_ref().unwrap().starts_with("Chapter 1 of "));
    assert!(app.top_header_caption().is_none());

    app.activate(Action::TopStatusHeader(true)).unwrap();
    assert_eq!(app.current_history_location(), origin);
    let caption = app.top_header_caption().unwrap();
    assert!(caption.starts_with("Ada Scout.  Coastal Ledger"));
    app.show(Screen::FooterSettings);
    tap_contents_item(&mut app, "Header battery status");
    assert_eq!(app.screen, Screen::HeaderBatterySettings);
    capture_test_screen(&app, &fonts, "READER_CRE_HEADER_BATTERY", false);
    tap_contents_item(&mut app, "Battery icon");
    assert_eq!(app.controls.top_header.battery, HeaderBattery::Off);
    tap_contents_item(&mut app, "Battery percentage");
    assert_eq!(app.controls.top_header.battery, HeaderBattery::Percentage);
    let back = app.navigation_layout().back();
    assert_eq!(
        app.input(Input::Tap((back.x + 4) as f32, (back.y + 4) as f32)),
        Change::Partial
    );
    assert_eq!(app.screen, Screen::FooterSettings);

    app.show(Screen::Book);
    let active = app.active.as_ref().unwrap();
    let flows = header_flows(active);
    let pages = Pagination::from_book(active, flows.as_ref());
    let battery = BatteryStatus {
        percent: Some(37),
        charging: true,
        fully_charged: false,
    };
    let time = clock_at(1795560000).unwrap();
    let status = app
        .header_parts(active, &pages, Some(&time), battery)
        .status;
    assert!(status.ends_with("[+37%]"));
    assert!(!status.contains("Time "));
    assert_eq!(app.status_caption(), default_text);
    let actual = primitive_app::frame(&app, &mut canvas, false);
    canvas.begin();
    app.draw(&mut canvas);
    app.draw_top_header_with_readings(&mut canvas, None, battery);
    let injected = canvas.finish(false);
    let limit = WIDTH as usize * TopHeader::HEIGHT as usize;
    assert_ne!(injected[..limit], actual[..limit]);
    assert_eq!(injected[limit..], actual[limit..]);
    if let Some(output) = std::env::var_os("READER_CRE_HEADER_COMPOSED") {
        primitive_app::write_png(Path::new(&output), WIDTH, HEIGHT, &injected).unwrap();
    }
    app.activate(Action::TopStatusHeader(false)).unwrap();
    assert_eq!(app.current_history_location(), origin);
    assert_eq!(primitive_app::frame(&app, &mut canvas, false), default);
    app.save().unwrap();
    let mut reopened = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
    reopened.open_path(&fixture.book).unwrap();
    assert_eq!(
        reopened.controls.top_header.battery,
        HeaderBattery::Percentage
    );
    assert_eq!(fs::read(&fixture.book).unwrap(), original);
    assert_eq!(fs::read(path).unwrap(), custom);
}
