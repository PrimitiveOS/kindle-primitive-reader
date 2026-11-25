use crate::{
    Book, Engine, Probe, PublisherRenderMode, Renderer, Settings,
    selection::TextSelection,
    test_fixture,
    user_styles::{Data, EPUB_FOOTNOTES, INLINE_ALL_FOOTNOTE_CONTENT},
};

const RUBY_CHAPTER: usize = 22;
const RUBY_PAINT_CHAPTER: usize = 23;

fn open(width: u32, chapter: usize, inline: bool) -> Probe {
    let mut data = Data::default();
    data.choose(1, EPUB_FOOTNOTES, Some(true)).unwrap();
    if inline {
        data.choose(1, INLINE_ALL_FOOTNOTE_CONTENT, Some(true))
            .unwrap();
    }
    let settings = PublisherRenderMode::Book.apply(Settings {
        width,
        height: 480,
        margin: 32,
        font_size: 25.,
        line_height: 1.2,
        ..Settings::default()
    });
    Probe::layout_opened_book(
        &mut Some(Book::open(&test_fixture("in-page-footnotes.epub")).unwrap()),
        Engine::Parley,
        settings,
        chapter,
        None,
        &Default::default(),
        &data.snapshot(1).unwrap(),
        || crate::font_context(&[test_fixture("fonts/NotoSans-Regular.ttf")]),
    )
    .unwrap()
}

fn selection(probe: &Probe, text: &str) -> TextSelection {
    let matches = probe.matches(text);
    assert_eq!(matches.len(), 1);
    let start = &matches[0];
    probe
        .select_range(start, &probe.anchor(start.block, start.offset + text.len()))
        .unwrap()
}

fn annotation_rows(probe: &Probe, page: usize, pixels: &[u8]) -> Vec<(usize, bool)> {
    let Renderer::Parley(layout) = &probe.renderer else {
        unreachable!();
    };
    let stride = probe.settings.width as usize;
    let mut rows = Vec::new();
    for (source, destination_y) in probe.page_parts(page) {
        for paragraph in layout.paragraphs_in(source.clone()) {
            for ruby in &paragraph.ruby {
                let top = paragraph.y + ruby.top;
                if !(source.start..source.end).contains(&top) {
                    continue;
                }
                let screen_y = (probe.settings.top_margin() + destination_y) as f32;
                let start_y = (screen_y + top - source.start).floor() as usize;
                let end_y = (screen_y + paragraph.y + ruby.bottom - source.start).ceil() as usize;
                let screen_x = probe.settings.left_margin() as f32 + paragraph.x + ruby.x;
                let start_x = screen_x.floor() as usize;
                let end_x = (screen_x + ruby.layout.width()).ceil() as usize;
                let ink = (start_y..end_y).any(|y| {
                    pixels[y * stride + start_x..y * stride + end_x]
                        .iter()
                        .any(|&gray| gray < 128)
                });
                assert_eq!(ink, ruby.painted, "ruby paint at ({start_x}, {start_y})");
                rows.push((start_y, ink));
            }
        }
    }
    rows
}

fn capture(probe: &mut Probe, page: usize, name: &str) -> Vec<u8> {
    let pixels = probe.capture(page).unwrap();
    if let Some(folder) = std::env::var_os("READER_IN_PAGE_FOOTNOTE_CAPTURES") {
        image::save_buffer(
            std::path::Path::new(&folder).join(format!("reader-note-inline-ruby-{name}.png")),
            &pixels,
            probe.settings.width,
            probe.settings.height,
            image::ColorType::L8,
        )
        .unwrap();
    }
    pixels
}

