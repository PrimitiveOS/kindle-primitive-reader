use super::*;
use crate::app::profile_panel::{Name, opening_rules::Command, opening_time::Endpoint};
use crate::app::selection_viewport_tests::{adopt, capture, key, press, tap};
use crate::app::tests::{Fixture, tap_contents_item};
use crate::library::{BookPreferences, FolderDirection};
use crate::reading_profiles::{
    Field,
    opening::{MetadataField, TimeWindow},
};
use crate::session::ReadingViewport;
use primitive_ui::Key;
use std::fs;
use std::time::Duration;

fn reader(fixture: &Fixture) -> Reader {
    let fonts = ["fonts/NotoSans-Regular.ttf", "fonts/NotoSans-Bold.ttf"]
        .into_iter()
        .map(crate::test_fixture)
        .collect();
    Reader::open(&fixture.root, &fixture.state, fonts).unwrap()
}

#[test]
fn conditional_closing_epub_requires_a_match_and_keeps_the_outgoing_passage() {
    let f = Fixture::new();
    let ordinary = f.root.join("Ordinary.epub");
    fs::copy(crate::test_fixture("field-notes-epub2.epub"), &ordinary).unwrap();
    let sidecar = f.root.join("Field Notes.sdr");
    fs::create_dir(&sidecar).unwrap();
    fs::copy(
        crate::test_fixture("koreader-custom-metadata.lua"),
        sidecar.join("custom_metadata.lua"),
    )
    .unwrap();
    let original = fs::read(&f.book).unwrap();
    let mut app = reader(&f);
    app.open_path(&f.book).unwrap();
    let out = app.open_book_id().unwrap();
    let basis = app.current_text_preferences().unwrap();
    app.apply_text_profile(BookPreferences {
        logical_font_size: Some(35.0),
        ..basis.clone()
    })
    .unwrap();
    close_profile(
        &mut app,
        "Coastal large print",
        vec![Field::FontSize],
        OnClose::Ask,
    );
    app.apply_text_profile(basis.clone()).unwrap();
    app.follow_reflow_page(1, 0).unwrap();
    let passage = app.current_position().unwrap().anchor;
    app.library.create_collection("Sea surveys").unwrap();
    app.library
        .set_collection_membership("Sea surveys", out, true)
        .unwrap();
    let event = Event::Closing;
    let mut canvas = adopt(&mut app, ReadingViewport::PORTRAIT, 1);
    app.show(event.screen(0, Page::Overview));
    tap_contents_item(&mut app, "Closing conditions");
    assert!(
        !app.items()
            .iter()
            .any(|item| item.title == "Only for new books")
    );
    tap_contents_item(&mut app, "Book metadata contains");
    tap_contents_item(&mut app, "Title");
    assert_eq!(
        app.screen,
        Screen::ProfileName(Name::ClosingMetadata(0, MetadataField::Title))
    );
    let current = app.current_opening_book_button().unwrap();
    tap(&mut app, current);
    assert_eq!(app.editor.string(), "Coastal Ledger");
    let save = key(&app, Key::Go, "Save");
    tap(&mut app, save);
    let back = app.navigation_layout().back();
    tap(&mut app, back);
    tap_contents_item(&mut app, "File path contains");
    app.editor.text = "other location, %H%/Field Notes.epub".chars().collect();
    app.editor.caret = app.editor.text.len();
    let save = key(&app, Key::Go, "Save");
    tap(&mut app, save);
    tap_contents_item(&mut app, "Book is in a collection");
    tap_contents_item(&mut app, "Sea surveys");
    tap(&mut app, back);
    assert_eq!(app.screen, event.screen(0, Page::Conditions));
    assert!(app.profiles[0].opening_conditions().is_none());
    capture(
        &app,
        &mut canvas,
        "profile-closing-conditions-portrait.png",
        false,
    );

    app.show(Screen::Book);
    app.open_path(&ordinary).unwrap();
    assert_eq!(app.open_book_id(), Some(out));
    assert_eq!(app.closing_profile.as_ref().unwrap().choices.len(), 1);
    assert_eq!(app.library.state(out).unwrap().preferences, basis);
    // Close facts are sampled once and the exact choice survives UI delays.
    app.library
        .set_collection_membership("Sea surveys", out, false)
        .unwrap();
    fs::remove_file(sidecar.join("custom_metadata.lua")).unwrap();
    assert_eq!(app.matching_closing_profiles(None).len(), 1); // Expanded path alone.
    let mut profiles = app.profiles.clone();
    let mut options = profiles[0].closing_options().unwrap().clone();
    options.conditions.path = Some("different folder".into());
    profiles[0].set_closing_options(options);
    app.commit_profiles(profiles).unwrap();
    assert!(app.matching_closing_profiles(None).is_empty());
    let confirm = app
        .closing_profile_sheet()
        .unwrap()
        .button(SheetAction::Confirm);
    tap(&mut app, confirm);
    let second = app.open_book_id().unwrap();
    assert_ne!(second, out);
    let state = app.library.state(out).unwrap();
    assert_eq!(state.preferences.logical_font_size, Some(35.0));
    assert_eq!(state.position.as_ref().unwrap().anchor, passage);
    assert_eq!(fs::read(&f.book).unwrap(), original);
    let ordinary_before = app.current_text_preferences().unwrap();
    assert!(app.matching_closing_profiles(None).is_empty());
    app.activate(Action::Show(Screen::Library)).unwrap();
    assert!(app.closing_profile.is_none());
    assert_eq!(app.screen, Screen::Library);
    app.open_path(&ordinary).unwrap();
    assert_eq!(app.current_text_preferences().unwrap(), ordinary_before);
}

