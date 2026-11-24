use super::*;
use crate::app::tests::{Fixture, capture_test_screen, write_outline_fixture};
use crate::app::{Action, Change, FixedJump, Input, ReadingPosition, Screen};
use crate::library::hidden_flows::{FlowPoint, HiddenFlows};
use kurbo::Point;
use primitive_app::App;
use std::fs;

fn open(fixture: &Fixture, path: &std::path::Path) -> Reader {
    let mut app = Reader::open(
        &fixture.root,
        &fixture.state,
        vec![
            crate::test_fixture("fonts/NotoSans-Regular.ttf"),
            crate::test_fixture("fonts/NotoSans-Bold.ttf"),
        ],
    )
    .unwrap();
    app.open_path(path).unwrap();
    app
}

fn tap_chapter(app: &mut Reader, label: &str) -> Change {
    app.open_reading_edge(true, 100.0);
    let rect = app.reading_button(label);
    app.input(Input::Tap(
        (rect.x + rect.w / 2) as f32,
        (rect.y + rect.h / 2) as f32,
    ))
}

fn location(app: &Reader) -> (usize, usize) {
    let position = app.current_position().unwrap();
    (position.chapter, position.page)
}

fn reflow(app: &mut Reader) -> &mut Reading {
    &mut app.active.as_mut().unwrap().reading
}

fn custom(app: &mut Reader) -> &mut crate::library::custom_contents::CustomContents {
    let id = app.open_book_id().unwrap();
    &mut app.library.state_mut(id).unwrap().custom_contents
}

#[test]
fn chapter_tiles_follow_page_order_and_keep_one_history_step() {
    let fixture = Fixture::new();
    let nav = br##"<html xmlns:epub="http://www.idpf.org/2007/ops"><body><nav epub:type="toc"><ol>
        <li><a href="first.xhtml">Start</a></li>
        <li><a href="first.xhtml#missing">Missing fragment</a></li>
        <li><a href="second.xhtml#missing">Missing remote fragment</a></li>
        <li><a href="first.xhtml#passage13">Later</a></li>
        <li><a href="first.xhtml#passage5">Middle</a><ol>
            <li><a href="first.xhtml#passage5">Same page</a></li>
        </ol></li>
        <li><a href="second.xhtml">Last</a></li>
        </ol></nav></body></html>"##;
    write_outline_fixture(&fixture.book, Some(nav), None);
    let mut app = open(&fixture, &fixture.book);
    let start = app.current_history_location().unwrap();
    let probe = &reflow(&mut app).probe;
    let middle = probe
        .page_for(&probe.fragment("passage5").unwrap())
        .unwrap();
    let later = probe
        .page_for(&probe.fragment("passage13").unwrap())
        .unwrap();
    assert!(0 < middle && middle + 1 < later);

    app.open_reading_edge(true, 100.0);
    capture_test_screen(&app, &app.fonts, "READER_CHAPTER_NAVIGATION_SCREEN", false);
    assert_eq!(tap_chapter(&mut app, "Previous chapter"), Change::Partial);
    assert_eq!(app.notice.as_deref(), Some("No previous chapter"));
    assert!(app.back.is_empty());
    assert_eq!(tap_chapter(&mut app, "Next chapter"), Change::Partial);
    assert_eq!(app.screen, Screen::Book);
    assert_eq!(location(&app), (0, middle));
    assert_eq!(app.back, [start.clone()]);
    app.activate(Action::ReturnToLocation).unwrap();
    assert_eq!(app.current_history_location(), Some(start));

    reflow(&mut app).go_to_page(0, middle + 1).unwrap();
    let within = app.current_history_location().unwrap();
    tap_chapter(&mut app, "Previous chapter");
    assert_eq!(location(&app), (0, middle));
    assert_eq!(app.back, [within]);
    tap_chapter(&mut app, "Previous chapter");
    assert_eq!(location(&app), (0, 0));
    for expected in [(0, middle), (0, later), (1, 0)] {
        tap_chapter(&mut app, "Next chapter");
        assert_eq!(location(&app), expected);
    }
    let history_len = app.back.len();
    tap_chapter(&mut app, "Next chapter");
    assert_eq!(app.notice.as_deref(), Some("No next chapter"));
    assert_eq!(app.back.len(), history_len);
    assert_eq!(location(&app), (1, 0));
}

#[test]
fn facing_pages_skip_the_visible_second_leaf_and_custom_contents_replace_publisher_heads() {
    let fixture = Fixture::new();
    write_outline_fixture(&fixture.book, None, None);
    let mut app = open(&fixture, &fixture.book);
    app.activate(Action::FacingPages(true)).unwrap();
    assert!(reflow(&mut app).uses_facing_pages());
    for page in 1..=2 {
        let probe = &reflow(&mut app).probe;
        let anchor = probe.page_anchor(page).unwrap();
        assert_eq!(probe.page_for(&anchor), Some(page));
        // A custom heading follows its source position even when the saved
        // screen page is from a different layout.
        custom(&mut app)
            .save(
                ReadingPosition {
                    chapter: 0,
                    page: page + 10,
                    anchor: Some(anchor),
                },
                &format!("My chapter {page}"),
            )
            .unwrap();
    }
    custom(&mut app).enabled = true;
    assert_eq!(reflow(&mut app).last_visible_page(), 1);
    tap_chapter(&mut app, "Next chapter");
    assert_eq!(location(&app), (0, 2));
    tap_chapter(&mut app, "Previous chapter");
    assert_eq!(location(&app), (0, 1));
    assert_eq!(reflow(&mut app).first_visible_page(), 0);

    let history_len = app.back.len();
    tap_chapter(&mut app, "Previous chapter");
    assert_eq!(app.back.len(), history_len);
    tap_chapter(&mut app, "Next chapter");
    custom(&mut app).enabled = false;
    tap_chapter(&mut app, "Next chapter");
    assert_eq!(location(&app), (1, 0));
}

