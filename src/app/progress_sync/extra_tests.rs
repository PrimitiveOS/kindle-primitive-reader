//! Recovery and real-worker checks for the opt-in native progress protocol.
use super::*;
use crate::app::tests::{Fixture, capture_test_screen};
use crate::app::{LIST_X, LIST_Y, ROW_HEIGHT, ROWS_PER_PAGE};
use crate::library::ReadingPosition;
use crate::progress_sync::tests::Server;
use primitive_app::App;
use std::fs;
use std::path::Path;
use std::time::Duration;

fn open(fixture: &Fixture, path: &Path) -> Reader {
    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut app = Reader::open(path.parent().unwrap(), &fixture.state, fonts).unwrap();
    app.open_path(path).unwrap();
    app
}

fn at(app: &Reader, position: ReadingPosition) -> Remote {
    let mut record = app.local_sync_record().unwrap();
    record.position = Some(position);
    Remote {
        record,
        etag: "\"checked\"".into(),
    }
}

fn tap(app: &mut Reader, rect: Rect) -> Change {
    app.input(Input::Tap(
        (rect.x + rect.w / 2) as f32,
        (rect.y + rect.h / 2) as f32,
    ))
}

fn save_editor(app: &mut Reader, value: &str) {
    app.editor.text = value.chars().collect();
    app.editor.caret = app.editor.text.len();
    let go = Layout::new(WIDTH, HEIGHT, 4, 5)
        .keyboard_keys_with(app.editor.layer, "Save")
        .into_iter()
        .find(|(key, _)| key.key == primitive_ui::Key::Go)
        .unwrap()
        .1;
    assert_eq!(tap(app, go), Change::Partial);
    assert_eq!(app.screen, Screen::ProgressSyncSettings);
}

#[test]
fn stale_book_puts_and_clears_are_rejected_before_a_worker_can_spawn() {
    let fixture = Fixture::new();
    let mut app = open(&fixture, &fixture.book);
    let stale = app.local_sync_record().unwrap();
    let stale_remote = at(&app, app.current_position().unwrap());
    let pdf = fixture.root.join("Different edition.pdf");
    fs::write(&pdf, crate::fixed::tests::document()).unwrap();
    app.open_path(&pdf).unwrap();
    assert_ne!(stale.book, app.open_book_id().unwrap());
    let current = app.local_sync_record().unwrap();
    let server = Server::start();
    app.progress_sync.config = server.config();
    // Reaching spawn is a test failure, rather than accidentally using the test
    // harness itself as the --sync-worker program.
    app.progress_sync.test_worker = Some(fixture.root.join("must-not-start"));
    app.progress_sync.local = Some(stale.clone());
    app.progress_sync.remote = Some(stale_remote.clone());
    for operation in [
        Operation::Put {
            record: stale.clone(),
            expected: Some(stale_remote.etag.clone()),
        },
        Operation::Delete {
            expected: Some(stale_remote.etag.clone()),
        },
    ] {
        let error = app
            .start_progress_request(operation)
            .unwrap_err()
            .to_string();
        assert!(error.contains("book changed"), "{error}");
        assert!(app.progress_sync.job.is_none());
    }
    app.progress_sync.local = Some(current.clone());
    let error = app
        .start_progress_request(Operation::Put {
            record: current.clone(),
            expected: Some(stale_remote.etag),
        })
        .unwrap_err()
        .to_string();
    assert!(error.contains("another edition"), "{error}");
    app.progress_sync.remote = None;
    let error = app
        .start_progress_request(Operation::Put {
            record: stale,
            expected: None,
        })
        .unwrap_err()
        .to_string();
    assert!(error.contains("another edition"), "{error}");
    assert_eq!(server.calls(), 0);
    assert!(app.progress_sync.job.is_none());
}

