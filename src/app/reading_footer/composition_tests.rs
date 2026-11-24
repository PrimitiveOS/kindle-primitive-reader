use super::*;
use crate::app::selection_viewport_tests::{adopt, capture};
use crate::app::tests::{Fixture, write_outline_fixture};
use crate::epub::Destination;
use primitive_app::App;
use primitive_ui::Input;
use std::fs;

fn open(fixture: &Fixture, path: &std::path::Path) -> Reader {
    let fonts = vec![
        crate::test_fixture("fonts/NotoSans-Regular.ttf"),
        crate::test_fixture("fonts/NotoSans-Bold.ttf"),
    ];
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
    app.open_path(path).unwrap();
    app
}

fn order(items: &[FooterItem]) -> u64 {
    items.iter().enumerate().fold(0, |order, (slot, item)| {
        order | (u64::from(*item as u8) << (slot * 4))
    })
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
