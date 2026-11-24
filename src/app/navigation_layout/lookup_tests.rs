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
            .map(|index| format!("Harbor term {index:02}"))
            .collect();
        h.dictionary.insert(15, "tide".into());
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
