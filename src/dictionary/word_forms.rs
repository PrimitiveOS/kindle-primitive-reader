//! Ordered English spelling variants used when a dictionary has no direct entry.
use std::collections::HashMap;

use super::MAX_RESULTS;

#[derive(Default)]
pub(super) struct WordForms {
    order: HashMap<String, usize>,
}

impl WordForms {
    pub(super) fn for_word(word: &str) -> Self {
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
        forms
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
        ("happily", vec!["happi"]),
        ("boxes", vec!["boxe", "box"]),
        ("carried", vec!["carrie", "carri", "carry"]),
        ("earliest", vec!["earli"]),
        ("Dogs", vec!["Dog", "dog"]),
        ("iPods", vec!["iPod"]),
        ("iPODS", vec!["iPOD", "ipod"]),
        ("WALKED", vec!["WALKE", "walke", "WALK", "walk"]),
        ("triEd", vec![]),
        ("older", vec!["old"]),
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
