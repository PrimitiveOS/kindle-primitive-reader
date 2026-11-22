//! Daily statistics, a month calendar and clipped day timelines.
mod calendar;
use super::{Action, Canvas, Change, HEIGHT, Input, Item, Reader, Rect, Result, Screen, WIDTH};
use crate::reading_stats::history::{
    History, Options,
    date::{Date, Zone},
};
use primitive_ui::{BLACK, BODY, CAPTION, HAIRLINE, TextStyle, back_button, header};
use std::time::Instant;

const PREVIOUS: Rect = Rect::new(40, 146, 156, 76);
const NEXT: Rect = Rect::new(WIDTH as i32 - 196, 146, 156, 76);
const SUMMARY: Rect = Rect::new(40, 238, 376, 76);
const DAILY: Rect = Rect::new(444, 238, 376, 76);
const TODAY: Rect = Rect::new(848, 238, 376, 76);
const OPTIONS: Rect = Rect::new(40, HEIGHT as i32 - 120, 376, 80);
const FILTER: Rect = Rect::new(444, HEIGHT as i32 - 120, 780, 80);
const RUN_Y: i32 = 620;
const RUN_ROWS: usize = 8;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Page {
    Summary,
    Daily,
    Calendar,
    Day(Date),
    Books,
    BookDay(Date, u64),
    Options,
}
impl Page {
    pub(super) fn title(self) -> &'static str {
        match self {
            Self::Summary => "Reading statistics",
            Self::Daily => "Daily reading history",
            Self::Calendar => "Reading calendar",
            Self::Day(_) => "Reading timeline",
            Self::Books => "Statistics by book",
            Self::BookDay(_, _) => "Book reading sessions",
            Self::Options => "Statistics settings",
        }
    }
}
#[derive(Clone, Copy)]
pub(super) enum Command {
    Open(Option<u64>),
    Show(Page),
    Filter(Option<u64>),
    Month(i32),
    Day(i32),
    Today,
    RunPage(i32),
    Configure(Options),
}
pub(super) struct View {
    back: Screen,
    pub month: Date,
    pub book: Option<u64>,
    day_back: Page,
    run_page: usize,
}
impl Default for View {
    fn default() -> Self {
        Self {
            back: Screen::Library,
            month: Date::today().first(),
            book: None,
            day_back: Page::Calendar,
            run_page: 0,
        }
    }
}
#[derive(Clone)]
struct Run {
    book: u64,
    start: i64,
    end: i64,
    seconds: u64,
}
fn runs(history: &History, date: Date, book: Option<u64>) -> Vec<Run> {
    let Some((start, end)) = Zone::Local.range(date, history.options().day_start) else {
        return Vec::new();
    };
    let mut visits = history.timeline(date, book, Zone::Local);
    visits.sort_by_key(|v| v.start);
    let mut result: Vec<Run> = Vec::new();
    for visit in visits {
        let a = visit.start.unwrap().max(start);
        let b = (visit.start.unwrap() + i64::from(visit.seconds)).min(end);
        let seconds = (b - a) as u64;
        if let Some(last) = result.last_mut()
            && last.book == visit.book
            && a >= last.start
            && a <= last.end + i64::from(history.options().min_seconds.max(30))
        {
            last.end = last.end.max(b);
            last.seconds = last.seconds.saturating_add(seconds);
        } else {
            result.push(Run {
                book: visit.book,
                start: a,
                end: b,
                seconds,
            });
        }
    }
    result
}
fn short_time(seconds: u64) -> String {
    if seconds < 60 {
        format!("{seconds}s")
    } else if seconds < 3600 {
        if seconds % 60 == 0 {
            format!("{}m", seconds / 60)
        } else {
            format!("{}m {}s", seconds / 60, seconds % 60)
        }
    } else {
        format!("{}h {}m", seconds / 3600, seconds / 60 % 60)
    }
}
fn clock(at: i64) -> String {
    Zone::Local.clock(at).map_or_else(
        || "Unknown time".into(),
        |(h, m, s)| format!("{h:02}:{m:02}:{s:02}"),
    )
}
fn cell(index: usize) -> Rect {
    Rect::new(
        40 + index as i32 % 7 * 169,
        376 + index as i32 / 7 * 166,
        164,
        158,
    )
}
impl Reader {
    fn history_title(&self, id: u64) -> String {
        self.reading_history
            .title(id)
            .map(str::to_owned)
            .or_else(|| self.library.state(id).map(|b| b.title.clone()))
            .unwrap_or_else(|| format!("Unknown book {id:016x}"))
    }
    pub(super) fn activate_reading_history(&mut self, command: Command) -> Result<()> {
        use Command::*;
        match command {
            Open(book) => {
                if !matches!(self.screen, Screen::ReadingHistory(_)) {
                    self.history_view.back = self.screen;
                }
                self.history_view.book = book;
                self.history_view.month = Date::today().first();
                self.show(Screen::ReadingHistory(Page::Summary));
            }
            Show(page) => {
                if matches!(page, Page::Day(_)) {
                    self.history_view.day_back = if let Screen::ReadingHistory(old) = self.screen {
                        if matches!(old, Page::Day(_)) {
                            self.history_view.day_back
                        } else {
                            old
                        }
                    } else {
                        Page::Calendar
                    };
                    self.history_view.run_page = 0;
                }
                self.show(Screen::ReadingHistory(page));
            }
            Filter(book) => {
                self.history_view.book = book;
                self.history_view.month = Date::today().first();
                self.show(Screen::ReadingHistory(Page::Summary));
            }
            Month(delta) => {
                if let Some(month) = self.history_view.month.add_months(delta)
                    && self.history_month_allowed(month)
                {
                    self.history_view.month = month;
                }
            }
            Today => {
                self.history_view.month = Date::today().first();
                self.history_view.run_page = 0;
                self.show(Screen::ReadingHistory(Page::Day(Date::today())));
            }
            Day(delta) => {
                if let Screen::ReadingHistory(Page::Day(day)) = self.screen
                    && let Some(day) = day.add_days(delta)
                    && day <= Date::today()
                {
                    self.history_view.run_page = 0;
                    self.show(Screen::ReadingHistory(Page::Day(day)));
                }
            }
            RunPage(delta) => {
                self.history_view.run_page = self
                    .history_view
                    .run_page
                    .saturating_add_signed(delta as isize);
            }
            Configure(options) => {
                if !options.future_months {
                    self.history_view.month = self.history_view.month.min(Date::today().first());
                }
                self.reading_history
                    .configure(options, Instant::now(), crate::mark_time::now())?;
                self.show(Screen::ReadingHistory(Page::Options));
            }
        }
        Ok(())
    }
    fn history_month_allowed(&self, month: Date) -> bool {
        let first = self
            .reading_history
            .days(self.history_view.book, Zone::Local)
            .keys()
            .next()
            .copied()
            .unwrap_or_else(Date::today)
            .first()
            .min(Date::today().first());
        month >= first
            && (self.reading_history.options().future_months || month <= Date::today().first())
    }
    pub(super) fn history_back(&mut self) {
        let Screen::ReadingHistory(page) = self.screen else {
            return;
        };
        let back = match page {
            Page::Summary => self.history_view.back,
            Page::Day(_) => Screen::ReadingHistory(self.history_view.day_back),
            Page::BookDay(day, _) => Screen::ReadingHistory(Page::Day(day)),
            _ => Screen::ReadingHistory(Page::Summary),
        };
        self.show(back);
    }
    pub(super) fn reading_history_items(&self, page: Page) -> Vec<Item> {
        let history = &self.reading_history;
        let book = self.history_view.book;
        let options = history.options();
        let show =
            |title: &str, page| Item::new(title, Action::ReadingHistory(Command::Show(page)));
        let mut rows = Vec::new();
        match page {
            Page::Summary => {
                let summary = history.summary(book, None);
                rows.push(
                    Item::new(
                        book.map_or_else(|| "All books".into(), |id| self.history_title(id)),
                        Action::ReadingHistory(Command::Show(Page::Books)),
                    )
                    .detail("Choose books"),
                );
                rows.push(
                    Item::new("Recorded reading time", Action::Heading)
                        .detail(short_time(summary.seconds)),
                );
                rows.push(
                    Item::new("Distinct reading screens", Action::Heading).detail(format!(
                        "{} · {} {}",
                        summary.screens,
                        summary.books,
                        if summary.books == 1 { "book" } else { "books" }
                    )),
                );
                rows.push(
                    Item::new("Page time, capped per screen", Action::Heading)
                        .detail(short_time(summary.capped_seconds)),
                );
                let undated = history
                    .visits()
                    .iter()
                    .filter(|v| book.is_none_or(|b| v.book == b) && v.start.is_none())
                    .map(|v| u64::from(v.seconds))
                    .sum();
                rows.push(
                    Item::new("Earlier screen time · undated", Action::Heading)
                        .detail(short_time(history.prior(book))),
                );
                if undated > 0 {
                    rows.push(
                        Item::new("Uncertain clock · undated", Action::Heading)
                            .detail(short_time(undated)),
                    );
                }
                rows.extend([
                    show("Daily history", Page::Daily),
                    show("Calendar", Page::Calendar),
                    Item::new("Today's timeline", Action::ReadingHistory(Command::Today)),
                    show("Statistics settings", Page::Options),
                ]);
                if let Some(warning) = history.warning() {
                    rows.push(Item::new(warning, Action::Heading));
                }
            }
            Page::Daily => {
                rows.extend(
                    history
                        .days(book, Zone::Local)
                        .into_iter()
                        .rev()
                        .map(|(day, s)| {
                            show(&day.label(), Page::Day(day)).detail(format!(
                                "{} · {} {} · {} {}",
                                short_time(s.seconds),
                                s.screens,
                                if s.screens == 1 { "screen" } else { "screens" },
                                s.books,
                                if s.books == 1 { "book" } else { "books" }
                            ))
                        }),
                );
                if rows.is_empty() {
                    rows.push(
                        Item::new("No dated reading sessions yet", Action::Heading)
                            .detail("Earlier aggregate time has no recorded dates."),
                    );
                }
            }
            Page::Books => {
                rows.push(Item::new(
                    "All books",
                    Action::ReadingHistory(Command::Filter(None)),
                ));
                let mut books: Vec<_> = history
                    .books()
                    .into_iter()
                    .map(|id| (self.history_title(id), id))
                    .collect();
                books.sort();
                rows.extend(books.into_iter().map(|(title, id)| {
                    Item::new(title, Action::ReadingHistory(Command::Filter(Some(id)))).detail(
                        format!(
                            "{} recorded · {} earlier undated",
                            short_time(history.summary(Some(id), None).seconds),
                            short_time(history.prior(Some(id)))
                        ),
                    )
                }));
            }
            Page::BookDay(day, id) => {
                rows.push(Item::new(self.history_title(id), Action::Heading).detail(day.label()));
                rows.extend(runs(history, day, Some(id)).into_iter().map(|r| {
                    Item::new(
                        format!("{}–{}", clock(r.start), clock(r.end)),
                        Action::Heading,
                    )
                    .detail(short_time(r.seconds))
                }));
                if self.library.state(id).is_some() {
                    rows.push(
                        Item::new("Continue reading", Action::OpenTracked(id))
                            .detail("Resume this book's saved position"),
                    );
                }
            }
            Page::Options => {
                let opt = |title: &str, value: String, next: Options| {
                    Item::new(title, Action::ReadingHistory(Command::Configure(next))).detail(value)
                };
                rows.push(opt(
                    "Reading statistics",
                    if options.enabled { "On" } else { "Off" }.into(),
                    Options {
                        enabled: !options.enabled,
                        ..options
                    },
                ));
                let min = match options.min_seconds {
                    1 => 5,
                    5 => 10,
                    10 => 15,
                    _ => 1,
                };
                rows.push(opt(
                    "Minimum time per visit",
                    format!("{} seconds", options.min_seconds),
                    Options {
                        min_seconds: min.min(options.max_seconds),
                        ..options
                    },
                ));
                let max = match options.max_seconds {
                    60 => 120,
                    120 => 300,
                    300 => 600,
                    _ => 60,
                };
                rows.push(opt(
                    "Maximum time per visit",
                    format!("{} seconds", options.max_seconds),
                    Options {
                        max_seconds: max,
                        min_seconds: options.min_seconds.min(max),
                        ..options
                    },
                ));
                rows.push(opt(
                    "Freeze finished books",
                    if options.freeze_finished { "On" } else { "Off" }.into(),
                    Options {
                        freeze_finished: !options.freeze_finished,
                        ..options
                    },
                ));
                let next = match options.week_start {
                    1 => 0,
                    0 => 6,
                    6 => 2,
                    _ => 1,
                };
                rows.push(opt(
                    "Calendar week starts",
                    [
                        "Sunday",
                        "Monday",
                        "Tuesday",
                        "Wednesday",
                        "Thursday",
                        "Friday",
                        "Saturday",
                    ][options.week_start as usize]
                        .into(),
                    Options {
                        week_start: next,
                        ..options
                    },
                ));
                rows.push(opt(
                    "Calendar day starts",
                    format!(
                        "{:02}:{:02}",
                        options.day_start / 60,
                        options.day_start % 60
                    ),
                    Options {
                        day_start: match options.day_start {
                            0 => 240,
                            240 => 360,
                            _ => 0,
                        },
                        ..options
                    },
                ));
                rows.push(opt(
                    "Hourly histogram",
                    if options.histogram { "On" } else { "Off" }.into(),
                    Options {
                        histogram: !options.histogram,
                        ..options
                    },
                ));
                rows.push(opt(
                    "Browse future months",
                    if options.future_months { "On" } else { "Off" }.into(),
                    Options {
                        future_months: !options.future_months,
                        ..options
                    },
                ));
                rows.push(
                    Item::new("Visit filters apply to new reading", Action::Heading)
                        .detail("Quick skims are ignored. Rereads retain their own sessions."),
                );
            }
            Page::Calendar | Page::Day(_) => {}
        }
        rows
    }
    pub(super) fn reading_history_input(&mut self, input: Input) -> Option<Change> {
        let Screen::ReadingHistory(page) = self.screen else {
            return None;
        };
        if matches!(input,Input::Tap(x,y) if back_button(WIDTH as i32).contains(x,y)) {
            self.history_back();
            return Some(Change::Partial);
        }
        let day_step = |delta| {
            if let Page::Day(day) = page {
                let count = runs(&self.reading_history, day, self.history_view.book).len();
                if (delta < 0 && self.history_view.run_page > 0)
                    || (delta > 0 && (self.history_view.run_page + 1) * RUN_ROWS < count)
                {
                    return Command::RunPage(delta);
                }
            }
            Command::Day(delta)
        };
        let command = match input {
            Input::PageBack if page == Page::Calendar => Some(Command::Month(-1)),
            Input::PageForward if page == Page::Calendar => Some(Command::Month(1)),
            Input::PageBack if matches!(page, Page::Day(_)) => Some(day_step(-1)),
            Input::PageForward if matches!(page, Page::Day(_)) => Some(day_step(1)),
            Input::Swipe { dx, dy, .. }
                if matches!(page, Page::Calendar | Page::Day(_))
                    && dx.abs() > 60.
                    && dx.abs() > dy.abs() =>
            {
                Some(if page == Page::Calendar {
                    Command::Month(if dx > 0. { -1 } else { 1 })
                } else {
                    Command::Day(if dx > 0. { -1 } else { 1 })
                })
            }
            Input::Tap(x, y) if matches!(page, Page::Calendar | Page::Day(_)) => {
                if PREVIOUS.contains(x, y) {
                    Some(if page == Page::Calendar {
                        Command::Month(-1)
                    } else {
                        Command::Day(-1)
                    })
                } else if NEXT.contains(x, y) {
                    Some(if page == Page::Calendar {
                        Command::Month(1)
                    } else {
                        Command::Day(1)
                    })
                } else if SUMMARY.contains(x, y) {
                    Some(Command::Show(Page::Summary))
                } else if DAILY.contains(x, y) {
                    Some(Command::Show(Page::Daily))
                } else if TODAY.contains(x, y) {
                    Some(Command::Today)
                } else if OPTIONS.contains(x, y) {
                    Some(Command::Show(Page::Options))
                } else if FILTER.contains(x, y) {
                    Some(Command::Show(Page::Books))
                } else if let Page::Day(day) = page {
                    let rows = runs(&self.reading_history, day, self.history_view.book);
                    let first = self.history_view.run_page * RUN_ROWS;
                    if Rect::new(40, 1450, 330, 70).contains(x, y) && self.history_view.run_page > 0
                    {
                        Some(Command::RunPage(-1))
                    } else if Rect::new(894, 1450, 330, 70).contains(x, y)
                        && first + RUN_ROWS < rows.len()
                    {
                        Some(Command::RunPage(1))
                    } else {
                        rows.iter()
                            .skip(first)
                            .take(RUN_ROWS)
                            .enumerate()
                            .find(|(i, _)| {
                                Rect::new(40, RUN_Y + *i as i32 * 100, 1184, 96).contains(x, y)
                            })
                            .map(|(_, r)| Command::Show(Page::BookDay(day, r.book)))
                    }
                } else {
                    let month = self.history_view.month;
                    let offset =
                        (month.weekday() + 7 - self.reading_history.options().week_start) % 7;
                    (1..=month.month_days())
                        .find(|day| cell(usize::from(offset + *day - 1)).contains(x, y))
                        .map(|day| Date { day, ..month })
                        .filter(|d| *d <= Date::today())
                        .map(|day| Command::Show(Page::Day(day)))
                }
            }
            _ => None,
        };
        command.map(|command| match self.activate_reading_history(command) {
            Ok(()) => Change::Partial,
            Err(error) => self.report(error),
        })
    }
}
#[cfg(test)]
mod tests;
