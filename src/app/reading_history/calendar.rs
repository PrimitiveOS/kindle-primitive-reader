//! Month cells and hourly day timelines use the same native Canvas controls.
use super::*;

impl Reader {
    pub(in crate::app) fn draw_reading_history(&self, canvas: &mut Canvas) -> bool {
        let Screen::ReadingHistory(page) = self.screen else {
            return false;
        };
        if !matches!(page, Page::Calendar | Page::Day(_)) {
            return false;
        }
        header(canvas, page.title(), "Back");
        canvas.button(SUMMARY, "Summary", false, false);
        canvas.button(DAILY, "Daily history", false, false);
        canvas.button(TODAY, "Today", false, false);
        let label = if let Page::Day(day) = page {
            format!(
                "{} · {:02}:{:02} start",
                day.label(),
                self.reading_history.options().day_start / 60,
                self.reading_history.options().day_start % 60
            )
        } else {
            self.history_view.month.month_label()
        };
        canvas.text(Rect::new(220, 146, 824, 80), &label, BODY.bold().center());
        let (prev, next) = if let Page::Day(day) = page {
            (day.add_days(-1).is_some(), day < Date::today())
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
        let week = self.reading_history.options().week_start;
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
        let total_max = days
            .values()
            .map(|(seconds, _)| *seconds)
            .max()
            .unwrap_or(0)
            .max(1);
        for day in 1..=month.month_days() {
            let date = Date { day, ..month };
            let rect = cell(usize::from(offset + day - 1));
            let allowed = date <= Date::today();
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
                if date == Date::today() { 3 } else { 1 },
                if pressed { 0 } else { 255 },
            );
            canvas.text(
                Rect::new(rect.x + 10, rect.y + 7, rect.w - 20, 38),
                &day.to_string(),
                TextStyle::new(30).bold().ink(ink),
            );
            let seconds = days.get(&date).map_or(0, |(seconds, _)| *seconds);
            if seconds > 0 {
                canvas.text(
                    Rect::new(rect.x + 10, rect.y + 45, rect.w - 20, 34),
                    &short_time(seconds),
                    TextStyle::new(25).ink(ink),
                );
                let books = &days[&date].1;
                canvas.text(
                    Rect::new(rect.x + 10, rect.y + 84, rect.w - 20, 32),
                    &format!(
                        "{} {}",
                        books.len(),
                        if books.len() == 1 { "book" } else { "books" }
                    ),
                    TextStyle::new(22).ink(ink),
                );
                canvas.fill(
                    Rect::new(
                        rect.x + 10,
                        rect.bottom() - 19,
                        ((rect.w - 20) as u64 * seconds / total_max).max(2) as i32,
                        7,
                    ),
                    ink,
                );
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
            let origin = self.reading_history.options().day_start / 60;
            for index in 0..24 {
                let hour = (index + origin as usize) % 24;
                let height = (u64::from(bins[hour]) * 130 / u64::from(max)) as i32;
                canvas.fill(
                    Rect::new(54 + index as i32 * 48, 540 - height, 36, height),
                    BLACK,
                );
                if index % 6 == 0 {
                    canvas.text(
                        Rect::new(50 + index as i32 * 48, 550, 78, 36),
                        &format!("{hour:02}:00"),
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
