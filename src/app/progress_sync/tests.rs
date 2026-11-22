use super::*;
use crate::app::tests::{Fixture, capture_test_screen};
use crate::library::ReadingPosition;
use primitive_app::App;

fn reader(fixture: &Fixture) -> (Reader, Vec<PathBuf>) {
    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
    app.open_path(&fixture.book).unwrap();
    (app, fonts)
}

fn remote(app: &Reader, position: ReadingPosition) -> Remote {
    let mut record = app.local_sync_record().unwrap();
    record.position = Some(position);
    record.device = "Other Kindle".into();
    Remote {
        record,
        etag: "\"v1\"".into(),
    }
}

fn target(app: &mut Reader) -> ReadingPosition {
    let active = app.active.as_mut().unwrap();
    let probe = active.reading.config.chapter(1).unwrap();
    ReadingPosition {
        chapter: 1,
        page: 0,
        anchor: probe.fragment("door"),
    }
}

#[test]
fn disabled_sync_has_no_worker_and_sign_out_preserves_local_reading_data() {
    let fixture = Fixture::new();
    let (mut app, fonts) = reader(&fixture);
    let before = app.current_position();
    app.progress_sync_command(Command::Open).unwrap();
    assert!(app.progress_sync.job.is_none());
    assert!(!app.progress_sync.config.enabled);
    assert!(app.progress_sync_command(Command::Sync).is_err());
    assert!(app.progress_sync.job.is_none());
    app.progress_sync.config.username = "local-test-account".into();
    app.progress_sync.config.password = "not-a-real-password".into();
    capture_test_screen(&app, &fonts, "READER_SYNC_SCREENSHOT", false);
    app.progress_sync_command(Command::SignOut).unwrap();
    assert!(app.progress_sync.config.password.is_empty());
    assert!(app.progress_sync.config.username.is_empty());
    assert_eq!(app.current_position(), before);
    app.save().unwrap();
    let mut reopened = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
    reopened.activate(Action::Continue).unwrap();
    assert_eq!(reopened.current_position(), before);
}

#[test]
fn a_delayed_same_book_result_compares_the_current_passage_and_cancel_keeps_it() {
    let fixture = Fixture::new();
    let (mut app, fonts) = reader(&fixture);
    let initial = app.local_sync_record().unwrap();
    app.progress_sync.config.seen.insert(
        initial.edition.clone(),
        Seen {
            etag: "\"old\"".into(),
            position: initial.position.clone(),
        },
    );
    app.progress_sync.local = Some(initial);
    app.progress_sync.intent = Intent::Sync;
    let server_target = target(&mut app);
    let remote = remote(&app, server_target);
    let active = app.active.as_mut().unwrap();
    active.reading.jump(0, Some("lantern")).unwrap();
    app.record_position();
    let current = app.current_position();
    app.finish_progress_sync(Reply::Found(remote)).unwrap();
    assert!(app.progress_sync.conflict);
    assert_eq!(app.current_position(), current);
    capture_test_screen(&app, &fonts, "READER_SYNC_CONFLICT_SCREENSHOT", false);
    app.input(Input::PageBack);
    assert!(!app.progress_sync.conflict);
    assert_eq!(app.current_position(), current);
}

#[test]
fn opening_a_synced_source_anchor_survives_a_different_font_and_keeps_return_history() {
    let fixture = Fixture::new();
    let (mut app, _) = reader(&fixture);
    let origin = app.current_position().unwrap();
    let remote_position = target(&mut app);
    let remote = remote(&app, remote_position.clone());
    let mut settings = app.active.as_ref().unwrap().reading.config.settings;
    settings.font_size = 68.0;
    app.activate(Action::Text(settings)).unwrap();
    app.open_synced_position(&remote).unwrap();
    let position = app.current_position().unwrap();
    assert!(wire::same_position(&position, &remote_position));
    app.activate(Action::ReturnToLocation).unwrap();
    assert!(wire::same_position(
        &app.current_position().unwrap(),
        &origin
    ));
}

#[test]
fn malformed_or_unresolvable_remote_passages_do_not_change_local_state() {
    let fixture = Fixture::new();
    let (mut app, _) = reader(&fixture);
    let before = app.current_position();
    let mut bad = target(&mut app);
    bad.anchor.as_mut().unwrap().revision = 12_345;
    let remote = remote(&app, bad);
    assert!(app.open_synced_position(&remote).is_err());
    assert_eq!(app.current_position(), before);
    let mut bad = remote;
    bad.record.book ^= 1;
    assert!(app.open_synced_position(&bad).is_err());
    assert_eq!(app.current_position(), before);
}

