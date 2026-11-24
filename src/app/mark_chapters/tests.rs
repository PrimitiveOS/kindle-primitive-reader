use super::*;
use crate::app::chapter_navigation::LevelSetting;
use crate::app::selection_viewport_tests::{adopt, capture};
use crate::app::tests::Fixture;
use crate::app::{Action, Screen};
use crate::library::BookState;
use crate::session::ReadingViewport;
use primitive_app::App;
use std::{fs, path::Path};

fn open(fixture: &Fixture, path: &Path) -> Reader {
    let fonts = ["fonts/NotoSans-Regular.ttf", "fonts/NotoSans-Bold.ttf"].map(crate::test_fixture);
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts.to_vec()).unwrap();
    app.open_path(path).unwrap();
    app
}

fn state(app: &Reader) -> &BookState {
    app.library.state(app.open_book_id().unwrap()).unwrap()
}

fn record_quote(app: &mut Reader, text: &str) {
    let probe = &app.active.as_ref().unwrap().reading.probe;
    let start = probe.matches(text).remove(0);
    let mut end = start.clone();
    end.offset += text.len();
    app.selection = probe.select_range(&start, &end);
    app.save_highlight_in_place(None).unwrap();
}

#[test]
fn mark_chapters_snapshot_same_page_source_and_preceding_spine_titles() {
    let fixture = Fixture::new();
    let path = fixture.root.join("Harbor Survey.epub");
    let bytes = fs::read(crate::test_fixture("alternative-contents.epub")).unwrap();
    fs::write(&path, &bytes).unwrap();
    let mut app = open(&fixture, &path);
    let id = app.open_book_id().unwrap();
    app.library.set_alternative_contents(id, true).unwrap();
    app.sync_alternative_contents();
    for _ in 0..4 {
        let Some(deadline) = app.alternative_contents_deadline() else {
            break;
        };
        app.poll_alternative_contents(deadline);
    }
    assert_eq!(app.alternative_entries(id).unwrap().len(), 4);

    let mut wide = adopt(&mut app, ReadingViewport::LANDSCAPE, 1);
    let reading = &mut app.active.as_mut().unwrap().reading;
    reading.jump(1, None).unwrap();
    let east = reading.probe.matches("An east quay field note.").remove(0);
    let west = reading.probe.matches("A west quay field note.").remove(0);
    assert_eq!(reading.probe.page_for(&east), reading.probe.page_for(&west));
    assert_eq!(app.bound_footer_chapter_title(), Some("Harbor Survey"));

    app.activate(Action::ToggleBookmark).unwrap();
    assert_eq!(
        state(&app).bookmarks[0].chapter_title.as_deref(),
        Some("Harbor Survey")
    );
    record_quote(&mut app, "An east quay field note.");
    record_quote(&mut app, "A west quay field note.");
    assert_eq!(
        state(&app).highlights[0].chapter_title.as_deref(),
        Some("East softhyphen quay")
    );
    assert_eq!(
        state(&app).highlights[1].chapter_title.as_deref(),
        Some("Harbor Survey"),
        "Stock retains its initial page title at the end of the outline"
    );

    app.set_chapter_level(LevelSetting::ToggleLevel(2)).unwrap();
    app.set_chapter_level(LevelSetting::BindTitle).unwrap();
    record_quote(&mut app, "east quay field note.");
    assert!(
        state(&app).highlights[2].chapter_title.is_none(),
        "The depth placeholder has no title"
    );
    app.set_chapter_level(LevelSetting::ToggleLevel(1)).unwrap();
    record_quote(&mut app, "An east quay");
    assert_eq!(
        state(&app).highlights[3].chapter_title.as_deref(),
        Some("Harbor Survey")
    );
    app.set_chapter_level(LevelSetting::BindTitle).unwrap();

    app.active.as_mut().unwrap().reading.jump(2, None).unwrap();
    record_quote(&mut app, "An appendix without headings.");
    assert_eq!(
        state(&app).highlights[4].chapter_title.as_deref(),
        Some("West quay")
    );
    app.show(Screen::Bookmarks);
    let rows = app.items();
    assert!(rows.iter().any(|row| row.title == "In Harbor Survey"
        && row.detail.as_deref() == Some("Page bookmark · Page 1")));
    assert!(
        rows.iter()
            .any(|row| row.title == "An east quay field note."
                && row.detail.as_deref() == Some("Highlight · East softhyphen quay · Page 1"))
    );
    app.highlight_index = 0;
    app.show(Screen::Annotation);
    assert!(
        app.items()
            .iter()
            .any(|row| row.title == "Chapter"
                && row.detail.as_deref() == Some("East softhyphen quay"))
    );
    capture(&app, &mut wide, "saved-mark-chapter-landscape.png", false);

    app.library.set_alternative_contents(id, false).unwrap();
    app.sync_alternative_contents();
    app.save().unwrap();
    let mut reopened = open(&fixture, &path);
    assert!(!reopened.uses_alternative_contents());
    assert_eq!(
        state(&reopened).bookmarks[0].chapter_title,
        state(&app).bookmarks[0].chapter_title
    );
    assert_eq!(state(&reopened).highlights, state(&app).highlights);
    reopened.show(Screen::Bookmarks);
    let mut wide = adopt(&mut reopened, ReadingViewport::LANDSCAPE, 1);
    capture(
        &reopened,
        &mut wide,
        "saved-chapter-bookmarks-landscape.png",
        true,
    );
    reopened.activate(Action::Highlight(0)).unwrap();
    let reading = &reopened.active.as_ref().unwrap().reading;
    assert_eq!(reading.chapter, 1);
    assert_eq!(reading.probe.page_for(&east), Some(reading.page));
    assert_eq!(fs::read(&path).unwrap(), bytes);
}

