//! A whole-book font-size change commits only after all reusable layouts are ready.
use super::*;
use crate::session::screen_pages::eager::{Job, Prepared, supported};
use serde::{Deserialize, Serialize};
use std::time::Duration;

const POLL: Duration = Duration::from_millis(80);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Mode {
    #[default]
    Chapter,
    WholeBook,
}

impl Mode {
    pub(crate) fn is_default(&self) -> bool {
        *self == Self::default()
    }

    fn label(self) -> &'static str {
        match self {
            Self::Chapter => "Chapter first",
            Self::WholeBook => "Whole book first",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Scope {
    Book,
    Default,
}

struct Draft {
    id: u64,
    owner: u64,
    size: f64,
    baseline: BookConfig,
    layout: BookConfig,
}

struct Running {
    id: u64,
    job: Job,
    poll_at: Instant,
}

#[derive(Default)]
pub(super) struct State {
    epoch: u64,
    draft: Option<Draft>,
    running: Option<Running>,
    prepared: Option<Prepared>,
}

impl State {
    pub(super) fn cancel(&mut self) {
        if let Some(running) = &self.running {
            running.job.cancel();
        }
        self.draft = None;
        self.prepared = None;
    }

    pub(super) fn pending(&self) -> bool {
        self.draft.is_some()
    }

    pub(super) fn waiting_for_layout(&self) -> bool {
        self.pending() && self.prepared.is_none()
    }
}

impl Drop for State {
    fn drop(&mut self) {
        self.cancel();
    }
}

impl Reader {
    fn selected_font_rendering(&self) -> Option<Mode> {
        self.library
            .state(self.open_book_id()?)?
            .font_size_rendering
    }

    pub(super) fn font_rendering_label(&self) -> &'static str {
        self.selected_font_rendering()
            .unwrap_or(self.controls.font_size_rendering)
            .label()
    }

    pub(super) fn font_rendering_items(&self, scope: Scope) -> Vec<Item> {
        let selected = match scope {
            Scope::Book => self.selected_font_rendering(),
            Scope::Default => Some(self.controls.font_size_rendering),
        };
        let mut items = Vec::new();
        if scope == Scope::Book {
            items.push(
                Item::new("Use default", Action::FontRendering(scope, None)).detail(
                    if selected.is_none() {
                        "Selected"
                    } else {
                        self.controls.font_size_rendering.label()
                    },
                ),
            );
        }
        for mode in [Mode::Chapter, Mode::WholeBook] {
            items.push(
                Item::new(mode.label(), Action::FontRendering(scope, Some(mode))).detail(
                    if Some(mode) == selected {
                        "Selected"
                    } else {
                        match mode {
                            Mode::Chapter => "Read the edited chapter while the page index builds",
                            Mode::WholeBook => {
                                "Keep the old page until all supported chapters are ready"
                            }
                        }
                    },
                ),
            );
        }
        if scope == Scope::Book {
            items.push(Item::new(
                "Default for EPUBs",
                Action::Show(Screen::FontRendering(Scope::Default)),
            ));
            if !self
                .active
                .as_ref()
                .is_some_and(|book| supported(&book.reading))
            {
                items.push(
                    Item::new("Chapter first for this layout", Action::Heading)
                        .detail("Whole book first needs a bounded horizontal text EPUB"),
                );
            }
            if self.eager_typography.pending() {
                items.push(Item::new("Cancel preparation", Action::CancelEagerFont));
            }
        }
        items
    }

    pub(super) fn set_font_rendering(&mut self, scope: Scope, mode: Option<Mode>) -> Result<()> {
        if scope == Scope::Default {
            let mut controls = self.controls;
            controls.font_size_rendering = mode.unwrap_or_default();
            controls.save(&self.controls_path)?;
            self.controls = controls;
        } else {
            let id = self.open_book_id().ok_or("Open an EPUB first")?;
            let book = self
                .library
                .state_mut(id)
                .ok_or("Reading state is unavailable")?;
            let old = book.font_size_rendering;
            book.font_size_rendering = mode;
            if let Err(error) = self.library.persist() {
                self.library.state_mut(id).unwrap().font_size_rendering = old;
                return Err(error.into());
            }
        }
        self.eager_typography.cancel();
        self.notice = None;
        Ok(())
    }

    pub(super) fn queue_eager_font(&mut self, owner: u64, size: f64, layout: &BookConfig) -> bool {
        let mode = self
            .selected_font_rendering()
            .unwrap_or(self.controls.font_size_rendering);
        let Some(book) = self.active.as_ref() else {
            return false;
        };
        if mode != Mode::WholeBook || !supported(&book.reading) {
            return false;
        }
        if layout.settings == book.reading.config.settings {
            if self.eager_typography.pending() {
                self.eager_typography.cancel();
                self.show(Screen::Book);
                return true;
            }
            return false;
        }
        let baseline = book.reading.config.clone();
        self.eager_typography.cancel();
        self.eager_typography.epoch = self.eager_typography.epoch.wrapping_add(1);
        self.eager_typography.draft = Some(Draft {
            id: self.eager_typography.epoch,
            owner,
            size,
            baseline,
            layout: layout.clone(),
        });
        self.screen_page_worker.cancel();
        self.show(Screen::Book);
        self.notice = Some("Preparing font size for the whole book…".into());
        true
    }

    pub(super) fn pending_logical_size(&self, owner: u64) -> Option<f64> {
        let active = self.active.as_ref().filter(|book| book.id == owner)?;
        let draft = self.eager_typography.draft.as_ref()?;
        (draft.owner == owner && same_layout(&draft.baseline, &active.reading.config))
            .then_some(draft.size)
    }

    fn eager_commit_visible(&self) -> bool {
        self.stats_awake
            && self.screen == Screen::Book
            && self.touch_selection_enabled()
            && self.touch_hold.is_none()
            && self.selection.is_none()
            && self.closing_profile.is_none()
            && !self.opening_ask_visible()
            && self.pending_opening_profiles.is_none()
            && self
                .active
                .as_ref()
                .is_some_and(|book| !book.reading.page_turn_pending())
    }

    pub(super) fn eager_font_deadline(&self) -> Option<Instant> {
        if let Some(running) = &self.eager_typography.running {
            return Some(running.poll_at);
        }
        if self.eager_typography.draft.is_some()
            && if self.eager_typography.waiting_for_layout() {
                self.screen_page_worker.deadline(None).is_none()
            } else {
                self.eager_commit_visible()
            }
        {
            return Some(Instant::now());
        }
        None
    }

    pub(super) fn eager_font_tick(&mut self, now: Instant) -> Option<Change> {
        if self.eager_typography.draft.as_ref().is_some_and(|draft| {
            !self.stats_awake
                || self.active.as_ref().is_none_or(|active| {
                    active.id != draft.owner
                        || !same_layout(&draft.baseline, &active.reading.config)
                })
        }) {
            self.eager_typography.cancel();
        }
        if let Some(running) = &mut self.eager_typography.running {
            if !running.job.finished() {
                running.poll_at = now + POLL;
                return None;
            }
            let running = self.eager_typography.running.take().unwrap();
            if self
                .eager_typography
                .draft
                .as_ref()
                .is_some_and(|draft| draft.id == running.id)
            {
                match running.job.collect() {
                    Ok(prepared) => self.eager_typography.prepared = Some(prepared),
                    Err(error) => {
                        self.eager_typography.cancel();
                        return Some(self.report(error));
                    }
                }
            }
        }
        let draft = self.eager_typography.draft.as_ref()?;
        if self.eager_typography.prepared.is_none() {
            if self.screen_page_worker.deadline(None).is_some() {
                return None;
            }
            let reading = &self.active.as_ref()?.reading;
            return match Job::start(&draft.layout, reading) {
                Ok(job) => {
                    self.eager_typography.running = Some(Running {
                        id: draft.id,
                        job,
                        poll_at: now + POLL,
                    });
                    None
                }
                Err(error) => {
                    self.eager_typography.cancel();
                    Some(self.report(error))
                }
            };
        }
        if !self.eager_commit_visible() {
            return None;
        }
        let draft = self.eager_typography.draft.take().unwrap();
        let prepared = self.eager_typography.prepared.take().unwrap();
        Some(match self.commit_eager_font(draft, prepared) {
            Ok(()) => Change::Full,
            Err(error) => self.report(error),
        })
    }

    fn commit_eager_font(&mut self, draft: Draft, prepared: Prepared) -> Result<()> {
        let book = self.active.as_ref().ok_or("The EPUB closed")?;
        let location = position(&book.reading);
        let anchor = location
            .anchor
            .as_ref()
            .ok_or("The current reading position is unavailable")?;
        if with_storage(|| self.library.identify(&book.entry))? != draft.owner {
            return Err("The EPUB edition changed during whole-book preparation".into());
        }
        let mut layout = draft.layout;
        prepared.install(&mut layout)?;
        let candidate = Reading::restore(layout, location.chapter, location.page, Some(anchor))?;
        if candidate.anchor().as_ref() != Some(anchor)
            || candidate.whole_book_screen_pages().is_none()
        {
            return Err("The prepared EPUB could not keep this reading position".into());
        }
        self.commit_logical_font(draft.owner, draft.size, candidate)?;
        self.notice = None;
        Ok(())
    }
}

fn same_layout(first: &BookConfig, second: &BookConfig) -> bool {
    first.book == second.book
        && first.settings == second.settings
        && first.viewport == second.viewport
        && first.fonts == second.fonts
        && first.user_styles == second.user_styles
        && first.user_hyphenation == second.user_hyphenation
        && first.scroll == second.scroll
        && first.facing_pages == second.facing_pages
        && std::rc::Rc::ptr_eq(&first.hidden_flows, &second.hidden_flows)
        && first.hide_nonlinear == second.hide_nonlinear
}

#[cfg(test)]
mod tests;
