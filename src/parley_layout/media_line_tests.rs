use super::*;
use crate::{Engine, Probe, Renderer, flow_projection::Projection, source::Source};

fn open(count: usize, projected: bool) -> Probe {
    let removed = if projected {
        "<span style='display:none'>REMOVED</span>"
    } else {
        ""
    };
    let markup = format!(
        "<html><body><p>{removed}Captioné.{} </p></body></html>",
        "<img src='../images/shore.png' width='392' height='180'/>".repeat(count)
    );
    Probe::from_book_content(
        Book::open(&crate::test_fixture("field-notes-epub3.epub")).unwrap(),
        Engine::Parley,
        &[crate::test_fixture("fonts/NotoSans-Regular.ttf")],
        Settings {
            width: 440,
            height: 420,
            margin: 24,
            font_size: 24.,
            ..Default::default()
        },
        0,
        Some(markup),
    )
    .unwrap()
}

#[test]
fn one_and_multiple_wrapped_media_lines_keep_valid_empty_input_carets_and_real_image_anchors() {
    for count in [1, 2, 3] {
        for projected in [false, true] {
            let mut probe = open(count, projected);
            assert_eq!(probe.matches("Captioné.").len(), 1);
            assert!(probe.matches("REMOVED").is_empty());
            let Renderer::Parley(renderer) = &probe.renderer else {
                panic!("parley")
            };
            let mut media_lines = 0;
            for paragraph in &renderer.paragraphs {
                let block = probe
                    .source
                    .blocks
                    .iter()
                    .find(|block| block.key == paragraph.key)
                    .unwrap();
                let view = probe.reading_flow.view(&probe.source, block).unwrap();
                for line in paragraph.layout.lines() {
                    let bytes = line.text_range();
                    assert!(
                        view.text().is_char_boundary(bytes.start)
                            && view.text().is_char_boundary(bytes.end),
                        "{bytes:?}/{}",
                        view.text().len()
                    );
                    let boxes = line
                        .items()
                        .filter(|item| matches!(item, PositionedLayoutItem::InlineBox(_)))
                        .count();
                    if boxes > 0 && line.runs().next().is_none() {
                        media_lines += 1;
                        assert_eq!(bytes, view.text().len()..view.text().len());
                        let top = paragraph.y + line.metrics().content_block_min_coord;
                        assert!(renderer.image_ordinal(block.key, top).is_some());
                    }
                }
            }
            assert_eq!(media_lines, count);
            let (covered, total) = probe.coverage();
            assert_eq!(covered, "Captioné.".chars().count());
            assert_eq!(total, covered + if projected { "REMOVED".len() } else { 0 });
            for page in 0..probe.page_count() {
                let anchor = probe.page_anchor(page).expect("text or media anchor");
                assert_eq!(
                    probe.page_for(&anchor),
                    Some(page),
                    "count{count}/page{page}/{anchor:?}"
                );
                if page > 0 {
                    assert!(anchor.illustration.is_some());
                }
                let bytes = serde_json::to_vec(&anchor).unwrap();
                let stored: crate::Anchor = serde_json::from_slice(&bytes).unwrap();
                assert_eq!(open(count, projected).page_for(&stored), Some(page));
                let pixels = probe.capture(page).unwrap();
                assert!(pixels.iter().any(|&p| p < 200));
                let range = probe.pages.range(page).unwrap();
                assert!(probe.anchor_in(range.clone()).is_some());
                let _ = probe.capture_range(range.start..range.end).unwrap();
                if let Some(root) = std::env::var_os("READER_MEDIA_LINE_SCREENS") {
                    image::save_buffer(
                        std::path::PathBuf::from(root)
                            .join(format!("media-{count}-{projected}-{page}.png")),
                        &pixels,
                        probe.settings.width,
                        probe.settings.height,
                        image::ColorType::L8,
                    )
                    .unwrap();
                }
            }
        }
    }
}

#[test]
fn media_caret_fix_does_not_accept_invalid_text_utf8_or_out_of_range_source_bounds() {
    let source = Source::parse("<html><body><p>é text</p></body></html>").unwrap();
    let view = Projection::identity(&source.blocks[0]);
    for bytes in [1..2, 0..100, usize::MAX..0] {
        let mut line = Line {
            block: source.blocks[0].key,
            bytes: bytes.clone(),
            spans: None,
            additional_sources: None,
            top: 0.,
            bottom: 20.,
            page_extension: [0.0; 2],
            missing_glyphs: 0,
        };
        assert!(line.project(&view, bytes).is_err());
    }
}
