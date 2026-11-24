//! Hold a prepared reading view until the runtime verifies its presentation.

use super::{
    Reader, ReadingPosition, Screen, bookmark_browser,
    footnote_popup::Footnote,
    list_density::Geometry,
    navigation_layout::{NavigationLayout, ReadingListStage},
    selection_html::Inspector,
};
use crate::Result;
use crate::fixed_session::PreparedFixedViewport;
use crate::session::{Reading, ReadingViewport};
use primitive_app::App;
use primitive_ui::Canvas;
use std::ops::Range;

enum Candidate {
    Current,
    Library(ReadingViewport),
    Reflow(Box<Reading>),
    Fixed(Box<PreparedFixedViewport>),
}

pub(super) struct Stage {
    book: Option<u64>,
    position: Option<ReadingPosition>,
    from: ReadingViewport,
    target: ReadingViewport,
    candidate: Candidate,
    screen: Screen,
    illustration: Option<Box<PreparedFixedViewport>>,
    footnote: Option<Footnote>,
    selection_html: Option<Inspector>,
    text_flow: Option<crate::text_flow::ViewportStage>,
    marks: Option<(super::saved_marks_view::FlexibleMarks, usize)>,
    visual_navigation: super::visual_navigation::VisualNavigation,
    matches: Option<super::search_layout::MatchStage>,
    bookmark_page: Option<(usize, usize)>,
    reading_list: Option<ReadingListStage>,
    bookmark_text_pages: Option<Vec<Range<usize>>>,
}

impl Reader {
    pub(super) fn prepare_reading_viewport(&self, target: ReadingViewport) -> Result<Stage> {
        let from = self.reading_viewport();
        if target != from && self.screen == Screen::FixedCrop {
            return Err("Finish the crop preview before changing the screen size".into());
        }
        let candidate = if target == from {
            Candidate::Current
        } else if let Some(book) = &self.active {
            Candidate::Reflow(Box::new(book.reading.prepare_viewport(target)?))
        } else if let Some(book) = &self.fixed {
            Candidate::Fixed(Box::new(book.reading.prepare_viewport(target)?))
        } else {
            Candidate::Library(target)
        };
        let bookmark_page = if target != from {
            self.bookmark_list_anchor().map(|ordinal| {
                let (width, height) = target.dimensions();
                let new_rows = Geometry::STANDARD
                    .in_navigation(NavigationLayout {
                        width: width as i32,
                        height: height as i32,
                    })
                    .rows;
                (ordinal / new_rows, ordinal)
            })
        } else {
            None
        };
        let text_flow = if target != from {
            self.text_flow
                .as_ref()
                .map(|flow| {
                    let document = &self.fixed.as_ref().ok_or("The document closed")?.reading;
                    flow.prepare_viewport(document, target)
                })
                .transpose()?
        } else {
            None
        };
        Ok(Stage {
            book: self.open_book_id(),
            position: self.current_position(),
            from,
            target,
            candidate,
            screen: self.screen,
            illustration: self
                .illustration
                .as_ref()
                .filter(|view| view.viewport() != target)
                .map(|view| view.prepare_viewport(target).map(Box::new))
                .transpose()?,
            footnote: self.resize_footnote(target)?,
            selection_html: self
                .selection_html
                .as_ref()
                .and_then(|inspector| inspector.resized(target)),
            text_flow,
            marks: None,
            visual_navigation: super::visual_navigation::VisualNavigation::capture(self),
            matches: (target != from && self.screen == Screen::Matches)
                .then(|| super::search_layout::MatchStage::capture(self)),
            bookmark_page,
            reading_list: (target != from)
                .then(|| ReadingListStage::capture(self))
                .flatten(),
            bookmark_text_pages: None,
        })
    }

