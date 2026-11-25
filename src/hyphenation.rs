//! Byte offsets of discretionary breaks. Publication text and its source anchors stay intact.
use crate::{
    Settings,
    book_style::{Hyphens, Style},
    source::{Block, StyledText},
};
use icu_normalizer::{ComposingNormalizer, DecomposingNormalizer};
use icu_properties::{
    CodePointMapData,
    props::{GeneralCategory, Script},
};
use icu_segmenter::GraphemeClusterSegmenter;
use parley::{Language, LineBreak, WordBreak};
use serde::{Deserialize, Serialize};

mod algorithmic;
pub(crate) mod user_patterns;

include!(concat!(env!("OUT_DIR"), "/hyphenation_patterns.rs"));
const MAX_WORD: usize = 64;
const MAX_BREAKS: usize = 4096;

/// Global minimum visible letters on either side of an automatic break.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HyphenationLimits {
    pub left: u8,
    pub right: u8,
}

impl HyphenationLimits {
    pub const fn valid(self) -> bool {
        self.left >= 1 && self.left <= 10 && self.right >= 1 && self.right <= 10
    }
}

pub(crate) const ENGLISH_LIMITS: HyphenationLimits = HyphenationLimits { left: 2, right: 3 };
pub(crate) const ALGORITHMIC_LIMITS: HyphenationLimits = HyphenationLimits { left: 2, right: 2 };

struct Patterns {
    nodes: &'static [(u16, u8, u16, u8)],
    edges: &'static [(char, u16)],
    weights: &'static [u8],
}

#[derive(Clone, Copy)]
pub(crate) struct PatternView<'a> {
    nodes: &'a [(u16, u8, u16, u8)],
    edges: &'a [(char, u16)],
    weights: &'a [u8],
}

impl Patterns {
    fn view(&self) -> PatternView<'_> {
        PatternView {
            nodes: self.nodes,
            edges: self.edges,
            weights: self.weights,
        }
    }
}

// Separate wide tables avoid increasing every other dictionary's memory cost.
struct WidePatterns {
    nodes: &'static [(u32, u32, u32, u32)],
    edges: &'static [(char, u32)],
    weights: &'static [u8],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Dictionary {
    American,
    British,
    Spanish,
    Italian,
    Portuguese,
    PortugueseBrazil,
    Estonian,
    Finnish,
    Danish,
    Dutch,
    French,
    German,
    Bulgarian,
    Galician,
    Irish,
    Ukrainian,
    Polish,
    Russian,
    RussianEnglishBritish,
    RussianEnglishAmerican,
    Swedish,
    Catalan,
    Turkish,
    Croatian,
    Norwegian,
    Lithuanian,
    Icelandic,
    Occitan,
    Welsh,
    Piedmontese,
    Romansh,
    Basque,
    Friulian,
    Esperanto,
    Georgian,
    Greek,
    Hungarian,
    Malayalam,
    Latin,
    LiturgicalLatin,
    Slovak,
    Slovenian,
}

impl Dictionary {
    pub(crate) fn for_language(lang: Option<Language>) -> Option<Self> {
        let lang = lang?;
        let primary = crate::typography_language::canonical_dictionary_language(lang.language());
        if primary == "ml" {
            return matches!(lang.script(), None | Some("Mlym")).then_some(Self::Malayalam);
        }
        if primary == "el" {
            return matches!(lang.script(), None | Some("Grek")).then_some(Self::Greek);
        }
        if primary == "ka" {
            return matches!(lang.script(), None | Some("Geor")).then_some(Self::Georgian);
        }
        if matches!(primary, "bg" | "uk" | "ru") {
            if !matches!(lang.script(), None | Some("Cyrl")) {
                return None;
            }
            return match primary {
                "bg" => Some(Self::Bulgarian),
                "uk" => Some(Self::Ukrainian),
                "ru" => Some(match lang.region() {
                    Some("GB") => Self::RussianEnglishBritish,
                    Some("US") => Self::RussianEnglishAmerican,
                    _ => Self::Russian,
                }),
                _ => unreachable!(),
            };
        }
        if !matches!(lang.script(), None | Some("Latn")) {
            return None;
        }
        match (primary, lang.region()) {
            ("en", Some("GB")) => Some(Self::British),
            ("en", _) => Some(Self::American),
            ("es", _) => Some(Self::Spanish),
            ("it", _) => Some(Self::Italian),
            ("pt", Some("BR")) => Some(Self::PortugueseBrazil),
            ("pt", _) => Some(Self::Portuguese),
            ("et", _) => Some(Self::Estonian),
            ("fi", _) => Some(Self::Finnish),
            ("da", _) => Some(Self::Danish),
            ("nl", _) => Some(Self::Dutch),
            ("fr", _) => Some(Self::French),
            ("de", _) => Some(Self::German),
            ("gl", _) => Some(Self::Galician),
            ("ga", _) => Some(Self::Irish),
            ("pl", _) => Some(Self::Polish),
            ("sv", _) => Some(Self::Swedish),
            ("ca", _) => Some(Self::Catalan),
            ("tr", _) => Some(Self::Turkish),
            ("hr", _) => Some(Self::Croatian),
            ("no", _) => Some(Self::Norwegian),
            ("lt", _) => Some(Self::Lithuanian),
            ("is", _) => Some(Self::Icelandic),
            ("oc", _) => Some(Self::Occitan),
            ("cy", _) => Some(Self::Welsh),
            ("pms", _) => Some(Self::Piedmontese),
            ("rm", _) => Some(Self::Romansh),
            ("eu", _) => Some(Self::Basque),
            ("fur", _) => Some(Self::Friulian),
            ("eo", _) => Some(Self::Esperanto),
            ("la", _) => Some(Self::Latin),
            ("hu", _) => Some(Self::Hungarian),
            ("sk", _) => Some(Self::Slovak),
            ("sl", _) => Some(Self::Slovenian),
            _ => None,
        }
    }

