use super::*;
use crate::{Engine, Probe, Settings, test_fixture};

fn foreign_fixture() -> (Book, String, Source) {
    let mut book = Book::open(&test_fixture("harbor-continuation.fb3")).unwrap();
    let chapter = book.chapters[0].href.clone();
    let xhtml = book.chapter(0).unwrap();
    let source = book.chapter_source(&chapter, &xhtml).unwrap();
    (book, chapter, source)
}

#[test]
fn cross_part_candidate_keeps_the_foreign_source_revision_and_anchor_keys() {
    let path = test_fixture("harbor-continuation.fb3");
    let before = std::fs::read(&path).unwrap();
    let (mut book, chapter, host) = foreign_fixture();
    let plan = prepare(&mut book, &chapter, &host).unwrap();
    assert_eq!(plan.notes.len(), 1);
    assert_eq!(plan.links.len(), 1);
    let note = &plan.notes[plan.links[0].note];
    assert_eq!(note.id, "later-note");
    assert_eq!(note.chapter, "fb3-section-2.xhtml");
    assert_eq!(note.book, book.revision());
    let phrase = "The note records a calm sea";
    let block = note
        .source
        .blocks
        .iter()
        .find(|block| block.text.contains(phrase))
        .unwrap();
    assert!(note.blocks.contains(&block.key));
    let offset = block.text.find(phrase).unwrap();
    let index = book.chapter_index(&note.chapter).unwrap();
    let probe = Probe::open(
        &path,
        Engine::Parley,
        &[test_fixture("fonts/NotoSans-Regular.ttf")],
        Settings::default(),
        index,
    )
    .unwrap();
    let anchor = probe.matches(phrase).pop().unwrap();
    assert_eq!(anchor.book, note.book);
    assert_eq!(anchor.chapter, note.chapter);
    assert_eq!(anchor.revision, note.source.revision);
    assert_eq!((anchor.block, anchor.offset), (block.key, offset));
    assert_eq!(std::fs::read(path).unwrap(), before);
}

#[test]
fn cross_part_candidates_are_bounded_and_only_explicit_body_links_count() {
    let (mut book, chapter, _) = foreign_fixture();
    let link = "<a epub:type=\"noteref\" href=\"fb3-section-2.xhtml#later-note\">note</a>";
    let xhtml = format!(
        "<html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\"><body><p>{}</p><aside epub:type=\"footnote\"><p>{link}</p></aside></body></html>",
        link.repeat(MAX_REFERENCES)
    );
    let host = Source::parse(&xhtml).unwrap();
    let plan = prepare(&mut book, &chapter, &host).unwrap();
    assert_eq!(plan.notes.len(), 1);
    assert_eq!(plan.links.len(), MAX_REFERENCES);
    assert!(plan.links.iter().all(|link| link.note == 0));
    let host_block = host
        .blocks
        .iter()
        .find(|block| block.key == plan.links[0].host_block)
        .unwrap();
    assert_eq!(&host_block.text[plan.links[0].bytes.clone()], "note");

    let overflow = xhtml.replace("</p>", &format!("{link}</p>"));
    assert!(prepare(&mut book, &chapter, &Source::parse(&overflow).unwrap()).is_none());

    let ordinary = xhtml.replace("epub:type=\"noteref\"", "epub:type=\"link\"");
    assert!(prepare(&mut book, &chapter, &Source::parse(&ordinary).unwrap()).is_none());
}

fn note(xhtml: &str) -> (Source, usize) {
    let xml = format!(
        "<html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\"><body><p>Body text.</p><aside id=\"n\" epub:type=\"footnote\">{xhtml}</aside></body></html>"
    );
    let source = Source::parse(&xml).unwrap();
    let root = source.fragment_owner("n").unwrap();
    (source, root)
}

