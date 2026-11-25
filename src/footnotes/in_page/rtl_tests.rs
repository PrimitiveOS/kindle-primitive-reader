use crate::{
    Anchor, Book, Engine, Probe, PublisherRenderMode, Settings,
    selection::HighlightRect,
    test_fixture,
    user_styles::{Data, EPUB_FOOTNOTES, INLINE_ALL_FOOTNOTE_CONTENT},
};

const RTL_CHAPTER: usize = 26;
const TIE_CHAPTER: usize = 27;

fn open(chapter: usize, presets: &[&str]) -> Probe {
    let mut data = Data::default();
    for preset in presets {
        data.choose(1, preset, Some(true)).unwrap();
    }
    let settings = PublisherRenderMode::Book.apply(Settings {
        width: 720,
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
        || {
            crate::font_context(&[
                test_fixture("fonts/NotoSans-Regular.ttf"),
                test_fixture("fonts/NotoSansArabic-Regular.ttf"),
            ])
        },
    )
    .unwrap()
}

fn rect(probe: &Probe, page: usize, phrase: &str) -> HighlightRect {
    let starts = probe.matches(phrase);
    assert_eq!(starts.len(), 1, "{phrase}");
    let first = &starts[0];
    let last = probe.anchor(first.block, first.offset + phrase.len());
    let selection = probe.select_range(first, &last).unwrap();
    assert_eq!(selection.text, phrase);
    let boxes = probe.selection_rects(page, &selection);
    assert_eq!(boxes.len(), 1, "{phrase}");
    boxes[0]
}

fn center(rect: HighlightRect) -> (f32, f32) {
    (
        (rect.x + rect.width / 2) as f32,
        (rect.y + rect.height / 2) as f32,
    )
}

fn capture(probe: &mut Probe, page: usize, name: &str) -> Vec<u8> {
    let pixels = probe.capture(page).unwrap();
    if let Some(folder) = std::env::var_os("READER_IN_PAGE_FOOTNOTE_CAPTURES") {
        image::save_buffer(
            std::path::Path::new(&folder).join(format!("reader-rtl-note-{name}.png")),
            &pixels,
            probe.settings.width,
            probe.settings.height,
            image::ColorType::L8,
        )
        .unwrap();
    }
    pixels
}

fn check_rule(probe: &Probe, page: usize, frame: &[u8], right: bool) {
    let info = &probe.in_page_notes.as_ref().unwrap()[page];
    let y = (info.separator.unwrap() + probe.settings.top_margin()) as usize;
    assert_eq!(info.separator_on_right, right);
    let stride = probe.settings.width as usize;
    let width = stride / 7;
    let left = probe.settings.left_margin() as usize;
    let right_end = stride - probe.settings.right_margin() as usize;
    let row = &frame[y * stride..(y + 1) * stride];
    let match_rule = |start| row[start..start + width].iter().all(|&ink| ink == 85);
    assert_eq!(match_rule(left), !right);
    assert_eq!(match_rule(right_end - width), right);
}

#[test]
fn in_page_rtl_preserves_arabic_order_latin_words_links_and_saved_anchors() {
    let mut ordinary = open(RTL_CHAPTER, &[]);
    let mut composed = open(RTL_CHAPTER, &[EPUB_FOOTNOTES]);
    let mut inline = open(RTL_CHAPTER, &[EPUB_FOOTNOTES, INLINE_ALL_FOOTNOTE_CONTENT]);
    let reference = ordinary.fragment("rtl-local-ref").unwrap();
    let source = ordinary.fragment("rtl-local-quote").unwrap();
    let saved: Anchor = serde_json::from_slice(&serde_json::to_vec(&source).unwrap()).unwrap();
    let page = ordinary.page_for(&reference).unwrap();
    let original = ordinary.page_for(&saved).unwrap();
    assert!(original > page);
    let (bx, by) = center(rect(&composed, page, "11"));
    assert_eq!(
        composed.link_at(page, bx, by).unwrap().fragment.as_deref(),
        Some("rtl-local-note")
    );
    assert_eq!(rect(&composed, page, "يسجل"), rect(&ordinary, page, "يسجل"));
    for probe in [&composed, &inline] {
        assert_eq!(probe.page_for(&saved), Some(original));
        assert_eq!(probe.fragment("rtl-local-quote"), Some(saved.clone()));
        assert_eq!(probe.coverage(), ordinary.coverage());
        assert_eq!(probe.missing_glyphs(), 0);
        assert_eq!(probe.matches("قياس المد"), ordinary.matches("قياس المد"));
        for current in [page, original] {
            let start = rect(probe, current, "أظهر");
            let latin = rect(probe, current, "S4");
            let end = rect(probe, current, "الغروب");
            assert!(start.x > latin.x && latin.x > end.x);
            assert_eq!(start.y, end.y);
            let (x, y) = center(rect(probe, current, "المد"));
            assert_eq!(probe.word_at(current, x, y).unwrap().text, "المد");
            let (x, y) = center(rect(probe, current, "العودة"));
            assert_eq!(
                probe.link_at(current, x, y).unwrap().fragment.as_deref(),
                Some("rtl-local-ref")
            );
        }
    }
    let reopened = open(RTL_CHAPTER, &[EPUB_FOOTNOTES]);
    assert_eq!(reopened.page_for(&saved), Some(original));
    let off = capture(&mut ordinary, page, "off");
    let band = capture(&mut composed, page, "band");
    assert_ne!(off, band);
    assert_eq!(band, inline.capture(page).unwrap());
    check_rule(&composed, page, &band, true);
    capture(&mut composed, original, "original");
}

#[test]
fn in_page_rtl_and_ltr_note_line_tie_keeps_a_left_separator() {
    let mut probe = open(TIE_CHAPTER, &[EPUB_FOOTNOTES]);
    let page = probe
        .page_for(&probe.fragment("rtl-tie-ref").unwrap())
        .unwrap();
    let rtl = rect(&probe, page, "الحاشية تفاصيل");
    let ltr = rect(&probe, page, "The paired line");
    assert!(rtl.y < ltr.y && rtl.x > ltr.x);
    let band = capture(&mut probe, page, "tie");
    check_rule(&probe, page, &band, false);
}