    pub(super) fn draw_prepared_reading_viewport(
        &mut self,
        staged: &mut Stage,
        canvas: &mut Canvas,
    ) -> Result<()> {
        staged.check_origin(self)?;
        let (width, height) = staged.target.dimensions();
        if canvas.bounds().w != width as i32 || canvas.bounds().h != height as i32 {
            return Err("The staged canvas has different reading dimensions".into());
        }
        self.exchange_staged_reading(&mut staged.candidate)?;
        if let Err(error) = staged.exchange_illustration(self) {
            self.exchange_staged_reading(&mut staged.candidate)?;
            return Err(error);
        }
        staged.exchange_reading_tools(self);
        let previous_page = staged
            .bookmark_page
            .map(|(page, _)| std::mem::replace(&mut self.list_page, page));
        let previous_text_pages = (staged.from != staged.target
            && matches!(
                self.screen,
                Screen::BookmarkBrowser(
                    bookmark_browser::Page::Detail | bookmark_browser::Page::Description
                )
            ))
        .then(|| self.bookmark_browser.replace_text_pages(Vec::new()));
        let drawn = self.draw_staged_content(staged, canvas);
        if let Some(previous_page) = previous_page {
            self.list_page = previous_page;
        }
        staged.bookmark_text_pages =
            previous_text_pages.map(|pages| self.bookmark_browser.replace_text_pages(pages));
        staged.exchange_reading_tools(self);
        let illustration_restored = staged.exchange_illustration(self);
        let restored = self.exchange_staged_reading(&mut staged.candidate);
        drawn.and(illustration_restored).and(restored)
    }

    fn draw_staged_content(&mut self, staged: &mut Stage, canvas: &mut Canvas) -> Result<()> {
        if let Some(matches) = &mut staged.matches {
            matches.draw(self, canvas);
            return Ok(());
        }
        if let Some(list) = &mut staged.reading_list {
            list.draw(self, canvas);
            return Ok(());
        }
        if staged.from != staged.target && staged.visual_navigation.draw(self, canvas)? {
            return Ok(());
        }
        if staged.from != staged.target
            && let Some((marks, page)) = self.resize_flexible_marks()?
        {
            let previous_marks = self.flexible_marks.replace(marks);
            let previous_page = std::mem::replace(&mut self.list_page, page);
            self.draw(canvas);
            staged.marks = self.flexible_marks.take().map(|view| (view, page));
            self.flexible_marks = previous_marks;
            self.list_page = previous_page;
        } else {
            self.draw(canvas);
        }
        Ok(())
    }

    pub(super) fn commit_reading_viewport(&mut self, mut staged: Stage) -> Result<()> {
        staged.check_origin(self)?;
        match std::mem::replace(&mut staged.candidate, Candidate::Current) {
            Candidate::Current => return Ok(()),
            Candidate::Library(viewport) => {
                self.library_viewport = viewport;
            }
            Candidate::Reflow(reading) => {
                self.active.as_mut().ok_or("The book closed")?.reading = *reading;
            }
            Candidate::Fixed(prepared) => {
                self.fixed
                    .as_mut()
                    .ok_or("The document closed")?
                    .reading
                    .commit_viewport(*prepared)?;
            }
        }
        staged.exchange_text_flow(self);
        if let Some(prepared) = staged.illustration {
            self.illustration
                .as_mut()
                .ok_or("The image closed")?
                .commit_viewport(*prepared)?;
        }
        if let Some(footnote) = staged.footnote {
            self.footnote = Some(footnote);
        }
        if let Some(selection_html) = staged.selection_html {
            self.selection_html = Some(selection_html);
        }
        if let Some((marks, page)) = staged.marks {
            self.flexible_marks = Some(marks);
            self.list_page = page;
        }
        staged.visual_navigation.commit(self);
        if let Some(matches) = staged.matches {
            matches.commit(self);
        }
        if let Some((page, ordinal)) = staged.bookmark_page {
            self.set_bookmark_list_anchor(page, ordinal);
        }
        if let Some(list) = staged.reading_list {
            list.commit(self);
        }
        if let Some(pages) = staged.bookmark_text_pages {
            self.bookmark_browser.replace_text_pages(pages);
        }
        if staged.book.is_none() {
            return Ok(());
        }
        self.screen_page_worker.cancel();
        self.pages_since_refresh = 0;
        self.record_position();
        Ok(())
    }

    fn exchange_staged_reading(&mut self, candidate: &mut Candidate) -> Result<()> {
        match candidate {
            Candidate::Current => Ok(()),
            Candidate::Library(viewport) => {
                std::mem::swap(&mut self.library_viewport, viewport);
                Ok(())
            }
            Candidate::Reflow(reading) => {
                let active = self.active.as_mut().ok_or("The book closed")?;
                std::mem::swap(&mut active.reading, reading.as_mut());
                Ok(())
            }
            Candidate::Fixed(prepared) => self
                .fixed
                .as_mut()
                .ok_or("The document closed")?
                .reading
                .exchange_prepared_viewport(prepared),
        }
    }
}

