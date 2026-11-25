use crate::{
    Book, Engine, Probe, PublisherRenderMode, Renderer, Settings,
    flow_projection::Affinity,
    selection::TextSelection,
    test_fixture,
    user_styles::{Data, EPUB_FOOTNOTES, INLINE_ALL_FOOTNOTE_CONTENT},
};

const RUBY_CHAPTER: usize = 22;
const RUBY_PAINT_CHAPTER: usize = 23;
const GENERATED_CHAPTER: usize = 28;

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
    let name = format!("ruby-{name}");
    capture_named(probe, page, &name)
}

fn capture_named(probe: &mut Probe, page: usize, name: &str) -> Vec<u8> {
    let pixels = probe.capture(page).unwrap();
    if let Some(folder) = std::env::var_os("READER_IN_PAGE_FOOTNOTE_CAPTURES") {
        image::save_buffer(
            std::path::Path::new(&folder).join(format!("reader-note-inline-{name}.png")),
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
fn whole_note_inline_generated_marks_stay_out_of_source_and_keep_media() {
    let mut ordinary = open(720, GENERATED_CHAPTER, false);
    let mut inline = open(720, GENERATED_CHAPTER, true);
    let reference = ordinary.fragment("generated-ref").unwrap();
    let page = ordinary.page_for(&reference).unwrap();
    let memo = selection(&ordinary, "memo");
    let seal = selection(&ordinary, "river seal");
    let buoy = selection(&ordinary, "blue buoy");
    let tail = selection(&ordinary, "Keep a duplicate");
    let following = selection(&ordinary, "Following remark");
    let original = ordinary.page_for(&seal.start).unwrap();
    assert!(original > page);
    for probe in [&ordinary, &inline] {
        assert!(
            probe.in_page_notes.as_ref().unwrap()[page]
                .separator
                .is_some()
        );
        assert_eq!(probe.page_for(&memo.start), Some(original));
        assert_eq!(probe.page_for(&seal.start), Some(original));
        assert_eq!(probe.page_for(&buoy.start), Some(original));
        assert_eq!(selection(probe, "memo"), memo);
        assert_eq!(selection(probe, "river seal"), seal);
        assert_eq!(selection(probe, "blue buoy"), buoy);
        assert_eq!(selection(probe, "Following remark"), following);
        let selected = probe.select_range(&memo.start, &tail.end).unwrap();
        assert_eq!(
            selected.text.lines().map(str::trim).collect::<Vec<_>>(),
            ["memo.", "river seal.", "blue buoy.", "Keep a duplicate"]
        );
        for token in ["[memo]", "‘", "’"] {
            assert!(probe.matches(token).is_empty(), "{token}");
        }
    }
    assert_eq!(ordinary.coverage(), inline.coverage());
    let Renderer::Parley(layout) = &inline.renderer else {
        unreachable!()
    };
    let (source, _) = inline.page_parts(original).next().unwrap();
    let note = layout
        .paragraphs_in(source)
        .filter_map(|paragraph| paragraph.joined_note.as_ref())
        .find(|note| {
            note.pieces
                .iter()
                .any(|part| part.block == seal.start.block)
        })
        .unwrap();
    assert!(note.text.contains("Packet [memo]."), "{:?}", note.text);
    assert!(
        note.text.contains("‘\u{2060}river seal\u{2060}’"),
        "{:?}",
        note.text
    );
    assert!(note.text.contains("blue buoy\u{2060}’"), "{:?}", note.text);
    for (offset, token) in note
        .text
        .char_indices()
        .filter(|(_, ch)| "[]‘’\u{2060}".contains(*ch))
    {
        assert_eq!(
            note.source_location(offset, Affinity::After),
            None,
            "{token}: {offset}"
        );
    }
    for (needle, anchor) in [
        ("memo", &memo.start),
        ("river seal", &seal.start),
        ("blue buoy", &buoy.start),
    ] {
        assert_eq!(
            note.source_location(note.text.find(needle).unwrap(), Affinity::After),
            Some((anchor.block, anchor.offset))
        );
    }
    assert!(
        ordinary.selection_rects(page, &memo)[0].y < ordinary.selection_rects(page, &tail)[0].y
    );
    for current in [page, original] {
        let memo = inline.selection_rects(current, &memo)[0];
        let word = inline.selection_rects(current, &seal)[0];
        let tail = inline.selection_rects(current, &tail)[0];
        assert_eq!(memo.y, tail.y);
        assert!(memo.x < word.x && word.x < tail.x);
        let (x, y) = ((word.x + 5) as f32, (word.y + word.height / 2) as f32);
        assert_eq!(inline.word_at(current, x, y).unwrap().start, seal.start);
        assert_eq!(
            inline.link_at(current, x, y).unwrap().fragment.as_deref(),
            Some("generated-ref")
        );
    }
    let before = capture_named(&mut ordinary, page, "generated-before");
    let after = capture_named(&mut inline, page, "generated-after");
    let canonical = capture_named(&mut inline, original, "generated-original");
    assert_ne!(before, after);
    for (probe, current, pixels) in [
        (&ordinary, page, before),
        (&inline, page, after),
        (&inline, original, canonical),
    ] {
        let annotation = annotation_rows(probe, current, &pixels);
        assert_eq!(annotation.len(), 1, "ruby on page {current}");
        assert!(annotation[0].1);
        let Renderer::Parley(layout) = &probe.renderer else {
            unreachable!();
        };
        let (art, art_y) = probe
            .page_parts(current)
            .find_map(|(source, destination)| {
                let (_, columns, rows) = layout
                    .visible_illustrations(source, probe.settings)
                    .next()?;
                Some((columns, rows.start + destination as usize))
            })
            .expect("the chart shares the ruby line");
        let x = art.start + art.len() / 2;
        assert_eq!(
            probe.illustration_at(current, x as f32, (art_y + 5) as f32),
            Some("chart.png")
        );
        let width = probe.settings.width as usize;
        assert!(
            pixels[art_y * width + art.start..art_y * width + art.end]
                .iter()
                .any(|&gray| gray < 235)
        );
    }
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
