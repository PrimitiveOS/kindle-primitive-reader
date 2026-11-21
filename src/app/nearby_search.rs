//! Directional reflow search. A single worker serializes reads, discards replaced
//! requests and leaves the reading/keyboard loop free to handle cancellation.
use super::*;
use crate::Anchor;
use crate::search::{ChapterText, NearbySearch, SearchDirection};
use std::sync::{
    Arc, Condvar, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc,
};

const PREVIOUS: Rect = Rect::new(LIST_X, LIST_Y + 364, 555, 100);
const NEXT_HERE: Rect = Rect::new(LIST_X + 579, LIST_Y + 364, 555, 100);

struct Request {
    engine: NearbySearch,
    path: PathBuf,
    chapters: usize,
    chapter: usize,
    anchor: Anchor,
    cancelled: Arc<AtomicBool>,
    output: mpsc::Sender<Result<Option<SearchHit>>>,
}

fn load_text(path: &Path, ci: usize) -> Result<ChapterText> {
    // Only owned text escapes the storage lease; matching never holds the device storage lock.
    Ok(with_storage(|| {
        let mut book = Book::open(path).map_err(io::Error::other)?;
        ChapterText::read(&mut book, ci).map_err(io::Error::other)
    })?)
}

impl Request {
    fn run(self) {
        let found = self.engine.scan(
            self.chapter,
            &self.anchor,
            &self.cancelled,
            self.chapters,
            |ci| load_text(&self.path, ci),
        );
        if !self.cancelled.load(Ordering::Relaxed) {
            let _ = self.output.send(found);
        }
    }
}

#[derive(Default)]
struct Mailbox {
    next: Option<Box<dyn FnOnce() + Send>>,
    closed: bool,
}
#[derive(Default)]
struct SearchQueue {
    mailbox: Mutex<Mailbox>,
    ready: Condvar,
}

struct Worker(Arc<SearchQueue>);

impl Worker {
    fn start() -> io::Result<Self> {
        let queue = Arc::new(SearchQueue::default());
        let thread = Arc::clone(&queue);
        std::thread::Builder::new()
            .name("reader-nearby-search".into())
            .spawn(move || {
                let SearchQueue { mailbox, ready } = &*thread;
                loop {
                    let mut queue = mailbox.lock().unwrap();
                    while queue.next.is_none() && !queue.closed {
                        queue = ready.wait(queue).unwrap();
                    }
                    if queue.closed {
                        break;
                    }
                    let request = queue.next.take().unwrap();
                    drop(queue);
                    request();
                }
            })?;
        Ok(Self(queue))
    }
    fn submit(&self, request: impl FnOnce() + Send + 'static) {
        self.0.mailbox.lock().unwrap().next = Some(Box::new(request));
        self.0.ready.notify_one();
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        let mut queue = self.0.mailbox.lock().unwrap();
        queue.next = None;
        queue.closed = true;
        self.0.ready.notify_one();
    }
}

