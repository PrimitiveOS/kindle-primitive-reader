use super::{
    Action, CAPTION, Canvas, Change, Command, Input, Item, Options, Page, Reader, Rect, Screen,
    TextStyle,
};

const WEEKDAYS: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];
const WEEK_START_CHOICES: [u8; 4] = [5, 6, 0, 1];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::app) enum Choice {
    WeekStart,
    BooksPerDay,
}

impl Choice {
    pub(super) fn title(self) -> &'static str {
        match self {
            Self::WeekStart => "Calendar week starts",
            Self::BooksPerDay => "Books per calendar day",
        }
    }
}

pub(super) fn timeline_clock(minutes: u16) -> String {
    format!("{:02}:{:02}", minutes / 60, minutes % 60)
}

#[derive(Clone, Copy)]
enum Part {
    Hour,
    Minute,
}

impl Part {
    fn value(self, minutes: u16) -> u16 {
        match self {
            Self::Hour => minutes / 60,
            Self::Minute => minutes % 60,
        }
    }

    fn step(self, minutes: u16, direction: i32, held: bool) -> u16 {
        let (maximum, amount) = match (self, held) {
            (Self::Hour, false) => (23, 1),
            (Self::Hour, true) => (23, 4),
            (Self::Minute, false) => (50, 10),
            (Self::Minute, true) => (50, 30),
        };
        let value = i32::from(self.value(minutes)) + direction * amount;
        let value = if value < 0 {
            maximum
        } else if value > maximum {
            0
        } else {
            value
        } as u16;
        match self {
            Self::Hour => value * 60 + minutes % 60,
            Self::Minute => minutes / 60 * 60 + value,
        }
    }
}

const POPUP: Rect = Rect::new(86, 550, 1092, 580);
const CANCEL: Rect = Rect::new(130, 988, 472, 86);
const SAVE: Rect = Rect::new(662, 988, 472, 86);
const STEPS: [(Part, i32, Rect); 4] = [
    (Part::Hour, -1, Rect::new(130, 804, 126, 90)),
    (Part::Hour, 1, Rect::new(476, 804, 126, 90)),
    (Part::Minute, -1, Rect::new(662, 804, 126, 90)),
    (Part::Minute, 1, Rect::new(1008, 804, 126, 90)),
];

impl Reader {
    pub(super) fn calendar_setting_items(&self) -> [Item; 4] {
        let language = self.controls.interface_language;
        let options = self.reading_history.options();
        let choice = |choice: Choice| {
            Item::new(
                language.history_calendar_text(choice.title()),
                Action::ReadingHistory(Command::Show(Page::CalendarChoice(choice))),
            )
        };
        [
            choice(Choice::WeekStart)
                .detail(language.history_calendar_text(WEEKDAYS[usize::from(options.week_start)])),
            Item::new(
                language.history_calendar_text("Daily timeline starts at"),
                Action::ReadingHistory(Command::EditTimelineStart),
            )
            .detail(timeline_clock(options.day_start)),
            Item::new(
                language.history_calendar_text("Also use in calendar view"),
                Action::ReadingHistory(Command::Configure(Options {
                    shift_calendar: Some(!options.calendar_uses_shift()),
                    ..options
                })),
            )
            .detail(if options.calendar_uses_shift() {
                language.history_calendar_text("On")
            } else {
                language.history_calendar_text("Off")
            }),
            choice(Choice::BooksPerDay).detail(options.calendar_book_spans.to_string()),
        ]
    }

    pub(super) fn calendar_choice_items(&self, choice: Choice) -> Vec<Item> {
        let language = self.controls.interface_language;
        let options = self.reading_history.options();
        let item = |title: String, next: Options| {
            let item = Item::new(title, Action::ReadingHistory(Command::Configure(next)));
            if next == options {
                item.detail(language.history_calendar_text("Selected"))
            } else {
                item
            }
        };
        match choice {
            Choice::WeekStart => WEEK_START_CHOICES
                .iter()
                .map(|&day| {
                    item(
                        language
                            .history_calendar_text(WEEKDAYS[usize::from(day)])
                            .into(),
                        Options {
                            week_start: day,
                            ..options
                        },
                    )
                })
                .collect(),
            Choice::BooksPerDay => (1..=5)
                .map(|count| {
                    item(
                        count.to_string(),
                        Options {
                            calendar_book_spans: count,
                            ..options
                        },
                    )
                })
                .collect(),
        }
    }

