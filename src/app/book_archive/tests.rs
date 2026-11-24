use super::*;
use crate::app::tests::{Fixture, capture_test_screen, tap_reading_control};

fn reader(fixture: &Fixture) -> Reader {
    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    Reader::open(&fixture.root, &fixture.state, fonts).unwrap()
}

fn tap(rect: Rect) -> Input {
    Input::Tap((rect.x + rect.w / 2) as f32, (rect.y + rect.h / 2) as f32)
}

fn choose(reader: &mut Reader, title: &str) {
    let row = reader
        .items()
        .iter()
        .position(|item| item.title == title)
        .unwrap_or_else(|| panic!("Missing archive action {title}"));
    let geometry = reader.list_geometry();
    reader.list_page = row / geometry.rows;
    assert_eq!(
        reader.input(tap(geometry.row(row % geometry.rows))),
        Change::Partial
    );
}

fn confirm(reader: &mut Reader) {
    let sheet = Sheet::centered(VIEW, "", "", "");
    assert_eq!(
        reader.input(tap(sheet.button(SheetAction::Confirm))),
        Change::Partial
    );
}

#[test]
fn book_archive_pdf_chooses_folder_confirms_and_restores_reader_data() {
    let fixture = Fixture::new();
    let inbox = fixture.root.join("Inbox");
    let archive = fixture.root.join("Reading archive/2026");
    fs::create_dir(&inbox).unwrap();
    fs::create_dir_all(&archive).unwrap();
    let source = inbox.join("Field survey.pdf");
    let bytes = crate::fixed_session::tests::striped_pdf_bytes(5);
    fs::write(&source, &bytes).unwrap();
    let modified = fs::metadata(&source).unwrap().modified().unwrap();
    let mut app = reader(&fixture);
    app.open_path(&source).unwrap();
    app.follow_fixed(2, None).unwrap();
    app.activate(Action::ToggleBookmark).unwrap();
    let position = app.current_position().unwrap();
    let book = app.current_document().unwrap().0.clone();
    let id = app.open_book_id().unwrap();
    let state = app.library.track(&book, id);
    state.favorite = true;
    state.set_status(ReadingStatus::Finished);
    let percent = state.reading_percent;
    app.library.create_collection("Field studies").unwrap();
    app.library
        .set_collection_membership("Field studies", id, true)
        .unwrap();

    app.open_reading_edge(true, 630.0);
    assert_eq!(tap_reading_control(&mut app, "Library"), Change::Partial);
    assert_eq!(
        tap_reading_control(&mut app, "Move to archive"),
        Change::Partial
    );
    choose(&mut app, "Archive folder");
    choose(&mut app, "Reading archive");
    choose(&mut app, "2026");
    capture_test_screen(&app, &app.fonts, "READER_ARCHIVE_FOLDER", false);
    choose(&mut app, "Use this folder");
    assert_eq!(app.archive.folder, Path::new("Reading archive/2026"));
    choose(&mut app, "Move current book");
    assert!(app.archive.plan.is_some());
    let target = archive.join("Field survey.pdf");
    assert!(!target.exists());
    capture_test_screen(&app, &app.fonts, "READER_ARCHIVE_CONFIRM", false);
    assert_eq!(app.input(Input::PageBack), Change::Partial);
    assert!(app.archive.plan.is_none());
    assert!(source.is_file() && !target.exists());
    assert_eq!(app.current_position(), Some(position.clone()));
    choose(&mut app, "Move current book");
    confirm(&mut app);
    assert_eq!(app.screen, Screen::Library);
    assert!(app.open_book_id().is_none());
    assert_eq!(app.notice.as_deref(), Some("Book moved to archive"));
    assert!(!source.exists());
    assert_eq!(fs::read(&target).unwrap(), bytes);
    assert_eq!(fs::metadata(&target).unwrap().modified().unwrap(), modified);
    assert_eq!(app.library.matching("Field survey").count(), 1);
    capture_test_screen(&app, &app.fonts, "READER_ARCHIVE_DONE", false);

    let mut reopened = reader(&fixture);
    assert_eq!(reopened.archive.folder, Path::new("Reading archive/2026"));
    reopened.activate(Action::Continue).unwrap();
    assert_eq!(reopened.open_book_id(), Some(id));
    assert_eq!(reopened.current_position(), Some(position));
    assert!(reopened.page_is_bookmarked());
    let saved = reopened.library.state(id).unwrap();
    assert_eq!(
        saved.path,
        Path::new("Reading archive/2026/Field survey.pdf")
    );
    assert_eq!(saved.reading_percent, percent);
    assert_eq!(saved.status(), ReadingStatus::Finished);
    assert!(saved.favorite);
    assert!(
        reopened
            .library
            .collection("Field studies")
            .unwrap()
            .book_ids()
            .contains(&id)
    );
}

