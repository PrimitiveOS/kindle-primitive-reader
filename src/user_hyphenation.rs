//! Personal discretionary break positions. Rules contain grapheme counts, never source byte offsets.
use crate::Result;
use icu_segmenter::GraphemeClusterSegmenter;
use parley::Language;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, ops::Range};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum OptionalPatterns {
    #[default]
    Missing,
    Invalid,
    Loaded(std::sync::Arc<crate::hyphenation::user_patterns::OwnedPatterns>),
}

const MAX_WORD_CHARS: usize = 96;
// Allow a four-byte character and a source soft hyphen at every position.
const MAX_SOURCE_WORD_BYTES: usize = 6 * MAX_WORD_CHARS;
const MAX_ENTRIES: usize = 4096;
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Rules {
    pub(crate) enabled: bool,
    #[serde(skip)]
    pub(crate) armenian: OptionalPatterns,
    #[serde(skip)]
    pub(crate) zulu: OptionalPatterns,
    #[serde(skip)]
    pub(crate) serbian_latin: OptionalPatterns,
    #[serde(skip)]
    pub(crate) serbian_cyrillic: OptionalPatterns,
    #[serde(skip)]
    pub(crate) czech: OptionalPatterns,
    words: BTreeMap<String, BTreeMap<String, Vec<u16>>>,
}
pub(crate) fn locale(tag: &Language) -> &str {
    match tag.as_str() {
        "en" | "eng" | "en-Latn" | "en-Latn-US" => "en-US",
        "en-Latn-GB" => "en-GB",
        other => other,
    }
}

fn letters(word: &str) -> Option<String> {
    if word.len() > MAX_SOURCE_WORD_BYTES {
        return None;
    }
    let text = word.replace('\u{ad}', "");
    if text.chars().count() > MAX_WORD_CHARS
        || GraphemeClusterSegmenter::new().segment_str(&text).count() < 5
        || crate::selection::word_ranges(&text).next() != Some(0..text.len())
        || !text.chars().any(char::is_alphabetic)
        || text
            .chars()
            .any(|c| c.is_whitespace() || matches!(c, '-' | ';' | ',' | '.'))
    {
        return None;
    }
    Some(text)
}
pub(crate) fn eligible(word: &str) -> bool {
    letters(word).is_some()
}
fn boundaries(word: &str) -> Vec<usize> {
    GraphemeClusterSegmenter::new().segment_str(word).collect()
}

impl Rules {
    pub(crate) fn has_rules(&self) -> bool {
        self.enabled && !self.words.is_empty()
    }
    pub(crate) fn mask(&self, language: &str, word: &str) -> Option<Vec<usize>> {
        let entries = self.words.get(language)?;
        if word.len() > MAX_SOURCE_WORD_BYTES {
            return None;
        }
        let word = if word.contains('\u{ad}') {
            std::borrow::Cow::Owned(word.replace('\u{ad}', ""))
        } else {
            std::borrow::Cow::Borrowed(word)
        };
        let indexes = entries.get(&word.to_lowercase())?;
        let boundaries = boundaries(&word);
        indexes
            .iter()
            .map(|&i| boundaries.get(usize::from(i)).copied())
            .collect()
    }
    pub(crate) fn contains(&self, language: &str, word: &str) -> bool {
        self.mask(language, word).is_some()
    }
    pub(crate) fn set(&mut self, language: &str, word: &str, entered: &str) -> Result<()> {
        if language.len() > 64 || Language::parse(language).is_err() {
            return Err("The word’s language tag is unavailable".into());
        }
        let word = letters(word).ok_or("Select one word of at least four letters")?;
        let suggestion = entered.trim().replace(' ', "-");
        let suggestion = suggestion.strip_prefix('-').unwrap_or(&suggestion);
        let suggestion = suggestion.strip_suffix('-').unwrap_or(suggestion);
        if suggestion.contains("--")
            || suggestion.replace('-', "").to_lowercase() != word.to_lowercase()
        {
            return Err("Keep the letters of the selected word and change only its breaks".into());
        }
        let plain = suggestion.replace('-', "");
        let stops = boundaries(&plain);
        let mut offset = 0;
        let mut mask = Vec::new();
        for part in suggestion.split('-') {
            if offset > 0 {
                let at = stops
                    .binary_search(&offset)
                    .map_err(|_| "A break cannot divide one written character")?;
                mask.push(at as u16);
            }
            offset += part.len();
        }
        let graphemes = boundaries(&word).len();
        if stops.len() != graphemes
            || mask
                .iter()
                .any(|&i| i == 0 || usize::from(i) >= graphemes - 1)
        {
            return Err("Choose a break between complete letters".into());
        }
        if !self.contains(language, &word)
            && self.words.values().map(BTreeMap::len).sum::<usize>() >= MAX_ENTRIES
        {
            return Err("Custom hyphenation rules are full".into());
        }
        self.words
            .entry(language.to_owned())
            .or_default()
            .insert(word.to_lowercase(), mask);
        Ok(())
    }
    pub(crate) fn remove(&mut self, language: &str, word: &str) {
        if let Some(table) = self.words.get_mut(language) {
            table.remove(&word.replace('\u{ad}', "").to_lowercase());
            if table.is_empty() {
                self.words.remove(language);
            }
        }
    }
    pub(crate) fn valid(&self) -> bool {
        self.words.len() <= MAX_ENTRIES
            && self.words.values().map(BTreeMap::len).sum::<usize>() <= MAX_ENTRIES
            && self.words.iter().all(|(lang, entries)| {
                lang.len() <= 64
                    && Language::parse(lang).is_ok()
                    && entries.iter().all(|(word, positions)| {
                        let graphemes = boundaries(word).len().saturating_sub(1);
                        eligible(word)
                            && *word == word.to_lowercase()
                            && positions.windows(2).all(|p| p[0] < p[1])
                            && positions
                                .iter()
                                .all(|&p| p > 0 && usize::from(p) < graphemes)
                    })
            })
    }
}

