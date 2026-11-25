//! Confirmation and scheduling for a single document opening.
use super::opening_rules::{Command, Page};
use super::{
    Action, CAPTION, Canvas, Change, Input, Item, Profile, Reader, Rect, Result, Screen, Sheet,
    SheetAction,
};
use crate::reading_profiles::opening::Timing;
use std::time::{Duration, Instant};

enum Phase {
    Waiting,
    Asking,
    Due(Instant),
}

struct Task {
    order: usize,
    profile: Profile,
    phase: Phase,
}

pub(in crate::app) struct OpeningExecution {
    book: u64,
    epoch: u64,
    opened: Instant,
    tasks: Vec<Task>,
}

impl OpeningExecution {
    fn new(book: u64, epoch: u64, ordered: Vec<Profile>, opened: Instant) -> Option<Self> {
        let tasks: Vec<_> = ordered
            .into_iter()
            .enumerate()
            .filter_map(|(order, profile)| {
                let execution = profile.opening_execution()?;
                let phase = if execution.ask {
                    Phase::Waiting
                } else {
                    Phase::Due(due_at(execution.timing, opened))
                };
                Some(Task {
                    order,
                    profile,
                    phase,
                })
            })
            .collect();
        (!tasks.is_empty()).then_some(Self {
            book,
            epoch,
            opened,
            tasks,
        })
    }

    fn shown(&self) -> Option<&Task> {
        self.tasks
            .iter()
            .find(|task| matches!(task.phase, Phase::Asking))
    }

    fn deadline(&self) -> Option<Instant> {
        let show_prompt = self.shown().is_none();
        self.tasks
            .iter()
            .filter_map(|task| match task.phase {
                Phase::Waiting if show_prompt => Some(self.opened),
                Phase::Due(due) => Some(due),
                _ => None,
            })
            .min()
    }

    fn show_next(&mut self) -> bool {
        if self.shown().is_some() {
            return false;
        }
        let Some(task) = self
            .tasks
            .iter_mut()
            .find(|task| matches!(task.phase, Phase::Waiting))
        else {
            return false;
        };
        task.phase = Phase::Asking;
        true
    }

    fn decide(&mut self, accept: bool, now: Instant) -> bool {
        let Some(index) = self
            .tasks
            .iter()
            .position(|task| matches!(task.phase, Phase::Asking))
        else {
            return false;
        };
        if !accept {
            self.tasks.remove(index);
            return false;
        }
        let task = &mut self.tasks[index];
        let timing = task
            .profile
            .opening_execution()
            .expect("queued policy")
            .timing;
        task.phase = Phase::Due(due_at(timing, now));
        timing == Timing::Prompt
    }

    fn take_due(&mut self, now: Instant) -> Vec<Profile> {
        let mut ready: Vec<_> = self
            .tasks
            .extract_if(
                ..,
                |task| matches!(task.phase, Phase::Due(due) if due <= now),
            )
            .collect();
        ready.sort_by_key(|task| {
            (
                match task.phase {
                    Phase::Due(due) => due,
                    _ => unreachable!(),
                },
                task.order,
            )
        });
        ready.into_iter().map(|task| task.profile).collect()
    }
}

fn due_at(timing: Timing, now: Instant) -> Instant {
    match timing {
        Timing::Custom { tenths } => now + Duration::from_millis(u64::from(tenths) * 100),
        Timing::Default | Timing::Prompt => now,
    }
}

impl Reader {
    pub(in crate::app) fn arm_opening_execution(&mut self, profiles: Vec<Profile>, now: Instant) {
        self.opening_execution = self.open_book_id().and_then(|book| {
            OpeningExecution::new(book, self.opening_profile_epoch, profiles, now)
        });
    }

    fn opening_execution_current(&self) -> bool {
        self.opening_execution.as_ref().is_some_and(|run| {
            self.open_book_id() == Some(run.book) && self.opening_profile_epoch == run.epoch
        })
    }

    fn opening_execution_eligible(&self) -> bool {
        self.opening_execution_current()
            && self.stats_awake
            && matches!(self.screen, Screen::Book | Screen::Fixed)
            && self.end_dialog.is_none()
            && !self.back_exit_confirmation_open()
            && self.link_popup.is_none()
            && self.saved_popup.is_none()
            && self.selection.is_none()
            && self.pdf_selection.is_none()
            && self.touch_hold.is_none()
            && !self.selection_mode
            && self.range_edit.is_none()
            && !self.read_alarm.modal()
            && !self.fixed_search_fit_active()
            && !self.touch_locked
    }

    pub(in crate::app) fn opening_execution_deadline(&self) -> Option<Instant> {
        self.opening_execution_eligible()
            .then(|| self.opening_execution.as_ref()?.deadline())
            .flatten()
    }

