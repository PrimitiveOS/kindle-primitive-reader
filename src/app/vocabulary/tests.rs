use super::*;
use crate::app::{
    lookup_history::LookupHistory,
    tests::{Fixture, capture_test_screen},
};
use primitive_app::App;
use std::{fs, path::PathBuf};

fn choose(app: &mut Reader, title: &str) {
    let (_, rows) = app.list_window();
    let n = rows
        .iter()
        .position(|item| item.title == title)
        .unwrap_or_else(|| panic!("Missing {title:?} on {:?}", app.screen));
    let rect = crate::app::list_rect(rows.len());
    assert_eq!(
        app.input(Input::Tap(
            (rect.x + 80) as f32,
            (rect.y + crate::app::ROW_HEIGHT * n as i32 + 35) as f32
        )),
        Change::Partial
    );
}

fn reader(fixture: &Fixture) -> (Reader, Vec<PathBuf>) {
    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
    app.open_path(&fixture.book).unwrap();
    (app, fonts)
}

#[test]
fn review_intervals_and_long_streaks_keep_the_stock_schedule() {
    let mut entry = Entry::default();
    for (index, delay) in [
        1800, 43200, 86400, 172800, 345600, 604800, 1296000, 2592000, 5184000, 10368000, 20736000,
        41472000, 82944000, 165888000, 165888000,
    ]
    .into_iter()
    .enumerate()
    {
        entry.review(true, 1000);
        assert_eq!(entry.due_at, Some(1000 + delay));
        assert_eq!(entry.streak, index as u32 + 1);
    }
    entry.review(false, 2000);
    assert_eq!(
        (entry.streak, entry.reviews, entry.due_at),
        (0, 14, Some(2300))
    );
    for _ in 0..20 {
        entry.review(false, 3000);
    }
    assert_eq!(
        (entry.streak, entry.reviews, entry.due_at),
        (0, 0, Some(3300))
    );
    entry.review(true, u64::MAX);
    assert_eq!(entry.due_at, Some(u64::MAX));
}

#[test]
fn invalid_or_unrepresentable_saved_dates_stay_unknown() {
    assert_eq!(date(None), "Unknown");
    assert_eq!(date(Some(u64::MAX)), "Unknown");
    assert_eq!(date(Some(67_768_036_160_140_800)), "Unknown");
    let stamp = date(Some(1_740_777_900));
    assert!(stamp.starts_with("2025-"));
    assert_eq!(stamp.len(), 16);
}

#[test]
fn repeated_lookups_preserve_supplied_context_and_reset_the_review_streak() {
    let mut vocabulary = Vocabulary::default();
    let entry = Entry {
        book: "Coast".into(),
        before: Some("Along the".into()),
        after: Some("at dusk".into()),
        ..Default::default()
    };
    vocabulary.remember("shore", entry, 100);
    let saved = vocabulary.words.get_mut("shore").unwrap();
    for _ in 0..3 {
        saved.review(true, 200);
    }
    vocabulary.remember(
        "shore",
        Entry {
            book: "Other book".into(),
            after: Some("after rain".into()),
            ..Default::default()
        },
        400,
    );
    let saved = &vocabulary.words["shore"];
    assert_eq!(
        (
            saved.reviews,
            saved.streak,
            saved.created_at,
            saved.reviewed_at,
            saved.due_at
        ),
        (2, 0, Some(400), Some(200), Some(700))
    );
    assert_eq!(saved.before.as_deref(), Some("Along the"));
    assert_eq!(saved.after.as_deref(), Some("after rain"));
    assert_eq!(saved.book, "Other book");
    vocabulary.remember("Shore", Entry::default(), 400);
    assert_eq!(vocabulary.words.len(), 2); // Publisher capitalization remains significant.
}

#[test]
fn older_saved_word_lists_migrate_without_inventing_dates_or_discarding_words() {
    let fixture = Fixture::new();
    let path = fixture.root.join("old-lookups.json");
    let words: Vec<_> = (0..600).map(|i| format!("word{i}")).collect();
    fs::write(
        &path,
        serde_json::to_vec(&serde_json::json!({"vocabulary": words})).unwrap(),
    )
    .unwrap();
    let history = LookupHistory::load(&path);
    assert_eq!(
        (history.vocabulary.len(), history.review.words.len()),
        (600, 600)
    );
    assert!(!history.review.automatic);
    assert!(history.review.with_context);
    assert!(
        history
            .review
            .words
            .values()
            .all(|e| e.created_at.is_none() && e.due_at.is_none())
    );
    history.save(&path).unwrap();
    assert_eq!(LookupHistory::load(&path).review, history.review);
}

