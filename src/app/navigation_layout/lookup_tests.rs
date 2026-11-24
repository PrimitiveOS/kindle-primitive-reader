use super::super::*;
use crate::app::selection_viewport_tests::{adopt, capture, press, tap};
use crate::app::tests::Fixture;
use crate::online_lookup::{Request, Service};
use crate::session::ReadingViewport;
use primitive_app::viewport::{LogicalTurn, ViewportDecision};

fn open(fixture: &Fixture) -> Reader {
    let fonts = vec![
        crate::test_fixture("fonts/NotoSans-Regular.ttf"),
        crate::test_fixture("fonts/NotoSans-Bold.ttf"),
    ];
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
    app.open_path(&fixture.book).unwrap();
    app
}

fn turn(app: &mut Reader, viewport: ReadingViewport, generation: u64) -> Canvas {
    let orientation = if viewport == ReadingViewport::LANDSCAPE {
        ReadingOrientation::LandscapeRight
    } else {
        ReadingOrientation::Upright
    };
    app.set_reading_orientation(orientation).unwrap();
    assert_eq!(app.requested_reading_orientation(), orientation);
    adopt(app, viewport, generation)
}

fn close_definition(app: &mut Reader) {
    let done = app.definition_layout().1;
    tap(app, done);
}

fn tap_visible(app: &mut Reader, title: &str) {
    let (_, rows) = app.list_window();
    let index = rows.iter().position(|row| row.title == title).unwrap();
    let row = app.list_geometry().row(index);
    assert!(row.bottom() <= app.navigation_layout().previous().y);
    assert_eq!(
        app.input(Input::Tap(
            (row.right() - 24) as f32,
            (row.y + row.h / 2) as f32
        )),
        Change::Partial
    );
}

#[test]
fn lookup_settings_viewport_keeps_dictionary_preferences_and_the_history_window() {
    let fixture = Fixture::new();
    let dictionaries = fixture.root.join("dictionaries");
    fs::create_dir(&dictionaries).unwrap();
    for index in 1..=10 {
        let name = format!("Harbor glossary {index:02}");
        crate::dictionary::tests::write_dictionary(
            &dictionaries,
            &name,
            "sametypesequence=m\n",
            &[("tide", b"The regular rise and fall of the sea.")],
        );
    }
    let portrait = ReadingViewport::PORTRAIT;
    let landscape = ReadingViewport::LANDSCAPE;
    let mut app = open(&fixture);
    let position = app.current_position();
    adopt(&mut app, portrait, 1);
    app.activate(Action::ShowDictionaries).unwrap();
    let mut wide = turn(&mut app, landscape, 2);
    assert_eq!(app.list_geometry().rows, 7);
    let layout = app.navigation_layout();
    press(&app, &mut wide, layout.next(), false);
    tap(&mut app, layout.next());
    assert_eq!(app.list_page, 1);
    let source = "Harbor glossary 06";
    assert_eq!(app.list_window().1[0].title, source);
    tap_visible(&mut app, source);
    assert_eq!(app.screen, Screen::DictionaryOptions(5));
    tap_visible(&mut app, "Prefer for this book");
    tap(&mut app, layout.back());
    assert_eq!(app.list_window().1[0].title, source);
    assert_eq!(
        app.list_window().1[0].detail.as_deref(),
        Some("Book preference 1")
    );
    tap(&mut app, layout.back());
    assert_eq!(app.screen, Screen::Book);
    assert_eq!(app.current_position(), position);

    app.update_lookup_history(|h| {
        h.dictionary = (1..=25)
            .map(|index| {
                lookup_history::DictionaryLookup::Legacy(format!("Harbor term {index:02}"))
            })
            .collect();
        h.dictionary
            .insert(15, lookup_history::DictionaryLookup::Legacy("tide".into()));
    })
    .unwrap();
    app.activate(Action::Show(Screen::DictionaryHistory))
        .unwrap();
    assert_eq!(app.input(Input::PageForward), Change::Partial);
    assert_eq!(app.input(Input::PageForward), Change::Partial);
    assert_eq!(app.reading_list_ordinal(), 14);
    tap_visible(&mut app, "tide");
    assert_eq!(app.screen, Screen::Definition);
    assert_eq!(app.definitions[0].dictionary, source);
    let mut upright = turn(&mut app, portrait, 3);
    let done = app.definition_layout().1;
    press(&app, &mut upright, done, false);
    tap(&mut app, done);
    assert_eq!(app.screen, Screen::DictionaryHistory);
    assert_eq!(app.list_page, 1);
    assert_eq!(app.reading_list_ordinal(), 14);
    assert_eq!(app.current_position(), position);

    let rejected =
        reading_viewport_stage::draw_test_candidate(&mut app, 4, LogicalTurn::Clockwise, "", false);
    app.finish_viewport(rejected, ViewportDecision::Abort);
    assert_eq!(app.reading_viewport(), portrait);
    assert_eq!(app.reading_list_ordinal(), 14);
    turn(&mut app, landscape, 5);
    assert_eq!(app.list_page, 2);
    assert_eq!(app.reading_list_ordinal(), 14);
    drop(app);
    let reopened = open(&fixture);
    assert_eq!(
        reopened.lookup_sources("tide").unwrap()[0]
            .definition
            .dictionary,
        source
    );
    assert_eq!(reopened.current_position(), position);
}

