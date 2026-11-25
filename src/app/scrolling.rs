//! Global scrolling options and contact routing for the continuous providers.
use super::{
    Action, Canvas, Change, Input, Item, LIST_Y, MenuActivation, NumberField, Reader, Reading,
    Rect, Result, Screen,
};
use crate::library::{FixedPreferences as View, ReadingPosition};
use primitive_app::{App, PanPhase, TouchPan};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

pub(super) const MAX_DELAY_MS: u16 = 2000;
const DELAY_MINUS: Rect = Rect::new(48, LIST_Y + 286, 352, 112);
const DELAY_DEFAULT: Rect = Rect::new(424, LIST_Y + 286, 416, 112);
const DELAY_PLUS: Rect = Rect::new(864, LIST_Y + 286, 352, 112);

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Method {
    #[default]
    Classic,
    Turbo,
    OnRelease,
}

impl Method {
    pub(crate) fn is_default(&self) -> bool {
        *self == Self::Classic
    }

    fn label(self) -> &'static str {
        match self {
            Self::Classic => "Classic scrolling",
            Self::Turbo => "Turbo scrolling",
            Self::OnRelease => "On-release scrolling",
        }
    }

    fn help(self) -> &'static str {
        match self {
            Self::Classic => "Move the page with your finger",
            Self::Turbo => "Move farther as your finger moves away from its starting point",
            Self::OnRelease => "Move the page when you lift your finger",
        }
    }
}

#[derive(Clone, Copy)]
struct Book {
    screen: Screen,
    id: u64,
}

impl Book {
    fn current(self, reader: &Reader) -> bool {
        reader.screen == self.screen && reader.open_book_id() == Some(self.id)
    }
}

enum Origin {
    Reflow(ReadingPosition, f32),
    Fixed(usize, View),
}

struct Contact {
    start: (f32, f32),
    book: Book,
    origin: Origin,
    last_y: f32,
    accepted: bool,
    changed: bool,
}

#[derive(Default)]
pub(super) struct Pan(Option<Contact>);

impl Pan {
    pub(super) fn busy(&self) -> bool {
        self.0.is_some()
    }
}

impl Reader {
    fn scroll_view_available(&self) -> bool {
        self.active
            .as_ref()
            .is_some_and(|active| active.reading.scroll_history_top().is_some())
            || self
                .fixed
                .as_ref()
                .is_some_and(|fixed| fixed.reading.is_continuous())
    }

    pub(super) fn scroll_settings_item(&self) -> Item {
        if self.scroll_view_available() {
            Item::new("Scrolling", Action::Show(Screen::Scrolling))
                .detail(self.controls.scroll_method.label())
        } else {
            Item::new("Scrolling", Action::Heading).detail("Available in continuous view")
        }
    }

    pub(super) fn scroll_settings_items(&self) -> Vec<Item> {
        let language = self.controls.interface_language;
        let mut items = [Method::Classic, Method::Turbo, Method::OnRelease]
            .into_iter()
            .map(|method| {
                Item::new(
                    language.scrolling_settings_text(method.label()),
                    Action::SetReadingControls(crate::reading_controls::ReadingControls {
                        scroll_method: method,
                        ..self.controls
                    }),
                )
                .detail(if method == self.controls.scroll_method {
                    format!(
                        "{} · {}",
                        language.scrolling_settings_text("Selected"),
                        language.scrolling_settings_text(method.help())
                    )
                } else {
                    language.scrolling_settings_text(method.help()).into()
                })
            })
            .collect::<Vec<_>>();
        items.push(
            Item::new(
                language.scrolling_settings_text("Activation delay"),
                Action::Show(Screen::NumberingValue(NumberField::ScrollDelay)),
            )
            .detail(format!("{} ms", self.scroll_delay_ms())),
        );
        items
    }

    pub(super) fn scroll_default_delay_ms(&self) -> u16 {
        if self.controls.menu_activation == MenuActivation::Tap {
            0
        } else {
            500
        }
    }

    pub(super) fn scroll_delay_ms(&self) -> u16 {
        self.controls
            .scroll_delay_ms
            .filter(|delay| *delay <= MAX_DELAY_MS)
            .unwrap_or_else(|| self.scroll_default_delay_ms())
    }

    pub(super) fn set_scroll_delay_draft(&mut self, delay: u16) {
        self.editor.text = delay.to_string().chars().collect();
        self.editor.caret = self.editor.text.len();
        self.notice = None;
    }

