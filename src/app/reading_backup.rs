//! Explicit, component-scoped local reading-data backup and restore.
use super::*;
use crate::reading_backup::{self as archive, Component, Envelope, Key};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub(super) struct Preview {
    library: Vec<u8>,
    summary: crate::library::BackupSummary,
    components: Vec<Choice>,
    titles: BTreeMap<u64, String>,
}
struct Choice {
    key: Key,
    data: Value,
    existing: bool,
    selected: bool,
}
#[derive(Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
struct LocalLookup {
    searches: Vec<lookup_history::SearchQuery>,
    dictionary: Vec<lookup_history::DictionaryLookup>,
    vocabulary: Vec<String>,
    review: vocabulary::Vocabulary,
}
fn valid_text(value: &str, max: usize) -> bool {
    value.len() <= max
        && !value
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\t'))
}
fn local_lookup(value: Value) -> io::Result<Value> {
    let local: LocalLookup = serde_json::from_value(value)?;
    let unique = local.vocabulary.iter().collect::<BTreeSet<_>>().len() == local.vocabulary.len();
    if local.searches.len() > 64
        || local.dictionary.len() > 64
        || local.vocabulary.len() > 65_536
        || local.review.words.len() > 65_536
        || local.review.excluded_books.len() > 65_536
        || !unique
        || local
            .searches
            .iter()
            .any(|q| q.text.is_empty() || q.text.len() > crate::search::MAX_QUERY_BYTES)
        || local.dictionary.iter().any(|lookup| !lookup.valid())
        || local.vocabulary.iter().any(|w| !lookup_history::valid(w))
        || local
            .review
            .excluded_books
            .iter()
            .any(|s| !valid_text(s, 16_384))
        || local.review.words.iter().any(|(word, entry)| {
            !lookup_history::valid(word)
                || !valid_text(&entry.book, 16_384)
                || [&entry.before, &entry.after, &entry.highlight]
                    .into_iter()
                    .any(|text| text.as_deref().is_some_and(|s| !valid_text(s, 64 * 1024)))
                || [entry.created_at, entry.reviewed_at, entry.due_at]
                    .into_iter()
                    .flatten()
                    .any(|n| n > i64::MAX as u64)
        })
    {
        return Err(archive::invalid(
            "The vocabulary or local lookup history backup is invalid",
        ));
    }
    serde_json::to_value(local).map_err(io::Error::other)
}
fn lookup_subset(value: &Value) -> Value {
    let mut local = serde_json::Map::new();
    for key in ["searches", "dictionary", "vocabulary", "review"] {
        if let Some(value) = value.get(key) {
            local.insert(key.into(), value.clone());
        }
    }
    Value::Object(local)
}
fn canonical(key: Key, value: Value) -> io::Result<Value> {
    let bytes = serde_json::to_vec(&value)?;
    if bytes.len() as u64 > key.limit() {
        return Err(archive::invalid("A backup component exceeds its limit"));
    }
    match key {
        Key::Controls => {
            let controls: ReadingControls = serde_json::from_value(value.clone())?;
            if controls
                .numbering
                .stride
                .is_some_and(|n| !(500..=3000).contains(&n))
                || !(8..=20).contains(&controls.numbering.label_size)
                || controls.highlight_appearance.highlight_opacity > 100
                || controls.highlight_appearance.selection_opacity > 100
                || controls.highlight_appearance.line_height > 100
                || value.get("highlight_color").is_some_and(|v| {
                    v.as_str().is_none_or(|s| {
                        ![
                            "gray", "red", "orange", "yellow", "green", "olive", "cyan", "blue",
                            "purple",
                        ]
                        .contains(&s)
                    })
                })
                || value.get("note_marker").is_some_and(|v| {
                    v.as_str()
                        .is_none_or(|s| !["none", "underline", "sideline", "sidemark"].contains(&s))
                })
            {
                return Err(archive::invalid("The reading controls backup is invalid"));
            }
            serde_json::to_value(controls).map_err(io::Error::other)
        }
        Key::FooterCustomText => footer_custom_text::backup_value(&bytes),
        Key::Profiles => {
            let profiles = crate::reading_profiles::decode(value)?;
            serde_json::to_value(profiles).map_err(io::Error::other)
        }
        Key::Gestures => {
            let gestures: BTreeMap<gestures::Gesture, gestures::Binding> =
                serde_json::from_value(value)?;
            if gestures.len() > gestures::Gesture::ALL.len() {
                return Err(archive::invalid("Too many gesture bindings"));
            }
            serde_json::to_value(gestures).map_err(io::Error::other)
        }
        Key::Dictionaries => dictionary_sources::SourcePreferences::backup_value(&bytes),
        Key::Lookup => local_lookup(value),
        Key::Statistics => crate::reading_stats::backup_value(&bytes),
        Key::History => crate::reading_stats::history::backup_value(&bytes),
        Key::Hyphenation => {
            let rules: crate::user_hyphenation::Rules = serde_json::from_value(value)?;
            if !rules.valid() {
                return Err(archive::invalid(
                    "The personal hyphenation backup is invalid",
                ));
            }
            serde_json::to_value(rules).map_err(io::Error::other)
        }
        Key::Browser => bookmark_browser::backup_value(&bytes),
        Key::BookMetadata => bookmark_browser::metadata_backup_value(&bytes),
        Key::Timer => read_alarm::backup_value(&bytes),
        Key::Pace(_) => crate::reading_pace::backup_value(&bytes),
        Key::StyleTweaks => {
            crate::user_styles::storage::decode(&bytes)?;
            Ok(value)
        }
        Key::DocumentDefaults => {
            crate::document_defaults::validate_backup(&bytes)?;
            Ok(value)
        }
        Key::Library => Err(archive::invalid(
            "Library data belongs in the library section",
        )),
    }
}
fn component(state: &Path, key: Key) -> io::Result<Option<Component>> {
    let bytes = match archive::read(state, key) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        result => result?,
    };
    let mut value: Value = serde_json::from_slice(&bytes)?;
    if key == Key::Lookup {
        // Validate the current full object, but never copy online lookup preferences/history.
        let _: lookup_history::LookupHistory = serde_json::from_value(value.clone())?;
        value = lookup_subset(&value);
    }
    Ok(Some(Component {
        key,
        data: canonical(key, value)?,
    }))
}
fn exists(state: &Path, key: Key) -> io::Result<bool> {
    with_storage(|| {
        let root = state.parent().unwrap_or_else(|| Path::new("."));
        match fs::symlink_metadata(root.join(key.relative(state))) {
            Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => Ok(true),
            Ok(_) => Err(archive::invalid(
                "A restore destination is not a regular file",
            )),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e),
        }
    })
}
pub(super) fn recover(state: &Path) -> io::Result<()> {
    with_storage(|| archive::recover(state))
}
impl Reader {
    pub(super) fn reading_backup_bytes(&mut self) -> io::Result<Vec<u8>> {
        self.save()?;
        with_storage(|| self.reading_backup_snapshot())
    }
    fn reading_backup_snapshot(&self) -> io::Result<Vec<u8>> {
        let state = self.library.state_path();
        let mut components = Vec::new();
        for key in Key::SETTINGS.into_iter().filter(|key| *key != Key::Library) {
            if let Some(component) = component(state, key)? {
                components.push(component);
            } else if key == Key::Controls {
                // Legacy save-policy defaults depend on whether a library already
                // exists. Retain the source profile's effective choice explicitly.
                components.push(Component {
                    key,
                    data: canonical(key, serde_json::to_value(self.controls)?)?,
                });
            }
        }
        let pace_directory = state.parent().unwrap().join("reading-pace");
        match fs::symlink_metadata(&pace_directory) {
            Ok(metadata) => {
                if !metadata.is_dir() || metadata.file_type().is_symlink() {
                    return Err(archive::invalid(
                        "The reading-pace folder is not a regular directory",
                    ));
                }
                for entry in fs::read_dir(pace_directory)? {
                    let name = entry?
                        .file_name()
                        .into_string()
                        .map_err(|_| archive::invalid("A reading-pace filename is invalid"))?;
                    let Some(hex) = name.strip_suffix(".json").filter(|s| {
                        s.len() == 16
                            && s.bytes()
                                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                    }) else {
                        continue; // Unrelated files and temporary saves are never exported.
                    };
                    if components.len() >= archive::MAX_COMPONENTS {
                        return Err(archive::invalid(
                            "The backup has too many reading-pace files",
                        ));
                    }
                    let key = Key::Pace(u64::from_str_radix(hex, 16).unwrap());
                    if let Some(component) = component(state, key)? {
                        components.push(component);
                    }
                }
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => (),
            Err(e) => return Err(e),
        }
        components.sort_by_key(|c| c.key);
        Envelope::new(
            serde_json::from_slice(&self.library.backup_bytes()?)?,
            components,
        )
        .bytes()
    }
    pub(super) fn prepare_reading_backup(&self, bytes: &[u8]) -> io::Result<Preview> {
        with_storage(|| self.prepare_reading_backup_inner(bytes))
    }
    fn prepare_reading_backup_inner(&self, bytes: &[u8]) -> io::Result<Preview> {
        let envelope = Envelope::parse(bytes)?;
        let titles = envelope
            .library
            .get("books")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|book| {
                Some((
                    book.get("id")?.as_u64()?,
                    book.get("title")?.as_str()?.chars().take(512).collect(),
                ))
            })
            .collect();
        let library = serde_json::to_vec(&envelope.library)?;
        let (_, summary) = self.library.merged_backup_bytes(&library)?;
        let mut components = Vec::new();
        for component in envelope.components {
            let data = canonical(component.key, component.data)?;
            let existing = exists(self.library.state_path(), component.key)?;
            components.push(Choice {
                key: component.key,
                data,
                existing,
                selected: !existing,
            });
        }
        Ok(Preview {
            library,
            summary,
            components,
            titles,
        })
    }
    pub(super) fn reading_backup_items(&self) -> Vec<Item> {
        let Some(preview) = &self.backup_import else {
            return Vec::new();
        };
        let language = self.controls.interface_language;
        let summary = preview.summary;
        let selected = preview.components.iter().filter(|c| c.selected).count();
        let mut items = vec![
            Item::new(
                language.reading_backup_text("Apply backup"),
                Action::MergeBackup,
            )
            .detail(format!(
                "{}: {} · {}: {selected}",
                language.reading_backup_text("Books to merge"),
                summary.new_books + summary.existing_books,
                language.reading_backup_text("saved components to apply")
            )),
            Item::new(
                language.reading_backup_text("Books and collections · merge"),
                Action::Heading,
            )
            .detail(format!(
                "{}: {} · {}: {} · {}",
                language.reading_backup_text("New"),
                summary.new_books,
                language.reading_backup_text("already known"),
                summary.existing_books,
                language.reading_backup_text("existing positions and settings kept")
            )),
            Item::new(
                format!(
                    "{}: {} · {}: {}",
                    language.reading_backup_text("Bookmarks"),
                    summary.bookmarks_added,
                    language.reading_backup_text("highlights"),
                    summary.highlights_added + summary.pdf_highlights_added
                ),
                Action::Heading,
            )
            .detail(format!(
                "{}: {} · {}: {} · {}: {} · {}: {}",
                language.reading_backup_text("Notes"),
                summary.notes_added,
                language.reading_backup_text("duplicates"),
                summary.duplicates,
                language.reading_backup_text("skipped"),
                summary.skipped,
                language.reading_backup_text("note conflicts"),
                summary.note_conflicts
            )),
            Item::new(
                language.reading_backup_text("Saved components"),
                Action::Heading,
            )
            .detail(language.reading_backup_text(
                "New files are selected. Tap an existing component to replace it.",
            )),
        ];
        items.insert(
            3,
            Item::new(
                language.reading_backup_text("Collections and custom navigation"),
                Action::Heading,
            )
            .detail(format!(
                "{}: {} · {}: {} · {}: {} · {}: {}",
                language.reading_backup_text("Collections"),
                summary.collections_added,
                language.reading_backup_text("memberships"),
                summary.memberships_added,
                language.reading_backup_text("custom chapters"),
                summary.custom_chapters_added,
                language.reading_backup_text("flow boundaries"),
                summary.custom_flow_points_added
            )),
        );
        for (index, component) in preview.components.iter().enumerate() {
            let detail = match (component.existing, component.selected) {
                (true, true) => "Selected · replace existing local data",
                (true, false) => "Skipped · keep existing local data",
                (false, true) => "Selected · add missing local data",
                (false, false) => "Skipped · leave missing",
            };
            let title = match component.key {
                Key::Pace(book) => preview.titles.get(&book).map_or_else(
                    || {
                        language
                            .reading_backup_text("Reading-time estimates")
                            .into()
                    },
                    |title| format!("{} · {title}", language.reading_backup_text("Reading pace")),
                ),
                _ => language
                    .reading_backup_text(&component.key.label())
                    .to_owned(),
            };
            items.push(
                Item::new(title, Action::ToggleBackupComponent(index))
                    .detail(language.reading_backup_text(detail)),
            );
        }
        if preview.components.is_empty() {
            items.push(
                Item::new(
                    language.reading_backup_text("Legacy library-only backup"),
                    Action::Heading,
                )
                .detail(language.reading_backup_text(
                    "This file contains no global controls, vocabulary or dated history.",
                )),
            );
        }
        items.push(
            Item::new(
                language.reading_backup_text("Restore returns to the library tools"),
                Action::Heading,
            )
            .detail(
                language
                    .reading_backup_text("Your documents and current network accounts are kept."),
            ),
        );
        items
    }
    pub(super) fn toggle_backup_component(&mut self, index: usize) -> Result<()> {
        let component = self
            .backup_import
            .as_mut()
            .and_then(|p| p.components.get_mut(index))
            .ok_or("Preview this backup again")?;
        component.selected = !component.selected;
        Ok(())
    }
    pub(super) fn apply_reading_backup(&mut self) -> io::Result<()> {
        let preview = self
            .backup_import
            .take()
            .ok_or_else(|| archive::invalid("Preview the backup again"))?;
        self.save()?;
        let (library, _) = self.library.merged_backup_bytes(&preview.library)?;
        let state = self.library.state_path().to_owned();
        let baseline = self.library.v2_disk_baseline()?;
        let documents = self.library.documents_path().to_owned();
        let fonts = self.fonts.clone();
        let mut updates = vec![(Key::Library, library)];
        let mut count = 0;
        for component in preview.components.into_iter().filter(|c| c.selected) {
            let mut value = canonical(component.key, component.data)?;
            if component.key == Key::Lookup {
                match with_storage(|| archive::read(&state, Key::Lookup)) {
                    Ok(bytes) => {
                        let existing: Value = serde_json::from_slice(&bytes)?;
                        let _: lookup_history::LookupHistory =
                            serde_json::from_value(existing.clone())?;
                        for key in ["online", "online_languages"] {
                            if let Some(prior) = existing.get(key) {
                                value
                                    .as_object_mut()
                                    .unwrap()
                                    .insert(key.into(), prior.clone());
                            }
                        }
                    }
                    Err(e) if e.kind() == io::ErrorKind::NotFound => (),
                    Err(e) => return Err(e),
                }
            }
            let bytes = serde_json::to_vec(&value)?;
            if bytes.len() as u64 > component.key.limit() {
                return Err(archive::invalid("The restored component exceeds its limit"));
            }
            updates.push((component.key, bytes));
            count += 1;
        }
        let mut restored = with_storage(|| {
            if baseline.is_some() {
                archive::require_no_pending(&state)?;
            }
            archive::transaction(&state, &updates, || {
                if let Some(expected) = baseline {
                    archive::verify_captured_state(&state, &expected)?;
                }
                Self::open_data(&documents, &state, fonts)
            })
        })?;
        restored.show(Screen::ReadingData);
        restored.notice = Some(format!("Reading data restored · {count} saved components"));
        *self = restored;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
