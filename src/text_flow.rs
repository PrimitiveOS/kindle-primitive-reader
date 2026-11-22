use std::io;
use std::path::PathBuf;
use std::time::Instant;

use primitive_app::{WIDTH, with_storage};
use primitive_ui::{CAPTION, Canvas, GrayImage, HAIRLINE, Input, Rect};

use crate::fixed_session::FixedReading;
use crate::scan_reflow::ScanReflow;
use crate::session::{
    DONE, PAGE_HEIGHT, PAGE_REFRESH_GAP, PagePaint, PageTurns, draw_page_picture,
};
use crate::{Probe, Result, Settings};

const SCAN_SMALLER: Rect = Rect::new(840, PAGE_HEIGHT as i32 + 5, 120, 60);
const SCAN_LARGER: Rect = Rect::new(974, PAGE_HEIGHT as i32 + 5, 120, 60);
const MIN_SCAN_HEIGHT: u32 = 24;
const MAX_SCAN_HEIGHT: u32 = 72;
const SCAN_SIZE_STEP: i32 = 8;

#[derive(Clone)]
enum FlowStyle {
    Text {
        title: String,
        fonts: Vec<PathBuf>,
        settings: Settings,
    },
    Scan {
        text_height: u32,
    },
}

enum FlowLayout {
    Text(Box<Probe>),
    Scan(ScanReflow),
}

impl FlowStyle {
    fn layout_text(&self, text: &str) -> Result<Option<FlowLayout>> {
        let Self::Text {
            title,
            fonts,
            settings,
        } = self
        else {
            return Err("Choose the text reading view first".into());
        };
        if text.trim().is_empty() {
            return Ok(None);
        }
        let probe = load_probe(title, text, fonts, *settings)?;
        Ok(Some(FlowLayout::Text(Box::new(probe))))
    }

    fn layout_page(&self, document: &FixedReading, page: usize) -> Result<Option<FlowLayout>> {
        match self {
            Self::Text { .. } => self.layout_text(&document.page_text(page)?.text),
            Self::Scan { text_height } => {
                let Some(words) = document
                    .cached_ocr(page)
                    .filter(|text| !text.words.is_empty())
                else {
                    return Ok(None);
                };
                let raster = document.ocr_raster(page)?;
                let layout = ScanReflow::new(raster, &words, WIDTH, PAGE_HEIGHT, *text_height)?;
                Ok(Some(FlowLayout::Scan(layout)))
            }
        }
    }

    fn label(&self) -> &'static str {
        match self {
            Self::Text { .. } => "Text",
            Self::Scan { .. } => "Scan",
        }
    }

    fn missing_text(&self) -> &'static str {
        match self {
            Self::Text { .. } => "has no available text. Use OCR to read a scan.",
            Self::Scan { .. } => "needs OCR before the scan can reflow.",
        }
    }
}

impl FlowLayout {
    fn page_count(&self) -> usize {
        match self {
            Self::Text(probe) => probe.page_count(),
            Self::Scan(scan) => scan.page_count(),
        }
    }

    fn capture(&mut self, page: usize) -> Result<GrayImage> {
        match self {
            Self::Text(probe) => Ok(GrayImage {
                width: WIDTH,
                height: PAGE_HEIGHT,
                pixels: probe.capture(page)?,
            }),
            Self::Scan(scan) => Ok(scan.render(page)?),
        }
    }
}

/// Paginated text or image crops from a fixed document, one source page at a time.
pub(crate) struct TextFlow {
    document_page: usize,
    #[cfg(test)]
    document_pages: usize,
    style: FlowStyle,
    layout: FlowLayout,
    page: usize,
    frame: GrayImage,
    turns: PageTurns,
    boundary: Option<String>,
    end_attempt: bool,
}

