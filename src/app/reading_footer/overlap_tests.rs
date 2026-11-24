use super::*;
use crate::app::tests::{Fixture, capture_test_screen, tap_contents_item};
use crate::reading_controls::StatusBar;
use crate::session::{DONE, PAGE_HEIGHT, SHOW_STATUS};
use primitive_app::{App, Change, HEIGHT, WIDTH};
use primitive_ui::{Canvas, Input, Rect};
use std::path::PathBuf;

fn center(rect: Rect) -> Input {
    Input::Tap((rect.x + rect.w / 2) as f32, (rect.y + rect.h / 2) as f32)
}

fn settings(app: &mut Reader) {
    app.open_reading_edge(true, 30.0);
    app.input(center(app.reading_button("Controls")));
    assert_eq!(app.screen, Screen::ReadingControls);
    tap_contents_item(app, "Reading status bar");
    assert_eq!(app.screen, Screen::FooterSettings);
}

fn status(app: &mut Reader, mode: StatusBar) {
    for _ in 0..12 {
        if app.controls.status_bar == mode {
            return;
        }
        tap_contents_item(app, "Status text");
    }
    panic!("Status mode unavailable")
}

fn return_to_page(app: &mut Reader) {
    let back = center(primitive_ui::back_button(WIDTH as i32));
    app.input(back);
    app.input(back);
    let reading = if app.fixed.is_some() {
        Screen::Fixed
    } else {
        Screen::Book
    };
    for _ in 0..3 {
        if app.screen == reading {
            return;
        }
        app.input(Input::Tap(630., 700.));
    }
    assert_eq!(app.screen, reading);
}

fn frame(app: &Reader, fonts: &[PathBuf]) -> Vec<u8> {
    let files = fonts
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>();
    let mut c = Canvas::new(WIDTH, HEIGHT, &files).unwrap();
    primitive_app::frame(app, &mut c, false)
}

