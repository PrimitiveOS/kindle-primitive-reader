use super::*;
use crate::library::DictionaryFuzzySearch;

const MAX_PREFERENCES_BYTES: usize = 64 * 1024;

pub(super) fn valid_dictionary_key(path: &Path) -> bool {
    path.to_str()
        .is_some_and(|s| !s.is_empty() && s.len() <= 4096 && !s.chars().any(char::is_control))
        && !path.components().any(|c| {
            matches!(
                c,
                std::path::Component::ParentDir | std::path::Component::CurDir
            )
        })
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub(super) struct SourcePreferences {
    order: Vec<PathBuf>,
    pub(super) disabled: Vec<PathBuf>,
    #[serde(skip_serializing_if = "dictionary_presets::Settings::is_empty")]
    pub(super) presets: dictionary_presets::Settings,
    disable_word_forms: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    enable_fuzzy_search: Option<bool>,
    pub(super) large_window: bool,
    pub(super) justify_text: bool,
}

impl Default for SourcePreferences {
    fn default() -> Self {
        Self {
            order: Vec::new(),
            disabled: Vec::new(),
            presets: dictionary_presets::Settings::default(),
            disable_word_forms: false,
            enable_fuzzy_search: None,
            large_window: false,
            justify_text: true,
        }
    }
}

impl SourcePreferences {
    pub(super) fn word_forms_default(&self) -> bool {
        !self.disable_word_forms
    }

    pub(super) fn fuzzy_search_default(&self) -> bool {
        self.enable_fuzzy_search
            .unwrap_or_else(|| self.word_forms_default())
    }

    pub(super) fn paths(&self) -> (&[PathBuf], &[PathBuf]) {
        (&self.order, &self.disabled)
    }
    pub(super) fn backup_value(bytes: &[u8]) -> io::Result<serde_json::Value> {
        let value: serde_json::Value = serde_json::from_slice(bytes)?;
        let settings: Self = if value.is_object() {
            serde_json::from_value(value)?
        } else {
            let prior: Option<PathBuf> = serde_json::from_value(value)?;
            Self {
                order: prior.into_iter().collect(),
                ..Self::default()
            }
        };
        if settings.order.len() > 512
            || settings.disabled.len() > 512
            || settings
                .order
                .iter()
                .chain(&settings.disabled)
                .any(|path| !valid_dictionary_key(path))
            || !settings.presets.is_valid()
            || serde_json::to_vec(&settings)?.len() > MAX_PREFERENCES_BYTES
        {
            return Err(io::Error::other(
                "The dictionary preferences backup is invalid",
            ));
        }
        serde_json::to_value(settings).map_err(io::Error::other)
    }

    pub(super) fn load(path: &Path) -> Self {
        let Ok(bytes) = read_import_file(path, MAX_PREFERENCES_BYTES as u64) else {
            return Self::default();
        };
        if let Ok(mut settings) = serde_json::from_slice::<Self>(&bytes) {
            if !settings.presets.is_valid() {
                settings.presets = dictionary_presets::Settings::default();
            }
            return settings;
        }
        // Earlier releases persisted either null or one preferred relative path.
        Self {
            order: serde_json::from_slice::<Option<PathBuf>>(&bytes)
                .ok()
                .flatten()
                .into_iter()
                .collect(),
            ..Self::default()
        }
    }
}

pub(super) enum SourceAction {
    Open(usize),
    Toggle,
    Earlier,
    Later,
    ForBook,
    ToggleForBook,
    ResetBookSources,
    EarlierForBook,
    LaterForBook,
    ResetOrder,
    DefaultWordForms,
    BookWordForms,
    ResetBookWordForms,
    DefaultFuzzySearch,
    BookFuzzySearch,
    ResetBookFuzzySearch,
    LargeWindow,
    JustifyText,
}

fn toggle(paths: &mut Vec<PathBuf>, key: &Path) {
    if paths.iter().any(|path| path == key) {
        paths.retain(|path| path != key);
    } else {
        paths.push(key.to_owned());
    }
}

impl Reader {
    pub(super) fn book_dictionaries(&self) -> &[PathBuf] {
        self.open_book_id()
            .and_then(|id| self.library.state(id))
            .map_or(&[], |book| book.preferred_dictionaries.as_slice())
    }

    pub(super) fn book_dictionary_exclusions(&self) -> &[PathBuf] {
        self.open_book_id()
            .and_then(|id| self.library.state(id))
            .map_or(&[], |book| book.excluded_dictionaries.as_slice())
    }

    fn book_word_forms(&self) -> Option<bool> {
        self.open_book_id()
            .and_then(|id| self.library.state(id))
            .and_then(|s| s.dictionary_word_forms)
    }

    pub(super) fn word_forms_enabled(&self) -> bool {
        self.book_word_forms()
            .unwrap_or_else(|| self.dictionary_settings.word_forms_default())
    }

    fn book_fuzzy_search(&self) -> Option<bool> {
        let state = self.library.state(self.open_book_id()?)?;
        state
            .dictionary_fuzzy_search
            .override_for(state.dictionary_word_forms)
    }

    pub(super) fn fuzzy_search_enabled(&self) -> bool {
        self.book_fuzzy_search()
            .unwrap_or_else(|| self.dictionary_settings.fuzzy_search_default())
    }

    #[cfg(test)]
    pub(super) fn lookup_sources(&self, word: &str) -> io::Result<Vec<dictionary::LookupEntry>> {
        self.lookup_source_report(word).map(|report| report.entries)
    }

    pub(super) fn lookup_source_report(&self, word: &str) -> io::Result<dictionary::LookupReport> {
        self.lookup_with_dictionary_preset(word, None)
    }

    pub(super) fn lookup_with_dictionary_preset(
        &self,
        word: &str,
        preset: Option<&[PathBuf]>,
    ) -> io::Result<dictionary::LookupReport> {
        let prefs = &self.dictionary_settings;
        self.dictionaries.lookup_report(
            word,
            dictionary::SourceScope {
                preferred: self.book_dictionaries(),
                excluded: self.book_dictionary_exclusions(),
                order: &prefs.order,
                disabled: &prefs.disabled,
                preset,
            },
            self.word_forms_enabled(),
        )
    }

    pub(super) fn start_spelling_lookup(&mut self, word: &str, preset: Option<Vec<PathBuf>>) {
        self.spelling_lookup = None;
        if !self.definitions.is_empty() || !dictionary::spelling_eligible(word) {
            return;
        }
        let sources = self.dictionaries.clone();
        let word = word.to_owned();
        let book = self.book_dictionaries().to_vec();
        let excluded = self.book_dictionary_exclusions().to_vec();
        let (order, disabled) = self.dictionary_settings.paths();
        let (order, disabled) = (order.to_vec(), disabled.to_vec());
        match dictionary_panel::SpellingLookup::start(move |cancel| {
            sources.suggested_definitions(
                &word,
                dictionary::SourceScope {
                    preferred: &book,
                    excluded: &excluded,
                    order: &order,
                    disabled: &disabled,
                    preset: preset.as_deref(),
                },
                cancel,
            )
        }) {
            Ok(job) => self.spelling_lookup = Some(job),
            Err(error) => {
                self.notice = Some(format!("Could not start dictionary search ({error})"))
            }
        }
    }

    fn sort_dictionaries(&mut self) {
        self.installed_dictionaries.sort_by(|a, b| {
            let rank = |key: &Path| {
                self.dictionary_settings
                    .order
                    .iter()
                    .position(|path| path == key)
                    .unwrap_or(usize::MAX)
            };
            rank(&a.path)
                .cmp(&rank(&b.path))
                .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
                .then_with(|| a.path.is_absolute().cmp(&b.path.is_absolute()))
                .then_with(|| a.path.cmp(&b.path))
        });
    }

    pub(super) fn refresh_installed_dictionaries(&mut self) -> Result<()> {
        self.installed_dictionaries = with_storage(|| self.dictionaries.installed())?;
        self.sort_dictionaries();
        Ok(())
    }

    pub(super) fn save_source_preferences(&mut self, next: SourcePreferences) -> Result<()> {
        let bytes = serde_json::to_vec(&next)?;
        if bytes.len() > MAX_PREFERENCES_BYTES || !next.presets.is_valid() {
            return Err("Dictionary preferences exceed Reader's limits".into());
        }
        primitive_app::save(&self.dictionary_preference, &bytes)?;
        self.dictionary_settings = next;
        self.sort_dictionaries();
        Ok(())
    }

    pub(super) fn open_sources(&mut self) -> Result<()> {
        self.refresh_installed_dictionaries()?;
        self.dictionary_origin = self.screen;
        self.show(Screen::Dictionaries);
        Ok(())
    }

    pub(super) fn source_rows(&self) -> Vec<Item> {
        let mut rows = vec![
            Item::new(
                "Large window",
                Action::DictionarySource(SourceAction::LargeWindow),
            )
            .detail(if self.dictionary_settings.large_window {
                "On"
            } else {
                "Off"
            }),
            Item::new(
                "Justify text",
                Action::DictionarySource(SourceAction::JustifyText),
            )
            .detail(if self.dictionary_settings.justify_text {
                "On"
            } else {
                "Off"
            }),
        ];
        rows.extend(
            self.installed_dictionaries
                .iter()
                .enumerate()
                .map(|(i, source)| {
                    let detail = if self.book_dictionary_exclusions().contains(&source.path) {
                        "Disabled for this book".to_owned()
                    } else if let Some(rank) = self
                        .book_dictionaries()
                        .iter()
                        .position(|key| key == &source.path)
                    {
                        format!("Book preference {}", rank + 1)
                    } else if self.dictionary_settings.disabled.contains(&source.path) {
                        "Disabled".to_owned()
                    } else {
                        "Enabled".to_owned()
                    };
                    Item::new(
                        &source.title,
                        Action::DictionarySource(SourceAction::Open(i)),
                    )
                    .detail(detail)
                }),
        );
        if self.installed_dictionaries.is_empty() {
            rows.push(
                Item::new("Add StarDict dictionaries", Action::Heading)
                    .detail("Copy dictionaries to reader/data/dictionaries"),
            );
        } else if !self.dictionary_settings.order.is_empty() {
            rows.push(Item::new(
                "Reset dictionary order",
                Action::DictionarySource(SourceAction::ResetOrder),
            ));
        }
        rows.push(Item::new(
            "Dictionary presets",
            Action::DictionaryPreset(dictionary_presets::Command::Open),
        ));
        if !self.book_dictionaries().is_empty() || !self.book_dictionary_exclusions().is_empty() {
            rows.push(Item::new(
                "Reset book dictionary choices",
                Action::DictionarySource(SourceAction::ResetBookSources),
            ));
        }
        if !self.installed_dictionaries.is_empty() {
            rows.push(
                Item::new(
                    "Word forms by default",
                    Action::DictionarySource(SourceAction::DefaultWordForms),
                )
                .detail(if self.dictionary_settings.disable_word_forms {
                    "Off"
                } else {
                    "On · inflections"
                }),
            );
            if self.open_book_id().is_some() {
                let state = match (self.word_forms_enabled(), self.book_word_forms().is_none()) {
                    (true, true) => "On · default",
                    (false, true) => "Off · default",
                    (true, false) => "On",
                    (false, false) => "Off",
                };
                rows.push(
                    Item::new(
                        "Word forms in this book",
                        Action::DictionarySource(SourceAction::BookWordForms),
                    )
                    .detail(state),
                );
                if self.book_word_forms().is_some() {
                    rows.push(Item::new(
                        "Use the default word forms setting",
                        Action::DictionarySource(SourceAction::ResetBookWordForms),
                    ));
                }
            }
            rows.push(
                Item::new(
                    "Fuzzy search by default",
                    Action::DictionarySource(SourceAction::DefaultFuzzySearch),
                )
                .detail(if self.dictionary_settings.fuzzy_search_default() {
                    "On · similar spellings"
                } else {
                    "Off"
                }),
            );
            if self.open_book_id().is_some() {
                let state = match (
                    self.fuzzy_search_enabled(),
                    self.book_fuzzy_search().is_none(),
                ) {
                    (true, true) => "On · default",
                    (false, true) => "Off · default",
                    (true, false) => "On",
                    (false, false) => "Off",
                };
                rows.push(
                    Item::new(
                        "Fuzzy search in this book",
                        Action::DictionarySource(SourceAction::BookFuzzySearch),
                    )
                    .detail(state),
                );
                if self.book_fuzzy_search().is_some() {
                    rows.push(Item::new(
                        "Use the default fuzzy search setting",
                        Action::DictionarySource(SourceAction::ResetBookFuzzySearch),
                    ));
                }
            }
        }
        rows
    }

    pub(super) fn source_options(&self, i: usize) -> Vec<Item> {
        let Some(source) = self.installed_dictionaries.get(i) else {
            return Vec::new();
        };
        let mut rows = vec![
            Item::new(&source.title, Action::Heading),
            Item::new(
                if self.dictionary_settings.disabled.contains(&source.path) {
                    "Enable by default"
                } else {
                    "Disable by default"
                },
                Action::DictionarySource(SourceAction::Toggle),
            ),
        ];
        let book_order = self.book_dictionaries();
        let rank = book_order.iter().position(|key| key == &source.path);
        if self.open_book_id().is_some() {
            let excluded = self.book_dictionary_exclusions().contains(&source.path);
            if excluded
                || rank.is_some()
                || !self.dictionary_settings.disabled.contains(&source.path)
            {
                rows.push(Item::new(
                    if excluded {
                        "Re-enable for this book"
                    } else {
                        "Disable for this book"
                    },
                    Action::DictionarySource(SourceAction::ToggleForBook),
                ));
            }
            if !excluded {
                rows.push(
                    Item::new(
                        if rank.is_some() {
                            "Remove book preference"
                        } else {
                            "Prefer for this book"
                        },
                        Action::DictionarySource(SourceAction::ForBook),
                    )
                    .detail("Book preferences take priority over the global list"),
                );
                if let Some(rank) = rank {
                    if rank > 0 {
                        rows.push(Item::new(
                            "Earlier for this book",
                            Action::DictionarySource(SourceAction::EarlierForBook),
                        ));
                    }
                    if rank + 1 < book_order.len() {
                        rows.push(Item::new(
                            "Later for this book",
                            Action::DictionarySource(SourceAction::LaterForBook),
                        ));
                    }
                }
            }
        }
        if i > 0 {
            rows.push(Item::new(
                "Earlier in default order",
                Action::DictionarySource(SourceAction::Earlier),
            ));
        }
        if i + 1 < self.installed_dictionaries.len() {
            rows.push(Item::new(
                "Later in default order",
                Action::DictionarySource(SourceAction::Later),
            ));
        }
        rows
    }

    pub(super) fn change_source(&mut self, action: SourceAction) -> Result<()> {
        if let SourceAction::Open(i) = action {
            if self.installed_dictionaries.get(i).is_none() {
                return Err("Dictionary unavailable".into());
            }
            self.show(Screen::DictionaryOptions(i));
            return Ok(());
        }
        if matches!(
            action,
            SourceAction::BookWordForms
                | SourceAction::ResetBookWordForms
                | SourceAction::BookFuzzySearch
                | SourceAction::ResetBookFuzzySearch
        ) {
            let id = self.open_book_id().ok_or("Open a book first")?;
            let word_forms = self.word_forms_enabled();
            let fuzzy_search = self.fuzzy_search_enabled();
            let state = self.library.state_mut(id).ok_or("Book unavailable")?;
            let old = (state.dictionary_word_forms, state.dictionary_fuzzy_search);
            match action {
                SourceAction::BookWordForms | SourceAction::ResetBookWordForms => {
                    // Freeze an old coupled override before changing just Word forms.
                    if state.dictionary_fuzzy_search == DictionaryFuzzySearch::Legacy {
                        state.dictionary_fuzzy_search = DictionaryFuzzySearch::from_override(old.0);
                    }
                    state.dictionary_word_forms =
                        matches!(action, SourceAction::BookWordForms).then_some(!word_forms);
                }
                SourceAction::BookFuzzySearch | SourceAction::ResetBookFuzzySearch => {
                    state.dictionary_fuzzy_search = DictionaryFuzzySearch::from_override(
                        matches!(action, SourceAction::BookFuzzySearch).then_some(!fuzzy_search),
                    );
                }
                _ => unreachable!(),
            }
            self.mark_dirty();
            if let Err(error) = self.save_library() {
                let state = self.library.state_mut(id).unwrap();
                (state.dictionary_word_forms, state.dictionary_fuzzy_search) = old;
                return Err(error.into());
            }
            return Ok(());
        }
        if matches!(action, SourceAction::ResetBookSources) {
            let id = self.open_book_id().ok_or("Open a book first")?;
            let state = self.library.state_mut(id).ok_or("Book unavailable")?;
            let preferred = std::mem::take(&mut state.preferred_dictionaries);
            let excluded = std::mem::take(&mut state.excluded_dictionaries);
            self.mark_dirty();
            if let Err(error) = self.save_library() {
                let state = self.library.state_mut(id).unwrap();
                state.preferred_dictionaries = preferred;
                state.excluded_dictionaries = excluded;
                return Err(error.into());
            }
            return Ok(());
        }
        let mut next = self.dictionary_settings.clone();
        let mut selected_key = None;
        if matches!(action, SourceAction::DefaultWordForms) {
            next.enable_fuzzy_search = Some(next.fuzzy_search_default());
            next.disable_word_forms = !next.disable_word_forms;
        } else if matches!(action, SourceAction::DefaultFuzzySearch) {
            next.enable_fuzzy_search = Some(!next.fuzzy_search_default());
        } else if matches!(action, SourceAction::ResetOrder) {
            next.order.clear();
        } else if matches!(action, SourceAction::LargeWindow) {
            next.large_window = !next.large_window;
        } else if matches!(action, SourceAction::JustifyText) {
            next.justify_text = !next.justify_text;
        } else {
            let Screen::DictionaryOptions(i) = self.screen else {
                return Err("Choose a dictionary first".into());
            };
            let key = self
                .installed_dictionaries
                .get(i)
                .ok_or("Dictionary unavailable")?
                .path
                .clone();
            match action {
                SourceAction::ForBook
                | SourceAction::ToggleForBook
                | SourceAction::EarlierForBook
                | SourceAction::LaterForBook => {
                    let id = self.open_book_id().ok_or("Open a book first")?;
                    let state = self.library.state_mut(id).ok_or("Book unavailable")?;
                    let is_excluded = matches!(action, SourceAction::ToggleForBook);
                    if !is_excluded && state.excluded_dictionaries.contains(&key) {
                        return Err("Re-enable this dictionary for the book first".into());
                    }
                    let affected = if is_excluded {
                        &mut state.excluded_dictionaries
                    } else {
                        &mut state.preferred_dictionaries
                    };
                    let previous = affected.clone();
                    if matches!(action, SourceAction::ForBook | SourceAction::ToggleForBook) {
                        toggle(affected, &key);
                    } else {
                        let rank = affected
                            .iter()
                            .position(|k| *k == key)
                            .ok_or("Choose a preferred dictionary")?;
                        let dest = if matches!(action, SourceAction::EarlierForBook) {
                            rank.checked_sub(1)
                        } else {
                            rank.checked_add(1)
                        };
                        let dest = dest
                            .filter(|i| *i < affected.len())
                            .ok_or("End of book preferences")?;
                        affected.swap(rank, dest);
                    }
                    self.mark_dirty();
                    if let Err(error) = self.save_library() {
                        let state = self.library.state_mut(id).unwrap();
                        if is_excluded {
                            state.excluded_dictionaries = previous;
                        } else {
                            state.preferred_dictionaries = previous;
                        }
                        return Err(error.into());
                    }
                    return Ok(());
                }
                SourceAction::Toggle => toggle(&mut next.disabled, &key),
                SourceAction::Earlier | SourceAction::Later => {
                    let j = if matches!(action, SourceAction::Earlier) {
                        i.checked_sub(1)
                    } else {
                        i.checked_add(1)
                    };
                    let j = j
                        .filter(|j| *j < self.installed_dictionaries.len())
                        .ok_or("End of the list")?;
                    let mut ordered: Vec<_> = self
                        .installed_dictionaries
                        .iter()
                        .map(|row| row.path.clone())
                        .collect();
                    ordered.swap(i, j);
                    // Keep unplugged or temporarily absent dictionaries in the saved order.
                    for old in next.order.drain(..) {
                        if !ordered.contains(&old) {
                            ordered.push(old);
                        }
                    }
                    next.order = ordered;
                }
                SourceAction::ResetOrder
                | SourceAction::ResetBookSources
                | SourceAction::Open(_)
                | SourceAction::DefaultWordForms
                | SourceAction::BookWordForms
                | SourceAction::ResetBookWordForms
                | SourceAction::DefaultFuzzySearch
                | SourceAction::BookFuzzySearch
                | SourceAction::ResetBookFuzzySearch
                | SourceAction::LargeWindow
                | SourceAction::JustifyText => unreachable!(),
            }
            selected_key = Some(key);
        }
        self.save_source_preferences(next)?;
        if let Some(key) = selected_key
            && let Some(index) = self
                .installed_dictionaries
                .iter()
                .position(|r| r.path == key)
        {
            self.show(Screen::DictionaryOptions(index));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::Fixture;

    fn row(app: &mut Reader, label: &str) {
        let index = app
            .items()
            .iter()
            .position(|item| item.title == label)
            .unwrap();
        let y = (LIST_Y + (index % ROWS_PER_PAGE) as i32 * ROW_HEIGHT + ROW_HEIGHT / 2) as f32;
        app.list_page = index / ROWS_PER_PAGE;
        app.input(Input::Tap(180.0, y));
    }

    fn back(app: &mut Reader) {
        let rect = primitive_ui::back_button(WIDTH as i32);
        app.input(Input::Tap(
            (rect.x + rect.w / 2) as f32,
            (rect.y + rect.h / 2) as f32,
        ));
    }

    fn select_for_dictionary(app: &mut Reader, word: &str) {
        app.controls.dictionary_on_single_word = Some(false);
        let reading = &app.active.as_ref().unwrap().reading;
        let block = reading
            .probe
            .source
            .blocks
            .iter()
            .find(|b| b.text.contains(word))
            .unwrap();
        let offset = block.text.find(word).unwrap();
        let range = reading
            .probe
            .select_range(
                &reading.probe.anchor(block.key, offset),
                &reading.probe.anchor(block.key, offset + word.len()),
            )
            .unwrap();
        let bounds = reading.highlight_rects(&range)[0];
        app.input(Input::LongTap(
            (bounds.x + bounds.width / 2) as f32,
            (bounds.y + bounds.height / 2) as f32,
        ));
        assert_eq!(app.selection.as_ref().unwrap().text, word);
        let rect = app.selection_button(selection_popup::SelectionAction::Dictionary);
        app.input(Input::Tap(
            (rect.x + rect.w / 2) as f32,
            (rect.y + rect.h / 2) as f32,
        ));
    }

    #[test]
    fn suggestions_arrive_in_the_open_popup_and_can_be_dismissed() {
        let fixture = Fixture::new();
        let dir = fixture.root.join("dictionaries");
        fs::create_dir(&dir).unwrap();
        crate::dictionary::tests::write_dictionary(
            &dir,
            "Beacon",
            "sametypesequence=m\n",
            &[("stone", b"A fragment of rock."), ("store", b"A supply.")],
        );
        let font = crate::test_fixture("fonts/NotoSans-Regular.ttf");
        let mut reader = Reader::open(&fixture.root, &fixture.state, vec![font.clone()]).unwrap();
        let passage = fixture.root.join("Cliff path.xhtml");
        fs::write(&passage, r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Field notebook</title></head><body>
            <h1>Field notebook</h1><p>The signal lamp stood beside the cliff path. A narrow band of quartz crossed its foundation.</p>
            <p>The surveyor had pencilled a note on the map: “Check the sotne at low tide.”</p>
            </body></html>"#).unwrap();
        reader.open_path(&passage).unwrap();
        let origin = reader.current_history_location();
        select_for_dictionary(&mut reader, "sotne");
        assert_eq!(reader.screen, Screen::Definition);
        assert!(reader.spelling_lookup.is_some());
        assert!(reader.definitions.is_empty());
        let until = Instant::now() + Duration::from_secs(10);
        while reader.spelling_lookup.is_some() && Instant::now() < until {
            reader.tick(Instant::now());
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(reader.definitions.first().unwrap().word, "stone");
        crate::app::tests::capture_test_screen(
            &reader,
            std::slice::from_ref(&font),
            "READER_SPELLING_POPUP_SCREEN",
            false,
        );
        let dismiss = |reader: &mut Reader| {
            let done = reader.definition_layout().1;
            reader.input(Input::Tap(
                (done.x + done.w / 2) as f32,
                (done.y + done.h / 2) as f32,
            ));
        };
        dismiss(&mut reader);
        assert_eq!(reader.screen, Screen::Book);
        reader.look_up("stnoe").unwrap();
        assert!(reader.spelling_lookup.is_some());
        dismiss(&mut reader);
        assert!(reader.spelling_lookup.is_none());
        reader.look_up("store").unwrap();
        assert!(reader.spelling_lookup.is_none());
        assert_eq!(reader.definitions.first().unwrap().word, "store");
        reader.tick(Instant::now() + Duration::from_secs(2));
        assert_eq!(reader.definitions.first().unwrap().word, "store");
        dismiss(&mut reader);
        reader.look_up("sotne").unwrap();
        reader.look_up("store").unwrap();
        assert!(reader.spelling_lookup.is_none());
        dismiss(&mut reader);
        assert_eq!(reader.definition_word, "sotne");
        assert!(reader.spelling_lookup.is_some());
        reader.input(Input::Sleep);
        assert!(reader.spelling_lookup.is_none());
        reader.input(Input::Wake);
        dismiss(&mut reader);
        assert_eq!(reader.current_history_location(), origin);
    }

    #[test]
    fn inflection_preferences_follow_the_open_book_and_survive_reopening() {
        let f = Fixture::new();
        let dictionaries = f.root.join("dictionaries");
        fs::create_dir(&dictionaries).unwrap();
        crate::dictionary::tests::write_dictionary(
            &dictionaries,
            "Harbour Dictionary",
            "sametypesequence=m\n",
            &[("carry", b"To take something from one place to another.")],
        );
        let source = f.root.join("Harbour log.xhtml");
        fs::write(
            &source,
            include_str!("../../tests/fixtures/dictionary-word-forms.xhtml"),
        )
        .unwrap();
        let second = f.root.join("Other.epub");
        fs::copy(crate::test_fixture("night-landing.epub"), &second).unwrap();
        let font = crate::test_fixture("fonts/NotoSans-Regular.ttf");
        let open = || Reader::open(&f.root, &f.state, vec![font.clone()]).unwrap();
        let mut app = open();
        app.open_path(&source).unwrap();
        let passage = app.current_history_location();
        app.activate(Action::ShowDictionaries).unwrap();
        assert_eq!(app.lookup_sources("carried").unwrap()[0].word, "carry");
        row(&mut app, "Word forms in this book");
        assert!(app.lookup_sources("carried").unwrap().is_empty());
        drop(app);
        let mut app = open();
        app.open_path(&source).unwrap();
        assert!(app.lookup_sources("carried").unwrap().is_empty());
        app.open_path(&second).unwrap();
        assert_eq!(app.lookup_sources("carried").unwrap()[0].word, "carry");
        app.activate(Action::ShowDictionaries).unwrap();
        row(&mut app, "Word forms by default");
        app.open_path(&source).unwrap();
        app.activate(Action::ShowDictionaries).unwrap();
        row(&mut app, "Use the default word forms setting");
        assert!(app.lookup_sources("carried").unwrap().is_empty());
        row(&mut app, "Word forms in this book");
        let before = app.lookup_history.vocabulary.clone();
        back(&mut app);
        assert_eq!(app.screen, Screen::Book);
        select_for_dictionary(&mut app, "carried");
        assert!(app.definition_over_book());
        assert_eq!(app.definition_word, "carried");
        assert_eq!(app.definitions[0].word, "carry");
        assert_eq!(app.lookup_history.vocabulary, before);
        crate::app::tests::capture_test_screen(
            &app,
            std::slice::from_ref(&font),
            "READER_WORD_FORM_SCREEN",
            false,
        );
        let done = app.definition_layout().1;
        app.input(Input::Tap(
            (done.x + done.w / 2) as f32,
            (done.y + done.h / 2) as f32,
        ));
        assert_eq!(app.screen, Screen::Book);
        assert_eq!(app.current_history_location(), passage);
    }

    #[test]
    fn independent_fuzzy_search_in_reflow_and_pdf() {
        let fixture = Fixture::new();
        let dictionaries = fixture.root.join("dictionaries");
        fs::create_dir(&dictionaries).unwrap();
        crate::dictionary::tests::write_dictionary(
            &dictionaries,
            "Harbour Dictionary",
            "sametypesequence=m\n",
            &[
                ("carry", b"To transport."),
                ("stone", b"A fragment of rock."),
            ],
        );
        let pdf = fixture.root.join("Cliff Survey.pdf");
        let content = "BT /F1 14 Tf 55 60 Td (sotne carried stone) Tj ET";
        let objects = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
            "<< /Type /Pages /Count 1 /Kids [3 0 R] /Resources << /Font << /F1 5 0 R >> >> >>"
                .to_owned(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 240 120] /Contents 4 0 R >>".to_owned(),
            format!(
                "<< /Length {} >>\nstream\n{content}\nendstream",
                content.len()
            ),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_owned(),
        ];
        fs::write(&pdf, crate::fixed::tests::pdf_objects(&objects)).unwrap();
        let fonts = vec![
            crate::test_fixture("fonts/NotoSans-Regular.ttf"),
            crate::test_fixture("fonts/NotoSans-Bold.ttf"),
        ];
        let open = || Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
        let mut app = open();
        app.open_path(&fixture.book).unwrap();
        let epub_origin = app.current_history_location();
        assert!(app.fuzzy_search_enabled() && app.word_forms_enabled());
        app.activate(Action::ShowDictionaries).unwrap();
        row(&mut app, "Word forms in this book");
        assert!(app.fuzzy_search_enabled());
        assert!(app.lookup_sources("carried").unwrap().is_empty());
        back(&mut app);
        app.look_up("sotne").unwrap();
        assert!(app.spelling_lookup.is_some());
        dismiss_definition(&mut app);
        assert!(app.spelling_lookup.is_none());
        app.activate(Action::ShowDictionaries).unwrap();
        row(&mut app, "Fuzzy search in this book");
        assert!(!app.fuzzy_search_enabled());
        back(&mut app);
        app.look_up("sotne").unwrap();
        assert!(app.spelling_lookup.is_none() && app.definitions.is_empty());
        assert_eq!(app.input(Input::PageBack), Change::Partial);
        assert_eq!(app.current_history_location(), epub_origin);

        app.open_path(&pdf).unwrap();
        let pdf_origin = app.current_history_location();
        assert!(app.fuzzy_search_enabled() && app.word_forms_enabled());
        app.activate(Action::ShowDictionaries).unwrap();
        row(&mut app, "Fuzzy search by default");
        assert!(!app.fuzzy_search_enabled());
        assert_eq!(app.lookup_sources("carried").unwrap()[0].word, "carry");
        back(&mut app);
        let layer = app.fixed.as_ref().unwrap().reading.page_text(0).unwrap();
        let bounds = layer.words[0].bounds;
        let point = hayro::kurbo::Point::new(
            f64::from((bounds.x0 + bounds.x1) / 2.0),
            f64::from((bounds.y0 + bounds.y1) / 2.0),
        );
        let selection = PdfSelection::at(0, point, layer).unwrap();
        assert_eq!(selection.text(), "sotne");
        app.pdf_selection = Some(selection.clone());
        app.look_up_selection().unwrap();
        assert!(app.spelling_lookup.is_none() && app.definitions.is_empty());
        assert_eq!(app.input(Input::PageBack), Change::Partial);
        assert_eq!(app.current_history_location(), pdf_origin);

        app.activate(Action::ShowDictionaries).unwrap();
        row(&mut app, "Fuzzy search in this book");
        assert!(app.fuzzy_search_enabled());
        crate::app::tests::capture_test_screen(
            &app,
            &fonts,
            "READER_FUZZY_SETTINGS_CAPTURE",
            false,
        );
        back(&mut app);
        app.pdf_selection = Some(selection);
        app.look_up_selection().unwrap();
        assert!(app.spelling_lookup.is_some());
        let until = Instant::now() + Duration::from_secs(10);
        while app.spelling_lookup.is_some() && Instant::now() < until {
            app.tick(Instant::now());
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(app.definitions[0].word, "stone");
        crate::app::tests::capture_test_screen(&app, &fonts, "READER_FUZZY_RESULT_CAPTURE", false);
        dismiss_definition(&mut app);
        app.look_up("stone").unwrap();
        assert!(app.spelling_lookup.is_none());
        assert_eq!(app.definitions[0].word, "stone");
        dismiss_definition(&mut app);
        app.look_up("読めなかった").unwrap();
        assert!(app.spelling_lookup.is_none() && app.definitions.is_empty());
        dismiss_definition(&mut app);
        assert_eq!(app.current_history_location(), pdf_origin);
        app.save().unwrap();
        drop(app);

        let mut app = open();
        app.open_path(&fixture.book).unwrap();
        assert!(!app.fuzzy_search_enabled() && !app.word_forms_enabled());
        assert_eq!(app.current_history_location(), epub_origin);
        app.open_path(&pdf).unwrap();
        assert!(app.fuzzy_search_enabled() && app.word_forms_enabled());
        assert_eq!(app.current_history_location(), pdf_origin);
        app.activate(Action::ShowDictionaries).unwrap();
        row(&mut app, "Use the default fuzzy search setting");
        assert!(!app.fuzzy_search_enabled());
        assert!(app.word_forms_enabled());
    }

    #[test]
    fn independent_fuzzy_search_migrates_the_original_paired_choices() {
        let fixture = Fixture::new();
        let mut app = fixture.app();
        app.open_path(&fixture.book).unwrap();
        let edition = app.open_book_id().unwrap();
        app.library
            .state_mut(edition)
            .unwrap()
            .dictionary_word_forms = Some(true);
        app.mark_dirty();
        app.save_library().unwrap();
        fs::write(&app.dictionary_preference, r#"{"disable_word_forms":true}"#).unwrap();
        drop(app);

        let mut app = fixture.app();
        assert!(!app.fuzzy_search_enabled() && !app.word_forms_enabled());
        app.open_path(&fixture.book).unwrap();
        assert_eq!(
            app.library.state(edition).unwrap().dictionary_fuzzy_search,
            DictionaryFuzzySearch::Legacy
        );
        assert!(app.fuzzy_search_enabled() && app.word_forms_enabled());
        app.change_source(SourceAction::BookWordForms).unwrap();
        assert!(app.fuzzy_search_enabled() && !app.word_forms_enabled());
        app.change_source(SourceAction::DefaultWordForms).unwrap();
        assert!(!app.dictionary_settings.fuzzy_search_default());
        assert!(app.dictionary_settings.word_forms_default());
        drop(app);

        let mut app = fixture.app();
        app.open_path(&fixture.book).unwrap();
        assert!(app.fuzzy_search_enabled() && !app.word_forms_enabled());
        app.change_source(SourceAction::ResetBookFuzzySearch)
            .unwrap();
        drop(app);

        let mut app = fixture.app();
        app.open_path(&fixture.book).unwrap();
        assert!(!app.fuzzy_search_enabled() && !app.word_forms_enabled());
        app.change_source(SourceAction::ResetBookWordForms).unwrap();
        assert!(!app.fuzzy_search_enabled() && app.word_forms_enabled());
    }

    fn dismiss_definition(app: &mut Reader) {
        let done = app.definition_layout().1;
        assert_eq!(
            app.input(Input::Tap(
                (done.x + done.w / 2) as f32,
                (done.y + done.h / 2) as f32,
            )),
            Change::Partial,
        );
    }

    #[test]
    fn book_dictionary_exclusions_preserve_priority_and_follow_each_edition() {
        let fixture = Fixture::new();
        let dictionaries = fixture.root.join("dictionaries");
        fs::create_dir(&dictionaries).unwrap();
        crate::dictionary::tests::write_dictionary(
            &dictionaries,
            "Harbour",
            "sametypesequence=m\n",
            &[
                ("tide", b"Harbour tide."),
                ("carry", b"To transport."),
                ("beacon", b"Marking light."),
            ],
        );
        crate::dictionary::tests::write_dictionary(
            &dictionaries,
            "Reference",
            "sametypesequence=m\n",
            &[("tide", b"Reference definition."), ("beacon", b"A light.")],
        );
        let pdf = fixture.root.join("Other edition.pdf");
        fs::write(&pdf, crate::fixed::tests::document()).unwrap();
        let fonts = vec![
            crate::test_fixture("fonts/NotoSans-Regular.ttf"),
            crate::test_fixture("fonts/NotoSans-Bold.ttf"),
        ];
        let open = || Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
        let mut app = open();
        app.open_path(&fixture.book).unwrap();
        let position = app.current_history_location();
        app.activate(Action::ShowDictionaries).unwrap();
        row(&mut app, "Harbour");
        row(&mut app, "Disable by default");
        row(&mut app, "Prefer for this book");
        assert_eq!(app.lookup_sources("tide").unwrap()[0].dictionary, "Harbour");
        assert_eq!(app.lookup_sources("carried").unwrap()[0].word, "carry");
        row(&mut app, "Disable for this book");
        assert_eq!(
            app.lookup_sources("tide").unwrap()[0].dictionary,
            "Reference"
        );
        assert!(app.lookup_sources("carried").unwrap().is_empty());
        assert_eq!(app.book_dictionaries(), [PathBuf::from("Harbour.ifo")]);
        assert_eq!(
            app.book_dictionary_exclusions(),
            [PathBuf::from("Harbour.ifo")]
        );
        assert!(
            app.items()
                .iter()
                .all(|item| item.title != "Remove book preference")
        );
        back(&mut app);
        crate::app::tests::capture_test_screen(
            &app,
            &fonts,
            "READER_BOOK_DICTIONARIES_CAPTURE",
            false,
        );
        back(&mut app);
        app.look_up("beacno").unwrap();
        assert!(app.spelling_lookup.is_some());
        let deadline = Instant::now() + Duration::from_secs(10);
        while app.spelling_lookup.is_some() && Instant::now() < deadline {
            app.tick(Instant::now());
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(app.definitions.len(), 1);
        assert_eq!(app.definitions[0].dictionary, "Reference");
        dismiss_definition(&mut app);
        assert_eq!(app.current_history_location(), position);
        drop(app);

        let mut app = open();
        app.open_path(&pdf).unwrap();
        assert!(app.book_dictionary_exclusions().is_empty());
        assert!(app.book_dictionaries().is_empty());
        assert_eq!(
            app.lookup_sources("tide").unwrap()[0].dictionary,
            "Reference"
        );
        app.open_path(&fixture.book).unwrap();
        assert_eq!(app.current_history_location(), position);
        assert_eq!(
            app.book_dictionary_exclusions(),
            [PathBuf::from("Harbour.ifo")]
        );
        app.activate(Action::ShowDictionaries).unwrap();
        row(&mut app, "Harbour");
        row(&mut app, "Re-enable for this book");
        assert_eq!(app.lookup_sources("tide").unwrap()[0].dictionary, "Harbour");
        row(&mut app, "Disable for this book");
        back(&mut app);
        row(&mut app, "Reference");
        row(&mut app, "Disable for this book");
        back(&mut app);
        row(&mut app, "Fuzzy search by default");
        back(&mut app);
        app.look_up("tide").unwrap();
        assert!(app.definitions.is_empty() && app.spelling_lookup.is_none());
        dismiss_definition(&mut app);
        app.activate(Action::ShowDictionaries).unwrap();
        row(&mut app, "Reset book dictionary choices");
        assert!(app.book_dictionary_exclusions().is_empty());
        assert!(app.book_dictionaries().is_empty());
        assert!(!app.fuzzy_search_enabled());
        assert_eq!(
            app.lookup_sources("tide").unwrap()[0].dictionary,
            "Reference"
        );
    }

    #[test]
    fn source_order_disable_and_book_preferences_persist_and_stay_with_the_book() {
        let f = Fixture::new();
        let dir = f.root.join("dictionaries");
        fs::create_dir(&dir).unwrap();
        for name in ["Harbor", "Estuary", "Coast"] {
            crate::dictionary::tests::write_dictionary(
                &dir,
                name,
                "sametypesequence=m\n",
                &[("tide", name.as_bytes())],
            );
        }
        let other = f.root.join("Coast.epub");
        fs::copy(crate::test_fixture("night-landing.epub"), &other).unwrap();
        fs::write(
            f.root.join("dictionary-preference.json"),
            br#""Harbor.ifo""#,
        )
        .unwrap();
        let font = crate::test_fixture("fonts/NotoSans-Regular.ttf");
        let open = || Reader::open(&f.root, &f.state, vec![font.clone()]).unwrap();
        let names = |app: &Reader| {
            app.lookup_sources("tide")
                .unwrap()
                .into_iter()
                .map(|hit| hit.definition.dictionary)
                .collect::<Vec<_>>()
        };

        let mut app = open();
        app.open_path(&f.book).unwrap();
        let origin = app.current_history_location();
        app.activate(Action::ShowDictionaries).unwrap();
        assert_eq!(app.installed_dictionaries[0].title, "Harbor");
        assert_eq!(names(&app), ["Harbor", "Coast", "Estuary"]);
        row(&mut app, "Coast");
        row(&mut app, "Earlier in default order");
        assert_eq!(names(&app), ["Coast", "Harbor", "Estuary"]);
        row(&mut app, "Disable by default");
        assert_eq!(names(&app), ["Harbor", "Estuary"]);
        row(&mut app, "Prefer for this book");
        assert_eq!(names(&app), ["Coast", "Harbor", "Estuary"]);
        back(&mut app);
        row(&mut app, "Estuary");
        row(&mut app, "Prefer for this book");
        row(&mut app, "Earlier for this book");
        assert_eq!(names(&app), ["Estuary", "Coast", "Harbor"]);
        if let Some(path) = std::env::var_os("READER_DICTIONARY_MANAGEMENT_SCREENSHOT") {
            let mut canvas = Canvas::new(WIDTH, HEIGHT, &[font.display().to_string()]).unwrap();
            let frame = primitive_app::frame(&app, &mut canvas, false);
            primitive_app::write_png(Path::new(&path), WIDTH, HEIGHT, &frame).unwrap();
        }
        back(&mut app);
        back(&mut app);
        assert_eq!(app.screen, Screen::Book);
        assert!(app.current_history_location() == origin);
        drop(app);

        let mut app = open();
        app.open_path(&other).unwrap();
        assert_eq!(names(&app), ["Harbor", "Estuary"]);
        app.open_path(&f.book).unwrap();
        assert_eq!(names(&app), ["Estuary", "Coast", "Harbor"]);
        app.activate(Action::ShowDictionaries).unwrap();
        row(&mut app, "Estuary");
        row(&mut app, "Remove book preference");
        back(&mut app);
        row(&mut app, "Coast");
        row(&mut app, "Remove book preference");
        back(&mut app);
        row(&mut app, "Reset dictionary order");
        assert_eq!(names(&app), ["Estuary", "Harbor"]);
    }
    #[test]
    #[ignore = "Supply two original publications, a lookup word and the installed dictionary root"]
    fn global_and_per_book_source_preferences_apply_to_original_books() {
        #[derive(serde::Deserialize)]
        struct Case {
            first: PathBuf,
            second: PathBuf,
            word: String,
            dictionary: String,
            sources: usize,
        }
        let case: Case = serde_json::from_slice(
            &fs::read(std::env::var_os("READER_SOURCE_PREFERENCE_CASE").unwrap()).unwrap(),
        )
        .unwrap();
        let docs = PathBuf::from(std::env::var_os("PRIMITIVE_READER_DOCUMENTS").unwrap());
        let f = Fixture::new();
        let font = crate::test_fixture("fonts/NotoSans-Regular.ttf");
        let open = || Reader::open(&docs, &f.state, vec![font.clone()]).unwrap();
        let includes = |app: &Reader| {
            app.lookup_sources(&case.word)
                .unwrap()
                .iter()
                .any(|d| d.dictionary == case.dictionary)
        };
        let first = |app: &Reader| {
            app.lookup_sources(&case.word)
                .unwrap()
                .first()
                .is_some_and(|d| d.dictionary == case.dictionary)
        };
        let mut app = open();
        app.open_path(&case.first).unwrap();
        app.activate(Action::ShowDictionaries).unwrap();
        assert_eq!(app.installed_dictionaries.len(), case.sources);
        assert!(includes(&app));
        row(&mut app, &case.dictionary);
        row(&mut app, "Disable by default");
        assert!(!includes(&app));
        row(&mut app, "Prefer for this book");
        assert!(first(&app));
        drop(app);
        let mut app = open();
        app.open_path(&case.second).unwrap();
        assert!(!includes(&app));
        app.open_path(&case.first).unwrap();
        assert!(first(&app));
        app.activate(Action::ShowDictionaries).unwrap();
        row(&mut app, &case.dictionary);
        row(&mut app, "Remove book preference");
        assert!(!includes(&app));
        row(&mut app, "Enable by default");
        assert!(includes(&app));
    }
}