    #[cfg(feature = "kindle")]
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::American => "English (US)",
            Self::British => "English (UK)",
            Self::Spanish => "Spanish",
            Self::Italian => "Italian",
            Self::Portuguese => "Portuguese (Portugal)",
            Self::PortugueseBrazil => "Portuguese (Brazil, shared rules)",
            Self::Estonian => "Estonian",
            Self::Finnish => "Finnish",
            Self::Danish => "Danish",
            Self::Dutch => "Dutch",
            Self::French => "French",
            Self::German => "German",
            Self::Bulgarian => "Bulgarian",
            Self::Galician => "Galician",
            Self::Irish => "Irish",
            Self::Ukrainian => "Ukrainian",
            Self::Polish => "Polish",
            Self::Russian => "Russian",
            Self::RussianEnglishBritish => "Russian + English (UK)",
            Self::RussianEnglishAmerican => "Russian + English (US)",
            Self::Swedish => "Swedish",
            Self::Catalan => "Catalan",
            Self::Turkish => "Turkish",
            Self::Croatian => "Croatian",
            Self::Norwegian => "Norwegian",
            Self::Lithuanian => "Lithuanian",
            Self::Icelandic => "Icelandic",
            Self::Occitan => "Occitan",
            Self::Welsh => "Welsh",
            Self::Piedmontese => "Piedmontese",
            Self::Romansh => "Romansh",
            Self::Basque => "Basque",
            Self::Friulian => "Friulian",
            Self::Esperanto => "Esperanto",
            Self::Georgian => "Georgian",
            Self::Greek => "Greek",
            Self::Hungarian => "Hungarian",
            Self::Malayalam => "Malayalam",
            Self::Latin => "Latin",
            Self::LiturgicalLatin => "Latin (liturgical)",
            Self::Slovak => "Slovak",
            Self::Slovenian => "Slovenian",
        }
    }

    pub(crate) fn limits(self) -> HyphenationLimits {
        match self {
            Self::Spanish
            | Self::Italian
            | Self::Finnish
            | Self::Danish
            | Self::Dutch
            | Self::German
            | Self::Bulgarian
            | Self::Galician
            | Self::Ukrainian
            | Self::Polish
            | Self::Russian
            | Self::RussianEnglishBritish
            | Self::RussianEnglishAmerican
            | Self::Swedish
            | Self::Catalan
            | Self::Turkish
            | Self::Croatian
            | Self::Norwegian
            | Self::Lithuanian
            | Self::Icelandic
            | Self::Occitan
            | Self::Piedmontese
            | Self::Romansh
            | Self::Basque
            | Self::Friulian
            | Self::Esperanto
            | Self::Latin
            | Self::Hungarian
            | Self::LiturgicalLatin
            | Self::Slovenian => HyphenationLimits { left: 2, right: 2 },
            Self::French => HyphenationLimits { left: 2, right: 1 },
            Self::Georgian => HyphenationLimits { left: 1, right: 2 },
            Self::Greek => HyphenationLimits { left: 1, right: 1 },
            Self::American
            | Self::British
            | Self::Portuguese
            | Self::PortugueseBrazil
            | Self::Estonian
            | Self::Irish
            | Self::Slovak
            | Self::Malayalam
            | Self::Welsh => ENGLISH_LIMITS,
        }
    }

    fn patterns(self) -> &'static Patterns {
        match self {
            Self::American => &AMERICAN,
            Self::British => &BRITISH,
            Self::Spanish => &SPANISH,
            Self::Italian => &ITALIAN,
            Self::Portuguese => &PORTUGUESE,
            Self::PortugueseBrazil => &PORTUGUESE,
            Self::Estonian => &ESTONIAN,
            Self::Finnish => &FINNISH,
            Self::Danish => &DANISH,
            Self::Dutch => &DUTCH,
            Self::French => &FRENCH,
            Self::German => &GERMAN,
            Self::Bulgarian => &BULGARIAN,
            Self::Galician => &GALICIAN,
            Self::Irish => &IRISH,
            Self::Ukrainian => &UKRAINIAN,
            Self::Polish => &POLISH,
            Self::Russian | Self::RussianEnglishBritish | Self::RussianEnglishAmerican => &RUSSIAN,
            Self::Swedish => &SWEDISH,
            Self::Catalan => &CATALAN,
            Self::Turkish => &TURKISH,
            Self::Croatian => &CROATIAN,
            Self::Norwegian => &NORWEGIAN,
            Self::Lithuanian => &LITHUANIAN,
            Self::Icelandic => &ICELANDIC,
            Self::Occitan => &OCCITAN,
            Self::Welsh => &WELSH,
            Self::Piedmontese => &PIEDMONTESE,
            Self::Romansh => &ROMANSH,
            Self::Friulian => &FRIULIAN,
            Self::Esperanto => &ESPERANTO,
            Self::Georgian => &GEORGIAN,
            Self::Greek => &GREEK,
            Self::Hungarian => unreachable!("Hungarian uses the wide matcher"),
            Self::Malayalam => &MALAYALAM,
            Self::Latin => &LATIN,
            Self::LiturgicalLatin => &LITURGICAL_LATIN,
            Self::Basque => {
                // Retain the full upstream notice and modification statement
                // alongside the compiled-in, modified Basque pattern data.
                std::hint::black_box(include_str!("../docs/licenses/HYPHENATION-EU.txt"));
                &BASQUE
            }
            Self::Slovak => &SLOVAK,
            Self::Slovenian => &SLOVENIAN,
        }
    }
}

fn dictionary(style: &Style, locale: Option<crate::TypographyLocale>) -> Option<Dictionary> {
    if !allows_user_rules(style) {
        return None;
    }
    match locale? {
        crate::TypographyLocale::LiturgicalLatin => Some(Dictionary::LiturgicalLatin),
        crate::TypographyLocale::Language(language) => Dictionary::for_language(Some(language)),
    }
}

pub(crate) fn opportunities(block: &Block, styles: &[Style], settings: Settings) -> Vec<usize> {
    opportunities_text(&block.text, &block.styled_text, styles, settings)
}

pub(crate) fn opportunities_text(
    text: &str,
    styled: &[StyledText],
    styles: &[Style],
    settings: Settings,
) -> Vec<usize> {
    opportunities_text_with_override(text, styled, styles, settings, None)
}

