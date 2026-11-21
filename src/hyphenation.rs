//! Byte offsets of discretionary breaks. Publication text and its source anchors stay intact.
use crate::{
    Settings,
    book_style::{Hyphens, Style},
    source::Block,
};
use parley::{Language, LineBreak, WordBreak};

include!(concat!(env!("OUT_DIR"), "/english_patterns.rs"));
const MAX_WORD: usize = 64;
const MAX_BREAKS: usize = 4096;

fn us_english(lang: Option<Language>) -> bool {
    lang.is_some_and(|lang| {
        lang.language() == "en"
            && matches!(lang.script(), None | Some("Latn"))
            && matches!(lang.region(), None | Some("US"))
    })
}

fn eligible(style: &Style, language: Option<Language>) -> bool {
    style.hyphens == Hyphens::Auto
        && style.word_break != WordBreak::BreakAll
        && style.line_break != Some(LineBreak::Anywhere)
        && us_english(language)
}

pub(crate) fn opportunities(block: &Block, styles: &[Style], settings: Settings) -> Vec<usize> {
    let mut breaks = Vec::new();
    if !settings.automatic_hyphenation
        || settings.disable_soft_hyphens
        || !block
            .styled_text
            .iter()
            .any(|run| eligible(&styles[run.element], run.language))
    {
        return breaks;
    }
    for range in crate::selection::word_ranges(&block.text) {
        let word = &block.text[range.clone()];
        let first = block
            .styled_text
            .partition_point(|span| span.bytes.end <= range.start);
        if block.styled_text[first..]
            .iter()
            .take_while(|span| span.bytes.start < range.end)
            .all(|span| eligible(&styles[span.element], span.language))
        {
            word_breaks(word, |offset| {
                if breaks.len() < MAX_BREAKS {
                    breaks.push(range.start + offset);
                }
            });
        }
        if breaks.len() == MAX_BREAKS {
            break;
        }
    }
    breaks
}

