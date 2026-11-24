use super::*;
use crate::app::selection_viewport_tests::{adopt, canvas, capture, key, press, tap};
use crate::app::tests::Fixture;
use crate::session::ReadingViewport;
use primitive_ui::Key;

fn font_paths() -> Vec<PathBuf> {
    vec![
        crate::test_fixture("fonts/NotoSans-Regular.ttf"),
        crate::test_fixture("fonts/NotoSans-Bold.ttf"),
    ]
}

fn tap_title(app: &mut Reader, title: &str) {
    let geometry = app.list_geometry();
    let ordinal = app
        .items()
        .iter()
        .position(|item| item.title == title)
        .unwrap();
    assert_eq!(ordinal / geometry.rows, app.list_page);
    let row = geometry.row(ordinal % geometry.rows);
    let x = (row.right() - 32) as f32;
    let y = (row.y + row.h / 2) as f32;
    assert!(x > WIDTH as f32);
    assert_eq!(app.input(Input::Tap(x, y)), Change::Partial);
}

fn type_and_save(app: &mut Reader, text: &str) {
    for ch in text.chars() {
        let key = key(
            app,
            if ch == ' ' { Key::Space } else { Key::Char(ch) },
            "Save",
        );
        tap(app, key);
    }
    let save = key(app, Key::Go, "Save");
    assert!(save.x > WIDTH as i32);
    tap(app, save);
}

#[test]
fn custom_contents_viewport_epub_bookmark_editor_and_hierarchy_keep_canonical_locations() {
    let fixture = Fixture::new();
    fs::copy(crate::test_fixture("long-chapter.epub"), &fixture.book).unwrap();
    let mut app = Reader::open(&fixture.root, &fixture.state, font_paths()).unwrap();
    app.open_path(&fixture.book).unwrap();
    let origin = app.current_history_location();
    let reading = &app.active.as_ref().unwrap().reading;
    let marks = (1..=14)
        .map(|number| {
            let anchor = reading
                .probe
                .fragment(&format!("station-{number:03}"))
                .unwrap();
            Bookmark {
                chapter_title: None,
                label: format!("Station {number:02}"),
                position: ReadingPosition {
                    chapter: 0,
                    page: reading.probe.page_for(&anchor).unwrap(),
                    anchor: Some(anchor),
                },
                created_at: None,
            }
        })
        .collect::<Vec<_>>();
    let source = marks.last().unwrap().position.clone();
    let id = app.open_book_id().unwrap();
    app.library.state_mut(id).unwrap().bookmarks = marks;
    app.show(Screen::CustomContents);
    let portrait_canvas = &mut canvas(&app, ReadingViewport::PORTRAIT);
    let before = primitive_app::frame(&app, portrait_canvas, false);
    let _ = adopt(&mut app, ReadingViewport::LANDSCAPE, 2);
    let layout = app.navigation_layout();
    let restored = &mut adopt(&mut app, ReadingViewport::PORTRAIT, 3);
    assert!(primitive_app::frame(&app, restored, false) == before);
    let mut landscape = adopt(&mut app, ReadingViewport::LANDSCAPE, 4);

    tap_title(&mut app, "Add from bookmark");
    assert_eq!(app.screen, Screen::CustomBookmarks);
    let next = layout.next();
    assert!(next.x > WIDTH as i32);
    press(&app, &mut landscape, next, false);
    capture(
        &app,
        &mut landscape,
        "landscape-custom-chapter-bookmarks.png",
        false,
    );
    tap(&mut app, next);
    let page = 13 / app.list_geometry().rows;
    while app.list_page < page {
        assert_eq!(app.input(Input::PageForward), Change::Partial);
    }
    tap_title(&mut app, "Station 14");
    assert_eq!(app.screen, Screen::CustomTitle);
    assert_eq!(
        app.chapter_draft.as_ref().unwrap().position.anchor,
        source.anchor
    );
    let cancel = layout.back();
    assert!(cancel.x > WIDTH as i32);
    press(&app, &mut landscape, cancel, false);
    capture(
        &app,
        &mut landscape,
        "landscape-custom-chapter-title.png",
        false,
    );
    let draft = app.editor.string();
    let _ = adopt(&mut app, ReadingViewport::PORTRAIT, 5);
    assert_eq!(app.screen, Screen::CustomTitle);
    assert_eq!(app.editor.string(), draft);
    assert_eq!(app.chapter_draft.as_ref().unwrap().position, source);
    landscape = adopt(&mut app, ReadingViewport::LANDSCAPE, 6);
    type_and_save(&mut app, " overlook");
    assert_eq!(app.screen, Screen::CustomContents);
    assert_eq!(app.custom_entry(0).unwrap().position, source);
    tap_title(&mut app, "Use custom contents");
    assert_eq!(app.screen, Screen::Contents);
    let outline = app.contents_row_rect(
        app.items()
            .iter()
            .position(|row| row.title == "Station 14 overlook")
            .unwrap(),
    );
    tap(&mut app, outline);
    let reading = &app.active.as_ref().unwrap().reading;
    assert_eq!(
        reading.page,
        reading
            .probe
            .page_for(source.anchor.as_ref().unwrap())
            .unwrap()
    );
    app.activate(Action::ReturnToLocation).unwrap();
    assert_eq!(app.current_history_location(), origin);

    app.show(Screen::ChapterNavigation);
    let target = chapter_navigation::LevelSetting::ToggleLevel(0);
    let original_level = app.chapter_level_checked(&target);
    tap_title(&mut app, "Contents level 1");
    assert_ne!(app.chapter_level_checked(&target), original_level);
    assert_eq!(app.current_history_location(), origin);
    app.show(Screen::ContentsSettings);
    let prior = app.controls.contents_density.rows();
    tap_title(&mut app, "Rows per page");
    let up = layout.centered(list_density::INCREASE);
    let apply = layout.centered(list_density::APPLY);
    press(&app, &mut landscape, up, false);
    tap(&mut app, up);
    capture(
        &app,
        &mut landscape,
        "landscape-contents-density.png",
        false,
    );
    tap(&mut app, apply);
    assert_eq!(app.controls.contents_density.rows(), prior + 1);
    tap(&mut app, layout.back());
    assert_eq!(app.screen, Screen::BookTools);
    assert_eq!(app.current_history_location(), origin);
}

