use super::*;
use crate::app::tests::{Fixture, capture_test_screen};
use std::os::unix::fs::{PermissionsExt, symlink};
use std::time::{Duration, SystemTime};

fn reader(documents: &Path, state: &Path) -> Reader {
    Reader::open(
        documents,
        state,
        vec![
            crate::test_fixture("fonts/NotoSans-Regular.ttf"),
            crate::test_fixture("fonts/NotoSans-Bold.ttf"),
        ],
    )
    .unwrap()
}

fn choose(app: &mut Reader, title: &str) {
    let index = app
        .items()
        .iter()
        .position(|item| item.title == title)
        .unwrap_or_else(|| panic!("Missing metadata item {title:?} {:?}", app.screen));
    let geometry = app.list_geometry();
    app.list_page = index / geometry.rows;
    let row = geometry.row(index % geometry.rows);
    assert_eq!(
        app.input(Input::Tap(
            (row.x + row.w / 2) as f32,
            (row.y + row.h / 2) as f32
        )),
        Change::Partial
    );
}

fn store_for(state: &Path) -> PathBuf {
    state
        .parent()
        .unwrap()
        .join(".book-metadata.sdr")
        .join(state.file_name().unwrap())
}

fn hash_record(state: &Path, id: u64) -> PathBuf {
    let hex = format!("{id:016x}");
    store_for(state)
        .join("hashdocsettings")
        .join(&hex[..2])
        .join(format!("{hex}.sdr"))
        .join("primitive-reader.json")
}

fn filename(id: u64) -> String {
    format!("primitive-reader-{id:016x}.json")
}

#[test]
fn live_metadata_restores_and_moves_between_three_locations_through_visible_controls() {
    let fixture = Fixture::new();
    let documents = fixture.root.join("Documents");
    fs::create_dir(&documents).unwrap();
    let original = documents.join("Field Notes.epub");
    fs::rename(&fixture.book, &original).unwrap();
    let bytes = fs::read(&original).unwrap();
    let mut initial = reader(&documents, &fixture.state);
    initial.open_path(&original).unwrap();
    initial.follow_reflow_page(1, 0).unwrap();
    initial.activate(Action::ToggleBookmark).unwrap();
    initial.save().unwrap();
    let location = initial.current_position().unwrap();
    let id = initial.open_book_id().unwrap();
    let title = initial.library.state(id).unwrap().title.clone();
    let profile_record = store_for(&fixture.state)
        .join("docsettings/Field Notes.sdr")
        .join(filename(id));
    assert!(profile_record.is_file());
    assert_eq!(initial.library.live_metadata_location(), Location::Profile);
    initial.show(Screen::ReadingControls);
    choose(&mut initial, "Book metadata location");
    choose(&mut initial, "About book metadata location");
    capture_test_screen(
        &initial,
        &initial.fonts,
        "READER_LIVE_METADATA_ABOUT",
        false,
    );
    initial.back_from_live_metadata(Page::About);
    choose(&mut initial, "Book folder");
    let adjacent_folder = original.with_extension("sdr");
    assert!(adjacent_folder.join(filename(id)).is_file());
    assert!(!profile_record.exists());
    // A neighboring stock metadata file has an independent name and lifetime.
    let lua = b"return { total_pages = 99 }";
    fs::write(adjacent_folder.join("metadata.epub.lua"), lua).unwrap();
    drop(initial);

    let destination = documents.join("Shelf");
    fs::create_dir(&destination).unwrap();
    let moved = destination.join("Moved Field Notes.epub");
    fs::rename(&original, &moved).unwrap();
    let moved_adjacent = moved.with_extension("sdr");
    fs::rename(&adjacent_folder, &moved_adjacent).unwrap();
    let second_state = fixture.root.join("second-library.json");
    let mut second = reader(&documents, &second_state);
    second.open_path(&moved).unwrap();
    assert_eq!(second.current_position(), Some(location.clone()));
    assert!(second.page_is_bookmarked());
    second.show(Screen::ReadingControls);
    choose(&mut second, "Book metadata location");
    choose(&mut second, "By edition fingerprint");
    assert!(hash_record(&second_state, id).is_file());
    assert!(!moved_adjacent.join(filename(id)).exists());
    assert_eq!(
        fs::read(moved_adjacent.join("metadata.epub.lua")).unwrap(),
        lua
    );
    capture_test_screen(
        &second,
        &second.fonts,
        "READER_LIVE_METADATA_SETTINGS",
        false,
    );
    choose(&mut second, "Books with fingerprint metadata");
    capture_test_screen(&second, &second.fonts, "READER_LIVE_METADATA_HASHED", false);
    drop(second);

    let renamed = documents.join("Renamed Again.epub");
    fs::rename(&moved, &renamed).unwrap();
    fs::remove_file(&second_state).unwrap();
    let mut restored = reader(&documents, &second_state);
    assert!(restored.library.state(id).is_none());
    restored.show(Screen::ReadingData);
    choose(&mut restored, "Book metadata location");
    choose(&mut restored, "Books with fingerprint metadata");
    choose(&mut restored, &title);
    assert_eq!(restored.current_position(), Some(location));
    assert!(restored.page_is_bookmarked());
    assert_eq!(
        restored.library.state(id).unwrap().path,
        Path::new("Renamed Again.epub")
    );
    restored.show(Screen::ReadingControls);
    choose(&mut restored, "Book metadata location");
    choose(&mut restored, "Reader profile");
    assert!(!hash_record(&second_state, id).exists());
    assert!(
        store_for(&second_state)
            .join("docsettings/Renamed Again.sdr")
            .join(filename(id))
            .is_file()
    );
    assert_eq!(fs::read(renamed).unwrap(), bytes);
}

