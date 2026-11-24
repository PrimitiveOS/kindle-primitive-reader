use super::*;
use crate::app::selection_viewport_tests::{adopt, capture, tap};
use crate::app::tests::Fixture;
use crate::session::ReadingViewport;
use primitive_app::App;
use std::{fs, path::Path};

fn step(reader: &mut Reader, axis: Axis, increase: bool, hold: bool) -> Change {
    let x = if increase { 965 } else { 688 };
    let rect = reader
        .navigation_layout()
        .centered(Rect::new(x, axis.row(), 100, 100));
    let (x, y) = ((rect.x + 50) as f32, (rect.y + 50) as f32);
    reader.input(if hold {
        Input::LongTap(x, y)
    } else {
        Input::Tap(x, y)
    })
}

fn open_menu(reader: &mut Reader) {
    let button = grid_button(reader.navigation_layout());
    tap(reader, button);
    assert!(reader.overview.as_ref().unwrap().grid_menu);
}

fn close_menu(reader: &mut Reader) {
    let button = reader.navigation_layout().centered(CLOSE);
    tap(reader, button);
    assert!(!reader.overview.as_ref().unwrap().grid_menu);
}

fn pdf(path: &Path, count: usize) {
    let mut atlas = lopdf::Document::load_mem(
        &crate::fixed_session::tests::striped_pdf_sized_bytes(count, 300, 400),
    )
    .unwrap();
    for (ordinal, id) in atlas.get_pages() {
        let content = format!(
            "0 g BT /F1 23 Tf 24 350 Td (Harbor Atlas) Tj 0 -42 Td /F1 16 Tf (Survey sheet {ordinal}) Tj ET\n             0.25 G 3 w 25 200 m 85 205 l 130 251 l 175 214 l 220 230 l 266 219 l S\n             0.65 G 1 w 25 126 m 265 126 l 25 150 m 265 150 l 25 174 m 265 174 l S\n"
        );
        let stream = atlas.add_object(lopdf::Stream::new(
            lopdf::Dictionary::new(),
            content.into_bytes(),
        ));
        atlas
            .get_object_mut(id)
            .unwrap()
            .as_dict_mut()
            .unwrap()
            .set("Contents", lopdf::Object::Reference(stream));
    }
    atlas.save(path).unwrap();
}

#[test]
fn page_browser_grid_changes_batch_and_row_stride_and_keeps_map_focus() {
    let fixture = Fixture::new();
    let path = fixture.root.join("Harbor Atlas.pdf");
    pdf(&path, 38);
    let font = crate::test_fixture("fonts/NotoSans-Regular.ttf");
    let mut reader = Reader::open(&fixture.root, &fixture.state, vec![font]).unwrap();
    reader.open_path(&path).unwrap();
    reader.follow_fixed(10, None).unwrap();
    let history = reader.back.clone();
    reader.show(Screen::BookMap);
    let map_window = reader.map_window;
    let mut landscape = adopt(&mut reader, ReadingViewport::LANDSCAPE, 1);
    let origin = reader.current_history_location();
    assert_eq!(origin.as_ref().unwrap().position.page, 10);
    let cell = crate::app::fixed_book_map::projected_cell(reader.navigation_layout(), 17);
    tap(&mut reader, cell);
    let page = reader.overview.as_ref().unwrap().focus.page;
    assert_eq!(page, 17);
    open_menu(&mut reader);
    assert_eq!(
        step(&mut reader, Axis::Columns, false, false),
        Change::Partial
    );
    assert_eq!(step(&mut reader, Axis::Rows, true, true), Change::Partial);
    assert_eq!(step(&mut reader, Axis::Rows, false, false), Change::Partial);
    let overview = reader.overview.as_ref().unwrap();
    assert_eq!(overview.grid, Some(PageBrowserGrid::new(3, 2)));
    assert_eq!(overview.tiles.len(), 6);
    assert_eq!(overview.focus.page, page);
    assert_eq!(overview.map_origin, Some(MapOrigin::Fixed(map_window)));
    capture(
        &reader,
        &mut landscape,
        "page-browser-grid-controls.png",
        false,
    );
    close_menu(&mut reader);
    capture(
        &reader,
        &mut landscape,
        "page-browser-grid-landscape.png",
        false,
    );
    let first = reader.overview.as_ref().unwrap().first;
    assert_eq!(reader.input(Input::PageForward), Change::Partial);
    assert_eq!(reader.overview.as_ref().unwrap().first, first + 6);
    assert_eq!(
        reader.swipe(
            Input::Swipe {
                dx: 0.0,
                dy: -160.0,
                start_y: 550.0
            },
            300.0
        ),
        Change::Partial,
    );
    assert_eq!(reader.overview.as_ref().unwrap().first, first + 9);
    let back = reader.navigation_layout().back();
    tap(&mut reader, back);
    assert_eq!(reader.screen, Screen::BookMap);
    assert_eq!(reader.map_window, map_window);
    assert_eq!(reader.current_history_location(), origin);
    assert_eq!(reader.back, history);

    let id = reader.open_book_id().unwrap();
    let reopened = Reader::open(&fixture.root, &fixture.state, Vec::new()).unwrap();
    assert_eq!(
        reopened.library.page_browser_grid(id),
        Some(PageBrowserGrid::new(3, 2))
    );
}

