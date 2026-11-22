use super::*;
use crate::app::tests::{Fixture, capture_test_screen};
use serde_json::json;

fn fonts() -> Vec<PathBuf> {
    vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")]
}
fn open(fixture: &Fixture) -> Reader {
    Reader::open(&fixture.root, &fixture.state, fonts()).unwrap()
}
fn put(root: &Path, name: &str, value: Value) {
    fs::write(root.join(name), serde_json::to_vec(&value).unwrap()).unwrap();
}
fn source(fixture: &Fixture) -> (Reader, u64) {
    let mut app = open(fixture);
    app.open_path(&fixture.book).unwrap();
    app.activate(Action::ToggleBookmark).unwrap();
    let reading = &app.active.as_ref().unwrap().reading;
    let probe = &reading.probe;
    let block = probe
        .source
        .blocks
        .iter()
        .find(|block| block.text.chars().count() > 12)
        .unwrap();
    let start = probe.anchor(block.key, 0);
    let end_offset = block.text.char_indices().nth(8).unwrap().0;
    let end = probe.anchor(block.key, end_offset);
    app.selection = probe.select_range(&start, &end);
    app.save_highlight(Some("Keep this passage".into()))
        .unwrap();
    let id = app.active.as_ref().unwrap().id;
    app.save().unwrap();
    app.controls.large_toolbar = true;
    app.controls.highlight_appearance.line_height = 0;
    app.controls
        .save(&fixture.root.join("reading-controls.json"))
        .unwrap();
    drop(app);
    put(
        &fixture.root,
        "reading-profiles.json",
        json!([{"name":"Evening","preferences":{"font_size":58.0,"line_height":1.6}}]),
    );
    put(
        &fixture.root,
        "reading-gestures.json",
        json!({"tap_top_left":"contents","left_up":"find"}),
    );
    put(
        &fixture.root,
        "dictionary-preference.json",
        json!({"order":["Coast.ifo"],"disabled":["Legacy.ifo"],"disable_word_forms":true}),
    );
    put(
        &fixture.root,
        "lookup-history.json",
        json!({
        "searches":[{"text":"harbor","match_case":false,"ignore_accents":true,"regular_expression":false}],
        "dictionary":["harbor"],"vocabulary":["harbor"],
        "review":{"automatic":true,"with_context":true,"due_only":true,"excluded_books":["Other book"],"words":{
            "harbor":{"book":"Field notes","created_at":1700000000u64,"reviewed_at":1700000100u64,
                "due_at":1700086500u64,"reviews":7,"streak":3,"before":"The quiet","after":"was calm","highlight":"harbor"}}},
        "online":[{"service":"Wikipedia","text":"Source remote history","from":"auto","to":"en","full_article":false}],
        "online_languages":{"wikipedia":"vi","source":"auto","target":"en","translator":"Google","forget_queries":false}}),
    );
    put(
        &fixture.root,
        "reading-stats.json",
        json!({"version":1,"seconds_read":{(id.to_string()):222}}),
    );
    put(
        &fixture.root,
        "reading-history.json",
        json!({"version":1,"options":{"min_seconds":5,"max_seconds":120},
        "prior":{(id.to_string()):20},"titles":{(id.to_string()):"Field notes"},
        "visits":[{"book":id,"page":42,"start":1700000000i64,"seconds":37,"id":1}]}),
    );
    fs::create_dir_all(fixture.root.join("reading-pace")).unwrap();
    put(
        &fixture.root,
        &format!("reading-pace/{id:016x}.json"),
        json!({"version":1,"pages":{"1":30,"2":40}}),
    );
    let mut rules = crate::user_hyphenation::Rules::default();
    rules.enabled = true;
    rules.set("en-US", "laboratory", "labo-ratory").unwrap();
    put(
        &fixture.root,
        "user-hyphenation.json",
        serde_json::to_value(rules).unwrap(),
    );
    put(
        &fixture.root,
        "bookmark-browser.json",
        json!({"source":{"Books":[id]},"kind":"Note","style":"underscore","sort_by_date":true,"reverse":true}),
    );
    put(
        &fixture.root,
        "bookmark-metadata.json",
        json!({(id.to_string()):{
        "series":["Harbor notebooks"],"language":"en","keywords":["Coast"],
        "description":"Context retained after the original source is removed",
        "has_cover":true,"chapters":{"notes.xhtml":0},"fixed":false}}),
    );
    put(
        &fixture.root,
        "read-timer.json",
        json!({"minutes":25,"repeat":true,"header":true,
        "footer":false,"expiry":"notification","message":"Rest your eyes"}),
    );
    put(
        &fixture.root,
        "document-defaults.json",
        json!({"version":1,"reflow":{"font_size":55.0},"fixed":null}),
    );
    put(
        &fixture.root,
        "opds-catalogs.json",
        json!({"catalogs":[{"name":"Private","url":"https://catalog.example/opds","username":"reader","password":"source-secret"}]}),
    );
    put(
        &fixture.root,
        "progress-sync.json",
        json!({"enabled":false,"username":"private","password":"sync-secret","device":"Source"}),
    );
    (open(fixture), id)
}
fn envelope(app: &mut Reader) -> Vec<u8> {
    app.reading_backup_bytes().unwrap()
}
fn preview(app: &mut Reader, bytes: &[u8]) {
    app.backup_import = Some(app.prepare_reading_backup(bytes).unwrap());
    app.show(Screen::RestoreBackup);
}
fn select_all(app: &mut Reader) {
    let indices: Vec<_> = app
        .backup_import
        .as_ref()
        .unwrap()
        .components
        .iter()
        .enumerate()
        .filter_map(|(i, c)| (!c.selected).then_some(i))
        .collect();
    for index in indices {
        app.activate(Action::ToggleBackupComponent(index)).unwrap();
    }
}
fn digest(bytes: &[u8]) -> Vec<u8> {
    ring::digest::digest(&ring::digest::SHA256, bytes)
        .as_ref()
        .to_vec()
}

