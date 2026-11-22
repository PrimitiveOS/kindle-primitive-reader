use super::*;
use crate::{Book, Engine, Settings, SourceBoundary};

fn open(html: &str, publisher: bool, font: f32) -> Probe {
    Probe::from_book_content(
        Book::open(&crate::test_fixture("field-notes-epub3.epub")).unwrap(),
        Engine::Parley,
        &[crate::test_fixture("fonts/NotoSans-Regular.ttf")],
        Settings {
            width: 440,
            height: 420,
            margin: 24,
            font_size: font,
            publisher_styles: publisher,
            ..Default::default()
        },
        0,
        Some(html.into()),
    )
    .unwrap()
}

fn capture_if_requested(probe: &mut Probe, page: usize, name: &str) {
    let Some(path) = std::env::var_os("READER_BOUNDARY_CAPTURE_DIR") else {
        return;
    };
    let path = std::path::PathBuf::from(path);
    std::fs::create_dir_all(&path).unwrap();
    let pixels = probe.capture(page).unwrap();
    image::GrayImage::from_raw(probe.settings.width, probe.settings.height, pixels)
        .unwrap()
        .save(path.join(format!("{name}.png")))
        .unwrap();
}

#[test]
fn legacy_anchor_json_is_unchanged_and_boundary_payloads_are_typed() {
    for raw in [
        r#"{"book":1,"chapter":"ch.xhtml","revision":2,"block":3,"offset":4}"#,
        r#"{"book":1,"chapter":"ch.xhtml","revision":2,"block":3,"offset":0,"illustration":1}"#,
    ] {
        let anchor: Anchor = serde_json::from_str(raw).unwrap();
        assert!(anchor.boundary.is_none());
        assert_eq!(serde_json::to_string(&anchor).unwrap(), raw);
    }
    let mut anchor: Anchor = serde_json::from_str(
        r#"{"book":1,"chapter":"ch.xhtml","revision":2,"block":3,"offset":4}"#,
    )
    .unwrap();
    anchor.boundary = Some(SourceBoundary {
        origin: 25,
        relative: 0,
    });
    let bytes = serde_json::to_vec(&anchor).unwrap();
    assert_eq!(serde_json::from_slice::<Anchor>(&bytes).unwrap(), anchor);
    assert!(
        std::str::from_utf8(&bytes)
            .unwrap()
            .contains(r#""boundary":{"origin":25}"#)
    );
    for invalid in [
        r#"{"origin":-1}"#,
        r#"{"origin":3,"element":0}"#,
        r#"false"#,
    ] {
        assert!(serde_json::from_str::<SourceBoundary>(invalid).is_err());
    }
}

#[test]
fn visible_li_hidden_children_get_real_origin_anchored_marker_geometry_and_hidden_owners_do_not() {
    let html = "<html><body><p>Before.</p><ol><li id='empty'><p id='hidden-child' style='display:none'>HIDDEN CHILD</p></li><li>Next.</li><li id='removed' style='display:none'>REMOVED</li></ol><p>After.</p></body></html>";
    let probe = open(html, true, 24.);
    let anchor = probe.fragment("empty").unwrap();
    let token = anchor
        .boundary
        .as_ref()
        .expect("marker-only target must retain DOM identity");
    assert_eq!(token.origin, html.find("<li id='empty'").unwrap());
    assert_eq!(
        probe.source_presentation(&anchor),
        Some(SourcePresentation::Normal)
    );
    assert!(probe.page_for(&anchor).is_some());
    assert!(probe.fragment("hidden-child").is_none());
    assert!(probe.fragment("removed").is_none());
    assert!(
        probe
            .select_range(&anchor, &probe.matches("After.")[0])
            .is_none()
    );
    assert!(
        probe
            .select_source_range(&anchor, &probe.matches("After.")[0])
            .is_none()
    );
    let encoded = serde_json::to_vec(&anchor).unwrap();
    let stored: Anchor = serde_json::from_slice(&encoded).unwrap();
    for font in [18., 32.] {
        let changed = open(html, true, font);
        assert!(changed.page_for(&stored).is_some());
        assert_eq!(changed.fragment("empty").unwrap(), stored);
    }
    let revealed = open(html, false, 24.);
    assert!(revealed.page_for(&stored).is_some());
    assert_eq!(revealed.source.revision, probe.source.revision);
    let mut invalid = stored.clone();
    invalid.boundary.as_mut().unwrap().origin += 1;
    assert!(probe.page_for(&invalid).is_none());
    invalid = stored.clone();
    invalid.revision ^= 1;
    assert!(probe.page_for(&invalid).is_none());
    invalid = stored.clone();
    invalid.block = probe.matches("After.")[0].block;
    assert!(probe.page_for(&invalid).is_none());
    invalid = stored;
    invalid.boundary.as_mut().unwrap().relative = 10001;
    assert!(probe.page_for(&invalid).is_none());
}

#[test]
fn padded_bordered_generated_boxes_render_and_later_pages_resume_without_aliasing_empty_neighbours()
{
    let html = "<html><body><div id='box' style='padding:400px 12px;background:#ddd;border:3px solid black'><p style='display:none'>REMOVED SOURCE</p></div><span id='empty'/><p id='after'>AFTER BOX</p></body></html>";
    let mut probe = open(html, true, 24.);
    let boundary = probe.fragment("box").unwrap();
    assert!(boundary.boundary.is_some());
    assert_eq!(
        probe.source_presentation(&boundary),
        Some(SourcePresentation::Normal)
    );
    assert!(probe.page_count() >= 2);
    let pixels = probe.capture(0).unwrap();
    assert!(pixels.iter().any(|&p| p == 221));
    assert!(pixels.iter().any(|&p| p == 0));
    capture_if_requested(&mut probe, 0, "generated-box-page1");
    capture_if_requested(&mut probe, 1, "generated-box-page2");
    let resume = probe.page_anchor(1).unwrap();
    assert_eq!(
        resume.boundary.as_ref().unwrap().origin,
        boundary.boundary.as_ref().unwrap().origin
    );
    assert!(resume.boundary.as_ref().unwrap().relative > 0);
    assert_eq!(probe.page_for(&resume), Some(1));
    let stored: Anchor = serde_json::from_slice(&serde_json::to_vec(&resume).unwrap()).unwrap();
    assert_eq!(open(html, true, 24.).page_for(&stored), Some(1));
    assert!(open(html, true, 30.).page_for(&stored).is_some());
    let empty = probe.fragment("empty").unwrap();
    assert_ne!(
        empty.boundary.as_ref().unwrap().origin,
        boundary.boundary.as_ref().unwrap().origin
    );
    assert!(probe.anchor_top(&empty).unwrap() > probe.anchor_top(&boundary).unwrap());
    assert!(empty.boundary.is_some());
    assert_ne!(empty, probe.fragment("after").unwrap());
    assert_eq!(
        probe.source.fragments["empty"],
        probe.source.fragments["after"]
    );
    assert_eq!(probe.coverage().1, open(html, false, 24.).coverage().1);
    assert!(probe.matches("REMOVED SOURCE").is_empty());
    let revealed = open(html, false, 24.);
    assert!(revealed.page_for(&boundary).is_some());
}

#[test]
fn generated_geometry_between_viewport_and_next_text_is_never_skipped_by_pagination() {
    let lines = [
        crate::Line {
            block: 1,
            bytes: 0..1,
            spans: None,
            top: 0.,
            bottom: 100.,
            missing_glyphs: 0,
        },
        crate::Line {
            block: 2,
            bytes: 0..1,
            spans: None,
            top: 180.,
            bottom: 190.,
            missing_glyphs: 0,
        },
    ];
    let legacy = crate::Pagination::with_breaks(&lines, 100., 190., &[]).unwrap();
    assert_eq!(legacy.bounds, [0., 180., 190.]);
    for area in [90. ..170., 100. ..170., 110. ..170.] {
        let pages =
            crate::Pagination::with_geometry(&lines, 100., 190., &[], &[area.clone()]).unwrap();
        assert_eq!(pages.bounds, [0., 100., 190.], "{area:?}");
        for y in [110., 169.] {
            let range = pages.range(pages.page(y)).unwrap();
            assert!(range.contains(&y));
            assert!(y - range.start < 100.);
        }
    }
}

#[test]
fn visible_empty_inline_owner_before_removed_prefix_keeps_its_own_projected_caret() {
    let html = "<html><body><p>Before.</p><p id='parent'><span id='point'/><span style='display:none'>REMOVED PREFIX</span>Retained text.</p><p id='hidden' style='display:none'><span id='hidden-point'/>Hidden paragraph.</p><p>After.</p></body></html>";
    let mut probe = open(html, true, 24.);
    let point = probe.fragment("point").unwrap();
    assert!(point.boundary.is_some());
    assert_eq!(
        point.boundary.as_ref().unwrap().origin,
        html.find("<span id='point'").unwrap()
    );
    assert_eq!(
        probe.page_for(&point),
        probe.page_for(&probe.matches("Retained")[0])
    );
    assert_eq!(
        probe.source_presentation(&point),
        Some(SourcePresentation::Normal)
    );
    assert!(probe.fragment("hidden-point").is_none());
    let stored: Anchor = serde_json::from_slice(&serde_json::to_vec(&point).unwrap()).unwrap();
    assert!(open(html, true, 32.).page_for(&stored).is_some());
    assert!(open(html, false, 24.).page_for(&stored).is_some());
    assert!(
        probe
            .select_range(&point, &probe.matches("Retained")[0])
            .is_none()
    );
    assert_eq!(
        probe.source.fragments,
        open(html, false, 24.).source.fragments
    );
    assert_eq!(
        probe.source.revision,
        open(html, false, 24.).source.revision
    );
    capture_if_requested(&mut probe, 0, "inline-caret-before-css-prefix");
}

#[test]
fn an_unavailable_typed_boundary_never_snaps_to_its_legacy_text_fallback() {
    let html = "<html><body><p>Before.</p><div id='point' style='display:none'/><p>Unrelated fallback text.</p></body></html>";
    let initial = open(html, false, 24.);
    let point = initial.fragment("point").unwrap();
    assert!(point.boundary.is_some());
    let removed = open(html, true, 24.);
    assert_eq!(initial.source.revision, removed.source.revision);
    assert!(removed.fragment("point").is_none());
    assert!(removed.page_for(&point).is_none());
    assert!(removed.restore_anchor(&point).is_none());
    assert!(removed.flow_boundary_page(&point).is_none());
    assert!(initial.page_for(&point).is_some());
}

#[test]
fn nested_empty_boxes_keep_real_padding_order_and_visibility_only_changes_paint() {
    let html = "<html><body><div id='outer' style='padding:20px;background:#ddd'><div id='inner' style='padding:30px;border:2px solid black;visibility:hidden'><p style='display:none'>SOURCE</p></div></div><p>After.</p></body></html>";
    let mut probe = open(html, true, 24.);
    let outer = probe.fragment("outer").unwrap();
    let inner = probe.fragment("inner").unwrap();
    assert!(probe.anchor_top(&outer).unwrap() < probe.anchor_top(&inner).unwrap());
    assert_eq!(
        probe.source_presentation(&inner),
        Some(SourcePresentation::VisibilityHidden)
    );
    assert!(probe.page_for(&inner).is_some());
    let pixels = probe.capture(0).unwrap();
    assert!(pixels.iter().any(|&p| p == 221));
    assert!(probe.source.fragments.contains_key("inner"));
    assert!(probe.matches("SOURCE").is_empty());
    capture_if_requested(&mut probe, 0, "nested-empty-boxes-visibility");
}

#[test]
fn normalized_numeric_ruby_owners_keep_canonical_fallback_keys_and_exact_current_revision() {
    let html = format!(
        "<html><body><p>{}<ruby>Base<rt>reading</rt></ruby></p><div id='box' style='padding:40px;background:#ddd'><p style='display:none'>HIDDEN</p></div><p>After.</p></body></html>",
        "&#146; ".repeat(60)
    );
    let probe = open(&html, true, 24.);
    let plain = open(&html, false, 24.);
    assert_ne!(probe.source.revision, crate::revision(&html));
    assert_eq!(probe.source.revision, plain.source.revision);
    assert_eq!(probe.source.fragments, plain.source.fragments);
    let anchor = probe.fragment("box").unwrap();
    assert!(anchor.boundary.is_some());
    assert_eq!((anchor.block, anchor.offset), probe.source.fragments["box"]);
    assert!(probe.page_for(&anchor).is_some());
    assert!(open(&html, true, 32.).page_for(&anchor).is_some());
    let mut stale = anchor.clone();
    stale.revision = crate::revision(&html);
    assert!(probe.page_for(&stale).is_none());
    assert_eq!(
        probe
            .select_source_range(
                &plain.matches("Base")[0],
                &Anchor {
                    offset: plain.matches("Base")[0].offset + 4,
                    ..plain.matches("Base")[0].clone()
                }
            )
            .unwrap()
            .text,
        "Base"
    );
}

#[test]
fn generated_borders_have_an_independent_cascade_and_leave_nonempty_table_contract_unchanged() {
    use crate::book_style::{BookStyle, borders::Pattern};
    let html = "<html><body><div id='parent' style='border:6px solid black'><p id='inherited' style='border:inherit'>Words.</p><p id='plain'>Other words.</p><table id='legacy' style='border:inherit'><tr><td>Cell</td></tr></table></div></body></html>";
    let source = crate::source::Source::parse(html).unwrap();
    let mut book = Book::open(&crate::test_fixture("field-notes-epub3.epub")).unwrap();
    let href = book.chapters[0].href.clone();
    let sheet =
        BookStyle::load_for_flow(&mut book, &href, &source, true, &Default::default()).unwrap();
    let styles = sheet.compute_for_dpi(&source.elements, 192);
    let at = |id| &styles[source.fragment_owner(id).unwrap()];
    assert!(at("parent").borders.is_none());
    assert!(at("inherited").borders.is_none());
    assert!(
        at("plain").generated_box_borders.is_none(),
        "borders are not implicitly inherited"
    );
    for id in ["parent", "inherited"] {
        let edge = at(id).generated_box_borders.as_ref().unwrap()[0];
        assert_eq!(edge.pattern, Pattern::Solid);
        assert_eq!(edge.width.box_pixels(392, 24.), 12.);
    }
    // The pre-existing table-only inheritance path never acquired the DIV's
    // border. Keep that contract; the scoped empty-box metadata is independent.
    assert_eq!(
        at("legacy").borders.as_ref().unwrap()[0].pattern,
        Pattern::None
    );
    assert_eq!(
        at("legacy").generated_box_borders.as_ref().unwrap()[0].pattern,
        Pattern::Solid
    );
    let mut with = open(
        "<html><body><div style='border:6px solid black'><p>Nonempty unchanged words.</p></div></body></html>",
        true,
        24.,
    );
    let mut without = open(
        "<html><body><div style='border:none'><p>Nonempty unchanged words.</p></div></body></html>",
        true,
        24.,
    );
    assert_eq!(with.capture(0).unwrap(), without.capture(0).unwrap());
    assert_eq!(with.coverage(), without.coverage());
}

#[test]
fn nested_generated_borders_reserve_space_before_percentage_padding_and_paint_real_edges() {
    let html = "<html><body><div id='outer' style='border:10px solid black;padding:0 10%'><div id='inner' style='border:3px solid black;padding:40px;background:#ddd'><span style='display:none'>SOURCE</span></div></div></body></html>";
    let mut probe = open(html, true, 24.);
    let outer = probe.fragment("outer").unwrap();
    let inner = probe.fragment("inner").unwrap();
    assert_eq!(probe.anchor_top(&outer), Some(0.));
    assert_eq!(probe.anchor_top(&inner), Some(10.));
    let pixels = probe.capture(0).unwrap();
    let pixel = |x: usize, y: usize| pixels[y * 440 + x];
    // Outer content width392 -20 border =372; 10% padding truncates to37.
    assert_eq!(pixel(24 + 2, 24 + 2), 0);
    assert_eq!(pixel(24 + 10 + 37, 24 + 10 + 1), 0);
    assert_eq!(pixel(24 + 10 + 37 + 3 + 1, 24 + 10 + 3 + 1), 221);
    assert_eq!(probe.coverage(), (0, 6));
    assert!(probe.matches("SOURCE").is_empty());
}

#[test]
#[ignore = "requires private authorized original empty-anchor cases, fonts and a new isolated profile directory"]
fn original_empty_anchors_survive_library_save_reopen_and_live_reflow_without_source_changes() {
    use crate::{
        library::{Bookmark, Library, ReadingPosition},
        session::{BookConfig, PAGE_HEIGHT, Reading},
    };
    use std::{collections::BTreeMap, path::PathBuf};
    #[derive(serde::Deserialize)]
    struct Case {
        book: PathBuf,
        id: String,
        chapter: usize,
        fragment: String,
    }
    #[derive(serde::Deserialize)]
    struct Cases {
        cases: Vec<Case>,
    }
    let cases: Cases = serde_json::from_slice(
        &std::fs::read(std::env::var_os("READER_BOUNDARY_CASES").unwrap()).unwrap(),
    )
    .unwrap();
    assert!(!cases.cases.is_empty() && cases.cases.len() <= 32);
    let fonts: Vec<_> =
        std::env::split_paths(&std::env::var_os("READER_BOUNDARY_FONTS").unwrap()).collect();
    assert!(!fonts.is_empty());
    let profile = PathBuf::from(std::env::var_os("READER_BOUNDARY_PROFILE").unwrap());
    std::fs::create_dir(&profile).unwrap();
    let mut hashes = BTreeMap::new();
    let mut rows = Vec::new();
    let mut boundaries = 0;
    let mut original039_audit = None;
    let digest = |path: &std::path::Path| {
        ring::digest::digest(&ring::digest::SHA256, &std::fs::read(path).unwrap())
            .as_ref()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    };
    for (n, case) in cases.cases.into_iter().enumerate() {
        hashes
            .entry(case.book.clone())
            .or_insert_with(|| digest(&case.book));
        let config = BookConfig {
            user_styles: Default::default(),
            user_hyphenation: Default::default(),
            engine: Engine::Parley,
            book: case.book.clone(),
            fonts: fonts.clone(),
            settings: Settings {
                width: primitive_app::WIDTH,
                height: PAGE_HEIGHT,
                font_size: 30.,
                ..Default::default()
            },
            scroll: false,
            facing_pages: false,
            hide_nonlinear: false,
            hidden_flows: Default::default(),
            chapter_offsets: Default::default(),
        };
        let mut reading = Reading::restore(config.clone(), case.chapter, 0, None).unwrap();
        if case.id == "039" && case.chapter == 7 && original039_audit.is_none() {
            let mut new_points = 0;
            let mut eligible = 0;
            let mut omitted = 0;
            for name in reading.probe.source.fragments.keys() {
                if reading.probe.fragment_presentation(name) == Some(SourcePresentation::CssOmitted)
                {
                    omitted += 1;
                    continue;
                }
                eligible += 1;
                let point = reading.probe.fragment(name).unwrap_or_else(|| {
                    panic!("original039 chapter7 eligible fragment remains unresolved: {name}")
                });
                assert!(reading.probe.page_for(&point).is_some());
                new_points += usize::from(point.boundary.is_some());
            }
            assert!(
                new_points > 0,
                "the original039 acceptance must exercise new DOM geometry"
            );
            original039_audit = Some(
                serde_json::json!({"chapter":7,"eligible_fragments":eligible,"located_fragments":eligible,"css_omitted_fragments":omitted,"new_boundary_targets":new_points,"unresolved":0}),
            );
        }

        let presentation = reading
            .probe
            .fragment_presentation(&case.fragment)
            .expect("original fragment owner");
        if presentation == SourcePresentation::CssOmitted {
            assert!(reading.probe.fragment(&case.fragment).is_none());
            rows.push(serde_json::json!({"id":case.id,"chapter":case.chapter,"case":n,"css_omitted":true}));
            continue;
        }
        reading.jump(case.chapter, Some(&case.fragment)).unwrap();
        let anchor = reading.anchor().unwrap();
        let page = reading.page;
        assert_eq!(
            reading.probe.page_for(&anchor),
            Some(page),
            "original{} case{n}",
            case.id
        );
        boundaries += usize::from(anchor.boundary.is_some());
        let position = ReadingPosition {
            chapter: case.chapter,
            page,
            anchor: Some(anchor.clone()),
        };
        let state = profile.join(format!("{n}-library.json"));
        let root = case.book.parent().unwrap();
        let mut library = Library::open(root, &state).unwrap();
        let entry = library.resolve(&case.book).unwrap();
        let id = library.identify(&entry).unwrap();
        let saved = library.remember(&entry, id, "Original boundary verification");
        saved.position = Some(position.clone());
        saved.bookmarks.push(Bookmark {
            label: "Empty original anchor".into(),
            created_at: None,
            position: position.clone(),
        });
        library
            .save_with(|path, bytes| std::fs::write(path, bytes))
            .unwrap();
        drop(library);
        let library = Library::open(root, &state).unwrap();
        let saved = library.state(id).unwrap();
        assert_eq!(saved.position.as_ref(), Some(&position));
        assert_eq!(saved.bookmarks[0].position, position);
        let mut reopened = Reading::restore(
            config.clone(),
            case.chapter,
            page,
            saved.position.as_ref().unwrap().anchor.as_ref(),
        )
        .unwrap();
        assert_eq!(reopened.anchor(), Some(anchor.clone()));
        assert_eq!(reopened.page, page);
        reopened
            .reflow(Settings {
                font_size: 33.,
                ..config.settings
            })
            .unwrap();
        assert_eq!(reopened.anchor(), Some(anchor.clone()));
        assert!(reopened.probe.page_for(&anchor).is_some());
        let mut legacy = anchor.clone();
        legacy.boundary = None;
        let legacy_bytes = serde_json::to_vec(&legacy).unwrap();
        assert!(
            !std::str::from_utf8(&legacy_bytes)
                .unwrap()
                .contains("boundary")
        );
        assert_eq!(
            serde_json::to_vec(&serde_json::from_slice::<Anchor>(&legacy_bytes).unwrap()).unwrap(),
            legacy_bytes
        );
        rows.push(serde_json::json!({"id":case.id,"chapter":case.chapter,"case":n,"new_boundary":anchor.boundary.is_some(),"reopened_page":page,"reflowed_page":reopened.page}));
    }
    assert!(
        boundaries > 0 && rows.len() >= 3,
        "must validate actual new boundary anchors across original cases"
    );
    let mut sources = Vec::new();
    for (path, before) in hashes {
        assert_eq!(digest(&path), before);
        sources.push(serde_json::json!({"id":path.file_stem().unwrap().to_string_lossy(),"sha256":before,"unchanged":true}));
    }
    let receipt = serde_json::json!({"new_boundaries":boundaries,"cases":rows,"sources":sources,"source_changes":false,"original039_chapter7":original039_audit});
    std::fs::write(
        std::env::var_os("READER_BOUNDARY_RECEIPT").unwrap(),
        serde_json::to_vec_pretty(&receipt).unwrap(),
    )
    .unwrap();
}
