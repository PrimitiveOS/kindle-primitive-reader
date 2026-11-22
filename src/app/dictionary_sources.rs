use super::*;

#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub(super) struct SourcePreferences {
    order: Vec<PathBuf>,
    disabled: Vec<PathBuf>,
    disable_word_forms: bool,
}

impl SourcePreferences {
    pub(super) fn paths(&self) -> (&[PathBuf], &[PathBuf]) { (&self.order, &self.disabled) }
    pub(super) fn backup_value(bytes: &[u8]) -> io::Result<serde_json::Value> {
        let value: serde_json::Value = serde_json::from_slice(bytes)?;
        let settings: Self = if value.is_object() {
            serde_json::from_value(value)?
        } else {
            let prior: Option<PathBuf> = serde_json::from_value(value)?;
            Self { order: prior.into_iter().collect(), ..Self::default() }
        };
        if settings.order.len() > 512 || settings.disabled.len() > 512
            || settings.order.iter().chain(&settings.disabled).any(|path| {
                path.as_os_str().is_empty() || path.to_str().is_none_or(|s| s.len() > 4096 || s.chars().any(char::is_control))
                    || path.components().any(|c| matches!(c, std::path::Component::ParentDir | std::path::Component::CurDir))
            }) {
            return Err(io::Error::other("The dictionary preferences backup is invalid"));
        }
        serde_json::to_value(settings).map_err(io::Error::other)
    }

    pub(super) fn load(path: &Path) -> Self {
        let Ok(bytes) = read_import_file(path, 64 * 1024) else {
            return Self::default();
        };
        if let Ok(settings) = serde_json::from_slice::<Self>(&bytes) {
            return settings;
        }
        // Earlier releases persisted either null or one preferred relative path.
        Self {
            order: serde_json::from_slice::<Option<PathBuf>>(&bytes)
                .ok()
                .flatten()
                .into_iter()
                .collect(),
            disabled: Vec::new(),
            disable_word_forms: false,
        }
    }
}

pub(super) enum SourceAction {
    Open(usize),
    Toggle,
    Earlier,
    Later,
    ForBook,
    EarlierForBook,
    LaterForBook,
    ResetOrder,
    DefaultWordForms,
    BookWordForms,
    ResetBookWordForms,
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

    fn book_word_forms(&self) -> Option<bool> {
        self.open_book_id().and_then(|id| self.library.state(id)).and_then(|s| s.dictionary_word_forms)
    }

    pub(super) fn word_forms_enabled(&self) -> bool {
        self.book_word_forms().unwrap_or(!self.dictionary_settings.disable_word_forms)
    }

    pub(super) fn lookup_sources(&self, word: &str) -> io::Result<Vec<dictionary::Definition>> {
        let prefs = &self.dictionary_settings;
        self.dictionaries.lookup_configured(
            word,
            self.book_dictionaries(),
            &prefs.order,
            &prefs.disabled,
            self.word_forms_enabled(),
        )
    }

