use super::*;

fn historical(xml: &str, ruby: bool) -> Source {
    Source::parse_document(
        xml,
        crate::revision(xml),
        None,
        None,
        &BTreeSet::new(),
        Extraction {
            ruby,
            origins: true,
            ignore_embedded_language: false,
        },
    )
    .unwrap()
}

#[test]
fn all_defined_c1_references_match_crengine_and_unassigned_remain_controls() {
    let expected = [
        (128, '€'),
        (130, '‚'),
        (131, 'ƒ'),
        (132, '„'),
        (133, '…'),
        (134, '†'),
        (135, '‡'),
        (136, 'ˆ'),
        (137, '‰'),
        (138, 'Š'),
        (139, '‹'),
        (140, 'Œ'),
        (142, 'Ž'),
        (145, '‘'),
        (146, '’'),
        (147, '“'),
        (148, '”'),
        (149, '•'),
        (150, '–'),
        (151, '—'),
        (152, '˜'),
        (153, '™'),
        (154, 'š'),
        (155, '›'),
        (156, 'œ'),
        (158, 'ž'),
        (159, 'Ÿ'),
    ];
    for (code, replacement) in expected {
        assert_eq!(converted(code), Some(replacement));
        for reference in [
            format!("&#{code};"),
            format!("&#x{code:x};"),
            format!("&#x{code:X};"),
            format!("&#000{code};"),
        ] {
            let xml = format!("<html><body><p>A{reference}B</p></body></html>");
            let source = Source::parse(&xml).unwrap();
            assert_eq!(
                source.blocks[0].text,
                format!("A{replacement}B"),
                "{reference}"
            );
            let old = historical(&xml, true);
            for offset in [0, 1, 3, 4] {
                let expected = match offset {
                    0 => 0,
                    1 => 1,
                    3 => 1 + replacement.len_utf8(),
                    _ => 2 + replacement.len_utf8(),
                };
                assert_eq!(
                    source.project_offset(&source.blocks[0], old.revision, offset),
                    Some(expected)
                );
            }
            assert_eq!(
                source.project_offset(&source.blocks[0], old.revision, 2),
                None
            );
            assert_eq!(source.blocks[0].key, old.blocks[0].key);
        }
    }
    for code in [129, 141, 143, 144, 157] {
        let xml = format!("<html><body><p>A&#{code};B</p></body></html>");
        assert_eq!(normalize(&xml), None);
        assert_eq!(
            Source::parse(&xml).unwrap().blocks[0].text,
            format!("A{}B", char::from_u32(code).unwrap())
        );
    }
    assert_eq!(normalize("&#127;&#160;&#8217;&#x1f642;"), None);
}

#[test]
fn literal_controls_cdata_comments_processing_instructions_and_escaped_amp_are_untouched() {
    let xml = "<?test &#146;?><html><!-- &#146; --><body><p>\u{92} <![CDATA[&#146; \u{92}]]> &amp;#146; &#146;</p></body></html>";
    let normalized = normalize(xml).unwrap();
    assert_eq!(normalized, xml.replacen(" &#146;</p>", " &#8217;</p>", 1));
    let source = Source::parse(xml).unwrap();
    assert_eq!(source.blocks[0].text, "\u{92} &#146; \u{92} &#146; ’");
    assert!(normalize("&#146 &#x; &#X92; &#999999999999999999999999999;").is_none());
}

#[test]
fn attributes_links_styles_and_historical_fragment_names_use_correct_model() {
    let xml = "<html><head><style>.a { color: black; }</style></head><body><p id='quote&#146;' class='a&#146;' style='font-family: &#147;serif&#148;'>A&#146;<a href='#quote&#146;'>B&#150;C</a><img src='a&#146;.png' alt='D&#146;E'/></p><p id='after'>Tail</p></body></html>";
    let old = historical(xml, true);
    let source = Source::parse(xml).unwrap();
    let block = &source.blocks[0];
    assert_eq!(block.text, "A’B–C");
    assert_eq!(block.links[0].href, "#quote’");
    assert_eq!(&block.text[block.links[0].bytes.clone()], "B–C");
    assert_eq!(block.images[0].href, "a’.png");
    assert_eq!(block.images[0].label, "D’E");
    assert_eq!(source.fragments["quote’"], source.fragments["quote\u{92}"]);
    assert_eq!(source.fragments["after"].0, old.fragments["after"].0);
    let element = &source.elements[block.element];
    assert_eq!(element.classes, "a’");
    assert!(element.inline_style.as_ref().unwrap().contains("“serif”"));
    let rendered = source.with_styles(xml, "p { margin: 0; }");
    let dom = Document::parse(&rendered).unwrap();
    let p = dom
        .descendants()
        .find(|node| node.has_tag_name("p"))
        .unwrap();
    assert_eq!(
        p.attribute(SOURCE_ATTRIBUTE),
        Some(old.blocks[0].key.to_string().as_str())
    );
    assert_eq!(p.attribute("id"), Some("quote’"));
}