#[test]
fn conditional_closing_pdf_uses_the_accepted_turn_and_preserves_the_physical_page() {
    let f = Fixture::new();
    let pdf = f.root.join("Harbor scan.pdf");
    fs::write(
        &pdf,
        crate::fixed_session::tests::striped_pdf_sized_bytes(4, 612, 820),
    )
    .unwrap();
    let original = fs::read(&pdf).unwrap();
    let mut app = reader(&f);
    app.open_path(&pdf).unwrap();
    let id = app.open_book_id().unwrap();
    app.activate(Action::FixedPage(2)).unwrap();
    app.activate(Action::ScanGamma(21)).unwrap();
    close_profile(
        &mut app,
        "Landscape scan",
        vec![Field::ScanContrast],
        OnClose::Apply,
    );
    app.activate(Action::ScanGamma(13)).unwrap();
    let event = Event::Closing;
    let mut canvas = adopt(&mut app, ReadingViewport::LANDSCAPE, 1);
    app.show(event.screen(0, Page::Overview));
    tap_contents_item(&mut app, "Closing conditions");
    tap_contents_item(&mut app, "Closing screen orientation");
    assert_eq!(app.items().len(), 4);
    tap_contents_item(&mut app, "Landscape right");
    capture(
        &app,
        &mut canvas,
        "profile-closing-orientation-landscape.png",
        true,
    );
    let accepted = app.accepted_viewport.unwrap();
    assert_eq!(
        accepted.turn,
        primitive_app::viewport::LogicalTurn::Clockwise
    );
    assert_eq!(
        app.reading_orientation(),
        crate::app::ReadingOrientation::FollowLauncher
    );
    assert_eq!(app.matching_closing_profiles(None).len(), 1);
    app.show(Screen::Fixed);
    app.activate(Action::Show(Screen::Library)).unwrap();
    assert_eq!(
        app.library
            .state(id)
            .unwrap()
            .fixed_preferences
            .scan_gamma_tenths,
        Some(21)
    );
    assert_eq!(
        app.library
            .state(id)
            .unwrap()
            .position
            .as_ref()
            .unwrap()
            .page,
        2
    );

    app.open_path(&pdf).unwrap();
    adopt(&mut app, ReadingViewport::PORTRAIT, 2);
    app.activate(Action::ScanGamma(17)).unwrap();
    assert!(app.matching_closing_profiles(None).is_empty());
    app.activate(Action::Show(Screen::Library)).unwrap();
    assert_eq!(
        app.library
            .state(id)
            .unwrap()
            .fixed_preferences
            .scan_gamma_tenths,
        Some(17)
    );
    assert_eq!(
        app.library
            .state(id)
            .unwrap()
            .position
            .as_ref()
            .unwrap()
            .page,
        2
    );
    assert_eq!(fs::read(&pdf).unwrap(), original);
}

