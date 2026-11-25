use super::*;
use crate::library::DocumentFormat;
use crate::{Engine, Probe, Settings};
use std::io::{Cursor, Write};
use std::path::Path;

fn probe(path: &Path) -> Probe {
    chapter(path, 0)
}

fn chapter(path: &Path, chapter: usize) -> Probe {
    Probe::open(
        path,
        Engine::Parley,
        &[
            crate::test_fixture("fonts/NotoSans-Regular.ttf"),
            crate::test_fixture("fonts/NotoSans-Bold.ttf"),
        ],
        Settings {
            width: 650,
            height: 900,
            margin: 30,
            font_size: 26.,
            ..Default::default()
        },
        chapter,
    )
    .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

#[test]
fn fb3_large_books_keep_contents_and_local_note_links_across_section_groups() {
    let filler =
        "<p>A bounded paragraph keeps the original text in order near the harbor.</p>".repeat(3800);
    let source = format!(
        r#"<fb3-body xmlns="{FB3_BODY_NS}">
        <section id="harbor"><title><p>Harbor</p></title><p><spacing>Measured gap</spacing> <note href="note">Tide</note><a href="missing">Broken</a></p>{filler}</section>
        <section id="report"><title><p>Report</p></title>{filler}</section>
        <notes><notebody id="note"><p>Return to the <a href="harbor">harbor</a>.</p></notebody></notes>
        <script id="missing">Hidden test string</script></fb3-body>"#
    );
    let tree = parse_xml(&source).unwrap();
    let converted = body::convert(
        tree.root_element(),
        "Survey",
        None,
        None,
        &BTreeMap::new(),
        |_| None,
    )
    .unwrap();
    let [(first, opening), (second, closing)] = converted.chapters.as_slice() else {
        panic!("expected two bounded sections");
    };
    assert_eq!(first, crate::loose::CHAPTER_PATH);
    assert_eq!(second, "fb3-section-2.xhtml");
    assert!(opening.contains("<span>Measured gap</span>"));
    assert!(opening.contains("href=\"fb3-section-2.xhtml#note\" epub:type=\"noteref\""));
    assert!(opening.contains("<a>Broken</a>"));
    assert!(closing.contains("href=\"document.xhtml#harbor\""));
    assert!(!closing.contains("Hidden test string"));
    assert_eq!(converted.toc[1].destination.as_ref().unwrap().path, *second);
}

#[test]
#[cfg(feature = "kindle")]
fn fb3_split_note_stays_linked_and_is_not_copied_incompletely() {
    use crate::footnotes::in_page::Targets;
    use crate::user_styles::{Data, FB2_FOOTNOTES};

    let paragraph = "<p>A long note keeps every recorded sounding and observation intact for later reading.</p>";
    let source = format!(
        "<fb3-body xmlns=\"{FB3_BODY_NS}\"><section id=\"origin\"><p>Read the <note href=\"long-note\">long note</note>.</p></section><notes><notebody id=\"long-note\"><p>Opening sounding.</p>{}<p>Final sounding.</p></notebody></notes></fb3-body>",
        paragraph.repeat(5_200),
    );
    let tree = parse_xml(&source).unwrap();
    let converted = body::convert(
        tree.root_element(),
        "Survey",
        None,
        None,
        &BTreeMap::new(),
        |_| None,
    )
    .unwrap();
    assert!(converted.chapters.len() >= 2);
    assert_eq!(
        converted.incomplete_note_ids,
        BTreeSet::from(["long-note".to_owned()])
    );
    let (path, first) = &converted.chapters[0];
    assert_eq!(path, crate::loose::CHAPTER_PATH);
    assert!(first.contains("href=\"#long-note\" epub:type=\"noteref\""));
    // The out-of-band marker must not change XHTML or existing source anchors.
    assert!(first.contains("<aside id=\"long-note\" epub:type=\"footnote\">"));
    assert!(!first.contains("primitiveos-split"));
    assert!(first.contains("Opening sounding."));
    assert!(
        converted
            .chapters
            .last()
            .unwrap()
            .1
            .contains("Final sounding.")
    );
    let extracted = crate::Source::parse(first).unwrap();
    let mut styles = Data::default();
    styles.choose(1, FB2_FOOTNOTES, Some(true)).unwrap();
    let styles = styles.snapshot(1).unwrap();
    assert!(Targets::fb3(path, &extracted, &styles, &BTreeSet::new()).is_some());
    assert!(Targets::fb3(path, &extracted, &styles, &converted.incomplete_note_ids).is_none());
}

#[test]
fn fb3_legacy_readable_markup_keeps_exact_grouping_and_source() {
    // Each section exceeds half the newer raw-markup planning budget, while
    // their combined converted XHTML still fits the previous output limit.
    let runs = "<em xml:lang=\"en-US\">x</em>".repeat(15_000);
    let source = format!(
        "<fb3-body xmlns=\"{FB3_BODY_NS}\"><section id=\"first\"><title><p>West</p></title><p>{runs}</p></section><section><title><p>East</p></title><p>{runs}</p></section></fb3-body>"
    );
    let tree = parse_xml(&source).unwrap();
    let converted = body::convert(
        tree.root_element(),
        "Survey",
        None,
        None,
        &BTreeMap::new(),
        |_| None,
    )
    .unwrap();
    let [(path, xhtml)] = converted.chapters.as_slice() else {
        panic!("legacy-readable sections must remain in the original chapter");
    };
    assert_eq!(path, crate::loose::CHAPTER_PATH);
    let expected = format!(
        "<html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\"><head><title>Survey</title></head><body><section id=\"first\"><div><h2>West</h2></div><p>{runs}</p></section><section id=\"primitiveos-fb3-section-1\"><div><h2>East</h2></div><p>{runs}</p></section></body></html>"
    );
    assert_eq!(xhtml, &expected);
    assert_eq!(converted.toc.len(), 2);
    assert_eq!(converted.toc[1].destination.as_ref().unwrap().path, *path);
    assert_eq!(
        converted.toc[1]
            .destination
            .as_ref()
            .unwrap()
            .fragment
            .as_deref(),
        Some("primitiveos-fb3-section-1")
    );
}

#[test]
fn fb3_legacy_readable_single_paragraph_is_not_rejected_by_markup_planning() {
    let runs = "<em xml:lang=\"en-US\">x</em>".repeat(28_000);
    let source =
        format!("<fb3-body xmlns=\"{FB3_BODY_NS}\"><section><p>{runs}</p></section></fb3-body>");
    let tree = parse_xml(&source).unwrap();
    let converted = body::convert(
        tree.root_element(),
        "",
        None,
        None,
        &BTreeMap::new(),
        |_| None,
    )
    .unwrap();
    assert_eq!(converted.chapters.len(), 1);
    assert!(converted.chapters[0].1.contains(&format!("<p>{runs}</p>")));
}

#[test]
fn fb3_output_limit_retry_splits_sections_without_repeating_asset_loads() {
    let language = format!("en-{}", "a".repeat(58));
    let run = format!("<em xml:lang=\"{language}\">x</em>");
    let runs = run.repeat(6_500);
    let source = format!(
        "<fb3-body xmlns=\"{FB3_BODY_NS}\"><section id=\"first\"><img src=\"pic\" alt=\"Beacon\"/><p>{runs}</p></section><section id=\"second\"><p>{runs}</p></section></fb3-body>"
    );
    let tree = parse_xml(&source).unwrap();
    let images = BTreeMap::from([("pic".to_owned(), "images/pic".to_owned())]);
    let loads = std::cell::Cell::new(0);
    let converted = body::convert(tree.root_element(), "", None, None, &images, |_| {
        loads.set(loads.get() + 1);
        Some("assets/pic.png".into())
    })
    .unwrap();
    assert_eq!(loads.get(), 1);
    assert_eq!(converted.chapters.len(), 2);
    assert!(converted.chapters[0].1.contains("alt=\"Beacon\""));
    assert!(converted.chapters[0].1.contains("id=\"first\""));
    assert!(converted.chapters[1].1.contains("id=\"second\""));
}

#[test]
fn fb3_nested_continuation_keeps_structure_destinations_and_late_reading_positions() {
    let path = crate::test_fixture("harbor-continuation.fb3");
    let before = std::fs::read(&path).unwrap();
    let mut book = Book::open(&path).unwrap();
    assert_eq!(book.chapters.len(), 2);
    let opening = book.chapter(0).unwrap();
    let continuation = book.chapter(1).unwrap();
    let last = &book.chapters[1].href;
    assert_eq!(last, "fb3-section-2.xhtml");
    assert_eq!(
        book.toc
            .iter()
            .map(|entry| (&*entry.label, entry.depth))
            .collect::<Vec<_>>(),
        [
            ("Outer Survey Log", 0),
            ("Long Soundings", 1),
            ("Shore Register", 1),
        ]
    );
    let dest = |index: usize| book.toc[index].destination.as_ref().unwrap();
    assert_eq!(
        dest(0).fragment.as_deref(),
        Some("primitiveos-fb3-section-2")
    );
    assert_eq!(dest(1).fragment.as_deref(), Some("soundings"));
    assert_eq!(
        dest(2).fragment.as_deref(),
        Some("primitiveos-fb3-section-1")
    );
    assert_eq!(&dest(2).path, last);
    for target in ["primitiveos-fb3-section-2", "soundings", "opening"] {
        assert_eq!(opening.matches(&format!("id=\"{target}\"")).count(), 1);
        assert!(!continuation.contains(&format!("id=\"{target}\"")));
    }
    assert!(continuation.contains("<section xml:lang=\"en\"><section xml:lang=\"en-GB\"><blockquote xml:lang=\"en-GB\" class=\"fb2-epigraph\">"));
    assert!(opening.contains("href=\"fb3-section-2.xhtml#late\""));
    assert!(opening.contains("href=\"fb3-section-2.xhtml#later-note\" epub:type=\"noteref\""));
    assert_eq!(
        continuation
            .matches("href=\"document.xhtml#opening\"")
            .count(),
        2
    );
    assert!(!continuation.contains("Outer Survey Log") && !continuation.contains("Long Soundings"));
    assert_eq!(
        opening.matches("The lantern remains lit").count()
            + continuation.matches("The lantern remains lit").count(),
        4290
    );
    let markers = [
        "The final sounding",
        "The shore register",
        "The note records",
    ];
    assert!(
        markers
            .windows(2)
            .all(|pair| continuation.find(pair[0]).unwrap() < continuation.find(pair[1]).unwrap())
    );

    let source = book.chapter_source(last, &continuation).unwrap();
    let sounding = source
        .blocks
        .iter()
        .find(|block| block.text.contains(markers[0]))
        .unwrap();
    assert_eq!(
        sounding.language.as_ref().and_then(|tag| tag.region()),
        Some("GB")
    );
    let view = chapter(&path, 1);
    let at = view.matches(markers[0]).pop().unwrap();
    assert!(view.fragment("late").is_some() && view.fragment("later-note").is_some());
    let mut reopened = chapter(&path, 1);
    assert_eq!(reopened.restore_anchor(&at), Some(at.clone()));
    let page = reopened.page_for(&at).unwrap();
    let pixels = reopened.capture(page).unwrap();
    assert!(pixels.iter().any(|&pixel| pixel < 128));
    assert_eq!(std::fs::read(path).unwrap(), before);
    #[cfg(feature = "kindle")]
    if let Some(directory) = std::env::var_os("READER_FB3_CAPTURES") {
        primitive_app::write_png(
            &Path::new(&directory).join("reader-fb3-harbor-continuation.png"),
            reopened.settings.width,
            reopened.settings.height,
            &pixels,
        )
        .unwrap();
    }
}

#[test]
fn fb3_rejects_an_indivisible_block_over_the_chapter_budget() {
    let source = format!(
        "<fb3-body xmlns=\"{FB3_BODY_NS}\"><section><p>{}</p></section></fb3-body>",
        "x".repeat(384 * 1024 + 1)
    );
    let tree = parse_xml(&source).unwrap();
    let err = body::convert(
        tree.root_element(),
        "Oversized",
        None,
        None,
        &BTreeMap::new(),
        |_| None,
    )
    .err()
    .unwrap();
    assert!(err.to_string().contains("indivisible FB3 block"));
}

#[test]
fn fb3_opens_as_a_reflow_document_with_contents_images_notes_and_saved_locations() {
    let path = crate::test_fixture("harbor-survey.fb3");
    let before = std::fs::read(&path).unwrap();
    assert_eq!(DocumentFormat::for_path(&path), Some(DocumentFormat::Fb3));
    assert_eq!(
        DocumentFormat::for_path(Path::new("Notes.FB3")),
        Some(DocumentFormat::Fb3)
    );
    let mut book = Book::open(&path).unwrap();
    assert_eq!(book.title, "Harbor Survey");
    assert_eq!(book.author.as_deref(), Some("Mara Vale"));
    assert_eq!(book.language.unwrap().language(), "en");
    assert_eq!(book.cover().unwrap().path, "fb3-images/1.png");
    assert_eq!(
        book.publisher_metadata().unwrap().language.as_deref(),
        Some("en")
    );
    assert_eq!(book.toc.len(), 2);
    assert_eq!(
        (&*book.toc[0].label, book.toc[0].depth),
        ("Morning Soundings", 0)
    );
    assert_eq!(
        (&*book.toc[1].label, book.toc[1].depth),
        ("Ridge Observations", 1)
    );
    assert_eq!(
        book.toc[1]
            .destination
            .as_ref()
            .unwrap()
            .fragment
            .as_deref(),
        Some("ridge")
    );
    let xhtml = book.chapter(0).unwrap();
    assert!(xhtml.contains("<img src=\"fb3-images/1.png\" alt=\"Cover\""));
    assert!(xhtml.contains("<img src=\"fb3-images/2.png\" alt=\"Harbor signal diagram\""));
    assert!(xhtml.contains("Remote chart unavailable") && !xhtml.contains("example.invalid"));
    assert!(xhtml.contains("<th colspan=\"2\">") && xhtml.contains("<ol start=\"3\">"));
    assert!(xhtml.contains("<a href=\"#tide-note\" epub:type=\"noteref\">"));
    assert_eq!(
        image::load_from_memory(&book.resource("fb3-images/2.png").unwrap())
            .unwrap()
            .width(),
        46
    );

    let mut first = probe(&path);
    assert!(first.page_count() > 1);
    let passage = first.matches("The silver ridge").pop().unwrap();
    assert!(first.fragment("tide-note").is_some());
    let page = first.page_for(&passage).unwrap();
    let pixels = first.capture(page).unwrap();
    let mut reopened = probe(&path);
    assert_eq!(reopened.restore_anchor(&passage), Some(passage));
    assert_eq!(reopened.capture(page).unwrap(), pixels);
    assert_eq!(std::fs::read(path).unwrap(), before);
    #[cfg(feature = "kindle")]
    if let Some(directory) = std::env::var_os("READER_FB3_CAPTURES") {
        let opening = first.capture(0).unwrap();
        primitive_app::write_png(
            &Path::new(&directory).join("reader-fb3-harbor-survey.png"),
            first.settings.width,
            first.settings.height,
            &opening,
        )
        .unwrap();
    }
}

fn package(parts: &[(&str, &str)]) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for &(name, contents) in parts {
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(contents.as_bytes()).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

#[test]
fn fb3_rejects_ambiguous_parts_and_escaping_targets_before_reflow() {
    let duplicate = package(&[("book/k%65y", "a"), ("book/KEY", "b")]);
    let err = Package::open(Cursor::new(&duplicate), duplicate.len() as u64)
        .err()
        .unwrap();
    assert!(err.to_string().contains("duplicate part names"));
    for target in [
        "../../elsewhere",
        "https://example.invalid/x",
        "images/a%2fb",
        "/../outside",
    ] {
        assert!(
            package_path("chapters/body.xml", target).is_err(),
            "{target}"
        );
    }
    let marker = format!(
        "<Types xmlns=\"{TYPES_NS}\"><Override PartName=\"/body\" ContentType=\"{BODY_TYPE}\"/><Override PartName=\"/other\" ContentType=\"{BODY_TYPE}\"/></Types>"
    );
    let bytes = package(&[("[Content_Types].xml", &marker)]);
    let err = open(Cursor::new(&bytes), bytes.len() as u64, "Missing chapters")
        .err()
        .unwrap();
    assert!(err.to_string().contains("more than one part"));
}

#[test]
#[ignore = "Requires the format author's three unchanged public specimens in READER_FB3_ORIGINALS"]
fn fb3_real_upstream_publications_open_paint_turn_and_keep_their_original_bytes() {
    let dir = std::env::var_os("READER_FB3_ORIGINALS").unwrap();
    for name in ["anatomy.fb3", "part-names.fb3", "nightmare.fb3"] {
        let path = Path::new(&dir).join(name);
        let before = std::fs::read(&path).unwrap();
        let book = Book::open(&path).unwrap_or_else(|error| panic!("{name}: {error}"));
        assert!(!book.toc.is_empty(), "{name}");
        #[cfg(feature = "kindle")]
        {
            let docs = Path::new(&dir);
            let scratch = docs
                .parent()
                .unwrap()
                .join(format!("reader-ui-{}-{name}", std::process::id()));
            crate::document_check::verify(
                docs,
                &path,
                &scratch,
                vec![
                    crate::test_fixture("fonts/NotoSans-Regular.ttf"),
                    crate::test_fixture("fonts/NotoSans-Bold.ttf"),
                ],
                true,
            )
            .unwrap_or_else(|error| panic!("{name} picker: {error}"));
            std::fs::remove_dir_all(&scratch).unwrap();
        }
        let mut view = probe(&path);
        assert!(view.page_count() > 2, "{name}");
        let opening = view.capture(0).unwrap();
        let following = view.capture(1).unwrap();
        assert_ne!(opening, following, "{name}");
        assert!(opening.iter().any(|&pixel| pixel < 128), "{name}");
        let anchor = view.page_anchor(1).unwrap();
        let mut reopened = probe(&path);
        assert_eq!(reopened.restore_anchor(&anchor), Some(anchor));
        assert_eq!(reopened.capture(1).unwrap(), following);
        for index in 1..book.chapters.len() {
            let mut section = chapter(&path, index);
            assert!(section.capture(0).unwrap().iter().any(|&pixel| pixel < 128));
        }
        assert_eq!(std::fs::read(path).unwrap(), before);
        eprintln!(
            "{name}: {} contents, {} sections, {} rendered opening-section pages",
            book.toc.len(),
            book.chapters.len(),
            view.page_count()
        );
    }
}