#[test]
fn vocabulary_filters_due_dates_sources_and_unicode_words_independently() {
    let mut data = Vocabulary::default();
    for (word, book, due) in [
        ("Écume", "Coast", 500),
        ("shore", "Coast", 200),
        ("Shore", "River", 700),
    ] {
        data.words.insert(
            word.into(),
            Entry {
                book: book.into(),
                due_at: Some(due),
                ..Default::default()
            },
        );
    }
    assert_eq!(data.visible("ÉC", 600)[0].0, "Écume");
    data.due_only = true;
    assert_eq!(
        data.visible("", 600)
            .iter()
            .map(|(w, _)| w.as_str())
            .collect::<Vec<_>>(),
        ["Écume", "shore"]
    );
    data.excluded_books.insert("Coast".into());
    assert!(data.visible("", 600).is_empty());
    data.due_only = false;
    assert_eq!(data.visible("SHORE", 600)[0].0, "Shore");
}

#[test]
fn normal_vocabulary_controls_review_undo_confirm_and_resume_the_same_passage() {
    let fixture = Fixture::new();
    let (mut app, fonts) = reader(&fixture);
    let active = app.active.as_mut().unwrap();
    active.reading.jump(1, Some("door")).unwrap();
    let start = active.reading.probe.fragment("door").unwrap();
    let mut end = start.clone();
    end.offset += "At dusk".len();
    app.selection = Some(active.reading.probe.select_range(&start, &end).unwrap());
    app.record_position();
    let at = app.current_position().unwrap();
    let entry = app.vocabulary_entry("dusk");
    assert_eq!(entry.highlight.as_deref(), Some("At dusk"));
    assert!(
        entry
            .after
            .as_ref()
            .is_some_and(|s| s.starts_with("the door"))
    );
    app.update_lookup_history(|h| {
        h.save_word("dusk");
        h.review.remember("dusk", entry, 1_740_777_900);
    })
    .unwrap();
    app.show(Screen::Vocabulary);
    capture_test_screen(&app, &fonts, "READER_VOCABULARY_LIST_SCREENSHOT", false);
    choose(&mut app, "dusk");
    capture_test_screen(&app, &fonts, "READER_VOCABULARY_REVIEW_SCREENSHOT", false);
    capture_test_screen(
        &app,
        &fonts,
        "READER_VOCABULARY_REVIEW_DARK_SCREENSHOT",
        true,
    );
    choose(&mut app, "Reading context");
    capture_test_screen(&app, &fonts, "READER_VOCABULARY_CONTEXT_SCREENSHOT", false);
    let back = primitive_ui::back_button(primitive_app::WIDTH as i32);
    assert_eq!(
        app.input(Input::Tap((back.x + 20) as f32, (back.y + 20) as f32)),
        Change::Partial
    );
    assert_eq!(app.screen, Screen::VocabularyWord);
    let prior = app.lookup_history.review.words["dusk"].clone();
    choose(&mut app, "Got it");
    assert_eq!(app.lookup_history.review.words["dusk"].streak, 1);
    choose(&mut app, "Undo last review");
    assert_eq!(app.lookup_history.review.words["dusk"], prior);
    choose(&mut app, "Forgot");
    assert_eq!(app.lookup_history.review.words["dusk"].streak, 0);
    choose(&mut app, "Show definition");
    assert_eq!(app.screen, Screen::Definition);
    let (_, close, _) = app.definition_layout();
    assert_eq!(
        app.input(Input::Tap((close.x + 10) as f32, (close.y + 10) as f32)),
        Change::Partial
    );
    assert_eq!(app.screen, Screen::VocabularyWord);
    choose(&mut app, "Remove word");
    capture_test_screen(&app, &fonts, "READER_VOCABULARY_CONFIRM_SCREENSHOT", true);
    assert_eq!(app.input(Input::Tap(20.0, 200.0)), Change::Partial);
    assert!(app.lookup_history.review.words.contains_key("dusk"));
    choose(&mut app, "Remove word");
    assert_eq!(
        app.input(Input::Tap((CONFIRM.x + 25) as f32, (CONFIRM.y + 25) as f32)),
        Change::Partial
    );
    assert!(!app.lookup_history.review.words.contains_key("dusk"));
    assert!(app.lookup_history.vocabulary.is_empty());
    assert_eq!(app.current_position(), Some(at.clone()));
    app.save().unwrap();
    let mut reopened = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
    reopened.activate(Action::Continue).unwrap();
    assert_eq!(reopened.current_position(), Some(at));
}

