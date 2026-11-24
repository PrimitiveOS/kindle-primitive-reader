use super::super::super::list_density::{APPLY, DECREASE, DISABLE_LINES, INCREASE, LINE_DEFAULT};
use super::super::super::tests::{Fixture, capture_test_screen, tap_contents_item};
use super::super::tests::{bookmark_settings, hold, open, tap};
use super::*;
use crate::app::Screen;
use crate::fixed_text::PdfTextRect;
use crate::library::{Bookmark, PdfHighlight, ReadingPosition};
use crate::reading_controls::ListDensity;
use primitive_app::App;
use std::fs;

#[test]
fn flexible_bookmark_pages_measure_lazily_and_pack_every_item_once() {
    let fonts = [crate::test_fixture("fonts/NotoSans-Regular.ttf")
        .display()
        .to_string()];
    let paragraph = "Verify the shoreline and compare the marked heights before recording the next set of measurements. ";
    let items: Vec<_> = (0..64)
        .map(|index| {
            let text = if index % 4 == 1 {
                paragraph.repeat(8)
            } else {
                format!("Mark {index}")
            };
            Item::new(text, Action::Bookmark(index)).detail("Page 3")
        })
        .collect();
    let mut view = FlexibleMarks::new(items, &fonts, 28, 4).unwrap();
    assert_eq!(view.pages.len(), 1);
    assert!(view.pages[0].end() < view.items.len());
    assert!(view.measuring.is_some());
    let heights = || view.pages[0].rows.iter().map(|row| row.rect.h);
    assert!(heights().min() < heights().max());
    assert!(view.spill.is_some());

    while view.append_page() {}
    let mut seen = 0;
    for page in &view.pages {
        let mut top = LIST_Y;
        for row in &page.rows {
            assert_eq!(row.rect.y, top);
            assert!(row.rect.h >= MIN_ROW_HEIGHT && row.rect.bottom() <= LIST_Y + LIST_HEIGHT);
            assert_eq!(
                page.hit((LIST_X + 30) as f32, (row.rect.bottom() - 1) as f32),
                Some(seen)
            );
            assert!(row.title.height() <= 4 * ((font_pixels(28) * 13 + 5) / 10));
            top = row.rect.bottom();
            seen += 1;
        }
        assert_eq!(page.hit((LIST_X + 30) as f32, top as f32), None);
    }
    assert_eq!(seen, view.items.len());
    assert!(view.measuring.is_none() && view.spill.is_none());
}

