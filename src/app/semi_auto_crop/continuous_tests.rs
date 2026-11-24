use super::*;
use crate::app::tests::{Fixture, capture_test_screen};

fn authored_pdf() -> Vec<u8> {
    let stream = |text: &str| format!("<< /Length {} >>\nstream\n{text}endstream", text.len());
    crate::fixed::tests::pdf_objects(&[
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        "<< /Type /Pages /Kids [3 0 R 4 0 R 5 0 R] /Count 3 /MediaBox [0 0 600 800] /Resources << /Font << /F1 9 0 R >> >> >>".into(),
        "<< /Type /Page /Parent 2 0 R /Contents 6 0 R >>".into(),
        "<< /Type /Page /Parent 2 0 R /Contents 7 0 R /Annots [10 0 R 11 0 R] >>".into(),
        "<< /Type /Page /Parent 2 0 R /Contents 8 0 R >>".into(),
        stream(concat!(
            "0 g 100 670 280 5 re f 100 160 280 5 re f\n",
            "BT /F1 19 Tf 111 633 Td (Western shelf) Tj ET\n",
            "BT /F1 13 Tf 111 458 Td (Follow the shore markers in order.) Tj ET\n",
            "BT /F1 13 Tf 111 212 Td (Last notes from the western shelf.) Tj ET\n",
        )),
        stream(concat!(
            "0 g 305 680 195 5 re f 305 260 195 5 re f\n",
            "BT /F1 19 Tf 309 647 Td (Eastern basin) Tj ET\n",
            "BT /F1 13 Tf 309 615 Td (See the landing map.) Tj ET\n",
            "BT /F1 13 Tf 309 585 Td (Follow the channel on a rising tide.) Tj ET\n",
            "BT /F1 13 Tf 309 555 Td (Anchor beyond the narrow shelf.) Tj ET\n",
            "0 g 45 650 24 16 re f\n",
        )),
        stream(concat!(
            "0 g 100 680 270 5 re f 100 150 270 5 re f\n",
            "BT /F1 19 Tf 110 647 Td (Landing map) Tj ET\n",
            "BT /F1 13 Tf 110 615 Td (The lower steps stay sheltered.) Tj ET\n",
        )),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),
        "<< /Subtype /Link /Rect [430 646 481 663] /Dest [5 0 R /XYZ 200 685 null] >>".into(),
        "<< /Subtype /Link /Rect [45 650 69 666] /Dest [3 0 R /Fit] >>".into(),
    ])
}

fn bounds(x0: f32, y0: f32, x1: f32, y1: f32) -> PdfTextRect {
    PdfTextRect { x0, y0, x1, y1 }
}

fn tap(rect: Rect) -> Input {
    Input::Tap((rect.x + rect.w / 2) as f32, (rect.y + rect.h / 2) as f32)
}

fn reading(app: &Reader) -> &FixedReading {
    &app.fixed.as_ref().unwrap().reading
}

fn apply_seed(app: &mut Reader, seed: PdfTextRect) {
    app.activate(Action::SemiAutoCrop).unwrap();
    let draft = app.semi_auto_crop.as_mut().unwrap();
    assert!(draft.original.continuous && draft.original.width_fit);
    draft.seed = Seed::from_normalized(seed, draft.size);
    assert_eq!(app.input(tap(APPLY)), Change::Partial);
    assert_eq!(app.screen, Screen::Fixed);
    assert!(reading(app).is_continuous());
    assert!(reading(app).preferences().semi_auto_crop);
}

