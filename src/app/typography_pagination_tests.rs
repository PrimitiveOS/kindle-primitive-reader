use super::*;
use crate::WordSpacePercent;
use crate::app::selection_viewport_tests::{adopt, capture, key, tap};
use crate::app::tests::{Fixture, write_outline_fixture};
use crate::session::ReadingViewport;
use primitive_ui::Key;
use std::time::Duration;

fn book(app: &Reader) -> &Reading {
    &app.active.as_ref().unwrap().reading
}

fn count(app: &Reader) -> Option<(usize, usize)> {
    book(app).whole_book_screen_pages()
}

fn typography_pending(app: &Reader) -> bool {
    book(app).config.screen_pages.typography_pending()
}

fn begin_count(app: &mut Reader) {
    assert!(app.screen_page_count_intent() == Some(ScreenPageIntent::Typography));
    let now = Instant::now();
    app.tick(now);
    app.tick(now + Duration::from_millis(2999));
    assert!(app.screen_page_worker.deadline(None).is_none());
    app.tick(now + Duration::from_secs(3));
    assert!(app.screen_page_worker.deadline(None).is_some());
}

fn wait_for_count(app: &mut Reader) -> (usize, usize) {
    let timeout = Instant::now() + Duration::from_secs(15);
    while count(app).is_none() {
        assert!(Instant::now() < timeout);
        app.tick(Instant::now());
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!typography_pending(app));
    count(app).unwrap()
}

