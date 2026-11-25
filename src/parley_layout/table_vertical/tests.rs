use crate::{
    Anchor, Book, Engine, Probe, PublisherRenderMode, Renderer, Settings, selection::HighlightRect,
    test_fixture, user_styles,
};

fn settings() -> Settings {
    PublisherRenderMode::Book.apply(Settings {
        width: 760,
        height: 930,
        margin: 32,
        font_size: 20.,
        ..Settings::default()
    })
}

fn open(markup: &str, styles: &user_styles::Snapshot) -> Probe {
    Probe::from_book_with_rules(
        Book::open(&test_fixture("field-notes-epub3.epub")).unwrap(),
        Engine::Parley,
        &[test_fixture("fonts/NotoSans-Regular.ttf")],
        settings(),
        0,
        Some(markup.into()),
        &crate::user_hyphenation::Rules::default(),
        styles,
    )
    .unwrap()
}

fn shape(probe: &Probe, text: &str) -> (Anchor, HighlightRect, f32) {
    let start = probe.matches(text).remove(0);
    let end = probe.anchor(start.block, start.offset + text.len());
    let selected = probe.select_range(&start, &end).unwrap();
    assert_eq!(selected.text, text);
    assert_eq!(probe.page_for(&start), Some(0));
    let rect = probe.selection_rects(0, &selected)[0];
    let paragraph = renderer(probe)
        .paragraphs
        .iter()
        .find(|item| item.key == start.block)
        .unwrap();
    let line = paragraph
        .layout
        .lines()
        .find(|line| line.text_range().contains(&start.offset))
        .unwrap();
    let baseline = paragraph.y + line.metrics().baseline;
    (start, rect, baseline)
}

fn renderer(probe: &Probe) -> &super::super::ParleyChapter {
    let Renderer::Parley(layout) = &probe.renderer else {
        unreachable!();
    };
    layout
}

fn capture(probe: &mut Probe, name: &str) -> Vec<u8> {
    let pixels = probe.capture(0).unwrap();
    #[cfg(feature = "kindle")]
    if let Some(path) = std::env::var_os("READER_STYLE_GRID_CAPTURES") {
        primitive_app::write_png(
            &std::path::Path::new(&path).join(format!("reader-table-vertical-{name}.png")),
            probe.settings.width,
            probe.settings.height,
            &pixels,
        )
        .unwrap();
    }
    #[cfg(not(feature = "kindle"))]
    let _ = name;
    pixels
}

#[test]
fn table_vertical_override_matches_td_baselines_and_repositions_rowspans() {
    let fixture = include_str!("../../../tests/fixtures/table-vertical-alignment.xhtml");
    let mut choices = user_styles::Data::default();
    choices
        .choose(1, "td_vertical_align_none", Some(true))
        .unwrap();
    let mut plain = open(fixture, &Default::default());
    let mut styled = open(fixture, &choices.snapshot(1).unwrap());
    let get = |text: &str| {
        let (old, before, old_baseline) = shape(&plain, text);
        let (new, after, new_baseline) = shape(&styled, text);
        assert_eq!(old, new);
        assert_eq!(before.x, after.x);
        ((before.y, old_baseline), (after.y, new_baseline))
    };
    let around = |label: &str, a: f32, b: f32| {
        assert!((a - b).abs() < 1.0, "{label}: {a} and {b}");
    };
    let (top, top_new) = get("Top sample");
    let (mid, mid_new) = get("Mid sample");
    let (bottom, bottom_new) = get("Bottom sample");
    let (header, header_new) = get("Header sample");
    assert!(top.0 < mid.0 && mid.0 < bottom.0);
    around("top and middle baseline", top_new.1, mid_new.1);
    around("top and bottom baseline", top_new.1, bottom_new.1);
    assert_eq!(header, header_new);
    let (small, small_new) = get("Small capitals");
    let (large, large_new) = get("Large capitals");
    assert!(small_new.0 > small.0 && small.1 < large.1);
    around("mixed-size baseline", small_new.1, large_new.1);
    assert_eq!(large, large_new);
    let growth = small_new.0 - small.0;
    let (after, after_new) = get("After table");
    assert!((after_new.0 - after.0 - growth).abs() <= 1);
    let (span, span_new) = get("Span bottom");
    let (upper, upper_new) = get("Upper first");
    let (lower, _) = get("Lower last");
    assert!(span.0 > upper.0);
    around("bottom rowspan with last lower line", span.1, lower.1);
    around("rowspan baseline with upper line", span_new.1, upper_new.1);
    let (tall_span, tall_span_new) = get("Span small start");
    let (peer, peer_new) = get("Early peer");
    assert_eq!(tall_span_new.0 - tall_span.0, peer_new.0 - peer.0);

    for probe in [&plain, &styled] {
        let (_, _, _) = shape(probe, "Tall three");
        let (_, h, _) = shape(probe, "Header sample");
        let (_, s, _) = shape(probe, "Span bottom");
        for rect in [h, s] {
            let x = (rect.x + 5) as f32;
            let y = (rect.y + rect.height / 2) as f32;
            assert_eq!(
                probe.link_at(0, x, y).unwrap().fragment.as_deref(),
                Some("after")
            );
            assert!(probe.word_at(0, x, y).is_some());
        }
    }
    assert_eq!(plain.coverage(), styled.coverage());
    assert!(capture(&mut plain, "publisher") != capture(&mut styled, "baseline"));
}

