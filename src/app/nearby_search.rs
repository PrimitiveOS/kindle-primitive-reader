//! Reflow searches share one cancellable worker. Directional PDF and DjVu searches
//! reuse the open document and yield between physical pages.
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

#[derive(Clone, Copy)]
enum FixedNavigation {
    First,
    Previous,
    Next,
    Last,
}

impl FixedNavigation {
    const ITEMS: [Self; 4] = [Self::First, Self::Previous, Self::Next, Self::Last];

    fn direction(self) -> SearchDirection {
        match self {
            Self::First | Self::Next => SearchDirection::Next,
            Self::Previous | Self::Last => SearchDirection::Previous,
        }
    }

    fn from_edge(self) -> bool {
        matches!(self, Self::First | Self::Last)
    }

    fn entry(self) -> (Rect, &'static str) {
        let label = match self {
            Self::First => "From start",
            Self::Previous => "Previous",
            Self::Next => "Next",
            Self::Last => "From end",
        };
        (
            Rect::new(LIST_X + self as i32 * 294, PREVIOUS.y, 276, PREVIOUS.h),
            label,
        )
    }

    fn button(self) -> Rect {
        Rect::new(412 + self as i32 * 104, MATCH_PREV.y, 96, MATCH_PREV.h)
    }

    fn icon(self) -> primitive_ui::Icon {
        let stroke: &'static [&'static str] = match self {
            Self::First => &["M5 5 V19", "M17 5 L10 12 L17 19"],
            Self::Previous => &["M15 5 L8 12 L15 19"],
            Self::Next => &["M9 5 L16 12 L9 19"],
            Self::Last => &["M19 5 V19", "M7 5 L14 12 L7 19"],
        };
        primitive_ui::Icon { stroke, fill: &[] }
    }
}

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
struct FixedPending {
    search: PdfSearch,
    query: SearchQuery,
    direction: SearchDirection,
    next_page: Option<usize>,
    stop_before: Option<usize>,
    continuing: bool,
    at: Instant,
}

fn adjacent_page(page: usize, count: usize, direction: SearchDirection) -> Option<usize> {
    match direction {
        SearchDirection::Previous => page.checked_sub(1),
        SearchDirection::Next => page.checked_add(1).filter(|&next| next < count),
    }
}

#[derive(Default)]
pub(super) struct Nearby {
    worker: Option<Worker>,
    pending: Option<Pending>,
    fixed: Option<FixedPending>,
    all: Option<PendingAll>,
    current: Option<SearchQuery>,
    end: Option<SearchDirection>,
}

