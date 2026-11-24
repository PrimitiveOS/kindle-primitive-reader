use super::popup::*;
use super::*;
use crate::app::tests::{Fixture, capture_test_screen};

fn touch(rect: Rect) -> Input {
    Input::Tap((rect.x + rect.w / 2) as f32, (rect.y + rect.h / 2) as f32)
}

fn frame(app: &Reader, fonts: &[PathBuf]) -> Vec<u8> {
    let names = fonts
        .iter()
        .map(|f| f.display().to_string())
        .collect::<Vec<_>>();
    let mut canvas = Canvas::new(WIDTH, HEIGHT, &names).unwrap();
    primitive_app::frame(app, &mut canvas, false)
}

fn open_tools(app: &mut Reader) {
    assert_eq!(app.open_reading_edge(true, 630.), Change::Partial);
    if app.top_group != Some(TopGroup::Tools) {
        let tools = app.reading_button("Tools");
        app.input(touch(tools));
    }
    let alarm = app.reading_button("Read timer");
    assert_eq!(app.input(touch(alarm)), Change::Partial);
    assert!(app.read_alarm.panel == Some(Panel::Settings));
}

#[test]
fn read_alarm_interval_preserves_page_persists_choices_and_repeats_once() {
    let fixture = Fixture::new();
    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
    let specimen = fixture.root.join("Early light.xhtml");
    fs::write(&specimen, r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Early light</title></head>
            <body><h1>Early light</h1><p>Just after dawn, the path along the shore was quiet. We stopped beside the boathouse to watch the tide turn.</p>
            <p>At the end of the pier, someone had left a notebook open on the rail. A page lifted in the breeze and settled again.</p></body></html>"#).unwrap();
    app.open_path(&specimen).unwrap();
    let original = frame(&app, &fonts);
    let location = app.current_history_location().unwrap();
    open_tools(&mut app);
    capture_test_screen(&app, &fonts, "READER_ALARM_SETTINGS_SCREEN", false);
    assert_eq!(app.input(touch(PICK_ALARM)), Change::Partial);
    capture_test_screen(&app, &fonts, "READER_ALARM_CLOCK_PICKER_SCREEN", false);
    assert_eq!(app.input(Input::PageBack), Change::Partial);
    assert_eq!(app.input(touch(PICK_INTERVAL)), Change::Partial);
    assert_eq!(app.input(touch(START)), Change::None);
    let Input::Tap(x, y) = touch(MIN_UP) else {
        unreachable!()
    };
    app.input(Input::LongTap(x, y));
    app.input(touch(MIN_UP));
    assert!(matches!(
        app.read_alarm.panel,
        Some(Panel::Set(Kind::Interval, 11))
    ));
    assert_eq!(app.input(touch(START)), Change::Partial);
    assert_eq!(app.read_alarm.options.minutes, 11);
    let deadline = app.read_alarm.running.unwrap().due;
    app.poll_read_alarm(deadline - Duration::from_secs(350), false);
    assert_eq!(frame(&app, &fonts), original);
    let position = app.current_history_location().unwrap();
    assert_eq!(position, location);
    assert_eq!(app.poll_read_alarm(deadline, false), Change::Partial);
    assert!(
        app.read_alarm
            .alert
            .as_ref()
            .unwrap()
            .repeat_interval
            .is_some()
    );
    capture_test_screen(&app, &fonts, "READER_ALARM_EXPIRY_SCREEN", false);
    assert_ne!(frame(&app, &fonts), original);
    app.input(touch(REPEAT));
    assert_eq!(
        app.read_alarm.running.unwrap().repeat_interval,
        Some(Duration::from_secs(660))
    );
    let next = app.read_alarm.running.unwrap().due;
    assert_eq!(app.poll_read_alarm(next, false), Change::Partial);
    assert_eq!(app.input(Input::PageBack), Change::Partial);
    assert_eq!(frame(&app, &fonts), original);
    assert_eq!(app.current_history_location().unwrap(), location);
    let reopened = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
    assert_eq!(reopened.read_alarm.options.minutes, 11);
    assert!(reopened.read_alarm.running.is_none());
}

#[test]
fn read_alarm_monotonic_expiry_wake_stop_and_notification_match_stock() {
    let fixture = Fixture::new();
    let mut timer = ReadAlarm::open(fixture.root.join("timer.json"));
    let now = Instant::now();
    let interval = Duration::from_secs(120);
    timer.options.repeat = true;
    timer.options.expiry = Expiry::Notification;
    timer.start(now, interval, Some(interval));
    assert_eq!(
        timer.tick(now + Duration::from_secs(30), true),
        Change::None
    );
    assert_eq!(
        timer.caption(now + Duration::from_secs(30)).as_deref(),
        Some("00:02")
    );
    assert_eq!(timer.tick(now + interval, false), Change::Partial);
    assert!(timer.alert.as_ref().unwrap().hide == Some(now + interval + Duration::from_secs(5)));
    assert_eq!(timer.running.unwrap().due, now + interval * 2);
    assert_eq!(
        timer.tick(now + interval + Duration::from_secs(5), false),
        Change::Partial
    );
    assert!(timer.alert.is_none());
    assert_eq!(timer.tick(now + interval * 2, true), Change::Partial);
    assert!(timer.running.is_none());
    timer.options.expiry = Expiry::Silent;
    timer.start(now, interval, Some(interval));
    timer.tick(now + interval, false);
    assert!(timer.alert.is_none());
    assert_eq!(timer.running.unwrap().due, now + interval * 2);
    timer.panel = Some(Panel::Settings);
    timer.handle(touch(STOP), now, SystemTime::now()).unwrap();
    assert!(timer.running.is_none());
    assert_eq!(timer.tick(now + interval * 4, true), Change::None);
}

#[test]
fn read_alarm_local_clock_selects_the_next_occurrence_and_preserves_other_settings() {
    let fixture = Fixture::new();
    let mut timer = ReadAlarm::open(fixture.root.join("timer.json"));
    let now = Instant::now();
    let wall = SystemTime::now();
    let local = local_clock(wall).unwrap();
    let minutes = (local.hour * 60 + local.minute) as u16;
    let next = clock_occurrence(wall, minutes).unwrap();
    // A next-day local occurrence can cross a daylight-saving boundary.
    assert!((22 * 3600..=26 * 3600).contains(&next.delay.as_secs()));
    assert!(clock_occurrence(wall, 24 * 60).is_none());
    timer.panel = Some(Panel::Settings);
    for rect in [MODE, MESSAGE] {
        timer.handle(touch(rect), now, wall).unwrap();
    }
    assert!(matches!(timer.panel, Some(Panel::EditMessage)));
    timer.handle(Input::PageBack, now, wall).unwrap();
    for rect in [HEADER, FOOTER, AUTO, PICK_ALARM, START] {
        timer.handle(touch(rect), now, wall).unwrap();
    }
    let running = timer.running.unwrap();
    assert!(running.repeat_interval.is_none());
    assert_eq!(running.due, now + next.delay);
    let confirmation = timer.alert.as_ref().unwrap();
    assert_eq!(confirmation.text, next.confirmation());
    assert_eq!(confirmation.hide, Some(now + Duration::from_secs(5)));
    assert!(!timer.modal());
    assert!(timer.footer().is_some());
    assert!(timer.options.repeat && timer.options.header && timer.options.footer);
    assert!(timer.options.expiry == Expiry::Notification);
    let restored = ReadAlarm::open(timer.file.clone());
    assert!(
        restored.running.is_none()
            && restored.options.header
            && restored.options.footer
            && restored.options.repeat
    );
}

#[test]
fn read_alarm_uses_the_local_day_before_resolving_the_requested_minute() {
    const UTC_PLUS_SEVEN: i32 = 7 * 60 * 60;
    let resolve_after = |civil: i64, after| {
        let target = civil - i64::from(UTC_PLUS_SEVEN);
        let minutes = civil.rem_euclid(24 * 60 * 60) as i32 / 60;
        (target > after).then_some((target, minutes / 60, minutes % 60))
    };
    // It is 00:10 on the Kindle, even though the UTC date has not rolled over.
    let utc = 17 * 3600 + 10 * 60;
    assert_eq!(
        clock_occurrence_in_zone(utc, UTC_PLUS_SEVEN, 15, resolve_after)
            .unwrap()
            .delay,
        Duration::from_secs(5 * 60)
    );
    assert_eq!(
        clock_occurrence_in_zone(utc, UTC_PLUS_SEVEN, 5, resolve_after)
            .unwrap()
            .delay,
        Duration::from_secs(23 * 3600 + 55 * 60)
    );
}

#[test]
fn read_alarm_confirmation_displays_the_time_resolved_after_a_skipped_hour() {
    let now = 3600 + 45 * 60 + 13;
    let minutes = 2 * 60 + 30;
    let alarm = clock_occurrence_in_zone(now, 0, minutes, |civil, after| {
        assert_eq!(civil, 2 * 3600 + 30 * 60);
        assert_eq!(after, now);
        // The clock advances from 02:00 to 03:00. The missing 02:30 resolves
        // to 03:30, at the instant when the pre-transition clock read 02:30.
        Some((civil, 3, 30))
    })
    .unwrap();

    assert_eq!(alarm.delay, Duration::from_secs(44 * 60 + 47));
    assert_eq!(
        alarm.confirmation(),
        "Alarm set for 03:30.\n44 min 47 sec from now."
    );
}

#[test]
fn read_alarm_confirmation_reports_seconds_with_or_without_minutes() {
    let confirmation = |seconds| {
        AlarmOccurrence {
            delay: Duration::from_secs(seconds),
            hour: 8,
            minute: 5,
        }
        .confirmation()
    };
    assert_eq!(confirmation(18), "Alarm set for 08:05.\n18 sec from now.");
    assert_eq!(
        confirmation(3600 + 4 * 60),
        "Alarm set for 08:05.\n1 hr 4 min from now."
    );
}

#[test]
fn read_alarm_confirmation_leaves_the_reading_page_available_and_dismisses() {
    let fixture = Fixture::new();
    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
    app.open_path(&fixture.book).unwrap();
    let original = frame(&app, &fonts);
    let position = app.current_history_location().unwrap();
    open_tools(&mut app);

    let now = Instant::now();
    let wall = SystemTime::now();
    app.read_alarm.handle(touch(PICK_ALARM), now, wall).unwrap();
    let Some(Panel::Set(Kind::Alarm, minutes)) = app.read_alarm.panel else {
        panic!("Expected the local alarm picker");
    };
    let minutes = (minutes + 5) % (24 * 60);
    app.read_alarm.panel = Some(Panel::Set(Kind::Alarm, minutes));
    let expected = clock_occurrence(wall, minutes).unwrap();

    assert_eq!(
        app.read_alarm.handle(touch(START), now, wall).unwrap(),
        Change::Partial
    );
    assert!(!app.read_alarm.modal());
    assert_eq!(
        app.read_alarm.alert.as_ref().unwrap().text,
        expected.confirmation()
    );
    assert_ne!(frame(&app, &fonts), original);
    capture_test_screen(&app, &fonts, "READER_ALARM_CONFIRMATION_SCREEN", false);

    assert_eq!(
        app.poll_read_alarm(now + Duration::from_secs(5), false),
        Change::Partial
    );
    assert_eq!(app.read_alarm.running.unwrap().due, now + expected.delay);
    assert_eq!(app.current_history_location().unwrap(), position);
    assert_eq!(frame(&app, &fonts), original);
}

#[cfg(all(target_os = "linux", target_arch = "arm", target_env = "musl"))]
#[test]
fn read_alarm_picker_matches_the_kindle_system_clock() {
    let output = std::process::Command::new("date")
        .arg("+%s %H %M")
        .env_remove("TZ")
        .output()
        .unwrap();
    assert!(output.status.success());
    let stamp = String::from_utf8(output.stdout).unwrap();
    let values: Vec<u64> = stamp
        .split_whitespace()
        .map(|value| value.parse().unwrap())
        .collect();
    let [utc, hour, minute] = values.as_slice() else {
        panic!("Expected epoch, hour, and minute from the Kindle clock");
    };
    let fixture = Fixture::new();
    let mut timer = ReadAlarm::open(fixture.root.join("timer.json"));
    timer.panel = Some(Panel::Settings);
    timer
        .handle(
            touch(PICK_ALARM),
            Instant::now(),
            UNIX_EPOCH + Duration::from_secs(*utc),
        )
        .unwrap();
    assert!(matches!(
        timer.panel,
        Some(Panel::Set(Kind::Alarm, minutes)) if u64::from(minutes) == hour * 60 + minute
    ));
}

#[test]
#[ignore = "requires a nominated original from Documents"]
fn read_alarm_original_current_page_and_pixels_survive_expiry() {
    let source = PathBuf::from(std::env::var_os("READER_ALARM_ORIGINAL").unwrap());
    let documents = source.parent().unwrap();
    let before = source.metadata().unwrap().len();
    let fixture = Fixture::new();
    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut app = Reader::open(documents, &fixture.state, fonts.clone()).unwrap();
    app.open_path(&source).unwrap();
    let loc = app.current_history_location().unwrap();
    let original = frame(&app, &fonts);
    open_tools(&mut app);
    app.input(touch(PICK_INTERVAL));
    app.input(touch(MIN_UP));
    app.input(touch(START));
    let due = app.read_alarm.running.unwrap().due;
    let page = app
        .fixed
        .as_ref()
        .map(|f| f.reading.page_status())
        .or_else(|| app.active.as_ref().map(|a| a.reading.page_status()))
        .unwrap();
    app.read_alarm.options.footer = true;
    assert!(
        app.status_caption()
            .is_some_and(|caption| caption.contains("Timer 00:01")
                && (caption.contains(&page) || caption.contains("page ")))
    );
    app.read_alarm.options.footer = false;
    app.input(Input::Sleep);
    assert!(!app.stats_awake);
    assert_eq!(
        app.tick(due),
        Change::None,
        "no wake alarm or redraw should be requested while asleep"
    );
    app.input(Input::Wake);
    assert!(app.stats_awake);
    app.read_alarm.options.message = "%T · %C · %p%".into();
    assert_eq!(app.poll_read_alarm(due, true), Change::Partial);
    let text = &app.read_alarm.alert.as_ref().unwrap().text;
    assert!(!text.contains("%T") && !text.contains("%C") && !text.contains("%p"));
    assert_eq!(app.input(touch(DISMISS)), Change::Partial);
    assert_eq!(app.current_history_location().unwrap(), loc);
    assert_eq!(frame(&app, &fonts), original);
    assert_eq!(source.metadata().unwrap().len(), before);
    println!(
        "read-timer-original source_restored=1 pixels_equal={} state_preserved=1",
        original.len()
    );
}

#[test]
fn read_alarm_message_keyboard_previews_multiline_book_tokens_and_long_press_stops() {
    let fixture = Fixture::new();
    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
    app.open_path(&fixture.book).unwrap();
    open_tools(&mut app);
    app.input(touch(MESSAGE));
    let keys = Layout::new(WIDTH, HEIGHT, 4, 5);
    let title = app.active.as_ref().unwrap().reading.probe.title.clone();
    app.read_alarm.editor = Multiline::new("%T", MESSAGE_LIMIT);
    app.read_alarm.editor.editor.caret = 2;
    let go = keys
        .keyboard_keys_with(app.read_alarm.editor.editor.layer, "New line")
        .into_iter()
        .find(|(key, _)| key.key == primitive_ui::Key::Go)
        .unwrap();
    app.input(touch(go.1));
    app.read_alarm
        .editor
        .editor
        .press(primitive_ui::Key::Char('%'));
    app.read_alarm
        .editor
        .editor
        .press(primitive_ui::Key::Char('C'));
    app.input(touch(PREVIEW_MESSAGE));
    let preview = &app.read_alarm.alert.as_ref().unwrap().text;
    assert!(preview.starts_with(&format!("{title}\n")) && !preview.contains("%C"));
    assert_eq!(app.input(touch(DISMISS)), Change::Partial);
    assert!(matches!(app.read_alarm.panel, Some(Panel::EditMessage)));
    app.input(touch(SAVE_MESSAGE));
    assert_eq!(app.read_alarm.options.message, "%T\n%C");
    app.input(touch(PICK_INTERVAL));
    app.input(touch(MIN_UP));
    app.input(touch(START));
    app.open_reading_edge(true, 630.);
    if app.top_group != Some(TopGroup::Tools) {
        let bar = app.reading_button("Tools");
        app.input(touch(bar));
    }
    let row = app.reading_button("Read timer");
    let Input::Tap(x, y) = touch(row) else {
        unreachable!()
    };
    assert_eq!(app.input(Input::LongTap(x, y)), Change::Partial);
    assert!(app.read_alarm.running.is_none() && app.top_group == Some(TopGroup::Tools));
}