#[test]
fn table_vertical_alignment_moves_linked_art_marker_and_ruby_with_its_cell() {
    let fixture = r##"<html><head><style>
table { width:100%;border:1px solid black;border-collapse:separate;border-spacing:5px 7px }
td { border:1px solid black;padding:5px }
p { margin:0;text-indent:0 }
</style></head><body><table>
<tr><td style="vertical-align:top">
Long first<br/>Long second<br/>Long third<br/>Long fourth<br/>Long fifth<br/>Long sixth
</td><td id="art-cell" style="vertical-align:bottom;background:#ddd">
<p style="padding:3px;background:#eee"><a href="#result"><img src="../images/shore.png" width="40" height="24"/></a> Picture label</p>
<p>A <ruby>seal<rt>stamp</rt></ruby> record.</p>
<ol><li>Reading entry</li></ol>
</td></tr><tr><td>Next left</td><td>Next right</td></tr>
</table><p id="result">Following prose</p></body></html>"##;
    let bottom = open(fixture, &Default::default());
    let mut overrides = user_styles::Data::default();
    let style = overrides.books.entry(1).or_default();
    style.custom = Some("#art-cell { vertical-align:top !important; }".into());
    style.custom_enabled = true;
    let top = open(fixture, &overrides.snapshot(1).unwrap());
    let delta = shape(&bottom, "Picture label").1.y - shape(&top, "Picture label").1.y;
    assert!(delta > 0);
    for phrase in ["seal", "Reading entry"] {
        let (a, lower, _) = shape(&bottom, phrase);
        let (b, upper, _) = shape(&top, phrase);
        assert_eq!(a, b);
        assert!((lower.y - upper.y - delta).abs() <= 1);
    }
    assert_eq!(
        shape(&bottom, "Following prose").1,
        shape(&top, "Following prose").1
    );
    let mut picture_positions = Vec::new();
    let mut ruby_positions = Vec::new();
    for probe in [&top, &bottom] {
        let chapter = renderer(probe);
        let bounds = probe.pages.range(0).unwrap();
        let (path, columns, rows) = chapter
            .visible_illustrations(bounds, probe.settings)
            .next()
            .unwrap();
        assert_eq!((columns.len(), rows.len()), (40, 24));
        let x = (columns.start + 8) as f32;
        let y = (rows.start + 8) as f32;
        assert_eq!(probe.illustration_at(0, x, y), Some(path));
        assert_eq!(
            probe.link_at(0, x, y).unwrap().fragment.as_deref(),
            Some("result")
        );
        picture_positions.push(rows.start as i32);
        let (ruby, paragraph) = chapter
            .paragraphs
            .iter()
            .find_map(|p| p.ruby.first().map(|ruby| (ruby, p)))
            .unwrap();
        ruby_positions.push(paragraph.y + ruby.y);
    }
    assert!((picture_positions[1] - picture_positions[0] - delta).abs() <= 1);
    assert!((ruby_positions[1] - ruby_positions[0] - delta as f32).abs() <= 1.0);
    let (base, moved) = (&renderer(&top).markers[0], &renderer(&bottom).markers[0]);
    assert!((moved.top - base.top - delta as f32).abs() <= 1.0);
}

#[test]
fn table_vertical_nested_grid_keeps_existing_layout() {
    let fixture = "<html><body><table><tr><td style='vertical-align:bottom'><p>Outer before</p>\
        <table><tr><td style='vertical-align:middle'>Inner content</td></tr></table><p>Outer after</p>\
        </td></tr></table><p>Following prose</p></body></html>";
    let mut choices = user_styles::Data::default();
    choices
        .choose(1, "td_vertical_align_none", Some(true))
        .unwrap();
    let mut ordinary = open(fixture, &Default::default());
    let mut styled = open(fixture, &choices.snapshot(1).unwrap());
    assert!(ordinary.source.table_cells.is_empty());
    assert!(styled.source.table_cells.is_empty());
    for phrase in [
        "Outer before",
        "Inner content",
        "Outer after",
        "Following prose",
    ] {
        assert_eq!(shape(&ordinary, phrase), shape(&styled, phrase));
    }
    assert_eq!(ordinary.capture(0).unwrap(), styled.capture(0).unwrap());
}
