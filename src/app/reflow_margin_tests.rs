use super::*;
use crate::Settings;
use crate::app::tests::{Fixture, capture_test_screen};
use crate::selection::{HighlightRect, TextSelection};
use std::path::PathBuf;

fn tap_rect(app: &mut Reader, rect: Rect) {
    assert_ne!(
        app.input(Input::Tap(
            (rect.x + rect.w / 2) as f32,
            (rect.y + rect.h / 2) as f32
        )),
        Change::None,
    );
}

fn tap(app: &mut Reader, label: &str) {
    tap_rect(app, app.reading_button(label));
}

fn adjust(app: &mut Reader, edge: &str, step: &str) {
    let bars = app.reading_bars();
    let (row, _) = bars
        .row_labels
        .iter()
        .find(|(_, label)| label.starts_with(&format!("{edge} · ")))
        .unwrap();
    let cell = bars
        .buttons
        .iter()
        .find(|button| {
            button.label == step
                && (row.y..row.y + row.h).contains(&(button.rect.y + button.rect.h / 2))
        })
        .unwrap();
    tap_rect(app, cell.rect);
    assert_eq!(app.reading_panel, Some(BarPanel::TextMargins));
}

fn geometry(app: &Reader) -> (u32, u32, u32, u32, bool) {
    let s = app.active.as_ref().unwrap().reading.config.settings;
    (
        s.left_margin(),
        s.right_margin(),
        s.top_margin(),
        s.bottom_margin(),
        s.link_vertical_margins,
    )
}

fn word(app: &Reader) -> (TextSelection, Vec<HighlightRect>) {
    let b = &app.active.as_ref().unwrap().reading;
    let p = &b.probe;
    for y in (60..p.settings.height as usize - 60).step_by(8) {
        for x in (60..p.settings.width as usize - 60).step_by(13) {
            if let Some(w) = p.word_at(b.page, x as f32, y as f32) {
                let rects = p.selection_rects(b.page, &w);
                if !rects.is_empty() && w.text.chars().filter(|c| c.is_alphanumeric()).count() >= 4
                {
                    return (w, rects);
                }
            }
        }
    }
    panic!("Expected a selectable word in this fixture")
}

#[test]
fn adjusts_each_edge_from_the_page_and_restores_the_same_source_after_reopen() {
    let fixture = Fixture::new();
    let external = std::env::var_os("READER_MARGIN_BOOK").map(PathBuf::from);
    let authored = external.is_none();
    let path = external.unwrap_or_else(|| {
        let path = fixture.root.join("Coastal observations.epub");
        std::fs::write(
            &path,
            include_bytes!("../../tests/fixtures/long-chapter.epub"),
        )
        .unwrap();
        path
    });
    let state = &fixture.state;
    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut app = Reader::open(path.parent().unwrap(), state, fonts.clone()).unwrap();
    app.open_path(&path).unwrap();
    let chapter = std::env::var("READER_MARGIN_CHAPTER")
        .ok()
        .map_or(0, |v| v.parse::<usize>().unwrap() - 1);
    app.active
        .as_mut()
        .unwrap()
        .reading
        .go_to_page(chapter, 0)
        .unwrap();
    app.record_position();
    let anchor = app.current_position().unwrap().anchor.unwrap();
    let initial = geometry(&app);
    assert_eq!(initial, (54, 54, 54, 54, false));
    let (selected, before_rects) = word(&app);
    if let Some(file) = std::env::var_os("READER_MARGIN_PASSAGES") {
        // The comparison uses only temporary, private observations on the device.
        let p = &app.active.as_ref().unwrap().reading.probe;
        let frame = p.pages.range(0).unwrap();
        let mut candidates = Vec::new();
        for line in p
            .lines
            .iter()
            .filter(|l| l.top < frame.end && l.bottom > frame.start)
        {
            let block = p
                .source
                .blocks
                .binary_search_by_key(&line.block, |b| b.key)
                .unwrap();
            let words: Vec<_> = p.source.blocks[block].text[line.bytes.clone()]
                .split_whitespace()
                .take(12)
                .collect();
            if words.len() >= 4 {
                candidates.push(words.join(" "));
            }
        }
        assert!(!candidates.is_empty());
        std::fs::write(file, candidates.join("\n")).unwrap();
    }
    let before = app
        .active
        .as_mut()
        .unwrap()
        .reading
        .probe
        .capture(0)
        .unwrap();
    let width = app.active.as_ref().unwrap().reading.probe.settings.width as usize;
    let rows = before.len() / width;
    app.open_reading_edge(false, 500.0);
    tap(&mut app, "Margins");
    adjust(&mut app, "Left", "+12");
    assert_eq!(geometry(&app), (66, 54, 54, 54, false));
    adjust(&mut app, "Right", "-12");
    assert_eq!(geometry(&app), (66, 42, 54, 54, false));
    let b = &mut app.active.as_mut().unwrap().reading;
    assert_eq!(b.probe.page_for(&anchor), Some(b.page));
    let after = b.probe.capture(b.page).unwrap();
    let mut ink = 0;
    for y in 0..rows {
        let a = &before[y * width..(y + 1) * width];
        let c = &after[y * width..(y + 1) * width];
        assert!(c[..12].iter().all(|&v| v == 255));
        assert!(a[..width - 12] == c[12..], "Mismatched translated row {y}");
        ink += a.iter().filter(|&&v| v < 255).count();
    }
    assert!(ink > 100);
    let rects = b.probe.selection_rects(b.page, &selected);
    assert_eq!(rects.len(), before_rects.len());
    for (a, c) in before_rects.iter().zip(&rects) {
        assert_eq!(
            (a.x + 12, a.y, a.width, a.height),
            (c.x, c.y, c.width, c.height)
        );
    }
    let c = rects[0];
    assert_eq!(
        b.probe
            .word_at(
                b.page,
                (c.x + c.width / 2) as f32,
                (c.y + c.height / 2) as f32
            )
            .unwrap(),
        selected
    );
    adjust(&mut app, "Top", "+1");
    adjust(&mut app, "Bottom", "+12");
    assert_eq!(geometry(&app), (66, 42, 55, 66, false));
    tap(&mut app, "Linked");
    assert_eq!(geometry(&app), (66, 42, 61, 61, true));
    adjust(&mut app, "Bottom", "+12");
    assert_eq!(geometry(&app), (66, 42, 73, 73, true));
    if authored {
        capture_test_screen(&app, &fonts, "READER_MARGIN_PANEL_IMAGE", false);
    }
    assert_ne!(app.input(Input::Tap(120.0, 160.0)), Change::None);
    assert_eq!(app.screen, Screen::BookTools);
    assert_ne!(app.input(Input::Tap(120.0, 160.0)), Change::None);
    assert_eq!(app.screen, Screen::Book);
    app.save().unwrap();
    if authored {
        capture_test_screen(&app, &fonts, "READER_MARGIN_PAGE_IMAGE", false);
    }
    drop(app);
    let mut reopened = Reader::open(path.parent().unwrap(), state, fonts).unwrap();
    reopened.open_path(&path).unwrap();
    assert_eq!(geometry(&reopened), (66, 42, 73, 73, true));
    let b = &reopened.active.as_ref().unwrap().reading;
    assert_eq!(b.probe.page_for(&anchor), Some(b.page));
    reopened.open_reading_edge(false, 500.0);
    assert_eq!(reopened.reading_panel, Some(BarPanel::TextMargins));
    tap(&mut reopened, "Independent");
    adjust(&mut reopened, "Bottom", "-1");
    assert_eq!(geometry(&reopened), (66, 42, 73, 72, false));
    tap(&mut reopened, "Reset margins");
    assert_eq!(geometry(&reopened), initial);
    let b = &mut reopened.active.as_mut().unwrap().reading;
    let restored = b.probe.capture(b.page).unwrap();
    assert!(
        before == restored,
        "The saved margins failed to restore the authored frame"
    );
    if let Some(p) = std::env::var_os("READER_MARGIN_REPORT") {
        let record = serde_json::json!({"translated_ink_pixels":ink,"translated_layout_rows":rows,"shift_px":12,"selected_source_bytes":selected.text.len(),"highlight_rects":rects.len(),"chapter_one_based":chapter+1,"reopened_anchor":true,"reset_identical_frame":true});
        std::fs::write(p, serde_json::to_vec(&record).unwrap()).unwrap();
    }
}