impl TextFlow {
    pub(crate) fn open(
        document: &FixedReading,
        title: &str,
        fonts: &[PathBuf],
        settings: Settings,
        scan: bool,
    ) -> Result<Self> {
        let style = if scan {
            FlowStyle::Scan { text_height: 40 }
        } else {
            FlowStyle::Text {
                title: title.into(),
                fonts: fonts.to_vec(),
                settings,
            }
        };
        let page = document.page();
        let layout = style
            .layout_page(document, page)?
            .ok_or_else(|| format!("Page {} {}", page + 1, style.missing_text()))?;
        Self::with_layout(page, document.count(), style, layout)
    }

    fn with_layout(
        document_page: usize,
        document_pages: usize,
        style: FlowStyle,
        mut layout: FlowLayout,
    ) -> Result<Self> {
        if document_page >= document_pages {
            return Err("Document page is unavailable".into());
        }
        let frame = layout.capture(0)?;
        Ok(Self {
            document_page,
            #[cfg(test)]
            document_pages,
            style,
            layout,
            page: 0,
            frame,
            turns: PageTurns::new(Instant::now()),
            boundary: None,
            end_attempt: false,
        })
    }

    pub(crate) fn document_page(&self) -> usize {
        self.document_page
    }

    pub(crate) fn from_start(&self, document: &FixedReading) -> Result<Self> {
        let style = self.style.clone();
        let layout = style.layout_page(document, 0)?
            .ok_or_else(|| format!("Page 1 {}", style.missing_text()))?;
        Self::with_layout(0, document.count(), style, layout)
    }

    pub(crate) fn deadline(&self) -> Option<Instant> {
        self.turns.deadline()
    }

    pub(crate) fn pause(&mut self) {
        self.turns = PageTurns::new(Instant::now());
        self.end_attempt = false;
    }

    pub(crate) fn take_end_attempt(&mut self) -> bool {
        std::mem::take(&mut self.end_attempt)
    }

    pub(crate) fn input(&mut self, input: Input, document: &FixedReading) -> Result<bool> {
        if matches!(self.style, FlowStyle::Scan { .. })
            && let Input::Tap(x, y) = input
        {
            if SCAN_SMALLER.contains(x, y) {
                return self.resize_scan(-SCAN_SIZE_STEP, document);
            }
            if SCAN_LARGER.contains(x, y) {
                return self.resize_scan(SCAN_SIZE_STEP, document);
            }
        }
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
        self.tick(Instant::now(), document)
    }

    fn resize_scan(&mut self, step: i32, document: &FixedReading) -> Result<bool> {
        let (FlowStyle::Scan { text_height }, FlowLayout::Scan(scan)) = (&self.style, &self.layout)
        else {
            return Ok(false);
        };
        let height = text_height
            .saturating_add_signed(step)
            .clamp(MIN_SCAN_HEIGHT, MAX_SCAN_HEIGHT);
        if height == *text_height {
            return Ok(false);
        }
        let word = scan
            .page_first_word(self.page)
            .ok_or("This scan page is unavailable")?;
        let style = FlowStyle::Scan {
            text_height: height,
        };
        let Some(FlowLayout::Scan(layout)) = style.layout_page(document, self.document_page)?
        else {
            return Err("This scan needs OCR before it can reflow".into());
        };
        let page = layout
            .page_for_word(word)
            .ok_or("The current word is unavailable")?;
        let frame = layout.render(page)?;
        self.style = style;
        self.layout = FlowLayout::Scan(layout);
        self.page = page;
        self.frame = frame;
        self.boundary = None;
        self.turns = PageTurns::new(Instant::now());
        Ok(true)
    }

    pub(crate) fn tick(&mut self, now: Instant, document: &FixedReading) -> Result<bool> {
        self.step_with_navigation(now, |style, page| style.layout_page(document, page),
            |page, direction| document.next_reading_page(page, direction))
    }

    #[cfg(test)]
    fn step(
        &mut self,
        now: Instant,
        layout_page: impl FnMut(&FlowStyle, usize) -> Result<Option<FlowLayout>>,
    ) -> Result<bool> {
        let count = self.document_pages;
        self.step_with_navigation(now, layout_page,
            |page, direction| page.checked_add_signed(direction).filter(|&page| page < count))
    }

