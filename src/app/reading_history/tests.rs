use super::*;
use crate::app::tests::{Fixture, capture_test_screen};
use primitive_app::{App, Lifecycle};
use std::{fs, path::PathBuf, time::Duration};
fn setup(f: &Fixture) -> Reader {
    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    Reader::open(&f.root, &f.state, fonts).unwrap()
}
fn seed(app: &mut Reader, id: u64, title: &str, day: Date, seconds: u64) {
    let wall = Zone::Local.range(day, 0).unwrap().0 + 10 * 3600;
    let now = Instant::now();
    app.reading_history.discard_active();
    app.reading_history.remember_title(id, title);
    app.reading_history
        .set_page_at(Some((id, 1)), now, Some(wall));
    app.reading_history.set_page_at(
        None,
        now + Duration::from_secs(seconds),
        Some(wall + seconds as i64),
    );
}
#[test]
fn calendar_daily_book_filters_and_settings_preserve_the_reading_position() {
    let fixture = Fixture::new();
    let mut app = setup(&fixture);
    app.open_path(&fixture.book).unwrap();
    let id = app.open_book_id().unwrap();
    let before = app.current_position().unwrap();
    let today = Date::today();
    seed(&mut app, id, "Field notes", today, 90);
    seed(&mut app, 123, "Geometry notebook", today, 30);
    seed(&mut app, id, "Field notes", today.add_days(-1).unwrap(), 60);
    app.activate(Action::ReadingHistory(Command::Open(Some(id))))
        .unwrap();
    assert_eq!(app.history_view.book, Some(id));
    assert!(app.items().iter().any(|r| r.title == "Calendar"));
    capture_test_screen(&app, &app.fonts, "READER_STATS_SUMMARY_SCREEN", false);
    app.activate_reading_history(Command::Show(Page::Calendar))
        .unwrap();
    capture_test_screen(&app, &app.fonts, "READER_STATS_CALENDAR_SCREEN", false);
    let offset = (today.first().weekday() + 7 - app.reading_history.options().week_start) % 7;
    let rect = cell(usize::from(offset + today.day - 1));
    assert_eq!(
        app.input(Input::Tap((rect.x + 20) as f32, (rect.y + 20) as f32)),
        Change::Partial
    );
    assert_eq!(app.screen, Screen::ReadingHistory(Page::Day(today)));
    capture_test_screen(&app, &app.fonts, "READER_STATS_DAY_SCREEN", false);
    app.activate_reading_history(Command::Show(Page::BookDay(today, id)))
        .unwrap();
    assert!(
        app.items()
            .iter()
            .any(|row| row.title == "Continue reading")
    );
    capture_test_screen(&app, &app.fonts, "READER_STATS_BOOK_DAY_SCREEN", false);
    app.history_back();
    let next = app.history_view.month.add_months(1).unwrap();
    app.activate_reading_history(Command::Show(Page::Calendar))
        .unwrap();
    app.activate_reading_history(Command::Month(1)).unwrap();
    assert_ne!(app.history_view.month, next);
    let options = Options {
        future_months: true,
        ..app.reading_history.options()
    };
    app.activate_reading_history(Command::Configure(options))
        .unwrap();
    capture_test_screen(&app, &app.fonts, "READER_STATS_OPTIONS_SCREEN", false);
    app.activate_reading_history(Command::Show(Page::Calendar))
        .unwrap();
    app.input(Input::PageForward);
    assert_eq!(app.history_view.month, next);
    app.input(Input::PageBack);
    assert_eq!(app.history_view.month, today.first());
    app.activate_reading_history(Command::Month(2)).unwrap();
    assert!(app.history_view.month > today.first());
    app.activate_reading_history(Command::Configure(Options {
        future_months: false,
        ..app.reading_history.options()
    }))
    .unwrap();
    assert_eq!(app.history_view.month, today.first());
    app.activate_reading_history(Command::Show(Page::Daily))
        .unwrap();
    capture_test_screen(&app, &app.fonts, "READER_STATS_DAILY_SCREEN", false);
    assert_eq!(app.items().len(), 2);
    app.activate_reading_history(Command::Filter(None)).unwrap();
    assert_eq!(app.reading_history.summary(None, None).books, 2);
    app.activate_reading_history(Command::Show(Page::Books))
        .unwrap();
    capture_test_screen(&app, &app.fonts, "READER_STATS_BOOKS_SCREEN", false);
    assert!(app.items().iter().any(|r| r.title == "Geometry notebook"));
    app.history_back();
    app.history_back();
    assert_eq!(app.screen, Screen::Book);
    assert_eq!(app.current_position().unwrap(), before);
    app.save().unwrap();
    let restored = setup(&fixture);
    assert_eq!(restored.reading_history.summary(None, None).seconds, 180);
}

