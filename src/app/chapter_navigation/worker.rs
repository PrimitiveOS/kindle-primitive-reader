//! Bounded, cancellable reflow chapter navigation for verified EPUB layouts.
use super::*;
use crate::session::{Reading, SearchLayoutRecipe};
use primitive_app::Change;
use primitive_ui::{BODY, CAPTION, Canvas, Input, Rect};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};

const MAX_TARGETS: usize = 2048;
const MAX_CHAPTERS: usize = 64;
const MAX_SNAPSHOT_BYTES: usize = 1024 * 1024;
const POLL: Duration = Duration::from_millis(100);

fn panel(reader: &Reader) -> (Rect, Rect) {
    let layout = reader.navigation_layout();
    let width = (layout.width - 80).clamp(200, 760);
    let height = (layout.height - 80).clamp(260, 380);
    let panel = Rect::new(
        (layout.width - width) / 2,
        (layout.height - height) / 2,
        width,
        height,
    );
    let button = Rect::new(panel.x + (width - 240) / 2, panel.bottom() - 120, 240, 100);
    (panel, button)
}

struct Guard(Arc<AtomicBool>);
impl Drop for Guard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

struct Outcome {
    chapter: usize,
    page: usize,
    prepared: crate::PreparedParleyChapter,
}

pub(in crate::app) struct Job {
    id: u64,
    screen: super::super::Screen,
    origin: super::super::HistoryLocation,
    scroll_top: Option<f32>,
    recipe: SearchLayoutRecipe,
    outline: Option<(Vec<(usize, ContentsEntry)>, Vec<Bookmark>)>,
    page_jump: bool,
    cancel: Arc<AtomicBool>,
    receiver: mpsc::Receiver<std::result::Result<Option<Outcome>, String>>,
    deadline: Instant,
}
impl Drop for Job {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}
impl Job {
    pub(in crate::app) fn deadline(&self) -> Instant {
        self.deadline
    }
}

#[derive(Clone)]
enum Wanted {
    Start,
    Fragment(String),
    Anchor(Anchor),
}
fn target_bytes(target: &Target) -> usize {
    match target {
        Target::Reflow(destination) => destination
            .path
            .len()
            .saturating_add(destination.fragment.as_ref().map_or(0, String::len)),
        Target::Alternative(OutlineTarget::Heading(_, anchor)) => anchor.chapter.len(),
        _ => 0,
    }
}
impl Wanted {
    fn page(&self, probe: &Probe) -> Option<usize> {
        match self {
            Self::Start => Some(0),
            Self::Fragment(fragment) => probe.page_for(&probe.fragment(fragment)?),
            Self::Anchor(anchor) => probe.page_for(anchor),
        }
    }
}

impl Reader {
    pub(in crate::app) fn can_defer_chapter_navigation(&self) -> bool {
        let Some(active) = &self.active else {
            return false;
        };
        let reading = &active.reading;
        let recipe = reading.config.search_layout_recipe(&reading.probe);
        self.screen != Screen::Skim
            && self.touch_hold.is_none()
            && !self.scroll_pan.busy()
            && active.entry.format == crate::library::DocumentFormat::Epub
            && !crate::epub_members::is_folder_book(&active.entry.path)
            && !reading.page_turn_pending()
            && !reading.probe.has_raw_layout()
            && reading.probe.in_page_notes.is_none()
            && reading.probe.foreign_note_bands.is_empty()
            && recipe.has_verified_sources()
            && recipe.allows_prepared_navigation()
    }

    pub(in crate::app) fn can_defer_relative_page_navigation(&self, delta: isize) -> bool {
        let Some(active) = &self.active else {
            return false;
        };
        let reading = &active.reading;
        let entries = reading.probe.chapter_entries();
        if !self.can_defer_chapter_navigation()
            || reading.config.scroll
            || reading.config.facing_pages
            || reading.is_unsupported_spine_notice()
            || delta == 0
            || delta.unsigned_abs() > 100_000
            || entries.is_empty()
            || reading.chapter >= entries.len()
            || entries.len() > 4096
            || entries.iter().any(|entry| entry.nonlinear)
        {
            return false;
        }
        let mut bytes = 0usize;
        let mut seen = std::collections::HashSet::new();
        entries.iter().all(|entry| {
            bytes = bytes.saturating_add(entry.href.len());
            bytes <= MAX_SNAPSHOT_BYTES && seen.insert(entry.href.as_str())
        })
    }

