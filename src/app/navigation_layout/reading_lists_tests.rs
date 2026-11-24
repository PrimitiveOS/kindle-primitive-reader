use super::super::*;
use crate::app::selection_viewport_tests::{adopt, capture, press, tap};
use crate::app::tests::{Fixture, write_outline_fixture};
use crate::session::ReadingViewport;
use primitive_app::viewport::{LogicalTurn, ViewportDecision};
use std::fmt::Write as _;

fn open(fixture: &Fixture) -> Reader {
    let fonts = vec![
        crate::test_fixture("fonts/NotoSans-Regular.ttf"),
        crate::test_fixture("fonts/NotoSans-Bold.ttf"),
    ];
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
    app.open_path(&fixture.book).unwrap();
    app
}

fn tap_row(app: &mut Reader, offset: usize) {
    let row = app.list_geometry().row(offset);
    let x = (row.right() - 24) as f32;
    let y = (row.y + row.h / 2) as f32;
    assert!(x > WIDTH as f32);
    assert_eq!(app.input(Input::Tap(x, y)), Change::Partial);
}

#[test]
fn reading_lists_viewport_printed_pages_keep_the_first_label_and_use_the_shifted_shortcuts() {
    let fixture = Fixture::new();
    let mut nav = String::from(
        r#"<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><body><nav epub:type="page-list"><ol>"#,
    );
    for n in 1..=14 {
        write!(
            nav,
            r#"<li><a href="first.xhtml#passage{n}">Survey {n:02}</a></li>"#
        )
        .unwrap();
    }
    nav.push_str(r#"<li><a href="second.xhtml">Appendix</a></li></ol></nav></body></html>"#);
    write_outline_fixture(&fixture.book, Some(nav.as_bytes()), None);
    let mut app = open(&fixture);
    app.show(Screen::PublisherPages);
    let old_rows = app.list_geometry().rows;
    assert_eq!(app.input(Input::PageForward), Change::Partial);
    let original = app.list_window().1[0].title.clone();
    let old_page = app.list_page;
    let old_marker = app.current_contents_row;
    let candidate =
        reading_viewport_stage::draw_test_candidate(&mut app, 2, LogicalTurn::Clockwise, "", false);
    assert_eq!(app.list_page, old_page);
    assert_eq!(app.current_contents_row, old_marker);
    app.finish_viewport(candidate, ViewportDecision::Abort);
    assert_eq!(app.reading_viewport(), ReadingViewport::PORTRAIT);
    assert_eq!(app.list_window().1[0].title, original);

    let _landscape = adopt(&mut app, ReadingViewport::LANDSCAPE, 3);
    let _portrait = adopt(&mut app, ReadingViewport::PORTRAIT, 4);
    assert_eq!(app.list_page, old_page);
    assert_eq!(app.list_window().1[0].title, original);
    let mut canvas = adopt(&mut app, ReadingViewport::LANDSCAPE, 5);
    let geometry = app.list_geometry();
    let layout = app.navigation_layout();
    assert!(geometry.rows < old_rows);
    let (count, visible) = app.list_window();
    assert_eq!(count, 15);
    assert_eq!(visible.len(), geometry.rows);
    assert!(visible.iter().any(|item| item.title == original));
    assert_eq!(app.current_contents_row, app.visible_publisher_page());
    let origin = app.current_history_location();
    let next = layout.edge_footer(NEXT_MATCH_PAGE);
    let first = layout.edge_footer(FIRST_MATCH_PAGE);
    let last = layout.edge_footer(LAST_MATCH_PAGE);
    assert!(next.x + next.w / 2 > WIDTH as i32 && last.bottom() < layout.height);
    press(&app, &mut canvas, next, false);
    capture(&app, &mut canvas, "landscape-publisher-pages.png", false);
    tap(&mut app, next);
    assert_eq!(app.list_page, 2);
    tap(&mut app, first);
    assert_eq!(app.list_page, 0);
    tap(&mut app, last);
    assert_eq!(app.list_page, 2);
    assert_eq!(app.list_window().1[0].title, "Appendix");
    tap_row(&mut app, 0);
    assert_eq!(app.current_position().unwrap().chapter, 1);
    assert_eq!(app.current_publisher_page(), Some(14));
    app.activate(Action::ReturnToLocation).unwrap();
    assert_eq!(app.current_history_location(), origin);

    let location = app.publisher_page_window(8, 1).1.remove(0).action;
    app.activate(location).unwrap();
    app.show(Screen::PublisherPages);
    let marker = app.current_contents_row.unwrap();
    assert_eq!(app.list_page, marker / geometry.rows);
    assert!(app.list_window().1.len() <= geometry.rows);
}

#[test]
fn reading_lists_viewport_location_history_keeps_the_original_history_index_and_source_anchor() {
    let fixture = Fixture::new();
    fs::copy(crate::test_fixture("long-chapter.epub"), &fixture.book).unwrap();
    let mut app = open(&fixture);
    let probe = &app.active.as_ref().unwrap().reading.probe;
    let locations: Vec<_> = (0..14)
        .map(|page| ReadingPosition {
            chapter: 0,
            page,
            anchor: Some(probe.page_anchor(page).unwrap()),
        })
        .collect();
    for location in locations {
        app.push_history(location);
    }
    app.show(Screen::LocationHistory);
    assert_eq!(app.input(Input::PageForward), Change::Partial);
    let Action::ReturnToHistory(old_index) = app.list_window().1[0].action else {
        panic!("Expected the oldest visible history row");
    };
    let source = app.back[old_index].position.anchor.clone().unwrap();
    let mut canvas = adopt(&mut app, ReadingViewport::LANDSCAPE, 2);
    let (total, rows) = app.list_window();
    assert_eq!(total, 14);
    let offset = rows
        .iter()
        .position(|row| matches!(row.action, Action::ReturnToHistory(i) if i == old_index))
        .unwrap();
    let layout = app.navigation_layout();
    press(&app, &mut canvas, layout.back(), false);
    tap_row(&mut app, offset);
    assert_eq!(app.screen, Screen::Book);
    let reading = &app.active.as_ref().unwrap().reading;
    assert_eq!(reading.probe.page_for(&source), Some(reading.page));
    assert!(!app.forward.is_empty());
}