fn opportunities_text_with_override(
    text: &str,
    styled: &[StyledText],
    styles: &[Style],
    settings: Settings,
    rules: Option<&crate::user_hyphenation::Rules>,
) -> Vec<usize> {
    let mut breaks = Vec::new();
    let algorithmic = settings.algorithmic_hyphenation;
    if !settings.automatic_hyphenation
        || settings.disable_soft_hyphens
        || !styled.iter().any(|run| {
            let style = &styles[run.element];
            if algorithmic {
                allows_user_rules(style)
            } else {
                dictionary(style, run.hyphenation_language).is_some()
            }
        })
    {
        return breaks;
    }
    for range in crate::selection::word_ranges(text) {
        let word = &text[range.clone()];
        let first = styled.partition_point(|span| span.bytes.end <= range.start);
        let mut spans = styled[first..]
            .iter()
            .take_while(|span| span.bytes.start < range.end);
        let Some(head) = spans.next() else {
            continue;
        };
        if head.bytes.start > range.start {
            continue;
        }
        let locale = dictionary(&styles[head.element], head.hyphenation_language);
        if (algorithmic && !allows_user_rules(&styles[head.element]))
            || (!algorithmic && locale.is_none())
        {
            continue;
        }
        let mut covered = head.bytes.end.min(range.end);
        let valid = spans.all(|span| {
            if span.bytes.start > covered
                || if algorithmic {
                    !allows_user_rules(&styles[span.element])
                } else {
                    dictionary(&styles[span.element], span.hyphenation_language) != locale
                }
            {
                return false;
            }
            covered = covered.max(span.bytes.end.min(range.end));
            true
        });
        if !valid || covered != range.end {
            continue;
        }
        let emit = |offset| {
            if breaks.len() < MAX_BREAKS {
                breaks.push(range.start + offset);
            }
        };
        if algorithmic {
            let limits = settings.hyphenation_limits.unwrap_or(ALGORITHMIC_LIMITS);
            algorithmic::breaks(word, limits, emit);
        } else if let Some(locale) = locale
            && locale != Dictionary::Greek
        {
            let limits = settings
                .hyphenation_limits
                .unwrap_or_else(|| locale.limits());
            let override_state = rules.and_then(|rules| match locale {
                Dictionary::French => Some(&rules.french),
                Dictionary::Polish => Some(&rules.polish),
                Dictionary::PortugueseBrazil => Some(&rules.brazilian),
                _ => None,
            });
            if let Some(crate::user_hyphenation::OptionalPatterns::Loaded(patterns)) =
                override_state
            {
                optional_pattern_word(patterns.view(), word, limits, emit);
            } else {
                word_breaks_with_limits(locale, word, limits, emit);
            }
        }
        if breaks.len() == MAX_BREAKS {
            break;
        }
    }
    if !algorithmic
        && breaks.len() < MAX_BREAKS
        && styled.iter().any(|run| {
            dictionary(&styles[run.element], run.hyphenation_language) == Some(Dictionary::Greek)
        })
    {
        for range in greek_word_ranges(text) {
            let first = styled.partition_point(|span| span.bytes.end <= range.start);
            let mut covered = range.start;
            let mut valid = true;
            for span in styled[first..]
                .iter()
                .take_while(|span| span.bytes.start < range.end)
            {
                if span.bytes.start > covered
                    || dictionary(&styles[span.element], span.hyphenation_language)
                        != Some(Dictionary::Greek)
                {
                    valid = false;
                    break;
                }
                covered = covered.max(span.bytes.end.min(range.end));
            }
            if valid && covered == range.end {
                let limits = settings
                    .hyphenation_limits
                    .unwrap_or_else(|| Dictionary::Greek.limits());
                word_breaks_with_limits(
                    Dictionary::Greek,
                    &text[range.clone()],
                    limits,
                    |offset| {
                        if breaks.len() < MAX_BREAKS {
                            breaks.push(range.start + offset);
                        }
                    },
                );
            }
            if breaks.len() == MAX_BREAKS {
                break;
            }
        }
        breaks.sort_unstable();
        breaks.dedup();
    }
    breaks
}

// CRE's word scanner treats U+02BC as an alphabetic letter and Greek
// koronis/psili as modifiers. ASCII and right quotation apostrophes split.
fn greek_word_ranges(text: &str) -> impl Iterator<Item = std::ops::Range<usize>> + '_ {
    let mut chars = text.char_indices().peekable();
    let categories = CodePointMapData::<GeneralCategory>::new();
    let scripts = CodePointMapData::<Script>::new();
    std::iter::from_fn(move || {
        let mut start = None;
        while let Some((at, ch)) = chars.next() {
            let modifier = matches!(ch, '\u{1fbd}' | '\u{1fbf}')
                || matches!(
                    categories.get(ch),
                    GeneralCategory::NonspacingMark
                        | GeneralCategory::SpacingMark
                        | GeneralCategory::EnclosingMark
                );
            let letter = !modifier
                && ch.is_alphabetic()
                && !matches!(
                    scripts.get(ch),
                    Script::Han | Script::Hiragana | Script::Katakana | Script::Hangul
                );
            let part = letter || modifier || ch == '\u{ad}';
            if start.is_none() {
                if letter || ch == '\u{ad}' {
                    start = Some(at);
                }
            } else if !part {
                return Some(start.unwrap()..at);
            }
        }
        start.map(|at| at..text.len())
    })
}

pub(crate) fn suggestion(
    rules: &crate::user_hyphenation::Rules,
    language: &str,
    word: &str,
) -> String {
    if let Some(mask) = rules.mask(language, word) {
        return crate::user_hyphenation::marked(
            word,
            &crate::user_hyphenation::source_offsets(word, &mask),
        );
    }
    let mut breaks = Vec::new();
    if word.contains('\u{ad}') {
        breaks.extend(
            word.char_indices()
                .filter_map(|(i, c)| (c == '\u{ad}').then_some(i)),
        );
    } else if is_armenian_language(crate::TypographyLocale::parse(language))
        && word.len() <= MAX_WORD * 5
        && let crate::user_hyphenation::OptionalPatterns::Loaded(patterns) = &rules.armenian
    {
        pattern_breaks(
            patterns.view(),
            None,
            word,
            HyphenationLimits { left: 1, right: 2 },
            false,
            false,
            |at| breaks.push(at),
        );
    } else if is_zulu_language(crate::TypographyLocale::parse(language))
        && word.len() <= MAX_WORD * 5
        && let crate::user_hyphenation::OptionalPatterns::Loaded(patterns) = &rules.zulu
    {
        pattern_breaks(
            patterns.view(),
            None,
            word,
            HyphenationLimits { left: 2, right: 1 },
            false,
            false,
            |at| breaks.push(at),
        );
    } else if word.len() <= MAX_WORD * 5
        && is_serbian_script(crate::TypographyLocale::parse(language), true)
        && word_has_script(word, Script::Latin)
        && let crate::user_hyphenation::OptionalPatterns::Loaded(patterns) = &rules.serbian_latin
    {
        pattern_breaks(
            patterns.view(),
            None,
            word,
            HyphenationLimits { left: 2, right: 2 },
            false,
            false,
            |at| breaks.push(at),
        );
    } else if word.len() <= MAX_WORD * 5
        && is_serbian_script(crate::TypographyLocale::parse(language), false)
        && word_has_script(word, Script::Cyrillic)
        && let crate::user_hyphenation::OptionalPatterns::Loaded(patterns) = &rules.serbian_cyrillic
    {
        pattern_breaks(
            patterns.view(),
            None,
            word,
            HyphenationLimits { left: 2, right: 2 },
            false,
            false,
            |at| breaks.push(at),
        );
    } else if word.len() <= MAX_WORD * 5
        && is_czech_language(crate::TypographyLocale::parse(language))
        && word_has_script(word, Script::Latin)
        && let crate::user_hyphenation::OptionalPatterns::Loaded(patterns) = &rules.czech
    {
        pattern_breaks(
            patterns.view(),
            None,
            word,
            HyphenationLimits { left: 2, right: 3 },
            false,
            false,
            |at| breaks.push(at),
        );
    } else if word.len() <= MAX_WORD * 5
        && is_romanian_language(crate::TypographyLocale::parse(language))
        && word_has_script(word, Script::Latin)
        && let crate::user_hyphenation::OptionalPatterns::Loaded(patterns) = &rules.romanian
    {
        pattern_breaks(
            patterns.view(),
            None,
            word,
            HyphenationLimits { left: 2, right: 2 },
            false,
            false,
            |at| breaks.push(at),
        );
    } else if word.len() <= MAX_WORD * 5
        && is_latvian_language(crate::TypographyLocale::parse(language))
        && word_has_script(word, Script::Latin)
        && let crate::user_hyphenation::OptionalPatterns::Loaded(patterns) = &rules.latvian
    {
        pattern_breaks(
            patterns.view(),
            None,
            word,
            HyphenationLimits { left: 2, right: 2 },
            false,
            false,
            |at| breaks.push(at),
        );
    } else if dictionary(&Style::default(), crate::TypographyLocale::parse(language))
        == Some(Dictionary::French)
        && let crate::user_hyphenation::OptionalPatterns::Loaded(patterns) = &rules.french
    {
        optional_pattern_word(
            patterns.view(),
            word,
            HyphenationLimits { left: 2, right: 1 },
            |at| breaks.push(at),
        );
    } else if dictionary(&Style::default(), crate::TypographyLocale::parse(language))
        == Some(Dictionary::Polish)
        && let crate::user_hyphenation::OptionalPatterns::Loaded(patterns) = &rules.polish
    {
        optional_pattern_word(
            patterns.view(),
            word,
            HyphenationLimits { left: 2, right: 2 },
            |at| breaks.push(at),
        );
    } else if word.len() <= MAX_WORD * 5
        && is_macedonian_language(crate::TypographyLocale::parse(language))
        && word_has_script(word, Script::Cyrillic)
        && let crate::user_hyphenation::OptionalPatterns::Loaded(patterns) = &rules.macedonian
    {
        pattern_breaks(
            patterns.view(),
            None,
            word,
            HyphenationLimits { left: 2, right: 2 },
            false,
            false,
            |at| breaks.push(at),
        );
    } else if dictionary(&Style::default(), crate::TypographyLocale::parse(language))
        == Some(Dictionary::PortugueseBrazil)
        && let crate::user_hyphenation::OptionalPatterns::Loaded(patterns) = &rules.brazilian
    {
        optional_pattern_word(
            patterns.view(),
            word,
            HyphenationLimits { left: 2, right: 3 },
            |at| breaks.push(at),
        );
    } else if let Some(locale) =
        dictionary(&Style::default(), crate::TypographyLocale::parse(language))
    {
        word_breaks(locale, word, |i| breaks.push(i));
    }
    crate::user_hyphenation::marked(word, &breaks)
}

