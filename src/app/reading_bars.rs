//! Compact controls layered over the current page without changing its layout.

use primitive_app::{App, Change, HEIGHT, WIDTH};
use primitive_ui::{CAPTION, Canvas, Input, Rect, TextStyle};

use super::{Action, Item, Reader, Screen, text_size_items};
use crate::session::{PAGE_HEIGHT, PagePaint};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum BarPanel {
    Pages,
    Text,
    View,
    Scan,
}

enum Target {
    Invoke(Action),
    Toggle(BarPanel),
    Exit,
}

struct Button {
    rect: Rect,
    label: String,
    target: Target,
}

struct Bars {
    top: Rect,
    bottom: Rect,
    panel: Option<Rect>,
    title: Rect,
    buttons: Vec<Button>,
}

impl Bars {
    fn new(large: bool) -> Self {
        let width = WIDTH as i32;
        let top_height = if large { 98 } else { 80 };
        let bottom_height = if large { 104 } else { 86 };
        Self {
            top: Rect::new(0, 0, width, top_height),
            bottom: Rect::new(0, HEIGHT as i32 - bottom_height, width, bottom_height),
            panel: None,
            title: Rect::new(171, 6, width - 690, top_height - 12),
            buttons: Vec::new(),
        }
    }

    fn push(&mut self, rect: Rect, label: impl Into<String>, target: Target) {
        self.buttons.push(Button {
            rect,
            label: label.into(),
            target,
        });
    }

    fn group(&mut self, items: Vec<Item>, large: bool) {
        if items.is_empty() {
            return;
        }
        let columns = items.len().min(4);
        let rows = items.len().div_ceil(columns);
        let row_height = if large { 78 } else { 68 };
        let height = rows as i32 * row_height + 14;
        let panel = Rect::new(0, self.bottom.y - height, WIDTH as i32, height);
        self.panel = Some(panel);
        let step = (panel.w - 16) / columns as i32;
        for (index, item) in items.into_iter().enumerate() {
            let x = panel.x + 8 + index as i32 % columns as i32 * step;
            let y = panel.y + 7 + (index / columns) as i32 * row_height;
            self.push(
                Rect::new(x, y, step, row_height),
                item.title,
                Target::Invoke(item.action),
            );
        }
    }

    fn button_at(&mut self, x: f32, y: f32) -> Option<Target> {
        let index = self
            .buttons
            .iter()
            .position(|button| button.rect.contains(x, y))?;
        Some(self.buttons.swap_remove(index).target)
    }
}

impl Reader {
    fn reading_bars(&self) -> Bars {
        let large = self.controls.large_toolbar;
        let mut bars = Bars::new(large);
        let width = WIDTH as i32;
        let top = |x, w| Rect::new(x, 5, w, bars.top.h - 10);
        let top_buttons = [
            (
                top(12, 148),
                "Library",
                Target::Invoke(Action::Show(Screen::Library)),
            ),
            (
                top(width - 500, 145),
                "Pages",
                Target::Toggle(BarPanel::Pages),
            ),
            (
                top(width - 355, 189),
                "Controls",
                Target::Invoke(Action::Show(Screen::ReadingControls)),
            ),
            (top(width - 166, 154), "Exit", Target::Exit),
        ];
        for (rect, label, target) in top_buttons {
            bars.push(rect, label, target);
        }

        let fixed = self.fixed.as_ref().map(|document| &document.reading);
        let has_scan = fixed.is_some_and(|reading| {
            reading.supports_crop() || reading.supports_text() || reading.supports_scan_contrast()
        });
        let fixed_book = fixed.is_some();
        let supports_text = fixed.is_none_or(|reading| reading.supports_text());
        let tabs = [
            fixed
                .is_none_or(|reading| !reading.outline().is_empty())
                .then_some((
                    "Contents",
                    Target::Invoke(Action::Show(if fixed_book {
                        Screen::FixedContents
                    } else {
                        Screen::Contents
                    })),
                )),
            Some((
                "Bookmarks",
                Target::Invoke(Action::Show(if fixed_book {
                    Screen::FixedBookmarks
                } else {
                    Screen::Bookmarks
                })),
            )),
            supports_text.then_some((
                "Highlights",
                Target::Invoke(Action::Show(Screen::Highlights)),
            )),
            supports_text.then_some(("Find", Target::Invoke(Action::Show(Screen::Find)))),
            Some(("Go to", Target::Invoke(Action::Show(Screen::GoToPage)))),
            Some(if fixed_book {
                ("View", Target::Toggle(BarPanel::View))
            } else {
                ("Text", Target::Toggle(BarPanel::Text))
            }),
            has_scan.then_some(("Scan", Target::Toggle(BarPanel::Scan))),
        ];
        let columns = if has_scan { 7 } else { 6 };
        for (index, tab) in tabs.into_iter().enumerate() {
            if let Some((label, target)) = tab {
                let x = index as i32 * width / columns;
                let right = (index as i32 + 1) * width / columns;
                bars.push(
                    Rect::new(x + 3, bars.bottom.y + 5, right - x - 6, bars.bottom.h - 10),
                    label,
                    target,
                );
            }
        }
        if let Some(panel) = self.reading_panel {
            bars.group(self.reading_bar_items(panel), large);
        }
        bars
    }

