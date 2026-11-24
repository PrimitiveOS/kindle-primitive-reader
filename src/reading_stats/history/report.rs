//! Reading totals grouped by the civil date on which a visit began.
use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};

use super::{Date, History, Visit, Zone};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Period {
    Week,
    Month,
}

impl Period {
    fn start(self, date: Date) -> Option<Date> {
        match self {
            Self::Week => date.add_days(-i32::from((date.weekday() + 6) % 7)),
            Self::Month => Some(date.first()),
        }
    }

    pub(crate) fn label(self, start: Date) -> String {
        match self {
            Self::Week => format!("Week of {}", start.label()),
            Self::Month => start.month_label(),
        }
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
        let mut groups = BTreeMap::<Date, Group>::new();
        for (start, visit) in self.dated_by_period(period, book, zone) {
            groups.entry(start).or_default().add(visit);
        }
        groups
            .into_iter()
            .rev()
            .map(|(date, group)| (date, group.totals()))
            .collect()
    }

    pub(crate) fn books_in_period(
        &self,
        period: Period,
        start: Date,
        book: Option<u64>,
        zone: Zone,
    ) -> Vec<(u64, Totals)> {
        let mut groups = BTreeMap::<u64, Group>::new();
        for (date, visit) in self.dated_by_period(period, book, zone) {
            if date == start {
                groups.entry(visit.book).or_default().add(visit);
            }
        }
        let mut groups: Vec<_> = groups.into_iter().collect();
        groups.sort_by_key(|(id, group)| (Reverse(group.last_read), *id));
        groups
            .into_iter()
            .map(|(id, group)| (id, group.totals()))
            .collect()
    }

    fn dated_by_period(
        &self,
        period: Period,
        book: Option<u64>,
        zone: Zone,
    ) -> impl Iterator<Item = (Date, &Visit)> {
        let calendar = zone.calendar();
        self.visits().iter().filter_map(move |visit| {
            if book.is_some_and(|book| visit.book != book) {
                return None;
            }
            let date = period.start(calendar.date(visit.start?)?)?;
            Some((date, visit))
        })
    }
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
                    monday,
                    Totals {
                        seconds: 105,
                        screens: 2,
                        books: 2,
                    },
                ),
                (
                    Date::new(2025, 12, 22).unwrap(),
                    Totals {
                        seconds: 15,
                        screens: 1,
                        books: 1,
                    },
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
            vec![
                (
                    2,
                    Totals {
                        seconds: 45,
                        screens: 1,
                        books: 1
                    }
                ),
                (
                    1,
                    Totals {
                        seconds: 60,
                        screens: 1,
                        books: 1
                    }
                ),
            ]
        );
        assert_eq!(history.prior(Some(9)), 300);
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
