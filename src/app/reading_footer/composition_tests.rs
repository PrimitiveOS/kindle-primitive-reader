use super::*;
use crate::app::selection_viewport_tests::{adopt, capture};
use crate::app::tests::{Fixture, write_outline_fixture};
use crate::epub::Destination;
use crate::reading_controls::FooterOrder;
use primitive_app::App;
use primitive_ui::Input;

fn open(fixture: &Fixture, path: &std::path::Path) -> Reader {
    let fonts = vec![
        crate::test_fixture("fonts/NotoSans-Regular.ttf"),
        crate::test_fixture("fonts/NotoSans-Bold.ttf"),
    ];
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
    app.open_path(path).unwrap();
    app
}

fn order(items: &[FooterItem]) -> FooterOrder {
    FooterOrder::Legacy(items.iter().enumerate().fold(0, |order, (slot, item)| {
        order | (u64::from(*item as u8) << (slot * 4))
    }))
}

#[test]
fn footer_hides_inactive_metrics_only_when_combined_and_restores_saved_settings() {
    use crate::reading_controls::{FooterSeparator, TopHeader};

    let fixture = Fixture::new();
    let mut app = open(&fixture, &fixture.book);
    let mut canvas = adopt(&mut app, ReadingViewport::PORTRAIT, 1);
    let initial = app.current_history_location();
    let original_frame = primitive_app::frame(&app, &mut canvas, false);
    app.activate(Action::SetDisplayLevel(DisplayLevel::FrontLight, 0))
        .unwrap();
    app.activate(Action::SetReadingControls(ReadingControls {
        status_bar: StatusBar::Custom,
        footer_items: FooterItem::BookmarkCount.bit()
            | FooterItem::BrightnessLevel.bit()
            | FooterItem::WarmthLevel.bit()
            | FooterItem::PageTurningInverted.bit(),
        footer_symbol_style: FooterSymbolStyle::Letters,
        footer_separator: FooterSeparator::Bar,
        ..app.controls
    }))
    .unwrap();
    let setting = |app: &Reader, title| {
        app.footer_items()
            .into_iter()
            .find(|item| item.title == title)
            .unwrap()
            .action
    };
    assert_eq!(
        app.status_caption().as_deref(),
        Some("BM: 0 bookmarks | Pg: Off | L: Off | LW: Off")
    );
    capture(&app, &mut canvas, "status-inactive-shown.png", false);
    app.show(Screen::FooterSettings);
    app.activate(setting(&app, "Hide inactive items")).unwrap();
    app.activate(setting(&app, "12-hour clock")).unwrap();
    app.show(Screen::Book);
    assert_eq!(app.status_caption().as_deref(), Some(""));
    assert!(!app.custom_footer_icons_enabled());
    capture(&app, &mut canvas, "status-inactive-hidden.png", false);

    app.activate(Action::SetDisplayLevel(DisplayLevel::FrontLight, 7))
        .unwrap();
    app.activate(Action::SetDisplayLevel(DisplayLevel::Warmth, 12))
        .unwrap();
    app.activate(Action::ToggleBookmark).unwrap();
    assert_eq!(
        app.status_caption().as_deref(),
        Some("BM: 1 bookmarks | L: 7 | LW: 50%")
    );
    let id = app.open_book_id().unwrap();
    let location = app.active.as_ref().unwrap().reading.anchor().unwrap();
    let mut end = location.clone();
    end.offset += 1;
    app.library
        .state_mut(id)
        .unwrap()
        .highlights
        .push(crate::library::Highlight {
            updated_at: None,
            chapter_title: None,
            created_at: None,
            style: Default::default(),
            color: Default::default(),
            selection: crate::selection::TextSelection {
                start: location,
                end,
                text: "A".into(),
            },
            note: Some("A saved note".into()),
        });
    assert_eq!(
        app.status_caption().as_deref(),
        Some("BM: 2 bookmarks | L: 7 | LW: 50%")
    );
    app.activate(setting(&app, "Move up: Warmth level"))
        .unwrap();
    assert_eq!(
        app.status_caption().as_deref(),
        Some("BM: 2 bookmarks | LW: 50% | L: 7")
    );
    app.notice = None;
    capture(&app, &mut canvas, "status-active-readouts.png", false);
    app.activate(Action::SetReadingControls(ReadingControls {
        footer_symbol_style: FooterSymbolStyle::Icons,
        ..app.controls
    }))
    .unwrap();
    assert!(app.custom_footer_icons_enabled());
    capture(&app, &mut canvas, "status-active-icons.png", false);
    app.activate(Action::SetReadingControls(ReadingControls {
        footer_symbol_style: FooterSymbolStyle::Letters,
        ..app.controls
    }))
    .unwrap();

    app.activate(Action::SetDisplayLevel(DisplayLevel::FrontLight, 0))
        .unwrap();
    app.activate(setting(&app, "Cycle custom items")).unwrap();
    assert!(
        app.footer_items()
            .iter()
            .all(|row| row.title != "Hide inactive items")
    );
    let (_, height) = app.reading_viewport().dimensions();
    assert_eq!(
        app.input(Input::Tap(130.0, (height - 32) as f32)),
        Change::Partial
    );
    assert_eq!(app.status_caption().as_deref(), Some("Pg: Off"));
    assert_eq!(
        app.input(Input::Tap(130.0, (height - 32) as f32)),
        Change::Partial
    );
    assert_eq!(app.status_caption().as_deref(), Some("LW: Off"));
    assert_eq!(app.current_history_location(), initial);

    let saved = ReadingControls::load(&app.controls_path, true);
    assert!(saved.footer_hide_inactive && saved.twelve_hour_clock);
    assert_eq!(saved.footer_order, app.controls.footer_order);
    let preset = super::super::footer_presets::Preset::capture(
        "Quiet status".into(),
        saved,
        None,
        Default::default(),
    );
    let restored = preset.apply(ReadingControls::default()).0;
    assert!(restored.footer_hide_inactive);
    assert_eq!(restored.footer_items, saved.footer_items);
    assert_eq!(restored.footer_order, saved.footer_order);
    app.save().unwrap();
    let mut reopened = open(&fixture, &fixture.book);
    assert_eq!(reopened.controls, saved);
    assert_eq!(reopened.library.state(id).unwrap().bookmarks.len(), 1);
    reopened.activate(Action::ToggleBookmark).unwrap();
    reopened.library.state_mut(id).unwrap().highlights.clear();
    reopened.notice = None;
    let mut reopened_canvas = adopt(&mut reopened, ReadingViewport::PORTRAIT, 1);
    reopened
        .activate(Action::SetReadingControls(ReadingControls {
            status_bar: StatusBar::Chapter,
            top_header: TopHeader::default(),
            ..reopened.controls
        }))
        .unwrap();
    assert!(
        primitive_app::frame(&reopened, &mut reopened_canvas, false) == original_frame,
        "the restored default reading frame differs"
    );
}