    fn reading_bar_items(&self, panel: BarPanel) -> Vec<Item> {
        let mut items = Vec::new();
        match panel {
            BarPanel::Pages => {
                items.push(Item::new("Thumbnails", Action::Overview));
                items.push(Item::new("Add bookmark", Action::AddBookmark));
                if self
                    .active
                    .as_ref()
                    .is_some_and(|active| !active.reading.links().is_empty())
                {
                    items.push(Item::new("Links", Action::Show(Screen::Links)));
                }
                if !self.back.is_empty() {
                    items.push(Item::new("History", Action::Show(Screen::LocationHistory)));
                    items.push(Item::new("Previous location", Action::ReturnToLocation));
                }
                if !self.hits.is_empty() || !self.pdf_hits.is_empty() {
                    items.push(Item::new("Matches", Action::Show(Screen::Matches)));
                }
            }
            BarPanel::Text => {
                if let Some(active) = &self.active {
                    let scroll = active.reading.config.scroll;
                    items.extend(text_size_items(active.reading.config.settings));
                    if !active.reading.probe.is_vertical() {
                        items.push(Item::new(
                            if scroll { "Page turns" } else { "Scroll" },
                            Action::ReadingMode(!scroll),
                        ));
                    }
                    items.push(Item::new("Text settings", Action::Show(Screen::Typography)));
                }
            }
            BarPanel::View => {
                if let Some(fixed) = &self.fixed {
                    let reading = &fixed.reading;
                    if !reading.is_spread() {
                        items.extend([
                            Item::new("Fit page", Action::FitPage(false)),
                            Item::new("Fit width", Action::FitPage(true)),
                            Item::new("Fit height", Action::FitHeight),
                        ]);
                    }
                    items.push(Item::new("Rotate", Action::RotatePage));
                    if reading.supports_spreads() {
                        let spread = reading.is_spread();
                        items.push(Item::new(
                            if spread { "One page" } else { "Two pages" },
                            Action::ComicSpread(!spread),
                        ));
                    }
                    if reading.supports_continuous() {
                        let scroll = reading.is_continuous();
                        items.push(Item::new(
                            if scroll { "Page turns" } else { "Scroll" },
                            Action::FixedScroll(!scroll),
                        ));
                    }
                    if reading.supports_rtl() {
                        let rtl = reading.preferences().right_to_left;
                        items.push(Item::new(
                            if rtl {
                                "Left to right"
                            } else {
                                "Right to left"
                            },
                            Action::ReversePageDirection,
                        ));
                    }
                }
            }
            BarPanel::Scan => {
                if let Some(fixed) = &self.fixed {
                    let reading = &fixed.reading;
                    if reading.supports_scan_contrast() {
                        let higher = reading.scan_contrast();
                        items.push(Item::new(
                            if higher {
                                "Normal contrast"
                            } else {
                                "Higher contrast"
                            },
                            Action::ScanContrast(!higher),
                        ));
                    }
                    if reading.supports_crop() {
                        items.push(Item::new("Crop", Action::CropPage));
                        items.push(Item::new("Columns", Action::ScanColumns));
                        if self.ocr_assets.is_some() && self.ocr.is_none() {
                            items.push(Item::new("OCR", Action::RecognizeText));
                        }
                    }
                    if reading.supports_text() {
                        items.push(Item::new("Read text", Action::ReadFixedText));
                        items.push(Item::new("Reflow scan", Action::ReflowScan));
                    }
                }
            }
        }
        items
    }