    pub(in crate::app) fn draw_history_timeline_start(&self, canvas: &mut Canvas) {
        let Some(draft) = self
            .history_view
            .timeline_start_draft
            .filter(|_| self.screen == Screen::ReadingHistory(Page::Options))
        else {
            return;
        };
        let language = self.controls.interface_language;
        let layout = self.navigation_layout();
        let place = |rect| layout.centered(rect);
        canvas.popup(place(POPUP));
        canvas.text(
            place(Rect::new(130, 580, 1004, 78)),
            language.history_calendar_text("Daily timeline starts at"),
            TextStyle::new(48).bold(),
        );
        canvas.text(
            place(Rect::new(130, 667, 1004, 67)),
            language.history_calendar_text(
                "Start after midnight to keep late-night reading with the previous evening.",
            ),
            CAPTION.lines(2),
        );
        for (part, title, left) in [(Part::Hour, "Hour", 130), (Part::Minute, "Minute", 662)] {
            canvas.text(
                place(Rect::new(left, 748, 472, 45)),
                language.history_calendar_text(title),
                CAPTION.center(),
            );
            canvas.text(
                place(Rect::new(left + 142, 804, 188, 90)),
                &format!("{:02}", part.value(draft)),
                TextStyle::new(56).bold().center(),
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
                place(Rect::new(130, 903, 1004, 70)),
                notice,
                CAPTION.lines(2),
            );
        }
        canvas.button(
            place(CANCEL),
            language.history_calendar_text("Cancel"),
            false,
            false,
        );
        canvas.button(
            place(SAVE),
            language.history_calendar_text("Set time"),
            true,
            false,
        );
    }

