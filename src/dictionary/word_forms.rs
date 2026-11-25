//! Bounded lookup candidates; source wording and anchors are never normalized.
mod japanese;
use std::collections::HashMap;

use super::MAX_RESULTS;

#[derive(Default)]
pub(super) struct WordForms {
    pub(super) order: HashMap<String, usize>,
}

impl WordForms {
    pub(super) fn for_word(word: &str) -> Self {
        let normalized = icu_normalizer::ComposingNormalizer::new_nfkc().normalize(word);
        let mut forms = Self::english(&normalized);
        if normalized != word {
            for rank in forms.order.values_mut() {
                *rank += 1;
            }
            forms.order.insert(normalized.into_owned(), 0);
        }
        for (variant, cost) in japanese::candidates(word) {
            forms.order.entry(variant).or_insert(cost);
        }
        forms
    }

    fn english(word: &str) -> Self {
        let mut forms = Self::default();
        if word.len() < 2 || !word.is_ascii() {
            return forms;
        }
        let mut add = |prefix: &str, replacement: &str, uppercase: bool| {
            let mut variant = prefix.to_owned();
            if uppercase {
                variant.push_str(&replacement.to_ascii_uppercase());
            } else {
                variant.push_str(replacement);
            }
            if variant.is_empty() || variant == word {
                return;
            }
            let lower = (uppercase || word.as_bytes()[0].is_ascii_uppercase())
                .then(|| variant.to_ascii_lowercase());
            for spelling in Some(variant).into_iter().chain(lower) {
                let rank = forms.order.len();
                forms.order.entry(spelling).or_insert(rank);
            }
        };
        if let Some((_, uppercase)) = suffix(word, "s").or_else(|| suffix(word, "ed")) {
            add(&word[..word.len() - 1], "", uppercase);
        }
        for ending in ["ly", "ing"] {
            if let Some((stem, uppercase)) = suffix(word, ending) {
                if let Some(single) = undouble(stem) {
                    add(single, "", uppercase);
                }
                add(stem, "", uppercase);
                if ending == "ing" {
                    add(stem, "e", uppercase);
                }
            }
        }
        if let Some((stem, uppercase)) = suffix(word, "es")
            && word.len() > 3
            && ["s", "x", "o", "ch", "sh"].iter().any(|ending| {
                if uppercase {
                    stem.ends_with(&ending.to_ascii_uppercase())
                } else {
                    stem.ends_with(ending)
                }
            })
        {
            add(stem, "", uppercase);
        }
        if let Some((stem, uppercase)) = suffix(word, "ed")
            && word.len() > 3
        {
            if let Some(single) = undouble(stem) {
                add(single, "", uppercase);
            }
            add(stem, "", uppercase);
        }
        for ending in ["ied", "ies"] {
            if let Some((stem, uppercase)) = suffix(word, ending) {
                add(stem, "y", uppercase);
            }
        }
        for ending in ["er", "est"] {
            if let Some((stem, uppercase)) = suffix(word, ending) {
                add(stem, "", uppercase);
            }
        }
        // Adverbs and comparatives can change final y to i, double the
        // preceding consonant, or drop a silent e. These remain fallback
        // candidates: the selected spelling and earlier candidates keep their
        // established priority, and only installed dictionary entries match.
        if let Some((stem, uppercase)) = suffix(word, "ily") {
            add(stem, "y", uppercase);
        }
        for ending in ["ier", "iest"] {
            if let Some((stem, uppercase)) = suffix(word, ending) {
                add(stem, "y", uppercase);
            }
        }
        for ending in ["er", "est"] {
            if let Some((stem, uppercase)) = suffix(word, ending) {
                if let Some(single) = undouble(stem) {
                    add(single, "", uppercase);
                }
                add(stem, "e", uppercase);
            }
        }
        forms
    }

    pub(super) fn spellings(&self) -> impl Iterator<Item = (&str, usize)> {
        self.order.iter().map(|(word, rank)| (word.as_str(), *rank))
    }

