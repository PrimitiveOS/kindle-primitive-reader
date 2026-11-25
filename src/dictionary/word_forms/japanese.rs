//! Independently authored, dictionary-validated Japanese morphology.
//!
//! These are productive conjugation facts, not a port of KOReader/Yomichan's
//! licensed rule tables. Ambiguous bases have equal cost. A real installed
//! dictionary, never this module, decides which candidates have definitions.
use std::collections::{HashSet, VecDeque};

pub(super) const MAX_CANDIDATES: usize = 128;
const MAX_DEPTH: usize = 4;

pub(super) fn candidates(word: &str) -> Vec<(String, usize)> {
    let normalized = icu_normalizer::ComposingNormalizer::new_nfkc().normalize(word);
    if !normalized
        .chars()
        .any(|c| matches!(c, '\u{3041}'..='\u{3096}'))
    {
        return Vec::new();
    }
    let mut seen = HashSet::from([normalized.to_string()]);
    let mut queue = VecDeque::from([(normalized.into_owned(), 0)]);
    let mut result = Vec::new();
    while let Some((text, depth)) = queue.pop_front() {
        if depth == MAX_DEPTH {
            continue;
        }
        for base in bases(&text) {
            if base.len() > super::super::MAX_WORD_BYTES || !seen.insert(base.clone()) {
                continue;
            }
            // A valid auxiliary form and its final verb must both remain
            // visible when a dictionary indexes both (e.g. して and する).
            // Depth limits work; it does not guess the entry's lexical class.
            result.push((base.clone(), 1));
            if result.len() == MAX_CANDIDATES {
                return result;
            }
            queue.push_back((base, depth + 1));
        }
    }
    result
}