impl Stage {
    fn exchange_illustration(&mut self, reader: &mut Reader) -> Result<()> {
        if let Some(prepared) = &mut self.illustration {
            reader
                .illustration
                .as_mut()
                .ok_or("The image closed")?
                .exchange_prepared_viewport(prepared)?;
        }
        Ok(())
    }

    fn exchange_reading_tools(&mut self, reader: &mut Reader) {
        self.exchange_text_flow(reader);
        if let (Some(current), Some(prepared)) = (&mut reader.footnote, &mut self.footnote) {
            std::mem::swap(current, prepared);
        }
        if let (Some(current), Some(prepared)) =
            (&mut reader.selection_html, &mut self.selection_html)
        {
            std::mem::swap(current, prepared);
        }
    }

    fn exchange_text_flow(&mut self, reader: &mut Reader) {
        if let (Some(current), Some(prepared)) = (&mut reader.text_flow, &mut self.text_flow) {
            prepared.exchange(current);
        }
    }

    fn check_origin(&self, reader: &Reader) -> Result<()> {
        if reader.open_book_id() != self.book
            || reader.screen != self.screen
            || reader.current_position() != self.position
            || reader.reading_viewport() != self.from
            || self.illustration.as_ref().is_some_and(|prepared| {
                !reader
                    .illustration
                    .as_ref()
                    .is_some_and(|view| view.owns_prepared_viewport(prepared))
            })
            || (self.from != self.target
                && match (&self.text_flow, &reader.text_flow) {
                    (Some(stage), Some(flow)) => !stage.matches(flow),
                    (None, None) => false,
                    _ => true,
                })
            || !self.visual_navigation.matches(reader)
            || self
                .reading_list
                .as_ref()
                .is_some_and(|list| !list.matches(reader))
            || self
                .matches
                .as_ref()
                .is_some_and(|matches| !matches.matches(reader))
        {
            return Err("The document changed before the viewport was presented".into());
        }
        Ok(())
    }
}

