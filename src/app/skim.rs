//! Transient page preview over the current reading view.

use primitive_app::{App, Change, HEIGHT, WIDTH};
use primitive_ui::{CAPTION, Canvas, Input, Rect, TextStyle};

use super::{DONE, HistoryLocation, Reader, ReadingPosition, Screen, fixed_page_label};
use crate::fixed_session::FixedJump;
use crate::reading_controls::SkimPosition;

pub(super) struct Layout {
    pub(super) panel: Rect,
    pub(super) track: Rect,
    previous_ten: Rect,
    previous: Rect,
    origin: Rect,
    next: Rect,
    next_ten: Rect,
    pub(super) cancel: Rect,
    pub(super) commit: Rect,
}

#[cfg(test)]
mod placement_tests {
    use super::*;
    use crate::app::{
        Action,
        tests::{Fixture, capture_test_screen},
    };

    fn tap(rect: Rect) -> Input {
        Input::Tap((rect.x + rect.w / 2) as f32, (rect.y + rect.h / 2) as f32)
    }

    #[test]
    fn skim_ten_page_buttons_preview_in_both_directions_without_recording_jumps() {
        let fixture = Fixture::new();
        let epub = fixture.root.join("Long chapter.epub");
        std::fs::copy(crate::test_fixture("long-chapter.epub"), &epub).unwrap();
        let mut app = fixture.app();
        app.open_path(&epub).unwrap();
        let initial = app.current_history_location().unwrap();
        assert!(app.active.as_ref().unwrap().reading.probe.page_count() > 10);

        app.activate(Action::Skim).unwrap();
        let layout = app.skim_layout();
        assert_eq!(app.input(tap(layout.next_ten)), Change::Partial);
        let settle = |app: &mut Reader| {
            for _ in 0..12 {
                if !app.active.as_ref().unwrap().reading.is_turning() {
                    return;
                }
                let deadline = app.deadline().unwrap();
                app.tick(deadline + std::time::Duration::from_secs(1));
            }
            assert!(!app.active.as_ref().unwrap().reading.is_turning());
        };
        settle(&mut app);
        assert_eq!(app.active.as_ref().unwrap().reading.page, 10);
        assert!(app.back.is_empty());
        assert_eq!(app.input(tap(layout.previous_ten)), Change::Partial);
        settle(&mut app);
        assert_eq!(app.current_history_location().as_ref(), Some(&initial));
        assert_eq!(app.input(tap(layout.cancel)), Change::Partial);
        assert!(app.back.is_empty());

        let pdf = fixture.root.join("Tilted scan.pdf");
        std::fs::copy(crate::test_fixture("tilted-scan.pdf"), &pdf).unwrap();
        app.open_path(&pdf).unwrap();
        app.activate(Action::Skim).unwrap();
        assert_eq!(app.input(tap(layout.next_ten)), Change::Partial);
        assert_eq!(app.current_history_location().unwrap().position.page, 1);
        assert_eq!(app.input(tap(layout.previous_ten)), Change::Partial);
        assert_eq!(app.current_history_location().unwrap().position.page, 0);
        assert!(app.back.is_empty());
    }

