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
    std::fs::write(&path,"<html><head><title>Large text</title></head><body><p id='start'>Harbor <a href='#end'>lantern</a> returns at dusk.</p><p id='end'>End.</p></body></html>").unwrap();
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