    fn scroll_delay_buttons(&self) -> [Rect; 3] {
        self.numeric_layout()
            .steps([DELAY_MINUS, DELAY_DEFAULT, DELAY_PLUS], false)
    }

    fn scroll_delay_draft(&self) -> Result<u16> {
        self.editor
            .string()
            .trim()
            .parse::<u16>()
            .ok()
            .filter(|value| *value <= MAX_DELAY_MS)
            .ok_or_else(|| "Enter a delay from 0 to 2,000 ms".into())
    }

    pub(super) fn scroll_delay_input(&mut self, input: Input) -> Option<Change> {
        if self.screen != Screen::NumberingValue(NumberField::ScrollDelay) {
            return None;
        }
        let (x, y, amount) = match input {
            Input::Tap(x, y) => (x, y, 100i16),
            Input::LongTap(x, y) => (x, y, 500),
            _ => return None,
        };
        let [decrease, default, increase] = self.scroll_delay_buttons();
        let current = self.scroll_delay_draft();
        let draft = if default.contains(x, y) {
            Ok(self.scroll_default_delay_ms())
        } else if decrease.contains(x, y) {
            current.map(|value| value.saturating_add_signed(-amount))
        } else if increase.contains(x, y) {
            current.map(|value| value.saturating_add_signed(amount).min(MAX_DELAY_MS))
        } else {
            return None;
        };
        Some(match draft {
            Ok(delay) => {
                self.set_scroll_delay_draft(delay);
                Change::Partial
            }
            Err(error) => self.report(error),
        })
    }

    pub(super) fn apply_scroll_delay(&mut self) -> Result<()> {
        let value = self.scroll_delay_draft()?;
        self.activate(Action::SetReadingControls(
            crate::reading_controls::ReadingControls {
                scroll_delay_ms: (value != self.scroll_default_delay_ms()).then_some(value),
                ..self.controls
            },
        ))?;
        self.show(Screen::Scrolling);
        Ok(())
    }

    pub(super) fn draw_scroll_delay(&self, canvas: &mut Canvas) {
        if self.screen == Screen::NumberingValue(NumberField::ScrollDelay) {
            let [decrease, default, increase] = self.scroll_delay_buttons();
            canvas.button(decrease, "-100", false, false);
            canvas.button(default, "Default", false, false);
            canvas.button(increase, "+100", false, false);
        }
    }

    fn scroll_contact(&self, start: (f32, f32)) -> Option<Contact> {
        if !self.touch_selection_enabled()
            || !self.stats_awake
            || self.selection.is_some()
            || self.pdf_selection.is_some()
            || self.selection_mode
            || self.touch_hold.is_some()
            || self.fixed_search_fit_active()
            || self.closing_profile.is_some()
            || self.opening_ask_visible()
            || self.pending_opening_profiles.is_some()
            || self.back_exit_confirmation_open()
            || !(0.0..self.reading_viewport().dimensions().0 as f32).contains(&start.0)
            || !(0.0..self.reading_viewport().page_height() as f32).contains(&start.1)
        {
            return None;
        }
        let origin = match self.screen {
            Screen::Book => {
                let reading = &self.active.as_ref()?.reading;
                Origin::Reflow(super::position(reading), reading.scroll_history_top()?)
            }
            Screen::Fixed => {
                let reading = &self.fixed.as_ref()?.reading;
                if !reading.is_continuous() {
                    return None;
                }
                Origin::Fixed(reading.page(), reading.preferences())
            }
            _ => return None,
        };
        Some(Contact {
            start,
            book: Book {
                screen: self.screen,
                id: self.open_book_id()?,
            },
            origin,
            last_y: 0.0,
            accepted: false,
            changed: false,
        })
    }

    pub(super) fn finish_interrupted_scroll(&mut self) {
        if self
            .scroll_pan
            .0
            .take()
            .is_some_and(|contact| contact.changed && contact.book.current(self))
        {
            self.record_position();
        }
    }

