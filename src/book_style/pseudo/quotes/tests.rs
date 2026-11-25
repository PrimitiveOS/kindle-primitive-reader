use super::*;
use crate::{Anchor, BaseStylesheet, Engine, Probe, Source, TypographyLocale};

const WIDTH: u32 = 860;
const HEIGHT: u32 = 920;

fn style_settings(base: BaseStylesheet, main: Option<&str>, embedded: bool) -> Settings {
    Settings {
        base_stylesheet: base,
        typography: crate::TypographyLanguages {
            main: main.and_then(TypographyLocale::parse),
            respect_embedded: embedded,
            ..Default::default()
        },
        height: HEIGHT,
        width: WIDTH,
        margin: 40,
        font_size: 22.0,
        ..Settings::default()
    }
}

fn flow_styles(source: &Source, settings: Settings) -> Vec<Style> {
    let mut book = Book::open(&crate::test_fixture("quote-marks.xhtml")).unwrap();
    let href = book.chapters[0].href.clone();
    BookStyle::load_for_flow_with_base(
        &mut book,
        &href,
        source,
        true,
        &Default::default(),
        settings.base_stylesheet,
    )
    .unwrap()
    .compute_for_settings(&source.elements, settings)
}

fn in_case(source: &Source, id: &str, tag: &str) -> Vec<usize> {
    let parent = source
        .elements
        .iter()
        .position(|element| element.id.as_deref() == Some(id))
        .unwrap();
    let descendant = |mut index: usize| {
        while let Some(next) = source.elements[index].parent {
            if next == parent {
                return true;
            }
            index = next;
        }
        false
    };
    (parent + 1..source.elements.len())
        .take_while(|&index| descendant(index))
        .filter(|&index| source.elements[index].tag == tag)
        .collect()
}

fn generated(styles: &[Style], host: usize, side: Side) -> Option<&Generated> {
    styles[host]
        .generated
        .as_ref()?
        .get(side as usize)?
        .as_ref()
}

fn marks(styles: &[Style], host: usize) -> (Option<&str>, Option<&str>) {
    (
        generated(styles, host, Side::Before).map(|part| part.text.as_str()),
        generated(styles, host, Side::After).map(|part| part.text.as_str()),
    )
}