    #[test]
    fn skim_placement_persists_and_keeps_progress_and_actions_in_the_drawn_panel() {
        let fixture = Fixture::new();
        let pdf = fixture.root.join("Tilted Scan.pdf");
        std::fs::copy(crate::test_fixture("tilted-scan.pdf"), &pdf).unwrap();
        let fonts = vec![
            crate::test_fixture("fonts/NotoSans-Regular.ttf"),
            crate::test_fixture("fonts/NotoSans-Bold.ttf"),
        ];
        let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
        assert_eq!(app.controls.skim_position, SkimPosition::Center);
        app.open_path(&pdf).unwrap();
        let opening = app.current_history_location().unwrap();

        for (index, position, capture) in [
            (2, SkimPosition::Top, "READER_SKIM_TOP"),
            (3, SkimPosition::Center, "READER_SKIM_CENTER"),
            (4, SkimPosition::Bottom, "READER_SKIM_BOTTOM"),
        ] {
            app.show(Screen::NavigationSettings);
            assert_eq!(
                app.input(tap(app.list_geometry().row(index))),
                Change::Partial
            );
            assert_eq!(app.controls.skim_position, position);
            app.activate(Action::Skim).unwrap();
            let layout = app.skim_layout();
            match position {
                SkimPosition::Top => assert_eq!(layout.panel.y, 32),
                SkimPosition::Center => {
                    assert_eq!(2 * layout.panel.y + layout.panel.h, HEIGHT as i32)
                }
                SkimPosition::Bottom => assert_eq!(layout.panel.bottom(), HEIGHT as i32 - 76),
            }
            assert!(layout.track.bottom() < layout.origin.y);
            capture_test_screen(&app, &fonts, capture, false);
            let track_y = (layout.track.y + layout.track.h / 2) as f32;
            let from_x = (layout.track.x + 40) as f32;
            let swipe = Input::Swipe {
                dx: layout.track.w as f32,
                dy: 0.0,
                start_y: track_y,
            };
            assert_eq!(app.swipe(swipe, from_x), Change::Partial);
            assert_eq!(app.current_history_location().unwrap().position.page, 1);
            assert_eq!(app.input(tap(layout.origin)), Change::Partial);
            assert_eq!(app.current_history_location().as_ref(), Some(&opening));
            assert_eq!(app.input(tap(layout.cancel)), Change::Partial);
            assert_eq!(app.screen, Screen::Fixed);
            assert!(app.back.is_empty());
        }

        let app = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
        assert_eq!(app.controls.skim_position, SkimPosition::Bottom);
    }
}

impl Layout {
    fn new(position: SkimPosition) -> Self {
        let (height, track, preview, actions) = match position {
            SkimPosition::Center => (384, 142, 224, 300),
            SkimPosition::Top | SkimPosition::Bottom => (320, 92, 154, 232),
        };
        let top = match position {
            SkimPosition::Top => 32,
            SkimPosition::Center => (HEIGHT as i32 - height) / 2,
            SkimPosition::Bottom => HEIGHT as i32 - height - 76,
        };
        let preview_button = |index| Rect::new(80 + 224 * index, top + preview, 208, 68);
        Self {
            panel: Rect::new(40, top, WIDTH as i32 - 80, height),
            track: Rect::new(92, top + track, WIDTH as i32 - 184, 42),
            previous_ten: preview_button(0),
            previous: preview_button(1),
            origin: preview_button(2),
            next: preview_button(3),
            next_ten: preview_button(4),
            cancel: Rect::new(260, top + actions, 300, 72),
            commit: Rect::new(704, top + actions, 300, 72),
        }
    }

    fn fraction(&self, x: f32) -> f64 {
        f64::from(((x - self.track.x as f32) / self.track.w as f32).clamp(0.0, 1.0))
    }
}

pub(super) struct Session {
    origin: HistoryLocation,
    commit_when_idle: bool,
}

impl Reader {
    pub(super) fn skim_layout(&self) -> Layout {
        Layout::new(self.controls.skim_position)
    }

    pub(super) fn start_skim(&mut self) -> crate::Result<()> {
        if self.active.is_none() && self.fixed.is_none() {
            return Err("Open a document before skimming".into());
        }
        let origin = self
            .current_history_location()
            .ok_or("Open a document before skimming")?;
        self.skim = Some(Session {
            origin,
            commit_when_idle: false,
        });
        self.selection = None;
        self.pdf_selection = None;
        self.search_result = None;
        self.result_selection = None;
        self.pdf_result_regions.clear();
        self.show(Screen::Skim);
        Ok(())
    }

    fn skim_origin(&self) -> Option<HistoryLocation> {
        self.skim.as_ref().map(|skim| skim.origin.clone())
    }

