//! Reflow searches share one cancellable worker. Directional PDF and DjVu searches
//! reuse the open document and yield between physical pages.
use super::*;
use crate::Anchor;
use crate::fixed_session::SEARCH_FIT_CLOSE;
use crate::search::{ChapterText, FlowConfig, NearbySearch, SearchDirection};
use std::sync::{
    Arc, Condvar, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc,
};

const PREVIOUS: Rect = Rect::new(LIST_X, LIST_Y + 364, 555, 100);
const NEXT_HERE: Rect = Rect::new(LIST_X + 579, LIST_Y + 364, 555, 100);
const LIST_RESULT_PREVIOUS: Rect = Rect::new(420, MATCH_PREV.y, 104, MATCH_PREV.h);
const LIST_RESULT_LIST: Rect = Rect::new(534, MATCH_PREV.y, 176, MATCH_PREV.h);
const LIST_RESULT_NEXT: Rect = Rect::new(720, MATCH_PREV.y, 104, MATCH_PREV.h);

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

    fn draw(self, canvas: &mut Canvas, rect: Rect, active: bool) {
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
            self.icon(),
            ink,
        );
    }
}

struct Request {
    engine: NearbySearch,
    path: PathBuf,
    chapters: usize,
    chapter: usize,
    anchor: Option<Anchor>,
    publication: u64,
    stop_anchor: Option<Anchor>,
    flow: FlowConfig,
    raw_layout: Option<crate::session::SearchLayoutRecipe>,
    cancelled: Arc<AtomicBool>,
    output: mpsc::Sender<Result<Option<SearchHit>>>,
}

fn load_text(
    path: &Path,
    ci: usize,
    flow: &FlowConfig,
    raw_layout: Option<&crate::session::SearchLayoutRecipe>,
    cancelled: &AtomicBool,
) -> Result<ChapterText> {
    // Canonical search remains font-free. If raw layout is enabled, shape the
    // exact chapter separately; matching itself runs outside the storage lease.
    if cancelled.load(Ordering::Relaxed) {
        return Err("Search was cancelled".into());
    }
    let chapter = with_storage(|| {
        let mut book = match raw_layout {
            Some(recipe) => recipe.open_search_book(path),
            None => Book::open(path),
        }
        .map_err(io::Error::other)?;
        ChapterText::read_with_flow(&mut book, ci, flow).map_err(io::Error::other)
    })?;
    if cancelled.load(Ordering::Relaxed) {
        return Err("Search was cancelled".into());
    }
    if let Some(recipe) = raw_layout {
        let probe = recipe.chapter(ci, cancelled)?;
        chapter.with_verified_raw_layout(&probe)
    } else {
        Ok(chapter)
    }
}

impl Request {
    fn run(self) {
        let found = match self.anchor.as_ref() {
            Some(anchor) => {
                self.engine
                    .scan(self.chapter, anchor, &self.cancelled, self.chapters, |ci| {
                        load_text(
                            &self.path,
                            ci,
                            &self.flow,
                            self.raw_layout.as_ref(),
                            &self.cancelled,
                        )
                    })
            }
            None => self.engine.scan_edge(
                self.publication,
                self.chapter,
                self.stop_anchor.as_ref(),
                &self.cancelled,
                self.chapters,
                |ci| {
                    load_text(
                        &self.path,
                        ci,
                        &self.flow,
                        self.raw_layout.as_ref(),
                        &self.cancelled,
                    )
                },
            ),
        };
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
    Skipped(usize),
    Finished {
        limited: bool,
        memory_limited: bool,
        skipped: usize,
    },
}

#[derive(Default)]
struct AllCompleteness {
    skipped: usize,
    limited: bool,
    memory_limited: bool,
    stopped: bool,
    failed: bool,
}

#[derive(Clone, Debug, PartialEq)]
struct ReflowContext {
    edition: u64,
    publication: u64,
    flow: FlowConfig,
    raw_layout: Option<crate::session::SearchLayoutRecipe>,
}
struct PendingAll {
    cancel: Arc<AtomicBool>,
    result: mpsc::Receiver<Result<AllEvent>>,
    at: Instant,
}

struct PendingPageMarks {
    cancel: Arc<AtomicBool>,
    result: mpsc::Receiver<Result<Vec<SearchHit>>>,
    at: Instant,
    chapter: usize,
    page: usize,
    selected: TextSelection,
    recipe: crate::session::SearchLayoutRecipe,
    layout_identity: Arc<()>,
    revision: u64,
    viewport: crate::session::ReadingViewport,
    scroll_top: Option<f32>,
    hits: Option<std::collections::VecDeque<SearchHit>>,
    rects: Vec<crate::selection::HighlightRect>,
    limited: bool,
}

impl Drop for PendingPageMarks {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

struct PageMarks {
    chapter: usize,
    page: usize,
    selected_start: Anchor,
    selected_end: Anchor,
    rects: Vec<crate::selection::HighlightRect>,
    layout_identity: Arc<()>,
    viewport: crate::session::ReadingViewport,
    scroll_top: Option<f32>,
}

struct PageMarksResume {
    chapter: usize,
    page: usize,
    selected_start: Anchor,
    selected_end: Anchor,
    layout_identity: Arc<()>,
    viewport: crate::session::ReadingViewport,
    scroll_top: Option<f32>,
    query: SearchQuery,
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

struct FixedSearchFit {
    original_view: FixedPreferences,
    original_location: HistoryLocation,
    purpose: FitPurpose,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum FitPurpose {
    Directional,
    ResultList,
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
    all_completeness: Option<AllCompleteness>,
    all_details_open: bool,
    page_pending: Option<PendingPageMarks>,
    page_marks: Option<PageMarks>,
    page_resume: Option<PageMarksResume>,
    current: Option<SearchQuery>,
    end: Option<SearchDirection>,
    // Retain this even after a completed/cancelled job while hits still exist.
    // Closing a panel must not make old-style results current again.
    results_context: Option<ReflowContext>,
    fit: Option<FixedSearchFit>,
}

impl Nearby {
    pub(super) fn cancel_page_marks(&mut self) {
        self.page_pending = None;
        self.page_marks = None;
        self.page_resume = None;
    }

    pub(super) fn suspend_page_marks(&mut self) {
        if let (Some(pending), Some(query)) = (&self.page_pending, &self.current) {
            self.page_resume = Some(PageMarksResume {
                chapter: pending.chapter,
                page: pending.page,
                selected_start: pending.selected.start.clone(),
                selected_end: pending.selected.end.clone(),
                layout_identity: Arc::clone(&pending.layout_identity),
                viewport: pending.viewport,
                scroll_top: pending.scroll_top,
                query: query.clone(),
            });
        }
        self.page_pending = None;
    }

    pub(super) fn deadline(&self) -> Option<Instant> {
        self.pending
            .as_ref()
            .map(|p| p.at)
            .or_else(|| self.fixed.as_ref().map(|p| p.at))
            .or_else(|| self.all.as_ref().map(|p| p.at))
            .or_else(|| self.page_pending.as_ref().map(|p| p.at))
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
        if self.all.take().is_some()
            && let Some(status) = &mut self.all_completeness
        {
            status.stopped = true;
        }
        self.cancel_page_marks();
        self.all_details_open = false;
    }