#[test]
fn cross_part_candidate_rejects_rich_empty_nested_and_overlarge_notes() {
    let (source, root) = note("<p>Simple <em>text</em> and a <a href=\"#back\">backlink</a>.</p>");
    assert!(simple_note(&source, root, "n").is_some());
    for content in [
        "<p><img src=\"pic.png\" alt=\"picture\"/></p>",
        "<p><ruby>base<rt>reading</rt></ruby></p>",
        "<table><tr><td>cell</td></tr></table>",
        "<ul><li>item</li></ul>",
        "<p>Nested <a epub:type=\"noteref\" href=\"#other\">note</a>.</p>",
        "<p> </p>",
    ] {
        let (source, root) = note(content);
        assert!(simple_note(&source, root, "n").is_none(), "{content}");
    }
    let (source, root) = note(&format!("<p>{}</p>", "x".repeat(MAX_NOTE_BYTES + 1)));
    assert!(simple_note(&source, root, "n").is_none());
}

#[test]
fn cross_part_candidate_rejects_links_beyond_the_listing_budget() {
    let link = "<a href=\"#back\">return</a> ";
    let (supported, root) = note(&format!("<p>{}</p>", link.repeat(MAX_NOTE_LINKS)));
    assert!(simple_note(&supported, root, "n").is_some());
    let (overflow, root) = note(&format!("<p>{}</p>", link.repeat(MAX_NOTE_LINKS + 1)));
    assert!(simple_note(&overflow, root, "n").is_none());
}

#[test]
fn cross_part_candidate_keeps_same_part_references_in_ordinary_flow() {
    let mut book = Book::open(&test_fixture("harbor-survey.fb3")).unwrap();
    let chapter = book.chapters[0].href.clone();
    let xhtml = book.chapter(0).unwrap();
    let host = book.chapter_source(&chapter, &xhtml).unwrap();
    assert!(prepare(&mut book, &chapter, &host).is_none());
}

struct Temporary(std::path::PathBuf);

impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn quiet_fixture_with_replacement(target: &str, replacement: &str) -> Temporary {
    use std::io::{Read, Write};
    use std::time::{SystemTime, UNIX_EPOCH};

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = Temporary(std::env::temp_dir().join(format!(
        "reader-fb3-foreign-note-{}-{unique}.fb3",
        std::process::id()
    )));
    let original = std::fs::File::open(test_fixture("cross-note-quiet.fb3")).unwrap();
    let mut input = zip::ZipArchive::new(original).unwrap();
    let output = std::fs::File::create(&path.0).unwrap();
    let mut output = zip::ZipWriter::new(output);
    for index in 0..input.len() {
        let mut entry = input.by_index(index).unwrap();
        let name = entry.name().to_owned();
        let options =
            zip::write::SimpleFileOptions::default().compression_method(entry.compression());
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).unwrap();
        if name == "body.xml" {
            let body = String::from_utf8(bytes).unwrap();
            assert_eq!(body.matches(target).count(), 1);
            bytes = body.replace(target, replacement).into_bytes();
        }
        output.start_file(name, options).unwrap();
        output.write_all(&bytes).unwrap();
    }
    output.finish().unwrap();
    path
}

#[test]
fn cross_part_candidate_uses_effective_language_and_embedded_tag_policy() {
    let path = quiet_fixture_with_replacement(
        "<notebody id=\"rain-note\">",
        "<notebody id=\"rain-note\" xml:lang=\"fr\">",
    );
    let mut book = Book::open(&path.0).unwrap();
    let chapter = book.chapters[0].href.clone();
    let xhtml = book.chapter(0).unwrap();
    let host = book.chapter_source(&chapter, &xhtml).unwrap();
    let mut settings = Settings::default();
    settings.typography.main = crate::TypographyLocale::parse("de");
    settings.typography.respect_embedded = true;
    let embedded = prepare_for_settings(&mut book, &chapter, &host, settings)
        .unwrap()
        .notes
        .pop()
        .unwrap();
    let language = |note: &Note| {
        note.source
            .blocks
            .iter()
            .find(|block| block.text.contains("Rainfall reached"))
            .and_then(|block| block.language)
            .map(|language| language.as_str().to_owned())
    };
    assert_eq!(language(&embedded).as_deref(), Some("fr"));
    let mut without_embedded = settings;
    without_embedded.typography.respect_embedded = false;
    let main = prepare_for_settings(&mut book, &chapter, &host, without_embedded)
        .unwrap()
        .notes
        .pop()
        .unwrap();
    assert_eq!(language(&main).as_deref(), Some("de"));

    let mut data = crate::user_styles::Data::default();
    data.choose(1, crate::user_styles::FB2_FOOTNOTES, Some(true))
        .unwrap();
    let styles = data.snapshot(1).unwrap();
    let fonts = || crate::font_context(&[test_fixture("fonts/NotoSans-Regular.ttf")]).unwrap();
    assert!(
        embedded
            .layout(
                &mut book,
                fonts(),
                without_embedded,
                &styles,
                &Default::default()
            )
            .is_none()
    );
    assert!(
        main.layout(
            &mut book,
            fonts(),
            without_embedded,
            &styles,
            &Default::default()
        )
        .is_some()
    );
}

