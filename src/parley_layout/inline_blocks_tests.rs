use super::*;
use crate::epub::Destination;
use crate::{Anchor, Engine, InlineBlockMode, Probe, Renderer};

const SPECIMEN: &str = include_str!("../../tests/fixtures/render-inline-blocks.xhtml");

fn render(html: &str, settings: Settings) -> Probe {
    Probe::from_book_content(
        Book::open(&crate::test_fixture("field-notes-epub3.epub")).unwrap(),
        Engine::Parley,
        &[crate::test_fixture("fonts/NotoSans-Regular.ttf")],
        settings,
        0,
        Some(html.into()),
    )
    .unwrap()
}

fn config() -> Settings {
    Settings {
        width: 600,
        height: 720,
        margin: 36,
        font_size: 32.,
        line_height: 1.2,
        ..Settings::default()
    }
}

fn span(probe: &Probe, id: &str, word: &str) -> (usize, crate::selection::TextSelection) {
    let start = probe.fragment(id).unwrap();
    let end = Anchor {
        offset: start.offset + word.len(),
        ..start.clone()
    };
    let selection = probe.select_range(&start, &end).unwrap();
    assert_eq!(selection.text, word);
    (probe.page_for(&start).unwrap(), selection)
}

#[test]
fn inline_blocks_keep_atoms_and_source_interactions() {
    let settings = config();
    let mut legacy = render(SPECIMEN, settings);
    let mut boxed = render(
        SPECIMEN,
        Settings {
            inline_block_mode: InlineBlockMode::BoxSizing,
            ..settings
        },
    );
    let Renderer::Parley(layout) = &boxed.renderer else {
        panic!("Expected Parley");
    };
    let boxes: Vec<_> = layout
        .paragraphs
        .iter()
        .filter_map(|p| p.inline_text.as_ref())
        .collect();
    assert!(
        layout.paragraphs[0]
            .layout
            .lines()
            .flat_map(|line| line.items())
            .all(|item| {
                match item {
                    PositionedLayoutItem::GlyphRun(run) => !run.style().brush.decoration.underline,
                    _ => true,
                }
            })
    );
    assert_eq!(boxes.len(), 6);
    assert!(boxes.iter().all(|p| p.boxes.len() == 1));
    assert_eq!(boxes[2].boxes[0].layout.len(), 2);
    assert!(boxes[0].boxes[0].layout.width() <= 192.);

    for (id, word) in [
        ("trail", "Trail"),
        ("cafe", "Café"),
        ("opens", "opens"),
        ("glass", "glass"),
        ("shines", "shines"),
        ("rests", "rests"),
        ("shelter", "Shelter"),
        ("distant", "Distant"),
        ("water", "water"),
        ("arrival", "Arrival"),
    ] {
        let (page, selection) = span(&boxed, id, word);
        let (_, original) = span(&legacy, id, word);
        assert_eq!(selection, original);
        let rects = boxed.selection_rects(page, &selection);
        assert_eq!(rects.len(), 1, "{id}: {rects:?}");
        let bounds = rects[0];
        let x = (bounds.x + bounds.width / 2) as f32;
        let y = (bounds.y + bounds.height / 2) as f32;
        assert_eq!(
            boxed.word_at(page, x, y),
            Some(selection.clone()),
            "{id}: {bounds:?}"
        );
        let caret = boxed.text_position_at(page, x, y).unwrap();
        assert!((selection.start.offset..=selection.end.offset).contains(&caret.offset));
        if id == "cafe" {
            assert!(
                matches!(boxed.link_at(page, x, y), Some(Destination { fragment: Some(id), .. }) if id == "arrival")
            );
        }
    }

    let rect = |p: &Probe, id: &str, word: &str| {
        let (page, selection) = span(p, id, word);
        p.selection_rects(page, &selection)[0]
    };
    assert!(rect(&boxed, "opens", "opens").x > rect(&legacy, "opens", "opens").x + 120);
    assert!(rect(&boxed, "distant", "Distant").y > rect(&boxed, "bank", "Across").y);
    assert!((rect(&boxed, "distant", "Distant").y - rect(&boxed, "water", "water").y).abs() <= 3);
    let (page, multiline) = span(&boxed, "inside", "River bend");
    assert_eq!(boxed.selection_rects(page, &multiline).len(), 2);

    let mut flat = render(
        SPECIMEN,
        Settings {
            inline_block_mode: InlineBlockMode::Flat,
            ..settings
        },
    );
    let band = |frame: &[u8], gray| {
        frame
            .chunks_exact(settings.width as usize)
            .flat_map(|row| row.split(move |pixel| *pixel != gray))
            .map(<[_]>::len)
            .max()
            .unwrap()
    };
    let legacy_frame = legacy.capture(0).unwrap();
    let flat_frame = flat.capture(0).unwrap();
    let frame = boxed.capture(0).unwrap();
    assert_eq!(band(&flat_frame, 210), 192);
    assert_eq!(band(&frame, 210), 216);
    assert_eq!(band(&flat_frame, 222), 211);
    assert_eq!(band(&frame, 222), 211);
    if let Some(dir) = std::env::var_os("READER_INLINE_BLOCK_SCREENS") {
        for (name, frame) in [
            ("legacy", legacy_frame),
            ("flat", flat_frame),
            ("book-web", frame),
        ] {
            image::save_buffer(
                std::path::PathBuf::from(&dir)
                    .join(format!("render-inline-blocks-reader-{name}.png")),
                &frame,
                settings.width,
                settings.height,
                image::ColorType::L8,
            )
            .unwrap();
        }
    }
}

#[test]
fn inline_blocks_keep_fallback_text_and_selection_for_unsupported_boxes() {
    let html = r#"<html><head><style>.box { display:inline-block; width:120px }</style></head>
    <body><p><span class="box" style="height:80px" id="height">Fixed height</span> still flows</p>
    <p><span class="box" id="nested">Outer <span class="box">inner</span></span> still flows</p>
    <p><span class="box" style="width:50000px" id="wide">Too wide</span> still flows</p></body></html>"#;
    let settings = config();
    let mut legacy = render(html, settings);
    let mut boxed = render(
        html,
        Settings {
            inline_block_mode: InlineBlockMode::BoxSizing,
            ..settings
        },
    );
    assert_eq!(legacy.capture(0).unwrap(), boxed.capture(0).unwrap());
    for (id, word) in [
        ("height", "Fixed height"),
        ("nested", "Outer inner"),
        ("wide", "Too wide"),
    ] {
        assert_eq!(span(&legacy, id, word), span(&boxed, id, word));
    }
}

#[test]
fn inline_blocks_move_a_multiline_box_to_a_new_page_as_one_atom() {
    let html = r#"<html><head><style>p { margin:0; padding:0; text-indent:0 }
    .box { display:inline-block; width:120px; background-color:#ccc; padding:8px; }</style></head>
    <body><p>Quay.</p><p>Beacon.</p><p>Marsh.</p><p>Harbor.</p>
    <p><span class="box" id="bend">River bend</span></p></body></html>"#;
    let settings = Settings {
        height: 326,
        inline_block_mode: InlineBlockMode::BoxSizing,
        ..config()
    };
    let probe = render(html, settings);
    let (page, selection) = span(&probe, "bend", "River bend");
    assert!(page > 0);
    assert_eq!(probe.selection_rects(page, &selection).len(), 2);
    assert!(probe.selection_rects(page - 1, &selection).is_empty());
    assert_eq!(probe.page_for(&selection.end), Some(page));
}
