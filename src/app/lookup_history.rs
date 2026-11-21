use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::read_import_file;

const MAX_QUERY_HISTORY: usize = 64;
const MAX_VOCABULARY: usize = 256;
const MAX_QUERY_CHARS: usize = 128;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub(super) struct SearchQuery {
    pub text: String,
    pub match_case: bool,
    #[serde(default)]
    pub ignore_accents: bool,
    #[serde(default)]
    pub regular_expression: bool,
}

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub(super) struct LookupHistory {
    pub searches: Vec<SearchQuery>,
    pub dictionary: Vec<String>,
    pub vocabulary: Vec<String>,
    pub online: Vec<crate::online_lookup::Request>,
    pub online_languages: super::online_panel::Languages,
}

impl LookupHistory {
    pub fn load(path: &Path) -> Self {
        let mut history: Self = read_import_file(path, 256 * 1024)
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
        history.dictionary.retain(|word| valid(word));
        history.vocabulary.retain(|word| valid(word));
        history.online.retain(|item| item.validate().is_ok());
        history.online.truncate(8);
        history.online_languages.normalize();
        history.searches.truncate(MAX_QUERY_HISTORY);
        history.dictionary.truncate(MAX_QUERY_HISTORY);
        history.vocabulary.truncate(MAX_VOCABULARY);
        history
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        let contents = serde_json::to_vec(self).map_err(io::Error::other)?;
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

    pub fn lookup(&mut self, word: &str) {
        remember(&mut self.dictionary, word.to_owned(), MAX_QUERY_HISTORY);
    }

    pub fn online(&mut self, request: crate::online_lookup::Request) {
        remember(&mut self.online, request, 8);
    }

    pub fn save_word(&mut self, word: &str) -> bool {
        if let Some(index) = self.vocabulary.iter().position(|saved| saved == word) {
            self.vocabulary.remove(index);
            return false;
        }
        remember(&mut self.vocabulary, word.to_owned(), MAX_VOCABULARY);
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