#[test]
fn continuous_semi_auto_crop_resolves_each_strip_and_preserves_original_taps() {
    let paths = Fixture::new();
    let document = paths.root.join("Alternating PDF crops.pdf");
    fs::write(&document, authored_pdf()).unwrap();
    let fonts = vec![
        crate::test_fixture("fonts/NotoSans-Regular.ttf"),
        crate::test_fixture("fonts/NotoSans-Bold.ttf"),
    ];
    let mut app = Reader::open(&paths.root, &paths.state, fonts.clone()).unwrap();
    app.open_path(&document).unwrap();
    app.activate(Action::FixedScroll(true)).unwrap();
    app.activate(Action::FixedGap(24)).unwrap();
    assert!(
        app.scan_items()
            .iter()
            .any(|item| item.title == "Semi-auto crop")
    );
    let odd = bounds(0.13, 0.05, 0.67, 0.95);
    let even = bounds(0.47, 0.05, 0.88, 0.95);
    apply_seed(&mut app, odd);
    let first = reading(&app).preferences().crop.unwrap();
    app.activate(Action::FixedPage(1)).unwrap();
    assert_eq!(reading(&app).page(), 1);
    apply_seed(&mut app, even);
    let second = reading(&app).preferences().crop.unwrap();
    assert!(second.x0 > first.x0 + 0.25);
    assert!((second.x1 - second.x0) < (first.x1 - first.x0));
    app.activate(Action::FixedPage(0)).unwrap();

    let first_height =
        (first.y1 - first.y0) * 800.0 * reading(&app).page_viewport().unwrap().1.scale;
    for delta in [1200.0, first_height - 1200.0 - 515.0] {
        app.input(Input::Swipe {
            dx: 0.0,
            dy: -delta,
            start_y: 1000.0,
        });
        app.tick(Instant::now() + Duration::from_secs(1));
    }
    assert_eq!(reading(&app).visible_pages().collect::<Vec<_>>(), [0, 1]);
    assert_eq!(reading(&app).preferences().crop, Some(first));
    let original = app.current_history_location().unwrap();
    let saved_view = reading(&app).preferences();
    let font_paths: Vec<_> = fonts
        .iter()
        .map(|font| font.display().to_string())
        .collect();
    let mut canvas = Canvas::new(WIDTH, HEIGHT, &font_paths).unwrap();
    let original_frame = primitive_app::frame(&app, &mut canvas, false);
    app.activate(Action::SemiAutoCrop).unwrap();
    assert_eq!(reading(&app).visible_pages().collect::<Vec<_>>(), [0]);
    app.input(tap(CANCEL_CROP));
    assert_eq!(app.current_history_location(), Some(original.clone()));
    assert_eq!(reading(&app).preferences(), saved_view);
    assert_eq!(
        primitive_app::frame(&app, &mut canvas, false),
        original_frame
    );

    apply_seed(&mut app, odd);
    assert_eq!(app.current_history_location(), Some(original.clone()));
    assert_eq!(reading(&app).visible_pages().collect::<Vec<_>>(), [0, 1]);
    capture_test_screen(&app, &fonts, "READER_CONTINUOUS_CROP_FRAME", false);
    app.save().unwrap();
    let mut reopened = Reader::open(&paths.root, &paths.state, fonts).unwrap();
    reopened.open_path(&document).unwrap();
    assert_eq!(reopened.current_history_location(), Some(original.clone()));
    assert_eq!(
        reading(&reopened).visible_pages().collect::<Vec<_>>(),
        [0, 1]
    );

    let retained = bounds(0.69, 0.22, 0.76, 0.24);
    let clipped = bounds(0.075, 0.1675, 0.115, 0.188);
    assert!(retained.x0 > first.x1);
    assert!(reading(&reopened).visible_rectangle(1, clipped).is_none());
    let location = reading(&reopened)
        .visible_rectangle(1, retained)
        .unwrap()
        .center();
    let active = reopened.fixed.as_ref().unwrap();
    reopened
        .library
        .remember(&active.entry, active.id, &active.entry.title)
        .pdf_highlights
        .push(PdfHighlight {
            chapter_title: None,
            created_at: None,
            style: Default::default(),
            color: Default::default(),
            page: 1,
            bounds: vec![retained],
            text: "Anchor beyond the narrow shelf".into(),
            note: Some("Eastern references".into()),
            continuation: Vec::new(),
        });
    reopened.input(Input::Tap(location.x as f32, location.y as f32));
    assert!(reopened.saved_popup.is_some());
    reopened.saved_popup = None;
    let links = reading(&reopened).visible_links();
    assert_eq!(links.len(), 1);
    let link = reading(&reopened)
        .visible_rectangle(1, links[0].bounds)
        .unwrap()
        .center();
    reopened.input(Input::Tap(link.x as f32, link.y as f32));
    assert_eq!(reading(&reopened).page(), 2);
    reopened.activate(Action::ReturnToLocation).unwrap();
    assert_eq!(reopened.current_history_location(), Some(original));

    reopened.activate(Action::SetFixedRotation(1)).unwrap();
    let turned = reading(&reopened).page_viewport().unwrap().1;
    assert_eq!(turned.turns, 1);
    let crop = reading(&reopened).preferences().crop.unwrap();
    assert!((turned.scale - WIDTH as f32 / (800.0 * (crop.y1 - crop.y0))).abs() < 0.001);
    reopened.activate(Action::SetFixedZoom(1)).unwrap();
    let scaled = reading(&reopened).page_viewport().unwrap().1;
    assert!((scaled.scale - turned.scale * 1.25).abs() < 0.001);
    reopened.activate(Action::FixedScroll(false)).unwrap();
    assert!(!reading(&reopened).is_continuous());
    assert!(reading(&reopened).preferences().semi_auto_crop);
    reopened.activate(Action::FixedScroll(true)).unwrap();
    assert!(reading(&reopened).is_continuous());
    assert_eq!(reading(&reopened).preferences().crop, Some(crop));
}

