use super::*;
use crate::source::Source;

fn parse_source(body: &str) -> Source {
    Source::parse(&format!("<html><body>{body}</body></html>")).unwrap()
}

fn masks(
    source: &Source,
    suppressed: &[&str],
    hidden: &[&str],
    visible: &[&str],
) -> Vec<ElementFlow> {
    let mut flows: Vec<ElementFlow> = Vec::new();
    for element in &source.elements {
        let mut flow = element
            .parent
            .map_or_else(ElementFlow::default, |i| flows[i]);
        if element
            .id
            .as_deref()
            .is_some_and(|id| suppressed.contains(&id))
        {
            flow.suppressed = true;
        }
        if element.id.as_deref().is_some_and(|id| hidden.contains(&id)) {
            flow.painted = false;
        }
        if element
            .id
            .as_deref()
            .is_some_and(|id| visible.contains(&id))
        {
            flow.painted = true;
        }
        flows.push(flow);
    }
    flows
}

#[test]
fn identity_borrows_canonical_text_and_requires_no_projection_allocations() {
    let source =
        parse_source("<p id='p'>  A <em>cafe\u{301}</em><br/>後 <img src='x.png'/> end. </p>");
    let block = &source.blocks[0];
    let projection =
        Projection::new(block, &vec![ElementFlow::default(); source.elements.len()]).unwrap();
    assert!(projection.is_identity());
    assert_eq!(projection.text().as_ptr(), block.text.as_ptr());
    assert!(projection.runs().is_none());
    assert!(projection.images().is_none());
    for offset in 0..=block.text.len() {
        for affinity in [Affinity::Before, Affinity::After] {
            let expected = block.text.is_char_boundary(offset).then_some(offset);
            assert_eq!(projection.project_offset(offset, affinity), expected);
            assert_eq!(projection.source_offset(offset, affinity), expected);
        }
    }
    assert!(projection.into_owned().is_none());
}

#[test]
fn owned_payload_rebinds_without_source_ownership_and_rejects_wrong_or_stale_blocks() {
    let body = "<p>A <span id='cut'>X</span>B</p><p>A XB</p>";
    let mut source = parse_source(body);
    let flows = masks(&source, &["cut"], &[], &[]);
    let owned = Projection::new(&source.blocks[0], &flows)
        .unwrap()
        .into_owned()
        .unwrap();
    let view = owned.bind(&source.blocks[0]).unwrap();
    assert_eq!(view.text(), "A B");
    assert_eq!(view.text().as_ptr(), owned.text().as_ptr());
    assert_eq!(view.source_spans(0..3), Some(vec![0..2, 3..4]));
    assert!(matches!(
        owned.bind(&source.blocks[1]),
        Err(ProjectionError::WrongBlock)
    ));
    drop(view);
    let same_revision_new_parse = parse_source(body);
    assert_eq!(same_revision_new_parse.revision, source.revision);
    assert_eq!(same_revision_new_parse.blocks[0].key, source.blocks[0].key);
    assert_eq!(
        same_revision_new_parse.blocks[0].text,
        source.blocks[0].text
    );
    assert!(matches!(
        owned.bind(&same_revision_new_parse.blocks[0]),
        Err(ProjectionError::WrongBlock)
    ));
    source.blocks[0].text.replace_range(0..1, "Z");
    assert!(matches!(
        owned.bind(&source.blocks[0]),
        Err(ProjectionError::WrongBlock)
    ));
    drop(source);
    assert!(
        owned.chapter.upgrade().is_none(),
        "Owned layout must not retain canonical source lifetime"
    );
    assert_eq!(owned.text(), "A B");
    assert!(!owned.runs().is_empty());
    assert!(owned.images().is_empty());
}

