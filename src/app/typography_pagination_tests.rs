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

/// Two chapters: a short opening chapter, then a chapter that runs to several
/// pages and carries the `tide-*` anchors the test navigates to.
fn write_tide_fixture(path: &std::path::Path) {
    use std::io::Write as _;
    let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    let stored =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let deflated = zip::write::SimpleFileOptions::default();
    zip.start_file("mimetype", stored).unwrap();
    zip.write_all(b"application/epub+zip").unwrap();
    zip.start_file("META-INF/container.xml", deflated).unwrap();
    zip.write_all(br#"<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container" version="1.0"><rootfiles><rootfile full-path="EPUB/book.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#).unwrap();
    zip.start_file("EPUB/book.opf", deflated).unwrap();
    zip.write_all(br#"<package xmlns="http://www.idpf.org/2007/opf" version="3.0"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>Tide survey</dc:title><dc:language>en</dc:language><dc:identifier id="id">tide-survey</dc:identifier></metadata><manifest><item id="c0" href="c0.xhtml" media-type="application/xhtml+xml"/><item id="c1" href="c1.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="c0"/><itemref idref="c1"/></spine></package>"#).unwrap();
    zip.start_file("EPUB/c0.xhtml", deflated).unwrap();
    zip.write_all(br#"<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Opening</title></head><body><h1>Opening</h1><p>A short opening chapter.</p></body></html>"#).unwrap();
    let filler = "The water rises and falls with the moon over the slow channel. ".repeat(12);
    let mut body = String::from(
        r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Tides</title></head><body><h1>Tides</h1>"#,
    );
    for index in 0..40 {
        if index == 5 {
            body.push_str(r#"<p id="tide-a">Tide observation A.</p>"#);
        }
        if index == 30 {
            body.push_str(r#"<p id="tide-b">Tide observation B.</p>"#);
        }
        body.push_str(&format!("<p>{filler}</p>"));
    }
    body.push_str("</body></html>");
    zip.start_file("EPUB/c1.xhtml", deflated).unwrap();
    zip.write_all(body.as_bytes()).unwrap();
    zip.finish().unwrap();
}

#[test]
fn page_browser_indexes_saved_anchors_and_highlight_endpoints_outside_the_loaded_chapter() {
    let fixture = Fixture::new();
    let mut app = fixture.app();
    let book_path = fixture.root.join("Tide survey.epub");
    write_tide_fixture(&book_path);
    app.open_path(&book_path).unwrap();
    let target_chapter = 1;
    assert_ne!(book(&app).chapter, target_chapter);

    let target_probe = book(&app).config.chapter(target_chapter).unwrap();
    let anchor = target_probe.fragment("tide-b").unwrap();
    let target_page = target_probe.page_for(&anchor).unwrap();
    let range_start = target_probe.page_anchor(0).unwrap();
    let range_start_page = target_probe.page_for(&range_start).unwrap();
    let mut range_end = target_probe
        .page_anchor(target_probe.page_count().saturating_sub(1))
        .unwrap();
    range_end.side_bias = Some(crate::SideBias::Before);
    let range_end_page = target_probe.page_for(&range_end).unwrap();
    let range_selection = target_probe.select_range(&range_start, &range_end).unwrap();
    drop(target_probe);

    let id = app.open_book_id().unwrap();
    let state = app.library.state_mut(id).unwrap();
    state
        .custom_contents
        .save(
            crate::library::ReadingPosition {
                chapter: target_chapter,
                page: 0,
                anchor: Some(anchor.clone()),
            },
            "Cross-chapter anchor",
        )
        .unwrap();
    state.custom_contents.enabled = true;
    state.bookmarks.push(crate::library::Bookmark {
        note: None,
        updated_at: None,
        chapter_title: None,
        created_at: None,
        label: "Cross-chapter bookmark".into(),
        position: crate::library::ReadingPosition {
            chapter: target_chapter,
            page: 0,
            anchor: Some(anchor.clone()),
        },
    });
    app.back.push(
        crate::library::ReadingPosition {
            chapter: target_chapter,
            page: 0,
            anchor: Some(anchor.clone()),
        }
        .into(),
    );
    state.highlights.push(crate::library::Highlight {
        updated_at: None,
        chapter_title: None,
        created_at: None,
        style: Default::default(),
        color: Default::default(),
        selection: range_selection,
        note: None,
    });

    app.activate(Action::Overview).unwrap();
    assert_eq!(app.screen, Screen::PageOverview);
    wait_for_count(&mut app);

    let targets = app.overview_page_targets();
    let reading = book(&app);
    let indexed = reading
        .whole_book_custom_locations(&targets.targets)
        .unwrap();
    let prefix: usize = (0..target_chapter)
        .map(|chapter| reading.config.chapter(chapter).unwrap().page_count())
        .sum();
    let expected = prefix + target_page;
    assert_eq!(
        &indexed.pages[..3],
        &[Some(expected), Some(expected), Some(expected)]
    );
    assert_eq!(
        &indexed.pages[3..],
        &[
            Some(prefix + range_start_page),
            Some(prefix + range_end_page)
        ]
    );
    assert!(
        app.overview
            .as_ref()
            .unwrap()
            .book_outline_starts()
            .contains(&expected)
    );
    assert_eq!(
        app.overview.as_ref().unwrap().bookmark_pages(),
        &[Some(expected)]
    );
    let (back_pages, forward_pages) = app.overview.as_ref().unwrap().history_pages();
    assert_eq!(back_pages, &[Some(expected)]);
    assert!(forward_pages.is_empty());
    assert_eq!(
        app.overview.as_ref().unwrap().highlight_pages(),
        &[Some((prefix + range_start_page, prefix + range_end_page))]
    );
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
