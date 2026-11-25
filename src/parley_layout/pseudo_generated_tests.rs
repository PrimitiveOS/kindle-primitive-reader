use crate::{Anchor, Book, Engine, Probe, Renderer, Settings, user_styles};

const FIXTURE: &str = include_str!("../../tests/fixtures/pseudo-generated.xhtml");
const TWEAK: &str = "no_pseudo_element_before_after";

fn open(markup: &str, settings: Settings, snapshot: &user_styles::Snapshot) -> Probe {
    Probe::from_book_with_rules(
        Book::open(&crate::test_fixture("field-notes-epub3.epub")).unwrap(),
        Engine::Parley,
        &[crate::test_fixture("fonts/NotoSans-Regular.ttf")],
        settings,
        0,
        Some(markup.into()),
        &Default::default(),
        snapshot,
    )
    .unwrap()
}

fn rendered(probe: &Probe, key: usize) -> Option<&str> {
    let Renderer::Parley(chapter) = &probe.renderer else {
        unreachable!()
    };
    chapter
        .paragraphs
        .iter()
        .find(|paragraph| paragraph.key == key)?
        .inline_text
        .as_ref()
        .map(|content| content.text.as_str())
}

#[test]
fn pseudo_generated_publisher_geometry_preserves_source_search_copy_links_and_anchors() {
    let settings = Settings {
        width: 617,
        height: 780,
        margin: 30,
        font_size: 30.,
        ..Settings::default()
    };
    let mut styles = user_styles::Data::default();
    let mut decorated = open(FIXTURE, settings, &styles.snapshot(1).unwrap());
    styles.choose(1, TWEAK, Some(true)).unwrap();
    let mut plain = open(FIXTURE, settings, &styles.snapshot(1).unwrap());
    styles.choose(1, TWEAK, Some(false)).unwrap();
    let mut restored = open(FIXTURE, settings, &styles.snapshot(1).unwrap());
    let anchor = decorated.fragment("alpha").unwrap();
    assert_eq!(plain.fragment("alpha").unwrap(), anchor);
    assert_eq!(restored.fragment("alpha").unwrap(), anchor);
    assert_eq!(plain.restore_anchor(&anchor), Some(anchor.clone()));
    let composite = rendered(&decorated, anchor.block).unwrap();
    assert!(
        composite.starts_with("[Chart seven] Alpha tide"),
        "{composite:?}"
    );
    assert!(composite.contains("(amber)"), "{composite:?}");
    assert!(composite.contains("SIGNAL follows"), "{composite:?}");
    assert!(composite.ends_with(" END"), "{composite:?}");
    assert!(!composite.contains("SECRET") && !composite.contains("IGNORE"));
    assert!(rendered(&plain, anchor.block).is_none());

    let geometry = |probe: &Probe, start: &Anchor, word: &str| {
        let page = probe.page_for(start).unwrap();
        let end = Anchor {
            offset: start.offset + word.len(),
            ..start.clone()
        };
        let selection = probe.select_range(start, &end).unwrap();
        assert_eq!(selection.text, word);
        probe.selection_rects(page, &selection)[0]
    };
    let original = "Alpha tide amber follows the pilings.";
    let end = Anchor {
        offset: anchor.offset + original.len(),
        ..anchor.clone()
    };
    for probe in [&decorated, &plain, &restored] {
        let selection = probe.select_range(&anchor, &end).unwrap();
        assert_eq!(selection.text, original);
        assert_eq!(probe.matches(original), vec![anchor.clone()]);
        assert!(probe.matches("Chart seven").is_empty());
        assert!(probe.matches("SIGNAL").is_empty());
        assert!(probe.matches("tide (amber)").is_empty());
        let trail = probe.fragment("trail").unwrap();
        assert_eq!(Some(trail.clone()), plain.fragment("trail"));
        let hit = geometry(probe, &trail, "follows");
        let (x, y) = (
            (hit.x + hit.width / 2) as f32,
            (hit.y + hit.height / 2) as f32,
        );
        let page = probe.page_for(&trail).unwrap();
        assert_eq!(probe.word_at(page, x, y).unwrap().start, trail);
        assert!(probe.link_at(page, x, y).is_some());
    }
    let shifted = geometry(&decorated, &anchor, "Alpha");
    let ordinary = geometry(&plain, &anchor, "Alpha");
    assert!(shifted.x > ordinary.x + 100);
    let y = (shifted.y + shifted.height / 2) as f32;
    assert!(decorated.word_at(0, 55., y).is_none());
    assert!(decorated.text_position_at(0, 55., y).is_none());
    assert_eq!(geometry(&restored, &anchor, "Alpha"), shifted);
    let decorated_frame = decorated.capture(0).unwrap();
    let plain_frame = plain.capture(0).unwrap();
    assert_ne!(plain_frame, decorated_frame);
    assert_eq!(restored.capture(0).unwrap(), decorated_frame);

    #[cfg(feature = "kindle")]
    if let Some(directory) = std::env::var_os("READER_PSEUDO_GENERATED_CAPTURES") {
        for (name, frame) in [("publisher", decorated_frame), ("off", plain_frame)] {
            primitive_app::write_png(
                &std::path::Path::new(&directory)
                    .join(format!("reader-pseudo-generated-{name}.png")),
                settings.width,
                settings.height,
                &frame,
            )
            .unwrap();
        }
    }
}

#[test]
fn pseudo_generated_wrapped_prefix_and_suffix_pages_are_painted_and_unselectable() {
    let text = "Isolated decoration ".repeat(26);
    let markup = format!(
        r#"<html><head><style>p::before {{content:"{text}"}}</style></head><body>
      <p id="canonical">The tidal gate is open.</p></body></html>"#
    );
    let settings = Settings {
        width: 340,
        height: 240,
        margin: 30,
        font_size: 30.,
        ..Settings::default()
    };
    for side in ["before", "after"] {
        let mut probe = open(
            &markup.replace("::before", &format!("::{side}")),
            settings,
            &Default::default(),
        );
        assert!(probe.page_count() > 2);
        let anchor = probe.fragment("canonical").unwrap();
        assert_eq!(anchor.offset, 0);
        let source_page = probe.page_for(&anchor).unwrap();
        let generated_page = if side == "before" {
            0
        } else {
            probe.page_count() - 1
        };
        assert_ne!(source_page, generated_page);
        assert!(
            !probe
                .capture(generated_page)
                .unwrap()
                .iter()
                .all(|&pixel| pixel == 255)
        );
        assert!(probe.matches("Isolated decoration").is_empty());
        for y in (settings.margin..settings.height - settings.margin).step_by(12) {
            for x in (settings.margin + 7..settings.width - settings.margin).step_by(16) {
                assert!(probe.word_at(generated_page, x as f32, y as f32).is_none());
                assert!(
                    probe
                        .text_position_at(generated_page, x as f32, y as f32)
                        .is_none()
                );
            }
        }
    }
}
