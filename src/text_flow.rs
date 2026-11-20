use std::path::PathBuf;
use std::time::Instant;

use primitive_app::WIDTH;
use primitive_ui::{CAPTION, Canvas, GrayImage, HAIRLINE, Input, Rect};

use crate::session::{DONE, PAGE_HEIGHT, PAGE_REFRESH_GAP, PageTurns, draw_page_picture};
use crate::{Probe, Result, Settings};

/// Paginated text from one document page, kept independent of its fixed layout.
pub(crate) struct TextFlow {
    document_page: usize,
    probe: Probe,
    page: usize,
    frame: GrayImage,
    turns: PageTurns,
}

impl TextFlow {
    pub(crate) fn open(
        document_page: usize,
        title: &str,
        text: &str,
        fonts: &[PathBuf],
        settings: Settings,
    ) -> Result<Self> {
        let mut probe = Probe::from_text(title.into(), text, fonts, settings)?;
        let frame = capture(&mut probe, 0)?;
        Ok(Self {
            document_page,
            probe,
            page: 0,
            frame,
            turns: PageTurns::new(Instant::now()),
        })
    }

    pub(crate) fn deadline(&self) -> Option<Instant> {
        self.turns.deadline()
    }

    pub(crate) fn pause(&mut self) {
        self.turns = PageTurns::new(Instant::now());
    }

    pub(crate) fn input(&mut self, input: Input) -> Result<bool> {
        let direction = match input {
            Input::PageForward => 1,
            Input::PageBack => -1,
            Input::Swipe { dx, dy, .. } => {
                let displacement = if dx.abs() >= dy.abs() { dx } else { dy };
                if displacement < 0.0 { 1 } else { -1 }
            }
            Input::Tap(x, y) if y < PAGE_HEIGHT as f32 && x < WIDTH as f32 / 3.0 => -1,
            Input::Tap(x, y) if y < PAGE_HEIGHT as f32 && x > WIDTH as f32 * 2.0 / 3.0 => 1,
            _ => return Ok(false),
        };
        self.turns.pending.push(direction);
        self.tick(Instant::now())
    }

    pub(crate) fn tick(&mut self, now: Instant) -> Result<bool> {
        let last = self.probe.page_count() - 1;
        let page = self
            .turns
            .take_due(now)
            .into_iter()
            .fold(self.page, |page, step| {
                page.saturating_add_signed(step).min(last)
            });
        if page == self.page {
            return Ok(false);
        }
        self.frame = capture(&mut self.probe, page)?;
        self.page = page;
        self.turns.next_refresh = Instant::now() + PAGE_REFRESH_GAP;
        Ok(true)
    }

    pub(crate) fn draw(&self, canvas: &mut Canvas, notice: Option<&str>) {
        draw_page_picture(canvas, self.frame.clone(), 0..PAGE_HEIGHT);
        canvas.fill(Rect::new(0, PAGE_HEIGHT as i32, WIDTH as i32, 1), HAIRLINE);
        let caption = format!(
            "Document page {} · Text {} of {}",
            self.document_page + 1,
            self.page + 1,
            self.probe.page_count(),
        );
        canvas.text(
            Rect::new(38, PAGE_HEIGHT as i32 + 12, DONE.x - 60, 48),
            notice.unwrap_or(&caption),
            CAPTION,
        );
        canvas.button(DONE, "Back", false, false);
    }
}

fn capture(probe: &mut Probe, page: usize) -> Result<GrayImage> {
    Ok(GrayImage {
        width: WIDTH,
        height: PAGE_HEIGHT,
        pixels: probe.capture(page)?,
    })
}