#[test]
fn footer_composition_counts_authored_reference_pages_and_keeps_the_default_frame() {
    let fixture = Fixture::new();
    let nav = br#"<html><body><nav type="toc"><ol>
      <li><a href="first.xhtml">Field notes</a></li>
      <li><a href="second.xhtml">Return</a></li>
    </ol></nav><nav type="page-list"><ol>
      <li><a href="first.xhtml#passage1">i</a></li>
      <li><a href="first.xhtml#passage6">ii</a></li>
      <li><a href="first.xhtml#passage12">iii</a></li>
    </ol></nav></body></html>"#;
    write_outline_fixture(&fixture.book, Some(nav), None);
    let mut app = open(&fixture, &fixture.book);
    app.activate(Action::Jump(Destination {
        path: "EPUB/first.xhtml".to_owned(),
        fragment: Some("passage6".to_owned()),
    }))
    .unwrap();
    let location = app.current_history_location();
    let mut canvas = adopt(&mut app, ReadingViewport::PORTRAIT, 1);
    let default_caption = app.status_caption().unwrap();
    let default_frame = primitive_app::frame(&app, &mut canvas, false);
    assert!(default_caption.starts_with("Chapter "));
    assert!(default_caption.contains(" - Page "));
    let controls = ReadingControls {
        status_bar: StatusBar::Custom,
        footer_items: FooterItem::ChapterProgress.bit(),
        ..app.controls
    };
    app.activate(Action::SetReadingControls(controls)).unwrap();
    let progress = app.footer_progress().unwrap();
    let screen_caption = format!("{} ⁄⁄ {}", progress.section_page, progress.section_len);
    assert_eq!(app.status_caption().unwrap(), screen_caption);
    app.activate(Action::TogglePublisherFooter).unwrap();
    assert_eq!(app.status_caption().as_deref(), Some("2 ⁄⁄ 3"));
    assert_eq!(app.current_history_location(), location);
    assert!(!app.custom_footer_icons_enabled());
    app.activate(Action::TogglePublisherFooter).unwrap();
    app.activate(Action::SetReadingControls(ReadingControls {
        status_bar: StatusBar::Chapter,
        ..app.controls
    }))
    .unwrap();
    assert_eq!(app.status_caption().unwrap(), default_caption);
    assert_eq!(
        primitive_app::frame(&app, &mut canvas, false),
        default_frame
    );
}

