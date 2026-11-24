use super::*;
use crate::app::selection_viewport_tests::{adopt, capture, tap};
use crate::app::tests::{Fixture, tap_contents_item};
use crate::generated_contents::OutlineTarget;
use crate::session::ReadingViewport;
use std::{fs, path::Path};

fn open(fixture: &Fixture, path: &Path) -> Reader {
    let fonts = ["fonts/NotoSans-Regular.ttf", "fonts/NotoSans-Bold.ttf"]
        .into_iter()
        .map(crate::test_fixture)
        .collect();
    let mut reader = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
    reader.open_path(path).unwrap();
    reader
}

fn complete(reader: &mut Reader) {
    for _ in 0..8 {
        let Some(deadline) = reader.alternative_contents_deadline() else {
            return;
        };
        assert!(reader.poll_alternative_contents(deadline).is_some());
    }
    panic!("The sample alternative contents did not complete");
}

fn prompt(reader: &mut Reader) {
    reader.show(Screen::ContentsSettings);
    tap_contents_item(reader, "Alternative table of contents");
}

fn apply(reader: &mut Reader) {
    let at = reader.navigation_layout().centered(APPLY);
    tap(reader, at);
}

#[test]
fn alternative_contents_switches_normal_contents_and_preserves_anchor_across_reopen() {
    let asset = "alternative-contents.epub";
    let fixture = Fixture::new();
    let path = fixture.root.join(asset);
    fs::copy(crate::test_fixture(asset), &path).unwrap();
    let mut reader = open(&fixture, &path);
    let origin = reader.current_history_location().unwrap();
    let id = reader.open_book_id().unwrap();
    let state = reader.library.state_mut(id).unwrap();
    state.contents_depths.toggle(2);
    state.contents_depths.bind_navigation = true;
    let origin_history = reader.back.clone();
    prompt(&mut reader);
    assert_eq!(reader.alternative_contents.confirm, Some(true));
    let cancel = reader.navigation_layout().centered(CANCEL);
    tap(&mut reader, cancel);
    assert!(!reader.uses_alternative_contents());
    assert!(
        reader
            .library
            .state(id)
            .unwrap()
            .contents_depths
            .has_ignored()
    );

    prompt(&mut reader);
    let mut wide = adopt(&mut reader, ReadingViewport::LANDSCAPE, 1);
    capture(
        &reader,
        &mut wide,
        "alternative-contents-landscape-confirmation.png",
        false,
    );
    apply(&mut reader);
    assert_eq!(reader.screen, Screen::Contents);
    complete(&mut reader);
    let levels = &reader.library.state(id).unwrap().contents_depths;
    assert!(!levels.has_ignored());
    assert!(levels.bind_navigation);
    assert_eq!(
        reader.current_history_location().unwrap().position.anchor,
        origin.position.anchor
    );
    assert_eq!(reader.back, origin_history);
    let rows = reader.alternative_entries(id).unwrap();
    assert_eq!(
        rows.iter()
            .map(|row| (row.title.as_str(), row.depth))
            .collect::<Vec<_>>(),
        [
            ("Harbor Survey", 0),
            ("–", 1),
            ("East softhyphen quay", 2),
            ("West quay", 1)
        ]
    );
    assert_eq!(rows[1].target, rows[2].target);
    assert!(matches!(&rows[2].target, OutlineTarget::Heading(1, _)));
    let target = rows[2].target.clone();
    capture(
        &reader,
        &mut wide,
        "alternative-contents-landscape.png",
        false,
    );
    reader
        .activate(Action::AlternativeContentsEntry(target.clone()))
        .unwrap();
    assert_eq!(reader.screen, Screen::Book);
    assert_eq!(
        reader.current_history_location().unwrap().position.chapter,
        1
    );
    assert_eq!(reader.back.len(), origin_history.len() + 1);
    reader.activate(Action::ReturnToLocation).unwrap();
    assert_eq!(
        reader.current_history_location().unwrap().position.anchor,
        origin.position.anchor
    );

    let mut reopened = open(&fixture, &path);
    assert!(reopened.uses_alternative_contents());
    complete(&mut reopened);
    assert_eq!(reopened.alternative_entries(id).unwrap()[2].target, target);
    let state = reopened.library.state_mut(id).unwrap();
    state.custom_contents.enabled = true;
    assert!(!reopened.uses_alternative_contents());
    assert!(matches!(
        reopened.alternative_contents_setting().unwrap().action,
        Action::Heading
    ));
    reopened
        .library
        .state_mut(id)
        .unwrap()
        .custom_contents
        .enabled = false;
    prompt(&mut reopened);
    assert_eq!(reopened.alternative_contents.confirm, Some(false));
    apply(&mut reopened);
    assert!(!reopened.uses_alternative_contents());
    let (_, rows) = reopened.contents_items(0, 10);
    assert!(rows.iter().any(|row| row.title == "Publisher voyage"));
    assert!(!open(&fixture, &path).uses_alternative_contents());
}

#[test]
fn alternative_contents_uses_fragment_starts_only_without_document_headings() {
    let asset = "alternative-contents-fragments.epub";
    let fixture = Fixture::new();
    let path = fixture.root.join(asset);
    fs::copy(crate::test_fixture(asset), &path).unwrap();
    let mut reader = open(&fixture, &path);
    prompt(&mut reader);
    apply(&mut reader);
    complete(&mut reader);
    let rows = reader
        .alternative_entries(reader.open_book_id().unwrap())
        .unwrap();
    assert_eq!(rows.len(), 3);
    for (index, row) in rows.iter().enumerate() {
        assert_eq!((row.title.as_str(), row.depth), ("–", 0));
        assert_eq!(row.target, OutlineTarget::Chapter(index));
    }
    reader
        .jump_chapter(super::super::chapter_navigation::Direction::Next)
        .unwrap();
    assert_eq!(reader.current_position().unwrap().chapter, 1);
}