#[test]
fn cross_part_layout_falls_back_for_margin_bleed_modes() {
    let (mut book, chapter, host) = foreign_fixture();
    let mut data = crate::user_styles::Data::default();
    data.choose(1, crate::user_styles::FB2_FOOTNOTES, Some(true))
        .unwrap();
    let styles = data.snapshot(1).unwrap();
    for settings in [
        Settings {
            publisher_signed_horizontal_margins: true,
            ..Settings::default()
        },
        Settings {
            fit_content: crate::FitContentMode::Web,
            ..Settings::default()
        },
        Settings {
            hanging_punctuation: true,
            ..Settings::default()
        },
    ] {
        let note = prepare_for_settings(&mut book, &chapter, &host, settings)
            .unwrap()
            .notes
            .pop()
            .unwrap();
        let fonts = crate::font_context(&[test_fixture("fonts/NotoSans-Regular.ttf")]).unwrap();
        assert!(
            note.layout(&mut book, fonts, settings, &styles, &Default::default())
                .is_none()
        );
        // The canonical target remains available for ordinary link navigation.
        assert!(book.chapter_index("fb3-section-2.xhtml").is_some());
    }
}

#[test]
fn cross_part_separator_follows_rtl_majority_and_keeps_left_for_a_tie() {
    let original = "<p>Rainfall reached six millimeters overnight. <a l:href=\"#dispatch\">Return to dispatch</a>.</p>";
    let cases = [
        (
            "<p xml:lang=\"ar\">تسجل الملاحظات موج البحر والهواء.</p>",
            true,
        ),
        (
            "<p xml:lang=\"ar\">تسجل الملاحظات موج البحر.</p><p xml:lang=\"en\">The paired line is calm.</p>",
            false,
        ),
    ];
    for (replacement, right) in cases {
        let path = quiet_fixture_with_replacement(original, replacement);
        let mut book = Book::open(&path.0).unwrap();
        let chapter = book.chapters[0].href.clone();
        let xhtml = book.chapter(0).unwrap();
        let host = book.chapter_source(&chapter, &xhtml).unwrap();
        let settings = Settings {
            width: 720,
            height: 480,
            margin: 32,
            font_size: 25.0,
            line_height: 1.2,
            ..Settings::default()
        };
        let mut data = crate::user_styles::Data::default();
        data.choose(1, crate::user_styles::FB2_FOOTNOTES, Some(true))
            .unwrap();
        let note = prepare_for_settings(&mut book, &chapter, &host, settings)
            .unwrap()
            .notes
            .pop()
            .unwrap();
        let fonts = crate::font_context(&[
            test_fixture("fonts/NotoSans-Regular.ttf"),
            test_fixture("fonts/NotoSansArabic-Regular.ttf"),
        ])
        .unwrap();
        let layout = note
            .layout(
                &mut book,
                fonts,
                settings,
                &data.snapshot(1).unwrap(),
                &Default::default(),
            )
            .unwrap();
        assert_eq!(layout.lines.len(), if right { 1 } else { 2 });
        assert_eq!(layout.separator_on_right, right);
        let mut pixels = vec![255; settings.width as usize * settings.height as usize];
        let mut band = Band::new(0, layout, &pixels).unwrap();
        assert!(band.paint(&mut pixels).unwrap());
        let width = settings.width as usize;
        let y = (settings.top_margin() + band.placements[0].separator.unwrap().0) as usize;
        let row = &pixels[y * width..(y + 1) * width];
        let rule = |left: usize| row[left..left + width / 7].iter().all(|&ink| ink == 85);
        assert_eq!(rule(settings.left_margin() as usize), !right);
        assert_eq!(
            rule(width - settings.right_margin() as usize - width / 7),
            right
        );
    }
}

