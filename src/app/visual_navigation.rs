//! Keep visual browsing positions separate from the reading position during a resize.
use primitive_app::App;
use primitive_ui::Canvas;

use super::fixed_book_map::MapWindow;
use super::page_overview::{OverviewFocus, PageOverview};
use super::{Reader, Screen};
use crate::Result;

#[derive(PartialEq, Eq)]
enum Origin {
    Other,
    Overview(Option<usize>, usize, OverviewFocus),
    FixedMap(MapWindow),
    ChapterMap { list_page: usize, chapter: usize },
}

enum Prepared {
    Overview(PageOverview),
    ChapterMap { list_page: usize, chapter: usize },
}

pub(super) struct VisualNavigation {
    origin: Origin,
    prepared: Option<Prepared>,
}

impl VisualNavigation {
    pub(super) fn capture(reader: &Reader) -> Self {
        Self {
            origin: Origin::capture(reader),
            prepared: None,
        }
    }

    pub(super) fn matches(&self, reader: &Reader) -> bool {
        self.origin == Origin::capture(reader)
    }

    pub(super) fn draw(&mut self, reader: &mut Reader, canvas: &mut Canvas) -> Result<bool> {
        self.prepared = None;
        let prepared = match &self.origin {
            Origin::Overview(Some(chapter), _, focus) => {
                let view = reader.resize_overview(*chapter, focus)?;
                let previous = reader.overview.replace(view);
                reader.draw(canvas);
                let view = reader.overview.take().expect("a staged overview was drawn");
                reader.overview = previous;
                Prepared::Overview(view)
            }
            Origin::ChapterMap { chapter, .. } => {
                let page = chapter / reader.list_geometry().rows;
                let previous = std::mem::replace(&mut reader.list_page, page);
                reader.draw(canvas);
                reader.list_page = previous;
                Prepared::ChapterMap {
                    list_page: page,
                    chapter: *chapter,
                }
            }
            _ => return Ok(false),
        };
        self.prepared = Some(prepared);
        Ok(true)
    }

    pub(super) fn commit(self, reader: &mut Reader) {
        match self.prepared {
            Some(Prepared::Overview(view)) => reader.overview = Some(view),
            Some(Prepared::ChapterMap { list_page, chapter }) => {
                reader.list_page = list_page;
                reader.book_map_focus = Some(chapter);
            }
            None => {}
        }
    }
}