#[test]
fn quote_marks_preserve_language_depth_hidden_and_cascade() {
    let document = std::fs::read_to_string(crate::test_fixture("quote-marks.xhtml")).unwrap();
    let source = Source::parse(&document).unwrap();
    let base = BaseStylesheet::Traditional;
    let styles = flow_styles(&source, style_settings(base, None, true));
    let en_outer = (Some("“\u{2060}"), Some("\u{2060}”"));
    let en_inner = (Some("‘\u{2060}"), Some("\u{2060}’"));
    let fr_outer = (Some("«\u{2060}"), Some("\u{2060}»"));
    let de_outer = (Some("„\u{2060}"), Some("\u{2060}“"));
    let de_inner = (Some("‚\u{2060}"), Some("\u{2060}‘"));
    let inspect = |styles: &[Style], id: &str| {
        in_case(&source, id, "q")
            .into_iter()
            .map(|host| {
                let (left, right) = marks(styles, host);
                (left.map(str::to_owned), right.map(str::to_owned))
            })
            .collect::<Vec<_>>()
    };
    let check = |styles: &[Style], id, wanted: &[(Option<&str>, Option<&str>)]| {
        assert_eq!(
            inspect(styles, id),
            wanted
                .iter()
                .map(|(a, b)| (a.map(str::to_owned), b.map(str::to_owned)))
                .collect::<Vec<_>>(),
            "{id}",
        );
    };

    check(&styles, "eng", &[en_outer, en_inner, en_outer]);
    check(&styles, "fra", &[fr_outer, en_outer]);
    check(&styles, "deu", &[de_outer, de_inner]);
    check(
        &styles,
        "swi",
        &[fr_outer, (Some("‹\u{2060}"), Some("\u{2060}›"))],
    );
    check(&styles, "mix", &[en_outer, fr_outer, en_inner]);
    check(&styles, "sub", &[en_outer, en_outer, en_outer, en_inner]);
    check(&styles, "tokens", &[en_outer]);
    let bundle = in_case(&source, "bundle", "span")[0];
    assert_eq!(marks(&styles, bundle), (Some("[]"), None));
    check(&styles, "display", &[en_outer]);
    check(&styles, "visibility", &[en_inner]);
    let hidden = in_case(&source, "visibility", "span")[0];
    assert!(
        !generated(&styles, hidden, Side::Before)
            .unwrap()
            .style
            .flow
            .painted
    );
    let first_close = in_case(&source, "extra_close", "span")[0];
    assert_eq!(marks(&styles, first_close), (None, en_inner.1));
    check(&styles, "extra_close", &[en_inner]);
    check(&styles, "extra_silent", &[en_inner]);
    check(&styles, "custompairs", &[en_outer, en_inner]);
    check(&styles, "nopaired", &[en_outer, en_inner]);
    check(&styles, "carry_mid", &[en_inner]);
    check(&styles, "carry_end", &[en_outer]);

    let german = flow_styles(&source, style_settings(base, Some("de"), false));
    check(&german, "fra", &[de_outer, de_inner]);
    check(&german, "mix", &[de_outer, de_inner, de_outer]);
    let french = flow_styles(&source, style_settings(base, Some("fr"), false));
    check(&french, "deu", &[fr_outer, en_outer]);
    let html = flow_styles(
        &source,
        style_settings(BaseStylesheet::HtmlStandard, None, true),
    );
    check(&html, "eng", &[en_outer, en_inner, en_outer]);
    let no_sheet = flow_styles(&source, style_settings(BaseStylesheet::None, None, true));
    check(&no_sheet, "eng", &[(None, None); 3]);
    let author = in_case(&source, "tokens", "span")[0];
    assert_eq!(marks(&no_sheet, author), (en_outer.0, None));
}

fn open(file: &str, chapter: usize, main: Option<&str>, disable: bool) -> Probe {
    let mut rules = user_styles::Data::default();
    if disable {
        rules
            .choose(1, "no_pseudo_element_before_after", Some(true))
            .unwrap();
    }
    Probe::from_book_with_rules(
        Book::open(&crate::test_fixture(file)).unwrap(),
        Engine::Parley,
        &[crate::test_fixture("fonts/NotoSans-Regular.ttf")],
        style_settings(BaseStylesheet::Traditional, main, main.is_none()),
        chapter,
        None,
        &Default::default(),
        &rules.snapshot(1).unwrap(),
    )
    .unwrap()
}

fn position(probe: &Probe, phrase: &str) -> (Anchor, crate::selection::HighlightRect) {
    let mut matches = probe.matches(phrase);
    assert_eq!(matches.len(), 1, "{phrase}");
    let anchor = matches.pop().unwrap();
    let end = Anchor {
        offset: anchor.offset + phrase.len(),
        ..anchor.clone()
    };
    let selection = probe.select_range(&anchor, &end).unwrap();
    assert_eq!(selection.text, phrase);
    assert_eq!(probe.page_for(&anchor), Some(0));
    (anchor, probe.selection_rects(0, &selection)[0])
}

