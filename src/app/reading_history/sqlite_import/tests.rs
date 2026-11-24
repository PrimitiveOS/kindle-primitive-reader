use super::*;
use crate::app::{
    Change, Input,
    selection_viewport_tests::{adopt, capture, tap},
    tests::{Fixture, capture_test_screen},
};
use crate::reading_stats::history::History;
use crate::session::ReadingViewport;
use primitive_app::App;
use std::time::{Duration, Instant};

const STOCK: &[u8] = include_bytes!("../../../../tests/fixtures/koreader-statistics/pdf.sqlite3");
const PDF: &[u8] = include_bytes!("../../../../tests/fixtures/scan-reflow/vector.pdf");
const STOCK_DJVU: &[u8] =
    include_bytes!("../../../../tests/fixtures/koreader-statistics/djvu.sqlite3");
const DJVU: &[u8] = include_bytes!("../../../../tests/fixtures/beacon-atlas.djvu");
const FIRST: i64 = 1_769_990_394;

fn setup() -> (Fixture, Reader, u64) {
    setup_book("Survey.pdf", PDF, STOCK, 1)
}

fn setup_book(name: &str, document: &[u8], database: &[u8], page: usize) -> (Fixture, Reader, u64) {
    let fixture = Fixture::new();
    let path = fixture.root.join(name);
    fs::write(&path, document).unwrap();
    fs::write(fixture.root.join(DATABASE), database).unwrap();
    let fonts = vec![
        crate::test_fixture("fonts/NotoSans-Regular.ttf"),
        crate::test_fixture("fonts/NotoSans-Bold.ttf"),
    ];
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
    app.open_path(&path).unwrap();
    app.jump_fixed(page, None).unwrap();
    let id = app.open_book_id().unwrap();
    (fixture, app, id)
}

#[test]
fn stock_djvu_sqlite_import_keeps_physical_pages_zoom_and_duplicates_across_djv_alias() {
    let (fixture, mut app, id) = setup_book("Beacon Atlas.djvu", DJVU, STOCK_DJVU, 2);
    app.activate(Action::SetFixedZoom(3)).unwrap();
    app.fixed
        .as_mut()
        .unwrap()
        .reading
        .restore_history_offsets(2, -250.0, 117.0)
        .unwrap();
    let reading = &app.fixed.as_ref().unwrap().reading;
    assert_eq!((reading.page(), reading.count()), (2, 6));
    let before = reading.preferences();
    let request = reading.page_viewport();
    let anchor = reading.history_point();
    let position = app.current_position();
    let source = fixture.root.join("Beacon Atlas.djvu");
    let database = fixture.root.join(DATABASE);
    let source_date = fs::metadata(&source).unwrap().modified().unwrap();
    let database_date = fs::metadata(&database).unwrap().modified().unwrap();

    preview(&mut app, id);
    let pending = app.history_view.koreader_import.as_ref().unwrap();
    assert_eq!(
        (pending.preview.new_visits, pending.preview.seconds),
        (4, 75)
    );
    assert_eq!(pending.preview.first_start, Some(FIRST));
    assert_eq!(pending.preview.last_start, Some(FIRST + 60));
    assert!(
        app.items()
            .iter()
            .any(|row| row.title == "Matched KOReader book"
                && row.detail.as_deref() == Some("Beacon Atlas"))
    );
    capture_test_screen(&app, &app.fonts, "READER_STATS_DJVU_IMPORT_SCREEN", false);

    // Saving the Reader journal must never follow the old shared temporary path.
    let old_temp = fixture.state.with_file_name("reading-history.json.tmp");
    fs::hard_link(&database, &old_temp).unwrap();
    tap_row(&mut app, "Import records");
    let mut visits = app
        .reading_history
        .visits()
        .iter()
        .map(|visit| (visit.page, visit.start, visit.seconds))
        .collect::<Vec<_>>();
    visits.sort_by_key(|visit| visit.1);
    assert_eq!(
        visits,
        [
            (0, Some(FIRST), 12),
            (2, Some(FIRST + 12), 18),
            (5, Some(FIRST + 30), 30),
            (2, Some(FIRST + 60), 15)
        ]
    );
    let summary = app.reading_history.summary(Some(id), None);
    assert_eq!((summary.seconds, summary.screens), (75, 3));
    let reading = &app.fixed.as_ref().unwrap().reading;
    assert_eq!(reading.preferences(), before);
    assert_eq!(reading.page_viewport(), request);
    assert_eq!(reading.history_point(), anchor);
    assert_eq!(app.current_position(), position);
    assert_eq!(fs::read(&source).unwrap(), DJVU);
    assert_eq!(fs::read(&database).unwrap(), STOCK_DJVU);
    assert_eq!(fs::read(&old_temp).unwrap(), STOCK_DJVU);
    assert_eq!(
        fs::metadata(&source).unwrap().modified().unwrap(),
        source_date
    );
    assert_eq!(
        fs::metadata(&database).unwrap().modified().unwrap(),
        database_date
    );
    assert!(!fixture.root.join("statistics.sqlite3-shm").exists());

    app.history_back();
    assert_eq!(app.screen, Screen::Fixed);
    assert_eq!(app.current_position(), position);
    let alias = fixture.root.join("Beacon Atlas.djv");
    fs::rename(&source, &alias).unwrap();
    let mut reopened = Reader::open(&fixture.root, &fixture.state, app.fonts.clone()).unwrap();
    reopened.open_path(&alias).unwrap();
    assert_eq!(reopened.open_book_id(), Some(id));
    preview(&mut reopened, id);
    let pending = reopened.history_view.koreader_import.as_ref().unwrap();
    assert_eq!(
        (
            pending.preview.new_visits,
            pending.preview.duplicate,
            pending.preview.conflicting
        ),
        (0, 4, 0)
    );
    assert!(
        !reopened
            .items()
            .iter()
            .any(|row| row.title == "Import records")
    );
}