#[test]
fn automatic_recording_is_opt_in_and_definition_review_does_not_reset_progress() {
    let fixture = Fixture::new();
    let (mut app, _) = reader(&fixture);
    app.look_up("shore").unwrap();
    assert!(app.lookup_history.review.words.is_empty());
    app.vocabulary_command(Command::ToggleAutomatic).unwrap();
    app.look_up("shore").unwrap();
    assert_eq!(app.lookup_history.review.words.len(), 1);
    app.vocabulary_command(Command::Open("shore".into()))
        .unwrap();
    app.vocabulary_command(Command::Review(true)).unwrap();
    let previous = app.lookup_history.review.words["shore"].clone();
    app.look_up("shore").unwrap();
    assert_eq!(app.lookup_history.review.words["shore"], previous);
    app.look_up("shore").unwrap(); // Nested definition of the same review item.
    assert!(app.vocabulary_view.repeated.is_none());
    assert_eq!(app.lookup_history.review.words["shore"], previous);
    let (popup, _, _) = app.definition_layout();
    let remove = crate::app::dictionary_panel::definition_vocabulary(popup);
    app.input(Input::Tap((remove.x + 15) as f32, (remove.y + 15) as f32));
    assert!(app.vocabulary_view.confirm.is_some());
    app.input(Input::PageBack);
    assert_eq!(app.lookup_history.review.words["shore"], previous);
    let bad_parent = fixture.root.join("not-a-directory");
    fs::write(&bad_parent, b"x").unwrap();
    app.lookup_history_path = bad_parent.join("lookup.json");
    assert!(app.vocabulary_command(Command::Review(false)).is_err());
    assert_eq!(app.lookup_history.review.words["shore"], previous);
}

#[test]
fn repeat_lookup_requires_a_choice_and_keeps_progress_when_cancelled() {
    let fixture = Fixture::new();
    let (mut app, _) = reader(&fixture);
    app.vocabulary_command(Command::ToggleAutomatic).unwrap();
    app.look_up("shore").unwrap();
    app.review_vocabulary_word("shore", true).unwrap();
    app.show(Screen::Book);
    let prior = app.lookup_history.review.words["shore"].clone();
    app.look_up("shore").unwrap();
    assert!(app.vocabulary_view.repeated.is_some());
    assert_eq!(app.lookup_history.review.words["shore"], prior);
    assert_eq!(app.input(Input::PageBack), Change::Partial);
    assert_eq!(app.lookup_history.review.words["shore"], prior);
    app.show(Screen::Book);
    app.look_up("shore").unwrap();
    let r = EXIST_CHOICES[2];
    assert_eq!(
        app.input(Input::Tap((r.x + 20) as f32, (r.y + 20) as f32)),
        Change::Partial
    );
    assert!(app.vocabulary_view.repeated.is_none());
    assert_eq!(app.lookup_history.review.words["shore"].streak, 0);
    assert_eq!(app.lookup_history.review.words["shore"].reviews, 0);
    assert!(!app.vocabulary_view.undo.contains_key("shore"));
}

