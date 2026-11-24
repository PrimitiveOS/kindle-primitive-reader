//! Local-time gate and clock editor for automatic opening profiles.
use super::opening_rules::{Command, Page};
use super::{Action, CAPTION, Canvas, Change, Input, Item, Name, Reader, Rect, Result, Screen};
use crate::reading_profiles::opening::TimeWindow;
use primitive_ui::TextStyle;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::app) enum Endpoint {
    Start,
    End,
}

impl Endpoint {
    fn label(self) -> &'static str {
        match self {
            Self::Start => "Start time",
            Self::End => "End time",
        }
    }

    fn select(self, time: TimeWindow) -> u16 {
        match self {
            Self::Start => time.start,
            Self::End => time.end,
        }
    }

    fn replace(self, time: &mut TimeWindow, minute: u16) {
        match self {
            Self::Start => time.start = minute,
            Self::End => time.end = minute,
        }
    }
}

fn clock(minute: u16) -> String {
    format!("{:02}:{:02}", minute / 60, minute % 60)
}

pub(super) fn window_label(time: TimeWindow) -> String {
    if time.start == time.end {
        "All day".into()
    } else {
        format!("{} - {}", clock(time.start), clock(time.end))
    }
}

fn current_minute() -> Option<u16> {
    let time = crate::mark_time::local_parts(crate::mark_time::now()?)?;
    (time.hour * 60 + time.minute).try_into().ok()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::app) enum Part {
    Hour,
    Minute,
}

impl Part {
    pub(in crate::app) fn title(self) -> &'static str {
        match self {
            Self::Hour => "Enter hour",
            Self::Minute => "Enter minute",
        }
    }

    pub(in crate::app) fn range_label(self) -> &'static str {
        match self {
            Self::Hour => "0 to 23",
            Self::Minute => "0 to 59",
        }
    }

    fn max(self) -> u16 {
        match self {
            Self::Hour => 23,
            Self::Minute => 59,
        }
    }

    fn value(self, minute: u16) -> u16 {
        match self {
            Self::Hour => minute / 60,
            Self::Minute => minute % 60,
        }
    }

    fn step(self, minute: u16, direction: i32, held: bool) -> u16 {
        let hold_step = match self {
            Self::Hour => 4,
            Self::Minute => 10,
        };
        let value = i32::from(self.value(minute)) + direction * if held { hold_step } else { 1 };
        let value = match value {
            value if value < 0 => self.max(),
            value if value > i32::from(self.max()) => 0,
            value => value as u16,
        };
        self.replace(minute, value)
    }

    fn replace(self, minute: u16, value: u16) -> u16 {
        match self {
            Self::Hour => value * 60 + minute % 60,
            Self::Minute => minute / 60 * 60 + value,
        }
    }
}

const PANEL: Rect = Rect::new(86, 566, 1092, 548);
const CANCEL: Rect = Rect::new(130, 970, 472, 100);
const SAVE: Rect = Rect::new(662, 970, 472, 100);
const VALUES: [(Part, &str, i32); 2] = [(Part::Hour, "Hour", 130), (Part::Minute, "Minute", 662)];
const STEPS: [(Part, i32, Rect); 4] = [
    (Part::Hour, -1, Rect::new(130, 786, 126, 100)),
    (Part::Hour, 1, Rect::new(476, 786, 126, 100)),
    (Part::Minute, -1, Rect::new(662, 786, 126, 100)),
    (Part::Minute, 1, Rect::new(1008, 786, 126, 100)),
];

impl Reader {
    pub(in crate::app) fn profile_opening_minute(&self) -> Option<u16> {
        self.profiles
            .iter()
            .any(|profile| profile.opening_time().is_some())
            .then(current_minute)
            .flatten()
    }

    pub(in crate::app) fn prepare_opening_time(&mut self, index: usize, endpoint: Endpoint) {
        if matches!(self.screen, Screen::ProfileName(Name::OpeningTime(i, bound, _)) if i == index && bound == endpoint)
        {
            return;
        }
        self.profile_opening_time_draft = self
            .profiles
            .get(index)
            .and_then(|profile| profile.opening_time())
            .map(|time| endpoint.select(time))
            .or_else(current_minute)
            .unwrap_or(0);
    }

    pub(super) fn opening_time_items(&self, index: usize) -> Vec<Item> {
        let time = self
            .profiles
            .get(index)
            .and_then(|profile| profile.opening_time());
        let mut items: Vec<_> = [Endpoint::Start, Endpoint::End]
            .into_iter()
            .map(|endpoint| {
                Item::new(
                    endpoint.label(),
                    Action::Show(Screen::ReadingProfileOpening(
                        index,
                        Page::TimeEdit(endpoint),
                    )),
                )
                .detail(time.map_or_else(|| "Unset".into(), |time| clock(endpoint.select(time))))
            })
            .collect();
        if time.is_some() {
            items.push(Item::new(
                "Clear time window",
                Action::OpeningProfileRule(index, Command::ClearTimeWindow),
            ));
        }
        items
    }

