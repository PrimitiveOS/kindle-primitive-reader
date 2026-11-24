//! Portrait screen orientation is independent of fixed-page rotation and reading position.
use super::{
    Canvas, Change, HEIGHT, Input, Reader, ReadingControls, ReadingOrientation, Rect, WIDTH,
};
use crate::Result;
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
        self.refresh_display |= self.reading_orientation() != before;
        Ok(())
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

    fn orientation_default_sheet(prompt: DefaultPrompt) -> Sheet<'static> {
        let title = match prompt.format {
            Format::Reflow => "Default book orientation",
            Format::Fixed => "Default fixed-page orientation",
        };
        Sheet::centered(
            Rect::new(0, 0, WIDTH as i32, HEIGHT as i32),
            title,
            prompt.orientation.label(),
            "Set as default",
        )
    }

    pub(super) fn draw_orientation_default_prompt(&self, canvas: &mut Canvas) {
        if let Some(prompt) = self.orientation_default_prompt {
            Self::orientation_default_sheet(prompt).draw(canvas, None);
        }
    }

    pub(super) fn orientation_default_input(&mut self, input: Input) -> Option<Change> {
        let prompt = self.orientation_default_prompt?;
        let confirm = match input {
            Input::Sleep | Input::Wake => return None,
            Input::PageBack => false,
            Input::Tap(x, y) => match Self::orientation_default_sheet(prompt).hit(x, y) {
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
        let rect = Reader::orientation_default_sheet(app.orientation_default_prompt.unwrap())
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
