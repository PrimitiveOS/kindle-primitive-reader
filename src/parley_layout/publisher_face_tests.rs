use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::PathBuf;

use super::*;
use crate::{Engine, Probe, Renderer};

#[test]
fn publisher_faces_choose_declared_variants_and_share_assets_across_families() {
    let path = crate::test_fixture("publisher-faces.epub");
    let fonts = [crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let settings = Settings::default();
    let mut probe = Probe::open(&path, Engine::Parley, &fonts, settings, 0).unwrap();
    let noto = fs::read(&fonts[0]).unwrap();
    let silk = fs::read(crate::test_fixture("fonts/Silkscreen-Regular.ttf")).unwrap();
    let Renderer::Parley(chapter) = &probe.renderer else {
        unreachable!()
    };
    for (fragment, expected) in [
        ("regular", &noto),
        ("bold", &silk),
        ("italic", &silk),
        ("both", &noto),
        ("display", &silk),
    ] {
        let (key, _) = probe.source.fragments[fragment];
        let paragraph = chapter.paragraphs.iter().find(|p| p.key == key).unwrap();
        let mut runs = paragraph
            .layout
            .lines()
            .flat_map(|line| line.runs())
            .peekable();
        assert!(runs.peek().is_some());
        assert!(
            runs.all(|run| run.font().data.data() == expected),
            "{fragment}"
        );
    }
    let anchor = probe.fragment("italic").unwrap();
    let page = probe.page_for(&anchor).unwrap();
    let with_publisher = probe.capture(page).unwrap();
    let mut plain = Probe::open(
        &path,
        Engine::Parley,
        &fonts,
        Settings {
            publisher_styles: false,
            ..settings
        },
        0,
    )
    .unwrap();
    assert_eq!(plain.fragment("italic").unwrap(), anchor);
    assert_ne!(
        with_publisher,
        plain.capture(plain.page_for(&anchor).unwrap()).unwrap()
    );
    let mut resumed = Probe::open(&path, Engine::Parley, &fonts, settings, 0).unwrap();
    assert_eq!(
        with_publisher,
        resumed.capture(resumed.page_for(&anchor).unwrap()).unwrap()
    );
}

/// Optional local reference. Each file path is the actual resource assigned to
/// a CSS style by an independent inspection of this publication's stylesheet.
/// Resources stay in the archive and are compared to Parley's painted runs in memory.
#[test]
fn publisher_faces_compare_original_reading_runs_to_publication_resources() {
    let Some(path) = std::env::var_os("READER_TEST_FACES_BOOK").map(PathBuf::from) else {
        return;
    };
    #[derive(serde::Deserialize)]
    struct Reference {
        chapter: usize,
        paths: [String; 4],
        minimum_variants: usize,
    }
    let reference: Reference = serde_json::from_slice(
        &fs::read(std::env::var_os("READER_TEST_FACES_REFERENCE").unwrap()).unwrap(),
    )
    .unwrap();
    let mut book = Book::open(&path).unwrap();
    let expected = reference
        .paths
        .map(|path| book.resource_limited(&path, 2 * 1024 * 1024).unwrap());
    let fonts = [crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let probe = Probe::open(
        &path,
        Engine::Parley,
        &fonts,
        Settings::default(),
        reference.chapter,
    )
    .unwrap();
    let Renderer::Parley(chapter) = &probe.renderer else {
        unreachable!()
    };
    let mut context = crate::font_context(&fonts).unwrap();
    let href = &probe.chapters[reference.chapter].href;
    let styles = crate::book_style::BookStyle::load(&mut book, href, &probe.source, &mut context)
        .compute(&probe.source.elements);
    let mut source_ids = HashMap::new();
    let mut correct = [0usize; 4];
    let mut mismatches = 0;
    for para in &chapter.paragraphs {
        let block = probe
            .source
            .blocks
            .iter()
            .find(|block| block.key == para.key)
            .unwrap();
        for line in para.layout.lines() {
            for run in line.runs() {
                let actual = *source_ids.entry(run.font().data.id()).or_insert_with(|| {
                    expected
                        .iter()
                        .position(|data| run.font().data.data() == data)
                });
                for cluster in run.clusters() {
                    let range = cluster.text_range();
                    if !block.text[range.clone()]
                        .chars()
                        .any(|c| c.is_ascii_alphabetic())
                    {
                        continue;
                    }
                    let element = block
                        .styled_text
                        .iter()
                        .find(|span| span.bytes.contains(&range.start))
                        .map_or(block.element, |span| span.element);
                    let style = &styles[element];
                    if !matches!(&style.font_family, crate::book_style::Family::Named(name) if name.starts_with("PrimitiveOS EPUB Face "))
                    {
                        continue;
                    }
                    let variant = usize::from(style.font_weight >= FontWeight::SEMI_BOLD)
                        + 2 * usize::from(style.font_style != parley::FontStyle::Normal);
                    if actual == Some(variant) {
                        correct[variant] += 1
                    } else {
                        mismatches += 1
                    }
                }
            }
        }
    }
    let variants = correct.iter().filter(|&&n| n > 0).count();
    println!(
        "publisher_face_reference matched={} mismatched={mismatches} variants={variants}",
        correct.iter().sum::<usize>()
    );
    assert_eq!(mismatches, 0);
    assert!(variants >= reference.minimum_variants);
}

/// Prints only an inventory of Unicode scalars in clusters with missing glyphs.
/// No words, titles, resources, output frames or persistent reading data are emitted.
#[test]
fn publisher_faces_inventory_original_missing_glyph_clusters() {
    let Some(path) = std::env::var_os("READER_TEST_GLYPH_BOOK").map(PathBuf::from) else {
        return;
    };
    let chapters = std::env::var("READER_TEST_GLYPH_CHAPTERS").unwrap();
    let fonts = std::env::split_paths(&std::env::var_os("READER_TEST_GLYPH_FONTS").unwrap())
        .collect::<Vec<_>>();
    let mut counts = BTreeMap::<u32, usize>::new();
    for index in chapters.split(',').map(|s| s.parse::<usize>().unwrap()) {
        let probe = Probe::open(&path, Engine::Parley, &fonts, Settings::default(), index).unwrap();
        let Renderer::Parley(chapter) = &probe.renderer else {
            continue;
        };
        for para in &chapter.paragraphs {
            let block = probe
                .source
                .blocks
                .iter()
                .find(|b| b.key == para.key)
                .unwrap();
            for run in para.layout.lines().flat_map(|line| line.runs()) {
                for cluster in run.clusters().filter(|c| c.glyphs().any(|g| g.id == 0)) {
                    for ch in block.text[cluster.text_range()].chars() {
                        *counts.entry(ch as u32).or_default() += 1;
                    }
                }
            }
        }
    }
    println!(
        "publisher_glyph_inventory {}",
        serde_json::to_string(&counts).unwrap()
    );
}

/// The independent reference identifies checksums of original SFNT data read from
/// Palm FONT records. No original typeface, book text, or page is written to disk.
#[test]
fn original_kf8_fonts_match_record_bytes_and_reading_runs() {
    let Some(path) = std::env::var_os("READER_TEST_KF8_FONT_BOOK").map(PathBuf::from) else {
        return;
    };
    #[derive(serde::Deserialize)]
    struct Reference {
        records: BTreeMap<usize, u64>,
        chapters: Vec<usize>,
        #[serde(default)]
        unreadable: Vec<usize>,
        #[serde(default = "one_face")]
        minimum_active: usize,
    }
    fn one_face() -> usize {
        1
    }
    let rf: Reference = serde_json::from_slice(
        &fs::read(std::env::var_os("READER_TEST_KF8_FONT_REFERENCE").unwrap()).unwrap(),
    )
    .unwrap();
    use skrifa::{FontRef, MetadataProvider as _};
    let mut controls = Vec::new();
    let mut book = Book::open(&path).unwrap();
    for (&number, &hash) in &rf.records {
        let found = book.kindle_font(number, 2 * 1024 * 1024);
        if rf.unreadable.contains(&number) {
            assert!(found.is_err());
            continue;
        }
        let data = found.unwrap();
        assert_eq!(crate::revision_bytes(&data), hash, "resource {number}");
        let face = FontRef::from_index(&data, 0).unwrap();
        let map = face.charmap();
        let ch = ('A'..='Z')
            .chain('a'..='z')
            .find(|&c| map.map(c).is_some())
            .or_else(|| {
                map.mappings()
                    .filter_map(|(cp, _)| char::from_u32(cp))
                    .find(|c| c.is_alphanumeric())
            })
            .unwrap();
        controls.push((number, hash, ch));
    }
    let fonts = [crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    // Separate controls request each font even when a publisher never applies
    // the declared family to any original text. Check faces separately from reading usage.
    for group in controls.chunks(4) {
        let mut css = String::new();
        let mut body = String::new();
        const DIGITS: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUV";
        for &(number, _, ch) in group {
            let code = (0..4)
                .rev()
                .map(|shift| DIGITS[(number >> (5 * shift)) & 31] as char)
                .collect::<String>();
            css.push_str(&format!(
                "@font-face{{font-family:Face{number};src:url('kindle:embed:{code}')}}"
            ));
            body.push_str(&format!(
                "<p id='f{number}' style='font-family:Face{number}'>&#{};</p>",
                ch as u32
            ));
        }
        let markup = format!("<html><head><style>{css}</style></head><body>{body}</body></html>");
        let mut control = Probe::from_book_content(
            Book::open(&path).unwrap(),
            Engine::Parley,
            &fonts,
            Settings::default(),
            0,
            Some(markup),
        )
        .unwrap();
        let Renderer::Parley(layout) = &control.renderer else {
            unreachable!()
        };
        for &(number, hash, _) in group {
            let key = control.source.fragments[&format!("f{number}")].0;
            let paragraph = layout.paragraphs.iter().find(|p| p.key == key).unwrap();
            let run = paragraph
                .layout
                .lines()
                .flat_map(|l| l.runs())
                .next()
                .unwrap();
            assert_eq!(crate::revision_bytes(run.font().data.data()), hash);
        }
        assert!(control.capture(0).unwrap().iter().any(|&p| p < 180));
    }
    let mut clusters = 0usize;
    let mut used = std::collections::BTreeSet::new();
    let mut pages = 0;
    for chapter in rf.chapters {
        let mut probe =
            Probe::open(&path, Engine::Parley, &fonts, Settings::default(), chapter).unwrap();
        let Renderer::Parley(layout) = &probe.renderer else {
            continue;
        };
        let mut observed = HashMap::new();
        for para in &layout.paragraphs {
            for run in para.layout.lines().flat_map(|line| line.runs()) {
                let hit = *observed.entry(run.font().data.id()).or_insert_with(|| {
                    let h = crate::revision_bytes(run.font().data.data());
                    rf.records
                        .iter()
                        .find_map(|(&n, &hash)| (hash == h).then_some(n))
                });
                if let Some(n) = hit {
                    used.insert(n);
                    clusters += run.clusters().count();
                }
            }
        }
        let anchor = probe.page_anchor(0).unwrap();
        assert_eq!(probe.page_for(&anchor), Some(0));
        assert!(probe.capture(0).unwrap().iter().any(|&v| v < 180));
        pages += 1;
    }
    println!(
        "kf8_font_reference assets={} used={} clusters={} chapters={}",
        rf.records.len(),
        used.len(),
        clusters,
        pages
    );
    assert!(used.len() >= rf.minimum_active);
    if rf.minimum_active == 0 {
        assert_eq!(clusters, 0);
    }
}

#[test]
fn kf8_font_from_css_changes_real_page_and_invalid_face_falls_back() {
    use std::io::Write;
    let fallback = crate::test_fixture("fonts/NotoSans-Regular.ttf");
    let embedded = fs::read(crate::test_fixture("fonts/Silkscreen-Regular.ttf")).unwrap();
    let settings = Settings {
        width: 640,
        height: 820,
        margin: 35,
        font_size: 32.,
        ..Settings::default()
    };
    let record = crate::kf8::fonts::tests::record(&embedded, true, false);
    let mut no_signature = embedded.clone();
    no_signature[..4].copy_from_slice(b"JUNK");
    let mut authored_frame = None;
    for (index, payload) in [
        record,
        crate::kf8::fonts::tests::record(&no_signature, true, false),
    ]
    .iter()
    .enumerate()
    {
        let path = std::env::temp_dir().join(format!(
            "reader-kf8-face-{}-{index}.azw3",
            std::process::id()
        ));
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        file.write_all(&crate::kf8::tests::font_fixture(payload))
            .unwrap();
        drop(file);
        let mut probe = Probe::open(
            &path,
            Engine::Parley,
            std::slice::from_ref(&fallback),
            settings,
            0,
        )
        .unwrap();
        fs::remove_file(&path).unwrap();
        let (key, _) = probe.source.fragments["font-sample"];
        let Renderer::Parley(layout) = &probe.renderer else {
            unreachable!()
        };
        let paragraph = layout.paragraphs.iter().find(|p| p.key == key).unwrap();
        let chosen = paragraph
            .layout
            .lines()
            .flat_map(|line| line.runs())
            .next()
            .unwrap();
        assert_eq!(chosen.font().data.data() == embedded, index == 0);
        let anchor = probe.fragment("font-sample").unwrap();
        let page = probe.page_for(&anchor).unwrap();
        let pixels = probe.capture(page).unwrap();
        assert!(pixels.iter().any(|&p| p < 128));
        if let Some(other) = &authored_frame {
            assert_ne!(&pixels, other);
        } else {
            if let Some(output) = std::env::var_os("READER_TEST_KF8_FONT_SCREEN") {
                image::save_buffer(
                    output,
                    &pixels,
                    settings.width,
                    settings.height,
                    image::ColorType::L8,
                )
                .unwrap();
            }
            authored_frame = Some(pixels);
        }
    }
}
