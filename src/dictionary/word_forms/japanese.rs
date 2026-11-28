//! Independently authored, dictionary-validated Japanese morphology.
//!
//! These are productive conjugation facts, not a port of KOReader/Yomichan's
//! licensed rule tables. Ambiguous bases have equal cost. A real installed
//! dictionary, never this module, decides which candidates have definitions.
use std::collections::{HashSet, VecDeque};

pub(super) const MAX_CANDIDATES: usize = 128;
const MAX_TEXT_VARIANTS: usize = 32;
const MAX_DEPTH: usize = 4;

// Productive te/de auxiliaries can be removed before reducing the underlying
// verb form. Keep this list limited to common inflections; a dictionary still
// decides whether the resulting lexical form exists.
const TE_DE_AUXILIARIES: &[&str] = &[
    "くださいませんでした",
    "くださいません",
    "くださいましたら",
    "くださいました",
    "くださいませんか",
    "くださいます",
    "くださった",
    "くださって",
    "くださる",
    "ください",
    "下さいませんでした",
    "下さいません",
    "下さいました",
    "下さいましたら",
    "下さい",
    "下さいます",
    "下さった",
    "下さって",
    "下さる",
    "いただけませんでした",
    "いただけません",
    "いただけましたら",
    "いただけました",
    "いただけます",
    "いただけない",
    "いただけなかった",
    "いただければ",
    "いただけた",
    "いただきませんでした",
    "いただきません",
    "いただきましたら",
    "いただきました",
    "いただいて",
    "いただいた",
    "いただく",
    "頂けませんでした",
    "頂けません",
    "頂けましたら",
    "頂けました",
    "頂けます",
    "頂けない",
    "頂けなかった",
    "頂ければ",
    "頂けた",
    "頂きませんでした",
    "頂きません",
    "頂きましたら",
    "頂きました",
    "頂いて",
    "頂いた",
    "頂く",
    "もらえませんでした",
    "もらえません",
    "もらえました",
    "もらえます",
    "もらえなかった",
    "もらえない",
    "もらえれば",
    "もらえた",
    "もらいませんでした",
    "もらいません",
    "もらいました",
    "もらいます",
    "もらわなかった",
    "もらわない",
    "もらって",
    "もらった",
    "もらう",
    "貰えませんでした",
    "貰えません",
    "貰えました",
    "貰えます",
    "貰えなかった",
    "貰えない",
    "貰えれば",
    "貰えた",
    "貰いませんでした",
    "貰いません",
    "貰いました",
    "貰います",
    "貰わなかった",
    "貰わない",
    "貰って",
    "貰った",
    "貰う",
    "くれませんでした",
    "くれません",
    "くれました",
    "くれます",
    "くれなかった",
    "くれない",
    "くれれば",
    "くれて",
    "くれた",
    "くれる",
    "あげませんでした",
    "あげません",
    "あげました",
    "あげます",
    "あげなかった",
    "あげない",
    "あげれば",
    "あげて",
    "あげた",
    "あげる",
    "やりませんでした",
    "やりません",
    "やりました",
    "やります",
    "やらなかった",
    "やらない",
    "やれば",
    "やって",
    "やった",
    "やる",
    "みませんでした",
    "みません",
    "みたくなかった",
    "みたくない",
    "みたかった",
    "みたい",
    "みれば",
    "みよう",
    "みろ",
    "みました",
    "みます",
    "みなかった",
    "みない",
    "みて",
    "みた",
    "みる",
    "見ませんでした",
    "見ません",
    "見たくなかった",
    "見たくない",
    "見たかった",
    "見たい",
    "見れば",
    "見よう",
    "見ろ",
    "見ました",
    "見ます",
    "見なかった",
    "見ない",
    "見て",
    "見た",
    "見る",
    "いきませんでした",
    "いきません",
    "いきました",
    "いきます",
    "いかなかった",
    "いかない",
    "いけば",
    "いって",
    "いった",
    "いく",
    "きませんでした",
    "きません",
    "きました",
    "きます",
    "こなかった",
    "こない",
    "くれば",
    "きて",
    "きた",
    "くる",
    "おりませんでした",
    "おりません",
    "おりました",
    "おります",
    "おらなかった",
    "おらない",
    "おれば",
    "おって",
    "おった",
    "おる",
    "おきませんでした",
    "おきません",
    "おきましょう",
    "おきました",
    "おきます",
    "おきたくなかった",
    "おきたくない",
    "おきたかった",
    "おきたい",
    "おかなかった",
    "おかない",
    "おけば",
    "おこう",
    "おいて",
    "おいた",
    "おく",
    "しまいたくなかった",
    "しまいたくない",
    "しまいたかった",
    "しまいたい",
    "しまいませんでした",
    "しまいません",
    "しまいました",
    "しまいます",
    "しまわなかった",
    "しまわない",
    "しまえば",
    "しまって",
    "しまった",
    "しまう",
];