#[cfg(test)]
pub(super) fn draw_test_candidate(
    reader: &mut Reader,
    generation: u64,
    turn: primitive_app::viewport::LogicalTurn,
    screenshot: &str,
    dark: bool,
) -> primitive_app::viewport::Viewport {
    use primitive_app::viewport::{LogicalTurn, Viewport};
    let dimensions = match turn {
        LogicalTurn::Upright | LogicalTurn::UpsideDown => ReadingViewport::PORTRAIT.dimensions(),
        LogicalTurn::Clockwise | LogicalTurn::Counterclockwise => {
            ReadingViewport::LANDSCAPE.dimensions()
        }
    };
    let candidate = Viewport {
        generation,
        turn,
        logical_size: dimensions,
    };
    let paths: Vec<_> = reader
        .fonts
        .iter()
        .map(|p| p.display().to_string())
        .collect();
    let mut canvas = Canvas::new_viewport(dimensions.0, dimensions.1, 1264, &paths).unwrap();
    reader.stage_viewport(candidate).unwrap();
    canvas.begin();
    reader.draw_staged_viewport(&mut canvas).unwrap();
    let pixels = canvas.finish(dark);
    assert_eq!(pixels.len(), (dimensions.0 * dimensions.1) as usize);
    assert!(pixels.iter().any(|&ink| ink < 64));
    assert!(pixels.iter().any(|&ink| ink > 230));
    if let Some(folder) = std::env::var_os("READER_LANDSCAPE_TOOLS_CAPTURES")
        && !screenshot.is_empty()
    {
        let folder = std::path::PathBuf::from(folder);
        std::fs::create_dir_all(&folder).unwrap();
        primitive_app::write_png(
            &folder.join(format!("{screenshot}.png")),
            dimensions.0,
            dimensions.1,
            &pixels,
        )
        .unwrap();
    }
    candidate
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::selection_viewport_tests::{adopt, canvas, capture, press, tap};
    use crate::app::tests::Fixture;
    use crate::app::{Action, ReadingOrientation};
    use primitive_app::Change;
    use primitive_app::viewport::{LogicalTurn, Viewport, ViewportDecision};
    use primitive_ui::{Input, Rect};

    #[test]
    fn scan_text_viewport_stages_rolls_back_and_uses_landscape_controls() {
        let portrait = ReadingViewport::PORTRAIT;
        let landscape = ReadingViewport::LANDSCAPE;
        for (file, scan, label) in [
            ("vector.pdf", false, "read-text"),
            ("raster.pdf", true, "reflow-scan"),
        ] {
            let action = || {
                if scan {
                    Action::ReflowScan
                } else {
                    Action::ReadFixedText
                }
            };
            let fixture = Fixture::new();
            let source = fixture.root.join("Harbor survey.pdf");
            std::fs::copy(crate::test_fixture(&format!("scan-reflow/{file}")), &source).unwrap();
            let fonts = vec![
                crate::test_fixture("fonts/NotoSans-Regular.ttf"),
                crate::test_fixture("fonts/NotoSans-Bold.ttf"),
            ];
            let mut reader = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
            reader.open_path(&source).unwrap();
            reader.jump_fixed(1, None).unwrap();
            if scan {
                let document = &mut reader.fixed.as_mut().unwrap().reading;
                assert!(document.page_text(1).unwrap().words.is_empty());
                let ocr = crate::ocr::parse_tsv(
                    include_bytes!("../../tests/fixtures/scan-reflow/page-2-ocr.tsv"),
                    1350,
                    1800,
                )
                .unwrap();
                document.remember_ocr(1, "eng", ocr.text).unwrap();
            }
            let mut portrait_canvas = adopt(&mut reader, portrait, 1);
            reader.activate(action()).unwrap();
            assert_eq!(reader.screen, Screen::ReadFixedText);
            let position = reader.current_position();
            let history_len = reader.back.len();
            let first = primitive_app::frame(&reader, &mut portrait_canvas, false);
            let turn = |reader: &mut Reader, input: Input| {
                let changed = reader.input(input);
                if let Some(due) = reader.text_flow.as_ref().unwrap().deadline() {
                    assert_eq!(reader.tick(due), Change::Partial);
                } else {
                    assert_eq!(changed, Change::Partial);
                }
            };
            turn(&mut reader, Input::PageForward);
            let before = primitive_app::frame(&reader, &mut portrait_canvas, false);
            assert_ne!(first, before);

            let candidate = Viewport {
                generation: 2,
                turn: LogicalTurn::Clockwise,
                logical_size: landscape.dimensions(),
            };
            let mut wide = canvas(&reader, landscape);
            reader.stage_viewport(candidate).unwrap();
            wide.begin();
            reader.draw_staged_viewport(&mut wide).unwrap();
            let staged = wide.finish(false);
            assert_eq!(staged.len(), 1680 * 1264);
            assert_eq!(reader.reading_viewport(), portrait);
            assert_eq!(reader.current_position(), position);
            assert_eq!(
                primitive_app::frame(&reader, &mut portrait_canvas, false),
                before
            );
            reader.finish_viewport(candidate, ViewportDecision::Abort);
            assert_eq!(reader.reading_viewport(), portrait);
            assert_eq!(
                primitive_app::frame(&reader, &mut portrait_canvas, false),
                before
            );

            let candidate = Viewport {
                generation: 3,
                ..candidate
            };
            reader.stage_viewport(candidate).unwrap();
            let portrait_spacing = Rect::new(622, 1611, 192, 60);
            tap(&mut reader, portrait_spacing);
            wide.begin();
            assert!(reader.draw_staged_viewport(&mut wide).is_err());
            reader.finish_viewport(candidate, ViewportDecision::Abort);
            assert_eq!(reader.input(Input::PageBack), Change::Partial);

            reader
                .set_reading_orientation(ReadingOrientation::LandscapeRight)
                .unwrap();
            assert_eq!(
                reader.viewport_request(),
                Some(primitive_app::viewport::ViewportRequest::Fixed(
                    LogicalTurn::Clockwise
                ))
            );
            let mut wide = adopt(&mut reader, landscape, 4);
            assert_eq!(primitive_app::frame(&reader, &mut wide, false), staged);
            assert_eq!(reader.current_position(), position);
            assert_eq!(reader.back.len(), history_len);
            capture(&reader, &mut wide, &format!("landscape-{label}.png"), false);

            let spacing = Rect::new(1038, 1195, 192, 60);
            press(&reader, &mut wide, spacing, false);
            tap(&mut reader, spacing);
            let large = Rect::new(418, 1112, 176, 60);
            tap(&mut reader, large);
            capture(
                &reader,
                &mut wide,
                &format!("landscape-{label}-spacing.png"),
                true,
            );
            press(&reader, &mut wide, Rect::new(1256, 1112, 178, 60), false);
            tap(&mut reader, Rect::new(1256, 1112, 178, 60));
            assert_eq!(primitive_app::frame(&reader, &mut wide, false), staged);

            if scan {
                tap(&mut reader, Rect::new(1390, 1195, 120, 60));
                assert_ne!(primitive_app::frame(&reader, &mut wide, false), staged);
                tap(&mut reader, Rect::new(1256, 1195, 120, 60));
                assert_eq!(primitive_app::frame(&reader, &mut wide, false), staged);
            }
            let mut portrait_canvas = adopt(&mut reader, portrait, 5);
            assert_eq!(
                primitive_app::frame(&reader, &mut portrait_canvas, false),
                before
            );
            assert_eq!(reader.current_position(), position);
            assert_eq!(reader.back.len(), history_len);

            let mut wide = adopt(&mut reader, landscape, 6);
            assert_eq!(reader.input(Input::Tap(950.0, 600.0)), Change::None);
            assert!(reader.text_flow.as_ref().unwrap().deadline().is_none());
            turn(&mut reader, Input::Tap(1550.0, 600.0));
            assert_ne!(primitive_app::frame(&reader, &mut wide, false), staged);
            turn(&mut reader, Input::PageBack);
            assert_eq!(primitive_app::frame(&reader, &mut wide, false), staged);
            press(&reader, &mut wide, landscape.done(), false);
            tap(&mut reader, landscape.done());
            assert_eq!(reader.screen, Screen::Fixed);
            assert_eq!(reader.current_position(), position);
            reader.activate(action()).unwrap();
            press(&reader, &mut wide, landscape.done(), false);
        }
    }

    #[test]
    fn viewport_hooks_prepare_and_draw_before_adopting_reflow_and_fixed_frames() {
        for fixed in [false, true] {
            let fixture = Fixture::new();
            let pdf = fixture.root.join("Tilted Scan.pdf");
            std::fs::copy(crate::test_fixture("tilted-scan.pdf"), &pdf).unwrap();
            let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
            let mut reader = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
            reader
                .open_path(if fixed { &pdf } else { &fixture.book })
                .unwrap();
            let original = reader.current_history_location().unwrap();
            let original_dimensions = reader.reading_viewport();
            let paths: Vec<_> = fonts
                .iter()
                .map(|font| font.display().to_string())
                .collect();
            let target = Viewport {
                generation: 2,
                turn: LogicalTurn::Clockwise,
                logical_size: ReadingViewport::LANDSCAPE.dimensions(),
            };
            let (width, height) = target.logical_size;
            let mut staged = Canvas::new_viewport(width, height, 1264, &paths).unwrap();
            reader.stage_viewport(target).unwrap();
            staged.begin();
            reader.draw_staged_viewport(&mut staged).unwrap();
            assert_eq!(reader.current_history_location().as_ref(), Some(&original));
            assert_eq!(reader.reading_viewport(), original_dimensions);
            let frame = staged.finish(false);
            assert_eq!(frame.len(), (width * height) as usize);
            assert!(frame.iter().any(|ink| *ink < 64));
            reader.finish_viewport(target, ViewportDecision::Abort);
            assert_eq!(reader.reading_viewport(), original_dimensions);
            assert!(reader.staged_viewport.is_none());

            let target = Viewport {
                generation: 3,
                ..target
            };
            reader.stage_viewport(target).unwrap();
            let mut wrong = Canvas::new(1264, 1680, &paths).unwrap();
            assert!(reader.draw_staged_viewport(&mut wrong).is_err());
            assert_eq!(reader.reading_viewport(), original_dimensions);
            staged.begin();
            reader.draw_staged_viewport(&mut staged).unwrap();
            reader.finish_viewport(target, ViewportDecision::Commit);
            assert_eq!(reader.reading_viewport(), ReadingViewport::LANDSCAPE);
            assert_eq!(
                reader.current_position().unwrap().chapter,
                original.position.chapter
            );
            assert!(reader.staged_viewport.is_none());
        }
    }
}