    pub(in crate::app) fn opening_execution_tick(&mut self, now: Instant) -> Option<Change> {
        if !self.opening_execution_eligible() {
            return None;
        }
        let ready = self.opening_execution.as_mut()?.take_due(now);
        let mut change = (!ready.is_empty()).then(|| match self.apply_opening_batch(&ready) {
            Ok(()) => Change::Partial,
            Err(error) => self.report(format!("Opening profile could not be applied: {error}")),
        });
        if let Some(run) = &mut self.opening_execution {
            if run.show_next() {
                change = Some(Change::Partial);
            }
            if run.tasks.is_empty() {
                self.opening_execution = None;
            }
        }
        if change.is_some() {
            self.sync_auto_turn(now);
        }
        change
    }

    fn apply_opening_batch(&mut self, profiles: &[Profile]) -> Result<()> {
        if !self.opening_execution_current() {
            return Err("The book changed before opening profile execution".into());
        }
        self.apply_profile_batch(profiles)?;
        let message = match profiles {
            [] => return Ok(()),
            [profile] => format!("Applied profile · {}", profile.name()),
            profiles => format!("Applied {} reading profiles", profiles.len()),
        };
        self.notify_reading(
            crate::app::reading_notifications::Source::Dispatcher,
            message,
        );
        Ok(())
    }

    pub(in crate::app) fn opening_ask_visible(&self) -> bool {
        self.opening_execution_eligible()
            && self
                .opening_execution
                .as_ref()
                .and_then(OpeningExecution::shown)
                .is_some()
    }

    fn opening_ask_sheet(&self) -> Option<Sheet<'_>> {
        if !self.opening_ask_visible() {
            return None;
        }
        let profile = &self.opening_execution.as_ref()?.shown()?.profile;
        let (width, height) = self.reading_viewport().dimensions();
        Some(Sheet::centered(
            Rect::new(0, 0, width as i32, height as i32),
            self.controls
                .interface_language
                .reading_profile_text("Apply reading profile?"),
            profile.name(),
            self.controls
                .interface_language
                .reading_profile_text("Apply"),
        ))
    }

    pub(in crate::app) fn draw_opening_ask(&self, canvas: &mut Canvas) {
        if let Some(sheet) = self.opening_ask_sheet() {
            sheet.draw(canvas, None);
            let language = self.controls.interface_language;
            if !language.is_english() {
                canvas.button(
                    sheet.button(SheetAction::Cancel),
                    language.reading_profile_text("Cancel"),
                    false,
                    false,
                );
            }
        }
    }

    pub(in crate::app) fn opening_ask_input(
        &mut self,
        input: Input,
        now: Instant,
    ) -> Option<Change> {
        if matches!(input, Input::Sleep | Input::Wake) {
            return None;
        }
        let sheet = self.opening_ask_sheet()?;
        let action = match input {
            Input::PageBack => Some(SheetAction::Cancel),
            Input::Tap(x, y) => sheet.hit(x, y),
            _ => None,
        };
        let Some(action) = action else {
            return Some(Change::None);
        };
        let promptly = self
            .opening_execution
            .as_mut()?
            .decide(action == SheetAction::Confirm, now);
        self.sync_auto_turn(now);
        if promptly {
            return Some(self.opening_execution_tick(now).unwrap_or(Change::Partial));
        }
        Some(Change::Partial)
    }

    pub(in crate::app) fn opening_modal_press(&self, input: Input) -> Option<Option<(f32, f32)>> {
        let point = if let Input::Tap(x, y) = input {
            Some((x, y))
        } else {
            None
        };
        if let Some(sheet) = self.opening_ask_sheet() {
            return Some(point.filter(|&(x, y)| {
                [SheetAction::Cancel, SheetAction::Confirm]
                    .into_iter()
                    .any(|action| sheet.button(action).contains(x, y))
            }));
        }
        if matches!(
            self.screen,
            Screen::ReadingProfileOpening(_, Page::CustomDelay)
        ) {
            return Some(point.filter(|&(x, y)| {
                [LESS, MORE, CANCEL, SAVE]
                    .into_iter()
                    .any(|rect| self.navigation_layout().centered(rect).contains(x, y))
            }));
        }
        None
    }
}

#[cfg(test)]
mod tests;

pub(super) fn timing_label(timing: Timing) -> String {
    match timing {
        Timing::Default => "Default".into(),
        Timing::Prompt => "Prompt".into(),
        Timing::Custom { tenths } => seconds(tenths),
    }
}

fn seconds(tenths: u8) -> String {
    format!("{}.{} s", tenths / 10, tenths % 10)
}

