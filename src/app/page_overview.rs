//! Page overview thumbnails and the source position of the browsed page.
use primitive_app::Change;
use primitive_ui::{CAPTION, Canvas, GrayImage, Input, Rect};
use std::collections::{BTreeMap, BTreeSet};

use super::fixed_book_map::MapWindow;
use super::navigation_layout::NavigationLayout;
use super::{Action, LIST_X, LIST_Y, OVERVIEW_PAGES, Reader, Screen, bar_icons, fixed_page_label};
use crate::generated_contents::OutlineTarget;
use crate::library::{PageBrowserGrid, PageBrowserNumbers};
use crate::session::{PageThumbnail, Reading, page_thumbnail, screen_pages};
use crate::{Anchor, Probe, Result};

const PAGE_BROWSER_MARK_LIMIT: usize = 512;

mod grid_menu;
mod raw_verification;
mod ribbon_map;
pub(super) use raw_verification::Verification as RawVerification;
pub(super) use ribbon_map::{RibbonMap, RibbonMapTarget};

fn capacity(grid: Option<PageBrowserGrid>) -> usize {
    grid.map_or(OVERVIEW_PAGES, |grid| {
        let (cols, rows) = grid.dimensions();
        usize::from(cols) * usize::from(rows)
    })
}

/// A centered run of page slots around the thumbnail focus.
struct PageStripe {
    start: i64,
    slots: usize,
}

impl PageStripe {
    fn centered(focus: usize, batch: usize) -> Self {
        let mut slots = batch.saturating_mul(3).max(20);
        if slots % 2 != batch % 2 {
            slots += 1;
        }
        let focus = i64::try_from(focus).unwrap_or(i64::MAX);
        Self {
            start: focus.saturating_sub((slots / 2) as i64),
            slots,
        }
    }

    fn slot(&self, page: usize) -> Option<usize> {
        let slot = i64::try_from(page).ok()?.checked_sub(self.start)?;
        usize::try_from(slot).ok().filter(|slot| *slot < self.slots)
    }

