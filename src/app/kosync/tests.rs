use super::*;
use crate::{
    app::tests::{Fixture, capture_test_screen},
    kosync::tests::Server,
};
use primitive_app::App;
use std::{fs, path::Path, time::Duration};
fn pdf(f: &Fixture) -> (Reader, PathBuf) {
    let path = f.root.join("Authored PDF.pdf");
    fs::write(&path, crate::fixed::tests::document()).unwrap();
    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut app = Reader::open(&f.root, &f.state, fonts).unwrap();
    app.open_path(&path).unwrap();
    (app, path)
}
fn remote(app: &Reader, page: usize) -> Position {
    let mut config = app.kosync.config.clone();
    config.device = "Stock QA".into();
    config.device_id = "stock-qa".into();
    Position::pdf(page, app.fixed.as_ref().unwrap().reading.count(), &config).unwrap()
}
fn pending(app: &mut Reader) -> Book {
    let book = app.kosync_book().unwrap();
    app.kosync.pending = Some(Pending {
        epoch: app.kosync.epoch,
        book: Some(book.clone()),
    });
    book
}
fn canvas(app: &Reader) -> Canvas {
    Canvas::new(WIDTH, HEIGHT, &[app.fonts[0].display().to_string()]).unwrap()
}
fn tap(app: &mut Reader, rect: Rect) -> Change {
    app.input(Input::Tap(
        (rect.x + rect.w / 2) as f32,
        (rect.y + rect.h / 2) as f32,
    ))
}
fn save(app: &mut Reader, value: &str) {
    app.editor.text = value.chars().collect();
    app.editor.caret = app.editor.text.len();
    let go = Layout::new(WIDTH, HEIGHT, 4, 5)
        .keyboard_keys_with(app.editor.layer, "Save")
        .into_iter()
        .find(|(k, _)| k.key == primitive_ui::Key::Go)
        .unwrap()
        .1;
    tap(app, go);
}
#[test]
fn unsupported_reflow_has_no_push_or_pull_and_opening_settings_is_offline() {
    let f = Fixture::new();
    let mut app = Reader::open(
        &f.root,
        &f.state,
        vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")],
    )
    .unwrap();
    app.open_path(&f.book).unwrap();
    let before = app.current_position();
    app.kosync_command(Command::Open).unwrap();
    assert!(!app.kosync.config.enabled);
    assert!(app.kosync.job.is_none());
    assert!(
        app.kosync_items()
            .iter()
            .all(|i| !matches!(i.action, Action::KoSync(Command::Fetch | Command::Push)))
    );
    assert!(app.kosync_command(Command::Push).is_err());
    assert!(app.kosync_command(Command::Fetch).is_err());
    assert_eq!(app.current_position(), before);
    app.kosync_command(Command::Settings).unwrap();
    capture_test_screen(&app, &app.fonts, "READER_KOSYNC_SETTINGS", false);
    assert!(app.kosync.job.is_none());
}
#[test]
fn account_changes_hash_password_and_sign_out_preserves_webdav_and_reading_state() {
    let f = Fixture::new();
    let (mut app, _) = pdf(&f);
    let before = app.current_position();
    let dav_path = f.root.join("progress-sync.json");
    let dav = crate::progress_sync::Config {
        username: "Existing WebDAV".into(),
        password: "authored-dav-secret".into(),
        ..crate::progress_sync::Config::default()
    };
    dav.save(&dav_path).unwrap();
    app.progress_sync = crate::app::progress_sync::View::load(dav_path.clone());
    let old = fs::read(&dav_path).unwrap();
    app.kosync.config = Server::start().config();
    app.kosync_command(Command::Open).unwrap();
    app.kosync_command(Command::Edit(Field::Password)).unwrap();
    assert!(app.editor.string().is_empty());
    save(&mut app, "authored replacement");
    assert_eq!(
        app.kosync.config.key,
        wire::password_key("authored replacement").unwrap()
    );
    assert!(!app.kosync.config.authorized && !app.kosync.config.enabled);
    assert!(app.editor.string().is_empty());
    app.kosync_command(Command::SignOut).unwrap();
    assert!(app.kosync.config.key.is_empty());
    assert_eq!(fs::read(&dav_path).unwrap(), old);
    assert_eq!(app.current_position(), before);
    assert!(
        !fs::read_to_string(&app.kosync.path)
            .unwrap()
            .contains("authored replacement")
    );
}
#[test]
fn grid_and_masked_form_close_restore_the_exact_book_frame_and_position() {
    let f = Fixture::new();
    let (mut app, _) = pdf(&f);
    app.notice = None;
    let before = app.current_position();
    let mut c = canvas(&app);
    let frame = primitive_app::frame(&app, &mut c, false);
    app.kosync_command(Command::Open).unwrap();
    capture_test_screen(&app, &app.fonts, "READER_KOSYNC_GRID", false);
    capture_test_screen(&app, &app.fonts, "READER_KOSYNC_GRID_DARK", true);
    let rect = tile(0, false);
    let normal = primitive_app::frame(&app, &mut c, false);
    c.press_at((rect.x + rect.w / 2) as f32, (rect.y + rect.h / 2) as f32);
    let pressed = primitive_app::frame(&app, &mut c, false);
    assert!(normal != pressed);
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let i = (y * WIDTH + x) as usize;
            if normal[i] != pressed[i] {
                assert!(rect.contains(x as f32, y as f32));
            }
        }
    }
    c.clear_press();
    app.kosync_command(Command::Edit(Field::Password)).unwrap();
    app.editor.text = "authored secret not visible".chars().collect();
    capture_test_screen(&app, &app.fonts, "READER_KOSYNC_PASSWORD", false);
    tap(&mut app, EDIT_BACK);
    app.kosync_command(Command::Open).unwrap();
    tap(&mut app, sheet_back(false));
    assert_eq!(app.current_position(), before);
    assert_eq!(primitive_app::frame(&app, &mut c, false), frame);
}
#[test]
fn stale_epoch_book_and_account_replies_do_not_navigate_or_authorize() {
    let f = Fixture::new();
    let (mut app, _) = pdf(&f);
    let before = app.current_position();
    let p = remote(&app, 1);
    pending(&mut app);
    app.kosync.epoch += 1;
    assert!(app.finish_kosync(Reply::Found(p.clone())).is_err());
    assert_eq!(app.current_position(), before);
    pending(&mut app);
    app.open_path(&f.book).unwrap();
    assert!(app.finish_kosync(Reply::Found(p)).is_err());
    assert!(app.kosync.remote.is_none());
    app.kosync.pending = Some(Pending {
        epoch: app.kosync.epoch,
        book: None,
    });
    app.kosync_command(Command::SignOut).unwrap();
    assert!(app.finish_kosync(Reply::Authorized).is_err());
    assert!(!app.kosync.config.authorized);
}
#[test]
fn server_page_preview_requires_visible_consent_and_reopens_exactly() {
    let f = Fixture::new();
    let (mut app, path) = pdf(&f);
    app.kosync_command(Command::Open).unwrap();
    let before = app.current_position();
    let p = remote(&app, 1);
    pending(&mut app);
    app.finish_kosync(Reply::Found(p.clone())).unwrap();
    assert_eq!(app.current_position(), before);
    capture_test_screen(&app, &app.fonts, "READER_KOSYNC_PREVIEW", false);
    capture_test_screen(&app, &app.fonts, "READER_KOSYNC_PREVIEW_DARK", true);
    tap(&mut app, KEEP);
    assert_eq!(app.current_position(), before);
    pending(&mut app);
    app.finish_kosync(Reply::Found(p)).unwrap();
    tap(&mut app, OPEN);
    assert_eq!(app.current_position().unwrap().page, 1);
    app.save().unwrap();
    let mut reopened = Reader::open(&f.root, &f.state, app.fonts.clone()).unwrap();
    reopened.open_path(&path).unwrap();
    assert_eq!(reopened.current_position(), app.current_position());
    assert!(!app.back.is_empty());
}
#[test]
fn failed_server_page_save_rolls_back_frame_marks_and_position() {
    let f = Fixture::new();
    let (mut app, _) = pdf(&f);
    app.activate(Action::AddBookmark).unwrap();
    app.show(Screen::Fixed);
    app.notice = None;
    app.save().unwrap();
    let before = app.current_position();
    let id = app.open_book_id().unwrap();
    let state = serde_json::to_vec(app.library.state(id).unwrap()).unwrap();
    let mut c = canvas(&app);
    let frame = primitive_app::frame(&app, &mut c, false);
    let book = pending(&mut app);
    let p = remote(&app, 1);
    app.kosync.remote = Some((book, p));
    let saved = f.root.join("prior-library.json");
    fs::rename(&f.state, &saved).unwrap();
    fs::create_dir(&f.state).unwrap();
    assert!(app.open_kosync_position().is_err());
    assert_eq!(app.current_position(), before);
    assert_eq!(
        serde_json::to_vec(app.library.state(id).unwrap()).unwrap(),
        state
    );
    assert_eq!(primitive_app::frame(&app, &mut c, false), frame);
    fs::remove_dir(&f.state).unwrap();
    fs::rename(saved, &f.state).unwrap();
}
fn settle(app: &mut Reader) {
    let end = Instant::now() + Duration::from_secs(20);
    while app.kosync.job.is_some() {
        assert!(Instant::now() < end);
        app.tick(Instant::now());
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        app.notice
            .as_deref()
            .is_none_or(|s| !s.contains("worker stopped")),
        "{:?}",
        app.notice
    );
}
#[test]
#[ignore = "requires built READER_KOSYNC_WORKER plus pinned KOSYNC_STOCK_ROOT/LUA and QA-local KO_HOME"]
fn actual_stock_client_and_reader_worker_roundtrip_authored_pdf_source_pages() {
    let worker = PathBuf::from(
        std::env::var_os("READER_KOSYNC_WORKER").expect("built kosync_worker example"),
    );
    let stock = PathBuf::from(std::env::var_os("KOSYNC_STOCK_ROOT").expect("pinned runtime root"));
    let lua = PathBuf::from(std::env::var_os("KOSYNC_STOCK_LUA").expect("pinned LuaJIT"));
    let f = Fixture::new();
    let (mut app, path) = pdf(&f);
    let hash = ring::digest::digest(&ring::digest::SHA256, &fs::read(&path).unwrap());
    let server = Server::start();
    app.kosync.config = server.config();
    app.kosync.test_worker = Some(worker);
    let book = app.kosync_book().unwrap();
    let script =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/helpers/kosync-stock-client.lua");
    let profile = f.root.join("isolated-stock-profile");
    fs::create_dir(&profile).unwrap();
    fs::write(
        profile.join("settings.reader.lua"),
        "return {language='en',external_dict_lookup=false,emulator_fake_wifi_connected=false}",
    )
    .unwrap();
    let staging = stock
        .parent()
        .and_then(Path::parent)
        .expect("QA runtime layout")
        .join("build/staging");
    assert!(staging.is_dir());
    let output = f.root.join("stock-result.json");
    let run_stock = |mode: &str| {
        let status = std::process::Command::new(&lua)
            .arg(&script)
            .current_dir(&stock)
            .env("KO_HOME", &profile)
            .env("KO_DYLD_PREFIX", &staging)
            .env("EMULATE_READER_W", "632")
            .env("EMULATE_READER_H", "840")
            .env("EMULATE_READER_DPI", "300")
            .env("KOSYNC_QA_URL", &server.url)
            .env("KOSYNC_QA_DOCUMENT", &book.document)
            .env("KOSYNC_QA_PDF", &path)
            .env("KOSYNC_QA_MODE", mode)
            .env("KOSYNC_QA_OUTPUT", &output)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(status.success());
        serde_json::from_slice::<serde_json::Value>(&fs::read(&output).unwrap()).unwrap()
    };
    let stock_push = run_stock("push");
    assert_eq!(stock_push["pdf_pages"], 2);
    assert_eq!(stock_push["progress"], "2");
    app.kosync_command(Command::Open).unwrap();
    app.kosync_command(Command::Fetch).unwrap();
    settle(&mut app);
    assert!(app.kosync.preview, "{:?}", app.notice);
    assert_eq!(app.current_position().unwrap().page, 0);
    tap(&mut app, OPEN);
    assert_eq!(app.current_position().unwrap().page, 1);
    app.jump_fixed(0, None).unwrap();
    app.kosync_command(Command::Open).unwrap();
    app.kosync_command(Command::Push).unwrap();
    settle(&mut app);
    assert_eq!(run_stock("pull")["progress"], "1");
    assert_eq!(
        hash.as_ref(),
        ring::digest::digest(&ring::digest::SHA256, &fs::read(&path).unwrap()).as_ref()
    );
    assert!(server.calls() >= 8);
    assert_eq!(app.current_position().unwrap().page, 0);
}
#[test]
fn sleep_usb_and_leaving_provider_cancel_owned_work_without_moving_the_pdf() {
    let f = Fixture::new();
    let (mut app, _) = pdf(&f);
    let server = Server::start();
    app.kosync.config = server.config();
    let before = app.current_position();
    for phase in 0..3 {
        app.kosync_command(Command::Open).unwrap();
        let request = Request {
            config: app.kosync.config.clone(),
            operation: Operation::Authorize,
        };
        let mut process = std::process::Command::new("sleep");
        process.arg("30");
        app.kosync.job = Some(Job::spawn(process, &request).unwrap());
        app.kosync.pending = Some(Pending {
            epoch: app.kosync.epoch,
            book: None,
        });
        match phase {
            0 => {
                app.input(Input::Sleep);
                assert!(!app.stats_awake);
                app.input(Input::Wake);
                assert!(app.stats_awake);
            }
            1 => {
                app.save().unwrap();
                app.storage_returned().unwrap();
            }
            _ => app.show(Screen::Fixed),
        }
        assert!(app.kosync.job.is_none());
        assert!(app.kosync.pending.is_none());
        assert_eq!(app.current_position(), before);
    }
    assert_eq!(server.calls(), 0);
}
