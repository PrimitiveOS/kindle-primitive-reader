//! Reading-language choices and the engine settings applied to embedded tags.
use parley::Language;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TypographyLanguages {
    pub main: Option<TypographyLocale>,
    pub fallback: Option<TypographyLocale>,
    pub respect_embedded: bool,
}

impl Default for TypographyLanguages {
    fn default() -> Self {
        Self {
            main: None,
            fallback: None,
            respect_embedded: true,
        }
    }
}

impl TypographyLanguages {
    pub(crate) fn resolve(self, publication: Option<TypographyLocale>) -> Option<TypographyLocale> {
        self.main.or(publication).or(self.fallback)
    }
}

/// A shaping locale, preserving KOReader's distinct liturgical Latin setting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypographyLocale {
    Language(Language),
    LiturgicalLatin,
}

impl TypographyLocale {
    pub fn parse(tag: &str) -> Option<Self> {
        if tag.eq_ignore_ascii_case("la-lit")
            || tag.eq_ignore_ascii_case("lat-lit")
            || tag.eq_ignore_ascii_case("la-x-liturgic")
        {
            return Some(Self::LiturgicalLatin);
        }
        let language = Language::parse(tag).ok()?;
        if language.language() == "lat" {
            return Some(Self::Language(Language::parse("la").ok()?));
        }
        Some(Self::Language(language))
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Language(language) => language.as_str(),
            Self::LiturgicalLatin => "la-lit",
        }
    }

    pub(crate) fn shaping_language(self) -> Language {
        match self {
            Self::Language(language) => language,
            Self::LiturgicalLatin => Language::parse("la").expect("valid Latin language tag"),
        }
    }

    pub(crate) fn publication(language: Language) -> Self {
        if language.as_str().eq_ignore_ascii_case("la-x-liturgic") {
            return Self::LiturgicalLatin;
        }
        CATALOG
            .iter()
            .find(|entry| {
                entry
                    .aliases
                    .iter()
                    .any(|tag| tag.eq_ignore_ascii_case(language.as_str()))
            })
            .and_then(|entry| Self::parse(entry.tag))
            .unwrap_or(Self::Language(language))
    }
}

#[cfg(feature = "kindle")]
pub(crate) fn label(language: Option<TypographyLocale>) -> String {
    language.map_or_else(
        || "Unavailable".to_owned(),
        |language| {
            CATALOG
                .iter()
                .find(|entry| entry.tag.eq_ignore_ascii_case(language.as_str()))
                .map_or(language.as_str(), |entry| entry.title)
                .to_owned()
        },
    )
}

#[cfg_attr(not(feature = "kindle"), allow(dead_code))]
pub(crate) struct Entry {
    pub tag: &'static str,
    pub title: &'static str,
    aliases: &'static [&'static str],
}

// The display names and codes from the pinned typography menu.
#[rustfmt::skip]
pub(crate) const CATALOG: [Entry; 53] = [
    Entry { tag: "hy", title: "Armenian", aliases: &["arm", "hye", "hyw"] },
    Entry { tag: "eu", title: "Basque", aliases: &[] },
    Entry { tag: "bg", title: "Bulgarian", aliases: &["bul"] },
    Entry { tag: "ca", title: "Catalan", aliases: &["cat"] },
    Entry { tag: "zh-CN", title: "Chinese (Simplified)", aliases: &["zh", "zh-Hans"] },
    Entry { tag: "zh-TW", title: "Chinese (Traditional)", aliases: &["zh-Hant"] },
    Entry { tag: "hr", title: "Croatian", aliases: &[] },
    Entry { tag: "cs", title: "Czech", aliases: &["ces"] },
    Entry { tag: "da", title: "Danish", aliases: &["dan"] },
    Entry { tag: "nl", title: "Dutch", aliases: &["nld"] },
    Entry { tag: "en-GB", title: "English (UK)", aliases: &[] },
    Entry { tag: "en-US", title: "English (US)", aliases: &["en", "eng"] },
    Entry { tag: "eo", title: "Esperanto", aliases: &["epo"] },
    Entry { tag: "et", title: "Estonian", aliases: &["est"] },
    Entry { tag: "fi", title: "Finnish", aliases: &["fin"] },
    Entry { tag: "fr", title: "French", aliases: &["fra", "fre"] },
    Entry { tag: "fur", title: "Friulian", aliases: &[] },
    Entry { tag: "gl", title: "Galician", aliases: &["glg"] },
    Entry { tag: "ka", title: "Georgian", aliases: &[] },
    Entry { tag: "de", title: "German", aliases: &["deu"] },
    Entry { tag: "el", title: "Greek", aliases: &["ell"] },
    Entry { tag: "hu", title: "Hungarian", aliases: &["hun"] },
    Entry { tag: "is", title: "Icelandic", aliases: &["isl"] },
    Entry { tag: "ga", title: "Irish", aliases: &["gle"] },
    Entry { tag: "it", title: "Italian", aliases: &["ita"] },
    Entry { tag: "ja", title: "Japanese", aliases: &[] },
    Entry { tag: "ko", title: "Korean", aliases: &[] },
    Entry { tag: "la", title: "Latin", aliases: &["lat"] },
    Entry { tag: "la-lit", title: "Latin (liturgical)", aliases: &["lat-lit"] },
    Entry { tag: "lv", title: "Latvian", aliases: &["lav"] },
    Entry { tag: "lt", title: "Lithuanian", aliases: &["lit"] },
    Entry { tag: "mk", title: "Macedonian", aliases: &[] },
    Entry { tag: "ml", title: "Malayalam", aliases: &["mal"] },
    Entry { tag: "no", title: "Norwegian", aliases: &["nor"] },
    Entry { tag: "oc", title: "Occitan", aliases: &["oci"] },
    Entry { tag: "pl", title: "Polish", aliases: &["pol"] },
    Entry { tag: "pms", title: "Piedmontese", aliases: &[] },
    Entry { tag: "pt", title: "Portuguese", aliases: &["por"] },
    Entry { tag: "pt-BR", title: "Portuguese (BR)", aliases: &[] },
    Entry { tag: "rm", title: "Romansh", aliases: &["roh"] },
    Entry { tag: "ro", title: "Romanian", aliases: &["ron"] },
    Entry { tag: "ru", title: "Russian", aliases: &["rus"] },
    Entry { tag: "ru-GB", title: "Russian + English (UK)", aliases: &[] },
    Entry { tag: "ru-US", title: "Russian + English (US)", aliases: &[] },
    Entry { tag: "sr", title: "Serbian", aliases: &["srp"] },
    Entry { tag: "sk", title: "Slovak", aliases: &["slk"] },
    Entry { tag: "sl", title: "Slovenian", aliases: &["slv"] },
    Entry { tag: "es", title: "Spanish", aliases: &["spa"] },
    Entry { tag: "sv", title: "Swedish", aliases: &["swe"] },
    Entry { tag: "tr", title: "Turkish", aliases: &["tur"] },
    Entry { tag: "uk", title: "Ukrainian", aliases: &["ukr"] },
    Entry { tag: "cy", title: "Welsh", aliases: &["cym"] },
    Entry { tag: "zu", title: "Zulu", aliases: &["zul"] },
];