#[test]
fn complete_backup_round_trip_fresh_profile_repeated_replace_and_legacy_merge_preserve_reading_data()
 {
    let fixture = Fixture::new();
    let original = digest(&fs::read(&fixture.book).unwrap());
    let (mut source, id) = source(&fixture);
    let bytes = envelope(&mut source);
    let text = std::str::from_utf8(&bytes).unwrap();
    assert!(!text.contains("source-secret") && !text.contains("sync-secret"));
    assert!(!text.contains("Source remote history") && !text.contains("online_languages"));
    let saved = Envelope::parse(&bytes).unwrap();
    assert!(saved.components.len() >= 13);
    let target = Fixture::new();
    let mut restored = open(&target);
    preview(&mut restored, &bytes);
    assert!(
        restored
            .backup_import
            .as_ref()
            .unwrap()
            .components
            .iter()
            .all(|c| c.selected && !c.existing)
    );
    capture_test_screen(&restored, &fonts(), "READER_BACKUP_NEW_SCREENSHOT", false);
    restored.activate(Action::MergeBackup).unwrap();
    assert_eq!(restored.screen, Screen::ReadingData);
    assert_eq!(restored.controls, source.controls);
    assert_eq!(restored.lookup_history.review.words["harbor"].streak, 3);
    assert_eq!(
        restored.lookup_history.review.words["harbor"].due_at,
        Some(1700086500)
    );
    assert!(restored.lookup_history.online.is_empty());
    assert_eq!(restored.reading_history.visits().len(), 1);
    assert_eq!(restored.reading_history.visits()[0].seconds, 37);
    assert_eq!(restored.library.state(id).unwrap().bookmarks.len(), 1);
    assert_eq!(
        restored.library.state(id).unwrap().highlights[0]
            .note
            .as_deref(),
        Some("Keep this passage")
    );
    assert!(restored.read_alarm.caption(Instant::now()).is_none());
    assert!(!target.root.join("opds-catalogs.json").exists());
    assert!(!target.root.join("progress-sync.json").exists());
    for saved_component in &saved.components {
        let actual = component(&target.state, saved_component.key)
            .unwrap()
            .unwrap();
        assert_eq!(
            actual.data,
            saved_component.data,
            "{}",
            saved_component.key.label()
        );
    }
    preview(&mut restored, &bytes);
    assert!(
        restored
            .backup_import
            .as_ref()
            .unwrap()
            .components
            .iter()
            .all(|c| !c.selected && c.existing)
    );
    capture_test_screen(
        &restored,
        &fonts(),
        "READER_BACKUP_EXISTING_SCREENSHOT",
        false,
    );
    restored.activate(Action::MergeBackup).unwrap();
    assert_eq!(restored.reading_history.visits().len(), 1);
    preview(&mut restored, &bytes);
    select_all(&mut restored);
    restored.activate(Action::MergeBackup).unwrap();
    assert_eq!(restored.reading_history.visits().len(), 1);
    assert_eq!(restored.lookup_history.review.words["harbor"].reviews, 7);
    assert_eq!(restored.library.state(id).unwrap().highlights.len(), 1);
    // A v1 library-only backup still uses the additive merge and touches no global component.
    let legacy = source.library.backup_bytes().unwrap();
    preview(&mut restored, &legacy);
    assert!(
        restored
            .backup_import
            .as_ref()
            .unwrap()
            .components
            .is_empty()
    );
    restored.activate(Action::MergeBackup).unwrap();
    assert_eq!(restored.controls, source.controls);
    restored.activate(Action::Continue).unwrap();
    assert_eq!(restored.screen, Screen::Book);
    assert!(restored.current_position().is_some());
    assert_eq!(digest(&fs::read(&fixture.book).unwrap()), original);
    assert_eq!(digest(&fs::read(&target.book).unwrap()), original);
}