#[test]
fn cross_part_layout_uses_fictionbook_size_and_canonical_foreign_anchors() {
    use crate::user_styles::{Data, FB2_FOOTNOTES, FB2_REGULAR_NOTE_SIZE};

    let (mut book, chapter, host) = foreign_fixture();
    let path = test_fixture("harbor-continuation.fb3");
    let before = std::fs::read(&path).unwrap();
    let settings = Settings {
        width: 720,
        height: 480,
        margin: 32,
        font_size: 25.0,
        line_height: 1.2,
        ..Settings::default()
    };
    let fonts = || crate::font_context(&[test_fixture("fonts/NotoSans-Regular.ttf")]).unwrap();
    let style = |regular: bool| {
        let mut data = Data::default();
        data.choose(1, FB2_FOOTNOTES, Some(true)).unwrap();
        if regular {
            data.choose(1, FB2_REGULAR_NOTE_SIZE, Some(true)).unwrap();
        }
        data.snapshot(1).unwrap()
    };
    let make = |book: &mut Book, regular| {
        let note = prepare(book, &chapter, &host).unwrap().notes.pop().unwrap();
        note.layout(
            book,
            fonts(),
            settings,
            &style(regular),
            &Default::default(),
        )
        .unwrap()
    };
    let mut small = make(&mut book, false);
    let regular = make(&mut book, true);
    assert!((small.root_scale - 0.75).abs() < 0.001);
    assert!((regular.root_scale - 1.0).abs() < 0.001);
    assert!(small.extent < regular.extent);
    assert!(!small.lines.is_empty());
    let pixels = small.renderer.capture(0.0..small.extent, settings).unwrap();
    assert!(pixels.iter().any(|&pixel| pixel < 128));
    let line = &small.lines[0];
    let mut anchor = None;
    for y in line.top.floor().max(0.0) as usize..line.bottom.ceil() as usize {
        for x in 0..settings.text_width() as usize {
            if let Some(found) = small.anchor_at(x as f32 + 0.5, y as f32 + 0.5) {
                anchor = Some(found);
                break;
            }
        }
        if anchor.is_some() {
            break;
        }
    }
    let anchor = anchor.expect("foreign note must expose source hit testing");
    assert_eq!(anchor.chapter, small.note.chapter);
    assert_eq!(anchor.revision, small.note.source.revision);
    let target = book.chapter_index(&small.note.chapter).unwrap();
    let probe = Probe::open(
        &path,
        Engine::Parley,
        &[test_fixture("fonts/NotoSans-Regular.ttf")],
        settings,
        target,
    )
    .unwrap();
    assert_eq!(probe.restore_anchor(&anchor), Some(anchor));
    let block = small
        .note
        .source
        .blocks
        .iter()
        .find(|block| block.key == small.note.blocks[0])
        .unwrap();
    assert!(
        small
            .flow
            .view(&small.note.source, block)
            .unwrap()
            .source_is_present(0)
    );
    assert_eq!(std::fs::read(path).unwrap(), before);
}

#[test]
fn cross_part_layout_rejects_note_extension_and_inline_all() {
    use crate::user_styles::{Data, EXTEND_NOTES, FB2_FOOTNOTES, INLINE_ALL_FOOTNOTE_CONTENT};

    let (mut book, chapter, host) = foreign_fixture();
    for extra in [EXTEND_NOTES, INLINE_ALL_FOOTNOTE_CONTENT] {
        let mut data = Data::default();
        data.choose(1, FB2_FOOTNOTES, Some(true)).unwrap();
        data.choose(1, extra, Some(true)).unwrap();
        let note = prepare(&mut book, &chapter, &host)
            .unwrap()
            .notes
            .pop()
            .unwrap();
        let fonts = crate::font_context(&[test_fixture("fonts/NotoSans-Regular.ttf")]).unwrap();
        assert!(
            note.layout(
                &mut book,
                fonts,
                Settings::default(),
                &data.snapshot(1).unwrap(),
                &Default::default()
            )
            .is_none()
        );
    }
}