    pub(super) fn continuous_touch_pan(&mut self, pan: TouchPan) -> Change {
        let mut contact = self.scroll_pan.0.take();
        if contact
            .as_ref()
            .is_some_and(|contact| contact.start != pan.start || !contact.book.current(self))
        {
            if contact
                .as_ref()
                .is_some_and(|contact| contact.changed && contact.book.current(self))
            {
                self.record_position();
            }
            contact = None;
        }
        let Input::Swipe { dx, dy, .. } = pan.swipe() else {
            unreachable!("touch pan always provides a swipe displacement")
        };
        let cancelled = pan.phase == PanPhase::Cancel;
        if !dx.is_finite() || !dy.is_finite() {
            return Change::None;
        }
        if contact.is_some() && !self.touch_selection_enabled() {
            if contact.is_some_and(|contact| contact.changed) {
                self.record_position();
            }
            return if pan.phase == PanPhase::Release {
                self.swipe(pan.swipe(), pan.start.0)
            } else {
                Change::None
            };
        }
        let horizontal = dx.abs() > dy.abs();
        let reserved = self.scroll_swipe_reserved(pan.swipe(), pan.start.0);
        if cancelled || (pan.phase == PanPhase::Release && (horizontal || reserved)) {
            let restored = if let Some(contact) = contact.filter(|contact| contact.changed) {
                if let Err(error) = self.restore_scroll_origin(contact.origin) {
                    self.record_position();
                    return self.report(error);
                }
                Change::Partial
            } else {
                Change::None
            };
            self.sync_auto_turn(Instant::now());
            if cancelled {
                return restored;
            }
            return match self.swipe(pan.swipe(), pan.start.0) {
                Change::None => restored,
                change => change,
            };
        }
        let mut contact = match contact.or_else(|| self.scroll_contact(pan.start)) {
            Some(contact) => contact,
            None if pan.phase == PanPhase::Release => return self.swipe(pan.swipe(), pan.start.0),
            None => return Change::None,
        };
        if pan.phase == PanPhase::Move {
            self.auto_turn.activity(Instant::now());
            self.auto_turn.pause();
            contact.accepted |= !horizontal
                && !reserved
                && pan.elapsed >= Duration::from_millis(u64::from(self.scroll_delay_ms()));
        }
        let release = pan.phase == PanPhase::Release;
        let step = if contact.accepted && !horizontal && !reserved {
            let step = match self.controls.scroll_method {
                Method::Classic => dy - contact.last_y,
                Method::Turbo if !release => dy,
                Method::OnRelease if release => dy,
                _ => 0.0,
            };
            contact.last_y = dy;
            step
        } else {
            0.0
        };
        let change = match self.move_scroll_contact(step, &contact.origin) {
            Ok(true) => {
                contact.changed = true;
                Change::Partial
            }
            Ok(false) => Change::None,
            Err(error) => {
                if contact.changed {
                    self.record_position();
                }
                self.sync_auto_turn(Instant::now());
                return self.report(error);
            }
        };
        if release {
            if contact.changed {
                self.notice = None;
                self.reset_back_resistance();
                self.clear_stale_selection();
                if self.fixed.is_some() {
                    self.nearby.clear();
                    self.search_result = None;
                    self.pdf_result_regions.clear();
                }
                self.record_position();
            }
            self.auto_turn.activity(Instant::now());
            self.sync_auto_turn(Instant::now());
        } else {
            self.scroll_pan.0 = Some(contact);
        }
        change
    }

    fn scroll_swipe_reserved(&self, input: Input, start_x: f32) -> bool {
        self.reading_edge_swipe_direction(input, start_x).is_some()
            || self
                .gestures
                .reserves_swipe(input, start_x, self.reading_viewport())
    }

    fn move_scroll_contact(&mut self, dy: f32, origin: &Origin) -> Result<bool> {
        if dy == 0.0 {
            return Ok(false);
        }
        match origin {
            Origin::Reflow(..) => Ok(self
                .active
                .as_mut()
                .ok_or("No open book")?
                .reading
                .drag_scroll(-dy)?
                == Change::Partial),
            Origin::Fixed(..) => self
                .fixed
                .as_mut()
                .ok_or("No open document")?
                .reading
                .drag_scroll(0.0, dy),
        }
    }

    fn restore_scroll_origin(&mut self, origin: Origin) -> Result<()> {
        match origin {
            Origin::Reflow(position, top) => {
                let reading = &mut self.active.as_mut().ok_or("No open book")?.reading;
                *reading = Reading::restore_with_scroll_top(
                    reading.config.clone(),
                    position.chapter,
                    position.page,
                    position.anchor.as_ref(),
                    Some(top),
                )?;
            }
            Origin::Fixed(page, view) => self
                .fixed
                .as_mut()
                .ok_or("No open document")?
                .reading
                .restore_view_at(page, view)?,
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
