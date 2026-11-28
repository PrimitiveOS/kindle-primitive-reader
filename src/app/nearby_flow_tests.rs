use super::*;
use crate::app::tests::Fixture;

fn open() -> (Fixture, Reader) {
    let fixture = Fixture::new();
    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
    app.open_path(&fixture.book).unwrap();
    (fixture, app)
}

#[test]
fn changed_publisher_policy_cancels_an_actual_search_worker_before_delivery() {
    let (_fixture, mut app) = open();
    app.start_all_reflow("Linh").unwrap();
    let cancel = Arc::clone(&app.nearby.all.as_ref().unwrap().cancel);
    let before = app.active.as_ref().unwrap().reading.anchor().unwrap();
    let settings = app.active.as_ref().unwrap().reading.config.settings;
    app.activate(Action::Text(crate::Settings {
        publisher_styles: !settings.publisher_styles,
        ..settings
    }))
    .unwrap();
    assert!(
        app.poll_nearby(Instant::now())
            .unwrap_err()
            .to_string()
            .contains("search again")
    );
    assert!(cancel.load(Ordering::Relaxed));
    assert!(app.nearby.all.is_none());
    assert!(app.hits.is_empty());
    assert_eq!(
        app.active.as_ref().unwrap().reading.anchor().unwrap(),
        before
    );
}

#[test]
fn retained_results_keep_their_style_key_when_the_controls_close() {
    let (fixture, mut app) = open();
    let mut book = Book::open(&fixture.book).unwrap();
    app.hits = Search::new("Linh").unwrap().step(&mut book).unwrap();
    assert!(!app.hits.is_empty());
    app.nearby.results_context = app.current_reflow_context();
    app.cancel_search();
    assert!(app.nearby.results_context.is_some());
    assert!(app.ensure_reflow_search_current().is_ok());
    // A successful style commit replaces the immutable session snapshot. The
    // guard compares it even after the producing job and panel have closed.
    app.active.as_mut().unwrap().reading.config.user_styles =
        crate::user_styles::Snapshot::new("p {font-weight:700}".into()).unwrap();
    assert!(
        app.ensure_reflow_search_current()
            .unwrap_err()
            .to_string()
            .contains("search again")
    );
    assert!(app.hits.is_empty());
    assert!(app.nearby.results_context.is_none());
}

#[test]
fn discarded_empty_reflow_results_do_not_error_after_leaving_the_document() {
    let (_fixture, mut app) = open();
    app.nearby.results_context = app.current_reflow_context();
    app.hits.clear();
    app.active = None;
    assert!(app.ensure_reflow_search_current().is_ok());
    assert!(app.nearby.results_context.is_none());
}

#[test]
fn stepping_a_completed_directional_match_cannot_rebind_a_stale_style_key() {
    let (_fixture, mut app) = open();
    app.editor.text = "Linh".chars().collect();
    app.editor.caret = app.editor.text.len();
    app.begin_nearby(SearchDirection::Next, false).unwrap();
    let started = Instant::now();
    while app.nearby.pending.is_some() {
        assert!(started.elapsed() < std::time::Duration::from_secs(15));
        app.poll_nearby(Instant::now()).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert!(app.nearby.current.is_some());
    assert!(app.result_selection.is_some());
    let settings = app.active.as_ref().unwrap().reading.config.settings;
    app.activate(Action::Text(crate::Settings {
        publisher_styles: !settings.publisher_styles,
        ..settings
    }))
    .unwrap();
    let error = app
        .activate(Action::StepNearby(SearchDirection::Next))
        .unwrap_err();
    assert!(error.to_string().contains("search again"));
    assert!(app.nearby.pending.is_none());
    assert!(app.nearby.current.is_none());
    assert!(app.hits.is_empty());
}

// Exercise the same directional controls that are drawn on Find and on the
// selected passage, including KOReader's current-page boundary for edge jumps.
#[test]
fn reflow_search_edge_controls_find_and_highlight_source_passages() {
    let (fixture, mut app) = open();
    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let expected = |app: &Reader, direction| {
        let active = app.active.as_ref().unwrap();
        let boundary_direction = match direction {
            SearchDirection::Next => SearchDirection::Previous,
            SearchDirection::Previous => SearchDirection::Next,
        };
        let boundary = active.reading.search_boundary(boundary_direction).unwrap();
        let context = app.current_reflow_context().unwrap();
        let mut book = Book::open(&fixture.book).unwrap();
        let chapters = book.chapters.len();
        NearbySearch::new("Linh", false, false, direction)
            .unwrap()
            .with_flow_config(context.flow.clone())
            .scan_edge(
                context.publication,
                active.reading.chapter,
                Some(&boundary),
                &AtomicBool::new(false),
                chapters,
                |index| {
                    ChapterText::read_with_flow(&mut book, index, &context.flow)
                        .map(ChapterText::with_source_only_verified_layout)
                },
            )
            .unwrap()
            .unwrap()
            .anchor
    };
    let settle = |app: &mut Reader| {
        let started = Instant::now();
        while app.nearby.pending.is_some() {
            assert!(started.elapsed() < std::time::Duration::from_secs(15));
            app.poll_nearby(Instant::now()).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    };
    let tap_rect = |rect: Rect| ((rect.x + rect.w / 2) as f32, (rect.y + rect.h / 2) as f32);
    app.show(Screen::Find);
    app.editor.text = "Linh".chars().collect();
    app.editor.caret = app.editor.text.len();
    crate::app::tests::capture_test_screen(&app, &fonts, "READER_REFLOW_EDGE_FIND_FRAME", false);
    let last = expected(&app, SearchDirection::Previous);
    let (x, y) = tap_rect(
        app.search_layout()
            .find_direction(FixedNavigation::Last.entry().0),
    );
    assert_eq!(app.nearby_input(x, y), Some(Change::Partial));
    settle(&mut app);
    assert_eq!(app.hits[0].anchor, last);
    assert!(app.fixed_result_navigation());
    assert_eq!(app.result_selection.as_ref().unwrap().start, last);
    crate::app::tests::capture_test_screen(&app, &fonts, "READER_REFLOW_EDGE_RESULT_FRAME", false);
    let first = expected(&app, SearchDirection::Next);
    let (x, y) = tap_rect(
        app.search_layout()
            .reading_footer(FixedNavigation::First.button()),
    );
    assert_eq!(app.search_result_input(x, y), Some(Change::Partial));
    settle(&mut app);
    assert_eq!(app.hits[0].anchor, first);
    assert_eq!(app.result_selection.as_ref().unwrap().start, first);
    assert!(app.search_result.is_some());
}