    pub(super) fn timeline_start_input(&mut self, input: Input) -> Option<Change> {
        let draft = self
            .history_view
            .timeline_start_draft
            .filter(|_| self.screen == Screen::ReadingHistory(Page::Options))?;
        if matches!(input, Input::Sleep | Input::Wake) {
            return None;
        }
        if input == Input::PageBack {
            self.history_view.timeline_start_draft = None;
            self.notice = None;
            return Some(Change::Partial);
        }
        let (x, y, held) = match input {
            Input::Tap(x, y) => (x, y, false),
            Input::LongTap(x, y) => (x, y, true),
            _ => return Some(Change::None),
        };
        let layout = self.navigation_layout();
        if !held && layout.centered(CANCEL).contains(x, y) {
            self.history_view.timeline_start_draft = None;
        } else if !held && layout.centered(SAVE).contains(x, y) {
            let options = self.reading_history.options();
            if let Err(error) = self.activate_reading_history(Command::Configure(Options {
                day_start: draft,
                shift_calendar: Some(options.calendar_uses_shift()),
                ..options
            })) {
                return Some(self.report(error));
            }
            self.history_view.timeline_start_draft = None;
        } else if let Some((part, direction, _)) = STEPS
            .iter()
            .find(|(_, _, rect)| layout.centered(*rect).contains(x, y))
        {
            self.history_view.timeline_start_draft = Some(part.step(draft, *direction, held));
        } else {
            return Some(Change::None);
        }
        self.notice = None;
        Some(Change::Partial)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::{Fixture, capture_test_screen};
    use crate::reading_stats::history::date::{Date, Zone};
    use primitive_app::App;
    use std::time::{Duration, Instant};

    fn tap(rect: Rect) -> Input {
        Input::Tap((rect.x + rect.w / 2) as f32, (rect.y + rect.h / 2) as f32)
    }

    fn select(app: &mut Reader, label: &str) {
        let row = app
            .list_window()
            .1
            .iter()
            .position(|item| item.title == label)
            .unwrap_or_else(|| panic!("Missing {label}"));
        assert_eq!(
            app.input(tap(app.list_geometry().row(row))),
            Change::Partial
        );
    }

    #[test]
    fn calendar_settings_edit_cancel_and_persist_independent_options_without_moving_the_book() {
        let fixture = Fixture::new();
        let fonts = [
            crate::test_fixture("fonts/NotoSans-Regular.ttf"),
            crate::test_fixture("fonts/NotoSans-Bold.ttf"),
        ];
        let mut app = Reader::open(&fixture.root, &fixture.state, fonts.to_vec()).unwrap();
        app.open_path(&fixture.book).unwrap();
        let position = app.current_position().unwrap();
        app.activate_reading_history(Command::Open(None)).unwrap();
        app.activate_reading_history(Command::Show(Page::Options))
            .unwrap();

        select(&mut app, "Daily timeline starts at");
        assert_eq!(app.input(tap(STEPS[0].2)), Change::Partial);
        assert_eq!(app.history_view.timeline_start_draft, Some(23 * 60));
        assert_eq!(app.input(tap(CANCEL)), Change::Partial);
        assert_eq!(app.reading_history.options().day_start, 0);
        select(&mut app, "Daily timeline starts at");
        let Input::Tap(x, y) = tap(STEPS[1].2) else {
            unreachable!()
        };
        assert_eq!(app.input(Input::LongTap(x, y)), Change::Partial);
        for _ in 0..2 {
            assert_eq!(app.input(tap(STEPS[3].2)), Change::Partial);
        }
        assert_eq!(app.history_view.timeline_start_draft, Some(260));
        capture_test_screen(&app, &fonts, "READER_STATS_ORIGIN_SCREEN", false);
        assert_eq!(app.input(tap(SAVE)), Change::Partial);
        assert_eq!(app.reading_history.options().day_start, 260);
        assert!(!app.reading_history.options().calendar_uses_shift());
        select(&mut app, "Also use in calendar view");
        assert!(app.reading_history.options().calendar_uses_shift());
        select(&mut app, "Calendar week starts");
        let weekdays: Vec<_> = app
            .list_window()
            .1
            .into_iter()
            .map(|item| item.title)
            .collect();
        assert_eq!(weekdays, ["Friday", "Saturday", "Sunday", "Monday"]);
        select(&mut app, "Friday");
        select(&mut app, "Books per calendar day");
        select(&mut app, "2");
        let options = app.reading_history.options();
        assert!(options.calendar_uses_shift());
        assert_eq!(options.week_start, 5);
        assert_eq!(options.calendar_book_spans, 2);

        let month = Date::today().add_months(-1).unwrap();
        let friday = 8 + (5 + 7 - month.weekday()) % 7;
        let mut index = 0;
        for (day, visits) in [
            (friday, vec![(1, 100), (2, 80), (3, 65)]),
            (friday + 1, vec![(2, 110), (1, 75), (4, 36)]),
            (friday + 2, vec![(1, 110), (4, 78), (3, 45)]),
        ] {
            let day = Date { day, ..month };
            for (book, seconds) in visits {
                let at = Instant::now() + Duration::from_secs(index * 300);
                let wall = Zone::Local.range(day, 600).unwrap().0 + (index * 180) as i64;
                app.reading_history.remember_title(
                    book,
                    ["", "Harbor notes", "Coast atlas", "North trail", "Bird log"][book as usize],
                );
                app.reading_history
                    .set_page_at(Some((book, 1)), at, Some(wall));
                app.reading_history.set_page_at(
                    None,
                    at + Duration::from_secs(seconds),
                    Some(wall + seconds as i64),
                );
                index += 1;
            }
        }
        app.activate_reading_history(Command::Show(Page::Calendar))
            .unwrap();
        assert_eq!(app.input(Input::PageBack), Change::Partial);
        assert_eq!(app.history_view.month, month);
        capture_test_screen(&app, &fonts, "READER_STATS_SPANS_SCREEN", false);
        let offset = (month.weekday() + 7 - options.week_start) % 7;
        let day = Date {
            day: friday,
            ..month
        };
        assert_eq!(
            app.input(tap(super::super::cell(usize::from(offset + day.day - 1)))),
            Change::Partial
        );
        assert_eq!(app.screen, Screen::ReadingHistory(Page::Day(day)));
        assert_eq!(app.reading_history.day_seconds(day, None, Zone::Local), 245);
        assert_eq!(app.current_position().unwrap(), position);
        app.save().unwrap();
        let restored = Reader::open(&fixture.root, &fixture.state, fonts.to_vec()).unwrap();
        assert_eq!(restored.reading_history.options(), options);
    }
}
