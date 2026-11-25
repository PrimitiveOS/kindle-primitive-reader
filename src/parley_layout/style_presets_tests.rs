use super::*;
use crate::{Engine, Probe, Renderer, user_styles};
use std::path::Path;

fn snapshot(preset: &str) -> user_styles::Snapshot {
    let mut data = user_styles::Data::default();
    data.choose(1, preset, Some(true)).unwrap();
    data.snapshot(1).unwrap()
}

fn open(
    path: &Path,
    settings: Settings,
    excerpt: Option<String>,
    style: &user_styles::Snapshot,
) -> Probe {
    Probe::from_book_with_rules(
        Book::open(path).unwrap(),
        Engine::Parley,
        &[crate::test_fixture("fonts/NotoSans-Regular.ttf")],
        settings,
        0,
        excerpt,
        &crate::user_hyphenation::Rules::default(),
        style,
    )
    .unwrap()
}

fn paragraph(probe: &Probe, key: usize) -> &Paragraph {
    let Renderer::Parley(chapter) = &probe.renderer else {
        unreachable!()
    };
    chapter.paragraphs.iter().find(|p| p.key == key).unwrap()
}

fn capture(probe: &mut Probe, name: &str) {
    #[cfg(feature = "kindle")]
    if let Some(directory) = std::env::var_os("READER_STYLE_GRID_CAPTURES") {
        let pixels = probe.capture(0).unwrap();
        primitive_app::write_png(
            &Path::new(&directory).join(name),
            probe.settings.width,
            probe.settings.height,
            &pixels,
        )
        .unwrap();
    }
}

#[test]
fn reading_style_paragraph_spacing_respects_siblings_and_authored_web_overrides() {
    let path = crate::test_fixture("field-notes-epub3.epub");
    let markup = include_str!("../../tests/fixtures/reading-paragraph-spacing.xhtml");
    let settings = Settings {
        width: 617,
        height: 760,
        margin: 30,
        font_size: 40.0,
        block_margins: crate::BlockMarginMode::Ignore,
        ..Settings::default()
    };
    let book = |style: &user_styles::Snapshot| open(&path, settings, Some(markup.into()), style);
    let by_id = |probe: &Probe, id| probe.fragment(id).unwrap().block;
    let between = |probe: &Probe, a, b| {
        let next = paragraph(probe, by_id(probe, b));
        let top = next.y
            + next
                .layout
                .get(0)
                .unwrap()
                .metrics()
                .content_block_min_coord;
        top - paragraph(probe, by_id(probe, a)).text_bottom
    };
    let first_x = |probe: &Probe, id| {
        paragraph(probe, by_id(probe, id))
            .layout
            .get(0)
            .unwrap()
            .items()
            .find_map(|item| match item {
                PositionedLayoutItem::GlyphRun(run) => run.positioned_glyphs().next().map(|g| g.x),
                _ => None,
            })
            .unwrap()
    };
    let base = book(&user_styles::Snapshot::default());
    for (preset, label, expected) in [
        ("paragraph_no_whitespace", "none", 0.0),
        ("paragraph_whitespace_half", "half", 20.0),
        ("paragraph_whitespace", "full", 40.0),
        ("paragraph_web_browser_style", "web", 40.0),
    ] {
        let mut probe = book(&snapshot(preset));
        let first = between(&probe, "first", "second");
        let second = between(&probe, "second", "third");
        let authored = between(&probe, "after-heading", "authored");
        assert!(
            (first - expected).abs() < 0.01,
            "{preset}: first gap {first}"
        );
        assert!(
            (second - expected).abs() < 0.01,
            "{preset}: second gap {second}"
        );
        assert!(
            (authored - if label == "web" { 80.0 } else { expected }).abs() < 0.01,
            "{preset}: authored gap {authored}"
        );
        if label == "half" || label == "full" {
            let other_boundary = between(&probe, "heading", "after-heading");
            assert!((other_boundary - between(&base, "heading", "after-heading")).abs() < 0.01);
        }
        assert_eq!(first_x(&probe, "authored"), first_x(&base, "authored"));
        if label == "web" {
            assert!(first_x(&probe, "authored") - first_x(&probe, "after-heading") > 75.0);
        }
        assert_eq!(probe.coverage(), base.coverage());
        assert_eq!(
            probe.matches("A bright signal"),
            base.matches("A bright signal")
        );
        assert_eq!(
            probe.restore_anchor(&base.fragment("authored").unwrap()),
            probe.fragment("authored")
        );
        assert_eq!(
            probe.page_for(&probe.fragment("authored").unwrap()),
            Some(0)
        );
        capture(&mut probe, &format!("reader-paragraph-spacing-{label}.png"));
    }
}