#[test]
fn cross_part_band_never_covers_host_pixels_and_uses_foreign_link_base() {
    use crate::user_styles::{Data, FB2_FOOTNOTES};

    let (mut book, chapter, host) = foreign_fixture();
    let settings = Settings {
        width: 720,
        height: 480,
        margin: 32,
        font_size: 25.0,
        line_height: 1.2,
        ..Settings::default()
    };
    let mut data = Data::default();
    data.choose(1, FB2_FOOTNOTES, Some(true)).unwrap();
    let styles = data.snapshot(1).unwrap();
    let make = |book: &mut Book| {
        let note = prepare(book, &chapter, &host).unwrap().notes.pop().unwrap();
        let fonts = crate::font_context(&[test_fixture("fonts/NotoSans-Regular.ttf")]).unwrap();
        note.layout(book, fonts, settings, &styles, &Default::default())
            .unwrap()
    };
    let mut host_pixels = vec![255; settings.width as usize * settings.height as usize];
    let mut band = Band::new(3, make(&mut book), &host_pixels).unwrap();
    assert!(band.add_page(5, &host_pixels));
    assert!(band.displays(3) && band.displays(5));
    let mut occupied = host_pixels.clone();
    let occupied_at =
        (settings.top_margin() + band.placements[0].clear_from) as usize * settings.width as usize;
    occupied[occupied_at] = 0;
    assert!(!band.add_page(7, &occupied));
    band.remove_page(3);
    assert!(!band.displays(3) && band.displays(5));
    assert!(band.add_page(3, &host_pixels));
    assert!(band.paint(&mut host_pixels).unwrap());
    assert!(host_pixels.iter().any(|&pixel| pixel < 128));
    let top = settings.top_margin() + band.y_offset(3);
    assert!(band.contains(3, 0.0, top as f32));
    assert!(!band.contains(2, 0.0, top as f32));
    assert!(band.coordinates(3, 0.0, top as f32).is_none());

    let line = &band.layout.lines[0];
    let mut found = None;
    for y in line.top.floor().max(0.0) as u32..line.bottom.ceil() as u32 {
        for x in settings.left_margin()..settings.width - settings.right_margin() {
            if let Some(link) = band.link_at(3, x as f32 + 0.5, (top + y) as f32 + 0.5) {
                found = Some((link.0.to_owned(), link.1.to_owned()));
                break;
            }
        }
        if found.is_some() {
            break;
        }
    }
    let (base, href) = found.expect("backlink must hit in the foreign note");
    assert_eq!(base, "fb3-section-2.xhtml");
    let target = Book::resolve_link(&base, &href).unwrap();
    assert_eq!(target.path, "document.xhtml");
    assert_eq!(target.fragment.as_deref(), Some("opening"));

    let mut changed = vec![255; host_pixels.len()];
    let mut unpainted = Band::new(0, make(&mut book), &changed).unwrap();
    let collision = (settings.top_margin() + unpainted.placements[0].clear_from) as usize
        * settings.width as usize;
    changed[collision] = 0;
    let before = changed.clone();
    assert!(!unpainted.paint(&mut changed).unwrap());
    assert_eq!(changed, before);
    assert!(Band::new(0, make(&mut book), &changed).is_none());
}

