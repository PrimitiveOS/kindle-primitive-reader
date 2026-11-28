use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use primitive_app::with_storage;
use primitive_ui::{CAPTION, Canvas, GrayImage, HAIRLINE, Input, Rect};

use crate::fixed_session::FixedReading;
use crate::library::{
    DEFAULT_SCAN_REFLOW_TEXT_SCALE, FixedReflowAlignment, FixedReflowColumns,
    FixedReflowDirection, FixedReflowSpacing, FixedReflowWordGap, MAX_SCAN_REFLOW_TEXT_SCALE,
    FixedTextReflowDirection, MIN_SCAN_REFLOW_TEXT_SCALE, SCAN_REFLOW_TEXT_SCALE_STEP,
};
use crate::scan_reflow::ScanReflow;
use crate::session::page_overlap::Marker;
use crate::session::{PAGE_REFRESH_GAP, PagePaint, PageTurns, ReadingViewport, draw_page_picture};
use crate::{Anchor, PageOverlapStyle, Probe, Result, Settings};

const SCAN_SIZE_STEP: i32 = SCAN_REFLOW_TEXT_SCALE_STEP as i32;

struct Controls {
    footer_y: i32,
    width: i32,
    spacing: Rect,
    panel: Rect,
    choices: [(FixedReflowSpacing, &'static str, Rect); 3],
    column_choices: [(FixedReflowColumns, &'static str, Rect); 3],
    word_gap_choices: [(FixedReflowWordGap, &'static str, Rect); 3],
    alignment_choices: [(FixedReflowAlignment, &'static str, Rect); 5],
    direction_choices: Vec<(DirectionChoice, &'static str, Rect)>,
    overlap_choices: [(bool, &'static str, Rect); 2],
    markers: [(PageOverlapStyle, Rect); 5],
    cancel: Rect,
    apply: Rect,
    smaller: Rect,
    larger: Rect,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DirectionChoice {
    Scan(FixedReflowDirection),
    Text(FixedTextReflowDirection),
}

impl Controls {
    fn new(viewport: ReadingViewport, scan: bool) -> Self {
        let width = viewport.dimensions().0 as i32;
        let footer_y = viewport.page_height() as i32;
        let panel_height = if scan { 452 } else { 386 };
        let panel = Rect::new(0, footer_y - panel_height, width, panel_height);
        let spacing_row = panel.y + 57;
        let overlap_row = spacing_row + 66;
        let alignment_row = overlap_row + 66;
        let direction_row = alignment_row + if scan { 66 } else { 0 };
        let marker_row = direction_row + 66;
        let action_row = marker_row + 66;
        let marker_width = (width - 76 - 10 * 4) / 5;
        let mut marker_x = 38;
        let markers = std::array::from_fn(|index| {
            let width = marker_width + [18, -6, -6, -6, 0][index];
            let rect = Rect::new(marker_x, marker_row, width, 60);
            marker_x += width + 10;
            (PageOverlapStyle::ALL[index], rect)
        });
        let alignment_width = (width - 214 - 38 - 10 * 4) / 5;
        let alignment_labels = ["Auto", "Left", "Center", "Right", "Justify"];
        let alignment_choices = std::array::from_fn(|index| {
            (
                FixedReflowAlignment::ALL[index],
                alignment_labels[index],
                Rect::new(
                    214 + index as i32 * (alignment_width + 10),
                    alignment_row,
                    alignment_width,
                    60,
                ),
            )
        });
        let direction_start = 758;
        let direction_step = 104;
        let direction_choices = if scan {
            FixedReflowDirection::ALL
                .into_iter()
                .enumerate()
                .map(|(index, direction)| {
                    (
                        DirectionChoice::Scan(direction),
                        direction.label(),
                        Rect::new(
                            direction_start + index as i32 * direction_step,
                            direction_row,
                            96,
                            60,
                        ),
                    )
                })
                .collect()
        } else {
            FixedTextReflowDirection::ALL
                .into_iter()
                .enumerate()
                .map(|(index, direction)| {
                    (
                        DirectionChoice::Text(direction),
                        direction.label(),
                        Rect::new(
                            direction_start + index as i32 * direction_step,
                            direction_row,
                            96,
                            60,
                        ),
                    )
                })
                .collect()
        };
        Self {
            footer_y,
            width,
            spacing: Rect::new(width - 642, footer_y + 5, 192, 60),
            panel,
            choices: [
                (
                    FixedReflowSpacing::Small,
                    "Small",
                    Rect::new(38, spacing_row, 176, 60),
                ),
                (
                    FixedReflowSpacing::Medium,
                    "Medium",
                    Rect::new(228, spacing_row, 176, 60),
                ),
                (
                    FixedReflowSpacing::Large,
                    "Large",
                    Rect::new(418, spacing_row, 176, 60),
                ),
            ],
            column_choices: [
                (
                    FixedReflowColumns::One,
                    "1",
                    Rect::new(758, spacing_row, 96, 60),
                ),
                (
                    FixedReflowColumns::Two,
                    "2",
                    Rect::new(864, spacing_row, 96, 60),
                ),
                (
                    FixedReflowColumns::Three,
                    "3",
                    Rect::new(970, spacing_row, 96, 60),
                ),
            ],
            word_gap_choices: [
                (
                    FixedReflowWordGap::Small,
                    "Small",
                    Rect::new(758, overlap_row, 96, 60),
                ),
                (
                    FixedReflowWordGap::Auto,
                    "Auto",
                    Rect::new(864, overlap_row, 96, 60),
                ),
                (
                    FixedReflowWordGap::Large,
                    "Large",
                    Rect::new(970, overlap_row, 96, 60),
                ),
            ],
            alignment_choices,
            direction_choices,
            overlap_choices: [
                (false, "Off", Rect::new(242, overlap_row, 150, 60)),
                (true, "On", Rect::new(406, overlap_row, 150, 60)),
            ],
            markers,
            cancel: Rect::new(width - 424, action_row, 178, 58),
            apply: Rect::new(width - 232, action_row, 194, 58),
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
        direction: FixedTextReflowDirection,
    },
    Scan {
        text_scale: u16,
        spacing: FixedReflowSpacing,
        columns: FixedReflowColumns,
        word_gap: FixedReflowWordGap,
        alignment: FixedReflowAlignment,
        direction: FixedReflowDirection,
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

#[derive(Clone, Debug, PartialEq, Eq)]
struct Location {
    anchor: FlowAnchor,
    inset: i32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct FlowPreferences {
    pub(crate) spacing: FixedReflowSpacing,
    pub(crate) columns: FixedReflowColumns,
    pub(crate) word_gap: FixedReflowWordGap,
    pub(crate) alignment: FixedReflowAlignment,
    pub(crate) direction: FixedReflowDirection,
    pub(crate) text_direction: FixedTextReflowDirection,
    pub(crate) text_scale: Option<u16>,
    pub(crate) overlap_enabled: bool,
    pub(crate) marker: PageOverlapStyle,
}

impl FlowStyle {
    fn layout_text(&self, text: &str, viewport: ReadingViewport) -> Result<Option<FlowLayout>> {
        let Self::Text {
            title,
            fonts,
            settings,
            spacing,
            direction,
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
        let probe = load_probe(title, text, fonts, settings, *direction)?;
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
                text_scale,
                spacing,
                columns,
                word_gap,
                alignment,
                direction,
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
                    *text_scale,
                    *spacing,
                    *columns,
                    *word_gap,
                    *alignment,
                    *direction,
                )?;
                Ok(Some(FlowLayout::Scan(layout)))
            }
        }
    }

    fn set_spacing(&mut self, value: FixedReflowSpacing) {
        match self {
            Self::Text { spacing, .. } | Self::Scan { spacing, .. } => *spacing = value,
        }
    }

    fn set_columns(&mut self, value: FixedReflowColumns) {
        if let Self::Scan { columns, .. } = self {
            *columns = value;
        }
    }

    fn set_word_gap(&mut self, value: FixedReflowWordGap) {
        if let Self::Scan { word_gap, .. } = self {
            *word_gap = value;
        }
    }

    fn set_alignment(&mut self, value: FixedReflowAlignment) {
        if let Self::Scan { alignment, .. } = self {
            *alignment = value;
        }
    }

    fn set_direction(&mut self, value: FixedReflowDirection) {
        if let Self::Scan { direction, .. } = self {
            *direction = value;
        }
    }

    fn set_text_direction(&mut self, value: FixedTextReflowDirection) {
        if let Self::Text { direction, .. } = self {
            *direction = value;
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
    fn clip(&self) -> Rect {
        match self {
            Self::Text(probe) => Rect::new(
                0,
                probe.settings.top_margin() as i32,
                probe.settings.width as i32,
                probe.settings.text_height() as i32,
            ),
            Self::Scan(scan) => scan.scroll_clip(),
        }
    }

    fn limit(&self) -> u32 {
        match self {
            Self::Text(probe) => probe.scroll_end().round() as u32,
            Self::Scan(scan) => scan.scroll_end(),
        }
    }

    fn step(&self, overlap: u8) -> u32 {
        (self.clip().h as u32 * (100 - u32::from(overlap)))
            .div_ceil(100)
            .max(1)
    }

    fn location(&self, top: u32) -> Option<Location> {
        let anchor = match self {
            Self::Text(probe) => probe
                .anchor_in(probe.scroll_range(top as f32))
                .map(FlowAnchor::Text),
            Self::Scan(scan) => scan.word_at_scroll_top(top).map(FlowAnchor::Scan),
        }?;
        Some(Location {
            inset: top as i32 - self.anchor_top(&anchor)? as i32,
            anchor,
        })
    }

    fn anchor_top(&self, anchor: &FlowAnchor) -> Option<u32> {
        match (self, anchor) {
            (Self::Text(probe), FlowAnchor::Text(anchor)) => {
                Some(probe.anchor_top(anchor)?.round() as u32)
            }
            (Self::Scan(scan), FlowAnchor::Scan(word)) => scan.word_scroll_top(*word),
            _ => None,
        }
    }

    fn resolve(&self, location: &Location) -> Option<u32> {
        let top = self
            .anchor_top(&location.anchor)?
            .saturating_add_signed(location.inset);
        Some(top.min(self.limit()))
    }

    fn capture(&mut self, top: u32, viewport: ReadingViewport) -> Result<GrayImage> {
        match self {
            Self::Text(probe) => Ok(GrayImage {
                width: viewport.dimensions().0,
                height: viewport.page_height(),
                pixels: probe.capture_range(probe.scroll_range(top as f32))?,
            }),
            Self::Scan(scan) => Ok(scan.render_scroll(top)),
        }
    }
}

/// A view onto one reflowed source page from a fixed document.
pub(crate) struct TextFlow {
    viewport: ReadingViewport,
    instance: Arc<()>,
    revision: u64,
    document_page: usize,
    #[cfg(test)]
    document_pages: usize,
    style: FlowStyle,
    layout: FlowLayout,
    top: u32,
    zoom_overlap: u8,
    overlap_cue: Option<Marker>,
    // Keep the original source passage across successive layout changes until a page turn.
    retained_location: Option<Location>,
    preferences: FlowPreferences,
    layout_draft: Option<FlowPreferences>,
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
        let saved = document.preferences();
        let preferences = FlowPreferences {
            spacing: saved.fixed_reflow_spacing.unwrap_or_default(),
            columns: saved.fixed_reflow_columns.unwrap_or_default(),
            word_gap: saved.fixed_reflow_word_gap.unwrap_or_default(),
            alignment: saved.fixed_reflow_alignment.unwrap_or_default(),
            direction: saved.fixed_reflow_direction.unwrap_or_default(),
            text_direction: saved
                .fixed_text_reflow_direction
                .or_else(|| saved.fixed_reflow_direction.map(Into::into))
                .unwrap_or_default(),
            text_scale: saved.fixed_reflow_text_scale,
            overlap_enabled: saved.page_overlap_enabled,
            marker: saved.page_overlap_style.unwrap_or_default(),
        };
        let spacing = preferences.spacing;
        let style = if scan {
            FlowStyle::Scan {
                text_scale: preferences
                    .text_scale
                    .unwrap_or(DEFAULT_SCAN_REFLOW_TEXT_SCALE),
                spacing,
                columns: preferences.columns,
                word_gap: preferences.word_gap,
                alignment: preferences.alignment,
                direction: preferences.direction,
            }
        } else {
            FlowStyle::Text {
                title: title.into(),
                fonts: fonts.to_vec(),
                settings,
                spacing,
                direction: preferences.text_direction,
            }
        };
        let page = document.page();
        let viewport = document.viewport();
        let layout = style
            .layout_page(document, page, viewport)?
            .ok_or_else(|| format!("Page {} {}", page + 1, style.missing_text()))?;
        Self::with_layout(
            page,
            document.count(),
            style,
            layout,
            viewport,
            saved.zoom_overlap_v(),
            preferences,
        )
    }

    fn with_layout(
        document_page: usize,
        document_pages: usize,
        style: FlowStyle,
        mut layout: FlowLayout,
        viewport: ReadingViewport,
        zoom_overlap: u8,
        preferences: FlowPreferences,
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
            top: 0,
            zoom_overlap,
            overlap_cue: None,
            retained_location: None,
            preferences,
            layout_draft: None,
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
        Self::with_layout(
            0,
            document.count(),
            style,
            layout,
            viewport,
            self.zoom_overlap,
            self.preferences,
        )
    }

    pub(crate) fn prepare_viewport(
        &self,
        document: &FixedReading,
        viewport: ReadingViewport,
    ) -> Result<ViewportStage> {
        // The continuous fixed view may start on an earlier source page when
        // it fills the last screen. TextFlow keeps its own explicit page.
        let (layout, top, location, frame) = self.relayout(&self.style, document, viewport)?;
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
                top,
                zoom_overlap: self.zoom_overlap,
                overlap_cue: None,
                retained_location: Some(location),
                preferences: self.preferences,
                layout_draft: self.layout_draft,
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
        let controls = Controls::new(self.viewport, matches!(self.style, FlowStyle::Scan { .. }));
        if let Some(draft) = self.layout_draft {
            return self.layout_input(input, document, &controls, draft);
        }
        if let Input::Tap(x, y) = input
            && controls.spacing.contains(x, y)
        {
            self.layout_draft = Some(self.preferences);
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

    #[cfg(test)]
    pub(crate) fn line_spacing(&self) -> FixedReflowSpacing {
        self.preferences.spacing
    }

    pub(crate) fn preferences(&self) -> FlowPreferences {
        self.preferences
    }

    #[cfg(test)]
    pub(crate) fn overlap_test_targets(&self) -> [Rect; 4] {
        let controls = Controls::new(self.viewport, matches!(self.style, FlowStyle::Scan { .. }));
        [
            controls.spacing,
            controls.overlap_choices[1].2,
            controls.markers[4].1,
            controls.apply,
        ]
    }

    #[cfg(test)]
    pub(crate) fn spacing_test_targets(&self) -> [Rect; 4] {
        let controls = Controls::new(self.viewport, matches!(self.style, FlowStyle::Scan { .. }));
        [
            controls.spacing,
            controls.choices[2].2,
            controls.cancel,
            controls.apply,
        ]
    }

    #[cfg(test)]
    pub(crate) fn pan_offset(&self) -> u32 {
        self.top
    }

    fn layout_input(
        &mut self,
        input: Input,
        document: &FixedReading,
        controls: &Controls,
        mut draft: FlowPreferences,
    ) -> Result<bool> {
        match input {
            Input::PageBack => self.layout_draft = None,
            Input::Tap(x, y)
                if controls.cancel.contains(x, y) || controls.spacing.contains(x, y) =>
            {
                self.layout_draft = None;
            }
            Input::Tap(x, y) if controls.apply.contains(x, y) => {
                let scan = matches!(self.style, FlowStyle::Scan { .. });
                let direction_changed = if scan {
                    draft.direction != self.preferences.direction
                } else {
                    draft.text_direction != self.preferences.text_direction
                };
                if draft.spacing != self.preferences.spacing
                    || (scan
                        && (draft.columns != self.preferences.columns
                            || draft.word_gap != self.preferences.word_gap
                            || draft.alignment != self.preferences.alignment))
                    || direction_changed
                {
                    let mut style = self.style.clone();
                    style.set_spacing(draft.spacing);
                    style.set_columns(draft.columns);
                    style.set_word_gap(draft.word_gap);
                    style.set_alignment(draft.alignment);
                    if scan {
                        style.set_direction(draft.direction);
                    } else {
                        style.set_text_direction(draft.text_direction);
                    }
                    self.refresh_layout(style, document)?;
                }
                if !draft.overlap_enabled {
                    self.overlap_cue = None;
                }
                self.preferences = draft;
                self.layout_draft = None;
            }
            Input::Tap(x, y) => {
                if let Some((spacing, _, _)) = controls
                    .choices
                    .iter()
                    .find(|(_, _, rect)| rect.contains(x, y))
                {
                    draft.spacing = *spacing;
                } else if matches!(self.style, FlowStyle::Scan { .. })
                    && let Some((columns, _, _)) = controls
                        .column_choices
                        .iter()
                        .find(|(_, _, rect)| rect.contains(x, y))
                {
                    draft.columns = *columns;
                } else if matches!(self.style, FlowStyle::Scan { .. })
                    && let Some((word_gap, _, _)) = controls
                        .word_gap_choices
                        .iter()
                        .find(|(_, _, rect)| rect.contains(x, y))
                {
                    draft.word_gap = *word_gap;
                } else if matches!(self.style, FlowStyle::Scan { .. })
                    && draft.direction != FixedReflowDirection::TopToBottomRightToLeft
                    && let Some((alignment, _, _)) = controls
                        .alignment_choices
                        .iter()
                        .find(|(_, _, rect)| rect.contains(x, y))
                {
                    draft.alignment = *alignment;
                } else if let Some((direction, _, _)) = controls
                        .direction_choices
                        .iter()
                        .find(|(_, _, rect)| rect.contains(x, y))
                {
                    match direction {
                        DirectionChoice::Scan(direction) => draft.direction = *direction,
                        DirectionChoice::Text(direction) => draft.text_direction = *direction,
                    }
                } else if let Some((enabled, _, _)) = controls
                    .overlap_choices
                    .iter()
                    .find(|(_, _, rect)| rect.contains(x, y))
                {
                    draft.overlap_enabled = *enabled;
                } else if draft.overlap_enabled
                    && let Some((marker, _)) = controls
                        .markers
                        .iter()
                        .find(|(_, rect)| rect.contains(x, y))
                {
                    draft.marker = *marker;
                }
                if Some(draft) == self.layout_draft {
                    return Ok(false);
                }
                self.layout_draft = Some(draft);
            }
            _ => return Ok(false),
        }
        self.advance_revision();
        Ok(true)
    }

    fn resize_scan(&mut self, step: i32, document: &FixedReading) -> Result<bool> {
        let FlowStyle::Scan {
            text_scale,
            spacing,
            columns,
            word_gap,
            alignment,
            direction,
        } = self.style
        else {
            return Ok(false);
        };
        let scale = (i32::from(text_scale) + step)
            .clamp(
                i32::from(MIN_SCAN_REFLOW_TEXT_SCALE),
                i32::from(MAX_SCAN_REFLOW_TEXT_SCALE),
            ) as u16;
        if scale == text_scale {
            return Ok(false);
        }
        self.refresh_layout(
            FlowStyle::Scan {
                text_scale: scale,
                spacing,
                columns,
                word_gap,
                alignment,
                direction,
            },
            document,
        )?;
        self.preferences.text_scale = Some(scale);
        Ok(true)
    }

    fn refresh_layout(&mut self, style: FlowStyle, document: &FixedReading) -> Result<()> {
        let (layout, top, location, frame) = self.relayout(&style, document, self.viewport)?;
        self.style = style;
        self.layout = layout;
        self.top = top;
        self.retained_location = Some(location);
        self.overlap_cue = None;
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
    ) -> Result<(FlowLayout, u32, Location, GrayImage)> {
        let location = self
            .retained_location
            .clone()
            .or_else(|| self.layout.location(self.top))
            .ok_or("The current reading passage is unavailable")?;
        let mut layout = style
            .layout_page(document, self.document_page, viewport)?
            .ok_or_else(|| format!("Page {} {}", self.document_page + 1, style.missing_text()))?;
        let top = layout
            .resolve(&location)
            .ok_or("The current reading passage is unavailable after changing the layout")?;
        let frame = layout.capture(top, viewport)?;
        Ok((layout, top, location, frame))
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
        let mut top = self.top;
        let distance = self.layout.step(self.zoom_overlap) as i32;
        let limit = self.layout.limit();
        let mut boundary = None;
        while let Some(step) = turns.next() {
            let next = top
                .saturating_add_signed(distance * step.signum() as i32)
                .min(limit);
            if next != top {
                top = next;
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
            let top = if step > 0 { 0 } else { layout.limit() };
            let frame = layout.capture(top, self.viewport)?;
            self.document_page = adjacent;
            self.layout = layout;
            self.top = top;
            self.retained_location = None;
            self.overlap_cue = None;
            self.frame = frame;
            self.boundary = None;
            // A single source-page load per refresh keeps rapid turns responsive.
            self.turns.pending.extend(turns);
            self.defer_refresh(now);
            return Ok(true);
        }

        let moved = top != self.top;
        if moved {
            self.frame = self.layout.capture(top, self.viewport)?;
            self.overlap_cue = if self.preferences.overlap_enabled {
                Marker::from_pan(self.layout.clip(), 0, self.top as i32 - top as i32)
            } else {
                None
            };
            self.top = top;
            self.retained_location = None;
        }
        let changed = moved || self.boundary != boundary;
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
        let controls = Controls::new(self.viewport, matches!(self.style, FlowStyle::Scan { .. }));
        let mut paint = PagePaint::full(inverted);
        paint.reading_guides = guides;
        if self.layout_draft.is_some() {
            paint.visible.end = controls.panel.y as u32;
        }
        let mut frame = self.frame.clone();
        if let Some(marker) = self.overlap_cue {
            marker.paint(&mut frame, self.preferences.marker);
        }
        draw_page_picture(canvas, frame, paint);
        canvas.button(
            controls.spacing,
            "Layout",
            self.layout_draft.is_some(),
            false,
        );
        if let Some(draft) = self.layout_draft {
            canvas.fill(controls.panel, 255);
            canvas.fill(Rect::new(0, controls.panel.y, controls.width, 1), HAIRLINE);
            canvas.text(
                Rect::new(38, controls.panel.y + 8, 400, 53),
                "Line spacing",
                CAPTION,
            );
            for (choice, label, rect) in controls.choices {
                canvas.button(rect, label, draft.spacing == choice, false);
            }
            if matches!(self.style, FlowStyle::Scan { .. }) {
                canvas.text(
                    Rect::new(640, controls.choices[0].2.y + 4, 108, 52),
                    "Columns",
                    CAPTION,
                );
                for (choice, label, rect) in controls.column_choices {
                    canvas.button(rect, label, draft.columns == choice, false);
                }
                canvas.text(
                    Rect::new(640, controls.word_gap_choices[0].2.y + 4, 108, 52),
                    "Word gap",
                    CAPTION,
                );
                for (choice, label, rect) in controls.word_gap_choices {
                    canvas.button(rect, label, draft.word_gap == choice, false);
                }
                canvas.text(
                    Rect::new(38, controls.alignment_choices[0].2.y + 4, 164, 52),
                    if draft.direction == FixedReflowDirection::TopToBottomRightToLeft {
                        "Vertical · top"
                    } else {
                        "Alignment"
                    },
                    CAPTION,
                );
                if draft.direction != FixedReflowDirection::TopToBottomRightToLeft {
                    for (choice, label, rect) in controls.alignment_choices {
                        canvas.button(rect, label, draft.alignment == choice, false);
                    }
                }
            }
            canvas.text(
                Rect::new(640, controls.direction_choices[0].2.y + 4, 108, 52),
                "Direction",
                CAPTION,
            );
            for (choice, label, rect) in controls.direction_choices {
                let selected = match choice {
                    DirectionChoice::Scan(direction) => draft.direction == direction,
                    DirectionChoice::Text(direction) => draft.text_direction == direction,
                };
                canvas.button(rect, label, selected, false);
            }
            let overlap_row = controls.overlap_choices[0].2.y;
            canvas.text(
                Rect::new(38, overlap_row + 4, 198, 52),
                "Page overlap",
                CAPTION,
            );
            for (enabled, label, rect) in controls.overlap_choices {
                canvas.button(rect, label, draft.overlap_enabled == enabled, false);
            }
            if draft.overlap_enabled {
                for (marker, rect) in controls.markers {
                    canvas.button(rect, marker.label(), marker == draft.marker, false);
                }
            }
            canvas.button(controls.cancel, "Cancel", false, false);
            canvas.button(controls.apply, "Apply", false, false);
        }
        canvas.fill(Rect::new(0, controls.footer_y, controls.width, 1), HAIRLINE);
        let step = self.layout.step(self.zoom_overlap);
        let style_label = match &self.style {
            FlowStyle::Text { .. } => "Text".to_owned(),
            FlowStyle::Scan { text_scale, .. } => {
                format!("Scan · {:.2}×", f32::from(*text_scale) / 100.0)
            }
        };
        let caption = format!(
            "Document page {} · {} {} of {}",
            self.document_page + 1,
            style_label,
            self.top.div_ceil(step) + 1,
            self.layout.limit().div_ceil(step) + 1,
        );
        if self.layout_draft.is_none()
            && let FlowStyle::Scan { text_scale, .. } = self.style
        {
            if text_scale > MIN_SCAN_REFLOW_TEXT_SCALE {
                canvas.button(controls.smaller, "-", false, false);
            }
            if text_scale < MAX_SCAN_REFLOW_TEXT_SCALE {
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

fn load_probe(
    title: &str,
    text: &str,
    fonts: &[PathBuf],
    settings: Settings,
    direction: FixedTextReflowDirection,
) -> Result<Probe> {
    with_storage(|| {
        Probe::from_text(title.into(), text, fonts, settings, direction).map_err(io::Error::other)
    })
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
        use crate::library::{DocumentFormat, FixedPreferences, FixedReflowColumns, MAX_SCAN_REFLOW_TEXT_SCALE};
        let name = if scan { "raster.pdf" } else { "vector.pdf" };
        // These fixtures were authored for single-column scrolling. The product
        // default is two columns, so pin the premise explicitly.
        // At the default 105% scale the 57-line OCR page fits one screen, so pin a
        // larger scale to keep the fixture passage scrollable.
        let preferences = FixedPreferences {
            fixed_reflow_columns: Some(FixedReflowColumns::One),
            fixed_reflow_text_scale: Some(MAX_SCAN_REFLOW_TEXT_SCALE),
            ..FixedPreferences::default()
        };
        let mut document = FixedReading::open(
            PAGE_HEIGHT,
            DocumentFormat::Pdf,
            &crate::test_fixture(&format!("scan-reflow/{name}")),
            1,
            &[],
            preferences,
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
            let original_top = flow.top;
            let origin = flow.layout.location(original_top).unwrap();
            assert!(original_top > 0);
            assert!(matches!(origin.anchor, FlowAnchor::Scan(_)) == scan);
            let original = flow.frame.pixels.clone();
            let pending = flow.deadline();
            let mut staged = flow.prepare_viewport(&document, landscape).unwrap();
            assert!(staged.matches(&flow));
            staged.exchange(&mut flow);
            assert_eq!(flow.viewport, landscape);
            assert_eq!(flow.document_page(), 1);
            assert_eq!(flow.retained_location.as_ref(), Some(&origin));
            assert_eq!(flow.layout.resolve(&origin), Some(flow.top));
            assert_eq!(flow.frame.width, landscape.dimensions().0);
            assert_eq!(flow.frame.height, landscape.page_height());
            assert_eq!(flow.deadline(), pending);
            let mut round_trip = flow.prepare_viewport(&document, portrait).unwrap();
            round_trip.exchange(&mut flow);
            assert_eq!(flow.top, original_top);
            assert_eq!(flow.retained_location.as_ref(), Some(&origin));
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
    fn fixed_reflow_layout_applies_ltr_and_rtl_to_embedded_and_ocr_text() {
        let tap =
            |rect: Rect| Input::Tap((rect.x + rect.w / 2) as f32, (rect.y + rect.h / 2) as f32);
        let fonts = [crate::test_fixture("fonts/NotoSans-Regular.ttf")];
        let settings = Settings {
            width: WIDTH,
            height: PAGE_HEIGHT,
            ..Settings::default()
        };

        for scan in [false, true] {
            let controls = Controls::new(ReadingViewport::PORTRAIT, scan);
            let document = authored_document(scan);
            let mut flow = TextFlow::open(&document, "Harbor", &fonts, settings, scan).unwrap();
            if scan {
                assert_eq!(flow.preferences().direction, FixedReflowDirection::LeftToRight);
            } else {
                assert_eq!(
                    flow.preferences().text_direction,
                    FixedTextReflowDirection::LeftToRight
                );
            }
            let source_position = document.history_point();
            let reading_location = flow.layout.location(flow.top).unwrap();

            assert!(flow
                .input_with_turn(tap(controls.spacing), None, &document)
                .unwrap());
            assert!(flow
                .input_with_turn(tap(controls.direction_choices[1].2), None, &document)
                .unwrap());
            if scan {
                // The authored OCR runs left to right. A right-to-left reflow of
                // that word order is refused, and the saved layout stays in place.
                assert!(flow
                    .input_with_turn(tap(controls.apply), None, &document)
                    .is_err());
                assert_eq!(flow.preferences().direction, FixedReflowDirection::LeftToRight);
                assert!(matches!(
                    &flow.style,
                    FlowStyle::Scan {
                        direction: FixedReflowDirection::LeftToRight,
                        ..
                    }
                ));
                assert_eq!(flow.layout.location(flow.top), Some(reading_location));
                assert_eq!(document.history_point(), source_position);
                return;
            }
            assert!(flow
                .input_with_turn(tap(controls.apply), None, &document)
                .unwrap());

            assert_eq!(
                flow.preferences().text_direction,
                FixedTextReflowDirection::RightToLeft
            );
            assert!(matches!(
                &flow.style,
                FlowStyle::Text {
                    direction: FixedTextReflowDirection::RightToLeft,
                    ..
                }
            ));
            assert_eq!(flow.layout.location(flow.top), Some(reading_location));
            assert_eq!(document.history_point(), source_position);
        }
    }

    #[test]
    fn fixed_text_reflow_layout_applies_vertical_right_to_left_direction() {
        let tap =
            |rect: Rect| Input::Tap((rect.x + rect.w / 2) as f32, (rect.y + rect.h / 2) as f32);
        let controls = Controls::new(ReadingViewport::PORTRAIT, false);
        let document = authored_document(false);
        let fonts = [crate::test_fixture("fonts/NotoSans-Regular.ttf")];
        let settings = Settings {
            width: WIDTH,
            height: PAGE_HEIGHT,
            ..Settings::default()
        };
        let mut flow = TextFlow::open(&document, "Harbor", &fonts, settings, false).unwrap();
        let reading_location = flow.layout.location(flow.top).unwrap();

        assert!(flow
            .input_with_turn(tap(controls.spacing), None, &document)
            .unwrap());
        assert!(flow
            .input_with_turn(tap(controls.direction_choices[2].2), None, &document)
            .unwrap());
        assert!(flow
            .input_with_turn(tap(controls.apply), None, &document)
            .unwrap());

        assert_eq!(
            flow.preferences().text_direction,
            FixedTextReflowDirection::TopToBottomRightToLeft
        );
        assert!(matches!(
            &flow.style,
            FlowStyle::Text {
                direction: FixedTextReflowDirection::TopToBottomRightToLeft,
                ..
            }
        ));
        assert_eq!(flow.layout.location(flow.top), Some(reading_location));
    }

    #[test]
    fn fixed_reflow_spacing_preserves_a_later_passage_and_restores_cancelled_layout() {
        let tap =
            |rect: Rect| Input::Tap((rect.x + rect.w / 2) as f32, (rect.y + rect.h / 2) as f32);
        for scan in [false, true] {
            let controls = Controls::new(ReadingViewport::PORTRAIT, scan);
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
            assert!(flow.layout.limit() > 0);
            flow.input(Input::PageForward, &document).unwrap();
            flow.tick(Instant::now() + Duration::from_secs(1), &document)
                .unwrap();
            let original_top = flow.top;
            assert_eq!(flow.document_page(), 1);
            assert!(original_top > 0);
            let origin = flow.layout.location(original_top).unwrap();
            assert_ne!(Some(&origin), flow.layout.location(0).as_ref());
            let original = flow.frame.pixels.clone();
            assert!(flow.input(tap(controls.spacing), &document).unwrap());
            assert!(flow.input(tap(controls.choices[0].2), &document).unwrap());
            assert_eq!(flow.frame.pixels, original);
            assert!(!flow.input(Input::PageForward, &document).unwrap());
            assert!(flow.input(Input::PageBack, &document).unwrap());
            assert_eq!(flow.top, original_top);
            assert_eq!(flow.line_spacing(), FixedReflowSpacing::Medium);
            assert_eq!(flow.frame.pixels, original);

            for choice in [2, 0, 1] {
                flow.input(tap(controls.spacing), &document).unwrap();
                flow.input(tap(controls.choices[choice].2), &document)
                    .unwrap();
                flow.input(tap(controls.apply), &document).unwrap();
                assert_eq!(flow.line_spacing(), controls.choices[choice].0);
                assert_eq!(flow.retained_location.as_ref(), Some(&origin));
                assert_eq!(flow.layout.resolve(&origin), Some(flow.top));
                assert_eq!(flow.document_page(), 1);
                assert_eq!(document.history_point(), position);
                if choice != 1 {
                    assert_ne!(flow.frame.pixels, original);
                }
            }
            assert_eq!(flow.top, original_top);
            assert_eq!(flow.frame.pixels, original);
            if scan {
                document.forget_ocr();
                flow.input(tap(controls.spacing), &document).unwrap();
                flow.input(tap(controls.choices[2].2), &document).unwrap();
                assert!(flow.input(tap(controls.apply), &document).is_err());
                assert_eq!(flow.line_spacing(), FixedReflowSpacing::Medium);
                assert_eq!(flow.retained_location.as_ref(), Some(&origin));
                assert_eq!(flow.frame.pixels, original);
                assert!(flow.input(tap(controls.cancel), &document).unwrap());
            }
        }
    }

    #[test]
    fn fixed_reflow_overlap_moves_the_same_distance_without_the_cue_for_both_providers() {
        use primitive_app::HEIGHT;

        let fonts = [crate::test_fixture("fonts/NotoSans-Regular.ttf")];
        let font_paths = fonts
            .iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        let mut canvas = Canvas::new(WIDTH, HEIGHT, &font_paths).unwrap();
        let settings = Settings::default();
        let tap =
            |rect: Rect| Input::Tap((rect.x + rect.w / 2) as f32, (rect.y + rect.h / 2) as f32);
        for scan in [false, true] {
            let document = authored_document(scan);
            let mut flow = TextFlow::open(&document, "Harbor", &fonts, settings, scan).unwrap();
            let physical = document.history_point();
            let expected = flow.layout.limit().min(flow.layout.step(40));
            assert!(expected > 0);
            flow.turns.pending.push(1);
            assert!(flow.tick(flow.deadline().unwrap(), &document).unwrap());
            assert_eq!(flow.top, expected);
            assert!(flow.overlap_cue.is_none());
            let plain = flow.frame.pixels.clone();
            canvas.begin();
            flow.draw(&mut canvas, None, false, None);
            let unmarked = canvas.finish(false);

            flow.turns.pending.push(-1);
            assert!(flow.tick(flow.deadline().unwrap(), &document).unwrap());
            assert_eq!(flow.top, 0);
            let [layout, enable, dashed, apply] = flow.overlap_test_targets();
            for button in [layout, enable, dashed, apply] {
                assert!(flow.input(tap(button), &document).unwrap());
            }
            flow.turns.pending.push(1);
            assert!(flow.tick(flow.deadline().unwrap(), &document).unwrap());
            assert_eq!(flow.top, expected);
            assert_eq!(flow.frame.pixels, plain);
            assert_eq!(flow.document_page(), document.page());
            assert_eq!(document.history_point(), physical);

            canvas.begin();
            flow.draw(&mut canvas, None, false, None);
            let marked = canvas.finish(false);
            let row = (flow.layout.clip().bottom() as usize - expected as usize) * WIDTH as usize;
            let end = row + 3 * WIDTH as usize;
            assert_ne!(&marked[row..end], &unmarked[row..end]);
            assert_eq!(&marked[..row], &unmarked[..row]);
            assert_eq!(&marked[end..], &unmarked[end..]);

            flow.turns.pending.push(-1);
            assert!(flow.tick(flow.deadline().unwrap(), &document).unwrap());
            assert_eq!(flow.top, 0);
            assert!(flow.overlap_cue.is_some());
            flow.turns.pending.push(-1);
            assert!(flow.tick(flow.deadline().unwrap(), &document).unwrap());
            if scan {
                assert_eq!(flow.document_page(), 1);
                assert!(flow.boundary.as_deref().unwrap().contains("needs OCR"));
            } else {
                assert_eq!(flow.document_page(), 0);
                assert!(flow.overlap_cue.is_none());
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
            direction: FixedTextReflowDirection::LeftToRight,
        };
        let viewport = ReadingViewport::PORTRAIT;
        let layout = style.layout_text(texts[0], viewport).unwrap().unwrap();
        let mut flow = TextFlow::with_layout(
            0,
            texts.len(),
            style,
            layout,
            viewport,
            40,
            FlowPreferences::default(),
        )
        .unwrap();
        let last = flow.layout.limit();
        let slices = last.div_ceil(flow.layout.step(40)) as usize;
        assert!(last > 0);
        let requested = RefCell::new(Vec::new());
        let mut lookup = |style: &FlowStyle, page: usize| {
            requested.borrow_mut().push(page);
            style.layout_text(texts[page], viewport)
        };

        flow.turns.pending.extend(std::iter::repeat_n(1, slices));
        let due = flow.deadline().unwrap();
        assert!(
            !flow
                .step(due - Duration::from_millis(1), &mut lookup)
                .unwrap()
        );
        assert!(flow.step(due, &mut lookup).unwrap());
        assert_eq!((flow.document_page(), flow.top), (0, last));
        assert!(requested.borrow().is_empty());

        flow.turns.pending.extend([1, 1]);
        let due = flow.deadline().unwrap();
        assert!(flow.step(due, &mut lookup).unwrap());
        assert_eq!((flow.document_page(), flow.top), (1, 0));
        assert_eq!(*requested.borrow(), [1]);

        let pixels = flow.frame.pixels.clone();
        let next = flow.deadline().unwrap();
        assert!(next >= due + PAGE_REFRESH_GAP);
        assert!(flow.step(next, &mut lookup).unwrap());
        assert_eq!((flow.document_page(), flow.top), (1, 0));
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
        assert_eq!((flow.document_page(), flow.top), (0, last));
        assert_eq!(*requested.borrow(), [1, 2, 0]);
        assert!(flow.boundary.is_none());

        flow.turns
            .pending
            .extend(std::iter::repeat_n(-1, slices + 1));
        assert!(flow.step(flow.deadline().unwrap(), &mut lookup).unwrap());
        assert_eq!((flow.document_page(), flow.top), (0, 0));
        assert_eq!(flow.boundary.as_deref(), Some("Start of document"));
        assert_eq!(*requested.borrow(), [1, 2, 0]);
    }
}