    pub(super) fn start_spelling_lookup(&mut self, word: &str) {
        self.spelling_lookup = None;
        if !self.definitions.is_empty() || !self.word_forms_enabled() || !dictionary::spelling_eligible(word) {
            return;
        }
        let sources = self.dictionaries.clone();
        let word = word.to_owned();
        let book = self.book_dictionaries().to_vec();
        let settings = self.dictionary_settings.clone();
        match dictionary_panel::SpellingLookup::start(move |cancel| {
            sources.suggested_definitions(&word, &book, &settings.order, &settings.disabled, cancel)
        }) {
            Ok(job) => self.spelling_lookup = Some(job),
            Err(error) => self.notice = Some(format!("Could not start dictionary search ({error})")),
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

    pub(super) fn open_sources(&mut self) -> Result<()> {
        self.installed_dictionaries = with_storage(|| self.dictionaries.installed())?;
        self.sort_dictionaries();
        self.dictionary_origin = self.screen;
        self.show(Screen::Dictionaries);
        Ok(())
    }

    pub(super) fn source_rows(&self) -> Vec<Item> {
        let mut rows: Vec<_> = self
            .installed_dictionaries
            .iter()
            .enumerate()
            .map(|(i, source)| {
                let detail = if let Some(rank) = self
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
            })
            .collect();
        if rows.is_empty() {
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
        if !self.installed_dictionaries.is_empty() {
            rows.push(Item::new("Word forms by default", Action::DictionarySource(SourceAction::DefaultWordForms))
                .detail(if self.dictionary_settings.disable_word_forms { "Off" } else { "On · inflections and similar words" }));
            if self.open_book_id().is_some() {
                let state = match (self.word_forms_enabled(), self.book_word_forms().is_none()) {
                    (true, true) => "On · default", (false, true) => "Off · default",
                    (true, false) => "On", (false, false) => "Off",
                };
                rows.push(Item::new("Word forms in this book", Action::DictionarySource(SourceAction::BookWordForms)).detail(state));
                if self.book_word_forms().is_some() {
                    rows.push(Item::new("Use the default word forms setting", Action::DictionarySource(SourceAction::ResetBookWordForms)));
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
        if self.open_book_id().is_some() {
            let preferred = self.book_dictionaries().contains(&source.path);
            rows.push(
                Item::new(
                    if preferred {
                        "Remove book preference"
                    } else {
                        "Prefer for this book"
                    },
                    Action::DictionarySource(SourceAction::ForBook),
                )
                .detail("Book preferences take priority over the global list"),
            );
        }
        let book_order = self.book_dictionaries();
        if let Some(rank) = book_order.iter().position(|key| key == &source.path) {
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
        if matches!(action, SourceAction::BookWordForms | SourceAction::ResetBookWordForms) {
            let id = self.open_book_id().ok_or("Open a book first")?;
            let new = matches!(action, SourceAction::BookWordForms).then(|| !self.word_forms_enabled());
            let old = self.book_word_forms();
            self.library.state_mut(id).ok_or("Book unavailable")?.dictionary_word_forms = new;
            self.mark_dirty();
            if let Err(error) = self.save_library() {
                self.library.state_mut(id).unwrap().dictionary_word_forms = old;
                return Err(error.into());
            }
            return Ok(());
        }
        let mut next = self.dictionary_settings.clone();
        let mut selected_key = None;
        if matches!(action, SourceAction::DefaultWordForms) {
            next.disable_word_forms = !next.disable_word_forms;
        } else if matches!(action, SourceAction::ResetOrder) {
            next.order.clear();
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
                | SourceAction::EarlierForBook
                | SourceAction::LaterForBook => {
                    let id = self.open_book_id().ok_or("Open a book first")?;
                    let state = self.library.state_mut(id).ok_or("Book unavailable")?;
                    let book_order = &mut state.preferred_dictionaries;
                    let previous = book_order.clone();
                    if matches!(action, SourceAction::ForBook) {
                        toggle(book_order, &key);
                    } else {
                        let rank = book_order
                            .iter()
                            .position(|k| *k == key)
                            .ok_or("Choose a preferred dictionary")?;
                        let dest = if matches!(action, SourceAction::EarlierForBook) {
                            rank.checked_sub(1)
                        } else {
                            rank.checked_add(1)
                        };
                        let dest = dest
                            .filter(|i| *i < book_order.len())
                            .ok_or("End of book preferences")?;
                        book_order.swap(rank, dest);
                    }
                    self.mark_dirty();
                    if let Err(error) = self.save_library() {
                        self.library.state_mut(id).unwrap().preferred_dictionaries = previous;
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
                SourceAction::ResetOrder | SourceAction::Open(_) | SourceAction::DefaultWordForms
                | SourceAction::BookWordForms | SourceAction::ResetBookWordForms => unreachable!(),
            }
            selected_key = Some(key);
        }
        primitive_app::save(&self.dictionary_preference, &serde_json::to_vec(&next)?)?;
        self.dictionary_settings = next;
        self.sort_dictionaries();
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
        let reading = &app.active.as_ref().unwrap().reading;
        let block = reading.probe.source.blocks.iter().find(|b| b.text.contains(word)).unwrap();
        let offset = block.text.find(word).unwrap();
        let range = reading.probe.select_range(
            &reading.probe.anchor(block.key, offset),
            &reading.probe.anchor(block.key, offset + word.len()),
        ).unwrap();
        let bounds = reading.highlight_rects(&range)[0];
        app.input(Input::LongTap(
            (bounds.x + bounds.width / 2) as f32,
            (bounds.y + bounds.height / 2) as f32,
        ));
        assert_eq!(app.selection.as_ref().unwrap().text, word);
        let rect = app.selection_button(selection_popup::SelectionAction::Dictionary);
        app.input(Input::Tap((rect.x + rect.w / 2) as f32, (rect.y + rect.h / 2) as f32));
    }

    #[test]
    fn suggestions_arrive_in_the_open_popup_and_can_be_dismissed() {
        let fixture = Fixture::new();
        let dir = fixture.root.join("dictionaries");
        fs::create_dir(&dir).unwrap();
        crate::dictionary::tests::write_dictionary(&dir, "Beacon",
            "sametypesequence=m\n", &[("stone", b"A fragment of rock."), ("store", b"A supply.")]);
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
            &reader, std::slice::from_ref(&font), "READER_SPELLING_POPUP_SCREEN", false,
        );
        let dismiss = |reader: &mut Reader| {
            let done = reader.definition_layout().1;
            reader.input(Input::Tap((done.x + done.w / 2) as f32, (done.y + done.h / 2) as f32));
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
        reader.tick(Instant::now()+Duration::from_secs(2));
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
        crate::dictionary::tests::write_dictionary(&dictionaries, "Harbour Dictionary",
            "sametypesequence=m\n", &[("carry", b"To take something from one place to another.")]);
        let source = f.root.join("Harbour log.xhtml");
        fs::write(&source, include_str!("../../tests/fixtures/dictionary-word-forms.xhtml")).unwrap();
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
        crate::app::tests::capture_test_screen(&app, std::slice::from_ref(&font), "READER_WORD_FORM_SCREEN", false);
        let done = app.definition_layout().1;
        app.input(Input::Tap((done.x + done.w / 2) as f32, (done.y + done.h / 2) as f32));
        assert_eq!(app.screen, Screen::Book);
        assert_eq!(app.current_history_location(), passage);
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
                .map(|hit| hit.dictionary)
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