fn bases(word: &str) -> Vec<String> {
    let mut out = Vec::new();
    // Productive colloquial contractions return the original te/de form.
    // A later bounded step applies the ordinary verb-class mappings below.
    // Candidates are checked against installed dictionaries, so ambiguities
    // remain visible rather than guessing the input's lexical class.
    for (ending, connector) in [
        ("ちゃう", "て"),
        ("ちゃった", "て"),
        ("ちゃって", "て"),
        ("ちまう", "て"),
        ("ちまった", "て"),
        ("ちまって", "て"),
        ("じゃう", "で"),
        ("じゃった", "で"),
        ("じゃって", "で"),
        ("じまう", "で"),
        ("じまった", "で"),
        ("じまって", "で"),
        ("とく", "て"),
        ("といた", "て"),
        ("といて", "て"),
        ("どく", "で"),
        ("どいた", "で"),
        ("どいて", "で"),
        ("てる", "て"),
        ("てた", "て"),
        ("てます", "て"),
        ("でる", "で"),
        ("でた", "で"),
        ("でます", "で"),
        ("たら", "た"),
        ("だら", "だ"),
        ("たり", "た"),
        ("だり", "だ"),
        ("なくちゃ", "ない"),
        ("なくっちゃ", "ない"),
        ("なきゃ", "ない"),
    ] {
        if let Some(stem) = word.strip_suffix(ending)
            && !stem.is_empty()
        {
            out.push(format!("{stem}{connector}"));
        }
    }
    {
        let mut replace = |ending: &str, replacements: &[&str]| {
            if let Some(stem) = word.strip_suffix(ending) {
                for replacement in replacements {
                    if !stem.is_empty() {
                        out.push(format!("{stem}{replacement}"));
                    }
                }
            }
        };
        // Productive tense/connection endings. The same surface ending may belong
        // to several verb classes, so none is guessed to be the one true base.
        for ending in ["った", "って"] {
            replace(ending, &["う", "つ", "る"]);
        }
        for ending in ["んだ", "んで"] {
            replace(ending, &["ぬ", "ぶ", "む"]);
        }
        for ending in ["いた", "いて"] {
            replace(ending, &["く"]);
        }
        for ending in ["いだ", "いで"] {
            replace(ending, &["ぐ"]);
        }
        for ending in ["した", "して"] {
            replace(ending, &["す", "する"]);
        }
        for ending in ["た", "て"] {
            replace(ending, &["る"]);
        }
        for ending in ["かった", "くない", "くなかった", "くて", "ければ"] {
            replace(ending, &["い"]);
        }
        // Noun/na-adjective copulas are lookup forms of their predicate.
        for ending in [
            "です",
            "でした",
            "だった",
            "ではない",
            "じゃない",
            "ではなかった",
            "じゃなかった",
        ] {
            replace(ending, &[""]);
        }
    }
    for ending in ["ます", "ました", "ません", "ませんでした", "ましょう"] {
        if let Some(stem) = word.strip_suffix(ending) {
            stem_bases(stem, &I_ROW, &mut out);
            irregular(stem, "し", "する", &mut out);
            irregular(stem, "き", "くる", &mut out);
            if stem.ends_with('来') {
                out.push(format!("{stem}る"));
            }
        }
    }
    for ending in ["ない", "なかった", "なくて", "なければ", "ず"] {
        if let Some(stem) = word.strip_suffix(ending) {
            stem_bases(stem, &A_ROW, &mut out);
            irregular(stem, "し", "する", &mut out);
            irregular(stem, "こ", "くる", &mut out);
            if stem.ends_with('来') {
                out.push(format!("{stem}る"));
            }
        }
    }
    // Progressive/perfect auxiliaries return a te/de form for a later bounded
    // pass. Passive/causative/potential forms may likewise compose with tense.
    for ending in [
        "いる",
        "いた",
        "います",
        "いました",
        "いない",
        "おる",
        "おく",
        "おいた",
        "おいて",
        "しまう",
        "しまった",
        "しまって",
    ] {
        if let Some(stem) = word.strip_suffix(ending)
            && stem.ends_with(['て', 'で'])
        {
            out.push(stem.to_owned());
        }
    }
    for ending in ["られる", "させる"] {
        if let Some(stem) = word.strip_suffix(ending)
            && !stem.is_empty()
        {
            out.push(format!("{stem}る"));
        }
    }
    for ending in ["れば", "よう", "ろ", "よ"] {
        if let Some(stem) = word.strip_suffix(ending)
            && !stem.is_empty()
        {
            out.push(format!("{stem}る"));
        }
    }
    if let Some(stem) = word.strip_suffix('ば') {
        row_base(stem, &E_ROW, &mut out);
    }
    if let Some(stem) = word.strip_suffix('う') {
        row_base(stem, &O_ROW, &mut out);
    }
    // Desiderative endings attach to the same stem as polite -masu.
    for ending in ["たい", "たかった", "たくない", "たくなかった", "たくて"] {
        if let Some(stem) = word.strip_suffix(ending) {
            stem_bases(stem, &I_ROW, &mut out);
        }
    }
    // Polite imperatives and excess/appearance predicates use the same
    // i-row stems as polite verbs. Adjective and nominal readings remain
    // possible alongside the verb readings when the dictionary has them.
    for ending in ["なさい", "すぎる", "過ぎる", "そう"] {
        if let Some(stem) = word.strip_suffix(ending)
            && !stem.is_empty()
        {
            stem_bases(stem, &I_ROW, &mut out);
            irregular(stem, "し", "する", &mut out);
            irregular(stem, "き", "くる", &mut out);
            if ending != "なさい" {
                out.push(format!("{stem}い"));
                if ending == "そう" {
                    out.push(stem.to_owned());
                }
            }
        }
    }
    for (surface, base) in [
        ("される", "する"),
        ("させる", "する"),
        ("できる", "する"),
        ("こられる", "くる"),
        ("きた", "くる"),
        ("きて", "くる"),
        ("すれば", "する"),
        ("しよう", "する"),
        ("しろ", "する"),
        ("せよ", "する"),
        ("くれば", "くる"),
        ("こよう", "くる"),
        ("こい", "くる"),
    ] {
        if let Some(stem) = word.strip_suffix(surface) {
            out.push(format!("{stem}{base}"));
        }
    }
    for ending in ["れる", "せる"] {
        if let Some(stem) = word.strip_suffix(ending) {
            row_base(stem, &A_ROW, &mut out);
        }
    }
    if let Some(stem) = word.strip_suffix('る') {
        row_base(stem, &E_ROW, &mut out);
    }
    for (surface, base) in [
        ("した", "する"),
        ("して", "する"),
        ("きた", "くる"),
        ("きて", "くる"),
        ("こない", "くる"),
        ("できる", "する"),
    ] {
        if word == surface {
            out.push(base.into());
        }
    }
    for ending in ["った", "って"] {
        if let Some(stem) = word.strip_suffix(ending)
            && (stem.ends_with('行') || stem.ends_with('い'))
        {
            out.push(format!("{stem}く"));
        }
    }
    out.retain(|base| !base.is_empty() && base != word);
    out
}