    pub(super) fn start_chapter_navigation(
        &mut self,
        direction: Direction,
        headings: Vec<(usize, ContentsEntry)>,
        custom: Vec<Bookmark>,
    ) -> Result<()> {
        let active = self.active.as_ref().ok_or("Open a book first")?;
        let reading = &active.reading;
        let recipe = reading.config.search_layout_recipe(&reading.probe);
        let heading_bytes = headings.iter().try_fold(0usize, |total, (_, heading)| {
            total
                .checked_add(heading.title.len())?
                .checked_add(target_bytes(&heading.target))
        });
        let custom_bytes = custom.iter().try_fold(0usize, |total, mark| {
            total
                .checked_add(mark.label.len())?
                .checked_add(mark.note.as_ref().map_or(0, String::len))?
                .checked_add(mark.chapter_title.as_ref().map_or(0, String::len))?
                .checked_add(
                    mark.position
                        .anchor
                        .as_ref()
                        .map_or(0, |anchor| anchor.chapter.len()),
                )
        });
        if !self.can_defer_chapter_navigation()
            || headings.len() > MAX_TARGETS
            || custom.len() > MAX_TARGETS
            || heading_bytes
                .zip(custom_bytes)
                .and_then(|(a, b)| a.checked_add(b))
                .is_none_or(|size| size > MAX_SNAPSHOT_BYTES)
        {
            return Err("Chapter navigation is unavailable for this layout".into());
        }
        let chapters = reading.probe.chapter_entries();
        if chapters.len() > 4096
            || chapters
                .iter()
                .try_fold(0usize, |total, chapter| {
                    total.checked_add(chapter.href.len())
                })
                .is_none_or(|size| size > MAX_SNAPSHOT_BYTES)
        {
            return Err("The chapter list exceeds the navigation limit".into());
        }
        let mut by_path = HashMap::new();
        for (index, chapter) in chapters.iter().enumerate() {
            if by_path.insert(chapter.href.clone(), index).is_some() {
                return Err("Chapter navigation requires unique chapter paths".into());
            }
        }
        let mut groups: BTreeMap<usize, Vec<Wanted>> = BTreeMap::new();
        for (_, heading) in &headings {
            let (chapter, target) = match &heading.target {
                Target::Reflow(destination) => {
                    let Some(&chapter) = by_path.get(&destination.path) else {
                        continue;
                    };
                    let target = destination
                        .fragment
                        .as_ref()
                        .map_or(Wanted::Start, |fragment| Wanted::Fragment(fragment.clone()));
                    (chapter, target)
                }
                Target::Alternative(OutlineTarget::Chapter(chapter)) => (*chapter, Wanted::Start),
                Target::Alternative(OutlineTarget::Heading(chapter, anchor)) => {
                    (*chapter, Wanted::Anchor(anchor.clone()))
                }
                Target::Custom(index) => {
                    let Some(position) = custom.get(*index).map(|item| &item.position) else {
                        continue;
                    };
                    let Some(anchor) = &position.anchor else {
                        continue;
                    };
                    if by_path.get(&anchor.chapter) != Some(&position.chapter) {
                        continue;
                    }
                    (position.chapter, Wanted::Anchor(anchor.clone()))
                }
                _ => continue,
            };
            if chapter < chapters.len() && direction.beyond(chapter, reading.chapter) {
                groups.entry(chapter).or_default().push(target);
            }
        }
        if groups.is_empty() {
            self.notice = Some(
                match direction {
                    Direction::Previous => "No previous chapter",
                    Direction::Next => "No next chapter",
                }
                .into(),
            );
            return Ok(());
        }
        let id = active.id;
        let publication = reading.probe.book;
        let path = reading.config.book.clone();
        let origin = self
            .current_history_location()
            .ok_or("The reading position is unavailable")?;
        let scroll_top = reading.scroll_history_top();
        if self.chapter_navigation_busy.swap(true, Ordering::AcqRel) {
            return Err("The previous chapter search is still stopping".into());
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancel);
        let busy = Arc::clone(&self.chapter_navigation_busy);
        let worker_recipe = recipe.clone();
        let (send, receiver) = mpsc::sync_channel(1);
        if std::thread::Builder::new()
            .name("reader-chapter-navigation".into())
            .spawn(move || {
                let _guard = Guard(busy);
                let result = run(
                    groups,
                    direction,
                    worker_recipe,
                    path,
                    publication,
                    &worker_cancel,
                )
                .map_err(|error| error.to_string());
                if !worker_cancel.load(Ordering::Relaxed) {
                    let _ = send.send(result);
                }
            })
            .is_err()
        {
            self.chapter_navigation_busy.store(false, Ordering::Release);
            return Err("Could not start chapter navigation".into());
        }
        self.chapter_navigation_job = Some(Job {
            id,
            screen: self.screen,
            origin,
            scroll_top,
            recipe,
            outline: Some((headings, custom)),
            page_jump: false,
            cancel,
            receiver,
            deadline: Instant::now() + POLL,
        });
        Ok(())
    }