#[test]
fn complete_backup_keeps_existing_components_until_selected_and_preserves_local_online_account_data()
 {
    let fixture = Fixture::new();
    let (mut source, id) = source(&fixture);
    let bytes = envelope(&mut source);
    let target = Fixture::new();
    let mut prior = open(&target);
    prior.open_path(&target.book).unwrap();
    let current = prior.active.as_ref().unwrap().reading.config.settings;
    prior
        .activate(Action::Text(crate::Settings {
            font_size: 72.0,
            ..current
        }))
        .unwrap();
    prior.controls.large_toolbar = false;
    prior
        .controls
        .save(&target.root.join("reading-controls.json"))
        .unwrap();
    prior.lookup_history.online_languages.wikipedia = "fr".into();
    prior.lookup_history.online = vec![crate::online_lookup::Request {
        service: crate::online_lookup::Service::Wikipedia,
        text: "Target remote history".into(),
        from: "auto".into(),
        to: "fr".into(),
        full_article: false,
    }];
    prior.lookup_history.vocabulary.push("local-only".into());
    prior
        .lookup_history
        .save(&target.root.join("lookup-history.json"))
        .unwrap();
    prior.save().unwrap();
    let position = prior.library.state(id).unwrap().position.clone();
    put(
        &target.root,
        "opds-catalogs.json",
        json!({"catalogs":[{"name":"Local","url":"https://existing.example/opds","password":"keep-existing-account"}]}),
    );
    let account = fs::read(target.root.join("opds-catalogs.json")).unwrap();
    preview(&mut prior, &bytes);
    let lookup = prior
        .backup_import
        .as_ref()
        .unwrap()
        .components
        .iter()
        .position(|c| c.key == Key::Lookup)
        .unwrap();
    assert!(!prior.backup_import.as_ref().unwrap().components[lookup].selected);
    prior
        .activate(Action::ToggleBackupComponent(lookup))
        .unwrap();
    capture_test_screen(&prior, &fonts(), "READER_BACKUP_REPLACE_SCREENSHOT", false);
    capture_test_screen(&prior, &fonts(), "READER_BACKUP_DARK_SCREENSHOT", true);
    prior.activate(Action::MergeBackup).unwrap();
    assert!(!prior.controls.large_toolbar);
    assert_eq!(
        prior.library.state(id).unwrap().preferences.font_size,
        Some(72.0)
    );
    assert_eq!(prior.library.state(id).unwrap().position, position);
    assert_eq!(prior.lookup_history.vocabulary, vec!["harbor"]);
    assert_eq!(prior.lookup_history.online[0].text, "Target remote history");
    assert_eq!(prior.lookup_history.online_languages.wikipedia, "fr");
    assert_eq!(
        fs::read(target.root.join("opds-catalogs.json")).unwrap(),
        account
    );
    assert_eq!(prior.library.state(id).unwrap().highlights.len(), 1);
}