#[test]
fn page_browser_grid_survives_reflow_turn_and_failed_persistence() {
    let fixture = Fixture::new();
    let path = fixture.root.join("Long Chapter.epub");
    fs::copy(crate::test_fixture("long-chapter.epub"), &path).unwrap();
    let font = crate::test_fixture("fonts/NotoSans-Regular.ttf");
    let mut reader = Reader::open(&fixture.root, &fixture.state, vec![font]).unwrap();
    reader.open_path(&path).unwrap();
    reader.follow_reflow_page(0, 3).unwrap();
    let origin = reader.current_history_location();
    reader.open_overview().unwrap();
    let focus = reader.overview.as_ref().unwrap().focus.clone();
    assert!(focus.anchor.is_some());
    open_menu(&mut reader);
    assert_eq!(
        step(&mut reader, Axis::Columns, true, false),
        Change::Partial
    );
    assert_eq!(reader.overview.as_ref().unwrap().focus.anchor, focus.anchor);
    assert_eq!(
        reader.overview.as_ref().unwrap().grid,
        Some(PageBrowserGrid::new(3, 2))
    );
    let mut landscape = adopt(&mut reader, ReadingViewport::LANDSCAPE, 1);
    let view = reader.overview.as_ref().unwrap();
    assert!(view.grid_menu);
    assert_eq!(view.focus.anchor, focus.anchor);
    assert_eq!(view.grid, Some(PageBrowserGrid::new(3, 2)));
    assert_eq!(
        reader.current_history_location().unwrap().position.anchor,
        origin.unwrap().position.anchor
    );
    let state_path = reader.library.state_path().to_owned();
    let intact = state_path.with_extension("saved");
    fs::rename(&state_path, &intact).unwrap();
    fs::create_dir(&state_path).unwrap();
    assert_eq!(
        step(&mut reader, Axis::Columns, true, false),
        Change::Partial
    );
    assert!(reader.notice.is_some());
    assert_eq!(
        reader.overview.as_ref().unwrap().grid,
        Some(PageBrowserGrid::new(3, 2))
    );
    assert_eq!(
        reader
            .library
            .page_browser_grid(reader.open_book_id().unwrap()),
        Some(PageBrowserGrid::new(3, 2))
    );
    fs::remove_dir(&state_path).unwrap();
    fs::rename(&intact, &state_path).unwrap();
    close_menu(&mut reader);
    capture(
        &reader,
        &mut landscape,
        "page-browser-grid-reflow-landscape.png",
        false,
    );

    let second = fixture.root.join("Reference Atlas.pdf");
    pdf(&second, 25);
    reader.open_path(&second).unwrap();
    reader.open_overview().unwrap();
    assert_eq!(
        reader.overview.as_ref().unwrap().grid,
        Some(PageBrowserGrid::new(3, 2))
    );
    open_menu(&mut reader);
    assert_eq!(step(&mut reader, Axis::Rows, true, true), Change::Partial);
    assert_eq!(
        reader.overview.as_ref().unwrap().grid,
        Some(PageBrowserGrid::new(3, 4))
    );
    reader.open_path(&path).unwrap();
    reader.open_overview().unwrap();
    assert_eq!(
        reader.overview.as_ref().unwrap().grid,
        Some(PageBrowserGrid::new(3, 2))
    );

    let backup = reader.library.backup_bytes().unwrap();
    let target = Fixture::new();
    let restored = target.app();
    let (saved, _) = restored.library.merged_backup_bytes(&backup).unwrap();
    fs::write(restored.library.state_path(), saved).unwrap();
    let reopened = target.app();
    assert_eq!(
        reopened.library.page_browser_grid(u64::MAX),
        Some(PageBrowserGrid::new(3, 4))
    );
}