    /// Count only the intervening chapters; no whole-book page index is required.
    pub(in crate::app) fn start_relative_page_navigation(&mut self, delta: isize) -> Result<()> {
        let active = self.active.as_ref().ok_or("Open a book first")?;
        let reading = &active.reading;
        if !self.can_defer_chapter_navigation()
            || reading.config.scroll
            || reading.config.facing_pages
            || reading.is_unsupported_spine_notice()
            || delta == 0
            || delta.unsigned_abs() > 100_000
        {
            return Err("This relative page jump is unavailable for the current layout".into());
        }
        let entries = reading.probe.chapter_entries();
        if entries.is_empty() || entries.len() > 4096 || entries.iter().any(|entry| entry.nonlinear)
        {
            return Err("This relative page jump needs a simple linear spine".into());
        }
        let mut seen = std::collections::HashSet::new();
        let mut bytes = 0usize;
        let mut paths = Vec::with_capacity(entries.len());
        for entry in entries {
            bytes = bytes
                .checked_add(entry.href.len())
                .ok_or("The chapter list is too large")?;
            if bytes > MAX_SNAPSHOT_BYTES || !seen.insert(entry.href.as_str()) {
                return Err("This relative page jump needs unique chapter paths".into());
            }
            paths.push(entry.href.clone());
        }
        let count = reading.probe.page_count();
        if count == 0 || reading.page >= count {
            return Err("The current page is unavailable".into());
        }
        let edge = if delta > 0 && reading.chapter + 1 == entries.len() {
            Some(count - 1)
        } else if delta < 0 && reading.chapter == 0 {
            Some(0)
        } else {
            None
        };
        if edge == Some(reading.page) {
            self.show(Screen::Book);
            return Ok(());
        }
        let origin = self
            .current_history_location()
            .ok_or("The reading position is unavailable")?;
        let id = active.id;
        let publication = reading.probe.book;
        let path = reading.config.book.clone();
        let chapter = reading.chapter;
        let page = reading.page;
        let scroll_top = reading.scroll_history_top();
        let recipe = reading.config.search_layout_recipe(&reading.probe);
        if self.chapter_navigation_busy.swap(true, Ordering::AcqRel) {
            return Err("The previous navigation is still stopping".into());
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancel);
        let busy = Arc::clone(&self.chapter_navigation_busy);
        let worker_recipe = recipe.clone();
        let (send, receiver) = mpsc::sync_channel(1);
        if std::thread::Builder::new()
            .name("reader-relative-page".into())
            .spawn(move || {
                let _guard = Guard(busy);
                let result = run_relative(
                    worker_recipe,
                    path,
                    publication,
                    paths,
                    chapter,
                    page,
                    count,
                    delta,
                    &worker_cancel,
                )
                .map_err(|error| error.to_string());
                if !worker_cancel.load(Ordering::Relaxed) {
                    let _ = send.send(result);
                }
            })
            .is_err()
        {
            self.chapter_navigation_busy.store(false, Ordering::Release);
            return Err("Could not start page navigation".into());
        }
        self.chapter_navigation_job = Some(Job {
            id,
            screen: self.screen,
            origin,
            scroll_top,
            recipe,
            outline: None,
            page_jump: true,
            cancel,
            receiver,
            deadline: Instant::now() + POLL,
        });
        Ok(())
    }

    pub(in crate::app) fn chapter_navigation_input(&mut self, input: Input) -> Option<Change> {
        let page_jump = self.chapter_navigation_job.as_ref()?.page_jump;
        if matches!(input, Input::Sleep | Input::Wake) {
            return None;
        }
        if input == Input::PageForward {
            // A later hardware request belongs to the user. Cancel the worker
            // and let the ordinary input path process that request.
            self.chapter_navigation_job = None;
            self.notice = Some(
                if page_jump {
                    "Page navigation cancelled"
                } else {
                    "Chapter navigation cancelled"
                }
                .into(),
            );
            return None;
        }
        let cancel = match input {
            Input::PageBack => true,
            Input::Tap(x, y) => panel(self).1.contains(x, y),
            _ => false,
        };
        if cancel {
            self.chapter_navigation_job = None;
            self.notice = Some(
                if page_jump {
                    "Page navigation cancelled"
                } else {
                    "Chapter navigation cancelled"
                }
                .into(),
            );
            return Some(Change::Partial);
        }
        Some(Change::None)
    }