const I_ROW: [(char, char); 9] = [
    ('い', 'う'),
    ('き', 'く'),
    ('ぎ', 'ぐ'),
    ('し', 'す'),
    ('ち', 'つ'),
    ('に', 'ぬ'),
    ('び', 'ぶ'),
    ('み', 'む'),
    ('り', 'る'),
];
const A_ROW: [(char, char); 9] = [
    ('わ', 'う'),
    ('か', 'く'),
    ('が', 'ぐ'),
    ('さ', 'す'),
    ('た', 'つ'),
    ('な', 'ぬ'),
    ('ば', 'ぶ'),
    ('ま', 'む'),
    ('ら', 'る'),
];
const O_ROW: [(char, char); 9] = [
    ('お', 'う'),
    ('こ', 'く'),
    ('ご', 'ぐ'),
    ('そ', 'す'),
    ('と', 'つ'),
    ('の', 'ぬ'),
    ('ぼ', 'ぶ'),
    ('も', 'む'),
    ('ろ', 'る'),
];
const E_ROW: [(char, char); 9] = [
    ('え', 'う'),
    ('け', 'く'),
    ('げ', 'ぐ'),
    ('せ', 'す'),
    ('て', 'つ'),
    ('ね', 'ぬ'),
    ('べ', 'ぶ'),
    ('め', 'む'),
    ('れ', 'る'),
];

fn stem_bases(stem: &str, row: &[(char, char)], out: &mut Vec<String>) {
    if stem.is_empty() {
        return;
    }
    out.push(format!("{stem}る")); // ichidan
    row_base(stem, row, out);
}
fn row_base(stem: &str, row: &[(char, char)], out: &mut Vec<String>) {
    let Some(last) = stem.chars().last() else {
        return;
    };
    if let Some((_, ending)) = row.iter().find(|(kana, _)| *kana == last) {
        let prefix = &stem[..stem.len() - last.len_utf8()];
        out.push(format!("{prefix}{ending}"));
    }
}
fn irregular(stem: &str, ending: &str, base: &str, out: &mut Vec<String>) {
    if let Some(prefix) = stem.strip_suffix(ending) {
        out.push(format!("{prefix}{base}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn factual_conjugations_and_composed_auxiliaries_are_bounded() {
        for (surface, base) in [
            ("食べました", "食べる"),
            ("読みませんでした", "読む"),
            ("話した", "話す"),
            ("書いて", "書く"),
            ("泳いだ", "泳ぐ"),
            ("買った", "買う"),
            ("待って", "待つ"),
            ("帰った", "帰る"),
            ("遊んだ", "遊ぶ"),
            ("死んで", "死ぬ"),
            ("行った", "行く"),
            ("勉強しました", "勉強する"),
            ("来ました", "来る"),
            ("こなかった", "くる"),
            ("高くなかった", "高い"),
            ("静かでした", "静か"),
            ("読んでいた", "読む"),
            ("食べられました", "食べる"),
            ("読ませました", "読む"),
            ("読めなかった", "読む"),
            ("読めば", "読む"),
            ("読もう", "読む"),
            ("食べよう", "食べる"),
            ("食べろ", "食べる"),
            ("勉強しよう", "勉強する"),
            ("こい", "くる"),
            ("読みたくなかった", "読む"),
            ("食べさせられました", "食べる"),
            ("勉強された", "勉強する"),
            ("勉強させた", "勉強する"),
            ("勉強させられました", "勉強する"),
            ("持ってきた", "持ってくる"),
            ("勉強できる", "勉強する"),
            ("こられた", "くる"),
            ("食べちゃった", "食べる"),
            ("読んじゃう", "読む"),
            ("書いちまった", "書く"),
            ("読んじまった", "読む"),
            ("書いとく", "書く"),
            ("読んどいた", "読む"),
            ("食べてる", "食べる"),
            ("読んでた", "読む"),
            ("食べてしまった", "食べる"),
            ("読んでおいた", "読む"),
            ("読んだら", "読む"),
            ("食べたり", "食べる"),
            ("読まなきゃ", "読む"),
            ("読みなさい", "読む"),
            ("勉強しなさい", "勉強する"),
            ("高すぎる", "高い"),
            ("読みすぎる", "読む"),
            ("静かそう", "静か"),
        ] {
            let found = candidates(surface);
            assert!(
                found.iter().any(|(word, _)| word == base),
                "{surface} -> {base}: {found:?}"
            );
            assert!(found.len() <= MAX_CANDIDATES);
        }
        let ambiguous = candidates("読んだ");
        let rank = |word| ambiguous.iter().find(|(s, _)| s == word).unwrap().1;
        assert_eq!(rank("読む"), rank("読ぶ"));
        for word in ["中文", "English", "", "ます", "ない"] {
            assert!(!candidates(word).iter().any(|(base, _)| base.is_empty()));
        }
    }
}
