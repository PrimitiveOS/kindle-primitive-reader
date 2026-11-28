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
            .map(|row| (row.display_title(), row.depth))
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
    let parent = reader.contents_branch_button(1).unwrap().0;
    tap(&mut reader, parent);
    let intermediate = reader.contents_branch_button(2).unwrap().0;
    tap(&mut reader, intermediate);
    capture(
        &reader,
        &mut wide,
        "alternative-contents-landscape.png",
        false,
    );
    let (_, items) = reader.contents_items(0, reader.contents_rows());
    let row = items
        .iter()
        .position(|item| item.title == "East softhyphen quay")
        .unwrap();
    let heading = reader.contents_row_rect(row);
    tap(&mut reader, heading);
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

/// Reflow chapter jumps are applied by a worker; wait until it has finished.
fn settle_chapter_jump(reader: &mut Reader) {
    let stop = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while reader.chapter_navigation_job.is_some() {
        assert!(std::time::Instant::now() < stop, "chapter jump did not finish");
        let due = reader
            .chapter_navigation_job
            .as_ref()
            .map_or_else(std::time::Instant::now, |job| job.deadline());
        reader.poll_chapter_navigation(due + std::time::Duration::from_secs(1));
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
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
        assert_eq!((row.display_title(), row.depth), ("–", 0));
        assert_eq!(row.target, OutlineTarget::Chapter(index));
    }
    reader
        .jump_chapter(super::super::chapter_navigation::Direction::Next)
        .unwrap();
    settle_chapter_jump(&mut reader);
    assert_eq!(reader.current_position().unwrap().chapter, 1);
}

fn long_marker_fixture(path: &Path) {
    use std::io::{Read, Write};
    let asset = crate::test_fixture("alternative-contents.epub");
    let mut source = zip::ZipArchive::new(fs::File::open(asset).unwrap()).unwrap();
    let mut target = zip::ZipWriter::new(fs::File::create(path).unwrap());
    for index in 0..source.len() {
        let mut file = source.by_index(index).unwrap();
        let mut text = String::new();
        file.read_to_string(&mut text).unwrap();
        if file.name() == "part1.xhtml" {
            let passage =
                "<p>The coastal survey crew logged a detailed harbor observation.</p>".repeat(94);
            text = text.replace("<h3>", &format!("{passage}<h3>"));
            text = text.replace("<h2>", &format!("{passage}<h2>"));
        }
        target
            .start_file(file.name(), zip::write::SimpleFileOptions::default())
            .unwrap();
        target.write_all(text.as_bytes()).unwrap();
    }
    target.finish().unwrap();
}

