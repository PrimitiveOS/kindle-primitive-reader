use std::io;
use std::ops::Range;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::epub::{Book, Destination};
use crate::selection::{HighlightRect, TextSelection};
use crate::{Anchor, Engine, Probe, Result, Settings};
use primitive_app::{App, Change, HEIGHT, WIDTH, with_storage};
use primitive_ui::{CAPTION, Canvas, GrayImage, HAIRLINE, Input, Rect};

const FOOTER_HEIGHT: u32 = 74;
pub(crate) const PAGE_HEIGHT: u32 = HEIGHT - FOOTER_HEIGHT;
pub(crate) const SHOW_STATUS: Rect = Rect::new(8, PAGE_HEIGHT as i32 + 5, 148, 60);
pub(crate) const DONE: Rect = Rect::new(WIDTH as i32 - 154, PAGE_HEIGHT as i32 + 5, 148, 60);
const FACING_GUTTER: u32 = 32;
const FACING_PAGE_WIDTH: u32 = (WIDTH - FACING_GUTTER) / 2;
const PREFETCH_DELAY: Duration = Duration::from_millis(480);
// Give a partial E Ink update time to settle before submitting another.
pub(crate) const PAGE_REFRESH_GAP: Duration = Duration::from_millis(450);

pub(crate) struct PagePaint {
    pub visible: Range<u32>,
    pub inverted: bool,
}

impl PagePaint {
    pub fn full(inverted: bool) -> Self {
        Self {
            visible: 0..PAGE_HEIGHT,
            inverted,
        }
    }

    pub(crate) fn apply(self, mut page: GrayImage) -> (Rect, GrayImage) {
        let Self { visible, inverted } = self;
        let visible = visible.start.min(page.height)..visible.end.min(page.height);
        if visible != (0..page.height) {
            let rows = (visible.start * WIDTH) as usize..(visible.end * WIDTH) as usize;
            let length = rows.len();
            page.pixels.copy_within(rows, 0);
            page.pixels.truncate(length);
            page.height = visible.end - visible.start;
        }
        if inverted {
            for pixel in &mut page.pixels {
                *pixel = 255 - *pixel;
            }
        }
        let rect = Rect::new(0, visible.start as i32, WIDTH as i32, page.height as i32);
        (rect, page)
    }
}

pub(crate) struct PageTurns<T = isize> {
    pub(crate) pending: Vec<T>,
    pub(crate) next_refresh: Instant,
}

impl<T> PageTurns<T> {
    pub(crate) fn new(now: Instant) -> Self {
        Self {
            pending: Vec::new(),
            next_refresh: now + PAGE_REFRESH_GAP,
        }
    }

    pub(crate) fn deadline(&self) -> Option<Instant> {
        (!self.pending.is_empty()).then_some(self.next_refresh)
    }

    pub(crate) fn take_due(&mut self, now: Instant) -> Vec<T> {
        if now < self.next_refresh {
            return Vec::new();
        }
        std::mem::take(&mut self.pending)
    }
}

enum ReadingMotion {
    Page(isize),
    Pixels(f32),
}

#[derive(Clone)]
pub(crate) struct BookConfig {
    pub(crate) user_hyphenation: std::rc::Rc<crate::user_hyphenation::Rules>,
    pub(crate) engine: Engine,
    pub(crate) book: PathBuf,
    pub(crate) fonts: Vec<PathBuf>,
    pub(crate) settings: Settings,
    pub(crate) scroll: bool,
    pub(crate) facing_pages: bool,
    // Shared across chapter layouts, previews and font changes for the same open publication.
    pub(crate) chapter_offsets: std::rc::Rc<std::cell::OnceCell<Vec<u64>>>,
}

impl BookConfig {
    pub(crate) fn chapter(&self, index: usize) -> Result<Probe> {
        let start = Instant::now();
        let probe = with_storage(|| {
            let book = Book::open(&self.book).map_err(io::Error::other)?;
            self.layout_chapter(book, index).map_err(io::Error::other)
        })?;
        eprintln!(
            "reader engine={:?} chapter={} layout_us={} pages={}",
            self.engine,
            index + 1,
            start.elapsed().as_micros(),
            probe.page_count(),
        );
        Ok(probe)
    }

    fn layout_chapter(&self, mut book: Book, index: usize) -> Result<Probe> {
        self.chapter_offsets
            .get_or_init(|| book.chapter_offsets().unwrap_or_default());
        let settings = if self.facing_pages && !self.scroll {
            let margin_limit = FACING_PAGE_WIDTH / 4;
            Settings {
                width: FACING_PAGE_WIDTH,
                margin: self.settings.margin.min(margin_limit),
                margin_sides: None,
                margin_left: Some(self.settings.left_margin().min(margin_limit)),
                margin_right: Some(self.settings.right_margin().min(margin_limit)),
                margin_top: Some(self.settings.top_margin()),
                margin_bottom: Some(self.settings.bottom_margin()),
                ..self.settings
            }
        } else {
            self.settings
        };
        settings.validate()?;
        let narrow = settings.width == FACING_PAGE_WIDTH;
        match Probe::from_book_with_rules(
            book, self.engine, &self.fonts, settings, index, None, &self.user_hyphenation,
        ) {
            Ok(probe) if !narrow || !probe.is_vertical() => Ok(probe),
            Err(error) if !narrow => Err(error),
            _ => {
                // Retain the full viewport for vertical layout and for a chapter
                // that only fits the renderer's bounds at its original width.
                let book = Book::open(&self.book).map_err(|e| e.to_string())?;
                Probe::from_book_with_rules(
                    book, self.engine, &self.fonts, self.settings, index, None, &self.user_hyphenation,
                )
            }
        }
    }
}

pub(crate) struct Reading {
    pub(crate) config: BookConfig,
    pub(crate) chapter: usize,
    pub(crate) probe: Probe,
    pub(crate) page: usize,
    scroll_top: Option<f32>,
    focus_anchor: Option<Anchor>,
    frame: GrayImage,
    error: Option<String>,
    adjacent: Vec<(usize, GrayImage)>,
    prefetch: Option<Instant>,
    turns: PageTurns<ReadingMotion>,
    end_attempt: bool,
}

impl Reading {
    pub(crate) fn open(config: BookConfig) -> Result<Self> {
        Self::restore(config, 0, 0, None)
    }

    pub(crate) fn restore(
        config: BookConfig,
        chapter: usize,
        fallback_page: usize,
        anchor: Option<&Anchor>,
    ) -> Result<Self> {
        let start = Instant::now();
        let mut probe = config.chapter(chapter)?;
        let focused_page = anchor.and_then(|anchor| probe.page_for(anchor));
        let anchor = anchor.filter(|_| focused_page.is_some());
        let page = focused_page.unwrap_or(fallback_page.min(probe.page_count() - 1));
        let scroll_top = (config.scroll && !probe.is_vertical()).then(|| {
            anchor
                .and_then(|anchor| probe.anchor_top(anchor))
                .unwrap_or_else(|| probe.page_range(page).start)
                .clamp(0.0, probe.scroll_end())
        });
        let page = scroll_top.map_or(page, |top| probe.page_at(top));
        let frame = capture(&mut probe, chapter, page, scroll_top)?;
        eprintln!(
            "reader first_frame_ready_us={}",
            start.elapsed().as_micros()
        );
        let mut session = Self {
            config,
            chapter,
            probe,
            page,
            scroll_top,
            focus_anchor: anchor.cloned(),
            frame,
            error: None,
            adjacent: Vec::new(),
            prefetch: None,
            turns: PageTurns::new(Instant::now()),
            end_attempt: false,
        };
        session.schedule_prefetch();
        Ok(session)
    }