#[test]
fn lookup_settings_viewport_online_history_preview_and_confirmation_restore_their_routes() {
    let fixture = Fixture::new();
    let mut app = open(&fixture);
    let position = app.current_position();
    let portrait = ReadingViewport::PORTRAIT;
    let landscape = ReadingViewport::LANDSCAPE;
    app.update_lookup_history(|history| {
        for text in [
            "tide",
            "harbor",
            "coast",
            "jetty",
            "waterline",
            "channel",
            "ferry",
            "anchor",
        ] {
            history.online(Request {
                service: Service::Google,
                from: "en".into(),
                to: "fr".into(),
                text: text.into(),
                full_article: false,
                with_romanization: false,
            });
        }
    })
    .unwrap();

    adopt(&mut app, portrait, 1);
    app.activate(Action::Show(Screen::OnlineHistory)).unwrap();
    let mut wide = turn(&mut app, landscape, 2);
    let layout = app.navigation_layout();
    let (count, rows) = app.list_window();
    assert_eq!((count, rows.len()), (10, 7));
    capture(&app, &mut wide, "landscape-lookup-history.png", false);
    press(&app, &mut wide, layout.next(), true);
    tap(&mut app, layout.next());
    let topic = app.list_window().1[0].title.clone();
    tap_visible(&mut app, &topic);
    let panel = app.online.as_ref().unwrap();
    assert_eq!(panel.request.text, topic);
    assert!(panel.job.is_none());
    assert_eq!(app.input(Input::Tap(1284.0, 198.0)), Change::Partial);
    assert_eq!(app.screen, Screen::OnlineHistory);
    assert_eq!(app.list_window().1[0].title, topic);
    tap_visible(&mut app, &topic);
    turn(&mut app, portrait, 3);
    tap(&mut app, online_panel::CLOSE);
    assert_eq!(app.screen, Screen::OnlineHistory);
    assert_eq!(app.list_page, 0);
    assert_eq!(app.reading_list_ordinal(), 7);
    let mut wide = turn(&mut app, landscape, 4);
    assert_eq!(app.list_page, 1);
    tap(&mut app, layout.previous());

    tap_visible(&mut app, "Clear online history");
    assert_eq!(app.screen, Screen::ClearOnlineHistory);
    let cancel = Rect::new(422, 706, 326, 100);
    press(&app, &mut wide, cancel, false);
    tap(&mut app, cancel);
    assert_eq!(app.screen, Screen::OnlineHistory);
    assert_eq!(app.lookup_history.online.len(), 8);
    tap_visible(&mut app, "Clear online history");
    turn(&mut app, portrait, 5);
    tap(&mut app, Rect::new(632, 914, 400, 100));
    assert_eq!(app.screen, Screen::OnlineHistory);
    assert!(app.lookup_history.online.is_empty());

    app.activate(Action::Show(Screen::TranslationSettings))
        .unwrap();
    turn(&mut app, landscape, 6);
    tap_visible(&mut app, "Show romanizations");
    app.activate(Action::Show(Screen::WikipediaSettings))
        .unwrap();
    tap_visible(&mut app, "Show image in search results");
    tap(&mut app, layout.back());
    assert_eq!(app.screen, Screen::BookTools);
    assert_eq!(app.current_position(), position);
    drop(app);
    let reopened = open(&fixture);
    assert!(reopened.lookup_history.online.is_empty());
    assert!(reopened.lookup_history.online_languages.show_romanizations);
    assert!(!reopened.lookup_history.online_languages.show_wiki_image);
    assert_eq!(reopened.current_position(), position);
}