#[test]
fn stock_djvu_sqlite_import_rejects_changed_edition_or_reader_journal() {
    let (fixture, mut app, id) = setup_book("Beacon Atlas.djv", DJVU, STOCK_DJVU, 2);
    preview(&mut app, id);
    let source = fixture.root.join("Beacon Atlas.djv");
    let unrelated = include_bytes!("../../../../tests/fixtures/signal-log.djvu");
    fs::write(&source, unrelated).unwrap();
    assert!(app.apply_koreader_statistics_import().is_err());
    assert!(app.reading_history.visits().is_empty());
    assert_eq!(fs::read(&source).unwrap(), unrelated);
    fs::write(&source, DJVU).unwrap();

    let journal = fixture.state.with_file_name("reading-history.json");
    let mut other = History::load(journal.clone(), Default::default());
    other
        .configure(
            crate::reading_stats::history::Options {
                min_seconds: 6,
                ..Default::default()
            },
            Instant::now(),
            Some(FIRST),
        )
        .unwrap();
    let saved = fs::read(&journal).unwrap();
    assert!(
        app.apply_koreader_statistics_import()
            .unwrap_err()
            .to_string()
            .contains("changed on disk")
    );
    assert!(app.history_view.koreader_import.is_some());
    assert!(app.reading_history.visits().is_empty());
    assert_eq!(fs::read(&journal).unwrap(), saved);
}

fn tap_row(app: &mut Reader, title: &str) {
    let (pages, rows) = app.list_window();
    let row = match rows.iter().position(|row| row.title == title) {
        Some(row) => row,
        None => {
            assert!(pages > 1);
            app.input(Input::PageForward);
            app.list_window()
                .1
                .iter()
                .position(|row| row.title == title)
                .unwrap()
        }
    };
    let rect = app.list_geometry().row(row);
    assert_eq!(
        app.input(Input::Tap(
            (rect.x + rect.w / 2) as f32,
            (rect.y + rect.h / 2) as f32
        )),
        Change::Partial
    );
}