/// Apply the same bounded kana-conversion combinations as the Japanese
/// dictionary workflow. These spellings are lookup candidates only; source
/// text and selection anchors remain untouched.
pub(super) fn text_conversion_variants(
    text: &str,
    settings: super::JapaneseTextConversions,
) -> Vec<String> {
    use super::JapaneseTextConversion as Conversion;

    let mut variants = vec![text.to_owned()];
    for conversion in [
        Conversion::HalfwidthToFullwidth,
        Conversion::HiraganaToKatakana,
        Conversion::KatakanaToHiragana,
        Conversion::CollapseEmphatic,
    ] {
        if !settings.enabled(conversion) {
            continue;
        }
        let prior = variants.clone();
        for variant in prior {
            let converted = match conversion {
                Conversion::HalfwidthToFullwidth => vec![halfwidth_to_fullwidth(&variant)],
                Conversion::HiraganaToKatakana => vec![hiragana_to_katakana(&variant)],
                Conversion::KatakanaToHiragana => vec![katakana_to_hiragana(&variant)],
                Conversion::CollapseEmphatic => collapse_emphatic(&variant),
            };
            for candidate in converted {
                if !candidate.is_empty()
                    && candidate.len() <= super::super::MAX_WORD_BYTES
                    && !variants.contains(&candidate)
                    && variants.len() < MAX_TEXT_VARIANTS
                {
                    variants.push(candidate);
                }
            }
        }
    }
    variants
}

pub(super) fn compatibility_normalize(text: &str, halfwidth_to_fullwidth: bool) -> String {
    let normalizer = icu_normalizer::ComposingNormalizer::new_nfkc();
    if halfwidth_to_fullwidth {
        return normalizer.normalize(text).into_owned();
    }

    // Preserve half-width kana when that Japanese conversion is disabled,
    // while keeping Reader's independent compatibility normalization for all
    // other text.
    let mut output = String::with_capacity(text.len());
    let mut run = String::new();
    let flush = |run: &mut String, output: &mut String| {
        if !run.is_empty() {
            output.push_str(&normalizer.normalize(run).to_string());
            run.clear();
        }
    };
    for ch in text.chars() {
        if matches!(ch, '\u{ff66}'..='\u{ff9f}') {
            flush(&mut run, &mut output);
            output.push(ch);
        } else {
            run.push(ch);
        }
    }
    flush(&mut run, &mut output);
    output
}

fn halfwidth_to_fullwidth(text: &str) -> String {
    let chars: Vec<_> = text.chars().collect();
    let mut output = String::with_capacity(text.len());
    let normalizer = icu_normalizer::ComposingNormalizer::new_nfkc();
    let mut index = 0;
    while index < chars.len() {
        if matches!(chars[index], '\u{ff66}'..='\u{ff9d}') {
            let mut pair = chars[index].to_string();
            if chars
                .get(index + 1)
                .is_some_and(|mark| matches!(mark, '\u{ff9e}' | '\u{ff9f}'))
            {
                pair.push(chars[index + 1]);
                index += 1;
            }
            output.push_str(&normalizer.normalize(&pair).to_string());
        } else {
            output.push(chars[index]);
        }
        index += 1;
    }
    output
}

