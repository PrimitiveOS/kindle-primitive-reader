use super::*;
use crate::app::tests::{Fixture, capture_test_screen};
use crate::opds::tests::{Server, response};

fn editor(app: &mut Reader, field: Field, value: &str) {
    app.activate(Action::Catalog(Command::Field(field)))
        .unwrap();
    app.editor.text = value.chars().collect();
    app.editor.caret = app.editor.text.len();
    let action = if matches!(field, Field::Search) {
        "Search"
    } else {
        "Apply"
    };
    let rect = Layout::new(WIDTH, HEIGHT, 4, 5)
        .keyboard_keys_with(app.editor.layer, action)
        .into_iter()
        .find(|(key, _)| key.key == primitive_ui::Key::Go)
        .unwrap()
        .1;
    assert_eq!(
        app.input(Input::Tap(
            (rect.x + rect.w / 2) as f32,
            (rect.y + rect.h / 2) as f32
        )),
        Change::Partial
    );
}
fn setup(fixture: &Fixture) -> Reader {
    Reader::open(
        &fixture.root,
        &fixture.state,
        vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")],
    )
    .unwrap()
}
fn catalog(app: &mut Reader, url: &str) {
    app.activate(Action::Catalog(Command::Open)).unwrap();
    app.activate(Action::Catalog(Command::Add)).unwrap();
    for (field, value) in [
        (Field::Name, "Reference shelves"),
        (Field::Url, url),
        (Field::Username, "reader"),
        (Field::Password, "catalog secret"),
    ] {
        editor(app, field, value);
    }
    app.activate(Action::Catalog(Command::Save)).unwrap();
}
fn inject_feed(app: &mut Reader, url: &str) {
    let source = serde_json::json!({"metadata":{"title":"Coastal library"},"links":[{"rel":"next","href":"?page=2","type":"application/opds+json"},{"rel":"search","href":"search{?query}","type":"application/opds+json"}],
        "publications":[{"metadata":{"title":"Field notes","author":"Mira Coast","description":"An illustrated notebook of tides, harbors and evening walks. Choose a format to keep a copy in Documents and read it offline."},
        "links":[{"rel":"http://opds-spec.org/acquisition/open-access","href":"field-notes.epub","type":"application/epub+zip"},{"rel":"download","href":"field-notes.pdf","type":"application/pdf"},{"rel":"http://opds-spec.org/acquisition/borrow","href":"borrow","type":"application/vnd.adobe.adept+xml"}]}]});
    let Parsed::Feed(feed) = catalog::parse(&serde_json::to_vec(&source).unwrap(), url).unwrap()
    else {
        panic!()
    };
    app.catalog.feed = Some(feed);
    app.catalog.selected_catalog = Some(0);
    app.show(Screen::Catalog(Page::Feed));
}
#[test]
fn opds_ui_configures_credentials_folder_details_and_explicit_format_choice_without_network() {
    let fixture = Fixture::new();
    let original = fs::read(&fixture.book).unwrap();
    fs::create_dir(fixture.root.join("Downloads")).unwrap();
    fs::create_dir(fixture.root.join("Publication.sdr")).unwrap();
    let mut app = setup(&fixture);
    catalog(&mut app, "http://127.0.0.1:8080/opds");
    assert_eq!(app.catalog.settings.catalogs.len(), 1);
    assert_eq!(app.catalog.settings.catalogs[0].password, "catalog secret");
    assert!(app.catalog.job.is_none());
    app.activate(Action::Catalog(Command::Folder(PathBuf::new())))
        .unwrap();
    assert!(
        app.catalog_items(Page::Folder)
            .iter()
            .any(|row| row.title == "Downloads")
    );
    assert!(
        !app.catalog_items(Page::Folder)
            .iter()
            .any(|row| row.title == "Publication.sdr")
    );
    app.activate(Action::Catalog(Command::Folder("Downloads".into())))
        .unwrap();
    app.activate(Action::Catalog(Command::UseFolder)).unwrap();
    assert_eq!(app.screen, Screen::Catalog(Page::Catalogs));
    assert_eq!(app.catalog.settings.folder, Path::new("Downloads"));
    capture_test_screen(&app, &app.fonts, "READER_OPDS_CATALOGS_SCREENSHOT", false);
    inject_feed(&mut app, "http://127.0.0.1:8080/opds");
    assert!(
        app.catalog_items(Page::Feed)
            .iter()
            .any(|row| row.title == "Next catalog page")
    );
    capture_test_screen(&app, &app.fonts, "READER_OPDS_FEED_SCREENSHOT", false);
    app.activate(Action::Catalog(Command::Select(0))).unwrap();
    assert_eq!(app.screen, Screen::Catalog(Page::Details));
    assert!(app.catalog.job.is_none());
    capture_test_screen(&app, &app.fonts, "READER_OPDS_DETAILS_SCREENSHOT", false);
    app.activate(Action::Catalog(Command::Formats)).unwrap();
    let rows = app.catalog_items(Page::Formats);
    assert!(rows.iter().any(|row| row.title == "Download EPUB"));
    assert!(rows.iter().any(|row| row.title == "Download PDF"));
    assert!(rows.iter().any(|row| matches!(row.action, Action::Heading)));
    capture_test_screen(&app, &app.fonts, "READER_OPDS_FORMATS_SCREENSHOT", false);
    capture_test_screen(&app, &app.fonts, "READER_OPDS_DARK_SCREENSHOT", true);
    let reopened = setup(&fixture);
    assert_eq!(reopened.catalog.settings.catalogs[0].username, "reader");
    assert_eq!(reopened.catalog.settings.folder, Path::new("Downloads"));
    assert!(reopened.catalog.job.is_none());
    assert_eq!(fs::read(&fixture.book).unwrap(), original);
}
#[test]
fn opds_ui_delete_confirms_and_stale_owned_partial_cleanup_preserves_publications() {
    let fixture = Fixture::new();
    let before = fs::read(&fixture.book).unwrap();
    let mut app = setup(&fixture);
    catalog(&mut app, "http://localhost/opds");
    app.activate(Action::Catalog(Command::Edit(0))).unwrap();
    app.activate(Action::Catalog(Command::Remove)).unwrap();
    assert!(app.catalog.confirm_remove);
    let cancel = Sheet::centered(Rect::new(0, 0, WIDTH as i32, HEIGHT as i32), "", "", "")
        .button(SheetAction::Cancel);
    app.input(Input::Tap((cancel.x + 20) as f32, (cancel.y + 20) as f32));
    assert_eq!(app.catalog.settings.catalogs.len(), 1);
    app.activate(Action::Catalog(Command::Remove)).unwrap();
    let yes = Sheet::centered(Rect::new(0, 0, WIDTH as i32, HEIGHT as i32), "", "", "")
        .button(SheetAction::Confirm);
    app.input(Input::Tap((yes.x + 20) as f32, (yes.y + 20) as f32));
    assert!(app.catalog.settings.catalogs.is_empty());
    let temp = catalog::temporary(&app.catalog.root, Path::new(""), "epub").unwrap();
    fs::write(app.catalog.root.join(&temp), b"Owned partial").unwrap();
    app.catalog.settings.pending = Some(temp.clone());
    app.catalog.save().unwrap();
    drop(app);
    let app = setup(&fixture);
    assert!(!app.catalog.root.join(temp).exists());
    assert!(app.catalog.settings.pending.is_none());
    assert_eq!(fs::read(&fixture.book).unwrap(), before);
}
fn settle(app: &mut Reader) {
    for _ in 0..1000 {
        let Some(due) = app.catalog.job.as_ref().map(|job| job.poll_at) else {
            return;
        };
        app.tick(due + Duration::from_millis(1));
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("The actual OPDS worker did not finish");
}
#[test]
#[ignore = "requires READER_OPDS_WORKER pointing to freshly built reader-kindle"]
fn opds_ui_actual_worker_browses_searches_acquires_cancels_and_opens_offline() {
    let fixture = Fixture::new();
    let book = fs::read(&fixture.book).unwrap();
    let body = book.clone();
    let server = Server::new(move |request, stream| {
        if request.starts_with("GET /book.epub") {
            response(
                stream,
                "200 OK",
                "Content-Type: application/epub+zip\r\n",
                &body,
            );
        } else {
            let feed = serde_json::json!({"metadata":{"title":"Local collection"},"links":[{"rel":"search","href":"/search{?query}","type":"application/opds+json"}],"publications":[{"metadata":{"title":"Acquired field notes","author":"Mira Coast"},"links":[{"rel":"download","href":"/book.epub","type":"application/epub+zip"}]}]});
            response(
                stream,
                "200 OK",
                "Content-Type: application/opds+json\r\n",
                &serde_json::to_vec(&feed).unwrap(),
            );
        }
    });
    let mut app = setup(&fixture);
    app.catalog.worker = Some(PathBuf::from(
        std::env::var_os("READER_OPDS_WORKER").unwrap(),
    ));
    catalog(&mut app, &server.url);
    app.activate(Action::Catalog(Command::Connect(0))).unwrap();
    assert!(app.catalog.job.is_some());
    settle(&mut app);
    assert_eq!(app.screen, Screen::Catalog(Page::Feed));
    app.activate(Action::Catalog(Command::Search)).unwrap();
    editor(&mut app, Field::Search, "tides & harbors");
    settle(&mut app);
    assert_eq!(app.screen, Screen::Catalog(Page::Feed));
    app.activate(Action::Catalog(Command::Select(0))).unwrap();
    app.activate(Action::Catalog(Command::Formats)).unwrap();
    app.activate(Action::Catalog(Command::Download(0))).unwrap();
    settle(&mut app);
    assert_eq!(app.screen, Screen::Catalog(Page::Complete));
    let file = app.catalog.settings.last_download.clone().unwrap();
    assert_eq!(fs::read(app.catalog.root.join(&file)).unwrap(), book);
    drop(server);
    app.activate(Action::Catalog(Command::OpenDownloaded))
        .unwrap();
    assert_eq!(app.screen, Screen::Book);
    app.save().unwrap();
    let mut reopened = setup(&fixture);
    reopened
        .activate(Action::Catalog(Command::OpenDownloaded))
        .unwrap();
    assert_eq!(reopened.screen, Screen::Book);
    // A later explicit catalog request is cancellable without changing the downloaded book.
    reopened.catalog.worker = app.catalog.worker.clone();
    reopened
        .activate(Action::Catalog(Command::Connect(0)))
        .unwrap();
    reopened.input(Input::Sleep);
    assert!(reopened.catalog.job.is_none());
    assert_ne!(reopened.screen, Screen::Catalog(Page::Loading));
    assert_eq!(fs::read(reopened.catalog.root.join(file)).unwrap(), book);
}

#[test]
#[ignore = "requires an isolated loopback calibre-server, READER_OPDS_CALIBRE_URL, READER_OPDS_WORKER and READER_OPDS_ORIGINAL"]
fn opds_actual_calibre_catalog_search_pagination_acquisition_and_resume() {
    let url = std::env::var("READER_OPDS_CALIBRE_URL").unwrap();
    assert_eq!(
        catalog::checked_url(&url).unwrap().host_str(),
        Some("127.0.0.1")
    );
    let original = PathBuf::from(std::env::var_os("READER_OPDS_ORIGINAL").unwrap());
    let source = fs::read(&original).unwrap();
    let fixture = Fixture::new();
    let mut app = setup(&fixture);
    app.catalog.worker = Some(PathBuf::from(
        std::env::var_os("READER_OPDS_WORKER").unwrap(),
    ));
    catalog(&mut app, &url);
    if std::env::var_os("READER_OPDS_CALIBRE_AUTH").is_some() {
        app.catalog.settings.catalogs[0].password = "incorrect test password".into();
        app.activate(Action::Catalog(Command::Connect(0))).unwrap();
        settle(&mut app);
        assert_eq!(app.screen, Screen::Catalog(Page::Catalogs));
        assert!(
            app.notice
                .as_deref()
                .is_some_and(|n| n.contains("credentials"))
        );
        app.catalog.settings.catalogs[0].password = "catalog secret".into();
    }
    app.activate(Action::Catalog(Command::Connect(0))).unwrap();
    settle(&mut app);
    assert_eq!(app.screen, Screen::Catalog(Page::Feed));
    let newest = app
        .catalog
        .feed
        .as_ref()
        .unwrap()
        .entries
        .iter()
        .position(|entry| entry.title == "By Newest")
        .unwrap();
    app.activate(Action::Catalog(Command::Select(newest)))
        .unwrap();
    settle(&mut app);
    let first = app.catalog.feed.as_ref().unwrap().clone();
    assert_eq!(first.entries.len(), 1); // calibre-server --max-opds-items 1
    let next = first.next.as_ref().unwrap().url.clone();
    app.activate(Action::Catalog(Command::Fetch(next, true)))
        .unwrap();
    settle(&mut app);
    let second = app.catalog.feed.as_ref().unwrap();
    assert_ne!(first.entries[0].title, second.entries[0].title);
    assert!(second.previous.is_some());
    app.catalog_back(Page::Feed);
    assert_eq!(app.catalog.feed.as_ref().unwrap().url, first.url);
    app.activate(Action::Catalog(Command::Search)).unwrap();
    editor(&mut app, Field::Search, "Original EPUB verification");
    settle(&mut app);
    assert_eq!(app.screen, Screen::Catalog(Page::Feed));
    assert_eq!(app.catalog.feed.as_ref().unwrap().entries.len(), 1);
    assert_eq!(
        app.catalog.feed.as_ref().unwrap().entries[0].title,
        "Original EPUB verification"
    );
    capture_test_screen(&app, &app.fonts, "READER_OPDS_CALIBRE_SCREENSHOT", false);
    app.activate(Action::Catalog(Command::Select(0))).unwrap();
    app.activate(Action::Catalog(Command::Formats)).unwrap();
    let format = app
        .catalog
        .selected
        .as_ref()
        .unwrap()
        .acquisitions
        .iter()
        .position(|link| link.extension() == Some("epub"))
        .unwrap();
    app.activate(Action::Catalog(Command::Download(format)))
        .unwrap();
    settle(&mut app);
    assert_eq!(app.screen, Screen::Catalog(Page::Complete));
    let relative = app.catalog.settings.last_download.clone().unwrap();
    let publication = app.catalog.root.join(&relative);
    // Calibre rewrites OPF metadata on acquisition. Every original spine document
    // remains byte-identical after decoding, while the authorized input is never edited.
    let mut before = crate::Book::open(&original).unwrap();
    let mut acquired = crate::Book::open(&publication).unwrap();
    assert_eq!(before.chapters.len(), acquired.chapters.len());
    for index in 0..before.chapters.len() {
        assert_eq!(
            before.chapter(index).unwrap(),
            acquired.chapter(index).unwrap()
        );
    }
    app.activate(Action::Catalog(Command::OpenDownloaded))
        .unwrap();
    assert_eq!(app.screen, Screen::Book);
    app.input(Input::PageForward);
    let position = app.current_position().unwrap();
    app.save().unwrap();
    drop(app);
    let mut reopened = setup(&fixture);
    assert!(reopened.catalog.job.is_none());
    reopened
        .activate(Action::Catalog(Command::OpenDownloaded))
        .unwrap();
    assert_eq!(reopened.screen, Screen::Book);
    assert_eq!(reopened.current_position().unwrap(), position);
    assert_eq!(fs::read(&original).unwrap(), source);
    if let Some(output) = std::env::var_os("READER_OPDS_CALIBRE_DOWNLOAD") {
        fs::copy(&publication, PathBuf::from(output)).unwrap();
    }
}