#[test]
fn each_review_keeps_its_undo_and_resets_only_the_confirmed_scope() {
    let fixture = Fixture::new();
    let (mut app, _) = reader(&fixture);
    app.update_lookup_history(|h| {
        for word in ["shore", "coast"] {
            h.save_word(word);
            h.review.remember(word, Entry::default(), 100);
        }
    })
    .unwrap();
    let prior = app.lookup_history.review.words.clone();
    app.review_vocabulary_word("shore", true).unwrap();
    app.review_vocabulary_word("coast", true).unwrap();
    app.vocabulary_command(Command::Open("shore".into()))
        .unwrap();
    choose(&mut app, "Undo last review");
    assert_eq!(app.lookup_history.review.words["shore"], prior["shore"]);
    assert_eq!(app.lookup_history.review.words["coast"].streak, 1);
    app.vocabulary_command(Command::Open("coast".into()))
        .unwrap();
    choose(&mut app, "Reset this word's progress");
    app.input(Input::Sleep);
    assert!(!app.stats_awake);
    app.input(Input::Wake);
    assert!(app.stats_awake);
    let tap = Input::Tap((CONFIRM.x + 20) as f32, (CONFIRM.y + 20) as f32);
    assert_eq!(app.input(tap), Change::Partial);
    assert_eq!(app.lookup_history.review.words["coast"].streak, 0);
    assert_eq!(app.lookup_history.review.words["shore"], prior["shore"]);
    app.show(Screen::VocabularyOptions);
    choose(&mut app, "Reset all review progress");
    app.input(tap);
    assert!(
        app.lookup_history
            .review
            .words
            .values()
            .all(|e| e.reviews == 0 && e.due_at.unwrap() <= now())
    );
    assert_eq!(app.lookup_history.review.words.len(), 2);
}

#[test]
fn context_segmentation_bounds_unicode_and_unbroken_tokens() {
    let text = "沿岸の小さな灯台では静かな夜に波の音を聞きながら本を読む。".repeat(500);
    for end in [false, true] {
        let context = crate::selection::bounded_context(&text, 15, end);
        assert!(!context.is_empty());
        assert!(context.chars().count() <= 512);
        assert!(crate::selection::word_ranges(&context).count() <= 15);
        let token = "é".repeat(10_000);
        assert_eq!(
            crate::selection::bounded_context(&token, 15, end)
                .chars()
                .count(),
            512
        );
    }
    assert_eq!(
        crate::selection::bounded_context("one, two! three", 2, false),
        "one, two"
    );
    assert_eq!(
        crate::selection::bounded_context("one, two! three", 2, true),
        "two! three"
    );
}

#[test]
fn context_view_turns_with_buttons_swipes_and_returns_to_the_word() {
    let fixture = Fixture::new();
    let (mut app, fonts) = reader(&fixture);
    app.update_lookup_history(|h| {
        h.vocabulary.push("source".into());
        h.review.words.insert(
            "source".into(),
            Entry {
                before: Some("m".repeat(512)),
                after: Some("m".repeat(512)),
                ..Default::default()
            },
        );
    })
    .unwrap();
    app.vocabulary_command(Command::Open("source".into()))
        .unwrap();
    choose(&mut app, "Reading context");
    let paths: Vec<_> = fonts.iter().map(|p| p.display().to_string()).collect();
    let mut canvas = Canvas::new(primitive_app::WIDTH, primitive_app::HEIGHT, &paths).unwrap();
    primitive_app::frame(&app, &mut canvas, false);
    assert!(app.vocabulary_view.text_pages.get() > 1);
    assert_eq!(app.input(Input::PageForward), Change::Partial);
    assert_eq!(app.vocabulary_view.text_page, 1);
    assert_eq!(
        app.input(Input::Swipe {
            dx: 180.0,
            dy: 0.0,
            start_y: 700.0
        }),
        Change::Partial
    );
    assert_eq!(app.vocabulary_view.text_page, 0);
    let back = primitive_ui::back_button(primitive_app::WIDTH as i32);
    app.input(Input::Tap((back.x + 20) as f32, (back.y + 20) as f32));
    assert_eq!(app.screen, Screen::VocabularyWord);
}