#[test]
fn page_browser_grid_bounds_fit_the_viewport() {
    let grid = PageBrowserGrid::new(255, 0);
    assert_eq!(grid.dimensions(), (6, 1));
    assert_eq!(
        serde_json::from_str::<PageBrowserGrid>("[0,255]")
            .unwrap()
            .dimensions(),
        (1, 6)
    );
    for (width, height) in [(1264, 1680), (1680, 1264)] {
        let nav = NavigationLayout { width, height };
        let layout = OverviewLayout::with_grid(nav, Some(PageBrowserGrid::new(6, 6)));
        let first = layout.tile(0);
        let last = layout.tile(35);
        assert!(first.w >= 100 && first.h >= 100);
        assert!(last.right() <= width - LIST_X);
        assert!(last.bottom() < layout.track_caption().y);
        assert!(layout.label(last).bottom() < last.bottom());
    }
}

fn swipe(reader: &mut Reader, (x, y): (f32, f32), (dx, dy): (f32, f32)) -> Change {
    reader.swipe(Input::Swipe { dx, dy, start_y: y }, x)
}

#[test]
fn page_browser_grid_edge_swipes_resize_from_contact_in_both_viewports() {
    let fixture = Fixture::new();
    let path = fixture.root.join("Harbor Atlas.pdf");
    pdf(&path, 46);
    let font = crate::test_fixture("fonts/NotoSans-Regular.ttf");
    let mut reader = Reader::open(&fixture.root, &fixture.state, vec![font]).unwrap();
    reader.open_path(&path).unwrap();
    reader.follow_fixed(10, None).unwrap();
    let history = reader.back.clone();
    reader.show(Screen::BookMap);
    let map_window = reader.map_window;
    let cell = crate::app::fixed_book_map::projected_cell(reader.navigation_layout(), 14);
    tap(&mut reader, cell);
    let mut portrait = adopt(&mut reader, ReadingViewport::PORTRAIT, 1);
    let origin = reader.current_history_location().unwrap().position.page;

    // Starts inside the top eighth and ends below it.
    assert_eq!(
        swipe(&mut reader, (950.0, 209.0), (-230.0, 28.0)),
        Change::Partial
    );
    assert_eq!(
        reader.overview.as_ref().unwrap().grid,
        Some(PageBrowserGrid::new(3, 2))
    );
    assert_eq!(reader.overview.as_ref().unwrap().focus.page, 14);
    capture(
        &reader,
        &mut portrait,
        "page-browser-edge-swipes-portrait.png",
        false,
    );

    let mut landscape = adopt(&mut reader, ReadingViewport::LANDSCAPE, 2);
    assert_eq!(
        swipe(&mut reader, (660.0, 157.0), (230.0, 26.0)),
        Change::Partial
    );
    assert_eq!(
        reader.overview.as_ref().unwrap().grid,
        Some(PageBrowserGrid::new(2, 2))
    );
    assert_eq!(
        swipe(&mut reader, (900.0, 157.0), (-220.0, 0.0)),
        Change::Partial
    );
    // Starts inside the physical left eighth and ends to its right.
    assert_eq!(
        swipe(&mut reader, (209.0, 660.0), (65.0, -240.0)),
        Change::Partial
    );
    let view = reader.overview.as_ref().unwrap();
    let grid = Some(PageBrowserGrid::new(3, 3));
    assert_eq!(view.grid, grid);
    assert_eq!(view.focus.page, 14);
    assert_eq!(view.map_origin, Some(MapOrigin::Fixed(map_window)));
    assert_eq!(view.tiles.len(), 9);
    capture(
        &reader,
        &mut landscape,
        "page-browser-edge-swipes-landscape.png",
        false,
    );

    let first = reader.overview.as_ref().unwrap().first;
    // Exact eighth is outside the edge. A diagonal does not resize the grid.
    assert_eq!(
        swipe(&mut reader, (210.0, 660.0), (-40.0, -240.0)),
        Change::Partial
    );
    assert_eq!(reader.overview.as_ref().unwrap().first, first + 3);
    assert_eq!(
        swipe(&mut reader, (900.0, 158.0), (-230.0, 0.0)),
        Change::Partial
    );
    assert_eq!(reader.overview.as_ref().unwrap().first, first + 12);
    assert_eq!(
        swipe(&mut reader, (900.0, 80.0), (-220.0, 180.0)),
        Change::Partial
    );
    assert_eq!(reader.overview.as_ref().unwrap().first, first + 21);
    assert_eq!(reader.overview.as_ref().unwrap().grid, grid);

    open_menu(&mut reader);
    assert_eq!(
        swipe(&mut reader, (900.0, 120.0), (-230.0, 0.0)),
        Change::None
    );
    assert_eq!(reader.overview.as_ref().unwrap().grid, grid);
    close_menu(&mut reader);
    assert_eq!(reader.input(Input::PageBack), Change::Partial);
    assert_eq!(reader.overview.as_ref().unwrap().first, first + 12);
    assert_eq!(reader.input(Input::PageForward), Change::Partial);
    assert_eq!(reader.overview.as_ref().unwrap().first, first + 21);
    assert_eq!(
        reader.current_history_location().unwrap().position.page,
        origin
    );
    assert_eq!(reader.back, history);
    let back = reader.navigation_layout().back();
    tap(&mut reader, back);
    assert_eq!(reader.screen, Screen::BookMap);
    assert_eq!(reader.map_window, map_window);

    let id = reader.open_book_id().unwrap();
    let reopened = Reader::open(&fixture.root, &fixture.state, Vec::new()).unwrap();
    assert_eq!(reopened.library.page_browser_grid(id), grid);
}