#[test]
fn cross_part_multiple_reference_pages_keep_original_note_sources() {
    use crate::user_styles::{Data, FB2_FOOTNOTES};
    let path = test_fixture("cross-note-multiple.fb3");
    let before = std::fs::read(&path).unwrap();
    let settings = Settings {
        width: 720,
        height: 480,
        margin: 32,
        font_size: 28.0,
        line_height: 1.2,
        ..Settings::default()
    };
    let open = |separate| {
        let mut data = Data::default();
        data.choose(1, FB2_FOOTNOTES, Some(true)).unwrap();
        if separate {
            let book = data.books.entry(1).or_default();
            book.custom = Some("#later, #again { break-before: page !important; }".into());
            book.custom_enabled = true;
        }
        let mut book = Some(Book::open(&path).unwrap());
        Probe::layout_opened_book(
            &mut book,
            Engine::Parley,
            settings,
            0,
            None,
            &Default::default(),
            &data.snapshot(1).unwrap(),
            || crate::font_context(&[test_fixture("fonts/NotoSans-Regular.ttf")]),
        )
        .unwrap()
    };
    let mut same = open(false);
    let first = same.matches("rain one").pop().unwrap();
    let other = same.matches("snow note").pop().unwrap();
    let shared_page = same.page_for(&first).unwrap();
    assert_eq!(same.page_for(&other), Some(shared_page));
    let bands = same.foreign_bands(shared_page);
    assert_eq!(bands.len(), 2, "both distinct notes must be stacked once");
    assert_eq!(bands[0].layout.note.id, "rain-note");
    assert_eq!(bands[1].layout.note.id, "snow-note");
    assert!(bands[0].y_offset(shared_page) < bands[1].y_offset(shared_page));
    assert!(
        bands
            .iter()
            .all(|band| !band.layout.note_links().is_empty())
    );
    for band in &bands {
        let (_, href, rects) = band.layout.note_links().into_iter().next().unwrap();
        let rect = rects.first().unwrap();
        let x = (rect.x + rect.width / 2) as f32;
        let y = (rect.y + band.y_offset(shared_page) as i32 + rect.height / 2) as f32;
        let destination = same.link_at(shared_page, x, y).unwrap();
        assert_eq!(
            destination,
            Book::resolve_link(&band.layout.note.chapter, href).unwrap()
        );
        let selection = same.word_at(shared_page, x, y).unwrap();
        assert_eq!(selection.start.chapter, band.layout.note.chapter);
        assert!(!same.selection_rects(shared_page, &selection).is_empty());
    }
    assert_eq!(same.foreign_bands(shared_page).len(), 2);
    assert!(same.capture(shared_page).is_ok());
    assert_eq!(same.foreign_bands(shared_page).len(), 2);

    let mut separate = open(true);
    let first = separate.matches("rain one").pop().unwrap();
    let repeat = separate.matches("rain two").pop().unwrap();
    let other = separate.matches("snow note").pop().unwrap();
    let again = separate.matches("rain three").pop().unwrap();
    let rain_page = separate.page_for(&first).unwrap();
    let snow_page = separate.page_for(&other).unwrap();
    let again_page = separate.page_for(&again).unwrap();
    assert_eq!(separate.page_for(&repeat), Some(rain_page));
    assert_ne!(rain_page, snow_page);
    assert_ne!(again_page, snow_page);
    assert_ne!(again_page, rain_page);
    let rain = separate
        .foreign_band(rain_page)
        .expect("repeated note must appear once");
    let snow = separate
        .foreign_band(snow_page)
        .expect("different page must show its note");
    assert_eq!(rain.layout.note.id, "rain-note");
    assert_eq!(snow.layout.note.id, "snow-note");
    assert_eq!(
        separate.foreign_band(again_page).unwrap().layout.note.id,
        "rain-note"
    );
    assert_eq!(rain.layout.note.chapter, snow.layout.note.chapter);
    let block = rain
        .layout
        .note
        .source
        .blocks
        .iter()
        .find(|block| block.text.contains("Rainfall"))
        .unwrap();
    let start = Anchor {
        raw_caret: None,
        book: rain.layout.note.book,
        chapter: rain.layout.note.chapter.clone(),
        revision: rain.layout.note.source.revision,
        block: block.key,
        offset: block.text.find("Rainfall").unwrap(),
        illustration: None,
    };
    let mut end = start.clone();
    end.offset += "Rainfall".len();
    assert!(separate.select_range(&start, &end).is_some());
    assert_ne!(
        separate.capture(rain_page).unwrap(),
        separate.capture(snow_page).unwrap()
    );
    assert_eq!(std::fs::read(path).unwrap(), before);
}

