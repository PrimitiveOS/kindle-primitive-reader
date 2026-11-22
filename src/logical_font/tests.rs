use super::*;
use crate::{Engine, Probe, Renderer};

#[test]
fn matches_all_7380_pinned_screen_vectors_including_typed_non_half_values() {
    let vectors: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../tests/fixtures/font-scale/pinned-screen-vectors.json"
    ))
    .unwrap();
    assert_eq!(vectors.len(), 7380);
    for v in vectors {
        let short = v["short_side"].as_u64().unwrap() as u32;
        let context =
            ScaleContext::with_override(short, 4096, v["override"].as_u64().map(|n| n as u16))
                .unwrap();
        let logical = v["logical"].as_f64().unwrap();
        assert_eq!(
            context.requested(logical).unwrap(),
            v["requested_px"].as_u64().unwrap() as u32,
            "{v}"
        );
        assert_eq!(
            context.effective(logical).unwrap(),
            v["effective_px"].as_f64().unwrap() as f32,
            "{v}"
        );
    }
    for bad in [11.999, 255.001, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(!valid(bad));
    }
}

#[test]
fn immutable_full_screen_context_ignores_content_dpi_and_facing_width() {
    let base = Settings::default().with_logical_font(22.).unwrap();
    assert_eq!(base.font_size, 47.);
    for dpi in crate::RENDER_DPI_PRESETS {
        let pane = Settings {
            width: 616,
            render_dpi: dpi,
            ..base
        };
        assert_eq!(pane.with_logical_font(22.).unwrap().font_size, 47.);
        assert_eq!(pane.font_scale, base.font_scale);
    }
    assert_eq!(ScaleContext::new(1264, 1680), ScaleContext::new(1680, 1264));
    assert_eq!(
        ScaleContext::with_override(1264, 1680, Some(300))
            .unwrap()
            .effective(22.)
            .unwrap(),
        44.
    );
    assert_eq!(base.with_logical_font(255.).unwrap().font_size, 340.);
}

#[test]
fn legacy_pixels_and_missing_serialized_fields_keep_the_exact_original_mode() {
    let prefs = crate::library::BookPreferences {
        font_size: Some(42.25),
        ..Default::default()
    };
    let encoded = serde_json::to_string(&prefs).unwrap();
    assert!(!encoded.contains("logical_font_size"));
    let old: crate::library::BookPreferences = serde_json::from_str(&encoded).unwrap();
    assert_eq!(old, prefs);
    let settings = old.apply_to(Settings::default());
    assert_eq!(settings.font_size, 42.25);
    assert_eq!(settings.logical_font_size, None);
    assert_eq!(settings.styled_font(8.), 192.);
    let new = Settings::default().with_logical_font(12.).unwrap();
    assert_eq!(new.styled_font(0.125), 8.);
    assert_eq!(new.styled_font(128.), 340.);
}

