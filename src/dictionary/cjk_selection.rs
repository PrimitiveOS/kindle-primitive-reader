//! One bounded dictionary-index pass for an automatic CJK word selection.
#[cfg(feature = "kindle")]
use super::*;
#[cfg(feature = "kindle")]
use std::sync::atomic::{AtomicBool, Ordering};

pub(crate) const MAX_SCAN_CHARACTERS: usize = 20;
#[cfg(feature = "kindle")]
const MAX_SELECTION_TERMS: usize = 4096;

pub(crate) fn word_character(c: char) -> bool {
    matches!(c, '\u{3041}'..='\u{3096}' | '\u{3099}'..='\u{309f}' |
        '\u{30a1}'..='\u{30fa}' | '\u{30fc}'..='\u{30ff}' |
        '\u{ff66}'..='\u{ff9f}' | '\u{fe00}'..='\u{fe0f}' | '\u{e0100}'..='\u{e01ef}' | '\u{3400}'..='\u{4dbf}' |
        '\u{4e00}'..='\u{9fff}' | '\u{f900}'..='\u{faff}' |
        '\u{20000}'..='\u{323af}' | '々' | '〆' | '〇' | '〻')
}

#[cfg(feature = "kindle")]
impl DictionarySources {
    /// `text` starts at the original word's source anchor. Prefix rank is
    /// longest-first, independent of dictionary order and grammatical ambiguity.
    /// No definition text is invented and no source bytes are normalized.
    pub(crate) fn longest_cjk_prefix(
        &self,
        text: &str,
        book: &[PathBuf],
        order: &[PathBuf],
        disabled: &[PathBuf],
        with_forms: bool,
        cancel: &AtomicBool,
    ) -> io::Result<Option<usize>> {
        let (forms, ends) = prefix_forms(text, with_forms);
        if ends.is_empty() {
            return Ok(None);
        }
        let check = || {
            if cancel.load(Ordering::Relaxed) {
                Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "Word selection cancelled",
                ))
            } else {
                Ok(())
            }
        };
        let (sources, mut first_error) =
            primitive_app::with_storage(|| self.enabled(book, order, disabled))?;
        let mut budget = LookupBudget {
            index_bytes: MAX_TOTAL_INDEX_BYTES,
            dictzip_bytes: MAX_DZ_LOOKUP_BYTES,
        };
        let mut best = usize::MAX;
        let mut read_any = false;
        for (file, info) in sources {
            check()?;
            let result = primitive_app::with_storage(|| {
                let mut aliases = Ranked::default();
                let synonyms = visit_synonyms(
                    &file.root,
                    &file.path,
                    &info,
                    &mut budget,
                    |word, target| {
                        check()?;
                        if let Some(rank) = forms.rank(word) {
                            if rank < aliases.rank || !aliases.values.contains(&target) {
                                aliases.record(rank, || target);
                            }
                        }
                        Ok(())
                    },
                );
                if let Err(error) = synonyms {
                    if error.kind() == io::ErrorKind::Interrupted {
                        return Err(error);
                    }
                    // A truncated/malformed synonym index cannot authorize an
                    // automatic source endpoint from its partial records.
                    aliases = Ranked::default();
                }
                aliases.values.sort_unstable();
                aliases.values.dedup();
                let (index, mut data) =
                    open_index_and_data(&file.root, &file.path, &info, &mut budget)?;
                let mut matches = Ranked::default();
                visit_index(
                    index,
                    &info,
                    data.len()?,
                    |ordinal, headword, offset, size| {
                        check()?;
                        let alias = aliases
                            .values
                            .binary_search(&ordinal)
                            .ok()
                            .map(|_| aliases.rank);
                        if let Some(rank) = forms.rank(headword).into_iter().chain(alias).min() {
                            matches.record(rank, || IndexMatch {
                                word: headword.into(),
                                offset,
                                size: size as usize,
                            });
                        }
                        Ok(())
                    },
                )?;
                check()?;
                let rank = matches.rank;
                let definitions =
                    read_definitions(&mut data, &info, &file.key, &mut budget, matches.values)?;
                check()?;
                Ok::<_, io::Error>((!definitions.is_empty()).then_some(rank))
            });
            match result {
                Ok(rank) => {
                    read_any = true;
                    if let Some(rank) = rank {
                        best = best.min(rank);
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => return Err(error),
                Err(error) => {
                    first_error.get_or_insert(error);
                }
            }
            if best == 0 || budget.index_bytes == 0 {
                break;
            }
        }
        if !read_any && let Some(error) = first_error {
            return Err(error);
        }
        Ok(ends.get(best).copied())
    }
}