#[test]
fn versioned_clear_survives_restart_without_resetting_or_recreating_local_progress() {
    let fixture = Fixture::new();
    let (mut app, fonts) = reader(&fixture);
    let before = app.current_position();
    let mut record = app.local_sync_record().unwrap();
    record.position = None;
    record.device.clear();
    let cleared = Remote {
        record,
        etag: "\"cleared\"".into(),
    };
    app.progress_sync.local = Some(app.local_sync_record().unwrap());
    app.progress_sync.intent = Intent::Sync;
    app.finish_progress_sync(Reply::Found(cleared.clone()))
        .unwrap();
    assert!(app.progress_sync.job.is_none());
    app.save().unwrap();
    let mut reopened = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
    reopened.activate(Action::Continue).unwrap();
    reopened.progress_sync.local = Some(reopened.local_sync_record().unwrap());
    reopened.progress_sync.intent = Intent::Sync;
    reopened
        .finish_progress_sync(Reply::Found(cleared))
        .unwrap();
    assert!(reopened.progress_sync.job.is_none());
    assert_eq!(reopened.current_position(), before);
}

#[test]
fn every_sync_view_renders_without_changing_the_source_and_busy_cancel_reaps_its_worker() {
    let fixture = Fixture::new();
    let (mut app, fonts) = reader(&fixture);
    let origin = app.current_position();
    let capture = |app: &Reader, name: &str, dark: bool| {
        if let Some(root) = std::env::var_os("READER_SYNC_REVIEW_DIR") {
            let root = PathBuf::from(root);
            std::fs::create_dir_all(&root).unwrap();
            let paths: Vec<_> = fonts.iter().map(|p| p.display().to_string()).collect();
            let mut canvas = Canvas::new(WIDTH, HEIGHT, &paths).unwrap();
            let pixels = primitive_app::frame(app, &mut canvas, dark);
            primitive_app::write_png(&root.join(format!("{name}.png")), WIDTH, HEIGHT, &pixels)
                .unwrap();
        }
    };
    app.progress_sync_command(Command::Open).unwrap();
    capture(&app, "overview", false);
    app.progress_sync.config.folder = "https://sync.example.test/reading/".into();
    app.progress_sync.config.username = "reader".into();
    app.progress_sync.config.password = "sample-token".into();
    app.progress_sync.config.enabled = true;
    app.show(Screen::ProgressSyncSettings);
    capture(&app, "settings", false);
    for (field, name) in [
        (Field::Folder, "folder"),
        (Field::Username, "username"),
        (Field::Password, "password"),
        (Field::Device, "device"),
    ] {
        app.progress_sync_command(Command::Edit(field)).unwrap();
        capture(&app, name, false);
        app.input(Input::PageBack);
    }
    app.show(Screen::ProgressSync);
    app.progress_sync.local = Some(app.local_sync_record().unwrap());
    let at = target(&mut app);
    app.progress_sync.remote = Some(remote(&app, at));
    capture(&app, "preview", false);
    app.progress_sync.conflict = true;
    capture(&app, "conflict", false);
    capture(&app, "conflict-dark", true);
    app.input(Input::PageBack);
    app.progress_sync.delete_confirmation = true;
    capture(&app, "clear", false);
    app.input(Input::PageBack);
    let local = app.local_sync_record().unwrap();
    let request = Request {
        config: app.progress_sync.config.clone(),
        edition: local.edition,
        book: local.book,
        operation: Operation::Fetch,
    };
    let mut command = std::process::Command::new("/bin/sh");
    command.args(["-c", "exec sleep 30"]);
    app.progress_sync.job = Some(Job::spawn(command, &request).unwrap());
    capture(&app, "waiting", false);
    assert_eq!(
        app.input(Input::Tap(
            (WAIT_CANCEL.x + 20) as f32,
            (WAIT_CANCEL.y + 20) as f32
        )),
        Change::Partial
    );
    assert!(app.progress_sync.job.is_none());
    assert_eq!(app.current_position(), origin);
}

#[test]
fn boundary_location_is_rejected_before_a_sync_worker_or_remote_write_starts() {
    let fixture = Fixture::new();
    let (mut app, _) = reader(&fixture);
    let active = app.active.as_mut().unwrap();
    let owner = active.reading.probe.source.blocks[0].element;
    let anchor = active.reading.probe.boundary_anchor(owner).unwrap();
    active.reading.jump_to_anchor(0, &anchor).unwrap();
    app.progress_sync.config.enabled = true;
    app.progress_sync.config.folder = "http://127.0.0.1:9/reading/".into();
    let before = app.current_position();
    assert!(app.local_sync_record().is_err());
    assert!(app.progress_sync_command(Command::Sync).is_err());
    assert!(app.progress_sync.job.is_none());
    assert_eq!(app.current_position(), before);
    assert_eq!(
        app.library
            .state(app.active.as_ref().unwrap().id)
            .unwrap()
            .position,
        before
    );
}
