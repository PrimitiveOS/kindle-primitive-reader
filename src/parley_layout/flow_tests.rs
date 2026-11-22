use super::*;
use crate::{Book, Engine, Probe, Renderer, Settings};

#[test]
fn marker_rehomes_to_retained_content_and_keeps_empty_item_strut_without_hidden_source_coverage() {
    let html = "<html><body><ol><li><p style='display:none'>FIRST TRAP</p><p>Kept first.</p></li><li><p style='display:none'>EMPTY TRAP</p></li><li style='visibility:hidden'><p style='visibility:visible'>Visible descendant.</p></li><li style='display:none'>REMOVED ITEM</li><li>Last item.</li></ol></body></html>";
    let probe = Probe::from_book_content(
        Book::open(&crate::test_fixture("field-notes-epub3.epub")).unwrap(),
        Engine::Parley,
        &[crate::test_fixture("fonts/NotoSans-Regular.ttf")],
        Settings::default(),
        0,
        Some(html.into()),
    )
    .unwrap();
    let Renderer::Parley(chapter) = &probe.renderer else {
        panic!("not parley")
    };
    assert_eq!(chapter.markers.len(), 4);
    assert_eq!(
        chapter
            .markers
            .iter()
            .map(|m| m.painted)
            .collect::<Vec<_>>(),
        [true, true, false, true]
    );
    assert!(chapter.markers.windows(2).all(|m| m[0].bottom <= m[1].top));
    assert!(!probe.source.blocks.iter().any(|b| b.text.contains("4. ")));
    assert!(probe.matches("TRAP").is_empty());
    assert!(probe.matches("REMOVED ITEM").is_empty());
    assert_eq!(probe.matches("Visible descendant.").len(), 1);
    let (_, total) = probe.coverage();
    let canonical_total = probe
        .source
        .blocks
        .iter()
        .flat_map(|b| b.text.chars())
        .filter(|c| !c.is_whitespace())
        .count();
    assert_eq!(total, canonical_total);
}