#[test]
fn cross_part_probe_displays_a_quiet_page_without_changing_progress_or_anchors() {
    use crate::user_styles::{Data, FB2_FOOTNOTES, FB2_REGULAR_NOTE_SIZE};

    let path = test_fixture("cross-note-quiet.fb3");
    let before = std::fs::read(&path).unwrap();
    let settings = Settings {
        width: 720,
        height: 480,
        margin: 32,
        font_size: 25.0,
        line_height: 1.2,
        ..Settings::default()
    };
    let open = |enabled, regular| {
        let mut data = Data::default();
        if enabled {
            data.choose(1, FB2_FOOTNOTES, Some(true)).unwrap();
        }
        if regular {
            data.choose(1, FB2_REGULAR_NOTE_SIZE, Some(true)).unwrap();
        }
        let mut book = Some(Book::open(&path).unwrap());
        Probe::layout_opened_book(
            &mut book,
            Engine::Parley,
            settings,
            0,
            None,
            &Default::default(),
            &data.snapshot(1).unwrap(),
            || crate::font_context(&[test_fixture("fonts/NotoSans-Regular.ttf")]),
        )
        .unwrap()
    };
    let mut plain = open(false, false);
    let mut on = open(true, false);
    let mut regular = open(true, true);
    let reference = plain.matches("Read the rain note").pop().unwrap();
    let page = plain.page_for(&reference).unwrap();
    assert_eq!(on.page_for(&reference), Some(page));
    assert_eq!(on.page_count(), plain.page_count());
    assert_eq!(on.page_anchor(page), plain.page_anchor(page));
    assert!(plain.foreign_band(page).is_none());
    let off = plain.capture(page).unwrap();
    let with_note = on.capture(page).unwrap();
    assert_eq!(regular.page_count(), plain.page_count());
    assert_eq!(regular.page_anchor(page), plain.page_anchor(page));
    let regular_note = regular.capture(page).unwrap();
    assert!(regular.foreign_band(page).is_some());
    assert_ne!(with_note, off);
    assert_ne!(regular_note, with_note);
    if let Some(directory) = std::env::var_os("READER_IN_PAGE_FOOTNOTE_CAPTURES") {
        let directory = std::path::PathBuf::from(directory);
        for (name, pixels) in [
            ("reader-fb3-cross-quiet-note-off.png", &off),
            ("reader-fb3-cross-quiet-note-on.png", &with_note),
            ("reader-fb3-cross-quiet-note-regular.png", &regular_note),
        ] {
            image::save_buffer(
                directory.join(name),
                pixels,
                settings.width,
                settings.height,
                image::ColorType::L8,
            )
            .unwrap();
        }
    }
    let band = on
        .foreign_band(page)
        .expect("quiet host should show a complete foreign note");
    assert_eq!(band.layout.note.chapter, "fb3-section-3.xhtml");
    let note_y = band.y_offset(page) as i32;
    let (_, href, rects) = band
        .layout
        .note_links()
        .into_iter()
        .find(|(label, _, _)| label.contains("Return to dispatch"))
        .expect("foreign backlink must be listed");
    assert!(href.ends_with("#dispatch"));
    let rect = rects[0];
    let x = (rect.x + rect.width / 2) as f32;
    let y = (rect.y + note_y + rect.height / 2) as f32;
    let target = on.link_at(page, x, y).unwrap();
    assert_eq!(target.path, "document.xhtml");
    assert_eq!(target.fragment.as_deref(), Some("dispatch"));
    let selected = on.word_at(page, x, y).unwrap();
    assert_eq!(selected.start.chapter, "fb3-section-3.xhtml");
    assert_eq!(selected.start.revision, band.layout.note.source.revision);
    assert_eq!(
        on.select_range(&selected.start, &selected.end),
        Some(selected.clone())
    );
    assert!(!on.selection_rects(page, &selected).is_empty());
    assert_eq!(
        on.text_position_at(page, x, y).unwrap().chapter,
        selected.start.chapter
    );
    assert!(
        on.links_on_page(page)
            .iter()
            .any(|(_, link)| link == &target)
    );
    assert_eq!(std::fs::read(path).unwrap(), before);
}
