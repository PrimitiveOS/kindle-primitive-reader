//! Fixed-page search works on the provider's original word boxes. Folded text is
//! used only for comparison, so every result points back into the unmodified page.

use super::{MAX_QUERY_BYTES, accent_mark, phrase_text};
use crate::{Result, fixed_text::PdfPageText};
use icu_casemap::CaseMapper;
use icu_normalizer::{ComposingNormalizer, DecomposingNormalizer};
use icu_properties::{CodePointSetData, props::DefaultIgnorableCodePoint};
use std::{borrow::Cow, collections::VecDeque, ops::Range};

pub(super) struct FixedPattern {
    tokens: Vec<String>,
    match_case: bool,
    ignore_accents: bool,
    from_start: bool,
    from_end: bool,
    split_cjk: bool,
}

fn cjk(c: char) -> bool {
    matches!(c as u32, 0x1100..=0x11ff | 0x2e80..=0x9fff | 0xa960..=0xa97f
        | 0xac00..=0xd7ff | 0xf900..=0xfaff | 0xfe30..=0xfe4f | 0xff00..=0xffef
        | 0x20000..=0x2a6df | 0x2a700..=0x2ebef | 0x2f800..=0x2fa1f | 0x30000..=0x3134f)
}

/// KOpt preserves ASCII runs in a CJK token and splits its multibyte code points.
fn segments(word: &str, split: bool) -> impl Iterator<Item = (usize, &str)> {
    let mut cursor = 0;
    std::iter::from_fn(move || {
        if cursor == word.len() {
            return None;
        }
        let start = cursor;
        let tail = &word[start..];
        cursor += if !split {
            tail.len()
        } else if tail.as_bytes()[0].is_ascii() {
            tail.bytes().take_while(u8::is_ascii).count()
        } else {
            tail.chars().next()?.len_utf8()
        };
        Some((start, &word[start..cursor]))
    })
}

fn normalize(word: &str, match_case: bool, ignore_accents: bool) -> Cow<'_, str> {
    if word.is_ascii() {
        return if !match_case && word.bytes().any(|b| b.is_ascii_uppercase()) {
            Cow::Owned(word.to_ascii_lowercase())
        } else {
            Cow::Borrowed(word)
        };
    }
    let folded = if match_case {
        Cow::Borrowed(word)
    } else {
        let nfkc = ComposingNormalizer::new_nfkc();
        let compatible = nfkc.normalize(word);
        let case = CaseMapper::new().fold_string(&compatible);
        let ignorable = CodePointSetData::new::<DefaultIgnorableCodePoint>();
        let clean = case.chars().filter(|&c| !ignorable.contains(c));
        Cow::Owned(nfkc.normalize_iter(clean).collect())
    };
    if ignore_accents {
        Cow::Owned(
            DecomposingNormalizer::new_nfd()
                .normalize_iter(folded.chars())
                .filter(|&c| !accent_mark(c))
                .collect(),
        )
    } else {
        folded
    }
}

impl FixedPattern {
    pub(super) fn new(pattern: &str, match_case: bool, ignore_accents: bool) -> Result<Self> {
        if pattern.len() > MAX_QUERY_BYTES {
            return Err("Enter a search term of up to 256 UTF-8 bytes".into());
        }
        let mut tokens = Vec::new();
        let mut split_cjk = false;
        // Stock splits the input before normalization using Lua's %s (ASCII
        // whitespace). Compatibility spaces inside a word are part of that token.
        for word in pattern
            .split(|c: char| c.is_ascii_whitespace() || c == '\u{b}')
            .filter(|w| !w.is_empty())
        {
            let split = word.chars().any(cjk);
            split_cjk |= split;
            for (_, part) in segments(word, split) {
                let normalized = normalize(part, match_case, ignore_accents);
                if normalized.is_empty() {
                    return Err("Enter at least one searchable character".into());
                }
                tokens.push(normalized.into_owned());
            }
        }
        if tokens.is_empty() {
            return Err("Enter a search term of up to 256 UTF-8 bytes".into());
        }
        Ok(Self {
            tokens,
            match_case,
            ignore_accents,
            from_start: pattern.starts_with(' '),
            from_end: pattern.ends_with(' '),
            split_cjk,
        })
    }