    fn skim_restore_origin(&mut self) -> crate::Result<()> {
        let origin = self.skim_origin().ok_or("Skim is not open")?;
        self.skim_apply_location(&origin)?;
        if let Some(skim) = &mut self.skim {
            skim.commit_when_idle = false;
        }
        Ok(())
    }

    fn skim_apply_location(&mut self, target: &HistoryLocation) -> crate::Result<()> {
        if let Some(fixed) = &mut self.fixed {
            let scan_target = target.scan_point.map(FixedJump::LinkPoint);
            fixed.reading.jump_to(
                target
                    .position
                    .page
                    .min(fixed.reading.count().saturating_sub(1)),
                scan_target.as_ref(),
            )?;
        } else {
            let reading = &mut self.active.as_mut().ok_or("Open a book first")?.reading;
            if let Some(anchor) = &target.position.anchor {
                reading.jump_to_anchor(target.position.chapter, anchor)?;
            } else {
                reading.go_to_page(target.position.chapter, target.position.page)?;
            }
        }
        self.selection = None;
        self.pdf_selection = None;
        self.result_selection = None;
        self.search_result = None;
        self.pdf_result_regions.clear();
        self.notice = None;
        Ok(())
    }

    fn skim_close(&mut self, commit: bool) -> crate::Result<()> {
        if commit
            && (self
                .active
                .as_ref()
                .is_some_and(|book| book.reading.is_turning())
                || self
                    .fixed
                    .as_ref()
                    .is_some_and(|book| book.reading.is_turning()))
        {
            if let Some(skim) = &mut self.skim {
                skim.commit_when_idle = true;
            }
            return Ok(());
        }
        let session = self.skim.take().ok_or("Skim is not open")?;
        if commit {
            if self.current_history_location().as_ref() != Some(&session.origin) {
                self.push_history(session.origin);
            }
        } else {
            self.skim_apply_location(&session.origin)?;
        }
        self.show(if self.fixed.is_some() {
            Screen::Fixed
        } else {
            Screen::Book
        });
        self.record_position();
        Ok(())
    }

    fn skim_progress(&self) -> f64 {
        if let Some(fixed) = &self.fixed {
            let denominator = fixed.reading.count().saturating_sub(1).max(1);
            return fixed.reading.page() as f64 / denominator as f64;
        }
        let Some(reading) = self.active.as_ref().map(|book| &book.reading) else {
            return 0.0;
        };
        let weights = reading.progress_weights();
        let total = weights.last().copied().unwrap_or(0);
        if total == 0 || reading.chapter + 1 >= weights.len() {
            let chapters = reading.probe.chapter_count().saturating_sub(1).max(1);
            return reading.chapter as f64 / chapters as f64;
        }
        let chapter_start = weights[reading.chapter];
        let chapter_span = weights[reading.chapter + 1].saturating_sub(chapter_start);
        let pages = reading.probe.page_count().saturating_sub(1).max(1);
        let within = reading.page.min(pages) as f64 / pages as f64;
        ((chapter_start as f64 + chapter_span as f64 * within) / total as f64).clamp(0.0, 1.0)
    }

    fn skim_label(&self) -> String {
        if let Some(fixed) = &self.fixed {
            format!(
                "{} of {}",
                fixed_page_label(&fixed.reading, fixed.reading.page()),
                fixed.reading.count(),
            )
        } else if let Some(active) = &self.active {
            format!(
                "{} · Page {} of {}",
                active.reading.probe.chapter_label,
                active.reading.page + 1,
                active.reading.probe.page_count(),
            )
        } else {
            "No open document".into()
        }
    }

