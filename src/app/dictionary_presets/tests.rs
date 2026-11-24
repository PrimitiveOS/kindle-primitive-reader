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
        app.notice.as_deref(),
        Some("Dictionary preset applied · 1 unavailable")
    );
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