#[test]
fn conditional_closing_clock_and_backup_keep_opening_preferences_independent() {
    let f = Fixture::new();
    let mut app = reader(&f);
    app.open_path(&f.book).unwrap();
    close_profile(
        &mut app,
        "Evening reading",
        vec![Field::FontSize],
        OnClose::Apply,
    );
    app.save_profile_time(Event::Opening, 0, Some(TimeWindow { start: 3, end: 4 }))
        .unwrap();
    let event = Event::Closing;
    let mut canvas = adopt(&mut app, ReadingViewport::LANDSCAPE, 1);
    app.show(event.screen(0, Page::Overview));
    tap_contents_item(&mut app, "Only within time window");
    let save = app
        .navigation_layout()
        .centered(Rect::new(662, 970, 472, 100));
    for (label, endpoint, minute) in [
        ("Start time", Endpoint::Start, 1320),
        ("End time", Endpoint::End, 390),
    ] {
        tap_contents_item(&mut app, label);
        assert_eq!(app.screen, event.screen(0, Page::TimeEdit(endpoint)));
        app.profile_opening_time_draft = minute;
        tap(&mut app, save);
    }
    assert_eq!(
        event.time(&app.profiles[0]),
        Some(TimeWindow {
            start: 1320,
            end: 390
        })
    );
    assert_eq!(
        Event::Opening.time(&app.profiles[0]),
        Some(TimeWindow { start: 3, end: 4 })
    );
    for (minute, applies) in [
        (1319, false),
        (1320, true),
        (1439, true),
        (0, true),
        (390, true),
        (391, false),
    ] {
        assert_eq!(
            !app.matching_closing_profiles(Some(minute)).is_empty(),
            applies
        );
    }
    assert!(app.matching_closing_profiles(None).is_empty());
    capture(
        &app,
        &mut canvas,
        "profile-closing-time-landscape.png",
        false,
    );
    assert!(app.change_profile_rule(event, 0, Command::NewOnly).is_err());
    let saved = serde_json::to_value(&app.profiles).unwrap();
    assert_eq!(
        crate::reading_profiles::decode(saved.clone()).unwrap(),
        app.profiles
    );
    for invalid in [
        serde_json::json!({"conditions":{"new_only":true}}),
        serde_json::json!({"conditions":{"orientations":["follow_launcher"]}}),
        serde_json::json!({"time":{"start":1440,"end":1}}),
    ] {
        let mut backup = saved.clone();
        backup[0]["closing"] = invalid;
        assert!(crate::reading_profiles::decode(backup).is_err());
    }
    let mut legacy = saved.clone();
    legacy[0].as_object_mut().unwrap().remove("closing");
    assert_eq!(
        crate::reading_profiles::decode(legacy).unwrap()[0].apply_on_close(),
        Some(OnClose::Apply)
    );
    tap_contents_item(&mut app, "Clear time window");
    assert!(app.profiles[0].closing_options().is_none());
    assert!(app.profiles[0].opening_time().is_some());
}

fn close_profile(app: &mut Reader, name: &str, fields: Vec<Field>, policy: OnClose) {
    app.save_reading_profile(name).unwrap();
    let index = app.profiles.len() - 1;
    let mut profiles = app.profiles.clone();
    profiles[index].set_fields(fields).unwrap();
    profiles[index].set_apply_on_close(Some(policy));
    app.commit_profiles(profiles).unwrap();
}

