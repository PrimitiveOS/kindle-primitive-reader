use super::*;
use crate::app::{
    App, BarPanel, Change,
    gestures::{Binding, Gesture},
    selection_viewport_tests::{adopt, canvas, key, press, tap},
    tests::{Fixture, capture_test_screen, tap_contents_item},
    top_config::TopGroup,
};
use crate::session::ReadingViewport;
use primitive_app::viewport::{LogicalTurn, Viewport, ViewportDecision};
use primitive_ui::Key;

fn open(app: &mut Reader) {
    app.open_reading_edge(true, 640.0);
    if app.top_group != Some(TopGroup::Settings) {
        tap_toolbar(app, "Settings");
    }
    tap_toolbar(app, "Notifications");
    assert_eq!(app.screen, Screen::ReadingNotifications);
}

fn text_panel(app: &mut Reader, panel: BarPanel, title: &str) {
    app.open_reading_edge(false, 640.0);
    if app.reading_panel != Some(panel) {
        tap_toolbar(app, title);
    }
    assert_eq!(app.reading_panel, Some(panel));
}

fn tap_toolbar(app: &mut Reader, title: &str) {
    let rect = app.reading_button(title);
    tap(app, rect);
}

fn tap_choice(app: &mut Reader, row: &str, title: &str) {
    let rect = app.reading_button_in_row(row, title);
    tap(app, rect);
}

fn fonts() -> Vec<std::path::PathBuf> {
    ["fonts/NotoSans-Regular.ttf", "fonts/NotoSans-Bold.ttf"]
        .map(crate::test_fixture)
        .to_vec()
}

fn visible(app: &Reader) -> Option<&str> {
    app.notifications.visible_until?;
    app.notifications
        .history
        .front()
        .map(|message| message.text.as_str())
}

#[test]
fn reading_notifications_toggles_persist_independently_and_history_is_session_local() {
    let fixture = Fixture::new();
    let mut app = fixture.app();
    app.open_path(&fixture.book).unwrap();
    open(&mut app);
    assert_eq!(Source::mask(app.controls.notification_sources), 0x3c);
    for (index, (source, label)) in Source::ALL.into_iter().enumerate() {
        let before = Source::mask(app.controls.notification_sources);
        tap_contents_item(&mut app, label);
        let after = Source::mask(app.controls.notification_sources);
        assert_eq!(before ^ after, 1 << index);
        assert_eq!(
            source.enabled(app.controls.notification_sources),
            after & source as u8 != 0
        );
    }
    assert_eq!(app.controls.notification_sources, Some(0x43));
    app.notify_reading(Source::Icons, "Test screen");

    let mut reopened = fixture.app();
    reopened.open_path(&fixture.book).unwrap();
    open(&mut reopened);
    assert_eq!(reopened.controls.notification_sources, Some(0x43));
    tap_contents_item(&mut reopened, "Show past notifications");
    assert_eq!(reopened.screen, Screen::PastNotifications);
    assert_eq!(reopened.items()[0].title, "No notifications yet");
    let back = reopened.navigation_layout().back();
    tap(&mut reopened, back);
    assert_eq!(reopened.screen, Screen::ReadingNotifications);

    std::fs::remove_file(&reopened.controls_path).unwrap();
    std::fs::create_dir(&reopened.controls_path).unwrap();
    tap_contents_item(&mut reopened, "Other sources");
    assert!(reopened.notice.is_some());
    assert_eq!(reopened.controls.notification_sources, Some(0x43));
}

