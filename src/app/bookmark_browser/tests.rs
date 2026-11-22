use super::*;
use crate::app::tests::{Fixture, capture_test_screen};
use crate::library::{Highlight, PdfHighlight};
use primitive_app::App;
use std::fs;

fn center(rect: Rect) -> (f32, f32) {
    (
        rect.x as f32 + rect.w as f32 / 2.0,
        rect.y as f32 + rect.h as f32 / 2.0,
    )
}

fn tap(app: &mut Reader, rect: Rect) {
    let (x, y) = center(rect);
    assert_eq!(app.input(Input::Tap(x, y)), Change::Partial);
}

fn choose(app: &mut Reader, label: &str) {
    let (_, rows) = app.list_window();
    let row = rows
        .iter()
        .position(|item| item.title == label)
        .unwrap_or_else(|| panic!("Missing row {label:?} on {:?}", app.screen));
    let layout = super::super::list_rect(rows.len());
    let x = layout.x as f32 + 80.0;
    let y = layout.y as f32 + super::super::ROW_HEIGHT as f32 * row as f32 + 30.0;
    assert_eq!(app.input(Input::Tap(x, y)), Change::Partial);
}

fn mount_marks(app: &mut Reader, fixture: &Fixture, pdf: &std::path::Path) -> (u64, u64) {
    app.open_path(&fixture.book).unwrap();
    app.activate(Action::AddBookmark).unwrap();
    let epub_id = app.open_book_id().unwrap();
    let active = app.active.as_mut().unwrap();
    active.reading.jump(1, Some("door")).unwrap();
    let start = active.reading.probe.fragment("door").unwrap();
    let mut end = start.clone();
    end.offset += "At dusk the door".len();
    let selection = active.reading.probe.select_range(&start, &end).unwrap();
    let state = app.library.state_mut(epub_id).unwrap();
    state.author = Some("Zani Rivers".into());
    state.bookmarks[0].created_at = Some(1_740_777_900);
    state.highlights.push(Highlight {
        selection,
        note: Some("Review the Ámber chart".into()),
        style: HighlightStyle::Underline,
        created_at: Some(1_730_777_900),
    });
    app.open_path(pdf).unwrap();
    let fixed = app.fixed.as_ref().unwrap();
    let layer = fixed.reading.page_text(1).unwrap();
    let word = layer.words.first().unwrap();
    let id = fixed.id;
    let state = app.library.state_mut(id).unwrap();
    state.author = Some("Aria Coast".into());
    state
        .add_pdf_highlight(PdfHighlight {
            style: HighlightStyle::Lighten,
            page: 1,
            bounds: vec![word.bounds],
            continuation: Vec::new(),
            text: layer.text[word.bytes.clone()].to_owned(),
            note: Some("Harbor sounding".into()),
            created_at: None,
        })
        .unwrap();
    app.library.create_collection("Field reports").unwrap();
    app.library
        .set_collection_membership("Field reports", epub_id, true)
        .unwrap();
    app.library
        .set_collection_membership("Field reports", id, true)
        .unwrap();
    (epub_id, id)
}