#[test]
fn live_metadata_failed_migration_keeps_prior_record_and_retries_without_following_links() {
    let fixture = Fixture::new();
    let mut app = reader(&fixture.root, &fixture.state);
    app.open_path(&fixture.book).unwrap();
    app.activate(Action::ToggleBookmark).unwrap();
    app.save().unwrap();
    let id = app.open_book_id().unwrap();
    let initial = store_for(&fixture.state)
        .join("docsettings/Field Notes.sdr")
        .join(filename(id));
    assert!(initial.is_file());
    let prior = fs::read(&initial).unwrap();
    let blocked = store_for(&fixture.state).join("hashdocsettings");
    let unrelated = fixture.root.join("Unrelated");
    fs::create_dir(&unrelated).unwrap();
    symlink(&unrelated, &blocked).unwrap();
    app.show(Screen::ReadingControls);
    choose(&mut app, "Book metadata location");
    choose(&mut app, "By edition fingerprint");
    assert!(app.notice.as_deref().unwrap().contains("metadata location"));
    assert_eq!(fs::read(&initial).unwrap(), prior);
    assert_eq!(fs::read_dir(&unrelated).unwrap().count(), 0);
    assert!(app.page_is_bookmarked());
    fs::remove_file(blocked).unwrap();
    app.activate(Action::SaveBookData).unwrap();
    assert!(hash_record(&fixture.state, id).is_file());
    assert!(!initial.exists());
    let reopened = reader(&fixture.root, &fixture.state);
    assert_eq!(reopened.library.live_metadata_location(), Location::Hash);
}