    pub(in crate::app) fn draw_chapter_navigation(&self, canvas: &mut Canvas) {
        if let Some(job) = &self.chapter_navigation_job {
            let (area, cancel) = panel(self);
            canvas.popup(area);
            canvas.text(
                Rect::new(area.x + 36, area.y + 24, area.w - 72, 64),
                if job.page_jump {
                    "Finding page"
                } else {
                    "Finding chapter"
                },
                BODY.bold(),
            );
            canvas.text(
                Rect::new(area.x + 36, area.y + 108, area.w - 72, 64),
                if job.page_jump {
                    "Counting nearby pages…"
                } else {
                    "Checking the next chapter…"
                },
                CAPTION,
            );
            canvas.button(cancel, "Cancel", false, false);
        }
    }
    pub(in crate::app) fn poll_chapter_navigation(&mut self, now: Instant) -> Option<Change> {
        let mut job = self.chapter_navigation_job.take()?;
        if now < job.deadline {
            self.chapter_navigation_job = Some(job);
            return None;
        }
        let result = match job.receiver.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => {
                job.deadline = now + POLL;
                self.chapter_navigation_job = Some(job);
                return None;
            }
            Err(mpsc::TryRecvError::Disconnected) => Err("Chapter navigation stopped".into()),
        };
        let current = self.screen == job.screen
            && self.open_book_id() == Some(job.id)
            && self.current_history_location().as_ref() == Some(&job.origin)
            && self.touch_hold.is_none()
            && !self.scroll_pan.busy()
            && self.active.as_ref().is_some_and(|active| {
                !active.reading.page_turn_pending()
                    && active.reading.scroll_history_top() == job.scroll_top
                    && active
                        .reading
                        .config
                        .search_layout_recipe(&active.reading.probe)
                        == job.recipe
            });
        if !current {
            return Some(self.report("The book or reading position changed; try again"));
        }
        if let Some((headings, custom)) = &job.outline {
            let state = self.library.state(job.id);
            let current_headings: Vec<_> = self
                .contents_entries(job.id)
                .into_iter()
                .enumerate()
                .filter(|(_, heading)| {
                    state
                        .is_none_or(|state| state.contents_depths.use_for_navigation(heading.depth))
                })
                .collect();
            if &current_headings != headings
                || state.is_none_or(|state| state.custom_contents.entries() != custom.as_slice())
            {
                return Some(self.report("The book or chapter list changed; try again"));
            }
        }
        match result {
            Ok(Some(outcome)) => {
                let reading = &self.active.as_ref().unwrap().reading;
                let restored = Reading::restore_prepared(
                    reading.config.clone(),
                    outcome.chapter,
                    outcome.page,
                    None,
                    outcome.prepared,
                );
                match restored {
                    Ok(restored)
                        if restored.chapter == outcome.chapter
                            && restored.page <= outcome.page
                            && restored.last_visible_page() >= outcome.page
                            && restored.config.search_layout_recipe(&restored.probe)
                                == job.recipe
                            && restored
                                .config
                                .search_layout_recipe(&restored.probe)
                                .has_verified_sources() =>
                    {
                        self.active.as_mut().unwrap().reading = restored;
                        self.finish_reflow_jump(job.origin.position.clone());
                        Some(Change::Full)
                    }
                    Ok(_) => Some(self.report("The chapter changed; try again")),
                    Err(error) => Some(self.report(error)),
                }
            }
            Ok(None) => {
                self.notice = Some("No chapter found in that direction".into());
                Some(Change::Partial)
            }
            Err(error) => Some(self.report(error)),
        }
    }
}

fn run(
    mut groups: BTreeMap<usize, Vec<Wanted>>,
    direction: Direction,
    recipe: SearchLayoutRecipe,
    path: std::path::PathBuf,
    publication: u64,
    cancel: &AtomicBool,
) -> Result<Option<Outcome>> {
    let available = primitive_app::with_storage(|| {
        let book = recipe
            .open_search_book(&path)
            .map_err(std::io::Error::other)?;
        if book.revision() != publication {
            return Err(std::io::Error::other("The publication changed"));
        }
        groups
            .keys()
            .map(|&chapter| {
                let entry = book
                    .chapters
                    .get(chapter)
                    .ok_or_else(|| std::io::Error::other("The chapter is unavailable"))?;
                Ok((chapter, !book.recovered_placeholder(entry)))
            })
            .collect::<std::io::Result<BTreeMap<_, _>>>()
    })?;
    let mut visited = 0usize;
    while let Some((chapter, targets)) = match direction {
        Direction::Previous => groups.pop_last(),
        Direction::Next => groups.pop_first(),
    } {
        if cancel.load(Ordering::Relaxed) {
            return Err("Chapter navigation cancelled".into());
        }
        if visited >= MAX_CHAPTERS {
            return Err("Too many chapters to search for the next heading".into());
        }
        visited += 1;
        if available.get(&chapter) != Some(&true) {
            return Err("The next chapter could not be checked".into());
        }
        let (probe, exact) = recipe.chapter_with_quote_status(chapter, cancel)?;
        if !exact {
            return Err("This chapter requires foreground layout".into());
        }
        let page = direction.closest(targets.iter().filter_map(|target| target.page(&probe)));
        if let Some(page) = page {
            let prepared = probe
                .into_prepared_parley()
                .ok_or("This chapter cannot be prepared")?;
            return Ok(Some(Outcome {
                chapter,
                page,
                prepared,
            }));
        }
    }
    Ok(None)
}

