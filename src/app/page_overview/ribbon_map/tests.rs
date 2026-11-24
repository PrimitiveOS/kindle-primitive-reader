use super::*;
use crate::app::fixed_book_map;
use crate::app::page_overview::OverviewLayout;
use crate::app::selection_viewport_tests::{adopt, capture, tap};
use crate::app::tests::Fixture;
use crate::session::ReadingViewport;
use primitive_app::App;
use std::{fs, path::Path};

fn open(fixture: &Fixture, path: &Path) -> Reader {
    let fonts = ["fonts/NotoSans-Regular.ttf", "fonts/NotoSans-Bold.ttf"]
        .into_iter()
        .map(crate::test_fixture)
        .collect();
    let mut reader = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
    reader.open_path(path).unwrap();
    reader
}

fn ribbon(reader: &mut Reader, ordinal: usize, count: usize, hold: bool) -> Change {
    let layout = OverviewLayout::new(reader.navigation_layout());
    let track = layout.track();
    let x = track.x as f32 + track.w as f32 * (ordinal as f32 / (count - 1).max(1) as f32);
    let x = x.min((track.right() - 1) as f32);
    let y = (track.y + track.h / 2) as f32;
    reader.input(if hold {
        Input::LongTap(x, y)
    } else {
        Input::Tap(x, y)
    })
}

fn back(reader: &mut Reader) {
    let button = reader.navigation_layout().back();
    tap(reader, button);
}

#[test]
fn ribbon_book_map_focuses_fixed_pages_and_restores_nested_browser_views() {
    let fixture = Fixture::new();
    let path = fixture.root.join("Harbor Atlas.pdf");
    let pages = 96;
    fs::write(
        &path,
        crate::fixed_session::tests::striped_pdf_sized_bytes(pages, 300, 400),
    )
    .unwrap();
    let mut reader = open(&fixture, &path);
    reader.follow_fixed(6, None).unwrap();
    reader.open_overview().unwrap();
    assert_eq!(ribbon(&mut reader, 52, pages, false), Change::Partial);
    let initial_focus = reader.overview.as_ref().unwrap().focus.clone();
    assert_eq!(initial_focus.page, 52);
    assert_eq!(ribbon(&mut reader, 64, pages, true), Change::Partial);
    assert_eq!(reader.screen, Screen::BookMap);
    assert_eq!(reader.fixed_ribbon_map_focus(), Some((64, 52)));
    let focused_window = reader.map_window;
    assert_eq!(focused_window, MapWindow::focused(64, pages));
    let mut wide = adopt(&mut reader, ReadingViewport::LANDSCAPE, 1);
    capture(
        &reader,
        &mut wide,
        "ribbon-book-map-fixed-landscape.png",
        false,
    );
    let location = reader.current_history_location();
    assert_eq!(location.as_ref().unwrap().position.page, 6);
    let history = reader.back.clone();

    // Individual pages are selectable in this large book's focused window.
    let cell = fixed_book_map::projected_cell(reader.navigation_layout(), 16);
    tap(&mut reader, cell);
    assert_eq!(reader.screen, Screen::PageOverview);
    assert_eq!(reader.overview.as_ref().unwrap().focus.page, 64);
    assert_eq!(ribbon(&mut reader, 20, pages, true), Change::Partial);
    assert_eq!(reader.fixed_ribbon_map_focus(), Some((20, 64)));
    assert_eq!(reader.ribbon_maps.len(), 2);
    back(&mut reader);
    assert_eq!(reader.screen, Screen::PageOverview);
    assert_eq!(reader.overview.as_ref().unwrap().focus.page, 64);
    back(&mut reader);
    assert_eq!(reader.screen, Screen::BookMap);
    assert_eq!(reader.map_window, focused_window);
    assert_eq!(reader.fixed_ribbon_map_focus(), Some((64, 52)));
    back(&mut reader);
    assert_eq!(reader.screen, Screen::PageOverview);
    assert_eq!(reader.overview.as_ref().unwrap().focus, initial_focus);
    assert!(reader.ribbon_maps.is_empty());
    assert_eq!(reader.current_history_location(), location);
    assert_eq!(reader.back, history);
}

