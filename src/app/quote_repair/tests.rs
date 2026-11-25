use super::*;
use crate::app::tests::Fixture;
use crate::epub::Book;
use crate::{Engine, Probe, user_hyphenation};
use primitive_app::App;
use primitive_ui::Input;
use std::io::{Cursor, Read, Write};
use std::sync::Arc;

fn deep_jump(fixture: &Fixture) -> Reader {
    let bytes = crate::session::quote_prefixes::tests::epub("open-quote", 82, 4096);
    std::fs::write(&fixture.book, bytes).unwrap();
    let mut app = fixture.app();
    app.fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    app.open_path(&fixture.book).unwrap();
    app.active.as_mut().unwrap().reading.jump(75, None).unwrap();
    assert!(
        app.active
            .as_ref()
            .unwrap()
            .reading
            .quote_marker()
            .is_some()
    );
    app
}

fn long_scroll_jump(fixture: &Fixture) -> Reader {
    let bytes = crate::session::quote_prefixes::tests::epub("open-quote", 82, 4096);
    let mut input = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut output = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for index in 0..input.len() {
        let mut entry = input.by_index(index).unwrap();
        let name = entry.name().to_owned();
        let mut data = Vec::new();
        entry.read_to_end(&mut data).unwrap();
        if name == "chapter-75.xhtml" {
            let text = String::from_utf8(data).unwrap();
            data = text
                .replace(
                    "</body>",
                    &format!("<p>{}</p></body>", "Long reading line ".repeat(900)),
                )
                .into_bytes();
        }
        output
            .start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        output.write_all(&data).unwrap();
    }
    std::fs::write(&fixture.book, output.finish().unwrap().into_inner()).unwrap();
    let mut app = fixture.app();
    app.fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    app.open_path(&fixture.book).unwrap();
    app.activate(crate::app::Action::ReadingMode(true)).unwrap();
    app.active.as_mut().unwrap().reading.jump(75, None).unwrap();
    assert!(
        app.active
            .as_ref()
            .unwrap()
            .reading
            .quote_marker()
            .is_some()
    );
    app
}

fn start(app: &mut Reader) {
    let now = Instant::now();
    app.tick(now);
    app.tick(now + FIRST_FRAME_DELAY + Duration::from_millis(1));
    assert!(app.quote_repair.running.is_some());
}

