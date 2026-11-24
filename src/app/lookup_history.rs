use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::read_import_file;

const MAX_QUERY_HISTORY: usize = 64;
const MAX_HISTORY_BYTES: u64 = 8 * 1024 * 1024;
const MAX_QUERY_CHARS: usize = 128;
const MAX_BOOK_TITLE_BYTES: usize = 512;
const LAST_TIMESTAMP: i64 = 253_402_300_799; // 9999-12-31 23:59:59 UTC

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub(super) struct SearchQuery {
    pub text: String,
    pub match_case: bool,
    #[serde(default)]
    pub ignore_accents: bool,
    #[serde(default)]
    pub regular_expression: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(untagged)]
pub(super) enum DictionaryLookup {
    Legacy(String),
    Recorded {
        word: String,
        book_title: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        time: Option<i64>,
    },
}

impl DictionaryLookup {
    pub(super) fn word(&self) -> &str {
        match self {
            Self::Legacy(word) | Self::Recorded { word, .. } => word,
        }
    }

    pub(super) fn book_title(&self) -> Option<&str> {
        match self {
            Self::Legacy(_) => None,
            Self::Recorded { book_title, .. } => Some(book_title),
        }
    }

    pub(super) fn time(&self) -> Option<i64> {
        match self {
            Self::Legacy(_) => None,
            Self::Recorded { time, .. } => *time,
        }
    }

    pub(super) fn valid(&self) -> bool {
        valid(self.word())
            && self.book_title().is_none_or(|title| {
                !title.trim().is_empty()
                    && title.len() <= MAX_BOOK_TITLE_BYTES
                    && !title.chars().any(char::is_control)
            })
            && self.time().is_none_or(valid_time)
    }
}

fn valid_time(time: i64) -> bool {
    (0..=LAST_TIMESTAMP).contains(&time)
}

fn book_title(title: &str) -> String {
    let mut cleaned = String::new();
    for part in title.split_whitespace() {
        if !cleaned.is_empty() && cleaned.len() < MAX_BOOK_TITLE_BYTES {
            cleaned.push(' ');
        }
        for ch in part.chars().filter(|ch| !ch.is_control()) {
            if cleaned.len() + ch.len_utf8() > MAX_BOOK_TITLE_BYTES {
                return cleaned.trim_end().to_owned();
            }
            cleaned.push(ch);
        }
    }
    if cleaned.trim().is_empty() {
        return "Dictionary lookup".into();
    }
    cleaned.trim_end().to_owned()
}

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub(super) struct LookupHistory {
    pub searches: Vec<SearchQuery>,
    pub dictionary: Vec<DictionaryLookup>,
    pub vocabulary: Vec<String>,
    pub review: super::vocabulary::Vocabulary,
    pub online: Vec<crate::online_lookup::Request>,
    pub online_languages: super::online_panel::Languages,
}

impl LookupHistory {
    pub fn load(path: &Path) -> Self {
        let mut history: Self = read_import_file(path, MAX_HISTORY_BYTES)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        history.searches.retain(|item| {
            item.text.len() <= crate::search::MAX_QUERY_BYTES
                && if item.regular_expression {
                    !item.text.is_empty()
                } else {
                    !item.text.trim().is_empty()
                }
        });
        history.dictionary.retain(DictionaryLookup::valid);
        let mut seen = std::collections::BTreeSet::new();
        history
            .vocabulary
            .retain(|word| valid(word) && seen.insert(word.clone()));
        history.online.retain(|item| item.validate().is_ok());
        history.online.truncate(8);
        history.online_languages.normalize();
        history.searches.truncate(MAX_QUERY_HISTORY);
        history.dictionary.truncate(MAX_QUERY_HISTORY);
        history.review.words.retain(|word, _| valid(word));
        for word in &history.vocabulary {
            history.review.words.entry(word.clone()).or_default();
        }
        for word in history.review.words.keys() {
            if seen.insert(word.clone()) {
                history.vocabulary.push(word.clone());
            }
        }
        history
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        let contents = serde_json::to_vec(self).map_err(io::Error::other)?;
        if contents.len() as u64 > MAX_HISTORY_BYTES {
            return Err(io::Error::other(
                "Vocabulary and lookup history exceed 8 MiB",
            ));
        }
        primitive_app::save(path, &contents)
    }

    pub fn search(
        &mut self,
        text: &str,
        match_case: bool,
        ignore_accents: bool,
        regular_expression: bool,
    ) {
        let current = SearchQuery {
            // Literal spaces at either end carry word-box semantics on fixed pages.
            // Older literal queries also allowed surrounding whitespace outside the byte limit.
            text: if text.len() <= crate::search::MAX_QUERY_BYTES {
                text.to_owned()
            } else {
                text.trim().to_owned()
            },
            match_case,
            ignore_accents,
            regular_expression,
        };
        remember(&mut self.searches, current, MAX_QUERY_HISTORY);
    }

    pub(super) fn lookup(&mut self, word: &str, title: &str, time: Option<i64>) {
        self.dictionary.insert(
            0,
            DictionaryLookup::Recorded {
                word: word.to_owned(),
                book_title: book_title(title),
                time: time.filter(|&time| valid_time(time)),
            },
        );
        self.dictionary.truncate(MAX_QUERY_HISTORY);
    }

    pub fn online(&mut self, request: crate::online_lookup::Request) {
        remember(&mut self.online, request, 8);
    }

    pub fn save_word(&mut self, word: &str) -> bool {
        if let Some(index) = self.vocabulary.iter().position(|saved| saved == word) {
            self.vocabulary.remove(index);
            self.review.words.remove(word);
            return false;
        }
        self.vocabulary.insert(0, word.to_owned());
        true
    }
}

pub(super) fn valid(text: &str) -> bool {
    !text.trim().is_empty() && text.chars().count() <= MAX_QUERY_CHARS
}

fn remember<T: PartialEq>(entries: &mut Vec<T>, entry: T, limit: usize) {
    entries.retain(|prior| prior != &entry);
    entries.insert(0, entry);
    entries.truncate(limit);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dictionary_history_counts_repeats_and_bounds_title_and_time() {
        let old = DictionaryLookup::Legacy("old word".into());
        let mut history = LookupHistory {
            dictionary: vec![old],
            ..Default::default()
        };
        for timestamp in 0..70 {
            history.lookup(
                "harbour",
                &format!("  海港 \n {} ", "海".repeat(512)),
                Some(timestamp),
            );
        }
        assert_eq!(history.dictionary.len(), MAX_QUERY_HISTORY);
        assert_eq!(history.dictionary.first().unwrap().time(), Some(69));
        assert_eq!(history.dictionary.last().unwrap().time(), Some(6));
        assert!(history.dictionary.iter().all(DictionaryLookup::valid));
        let title = history.dictionary[0].book_title().unwrap();
        assert!(title.starts_with("海港 "));
        assert!(title.len() <= MAX_BOOK_TITLE_BYTES);
        history.lookup("example", "\n\r", Some(LAST_TIMESTAMP + 1));
        let latest = &history.dictionary[0];
        assert_eq!(latest.book_title(), Some("Dictionary lookup"));
        assert_eq!(latest.time(), None);
    }
}
