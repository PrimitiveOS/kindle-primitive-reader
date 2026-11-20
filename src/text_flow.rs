use std::io;
use std::path::PathBuf;
use std::time::Instant;

use primitive_app::{WIDTH, with_storage};
use primitive_ui::{CAPTION, Canvas, GrayImage, HAIRLINE, Input, Rect};

use crate::fixed_text::PdfPageText;
use crate::session::{
    DONE, PAGE_HEIGHT, PAGE_REFRESH_GAP, PagePaint, PageTurns, draw_page_picture,
};
use crate::{Probe, Result, Settings};

/// Paginated text from a fixed document, with one source page laid out at a time.
pub(crate) struct TextFlow {
    document_page: usize,
    document_pages: usize,
    title: String,
    fonts: Vec<PathBuf>,
    settings: Settings,
    probe: Probe,
    page: usize,
    frame: GrayImage,
    turns: PageTurns,
    boundary: Option<String>,
}

impl TextFlow {
    pub(crate) fn open(
        document_page: usize,
        document_pages: usize,
        title: &str,
        text: &str,
        fonts: &[PathBuf],
        settings: Settings,
    ) -> Result<Self> {
        if document_page >= document_pages {
            return Err("Document page is unavailable".into());
        }
        let mut probe = load_probe(title, text, fonts, settings)?;
        let frame = capture(&mut probe, 0)?;
        Ok(Self {
            document_page,
            document_pages,
            title: title.into(),
            fonts: fonts.to_vec(),
            settings,
            probe,
            page: 0,
            frame,
            turns: PageTurns::new(Instant::now()),
            boundary: None,
        })
    }

    pub(crate) fn document_page(&self) -> usize {
        self.document_page
    }

    pub(crate) fn deadline(&self) -> Option<Instant> {
        self.turns.deadline()
    }

    pub(crate) fn pause(&mut self) {
        self.turns = PageTurns::new(Instant::now());
    }

    pub(crate) fn input(
        &mut self,
        input: Input,
        page_text: impl FnMut(usize) -> Result<PdfPageText>,
    ) -> Result<bool> {
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
        self.tick(Instant::now(), page_text)
    }

    pub(crate) fn tick(
        &mut self,
        now: Instant,
        mut page_text: impl FnMut(usize) -> Result<PdfPageText>,
    ) -> Result<bool> {
        let due = self.turns.take_due(now);
        if due.is_empty() {
            return Ok(false);
        }
        let mut turns = due.into_iter();
        let mut page = self.page;
        let mut boundary = None;
        while let Some(step) = turns.next() {
            if let Some(next) = page
                .checked_add_signed(step)
                .filter(|next| *next < self.probe.page_count())
            {
                page = next;
                continue;
            }
            let Some(adjacent) = self
                .document_page
                .checked_add_signed(step)
                .filter(|next| *next < self.document_pages)
            else {
                boundary = Some(
                    if step > 0 {
                        "End of document"
                    } else {
                        "Start of document"
                    }
                    .into(),
                );
                break;
            };
            let text = page_text(adjacent)?;
            if text.text.trim().is_empty() {
                boundary = Some(format!(
                    "Page {} has no available text. Use OCR to read a scan.",
                    adjacent + 1
                ));
                break;
            }
            let mut probe = load_probe(&self.title, &text.text, &self.fonts, self.settings)?;
            let page = if step > 0 { 0 } else { probe.page_count() - 1 };
            let frame = capture(&mut probe, page)?;
            self.document_page = adjacent;
            self.probe = probe;
            self.page = page;
            self.frame = frame;
            self.boundary = None;
            // A single source-page load per refresh keeps rapid turns responsive.
            self.turns.pending.extend(turns);
            self.defer_refresh(now);
            return Ok(true);
        }

        let page_changed = page != self.page;
        if page_changed {
            self.frame = capture(&mut self.probe, page)?;
            self.page = page;
        }
        let changed = page_changed || self.boundary != boundary;
        if changed {
            self.boundary = boundary;
            self.defer_refresh(now);
        }
        Ok(changed)
    }

    fn defer_refresh(&mut self, now: Instant) {
        self.turns.next_refresh = Instant::now().max(now) + PAGE_REFRESH_GAP;
    }