    pub(super) fn discard_all_completeness(&mut self) {
        self.all_completeness = None;
        self.all_details_open = false;
    }

    pub(super) fn close_all_details(&mut self) {
        self.all_details_open = false;
    }

    pub(super) fn all_details_open(&self) -> bool {
        self.all_details_open
    }
    pub(super) fn clear(&mut self) {
        self.cancel();
        self.current = None;
        self.end = None;
    }
    pub(super) fn result(&self) -> bool {
        self.current.is_some()
    }

    pub(super) fn clear_exhaustion(&mut self) {
        self.end = None;
    }
    pub(super) fn can_step(&self, direction: SearchDirection) -> bool {
        self.current.is_some() && self.end != Some(direction)
    }
}

impl Reader {
    /// Results can outlive the worker and any transient notice. Keep their
    /// completeness visible whenever the retained reflow list is reopened.
    pub(super) fn reflow_results_completeness(&self) -> Option<String> {
        let status = self.nearby.all_completeness.as_ref()?;
        if status.skipped == 0
            && !status.limited
            && !status.memory_limited
            && !status.stopped
            && !status.failed
        {
            return None;
        }
        let language = self.controls.interface_language;
        let mut parts = vec![
            language
                .document_search_text("Search incomplete")
                .to_owned(),
        ];
        if status.skipped > 0 {
            parts.push(format!(
                "{} {}",
                language.document_search_text("Scanned skips (at least)"),
                status.skipped
            ));
        }
        if status.limited {
            parts.push(language.document_search_text("5,000 limit").to_owned());
        }
        if status.memory_limited {
            parts.push(language.document_search_text("Memory limit").to_owned());
        }
        if status.stopped {
            parts.push(language.document_search_text("Search stopped").to_owned());
        }
        if status.failed {
            parts.push(language.document_search_text("Search failed").to_owned());
        }
        if status.stopped || status.failed || status.limited || status.memory_limited {
            parts.push(
                language
                    .document_search_text("More may be unsearched")
                    .to_owned(),
            );
        }
        Some(parts.join("\n"))
    }

    fn completeness_indicator_hit(&self) -> Rect {
        self.search_layout().list_caption(LIST_X, 210, 100)
    }

    fn completeness_panel(&self) -> (Rect, Rect) {
        let layout = self.navigation_layout();
        let width = (layout.width - 80).clamp(200, 900);
        let height = (layout.height - 80).clamp(360, 760);
        let panel = Rect::new(
            (layout.width - width) / 2,
            (layout.height - height) / 2,
            width,
            height,
        );
        let close = Rect::new(
            panel.x + (panel.w - 240) / 2,
            panel.bottom() - 124,
            240,
            100,
        );
        (panel, close)
    }

    /// Draw only a compact control in the existing footer; its details are
    /// available on demand without reducing the result list's height.
    pub(super) fn draw_reflow_completeness_indicator(&self, canvas: &mut Canvas) -> bool {
        if self.reflow_results_completeness().is_none() {
            return false;
        }
        let language = self.controls.interface_language;
        let area = self.search_layout().list_caption(LIST_X, 179, 64);
        let full = language.document_search_text("Search incomplete · Details");
        let short = language.document_search_text("Incomplete · Details");
        let label = if canvas.measure(full, 28) <= area.w {
            full
        } else if canvas.measure(short, 28) <= area.w {
            short
        } else {
            language.document_search_text("Details")
        };
        canvas.text(area, label, CAPTION);
        true
    }

    pub(super) fn draw_reflow_completeness_details(&self, canvas: &mut Canvas) {
        if self.screen != Screen::Matches || !self.nearby.all_details_open {
            return;
        }
        let Some(details) = self.reflow_results_completeness() else {
            return;
        };
        let (panel, close) = self.completeness_panel();
        let language = self.controls.interface_language;
        canvas.popup(panel);
        canvas.text(
            Rect::new(panel.x + 32, panel.y + 24, panel.w - 64, 64),
            language.document_search_text("Search details"),
            BODY.bold(),
        );
        canvas.text(
            Rect::new(panel.x + 32, panel.y + 94, panel.w - 64, panel.h - 236),
            &details,
            CAPTION.lines(14).break_long_words(),
        );
        canvas.button(close, language.document_search_text("Done"), false, false);
    }

    pub(super) fn reflow_completeness_input(&mut self, input: Input) -> Option<Change> {
        if self.screen != Screen::Matches || self.read_alarm.modal() {
            return None;
        }
        if self.nearby.all_details_open {
            if matches!(input, Input::Sleep | Input::Wake) {
                return None;
            }
            let (panel, close) = self.completeness_panel();
            if matches!(input, Input::PageBack)
                || matches!(input, Input::Tap(x, y) if close.contains(x, y) || !panel.contains(x, y))
            {
                self.nearby.all_details_open = false;
                return Some(Change::Partial);
            }
            return Some(Change::None);
        }
        if !self.search_results_navigation.menu_open
            && self.result_context.is_none()
            && self.reflow_results_completeness().is_some()
            && matches!(input, Input::Tap(x, y) if self.completeness_indicator_hit().contains(x, y))
        {
            self.nearby.all_details_open = true;
            return Some(Change::Partial);
        }
        None
    }

    pub(super) fn resume_page_marks(&mut self) {
        let Some(resume) = self.nearby.page_resume.take() else {
            return;
        };
        if self.screen != Screen::Book
            || self.folder_resume_gate.is_some()
            || self.nearby.searching_direction()
            || self.nearby.searching_all()
            || self.nearby.page_pending.is_some()
            || self.nearby.current.as_ref() != Some(&resume.query)
        {
            return;
        }
        let Some(active) = &self.active else { return };
        let reading = &active.reading;
        if reading.chapter != resume.chapter
            || reading.page != resume.page
            || reading.config.viewport != resume.viewport
            || reading.scroll_history_top() != resume.scroll_top
            || !Arc::ptr_eq(&reading.probe.layout_identity, &resume.layout_identity)
            || !self.result_selection.as_ref().is_some_and(|selection| {
                selection.start == resume.selected_start && selection.end == resume.selected_end
            })
        {
            return;
        }
        self.begin_page_marks(&resume.query);
    }