#[test]
fn editing_each_account_identity_field_clears_only_its_sync_baseline_and_cached_reply() {
    for (field, value) in [
        (Field::Folder, "https://different.example.test/reading/"),
        (Field::Username, "different-reader"),
        (Field::Password, "replacement-token"),
    ] {
        let fixture = Fixture::new();
        let mut app = open(&fixture, &fixture.book);
        let server = Server::start();
        app.progress_sync.config = server.config();
        let local = app.local_sync_record().unwrap();
        let before = app.current_position();
        app.progress_sync.config.seen.insert(
            local.edition.clone(),
            Seen {
                etag: "\"old-account\"".into(),
                position: local.position.clone(),
            },
        );
        app.progress_sync.local = Some(local);
        app.progress_sync.remote = Some(at(&app, before.clone().unwrap()));
        app.progress_sync_command(Command::Edit(field)).unwrap();
        save_editor(&mut app, value);
        assert!(app.progress_sync.config.seen.is_empty());
        assert!(app.progress_sync.local.is_none());
        assert!(app.progress_sync.remote.is_none());
        assert_eq!(app.current_position(), before);
        let saved = Config::load(&app.progress_sync.path);
        assert!(saved.seen.is_empty());
        match field {
            Field::Folder => assert_eq!(saved.folder, value),
            Field::Username => assert_eq!(saved.username, value),
            Field::Password => assert_eq!(saved.password, value),
            Field::Device => unreachable!(),
        }
        assert_eq!(
            server.calls(),
            0,
            "editing settings must not contact a server"
        );
    }
}

#[test]
fn renaming_this_device_keeps_the_account_baseline_and_cached_version() {
    let fixture = Fixture::new();
    let mut app = open(&fixture, &fixture.book);
    app.progress_sync.config = Server::start().config();
    let local = app.local_sync_record().unwrap();
    app.progress_sync.config.seen.insert(
        local.edition.clone(),
        Seen {
            etag: "\"same-account\"".into(),
            position: local.position.clone(),
        },
    );
    app.progress_sync.local = Some(local.clone());
    app.progress_sync.remote = Some(at(&app, local.position.clone().unwrap()));
    app.progress_sync_command(Command::Edit(Field::Device))
        .unwrap();
    save_editor(&mut app, "Living-room Kindle");
    assert_eq!(app.progress_sync.config.device, "Living-room Kindle");
    assert!(app.progress_sync.config.seen.contains_key(&local.edition));
    assert!(app.progress_sync.local.is_some());
    assert!(app.progress_sync.remote.is_some());
}

#[test]
fn a_pdf_receive_that_cannot_save_restores_its_frame_position_marks_and_return_history() {
    let fixture = Fixture::new();
    let pdf = fixture.root.join("Two pages.pdf");
    fs::write(&pdf, crate::fixed::tests::document()).unwrap();
    let mut app = open(&fixture, &pdf);
    app.activate(Action::AddBookmark).unwrap();
    app.show(Screen::Fixed);
    app.save().unwrap();
    let before = app.current_history_location().unwrap();
    let history = app.back.clone();
    let book_id = app.open_book_id().unwrap();
    let state = serde_json::to_vec(app.library.state(book_id).unwrap()).unwrap();
    let mut canvas = Canvas::new(WIDTH, HEIGHT, &[app.fonts[0].display().to_string()]).unwrap();
    let frame = primitive_app::frame(&app, &mut canvas, false);
    let remote = at(
        &app,
        ReadingPosition {
            chapter: 0,
            page: 1,
            anchor: None,
        },
    );
    let saved = fixture.root.join("saved-library.json");
    fs::rename(&fixture.state, &saved).unwrap();
    fs::create_dir(&fixture.state).unwrap();
    assert!(app.open_synced_position(&remote).is_err());
    assert_eq!(app.current_history_location(), Some(before.clone()));
    assert_eq!(app.back, history);
    assert_eq!(
        serde_json::to_vec(app.library.state(book_id).unwrap()).unwrap(),
        state
    );
    assert!(primitive_app::frame(&app, &mut canvas, false) == frame);
    fs::remove_dir(&fixture.state).unwrap();
    fs::rename(&saved, &fixture.state).unwrap();
    app.save().unwrap();
    let mut reopened = open(&fixture, &pdf);
    assert_eq!(reopened.current_position(), Some(before.position));
    assert_eq!(reopened.library.state(book_id).unwrap().bookmarks.len(), 1);
    reopened.save().unwrap();
}

fn sleeping_job(app: &Reader, server: &Server) -> Job {
    let local = app.local_sync_record().unwrap();
    let request = Request {
        config: server.config(),
        edition: local.edition,
        book: local.book,
        operation: Operation::Fetch,
    };
    let mut process = std::process::Command::new("/bin/sh");
    process.args(["-c", "exec sleep 30"]);
    Job::spawn(process, &request).unwrap()
}