#[test]
fn mark_chapters_pdf_titles_follow_physical_page_and_custom_outline() {
    let fixture = Fixture::new();
    let path = fixture.root.join("Coast field guide.pdf");
    crate::app::progress_markers::tests::write_pdf(&path);
    let source = fs::read(&path).unwrap();
    let mut app = open(&fixture, &path);
    app.follow_fixed(1, None).unwrap();
    app.activate(Action::ToggleBookmark).unwrap();
    assert_eq!(
        state(&app).bookmarks[0].chapter_title.as_deref(),
        Some("Western tide")
    );
    app.follow_fixed(2, None).unwrap();
    app.activate(Action::ToggleBookmark).unwrap();
    assert_eq!(
        state(&app).bookmarks[1].chapter_title.as_deref(),
        Some("Harbor lookout")
    );
    app.set_chapter_level(LevelSetting::ToggleLevel(1)).unwrap();
    app.set_chapter_level(LevelSetting::BindTitle).unwrap();

    let page = 2;
    let text = app.fixed.as_mut().unwrap().reading.page_text(page).unwrap();
    let bounds = text.words[0].bounds;
    app.jump_fixed(page, crate::app::FixedJump::regions(&[bounds]))
        .unwrap();
    app.pdf_result_regions.clear();
    let fixed = app.fixed.as_ref().unwrap();
    let at = fixed
        .reading
        .visible_rectangle(page, bounds)
        .unwrap()
        .center();
    let crate::fixed_session::WordAtTouch::Ready(Some(selection)) =
        fixed.reading.select_word(at.x as f32, at.y as f32).unwrap()
    else {
        panic!("The authored PDF word must be selectable")
    };
    app.pdf_selection = Some(selection);
    app.save_pdf_highlight(Some("Track the tide".into()))
        .unwrap();
    assert_eq!(
        state(&app).pdf_highlights[0].chapter_title.as_deref(),
        Some("Western tide")
    );
    assert_eq!(
        state(&app).bookmarks[1].chapter_title.as_deref(),
        Some("Harbor lookout")
    );

    let id = app.open_book_id().unwrap();
    let custom = &mut app.library.state_mut(id).unwrap().custom_contents;
    let position = ReadingPosition {
        chapter: 0,
        page: 3,
        anchor: None,
    };
    custom.save(position, "Reader's route").unwrap();
    custom.enabled = true;
    app.follow_fixed(3, None).unwrap();
    app.activate(Action::ToggleBookmark).unwrap();
    assert_eq!(
        state(&app).bookmarks[2].chapter_title.as_deref(),
        Some("Reader's route")
    );
    assert_eq!(fs::read(&path).unwrap(), source);
}

#[test]
fn mark_chapters_decode_legacy_marks_and_preserve_the_snapshot_in_backup() {
    let legacy: crate::library::Bookmark = serde_json::from_str(
        r#"{"label":"Page 7","position":{"chapter":0,"page":6,"anchor":null}}"#,
    )
    .unwrap();
    assert!(legacy.chapter_title.is_none());
    assert!(
        !serde_json::to_string(&legacy)
            .unwrap()
            .contains("chapter_title")
    );
    let newer: crate::library::Bookmark = serde_json::from_str(
        r#"{"chapter_title":"  Coastal\u00ad   Route\n","label":"Page 7","position":{"chapter":0,"page":6,"anchor":null}}"#,
    ).unwrap();
    assert_eq!(newer.chapter_title.as_deref(), Some("Coastal Route"));
    let mut source: BookState =
        serde_json::from_str(r#"{"id":7,"path":"coast.pdf","title":"Coast"}"#).unwrap();
    source.bookmarks.push(newer);
    let mut original = source.clone();
    original.bookmarks[0].chapter_title = Some("Original title".into());
    let mut summary = crate::library::BackupSummary::default();
    crate::library::merge_text_marks(&mut original, &source.bookmarks, &[], &mut summary);
    assert_eq!(
        original.bookmarks[0].chapter_title.as_deref(),
        Some("Original title")
    );
    let mut restored = source.clone();
    restored.bookmarks.clear();
    crate::library::merge_text_marks(&mut restored, &source.bookmarks, &[], &mut summary);
    assert_eq!(
        restored.bookmarks[0].chapter_title.as_deref(),
        Some("Coastal Route")
    );
}