    fn turn(&mut self, direction: isize) -> Result<Change> {
        let start = Instant::now();
        let mut crossed_chapter = false;
        let mut cache_hit = false;
        let span = frame_span(&self.probe);
        let frame_page = first_frame_page(&self.probe, self.page);
        if let Some(page) = frame_page
            .checked_add_signed(direction * span as isize)
            .filter(|page| *page < self.probe.page_count())
        {
            let next =
                if let Some(index) = self.adjacent.iter().position(|(index, _)| *index == page) {
                    cache_hit = true;
                    self.adjacent.swap_remove(index).1
                } else {
                    capture(&mut self.probe, self.chapter, page, None)?
                };
            let prior = std::mem::replace(&mut self.frame, next);
            self.adjacent
                .retain(|(index, _)| index.abs_diff(page) == span);
            self.adjacent.push((frame_page, prior));
            self.page = page;
        } else if let Some(chapter) = self
            .chapter
            .checked_add_signed(direction)
            .filter(|chapter| *chapter < self.probe.chapter_count())
        {
            let probe = self.config.chapter(chapter)?;
            self.land_at_chapter_edge(probe, chapter, direction)?;
            crossed_chapter = true;
        } else {
            return Ok(Change::None);
        }
        self.focus_anchor = None;
        self.error = None;
        eprintln!(
            "reader turn_frame_ready_us={} chapter_transition={crossed_chapter} cache_hit={cache_hit}",
            start.elapsed().as_micros(),
        );
        Ok(Change::Partial)
    }

    fn present_page_turns(&mut self, now: Instant) -> Change {
        let pending = self.turns.take_due(now);
        if pending.is_empty() {
            return Change::None;
        }
        let count = pending.len();
        let previous = (self.chapter, self.page, self.scroll_top);
        let mut change = Change::None;
        for motion in pending {
            let forward = match motion {
                ReadingMotion::Page(direction) => direction > 0,
                ReadingMotion::Pixels(distance) => distance > 0.0,
            };
            let result = match (motion, self.scroll_top) {
                (ReadingMotion::Page(direction), Some(top)) => {
                    let distance = (self.config.settings.text_height() as f32 - 90.0).max(1.0);
                    self.scroll(top, direction as f32 * distance)
                }
                (ReadingMotion::Page(direction), None) => self.turn(direction),
                (ReadingMotion::Pixels(distance), Some(top)) => self.scroll(top, distance),
                (ReadingMotion::Pixels(_), None) => continue,
            };
            self.end_attempt = forward && matches!(result, Ok(Change::None));
            match result {
                Ok(Change::Partial) => change = Change::Partial,
                Ok(_) => {}
                Err(error) => {
                    eprintln!("reader could not turn the page: {error}");
                    self.error = Some("Could not open that page".into());
                    change = Change::Partial;
                    break;
                }
            }
        }
        if previous == (self.chapter, self.page, self.scroll_top) && self.error.is_none() {
            change = Change::None;
        }
        if change == Change::Partial {
            self.turns.next_refresh = Instant::now() + PAGE_REFRESH_GAP;
            eprintln!(
                "reader page refresh chapter={} page={} input_count={count}",
                self.chapter + 1,
                self.page + 1
            );
        }
        self.schedule_prefetch();
        change
    }

    pub(crate) fn reflow(&mut self, settings: Settings) -> Result<()> {
        if (Settings {
            font_gamma_index: settings.font_gamma_index,
            text_contrast: settings.text_contrast,
            ..self.config.settings
        }) == settings
            && settings != self.config.settings
        {
            let previous = self.probe.settings;
            self.probe.settings = Settings {
                font_gamma_index: settings.font_gamma_index,
                text_contrast: settings.text_contrast,
                ..previous
            };
            let frame = match capture(&mut self.probe, self.chapter, self.page, self.scroll_top) {
                Ok(frame) => frame,
                Err(error) => {
                    self.probe.settings = previous;
                    return Err(error);
                }
            };
            self.config.settings = settings;
            self.frame = frame;
            self.adjacent.clear();
            self.schedule_prefetch();
            return Ok(());
        }
        if (Settings {
            invert_night_images: settings.invert_night_images,
            link_vertical_margins: settings.link_vertical_margins,
            ..self.config.settings
        }) == settings
        {
            self.config.settings.invert_night_images = settings.invert_night_images;
            self.probe.settings.invert_night_images = settings.invert_night_images;
            self.config.settings.link_vertical_margins = settings.link_vertical_margins;
            self.probe.settings.link_vertical_margins = settings.link_vertical_margins;
            return Ok(());
        }
        let anchor = self.anchor();
        let mut config = self.config.clone();
        config.settings = settings;
        *self = Self::restore(config, self.chapter, self.page, anchor.as_ref())?;
        Ok(())
    }

    pub(crate) fn set_fonts(&mut self, fonts: Vec<PathBuf>) -> Result<()> {
        let mut config = self.config.clone();
        config.fonts = fonts;
        *self = Self::restore(config, self.chapter, self.page, self.anchor().as_ref())?;
        Ok(())
    }

    pub(crate) fn set_scroll(&mut self, scroll: bool) -> Result<()> {
        if scroll && self.probe.is_vertical() {
            return Err("Vertical EPUB chapters use page turns".into());
        }
        let anchor = self.anchor();
        let mut config = self.config.clone();
        config.scroll = scroll;
        *self = Self::restore(config, self.chapter, self.page, anchor.as_ref())?;
        Ok(())
    }

    pub(crate) fn set_facing_pages(&mut self, facing_pages: bool) -> Result<()> {
        if self.config.facing_pages == facing_pages {
            return Ok(());
        }
        if facing_pages && (self.config.scroll || self.probe.is_vertical()) {
            return Err("Two pages require horizontal text in page-turn mode".into());
        }
        let mut config = self.config.clone();
        config.facing_pages = facing_pages;
        let next = Self::restore(config, self.chapter, self.page, self.anchor().as_ref())?;
        if facing_pages && !next.uses_facing_pages() {
            return Err("Two-page layout is unavailable for this chapter".into());
        }
        *self = next;
        Ok(())
    }

    pub(crate) fn has_page_error(&self) -> bool {
        self.error.is_some()
    }

    pub(crate) fn paged_frame_ready(&self) -> bool {
        self.scroll_top.is_none() && self.error.is_none()
    }

    pub(crate) fn last_visible_page(&self) -> usize {
        let span = frame_span(&self.probe);
        (first_frame_page(&self.probe, self.page) + span - 1).min(self.probe.page_count() - 1)
    }

    pub(crate) fn uses_facing_pages(&self) -> bool {
        frame_span(&self.probe) == 2
    }

    pub(crate) fn go_to_page(&mut self, chapter: usize, page: usize) -> Result<()> {
        let next = Self::restore(self.config.clone(), chapter, page, None)?;
        if page >= next.probe.page_count() {
            return Err("That page is outside the chapter".into());
        }
        *self = next;
        Ok(())
    }

    pub(crate) fn visible_end(&self) -> f32 {
        self.visible_columns().map(|(range, _)| range.end).fold(0.0, f32::max)
    }

    fn visible_range(&self) -> Range<f32> {
        self.scroll_top.map_or_else(
            || self.probe.page_range(self.page),
            |top| self.probe.scroll_range(top),
        )
    }

