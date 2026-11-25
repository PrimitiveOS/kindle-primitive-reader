use super::super::super::tests::{Fixture, capture_test_screen, tap_contents_item};
use super::super::tests::{bookmark_settings, hold, open};
use super::*;
use crate::app::Screen;
use crate::app::{HEIGHT, WIDTH};
use crate::fixed_text::PdfTextRect;
use crate::highlight_style::HighlightStyle;
use crate::library::{Bookmark, Highlight, PdfHighlight};
use crate::reading_controls::{ListDensity, ReadingControls};
use primitive_app::App;
use std::fs;

const DEFAULT_NOTE: &str = "Tide line in the book's default color";
const NAMED_NOTE: &str =
    "Compare the readings against the coastal chart and the notes from the field survey.";
const LEGACY_NOTE: &str = "An older gray highlight";
const PAGE_MARK: &str = "Saved page";

fn row(app: &Reader, title: &str) -> Rect {
    if let Some(view) = &app.flexible_marks {
        let page = &view.pages[app.list_page];
        let offset = view.items[page.first..page.end()]
            .iter()
            .position(|item| item.title == title)
            .expect("mark is shown on the planned page");
        page.rows[offset].rect
    } else {
        let offset = app
            .list_window()
            .1
            .iter()
            .position(|item| item.title == title)
            .expect("mark is shown on the fixed page");
        app.list_geometry().row(offset)
    }
}

fn differs_in(before: &[u8], after: &[u8], rect: Rect) -> bool {
    (rect.y..rect.bottom()).any(|y| {
        let start = y as usize * WIDTH as usize + rect.x as usize;
        let end = start + rect.w as usize;
        before[start..end] != after[start..end]
    })
}