#[test]
fn reading_notifications_lower_controls_and_history_keep_the_current_passage() {
    let fixture = Fixture::new();
    let fonts = fonts();
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
    app.open_path(&fixture.book).unwrap();
    let anchor = app.current_history_location().unwrap().position.anchor;
    text_panel(&mut app, BarPanel::TextLayout, "Layout");

    tap_toolbar(&mut app, "125%");
    assert_eq!(visible(&app), Some("Line spacing · 125%"));
    assert_eq!(app.input(Input::Tap(33.0, 200.0)), Change::Partial);
    assert_eq!(visible(&app), None);
    text_panel(&mut app, BarPanel::TextLayout, "Layout");
    let increase = app.reading_button_in_row("Fine · 125%", "+1%");
    tap(&mut app, increase);
    assert_eq!(visible(&app), Some("Line spacing · 126%"));
    let until = app.notifications.deadline().unwrap();
    assert!(until > Instant::now());
    capture_test_screen(&app, &fonts, "READER_NOTIFICATION_TOAST_CAPTURE", false);
    let mut viewport = canvas(&app, ReadingViewport::PORTRAIT);
    let active_control = app.reading_button_in_row("Fine · 126%", "-1%");
    press(&app, &mut viewport, active_control, false);
    let before_expiry = primitive_app::frame(&app, &mut viewport, false);
    let cover = app.reading_notification_cover().unwrap();
    assert_eq!(app.tick(until), Change::Partial);
    assert_eq!(visible(&app), None);
    let after_expiry = primitive_app::frame(&app, &mut viewport, false);
    let toast_bottom = viewport.bounds().w as usize * 140;
    assert_ne!(before_expiry[..toast_bottom], after_expiry[..toast_bottom]);
    assert_eq!(before_expiry[toast_bottom..], after_expiry[toast_bottom..]);
    let width = viewport.bounds().w as usize;
    for (index, (&before, &after)) in before_expiry.iter().zip(&after_expiry).enumerate() {
        if before != after {
            assert!(cover.contains((index % width) as f32, (index / width) as f32));
        }
    }

    tap_toolbar(&mut app, "Set value");
    assert_eq!(app.notifications.history.len(), 2);
    assert_eq!(app.input(Input::PageBack), Change::Partial);
    assert_eq!(app.notifications.history.len(), 2);
    tap_toolbar(&mut app, "Set value");
    app.editor.text = "118".chars().collect();
    app.editor.caret = app.editor.text.len();
    let submit = key(&app, Key::Go, "Set");
    tap(&mut app, submit);
    assert_eq!(visible(&app), Some("Line spacing · 118%"));

    text_panel(&mut app, BarPanel::TextRendering, "Rendering");
    tap_choice(&mut app, "Font kerning", "Off");
    assert_eq!(app.notifications.history.len(), 3);
    open(&mut app);
    capture_test_screen(&app, &fonts, "READER_NOTIFICATION_SETTINGS_CAPTURE", false);
    tap_contents_item(&mut app, "Bottom menu toggles");
    tap_contents_item(&mut app, "Bottom menu ± buttons");
    text_panel(&mut app, BarPanel::TextLayout, "Layout");
    tap_choice(&mut app, "Fine · 118%", "+1%");
    assert_eq!(app.notifications.history.len(), 3);
    text_panel(&mut app, BarPanel::TextRendering, "Rendering");
    tap_choice(&mut app, "Font kerning", "Fast");
    assert_eq!(visible(&app), Some("Font kerning · Fast"));

    open(&mut app);
    tap_contents_item(&mut app, "Show past notifications");
    let labels: Vec<_> = app.items().into_iter().map(|item| item.title).collect();
    assert_eq!(
        labels,
        [
            "Font kerning · Fast",
            "Line spacing · 118%",
            "Line spacing · 126%",
            "Line spacing · 125%",
        ]
    );
    for (index, message) in app.notifications.history.iter_mut().rev().enumerate() {
        message.time = Some(format!("09:41:{:02}", 11 + index * 3));
    }
    capture_test_screen(&app, &fonts, "READER_NOTIFICATION_HISTORY_CAPTURE", false);
    let back = app.navigation_layout().back();
    tap(&mut app, back);
    assert_eq!(app.screen, Screen::ReadingNotifications);
    assert_eq!(
        app.current_history_location().unwrap().position.anchor,
        anchor
    );
}

#[test]
fn reading_notifications_pdf_contrast_and_fit_are_routed_to_their_own_switches() {
    let fixture = Fixture::new();
    let pdf = fixture.root.join("Tilted scan.pdf");
    std::fs::copy(crate::test_fixture("tilted-scan.pdf"), &pdf).unwrap();
    let mut app = fixture.app();
    app.open_path(&pdf).unwrap();
    let original = app.current_history_location().unwrap().position;
    text_panel(&mut app, BarPanel::View, "View");
    tap_choice(&mut app, "Fit", "Height");
    assert!(app.notifications.history.is_empty());

    text_panel(&mut app, BarPanel::Scan, "Scan");
    tap_choice(&mut app, "Contrast", "2.0");
    assert_eq!(visible(&app), Some("Contrast · 2.0"));
    tap_choice(&mut app, "Fine · 2.0", "+0.1");
    assert_eq!(visible(&app), Some("Contrast · 2.1"));

    open(&mut app);
    tap_contents_item(&mut app, "Bottom menu toggles");
    tap_contents_item(&mut app, "Bottom menu progress bars");
    text_panel(&mut app, BarPanel::Scan, "Scan");
    tap_choice(&mut app, "Contrast", "1.5");
    assert_eq!(app.notifications.history.len(), 2);
    assert_eq!(
        app.fixed
            .as_ref()
            .unwrap()
            .reading
            .preferences()
            .scan_gamma_tenths,
        Some(15)
    );
    text_panel(&mut app, BarPanel::View, "View");
    // Fit changes are silent, as on the earlier Height choice: the fit axis is shown by the bar itself.
    let before = app.notifications.history.len();
    tap_choice(&mut app, "Fit", "Width");
    assert_eq!(app.notifications.history.len(), before);
    assert_eq!(app.current_history_location().unwrap().position, original);
}

