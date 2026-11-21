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

struct Patterns {
    nodes: &'static [(u16, u8, u16, u8)],
    edges: &'static [(u8, u16)],
    weights: &'static [u8],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum English {
    American,
    British,
}

impl English {
    fn patterns(self) -> &'static Patterns {
        match self {
            Self::American => &AMERICAN,
            Self::British => &BRITISH,
        }
    }
}

fn dictionary(style: &Style, lang: Option<Language>) -> Option<English> {
    let lang = lang?;
    if style.hyphens != Hyphens::Auto
        || style.word_break == WordBreak::BreakAll
        || style.line_break == Some(LineBreak::Anywhere)
        || lang.language() != "en"
        || !matches!(lang.script(), None | Some("Latn"))
    {
        return None;
    }
    match lang.region() {
        None | Some("US") => Some(English::American),
        Some("GB") => Some(English::British),
        _ => None,
    }
}

pub(crate) fn opportunities(block: &Block, styles: &[Style], settings: Settings) -> Vec<usize> {
    let mut breaks = Vec::new();
    if !settings.automatic_hyphenation
        || settings.disable_soft_hyphens
        || !block
            .styled_text
            .iter()
            .any(|run| dictionary(&styles[run.element], run.language).is_some())
    {
        return breaks;
    }
    for range in crate::selection::word_ranges(&block.text) {
        let word = &block.text[range.clone()];
        let first = block
            .styled_text
            .partition_point(|span| span.bytes.end <= range.start);
        let mut spans = block.styled_text[first..]
            .iter()
            .take_while(|span| span.bytes.start < range.end);
        let Some(locale) = spans
            .next()
            .and_then(|span| dictionary(&styles[span.element], span.language))
        else {
            continue;
        };
        if spans.all(|span| dictionary(&styles[span.element], span.language) == Some(locale)) {
            word_breaks(locale, word, |offset| {
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

fn word_breaks(locale: English, word: &str, mut emit: impl FnMut(usize)) {
    let patterns = locale.patterns();
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
            let (first, count, _, _) = patterns.nodes[node];
            let branches = &patterns.edges[usize::from(first)..][..usize::from(count)];
            let Ok(next) = branches.binary_search_by_key(&ch, |&(label, _)| label) else {
                break;
            };
            node = usize::from(branches[next].1);
            let (_, _, index, len) = patterns.nodes[node];
            for (score, &weight) in scores[start..]
                .iter_mut()
                .zip(&patterns.weights[usize::from(index)..][..usize::from(len)])
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
            word_breaks(English::American, word, |b| actual.push(b));
            assert_eq!(actual, expected);
        }
        for word in ["short", "documentation", "transformation"] {
            word_breaks(English::American, word, |b| {
                assert!(b >= 2 && word.len() - b >= 3)
            });
        }
    }

    #[test]
    fn compiled_rules_and_all_correction_words_match_the_pinned_reference() {
        use std::collections::BTreeSet;

        // Independent digests from CREngine b32a88ff, normalized by the QA verifier.
        for (locale, rules, count, digest, sources, corrections, expected_matrix) in [
            (
                English::American,
                include_str!(concat!(env!("OUT_DIR"), "/english_rules.txt")),
                6708,
                "bfff372769d4ec4231d27c19f7ef0c99",
                &[
                    include_str!("../assets/hyphenation/hyph-en-us.tex"),
                    include_str!("../assets/hyphenation/ushyphex.tex"),
                ][..],
                1766,
                "55189508e7be87958f02af1cc6457f5e",
            ),
            (
                English::British,
                include_str!(concat!(env!("OUT_DIR"), "/british_english_rules.txt")),
                8535,
                "56aaf1a95bc02a467aaaec694459f770",
                &[include_str!("../assets/hyphenation/hyph-en-gb.tex")][..],
                8,
                "beed0132614b7c565ac305f6f9300b47",
            ),
        ] {
            assert_eq!(rules.lines().count(), count);
            assert_eq!(format!("{:x}", md5::compute(rules)), digest);
            let mut words = BTreeSet::new();
            for source in sources {
                let group = source
                    .split_once("\\hyphenation{")
                    .unwrap()
                    .1
                    .split_once('}')
                    .unwrap()
                    .0;
                words.extend(
                    group
                        .lines()
                        .flat_map(|line| line.split('%').next().unwrap().split_whitespace())
                        .map(|entry| entry.replace('-', "").to_ascii_lowercase()),
                );
            }
            assert_eq!(words.len(), corrections);
            let mut matrix = String::new();
            for word in words {
                let mut offsets = Vec::new();
                word_breaks(locale, &word, |n| offsets.push(n.to_string()));
                matrix.push_str(&format!("{word} {}\n", offsets.join(",")));
            }
            assert_eq!(format!("{:x}", md5::compute(matrix)), expected_matrix);
        }
    }

    #[test]
    fn mixed_english_regions_follow_whole_word_language_and_keep_source_offsets() {
        let xhtml = r#"<html><body><p>laboratory <span lang="en-GB">lab<b>ora</b>tory</span> <span xml:lang="en-Latn-GB">laboratory</span> <span lang="vi">laboratory</span> <span lang="en-AU">laboratory</span> <span lang="en-GB">labo<span lang="en-US">ratory</span></span></p></body></html>"#;
        let src = Source::parse_with_language(xhtml, Some(Language::parse("en").unwrap())).unwrap();
        let block = &src.blocks[0];
        let text = &block.text;
        let starts: Vec<_> = text
            .match_indices("laboratory")
            .map(|(offset, _)| offset)
            .collect();
        assert_eq!(starts.len(), 6);
        let expected: Vec<_> = starts
            .into_iter()
            .zip([&[3, 4, 6][..], &[5, 7], &[5, 7], &[], &[], &[]])
            .flat_map(|(start, offsets)| offsets.iter().map(move |end| start + end))
            .collect();
        let actual = opportunities(
            block,
            &vec![Style::default(); src.elements.len()],
            Settings::default(),
        );
        assert_eq!(actual, expected);
    }

    #[test]
    fn bounds_untrusted_words_and_paragraphs() {
        for word in ["", "a", "éclair", &"hyphenation".repeat(6)] {
            word_breaks(English::American, word, |_| {
                panic!("Unexpected opportunity")
            });
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
            if [0, 1, 7, 8].contains(&i) {
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