fn word_breaks(word: &str, mut emit: impl FnMut(usize)) {
    let n = word.len();
    if !(5..=MAX_WORD).contains(&n) || !word.bytes().all(|ch| ch.is_ascii_alphabetic()) {
        return;
    }
    let mut letters = [b'.'; MAX_WORD + 2];
    for (dest, src) in letters[1..=n].iter_mut().zip(word.bytes()) {
        *dest = src.to_ascii_lowercase();
    }
    let mut scores = [0u8; MAX_WORD + 3];
    for start in 0..n + 2 {
        let mut node = 0;
        for &ch in &letters[start..n + 2] {
            let (first, count, _, _) = NODES[node];
            let branches = &EDGES[usize::from(first)..][..usize::from(count)];
            let Ok(next) = branches.binary_search_by_key(&ch, |&(label, _)| label) else {
                break;
            };
            node = usize::from(branches[next].1);
            let (_, _, index, len) = NODES[node];
            for (score, &weight) in scores[start..]
                .iter_mut()
                .zip(&WEIGHTS[usize::from(index)..][..usize::from(len)])
            {
                *score = (*score).max(weight);
            }
        }
    }
    for at in 2..=n - 3 {
        if scores[at + 1] % 2 == 1 {
            emit(at);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{book_style::BookStyle, source::Source};

    #[test]
    fn patterns_minima_and_exceptions_keep_original_offsets() {
        for (word, expected) in [
            ("hyphenation", &[2, 6, 7][..]),
            ("ASSOCIATE", &[2, 4, 6][..]),
            ("reciprocity", &[3, 4, 8][..]),
            ("added", &[][..]),
            ("fitted", &[3][..]),
            ("mugged", &[][..]),
            ("project", &[][..]),
            ("ACADEMIES", &[4, 5][..]),
            ("QWERTY", &[][..]),
        ] {
            let mut actual = Vec::new();
            word_breaks(word, |b| actual.push(b));
            assert_eq!(actual, expected);
        }
        for word in ["short", "documentation", "transformation"] {
            word_breaks(word, |b| assert!(b >= 2 && word.len() - b >= 3));
        }
    }

    #[test]
    fn compiled_rules_and_all_correction_words_match_the_pinned_reference() {
        use std::collections::BTreeSet;

        // Independent digests from CREngine b32a88ff, normalized by the QA verifier.
        let rules = include_str!(concat!(env!("OUT_DIR"), "/english_rules.txt"));
        assert_eq!(rules.lines().count(), 6708);
        assert_eq!(
            format!("{:x}", md5::compute(rules)),
            "bfff372769d4ec4231d27c19f7ef0c99"
        );

        let mut words = BTreeSet::new();
        for source in [
            include_str!("../assets/hyphenation/hyph-en-us.tex"),
            include_str!("../assets/hyphenation/ushyphex.tex"),
        ] {
            let group = source
                .split_once("\\hyphenation{")
                .unwrap()
                .1
                .split_once('}')
                .unwrap()
                .0;
            words.extend(
                group
                    .split_whitespace()
                    .map(|entry| entry.replace('-', "").to_ascii_lowercase()),
            );
        }
        assert_eq!(words.len(), 1766);
        let mut matrix = String::new();
        for word in words {
            let mut offsets = Vec::new();
            word_breaks(&word, |n| offsets.push(n.to_string()));
            matrix.push_str(&format!("{word} {}\n", offsets.join(",")));
        }
        assert_eq!(
            format!("{:x}", md5::compute(matrix)),
            "55189508e7be87958f02af1cc6457f5e"
        );
    }

    #[test]
    fn bounds_untrusted_words_and_paragraphs() {
        for word in ["", "a", "éclair", &"hyphenation".repeat(6)] {
            word_breaks(word, |_| panic!("Unexpected opportunity"));
        }
        let text = "hyphenation ".repeat(3000);
        let source = Source::parse(&format!(
            "<html lang='en'><body><p>{text}</p></body></html>"
        ))
        .unwrap();
        let styles = vec![Style::default(); source.elements.len()];
        let breaks = opportunities(&source.blocks[0], &styles, Settings::default());
        assert_eq!(breaks.len(), MAX_BREAKS);
        assert!(breaks.windows(2).all(|p| p[0] < p[1]));
    }

    #[test]
    #[cfg(feature = "kindle")]
    fn saved_books_keep_their_old_disabled_setting_and_new_mode() {
        use crate::{HyphenationMode, library::BookPreferences};
        let settings = Settings::default();
        let read = |json| {
            serde_json::from_str::<BookPreferences>(json)
                .unwrap()
                .apply_to(settings)
                .hyphenation_mode()
        };
        assert_eq!(read("{}"), HyphenationMode::Automatic);
        assert_eq!(
            read(r#"{"disable_soft_hyphens":true}"#),
            HyphenationMode::Off
        );
        let preferences = BookPreferences {
            automatic_hyphenation: Some(false),
            ..BookPreferences::default()
        };
        assert_eq!(
            read(&serde_json::to_string(&preferences).unwrap()),
            HyphenationMode::Authored
        );
    }

    #[test]
    fn package_language_inline_overrides_css_and_explicit_points_limit_automatic_breaks() {
        let xhtml = "<html><body><p>hyphenation</p><p lang='en-GB'>hyphenation</p><p lang=''>hyphenation</p><p lang='vi'>hyphenation</p><p style='hyphens: manual'>hyphenation</p><p>hy<span style='hyphens: none'>phen</span>ation</p><p lang='en-US'>hy&#173;phenation</p><p lang='en-US'>hy<span>phen</span>ation</p><p lang='en-Latn-US'>hyphenation</p><p lang='en'>internationalé</p></body></html>";
        let src = Source::parse_with_language(xhtml, Some(Language::parse("en").unwrap())).unwrap();
        let mut book =
            crate::epub::Book::open(&crate::test_fixture("field-notes-epub3.epub")).unwrap();
        let chapter = book.chapters[0].href.clone();
        let mut fonts =
            crate::font_context(&[crate::test_fixture("fonts/NotoSans-Regular.ttf")]).unwrap();
        let styles = BookStyle::load(&mut book, &chapter, &src.stylesheets, &mut fonts)
            .compute(&src.elements);
        let active = Settings::default();
        for (i, block) in src.blocks.iter().enumerate() {
            let actual = opportunities(block, &styles, active);
            if [0, 7, 8].contains(&i) {
                assert_eq!(actual, [2, 6, 7], "block {i}");
            } else {
                assert!(actual.is_empty(), "block {i}: {actual:?}");
            }
            assert!(
                opportunities(
                    block,
                    &styles,
                    Settings {
                        automatic_hyphenation: false,
                        ..active
                    }
                )
                .is_empty()
            );
            assert!(
                opportunities(
                    block,
                    &styles,
                    Settings {
                        disable_soft_hyphens: true,
                        ..active
                    }
                )
                .is_empty()
            );
        }
        let unknown = Source::parse("<html><body><p>hyphenation</p></body></html>").unwrap();
        assert!(
            opportunities(
                &unknown.blocks[0],
                &vec![Style::default(); unknown.elements.len()],
                active
            )
            .is_empty()
        );
    }
}