    pub(crate) fn visible_columns(&self) -> impl Iterator<Item = (Range<f32>, u32)> + '_ {
        frame_columns(&self.probe, self.page).map(|(page, x)| {
            let range = self.scroll_top.map_or_else(
                || self.probe.page_range(page),
                |top| self.probe.scroll_range(top),
            );
            (range, x)
        })
    }

    fn column_at(&self, x: f32) -> Option<(Range<f32>, f32)> {
        self.visible_columns().find_map(|(range, origin)| {
            let relative = x - origin as f32;
            (0.0..self.probe.settings.width as f32)
                .contains(&relative)
                .then_some((range, relative))
        })
    }

    pub(crate) fn word_at(&self, x: f32, y: f32) -> Option<TextSelection> {
        let (range, x) = self.column_at(x)?;
        self.probe.word_in(range, x, y)
    }

    /// Include the saved selection, the newly touched word and the intervening source.
    pub(crate) fn extend_saved_quote(
        &self,
        saved: &TextSelection,
        word: &TextSelection,
    ) -> Result<TextSelection> {
        let order = |anchor: &Anchor| -> Result<(usize, usize, usize)> {
            let chapter = self
                .probe
                .chapter_index(&anchor.chapter)
                .ok_or("The saved section is unavailable")?;
            Ok((chapter, anchor.block, anchor.offset))
        };
        let start = if order(&saved.start)? <= order(&word.start)? {
            &saved.start
        } else {
            &word.start
        };
        let end = if order(&saved.end)? >= order(&word.end)? {
            &saved.end
        } else {
            &word.end
        };
        self.anchored_quote(start, end)
    }

    pub(crate) fn anchored_quote(&self, start: &Anchor, end: &Anchor) -> Result<TextSelection> {
        let first = self.probe.chapter_index(&start.chapter)
            .ok_or("The saved section is unavailable")?;
        let last = self.probe.chapter_index(&end.chapter)
            .ok_or("The saved section is unavailable")?;
        if (first, start.block, start.offset) >= (last, end.block, end.offset) {
            return Err("The passage endpoints are out of order".into());
        }
        let mut text = String::new();
        for index in first..=last {
            let loaded;
            let probe = if index == self.chapter {
                &self.probe
            } else {
                loaded = self.config.chapter(index)?;
                &loaded
            };
            let part = probe.select_range(start, end);
            if let Some(part) = part {
                let extra = usize::from(!text.is_empty());
                if text.len() + extra + part.text.len() > crate::library::MAX_HIGHLIGHT_TEXT_BYTES {
                    return Err("This selection exceeds the saved highlight size limit".into());
                }
                if extra != 0 {
                    text.push('\n');
                }
                text.push_str(&part.text);
            } else if index == first || index == last {
                return Err("The saved selection no longer resolves in this edition".into());
            }
        }
        Ok(TextSelection {
            start: start.clone(),
            end: end.clone(),
            text,
        })
    }

    pub(crate) fn highlight_rects(&self, selection: &TextSelection) -> Vec<HighlightRect> {
        self.visible_columns()
            .flat_map(|(range, x)| {
                self.probe
                    .selection_rects_in(range, selection)
                    .into_iter()
                    .map(move |mut rect| {
                        rect.x += x as i32;
                        rect
                    })
            })
            .collect()
    }

    pub(crate) fn note_reference_at(
        &self,
        x: f32,
        y: f32,
    ) -> Option<(crate::footnotes::ReferenceContext, TextSelection)> {
        let (range, x) = self.column_at(x)?;
        let (key, offset) = self.probe.source_at(range, x, y)?;
        let block = self.probe.source.blocks.iter().find(|b| b.key == key)?;
        let link = block.links.iter().find(|l| l.bytes.contains(&offset))?;
        let linked = &block.text[link.bytes.clone()];
        let label = linked.trim();
        let first = link.bytes.start + linked.len() - linked.trim_start().len();
        let selection = self.probe.select_range(
            &self.probe.anchor(key, first),
            &self.probe.anchor(key, first + label.len()),
        )?;
        Some((
            crate::footnotes::ReferenceContext {
                kind: link.note_reference,
                label: label.chars().take(64).collect(),
                in_prose: block.text[..link.bytes.start]
                    .chars()
                    .chain(block.text[link.bytes.end..].chars())
                    .any(char::is_alphanumeric),
                source_block: key,
            },
            selection,
        ))
    }

    pub(crate) fn link_at(&self, x: f32, y: f32) -> Option<Destination> {
        let (range, x) = self.column_at(x)?;
        self.probe.link_in(range, x, y)
    }

    pub(crate) fn illustration_at(&self, x: f32, y: f32) -> Option<&str> {
        let (range, x) = self.column_at(x)?;
        self.probe.illustration_in(range, x, y)
    }

    pub(crate) fn open_illustration(&self, package_path: &str) -> Result<crate::comic::ImagePage> {
        Ok(with_storage(|| {
            self.probe
                .open_illustration(&self.config.book, package_path)
                .map_err(io::Error::other)
        })?)
    }

    pub(crate) fn search_boundary(
        &self,
        direction: crate::search::SearchDirection,
    ) -> Option<Anchor> {
        let previous = direction == crate::search::SearchDirection::Previous;
        let mut edges = self.visible_columns().filter_map(|(range, _)| {
            let mut lines = self.probe.lines.iter().filter(|line| {
                !line.bytes.is_empty() && line.bottom > range.start && line.top < range.end
            });
            let line = if previous {
                lines.next_back()
            } else {
                lines.next()
            }?;
            Some(self.probe.anchor(
                line.block,
                if previous {
                    line.bytes.end
                } else {
                    line.bytes.start
                },
            ))
        });
        if previous { edges.last() } else { edges.next() }.or_else(|| self.anchor())
    }

    pub(crate) fn visible_text(&self) -> String {
        let mut text = String::new();
        for (range, _) in self.visible_columns() {
            let mut visible = self.probe.lines.iter().filter(|line| {
                !line.bytes.is_empty() && line.bottom > range.start && line.top < range.end
            });
            let Some(first) = visible.next() else { continue };
            let last = visible.last().unwrap_or(first);
            let start = self.probe.anchor(first.block, first.bytes.start);
            let end = self.probe.anchor(last.block, last.bytes.end);
            if let Some(selection) = self.probe.select_range(&start, &end) {
                if !text.is_empty() { text.push('\n'); }
                text.push_str(&selection.text);
            }
        }
        text
    }

    pub(crate) fn links(&self) -> Vec<(String, Destination)> {
        let mut columns = self.visible_columns().map(|(range, _)| range);
        let Some(first) = columns.next() else {
            return Vec::new();
        };
        let end = columns.last().map_or(first.end, |last| last.end);
        self.probe.links_in(first.start..end)
    }

    pub(crate) fn anchor(&self) -> Option<Anchor> {
        self.focus_anchor
            .clone()
            .or_else(|| self.probe.anchor_in(self.visible_range()))
    }

    fn scroll(&mut self, top: f32, distance: f32) -> Result<Change> {
        if distance == 0.0 {
            return Ok(Change::None);
        }
        let next = (top + distance).clamp(0.0, self.probe.scroll_end());
        if next != top {
            let page = self.probe.page_at(next);
            self.frame = capture(&mut self.probe, self.chapter, page, Some(next))?;
            self.page = page;
            self.scroll_top = Some(next);
        } else {
            let direction = if distance > 0.0 { 1 } else { -1 };
            let Some(chapter) = self
                .chapter
                .checked_add_signed(direction)
                .filter(|&chapter| chapter < self.probe.chapter_count())
            else {
                return Ok(Change::None);
            };
            let probe = self.config.chapter(chapter)?;
            self.land_at_chapter_edge(probe, chapter, direction)?;
        }
        self.focus_anchor = None;
        self.error = None;
        Ok(Change::Partial)
    }

    fn land_at_chapter_edge(
        &mut self,
        mut probe: Probe,
        chapter: usize,
        direction: isize,
    ) -> Result<()> {
        let page = if direction < 0 {
            first_frame_page(&probe, probe.page_count() - 1)
        } else {
            0
        };
        let top = (self.config.scroll && !probe.is_vertical()).then(|| {
            if direction < 0 {
                probe.scroll_end()
            } else {
                0.0
            }
        });
        let page = top.map_or(page, |top| probe.page_at(top));
        self.frame = capture(&mut probe, chapter, page, top)?;
        self.probe = probe;
        self.chapter = chapter;
        self.page = page;
        self.scroll_top = top;
        self.adjacent.clear();
        Ok(())
    }

    pub(crate) fn jump(&mut self, chapter: usize, fragment: Option<&str>) -> Result<()> {
        let candidate = self.config.chapter(chapter)?;
        let anchor = match fragment {
            Some(name) => candidate
                .fragment(name)
                .filter(|anchor| candidate.page_for(anchor).is_some())
                .map(Some)
                .ok_or("That location is unavailable")?,
            None => None,
        };
        self.use_chapter(candidate, chapter, anchor.as_ref(), 0)
    }

    pub(crate) fn jump_to_anchor(&mut self, chapter: usize, anchor: &Anchor) -> Result<()> {
        let candidate = self.config.chapter(chapter)?;
        if candidate.page_for(anchor).is_none() {
            return Err("That location is unavailable".into());
        }
        self.use_chapter(candidate, chapter, Some(anchor), 0)
    }

    pub(crate) fn jump_progress(&mut self, fraction: f64) -> Result<()> {
        let (chapter, in_chapter, probe) = with_storage(|| {
            let mut book = Book::open(&self.config.book).map_err(io::Error::other)?;
            let (chapter, in_chapter) = book.progress_target(fraction).map_err(io::Error::other)?;
            let probe = self
                .config
                .layout_chapter(book, chapter)
                .map_err(io::Error::other)?;
            Ok((chapter, in_chapter, probe))
        })?;
        let anchor = (0.0 < in_chapter && in_chapter < 1.0)
            .then(|| probe.proportional_anchor(in_chapter))
            .flatten();
        let fallback = ((probe.page_count() - 1) as f64 * in_chapter).round() as usize;
        self.use_chapter(probe, chapter, anchor.as_ref(), fallback)
    }

    fn use_chapter(
        &mut self,
        mut candidate: Probe,
        chapter: usize,
        anchor: Option<&Anchor>,
        fallback_page: usize,
    ) -> Result<()> {
        let focused_page = anchor.and_then(|anchor| candidate.page_for(anchor));
        let page = focused_page.unwrap_or(fallback_page);
        let top = (self.config.scroll && !candidate.is_vertical()).then(|| {
            anchor
                .and_then(|anchor| candidate.anchor_top(anchor))
                .unwrap_or_else(|| candidate.page_range(page).start)
                .clamp(0.0, candidate.scroll_end())
        });
        let frame = capture(&mut candidate, chapter, page, top)?;
        self.probe = candidate;
        self.frame = frame;
        self.chapter = chapter;
        self.page = top.map_or(page, |top| self.probe.page_at(top));
        self.scroll_top = top;
        self.focus_anchor = anchor.filter(|_| focused_page.is_some()).cloned();
        self.error = None;
        self.adjacent.clear();
        self.turns = PageTurns::new(Instant::now());
        self.schedule_prefetch();
        Ok(())
    }

    fn prefetch_page(&self) -> Option<usize> {
        let span = frame_span(&self.probe);
        let page = first_frame_page(&self.probe, self.page);
        [Some(page + span), page.checked_sub(span)]
            .into_iter()
            .flatten()
            .find(|page| {
                *page < self.probe.page_count()
                    && !self.adjacent.iter().any(|(cached, _)| cached == page)
            })
    }

    fn schedule_prefetch(&mut self) {
        self.prefetch = if self.scroll_top.is_none() {
            self.prefetch_page()
                .map(|_| Instant::now() + PREFETCH_DELAY)
        } else {
            None
        };
    }

    pub(crate) fn pause_turns(&mut self) {
        self.turns.pending.clear();
        self.prefetch = None;
        self.end_attempt = false;
    }

    pub(crate) fn take_end_attempt(&mut self) -> bool {
        std::mem::take(&mut self.end_attempt)
    }

    pub(crate) fn page_status(&self) -> String {
        let first = first_frame_page(&self.probe, self.page) + 1;
        let count = self.probe.page_count();
        let last = self.last_visible_page() + 1;
        let pages = if first == last {
            format!("Page {first} of {count}")
        } else {
            format!("Pages {first}–{last} of {count}")
        };
        format!(
            "Chapter {} of {}  ·  {pages}",
            self.chapter + 1,
            self.probe.chapter_count()
        )
    }

    pub(crate) fn draw_page(
        &self,
        canvas: &mut Canvas,
        action: Option<&str>,
        hidden_status: bool,
        notice: Option<&str>,
        highlights: &[(HighlightRect, crate::highlight_style::Paint)],
        selection: &[HighlightRect],
        paint: PagePaint,
    ) {
        let page = self.marked_frame(highlights, selection, &paint);
        let Some(full) =
            draw_document_and_footer(canvas, page, paint, hidden_status && action.is_some())
        else {
            return;
        };
        let Some(action) = action else { return };
        let progress = self.page_status();
        canvas.text(
            Rect::new(38, PAGE_HEIGHT as i32 + 12, DONE.x - 60, 48),
            self.error.as_deref().or(notice).unwrap_or(&progress),
            CAPTION,
        );
        if full {
            canvas.button(DONE, action, false, false);
        }
    }

    fn marked_frame(
        &self,
        highlights: &[(HighlightRect, crate::highlight_style::Paint)],
        selection: &[HighlightRect],
        paint: &PagePaint,
    ) -> GrayImage {
        let mut page = self.frame.clone();
        if paint.inverted && !self.config.settings.invert_night_images {
            for (range, x) in self.visible_columns() {
                preinvert_images(&self.probe, range, x, page.width, &mut page.pixels);
            }
        }
        for &(rect, ink) in highlights {
            draw_highlight_rect(&mut page, rect, ink);
        }
        for &rect in selection {
            draw_highlight_rect(
                &mut page,
                rect,
                crate::highlight_style::Paint::Temporary(176),
            );
        }
        if paint.visible != (0..self.frame.height) && !self.probe.is_vertical() {
            self.clear_lines_cut_by_tools(&mut page, &paint.visible);
        }
        page
    }

    fn clear_lines_cut_by_tools(&self, page: &mut GrayImage, visible: &Range<u32>) {
        for (column, x) in self.visible_columns() {
            let offset = self.probe.settings.top_margin() as f32 - column.start;
            let (mut start, mut end) = (visible.start, visible.end);
            // The two pages can have different line breaks beneath the bars.
            for line in self
                .probe
                .lines
                .iter()
                .filter(|line| !line.bytes.is_empty())
            {
                let top = line.top + offset;
                let bottom = line.bottom + offset;
                if top < visible.start as f32 && bottom > visible.start as f32 {
                    start = start.max((bottom.ceil() as u32).min(visible.end));
                }
                if top < visible.end as f32 && bottom > visible.end as f32 {
                    end = end.min((top.floor() as u32).max(visible.start));
                }
            }
            for rows in [visible.start..start, end..visible.end] {
                for row in rows {
                    let start = (row * WIDTH + x) as usize;
                    let end = start + self.probe.settings.width as usize;
                    page.pixels[start..end].fill(255);
                }
            }
        }
    }
}