#[test]
#[ignore = "requires the owner's cached, unchanged EPUB and PDF documents"]
fn vocabulary_context_and_reviews_preserve_original_documents_and_resume() {
    let fixture = Fixture::new();
    let root =
        PathBuf::from(std::env::var_os("READER_VOCABULARY_ORIGINAL_ROOT").expect("original root"));
    let list = fs::read_to_string(
        std::env::var_os("READER_VOCABULARY_ORIGINAL_LIST").expect("original list"),
    )
    .unwrap();
    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut app = Reader::open(&root, &fixture.state, fonts.clone()).unwrap();
    let mut count = 0;
    for filename in list.lines() {
        let path = root.join(filename);
        let bytes = fs::read(&path).unwrap();
        app.open_path(&path).unwrap();
        let mut word = None;
        if let Some(active) = &mut app.active {
            for chapter in 0..active.reading.probe.chapter_count().min(8) {
                active.reading.jump(chapter, None).unwrap();
                let probe = &active.reading.probe;
                let selected = probe.source.blocks.iter().find_map(|block| {
                    let range = crate::selection::word_ranges(&block.text).find(|r| {
                        let s = &block.text[r.clone()];
                        (2..=128).contains(&s.chars().count()) && s.chars().any(char::is_alphabetic)
                    })?;
                    let start = probe.anchor(block.key, range.start);
                    let end = probe.anchor(block.key, range.end);
                    probe.select_range(&start, &end)
                });
                if let Some(selected) = selected {
                    word = Some(selected.text.clone());
                    app.selection = Some(selected);
                    break;
                }
            }
        } else if let Some(fixed) = &mut app.fixed {
            for page in 0..fixed.reading.count().min(8) {
                let layer = fixed.reading.page_text(page).unwrap();
                let selected = layer.words.iter().find(|w| {
                    let s = &layer.text[w.bytes.clone()];
                    (2..=128).contains(&s.chars().count()) && s.chars().any(char::is_alphabetic)
                });
                if let Some(selected) = selected {
                    word = Some(layer.text[selected.bytes.clone()].to_owned());
                    let b = selected.bounds;
                    app.pdf_selection = crate::fixed_session::PdfSelection::at(
                        page,
                        hayro::kurbo::Point::new(
                            f64::from((b.x0 + b.x1) / 2.0),
                            f64::from((b.y0 + b.y1) / 2.0),
                        ),
                        layer,
                    );
                    assert!(app.pdf_selection.is_some());
                    break;
                }
            }
        }
        let word = word.expect("readable source word");
        if let Some(selected) = app.pdf_selection.take() {
            app.follow_fixed(selected.page(), None).unwrap();
            app.pdf_selection = Some(selected);
        }
        app.record_position();
        let position = app.current_position().unwrap();
        let entry = app.vocabulary_entry(&word);
        assert!(entry.before.is_some() || entry.after.is_some());
        assert!(!entry.book.is_empty());
        assert_eq!(entry.highlight.as_deref(), Some(word.as_str()));
        let before = entry.before.clone();
        let after = entry.after.clone();
        app.update_lookup_history(|h| {
            if !h.vocabulary.contains(&word) {
                h.vocabulary.insert(0, word.clone());
            }
            h.review.remember(&word, entry, now());
        })
        .unwrap();
        app.vocabulary_command(Command::Open(word.clone())).unwrap();
        choose(&mut app, "Got it");
        choose(&mut app, "Undo last review");
        app.save().unwrap();
        let mut reopened = Reader::open(&root, &fixture.state, fonts.clone()).unwrap();
        assert_eq!(reopened.lookup_history.review.words[&word].before, before);
        assert_eq!(reopened.lookup_history.review.words[&word].after, after);
        reopened.activate(Action::Continue).unwrap();
        assert_eq!(reopened.current_position(), Some(position));
        assert_eq!(fs::read(&path).unwrap(), bytes);
        count += 1;
    }
    assert!(count >= 4);
}

#[test]
#[ignore = "requires vectors produced by the pinned upstream Lua scheduling method"]
fn upstream_vocabulary_schedule_vectors_match() {
    let path =
        std::env::var_os("READER_VOCABULARY_REFERENCE").expect("READER_VOCABULARY_REFERENCE");
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert!(rows.len() > 2000);
    for row in rows {
        let mut entry = Entry {
            reviews: row[0].as_u64().unwrap() as u32,
            streak: row[1].as_u64().unwrap() as u32,
            ..Default::default()
        };
        let got = row[2].as_bool().unwrap();
        let time = row[3].as_u64().unwrap();
        entry.review(got, time);
        assert_eq!(
            (
                entry.reviews as u64,
                entry.streak as u64,
                entry.due_at.unwrap()
            ),
            (
                row[4].as_u64().unwrap(),
                row[5].as_u64().unwrap(),
                row[6].as_u64().unwrap()
            )
        );
    }
}