#[test]
fn closing_profile_asks_at_the_epub_anchor_and_persists_only_when_applied() {
    for (viewport, suffix) in [
        (ReadingViewport::PORTRAIT, "portrait"),
        (ReadingViewport::LANDSCAPE, "landscape"),
    ] {
        let f = Fixture::new();
        let folder = f.root.join("Field reports");
        fs::create_dir(&folder).unwrap();
        let book = folder.join("Field Notes.epub");
        fs::copy(&f.book, &book).unwrap();
        let original = fs::read(&book).unwrap();
        let mut app = reader(&f);
        app.open_path(&book).unwrap();
        let basis = app.current_text_preferences().unwrap();
        app.apply_text_profile(BookPreferences {
            logical_font_size: Some(35.0),
            ..basis.clone()
        })
        .unwrap();
        app.save_reading_profile("Large print for next time")
            .unwrap();
        let mut profiles = app.profiles.clone();
        profiles[0].set_fields(vec![Field::FontSize]).unwrap();
        app.commit_profiles(profiles).unwrap();
        app.apply_text_profile(basis.clone()).unwrap();
        app.follow_reflow_page(1, 0).unwrap();
        let id = app.open_book_id().unwrap();
        let anchor = app.current_position().unwrap().anchor;
        let mut canvas = adopt(&mut app, viewport, 1);
        app.show(Screen::ReadingProfile(0));
        tap_contents_item(&mut app, "Apply on book closing");
        tap_contents_item(&mut app, "Ask to execute");
        assert_eq!(app.profiles[0].apply_on_close(), Some(OnClose::Ask));
        assert!(!app.profiles[0].applies_on_open());
        capture(
            &app,
            &mut canvas,
            &format!("profile-book-closing-settings-{suffix}.png"),
            false,
        );

        app.show(Screen::Book);
        app.current_book_folder().unwrap();
        assert_eq!(app.open_book_id(), Some(id));
        assert_eq!(app.library.state(id).unwrap().preferences, basis);
        assert_eq!(app.current_position().unwrap().anchor, anchor);
        let apply = app
            .closing_profile_sheet()
            .unwrap()
            .button(SheetAction::Confirm);
        press(&app, &mut canvas, apply, false);
        capture(
            &app,
            &mut canvas,
            &format!("profile-book-closing-ask-{suffix}.png"),
            suffix == "landscape",
        );
        assert_eq!(app.input(Input::PageForward), Change::None);
        assert_eq!(app.input(Input::PageBack), Change::Partial);
        assert_eq!(app.screen, Screen::Library);
        assert!(app.open_book_id().is_none());
        assert_eq!(
            app.browser.directory(),
            std::path::Path::new("Field reports")
        );
        assert_eq!(app.library.state(id).unwrap().preferences, basis);
        app.open_path(&book).unwrap();
        assert_eq!(app.current_position().unwrap().anchor, anchor);

        // Done still exits even if the user declines an optional profile.
        let done = app.reading_viewport().done();
        tap(&mut app, done);
        assert_eq!(app.input(Input::Tap(20.0, 20.0)), Change::Exit);
        assert_eq!(app.library.state(id).unwrap().preferences, basis);
        tap(&mut app, done);
        let confirm = app
            .closing_profile_sheet()
            .unwrap()
            .button(SheetAction::Confirm);
        assert_eq!(
            app.input(Input::Tap((confirm.x + 20) as f32, (confirm.y + 20) as f32)),
            Change::Exit
        );
        assert_eq!(
            app.library.state(id).unwrap().preferences.logical_font_size,
            Some(35.0)
        );
        assert_eq!(
            app.library
                .state(id)
                .unwrap()
                .position
                .as_ref()
                .unwrap()
                .anchor,
            anchor
        );
        assert_eq!(fs::read(&book).unwrap(), original);

        let mut reopened = reader(&f);
        assert_eq!(reopened.profiles[0].apply_on_close(), Some(OnClose::Ask));
        reopened.open_path(&book).unwrap();
        assert_eq!(reopened.current_position().unwrap().anchor, anchor);
        assert_eq!(
            reopened
                .current_text_preferences()
                .unwrap()
                .logical_font_size,
            Some(35.0)
        );
    }
}