fn hiragana_to_katakana(text: &str) -> String {
    let chars: Vec<_> = text.chars().collect();
    let mut output = String::with_capacity(text.len());
    let mut index = 0;
    while index < chars.len() {
        let ch = chars[index];
        let code = u32::from(ch);
        if ch == 'わ' && chars.get(index + 1) == Some(&'\u{3099}') {
            output.push('ヷ');
            index += 1;
        } else if ch == 'を' && chars.get(index + 1) == Some(&'\u{3099}') {
            output.push('ヺ');
            index += 1;
        } else if matches!(code, 0x3041..=0x308f | 0x3094..=0x3096 | 0x309d..=0x309e) {
            // The reference's paired kana table intentionally omits historic
            // ゐ/ゑ and maps only supported current/iteration characters.
            output.push(char::from_u32(code + 0x60).unwrap_or(ch));
        } else {
            output.push(ch);
        }
        index += 1;
    }
    output
}

fn katakana_to_hiragana(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    for ch in text.chars() {
        let code = u32::from(ch);
        match code {
            0x30a1..=0x30f6 | 0x30fd..=0x30fe => {
                output.push(char::from_u32(code - 0x60).unwrap_or(ch));
            }
            0x30f7 => output.push_str("わ\u{3099}"),
            0x30fa => output.push_str("を\u{3099}"),
            _ => output.push(ch),
        }
    }
    output
}

fn collapse_emphatic(text: &str) -> Vec<String> {
    let mut one_each = String::with_capacity(text.len());
    let mut none = String::with_capacity(text.len());
    let mut previous = None;
    let mut found_marker = false;
    for ch in text.chars() {
        let emphatic = matches!(ch, 'っ' | 'ッ' | 'ー' | '〜');
        if !emphatic {
            one_each.push(ch);
            none.push(ch);
        } else {
            found_marker = true;
            if previous != Some(ch) {
                one_each.push(ch);
            }
        }
        previous = Some(ch);
    }
    if !found_marker {
        return Vec::new();
    }
    let mut variants = Vec::with_capacity(2);
    if one_each != text {
        variants.push(one_each);
    }
    if none != text && !variants.contains(&none) {
        variants.push(none);
    }
    variants
}

pub(super) fn candidates(word: &str) -> Vec<(String, usize)> {
    if !word
        .chars()
        .any(|c| matches!(c, '\u{3041}'..='\u{3096}'))
    {
        return Vec::new();
    }
    let mut seen = HashSet::from([word.to_owned()]);
    // (text, depth, cost): depth bounds the unwrapping; cost ranks the result.
    let mut queue = VecDeque::from([(word.to_owned(), 0, 0)]);
    let mut result = Vec::new();
    while let Some((text, depth, cost)) = queue.pop_front() {
        if depth == MAX_DEPTH {
            continue;
        }
        // A bare auxiliary (して, した) is itself the valid form, and its verb
        // (する, す) is the same lexical unit, so unwrapping it adds no cost.
        let bare_auxiliary = matches!(text.as_str(), "して" | "した");
        let base_cost = if bare_auxiliary { cost } else { cost + 1 };
        for base in bases(&text) {
            if base.len() > super::super::MAX_WORD_BYTES || !seen.insert(base.clone()) {
                continue;
            }
            // A valid auxiliary form and its final verb must both remain
            // visible when a dictionary indexes both (e.g. して and する).
            // Depth limits work; it does not guess the entry's lexical class.
            // Prefer a direct conjugation to one that must unwrap several
            // auxiliary layers, while keeping each valid intermediate form
            // available if an installed dictionary indexes it.
            result.push((base.clone(), base_cost));
            if result.len() == MAX_CANDIDATES {
                return result;
            }
            queue.push_back((base, depth + 1, base_cost));
        }
    }
    result
}