const PANEL: Rect = Rect::new(182, 587, 900, 506);
const LESS: Rect = Rect::new(246, 779, 130, 100);
const MORE: Rect = Rect::new(888, 779, 130, 100);
const CANCEL: Rect = Rect::new(246, 937, 362, 100);
const SAVE: Rect = Rect::new(656, 937, 362, 100);

impl Reader {
    pub(super) fn opening_delay_items(&self, index: usize) -> Vec<Item> {
        let language = self.controls.interface_language;
        let current = self
            .profiles
            .get(index)
            .and_then(Profile::opening_execution)
            .map(|execution| execution.timing);
        let choice = |label, timing, detail| {
            Item::new(
                language.reading_profile_text(label),
                Action::OpeningProfileRule(index, Command::SetTiming(timing)),
            )
            .detail(if current == Some(timing) {
                language.reading_profile_text("Selected")
            } else {
                language.reading_profile_text(detail)
            })
        };
        let custom = current
            .filter(|timing| matches!(timing, Timing::Custom { .. }))
            .map_or_else(|| "1.0–10.0 s".into(), timing_label);
        vec![
            choice("Prompt", Timing::Prompt, "Apply during opening"),
            choice("Default", Timing::Default, "Apply just after opening"),
            Item::new(
                language.reading_profile_text("Custom"),
                Action::Show(Screen::ReadingProfileOpening(index, Page::CustomDelay)),
            )
            .detail(custom),
        ]
    }

    pub(in crate::app) fn prepare_opening_delay(&mut self, index: usize) {
        self.opening_delay_draft = match self
            .profiles
            .get(index)
            .and_then(Profile::opening_execution)
            .map(|execution| execution.timing)
        {
            Some(Timing::Custom { tenths }) => tenths,
            _ => Timing::INITIAL_TENTHS,
        };
    }

    pub(in crate::app) fn draw_opening_delay(&self, canvas: &mut Canvas) {
        if !matches!(
            self.screen,
            Screen::ReadingProfileOpening(_, Page::CustomDelay)
        ) {
            return;
        }
        let language = self.controls.interface_language;
        let place = |rect| self.navigation_layout().centered(rect);
        canvas.popup(place(PANEL));
        canvas.text(
            place(Rect::new(246, 626, 772, 80)),
            language.reading_profile_text("Custom delay"),
            primitive_ui::TextStyle::new(48).bold(),
        );
        canvas.text(
            place(Rect::new(406, 779, 452, 100)),
            &seconds(self.opening_delay_draft),
            primitive_ui::TextStyle::new(52).bold().center(),
        );
        canvas.button(place(LESS), "-", false, false);
        canvas.button(place(MORE), "+", false, false);
        if let Some(notice) = &self.notice {
            canvas.text(place(Rect::new(246, 881, 772, 54)), notice, CAPTION);
        }
        canvas.button(
            place(CANCEL),
            language.reading_profile_text("Cancel"),
            false,
            false,
        );
        canvas.button(
            place(SAVE),
            language.reading_profile_text("Set delay"),
            true,
            false,
        );
    }

    pub(in crate::app) fn opening_delay_input(&mut self, input: Input) -> Option<Change> {
        let Screen::ReadingProfileOpening(index, Page::CustomDelay) = self.screen else {
            return None;
        };
        if matches!(input, Input::Sleep | Input::Wake) {
            return None;
        }
        let back = Screen::ReadingProfileOpening(index, Page::Delay);
        if input == Input::PageBack {
            self.show(back);
            return Some(Change::Partial);
        }
        let (x, y, held) = match input {
            Input::Tap(x, y) => (x, y, false),
            Input::LongTap(x, y) => (x, y, true),
            _ => return Some(Change::None),
        };
        let place = |rect| self.navigation_layout().centered(rect);
        if !held && (place(CANCEL).contains(x, y) || !place(PANEL).contains(x, y)) {
            self.show(back);
        } else if !held && place(SAVE).contains(x, y) {
            let cmd = Command::SetTiming(Timing::Custom {
                tenths: self.opening_delay_draft,
            });
            if let Err(error) = self.change_opening_rule(index, cmd) {
                return Some(self.report(error));
            }
            self.show(back);
        } else {
            let direction = if place(LESS).contains(x, y) {
                -1
            } else if place(MORE).contains(x, y) {
                1
            } else {
                return Some(Change::None);
            };
            let step = if held { 1 } else { 10 };
            self.opening_delay_draft = self
                .opening_delay_draft
                .saturating_add_signed(direction * step)
                .clamp(Timing::MIN_TENTHS, Timing::MAX_TENTHS);
        }
        Some(Change::Partial)
    }
}
