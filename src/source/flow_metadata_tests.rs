//! Counter/target provenance must not change the canonical reading model.
use super::*;

fn element(source: &Source, id: &str) -> usize {
    source
        .elements
        .iter()
        .position(|element| element.id.as_deref() == Some(id))
        .unwrap()
}

fn group<'a>(source: &'a Source, id: &str) -> &'a crate::list_marker::CounterGroup {
    let index = element(source, id);
    source
        .list_counters
        .iter()
        .find(|group| group.element == index)
        .unwrap()
}

#[test]
fn counters_retain_explicit_natural_resets_and_non_li_direct_children() {
    let xml = "<html><body><ol id='list' start='5'><li id='a'>A</li><li id='b' value='6' style='display:none'>B</li><li id='c'>C</li><div id='extra' value='12' style='display:list-item'>D</div></ol></body></html>";
    let source = Source::parse(xml).unwrap();
    let inputs = group(&source, "list");
    assert_eq!(inputs.start, Some(5));
    assert!(!inputs.reversed);
    let expected: Vec<_> = [
        ("a", None),
        ("b", Some(6)),
        ("c", None),
        ("extra", Some(12)),
    ]
    .into_iter()
    .map(|(id, value)| crate::list_marker::CounterChild {
        element: element(&source, id),
        value,
    })
    .collect();
    assert_eq!(inputs.children, expected);
    // Legacy ordinals remain untouched until a layout consumer deliberately
    // chooses a computed counter. The div is still ordinary canonical text.
    assert_eq!(
        source
            .blocks
            .iter()
            .filter_map(|block| block.list_item.map(|item| item.ordinal))
            .collect::<Vec<_>>(),
        [5, 6, 7]
    );
    assert_eq!(source.blocks[3].text, "D");
    assert!(source.blocks[3].list_item.is_none());
    assert_eq!(source.revision, crate::revision(xml));
}

#[test]
fn reversed_counters_keep_implicit_start_and_exclude_structurally_hidden_children() {
    let xml = "<html><head><style>li{color:black}</style></head><body><ol id='list' reversed='reversed'><li id='first'>A</li><li id='omitted' hidden='hidden' value='99'>Omitted</li><li id='reset' value='2'>B</li><li id='last'>C</li></ol></body></html>";
    let source = Source::parse(xml).unwrap();
    let inputs = group(&source, "list");
    assert_eq!(inputs.start, None);
    assert!(inputs.reversed);
    assert_eq!(inputs.children.len(), 3);
    assert_eq!(inputs.children[1].value, Some(2));
    assert_eq!(inputs.children[0].element, element(&source, "first"));
    assert!(
        !inputs
            .children
            .iter()
            .any(|child| child.element == element(&source, "omitted"))
    );
    assert!(source.fragment_owner("omitted").is_none());
    assert!(!source.fragments.contains_key("omitted"));
    assert_eq!(
        source
            .blocks
            .iter()
            .map(|block| block.text.as_str())
            .collect::<Vec<_>>(),
        ["A", "B", "C"]
    );
    assert_eq!(source.blocks[0].list_item.unwrap().ordinal, 4);
    assert_eq!(source.blocks[1].list_item.unwrap().ordinal, 2);
    assert_eq!(source.blocks[2].list_item.unwrap().ordinal, 1);
}

#[test]
fn nested_groups_and_malformed_counter_attributes_keep_separate_raw_inputs() {
    let xml = "<html><body><ol id='outer' start='2147483648'><li id='first' value='bad'>A<ul id='inner'><li id='nested' value=' -7 '>B</li></ul></li><li id='last' value='2147483648'>C</li></ol></body></html>";
    let source = Source::parse(xml).unwrap();
    let outer = group(&source, "outer");
    assert_eq!(outer.start, None);
    assert_eq!(outer.children.len(), 2);
    assert!(outer.children.iter().all(|child| child.value.is_none()));
    assert_eq!(group(&source, "inner").children[0].value, Some(-7));
    assert_eq!(
        group(&source, "inner").children[0].element,
        element(&source, "nested")
    );
    assert_eq!(source.list_counters.len(), 2);
    assert!(
        source
            .list_counters
            .iter()
            .map(|group| group.children.len())
            .sum::<usize>()
            <= source.elements.len()
    );
}