#[test]
fn reading_notifications_orientation_gesture_profile_and_manual_jump_obey_distinct_sources() {
    let fixture = Fixture::new();
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts()).unwrap();
    app.open_path(&fixture.book).unwrap();
    open(&mut app);
    tap_contents_item(&mut app, "Bottom menu icons");
    text_panel(&mut app, BarPanel::TextScreen, "Screen");
    let rotate = app.reading_button_in_row("Screen orientation", "Upside down");
    assert_eq!(
        app.input(Input::Tap(
            (rotate.x + rotate.w / 2) as f32,
            (rotate.y + rotate.h / 2) as f32
        )),
        Change::Full
    );
    assert_eq!(visible(&app), Some("Screen orientation · Upside down"));
    let pending = app.notifications.visible_until.unwrap();
    let mut viewport = adopt(&mut app, ReadingViewport::LANDSCAPE, 5);
    assert_eq!(viewport.bounds().w, 1680);
    assert!(app.notifications.tick(pending));

    let initial = app.current_history_location();
    app.follow_reflow_page(1, 0).unwrap();
    app.gestures
        .set(Gesture::TapTopLeft, Binding::Back)
        .unwrap();
    assert_eq!(app.input(Input::Tap(32.0, 32.0)), Change::Partial);
    assert_eq!(visible(&app), Some("Returned to previous location"));
    assert_eq!(app.current_history_location(), initial);

    app.save_reading_profile("Fieldwork").unwrap();
    app.show(Screen::ReadingProfile(0));
    tap_contents_item(&mut app, "Apply to this book");
    assert_eq!(visible(&app), Some("Applied profile · Fieldwork"));
    assert_eq!(app.current_history_location(), initial);

    open(&mut app);
    tap_contents_item(&mut app, "Gestures and profiles");
    tap_contents_item(&mut app, "Other sources");
    app.follow_reflow_page(1, 0).unwrap();
    assert_eq!(app.input(Input::Tap(32.0, 32.0)), Change::Partial);
    assert_eq!(app.notifications.history.len(), 3);
    app.follow_reflow_page(1, 0).unwrap();
    app.open_reading_edge(true, 24.0);
    tap_toolbar(&mut app, "Previous location");
    assert_eq!(visible(&app), Some("Returned to previous location"));
    assert_eq!(app.current_history_location(), initial);
    assert_eq!(
        primitive_app::frame(&app, &mut viewport, true).len(),
        1680 * 1264
    );
    app.input(Input::PageForward);
    assert_eq!(visible(&app), None);
    assert_eq!(app.notifications.history.len(), 4);
    // The same physical key may be queued by the e-ink refresh pacing.
    app.notifications.tick(Instant::now());
    if app.current_history_location() == initial {
        let due = app.active.as_ref().unwrap().reading.deadline().unwrap();
        app.tick(due);
    }
    assert_ne!(app.current_history_location(), initial);
}

#[test]
fn reading_notifications_wait_for_the_viewport_to_accept_an_orientation_change() {
    let fixture = Fixture::new();
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts()).unwrap();
    app.open_path(&fixture.book).unwrap();
    open(&mut app);
    tap_contents_item(&mut app, "Bottom menu icons");
    adopt(&mut app, ReadingViewport::PORTRAIT, 1);
    text_panel(&mut app, BarPanel::TextScreen, "Screen");

    tap_choice(&mut app, "Landscape", "Left");
    assert!(app.has_pending_orientation());
    assert!(app.notifications.history.is_empty());
    let rejected = Viewport {
        generation: 2,
        turn: LogicalTurn::Counterclockwise,
        logical_size: ReadingViewport::LANDSCAPE.dimensions(),
    };
    app.stage_viewport(rejected).unwrap();
    app.finish_viewport(rejected, ViewportDecision::Abort);
    assert!(app.notifications.history.is_empty());
    assert!(app.notice.as_deref().unwrap().contains("previous setting"));
    assert_eq!(app.reading_viewport(), ReadingViewport::PORTRAIT);

    tap_choice(&mut app, "Landscape", "Right");
    assert!(app.has_pending_orientation());
    assert!(app.notifications.history.is_empty());
    adopt(&mut app, ReadingViewport::LANDSCAPE, 3);
    assert_eq!(app.notifications.history.len(), 1);
    assert_eq!(visible(&app), Some("Screen orientation · Landscape right"));
    let repaint = app.notifications.deadline().unwrap();
    assert!(repaint < app.notifications.visible_until.unwrap());
    assert_eq!(app.tick(repaint), Change::Partial);
    assert!(visible(&app).is_some());
    assert_eq!(
        app.tick(app.notifications.visible_until.unwrap()),
        Change::Partial
    );
    assert_eq!(visible(&app), None);
}

#[test]
fn reading_notifications_history_keeps_only_twenty_and_dismissal_does_not_drop_entries() {
    let mut record = Notifications::default();
    let now = Instant::now();
    for i in 0..25 {
        record.show(i.to_string(), now);
    }
    assert_eq!(record.history.len(), 20);
    assert_eq!(record.history.front().unwrap().text, "24");
    assert_eq!(record.history.back().unwrap().text, "5");
    assert!(!record.tick(now + Duration::from_millis(1999)));
    record.handle_input(Input::HoldStart(12., 80.));
    assert!(record.visible_until.is_none());
    assert!(record.tick(record.deadline().unwrap()));
    record.show("After the hold".into(), now);
    record.handle_input(Input::Sleep);
    assert!(record.deadline().is_none());
    assert_eq!(record.history.front().unwrap().text, "After the hold");
}