#[test]
fn pre_ruby_and_post_ruby_saved_offsets_survive_variable_width_numeric_references() {
    let xml = "<html><body><p id='start'>A&#146; <ruby>日<rp>(</rp><rt>に&#146;ち</rt><rp>)</rp>本<rt>ほん</rt></ruby> <a href='#tail'>Z&#150;E</a></p><p id='tail'>After&#128; end</p></body></html>";
    let old = historical(xml, true);
    let old_flat = historical(xml, false);
    let source = Source::parse(xml).unwrap();
    assert_eq!(source.blocks[0].text, "A’ 日本 Z–E");
    assert_eq!(source.blocks[0].ruby[0].annotation, "に’ち");
    for (before, revision) in [(&old, old.revision), (&old_flat, crate::revision(xml))] {
        assert_ne!(source.revision, revision);
        for (i, previous) in before.blocks.iter().enumerate() {
            let now = &source.blocks[i];
            assert_eq!(now.key, previous.key);
            for word in ["A", "日", "本", "Z", "E", "After", "end"] {
                if let Some(offset) = previous.text.find(word) {
                    assert_eq!(
                        source.project_offset(now, revision, offset),
                        now.text.find(word),
                        "{word} revision {revision}"
                    );
                }
            }
            assert_eq!(
                source.project_offset(now, revision, previous.text.len()),
                Some(now.text.len())
            );
            assert_eq!(
                source.project_offset(now, revision, previous.text.len() + 1),
                None
            );
        }
    }
    assert_eq!(
        source.blocks[0].text[source.blocks[0].links[0].bytes.clone()].trim_start(),
        "Z–E"
    );
    assert!(source.with_styles(xml, "").contains("<ruby>日本</ruby>"));
}

#[test]
fn changed_ruby_annotation_limit_does_not_corrupt_prior_reading_positions() {
    // The old 190-byte pronunciation is supported, the converted 193-byte one is not.
    let annotation = format!("{}&#146;&#146;&#146;", "a".repeat(184));
    let xml =
        format!("<html><body><p>A <ruby>日<rt>{annotation}</rt></ruby> Z&#146;</p></body></html>");
    let old = historical(&xml, true);
    assert_eq!(old.blocks[0].ruby.len(), 1);
    let source = Source::parse(&xml).unwrap();
    assert!(source.blocks[0].ruby.is_empty());
    let block = &source.blocks[0];
    assert_eq!(
        source.project_offset(block, old.revision, old.blocks[0].text.find('Z').unwrap()),
        block.text.find('Z')
    );
    let flat = historical(&xml, false);
    assert_eq!(
        source.project_offset(
            block,
            crate::revision(&xml),
            flat.blocks[0].text.find('Z').unwrap()
        ),
        block.text.find('Z')
    );
}

#[test]
fn newly_visible_inline_run_keeps_other_keys_and_duplicate_paragraph_identity() {
    let xml = "<html><body>&#133;<p id='one'>Same&#146; text</p> &#133; <p id='two'>Same&#146; text</p><p>Last</p></body></html>";
    let old = historical(xml, true);
    let source = Source::parse(xml).unwrap();
    assert_eq!(old.blocks.len(), 3);
    assert_eq!(source.blocks.len(), 5);
    assert!(
        source
            .blocks
            .windows(2)
            .all(|pair| pair[0].key < pair[1].key)
    );
    for (index, id) in ["one", "two"].into_iter().enumerate() {
        assert_eq!(source.fragments[id].0, old.fragments[id].0);
        let block = source
            .blocks
            .iter()
            .find(|b| b.key == old.blocks[index].key)
            .unwrap();
        let offset = old.blocks[index].text.find("text").unwrap();
        assert_eq!(
            source.project_offset(block, old.revision, offset),
            block.text.find("text")
        );
    }
    let rendered = source.with_styles(xml, "");
    Document::parse(&rendered).unwrap();
    for block in &source.blocks {
        assert!(rendered.contains(&format!("{SOURCE_ATTRIBUTE}=\"{}\"", block.key)));
    }
}

