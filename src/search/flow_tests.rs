use super::*;
use std::{fs, path::PathBuf, sync::atomic::AtomicU64};

struct Publication(PathBuf);
impl Publication {
    fn new(body: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "reader-search-flow-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        fs::write(root.join("book.xhtml"), format!(
            "<html><head><style>.cut{{display:none}}.unpainted{{visibility:hidden}}.restored{{visibility:visible}}</style></head><body>{body}</body></html>"
        )).unwrap();
        Self(root)
    }
    fn book(&self) -> Book {
        Book::open(&self.0.join("book.xhtml")).unwrap()
    }
}
impl Drop for Publication {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn canonical(book: &mut Book) -> Source {
    let href = book.chapters[0].href.clone();
    let xml = book.chapter(0).unwrap();
    book.chapter_source(&href, &xml).unwrap()
}

#[test]
fn whole_literal_and_regex_match_projected_words_and_keep_disjoint_canonical_intervals() {
    let publication = Publication::new("<p>Al<span class='cut'>SECRET</span>pha and Alpha.</p>");
    let mut book = publication.book();
    let original = fs::read(publication.0.join("book.xhtml")).unwrap();
    for mut search in [
        Search::new("alpha").unwrap(),
        Search::with_regular_expression("Al(pha)", false).unwrap(),
    ] {
        let hits = search.step(&mut book).unwrap();
        assert_eq!(hits.len(), 2);
        let source = canonical(&mut book);
        let block = &source.blocks[0];
        let first = &hits[0];
        assert_eq!(&first.snippet[first.matched.clone()], "Alpha");
        assert!(!first.snippet.contains("SECRET"));
        assert_eq!(first.source_spans.len(), 2);
        assert_eq!(
            first
                .source_spans
                .iter()
                .map(|span| &block.text[span.clone()])
                .collect::<String>(),
            "Alpha"
        );
        assert!(block.text[first.anchor.offset..first.end.offset].contains("SECRET"));
        assert_eq!(hits[1].source_spans.len(), 1);
        assert_eq!(first.anchor.revision, source.revision);
    }
    assert!(
        Search::new("SECRET")
            .unwrap()
            .step(&mut book)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        fs::read(publication.0.join("book.xhtml")).unwrap(),
        original
    );
}

#[test]
fn visibility_hidden_remains_searchable_but_display_none_cannot_be_restored_by_descendants() {
    let publication = Publication::new(
        "<p><span class='unpainted'>metric <b class='restored'>visible</b></span><span class='cut'><b style='display:inline;visibility:visible'>secret</b></span></p>",
    );
    let mut book = publication.book();
    for word in ["metric", "visible"] {
        assert_eq!(Search::new(word).unwrap().step(&mut book).unwrap().len(), 1);
    }
    assert!(
        Search::new("secret")
            .unwrap()
            .step(&mut book)
            .unwrap()
            .is_empty()
    );
    let config = FlowConfig {
        publisher_styles: false,
        ..Default::default()
    };
    assert_eq!(
        Search::new("secret")
            .unwrap()
            .with_flow_config(config)
            .step(&mut book)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn directional_matches_cross_hidden_joins_in_both_directions_and_do_not_leak_context() {
    let publication = Publication::new("<p>Al<span class='cut'>SECRET</span>pha and Alpha.</p>");
    let mut book = publication.book();
    let source = canonical(&mut book);
    let block = &source.blocks[0];
    let origin = Anchor {
        raw_caret: None,
        illustration: None,
        book: book.revision(),
        chapter: book.chapters[0].href.clone(),
        revision: source.revision,
        block: block.key,
        offset: 0,
    };
    let cancel = AtomicBool::new(false);
    let next = NearbySearch::new("alpha", false, false, SearchDirection::Next).unwrap();
    let first = next.find(&mut book, 0, &origin, &cancel).unwrap().unwrap();
    assert_eq!(first.source_spans.len(), 2);
    let second = next
        .find(&mut book, 0, &first.end, &cancel)
        .unwrap()
        .unwrap();
    assert_eq!(second.source_spans.len(), 1);
    let previous = NearbySearch::new("alpha", false, false, SearchDirection::Previous).unwrap();
    let back = previous
        .find(&mut book, 0, &second.anchor, &cancel)
        .unwrap()
        .unwrap();
    assert_eq!(back.anchor, first.anchor);
    assert_eq!(back.end, first.end);
    assert_eq!(back.source_spans, first.source_spans);
    assert!(!back.snippet.contains("SECRET"));
}

#[test]
fn accent_folding_keeps_utf8_spans_across_suppressed_content() {
    let publication = Publication::new(
        "<p>Caf<span class='cut'>STOP</span>é and e<span class='cut'>STOP</span>́.</p>",
    );
    let mut book = publication.book();
    let source = canonical(&mut book);
    let block = &source.blocks[0];
    let hits = Search::with_options("cafe", false, true)
        .unwrap()
        .step(&mut book)
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(&hits[0].snippet[hits[0].matched.clone()], "Café");
    assert_eq!(
        hits[0]
            .source_spans
            .iter()
            .map(|span| &block.text[span.clone()])
            .collect::<String>(),
        "Café"
    );
    let hits = Search::with_options("é", false, true)
        .unwrap()
        .step(&mut book)
        .unwrap();
    assert_eq!(hits.len(), 2);
    assert!(hits.iter().flat_map(|hit| &hit.source_spans).all(|span| {
        block.text.is_char_boundary(span.start) && block.text.is_char_boundary(span.end)
    }));
}

#[test]
fn loaded_chapters_cannot_cross_style_or_publication_request_keys() {
    let publication = Publication::new("<p>Alpha <span class='cut'>secret</span>.</p>");
    let mut book = publication.book();
    let disabled = FlowConfig {
        publisher_styles: false,
        ..Default::default()
    };
    let text = nearby::ChapterText::read_with_flow(&mut book, 0, &disabled).unwrap();
    let mut search = Search::new("Alpha").unwrap();
    assert!(
        search
            .step_loaded(text, 1, &AtomicBool::new(false))
            .unwrap_err()
            .to_string()
            .contains("styles changed")
    );
    assert!(search.is_finished());
    let text = nearby::ChapterText::read(&mut book, 0).unwrap();
    let mut search = Search::new("Alpha")
        .unwrap()
        .for_publication(book.revision() ^ 1, 1);
    assert!(
        search
            .step_loaded(text, 1, &AtomicBool::new(false))
            .unwrap_err()
            .to_string()
            .contains("publication changed")
    );
}

#[test]
#[ignore = "requires READER_FLOW_SEARCH_ORIGINAL pointing to an unchanged authorized reflow book"]
fn original_publication_search_returns_projected_quotes_with_canonical_spans() {
    let path = PathBuf::from(std::env::var_os("READER_FLOW_SEARCH_ORIGINAL").unwrap());
    let before = fs::read(&path).unwrap();
    let mut book = Book::open(&path).unwrap();
    let config = FlowConfig::default();
    let word = (0..book.chapters.len())
        .find_map(|chapter| {
            let text = nearby::ChapterText::read_with_flow(&mut book, chapter, &config).ok()?;
            let source = text.source.as_ref()?;
            source.blocks.iter().find_map(|block| {
                let view = text.flow.view(source, block).ok()?;
                view.text()
                    .split_whitespace()
                    .map(|word| word.trim_matches(|c: char| !c.is_alphanumeric()))
                    .find(|word| {
                        (4..=16).contains(&word.chars().count())
                            && word.chars().all(char::is_alphabetic)
                    })
                    .map(str::to_owned)
            })
        })
        .expect("The original has no bounded searchable word");
    let mut search = Search::with_case(&word, true).unwrap().at_most(16);
    let mut hits = Vec::new();
    while !search.is_finished() {
        hits.extend(search.step(&mut book).unwrap());
    }
    assert!(!hits.is_empty());
    for hit in &hits {
        let text = nearby::ChapterText::read_with_flow(&mut book, hit.chapter, &config).unwrap();
        let source = text.source.as_ref().unwrap();
        assert_eq!(source.revision, hit.anchor.revision);
        let block = source
            .blocks
            .iter()
            .find(|block| block.key == hit.anchor.block)
            .unwrap();
        let view = text.flow.view(source, block).unwrap();
        let quote = hit
            .source_spans
            .iter()
            .map(|span| &block.text[span.clone()])
            .collect::<String>();
        assert_eq!(
            without_soft_hyphens(&quote),
            &hit.snippet[hit.matched.clone()]
        );
        assert!(
            hit.source_spans
                .iter()
                .all(|span| !view.layout_spans(span.clone()).unwrap().is_empty())
        );
    }
    assert_eq!(fs::read(&path).unwrap(), before);
    eprintln!("Original search checked {} projected quotes", hits.len());
}