#[test]
fn empty_fragment_owners_do_not_follow_their_canonical_fallback_paragraph() {
    let xml = "<html><body><p>North.</p><p id='marker'><a id='middle'/></p><a id='before'/><p id='south'>South.</p><a id='end'/></body></html>";
    let source = Source::parse(xml).unwrap();
    let south = (source.blocks[1].key, 0);
    for id in ["marker", "middle", "before"] {
        assert_eq!(source.fragments[id], south);
        assert_eq!(source.fragment_owner(id), Some(element(&source, id)));
        assert_ne!(source.fragment_owner(id), Some(source.blocks[1].element));
    }
    assert_eq!(source.fragments["end"], (south.0, "South.".len()));
    assert_eq!(source.fragment_owner("end"), Some(element(&source, "end")));
    assert_eq!(
        source.fragment_owner("south"),
        Some(source.blocks[1].element)
    );
    assert!(source.fragment_owner("absent").is_none());
}

#[test]
fn duplicate_fragment_owners_follow_the_existing_position_winner() {
    // Position extraction historically registers a leaf before its ancestors,
    // and rendered leaves before empty fallback anchors. Metadata must keep the
    // same winner rather than introduce a different DOM-first tie-break.
    let xml = "<html><body><a id='repeated'/><div id='repeated'><p id='repeated'>First.</p></div><p id='repeated'>Second.</p></body></html>";
    let source = Source::parse(xml).unwrap();
    assert_eq!(source.fragments["repeated"], (source.blocks[0].key, 0));
    assert_eq!(
        source.fragment_owner("repeated"),
        Some(source.blocks[0].element)
    );
    assert_eq!(
        source.elements[source.fragment_owner("repeated").unwrap()].tag,
        "p"
    );
}

#[test]
fn synthetic_kf8_fragment_names_keep_the_actual_empty_element_owner() {
    let xml = "<html><body><p>North.</p><a id='empty'/><p>South.</p></body></html>";
    let dom = Document::parse(xml).unwrap();
    let ordinal = dom
        .root_element()
        .descendants()
        .filter(crate::kf8::publisher_contents::html_node)
        .position(|node| node.attribute("id") == Some("empty"))
        .unwrap();
    let source = Source::parse_with_kindle_elements(xml, None, &BTreeSet::from([ordinal])).unwrap();
    let alias = crate::kf8::publisher_contents::element_name(ordinal);
    assert_eq!(source.fragments[&alias], source.fragments["empty"]);
    assert_eq!(
        source.fragment_owner(&alias),
        Some(element(&source, "empty"))
    );
    assert_ne!(
        source.fragment_owner(&alias),
        Some(source.blocks[1].element)
    );
}

#[test]
fn historical_numeric_aliases_keep_normalized_dom_owners_across_anonymous_boxes() {
    let xml = "<html><body><div> &#133; <p id='quote&#146;'>Alpha.</p><a id='empty&#146;'/>Tail.</div></body></html>";
    let source = Source::parse(xml).unwrap();
    assert_eq!(source.fragments["quote’"], source.fragments["quote\u{92}"]);
    assert_eq!(source.fragments["empty’"], source.fragments["empty\u{92}"]);
    for (current, historical) in [("quote’", "quote\u{92}"), ("empty’", "empty\u{92}")] {
        assert_eq!(
            source.fragment_owner(current),
            Some(element(&source, current))
        );
        assert_eq!(
            source.fragment_owner(historical),
            source.fragment_owner(current)
        );
    }
    assert!(source.element_origins.is_empty());
    assert_eq!(source.blocks[0].text, "…");
}

#[test]
fn ruby_annotation_owner_is_separate_from_the_canonical_base_text() {
    let xml = "<html><body><p id='paragraph'><ruby>日本<rt id='annotation' style='visibility:visible'>にほん</rt></ruby> coast.</p></body></html>";
    let source = Source::parse(xml).unwrap();
    let block = &source.blocks[0];
    assert_eq!(block.text, "日本 coast.");
    assert_eq!(block.ruby.len(), 1);
    assert_eq!(&block.text[block.ruby[0].bytes.clone()], "日本");
    assert_eq!(block.ruby[0].element, element(&source, "annotation"));
    assert_eq!(block.ruby[0].annotation, "にほん");
    // The existing empty-anchor pass retains an rt ID at the chapter's last
    // character. Keep that tuple, but remember that its owner is rt, not p.
    assert_eq!(
        source.fragments["annotation"],
        (block.key, block.text.len())
    );
    assert_eq!(
        source.fragment_owner("annotation"),
        Some(block.ruby[0].element)
    );
}
