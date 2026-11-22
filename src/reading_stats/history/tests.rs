use super::*;
fn temp(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "reader-history-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}
fn stamp(value: &str) -> i64 {
    crate::mark_time::parse(value.as_bytes()).unwrap()
}
fn fixture(name: &str) -> (History, Instant, i64, PathBuf) {
    let dir = temp(name);
    fs::create_dir_all(&dir).unwrap();
    let history = History::load(dir.join("history.json"), BTreeMap::from([(99, 1234)]));
    (history, Instant::now(), stamp("2026-01-31T23:59:30Z"), dir)
}
#[test]
fn filters_rereads_checkpoints_and_undated_legacy_totals_are_distinct() {
    let (mut h, t, w, dir) = fixture("filters");
    let at = |s| t + Duration::from_secs(s);
    h.set_page_at(Some((1, 10)), at(0), Some(w));
    h.set_page_at(Some((1, 11)), at(4), Some(w + 4)); // skim
    h.set_page_at(Some((1, 12)), at(9), Some(w + 9)); // exactly minimum
    assert_eq!(h.summary(None, None).seconds, 5);
    h.save(at(69), Some(w + 69), false).unwrap();
    h.save(at(129), Some(w + 129), false).unwrap();
    h.set_page_at(Some((1, 11)), at(309), Some(w + 309)); // exactly120, not300
    h.set_page_at(None, at(339), Some(w + 339)); // reread30
    assert_eq!(
        h.summary(None, None),
        Summary {
            seconds: 155,
            screens: 2,
            capped_seconds: 155,
            books: 1
        }
    );
    h.set_page_at(Some((1, 12)), at(340), Some(w + 340));
    h.set_page_at(None, at(460), Some(w + 460));
    assert_eq!(h.summary(None, None).seconds, 275);
    assert_eq!(h.summary(None, None).capped_seconds, 155);
    assert_eq!(h.prior(None), 1234);
    assert_eq!(
        h.days(None, Zone::Fixed(0))
            .values()
            .map(|s| s.seconds)
            .sum::<u64>(),
        275
    );
    h.save(at(460), Some(w + 460), true).unwrap();
    let restored = History::load(dir.join("history.json"), BTreeMap::from([(99, 9999)]));
    assert_eq!(restored.prior(None), 1234);
    assert_eq!(restored.summary(None, None), h.summary(None, None));
    fs::remove_dir_all(dir).unwrap();
}
#[test]
fn daily_start_totals_and_midnight_clipped_calendar_match_separate_reference_queries() {
    let (mut h, t, w, dir) = fixture("midnight");
    h.set_page_at(Some((1, 1)), t, Some(w));
    h.set_page_at(None, t + Duration::from_secs(90), Some(w + 90));
    let jan = Date::new(2026, 1, 31).unwrap();
    let feb = jan.add_days(1).unwrap();
    assert_eq!(h.days(None, Zone::Fixed(0))[&jan].seconds, 90);
    assert!(!h.days(None, Zone::Fixed(0)).contains_key(&feb));
    assert_eq!(h.day_seconds(jan, None, Zone::Fixed(0)), 30);
    assert_eq!(h.day_seconds(feb, None, Zone::Fixed(0)), 60);
    assert_eq!(h.hourly(jan, None, Zone::Fixed(0))[23], 30);
    assert_eq!(h.hourly(feb, None, Zone::Fixed(0))[0], 60);
    // A UTC+7 display groups the same unchanged instants into February.
    assert_eq!(h.days(None, Zone::Fixed(7 * 3600))[&feb].seconds, 90);
    // A04:00 calendar origin assigns the after-midnight minute to January31.
    h.data.options.day_start = 240;
    assert_eq!(h.day_seconds(jan, None, Zone::Fixed(0)), 90);
    assert_eq!(h.day_seconds(feb, None, Zone::Fixed(0)), 0);
    fs::remove_dir_all(dir).unwrap();
}
#[test]
fn sleep_and_clock_corrections_cannot_create_paused_or_invented_dated_time() {
    let (mut h, t, w, dir) = fixture("clock");
    let at = |s| t + Duration::from_secs(s);
    h.set_page_at(Some((1, 1)), at(0), Some(w));
    h.set_page_at(None, at(20), Some(w + 20));
    h.set_page_at(Some((1, 1)), at(3620), Some(w + 3620));
    h.set_page_at(None, at(3640), Some(w + 3640));
    assert_eq!(h.summary(None, None).seconds, 40);
    h.set_page_at(Some((2, 1)), at(4000), Some(w + 4000));
    h.save(at(4060), Some(w + 4060), false).unwrap();
    // Backward wall correction while monotonic reading continues leaves only this remainder undated.
    h.set_page_at(None, at(4080), Some(w + 10));
    assert_eq!(
        h.visits()
            .iter()
            .filter(|v| v.book == 2 && v.start.is_none())
            .map(|v| v.seconds)
            .sum::<u32>(),
        20
    );
    assert_eq!(
        h.visits()
            .iter()
            .filter(|v| v.book == 2 && v.start.is_some())
            .map(|v| v.seconds)
            .sum::<u32>(),
        60
    );
    h.set_page_at(Some((3, 1)), at(4100), Some(10));
    h.set_page_at(None, at(4110), Some(20));
    assert!(
        h.visits()
            .iter()
            .filter(|v| v.book == 3)
            .all(|v| v.start.is_none())
    );
    h.set_page_at(Some((4, 1)), at(4200), Some(w + 4200));
    h.set_page_at(None, at(4199), Some(w + 4199));
    assert_eq!(h.summary(Some(4), None).seconds, 0);
    fs::remove_dir_all(dir).unwrap();
}
#[test]
fn failed_atomic_save_usb_discard_and_future_format_preserve_existing_data() {
    let (mut h, t, w, dir) = fixture("storage");
    h.save(t, Some(w), true).unwrap();
    let original = fs::read(&h.path).unwrap();
    h.set_page_at(Some((1, 1)), t, Some(w));
    let path = h.path.clone();
    h.path = dir.join("missing/history.json");
    assert!(
        h.save(t + Duration::from_secs(60), Some(w + 60), false)
            .is_err()
    );
    assert!(h.visits().is_empty());
    assert_eq!(fs::read(&path).unwrap(), original);
    h.discard_active();
    h.path = path;
    h.set_page_at(Some((1, 1)), t + Duration::from_secs(600), Some(w + 600));
    h.set_page_at(None, t + Duration::from_secs(610), Some(w + 610));
    h.save(t + Duration::from_secs(610), Some(w + 610), true)
        .unwrap();
    assert_eq!(h.summary(None, None).seconds, 10);
    let future = br#"{"version":9,"prior":{},"titles":{},"visits":[]}"#;
    fs::write(&h.path, future).unwrap();
    let mut restored = History::load(h.path.clone(), BTreeMap::new());
    assert!(restored.warning().is_some());
    restored.set_page_at(Some((1, 1)), t, Some(w));
    assert!(
        !restored
            .save(t + Duration::from_secs(90), Some(w + 90), true)
            .unwrap()
    );
    assert_eq!(fs::read(&h.path).unwrap(), future);
    fs::remove_dir_all(dir).unwrap();
}
#[test]
fn calendar_arithmetic_handles_leap_years_centuries_and_week_starts() {
    let leap = Date::new(2024, 2, 28).unwrap();
    assert_eq!(leap.add_days(1), Date::new(2024, 2, 29));
    assert_eq!(leap.add_days(2), Date::new(2024, 3, 1));
    assert!(Date::new(2100, 2, 29).is_none());
    assert!(Date::new(2000, 2, 29).is_some());
    assert_eq!(
        Date::new(2026, 12, 31).unwrap().add_days(1),
        Date::new(2027, 1, 1)
    );
    assert_eq!(
        Date::new(2026, 1, 31).unwrap().add_months(-1),
        Date::new(2025, 12, 1)
    );
    assert_eq!(Date::new(1970, 1, 1).unwrap().weekday(), 4);
    for year in [1901, 1999, 2000, 2024, 2100, 9998] {
        for month in 1..=12 {
            let day = Date::new(year, month, 1).unwrap();
            assert_eq!(day.add_days(-1).unwrap().add_days(1), Some(day));
        }
    }
}
#[test]
fn changing_filters_pauses_the_old_visit_and_disabled_statistics_stop_recording() {
    let (mut h, t, w, dir) = fixture("options");
    h.set_page_at(Some((1, 1)), t, Some(w));
    h.configure(
        Options {
            enabled: false,
            ..h.options()
        },
        t + Duration::from_secs(10),
        Some(w + 10),
    )
    .unwrap();
    h.set_page_at(Some((2, 1)), t + Duration::from_secs(20), Some(w + 20));
    assert!(h.deadline().is_none());
    assert_eq!(h.summary(None, None).seconds, 10);
    h.configure(
        Options {
            enabled: true,
            min_seconds: 10,
            max_seconds: 60,
            ..h.options()
        },
        t + Duration::from_secs(30),
        Some(w + 30),
    )
    .unwrap();
    h.set_page_at(Some((2, 1)), t + Duration::from_secs(30), Some(w + 30));
    h.set_page_at(None, t + Duration::from_secs(35), Some(w + 35));
    assert_eq!(h.summary(None, None).seconds, 10);
    fs::remove_dir_all(dir).unwrap();
}
#[test]
#[ignore = "Run isolated with TZ=America/New_York and READER_STATS_ZONE_TEST=1"]
fn local_midnight_boundaries_respect_dst_day_lengths() {
    assert_eq!(std::env::var("READER_STATS_ZONE_TEST").as_deref(), Ok("1"));
    let (a, b) = Zone::Local
        .range(Date::new(2026, 3, 8).unwrap(), 0)
        .unwrap();
    assert_eq!(b - a, 23 * 3600);
    let (a, b) = Zone::Local
        .range(Date::new(2026, 11, 1).unwrap(), 0)
        .unwrap();
    assert_eq!(b - a, 25 * 3600);
}

#[test]
fn short_intervals_across_sleep_combine_without_charging_the_gap_or_resetting_the_cap() {
    let (mut h, t, w, dir) = fixture("paused-min");
    let at = |s| t + Duration::from_secs(s);
    h.set_page_at(Some((1, 1)), at(0), Some(w));
    h.set_page_at(None, at(2), Some(w + 2));
    assert!(h.visits().is_empty());
    assert!(h.deadline().is_none());
    h.set_page_at(Some((1, 1)), at(602), Some(w + 602));
    h.set_page_at(None, at(605), Some(w + 605));
    assert_eq!(h.summary(None, None).seconds, 5);
    assert_eq!(h.visits().len(), 2);
    assert_eq!(h.visits()[0].start, Some(w));
    assert_eq!(h.visits()[1].start, Some(w + 602));
    h.set_page_at(Some((1, 1)), at(1000), Some(w + 1000));
    h.set_page_at(None, at(1300), Some(w + 1300));
    assert_eq!(h.summary(None, None).seconds, 120);
    h.finish(at(1400));
    h.set_page_at(Some((1, 1)), at(1400), Some(w + 1400));
    h.set_page_at(None, at(1410), Some(w + 1410));
    assert_eq!(h.summary(None, None).seconds, 130);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn frequent_notifications_keep_the_original_start_day_and_a_fixed_save_deadline() {
    let (mut h, t, w, dir) = fixture("frequent");
    h.set_page_at(Some((1, 1)), t, Some(w));
    let deadline = h.deadline();
    for millis in [100, 900, 1100, 1900, 2100, 4900, 5100] {
        h.set_page_at(
            Some((1, 1)),
            t + Duration::from_millis(millis),
            Some(w + (millis / 1000) as i64),
        );
        assert_eq!(h.deadline(), deadline);
    }
    h.set_page_at(None, t + Duration::from_secs(6), Some(w + 6));
    assert_eq!(h.visits()[0].start, Some(w));
    assert_eq!(h.summary(None, None).seconds, 6);
    let jan = Date::new(2026, 1, 31).unwrap();
    let months = h.calendar_month(jan.first(), None, Zone::Fixed(0));
    assert_eq!(months[&jan].0, h.day_seconds(jan, None, Zone::Fixed(0)));
    fs::remove_dir_all(dir).unwrap();
}
