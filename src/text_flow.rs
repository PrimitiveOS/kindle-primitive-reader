use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use primitive_app::with_storage;
use primitive_ui::{CAPTION, Canvas, GrayImage, HAIRLINE, Input, Rect};

use crate::fixed_session::FixedReading;
use crate::library::FixedReflowSpacing;
use crate::scan_reflow::ScanReflow;
use crate::session::{PAGE_REFRESH_GAP, PagePaint, PageTurns, ReadingViewport, draw_page_picture};
use crate::{Anchor, Probe, Result, Settings};

const MIN_SCAN_HEIGHT: u32 = 24;
const MAX_SCAN_HEIGHT: u32 = 72;
const SCAN_SIZE_STEP: i32 = 8;

struct Controls {
    footer_y: i32,
    width: i32,
    spacing: Rect,
    panel: Rect,
    choices: [(FixedReflowSpacing, &'static str, Rect); 3],
    cancel: Rect,
    apply: Rect,
    smaller: Rect,
    larger: Rect,
}

impl Controls {
    fn new(viewport: ReadingViewport) -> Self {
        let width = viewport.dimensions().0 as i32;
        let footer_y = viewport.page_height() as i32;
        let panel = Rect::new(0, footer_y - 146, width, 146);
        let row = panel.y + 68;
        Self {
            footer_y,
            width,
            spacing: Rect::new(width - 642, footer_y + 5, 192, 60),
            panel,
            choices: [
                (
                    FixedReflowSpacing::Small,
                    "Small",
                    Rect::new(38, row, 176, 60),
                ),
                (
                    FixedReflowSpacing::Medium,
                    "Medium",
                    Rect::new(228, row, 176, 60),
                ),
                (
                    FixedReflowSpacing::Large,
                    "Large",
                    Rect::new(418, row, 176, 60),
                ),
            ],
            cancel: Rect::new(width - 424, row, 178, 60),
            apply: Rect::new(width - 232, row, 194, 60),
            smaller: Rect::new(width - 424, footer_y + 5, 120, 60),
            larger: Rect::new(width - 290, footer_y + 5, 120, 60),
        }
    }
}

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
    fn layout_text(&self, text: &str, viewport: ReadingViewport) -> Result<Option<FlowLayout>> {
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
        let settings = Settings {
            width: viewport.dimensions().0,
            height: viewport.page_height(),
            ..*settings
        };
        let settings = if *spacing == FixedReflowSpacing::Medium {
            settings
        } else {
            let base = u32::from(settings.effective_line_spacing_percent());
            settings.with_line_spacing_percent(spacing.scaled_pitch(base).clamp(50, 200) as u16)
        };
        let probe = load_probe(title, text, fonts, settings)?;
        Ok(Some(FlowLayout::Text(Box::new(probe))))
    }

    fn layout_page(
        &self,
        document: &FixedReading,
        page: usize,
        viewport: ReadingViewport,
    ) -> Result<Option<FlowLayout>> {
        match self {
            Self::Text { .. } => self.layout_text(&document.page_text(page)?.text, viewport),
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
                let layout = ScanReflow::new(
                    raster,
                    &words,
                    viewport.dimensions().0,
                    viewport.page_height(),
                    *text_height,
                    *spacing,
                )?;
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

    fn capture(&mut self, page: usize, viewport: ReadingViewport) -> Result<GrayImage> {
        match self {
            Self::Text(probe) => Ok(GrayImage {
                width: viewport.dimensions().0,
                height: viewport.page_height(),
                pixels: probe.capture(page)?,
            }),
            Self::Scan(scan) => Ok(scan.render(page)?),
        }
    }
}

/// Paginated text or image crops from a fixed document, one source page at a time.
pub(crate) struct TextFlow {
    viewport: ReadingViewport,
    instance: Arc<()>,
    revision: u64,
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

pub(crate) struct ViewportStage {
    candidate: Box<TextFlow>,
}

impl ViewportStage {
    pub(crate) fn matches(&self, flow: &TextFlow) -> bool {
        Arc::ptr_eq(&self.candidate.instance, &flow.instance)
            && self.candidate.revision == flow.revision
    }

    pub(crate) fn exchange(&mut self, flow: &mut TextFlow) {
        std::mem::swap(self.candidate.as_mut(), flow);
    }
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
        let viewport = document.viewport();
        let layout = style
            .layout_page(document, page, viewport)?
            .ok_or_else(|| format!("Page {} {}", page + 1, style.missing_text()))?;
        Self::with_layout(page, document.count(), style, layout, viewport)
    }

    fn with_layout(
        document_page: usize,
        document_pages: usize,
        style: FlowStyle,
        mut layout: FlowLayout,
        viewport: ReadingViewport,
    ) -> Result<Self> {
        if document_page >= document_pages {
            return Err("Document page is unavailable".into());
        }
        let frame = layout.capture(0, viewport)?;
        Ok(Self {
            viewport,
            instance: Arc::new(()),
            revision: 0,
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
        let viewport = document.viewport();
        let layout = style
            .layout_page(document, 0, viewport)?
            .ok_or_else(|| format!("Page 1 {}", style.missing_text()))?;
        Self::with_layout(0, document.count(), style, layout, viewport)
    }

    pub(crate) fn prepare_viewport(
        &self,
        document: &FixedReading,
        viewport: ReadingViewport,
    ) -> Result<ViewportStage> {
        // The continuous fixed view may start on an earlier source page when
        // it fills the last screen. TextFlow keeps its own explicit page.
        let (layout, page, anchor, frame) = self.relayout(&self.style, document, viewport)?;
        Ok(ViewportStage {
            candidate: Box::new(Self {
                viewport,
                instance: Arc::clone(&self.instance),
                revision: self.revision,
                document_page: self.document_page,
                #[cfg(test)]
                document_pages: self.document_pages,
                style: self.style.clone(),
                layout,
                page,
                retained_anchor: Some(anchor),
                spacing_draft: self.spacing_draft,
                frame,
                turns: PageTurns {
                    pending: self.turns.pending.clone(),
                    next_refresh: self.turns.next_refresh,
                },
                boundary: self.boundary.clone(),
                end_attempt: self.end_attempt,
            }),
        })
    }

    pub(crate) fn deadline(&self) -> Option<Instant> {
        self.turns.deadline()
    }

    pub(crate) fn pause(&mut self) {
        self.turns = PageTurns::new(Instant::now());
        self.end_attempt = false;
        self.advance_revision();
    }

    pub(crate) fn take_end_attempt(&mut self) -> bool {
        std::mem::take(&mut self.end_attempt)
    }

    #[cfg(test)]
    pub(crate) fn input(&mut self, input: Input, document: &FixedReading) -> Result<bool> {
        let turn = match input {
            Input::PageForward | Input::PageBack => Some(input),
            _ => None,
        };
        self.input_with_turn(input, turn, document)
    }

    pub(crate) fn input_with_turn(
        &mut self,
        input: Input,
        turn: Option<Input>,
        document: &FixedReading,
    ) -> Result<bool> {
        let controls = Controls::new(self.viewport);
        if let Some(draft) = self.spacing_draft {
            return self.spacing_input(input, document, &controls, draft);
        }
        if let Input::Tap(x, y) = input
            && controls.spacing.contains(x, y)
        {
            self.spacing_draft = Some(self.line_spacing());
            self.pause();
            return Ok(true);
        }
        if matches!(self.style, FlowStyle::Scan { .. })
            && let Input::Tap(x, y) = input
        {
            if controls.smaller.contains(x, y) {
                return self.resize_scan(-SCAN_SIZE_STEP, document);
            }
            if controls.larger.contains(x, y) {
                return self.resize_scan(SCAN_SIZE_STEP, document);
            }
        }
        let direction = match turn {
            Some(Input::PageForward) => 1,
            Some(Input::PageBack) => -1,
            _ => return Ok(false),
        };
        self.turns.pending.push(direction);
        self.advance_revision();
        self.tick(Instant::now(), document)
    }

    pub(crate) fn line_spacing(&self) -> FixedReflowSpacing {
        self.style.spacing()
    }

    fn spacing_input(
        &mut self,
        input: Input,
        document: &FixedReading,
        controls: &Controls,
        draft: FixedReflowSpacing,
    ) -> Result<bool> {
        match input {
            Input::PageBack => self.spacing_draft = None,
            Input::Tap(x, y)
                if controls.cancel.contains(x, y) || controls.spacing.contains(x, y) =>
            {
                self.spacing_draft = None;
            }
            Input::Tap(x, y) if controls.apply.contains(x, y) => {
                if draft != self.line_spacing() {
                    let mut style = self.style.clone();
                    style.set_spacing(draft);
                    self.refresh_layout(style, document)?;
                }
                self.spacing_draft = None;
            }
            Input::Tap(x, y) => {
                let Some((choice, _, _)) = controls
                    .choices
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
        self.advance_revision();
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
        let (layout, page, anchor, frame) = self.relayout(&style, document, self.viewport)?;
        self.style = style;
        self.layout = layout;
        self.page = page;
        self.retained_anchor = Some(anchor);
        self.frame = frame;
        self.boundary = None;
        self.pause();
        Ok(())
    }

    fn relayout(
        &self,
        style: &FlowStyle,
        document: &FixedReading,
        viewport: ReadingViewport,
    ) -> Result<(FlowLayout, usize, FlowAnchor, GrayImage)> {
        let anchor = self
            .retained_anchor
            .clone()
            .or_else(|| self.layout.anchor(self.page))
            .ok_or("The current reading passage is unavailable")?;
        let mut layout = style
            .layout_page(document, self.document_page, viewport)?
            .ok_or_else(|| format!("Page {} {}", self.document_page + 1, style.missing_text()))?;
        let page = layout
            .page_for(&anchor)
            .ok_or("The current reading passage is unavailable after changing the layout")?;
        let frame = layout.capture(page, viewport)?;
        Ok((layout, page, anchor, frame))
    }

    pub(crate) fn tick(&mut self, now: Instant, document: &FixedReading) -> Result<bool> {
        let viewport = self.viewport;
        self.step_with_navigation(
            now,
            |style, page| style.layout_page(document, page, viewport),
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
        self.advance_revision();
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
            let frame = layout.capture(page, self.viewport)?;
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
            self.frame = self.layout.capture(page, self.viewport)?;
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

    fn advance_revision(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }

    pub(crate) fn draw(
        &self,
        canvas: &mut Canvas,
        notice: Option<&str>,
        inverted: bool,
        guides: Option<crate::app::reading_guides::Lines>,
    ) {
        let controls = Controls::new(self.viewport);
        let mut paint = PagePaint::full(inverted);
        paint.reading_guides = guides;
        if self.spacing_draft.is_some() {
            paint.visible.end = controls.panel.y as u32;
        }
        draw_page_picture(canvas, self.frame.clone(), paint);
        canvas.button(
            controls.spacing,
            "Spacing",
            self.spacing_draft.is_some(),
            false,
        );
        if let Some(draft) = self.spacing_draft {
            canvas.fill(controls.panel, 255);
            canvas.fill(Rect::new(0, controls.panel.y, controls.width, 1), HAIRLINE);
            canvas.text(
                Rect::new(38, controls.panel.y + 8, 400, 53),
                "Line spacing",
                CAPTION,
            );
            for (choice, label, rect) in controls.choices {
                canvas.button(rect, label, draft == choice, false);
            }
            canvas.button(controls.cancel, "Cancel", false, false);
            canvas.button(controls.apply, "Apply", false, false);
        }
        canvas.fill(Rect::new(0, controls.footer_y, controls.width, 1), HAIRLINE);
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
                canvas.button(controls.smaller, "-", false, false);
            }
            if text_height < MAX_SCAN_HEIGHT {
                canvas.button(controls.larger, "+", false, false);
            }
        }
        canvas.text(
            Rect::new(38, controls.footer_y + 12, controls.spacing.x - 60, 48),
            notice.or(self.boundary.as_deref()).unwrap_or(&caption),
            CAPTION,
        );
        canvas.button(self.viewport.done(), "Back", false, false);
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
    use crate::session::PAGE_HEIGHT;
    use primitive_app::WIDTH;

    fn authored_document(scan: bool) -> FixedReading {
        use crate::library::{DocumentFormat, FixedPreferences};
        let name = if scan { "raster.pdf" } else { "vector.pdf" };
        let mut document = FixedReading::open(
            PAGE_HEIGHT,
            DocumentFormat::Pdf,
            &crate::test_fixture(&format!("scan-reflow/{name}")),
            1,
            &[],
            FixedPreferences::default(),
            None,
            None,
            None,
        )
        .unwrap()
        .unwrap();
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
        document
    }

    #[test]
    fn scan_text_viewport_retains_the_exact_reading_anchor_for_both_providers() {
        let fonts = [crate::test_fixture("fonts/NotoSans-Regular.ttf")];
        let portrait = ReadingViewport::PORTRAIT;
        let landscape = ReadingViewport::LANDSCAPE;
        for scan in [false, true] {
            let mut document = authored_document(scan);
            let mut flow =
                TextFlow::open(&document, "Harbor", &fonts, Settings::default(), scan).unwrap();
            flow.input(Input::PageForward, &document).unwrap();
            flow.tick(Instant::now() + Duration::from_secs(1), &document)
                .unwrap();
            let origin = flow.layout.anchor(flow.page).unwrap();
            assert_eq!(flow.page, 1);
            assert!(matches!(origin, FlowAnchor::Scan(_)) == scan);
            let original = flow.frame.pixels.clone();
            let pending = flow.deadline();
            let mut staged = flow.prepare_viewport(&document, landscape).unwrap();
            assert!(staged.matches(&flow));
            staged.exchange(&mut flow);
            assert_eq!(flow.viewport, landscape);
            assert_eq!(flow.document_page(), 1);
            assert_eq!(flow.retained_anchor.as_ref(), Some(&origin));
            assert_eq!(flow.layout.page_for(&origin), Some(flow.page));
            assert_eq!(flow.frame.width, landscape.dimensions().0);
            assert_eq!(flow.frame.height, landscape.page_height());
            assert_eq!(flow.deadline(), pending);
            let mut round_trip = flow.prepare_viewport(&document, portrait).unwrap();
            round_trip.exchange(&mut flow);
            assert_eq!(flow.page, 1);
            assert_eq!(flow.retained_anchor.as_ref(), Some(&origin));
            assert_eq!(flow.frame.pixels, original);
            if scan {
                document.forget_ocr();
                assert!(flow.prepare_viewport(&document, landscape).is_err());
                assert_eq!(flow.viewport, portrait);
                assert_eq!(flow.frame.pixels, original);
            }
        }
    }

    #[test]
    fn fixed_reflow_spacing_preserves_a_later_passage_and_restores_cancelled_layout() {
        let controls = Controls::new(ReadingViewport::PORTRAIT);
        let tap =
            |rect: Rect| Input::Tap((rect.x + rect.w / 2) as f32, (rect.y + rect.h / 2) as f32);
        for scan in [false, true] {
            let mut document = authored_document(scan);
            let position = document.history_point();
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
            assert!(flow.input(tap(controls.spacing), &document).unwrap());
            assert!(flow.input(tap(controls.choices[0].2), &document).unwrap());
            assert_eq!(flow.frame.pixels, original);
            assert!(!flow.input(Input::PageForward, &document).unwrap());
            assert!(flow.input(Input::PageBack, &document).unwrap());
            assert_eq!(flow.page, 1);
            assert_eq!(flow.line_spacing(), FixedReflowSpacing::Medium);
            assert_eq!(flow.frame.pixels, original);

            for choice in [2, 0, 1] {
                flow.input(tap(controls.spacing), &document).unwrap();
                flow.input(tap(controls.choices[choice].2), &document)
                    .unwrap();
                flow.input(tap(controls.apply), &document).unwrap();
                assert_eq!(flow.line_spacing(), controls.choices[choice].0);
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
                flow.input(tap(controls.spacing), &document).unwrap();
                flow.input(tap(controls.choices[2].2), &document).unwrap();
                assert!(flow.input(tap(controls.apply), &document).is_err());
                assert_eq!(flow.line_spacing(), FixedReflowSpacing::Medium);
                assert_eq!(flow.retained_anchor.as_ref(), Some(&origin));
                assert_eq!(flow.frame.pixels, original);
                assert!(flow.input(tap(controls.cancel), &document).unwrap());
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
        let viewport = ReadingViewport::PORTRAIT;
        let layout = style.layout_text(texts[0], viewport).unwrap().unwrap();
        let mut flow = TextFlow::with_layout(0, texts.len(), style, layout, viewport).unwrap();
        let last = flow.layout.page_count() - 1;
        assert!(last > 0);
        let requested = RefCell::new(Vec::new());
        let mut lookup = |style: &FlowStyle, page: usize| {
            requested.borrow_mut().push(page);
            style.layout_text(texts[page], viewport)
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
