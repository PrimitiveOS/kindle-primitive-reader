//! Render the weekly progress snapshot and return to the originating statistics list.
use super::{BLACK, CAPTION, Canvas, Change, Input, Reader, Rect, TextStyle, Zone};
use crate::reading_stats::history::report::{PROGRESS_DAYS, Progress};

#[derive(Default)]
pub(super) struct View {
    pub(super) summary_page: usize,
    snapshot: Option<Progress>,
}

const WEEKDAYS: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];

fn duration(seconds: u64) -> String {
    format!(
        "{}:{:02}:{:02}",
        seconds / 3600,
        seconds / 60 % 60,
        seconds % 60
    )
}

fn metric(canvas: &mut Canvas, rect: Rect, label: &str, value: &str) {
    canvas.text(
        Rect::new(rect.x + 9, rect.y + 6, rect.w - 18, 43),
        label,
        CAPTION.center(),
    );
    canvas.text(
        Rect::new(rect.x + 9, rect.y + 50, rect.w - 18, rect.h - 59),
        value,
        TextStyle::new(39).bold().center(),
    );
}

impl Reader {
    pub(in crate::app) fn refresh_reading_progress(&mut self) -> bool {
        let next = self
            .reading_history
            .progress(crate::mark_time::now(), Zone::Local);
        let changed = self.history_view.progress.snapshot != next;
        self.history_view.progress.snapshot = next;
        changed
    }

    pub(super) fn draw_reading_progress(&self, canvas: &mut Canvas) {
        let language = self.controls.interface_language;
        let layout = self.navigation_layout();
        layout.header_action(
            canvas,
            language.history_progress_text("Reading progress"),
            language.history_progress_text("Close"),
        );
        let Some(progress) = &self.history_view.progress.snapshot else {
            canvas.text(
                layout.body(236, 80),
                language.history_progress_text("No reading recorded in the last week."),
                crate::app::BODY.center(),
            );
            return;
        };
        let metric_height = if layout.width > layout.height {
            108
        } else {
            138
        };
        let heading = TextStyle::new(36).bold();
        let (seconds, screens) = progress.week_totals();
        let week_summary = layout.body(229, metric_height);
        let quarter = week_summary.w / 4;
        canvas.text(
            layout.body(169, 48),
            language.history_progress_text("Last week"),
            heading,
        );
        canvas.card(week_summary);
        for (column, (label, value)) in [
            ("Total pages", screens.to_string()),
            ("Total time", duration(seconds)),
            ("Pages per day", (screens / PROGRESS_DAYS).to_string()),
            ("Time per day", duration(seconds / PROGRESS_DAYS as u64)),
        ]
        .into_iter()
        .enumerate()
        {
            metric(
                canvas,
                Rect::new(
                    week_summary.x + quarter * column as i32,
                    week_summary.y,
                    quarter,
                    metric_height,
                ),
                language.history_progress_text(label),
                &value,
            );
        }

        let week_heading = week_summary.bottom() + 30;
        canvas.text(
            layout.body(week_heading, 46),
            language.history_progress_text("Week progress"),
            heading,
        );
        let chart_y = week_heading + 57;
        let totals_y = layout.height - metric_height - 37;
        let chart_bottom = totals_y - 62;
        let row_height = (chart_bottom - chart_y) / PROGRESS_DAYS as i32;
        let longest = progress
            .days
            .iter()
            .map(|day| day.totals.seconds)
            .max()
            .unwrap_or(0)
            .max(1);
        for (index, day) in progress.days.iter().enumerate() {
            let row = layout.body(chart_y + index as i32 * row_height, row_height - 12);
            let heading_height = if row_height < 96 { 44 } else { 53 };
            let weekday = language.history_calendar_text(WEEKDAYS[usize::from(day.date.weekday())]);
            let date = format!("{weekday} · {}", day.date.label());
            let text = if index == 0 {
                TextStyle::new(31).bold()
            } else {
                TextStyle::new(31)
            };
            canvas.text(
                Rect::new(row.x, row.y, row.w - 275, heading_height),
                &date,
                text,
            );
            canvas.text(
                Rect::new(row.right() - 244, row.y, 244, heading_height),
                &duration(day.totals.seconds),
                text.center(),
            );
            let track = Rect::new(row.x, row.y + heading_height + 3, row.w, 10);
            canvas.fill(track, 221);
            if day.totals.seconds > 0 {
                let width = ((track.w as u64 * day.totals.seconds / longest) as i32).max(2);
                canvas.fill(Rect::new(track.x, track.y, width, track.h), BLACK);
            }
        }

        let baseline = layout.body(totals_y, metric_height);
        let today = progress.days[0].totals;
        for (index, (title, totals)) in [("Session", progress.session), ("Today", today)]
            .into_iter()
            .enumerate()
        {
            let rect = Rect::new(
                baseline.x + (baseline.w / 2 + 12) * index as i32,
                baseline.y,
                baseline.w / 2 - 12,
                baseline.h,
            );
            canvas.text(
                Rect::new(rect.x, rect.y - 50, rect.w, 42),
                language.history_progress_text(title),
                heading,
            );
            canvas.card(rect);
            let half = rect.w / 2;
            metric(
                canvas,
                Rect::new(rect.x, rect.y, half, rect.h),
                language.history_progress_text("Pages"),
                &totals.screens.to_string(),
            );
            metric(
                canvas,
                Rect::new(rect.x + half, rect.y, half, rect.h),
                language.history_progress_text("Time"),
                &duration(totals.seconds),
            );
        }
    }