pub(crate) fn is_armenian_language(locale: Option<crate::TypographyLocale>) -> bool {
    let Some(crate::TypographyLocale::Language(language)) = locale else {
        return false;
    };
    matches!(language.language(), "hy" | "hye" | "arm" | "hyw")
        && matches!(language.script(), None | Some("Armn"))
}

pub(crate) fn is_zulu_language(locale: Option<crate::TypographyLocale>) -> bool {
    let Some(crate::TypographyLocale::Language(language)) = locale else {
        return false;
    };
    matches!(language.language(), "zu" | "zul") && matches!(language.script(), None | Some("Latn"))
}

pub(crate) fn is_macedonian_language(locale: Option<crate::TypographyLocale>) -> bool {
    let Some(crate::TypographyLocale::Language(language)) = locale else {
        return false;
    };
    language.language() == "mk" && matches!(language.script(), None | Some("Cyrl"))
}

pub(crate) fn is_latvian_language(locale: Option<crate::TypographyLocale>) -> bool {
    let Some(crate::TypographyLocale::Language(language)) = locale else {
        return false;
    };
    matches!(language.language(), "lv" | "lav") && matches!(language.script(), None | Some("Latn"))
}

pub(crate) fn is_romanian_language(locale: Option<crate::TypographyLocale>) -> bool {
    let Some(crate::TypographyLocale::Language(language)) = locale else {
        return false;
    };
    matches!(language.language(), "ro" | "ron") && matches!(language.script(), None | Some("Latn"))
}

pub(crate) fn is_czech_language(locale: Option<crate::TypographyLocale>) -> bool {
    let Some(crate::TypographyLocale::Language(language)) = locale else {
        return false;
    };
    matches!(language.language(), "cs" | "ces") && matches!(language.script(), None | Some("Latn"))
}

pub(crate) fn is_serbian_language(locale: Option<crate::TypographyLocale>) -> bool {
    let Some(crate::TypographyLocale::Language(language)) = locale else {
        return false;
    };
    matches!(language.language(), "sr" | "srp")
        && matches!(language.script(), None | Some("Latn") | Some("Cyrl"))
}

fn is_serbian_script(locale: Option<crate::TypographyLocale>, latin: bool) -> bool {
    let Some(crate::TypographyLocale::Language(language)) = locale else {
        return false;
    };
    matches!(language.language(), "sr" | "srp")
        && (language.script().is_none()
            || language.script() == Some(if latin { "Latn" } else { "Cyrl" }))
}

// Mixed-script words must not receive rules from only one Serbian table.
fn word_has_script(word: &str, target: Script) -> bool {
    let scripts = CodePointMapData::<Script>::new();
    let mut found = false;
    for ch in word.chars().filter(|ch| ch.is_alphabetic()) {
        match scripts.get(ch) {
            script if script == target => found = true,
            Script::Common | Script::Inherited => {}
            _ => return false,
        }
    }
    found
}

pub(crate) fn allows_user_rules(style: &Style) -> bool {
    style.hyphens == Hyphens::Auto
        && style.word_break != WordBreak::BreakAll
        && style.line_break != Some(LineBreak::Anywhere)
}

pub(crate) fn with_user_rules(
    block: &Block,
    styles: &[Style],
    settings: Settings,
    rules: &crate::user_hyphenation::Rules,
) -> crate::user_hyphenation::Opportunities {
    with_user_rules_text(&block.text, &block.styled_text, styles, settings, rules)
}