impl Nearby {
    pub(super) fn deadline(&self) -> Option<Instant> {
        self.pending
            .as_ref()
            .map(|p| p.at)
            .or_else(|| self.fixed.as_ref().map(|p| p.at))
            .or_else(|| self.all.as_ref().map(|p| p.at))
    }
    pub(super) fn searching_direction(&self) -> bool {
        self.pending.is_some() || self.fixed.is_some()
    }
    pub(super) fn searching_all(&self) -> bool {
        self.all.is_some()
    }
    pub(super) fn cancel(&mut self) {
        self.pending = None;
        self.fixed = None;
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
    fn supports_nearby_search(&self) -> bool {
        self.active.is_some()
            || self
                .fixed
                .as_ref()
                .is_some_and(|book| book.reading.supports_text())
    }

    pub(super) fn nearby_input(&mut self, x: f32, y: f32) -> Option<Change> {
        if !self.supports_nearby_search() {
            return None;
        }
        if self.fixed.is_some() && !self.nearby.searching_direction() {
            let step = FixedNavigation::ITEMS
                .into_iter()
                .find(|s| s.entry().0.contains(x, y))?;
            return Some(self.fixed_navigation(step, false));
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
        if !self.supports_nearby_search() {
            return;
        }
        if self.nearby.searching_direction() {
            canvas.button(PREVIOUS, "Cancel search", false, false);
        } else if self.fixed.is_some() {
            for step in FixedNavigation::ITEMS {
                let (rect, label) = step.entry();
                canvas.button(rect, label, false, false);
            }
        } else {
            canvas.button(PREVIOUS, "Find previous", false, false);
            canvas.button(NEXT_HERE, "Find next", false, false);
        }
    }
    fn fixed_navigation(&mut self, step: FixedNavigation, continuing: bool) -> Change {
        match self.begin_search_from(step.direction(), continuing, step.from_edge()) {
            Ok(()) => Change::Partial,
            Err(error) => self.report(error),
        }
    }

    pub(super) fn fixed_result_navigation(&self) -> bool {
        self.fixed.is_some() && self.nearby.result() && self.search_result.is_some()
    }

    pub(super) fn fixed_result_input(&mut self, x: f32, y: f32) -> Option<Change> {
        let step = FixedNavigation::ITEMS
            .into_iter()
            .find(|s| s.button().contains(x, y))?;
        Some(
            if step.from_edge() || self.nearby.can_step(step.direction()) {
                self.fixed_navigation(step, true)
            } else {
                Change::None
            },
        )
    }

    pub(super) fn draw_fixed_result_navigation(&self, canvas: &mut Canvas) -> bool {
        if !self.fixed_result_navigation() {
            return false;
        }
        for step in FixedNavigation::ITEMS {
            let rect = step.button();
            let active = step.from_edge() || self.nearby.can_step(step.direction());
            let pressed = active && canvas.control_pressed(rect, false);
            canvas.panel(rect, 12, 2, if pressed { 0 } else { 255 });
            let ink = if pressed {
                255
            } else if active {
                0
            } else {
                150
            };
            canvas.icon(
                Rect::new(
                    rect.x + (rect.w - 40) / 2,
                    rect.y + (rect.h - 40) / 2,
                    40,
                    40,
                ),
                step.icon(),
                ink,
            );
        }
        true
    }

    pub(super) fn begin_nearby(
        &mut self,
        direction: SearchDirection,
        continuing: bool,
    ) -> Result<()> {
        self.begin_search_from(direction, continuing, false)
    }

    fn begin_search_from(
        &mut self,
        direction: SearchDirection,
        continuing: bool,
        from_edge: bool,
    ) -> Result<()> {
        let query = if continuing {
            self.nearby.current.clone().ok_or("No current search")?
        } else {
            SearchQuery {
                text: self.editor.string(),
                match_case: self.find_match_case,
                ignore_accents: self.find_ignore_accents,
                regular_expression: self.active.is_some() && self.find_regex,
            }
        };
        if let Some(fixed) = &self.fixed {
            if !fixed.reading.supports_text() {
                return Err("This document has no searchable text layer".into());
            }
            let search =
                PdfSearch::with_options(&query.text, query.match_case, query.ignore_accents)?;
            let page = fixed.reading.page();
            let stop_before = from_edge.then_some(page);
            let next_page = if from_edge {
                // KOpt's origin -1 searches the opposite side of the current page.
                let edge = match direction {
                    SearchDirection::Next => 0,
                    SearchDirection::Previous => fixed.reading.count() - 1,
                };
                (edge != page).then_some(edge)
            } else if continuing {
                adjacent_page(page, fixed.reading.count(), direction)
            } else {
                Some(page)
            };
            if !continuing {
                self.update_lookup_history(|h| {
                    h.search(&query.text, query.match_case, query.ignore_accents, false)
                })?;
                self.cancel_search();
                self.search_result = None;
            }
            self.nearby.fixed = Some(FixedPending {
                search,
                query,
                direction,
                next_page,
                stop_before,
                continuing,
                at: Instant::now(),
            });
            self.notice = Some("Searching pages".into());
            return Ok(());
        }
        let active = self.active.as_ref().ok_or("Open a book first")?;
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
        if let Some(mut pending) = self.nearby.fixed.take() {
            let fixed = self.fixed.as_ref().ok_or("No open document")?;
            let Some(page) = pending.next_page else {
                let exhausted = match (pending.direction, pending.stop_before.is_some()) {
                    (SearchDirection::Next, true) => SearchDirection::Previous,
                    (SearchDirection::Previous, true) => SearchDirection::Next,
                    (direction, false) => direction,
                };
                self.nearby.end = Some(exhausted);
                let message = if pending.search.skipped > 0 {
                    "No match found. Some pages could not be searched"
                } else {
                    match exhausted {
                        SearchDirection::Previous => "No earlier matching page",
                        SearchDirection::Next => "No later matching page",
                    }
                };
                self.notice = Some(message.into());
                return Ok(Change::Partial);
            };
            // Reuse the open provider and its cached OCR. Yield after every page so
            // search can be cancelled without opening a second copy of a large PDF.
            let found = match fixed.reading.page_text(page) {
                Ok(text) => pending.search.page_match(&text, pending.direction),
                Err(_) => {
                    pending.search.skipped += 1;
                    None
                }
            };
            let Some(found) = found else {
                pending.next_page = adjacent_page(page, fixed.reading.count(), pending.direction)
                    .filter(|next| Some(*next) != pending.stop_before);
                pending.at = now + SEARCH_DELAY;
                self.nearby.fixed = Some(pending);
                return Ok(Change::None);
            };
            // Keep all matching words highlighted, while a zoomed viewport
            // follows the first occurrence forward or the last one backward.
            let previous = self.jump_fixed(page, FixedJump::regions(&found.focus))?;
            if !pending.continuing {
                self.push_history(previous);
            }
            self.pdf_result_regions = found
                .regions
                .into_iter()
                .map(|bounds| (page, bounds))
                .collect();
            if found.limited {
                self.notice = Some("Showing the first 5,000 matches on this page".into());
            }
            self.search_result = Some(0);
            self.nearby.current = Some(pending.query);
            self.nearby.end = None;
            return Ok(Change::Partial);
        }
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

#[cfg(test)]
mod fixed_tests {
    use super::*;
    use crate::app::tests::{Fixture, capture_test_screen};

    fn coastal_guide() -> Vec<u8> {
        let pages = [
            ("Buoy register", vec!["beacon ".repeat(8); 15]),
            (
                "Outer harbour",
                vec!["A beacon guards the harbour mouth.".into()],
            ),
            (
                "Night passage",
                vec![
                    "The beacon stands above the tide.".into(),
                    "A second beacon marks the channel.".into(),
                ],
            ),
            (
                "Tidal notes",
                vec![
                    "Slack water begins before dawn.".into(),
                    "Fine sand covers the sandbar at low water.".into(),
                    "Quicksand and quicksands shift here.".into(),
                    "Sand, shells and quicksand gather.".into(),
                    "A sand-sand boundary marks the inlet.".into(),
                ],
            ),
            (
                "Arrival",
                vec!["The beacon signals the final approach.".into()],
            ),
        ];
        let mut objects = vec![
            "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
            "<< /Type /Pages /Count 5 /Kids [4 0 R 6 0 R 8 0 R 10 0 R 12 0 R] /Resources << /Font << /F1 3 0 R >> >> >>".to_owned(),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_owned(),
        ];
        for (heading, lines) in pages {
            let stream_no = objects.len() + 2;
            objects.push(format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 360 480] /Contents {stream_no} 0 R >>"
            ));
            let mut content = format!(
                "BT /F1 12 Tf 28 440 Td (COASTAL FIELD GUIDE) Tj 0 -44 Td /F1 22 Tf ({heading}) Tj 0 -38 Td /F1 13 Tf"
            );
            for line in lines {
                content.push_str(&format!(" ({line}) Tj 0 -20 Td"));
            }
            content.push_str(" ET\n");
            objects.push(format!(
                "<< /Length {} >>\nstream\n{content}endstream",
                content.len()
            ));
        }
        crate::fixed::tests::pdf_objects(&objects)
    }

    fn tap(app: &mut Reader, rect: Rect) -> Change {
        app.input(Input::Tap(
            (rect.x + rect.w / 2) as f32,
            (rect.y + rect.h / 2) as f32,
        ))
    }

    fn settle(app: &mut Reader) {
        let stop = Instant::now() + Duration::from_secs(90);
        while app.nearby.searching_direction() && Instant::now() < stop {
            app.poll_nearby(Instant::now()).unwrap();
        }
        assert!(!app.nearby.searching_direction());
    }

    #[test]
    fn finds_pdf_pages_from_the_visible_passage_and_restores_its_viewport() {
        let fixture = Fixture::new();
        let external = std::env::var_os("READER_TEST_FIXED_SEARCH_DOCUMENT").map(PathBuf::from);
        let authored = external.is_none();
        let path = external.unwrap_or_else(|| {
            let path = fixture.root.join("Coastal field guide.pdf");
            fs::write(&path, coastal_guide()).unwrap();
            path
        });
        let query =
            std::env::var("READER_TEST_FIXED_SEARCH_QUERY").unwrap_or_else(|_| "beacon".into());
        let expected =
            std::env::var("READER_TEST_FIXED_SEARCH_PAGES").unwrap_or_else(|_| "3,2,5".into());
        let expected: Vec<usize> = expected
            .split(',')
            .map(|n| n.parse::<usize>().unwrap() - 1)
            .collect();
        assert_eq!(expected.len(), 3);
        let (origin, before, after) = (expected[0], expected[1], expected[2]);
        let font = crate::test_fixture("fonts/NotoSans-Regular.ttf");
        let mut app =
            Reader::open(path.parent().unwrap(), &fixture.state, vec![font.clone()]).unwrap();
        app.open_path(&path).unwrap();
        app.fixed.as_mut().unwrap().reading.jump(origin).unwrap();
        let old = app.current_history_location().unwrap();
        if authored || std::env::var_os("READER_TEST_FIXED_SEARCH_OLD_LIST").is_some() {
            app.start_document_search(&query).unwrap();
            while app.search_at.is_some() {
                app.step_pdf_search(Instant::now()).unwrap();
            }
            assert_eq!(app.pdf_hits.len(), MAX_SEARCH_HITS);
            assert!(app.pdf_hits.last().unwrap().page < origin);
        }
        app.show(Screen::Find);
        app.editor.text = query.chars().collect();
        app.editor.caret = app.editor.text.len();
        if authored {
            capture_test_screen(
                &app,
                std::slice::from_ref(&font),
                "READER_FIXED_FIND_CONTROLS",
                false,
            );
        }
        assert_eq!(
            tap(&mut app, FixedNavigation::Next.entry().0),
            Change::Partial
        );
        settle(&mut app);
        assert_eq!(app.screen, Screen::Fixed);
        let page = |app: &Reader| app.fixed.as_ref().unwrap().reading.page();
        assert_eq!(page(&app), origin);
        let marks = app.pdf_result_regions.len();
        assert!(marks > 0);
        if authored {
            assert_eq!(marks, 2);
            capture_test_screen(
                &app,
                std::slice::from_ref(&font),
                "READER_FIXED_FIND_MATCH",
                false,
            );
        }
        let mut visited = vec![origin + 1];
        let mut word_boxes = vec![marks];
        for (button, expected) in [
            (FixedNavigation::Next.button(), after),
            (FixedNavigation::Previous.button(), origin),
            (FixedNavigation::Previous.button(), before),
        ] {
            assert_eq!(tap(&mut app, button), Change::Partial);
            settle(&mut app);
            assert_eq!(page(&app), expected);
            assert!(app.pdf_result_regions.iter().all(|(p, _)| *p == expected));
            visited.push(expected + 1);
            word_boxes.push(app.pdf_result_regions.len());
        }
        app.activate(Action::ReturnToLocation).unwrap();
        assert_eq!(app.current_history_location(), Some(old));
        if let Some(file) = std::env::var_os("READER_TEST_FIXED_SEARCH_REPORT") {
            let result = serde_json::json!({"visited":visited, "word_boxes":word_boxes,"viewport_restored":true});
            fs::write(file, serde_json::to_vec(&result).unwrap()).unwrap();
        }
    }

    #[test]
    fn finds_a_printed_ellipsis_with_the_fixed_document_controls() {
        let fixture = Fixture::new();
        let path = fixture.root.join("Harbour timetable.pdf");
        // U+2026 is 0x85 in the PDF WinAnsi encoding.
        let content = r"BT /F1 11 Tf 28 440 Td (HARBOUR TIMETABLE) Tj 0 -46 Td /F1 22 Tf (Gate notices) Tj 0 -43 Td /F1 13 Tf (Boarding\205 Please wait at gate four.) Tj 0 -25 Td (Departures resume when the tide turns.) Tj ET";
        let pdf = crate::fixed::tests::pdf_objects(&[
            "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
            "<< /Type /Pages /Count 1 /Kids [3 0 R] >>".to_owned(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 360 480] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>".to_owned(),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".to_owned(),
            format!("<< /Length {} >>\nstream\n{content}\nendstream",content.len()),
        ]);
        fs::write(&path, pdf).unwrap();
        let font = crate::test_fixture("fonts/NotoSans-Regular.ttf");
        let mut app = Reader::open(&fixture.root, &fixture.state, vec![font.clone()]).unwrap();
        app.open_path(&path).unwrap();
        let text = app.fixed.as_ref().unwrap().reading.page_text(0).unwrap();
        let printed = text
            .words
            .iter()
            .find(|w| &text.text[w.bytes.clone()] == "Boarding…")
            .unwrap();
        app.show(Screen::Find);
        app.editor.text = "...".chars().collect();
        tap(&mut app, FixedNavigation::Next.entry().0);
        settle(&mut app);
        assert_eq!(app.pdf_result_regions, [(0, printed.bounds)]);
        capture_test_screen(
            &app,
            std::slice::from_ref(&font),
            "READER_FIXED_UNICODE_SCREENSHOT",
            false,
        );
        app.show(Screen::Find);
        app.find_match_case = true;
        app.editor.text = "...".chars().collect();
        tap(&mut app, FixedNavigation::Next.entry().0);
        settle(&mut app);
        assert_eq!(app.notice.as_deref(), Some("No later matching page"));
    }

    #[test]
    fn jumps_from_an_interior_fixed_page_to_the_first_and_last_matching_pages() {
        let fixture = Fixture::new();
        let external = std::env::var_os("READER_TEST_MATCH_ENDS_DOCUMENT").map(PathBuf::from);
        let authored = external.is_none();
        let path = external.unwrap_or_else(|| {
            let path = fixture.root.join("Coastal field guide.pdf");
            fs::write(&path, coastal_guide()).unwrap();
            path
        });
        let term =
            std::env::var("READER_TEST_MATCH_ENDS_QUERY").unwrap_or_else(|_| "beacon".into());
        let expected =
            std::env::var("READER_TEST_MATCH_ENDS_PAGES").unwrap_or_else(|_| "3,1,5,105,1".into());
        let values: Vec<usize> = expected.split(',').map(|x| x.parse().unwrap()).collect();
        let [origin, first, last, first_boxes, last_boxes] = values[..] else {
            panic!("supply the reference pages and rectangle counts")
        };
        let font = crate::test_fixture("fonts/NotoSans-Regular.ttf");
        let mut app =
            Reader::open(path.parent().unwrap(), &fixture.state, vec![font.clone()]).unwrap();
        app.open_path(&path).unwrap();
        app.fixed
            .as_mut()
            .unwrap()
            .reading
            .jump(origin - 1)
            .unwrap();
        let source = app.current_history_location().unwrap();
        app.show(Screen::Find);
        app.editor.text = term.chars().collect();
        if authored {
            capture_test_screen(
                &app,
                std::slice::from_ref(&font),
                "READER_MATCH_ENDS_ENTRY",
                false,
            );
        }
        assert_eq!(
            tap(&mut app, FixedNavigation::Last.entry().0),
            Change::Partial
        );
        settle(&mut app);
        let page = |app: &Reader| app.fixed.as_ref().unwrap().reading.page() + 1;
        assert_eq!(page(&app), last);
        assert_eq!(app.pdf_result_regions.len(), last_boxes);
        let tail_view = app.current_history_location();
        assert_eq!(
            tap(&mut app, FixedNavigation::Last.button()),
            Change::Partial
        );
        settle(&mut app);
        assert_eq!(app.current_history_location(), tail_view);
        assert_eq!(app.notice.as_deref(), Some("No later matching page"));
        assert!(app.search_step(-1).is_some());
        assert!(app.search_step(1).is_none());
        assert_eq!(
            tap(&mut app, FixedNavigation::First.button()),
            Change::Partial
        );
        settle(&mut app);
        assert_eq!(page(&app), first);
        assert_eq!(app.pdf_result_regions.len(), first_boxes);
        assert_eq!(
            tap(&mut app, FixedNavigation::First.button()),
            Change::Partial
        );
        settle(&mut app);
        assert_eq!(page(&app), first);
        assert_eq!(app.notice.as_deref(), Some("No earlier matching page"));
        assert!(app.search_step(-1).is_none());
        assert!(app.search_step(1).is_some());
        assert_eq!(
            tap(&mut app, FixedNavigation::Last.button()),
            Change::Partial
        );
        settle(&mut app);
        assert_eq!(page(&app), last);
        assert_eq!(app.pdf_result_regions.len(), last_boxes);
        if authored {
            capture_test_screen(
                &app,
                std::slice::from_ref(&font),
                "READER_MATCH_ENDS_FOOTER",
                false,
            );
        }
        app.activate(Action::ReturnToLocation).unwrap();
        assert_eq!(app.current_history_location(), Some(source));
        println!(
            "fixed_match_ends pages={origin},{last},{first},{last} boxes={last_boxes},{first_boxes},{last_boxes} restored=true"
        );
    }

    #[test]
    fn fixed_word_edges_from_find_and_history_use_the_same_complete_word_boxes() {
        let fixture = Fixture::new();
        let external = std::env::var_os("READER_TEST_FIXED_WORD_EDGE_DOCUMENT").map(PathBuf::from);
        let authored = external.is_none();
        let path = external.unwrap_or_else(|| {
            let path = fixture.root.join("Coastal field guide.pdf");
            fs::write(&path, coastal_guide()).unwrap();
            path
        });
        let term =
            std::env::var("READER_TEST_FIXED_WORD_EDGE_QUERY").unwrap_or_else(|_| "sand".into());
        let page: usize = std::env::var("READER_TEST_FIXED_WORD_EDGE_PAGE")
            .ok()
            .map_or(4, |page| page.parse().unwrap());
        let counts = std::env::var("READER_TEST_FIXED_WORD_EDGE_COUNTS")
            .unwrap_or_else(|_| "7,4,4,1".into());
        let counts: Vec<usize> = counts.split(',').map(|c| c.parse().unwrap()).collect();
        assert_eq!(counts.len(), 4);
        let font = crate::test_fixture("fonts/NotoSans-Regular.ttf");
        let root = path.parent().unwrap();
        let mut app = Reader::open(root, &fixture.state, vec![font.clone()]).unwrap();
        app.open_path(&path).unwrap();
        app.fixed.as_mut().unwrap().reading.jump(page - 1).unwrap();
        let location = app.current_history_location().unwrap();
        let text = app
            .fixed
            .as_ref()
            .unwrap()
            .reading
            .page_text(page - 1)
            .unwrap();
        let patterns = [
            term.clone(),
            format!(" {term}"),
            format!("{term} "),
            format!(" {term} "),
        ];
        for (pattern, &expected) in patterns.iter().zip(&counts) {
            let mut search = PdfSearch::new(pattern, false).unwrap();
            let listed = search.hits(page - 1, &text, MAX_SEARCH_HITS);
            assert_eq!(listed.len(), expected);
            assert!(listed.iter().all(|hit| hit.bounds.len() == 1));
            app.show(Screen::Find);
            app.editor.text = pattern.chars().collect();
            assert_eq!(
                tap(&mut app, FixedNavigation::Next.entry().0),
                Change::Partial
            );
            app.poll_nearby(Instant::now()).unwrap();
            if expected == 0 {
                assert!(app.nearby.searching_direction());
                assert_eq!(tap(&mut app, PREVIOUS), Change::Partial);
            } else {
                assert_eq!(app.screen, Screen::Fixed);
                let highlighted: Vec<_> = app
                    .pdf_result_regions
                    .iter()
                    .map(|(_, rect)| *rect)
                    .collect();
                assert_eq!(
                    highlighted,
                    listed.iter().map(|hit| hit.bounds[0]).collect::<Vec<_>>()
                );
                if authored && pattern == &patterns[3] {
                    capture_test_screen(
                        &app,
                        std::slice::from_ref(&font),
                        "READER_WORD_EDGE_SCREENSHOT",
                        false,
                    );
                }
                app.activate(Action::ReturnToLocation).unwrap();
            }
            assert_eq!(app.current_history_location(), Some(location.clone()));
        }
        drop(app);
        let mut resumed = Reader::open(root, &fixture.state, vec![font]).unwrap();
        resumed.open_path(&path).unwrap();
        let saved = &resumed.lookup_history.searches;
        assert_eq!(
            saved.iter().map(|s| &s.text).collect::<Vec<_>>(),
            patterns.iter().rev().collect::<Vec<_>>()
        );
        let whole_word = saved[0].clone();
        resumed.activate(Action::RepeatSearch(whole_word)).unwrap();
        if authored {
            while resumed.search_at.is_some() {
                resumed.step_pdf_search(Instant::now()).unwrap();
            }
            assert_eq!(resumed.pdf_hits.len(), 1);
            assert_eq!(resumed.pdf_hits[0].page, page - 1);
        }
        println!(
            "fixed_word_edges page={page} boxes={},{},{},{} viewport_restored=true history_restored=true",
            counts[0], counts[1], counts[2], counts[3]
        );
    }

    #[test]
    fn directional_djvu_search_uses_the_embedded_word_rectangles() {
        let fixture = Fixture::new();
        let path = fixture.root.join("Tidal signal.djvu");
        fs::write(&path, crate::djvu::tests::document(1, true)).unwrap();
        let mut app = Reader::open(
            &fixture.root,
            &fixture.state,
            vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")],
        )
        .unwrap();
        app.open_path(&path).unwrap();
        let words = app.fixed.as_ref().unwrap().reading.page_text(0).unwrap();
        app.show(Screen::Find);
        app.find_ignore_accents = true;
        app.editor.text = "cafe".chars().collect();
        tap(&mut app, FixedNavigation::Next.entry().0);
        settle(&mut app);
        assert_eq!(app.screen, Screen::Fixed);
        assert_eq!(app.pdf_result_regions, vec![(0, words.words[1].bounds)]);
        tap(&mut app, FixedNavigation::Next.button());
        settle(&mut app);
        assert_eq!(app.notice.as_deref(), Some("No later matching page"));
    }

    #[test]
    fn a_zoomed_directional_pdf_match_returns_to_the_same_source_point() {
        let fixture = Fixture::new();
        let path = fixture.root.join("Guide.pdf");
        fs::write(&path, coastal_guide()).unwrap();
        let mut app = Reader::open(
            &fixture.root,
            &fixture.state,
            vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")],
        )
        .unwrap();
        app.open_path(&path).unwrap();
        let reading = &mut app.fixed.as_mut().unwrap().reading;
        reading.jump(2).unwrap();
        reading.set_zoom(3).unwrap();
        reading
            .input(Input::Swipe {
                dx: -240.0,
                dy: -450.0,
                start_y: 720.0,
            })
            .unwrap();
        let original = app.current_history_location().unwrap();
        assert!(original.scan_point.is_some());
        app.show(Screen::Find);
        app.editor.text = "beacon".chars().collect();
        tap(&mut app, FixedNavigation::Next.entry().0);
        settle(&mut app);
        assert_eq!(app.pdf_result_regions.len(), 2);
        tap(&mut app, FixedNavigation::Next.button());
        settle(&mut app);
        app.activate(Action::ReturnToLocation).unwrap();
        let restored = app.current_history_location().unwrap();
        assert_eq!(restored.position, original.position);
        let delta = restored.scan_point.unwrap() - original.scan_point.unwrap();
        assert!(delta.hypot() < 0.01);
    }

    #[test]
    fn cancels_fixed_search_and_stops_at_the_publication_edges() {
        let fixture = Fixture::new();
        let path = fixture.root.join("Guide.pdf");
        fs::write(&path, coastal_guide()).unwrap();
        let mut app = Reader::open(
            &fixture.root,
            &fixture.state,
            vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")],
        )
        .unwrap();
        app.open_path(&path).unwrap();
        app.show(Screen::Find);
        app.editor.text = "absent".chars().collect();
        tap(&mut app, FixedNavigation::Previous.entry().0);
        assert!(app.nearby.searching_direction());
        tap(&mut app, PREVIOUS);
        assert!(!app.nearby.searching_direction());
        assert_eq!(app.screen, Screen::Find);
        app.editor.text = "beacon".chars().collect();
        tap(&mut app, FixedNavigation::Previous.entry().0);
        settle(&mut app);
        assert_eq!(app.fixed.as_ref().unwrap().reading.page(), 0);
        tap(&mut app, FixedNavigation::Previous.button());
        settle(&mut app);
        assert_eq!(app.notice.as_deref(), Some("No earlier matching page"));
        assert!(app.search_step(-1).is_none());
        assert!(app.search_step(1).is_some());
        assert_eq!(app.fixed.as_ref().unwrap().reading.page(), 0);
    }
}
