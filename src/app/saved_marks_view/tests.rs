use super::super::list_density::{APPLY, DECREASE, INCREASE};
use super::super::tests::{Fixture, capture_test_screen, tap_contents_item, tap_reading_control};
use super::super::*;
use crate::library::{Bookmark, ReadingPosition};
use crate::reading_controls::ListDensity;
use primitive_app::App;

fn open(fixture: &Fixture, path: &Path) -> Reader {
    let fonts = vec![
        crate::test_fixture("fonts/NotoSans-Regular.ttf"),
        crate::test_fixture("fonts/NotoSans-Bold.ttf"),
    ];
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
    app.open_path(path).unwrap();
    app
}

fn at(rect: Rect) -> (f32, f32) {
    ((rect.x + rect.w / 2) as f32, (rect.y + rect.h / 2) as f32)
}

fn tap(app: &mut Reader, rect: Rect) -> Change {
    let (x, y) = at(rect);
    app.input(Input::Tap(x, y))
}

fn hold(app: &mut Reader, rect: Rect) -> Change {
    let (x, y) = at(rect);
    app.input(Input::LongTap(x, y))
}

fn bookmark_settings(app: &mut Reader) {
    assert_eq!(app.input(Input::Tap(630.0, 25.0)), Change::Partial);
    if app.top_group != Some(TopGroup::Settings) {
        assert_eq!(tap_reading_control(app, "Settings"), Change::Partial);
    }
    assert_eq!(
        tap_reading_control(app, "Bookmarks and notes"),
        Change::Partial
    );
    assert_eq!(app.screen, Screen::BookmarkListSettings);
}

#[test]
fn bookmark_density_resizes_the_saved_marks_list_and_preserves_date_order() {
    let fixture = Fixture::new();
    let mut app = open(&fixture, &fixture.book);
    let reading = &mut app.active.as_mut().unwrap().reading;
    reading.jump(0, Some("lantern")).unwrap();
    let start = reading.probe.fragment("lantern").unwrap();
    let mut end = start.clone();
    end.offset += "The copper lantern".len();
    app.selection = reading.probe.select_range(&start, &end);
    app.save_highlight(Some("Check the chart at dusk".into()))
        .unwrap();

    let id = app.open_book_id().unwrap();
    let state = app.library.state_mut(id).unwrap();
    state.highlights[0].created_at = Some(1_750_001_000);
    state.bookmarks.extend((1..=12).map(|index| Bookmark {
        created_at: Some(1_750_000_000 + i64::from(index)),
        label: format!("Reading mark {index:02}"),
        position: ReadingPosition {
            chapter: usize::from(index % 2 != 0),
            page: 0,
            anchor: None,
        },
    }));
    app.mark_dirty();
    app.save().unwrap();
    let origin = app.current_position().unwrap();
    let history = app.back.len();

    bookmark_settings(&mut app);
    tap_contents_item(&mut app, "Sort by date");
    tap_contents_item(&mut app, "Reverse sorting");
    tap_contents_item(&mut app, "Rows per page");
    assert_eq!(hold(&mut app, DECREASE), Change::Partial);
    assert_eq!(hold(&mut app, DECREASE), Change::Partial);
    assert_eq!(tap(&mut app, APPLY), Change::Partial);
    assert_eq!(app.controls.bookmark_density.rows(), 6);
    assert_eq!(app.controls.bookmark_density.font(), 24);
    tap_contents_item(&mut app, "Bookmark font size");
    assert_eq!(hold(&mut app, INCREASE), Change::Partial);
    assert_eq!(hold(&mut app, INCREASE), Change::Partial);
    assert_eq!(tap(&mut app, APPLY), Change::Partial);
    assert_eq!(app.controls.bookmark_density.font(), 32);
    assert_eq!(app.controls.contents_density.rows(), 14);
    assert_eq!(app.controls.contents_density.font(), 19);

    app.show(Screen::Bookmarks);
    let (count, first) = app.list_window();
    assert_eq!(count, 17);
    assert_eq!(first.len(), 6);
    assert_eq!(first[4].title, "Check the chart at dusk");
    assert!(first[4].detail.as_deref().unwrap().contains(" · Note · "));
    assert_eq!(first[5].title, "Reading mark 12");
    capture_test_screen(
        &app,
        &app.fonts,
        "READER_BOOKMARK_DENSITY_SCREENSHOT",
        false,
    );
    assert_eq!(app.input(Input::PageForward), Change::Partial);
    assert_eq!(app.list_window().1[0].title, "Reading mark 11");
    assert_eq!(app.input(Input::PageBack), Change::Partial);
    assert_eq!(app.current_position(), Some(origin.clone()));
    assert_eq!(app.back.len(), history);

    let mut reopened = open(&fixture, &fixture.book);
    assert_eq!(reopened.controls.bookmark_density.rows(), 6);
    assert_eq!(reopened.controls.bookmark_density.font(), 32);
    assert!(reopened.controls.bookmark_sort_by_date);
    assert!(reopened.controls.bookmark_reverse);
    reopened.show(Screen::Bookmarks);
    assert_eq!(reopened.input(Input::PageForward), Change::Partial);
    let row = reopened.list_geometry().row(0);
    assert_eq!(tap(&mut reopened, row), Change::Partial);
    assert_eq!(reopened.screen, Screen::Book);
    assert_eq!(reopened.current_position().unwrap().chapter, 1);
}

#[test]
fn bookmark_density_keeps_fixed_page_highlight_holds_on_the_second_list_page() {
    let fixture = Fixture::new();
    let pdf = fixture.root.join("Survey.pdf");
    fs::write(&pdf, crate::fixed::tests::document()).unwrap();
    let mut app = open(&fixture, &pdf);
    let id = app.open_book_id().unwrap();
    let fixed = app.fixed.as_ref().unwrap();
    let state = app.library.remember(&fixed.entry, id, &fixed.entry.title);
    for index in 0..24 {
        let top = 0.1 + index as f32 * 0.032;
        state
            .add_pdf_highlight(PdfHighlight {
                color: Default::default(),
                created_at: None,
                style: Default::default(),
                continuation: Vec::new(),
                page: 1,
                bounds: vec![PdfTextRect {
                    x0: 0.1,
                    y0: top,
                    x1: 0.4,
                    y1: top + 0.018,
                }],
                text: format!("Passage {}", index + 1),
                note: Some(format!("Survey note {}", index + 1)),
            })
            .unwrap();
    }
    app.controls.bookmark_density = ListDensity {
        rows_per_page: Some(24),
        font_size: Some(72),
    };
    app.show(Screen::FixedBookmarks);
    assert_eq!(app.list_window().1.len(), 24);
    assert_eq!(app.input(Input::PageForward), Change::Partial);
    let first = app.list_window().1.remove(0);
    let Action::PdfHighlight(expected) = first.action else {
        panic!("Expected an existing fixed-page note");
    };
    let row = app.list_geometry().row(0);
    assert_eq!(hold(&mut app, row), Change::Partial);
    assert_eq!(app.screen, Screen::Annotation);
    assert_eq!(app.highlight_index, expected);
    app.activate(Action::PdfHighlight(expected)).unwrap();
    assert_eq!(app.screen, Screen::Fixed);
    assert_eq!(app.fixed.as_ref().unwrap().reading.page(), 1);
}