#[test]
fn continuous_manual_and_auto_crop_switch_without_losing_source_geometry() {
    use crate::library::ContinuousCrop;
    let paths = Fixture::new();
    let document = paths.root.join("Alternating crop modes.pdf");
    fs::write(&document, authored_pdf()).unwrap();
    let fonts = vec![
        crate::test_fixture("fonts/NotoSans-Regular.ttf"),
        crate::test_fixture("fonts/NotoSans-Bold.ttf"),
    ];
    let mut app = Reader::open(&paths.root, &paths.state, fonts.clone()).unwrap();
    app.open_path(&document).unwrap();
    app.activate(Action::FixedScroll(true)).unwrap();
    app.activate(Action::FixedGap(24)).unwrap();
    let odd = bounds(0.12, 0.12, 0.69, 0.84);
    let even = bounds(0.45, 0.1, 0.87, 0.76);
    for (page, rect) in [(0, odd), (1, even)] {
        app.activate(Action::FixedPage(page)).unwrap();
        app.activate(Action::CropPage).unwrap();
        let draft = app.semi_auto_crop.as_mut().unwrap();
        assert!(draft.manual);
        draft.seed = Seed::from_normalized(rect, draft.size);
        assert_eq!(app.input(tap(APPLY)), Change::Partial);
        assert_eq!(app.screen, Screen::Fixed);
        let view = reading(&app).preferences();
        assert_eq!(view.continuous_crop, Some(ContinuousCrop::Manual));
        assert_eq!(view.crop, Some(rect));
        assert_eq!(view.straighten_limit, 0);
    }
    app.activate(Action::FixedPage(2)).unwrap();
    assert_eq!(reading(&app).preferences().crop, Some(odd));
    app.activate(Action::FixedPage(0)).unwrap();
    let first_height = (odd.y1 - odd.y0) * 800.0 * reading(&app).page_viewport().unwrap().1.scale;
    for delta in [1200.0, first_height - 1200.0 - 470.0] {
        app.input(Input::Swipe {
            dx: 0.0,
            dy: -delta,
            start_y: 1000.0,
        });
        app.tick(Instant::now() + Duration::from_secs(1));
    }
    assert_eq!(reading(&app).visible_pages().collect::<Vec<_>>(), [0, 1]);
    capture_test_screen(&app, &fonts, "READER_CONTINUOUS_MANUAL_CROP_FRAME", false);
    let old_view = reading(&app).preferences();
    let old_location = app.current_history_location();
    let font_paths = fonts
        .iter()
        .map(|font| font.display().to_string())
        .collect::<Vec<_>>();
    let mut canvas = Canvas::new(WIDTH, HEIGHT, &font_paths).unwrap();
    let old_frame = primitive_app::frame(&app, &mut canvas, false);
    app.activate(Action::CropPage).unwrap();
    assert_eq!(app.input(Input::PageBack), Change::Partial);
    assert_eq!(reading(&app).preferences(), old_view);
    assert_eq!(app.current_history_location(), old_location);
    assert_eq!(primitive_app::frame(&app, &mut canvas, false), old_frame);

    let retained = bounds(0.69, 0.22, 0.76, 0.24);
    let clipped = bounds(0.075, 0.1675, 0.115, 0.188);
    assert!(reading(&app).visible_rectangle(1, clipped).is_none());
    let note = reading(&app)
        .visible_rectangle(1, retained)
        .unwrap()
        .center();
    let active = app.fixed.as_ref().unwrap();
    app.library
        .remember(&active.entry, active.id, &active.entry.title)
        .pdf_highlights
        .push(PdfHighlight {
            chapter_title: None,
            created_at: None,
            style: Default::default(),
            color: Default::default(),
            page: 1,
            bounds: vec![retained],
            text: "Eastern references".into(),
            note: Some("An original source-page note".into()),
            continuation: Vec::new(),
        });
    app.input(Input::Tap(note.x as f32, note.y as f32));
    assert!(app.saved_popup.take().is_some());
    let links = reading(&app).visible_links();
    assert_eq!(links.len(), 1);
    let link = reading(&app)
        .visible_rectangle(1, links[0].bounds)
        .unwrap()
        .center();
    app.input(Input::Tap(link.x as f32, link.y as f32));
    assert_eq!(reading(&app).page(), 2);
    app.activate(Action::ReturnToLocation).unwrap();
    assert_eq!(app.current_history_location(), old_location);

    app.activate(Action::CropPage).unwrap();
    assert_eq!(app.input(tap(AUTO)), Change::Partial);
    assert_eq!(
        reading(&app).preferences().continuous_crop,
        Some(ContinuousCrop::Auto)
    );
    assert_ne!(reading(&app).preferences().crop, Some(odd));
    app.activate(Action::FixedPage(1)).unwrap();
    let auto_even = reading(&app).preferences().crop.unwrap();
    assert!(auto_even.x0 < 0.1);
    assert!(reading(&app).visible_rectangle(1, clipped).is_some());
    assert!(reading(&app).visible_links().len() >= 2);
    app.save().unwrap();
    drop(app);

    let mut reopened = Reader::open(&paths.root, &paths.state, fonts).unwrap();
    reopened.open_path(&document).unwrap();
    assert_eq!(
        reading(&reopened).preferences().continuous_crop,
        Some(ContinuousCrop::Auto)
    );
    assert_eq!(reading(&reopened).preferences().crop, Some(auto_even));
    reopened.activate(Action::CropPage).unwrap();
    let draft = reopened.semi_auto_crop.unwrap();
    assert_eq!(draft.seed.normalized(draft.size), Some(even));
    assert_eq!(reopened.input(tap(APPLY)), Change::Partial);
    assert_eq!(reading(&reopened).preferences().crop, Some(even));
    reopened.activate(Action::SetFixedRotation(1)).unwrap();
    reopened.activate(Action::SetFixedZoom(1)).unwrap();
    let viewport = reading(&reopened).page_viewport().unwrap().1;
    assert_eq!(viewport.turns, 1);
    assert_eq!(reading(&reopened).preferences().straighten_limit, 0);
    let active_page = reading(&reopened).page();
    let active_crop = if active_page % 2 == 0 { odd } else { even };
    assert_eq!(reading(&reopened).preferences().crop, Some(active_crop));
    let expected_scale = WIDTH as f32 / (800.0 * (active_crop.y1 - active_crop.y0)) * 1.25;
    assert!((viewport.scale - expected_scale).abs() < 0.001);
    reopened.activate(Action::FixedScroll(false)).unwrap();
    assert_eq!(reading(&reopened).preferences().zoom, 1);
    assert_eq!(reading(&reopened).preferences().crop, None);
    reopened.activate(Action::FixedScroll(true)).unwrap();
    assert_eq!(
        reading(&reopened).preferences().continuous_crop,
        Some(ContinuousCrop::Manual)
    );
    let active_crop = if reading(&reopened).page() % 2 == 0 {
        odd
    } else {
        even
    };
    assert_eq!(reading(&reopened).preferences().crop, Some(active_crop));
    reopened.activate(Action::StraightenScan(5)).unwrap();
    reopened.activate(Action::CropPage).unwrap();
    assert_eq!(reading(&reopened).preferences().straighten_limit, 0);
    assert_eq!(reopened.input(tap(RESET)), Change::Partial);
    let reset = reading(&reopened).preferences();
    assert_eq!(reset.continuous_crop, None);
    assert_eq!(reset.crop, None);
    assert!(reset.continuous);
    assert_eq!((reset.zoom, reset.turns, reset.straighten_limit), (1, 1, 5));
    reopened.activate(Action::CropPage).unwrap();
    let draft = reopened.semi_auto_crop.unwrap();
    let expected_seed = if reading(&reopened).page() % 2 == 0 {
        odd
    } else {
        even
    };
    assert_eq!(draft.seed.normalized(draft.size), Some(expected_seed));
}