    fn begin_page_marks(&mut self, query: &SearchQuery) {
        self.nearby.cancel_page_marks();
        let Some(active) = &self.active else { return };
        let Some(selected) = self.result_selection.clone() else {
            return;
        };
        let reading = &active.reading;
        let Some(start) = reading.visible_search_boundary(SearchDirection::Next) else {
            return;
        };
        let Some(end) = reading.visible_search_boundary(SearchDirection::Previous) else {
            return;
        };
        let recipe = reading.config.search_layout_recipe(&reading.probe);
        if !recipe.has_verified_sources() || start.chapter != end.chapter {
            return;
        }
        let chapter = reading.chapter;
        let page = reading.page;
        let chapters = reading.probe.chapter_count();
        let Some(context) = self.current_reflow_context() else {
            return;
        };
        let mut search = match if query.regular_expression {
            Search::with_regular_expression(&query.text, query.match_case)
        } else {
            Search::with_options(&query.text, query.match_case, query.ignore_accents)
        } {
            Ok(search) => search,
            Err(_) => return,
        };
        search = search
            .at_most(33)
            .with_flow_config(context.flow.clone())
            .for_publication(context.publication, chapters)
            .within_chapter(chapter, start.clone(), end.clone());
        let path = reading.config.book.clone();
        let layout_identity = Arc::clone(&reading.probe.layout_identity);
        let revision = reading.probe.source.revision;
        let viewport = reading.config.viewport;
        let scroll_top = reading.scroll_history_top();
        let worker_recipe = recipe.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancel);
        let (tx, result) = mpsc::channel();
        let Some(worker) = &self.nearby.worker else {
            return;
        };
        worker.submit(move || {
            let scan = (|| {
                let mut text = with_storage(|| {
                    let mut book = worker_recipe
                        .open_search_book(&path)
                        .map_err(io::Error::other)?;
                    ChapterText::read_with_flow(&mut book, chapter, &context.flow)
                        .map_err(io::Error::other)
                })?;
                let probe = worker_recipe.chapter(chapter, &worker_cancel)?;
                if crate::parley_layout::RAW_LAYOUT_ENABLED {
                    text = text.with_verified_raw_layout(&probe)?;
                }
                search.step_loaded(text, chapters, &worker_cancel)
            })();
            if !worker_cancel.load(Ordering::Relaxed) {
                let _ = tx.send(scan);
            }
        });
        self.nearby.page_pending = Some(PendingPageMarks {
            cancel,
            result,
            at: Instant::now() + SEARCH_DELAY,
            chapter,
            page,
            selected,
            recipe,
            layout_identity,
            revision,
            viewport,
            scroll_top,
            hits: None,
            rects: Vec::new(),
            limited: false,
        });
    }

    pub(super) fn directional_page_marks(&self) -> &[crate::selection::HighlightRect] {
        let Some(marks) = &self.nearby.page_marks else {
            return &[];
        };
        let Some(active) = &self.active else {
            return &[];
        };
        let reading = &active.reading;
        if self.nearby.current.is_none()
            || !self.result_selection.as_ref().is_some_and(|selection| {
                selection.start == marks.selected_start && selection.end == marks.selected_end
            })
            || reading.config.viewport != marks.viewport
            || reading.scroll_history_top() != marks.scroll_top
            || !Arc::ptr_eq(&marks.layout_identity, &reading.probe.layout_identity)
            || reading.chapter != marks.chapter
            || reading.page != marks.page
        {
            return &[];
        }
        &marks.rects
    }

    fn poll_page_marks(&mut self, now: Instant) -> Change {
        let Some(mut pending) = self.nearby.page_pending.take() else {
            return Change::None;
        };
        let Some(active) = &self.active else {
            return Change::None;
        };
        let reading = &active.reading;
        if self.screen != Screen::Book
            || self.nearby.current.is_none()
            || !self.result_selection.as_ref().is_some_and(|selection| {
                selection.start == pending.selected.start && selection.end == pending.selected.end
            })
            || reading.chapter != pending.chapter
            || reading.page != pending.page
            || reading.probe.source.revision != pending.revision
            || reading.config.viewport != pending.viewport
            || reading.scroll_history_top() != pending.scroll_top
            || !Arc::ptr_eq(&pending.layout_identity, &reading.probe.layout_identity)
        {
            return Change::None;
        }
        if pending.hits.is_none() {
            let mut hits = match pending.result.try_recv() {
                Err(mpsc::TryRecvError::Empty) => {
                    pending.at = now + SEARCH_DELAY;
                    self.nearby.page_pending = Some(pending);
                    return Change::None;
                }
                Ok(Ok(hits)) => hits,
                _ => return Change::None,
            };
            pending.limited = hits.len() > 32;
            hits.truncate(32);
            pending.hits = Some(hits.into());
        }
        // Resolve at most one match per event-loop tick. Keep geometry instead
        // of repeating source and paragraph scans on every repaint.
        if let Some(hit) = pending.hits.as_mut().and_then(|hits| hits.pop_front()) {
            if hit.anchor.revision == pending.revision
                && hit.end.revision == pending.revision
                && let Some(selection) = reading.probe.select_range(&hit.anchor, &hit.end)
                && (selection.start != pending.selected.start
                    || selection.end != pending.selected.end)
            {
                if selection.text.len() > 4096 {
                    pending.limited = true;
                } else {
                    let rects = reading.highlight_rects(&selection);
                    if pending.rects.len().saturating_add(rects.len()) > 512 {
                        pending.limited = true;
                        pending.hits.as_mut().unwrap().clear();
                    } else {
                        pending.rects.extend(rects);
                    }
                }
            }
            pending.at = now + Duration::from_millis(1);
            self.nearby.page_pending = Some(pending);
            return Change::None;
        }
        if reading.config.search_layout_recipe(&reading.probe) != pending.recipe {
            return Change::None;
        }
        self.nearby.page_marks = Some(PageMarks {
            chapter: pending.chapter,
            page: pending.page,
            selected_start: pending.selected.start.clone(),
            selected_end: pending.selected.end.clone(),
            rects: std::mem::take(&mut pending.rects),
            layout_identity: Arc::clone(&pending.layout_identity),
            viewport: pending.viewport,
            scroll_top: pending.scroll_top,
        });
        if pending.limited {
            self.notice = Some("Some page matches are not highlighted".into());
        }
        Change::Partial
    }

    pub(super) fn fixed_search_fit_active(&self) -> bool {
        self.nearby.fit.is_some()
    }

    fn enter_fixed_search_fit(
        &mut self,
        purpose: FitPurpose,
        page: usize,
    ) -> Result<Option<HistoryLocation>> {
        if let Some(fit) = &self.nearby.fit {
            if fit.purpose == purpose {
                self.fixed
                    .as_mut()
                    .ok_or("No open document")?
                    .reading
                    .show_search_fit_page(page)?;
                return Ok(None);
            }
            self.close_fixed_search_fit()?;
        }
        let original_location = self.current_history_location().ok_or("No open document")?;
        let reading = &mut self.fixed.as_mut().ok_or("No open document")?.reading;
        let original_view = reading.preferences();
        reading.show_search_fit_page(page)?;
        self.nearby.fit = Some(FixedSearchFit {
            original_view,
            original_location: original_location.clone(),
            purpose,
        });
        let now = Instant::now();
        self.update_stats_activity(now);
        self.sync_auto_turn(now);
        Ok(Some(original_location))
    }

    pub(super) fn open_fixed_list_result(&mut self, index: usize) -> Result<()> {
        if self.search_result_order().ordinal(index).is_none() {
            return Err("That result is unavailable".into());
        }
        let hit = self
            .pdf_hits
            .get(index)
            .ok_or("That result is unavailable")?;
        let (page, regions) = (hit.page, hit.bounds.clone());
        let previous = self.enter_fixed_search_fit(FitPurpose::ResultList, page)?;
        self.nearby.clear();
        self.pdf_selection = None;
        self.show(Screen::Fixed);
        if let Some(previous) = previous {
            self.push_history(previous);
        }
        self.pdf_result_regions = regions.into_iter().map(|bounds| (page, bounds)).collect();
        self.search_result = Some(index);
        self.focused_result = Some(index);
        let page = self.match_page_for_index(index);
        self.remember_match_window(page);
        Ok(())
    }