#[test]
fn complete_backup_rejects_malformed_typed_components_without_writing_profile_or_source_files() {
    let fixture = Fixture::new();
    let (mut source, _) = source(&fixture);
    let bytes = envelope(&mut source);
    let target = Fixture::new();
    let app = open(&target);
    let before = fs::read(&target.book).unwrap();
    for (key, data) in [
        (Key::Controls, json!({"numbering":{"stride":123}})),
        (
            Key::Controls,
            json!({"highlight_appearance":{"line_height":101}}),
        ),
        (
            Key::Profiles,
            json!([{"name":"Bad","preferences":{"font_size":-1.0}}]),
        ),
        (Key::Gestures, json!({"two-finger-unknown":"find"})),
        (Key::Dictionaries, json!({"order":["../escape.ifo"]})),
        (
            Key::Lookup,
            json!({"review":{"words":{"word":{"due_at":"tomorrow"}}}}),
        ),
        (
            Key::History,
            json!({"version":99,"prior":{},"titles":{},"visits":[]}),
        ),
        (Key::Statistics, json!({"version":99,"seconds_read":{}})),
        (
            Key::Hyphenation,
            json!({"words":{"en-US":{"laboratory":[0]}}}),
        ),
        (
            Key::Browser,
            json!({"source":{"Folder":["../outside",true]}}),
        ),
        (
            Key::BookMetadata,
            json!({"1":{"description":"x".repeat(8193)}}),
        ),
        (Key::Timer, json!({"minutes":2000})),
        (Key::Pace(1), json!({"version":1,"pages":{"1":0}})),
        (
            Key::DocumentDefaults,
            json!({"version":99,"reflow":null,"fixed":null}),
        ),
    ] {
        let mut altered = Envelope::parse(&bytes).unwrap();
        altered.components.retain(|c| c.key != key);
        altered.components.push(Component { key, data });
        assert!(
            app.prepare_reading_backup(&altered.bytes().unwrap())
                .is_err(),
            "{}",
            key.label()
        );
        assert!(!target.state.exists());
        assert!(!target.root.join("reading-controls.json").exists());
        assert_eq!(fs::read(&target.book).unwrap(), before);
    }
}

#[test]
fn complete_backup_startup_rolls_back_an_interrupted_commit_before_loading_library_and_controls() {
    let fixture = Fixture::new();
    let (mut prior, _) = source(&fixture);
    prior.save().unwrap();
    let state = fs::read(&fixture.state).unwrap();
    let controls = fs::read(fixture.root.join("reading-controls.json")).unwrap();
    let original = digest(&fs::read(&fixture.book).unwrap());
    drop(prior);
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        archive::interrupt_for_test(
            &fixture.state,
            &[
                (Key::Library, b"{ incomplete library".to_vec()),
                (Key::Controls, b"{ incomplete controls".to_vec()),
            ],
            2,
        );
    }));
    assert_ne!(fs::read(&fixture.state).unwrap(), state);
    let restored = open(&fixture);
    assert!(restored.controls.large_toolbar);
    assert_eq!(fs::read(&fixture.state).unwrap(), state);
    assert_eq!(
        fs::read(fixture.root.join("reading-controls.json")).unwrap(),
        controls
    );
    assert_eq!(digest(&fs::read(&fixture.book).unwrap()), original);
}

#[test]
#[ignore = "requires READER_BACKUP_ORIGINAL pointing to an authorized unchanged EPUB or PDF copy"]
fn complete_backup_original_document_marks_and_state_round_trip_without_changing_source() {
    let input = PathBuf::from(std::env::var_os("READER_BACKUP_ORIGINAL").unwrap());
    let bytes = fs::read(&input).unwrap();
    let hash = digest(&bytes);
    let source_fixture = Fixture::new();
    let name = PathBuf::from("Original").with_extension(input.extension().unwrap());
    let source_path = source_fixture.root.join(&name);
    fs::write(&source_path, &bytes).unwrap();
    let mut source = open(&source_fixture);
    source.open_path(&source_path).unwrap();
    source.activate(Action::ToggleBookmark).unwrap();
    source.input(Input::PageForward);
    source.save().unwrap();
    let saved = source.current_position().unwrap();
    let id = source.open_book_id().unwrap();
    let backup = envelope(&mut source);
    let target = Fixture::new();
    fs::write(target.root.join(&name), &bytes).unwrap();
    let mut restored = open(&target);
    preview(&mut restored, &backup);
    restored.activate(Action::MergeBackup).unwrap();
    restored.open_path(&target.root.join(&name)).unwrap();
    assert_eq!(restored.current_position().unwrap(), saved);
    assert_eq!(restored.library.state(id).unwrap().bookmarks.len(), 1);
    assert_eq!(digest(&fs::read(&input).unwrap()), hash);
    assert_eq!(digest(&fs::read(source_path).unwrap()), hash);
    assert_eq!(digest(&fs::read(target.root.join(name)).unwrap()), hash);
}