#[test]
fn crop_margin_adjusts_independent_strips_and_retains_source_annotations() {
    let paths = Fixture::new();
    let document = paths.root.join("Margin calibration.pdf");
    fs::write(&document, authored_pdf()).unwrap();
    let fonts = vec![
        crate::test_fixture("fonts/NotoSans-Regular.ttf"),
        crate::test_fixture("fonts/NotoSans-Bold.ttf"),
    ];
    let mut app = Reader::open(&paths.root, &paths.state, fonts.clone()).unwrap();
    app.open_path(&document).unwrap();
    assert_eq!(reading(&app).preferences().crop_margin(), 10);
    let full_page = reading(&app).page_viewport();
    app.activate(Action::SetCropMargin(55)).unwrap();
    assert_eq!(reading(&app).page_viewport(), full_page);
    assert!(reading(&app).preferences().crop.is_none());
    app.activate(Action::SetCropMargin(10)).unwrap();

    app.activate(Action::FixedScroll(true)).unwrap();
    app.activate(Action::FixedGap(24)).unwrap();
    let odd = bounds(0.13, 0.05, 0.67, 0.95);
    let even = bounds(0.47, 0.05, 0.88, 0.95);
    apply_seed(&mut app, odd);
    let first = reading(&app).preferences().crop.unwrap();
    app.activate(Action::FixedPage(1)).unwrap();
    apply_seed(&mut app, even);
    app.activate(Action::FixedPage(0)).unwrap();
    let first_height =
        (first.y1 - first.y0) * 800.0 * reading(&app).page_viewport().unwrap().1.scale;
    for delta in [1200.0, first_height - 1200.0 - 465.0] {
        app.input(Input::Swipe {
            dx: 0.0,
            dy: -delta,
            start_y: 1000.0,
        });
        app.tick(Instant::now() + Duration::from_secs(1));
    }
    assert_eq!(reading(&app).visible_pages().collect::<Vec<_>>(), [0, 1]);
    let source_before = reading(&app).history_point().unwrap();
    let first_before = reading(&app).page_viewport().unwrap().1.scale;

    app.open_reading_edge(false, 520.0);
    if app.reading_panel != Some(BarPanel::Scan) {
        app.input(tap(app.reading_button("Scan")));
    }
    assert_eq!(app.reading_panel, Some(BarPanel::Scan));
    let changed = app.reading_button_in_row("Margin · 0.10 in", "0.55");
    assert_eq!(app.input(tap(changed)), Change::Partial);
    assert_eq!(reading(&app).visible_pages().collect::<Vec<_>>(), [0, 1]);
    assert!(reading(&app).page_viewport().unwrap().1.scale < first_before);
    assert!((reading(&app).history_point().unwrap().y - source_before.y).abs() < 0.03);
    capture_test_screen(&app, &fonts, "READER_CROP_MARGIN_FRAME", false);

    let custom = app.reading_button_in_row("Margin · 0.55 in", "Custom");
    assert_eq!(app.input(tap(custom)), Change::Partial);
    assert_eq!(app.screen, Screen::NumberingValue(NumberField::CropMargin));
    assert_eq!(app.editor.string(), "0.55");
    app.editor.text = "1.501".chars().collect();
    assert!(app.apply_numbering_input(NumberField::CropMargin).is_err());
    assert_eq!(reading(&app).preferences().crop_margin(), 55);
    app.editor.text = "0.37".chars().collect();
    app.apply_numbering_input(NumberField::CropMargin).unwrap();
    assert_eq!(app.screen, Screen::Fixed);
    assert_eq!(reading(&app).preferences().crop_margin(), 37);
    assert_eq!(reading(&app).visible_pages().collect::<Vec<_>>(), [0, 1]);
    capture_test_screen(&app, &fonts, "READER_CROP_MARGIN_STRIPS", false);

    let retained = bounds(0.69, 0.22, 0.76, 0.24);
    let clipped = bounds(0.075, 0.1675, 0.115, 0.188);
    let active = app.fixed.as_ref().unwrap();
    app.library
        .remember(&active.entry, active.id, &active.entry.title)
        .pdf_highlights
        .push(PdfHighlight {
            chapter_title: None,
            created_at: None,
            style: Default::default(),
            color: Default::default(),
            page: 1,
            bounds: vec![retained],
            text: "Source note".into(),
            note: Some("Eastern basin".into()),
            continuation: Vec::new(),
        });
    let saved_view = reading(&app).preferences();
    app.save().unwrap();
    drop(app);
    let mut reopened = Reader::open(&paths.root, &paths.state, fonts).unwrap();
    reopened.open_path(&document).unwrap();
    assert_eq!(reading(&reopened).preferences(), saved_view);
    assert_eq!(
        reading(&reopened).visible_pages().collect::<Vec<_>>(),
        [0, 1]
    );
    assert!(reading(&reopened).visible_rectangle(1, clipped).is_none());
    let note = reading(&reopened)
        .visible_rectangle(1, retained)
        .unwrap()
        .center();
    reopened.input(Input::Tap(note.x as f32, note.y as f32));
    assert!(reopened.saved_popup.take().is_some());
    let links = reading(&reopened).visible_links();
    assert_eq!(links.len(), 1);
    let link = reading(&reopened)
        .visible_rectangle(1, links[0].bounds)
        .unwrap()
        .center();
    reopened.input(Input::Tap(link.x as f32, link.y as f32));
    assert_eq!(reading(&reopened).page(), 2);
    reopened.activate(Action::ReturnToLocation).unwrap();

    reopened.activate(Action::CropPage).unwrap();
    let draft = reopened.semi_auto_crop.as_mut().unwrap();
    draft.seed = Seed::from_normalized(odd, draft.size);
    reopened.input(tap(APPLY));
    let manual = reading(&reopened).page_viewport();
    reopened.activate(Action::SetCropMargin(100)).unwrap();
    assert_eq!(reading(&reopened).page_viewport(), manual);
    assert_eq!(reading(&reopened).preferences().crop, Some(odd));
    assert_eq!(reading(&reopened).preferences().crop_margin(), 100);
}