    pub(super) fn reading_progress_input(&mut self, input: Input) -> Option<Change> {
        let close = match input {
            Input::PageBack | Input::PageForward => true,
            Input::Tap(x, y) => self.navigation_layout().back().contains(x, y),
            Input::Swipe { dx, dy, .. } => {
                let (x, y) = (dx.abs(), dy.abs());
                if x.min(y) > 55.0 && x.max(y) <= x.min(y) * 1.5 {
                    return Some(Change::Full);
                }
                dy > 55.0 && y > x * 1.5
            }
            _ => false,
        };
        Some(if close {
            self.history_back();
            Change::Partial
        } else {
            Change::None
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        app::{
            Screen,
            reading_history::{Command, Page},
            selection_viewport_tests::{adopt, capture, press, tap},
            tests::{Fixture, capture_test_screen},
        },
        reading_stats::history::date::Date,
        session::ReadingViewport,
    };
    use primitive_app::App;
    use std::{
        fs,
        time::{Duration, Instant},
    };

    #[test]
    fn week_progress_reaches_every_viewport_and_keeps_the_original_book_position() {
        let fixture = Fixture::new();
        let fonts = [
            crate::test_fixture("fonts/NotoSans-Regular.ttf"),
            crate::test_fixture("fonts/NotoSans-Bold.ttf"),
        ];
        let mut app = Reader::open(&fixture.root, &fixture.state, fonts.to_vec()).unwrap();
        app.open_path(&fixture.book).unwrap();
        let active = app.open_book_id().unwrap();
        let origin = app.current_position().unwrap();
        let bytes_before = fs::read(&fixture.book).unwrap();
        app.activate_reading_history(Command::Open(Some(active)))
            .unwrap();
        let open = |app: &mut Reader| {
            let row = app
                .list_window()
                .1
                .iter()
                .position(|item| item.title == "Reading progress")
                .unwrap();
            let rect = app.list_geometry().row(row);
            tap(app, rect);
            assert_eq!(app.screen, Screen::ReadingHistory(Page::Progress));
        };
        open(&mut app);
        assert!(app.history_view.progress.snapshot.is_none());
        assert_eq!(app.input(Input::PageBack), Change::Partial);
        assert_eq!(app.screen, Screen::ReadingHistory(Page::Summary));

        let today = Date::today();
        let wall_now = crate::mark_time::now().unwrap();
        for (index, (days_ago, book, page, seconds)) in [
            (6, active, 1, 42u64),
            (4, 11, 8, 95),
            (2, 22, 19, 68),
            (1, 11, 7, 34),
            (0, active, 3, 87),
            (0, 11, 8, 30),
        ]
        .into_iter()
        .enumerate()
        {
            app.reading_history.discard_active();
            let date = today.add_days(-days_ago).unwrap();
            let start_of_day = Zone::Local.range(date, 0).unwrap().0;
            let wall = (start_of_day + 36_000 + 240 * index as i64).min(wall_now);
            let at = Instant::now();
            app.reading_history
                .set_page_at(Some((book, page)), at, Some(wall));
            app.reading_history.set_page_at(
                None,
                at + Duration::from_secs(seconds),
                Some(wall + seconds as i64),
            );
        }
        app.reading_history
            .begin_session(Some(Zone::Local.range(today, 0).unwrap().0));
        app.save().unwrap();
        let journal = fixture.root.join("reading-history.json");
        let saved = fs::read(&journal).unwrap();
        open(&mut app);
        let report = app.history_view.progress.snapshot.as_ref().unwrap();
        assert_eq!(report.week_totals(), (356, 6));
        assert_eq!(
            (report.days[0].totals.seconds, report.days[0].totals.screens),
            (117, 2)
        );
        assert_eq!((report.session.seconds, report.session.screens), (117, 2));
        capture_test_screen(&app, &fonts, "READER_WEEK_PROGRESS_SCREEN", false);
        assert_eq!(
            app.input(Input::Swipe {
                dx: 95.0,
                dy: 0.0,
                start_y: 708.0
            }),
            Change::None
        );
        assert_eq!(
            app.input(Input::Swipe {
                dx: 105.0,
                dy: 97.0,
                start_y: 708.0
            }),
            Change::Full
        );
        assert_eq!(app.input(Input::PageForward), Change::Partial);
        assert_eq!(app.current_position().unwrap(), origin);
        open(&mut app);
        let mut canvas = adopt(&mut app, ReadingViewport::LANDSCAPE, 1);
        let back = app.navigation_layout().back();
        press(&app, &mut canvas, back, true);
        capture(&app, &mut canvas, "landscape-reading-progress.png", true);
        tap(&mut app, back);
        assert_eq!(app.history_view.book, Some(active));
        adopt(&mut app, ReadingViewport::PORTRAIT, 2);
        open(&mut app);
        assert_eq!(
            app.input(Input::Swipe {
                dx: 0.0,
                dy: 112.0,
                start_y: 650.0
            }),
            Change::Partial
        );
        assert_eq!(app.screen, Screen::ReadingHistory(Page::Summary));
        assert_eq!(app.current_position().unwrap(), origin);
        app.reading_history.begin_session(Some(
            Zone::Local.range(today.add_days(-6).unwrap(), 0).unwrap().0,
        ));
        open(&mut app);
        assert_eq!(
            app.history_view
                .progress
                .snapshot
                .as_ref()
                .unwrap()
                .session
                .seconds,
            356
        );
        app.input(Input::Sleep);
        assert_eq!(app.input(Input::Wake), Change::Partial);
        let resumed = app.history_view.progress.snapshot.as_ref().unwrap();
        assert_eq!(resumed.week_totals(), (356, 6));
        assert!(resumed.session.seconds <= 117);
        assert_eq!(app.input(Input::PageBack), Change::Partial);
        assert_eq!(fs::read(journal).unwrap(), saved);
        assert_eq!(fs::read(&fixture.book).unwrap(), bytes_before);
    }
}