#[test]
fn closing_profile_switches_after_decisions_and_targets_only_the_outgoing_pdf() {
    let f = Fixture::new();
    let old = f.root.join("Harbor survey.pdf");
    let new = f.root.join("Island chart.pdf");
    let bytes = crate::fixed_session::tests::striped_pdf_sized_bytes(4, 612, 820);
    fs::write(&old, &bytes).unwrap();
    fs::write(
        &new,
        crate::fixed_session::tests::striped_pdf_sized_bytes(3, 820, 600),
    )
    .unwrap();
    let mut app = reader(&f);
    app.open_path(&old).unwrap();
    let before = app.open_book_id().unwrap();
    app.activate(Action::FixedPage(2)).unwrap();
    app.activate(Action::ScanGamma(18)).unwrap();
    close_profile(
        &mut app,
        "Alpha contrast",
        vec![Field::ScanContrast],
        OnClose::Apply,
    );
    app.activate(Action::ScanGamma(24)).unwrap();
    close_profile(
        &mut app,
        "Zulu contrast",
        vec![Field::ScanContrast],
        OnClose::Ask,
    );
    app.activate(Action::ScanGamma(32)).unwrap();
    app.open_path(&new).unwrap();
    assert_eq!(app.open_book_id(), Some(before));
    assert_eq!(
        app.library
            .state(before)
            .unwrap()
            .fixed_preferences
            .scan_gamma_tenths,
        Some(32)
    );
    let confirm = app
        .closing_profile_sheet()
        .unwrap()
        .button(SheetAction::Confirm);
    tap(&mut app, confirm);
    assert_ne!(app.open_book_id(), Some(before));
    assert_eq!(app.current_position().unwrap().page, 0);
    assert_eq!(
        app.fixed
            .as_ref()
            .unwrap()
            .reading
            .preferences()
            .scan_gamma_tenths,
        None
    );
    assert_eq!(
        app.library
            .state(before)
            .unwrap()
            .fixed_preferences
            .scan_gamma_tenths,
        Some(24)
    );
    assert_eq!(
        app.library
            .state(before)
            .unwrap()
            .position
            .as_ref()
            .unwrap()
            .page,
        2
    );
    assert_eq!(fs::read(&old).unwrap(), bytes);
    assert!(app.closing_profile.is_none());

    // Returning storage discards an outstanding continuation and approval.
    let second = app.open_book_id();
    app.activate(Action::AutoTurn(Some(Duration::from_secs(30))))
        .unwrap();
    assert!(app.auto_turn.deadline().is_some());
    app.open_path(&old).unwrap();
    assert!(app.closing_profile.is_some());
    assert!(app.auto_turn.deadline().is_none());
    app.input(Input::Sleep);
    let confirm = app
        .closing_profile_sheet()
        .unwrap()
        .button(SheetAction::Confirm);
    assert_eq!(
        app.input(Input::Tap((confirm.x + 20) as f32, (confirm.y + 20) as f32)),
        Change::None
    );
    assert_eq!(app.open_book_id(), second);
    app.input(Input::Wake);
    assert!(app.closing_profile.is_some());
    app.storage_returned().unwrap();
    assert!(app.closing_profile.is_none());
    assert_eq!(app.open_book_id(), second);
    assert!(app.closing_profile_input(Input::PageBack).is_none());
    assert!(app.auto_turn.deadline().is_some());
}