pub(crate) fn with_user_rules_text(
    text: &str,
    styled: &[StyledText],
    styles: &[Style],
    settings: Settings,
    rules: &crate::user_hyphenation::Rules,
) -> crate::user_hyphenation::Opportunities {
    let breaks = opportunities_text_with_override(text, styled, styles, settings, Some(rules));
    let mut result = crate::user_hyphenation::Opportunities {
        breaks,
        custom_words: Vec::new(),
        matching_soft_hyphens: Vec::new(),
    };
    if settings.automatic_hyphenation
        && !settings.algorithmic_hyphenation
        && !settings.disable_soft_hyphens
    {
        type LocaleCheck = fn(Option<crate::TypographyLocale>) -> bool;
        let sources: [(
            &crate::user_hyphenation::OptionalPatterns,
            LocaleCheck,
            Option<Script>,
            HyphenationLimits,
        ); 8] = [
            (
                &rules.armenian,
                is_armenian_language,
                None,
                HyphenationLimits { left: 1, right: 2 },
            ),
            (
                &rules.zulu,
                is_zulu_language,
                None,
                HyphenationLimits { left: 2, right: 1 },
            ),
            (
                &rules.serbian_latin,
                |locale| is_serbian_script(locale, true),
                Some(Script::Latin),
                HyphenationLimits { left: 2, right: 2 },
            ),
            (
                &rules.serbian_cyrillic,
                |locale| is_serbian_script(locale, false),
                Some(Script::Cyrillic),
                HyphenationLimits { left: 2, right: 2 },
            ),
            (
                &rules.czech,
                is_czech_language,
                Some(Script::Latin),
                HyphenationLimits { left: 2, right: 3 },
            ),
            (
                &rules.romanian,
                is_romanian_language,
                Some(Script::Latin),
                HyphenationLimits { left: 2, right: 2 },
            ),
            (
                &rules.latvian,
                is_latvian_language,
                Some(Script::Latin),
                HyphenationLimits { left: 2, right: 2 },
            ),
            (
                &rules.macedonian,
                is_macedonian_language,
                Some(Script::Cyrillic),
                HyphenationLimits { left: 2, right: 2 },
            ),
        ];
        for (state, matches_locale, script, defaults) in sources {
            let crate::user_hyphenation::OptionalPatterns::Loaded(patterns) = state else {
                continue;
            };
            for range in crate::selection::word_ranges(text) {
                if range.len() > MAX_WORD * 5
                    || script.is_some_and(|target| !word_has_script(&text[range.clone()], target))
                {
                    continue;
                }
                let first = styled.partition_point(|span| span.bytes.end <= range.start);
                let mut covered = range.start;
                let mut valid = true;
                for span in styled[first..]
                    .iter()
                    .take_while(|span| span.bytes.start < range.end)
                {
                    if span.bytes.start > covered
                        || !allows_user_rules(&styles[span.element])
                        || !matches_locale(span.hyphenation_language)
                    {
                        valid = false;
                        break;
                    }
                    covered = covered.max(span.bytes.end.min(range.end));
                }
                if !valid || covered != range.end {
                    continue;
                }
                let mut offset = 0;
                let limits = settings.hyphenation_limits.unwrap_or(defaults);
                if !limits.valid() {
                    continue;
                }
                for part in text[range.clone()].split_inclusive(['\'', '\u{2019}']) {
                    let word = part.trim_end_matches(['\'', '\u{2019}']);
                    pattern_breaks(patterns.view(), None, word, limits, false, false, |at| {
                        if result.breaks.len() < MAX_BREAKS {
                            result.breaks.push(range.start + offset + at);
                        }
                    });
                    offset += part.len();
                }
                if result.breaks.len() == MAX_BREAKS {
                    break;
                }
            }
        }
        result.breaks.sort_unstable();
        result.breaks.dedup();
    }
    if !rules.has_rules() || settings.disable_soft_hyphens {
        return result;
    }
    let mut custom_breaks = Vec::new();
    for range in crate::selection::word_ranges(text) {
        let first = styled.partition_point(|span| span.bytes.end <= range.start);
        let mut spans = styled[first..]
            .iter()
            .take_while(|span| span.bytes.start < range.end);
        let Some(run) = spans.next() else {
            continue;
        };
        let Some(language) = run.language else {
            continue;
        };
        let word = &text[range.clone()];
        let loc = rules.locale_for_word(&language, word);
        let allowed = allows_user_rules;
        let mut covered = run.bytes.end.min(range.end);
        if run.bytes.start > range.start
            || !allowed(&styles[run.element])
            || !spans.all(|s| {
                let valid = s.bytes.start <= covered
                    && allowed(&styles[s.element])
                    && s.language
                        .as_ref()
                        .is_some_and(|l| rules.locale_for_word(l, word) == loc);
                covered = covered.max(s.bytes.end.min(range.end));
                valid
            })
            || covered != range.end
        {
            continue;
        }
        if let Some(mask) = rules.mask(loc, word) {
            for byte in crate::user_hyphenation::source_offsets(word, &mask) {
                // A matching publisher soft hyphen already supplies the boundary and glyph.
                if let Some(prefix) = word[..byte].strip_suffix('\u{ad}') {
                    result
                        .matching_soft_hyphens
                        .push(range.start + prefix.len());
                } else {
                    custom_breaks.push(range.start + byte);
                }
            }
            result.custom_words.push(range);
            if result.custom_words.len() == MAX_BREAKS
                || custom_breaks.len() + result.matching_soft_hyphens.len() >= MAX_BREAKS
            {
                break;
            }
        }
    }
    if result.custom_words.is_empty() {
        return result;
    }
    result.breaks.retain(|pos| {
        let i = result.custom_words.partition_point(|r| r.end <= *pos);
        !result.custom_words.get(i).is_some_and(|r| r.contains(pos))
    });
    result.breaks.extend(custom_breaks);
    result.breaks.sort_unstable();
    result.breaks.dedup();
    result.breaks.truncate(MAX_BREAKS);
    result
}

fn word_breaks(locale: Dictionary, word: &str, emit: impl FnMut(usize)) {
    word_breaks_with_limits(locale, word, locale.limits(), emit);
}

fn word_breaks_with_limits(
    locale: Dictionary,
    word: &str,
    limits: HyphenationLimits,
    mut emit: impl FnMut(usize),
) {
    if !limits.valid() || word.len() > MAX_WORD * 5 {
        return;
    }
    if locale == Dictionary::Hungarian {
        let mut offset = 0;
        for part in word.split_inclusive(['\'', '\u{2019}']) {
            let text = part.trim_end_matches(['\'', '\u{2019}']);
            hungarian_pattern_breaks(text, limits, |at| emit(offset + at));
            offset += part.len();
        }
        return;
    }
    let patterns = locale.patterns();
    let additional = match locale {
        Dictionary::RussianEnglishBritish => Some(&BRITISH),
        Dictionary::RussianEnglishAmerican => Some(&AMERICAN),
        _ => None,
    };
    // CRE's dictionary lookup uses alphabetic runs, while ICU may keep
    // apostrophes and Catalan's middle dot inside a single selected word.
    // Split only the lookup input and retain the original UTF-8 offsets.
    let separators: &[char] = if locale == Dictionary::Catalan {
        &['\'', '\u{2019}', '\u{00b7}']
    } else {
        &['\'', '\u{2019}']
    };
    let mut offset = 0;
    for part in word.split_inclusive(separators) {
        let text = part.trim_end_matches(separators);
        if matches!(locale, Dictionary::American | Dictionary::British)
            && !word_has_script(text, Script::Latin)
        {
            // Do not apply English patterns to mixed or non-Latin runs.
            offset += part.len();
            continue;
        }
        if matches!(
            locale,
            Dictionary::Latin
                | Dictionary::LiturgicalLatin
                | Dictionary::Greek
                | Dictionary::Malayalam
        ) {
            decomposed_pattern_breaks(
                patterns,
                text,
                limits,
                locale == Dictionary::Greek,
                locale == Dictionary::Malayalam,
                |at| emit(offset + at),
            );
        } else {
            pattern_breaks(
                patterns.view(),
                additional.map(Patterns::view),
                text,
                limits,
                false,
                locale == Dictionary::Turkish,
                |at| emit(offset + at),
            );
        }
        offset += part.len();
    }
}

fn optional_pattern_word(
    patterns: PatternView<'_>,
    word: &str,
    limits: HyphenationLimits,
    mut emit: impl FnMut(usize),
) {
    if !limits.valid() || word.len() > MAX_WORD * 5 {
        return;
    }
    let mut offset = 0;
    for part in word.split_inclusive(['\'', '\u{2019}']) {
        let text = part.trim_end_matches(['\'', '\u{2019}']);
        pattern_breaks(patterns, None, text, limits, false, false, |at| {
            emit(offset + at)
        });
        offset += part.len();
    }
}

