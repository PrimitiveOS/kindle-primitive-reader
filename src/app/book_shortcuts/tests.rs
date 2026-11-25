use super::*;
use crate::app::gestures::Gesture;
use crate::app::tests::{Fixture, capture_test_screen};
use primitive_app::{App, HEIGHT, WIDTH};
use std::fs;

fn reader(fixture: &Fixture) -> Reader {
    let fonts = vec![
        crate::test_fixture("fonts/NotoSans-Regular.ttf"),
        crate::test_fixture("fonts/NotoSans-Bold.ttf"),
    ];
    Reader::open(&fixture.root, &fixture.state, fonts).unwrap()
}

fn row(app: &mut Reader, title: &str, hold: bool) -> Change {
    let index = app
        .items()
        .iter()
        .position(|item| item.title == title)
        .unwrap_or_else(|| panic!("Missing shortcut row {title}"));
    let geometry = app.list_geometry();
    app.list_page = index / geometry.rows;
    let row = geometry.row(index % geometry.rows);
    let x = (row.x + row.w / 2) as f32;
    let y = (row.y + row.h / 2) as f32;
    app.input(if hold {
        Input::LongTap(x, y)
    } else {
        Input::Tap(x, y)
    })
}

fn choose(app: &mut Reader, title: &str) {
    assert_eq!(row(app, title, false), Change::Partial);
}

fn back(app: &mut Reader) {
    let bounds = app.navigation_layout().back();
    assert_eq!(
        app.input(Input::Tap(
            (bounds.x + bounds.w / 2) as f32,
            (bounds.y + bounds.h / 2) as f32,
        )),
        Change::Partial
    );
}

fn shortcut_menu(app: &mut Reader) {
    let actions = app.top_config_items(super::super::top_config::TopGroup::Tools);
    let action = actions
        .into_iter()
        .find(|item| item.title == "Book shortcuts")
        .unwrap()
        .action;
    app.activate(action).unwrap();
}

#[test]
fn book_shortcuts_open_files_folders_and_the_last_book_in_the_selected_branch() {
    let fixture = Fixture::new();
    let shelf = Path::new("Field atlas");
    let sub = shelf.join("Charts");
    let similarly_named = Path::new("Field atlas extra");
    for folder in [sub.as_path(), similarly_named] {
        fs::create_dir_all(fixture.root.join(folder)).unwrap();
    }
    let direct = shelf.join("Night landing.epub");
    let deep = sub.join("Coastal dispatches.epub");
    let neighbor = similarly_named.join("Separate harbor.epub");
    for (source, target) in [
        ("night-landing.epub", &direct),
        ("long-chapter.epub", &deep),
        ("fast-pair-kerning.epub", &neighbor),
    ] {
        fs::copy(crate::test_fixture(source), fixture.root.join(target)).unwrap();
    }

    let mut app = reader(&fixture);
    app.show(Screen::OrganizeLibrary);
    choose(&mut app, "Book shortcuts");
    choose(&mut app, "New shortcut");
    choose(&mut app, "Field atlas");
    capture_test_screen(&app, &app.fonts, "READER_SHORTCUT_PICKER", false);
    choose(&mut app, "Add this folder");
    choose(&mut app, "New shortcut");
    choose(&mut app, "Field atlas");
    choose(&mut app, "Night landing.epub");
    assert_eq!(app.book_shortcuts.saved.shortcuts.len(), 2);
    capture_test_screen(&app, &app.fonts, "READER_SHORTCUTS", false);
    choose(&mut app, "Night landing.epub");
    assert_eq!(app.current_document().unwrap().0.library_path(), direct);
    assert_eq!(app.screen, Screen::Book);
    app.follow(super::super::Destination {
        path: "EPUB/chapters/notes.xhtml".to_owned(),
        fragment: Some("appendix".to_owned()),
    })
    .unwrap();
    let target = app.current_history_location().unwrap();

    shortcut_menu(&mut app);
    choose(&mut app, "Field atlas");
    assert_eq!(app.screen, Screen::Library);
    assert_eq!(app.browser.directory(), shelf);
    assert!(app.open_book_id().is_none());
    app.open_path(&deep).unwrap();
    app.open_path(&neighbor).unwrap();
    shortcut_menu(&mut app);
    choose(&mut app, "Folder action");
    choose(&mut app, "Open last-read book");
    back(&mut app);
    choose(&mut app, "Field atlas");
    assert_eq!(app.current_document().unwrap().0.library_path(), direct);
    assert_eq!(app.current_history_location().as_ref(), Some(&target));

    // Read the nested book again, then the latest neighboring book. Matching
    // path components keeps the neighboring folder out of the recursive result.
    app.open_path(&deep).unwrap();
    let nested_location = app.current_history_location().unwrap();
    app.open_path(&neighbor).unwrap();
    shortcut_menu(&mut app);
    choose(&mut app, "Folder action");
    choose(&mut app, "Include subfolders");
    capture_test_screen(&app, &app.fonts, "READER_SHORTCUT_FOLDER_ACTION", false);
    back(&mut app);
    choose(&mut app, "Field atlas");
    assert_eq!(app.current_document().unwrap().0.library_path(), deep);
    assert_eq!(
        app.current_history_location().as_ref(),
        Some(&nested_location)
    );

    app.save().unwrap();
    let mut reopened = reader(&fixture);
    reopened.show(Screen::OrganizeLibrary);
    choose(&mut reopened, "Book shortcuts");
    assert_eq!(
        reopened.book_shortcuts.saved.folder_action,
        FolderAction::LastBook
    );
    assert!(reopened.book_shortcuts.recursive());
    choose(&mut reopened, "Field atlas");
    assert_eq!(reopened.current_document().unwrap().0.library_path(), deep);
}