enum AllEvent {
    Hits(Vec<SearchHit>),
    Finished {
        limited: bool,
        memory_limited: bool,
        skipped: bool,
    },
}
struct PendingAll {
    cancel: Arc<AtomicBool>,
    result: mpsc::Receiver<Result<AllEvent>>,
    at: Instant,
}
impl Drop for PendingAll {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

struct Pending {
    query: SearchQuery,
    direction: SearchDirection,
    continuing: bool,
    cancelled: Arc<AtomicBool>,
    result: mpsc::Receiver<Result<Option<SearchHit>>>,
    at: Instant,
}
impl Drop for Pending {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}
#[derive(Default)]
pub(super) struct Nearby {
    worker: Option<Worker>,
    pending: Option<Pending>,
    all: Option<PendingAll>,
    current: Option<SearchQuery>,
    end: Option<SearchDirection>,
}

impl Nearby {
    pub(super) fn deadline(&self) -> Option<Instant> {
        self.pending
            .as_ref()
            .map(|p| p.at)
            .or_else(|| self.all.as_ref().map(|p| p.at))
    }
    pub(super) fn searching_direction(&self) -> bool {
        self.pending.is_some()
    }
    pub(super) fn searching_all(&self) -> bool {
        self.all.is_some()
    }
    pub(super) fn cancel(&mut self) {
        self.pending = None;
        self.all = None;
    }
    pub(super) fn clear(&mut self) {
        self.cancel();
        self.current = None;
        self.end = None;
    }
    pub(super) fn result(&self) -> bool {
        self.current.is_some()
    }
    pub(super) fn can_step(&self, direction: SearchDirection) -> bool {
        self.current.is_some() && self.end != Some(direction)
    }
}

impl Reader {
    pub(super) fn nearby_input(&mut self, x: f32, y: f32) -> Option<Change> {
        if self.active.is_none() {
            return None;
        }
        let direction = if PREVIOUS.contains(x, y) {
            SearchDirection::Previous
        } else if NEXT_HERE.contains(x, y) {
            SearchDirection::Next
        } else {
            return None;
        };
        if self.nearby.deadline().is_some() {
            self.nearby.cancel();
            self.notice = None;
            return Some(Change::Partial);
        }
        Some(match self.begin_nearby(direction, false) {
            Ok(()) => Change::Partial,
            Err(e) => self.report(e),
        })
    }
    pub(super) fn draw_nearby(&self, canvas: &mut Canvas) {
        if self.active.is_none() {
            return;
        }
        if self.nearby.pending.is_some() {
            canvas.button(PREVIOUS, "Cancel search", false, false);
        } else {
            canvas.button(PREVIOUS, "Find previous", false, false);
            canvas.button(NEXT_HERE, "Find next", false, false);
        }
    }
    pub(super) fn begin_nearby(
        &mut self,
        direction: SearchDirection,
        continuing: bool,
    ) -> Result<()> {
        let active = self.active.as_ref().ok_or("Open a reflowable book first")?;
        let query = if continuing {
            self.nearby.current.clone().ok_or("No current search")?
        } else {
            SearchQuery {
                text: self.editor.string(),
                match_case: self.find_match_case,
                ignore_accents: self.find_ignore_accents,
                regular_expression: self.find_regex,
            }
        };
        let engine = if query.regular_expression {
            NearbySearch::with_regular_expression(&query.text, query.match_case, direction)?
        } else {
            NearbySearch::new(
                &query.text,
                query.match_case,
                query.ignore_accents,
                direction,
            )?
        };
        let anchor = if continuing {
            let selection = self
                .result_selection
                .as_ref()
                .ok_or("The current match has moved")?;
            match direction {
                SearchDirection::Previous => selection.start.clone(),
                SearchDirection::Next => selection.end.clone(),
            }
        } else {
            active
                .reading
                .search_boundary(direction)
                .ok_or("The current page has no searchable position")?
        };
        let (tx, rx) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let request = Request {
            engine,
            path: active.reading.config.book.clone(),
            chapter: active.reading.chapter,
            chapters: active.reading.probe.chapter_count(),
            anchor,
            cancelled: Arc::clone(&cancelled),
            output: tx,
        };
        if !continuing {
            self.update_lookup_history(|h| {
                h.search(
                    &query.text,
                    query.match_case,
                    query.ignore_accents,
                    query.regular_expression,
                )
            })?;
            self.cancel_search();
            self.search_result = None;
        }
        if self.nearby.worker.is_none() {
            self.nearby.worker = Some(Worker::start()?);
        }
        self.nearby.pending = Some(Pending {
            query,
            direction,
            continuing,
            cancelled,
            result: rx,
            at: Instant::now() + SEARCH_DELAY,
        });
        self.nearby
            .worker
            .as_ref()
            .unwrap()
            .submit(move || request.run());
        self.notice = Some(
            match direction {
                SearchDirection::Previous => "Searching earlier text",
                SearchDirection::Next => "Searching later text",
            }
            .into(),
        );
        Ok(())
    }
    pub(super) fn start_all_reflow(&mut self, pattern: &str) -> Result<()> {
        let case = self.find_match_case;
        let regex = self.find_regex;
        let accents = self.find_ignore_accents && !regex;
        let mut search = if regex {
            Search::with_regular_expression(pattern, case)?
        } else {
            Search::with_options(pattern, case, accents)?
        };
        let active = self.active.as_ref().ok_or("Open a reflowable book first")?;
        let path = active.reading.config.book.clone();
        let chapters = active.reading.probe.chapter_count();
        self.update_lookup_history(|h| h.search(pattern, case, accents, regex))?;
        self.cancel_search();
        self.show(Screen::Matches);
        self.hits.clear();
        self.pdf_hits.clear();
        self.search_result = None;
        if self.nearby.worker.is_none() {
            self.nearby.worker = Some(Worker::start()?);
        }
        let (tx, result) = mpsc::sync_channel(1);
        let cancelled = Arc::new(AtomicBool::new(false));
        self.nearby.all = Some(PendingAll {
            cancel: Arc::clone(&cancelled),
            result,
            at: Instant::now() + SEARCH_DELAY,
        });
        self.nearby.worker.as_ref().unwrap().submit(move || {
            let scanned = (|| {
                for ci in 0..chapters {
                    if cancelled.load(Ordering::Relaxed) || search.is_finished() {
                        break;
                    }
                    let results =
                        search.step_loaded(load_text(&path, ci)?, chapters, &cancelled)?;
                    let mut hits = results.into_iter().peekable();
                    // At most one delivery waits in the mailbox. Dropping the
                    // receiver when leaving Find releases a blocked sender.
                    while hits.peek().is_some() && !cancelled.load(Ordering::Relaxed) {
                        let batch = hits.by_ref().take(128).collect();
                        if tx.send(Ok(AllEvent::Hits(batch))).is_err() {
                            break;
                        }
                    }
                }
                Ok(AllEvent::Finished {
                    limited: search.limit_reached(),
                    memory_limited: search.memory_limited(),
                    skipped: search.skipped_chapters() > 0,
                })
            })();
            if !cancelled.load(Ordering::Relaxed) {
                let _ = tx.send(scanned);
            }
        });
        Ok(())
    }

