//! Screen orientation is independent of fixed-page rotation and reading position.
use super::{Canvas, Change, Input, Reader, ReadingControls, ReadingOrientation, Rect, Screen};
use crate::Result;
use primitive_app::viewport::{LogicalTurn, ViewportRequest};
use primitive_ui::{Sheet, SheetAction};

#[derive(Clone, Copy)]
enum Format {
    Reflow,
    Fixed,
}

#[derive(Clone, Copy)]
pub(super) struct DefaultPrompt {
    format: Format,
    orientation: ReadingOrientation,
}

pub(super) struct PendingOrientation {
    book: Option<u64>,
    orientation: ReadingOrientation,
}

impl ReadingOrientation {
    pub(super) fn viewport_policy(self) -> ViewportRequest {
        match self {
            Self::FollowLauncher => ViewportRequest::FollowLauncher,
            Self::Upright => ViewportRequest::Fixed(LogicalTurn::Upright),
            Self::UpsideDown => ViewportRequest::Fixed(LogicalTurn::UpsideDown),
            Self::LandscapeLeft => ViewportRequest::Fixed(LogicalTurn::Counterclockwise),
            Self::LandscapeRight => ViewportRequest::Fixed(LogicalTurn::Clockwise),
        }
    }
}

impl Reader {
    fn orientation_format(&self) -> Option<Format> {
        if self.fixed.is_some() {
            Some(Format::Fixed)
        } else {
            self.active.as_ref().map(|_| Format::Reflow)
        }
    }

    pub(super) fn default_reading_orientation(&self) -> ReadingOrientation {
        match self.orientation_format() {
            Some(Format::Reflow) => self.controls.reflow_orientation_default,
            Some(Format::Fixed) => self.controls.fixed_orientation_default,
            None => None,
        }
        .unwrap_or(self.controls.orientation)
    }

    pub(super) fn reading_orientation(&self) -> ReadingOrientation {
        self.controls
            .sticky_orientation
            .or_else(|| self.library.state(self.open_book_id()?)?.screen_orientation)
            .unwrap_or_else(|| self.default_reading_orientation())
    }

    pub(super) fn set_reading_orientation(
        &mut self,
        orientation: ReadingOrientation,
    ) -> Result<()> {
        let before = self.reading_orientation();
        if orientation.is_landscape() && !self.landscape_choices_available() {
            return Err("Landscape needs a compatible Launcher and reading view".into());
        }
        if self.accepted_viewport.is_some() {
            if before != orientation || self.pending_orientation.is_some() {
                self.notice = None;
                self.pending_orientation = Some(PendingOrientation {
                    book: self.open_book_id(),
                    orientation,
                });
            }
            return Ok(());
        }
        self.persist_reading_orientation(orientation)?;
        self.refresh_display |= self.reading_orientation() != before;
        Ok(())
    }

    fn persist_reading_orientation(&mut self, orientation: ReadingOrientation) -> Result<()> {
        let mut controls = self.controls;
        if controls.sticky_orientation.is_some() {
            controls.sticky_orientation = Some(orientation);
        } else if self.open_book_id().is_none() {
            controls.orientation = orientation;
        }
        if controls != self.controls {
            controls.save(&self.controls_path)?;
            self.controls = controls;
        }
        self.remember_book_orientation(orientation);
        Ok(())
    }

    fn pending_reading_orientation(&self) -> Option<ReadingOrientation> {
        self.pending_orientation
            .as_ref()
            .filter(|pending| pending.book == self.open_book_id())
            .map(|pending| pending.orientation)
    }

    pub(super) fn has_pending_orientation(&self) -> bool {
        self.pending_reading_orientation().is_some()
    }

    pub(super) fn requested_reading_orientation(&self) -> ReadingOrientation {
        let desired = self
            .pending_reading_orientation()
            .unwrap_or_else(|| self.reading_orientation());
        if desired.is_landscape() && !self.supports_landscape_scene() {
            ReadingOrientation::FollowLauncher
        } else {
            desired
        }
    }

