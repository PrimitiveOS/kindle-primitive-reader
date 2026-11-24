//! Reading totals grouped by the civil date on which a visit began.
use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};

use super::{Date, History, Visit, Zone};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Period {
    Day,
    Week,
    Month,
}

impl Period {
    fn start(self, date: Date) -> Option<Date> {
        match self {
            Self::Day => Some(date),
            Self::Week => {
                let first = Date::new(date.year, 1, 1)?;
                Some(
                    date.add_days(-i32::from((date.weekday() + 6) % 7))?
                        .max(first),
                )
            }
            Self::Month => Some(date.first()),
        }
    }

    pub(crate) fn label(self, start: Date) -> String {
        match self {
            Self::Day => start.label(),
            Self::Week => format!("{} Week {:02}", start.year, start.monday_week()),
            Self::Month => start.month_label(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Range {
    LastWeek,
    LastMonthByDay,
    LastYearByDay,
    LastYearByWeek,
    AllMonths,
}

impl Range {
    pub(crate) const ALL: [Self; 5] = [
        Self::LastWeek,
        Self::LastMonthByDay,
        Self::LastYearByDay,
        Self::LastYearByWeek,
        Self::AllMonths,
    ];

    pub(crate) fn title(self) -> &'static str {
        match self {
            Self::LastWeek => "Last week",
            Self::LastMonthByDay => "Last month by day",
            Self::LastYearByDay => "Last year by day",
            Self::LastYearByWeek => "Last year by week",
            Self::AllMonths => "All stats by month",
        }
    }

    pub(crate) fn period(self) -> Period {
        match self {
            Self::LastWeek | Self::LastMonthByDay | Self::LastYearByDay => Period::Day,
            Self::LastYearByWeek => Period::Week,
            Self::AllMonths => Period::Month,
        }
    }

    pub(crate) fn date_label(self, date: Date) -> String {
        match self {
            Self::LastWeek | Self::LastMonthByDay => format!(
                "{} ({})",
                date.label(),
                ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"][usize::from(date.weekday())]
            ),
            _ => self.period().label(date),
        }
    }

    pub(crate) fn cutoff(self, now: Option<i64>, zone: Zone) -> i64 {
        let days = match self {
            Self::LastWeek => 7,
            Self::LastMonthByDay => 30,
            Self::LastYearByDay | Self::LastYearByWeek => 365,
            Self::AllMonths => return 0,
        };
        let Some(now) = now else { return i64::MAX };
        let Some((hour, minute, second)) = zone.clock(now) else {
            return i64::MAX;
        };
        now - (days - 1) * 86_400 - i64::from(hour * 3600 + minute * 60 + second)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Totals {
    pub(crate) seconds: u64,
    pub(crate) screens: usize,
    pub(crate) books: usize,
}

#[derive(Default)]
struct Group {
    seconds: u64,
    screens: BTreeSet<(u64, u64)>,
    books: BTreeSet<u64>,
    last_read: Option<i64>,
}

impl Group {
    fn add(&mut self, visit: &Visit) {
        self.seconds += u64::from(visit.seconds);
        self.screens.insert((visit.book, visit.page));
        self.books.insert(visit.book);
        self.last_read = self.last_read.max(visit.start);
    }

    fn totals(self) -> Totals {
        Totals {
            seconds: self.seconds,
            screens: self.screens.len(),
            books: self.books.len(),
        }
    }
}

impl History {
    pub(crate) fn by_period(
        &self,
        period: Period,
        book: Option<u64>,
        zone: Zone,
    ) -> Vec<(Date, Totals)> {
        self.periods_since(period, book, zone, 0)
    }

    pub(crate) fn books_in_period(
        &self,
        period: Period,
        start: Date,
        book: Option<u64>,
        zone: Zone,
    ) -> Vec<(u64, Totals)> {
        group_books(
            self.dated_by_period(period, book, zone, 0)
                .filter_map(|(date, visit)| (date == start).then_some(visit)),
        )
    }

    pub(crate) fn periods_since(
        &self,
        period: Period,
        book: Option<u64>,
        zone: Zone,
        cutoff: i64,
    ) -> Vec<(Date, Totals)> {
        group_dates(self.dated_by_period(period, book, zone, cutoff))
    }

    pub(crate) fn days_in_period(
        &self,
        period: Period,
        start: Date,
        book: Option<u64>,
        zone: Zone,
        cutoff: i64,
    ) -> Vec<(Date, Totals)> {
        group_dates(
            self.dated_by_period(Period::Day, book, zone, cutoff)
                .filter(|&(date, _)| period.start(date) == Some(start)),
        )
    }

    pub(crate) fn books_on_report_day(
        &self,
        day: Date,
        book: Option<u64>,
        zone: Zone,
        cutoff: i64,
    ) -> Vec<(u64, Totals)> {
        group_books(
            self.dated_by_period(Period::Day, book, zone, cutoff)
                .filter_map(|(date, visit)| (date == day).then_some(visit)),
        )
    }

    pub(crate) fn all_books_report(&self) -> Vec<(u64, Totals)> {
        group_books(self.visits().iter())
    }

    fn dated_by_period(
        &self,
        period: Period,
        book: Option<u64>,
        zone: Zone,
        cutoff: i64,
    ) -> impl Iterator<Item = (Date, &Visit)> {
        let calendar = zone.calendar();
        self.visits().iter().filter_map(move |visit| {
            if book.is_some_and(|book| visit.book != book) {
                return None;
            }
            let start = visit.start?;
            if start < cutoff {
                return None;
            }
            let date = period.start(calendar.date(start)?)?;
            Some((date, visit))
        })
    }
}

fn group_dates<'a>(visits: impl Iterator<Item = (Date, &'a Visit)>) -> Vec<(Date, Totals)> {
    let mut groups = BTreeMap::<Date, Group>::new();
    for (date, visit) in visits {
        groups.entry(date).or_default().add(visit);
    }
    groups
        .into_iter()
        .rev()
        .map(|(date, group)| (date, group.totals()))
        .collect()
}

fn group_books<'a>(visits: impl Iterator<Item = &'a Visit>) -> Vec<(u64, Totals)> {
    let mut groups = BTreeMap::<u64, Group>::new();
    for visit in visits {
        groups.entry(visit.book).or_default().add(visit);
    }
    let mut groups: Vec<_> = groups.into_iter().collect();
    groups.sort_by_key(|(id, group)| (Reverse(group.last_read), *id));
    groups
        .into_iter()
        .map(|(id, group)| (id, group.totals()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn reading_time_range_counts_rereads_and_deduplicates_screens_per_period() {
        let mut history = History::load(std::path::PathBuf::new(), BTreeMap::from([(9, 300)]));
        let zone = Zone::Fixed(0);
        let start = |year, month, day| {
            zone.range(Date::new(year, month, day).unwrap(), 0)
                .unwrap()
                .0
        };
        let mut visit = |book, page, time, seconds| {
            let now = Instant::now();
            history.discard_active();
            history.set_page_at(Some((book, page)), now, time);
            history.set_page_at(
                None,
                now + Duration::from_secs(seconds),
                time.map(|time| time + seconds as i64),
            );
        };
        visit(1, 7, Some(start(2025, 12, 28)), 15);
        visit(1, 7, Some(start(2025, 12, 29)), 25);
        visit(1, 7, Some(start(2026, 1, 1)), 35);
        visit(2, 7, Some(start(2026, 1, 1) + 120), 45);
        visit(3, 1, None, 60);
        let monday = Date::new(2025, 12, 29).unwrap();
        let january = Date::new(2026, 1, 1).unwrap();
        assert_eq!(
            history.by_period(Period::Week, None, zone),
            vec![
                (
                    january,
                    Totals {
                        seconds: 80,
                        screens: 2,
                        books: 2
                    }
                ),
                (
                    monday,
                    Totals {
                        seconds: 25,
                        screens: 1,
                        books: 1
                    }
                ),
                (
                    Date::new(2025, 12, 22).unwrap(),
                    Totals {
                        seconds: 15,
                        screens: 1,
                        books: 1
                    }
                ),
            ]
        );
        assert_eq!(
            history.by_period(Period::Month, Some(1), zone),
            vec![
                (
                    january,
                    Totals {
                        seconds: 35,
                        screens: 1,
                        books: 1
                    }
                ),
                (
                    Date::new(2025, 12, 1).unwrap(),
                    Totals {
                        seconds: 40,
                        screens: 1,
                        books: 1
                    }
                ),
            ]
        );
        assert_eq!(
            history.books_in_period(Period::Week, monday, None, zone),
            vec![(
                1,
                Totals {
                    seconds: 25,
                    screens: 1,
                    books: 1
                }
            )]
        );
        assert_eq!(Period::Week.label(january), "2026 Week 00");
        assert_eq!(history.prior(Some(9)), 300);
    }

    #[test]
    fn rolling_reading_reports_use_stock_cutoff_week_buckets_and_book_totals() {
        let mut history = History::load(std::path::PathBuf::new(), BTreeMap::new());
        let zone = Zone::Fixed(0);
        let december = Date::new(2025, 12, 31).unwrap();
        let january = Date::new(2026, 1, 1).unwrap();
        let today = Date::new(2026, 1, 6).unwrap();
        let start = |day| zone.range(day, 0).unwrap().0;
        let now = start(today) + 13 * 3600 + 53;
        let cutoff = Range::LastWeek.cutoff(Some(now), zone);
        assert_eq!(cutoff, start(december));
        assert_eq!(
            Range::LastMonthByDay.cutoff(Some(now), zone),
            start(today) - 29 * 86400
        );
        assert_eq!(
            Range::LastYearByWeek.cutoff(Some(now), zone),
            start(today) - 364 * 86400
        );
        assert_eq!(Range::AllMonths.cutoff(None, zone), 0);
        assert_eq!(Range::LastWeek.date_label(december), "2025-12-31 (Wed)");
        let now = Instant::now();
        for (book, at, seconds) in [
            (3, Some(cutoff - 1), 6),
            (1, Some(cutoff), 30),
            (1, Some(cutoff + 90), 20),
            (2, Some(start(january)), 45),
            (1, Some(start(today)), 10),
            (4, None, 25),
        ] {
            history.discard_active();
            history.set_page_at(Some((book, 7)), now, at);
            history.set_page_at(
                None,
                now + Duration::from_secs(seconds),
                at.map(|v| v + seconds as i64),
            );
        }
        let reports = history.periods_since(Period::Day, None, zone, cutoff);
        assert_eq!(
            reports,
            vec![
                (
                    today,
                    Totals {
                        seconds: 10,
                        screens: 1,
                        books: 1
                    }
                ),
                (
                    january,
                    Totals {
                        seconds: 45,
                        screens: 1,
                        books: 1
                    }
                ),
                (
                    december,
                    Totals {
                        seconds: 50,
                        screens: 1,
                        books: 1
                    }
                ),
            ]
        );
        assert_eq!(
            history.books_on_report_day(december, None, zone, cutoff)[0]
                .1
                .seconds,
            50
        );
        assert_eq!(
            history.periods_since(Period::Week, None, zone, cutoff)[1].0,
            january
        );
        assert_eq!(
            history.days_in_period(Period::Week, january, None, zone, cutoff),
            vec![reports[1]]
        );
        assert_eq!(
            history
                .all_books_report()
                .iter()
                .map(|(_, totals)| totals.seconds)
                .sum::<u64>(),
            136
        );
    }

    #[test]
    fn reading_time_range_uses_local_visit_start_even_across_midnight() {
        let mut history = History::load(std::path::PathBuf::new(), BTreeMap::new());
        let utc = Zone::Fixed(0);
        let local = Zone::Fixed(3600);
        let january = Date::new(2026, 1, 1).unwrap();
        let december = Date::new(2025, 12, 1).unwrap();
        let middle = utc.range(january, 0).unwrap().0 - 3600;
        let instant = Instant::now();
        history.set_page_at(Some((1, 1)), instant, Some(middle - 10));
        history.set_page_at(None, instant + Duration::from_secs(25), Some(middle + 15));
        history.set_page_at(
            Some((2, 1)),
            instant + Duration::from_secs(25),
            Some(middle + 15),
        );
        history.set_page_at(None, instant + Duration::from_secs(30), Some(middle + 20));
        let utc_months = history.by_period(Period::Month, None, utc);
        assert_eq!(utc_months[0].0, december);
        assert_eq!(utc_months[0].1.seconds, 30);
        assert_eq!(
            history.by_period(Period::Month, None, local),
            vec![
                (
                    january,
                    Totals {
                        seconds: 5,
                        screens: 1,
                        books: 1
                    }
                ),
                (
                    december,
                    Totals {
                        seconds: 25,
                        screens: 1,
                        books: 1
                    }
                ),
            ]
        );
    }
}
