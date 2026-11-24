use super::*;
use crate::app::selection_viewport_tests::{adopt, capture, key, tap};
use crate::app::tests::Fixture;
use crate::app::{SourceAction, SourcePreferences, fs};
use crate::dictionary::tests::write_dictionary;
use crate::session::ReadingViewport;
use primitive_app::App;
use primitive_ui::Key;

fn row(app: &mut Reader, title: &str) {
    let index = app
        .items()
        .iter()
        .position(|item| item.title == title)
        .unwrap();
    let geometry = app.list_geometry();
    app.navigate_list_page(index / geometry.rows);
    tap(app, geometry.row(index % geometry.rows));
}

fn back(app: &mut Reader) {
    tap(app, app.navigation_layout().back());
}

fn name(app: &mut Reader, value: &str) {
    app.editor.text = value.chars().collect();
    app.editor.caret = app.editor.text.len();
    tap(app, key(app, Key::Go, "Save"));
}

#[test]
fn dictionary_presets_manage_membership_without_overwriting_edition_preferences() {
    let fixture = Fixture::new();
    let dictionaries = fixture.root.join("dictionaries");
    fs::create_dir(&dictionaries).unwrap();
    let prepare = |name| {
        write_dictionary(
            &dictionaries,
            name,
            "sametypesequence=m\n",
            &[("tide", b"The regular rise and fall of water.")],
        );
    };
    prepare("Coastal");
    prepare("Reference");
    let pdf = fixture.root.join("Other edition.pdf");
    fs::write(&pdf, crate::fixed::tests::document()).unwrap();
    let open = || {
        Reader::open(
            &fixture.root,
            &fixture.state,
            vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")],
        )
        .unwrap()
    };
    let mut app = open();
    app.open_path(&fixture.book).unwrap();
    let origin = app.current_history_location();
    app.activate(Action::ShowDictionaries).unwrap();
    row(&mut app, "Reference");
    row(&mut app, "Disable by default");
    back(&mut app);
    row(&mut app, "Dictionary presets");
    row(&mut app, "Create preset from enabled dictionaries");
    assert_eq!(
        app.screen,
        Screen::DictionaryPresets(Page::Name(Name::Create))
    );
    let mut landscape = adopt(&mut app, ReadingViewport::LANDSCAPE, 1);
    name(&mut app, "Local tides");
    assert_eq!(
        app.dictionary_settings.presets.get(0).unwrap().1,
        [PathBuf::from("Coastal.ifo")]
    );
    row(&mut app, "Local tides");
    capture(
        &app,
        &mut landscape,
        "landscape-dictionary-preset.png",
        false,
    );
    back(&mut app);
    back(&mut app);

    prepare("Recently added");
    app.refresh_installed_dictionaries().unwrap();
    app.activate(Action::DictionarySource(SourceAction::Open(0)))
        .unwrap();
    row(&mut app, "Disable for this book");
    back(&mut app);
    row(&mut app, "Reference");
    row(&mut app, "Prefer for this book");
    back(&mut app);
    row(&mut app, "Large window");
    row(&mut app, "Fuzzy search by default");
    row(&mut app, "Dictionary presets");
    row(&mut app, "Local tides");
    row(&mut app, "Apply preset");
    assert_eq!(
        app.lookup_sources("tide")
            .unwrap()
            .iter()
            .map(|result| result.dictionary.as_str())
            .collect::<Vec<_>>(),
        ["Reference"]
    );
    assert!(
        app.dictionary_settings
            .disabled
            .contains(&PathBuf::from("Recently added.ifo"))
    );
    assert!(app.dictionary_settings.large_window && !app.fuzzy_search_enabled());
    assert_eq!(app.current_history_location(), origin);
    app.save().unwrap();
    drop(app);

    let mut app = open();
    app.open_path(&pdf).unwrap();
    assert_eq!(
        app.lookup_sources("tide")
            .unwrap()
            .iter()
            .map(|result| result.dictionary.as_str())
            .collect::<Vec<_>>(),
        ["Coastal"]
    );
    app.open_path(&fixture.book).unwrap();
    assert_eq!(app.current_history_location(), origin);
    assert_eq!(app.book_dictionaries(), [PathBuf::from("Reference.ifo")]);
    assert_eq!(
        app.book_dictionary_exclusions(),
        [PathBuf::from("Coastal.ifo")]
    );
    fs::remove_file(dictionaries.join("Coastal.ifo")).unwrap();
    app.activate(Action::ShowDictionaries).unwrap();
    row(&mut app, "Dictionary presets");
    row(&mut app, "Local tides");
    row(&mut app, "Apply preset");
    assert_eq!(
        app.screen,
        Screen::DictionaryPresets(Page::Unavailable(0, Return::Details))
    );
    assert_eq!(app.items()[1].title, "Coastal");
    back(&mut app);
    assert_eq!(
        app.dictionary_settings.presets.get(0).unwrap().1,
        [PathBuf::from("Coastal.ifo")]
    );
    row(&mut app, "Update from enabled dictionaries");
    row(&mut app, "Cancel");
    assert_eq!(
        app.dictionary_settings.presets.get(0).unwrap().1,
        [PathBuf::from("Coastal.ifo")]
    );
    row(&mut app, "Update from enabled dictionaries");
    row(&mut app, "Update preset");
    row(&mut app, "Rename");
    assert_eq!(app.editor.string(), "Local tides");
    name(&mut app, "Harbour reference");
    assert_eq!(
        app.dictionary_settings.presets.get(0).unwrap(),
        (
            "Harbour reference",
            [PathBuf::from("Reference.ifo")].as_slice()
        )
    );
    drop(app);

    let mut app = open();
    app.activate(Action::ShowDictionaries).unwrap();
    row(&mut app, "Dictionary presets");
    row(&mut app, "Harbour reference");
    row(&mut app, "Update from enabled dictionaries");
    row(&mut app, "Update preset");
    assert!(app.dictionary_settings.presets.get(0).unwrap().1.is_empty());
    row(&mut app, "Delete");
    row(&mut app, "Cancel");
    assert!(!app.dictionary_settings.presets.is_empty());
    row(&mut app, "Delete");
    row(&mut app, "Delete preset");
    assert!(app.dictionary_settings.presets.is_empty());
}