#[test]
fn ribbon_book_map_focuses_reflow_chapters_and_restores_exact_browser_anchor() {
    let fixture = Fixture::new();
    let path = fixture.root.join("Night Landing.epub");
    fs::copy(crate::test_fixture("night-landing.epub"), &path).unwrap();
    let mut reader = open(&fixture, &path);
    let reading = &mut reader.active.as_mut().unwrap().reading;
    reading
        .reflow(crate::Settings {
            logical_font_size: None,
            font_size: 70.0,
            ..reading.config.settings
        })
        .unwrap();
    assert!(reading.probe.page_count() > 1);
    let chapters = reading.probe.chapter_count();
    assert!(chapters > 1);
    reader.follow_reflow_page(0, 1).unwrap();
    let origin = reader.current_history_location().unwrap().position.anchor;
    let history = reader.back.clone();
    reader.open_overview().unwrap();
    let focus = reader.overview.as_ref().unwrap().focus.anchor.clone();
    assert!(focus.is_some());

    let target = chapters - 1;
    assert_eq!(ribbon(&mut reader, target, chapters, true), Change::Partial);
    assert_eq!(reader.screen, Screen::BookMap);
    assert_eq!(reader.book_map_focus, Some(target));
    assert_eq!(
        reader.ribbon_map_target(),
        Some(RibbonMapTarget::Chapter(target))
    );
    let mut wide = adopt(&mut reader, ReadingViewport::LANDSCAPE, 1);
    capture(
        &reader,
        &mut wide,
        "ribbon-book-map-epub-landscape.png",
        false,
    );
    assert_eq!(reader.book_map_focus, Some(target));
    assert_eq!(reader.list_page, target / reader.list_geometry().rows);
    back(&mut reader);
    assert_eq!(reader.screen, Screen::PageOverview);
    assert_eq!(reader.overview.as_ref().unwrap().focus.anchor, focus);
    assert_eq!(
        reader.current_history_location().unwrap().position.anchor,
        origin
    );
    assert_eq!(reader.back, history);

    // Choosing to read the mapped chapter closes both nested surfaces.
    assert_eq!(ribbon(&mut reader, target, chapters, true), Change::Partial);
    let option = reader.book_map_tap_option();
    tap(&mut reader, option);
    let layout = reader.list_geometry();
    tap(&mut reader, layout.row(target % layout.rows));
    assert_eq!(reader.screen, Screen::Book);
    assert_eq!(reader.current_position().unwrap().chapter, target);
    assert!(reader.ribbon_maps.is_empty());
    assert_eq!(reader.back.len(), history.len() + 1);
}

#[test]
fn ribbon_book_map_uses_the_same_fixed_projection_for_comics_and_djvu() {
    for asset in ["harbor-signals.cbz", "beacon-atlas.djvu"] {
        let fixture = Fixture::new();
        let path = fixture.root.join(asset);
        fs::copy(crate::test_fixture(asset), &path).unwrap();
        let mut reader = open(&fixture, &path);
        let count = reader.fixed.as_ref().unwrap().reading.count();
        assert!(count > 1, "{asset}");
        reader.open_overview().unwrap();
        let origin = reader.current_history_location();
        assert_eq!(ribbon(&mut reader, count - 1, count, true), Change::Partial);
        assert_eq!(reader.fixed_ribbon_map_focus(), Some((count - 1, 0)));
        back(&mut reader);
        assert_eq!(reader.screen, Screen::PageOverview);
        assert_eq!(reader.overview.as_ref().unwrap().focus.page, 0);
        assert_eq!(reader.current_history_location(), origin);
    }
}
