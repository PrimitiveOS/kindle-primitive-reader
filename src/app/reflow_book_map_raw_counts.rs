//! Bounded background verification of raw saved marks for the reflow Book Map.
use super::{Reader, reflow_book_map_marks::Counts};
use crate::Anchor;
use crate::session::SearchLayoutRecipe;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};

const SCAN: usize = 128;
const SLICE: usize = 4;
const POLL: Duration = Duration::from_millis(200);

struct Task {
    first: usize,
    last: usize,
    start: Anchor,
    end: Option<Anchor>,
    note: bool,
}
struct ResultItem {
    chapter: usize,
    highlight: bool,
    note: bool,
}

pub(super) struct Verification {
    recipe: SearchLayoutRecipe,
    total: usize,
    cursor: usize,
    cancelled: Arc<AtomicBool>,
    receiver: Option<mpsc::Receiver<Vec<ResultItem>>>,
    deadline: Option<Instant>,
    counts: Vec<Counts>,
}

impl Drop for Verification {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

impl Verification {
    pub(super) fn deadline(&self) -> Option<Instant> {
        self.deadline
    }
    pub(super) fn counts(&self) -> &[Counts] {
        &self.counts
    }
    fn pending(&self) -> bool {
        self.receiver.is_some()
    }
    fn wake(&mut self, now: Instant) {
        self.deadline = (self.cursor < self.total).then_some(now + POLL);
    }

    fn submit(&mut self, tasks: Vec<Task>, now: Instant) {
        if tasks.is_empty() {
            self.wake(now);
            return;
        }
        let recipe = self.recipe.clone();
        let cancelled = Arc::clone(&self.cancelled);
        let (tx, rx) = mpsc::sync_channel(1);
        if std::thread::Builder::new()
            .name("reader-map-marks".into())
            .spawn(move || {
                let mut output = Vec::new();
                for task in tasks {
                    if cancelled.load(Ordering::Relaxed) {
                        return;
                    }
                    let Ok(probe) = recipe.chapter(task.first, &cancelled) else {
                        continue;
                    };
                    let Some(first_page) = probe.page_for(&task.start) else {
                        continue;
                    };
                    if let Some(end) = &task.end {
                        let end_page = if task.first == task.last {
                            probe.page_for(end)
                        } else {
                            recipe
                                .chapter(task.last, &cancelled)
                                .ok()
                                .and_then(|probe| probe.page_for(end))
                        };
                        if end_page.is_none()
                            || (task.first == task.last
                                && end_page.is_some_and(|page| page < first_page))
                        {
                            continue;
                        }
                        if task.first == task.last
                            && !probe.select_raw_anchors(&task.start, end).is_some_and(
                                |selection| selection.start == task.start && selection.end == *end,
                            )
                        {
                            continue;
                        }
                    }
                    output.push(ResultItem {
                        chapter: task.first,
                        highlight: task.end.is_some(),
                        note: task.note,
                    });
                }
                if !cancelled.load(Ordering::Relaxed) {
                    let _ = tx.send(output);
                }
            })
            .is_err()
        {
            self.deadline = None;
            return;
        }
        self.receiver = Some(rx);
        self.deadline = Some(now + POLL);
    }