/// Byte positions refer to the original string even when a soft hyphen was present in the file.
pub(crate) fn source_offsets(word: &str, positions: &[usize]) -> Vec<usize> {
    let mut plain = 0;
    let mut next = positions.iter().copied().peekable();
    let mut result = Vec::with_capacity(positions.len());
    for (at, c) in word.char_indices() {
        if c == '\u{ad}' {
            continue;
        }
        if next.peek() == Some(&plain) {
            result.push(at);
            next.next();
        }
        plain += c.len_utf8();
    }
    result
}
pub(crate) fn marked(word: &str, positions: &[usize]) -> String {
    let mut marked = String::new();
    for (at, c) in word.char_indices() {
        if positions.binary_search(&at).is_ok() {
            marked.push('-');
        }
        if c != '\u{ad}' {
            marked.push(c);
        }
    }
    marked
}
#[derive(Default)]
pub(crate) struct Opportunities {
    pub(crate) breaks: Vec<usize>,
    pub(crate) custom_words: Vec<Range<usize>>,
    pub(crate) matching_soft_hyphens: Vec<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn region_masks_use_graphemes_and_never_change_the_stored_spelling() {
        let mut rules = Rules::default();
        let us = Language::parse("en-Latn").unwrap();
        let gb = Language::parse("en-Latn-GB").unwrap();
        rules
            .set(locale(&us), "Laboratory", " Labo ratory ")
            .unwrap();
        assert_eq!(rules.mask("en-US", "LABORATORY").unwrap(), vec![4]);
        assert_eq!(rules.mask(locale(&gb), "laboratory"), None);
        rules.set(locale(&gb), "laboratory", "laboratory").unwrap();
        assert_eq!(rules.mask("en-GB", "Laboratory"), Some(vec![]));
        let word = "e\u{301}lantern";
        rules.set("fr", word, "e\u{301}lan-tern").unwrap();
        assert_eq!(rules.mask("fr", word), Some(vec!["e\u{301}lan".len()]));
        assert!(rules.set("fr", word, "e-\u{301}lantern").is_err());
        assert!(rules.set("fr", "Élantern", "E\u{301}lan-tern").is_err());
        for bad in ["labo--ratory", "labor-atoryy", "different"] {
            assert!(rules.set("en-US", "laboratory", bad).is_err());
        }
        assert!(rules.valid());
        assert_eq!(source_offsets("labo\u{ad}ratory", &[4]), vec![6]);
        assert_eq!(marked("labo\u{ad}ratory", &[6]), "labo-ratory");
        rules.remove("en-US", "LABORATORY");
        assert_eq!(rules.mask("en-US", "laboratory"), None);
        assert_eq!(rules.mask("en-GB", "laboratory"), Some(vec![]));
        assert!(!eligible("cat"));
        assert!(!eligible(&"a".repeat(MAX_SOURCE_WORD_BYTES + 1)));
        assert!(!eligible("two words"));
    }