#[test]
fn browser_covers_sources_date_text_filters_preview_and_exact_return() {
    let fixture = Fixture::new();
    let folder = fixture.root.join("Surveys");
    fs::create_dir(&folder).unwrap();
    let pdf = folder.join("Log sheets.pdf");
    fs::write(&pdf, crate::fixed::tests::document()).unwrap();
    let font = crate::test_fixture("fonts/NotoSans-Regular.ttf");
    let fonts = vec![font];
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
    let (epub_id, pdf_id) = mount_marks(&mut app, &fixture, &pdf);
    let activate = |app: &mut Reader, command| app.activate_bookmark_browser(command).unwrap();
    let id = |app: &Reader, n: usize| app.bookmark_browser.marks[n].book;
    let layout = Layout::new(WIDTH, HEIGHT, 4, 5);
    let back = primitive_ui::back_button(WIDTH as i32);
    let at = app.current_history_location().unwrap();
    app.show(Screen::OrganizeLibrary);
    choose(&mut app, "Bookmark browser");
    choose(&mut app, "History");
    assert_eq!(app.bookmark_browser.marks.len(), 3);
    assert_eq!(id(&app, 0), pdf_id);
    assert_eq!(id(&app, 2), epub_id);
    assert_eq!(app.current_history_location(), Some(at.clone()));
    capture_test_screen(&app, &fonts, "READER_BOOKMARK_BROWSER_SCREENSHOT", false);
    choose(&mut app, "Harbor sounding");
    assert_eq!(app.screen, Screen::BookmarkBrowser(Page::Detail));
    assert_eq!(app.current_history_location(), Some(at.clone()));
    assert_eq!(app.input(Input::PageForward), Change::Partial);
    assert_eq!(app.bookmark_browser.focus.unwrap().book, epub_id);
    assert_eq!(app.input(Input::PageBack), Change::Partial);
    assert_eq!(app.bookmark_browser.focus.unwrap().book, pdf_id);
    capture_test_screen(&app, &fonts, "READER_BOOKMARK_DETAIL_SCREENSHOT", false);
    tap(&mut app, READ);
    assert_eq!(app.screen, Screen::Fixed);
    assert_eq!(app.fixed.as_ref().unwrap().reading.page(), 1);
    app.activate(Action::ReturnToLocation).unwrap();
    assert_eq!(app.current_history_location(), Some(at));

    activate(&mut app, Command::Open);
    choose(&mut app, "Documents");
    assert_eq!(app.bookmark_browser.marks.len(), 2);
    tap(&mut app, SOURCES);
    choose(&mut app, "Documents and subfolders");
    assert_eq!(app.bookmark_browser.marks.len(), 3);
    tap(&mut app, SOURCES);
    choose(&mut app, "Choose a folder");
    choose(&mut app, "Surveys");
    choose(&mut app, "Use this folder");
    assert_eq!(app.bookmark_browser.marks.len(), 1);
    assert_eq!(id(&app, 0), pdf_id);
    tap(&mut app, SOURCES);
    choose(&mut app, "Collections");
    choose(&mut app, "Field reports");
    choose(&mut app, "Apply collections");
    assert_eq!(app.bookmark_browser.marks.len(), 3);
    tap(&mut app, FILTERS);
    choose(&mut app, "Filter by book");
    choose(&mut app, "Select by author");
    choose(&mut app, "Zani Rivers");
    choose(&mut app, "Apply book selection");
    assert_eq!(app.bookmark_browser.marks.len(), 2);
    assert!(
        app.bookmark_browser
            .marks
            .iter()
            .all(|mark| mark.book == epub_id)
    );
    tap(&mut app, FILTERS);
    choose(&mut app, "Clear filters");
    choose(&mut app, "Date added");
    assert_eq!(app.bookmark_browser.marks[1].locator, Locator::Reflow(0));
    choose(&mut app, "Sort direction");
    assert_eq!(app.bookmark_browser.marks[1].locator, Locator::Bookmark(0));
    choose(&mut app, "Note");
    assert_eq!(app.bookmark_browser.marks.len(), 2);
    activate(&mut app, Command::SetStyle(Some(HighlightStyle::Underline)));
    assert_eq!(id(&app, 0), epub_id);
    tap(&mut app, back);
    tap(&mut app, FIND);
    let (_, go) = layout
        .keyboard_keys_with(app.editor.layer, "Find")
        .into_iter()
        .find(|(key, _)| key.key == primitive_ui::Key::Go)
        .unwrap();
    for (query, count) in [("ámber", 1), ("door Review", 0), ("DUSK", 1)] {
        app.editor.text = query.chars().collect();
        app.editor.caret = app.editor.text.len();
        tap(&mut app, go);
        assert_eq!(app.bookmark_browser.marks.len(), count);
        tap(&mut app, FIND);
    }
    tap(&mut app, CASE);
    tap(&mut app, go);
    assert!(app.bookmark_browser.marks.is_empty());
    activate(&mut app, Command::ClearFilters);
    app.save().unwrap();

    fs::remove_file(&pdf).unwrap();
    activate(&mut app, Command::Use(Source::Archive));
    assert_eq!(app.bookmark_browser.marks.len(), 1);
    assert_eq!(id(&app, 0), pdf_id);
    choose(&mut app, "Harbor sounding");
    let (x, y) = center(READ);
    assert_eq!(app.input(Input::Tap(x, y)), Change::None);
    tap(&mut app, back);
    tap(&mut app, SOURCES);
    let (x, y) = (130.0, super::super::LIST_Y as f32 + 35.0);
    assert_eq!(app.input(Input::LongTap(x, y)), Change::Partial);
    let mut reopened = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
    activate(&mut reopened, Command::Open);
    assert_eq!(reopened.screen, Screen::BookmarkBrowser(Page::Results));
    assert_eq!(reopened.bookmark_browser.source, Source::History);
    assert_eq!(reopened.bookmark_browser.marks.len(), 2);
}

