use std::env;
use std::io;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use primitive_app::{App, Change, HEIGHT, WIDTH, with_storage};
use primitive_ui::{CAPTION, Canvas, GrayImage, HAIRLINE, Input, Rect};
use reader::{Engine, Probe, Result, Settings};

const USAGE: &str = "Usage: reader-kindle <blitz|parley> <book.epub> <font> [fallback-font ...]";
const FOOTER_HEIGHT: u32 = 74;
const PAGE_HEIGHT: u32 = HEIGHT - FOOTER_HEIGHT;
const DONE: Rect = Rect::new(WIDTH as i32 - 154, PAGE_HEIGHT as i32 + 5, 148, 60);
const PREFETCH_DELAY: Duration = Duration::from_millis(480);
// Give a partial E Ink update time to settle before submitting another.
const PAGE_REFRESH_GAP: Duration = Duration::from_millis(450);

struct PageTurns {
    pending: Vec<isize>,
    next_refresh: Instant,
}

impl PageTurns {
    fn new(now: Instant) -> Self {
        Self {
            pending: Vec::new(),
            next_refresh: now + PAGE_REFRESH_GAP,
        }
    }

    fn deadline(&self) -> Option<Instant> {
        (!self.pending.is_empty()).then_some(self.next_refresh)
    }

    fn take_due(&mut self, now: Instant) -> Vec<isize> {
        if now < self.next_refresh {
            return Vec::new();
        }
        std::mem::take(&mut self.pending)
    }
}

struct BookConfig {
    engine: Engine,
    book: PathBuf,
    fonts: Vec<PathBuf>,
    settings: Settings,
}

impl BookConfig {
    fn chapter(&self, index: usize) -> Result<Probe> {
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

struct Reading {
    config: BookConfig,
    chapter: usize,
    probe: Probe,
    page: usize,
    frame: GrayImage,
    error: Option<String>,
    adjacent: Vec<(usize, GrayImage)>,
    prefetch: Option<Instant>,
    turns: PageTurns,
}

impl Reading {
    fn open(config: BookConfig) -> Result<Self> {
        let start = Instant::now();
        let mut probe = config.chapter(0)?;
        let frame = capture(&mut probe, 0, 0)?;
        eprintln!(
            "reader first_frame_ready_us={}",
            start.elapsed().as_micros()
        );
        let mut session = Self {
            config,
            chapter: 0,
            probe,
            page: 0,
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
                    capture(&mut self.probe, self.chapter, page)?
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
            self.frame = capture(&mut probe, chapter, page)?;
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
        let previous = (self.chapter, self.page, self.error.is_some());
        let mut change = Change::None;
        for direction in pending {
            match self.turn(direction) {
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
        if previous == (self.chapter, self.page, false) && self.error.is_none() {
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
        self.prefetch = self
            .prefetch_page()
            .map(|_| Instant::now() + PREFETCH_DELAY);
    }
}

impl App for Reading {
    fn draw(&self, canvas: &mut Canvas) {
        canvas.picture(
            Rect::new(0, 0, WIDTH as i32, PAGE_HEIGHT as i32),
            self.frame.clone(),
        );
        canvas.fill(Rect::new(0, PAGE_HEIGHT as i32, WIDTH as i32, 1), HAIRLINE);
        let progress = format!(
            "Chapter {} of {}  ·  Page {} of {}",
            self.chapter + 1,
            self.probe.chapter_count(),
            self.page + 1,
            self.probe.page_count(),
        );
        canvas.text(
            Rect::new(38, PAGE_HEIGHT as i32 + 12, DONE.x - 60, 48),
            self.error.as_deref().unwrap_or(&progress),
            CAPTION,
        );
        canvas.button(DONE, "Done", true, false);
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
                && let Ok(frame) = capture(&mut self.probe, self.chapter, page)
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

fn capture(probe: &mut Probe, chapter: usize, page: usize) -> Result<GrayImage> {
    let start = Instant::now();
    let pixels = probe.capture(page)?;
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

fn main() -> Result<()> {
    let mut args = env::args_os().skip(1);
    let engine = args.next().ok_or(USAGE)?;
    if engine == "--help" || engine == "-h" {
        println!("{USAGE}");
        return Ok(());
    }
    if env::var_os("KL_NATIVE_APP_RUN").is_none() {
        return Err("Open the display probe through the PrimitiveOS Launcher".into());
    }
    let engine = engine.to_str().ok_or(USAGE)?.parse()?;
    let book = args.next().map(PathBuf::from).ok_or(USAGE)?;
    let fonts = args.map(PathBuf::from).collect::<Vec<_>>();
    if fonts.is_empty() {
        return Err(USAGE.into());
    }
    let executable = env::current_exe()?;
    let app_dir = executable
        .parent()
        .ok_or("Reader's app directory is unavailable")?;
    let reading = Reading::open(BookConfig {
        engine,
        book,
        fonts,
        settings: Settings {
            width: WIDTH,
            height: PAGE_HEIGHT,
            ..Settings::default()
        },
    })?;
    primitive_app::run(reading, app_dir).map_err(Into::into)
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