fn preview(app: &mut Reader, id: u64) {
    app.activate_reading_history(Command::Open(Some(id)))
        .unwrap();
    tap_row(app, "Import KOReader statistics");
    assert_eq!(app.screen, Screen::ReadingHistory(Page::ImportKoReader));
    tap_row(app, "Preview database");
    assert!(
        app.history_view.koreader_import.is_some(),
        "{:?}",
        app.notice
    );
}

#[test]
fn sqlite_picker_pages_in_landscape_and_preserves_the_preview_on_cancel() {
    let (fixture, mut app, id) = setup();
    let root = &fixture.root;
    for index in 0..10 {
        fs::create_dir(root.join(format!("Archive {index:02}"))).unwrap();
    }
    let folder = root.join("Reading archive/2026");
    fs::create_dir_all(&folder).unwrap();
    let selected = folder.join("February.SQLITE");
    fs::write(&selected, STOCK).unwrap();
    fs::write(folder.join("Guide.txt"), b"Use a closed database").unwrap();
    let location = app.current_position();
    preview(&mut app, id);
    tap_row(&mut app, "Browse Documents");
    let mut canvas = adopt(&mut app, ReadingViewport::LANDSCAPE, 1);
    assert_eq!(app.list_page, 0);
    let next = app.navigation_layout().next();
    tap(&mut app, next);
    assert_eq!(app.list_page, 1);
    assert_eq!(app.input(Input::PageBack), Change::Partial);
    assert_eq!(app.list_page, 0);
    assert_eq!(app.input(Input::PageForward), Change::Partial);
    let ordinal = app.reading_list_ordinal();
    tap_row(&mut app, "Reading archive");
    let back = app.navigation_layout().back();
    assert!(back.x > 1264);
    tap(&mut app, back);
    assert_eq!(app.reading_list_ordinal(), ordinal);
    tap_row(&mut app, "Reading archive");
    tap_row(&mut app, "2026");
    assert!(
        app.items()
            .iter()
            .any(|item| item.title == "February.SQLITE")
    );
    assert!(!app.items().iter().any(|item| item.title == "Guide.txt"));
    assert_eq!(app.input(Input::PageBack), Change::Partial);
    assert_eq!(
        app.history_view
            .koreader_databases
            .as_ref()
            .unwrap()
            .directory,
        Path::new("Reading archive")
    );
    tap(&mut app, back);
    tap(&mut app, back);
    assert_eq!(app.screen, Screen::ReadingHistory(Page::ImportKoReader));
    assert_eq!(
        app.history_view.koreader_import.as_ref().unwrap().path,
        Path::new(DATABASE)
    );
    assert!(app.reading_history.visits().is_empty());

    tap_row(&mut app, "Browse Documents");
    tap(&mut app, next);
    tap_row(&mut app, "Reading archive");
    tap_row(&mut app, "2026");
    capture(
        &app,
        &mut canvas,
        "landscape-statistics-database-picker.png",
        false,
    );
    tap_row(&mut app, "February.SQLITE");
    let selected_path = Path::new("Reading archive/2026/February.SQLITE");
    assert_eq!(
        app.history_view.koreader_import.as_ref().unwrap().path,
        selected_path
    );
    fs::write(root.join(DATABASE), b"different database").unwrap();
    tap_row(&mut app, "Preview again");
    assert_eq!(
        app.history_view.koreader_import.as_ref().unwrap().path,
        selected_path
    );
    capture(
        &app,
        &mut canvas,
        "landscape-statistics-database-preview.png",
        false,
    );
    tap_row(&mut app, "Import records");
    assert_eq!(app.reading_history.summary(Some(id), None).seconds, 60);
    assert_eq!(app.current_position(), location);
    assert_eq!(app.reading_viewport(), ReadingViewport::LANDSCAPE);
    assert_eq!(fs::read(selected).unwrap(), STOCK);
}