    fn fixed_list_result_active(&self) -> bool {
        self.nearby
            .fit
            .as_ref()
            .is_some_and(|fit| fit.purpose == FitPurpose::ResultList)
    }

    fn fixed_list_result_control(&mut self, x: f32, y: f32, hold: bool) -> Option<Change> {
        let layout = self.search_layout();
        if !hold && layout.reading_footer(LIST_RESULT_LIST).contains(x, y) {
            return Some(match self.close_fixed_search_fit() {
                Ok(()) => {
                    self.matches_return_to_reading = true;
                    self.show(Screen::Matches);
                    Change::Partial
                }
                Err(error) => self.report(error),
            });
        }
        let direction = if layout.reading_footer(LIST_RESULT_PREVIOUS).contains(x, y) {
            -1
        } else if layout.reading_footer(LIST_RESULT_NEXT).contains(x, y) {
            1
        } else {
            return None;
        };
        let Some(current) = self.search_result else {
            return Some(Change::None);
        };
        let index = {
            let order = self.search_result_order();
            if hold {
                let edge = if direction < 0 {
                    Some(0)
                } else {
                    order.len().checked_sub(1)
                };
                edge.and_then(|ordinal| order.provider_index(ordinal))
            } else {
                order.step(current, direction)
            }
        };
        let Some(index) = index.filter(|&index| index != current) else {
            return Some(Change::None);
        };
        Some(match self.open_fixed_list_result(index) {
            Ok(()) => Change::Partial,
            Err(error) => self.report(error),
        })
    }

    pub(super) fn draw_fixed_list_search(&self, canvas: &mut Canvas) -> bool {
        if !self.fixed_list_result_active() {
            return false;
        }
        if let Some(index) = self.search_result {
            let order = self.search_result_order();
            let layout = self.search_layout();
            FixedNavigation::Previous.draw(
                canvas,
                layout.reading_footer(LIST_RESULT_PREVIOUS),
                order.step(index, -1).is_some(),
            );
            canvas.button(
                layout.reading_footer(LIST_RESULT_LIST),
                self.controls
                    .interface_language
                    .document_search_text("Results"),
                false,
                false,
            );
            FixedNavigation::Next.draw(
                canvas,
                layout.reading_footer(LIST_RESULT_NEXT),
                order.step(index, 1).is_some(),
            );
        }
        true
    }

    fn restore_fixed_search_fit(&mut self) -> Result<()> {
        let Some(fit) = &self.nearby.fit else {
            return Ok(());
        };
        if let Some(fixed) = &mut self.fixed {
            if fixed.reading.preferences() != fit.original_view {
                fixed.reading.restore_view(fit.original_view)?;
            }
            fixed.reading.pause();
        }
        self.nearby.fit = None;
        self.record_position();
        self.sync_auto_turn(Instant::now());
        Ok(())
    }

    pub(super) fn cancel_directional_search(&mut self) -> Result<()> {
        self.nearby.cancel();
        if self.fixed_search_fit_active() {
            self.restore_fixed_search_fit()?;
            self.nearby.clear();
            self.search_result = None;
            self.pdf_result_regions.clear();
        }
        self.notice = None;
        Ok(())
    }

    pub(super) fn close_fixed_search_fit(&mut self) -> Result<()> {
        if self.fixed_search_fit_active() {
            self.cancel_directional_search()?;
        }
        Ok(())
    }

    pub(super) fn fixed_search_fit_input(&mut self, input: Input) -> Option<Change> {
        if self.screen != Screen::Fixed || !self.fixed_search_fit_active() {
            return None;
        }
        let layout = self.search_layout();
        let keep = match input {
            Input::Tap(x, y) => {
                if layout.viewport.done().contains(x, y) {
                    return Some(self.exit());
                }
                let change = if self.fixed_list_result_active() {
                    self.fixed_list_result_control(x, y, false)
                } else {
                    self.search_result_input(x, y)
                };
                if let Some(change) = change {
                    return Some(change);
                }
                false
            }
            Input::LongTap(x, y) if self.fixed_list_result_active() => {
                if let Some(change) = self.fixed_list_result_control(x, y, true) {
                    return Some(change);
                }
                false
            }
            Input::HoldRelease {
                start_x,
                start_y,
                x,
                y,
                ..
            } if self.fixed_list_result_active() => {
                if [LIST_RESULT_PREVIOUS, LIST_RESULT_NEXT]
                    .into_iter()
                    .map(|rect| layout.reading_footer(rect))
                    .any(|rect| rect.contains(start_x, start_y) && rect.contains(x, y))
                {
                    return self.fixed_list_result_control(x, y, true);
                }
                false
            }
            Input::LongTap(..) | Input::HoldRelease { .. } => false,
            Input::HoldStart(..) | Input::HoldPan(..) => return Some(Change::None),
            Input::PageBack | Input::PageForward | Input::Swipe { .. } => true,
            _ => return None,
        };
        match self.cancel_directional_search() {
            Ok(()) if keep => match self.input(input) {
                Change::None => Some(Change::Partial),
                change => Some(change),
            },
            Ok(()) => Some(Change::Partial),
            Err(error) => Some(self.report(error)),
        }
    }

    pub(super) fn draw_fixed_search_fit_close(&self, canvas: &mut Canvas) {
        // Replace the temporary view's zoom controls in the reserved footer
        // area. The status caption and Exit keep their normal positions.
        let rect = self.reading_viewport().bottom_right(SEARCH_FIT_CLOSE);
        canvas.fill(rect, 255);
        canvas.button(
            rect,
            self.controls
                .interface_language
                .document_search_text("Close search"),
            false,
            false,
        );
    }

    fn current_reflow_context(&self) -> Option<ReflowContext> {
        self.active.as_ref().map(|active| ReflowContext {
            edition: active.id,
            publication: active.reading.probe.book,
            flow: FlowConfig {
                publisher_styles: active.reading.config.settings.publisher_styles,
                base_stylesheet: active.reading.config.settings.base_stylesheet,
                user_styles: active.reading.config.user_styles.clone(),
                media_context: Some(crate::book_style::MediaContext::from_settings(
                    active.reading.probe.settings,
                )),
            },
            raw_layout: crate::parley_layout::RAW_LAYOUT_ENABLED.then(|| {
                active
                    .reading
                    .config
                    .search_layout_recipe(&active.reading.probe)
            }),
        })
    }

