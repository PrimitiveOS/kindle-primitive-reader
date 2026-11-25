//! Reading statistics, reports, a month calendar and clipped day timelines.
mod calendar;
mod calendar_options;
mod duration_options;
mod month_navigation;
mod period_reset;
mod progress;
mod ranges;
mod report;
mod reset;
mod sqlite_import;
use super::{Action, Canvas, Change, HEIGHT, Input, Item, Reader, Rect, Result, Screen, WIDTH};
use crate::reading_stats::history::{
    History, Options,
    date::{Date, Zone},
    report::Period,
    reset::Scope,
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
    Progress,
    Daily,
    TimeRange,
    Range(ranges::Page),
    Periods(Period),
    PeriodBooks(Period, Date),
    PeriodBook(Period, Date, u64),
    Calendar,
    Day(Date),
    Books,
    BookDay(Date, u64),
    Options,
    CalendarChoice(calendar_options::Choice),
    Resets,
    ResetBooks,
    ConfirmReset(Scope),
    ConfirmPeriodReset,
    ImportKoReader,
    ChooseKoReaderDatabase,
}
impl Page {
    pub(super) fn title(self) -> &'static str {
        match self {
            Self::Summary => "Reading statistics",
            Self::Progress => "Reading progress",
            Self::Daily => "Daily reading history",
            Self::TimeRange => "Time range",
            Self::Range(page) => page.title(),
            Self::Periods(Period::Day) => "Books by day",
            Self::Periods(Period::Week) => "Books by week",
            Self::Periods(Period::Month) => "Books by month",
            Self::PeriodBooks(_, _) => "Books read",
            Self::PeriodBook(_, _, _) => "Book statistics",
            Self::Calendar => "Reading calendar",
            Self::Day(_) => "Reading timeline",
            Self::Books => "Statistics by book",
            Self::BookDay(_, _) => "Book reading sessions",
            Self::Options => "Statistics settings",
            Self::CalendarChoice(choice) => choice.title(),
            Self::Resets => "Reset reading statistics",
            Self::ResetBooks => "Choose a book",
            Self::ConfirmReset(_) | Self::ConfirmPeriodReset => "Confirm statistics reset",
            Self::ImportKoReader => "Import KOReader statistics",
            Self::ChooseKoReaderDatabase => "Choose a statistics database",
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
    EditTimelineStart,
    EditDurationLimits,
    ConfirmReset,
    ConfirmPeriodReset,
    CancelReset,
    BeginKoReaderImport,
    PreviewKoReaderImport,
    BrowseKoReaderDatabases,
    PickKoReaderEntry(usize),
    ApplyKoReaderImport,
}
pub(super) struct View {
    back: Screen,
    pub month: Date,
    month_entry: Option<month_navigation::Entry>,
    pub book: Option<u64>,
    day_back: Page,
    run_page: usize,
    report: report::ReportView,
    progress: progress::View,
    ranges: ranges::View,
    calendar_settings_page: usize,
    timeline_start_draft: Option<u16>,
    duration_limits_draft: Option<duration_options::Draft>,
    reset_list_page: usize,
    period_reset: Option<period_reset::Pending>,
    koreader_import: Option<sqlite_import::Pending>,
    koreader_databases: Option<sqlite_import::Picker>,
    import_summary_page: usize,
}
impl Default for View {
    fn default() -> Self {
        Self {
            back: Screen::Library,
            month: Date::today().first(),
            month_entry: None,
            book: None,
            day_back: Page::Calendar,
            run_page: 0,
            report: Default::default(),
            progress: Default::default(),
            ranges: Default::default(),
            calendar_settings_page: 0,
            timeline_start_draft: None,
            duration_limits_draft: None,
            reset_list_page: 0,
            period_reset: None,
            koreader_import: None,
            koreader_databases: None,
            import_summary_page: 0,
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
                self.history_view.month_entry = None;
                self.history_view.report = Default::default();
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
                if matches!(page, Page::ConfirmReset(_)) {
                    self.history_view.reset_list_page = self.list_page;
                }
                if matches!(page, Page::CalendarChoice(_)) {
                    self.history_view.calendar_settings_page = self.list_page;
                }
                self.show_history_page(page);
            }
            Filter(book) => {
                self.history_view.book = book;
                self.history_view.month = Date::today().first();
                self.history_view.month_entry = None;
                self.history_view.report = Default::default();
                self.show(Screen::ReadingHistory(Page::Summary));
            }
            Month(delta) => {
                if let Some(month) = self.history_month_step(delta) {
                    self.history_view.month = month;
                }
            }
            Today => {
                let day = self.history_timeline_today();
                self.history_view.month = day.first();
                self.activate_reading_history(Show(Page::Day(day)))?;
            }
            Day(delta) => {
                if let Screen::ReadingHistory(Page::Day(day)) = self.screen
                    && let Some(day) = day.add_days(delta)
                    && day <= self.history_timeline_today()
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
                self.reading_history
                    .configure(options, Instant::now(), crate::mark_time::now())?;
                self.pace
                    .set_duration_limits(options.min_seconds, options.max_seconds);
                if !options.future_months {
                    self.history_view.month = self.history_view.month.min(Date::today().first());
                }
                let return_page = match self.screen {
                    Screen::ReadingHistory(Page::CalendarChoice(_)) => {
                        self.history_view.calendar_settings_page
                    }
                    Screen::ReadingHistory(Page::Options) => self.list_page,
                    _ => 0,
                };
                self.show(Screen::ReadingHistory(Page::Options));
                self.list_page = return_page;
            }
            EditTimelineStart => {
                self.notice = None;
                self.history_view.timeline_start_draft =
                    Some(self.reading_history.options().day_start);
            }
            EditDurationLimits => self.open_duration_limits(),
            ConfirmReset => self.confirm_history_reset()?,
            ConfirmPeriodReset => self.confirm_period_reset()?,
            CancelReset => self.history_back(),
            BeginKoReaderImport => self.begin_koreader_statistics_import()?,
            PreviewKoReaderImport => self.preview_koreader_statistics_import()?,
            BrowseKoReaderDatabases => self.browse_koreader_databases()?,
            PickKoReaderEntry(index) => self.pick_koreader_entry(index)?,
            ApplyKoReaderImport => self.apply_koreader_statistics_import()?,
        }
        Ok(())
    }
    fn history_first_month(&self) -> Date {
        self.reading_history
            .first_calendar_day(self.history_view.book, Zone::Local)
            .unwrap_or_else(Date::today)
            .first()
            .min(Date::today().first())
    }
    fn history_month_step(&self, delta: i32) -> Option<Date> {
        let month = self.history_view.month.add_months(delta)?;
        let allowed = match delta.cmp(&0) {
            std::cmp::Ordering::Less => month >= self.history_first_month(),
            std::cmp::Ordering::Greater => {
                self.reading_history.options().future_months || month <= Date::today().first()
            }
            std::cmp::Ordering::Equal => false,
        };
        allowed.then_some(month)
    }
    fn history_timeline_today(&self) -> Date {
        crate::mark_time::now()
            .and_then(|time| {
                Zone::Local.date_with_origin(time, self.reading_history.options().day_start)
            })
            .unwrap_or_else(Date::today)
    }
    pub(super) fn history_back(&mut self) {
        let Screen::ReadingHistory(page) = self.screen else {
            return;
        };
        if page == Page::ConfirmPeriodReset {
            self.close_period_reset();
            return;
        }
        if page == Page::ChooseKoReaderDatabase {
            self.back_from_koreader_databases();
            return;
        }
        let (back, list_page) = match page {
            Page::Summary => (self.history_view.back, None),
            Page::Progress => (
                Screen::ReadingHistory(Page::Summary),
                Some(self.history_view.progress.summary_page),
            ),
            Page::ImportKoReader => {
                self.history_view.koreader_import = None;
                self.history_view.koreader_databases = None;
                (
                    Screen::ReadingHistory(Page::Summary),
                    Some(self.history_view.import_summary_page),
                )
            }
            Page::Day(day) => {
                if self.history_view.day_back == Page::Calendar {
                    self.history_view.month = day.first();
                }
                (Screen::ReadingHistory(self.history_view.day_back), None)
            }
            Page::BookDay(day, _) => (Screen::ReadingHistory(Page::Day(day)), None),
            Page::Range(page) => self.history_view.ranges.back(page),
            Page::Periods(_) => (Screen::ReadingHistory(Page::TimeRange), None),
            Page::Resets => (Screen::ReadingHistory(Page::Options), None),
            Page::CalendarChoice(_) => (
                Screen::ReadingHistory(Page::Options),
                Some(self.history_view.calendar_settings_page),
            ),
            Page::ResetBooks => (Screen::ReadingHistory(Page::Resets), None),
            Page::ConfirmReset(scope) => (
                Screen::ReadingHistory(self.history_reset_return(scope)),
                Some(self.history_view.reset_list_page),
            ),
            Page::PeriodBooks(period, _) => (
                Screen::ReadingHistory(Page::Periods(period)),
                Some(self.history_view.report.period_list_page),
            ),
            Page::PeriodBook(period, start, _) => (
                Screen::ReadingHistory(Page::PeriodBooks(period, start)),
                Some(self.history_view.report.book_list_page),
            ),
            _ => (Screen::ReadingHistory(Page::Summary), None),
        };
        self.show(back);
        if let Some(page) = list_page {
            self.list_page = page;
        }
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
                    show("Time range", Page::TimeRange),
                    show("Reading progress", Page::Progress),
                    show("Calendar", Page::Calendar),
                    Item::new("Today's timeline", Action::ReadingHistory(Command::Today)),
                    show("Statistics settings", Page::Options),
                ]);
                if self.koreader_statistics_book().is_some() {
                    rows.push(Item::new(
                        "Import KOReader statistics",
                        Action::ReadingHistory(Command::BeginKoReaderImport),
                    ));
                }
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
            Page::ConfirmPeriodReset => return self.period_reset_items(),
            Page::Resets | Page::ResetBooks | Page::ConfirmReset(_) => {
                return self.history_reset_items(page);
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
                rows.push(
                    Item::new(
                        "Read page duration limits",
                        Action::ReadingHistory(Command::EditDurationLimits),
                    )
                    .detail(format!(
                        "{} s - {} s",
                        options.min_seconds, options.max_seconds
                    )),
                );
                rows.push(opt(
                    "Freeze finished books",
                    if options.freeze_finished { "On" } else { "Off" }.into(),
                    Options {
                        freeze_finished: !options.freeze_finished,
                        ..options
                    },
                ));
                rows.extend(self.calendar_setting_items());
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
                rows.push(show("Reset statistics", Page::Resets));
                rows.push(
                    Item::new("Visit filters apply to new reading", Action::Heading)
                        .detail("Quick skims are ignored. Rereads retain their own sessions."),
                );
            }
            Page::CalendarChoice(choice) => return self.calendar_choice_items(choice),
            Page::ImportKoReader => return self.koreader_statistics_import_items(),
            Page::ChooseKoReaderDatabase => return self.koreader_database_items(),
            Page::TimeRange
            | Page::Range(_)
            | Page::Periods(_)
            | Page::PeriodBooks(_, _)
            | Page::PeriodBook(_, _, _) => {
                return self.history_report_window(page, 0, usize::MAX).unwrap().1;
            }
            Page::Calendar | Page::Day(_) | Page::Progress => {}
        }
        rows
    }
    pub(super) fn reading_history_input(&mut self, input: Input) -> Option<Change> {
        let Screen::ReadingHistory(page) = self.screen else {
            return None;
        };
        if page == Page::Progress {
            return self.reading_progress_input(input);
        }
        if let Some(change) = self.timeline_start_input(input) {
            return Some(change);
        }
        if let Some(change) = self.duration_limits_input(input) {
            return Some(change);
        }
        if page == Page::Calendar
            && let Some(change) = self.history_month_navigation_input(input)
        {
            return Some(change);
        }
        let back = if matches!(page, Page::ImportKoReader | Page::ChooseKoReaderDatabase) {
            self.navigation_layout().back()
        } else {
            back_button(WIDTH as i32)
        };
        if matches!(input, Input::Tap(x, y) if back.contains(x, y))
            || (page == Page::ChooseKoReaderDatabase
                && self.list_page == 0
                && input == Input::PageBack)
        {
            self.history_back();
            return Some(Change::Partial);
        }
        if let Some(change) = self.period_reset_input(page, input) {
            return Some(change);
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
                if page != Page::Calendar && PREVIOUS.contains(x, y) {
                    Some(Command::Day(-1))
                } else if page != Page::Calendar && NEXT.contains(x, y) {
                    Some(Command::Day(1))
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
