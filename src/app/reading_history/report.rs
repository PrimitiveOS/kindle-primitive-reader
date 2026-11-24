use super::*;
use crate::reading_stats::history::report::Totals;

#[derive(Default)]
pub(super) struct ReportView {
    periods: Vec<(Date, Totals)>,
    books: Vec<(u64, Totals)>,
    pub(super) period_list_page: usize,
    pub(super) book_list_page: usize,
}

pub(super) fn counts(totals: Totals) -> String {
    let screens = if totals.screens == 1 {
        "screen"
    } else {
        "screens"
    };
    format!(
        "{} · {} {screens}",
        short_time(totals.seconds),
        totals.screens
    )
}

impl Reader {
    pub(super) fn show_history_page(&mut self, page: Page) {
        let previous = self.list_page;
        self.show(Screen::ReadingHistory(page));
        if let Page::Range(range) = page {
            self.prepare_range_report(range, previous);
            return;
        }
        let book = self.history_view.book;
        let view = &mut self.history_view.report;
        match page {
            Page::Periods(period) => {
                view.periods = self.reading_history.by_period(period, book, Zone::Local);
            }
            Page::PeriodBooks(period, start) => {
                view.period_list_page = previous;
                view.books = self
                    .reading_history
                    .books_in_period(period, start, book, Zone::Local);
            }
            Page::PeriodBook(_, _, _) => view.book_list_page = previous,
            _ => {}
        }
    }

    pub(super) fn refresh_period_report(&mut self, period: Period, start: Date) {
        let book = self.history_view.book;
        let view = &mut self.history_view.report;
        view.periods = self.reading_history.by_period(period, book, Zone::Local);
        view.books = self
            .reading_history
            .books_in_period(period, start, book, Zone::Local);
    }

    pub(in crate::app) fn history_report_window(
        &self,
        page: Page,
        first: usize,
        limit: usize,
    ) -> Option<(usize, Vec<Item>)> {
        let show = |title, page| Item::new(title, Action::ReadingHistory(Command::Show(page)));
        let view = &self.history_view.report;
        let items = match page {
            Page::Range(page) => return Some(self.range_report_window(page, first, limit)),
            Page::TimeRange => {
                let mut items = Vec::new();
                if let Some(book) = self.history_view.book {
                    items.push(
                        Item::new(self.history_title(book), Action::Heading).detail("Book filter"),
                    );
                }
                items.extend([
                    show(
                        "All books".into(),
                        Page::Range(ranges::Page::Books(ranges::BookGroup::All)),
                    ),
                    show("Books by week".into(), Page::Periods(Period::Week)),
                    show("Books by month".into(), Page::Periods(Period::Month)),
                ]);
                items.extend(
                    crate::reading_stats::history::report::Range::ALL.map(|range| {
                        show(
                            range.title().into(),
                            Page::Range(ranges::Page::Periods(range)),
                        )
                    }),
                );
                items
            }
            Page::Periods(_) if view.periods.is_empty() => {
                vec![Item::new("No dated reading sessions yet", Action::Heading)]
            }
            Page::Periods(period) => {
                let rows =
                    view.periods
                        .iter()
                        .skip(first)
                        .take(limit)
                        .map(|&(start, totals)| {
                            let book = if totals.books == 1 { "book" } else { "books" };
                            show(period.label(start), Page::PeriodBooks(period, start))
                                .detail(format!("{} · {} {book}", counts(totals), totals.books))
                        });
                return Some((view.periods.len(), rows.collect()));
            }
            Page::PeriodBooks(period, start) => {
                let heading = Item::new(period.label(start), Action::Heading)
                    .detail("Hold a book to reset its time in this period");
                let books = view.books.iter().map(|&(id, totals)| {
                    show(self.history_title(id), Page::PeriodBook(period, start, id))
                        .detail(counts(totals))
                });
                let rows = std::iter::once(heading)
                    .chain(books)
                    .skip(first)
                    .take(limit);
                return Some((view.books.len() + 1, rows.collect()));
            }
            Page::PeriodBook(period, start, id) => {
                let totals = view
                    .books
                    .iter()
                    .find_map(|&(book, totals)| (book == id).then_some(totals))
                    .unwrap_or_default();
                let mut items = vec![
                    Item::new(self.history_title(id), Action::Heading).detail(period.label(start)),
                    Item::new("Recorded reading time", Action::Heading)
                        .detail(short_time(totals.seconds)),
                    Item::new("Distinct reading screens", Action::Heading)
                        .detail(totals.screens.to_string()),
                ];
                if self.library.state(id).is_some() {
                    items.push(Item::new("Continue reading", Action::OpenTracked(id)));
                }
                items
            }
            _ => return None,
        };
        Some((
            items.len(),
            items.into_iter().skip(first).take(limit).collect(),
        ))
    }
}
