//! Chapter-level navigation-history cues for the reflow Book Map.
use super::HistoryLocation;
use crate::session::Reading;
use std::collections::HashMap;

#[derive(Clone, Copy, Default)]
pub(super) struct History {
    pub(super) previous: Option<usize>,
    pub(super) next: Option<usize>,
}

pub(super) fn collect(
    reading: &Reading,
    previous: &[HistoryLocation],
    next: &[HistoryLocation],
    verified: Option<&Verification>,
) -> HashMap<usize, History> {
    let probe = &reading.probe;
    let mut chapters = HashMap::<usize, History>::new();
    for (locations, is_previous) in [(previous, true), (next, false)] {
        for (index, location) in locations.iter().rev().take(3).enumerate() {
            let chapter = match &location.position.anchor {
                Some(anchor) if anchor.book == probe.book => {
                    let Some(chapter) = probe.chapter_index(&anchor.chapter) else {
                        continue;
                    };
                    if anchor.raw_caret.is_some() {
                        let valid = if chapter == reading.chapter {
                            probe.page_for(anchor).is_some()
                        } else {
                            verified.is_some_and(|worker| worker.contains(anchor))
                        };
                        if !valid {
                            continue;
                        }
                    }
                    chapter
                }
                Some(_) => continue,
                None => location.position.chapter,
            };
            if chapter >= probe.chapter_count() {
                continue;
            }
            let history = chapters.entry(chapter).or_default();
            let slot = if is_previous {
                &mut history.previous
            } else {
                &mut history.next
            };
            if slot.is_none() {
                *slot = Some(index + 1);
            }
        }
    }
    chapters
}

use crate::Anchor;
use crate::session::SearchLayoutRecipe;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};

/// At most six recent anchors; probes stay on the worker thread.
pub(super) struct Verification {
    recipe: SearchLayoutRecipe,
    cancelled: Arc<AtomicBool>,
    receiver: mpsc::Receiver<Vec<Anchor>>,
    deadline: Option<Instant>,
    verified: Vec<Anchor>,
}

impl Drop for Verification {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

impl Verification {
    pub(super) fn start(
        reading: &Reading,
        previous: &[HistoryLocation],
        next: &[HistoryLocation],
    ) -> Option<Self> {
        let mut grouped = std::collections::BTreeMap::<usize, Vec<Anchor>>::new();
        for locations in [previous, next] {
            for location in locations.iter().rev().take(3) {
                let Some(anchor) = &location.position.anchor else {
                    continue;
                };
                if anchor.raw_caret.is_none() || anchor.book != reading.probe.book {
                    continue;
                }
                let Some(chapter) = reading.probe.chapter_index(&anchor.chapter) else {
                    continue;
                };
                if chapter != reading.chapter {
                    grouped.entry(chapter).or_default().push(anchor.clone());
                }
            }
        }
        if grouped.is_empty() {
            return None;
        }
        let recipe = reading.config.search_layout_recipe(&reading.probe);
        let worker_recipe = recipe.clone();
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = Arc::clone(&cancelled);
        let (sender, receiver) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("reader-map-history".into())
            .spawn(move || {
                let mut verified = Vec::new();
                for (chapter, anchors) in grouped {
                    if worker_cancelled.load(Ordering::Relaxed) {
                        return;
                    }
                    let Ok(probe) = worker_recipe.chapter(chapter, &worker_cancelled) else {
                        continue;
                    };
                    for anchor in anchors {
                        if worker_cancelled.load(Ordering::Relaxed) {
                            return;
                        }
                        if probe.page_for(&anchor).is_some() {
                            verified.push(anchor);
                        }
                    }
                }
                if !worker_cancelled.load(Ordering::Relaxed) {
                    let _ = sender.send(verified);
                }
            })
            .ok()?;
        Some(Self {
            recipe,
            cancelled,
            receiver,
            deadline: Some(Instant::now() + Duration::from_millis(200)),
            verified: Vec::new(),
        })
    }

    pub(super) fn contains(&self, anchor: &Anchor) -> bool {
        self.verified.contains(anchor)
    }
    pub(super) fn deadline(&self) -> Option<Instant> {
        self.deadline
    }
    pub(super) fn poll(&mut self, now: Instant, current: &SearchLayoutRecipe) -> bool {
        if self.recipe != *current {
            self.cancelled.store(true, Ordering::Relaxed);
            self.deadline = None;
            self.verified.clear();
            return true;
        }
        match self.receiver.try_recv() {
            Ok(verified) => {
                self.deadline = None;
                self.verified = verified;
                true
            }
            Err(mpsc::TryRecvError::Empty) => {
                self.deadline = Some(now + Duration::from_millis(200));
                false
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.deadline = None;
                false
            }
        }
    }
}

impl super::Reader {
    pub(super) fn start_book_map_raw_history(&mut self) {
        if self.fixed.is_some() {
            self.book_map_raw_history = None;
            return;
        }
        self.book_map_raw_history = self
            .active
            .as_ref()
            .and_then(|active| Verification::start(&active.reading, &self.back, &self.forward));
    }

    pub(super) fn poll_book_map_raw_history(
        &mut self,
        now: Instant,
    ) -> Option<primitive_app::Change> {
        if !self
            .book_map_raw_history
            .as_ref()
            .and_then(Verification::deadline)
            .is_some_and(|at| at <= now)
        {
            return None;
        }
        let Some(active) = &self.active else {
            self.book_map_raw_history = None;
            return None;
        };
        let recipe = active
            .reading
            .config
            .search_layout_recipe(&active.reading.probe);
        self.book_map_raw_history.as_mut().and_then(|worker| {
            worker
                .poll(now, &recipe)
                .then_some(primitive_app::Change::Partial)
        })
    }
}