    fn page(&self, slot: usize, count: usize) -> Option<usize> {
        if slot >= self.slots {
            return None;
        }
        let page = self.start.checked_add(i64::try_from(slot).ok()?)?;
        usize::try_from(page).ok().filter(|page| *page < count)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum MapOrigin {
    Fixed(MapWindow),
    Chapter(usize),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct OverviewFocus {
    pub(super) page: usize,
    slot: usize,
    pub(super) anchor: Option<Anchor>,
}

pub(super) struct PageOverview {
    pub(super) first: usize,
    pub(super) count: usize,
    pub(super) chapter: Option<usize>,
    pub(super) preview: Option<Box<Probe>>,
    pub(super) tiles: Vec<Option<PageThumbnail>>,
    pub(super) focus: OverviewFocus,
    bookmark_pages: Vec<Option<usize>>,
    back_pages: Vec<Option<usize>>,
    forward_pages: Vec<Option<usize>>,
    highlight_pages: Vec<Option<(usize, usize)>>,
    outline_spans: Vec<ReflowOutlineSpan>,
    book_outline_spans: Vec<ReflowOutlineSpan>,
    pub(super) map_origin: Option<MapOrigin>,
    pub(super) grid: Option<PageBrowserGrid>,
    pub(super) grid_menu: bool,
    pub(super) grid_help: bool,
}

impl PageOverview {
    #[cfg(test)]
    pub(super) fn book_outline_starts(&self) -> Vec<usize> {
        self.book_outline_spans
            .iter()
            .map(|span| span.start)
            .collect()
    }

    #[cfg(test)]
    pub(super) fn bookmark_pages(&self) -> &[Option<usize>] {
        &self.bookmark_pages
    }

    #[cfg(test)]
    pub(super) fn history_pages(&self) -> (&[Option<usize>], &[Option<usize>]) {
        (&self.back_pages, &self.forward_pages)
    }

    #[cfg(test)]
    pub(super) fn highlight_pages(&self) -> &[Option<(usize, usize)>] {
        &self.highlight_pages
    }
}

#[derive(Clone, Debug)]
struct ReflowOutlineSpan {
    start: usize,
    end: usize,
    depth: usize,
    title: String,
}

#[derive(Default)]
pub(super) struct OverviewPageTargets {
    pub(super) targets: Vec<screen_pages::CustomTarget>,
    pub(super) custom_count: usize,
    pub(super) bookmark_count: usize,
    pub(super) back_count: usize,
    pub(super) forward_count: usize,
    pub(super) highlight_count: usize,
}

impl OverviewPageTargets {
    fn push_position(&mut self, position: &crate::library::ReadingPosition) {
        let index = self.targets.len();
        self.targets.push(screen_pages::CustomTarget {
            index,
            chapter: position.chapter,
            anchor: position
                .anchor
                .as_ref()
                .filter(|anchor| anchor.raw_caret.is_none())
                .cloned(),
            starts_chapter: position.anchor.is_none() && position.page == 0,
        });
    }

    fn push_anchor(&mut self, probe: &Probe, anchor: Option<&Anchor>, before: bool) {
        let chapter = anchor
            .filter(|anchor| anchor.raw_caret.is_none() && anchor.book == probe.book)
            .and_then(|anchor| {
                let chapter = probe.chapter_index(&anchor.chapter)?;
                let unique = probe
                    .chapter_entries()
                    .iter()
                    .filter(|entry| entry.href == anchor.chapter)
                    .take(2)
                    .count()
                    == 1;
                unique.then_some(chapter)
            });
        let anchor = chapter.and_then(|_| anchor.cloned()).map(|mut anchor| {
            if before {
                // A saved selection is half-open. If its end lands exactly
                // on a page boundary, map it to the page containing the
                // preceding text rather than marking an empty continuation.
                anchor.side_bias = Some(crate::SideBias::Before);
            }
            anchor
        });
        let index = self.targets.len();
        self.targets.push(screen_pages::CustomTarget {
            index,
            chapter: chapter.unwrap_or(probe.chapter_count()),
            anchor,
            starts_chapter: false,
        });
    }
}

#[derive(Clone, Copy)]
pub(super) struct OverviewLayout {
    navigation: NavigationLayout,
    grid: Option<PageBrowserGrid>,
}

impl OverviewLayout {
    pub(super) fn new(navigation: NavigationLayout) -> Self {
        Self::with_grid(navigation, None)
    }

    pub(super) fn with_grid(navigation: NavigationLayout, grid: Option<PageBrowserGrid>) -> Self {
        Self { navigation, grid }
    }

    pub(super) fn dimensions(self) -> (usize, usize) {
        if let Some(grid) = self.grid {
            let (columns, rows) = grid.dimensions();
            (usize::from(columns), usize::from(rows))
        } else if self.navigation.width > self.navigation.height {
            (4, 1)
        } else {
            (2, 2)
        }
    }

    fn columns(self) -> usize {
        self.dimensions().0
    }

    pub(super) fn tile(self, index: usize) -> Rect {
        let (columns, rows) = self.dimensions();
        let (columns, rows) = (columns as i32, rows as i32);
        let (top, height, gap_x, gap_y) = if self.grid.is_some() {
            let top = 214;
            let gap_x = if columns >= 4 { 12 } else { 26 };
            let gap_y = if rows >= 4 { 6 } else { 24 };
            let height = (self.track().y - 36 - top - (rows - 1) * gap_y) / rows;
            (top, height, gap_x, gap_y)
        } else {
            let top = LIST_Y + 90;
            let height = if columns == 2 {
                420
            } else {
                self.track().y - top - 88
            };
            (top, height, 26, 32)
        };
        let width = (self.navigation.width - 2 * LIST_X - (columns - 1) * gap_x) / columns;
        self.navigation.horizontal(Rect::new(
            LIST_X + index as i32 % columns * (width + gap_x),
            top + index as i32 / columns * (height + gap_y),
            width,
            height,
        ))
    }

    fn label(self, tile: Rect) -> Rect {
        if self.grid.is_none() {
            return Rect::new(tile.x + 18, tile.bottom() - 84, tile.w - 36, 68);
        }
        let label_height = (tile.h / 5).clamp(28, 60);
        Rect::new(
            tile.x + 8,
            tile.bottom() - label_height - 7,
            tile.w - 16,
            label_height,
        )
    }

    fn draw_label(self, canvas: &mut Canvas, tile: Rect, label: &str, selected: bool) {
        let rect = self.label(tile);
        if self.grid.is_none() {
            canvas.button(rect, label, selected, false);
        } else {
            canvas.button(rect, "", selected, false);
            let ink = if canvas.is_control_pressed(rect) != selected {
                255
            } else {
                0
            };
            let font = (rect.h - 9).clamp(18, 32);
            canvas.text(
                Rect::new(rect.x + 4, rect.y + 2, rect.w - 8, rect.h - 3),
                label,
                primitive_ui::TextStyle::new(font).bold().center().ink(ink),
            );
        }
    }

    pub(super) fn track(self) -> Rect {
        Rect::new(
            80,
            self.navigation.height - 370,
            self.navigation.width - 160,
            75,
        )
    }

    fn blank_tap_direction(self, x: f32, y: f32) -> Option<isize> {
        let width = self.navigation.width as f32;
        let body = self.tile(0).y as f32..self.track_caption().y as f32;
        if !(0.0..width).contains(&x) || !body.contains(&y) {
            return None;
        }
        Some(if (x < width / 2.0) ^ self.navigation.mirrored {
            -1
        } else {
            1
        })
    }

    fn track_caption(self) -> Rect {
        let track = self.track();
        let offset = if self.grid.is_some() { 30 } else { 55 };
        Rect::new(track.x, track.y - offset, track.w, 45)
    }

    fn thumbnail(self, tile: Rect, image: GrayImage) -> (Rect, GrayImage) {
        let (top, body_height, max_width) = if self.grid.is_some() {
            let top = 9;
            (
                top,
                (self.label(tile).y - tile.y - top - 7).max(1),
                (tile.w - 16).max(1),
            )
        } else {
            (21, tile.h - 112, tile.w - 36)
        };
        let image = if image.width > max_width as u32 || image.height > body_height as u32 {
            let source = image::GrayImage::from_raw(image.width, image.height, image.pixels)
                .expect("thumbnail dimensions belong to its raster");
            let scale = (max_width as f64 / f64::from(image.width))
                .min(body_height as f64 / f64::from(image.height));
            let width = (f64::from(image.width) * scale).floor().max(1.0) as u32;
            let height = (f64::from(image.height) * scale).floor().max(1.0) as u32;
            let resized = image::imageops::thumbnail(&source, width, height);
            let (width, height) = resized.dimensions();
            GrayImage {
                width,
                height,
                pixels: resized.into_raw(),
            }
        } else {
            image
        };
        let width = image.width as i32;
        let height = image.height as i32;
        let extra_y = if self.grid.is_some() || self.columns() == 4 {
            (body_height - height) / 2
        } else {
            0
        };
        (
            Rect::new(
                tile.x + (tile.w - width) / 2,
                tile.y + top + extra_y,
                width,
                height,
            ),
            image,
        )
    }
}

impl Reader {
    fn overview_page_numbers(&self) -> PageBrowserNumbers {
        self.open_book_id()
            .and_then(|id| self.library.state(id))
            .and_then(|book| book.page_browser_numbers)
            .unwrap_or(self.controls.page_browser_numbers_default)
    }

    fn overview_custom_contents(&self) -> bool {
        self.open_book_id()
            .and_then(|id| self.library.state(id))
            .is_some_and(|state| state.custom_contents.enabled)
    }

    fn overview_max_chapter_levels(&self) -> u8 {
        let Some(id) = self.open_book_id() else {
            return 0;
        };
        if let Some(custom) = self
            .library
            .state(id)
            .map(|state| &state.custom_contents)
            .filter(|custom| custom.enabled)
        {
            return u8::from(!custom.entries().is_empty());
        }
        let depth = if let Some(fixed) = &self.fixed {
            fixed
                .reading
                .chapter_outline()
                .filter(|entry| entry.page.is_some_and(|page| page < fixed.reading.count()))
                .map(|entry| entry.depth)
                .max()
        } else if let Some(active) = &self.active {
            let probe = &active.reading.probe;
            if let Some(entries) = self.alternative_entries(id) {
                entries
                    .iter()
                    .filter(|entry| match &entry.target {
                        OutlineTarget::Chapter(chapter) | OutlineTarget::Heading(chapter, _) => {
                            *chapter < probe.chapter_count()
                        }
                    })
                    .map(|entry| entry.depth)
                    .max()
            } else if probe.toc.is_empty() {
                (!probe.chapter_entries().is_empty()).then_some(0)
            } else {
                probe
                    .toc
                    .iter()
                    .filter(|entry| {
                        entry
                            .destination
                            .as_ref()
                            .and_then(|destination| probe.chapter_index(&destination.path))
                            .is_some()
                    })
                    .map(|entry| entry.depth)
                    .max()
            }
        } else {
            None
        };
        depth.map_or(0, |depth| (depth.min(15) + 1) as u8)
    }

    fn overview_chapter_levels(&self) -> u8 {
        let maximum = self.overview_max_chapter_levels();
        self.open_book_id()
            .and_then(|id| self.library.state(id))
            .and_then(|state| {
                if state.custom_contents.enabled {
                    state.page_browser_custom_toc_depth
                } else {
                    state.page_browser_toc_depth
                }
            })
            .unwrap_or(maximum)
            .min(maximum)
    }

    fn book_map_previews_on_tap(&self) -> bool {
        self.controls.book_map_tap_to_page_browser.unwrap_or(true)
    }

    pub(super) fn overview_page_targets(&self) -> OverviewPageTargets {
        let Some(active) = &self.active else {
            return OverviewPageTargets::default();
        };
        let Some(state) = self.library.state(active.id) else {
            return OverviewPageTargets::default();
        };
        let mut result = OverviewPageTargets::default();
        if state.custom_contents.enabled {
            result.targets =
                super::contents_position::custom_page_targets(state.custom_contents.entries());
            result.custom_count = result.targets.len();
        }
        let bookmark_start = result.targets.len();
        for bookmark in state.bookmarks.iter().take(PAGE_BROWSER_MARK_LIMIT) {
            result.push_position(&bookmark.position);
        }
        result.bookmark_count = result.targets.len() - bookmark_start;
        let back_start = result.targets.len();
        for location in self.back.iter().rev().take(3) {
            result.push_position(&location.position);
        }
        result.back_count = result.targets.len() - back_start;
        let forward_start = result.targets.len();
        for location in self.forward.iter().rev().take(3) {
            result.push_position(&location.position);
        }
        result.forward_count = result.targets.len() - forward_start;
        let highlight_limit = PAGE_BROWSER_MARK_LIMIT
            .saturating_sub(state.bookmarks.len().min(PAGE_BROWSER_MARK_LIMIT));
        let probe = &active.reading.probe;
        for highlight in state.highlights.iter().take(highlight_limit) {
            result.push_anchor(probe, Some(&highlight.selection.start), false);
            result.push_anchor(probe, Some(&highlight.selection.end), true);
            result.highlight_count += 1;
        }
        result
    }

    pub(super) fn book_map_tap_option(&self) -> Rect {
        let layout = self.navigation_layout();
        layout.horizontal(Rect::new(layout.width - 744, 40, 444, 100))
    }

    pub(super) fn draw_book_map_tap_option(&self, canvas: &mut Canvas) {
        canvas.button(
            self.book_map_tap_option(),
            self.controls
                .interface_language
                .overview_text("Page browser on tap"),
            self.book_map_previews_on_tap(),
            false,
        );
    }

    pub(super) fn book_map_tap_option_input(&mut self, input: Input) -> Option<Change> {
        if !matches!(input, Input::Tap(x, y) if self.book_map_tap_option().contains(x, y)) {
            return None;
        }
        let mut controls = self.controls;
        controls.book_map_tap_to_page_browser = Some(!self.book_map_previews_on_tap());
        Some(match self.activate(Action::SetReadingControls(controls)) {
            Ok(()) => Change::Partial,
            Err(error) => self.report(error),
        })
    }

    pub(super) fn select_book_map_page(&mut self, map: MapOrigin, page: usize) -> Result<()> {
        if !self.book_map_previews_on_tap() {
            return match map {
                MapOrigin::Fixed(_) => self.follow_fixed(page, None),
                MapOrigin::Chapter(chapter) => self.follow_reflow_page(chapter, page),
            };
        }
        let chapter = match map {
            MapOrigin::Chapter(chapter) => Some(chapter),
            MapOrigin::Fixed(_) => None,
        };
        self.open_overview_at(chapter, page, None, Some(map))
    }

    pub(super) fn open_overview(&mut self) -> Result<()> {
        let (chapter, page, anchor) = if let Some(fixed) = &self.fixed {
            (None, fixed.reading.page(), None)
        } else {
            let reading = &self.active.as_ref().ok_or("Open a book first")?.reading;
            (Some(reading.chapter), reading.page, reading.anchor())
        };
        self.open_overview_at(chapter, page, anchor, None)
    }

    fn open_overview_at(
        &mut self,
        chapter: Option<usize>,
        page: usize,
        anchor: Option<Anchor>,
        map_origin: Option<MapOrigin>,
    ) -> Result<()> {
        let grid = self
            .open_book_id()
            .and_then(|id| self.library.page_browser_grid(id));
        let batch = capacity(grid);
        let targets = self.overview_page_targets();
        if let Some(reading) = self.active.as_mut().map(|book| &mut book.reading)
            && reading.has_exact_local_screen_pages()
        {
            reading.request_screen_page_navigation();
            reading.request_custom_page_locations(targets.targets);
        }
        let mut overview =
            self.prepare_overview(chapter, Some(page / batch * batch), None, grid)?;
        let anchor = if anchor.is_none() && overview.focus.page == page {
            overview.focus.anchor.take()
        } else {
            anchor
        };
        overview.focus = OverviewFocus {
            page,
            slot: page - overview.first,
            anchor,
        };
        overview.map_origin = map_origin;
        overview.grid_menu = false;
        overview.grid_help = false;
        self.overview = Some(overview);
        self.show(Screen::PageOverview);
        self.start_overview_raw_verification();
        Ok(())
    }

    pub(super) fn update_overview(
        &mut self,
        chapter: Option<usize>,
        first: Option<usize>,
    ) -> Result<()> {
        let previous_chapter = self.overview.as_ref().and_then(|view| view.chapter);
        let grid = self
            .overview
            .as_ref()
            .and_then(|view| view.grid)
            .or_else(|| {
                self.open_book_id()
                    .and_then(|id| self.library.page_browser_grid(id))
            });
        self.overview = Some(self.prepare_overview(chapter, first, None, grid)?);
        if previous_chapter != chapter {
            self.start_overview_raw_verification();
        }
        self.notice = None;
        Ok(())
    }

    pub(super) fn refresh_overview(&mut self) -> Result<()> {
        let Some(current) = self.overview.as_ref() else {
            return Ok(());
        };
        let chapter = current.chapter;
        let first = current.first;
        let focus = current.focus.clone();
        let grid = current.grid;
        let mut overview = self.prepare_overview(chapter, Some(first), None, grid)?;
        let page = focus.page.min(overview.count.saturating_sub(1));
        overview.focus = OverviewFocus {
            page,
            slot: page.saturating_sub(overview.first),
            anchor: focus.anchor,
        };
        self.overview = Some(overview);
        self.notice = None;
        Ok(())
    }

    pub(super) fn start_overview_raw_verification(&mut self) {
        self.overview_raw = None;
        let (Some(active), Some(overview)) = (&self.active, &self.overview) else {
            return;
        };
        let Some(chapter) = overview.chapter else {
            return;
        };
        let Some(state) = self.library.state(active.id) else {
            return;
        };
        let history = self.back.len().min(3) + self.forward.len().min(3);
        let total = state
            .bookmarks
            .len()
            .saturating_add(history)
            .saturating_add(state.highlights.len());
        let reading = &active.reading;
        let recipe = reading.config.search_layout_recipe(&reading.probe);
        self.overview_raw = Some(RawVerification::new(
            recipe,
            total,
            chapter,
            reading.probe.chapter_count(),
        ));
        self.queue_overview_raw_slice(std::time::Instant::now());
    }

    fn queue_overview_raw_slice(&mut self, now: std::time::Instant) {
        use raw_verification::{SCAN, SLICE, Task};
        let Some(mut job) = self.overview_raw.take() else {
            return;
        };
        if job.pending() {
            self.overview_raw = Some(job);
            return;
        }
        let (Some(active), Some(overview)) = (&self.active, &self.overview) else {
            return;
        };
        let Some(state) = self.library.state(active.id) else {
            return;
        };
        let probe = &active.reading.probe;
        let history = self.back.len().min(3) + self.forward.len().min(3);
        let total = state
            .bookmarks
            .len()
            .saturating_add(history)
            .saturating_add(state.highlights.len());
        if total != job.total || overview.chapter != Some(job.chapter) {
            return;
        }
        let mut tasks = Vec::new();
        let mut scanned = 0;
        while job.cursor < job.total && tasks.len() < SLICE && scanned < SCAN {
            let index = job.cursor;
            job.cursor += 1;
            scanned += 1;
            if index < state.bookmarks.len() {
                let Some(anchor) = state.bookmarks[index].position.anchor.as_ref() else {
                    continue;
                };
                if anchor.raw_caret.is_none() || anchor.book != probe.book {
                    continue;
                }
                if let Some(chapter) = probe.chapter_index(&anchor.chapter) {
                    tasks.push(Task::Mark {
                        first: chapter,
                        last: chapter,
                        start: anchor.clone(),
                        end: None,
                        flags: 4,
                        quote: None,
                    });
                }
            } else if index < state.bookmarks.len() + history {
                let ordinal = index - state.bookmarks.len();
                let location = self
                    .back
                    .iter()
                    .rev()
                    .take(3)
                    .chain(self.forward.iter().rev().take(3))
                    .nth(ordinal);
                let Some(anchor) = location.and_then(|location| location.position.anchor.as_ref())
                else {
                    continue;
                };
                if anchor.raw_caret.is_none() || anchor.book != probe.book {
                    continue;
                }
                if let Some(chapter) = probe.chapter_index(&anchor.chapter) {
                    tasks.push(Task::History {
                        chapter,
                        anchor: anchor.clone(),
                    });
                }
            } else {
                let selection =
                    &state.highlights[index - state.bookmarks.len() - history].selection;
                let raw = selection.start.raw_caret.is_some() || selection.end.raw_caret.is_some();
                if !raw
                    && (selection.start.chapter == selection.end.chapter
                        || !job.recipe.has_verified_sources())
                {
                    continue;
                }
                if selection.start.book != probe.book || selection.end.book != probe.book {
                    continue;
                }
                let (Some(first), Some(last)) = (
                    probe.chapter_index(&selection.start.chapter),
                    probe.chapter_index(&selection.end.chapter),
                ) else {
                    continue;
                };
                if first > last {
                    continue;
                }
                let quote = if first != last
                    && last.saturating_sub(first) < 8
                    && (first..=last).contains(&job.chapter)
                    && job.thumb_candidates < 4
                    && selection.text.len() <= crate::library::MAX_HIGHLIGHT_TEXT_BYTES
                {
                    job.thumb_candidates += 1;
                    Some(selection.clone())
                } else {
                    None
                };
                // Verify only cross-chapter selections that intersect the
                // focused chapter, keeping the quote worker bounded while
                // covering visible ribbon spans and thumbnail fragments.
                if !raw && quote.is_none() {
                    continue;
                }
                let noted = state.highlights[index - state.bookmarks.len() - history]
                    .note
                    .as_ref()
                    .is_some_and(|note| !note.trim().is_empty());
                tasks.push(Task::Mark {
                    first,
                    last,
                    start: selection.start.clone(),
                    end: Some(selection.end.clone()),
                    flags: if raw {
                        1 | if noted { 2 } else { 0 }
                    } else {
                        0
                    },
                    quote,
                });
            }
        }
        job.submit(tasks, now);
        self.overview_raw = Some(job);
    }

    pub(super) fn poll_overview_raw_verification(
        &mut self,
        now: std::time::Instant,
    ) -> Option<Change> {
        if !self
            .overview_raw
            .as_ref()
            .and_then(RawVerification::deadline)
            .is_some_and(|at| at <= now)
        {
            return None;
        }
        let Some(active) = &self.active else {
            self.overview_raw = None;
            return None;
        };
        let recipe = active
            .reading
            .config
            .search_layout_recipe(&active.reading.probe);
        let changed = self
            .overview_raw
            .as_mut()
            .is_some_and(|job| job.poll(now, &recipe));
        self.queue_overview_raw_slice(now);
        changed.then_some(Change::Partial)
    }

    fn prepare_overview(
        &mut self,
        chapter: Option<usize>,
        first: Option<usize>,
        saved_focus: Option<&OverviewFocus>,
        grid: Option<PageBrowserGrid>,
    ) -> Result<PageOverview> {
        let (map_origin, grid_menu, grid_help) =
            self.overview.as_ref().map_or((None, false, false), |view| {
                (view.map_origin, view.grid_menu, view.grid_help)
            });
        let batch = capacity(grid);
        let mut overview = if let Some(fixed) = &self.fixed {
            let count = fixed.reading.count();
            let selected = saved_focus.map(|focus| focus.page.min(count.saturating_sub(1)));
            let (first, focus) = overview_window(count, batch, first, saved_focus, selected);
            let tiles = (first..(first + batch).min(count))
                .map(|page| fixed.reading.thumbnail(page).ok().map(PageThumbnail::plain))
                .collect();
            PageOverview {
                chapter: None,
                first,
                count,
                tiles,
                preview: None,
                focus,
                bookmark_pages: Vec::new(),
                back_pages: Vec::new(),
                forward_pages: Vec::new(),
                highlight_pages: Vec::new(),
                outline_spans: Vec::new(),
                book_outline_spans: Vec::new(),
                map_origin: None,
                grid,
                grid_menu: false,
                grid_help: false,
            }
        } else {
            let reading = &mut self.active.as_mut().ok_or("Open a book first")?.reading;
            let chapter = chapter.ok_or("Choose a chapter first")?;
            let mut preview = if chapter == reading.chapter {
                None
            } else if saved_focus.is_none()
                && let Some(previous) = self.overview.as_mut()
                && previous.chapter == Some(chapter)
            {
                previous.preview.take()
            } else {
                Some(Box::new(reading.config.chapter(chapter)?))
            };
            let probe = preview.as_deref_mut().unwrap_or(&mut reading.probe);
            let mut overview = reflow_overview(probe, chapter, first, saved_focus, grid)?;
            overview.preview = preview;
            overview
        };
        if let (Some(active), Some(chapter)) = (self.active.as_ref(), overview.chapter) {
            let probe = overview.preview.as_deref().unwrap_or(&active.reading.probe);
            overview.outline_spans =
                self.reflow_overview_spans(active.id, chapter, overview.count, probe);
            if let Some((_, total)) = active.reading.whole_book_screen_position(chapter, 0) {
                let targets = self.overview_page_targets();
                let locations = active.reading.whole_book_custom_locations(&targets.targets);
                overview.bookmark_pages = locations
                    .as_ref()
                    .map(|locations| {
                        locations
                            .pages
                            .iter()
                            .skip(targets.custom_count)
                            .take(targets.bookmark_count)
                            .copied()
                            .collect()
                    })
                    .unwrap_or_default();
                let history_start = targets.custom_count + targets.bookmark_count;
                overview.back_pages = locations
                    .as_ref()
                    .map(|locations| {
                        locations
                            .pages
                            .iter()
                            .skip(history_start)
                            .take(targets.back_count)
                            .copied()
                            .collect()
                    })
                    .unwrap_or_default();
                let forward_start = history_start + targets.back_count;
                overview.forward_pages = locations
                    .as_ref()
                    .map(|locations| {
                        locations
                            .pages
                            .iter()
                            .skip(forward_start)
                            .take(targets.forward_count)
                            .copied()
                            .collect()
                    })
                    .unwrap_or_default();
                let highlight_start = forward_start + targets.forward_count;
                overview.highlight_pages = locations
                    .as_ref()
                    .map(|locations| {
                        (0..targets.highlight_count)
                            .map(|index| {
                                let start = locations
                                    .pages
                                    .get(highlight_start + index * 2)
                                    .copied()
                                    .flatten()?;
                                let end = locations
                                    .pages
                                    .get(highlight_start + index * 2 + 1)
                                    .copied()
                                    .flatten()?;
                                (start <= end).then_some((start, end))
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                overview.book_outline_spans = self.reflow_book_overview_spans(
                    active.id,
                    &active.reading,
                    chapter,
                    total,
                    probe,
                    locations.as_ref(),
                );
            }
        }
        overview.map_origin = map_origin;
        overview.grid_menu = grid_menu;
        overview.grid_help = grid_help;
        Ok(overview)
    }

    fn reflow_overview_spans(
        &self,
        book: u64,
        chapter: usize,
        count: usize,
        probe: &Probe,
    ) -> Vec<ReflowOutlineSpan> {
        let levels = self.overview_chapter_levels() as usize;
        if levels == 0 {
            return Vec::new();
        }
        let Some(state) = self.library.state(book) else {
            return Vec::new();
        };
        let mut headings = Vec::<(usize, usize, String)>::new();
        let mut push = |page: usize, depth: usize, title: &str| {
            if page < count && depth < levels {
                headings.push((page, depth, title.to_owned()));
            }
        };
        if state.custom_contents.enabled {
            for entry in state.custom_contents.entries() {
                if entry.position.chapter != chapter {
                    continue;
                }
                let page = match &entry.position.anchor {
                    Some(anchor) if anchor.raw_caret.is_none() => probe.page_for(anchor),
                    Some(_) => None,
                    None => Some(entry.position.page),
                };
                if let Some(page) = page {
                    push(page, 0, &entry.label);
                }
            }
        } else if let Some(entries) = self.alternative_entries(book) {
            for entry in entries {
                let page = match &entry.target {
                    OutlineTarget::Chapter(target) if *target == chapter => Some(0),
                    OutlineTarget::Heading(target, anchor) if *target == chapter => {
                        probe.page_for(anchor)
                    }
                    _ => None,
                };
                if let Some(page) = page {
                    push(page, entry.depth, &entry.title);
                }
            }
        } else {
            for entry in &probe.toc {
                if entry.depth >= levels {
                    continue;
                }
                let Some(destination) = entry.destination.as_ref() else {
                    continue;
                };
                if probe.chapter_index(&destination.path) != Some(chapter) {
                    continue;
                }
                let page = match destination.fragment.as_deref() {
                    Some(fragment) => probe
                        .fragment(fragment)
                        .and_then(|anchor| probe.page_for(&anchor)),
                    None => Some(0),
                };
                if let Some(page) = page {
                    push(page, entry.depth, &entry.label);
                }
            }
        }
        headings.sort_by_key(|(page, depth, _)| (*page, *depth));
        let mut spans = Vec::with_capacity(headings.len());
        for (index, (start, depth, title)) in headings.iter().enumerate() {
            let end = headings[index + 1..]
                .iter()
                .find(|(_, next_depth, _)| next_depth <= depth)
                .map_or(count, |(page, _, _)| *page);
            spans.push(ReflowOutlineSpan {
                start: *start,
                end,
                depth: *depth,
                title: title.clone(),
            });
        }
        spans
    }

    fn reflow_book_overview_spans(
        &self,
        book: u64,
        reading: &Reading,
        current_chapter: usize,
        total: usize,
        probe: &Probe,
        locations: Option<&screen_pages::CustomLocations>,
    ) -> Vec<ReflowOutlineSpan> {
        let levels = self.overview_chapter_levels() as usize;
        if levels == 0 {
            return Vec::new();
        }
        let Some(state) = self.library.state(book) else {
            return Vec::new();
        };
        let mut headings = Vec::<(usize, usize, String)>::new();
        let mut push = |page: usize, depth: usize, title: &str| {
            if page < total && depth < levels && !title.trim().is_empty() {
                headings.push((page, depth, title.to_owned()));
            }
        };
        if state.custom_contents.enabled {
            for (chapter, entry) in reading.probe.chapter_entries().iter().enumerate() {
                if let Some((ordinal, _)) = reading.whole_book_screen_position(chapter, 0) {
                    push(ordinal.saturating_sub(1), 0, &entry.label);
                }
            }
            let entries = state.custom_contents.entries();
            for (index, entry) in entries.iter().enumerate() {
                let position = &entry.position;
                let absolute = locations
                    .and_then(|locations| locations.pages.get(index).copied().flatten())
                    .or_else(|| {
                        let local_page = if position.chapter == current_chapter {
                            position
                                .anchor
                                .as_ref()
                                .and_then(|anchor| probe.page_for(anchor))
                                .unwrap_or(position.page)
                        } else {
                            position.page
                        };
                        reading
                            .whole_book_screen_position(position.chapter, local_page)
                            .map(|(ordinal, _)| ordinal.saturating_sub(1))
                    });
                if let Some(page) = absolute {
                    push(page, 0, &entry.label);
                }
            }
        } else if let Some(entries) = self.alternative_entries(book) {
            if let Some(locations) =
                reading.whole_book_alternative_locations(entries, |depth| depth < levels)
            {
                for location in locations.starts {
                    if let Some(entry) = entries.get(location.index) {
                        push(
                            location.absolute.saturating_sub(1),
                            entry.depth,
                            &entry.title,
                        );
                    }
                }
            }
        } else if let Some(locations) =
            reading.whole_book_publisher_locations(|depth| depth < levels)
        {
            for location in locations.starts {
                if let Some(entry) = probe.toc.get(location.index) {
                    push(
                        location.absolute.saturating_sub(1),
                        entry.depth,
                        &entry.label,
                    );
                }
            }
        }
        headings.sort_by_key(|(page, depth, _)| (*page, *depth));
        headings.dedup_by(|left, right| left.0 == right.0 && left.1 == right.1);
        let mut spans = Vec::with_capacity(headings.len());
        for (index, (start, depth, title)) in headings.iter().enumerate() {
            let end = headings[index + 1..]
                .iter()
                .find(|(page, next_depth, _)| page > start && next_depth <= depth)
                .map_or(total, |(page, _, _)| *page);
            spans.push(ReflowOutlineSpan {
                start: *start,
                end,
                depth: *depth,
                title: title.clone(),
            });
        }
        spans
    }

    pub(super) fn resize_overview(
        &mut self,
        chapter: usize,
        focus: &OverviewFocus,
    ) -> Result<PageOverview> {
        let grid = self.overview.as_ref().and_then(|view| view.grid);
        self.prepare_overview(Some(chapter), None, Some(focus), grid)
    }

    pub(super) fn draw_overview(&self, canvas: &mut Canvas) {
        let Some(overview) = &self.overview else {
            return;
        };
        let navigation = self.navigation_layout();
        let language = self.controls.interface_language;
        let layout = OverviewLayout::with_grid(navigation, overview.grid);
        let count = overview.count;
        self.draw_overview_grid_button(canvas, navigation);
        let (section, current, chapters) = if let Some(active) = &self.active {
            let reading = &active.reading;
            let probe = overview.preview.as_deref().unwrap_or(&reading.probe);
            (
                format!(
                    "{}  ·  {}",
                    probe.chapter_label,
                    language.overview_page_count(count)
                ),
                (overview.chapter == Some(reading.chapter)).then_some(reading.page),
                probe.chapter_count(),
            )
        } else {
            (
                language.overview_page_count(count),
                self.fixed.as_ref().map(|fixed| fixed.reading.page()),
                0,
            )
        };
        canvas.text(
            navigation.body(LIST_Y, 66),
            self.notice.as_deref().unwrap_or(&section),
            CAPTION,
        );
        // Limit the map to the visible thumbnail window. The value records
        // whether any visible highlight on that page also has a note.
        let mut highlighted_pages = BTreeMap::<usize, bool>::new();
        let end = overview.first.saturating_add(overview.tiles.len());
        if let Some(book) = self.fixed.as_ref().filter(|_| overview.chapter.is_none())
            && let Some(state) = self.library.state(book.id)
        {
            for mark in &state.pdf_highlights {
                let noted = mark
                    .note
                    .as_ref()
                    .is_some_and(|note| !note.trim().is_empty());
                for page in
                    std::iter::once(mark.page).chain(mark.continuation.iter().map(|part| part.page))
                {
                    if (overview.first..end).contains(&page) && page < overview.count {
                        *highlighted_pages.entry(page).or_default() |= noted;
                    }
                }
            }
        } else if let Some(active) = self.active.as_ref()
            && let Some(chapter) = overview.chapter
            && let Some(state) = self.library.state(active.id)
        {
            let probe = overview
                .preview
                .as_deref()
                .or_else(|| (active.reading.chapter == chapter).then_some(&active.reading.probe));
            if let Some(probe) = probe {
                // Page-local geometry handles half-open end anchors accurately.
                // Limit expensive checks on books with many large selections.
                let mut checks = 0usize;
                let mut cross_chapter_checks = 0usize;
                'marks: for mark in &state.highlights {
                    let saved = &mark.selection;
                    let local;
                    let selection = if saved.start.chapter != saved.end.chapter {
                        let (Some(first), Some(last)) = (
                            probe.chapter_index(&saved.start.chapter),
                            probe.chapter_index(&saved.end.chapter),
                        ) else {
                            continue;
                        };
                        if first > last
                            || !(first..=last).contains(&chapter)
                            || last.saturating_sub(first) >= 8
                            || cross_chapter_checks == 4
                        {
                            continue;
                        }
                        cross_chapter_checks += 1;
                        if !self
                            .overview_raw
                            .as_ref()
                            .is_some_and(|job| job.selection(saved))
                        {
                            continue;
                        }
                        let fragment =
                            if saved.start.raw_caret.is_some() || saved.end.raw_caret.is_some() {
                                probe.raw_chapter_fragment(&saved.start, &saved.end)
                            } else {
                                probe.select_source_range(&saved.start, &saved.end)
                            };
                        let Some(fragment) = fragment else {
                            continue;
                        };
                        local = fragment;
                        &local
                    } else {
                        saved
                    };
                    if probe.chapter_index(&selection.start.chapter) != Some(chapter)
                        || probe.chapter_index(&selection.end.chapter) != Some(chapter)
                    {
                        continue;
                    }
                    let (Some(first), Some(last)) = (
                        probe.page_for(&selection.start),
                        probe.page_for(&selection.end),
                    ) else {
                        continue;
                    };
                    if first > last {
                        continue;
                    }
                    let noted = mark
                        .note
                        .as_ref()
                        .is_some_and(|note| !note.trim().is_empty());
                    for page in first.max(overview.first)..last.saturating_add(1).min(end) {
                        if highlighted_pages
                            .get(&page)
                            .is_some_and(|&existing| existing || !noted)
                        {
                            continue;
                        }
                        if checks == 512 {
                            break 'marks;
                        }
                        checks += 1;
                        if !probe.selection_rects(page, selection).is_empty() {
                            *highlighted_pages.entry(page).or_default() |= noted;
                        }
                    }
                }
            }
        }
        for (index, tile) in overview.tiles.iter().enumerate() {
            let rect = layout.tile(index);
            canvas.card(rect);
            let page = overview.first + index;
            let hidden = self
                .active
                .as_ref()
                .and_then(|active| {
                    overview.chapter.and_then(|chapter| {
                        active
                            .reading
                            .custom_flow_for_page(chapter, page, overview.count)
                    })
                })
                .or_else(|| {
                    self.fixed.as_ref().and_then(|fixed| {
                        fixed
                            .reading
                            .hidden_flow_map()
                            .map(|flows| flows.flow(page))
                    })
                })
                .is_some_and(|flow| flow != 0);
            let map_focus = (overview.map_origin.is_some() || overview.grid.is_some())
                && overview.focus.page == page;
            if hidden || map_focus {
                canvas.panel(
                    rect,
                    22,
                    if map_focus { 6 } else { 2 },
                    if hidden { 235 } else { 255 },
                );
            }
            let show_number = match self.overview_page_numbers() {
                PageBrowserNumbers::None => false,
                PageBrowserNumbers::RowStart => index % layout.columns() == 0,
                PageBrowserNumbers::All => true,
            };
            let mut label = if show_number {
                self.fixed.as_ref().map_or_else(
                    || format!("{} {}", language.overview_text("Page"), page + 1),
                    |fixed| fixed_page_label(&fixed.reading, page),
                )
            } else {
                String::new()
            };
            if hidden {
                if !label.is_empty() {
                    label.push_str(" · ");
                }
                label.push_str(language.overview_text("Hidden"));
            }
            if let Some(tile) = tile {
                let (frame, thumbnail) =
                    layout.thumbnail(rect, tile.for_theme(self.uses_night_pages()));
                canvas.fill(frame.inset(-1), 170);
                canvas.picture(frame, thumbnail);
            } else {
                let y = rect.y + (rect.h / 3).min(120);
                canvas.text(
                    Rect::new(
                        rect.x + 10,
                        y,
                        rect.w - 20,
                        (layout.label(rect).y - y).clamp(1, 64),
                    ),
                    language.overview_text("Preview unavailable"),
                    CAPTION.center(),
                );
            }
            self.draw_overview_bookmark(canvas, rect, overview.chapter, page);
            if let Some(&noted) = highlighted_pages.get(&page) {
                let size = (rect.h / 3).min(rect.w / 3).min(54);
                let inset = (rect.h / 12).min(18);
                let badge = Rect::new(rect.x + inset, rect.y + inset, size, size);
                canvas.panel(badge, 5, 1, 255);
                canvas.icon(
                    badge.inset(4),
                    if noted {
                        bar_icons::EDIT
                    } else {
                        bar_icons::HIGHLIGHT
                    },
                    0,
                );
            }
            layout.draw_label(canvas, rect, &label, Some(page) == current);
        }
        if overview.first != 0 {
            canvas.button(
                navigation.previous(),
                language.overview_text("Previous"),
                false,
                false,
            );
        } else if overview.chapter.is_some_and(|chapter| chapter > 0) {
            canvas.button(
                navigation.previous(),
                language.overview_text("Prev chapter"),
                false,
                false,
            );
        }
        if overview.first + capacity(overview.grid) < count {
            canvas.button(
                navigation.next(),
                language.overview_text("Next"),
                false,
                false,
            );
        } else if overview
            .chapter
            .is_some_and(|chapter| chapter + 1 < chapters)
        {
            canvas.button(
                navigation.next(),
                language.overview_text("Next chapter"),
                false,
                false,
            );
        }
        self.draw_overview_track(canvas, overview, chapters, layout);
        if overview.grid_menu {
            self.draw_overview_grid_menu(canvas, layout);
        }
    }

    fn draw_overview_track(
        &self,
        canvas: &mut Canvas,
        overview: &PageOverview,
        chapters: usize,
        layout: OverviewLayout,
    ) {
        let language = self.controls.interface_language;
        let track = layout.track();
        let page = if overview.map_origin.is_some() {
            overview.focus.page
        } else {
            overview.first
        };
        let book_position = overview.chapter.and_then(|chapter| {
            self.active
                .as_ref()?
                .reading
                .whole_book_screen_position(chapter, overview.focus.page)
        });
        let selected_page = book_position.map_or(overview.focus.page, |(ordinal, _)| {
            ordinal.saturating_sub(1)
        });
        let stripe = overview
            .chapter
            .map(|_| PageStripe::centered(selected_page, capacity(overview.grid)));
        let stripe_count = book_position.map_or(overview.count, |(_, total)| total);
        let outline_spans = if book_position.is_some() {
            &overview.book_outline_spans
        } else {
            &overview.outline_spans
        };
        let (selected, count, title) = if let Some(stripe) = &stripe {
            (
                stripe.slot(selected_page).unwrap_or(0),
                stripe.slots,
                "Skim pages",
            )
        } else {
            overview
                .chapter
                .map_or((page, overview.count, "Skim pages"), |chapter| {
                    (chapter, chapters, "Skim chapters")
                })
        };
        let chapter_probe = self.active.as_ref().and_then(|active| {
            let chapter = overview.chapter?;
            if active.reading.chapter == chapter {
                Some(&active.reading.probe)
            } else {
                overview.preview.as_deref()
            }
        });
        let mut section_title = None;
        let mut closest_section_page = None;
        let mut ticks = BTreeSet::new();
        let mut outline_ticks = BTreeSet::new();
        let levels = self.overview_chapter_levels() as usize;
        if let Some(fixed) = self.fixed.as_ref().filter(|_| overview.chapter.is_none()) {
            let mut add_section = |start: usize, title: &str| {
                if start >= count {
                    return;
                }
                if start <= selected && closest_section_page.is_none_or(|old| start > old) {
                    closest_section_page = Some(start);
                    section_title = Some(title.to_owned());
                }
                // Quantize markers so a dense outline cannot draw more ticks
                // than the ribbon can distinguish. Mirror the same geometry
                // used by the scrub indicator below.
                let portion = start as f64 / count.saturating_sub(1).max(1) as f64;
                let portion = if layout.navigation.mirrored {
                    1.0 - portion
                } else {
                    portion
                };
                let offset = (portion * (track.w - 20).max(0) as f64).round() as i32;
                ticks.insert(offset / 3 * 3);
            };
            if let Some(custom) = self
                .library
                .state(fixed.id)
                .map(|book| &book.custom_contents)
                .filter(|custom| custom.enabled)
            {
                for entry in custom.entries() {
                    if levels > 0 && entry.position.chapter == 0 {
                        add_section(entry.position.page, &entry.label);
                    }
                }
            } else {
                for entry in fixed.reading.chapter_outline() {
                    if entry.depth < levels
                        && let Some(page) = entry.page
                    {
                        add_section(page, &entry.title);
                    }
                }
            }
        }
        if let Some(probe) = chapter_probe {
            section_title = overview
                .chapter
                .and_then(|chapter| probe.chapter_entries().get(chapter))
                .map(|entry| entry.label.clone());
            if let Some(stripe) = &stripe {
                for span in outline_spans {
                    if let Some(slot) = stripe.slot(span.start) {
                        let fraction = slot as f64 / stripe.slots.saturating_sub(1).max(1) as f64;
                        let fraction = if layout.navigation.mirrored {
                            1.0 - fraction
                        } else {
                            fraction
                        };
                        let offset = (fraction * (track.w - 20).max(0) as f64).round() as i32;
                        outline_ticks.insert(offset / 3 * 3);
                    }
                    if span.start <= selected_page
                        && closest_section_page.is_none_or(|previous| span.start > previous)
                        && !span.title.trim().is_empty()
                    {
                        closest_section_page = Some(span.start);
                        section_title = Some(span.title.clone());
                    }
                }
            }
        }
        if let Some(stripe) = &stripe {
            for slot in 0..stripe.slots {
                if stripe.page(slot, stripe_count).is_none() {
                    continue;
                }
                let fraction = slot as f64 / stripe.slots.saturating_sub(1).max(1) as f64;
                let fraction = if layout.navigation.mirrored {
                    1.0 - fraction
                } else {
                    fraction
                };
                let offset = (fraction * (track.w - 20).max(0) as f64).round() as i32;
                ticks.insert(offset / 3 * 3);
            }
        }
        let mut detail = format!(
            "{} · {} {} {}",
            language.overview_text(title),
            if stripe.is_some() {
                selected_page + 1
            } else {
                selected + 1
            },
            language.overview_text("of"),
            if stripe.is_some() {
                stripe_count
            } else {
                count
            }
        );
        if let Some(title) = section_title.filter(|title| !title.trim().is_empty()) {
            detail.push_str(" · ");
            detail.push_str(&title);
        }
        canvas.text(layout.track_caption(), &detail, CAPTION);
        let center = track.y + track.h / 2;
        if let Some(stripe) = &stripe {
            let span_height = 27usize;
            let lane_count = levels.max(1).min(span_height);
            let lane_height = (span_height / lane_count).max(1) as i32;
            let last_slot = stripe.slots.saturating_sub(1).max(1) as f64;
            let slot_step = (track.w - 20).max(0) as f64 / last_slot;
            let slot_boundary_x = |boundary: i64| {
                // KOReader's BookMapRow spans the full page slot from its
                // left edge to the right edge after the final page. Markers
                // stay centered, but section borders sit between markers.
                let logical = if layout.navigation.mirrored {
                    stripe.slots as f64 - boundary as f64 - 0.5
                } else {
                    boundary as f64 - 0.5
                };
                let x = track.x as f64 + 10.0 + logical * slot_step;
                x.clamp(track.x as f64, (track.x + track.w) as f64).round() as i32
            };
            for span in outline_spans {
                let start_slot = i64::try_from(span.start)
                    .unwrap_or(i64::MAX)
                    .saturating_sub(stripe.start);
                let end_slot = i64::try_from(span.end)
                    .unwrap_or(i64::MAX)
                    .saturating_sub(stripe.start);
                if end_slot <= 0 || start_slot >= stripe.slots as i64 {
                    continue;
                }
                let first = slot_boundary_x(start_slot);
                let last = slot_boundary_x(end_slot);
                let left = first.min(last);
                let right = first.max(last);
                let lane = span.depth.min(lane_count - 1) as i32;
                let y = track.y + 3 + lane * lane_height;
                let height = lane_height.min(27 - lane * lane_height).max(1);
                let width = right - left;
                if width <= 0 {
                    continue;
                }
                let shade = 232u8.saturating_sub((span.depth.min(7) as u8) * 11);
                canvas.fill(Rect::new(left, y, width, height), shade);
                let started_in_view = start_slot >= 0;
                let continued_after_view = end_slot > stripe.slots as i64 - 1;
                if started_in_view {
                    canvas.fill(Rect::new(first - 1, y, 2, height), 0);
                }
                if !continued_after_view {
                    canvas.fill(Rect::new(last - 1, y, 2, height), 0);
                }
                if started_in_view && height >= 14 && width >= 80 {
                    canvas.text(
                        Rect::new(left + 4, y, width - 8, height),
                        &span.title,
                        primitive_ui::TextStyle::new(12).center().ink(0),
                    );
                }
            }
        }
        canvas.fill(Rect::new(track.x, center - 3, track.w, 6), 151);
        for offset in ticks {
            canvas.fill(Rect::new(track.x + 10 + offset, center - 10, 2, 20), 85);
        }
        for offset in outline_ticks {
            canvas.fill(Rect::new(track.x + 10 + offset, center - 13, 3, 26), 0);
        }
        // 1 = highlight, 2 = note, 4 = bookmark. Coalesce nearby positions
        // to bound the marker count by available ribbon width.
        let mut saved = BTreeMap::<i32, u8>::new();
        let track_slot = |chapter: usize, page: usize| {
            if let Some(stripe) = &stripe {
                let page = if book_position.is_some() {
                    self.active
                        .as_ref()?
                        .reading
                        .whole_book_screen_position(chapter, page)?
                        .0
                        .checked_sub(1)?
                } else {
                    if Some(chapter) != overview.chapter || page >= overview.count {
                        return None;
                    }
                    page
                };
                stripe.slot(page)
            } else {
                (chapter == 0 && page < overview.count).then_some(page)
            }
        };
        let mut mark_page = |chapter: usize, page: usize, flags: u8| {
            let Some(page) = track_slot(chapter, page) else {
                return;
            };
            if page >= count {
                return;
            }
            let portion = page as f64 / count.saturating_sub(1).max(1) as f64;
            let portion = if layout.navigation.mirrored {
                1.0 - portion
            } else {
                portion
            };
            let offset = (portion * (track.w - 20).max(0) as f64).round() as i32;
            *saved.entry(offset / 5 * 5).or_default() |= flags;
        };
        // `page_for` scans line geometry and a highlight comparison also
        // inspects source blocks. Bound aggregate synchronous work per draw,
        // including history, rather than only bounding the mark count.
        let cue_work = std::cell::Cell::new(0usize);
        let unique_chapter = chapter_probe
            .and_then(|probe| {
                let path = &probe.chapter_entries().get(overview.chapter?)?.href;
                Some(
                    probe
                        .chapter_entries()
                        .iter()
                        .filter(|entry| &entry.href == path)
                        .take(2)
                        .count()
                        == 1,
                )
            })
            .unwrap_or(false);
        let chapter_for = |anchor: &Anchor| {
            let probe = chapter_probe?;
            if stripe.is_some() {
                if anchor.raw_caret.is_some() || !unique_chapter {
                    return None;
                }
                let cost = probe
                    .lines
                    .len()
                    .saturating_mul(4)
                    .saturating_add(probe.source.blocks.len().saturating_mul(16))
                    .saturating_add(probe.source.raw_only_blocks.len().saturating_mul(16))
                    .saturating_add(probe.source.elements.len().saturating_mul(16))
                    .max(1);
                let next = cue_work.get().saturating_add(cost);
                if next > 65_536 {
                    return None;
                }
                cue_work.set(next);
                return probe.page_for(anchor);
            }
            None
        };
        if let Some(fixed) = self.fixed.as_ref().filter(|_| overview.chapter.is_none())
            && let Some(state) = self.library.state(fixed.id)
        {
            for bookmark in &state.bookmarks {
                if bookmark.position.chapter == 0 && bookmark.position.anchor.is_none() {
                    mark_page(0, bookmark.position.page, 4);
                }
            }
            for highlight in &state.pdf_highlights {
                let flags = 1 | if highlight
                    .note
                    .as_ref()
                    .is_some_and(|note| !note.trim().is_empty())
                {
                    2
                } else {
                    0
                };
                for page in std::iter::once(highlight.page)
                    .chain(highlight.continuation.iter().map(|part| part.page))
                {
                    mark_page(0, page, flags);
                }
            }
        } else if let Some(active) = self.active.as_ref().filter(|_| overview.chapter.is_some())
            && let Some(state) = self.library.state(active.id)
        {
            let limit = PAGE_BROWSER_MARK_LIMIT;
            for (index, bookmark) in state.bookmarks.iter().take(limit).enumerate() {
                let saved_position = bookmark
                    .position
                    .anchor
                    .as_ref()
                    .and_then(|anchor| {
                        chapter_for(anchor).map(|page| (overview.chapter.unwrap_or(0), page))
                    })
                    .unwrap_or((bookmark.position.chapter, bookmark.position.page));
                let position = overview
                    .bookmark_pages
                    .get(index)
                    .copied()
                    .flatten()
                    .and_then(|page| self.active.as_ref()?.reading.screen_page_location(page + 1))
                    .unwrap_or(saved_position);
                mark_page(position.0, position.1, 4);
            }
            let highlight_limit = limit.saturating_sub(state.bookmarks.len().min(limit));
            let stripe_range = stripe.as_ref().map(|stripe| {
                let first = stripe.start.max(0) as usize;
                let last = stripe.start.saturating_add(stripe.slots as i64 - 1).max(0) as usize;
                (first, last)
            });
            for (index, highlight) in state.highlights.iter().take(highlight_limit).enumerate() {
                let selection = &highlight.selection;
                if selection.start.raw_caret.is_some() || selection.end.raw_caret.is_some() {
                    continue;
                }
                let noted = highlight
                    .note
                    .as_ref()
                    .is_some_and(|note| !note.trim().is_empty());
                let flags = 1 | if noted { 2 } else { 0 };
                let indexed = overview.highlight_pages.get(index).copied().flatten();
                if let Some((start, end)) = indexed {
                    let same_chapter = selection.start.chapter == selection.end.chapter;
                    let ordered_in_page = start < end
                        || (same_chapter
                            && start == end
                            && chapter_for(&selection.start).is_some_and(|start_page| {
                                chapter_probe
                                    .and_then(|probe| {
                                        (probe.compare_text_anchors(&selection.start, &selection.end)
                                            == Some(std::cmp::Ordering::Less))
                                        .then_some(start_page)
                                    })
                                    .is_some()
                            }));
                    let cross_chapter_verified = !same_chapter
                        && self
                            .overview_raw
                            .as_ref()
                            .is_some_and(|job| job.selection(selection));
                    if !ordered_in_page || (!same_chapter && !cross_chapter_verified) {
                        continue;
                    }
                    if let Some((visible_start, visible_end)) = stripe_range {
                        let first = start.max(visible_start);
                        let last = end.min(visible_end).min(stripe_count.saturating_sub(1));
                        if first <= last {
                            for absolute in first..=last {
                                let page = if book_position.is_some() {
                                    self.active.as_ref().and_then(|active| {
                                        active.reading.screen_page_location(absolute + 1)
                                    })
                                } else {
                                    Some((overview.chapter.unwrap_or(0), absolute))
                                };
                                if let Some((chapter, page)) = page {
                                    mark_page(chapter, page, flags);
                                }
                            }
                        }
                    }
                } else {
                    // When the whole-book index is unavailable, retain exact
                    // single-chapter cues from the already loaded layout.
                    let start = chapter_for(&selection.start);
                    let end = chapter_for(&selection.end);
                    if let (Some(start), Some(end), Some(probe)) = (start, end, chapter_probe)
                        && probe.compare_text_anchors(&selection.start, &selection.end)
                            == Some(std::cmp::Ordering::Less)
                    {
                        for page in start..=end {
                            mark_page(overview.chapter.unwrap_or(0), page, flags);
                        }
                    }
                }
            }
        }
        for (offset, flags) in saved {
            let x = track.x + 10 + offset;
            if flags & 1 != 0 {
                canvas.fill(Rect::new(x - 2, center - 5, 5, 10), 85);
            }
            if flags & 2 != 0 {
                canvas.fill(Rect::new(x - 2, center - 5, 5, 10), 0);
            }
            if flags & 4 != 0 {
                canvas.fill(Rect::new(x - 2, center - 13, 5, 26), 0);
            }
        }
        let portion = selected as f64 / count.saturating_sub(1).max(1) as f64;
        let portion = if layout.navigation.mirrored {
            1.0 - portion
        } else {
            portion
        };
        let x = track.x + (portion * (track.w - 20) as f64).round() as i32;
        canvas.fill(Rect::new(x, center - 15, 20, 30), 0);

        // The scrub handle is the browsed location; these separate marks keep
        // the reading location and recent history visible while
        // browsing without changing either one.
        let marker_offset = |position: usize| {
            let fraction = position as f64 / count.saturating_sub(1).max(1) as f64;
            let fraction = if layout.navigation.mirrored {
                1.0 - fraction
            } else {
                fraction
            };
            (fraction * (track.w - 20).max(0) as f64).round() as i32
        };
        if (self.fixed.is_some() && overview.chapter.is_none())
            || (self.active.is_some() && overview.chapter.is_some())
        {
            let mut history = BTreeMap::<i32, u8>::new();
            for (locations, flag, indexed_pages) in [
                (&self.back, 1, &overview.back_pages),
                (&self.forward, 2, &overview.forward_pages),
            ] {
                for (index, location) in locations.iter().rev().take(3).enumerate() {
                    let position = &location.position;
                    let indexed = indexed_pages
                        .get(index)
                        .copied()
                        .flatten()
                        .and_then(|page| {
                            self.active.as_ref()?.reading.screen_page_location(page + 1)
                        });
                    let saved = if overview.chapter.is_none() {
                        (position.chapter == 0).then_some((0, position.page))
                    } else {
                        position
                            .anchor
                            .as_ref()
                            .and_then(|anchor| {
                                chapter_for(anchor)
                                    .map(|page| (overview.chapter.unwrap_or(0), page))
                            })
                            .or(Some((position.chapter, position.page)))
                    };
                    let location = indexed.or(saved);
                    let coordinate = location.and_then(|(chapter, page)| track_slot(chapter, page));
                    if let Some(coordinate) = coordinate.filter(|&coordinate| coordinate < count) {
                        let bucket = marker_offset(coordinate) / 18 * 18;
                        *history.entry(bucket).or_default() |= flag;
                    }
                }
            }
            for (offset, flags) in history {
                let marker_x = track.x + 4 + offset;
                let y = center + 18;
                if flags & 1 != 0 {
                    canvas.fill(Rect::new(marker_x, y, 14, 14), 0);
                }
                if flags & 2 != 0 {
                    let x = marker_x + if flags & 1 != 0 { 7 } else { 0 };
                    canvas.fill(Rect::new(x, y, 14, 14), 0);
                    canvas.fill(Rect::new(x + 3, y + 3, 8, 8), 255);
                }
            }
        }
        let reading = if overview.chapter.is_none() {
            self.fixed.as_ref().map(|book| book.reading.page())
        } else {
            self.active.as_ref().and_then(|book| {
                if book_position.is_some() {
                    book.reading
                        .whole_book_screen_position(book.reading.chapter, book.reading.page)
                        .and_then(|(ordinal, _)| stripe.as_ref()?.slot(ordinal.saturating_sub(1)))
                } else {
                    (Some(book.reading.chapter) == overview.chapter)
                        .then_some(book.reading.page)
                        .and_then(|page| stripe.as_ref()?.slot(page))
                }
            })
        };
        if let Some(reading) = reading.filter(|&reading| reading < count) {
            let x = track.x + 10 + marker_offset(reading);
            let y = center + 16;
            // An upward caret points at the actual reading position.
            canvas.fill(Rect::new(x - 3, y, 6, 6), 0);
            canvas.fill(Rect::new(x - 6, y + 6, 12, 6), 0);
            canvas.fill(Rect::new(x - 9, y + 12, 18, 6), 0);
        }
    }

    pub(super) fn overview_swipe(&mut self, input: Input, start_x: f32) -> Option<Change> {
        let view = self.overview.as_ref()?;
        if view.grid_menu {
            return Some(Change::None);
        }
        let layout = OverviewLayout::with_grid(self.navigation_layout(), view.grid);
        let chapter = view.chapter;
        let focus_page = view.focus.page;
        let count = view.count;
        let book_position = chapter.and_then(|chapter| {
            self.active
                .as_ref()?
                .reading
                .whole_book_screen_position(chapter, focus_page)
        });
        let focus = book_position.map_or(focus_page, |(ordinal, _)| ordinal.saturating_sub(1));
        let stripe_count = book_position.map_or(count, |(_, total)| total);
        let stripe_slots = PageStripe::centered(focus, capacity(view.grid)).slots;
        let Input::Swipe { dx, dy, start_y } = input else {
            return None;
        };
        if let Some(change) = self.overview_grid_swipe(layout, (start_x, start_y), (dx, dy)) {
            return Some(change);
        }
        if dx.abs() > dy.abs()
            && layout.track().contains(start_x, start_y)
            && let Some(chapter) = chapter
        {
            let forward = (dx < 0.0) != layout.navigation.mirrored;
            let target = if forward {
                focus
                    .saturating_add(stripe_slots)
                    .min(stripe_count.saturating_sub(1))
            } else {
                focus.saturating_sub(stripe_slots)
            };
            return Some(if book_position.is_some() {
                self.active
                    .as_ref()
                    .and_then(|active| active.reading.screen_page_location(target + 1))
                    .map_or(Change::None, |(chapter, page)| {
                        self.focus_overview_page(chapter, page)
                    })
            } else {
                self.focus_overview_page(chapter, target)
            });
        }
        (dx.abs() > dy.abs() && layout.track().contains(start_x, start_y))
            .then(|| self.scrub_overview(start_x + dx))
    }

    fn overview_ribbon_target(&self, x: f32) -> Option<RibbonMapTarget> {
        let overview = self.overview.as_ref()?;
        let track = OverviewLayout::new(self.navigation_layout()).track();
        let fraction = ((x - track.x as f32) / track.w as f32).clamp(0.0, 1.0);
        let fraction = if self.navigation_layout().mirrored {
            1.0 - fraction
        } else {
            fraction
        };
        Some(if let Some(active) = &self.active {
            let count = active.reading.probe.chapter_count();
            let chapter = (fraction * count.saturating_sub(1) as f32).round() as usize;
            RibbonMapTarget::Chapter(chapter)
        } else {
            let page = (fraction * overview.count.saturating_sub(1) as f32).round() as usize;
            RibbonMapTarget::Fixed(page)
        })
    }

    fn scrub_overview(&mut self, x: f32) -> Change {
        if let Some(overview) = &self.overview
            && let Some(chapter) = overview.chapter
        {
            let track = OverviewLayout::new(self.navigation_layout()).track();
            let width = (track.w - 20).max(1) as f32;
            let fraction = ((x - (track.x + 10) as f32) / width).clamp(0.0, 1.0);
            let fraction = if self.navigation_layout().mirrored {
                1.0 - fraction
            } else {
                fraction
            };
            let book_position = self.active.as_ref().and_then(|active| {
                active
                    .reading
                    .whole_book_screen_position(chapter, overview.focus.page)
            });
            let focus = book_position.map_or(overview.focus.page, |(ordinal, _)| {
                ordinal.saturating_sub(1)
            });
            let total = book_position.map_or(overview.count, |(_, total)| total);
            let stripe = PageStripe::centered(focus, capacity(overview.grid));
            let slot = (fraction * stripe.slots.saturating_sub(1) as f32).round() as usize;
            return stripe.page(slot, total).map_or(Change::None, |page| {
                if book_position.is_some() {
                    self.active
                        .as_ref()
                        .and_then(|active| active.reading.screen_page_location(page + 1))
                        .map_or(Change::None, |(chapter, page)| {
                            self.focus_overview_page(chapter, page)
                        })
                } else {
                    self.focus_overview_page(chapter, page)
                }
            });
        }
        let result = match self.overview_ribbon_target(x) {
            Some(RibbonMapTarget::Chapter(chapter)) => {
                if self
                    .overview
                    .as_ref()
                    .is_some_and(|view| view.chapter == Some(chapter))
                {
                    return Change::None;
                }
                self.update_overview(Some(chapter), Some(0))
            }
            Some(RibbonMapTarget::Fixed(page)) => self.update_overview(None, Some(page)),
            None => return Change::None,
        };
        match result {
            Ok(()) => Change::Partial,
            Err(error) => self.report(error),
        }
    }

    fn focus_overview_page(&mut self, chapter: usize, page: usize) -> Change {
        if self
            .overview
            .as_ref()
            .is_some_and(|view| view.chapter == Some(chapter) && view.focus.page == page)
        {
            return Change::None;
        }
        match self.update_overview(Some(chapter), Some(page)) {
            Ok(()) => {
                if let Some(view) = &mut self.overview {
                    view.focus = OverviewFocus {
                        page,
                        slot: page - view.first,
                        anchor: None,
                    };
                }
                Change::Partial
            }
            Err(error) => self.report(error),
        }
    }

    pub(super) fn overview_input(&mut self, input: Input) -> Change {
        let Some(overview) = &self.overview else {
            return Change::None;
        };
        let navigation = self.navigation_layout();
        let layout = OverviewLayout::with_grid(navigation, overview.grid);
        if let Some(change) = self.overview_grid_input(input, layout) {
            return change;
        }
        let overview = self.overview.as_ref().expect("The overview is still open");
        let batch = capacity(overview.grid);
        let (direction, step) = match input {
            Input::LongTap(x, y) if layout.track().contains(x, y) => {
                return overview
                    .chapter
                    .map(RibbonMapTarget::Chapter)
                    .or_else(|| self.overview_ribbon_target(x))
                    .map_or(Change::None, |target| self.hold_overview_ribbon(target));
            }
            Input::LongTap(x, y) => {
                let Some(index) =
                    (0..overview.tiles.len()).find(|&index| layout.tile(index).contains(x, y))
                else {
                    return Change::None;
                };
                let result = self.hold_overview_page(overview.chapter, overview.first + index);
                return match result {
                    Ok(()) => Change::Partial,
                    Err(error) => self.report(error),
                };
            }
            Input::PageForward => (1, batch),
            Input::PageBack => (-1, batch),
            Input::Swipe { dx, dy, .. } if dy.abs() > dx.abs() => {
                (if dy < 0.0 { 1 } else { -1 }, layout.columns())
            }
            Input::Swipe { dx, dy, .. } if dx.abs() > dy.abs() => (
                if (dx < 0.0) ^ navigation.mirrored {
                    1
                } else {
                    -1
                },
                batch,
            ),
            Input::Tap(x, y) if navigation.previous().contains(x, y) => (-1, batch),
            Input::Tap(x, y) if navigation.next().contains(x, y) => (1, batch),
            Input::Tap(x, y) if layout.track().contains(x, y) => return self.scrub_overview(x),
            Input::Tap(x, y) if navigation.back().contains(x, y) => {
                if let Some(map) = overview.map_origin {
                    self.show(Screen::BookMap);
                    match map {
                        MapOrigin::Fixed(window) => self.map_window = window,
                        MapOrigin::Chapter(chapter) => self.focus_chapter_map(chapter),
                    }
                } else {
                    self.show(self.tools_screen());
                }
                return Change::Partial;
            }
            Input::Tap(x, y) => {
                let index =
                    (0..overview.tiles.len()).find(|&index| layout.tile(index).contains(x, y));
                if let Some(index) = index {
                    let page = overview.first + index;
                    let result = if let Some(chapter) = overview.chapter {
                        self.follow_reflow_page(chapter, page)
                    } else {
                        self.follow_fixed(page, None)
                    };
                    return match result {
                        Ok(()) => Change::Partial,
                        Err(error) => self.report(error),
                    };
                }
                let Some(direction) = layout.blank_tap_direction(x, y) else {
                    return Change::None;
                };
                (direction, batch)
            }
            _ => return Change::None,
        };
        let chapter = overview.chapter;
        let first = if direction < 0 {
            overview.first.saturating_sub(step)
        } else {
            (overview.first + step).min(overview_start(None, overview.count, batch))
        };
        let (chapter, first) = if first != overview.first {
            (chapter, Some(first))
        } else if let Some(chapter) = chapter
            && let Some(active) = &self.active
            && let Some(next) = chapter.checked_add_signed(direction)
            && next < active.reading.probe.chapter_count()
        {
            (Some(next), (direction > 0).then_some(0))
        } else {
            return Change::None;
        };
        match self.update_overview(chapter, first) {
            Ok(()) => Change::Partial,
            Err(error) => self.report(error),
        }
    }
}

fn overview_start(first: Option<usize>, count: usize, batch: usize) -> usize {
    let last = count.saturating_sub(1) / batch * batch;
    first.map_or(last, |first| first.min(last))
}

fn overview_window(
    count: usize,
    batch: usize,
    first: Option<usize>,
    saved: Option<&OverviewFocus>,
    selected: Option<usize>,
) -> (usize, OverviewFocus) {
    if let Some((saved, page)) = saved.zip(selected) {
        let first = overview_start(
            Some(page.saturating_sub(saved.slot.min(batch - 1))),
            count,
            batch,
        );
        (
            first,
            OverviewFocus {
                page,
                slot: page - first,
                anchor: saved.anchor.clone(),
            },
        )
    } else {
        let first = overview_start(first, count, batch);
        (
            first,
            OverviewFocus {
                page: first,
                slot: 0,
                anchor: None,
            },
        )
    }
}

fn reflow_overview(
    probe: &mut Probe,
    chapter: usize,
    first: Option<usize>,
    saved_focus: Option<&OverviewFocus>,
    grid: Option<PageBrowserGrid>,
) -> Result<PageOverview> {
    let count = probe.page_count();
    let batch = capacity(grid);
    let selected = if let Some(saved) = saved_focus {
        Some(if let Some(anchor) = &saved.anchor {
            probe
                .page_for(anchor)
                .ok_or("The overview could not keep its source page")?
        } else {
            saved.page.min(count.saturating_sub(1))
        })
    } else {
        None
    };
    let (first, mut focus) = overview_window(count, batch, first, saved_focus, selected);
    if focus.anchor.is_none() {
        focus.anchor = probe.page_anchor(focus.page);
    }
    let tiles = (first..(first + batch).min(count))
        .map(|page| page_thumbnail(probe, page).ok())
        .collect();
    Ok(PageOverview {
        first,
        count,
        chapter: Some(chapter),
        preview: None,
        tiles,
        focus,
        bookmark_pages: Vec::new(),
        back_pages: Vec::new(),
        forward_pages: Vec::new(),
        highlight_pages: Vec::new(),
        outline_spans: Vec::new(),
        book_outline_spans: Vec::new(),
        map_origin: None,
        grid,
        grid_menu: false,
        grid_help: false,
    })
}