#[test]
fn reads_and_selects_below_the_old_footer_without_repaginating_when_status_is_revealed() {
    let fixture = Fixture::new();
    let external = std::env::var_os("READER_OVERLAP_BOOK").map(PathBuf::from);
    let authored = external.is_none();
    let path = external.unwrap_or_else(|| {
        let path = fixture.root.join("Coastal observations.epub");
        std::fs::write(
            &path,
            include_bytes!("../../../tests/fixtures/long-chapter.epub"),
        )
        .unwrap();
        path
    });
    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let chapter = std::env::var("READER_OVERLAP_CHAPTER")
        .ok()
        .map_or(0, |v| v.parse::<usize>().unwrap() - 1);
    let mut app = Reader::open(path.parent().unwrap(), &fixture.state, fonts.clone()).unwrap();
    app.controls.dictionary_on_single_word = Some(false);
    app.open_path(&path).unwrap();
    let s = app.active.as_ref().unwrap().reading.config.settings;
    // Make the newly exposed text band reachable even on a short line.
    app.activate(Action::Text(crate::Settings {
        margin_bottom: Some(0),
        ..s
    }))
    .unwrap();
    app.active
        .as_mut()
        .unwrap()
        .reading
        .go_to_page(chapter, 0)
        .unwrap();
    app.record_position();
    assert_eq!(app.page_paint().visible, 0..PAGE_HEIGHT);
    let old = app.active.as_ref().unwrap().reading.probe.page_count();
    let start = app.current_position().unwrap().anchor.unwrap();
    settings(&mut app);
    tap_contents_item(&mut app, "Overlap status bar");
    status(&mut app, StatusBar::Minimal);
    return_to_page(&mut app);
    assert_eq!(app.page_paint().visible, 0..HEIGHT);
    let b = &mut app.active.as_mut().unwrap().reading;
    assert_eq!(b.probe.page_for(&start), Some(b.page));
    assert_eq!(b.probe.settings.height, HEIGHT);
    let count = b.probe.page_count();
    assert!(count <= old);
    let (target, x, y, w) = (0..count.min(40))
        .find_map(|i| {
            let pixels = b.probe.capture(i).unwrap();
            for y in ((PAGE_HEIGHT + 2)..HEIGHT - 1).step_by(3) {
                for x in (170..WIDTH - 170).step_by(4) {
                    if pixels[(y * WIDTH + x) as usize] >= 160 {
                        continue;
                    }
                    if let Some(w) = b.probe.word_at(i, x as f32, y as f32)
                        && w.text.chars().filter(|c| c.is_alphanumeric()).count() >= 3
                    {
                        return Some((i, x, y, w));
                    }
                }
            }
            None
        })
        .expect("A word must be selectable in the reclaimed area");
    let b = &mut app.active.as_mut().unwrap().reading;
    b.go_to_page(chapter, target).unwrap();
    let raw = b.probe.capture(target).unwrap();
    let rectangles = b.probe.selection_rects(target, &w);
    assert!(
        !rectangles.is_empty()
            && rectangles
                .iter()
                .any(|r| r.y + r.height > PAGE_HEIGHT as i32)
    );
    app.record_position();
    let shown = frame(&app, &fonts);
    let mut ink = 0;
    let mut compared = 0;
    for y in PAGE_HEIGHT..HEIGHT {
        for x in 0..WIDTH {
            if SHOW_STATUS.contains(x as f32, y as f32) || DONE.contains(x as f32, y as f32) {
                continue;
            }
            let i = (y * WIDTH + x) as usize;
            assert_eq!(shown[i], raw[i], "Unexpected overlay at {x},{y}");
            compared += 1;
            ink += usize::from(raw[i] < 255);
        }
    }
    assert!(ink > 10);
    for control in [SHOW_STATUS, DONE] {
        let visible_control_pixels = (control.y..control.y + control.h)
            .flat_map(|y| {
                (control.x..control.x + control.w).map(move |x| (y * WIDTH as i32 + x) as usize)
            })
            .filter(|&index| shown[index] < 160 && shown[index] != raw[index])
            .count();
        assert!(
            visible_control_pixels > 100,
            "Control must be visible on the page"
        );
    }
    if authored {
        capture_test_screen(&app, &fonts, "READER_OVERLAP_HIDDEN_IMAGE", false);
    }
    app.input(Input::LongTap(x as f32, y as f32));
    assert!(app.selection.as_ref().is_some_and(|s| s == &w));
    let cancel = app.selection_button(crate::app::selection_popup::SelectionAction::Cancel);
    app.input(center(cancel));
    assert!(app.selection.is_none());
    if let Some(file) = std::env::var_os("READER_OVERLAP_PASSAGES") {
        let probe = &app.active.as_ref().unwrap().reading.probe;
        let range = probe.pages.range(target).unwrap();
        let mut rows = Vec::new();
        for line in probe
            .lines
            .iter()
            .filter(|l| l.top < range.end && l.bottom > range.start)
            .rev()
        {
            let i = probe
                .source
                .blocks
                .binary_search_by_key(&line.block, |b| b.key)
                .unwrap();
            let words: Vec<_> = probe.source.blocks[i].text[line.bytes.clone()]
                .split_whitespace()
                .take(12)
                .collect();
            if words.len() >= 4 {
                rows.push(words.join(" "));
            }
        }
        assert!(!rows.is_empty());
        std::fs::write(file, rows.join("\n")).unwrap();
    }
    let snapshot = app.current_position().unwrap();
    let allocation = app.active.as_ref().unwrap().reading.probe.lines.as_ptr();
    assert_ne!(app.input(center(SHOW_STATUS)), Change::None);
    assert_eq!(app.controls.status_bar, StatusBar::PageNumbers);
    let expanded = frame(&app, &fonts);
    assert!(
        expanded != shown,
        "Showing the status bar must change the framebuffer"
    );
    assert!(
        expanded[(PAGE_HEIGHT * WIDTH) as usize..]
            .iter()
            .zip(&shown[(PAGE_HEIGHT * WIDTH) as usize..])
            .filter(|(visible, hidden)| **visible == 255 && **hidden < 200)
            .count()
            > 100
    );
    assert!(app.active.as_ref().unwrap().reading.probe.lines.as_ptr() == allocation);
    assert_eq!(app.current_position().unwrap(), snapshot);
    if authored {
        capture_test_screen(&app, &fonts, "READER_OVERLAP_VISIBLE_IMAGE", false);
    }
    app.input(Input::LongTap(x as f32, y as f32));
    assert!(app.selection.is_none());
    if app.screen != Screen::Book {
        app.show(Screen::Book);
    }
    assert_eq!(app.input(center(DONE)), Change::Exit);
    drop(app);
    let mut reopened = Reader::open(path.parent().unwrap(), &fixture.state, fonts).unwrap();
    reopened.open_path(&path).unwrap();
    assert!(reopened.controls.overlap_footer);
    let b = &reopened.active.as_ref().unwrap().reading;
    assert_eq!(b.probe.settings.height, HEIGHT);
    assert_eq!(
        b.probe.page_for(snapshot.anchor.as_ref().unwrap()),
        Some(b.page)
    );
    settings(&mut reopened);
    tap_contents_item(&mut reopened, "Overlap status bar");
    return_to_page(&mut reopened);
    assert_eq!(
        reopened
            .active
            .as_ref()
            .unwrap()
            .reading
            .probe
            .settings
            .height,
        PAGE_HEIGHT
    );
    if let Some(p) = std::env::var_os("READER_OVERLAP_REPORT") {
        let data = serde_json::json!({"old_height":PAGE_HEIGHT,"full_height":HEIGHT,"old_chapter_pages":old,"full_chapter_pages":count,"extra_pixels_compared":compared,"extra_ink":ink,"source_word_bytes":w.text.len(),"location_one_based":target+1,"unchanged_layout_on_reveal":true,"saved_and_restored":true});
        std::fs::write(p, serde_json::to_vec(&data).unwrap()).unwrap();
    }
}