#[test]
fn flexible_bookmark_lines_persist_and_restore_the_previous_fixed_layout() {
    let fixture = Fixture::new();
    let mut app = open(&fixture, &fixture.book);
    assert_eq!(app.controls.bookmark_max_lines(), None);
    let reading = &mut app.active.as_mut().unwrap().reading;
    reading.jump(0, Some("lantern")).unwrap();
    let start = reading.probe.fragment("lantern").unwrap();
    let mut end = start.clone();
    end.offset += "The copper lantern".len();
    app.selection = reading.probe.select_range(&start, &end);
    let note = "Compare the coastal chart against the readings recorded at the lantern. Keep the original field notes beside the new measurements and revisit the shoreline before dusk.";
    app.save_highlight(Some(note.into())).unwrap();
    let id = app.open_book_id().unwrap();
    let state = app.library.state_mut(id).unwrap();
    state.bookmarks.extend((1..=15).map(|index| Bookmark {
        created_at: None,
        label: format!("Reading mark {index:02}"),
        position: ReadingPosition {
            chapter: 1,
            page: 0,
            anchor: None,
        },
    }));
    app.controls.bookmark_shrink_to_fit = true;
    app.mark_dirty();
    app.save().unwrap();
    let font_names = app
        .fonts
        .iter()
        .map(|font| font.display().to_string())
        .collect::<Vec<_>>();
    let mut canvas = Canvas::new(WIDTH, HEIGHT, &font_names).unwrap();
    let origin = app.current_position().unwrap();
    let history = app.back.len();
    app.show(Screen::Bookmarks);
    let legacy = primitive_app::frame(&app, &mut canvas, false);

    app.show(Screen::Book);
    bookmark_settings(&mut app);
    tap_contents_item(&mut app, "Max lines per bookmark");
    assert_eq!(app.list_density_picker.as_ref().unwrap().value, Some(4));
    assert_eq!(hold(&mut app, DECREASE), Change::Partial);
    assert_eq!(app.input(Input::PageBack), Change::None);
    assert_eq!(hold(&mut app, INCREASE), Change::Partial);
    assert_eq!(hold(&mut app, INCREASE), Change::Partial);
    assert_eq!(hold(&mut app, INCREASE), Change::Partial);
    assert_eq!(app.input(Input::PageForward), Change::None);
    assert_eq!(app.list_density_picker.as_ref().unwrap().value, Some(10));
    assert_eq!(tap(&mut app, LINE_DEFAULT), Change::Partial);
    assert_eq!(tap(&mut app, APPLY), Change::Partial);
    assert_eq!(app.controls.bookmark_max_lines(), Some(4));

    for (label, detail) in [
        ("Rows per page", "Flexible"),
        (
            "Shrink bookmark font size to fit more text",
            "Unavailable with flexible rows",
        ),
    ] {
        let item = app
            .items()
            .into_iter()
            .find(|item| item.title == label)
            .unwrap();
        assert_eq!(item.detail.as_deref(), Some(detail));
        assert!(matches!(item.action, Action::Heading));
    }
    app.show(Screen::Bookmarks);
    let view = app.flexible_marks.as_ref().unwrap();
    let page = &view.pages[0];
    let note_index = view
        .items
        .iter()
        .position(|item| item.title == note)
        .unwrap();
    let note_rect = page.rows[note_index].rect;
    assert!(note_rect.h > MIN_ROW_HEIGHT);
    assert_ne!(primitive_app::frame(&app, &mut canvas, false), legacy);
    capture_test_screen(
        &app,
        &app.fonts,
        "READER_FLEXIBLE_BOOKMARK_SCREENSHOT",
        false,
    );
    assert_eq!(app.input(Input::PageForward), Change::Partial);
    assert_eq!(app.flexible_marks.as_ref().unwrap().pages.len(), 2);
    assert_eq!(app.input(Input::PageBack), Change::Partial);
    assert_eq!(app.current_position(), Some(origin.clone()));
    assert_eq!(app.back.len(), history);
    let search = app.flexible_marks.as_ref().unwrap().pages[0].rows[0].rect;
    assert_eq!(tap(&mut app, search), Change::Partial);
    assert_eq!(app.screen, Screen::BookmarkFind);
    assert_eq!(
        tap(&mut app, primitive_ui::back_button(WIDTH as i32)),
        Change::Partial
    );
    assert_eq!(app.screen, Screen::Bookmarks);

    let mut reopened = open(&fixture, &fixture.book);
    assert_eq!(reopened.controls.bookmark_max_lines(), Some(4));
    assert!(reopened.controls.bookmark_shrink_to_fit);
    reopened.show(Screen::Bookmarks);
    assert_eq!(hold(&mut reopened, note_rect), Change::Partial);
    assert_eq!(reopened.screen, Screen::Annotation);
    assert_eq!(reopened.highlight_index, 0);
    reopened.activate(Action::Highlight(0)).unwrap();
    assert_eq!(reopened.current_position(), Some(origin));

    bookmark_settings(&mut reopened);
    tap_contents_item(&mut reopened, "Max lines per bookmark");
    assert_eq!(tap(&mut reopened, DISABLE_LINES), Change::Partial);
    assert_eq!(tap(&mut reopened, APPLY), Change::Partial);
    assert_eq!(reopened.controls.bookmark_max_lines(), None);
    assert!(reopened.controls.bookmark_shrink_to_fit);
    reopened.show(Screen::Bookmarks);
    assert!(reopened.flexible_marks.is_none());
    assert_eq!(primitive_app::frame(&reopened, &mut canvas, false), legacy);
}

#[test]
fn flexible_fixed_page_rows_keep_oversized_notes_visible_and_target_the_painted_row() {
    let fixture = Fixture::new();
    let pdf = fixture.root.join("Survey.pdf");
    fs::write(&pdf, crate::fixed::tests::document()).unwrap();
    let mut app = open(&fixture, &pdf);
    let fixed = app.fixed.as_ref().unwrap();
    let state = app
        .library
        .remember(&fixed.entry, fixed.id, &fixed.entry.title);
    for index in 0..4 {
        let y = 0.1 + index as f32 * 0.1;
        state
            .add_pdf_highlight(PdfHighlight {
                color: Default::default(),
                created_at: Some(index),
                style: Default::default(),
                continuation: Vec::new(),
                page: 1,
                bounds: vec![PdfTextRect {
                    x0: 0.1,
                    x1: 0.5,
                    y0: y,
                    y1: y + 0.04,
                }],
                text: format!("Survey paragraph {index}"),
                note: Some(format!(
                    "Survey note {index}: {}",
                    "The water is rising along the marked shore. ".repeat(64)
                )),
            })
            .unwrap();
    }
    app.controls.bookmark_max_lines = Some(10);
    app.controls.bookmark_density = ListDensity {
        rows_per_page: Some(24),
        font_size: Some(72),
    };
    app.show(Screen::FixedBookmarks);
    assert_eq!(tap(&mut app, NEXT), Change::Partial);
    let view = app.flexible_marks.as_ref().unwrap();
    let page = &view.pages[1];
    assert_eq!(page.rows.len(), 1);
    let row = &page.rows[0];
    assert!(row.rect.bottom() <= LIST_Y + LIST_HEIGHT);
    assert!(!row.title.complete(&view.items[page.first].title));
    let Action::PdfHighlight(index) = view.items[page.first].action else {
        panic!("The second page should start with a PDF note");
    };
    let target = row.rect;
    assert_eq!(hold(&mut app, target), Change::Partial);
    assert_eq!(app.screen, Screen::Annotation);
    assert_eq!(app.highlight_index, index);
    app.activate(Action::PdfHighlight(index)).unwrap();
    assert_eq!(app.fixed.as_ref().unwrap().reading.page(), 1);
}
