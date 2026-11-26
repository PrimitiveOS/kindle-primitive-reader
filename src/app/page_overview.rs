//! Page overview thumbnails and the source position of the browsed page.
use primitive_app::Change;
use primitive_ui::{CAPTION, Canvas, GrayImage, Input, Rect};
use std::collections::{BTreeMap, BTreeSet};

use super::fixed_book_map::MapWindow;
use super::navigation_layout::NavigationLayout;
use super::{Action, LIST_X, LIST_Y, OVERVIEW_PAGES, Reader, Screen, bar_icons, fixed_page_label};
use crate::generated_contents::OutlineTarget;
use crate::library::{PageBrowserGrid, PageBrowserNumbers};
use crate::session::{PageThumbnail, page_thumbnail};
use crate::{Anchor, Probe, Result};

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
    pub(super) map_origin: Option<MapOrigin>,
    pub(super) grid: Option<PageBrowserGrid>,
    pub(super) grid_menu: bool,
    pub(super) grid_help: bool,
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
                // Canonical selections need a complete quote proof only for
                // the currently displayed thumbnail chapter; their ribbon
                // chapter cue is handled by the existing canonical path.
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
        overview.map_origin = map_origin;
        overview.grid_menu = grid_menu;
        overview.grid_help = grid_help;
        Ok(overview)
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
        let single_spine = overview.chapter == Some(0)
            && self.active.as_ref().is_some_and(|active| {
                active.reading.chapter == 0 && active.reading.probe.chapter_count() == 1
            });
        let (selected, count, title) = if single_spine {
            (page, overview.count, "Skim pages")
        } else {
            overview
                .chapter
                .map_or((page, overview.count, "Skim pages"), |chapter| {
                    (chapter, chapters, "Skim chapters")
                })
        };
        let mut section_title = None;
        let mut closest_section_page = None;
        let mut ticks = BTreeSet::new();
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
        if let Some(active) = self.active.as_ref().filter(|_| overview.chapter.is_some()) {
            let probe = &active.reading.probe;
            // The reflow track represents spine chapters, not pages within a
            // chapter. Its markers show which chapters contain TOC entries.
            let mut add_chapter = |chapter: usize| {
                if single_spine {
                    return;
                }
                if chapter >= count {
                    return;
                }
                let portion = chapter as f64 / count.saturating_sub(1).max(1) as f64;
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
                .state(active.id)
                .map(|book| &book.custom_contents)
                .filter(|custom| custom.enabled)
            {
                for entry in custom.entries().iter().filter(|_| levels > 0) {
                    let chapter = match &entry.position.anchor {
                        Some(anchor) => probe.chapter_index(&anchor.chapter),
                        None => Some(entry.position.chapter),
                    };
                    if let Some(chapter) = chapter {
                        add_chapter(chapter);
                    }
                }
            } else if let Some(entries) = self.alternative_entries(active.id) {
                for entry in entries.iter().filter(|entry| entry.depth < levels) {
                    let chapter = match &entry.target {
                        OutlineTarget::Chapter(chapter) | OutlineTarget::Heading(chapter, _) => {
                            *chapter
                        }
                    };
                    add_chapter(chapter);
                }
            } else if probe.toc.is_empty() {
                for (chapter, _) in probe
                    .chapter_entries()
                    .iter()
                    .enumerate()
                    .filter(|_| levels > 0)
                {
                    add_chapter(chapter);
                }
            } else {
                for entry in probe.toc.iter().filter(|entry| entry.depth < levels) {
                    if let Some(chapter) = entry
                        .destination
                        .as_ref()
                        .and_then(|destination| probe.chapter_index(&destination.path))
                    {
                        add_chapter(chapter);
                    }
                }
            }
            // A TOC heading can start later within the chapter: show the
            // spine label instead of implying the heading has started.
            section_title = probe
                .chapter_entries()
                .get(if single_spine { 0 } else { selected })
                .map(|entry| entry.label.clone());
        }
        let mut detail = format!(
            "{} · {} {} {count}",
            language.overview_text(title),
            selected + 1,
            language.overview_text("of")
        );
        if let Some(title) = section_title.filter(|title| !title.trim().is_empty()) {
            detail.push_str(" · ");
            detail.push_str(&title);
        }
        canvas.text(layout.track_caption(), &detail, CAPTION);
        let center = track.y + track.h / 2;
        canvas.fill(Rect::new(track.x, center - 3, track.w, 6), 151);
        for offset in ticks {
            canvas.fill(Rect::new(track.x + 10 + offset, center - 10, 2, 20), 85);
        }
        // 1 = highlight, 2 = note, 4 = bookmark. Coalesce nearby positions
        // to bound the marker count by available ribbon width.
        let mut saved = BTreeMap::<i32, u8>::new();
        let mut mark_page = |page: usize, flags: u8| {
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
        let chapter_for = |anchor: &Anchor| {
            let reading = &self.active.as_ref()?.reading;
            let probe = &reading.probe;
            if single_spine {
                if anchor.raw_caret.is_some() {
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
            if anchor.book != probe.book {
                return None;
            }
            let chapter = probe.chapter_index(&anchor.chapter)?;
            if anchor.raw_caret.is_some() {
                let loaded = if chapter == reading.chapter {
                    Some(probe)
                } else if overview.chapter == Some(chapter) {
                    overview.preview.as_deref()
                } else {
                    None
                };
                let valid = loaded.map_or_else(
                    || {
                        self.overview_raw
                            .as_ref()
                            .is_some_and(|job| job.history(anchor))
                    },
                    |probe| probe.page_for(anchor).is_some(),
                );
                if !valid {
                    return None;
                }
            }
            Some(chapter)
        };
        if let Some(fixed) = self.fixed.as_ref().filter(|_| overview.chapter.is_none())
            && let Some(state) = self.library.state(fixed.id)
        {
            for bookmark in &state.bookmarks {
                if bookmark.position.chapter == 0 && bookmark.position.anchor.is_none() {
                    mark_page(bookmark.position.page, 4);
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
                    mark_page(page, flags);
                }
            }
        } else if let Some(active) = self.active.as_ref().filter(|_| overview.chapter.is_some())
            && let Some(state) = self.library.state(active.id)
        {
            let limit = if single_spine { 512 } else { usize::MAX };
            for bookmark in state.bookmarks.iter().take(limit) {
                let chapter = match &bookmark.position.anchor {
                    Some(anchor) => chapter_for(anchor),
                    None if single_spine => {
                        (bookmark.position.chapter == 0).then_some(bookmark.position.page)
                    }
                    None => Some(bookmark.position.chapter),
                };
                if let Some(chapter) = chapter {
                    mark_page(chapter, 4);
                }
            }
            let highlight_limit = if single_spine {
                limit.saturating_sub(state.bookmarks.len().min(limit))
            } else {
                usize::MAX
            };
            for highlight in state.highlights.iter().take(highlight_limit) {
                let selection = &highlight.selection;
                if selection.start.raw_caret.is_some() || selection.end.raw_caret.is_some() {
                    continue;
                }
                let start = chapter_for(&selection.start);
                if single_spine
                    && (start.is_none()
                        || chapter_for(&selection.end).is_none()
                        || active
                            .reading
                            .probe
                            .compare_text_anchors(&selection.start, &selection.end)
                            != Some(std::cmp::Ordering::Less))
                {
                    continue;
                }
                if let Some(chapter) = start {
                    let noted = highlight
                        .note
                        .as_ref()
                        .is_some_and(|note| !note.trim().is_empty());
                    mark_page(chapter, 1 | if noted { 2 } else { 0 });
                }
            }
            if !single_spine && let Some(job) = &self.overview_raw {
                for (bin, &flags) in job.bins().iter().enumerate() {
                    if flags != 0 {
                        let chapter = bin
                            .saturating_mul(count.saturating_sub(1))
                            .saturating_add(127)
                            / 255;
                        mark_page(chapter, flags);
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
            for (locations, flag) in [(&self.back, 1), (&self.forward, 2)] {
                for location in locations.iter().rev().take(3) {
                    let position = &location.position;
                    let coordinate = if overview.chapter.is_none() {
                        (position.chapter == 0).then_some(position.page)
                    } else {
                        match &position.anchor {
                            Some(anchor) => chapter_for(anchor),
                            None if single_spine => {
                                (position.chapter == 0).then_some(position.page)
                            }
                            None => Some(position.chapter),
                        }
                    };
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
            self.active.as_ref().map(|book| {
                if single_spine {
                    book.reading.page
                } else {
                    book.reading.chapter
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
        let Input::Swipe { dx, dy, start_y } = input else {
            return None;
        };
        if let Some(change) = self.overview_grid_swipe(layout, (start_x, start_y), (dx, dy)) {
            return Some(change);
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
        if let (Some(active), Some(overview)) = (&self.active, &self.overview)
            && overview.chapter == Some(0)
            && active.reading.chapter == 0
            && active.reading.probe.chapter_count() == 1
        {
            let track = OverviewLayout::new(self.navigation_layout()).track();
            let width = (track.w - 20).max(1) as f32;
            let fraction = ((x - (track.x + 10) as f32) / width).clamp(0.0, 1.0);
            let fraction = if self.navigation_layout().mirrored {
                1.0 - fraction
            } else {
                fraction
            };
            let page = (fraction * overview.count.saturating_sub(1) as f32).round() as usize;
            if page == overview.first {
                return Change::None;
            }
            return match self.update_overview(Some(0), Some(page)) {
                Ok(()) => Change::Partial,
                Err(error) => self.report(error),
            };
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
                return self
                    .overview_ribbon_target(x)
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
        map_origin: None,
        grid,
        grid_menu: false,
        grid_help: false,
    })
}