#[test]
fn overlap_resizes_spreads_and_scroll_and_preserves_geometry_after_a_failed_save() {
    let fixture = Fixture::new();
    let path = fixture.root.join("Coastal observations.epub");
    std::fs::write(
        &path,
        include_bytes!("../../../tests/fixtures/long-chapter.epub"),
    )
    .unwrap();
    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
    app.open_path(&path).unwrap();
    let controls = crate::reading_controls::ReadingControls {
        overlap_footer: true,
        status_bar: StatusBar::Minimal,
        footer_tap_locked: true,
        ..app.controls
    };
    if app.controls_path.is_file() {
        std::fs::remove_file(&app.controls_path).unwrap();
    }
    std::fs::create_dir(&app.controls_path).unwrap();
    assert!(app.activate(Action::SetReadingControls(controls)).is_err());
    assert!(!app.controls.overlap_footer);
    assert_eq!(
        app.active.as_ref().unwrap().reading.config.settings.height,
        PAGE_HEIGHT
    );
    std::fs::remove_dir(&app.controls_path).unwrap();
    app.activate(Action::SetReadingControls(controls)).unwrap();
    assert_eq!(app.input(center(SHOW_STATUS)), Change::Partial);
    assert_eq!(app.screen, Screen::FooterSettings);
    app.show(Screen::Book);
    app.activate(Action::FacingPages(true)).unwrap();
    let b = &app.active.as_ref().unwrap().reading;
    assert!(b.uses_facing_pages());
    assert_eq!(b.probe.settings.height, HEIGHT);
    assert_eq!(frame(&app, &fonts).len(), (WIDTH * HEIGHT) as usize);
    let before = app.current_position().unwrap();
    app.input(Input::PageForward);
    if let Some(t) = app.active.as_ref().unwrap().reading.deadline() {
        app.tick(t);
    }
    let after = app.current_position().unwrap();
    assert_ne!(before.page, after.page);
    app.activate(Action::ReadingMode(true)).unwrap();
    assert!(app.active.as_ref().unwrap().reading.config.scroll);
    assert_eq!(
        app.active.as_ref().unwrap().reading.config.settings.height,
        HEIGHT
    );
    assert_eq!(frame(&app, &fonts).len(), (WIDTH * HEIGHT) as usize);
}