    pub(super) fn draw_skim(&self, canvas: &mut Canvas) {
        let progress = self.skim_progress();
        let layout = self.skim_layout();
        let panel = layout.panel;
        let track = layout.track;
        canvas.popup(panel);
        canvas.text(
            Rect::new(panel.x + 28, panel.y + 20, panel.w - 56, 46),
            &format!("Skim  ·  {}", self.skim_label()),
            TextStyle::new(34).bold(),
        );
        if self.controls.skim_position == SkimPosition::Center {
            canvas.text(
                Rect::new(panel.x + 28, panel.y + 65, panel.w - 56, 40),
                "Preview by swiping, using page buttons, or moving the progress bar.",
                CAPTION,
            );
        }
        canvas.panel(track, 8, 2, 224);
        let filled = (progress * track.w.saturating_sub(18) as f64).round() as i32;
        if filled > 0 {
            canvas.fill(Rect::new(track.x + 9, track.y + 17, filled, 8), 0);
        }
        if let Some(markers) = self.whole_book_marker_pages(false) {
            let last_page = markers.count.saturating_sub(1).max(1);
            let track_width = track.w - 18;
            let mut previous = 0;
            for page in markers.starts {
                let x = (page as f64 / last_page as f64 * track_width as f64).round() as i32;
                if x > previous + 2 && x < track_width - 2 {
                    canvas.fill(Rect::new(track.x + 9 + x, track.y + 11, 2, 20), 0);
                    previous = x;
                }
            }
        }
        let marker = track.x + 9 + filled - 8;
        canvas.panel(Rect::new(marker, track.y + 7, 18, track.h - 14), 8, 1, 0);
        canvas.button(layout.previous_ten, "-10", false, false);
        canvas.button(layout.previous, "‹  Previous", false, false);
        canvas.button(layout.origin, "Origin", false, false);
        canvas.button(layout.next, "Next  ›", false, false);
        canvas.button(layout.next_ten, "+10", false, false);
        canvas.button(layout.cancel, "Cancel", false, false);
        let waiting = self.skim.as_ref().is_some_and(|skim| skim.commit_when_idle);
        canvas.button(
            layout.commit,
            if waiting { "Waiting…" } else { "Read here" },
            true,
            false,
        );
    }

    fn skim_preview_progress(&mut self, fraction: f64) -> Change {
        if let Some(skim) = &mut self.skim {
            skim.commit_when_idle = false;
        }
        let before = self.current_position();
        let result = if let Some(active) = &mut self.active {
            active.reading.jump_progress(fraction.clamp(0.0, 1.0))
        } else if let Some(fixed) = &mut self.fixed {
            let page = (fraction.clamp(0.0, 1.0) * fixed.reading.count().saturating_sub(1) as f64)
                .round() as usize;
            fixed.reading.jump(page).map(|()| ())
        } else {
            return Change::None;
        };
        match result {
            Ok(()) => self.finish_skim_movement(before),
            Err(error) => self.report(error),
        }
    }

    fn skim_step(&mut self, input: Input) -> Change {
        if let Some(skim) = &mut self.skim {
            skim.commit_when_idle = false;
        }
        let Some(input) = self.controls.page_input(input) else {
            return Change::None;
        };
        let before = self.current_position();
        if let Some(active) = &mut self.active {
            let change = active.reading.input(input);
            active.reading.take_end_attempt();
            if change == Change::None {
                return if active.reading.is_turning() {
                    Change::Partial
                } else {
                    Change::None
                };
            }
        } else if let Some(fixed) = &mut self.fixed {
            let changed = match fixed.reading.input_with_status(input, false) {
                Ok(changed) => changed,
                Err(error) => return self.report(error),
            };
            fixed.reading.take_end_attempt();
            if !changed {
                return if fixed.reading.is_turning() {
                    Change::Partial
                } else {
                    Change::None
                };
            }
        } else {
            return Change::None;
        }
        self.finish_skim_movement(before)
    }

    fn skim_step_ten(&mut self, direction: isize) -> Change {
        if let Some(skim) = &mut self.skim {
            skim.commit_when_idle = false;
        }
        let before = self.current_position();
        if let Some(active) = &mut self.active {
            let change = active.reading.automatic_turn(100 * direction as i16);
            active.reading.take_end_attempt();
            if change == Change::None && !active.reading.is_turning() {
                return Change::None;
            }
        } else if let Some(fixed) = &mut self.fixed {
            let start = fixed.reading.page();
            let mut target = start;
            for _ in 0..10 {
                let Some(page) = fixed.reading.next_reading_page(target, direction) else {
                    break;
                };
                target = page;
            }
            if target == start {
                return Change::None;
            }
            if let Err(error) = fixed.reading.jump(target) {
                return self.report(error);
            }
        } else {
            return Change::None;
        }
        self.finish_skim_movement(before)
    }

