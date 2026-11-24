use super::*;
use crate::app::selection_viewport_tests::{adopt, capture, key, press, tap};
use crate::app::tests::{Fixture, tap_contents_item};
use crate::library::{BookPreferences, FolderDirection};
use crate::reading_profiles::Field;
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
