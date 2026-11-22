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