#[test]
fn book_shortcut_removal_keeps_its_file_and_does_not_retarget_an_assigned_gesture() {
    let fixture = Fixture::new();
    let name = "Coast notes.epub";
    let path = fixture.root.join(name);
    fs::copy(crate::test_fixture("night-landing.epub"), &path).unwrap();
    let bytes = fs::read(&path).unwrap();
    let mut app = reader(&fixture);
    app.open_path(&fixture.book).unwrap();
    shortcut_menu(&mut app);
    choose(&mut app, "New shortcut");
    choose(&mut app, name);
    let id = app.book_shortcuts.saved.shortcuts[0].id;
    let result = app.activate(Action::BookShortcut(Command::Add(
        Kind::Book,
        PathBuf::from(name),
    )));
    assert!(result.is_err());
    assert_eq!(app.book_shortcuts.saved.shortcuts.len(), 1);

    app.show(Screen::GestureBinding(Gesture::TapTopRight));
    choose(&mut app, "Open Coast notes.epub");
    app.show(Screen::Book);
    let corner = Input::Tap(WIDTH as f32 - 20.0, 20.0);
    assert_eq!(app.input(corner), Change::Partial);
    assert_eq!(
        app.current_document().unwrap().0.library_path(),
        Path::new(name)
    );
    app.save().unwrap();

    let mut reopened = reader(&fixture);
    reopened.open_path(&fixture.book).unwrap();
    assert_eq!(reopened.input(corner), Change::Partial);
    assert_eq!(
        reopened.current_document().unwrap().0.library_path(),
        Path::new(name)
    );
    shortcut_menu(&mut reopened);
    assert_eq!(row(&mut reopened, name, true), Change::Partial);
    capture_test_screen(&reopened, &reopened.fonts, "READER_SHORTCUT_REMOVE", false);
    assert_eq!(reopened.input(Input::PageBack), Change::Partial);
    assert_eq!(reopened.book_shortcuts.saved.shortcuts.len(), 1);
    assert_eq!(row(&mut reopened, name, true), Change::Partial);
    let confirm = reopened
        .book_shortcut_removal_sheet()
        .unwrap()
        .button(SheetAction::Confirm);
    assert_eq!(
        reopened.input(Input::Tap(
            (confirm.x + confirm.w / 2) as f32,
            (confirm.y + confirm.h / 2) as f32,
        )),
        Change::Partial
    );
    assert_eq!(fs::read(&path).unwrap(), bytes);
    assert!(reopened.book_shortcuts.saved.shortcuts.is_empty());
    choose(&mut reopened, "New shortcut");
    choose(&mut reopened, name);
    let replacement = reopened.book_shortcuts.saved.shortcuts[0].id;
    assert_ne!(id, replacement);
    reopened.open_path(&fixture.book).unwrap();
    assert_eq!(reopened.input(corner), Change::Partial);
    assert_eq!(
        reopened.notice.as_deref(),
        Some("This shortcut was removed")
    );
    assert_eq!(
        reopened.current_document().unwrap().0.library_path(),
        Path::new(fixture.book.file_name().unwrap())
    );
}

