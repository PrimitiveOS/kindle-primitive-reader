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

    for (wikipedia, expected_odd) in [(false, true), (true, false)] {
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
        let carried = prefix(
            &mut book,
            1,
            settings,
            &user,
            None,
            "en",
            &source.elements,
            &|| Ok(()),
        )
        .unwrap()
        .unwrap();
        assert_eq!(live.0.contains("en"), expected_odd);
        assert_eq!(carried.depths.0, live.0);
    }
}

const LONG_SPINE: &str = "quote-spine-long.epub";
const LAST_CHAPTER: usize = 20;

#[cfg(feature = "kindle")]
#[test]
fn quote_marks_stream_a_long_prefix_when_opened_directly() {
    let mut book = Book::open(&crate::test_fixture(LONG_SPINE)).unwrap();
    let bytes: usize = (0..LAST_CHAPTER)
        .map(|index| book.chapter(index).unwrap().len())
        .sum();
    assert!(bytes > 512 * 1024);
    assert_eq!(book.chapters.len(), LAST_CHAPTER + 1);
    let mut active = Probe::from_book_with_rules(
        long_snapshot(),
        Engine::Parley,
        &[crate::test_fixture("fonts/NotoSans-Regular.ttf")],
        style_settings(BaseStylesheet::Traditional, None, true),
        LAST_CHAPTER,
        None,
        &Default::default(),
        &Default::default(),
    )
    .unwrap();
    let off = open(LONG_SPINE, LAST_CHAPTER, None, true);
    let inset = |phrase| {
        let (anchor, painted) = position(&active, phrase);
        let (ordinary, cleared) = position(&off, phrase);
        assert_eq!(anchor, ordinary);
        painted.x - cleared.x
    };
    let inner = inset("Case LongTail");
    let outer = inset("Case RestoredDepth");
    assert!(inner > 0 && outer > inner, "{inner} {outer}");
    if let Some(dir) = std::env::var_os("READER_QUOTE_MARK_CAPTURES") {
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        image::GrayImage::from_raw(WIDTH, HEIGHT, active.capture(0).unwrap())
            .unwrap()
            .save(dir.join("reader-quote-marks-spine-long.png"))
            .unwrap();
    }
}

#[cfg(feature = "kindle")]
fn snapshot_with_member(name: &str, replacement: &[u8]) -> Book {
    use std::io::{Cursor, Read, Write};
    let original = std::fs::read(crate::test_fixture(LONG_SPINE)).unwrap();
    let mut input = zip::ZipArchive::new(Cursor::new(original)).unwrap();
    let mut output = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for index in 0..input.len() {
        let mut entry = input.by_index(index).unwrap();
        let options =
            zip::write::SimpleFileOptions::default().compression_method(entry.compression());
        output.start_file(entry.name(), options).unwrap();
        if entry.name() == name {
            output.write_all(replacement).unwrap();
        } else {
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).unwrap();
            output.write_all(&bytes).unwrap();
        }
    }
    Book::open_snapshot(output.finish().unwrap().into_inner().into()).unwrap()
}

#[cfg(feature = "kindle")]
fn long_snapshot() -> Book {
    Book::open_snapshot(
        std::fs::read(crate::test_fixture(LONG_SPINE))
            .unwrap()
            .into(),
    )
    .unwrap()
}

#[cfg(feature = "kindle")]
#[test]
fn quote_marks_slow_foreground_keeps_the_legacy_prefix_window() {
    let settings = style_settings(BaseStylesheet::Traditional, None, true);
    let user = Default::default();
    let slow_prefix = |book: &mut Book, chapter| {
        book.quote_prefixes = Some(Box::new(Checkpoints::foreground()));
        take_historical_items();
        let incoming = scan_prefix(
            book,
            chapter,
            settings,
            &user,
            None,
            "en",
            &[],
            &|| Ok(()),
            &|| true,
            false,
        )
        .unwrap();
        (incoming, take_historical_items())
    };
    let mut book = long_snapshot();
    let (incoming, items) = slow_prefix(&mut book, LEGACY_PREFIX_CHAPTERS);
    assert_eq!(items, LEGACY_PREFIX_CHAPTERS);
    assert!(incoming.unwrap().depths.0.contains("en"));

    let (incoming, items) = slow_prefix(&mut book, LEGACY_PREFIX_CHAPTERS + 1);
    assert_eq!(items, 0);
    assert!(incoming.is_none());
    assert_eq!(book.quote_prefixes.as_ref().unwrap().latest(), None);

    let large_item = format!(
        "<html><body><p>Large preceding XHTML</p><!--{}--></body></html>",
        "p".repeat(500 * 1024),
    );
    let mut book = snapshot_with_member("chapter-01.xhtml", large_item.as_bytes());
    let (incoming, items) = slow_prefix(&mut book, 3);
    assert_eq!(items, 2);
    assert!(incoming.is_none());
    assert_eq!(book.quote_prefixes.as_ref().unwrap().latest(), Some(2));
}

