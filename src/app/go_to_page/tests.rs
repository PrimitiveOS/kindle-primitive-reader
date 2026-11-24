use super::*;
use crate::app::tests::{Fixture, capture_test_screen};
use primitive_app::App;
use std::time::Duration;

fn submit(app: &mut Reader, value: &str) {
    app.editor.text = value.chars().collect();
    app.editor.caret = app.editor.text.len();
    let keyboard = Layout::new(WIDTH, HEIGHT, 4, 5);
    let (x, y) = (945..HEIGHT)
        .step_by(8)
        .find_map(|y| {
            (850..WIDTH).step_by(8).find_map(|x| {
                let key = keyboard_key(&keyboard, &app.editor, "Go", x as f32, y as f32)?;
                let mut editor = Editor::new(32);
                matches!(editor.press(key), Press::Go).then_some((x as f32, y as f32))
            })
        })
        .unwrap();
    assert_eq!(app.input(Input::Tap(x, y)), Change::Partial);
}

fn fonts() -> Vec<PathBuf> {
    vec![
        crate::test_fixture("fonts/NotoSans-Regular.ttf"),
        crate::test_fixture("fonts/NotoSans-Bold.ttf"),
    ]
}

#[test]
fn go_to_page_indexes_the_current_epub_layout_then_applies_absolute_and_relative_jumps() {
    let f = Fixture::new();
    let book = f.root.join("Many chapters.epub");
    fs::copy(crate::test_fixture("many-chapters.epub"), &book).unwrap();
    let fonts = fonts();
    let mut app = Reader::open(&f.root, &f.state, fonts.clone()).unwrap();
    app.open_path(&book).unwrap();
    let reading = &app.active.as_ref().unwrap().reading;
    assert_eq!(reading.config.settings.status_header_height, 0);
    assert!(reading.whole_book_screen_pages().is_none());
    let chapter = 3;
    let ordinal = (0..chapter)
        .map(|index| reading.config.chapter(index).unwrap().page_count())
        .sum::<usize>()
        + 1;
    let origin = app.current_history_location();
    assert!(!app.needs_screen_page_count());
    app.show(Screen::GoToPage);
    submit(&mut app, &ordinal.to_string());
    assert_eq!(app.screen, Screen::GoToPage);
    assert!(app.screen_page_jump.is_some());
    assert_eq!(app.current_history_location(), origin);
    assert!(app.back.is_empty());
    capture_test_screen(&app, &fonts, "READER_GOTO_INDEX", false);
    let deadline = Instant::now() + Duration::from_secs(10);
    while app.screen_page_jump.is_some() {
        assert!(Instant::now() < deadline);
        app.tick(Instant::now());
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(app.screen, Screen::Book, "{:?}", app.notice);
    let reading = &app.active.as_ref().unwrap().reading;
    assert_eq!((reading.chapter, reading.page), (chapter, 0));
    assert_eq!(reading.config.settings.status_header_height, 0);
    assert_eq!(reading.whole_book_screen_pages().unwrap().0, ordinal);
    assert_eq!(app.back.len(), 1);
    app.activate(Action::ReturnToLocation).unwrap();
    assert_eq!(app.current_history_location(), origin);
    app.show(Screen::GoToPage);
    submit(&mut app, &format!("+{}", ordinal - 1));
    assert_eq!(app.active.as_ref().unwrap().reading.chapter, chapter);
    app.show(Screen::GoToPage);
    submit(&mut app, "-999999");
    assert_eq!(app.current_history_location(), origin);
    app.show(Screen::GoToPage);
    submit(&mut app, "2:1");
    assert_eq!(app.active.as_ref().unwrap().reading.chapter, 1);
    app.show(Screen::GoToPage);
    submit(&mut app, "999999");
    assert_eq!(app.screen, Screen::GoToPage);
    assert!(
        app.notice
            .as_deref()
            .unwrap()
            .starts_with("Enter a page from 1 to")
    );
    let before = app.current_history_location();
    let history = app.back.len();
    let mut changed = fs::read(&book).unwrap();
    changed.push(0);
    fs::write(&book, changed).unwrap();
    submit(&mut app, "1");
    assert_eq!(app.screen, Screen::GoToPage);
    assert!(app.notice.as_deref().unwrap().contains("changed"));
    assert_eq!(app.current_history_location(), before);
    assert_eq!(app.back.len(), history);
}

#[test]
fn go_to_page_cancels_on_sleep_and_changed_layout_and_preserves_scrolling_fallback() {
    let f = Fixture::new();
    let mut app = f.app();
    app.open_path(&f.book).unwrap();
    let origin = app.current_history_location();
    app.show(Screen::GoToPage);
    submit(&mut app, "2");
    assert!(app.screen_page_jump.is_some());
    app.input(Input::Sleep);
    assert!(app.screen_page_jump.is_none());
    app.input(Input::Wake);
    submit(&mut app, "2");
    assert!(app.screen_page_jump.is_some());
    let read = &mut app.active.as_mut().unwrap().reading;
    read.reflow(Settings {
        height: read.config.settings.height - 48,
        ..read.config.settings
    })
    .unwrap();
    app.tick(Instant::now());
    assert!(app.screen_page_jump.is_none());
    assert!(app.notice.as_deref().unwrap().contains("layout changed"));
    assert!(app.back.is_empty());
    assert_eq!(
        app.current_history_location().unwrap().position.chapter,
        origin.unwrap().position.chapter
    );
    app.activate(Action::ReadingMode(true)).unwrap();
    app.show(Screen::GoToPage);
    submit(&mut app, "+2");
    assert_eq!(app.screen, Screen::GoToPage);
    assert!(app.notice.as_deref().unwrap().contains("unavailable"));
    submit(&mut app, "2:1");
    assert_eq!(app.active.as_ref().unwrap().reading.chapter, 1);
}

#[test]
fn go_to_page_pdf_preserves_physical_labels_percentages_and_hidden_flow_history() {
    let f = Fixture::new();
    let book = f.root.join("Flow map.pdf");
    fs::write(&book, crate::fixed_session::tests::striped_pdf_bytes(8)).unwrap();
    let fonts = fonts();
    let mut app = Reader::open(&f.root, &f.state, fonts.clone()).unwrap();
    app.open_path(&book).unwrap();
    let mut flows = crate::library::hidden_flows::HiddenFlows::default();
    flows.enabled = true;
    for page in [2, 4, 6] {
        flows
            .toggle(
                ReadingPosition {
                    chapter: 0,
                    page,
                    anchor: None,
                },
                None,
            )
            .unwrap();
    }
    app.fixed
        .as_mut()
        .unwrap()
        .reading
        .set_hidden_flows(&flows)
        .unwrap();
    let origin = app.current_history_location();
    app.show(Screen::GoToPage);
    capture_test_screen(&app, &fonts, "READER_GOTO_PDF_FLOWS", true);
    submit(&mut app, "+3");
    assert_eq!(app.fixed.as_ref().unwrap().reading.page(), 5);
    app.activate(Action::ReturnToLocation).unwrap();
    assert_eq!(app.current_history_location(), origin);
    app.show(Screen::GoToPage);
    submit(&mut app, "[2]1");
    assert_eq!(app.fixed.as_ref().unwrap().reading.page(), 3);
    app.show(Screen::GoToPage);
    submit(&mut app, "[4]");
    assert_eq!(app.fixed.as_ref().unwrap().reading.page(), 5);
    app.show(Screen::GoToPage);
    submit(&mut app, "[3]2");
    assert_eq!(app.screen, Screen::GoToPage);
    assert_eq!(app.fixed.as_ref().unwrap().reading.page(), 5);
    submit(&mut app, "100%");
    assert_eq!(app.fixed.as_ref().unwrap().reading.page(), 7);
    app.show(Screen::GoToPage);
    submit(&mut app, "1");
    assert_eq!(app.fixed.as_ref().unwrap().reading.page(), 0);
    let history = app.back.len();
    app.show(Screen::GoToPage);
    submit(&mut app, "-999999");
    assert_eq!(app.fixed.as_ref().unwrap().reading.page(), 0);
    assert_eq!(app.back.len(), history);
}
