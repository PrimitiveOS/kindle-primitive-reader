use std::io;
use std::path::PathBuf;
use std::time::Instant;

use primitive_app::{WIDTH, with_storage};
use primitive_ui::{CAPTION, Canvas, GrayImage, HAIRLINE, Input, Rect};

use crate::fixed_session::FixedReading;
use crate::library::FixedReflowSpacing;
use crate::scan_reflow::ScanReflow;
use crate::session::{
    DONE, PAGE_HEIGHT, PAGE_REFRESH_GAP, PagePaint, PageTurns, draw_page_picture,
};
use crate::{Anchor, Probe, Result, Settings};

const SPACING_TOGGLE: Rect = Rect::new(622, PAGE_HEIGHT as i32 + 5, 192, 60);
const SPACING_PANEL: Rect = Rect::new(0, PAGE_HEIGHT as i32 - 146, WIDTH as i32, 146);
const SPACING_BUTTONS: [(FixedReflowSpacing, &str, Rect); 3] = [
    (
        FixedReflowSpacing::Small,
        "Small",
        Rect::new(38, SPACING_PANEL.y + 68, 176, 60),
    ),
    (
        FixedReflowSpacing::Medium,
        "Medium",
        Rect::new(228, SPACING_PANEL.y + 68, 176, 60),
    ),
    (
        FixedReflowSpacing::Large,
        "Large",
        Rect::new(418, SPACING_PANEL.y + 68, 176, 60),
    ),
];
const SPACING_CANCEL: Rect = Rect::new(840, SPACING_PANEL.y + 68, 178, 60);
const SPACING_APPLY: Rect = Rect::new(1032, SPACING_PANEL.y + 68, 194, 60);

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
        spacing: FixedReflowSpacing,
    },
    Scan {
        text_height: u32,
        spacing: FixedReflowSpacing,
    },
}

enum FlowLayout {
    Text(Box<Probe>),
    Scan(ScanReflow),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum FlowAnchor {
    Text(Anchor),
    Scan(usize),
}

impl FlowStyle {
    fn layout_text(&self, text: &str) -> Result<Option<FlowLayout>> {
        let Self::Text {
            title,
            fonts,
            settings,
            spacing,
        } = self
        else {
            return Err("Choose the text reading view first".into());
        };
        if text.trim().is_empty() {
            return Ok(None);
        }
        let settings = if *spacing == FixedReflowSpacing::Medium {
            *settings
        } else {
            let base = u32::from(settings.effective_line_spacing_percent());
            settings.with_line_spacing_percent(spacing.scaled_pitch(base).clamp(50, 200) as u16)
        };
        let probe = load_probe(title, text, fonts, settings)?;
        Ok(Some(FlowLayout::Text(Box::new(probe))))
    }

    fn layout_page(&self, document: &FixedReading, page: usize) -> Result<Option<FlowLayout>> {
        match self {
            Self::Text { .. } => self.layout_text(&document.page_text(page)?.text),
            Self::Scan {
                text_height,
                spacing,
            } => {
                let Some(words) = document
                    .cached_ocr(page)
                    .filter(|text| !text.words.is_empty())
                else {
                    return Ok(None);
                };
                let raster = document.ocr_raster(page)?;
                let layout =
                    ScanReflow::new(raster, &words, WIDTH, PAGE_HEIGHT, *text_height, *spacing)?;
                Ok(Some(FlowLayout::Scan(layout)))
            }
        }
    }

    fn spacing(&self) -> FixedReflowSpacing {
        match self {
            Self::Text { spacing, .. } | Self::Scan { spacing, .. } => *spacing,
        }
    }