    pub(super) fn poll_nearby(&mut self, now: Instant) -> Result<Change> {
        if let Some(all) = &mut self.nearby.all {
            let outcome = match all.result.try_recv() {
                Ok(result) => result,
                Err(mpsc::TryRecvError::Empty) => {
                    all.at = now + SEARCH_DELAY;
                    return Ok(Change::None);
                }
                Err(mpsc::TryRecvError::Disconnected) => Err("The search stopped".into()),
            };
            match outcome {
                Ok(AllEvent::Hits(batch)) => {
                    self.hits.extend(batch);
                    all.at = now + SEARCH_DELAY;
                    if now < self.next_search_refresh {
                        return Ok(Change::None);
                    }
                    self.next_search_refresh = now + SEARCH_REFRESH;
                }
                Ok(AllEvent::Finished {
                    limited,
                    memory_limited,
                    skipped,
                }) => {
                    self.nearby.all = None;
                    if self.screen == Screen::Matches {
                        self.notice = if memory_limited {
                            Some("Search result memory limit reached".into())
                        } else if limited {
                            Some("Showing first 5,000 matches".into())
                        } else if skipped {
                            Some("Some chapters could not be searched".into())
                        } else {
                            None
                        };
                    }
                }
                Err(e) => {
                    self.nearby.all = None;
                    return Err(e);
                }
            }
            return Ok(Change::Partial);
        }
        let pending = self.nearby.pending.as_mut().ok_or("No active search")?;
        let outcome = match pending.result.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => {
                pending.at = now + SEARCH_DELAY;
                return Ok(Change::None);
            }
            Err(mpsc::TryRecvError::Disconnected) => Err("The search stopped".into()),
        };
        let pending = self.nearby.pending.take().unwrap();
        let Some(hit) = outcome? else {
            self.nearby.end = Some(pending.direction);
            self.notice = Some(
                match pending.direction {
                    SearchDirection::Previous => "No earlier match",
                    SearchDirection::Next => "No later match",
                }
                .into(),
            );
            return Ok(Change::Partial);
        };
        if !pending.continuing {
            self.search_result = None;
        }
        self.hits = vec![hit];
        self.activate(Action::SearchHit(0))?;
        self.nearby.current = Some(pending.query.clone());
        self.nearby.end = None;
        Ok(Change::Partial)
    }
}