fn wait_ready(app: &mut Reader) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while app.quote_repair.ready.is_none() {
        assert!(Instant::now() < deadline, "quote worker did not finish");
        app.tick(Instant::now() + Duration::from_secs(2));
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn quote_repair_deep_jump_waits_for_selection_and_preserves_anchor() {
    let fixture = Fixture::new();
    let mut app = deep_jump(&fixture);
    let active = app.active.as_mut().unwrap();
    let anchor = active.reading.anchor().unwrap();
    let first = active.reading.probe.matches("Quoted chapter 75").remove(0);
    let mut end = first.clone();
    end.offset += "Quoted".len();
    let selection = active.reading.probe.select_range(&first, &end).unwrap();
    let committed = active.reading.quote_marker().unwrap().clone();
    let _preview = active.reading.config.chapter_layout(74).unwrap();
    assert!(active.reading.quote_marker() == Some(&committed));
    start(&mut app);
    app.selection = Some(selection);
    wait_ready(&mut app);
    assert!(app.quote_repair_deadline().is_none());
    assert_eq!(
        app.active.as_ref().unwrap().reading.anchor(),
        Some(anchor.clone())
    );

    app.selection = None;
    app.reading_top = true;
    app.tick(Instant::now() + Duration::from_secs(2));
    assert!(app.quote_repair.ready.is_some());
    assert!(app.quote_repair_deadline().is_none());
    app.reading_top = false;
    assert_eq!(
        app.tick(Instant::now() + Duration::from_secs(2)),
        Change::Partial
    );
    let reading = &mut app.active.as_mut().unwrap().reading;
    assert!(reading.quote_marker().is_none());
    assert_eq!(reading.anchor(), Some(anchor.clone()));

    let bytes: Arc<[u8]> = std::fs::read(&fixture.book).unwrap().into();
    let mut expected = Probe::from_book_with_rules(
        Book::open_snapshot(bytes).unwrap(),
        Engine::Parley,
        &app.fonts,
        reading.probe.settings,
        75,
        None,
        &user_hyphenation::Rules::default(),
        &reading
            .config
            .user_styles
            .for_note_layout(true)
            .for_nonlinear_notes(false),
    )
    .unwrap();
    let expected_page = expected.page_for(&anchor).unwrap();
    assert_eq!(
        reading.probe.capture(reading.page).unwrap(),
        expected.capture(expected_page).unwrap()
    );
}

#[test]
fn quote_repair_sleep_cancels_old_worker_and_retries_after_wake() {
    let fixture = Fixture::new();
    let mut app = deep_jump(&fixture);
    let anchor = app.active.as_ref().unwrap().reading.anchor().unwrap();
    start(&mut app);
    let epoch = app.quote_repair.epoch;
    app.end_dialog = Some(crate::app::end_of_book::Dialog::Actions);
    app.input(Input::Sleep);
    assert_ne!(app.quote_repair.epoch, epoch);
    assert!(!app.stats_awake);
    assert!(app.quote_repair.sleeping);
    while app.quote_repair.running.is_some() {
        app.tick(Instant::now() + Duration::from_secs(2));
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(app.quote_repair.ready.is_none());
    // Even if a modal intercepts Sleep before the general awake flag is
    // updated, the independent worker gate must not schedule another job.
    app.stats_awake = true;
    assert!(app.quote_repair_deadline().is_none());
    app.tick(Instant::now() + Duration::from_secs(2));
    assert!(app.quote_repair.running.is_none());
    app.stats_awake = false;
    app.end_dialog = None;
    app.input(Input::Wake);
    assert!(!app.quote_repair.sleeping);
    let deadline = Instant::now() + Duration::from_secs(15);
    while app
        .active
        .as_ref()
        .unwrap()
        .reading
        .quote_marker()
        .is_some()
    {
        assert!(Instant::now() < deadline);
        app.tick(Instant::now() + Duration::from_secs(2));
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(app.active.as_ref().unwrap().reading.anchor(), Some(anchor));
}

#[test]
fn quote_repair_works_while_touch_is_locked() {
    let fixture = Fixture::new();
    let mut app = deep_jump(&fixture);
    let anchor = app.active.as_ref().unwrap().reading.anchor().unwrap();
    app.touch_locked = true;
    let deadline = Instant::now() + Duration::from_secs(15);
    while app
        .active
        .as_ref()
        .unwrap()
        .reading
        .quote_marker()
        .is_some()
    {
        assert!(Instant::now() < deadline);
        app.tick(Instant::now() + Duration::from_secs(2));
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(app.touch_locked);
    assert_eq!(app.active.as_ref().unwrap().reading.anchor(), Some(anchor));
}

#[test]
fn quote_repair_defers_during_a_real_scroll_pan_and_preserves_its_anchor() {
    use primitive_app::{Lifecycle, PanPhase, TouchPan};
    let fixture = Fixture::new();
    let mut app = long_scroll_jump(&fixture);
    app.controls.menu_activation = crate::reading_controls::MenuActivation::Tap;
    app.controls.scroll_delay_ms = Some(0);
    start(&mut app);
    let mut lifecycle = Lifecycle::default();
    let point = (620.0, 750.0);
    let movement = |dy, phase| TouchPan {
        start: point,
        end: (point.0, point.1 + dy),
        elapsed: Duration::from_millis(200),
        phase,
    };
    let fail = |error| panic!("unexpected storage error: {error}");
    lifecycle.pan(&mut app, movement(-120.0, PanPhase::Move), fail);
    assert!(app.scroll_pan.busy());
    assert!(
        app.active
            .as_ref()
            .unwrap()
            .reading
            .scroll_history_top()
            .unwrap()
            > 0.0
    );
    wait_ready(&mut app);
    assert!(app.quote_repair_deadline().is_none());
    lifecycle.pan(&mut app, movement(-160.0, PanPhase::Release), fail);
    assert!(!app.scroll_pan.busy());
    let anchor = app.active.as_ref().unwrap().reading.anchor().unwrap();
    app.tick(Instant::now() + Duration::from_secs(2));
    let reading = &app.active.as_ref().unwrap().reading;
    assert!(reading.quote_marker().is_none());
    assert_eq!(reading.anchor(), Some(anchor));
    assert!(reading.scroll_history_top().unwrap() > 0.0);
}

#[test]
fn quote_repair_never_caches_a_provisional_chapter_as_eager_layout() {
    let fixture = Fixture::new();
    let mut app = deep_jump(&fixture);
    let reading = &mut app.active.as_mut().unwrap().reading;
    reading.config.eager_layouts = crate::session::screen_pages::eager::Cached::empty_for_test(
        &reading.config,
        reading.probe.chapter_count(),
    );
    assert!(reading.quote_marker().is_some());
    assert!(reading.whole_book_screen_pages().is_none());
    reading.jump(74, None).unwrap();
    assert!(!reading.config.eager_layouts.contains(75));
    reading.jump(75, None).unwrap();
    assert!(reading.quote_marker().is_some());
}

#[test]
fn quote_repair_usb_return_rejects_a_replaced_edition() {
    let fixture = Fixture::new();
    let mut app = deep_jump(&fixture);
    start(&mut app);
    let epoch = app.quote_repair.epoch;
    app.save().unwrap();
    assert_ne!(app.quote_repair.epoch, epoch);
    let before = std::fs::read(&fixture.book).unwrap();
    std::fs::write(&fixture.book, &before).unwrap();
    app.storage_returned().unwrap();
    assert!(
        app.active
            .as_ref()
            .unwrap()
            .reading
            .quote_marker()
            .is_none()
    );
    while app.quote_repair.running.is_some() {
        app.tick(Instant::now() + Duration::from_secs(2));
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(app.quote_repair.ready.is_none());
    assert_eq!(std::fs::read(&fixture.book).unwrap(), before);
}

#[test]
fn quote_repair_go_to_document_page_opens_and_repairs_a_provisional_target() {
    let fixture = Fixture::new();
    std::fs::write(
        &fixture.book,
        crate::session::quote_prefixes::tests::epub("open-quote", 82, 4096),
    )
    .unwrap();
    let mut app = fixture.app();
    app.fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    app.open_path(&fixture.book).unwrap();
    app.show(Screen::GoToPage);
    app.editor.text = "76".chars().collect();
    app.jump_reflow_number().unwrap();
    assert!(app.screen_page_jump.is_some());
    let deadline = Instant::now() + Duration::from_secs(15);
    while app.screen_page_jump.is_some() {
        assert!(Instant::now() < deadline);
        app.tick(Instant::now() + Duration::from_secs(2));
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(app.screen, Screen::Book);
    assert_eq!(app.active.as_ref().unwrap().reading.chapter, 75);
    assert!(
        app.active
            .as_ref()
            .unwrap()
            .reading
            .quote_marker()
            .is_some()
    );
    assert_eq!(app.notice.as_deref(), Some(APPROXIMATE_PAGE_NOTICE));
    let anchor = app.active.as_ref().unwrap().reading.anchor().unwrap();
    while app
        .active
        .as_ref()
        .unwrap()
        .reading
        .quote_marker()
        .is_some()
    {
        assert!(Instant::now() < deadline);
        app.tick(Instant::now() + Duration::from_secs(2));
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(app.active.as_ref().unwrap().reading.anchor(), Some(anchor));
    assert_eq!(app.notice.as_deref(), Some(COMPLETED_PAGE_NOTICE));
    app.input(Input::PageForward);
    if app.active.as_ref().unwrap().reading.chapter == 75 {
        // E-ink pacing may queue the turn; the warning must remain until it
        // actually completes, not disappear on the button press alone.
        assert_eq!(app.notice.as_deref(), Some(COMPLETED_PAGE_NOTICE));
        app.tick(Instant::now() + Duration::from_secs(2));
    }
    assert_eq!(app.active.as_ref().unwrap().reading.chapter, 76);
    assert!(app.notice.is_none());
}

#[test]
fn quote_repair_go_to_relative_page_works_while_current_layout_is_provisional() {
    let fixture = Fixture::new();
    let mut app = long_scroll_jump(&fixture);
    app.activate(crate::app::Action::ReadingMode(false))
        .unwrap();
    let reading = &app.active.as_ref().unwrap().reading;
    assert!(reading.quote_marker().is_some());
    assert!(reading.probe.page_count() > 1);
    assert_eq!(reading.page, 0);
    app.show(Screen::GoToPage);
    app.editor.text = "+1".chars().collect();
    app.jump_reflow_number().unwrap();
    let reading = &app.active.as_ref().unwrap().reading;
    assert_eq!(app.screen, Screen::Book);
    assert_eq!(reading.page, 1);
    assert!(reading.quote_marker().is_some());
}

#[test]
fn quote_repair_failed_preparation_reports_approximate_page_without_retrying() {
    let fixture = Fixture::new();
    let mut app = deep_jump(&fixture);
    app.notice = Some(APPROXIMATE_PAGE_NOTICE.into());
    std::fs::OpenOptions::new()
        .write(true)
        .open(&fixture.book)
        .unwrap()
        .set_len(crate::session::screen_pages::background::ARCHIVE_LIMIT + 1)
        .unwrap();
    let now = Instant::now();
    app.tick(now);
    app.tick(now + FIRST_FRAME_DELAY + Duration::from_millis(1));
    assert_eq!(app.notice.as_deref(), Some(UNAVAILABLE_PAGE_NOTICE));
    assert!(app.quote_repair.rejected.is_some());
    assert!(app.quote_repair_deadline().is_none());
}

#[test]
fn quote_repair_final_notices_fit_a_narrow_footer_at_normal_and_large_sizes() {
    let fixture = Fixture::new();
    let mut app = deep_jump(&fixture);
    app.notice = None;
    let viewport = app.reading_viewport();
    let (width, height) = viewport.dimensions();
    let fonts: Vec<_> = app
        .fonts
        .iter()
        .map(|path| path.display().to_string())
        .collect();
    let mut canvas = primitive_ui::Canvas::new_viewport(width, height, width, &fonts).unwrap();
    // Extend the reference below the physical screen so ink clipped by the
    // screen edge is caught as well as ink clipped by the caption rectangle.
    let reference_height = height + 128;
    let mut reference =
        primitive_ui::Canvas::new_viewport(width, reference_height, width, &fonts).unwrap();
    let chapter_caption = app.active.as_ref().unwrap().reading.footer_caption(None);
    assert!(chapter_caption.contains("Chapter ") && chapter_caption.contains(" - Page "));
    assert!(app.footer_track_rect(200).is_some());
    assert!(viewport.done().right() <= width as i32);
    // This deliberately uses a 650-pixel footer with unscaled reference-size
    // type, which is stricter than scaling both together on a small screen.
    let narrow_slot = 650 - 154 - 60;
    for (caption_size, suffix) in [(14, "normal"), (36, "large")] {
        app.controls.footer_item_font_size = Some(caption_size);
        app.notice = None;
        let font_size = app.footer_caption_font_px();
        let slot = viewport.done().x.saturating_sub(60) as u32;
        let ordinary = app.footer_caption_rect(slot, 200, font_size);
        assert_eq!(ordinary.h, 28, "ordinary footer geometry changed");
        let without_notice = primitive_app::frame(&app, &mut canvas, false);
        for (notice, label) in [
            (COMPLETED_PAGE_NOTICE, "completed"),
            (UNAVAILABLE_PAGE_NOTICE, "unavailable"),
        ] {
            app.notice = Some(notice.into());
            let measured = canvas.measure_with_weight(notice, font_size, false);
            assert!(measured <= narrow_slot, "{notice:?} needs {measured}px");
            let rect = app.footer_caption_rect(
                app.footer_caption_width(slot, measured),
                measured,
                font_size,
            );
            assert_eq!(rect.w, measured, "the actual footer clips {notice:?}");
            let frame = primitive_app::frame(&app, &mut canvas, false);
            let footer_start = viewport.page_height() as usize * width as usize;
            assert!(
                frame[footer_start..]
                    .iter()
                    .zip(&without_notice[footer_start..])
                    .any(|(a, b)| a != b)
            );
            // Draw the same glyphs without the footer's height clip. Every
            // ink pixel must survive in the real app frame, including descenders.
            reference.begin();
            reference.text(
                primitive_ui::Rect::new(rect.x, rect.y, rect.w, reference_height as i32 - rect.y),
                notice,
                app.footer_caption_style(),
            );
            let expected = reference.finish(false);
            let mut ink = 0;
            for y in rect.y.max(0)..reference_height as i32 {
                for x in rect.x.max(0)..rect.right().min(width as i32) {
                    let index = y as usize * width as usize + x as usize;
                    if expected[index] != 255 {
                        ink += 1;
                        assert!(
                            y < height as i32,
                            "{notice:?} exceeds the screen at ({x}, {y})"
                        );
                        assert_eq!(
                            frame[index], expected[index],
                            "{notice:?} clipped at ({x}, {y})"
                        );
                    }
                }
            }
            assert!(ink > 0, "reference text did not paint");
            if let Some(dir) = std::env::var_os("READER_SELECTION_VIEWPORT_CAPTURES") {
                std::fs::create_dir_all(&dir).unwrap();
                let image = image::GrayImage::from_raw(width, height, frame).unwrap();
                let narrow_height = ((height as u64 * 650) / width as u64) as u32;
                let name = if suffix == "large" {
                    format!("quote-repair-{label}-narrow.png")
                } else {
                    format!("quote-repair-{label}-{suffix}-narrow.png")
                };
                image::imageops::resize(
                    &image,
                    650,
                    narrow_height,
                    image::imageops::FilterType::Triangle,
                )
                .save(std::path::Path::new(&dir).join(name))
                .unwrap();
            }
        }
    }
}
