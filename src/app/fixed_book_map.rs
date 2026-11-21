//! Page slots for the fixed-layout book map. Browsing the map leaves document history alone.
use std::ops::Range;

use primitive_app::{Change, HEIGHT, WIDTH};
use primitive_ui::{CAPTION, Canvas, Input, Rect, TextStyle};

use super::{LIST_X, NEXT, PREV, Reader, Screen, bar_icons};

const COLUMNS: usize = 6;
const ROWS: usize = 8;
const CAPACITY: usize = ROWS * COLUMNS;
const TOP: i32 = 214;
const GAP: i32 = 10;

#[derive(Clone, Copy, Default)]
pub(super) struct MapWindow {
    start: usize,
    // A span of zero denotes the whole document.
    span: usize,
}

impl MapWindow {
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

pub(super) fn cell(index: usize) -> Rect {
    let width = (WIDTH as i32 - 2 * LIST_X - (COLUMNS as i32 - 1) * GAP) / COLUMNS as i32;
    let height = (HEIGHT as i32 - 140 - TOP - (ROWS as i32 - 1) * GAP) / ROWS as i32;
    Rect::new(
        LIST_X + (index % COLUMNS) as i32 * (width + GAP),
        TOP + (index / COLUMNS) as i32 * (height + GAP),
        width,
        height,
    )
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
        let count = reading.count();
        let focus = reading.page();
        let state = self.library.state(fixed.id);
        let window = self.map_window;
        let pages = window.pages(count);
        let sections = self.fixed_sections(fixed);
        let summary = format!(
            "Pages {}   ·   Reading page {}",
            range_label(&pages),
            focus + 1
        );
        canvas.text(
            Rect::new(LIST_X, 137, WIDTH as i32 - 2 * LIST_X, 65),
            &summary,
            CAPTION,
        );
        for (index, slot) in window.slots(count).enumerate() {
            let rect = cell(index);
            let selected = slot.contains(&focus);
            let pressed = canvas.control_pressed(rect, false);
            let ink = if pressed { 255 } else { 0 };
            canvas.panel(
                rect,
                16,
                if selected { 6 } else { 2 },
                if pressed { 0 } else { 255 },
            );
            canvas.text(
                Rect::new(rect.x + 12, rect.y + 10, rect.w - 22, 53),
                &range_label(&slot),
                TextStyle::new(29).bold().ink(ink),
            );
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
                    Rect::new(rect.x + 12, rect.y + 54, rect.w - 24, rect.h - 93),
                    title,
                    TextStyle::new(24).ink(ink).lines(2),
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
                    canvas.fill(
                        Rect::new(rail.x + x, rail.y - 4, width.max(3).min(rail.w - x), 9),
                        ink,
                    );
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
                if state
                    .pdf_highlights
                    .iter()
                    .any(|mark| mark.parts().any(|(page, _, _)| slot.contains(&page)))
                {
                    canvas.icon(
                        Rect::new(rect.x + 50, icon_y, 30, 30),
                        bar_icons::HIGHLIGHT,
                        ink,
                    );
                }
            }
        }
        if window.adjacent(false, count).is_some() {
            canvas.button(PREV, "Previous", false, false);
        }
        if window.adjacent(true, count).is_some() {
            canvas.button(NEXT, "Next", false, false);
        }
    }

    pub(super) fn fixed_map_input(&mut self, input: Input) -> Change {
        let Some(fixed) = &self.fixed else {
            return Change::None;
        };
        let count = fixed.reading.count();
        let window = self.map_window;
        let forward = match input {
            Input::Tap(x, y) if primitive_ui::back_button(WIDTH as i32).contains(x, y) => {
                if window.span == 0 {
                    self.show(Screen::Fixed);
                } else {
                    self.map_window = window.parent(count);
                }
                return Change::Partial;
            }
            Input::Tap(x, y) if NEXT.contains(x, y) => true,
            Input::Tap(x, y) if PREV.contains(x, y) => false,
            Input::Tap(x, y) => {
                let Some(slot) = window
                    .slots(count)
                    .enumerate()
                    .find_map(|(i, pages)| cell(i).contains(x, y).then_some(pages))
                else {
                    return Change::None;
                };
                if slot.len() == 1 {
                    return match self.follow_fixed(slot.start, None) {
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
                    dx < 0.0
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