#[test]
fn whitespace_follows_its_actual_origin_across_suppressed_inline_content() {
    for (body, canonical, projected) in [
        ("A <span id='cut'>X </span>B", "A X B", "A B"),
        ("A<span id='cut'>X </span>B", "AX B", "AB"),
        ("A<span id='cut'>X</span> B", "AX B", "A B"),
        ("A <span id='cut'> X </span> B", "A X B", "A B"),
        ("<span id='cut'>X </span>B", "X B", "B"),
        ("A<span id='cut'> X</span>", "A X", "A"),
        ("A<span id='cut'> </span>B", "A B", "AB"),
        ("A<span id='cut'>X</span>B", "AXB", "AB"),
        ("A\t<span id='cut'>X</span>\nB", "A X B", "A B"),
        ("A\u{a0}<span id='cut'>X</span>B", "A\u{a0}XB", "A\u{a0}B"),
    ] {
        let source = parse_source(&format!("<p>{body}</p>"));
        let block = &source.blocks[0];
        assert_eq!(block.text, canonical, "{body}");
        let projection = Projection::new(block, &masks(&source, &["cut"], &[], &[])).unwrap();
        assert_eq!(projection.text(), projected, "{body}");
        assert_eq!(block.text, canonical);
    }
}

#[test]
fn hidden_breaks_preserve_outside_whitespace_without_inventing_source_bytes() {
    let source = parse_source("<p>A <br id='cut'/>B</p>");
    let block = &source.blocks[0];
    assert_eq!(block.text, "A\nB");
    let projection = Projection::new(block, &masks(&source, &["cut"], &[], &[])).unwrap();
    assert_eq!(projection.text(), "A B");
    let space = &projection.runs().unwrap()[1];
    assert_eq!(space.source, 1..1);
    assert_eq!(space.layout, 1..2);
    assert_eq!(projection.source_spans(0..3), Some(vec![0..1, 2..3]));
    assert_eq!(projection.layout_spans(0..3), Some(vec![0..1, 1..2, 2..3]));
    assert_eq!(projection.layout_spans(1..3), Some(vec![2..3]));
    assert_eq!(projection.source_offset(2, Affinity::Before), Some(1));
    assert_eq!(projection.source_offset(2, Affinity::After), Some(2));
    assert_eq!(projection.project_offset(1, Affinity::Before), Some(1));
    assert_eq!(projection.project_offset(1, Affinity::After), Some(2));

    let source = parse_source("<p>A<span id='cut'> </span><br id='br'/>B</p>");
    assert_eq!(
        Projection::new(&source.blocks[0], &masks(&source, &["cut", "br"], &[], &[]))
            .unwrap()
            .text(),
        "AB"
    );
    let source = parse_source("<p>A <br/> B<span id='cut'>X</span><br/>C</p>");
    assert_eq!(
        Projection::new(&source.blocks[0], &masks(&source, &["cut"], &[], &[]))
            .unwrap()
            .text(),
        "A\nB\nC"
    );
}

#[test]
fn join_affinities_and_selection_spans_do_not_enclose_removed_source_text() {
    let source = parse_source("<p>ab<span id='cut'>GONE</span>cd</p>");
    let block = &source.blocks[0];
    let projection = Projection::new(block, &masks(&source, &["cut"], &[], &[])).unwrap();
    assert_eq!(projection.text(), "abcd");
    assert_eq!(projection.source_offset(2, Affinity::Before), Some(2));
    assert_eq!(projection.source_offset(2, Affinity::After), Some(6));
    assert_eq!(projection.project_offset(4, Affinity::Before), None);
    assert_eq!(projection.project_nearest(4, Affinity::Before), Some(2));
    assert_eq!(projection.project_nearest(4, Affinity::After), Some(2));
    assert!(!projection.source_is_present(2));
    assert!(projection.source_is_present(6));
    // A word/search match crossing the new join gets two canonical spans.
    assert_eq!(projection.source_spans(1..3), Some(vec![1..2, 6..7]));
    assert_eq!(projection.layout_spans(1..7), Some(vec![1..2, 2..3]));
    assert!(projection.layout_spans(2..6).unwrap().is_empty());
    assert!(projection.source_spans(3..2).is_none());
}

