//! Prepare an exact quote prefix after the first, bounded foreground frame.
use super::{Change, Reader, Screen};
use crate::session::Reading;
use crate::session::quote_prefixes::{
    Marker,
    worker::{Job, Prepared, Request},
};
use std::time::{Duration, Instant};

#[cfg(test)]
mod tests;

const FIRST_FRAME_DELAY: Duration = Duration::from_millis(380);
const POLL: Duration = Duration::from_millis(150);
pub(super) const APPROXIMATE_PAGE_NOTICE: &str =
    "Opened an approximate page while the chapter layout is being prepared.";
const COMPLETED_PAGE_NOTICE: &str = "Approximate page.";
const UNAVAILABLE_PAGE_NOTICE: &str = "Page may differ.";

#[derive(Clone, PartialEq)]
struct Identity {
    owner: u64,
    marker: Marker,
}

struct Waiting {
    identity: Identity,
    after: Instant,
}

struct Running {
    identity: Identity,
    request: Request,
    job: Job,
    poll_at: Instant,
}

struct Ready {
    identity: Identity,
    request: Request,
    prepared: Prepared,
}

#[derive(Default)]
pub(super) struct State {
    epoch: u64,
    sleeping: bool,
    waiting: Option<Waiting>,
    running: Option<Running>,
    ready: Option<Ready>,
    rejected: Option<Identity>,
}

impl State {
    pub(super) fn sleep(&mut self) {
        self.cancel();
        self.sleeping = true;
    }

    pub(super) fn wake(&mut self) {
        self.sleeping = false;
    }

    pub(super) fn cancel(&mut self) {
        self.epoch = self.epoch.wrapping_add(1);
        if let Some(running) = &self.running {
            running.job.cancel();
        }
        self.waiting = None;
        self.ready = None;
        self.rejected = None;
    }
}

impl Drop for State {
    fn drop(&mut self) {
        self.cancel();
    }
}

impl Reader {
    fn reject_quote_repair(&mut self, identity: Identity) {
        self.quote_repair.rejected = Some(identity);
        if self.notice.as_deref() == Some(APPROXIMATE_PAGE_NOTICE) {
            self.notice = Some(UNAVAILABLE_PAGE_NOTICE.into());
        }
    }

    fn quote_identity(&self) -> Option<Identity> {
        let active = self.active.as_ref()?;
        let marker = active.reading.quote_marker()?.clone();
        (marker.edition == active.reading.probe.book
            && marker.need.chapter == active.reading.chapter
            && marker.need.settings == active.reading.probe.settings)
            .then_some(Identity {
                owner: active.id,
                marker,
            })
    }

    fn quote_can_start(&self) -> bool {
        !self.quote_repair.sleeping
            && self.stats_awake
            && self.screen == Screen::Book
            && self.active.is_some()
    }

    fn quote_can_commit(&self) -> bool {
        self.quote_can_start()
            && self.reading_overlays_clear()
            && self.quickstart.is_none()
            && !self.scroll_pan.busy()
            && !self.selection_mode
            && !self.back_exit_confirmation_open()
            && self.touch_hold.is_none()
            && self.selection.is_none()
            && self.closing_profile.is_none()
            && !self.opening_ask_visible()
            && self.pending_opening_profiles.is_none()
            && self.active.as_ref().is_some_and(|active| {
                !active.reading.page_turn_pending() && active.reading.anchor().is_some()
            })
    }

    fn quote_request(&self, identity: &Identity) -> Option<Request> {
        let active = self
            .active
            .as_ref()
            .filter(|active| active.id == identity.owner)?;
        let reading = &active.reading;
        if reading.quote_marker() != Some(&identity.marker)
            || reading.config.quote_prefixes.stamp().as_ref() != Some(&identity.marker.stamp)
        {
            return None;
        }
        Request::from_need(
            self.quote_repair.epoch,
            &reading.config.book,
            &reading.config.quote_prefixes,
            &identity.marker.need,
            reading.probe.book,
            reading.probe.chapter_entries(),
            reading.chapter,
            reading.probe.settings,
        )
    }

    pub(super) fn quote_repair_deadline(&self) -> Option<Instant> {
        if let Some(running) = &self.quote_repair.running {
            return Some(running.poll_at);
        }
        let identity = self.quote_identity();
        if let Some(ready) = &self.quote_repair.ready {
            return (identity.as_ref() != Some(&ready.identity) || self.quote_can_commit())
                .then(Instant::now);
        }
        if !self.quote_can_start() {
            return None;
        }
        let identity = identity?;
        if self.quote_repair.rejected.as_ref() == Some(&identity) {
            return None;
        }
        Some(
            self.quote_repair
                .waiting
                .as_ref()
                .filter(|waiting| waiting.identity == identity)
                .map_or_else(Instant::now, |waiting| waiting.after),
        )
    }