#[test]
fn sync_conflict_dialog_and_password_editor_pass_sleep_and_wake_to_the_reading_lifecycle() {
    let fixture = Fixture::new();
    let mut app = open(&fixture, &fixture.book);
    let server = Server::start();
    app.progress_sync.config = server.config();
    app.progress_sync_command(Command::Open).unwrap();
    app.progress_sync.conflict = true;
    app.progress_sync.job = Some(sleeping_job(&app, &server));
    let position = app.current_position();
    app.input(Input::Sleep);
    assert!(!app.stats_awake);
    assert!(app.progress_sync.job.is_none());
    assert!(app.progress_sync.conflict);
    app.input(Input::Wake);
    assert!(app.stats_awake);
    assert_eq!(app.current_position(), position);
    tap(&mut app, CANCEL);
    app.progress_sync_command(Command::Edit(Field::Password))
        .unwrap();
    let draft = app.editor.string();
    app.progress_sync.job = Some(sleeping_job(&app, &server));
    app.input(Input::Sleep);
    assert!(!app.stats_awake);
    assert!(app.progress_sync.job.is_none());
    assert_eq!(app.screen, Screen::SyncEdit(Field::Password));
    assert_eq!(app.editor.string(), draft);
    app.input(Input::Wake);
    assert!(app.stats_awake);
    assert_eq!(app.screen, Screen::SyncEdit(Field::Password));
    assert_eq!(app.editor.string(), draft);
    assert_eq!(app.current_position(), position);
    assert_eq!(server.calls(), 0);
}

#[test]
fn identical_source_passages_ignore_layout_pages_but_different_source_offsets_conflict() {
    let fixture = Fixture::new();
    let mut app = open(&fixture, &fixture.book);
    let mut small = app.current_position().unwrap();
    assert_ne!(
        small.anchor.as_ref().unwrap().book,
        app.open_book_id().unwrap(),
        "publication anchors and library fingerprints occupy separate identity domains"
    );
    let mut settings = app.active.as_ref().unwrap().reading.config.settings;
    settings.font_size = 76.0;
    app.activate(Action::Text(settings)).unwrap();
    let mut large = app.current_position().unwrap();
    large.anchor = small.anchor.clone();
    small.page = 18;
    large.page = 91;
    let remote = at(&app, large.clone());
    assert!(wire::same_position(&small, &large));
    assert_eq!(wire::decide(&small, Some(&remote), None), Decision::Equal);
    let seen = Seen {
        etag: "\"old\"".into(),
        position: Some(small.clone()),
    };
    let mut moved = large.clone();
    moved.anchor.as_mut().unwrap().offset += 1;
    assert!(!wire::same_position(&small, &moved));
    assert_eq!(
        wire::decide(&moved, Some(&remote), Some(&seen)),
        Decision::Send
    );
    let mut server_moved = remote.clone();
    server_moved
        .record
        .position
        .as_mut()
        .unwrap()
        .anchor
        .as_mut()
        .unwrap()
        .offset += 2;
    assert_eq!(
        wire::decide(&moved, Some(&server_moved), Some(&seen)),
        Decision::Conflict
    );
    let mut changed_revision = large;
    changed_revision.anchor.as_mut().unwrap().revision ^= 1;
    assert!(!wire::same_position(&small, &changed_revision));
}

fn tap_item(app: &mut Reader, title: &str) {
    let index = app
        .items()
        .iter()
        .position(|item| item.title == title)
        .unwrap_or_else(|| panic!("missing sync action {title}"));
    app.list_page = index / ROWS_PER_PAGE;
    let row = index % ROWS_PER_PAGE;
    assert_eq!(
        app.input(Input::Tap(
            (LIST_X + 50) as f32,
            (LIST_Y + row as i32 * ROW_HEIGHT + ROW_HEIGHT / 2) as f32
        )),
        Change::Partial
    );
}