#[test]
fn reading_time_range_drills_into_books_and_returns_to_the_same_report_page() {
    let fixture = Fixture::new();
    let mut app = setup(&fixture);
    app.open_path(&fixture.book).unwrap();
    let id = app.open_book_id().unwrap();
    let title = app.active.as_ref().unwrap().entry.title.clone();
    let original_position = app.current_position().unwrap();
    let today = Date::today();
    for week in 0..14 {
        seed(
            &mut app,
            id,
            &title,
            today.add_days(-7 * week).unwrap(),
            70 + week as u64 * 2,
        );
    }
    seed(&mut app, 54, "Harbor Notebook", today, 85);
    seed(&mut app, 75, "Letters from the Coast", today, 35);
    app.save().unwrap();
    let journal = fixture.root.join("reading-history.json");
    let before = fs::read(&journal).unwrap();

    let tap = |app: &mut Reader, index: usize| {
        let rect = app.list_geometry().row(index);
        assert_eq!(
            app.input(Input::Tap((rect.x + 20) as f32, (rect.y + 20) as f32)),
            Change::Partial
        );
    };
    let tap_row = |app: &mut Reader, title: &str| {
        let index = app
            .list_window()
            .1
            .iter()
            .position(|row| row.title == title)
            .unwrap();
        tap(app, index);
    };
    app.activate_reading_history(Command::Open(None)).unwrap();
    tap_row(&mut app, "Time range");
    capture_test_screen(&app, &app.fonts, "READER_STATS_TIME_RANGE_SCREEN", false);
    assert_report_feedback(&app, app.list_geometry().row(0));
    tap_row(&mut app, "Books by week");
    let week = app.list_window().1[0].title.clone();
    assert_eq!(app.list_window().0, 14);
    assert_eq!(
        app.list_window().1[0].detail.as_deref(),
        Some("3m 10s · 3 screens · 3 books")
    );
    capture_test_screen(&app, &app.fonts, "READER_STATS_WEEKLY_SCREEN", false);
    let rect = app.list_geometry().row(0);
    assert_report_feedback(&app, rect);
    tap(&mut app, 0);
    assert_eq!(app.list_window().1[0].title, week);
    assert_eq!(app.list_window().1.len(), 4);
    capture_test_screen(&app, &app.fonts, "READER_STATS_PERIOD_BOOKS_SCREEN", false);
    capture_test_screen(
        &app,
        &app.fonts,
        "READER_STATS_PERIOD_BOOKS_DARK_SCREEN",
        true,
    );
    assert_report_feedback(&app, app.list_geometry().row(1));
    tap_row(&mut app, "Harbor Notebook");
    let details = app.items();
    assert_eq!(details[0].title, "Harbor Notebook");
    assert_eq!(details[1].detail.as_deref(), Some("1m 25s"));
    assert!(!details.iter().any(|row| row.title == "Continue reading"));
    app.history_back();
    tap_row(&mut app, &title);
    capture_test_screen(&app, &app.fonts, "READER_STATS_PERIOD_BOOK_SCREEN", false);
    assert!(
        app.items()
            .iter()
            .any(|row| row.title == "Continue reading")
    );
    app.history_back();
    app.history_back();
    assert_eq!(app.input(Input::PageForward), Change::Partial);
    let later = app.list_window().1[0].title.clone();
    tap(&mut app, 0);
    app.history_back();
    assert_eq!(app.list_page, 1);
    assert_eq!(app.list_window().1[0].title, later);
    app.history_back();
    tap_row(&mut app, "Books by month");
    assert!(app.list_window().0 >= 3);
    capture_test_screen(&app, &app.fonts, "READER_STATS_MONTHLY_SCREEN", false);
    app.history_back();
    app.history_back();

    app.activate_reading_history(Command::Filter(Some(id)))
        .unwrap();
    tap_row(&mut app, "Time range");
    assert!(
        app.items()
            .iter()
            .any(|row| row.detail.as_deref() == Some("Book filter"))
    );
    tap_row(&mut app, "Books by week");
    tap(&mut app, 0);
    assert_eq!(app.items().len(), 2);
    assert_eq!(app.items()[1].title, title);
    for _ in 0..4 {
        app.history_back();
    }
    assert_eq!(app.screen, Screen::Book);
    assert_eq!(app.current_position().unwrap(), original_position);
    app.save().unwrap();
    assert_eq!(fs::read(journal).unwrap(), before);
}