fn draw_highlight_rect(
    page: &mut GrayImage,
    rect: HighlightRect,
    ink: crate::highlight_style::Paint,
) {
    let (top, height) = ink
        .line_band(rect.y, rect.height, page.width, false)
        .unwrap_or((rect.y, rect.height));
    let x0 = rect.x.clamp(0, page.width as i32) as usize;
    let x1 = (rect.x + rect.width).clamp(0, page.width as i32) as usize;
    let y0 = top.clamp(0, page.height as i32) as usize;
    let y1 = (top + height).clamp(0, page.height as i32) as usize;
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    for row in page
        .pixels
        .chunks_exact_mut(page.width as usize)
        .skip(y0)
        .take(y1 - y0)
    {
        ink.draw_row(&mut row[x0..x1]);
    }
}

/// Paint the visible document and its footer background. Return whether the
/// caller should draw the full set of regular status controls.
pub(crate) fn draw_document_and_footer(
    canvas: &mut Canvas,
    page: GrayImage,
    mut paint: PagePaint,
    hidden_status: bool,
) -> Option<bool> {
    let height = page.height;
    let full = paint.visible == (0..height);
    if paint.visible.end != height {
        draw_page_picture(canvas, page, paint);
        return None;
    }
    if height == HEIGHT && hidden_status {
        if full {
            draw_page_around_footer_buttons(canvas, page, paint);
            for (rect, title) in [(SHOW_STATUS, "Status"), (DONE, "Exit")] {
                canvas.fill(rect, 255);
                canvas.button(rect, title, false, false);
            }
        } else {
            draw_page_picture(canvas, page, paint);
        }
        return None;
    }
    if height == HEIGHT {
        paint.visible.start = paint.visible.start.min(PAGE_HEIGHT);
        paint.visible.end = PAGE_HEIGHT;
        canvas.fill(
            Rect::new(0, PAGE_HEIGHT as i32, WIDTH as i32, FOOTER_HEIGHT as i32),
            255,
        );
    }
    draw_page_picture(canvas, page, paint);
    canvas.fill(Rect::new(0, PAGE_HEIGHT as i32, WIDTH as i32, 1), HAIRLINE);
    Some(full)
}

