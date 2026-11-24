use super::{
    Action, Change, Command, Input, Instant, Item, Page, Period, Reader, Result, Screen, Zone,
    ranges,
};

pub(super) struct Pending {
    origin: Page,
    page_index: usize,
    book: u64,
    from: i64,
    until: i64,
    label: String,
}

impl Reader {
    pub(super) fn period_reset_input(&mut self, page: Page, input: Input) -> Option<Change> {
        let Input::LongTap(x, y) = input else {
            return None;
        };
        if !matches!(
            page,
            Page::PeriodBooks(..) | Page::Range(ranges::Page::Books(ranges::BookGroup::Day { .. }))
        ) {
            return None;
        }
        let geometry = self.list_geometry();
        let (_, items) = self.list_window();
        let item = items
            .into_iter()
            .enumerate()
            .find_map(|(index, item)| geometry.row(index).contains(x, y).then_some(item))?;
        let (period, start, cutoff, book) = match item.action {
            Action::ReadingHistory(Command::Show(Page::PeriodBook(period, start, book))) => {
                (period, start, 0, book)
            }
            Action::ReadingHistory(Command::Show(Page::Range(ranges::Page::Book(
                ranges::BookGroup::Day { day, .. },
                book,
            )))) => (Period::Day, day, self.history_view.ranges.cutoff(), book),
            _ => return None,
        };
        let (from, until) = period.bounds(start, Zone::Local, cutoff)?;
        self.history_view.period_reset = Some(Pending {
            origin: page,
            page_index: self.list_page,
            book,
            from,
            until,
            label: period.label(start),
        });
        self.show(Screen::ReadingHistory(Page::ConfirmPeriodReset));
        Some(Change::Partial)
    }

    pub(super) fn period_reset_items(&self) -> Vec<Item> {
        let Some(pending) = &self.history_view.period_reset else {
            return Vec::new();
        };
        let count =
            self.reading_history
                .count_period_visits(pending.book, pending.from, pending.until);
        let mut items = vec![
            Item::new(self.history_title(pending.book), Action::Heading).detail(&pending.label),
            Item::new(
                if count == 1 {
                    "Remove 1 recorded visit".into()
                } else {
                    format!("Remove {count} recorded visits")
                },
                Action::Heading,
            ),
        ];
        if count != 0 {
            items.push(Item::new(
                "Reset statistics for this period",
                Action::ReadingHistory(Command::ConfirmPeriodReset),
            ));
        }
        items.push(Item::new(
            "Cancel",
            Action::ReadingHistory(Command::CancelReset),
        ));
        items
    }

    pub(super) fn confirm_period_reset(&mut self) -> Result<()> {
        if self.screen != Screen::ReadingHistory(Page::ConfirmPeriodReset) {
            return Ok(());
        }
        let Some(pending) = &self.history_view.period_reset else {
            return Ok(());
        };
        let origin = pending.origin;
        let removed = self.reading_history.reset_period(
            pending.book,
            pending.from,
            pending.until,
            Instant::now(),
            crate::mark_time::now(),
        )?;
        match origin {
            Page::PeriodBooks(period, start) => self.refresh_period_report(period, start),
            Page::Range(ranges::Page::Books(ranges::BookGroup::Day { range, day, period })) => {
                self.refresh_range_books(range, day, period)
            }
            _ => {}
        }
        self.close_period_reset();
        self.notice = Some(
            if removed == 0 {
                "No statistics reset"
            } else {
                "Statistics reset for this period"
            }
            .into(),
        );
        Ok(())
    }

    pub(super) fn close_period_reset(&mut self) {
        let Some(pending) = self.history_view.period_reset.take() else {
            self.show(Screen::ReadingHistory(Page::TimeRange));
            return;
        };
        self.show(Screen::ReadingHistory(pending.origin));
        let rows = self.list_geometry().rows;
        self.list_page = pending
            .page_index
            .min(self.list_window().0.saturating_sub(1) / rows);
    }
}