#[test]
fn live_metadata_prefers_live_index_then_newest_valid_alternate_and_preserves_a_newer_copy() {
    let fixture = Fixture::new();
    let mut source = reader(&fixture.root, &fixture.state);
    source.open_path(&fixture.book).unwrap();
    source.follow_reflow_page(1, 0).unwrap();
    source.activate(Action::ToggleBookmark).unwrap();
    source.save().unwrap();
    let id = source.open_book_id().unwrap();
    let saved = source.current_position().unwrap();
    let record_for = |state: &Path| {
        store_for(state)
            .join("docsettings/Field Notes.sdr")
            .join(filename(id))
    };
    let original = fs::read(record_for(&fixture.state)).unwrap();
    let mut alternate: serde_json::Value = serde_json::from_slice(&original).unwrap();
    alternate["book"]["bookmarks"] = serde_json::json!([]);
    let newer = fixture.book.with_extension("sdr").join(filename(id));
    fs::create_dir_all(newer.parent().unwrap()).unwrap();
    fs::write(&newer, serde_json::to_vec(&alternate).unwrap()).unwrap();
    let later = SystemTime::now() + Duration::from_secs(120);
    let stamp = |path: &Path, modified| {
        fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(modified))
            .unwrap();
    };
    stamp(&newer, later);
    drop(source);

    let mut local = reader(&fixture.root, &fixture.state);
    local.open_path(&fixture.book).unwrap();
    assert_eq!(local.current_position(), Some(saved.clone()));
    assert!(local.page_is_bookmarked());
    local.save().unwrap();
    assert!(
        newer.exists(),
        "newer alternate lost during earlier checkpoint"
    );

    let first = fixture.root.join("newest-reader.json");
    let oldest = record_for(&first);
    fs::create_dir_all(oldest.parent().unwrap()).unwrap();
    fs::write(&oldest, &original).unwrap();
    stamp(&oldest, SystemTime::now() - Duration::from_secs(120));
    let mut external = reader(&fixture.root, &first);
    external.open_path(&fixture.book).unwrap();
    assert_eq!(external.current_position(), Some(saved.clone()));
    assert!(!external.page_is_bookmarked());

    alternate["book"]["id"] = serde_json::json!(id ^ 1);
    fs::write(&newer, serde_json::to_vec(&alternate).unwrap()).unwrap();
    stamp(&newer, later);
    let second = fixture.root.join("valid-reader.json");
    let valid = record_for(&second);
    fs::create_dir_all(valid.parent().unwrap()).unwrap();
    fs::write(&valid, &original).unwrap();
    let mut recovered = reader(&fixture.root, &second);
    recovered.open_path(&fixture.book).unwrap();
    assert_eq!(recovered.current_position(), Some(saved));
    assert!(recovered.page_is_bookmarked());
}

#[test]
fn live_metadata_read_only_documents_falls_back_then_migrates_when_writable() {
    if unsafe { libc::geteuid() } == 0 {
        return; // Root bypasses the fixture's directory mode bits.
    }
    let fixture = Fixture::new();
    let docs = fixture.root.join("Read-only shelf");
    fs::create_dir(&docs).unwrap();
    let book = docs.join(fixture.book.file_name().unwrap());
    fs::rename(&fixture.book, &book).unwrap();
    let mut app = reader(&docs, &fixture.state);
    app.open_path(&book).unwrap();
    app.activate(Action::ToggleBookmark).unwrap();
    let id = app.open_book_id().unwrap();
    fs::set_permissions(&docs, fs::Permissions::from_mode(0o500)).unwrap();
    let result = app.activate(Action::LiveMetadata(Command::Select(Location::BookFolder)));
    let adjacent = book.with_extension("sdr").join(filename(id));
    let while_read_only = adjacent.exists();
    fs::set_permissions(&docs, fs::Permissions::from_mode(0o700)).unwrap();
    result.unwrap();
    let prior = store_for(&fixture.state)
        .join("docsettings/Field Notes.sdr")
        .join(filename(id));
    assert!(!while_read_only);
    assert!(prior.is_file());
    assert!(app.notice.is_none() || app.notice.as_deref() == Some("Page bookmarked"));
    app.activate(Action::ToggleBookmark).unwrap();
    app.save().unwrap();
    assert!(adjacent.is_file());
    assert!(!prior.exists());
}