// Canvas paints document bitmaps after its text and buttons. Keep the two controls'
// rectangles out of the document image while retaining all surrounding page pixels.
fn draw_page_around_footer_buttons(canvas: &mut Canvas, page: GrayImage, paint: PagePaint) {
    let (rect, mut page) = paint.apply(page);
    debug_assert_eq!(rect.y, 0);
    let bottom = SHOW_STATUS.y + SHOW_STATUS.h;
    let width = page.width as i32;
    let strips = [
        Rect::new(0, SHOW_STATUS.y, SHOW_STATUS.x, SHOW_STATUS.h),
        Rect::new(
            SHOW_STATUS.x + SHOW_STATUS.w,
            SHOW_STATUS.y,
            DONE.x - (SHOW_STATUS.x + SHOW_STATUS.w),
            SHOW_STATUS.h,
        ),
        Rect::new(DONE.x + DONE.w, DONE.y, width - (DONE.x + DONE.w), DONE.h),
        Rect::new(0, bottom, width, page.height as i32 - bottom),
    ];
    for strip in strips {
        let mut pixels = Vec::with_capacity((strip.w * strip.h) as usize);
        for y in strip.y..strip.y + strip.h {
            let start = (y * width + strip.x) as usize;
            pixels.extend_from_slice(&page.pixels[start..start + strip.w as usize]);
        }
        canvas.picture(
            strip,
            GrayImage {
                width: strip.w as u32,
                height: strip.h as u32,
                pixels,
            },
        );
    }
    page.height = SHOW_STATUS.y as u32;
    page.pixels.truncate((page.width * page.height) as usize);
    canvas.picture(Rect::new(0, 0, width, page.height as i32), page);
}

pub(crate) fn draw_page_picture(canvas: &mut Canvas, page: GrayImage, paint: PagePaint) {
    let (rect, page) = paint.apply(page);
    canvas.picture(rect, page);
}

impl App for Reading {
    fn draw(&self, canvas: &mut Canvas) {
        self.draw_page(
            canvas,
            Some("Exit"),
            false,
            None,
            &[],
            &[],
            PagePaint {
                visible: 0..self.frame.height,
                inverted: false,
            },
        );
    }

    fn input(&mut self, input: Input) -> Change {
        let advance = if self.probe.pages_right_to_left() {
            -1
        } else {
            1
        };
        let direction = match input {
            Input::Sleep => {
                self.turns.pending.clear();
                self.prefetch = None;
                return Change::None;
            }
            Input::Wake => {
                self.turns = PageTurns::new(Instant::now());
                self.schedule_prefetch();
                return Change::None;
            }
            Input::Tap(x, y) if DONE.contains(x, y) => return Change::Exit,
            Input::PageForward => 1,
            Input::PageBack => -1,
            Input::Swipe { dx, dy, .. } if self.scroll_top.is_some() && dy.abs() >= dx.abs() => {
                self.turns.pending.push(ReadingMotion::Pixels(-dy));
                return self.present_page_turns(Instant::now());
            }
            Input::Swipe { dx, dy, .. } if dx.abs() > dy.abs() => {
                if dx < 0.0 {
                    advance
                } else {
                    -advance
                }
            }
            Input::Tap(x, y) if y < self.frame.height as f32 && x < WIDTH as f32 / 3.0 => -advance,
            Input::Tap(x, y) if y < self.frame.height as f32 && x > WIDTH as f32 * 2.0 / 3.0 => {
                advance
            }
            _ => return Change::None,
        };
        self.prefetch = None;
        self.turns.pending.push(ReadingMotion::Page(direction));
        self.present_page_turns(Instant::now())
    }

    fn deadline(&self) -> Option<Instant> {
        self.turns.deadline().or(self.prefetch)
    }

    fn tick(&mut self, now: Instant) -> Change {
        if self.turns.deadline().is_some() {
            return self.present_page_turns(now);
        }
        if self.prefetch.is_some_and(|deadline| now >= deadline) {
            self.prefetch = None;
            if let Some(page) = self.prefetch_page()
                && let Ok(frame) = capture(&mut self.probe, self.chapter, page, None)
            {
                self.adjacent.push((page, frame));
                self.schedule_prefetch();
            }
        }
        Change::None
    }

    fn storage_returned(&mut self) -> io::Result<()> {
        self.turns = PageTurns::new(Instant::now());
        self.schedule_prefetch();
        Ok(())
    }
}

/// Counteract the final page inversion just inside each visible illustration.
pub(crate) fn preinvert_images(
    probe: &Probe,
    range: Range<f32>,
    column: u32,
    stride: u32,
    pixels: &mut [u8],
) -> bool {
    let crate::Renderer::Parley(layout) = &probe.renderer else {
        return false;
    };
    let mut changed = false;
    for (_, cols, rows) in layout.visible_illustrations(range, probe.settings) {
        for row in rows {
            let start = row * stride as usize + column as usize + cols.start;
            for pixel in &mut pixels[start..start + cols.len()] {
                *pixel = 255 - *pixel;
            }
        }
        changed = true;
    }
    changed
}

pub(crate) struct PageThumbnail {
    light: GrayImage,
    dark: Option<GrayImage>,
}

impl PageThumbnail {
    pub(crate) fn plain(light: GrayImage) -> Self {
        Self { light, dark: None }
    }

    pub(crate) fn for_theme(&self, dark: bool) -> GrayImage {
        if !dark {
            return self.light.clone();
        }
        if let Some(image) = &self.dark {
            return image.clone();
        }
        let mut image = self.light.clone();
        for pixel in &mut image.pixels {
            *pixel = 255 - *pixel;
        }
        image
    }
}

pub(crate) fn page_thumbnail(probe: &mut Probe, page: usize) -> Result<PageThumbnail> {
    let full = probe.capture(page)?;
    let settings = probe.settings;
    let mut image = image::GrayImage::from_raw(settings.width, settings.height, full)
        .ok_or("The page has incorrect dimensions")?;
    let height = 280;
    let width = settings.width * height / settings.height;
    let shrink = |image: &image::GrayImage| GrayImage {
        width,
        height,
        pixels: image::imageops::thumbnail(image, width, height).into_raw(),
    };
    let mut thumbnail = PageThumbnail::plain(shrink(&image));
    // Only image-bearing previews need two rasters when the system theme changes.
    if !settings.invert_night_images
        && preinvert_images(
            probe,
            probe.page_range(page),
            0,
            settings.width,
            image.as_mut(),
        )
    {
        image::imageops::invert(&mut image);
        thumbnail.dark = Some(shrink(&image));
    }
    Ok(thumbnail)
}

fn frame_span(probe: &Probe) -> usize {
    if probe.settings.width == FACING_PAGE_WIDTH {
        2
    } else {
        1
    }
}

fn first_frame_page(probe: &Probe, page: usize) -> usize {
    let span = frame_span(probe);
    page / span * span
}

fn frame_columns(probe: &Probe, page: usize) -> impl Iterator<Item = (usize, u32)> + use<> {
    let first = first_frame_page(probe, page);
    let span = frame_span(probe);
    let end = (first + span).min(probe.page_count());
    let rtl = probe.pages_right_to_left();
    // Keep logical leaf order for text ranges and links. Only the physical origin changes.
    (first..end).enumerate().map(move |(index, page)| {
        let slot = if rtl { span - 1 - index } else { index };
        (page, slot as u32 * (FACING_PAGE_WIDTH + FACING_GUTTER))
    })
}

fn capture(probe: &mut Probe, chapter: usize, page: usize, top: Option<f32>) -> Result<GrayImage> {
    let start = Instant::now();
    let pixels = match top {
        Some(top) => probe.capture_range(probe.scroll_range(top))?,
        None if frame_span(probe) == 2 => capture_facing_pages(probe, page)?,
        None => probe.capture(page)?,
    };
    eprintln!(
        "reader chapter={} page={} paint_us={}",
        chapter + 1,
        page + 1,
        start.elapsed().as_micros(),
    );
    let height = probe.settings.height;
    if pixels.len() != (WIDTH * height) as usize {
        return Err("The page has incorrect dimensions".into());
    }
    Ok(GrayImage {
        width: WIDTH,
        height,
        pixels,
    })
}