#[test]
fn chapter_navigation_uses_the_regular_epub_flow_or_the_current_nonlinear_run() {
    let fixture = Fixture::new();
    crate::app::nonlinear_tests::authored_book(&fixture.book);
    let mut app = open(&fixture, &fixture.book);
    reflow(&mut app).set_hide_nonlinear(true);
    reflow(&mut app).go_to_page(1, 0).unwrap();
    tap_chapter(&mut app, "Next chapter");
    assert_eq!(location(&app), (4, 0));
    tap_chapter(&mut app, "Previous chapter");
    assert_eq!(location(&app), (1, 0));

    reflow(&mut app).jump(2, Some("note")).unwrap();
    tap_chapter(&mut app, "Next chapter");
    assert_eq!(location(&app), (3, 0));
    tap_chapter(&mut app, "Next chapter");
    assert_eq!(location(&app), (4, 0));
    tap_chapter(&mut app, "Previous chapter");
    assert_eq!(location(&app), (1, 0));
}

#[test]
fn scrolling_can_jump_to_the_short_final_page_and_continue_forward() {
    let fixture = Fixture::new();
    write_outline_fixture(&fixture.book, None, None);
    let mut app = open(&fixture, &fixture.book);
    app.activate(Action::ReadingMode(true)).unwrap();
    let probe = &reflow(&mut app).probe;
    let last_page = probe.page_count() - 1;
    assert!(probe.page_range(last_page).start > probe.scroll_end());
    let anchor = probe.page_anchor(last_page).unwrap();
    let next = reflow(&mut app).config.chapter(1).unwrap();
    for (chapter, page, anchor) in [(0, last_page, anchor), (1, 0, next.page_anchor(0).unwrap())] {
        custom(&mut app)
            .save(
                ReadingPosition {
                    chapter,
                    page,
                    anchor: Some(anchor),
                },
                "Chapter",
            )
            .unwrap();
    }
    custom(&mut app).enabled = true;
    tap_chapter(&mut app, "Next chapter");
    assert_eq!(location(&app), (0, last_page));
    tap_chapter(&mut app, "Next chapter");
    assert_eq!(location(&app), (1, 0));
    app.activate(Action::ReturnToLocation).unwrap();
    assert_eq!(location(&app), (0, last_page));
    app.input(Input::PageForward);
    if let Some(due) = reflow(&mut app).deadline() {
        app.tick(due + std::time::Duration::from_millis(1));
    }
    assert_eq!(location(&app), (1, 0));
}

#[test]
fn pdf_chapter_jump_restores_original_pan_and_obeys_custom_contents_and_flows() {
    let fixture = Fixture::new();
    let path = fixture.root.join("Chapter chart.pdf");
    fs::write(
        &path,
        crate::fixed::tests::pdf_objects(&[
            "<< /Type /Catalog /Pages 2 0 R /Outlines 9 0 R >>",
            "<< /Type /Pages /Kids [3 0 R 4 0 R 5 0 R 6 0 R 7 0 R 8 0 R] /Count 6 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 750] >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 750] >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 750] >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 750] >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 750] >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 750] >>",
            "<< /Type /Outlines /First 10 0 R /Last 12 0 R /Count 3 >>",
            "<< /Title (Preface) /Dest [3 0 R /Fit] /Parent 9 0 R /Next 11 0 R >>",
            "<< /Title (Chart) /Dest [5 0 R /Fit] /Parent 9 0 R /Prev 10 0 R /Next 12 0 R >>",
            "<< /Title (Appendix) /Dest [8 0 R /Fit] /Parent 9 0 R /Prev 11 0 R >>",
        ]),
    )
    .unwrap();
    let mut app = open(&fixture, &path);
    let reading = &mut app.fixed.as_mut().unwrap().reading;
    reading.set_zoom(3).unwrap();
    reading
        .jump_to(1, Some(&FixedJump::LinkPoint(Point::new(80.0, 220.0))))
        .unwrap();
    let original = app.current_history_location().unwrap();
    let original_pan = original.scan_point.unwrap();
    tap_chapter(&mut app, "Next chapter");
    assert_eq!(location(&app), (0, 2));
    assert_eq!(app.back, [original]);
    assert_ne!(
        app.current_history_location().unwrap().scan_point,
        Some(original_pan)
    );
    app.activate(Action::ReturnToLocation).unwrap();
    assert_eq!(location(&app), (0, 1));
    let restored = app.current_history_location().unwrap().scan_point.unwrap();
    assert!(restored.distance(original_pan) < 0.5);

    let fixed_position = |page| ReadingPosition {
        chapter: 0,
        page,
        anchor: None,
    };
    for page in [3, 4] {
        custom(&mut app)
            .save(fixed_position(page), &format!("My chart {page}"))
            .unwrap();
    }
    custom(&mut app).enabled = true;
    let mut flows = HiddenFlows::default();
    flows.enabled = true;
    for (page, hidden) in [(3, true), (4, false)] {
        flows
            .insert(FlowPoint {
                position: fixed_position(page),
                hidden,
            })
            .unwrap();
    }
    app.fixed
        .as_mut()
        .unwrap()
        .reading
        .set_hidden_flows(&flows)
        .unwrap();
    tap_chapter(&mut app, "Next chapter");
    assert_eq!(location(&app), (0, 4));
    let history_len = app.back.len();
    tap_chapter(&mut app, "Next chapter");
    assert_eq!(app.back.len(), history_len);
    custom(&mut app).enabled = false;
    tap_chapter(&mut app, "Next chapter");
    assert_eq!(location(&app), (0, 5));
}