    /// Call before accepting a worker delivery or activating a retained hit.
    pub(super) fn ensure_reflow_search_current(&mut self) -> Result<()> {
        if self.hits.is_empty()
            && self.nearby.pending.is_none()
            && self.nearby.all.is_none()
            && self.nearby.all_completeness.is_none()
        {
            self.nearby.results_context = None;
            return Ok(());
        }
        if self
            .nearby
            .results_context
            .as_ref()
            .is_some_and(|saved| Some(saved) != self.current_reflow_context().as_ref())
        {
            self.nearby.clear();
            self.nearby.results_context = None;
            self.nearby.discard_all_completeness();
            self.hits.clear();
            self.clear_flexible_results();
            self.reset_search_chapter();
            self.result_selection = None;
            self.search_result = None;
            return Err("Reading styles or the book changed; search again".into());
        }
        Ok(())
    }

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
        let layout = self.search_layout();
        if !self.nearby.searching_direction() {
            let step = FixedNavigation::ITEMS
                .into_iter()
                .find(|s| layout.find_direction(s.entry().0).contains(x, y))?;
            return Some(self.fixed_navigation(step, false));
        }
        let direction = if layout.find_direction(PREVIOUS).contains(x, y) {
            SearchDirection::Previous
        } else if layout.find_direction(NEXT_HERE).contains(x, y) {
            SearchDirection::Next
        } else {
            return None;
        };
        if self.nearby.deadline().is_some() {
            return Some(match self.cancel_directional_search() {
                Ok(()) => Change::Partial,
                Err(error) => self.report(error),
            });
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
        let layout = self.search_layout();
        if self.nearby.searching_direction() {
            canvas.button(
                layout.find_direction(PREVIOUS),
                self.controls
                    .interface_language
                    .document_search_text("Cancel search"),
                false,
                false,
            );
        } else {
            for step in FixedNavigation::ITEMS {
                let (rect, label) = step.entry();
                canvas.button(
                    layout.find_direction(rect),
                    self.controls.interface_language.document_search_text(label),
                    false,
                    false,
                );
            }
        }
    }
    fn fixed_navigation(&mut self, step: FixedNavigation, continuing: bool) -> Change {
        if continuing
            && !step.from_edge()
            && !self.controls.fixed_search_full_page
            && !self.fixed_search_fit_active()
        {
            let direction = match step {
                FixedNavigation::Previous => -1,
                FixedNavigation::Next => 1,
                FixedNavigation::First | FixedNavigation::Last => unreachable!(),
            };
            let pan = self.fixed.as_mut().map(|fixed| {
                let page = fixed.reading.page();
                (page, fixed.reading.search_view_step(direction))
            });
            match pan {
                Some((page, Ok(true))) => return self.finish_fixed_update(Ok(true), page),
                Some((_, Err(error))) => return self.report(error),
                _ => {}
            }
        }
        match self.begin_search_from(step.direction(), continuing, step.from_edge()) {
            Ok(()) => Change::Partial,
            Err(error) => self.report(error),
        }
    }

    pub(super) fn fixed_result_navigation(&self) -> bool {
        self.nearby.result() && self.search_result.is_some()
    }

    pub(super) fn fixed_result_input(&mut self, x: f32, y: f32) -> Option<Change> {
        let layout = self.search_layout();
        let step = FixedNavigation::ITEMS
            .into_iter()
            .find(|s| layout.reading_footer(s.button()).contains(x, y))?;
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
            let rect = self.search_layout().reading_footer(step.button());
            let active = step.from_edge() || self.nearby.can_step(step.direction());
            step.draw(canvas, rect, active);
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
        if continuing && self.active.is_some() {
            // Validate before consuming the old selection or replacing its key.
            self.ensure_reflow_search_current()?;
        }
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
        // A new directional request supersedes any page-decoration work.
        self.nearby.cancel_page_marks();
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
                self.reset_search_chapter();
                self.search_result = None;
            }
            if self.controls.fixed_search_full_page {
                self.enter_fixed_search_fit(FitPurpose::Directional, page)?;
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
            self.notice = Some(
                self.controls
                    .interface_language
                    .document_search_text("Searching pages")
                    .into(),
            );
            return Ok(());
        }
        let active = self.active.as_ref().ok_or("Open a book first")?;
        let context = self.current_reflow_context().ok_or("Open a book first")?;
        let engine = if query.regular_expression {
            NearbySearch::with_regular_expression(&query.text, query.match_case, direction)?
        } else {
            NearbySearch::new(
                &query.text,
                query.match_case,
                query.ignore_accents,
                direction,
            )?
        }
        .with_flow_config(context.flow.clone());
        let anchor = if from_edge {
            None
        } else if continuing {
            // Continue from the current match in the search direction, so a
            // second occurrence on the same visible page is not skipped.
            let current = self
                .result_selection
                .as_ref()
                .ok_or("The current match has moved")?;
            Some(match direction {
                SearchDirection::Next => current.end.clone(),
                SearchDirection::Previous => current.start.clone(),
            })
        } else {
            Some(
                active
                    .reading
                    .search_boundary(direction)
                    .ok_or("The current page has no searchable position")?,
            )
        };
        let (tx, rx) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let request = Request {
            engine,
            path: active.reading.config.book.clone(),
            chapter: active.reading.chapter,
            chapters: active.reading.probe.chapter_count(),
            anchor,
            publication: context.publication,
            stop_anchor: if from_edge {
                let boundary_direction = match direction {
                    SearchDirection::Next => SearchDirection::Previous,
                    SearchDirection::Previous => SearchDirection::Next,
                };
                active
                    .reading
                    .search_boundary(boundary_direction)
                    .filter(|anchor| anchor.illustration.is_none())
            } else {
                None
            },
            flow: context.flow.clone(),
            raw_layout: context.raw_layout.clone(),
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
            self.reset_search_chapter();
            self.search_result = None;
        }
        if self.nearby.worker.is_none() {
            self.nearby.worker = Some(Worker::start()?);
        }
        self.nearby.results_context = Some(context);
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
            self.controls
                .interface_language
                .document_search_text(match direction {
                    SearchDirection::Previous => "Searching earlier text",
                    SearchDirection::Next => "Searching later text",
                })
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
        let context = self
            .current_reflow_context()
            .ok_or("Open a reflowable book first")?;
        let path = active.reading.config.book.clone();
        let chapters = active.reading.probe.chapter_count();
        search = search
            .with_context_words(self.search_context_words())
            .with_flow_config(context.flow.clone())
            .for_publication(context.publication, chapters);
        self.update_lookup_history(|h| h.search(pattern, case, accents, regex))?;
        self.remember_all_results_origin();
        self.cancel_search();
        self.nearby.discard_all_completeness();
        self.search_result = None;
        self.reset_match_window();
        self.focused_result = None;
        self.show(Screen::Matches);
        self.hits.clear();
        self.pdf_hits.clear();
        if self.nearby.worker.is_none() {
            self.nearby.worker = Some(Worker::start()?);
        }
        let (tx, result) = mpsc::sync_channel(1);
        let cancelled = Arc::new(AtomicBool::new(false));
        self.nearby.results_context = Some(context.clone());
        self.nearby.all_completeness = Some(AllCompleteness::default());
        self.nearby.all = Some(PendingAll {
            cancel: Arc::clone(&cancelled),
            result,
            at: Instant::now() + SEARCH_DELAY,
        });
        self.nearby.worker.as_ref().unwrap().submit(move || {
            let scanned = (|| {
                let mut reported_skipped = 0;
                for ci in 0..chapters {
                    if cancelled.load(Ordering::Relaxed) || search.is_finished() {
                        break;
                    }
                    let results = search.step_loaded(
                        load_text(
                            &path,
                            ci,
                            &context.flow,
                            context.raw_layout.as_ref(),
                            &cancelled,
                        )?,
                        chapters,
                        &cancelled,
                    )?;
                    // One early update makes a cancelled partial list truthful
                    // without one UI round trip per unreadable chapter.
                    if reported_skipped == 0 && search.skipped_chapters() > 0 {
                        reported_skipped = search.skipped_chapters();
                        if tx.send(Ok(AllEvent::Skipped(reported_skipped))).is_err() {
                            break;
                        }
                    }
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
                    skipped: search.skipped_chapters(),
                })
            })();
            if !cancelled.load(Ordering::Relaxed) {
                let _ = tx.send(scanned);
            }
        });
        Ok(())
    }