#[test]
fn custom_contents_viewport_pdf_confirm_cancel_and_remove_preserve_the_saved_page_and_file() {
    let fixture = Fixture::new();
    let path = fixture.root.join("Survey sheets.pdf");
    let document = crate::fixed_session::tests::striped_pdf_bytes(8);
    fs::write(&path, &document).unwrap();
    let mut app = Reader::open(&fixture.root, &fixture.state, font_paths()).unwrap();
    app.open_path(&path).unwrap();
    app.fixed.as_mut().unwrap().reading.jump(5).unwrap();
    app.activate(Action::ToggleBookmark).unwrap();
    app.fixed.as_mut().unwrap().reading.jump(0).unwrap();
    let source_position = app.current_position();
    let id = app.open_book_id().unwrap();
    app.library.state_mut(id).unwrap().bookmarks[0].label = "Survey sheet".into();
    let mut landscape = adopt(&mut app, ReadingViewport::LANDSCAPE, 2);
    assert_eq!(app.current_position(), source_position);
    let origin = app.current_history_location();
    let layout = app.navigation_layout();
    app.show(Screen::CustomContents);
    tap_title(&mut app, "Add from bookmark");
    tap_title(&mut app, "Survey sheet");
    type_and_save(&mut app, " five");
    assert_eq!(
        app.custom_entry(0).unwrap().position,
        fixed_page_position(5)
    );
    tap_title(&mut app, "Edit chapters");
    tap_title(&mut app, "Survey sheet five");
    tap(&mut app, REMOVE);
    let cancel = layout.centered(CANCEL);
    let remove = layout.centered(DELETE);
    let panel = layout.centered(CONFIRM);
    assert!(panel.right() <= layout.width && panel.bottom() <= layout.height);
    press(&app, &mut landscape, remove, true);
    capture(
        &app,
        &mut landscape,
        "landscape-custom-chapter-remove-dark.png",
        true,
    );
    tap(&mut app, cancel);
    assert!(
        app.chapter_state()
            .unwrap()
            .custom_contents
            .find(&fixed_page_position(5))
            .is_some()
    );
    tap(&mut app, REMOVE);
    assert_eq!(app.input(Input::Tap(1630.0, 1010.0)), Change::Partial);
    assert!(!app.chapter_draft.as_ref().unwrap().confirm_remove);
    tap(&mut app, REMOVE);
    tap(&mut app, remove);
    assert_eq!(app.screen, Screen::CustomEdits);
    assert!(
        app.chapter_state()
            .unwrap()
            .custom_contents
            .entries()
            .is_empty()
    );
    assert_eq!(
        app.chapter_state().unwrap().bookmarks[0].position,
        fixed_page_position(5)
    );
    assert_eq!(app.current_history_location(), origin);
    app.save().unwrap();
    let mut reopened = Reader::open(&fixture.root, &fixture.state, font_paths()).unwrap();
    reopened.open_path(&path).unwrap();
    assert_eq!(
        reopened.chapter_state().unwrap().bookmarks[0].position,
        fixed_page_position(5)
    );
    assert!(
        reopened
            .chapter_state()
            .unwrap()
            .custom_contents
            .entries()
            .is_empty()
    );
    assert_eq!(fs::read(path).unwrap(), document);
}

#[test]
fn custom_contents_viewport_generated_outline_pages_and_opens_its_source_heading() {
    use crate::app::tests::write_outline_fixture;

    let fixture = Fixture::new();
    write_outline_fixture(&fixture.book, None, None);
    let mut app = Reader::open(&fixture.root, &fixture.state, font_paths()).unwrap();
    app.open_path(&fixture.book).unwrap();
    let mut landscape = adopt(&mut app, ReadingViewport::LANDSCAPE, 2);
    let origin = app.current_history_location();
    app.show(Screen::Contents);
    let generated = app.contents_row_rect(0);
    tap(&mut app, generated);
    assert_eq!(app.screen, Screen::GeneratedContents);
    while let Some(at) = app.outline.as_ref().unwrap().deadline {
        assert_ne!(app.tick(at), Change::None);
    }
    assert_eq!(app.items().len(), 15);
    let layout = app.navigation_layout();
    let next = layout.next();
    press(&app, &mut landscape, next, false);
    tap(&mut app, next);
    assert!(app.list_page > 0);
    capture(
        &app,
        &mut landscape,
        "landscape-generated-contents.png",
        false,
    );
    let entries = &app.outline.as_ref().unwrap().contents.entries;
    let OutlineTarget::Heading(0, anchor) = entries[12].target.clone() else {
        panic!("Expected a canonical publisher heading");
    };
    let title = app.items()[12].title.clone();
    tap_title(&mut app, &title);
    let reading = &app.active.as_ref().unwrap().reading;
    assert_eq!(reading.probe.page_for(&anchor), Some(reading.page));
    app.activate(Action::ReturnToLocation).unwrap();
    assert_eq!(app.current_history_location(), origin);
}