    pub(super) fn quote_repair_tick(&mut self, now: Instant) -> Option<Change> {
        let current = self.quote_identity();
        if let Some(running) = &mut self.quote_repair.running {
            if current.as_ref() != Some(&running.identity)
                || !self.stats_awake
                || self.quote_repair.sleeping
            {
                running.job.cancel();
            }
            if !running.job.finished() {
                running.poll_at = now + POLL;
                return None;
            }
            let running = self.quote_repair.running.take().unwrap();
            let current_job = current.as_ref() == Some(&running.identity)
                && running.request.epoch == self.quote_repair.epoch
                && self.stats_awake
                && !self.quote_repair.sleeping;
            match running.job.collect() {
                Ok(prepared) if current_job => {
                    self.quote_repair.ready = Some(Ready {
                        identity: running.identity,
                        request: running.request,
                        prepared,
                    });
                }
                Err(error) if current_job => {
                    eprintln!("reader quote preparation failed: {error}");
                    self.reject_quote_repair(running.identity);
                }
                _ => {}
            }
        }

        if let Some(ready) = &self.quote_repair.ready {
            if current.as_ref() != Some(&ready.identity) {
                self.quote_repair.ready = None;
            } else if self.quote_can_commit() {
                let ready = self.quote_repair.ready.take().unwrap();
                let identity = ready.identity.clone();
                return match self.commit_quote_repair(ready) {
                    Ok(true) => {
                        if self.notice.as_deref() == Some(APPROXIMATE_PAGE_NOTICE) {
                            self.notice = Some(COMPLETED_PAGE_NOTICE.into());
                        }
                        Some(Change::Partial)
                    }
                    Ok(false) => {
                        self.reject_quote_repair(identity);
                        None
                    }
                    Err(error) => {
                        eprintln!("reader quote repaint failed: {error}");
                        self.reject_quote_repair(identity);
                        None
                    }
                };
            } else {
                return None;
            }
        }
        if !self.quote_can_start() {
            self.quote_repair.waiting = None;
            return None;
        }
        let Some(identity) = current else {
            self.quote_repair.waiting = None;
            return None;
        };
        if self.quote_repair.rejected.as_ref() == Some(&identity) {
            return None;
        }
        if self
            .quote_repair
            .waiting
            .as_ref()
            .is_none_or(|waiting| waiting.identity != identity)
        {
            self.quote_repair.waiting = Some(Waiting {
                identity,
                after: now + FIRST_FRAME_DELAY,
            });
            return None;
        }
        if now < self.quote_repair.waiting.as_ref().unwrap().after {
            return None;
        }
        let waiting = self.quote_repair.waiting.take().unwrap();
        let active = self.active.as_ref().unwrap();
        if active.reading.config.quote_prefixes.stamp().as_ref()
            != Some(&waiting.identity.marker.stamp)
            && !active
                .reading
                .config
                .quote_prefixes
                .rearm(&active.reading.config.book, &waiting.identity.marker)
        {
            self.reject_quote_repair(waiting.identity);
            return None;
        }
        let Some(request) = self.quote_request(&waiting.identity) else {
            self.reject_quote_repair(waiting.identity);
            return None;
        };
        match Job::start(request.clone()) {
            Ok(job) => {
                self.quote_repair.running = Some(Running {
                    identity: waiting.identity,
                    request,
                    job,
                    poll_at: now + POLL,
                });
            }
            Err(error) => {
                eprintln!("reader quote preparation unavailable: {error}");
                self.reject_quote_repair(waiting.identity);
            }
        }
        None
    }

    fn commit_quote_repair(&mut self, ready: Ready) -> crate::Result<bool> {
        if self.quote_identity().as_ref() != Some(&ready.identity) {
            return Ok(false);
        }
        let Some(current) = self.quote_request(&ready.identity) else {
            return Ok(false);
        };
        if !ready.request.matches(&current) {
            return Ok(false);
        }
        let active = self.active.as_ref().ok_or("The EPUB closed")?;
        let anchor = active
            .reading
            .anchor()
            .ok_or("The reading position is unavailable")?;
        let chapter = active.reading.chapter;
        let page = active.reading.page;
        let scroll = active.reading.scroll_history_top();
        let mut config = active.reading.config.clone();
        if !ready.prepared.install(&current, &config.quote_prefixes)? {
            return Ok(false);
        }
        // A cached candidate may contain the old guessed prefix.
        config.eager_layouts = Default::default();
        let candidate =
            Reading::restore_with_scroll_top(config, chapter, page, Some(&anchor), scroll)?;
        if candidate.anchor().as_ref() != Some(&anchor) || candidate.quote_marker().is_some() {
            return Err("The exact quote layout could not preserve the reading position".into());
        }
        self.active.as_mut().ok_or("The EPUB closed")?.reading = candidate;
        Ok(true)
    }
}