fn assert_report_feedback(app: &Reader, rect: Rect) {
    let paths: Vec<_> = app.fonts.iter().map(|p| p.display().to_string()).collect();
    let mut canvas = Canvas::new(WIDTH, HEIGHT, &paths).unwrap();
    let normal = primitive_app::frame(app, &mut canvas, false);
    let pressed = Lifecycle::default()
        .press_feedback(
            app,
            &mut canvas,
            Input::Tap((rect.x + 20) as f32, (rect.y + 20) as f32),
            false,
        )
        .unwrap();
    assert!(normal != pressed);
    for (index, (a, b)) in normal.iter().zip(&pressed).enumerate() {
        if a != b {
            assert!(rect.contains(
                (index % WIDTH as usize) as f32,
                (index / WIDTH as usize) as f32
            ));
        }
    }
}
#[test]
fn calendar_day_and_shared_setting_controls_reverse_only_their_registered_rectangles() {
    let fixture = Fixture::new();
    let mut app = setup(&fixture);
    let today = Date::today();
    seed(&mut app, 1, "Field notes", today, 90);
    app.activate_reading_history(Command::Open(None)).unwrap();
    app.activate_reading_history(Command::Show(Page::Calendar))
        .unwrap();
    let offset = (today.first().weekday() + 7 - app.reading_history.options().week_start) % 7;
    let day = cell(usize::from(offset + today.day - 1));
    fn feedback(app: &Reader, rect: Rect) {
        let fonts: Vec<_> = app
            .fonts
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        let mut canvas = Canvas::new(WIDTH, HEIGHT, &fonts).unwrap();
        let normal = primitive_app::frame(app, &mut canvas, false);
        let pressed = Lifecycle::default()
            .press_feedback(
                app,
                &mut canvas,
                Input::Tap((rect.x + 20) as f32, (rect.y + 20) as f32),
                false,
            )
            .unwrap();
        let mut changes = 0;
        for (i, (&a, &b)) in normal.iter().zip(&pressed).enumerate() {
            if a != b {
                assert!(rect.contains((i % WIDTH as usize) as f32, (i / WIDTH as usize) as f32));
                changes += 1;
            }
        }
        assert!(changes > 100);
    }
    feedback(&app, day);
    app.activate_reading_history(Command::Show(Page::Day(today)))
        .unwrap();
    feedback(&app, PREVIOUS);
    feedback(&app, Rect::new(40, RUN_Y, 1184, 96));
    app.activate_reading_history(Command::Show(Page::Options))
        .unwrap();
    feedback(
        &app,
        Rect::new(
            super::super::LIST_X,
            super::super::LIST_Y,
            WIDTH as i32 - 2 * super::super::LIST_X,
            super::super::ROW_HEIGHT,
        ),
    );
}
#[test]
fn sleep_usb_and_finished_book_filter_leave_only_eligible_reading_sessions() {
    let fixture = Fixture::new();
    let mut app = setup(&fixture);
    app.open_path(&fixture.book).unwrap();
    let id = app.open_book_id().unwrap();
    app.reading_history
        .configure(
            Options {
                freeze_finished: true,
                ..app.reading_history.options()
            },
            Instant::now(),
            crate::mark_time::now(),
        )
        .unwrap();
    app.library
        .state_mut(id)
        .unwrap()
        .set_status(crate::library::ReadingStatus::Finished);
    app.update_stats_activity(Instant::now());
    assert!(app.reading_history.deadline().is_none());
    assert!(app.stats.deadline().is_none());
    app.input(Input::Sleep);
    assert!(!app.stats_awake);
    assert!(app.reading_history.deadline().is_none());
    app.input(Input::Wake);
    assert!(app.reading_history.deadline().is_none());
    app.reading_history
        .configure(
            Options {
                freeze_finished: false,
                ..app.reading_history.options()
            },
            Instant::now(),
            crate::mark_time::now(),
        )
        .unwrap();
    app.update_stats_activity(Instant::now());
    assert!(app.reading_history.deadline().is_some());
    assert!(app.stats.deadline().is_some());
    app.reading_history
        .configure(
            Options {
                enabled: false,
                ..app.reading_history.options()
            },
            Instant::now(),
            crate::mark_time::now(),
        )
        .unwrap();
    app.update_stats_activity(Instant::now());
    assert!(app.stats.deadline().is_none());
    assert!(app.reading_history.deadline().is_none());
    app.reading_history
        .configure(
            Options {
                enabled: true,
                ..app.reading_history.options()
            },
            Instant::now(),
            crate::mark_time::now(),
        )
        .unwrap();
    app.reading_history.discard_active();
    app.storage_returned().unwrap();
    assert!(app.reading_history.deadline().is_some());
}
#[test]
#[ignore = "Requires a privately selected list containing authorized original EPUB and PDF documents"]
fn original_documents_keep_their_positions_while_browsing_dated_statistics() {
    let list = PathBuf::from(std::env::var_os("READER_STATS_ORIGINAL_LIST").unwrap());
    let root = PathBuf::from(std::env::var_os("READER_STATS_ORIGINAL_ROOT").unwrap());
    let state = PathBuf::from(std::env::var_os("READER_STATS_ORIGINAL_STATE").unwrap());
    let fonts: Vec<_> =
        std::env::split_paths(&std::env::var_os("READER_STATS_ORIGINAL_FONTS").unwrap()).collect();
    fs::create_dir_all(&state).unwrap();
    let paths: Vec<_> = fs::read_to_string(list)
        .unwrap()
        .lines()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .collect();
    assert!(paths.len() >= 2);
    for (i, path) in paths.iter().enumerate() {
        let bytes = fs::read(path).unwrap();
        let mut app = Reader::open(&root, &state.join(i.to_string()), fonts.clone()).unwrap();
        app.open_path(path).unwrap();
        let id = app.open_book_id().unwrap();
        let before = app.library.state(id).unwrap().position.clone();
        seed(&mut app, id, "Original document", Date::today(), 90);
        app.activate_reading_history(Command::Open(Some(id)))
            .unwrap();
        app.activate_reading_history(Command::Show(Page::Calendar))
            .unwrap();
        app.activate_reading_history(Command::Show(Page::Day(Date::today())))
            .unwrap();
        app.history_back();
        app.history_back();
        app.history_back();
        assert!(matches!(app.screen, Screen::Book | Screen::Fixed));
        assert_eq!(app.library.state(id).unwrap().position, before);
        app.save().unwrap();
        assert_eq!(app.reading_history.summary(Some(id), None).seconds, 90);
        assert_eq!(fs::read(path).unwrap(), bytes);
    }
}

#[test]
fn page_buttons_browse_long_day_lists_before_changing_the_day() {
    let fixture = Fixture::new();
    let mut app = setup(&fixture);
    let today = Date::today();
    for id in 1..=9 {
        seed(&mut app, id, &format!("Notebook {id}"), today, 30);
    }
    app.activate_reading_history(Command::Open(None)).unwrap();
    app.activate_reading_history(Command::Show(Page::Day(today)))
        .unwrap();
    app.input(Input::PageForward);
    assert_eq!(app.history_view.run_page, 1);
    assert_eq!(app.screen, Screen::ReadingHistory(Page::Day(today)));
    app.input(Input::PageBack);
    assert_eq!(app.history_view.run_page, 0);
    app.input(Input::PageBack);
    assert_eq!(
        app.screen,
        Screen::ReadingHistory(Page::Day(today.add_days(-1).unwrap()))
    );
}