    pub(super) fn accept_pending_orientation(&mut self, turn: LogicalTurn) {
        let pending = self.pending_reading_orientation();
        self.pending_orientation = None;
        if let Some(orientation) = pending {
            let matches = match orientation.viewport_policy() {
                ViewportRequest::FollowLauncher => {
                    matches!(turn, LogicalTurn::Upright | LogicalTurn::UpsideDown)
                }
                ViewportRequest::Fixed(requested) => requested == turn,
            };
            if matches && let Err(error) = self.persist_reading_orientation(orientation) {
                self.notice = Some(error.to_string());
            }
        }
    }

    pub(super) fn reject_pending_orientation(&mut self) {
        if self.has_pending_orientation() {
            self.notice =
                Some("The screen could not turn. The previous setting is still active.".into());
        }
        self.pending_orientation = None;
    }

    fn supports_landscape_scene(&self) -> bool {
        if matches!(
            self.screen,
            Screen::Library | Screen::LibraryFind | Screen::BookmarkBrowser(_)
        ) {
            return true;
        }
        (matches!(
            self.screen,
            Screen::Book
                | Screen::BookTools
                | Screen::Fixed
                | Screen::FixedTools
                | Screen::Skim
                | Screen::Definition
                | Screen::OnlineLookup
                | Screen::OnlineEdit
                | Screen::Footnote
                | Screen::SelectionHtml
                | Screen::Hyphenate
        ) || self.screen.uses_navigation_viewport())
            && self.open_book_id().is_some()
    }

    pub(super) fn landscape_choices_available(&self) -> bool {
        self.accepted_viewport.is_some() && self.supports_landscape_scene()
    }

    pub(super) fn next_reading_orientation(&self) -> ReadingOrientation {
        let current = self.reading_orientation();
        if self.landscape_choices_available() {
            match current {
                ReadingOrientation::UpsideDown => return ReadingOrientation::LandscapeLeft,
                ReadingOrientation::LandscapeLeft => return ReadingOrientation::LandscapeRight,
                _ => {}
            }
        }
        current.next()
    }

    fn remember_book_orientation(&mut self, orientation: ReadingOrientation) {
        if let Some(id) = self.open_book_id()
            && let Some(state) = self.library.state_mut(id)
            && state.screen_orientation != Some(orientation)
        {
            state.screen_orientation = Some(orientation);
            self.mark_book_changed();
        }
    }

    pub(super) fn keep_reading_orientation(&mut self, keep: bool) -> Result<()> {
        if self.controls.sticky_orientation.is_some() == keep {
            return Ok(());
        }
        let orientation = self.reading_orientation();
        let controls = ReadingControls {
            sticky_orientation: keep.then_some(orientation),
            ..self.controls
        };
        controls.save(&self.controls_path)?;
        self.controls = controls;
        if !keep {
            // Unlocking leaves the current reading view in the turn it already uses.
            self.remember_book_orientation(orientation);
        }
        Ok(())
    }

    pub(super) fn confirm_reading_orientation_default(&mut self, orientation: ReadingOrientation) {
        if let Some(format) = self.orientation_format() {
            self.orientation_default_prompt = Some(DefaultPrompt {
                format,
                orientation,
            });
        }
    }

