use super::*;
use crate::app::tests::Fixture;
use primitive_app::App;

#[test]
fn top_header_legacy_book_keeps_enabled_after_new_book_default_changes() {
    let fixture = Fixture::new();
    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut original = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
    original.open_path(&fixture.book).unwrap();
    let id = original.active.as_ref().unwrap().id;
    // Recreate library and controls written before the per-book key was introduced.
    original.library.state_mut(id).unwrap().top_status_header = None;
    original.save().unwrap();
    std::fs::write(
        &original.controls_path,
        r#"{"top_header":{"enabled":true}}"#,
    )
    .unwrap();
    drop(original);

    let mut app = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
    assert!(app.controls.top_header.default_enabled());
    assert!(app.controls.top_header.legacy_enabled);
    app.activate(Action::SetReadingControls(ReadingControls {
        top_header: TopHeader {
            book_default: Some(false),
            ..app.controls.top_header
        },
        ..app.controls
    }))
    .unwrap();
    assert!(app.top_header_for_book(id));
    assert!(!app.controls.top_header.default_enabled());
    app.open_path(&fixture.book).unwrap();
    assert!(app.top_header_caption().is_some());
    assert_eq!(app.library.state(id).unwrap().top_status_header, Some(true));
}

#[test]
fn top_header_about_opens_from_settings_and_dismisses_without_changing_the_book() {
    use crate::app::{
        LIST_X, LIST_Y, ROW_HEIGHT, ROWS_PER_PAGE, Screen, tests::capture_test_screen,
    };

    fn tap(rect: Rect) -> Input {
        Input::Tap((rect.x + rect.w / 2) as f32, (rect.y + rect.h / 2) as f32)
    }

    let fixture = Fixture::new();
    let fonts = vec![
        crate::test_fixture("fonts/NotoSans-Regular.ttf"),
        crate::test_fixture("fonts/NotoSans-Bold.ttf"),
    ];
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
    app.open_path(&fixture.book).unwrap();
    app.activate(Action::Jump(crate::epub::Destination {
        path: "EPUB/chapters/dusk.xhtml".into(),
        fragment: Some("door".into()),
    }))
    .unwrap();
    app.activate(Action::TopStatusHeader(true)).unwrap();
    let id = app.active.as_ref().unwrap().id;
    let position = app.current_history_location();
    let controls = app.controls;
    let font_paths = fonts
        .iter()
        .map(|font| font.display().to_string())
        .collect::<Vec<_>>();
    let mut canvas = Canvas::new(WIDTH, HEIGHT, &font_paths).unwrap();
    assert_eq!(ABOUT.bottom() - ABOUT_DONE.bottom(), 48);
    assert!(ABOUT_SETUP.bottom() + 16 <= ABOUT_DONE.y);
    for (bounds, text) in [
        (ABOUT_BEHAVIOR, ABOUT_BEHAVIOR_TEXT),
        (ABOUT_SETUP, ABOUT_SETUP_TEXT),
    ] {
        assert_eq!(canvas.text_pages(bounds, text, BODY.lines(4)).len(), 1);
    }
    let before = primitive_app::frame(&app, &mut canvas, false);

    app.show(Screen::FooterSettings);
    let index = app
        .footer_items()
        .iter()
        .position(|row| row.title == "About the top status header")
        .unwrap();
    let page = index / ROWS_PER_PAGE;
    app.list_page = page;
    let row = (index % ROWS_PER_PAGE) as i32;
    let about = Input::Tap(
        (LIST_X + 40) as f32,
        (LIST_Y + ROW_HEIGHT * row + 26) as f32,
    );
    assert_eq!(app.input(about), Change::Partial);
    assert!(app.top_header_about);
    assert_eq!(app.screen, Screen::FooterSettings);
    assert_eq!(app.list_page, page);
    assert_eq!(app.input(tap(ABOUT_BEHAVIOR)), Change::None);
    assert_eq!(app.input(Input::PageForward), Change::None);
    assert_eq!(
        app.swipe(
            Input::Swipe {
                dx: 0.0,
                dy: -180.0,
                start_y: 800.0
            },
            500.0
        ),
        Change::None
    );
    capture_test_screen(&app, &fonts, "READER_TOP_HEADER_ABOUT_SCREENSHOT", false);
    capture_test_screen(
        &app,
        &fonts,
        "READER_TOP_HEADER_ABOUT_DARK_SCREENSHOT",
        true,
    );

    for close in [tap(ABOUT_DONE), Input::Tap(10.0, 10.0), Input::PageBack] {
        assert!(app.top_header_about);
        assert_eq!(app.input(close), Change::Partial);
        assert!(!app.top_header_about);
        assert_eq!(app.screen, Screen::FooterSettings);
        assert_eq!(app.list_page, page);
        assert_eq!(app.controls, controls);
        assert_eq!(app.library.state(id).unwrap().top_status_header, Some(true));
        assert_eq!(app.current_history_location(), position);
        assert_eq!(app.input(about), Change::Partial);
    }
    app.show(Screen::Book);
    assert!(!app.top_header_about);
    assert_eq!(app.current_history_location(), position);
    let after = primitive_app::frame(&app, &mut canvas, false);
    let footer_start = (crate::session::PAGE_HEIGHT * WIDTH) as usize;
    assert!(before[footer_start..] == after[footer_start..]);
}

