use super::*;
use crate::app::tests::{Fixture, capture_test_screen, tap_contents_item, tap_reading_control};
use crate::fixed::tests::pdf_objects;
use std::io::Write;

const ADDRESS: &str = "https://example.org/harbor/tide-chart?edition=2#table";

fn test_fonts() -> Vec<PathBuf> {
    ["fonts/NotoSans-Regular.ttf", "fonts/NotoSans-Bold.ttf"]
        .into_iter()
        .map(crate::test_fixture)
        .collect()
}

fn write_book(path: &Path) {
    write_book_with_leading_text(path, "");
}

fn write_book_with_leading_text(path: &Path, leading: &str) {
    let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
    let options = zip::write::SimpleFileOptions::default();
    let first = format!(
        r#"<html xmlns="http://www.w3.org/1999/xhtml"><body><h1>Harbor field guide</h1>
        {leading}<p id="approach">Consult the <a href="{ADDRESS}">online tide chart</a> before crossing the harbor.</p>
        <p><a href="routes.xhtml#route">Local route</a> passes the eastern beacon.</p>
        <p>Check the wind and current before returning.</p>
        <p><a href="javascript:alert('script')">Incorrect link</a></p></body></html>"#
    );
    for (name, text) in [
        ("mimetype", "application/epub+zip"),
        (
            "META-INF/container.xml",
            r#"<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="book.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#,
        ),
        (
            "book.opf",
            r#"<package xmlns="http://www.idpf.org/2007/opf" version="3.0"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>Harbor field guide</dc:title></metadata><manifest><item id="guide" href="guide.xhtml" media-type="application/xhtml+xml"/><item id="routes" href="routes.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="guide"/><itemref idref="routes"/></spine></package>"#,
        ),
        ("guide.xhtml", first.as_str()),
        (
            "routes.xhtml",
            r#"<html xmlns="http://www.w3.org/1999/xhtml"><body><h1 id="route">Eastern route</h1><p>Return by the beacon.</p></body></html>"#,
        ),
    ] {
        zip.start_file(name, options).unwrap();
        zip.write_all(text.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
}

fn hit(reading: &Reading, href: &str) -> (f32, f32) {
    (35..700)
        .step_by(5)
        .find_map(|y| {
            (35..1200)
                .step_by(5)
                .find(|&x| reading.link_href_at(x as f32, y as f32) == Some(href))
                .map(|x| (x as f32, y as f32))
        })
        .expect("a visible link")
}

fn open_preferences(app: &mut Reader) {
    app.open_reading_edge(true, WIDTH as f32 * 0.5);
    if app.top_group != Some(TopGroup::Settings) {
        assert_eq!(tap_reading_control(app, "Settings"), Change::Partial);
    }
    assert_eq!(tap_reading_control(app, "Links"), Change::Partial);
    assert_eq!(app.screen, Screen::LinkSettings);
}

#[test]
fn reflow_link_tap_settings_match_defaults_preserve_explicit_links_and_enlarge_internal_hits() {
    let fixture = Fixture::new();
    write_book(&fixture.book);
    let fonts = test_fonts();
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
    app.open_path(&fixture.book).unwrap();
    let origin = app.current_history_location().unwrap();
    let reading = &app.active.as_ref().unwrap().reading;
    let external = hit(reading, ADDRESS);
    let internal = hit(reading, "routes.xhtml#route");
    let nearby = [10.0, 18.0, 26.0, 34.0, 42.0]
        .into_iter()
        .map(|dx| (internal.0 - dx, internal.1))
        .find(|&(x, y)| {
            reading.link_href_at(x, y).is_none()
                && nearest_reflow_link(reading, x, y, LINK_TAP_RADIUS, true).is_some()
        })
        .expect("blank touch near local link");
    let mut canvas = Canvas::new(
        WIDTH,
        HEIGHT,
        &fonts
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>(),
    )
    .unwrap();
    let frame = primitive_app::frame(&app, &mut canvas, false);

    assert_eq!(
        app.input(Input::Tap(external.0, external.1)),
        Change::Partial
    );
    assert!(app.link_popup.is_some());
    assert_eq!(app.current_history_location(), Some(origin.clone()));
    app.input(Input::Tap(50.0, 50.0));
    assert!(app.link_popup.is_none());

    let defaults = app.controls;
    assert!(LinkPreference::Follow.checked(defaults));
    assert!(!LinkPreference::IgnoreExternal.checked(defaults));
    assert!(!LinkPreference::LargerArea.checked(defaults));
    assert!(app.reflow_tap_link(nearby.0, nearby.1).is_none());
    assert!(
        !app.page_link_items()
            .iter()
            .any(|item| item.title == "Incorrect link")
    );
    open_preferences(&mut app);
    tap_contents_item(&mut app, "Ignore external links on tap");
    tap_contents_item(&mut app, "Allow larger tap area around links");
    capture_test_screen(&app, &fonts, "READER_LINK_TAP_SETTINGS_FRAME", false);
    app.show(Screen::Book);
    assert_eq!(app.current_history_location(), Some(origin.clone()));
    assert_eq!(primitive_app::frame(&app, &mut canvas, false), frame);
    assert!(app.reflow_tap_link(external.0, external.1).is_none());
    assert!(
        app.reflow_tap_link(internal.0, internal.1 + 150.0)
            .is_none()
    );
    assert!(app.reflow_tap_link(nearby.0, nearby.1).is_some());
    assert_eq!(app.input(Input::Tap(nearby.0, nearby.1)), Change::Partial);
    assert_eq!(app.current_position().unwrap().chapter, 1);
    app.activate(Action::ReturnToLocation).unwrap();
    assert_eq!(app.current_history_location(), Some(origin.clone()));

    open_preferences(&mut app);
    tap_contents_item(&mut app, "Tap to follow links");
    let disabled: Vec<_> = app
        .items()
        .into_iter()
        .filter(|item| matches!(item.action, Action::Heading))
        .map(|item| item.title)
        .collect();
    assert_eq!(
        disabled,
        [
            "Allow larger tap area around links",
            "Ignore external links on tap",
            "Ignore external links on swipe",
        ]
    );
    app.show(Screen::Book);
    assert!(app.reflow_tap_link(internal.0, internal.1).is_none());
    app.show(Screen::Links);
    tap_contents_item(&mut app, "online tide chart");
    assert!(app.link_popup.is_some());
    assert_eq!(app.current_history_location(), Some(origin.clone()));
    app.input(Input::Tap(50.0, 50.0));
    app.show(Screen::Links);
    tap_contents_item(&mut app, "Local route");
    assert_eq!(app.current_position().unwrap().chapter, 1);
    app.activate(Action::ReturnToLocation).unwrap();
    app.save().unwrap();
    let mut reopened = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
    reopened.open_path(&fixture.book).unwrap();
    assert_eq!(reopened.current_history_location(), Some(origin));
    assert!(!LinkPreference::Follow.checked(reopened.controls));
    assert!(LinkPreference::IgnoreExternal.checked(reopened.controls));
    assert!(LinkPreference::LargerArea.checked(reopened.controls));
}

fn write_pdf(path: &Path) {
    fs::write(
        path,
        pdf_objects(&[
            "<< /Type /Catalog /Pages 2 0 R >>".into(),
            "<< /Type /Pages /Count 2 /Kids [3 0 R 4 0 R] /MediaBox [0 0 600 800] >>".into(),
            "<< /Type /Page /Parent 2 0 R /Annots [5 0 R 6 0 R] >>".into(),
            "<< /Type /Page /Parent 2 0 R >>".into(),
            format!(
                "<< /Subtype /Link /Rect [72 550 220 580] /A << /S /URI /URI ({ADDRESS}) >> >>"
            ),
            "<< /Subtype /Link /Rect [72 430 220 460] /Dest [4 0 R /XYZ 110 525 null] >>".into(),
        ]),
    )
    .unwrap();
}

#[test]
fn pdf_external_filter_keeps_internal_and_explicit_links_without_a_larger_tap_area() {
    let fixture = Fixture::new();
    let path = fixture.root.join("Harbor links.pdf");
    write_pdf(&path);
    let mut app = Reader::open(&fixture.root, &fixture.state, test_fonts()).unwrap();
    app.open_path(&path).unwrap();
    let original = app.current_history_location();
    let reading = &app.fixed.as_ref().unwrap().reading;
    let [external, internal] = reading.visible_links().try_into().unwrap();
    let center = |link: PdfLink| {
        let p = reading.visible_rectangle(0, link.bounds).unwrap().center();
        (p.x as f32, p.y as f32)
    };
    let external = center(external);
    let internal = center(internal);
    assert_eq!(
        app.input(Input::Tap(external.0, external.1)),
        Change::Partial
    );
    assert!(app.link_popup.is_some());
    app.input(Input::Tap(30.0, 30.0));

    open_preferences(&mut app);
    assert!(!app.items().iter().any(|item| item.title.contains("larger")));
    tap_contents_item(&mut app, "Ignore external links on tap");
    app.show(Screen::Fixed);
    assert!(app.fixed_tap_link(external.0, external.1).is_none());
    // A filtered ordinary tap goes to the same reading input path as unlinked ink.
    let _ = app.input(Input::Tap(external.0, external.1));
    assert!(app.link_popup.is_none());
    if app.current_history_location() != original {
        app.jump_fixed(0, None).unwrap();
    }
    assert_eq!(
        app.input(Input::Tap(internal.0, internal.1)),
        Change::Partial
    );
    assert_eq!(app.current_position().unwrap().page, 1);
    app.activate(Action::ReturnToLocation).unwrap();

    app.show(Screen::Links);
    tap_contents_item(&mut app, "External link");
    assert!(app.link_popup.is_some());
    assert_eq!(app.current_history_location(), original);
}

fn back_swipe(app: &mut Reader) -> Change {
    app.swipe(
        Input::Swipe {
            dx: 300.0,
            dy: 15.0,
            start_y: 700.0,
        },
        500.0,
    )
}

#[test]
fn epub_swipe_back_restores_the_exact_passage_then_yields_to_paging() {
    let fixture = Fixture::new();
    let long_approach =
        "<p>Follow the path from the marsh behind the harbor toward the eastern beacon.</p>"
            .repeat(65);
    write_book_with_leading_text(&fixture.book, &long_approach);
    let fonts = test_fonts();
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
    app.open_path(&fixture.book).unwrap();
    assert!(!app.controls.swipe_to_go_back);
    app.active
        .as_mut()
        .unwrap()
        .reading
        .jump(0, Some("approach"))
        .unwrap();
    let origin = app.current_history_location().unwrap();
    assert!(origin.position.page > 0 && origin.position.anchor.is_some());

    app.follow(Destination {
        path: "routes.xhtml".into(),
        fragment: Some("route".into()),
    })
    .unwrap();
    let target = app.current_history_location().unwrap();
    assert_eq!(target.position.chapter, 1);
    back_swipe(&mut app);
    assert_eq!(app.back, [origin.clone()]);
    app.restore_position(&target.position).unwrap();

    open_preferences(&mut app);
    tap_contents_item(&mut app, "Tap to follow links");
    tap_contents_item(&mut app, "Swipe to go back");
    assert!(app.controls.swipe_to_go_back);
    capture_test_screen(&app, &fonts, "READER_LINK_SWIPE_BACK_FRAME", false);
    app.show(Screen::Book);

    app.input(Input::PageBack);
    app.tick(Instant::now() + Duration::from_secs(1));
    assert_eq!(app.back, [origin.clone()]);
    app.restore_position(&target.position).unwrap();

    app.show_link_address(ADDRESS.into());
    assert_eq!(back_swipe(&mut app), Change::Partial);
    assert!(app.link_popup.is_some());
    assert_eq!(app.back, [origin.clone()]);
    app.input(Input::Tap(30.0, 30.0));

    app.gestures
        .set(gestures::Gesture::TopRight, gestures::Binding::Bookmarks)
        .unwrap();
    assert_eq!(
        app.swipe(
            Input::Swipe {
                dx: 300.0,
                dy: 0.0,
                start_y: 30.0,
            },
            500.0,
        ),
        Change::Partial
    );
    assert_eq!(app.screen, Screen::Bookmarks);
    assert_eq!(app.back, [origin.clone()]);
    app.show(Screen::Book);
    assert_eq!(
        app.swipe(
            Input::Swipe {
                dx: 40.0,
                dy: 230.0,
                start_y: 30.0,
            },
            500.0,
        ),
        Change::Partial
    );
    assert_eq!(app.screen, Screen::BookTools);
    assert!(app.reading_top);
    assert_eq!(app.back, [origin.clone()]);
    app.show(Screen::Book);

    let reading = &app.active.as_ref().unwrap().reading;
    app.selection = (30..220).step_by(5).find_map(|y| {
        (50..550)
            .step_by(5)
            .find_map(|x| reading.word_at(x as f32, y as f32))
    });
    assert!(app.selection.is_some());
    back_swipe(&mut app);
    assert_eq!(app.back, [origin.clone()]);
    app.restore_position(&target.position).unwrap();

    assert_eq!(back_swipe(&mut app), Change::Partial);
    assert_eq!(app.current_history_location(), Some(origin.clone()));
    assert!(app.back.is_empty());
    assert_eq!(app.forward, [target]);
    assert_eq!(back_swipe(&mut app), Change::Partial);
    assert_eq!(app.notice.as_deref(), Some("Location history is empty."));
    assert_eq!(app.current_history_location(), Some(origin.clone()));
    assert!(!app.swipe_back_notice_pending);
    back_swipe(&mut app);
    app.tick(Instant::now() + Duration::from_secs(1));
    assert_ne!(app.current_history_location(), Some(origin));
    assert!(app.notice.is_none());

    app.save().unwrap();
    let mut reopened = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
    reopened.open_path(&fixture.book).unwrap();
    assert!(reopened.controls.swipe_to_go_back);
    assert!(!reopened.swipe_back_notice_pending);
    let reopened_position = reopened.current_position().unwrap();
    back_swipe(&mut reopened);
    reopened.tick(Instant::now() + Duration::from_secs(1));
    assert_ne!(reopened.current_position(), Some(reopened_position));
    assert!(reopened.notice.is_none());
}

#[test]
fn pdf_swipe_back_restores_a_panned_viewport_before_normal_zoom_panning() {
    let fixture = Fixture::new();
    let path = fixture.root.join("Harbor links.pdf");
    write_pdf(&path);
    let mut app = Reader::open(&fixture.root, &fixture.state, test_fonts()).unwrap();
    app.open_path(&path).unwrap();
    open_preferences(&mut app);
    tap_contents_item(&mut app, "Swipe to go back");
    app.show(Screen::Fixed);
    app.activate(Action::SetFixedZoom(3)).unwrap();
    app.swipe(
        Input::Swipe {
            dx: -130.0,
            dy: -120.0,
            start_y: 700.0,
        },
        500.0,
    );
    let original = app.current_history_location().unwrap();
    let origin = original.scan_point.unwrap();
    app.show(Screen::Links);
    tap_contents_item(&mut app, "Page 2");
    assert_eq!(app.current_position().unwrap().page, 1);
    assert_eq!(app.back, [original.clone()]);

    let linked_viewport = app.current_history_location().unwrap();
    app.follow_fixed(1, Some(FixedJump::LinkPoint(kurbo::Point::ORIGIN)))
        .unwrap();
    assert_eq!(app.back, [original.clone(), linked_viewport.clone()]);
    assert_eq!(back_swipe(&mut app), Change::Partial);
    let same_page_return = app.current_history_location().unwrap();
    assert_eq!(same_page_return.position, linked_viewport.position);
    assert!(
        (same_page_return.scan_point.unwrap() - linked_viewport.scan_point.unwrap()).hypot() < 1.0
    );

    assert_eq!(back_swipe(&mut app), Change::Partial);
    let returned = app.current_history_location().unwrap();
    assert_eq!(returned.position, original.position);
    assert!((returned.scan_point.unwrap() - origin).hypot() < 1.0);
    assert_eq!(back_swipe(&mut app), Change::Partial);
    assert_eq!(app.notice.as_deref(), Some("Location history is empty."));
    assert_eq!(app.current_history_location(), Some(returned));
    assert_eq!(back_swipe(&mut app), Change::Partial);
    let panned = app.current_history_location().unwrap();
    assert_eq!(panned.position, original.position);
    assert!((panned.scan_point.unwrap() - origin).hypot() > 10.0);
    assert!(app.notice.is_none());
}

fn swipe_between(app: &mut ReaderRuntime, start: (f32, f32), end: (f32, f32)) -> Change {
    let (dx, dy) = (end.0 - start.0, end.1 - start.1);
    assert!(-dx > dy.abs() * 1.5, "test path must be a left swipe");
    app.swipe(
        Input::Swipe {
            dx,
            dy,
            start_y: start.1,
        },
        start.0,
    )
}

#[test]
fn epub_nearest_link_swipe_keeps_tap_independent_and_uses_the_device_touch_origin() {
    let fixture = Fixture::new();
    write_book(&fixture.book);
    let fonts = test_fonts();
    let mut runtime =
        ReaderRuntime(Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap());
    runtime.0.open_path(&fixture.book).unwrap();
    let origin = runtime.0.current_history_location().unwrap();
    let reading = &runtime.0.active.as_ref().unwrap().reading;
    let external = hit(reading, ADDRESS);
    let local = hit(reading, "routes.xhtml#route");

    assert!(!LinkPreference::SwipeFollow.checked(runtime.0.controls));
    assert!(LinkPreference::SwipeIgnoreExternal.checked(runtime.0.controls));
    assert_eq!(runtime.0.controls.swipe_ignore_external_links, None);
    swipe_between(&mut runtime, external, local);
    runtime.tick(Instant::now() + Duration::from_secs(1));
    assert_eq!(runtime.0.current_position().unwrap().chapter, 1);
    assert!(runtime.0.back.is_empty());
    runtime.0.restore_position(&origin.position).unwrap();

    open_preferences(&mut runtime.0);
    let ignore = runtime
        .0
        .items()
        .into_iter()
        .find(|item| item.title == "Ignore external links on swipe")
        .unwrap();
    assert!(matches!(ignore.action, Action::Heading));
    tap_contents_item(&mut runtime.0, "Tap to follow links");
    tap_contents_item(&mut runtime.0, "Swipe to follow nearest link");
    capture_test_screen(&runtime.0, &fonts, "READER_LINK_SWIPE_FOLLOW_FRAME", false);
    runtime.0.show(Screen::Book);

    assert_eq!(
        swipe_between(&mut runtime, external, local),
        Change::Partial
    );
    assert_eq!(runtime.0.current_position().unwrap().chapter, 1);
    assert!(runtime.0.link_popup.is_none());
    assert_eq!(runtime.0.back, [origin.clone()]);
    runtime.0.activate(Action::ReturnToLocation).unwrap();

    open_preferences(&mut runtime.0);
    tap_contents_item(&mut runtime.0, "Ignore external links on swipe");
    runtime.0.show(Screen::Book);
    assert!(!LinkPreference::Follow.checked(runtime.0.controls));
    assert!(!LinkPreference::SwipeIgnoreExternal.checked(runtime.0.controls));

    assert_eq!(
        swipe_between(&mut runtime, external, local),
        Change::Partial
    );
    assert!(runtime.0.link_popup.is_some());
    assert_eq!(runtime.0.current_history_location(), Some(origin.clone()));
    assert!(runtime.0.back.is_empty());

    runtime
        .0
        .gestures
        .set(gestures::Gesture::TopLeft, gestures::Binding::Bookmarks)
        .unwrap();
    let left_edge_swipe = Input::Swipe {
        dx: -300.0,
        dy: 0.0,
        start_y: 35.0,
    };
    runtime.swipe(left_edge_swipe, 700.0);
    assert!(runtime.0.link_popup.is_some());
    runtime.input(Input::Tap(30.0, 30.0));
    runtime.swipe(left_edge_swipe, 700.0);
    assert_eq!(runtime.0.screen, Screen::Bookmarks);
    assert!(runtime.0.back.is_empty());
    runtime.0.show(Screen::Book);

    runtime.0.save().unwrap();
    let mut reopened = ReaderRuntime(Reader::open(&fixture.root, &fixture.state, fonts).unwrap());
    reopened.0.open_path(&fixture.book).unwrap();
    assert!(LinkPreference::SwipeFollow.checked(reopened.0.controls));
    assert!(!LinkPreference::SwipeIgnoreExternal.checked(reopened.0.controls));
    assert_eq!(
        swipe_between(&mut reopened, external, local),
        Change::Partial
    );
    assert!(reopened.0.link_popup.is_some());
    assert_eq!(reopened.0.current_history_location(), Some(origin));
}

#[test]
fn epub_nearest_link_swipe_opens_and_dismisses_the_footnote_in_place() {
    let fixture = Fixture::new();
    let mut runtime =
        ReaderRuntime(Reader::open(&fixture.root, &fixture.state, test_fonts()).unwrap());
    runtime.0.open_path(&fixture.book).unwrap();
    let origin = runtime.0.current_history_location();
    let reading = &runtime.0.active.as_ref().unwrap().reading;
    let (x, y) = hit(reading, "#tide-note");
    runtime
        .0
        .toggle_link_preference(LinkPreference::SwipeFollow)
        .unwrap();
    runtime
        .0
        .toggle_link_preference(LinkPreference::SwipeBack)
        .unwrap();
    assert_eq!(
        swipe_between(&mut runtime, (x, y), (x - 80.0, y)),
        Change::Partial
    );
    assert_eq!(runtime.0.screen, Screen::Footnote);
    assert_eq!(runtime.0.current_history_location(), origin);
    assert!(runtime.0.back.is_empty());
    runtime.swipe(
        Input::Swipe {
            dx: 120.0,
            dy: 0.0,
            start_y: 700.0,
        },
        500.0,
    );
    assert_eq!(runtime.0.screen, Screen::Book);
    assert_eq!(runtime.0.current_history_location(), origin);
}

#[test]
fn pdf_nearest_link_swipe_stays_on_the_touched_scan_and_yields_to_paging() {
    let fixture = Fixture::new();
    let path = fixture.root.join("Three harbor charts.pdf");
    fs::write(
        &path,
        pdf_objects(&[
            "<< /Type /Catalog /Pages 2 0 R >>".into(),
            "<< /Type /Pages /Count 3 /Kids [3 0 R 4 0 R 5 0 R] /MediaBox [0 0 600 800] >>".into(),
            "<< /Type /Page /Parent 2 0 R /Annots [6 0 R 7 0 R] >>".into(),
            "<< /Type /Page /Parent 2 0 R >>".into(),
            "<< /Type /Page /Parent 2 0 R >>".into(),
            format!(
                "<< /Subtype /Link /Rect [420 450 550 480] /A << /S /URI /URI ({ADDRESS}) >> >>"
            ),
            "<< /Subtype /Link /Rect [70 450 190 480] /Dest [4 0 R /XYZ 110 525 null] >>".into(),
        ]),
    )
    .unwrap();
    let mut runtime =
        ReaderRuntime(Reader::open(&fixture.root, &fixture.state, test_fonts()).unwrap());
    runtime.0.open_path(&path).unwrap();
    let origin = runtime.0.current_history_location().unwrap();
    let reading = &runtime.0.fixed.as_ref().unwrap().reading;
    let [external, internal] = reading.visible_links().try_into().unwrap();
    let center = |link: PdfLink| {
        let point = reading.visible_rectangle(0, link.bounds).unwrap().center();
        (point.x as f32, point.y as f32)
    };
    let external = center(external);
    let internal = center(internal);

    open_preferences(&mut runtime.0);
    tap_contents_item(&mut runtime.0, "Swipe to follow nearest link");
    runtime.0.show(Screen::Fixed);
    assert_eq!(
        swipe_between(&mut runtime, external, internal),
        Change::Partial
    );
    assert_eq!(runtime.0.current_position().unwrap().page, 1);
    assert!(runtime.0.link_popup.is_none());
    assert_eq!(runtime.0.back, [origin.clone()]);

    swipe_between(&mut runtime, external, internal);
    runtime.tick(Instant::now() + Duration::from_secs(1));
    assert_eq!(runtime.0.current_position().unwrap().page, 2);
    assert_eq!(runtime.0.back, [origin.clone()]);
    runtime.0.activate(Action::ReturnToLocation).unwrap();

    open_preferences(&mut runtime.0);
    tap_contents_item(&mut runtime.0, "Ignore external links on swipe");
    tap_contents_item(&mut runtime.0, "Ignore external links on tap");
    runtime.0.show(Screen::Fixed);
    assert!(runtime.0.fixed_tap_link(external.0, external.1).is_none());
    assert_eq!(
        swipe_between(&mut runtime, external, internal),
        Change::Partial
    );
    assert!(runtime.0.link_popup.is_some());
    assert_eq!(runtime.0.current_history_location(), Some(origin));

    runtime.input(Input::Tap(30.0, 30.0));
    runtime.0.activate(Action::SetFixedZoom(1)).unwrap();
    let zoom_origin = runtime.0.current_history_location();
    let reading = &runtime.0.fixed.as_ref().unwrap().reading;
    let external_link = reading
        .visible_links()
        .into_iter()
        .find(|link| link.target == PdfLinkTarget::External)
        .unwrap();
    let center = reading
        .visible_rectangle(0, external_link.bounds)
        .unwrap()
        .center();
    let (x, y) = (center.x as f32, center.y as f32);
    assert!((x - external.0).abs() > 20.0 || (y - external.1).abs() > 20.0);
    assert_eq!(
        swipe_between(&mut runtime, (x, y), (x - 150.0, y)),
        Change::Partial
    );
    assert!(runtime.0.link_popup.is_some());
    assert_eq!(runtime.0.current_history_location(), zoom_origin);
}