fn wait_for_sync(app: &mut Reader) {
    let end = Instant::now() + Duration::from_secs(15);
    while app.progress_sync.job.is_some() {
        assert!(
            Instant::now() < end,
            "the localhost worker did not finish within 15 seconds"
        );
        app.tick(Instant::now());
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn synchronize(app: &mut Reader) {
    app.progress_sync_command(Command::Open).unwrap();
    tap_item(app, "Sync now");
    wait_for_sync(app);
}

fn move_reading(app: &mut Reader, fraction: f64) -> ReadingPosition {
    if let Some(active) = &mut app.active {
        active.reading.jump_progress(fraction).unwrap();
        app.show(Screen::Book);
        app.record_position();
    } else {
        let count = app.fixed.as_ref().unwrap().reading.count();
        assert!(count >= 4, "the selected PDF needs at least four pages");
        app.jump_fixed(((count - 1) as f64 * fraction) as usize, None)
            .unwrap();
    }
    app.current_position().unwrap()
}

#[test]
#[ignore = "requires authorized unchanged READER_TEST_SYNC_EPUB/PDF and built READER_TEST_SYNC_WORKER"]
fn two_real_reader_profiles_sync_original_epub_and_pdf_through_the_worker_and_touch_ui() {
    let worker = PathBuf::from(
        std::env::var_os("READER_TEST_SYNC_WORKER").expect("built sync_worker example"),
    );
    assert!(worker.is_file());
    for variable in ["READER_TEST_SYNC_EPUB", "READER_TEST_SYNC_PDF"] {
        let original =
            PathBuf::from(std::env::var_os(variable).expect("authorized original document"));
        let hash = ring::digest::digest(&ring::digest::SHA256, &fs::read(&original).unwrap());
        let first = Fixture::new();
        let second = Fixture::new();
        let server = Server::start();
        let mut a = open(&first, &original);
        let mut b = open(&second, &original);
        assert_eq!(a.open_book_id(), b.open_book_id());
        for (app, name) in [(&mut a, "Kindle A"), (&mut b, "Kindle B")] {
            let mut config = server.config();
            config.device = name.into();
            app.progress_sync.persist(config).unwrap();
            app.progress_sync.test_worker = Some(worker.clone());
        }
        let shared = move_reading(&mut a, 0.25);
        synchronize(&mut a);
        assert_eq!(server.record().unwrap().position, Some(shared.clone()));
        synchronize(&mut b);
        assert!(
            b.progress_sync.conflict,
            "an unfamiliar different server passage needs consent"
        );
        tap(&mut b, REMOTE);
        assert!(wire::same_position(&b.current_position().unwrap(), &shared));
        let a_changed = move_reading(&mut a, 0.5);
        let b_changed = move_reading(&mut b, 0.75);
        assert!(!wire::same_position(&a_changed, &b_changed));
        synchronize(&mut a);
        synchronize(&mut b);
        assert!(b.progress_sync.conflict);
        capture_test_screen(
            &b,
            &b.fonts,
            if b.fixed.is_some() {
                "READER_SYNC_ORIGINAL_PDF_SCREENSHOT"
            } else {
                "READER_SYNC_ORIGINAL_EPUB_SCREENSHOT"
            },
            false,
        );
        tap(&mut b, CANCEL);
        assert!(wire::same_position(
            &b.current_position().unwrap(),
            &b_changed
        ));
        synchronize(&mut b);
        assert!(b.progress_sync.conflict);
        tap(&mut b, LOCAL);
        wait_for_sync(&mut b);
        assert_eq!(server.record().unwrap().position, Some(b_changed.clone()));
        synchronize(&mut a);
        assert!(wire::same_position(
            &a.current_position().unwrap(),
            &b_changed
        ));
        tap_item(&mut b, "Remove server position");
        assert!(b.progress_sync.delete_confirmation);
        tap(&mut b, LOCAL);
        wait_for_sync(&mut b);
        assert!(server.record().unwrap().position.is_none());
        let deleted_version = server.version();
        for _ in 0..2 {
            synchronize(&mut a);
            assert!(a.progress_sync.job.is_none());
            assert!(!a.progress_sync.conflict);
            assert!(wire::same_position(
                &a.current_position().unwrap(),
                &b_changed
            ));
            assert_eq!(
                server.version(),
                deleted_version,
                "a tombstone must not be resurrected by Sync"
            );
        }
        tap_item(&mut a, "Send this device's position");
        wait_for_sync(&mut a);
        assert!(wire::same_position(
            server.record().unwrap().position.as_ref().unwrap(),
            &b_changed
        ));
        synchronize(&mut b);
        assert!(!b.progress_sync.conflict);
        a.save().unwrap();
        let reopened = open(&first, &original);
        assert!(wire::same_position(
            &reopened.current_position().unwrap(),
            &b_changed
        ));
        let after = ring::digest::digest(&ring::digest::SHA256, &fs::read(&original).unwrap());
        assert_eq!(
            hash.as_ref(),
            after.as_ref(),
            "sync must leave the original file byte-identical"
        );
    }
}
