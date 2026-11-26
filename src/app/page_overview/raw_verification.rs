//! Bounded, resumable source verification for reflow overview cues.
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};

use crate::Anchor;
use crate::selection::TextSelection;
use crate::session::SearchLayoutRecipe;

const POLL: Duration = Duration::from_millis(200);
pub(super) const SLICE: usize = 4;
pub(super) const SCAN: usize = 128;
const BINS: usize = 256;

pub(super) enum Task {
    Mark {
        first: usize,
        last: usize,
        start: Anchor,
        end: Option<Anchor>,
        flags: u8,
        quote: Option<TextSelection>,
    },
    History {
        chapter: usize,
        anchor: Anchor,
    },
}

struct Event {
    marks: Vec<(usize, u8)>,
    history: Vec<Anchor>,
    quotes: Vec<TextSelection>,
}

pub(crate) struct Verification {
    pub(super) recipe: SearchLayoutRecipe,
    pub(super) cursor: usize,
    pub(super) total: usize,
    pub(super) thumb_candidates: usize,
    pub(super) chapter: usize,
    pub(super) chapters: usize,
    cancel: Arc<AtomicBool>,
    receiver: Option<mpsc::Receiver<Event>>,
    next: Option<Instant>,
    bins: [u8; BINS],
    history: Vec<Anchor>,
    quotes: Vec<TextSelection>,
}

impl Drop for Verification {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl Verification {
    pub(super) fn new(
        recipe: SearchLayoutRecipe,
        total: usize,
        chapter: usize,
        chapters: usize,
    ) -> Self {
        Self {
            recipe,
            cursor: 0,
            total,
            thumb_candidates: 0,
            chapter,
            chapters,
            cancel: Arc::new(AtomicBool::new(false)),
            receiver: None,
            next: Some(Instant::now()),
            bins: [0; BINS],
            history: Vec::new(),
            quotes: Vec::new(),
        }
    }
    pub(crate) fn deadline(&self) -> Option<Instant> {
        self.next
    }
    pub(super) fn history(&self, anchor: &Anchor) -> bool {
        self.history.contains(anchor)
    }
    pub(super) fn selection(&self, selection: &TextSelection) -> bool {
        self.quotes.contains(selection)
    }
    pub(super) fn bins(&self) -> &[u8; BINS] {
        &self.bins
    }
    pub(super) fn pending(&self) -> bool {
        self.receiver.is_some()
    }
    pub(super) fn wake(&mut self, now: Instant) {
        self.next = (self.cursor < self.total).then_some(now + POLL);
    }

    pub(super) fn submit(&mut self, tasks: Vec<Task>, now: Instant) {
        if tasks.is_empty() {
            self.wake(now);
            return;
        }
        let (tx, rx) = mpsc::sync_channel(1);
        let recipe = self.recipe.clone();
        let cancel = Arc::clone(&self.cancel);
        if std::thread::Builder::new()
            .name("reader-map-raw".into())
            .spawn(move || {
                let mut result = Event {
                    marks: Vec::new(),
                    history: Vec::new(),
                    quotes: Vec::new(),
                };
                for task in tasks {
                    if cancel.load(Ordering::Relaxed) {
                        return;
                    }
                    match task {
                        Task::History { chapter, anchor } => {
                            if recipe
                                .chapter(chapter, &cancel)
                                .ok()
                                .and_then(|probe| probe.page_for(&anchor))
                                .is_some()
                            {
                                result.history.push(anchor);
                            }
                        }
                        Task::Mark {
                            first,
                            last,
                            start,
                            end,
                            flags,
                            quote,
                        } => {
                            let Ok(probe) = recipe.chapter(first, &cancel) else {
                                continue;
                            };
                            let Some(first_page) = probe.page_for(&start) else {
                                continue;
                            };
                            if let Some(end) = &end {
                                let end_page = if last == first {
                                    probe.page_for(end)
                                } else {
                                    recipe
                                        .chapter(last, &cancel)
                                        .ok()
                                        .and_then(|p| p.page_for(end))
                                };
                                if end_page.is_none()
                                    || (first == last && end_page.is_some_and(|p| p < first_page))
                                {
                                    continue;
                                }
                                if first == last
                                    && !probe.select_raw_anchors(&start, end).is_some_and(
                                        |selection| {
                                            selection.start == start && selection.end == *end
                                        },
                                    )
                                {
                                    continue;
                                }
                            }
                            if flags != 0 {
                                result.marks.push((first, flags));
                            }
                            if let Some(saved) = quote
                                && verify_selection(&recipe, &cancel, first, last, &saved)
                            {
                                result.quotes.push(saved);
                            }
                        }
                    }
                }
                if !cancel.load(Ordering::Relaxed) {
                    let _ = tx.send(result);
                }
            })
            .is_err()
        {
            self.next = None;
            return;
        }
        self.receiver = Some(rx);
        self.next = Some(now + POLL);
    }

    pub(super) fn poll(&mut self, now: Instant, current: &SearchLayoutRecipe) -> bool {
        if &self.recipe != current {
            self.cancel.store(true, Ordering::Relaxed);
            self.receiver = None;
            self.cursor = self.total;
            self.next = None;
            self.bins.fill(0);
            self.history.clear();
            self.quotes.clear();
            return true;
        }
        let Some(receiver) = &self.receiver else {
            return false;
        };
        match receiver.try_recv() {
            Ok(result) => {
                self.receiver = None;
                for (chapter, flags) in result.marks {
                    if chapter < self.chapters {
                        let bin = chapter.saturating_mul(BINS - 1)
                            / self.chapters.saturating_sub(1).max(1);
                        self.bins[bin] |= flags;
                    }
                }
                self.history.extend(result.history);
                self.quotes.extend(result.quotes);
                self.wake(now);
                true
            }
            Err(mpsc::TryRecvError::Empty) => {
                self.next = Some(now + POLL);
                false
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.receiver = None;
                self.wake(now);
                false
            }
        }
    }
}

fn verify_selection(
    recipe: &SearchLayoutRecipe,
    cancel: &AtomicBool,
    first: usize,
    last: usize,
    saved: &TextSelection,
) -> bool {
    if first > last
        || last.saturating_sub(first) >= 8
        || saved.start.book != saved.end.book
        || saved.text.len() > crate::library::MAX_HIGHLIGHT_TEXT_BYTES
    {
        return false;
    }
    if recipe
        .verify_quote_span(&saved.start, &saved.end, first, last, cancel)
        .is_err()
    {
        return false;
    }
    let raw = saved.start.raw_caret.is_some() || saved.end.raw_caret.is_some();
    let mut text = String::new();
    for index in first..=last {
        if cancel.load(Ordering::Relaxed) {
            return false;
        }
        let Ok(probe) = recipe.chapter(index, cancel) else {
            return false;
        };
        let fragment = if raw {
            probe.raw_chapter_fragment(&saved.start, &saved.end)
        } else {
            probe.select_source_range(&saved.start, &saved.end)
        };
        let Some(fragment) = fragment else {
            return false;
        };
        let joins = probe
            .odt_continued_before
            .get(index)
            .copied()
            .unwrap_or(false);
        let extra = usize::from(!text.is_empty() && !joins);
        if text
            .len()
            .saturating_add(extra)
            .saturating_add(fragment.text.len())
            > crate::library::MAX_HIGHLIGHT_TEXT_BYTES
        {
            return false;
        }
        if extra != 0 {
            text.push('\n');
        }
        text.push_str(&fragment.text);
    }
    !text.trim().is_empty() && text == saved.text && !cancel.load(Ordering::Relaxed)
}