fn run_relative(
    recipe: SearchLayoutRecipe,
    path: std::path::PathBuf,
    publication: u64,
    paths: Vec<String>,
    mut chapter: usize,
    page: usize,
    current_count: usize,
    delta: isize,
    cancel: &AtomicBool,
) -> Result<Option<Outcome>> {
    let forward = delta > 0;
    let amount = delta.unsigned_abs();
    let to_boundary = if forward {
        current_count - page
    } else {
        page + 1
    };
    let mut remaining = amount
        .checked_sub(to_boundary)
        .ok_or("This page jump stays in the current chapter")?;
    if cancel.load(Ordering::Relaxed) {
        return Err("Page navigation cancelled".into());
    }
    let available = primitive_app::with_storage(|| {
        if cancel.load(Ordering::Relaxed) {
            return Err(std::io::Error::other("Page navigation cancelled"));
        }
        let book = recipe
            .open_search_book(&path)
            .map_err(std::io::Error::other)?;
        if book.revision() != publication
            || book.chapters.len() != paths.len()
            || book
                .chapters
                .iter()
                .zip(&paths)
                .any(|(entry, expected)| &entry.href != expected || entry.nonlinear)
        {
            return Err(std::io::Error::other("The publication changed"));
        }
        Ok(book
            .chapters
            .iter()
            .map(|entry| !book.recovered_placeholder(entry))
            .collect::<Vec<_>>())
    })?;
    if available.get(chapter) != Some(&true) {
        return Err("The current chapter cannot be counted exactly".into());
    }
    // If the current chapter is already at a publication edge, recheck its
    // source/layout before clamping to its first or last page.
    if (!forward && chapter == 0) || (forward && chapter + 1 == paths.len()) {
        let (probe, exact) = recipe.chapter_with_quote_status(chapter, cancel)?;
        if !exact || probe.is_vertical() || probe.has_raw_layout()
            || probe.in_page_notes.is_some() || !probe.foreign_note_bands.is_empty()
            || probe.page_count() != current_count
        {
            return Err("The current chapter cannot be counted exactly".into());
        }
        let page = if forward { current_count - 1 } else { 0 };
        let prepared = probe.into_prepared_parley().ok_or("This page cannot be prepared")?;
        return Ok(Some(Outcome { chapter, page, prepared }));
    }
    for _ in 0..MAX_CHAPTERS {
        if cancel.load(Ordering::Relaxed) {
            return Err("Page navigation cancelled".into());
        }
        chapter = if forward {
            chapter.checked_add(1).filter(|index| *index < paths.len())
        } else {
            chapter.checked_sub(1)
        }
        .ok_or("This page jump has no neighboring chapter")?;
        if available.get(chapter) != Some(&true) {
            return Err("An intervening chapter could not be checked".into());
        }
        let (probe, exact) = recipe.chapter_with_quote_status(chapter, cancel)?;
        if !exact
            || probe.is_vertical()
            || probe.has_raw_layout()
            || probe.in_page_notes.is_some()
            || !probe.foreign_note_bands.is_empty()
        {
            return Err("An intervening chapter cannot be counted exactly".into());
        }
        let count = probe.page_count();
        if count == 0 {
            return Err("An intervening chapter has no pages".into());
        }
        let at_end = if forward {
            chapter + 1 == paths.len()
        } else {
            chapter == 0
        };
        if remaining < count || at_end {
            let page = if forward {
                remaining.min(count - 1)
            } else {
                count - 1 - remaining.min(count - 1)
            };
            let prepared = probe
                .into_prepared_parley()
                .ok_or("This page cannot be prepared")?;
            return Ok(Some(Outcome {
                chapter,
                page,
                prepared,
            }));
        }
        remaining -= count;
    }
    Err("Too many chapters to count for this page jump".into())
}
