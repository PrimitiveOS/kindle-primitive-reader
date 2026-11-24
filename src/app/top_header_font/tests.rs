use super::*;
use crate::app::{
    LIST_X, LIST_Y, ROW_HEIGHT, ROWS_PER_PAGE, Screen,
    tests::{Fixture, capture_test_screen},
};
use crate::session::PAGE_HEIGHT;
use primitive_app::App;

fn tap(rect: Rect) -> Input {
    Input::Tap((rect.x + rect.w / 2) as f32, (rect.y + rect.h / 2) as f32)
}

fn hold(rect: Rect) -> Input {
    Input::LongTap((rect.x + rect.w / 2) as f32, (rect.y + rect.h / 2) as f32)
}

fn open_from_settings(app: &mut Reader) {
    app.show(Screen::FooterSettings);
    let index = app
        .footer_items()
        .iter()
        .position(|row| row.title == "Header font size")
        .unwrap();
    app.list_page = index / ROWS_PER_PAGE;
    let row = (index % ROWS_PER_PAGE) as i32;
    assert_eq!(
        app.input(Input::Tap(
            (LIST_X + 40) as f32,
            (LIST_Y + ROW_HEIGHT * row + 30) as f32
        )),
        Change::Partial
    );
    assert_eq!(app.screen, Screen::FooterSettings);
    assert_eq!(app.list_page, index / ROWS_PER_PAGE);
}

fn draft(app: &Reader) -> u8 {
    app.top_header_font_picker.as_ref().unwrap().size
}

fn key(app: &mut Reader, value: char) -> Change {
    let layout = Layout::new(WIDTH, HEIGHT, 4, 5);
    let (_, rect) = layout
        .keyboard_keys_with(Layer::Lower, ENTRY_USE)
        .into_iter()
        .find(|(button, _)| button.key == primitive_ui::Key::Char(value))
        .unwrap();
    app.input(tap(rect))
}