#[test]
fn legacy_paired_margins_and_per_edge_overrides_survive_json_round_trips() {
    let base = Settings::default();
    let mut prefs: crate::library::BookPreferences = serde_json::from_str(
        r#"{"margin":32,"margin_sides":68,"margin_top":21,"margin_bottom":54}"#,
    )
    .unwrap();
    let old = prefs.apply_to(base);
    assert_eq!(
        (
            old.left_margin(),
            old.right_margin(),
            old.top_margin(),
            old.bottom_margin()
        ),
        (68, 68, 21, 54)
    );
    prefs.margin_left = Some(0);
    prefs.margin_right = Some(94);
    let bytes = serde_json::to_vec(&prefs).unwrap();
    let restored: crate::library::BookPreferences = serde_json::from_slice(&bytes).unwrap();
    let settings = restored.apply_to(base);
    assert_eq!(settings.text_width(), base.width - 94);
    assert_eq!((settings.left_margin(), settings.right_margin()), (0, 94));
    prefs.margin_right = Some(u32::MAX);
    assert_eq!(prefs.apply_to(base).right_margin(), 68);
}

#[test]
fn asymmetric_margins_translate_horizontal_illustrations_and_vertical_and_blitz_pages() {
    use crate::{Engine, Probe};
    let fonts = [crate::test_fixture("fonts/Silkscreen-Regular.ttf")];
    let base = Settings {
        width: 650,
        height: 760,
        font_size: 26.0,
        margin: 25,
        ..Settings::default()
    };
    let off_center = Settings {
        margin_left: Some(41),
        margin_right: Some(9),
        ..base
    };
    for (fixture, chapter, engine) in [
        ("gif-signals.epub", 0, Engine::Parley),
        ("vertical-chapters.epub", 1, Engine::Parley),
        ("long-chapter.epub", 0, Engine::Blitz),
    ] {
        let book = crate::test_fixture(fixture);
        let mut original = Probe::open(&book, engine, &fonts, base, chapter).unwrap();
        let mut shifted = Probe::open(&book, engine, &fonts, off_center, chapter).unwrap();
        assert_eq!(original.page_count(), shifted.page_count());
        let before = original.capture(0).unwrap();
        let after = shifted.capture(0).unwrap();
        assert!(before.iter().filter(|&&v| v < 200).count() > 100);
        for (a, b) in before.chunks_exact(650).zip(after.chunks_exact(650)) {
            assert!(b[..16].iter().all(|&v| v == 255));
            assert!(a[..634] == b[16..], "Pixel mismatch in {fixture}");
        }
        if fixture == "gif-signals.epub" {
            let (x, y, path) = (0..base.height)
                .step_by(8)
                .find_map(|y| {
                    (0..base.width).step_by(8).find_map(|x| {
                        original
                            .illustration_at(0, x as f32, y as f32)
                            .map(|path| (x, y, path.to_owned()))
                    })
                })
                .unwrap();
            assert_eq!(
                shifted.illustration_at(0, (x + 16) as f32, y as f32),
                Some(path.as_str())
            );
        }
    }
}
