//! Transient page preview over the current reading view.

use primitive_app::{App, Change, HEIGHT, WIDTH};
use primitive_ui::{CAPTION, Canvas, Input, Rect, TextStyle};

use super::{DONE, HistoryLocation, Reader, Screen, fixed_page_label};
use crate::fixed_session::FixedJump;

const PANEL: Rect = Rect::new(40, HEIGHT as i32 - 460, WIDTH as i32 - 80, 384);
const TRACK: Rect = Rect::new(92, HEIGHT as i32 - 318, WIDTH as i32 - 184, 42);
const BACK_PAGE: Rect = Rect::new(116, HEIGHT as i32 - 236, 205, 68);
const ORIGIN: Rect = Rect::new(530, HEIGHT as i32 - 236, 205, 68);
const FORWARD_PAGE: Rect = Rect::new(943, HEIGHT as i32 - 236, 205, 68);
const CANCEL: Rect = Rect::new(260, HEIGHT as i32 - 148, 300, 72);
const COMMIT: Rect = Rect::new(704, HEIGHT as i32 - 148, 300, 72);

pub(super) struct Session {
    origin: HistoryLocation,
    commit_when_idle: bool,
}

impl Reader {
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
                "{} · {}/{}",
                fixed_page_label(&fixed.reading, fixed.reading.page()),
                fixed.reading.page() + 1,
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
        canvas.popup(PANEL);
        canvas.text(
            Rect::new(PANEL.x + 28, PANEL.y + 20, PANEL.w - 56, 46),
            &format!("Skim  ·  {}", self.skim_label()),
            TextStyle::new(34).bold(),
        );
        canvas.text(
            Rect::new(PANEL.x + 28, PANEL.y + 65, PANEL.w - 56, 40),
            "Preview by swiping, using page buttons, or moving the progress bar.",
            CAPTION,
        );
        canvas.panel(TRACK, 8, 2, 224);
        let filled = (progress * TRACK.w.saturating_sub(18) as f64).round() as i32;
        if filled > 0 {
            canvas.fill(Rect::new(TRACK.x + 9, TRACK.y + 17, filled, 8), 0);
        }
        let marker = TRACK.x + 9 + filled - 8;
        canvas.panel(Rect::new(marker, TRACK.y + 7, 18, TRACK.h - 14), 8, 1, 0);
        canvas.button(BACK_PAGE, "‹  Previous", false, false);
        canvas.button(ORIGIN, "Origin", false, false);
        canvas.button(FORWARD_PAGE, "Next  ›", false, false);
        canvas.button(CANCEL, "Cancel", false, false);
        let waiting = self.skim.as_ref().is_some_and(|skim| skim.commit_when_idle);
        canvas.button(
            COMMIT,
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
            Ok(()) => {
                self.notice = None;
                self.selection = None;
                self.result_selection = None;
                self.pdf_selection = None;
                self.search_result = None;
                self.pdf_result_regions.clear();
                let changed = self.current_position() != before;
                if changed {
                    self.reading_update(true)
                } else {
                    Change::Partial
                }
            }
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
        self.notice = None;
        self.selection = None;
        self.pdf_selection = None;
        self.result_selection = None;
        self.search_result = None;
        self.pdf_result_regions.clear();
        if self.current_position() == before {
            Change::Partial
        } else {
            self.reading_update(true)
        }
    }

    pub(super) fn skim_swipe(&mut self, input: Input, start_x: f32) -> Change {
        let Input::Swipe { dx, dy, start_y } = input else {
            return Change::None;
        };
        if TRACK.contains(start_x, start_y) {
            let x = start_x + dx;
            let fraction = ((x - TRACK.x as f32) / TRACK.w as f32).clamp(0.0, 1.0);
            return self.skim_preview_progress(f64::from(fraction));
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
        match input {
            Input::Tap(x, y) if DONE.contains(x, y) => self.exit(),
            Input::PageForward => self.skim_step(Input::PageForward),
            Input::PageBack => self.skim_step(Input::PageBack),
            Input::Tap(x, y) if TRACK.contains(x, y) => {
                let fraction = ((x - TRACK.x as f32) / TRACK.w as f32).clamp(0.0, 1.0);
                self.skim_preview_progress(f64::from(fraction))
            }
            Input::Tap(x, y) if BACK_PAGE.contains(x, y) => self.skim_step(Input::PageBack),
            Input::Tap(x, y) if FORWARD_PAGE.contains(x, y) => self.skim_step(Input::PageForward),
            Input::Tap(x, y) if ORIGIN.contains(x, y) => match self.skim_restore_origin() {
                Ok(()) => Change::Partial,
                Err(error) => self.report(error),
            },
            Input::Tap(x, y) if CANCEL.contains(x, y) => match self.skim_close(false) {
                Ok(()) => Change::Partial,
                Err(error) => self.report(error),
            },
            Input::Tap(x, y) if COMMIT.contains(x, y) => match self.skim_close(true) {
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