impl Origin {
    fn capture(reader: &Reader) -> Self {
        match reader.screen {
            Screen::PageOverview if let Some(view) = &reader.overview => {
                Self::Overview(view.chapter, view.first, view.focus.clone())
            }
            Screen::BookMap if reader.fixed.is_some() => Self::FixedMap(reader.map_window),
            Screen::BookMap if let Some(book) = &reader.active => {
                let rows = reader.list_geometry().rows;
                let count = book.reading.probe.chapter_count();
                let first = (reader.list_page * rows).min(count.saturating_sub(1));
                let visible = first..(first + rows).min(count);
                let chapter = reader
                    .book_map_focus
                    .filter(|chapter| visible.contains(chapter))
                    .unwrap_or(first);
                Self::ChapterMap {
                    list_page: reader.list_page,
                    chapter,
                }
            }
            _ => Self::Other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::Fixture;
    use crate::app::{Action, ReadingPosition, fixed_book_map, page_overview::OverviewLayout};
    use primitive_app::{
        Change,
        viewport::{LogicalTurn, Viewport, ViewportDecision},
    };
    use primitive_ui::{Input, Rect};
    use std::{fs, path::PathBuf};

    fn app(fixture: &Fixture) -> Reader {
        Reader::open(
            &fixture.root,
            &fixture.state,
            vec![
                crate::test_fixture("fonts/NotoSans-Regular.ttf"),
                crate::test_fixture("fonts/NotoSans-Bold.ttf"),
            ],
        )
        .unwrap()
    }

    fn open(fixture: &Fixture, asset: &str) -> Reader {
        let path = fixture.root.join(asset);
        fs::copy(crate::test_fixture(asset), &path).unwrap();
        let mut reader = app(fixture);
        reader.open_path(&path).unwrap();
        reader
    }

    fn fonts(reader: &Reader) -> Vec<String> {
        reader
            .fonts
            .iter()
            .map(|font| font.display().to_string())
            .collect()
    }

    fn target(turn: LogicalTurn, generation: u64) -> Viewport {
        let logical_size = match turn {
            LogicalTurn::Clockwise | LogicalTurn::Counterclockwise => (1680, 1264),
            _ => (1264, 1680),
        };
        Viewport {
            generation,
            turn,
            logical_size,
        }
    }

    fn stage(reader: &mut Reader, viewport: Viewport, decision: ViewportDecision) -> Vec<u8> {
        let (width, height) = viewport.logical_size;
        let mut canvas = Canvas::new_viewport(width, height, 1264, &fonts(reader)).unwrap();
        reader.stage_viewport(viewport).unwrap();
        canvas.begin();
        reader.draw_staged_viewport(&mut canvas).unwrap();
        let pixels = canvas.finish(false);
        reader.finish_viewport(viewport, decision);
        assert_eq!(pixels.len(), (width * height) as usize);
        pixels
    }

    fn frame(reader: &Reader) -> Vec<u8> {
        let (width, height) = reader.reading_viewport().dimensions();
        let mut canvas = Canvas::new_viewport(width, height, 1264, &fonts(reader)).unwrap();
        primitive_app::frame(reader, &mut canvas, false)
    }

    fn capture(name: &str, size: (u32, u32), pixels: &[u8]) {
        if let Some(folder) = std::env::var_os("READER_VISUAL_NAVIGATION_CAPTURE_DIR") {
            let folder = PathBuf::from(folder);
            fs::create_dir_all(&folder).unwrap();
            primitive_app::write_png(&folder.join(name), size.0, size.1, pixels).unwrap();
        }
    }

    fn tap(rect: Rect) -> Input {
        Input::Tap((rect.x + rect.w / 2) as f32, (rect.y + rect.h / 2) as f32)
    }

    #[test]
    fn overview_resizes_an_independent_browsed_chapter_and_opens_the_wide_tile() {
        let fixture = Fixture::new();
        let mut reader = open(&fixture, "night-landing.epub");
        let reading = &mut reader.active.as_mut().unwrap().reading;
        reading
            .reflow(crate::Settings {
                logical_font_size: None,
                font_size: 70.0,
                ..reading.config.settings
            })
            .unwrap();
        let opening = reader.current_position().unwrap();
        reader.activate(Action::Overview).unwrap();
        let track = OverviewLayout::new(reader.navigation_layout()).track();
        assert_eq!(
            reader.input(Input::Tap(
                (track.right() - 1) as f32,
                (track.y + 20) as f32
            )),
            Change::Partial
        );
        assert_eq!(reader.overview.as_ref().unwrap().chapter, Some(1));
        assert_eq!(reader.input(Input::PageForward), Change::Partial);
        let overview = reader.overview.as_ref().unwrap();
        let original_first = overview.first;
        let source = overview.focus.anchor.clone().unwrap();
        assert!(overview.preview.is_some());
        let portrait = frame(&reader);
        capture("page-overview-portrait.png", (1264, 1680), &portrait);
        stage(
            &mut reader,
            target(LogicalTurn::Clockwise, 2),
            ViewportDecision::Abort,
        );
        assert_eq!(reader.reading_viewport().dimensions(), (1264, 1680));
        assert_eq!(reader.overview.as_ref().unwrap().first, original_first);
        assert_eq!(reader.current_position(), Some(opening.clone()));

        let wide = stage(
            &mut reader,
            target(LogicalTurn::Clockwise, 3),
            ViewportDecision::Commit,
        );
        capture("page-overview-landscape.png", (1680, 1264), &wide);
        assert_eq!(reader.reading_viewport().dimensions(), (1680, 1264));
        assert_eq!(frame(&reader), wide);
        let overview = reader.overview.as_ref().unwrap();
        assert_eq!(overview.chapter, Some(1));
        assert_eq!(overview.focus.anchor.as_ref(), Some(&source));
        let probe = overview.preview.as_ref().unwrap();
        assert_eq!(probe.settings.width, 1680);
        assert_eq!(probe.page_for(&source), Some(overview.focus.page));
        assert!(
            (overview.first..overview.first + overview.tiles.len()).contains(&overview.focus.page)
        );
        assert_eq!(reader.current_position().unwrap().anchor, opening.anchor);

        let returned = stage(
            &mut reader,
            target(LogicalTurn::Upright, 4),
            ViewportDecision::Commit,
        );
        assert_eq!(returned, portrait);
        assert_eq!(reader.overview.as_ref().unwrap().first, original_first);
        stage(
            &mut reader,
            target(LogicalTurn::Counterclockwise, 5),
            ViewportDecision::Commit,
        );
        let layout = OverviewLayout::new(reader.navigation_layout());
        let track = layout.track();
        let start_y = (track.y + 32) as f32;
        assert_eq!(
            reader.swipe(
                Input::Swipe {
                    dx: -1450.0,
                    dy: 0.0,
                    start_y
                },
                1530.0,
            ),
            Change::Partial
        );
        assert_eq!(reader.overview.as_ref().unwrap().chapter, Some(0));
        assert_eq!(
            reader.swipe(
                Input::Swipe {
                    dx: 1450.0,
                    dy: 0.0,
                    start_y
                },
                80.0,
            ),
            Change::Partial
        );
        let overview = reader.overview.as_ref().unwrap();
        assert_eq!(overview.chapter, Some(1));
        assert_eq!(overview.tiles.len(), 4);
        let page = overview.first + 3;
        let tile = layout.tile(3);
        assert!(tile.x + tile.w / 2 > 1264);
        assert_eq!(reader.input(tap(layout.tile(3))), Change::Partial);
        assert_eq!(reader.screen, Screen::Book);
        assert_eq!(reader.current_position().unwrap().chapter, 1);
        assert_eq!(reader.current_position().unwrap().page, page);
        reader.activate(Action::ReturnToLocation).unwrap();
        assert_eq!(reader.current_position().unwrap().anchor, opening.anchor);
    }

    #[test]
    fn chapter_map_keeps_its_visible_source_row_and_wide_touch_uses_that_list() {
        let fixture = Fixture::new();
        let mut reader = open(&fixture, "many-chapters.epub");
        let opening = reader.current_position().unwrap();
        reader.activate(Action::Show(Screen::BookMap)).unwrap();
        assert_eq!(reader.input(Input::PageForward), Change::Partial);
        assert_eq!(reader.input(Input::PageForward), Change::Partial);
        let portrait = frame(&reader);
        let source = reader.list_page * reader.list_geometry().rows;
        assert_eq!(source, 22);
        capture("chapter-map-portrait.png", (1264, 1680), &portrait);
        let wide = stage(
            &mut reader,
            target(LogicalTurn::Clockwise, 2),
            ViewportDecision::Commit,
        );
        assert_eq!(reader.book_map_focus, Some(source));
        let layout = reader.list_geometry();
        let row = layout.row(source - reader.list_page * layout.rows);
        assert!(row.right() > 1264);
        assert_eq!(reader.current_position().unwrap().anchor, opening.anchor);
        capture("chapter-map-landscape.png", (1680, 1264), &wide);
        assert_eq!(
            stage(
                &mut reader,
                target(LogicalTurn::Upright, 3),
                ViewportDecision::Commit
            ),
            portrait
        );
        assert_eq!(reader.list_page, 2);
        stage(
            &mut reader,
            target(LogicalTurn::Counterclockwise, 4),
            ViewportDecision::Commit,
        );
        reader.input(Input::Tap(1520.0, (row.y + row.h / 2) as f32));
        assert_eq!(reader.screen, Screen::Book);
        assert_eq!(reader.current_position().unwrap().chapter, source);
        reader.activate(Action::ReturnToLocation).unwrap();
        assert_eq!(reader.current_position().unwrap().anchor, opening.anchor);
    }

    #[test]
    fn fixed_map_keeps_nested_page_groups_and_follows_the_wide_cell() {
        let fixture = Fixture::new();
        let mut reader = app(&fixture);
        let pdf = fixture.root.join("Harbor Atlas.pdf");
        fs::write(
            &pdf,
            crate::fixed_session::tests::striped_pdf_sized_bytes(96, 300, 400),
        )
        .unwrap();
        reader.open_path(&pdf).unwrap();
        reader.follow_fixed(26, None).unwrap();
        reader.activate(Action::ToggleBookmark).unwrap();
        let id = reader.open_book_id().unwrap();
        let custom = &mut reader.library.state_mut(id).unwrap().custom_contents;
        custom.enabled = true;
        for (page, label) in [
            (0, "Harbor"),
            (14, "Estuary"),
            (26, "Headland"),
            (48, "Ferry"),
            (72, "Inlet"),
        ] {
            custom
                .save(
                    ReadingPosition {
                        chapter: 0,
                        page,
                        anchor: None,
                    },
                    label,
                )
                .unwrap();
        }
        reader.activate(Action::Show(Screen::BookMap)).unwrap();
        let original = reader.current_position().unwrap();
        let portrait = frame(&reader);
        capture("fixed-book-map-portrait.png", (1264, 1680), &portrait);
        let wide = stage(
            &mut reader,
            target(LogicalTurn::Clockwise, 2),
            ViewportDecision::Commit,
        );
        capture("fixed-book-map-landscape.png", (1680, 1264), &wide);
        assert_eq!(frame(&reader), wide);
        assert_eq!(reader.current_position(), Some(original.clone()));
        let root = reader.map_window;
        let eighth = fixed_book_map::projected_cell(reader.navigation_layout(), 7);
        assert!(eighth.x > 1264);
        assert_eq!(reader.input(tap(eighth)), Change::Partial);
        let nested = reader.map_window;
        assert_ne!(nested, root);
        assert_eq!(reader.input(Input::PageForward), Change::Partial);
        assert_eq!(reader.input(Input::PageBack), Change::Partial);
        assert_eq!(reader.map_window, nested);
        stage(
            &mut reader,
            target(LogicalTurn::Upright, 3),
            ViewportDecision::Commit,
        );
        assert_eq!(reader.map_window, nested);
        stage(
            &mut reader,
            target(LogicalTurn::Counterclockwise, 4),
            ViewportDecision::Commit,
        );
        let cell = fixed_book_map::projected_cell(reader.navigation_layout(), 1);
        assert_eq!(reader.input(tap(cell)), Change::Full);
        assert_eq!(reader.current_position().unwrap().page, 15);
        reader.activate(Action::ReturnToLocation).unwrap();
        assert_eq!(reader.current_position(), Some(original.clone()));

        reader.activate(Action::Overview).unwrap();
        let layout = OverviewLayout::new(reader.navigation_layout());
        let start = reader.overview.as_ref().unwrap().first;
        assert_eq!(
            reader.input(tap(reader.navigation_layout().next())),
            Change::Partial
        );
        assert_eq!(reader.overview.as_ref().unwrap().first, start + 4);
        let tile = layout.tile(3);
        let x = (tile.x + tile.w / 2) as f32;
        let y = (tile.y + tile.h / 2) as f32;
        assert!(x > 1264.0);
        assert_eq!(reader.input(Input::LongTap(x, y)), Change::Partial);
        let Some(super::super::hidden_flows::FlowPopup::Overview(actions)) = &reader.flow_popup
        else {
            panic!("the held thumbnail should open page actions");
        };
        let bookmark = actions
            .button_in(
                reader.navigation_layout(),
                super::super::overview_bookmarks::PageAction::Bookmark,
            )
            .unwrap();
        assert_eq!(bookmark.x + bookmark.w / 2, 840);
        assert!(bookmark.y + bookmark.h / 2 < 632);
        assert_eq!(reader.input(tap(bookmark)), Change::Partial);
        let state = reader.library.state(id).unwrap();
        assert!(
            state
                .bookmarks
                .iter()
                .any(|mark| mark.position.page == start + 7)
        );
        assert_eq!(reader.current_position(), Some(original));
    }
}