#[test]
fn visibility_changes_paint_only_and_visible_descendants_restore_their_own_runs() {
    let source = parse_source(
        "<p id='hide'>A <span id='show'>B</span> C<img id='image' src='x.png'/><span id='cut'>D</span></p>",
    );
    let block = &source.blocks[0];
    let flows = masks(&source, &["cut"], &["hide"], &["show"]);
    let projection = Projection::new(block, &flows).unwrap();
    assert_eq!(projection.text(), "A B C");
    let runs = projection.runs().unwrap();
    assert!(
        runs.iter()
            .any(|run| run.painted && &projection.text()[run.layout.clone()] == "B")
    );
    assert!(
        runs.iter()
            .any(|run| !run.painted && projection.text()[run.layout.clone()].contains('A'))
    );
    assert!(projection.source_is_present(block.text.find('A').unwrap()));
    assert!(!projection.images().unwrap()[0].painted);
    // The default path still exposes the complete canonical D when no CSS hides it.
    assert_eq!(Projection::identity(block).text(), "A B CD");
}

#[test]
fn paint_only_view_preserves_the_complete_canonical_layout_text() {
    let source = parse_source(
        "<p id='hide'> A <em>B </em><img src='one.png'/> C<br/> D<ruby>日<rt>にち</rt></ruby><img src='two.png'/> E </p>",
    );
    let block = &source.blocks[0];
    let projection = Projection::new(block, &masks(&source, &[], &["hide"], &[])).unwrap();
    assert_eq!(projection.text(), block.text);
    assert_eq!(
        projection
            .source_spans(0..block.text.len())
            .unwrap()
            .iter()
            .map(|range| &block.text[range.clone()])
            .collect::<String>(),
        block.text
    );
    assert!(projection.runs().unwrap().iter().all(|run| !run.painted));
    assert!(
        projection
            .images()
            .unwrap()
            .iter()
            .all(|image| !image.painted)
    );
    assert_eq!(projection.images().unwrap().len(), block.images.len());
    for (image, original) in projection.images().unwrap().iter().zip(&block.images) {
        assert_eq!(image.offset, original.offset);
        assert_eq!(image.space_after, original.space_after);
    }
    let hidden = Projection::new(block, &masks(&source, &["hide"], &[], &[])).unwrap();
    assert!(hidden.text().is_empty());
    assert!(hidden.images().unwrap().is_empty());
}

#[test]
fn paint_only_images_preserve_text_node_boundaries_and_unicode_whitespace_metadata() {
    for (body, spaces_after) in [
        ("A <img src='one'/><img src='two'/> B", vec![false, true]),
        ("A <img src='one'/>\u{a0}B", vec![true]),
        ("A <img src='one'/> <img src='two'/> B", vec![true, true]),
        (
            "A <img src='one'/><span></span><img src='two'/> B",
            vec![false, true],
        ),
        (
            "A <img src='one'/><span> </span><img src='two'/> B",
            vec![true, true],
        ),
    ] {
        let source = parse_source(&format!("<p id='hide'>{body}</p>"));
        let block = &source.blocks[0];
        assert_eq!(
            block
                .images
                .iter()
                .map(|image| image.space_after)
                .collect::<Vec<_>>(),
            spaces_after,
            "{body}"
        );
        let projection = Projection::new(block, &masks(&source, &[], &["hide"], &[])).unwrap();
        assert_eq!(projection.text(), block.text, "{body}");
        assert_eq!(
            projection
                .images()
                .unwrap()
                .iter()
                .map(|image| image.space_after)
                .collect::<Vec<_>>(),
            spaces_after,
            "{body}"
        );
        for (image, canonical) in projection.images().unwrap().iter().zip(&block.images) {
            assert_eq!(image.offset, canonical.offset, "{body}");
        }
    }
}