#[cfg(feature = "kindle")]
fn snapshot_prefix(
    book: &mut Book,
    chapter: usize,
    user: &user_styles::Snapshot,
    current: &[Element],
) -> (Depths, usize) {
    let attempts = std::cell::Cell::new(0);
    let result = prefix(
        book,
        chapter,
        style_settings(BaseStylesheet::Traditional, None, true),
        user,
        None,
        "en",
        current,
        &|| {
            attempts.set(attempts.get() + 1);
            Ok(())
        },
    )
    .unwrap();
    (
        result.map_or_else(Depths::default, |incoming| incoming.depths),
        attempts.get(),
    )
}

#[cfg(feature = "kindle")]
#[test]
fn quote_marks_keep_the_snapshot_witness_while_rewinding_and_advancing() {
    let mut book = long_snapshot();
    let user = Default::default();
    for (chapter, checks) in [(20, 21), (1, 2), (2, 2), (10, 9), (20, 1), (11, 2)] {
        let (depths, checked) = snapshot_prefix(&mut book, chapter, &user, &[]);
        assert_eq!(checked, checks, "chapter {chapter}");
        assert_eq!(depths.0, HashSet::from([Box::<str>::from("en")]));
    }
    let mut revised = snapshot_with_member("seed.css", b".seed::before { content: none; }");
    assert_eq!(revised.revision(), book.revision());
    let (depths, checked) = snapshot_prefix(&mut revised, 20, &user, &[]);
    assert_eq!(checked, 21);
    assert!(depths.0.is_empty());
}

#[cfg(feature = "kindle")]
#[test]
fn quote_marks_snapshot_retry_keeps_note_and_style_profiles_separate() {
    let mut book = Book::open_snapshot(
        std::fs::read(crate::test_fixture("quote-spine.epub"))
            .unwrap()
            .into(),
    )
    .unwrap();
    let mut preferences = user_styles::Data::default();
    preferences
        .choose(1, user_styles::WIKIPEDIA_FOOTNOTES, Some(true))
        .unwrap();
    let wiki = preferences.snapshot(1).unwrap();
    let ordinary = wiki.without_wikipedia_notes();
    preferences
        .choose(1, "no_pseudo_element_before_after", Some(true))
        .unwrap();
    let disabled = preferences.snapshot(1).unwrap();
    for (profile, odd, checks) in [
        (&wiki, false, 2),
        (&ordinary, true, 2),
        (&disabled, false, 2),
        (&wiki, false, 1),
        (&ordinary, true, 1),
    ] {
        let (depths, actual) = snapshot_prefix(&mut book, 1, profile, &[]);
        assert_eq!(depths.0.contains("en"), odd);
        assert_eq!(actual, checks);
    }
}

#[cfg(feature = "kindle")]
#[test]
fn quote_marks_snapshot_cancel_and_bad_member_never_cache_partial_depths() {
    let settings = style_settings(BaseStylesheet::Traditional, None, true);
    let user = Default::default();
    let mut book = long_snapshot();
    let checks = std::cell::Cell::new(0);
    let error = prefix(&mut book, 20, settings, &user, None, "en", &[], &|| {
        checks.set(checks.get() + 1);
        if checks.get() >= 2 {
            return Err(std::io::Error::from(std::io::ErrorKind::Interrupted).into());
        }
        Ok(())
    })
    .err()
    .unwrap();
    assert_eq!(
        error.downcast_ref::<std::io::Error>().unwrap().kind(),
        std::io::ErrorKind::Interrupted
    );
    assert!(book.quote_prefixes.as_ref().unwrap().0.is_empty());
    let (retried, checks) = snapshot_prefix(&mut book, 20, &user, &[]);
    assert_eq!(checks, 21);
    assert!(retried.0.contains("en"));

    let mut broken = snapshot_with_member("chapter-02.xhtml", b"\xff");
    for _ in 0..2 {
        let (fallback, checks) = snapshot_prefix(&mut broken, 20, &user, &[]);
        assert_eq!(checks, 4);
        assert!(fallback.0.is_empty());
        assert!(broken.quote_prefixes.as_ref().unwrap().0.is_empty());
    }
    // Cancellation observed at the read-failure boundary still aborts a worker.
    let checks = std::cell::Cell::new(0);
    assert!(
        prefix(&mut broken, 20, settings, &user, None, "en", &[], &|| {
            checks.set(checks.get() + 1);
            if checks.get() >= 4 {
                return Err(std::io::Error::from(std::io::ErrorKind::Interrupted).into());
            }
            Ok(())
        })
        .is_err()
    );
    assert!(broken.quote_prefixes.as_ref().unwrap().0.is_empty());
}