fn capture_facing_pages(probe: &mut Probe, page: usize) -> Result<Vec<u8>> {
    if page >= probe.page_count() {
        return Err("That page is unavailable".into());
    }
    let width = FACING_PAGE_WIDTH as usize;
    let height = probe.settings.height as usize;
    let mut frame = vec![255; WIDTH as usize * height];
    for (column, x) in frame_columns(probe, page) {
        let pixels = probe.capture(column)?;
        if pixels.len() != width * height {
            return Err("A facing page has incorrect dimensions".into());
        }
        for (row, source) in frame
            .chunks_exact_mut(WIDTH as usize)
            .zip(pixels.chunks_exact(width))
        {
            row[x as usize..x as usize + width].copy_from_slice(source);
        }
    }
    Ok(frame)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn example(book: &str, font_size: f32, facing_pages: bool) -> Reading {
        Reading::open(BookConfig {
            user_hyphenation: Default::default(),
            chapter_offsets: Default::default(),
            engine: Engine::Parley,
            book: crate::test_fixture(book),
            fonts: vec![crate::test_fixture("fonts/Silkscreen-Regular.ttf")],
            settings: Settings {
                width: WIDTH,
                height: PAGE_HEIGHT,
                font_size,
                ..Settings::default()
            },
            scroll: false,
            facing_pages,
        })
        .unwrap()
    }

    #[test]
    fn cover_and_blank_spine_pages_turn_both_ways_and_restore_their_location() {
        use std::io::{Cursor, Read, Write};

        let original = std::fs::File::open(crate::test_fixture("field-notes-epub3.epub")).unwrap();
        let mut archive = zip::ZipArchive::new(original).unwrap();
        let path = std::env::temp_dir().join(format!("reader-spine-cover-{}.epub", std::process::id()));
        let mut output = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        let cover = r#"<html><body id="cover"><div><svg xmlns="http://www.w3.org/2000/svg"
            xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 500 2300">
            <image width="500" height="2300" xlink:href="../images/shore.png"/></svg></div></body></html>"#;
        let blank = "<html><body><p> </p></body></html>";
        let chapter = r#"<html><body><div id="first">Opening sentence.</div>
            <img src="../images/shore.png"/><p id="end">Last words.</p></body></html>"#;
        let mut photo = Cursor::new(Vec::new());
        let art = image::GrayImage::from_fn(500, 2300, |x, _| image::Luma([if x < 250 { 40 } else { 180 }]));
        image::DynamicImage::ImageLuma8(art).write_to(&mut photo, image::ImageFormat::Png).unwrap();
        for index in 0..archive.len() {
            let mut file = archive.by_index(index).unwrap();
            output.start_file(file.name(), opts).unwrap();
            match file.name() {
                "EPUB/package.opf" => {
                    let mut xml = String::new();
                    file.read_to_string(&mut xml).unwrap();
                    let xml = xml.replace("</manifest>", r#"<item id="blank" href="blank.xhtml" media-type="application/xhtml+xml"/></manifest>"#)
                        .replace(r#"<itemref idref="resource-2"/>"#, r#"<itemref idref="blank"/><itemref idref="resource-2"/>"#);
                    output.write_all(xml.as_bytes()).unwrap();
                }
                "EPUB/chapters/shore.xhtml" => output.write_all(cover.as_bytes()).unwrap(),
                "EPUB/chapters/dusk.xhtml" => output.write_all(chapter.as_bytes()).unwrap(),
                "EPUB/images/shore.png" => output.write_all(photo.get_ref()).unwrap(),
                _ => { std::io::copy(&mut file, &mut output).unwrap(); }
            }
        }
        output.start_file("EPUB/blank.xhtml", opts).unwrap();
        output.write_all(blank.as_bytes()).unwrap();
        output.finish().unwrap();

        let mut config = example("field-notes-epub3.epub", 28.0, false).config;
        config.book = path.clone();
        config.chapter_offsets = Default::default();
        let mut reading = Reading::open(config.clone()).unwrap();
        assert_eq!(reading.probe.page_count(), 1);
        assert!(reading.frame.pixels.iter().filter(|&&p| p == 40).count() > 1000);
        let cover_anchor = reading.anchor().unwrap();
        assert_eq!(reading.probe.fragment("cover"), Some(cover_anchor.clone()));
        assert_eq!(reading.probe.page_for(&cover_anchor), Some(0));
        assert_eq!(reading.open_illustration("EPUB/images/shore.png").unwrap().page_size(), (500.0, 2300.0));
        reading.turn(1).unwrap();
        assert_eq!((reading.chapter, reading.page), (1, 0));
        assert!(reading.frame.pixels.iter().all(|&p| p == 255));
        let mut restored_blank = Reading::restore(config.clone(), 1, 0, reading.anchor().as_ref()).unwrap();
        restored_blank.turn(1).unwrap();
        assert_eq!(restored_blank.chapter, 2);
        let end = restored_blank.probe.fragment("end").unwrap();
        restored_blank.jump_to_anchor(2, &end).unwrap();
        assert_eq!(restored_blank.anchor(), Some(end.clone()));
        let mut restored = Reading::restore(config, 2, 0, Some(&end)).unwrap();
        assert_eq!(restored.anchor(), Some(end));
        restored.go_to_page(2, 0).unwrap();
        restored.turn(-1).unwrap();
        assert_eq!(restored.chapter, 1);
        restored.turn(-1).unwrap();
        assert_eq!(restored.anchor(), Some(cover_anchor));
        std::fs::remove_file(path).unwrap();
    }

    fn first_hit<T>(mut hit: impl FnMut(f32, f32) -> Option<T>) -> Option<(f32, f32, T)> {
        for y in (0..PAGE_HEIGHT).step_by(9) {
            for x in (0..FACING_PAGE_WIDTH).step_by(9) {
                if let Some(result) = hit(x as f32, y as f32) {
                    return Some((x as f32, y as f32, result));
                }
            }
        }
        None
    }

    #[test]
    fn facing_pages_apply_independent_margins_to_each_leaf_and_keep_the_gutter_clean() {
        let mut reading = example("long-chapter.epub", 42.0, true);
        let settings = reading.config.settings;
        assert!(reading.uses_facing_pages() && reading.last_visible_page() == 1);
        let before = reading.frame.pixels.clone();
        reading
            .reflow(Settings {
                margin_left: Some(66),
                margin_right: Some(42),
                ..settings
            })
            .unwrap();
        let after = &reading.frame.pixels;
        let gutter = FACING_GUTTER as usize;
        let width = FACING_PAGE_WIDTH as usize;
        for (a, c) in before
            .chunks_exact(WIDTH as usize)
            .zip(after.chunks_exact(WIDTH as usize))
        {
            assert!(c[width..width + gutter].iter().all(|&v| v == 255));
            for base in [0, width + gutter] {
                assert!(a[base..base + width - 12] == c[base + 12..base + width]);
            }
        }
    }

    #[test]
    fn facing_pages_rasterize_and_select_both_current_leaves() {
        let mut reading = example("long-chapter.epub", 42.0, true);
        assert_eq!(reading.config.settings.width, WIDTH);
        assert_eq!(reading.probe.settings.width, FACING_PAGE_WIDTH);
        assert_eq!(reading.last_visible_page(), 1);
        assert_eq!(reading.frame.pixels.len(), (WIDTH * PAGE_HEIGHT) as usize);

        let width = FACING_PAGE_WIDTH as usize;
        let right = width + FACING_GUTTER as usize;
        let left_pixels = reading.probe.capture(0).unwrap();
        let right_pixels = reading.probe.capture(1).unwrap();
        assert_ne!(left_pixels, right_pixels);
        assert!(right_pixels.iter().any(|&pixel| pixel < 255));
        for (y, row) in reading
            .frame
            .pixels
            .chunks_exact(WIDTH as usize)
            .enumerate()
        {
            let offset = y * width;
            assert_eq!(&row[..width], &left_pixels[offset..offset + width]);
            assert!(row[width..right].iter().all(|&pixel| pixel == 255));
            assert_eq!(&row[right..], &right_pixels[offset..offset + width]);
        }

        let (left_x, left_y, left_word) = first_hit(|x, y| reading.probe.word_at(0, x, y)).unwrap();
        let (right_x, right_y, right_word) =
            first_hit(|x, y| reading.probe.word_at(1, x, y)).unwrap();
        assert_eq!(reading.word_at(left_x, left_y), Some(left_word.clone()));
        assert_eq!(
            reading.word_at(right_x + right as f32, right_y),
            Some(right_word.clone())
        );
        assert_eq!(reading.word_at(WIDTH as f32 / 2.0, right_y), None);
        let selection = reading
            .probe
            .select_range(&left_word.start, &right_word.end)
            .unwrap();
        let rects = reading.highlight_rects(&selection);
        assert!(rects.iter().any(|rect| rect.x + rect.width <= width as i32));
        assert!(rects.iter().any(|rect| rect.x >= right as i32));
        assert!(rects.iter().all(|rect| rect.x + rect.width <= WIDTH as i32));
        let thumbnail = page_thumbnail(&mut reading.probe, 1).unwrap();
        assert_eq!(thumbnail.light.width, FACING_PAGE_WIDTH * 280 / PAGE_HEIGHT);
    }

    #[test]
    fn contrast_repaints_both_facing_leaves_and_scroll_without_replacing_their_geometry() {
        let mut reading = example("long-chapter.epub", 42.0, true);
        let (x, y, word) = first_hit(|x, y| reading.probe.word_at(1, x, y)).unwrap();
        let right = (FACING_PAGE_WIDTH + FACING_GUTTER) as f32;
        let anchor = reading.anchor();
        let lines = reading.probe.lines.as_ptr();
        let before = reading.frame.pixels.clone();
        let pages = reading.probe.page_count();
        reading.reflow(Settings {font_gamma_index:25, ..reading.config.settings}).unwrap();
        assert_eq!(reading.probe.settings.width, FACING_PAGE_WIDTH);
        assert_eq!(reading.config.settings.width, WIDTH);
        assert_eq!(reading.last_visible_page(), 1);
        assert_eq!(reading.probe.page_count(), pages);
        assert_eq!(reading.probe.lines.as_ptr(), lines);
        assert_eq!(reading.anchor(), anchor);
        assert_eq!(reading.word_at(x + right, y), Some(word));
        assert_ne!(reading.frame.pixels, before);
        reading.reflow(Settings {font_gamma_index:15, ..reading.config.settings}).unwrap();
        assert_eq!(reading.frame.pixels, before);

        reading.set_scroll(true).unwrap();
        reading.scroll(reading.scroll_top.unwrap(), 185.0).unwrap();
        let top = reading.scroll_top;
        let anchor = reading.anchor();
        let lines = reading.probe.lines.as_ptr();
        let before = reading.frame.pixels.clone();
        reading.reflow(Settings {font_gamma_index:25, ..reading.config.settings}).unwrap();
        assert_eq!(reading.scroll_top, top);
        assert_eq!(reading.anchor(), anchor);
        assert_eq!(reading.probe.lines.as_ptr(), lines);
        assert_ne!(reading.frame.pixels, before);
        reading.reflow(Settings {font_gamma_index:15, ..reading.config.settings}).unwrap();
        assert_eq!(reading.frame.pixels, before);
    }

    #[test]
    fn links_in_the_right_leaf_keep_their_original_destination() {
        let mut reading = example("field-notes-epub3.epub", 84.0, true);
        let (page, (x, y, destination)) = (1..reading.probe.page_count())
            .step_by(2)
            .find_map(|page| {
                first_hit(|x, y| reading.probe.link_at(page, x, y)).map(|hit| (page, hit))
            })
            .expect("The linked EPUB fixture has a link on an even-numbered leaf");
        reading.go_to_page(0, page - 1).unwrap();
        assert_eq!(
            reading.link_at(x + (FACING_PAGE_WIDTH + FACING_GUTTER) as f32, y),
            Some(destination.clone())
        );
        assert!(reading.links().iter().any(|(_, link)| *link == destination));
        assert_eq!(reading.link_at(WIDTH as f32 / 2.0, y), None);
    }

    #[test]
    fn facing_page_presses_step_by_a_spread_and_focus_survives_layout_changes() {
        let mut reading = example("long-chapter.epub", 42.0, true);
        let now = Instant::now();
        reading.turns.next_refresh = now + PAGE_REFRESH_GAP;
        for input in [
            Input::PageForward,
            Input::PageForward,
            Input::PageBack,
            Input::PageForward,
        ] {
            assert_eq!(reading.input(input), Change::None);
        }
        assert_eq!(reading.tick(now + PAGE_REFRESH_GAP), Change::Partial);
        assert_eq!(reading.page, 4);
        let anchor = reading
            .probe
            .anchor_in(reading.probe.page_range(5))
            .unwrap();
        reading.jump_to_anchor(0, &anchor).unwrap();
        assert_eq!(reading.page, 5);
        assert_eq!(reading.anchor(), Some(anchor.clone()));
        reading.set_facing_pages(false).unwrap();
        assert_eq!(reading.probe.settings.width, WIDTH);
        assert_eq!(reading.anchor(), Some(anchor.clone()));
        reading.set_facing_pages(true).unwrap();
        assert_eq!(reading.anchor(), Some(anchor.clone()));
        reading.set_scroll(true).unwrap();
        assert!(reading.config.facing_pages);
        assert_eq!(reading.probe.settings.width, WIDTH);
        assert_eq!(reading.anchor(), Some(anchor.clone()));
        reading.set_scroll(false).unwrap();
        assert_eq!(reading.probe.settings.width, FACING_PAGE_WIDTH);
        assert_eq!(reading.anchor(), Some(anchor));
    }

    #[test]
    fn night_pages_preserve_illustrations_on_both_sized_views_and_in_skim() {
        fn find_image(reading: &Reading) -> (usize, Range<usize>, Range<usize>) {
            let probe = &reading.probe;
            let crate::Renderer::Parley(layout) = &probe.renderer else {
                unreachable!()
            };
            (0..probe.page_count())
                .find_map(|page| {
                    layout
                        .visible_illustrations(probe.page_range(page), probe.settings)
                        .next()
                        .map(|(_, x, y)| (page, x, y))
                })
                .unwrap()
        }

        let render_night = |reading: &Reading| {
            let paint = PagePaint::full(true);
            let frame = reading.marked_frame(&[], &[], &paint);
            paint.apply(frame).1.pixels
        };
        let mut reading = example("field-notes-epub3.epub", 60.0, false);
        for facing in [true, false] {
            reading.set_facing_pages(facing).unwrap();
            let (page, cols, rows) = find_image(&reading);
            reading.go_to_page(0, page).unwrap();
            let (_, origin) = frame_columns(&reading.probe, page)
                .find(|(index, _)| *index == page)
                .unwrap();
            let frame = render_night(&reading);
            assert_eq!(frame[0], 0);
            let pos = rows.start * WIDTH as usize + origin as usize + cols.start;
            assert_eq!(
                &frame[pos..pos + cols.len()],
                &reading.frame.pixels[pos..pos + cols.len()]
            );
            let cutoff = (rows.start + rows.len() / 2) as u32;
            let mut clipped = reading.frame.clone();
            reading.clear_lines_cut_by_tools(&mut clipped, &(cutoff..PAGE_HEIGHT));
            let start = cutoff as usize * WIDTH as usize + origin as usize + cols.start;
            assert_eq!(
                &clipped.pixels[start..start + cols.len()],
                &reading.frame.pixels[start..start + cols.len()]
            );
            let thumb = page_thumbnail(&mut reading.probe, page).unwrap();
            let dark = thumb.for_theme(true);
            assert!(
                dark.pixels
                    .iter()
                    .zip(&thumb.light.pixels)
                    .any(|(&a, &b)| a != 255 - b)
            );
            assert_eq!(dark.pixels[0], 0);

            let anchor = reading.anchor();
            reading
                .reflow(Settings {
                    invert_night_images: true,
                    ..reading.config.settings
                })
                .unwrap();
            assert_eq!(reading.anchor(), anchor);
            let inverted = render_night(&reading);
            assert_eq!(inverted[pos], 255 - frame[pos]);
            let thumb = page_thumbnail(&mut reading.probe, page).unwrap();
            assert!(thumb.dark.is_none());
            reading
                .reflow(Settings {
                    invert_night_images: false,
                    ..reading.config.settings
                })
                .unwrap();
        }

        // A large type size leaves a full viewport after the image, so scrolling can bisect it.
        reading.reflow(Settings { font_size: 110.0, ..reading.config.settings }).unwrap();
        reading.set_scroll(true).unwrap();
        let (page, _, rows) = find_image(&reading);
        let top = reading.probe.page_range(page).start + (rows.start + rows.len() / 2) as f32
            - reading.probe.settings.top_margin() as f32;
        reading
            .scroll(
                reading.scroll_top.unwrap(),
                top - reading.scroll_top.unwrap(),
            )
            .unwrap();
        let (range, _) = reading.visible_columns().next().unwrap();
        let crate::Renderer::Parley(layout) = &reading.probe.renderer else {
            unreachable!()
        };
        let (_, cols, rows) = layout
            .visible_illustrations(range, reading.probe.settings)
            .next()
            .unwrap();
        assert_eq!(rows.start, reading.probe.settings.top_margin() as usize);
        let night = render_night(&reading);
        let idx = rows.start * WIDTH as usize + cols.start;
        assert_eq!(night[idx], reading.frame.pixels[idx]);
        assert_eq!(
            night[idx - WIDTH as usize],
            255 - reading.frame.pixels[idx - WIDTH as usize]
        );
    }

    #[test]
    fn rtl_spreads_keep_the_source_order_for_selection_links_and_resume() {
        let mut reading = example("rtl-notebook.epub", 42.0, true);
        assert!(reading.probe.pages_right_to_left());
        let width = FACING_PAGE_WIDTH as usize;
        let right = width + FACING_GUTTER as usize;
        let mut words = Vec::new();
        for (page, x) in [(0, right), (1, 0)] {
            let expected = reading.probe.capture(page).unwrap();
            assert!(expected.iter().any(|&pixel| pixel < 255));
            for (actual, expected_row) in reading
                .frame
                .pixels
                .chunks_exact(WIDTH as usize)
                .zip(expected.chunks_exact(width))
            {
                assert_eq!(&actual[x..x + width], expected_row);
            }
            let (wx, wy, word) = first_hit(|px, py| reading.probe.word_at(page, px, py)).unwrap();
            assert_eq!(reading.word_at(wx + x as f32, wy), Some(word.clone()));
            let rects = reading.highlight_rects(&word);
            assert!(!rects.is_empty());
            assert!(
                rects
                    .iter()
                    .all(|rect| rect.x >= x as i32 && rect.x + rect.width <= (x + width) as i32)
            );
            words.push(word);
            let (lx, ly, target) = first_hit(|px, py| reading.probe.link_at(page, px, py)).unwrap();
            assert_eq!(reading.link_at(lx + x as f32, ly), Some(target.clone()));
            assert!(reading.links().iter().any(|(_, link)| *link == target));
        }
        assert!(reading.word_at(WIDTH as f32 / 2.0, 150.0).is_none());
        let span = reading
            .probe
            .select_range(&words[0].start, &words[1].end)
            .unwrap();
        let rects = reading.highlight_rects(&span);
        assert!(rects.iter().any(|rect| rect.x >= right as i32));
        assert!(rects.iter().any(|rect| rect.x + rect.width <= width as i32));

        let anchor = words[1].start.clone();
        reading.jump_to_anchor(0, &anchor).unwrap();
        assert_eq!(reading.page, 1);
        reading.set_facing_pages(false).unwrap();
        assert_eq!(reading.anchor(), Some(anchor.clone()));
        reading.set_facing_pages(true).unwrap();
        let resumed =
            Reading::restore(reading.config.clone(), 0, 0, reading.anchor().as_ref()).unwrap();
        assert_eq!(resumed.anchor(), Some(anchor));
        assert_eq!(resumed.page, 1);
        assert_eq!(resumed.frame.pixels, reading.frame.pixels);
    }

    #[test]
    fn rtl_touch_follows_publisher_direction_and_page_buttons_follow_the_spine() {
        let mut reading = example("rtl-notebook.epub", 42.0, true);
        let right = FACING_PAGE_WIDTH + FACING_GUTTER;
        let mid = PAGE_HEIGHT as f32 / 2.0;
        for (input, expected_page) in [
            (Input::Tap(16.0, mid), 2),
            (Input::Tap(WIDTH as f32 - 16.0, mid), 0),
            (
                Input::Swipe {
                    dx: 250.0,
                    dy: 0.0,
                    start_y: mid,
                },
                2,
            ),
            (Input::PageBack, 0),
            (Input::PageForward, 2),
            (
                Input::Swipe {
                    dx: -250.0,
                    dy: 0.0,
                    start_y: mid,
                },
                0,
            ),
        ] {
            reading.turns.next_refresh = Instant::now();
            assert_eq!(reading.input(input), Change::Partial);
            assert_eq!(reading.page, expected_page);
        }
        reading.go_to_page(1, 0).unwrap();
        assert_eq!(reading.probe.page_count(), 1);
        assert_eq!(reading.probe.chapter_label, "Translation key");
        assert!(
            reading
                .frame
                .pixels
                .chunks_exact(WIDTH as usize)
                .all(|row| row[..right as usize].iter().all(|&pixel| pixel == 255))
        );
        assert!(reading.word_at(100.0, 150.0).is_none());
        assert_eq!(reading.turn(1).unwrap(), Change::None);
        assert_eq!(reading.turn(-1).unwrap(), Change::Partial);
        assert_eq!(reading.chapter, 0);
        assert_eq!(reading.last_visible_page(), reading.probe.page_count() - 1);
    }

    #[test]
    fn odd_chapter_tail_stays_blank_and_vertical_text_uses_the_full_view() {
        let mut reading = example("vertical-chapters.epub", 42.0, true);
        assert_eq!(reading.probe.page_count(), 1);
        let right = (FACING_PAGE_WIDTH + FACING_GUTTER) as usize;
        assert!(
            reading
                .frame
                .pixels
                .chunks_exact(WIDTH as usize)
                .all(|row| row[right..].iter().all(|&pixel| pixel == 255))
        );
        assert_eq!(reading.word_at(right as f32 + 80.0, 120.0), None);
        assert_eq!(reading.turn(1).unwrap(), Change::Partial);
        assert_eq!(reading.chapter, 1);
        assert!(reading.probe.is_vertical());
        assert_eq!(reading.probe.settings.width, WIDTH);
        assert!(reading.config.facing_pages);
        assert_eq!(reading.turn(-1).unwrap(), Change::Partial);
        assert_eq!(reading.chapter, 0);
        assert_eq!(reading.probe.settings.width, FACING_PAGE_WIDTH);
    }

    #[test]
    fn rapid_mixed_presses_are_drained_in_order_at_one_refresh_deadline() {
        let now = Instant::now();
        let mut turns = PageTurns::new(now);
        assert_eq!(turns.deadline(), None);
        turns.pending.extend([1, 1, -1, 1]);
        assert!(turns.take_due(now + Duration::from_millis(15)).is_empty());
        assert_eq!(turns.deadline(), Some(now + PAGE_REFRESH_GAP));
        assert_eq!(turns.take_due(now + PAGE_REFRESH_GAP), [1, 1, -1, 1]);
        assert_eq!(turns.deadline(), None);
    }

    #[test]
    fn enters_and_leaves_vertical_chapters_with_scroll_preference_preserved() {
        let mut reading = Reading::open(BookConfig {
            user_hyphenation: Default::default(),
            chapter_offsets: Default::default(),
            engine: Engine::Parley,
            book: crate::test_fixture("vertical-chapters.epub"),
            fonts: vec![crate::test_fixture("fonts/Silkscreen-Regular.ttf")],
            settings: Settings {
                width: WIDTH,
                height: PAGE_HEIGHT,
                ..Settings::default()
            },
            scroll: true,
            facing_pages: false,
        })
        .unwrap();
        assert!(reading.scroll_top.is_some());
        reading.input(Input::PageForward);
        reading.tick(reading.deadline().unwrap());
        assert_eq!(reading.chapter, 1);
        assert!(reading.probe.is_vertical());
        assert!(reading.scroll_top.is_none());
        assert!(reading.set_scroll(true).is_err());

        reading.input(Input::Tap(12.0, 350.0));
        reading.tick(reading.deadline().unwrap());
        assert_eq!(reading.page, 1);
        reading.input(Input::PageBack);
        reading.tick(reading.deadline().unwrap());
        assert_eq!(reading.page, 0);

        let last_page = reading.probe.page_count() - 1;
        reading.go_to_page(1, last_page).unwrap();
        reading.input(Input::PageForward);
        reading.tick(reading.deadline().unwrap());
        assert_eq!(reading.chapter, 2);
        assert_eq!(reading.scroll_top, Some(0.0));
        assert!(reading.config.scroll);
    }
}