#[test]
fn images_are_filtered_and_positioned_after_the_remaining_flow_whitespace() {
    let source = parse_source(
        "<p>A <img id='cut' src='gone.png'/><span id='cut-text'>X</span><img src='keep.png'/> B</p>",
    );
    let block = &source.blocks[0];
    let projection =
        Projection::new(block, &masks(&source, &["cut", "cut-text"], &[], &[])).unwrap();
    assert_eq!(projection.text(), "A B");
    assert_eq!(
        projection.images().unwrap(),
        &[Image {
            source_index: 1,
            offset: 2,
            space_after: true,
            painted: true
        }]
    );
    assert_eq!(block.images.len(), 2);

    let source = parse_source("<p>A <img src='keep.png'/><br id='cut'/>B</p>");
    let projection =
        Projection::new(&source.blocks[0], &masks(&source, &["cut"], &[], &[])).unwrap();
    assert_eq!(projection.text(), "A B");
    assert_eq!(projection.images().unwrap()[0].offset, 2);
    let source = parse_source("<p>A <img src='keep.png'/><br/><span id='cut'>B</span></p>");
    let projection =
        Projection::new(&source.blocks[0], &masks(&source, &["cut"], &[], &[])).unwrap();
    assert_eq!(projection.text(), "A\n");
    assert_eq!(projection.images().unwrap()[0].offset, 1);

    let source = parse_source("<p><img id='cut' src='gone.png'/><img src='keep.png'/></p>");
    let projection =
        Projection::new(&source.blocks[0], &masks(&source, &["cut"], &[], &[])).unwrap();
    assert_eq!(projection.text(), "");
    assert_eq!(projection.images().unwrap()[0].source_index, 1);
    assert_eq!(projection.images().unwrap()[0].offset, 0);
}

#[test]
fn combining_and_bidi_joins_keep_exact_utf8_boundaries_for_future_word_expansion() {
    for (body, expected) in [
        ("a<span id='cut'>x</span>\u{301}", "a\u{301}"),
        ("אב<span id='cut'>LATIN</span>العربية", "אבالعربية"),
        ("so<span id='cut'>TRAP</span>me", "some"),
        ("日<span id='cut'>TRAP</span>本", "日本"),
    ] {
        let source = parse_source(&format!("<p>{body}</p>"));
        let block = &source.blocks[0];
        let projection = Projection::new(block, &masks(&source, &["cut"], &[], &[])).unwrap();
        assert_eq!(projection.text(), expected);
        for offset in 0..=expected.len() {
            for affinity in [Affinity::Before, Affinity::After] {
                let value = projection.source_offset(offset, affinity);
                assert_eq!(value.is_some(), expected.is_char_boundary(offset));
                if let Some(value) = value {
                    assert!(block.text.is_char_boundary(value));
                }
            }
        }
        for offset in 0..=block.text.len() {
            if !block.text.is_char_boundary(offset) {
                assert_eq!(projection.project_offset(offset, Affinity::After), None);
                assert_eq!(projection.project_nearest(offset, Affinity::Before), None);
            }
        }
        let kept = projection.source_spans(0..expected.len()).unwrap();
        assert_eq!(
            kept.iter()
                .map(|span| &block.text[span.clone()])
                .collect::<String>(),
            expected
        );
    }
}

#[test]
fn projected_word_and_search_boundaries_map_back_to_disjoint_canonical_spans() {
    let source = parse_source("<p>some<span id='cut'>TRAP</span>where</p>");
    let block = &source.blocks[0];
    let projection = Projection::new(block, &masks(&source, &["cut"], &[], &[])).unwrap();
    let word = crate::selection::word_range(projection.text(), 5).unwrap();
    assert_eq!(&projection.text()[word.clone()], "somewhere");
    let matches = crate::search::definition_hits(projection.text(), "somewhere").unwrap();
    assert_eq!(matches, vec![0..9]);
    let spans = projection.source_spans(word.clone()).unwrap();
    assert_eq!(spans, vec![0..4, 8..13]);
    assert_eq!(
        spans
            .iter()
            .map(|span| &block.text[span.clone()])
            .collect::<String>(),
        "somewhere"
    );
    assert_eq!(
        projection.source_offset(word.start, Affinity::After),
        Some(0)
    );
    assert_eq!(
        projection.source_offset(word.end, Affinity::Before),
        Some(13)
    );
    assert!(
        crate::search::definition_hits(projection.text(), "TRAP")
            .unwrap()
            .is_empty()
    );
    // This verifies the view contract, not application integration. Actual word
    // expansion, copy/snippets and both search workers still need migration.
}

