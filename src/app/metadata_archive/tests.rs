use super::*;
use crate::app::tests::{Fixture, capture_test_screen};
use primitive_app::App;

fn reader(fixture: &Fixture, state: &Path) -> Reader {
    reader_in(&fixture.root, state)
}

fn reader_in(documents: &Path, state: &Path) -> Reader {
    Reader::open(
        documents,
        state,
        vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")],
    )
    .unwrap()
}

fn tap(rect: Rect) -> Input {
    Input::Tap((rect.x + rect.w / 2) as f32, (rect.y + rect.h / 2) as f32)
}

fn choose(app: &mut Reader, title: &str) {
    let row = app
        .items()
        .iter()
        .position(|item| item.title == title)
        .unwrap_or_else(|| panic!("Missing archive action {title}"));
    let geometry = app.list_geometry();
    app.list_page = row / geometry.rows;
    assert_eq!(
        app.input(tap(geometry.row(row % geometry.rows))),
        Change::Partial
    );
}

fn setup(app: &mut Reader) -> PathBuf {
    let folder = PathBuf::from("Reading archive");
    fs::create_dir(app.library.documents_path().join(&folder)).unwrap();
    app.show(Screen::ReadingControls);
    choose(app, "Book metadata archive");
    assert!(
        !app.items()
            .iter()
            .any(|item| item.title == "Archived books")
    );
    choose(app, "Archive folder");
    choose(app, "Reading archive");
    choose(app, "Use this folder");
    folder
}

#[test]
fn metadata_archive_closes_lists_and_restores_a_moved_epub_in_a_fresh_profile() {
    let fixture = Fixture::new();
    let documents = fixture.root.join("Documents");
    fs::create_dir(&documents).unwrap();
    let original_book = documents.join(fixture.book.file_name().unwrap());
    fs::rename(&fixture.book, &original_book).unwrap();
    let mut app = reader_in(&documents, &fixture.state);
    let folder = setup(&mut app);
    choose(&mut app, "Save metadata on book close");
    capture_test_screen(&app, &app.fonts, "READER_METADATA_ARCHIVE_SETTINGS", false);
    app.open_path(&original_book).unwrap();
    app.follow_reflow_page(1, 0).unwrap();
    app.activate(Action::ToggleBookmark).unwrap();
    let position = app.current_position().unwrap();
    let id = app.open_book_id().unwrap();
    let bytes = fs::read(&original_book).unwrap();
    assert!(app.library.archived_book(&folder, id).unwrap().is_none());

    let second = documents.join("Field Survey.pdf");
    fs::write(&second, crate::fixed_session::tests::striped_pdf_bytes(4)).unwrap();
    app.open_path(&second).unwrap();
    assert!(app.library.archived_book(&folder, id).unwrap().is_some());
    let second_id = app.open_book_id().unwrap();
    app.save().unwrap();
    assert!(
        app.library
            .archived_book(&folder, second_id)
            .unwrap()
            .is_none()
    );
    assert_eq!(app.exit(), Change::Exit);
    assert!(
        app.library
            .archived_book(&folder, second_id)
            .unwrap()
            .is_some()
    );
    app.show(Screen::ReadingData);
    choose(&mut app, "Book metadata archive");
    choose(&mut app, "Archived books");
    capture_test_screen(&app, &app.fonts, "READER_METADATA_ARCHIVE_LIST", false);
    let row = app.items().into_iter().find(|item| matches!(item.action, Action::MetadataArchive(Command::Entry(row_id)) if row_id == id)).unwrap();
    choose(&mut app, &row.title);
    assert!(
        app.items()
            .iter()
            .any(|item| item.detail.as_deref() == Some("Bookmark"))
    );

    let moved = documents.join("Restored Field Notes.epub");
    fs::rename(&original_book, &moved).unwrap();
    let mut fresh = reader_in(&documents, &fixture.root.join("clean-library.json"));
    fresh.open_path(&moved).unwrap();
    assert!(fresh.metadata_archive.waiting_to_restore());
    assert!(fresh.open_book_id().is_none());
    capture_test_screen(
        &fresh,
        &fresh.fonts,
        "READER_METADATA_ARCHIVE_RESTORE",
        false,
    );
    let confirm = fresh
        .metadata_archive_sheet()
        .unwrap()
        .button(SheetAction::Confirm);
    assert_eq!(fresh.input(tap(confirm)), Change::Partial);
    assert_eq!(fresh.open_book_id(), Some(id));
    assert_eq!(fresh.current_position(), Some(position));
    assert!(fresh.page_is_bookmarked());
    assert_eq!(
        fresh.library.state(id).unwrap().path,
        Path::new("Restored Field Notes.epub")
    );
    assert_eq!(fs::read(&moved).unwrap(), bytes);
    assert!(fresh.library.archived_book(&folder, id).unwrap().is_none());
    assert_eq!(fresh.exit(), Change::Exit); // A later close creates a new recoverable copy.
    assert!(fresh.library.archived_book(&folder, id).unwrap().is_some());

    let mut next = reader_in(&documents, &fixture.root.join("no-restore-library.json"));
    next.open_path(&moved).unwrap();
    assert!(next.metadata_archive.waiting_to_restore());
    assert_eq!(next.input(Input::PageBack), Change::Partial);
    assert!(next.open_book_id().is_none());
    next.open_path(&moved).unwrap();
    let cancel = next
        .metadata_archive_sheet()
        .unwrap()
        .button(SheetAction::Cancel);
    assert_eq!(next.input(tap(cancel)), Change::Partial);
    assert_eq!(next.open_book_id(), Some(id));
    assert!(next.library.state(id).unwrap().bookmarks.is_empty());
    assert!(next.library.archived_book(&folder, id).unwrap().is_none());
}