#[cfg(feature = "kindle")]
fn prefix_forms(text: &str, with_forms: bool) -> (WordForms, Vec<usize>) {
    let ends: Vec<_> = text
        .char_indices()
        .take(MAX_SCAN_CHARACTERS)
        .take_while(|(_, c)| word_character(*c))
        .map(|(i, c)| i + c.len_utf8())
        .collect();
    let mut forms = WordForms::default();
    // A source endpoint must not leave a combining voicing mark or variation
    // selector outside the selected rendered character.
    let maximum = ends.last().copied().unwrap_or(0);
    let boundaries: Vec<_> = icu_segmenter::GraphemeClusterSegmenter::new()
        .segment_str(text)
        .take_while(|end| *end <= maximum)
        .collect();
    let ends: Vec<_> = ends
        .into_iter()
        .rev()
        .filter(|end| boundaries.binary_search(end).is_ok())
        .collect();
    for (rank, &end) in ends.iter().enumerate() {
        let prefix = &text[..end];
        forms.order.entry(prefix.into()).or_insert(rank);
        if with_forms {
            for (candidate, _) in WordForms::for_word(prefix).spellings() {
                if forms.order.len() == MAX_SELECTION_TERMS {
                    break;
                }
                forms.order.entry(candidate.into()).or_insert(rank);
            }
        }
    }
    (forms, ends)
}