    pub(super) fn poll_nearby(&mut self, now: Instant) -> Result<Change> {
        self.ensure_reflow_search_current()?;
        if self.nearby.pending.is_none()
            && self.nearby.fixed.is_none()
            && self.nearby.all.is_none()
            && self.nearby.page_pending.is_some()
        {
            return Ok(self.poll_page_marks(now));
        }
        if let Some(mut pending) = self.nearby.fixed.take() {
            let fixed = self.fixed.as_ref().ok_or("No open document")?;
            let Some(page) = pending.next_page else {
                let exhausted = match (pending.direction, pending.stop_before.is_some()) {
                    (SearchDirection::Next, true) => SearchDirection::Previous,
                    (SearchDirection::Previous, true) => SearchDirection::Next,
                    (direction, false) => direction,
                };
                self.nearby.end = Some(exhausted);
                self.restore_fixed_search_fit()?;
                let message = if pending.search.skipped > 0 {
                    "No match found. Some pages could not be searched"
                } else {
                    match exhausted {
                        SearchDirection::Previous => "No earlier matching page",
                        SearchDirection::Next => "No later matching page",
                    }
                };
                self.notice = Some(
                    self.controls
                        .interface_language
                        .document_search_text(message)
                        .into(),
                );
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
            let previous = if let Some(fit) = &self.nearby.fit {
                let previous = fit.original_location.clone();
                if let Err(error) = self
                    .fixed
                    .as_mut()
                    .unwrap()
                    .reading
                    .show_search_fit_page(page)
                {
                    self.restore_fixed_search_fit()?;
                    return Err(error);
                }
                self.pdf_selection = None;
                self.show(Screen::Fixed);
                previous
            } else {
                self.jump_fixed(page, FixedJump::regions(&found.focus))?
            };
            if !pending.continuing {
                self.push_history(previous);
            }
            self.pdf_result_regions = found
                .regions
                .into_iter()
                .map(|bounds| (page, bounds))
                .collect();
            if found.limited {
                self.notice = Some(
                    self.controls
                        .interface_language
                        .document_search_text("Showing the first 5,000 matches on this page")
                        .into(),
                );
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
                Ok(AllEvent::Skipped(skipped)) => {
                    if let Some(status) = &mut self.nearby.all_completeness {
                        status.skipped = skipped;
                    }
                    all.at = now + SEARCH_DELAY;
                }
                Ok(AllEvent::Finished {
                    limited,
                    memory_limited,
                    skipped,
                }) => {
                    self.nearby.all = None;
                    if let Some(status) = &mut self.nearby.all_completeness {
                        status.skipped = skipped;
                        status.limited = limited;
                        status.memory_limited = memory_limited;
                    }
                }
                Err(e) => {
                    self.nearby.all = None;
                    if let Some(status) = &mut self.nearby.all_completeness {
                        status.failed = true;
                    }
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
                self.controls
                    .interface_language
                    .document_search_text(match pending.direction {
                        SearchDirection::Previous => "No earlier match",
                        SearchDirection::Next => "No later match",
                    })
                    .into(),
            );
            return Ok(Change::Partial);
        };
        if !pending.continuing {
            self.search_result = None;
        }
        self.clear_flexible_results();
        self.nearby.discard_all_completeness();
        self.hits = vec![hit];
        self.activate(Action::SearchHit(0))?;
        self.nearby.current = Some(pending.query.clone());
        self.nearby.end = None;
        self.begin_page_marks(&pending.query);
        Ok(Change::Partial)
    }
}

#[cfg(test)]
#[path = "nearby_flow_tests.rs"]
mod flow_tests;

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
    fn fixed_search_fit_is_temporary_and_restores_zoom_pan_and_history() {
        let fixture = Fixture::new();
        let path = fixture.root.join("Coastal field guide.pdf");
        fs::write(&path, coastal_guide()).unwrap();
        let fonts = vec![
            crate::test_fixture("fonts/NotoSans-Regular.ttf"),
            crate::test_fixture("fonts/NotoSans-Bold.ttf"),
        ];
        let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
        app.open_path(&path).unwrap();
        assert!(!app.controls.fixed_search_full_page);
        app.activate(Action::SetReadingControls(ReadingControls {
            fixed_search_full_page: true,
            ..app.controls
        }))
        .unwrap();
        let reading = &mut app.fixed.as_mut().unwrap().reading;
        reading.jump(2).unwrap();
        reading.rotate().unwrap();
        reading.set_zoom(3).unwrap();
        let touch = hayro::kurbo::Point::new(120.0, 140.0);
        reading
            .jump_to(2, Some(&FixedJump::LinkPoint(touch)))
            .unwrap();
        let original_view = reading.preferences();
        assert_eq!(original_view.zoom, 3);
        assert_ne!((original_view.offset_x, original_view.offset_y), (0.0, 0.0));
        let origin = app.current_history_location().unwrap();
        app.record_position();
        app.save().unwrap();
        let begin = |app: &mut Reader, query: &str| {
            app.show(Screen::Find);
            app.editor.text = query.chars().collect();
            app.editor.caret = app.editor.text.len();
            assert_eq!(tap(app, FixedNavigation::Next.entry().0), Change::Partial);
            assert!(app.fixed_search_fit_active());
            let view = app.fixed.as_ref().unwrap().reading.preferences();
            assert_eq!((view.zoom, view.turns), (0, original_view.turns));
            assert!(!view.width_fit && !view.height_fit && !view.continuous);
        };

        begin(&mut app, "beacon");
        assert_eq!(tap(&mut app, PREVIOUS), Change::Partial);
        assert!(!app.fixed_search_fit_active());
        assert_eq!(app.current_history_location(), Some(origin.clone()));
        assert_eq!(
            app.fixed.as_ref().unwrap().reading.preferences(),
            original_view
        );
        assert!(app.back.is_empty());

        begin(&mut app, "no such phrase appears");
        settle(&mut app);
        assert!(!app.fixed_search_fit_active());
        assert_eq!(app.current_history_location(), Some(origin.clone()));
        assert!(app.back.is_empty());

        begin(&mut app, "beacon");
        settle(&mut app);
        assert_eq!(app.screen, Screen::Fixed);
        assert_eq!(app.fixed.as_ref().unwrap().reading.page(), 2);
        assert_eq!(app.pdf_result_regions.len(), 2);
        assert!(app.fixed_search_fit_active());
        capture_test_screen(&app, &fonts, "READER_FIXED_SEARCH_FIT_ROTATED_FRAME", false);
        assert_eq!(app.back, [origin.clone()]);
        assert_eq!(
            tap(&mut app, FixedNavigation::Next.button()),
            Change::Partial
        );
        settle(&mut app);
        assert_eq!(app.fixed.as_ref().unwrap().reading.page(), 4);
        assert_eq!(app.back, [origin.clone()]);

        app.save().unwrap();
        let mut reopened = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
        reopened.open_path(&path).unwrap();
        assert_eq!(reopened.current_history_location(), Some(origin.clone()));
        assert_eq!(
            reopened.fixed.as_ref().unwrap().reading.preferences(),
            original_view
        );
        assert!(reopened.controls.fixed_search_full_page);
        assert_eq!(tap(&mut app, SEARCH_FIT_CLOSE), Change::Partial);
        assert!(!app.fixed_search_fit_active());
        assert!(app.pdf_result_regions.is_empty());
        assert_eq!(app.fixed.as_ref().unwrap().reading.page(), 4);
        assert_eq!(
            app.fixed.as_ref().unwrap().reading.preferences(),
            original_view
        );
        assert_eq!(app.back, [origin.clone()]);
        app.save().unwrap();
        let mut reopened = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
        reopened.open_path(&path).unwrap();
        assert_eq!(reopened.fixed.as_ref().unwrap().reading.page(), 4);
        assert_eq!(
            reopened.fixed.as_ref().unwrap().reading.preferences(),
            original_view
        );
        app.activate(Action::ReturnToLocation).unwrap();
        let restored = app.current_history_location().unwrap();
        assert_eq!(restored.position, origin.position);
        assert!(
            restored
                .scan_point
                .unwrap()
                .distance(origin.scan_point.unwrap())
                < 0.5
        );

        // An adjacent result can exhaust, restoring the saved reading mode
        // at the reached page while its search controls remain available.
        begin(&mut app, "beacon");
        settle(&mut app);
        assert_eq!(
            tap(&mut app, FixedNavigation::Next.button()),
            Change::Partial
        );
        settle(&mut app);
        assert_eq!(
            tap(&mut app, FixedNavigation::Next.button()),
            Change::Partial
        );
        settle(&mut app);
        assert!(!app.fixed_search_fit_active());
        assert_eq!(app.fixed.as_ref().unwrap().reading.page(), 4);
        assert_eq!(
            app.fixed.as_ref().unwrap().reading.preferences().zoom,
            original_view.zoom
        );
        assert_eq!(
            tap(&mut app, FixedNavigation::Previous.button()),
            Change::Partial
        );
        assert!(app.nearby.searching_direction());
        app.activate(Action::ReturnToLocation).unwrap();
        assert!(!app.nearby.searching_direction());
        assert!(!app.fixed_search_fit_active());
        assert_eq!(app.fixed.as_ref().unwrap().reading.page(), 2);
    }

    #[test]
    fn directional_search_fit_restores_continuous_view_before_an_assigned_gesture() {
        let fixture = Fixture::new();
        let path = fixture.root.join("Coastal field guide.pdf");
        fs::write(&path, coastal_guide()).unwrap();
        let fonts = vec![
            crate::test_fixture("fonts/NotoSans-Regular.ttf"),
            crate::test_fixture("fonts/NotoSans-Bold.ttf"),
        ];
        let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
        app.open_path(&path).unwrap();
        app.controls.fixed_search_full_page = true;
        app.activate(Action::FixedScroll(true)).unwrap();
        app.activate(Action::SetFixedZoom(2)).unwrap();
        let reading = &mut app.fixed.as_mut().unwrap().reading;
        reading.jump(2).unwrap();
        assert!(reading.preferences().continuous && reading.preferences().width_fit);
        app.show(Screen::Find);
        app.editor.text = "beacon".chars().collect();
        assert_eq!(
            tap(&mut app, FixedNavigation::Next.entry().0),
            Change::Partial
        );
        settle(&mut app);
        assert!(app.fixed_search_fit_active());
        assert!(!app.fixed.as_ref().unwrap().reading.is_continuous());
        capture_test_screen(&app, &fonts, "READER_FIXED_SEARCH_FIT_FRAME", false);
        app.gestures
            .set(Gesture::LeftUp, Binding::Bookmark)
            .unwrap();
        assert_eq!(
            app.swipe(
                Input::Swipe {
                    start_y: 400.0,
                    dx: 0.0,
                    dy: -180.0
                },
                30.0
            ),
            Change::Partial
        );
        assert!(!app.fixed_search_fit_active());
        assert!(app.fixed.as_ref().unwrap().reading.is_continuous());
        assert_eq!(app.fixed.as_ref().unwrap().reading.preferences().zoom, 2);
        assert!(app.page_is_bookmarked());
    }

    #[test]
    fn fixed_results_list_temporarily_fits_the_exact_match_and_returns_to_list_or_reading() {
        let fixture = Fixture::new();
        let path = fixture.root.join("Coastal field guide.pdf");
        fs::write(&path, coastal_guide()).unwrap();
        let fonts = vec![
            crate::test_fixture("fonts/NotoSans-Regular.ttf"),
            crate::test_fixture("fonts/NotoSans-Bold.ttf"),
        ];
        let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
        app.open_path(&path).unwrap();
        assert!(!app.controls.fixed_search_full_page);
        let reading = &mut app.fixed.as_mut().unwrap().reading;
        reading.set_zoom(3).unwrap();
        let point = hayro::kurbo::Point::new(120.0, 140.0);
        reading
            .jump_to(0, Some(&FixedJump::LinkPoint(point)))
            .unwrap();
        let original_view = reading.preferences();
        assert_ne!((original_view.offset_x, original_view.offset_y), (0.0, 0.0));
        let origin = app.current_history_location().unwrap();
        app.record_position();
        app.save().unwrap();

        app.start_document_search("beacon").unwrap();
        while app.search_at.is_some() {
            app.step_pdf_search(Instant::now()).unwrap();
        }
        assert!(!app.fixed_search_fit_active());
        assert_eq!(app.current_history_location(), Some(origin.clone()));
        let index = app.pdf_hits.iter().position(|hit| hit.page == 2).unwrap();
        let geometry = app.match_geometry();
        app.list_page = index / geometry.rows;
        assert!(app.list_page > 0);
        let row = geometry.row(index % geometry.rows);
        assert_eq!(
            app.input(Input::Tap((LIST_X + 30) as f32, (row.y + row.h / 2) as f32)),
            Change::Partial
        );

        let result = |app: &Reader, index: usize| {
            assert_eq!(app.screen, Screen::Fixed);
            assert!(app.fixed_list_result_active());
            let reading = &app.fixed.as_ref().unwrap().reading;
            let hit = &app.pdf_hits[index];
            assert_eq!(reading.page(), hit.page);
            let view = reading.preferences();
            assert_eq!(view.zoom, 0);
            assert_eq!(view.turns, original_view.turns);
            assert!(!view.continuous && !view.width_fit && !view.height_fit);
            assert_eq!(
                app.pdf_result_regions,
                hit.bounds
                    .iter()
                    .map(|&b| (hit.page, b))
                    .collect::<Vec<_>>()
            );
            assert!(
                hit.bounds
                    .iter()
                    .all(|&b| reading.visible_rectangle(hit.page, b).is_some())
            );
            assert_eq!(app.back, [origin.clone()]);
        };
        result(&app, index);
        capture_test_screen(&app, &fonts, "READER_FIXED_LIST_FIT_FRAME", false);
        assert_eq!(app.pdf_hits[index + 1].page, 2);
        assert_ne!(app.pdf_hits[index].bounds, app.pdf_hits[index + 1].bounds);
        assert_eq!(tap(&mut app, LIST_RESULT_NEXT), Change::Partial);
        result(&app, index + 1);
        assert_eq!(tap(&mut app, LIST_RESULT_PREVIOUS), Change::Partial);
        result(&app, index);
        let hold = |app: &mut Reader, rect: Rect| {
            app.input(Input::LongTap(
                (rect.x + rect.w / 2) as f32,
                (rect.y + rect.h / 2) as f32,
            ))
        };
        assert_eq!(hold(&mut app, LIST_RESULT_PREVIOUS), Change::Partial);
        result(&app, 0);
        assert_eq!(hold(&mut app, LIST_RESULT_PREVIOUS), Change::None);
        let x = (LIST_RESULT_NEXT.x + LIST_RESULT_NEXT.w / 2) as f32;
        let y = (LIST_RESULT_NEXT.y + LIST_RESULT_NEXT.h / 2) as f32;
        assert_eq!(app.input(Input::HoldStart(x, y)), Change::None);
        assert_eq!(
            app.input(Input::HoldRelease {
                start_x: x,
                start_y: y,
                x,
                y,
                held_ms: 700,
            }),
            Change::Partial
        );
        assert_eq!(app.pdf_hits[index + 2].page, 4);
        assert_eq!(app.pdf_hits.len(), index + 3);
        result(&app, index + 2);
        assert_eq!(tap(&mut app, LIST_RESULT_NEXT), Change::None);
        app.save().unwrap();
        let mut reopened = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
        reopened.open_path(&path).unwrap();
        assert_eq!(reopened.current_history_location(), Some(origin.clone()));
        assert_eq!(
            reopened.fixed.as_ref().unwrap().reading.preferences(),
            original_view
        );

        assert_eq!(tap(&mut app, LIST_RESULT_LIST), Change::Partial);
        assert_eq!(app.screen, Screen::Matches);
        assert_eq!(app.list_page, (index + 2) / geometry.rows);
        assert_eq!(app.focused_result, Some(index + 2));
        assert!(!app.fixed_search_fit_active());
        assert!(app.pdf_result_regions.is_empty());
        let reading = &app.fixed.as_ref().unwrap().reading;
        assert_eq!(reading.page(), 4);
        assert_eq!(reading.preferences(), original_view);
        assert_eq!(
            tap(&mut app, primitive_ui::back_button(WIDTH as i32)),
            Change::Partial
        );
        assert_eq!(app.screen, Screen::Fixed);
        assert_eq!(app.fixed.as_ref().unwrap().reading.page(), 4);
        app.activate(Action::ReturnToLocation).unwrap();
        assert_eq!(app.current_history_location(), Some(origin.clone()));

        // Ordinary location history restores a source point. Converting it
        // to screen offsets can round differently than the original view.
        let previous_view = app.fixed.as_ref().unwrap().reading.preferences();
        app.activate(Action::PdfSearchHit(index)).unwrap();
        assert_eq!(app.input(Input::Tap(600.0, 700.0)), Change::Partial);
        assert!(!app.fixed_search_fit_active());
        assert!(app.pdf_result_regions.is_empty());
        assert_eq!(app.fixed.as_ref().unwrap().reading.page(), 2);
        assert_eq!(
            app.fixed.as_ref().unwrap().reading.preferences(),
            previous_view
        );
        app.activate(Action::ReturnToLocation).unwrap();
        assert_eq!(app.current_history_location(), Some(origin));

        let previous_view = app.fixed.as_ref().unwrap().reading.preferences();
        app.activate(Action::PdfSearchHit(index)).unwrap();
        assert_eq!(tap(&mut app, SEARCH_FIT_CLOSE), Change::Partial);
        assert!(!app.fixed_search_fit_active());
        assert!(app.pdf_result_regions.is_empty());
        app.save().unwrap();
        let mut reopened = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
        reopened.open_path(&path).unwrap();
        let reading = &mut reopened.fixed.as_mut().unwrap().reading;
        assert_eq!(reading.page(), 2);
        assert_eq!(reading.preferences(), previous_view);
        reading.rotate().unwrap();
        let rotated_view = reading.preferences();
        let rotated_origin = reopened.current_history_location();
        assert_ne!(rotated_view.turns, original_view.turns);
        reopened.start_document_search("beacon").unwrap();
        while reopened.search_at.is_some() {
            reopened.step_pdf_search(Instant::now()).unwrap();
        }
        reopened.activate(Action::PdfSearchHit(index)).unwrap();
        let reading = &reopened.fixed.as_ref().unwrap().reading;
        assert_eq!(reading.preferences().turns, rotated_view.turns);
        assert!(
            reopened
                .pdf_result_regions
                .iter()
                .all(|&(page, bounds)| { reading.visible_rectangle(page, bounds).is_some() })
        );
        assert_eq!(tap(&mut reopened, SEARCH_FIT_CLOSE), Change::Partial);
        assert_eq!(reopened.current_history_location(), rotated_origin);
        assert_eq!(
            reopened.fixed.as_ref().unwrap().reading.preferences(),
            rotated_view
        );
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
            assert!(app.pdf_hits.len() > 100);
            assert!(app.pdf_hits[99].page < origin);
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
        // U+2026 and its adjacent closing quote are 0x85 and 0x94 in PDF WinAnsi.
        let content = r"BT /F1 11 Tf 28 440 Td (HARBOUR TIMETABLE) Tj 0 -46 Td /F1 22 Tf (Gate notices) Tj 0 -43 Td /F1 13 Tf (Boarding\205\224 Please wait at gate four.) Tj 0 -25 Td (Departures resume when the tide turns.) Tj ET";
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
        let quote = text
            .words
            .iter()
            .find(|w| &text.text[w.bytes.clone()] == "”")
            .unwrap();
        assert_eq!(printed.bytes.end, quote.bytes.start);
        assert!(printed.bounds.x1 <= quote.bounds.x0 + 0.001);
        assert_eq!(
            PdfSearch::new("... ", false).unwrap().hits(0, &text, 10)[0].bounds,
            [printed.bounds]
        );
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
            let listed = search.hits(page - 1, &text, crate::search::MAX_SEARCH_HITS);
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