    fn set_spacing(&mut self, value: FixedReflowSpacing) {
        match self {
            Self::Text { spacing, .. } | Self::Scan { spacing, .. } => *spacing = value,
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

    fn anchor(&self, page: usize) -> Option<FlowAnchor> {
        match self {
            Self::Text(probe) => probe.page_anchor(page).map(FlowAnchor::Text),
            Self::Scan(scan) => scan.page_first_word(page).map(FlowAnchor::Scan),
        }
    }

    fn page_for(&self, anchor: &FlowAnchor) -> Option<usize> {
        match (self, anchor) {
            (Self::Text(probe), FlowAnchor::Text(anchor)) => probe.page_for(anchor),
            (Self::Scan(scan), FlowAnchor::Scan(word)) => scan.page_for_word(*word),
            _ => None,
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
    // Keep the original source passage across successive layout changes until a page turn.
    retained_anchor: Option<FlowAnchor>,
    spacing_draft: Option<FixedReflowSpacing>,
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
        let spacing = document
            .preferences()
            .fixed_reflow_spacing
            .unwrap_or_default();
        let style = if scan {
            FlowStyle::Scan {
                text_height: 40,
                spacing,
            }
        } else {
            FlowStyle::Text {
                title: title.into(),
                fonts: fonts.to_vec(),
                settings,
                spacing,
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
            retained_anchor: None,
            spacing_draft: None,
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
        let layout = style
            .layout_page(document, 0)?
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
        if let Some(draft) = self.spacing_draft {
            return self.spacing_input(input, document, draft);
        }
        if let Input::Tap(x, y) = input
            && SPACING_TOGGLE.contains(x, y)
        {
            self.spacing_draft = Some(self.line_spacing());
            self.pause();
            return Ok(true);
        }
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

    pub(crate) fn line_spacing(&self) -> FixedReflowSpacing {
        self.style.spacing()
    }

    fn spacing_input(
        &mut self,
        input: Input,
        document: &FixedReading,
        draft: FixedReflowSpacing,
    ) -> Result<bool> {
        match input {
            Input::PageBack => self.spacing_draft = None,
            Input::Tap(x, y) if SPACING_CANCEL.contains(x, y) || SPACING_TOGGLE.contains(x, y) => {
                self.spacing_draft = None;
            }
            Input::Tap(x, y) if SPACING_APPLY.contains(x, y) => {
                if draft != self.line_spacing() {
                    let mut style = self.style.clone();
                    style.set_spacing(draft);
                    self.refresh_layout(style, document)?;
                }
                self.spacing_draft = None;
            }
            Input::Tap(x, y) => {
                let Some((choice, _, _)) = SPACING_BUTTONS
                    .iter()
                    .find(|(_, _, rect)| rect.contains(x, y))
                else {
                    return Ok(false);
                };
                if *choice == draft {
                    return Ok(false);
                }
                self.spacing_draft = Some(*choice);
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn resize_scan(&mut self, step: i32, document: &FixedReading) -> Result<bool> {
        let FlowStyle::Scan {
            text_height,
            spacing,
        } = self.style
        else {
            return Ok(false);
        };
        let height = text_height
            .saturating_add_signed(step)
            .clamp(MIN_SCAN_HEIGHT, MAX_SCAN_HEIGHT);
        if height == text_height {
            return Ok(false);
        }
        self.refresh_layout(
            FlowStyle::Scan {
                text_height: height,
                spacing,
            },
            document,
        )?;
        Ok(true)
    }

    fn refresh_layout(&mut self, style: FlowStyle, document: &FixedReading) -> Result<()> {
        let anchor = self
            .retained_anchor
            .clone()
            .or_else(|| self.layout.anchor(self.page))
            .ok_or("The current reading passage is unavailable")?;
        let mut layout = style
            .layout_page(document, self.document_page)?
            .ok_or_else(|| format!("Page {} {}", self.document_page + 1, style.missing_text()))?;
        let page = layout
            .page_for(&anchor)
            .ok_or("The current reading passage is unavailable after changing the layout")?;
        let frame = layout.capture(page)?;
        self.style = style;
        self.layout = layout;
        self.page = page;
        self.retained_anchor = Some(anchor);
        self.frame = frame;
        self.boundary = None;
        self.pause();
        Ok(())
    }

    pub(crate) fn tick(&mut self, now: Instant, document: &FixedReading) -> Result<bool> {
        self.step_with_navigation(
            now,
            |style, page| style.layout_page(document, page),
            |page, direction| document.next_reading_page(page, direction),
        )
    }

    #[cfg(test)]
    fn step(
        &mut self,
        now: Instant,
        layout_page: impl FnMut(&FlowStyle, usize) -> Result<Option<FlowLayout>>,
    ) -> Result<bool> {
        let count = self.document_pages;
        self.step_with_navigation(now, layout_page, |page, direction| {
            page.checked_add_signed(direction)
                .filter(|&page| page < count)
        })
    }

    fn step_with_navigation(
        &mut self,
        now: Instant,
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
            let Some(adjacent) = adjacent_page(self.document_page, step) else {
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
            self.retained_anchor = None;
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
            self.retained_anchor = None;
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
        let mut paint = PagePaint::full(inverted);
        if self.spacing_draft.is_some() {
            paint.visible.end = SPACING_PANEL.y as u32;
        }
        draw_page_picture(canvas, self.frame.clone(), paint);
        canvas.button(
            SPACING_TOGGLE,
            "Spacing",
            self.spacing_draft.is_some(),
            false,
        );
        if let Some(draft) = self.spacing_draft {
            canvas.fill(SPACING_PANEL, 255);
            canvas.fill(Rect::new(0, SPACING_PANEL.y, WIDTH as i32, 1), HAIRLINE);
            canvas.text(
                Rect::new(38, SPACING_PANEL.y + 8, 400, 53),
                "Line spacing",
                CAPTION,
            );
            for (choice, label, rect) in SPACING_BUTTONS {
                canvas.button(rect, label, draft == choice, false);
            }
            canvas.button(SPACING_CANCEL, "Cancel", false, false);
            canvas.button(SPACING_APPLY, "Apply", false, false);
        }
        canvas.fill(Rect::new(0, PAGE_HEIGHT as i32, WIDTH as i32, 1), HAIRLINE);
        let caption = format!(
            "Document page {} · {} {} of {}",
            self.document_page + 1,
            self.style.label(),
            self.page + 1,
            self.layout.page_count(),
        );
        if self.spacing_draft.is_none()
            && let FlowStyle::Scan { text_height, .. } = self.style
        {
            if text_height > MIN_SCAN_HEIGHT {
                canvas.button(SCAN_SMALLER, "-", false, false);
            }
            if text_height < MAX_SCAN_HEIGHT {
                canvas.button(SCAN_LARGER, "+", false, false);
            }
        }
        canvas.text(
            Rect::new(38, PAGE_HEIGHT as i32 + 12, SPACING_TOGGLE.x - 60, 48),
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
    fn fixed_reflow_spacing_preserves_a_later_passage_and_restores_cancelled_layout() {
        use crate::library::{DocumentFormat, FixedPreferences};
        let tap =
            |rect: Rect| Input::Tap((rect.x + rect.w / 2) as f32, (rect.y + rect.h / 2) as f32);
        for (scan, name) in [(false, "vector.pdf"), (true, "raster.pdf")] {
            let path = crate::test_fixture(&format!("scan-reflow/{name}"));
            let mut document = FixedReading::open(
                PAGE_HEIGHT,
                DocumentFormat::Pdf,
                &path,
                1,
                &[],
                FixedPreferences::default(),
                None,
                None,
                None,
            )
            .unwrap()
            .unwrap();
            let position = document.history_point();
            if scan {
                assert!(document.page_text(1).unwrap().words.is_empty());
                let ocr = crate::ocr::parse_tsv(
                    include_bytes!("../tests/fixtures/scan-reflow/page-2-ocr.tsv"),
                    1350,
                    1800,
                )
                .unwrap();
                assert!(ocr.text.words.len() > 500);
                document.remember_ocr(1, "eng", ocr.text).unwrap();
            }
            let fonts = [crate::test_fixture("fonts/NotoSans-Regular.ttf")];
            let settings = Settings {
                width: WIDTH,
                height: PAGE_HEIGHT,
                ..Settings::default()
            };
            let mut flow = TextFlow::open(&document, "Harbor", &fonts, settings, scan).unwrap();
            assert_eq!(flow.line_spacing(), FixedReflowSpacing::Medium);
            assert!(flow.layout.page_count() > 1);
            flow.input(Input::PageForward, &document).unwrap();
            flow.tick(Instant::now() + Duration::from_secs(1), &document)
                .unwrap();
            assert_eq!((flow.document_page(), flow.page), (1, 1));
            let origin = flow.layout.anchor(1).unwrap();
            assert_ne!(Some(&origin), flow.layout.anchor(0).as_ref());
            let original = flow.frame.pixels.clone();
            assert!(flow.input(tap(SPACING_TOGGLE), &document).unwrap());
            assert!(flow.input(tap(SPACING_BUTTONS[0].2), &document).unwrap());
            assert_eq!(flow.frame.pixels, original);
            assert!(!flow.input(Input::PageForward, &document).unwrap());
            assert!(flow.input(Input::PageBack, &document).unwrap());
            assert_eq!(flow.page, 1);
            assert_eq!(flow.line_spacing(), FixedReflowSpacing::Medium);
            assert_eq!(flow.frame.pixels, original);

            for choice in [2, 0, 1] {
                flow.input(tap(SPACING_TOGGLE), &document).unwrap();
                flow.input(tap(SPACING_BUTTONS[choice].2), &document)
                    .unwrap();
                flow.input(tap(SPACING_APPLY), &document).unwrap();
                assert_eq!(flow.line_spacing(), SPACING_BUTTONS[choice].0);
                assert_eq!(flow.retained_anchor.as_ref(), Some(&origin));
                assert_eq!(flow.layout.page_for(&origin), Some(flow.page));
                assert_eq!(flow.document_page(), 1);
                assert_eq!(document.history_point(), position);
                if choice != 1 {
                    assert_ne!(flow.frame.pixels, original);
                }
            }
            assert_eq!(flow.page, 1);
            assert_eq!(flow.frame.pixels, original);
            if scan {
                document.forget_ocr();
                flow.input(tap(SPACING_TOGGLE), &document).unwrap();
                flow.input(tap(SPACING_BUTTONS[2].2), &document).unwrap();
                assert!(flow.input(tap(SPACING_APPLY), &document).is_err());
                assert_eq!(flow.line_spacing(), FixedReflowSpacing::Medium);
                assert_eq!(flow.retained_anchor.as_ref(), Some(&origin));
                assert_eq!(flow.frame.pixels, original);
                assert!(flow.input(tap(SPACING_CANCEL), &document).unwrap());
            }
        }
    }

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
            spacing: FixedReflowSpacing::Medium,
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