    fn accepts(&self, word: &str, index: usize) -> bool {
        let term = &self.tokens[index];
        if self.tokens.len() == 1 {
            match (self.from_start, self.from_end) {
                (true, true) => word == term,
                (true, false) => word.starts_with(term),
                (false, true) => word.ends_with(term),
                (false, false) => word.contains(term),
            }
        } else if index == 0 {
            word.ends_with(term)
        } else if index + 1 == self.tokens.len() {
            word.starts_with(term)
        } else {
            word == term
        }
    }

    pub(super) fn ranges(&self, page: &PdfPageText, limit: usize) -> Vec<Range<usize>> {
        if limit == 0 {
            return Vec::new();
        }
        let separators = if self.tokens.len() > 1 {
            phrase_text(page)
        } else {
            Cow::Borrowed(page.text.as_str())
        };
        let mut found = Vec::new();
        let mut window = VecDeque::with_capacity(self.tokens.len());
        let mut previous = 0;
        let mut visit = |range: Range<usize>| {
            let Some(original) = page.text.get(range.clone()) else {
                return false;
            };
            if separators
                .get(previous..range.start)
                .is_none_or(|s| s.contains('\n'))
            {
                window.clear();
            }
            previous = range.end;
            // Preserve a provider's actual boxes. Only a transcript without
            // geometry needs to synthesize CJK tokens for phrase matching.
            let split = page.words.is_empty() && self.split_cjk && original.chars().any(cjk);
            for (offset, part) in segments(original, split) {
                let start = range.start + offset;
                window.push_back((start, normalize(part, self.match_case, self.ignore_accents)));
                if window.len() == self.tokens.len() {
                    if window
                        .iter()
                        .enumerate()
                        .all(|(i, (_, w))| self.accepts(w, i))
                    {
                        found.push(window[0].0..start + part.len());
                        if found.len() == limit {
                            return true;
                        }
                    }
                    window.pop_front();
                }
            }
            false
        };
        if page.words.is_empty() {
            // A DjVu transcript can omit its word boxes. Keep it searchable in Find all.
            let mut offset = 0;
            for part in page.text.split_inclusive(char::is_whitespace) {
                let start = offset;
                offset += part.len();
                let word = part.trim_end();
                if !word.is_empty() && visit(start..start + word.len()) {
                    break;
                }
            }
        } else {
            for word in &page.words {
                if visit(word.bytes.clone()) {
                    break;
                }
            }
        }
        found
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixed_text::{PdfTextRect, PdfTextSpan};

    #[test]
    fn matches_compatible_words_and_phrases_at_the_original_byte_ranges() {
        let mut page = PdfPageText::default();
        for (i, word) in [
            "Stop…",
            "Cafe\u{301}",
            "ＦＩＥＬＤ",
            "ﬁsh",
            "①",
            "ﬃ",
            "soft\u{ad}word",
            "…",
            "A\u{a0}B",
        ]
        .iter()
        .enumerate()
        {
            if i > 0 {
                page.text.push(' ');
            }
            let start = page.text.len();
            page.text.push_str(word);
            let x = i as f32 / 12.0;
            page.words.push(PdfTextSpan {
                bytes: start..page.text.len(),
                bounds: PdfTextRect {
                    x0: x,
                    y0: 0.1,
                    x1: x + 0.07,
                    y1: 0.15,
                },
            });
        }
        for (pattern, first, last) in [
            ("...", 0, 0),
            ("stop... café field FI", 0, 3),
            ("1 ffi softword", 4, 6),
            (" ... ", 7, 7),
            ("…", 0, 0),
            ("a\u{a0}b", 8, 8),
        ] {
            let m = FixedPattern::new(pattern, false, false)
                .unwrap()
                .ranges(&page, 1);
            assert_eq!(
                m,
                [page.words[first].bytes.start..page.words[last].bytes.end],
                "{pattern}"
            );
            assert_eq!(
                &page.text[m[0].clone()],
                &page.text[page.words[first].bytes.start..page.words[last].bytes.end]
            );
        }
        for pattern in ["...", "CAFÉ", "field", "fish", "1", "ffi", "softword"] {
            assert!(
                FixedPattern::new(pattern, true, false)
                    .unwrap()
                    .ranges(&page, 10)
                    .is_empty(),
                "{pattern}"
            );
        }
        assert_eq!(
            FixedPattern::new("stop... cafe", false, true)
                .unwrap()
                .ranges(&page, 2),
            [page.words[0].bytes.start..page.words[1].bytes.end]
        );
        assert_eq!(
            FixedPattern::new("…", true, false)
                .unwrap()
                .ranges(&page, 5)
                .len(),
            2
        );
        assert!(FixedPattern::new("\u{ad}", false, false).is_err());
    }

    #[test]
    #[ignore = "requires Unicode's DerivedNormalizationProps.txt"]
    fn fixed_nfkc_casefold_matches_independent_unicode_mapping() {
        use std::{collections::BTreeMap, fs};
        let data =
            fs::read_to_string(std::env::var_os("READER_TEST_UNICODE_UCD").unwrap()).unwrap();
        let mut changed = BTreeMap::new();
        for line in data.lines() {
            let p: Vec<_> = line
                .split('#')
                .next()
                .unwrap()
                .split(';')
                .map(str::trim)
                .collect();
            if p.get(1) != Some(&"NFKC_CF") {
                continue;
            }
            let bounds: Vec<_> = p[0]
                .split("..")
                .map(|v| u32::from_str_radix(v, 16).unwrap())
                .collect();
            let expected: String = p[2]
                .split_whitespace()
                .map(|v| char::from_u32(u32::from_str_radix(v, 16).unwrap()).unwrap())
                .collect();
            for cp in bounds[0]..=*bounds.last().unwrap() {
                changed.insert(cp, expected.clone());
            }
        }
        assert!(changed.len() > 5000);
        let mut buf = [0u8; 4];
        let mut wrong = Vec::new();
        for cp in 0..=0x10ffff {
            let Some(c) = char::from_u32(cp) else {
                continue;
            };
            let s = c.encode_utf8(&mut buf);
            if normalize(s, false, false) != changed.get(&cp).map_or(s.as_ref(), String::as_str) {
                wrong.push(cp);
            }
        }
        assert!(
            wrong.is_empty(),
            "{} scalar mappings differ; first {:?} (decimal)",
            wrong.len(),
            &wrong[..wrong.len().min(12)]
        );
    }

    #[test]
    #[ignore = "requires an original PDF and an independent case reference"]
    fn fixed_unicode_original_preserves_word_boxes_and_offsets() {
        use std::{env, fs};
        #[derive(serde::Deserialize)]
        struct Case {
            page: usize,
            query: String,
            expected: [usize; 2],
        }
        let p = env::var_os("READER_TEST_UNICODE_BOOK").unwrap();
        let cases: Vec<Case> = serde_json::from_slice(
            &fs::read(env::var_os("READER_TEST_UNICODE_CASES").unwrap()).unwrap(),
        )
        .unwrap();
        let pdf = crate::fixed::Pdf::open(fs::read(p).unwrap()).unwrap();
        let mut result = Vec::new();
        for case in cases {
            let page = pdf.page_text(case.page - 1).unwrap();
            for (sensitive, expected) in [false, true].into_iter().zip(case.expected) {
                let mut search =
                    crate::search::PdfSearch::with_options(&case.query, sensitive, false).unwrap();
                let exact = search.ranges(&page, 100);
                assert_eq!(exact.len(), expected);
                let listed = search.hits(case.page - 1, &page, 100);
                let mut boxes = Vec::new();
                let mut bytes = Vec::new();
                for (range, hit) in exact.iter().zip(listed) {
                    let source = page
                        .words
                        .iter()
                        .find(|w| w.bytes == *range)
                        .expect("match selects an intact original word");
                    assert!(page.text.get(range.clone()).is_some());
                    assert_eq!(hit.bounds, [source.bounds]);
                    let b = source.bounds;
                    boxes.push([b.x0, b.y0, b.x1, b.y1]);
                    bytes.push([range.start, range.end]);
                }
                let overlays = search.page_match(&page, crate::search::SearchDirection::Next);
                assert_eq!(overlays.map_or(0, |result| result.regions.len()), expected);
                result.push(serde_json::json!({"page":case.page,"case":sensitive,"boxes":boxes,"bytes":bytes}));
            }
        }
        fs::write(
            env::var_os("READER_TEST_UNICODE_OUTPUT").unwrap(),
            serde_json::to_vec(&result).unwrap(),
        )
        .unwrap();
    }
}
