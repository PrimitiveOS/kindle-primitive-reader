use super::*;
use crate::app::tests::{Fixture, capture_test_screen};

fn tap(app: &mut Reader, rect: Rect) {
    assert!(rect.w >= 100 && rect.h >= 100, "{rect:?}");
    assert_eq!(
        app.input(Input::Tap(
            (rect.x + rect.w / 2) as f32,
            (rect.y + rect.h / 2) as f32
        )),
        Change::Partial
    );
}

fn tap_button(app: &mut Reader, label: &str) {
    tap(app, app.reading_button(label));
}

fn tap_row_button(app: &mut Reader, row: &str, label: &str) {
    let rect = app.reading_button_in_row(row, label);
    tap(app, rect);
}

#[test]
fn scroll_page_overlap_controls_are_visible_and_persist_per_book() {
    let fixture = Fixture::new();
    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
    app.open_path(&fixture.book).unwrap();

    app.open_reading_edge(false, WIDTH as f32 / 2.0);
    if app.reading_panel != Some(BarPanel::Text) {
        tap_button(&mut app, "Type");
    }
    tap_button(&mut app, "Scroll");
    assert!(app.active.as_ref().unwrap().reading.config.scroll);

    app.open_reading_edge(false, WIDTH as f32 / 2.0);
    if app.reading_panel != Some(BarPanel::TextLayout) {
        tap_button(&mut app, "Layout");
    }
    tap_row_button(&mut app, "Page overlap", "On");
    tap_button(&mut app, "7");
    tap_row_button(&mut app, "Overlap marker", "Dashed line");

    let settings = app.active.as_ref().unwrap().reading.config.settings;
    assert!(settings.page_overlap_enabled);
    assert_eq!(settings.page_overlap_lines, 7);
    assert_eq!(
        settings.page_overlap_style,
        crate::PageOverlapStyle::DashedLine
    );
    capture_test_screen(
        &app,
        &fonts,
        "READER_PAGE_OVERLAP_SETTINGS_SCREENSHOT",
        false,
    );
    app.save().unwrap();

    let mut reopened = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
    reopened.open_path(&fixture.book).unwrap();
    let settings = reopened.active.as_ref().unwrap().reading.config.settings;
    assert!(settings.page_overlap_enabled);
    assert_eq!(settings.page_overlap_lines, 7);
    assert_eq!(
        settings.page_overlap_style,
        crate::PageOverlapStyle::DashedLine
    );
}

#[test]
fn fixed_page_overlap_controls_apply_on_a_hardware_turn_and_restore_per_pdf() {
    let fixture = Fixture::new();
    let book = fixture.root.join("Overlap.pdf");
    std::fs::copy(crate::test_fixture("scan-reflow/vector.pdf"), &book).unwrap();
    let fonts = vec![
        crate::test_fixture("fonts/NotoSans-Regular.ttf"),
        crate::test_fixture("fonts/NotoSans-Bold.ttf"),
    ];
    let font_paths = fonts
        .iter()
        .map(|font| font.display().to_string())
        .collect::<Vec<_>>();
    let mut canvas = Canvas::new(WIDTH, HEIGHT, &font_paths).unwrap();
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
    app.open_path(&book).unwrap();
    assert!(!app.fixed.as_ref().unwrap().reading.allows_page_overlap());
    app.activate(Action::FitPage(true)).unwrap();
    app.open_reading_edge(false, WIDTH as f32 / 2.0);
    if app.reading_panel != Some(BarPanel::ViewLayout) {
        tap_button(&mut app, "Layout");
    }
    tap_row_button(&mut app, "Page overlap", "On");
    tap_row_button(&mut app, "Overlap marker", "Dashed line");
    capture_test_screen(&app, &fonts, "READER_FIXED_OVERLAP_SETTINGS", false);
    app.show(Screen::Fixed);

    let start = app.fixed.as_ref().unwrap().reading.preferences();
    let _ = app.input(Input::PageForward);
    app.tick(Instant::now() + Duration::from_secs(1));
    let reading = &app.fixed.as_ref().unwrap().reading;
    let pose = reading.preferences();
    assert_eq!(reading.page(), 0);
    assert!(pose.offset_y < start.offset_y);
    let current_location = app.current_history_location();
    let repeated = primitive_app::frame(&app, &mut canvas, false);
    capture_test_screen(&app, &fonts, "READER_FIXED_OVERLAP_READING", false);

    app.activate(Action::SetFixedPageOverlap(
        true,
        crate::PageOverlapStyle::None,
    ))
    .unwrap();
    assert_eq!(app.current_history_location(), current_location);
    let unmarked = primitive_app::frame(&app, &mut canvas, false);
    let distance = (start.offset_y - pose.offset_y).round() as usize;
    let viewport_height = app.fixed.as_ref().unwrap().reading.frame_height() as usize;
    let boundary = (viewport_height - distance) * WIDTH as usize;
    let next_row = boundary + 3 * WIDTH as usize;
    assert_eq!(&repeated[..boundary], &unmarked[..boundary]);
    assert_ne!(&repeated[boundary..next_row], &unmarked[boundary..next_row]);
    assert_eq!(&repeated[next_row..], &unmarked[next_row..]);

    app.activate(Action::SetFixedPageOverlap(
        true,
        crate::PageOverlapStyle::DashedLine,
    ))
    .unwrap();
    app.save().unwrap();
    let mut reopened = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
    reopened.open_path(&book).unwrap();
    let prefs = reopened.fixed.as_ref().unwrap().reading.preferences();
    assert!(prefs.page_overlap_enabled);
    assert_eq!(
        prefs.page_overlap_style,
        Some(crate::PageOverlapStyle::DashedLine)
    );
    assert_eq!(reopened.current_history_location(), current_location);
}