// Hungarian is the only dictionary whose trie exceeds 16-bit node indexes.
// Keep source offsets bounded to original grapheme boundaries.
fn hungarian_pattern_breaks(word: &str, limits: HyphenationLimits, mut emit: impl FnMut(usize)) {
    let mut letters = ['.'; MAX_WORD + 2];
    let mut offsets = [0usize; MAX_WORD + 1];
    let mut n = 0usize;
    let mut start = 0usize;
    let nfc = ComposingNormalizer::new_nfc();
    for end in GraphemeClusterSegmenter::new().segment_str(word).skip(1) {
        let cluster = &word[start..end];
        start = end;
        if cluster == "\u{ad}" {
            continue;
        }
        let composed = nfc.normalize(cluster);
        let mut chars = composed.chars();
        let Some(character) = chars.next().filter(|ch| ch.is_alphabetic()) else {
            return;
        };
        let mut lowercase = character.to_lowercase();
        let Some(letter) = lowercase.next() else {
            return;
        };
        if n == MAX_WORD || chars.next().is_some() || lowercase.next().is_some() {
            return;
        }
        n += 1;
        letters[n] = letter;
        offsets[n] = end;
    }
    let left = usize::from(limits.left);
    let right = usize::from(limits.right);
    if n < left + right {
        return;
    }
    let mut scores = [0u8; MAX_WORD + 3];
    for start in 0..n + 2 {
        let mut node = 0usize;
        for &ch in &letters[start..n + 2] {
            let (first, count, _, _) = HUNGARIAN.nodes[node];
            let first = usize::try_from(first).expect("Hungarian edge offset");
            let count = usize::try_from(count).expect("Hungarian edge count");
            let branches = &HUNGARIAN.edges[first..][..count];
            let Ok(next) = branches.binary_search_by_key(&ch, |&(label, _)| label) else {
                break;
            };
            node = usize::try_from(branches[next].1).expect("Hungarian node index");
            let (_, _, index, len) = HUNGARIAN.nodes[node];
            let index = usize::try_from(index).expect("Hungarian weight offset");
            let len = usize::try_from(len).expect("Hungarian weight count");
            for (score, &weight) in scores[start..]
                .iter_mut()
                .zip(&HUNGARIAN.weights[index..][..len])
            {
                *score = (*score).max(weight);
            }
        }
    }
    for at in left..=n - right {
        let offset = offsets[at];
        if scores[at + 1] % 2 == 1 && !word[offset..].starts_with('\u{ad}') {
            emit(offset);
        }
    }
}

