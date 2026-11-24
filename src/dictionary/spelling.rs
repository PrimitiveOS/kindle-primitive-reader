//! Bounded headword suggestions after an ordinary dictionary lookup misses.
use std::sync::atomic::{AtomicBool, Ordering};

use super::*;

const SUGGESTIONS: usize = 10;
const DISTANCE_LIMIT: usize = 3;
const MAX_SUGGESTED_SENSES: usize = 64;

pub(crate) fn eligible(word: &str) -> bool {
    !word.is_empty()
        && word.len() <= MAX_WORD_BYTES
        && !word.chars().any(|ch| {
            matches!(ch as u32, 0x1100..=0x11ff | 0x2e80..=0x9fff | 0xa960..=0xa97f |
            0xac00..=0xd7ff | 0xf900..=0xfaff | 0xfe30..=0xfe4f | 0xff00..=0xffef |
            0x20000..=0x2ebef | 0x2f800..=0x2fa1f | 0x30000..=0x3134f)
        })
}

// sdcv compares lowercased Unicode scalars. It first filters candidate lengths,
// then compares at most as many candidate characters as the selected word has.
#[derive(Clone)]
struct Nearest {
    query: Vec<char>,
    word: Vec<char>,
    slots: [Option<(usize, String)>; SUGGESTIONS],
    threshold: usize,
}

impl Nearest {
    fn new(query: &str) -> Self {
        Self {
            query: query.chars().map(lower).collect(),
            word: Vec::with_capacity(query.len()),
            slots: Default::default(),
            threshold: DISTANCE_LIMIT,
        }
    }

    fn inspect(&mut self, headword: &str) {
        let n = self.query.len();
        let size = headword.chars().count();
        if size.abs_diff(n) >= self.threshold {
            return;
        }
        self.word.clear();
        self.word.extend(headword.chars().take(n).map(lower));
        let distance = distance(&self.word, &self.query, self.threshold);
        if distance >= self.threshold
            || distance >= n
            || self.slots.iter().flatten().any(|(_, s)| s == headword)
        {
            return;
        }
        // Once the list fills, replace the last slot tied at the old cutoff.
        let slot = self
            .slots
            .iter_mut()
            .rfind(|v| v.as_ref().map_or(DISTANCE_LIMIT, |(d, _)| *d) == self.threshold)
            .unwrap();
        *slot = Some((distance, headword.to_owned()));
        self.threshold = self
            .slots
            .iter()
            .map(|v| v.as_ref().map_or(DISTANCE_LIMIT, |(d, _)| *d))
            .max()
            .unwrap();
    }

    fn finish(self) -> Vec<String> {
        let mut selected: Vec<_> = self.slots.into_iter().flatten().collect();
        selected.sort_by(|(da, a), (db, b)| {
            da.cmp(db)
                .then_with(|| {
                    a.bytes()
                        .map(|c| c.to_ascii_lowercase())
                        .cmp(b.bytes().map(|c| c.to_ascii_lowercase()))
                })
                .then_with(|| a.cmp(b))
        });
        selected.into_iter().map(|(_, s)| s).collect()
    }
}

fn lower(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

/// Adjacent transposition costs one edit. Only the five diagonals that can be
/// within two edits are needed, regardless of the selected word's length.
fn distance(a: &[char], b: &[char], limit: usize) -> usize {
    let mut first = 0;
    while first < a.len().min(b.len()) && a[first] == b[first] {
        first += 1;
    }
    let (mut a, mut b) = (&a[first..], &b[first..]);
    while !a.is_empty() && !b.is_empty() && a.last() == b.last() {
        a = &a[..a.len() - 1];
        b = &b[..b.len() - 1];
    }
    if a.is_empty() || b.is_empty() {
        return a.len().max(b.len());
    }
    let k = limit - 1;
    let mut prev2 = [limit; 7];
    let mut prev = [limit; 7];
    // Store the central band using index (column - row + radius + 1).
    for j in 0..=k.min(b.len()) {
        prev[k + 1 + j] = j;
    }
    for i in 1..=a.len() {
        let mut row = [limit; 7];
        let start = i.saturating_sub(k);
        let end = (i + k).min(b.len());
        for j in start..=end {
            let t = j + k + 1 - i;
            if j == 0 {
                row[t] = i;
                continue;
            }
            row[t] = (prev[t + 1] + 1)
                .min(row[t - 1] + 1)
                .min(prev[t] + usize::from(a[i - 1] != b[j - 1]));
            if i >= 2 && j >= 2 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                row[t] = row[t].min(prev2[t] + 1);
            }
        }
        prev2 = prev;
        prev = row;
    }
    if a.len().abs_diff(b.len()) > k {
        limit
    } else {
        prev[b.len() + k + 1 - a.len()]
    }
}