#[test]
fn dictionary_history_groups_consecutive_titles_and_repeats_on_the_current_page() {
    let fixture = Fixture::new();
    let dictionaries = fixture.root.join("dictionaries");
    fs::create_dir(&dictionaries).unwrap();
    crate::dictionary::tests::write_dictionary(
        &dictionaries,
        "Coastal terms",
        "sametypesequence=m\n",
        &[
            ("tide", b"The rise and fall of the sea."),
            ("ferry", b"A boat that makes regular crossings."),
            ("quay", b"A platform beside the water."),
        ],
    );
    fs::write(
        fixture.root.join("lookup-history.json"),
        br#"{"dictionary":["legacy term"]}"#,
    )
    .unwrap();
    let pdf = fixture.root.join("Coast.pdf");
    fs::write(&pdf, crate::fixed::tests::document()).unwrap();
    let mut app = open(&fixture);
    let epub_title = app.vocabulary_entry("").book;
    for word in ["tide", "tide"] {
        app.look_up(word).unwrap();
        close_definition(&mut app);
    }
    app.open_path(&pdf).unwrap();
    let pdf_title = app.vocabulary_entry("").book;
    assert_ne!(epub_title, pdf_title);
    app.look_up("ferry").unwrap();
    close_definition(&mut app);
    app.open_path(&fixture.book).unwrap();
    app.look_up("quay").unwrap();
    close_definition(&mut app);
    let original = app.current_position();
    app.save().unwrap();
    drop(app);

    let mut app = open(&fixture);
    app.show(Screen::DictionaryHistory);
    let rows = app.items();
    let labels = rows
        .iter()
        .map(|row| row.title.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        labels,
        [
            epub_title.as_str(),
            "quay",
            pdf_title.as_str(),
            "ferry",
            epub_title.as_str(),
            "tide",
            "tide",
            "legacy term",
        ]
    );
    assert!(matches!(rows[0].action, Action::Heading));
    assert!(matches!(rows[2].action, Action::Heading));
    assert!(matches!(rows[4].action, Action::Heading));
    assert!(rows[7].detail.is_none());
    for index in [1, 3, 5, 6] {
        let stamp = rows[index].detail.as_deref().unwrap();
        assert_eq!(stamp.len(), 19);
        assert!(crate::mark_time::parse(stamp.as_bytes()).is_some());
    }
    // Keep the authored screenshot reproducible while exercising real lookup
    // timestamps above. Separate clock values also make the ordering visible.
    let screenshot_time = crate::mark_time::parse(b"2025-10-18 17:45:00").unwrap();
    app.update_lookup_history(|history| {
        for (offset, row) in history.dictionary.iter_mut().take(4).enumerate() {
            let lookup_history::DictionaryLookup::Recorded { time, .. } = row else {
                panic!("Expected a recorded lookup");
            };
            *time = Some(screenshot_time - offset as i64 * 1_120);
        }
    })
    .unwrap();
    let mut upright = adopt(&mut app, ReadingViewport::PORTRAIT, 1);
    capture(
        &app,
        &mut upright,
        "dictionary-history-grouped-portrait.png",
        false,
    );
    let mut wide = turn(&mut app, ReadingViewport::LANDSCAPE, 2);
    capture(
        &app,
        &mut wide,
        "dictionary-history-grouped-landscape.png",
        false,
    );
    tap_visible(&mut app, "ferry");
    assert_eq!(app.definition_word, "ferry");
    assert_eq!(app.screen, Screen::Definition);
    assert_eq!(app.current_position(), original);
    close_definition(&mut app);
    assert_eq!(app.screen, Screen::DictionaryHistory);
    assert_eq!(app.current_position(), original);
    let latest = &app.lookup_history.dictionary[0];
    assert_eq!(latest.word(), "ferry");
    assert_eq!(latest.book_title(), Some(epub_title.as_str()));
}

#[test]
fn dictionary_history_opt_out_and_clear_preserve_vocabulary_and_online_history() {
    let fixture = Fixture::new();
    let mut app = open(&fixture);
    app.look_up("first word").unwrap();
    close_definition(&mut app);
    app.update_lookup_history(|history| {
        history.vocabulary.push("saved word".into());
        history.online(Request {
            service: Service::Google,
            from: "en".into(),
            to: "fr".into(),
            text: "remote request".into(),
            full_article: false,
            with_romanization: false,
        });
        history.online_languages.forget_queries = true;
        history.review.automatic = true;
    })
    .unwrap();
    app.open_sources().unwrap();
    tap_visible(&mut app, "Record dictionary lookups");
    assert!(app.dictionary_settings.disable_dictionary_history);
    app.show(Screen::DictionaryEntry);
    app.look_up("second word").unwrap();
    assert_eq!(app.lookup_history.dictionary.len(), 1);
    assert!(
        app.lookup_history
            .vocabulary
            .contains(&"second word".into())
    );
    assert_eq!(app.lookup_history.online.len(), 1);
    let selection_position = app.current_position();
    adopt(&mut app, ReadingViewport::PORTRAIT, 1);
    app.show(Screen::ClearDictionaryHistory);
    let mut wide = turn(&mut app, ReadingViewport::LANDSCAPE, 2);
    capture(
        &app,
        &mut wide,
        "dictionary-history-clear-landscape.png",
        false,
    );
    assert_eq!(app.input(Input::PageBack), Change::Partial);
    assert_eq!(app.lookup_history.dictionary.len(), 1);
    app.show(Screen::ClearDictionaryHistory);
    assert_eq!(app.input(Input::Tap(1068.0, 755.0)), Change::Partial);
    assert_eq!(app.screen, Screen::Dictionaries);
    assert!(app.lookup_history.dictionary.is_empty());
    assert_eq!(app.current_position(), selection_position);
    drop(app);
    let reopened = open(&fixture);
    assert!(reopened.dictionary_settings.disable_dictionary_history);
    assert!(reopened.lookup_history.dictionary.is_empty());
    assert_eq!(
        reopened.lookup_history.vocabulary,
        ["second word", "saved word"]
    );
    assert_eq!(reopened.lookup_history.online.len(), 1);
    assert!(reopened.lookup_history.online_languages.forget_queries);
}