#[test]
fn top_header_font_picker_steps_types_applies_and_resets_without_changing_footer() {
    let fixture = Fixture::new();
    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
    app.open_path(&fixture.book).unwrap();
    app.activate(Action::Jump(crate::epub::Destination {
        path: "EPUB/chapters/dusk.xhtml".into(),
        fragment: Some("door".into()),
    }))
    .unwrap();
    app.activate(Action::TopStatusHeader(true)).unwrap();
    let origin = app.current_history_location();
    let font_paths: Vec<_> = fonts
        .iter()
        .map(|font| font.display().to_string())
        .collect();
    let mut canvas = Canvas::new(WIDTH, HEIGHT, &font_paths).unwrap();
    let baseline = primitive_app::frame(&app, &mut canvas, false);
    assert_eq!(app.controls.top_header.font_size(), 20);

    open_from_settings(&mut app);
    assert_eq!(draft(&app), 20);
    assert_eq!(app.input(tap(SET)), Change::None);
    assert_eq!(app.input(tap(INCREASE)), Change::Partial);
    assert_eq!(draft(&app), 21);
    assert_eq!(app.input(hold(DECREASE)), Change::Partial);
    assert_eq!(draft(&app), 17);
    assert_eq!(app.input(Input::PageForward), Change::Partial);
    assert_eq!(draft(&app), 18);
    assert_eq!(app.input(Input::PageBack), Change::Partial);
    assert_eq!(draft(&app), 17);
    assert_eq!(app.controls.top_header.font_size(), 20);

    assert_eq!(app.input(tap(VALUE)), Change::Partial);
    assert_eq!(key(&mut app, '4'), Change::Partial);
    assert_eq!(key(&mut app, '0'), Change::Partial);
    assert_eq!(app.input(tap(ENTRY_SET)), Change::Partial);
    assert!(
        app.top_header_font_picker
            .as_ref()
            .unwrap()
            .entry
            .as_ref()
            .unwrap()
            .invalid
    );
    assert_eq!(app.input(tap(ENTRY_CANCEL)), Change::Partial);
    assert_eq!(draft(&app), 17);
    assert_eq!(app.input(tap(VALUE)), Change::Partial);
    assert_eq!(key(&mut app, '3'), Change::Partial);
    assert_eq!(key(&mut app, '4'), Change::Partial);
    assert_eq!(app.input(tap(ENTRY_SET)), Change::Partial);
    assert_eq!(draft(&app), 34);
    assert_eq!(app.input(hold(INCREASE)), Change::Partial);
    assert_eq!(draft(&app), 36);
    assert_eq!(app.input(tap(INCREASE)), Change::None);
    assert_eq!(app.controls.top_header.font_size(), 20);
    let picker_frame = primitive_app::frame(&app, &mut canvas, false);
    for sign in [DECREASE, INCREASE] {
        let x = (sign.x + sign.w / 2) as usize;
        let y = (sign.y + sign.h / 2) as usize;
        let center = y * WIDTH as usize + x;
        assert!(picker_frame[center - 2..center + 3].contains(&0));
    }
    capture_test_screen(
        &app,
        &fonts,
        "READER_TOP_HEADER_FONT_PICKER_SCREENSHOT",
        false,
    );

    assert_eq!(app.input(tap(SET)), Change::Partial);
    assert_eq!(draft(&app), 36);
    assert_eq!(app.controls.top_header.font_size(), 36);
    assert_eq!(app.input(tap(DEFAULT)), Change::Partial);
    assert_eq!(draft(&app), 14);
    assert_eq!(app.controls.top_header.font_size(), 36);
    assert_eq!(app.input(tap(CLOSE)), Change::Partial);
    assert!(app.top_header_font_picker.is_none());
    assert_eq!(app.controls.top_header.font_size(), 36);
    app.show(Screen::Book);
    assert_eq!(app.current_history_location(), origin);
    let changed = primitive_app::frame(&app, &mut canvas, false);
    let header_end = (WIDTH * TopHeader::HEIGHT) as usize;
    let footer_start = (WIDTH * PAGE_HEIGHT) as usize;
    assert_ne!(&changed[..header_end], &baseline[..header_end]);
    assert!(changed[header_end..footer_start] == baseline[header_end..footer_start]);
    assert!(
        changed[footer_start..] == baseline[footer_start..],
        "resizing the header must preserve every footer pixel, including Exit"
    );
    capture_test_screen(
        &app,
        &fonts,
        "READER_TOP_HEADER_FONT_LARGE_SCREENSHOT",
        false,
    );

    app.save().unwrap();
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
    app.open_path(&fixture.book).unwrap();
    open_from_settings(&mut app);
    assert_eq!(draft(&app), 36);
    for _ in 0..7 {
        assert_eq!(app.input(hold(DECREASE)), Change::Partial);
    }
    assert_eq!(draft(&app), 8);
    assert_eq!(app.input(tap(DECREASE)), Change::None);
    assert_eq!(app.input(tap(DEFAULT)), Change::Partial);
    assert_eq!(app.input(tap(SET)), Change::Partial);
    assert_eq!(app.controls.top_header.font_size(), 14);
    assert_eq!(draft(&app), 14);
    assert_eq!(app.input(Input::Tap(10.0, 10.0)), Change::Partial);
    assert!(app.top_header_font_picker.is_none());
    assert_eq!(app.controls.top_header.font_size(), 14);
}

#[test]
fn top_header_font_picker_keeps_its_draft_when_saving_fails() {
    let fixture = Fixture::new();
    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
    open_from_settings(&mut app);
    std::fs::create_dir(&app.controls_path).unwrap();
    app.input(tap(INCREASE));
    app.input(tap(SET));
    assert_eq!(app.controls.top_header.font_size(), 20);
    assert_eq!(draft(&app), 21);
    assert!(app.notice.is_some());
    std::fs::remove_dir(&app.controls_path).unwrap();
    assert_eq!(app.input(tap(SET)), Change::Partial);
    assert_eq!(app.controls.top_header.font_size(), 21);
    assert!(app.notice.is_none());
}