#[test]
fn whole_note_inline_preserves_ruby_and_base_anchors() {
    let mut ordinary = open(720, RUBY_CHAPTER, false);
    let mut inline = open(720, RUBY_CHAPTER, true);
    let reference = ordinary.fragment("ruby-ref").unwrap();
    let first = selection(&ordinary, "bright water");
    let last = selection(&ordinary, "quiet letter");
    let page = ordinary.page_for(&reference).unwrap();
    let original = ordinary.page_for(&first.start).unwrap();
    assert!(original > page && first.start.block != last.start.block);
    for probe in [&ordinary, &inline] {
        assert!(
            probe.in_page_notes.as_ref().unwrap()[page]
                .separator
                .is_some()
        );
        assert_eq!(probe.page_for(&first.start), Some(original));
        assert_eq!(probe.page_for(&last.start), Some(original));
        assert_eq!(selection(probe, "bright water"), first);
        assert_eq!(selection(probe, "quiet letter"), last);
        assert!(probe.matches("reading gloss").is_empty());
        assert!(probe.matches("silent gloss").is_empty());
        assert!(probe.matches("Unseen draft").is_empty());
        assert_eq!(
            probe.select_range(&first.start, &last.end).unwrap().text,
            "bright water\nquiet letter"
        );
    }
    assert_eq!(ordinary.coverage(), inline.coverage());
    assert_eq!(ordinary.fragment("ruby-tail"), inline.fragment("ruby-tail"));
    for current in [page, original] {
        let old_first = ordinary.selection_rects(current, &first)[0];
        let old_last = ordinary.selection_rects(current, &last)[0];
        assert!(old_first.y < old_last.y);
        let new_first = inline.selection_rects(current, &first)[0];
        let new_last = inline.selection_rects(current, &last)[0];
        assert_eq!(new_first.y, new_last.y);
        assert!(new_first.x < new_last.x);
        let x = (new_first.x + 6) as f32;
        let y = (new_first.y + new_first.height / 2) as f32;
        assert_eq!(inline.word_at(current, x, y).unwrap().start, first.start);
        assert_eq!(
            inline.link_at(current, x, y).unwrap().fragment.as_deref(),
            Some("ruby-ref")
        );
        assert_eq!(
            inline
                .links_on_page(current)
                .iter()
                .filter(|(text, _)| text == "bright water")
                .count(),
            1
        );
    }
    let body = selection(&ordinary, "The ferry reports");
    assert_eq!(
        ordinary.selection_rects(page, &body),
        inline.selection_rects(page, &body)
    );
    let before = capture(&mut ordinary, page, "before");
    let after = capture(&mut inline, page, "after");
    let canonical = capture(&mut inline, original, "original");
    let ordinary_rows = annotation_rows(&ordinary, page, &before);
    assert_eq!(ordinary_rows.len(), 2);
    assert!(ordinary_rows[0].0 < ordinary_rows[1].0);
    for (current, frame) in [(page, after), (original, canonical)] {
        let shifted = annotation_rows(&inline, current, &frame);
        assert_eq!(shifted.len(), 2);
        assert!(shifted.iter().all(|(_, ink)| *ink));
        assert_eq!(shifted[0].0, shifted[1].0);
    }
}

#[test]
fn whole_note_inline_keeps_each_ruby_base_together_and_respects_annotation_visibility() {
    let mut probe = open(310, RUBY_CHAPTER, true);
    let reference = probe.fragment("ruby-ref").unwrap();
    let page = probe.page_for(&reference).unwrap();
    let first = probe.selection_rects(page, &selection(&probe, "bright water"));
    let second = probe.selection_rects(page, &selection(&probe, "quiet letter"));
    assert_eq!(first.len(), 1);
    assert_eq!(second.len(), 1);
    assert!(first[0].y < second[0].y);
    let frame = probe.capture(page).unwrap();
    let annotations = annotation_rows(&probe, page, &frame);
    assert_eq!(annotations.len(), 2);
    assert!(annotations.iter().all(|(_, ink)| *ink));

    let mut before = open(720, RUBY_PAINT_CHAPTER, false);
    let mut after = open(720, RUBY_PAINT_CHAPTER, true);
    let rope = selection(&before, "mooring rope");
    let reference = before.fragment("ruby-paint-ref").unwrap();
    let page = before.page_for(&reference).unwrap();
    let original = before.page_for(&rope.start).unwrap();
    for probe in [&mut before, &mut after] {
        assert_eq!(selection(probe, "mooring rope"), rope);
        assert_eq!(probe.page_for(&rope.start), Some(original));
        let frame = probe.capture(page).unwrap();
        let rows = annotation_rows(probe, page, &frame);
        assert_eq!(rows.len(), 2);
        assert!(rows[0].1 && !rows[1].1);
    }
    let frame = after.capture(original).unwrap();
    let rows = annotation_rows(&after, original, &frame);
    assert_eq!(rows.len(), 2);
    assert!(rows[0].1 && !rows[1].1);
    assert_eq!(rows[0].0, rows[1].0);
}
