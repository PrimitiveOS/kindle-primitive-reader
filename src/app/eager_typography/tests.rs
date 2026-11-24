use super::*;
use crate::app::selection_viewport_tests::{adopt, capture, key, tap};
use crate::app::tests::{Fixture, tap_contents_item, write_outline_fixture};
use crate::session::ReadingViewport;
use primitive_ui::Key;
use std::io::Write;

fn setup() -> (Fixture, Reader, PathBuf) {
    let fixture = Fixture::new();
    let path = fixture.root.join("Shore notebook.epub");
    write_outline_fixture(&path, None, None);
    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
    app.open_path(&path).unwrap();
    (fixture, app, path)
}

fn live(app: &Reader) -> &Reading {
    &app.active.as_ref().unwrap().reading
}

fn size(app: &Reader) -> Option<f64> {
    live(app).config.settings.logical_font_size
}

fn wait(app: &mut Reader) {
    let stop = Instant::now() + Duration::from_secs(15);
    while app.eager_font_deadline().is_some() {
        assert!(Instant::now() < stop);
        app.tick(Instant::now());
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn whole_typography_applies_the_prepared_spine_at_the_latest_reading_position() {
    let (fixture, mut app, path) = setup();
    if let Some(stock) = std::env::var_os("READER_TYPOGRAPHY_STOCK_FIXTURE") {
        fs::copy(&path, stock).unwrap();
    }
    let old = size(&app);
    let owner = app.open_book_id().unwrap();
    let original = fs::read(&path).unwrap();
    let mut canvas = adopt(&mut app, ReadingViewport::PORTRAIT, 1);
    app.show(Screen::ReadingControls);
    let index = app
        .items()
        .iter()
        .position(|item| item.title == "Font size rendering")
        .unwrap();
    let rows = app.list_geometry().rows;
    for _ in 0..index / rows {
        assert_eq!(app.input(Input::PageForward), Change::Partial);
    }
    tap_contents_item(&mut app, "Font size rendering");
    tap_contents_item(&mut app, "Default for EPUBs");
    tap_contents_item(&mut app, "Whole book first");
    assert_eq!(app.controls.font_size_rendering, Mode::WholeBook);
    let back = app.navigation_layout().back();
    tap(&mut app, back);
    assert!(app.selected_font_rendering().is_none());
    tap_contents_item(&mut app, "Chapter first");
    assert_eq!(app.selected_font_rendering(), Some(Mode::Chapter));
    tap_contents_item(&mut app, "Whole book first");
    assert_eq!(app.selected_font_rendering(), Some(Mode::WholeBook));
    capture(
        &app,
        &mut canvas,
        "whole-book-font-size-controls.png",
        false,
    );

    app.show(Screen::NumberingValue(NumberField::LogicalFont(owner)));
    app.editor.text = "34".chars().collect();
    app.editor.caret = app.editor.text.len();
    let go = key(&app, Key::Go, "Set");
    tap(&mut app, go);
    assert_eq!(app.screen, Screen::Book);
    assert!(app.eager_typography.pending());
    assert_eq!(size(&app), old);
    assert_ne!(
        app.library
            .state(owner)
            .unwrap()
            .preferences
            .logical_font_size,
        Some(34.0)
    );
    capture(
        &app,
        &mut canvas,
        "whole-book-font-size-preparing.png",
        false,
    );
    app.tick(Instant::now());
    assert!(app.eager_typography.running.is_some());
    app.follow_reflow_page(0, 2).unwrap();
    let latest = live(&app).anchor().unwrap();
    assert_eq!(size(&app), old);
    app.show(Screen::Contents);
    wait(&mut app);
    assert!(app.eager_typography.prepared.is_some());
    assert_eq!(size(&app), old);
    app.show(Screen::Book);
    wait(&mut app);
    assert_eq!(size(&app), Some(34.0));
    assert_eq!(
        app.library
            .state(owner)
            .unwrap()
            .preferences
            .logical_font_size,
        Some(34.0)
    );
    assert_eq!(live(&app).anchor().as_ref(), Some(&latest));
    let independent: usize = (0..live(&app).probe.chapter_count())
        .map(|chapter| live(&app).config.chapter(chapter).unwrap().page_count())
        .sum();
    assert_eq!(live(&app).whole_book_screen_pages().unwrap().1, independent);
    assert!(live(&app).config.eager_layouts.contains(1));
    assert_eq!(fs::read(&path).unwrap(), original);
    capture(
        &app,
        &mut canvas,
        "whole-book-font-size-committed.png",
        false,
    );

    app.follow_reflow_page(1, 0).unwrap();
    assert!(!live(&app).config.eager_layouts.contains(1));
    assert!(live(&app).config.eager_layouts.contains(0));
    app.follow_reflow_page(0, 0).unwrap();
    assert!(live(&app).config.eager_layouts.contains(1));
    let fonts = app.fonts.clone();
    let mut reopened = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
    reopened.open_path(&path).unwrap();
    assert_eq!(reopened.selected_font_rendering(), Some(Mode::WholeBook));
    assert_eq!(reopened.controls.font_size_rendering, Mode::WholeBook);
    assert_eq!(size(&reopened), Some(34.0));
    assert!(reopened.eager_font_deadline().is_none());
}

#[test]
fn whole_typography_supersedes_edits_and_keeps_old_settings_on_cancel_and_failed_save() {
    let (fixture, mut app, _) = setup();
    app.set_font_rendering(Scope::Book, Some(Mode::WholeBook))
        .unwrap();
    let owner = app.open_book_id().unwrap();
    let original = size(&app);
    let anchor = live(&app).anchor();

    app.apply_logical_font(owner, 25.0).unwrap();
    app.tick(Instant::now());
    app.apply_logical_font(owner, 31.0).unwrap();
    assert_eq!(size(&app), original);
    wait(&mut app);
    assert_eq!(size(&app), Some(31.0));
    assert_eq!(live(&app).anchor(), anchor);

    for boundary in [Input::Sleep, Input::Wake] {
        app.apply_logical_font(owner, 29.0).unwrap();
        app.tick(Instant::now());
        if boundary == Input::Wake {
            app.storage_returned().unwrap();
        } else {
            app.input(boundary);
        }
        wait(&mut app);
        app.input(Input::Wake);
        assert_eq!(size(&app), Some(31.0));
        assert!(app.eager_typography.draft.is_none());
    }

    app.apply_logical_font(owner, 38.0).unwrap();
    app.tick(Instant::now());
    let prior = live(&app).whole_book_screen_pages();
    fs::remove_file(&fixture.state).unwrap();
    fs::create_dir(&fixture.state).unwrap();
    wait(&mut app);
    assert!(app.notice.is_some());
    assert_eq!(size(&app), Some(31.0));
    assert_eq!(live(&app).anchor(), anchor);
    assert_eq!(live(&app).whole_book_screen_pages(), prior);
    fs::remove_dir(&fixture.state).unwrap();
}

#[test]
fn whole_typography_uses_chapter_first_for_a_spine_outside_the_budget() {
    let (fixture, mut app, _) = setup();
    app.set_font_rendering(Scope::Default, Some(Mode::WholeBook))
        .unwrap();
    let path = fixture.root.join("Many chapters.epub");
    fs::copy(crate::test_fixture("many-chapters.epub"), &path).unwrap();
    app.open_path(&path).unwrap();
    assert!(!supported(live(&app)));
    let owner = app.open_book_id().unwrap();
    app.apply_logical_font(owner, 30.0).unwrap();
    assert_eq!(size(&app), Some(30.0));
    assert!(app.eager_font_deadline().is_none());
    assert!(live(&app).config.screen_pages.typography_pending());
}

#[test]
fn whole_typography_allows_an_explicit_jump_before_its_commit() {
    let (_fixture, mut app, _) = setup();
    app.set_font_rendering(Scope::Book, Some(Mode::WholeBook))
        .unwrap();
    let old = size(&app);
    let owner = app.open_book_id().unwrap();
    assert!(live(&app).whole_book_screen_pages().is_none());
    app.apply_logical_font(owner, 31.0).unwrap();
    app.tick(Instant::now());
    app.show(Screen::GoToPage);
    app.editor.text = "5".chars().collect();
    app.jump_reflow_number().unwrap();
    let stop = Instant::now() + Duration::from_secs(10);
    while app.screen == Screen::GoToPage {
        assert!(Instant::now() < stop);
        app.tick(Instant::now());
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(size(&app), old);
    assert_eq!(live(&app).whole_book_screen_pages().unwrap().0, 5);
    let reached = live(&app).anchor();
    wait(&mut app);
    assert_eq!(size(&app), Some(31.0));
    assert_eq!(live(&app).anchor(), reached);
}

#[test]
fn whole_typography_rejects_embedded_assets_without_changing_the_accepted_page() {
    let (fixture, mut app, _) = setup();
    let path = fixture.root.join("With publisher font.epub");
    write_outline_fixture(&path, None, None);
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .unwrap();
    let mut archive = zip::ZipWriter::new_append(file).unwrap();
    archive
        .start_file(
            "EPUB/fonts/author.otf",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
    archive
        .write_all(b"An authored resource for the archive limit test")
        .unwrap();
    archive.finish().unwrap();
    app.open_path(&path).unwrap();
    app.set_font_rendering(Scope::Book, Some(Mode::WholeBook))
        .unwrap();
    let old = size(&app);
    let owner = app.open_book_id().unwrap();
    let prior = live(&app).anchor();
    app.apply_logical_font(owner, 31.0).unwrap();
    wait(&mut app);
    assert_eq!(size(&app), old);
    assert_eq!(live(&app).anchor(), prior);
    assert!(app.notice.as_deref().unwrap().contains("chapter-first"));
    assert!(live(&app).whole_book_screen_pages().is_none());
}
