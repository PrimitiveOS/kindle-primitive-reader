//! Dictionary-free break positions for eligible horizontal alphabetic words.
use super::{HyphenationLimits, MAX_WORD};
use icu_normalizer::DecomposingNormalizer;
use icu_segmenter::GraphemeClusterSegmenter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Letter {
    Vowel,
    Consonant,
    Sign,
}

fn classify(grapheme: &str) -> Option<(char, Letter)> {
    let mut text = grapheme.chars();
    let canonical = text.next()?.to_lowercase().next()?;
    let base = if canonical.is_ascii() {
        canonical
    } else {
        DecomposingNormalizer::new_nfd()
            .normalize(grapheme)
            .chars()
            .next()?
            .to_lowercase()
            .next()?
    };
    let kind = match (canonical, base) {
        ('ъ' | 'ь', _) => Letter::Sign,
        ('а' | 'е' | 'ё' | 'и' | 'о' | 'у' | 'ы' | 'э' | 'ю' | 'я' | 'і' | 'ї' | 'є', _)
        | (_, 'a' | 'e' | 'i' | 'o' | 'u' | 'y') => Letter::Vowel,
        ('б'..='я' | 'ґ' | 'ў' | 'æ' | 'œ' | 'ø' | 'ß' | 'ł' | 'ð' | 'þ', _) | (_, 'a'..='z') => {
            Letter::Consonant
        }
        _ => return None,
    };
    Some((canonical, kind))
}

pub(super) fn breaks(word: &str, limits: HyphenationLimits, mut emit: impl FnMut(usize)) {
    if !limits.valid() || word.len() > MAX_WORD * 5 {
        return;
    }
    let mut start = 0;
    let units: Option<Vec<_>> = GraphemeClusterSegmenter::new()
        .segment_str(word)
        .skip(1)
        .filter_map(|end| {
            let grapheme = &word[start..end];
            start = end;
            (grapheme != "\u{ad}").then_some((end, grapheme))
        })
        .map(|(end, grapheme)| {
            let (letter, kind) = classify(grapheme)?;
            Some((end, letter, kind))
        })
        .collect();
    let Some(units) = units.filter(|units| units.len() <= MAX_WORD) else {
        return;
    };
    let left = limits.left as usize;
    let right = limits.right as usize;
    let Some(last_vowel) = units.iter().rposition(|unit| unit.2 == Letter::Vowel) else {
        return;
    };
    let last = units.len().saturating_sub(right).min(last_vowel);
    for vowel in 0..last {
        if units[vowel].2 != Letter::Vowel {
            continue;
        }
        let boundary = match units.get(vowel + 1..vowel + 3) {
            Some([(_, _, Letter::Consonant), (_, _, Letter::Consonant)]) => vowel + 2,
            Some([(_, _, Letter::Consonant), (_, _, Letter::Sign)]) => vowel + 3,
            _ => vowel + 1,
        };
        if !(left..=last).contains(&boundary) {
            continue;
        }
        let (offset, before, _) = units[boundary - 1];
        let (_, after, _) = units[boundary];
        if matches!((before, after), ('s' | 't' | 'p' | 'c', 'h'))
            || word[offset..].starts_with('\u{ad}')
        {
            continue;
        }
        emit(offset);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hyphenation::ALGORITHMIC_LIMITS;

    #[test]
    fn algorithmic_hyphenation_uses_letter_counts_and_original_utf8_offsets() {
        let split = |word: &str, limits| {
            let mut out = Vec::new();
            breaks(word, limits, |at| out.push(word[..at].chars().count()));
            out
        };
        assert_eq!(split("laboratory", ALGORITHMIC_LIMITS), [2, 4, 6, 8]);
        assert_eq!(split("фотография", ALGORITHMIC_LIMITS), [2, 5, 7]);
        assert_eq!(
            split("laboratory", HyphenationLimits { left: 5, right: 4 }),
            [6]
        );
        assert!(split("strength", ALGORITHMIC_LIMITS).is_empty());
        assert!(split("中文", ALGORITHMIC_LIMITS).is_empty());
    }
}
