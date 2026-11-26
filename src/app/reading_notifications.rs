//! Optional feedback from successful reading controls.
use super::{Action, Item, Reader, Screen};
use crate::{Result, mark_time};
use primitive_ui::{Canvas, Input, Rect, TextStyle};
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

const HISTORY_LIMIT: usize = 20;
pub(super) const HISTORY_ROW: usize = Source::ALL.len();
const LIFETIME: Duration = Duration::from_secs(2);
const TOAST_HALO: i32 = 8;

#[derive(Clone, Copy)]
#[repr(u8)]
pub(super) enum Source {
    Icons = 0x01,
    Toggles = 0x02,
    Fine = 0x04,
    More = 0x08,
    Progress = 0x10,
    Dispatcher = 0x20,
    Other = 0x40,
}

impl Source {
    const DEFAULT: u8 =
        Self::Fine as u8 | Self::More as u8 | Self::Progress as u8 | Self::Dispatcher as u8;
    const ALL: [(Self, &'static str); 7] = [
        (Self::Icons, "Bottom menu icons"),
        (Self::Toggles, "Bottom menu toggles"),
        (Self::Fine, "Bottom menu ± buttons"),
        (Self::More, "Bottom menu custom values"),
        (Self::Progress, "Bottom menu progress bars"),
        (Self::Dispatcher, "Gestures and profiles"),
        (Self::Other, "Other sources"),
    ];

    fn mask(saved: Option<u8>) -> u8 {
        saved.unwrap_or(Self::DEFAULT)
    }

    pub(super) fn enabled(self, saved: Option<u8>) -> bool {
        Self::mask(saved) & self as u8 != 0
    }
}

#[derive(Default)]
pub(super) struct Notifications {
    history: VecDeque<Message>,
    visible_until: Option<Instant>,
    repaint_at: Option<Instant>,
}

struct Message {
    time: Option<String>,
    text: String,
}

impl Notifications {
    fn show(&mut self, text: String, now: Instant) {
        let time = mark_time::now()
            .and_then(mark_time::local_parts)
            .map(|clock| format!("{:02}:{:02}:{:02}", clock.hour, clock.minute, clock.second));
        if self.history.len() == HISTORY_LIMIT {
            self.history.pop_back();
        }
        self.history.push_front(Message { text, time });
        self.visible_until = Some(now + LIFETIME);
        self.repaint_at = None;
    }

    pub(super) fn handle_input(&mut self, input: Input) {
        if input == Input::Sleep {
            self.visible_until = None;
            self.repaint_at = None;
        } else if matches!(
            input,
            Input::Tap(..)
                | Input::LongTap(..)
                | Input::HoldStart(..)
                | Input::Swipe { .. }
                | Input::PageForward
                | Input::PageBack
        ) {
            self.dismiss();
        }
    }

    pub(super) fn dismiss(&mut self) {
        if self.visible_until.take().is_some() {
            // A handled action normally paints its own changes. Schedule removal
            // also when the same input otherwise needs no redraw.
            self.repaint_at = Some(Instant::now());
        }
    }

    pub(super) fn deadline(&self) -> Option<Instant> {
        self.visible_until.into_iter().chain(self.repaint_at).min()
    }

    pub(super) fn tick(&mut self, now: Instant) -> bool {
        let expired = self.visible_until.take_if(|until| now >= *until).is_some();
        let repaint = self.repaint_at.take_if(|at| now >= *at).is_some();
        expired || repaint
    }

    pub(super) fn repaint(&mut self) {
        // Negotiated viewport acceptance already presented the staged frame.
        self.repaint_at = Some(Instant::now());
    }
}

impl Reader {
    /// A one-shot advisory, separate from optional reading-control feedback.
    pub(super) fn announce_annotation_transfer(&mut self, message: String) {
        self.notifications.show(message, Instant::now());
    }

    pub(super) fn notify_reading(&mut self, source: Source, message: impl Into<String>) -> bool {
        if source.enabled(self.controls.notification_sources) {
            self.notifications.show(message.into(), Instant::now());
            true
        } else {
            false
        }
    }

    pub(super) fn reading_notifications_item(&self) -> Item {
        let enabled = Source::ALL
            .iter()
            .filter(|(source, _)| source.enabled(self.controls.notification_sources))
            .count();
        Item::new("Notifications", Action::Show(Screen::ReadingNotifications)).detail(
            self.controls
                .interface_language
                .notification_source_count(enabled, Source::ALL.len()),
        )
    }

    pub(super) fn notification_items(&self, screen: Screen) -> Option<Vec<Item>> {
        let language = self.controls.interface_language;
        let items = match screen {
            Screen::ReadingNotifications => Source::ALL
                .into_iter()
                .map(|(source, label)| {
                    Item::new(
                        language.notification_settings_text(label),
                        Action::ToggleNotificationSource(source),
                    )
                })
                .chain([Item::new(
                    language.notification_settings_text("Show past notifications"),
                    Action::Show(Screen::PastNotifications),
                )])
                .collect(),
            Screen::PastNotifications if self.notifications.history.is_empty() => {
                vec![Item::new(
                    language.notification_settings_text("No notifications yet"),
                    Action::Heading,
                )]
            }
            Screen::PastNotifications => self
                .notifications
                .history
                .iter()
                .map(|message| Item {
                    title: message.text.clone(),
                    detail: message.time.clone(),
                    action: Action::Heading,
                    folder: false,
                })
                .collect(),
            _ => return None,
        };
        Some(items)
    }

    pub(super) fn toggle_notification_source(&mut self, source: Source) -> Result<()> {
        let current = Source::mask(self.controls.notification_sources);
        let mask = current ^ source as u8;
        let controls = crate::reading_controls::ReadingControls {
            notification_sources: (mask != Source::DEFAULT).then_some(mask),
            ..self.controls
        };
        controls.save(&self.controls_path)?;
        self.controls = controls;
        self.notice = None;
        Ok(())
    }

    pub(super) fn draw_reading_notification(&self, canvas: &mut Canvas) {
        let Some((rect, message)) = self.reading_notification() else {
            return;
        };
        // A modal popup would intercept pressed feedback across the screen.
        canvas.fill(rect.inset(-TOAST_HALO), 255);
        canvas.panel(rect, 24, 4, 255);
        canvas.text(
            Rect::new(rect.x + 22, rect.y + 13, rect.w - 44, rect.h - 21),
            message,
            TextStyle::new(28).bold().center().lines(2),
        );
    }

    fn reading_notification(&self) -> Option<(Rect, &str)> {
        self.notifications.visible_until?;
        let message = &self.notifications.history.front()?.text;
        let width = self.reading_viewport().dimensions().0 as i32;
        let panel_width = (width - 80).min(520);
        let height = if message.chars().count() > 34 { 96 } else { 68 };
        Some((
            Rect::new((width - panel_width) / 2, 28, panel_width, height),
            message,
        ))
    }

    pub(super) fn reading_notification_cover(&self) -> Option<Rect> {
        self.reading_notification()
            .map(|(rect, _)| rect.inset(-TOAST_HALO))
    }
}

#[cfg(test)]
mod tests;