#[test]
fn closing_profile_keeps_outgoing_book_until_an_adjacent_pdf_unlocks() {
    let f = Fixture::new();
    let folder = f.root.join("Field reports");
    fs::create_dir(&folder).unwrap();
    let book = folder.join("Volume 1.epub");
    let locked = folder.join("Volume 2.pdf");
    fs::copy(&f.book, &book).unwrap();
    fs::copy(crate::test_fixture("password-required.pdf"), &locked).unwrap();
    let mut app = reader(&f);
    app.open_path(&book).unwrap();
    let basis = app.current_text_preferences().unwrap();
    app.apply_text_profile(BookPreferences {
        logical_font_size: Some(35.0),
        ..basis.clone()
    })
    .unwrap();
    close_profile(&mut app, "Large print", vec![Field::FontSize], OnClose::Ask);
    app.apply_text_profile(basis.clone()).unwrap();
    app.follow_reflow_page(1, 0).unwrap();
    let old = app.open_book_id().unwrap();
    let passage = app.current_position().unwrap();
    app.open_path(&book).unwrap();
    assert!(app.closing_profile.is_none());
    assert_eq!(app.current_position(), Some(passage.clone()));

    let next = |app: &mut Reader| {
        app.activate(Action::OpenFolderFile(FolderDirection::Next))
            .unwrap();
        let confirm = app
            .closing_profile_sheet()
            .unwrap()
            .button(SheetAction::Confirm);
        tap(app, confirm);
        assert_eq!(app.screen, Screen::PdfPassword);
        assert_eq!(app.open_book_id(), Some(old));
        assert_eq!(app.library.state(old).unwrap().preferences, basis);
    };
    let submit = |app: &mut Reader, password: &str| {
        app.editor.text = password.chars().collect();
        app.editor.caret = app.editor.text.len();
        tap(app, key(app, Key::Go, "Open"));
    };

    next(&mut app);
    submit(&mut app, "incorrect");
    assert_eq!(app.screen, Screen::PdfPassword);
    assert_eq!(app.input(Input::PageBack), Change::Partial);
    assert_eq!(app.screen, Screen::Book);
    assert_eq!(app.open_book_id(), Some(old));
    assert_eq!(app.current_position(), Some(passage.clone()));
    assert!(app.closing_before_switch.is_none());
    assert_eq!(app.library.state(old).unwrap().preferences, basis);

    let broken = f.root.join("Broken.epub");
    fs::write(&broken, b"This is not a publication.").unwrap();
    app.open_path(&broken).unwrap();
    let confirm = app
        .closing_profile_sheet()
        .unwrap()
        .button(SheetAction::Confirm);
    tap(&mut app, confirm);
    assert!(app.notice.is_some());
    assert_eq!(app.current_position(), Some(passage.clone()));
    assert!(app.closing_before_switch.is_none());
    assert_eq!(app.library.state(old).unwrap().preferences, basis);

    next(&mut app);
    app.storage_returned().unwrap();
    assert_eq!(app.screen, Screen::Book);
    assert_eq!(app.current_position(), Some(passage.clone()));
    assert!(app.locked.is_none() && app.closing_before_switch.is_none());
    assert_eq!(app.library.state(old).unwrap().preferences, basis);

    next(&mut app);
    submit(&mut app, "reader-test");
    assert_eq!(app.screen, Screen::Fixed);
    assert_ne!(app.open_book_id(), Some(old));
    assert!(app.fixed.as_ref().unwrap().reading.is_encrypted_pdf());
    assert!(app.closing_before_switch.is_none());
    let old_state = app.library.state(old).unwrap();
    assert_eq!(old_state.preferences.logical_font_size, Some(35.0));
    assert_eq!(old_state.position.as_ref().unwrap().anchor, passage.anchor);
}

#[test]
fn closing_profile_with_an_unavailable_font_leaves_the_saved_passage_and_allows_close() {
    let f = Fixture::new();
    let mut app = reader(&f);
    app.open_path(&f.book).unwrap();
    app.follow_reflow_page(1, 0).unwrap();
    let id = app.open_book_id().unwrap();
    let before = app.current_position();
    let basis = app.library.state(id).unwrap().preferences.clone();
    close_profile(
        &mut app,
        "Unavailable font",
        vec![Field::Font],
        OnClose::Apply,
    );
    let mut profiles = app.profiles.clone();
    let Profile::Text { preferences, .. } = &mut profiles[0] else {
        panic!()
    };
    preferences.font_file = Some("Missing-book-font.ttf".into());
    app.commit_profiles(profiles).unwrap();
    app.activate(Action::Show(Screen::Library)).unwrap();
    assert_eq!(app.screen, Screen::Library);
    assert!(app.open_book_id().is_none());
    assert!(
        app.notice
            .as_deref()
            .unwrap()
            .contains("Closing profile could not be applied")
    );
    assert_eq!(app.library.state(id).unwrap().position, before);
    assert_eq!(app.library.state(id).unwrap().preferences, basis);
}
