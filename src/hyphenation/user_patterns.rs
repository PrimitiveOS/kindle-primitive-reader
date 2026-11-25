//! Optional user-supplied hyphenation patterns, treated as inert data.
use super::PatternView;
use icu_properties::{CodePointMapData, props::Script};
use std::collections::BTreeMap;

pub(crate) const MAX_FILE_BYTES: u64 = 256 * 1024;
const MAX_RULES: usize = 4096;
const MAX_NODES: usize = 8192;
const MAX_PATTERN_BYTES: usize = 128;
const MAX_WEIGHTS: usize = u16::MAX as usize;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct OwnedPatterns {
    nodes: Vec<(u16, u8, u16, u8)>,
    edges: Vec<(char, u16)>,
    weights: Vec<u8>,
}

#[derive(Clone, Copy)]
enum Alphabet {
    Armenian,
    Zulu,
    SerbianLatin,
    SerbianCyrillic,
    Czech,
}

#[derive(Default)]
struct Node {
    edges: BTreeMap<char, usize>,
    weights: Vec<u8>,
}

impl OwnedPatterns {
    pub(crate) fn view(&self) -> PatternView<'_> {
        PatternView {
            nodes: &self.nodes,
            edges: &self.edges,
            weights: &self.weights,
        }
    }

    pub(crate) fn parse(bytes: &[u8]) -> Option<Self> {
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return None;
        }
        let input = std::str::from_utf8(bytes).ok()?;
        let cleaned = input
            .lines()
            .map(|line| line.split('%').next().unwrap_or(""))
            .collect::<Vec<_>>()
            .join("\n");
        let (prefix, tail) = cleaned.split_once("\\patterns{")?;
        let (group, suffix) = tail.split_once('}')?;
        if !prefix.trim().is_empty() || !suffix.trim().is_empty() {
            return None;
        }
        Self::parse_rules(group.split_whitespace(), Alphabet::Armenian)
    }

    /// Accept the upstream ASCII-compatible ISO-8859-1 dictionary syntax.
    /// KOReader omits the original apostrophe-only rule, so omit it here too.
    pub(crate) fn parse_zulu(bytes: &[u8]) -> Option<Self> {
        if bytes.len() as u64 > MAX_FILE_BYTES || !bytes.is_ascii() {
            return None;
        }
        let input = std::str::from_utf8(bytes).ok()?;
        let mut lines = input
            .lines()
            .map(|line| line.split('%').next().unwrap_or("").trim())
            .filter(|line| !line.is_empty());
        if lines.next()? != "ISO8859-1" {
            return None;
        }
        Self::parse_rules(lines.filter(|line| *line != "'1"), Alphabet::Zulu)
    }

    /// Czech TeX contains weighted patterns and explicit word exceptions.
    /// KOReader compiles the exceptions as weight-5 patterns.
    pub(crate) fn parse_czech(bytes: &[u8]) -> Option<Self> {
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return None;
        }
        let input = std::str::from_utf8(bytes).ok()?;
        let cleaned = input
            .lines()
            .map(|line| line.split('%').next().unwrap_or(""))
            .collect::<Vec<_>>()
            .join("\n");
        let (prefix, tail) = cleaned.split_once("\\patterns{")?;
        let (patterns, tail) = tail.split_once('}')?;
        let (between, tail) = tail.split_once("\\hyphenation{")?;
        let (exceptions, suffix) = tail.split_once('}')?;
        if !prefix.trim().is_empty() || !between.trim().is_empty() || !suffix.trim().is_empty() {
            return None;
        }
        let mut rules = Vec::new();
        for pattern in patterns.split_whitespace() {
            if rules.len() >= MAX_RULES || pattern.len() > MAX_PATTERN_BYTES {
                return None;
            }
            rules.push(pattern.to_owned());
        }
        let scripts = CodePointMapData::<Script>::new();
        for exception in exceptions.split_whitespace() {
            if rules.len() >= MAX_RULES
                || exception.len() > MAX_PATTERN_BYTES
                || exception.starts_with('-')
                || exception.ends_with('-')
                || exception.contains("--")
                || !exception.contains('-')
                || !exception
                    .chars()
                    .all(|ch| ch == '-' || (ch.is_lowercase() && scripts.get(ch) == Script::Latin))
            {
                return None;
            }
            rules.push(exception.replace('-', "5"));
        }
        Self::parse_rules(rules.iter().map(String::as_str), Alphabet::Czech)
    }

    /// Serbian sources are strict ISO-8859-2 (Latin) or ISO-8859-5 (Cyrillic).
    /// KOReader changes the Cyrillic `д2р` pattern to `д3р`.
    pub(crate) fn parse_serbian(bytes: &[u8], latin: bool) -> Option<Self> {
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return None;
        }
        let encoding = if latin {
            encoding_rs::ISO_8859_2
        } else {
            encoding_rs::ISO_8859_5
        };
        let (input, errors) = encoding.decode_without_bom_handling(bytes);
        if errors {
            return None;
        }
        let mut lines = input
            .lines()
            .map(|line| line.split('%').next().unwrap_or("").trim())
            .filter(|line| !line.is_empty());
        if lines.next()? != (if latin { "ISO8859-2" } else { "ISO8859-5" })
            || lines.next()? != "LEFTHYPHENMIN 2"
            || lines.next()? != "RIGHTHYPHENMIN 2"
        {
            return None;
        }
        let alphabet = if latin {
            Alphabet::SerbianLatin
        } else {
            Alphabet::SerbianCyrillic
        };
        Self::parse_rules(
            lines.map(|line| {
                if !latin && line == "д2р" {
                    "д3р"
                } else {
                    line
                }
            }),
            alphabet,
        )
    }

    fn parse_rules<'a>(rules: impl Iterator<Item = &'a str>, alphabet: Alphabet) -> Option<Self> {
        let scripts = CodePointMapData::<Script>::new();
        let mut nodes = vec![Node::default()];
        let mut count = 0usize;
        for pattern in rules {
            count += 1;
            if count > MAX_RULES || pattern.len() > MAX_PATTERN_BYTES {
                return None;
            }
            let mut at = 0usize;
            let mut weights = vec![0u8];
            let mut letters = String::new();
            let mut seen_digit = false;
            let mut previous_digit = false;
            for ch in pattern.chars() {
                if ch.is_ascii_digit() {
                    if previous_digit {
                        return None;
                    }
                    *weights.last_mut()? = ch as u8 - b'0';
                    seen_digit = true;
                    previous_digit = true;
                    continue;
                }
                previous_digit = false;
                let letter = match alphabet {
                    Alphabet::Zulu => ch.is_ascii_lowercase(),
                    Alphabet::Armenian => {
                        ('\u{0561}'..='\u{0587}').contains(&ch) && ch.is_lowercase()
                    }
                    Alphabet::SerbianLatin | Alphabet::Czech => {
                        ch.is_lowercase() && scripts.get(ch) == Script::Latin
                    }
                    Alphabet::SerbianCyrillic => {
                        ch.is_lowercase() && scripts.get(ch) == Script::Cyrillic
                    }
                };
                if ch != '.' && !letter {
                    return None;
                }
                letters.push(ch);
                let next = nodes.len();
                let index = *nodes.get_mut(at)?.edges.entry(ch).or_insert(next);
                if index == next {
                    if nodes.len() >= MAX_NODES {
                        return None;
                    }
                    nodes.push(Node::default());
                }
                at = index;
                weights.push(0);
            }
            let inner = letters.trim_matches('.');
            if !seen_digit
                || inner.is_empty()
                || inner.contains('.')
                || letters.len() - inner.len() > 2
                || letters.starts_with("..")
                || letters.ends_with("..")
            {
                return None;
            }
            let existing = &mut nodes.get_mut(at)?.weights;
            if existing.is_empty() {
                *existing = weights;
            } else {
                if existing.len() != weights.len() {
                    return None;
                }
                for (old, new) in existing.iter_mut().zip(weights) {
                    *old = (*old).max(new);
                }
            }
        }
        if count == 0 {
            return None;
        }
        let mut result = Self::default();
        let mut offsets = BTreeMap::new();
        for node in &nodes {
            let offset = if let Some(offset) = offsets.get(node.weights.as_slice()) {
                *offset
            } else {
                if result.weights.len().checked_add(node.weights.len())? > MAX_WEIGHTS {
                    return None;
                }
                let offset = u16::try_from(result.weights.len()).ok()?;
                result.weights.extend_from_slice(&node.weights);
                offsets.insert(node.weights.as_slice(), offset);
                offset
            };
            result.nodes.push((
                u16::try_from(result.edges.len()).ok()?,
                u8::try_from(node.edges.len()).ok()?,
                offset,
                u8::try_from(node.weights.len()).ok()?,
            ));
            for (&ch, &target) in &node.edges {
                result.edges.push((ch, u16::try_from(target).ok()?));
            }
        }
        Some(result)
    }
}