#[cfg(feature = "kindle")]
#[test]
fn quote_marks_prune_large_language_sets_without_losing_requested_depths() {
    let mut many = r#"<html><head><link rel="stylesheet" href="seed.css" /></head><body><p><span class="seed">English seed</span><span class="seed" xml:lang="fr">French seed</span>"#.to_string();
    for index in 0..=MAX_CACHED_LANGUAGES {
        many.push_str(&format!(
            r#"<span class="seed" xml:lang="en-x-l{index}">Other seed</span>"#
        ));
    }
    many.push_str("</p></body></html>");
    let current = Source::parse(
        r#"<html><body><p><q>English</q><q xml:lang="fr">French</q></p></body></html>"#,
    )
    .unwrap();
    let mut book = snapshot_with_member("chapter-00.xhtml", many.as_bytes());
    for chapter in [20, 1, 20] {
        let (depths, checks) =
            snapshot_prefix(&mut book, chapter, &Default::default(), &current.elements);
        assert_eq!(checks, chapter + 1);
        assert_eq!(
            depths.0,
            HashSet::from([Box::<str>::from("en"), Box::<str>::from("fr")])
        );
        assert!(book.quote_prefixes.as_ref().unwrap().0.is_empty());
    }
}

#[cfg(feature = "kindle")]
#[test]
fn quote_worker_pruned_prefix_is_exact_only_for_its_target_and_profile() {
    let mut many = r#"<html><head><link rel="stylesheet" href="seed.css" /></head><body><p><span class="seed">English seed</span>"#.to_owned();
    for index in 0..=MAX_CACHED_LANGUAGES {
        many.push_str(&format!(
            r#"<span class="seed" xml:lang="en-x-l{index}">Other seed</span>"#
        ));
    }
    many.push_str("</p></body></html>");
    let settings = style_settings(BaseStylesheet::Traditional, None, true);
    let user = user_styles::Snapshot::default();
    let mut snapshot = snapshot_with_member("chapter-00.xhtml", many.as_bytes());
    let exact = prepare_exact(&mut snapshot, 20, settings, &user, None, "en", &|| Ok(())).unwrap();
    assert!(!exact.reusable);
    assert!(exact.depths.0.contains("en"));

    let mut foreground = snapshot_with_member("chapter-00.xhtml", many.as_bytes());
    foreground.quote_prefixes = Some(Box::new(Checkpoints::foreground()));
    foreground
        .quote_prefixes
        .as_mut()
        .unwrap()
        .install_exact(exact);
    take_historical_items();
    let incoming = scan_prefix(
        &mut foreground,
        20,
        settings,
        &user,
        None,
        "en",
        &[],
        &|| Ok(()),
        &|| true,
        false,
    )
    .unwrap()
    .unwrap();
    assert!(incoming.depths.0.contains("en"));
    assert!(!incoming.reusable);
    assert_eq!(take_historical_items(), 0);
    let other = scan_prefix(
        &mut foreground,
        19,
        settings,
        &user,
        None,
        "en",
        &[],
        &|| Ok(()),
        &|| true,
        false,
    )
    .unwrap();
    assert!(other.is_none());
    let other_profile = scan_prefix(
        &mut foreground,
        20,
        settings,
        &user,
        None,
        "fr",
        &[],
        &|| Ok(()),
        &|| true,
        false,
    )
    .unwrap();
    assert!(other_profile.is_none());
}

#[cfg(feature = "kindle")]
#[test]
fn quote_worker_deadline_bounds_repeated_large_linked_stylesheets() {
    let css = format!(
        ".seed::before {{ content: open-quote; }} /*{}*/",
        "x".repeat(90 * 1024)
    );
    let mut book = snapshot_with_member("seed.css", css.as_bytes());
    let calls = std::cell::Cell::new(0);
    take_historical_items();
    let result = scan_prefix(
        &mut book,
        20,
        style_settings(BaseStylesheet::Traditional, None, true),
        &user_styles::Snapshot::default(),
        None,
        "en",
        &[],
        &|| Ok(()),
        &|| {
            calls.set(calls.get() + 1);
            calls.get() >= 3
        },
        true,
    );
    let error = match result {
        Ok(_) => panic!("repeated stylesheet work must stop at the worker deadline"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("preparation time limit"));
    assert_eq!(take_historical_items(), 2);
    assert!(book.quote_prefixes.as_ref().unwrap().0.is_empty());
}
