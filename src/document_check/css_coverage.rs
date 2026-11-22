//! Independent presentation accounting for the full-source verifier.
//!
//! This never claims that geometry or pixels succeeded. It partitions *all*
//! canonical source characters/graphics by CSS intent, so omitted content can
//! be reported explicitly without forgiving eligible content that lost layout.
#![allow(dead_code)] // Wired into the spine verifier with the Probe consumer migration.

use crate::{Result, flow_view::ChapterFlow, source::Source};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Intent {
    pub source_chars: usize,
    pub normal_chars: usize,
    pub visibility_hidden_chars: usize,
    pub css_omitted_chars: usize,
    pub source_graphics: usize,
    pub visibility_hidden_graphics: usize,
    pub css_omitted_graphics: usize,
}

pub(super) fn classify(source: &Source, flow: &ChapterFlow) -> Result<Intent> {
    let mut counts = Intent::default();
    for block in &source.blocks {
        // Bind/hash once per block, not once for each character of a long book.
        let view = flow
            .view(source, block)
            .map_err(|error| error.to_string())?;
        match view.runs() {
            None => {
                let length = block.text.chars().filter(|c| !c.is_whitespace()).count();
                counts.source_chars += length;
                counts.normal_chars += length;
            }
            Some(runs) => {
                // Runs preserve canonical source order. Empty source intervals
                // are re-collapsed spaces, never additional source characters.
                let mut runs = runs.iter().peekable();
                for (offset, _) in block
                    .text
                    .char_indices()
                    .filter(|(_, c)| !c.is_whitespace())
                {
                    counts.source_chars += 1;
                    while runs.peek().is_some_and(|run| run.source.end <= offset) {
                        runs.next();
                    }
                    match runs.peek().filter(|run| run.source.contains(&offset)) {
                        Some(run) if run.painted => counts.normal_chars += 1,
                        Some(_) => counts.visibility_hidden_chars += 1,
                        None => counts.css_omitted_chars += 1,
                    }
                }
            }
        }
        for image in &block.images {
            counts.source_graphics += 1;
            let intent = flow
                .element(image.element)
                .map_err(|error| error.to_string())?;
            if intent.suppressed {
                counts.css_omitted_graphics += 1;
            } else if !intent.painted {
                counts.visibility_hidden_graphics += 1;
            }
        }
    }
    Ok(counts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{book_style::BookStyle, epub::Book, user_styles::Snapshot};

    fn inspect(body: &str) -> Intent {
        let source = Source::parse(&format!("<html><body>{body}</body></html>")).unwrap();
        let mut book = Book::open(&crate::test_fixture("field-notes-epub3.epub")).unwrap();
        let href = book.chapters[0].href.clone();
        let styles =
            BookStyle::load_for_flow(&mut book, &href, &source, true, &Snapshot::default())
                .unwrap();
        let flow = ChapterFlow::new(
            &source,
            styles
                .compute(&source.elements)
                .into_iter()
                .map(|style| style.flow)
                .collect(),
        )
        .unwrap();
        let counts = classify(&source, &flow).unwrap();
        assert_eq!(
            counts.source_chars,
            counts.normal_chars + counts.visibility_hidden_chars + counts.css_omitted_chars
        );
        counts
    }

    #[test]
    fn default_intent_counts_canonical_scalars_without_asserting_layout_or_decoding() {
        let counts = inspect("<p>A 日本 B<img src='unavailable.png'/></p>");
        assert_eq!(counts.source_chars, 4);
        assert_eq!(counts.normal_chars, 4);
        assert_eq!(counts.source_graphics, 1);
        assert_eq!(counts.visibility_hidden_chars, 0);
        assert_eq!(counts.css_omitted_chars, 0);
    }

    #[test]
    fn hidden_geometry_and_omitted_source_are_distinct_and_visible_children_restore_only_paint() {
        let counts = inspect(
            "<p>A<span style='display:none'>secret</span><span style='visibility:hidden'>hide<b style='visibility:visible'>V</b></span>Z</p>",
        );
        assert_eq!(counts.source_chars, 13);
        assert_eq!(counts.normal_chars, 3);
        assert_eq!(counts.visibility_hidden_chars, 4);
        assert_eq!(counts.css_omitted_chars, 6);
    }

    #[test]
    fn utf8_multibyte_scalars_do_not_inflate_css_omission_counts() {
        let counts = inspect("<p>日<span style='display:none'>é𝄞</span>本</p>");
        assert_eq!(counts.source_chars, 4);
        assert_eq!(counts.normal_chars, 2);
        assert_eq!(counts.css_omitted_chars, 2);
    }

    #[test]
    fn fully_omitted_chapters_remain_reported_as_source_without_a_mapping_claim() {
        let counts = inspect("<p style='display:none'>Alpha<img src='unavailable.png'/></p>");
        assert_eq!(counts.source_chars, 5);
        assert_eq!(counts.normal_chars, 0);
        assert_eq!(counts.css_omitted_chars, 5);
        assert_eq!(counts.source_graphics, 1);
        assert_eq!(counts.css_omitted_graphics, 1);
    }

    #[test]
    fn visibility_hidden_images_retain_geometry_and_are_not_counted_as_css_omissions() {
        let counts = inspect("<p style='visibility:hidden'>Alpha<img src='unavailable.png'/></p>");
        assert_eq!(counts.source_chars, 5);
        assert_eq!(counts.visibility_hidden_chars, 5);
        assert_eq!(counts.css_omitted_chars, 0);
        assert_eq!(counts.source_graphics, 1);
        assert_eq!(counts.visibility_hidden_graphics, 1);
        assert_eq!(counts.css_omitted_graphics, 0);
    }
}
