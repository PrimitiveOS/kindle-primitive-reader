use std::io;
use std::ops::Range;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::epub::Destination;
use crate::selection::{HighlightRect, TextSelection};
use crate::{Anchor, Engine, Probe, Result, Settings};
use primitive_app::{App, Change, HEIGHT, WIDTH, with_storage};
use primitive_ui::{CAPTION, Canvas, GrayImage, HAIRLINE, Input, Rect};

const FOOTER_HEIGHT: u32 = 74;
pub(crate) const PAGE_HEIGHT: u32 = HEIGHT - FOOTER_HEIGHT;
pub(crate) const DONE: Rect = Rect::new(WIDTH as i32 - 154, PAGE_HEIGHT as i32 + 5, 148, 60);
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

    pub fn is_full(&self) -> bool {
        self.visible == (0..PAGE_HEIGHT)
    }

    pub(crate) fn apply(self, mut page: GrayImage) -> (Rect, GrayImage) {
        let Self { visible, inverted } = self;
        if visible != (0..PAGE_HEIGHT) {
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

pub(crate) struct PageTurns {
    pub(crate) pending: Vec<isize>,
    pub(crate) next_refresh: Instant,
}

impl PageTurns {
    pub(crate) fn new(now: Instant) -> Self {
        Self {
            pending: Vec::new(),
            next_refresh: now + PAGE_REFRESH_GAP,
        }
    }

    pub(crate) fn deadline(&self) -> Option<Instant> {
        (!self.pending.is_empty()).then_some(self.next_refresh)
    }

    pub(crate) fn take_due(&mut self, now: Instant) -> Vec<isize> {
        if now < self.next_refresh {
            return Vec::new();
        }
        std::mem::take(&mut self.pending)
    }
}

#[derive(Clone)]
pub(crate) struct BookConfig {
    pub(crate) engine: Engine,
    pub(crate) book: PathBuf,
    pub(crate) fonts: Vec<PathBuf>,
    pub(crate) settings: Settings,
    pub(crate) scroll: bool,
}

impl BookConfig {
    pub(crate) fn chapter(&self, index: usize) -> Result<Probe> {
        let start = Instant::now();
        let probe = with_storage(|| {
            Probe::open(&self.book, self.engine, &self.fonts, self.settings, index)
                .map_err(io::Error::other)
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
}

pub(crate) struct Reading {
    pub(crate) config: BookConfig,
    pub(crate) chapter: usize,
    pub(crate) probe: Probe,
    pub(crate) page: usize,
    scroll_top: Option<f32>,
    frame: GrayImage,
    error: Option<String>,
    adjacent: Vec<(usize, GrayImage)>,
    prefetch: Option<Instant>,
    turns: PageTurns,
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
        let page = anchor
            .and_then(|anchor| probe.page_for(anchor))
            .unwrap_or(fallback_page.min(probe.page_count() - 1));
        let scroll_top = config.scroll.then(|| {
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
            frame,
            error: None,
            adjacent: Vec::new(),
            prefetch: None,
            turns: PageTurns::new(Instant::now()),
        };
        session.schedule_prefetch();
        Ok(session)
    }

    fn turn(&mut self, direction: isize) -> Result<Change> {
        let start = Instant::now();
        let mut crossed_chapter = false;
        let mut cache_hit = false;
        if let Some(page) = self
            .page
            .checked_add_signed(direction)
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
            self.adjacent.retain(|(index, _)| index.abs_diff(page) == 1);
            self.adjacent.push((self.page, prior));
            self.page = page;
        } else if let Some(chapter) = self
            .chapter
            .checked_add_signed(direction)
            .filter(|chapter| *chapter < self.probe.chapter_count())
        {
            let mut probe = self.config.chapter(chapter)?;
            let page = if direction < 0 {
                probe.page_count() - 1
            } else {
                0
            };
            self.frame = capture(&mut probe, chapter, page, None)?;
            self.probe = probe;
            self.chapter = chapter;
            self.page = page;
            self.adjacent.clear();
            crossed_chapter = true;
        } else {
            return Ok(Change::None);
        }
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
        for direction in pending {
            let result = if let Some(top) = self.scroll_top {
                self.scroll(top, direction as f32)
            } else {
                self.turn(direction)
            };
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
        let anchor = self.anchor();
        let mut config = self.config.clone();
        config.scroll = scroll;
        *self = Self::restore(config, self.chapter, self.page, anchor.as_ref())?;
        Ok(())
    }

    pub(crate) fn go_to_page(&mut self, chapter: usize, page: usize) -> Result<()> {
        let next = Self::restore(self.config.clone(), chapter, page, None)?;
        if page >= next.probe.page_count() {
            return Err("That page is outside the chapter".into());
        }
        *self = next;
        Ok(())
    }

    fn visible_range(&self) -> Range<f32> {
        self.scroll_top.map_or_else(
            || self.probe.page_range(self.page),
            |top| self.probe.scroll_range(top),
        )
    }

    pub(crate) fn word_at(&self, x: f32, y: f32) -> Option<TextSelection> {
        self.probe.word_in(self.visible_range(), x, y)
    }

    pub(crate) fn highlight_rects(&self, selection: &TextSelection) -> Vec<HighlightRect> {
        self.probe
            .selection_rects_in(self.visible_range(), selection)
    }

    pub(crate) fn link_at(&self, x: f32, y: f32) -> Option<Destination> {
        self.probe.link_in(self.visible_range(), x, y)
    }

    pub(crate) fn illustration_at(&self, x: f32, y: f32) -> Option<&str> {
        self.probe.illustration_in(self.visible_range(), x, y)
    }

    pub(crate) fn open_illustration(&self, package_path: &str) -> Result<crate::comic::ImagePage> {
        Ok(with_storage(|| {
            self.probe
                .open_illustration(&self.config.book, package_path)
                .map_err(io::Error::other)
        })?)
    }

    pub(crate) fn links(&self) -> Vec<(String, Destination)> {
        self.probe.links_in(self.visible_range())
    }

    pub(crate) fn anchor(&self) -> Option<Anchor> {
        self.probe.anchor_in(self.visible_range())
    }

    fn scroll(&mut self, top: f32, distance: f32) -> Result<Change> {
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
            let mut probe = self.config.chapter(chapter)?;
            let next = if direction < 0 {
                probe.scroll_end()
            } else {
                0.0
            };
            let page = probe.page_at(next);
            self.frame = capture(&mut probe, chapter, page, Some(next))?;
            self.probe = probe;
            self.chapter = chapter;
            self.page = page;
            self.scroll_top = Some(next);
        }
        self.error = None;
        Ok(Change::Partial)
    }

    pub(crate) fn jump(&mut self, chapter: usize, fragment: Option<&str>) -> Result<()> {
        let mut candidate = self.config.chapter(chapter)?;
        let anchor = match fragment {
            Some(name) => candidate
                .fragment(name)
                .filter(|anchor| candidate.page_for(anchor).is_some())
                .map(Some)
                .ok_or("That location is unavailable")?,
            None => None,
        };
        let page = anchor
            .as_ref()
            .and_then(|anchor| candidate.page_for(anchor))
            .unwrap_or(0);
        let top = self.config.scroll.then(|| {
            anchor
                .as_ref()
                .and_then(|anchor| candidate.anchor_top(anchor))
                .unwrap_or(0.0)
                .clamp(0.0, candidate.scroll_end())
        });
        let frame = capture(&mut candidate, chapter, page, top)?;
        self.probe = candidate;
        self.frame = frame;
        self.chapter = chapter;
        self.page = top.map_or(page, |top| self.probe.page_at(top));
        self.scroll_top = top;
        self.error = None;
        self.adjacent.clear();
        self.turns = PageTurns::new(Instant::now());
        self.schedule_prefetch();
        Ok(())
    }

    fn prefetch_page(&self) -> Option<usize> {
        [Some(self.page + 1), self.page.checked_sub(1)]
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
    }

    pub(crate) fn draw_page(
        &self,
        canvas: &mut Canvas,
        action: Option<&str>,
        notice: Option<&str>,
        highlights: &[HighlightRect],
        selection: &[HighlightRect],
        paint: PagePaint,
    ) {
        let mut page = self.frame.clone();
        for (rects, gray) in [(highlights, 220_u8), (selection, 176_u8)] {
            for rect in rects {
                let (x, y) = (rect.x as usize, rect.y as usize);
                let (width, height) = (rect.width as usize, rect.height as usize);
                for row in page
                    .pixels
                    .as_chunks_mut::<{ WIDTH as usize }>()
                    .0
                    .iter_mut()
                    .skip(y)
                    .take(height)
                {
                    for pixel in &mut row[x..x + width] {
                        *pixel = (*pixel).min(gray);
                    }
                }
            }
        }
        let full = paint.is_full();
        draw_page_picture(canvas, page, paint);
        if !full {
            return;
        }
        canvas.fill(Rect::new(0, PAGE_HEIGHT as i32, WIDTH as i32, 1), HAIRLINE);
        let Some(action) = action else { return };
        let progress = format!(
            "Chapter {} of {}  ·  Page {} of {}",
            self.chapter + 1,
            self.probe.chapter_count(),
            self.page + 1,
            self.probe.page_count(),
        );
        canvas.text(
            Rect::new(38, PAGE_HEIGHT as i32 + 12, DONE.x - 60, 48),
            notice.or(self.error.as_deref()).unwrap_or(&progress),
            CAPTION,
        );
        canvas.button(DONE, action, false, false);
    }
}

pub(crate) fn draw_page_picture(canvas: &mut Canvas, page: GrayImage, paint: PagePaint) {
    let (rect, page) = paint.apply(page);
    canvas.picture(rect, page);
}

impl App for Reading {
    fn draw(&self, canvas: &mut Canvas) {
        self.draw_page(canvas, Some("Done"), None, &[], &[], PagePaint::full(false));
    }

    fn input(&mut self, input: Input) -> Change {
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
            Input::Swipe { dx, dy, .. } if self.config.scroll && dy.abs() >= dx.abs() => {
                self.turns.pending.push((-dy).round() as isize);
                return self.present_page_turns(Instant::now());
            }
            Input::Swipe { dx, dy, .. } if dx.abs() > dy.abs() => {
                if dx < 0.0 {
                    1
                } else {
                    -1
                }
            }
            Input::Tap(x, y) if y < PAGE_HEIGHT as f32 && x < WIDTH as f32 / 3.0 => -1,
            Input::Tap(x, y) if y < PAGE_HEIGHT as f32 && x > WIDTH as f32 * 2.0 / 3.0 => 1,
            _ => return Change::None,
        };
        self.prefetch = None;
        let direction = if self.config.scroll {
            direction * (self.config.settings.text_height() as isize - 90)
        } else {
            direction
        };
        self.turns.pending.push(direction);
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

pub(crate) fn page_thumbnail(probe: &mut Probe, page: usize) -> Result<GrayImage> {
    let full = probe.capture(page)?;
    let image = image::GrayImage::from_raw(WIDTH, PAGE_HEIGHT, full)
        .ok_or("The page has incorrect dimensions")?;
    let height = 280;
    let width = WIDTH * height / PAGE_HEIGHT;
    let pixels = image::imageops::thumbnail(&image, width, height).into_raw();
    Ok(GrayImage {
        width,
        height,
        pixels,
    })
}

fn capture(probe: &mut Probe, chapter: usize, page: usize, top: Option<f32>) -> Result<GrayImage> {
    let start = Instant::now();
    let pixels = match top {
        Some(top) => probe.capture_range(probe.scroll_range(top))?,
        None => probe.capture(page)?,
    };
    eprintln!(
        "reader chapter={} page={} paint_us={}",
        chapter + 1,
        page + 1,
        start.elapsed().as_micros(),
    );
    if pixels.len() != (WIDTH * PAGE_HEIGHT) as usize {
        return Err("The page has incorrect dimensions".into());
    }
    Ok(GrayImage {
        width: WIDTH,
        height: PAGE_HEIGHT,
        pixels,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
