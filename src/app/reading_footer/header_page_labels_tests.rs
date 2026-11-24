use super::*;
use crate::app::tests::{Fixture, capture_test_screen, write_outline_fixture};
use std::path::PathBuf;

fn open(app: &mut Reader, fixture: &Fixture) {
    app.open_path(&fixture.book).unwrap();
    app.activate(Action::TopStatusHeader(true)).unwrap();
    app.activate(Action::SetReadingControls(ReadingControls {
        top_header: TopHeader {
            clock: false,
            battery: HeaderBattery::Off,
            font_size: 28,
            ..app.controls.top_header
        },
        ..app.controls
    }))
    .unwrap();
}

fn fonts() -> Vec<PathBuf> {
    vec![
        crate::test_fixture("fonts/NotoSans-Regular.ttf"),
        crate::test_fixture("fonts/NotoSans-Bold.ttf"),
    ]
}

fn header(app: &Reader) -> String {
    app.top_header_parts().unwrap().1
}

fn screen_numbers(app: &Reader) -> String {
    let reading = &app.active.as_ref().unwrap().reading;
    format!("{}/{}", reading.page + 1, reading.probe.page_count())
}

#[test]
fn top_header_labels_follow_enabled_publisher_boundaries_and_keep_footer_pixels() {
    let fixture = Fixture::new();
    let nav = br#"<html><body><nav type="page-list"><ol>
      <li><a href="first.xhtml#passage2">Page iii</a></li>
      <li><a href="first.xhtml#passage5">vii</a></li>
      <li><a href="second.xhtml">A-19</a></li>
    </ol></nav></body></html>"#;
    write_outline_fixture(&fixture.book, Some(nav), None);
    let fonts = fonts();
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
    open(&mut app, &fixture);
    assert_eq!(header(&app), screen_numbers(&app));
    app.activate(Action::TogglePublisherMargins).unwrap();
    assert_eq!(header(&app), screen_numbers(&app));
    app.activate(Action::TogglePublisherMargins).unwrap();

    let paths: Vec<_> = fonts
        .iter()
        .map(|font| font.display().to_string())
        .collect();
    let mut canvas = Canvas::new(WIDTH, HEIGHT, &paths).unwrap();
    let baseline = primitive_app::frame(&app, &mut canvas, false);
    let origin = app.current_history_location();
    app.activate(Action::TogglePublisherFooter).unwrap();
    assert_eq!(app.current_history_location(), origin);
    assert_eq!(header(&app), "iii / A-19");
    let labels = primitive_app::frame(&app, &mut canvas, false);
    let header_end = (WIDTH * TopHeader::HEIGHT) as usize;
    assert!(labels[..header_end] != baseline[..header_end]);
    assert!(labels[header_end..] == baseline[header_end..]);
    capture_test_screen(
        &app,
        &fonts,
        "READER_TOP_HEADER_PUBLISHER_LABEL_SCREENSHOT",
        false,
    );

    let selected = app.publisher_page_window(1, 1).1.remove(0);
    app.activate(selected.action).unwrap();
    assert_eq!(header(&app), "vii / A-19");
    let presentation = app.controls.top_header;
    for (page_number, page_count, expected) in [
        (true, false, "vii"),
        (false, true, "A-19"),
        (false, false, ""),
        (true, true, "vii / A-19"),
    ] {
        app.activate(Action::SetReadingControls(ReadingControls {
            top_header: TopHeader {
                page_number,
                page_count,
                ..presentation
            },
            ..app.controls
        }))
        .unwrap();
        assert_eq!(header(&app), expected);
    }
    let selected = app.publisher_page_window(2, 1).1.remove(0);
    app.activate(selected.action).unwrap();
    assert_eq!(header(&app), "A-19 / A-19");
    app.activate(Action::TogglePublisherFooter).unwrap();
    assert_eq!(header(&app), screen_numbers(&app));
}

#[test]
fn top_header_labels_fall_back_when_the_publisher_map_is_unresolved_or_out_of_order() {
    for nav in [
        br#"<html><body><nav type="page-list"><ol>
          <li><a href="first.xhtml#passage2">iii</a></li>
          <li><a href="first.xhtml#missing">vii</a></li>
          <li><a href="second.xhtml">xi</a></li>
        </ol></nav></body></html>"#
            .as_slice(),
        br#"<html><body><nav type="page-list"><ol>
          <li><a href="first.xhtml#passage5">vii</a></li>
          <li><a href="first.xhtml#passage2">iii</a></li>
          <li><a href="second.xhtml">xi</a></li>
        </ol></nav></body></html>"#
            .as_slice(),
    ] {
        let fixture = Fixture::new();
        write_outline_fixture(&fixture.book, Some(nav), None);
        let mut app = Reader::open(&fixture.root, &fixture.state, fonts()).unwrap();
        open(&mut app, &fixture);
        app.activate(Action::TogglePublisherFooter).unwrap();
        assert_eq!(app.active.as_ref().unwrap().page_labels(), 3);
        assert_eq!(app.top_header_page_labels(), None);
        assert_eq!(header(&app), screen_numbers(&app));
    }
}

#[test]
fn top_header_labels_use_only_completed_character_maps_and_reopen_exactly() {
    use primitive_app::App;

    let fixture = Fixture::new();
    write_outline_fixture(&fixture.book, None, None);
    let fonts = fonts();
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
    open(&mut app, &fixture);
    app.activate(Action::SetCharacterPages(Some(500))).unwrap();
    app.show(Screen::Book);
    assert!(app.numbering_scan.is_some());
    assert!(app.active.as_ref().unwrap().numbering.is_none());
    assert_eq!(header(&app), screen_numbers(&app));
    let deadline = app.numbering_scan.as_ref().unwrap().deadline;
    app.step_character_pages(deadline).unwrap();
    assert!(app.numbering_scan.is_some());
    assert_eq!(header(&app), screen_numbers(&app));
    let deadline = app.numbering_scan.as_ref().unwrap().deadline;
    app.step_character_pages(deadline).unwrap();
    assert!(app.numbering_scan.is_none());
    let exact_count = app
        .active
        .as_ref()
        .unwrap()
        .numbering
        .as_ref()
        .unwrap()
        .count;
    assert!(exact_count > app.active.as_ref().unwrap().reading.probe.chapter_count());
    assert!(!app.publisher_footer_enabled());
    app.activate(Action::CharacterPage(exact_count - 1))
        .unwrap();
    assert_eq!(header(&app), format!("{exact_count} / {exact_count}"));
    capture_test_screen(
        &app,
        &fonts,
        "READER_TOP_HEADER_CHARACTER_LABEL_SCREENSHOT",
        false,
    );
    let saved = app.current_history_location();
    app.save().unwrap();
    drop(app);

    let mut app = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
    app.open_path(&fixture.book).unwrap();
    assert!(app.numbering_scan.is_none());
    assert_eq!(app.current_history_location(), saved);
    assert_eq!(header(&app), format!("{exact_count} / {exact_count}"));
    app.activate(Action::SetCharacterPages(None)).unwrap();
    assert_eq!(header(&app), screen_numbers(&app));
}