#[cfg(feature = "kindle")]
impl DictionarySources {
    pub(crate) fn suggested_definitions(
        &self,
        query: &str,
        book_order: &[PathBuf],
        order: &[PathBuf],
        disabled: &[PathBuf],
        cancel: &AtomicBool,
    ) -> io::Result<Vec<LookupEntry>> {
        if !eligible(query) {
            return Ok(Vec::new());
        }
        let (sources, mut first_error) =
            primitive_app::with_storage(|| self.enabled(book_order, order, disabled))?;
        let mut nearest = Nearest::new(query);
        let mut budget = LookupBudget {
            index_bytes: MAX_TOTAL_INDEX_BYTES,
            dictzip_bytes: MAX_DZ_LOOKUP_BYTES,
        };
        let check = || {
            if cancel.load(Ordering::Relaxed) {
                Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "Dictionary lookup cancelled",
                ))
            } else {
                Ok(())
            }
        };
        let mut valid = false;
        for (file, info) in &sources {
            check()?;
            let mut candidate = nearest.clone();
            let scan = primitive_app::with_storage(|| {
                let (index, data) = open_index_and_data(&file.root, &file.path, info, &mut budget)?;
                visit_index(index, info, data.len()?, |ordinal, word, _, _| {
                    if ordinal % 128 == 0 {
                        check()?;
                    }
                    candidate.inspect(word);
                    Ok(())
                })
            });
            match scan {
                Ok(()) => {
                    valid = true;
                    nearest = candidate;
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => return Err(e),
                Err(e) => {
                    first_error.get_or_insert(e);
                }
            }
        }
        if !valid && let Some(e) = first_error {
            return Err(e);
        }
        let names = nearest.finish();
        if names.is_empty() {
            return Ok(Vec::new());
        }
        let queries = SuggestionQueries::new(&names);
        let mut by_word: Vec<Vec<LookupEntry>> = (0..names.len()).map(|_| Vec::new()).collect();
        let mut budget = LookupBudget {
            index_bytes: MAX_TOTAL_INDEX_BYTES,
            dictzip_bytes: MAX_DZ_LOOKUP_BYTES,
        };
        for (file, info) in sources {
            check()?;
            let definitions =
                primitive_app::with_storage(|| queries.resolve(&file, &info, &mut budget, &check));
            match definitions {
                Ok(found) => {
                    for (word, entries) in by_word.iter_mut().zip(found) {
                        word.extend(
                            entries
                                .into_iter()
                                .take(MAX_RESULTS.saturating_sub(word.len())),
                        );
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => return Err(e),
                Err(e) => {
                    first_error.get_or_insert(e);
                }
            }
        }
        let mut results: Vec<LookupEntry> = Vec::new();
        for entry in by_word.into_iter().flatten() {
            if !results
                .iter()
                .any(|found| found.definition == entry.definition)
            {
                results.push(entry);
            }
            if results.len() == MAX_SUGGESTED_SENSES {
                break;
            }
        }
        Ok(results)
    }
}

/// Resolve all ten suggestions in a single pass over a dictionary. Each term
/// retains its own precedence for direct spelling, case forms and inflections.
#[cfg(feature = "kindle")]
struct SuggestionQueries {
    names: std::collections::HashMap<String, Vec<(usize, usize)>>,
    count: usize,
}

#[cfg(feature = "kindle")]
impl SuggestionQueries {
    fn new(words: &[String]) -> Self {
        let mut names = std::collections::HashMap::<String, Vec<(usize, usize)>>::new();
        for (i, word) in words.iter().enumerate() {
            let mut add = |key: &str, rank: usize| {
                let targets = names.entry(key.to_owned()).or_default();
                if let Some((_, old)) = targets.iter_mut().find(|(which, _)| *which == i) {
                    *old = (*old).min(rank);
                } else {
                    targets.push((i, rank));
                }
            };
            add(word, 0);
            add(&word.to_lowercase(), 1);
            add(&word.to_uppercase(), 2);
            if let Some(first) = word.chars().next() {
                let mut title = first.to_uppercase().to_string();
                title.push_str(&word[first.len_utf8()..].to_lowercase());
                add(&title, 3);
            }
            for (form, rank) in WordForms::for_word(word).spellings() {
                add(form, 4 + rank);
            }
        }
        Self {
            names,
            count: words.len(),
        }
    }

    fn resolve(
        &self,
        file: &DictionaryFile,
        info: &Info,
        budget: &mut LookupBudget,
        check: &impl Fn() -> io::Result<()>,
    ) -> io::Result<Vec<Vec<LookupEntry>>> {
        let mut aliases: Vec<Ranked<usize>> = (0..self.count).map(|_| Ranked::default()).collect();
        let mut visited = 0usize;
        let alias_result = visit_synonyms(&file.root, &file.path, info, budget, |name, ordinal| {
            if visited % 128 == 0 {
                check()?;
            }
            visited += 1;
            if let Some(targets) = self.names.get(name) {
                for &(query, rank) in targets {
                    let list = &mut aliases[query];
                    if rank < list.rank || !list.values.contains(&ordinal) {
                        list.record(rank, || ordinal);
                    }
                }
            }
            Ok(())
        });
        check()?;
        let mut by_ordinal = std::collections::HashMap::<usize, Vec<(usize, usize)>>::new();
        if alias_result.is_ok() {
            for (query, list) in aliases.into_iter().enumerate() {
                for ordinal in list.values {
                    by_ordinal
                        .entry(ordinal)
                        .or_default()
                        .push((query, list.rank));
                }
            }
        }
        let (index, mut data) = open_index_and_data(&file.root, &file.path, info, budget)?;
        let mut hits: Vec<Ranked<IndexMatch>> =
            (0..self.count).map(|_| Ranked::default()).collect();
        visit_index(index, info, data.len()?, |ordinal, name, offset, size| {
            if ordinal % 128 == 0 {
                check()?;
            }
            let direct = self.names.get(name);
            let indirect = by_ordinal.get(&ordinal);
            if direct.is_none() && indirect.is_none() {
                return Ok(());
            }
            let mut ranks = [usize::MAX; SUGGESTIONS];
            for &(query, rank) in direct.into_iter().chain(indirect).flatten() {
                ranks[query] = ranks[query].min(rank);
            }
            for (slot, rank) in hits.iter_mut().zip(ranks) {
                if rank != usize::MAX {
                    slot.record(rank, || IndexMatch {
                        word: name.to_owned(),
                        offset,
                        size: size as usize,
                    });
                }
            }
            Ok(())
        })?;
        hits.into_iter()
            .map(|h| read_definitions(&mut data, info, &file.key, budget, h.values))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ranking_handles_transposition_prefixes_ties_and_cjk_queries() {
        let mut candidates = Nearest::new("sotne");
        for word in ["settle", "stone", "sotneworks", "some", "stone"] {
            candidates.inspect(word);
        }
        let words = candidates.finish();
        assert_eq!(words.first().map(String::as_str), Some("stone"));
        assert!(!words.contains(&"sotneworks".to_string()));
        let mut starts = Nearest::new("port");
        for w in ["PORTS", "port", "portals", "part"] {
            starts.inspect(w);
        }
        assert_eq!(starts.finish(), ["port", "PORTS", "part"]);
        assert!(!eligible("東京"));
        assert!(!eligible("手book"));
        assert!(eligible("Été"));
    }
    #[test]
    fn banded_distance_matches_a_full_reference_table() {
        fn cost(a: &[char], b: &[char]) -> usize {
            let mut grid = vec![vec![0; b.len() + 1]; a.len() + 1];
            for (i, row) in grid.iter_mut().enumerate() {
                row[0] = i;
            }
            for j in 0..=b.len() {
                grid[0][j] = j;
            }
            for i in 1..=a.len() {
                for j in 1..=b.len() {
                    let mut d = (grid[i - 1][j] + 1)
                        .min(grid[i][j - 1] + 1)
                        .min(grid[i - 1][j - 1] + usize::from(a[i - 1] != b[j - 1]));
                    if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                        d = d.min(grid[i - 2][j - 2] + 1);
                    }
                    grid[i][j] = d;
                }
            }
            grid[a.len()][b.len()]
        }
        let mut words = vec![Vec::new()];
        for _ in 0..4 {
            let old = words.clone();
            for w in old {
                for c in ['a', 'b', 'c'] {
                    let mut v = w.clone();
                    v.push(c);
                    words.push(v);
                }
            }
        }
        words.sort();
        words.dedup();
        for a in &words {
            for b in &words {
                for k in 1..=3 {
                    assert_eq!(distance(a, b, k).min(k), cost(a, b).min(k));
                }
            }
        }
    }
}
