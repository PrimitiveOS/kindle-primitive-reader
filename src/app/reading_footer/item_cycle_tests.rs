use super::*;
use crate::app::tests::{Fixture, capture_test_screen};
use crate::session::SHOW_STATUS;
use primitive_app::App;
use primitive_ui::Input;

fn fonts() -> Vec<std::path::PathBuf> {
    vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")]
}

fn tap_status(app: &mut Reader) -> Change {
    app.input(Input::Tap(150.0, HEIGHT as f32 - 40.0))
}

#[test]
fn footer_item_cycle_uses_enabled_order_and_keeps_the_reading_location() {
    let fixture = Fixture::new();
    let fonts = fonts();
    let book = fixture.root.join("Harbor Notebook.epub");
    std::fs::copy(crate::test_fixture("many-chapters.epub"), &book).unwrap();
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
    app.open_path(&book).unwrap();
    let default_caption = app.status_caption().unwrap();
    assert!(default_caption.starts_with("Chapter "));
    assert!(default_caption.contains(" - Page "));
    let default_track = app.footer_track_rect(0);
    let original = app.current_history_location();
    let selected = FooterItem::BookTitle.bit()
        | FooterItem::BookProgress.bit()
        | FooterItem::BookmarkCount.bit();
    let order = u64::from(FooterItem::BookTitle as u8)
        | (u64::from(FooterItem::BookmarkCount as u8) << 4)
        | (u64::from(FooterItem::BookProgress as u8) << 8);
    app.activate(Action::SetReadingControls(ReadingControls {
        status_bar: StatusBar::Custom,
        footer_items: selected,
        footer_order: crate::reading_controls::FooterOrder::Legacy(order),
        ..app.controls
    }))
    .unwrap();
    let combined = app.status_caption().unwrap();
    assert!(combined.contains("0 bookmarks"));
    assert!(combined.contains('%'));
    app.show(Screen::FooterSettings);
    let cycle = app
        .footer_items()
        .into_iter()
        .find(|item| item.title == "Cycle custom items")
        .unwrap();
    app.activate(cycle.action).unwrap();
    capture_test_screen(&app, &fonts, "READER_CYCLE_FOOTER_SETTINGS_FRAME", false);
    app.show(Screen::Book);
    let state = || {
        Some(FooterItemCycle {
            selected: Some(FooterItem::BookTitle),
        })
    };
    assert_eq!(app.controls.footer_item_cycle, state());
    assert!(!app.status_caption().unwrap().contains("0 bookmarks"));
    capture_test_screen(&app, &fonts, "READER_CYCLE_FOOTER_BOOK_FRAME", false);

    assert_eq!(tap_status(&mut app), Change::Partial);
    assert_eq!(
        app.controls.footer_item_cycle.unwrap().selected,
        Some(FooterItem::BookmarkCount)
    );
    assert!(app.status_caption().unwrap().contains("0 bookmarks"));
    assert_eq!(tap_status(&mut app), Change::Partial);
    assert_eq!(
        app.controls.footer_item_cycle.unwrap().selected,
        Some(FooterItem::BookProgress)
    );
    assert!(app.status_caption().unwrap().contains('%'));
    assert_eq!(tap_status(&mut app), Change::Partial);
    assert_eq!(app.controls.footer_item_cycle.unwrap().selected, None);
    assert_eq!(app.status_caption().unwrap(), "");
    assert_eq!(app.footer_track_rect(0), None);
    assert_eq!(app.current_history_location(), original);
    assert_eq!(tap_status(&mut app), Change::Partial);
    assert_eq!(app.controls.footer_item_cycle, state());

    app.activate(Action::SetReadingControls(ReadingControls {
        footer_tap_locked: true,
        ..app.controls
    }))
    .unwrap();
    assert_eq!(tap_status(&mut app), Change::None);
    assert_eq!(app.controls.footer_item_cycle, state());
    let preset = super::super::footer_presets::Preset::capture(
        "Cycle".into(),
        app.controls,
        None,
        Default::default(),
    );
    assert_eq!(
        preset.apply(ReadingControls::default()).0.footer_item_cycle,
        state()
    );
    app.save().unwrap();
    let mut reopened = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
    reopened.open_path(&book).unwrap();
    assert_eq!(reopened.controls.footer_item_cycle, state());
    assert_eq!(reopened.current_history_location(), original);
    reopened
        .activate(Action::SetReadingControls(ReadingControls {
            status_bar: StatusBar::Chapter,
            footer_item_cycle: None,
            ..reopened.controls
        }))
        .unwrap();
    assert_eq!(reopened.status_caption(), Some(default_caption));
    assert_eq!(reopened.footer_track_rect(0), default_track);
}

#[test]
fn footer_item_cycle_pauses_clock_and_restores_an_overlapped_pdf_footer() {
    let fixture = Fixture::new();
    let pdf = fixture.root.join("Coastal scan.pdf");
    std::fs::write(&pdf, crate::fixed::tests::document()).unwrap();
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts()).unwrap();
    app.open_path(&pdf).unwrap();
    app.activate(Action::SetReadingControls(ReadingControls {
        overlap_footer: true,
        status_bar: StatusBar::Custom,
        footer_items: FooterItem::CurrentPage.bit() | FooterItem::CurrentTime.bit(),
        footer_item_cycle: Some(FooterItemCycle {
            selected: Some(FooterItem::CurrentPage),
        }),
        footer_auto_refresh: true,
        ..app.controls
    }))
    .unwrap();
    assert!(!app.clock_refresh_visible());
    let origin = app.current_history_location();
    assert_eq!(tap_status(&mut app), Change::Partial);
    assert_eq!(
        app.controls.footer_item_cycle.unwrap().selected,
        Some(FooterItem::CurrentTime)
    );
    assert!(app.clock_refresh_visible());
    assert_eq!(tap_status(&mut app), Change::Partial);
    assert!(app.hidden_overlap_footer());
    assert!(!app.clock_refresh_visible());
    assert_eq!(
        app.input(Input::Tap(
            (SHOW_STATUS.x + SHOW_STATUS.w / 2) as f32,
            (SHOW_STATUS.y + SHOW_STATUS.h / 2) as f32
        )),
        Change::Partial
    );
    assert_eq!(
        app.controls.footer_item_cycle.unwrap().selected,
        Some(FooterItem::CurrentPage)
    );
    assert_eq!(app.current_history_location(), origin);
}
