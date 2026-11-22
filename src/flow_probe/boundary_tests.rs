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
}
