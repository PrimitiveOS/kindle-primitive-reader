//! A reader-authored, single-level contents list attached to one publication.
use super::{Bookmark, ReadingPosition, invalid_state, same_position, valid_position};
use serde::{Deserialize, Serialize};
use std::io;

pub(crate) const TITLE_LIMIT: usize = 96;
const ENTRY_LIMIT: usize = 256;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub(crate) struct CustomContents {
    pub enabled: bool,
    entries: Vec<Bookmark>,
}

impl CustomContents {
    pub fn is_default(&self) -> bool {
        !self.enabled && self.entries.is_empty()
    }

    pub fn entries(&self) -> &[Bookmark] {
        &self.entries
    }

    pub fn find(&self, position: &ReadingPosition) -> Option<usize> {
        self.entries.iter().position(|entry| {
            entry.position.chapter == position.chapter && same_position(&entry.position, position)
        })
    }

    pub fn save(&mut self, position: ReadingPosition, label: &str) -> io::Result<()> {
        let title = label.trim();
        if title.is_empty()
            || title.chars().count() > TITLE_LIMIT
            || title.chars().any(char::is_control)
        {
            return Err(invalid_state("Enter a chapter title of 1 to 96 characters"));
        }
        if !valid_position(&position) {
            return Err(invalid_state("That chapter location is unavailable"));
        }
        if let Some(index) = self.find(&position) {
            self.entries[index].label = title.to_owned();
            return Ok(());
        }
        if self.entries.len() >= ENTRY_LIMIT {
            return Err(invalid_state("This book already has 256 custom chapters"));
        }
        let key = |p: &ReadingPosition| {
            p.anchor.as_ref().map_or((p.chapter, 0, p.page, 0), |a| {
                (p.chapter, 1, a.block, a.offset)
            })
        };
        let index = self
            .entries
            .partition_point(|entry| key(&entry.position) <= key(&position));
        self.entries.insert(
            index,
            Bookmark {
                chapter_title: None,
                created_at: None,
                label: title.to_owned(),
                position,
            },
        );
        Ok(())
    }

    pub fn remove(&mut self, position: &ReadingPosition) -> io::Result<()> {
        let index = self
            .find(position)
            .ok_or_else(|| invalid_state("That chapter is unavailable"))?;
        self.entries.remove(index);
        Ok(())
    }

    pub fn sanitize(&mut self) {
        let mut input = std::mem::take(&mut self.entries);
        input.truncate(ENTRY_LIMIT);
        for entry in input {
            let _ = self.save(entry.position, &entry.label);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::{BookState, merge_states};

    #[test]
    fn backup_merge_keeps_existing_titles_and_bounds_custom_entries() {
        let initial: BookState =
            serde_json::from_str(r#"{"id":7,"path":"archive.pdf","title":"Archive"}"#).unwrap();
        let mut source = initial.clone();
        let mut current = initial;
        let page = |page| ReadingPosition {
            chapter: 0,
            page,
            anchor: None,
        };
        current.custom_contents.save(page(3), "Current").unwrap();
        source.custom_contents.enabled = true;
        source.custom_contents.save(page(3), "Older title").unwrap();
        source.custom_contents.save(page(1), "Foreword").unwrap();
        let (mut merged, summary) = merge_states(&[current], &[source.clone()]);
        assert_eq!((summary.custom_chapters_added, summary.duplicates), (1, 1));
        assert_eq!(
            merged[0]
                .custom_contents
                .entries
                .iter()
                .map(|entry| entry.label.as_str())
                .collect::<Vec<_>>(),
            ["Foreword", "Current"]
        );
        assert!(!merged[0].custom_contents.enabled);
        let (_, summary) = merge_states(&merged, &[source.clone()]);
        assert_eq!((summary.custom_chapters_added, summary.duplicates), (0, 2));
        let (restored, _) = merge_states(&[], &[source]);
        assert!(restored[0].custom_contents.enabled);
        let chapters = &mut merged[0].custom_contents;
        assert!(chapters.save(page(2), "").is_err());
        assert!(
            chapters
                .save(page(2), &"x".repeat(TITLE_LIMIT + 1))
                .is_err()
        );
        for i in 10..10 + ENTRY_LIMIT {
            let _ = chapters.save(page(i), "Chapter");
        }
        assert_eq!(chapters.entries.len(), ENTRY_LIMIT);
        assert!(chapters.save(page(0), "Excess").is_err());
        chapters.save(page(3), "Renamed at the limit").unwrap();
    }
}