    fn poll(&mut self, now: Instant, current: &SearchLayoutRecipe) -> bool {
        if self.recipe != *current {
            self.cancelled.store(true, Ordering::Relaxed);
            self.receiver = None;
            self.cursor = self.total;
            self.deadline = None;
            self.counts.fill(Counts::default());
            return true;
        }
        let Some(receiver) = &self.receiver else {
            return false;
        };
        match receiver.try_recv() {
            Ok(result) => {
                self.receiver = None;
                let changed = !result.is_empty();
                for item in result {
                    if let Some(count) = self.counts.get_mut(item.chapter) {
                        if item.highlight {
                            count.highlights = count.highlights.saturating_add(1);
                        } else {
                            count.bookmarks = count.bookmarks.saturating_add(1);
                        }
                        if item.note {
                            count.notes = count.notes.saturating_add(1);
                        }
                    }
                }
                self.wake(now);
                changed
            }
            Err(mpsc::TryRecvError::Empty) => {
                self.deadline = Some(now + POLL);
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

impl Reader {
    pub(super) fn start_book_map_raw_counts(&mut self) {
        self.book_map_raw_counts = None;
        if self.fixed.is_some() {
            return;
        }
        let Some(active) = &self.active else {
            return;
        };
        let Some(book) = self.library.state(active.id) else {
            return;
        };
        let reading = &active.reading;
        let total = book.bookmarks.len().saturating_add(book.highlights.len());
        let recipe = reading.config.search_layout_recipe(&reading.probe);
        self.book_map_raw_counts = Some(Verification {
            recipe,
            total,
            cursor: 0,
            cancelled: Arc::new(AtomicBool::new(false)),
            receiver: None,
            deadline: Some(Instant::now()),
            counts: vec![Counts::default(); reading.probe.chapter_count()],
        });
        self.queue_book_map_raw_counts(Instant::now());
    }

    fn queue_book_map_raw_counts(&mut self, now: Instant) {
        let Some(mut job) = self.book_map_raw_counts.take() else {
            return;
        };
        if job.pending() {
            self.book_map_raw_counts = Some(job);
            return;
        }
        let Some(active) = &self.active else {
            return;
        };
        let Some(book) = self.library.state(active.id) else {
            return;
        };
        if book.bookmarks.len().saturating_add(book.highlights.len()) != job.total {
            return;
        }
        let reading = &active.reading;
        let probe = &reading.probe;
        let mut tasks = Vec::new();
        let mut scanned = 0;
        while job.cursor < job.total && scanned < SCAN && tasks.len() < SLICE {
            let index = job.cursor;
            job.cursor += 1;
            scanned += 1;
            if index < book.bookmarks.len() {
                let mark = &book.bookmarks[index];
                let Some(anchor) = &mark.position.anchor else {
                    continue;
                };
                if anchor.raw_caret.is_none() || anchor.book != probe.book {
                    continue;
                }
                let Some(chapter) = probe.chapter_index(&anchor.chapter) else {
                    continue;
                };
                if chapter == reading.chapter {
                    continue;
                }
                tasks.push(Task {
                    first: chapter,
                    last: chapter,
                    start: anchor.clone(),
                    end: None,
                    note: mark.note.as_ref().is_some_and(|s| !s.trim().is_empty()),
                });
            } else {
                let mark = &book.highlights[index - book.bookmarks.len()];
                let start = &mark.selection.start;
                let end = &mark.selection.end;
                if start.raw_caret.is_none() && end.raw_caret.is_none() {
                    continue;
                }
                if start.book != probe.book || end.book != probe.book {
                    continue;
                }
                let (Some(first), Some(last)) = (
                    probe.chapter_index(&start.chapter),
                    probe.chapter_index(&end.chapter),
                ) else {
                    continue;
                };
                if first > last || (first == reading.chapter && last == reading.chapter) {
                    continue;
                }
                tasks.push(Task {
                    first,
                    last,
                    start: start.clone(),
                    end: Some(end.clone()),
                    note: mark.note.as_ref().is_some_and(|s| !s.trim().is_empty()),
                });
            }
        }
        job.submit(tasks, now);
        self.book_map_raw_counts = Some(job);
    }

    pub(super) fn poll_book_map_raw_counts(
        &mut self,
        now: Instant,
    ) -> Option<primitive_app::Change> {
        if !self
            .book_map_raw_counts
            .as_ref()
            .and_then(Verification::deadline)
            .is_some_and(|at| at <= now)
        {
            return None;
        }
        let Some(active) = &self.active else {
            self.book_map_raw_counts = None;
            return Some(primitive_app::Change::Partial);
        };
        let recipe = active
            .reading
            .config
            .search_layout_recipe(&active.reading.probe);
        let changed = self
            .book_map_raw_counts
            .as_mut()
            .is_some_and(|job| job.poll(now, &recipe));
        self.queue_book_map_raw_counts(now);
        let finished = self
            .book_map_raw_counts
            .as_ref()
            .and_then(Verification::deadline)
            .is_none();
        (changed || finished).then_some(primitive_app::Change::Partial)
    }
}