fn confirm_delete(app: &mut Reader) {
    let end_panel_x = (WIDTH as i32 - 864) / 2;
    let end_panel_y = (HEIGHT as i32 - 632) / 2;
    let delete_button = Rect::new(end_panel_x + 40, end_panel_y + 372, 382, 100);
    app.input(Input::PageForward);
    if let Some(deadline) = app
        .fixed
        .as_ref()
        .and_then(|fixed| fixed.reading.deadline())
    {
        app.tick(deadline);
    }
    assert!(app.end_dialog.is_some());
    assert_eq!(app.input(tap(delete_button)), Change::Partial);
    let sheet = Sheet::centered(Rect::new(0, 0, WIDTH as i32, HEIGHT as i32), "", "", "");
    assert_eq!(
        app.input(tap(sheet.button(SheetAction::Confirm))),
        Change::Partial
    );
}

#[test]
fn metadata_archive_before_delete_preserves_marks_and_remove_keeps_live_state() {
    let fixture = Fixture::new();
    let mut app = reader(&fixture, &fixture.state);
    let folder = setup(&mut app);
    let pdf = fixture.root.join("Field Survey.pdf");
    let bytes = crate::fixed_session::tests::striped_pdf_bytes(5);
    fs::write(&pdf, &bytes).unwrap();
    app.open_path(&pdf).unwrap();
    app.follow_fixed(2, None).unwrap();
    app.activate(Action::ToggleBookmark).unwrap();
    let id = app.open_book_id().unwrap();
    let position = app.current_position().unwrap();
    let source = app.current_document().unwrap().0.clone();
    assert!(!app.metadata_archive.preference.as_ref().unwrap().on_close);

    let archive = fixture
        .root
        .join(&folder)
        .join("primitive-reader-book-metadata.json");
    fs::write(&archive, "invalid data").unwrap();
    app.follow_fixed(4, None).unwrap();
    confirm_delete(&mut app);
    assert!(pdf.is_file());
    assert_eq!(fs::read(&archive).unwrap(), b"invalid data");
    fs::remove_file(&archive).unwrap();
    confirm_delete(&mut app);
    assert!(!pdf.exists());
    assert!(app.library.state(id).is_some());
    let saved = app.library.archived_book(&folder, id).unwrap().unwrap();
    assert!(saved.saved_on == SavedOn::Delete);
    assert_eq!(saved.book.bookmarks[0].position, position);
    app.show(Screen::ReadingControls);
    choose(&mut app, "Book metadata archive");
    choose(&mut app, "Archived books");
    let title = app.items()[0].title.clone();
    choose(&mut app, &title);
    choose(&mut app, "Remove saved copy");
    let confirm = app
        .metadata_archive_sheet()
        .unwrap()
        .button(SheetAction::Confirm);
    assert_eq!(app.input(tap(confirm)), Change::Partial);
    assert!(app.library.archived_book(&folder, id).unwrap().is_none());
    assert_eq!(
        app.library.state(id).unwrap().bookmarks[0].position,
        position
    );
    fs::write(&pdf, &bytes).unwrap();
    assert_eq!(app.library.identify(&source).unwrap(), id);
    app.open_path(&pdf).unwrap();
    assert!(!app.metadata_archive.waiting_to_restore());
    assert_eq!(app.open_book_id(), Some(id));
}

#[test]
fn metadata_archive_consumption_preserves_a_newer_copy() {
    let fixture = Fixture::new();
    let mut writer = reader(&fixture, &fixture.state);
    let folder = setup(&mut writer);
    writer.open_path(&fixture.book).unwrap();
    writer.activate(Action::ToggleBookmark).unwrap();
    let id = writer.open_book_id().unwrap();
    writer.save_metadata_archive(id, SavedOn::Delete).unwrap();
    let original = writer.library.archived_book(&folder, id).unwrap().unwrap();
    let mut fresh = reader(&fixture, &fixture.root.join("clean-library.json"));
    fresh.open_path(&fixture.book).unwrap();
    assert!(fresh.metadata_archive.waiting_to_restore());
    writer.follow_reflow_page(1, 0).unwrap();
    writer.save_metadata_archive(id, SavedOn::Delete).unwrap();
    let newer = writer.library.archived_book(&folder, id).unwrap().unwrap();
    assert_ne!(newer.revision, original.revision);
    let confirm = fresh
        .metadata_archive_sheet()
        .unwrap()
        .button(SheetAction::Confirm);
    assert_eq!(fresh.input(tap(confirm)), Change::Partial);
    assert_eq!(fresh.open_book_id(), Some(id));
    assert!(fresh.page_is_bookmarked());
    assert_eq!(
        fresh
            .library
            .archived_book(&folder, id)
            .unwrap()
            .unwrap()
            .revision,
        newer.revision
    );
}
