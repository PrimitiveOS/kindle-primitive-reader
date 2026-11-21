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