fn tide_chart() -> Vec<u8> {
    let mut page = String::from("BT /F 23 Tf 29 430 Td (Tide measurements) Tj ET\n");
    page.push_str("BT /F 11 Tf 30 407 Td (Field notebook  /  North cove) Tj ET\n");
    page.push_str("BT /F 12 Tf 30 363 Td (Water depth above the same rock ledge) Tj ET\n");
    for (i, (time, depth)) in [
        ("06:00", 20),
        ("07:00", 34),
        ("08:00", 52),
        ("09:00", 74),
        ("10:00", 67),
        ("11:00", 49),
    ]
    .iter()
    .enumerate()
    {
        let y = 314 - 33 * i;
        page.push_str(&format!("BT /F 11 Tf 30 {y} Td ({time}) Tj ET\n0.73 g 75 {y} {} 9 re f 0 g\nBT /F 11 Tf 255 {y} Td ({depth} cm) Tj ET\n", depth * 2));
    }
    page.push_str("0.5 w 30 119 m 325 119 l S\nBT /F 11 Tf 30 93 Td (The highest recorded level was 74 cm at 09:00.) Tj ET\n");
    page.push_str("BT /F 9 Tf 249 8 Td (Datum) Tj ET\n");
    crate::fixed::tests::pdf_objects(&[
        "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
        "<< /Type /Pages /Count 1 /Kids [3 0 R] >>".to_owned(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 360 480] /Resources << /Font << /F 5 0 R >> >> /Contents 4 0 R >>".to_owned(),
        format!("<< /Length {} >>\nstream\n{page}endstream", page.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_owned(),
    ])
}

#[test]
fn fixed_pages_fit_the_full_height_and_reveal_status_without_rescaling() {
    let fixture = Fixture::new();
    let original = std::env::var_os("READER_FIXED_OVERLAP_BOOK").map(PathBuf::from);
    let authored = original.is_none();
    let path = original.unwrap_or_else(|| {
        let p = fixture.root.join("Chart.pdf");
        std::fs::write(&p, tide_chart()).unwrap();
        p
    });
    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut app = Reader::open(path.parent().unwrap(), &fixture.state, fonts.clone()).unwrap();
    app.controls.dictionary_on_single_word = Some(false);
    app.open_path(&path).unwrap();
    let page = std::env::var("READER_FIXED_OVERLAP_PAGE")
        .ok()
        .map_or(0, |v| v.parse::<usize>().unwrap() - 1);
    let reader = &mut app.fixed.as_mut().unwrap().reading;
    reader.jump(page).unwrap();
    reader.fit_height().unwrap();
    let (size, old) = reader.page_viewport().unwrap();
    assert_eq!(old.height, PAGE_HEIGHT);
    let baseline = frame(&app, &fonts);
    settings(&mut app);
    tap_contents_item(&mut app, "Overlap status bar");
    status(&mut app, StatusBar::Minimal);
    return_to_page(&mut app);
    let reader = &app.fixed.as_ref().unwrap().reading;
    let (_, full) = reader.page_viewport().unwrap();
    assert_eq!(reader.page(), page);
    assert_eq!(full.height, HEIGHT);
    assert!((full.scale / old.scale - HEIGHT as f32 / PAGE_HEIGHT as f32).abs() < 0.00001);
    assert!(app.hidden_overlap_footer());
    let hidden = frame(&app, &fonts);
    assert_ne!(baseline, hidden);
    if authored {
        capture_test_screen(&app, &fonts, "READER_FIXED_OVERLAP_IMAGE", false);
        // Click the authored word under the usual zoom buttons through the same
        // selection routing that handles physical long presses.
        let mut at = None;
        for y in (1618..1668).step_by(3) {
            for x in (850..955).step_by(6) {
                if let crate::fixed_session::WordAtTouch::Ready(Some(word)) =
                    reader.select_word(x as f32, y as f32).unwrap()
                    && word.text() == "Datum"
                {
                    at = Some((x as f32, y as f32));
                    break;
                }
            }
            if at.is_some() {
                break;
            }
        }
        let (x, y) = at.expect("The PDF word must occupy the exposed zoom-control area");
        app.input(Input::LongTap(x, y));
        assert_eq!(app.pdf_selection.as_ref().unwrap().text(), "Datum");
        let cancel = app.selection_button(crate::app::selection_popup::SelectionAction::Cancel);
        app.input(center(cancel));
        assert!(app.pdf_selection.is_none());
    }
    app.input(center(SHOW_STATUS));
    assert_eq!(app.controls.status_bar, StatusBar::PageNumbers);
    let shown = frame(&app, &fonts);
    let cut = (WIDTH * PAGE_HEIGHT) as usize;
    assert!(hidden[..cut] == shown[..cut] && hidden[cut..] != shown[cut..]);
    assert_eq!(
        app.fixed
            .as_ref()
            .unwrap()
            .reading
            .page_viewport()
            .unwrap()
            .1,
        full
    );
    let visible_zoom = app.fixed.as_ref().unwrap().reading.preferences().zoom;
    assert_eq!(app.input(Input::Tap(1030., 1642.)), Change::Partial);
    assert_eq!(
        app.fixed.as_ref().unwrap().reading.preferences().zoom,
        visible_zoom + 1
    );
    assert_eq!(app.input(Input::Tap(895., 1642.)), Change::Partial);
    assert_eq!(
        app.fixed.as_ref().unwrap().reading.preferences().zoom,
        visible_zoom
    );
    assert_eq!(app.input(center(DONE)), Change::Exit);
    drop(app);
    let mut reopened = Reader::open(path.parent().unwrap(), &fixture.state, fonts.clone()).unwrap();
    reopened.open_path(&path).unwrap();
    let reader = &reopened.fixed.as_ref().unwrap().reading;
    assert_eq!(reader.page(), page);
    assert_eq!(reader.page_viewport().unwrap().1, full);
    settings(&mut reopened);
    tap_contents_item(&mut reopened, "Overlap status bar");
    return_to_page(&mut reopened);
    assert_eq!(
        reopened
            .fixed
            .as_ref()
            .unwrap()
            .reading
            .page_viewport()
            .unwrap()
            .1,
        old
    );
    let reset = frame(&reopened, &fonts);
    assert!(baseline[..cut] == reset[..cut]);
    if let Some(p) = std::env::var_os("READER_FIXED_OVERLAP_REPORT") {
        let data = serde_json::json!({"page":page+1,"source_width":size.0,"source_height":size.1,
        "old_height":old.height,"full_height":full.height,"scale_ratio":full.scale / old.scale,
        "reveal_unchanged_pixels":cut,"reset_identical":true,"restored_page_and_view":true});
        std::fs::write(p, serde_json::to_vec(&data).unwrap()).unwrap();
    }
}

#[test]
fn exposed_saved_marks_open_while_visible_footer_buttons_stay_in_front() {
    let fixture = Fixture::new();
    let path = fixture.root.join("Tides.pdf");
    std::fs::write(&path, tide_chart()).unwrap();
    let mut app = Reader::open(
        &fixture.root,
        &fixture.state,
        vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")],
    )
    .unwrap();
    app.open_path(&path).unwrap();
    app.activate(Action::SetReadingControls(ReadingControls {
        overlap_footer: true,
        status_bar: StatusBar::Minimal,
        ..app.controls
    }))
    .unwrap();
    app.fixed.as_mut().unwrap().reading.fit_height().unwrap();
    let f = app.fixed.as_ref().unwrap();
    app.library
        .remember(&f.entry, f.id, &f.entry.title)
        .add_pdf_highlight(crate::library::PdfHighlight {
            updated_at: None,
            chapter_title: None,
            color: Default::default(),
            created_at: None,
            style: Default::default(),
            page: 0,
            bounds: vec![crate::fixed_text::PdfTextRect {
                x0: 0.01,
                y0: 0.95,
                x1: 0.99,
                y1: 0.995,
            }],
            text: "Source annotation in the lower band".into(),
            note: None,
            continuation: Vec::new(),
        })
        .unwrap();
    app.input(Input::Tap(480., 1640.));
    assert!(app.saved_popup_over_book());
    app.input(Input::Tap(500., 420.));
    assert!(!app.saved_popup_over_book());
    assert!(app.hidden_overlap_footer());
    app.input(center(SHOW_STATUS));
    assert_eq!(app.controls.status_bar, StatusBar::PageNumbers);
    assert!(!app.saved_popup_over_book());
    app.activate(Action::SetReadingControls(ReadingControls {
        status_bar: StatusBar::Minimal,
        ..app.controls
    }))
    .unwrap();
    assert_eq!(app.input(center(DONE)), Change::Exit);
}