#[test]
fn book_archive_epub_creates_default_folder_and_preserves_canonical_anchor() {
    let fixture = Fixture::new();
    let mut app = reader(&fixture);
    app.open_path(&fixture.book).unwrap();
    app.follow_reflow_page(1, 0).unwrap();
    app.activate(Action::ToggleBookmark).unwrap();
    let position = app.current_position().unwrap();
    assert!(position.anchor.is_some());
    let id = app.open_book_id().unwrap();
    let supported = app.current_document().unwrap().0.path.clone();
    app.archive_command(Command::Open).unwrap();
    app.archive_command(Command::Move).unwrap();
    assert!(!fixture.root.join(DEFAULT_FOLDER).exists());
    confirm(&mut app);
    assert!(!fixture.book.exists());
    assert!(fixture.root.join(DEFAULT_FOLDER).join(&supported).is_file());
    let mut reopened = reader(&fixture);
    reopened.activate(Action::Continue).unwrap();
    assert_eq!(reopened.open_book_id(), Some(id));
    assert_eq!(reopened.current_position(), Some(position));
    assert!(reopened.page_is_bookmarked());
}

#[test]
fn book_archive_rechecks_competing_destinations_and_changed_source_after_confirmation() {
    let fixture = Fixture::new();
    let archive = fixture.root.join(DEFAULT_FOLDER);
    fs::create_dir(&archive).unwrap();
    let destination = archive.join("Field Notes.epub");
    let original = fs::read(&fixture.book).unwrap();
    let mut app = reader(&fixture);
    app.open_path(&fixture.book).unwrap();
    app.archive_command(Command::Open).unwrap();
    app.archive_command(Command::Move).unwrap();
    fs::write(&destination, b"Other document").unwrap();
    confirm(&mut app);
    assert!(app.notice.as_deref().unwrap().contains("already exists"));
    assert_eq!(app.screen, Screen::BookArchive);
    assert_eq!(fs::read(&fixture.book).unwrap(), original);
    assert_eq!(fs::read(&destination).unwrap(), b"Other document");
    assert!(
        app.archive_command(Command::Move)
            .unwrap_err()
            .to_string()
            .contains("already exists")
    );
    fs::remove_file(&destination).unwrap();

    app.archive_command(Command::Move).unwrap();
    let replacement = vec![b'X'; original.len()];
    fs::write(&fixture.book, &replacement).unwrap();
    confirm(&mut app);
    assert!(app.notice.as_deref().unwrap().contains("book changed"));
    assert_eq!(fs::read(&fixture.book).unwrap(), replacement);
    assert!(!destination.exists());
    assert_eq!(app.screen, Screen::BookArchive);
}

#[test]
fn book_archive_does_not_follow_a_replaced_destination_folder() {
    use std::os::unix::fs::symlink;

    let fixture = Fixture::new();
    let archive = fixture.root.join(DEFAULT_FOLDER);
    fs::create_dir(&archive).unwrap();
    let rest = fixture.root.join("Replacement folder");
    fs::create_dir(&rest).unwrap();
    let mut app = reader(&fixture);
    app.open_path(&fixture.book).unwrap();
    app.archive_command(Command::Open).unwrap();
    app.archive_command(Command::Move).unwrap();
    fs::rename(&archive, fixture.root.join("Original archive")).unwrap();
    symlink(&rest, &archive).unwrap();
    confirm(&mut app);
    assert_eq!(app.screen, Screen::BookArchive);
    assert!(app.notice.is_some());
    assert!(fixture.book.is_file());
    assert!(fs::read_dir(rest).unwrap().next().is_none());
}