#[test]
fn quote_marks_capture_without_changing_source_links_or_selection() {
    let mut source = open("quote-marks.xhtml", 0, None, false);
    let (anchor, point) = position(&source, "Deepest");
    let (copied, _) = position(&source, "Outer Inner Deepest");
    assert!(source.matches("“").is_empty());
    assert!(source.matches("\u{2060}").is_empty());
    let link = (point.x + 1..point.x + point.width)
        .find_map(|x| source.link_at(0, x as f32, (point.y + point.height / 2) as f32))
        .unwrap();
    assert_eq!(link.fragment.as_deref(), Some("carry_end"));
    let publisher = source.capture(0).unwrap();
    for (state, main, disabled) in [
        ("publisher", None, false),
        ("main-de", Some("de"), false),
        ("pseudo-off", None, true),
    ] {
        let mut layout = open("quote-marks.xhtml", 0, main, disabled);
        let (same, new) = position(&layout, "Deepest");
        assert_eq!(same, anchor);
        assert_eq!(layout.restore_anchor(&copied), Some(copied.clone()));
        let pixels = layout.capture(0).unwrap();
        if disabled {
            assert!(new.x + 12 < point.x);
        }
        assert_eq!(pixels == publisher, state == "publisher");
        if let Some(dir) = std::env::var_os("READER_QUOTE_MARK_CAPTURES") {
            let dir = std::path::PathBuf::from(dir);
            std::fs::create_dir_all(&dir).unwrap();
            image::GrayImage::from_raw(WIDTH, HEIGHT, pixels)
                .unwrap()
                .save(dir.join(format!("reader-quote-marks-{state}.png")))
                .unwrap();
        }
    }
}

#[test]
fn quote_marks_carry_across_an_epub_spine_on_direct_open() {
    let active = open("quote-spine.epub", 1, None, false);
    let off = open("quote-spine.epub", 1, None, true);
    let inset = |phrase| {
        let (anchor, active) = position(&active, phrase);
        let (same, off) = position(&off, phrase);
        assert_eq!(anchor, same);
        active.x - off.x
    };
    let inner = inset("Across the spine");
    let outer = inset("Normal quote");
    assert!(inner > 0 && outer > inner, "{inner} {outer}");
}

#[test]
fn quote_marks_require_a_complete_language_subtag() {
    assert_eq!(pairs("frr"), pairs("en"));
    assert_eq!(pairs("deu"), pairs("en"));
    assert_eq!(pairs("fr-CH"), [('«', '»'), ('‹', '›')]);
    assert_eq!(pairs("fr-chish"), pairs("fr"));
    assert_eq!(pairs("DE-AT"), pairs("de"));
}

#[test]
fn quote_marks_prefix_uses_the_live_wikipedia_backlink_policy() {
    let path = crate::test_fixture("quote-spine.epub");
    let mut book = Book::open(&path).unwrap();
    let href = book.chapters[0].href.clone();
    let xhtml = book.chapter(0).unwrap();
    let source = book.chapter_source(&href, &xhtml).unwrap();
    let backlink = source
        .elements
        .iter()
        .position(|node| node.classes == "mw-cite-backlink")
        .unwrap();
    let settings = style_settings(BaseStylesheet::Traditional, None, true);

    for (wikipedia, expected) in [(false, 1), (true, 2)] {
        let mut preferences = user_styles::Data::default();
        if wikipedia {
            preferences
                .choose(1, user_styles::WIKIPEDIA_FOOTNOTES, Some(true))
                .unwrap();
        }
        let user = preferences.snapshot(1).unwrap();
        let targets = crate::footnotes::in_page::Targets::epub_for_settings(
            &href, &xhtml, &source, settings, &user,
        );
        assert_eq!(
            targets
                .as_ref()
                .is_some_and(|target| target.hides_wikipedia_backlink(backlink)),
            wikipedia,
        );
        let style = BookStyle::load_for_flow_with_base(
            &mut book,
            &href,
            &source,
            true,
            &user,
            BaseStylesheet::Traditional,
        )
        .unwrap();
        let (mut styles, quotes) =
            style.compute_deferred_quotes(&source.elements, settings, targets.as_ref());
        let mut live = Depths::default();
        quotes.resolve(&source.elements, &mut styles, "en", true, &mut live);
        let carried = prefix(&mut book, 1, settings, &user, None, "en");
        assert_eq!(live.0.get("en"), Some(&expected));
        assert_eq!(carried.0, live.0);
    }
}