    pub(crate) fn draw(&self, canvas: &mut Canvas, notice: Option<&str>, inverted: bool) {
        draw_page_picture(canvas, self.frame.clone(), PagePaint::full(inverted));
        canvas.fill(Rect::new(0, PAGE_HEIGHT as i32, WIDTH as i32, 1), HAIRLINE);
        let caption = format!(
            "Document page {} · Text {} of {}",
            self.document_page + 1,
            self.page + 1,
            self.probe.page_count(),
        );
        canvas.text(
            Rect::new(38, PAGE_HEIGHT as i32 + 12, DONE.x - 60, 48),
            notice.or(self.boundary.as_deref()).unwrap_or(&caption),
            CAPTION,
        );
        canvas.button(DONE, "Back", false, false);
    }
}

fn load_probe(title: &str, text: &str, fonts: &[PathBuf], settings: Settings) -> Result<Probe> {
    with_storage(|| Probe::from_text(title.into(), text, fonts, settings).map_err(io::Error::other))
        .map_err(Into::into)
}

fn capture(probe: &mut Probe, page: usize) -> Result<GrayImage> {
    Ok(GrayImage {
        width: WIDTH,
        height: PAGE_HEIGHT,
        pixels: probe.capture(page)?,
    })
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::time::Duration;

    use super::*;

    #[test]
    fn preserves_panel_position_across_source_boundaries_in_both_directions() {
        let long_text = "A clear tide comes back toward shore.\n\n".repeat(90);
        let texts = [
            long_text.as_str(),
            "The next page has a single short line.",
            " \n ",
            "A readable page after the scan.",
        ];
        let fonts = [crate::test_fixture("fonts/Silkscreen-Regular.ttf")];
        let settings = Settings {
            width: WIDTH,
            height: PAGE_HEIGHT,
            ..Settings::default()
        };
        let mut flow =
            TextFlow::open(0, texts.len(), "Field Notes", texts[0], &fonts, settings).unwrap();
        let last = flow.probe.page_count() - 1;
        assert!(last > 0);
        let requested = RefCell::new(Vec::new());
        let mut lookup = |page| {
            requested.borrow_mut().push(page);
            Ok(PdfPageText {
                text: String::from(texts[page]),
                ..PdfPageText::default()
            })
        };

        flow.turns.pending.extend(std::iter::repeat_n(1, last));
        let due = flow.deadline().unwrap();
        assert!(
            !flow
                .tick(due - Duration::from_millis(1), &mut lookup)
                .unwrap()
        );
        assert!(flow.tick(due, &mut lookup).unwrap());
        assert_eq!((flow.document_page(), flow.page), (0, last));
        assert!(requested.borrow().is_empty());

        flow.turns.pending.extend([1, 1]);
        let due = flow.deadline().unwrap();
        assert!(flow.tick(due, &mut lookup).unwrap());
        assert_eq!((flow.document_page(), flow.page), (1, 0));
        assert_eq!(*requested.borrow(), [1]);

        let pixels = flow.frame.pixels.clone();
        let next = flow.deadline().unwrap();
        assert!(next >= due + PAGE_REFRESH_GAP);
        assert!(flow.tick(next, &mut lookup).unwrap());
        assert_eq!((flow.document_page(), flow.page), (1, 0));
        assert_eq!(*requested.borrow(), [1, 2]);
        assert_eq!(flow.frame.pixels, pixels);
        assert!(
            flow.boundary
                .as_ref()
                .unwrap()
                .contains("Page 3 has no available text")
        );

        flow.turns.pending.push(-1);
        assert!(flow.tick(flow.deadline().unwrap(), &mut lookup).unwrap());
        assert_eq!((flow.document_page(), flow.page), (0, last));
        assert_eq!(*requested.borrow(), [1, 2, 0]);
        assert!(flow.boundary.is_none());

        flow.turns.pending.extend(std::iter::repeat_n(-1, last + 1));
        assert!(flow.tick(flow.deadline().unwrap(), &mut lookup).unwrap());
        assert_eq!((flow.document_page(), flow.page), (0, 0));
        assert_eq!(flow.boundary.as_deref(), Some("Start of document"));
        assert_eq!(*requested.borrow(), [1, 2, 0]);
    }
}