#[test]
fn ruby_exclusions_and_legacy_anchor_projection_are_unchanged() {
    let xml = "<html><body><p>A <ruby id='ruby'>日<rp>(</rp><rt>にち</rt><rp>)</rp>本<rt>ほん</rt></ruby> <span id='cut'>X</span>Z</p><p id='later'>Another paragraph.</p></body></html>";
    let source = Source::parse(xml).unwrap();
    let revision = source.revision;
    let fragments = source.fragments.clone();
    let block = &source.blocks[0];
    assert_eq!(block.text, "A 日本 XZ");
    let old = crate::revision(xml);
    assert_ne!(old, revision);
    let old_text = "A 日(にち)本ほん XZ";
    let legacy = ["日", "本", "Z"]
        .map(|word| source.project_offset(block, old, old_text.find(word).unwrap()));
    let projection = Projection::new(block, &masks(&source, &["cut"], &[], &[])).unwrap();
    assert_eq!(projection.text(), "A 日本 Z");
    assert_eq!(block.ruby.len(), 2);
    assert!(!projection.text().contains("にち"));
    assert!(!projection.text().contains("ほん"));
    assert_eq!(source.revision, revision);
    assert_eq!(source.fragments, fragments);
    assert_eq!(
        ["日", "本", "Z"].map(|word| source.project_offset(
            block,
            old,
            old_text.find(word).unwrap()
        )),
        legacy
    );
    assert_eq!(
        Projection::new(block, &masks(&source, &["ruby"], &[], &[]))
            .unwrap()
            .text(),
        "A XZ"
    );
    assert_eq!(block.text, "A 日本 XZ");
}

#[test]
fn html_hidden_and_other_structural_omissions_remain_irreversible_source_behavior() {
    let xml = "<html><body><p id='keep'>A<span hidden='hidden'>lost</span><script>lost</script><span id='cut'>X</span>B</p><p hidden='hidden'>Never extracted</p></body></html>";
    let source = Source::parse(xml).unwrap();
    assert_eq!(source.revision, crate::revision(xml));
    assert_eq!(source.blocks.len(), 1);
    assert_eq!(source.blocks[0].text, "AXB");
    assert_eq!(source.fragments["keep"], (source.blocks[0].key, 0));
    let projection =
        Projection::new(&source.blocks[0], &masks(&source, &["cut"], &[], &[])).unwrap();
    assert_eq!(projection.text(), "AB");
    assert!(!projection.text().contains("lost"));
}

#[test]
fn provenance_limits_are_explicit_and_the_identity_path_still_works() {
    let source = parse_source(&format!(
        "<p id='cut'>{}</p>",
        "a ".repeat(MAX_ORIGIN_EVENTS / 2 + 2)
    ));
    let block = &source.blocks[0];
    assert!(!block.flow_origins.complete());
    assert!(block.flow_origins.events.is_empty());
    let identity = Projection::new(block, &[]).unwrap();
    assert!(identity.is_identity());
    assert_eq!(identity.text().as_ptr(), block.text.as_ptr());
    assert!(matches!(
        Projection::new(block, &masks(&source, &["cut"], &[], &[])),
        Err(ProjectionError::IncompleteOrigins)
    ));
}

