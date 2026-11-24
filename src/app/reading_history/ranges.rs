use super::{Action, Command, Date, Item, Reader, Screen, Zone, report::counts, short_time};
use crate::reading_stats::history::report::{Period, Range, Totals};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum BookGroup {
    All,
    Day {
        range: Range,
        day: Date,
        period: Option<Date>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Page {
    Periods(Range),
    Days(Range, Date),
    Books(BookGroup),
    Book(BookGroup, u64),
}

impl Page {
    pub(super) fn title(self) -> &'static str {
        match self {
            Self::Periods(range) => range.title(),
            Self::Days(_, _) => "Reading by day",
            Self::Books(BookGroup::All) => "All books",
            Self::Books(BookGroup::Day { .. }) => "Books read that day",
            Self::Book(_, _) => "Book statistics",
        }
    }
}

#[derive(Default)]
pub(super) struct View {
    cutoff: i64,
    periods: Vec<(Date, Totals)>,
    days: Vec<(Date, Totals)>,
    books: Vec<(u64, Totals)>,
    periods_page: usize,
    days_page: usize,
    books_page: usize,
}

impl View {
    pub(super) fn cutoff(&self) -> i64 {
        self.cutoff
    }

    pub(super) fn back(&self, page: Page) -> (Screen, Option<usize>) {
        let child = |page| Screen::ReadingHistory(super::Page::Range(page));
        match page {
            Page::Periods(_) | Page::Books(BookGroup::All) => {
                (Screen::ReadingHistory(super::Page::TimeRange), None)
            }
            Page::Days(range, _) => (child(Page::Periods(range)), Some(self.periods_page)),
            Page::Books(BookGroup::Day {
                range,
                period: Some(period),
                ..
            }) => (child(Page::Days(range, period)), Some(self.days_page)),
            Page::Books(BookGroup::Day {
                range,
                period: None,
                ..
            }) => (child(Page::Periods(range)), Some(self.periods_page)),
            Page::Book(group, _) => (child(Page::Books(group)), Some(self.books_page)),
        }
    }
}

impl Reader {
    pub(super) fn prepare_range_report(&mut self, page: Page, previous: usize) {
        let view = &mut self.history_view.ranges;
        let book = self.history_view.book;
        let history = &self.reading_history;
        match page {
            Page::Periods(range) => {
                view.cutoff = range.cutoff(crate::mark_time::now(), Zone::Local);
                view.periods =
                    history.periods_since(range.period(), book, Zone::Local, view.cutoff);
            }
            Page::Days(range, date) => {
                view.periods_page = previous;
                view.days =
                    history.days_in_period(range.period(), date, book, Zone::Local, view.cutoff);
            }
            Page::Books(BookGroup::All) => view.books = history.all_books_report(),
            Page::Books(BookGroup::Day { day, period, .. }) => {
                if period.is_some() {
                    view.days_page = previous
                } else {
                    view.periods_page = previous
                }
                view.books = history.books_on_report_day(day, book, Zone::Local, view.cutoff);
            }
            Page::Book(_, _) => view.books_page = previous,
        }
    }

    pub(super) fn refresh_range_books(&mut self, range: Range, day: Date, period: Option<Date>) {
        let view = &mut self.history_view.ranges;
        let book = self.history_view.book;
        let history = &self.reading_history;
        view.periods = history.periods_since(range.period(), book, Zone::Local, view.cutoff);
        if let Some(period) = period {
            view.days =
                history.days_in_period(range.period(), period, book, Zone::Local, view.cutoff);
        }
        view.books = history.books_on_report_day(day, book, Zone::Local, view.cutoff);
    }

    pub(super) fn range_report_window(
        &self,
        page: Page,
        first: usize,
        limit: usize,
    ) -> (usize, Vec<Item>) {
        let view = &self.history_view.ranges;
        let show = |title: String, page| {
            Item::new(
                title,
                Action::ReadingHistory(Command::Show(super::Page::Range(page))),
            )
        };
        let window = |items: Vec<Item>| {
            (
                items.len(),
                items.into_iter().skip(first).take(limit).collect(),
            )
        };
        let no_rows = || {
            window(vec![Item::new(
                "No reading sessions in this report",
                Action::Heading,
            )])
        };
        match page {
            Page::Periods(_) if view.periods.is_empty() => no_rows(),
            Page::Periods(range) => {
                let rows = view
                    .periods
                    .iter()
                    .skip(first)
                    .take(limit)
                    .map(|&(date, totals)| {
                        let target = if range.period() == Period::Day {
                            Page::Books(BookGroup::Day {
                                range,
                                day: date,
                                period: None,
                            })
                        } else {
                            Page::Days(range, date)
                        };
                        show(range.date_label(date), target).detail(counts(totals))
                    });
                (view.periods.len(), rows.collect())
            }
            Page::Days(range, period) => {
                let heading = Item::new(range.date_label(period), Action::Heading);
                let rows = view.days.iter().map(|&(day, totals)| {
                    show(
                        day.label(),
                        Page::Books(BookGroup::Day {
                            range,
                            day,
                            period: Some(period),
                        }),
                    )
                    .detail(counts(totals))
                });
                (
                    view.days.len() + 1,
                    std::iter::once(heading)
                        .chain(rows)
                        .skip(first)
                        .take(limit)
                        .collect(),
                )
            }
            Page::Books(_) if view.books.is_empty() => no_rows(),
            Page::Books(group) => {
                let heading = match group {
                    BookGroup::All => Item::new("Total time spent reading", Action::Heading)
                        .detail(short_time(
                            view.books.iter().map(|(_, totals)| totals.seconds).sum(),
                        )),
                    BookGroup::Day { day, .. } => Item::new(day.label(), Action::Heading)
                        .detail("Hold a book to reset its reading time for this day"),
                };
                let rows = view.books.iter().map(|&(book, totals)| {
                    show(self.history_title(book), Page::Book(group, book)).detail(counts(totals))
                });
                (
                    view.books.len() + 1,
                    std::iter::once(heading)
                        .chain(rows)
                        .skip(first)
                        .take(limit)
                        .collect(),
                )
            }
            Page::Book(group, book) => {
                let totals = view
                    .books
                    .iter()
                    .find(|(id, _)| *id == book)
                    .map(|&(_, totals)| totals)
                    .unwrap_or_default();
                let scope = match group {
                    BookGroup::All => "All recorded sessions".into(),
                    BookGroup::Day { day, .. } => day.label(),
                };
                let mut rows = vec![
                    Item::new(self.history_title(book), Action::Heading).detail(scope),
                    Item::new("Recorded reading time", Action::Heading)
                        .detail(short_time(totals.seconds)),
                    Item::new("Distinct reading screens", Action::Heading)
                        .detail(totals.screens.to_string()),
                ];
                if self.library.state(book).is_some() {
                    rows.push(Item::new("Continue reading", Action::OpenTracked(book)));
                }
                window(rows)
            }
        }
    }
}
