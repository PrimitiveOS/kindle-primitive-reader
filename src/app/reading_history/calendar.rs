//! Month cells and hourly day timelines use the same native Canvas controls.
use super::calendar_options::timeline_clock;
use super::*;

const DAYS_PER_WEEK: usize = 7;
const MAX_BOOK_ROWS: usize = 5;

fn week_book_rows(
    days: [&[u64]; DAYS_PER_WEEK],
    limit: usize,
) -> [[Option<u64>; MAX_BOOK_ROWS]; DAYS_PER_WEEK] {
    let mut rows = [[None; MAX_BOOK_ROWS]; DAYS_PER_WEEK];
    for (day, books) in days.into_iter().enumerate() {
        let books = &books[..books.len().min(limit)];
        let (earlier, remaining) = rows.split_at_mut(day);
        let slots = &mut remaining[0][..limit];
        for (slot, &book) in slots.iter_mut().zip(books) {
            *slot = Some(book);
        }
        if let Some(previous) = earlier.last() {
            for (old_slot, &book) in previous[..limit].iter().enumerate() {
                if book.is_some()
                    && let Some(new_slot) = slots.iter().position(|&id| id == book)
                {
                    slots.swap(old_slot, new_slot);
                }
            }
        }
    }
    rows
}

#[cfg(test)]
#[test]
fn calendar_spans_keep_contiguous_books_in_their_rows_after_duration_rank_changes() {
    let days: [&[u64]; 7] = [&[], &[1, 2, 3], &[3, 2, 1], &[3, 4, 2], &[], &[3, 2], &[2]];
    let rows = week_book_rows(days, 2);
    assert_eq!(
        rows.map(|day| [day[0], day[1]]),
        [
            [None, None],
            [Some(1), Some(2)],
            [Some(3), Some(2)],
            [Some(3), Some(4)],
            [None, None],
            [Some(3), Some(2)],
            [None, Some(2)],
        ]
    );
    assert!(rows.iter().all(|row| row[2..].iter().all(Option::is_none)));
    let swapped = week_book_rows([&[1, 2, 3], &[4, 5, 1], &[], &[], &[], &[], &[]], 3);
    assert_eq!(swapped[1][..3], [Some(1), Some(5), Some(4)]);
}