#[test]
fn sqlite_picker_rechecks_nested_djvu_sidecars_and_directory_file_replacements() {
    let (fixture, mut app, id) = setup_book("Beacon Atlas.djv", DJVU, STOCK_DJVU, 2);
    let outside = Fixture::new();
    let outside_database = outside.root.join("Saved.db");
    fs::write(&outside_database, STOCK_DJVU).unwrap();
    let folder = fixture.root.join("Checkpoint");
    let archived = fixture.root.join("Retired");
    fs::create_dir(&folder).unwrap();
    let source = folder.join("Saved.db");
    fs::write(&source, STOCK_DJVU).unwrap();
    std::os::unix::fs::symlink(&outside.root, fixture.root.join("Outside folder")).unwrap();
    std::os::unix::fs::symlink(&outside_database, folder.join("Outside file.db")).unwrap();
    fs::write(
        fixture.root.join("statistics.sqlite3-wal"),
        b"only the root database is open",
    )
    .unwrap();
    let reading = app.current_position();

    app.activate_reading_history(Command::Open(Some(id)))
        .unwrap();
    tap_row(&mut app, "Import KOReader statistics");
    tap_row(&mut app, "Browse Documents");
    assert!(
        !app.items()
            .iter()
            .any(|item| item.title == "Outside folder")
    );
    tap_row(&mut app, "Checkpoint");
    assert!(
        !app.items()
            .iter()
            .any(|item| item.title == "Outside file.db")
    );
    tap_row(&mut app, "Saved.db");
    assert_eq!(
        app.history_view
            .koreader_import
            .as_ref()
            .unwrap()
            .preview
            .new_visits,
        4
    );

    let sidecar = folder.join("Saved.db-wal");
    fs::write(&sidecar, b"uncheckpointed frames").unwrap();
    assert!(
        app.apply_koreader_statistics_import()
            .unwrap_err()
            .to_string()
            .contains("pending journal")
    );
    fs::remove_file(sidecar).unwrap();
    fs::rename(&folder, &archived).unwrap();
    std::os::unix::fs::symlink(&outside.root, &folder).unwrap();
    assert!(app.apply_koreader_statistics_import().is_err());
    fs::remove_file(&folder).unwrap();
    fs::rename(&archived, &folder).unwrap();
    fs::remove_file(&source).unwrap();
    std::os::unix::fs::symlink(&outside_database, &source).unwrap();
    assert!(app.apply_koreader_statistics_import().is_err());
    fs::remove_file(&source).unwrap();
    let mut changed = STOCK_DJVU.to_vec();
    changed.push(0);
    fs::write(&source, &changed).unwrap();
    assert!(
        app.apply_koreader_statistics_import()
            .unwrap_err()
            .to_string()
            .contains("database changed")
    );
    assert!(app.reading_history.visits().is_empty());
    fs::write(&source, STOCK_DJVU).unwrap();
    tap_row(&mut app, "Import records");
    assert_eq!(app.reading_history.summary(Some(id), None).seconds, 75);
    assert_eq!(app.current_position(), reading);
    assert_eq!(fs::read(&source).unwrap(), STOCK_DJVU);
    assert_eq!(fs::read(&outside_database).unwrap(), STOCK_DJVU);
}