#[test]
fn book_archive_persistence_failure_cleans_only_the_exclusive_copy() {
    let fixture = Fixture::new();
    fs::create_dir(fixture.root.join(DEFAULT_FOLDER)).unwrap();
    let mut app = reader(&fixture);
    app.open_path(&fixture.book).unwrap();
    app.save().unwrap();
    let (book, id) = app.current_document().unwrap();
    let original = fs::read(&fixture.book).unwrap();
    let plan = app
        .library
        .archive_plan(book, id, Path::new(DEFAULT_FOLDER))
        .unwrap();
    let error = app
        .library
        .archive_with(
            &plan,
            |_, _| Err(io::Error::other("Simulated save failure")),
            |_| panic!("The source must survive a failed state write"),
        )
        .err()
        .unwrap();
    assert!(error.to_string().contains("save failure"));
    let target = fixture.root.join(&plan.destination);
    assert!(!target.exists());
    assert_eq!(fs::read(&fixture.book).unwrap(), original);
    let error = app
        .library
        .archive_with(
            &plan,
            |_, _| {
                fs::write(&target, b"Updated archive file")?;
                Err(io::Error::other(
                    "Simulated save failure after in-place edit",
                ))
            },
            |_| panic!("The source must survive a failed state write"),
        )
        .err()
        .unwrap();
    assert!(error.to_string().contains("in-place edit"));
    assert_eq!(fs::read(&target).unwrap(), b"Updated archive file");
    assert_eq!(fs::read(&fixture.book).unwrap(), original);
    fs::remove_file(&target).unwrap();
    let archived_copy = target.with_file_name("Previous copy.epub");
    let error = app
        .library
        .archive_with(
            &plan,
            |_, _| {
                fs::rename(&target, &archived_copy)?;
                fs::write(&target, b"Other user's file")?;
                Err(io::Error::other("Simulated save failure after replacement"))
            },
            |_| panic!("The source must survive a failed state write"),
        )
        .err()
        .unwrap();
    assert!(error.to_string().contains("after replacement"));
    assert_eq!(fs::read(&target).unwrap(), b"Other user's file");
    assert_eq!(fs::read(archived_copy).unwrap(), original);
    assert_eq!(fs::read(&fixture.book).unwrap(), original);
    let mut reopened = reader(&fixture);
    reopened.activate(Action::Continue).unwrap();
    assert_eq!(
        reopened.current_document().unwrap().0.path,
        plan.source.path
    );
}

#[test]
fn book_archive_keeps_both_files_and_continues_the_saved_archive_on_removal_failure() {
    let fixture = Fixture::new();
    fs::create_dir(fixture.root.join(DEFAULT_FOLDER)).unwrap();
    let mut app = reader(&fixture);
    app.open_path(&fixture.book).unwrap();
    app.follow_reflow_page(1, 0).unwrap();
    app.activate(Action::ToggleBookmark).unwrap();
    let position = app.current_position();
    app.save().unwrap();
    let (book, id) = app.current_document().unwrap();
    let original = fs::read(&fixture.book).unwrap();
    let plan = app
        .library
        .archive_plan(book, id, Path::new(DEFAULT_FOLDER))
        .unwrap();
    let result = app
        .library
        .archive_with(&plan, primitive_app::save, |_| {
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Simulated removal failure",
            ))
        })
        .unwrap();
    assert!(result.source_kept);
    assert_eq!(fs::read(&fixture.book).unwrap(), original);
    assert_eq!(
        fs::read(fixture.root.join(&plan.destination)).unwrap(),
        original
    );
    let mut reopened = reader(&fixture);
    reopened.activate(Action::Continue).unwrap();
    assert_eq!(reopened.open_book_id(), Some(id));
    assert_eq!(reopened.current_position(), position);
    assert_eq!(
        reopened.current_document().unwrap().0.path,
        plan.destination
    );
    assert!(reopened.page_is_bookmarked());
}

#[test]
fn book_archive_preserves_a_source_replacement_after_the_copy_is_committed() {
    let fixture = Fixture::new();
    fs::create_dir(fixture.root.join(DEFAULT_FOLDER)).unwrap();
    let mut app = reader(&fixture);
    app.open_path(&fixture.book).unwrap();
    app.save().unwrap();
    let (book, id) = app.current_document().unwrap();
    let original = fs::read(&fixture.book).unwrap();
    let plan = app
        .library
        .archive_plan(book, id, Path::new(DEFAULT_FOLDER))
        .unwrap();
    let replacement = vec![b'X'; original.len()];
    let result = app
        .library
        .archive_with(
            &plan,
            |state, bytes| {
                primitive_app::save(state, bytes)?;
                fs::write(&fixture.book, &replacement)
            },
            |_| panic!("The replacement source must survive"),
        )
        .unwrap();
    assert!(result.source_kept);
    assert_eq!(fs::read(&fixture.book).unwrap(), replacement);
    assert_eq!(
        fs::read(fixture.root.join(&plan.destination)).unwrap(),
        original
    );
    let mut reopened = reader(&fixture);
    reopened.activate(Action::Continue).unwrap();
    assert_eq!(
        reopened.current_document().unwrap().0.path,
        plan.destination
    );
}
