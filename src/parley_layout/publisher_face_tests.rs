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

#[test]
fn idpf_embedded_faces_restore_original_reading_pixels_and_exclude_nonfonts() {
    use std::io::{Read, Write};
    const KEY: [u8; 20] = [
        222, 234, 161, 42, 141, 244, 104, 153, 6, 135, 177, 51, 57, 123, 171, 78, 195, 119, 87, 104,
    ];
    let fixture = crate::test_fixture("publisher-faces.epub");
    let fonts = [crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut original =
        Probe::open(&fixture, Engine::Parley, &fonts, Settings::default(), 0).unwrap();
    let pixels = original.capture(0).unwrap();
    let anchor = original.fragment("display").unwrap();
    let font = "EPUB/fonts/Silkscreen-Regular.ttf";
    let css = "EPUB/styles/reader.css";
    for variant in ["idpf", "unknown", "missing", "duplicate"] {
        let path =
            std::env::temp_dir().join(format!("reader-idpf-{}-{variant}.epub", std::process::id()));
        let out = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)
            .unwrap();
        let mut dst = zip::ZipWriter::new(out);
        let mut src = zip::ZipArchive::new(fs::File::open(&fixture).unwrap()).unwrap();
        for i in 0..src.len() {
            let mut member = src.by_index(i).unwrap();
            if member.name() == font || member.name().ends_with(".opf") {
                let name = member.name().to_owned();
                let mut data = Vec::new();
                member.read_to_end(&mut data).unwrap();
                if name == font {
                    for (i, byte) in data.iter_mut().take(1040).enumerate() {
                        *byte ^= KEY[i % KEY.len()];
                    }
                } else {
                    let xml = String::from_utf8(data).unwrap().replace(
                        "urn:primitiveos:reader:fixture:publisher-faces",
                        "urn:primitiveos:reader:\n  fixture:publisher-faces",
                    );
                    data = (if variant == "missing" {
                        xml.replace(
                            "unique-identifier=\"book-id\"",
                            "unique-identifier=\"absent\"",
                        )
                    } else {
                        xml
                    })
                    .into_bytes();
                }
                dst.start_file(name, zip::write::SimpleFileOptions::default())
                    .unwrap();
                dst.write_all(&data).unwrap();
            } else {
                dst.raw_copy_file(member).unwrap();
            }
        }
        let method = if variant == "unknown" {
            "urn:unknown-method"
        } else {
            "http://www.idpf.org/2008/embedding"
        };
        let entry = |name: &str| {
            format!(
                "<e:EncryptedData><e:EncryptionMethod Algorithm='{method}'/><e:CipherData><e:CipherReference URI='{name}'/></e:CipherData></e:EncryptedData>"
            )
        };
        let repeated = if variant == "duplicate" {
            entry(font)
        } else {
            String::new()
        };
        let map = format!(
            "<encryption xmlns='urn:oasis:names:tc:opendocument:xmlns:container' xmlns:e='http://www.w3.org/2001/04/xmlenc#'>{}{}{repeated}</encryption>",
            entry(font),
            entry(css)
        );
        dst.start_file(
            "META-INF/encryption.xml",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        dst.write_all(map.as_bytes()).unwrap();
        drop(dst.finish().unwrap());
        let mut book = Book::open(&path).unwrap();
        assert_eq!(
            book.resource(css).unwrap(),
            book.local_font(css, 16384).unwrap()
        );
        let raw = book.resource(font).unwrap();
        let for_layout = book.local_font(font, 2 * 1024 * 1024).unwrap();
        assert_eq!(raw != for_layout, variant == "idpf");
        assert_eq!(raw[1040..], for_layout[1040..]);
        let mut probe = Probe::open(&path, Engine::Parley, &fonts, Settings::default(), 0).unwrap();
        fs::remove_file(&path).unwrap();
        let restored = probe.fragment("display").unwrap();
        // Each modified OPF identifies its own edition. Its source chapter is identical.
        assert_eq!(
            restored,
            crate::Anchor {
                raw_caret: None,
                book: restored.book,
                ..anchor.clone()
            }
        );
        assert_eq!(probe.page_for(&restored), Some(0));
        assert_eq!(probe.capture(0).unwrap() == pixels, variant == "idpf");
    }
}

#[test]
fn original_idpf_fonts_match_independent_assets_and_painted_text() {
    let Some(path) = std::env::var_os("READER_TEST_IDPF_BOOK").map(PathBuf::from) else {
        return;
    };
    let info: BTreeMap<String, u64> = serde_json::from_slice(
        &fs::read(std::env::var_os("READER_TEST_IDPF_REFERENCE").unwrap()).unwrap(),
    )
    .unwrap();
    let mut book = Book::open(&path).unwrap();
    for (resource, &expected) in &info {
        let bytes = book.local_font(resource, 2 * 1024 * 1024).unwrap();
        assert_eq!(crate::revision_bytes(&bytes), expected);
        assert_ne!(bytes, book.resource(resource).unwrap());
    }
    let fonts = [crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut seen = std::collections::BTreeSet::new();
    let mut clusters = 0usize;
    let mut ink_pages = 0;
    for index in 0..book.chapters.len() {
        let mut probe =
            Probe::open(&path, Engine::Parley, &fonts, Settings::default(), index).unwrap();
        let Renderer::Parley(layout) = &probe.renderer else {
            continue;
        };
        let mut by_id = HashMap::new();
        for para in &layout.paragraphs {
            for run in para.layout.lines().flat_map(|l| l.runs()) {
                let fingerprint = *by_id
                    .entry(run.font().data.id())
                    .or_insert_with(|| crate::revision_bytes(run.font().data.data()));
                if info.values().any(|h| *h == fingerprint) {
                    seen.insert(fingerprint);
                    clusters += run.clusters().count();
                }
            }
        }
        let anchor = probe.page_anchor(0).unwrap();
        assert_eq!(probe.page_for(&anchor), Some(0));
        if probe.capture(0).unwrap().iter().any(|&p| p < 180) {
            ink_pages += 1;
        }
    }
    println!(
        "idpf_font_reference assets={} active={} clusters={} painted_chapters={}",
        info.len(),
        seen.len(),
        clusters,
        ink_pages
    );
    assert!(!seen.is_empty() && clusters > 0 && ink_pages > 0);
}

#[test]
fn adobe_and_idpf_font_obfuscation_can_coexist_without_touching_other_resources() {
    use std::io::{Read, Write};
    const ADOBE_KEY: [u8; 16] = [
        0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee,
        0xff,
    ];
    const IDPF_KEY: [u8; 20] = [
        222, 234, 161, 42, 141, 244, 104, 153, 6, 135, 177, 51, 57, 123, 171, 78, 195, 119, 87, 104,
    ];
    let fixture = crate::test_fixture("publisher-faces.epub");
    let fonts = [crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut baseline =
        Probe::open(&fixture, Engine::Parley, &fonts, Settings::default(), 0).unwrap();
    let pixels = baseline.capture(0).unwrap();
    let adobe_font = "EPUB/fonts/Silkscreen-Regular.ttf";
    let idpf_font = "EPUB/fonts/NotoSans-Regular.ttf";
    let css = "EPUB/styles/reader.css";
    let mut reference = Book::open(&fixture).unwrap();
    let expected_adobe = reference.resource(adobe_font).unwrap();
    let expected_idpf = reference.resource(idpf_font).unwrap();
    for variant in ["both", "missing-uuid", "duplicate-adobe", "unknown-adobe"] {
        let path = std::env::temp_dir().join(format!(
            "reader-adobe-{}-{variant}.epub",
            std::process::id()
        ));
        struct Cleanup(PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = fs::remove_file(&self.0);
            }
        }
        let _cleanup = Cleanup(path.clone());
        let out = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)
            .unwrap();
        let mut dst = zip::ZipWriter::new(out);
        let mut src = zip::ZipArchive::new(fs::File::open(&fixture).unwrap()).unwrap();
        for index in 0..src.len() {
            let mut member = src.by_index(index).unwrap();
            let name = member.name().to_owned();
            if name == adobe_font || name == idpf_font || name.ends_with(".opf") {
                let mut data = Vec::new();
                member.read_to_end(&mut data).unwrap();
                if name == adobe_font {
                    for (i, byte) in data.iter_mut().take(1024).enumerate() {
                        *byte ^= ADOBE_KEY[i % ADOBE_KEY.len()];
                    }
                } else if name == idpf_font {
                    for (i, byte) in data.iter_mut().take(1040).enumerate() {
                        *byte ^= IDPF_KEY[i % IDPF_KEY.len()];
                    }
                } else if variant != "missing-uuid" {
                    let xml = String::from_utf8(data).unwrap().replace("</metadata>", "<dc:identifier>urn:uuid:00112233-4455-6677-8899-aabbccddeeff</dc:identifier></metadata>");
                    data = xml.into_bytes();
                }
                dst.start_file(name, zip::write::SimpleFileOptions::default())
                    .unwrap();
                dst.write_all(&data).unwrap();
            } else {
                dst.raw_copy_file(member).unwrap();
            }
        }
        let adobe_method = if variant == "unknown-adobe" {
            "urn:unknown"
        } else {
            "http://ns.adobe.com/pdf/enc#RC"
        };
        let entry = |name: &str, method: &str| {
            format!(
                "<e:EncryptedData><e:EncryptionMethod Algorithm='{method}'/><e:CipherData><e:CipherReference URI='{name}'/></e:CipherData></e:EncryptedData>"
            )
        };
        let duplicate = if variant == "duplicate-adobe" {
            entry(adobe_font, adobe_method)
        } else {
            String::new()
        };
        let encryption = format!(
            "<encryption xmlns='urn:oasis:names:tc:opendocument:xmlns:container' xmlns:e='http://www.w3.org/2001/04/xmlenc#'>{}{}{}{duplicate}</encryption>",
            entry(adobe_font, adobe_method),
            entry(idpf_font, "http://www.idpf.org/2008/embedding"),
            entry(css, adobe_method)
        );
        dst.start_file(
            "META-INF/encryption.xml",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        dst.write_all(encryption.as_bytes()).unwrap();
        drop(dst.finish().unwrap());
        let mut book = Book::open(&path).unwrap();
        assert_eq!(
            book.local_font(idpf_font, 2 * 1024 * 1024).unwrap(),
            expected_idpf
        );
        let adobe = book.local_font(adobe_font, 2 * 1024 * 1024).unwrap();
        assert_eq!(adobe == expected_adobe, variant == "both", "{variant}");
        assert_eq!(
            book.resource(css).unwrap(),
            book.local_font(css, 16384).unwrap()
        );
        if variant == "both" {
            let mut probe =
                Probe::open(&path, Engine::Parley, &fonts, Settings::default(), 0).unwrap();
            assert_eq!(probe.capture(0).unwrap(), pixels);
        }
    }
}