impl Reader {
    pub(in crate::app) fn draw_reading_history(&self, canvas: &mut Canvas) -> bool {
        let Screen::ReadingHistory(page) = self.screen else {
            return false;
        };
        if page == Page::Progress {
            self.draw_reading_progress(canvas);
            return true;
        }
        if !matches!(page, Page::Calendar | Page::Day(_)) {
            return false;
        }
        header(canvas, page.title(), "Back");
        canvas.button(SUMMARY, "Summary", false, false);
        canvas.button(DAILY, "Daily history", false, false);
        canvas.button(TODAY, "Today", false, false);
        let label = if let Page::Day(day) = page {
            format!(
                "{} · {} start",
                day.label(),
                timeline_clock(self.reading_history.options().day_start),
            )
        } else {
            self.history_view.month.month_label()
        };
        canvas.text(Rect::new(220, 146, 824, 80), &label, BODY.bold().center());
        let (prev, next) = if let Page::Day(day) = page {
            (
                day.add_days(-1).is_some(),
                day < self.history_timeline_today(),
            )
        } else {
            (
                self.history_view
                    .month
                    .add_months(-1)
                    .is_some_and(|m| self.history_month_allowed(m)),
                self.history_view
                    .month
                    .add_months(1)
                    .is_some_and(|m| self.history_month_allowed(m)),
            )
        };
        for (rect, icon, available) in [
            (PREVIOUS, crate::app::bar_icons::PREVIOUS_PAGE, prev),
            (NEXT, crate::app::bar_icons::NEXT_PAGE, next),
        ] {
            if available {
                let pressed = canvas.control_pressed(rect, false);
                canvas.button(rect, "", false, false);
                canvas.icon(
                    Rect::new(
                        rect.x + (rect.w - 40) / 2,
                        rect.y + (rect.h - 40) / 2,
                        40,
                        40,
                    ),
                    icon,
                    if pressed { 255 } else { 0 },
                );
            }
        }
        match page {
            Page::Calendar => self.draw_history_calendar(canvas),
            Page::Day(day) => self.draw_history_day(canvas, day),
            _ => {}
        }
        canvas.button(OPTIONS, "Settings", false, false);
        canvas.button(
            FILTER,
            &self
                .history_view
                .book
                .map_or_else(|| "All books".into(), |id| self.history_title(id)),
            false,
            false,
        );
        true
    }
    fn draw_history_calendar(&self, canvas: &mut Canvas) {
        let month = self.history_view.month;
        let options = self.reading_history.options();
        let week = options.week_start;
        let today = Date::today();
        let limit = usize::from(options.calendar_book_spans);
        let offset = (month.weekday() + 7 - week) % 7;
        for col in 0..7 {
            canvas.text(
                Rect::new(40 + col * 169, 328, 164, 40),
                ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"]
                    [(col as usize + week as usize) % 7],
                CAPTION.center(),
            );
        }
        let days = self
            .reading_history
            .calendar_month(month, self.history_view.book, Zone::Local);
        for day in 1..=month.month_days() {
            let date = Date { day, ..month };
            let rect = cell(usize::from(offset + day - 1));
            let allowed = date <= today;
            let pressed = allowed && canvas.control_pressed(rect, false);
            let ink = if pressed {
                255
            } else if allowed {
                0
            } else {
                178
            };
            canvas.panel(
                rect,
                14,
                if date == today { 3 } else { 1 },
                if pressed { 0 } else { 255 },
            );
            canvas.text(
                Rect::new(rect.x + 10, rect.y + 5, 39, 38),
                &day.to_string(),
                TextStyle::new(30).bold().ink(ink),
            );
            if let Some(day) = days.get(&date) {
                canvas.text(
                    Rect::new(rect.x + 51, rect.y + 7, rect.w - 59, 34),
                    &short_time(day.seconds),
                    TextStyle::new(21).ink(ink).center(),
                );
                let hidden = day.books.len().saturating_sub(limit);
                if hidden > 0 {
                    canvas.text(
                        Rect::new(rect.right() - 57, rect.bottom() - 21, 50, 19),
                        &format!("+{hidden}"),
                        TextStyle::new(16).ink(ink).center(),
                    );
                }
                if options.histogram {
                    let chart_width = if hidden > 0 { rect.w - 71 } else { rect.w - 20 };
                    let step = chart_width / 24;
                    let baseline = rect.bottom() - 5;
                    canvas.fill(Rect::new(rect.x + 10, baseline - 1, 24 * step, 1), ink);
                    for (hour, &seconds) in day.hours.iter().enumerate() {
                        if seconds > 0 {
                            let height = (seconds.min(3600) * 15 / 3600).max(2) as i32;
                            canvas.fill(
                                Rect::new(
                                    rect.x + 10 + hour as i32 * step,
                                    baseline - height,
                                    step - 1,
                                    height,
                                ),
                                ink,
                            );
                        }
                    }
                }
            }
        }
        let last = usize::from(offset + month.month_days());
        let stride = 94 / i32::from(options.calendar_book_spans);
        let font = (stride - 5).min(24);
        for first in (0..last).step_by(DAYS_PER_WEEK) {
            let ranked = std::array::from_fn(|column| {
                let day = (first + column + 1).checked_sub(usize::from(offset));
                day.and_then(|day| Date::new(month.year, month.month, day as u8))
                    .and_then(|date| days.get(&date))
                    .map_or(&[][..], |entry| entry.books.as_slice())
            });
            let week = week_book_rows(ranked, limit);
            for row in 0..limit {
                for (column, slots) in week.iter().enumerate() {
                    let Some(id) = slots[row] else { continue };
                    if column > 0 && week[column - 1][row] == Some(id) {
                        continue;
                    }
                    let length = week[column..]
                        .iter()
                        .take_while(|slots| slots[row] == Some(id))
                        .count();
                    let start = cell(first + column);
                    let end = cell(first + column + length - 1);
                    let rect = Rect::new(
                        start.x + 7,
                        start.y + 42 + row as i32 * stride,
                        end.right() - start.x - 14,
                        stride - 2,
                    );
                    canvas.panel(rect, 4, 1, [235, 220, 205][(id % 3) as usize]);
                    canvas.text(
                        Rect::new(rect.x + 5, rect.y, rect.w - 10, rect.h),
                        &self.history_title(id),
                        TextStyle::new(font).center(),
                    );
                }
            }
        }
    }
    fn draw_history_day(&self, canvas: &mut Canvas, day: Date) {
        let total = self
            .reading_history
            .day_seconds(day, self.history_view.book, Zone::Local);
        canvas.text(
            Rect::new(40, 334, 1184, 56),
            &format!("{} on this timeline", short_time(total)),
            BODY.bold(),
        );
        if self.reading_history.options().histogram {
            let bins = self
                .reading_history
                .hourly(day, self.history_view.book, Zone::Local);
            let max = bins.iter().copied().max().unwrap_or(1).max(1);
            let origin = self.reading_history.options().day_start;
            for (index, &seconds) in bins.iter().enumerate() {
                let height = (u64::from(seconds) * 130 / u64::from(max)) as i32;
                canvas.fill(
                    Rect::new(54 + index as i32 * 48, 540 - height, 36, height),
                    BLACK,
                );
                if index % 6 == 0 {
                    canvas.text(
                        Rect::new(50 + index as i32 * 48, 550, 78, 36),
                        &timeline_clock((origin + index as u16 * 60) % 1440),
                        TextStyle::new(22),
                    );
                }
            }
        }
        let rows = runs(&self.reading_history, day, self.history_view.book);
        let first = self.history_view.run_page * RUN_ROWS;
        if rows.is_empty() {
            canvas.text(
                Rect::new(40, RUN_Y, 1184, 100),
                "No recorded reading in this day.",
                BODY,
            );
        }
        for (i, run) in rows.iter().skip(first).take(RUN_ROWS).enumerate() {
            let rect = Rect::new(40, RUN_Y + i as i32 * 100, 1184, 96);
            let pressed = canvas.control_pressed(rect, false);
            if pressed {
                canvas.fill(rect, BLACK);
            }
            let ink = if pressed { 255 } else { 0 };
            canvas.text(
                Rect::new(rect.x + 15, rect.y + 4, 850, 42),
                &self.history_title(run.book),
                BODY.ink(ink),
            );
            canvas.text(
                Rect::new(rect.x + 15, rect.y + 48, 1100, 36),
                &format!(
                    "{}–{} · {}",
                    clock(run.start),
                    clock(run.end),
                    short_time(run.seconds)
                ),
                CAPTION.ink(ink),
            );
            canvas.fill(Rect::new(rect.x, rect.bottom(), rect.w, 1), HAIRLINE);
        }
        if self.history_view.run_page > 0 {
            canvas.button(
                Rect::new(40, 1450, 330, 70),
                "Earlier sessions",
                false,
                false,
            );
        }
        if first + RUN_ROWS < rows.len() {
            canvas.button(
                Rect::new(894, 1450, 330, 70),
                "Later sessions",
                false,
                false,
            );
        }
    }
}