#[cfg(all(test, feature = "kindle"))]
mod tests {
    use super::super::tests::{TestDirectory, write_dictionary, write_synonyms};
    use super::*;
    #[test]
    fn longest_source_prefix_wins_across_dictionaries_and_synonyms() {
        let root = TestDirectory::new();
        write_dictionary(
            &root.0,
            "First",
            "sametypesequence=m\n",
            &[("読", b"read"), ("読む", b"read a book")],
        );
        write_dictionary(
            &root.0,
            "Second",
            "sametypesequence=m\n",
            &[("読み込む", b"read thoroughly")],
        );
        write_synonyms(&root.0, "Second", &[("読み込み", 0)]);
        let sources = DictionarySources {
            local: root.0.clone(),
            shared_storage: None,
        };
        let cancel = AtomicBool::new(false);
        let lookup = |text, forms| {
            sources
                .longest_cjk_prefix(text, &[], &[], &[], forms, &cancel)
                .unwrap()
        };
        assert_eq!(
            lookup("読み込みました。本", true),
            Some("読み込みました".len())
        );
        assert_eq!(lookup("読み込みました。本", false), Some("読み込み".len()));
        assert_eq!(lookup("読む。本", true), Some("読む".len()));
        assert_eq!(lookup("中文English", true), None);
        cancel.store(true, Ordering::Relaxed);
        assert_eq!(
            sources
                .longest_cjk_prefix("読む", &[], &[], &[], true, &cancel)
                .unwrap_err()
                .kind(),
            io::ErrorKind::Interrupted
        );
    }
    #[test]
    fn direct_spelling_wins_and_equally_plausible_bases_keep_their_real_definitions() {
        let root = TestDirectory::new();
        write_dictionary(
            &root.0,
            "Authored",
            "sametypesequence=m\n",
            &[
                ("読ぶ", b"an authored ambiguity control"),
                ("読む", b"to read"),
                ("ガクセイ", b"student"),
                ("Dogs", b"plural noun"),
                ("Dog", b"singular noun"),
            ],
        );
        let sources = DictionarySources {
            local: root.0.clone(),
            shared_storage: None,
        };
        let lookup = |word| {
            sources
                .lookup_configured(word, &[], &[], &[], true)
                .unwrap()
        };
        let entries = lookup("読んだ");
        assert_eq!(
            entries.iter().map(|d| d.word.as_str()).collect::<Vec<_>>(),
            ["読ぶ", "読む"]
        );
        assert_eq!(lookup("Ｄｏｇｓ")[0].word, "Dogs");
        assert_eq!(lookup("ｶﾞｸｾｲ")[0].word, "ガクセイ");
        write_dictionary(
            &root.0,
            "Auxiliary",
            "sametypesequence=m\n",
            &[("して", b"an indexed connective"), ("する", b"to do")],
        );
        let auxiliary = lookup("していた");
        assert_eq!(
            auxiliary
                .iter()
                .filter(|d| d.dictionary == "Auxiliary")
                .map(|d| d.word.as_str())
                .collect::<Vec<_>>(),
            ["して", "する"]
        );
        write_dictionary(
            &root.0,
            "Authored",
            "sametypesequence=m\n",
            &[("読んだ", b"literal"), ("読む", b"base")],
        );
        assert_eq!(lookup("読んだ")[0].word, "読んだ");
    }
    #[test]
    fn malformed_synonym_records_cannot_apply_a_partial_automatic_match() {
        let root = TestDirectory::new();
        write_dictionary(
            &root.0,
            "Aliases",
            "sametypesequence=m\n",
            &[("canonical", b"definition")],
        );
        write_synonyms(&root.0, "Aliases", &[("読み込む", 0), ("未使用", 0)]);
        let sources = DictionarySources {
            local: root.0.clone(),
            shared_storage: None,
        };
        let cancel = AtomicBool::new(false);
        let lookup = || {
            sources
                .longest_cjk_prefix("読み込みました", &[], &[], &[], true, &cancel)
                .unwrap()
        };
        assert_eq!(lookup(), Some("読み込みました".len()));
        let path = root.0.join("Aliases.syn");
        let bytes = fs::read(&path).unwrap();
        fs::write(path, &bytes[..bytes.len() - 1]).unwrap();
        assert_eq!(lookup(), None);
    }
    #[test]
    fn source_prefixes_do_not_split_voicing_marks_or_ideographic_variation_sequences() {
        let root = TestDirectory::new();
        write_dictionary(
            &root.0,
            "Kana",
            "sametypesequence=m\n",
            &[("か", b"unvoiced"), ("カ", b"unvoiced")],
        );
        let sources = DictionarySources {
            local: root.0.clone(),
            shared_storage: None,
        };
        let cancel = AtomicBool::new(false);
        for text in ["か\u{3099}", "ｶﾞ", "か\u{fe00}"] {
            assert_eq!(
                sources
                    .longest_cjk_prefix(text, &[], &[], &[], true, &cancel)
                    .unwrap(),
                None
            );
        }
        write_dictionary(
            &root.0,
            "Voiced",
            "sametypesequence=m\n",
            &[("が", b"voiced"), ("ガ", b"voiced")],
        );
        for text in ["か\u{3099}", "ｶﾞ"] {
            assert_eq!(
                sources
                    .longest_cjk_prefix(text, &[], &[], &[], true, &cancel)
                    .unwrap(),
                Some(text.len())
            );
        }
        let (forms, ends) = prefix_forms("字\u{e0100}次", false);
        assert!(!ends.contains(&"字".len()));
        assert!(forms.rank("字").is_none());
    }
    #[test]
    fn scanning_never_crosses_punctuation_or_its_scalar_and_candidate_budgets() {
        let text = "食べました。次の語";
        let (forms, ends) = prefix_forms(text, true);
        assert_eq!(ends[0], "食べました".len());
        assert_eq!(forms.rank("食べる"), Some(0));
        assert!(forms.order.keys().all(|s| !s.contains('。')));
        let (forms, ends) = prefix_forms(&"あ".repeat(1000), true);
        assert_eq!(ends.len(), MAX_SCAN_CHARACTERS);
        assert!(forms.order.len() <= MAX_SELECTION_TERMS);
        for punctuation in ['・', '･', '、', '。', '「', '」', ' ', '\n', '！', 'a'] {
            assert!(!word_character(punctuation));
        }
        assert!(word_character('𠮷'));
    }
}