    pub(super) fn save_opening_time(
        &mut self,
        index: usize,
        time: Option<TimeWindow>,
    ) -> Result<()> {
        let mut profiles = self.profiles.clone();
        profiles
            .get_mut(index)
            .ok_or("That profile is unavailable")?
            .set_opening_time(time);
        self.commit_profiles(profiles)?;
        self.notice = None;
        Ok(())
    }

    pub(super) fn set_opening_time_value(
        &mut self,
        mode: Name,
        part: Part,
        value: &str,
    ) -> Result<()> {
        let value = value
            .parse::<u16>()
            .ok()
            .filter(|&value| value <= part.max())
            .ok_or_else(|| format!("Enter a whole number from {}", part.range_label()))?;
        self.profile_opening_time_draft = part.replace(self.profile_opening_time_draft, value);
        self.show(mode.back());
        Ok(())
    }

    pub(in crate::app) fn draw_opening_time(&self, canvas: &mut Canvas) {
        let Screen::ReadingProfileOpening(_, Page::TimeEdit(endpoint)) = self.screen else {
            return;
        };
        let layout = self.navigation_layout();
        let place = |rect| layout.centered(rect);
        canvas.popup(place(PANEL));
        canvas.text(
            place(Rect::new(130, 599, 1004, 88)),
            endpoint.label(),
            TextStyle::new(48).bold(),
        );
        let minute = self.profile_opening_time_draft;
        for (part, title, left) in VALUES {
            canvas.text(
                place(Rect::new(left, 725, 472, 52)),
                title,
                CAPTION.center(),
            );
            canvas.button(
                place(Rect::new(left + 142, 786, 188, 100)),
                &format!("{:02}", part.value(minute)),
                false,
                false,
            );
        }
        for (_, direction, rect) in STEPS {
            canvas.button(
                place(rect),
                if direction < 0 { "-" } else { "+" },
                false,
                false,
            );
        }
        if let Some(notice) = &self.notice {
            canvas.text(
                place(Rect::new(130, 893, 1004, 69)),
                notice,
                CAPTION.lines(2),
            );
        }
        canvas.button(place(CANCEL), "Cancel", false, false);
        canvas.button(place(SAVE), "Set time", true, false);
    }

    pub(in crate::app) fn opening_time_input(&mut self, input: Input) -> Option<Change> {
        let Screen::ReadingProfileOpening(index, Page::TimeEdit(endpoint)) = self.screen else {
            return None;
        };
        if matches!(input, Input::Sleep | Input::Wake) {
            return None;
        }
        let layout = self.navigation_layout();
        let back = Screen::ReadingProfileOpening(index, Page::TimeWindow);
        if input == Input::PageBack {
            self.show(back);
            return Some(Change::Partial);
        }
        let (x, y, held) = match input {
            Input::Tap(x, y) => (x, y, false),
            Input::LongTap(x, y) => (x, y, true),
            _ => return Some(Change::None),
        };
        if !held
            && (layout.centered(CANCEL).contains(x, y) || !layout.centered(PANEL).contains(x, y))
        {
            self.show(back);
        } else if !held && layout.centered(SAVE).contains(x, y) {
            let minute = self.profile_opening_time_draft;
            let mut window = self
                .profiles
                .get(index)
                .and_then(|profile| profile.opening_time())
                .unwrap_or(TimeWindow {
                    start: minute,
                    end: minute,
                });
            endpoint.replace(&mut window, minute);
            if let Err(error) = self.save_opening_time(index, Some(window)) {
                return Some(self.report(error));
            }
            self.show(back);
        } else if !held
            && let Some((part, _, _)) = VALUES.iter().find(|(_, _, left)| {
                layout
                    .centered(Rect::new(left + 142, 786, 188, 100))
                    .contains(x, y)
            })
        {
            self.show(Screen::ProfileName(Name::OpeningTime(
                index, endpoint, *part,
            )));
        } else if let Some((part, direction, _)) = STEPS
            .iter()
            .find(|(_, _, rect)| layout.centered(*rect).contains(x, y))
        {
            self.profile_opening_time_draft =
                part.step(self.profile_opening_time_draft, *direction, held);
            self.notice = None;
        } else {
            return Some(Change::None);
        }
        Some(Change::Partial)
    }
}

#[cfg(test)]
mod tests;