#[test]
fn stock_pdf_sqlite_import_previews_cancels_preserves_position_and_persists_once() {
    let (fixture, mut app, id) = setup();
    let position = app.current_position();
    let now = Instant::now();
    let existing = FIRST - 3600;
    app.reading_history.discard_active();
    app.reading_history
        .set_page_at(Some((id, 1)), now, Some(existing));
    app.reading_history
        .set_page_at(None, now + Duration::from_secs(8), Some(existing + 8));
    app.reading_history.discard_active();
    preview(&mut app, id);
    let pending = app.history_view.koreader_import.as_ref().unwrap();
    assert_eq!(pending.preview.new_visits, 3);
    assert_eq!(pending.preview.seconds, 60);
    assert!(pending.preview.can_store);
    assert!(
        app.items()
            .iter()
            .any(|item| item.title == "Matched KOReader book"
                && item.detail.as_deref() == Some("Survey"))
    );
    assert_eq!(pending.preview.first_start, Some(FIRST));
    assert_eq!(pending.preview.last_start, Some(FIRST + 30));
    assert_eq!(app.reading_history.summary(Some(id), None).seconds, 8);
    capture_test_screen(&app, &app.fonts, "READER_STATS_IMPORT_SCREEN", false);
    app.history_back();
    assert!(app.history_view.koreader_import.is_none());
    assert_eq!(app.reading_history.summary(Some(id), None).seconds, 8);

    preview(&mut app, id);
    tap_row(&mut app, "Import records");
    assert_eq!(app.screen, Screen::ReadingHistory(Page::Summary));
    let visits: Vec<_> = app
        .reading_history
        .visits()
        .iter()
        .map(|row| (row.page, row.start, row.seconds))
        .collect();
    assert_eq!(
        visits,
        [
            (1, Some(existing), 8),
            (0, Some(FIRST), 12),
            (0, Some(FIRST + 30), 30),
            (1, Some(FIRST + 12), 18)
        ]
    );
    assert_eq!(app.reading_history.summary(Some(id), None).seconds, 68);
    assert_eq!(app.reading_history.summary(Some(id), None).screens, 2);
    assert_eq!(app.current_position(), position);
    assert_eq!(fs::read(fixture.root.join("Survey.pdf")).unwrap(), PDF);
    assert_eq!(fs::read(fixture.root.join(DATABASE)).unwrap(), STOCK);
    assert!(!fixture.root.join("statistics.sqlite3-shm").exists());

    let mut reopened = Reader::open(&fixture.root, &fixture.state, app.fonts.clone()).unwrap();
    reopened
        .open_path(&fixture.root.join("Survey.pdf"))
        .unwrap();
    preview(&mut reopened, id);
    let imported = reopened
        .history_view
        .koreader_import
        .as_ref()
        .unwrap()
        .preview;
    assert_eq!(
        (
            imported.new_visits,
            imported.duplicate,
            imported.conflicting
        ),
        (0, 3, 0)
    );
    assert!(
        !reopened
            .items()
            .iter()
            .any(|row| row.title == "Import records")
    );
}

#[test]
fn stock_pdf_sqlite_import_rejects_mismatched_editions_hot_journals_and_changed_sources() {
    let (fixture, mut app, id) = setup();
    preview(&mut app, id);
    let database = fixture.root.join(DATABASE);
    let mut changed = STOCK.to_vec();
    changed.push(0);
    fs::write(&database, changed).unwrap();
    assert!(
        app.apply_koreader_statistics_import()
            .unwrap_err()
            .to_string()
            .contains("database changed")
    );
    assert!(app.reading_history.visits().is_empty());
    fs::write(&database, STOCK).unwrap();
    let journal = fixture.root.join("statistics.sqlite3-wal");
    fs::write(&journal, b"pending frames").unwrap();
    assert!(
        app.preview_koreader_statistics_import()
            .unwrap_err()
            .to_string()
            .contains("pending journal")
    );
    assert!(app.history_view.koreader_import.is_none());
    fs::remove_file(journal).unwrap();

    let same_title = fixture.root.join("Alternate").join("Survey.pdf");
    fs::create_dir(same_title.parent().unwrap()).unwrap();
    fs::write(
        &same_title,
        crate::fixed_session::tests::striped_pdf_sized_bytes(2, 600, 800),
    )
    .unwrap();
    app.open_path(&same_title).unwrap();
    app.activate_reading_history(Command::Open(app.open_book_id()))
        .unwrap();
    app.begin_koreader_statistics_import().unwrap();
    assert!(
        app.preview_koreader_statistics_import()
            .unwrap_err()
            .to_string()
            .contains("no match")
    );
    app.open_path(&fixture.book).unwrap();
    app.activate_reading_history(Command::Open(app.open_book_id()))
        .unwrap();
    assert!(
        !app.items()
            .iter()
            .any(|row| row.title == "Import KOReader statistics")
    );
}

