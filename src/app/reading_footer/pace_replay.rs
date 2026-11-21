use super::*;
use crate::app::tests::{Fixture, capture_test_screen};
use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};

fn visit(app: &mut Reader, chapter: usize, page: usize) {
    if let Some(fixed) = &mut app.fixed {
        fixed.reading.jump(page).unwrap();
    } else {
        app.active
            .as_mut()
            .unwrap()
            .reading
            .go_to_page(chapter, page)
            .unwrap();
    }
}

#[test]
fn replays_reading_pauses_rereads_and_reopens_the_measured_footer() {
    let fixture = Fixture::new();
    let external = std::env::var_os("READER_ETA_DOCUMENT").map(PathBuf::from);
    let authored = external.is_none();
    let path = external.unwrap_or_else(|| {
        let file = fixture.root.join("Coastal observations.epub");
        fs::write(
            &file,
            include_bytes!("../../../tests/fixtures/long-chapter.epub"),
        )
        .unwrap();
        file
    });
    let index = |key, default| {
        std::env::var(key)
            .ok()
            .map_or(default, |s| s.parse::<usize>().unwrap() - 1)
    };
    let chapter = index("READER_ETA_CHAPTER", 0);
    let page = index("READER_ETA_PAGE", 2);
    let font = crate::test_fixture("fonts/NotoSans-Regular.ttf");
    let mut app = Reader::open(path.parent().unwrap(), &fixture.state, vec![font.clone()]).unwrap();
    app.open_path(&path).unwrap();
    visit(&mut app, chapter, page);
    let id = app
        .fixed
        .as_ref()
        .map(|b| b.id)
        .or_else(|| app.active.as_ref().map(|b| b.id))
        .unwrap();
    let legacy = serde_json::json!({"version":1,"seconds_read":{id.to_string():3600}});
    fs::write(&app.stats_path, serde_json::to_vec(&legacy).unwrap()).unwrap();
    app.stats = crate::reading_stats::ReadingStats::load(&app.stats_path);
    app.pace.storage_returned();
    let clock = Instant::now();
    let at = |s| clock + Duration::from_secs(s);
    app.update_stats_activity(at(0));
    let reading = app.screen;
    app.screen = if app.fixed.is_some() {
        Screen::FixedTools
    } else {
        Screen::BookTools
    };
    app.update_stats_activity(at(10)); // Page still visible for 10 s before opening tools.
    app.screen = reading;
    app.update_stats_activity(at(1010)); // Menu time excluded.
    app.stats_awake = false;
    app.update_stats_activity(at(1015));
    app.stats_awake = true;
    app.update_stats_activity(at(2015)); // Sleep time excluded.
    visit(&mut app, chapter, page + 1);
    app.update_stats_activity(at(2020)); // 20 s on the first page.
    visit(&mut app, chapter, page + 2);
    app.update_stats_activity(at(2060)); // 40 s on the next.
    visit(&mut app, chapter, page + 1);
    app.update_stats_activity(at(2062)); // A two-second skim.
    visit(&mut app, chapter, page + 2);
    app.update_stats_activity(at(2072)); // 10 s rereading the second page.
    app.pace.checkpoint(at(2072)).unwrap();
    assert_eq!(app.pace.rate(id), 35.0);
    assert_eq!(app.stats.seconds_read(id, at(2072)), 3672);
    app.stats
        .checkpoint(&app.stats_path, at(2072), true)
        .unwrap();

    let progress = app.footer_progress().unwrap();
    let (chapter_left, book_left, approximate) = (
        progress.pages_left(),
        progress.book_pages_left,
        progress.future_pages_estimated,
    );
    assert!(book_left >= chapter_left);
    let expected = |pages: usize, unit: &str, approximate: bool| {
        let seconds = pages * 35;
        let estimate = match seconds {
            0 => "0 min".into(),
            1..60 => "Under 1 min".into(),
            60..3600 => format!("{} min", seconds / 60),
            _ => format!("{} hr {} min", seconds / 3600, seconds / 60 % 60),
        };
        format!(
            "{}{estimate} left in {unit}",
            if approximate && seconds >= 60 {
                "About "
            } else {
                ""
            }
        )
    };
    // An opt-in stock comparison locates this same EPUB passage in CREngine.
    // Passages exist only inside the caller's private scratch directory.
    if let (Some(active), Some(path)) = (
        &app.active,
        std::env::var_os("READER_ETA_REFERENCE_PASSAGES"),
    ) {
        let probe = &active.reading.probe;
        let frame = probe.pages.range(active.reading.page).unwrap();
        let mut snippets = Vec::new();
        for line in probe
            .lines
            .iter()
            .filter(|line| line.top < frame.end && line.bottom > frame.start)
        {
            let index = probe
                .source
                .blocks
                .binary_search_by_key(&line.block, |block| block.key)
                .unwrap();
            let words: Vec<_> = probe.source.blocks[index].text[line.bytes.clone()]
                .split_whitespace()
                .take(10)
                .collect();
            if words.len() >= 4 {
                snippets.push(words.join(" "));
            }
        }
        assert!(!snippets.is_empty());
        fs::write(path, snippets.join("\n")).unwrap();
    }
    app.controls.footer_all_at_once = false;
    app.controls.status_bar = StatusBar::ChapterTime;
    assert_eq!(
        app.status_caption().as_deref(),
        Some(expected(chapter_left, "chapter", false).as_str())
    );
    if authored {
        capture_test_screen(
            &app,
            std::slice::from_ref(&font),
            "READER_ETA_CHAPTER_SCREEN",
            false,
        );
    }
    app.controls.status_bar = StatusBar::BookTime;
    assert_eq!(
        app.status_caption().as_deref(),
        Some(expected(book_left, "book", approximate).as_str())
    );
    if authored {
        capture_test_screen(
            &app,
            std::slice::from_ref(&font),
            "READER_ETA_BOOK_SCREEN",
            false,
        );
    }
    app.controls.footer_all_at_once = true;
    app.controls.footer_combined_time = Some(ProgressScope::Chapter);
    assert!(app.status_caption().unwrap().contains("left in chapter"));
    app.pace.storage_returned();
    app.update_stats_activity(at(2080));
    assert_eq!(app.pace.rate(id), 35.0);
    let saved = crate::reading_stats::ReadingStats::load(&app.stats_path);
    assert_eq!(saved.seconds_read(id, at(3000)), 3672);
    if authored {
        let book = &app.active.as_ref().unwrap().reading;
        let last_spine = book.probe.chapter_count() - 1;
        visit(&mut app, last_spine, 0);
        let last_leaf = app.active.as_ref().unwrap().reading.probe.page_count() - 1;
        visit(&mut app, last_spine, last_leaf);
        app.controls.footer_all_at_once = false;
        app.controls.status_bar = StatusBar::ChapterTime;
        assert_eq!(
            app.status_caption().as_deref(),
            Some("0 min left in chapter")
        );
        app.controls.status_bar = StatusBar::BookTime;
        assert_eq!(app.status_caption().as_deref(), Some("0 min left in book"));
    }
    if let Some(out) = std::env::var_os("READER_ETA_REPORT") {
        let value = serde_json::json!({"rate_seconds":35,"legacy_seconds":3600,"active_seconds":72,"chapter_remaining_layout_pages":chapter_left,"book_remaining_layout_pages":book_left,"chapter_seconds":chapter_left*35,"book_seconds":book_left*35,"weighted_spine":approximate});
        fs::write(out, serde_json::to_vec(&value).unwrap()).unwrap();
    }
}