    pub(super) fn rank(&self, headword: &str) -> Option<usize> {
        self.order.get(headword).copied()
    }
}

fn suffix<'a>(word: &'a str, ending: &str) -> Option<(&'a str, bool)> {
    let offset = word.len().checked_sub(ending.len())?;
    let (prefix, suffix) = word.split_at(offset);
    if prefix.is_empty() || !suffix.eq_ignore_ascii_case(ending) {
        return None;
    }
    let uppercase = suffix.bytes().all(|c| c.is_ascii_uppercase());
    (uppercase || suffix == ending).then_some((prefix, uppercase))
}

fn undouble(stem: &str) -> Option<&str> {
    let bytes = stem.as_bytes();
    let [.., vowel, a, b] = bytes else {
        return None;
    };
    let vowel_set = b"aeiou";
    (bytes.len() > 3
        && a == b
        && !vowel_set.contains(&b.to_ascii_lowercase())
        && vowel_set.contains(&vowel.to_ascii_lowercase()))
    .then(|| &stem[..stem.len() - 1])
}

/// Keep only the best spelling that a particular source actually contains.
pub(super) struct Ranked<T> {
    pub rank: usize,
    pub values: Vec<T>,
}

impl<T> Default for Ranked<T> {
    fn default() -> Self {
        Self {
            rank: usize::MAX,
            values: Vec::new(),
        }
    }
}

impl<T> Ranked<T> {
    pub(super) fn record(&mut self, rank: usize, value: impl FnOnce() -> T) {
        if rank < self.rank {
            self.rank = rank;
            self.values.clear();
        }
        if rank == self.rank && self.values.len() < MAX_RESULTS {
            self.values.push(value());
        }
    }
}

#[test]
fn inflection_order_prefers_a_dictionary_entry_for_the_closest_word_form() {
    for (word, expected) in [
        ("hoped", vec!["hope", "hop"]),
        ("stopped", vec!["stoppe", "stop", "stopp"]),
        ("running", vec!["run", "runn", "runne"]),
        ("making", vec!["mak", "make"]),
        ("happily", vec!["happi", "happy"]),
        ("boxes", vec!["boxe", "box"]),
        ("carried", vec!["carrie", "carri", "carry"]),
        ("earliest", vec!["earli", "early", "earlie"]),
        ("Dogs", vec!["Dog", "dog"]),
        ("iPods", vec!["iPod"]),
        ("iPODS", vec!["iPOD", "ipod"]),
        ("WALKED", vec!["WALKE", "walke", "WALK", "walk"]),
        ("triEd", vec![]),
        ("older", vec!["old", "olde"]),
    ] {
        let forms = WordForms::for_word(word);
        let mut actual: Vec<_> = forms.order.iter().collect();
        actual.sort_by_key(|(_, r)| **r);
        assert_eq!(
            actual.iter().map(|(s, _)| s.as_str()).collect::<Vec<_>>(),
            expected,
            "{word}"
        );
    }
    assert!(WordForms::for_word("hôtels").order.is_empty());
    assert!(WordForms::for_word("s").order.is_empty());
}

#[test]
fn compatibility_normalization_prefers_the_normalized_entry_and_preserves_ambiguity() {
    let forms = WordForms::for_word("Ｄｏｇｓ");
    assert_eq!(forms.rank("Dogs"), Some(0));
    assert!(forms.rank("Dog").unwrap() > forms.rank("Dogs").unwrap());
    assert_eq!(WordForms::for_word("ｶﾞｸｾｲ").rank("ガクセイ"), Some(0));
    assert_eq!(
        WordForms::for_word("か\u{3099}くせい").rank("がくせい"),
        Some(0)
    );
    let forms = WordForms::for_word("読んだ");
    assert_eq!(forms.rank("読む"), forms.rank("読ぶ"));
    assert!(forms.rank("読む").is_some());
    assert!(WordForms::for_word("讀書").order.is_empty());
}