#[test]
fn fixed_reflow_overlap_layout_persists_without_moving_source_page_or_bookmark() {
    let fixture = Fixture::new();
    let fonts = vec![
        crate::test_fixture("fonts/NotoSans-Regular.ttf"),
        crate::test_fixture("fonts/NotoSans-Bold.ttf"),
    ];
    for (scan, name) in [(false, "vector"), (true, "raster")] {
        let book = fixture.root.join(format!("{name}.pdf"));
        std::fs::copy(
            crate::test_fixture(&format!("scan-reflow/{name}.pdf")),
            &book,
        )
        .unwrap();
        let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
        app.open_path(&book).unwrap();
        app.activate(Action::FixedPage(1)).unwrap();
        app.activate(Action::ToggleBookmark).unwrap();
        let fixed = app.fixed.as_mut().unwrap();
        let id = fixed.id;
        if scan {
            let ocr = crate::ocr::parse_tsv(
                include_bytes!("../../tests/fixtures/scan-reflow/page-2-ocr.tsv"),
                1350,
                1800,
            )
            .unwrap();
            fixed.reading.remember_ocr(1, "eng", ocr.text).unwrap();
        }
        let position = app.current_history_location();
        let bookmarks = app.library.state(id).unwrap().bookmarks.len();
        assert!(bookmarks > 0);
        app.activate(if scan {
            Action::ReflowScan
        } else {
            Action::ReadFixedText
        })
        .unwrap();
        let [layout, enable, dashed, apply] =
            app.text_flow.as_ref().unwrap().overlap_test_targets();
        for button in [layout, enable, dashed, apply] {
            assert_eq!(
                app.input(Input::Tap(
                    (button.x + button.w / 2) as f32,
                    (button.y + button.h / 2) as f32
                )),
                Change::Partial
            );
            if button == dashed && !scan {
                capture_test_screen(&app, &fonts, "READER_REFLOW_OVERLAP_SETTINGS", false);
            }
        }
        let mut changed = app.input(Input::PageForward);
        if let Some(due) = app.text_flow.as_ref().and_then(TextFlow::deadline) {
            changed = app.tick(due);
        }
        assert_eq!(changed, Change::Partial);
        assert!(app.text_flow.as_ref().unwrap().pan_offset() > 0);
        assert_eq!(app.current_history_location(), position);
        assert_eq!(app.library.state(id).unwrap().bookmarks.len(), bookmarks);
        capture_test_screen(
            &app,
            &fonts,
            if scan {
                "READER_REFLOW_OVERLAP_RASTER"
            } else {
                "READER_REFLOW_OVERLAP_TEXT"
            },
            false,
        );

        app.save().unwrap();
        let mut restored = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
        restored.open_path(&book).unwrap();
        let fixed = restored.fixed.as_ref().unwrap();
        let prefs = fixed.reading.preferences();
        assert!(prefs.page_overlap_enabled);
        assert_eq!(
            prefs.page_overlap_style,
            Some(crate::PageOverlapStyle::DashedLine)
        );
        assert_eq!(fixed.reading.page(), 1);
        assert_eq!(
            restored.library.state(fixed.id).unwrap().bookmarks.len(),
            bookmarks
        );
    }
}