#[test]
fn stock_pdf_sqlite_import_protects_the_journal_and_keeps_failed_draft_for_retry() {
    let (fixture, mut app, id) = setup();
    preview(&mut app, id);
    let path = fixture.state.with_file_name("reading-history.json");
    let baseline = fs::read(&path).ok();
    // Foreign valid history is still a conflict and is not overwritten.
    let now = Instant::now();
    let mut other = History::load(path.clone(), Default::default());
    other
        .configure(
            crate::reading_stats::history::Options {
                min_seconds: 6,
                ..Default::default()
            },
            now,
            Some(FIRST),
        )
        .unwrap();
    let saved = fs::read(&path).unwrap();
    assert!(
        app.apply_koreader_statistics_import()
            .unwrap_err()
            .to_string()
            .contains("changed on disk")
    );
    assert_eq!(fs::read(&path).unwrap(), saved);
    assert!(app.reading_history.visits().is_empty());
    if let Some(baseline) = baseline {
        fs::write(&path, baseline).unwrap();
    } else {
        fs::remove_file(&path).unwrap();
    }

    // Block the normal atomic write, then retry the same reviewed records.
    let lock = path.with_extension("lock");
    fs::remove_file(&lock).unwrap();
    fs::create_dir(&lock).unwrap();
    assert!(app.apply_koreader_statistics_import().is_err());
    assert!(app.reading_history.visits().is_empty());
    assert!(app.history_view.koreader_import.is_some());
    fs::remove_dir(&lock).unwrap();
    let target = path.with_file_name("reading-history.json.tmp");
    std::os::unix::fs::symlink(fixture.root.join("Survey.pdf"), &target).unwrap();
    app.apply_koreader_statistics_import().unwrap();
    assert!(fs::symlink_metadata(&target).unwrap().is_symlink());
    assert_eq!(fs::read(&target).unwrap(), PDF);
    assert_eq!(app.reading_history.summary(Some(id), None).seconds, 60);
}

#[test]
fn stock_pdf_sqlite_import_does_not_write_the_old_temporary_hardlink() {
    let (fixture, mut app, id) = setup();
    let database = fixture.root.join(DATABASE);
    let old_temp = fixture.state.with_file_name("reading-history.json.tmp");
    fs::hard_link(database, &old_temp).unwrap();
    preview(&mut app, id);
    app.apply_koreader_statistics_import().unwrap();
    assert_eq!(fs::read(&old_temp).unwrap(), STOCK);
}

#[test]
fn stock_pdf_sqlite_import_separates_conflicts_mixed_pagination_and_invalid_rows() {
    let (fixture, mut app, id) = setup();
    let database = fixture.root.join(DATABASE);
    let edit_stock = |sql: &str| {
        let db = rusqlite::Connection::open(&database).unwrap();
        db.execute_batch(sql).unwrap();
        db.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)").unwrap();
    };
    edit_stock("INSERT INTO page_stat_data VALUES((SELECT id FROM book), 3, 1769990435, 45, 3)");
    let now = Instant::now();
    app.reading_history.discard_active();
    app.reading_history
        .set_page_at(Some((id, 0)), now, Some(FIRST));
    app.reading_history
        .set_page_at(None, now + Duration::from_secs(8), Some(FIRST + 8));
    app.reading_history.discard_active();
    preview(&mut app, id);
    let pending = app.history_view.koreader_import.as_ref().unwrap();
    assert_eq!(pending.records.other_page_counts, 1);
    assert_eq!(
        (
            pending.preview.new_visits,
            pending.preview.conflicting,
            pending.preview.seconds
        ),
        (2, 1, 48)
    );
    app.apply_koreader_statistics_import().unwrap();
    assert_eq!(app.reading_history.summary(Some(id), None).seconds, 56);

    edit_stock("UPDATE page_stat_data SET duration = 0 WHERE start_time = 1769990394");
    app.begin_koreader_statistics_import().unwrap();
    assert!(app.preview_koreader_statistics_import().is_err());
    assert!(app.history_view.koreader_import.is_none());
    edit_stock(
        "INSERT INTO book (title, authors, md5, pages) SELECT 'Other', authors, md5, pages FROM book",
    );
    assert!(
        app.preview_koreader_statistics_import()
            .unwrap_err()
            .to_string()
            .contains("Several")
    );
    assert_eq!(app.reading_history.summary(Some(id), None).seconds, 56);
}