#[test]
fn reading_style_cjk_grid_centers_lines_and_rounds_the_existing_signed_indent() {
    for (width, em, percent, expected) in [
        (557, 40.0, 100, Some((18, 520, 40))),
        (557, 40.0, 125, Some((3, 550, 50))),
        (19, 40.0, 100, None),
    ] {
        assert_eq!(cjk_width::grid(width, em, percent), expected);
    }
    for (pixels, width, unit, expected) in [
        (0.0, 520, 40, 0.0),
        (9.0, 280, 40, 40.0),
        (48.0, 280, 40, 40.0),
        (48.0, 520, 40, 80.0),
        (-48.0, 520, 40, -80.0),
        (60.0, 550, 50, 100.0),
    ] {
        assert_eq!(cjk_width::grid_indent(pixels, width, unit), expected);
    }
    let path = crate::test_fixture("harbour-cjk-width.epub");
    let markup = Book::open(&path)
        .unwrap()
        .chapter(0)
        .unwrap()
        .replace("id=\"sample\"", "id=\"sample\" style=\"text-indent:1.2em\"")
        .replace(
            "id=\"t1\"",
            "id=\"t1\" style=\"text-align:justify!important;text-indent:1.2em!important\"",
        );
    assert!(markup.contains("id=\"sample\" style=\"text-indent:1.2em\""));
    let settings = Settings {
        width: 617,
        height: 680,
        margin: 30,
        font_size: 40.0,
        alignment: crate::TextAlignment::Start,
        ..Settings::default()
    };
    let base = user_styles::Snapshot::default();
    let grid = snapshot("cjk_tailored");
    assert_ne!(grid.digest, base.digest);
    let mut plain = open(&path, settings, Some(markup.clone()), &base);
    let mut tailored = open(&path, settings, Some(markup.clone()), &grid);
    let mut wide = open(
        &path,
        Settings {
            cjk_width_percent: 125,
            ..settings
        },
        Some(markup),
        &grid,
    );
    let start = plain.fragment("sample").unwrap();
    let end = crate::Anchor {
        offset: start.offset + "港".len(),
        ..start.clone()
    };
    let first = |probe: &Probe| {
        let sample = paragraph(probe, start.block);
        let glyph = sample
            .layout
            .get(0)
            .unwrap()
            .items()
            .find_map(|item| {
                let PositionedLayoutItem::GlyphRun(run) = item else {
                    return None;
                };
                run.positioned_glyphs().next()
            })
            .unwrap();
        (sample.x, glyph.x)
    };
    assert_eq!(first(&plain), (0.0, 48.0));
    assert_eq!(first(&tailored), (18.0, 80.0));
    // The wider glyph is centered in its 50px cell by the CJK width control.
    assert_eq!(first(&wide), (3.0, 105.0));
    for probe in [&plain, &tailored, &wide] {
        assert_eq!(probe.coverage(), plain.coverage());
        assert_eq!(probe.missing_glyphs(), 0);
        assert_eq!(probe.restore_anchor(&start).unwrap(), start);
        let page = probe.page_for(&start).unwrap();
        let selection = probe.select_range(&start, &end).unwrap();
        assert_eq!(selection.text, "港");
        let rect = probe.selection_rects(page, &selection)[0];
        assert!(
            probe
                .word_at(
                    page,
                    (rect.x + rect.width / 2) as f32,
                    (rect.y + rect.height / 2) as f32
                )
                .is_some()
        );
        assert_eq!(probe.matches("小さな船"), plain.matches("小さな船"));
    }
    capture(&mut plain, "reader-style-grid-off.png");
    capture(&mut tailored, "reader-style-grid-100.png");
    capture(&mut wide, "reader-style-grid-125.png");
}