#[test]
fn top_header_live_identity_edits_from_book_details_and_preserves_the_reading_page() {
    use crate::app::{
        Screen, tests::capture_test_screen, tests::tap_contents_item, top_config::TopGroup,
    };
    use primitive_ui::{Key, Layer};
    use std::fs;

    fn tap(app: &mut Reader, target: Rect) -> Change {
        app.input(Input::Tap(
            (target.x + target.w / 2) as f32,
            (target.y + target.h / 2) as f32,
        ))
    }

    fn type_key(app: &mut Reader, key: Key) {
        let target = app
            .navigation_layout()
            .keyboard()
            .keyboard_keys_with(app.editor.layer, "Save")
            .into_iter()
            .find_map(|(candidate, rect)| (candidate.key == key).then_some(rect))
            .unwrap_or_else(|| panic!("Missing {key:?} on {:?}", app.editor.layer));
        assert_eq!(tap(app, target), Change::Partial);
    }

    fn enter_text(app: &mut Reader, text: &str) {
        for _ in 0..app.editor.caret {
            type_key(app, Key::Delete);
        }
        if app.editor.layer == Layer::Upper {
            type_key(app, Key::Shift);
        }
        for character in text.chars() {
            type_key(
                app,
                if character == ' ' {
                    Key::Space
                } else {
                    Key::Char(character)
                },
            );
        }
        type_key(app, Key::Go);
        assert_eq!(app.screen, Screen::BookDetails);
    }

    fn details(app: &mut Reader) {
        let action = app
            .top_config_items(TopGroup::Library)
            .into_iter()
            .find(|item| item.title == "Book details")
            .unwrap()
            .action;
        app.activate(action).unwrap();
        assert_eq!(app.screen, Screen::BookDetails);
    }

    fn reading(app: &mut Reader) {
        assert_eq!(
            tap(app, primitive_ui::back_button(WIDTH as i32)),
            Change::Partial
        );
        assert_eq!(app.screen, Screen::BookTools);
        // Dismiss the overlay without advancing the current page.
        let y = app.navigation_layout().height / 2;
        assert_eq!(app.input(Input::Tap(600., y as f32)), Change::Partial);
        assert_eq!(app.screen, Screen::Book);
    }

    let fixture = Fixture::new();
    let original = fs::read(crate::test_fixture("many-chapters.epub")).unwrap();
    fs::write(&fixture.book, &original).unwrap();
    let sidecar = fixture
        .book
        .with_extension("sdr")
        .join("custom_metadata.lua");
    fs::create_dir_all(sidecar.parent().unwrap()).unwrap();
    let custom = b"return {custom_props = {title = 'Coastal Ledger', authors = 'Ada Scout'}}";
    fs::write(&sidecar, custom).unwrap();
    let fonts = vec![
        crate::test_fixture("fonts/NotoSans-Regular.ttf"),
        crate::test_fixture("fonts/NotoSans-Bold.ttf"),
    ];
    let font_paths = fonts
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>();
    let mut canvas = Canvas::new(WIDTH, HEIGHT, &font_paths).unwrap();
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
    app.open_path(&fixture.book).unwrap();
    app.activate(Action::TopStatusHeader(true)).unwrap();
    app.controls.top_header.clock = false;
    app.controls.top_header.battery = HeaderBattery::Off;
    let before = primitive_app::frame(&app, &mut canvas, false);
    let location = app.current_history_location();
    let panel = app.status_caption();
    assert!(
        app.top_header_caption()
            .unwrap()
            .starts_with("Ada Scout.  Coastal Ledger")
    );

    details(&mut app);
    tap_contents_item(&mut app, "Title");
    assert_eq!(app.screen, Screen::BookIdentityEdit(IdentityField::Title));
    assert_eq!(app.editor.string(), "Coastal Ledger");
    capture_test_screen(&app, &fonts, "READER_HEADER_IDENTITY_EDITOR", false);
    enter_text(&mut app, "harbor log");

    tap_contents_item(&mut app, "Author");
    assert_eq!(app.editor.string(), "Ada Scout");
    enter_text(&mut app, "linh tran");
    reading(&mut app);
    let after = primitive_app::frame(&app, &mut canvas, false);
    let top = (WIDTH * TopHeader::HEIGHT) as usize;
    assert_ne!(&before[..top], &after[..top]);
    assert_eq!(&before[top..], &after[top..]);
    assert_eq!(app.current_history_location(), location);
    assert_eq!(app.status_caption(), panel);
    assert!(
        app.top_header_caption()
            .unwrap()
            .starts_with("linh tran.  harbor log")
    );
    capture_test_screen(&app, &fonts, "READER_HEADER_IDENTITY_READING", false);

    app.save().unwrap();
    drop(app);
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
    app.open_path(&fixture.book).unwrap();
    assert!(
        app.top_header_caption()
            .unwrap()
            .starts_with("linh tran.  harbor log")
    );
    details(&mut app);
    tap_contents_item(&mut app, "Title");
    let reset = app.book_identity_reset(IdentityField::Title).unwrap();
    assert_eq!(tap(&mut app, reset), Change::Partial);
    assert_eq!(app.screen, Screen::BookDetails);
    tap_contents_item(&mut app, "Author");
    type_key(&mut app, Key::Delete);
    assert_eq!(app.input(Input::PageBack), Change::Partial);
    reading(&mut app);
    assert!(
        app.top_header_caption()
            .unwrap()
            .starts_with("linh tran.  Coastal Ledger")
    );

    app.save().unwrap();
    assert_eq!(fs::read(&fixture.book).unwrap(), original);
    assert_eq!(fs::read(&sidecar).unwrap(), custom);
}