fn drain(app: &mut Reader) {
    let timeout = Instant::now() + Duration::from_secs(8);
    while app.screen_page_worker.deadline(None).is_some() {
        assert!(Instant::now() < timeout);
        app.tick(Instant::now());
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn typography_pagination_builds_the_edited_epub_index_with_the_header_off() {
    let fixture = Fixture::new();
    let path = fixture.root.join("Shore notebook.epub");
    write_outline_fixture(&path, None, None);
    if let Some(stock) = std::env::var_os("READER_TYPOGRAPHY_STOCK_FIXTURE") {
        fs::copy(&path, stock).unwrap();
    }
    let mut app = fixture.app();
    app.fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    app.open_path(&path).unwrap();
    app.follow_reflow_page(0, 2).unwrap();
    let owner = app.active.as_ref().unwrap().id;
    let anchor = book(&app).anchor().unwrap();
    assert!(app.screen_page_count_intent().is_none());
    let baseline_total: usize = (0..book(&app).probe.chapter_count())
        .map(|chapter| book(&app).config.chapter(chapter).unwrap().page_count())
        .sum();
    let mut canvas = adopt(&mut app, ReadingViewport::PORTRAIT, 1);
    let settings = book(&app).config.settings;
    app.activate(Action::Text(Settings {
        font_gamma_index: 21,
        ..settings
    }))
    .unwrap();
    assert!(app.screen_page_count_intent().is_none());
    app.activate(Action::Text(Settings {
        smooth_images: !settings.smooth_images,
        ..book(&app).config.settings
    }))
    .unwrap();
    assert!(app.screen_page_count_intent().is_none());

    app.show(Screen::NumberingValue(NumberField::LogicalFont(owner)));
    app.editor.text = "36".chars().collect();
    app.editor.caret = app.editor.text.len();
    let set = key(&app, Key::Go, "Set");
    tap(&mut app, set);
    assert_eq!(book(&app).config.settings.logical_font_size, Some(36.0));
    assert!(typography_pending(&app));
    assert!(app.screen_page_count_intent().is_none());
    assert_eq!(book(&app).anchor().as_ref(), Some(&anchor));

    app.show(Screen::Book);
    let edited = primitive_app::frame(&app, &mut canvas, false);
    let caption = app.status_caption();
    let exact_total: usize = (0..book(&app).probe.chapter_count())
        .map(|chapter| book(&app).config.chapter(chapter).unwrap().page_count())
        .sum();
    assert_ne!(baseline_total, exact_total);
    begin_count(&mut app);
    let (ordinal, total) = wait_for_count(&mut app);
    assert_eq!(total, exact_total);
    assert_eq!(book(&app).anchor().as_ref(), Some(&anchor));
    assert_eq!(primitive_app::frame(&app, &mut canvas, false), edited);
    assert_eq!(app.status_caption(), caption);
    capture(
        &app,
        &mut canvas,
        "typography-pagination-edited-epub.png",
        false,
    );

    app.show(Screen::GoToPage);
    capture(
        &app,
        &mut canvas,
        "typography-pagination-completed-jump.png",
        false,
    );
    let target = if ordinal == total { 1 } else { total };
    app.editor.text = target.to_string().chars().collect();
    app.jump_reflow_number().unwrap();
    assert_eq!(app.screen, Screen::Book);
    assert_eq!(count(&app), Some((target, total)));
    assert!(app.screen_page_count_intent().is_none());
    app.activate(Action::ReturnToLocation).unwrap();
    assert_eq!(book(&app).anchor().as_ref(), Some(&anchor));
}

#[test]
fn typography_pagination_replaces_generations_and_cancels_through_sleep_and_usb() {
    let fixture = Fixture::new();
    let path = fixture.root.join("Many Chapters.epub");
    fs::copy(crate::test_fixture("many-chapters.epub"), &path).unwrap();
    let mut app = fixture.app();
    app.open_path(&path).unwrap();
    let owner = app.active.as_ref().unwrap().id;
    app.apply_line_spacing(owner, 135).unwrap();
    app.show(Screen::Book);
    begin_count(&mut app);
    let anchor = book(&app).anchor();
    app.apply_word_spacing(
        owner,
        WordSpacePercent {
            scaling: 127,
            minimum: 90,
        },
    )
    .unwrap();
    assert!(typography_pending(&app));
    assert!(count(&app).is_none());
    assert_eq!(book(&app).anchor(), anchor);
    assert!(app.screen_page_count_intent().is_none());
    drain(&mut app);
    assert!(count(&app).is_none());

    app.show(Screen::Book);
    begin_count(&mut app);
    app.input(Input::Sleep);
    drain(&mut app);
    assert!(count(&app).is_none());
    assert!(app.screen_page_count_intent().is_none());
    app.input(Input::Wake);
    begin_count(&mut app);
    app.storage_returned().unwrap();
    drain(&mut app);
    assert!(app.screen_page_count_intent().is_none());
    assert!(count(&app).is_none());
    assert_eq!(book(&app).anchor(), anchor);

    app.show(Screen::GoToPage);
    app.editor.text = "17".chars().collect();
    app.jump_reflow_number().unwrap();
    assert!(app.screen_page_count_intent() == Some(ScreenPageIntent::Visible));
    let now = Instant::now();
    app.tick(now);
    app.tick(now + Duration::from_millis(380));
    assert_eq!(wait_for_count(&mut app), (17, 48));
    app.activate(Action::ReturnToLocation).unwrap();
    assert_eq!(book(&app).anchor(), anchor);

    app.editor.text = "144".chars().collect();
    app.apply_content_dpi(owner).unwrap();
    assert!(typography_pending(&app));
    app.show(Screen::Book);
    begin_count(&mut app);
    assert_eq!(wait_for_count(&mut app), (1, 48));
}

#[test]
fn typography_pagination_keeps_the_accepted_index_when_a_profile_save_fails() {
    let fixture = Fixture::new();
    let path = fixture.root.join("Typography notes.epub");
    write_outline_fixture(&path, None, None);
    let mut app = fixture.app();
    app.open_path(&path).unwrap();
    let original = app.current_text_preferences().unwrap();
    let edited = BookPreferences {
        logical_font_size: Some(33.0),
        ..original.clone()
    };
    app.apply_text_profile(edited).unwrap();
    begin_count(&mut app);
    let accepted = wait_for_count(&mut app);
    let position = app.current_position();
    app.activate(Action::SetHangingPunctuation {
        enabled: true,
        for_new_books: false,
    })
    .unwrap();
    assert_eq!(count(&app), Some(accepted));
    assert_eq!(app.current_position(), position);
    assert!(!typography_pending(&app));
    fs::remove_file(&fixture.state).unwrap();
    fs::create_dir(&fixture.state).unwrap();
    assert!(app.apply_text_profile(original).is_err());
    assert_eq!(count(&app), Some(accepted));
    assert_eq!(app.current_position(), position);
    assert!(!typography_pending(&app));
    fs::remove_dir(&fixture.state).unwrap();
}