    fn finish_skim_movement(&mut self, before: Option<ReadingPosition>) -> Change {
        self.notice = None;
        self.selection = None;
        self.pdf_selection = None;
        self.result_selection = None;
        self.search_result = None;
        self.pdf_result_regions.clear();
        self.reading_update(self.current_position() != before)
    }

    pub(super) fn skim_swipe(&mut self, input: Input, start_x: f32) -> Change {
        let Input::Swipe { dx, dy, start_y } = input else {
            return Change::None;
        };
        let layout = self.skim_layout();
        if layout.track.contains(start_x, start_y) {
            return self.skim_preview_progress(layout.fraction(start_x + dx));
        }
        if dx.abs() > dy.abs() {
            self.skim_step(if dx < 0.0 {
                Input::PageForward
            } else {
                Input::PageBack
            })
        } else {
            Change::None
        }
    }

    pub(super) fn skim_input(&mut self, input: Input) -> Change {
        let layout = self.skim_layout();
        match input {
            Input::Tap(x, y) if DONE.contains(x, y) => self.exit(),
            Input::PageForward => self.skim_step(Input::PageForward),
            Input::PageBack => self.skim_step(Input::PageBack),
            Input::Tap(x, y) if layout.track.contains(x, y) => {
                self.skim_preview_progress(layout.fraction(x))
            }
            Input::Tap(x, y) if layout.previous_ten.contains(x, y) => self.skim_step_ten(-1),
            Input::Tap(x, y) if layout.previous.contains(x, y) => self.skim_step(Input::PageBack),
            Input::Tap(x, y) if layout.next.contains(x, y) => self.skim_step(Input::PageForward),
            Input::Tap(x, y) if layout.next_ten.contains(x, y) => self.skim_step_ten(1),
            Input::Tap(x, y) if layout.origin.contains(x, y) => match self.skim_restore_origin() {
                Ok(()) => Change::Partial,
                Err(error) => self.report(error),
            },
            Input::Tap(x, y) if layout.cancel.contains(x, y) => match self.skim_close(false) {
                Ok(()) => Change::Partial,
                Err(error) => self.report(error),
            },
            Input::Tap(x, y) if layout.commit.contains(x, y) => match self.skim_close(true) {
                Ok(()) => Change::Partial,
                Err(error) => self.report(error),
            },
            _ => Change::None,
        }
    }

    pub(super) fn skim_tick(&mut self, now: std::time::Instant) -> Change {
        let change = if let Some(active) = &mut self.active {
            let change = active.reading.tick(now);
            active.reading.take_end_attempt();
            change
        } else if let Some(fixed) = &mut self.fixed {
            match fixed.reading.tick(now) {
                Ok(changed) => {
                    fixed.reading.take_end_attempt();
                    if changed {
                        Change::Partial
                    } else {
                        Change::None
                    }
                }
                Err(error) => return self.report(error),
            }
        } else {
            return Change::None;
        };
        if change == Change::Partial {
            self.notice = None;
            self.selection = None;
            self.pdf_selection = None;
            self.result_selection = None;
            self.search_result = None;
            self.pdf_result_regions.clear();
            let commit = self.skim.as_ref().is_some_and(|skim| skim.commit_when_idle);
            let turning = self
                .active
                .as_ref()
                .is_some_and(|book| book.reading.is_turning())
                || self
                    .fixed
                    .as_ref()
                    .is_some_and(|book| book.reading.is_turning());
            if commit && !turning {
                return match self.skim_close(true) {
                    Ok(()) => Change::Partial,
                    Err(error) => self.report(error),
                };
            }
            self.reading_update(true)
        } else {
            Change::None
        }
    }
}