#[test]
fn footer_composition_distributes_items_around_filler_and_cycles_only_metrics() {
    let fixture = Fixture::new();
    let pdf = fixture.root.join("Field guide to the coast.pdf");
    crate::app::progress_markers::tests::write_pdf(&pdf);
    let mut app = open(&fixture, &pdf);
    app.follow_fixed(2, None).unwrap();
    let mut canvas = adopt(&mut app, ReadingViewport::LANDSCAPE, 1);
    let original = app.current_history_location();
    let order = order(&[
        FooterItem::ChapterProgress,
        FooterItem::DynamicFiller,
        FooterItem::BookTitle,
    ]);
    let selected = FooterItem::ChapterProgress.bit() | FooterItem::BookTitle.bit();
    app.activate(Action::SetReadingControls(ReadingControls {
        status_bar: StatusBar::Custom,
        footer_order: order,
        footer_items: selected,
        footer_separator: crate::reading_controls::FooterSeparator::Bar,
        footer_item_bold: true,
        ..app.controls
    }))
    .unwrap();
    let no_filler = primitive_app::frame(&app, &mut canvas, false);
    let normal_status = app.status_caption();
    assert!(normal_status.as_deref().unwrap().starts_with("2 ⁄⁄ 2"));
    let toggle = app
        .footer_items()
        .into_iter()
        .find(|item| item.title == "Dynamic filler")
        .unwrap();
    app.activate(toggle.action).unwrap();
    assert_eq!(app.status_caption(), normal_status);
    let width = app.footer_right().saturating_sub(60) as u32;
    let line = app
        .custom_footer_filler_line(&mut canvas, width, None)
        .unwrap();
    assert_eq!(line.width(), width);
    assert!(
        app.custom_footer_filler_line(&mut canvas, 120, None)
            .is_none()
    );
    capture(&app, &mut canvas, "footer-filler-landscape-pdf.png", false);

    let frame = primitive_app::frame(&app, &mut canvas, false);
    let viewport = app.reading_viewport();
    let (w, h) = viewport.dimensions();
    let status = app.footer_caption_rect(width, width as i32, app.footer_caption_font_px());
    let has_ink = |pixels: &[u8], start: usize, end: usize| {
        (status.y..status.bottom()).any(|y| {
            pixels[y as usize * w as usize + start..y as usize * w as usize + end]
                .iter()
                .copied()
                .any(|pixel| pixel < 180)
        })
    };
    let left = status.x as usize;
    let right = status.right() as usize;
    assert!(has_ink(&frame, left, left + 110));
    assert!(has_ink(&frame, right - 400, right));
    assert!(!has_ink(&frame, left + 350, right - 410));
    assert!(!has_ink(&no_filler, right - 400, right));
    let exit = viewport.done();
    for y in exit.y..exit.bottom().min(h as i32) {
        let start = y as usize * w as usize + exit.x as usize;
        let end = y as usize * w as usize + exit.right() as usize;
        assert_eq!(&frame[start..end], &no_filler[start..end]);
    }

    let book = app.fixed.as_mut().unwrap();
    let title = std::mem::replace(&mut book.entry.title, "Long metadata ".repeat(120));
    assert!(
        app.custom_footer_filler_line(&mut canvas, width, None)
            .is_none()
    );
    let long_frame = primitive_app::frame(&app, &mut canvas, false);
    let controls_start = app.footer_right() as usize;
    for y in exit.y..exit.bottom().min(h as i32) {
        let start = y as usize * w as usize + controls_start;
        let end = y as usize * w as usize + exit.right() as usize;
        assert_eq!(&frame[start..end], &long_frame[start..end]);
    }
    app.fixed.as_mut().unwrap().entry.title = title;

    app.activate(Action::SetReadingControls(ReadingControls {
        footer_track_position: FooterTrackPosition::Alongside,
        ..app.controls
    }))
    .unwrap();
    assert!(app.footer_track_rect(0).is_some());
    assert!(
        app.custom_footer_filler_line(&mut canvas, width, None)
            .is_none()
    );
    app.activate(Action::SetReadingControls(ReadingControls {
        footer_scope: ProgressScope::Off,
        ..app.controls
    }))
    .unwrap();
    assert!(
        app.custom_footer_filler_line(&mut canvas, width, None)
            .is_some()
    );

    let cycle = app
        .footer_items()
        .into_iter()
        .find(|item| item.title == "Cycle custom items")
        .unwrap();
    app.activate(cycle.action).unwrap();
    assert_eq!(
        app.controls.footer_item_cycle.unwrap().selected,
        Some(FooterItem::ChapterProgress)
    );
    assert!(
        app.custom_footer_filler_line(&mut canvas, width, None)
            .is_none()
    );
    assert_eq!(
        app.input(Input::Tap(130.0, (h - 32) as f32)),
        Change::Partial
    );
    assert_eq!(
        app.controls.footer_item_cycle.unwrap().selected,
        Some(FooterItem::BookTitle)
    );
    assert_eq!(app.current_history_location(), original);

    let preset = super::super::footer_presets::Preset::capture(
        "Wide status".into(),
        app.controls,
        None,
        Default::default(),
    );
    let applied = preset.apply(ReadingControls::default()).0;
    assert_eq!(applied.footer_items, app.controls.footer_items);
    assert_eq!(applied.footer_order, order);
    app.save().unwrap();
    let reopened = open(&fixture, &pdf);
    assert_eq!(reopened.controls.footer_items, app.controls.footer_items);
    assert_eq!(reopened.controls.footer_order, order);
    assert_eq!(
        reopened.controls.footer_item_cycle,
        app.controls.footer_item_cycle
    );
}