#[test]
fn alternative_markers_resolve_cross_spine_heading_pages_for_header_footer_and_skim() {
    use super::super::chapter_navigation::Direction;
    use super::super::progress_markers::MarkerTrack;
    use crate::reading_controls::{FooterItem, StatusBar};
    use primitive_app::App;
    use std::time::Duration;
    let fixture = Fixture::new();
    let path = fixture.root.join("Long Harbor Survey.epub");
    long_marker_fixture(&path);
    let mut reader = open(&fixture, &path);
    reader.activate(Action::TopStatusHeader(true)).unwrap();
    reader.controls.top_header.clock = false;
    prompt(&mut reader);
    apply(&mut reader);
    complete(&mut reader);
    let mut wide = adopt(&mut reader, ReadingViewport::LANDSCAPE, 1);
    reader.show(Screen::Book);
    let original = reader.current_history_location();
    let history = reader.back.clone();
    let id = reader.open_book_id().unwrap();
    let entries = reader.alternative_entries(id).unwrap();
    assert_eq!(entries.len(), 4);
    let reading = &reader.active.as_ref().unwrap().reading;
    let local = reading.probe.page_count();
    let remote = reading.config.chapter(1).unwrap();
    let ending = reading.config.chapter(2).unwrap().page_count();
    let wanted: Vec<_> = [0, 2, 3]
        .map(|index| {
            let OutlineTarget::Heading(1, anchor) = &entries[index].target else {
                panic!("remote heading")
            };
            remote.page_for(anchor).unwrap()
        })
        .into();
    assert!(wanted.windows(2).all(|pair| pair[0] < pair[1]));
    let total_pages = local + remote.page_count() + ending;
    let before = reader
        .alternative_header_marker_pages(reader.active.as_ref().unwrap())
        .unwrap();
    assert!(before.starts.is_empty());
    assert_eq!(before.count, local);
    assert!(reader.needs_screen_page_count());
    let expiry = Instant::now() + Duration::from_secs(20);
    let expected_header: Vec<_> = wanted.iter().map(|page| local + page).collect();
    assert_eq!(reader.active_alternative_chapter_title(), Some(""));
    assert!(reader.alternative_chapter_progress().is_none());
    let (start, span, total) = {
        let weights = reading.progress_weights();
        (
            weights[1] as f64,
            (weights[2] - weights[1]) as f64,
            weights[3] as f64,
        )
    };
    let count = remote.page_count();
    loop {
        let markers = reader
            .alternative_header_marker_pages(reader.active.as_ref().unwrap())
            .unwrap();
        if markers.count == total_pages {
            assert_eq!(markers.starts, expected_header);
            break;
        }
        assert!(
            Instant::now() < expiry,
            "whole book marker request did not finish"
        );
        reader.tick(Instant::now());
        std::thread::sleep(Duration::from_millis(15));
    }
    for (track, denominator) in [(MarkerTrack::Footer, count), (MarkerTrack::Skim, count - 1)] {
        let markers = reader.alternative_book_marker_fractions(track).unwrap();
        assert_eq!(markers.len(), wanted.len());
        for (fraction, page) in markers.into_iter().zip(&wanted) {
            let expected = (start + span * *page as f64 / denominator as f64) / total;
            assert!((fraction - expected).abs() < 1e-9);
        }
    }
    assert_eq!(reader.current_history_location(), original);
    let first_section = format!("Chapter 1 of 4 - Page 1 of {}", expected_header[0]);
    assert_eq!(
        reader.status_caption().as_deref(),
        Some(first_section.as_str())
    );
    let track = reader.footer_track_rect(0);
    capture(
        &reader,
        &mut wide,
        "alternative-progress-reading-landscape.png",
        false,
    );
    reader.activate(Action::Skim).unwrap();
    capture(
        &reader,
        &mut wide,
        "alternative-progress-skim-landscape.png",
        false,
    );
    assert_eq!(reader.current_history_location(), original);
    assert_eq!(reader.back, history);
    reader.show(Screen::Book);
    reader.activate(Action::Chapter(Direction::Next)).unwrap();
    settle_chapter_jump(&mut reader);
    assert_eq!(
        reader.active_alternative_chapter_title(),
        Some("Harbor Survey")
    );
    reader.activate(Action::Chapter(Direction::Next)).unwrap();
    settle_chapter_jump(&mut reader);
    assert_eq!(reader.active_alternative_chapter_title(), Some(""));
    let after_jumps = reader.current_history_location();
    let checked_history = reader.back.clone();
    let east_pages = wanted[2] - wanted[1];
    assert_eq!(
        reader.status_caption(),
        Some(format!("Chapter 3 of 4 - Page 1 of {east_pages}"))
    );
    assert_eq!(reader.footer_track_rect(0), track);
    capture(
        &reader,
        &mut wide,
        "alternative-section-count-landscape.png",
        false,
    );
    reader.show(Screen::ChapterNavigation);
    tap_contents_item(&mut reader, "Contents level 2");
    tap_contents_item(&mut reader, "Chapter titles from ticks only");
    assert_eq!(
        reader.active_alternative_chapter_title(),
        Some("East softhyphen quay")
    );
    assert_eq!(
        reader.status_caption(),
        Some(format!("Chapter 3 of 4 - Page 1 of {east_pages}"))
    );
    reader.show(Screen::Book);
    reader.controls.status_bar = StatusBar::Custom;
    reader.controls.footer_items = FooterItem::ChapterTitle.bit();
    assert_eq!(
        reader.status_caption().as_deref(),
        Some("East softhyphen quay")
    );
    assert_eq!(
        reader.export_progress_token('C').as_deref(),
        Some("East softhyphen quay")
    );
    capture(
        &reader,
        &mut wide,
        "alternative-section-title-landscape.png",
        false,
    );
    assert_eq!(reader.current_history_location(), after_jumps);
    assert_eq!(reader.back, checked_history);
    reader.controls.status_bar = StatusBar::Chapter;
    reader.activate(Action::Skim).unwrap();
    capture(
        &reader,
        &mut wide,
        "alternative-section-skim-landscape.png",
        true,
    );
    reader.show(Screen::Book);
    let reading = &mut reader.active.as_mut().unwrap().reading;
    reading.go_to_page(1, wanted[1] + 1).unwrap();
    assert_eq!(
        reader.active_alternative_chapter_title(),
        Some("East softhyphen quay")
    );
    reader.show(Screen::ChapterNavigation);
    tap_contents_item(&mut reader, "Chapter titles from ticks only");
    assert_eq!(
        reader.active_alternative_chapter_title(),
        Some("East softhyphen quay")
    );
    tap_contents_item(&mut reader, "Chapter titles from ticks only");
    tap_contents_item(&mut reader, "Use levels for chapter jumps");
    let reading = &mut reader.active.as_mut().unwrap().reading;
    reading.go_to_page(2, 0).unwrap();
    let only_included = total_pages - expected_header[1];
    assert_eq!(
        reader.active_alternative_chapter_title(),
        Some("East softhyphen quay")
    );
    assert_eq!(
        reader.status_caption(),
        Some(format!(
            "Chapter 3 of 3 - Page {} of {only_included}",
            local + count - expected_header[1] + 1
        ))
    );
    reader.save().unwrap();
    let mut reopened = open(&fixture, &path);
    complete(&mut reopened);
    assert!(reopened.uses_alternative_contents());
    assert!(
        reopened
            .library
            .state(id)
            .unwrap()
            .contents_depths
            .bind_title
    );
    assert!(
        reopened
            .library
            .state(id)
            .unwrap()
            .contents_depths
            .bind_navigation
    );
    let reading = &mut reader.active.as_mut().unwrap().reading;
    reading.go_to_page(0, 0).unwrap();

    // Filter depths before merging headings that share a rendered page.
    let state = reader.library.state_mut(id).unwrap();
    state.contents_depths.toggle(2);
    assert_eq!(
        reader
            .alternative_header_marker_pages(reader.active.as_ref().unwrap())
            .unwrap()
            .starts,
        [local + wanted[0]]
    );
    let state = reader.library.state_mut(id).unwrap();
    state.custom_contents.enabled = true;
    assert!(
        reader
            .alternative_book_marker_fractions(MarkerTrack::Skim)
            .is_none()
    );
    assert!(
        reader
            .alternative_header_marker_pages(reader.active.as_ref().unwrap())
            .is_none()
    );
    reader
        .library
        .state_mut(id)
        .unwrap()
        .custom_contents
        .enabled = false;
    prompt(&mut reader);
    apply(&mut reader);
    assert!(
        reader
            .alternative_book_marker_fractions(MarkerTrack::Skim)
            .is_none()
    );
    let publisher = reader
        .publisher_header_marker_pages(reader.active.as_ref().unwrap())
        .unwrap();
    assert_eq!(publisher.starts, [0, local, local + count]);
    assert_eq!(reader.current_history_location(), original);
}

#[test]
fn alternative_markers_show_fragment_boundaries_on_both_tracks_before_pagination() {
    use super::super::progress_markers::MarkerTrack;
    let fixture = Fixture::new();
    let asset = "alternative-contents-fragments.epub";
    let path = fixture.root.join(asset);
    fs::copy(crate::test_fixture(asset), &path).unwrap();
    let mut reader = open(&fixture, &path);
    prompt(&mut reader);
    apply(&mut reader);
    complete(&mut reader);
    let weights = reader.active.as_ref().unwrap().reading.progress_weights();
    assert_eq!(weights.len(), 4);
    let expected: Vec<_> = weights[..3]
        .iter()
        .map(|n| *n as f64 / weights[3] as f64)
        .collect();
    for track in [MarkerTrack::Footer, MarkerTrack::Skim] {
        assert_eq!(
            reader.alternative_book_marker_fractions(track).unwrap(),
            expected
        );
    }
}