#[test]
fn reading_style_following_paragraph_indent_respects_real_siblings() {
    let path = crate::test_fixture("field-notes-epub3.epub");
    let content = "<html><body><p id='first' style='text-indent:2em'>First paragraph.</p>\
        <p id='following' style='text-indent:2em!important'>Following paragraph.</p>\
        <h2>Heading</h2><p id='after-heading' style='text-indent:2em'>After heading.</p>\
        <div><p id='nested-first' style='text-indent:2em'>New container.</p></div></body></html>";
    let base = open(
        &path,
        Settings::default(),
        Some(content.into()),
        &user_styles::Snapshot::default(),
    );
    let preset = open(
        &path,
        Settings::default(),
        Some(content.into()),
        &snapshot("paragraph_following_no_indent"),
    );
    let x = |probe: &Probe, id| {
        let source = probe.fragment(id).unwrap();
        let layout = &paragraph(probe, source.block).layout;
        let line = layout.get(0).unwrap();
        line.items()
            .find_map(|item| {
                let PositionedLayoutItem::GlyphRun(run) = item else {
                    return None;
                };
                run.positioned_glyphs().next().map(|glyph| glyph.x)
            })
            .unwrap()
    };
    let authored = x(&base, "following");
    assert!(authored > 0.0);
    assert_eq!(x(&preset, "following"), 0.0);
    for id in ["first", "after-heading", "nested-first"] {
        assert_eq!(x(&preset, id), x(&base, id));
    }
    assert_eq!(
        base.matches("Following paragraph"),
        preset.matches("Following paragraph")
    );
    assert_eq!(base.coverage(), preset.coverage());
}

#[test]
fn reading_style_monochrome_clears_backgrounds_and_paints_existing_borders_black() {
    let path = crate::test_fixture("field-notes-epub3.epub");
    let content =
        std::fs::read_to_string(crate::test_fixture("reading-style-grid-colors.xhtml")).unwrap();
    let settings = crate::PublisherRenderMode::Web.apply(Settings {
        width: 617,
        height: 680,
        margin: 30,
        font_size: 40.0,
        ..Settings::default()
    });
    let mut base = open(
        &path,
        settings,
        Some(content.clone()),
        &user_styles::Snapshot::default(),
    );
    let mut high_contrast = open(
        &path,
        settings,
        Some(content),
        &snapshot("pure_black_and_white"),
    );
    for (id, background, text) in [
        ("slate", 187, "Slate border specimen."),
        ("tinted", 224, "Tinted paper specimen."),
    ] {
        let start = base.fragment(id).unwrap();
        assert_eq!(paragraph(&base, start.block).background, Some(background));
        assert_eq!(paragraph(&high_contrast, start.block).background, None);
        assert_eq!(base.matches(text), high_contrast.matches(text));
        let end = crate::Anchor {
            offset: start.offset + text.len(),
            ..start.clone()
        };
        let hits = |probe: &Probe| {
            let selection = probe.select_range(&start, &end).unwrap();
            probe.selection_rects(probe.page_for(&start).unwrap(), &selection)
        };
        assert_eq!(hits(&base), hits(&high_contrast));
    }
    assert_eq!(base.coverage(), high_contrast.coverage());
    let old = base.capture(0).unwrap();
    let new = high_contrast.capture(0).unwrap();
    capture(&mut base, "reader-style-monochrome-off.png");
    capture(&mut high_contrast, "reader-style-monochrome-on.png");
    let mut towards_black = [0usize; 256];
    for (&before, &after) in old.iter().zip(&new) {
        if after == 0 {
            towards_black[usize::from(before)] += 1;
        }
    }
    assert!(
        old.iter()
            .zip(&new)
            .filter(|(a, b)| **a == 104 && **b == 0)
            .count()
            > 100,
        "gray-to-black counts near the expected border: {:?}",
        &towards_black[100..=108],
    );
    assert!(
        old.iter()
            .zip(&new)
            .filter(|(a, b)| **a == 187 && **b == 255)
            .count()
            > 100
    );
}