    pub(super) fn draw_reading_tools(&self, canvas: &mut Canvas) {
        let bars = self.reading_bars();
        for rect in [Some(bars.top), Some(bars.bottom), bars.panel]
            .into_iter()
            .flatten()
        {
            canvas.fill(rect, 255);
            let edge = if rect.y == 0 { rect.h - 2 } else { rect.y };
            canvas.fill(Rect::new(rect.x, edge, rect.w, 2), 166);
        }

        let size = if self.controls.large_toolbar { 34 } else { 28 };
        let title = self
            .active
            .as_ref()
            .map(|book| book.reading.probe.title.as_str())
            .or_else(|| self.fixed.as_ref().map(|book| book.entry.title.as_str()))
            .unwrap_or("Reader");
        canvas.text(
            Rect::new(
                bars.title.x,
                bars.title.y + 15,
                bars.title.w,
                bars.title.h - 15,
            ),
            self.notice.as_deref().unwrap_or(title),
            CAPTION.ink(70),
        );
        for button in bars.buttons {
            let selected =
                matches!(button.target, Target::Toggle(panel) if Some(panel) == self.reading_panel);
            let rect = button.rect;
            if canvas.control_pressed(rect, false) {
                canvas.fill(rect, 218);
            }
            if selected {
                canvas.fill(
                    Rect::new(rect.x + 14, rect.y + rect.h - 3, rect.w - 28, 3),
                    0,
                );
            }
            canvas.text(
                Rect::new(rect.x, rect.y + (rect.h - size) / 2, rect.w, size + 12),
                &button.label,
                TextStyle::new(size).bold().center(),
            );
        }
    }

    pub(super) fn page_paint(&self) -> PagePaint {
        let inverted = self.controls.night_pages;
        if !matches!(self.screen, Screen::BookTools | Screen::FixedTools) {
            return PagePaint::full(inverted);
        }
        let bars = self.reading_bars();
        PagePaint {
            // Keep text fragments at the panel edges from peeking behind the controls.
            visible: bars.top.h as u32..bars.panel.unwrap_or(bars.bottom).y as u32,
            inverted,
        }
    }

    pub(super) fn reveal_reading_tools(&mut self, x: f32, y: f32, screen: Screen) -> bool {
        let middle = WIDTH as f32 / 2.0;
        let panel =
            if (x - middle).abs() <= 150.0 && (PAGE_HEIGHT as f32..HEIGHT as f32).contains(&y) {
                Some(if screen == Screen::FixedTools {
                    BarPanel::View
                } else {
                    BarPanel::Text
                })
            } else if (WIDTH as f32 / 3.0..=WIDTH as f32 * 2.0 / 3.0).contains(&x)
                && (0.0..PAGE_HEIGHT as f32 * 0.55).contains(&y)
            {
                None
            } else {
                return false;
            };
        self.show(screen);
        self.reading_panel = panel;
        true
    }

    pub(super) fn reading_tools_input(&mut self, input: Input) -> Change {
        let reading_screen = if self.screen == Screen::FixedTools {
            Screen::Fixed
        } else {
            Screen::Book
        };
        match input {
            Input::Tap(x, y) => {
                let mut bars = self.reading_bars();
                match bars.button_at(x, y) {
                    Some(Target::Exit) => self.exit(),
                    Some(Target::Toggle(panel)) => {
                        self.reading_panel = (self.reading_panel != Some(panel)).then_some(panel);
                        Change::Partial
                    }
                    Some(Target::Invoke(action)) => match self.activate(action) {
                        Ok(()) => Change::Partial,
                        Err(error) => self.report(error),
                    },
                    None if !bars.top.contains(x, y)
                        && !bars.bottom.contains(x, y)
                        && !bars.panel.is_some_and(|panel| panel.contains(x, y)) =>
                    {
                        if self.reading_panel.take().is_none() {
                            self.show(reading_screen);
                        }
                        Change::Partial
                    }
                    None => Change::None,
                }
            }
            Input::PageBack | Input::PageForward | Input::Swipe { .. } => {
                self.show(reading_screen);
                match self.input(input) {
                    Change::None => Change::Partial,
                    change => change,
                }
            }
            _ => Change::None,
        }
    }

    #[cfg(test)]
    pub(super) fn reading_button(&self, label: &str) -> Rect {
        self.reading_bars()
            .buttons
            .into_iter()
            .find(|button| button.label == label)
            .unwrap_or_else(|| panic!("Missing reading control {label}"))
            .rect
    }
}