#[test]
fn missing_element_and_corrupt_utf8_origins_fail_without_changing_canonical_text() {
    let mut source = parse_source("<p>A<span id='cut'>X</span>é</p>");
    let mut flow = masks(&source, &["cut"], &[], &[]);
    flow.truncate(1);
    flow[0].painted = false;
    assert!(matches!(
        Projection::new(&source.blocks[0], &flow),
        Err(ProjectionError::MissingElement(_))
    ));
    let flow = masks(&source, &["cut"], &[], &[]);
    let block = &mut source.blocks[0];
    let last = block.flow_origins.events.last_mut().unwrap();
    if let Event::Text { bytes, .. } = last {
        bytes.start += 1;
    } else {
        panic!("Expected final text origin");
    }
    assert!(matches!(
        Projection::new(block, &flow),
        Err(ProjectionError::InvalidOrigin)
    ));
    assert_eq!(block.text, "AXé");

    let mut source = parse_source("<p>A <span id='cut'>X</span>B</p>");
    let flow = masks(&source, &["cut"], &[], &[]);
    let block = &mut source.blocks[0];
    let invalid = block.text.len() + 1;
    let space = block
        .flow_origins
        .events
        .iter_mut()
        .find_map(|event| {
            if let Event::Space { at, .. } = event {
                Some(at)
            } else {
                None
            }
        })
        .unwrap();
    *space = invalid;
    assert!(matches!(
        Projection::new(block, &flow),
        Err(ProjectionError::InvalidOrigin)
    ));
    assert_eq!(block.text, "A XB");
}

#[cfg(feature = "kindle")]
#[test]
#[ignore = "requires READER_FLOW_FOOTNOTE_EPUB pointing to the authorized private original 034"]
fn original_hidden_footnotes_retain_canonical_targets_and_independent_popup_text() {
    use crate::{epub::Book, footnotes};
    use ring::digest::{SHA256, digest};
    let path = std::path::PathBuf::from(std::env::var_os("READER_FLOW_FOOTNOTE_EPUB").unwrap());
    let before = digest(&SHA256, &std::fs::read(&path).unwrap());
    let mut book = Book::open(&path).unwrap();
    let mut notes = 0;
    let mut omitted = 0;
    for chapter in 0..book.chapters.len() {
        let xml = book.chapter(chapter).unwrap();
        let markup = crate::epub_xml::without_plain_doctype(&xml);
        let doc = roxmltree::Document::parse(&markup).unwrap();
        let ids: Vec<_> = doc
            .descendants()
            .filter(|node| {
                node.has_tag_name("aside")
                    && node
                        .attribute(("http://www.idpf.org/2007/ops", "type"))
                        .is_some_and(|kind| {
                            kind.split_ascii_whitespace()
                                .any(|token| token == "footnote")
                        })
            })
            .filter_map(|node| node.attribute("id"))
            .collect();
        if ids.is_empty() {
            continue;
        }
        let source = Source::parse(&xml).unwrap();
        let flows = masks(&source, &ids, &[], &[]);
        for block in &source.blocks {
            let projected = Projection::new(block, &flows).unwrap();
            if projected.text().len() < block.text.len() {
                omitted += 1;
            }
        }
        for id in ids {
            let &(key, offset) = source
                .fragments
                .get(id)
                .expect("Original footnote anchor must remain canonical");
            let block = source.blocks.iter().find(|block| block.key == key).unwrap();
            assert!(offset <= block.text.len());
            assert!(Projection::new(block, &flows).unwrap().text().is_empty());
            let excerpt = footnotes::extract(
                &xml,
                id,
                &footnotes::ReferenceContext {
                    kind: footnotes::Reference::Explicit,
                    label: "1".into(),
                    in_prose: true,
                    source_block: 0,
                },
                std::cmp::Ordering::Equal,
                false,
                false,
            )
            .expect("Semantic footnote popup remains independently readable");
            let popup = Source::parse(&excerpt).unwrap();
            assert!(
                popup
                    .blocks
                    .iter()
                    .any(|block| !block.text.trim().is_empty())
            );
            assert!(popup.stylesheets.is_empty());
            notes += 1;
        }
    }
    assert!(notes > 0 && omitted > 0);
    let after = digest(&SHA256, &std::fs::read(&path).unwrap());
    assert_eq!(before.as_ref(), after.as_ref());
    // The namespace selector's cascade itself is deliberately not claimed here:
    // this foundation receives effective masks, not arbitrary CSS selectors.
}