#[test]
fn dictionary_presets_use_the_existing_bounded_preferences_backup() {
    let prior = SourcePreferences::backup_value(br#"{"large_window":true,"disabled":["Old.ifo"]}"#)
        .unwrap();
    assert!(prior.get("presets").is_none());
    assert_eq!(prior["large_window"], true);
    let configured = SourcePreferences::backup_value(br#"{"presets":{"Coastal":["Harbour.ifo","/koreader/data/dict/Reference.ifo"]},"disable_word_forms":true}"#).unwrap();
    assert_eq!(
        configured["presets"]["Coastal"].as_array().unwrap().len(),
        2
    );
    for invalid in [
        r#"{"presets":{" ":["Harbour.ifo"]}}"#,
        r#"{"presets":{"Coastal":["../Harbour.ifo"]}}"#,
        r#"{"presets":{"Coastal":["Harbour.ifo","Harbour.ifo"]}}"#,
    ] {
        assert!(
            SourcePreferences::backup_value(invalid.as_bytes()).is_err(),
            "{invalid}"
        );
    }
    let empty = SourcePreferences::backup_value(br#"{"presets":{"Coastal":[]}}"#).unwrap();
    assert_eq!(empty["presets"]["Coastal"].as_array().unwrap().len(), 0);
}

#[test]
fn dictionary_presets_report_saved_titles_and_fall_back_for_legacy_paths() {
    let fixture = Fixture::new();
    let dictionaries = fixture.root.join("dictionaries");
    fs::create_dir(&dictionaries).unwrap();
    let install_reference = || {
        write_dictionary(
            &dictionaries,
            "shore",
            "sametypesequence=m\n",
            &[("tide", b"The water rises at the quay.")],
        );
        let metadata = dictionaries.join("shore.ifo");
        let text = fs::read_to_string(&metadata).unwrap();
        fs::write(
            metadata,
            text.replace("bookname=shore", "bookname=Atlantic Coastal Dictionary"),
        )
        .unwrap();
    };
    install_reference();
    let mut app = Reader::open(
        &fixture.root,
        &fixture.state,
        vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")],
    )
    .unwrap();
    app.open_path(&fixture.book).unwrap();
    app.save_dictionary_preset_name(Name::Create, "Atlantic reading")
        .unwrap();
    let backed_up =
        SourcePreferences::backup_value(&fs::read(&app.dictionary_preference).unwrap()).unwrap();
    assert_eq!(
        backed_up["presets"]["Atlantic reading"]["labels"][0],
        "Atlantic Coastal Dictionary"
    );

    let mut settings = app.dictionary_settings.clone();
    settings.presets.0.insert(
        "Shared reference".into(),
        Membership::Legacy(vec![
            "/koreader/data/dict/English Reference.ifo".into(),
            "Old field glossary.ifo".into(),
        ]),
    );
    app.save_source_preferences(settings).unwrap();
    fs::remove_file(dictionaries.join("shore.ifo")).unwrap();
    let mut landscape = adopt(&mut app, ReadingViewport::LANDSCAPE, 1);
    row(&mut app, "Atlantic reading");
    row(&mut app, "Apply preset");
    assert_eq!(
        app.screen,
        Screen::DictionaryPresets(Page::Unavailable(0, Return::Details))
    );
    assert_eq!(app.items()[1].title, "Atlantic Coastal Dictionary");
    capture(
        &app,
        &mut landscape,
        "landscape-dictionary-preset-unavailable.png",
        false,
    );
    back(&mut app);
    row(&mut app, "Unavailable dictionaries");
    assert_eq!(app.items()[1].title, "Atlantic Coastal Dictionary");
    back(&mut app);
    back(&mut app);
    row(&mut app, "Shared reference");
    row(&mut app, "Apply preset");
    assert_eq!(
        app.items()
            .into_iter()
            .map(|row| row.title)
            .collect::<Vec<_>>(),
        [
            "Shared reference",
            "English Reference",
            "Old field glossary"
        ],
    );
    back(&mut app);
    back(&mut app);
    install_reference();
    write_dictionary(
        &dictionaries,
        "newly installed",
        "sametypesequence=m\n",
        &[("tide", b"New reference")],
    );
    row(&mut app, "Atlantic reading");
    row(&mut app, "Apply preset");
    assert_eq!(app.screen, Screen::DictionaryPresets(Page::Details(0)));
    assert_eq!(
        app.lookup_sources("tide").unwrap()[0].dictionary,
        "Atlantic Coastal Dictionary"
    );
    assert_eq!(
        app.dictionary_settings.disabled,
        [PathBuf::from("newly installed.ifo")]
    );
    assert!(
        app.items()
            .iter()
            .all(|row| row.title != "Unavailable dictionaries")
    );
    row(&mut app, "Apply preset");
    assert_eq!(
        app.dictionary_settings.presets.get(0).unwrap().1,
        [PathBuf::from("shore.ifo")]
    );
}

#[test]
fn dictionary_presets_cycle_in_name_order_through_reading_gesture_and_reload() {
    use crate::app::Gesture;
    use primitive_ui::Input;

    let fixture = Fixture::new();
    let dictionaries = fixture.root.join("dictionaries");
    fs::create_dir(&dictionaries).unwrap();
    for source in ["Spoken", "Technical"] {
        write_dictionary(
            &dictionaries,
            source,
            "sametypesequence=m\n",
            &[("harbour", b"A shelter.")],
        );
    }
    let pdf = fixture.root.join("Coast.pdf");
    fs::write(&pdf, crate::fixed::tests::document()).unwrap();
    let mut app = fixture.app();
    app.open_path(&fixture.book).unwrap();
    let mut settings = app.dictionary_settings.clone();
    settings.presets = serde_json::from_value(serde_json::json!({
        "Zulu technical": ["Technical.ifo"],
        "Everyday speech": ["Spoken.ifo"],
    }))
    .unwrap();
    settings.large_window = true;
    app.save_source_preferences(settings).unwrap();
    app.show(Screen::GestureBinding(Gesture::TapTopRight));
    row(&mut app, "Cycle through dictionary presets");
    app.show(Screen::DictionaryPresets(Page::Details(1)));
    row(&mut app, "Apply preset");
    assert!(app.dictionary_settings.preset_cycle_after.is_none());
    back(&mut app);
    row(&mut app, "Cycle through dictionary presets");
    assert_eq!(
        app.dictionary_settings.preset_cycle_after.as_deref(),
        Some("Everyday speech")
    );
    assert_eq!(
        app.lookup_sources("harbour").unwrap()[0].dictionary,
        "Spoken"
    );
    drop(app);

    let mut app = fixture.app();
    app.open_path(&pdf).unwrap();
    let location = app.current_history_location();
    let tap_cycle = |app: &mut Reader| app.input(Input::Tap(1244.0, 22.0));
    assert_eq!(tap_cycle(&mut app), super::super::Change::Partial);
    assert_eq!(app.screen, Screen::Fixed);
    assert_eq!(
        app.lookup_sources("harbour").unwrap()[0].dictionary,
        "Technical"
    );
    assert_eq!(app.current_history_location(), location);
    assert!(app.dictionary_settings.large_window);
    fs::remove_file(dictionaries.join("Spoken.ifo")).unwrap();
    assert_eq!(tap_cycle(&mut app), super::super::Change::Partial);
    assert_eq!(
        app.screen,
        Screen::DictionaryPresets(Page::Unavailable(0, Return::Fixed))
    );
    assert_eq!(app.items()[1].title, "Spoken");
    back(&mut app);
    assert_eq!(app.screen, Screen::Fixed);
    assert_eq!(app.current_history_location(), location);
    assert!(app.lookup_sources("harbour").unwrap().is_empty());
    assert_eq!(tap_cycle(&mut app), super::super::Change::Partial);
    assert_eq!(
        app.lookup_sources("harbour").unwrap()[0].dictionary,
        "Technical"
    );
    assert_eq!(app.current_history_location(), location);
}

#[test]
fn dictionary_presets_validate_labels_without_losing_path_only_formats() {
    let bounded = display_label(&format!("  \u{202d}{}\n ", "海".repeat(80))).unwrap();
    assert_eq!(bounded, "海".repeat(48));
    assert!(display_label("\u{202e}\n ").is_none());
    let legacy = br#"{"presets":{"Names":["old.ifo"]},"preset_cycle_after":"Names"}"#;
    assert_eq!(
        SourcePreferences::backup_value(legacy).unwrap()["presets"]["Names"][0],
        "old.ifo"
    );
    for invalid in [
        serde_json::json!({"paths":["safe.ifo"],"labels":[]}),
        serde_json::json!({"paths":["safe.ifo"],"labels":["\u{202e}Title"]}),
        serde_json::json!({"paths":["safe.ifo"],"labels":["x".repeat(MAX_LABEL_BYTES + 1)]}),
    ] {
        assert!(
            SourcePreferences::backup_value(
                serde_json::to_string(&serde_json::json!({"presets":{"Set":invalid}}))
                    .unwrap()
                    .as_bytes()
            )
            .is_err()
        );
    }
    assert!(SourcePreferences::backup_value(br#"{"preset_cycle_after":" invalid"}"#).is_err());
}
