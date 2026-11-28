use super::*;
use crate::epub::Destination;

fn anchor(original: &Source, xhtml: &str, block: usize, offset: usize) -> Anchor {
    Anchor {
        raw_caret: None,
        book: crate::revision(xhtml),
        chapter: "document.xhtml".into(),
        revision: original.revision,
        block,
        offset,
        illustration: None,
        side_bias: None,
    }
}

#[test]
fn small_and_markup_heavy_books_keep_the_original_chapter() {
    let small = "<html><body><p>A small page.</p></body></html>";
    assert!(
        prepare(small, "document.xhtml", "Book", &[])
            .unwrap()
            .is_none()
    );
    // Larger than the old XHTML limit while normalized text stays small.
    let long_attribute = "a".repeat(600 * 1024);
    let markup = format!("<html><body><p title='{long_attribute}'>Small text.</p></body></html>");
    assert!(
        prepare(&markup, "document.xhtml", "Book", &[])
            .unwrap()
            .is_none()
    );
}

#[test]
fn single_long_paragraph_keeps_text_and_maps_anchors_across_parts() {
    let paragraph = "A multibyte café &amp; a passage ".repeat(24_000);
    let xml = format!("<html><body><p>{paragraph}</p></body></html>");
    let old = source(&xml).unwrap();
    let old_block = &old.blocks[0];
    let split = prepare(&xml, "document.xhtml", "Book", &[])
        .unwrap()
        .unwrap();
    assert!(split.chapters.len() > 1);
    let span = split.history.blocks.get(&old_block.key).unwrap();
    assert!(span.len() > 1);
    let boundary = span[0].original.end;
    for (offset, end) in [
        (0, false),
        (boundary, false),
        (boundary, true),
        (old_block.text.len() - 1, false),
    ] {
        let original = anchor(&old, &xml, old_block.key, offset);
        let (chapter, current) = split.history.migrate(&original, end).unwrap().unwrap();
        let parsed = source(&split.chapters[chapter].xhtml).unwrap();
        let block = parsed
            .blocks
            .iter()
            .find(|block| block.key == current.block)
            .unwrap();
        assert!(block.text.is_char_boundary(current.offset));
        assert_eq!(current.book, original.book);
        if offset < old_block.text.len() && current.offset < block.text.len() {
            assert_eq!(
                old_block.text[offset..].chars().next(),
                block.text[current.offset..].chars().next()
            );
        }
    }
    let mut wrong = anchor(&old, &xml, old_block.key, 0);
    wrong.book ^= 1;
    assert!(split.history.migrate(&wrong, false).is_err());
}

#[test]
fn duplicate_and_percent_encoded_fragments_use_first_actual_destination() {
    let spacer = "<p>Coastal notes and reading text.</p>".repeat(23_000);
    let xml = format!(
        "<html><body><p id='duplicate'>First</p>{spacer}<p id='duplicate'>Second</p><p id='shore café'>Target</p><p><a href='#duplicate'>Back</a><a href='#shore%20caf%C3%A9'>Go</a></p></body></html>"
    );
    let toc = [TocEntry {
        label: "Target".into(),
        depth: 0,
        destination: Some(Destination {
            path: "document.xhtml".into(),
            fragment: Some("shore café".into()),
        }),
    }];
    let split = prepare(&xml, "document.xhtml", "Book", &toc)
        .unwrap()
        .unwrap();
    let owner = split
        .chapters
        .iter()
        .find(|part| part.xhtml.contains("id='shore café'"))
        .unwrap();
    assert_eq!(split.toc[0].destination.as_ref().unwrap().path, owner.path);
    let link_part = split.chapters.last().unwrap();
    let xml = roxmltree::Document::parse(&link_part.xhtml).unwrap();
    let hrefs = xml
        .descendants()
        .filter(|node| node.has_tag_name("a"))
        .filter_map(|node| node.attribute("href"))
        .collect::<Vec<_>>();
    assert!(hrefs.iter().any(|href| *href == "document.xhtml#duplicate"));
    assert!(
        hrefs
            .iter()
            .any(|href| href.ends_with("#shore%20caf%C3%A9"))
    );
}
