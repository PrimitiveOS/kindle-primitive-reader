use super::*;
use crate::epub::Book;
use crate::footnotes::cross_part::{self, Layout as NoteLayout};
use crate::user_styles::{Data, FB2_FOOTNOTES};

fn foreign_note() -> NoteLayout {
    let path = crate::test_fixture("harbor-continuation.fb3");
    let mut book = Book::open(&path).unwrap();
    let host_chapter = book.chapters[0].href.clone();
    let xhtml = book.chapter(0).unwrap();
    let host = book.chapter_source(&host_chapter, &xhtml).unwrap();
    let settings = Settings {
        width: 720,
        height: 480,
        margin: 32,
        font_size: 25.0,
        line_height: 1.2,
        ..Settings::default()
    };
    let note = cross_part::prepare_for_settings(&mut book, &host_chapter, &host, settings)
        .unwrap()
        .notes
        .pop()
        .unwrap();
    let fonts = crate::font_context(&[crate::test_fixture("fonts/NotoSans-Regular.ttf")]).unwrap();
    let mut styles = Data::default();
    styles.choose(1, FB2_FOOTNOTES, Some(true)).unwrap();
    note.layout(
        &mut book,
        fonts,
        settings,
        &styles.snapshot(1).unwrap(),
        &Default::default(),
    )
    .unwrap()
}

fn word_anchors(layout: &NoteLayout, word: &str) -> (Anchor, Anchor) {
    let block = layout
        .note
        .source
        .blocks
        .iter()
        .find(|block| layout.note.blocks.contains(&block.key) && block.text.contains(word))
        .unwrap();
    let start = block.text.find(word).unwrap();
    (
        layout.note_anchor(block.key, start).unwrap(),
        layout.note_anchor(block.key, start + word.len()).unwrap(),
    )
}

fn point_for(layout: &NoteLayout, start: &Anchor, end: &Anchor) -> (f32, f32) {
    for y in 0..layout.extent.ceil() as usize {
        for x in 0..layout.settings.text_width() as usize {
            let (x, y) = (x as f32 + 0.5, y as f32 + 0.5);
            if layout.anchor_at(x, y).is_some_and(|hit| {
                hit.block == start.block && (start.offset..end.offset).contains(&hit.offset)
            }) {
                return (x, y);
            }
        }
    }
    panic!("the retained foreign word has no hittable painted point");
}

#[test]
fn cross_part_selection_word_and_caret_preserve_foreign_anchor_identity() {
    let layout = foreign_note();
    let visible = layout.note_visible_text().unwrap();
    assert!(visible.contains("The note records a calm sea"));
    assert!(visible.contains("Return to the boat"));
    assert!(!visible.contains("The final sounding"));
    let (start, end) = word_anchors(&layout, "calm");
    let point = point_for(&layout, &start, &end);
    let selected = layout.note_word_at(point.0, point.1).unwrap();
    assert_eq!(selected.text, "calm");
    assert_eq!(selected.start, start);
    assert_eq!(selected.end, end);
    let caret = layout.note_caret_at(point.0, point.1).unwrap();
    assert_eq!(caret.book, layout.note.book);
    assert_eq!(caret.chapter, "fb3-section-2.xhtml");
    assert_eq!(caret.revision, layout.note.source.revision);
    assert_eq!(caret.block, start.block);
    assert!((start.offset..=end.offset).contains(&caret.offset));
    assert_eq!(layout.note_select_range(&end, &start), Some(selected));

    let selection = layout.note_select_range(&start, &end).unwrap();
    let (before, after) = layout.note_word_context(&selection, 20).unwrap();
    assert!(before.contains("The note records a"));
    assert!(after.contains("sea"));
    assert!(!before.contains("final sounding") && !after.contains("final sounding"));
    assert!(layout.note_dictionary_scan(&selection).is_none());

    for (x, y) in [
        (-0.1, point.1),
        (layout.settings.text_width() as f32, point.1),
        (point.0, -0.1),
        (point.0, layout.extent),
        (f32::NAN, point.1),
        (point.0, f32::INFINITY),
    ] {
        assert!(layout.note_word_at(x, y).is_none());
        assert!(layout.note_caret_at(x, y).is_none());
    }
}

#[test]
fn cross_part_selection_link_geometry_and_reference_use_the_foreign_source() {
    let layout = foreign_note();
    let links = layout.note_links();
    let (label, href, rects) = links
        .iter()
        .find(|(label, _, _)| label == "Return to the boat")
        .unwrap();
    assert_eq!(*href, "document.xhtml#opening");
    assert_eq!(label, "Return to the boat");
    let (start, end) = word_anchors(&layout, "Return");
    let point = point_for(&layout, &start, &end);
    let (context, reference) = layout.note_reference_at(point.0, point.1).unwrap();
    assert_eq!(context.label, "Return to the boat");
    assert_eq!(reference.text, "Return to the boat");
    assert_eq!(reference.start.chapter, "fb3-section-2.xhtml");
    assert!(rects.iter().any(|rect| {
        let x = point.0 + layout.settings.left_margin() as f32;
        let y = point.1 + layout.settings.top_margin() as f32;
        (rect.x as f32..(rect.x + rect.width) as f32).contains(&x)
            && (rect.y as f32..(rect.y + rect.height) as f32).contains(&y)
    }));
}

#[test]
fn cross_part_selection_rectangles_follow_foreign_text_and_reject_stale_anchors() {
    let mut layout = foreign_note();
    let (start, end) = word_anchors(&layout, "calm");
    let selection = layout.note_select_range(&start, &end).unwrap();
    let rects = layout.note_selection_rects(&selection);
    assert!(!rects.is_empty());
    let settings = layout.settings;
    let frame = layout
        .renderer
        .capture(0.0..layout.extent, settings)
        .unwrap();
    let width = settings.width as usize;
    assert!(rects.iter().any(|rect| {
        (rect.y.max(0) as usize..(rect.y + rect.height).max(0) as usize).any(|y| {
            (rect.x.max(0) as usize..(rect.x + rect.width).max(0) as usize)
                .any(|x| frame.get(y * width + x).is_some_and(|&pixel| pixel < 128))
        })
    }));
    assert!(rects.iter().all(|rect| {
        rect.x >= settings.left_margin() as i32
            && rect.y >= settings.top_margin() as i32
            && rect.y + rect.height <= settings.top_margin() as i32 + layout.extent.ceil() as i32
    }));

    let mut invalid = start.clone();
    invalid.chapter = "document.xhtml".into();
    assert!(layout.note_select_range(&invalid, &end).is_none());
    let mut invalid = start.clone();
    invalid.revision ^= 1;
    assert!(layout.note_select_range(&invalid, &end).is_none());
    let mut invalid = start.clone();
    invalid.book ^= 1;
    assert!(layout.note_select_range(&invalid, &end).is_none());
    let mut invalid = start.clone();
    invalid.illustration = Some(0);
    assert!(layout.note_select_range(&invalid, &end).is_none());
    let mut invalid = start.clone();
    invalid.offset = usize::MAX;
    assert!(layout.note_select_range(&invalid, &end).is_none());
    let outside = layout
        .note
        .source
        .blocks
        .iter()
        .find(|block| !layout.note.blocks.contains(&block.key) && !block.text.is_empty())
        .unwrap();
    let mut invalid = start.clone();
    invalid.block = outside.key;
    invalid.offset = 0;
    assert!(layout.note_select_range(&invalid, &end).is_none());
    let mut invalid_selection = selection;
    invalid_selection.start = invalid;
    assert!(layout.note_selection_rects(&invalid_selection).is_empty());
}
