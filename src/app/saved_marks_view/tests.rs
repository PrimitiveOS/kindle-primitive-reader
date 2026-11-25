use super::super::list_density::{APPLY, DECREASE, INCREASE};
use super::super::tests::{Fixture, capture_test_screen, tap_contents_item, tap_reading_control};
use super::super::*;
use crate::library::{Bookmark, ReadingPosition};
use crate::reading_controls::ListDensity;
use primitive_app::App;

pub(super) fn open(fixture: &Fixture, path: &Path) -> Reader {
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

pub(super) fn tap(app: &mut Reader, rect: Rect) -> Change {
    let (x, y) = at(rect);
    app.input(Input::Tap(x, y))
}

pub(super) fn hold(app: &mut Reader, rect: Rect) -> Change {
    let (x, y) = at(rect);
    app.input(Input::LongTap(x, y))
}

pub(super) fn bookmark_settings(app: &mut Reader) {
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
        note: None,
        updated_at: None,
        chapter_title: None,
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
                updated_at: None,
                chapter_title: None,
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
                note: Some(format!(
                    "Survey note {}: compare the page image with the original measurements and record any differences.",
                    index + 1
                )),
            })
            .unwrap();
    }
    app.controls.bookmark_density = ListDensity {
        rows_per_page: Some(24),
        font_size: Some(72),
    };
    app.controls.bookmark_shrink_to_fit = true;
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

#[test]
fn bookmark_font_fit_uses_the_largest_fitting_font_and_respects_both_size_limits() {
    use super::fit::TitleFit;

    let font = crate::test_fixture("fonts/NotoSans-Regular.ttf");
    let mut canvas = Canvas::new(WIDTH, HEIGHT, &[font.display().to_string()]).unwrap();
    let area = Rect::new(40, 100, 500, 98);
    let short = "Return to the chart.";
    let unchanged =
        TitleFit::measure(&mut canvas, short, area, 19).shrink(&mut canvas, short, area);
    assert_eq!(unchanged.logical_font, 19);
    assert_eq!(unchanged.displayed(short), short);

    let wraps = "Bring the coastal chart back to the cabin and compare the tide marks.";
    let normal = TitleFit::measure(&mut canvas, wraps, area, 19);
    assert!(!normal.complete(wraps));
    let fitted = normal.shrink(&mut canvas, wraps, area);
    assert!((12..19).contains(&fitted.logical_font));
    assert!(fitted.complete(wraps));
    assert!(!TitleFit::measure(&mut canvas, wraps, area, fitted.logical_font + 1).complete(wraps));

    let long = "Changes in the shoreline 夏休み。 ".repeat(30);
    for (requested, floor) in [(24, 16), (19, 12), (10, 10)] {
        let fit =
            TitleFit::measure(&mut canvas, &long, area, requested).shrink(&mut canvas, &long, area);
        assert_eq!(fit.logical_font, floor);
        assert!(!fit.complete(&long));
        assert!(fit.displayed(&long).ends_with('…'));
        assert!(fit.height() <= area.h);
    }

    let excerpt = "one piece of the shoreline ".repeat(24);
    assert!(bookmark_excerpt(&excerpt, false).ends_with('…'));
    assert_eq!(bookmark_excerpt(&excerpt, true), excerpt.trim());
}

#[test]
fn bookmark_font_fit_switch_persists_and_retains_the_exact_note_row() {
    let fixture = Fixture::new();
    let mut app = open(&fixture, &fixture.book);
    assert!(!app.controls.bookmark_shrink_to_fit);
    let reading = &mut app.active.as_mut().unwrap().reading;
    reading.jump(0, Some("lantern")).unwrap();
    let start = reading.probe.fragment("lantern").unwrap();
    let mut end = start.clone();
    end.offset += "The copper lantern".len();
    app.selection = reading.probe.select_range(&start, &end);
    let note = "Compare the coastal chart and the lantern measurements before returning to the shore at dusk.";
    app.save_highlight(Some(note.into())).unwrap();
    app.activate(Action::ToggleBookmark).unwrap();
    app.save().unwrap();

    app.show(Screen::Bookmarks);
    let origin = app.current_position().unwrap();
    let rows = app.list_geometry().rows;
    let marks = app.items();
    let index = marks.iter().position(|item| item.title == note).unwrap();
    let rect = app.list_geometry().row(index);
    let fonts = app
        .fonts
        .iter()
        .map(|font| font.display().to_string())
        .collect::<Vec<_>>();
    let mut canvas = Canvas::new(WIDTH, HEIGHT, &fonts).unwrap();
    let prior = primitive_app::frame(&app, &mut canvas, false);

    app.show(Screen::Book);
    bookmark_settings(&mut app);
    tap_contents_item(&mut app, "Shrink bookmark font size to fit more text");
    assert!(app.controls.bookmark_shrink_to_fit);
    app.show(Screen::Bookmarks);
    let fitted = primitive_app::frame(&app, &mut canvas, false);
    assert_ne!(fitted, prior);
    assert_eq!(app.list_geometry().rows, rows);
    assert_eq!(app.list_geometry().row(index), rect);
    assert_eq!(app.current_position(), Some(origin.clone()));
    capture_test_screen(&app, &app.fonts, "READER_BOOKMARK_FIT_SCREENSHOT", false);

    let mut reopened = open(&fixture, &fixture.book);
    assert!(reopened.controls.bookmark_shrink_to_fit);
    reopened.show(Screen::Bookmarks);
    assert_eq!(hold(&mut reopened, rect), Change::Partial);
    assert_eq!(reopened.screen, Screen::Annotation);
    assert_eq!(reopened.highlight_index, 0);
    reopened.activate(Action::Highlight(0)).unwrap();
    assert_eq!(reopened.current_position(), Some(origin));
}