fn bases(word: &str) -> Vec<String> {
    let mut out = Vec::new();
    // Common obligation and compulsion phrases contain a negative verb stem,
    // followed by a predicate that is not part of the dictionary headword.
    // Reduce the complete phrase before generic suffix handling so both
    // ichidan and godan roots remain available to the installed dictionary.
    for ending in [
        "なくてはならない",
        "なくてはいけない",
        "なければならない",
        "なければいけない",
        "ないとならない",
        "ないといけない",
        "なくちゃならない",
        "なくちゃいけない",
        "なくっちゃならない",
        "なくっちゃいけない",
        "なきゃならない",
        "なきゃいけない",
        "ねばならない",
        "ねばならぬ",
        "ざるを得ない",
        "ずにはいられない",
        "ないではいられない",
    ] {
        if let Some(stem) = word.strip_suffix(ending)
            && !stem.is_empty()
        {
            negative_stem_bases(stem, &mut out);
        }
    }
    // Common colloquial negative forms use ねえ/ねぇ/ねー in place of ない.
    // Reduce first to the ordinary negative so the verb/adjective rules below
    // can handle the underlying lexical class without guessing it here.
    for ending in ["ねえ", "ねぇ", "ねー"] {
        if let Some(stem) = word.strip_suffix(ending)
            && !stem.is_empty()
        {
            out.push(format!("{stem}ない"));
        }
    }
    if let Some(stem) = word.strip_suffix("しげ") {
        out.push(format!("{stem}しい"));
    }
    if let Some(stem) = word.strip_suffix('き')
        && !stem.is_empty()
    {
        // Classical attributive adjectives (e.g. 美しき, 高き) retain the
        // ordinary dictionary adjective as a bounded lookup candidate.
        out.push(format!("{stem}い"));
    }
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
        ("とります", "て"),
        ("どります", "で"),
        ("とりました", "て"),
        ("どりました", "で"),
        ("とった", "て"),
        ("どった", "で"),
        ("とって", "て"),
        ("どって", "で"),
        ("とる", "て"),
        ("どる", "で"),
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
    // The non-finite connectors つつ and ながら take the verb's 連用形.
    // Return dictionary-form candidates through the same godan/ichidan row
    // mapping used for other productive stems; an installed dictionary decides
    // which lexical reading is real.
    for ending in [
        "つつありました",
        "つつありまして",
        "つつあります",
        "つつあった",
        "つつある",
        "つつあり",
        "ながらも",
        "ながら",
        "つつ",
    ] {
        if let Some(stem) = word.strip_suffix(ending)
            && !stem.is_empty()
        {
            stem_bases(stem, &I_ROW, &mut out);
        }
    }
    {
        let replace = |out: &mut Vec<String>, ending: &str, replacements: &[&str]| {
            if let Some(stem) = word.strip_suffix(ending) {
                for replacement in replacements {
                    // A bare auxiliary (して, した) is itself the form to unwrap, so its empty stem is kept.
                    if !stem.is_empty() || matches!(ending, "して" | "した") {
                        out.push(format!("{stem}{replacement}"));
                    }
                }
            }
        };
        // Productive tense/connection endings. The same surface ending may belong
        // to several verb classes, so none is guessed to be the one true base.
        for ending in ["った", "って"] {
            replace(&mut out, ending, &["う", "つ", "る"]);
        }
        for ending in ["んだ", "んで"] {
            replace(&mut out, ending, &["ぬ", "ぶ", "む"]);
        }
        // Progressive and resultative auxiliaries (…でいる, …ている): the te-form of the verb is
        // the stem before いる, so 読んでいた reaches 読んで and then 読む through the rules below.
        for ending in ["でいた", "でいて", "でいる", "でいます", "でいました"] {
            replace(&mut out, ending, &["で"]);
        }
        for ending in ["ていた", "ていて", "ている", "ています", "ていました"] {
            replace(&mut out, ending, &["て"]);
        }
        for ending in ["いた", "いて"] {
            replace(&mut out, ending, &["く"]);
        }
        for ending in ["いだ", "いで"] {
            replace(&mut out, ending, &["ぐ"]);
        }
        for ending in ["した", "して"] {
            replace(&mut out, ending, &["す", "する"]);
        }
        for ending in ["た", "て"] {
            replace(&mut out, ending, &["る"]);
        }
        for ending in [
            "くはございませんでした",
            "くございませんでした",
            "くはありませんでした",
            "くもありませんでした",
            "くありませんでした",
            "くなさそうでした",
            "くなさそうだった",
            "くはなかったです",
            "くもなかったです",
            "くなかったです",
            "くはございません",
            "くございません",
            "くはありません",
            "くもありません",
            "くありません",
            "くなさそうです",
            "くなさそうだ",
            "くなさそうな",
            "くなさそうに",
            "くなさそうで",
            "くはないです",
            "くもないです",
            "くないです",
            "くはなくて",
            "くもなくて",
            "くなくて",
            "くはなかった",
            "くもなかった",
            "かったです",
            "かった",
            "くなさそう",
            "くはない",
            "くもない",
            "くない",
            "くなかった",
            "くて",
            "ければ",
            "かろう",
        ] {
            replace(&mut out, ending, &["い"]);
        }
        // `いい` (also written `良い`) uses the historical `よ` stem for
        // most inflections. Compounds such as かっこいい commonly spell
        // that stem as よ, so retain both dictionary headword spellings.
        for ending in [
            "くはございませんでした",
            "くございませんでした",
            "くはありませんでした",
            "くもありませんでした",
            "くありませんでした",
            "くなさそうでした",
            "くなさそうだった",
            "くはなかったです",
            "くもなかったです",
            "くなかったです",
            "くはございません",
            "くございません",
            "くはありません",
            "くもありません",
            "くありません",
            "くなさそうです",
            "くなさそうだ",
            "くなさそうな",
            "くなさそうに",
            "くなさそうで",
            "くはないです",
            "くもないです",
            "くないです",
            "くなくて",
            "くはなくて",
            "くもなくて",
            "くはなかった",
            "くもなかった",
            "くなかった",
            "くなさそう",
            "くはない",
            "くもない",
            "くない",
            "くて",
            "く",
            "ければ",
            "かったです",
            "かった",
            "さそう",
            "かろう",
        ] {
            if let Some(stem) = word.strip_suffix(ending) {
                if stem.ends_with('良') {
                    out.push(format!("{stem}い"));
                }
                if let Some(prefix) = stem.strip_suffix('よ') {
                    out.push(format!("{prefix}いい"));
                    out.push(format!("{prefix}よい"));
                }
            }
        }
        // Noun/na-adjective copulas are lookup forms of their predicate.
        for ending in [
            "ではありませんでした",
            "じゃありませんでした",
            "ではなかったです",
            "じゃなかったです",
            "ではありません",
            "じゃありません",
            "ではないです",
            "じゃないです",
            "ではなかった",
            "じゃなかった",
            "ではない",
            "じゃない",
            "だったでしょう",
            "だっただろう",
            "だったです",
            "であった",
            "である",
            "でしょう",
            "だろう",
            "でした",
            "だった",
            "です",
            "ならば",
            "なら",
        ] {
            replace(&mut out, ending, &[""]);
        }
    }
    for ending in ["ます", "ました", "ません", "ませんでした", "ましょう"] {
        if let Some(stem) = word.strip_suffix(ending) {
            stem_bases(stem, &I_ROW, &mut out);
            honorific_suru_stem_bases(stem, &mut out);
            irregular(stem, "し", "する", &mut out);
            irregular(stem, "き", "くる", &mut out);
            // Common honorific godan verbs have a polite i-stem that does
            // not map back through the ordinary i-row rules.
            for (surface, base) in [
                ("っしゃい", "っしゃる"),
                ("らっしゃい", "らっしゃる"),
                ("ください", "くださる"),
                ("なさい", "なさる"),
                ("ござい", "ござる"),
            ] {
                irregular(stem, surface, base, &mut out);
            }
            if stem.ends_with('来') {
                out.push(format!("{stem}る"));
            }
        }
    }
    // Common respectful requests and humble benefactive phrases keep the
    // lexical verb as an お/ご-prefixed i-stem before ください or いただく.
    // Strip only the auxiliary when the required honorific prefix is present;
    // ordinary ください/いただく lookups therefore remain literal.
    for ending in [
        "くださいませんでした",
        "くださいません",
        "くださいましたら",
        "くださいました",
        "ください",
        "くださいます",
        "くださった",
        "くださって",
        "くださる",
        "いただけませんでした",
        "いただけません",
        "いただけましたら",
        "いただけました",
        "いただけます",
        "いただけない",
        "いただけなかった",
        "いただければ",
        "いただきませんでした",
        "いただきません",
        "いただきましたら",
        "いただきました",
        "いただいて",
        "いただいた",
        "いただく",
    ] {
        if let Some(stem) = word.strip_suffix(ending) {
            honorific_verb_stem_bases(stem, &mut out);
        }
    }
    // Respectful お/ご + verb-stem + になる phrases retain the lexical
    // verb's i-stem under the polite auxiliary. This independently-authored
    // grammar path does not use KOReader's bundled Yomichan rule table.
    for ending in [
        "になっていました",
        "になっております",
        "になっています",
        "になられました",
        "になられた",
        "になりましたら",
        "になっていた",
        "になっている",
        "になりました",
        "にならない",
        "になりません",
        "になった",
        "になって",
        "になります",
        "になる",
    ] {
        if let Some(stem) = word.strip_suffix(ending) {
            honorific_verb_stem_bases(stem, &mut out);
        }
    }
    for ending in ["ない", "なかった", "なくて", "なければ", "ず", "ぬ", "ん"] {
        if let Some(stem) = word.strip_suffix(ending) {
            negative_stem_bases(stem, &mut out);
        }
    }
    // Aspect, trial, motion, benefactive and request auxiliaries can follow a
    // te/de form. Return that form for the next bounded deconjugation step.
    // This is independently-authored grammar coverage rather than KOReader's
    // bundled GPL rule table.
    for &ending in TE_DE_AUXILIARIES {
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
    // `ずる` has a common `じる` surface variant in past, te, negative,
    // polite, conditional and imperative forms. Keep it alongside the
    // productive `じる` candidate; the installed dictionary resolves which
    // lexical entry exists.
    for ending in [
        "じなさい",
        "じませんでした",
        "じましょう",
        "じすぎる",
        "じません",
        "じました",
        "じます",
        "じなかった",
        "じなければ",
        "じなくて",
        "じない",
        "じれば",
        "じよう",
        "じそう",
        "じろ",
        "じよ",
        "ざれる",
        "ぜられる",
        "ぜる",
        "じて",
        "じた",
        "じず",
    ] {
        if let Some(stem) = word.strip_suffix(ending) {
            out.push(format!("{stem}ずる"));
        }
    }
    if let Some(stem) = word.strip_suffix("じる") {
        out.push(format!("{stem}ずる"));
    }
    if let Some(stem) = word.strip_suffix("ずる") {
        out.push(format!("{stem}じる"));
    }
    // Older dictionaries retain 為る as the headword spelling for する.
    // Recognize its common inflections without rewriting the selected text.
    for ending in [
        "為しませんでした",
        "為しません",
        "為しましょう",
        "為しました",
        "為します",
        "為さなかった",
        "為さない",
        "為さず",
        "為される",
        "為させる",
        "為せば",
        "為そう",
        "為しよう",
        "為しろ",
        "為せよ",
        "為して",
        "為した",
        "為せる",
    ] {
        if let Some(stem) = word.strip_suffix(ending) {
            out.push(format!("{stem}為る"));
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
    // A few common literary and dialectal godan-u verbs retain the older
    // うた/うて past and connective forms rather than shifting to った/って.
    for ending in ["うた", "うて"] {
        if let Some(stem) = word.strip_suffix(ending) {
            out.push(format!("{stem}う"));
        }
    }
    // The negative imperative is a dictionary verb followed by な. Restrict
    // this to kana-final verb spellings to avoid stripping the sentence-final
    // particle from ordinary nouns and names.
    if let Some(verb) = word.strip_suffix('な')
        && verb.chars().last().is_some_and(|last| {
            matches!(last, 'る' | 'う' | 'く' | 'ぐ' | 'す' | 'つ' | 'ぬ' | 'ぶ' | 'む')
        })
    {
        out.push(verb.to_owned());
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

fn negative_stem_bases(stem: &str, out: &mut Vec<String>) {
    stem_bases(stem, &A_ROW, out);
    irregular(stem, "し", "する", out);
    irregular(stem, "こ", "くる", out);
    if stem.ends_with('来') {
        out.push(format!("{stem}る"));
    }
}

fn honorific_verb_stem_bases(stem: &str, out: &mut Vec<String>) {
    let Some(stem) = honorific_prefix_stem(stem) else {
        return;
    };

    stem_bases(stem, &I_ROW, out);
    // Compound verbal nouns can take する after ご (e.g. ご利用). Avoid
    // inventing that reading for an ordinary verb i-stem such as 読み.
    if !I_ROW.iter().any(|(kana, _)| stem.ends_with(*kana)) {
        out.push(format!("{stem}する"));
    }
}

fn honorific_suru_stem_bases(stem: &str, out: &mut Vec<String>) {
    let Some(stem) = honorific_prefix_stem(stem) else {
        return;
    };
    let stem = stem.strip_suffix('し').unwrap_or(stem);
    stem_bases(stem, &I_ROW, out);
    if !I_ROW.iter().any(|(kana, _)| stem.ends_with(*kana)) {
        out.push(format!("{stem}する"));
    }
}

fn honorific_prefix_stem(stem: &str) -> Option<&str> {
    stem
        .strip_prefix('お')
        .or_else(|| stem.strip_prefix('ご'))
        .filter(|stem| !stem.is_empty())
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
            ("高くありませんでした", "高い"),
            ("高くはありませんでした", "高い"),
            ("高くございませんでした", "高い"),
            ("高かろう", "高い"),
            ("よかろう", "よい"),
            ("よくはなかった", "よい"),
            ("高くなかったです", "高い"),
            ("高くなさそう", "高い"),
            ("高くなさそうでした", "高い"),
            ("かっこよくなさそう", "かっこいい"),
            ("よくなさそう", "よい"),
            ("高くないです", "高い"),
            ("高かったです", "高い"),
            ("静かでした", "静か"),
            ("静かではありませんでした", "静か"),
            ("静かじゃないです", "静か"),
            ("静かでしょう", "静か"),
            ("静かであった", "静か"),
            ("読んでいた", "読む"),
            ("読みながら", "読む"),
            ("読みながらも", "読む"),
            ("読みつつ", "読む"),
            ("読みつつあります", "読む"),
            ("読みつつあった", "読む"),
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
            ("いらっしゃいました", "いらっしゃる"),
            ("おっしゃいます", "おっしゃる"),
            ("くださいました", "くださる"),
            ("なさいます", "なさる"),
            ("ございます", "ござる"),
            ("お読みします", "読む"),
            ("お読みしました", "読む"),
            ("お読みになりました", "読む"),
            ("お読みになっていました", "読む"),
            ("お読みください", "読む"),
            ("お読みくださいました", "読む"),
            ("ご確認ください", "確認する"),
            ("ご確認いただきました", "確認する"),
            ("ご利用いただけませんでした", "利用する"),
            ("お読みいただけます", "読む"),
            ("ご利用になりました", "利用する"),
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
            ("読んでみたかった", "読む"),
            ("書いておきたかった", "書く"),
            ("書いておけば", "書く"),
            ("書いておこう", "書く"),
            ("読んでしまいたい", "読む"),
            ("読んだら", "読む"),
            ("食べたり", "食べる"),
            ("読まなきゃ", "読む"),
            ("読みなさい", "読む"),
            ("勉強しなさい", "勉強する"),
            ("高すぎる", "高い"),
            ("読みすぎる", "読む"),
            ("静かそう", "静か"),
            ("読まなくてはならない", "読む"),
            ("食べなければいけない", "食べる"),
            ("勉強しないといけない", "勉強する"),
            ("読まなきゃならない", "読む"),
            ("食べなくっちゃいけない", "食べる"),
            ("読まずにはいられない", "読む"),
            ("食べないではいられない", "食べる"),
            ("使わざるを得ない", "使う"),
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
        assert!(candidates("ください").is_empty());
        assert!(candidates("いただく").is_empty());
    }
}