    fn orientation_default_sheet(&self, prompt: DefaultPrompt) -> Sheet<'static> {
        let title = match prompt.format {
            Format::Reflow => "Default book orientation",
            Format::Fixed => "Default fixed-page orientation",
        };
        let (width, height) = self.reading_viewport().dimensions();
        Sheet::centered(
            Rect::new(0, 0, width as i32, height as i32),
            title,
            prompt.orientation.label(),
            "Set as default",
        )
    }

    pub(super) fn draw_orientation_default_prompt(&self, canvas: &mut Canvas) {
        if let Some(prompt) = self.orientation_default_prompt {
            self.orientation_default_sheet(prompt).draw(canvas, None);
        }
    }

    pub(super) fn orientation_default_input(&mut self, input: Input) -> Option<Change> {
        let prompt = self.orientation_default_prompt?;
        let confirm = match input {
            Input::Sleep | Input::Wake => return None,
            Input::PageBack => false,
            Input::Tap(x, y) => match self.orientation_default_sheet(prompt).hit(x, y) {
                Some(SheetAction::Cancel) => false,
                Some(SheetAction::Confirm) => true,
                None => return Some(Change::None),
            },
            _ => return Some(Change::None),
        };
        self.orientation_default_prompt = None;
        if confirm {
            let mut controls = self.controls;
            *match prompt.format {
                Format::Reflow => &mut controls.reflow_orientation_default,
                Format::Fixed => &mut controls.fixed_orientation_default,
            } = Some(prompt.orientation);
            if let Err(error) = controls.save(&self.controls_path) {
                return Some(self.report(error));
            }
            self.controls = controls;
        }
        Some(Change::Partial)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{
        Action, BarPanel, Screen,
        tests::{Fixture, capture_test_screen},
    };
    use ReadingOrientation::{FollowLauncher, Upright, UpsideDown};
    use primitive_app::App;
    use primitive_app::viewport::{Viewport, ViewportDecision};
    use std::{fs, path::PathBuf};

    fn document(fixture: &Fixture, asset: &str, name: &str) -> PathBuf {
        let path = fixture.root.join(name);
        fs::copy(crate::test_fixture(asset), &path).unwrap();
        path
    }

    fn choose(app: &mut Reader, orientation: ReadingOrientation) {
        app.activate(Action::SetReadingOrientation(orientation))
            .unwrap();
    }

    fn present(app: &mut Reader, generation: u64, turn: LogicalTurn, decision: ViewportDecision) {
        let landscape = matches!(turn, LogicalTurn::Clockwise | LogicalTurn::Counterclockwise);
        let (width, height) = if landscape {
            (1680, 1264)
        } else {
            (1264, 1680)
        };
        let candidate = Viewport {
            generation,
            turn,
            logical_size: (width, height),
        };
        let fonts: Vec<_> = app
            .fonts
            .iter()
            .map(|font| font.display().to_string())
            .collect();
        let mut canvas = Canvas::new_viewport(width, height, 1264, &fonts).unwrap();
        app.stage_viewport(candidate).unwrap();
        canvas.begin();
        app.draw_staged_viewport(&mut canvas).unwrap();
        let pixels = canvas.finish(false);
        assert_eq!(pixels.len(), (width * height) as usize);
        app.finish_viewport(candidate, decision);
    }

    #[test]
    fn negotiated_turn_acceptance_controls_persistence_and_legacy_fallback() {
        use ReadingOrientation::{LandscapeLeft, LandscapeRight};

        let fixture = Fixture::new();
        let fonts = vec![
            crate::test_fixture("fonts/NotoSans-Regular.ttf"),
            crate::test_fixture("fonts/NotoSans-Bold.ttf"),
        ];
        let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
        app.open_path(&fixture.book).unwrap();
        let position = app.current_position().unwrap();
        assert!(!app.landscape_choices_available());
        assert!(app.set_reading_orientation(LandscapeLeft).is_err());
        assert_eq!(app.reading_orientation(), FollowLauncher);
        present(&mut app, 1, LogicalTurn::Upright, ViewportDecision::Commit);
        assert!(app.landscape_choices_available());
        orientation_controls(&mut app);
        capture_test_screen(
            &app,
            &fonts,
            "READER_NEGOTIATED_ORIENTATION_PORTRAIT",
            false,
        );

        choose(&mut app, Upright);
        assert_eq!(app.reading_orientation(), FollowLauncher);
        assert!(app.viewport_needs_stage(app.accepted_viewport.unwrap()));
        present(&mut app, 2, LogicalTurn::Upright, ViewportDecision::Commit);
        assert_eq!(app.reading_orientation(), Upright);
        assert!(!app.viewport_needs_stage(app.accepted_viewport.unwrap()));
        let left = app.reading_button_in_row("Landscape", "Left");
        assert_eq!(
            app.input(Input::Tap(
                (left.x + left.w / 2) as f32,
                (left.y + left.h / 2) as f32,
            )),
            Change::Partial
        );
        assert_eq!(
            app.viewport_request(),
            Some(ViewportRequest::Fixed(LogicalTurn::Counterclockwise))
        );
        assert_eq!(app.reading_orientation(), Upright);
        present(
            &mut app,
            3,
            LogicalTurn::Counterclockwise,
            ViewportDecision::Abort,
        );
        assert_eq!(app.reading_orientation(), Upright);
        assert_eq!(app.current_position().as_ref(), Some(&position));
        assert_eq!(app.reading_viewport().dimensions(), (1264, 1680));
        assert!(app.notice.is_some());

        choose(&mut app, LandscapeRight);
        present(
            &mut app,
            4,
            LogicalTurn::Clockwise,
            ViewportDecision::Commit,
        );
        assert_eq!(app.reading_orientation(), LandscapeRight);
        assert_eq!(app.reading_viewport().dimensions(), (1680, 1264));
        assert_eq!(app.preferred_rotation(), None);
        assert_eq!(app.current_position().unwrap().chapter, position.chapter);
        assert!(app.notice.is_none());
        if let Some(path) = std::env::var_os("READER_NEGOTIATED_ORIENTATION_LANDSCAPE") {
            let fonts: Vec<_> = fonts
                .iter()
                .map(|font| font.display().to_string())
                .collect();
            let mut canvas = Canvas::new_viewport(1680, 1264, 1264, &fonts).unwrap();
            let frame = primitive_app::frame(&app, &mut canvas, false);
            primitive_app::write_png(std::path::Path::new(&path), 1680, 1264, &frame).unwrap();
        }

        app.show(Screen::ReadingControls);
        assert_eq!(
            app.viewport_request(),
            Some(ViewportRequest::FollowLauncher)
        );
        present(&mut app, 5, LogicalTurn::Upright, ViewportDecision::Commit);
        assert_eq!(app.reading_orientation(), LandscapeRight);
        app.show(Screen::Book);
        assert_eq!(
            app.viewport_request(),
            Some(ViewportRequest::Fixed(LogicalTurn::Clockwise))
        );
        let other = document(&fixture, "tilted-scan.pdf", "Other.pdf");
        app.open_path(&other).unwrap();
        assert_ne!(app.reading_orientation(), LandscapeRight);
        app.save().unwrap();
        let mut reopened = fixture.app();
        reopened.open_path(&fixture.book).unwrap();
        assert_eq!(reopened.reading_orientation(), LandscapeRight);
        assert_eq!(reopened.preferred_rotation(), None);
        assert!(!reopened.landscape_choices_available());
    }

    #[test]
    fn a_delayed_callback_cannot_discard_the_current_viewport_proposal() {
        let fixture = Fixture::new();
        let files = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
        let mut app = Reader::open(&fixture.root, &fixture.state, files).unwrap();
        app.open_path(&fixture.book).unwrap();
        present(&mut app, 1, LogicalTurn::Upright, ViewportDecision::Commit);
        choose(&mut app, ReadingOrientation::LandscapeRight);

        let stale = Viewport {
            generation: 2,
            turn: LogicalTurn::Counterclockwise,
            logical_size: (1680, 1264),
        };
        let proposed = Viewport {
            generation: 3,
            turn: LogicalTurn::Clockwise,
            logical_size: (1680, 1264),
        };
        app.stage_viewport(proposed).unwrap();
        app.finish_viewport(stale, ViewportDecision::Abort);
        app.finish_viewport(stale, ViewportDecision::Commit);
        assert!(app.has_pending_orientation());
        assert_eq!(app.staged_viewport.as_ref().unwrap().0, proposed);
        let paths: Vec<_> = app
            .fonts
            .iter()
            .map(|font| font.display().to_string())
            .collect();
        let mut canvas = Canvas::new_viewport(1680, 1264, 1264, &paths).unwrap();
        canvas.begin();
        app.draw_staged_viewport(&mut canvas).unwrap();
        app.finish_viewport(proposed, ViewportDecision::Commit);
        assert_eq!(
            app.reading_orientation(),
            ReadingOrientation::LandscapeRight
        );
        assert_eq!(app.accepted_viewport, Some(proposed));

        choose(&mut app, ReadingOrientation::UpsideDown);
        let rejected = Viewport {
            generation: 4,
            logical_size: (640, 480),
            ..proposed
        };
        assert!(app.stage_viewport(rejected).is_err());
        app.finish_viewport(rejected, ViewportDecision::Abort);
        assert!(!app.has_pending_orientation());
        assert_eq!(
            app.reading_orientation(),
            ReadingOrientation::LandscapeRight
        );
    }

    #[test]
    fn negotiated_rotation_accepts_a_vertical_epub() {
        let fixture = Fixture::new();
        let book = document(&fixture, "harbor-vertical-viewport.epub", "Harbor.epub");
        let fonts = vec![
            crate::test_fixture("fonts/NotoSans-Regular.ttf"),
            crate::test_fixture("fonts/NotoSans-Bold.ttf"),
        ];
        let mut app = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
        app.open_path(&book).unwrap();
        assert!(app.active.as_ref().unwrap().reading.probe.is_vertical());
        let position = app.current_position().unwrap();
        present(&mut app, 1, LogicalTurn::Upright, ViewportDecision::Commit);
        assert!(app.landscape_choices_available());
        choose(&mut app, ReadingOrientation::LandscapeRight);
        assert_eq!(
            app.viewport_request(),
            Some(ViewportRequest::Fixed(LogicalTurn::Clockwise))
        );
        present(
            &mut app,
            2,
            LogicalTurn::Clockwise,
            ViewportDecision::Commit,
        );
        assert!(app.active.as_ref().unwrap().reading.probe.is_vertical());
        assert_eq!(app.reading_viewport().dimensions(), (1680, 1264));
        if let Some(path) = std::env::var_os("READER_NEGOTIATED_VERTICAL_LANDSCAPE") {
            let fonts: Vec<_> = app
                .fonts
                .iter()
                .map(|font| font.display().to_string())
                .collect();
            let mut canvas = Canvas::new_viewport(1680, 1264, 1264, &fonts).unwrap();
            let frame = primitive_app::frame(&app, &mut canvas, false);
            primitive_app::write_png(std::path::Path::new(&path), 1680, 1264, &frame).unwrap();
        }
        choose(&mut app, ReadingOrientation::Upright);
        present(&mut app, 3, LogicalTurn::Upright, ViewportDecision::Commit);
        assert!(app.active.as_ref().unwrap().reading.probe.is_vertical());
        assert_eq!(app.current_position().unwrap(), position);
    }

    #[test]
    fn a_sticky_turn_stages_a_new_book_at_the_same_accepted_angle() {
        use ReadingOrientation::{LandscapeLeft, LandscapeRight};

        let fixture = Fixture::new();
        let fonts = vec![
            crate::test_fixture("fonts/NotoSans-Regular.ttf"),
            crate::test_fixture("fonts/NotoSans-Bold.ttf"),
        ];
        let mut app = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
        app.open_path(&fixture.book).unwrap();
        present(&mut app, 1, LogicalTurn::Upright, ViewportDecision::Commit);
        choose(&mut app, LandscapeRight);
        present(
            &mut app,
            2,
            LogicalTurn::Clockwise,
            ViewportDecision::Commit,
        );
        app.keep_reading_orientation(true).unwrap();

        choose(&mut app, LandscapeLeft);
        assert!(app.has_pending_orientation());
        let other = document(&fixture, "tilted-scan.pdf", "Other.pdf");
        app.open_path(&other).unwrap();
        assert!(!app.has_pending_orientation());
        let accepted = app.accepted_viewport.unwrap();
        assert_eq!(accepted.turn, LogicalTurn::Clockwise);
        assert_eq!(
            app.viewport_request(),
            Some(ViewportRequest::Fixed(accepted.turn))
        );
        assert!(app.viewport_needs_stage(accepted));
        let initial = app.current_position().unwrap();
        present(&mut app, 3, accepted.turn, ViewportDecision::Commit);
        assert_eq!(app.reading_viewport().dimensions(), accepted.logical_size);
        assert_eq!(app.current_position().unwrap().page, initial.page);
        assert!(!app.viewport_needs_stage(app.accepted_viewport.unwrap()));
        assert_eq!(app.controls.sticky_orientation, Some(LandscapeRight));

        app.open_path(&fixture.book).unwrap();
        assert!(!app.has_pending_orientation());
        assert_eq!(app.reading_orientation(), LandscapeRight);
    }

    fn orientation_controls(app: &mut Reader) {
        let (screen, panel) = if app.fixed.is_some() {
            (Screen::FixedTools, BarPanel::ViewScreen)
        } else {
            (Screen::BookTools, BarPanel::TextScreen)
        };
        app.show(screen);
        app.reading_panel = Some(panel);
    }

    fn hold_default(app: &mut Reader, orientation: ReadingOrientation) {
        let rect = app.reading_button(orientation.label());
        assert_eq!(
            app.input(Input::LongTap(
                (rect.x + rect.w / 2) as f32,
                (rect.y + rect.h / 2) as f32
            )),
            Change::Partial
        );
        assert!(app.orientation_default_prompt.is_some());
    }

    fn confirm_default(app: &mut Reader) {
        let rect = app
            .orientation_default_sheet(app.orientation_default_prompt.unwrap())
            .button(SheetAction::Confirm);
        assert_eq!(
            app.input(Input::Tap((rect.x + 15) as f32, (rect.y + 15) as f32)),
            Change::Partial
        );
        assert!(app.orientation_default_prompt.is_none());
    }

    #[test]
    fn edition_orientation_is_independent_and_restores_the_epub_source() {
        let fixture = Fixture::new();
        let second = document(&fixture, "night-landing.epub", "Night Landing.epub");
        let mut app = fixture.app();
        app.controls.orientation = UpsideDown;
        app.controls.save(&app.controls_path).unwrap();
        app.open_path(&fixture.book).unwrap();
        assert_eq!(app.reading_orientation(), UpsideDown);
        app.activate(Action::Chapter(
            super::super::chapter_navigation::Direction::Next,
        ))
        .unwrap();
        let first_position = app.current_position().unwrap();
        assert!(first_position.anchor.is_some());
        choose(&mut app, Upright);
        assert_eq!(app.current_position(), Some(first_position.clone()));
        assert_eq!(
            app.preferred_rotation(),
            Some(primitive_app::Rotation::Upright)
        );

        app.open_path(&second).unwrap();
        assert_eq!(app.reading_orientation(), UpsideDown);
        choose(&mut app, FollowLauncher);
        assert!(app.preferred_rotation().is_none());
        app.activate(Action::Show(Screen::Library)).unwrap();
        assert_eq!(app.reading_orientation(), UpsideDown);

        let mut reopened = fixture.app();
        reopened.open_path(&fixture.book).unwrap();
        assert_eq!(reopened.reading_orientation(), Upright);
        assert_eq!(reopened.current_position(), Some(first_position));
        reopened.open_path(&second).unwrap();
        assert_eq!(reopened.reading_orientation(), FollowLauncher);
    }

    #[test]
    fn holding_a_turn_saves_independent_format_defaults_without_rotating_the_book() {
        let fixture = Fixture::new();
        let second = document(&fixture, "night-landing.epub", "Night Landing.epub");
        let pdf = document(&fixture, "pdf-password/plain.pdf", "Test Pages.pdf");
        let another_pdf = document(&fixture, "tilted-scan.pdf", "Scanned Pages.pdf");
        let fonts = vec![
            crate::test_fixture("fonts/NotoSans-Regular.ttf"),
            crate::test_fixture("fonts/NotoSans-Bold.ttf"),
        ];
        let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
        app.open_path(&fixture.book).unwrap();
        let original = app.current_position();
        orientation_controls(&mut app);
        hold_default(&mut app, UpsideDown);
        assert_eq!(app.input(Input::PageBack), Change::Partial);
        assert!(app.controls.reflow_orientation_default.is_none());
        hold_default(&mut app, UpsideDown);
        capture_test_screen(&app, &fonts, "READER_ORIENTATION_DEFAULT_FRAME", false);
        confirm_default(&mut app);
        assert_eq!(app.controls.reflow_orientation_default, Some(UpsideDown));
        assert_eq!(app.reading_orientation(), FollowLauncher);
        assert_eq!(app.current_position(), original);
        assert_eq!(app.reading_panel, Some(BarPanel::TextScreen));
        capture_test_screen(&app, &fonts, "READER_ORIENTATION_PANEL_FRAME", false);

        app.open_path(&second).unwrap();
        assert_eq!(app.reading_orientation(), UpsideDown);
        app.open_path(&pdf).unwrap();
        assert_eq!(app.reading_orientation(), FollowLauncher);
        app.activate(Action::SetFixedRotation(1)).unwrap();
        let source = app.current_position();
        orientation_controls(&mut app);
        hold_default(&mut app, Upright);
        confirm_default(&mut app);
        assert_eq!(app.controls.fixed_orientation_default, Some(Upright));
        assert_eq!(app.reading_orientation(), FollowLauncher);
        assert_eq!(app.current_position(), source);
        assert_eq!(app.fixed.as_ref().unwrap().reading.preferences().turns, 1);
        app.open_path(&another_pdf).unwrap();
        assert_eq!(app.reading_orientation(), Upright);
        app.save().unwrap();

        let mut reopened = fixture.app();
        assert_eq!(
            reopened.controls.reflow_orientation_default,
            Some(UpsideDown)
        );
        assert_eq!(reopened.controls.fixed_orientation_default, Some(Upright));
        reopened.open_path(&fixture.book).unwrap();
        assert_eq!(reopened.reading_orientation(), FollowLauncher);
        reopened.open_path(&pdf).unwrap();
        assert_eq!(reopened.reading_orientation(), FollowLauncher);
        assert_eq!(
            reopened.fixed.as_ref().unwrap().reading.preferences().turns,
            1
        );
    }

    #[test]
    fn sticky_turn_survives_library_switching_and_keeps_each_editions_choice() {
        let fixture = Fixture::new();
        let second = document(&fixture, "night-landing.epub", "Night Landing.epub");
        let mut app = fixture.app();
        app.open_path(&fixture.book).unwrap();
        let first_id = app.open_book_id().unwrap();
        choose(&mut app, Upright);
        assert_eq!(app.applied_change(), Change::Full);
        orientation_controls(&mut app);
        let on = app.reading_button("On");
        assert_eq!(
            app.input(Input::Tap((on.x + 15) as f32, (on.y + 15) as f32)),
            Change::Partial
        );
        assert_eq!(app.controls.sticky_orientation, Some(Upright));

        app.activate(Action::Show(Screen::Library)).unwrap();
        assert_eq!(app.reading_orientation(), Upright);
        app.open_path(&second).unwrap();
        let second_id = app.open_book_id().unwrap();
        let second_source = app.current_position();
        assert_eq!(app.reading_orientation(), Upright);
        assert_eq!(
            app.library.state(second_id).unwrap().screen_orientation,
            Some(FollowLauncher)
        );
        choose(&mut app, UpsideDown);
        assert_eq!(
            app.library.state(second_id).unwrap().screen_orientation,
            Some(UpsideDown)
        );
        assert_eq!(app.current_position(), second_source);
        app.open_path(&fixture.book).unwrap();
        assert_eq!(app.reading_orientation(), UpsideDown);
        assert_eq!(
            app.library.state(first_id).unwrap().screen_orientation,
            Some(Upright)
        );
        app.save().unwrap();

        let mut reopened = fixture.app();
        assert_eq!(reopened.reading_orientation(), UpsideDown);
        reopened.open_path(&fixture.book).unwrap();
        let source = reopened.current_position();
        reopened
            .activate(Action::KeepReadingOrientation(false))
            .unwrap();
        assert_eq!(reopened.reading_orientation(), UpsideDown);
        assert_eq!(reopened.current_position(), source);
        assert_eq!(
            reopened.library.state(first_id).unwrap().screen_orientation,
            Some(UpsideDown)
        );
        reopened.activate(Action::Show(Screen::Library)).unwrap();
        assert_eq!(reopened.reading_orientation(), FollowLauncher);
        reopened.open_path(&second).unwrap();
        assert_eq!(reopened.reading_orientation(), UpsideDown);
        assert_eq!(reopened.current_position(), second_source);
    }
}