    #[test]
    fn selected_word_replaces_automatic_breaks_only_in_the_intended_language_and_css() {
        use crate::{book_style::BookStyle, source::Source};
        let source=Source::parse(r#"<html lang="en"><head><style>.n {hyphens:none} .m {hyphens:manual}</style></head><body>
            <p>laboratory</p><p lang="en-GB">laboratory</p><p class="m">labo&#173;ratory</p>
            <p class="n">laboratory</p><p>labo&#173;ratory</p><p>labo<span lang="en-GB">ratory</span></p>
            </body></html>"#).unwrap();
        let mut book =
            crate::epub::Book::open(&crate::test_fixture("field-notes-epub3.epub")).unwrap();
        let href = book.chapters[0].href.clone();
        let mut fonts =
            crate::font_context(&[crate::test_fixture("fonts/NotoSans-Regular.ttf")]).unwrap();
        let styles =
            BookStyle::load(&mut book, &href, &source, &mut fonts).compute(&source.elements);
        let settings = crate::Settings {
            automatic_hyphenation: true,
            disable_soft_hyphens: false,
            ..Default::default()
        };
        let mut rules = Rules::default();
        rules.enabled = true;
        rules.set("en-US", "laboratory", "la-boratory").unwrap();
        assert_eq!(
            crate::hyphenation::suggestion(&Rules::default(), "en-US", "laboratory"),
            "lab-o-ra-tory"
        );
        assert_eq!(
            crate::hyphenation::suggestion(&Rules::default(), "en-GB", "laboratory"),
            "labor-at-ory"
        );
        let collect = |n: usize, settings, rules: &Rules| {
            crate::hyphenation::with_user_rules(&source.blocks[n], &styles, settings, rules)
        };
        assert_eq!(collect(0, settings, &rules).breaks, vec![2]);
        assert_eq!(collect(1, settings, &rules).breaks, vec![5, 7]);
        for n in [2, 3, 5] {
            let got = collect(n, settings, &rules);
            assert!(got.breaks.is_empty() && got.custom_words.is_empty());
        }
        let soft = collect(4, settings, &rules);
        assert_eq!(soft.breaks, vec![2]);
        assert_eq!(soft.custom_words, vec![0.."labo\u{ad}ratory".len()]);
        let soft_only = crate::Settings {
            automatic_hyphenation: false,
            ..settings
        };
        assert_eq!(collect(0, soft_only, &rules).breaks, [2]);
        assert!(
            collect(
                0,
                crate::Settings {
                    disable_soft_hyphens: true,
                    ..soft_only
                },
                &rules,
            )
            .custom_words
            .is_empty()
        );
        rules.set("en-US", "laboratory", "laboratory").unwrap();
        assert!(collect(0, settings, &rules).breaks.is_empty());
        rules.remove("en-US", "laboratory");
        assert_eq!(collect(0, settings, &rules).breaks, vec![3, 4, 6]);
    }
}

#[cfg(test)]
mod layout_tests {
    use super::*;
    use crate::{Engine, Probe, Settings, epub::Book};

    #[test]
    fn custom_rule_matches_an_independent_publisher_break_and_preserves_reading_anchors() {
        let original =
            "<html lang='en-US'><body><p>Look for hyphenation in this paragraph.</p></body></html>";
        let manual = original.replace("hyphenation", "hyph&#173;enation");
        let conflicting = original.replace("hyphenation", "hy&#173;phenation");
        let opener = |source: &str, rules: &Rules, automatic: bool| {
            Probe::from_book_with_rules(
                Book::open(&crate::test_fixture("english-hyphenation.epub")).unwrap(),
                Engine::Parley,
                &[crate::test_fixture("fonts/NotoSans-Regular.ttf")],
                Settings {
                    width: 210,
                    height: 480,
                    margin: 15,
                    font_size: 26.,
                    automatic_hyphenation: automatic,
                    base_stylesheet: crate::BaseStylesheet::None,
                    ..Default::default()
                },
                0,
                Some(source.into()),
                rules,
                &crate::user_styles::Snapshot::default(),
            )
            .unwrap()
        };
        let plain = Rules::default();
        let mut user = Rules {
            enabled: true,
            ..Default::default()
        };
        user.set("en-US", "hyphenation", "hyph-enation").unwrap();
        let mut expected = opener(&manual, &plain, false);
        let mut got = opener(original, &user, true);
        let mut conflict = opener(&conflicting, &user, true);
        let mut coincident = opener(&manual, &user, true);
        let mut baseline = opener(original, &plain, true);
        let anchor = got.matches("hyphenation").remove(0);
        let end = crate::Anchor {
            offset: anchor.offset + "hyphenation".len(),
            ..anchor.clone()
        };
        let offsets = |p: &Probe| {
            p.lines
                .iter()
                .map(|line| line.bytes.end)
                .collect::<Vec<_>>()
        };
        assert_eq!(offsets(&got)[0], anchor.offset + 4);
        assert_eq!(
            offsets(&expected)[0],
            anchor.offset + 4 + '\u{ad}'.len_utf8()
        );
        assert_eq!(
            offsets(&conflict)[0],
            anchor.offset + 4 + '\u{ad}'.len_utf8()
        );
        assert_ne!(offsets(&got), offsets(&baseline));
        let pixels = expected.capture(0).unwrap();
        assert!(
            got.capture(0).unwrap() == pixels,
            "The drawn hyphen should match a publisher-authored discretionary break"
        );
        assert!(
            coincident.capture(0).unwrap() == pixels,
            "Matching publisher and custom points draw once"
        );
        assert!(
            conflict.capture(0).unwrap() == pixels,
            "A personal rule supersedes a different publisher soft hyphen"
        );
        assert_eq!(baseline.matches("hyphenation")[0], anchor);
        for probe in [&baseline, &got] {
            assert_eq!(
                probe.select_range(&anchor, &end).unwrap().text,
                "hyphenation"
            );
            assert_eq!(probe.page_for(&anchor), Some(0));
        }
        user.remove("en-US", "hyphenation");
        assert!(opener(original, &user, true).capture(0).unwrap() == baseline.capture(0).unwrap());
    }
}