#[test]
fn page_browser_grid_edge_swipes_keep_rtl_book_focus_and_obey_bounds() {
    let fixture = Fixture::new();
    let path = fixture.root.join("Harbor Notebook.epub");
    fs::copy(crate::test_fixture("rtl-notebook.epub"), &path).unwrap();
    let fonts = [
        "fonts/NotoSans-Regular.ttf",
        "fonts/NotoSansArabic-Regular.ttf",
    ]
    .into_iter()
    .map(crate::test_fixture)
    .collect();
    let mut reader = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
    reader.open_path(&path).unwrap();
    assert!(
        reader
            .active
            .as_ref()
            .unwrap()
            .reading
            .probe
            .pages_right_to_left()
    );
    reader.open_overview().unwrap();
    let focus = reader.overview.as_ref().unwrap().focus.anchor.clone();
    let origin = reader.current_history_location().unwrap().position.anchor;
    assert!(focus.is_some());
    assert_eq!(
        swipe(&mut reader, (900.0, 130.0), (-230.0, 0.0)),
        Change::Partial
    );
    assert_eq!(
        reader.overview.as_ref().unwrap().grid,
        Some(PageBrowserGrid::new(3, 2))
    );
    adopt(&mut reader, ReadingViewport::LANDSCAPE, 1);
    assert_eq!(
        swipe(&mut reader, (130.0, 670.0), (0.0, 230.0)),
        Change::Partial
    );
    assert_eq!(
        swipe(&mut reader, (130.0, 670.0), (0.0, 230.0)),
        Change::None
    );
    let view = reader.overview.as_ref().unwrap();
    assert_eq!(view.grid, Some(PageBrowserGrid::new(3, 1)));
    assert_eq!(view.focus.anchor, focus);
    adopt(&mut reader, ReadingViewport::PORTRAIT, 2);
    assert_eq!(reader.overview.as_ref().unwrap().focus.anchor, focus);
    assert_eq!(
        reader.current_history_location().unwrap().position.anchor,
        origin
    );
}