#[test]
fn previous_mobi_numeric_history_projects_keys_and_offsets_together() {
    let old_xml =
        "<html><body><p id='old'>A&#146; B <ruby>日<rt>にち</rt></ruby> Z</p></body></html>";
    let old = historical(old_xml, true);
    let mut current = Source::parse("<html><head><title>Restored images</title></head><body><p>A’ B <ruby>日<rt>にち</rt></ruby> Z</p></body></html>").unwrap();
    current.accept_previous_mobi(old_xml).unwrap();
    let key = current
        .current_key(old.blocks[0].key, old.revision)
        .unwrap();
    assert_eq!(key, current.blocks[0].key);
    assert_eq!(
        current.project_offset(
            &current.blocks[0],
            old.revision,
            old.blocks[0].text.find('Z').unwrap()
        ),
        current.blocks[0].text.find('Z')
    );
    let flat = historical(old_xml, false);
    assert_eq!(
        current.current_key(flat.blocks[0].key, flat.revision),
        Some(key)
    );
    assert_eq!(
        current.project_offset(
            &current.blocks[0],
            flat.revision,
            flat.blocks[0].text.find('Z').unwrap()
        ),
        current.blocks[0].text.find('Z')
    );
}

#[test]
fn search_display_and_saved_selection_use_normalized_quotes() {
    use crate::{Anchor, Engine, Probe, Settings};
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "reader-numeric-{}-{unique}.xhtml",
        std::process::id()
    ));
    let xml = "<html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>Numeric references</title></head><body><h1>Publisher punctuation</h1><p id=\"sample\">A&#146; <ruby>day<rt>dei</rt></ruby> B&#150;E</p><p>Normal quotation marks and the same saved selection after migration.</p></body></html>";
    std::fs::write(&path, xml).unwrap();
    let mut book = crate::epub::Book::open(&path).unwrap();
    let old_xml = book.chapter(0).unwrap();
    assert!(old_xml.contains("&#146;"));
    let old = historical(&old_xml, true);
    let old_flat = historical(&old_xml, false);
    let mut probe = Probe::open(
        &path,
        Engine::Parley,
        &[crate::test_fixture("fonts/NotoSans-Regular.ttf")],
        Settings::default(),
        0,
    )
    .unwrap();
    assert_eq!(probe.missing_glyphs(), 0);
    let sample = probe.fragment("sample").unwrap();
    for prior in [&old, &old_flat] {
        let before = prior.blocks.iter().find(|b| b.key == sample.block).unwrap();
        let start = Anchor {
            revision: prior.revision,
            offset: before.text.find('A').unwrap(),
            ..sample.clone()
        };
        let end = Anchor {
            offset: before.text.len(),
            ..start.clone()
        };
        let saved = serde_json::to_vec(&(start, end)).unwrap();
        let (start, end): (Anchor, Anchor) = serde_json::from_slice(&saved).unwrap();
        let selection = probe.select_range(&start, &end).unwrap();
        assert_eq!(selection.text, "A’ day B–E");
        assert!(!probe.selection_rects(0, &selection).is_empty());
    }
    let mut search = crate::search::Search::new("A’").unwrap();
    let hits = search.step(&mut book).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(
        probe
            .select_range(&hits[0].anchor, &hits[0].end)
            .unwrap()
            .text,
        "A’"
    );
    let pixels = probe.capture(0).unwrap();
    assert!(pixels.iter().any(|&p| p < 100));
    if let Some(output) = std::env::var_os("READER_NUMERIC_CAPTURE") {
        #[cfg(feature = "kindle")]
        primitive_app::write_png(
            std::path::Path::new(&output),
            Settings::default().width,
            Settings::default().height,
            &pixels,
        )
        .unwrap();
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn unsupported_nested_ruby_retains_its_text_and_historical_boundaries() {
    let xml = "<html><body><p>A&#146; <ruby>Outer<ruby>Inner<rt>in&#146;</rt></ruby><rt>out&#150;</rt></ruby> Z</p></body></html>";
    let old = historical(xml, true);
    let source = Source::parse(xml).unwrap();
    assert!(old.blocks[0].ruby.is_empty());
    assert!(source.blocks[0].ruby.is_empty());
    let before = &old.blocks[0];
    let current = &source.blocks[0];
    assert_eq!(current.text, "A’ OuterInnerin’out– Z");
    for word in ["Outer", "Inner", "in", "out", "Z"] {
        assert_eq!(
            source.project_offset(current, old.revision, before.text.find(word).unwrap()),
            current.text.find(word)
        );
    }
    assert_eq!(
        source.project_offset(current, old.revision, before.text.len()),
        Some(current.text.len())
    );
}