#[test]
fn password_prompt_keeps_the_mark_pending_until_authentication_or_cancel() {
    let fixture = Fixture::new();
    let pdf = fixture.root.join("Weather survey.pdf");
    fs::write(
        &pdf,
        include_bytes!("../../../tests/fixtures/pdf-password/r6-long.pdf"),
    )
    .unwrap();
    let font = crate::test_fixture("fonts/NotoSans-Regular.ttf");
    let mut app = Reader::open(&fixture.root, &fixture.state, vec![font]).unwrap();
    let entry = app.library.resolve(&pdf).unwrap();
    let secret = format!("estuary{}river", "n".repeat(88));
    app.open_with_password(entry, Some(&secret)).unwrap();
    app.activate(Action::AddBookmark).unwrap();
    let id = app.open_book_id().unwrap();
    app.open_path(&fixture.book).unwrap();
    let origin = app.current_history_location().unwrap();
    app.activate_bookmark_browser(Command::Open).unwrap();
    app.activate_bookmark_browser(Command::Use(Source::History))
        .unwrap();
    let key = *app
        .bookmark_browser
        .marks
        .iter()
        .find(|key| key.book == id)
        .unwrap();
    app.activate_bookmark_browser(Command::Inspect(key))
        .unwrap();
    tap(&mut app, READ);
    assert_eq!(app.screen, Screen::PdfPassword);
    assert_eq!(app.bookmark_browser.after_password, Some(key));
    app.input(Input::PageBack);
    assert_eq!(app.screen, Screen::BookmarkBrowser(Page::Detail));
    assert_eq!(app.current_history_location(), Some(origin));
    assert_eq!(app.bookmark_browser.after_password, None);
    tap(&mut app, READ);
    let go = Layout::new(WIDTH, HEIGHT, 4, 5)
        .keyboard_keys_with(app.editor.layer, "Open")
        .into_iter()
        .find(|(key, _)| key.key == primitive_ui::Key::Go)
        .unwrap()
        .1;
    for (password, screen) in [
        ("wrong", Screen::PdfPassword),
        (secret.as_str(), Screen::Fixed),
    ] {
        app.editor.text = password.chars().collect();
        app.editor.caret = app.editor.text.len();
        tap(&mut app, go);
        assert_eq!(app.screen, screen);
    }
    assert_eq!(app.open_book_id(), Some(id));
    assert_eq!(app.fixed.as_ref().unwrap().reading.page(), 0);
    assert_eq!(app.bookmark_browser.after_password, None);
    app.save().unwrap();
    let disk = fs::read_to_string(&fixture.state).unwrap();
    assert!(!disk.contains(&secret));
}

#[test]
#[ignore = "Requires an explicitly selected list of authorized original Documents files"]
fn original_documents_restore_bookmarks_through_the_browser() {
    let fixture = Fixture::new();
    let root = std::env::var_os("READER_BOOKMARK_ORIGINAL_ROOT").expect("Original root");
    let list = std::env::var_os("READER_BOOKMARK_ORIGINAL_LIST").expect("Original list");
    let root = std::path::Path::new(&root);
    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut app = Reader::open(root, &fixture.state, fonts).unwrap();
    let sources = fs::read_to_string(list).unwrap();
    assert!(!sources.trim().is_empty());
    for filename in sources.lines() {
        let source = root.join(filename);
        let before = fs::read(&source).unwrap();
        app.open_path(&source).unwrap();
        assert!(matches!(app.screen, Screen::Book | Screen::Fixed));
        let first = app.current_history_location().unwrap();
        app.input(Input::PageForward);
        let due = if let Some(active) = &app.active {
            active.reading.deadline()
        } else {
            app.fixed.as_ref().unwrap().reading.deadline()
        };
        if let Some(due) = due {
            app.tick(due);
        }
        let saved = app.current_position().unwrap();
        assert_ne!(saved, first.position);
        app.activate(Action::AddBookmark).unwrap();
        let id = app.open_book_id().unwrap();
        if app.fixed.is_some() {
            app.follow_fixed(0, None).unwrap();
        } else {
            app.follow_reflow_page(0, 0).unwrap();
        }
        let origin = app.current_history_location().unwrap();
        app.activate_bookmark_browser(Command::Open).unwrap();
        app.activate_bookmark_browser(Command::Use(Source::Books(BTreeSet::from([id]))))
            .unwrap();
        let key = *app
            .bookmark_browser
            .marks
            .iter()
            .find(|key| matches!(key.locator, Locator::Bookmark(_)))
            .unwrap();
        app.activate_bookmark_browser(Command::Inspect(key))
            .unwrap();
        assert_eq!(app.current_history_location(), Some(origin.clone()));
        tap(&mut app, READ);
        assert_eq!(app.current_position(), Some(saved));
        app.activate(Action::ReturnToLocation).unwrap();
        assert_eq!(app.current_history_location(), Some(origin));
        app.save().unwrap();
        assert_eq!(fs::read(source).unwrap(), before);
    }
}