    fn step_with_navigation(
        &mut self, now: Instant,
        mut layout_page: impl FnMut(&FlowStyle, usize) -> Result<Option<FlowLayout>>,
        mut adjacent_page: impl FnMut(usize, isize) -> Option<usize>,
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
                .filter(|next| *next < self.layout.page_count())
            {
                page = next;
                continue;
            }
            let Some(adjacent) = adjacent_page(self.document_page, step)
            else {
                self.end_attempt = step > 0;
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
            let Some(mut layout) = layout_page(&self.style, adjacent)? else {
                boundary = Some(format!(
                    "Page {} {}",
                    adjacent + 1,
                    self.style.missing_text()
                ));
                break;
            };
            let page = if step > 0 { 0 } else { layout.page_count() - 1 };
            let frame = layout.capture(page)?;
            self.document_page = adjacent;
            self.layout = layout;
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
            self.frame = self.layout.capture(page)?;
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
            "Document page {} · {} {} of {}",
            self.document_page + 1,
            self.style.label(),
            self.page + 1,
            self.layout.page_count(),
        );
        let caption_right = if let FlowStyle::Scan { text_height } = self.style {
            if text_height > MIN_SCAN_HEIGHT {
                canvas.button(SCAN_SMALLER, "−", false, false);
            }
            if text_height < MAX_SCAN_HEIGHT {
                canvas.button(SCAN_LARGER, "+", false, false);
            }
            SCAN_SMALLER.x
        } else {
            DONE.x
        };
        canvas.text(
            Rect::new(38, PAGE_HEIGHT as i32 + 12, caption_right - 60, 48),
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
        let style = FlowStyle::Text {
            title: "Field Notes".into(),
            fonts: fonts.to_vec(),
            settings,
        };
        let layout = style.layout_text(texts[0]).unwrap().unwrap();
        let mut flow = TextFlow::with_layout(0, texts.len(), style, layout).unwrap();
        let last = flow.layout.page_count() - 1;
        assert!(last > 0);
        let requested = RefCell::new(Vec::new());
        let mut lookup = |style: &FlowStyle, page: usize| {
            requested.borrow_mut().push(page);
            style.layout_text(texts[page])
        };

        flow.turns.pending.extend(std::iter::repeat_n(1, last));
        let due = flow.deadline().unwrap();
        assert!(
            !flow
                .step(due - Duration::from_millis(1), &mut lookup)
                .unwrap()
        );
        assert!(flow.step(due, &mut lookup).unwrap());
        assert_eq!((flow.document_page(), flow.page), (0, last));
        assert!(requested.borrow().is_empty());

        flow.turns.pending.extend([1, 1]);
        let due = flow.deadline().unwrap();
        assert!(flow.step(due, &mut lookup).unwrap());
        assert_eq!((flow.document_page(), flow.page), (1, 0));
        assert_eq!(*requested.borrow(), [1]);

        let pixels = flow.frame.pixels.clone();
        let next = flow.deadline().unwrap();
        assert!(next >= due + PAGE_REFRESH_GAP);
        assert!(flow.step(next, &mut lookup).unwrap());
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
        assert!(flow.step(flow.deadline().unwrap(), &mut lookup).unwrap());
        assert_eq!((flow.document_page(), flow.page), (0, last));
        assert_eq!(*requested.borrow(), [1, 2, 0]);
        assert!(flow.boundary.is_none());

        flow.turns.pending.extend(std::iter::repeat_n(-1, last + 1));
        assert!(flow.step(flow.deadline().unwrap(), &mut lookup).unwrap());
        assert_eq!((flow.document_page(), flow.page), (0, 0));
        assert_eq!(flow.boundary.as_deref(), Some("Start of document"));
        assert_eq!(*requested.borrow(), [1, 2, 0]);
    }
}