fn sample() -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "reader-logical-{}-{}.xhtml",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::write(&path,"<html><head><title>Large text</title></head><body><p id='start'>Harbor WWW <a href='#end'>lantern</a> returns at dusk.</p><p id='end'>End.</p></body></html>").unwrap();
    path
}
#[test]
fn large_stock_fonts_paint_real_ink_and_retain_source_selection_geometry() {
    let path = sample();
    let fonts = [crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut prior_340 = None;
    for logical in [12., 22., 80., 120., 160., 170., 255.] {
        let settings = Settings::default().with_logical_font(logical).unwrap();
        let mut probe = Probe::open(&path, Engine::Parley, &fonts, settings, 0).unwrap();
        let start = probe.fragment("start").unwrap();
        let mut end = start.clone();
        end.offset = 6;
        let selected = probe.select_range(&start, &end).unwrap();
        assert_eq!(selected.text, "Harbor");
        assert!(
            !probe
                .selection_rects(probe.page_for(&start).unwrap(), &selected)
                .is_empty()
        );
        let page = probe.page_for(&start).unwrap();
        let pixels = probe.capture(page).unwrap();
        let rects = probe.selection_rects(page, &selected);
        for rect in &rects {
            let x0 = rect.x.max(0) as usize;
            let x1 = (rect.x + rect.width).min(settings.width as i32).max(0) as usize;
            let y0 = rect.y.max(0) as usize;
            let y1 = (rect.y + rect.height).min(settings.height as i32).max(0) as usize;
            assert!(
                (y0..y1).any(|y| pixels
                    [y * settings.width as usize + x0..y * settings.width as usize + x1]
                    .iter()
                    .any(|&n| n < 128)),
                "selected glyph ink is outside its hit rectangle at{logical}"
            );
        }
        let rect = &rects[0];
        let x = (rect.x + rect.width / 2) as f32;
        let y = (rect.y + rect.height / 2) as f32;
        let hit = probe.word_at(page, x, y).unwrap();
        assert_eq!(hit.text, "Harbor");
        assert_eq!(hit.start.book, start.book);
        assert_eq!(hit.start.block, start.block);

        assert!(
            pixels.iter().filter(|&&n| n < 128).count() > 100,
            "size={logical}"
        );
        let Renderer::Parley(layout) = &probe.renderer else {
            panic!()
        };
        let sizes: Vec<_> = layout
            .selection_layouts(probe.page_range(page))
            .flat_map(|(_, _, _, _, p)| p.lines())
            .flat_map(|l| l.runs())
            .map(|r| r.font_size())
            .collect();
        assert!(
            sizes.contains(&settings.font_size),
            "size={logical}: {sizes:?}"
        );
        if settings.font_size == 340. {
            if let Some(prior) = prior_340.take() {
                assert_eq!(pixels, prior);
            }
            prior_340 = Some(pixels);
        }
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn logical_css_font_caps_do_not_corrupt_nested_inheritance_or_em_lengths() {
    let source=crate::source::Source::parse("<html><body><div style='font-size:800%'><p style='font-size:12.5%;padding:1em'>Restored size</p></div></body></html>").unwrap();
    // The loaded author sheet includes inline properties even with no linked sheet.
    let path = sample();
    let mut book = crate::epub::Book::open(&path).unwrap();
    let mut fonts =
        crate::font_context(&[crate::test_fixture("fonts/NotoSans-Regular.ttf")]).unwrap();
    let sheet = crate::book_style::BookStyle::load(&mut book, "content.xhtml", &source, &mut fonts);
    let settings = Settings::default().with_logical_font(120.).unwrap();
    let styles = sheet.compute_for_settings(&source.elements, settings);
    let div = source.elements.iter().position(|e| e.tag == "div").unwrap();
    let p = source.elements.iter().position(|e| e.tag == "p").unwrap();
    assert_eq!(styles[div].size_scale, 8.);
    assert_eq!(styles[p].size_scale, 1.);
    assert_eq!(settings.styled_font(styles[div].size_scale), 340.);
    assert_eq!(
        styles[p].padding[0].box_pixels(1000, settings.font_size),
        settings.font_size
    );
    std::fs::remove_file(path).unwrap();
}

#[test]
fn legacy_context_changes_leave_all_pixels_and_page_anchors_identical() {
    let path = sample();
    let fonts = [crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let settings = Settings {
        font_size: 42.25,
        ..Settings::default()
    };
    let mut baseline = Probe::open(&path, Engine::Parley, &fonts, settings, 0).unwrap();
    let mut changed = Probe::open(
        &path,
        Engine::Parley,
        &fonts,
        Settings {
            font_scale: ScaleContext::with_override(600, 800, Some(900)).unwrap(),
            ..settings
        },
        0,
    )
    .unwrap();
    assert_eq!(baseline.page_count(), changed.page_count());
    for page in 0..baseline.page_count() {
        assert_eq!(baseline.page_anchor(page), changed.page_anchor(page));
        assert_eq!(
            baseline.capture(page).unwrap(),
            changed.capture(page).unwrap()
        );
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn large_logical_publication_can_exceed_legacy_height_without_losing_its_last_passage() {
    let path = sample();
    let html = format!(
        "<html><body><p>{}</p><p id='end'>END</p></body></html>",
        "W<br/>".repeat(5200)
    );
    std::fs::write(&path, html).unwrap();
    let fonts = [crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut legacy = Probe::open(&path, Engine::Parley, &fonts, Settings::default(), 0).unwrap();
    let mut large = Probe::open(
        &path,
        Engine::Parley,
        &fonts,
        Settings::default().with_logical_font(255.).unwrap(),
        0,
    )
    .unwrap();
    assert!(large.page_range(large.page_count() - 1).end > 2_000_000.);
    assert!(large.page_range(large.page_count() - 1).end < MAX_LOGICAL_HEIGHT);
    let anchor = large.fragment("end").unwrap();
    assert_eq!(legacy.fragment("end"), Some(anchor.clone()));
    let mut end = anchor.clone();
    end.offset += 3;
    let selection = large.select_range(&anchor, &end).unwrap();
    assert_eq!(selection.text, "END");
    let page = large.page_for(&anchor).unwrap();
    let rects = large.selection_rects(page, &selection);
    assert!(!rects.is_empty());
    let rect = &rects[0];
    assert_eq!(
        large
            .word_at(
                page,
                (rect.x + rect.width / 2) as f32,
                (rect.y + rect.height / 2) as f32
            )
            .unwrap()
            .text,
        "END"
    );
    assert!(large.capture(page).unwrap().iter().any(|&n| n < 128));
    assert!(
        legacy
            .capture(legacy.page_for(&anchor).unwrap())
            .unwrap()
            .iter()
            .any(|&n| n < 128)
    );
    assert!(crate::Pagination::fixed_stride(1000., 2_000_001.).is_err());
    assert!(crate::Pagination::with_breaks(&[], 1000., 2_000_001., &[]).is_err());
    assert!(
        crate::Pagination::fixed_stride_capped(1000., MAX_LOGICAL_HEIGHT + 1., MAX_LOGICAL_HEIGHT)
            .is_err()
    );
    assert!(
        crate::Pagination::with_breaks_capped(
            &[],
            1000.,
            MAX_LOGICAL_HEIGHT + 1.,
            &[],
            MAX_LOGICAL_HEIGHT
        )
        .is_err()
    );
    std::fs::remove_file(path).unwrap();
}