#[test]
fn bookmark_appearance_preserves_older_profiles_and_explicit_separator_choices() {
    let legacy: ReadingControls = serde_json::from_str(r#"{"bookmark_reverse":true}"#).unwrap();
    assert!(legacy.bookmark_list_separators && legacy.bookmark_reverse);
    assert!(!legacy.bookmark_show_highlight_colors && !legacy.bookmark_show_default_color);
    let mut fresh = ReadingControls::default();
    assert!(!fresh.bookmark_list_separators);
    fresh.bookmark_show_default_color = true;
    let restored: ReadingControls =
        serde_json::from_str(&serde_json::to_string(&fresh).unwrap()).unwrap();
    assert_eq!(restored, fresh);
}

#[test]
fn bookmark_appearance_switches_persist_and_keep_epub_note_targets_in_both_layouts() {
    let fixture = Fixture::new();
    let mut app = open(&fixture, &fixture.book);
    let reading = &mut app.active.as_mut().unwrap().reading;
    reading.jump(0, Some("lantern")).unwrap();
    let start = reading.probe.fragment("lantern").unwrap();
    let highlights: Vec<_> = [
        (
            0,
            3,
            HighlightColor::Red,
            HighlightStyle::Lighten,
            DEFAULT_NOTE,
        ),
        (
            4,
            10,
            HighlightColor::Purple,
            HighlightStyle::Underline,
            NAMED_NOTE,
        ),
        (
            11,
            18,
            HighlightColor::Gray,
            HighlightStyle::Invert,
            LEGACY_NOTE,
        ),
    ]
    .into_iter()
    .map(|(first, last, color, style, note)| {
        let mut from = start.clone();
        from.offset += first;
        let mut to = start.clone();
        to.offset += last;
        Highlight {
            updated_at: None,
            chapter_title: None,
            created_at: Some(1_750_000_000 + first as i64),
            style,
            color,
            selection: reading.probe.select_range(&from, &to).unwrap(),
            note: Some(note.into()),
        }
    })
    .collect();
    let position = app.current_position().unwrap();
    let id = app.open_book_id().unwrap();
    let state = app.library.state_mut(id).unwrap();
    state.highlight_color = Some(HighlightColor::Red);
    state.highlights.extend(highlights);
    state.bookmarks.push(Bookmark {
        note: None,
        updated_at: None,
        chapter_title: None,
        position: position.clone(),
        label: PAGE_MARK.into(),
        created_at: None,
    });
    app.mark_dirty();
    app.save().unwrap();
    app.controls.bookmark_density.rows_per_page = Some(24);

    bookmark_settings(&mut app);
    let also = app
        .items()
        .into_iter()
        .find(|item| item.title == "Also show default highlight color")
        .unwrap();
    assert!(matches!(also.action, Action::Heading));
    tap_contents_item(&mut app, "Show separator between items");
    tap_contents_item(&mut app, "Show highlight colors");
    assert_eq!(app.bookmark_row_color(&Action::Bookmark(0)), None);
    assert_eq!(app.bookmark_row_color(&Action::Highlight(0)), None);
    assert_eq!(
        app.bookmark_row_color(&Action::Highlight(1)),
        Some(HighlightColor::Purple)
    );
    assert_eq!(
        app.bookmark_row_color(&Action::Highlight(2)),
        Some(HighlightColor::Gray)
    );
    tap_contents_item(&mut app, "Also show default highlight color");
    assert_eq!(
        app.bookmark_row_color(&Action::Highlight(0)),
        Some(HighlightColor::Red)
    );

    let mut reopened = open(&fixture, &fixture.book);
    let enabled = reopened.controls;
    assert!(
        enabled.bookmark_list_separators
            && enabled.bookmark_show_highlight_colors
            && enabled.bookmark_show_default_color
    );
    let fonts = reopened
        .fonts
        .iter()
        .map(|font| font.display().to_string())
        .collect::<Vec<_>>();
    let mut canvas = Canvas::new(WIDTH, HEIGHT, &fonts).unwrap();

    for max_lines in [None, Some(4)] {
        reopened.controls.bookmark_max_lines = max_lines;
        reopened.show(Screen::Bookmarks);
        let actual_target = row(&reopened, NAMED_NOTE);
        let actual_default = row(&reopened, DEFAULT_NOTE);
        let actual_page_mark = row(&reopened, PAGE_MARK);
        let colored = primitive_app::frame(&reopened, &mut canvas, false);
        let on_dark = primitive_app::frame(&reopened, &mut canvas, true);

        reopened.controls.bookmark_show_default_color = false;
        let exclude_default = primitive_app::frame(&reopened, &mut canvas, false);
        assert!(differs_in(&colored, &exclude_default, actual_default));
        assert!(!differs_in(&colored, &exclude_default, actual_target));
        reopened.controls.bookmark_show_default_color = true;
        reopened.controls.bookmark_show_highlight_colors = false;
        let separators_only = primitive_app::frame(&reopened, &mut canvas, false);
        let separators_only_dark = primitive_app::frame(&reopened, &mut canvas, true);
        assert!(differs_in(&colored, &separators_only, actual_target));
        assert!(differs_in(&on_dark, &separators_only_dark, actual_target));
        assert!(!differs_in(&colored, &separators_only, actual_page_mark));
        assert_eq!(reopened.bookmark_row_color(&Action::Highlight(1)), None);

        reopened.controls.bookmark_list_separators = false;
        let plain = primitive_app::frame(&reopened, &mut canvas, false);
        let plain_dark = primitive_app::frame(&reopened, &mut canvas, true);
        let last_pixels = Rect::new(
            actual_default.x + 48,
            actual_default.bottom() - 2,
            actual_default.w - 96,
            2,
        );
        assert!(differs_in(&plain, &separators_only, last_pixels));
        assert!(differs_in(&plain_dark, &on_dark, last_pixels));
        assert_eq!(row(&reopened, NAMED_NOTE), actual_target);
        if max_lines.is_some() {
            capture_test_screen(
                &reopened,
                &reopened.fonts,
                "READER_BOOKMARK_APPEARANCE_BASE",
                false,
            );
        }

        reopened.controls.bookmark_list_separators = true;
        reopened.controls.bookmark_show_highlight_colors = true;
        if max_lines.is_some() {
            capture_test_screen(
                &reopened,
                &reopened.fonts,
                "READER_BOOKMARK_APPEARANCE_ON",
                false,
            );
        }
        assert_eq!(hold(&mut reopened, actual_target), Change::Partial);
        assert_eq!(reopened.screen, Screen::Annotation);
        assert_eq!(reopened.highlight_index, 1);
    }
}

#[test]
fn bookmark_appearance_keeps_fixed_page_cues_and_actions_in_both_layouts() {
    let fixture = Fixture::new();
    let pdf = fixture.root.join("Survey.pdf");
    fs::write(&pdf, crate::fixed::tests::document()).unwrap();
    let mut app = open(&fixture, &pdf);
    let fixed = app.fixed.as_ref().unwrap();
    let state = app
        .library
        .remember(&fixed.entry, fixed.id, &fixed.entry.title);
    state.highlight_color = Some(HighlightColor::Blue);
    state
        .add_pdf_highlight(PdfHighlight {
            updated_at: None,
            chapter_title: None,
            color: HighlightColor::Blue,
            created_at: None,
            style: HighlightStyle::Underline,
            continuation: Vec::new(),
            page: 1,
            bounds: vec![PdfTextRect {
                x0: 0.1,
                x1: 0.5,
                y0: 0.1,
                y1: 0.15,
            }],
            text: "Survey paragraph".into(),
            note: Some(NAMED_NOTE.into()),
        })
        .unwrap();
    app.controls.bookmark_density = ListDensity {
        rows_per_page: Some(24),
        font_size: None,
    };
    let fonts = app
        .fonts
        .iter()
        .map(|font| font.display().to_string())
        .collect::<Vec<_>>();
    let mut canvas = Canvas::new(WIDTH, HEIGHT, &fonts).unwrap();

    for max_lines in [None, Some(4)] {
        app.controls.bookmark_max_lines = max_lines;
        app.controls.bookmark_show_highlight_colors = true;
        app.controls.bookmark_show_default_color = false;
        app.controls.bookmark_list_separators = false;
        app.show(Screen::FixedBookmarks);
        let target = row(&app, NAMED_NOTE);
        assert_eq!(app.bookmark_row_color(&Action::PdfHighlight(0)), None);
        let excluded = primitive_app::frame(&app, &mut canvas, false);
        app.controls.bookmark_show_default_color = true;
        app.controls.bookmark_list_separators = true;
        assert_eq!(
            app.bookmark_row_color(&Action::PdfHighlight(0)),
            Some(HighlightColor::Blue)
        );
        let included = primitive_app::frame(&app, &mut canvas, false);
        assert!(differs_in(&excluded, &included, target));
        let search = row(&app, "Find bookmarks");
        assert!(differs_in(
            &excluded,
            &included,
            Rect::new(search.x + 48, search.bottom() - 2, search.w - 96, 2)
        ));
        assert_eq!(row(&app, NAMED_NOTE), target);
        assert_eq!(hold(&mut app, target), Change::Partial);
        assert_eq!(app.screen, Screen::Annotation);
        app.activate(Action::PdfHighlight(0)).unwrap();
        assert_eq!(app.fixed.as_ref().unwrap().reading.page(), 1);
    }
}
