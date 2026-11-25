//! Page slots for the fixed-layout book map. Browsing the map leaves document history alone.
use std::ops::Range;

use primitive_app::Change;
use primitive_ui::{CAPTION, Canvas, Input, Rect, TextStyle};

use super::navigation_layout::NavigationLayout;
use super::{LIST_X, Reader, Screen, bar_icons};

const COLUMNS: usize = 6;
const ROWS: usize = 8;
const CAPACITY: usize = ROWS * COLUMNS;
const TOP: i32 = 214;
const GAP: i32 = 10;
const HELP_PANEL: Rect = Rect::new(160, 420, 944, 900);
const HELP_DONE: Rect = Rect::new(868, 443, 197, 100);

fn help_button(layout: NavigationLayout) -> Rect {
    layout.horizontal(Rect::new(
        layout.width / 2 + 48,
        layout.height - 118,
        240,
        100,
    ))
}

fn return_to_book_button(layout: NavigationLayout) -> Rect {
    layout.horizontal(Rect::new(
        layout.width / 2 - 312,
        layout.height - 118,
        300,
        100,
    ))
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct MapWindow {
    start: usize,
    // A span of zero denotes the whole document.
    span: usize,
}

impl MapWindow {
    pub(super) fn focused(page: usize, total: usize) -> Self {
        if total <= CAPACITY {
            return Self::default();
        }
        let page = page.min(total - 1);
        Self {
            start: page / CAPACITY * CAPACITY,
            span: CAPACITY,
        }
    }

    fn pages(self, total: usize) -> Range<usize> {
        let length = if self.span == 0 { total } else { self.span };
        self.start..self.start.saturating_add(length).min(total)
    }

    fn slot_span(self, total: usize) -> usize {
        let span = if self.span == 0 { total } else { self.span };
        let needed = span.div_ceil(CAPACITY).max(1);
        // Align nested groups so swipes cover adjoining pages even in the last partial group.
        let mut quantum = 1;
        while quantum <= needed / CAPACITY {
            quantum *= CAPACITY;
        }
        needed.next_multiple_of(quantum)
    }

    fn slots(self, total: usize) -> impl Iterator<Item = Range<usize>> {
        let pages = self.pages(total);
        let step = self.slot_span(total);
        let end = pages.end;
        pages
            .step_by(step)
            .map(move |start| start..start.saturating_add(step).min(end))
    }

    fn adjacent(self, forward: bool, total: usize) -> Option<Self> {
        if self.span == 0 {
            return None;
        }
        let start = if forward {
            self.start
                .checked_add(self.span)
                .filter(|start| *start < total)?
        } else {
            self.start.checked_sub(self.span)?
        };
        Some(Self { start, ..self })
    }

    fn parent(self, total: usize) -> Self {
        let mut parent = Self::default();
        while self.span > 0 {
            let step = parent.slot_span(total);
            if step <= self.span {
                break;
            }
            parent = Self {
                start: (self.start / step) * step,
                span: step,
            };
        }
        parent
    }

    fn zoom(self, slot: &Range<usize>, total: usize) -> Self {
        Self {
            start: slot.start,
            span: self.slot_span(total),
        }
    }
}

pub(super) fn projected_cell(layout: NavigationLayout, index: usize) -> Rect {
    let (columns, rows) = if layout.width > layout.height {
        (ROWS, COLUMNS)
    } else {
        (COLUMNS, ROWS)
    };
    let width = (layout.width - 2 * LIST_X - (columns as i32 - 1) * GAP) / columns as i32;
    let height = (layout.height - 140 - TOP - (rows as i32 - 1) * GAP) / rows as i32;
    layout.horizontal(Rect::new(
        LIST_X + (index % columns) as i32 * (width + GAP),
        TOP + (index / columns) as i32 * (height + GAP),
        width,
        height,
    ))
}

#[cfg(test)]
pub(super) fn cell(index: usize) -> Rect {
    projected_cell(NavigationLayout::PORTRAIT, index)
}

fn range_label(range: &Range<usize>) -> String {
    if range.len() == 1 {
        (range.start + 1).to_string()
    } else {
        format!("{}–{}", range.start + 1, range.end)
    }
}

impl Reader {
    pub(super) fn draw_fixed_book_map(&self, canvas: &mut Canvas) {
        let Some(fixed) = &self.fixed else { return };
        let reading = &fixed.reading;
        let language = self.controls.interface_language;
        let layout = self.navigation_layout();
        let count = reading.count();
        let reading_page = reading.page();
        let ribbon = self.fixed_ribbon_map_focus();
        let focus = ribbon.map_or(reading_page, |(page, _)| page);
        let state = self.library.state(fixed.id);
        let window = self.map_window;
        let pages = window.pages(count);
        let sections = self.fixed_sections(fixed);
        let history_pages = |locations: &[super::HistoryLocation]| {
            locations
                .iter()
                .rev()
                .take(3)
                .enumerate()
                .filter_map(|(index, location)| {
                    let position = &location.position;
                    (position.chapter == 0 && position.anchor.is_none() && position.page < count)
                        .then_some((position.page, index + 1))
                })
                .collect::<Vec<_>>()
        };
        let previous_history = history_pages(&self.back);
        let next_history = history_pages(&self.forward);
        // Fixed-document visit keys are zero-based physical pages. Aggregate
        // only the visible bounded slots, not unrelated or undated totals.
        let mut slot_seconds = [0u64; CAPACITY];
        let mut slot_current_session = [false; CAPACITY];
        let now = crate::mark_time::now();
        if self.reading_history.options().enabled {
            let span = window.slot_span(count);
            for visit in self
                .reading_history
                .visits()
                .iter()
                .filter(|visit| visit.book == fixed.id)
            {
                let Ok(page) = usize::try_from(visit.page) else {
                    continue;
                };
                if !pages.contains(&page) {
                    continue;
                }
                let slot = (page - pages.start) / span;
                if let Some(seconds) = slot_seconds.get_mut(slot) {
                    *seconds = seconds.saturating_add(u64::from(visit.seconds));
                    slot_current_session[slot] |= now.is_some_and(|now| {
                        self.reading_history.visit_in_current_session(visit, now)
                    });
                }
            }
        }
        let max_seconds = slot_seconds.iter().copied().max().unwrap_or(0);
        let mut summary = format!(
            "{} {}   ·   {} {}",
            language.book_map_text("Pages"),
            range_label(&pages),
            language.book_map_text("Reading page"),
            reading_page + 1
        );
        if ribbon.is_some() {
            summary.push_str(&format!(
                "   ·   {} {}",
                language.book_map_text("Map focus"),
                focus + 1
            ));
        }
        if max_seconds > 0 {
            summary.push_str("   ·   ");
            summary.push_str(language.book_map_text("Reading time"));
            if slot_current_session.iter().any(|&current| current) {
                summary.push_str("   ·   ");
                summary.push_str(language.book_map_text("Gray: this session"));
            }
        }
        if previous_history
            .iter()
            .chain(&next_history)
            .any(|(page, _)| pages.contains(page))
        {
            summary.push_str("   ·   ");
            summary.push_str(language.book_map_text("Filled: previous; outline: next"));
        }
        canvas.text(layout.body(137, 65), &summary, CAPTION.lines(2));
        for (index, slot) in window.slots(count).enumerate() {
            let rect = projected_cell(layout, index);
            let previous = previous_history
                .iter()
                .find(|(page, _)| slot.contains(page))
                .map(|(_, number)| *number);
            let next = next_history
                .iter()
                .find(|(page, _)| slot.contains(page))
                .map(|(_, number)| *number);
            let selected = slot.contains(&focus);
            let hidden = reading
                .hidden_flow_map()
                .is_some_and(|flows| flows.count_in(0, slot.clone()) < slot.len());
            let pressed = canvas.control_pressed(rect, false);
            let ink = if pressed { 255 } else { 0 };
            canvas.panel(
                rect,
                16,
                if selected { 6 } else { 2 },
                if pressed {
                    0
                } else if hidden {
                    235
                } else {
                    255
                },
            );
            let browser_marker = ribbon.is_some_and(|(_, page)| slot.contains(&page));
            let reading_marker = slot.contains(&reading_page);
            let label_inset = if reading_marker { 38 } else { 12 };
            let label_width = (rect.w - label_inset - if browser_marker { 38 } else { 10 }).max(1);
            if reading_marker {
                // Keep the actual reading page visible even when the map's
                // focus came from a different Page Browser position.
                let x = rect.x + 12;
                let y = rect.y + 23;
                canvas.fill(Rect::new(x + 6, y, 6, 6), ink);
                canvas.fill(Rect::new(x + 3, y + 6, 12, 6), ink);
                canvas.fill(Rect::new(x, y + 12, 18, 6), ink);
            }
            canvas.text(
                Rect::new(rect.x + label_inset, rect.y + 10, label_width, 53),
                &range_label(&slot),
                TextStyle::new(29).bold().ink(ink),
            );
            if browser_marker {
                canvas.panel(
                    Rect::new(rect.right() - 31, rect.y + 27, 17, 17),
                    4,
                    3,
                    if pressed { 0 } else { 255 },
                );
            }
            let chapter = sections
                .at(sections.before(slot.start))
                .filter(|(page, _)| *page < slot.end)
                .map(|(_, title)| title);
            let folio = if slot.len() == 1 {
                reading
                    .page_label(slot.start)
                    .filter(|label| label != &(slot.start + 1).to_string())
            } else {
                None
            };
            if let Some(title) = chapter.or(folio.as_deref()) {
                canvas.text(
                    Rect::new(
                        rect.x + 12,
                        rect.y + 54,
                        (rect.w
                            - 24
                            - if previous.is_some() || next.is_some() {
                                66
                            } else {
                                0
                            })
                        .max(1),
                        rect.h - 93,
                    ),
                    title,
                    TextStyle::new(24).ink(ink).lines(2),
                );
            }
            // Filled and outlined numbered badges mirror the two history
            // directions. Grouped slots show only the nearest in each stack.
            let background = if pressed {
                0
            } else if hidden {
                235
            } else {
                255
            };
            if let Some(number) = previous {
                let badge = Rect::new(rect.right() - 72, rect.y + 64, 24, 24);
                canvas.fill(badge, ink);
                canvas.text(
                    badge,
                    &number.to_string(),
                    TextStyle::new(18).bold().center().ink(background),
                );
            }
            if let Some(number) = next {
                let badge = Rect::new(rect.right() - 43, rect.y + 64, 24, 24);
                canvas.fill(badge, ink);
                canvas.fill(badge.inset(2), background);
                canvas.text(
                    badge,
                    &number.to_string(),
                    TextStyle::new(18).bold().center().ink(ink),
                );
            }
            // The rail locates the current page inside a grouped tile.
            if slot.len() > 1 {
                let rail = Rect::new(rect.x + 12, rect.bottom() - 15, rect.w - 24, 5);
                canvas.fill(rail, if pressed { 160 } else { 186 });
                let divisions = slot.len().min((rail.w / 6) as usize);
                for i in 1..divisions {
                    canvas.fill(
                        Rect::new(
                            rail.x + (i * rail.w as usize / divisions) as i32,
                            rail.y,
                            2,
                            rail.h,
                        ),
                        if pressed { 0 } else { 255 },
                    );
                }
                if selected {
                    let x = ((focus - slot.start) as f64 / slot.len() as f64 * f64::from(rail.w))
                        as i32;
                    let width = (rail.w as f64 / slot.len() as f64).ceil() as i32;
                    let width = width.max(3).min(rail.w - x);
                    let x = if layout.mirrored {
                        rail.right() - x - width
                    } else {
                        rail.x + x
                    };
                    canvas.fill(Rect::new(x, rail.y - 4, width, 9), ink);
                }
            }
            if let Some(state) = state {
                let icon_y = rect.bottom() - 50;
                if state
                    .bookmarks
                    .iter()
                    .any(|mark| slot.contains(&mark.position.page))
                {
                    canvas.icon(
                        Rect::new(rect.x + 12, icon_y, 30, 30),
                        bar_icons::BOOKMARK,
                        ink,
                    );
                }
                let highlighted = state
                    .pdf_highlights
                    .iter()
                    .any(|mark| mark.parts().any(|(page, _, _)| slot.contains(&page)));
                let noted = state.bookmarks.iter().any(|mark| {
                    mark.position.chapter == 0
                        && mark.position.anchor.is_none()
                        && slot.contains(&mark.position.page)
                        && mark
                            .note
                            .as_ref()
                            .is_some_and(|note| !note.trim().is_empty())
                }) || state.pdf_highlights.iter().any(|mark| {
                    mark.note
                        .as_ref()
                        .is_some_and(|note| !note.trim().is_empty())
                        && mark.parts().any(|(page, _, _)| slot.contains(&page))
                });
                let separate_note = highlighted && noted && !hidden && rect.w >= 150;
                if highlighted && (!noted || separate_note) {
                    canvas.icon(
                        Rect::new(rect.x + 50, icon_y, 30, 30),
                        bar_icons::HIGHLIGHT,
                        ink,
                    );
                }
                if noted {
                    canvas.icon(
                        Rect::new(rect.x + if separate_note { 88 } else { 50 }, icon_y, 30, 30),
                        bar_icons::EDIT,
                        ink,
                    );
                }
            }
            if hidden {
                canvas.text(
                    Rect::new(rect.right() - 105, rect.bottom() - 50, 93, 34),
                    language.book_map_text("Hidden"),
                    TextStyle::new(22).ink(ink),
                );
            }
            let seconds = slot_seconds.get(index).copied().unwrap_or(0);
            if max_seconds > 0 && seconds > 0 {
                let max_height = (rect.h - 20).max(1);
                let height =
                    ((u128::from(seconds) * max_height as u128) / u128::from(max_seconds)) as i32;
                let height = height.clamp(6.min(max_height), max_height);
                let current = slot_current_session.get(index).copied().unwrap_or(false);
                let bar_ink = match (pressed, current) {
                    (true, true) => 170,
                    (true, false) => 255,
                    (false, true) => 140,
                    (false, false) => 0,
                };
                canvas.fill(
                    Rect::new(rect.right() - 12, rect.bottom() - 10 - height, 4, height),
                    bar_ink,
                );
            }
        }
        if window.adjacent(false, count).is_some() {
            canvas.button(
                layout.previous(),
                language.book_map_text("Previous"),
                false,
                false,
            );
        }
        if window.adjacent(true, count).is_some() {
            canvas.button(layout.next(), language.book_map_text("Next"), false, false);
        }
        canvas.button(
            return_to_book_button(layout),
            language.book_map_text("Return to book"),
            false,
            false,
        );
        canvas.button(
            help_button(layout),
            language.book_map_text("Help"),
            false,
            false,
        );
        if self.fixed_map_help {
            self.draw_fixed_map_help(canvas, layout);
        }
    }

    fn draw_fixed_map_help(&self, canvas: &mut Canvas, layout: NavigationLayout) {
        let language = self.controls.interface_language;
        let place = |rect| layout.centered(rect);
        canvas.popup(place(HELP_PANEL));
        canvas.text(
            place(Rect::new(196, 449, 624, 84)),
            language.book_map_text("Book map help"),
            TextStyle::new(42).bold(),
        );
        canvas.button(
            place(HELP_DONE),
            language.book_map_text("Done"),
            true,
            false,
        );
        for (index, line) in [
            "Tap a group to zoom. Tap one page to preview or read it.",
            "Page buttons move groups. Back returns to the prior view.",
            "Bookmark, marker and pencil icons indicate bookmarks, highlights and notes.",
            "Reading-time bars show visits; gray includes this session when known.",
            "Filled numbered badges are previous locations; outlines are next locations.",
            "Triangle: reading page. Small outline: browser focus. Shading: hidden pages.",
            "In a crowded slot, a note icon can replace a highlight icon.",
            "Return to book closes nested maps without moving the reading page.",
        ]
        .into_iter()
        .enumerate()
        {
            canvas.text(
                place(Rect::new(196, 555 + index as i32 * 82, 864, 76)),
                language.book_map_text(line),
                TextStyle::new(25).lines(2),
            );
        }
    }

    pub(super) fn fixed_map_help_input(&mut self, input: Input) -> Option<Change> {
        if matches!(input, Input::Sleep | Input::Wake) {
            return None;
        }
        let layout = self.navigation_layout();
        if self.fixed_map_help {
            if matches!(input, Input::PageBack)
                || matches!(input, Input::Tap(x, y) if layout.centered(HELP_DONE).contains(x, y)
                    || layout.back().contains(x, y) || !layout.centered(HELP_PANEL).contains(x, y))
            {
                self.fixed_map_help = false;
                return Some(Change::Partial);
            }
            return Some(Change::None);
        }
        if matches!(input, Input::Tap(x, y) if return_to_book_button(layout).contains(x, y)) {
            self.show(Screen::Fixed);
            return Some(Change::Full);
        }
        if matches!(input, Input::Tap(x, y) if help_button(layout).contains(x, y)) {
            self.fixed_map_help = true;
            return Some(Change::Partial);
        }
        None
    }

    pub(super) fn fixed_map_input(&mut self, input: Input) -> Change {
        let Some(fixed) = &self.fixed else {
            return Change::None;
        };
        let layout = self.navigation_layout();
        let count = fixed.reading.count();
        let window = self.map_window;
        let forward = match input {
            Input::Tap(x, y) if layout.back().contains(x, y) => {
                if window.span == 0 {
                    self.show(Screen::Fixed);
                } else {
                    self.map_window = window.parent(count);
                }
                return Change::Partial;
            }
            Input::Tap(x, y) if layout.next().contains(x, y) => true,
            Input::Tap(x, y) if layout.previous().contains(x, y) => false,
            Input::Tap(x, y) => {
                let Some(slot) = window.slots(count).enumerate().find_map(|(i, pages)| {
                    projected_cell(layout, i).contains(x, y).then_some(pages)
                }) else {
                    return Change::None;
                };
                if slot.len() == 1 {
                    return match self.select_book_map_page(
                        super::page_overview::MapOrigin::Fixed(window),
                        slot.start,
                    ) {
                        Ok(()) if self.screen == Screen::PageOverview => Change::Partial,
                        Ok(()) => Change::Full,
                        Err(err) => self.report(err),
                    };
                }
                self.map_window = window.zoom(&slot, count);
                return Change::Partial;
            }
            Input::PageForward => true,
            Input::PageBack => false,
            Input::Swipe { dx, dy, .. } => {
                if dy.abs() >= dx.abs() {
                    dy < 0.0
                } else {
                    (dx < 0.0) ^ layout.mirrored
                }
            }
            _ => return Change::None,
        };
        if window.span == 0 && count > CAPACITY {
            let focus = fixed.reading.page();
            if let Some(active) = window.slots(count).find(|pages| pages.contains(&focus)) {
                let at = window.zoom(&active, count);
                self.map_window = at.adjacent(forward, count).unwrap_or(at);
                return Change::Partial;
            }
        }
        if let Some(next) = window.adjacent(forward, count) {
            self.map_window = next;
            Change::Partial
        } else {
            Change::None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_nested_detail_restores_its_parent_even_after_moving_to_a_sibling() {
        let total = 12543;
        let root = MapWindow::default();
        let upper = root.slots(total).nth(4).unwrap();
        let group = root.zoom(&upper, total);
        let leaf = group.zoom(&group.slots(total).nth(7).unwrap(), total);
        let next = leaf.adjacent(true, total).unwrap();
        assert_eq!(next.parent(total).pages(total), upper);
        assert_eq!(next.parent(total).parent(total).pages(total), 0..total);
        assert!(next.pages(total).start == leaf.pages(total).end);
    }

    #[test]
    fn whole_map_covers_each_page_once_and_preserves_the_last_partial_window() {
        for count in [1, 48, 49, 1066, 12543] {
            let root = MapWindow::default();
            let slots: Vec<_> = root.slots(count).collect();
            assert!(slots.len() <= CAPACITY);
            assert!(slots.iter().cloned().flatten().eq(0..count));
            let last = slots.last().unwrap();
            if last.len() > 1 {
                let detail = root.zoom(last, count);
                assert_eq!(detail.pages(count), *last);
                assert!(detail.adjacent(true, count).is_none());
                assert_eq!(
                    detail.adjacent(false, count).unwrap().pages(count).end,
                    last.start
                );
            }
        }
    }
}