#[test]
fn missing_and_outside_shortcuts_leave_the_current_document_open() {
    let fixture = Fixture::new();
    let path = fixture.root.join("Temporary.epub");
    fs::copy(crate::test_fixture("night-landing.epub"), &path).unwrap();
    let mut app = reader(&fixture);
    app.open_path(&fixture.book).unwrap();
    let location = app.current_history_location();
    shortcut_menu(&mut app);
    choose(&mut app, "New shortcut");
    choose(&mut app, "Temporary.epub");
    let missing = path.with_file_name("Moved.epub");
    fs::rename(&path, &missing).unwrap();
    choose(&mut app, "Temporary.epub");
    assert_eq!(
        app.notice.as_deref(),
        Some("This shortcut's book or folder is unavailable")
    );
    assert_eq!(app.current_history_location(), location);
    assert!(app.items().iter().any(|item| item.title == "Temporary.epub"
        && item.detail.as_deref().unwrap().starts_with("Unavailable")));
    assert!(
        app.book_shortcuts
            .add(&app.library, Kind::Book, PathBuf::from("../Outside.epub"))
            .is_err()
    );
    let link = fixture.root.join("Linked.epub");
    std::os::unix::fs::symlink(&missing, &link).unwrap();
    assert!(
        app.book_shortcuts
            .add(&app.library, Kind::Book, PathBuf::from("Linked.epub"))
            .is_err()
    );
    assert_eq!(app.book_shortcuts.saved.shortcuts.len(), 1);
    assert_eq!(HEIGHT, app.reading_viewport().dimensions().1);
}

#[test]
#[ignore = "Requires an explicitly authorized local EPUB in READER_SHORTCUT_ORIGINAL"]
fn original_epub_reopens_at_the_saved_location_through_a_shortcut() {
    let source = PathBuf::from(std::env::var_os("READER_SHORTCUT_ORIGINAL").unwrap());
    let hash = |path: &Path| ring::digest::digest(&ring::digest::SHA256, &fs::read(path).unwrap());
    let original = hash(&source);
    let fixture = Fixture::new();
    let name = "Verification copy.epub";
    let copy = fixture.root.join(name);
    fs::copy(&source, &copy).unwrap();
    let mut app = reader(&fixture);
    app.show(Screen::OrganizeLibrary);
    choose(&mut app, "Book shortcuts");
    choose(&mut app, "New shortcut");
    choose(&mut app, name);
    choose(&mut app, name);
    let first = app.current_history_location().unwrap();
    app.input(Input::PageForward);
    for _ in 0..16 {
        if app.current_history_location().as_ref() != Some(&first) {
            break;
        }
        let deadline = app.deadline().expect("the page turn scheduled a deadline");
        app.tick(deadline + std::time::Duration::from_millis(1));
    }
    let later = app.current_history_location().unwrap();
    assert_ne!(later, first);
    app.save().unwrap();

    let mut reopened = reader(&fixture);
    reopened.show(Screen::OrganizeLibrary);
    choose(&mut reopened, "Book shortcuts");
    choose(&mut reopened, name);
    assert_eq!(reopened.current_history_location(), Some(later));
    assert_eq!(hash(&copy).as_ref(), original.as_ref());
    assert_eq!(hash(&source).as_ref(), original.as_ref());
}