// Some pattern rules describe several Unicode scalars in a single grapheme.
// Match NFD scalars, but emit only original grapheme boundaries.
fn decomposed_pattern_breaks(
    patterns: &Patterns,
    word: &str,
    limits: HyphenationLimits,
    greek_modifiers: bool,
    malayalam_joiners: bool,
    mut emit: impl FnMut(usize),
) {
    const MAX_SCALARS: usize = MAX_WORD * 4;
    let mut letters = ['.'; MAX_SCALARS + 2];
    let mut boundaries = [None; MAX_SCALARS + 1];
    let mut ordinal = [0u8; MAX_SCALARS + 1];
    let mut scalars = 0usize;
    let mut graphemes = 0usize;
    let mut start = 0usize;
    let nfd = DecomposingNormalizer::new_nfd();
    let categories = CodePointMapData::<GeneralCategory>::new();
    for end in GraphemeClusterSegmenter::new().segment_str(word).skip(1) {
        let cluster = &word[start..end];
        start = end;
        if cluster == "\u{ad}" {
            continue;
        }
        let lower = cluster.to_lowercase();
        let normalized = nfd.normalize(&lower);
        let mut chars = normalized.chars();
        let Some(first) = chars.next() else {
            return;
        };
        let modifier = (greek_modifiers && matches!(first, '\u{1fbd}' | '\u{1fbf}'))
            || (malayalam_joiners && matches!(first, '\u{200c}' | '\u{200d}'));
        if modifier {
            if graphemes == 0 {
                return;
            }
            // The preceding grapheme owns this modifier; never wrap between them.
            boundaries[scalars] = None;
        } else if !first.is_alphabetic() || graphemes == MAX_WORD {
            return;
        }
        for ch in std::iter::once(first).chain(chars) {
            if !ch.is_alphabetic()
                && !(greek_modifiers && matches!(ch, '\u{1fbd}' | '\u{1fbf}'))
                && !(malayalam_joiners && matches!(ch, '\u{200c}' | '\u{200d}'))
                && !matches!(
                    categories.get(ch),
                    GeneralCategory::NonspacingMark
                        | GeneralCategory::SpacingMark
                        | GeneralCategory::EnclosingMark
                )
            {
                return;
            }
            if scalars == MAX_SCALARS {
                return;
            }
            scalars += 1;
            letters[scalars] = ch;
        }
        if !modifier {
            graphemes += 1;
        }
        boundaries[scalars] = Some(end);
        ordinal[scalars] = graphemes as u8;
    }
    let left = usize::from(limits.left);
    let right = usize::from(limits.right);
    if graphemes < left + right {
        return;
    }
    let mut scores = [0u8; MAX_SCALARS + 3];
    for start in 0..scalars + 2 {
        let mut node = 0;
        for &ch in &letters[start..scalars + 2] {
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
    for scalar in 1..=scalars {
        let Some(offset) = boundaries[scalar] else {
            continue;
        };
        let position = usize::from(ordinal[scalar]);
        if position >= left
            && graphemes - position >= right
            && scores[scalar + 1] % 2 == 1
            && !word[offset..].starts_with('\u{ad}')
        {
            emit(offset);
        }
    }
}

fn pattern_breaks(
    patterns: PatternView<'_>,
    additional: Option<PatternView<'_>>,
    word: &str,
    limits: HyphenationLimits,
    apostrophes: bool,
    turkish: bool,
    mut emit: impl FnMut(usize),
) {
    let mut letters = ['.'; MAX_WORD + 2];
    let mut offsets = [0; MAX_WORD + 1];
    let mut n = 0;
    if word.is_ascii() {
        if word.len() > MAX_WORD
            || !word
                .bytes()
                .all(|byte| byte.is_ascii_alphabetic() || (apostrophes && byte == b'\''))
        {
            return;
        }
        for (at, byte) in word.bytes().enumerate() {
            n += 1;
            letters[n] = if turkish && byte == b'I' {
                'ı'
            } else {
                char::from(byte.to_ascii_lowercase())
            };
            offsets[n] = at + 1;
        }
    } else {
        let nfc = ComposingNormalizer::new_nfc();
        let mut start = 0;
        for end in GraphemeClusterSegmenter::new().segment_str(word).skip(1) {
            let cluster = &word[start..end];
            start = end;
            if cluster == "\u{ad}" {
                continue;
            }
            let composed = nfc.normalize(cluster);
            let mut chars = composed.chars();
            let Some(character) = chars.next().filter(|ch| {
                ch.is_alphabetic() || (apostrophes && matches!(*ch, '\'' | '\u{2019}'))
            }) else {
                return;
            };
            let character = if character == '\u{2019}' {
                '\''
            } else if turkish && character == 'I' {
                'ı'
            } else if turkish && character == 'İ' {
                'i'
            } else {
                character
            };
            let mut lower = character.to_lowercase();
            let letter = lower.next().expect("a lowercase mapping");
            if n == MAX_WORD || chars.next().is_some() || lower.next().is_some() {
                return;
            }
            n += 1;
            letters[n] = letter;
            offsets[n] = end;
        }
    }
    let left = usize::from(limits.left);
    let right = usize::from(limits.right);
    if n < left + right {
        return;
    }
    let mut scores = [0u8; MAX_WORD + 3];
    // Composite Russian/English dictionaries combine the maximum pattern
    // weights before deciding odd/even breaks, rather than unioning results.
    for patterns in [Some(patterns), additional].into_iter().flatten() {
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
    }
    for at in left..=n - right {
        let offset = offsets[at];
        // The authored soft hyphen supplies this exact opportunity.
        if scores[at + 1] % 2 == 1 && !word[offset..].starts_with('\u{ad}') {
            emit(offset);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{book_style::BookStyle, source::Source};

    #[test]
    fn additional_dictionaries_respect_script_and_default_limits() {
        for (tag, dictionary, left, right) in [
            ("bg-Cyrl-BG", Dictionary::Bulgarian, 2, 2),
            ("gl-ES", Dictionary::Galician, 2, 2),
            ("ga-IE", Dictionary::Irish, 2, 3),
            ("uk-Cyrl-UA", Dictionary::Ukrainian, 2, 2),
            ("ru-Cyrl-RU", Dictionary::Russian, 2, 2),
        ] {
            let actual = Dictionary::for_language(Language::parse(tag).ok());
            assert_eq!(actual, Some(dictionary), "{tag}");
            assert_eq!(
                dictionary.limits(),
                HyphenationLimits { left, right },
                "{tag}"
            );
        }
        for tag in ["bg-Latn", "uk-Latn", "ru-Latn", "gl-Cyrl", "ga-Cyrl"] {
            assert_eq!(
                Dictionary::for_language(Language::parse(tag).ok()),
                None,
                "{tag}"
            );
        }
    }

    #[test]
    fn russian_exception_supplies_source_safe_breaks() {
        let word = "сегодня";
        let mut offsets = Vec::new();
        word_breaks(Dictionary::Russian, word, |offset| offsets.push(offset));
        assert!(offsets.contains(&"се".len()));
        assert!(offsets.contains(&"сего".len()));
        assert!(offsets.iter().all(|&offset| word.is_char_boundary(offset)));
        let mut limited = Vec::new();
        word_breaks_with_limits(
            Dictionary::Russian,
            word,
            HyphenationLimits { left: 4, right: 2 },
            |offset| limited.push(offset),
        );
        assert!(
            limited
                .iter()
                .all(|&offset| word[..offset].chars().count() >= 4)
        );
    }

    #[test]
    fn ukrainian_apostrophes_preserve_offsets_and_share_matching_rules() {
        let breaks = |word: &str| {
            let mut positions = Vec::new();
            word_breaks(Dictionary::Ukrainian, word, |offset| {
                assert!(word.is_char_boundary(offset));
                positions.push(word[..offset].chars().count());
            });
            positions
        };
        assert_eq!(breaks("підв'язувати"), breaks("підв’язувати"));
    }

    #[test]
    fn multilingual_hyphenation_rules_match_the_pinned_dictionaries() {
        // Fingerprints of sorted stock rules, with CRE word-edge spaces normalized to dots.
        for (rules, count, digest) in [
            (
                include_str!(concat!(env!("OUT_DIR"), "/es_rules.txt")),
                4692,
                "4c28ce4ab4490eb7a0c9049ee9a04186",
            ),
            (
                include_str!(concat!(env!("OUT_DIR"), "/it_rules.txt")),
                326,
                "5f6ed27de5d159dfdf60d9aa47826a34",
            ),
            (
                include_str!(concat!(env!("OUT_DIR"), "/pt_rules.txt")),
                308,
                "9bfdcde055dc8d8625a5e1a790e94067",
            ),
            (
                include_str!(concat!(env!("OUT_DIR"), "/et_rules.txt")),
                3691,
                "d4dbc5867827914d13b74b23e4a94762",
            ),
            (
                include_str!(concat!(env!("OUT_DIR"), "/fi_rules.txt")),
                284,
                "dc3f7225640585ff3ddcae30c90825c0",
            ),
            (
                include_str!(concat!(env!("OUT_DIR"), "/da_rules.txt")),
                1144,
                "e059248b00298eaa3edde6799e260c56",
            ),
            (
                include_str!(concat!(env!("OUT_DIR"), "/nl_rules.txt")),
                12764,
                "662e0ec1a5e3cf040ab3b74f327d6c3f",
            ),
        ] {
            assert_eq!(rules.lines().count(), count);
            assert_eq!(format!("{:x}", md5::compute(rules)), digest);
        }
    }

    #[test]
    fn multilingual_hyphenation_matches_native_word_boundaries_and_per_language_limits() {
        let global = HyphenationLimits { left: 4, right: 4 };
        let breaks = |dictionary, word: &str, limits| {
            let mut offsets = Vec::new();
            word_breaks_with_limits(dictionary, word, limits, |at| {
                assert!(word.is_char_boundary(at));
                offsets.push(word[..at].chars().count());
            });
            offsets
        };
        for (tag, word, ordinary, limited) in [
            ("es-ES", "información", &[2, 5, 7][..], &[5, 7][..]),
            ("es-ES", "lingüística", &[3, 7, 9], &[7]),
            ("es-MX", "murciélago", &[3, 6, 8], &[6]),
            ("it-IT", "straordinario", &[6, 8, 10], &[6, 8]),
            (
                "it-IT",
                "elettroencefalogramma",
                &[4, 9, 11, 13, 15, 19],
                &[4, 9, 11, 13, 15],
            ),
            ("it", "acqua", &[2], &[]),
            ("pt-PT", "informação", &[2, 5, 7], &[5]),
            ("pt", "linguística", &[3, 7], &[7]),
            ("pt-PT", "extraordinário", &[2, 5, 7, 9, 11], &[5, 7, 9]),
            ("pt-PT", "saúde", &[], &[]),
            (
                "et-EE",
                "õigekeelsussõnaraamat",
                &[2, 4, 8, 11, 13, 15, 18],
                &[4, 8, 11, 13, 15],
            ),
            ("et", "raudteejaam", &[4, 7], &[4, 7]),
            ("et-Latn-EE", "lühiülevaade", &[2, 4, 7], &[4, 7]),
            (
                "et-EE",
                "päikesepaisteline",
                &[3, 5, 7, 11, 13],
                &[5, 7, 11, 13],
            ),
            (
                "fi-FI",
                "työskenteleminen",
                &[4, 7, 9, 11, 13],
                &[4, 7, 9, 11],
            ),
            ("fi", "ääntäminen", &[3, 5, 7], &[5]),
            ("fi-Latn-FI", "kansainvälinen", &[3, 7, 9, 11], &[7, 9]),
            ("fi-FI", "taloudellinen", &[2, 5, 8, 10], &[5, 8]),
            ("fi-FI", "kalastus", &[2, 5], &[]),
            ("da-DK", "LÆSEOPLEVELSE", &[2, 4, 6, 8, 11], &[4, 6, 8]),
            ("da-DK", "bibliotekernes", &[2, 5, 6, 8, 11], &[5, 6, 8]),
            (
                "da-DK",
                "samarbejdsmuligheder",
                &[3, 5, 10, 12, 15, 17],
                &[5, 10, 12, 15],
            ),
            ("da", "påmindelser", &[2, 5, 8], &[5]),
            (
                "da-Latn-DK",
                "færgeforbindelse",
                &[3, 5, 8, 11, 14],
                &[5, 8, 11],
            ),
            ("nl-NL", "coördinatie", &[2, 4, 6, 8], &[4, 6]),
            ("nl", "bibliotheek", &[2, 5, 6], &[5, 6]),
            ("nl-BE", "schilderstalent", &[5, 8, 9, 11], &[5, 8, 9, 11]),
            (
                "nl-NL",
                "waterstaatsingenieur",
                &[2, 5, 10, 11, 13, 15, 17],
                &[5, 10, 11, 13, 15],
            ),
            ("nl-NL", "maatschappij", &[4, 9], &[4]),
        ] {
            let dictionary = Dictionary::for_language(Language::parse(tag).ok()).unwrap();
            assert_eq!(
                breaks(dictionary, word, dictionary.limits()),
                ordinary,
                "{tag} {word}"
            );
            assert_eq!(breaks(dictionary, word, global), limited, "{tag} {word}");
        }
    }

    #[test]
    fn multilingual_hyphenation_keeps_original_utf8_offsets_and_respects_embedded_languages() {
        let source = Source::parse(
            "<html lang='es'><body><p>murcie\u{301}lago <span lang='it'>l’<b>ital</b>iano</span> \
             <span lang='pt-PT'>infor<b>ma</b>ção</span> \
             <span lang='pt-BR'>informação</span> \
             <span lang='et-EE'>lu\u{308}hi<b>ülevaa</b>de</span> \
             <span lang='fi-FI'>a\u{308}äntäminen</span> \
             <span lang='da-DK'>pa\u{30a}min<b>del</b>ser</span> \
             <span lang='nl-NL'>coo\u{308}rdinatie</span> \
             <span style='hyphens:manual'>murciélago</span> \
             <span xml:lang='es-Cyrl'>murciélago</span></p></body></html>",
        )
        .unwrap();
        let mut book =
            crate::epub::Book::open(&crate::test_fixture("field-notes-epub3.epub")).unwrap();
        let chapter = book.chapters[0].href.clone();
        let mut fonts =
            crate::font_context(&[crate::test_fixture("fonts/NotoSans-Regular.ttf")]).unwrap();
        let styles =
            BookStyle::load(&mut book, &chapter, &source, &mut fonts).compute(&source.elements);
        let block = &source.blocks[0];
        let text = &block.text;
        let parts = [
            (
                "murcie\u{301}lago",
                &["mur", "murcie\u{301}", "murcie\u{301}la"][..],
            ),
            ("l’italiano", &["l’ita", "l’italia"]),
            ("informação", &["in", "infor", "informa"]),
            ("informação", &["in", "infor", "informa"]),
            (
                "lu\u{308}hiülevaade",
                &["lu\u{308}", "lu\u{308}hi", "lu\u{308}hiüle"],
            ),
            (
                "a\u{308}äntäminen",
                &["a\u{308}än", "a\u{308}äntä", "a\u{308}äntämi"],
            ),
            (
                "pa\u{30a}mindelser",
                &["pa\u{30a}", "pa\u{30a}min", "pa\u{30a}mindel"],
            ),
            (
                "coo\u{308}rdinatie",
                &["co", "coo\u{308}r", "coo\u{308}rdi", "coo\u{308}rdina"],
            ),
        ];
        let mut expected = Vec::new();
        let mut from = 0;
        for (word, prefixes) in parts {
            let at = text[from..].find(word).unwrap() + from;
            expected.extend(prefixes.iter().map(|prefix| at + prefix.len()));
            from = at + word.len();
        }
        assert_eq!(opportunities(block, &styles, Settings::default()), expected);
        assert!(
            opportunities(
                block,
                &styles,
                Settings {
                    automatic_hyphenation: false,
                    ..Settings::default()
                },
            )
            .is_empty()
        );
    }

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
            word_breaks(Dictionary::American, word, |b| actual.push(b));
            assert_eq!(actual, expected);
        }
        for word in ["short", "documentation", "transformation"] {
            word_breaks(Dictionary::American, word, |b| {
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
                Dictionary::American,
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
                Dictionary::British,
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
            .zip([&[3, 4, 6][..], &[5, 7], &[5, 7], &[], &[3, 4, 6], &[]])
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
        for word in ["", "a", &"hyphenation".repeat(6)] {
            word_breaks(Dictionary::American, word, |_| {
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
    fn package_language_css_and_authored_points_preserve_all_eligible_break_offsets() {
        let xhtml = "<html><body><p>hyphenation</p><p lang='en-GB'>hyphenation</p><p lang=''>hyphenation</p><p lang='vi'>hyphenation</p><p style='hyphens: manual'>hyphenation</p><p>hy<span style='hyphens: none'>phen</span>ation</p><p lang='en-US'>hy&#173;phenation</p><p lang='en-US'>hy<span>phen</span>ation</p><p lang='en-Latn-US'>hyphenation</p><p lang='en'>internationalé</p></body></html>";
        let src = Source::parse_with_language(xhtml, Some(Language::parse("en").unwrap())).unwrap();
        let mut book =
            crate::epub::Book::open(&crate::test_fixture("field-notes-epub3.epub")).unwrap();
        let chapter = book.chapters[0].href.clone();
        let mut fonts =
            crate::font_context(&[crate::test_fixture("fonts/NotoSans-Regular.ttf")]).unwrap();
        let styles = BookStyle::load(&mut book, &chapter, &src, &mut fonts).compute(&src.elements);
        let active = Settings::default();
        for (i, block) in src.blocks.iter().enumerate() {
            let actual = opportunities(block, &styles, active);
            if [0, 1, 7, 8].contains(&i) {
                assert_eq!(actual, [2, 6, 7], "block {i}");
            } else if i == 6 {
                // The authored break at hy- stays separate. Dictionary
                // candidates after it still map to the original UTF-8 bytes.
                assert_eq!(actual, [8, 9]);
            } else if i == 9 {
                // Accented Latin letters are eligible; every break must still
                // refer to an original UTF-8 character boundary.
                assert!(
                    actual
                        .iter()
                        .all(|&offset| block.text.is_char_boundary(offset))
                );
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
