//! Edit a single word from its selection. Personal rules follow language across documents.
use super::*;
use crate::user_hyphenation::{Rules, eligible, locale};

const MAX_FILE_BYTES: u64 = 1024 * 1024;
const REMOVE: Rect = Rect::new(LIST_X, LIST_Y + 350, 330, 96);

pub(super) struct Draft {
    word: String,
    language: String,
    original: String,
}

pub(super) fn load(path: &Path) -> io::Result<Rules> {
    let bytes = match read_import_file(path, MAX_FILE_BYTES) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Rules::default()),
        result => result?,
    };
    let rules: Rules = serde_json::from_slice(&bytes)?;
    if !rules.valid() {
        return Err(io::Error::other(
            "The custom hyphenation file contains invalid rules",
        ));
    }
    Ok(rules)
}

impl Reader {
    pub(super) fn hyphenation_word(&self) -> Option<(&str, &str)> {
        if !self.user_hyphenation.enabled {
            return None;
        }
        let reading = &self.active.as_ref()?.reading;
        let probe = &reading.probe;
        if probe.is_vertical()
            || reading.config.settings.hyphenation_mode() != crate::HyphenationMode::Automatic
        {
            return None;
        }
        let selection = self.selection.as_ref()?;
        let (start_block, start) = probe.project_anchor(&selection.start)?;
        let (end_block, end) = probe.project_anchor(&selection.end)?;
        if start_block != end_block {
            return None;
        }
        let block = &probe.source.blocks[start_block];
        let word = block.text.get(start..end)?;
        if !eligible(word)
            || !crate::selection::word_ranges(&block.text)
                .take_while(|r| r.start <= start)
                .any(|r| r == (start..end))
        {
            return None;
        }
        let mut spans = block
            .styled_text
            .iter()
            .skip_while(|s| s.bytes.end <= start)
            .take_while(|s| s.bytes.start < end);
        let run = spans.next()?;
        let language = locale(run.language.as_ref()?);
        let allowed = |span: &crate::source::StyledText| {
            probe.custom_hyphenation_allowed.get(span.element) == Some(&true)
                && span
                    .language
                    .as_ref()
                    .is_some_and(|lang| locale(lang) == language)
        };
        (allowed(run) && spans.all(allowed)).then_some((word, language))
    }

    pub(super) fn begin_hyphenation(&mut self) -> Result<()> {
        let (word, language) = self
            .hyphenation_word()
            .ok_or("Select one word with automatic hyphenation")?;
        let draft = Draft {
            original: crate::hyphenation::suggestion(&self.user_hyphenation, language, word),
            language: language.to_owned(),
            word: word.to_owned(),
        };
        self.editor = Editor::new(200);
        self.editor.text = draft.original.chars().collect();
        self.editor.caret = self.editor.text.len();
        self.show(Screen::Hyphenate);
        self.hyphenation_edit = Some(draft);
        Ok(())
    }

    fn apply_rules(&mut self, rules: Rules) -> Result<()> {
        let encoded = serde_json::to_vec(&rules)?;
        if encoded.len() as u64 > MAX_FILE_BYTES {
            return Err("Custom hyphenation rules are full".into());
        }
        let rules = std::rc::Rc::new(rules);
        let next = self
            .active
            .as_ref()
            .map(|book| {
                let reading = &book.reading;
                let mut config = reading.config.clone();
                config.user_hyphenation = rules.clone();
                Reading::restore(
                    config,
                    reading.chapter,
                    reading.page,
                    reading.anchor().as_ref(),
                )
            })
            .transpose()?;
        primitive_app::save(&self.user_hyphenation_path, &encoded)?;
        if let (Some(book), Some(next)) = (&mut self.active, next) {
            book.reading = next;
        }
        self.user_hyphenation = rules;
        Ok(())
    }

    pub(super) fn set_user_hyphenation(&mut self, enabled: bool) -> Result<()> {
        let mut rules = load(&self.user_hyphenation_path)?;
        if rules.enabled == enabled && rules == *self.user_hyphenation {
            return Ok(());
        }
        rules.enabled = enabled;
        self.apply_rules(rules)
    }

    fn finish_hyphenation(&mut self, remove: bool) -> Result<()> {
        let draft = self
            .hyphenation_edit
            .as_ref()
            .ok_or("Select a word first")?;
        let mut rules = load(&self.user_hyphenation_path)?;
        if remove {
            rules.remove(&draft.language, &draft.word);
        } else {
            let value = self.editor.string();
            let mut changed = rules.clone();
            changed.set(&draft.language, &draft.word, &value)?;
            let after = crate::hyphenation::suggestion(&changed, &draft.language, &draft.word);
            if after.to_lowercase() != draft.original.to_lowercase() {
                rules = changed;
            }
        }
        if *self.user_hyphenation != rules {
            self.apply_rules(rules)?;
        }
        self.selection = None;
        self.show(Screen::Book);
        Ok(())
    }

    pub(super) fn draw_hyphenation(&self, canvas: &mut Canvas) {
        let Some(draft) = &self.hyphenation_edit else {
            return;
        };
        header(canvas, "Hyphenate", "Cancel");
        let width = WIDTH as i32 - 2 * LIST_X;
        canvas.text(
            Rect::new(LIST_X, LIST_Y, width, 72),
            &format!("Word breaks · {}", draft.language),
            BODY,
        );
        canvas.single_line_field(
            Rect::new(LIST_X, LIST_Y + 82, width, 130),
            &self.editor,
            "Word with breaks",
            false,
        );
        canvas.text(Rect::new(LIST_X, LIST_Y + 222, width, 120), self.notice.as_deref().unwrap_or("Insert a space or hyphen at each break. Remove all breaks to keep the word together."), CAPTION.lines(2));
        if self.user_hyphenation.contains(&draft.language, &draft.word) {
            canvas.button(REMOVE, "Remove rule", false, false);
        }
        canvas.keyboard(self.editor.layer, "Save", None);
    }

    pub(super) fn hyphenation_input(&mut self, input: Input) -> Change {
        if matches!(input, Input::PageBack)
            || matches!(input, Input::Tap(x,y) if primitive_ui::back_button(WIDTH as i32).contains(x,y))
        {
            self.selection = None;
            self.show(Screen::Book);
            return Change::Partial;
        }
        let Input::Tap(x, y) = input else {
            return Change::None;
        };
        let remove = REMOVE.contains(x, y)
            && self
                .hyphenation_edit
                .as_ref()
                .is_some_and(|draft| self.user_hyphenation.contains(&draft.language, &draft.word));
        if !remove {
            let layout = Layout::new(WIDTH, HEIGHT, 4, 5);
            let Some(key) = keyboard_key(&layout, &self.editor, "Save", x, y) else {
                return Change::None;
            };
            self.notice = None;
            if !matches!(self.editor.press(key), Press::Go) {
                return Change::Partial;
            }
        }
        match self.finish_hyphenation(remove) {
            Ok(()) => Change::Partial,
            Err(err) => self.report(err),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{selection_popup::SelectionAction, tests::Fixture};
    use super::*;
    use primitive_ui::Key;

    fn tap(rect: Rect) -> Input {
        Input::Tap((rect.x + rect.w / 2) as f32, (rect.y + rect.h / 2) as f32)
    }

    fn key(app: &mut Reader, key: Key) {
        let rect = Layout::new(WIDTH, HEIGHT, 4, 5)
            .keyboard_keys_with(app.editor.layer, "Save")
            .into_iter()
            .find(|(k, _)| k.key == key)
            .unwrap()
            .1;
        app.input(tap(rect));
    }

    fn candidate(probe: &Probe) -> Option<TextSelection> {
        for line in &probe.lines {
            let block = probe.source.blocks.iter().find(|b| b.key == line.block)?;
            let at = line.bytes.end;
            let Some(word) = crate::selection::word_range(&block.text, at) else {
                continue;
            };
            if word.start >= at
                || word.end <= at
                || word.len() < 5
                || word.len() > 32
                || !block.text[word.clone()]
                    .bytes()
                    .all(|b| b.is_ascii_alphabetic())
            {
                continue;
            }
            let selected = probe.select_range(
                &probe.anchor(block.key, word.start),
                &probe.anchor(block.key, word.end),
            )?;
            if let Some(source) = block.styled_text.iter().find(|s| s.bytes.contains(&at))
                && source.language.is_some_and(|l| l.language() == "en")
            {
                return Some(selected);
            }
        }
        None
    }

    fn select(app: &mut Reader, selected: &TextSelection) {
        let reading = &app.active.as_ref().unwrap().reading;
        let rect = reading.highlight_rects(selected)[0];
        let hold = Input::LongTap(
            (rect.x + rect.width / 2) as f32,
            (rect.y + rect.height / 2) as f32,
        );
        app.input(hold);
        assert!(
            app.selection.as_ref() == Some(selected),
            "Long pressing the original geometry should select the whole word"
        );
    }

    fn edit(app: &mut Reader) {
        let rect = app.selection_button(SelectionAction::Hyphenate);
        app.input(tap(rect));
        assert_eq!(app.screen, Screen::Hyphenate);
    }

    fn line_end_in_word(app: &Reader, word: &TextSelection) -> Option<usize> {
        let probe = &app.active.as_ref().unwrap().reading.probe;
        let (b, from) = probe.project_anchor(&word.start)?;
        let (_, to) = probe.project_anchor(&word.end)?;
        probe
            .lines
            .iter()
            .find(|l| {
                l.block == probe.source.blocks[b].key && (from + 1..to).contains(&l.bytes.end)
            })
            .map(|l| l.bytes.end - from)
    }

    #[test]
    fn invalid_or_unwritable_personal_rules_preserve_the_previous_dictionary_and_page() {
        let fixture = Fixture::new();
        let font = crate::test_fixture("fonts/NotoSans-Regular.ttf");
        let mut app = Reader::open(&fixture.root, &fixture.state, vec![font]).unwrap();
        app.open_path(&fixture.book).unwrap();
        let before = app.current_history_location();
        let corrupt = br#"{"enabled":true,"words":{"en-US":{"laboratory":[5,4]}}}"#;
        fs::write(&app.user_hyphenation_path, corrupt).unwrap();
        assert!(app.set_user_hyphenation(true).is_err());
        assert_eq!(fs::read(&app.user_hyphenation_path).unwrap(), corrupt);
        assert!(!app.user_hyphenation.enabled);
        fs::remove_file(&app.user_hyphenation_path).unwrap();
        fs::create_dir(app.user_hyphenation_path.with_extension("json.tmp")).unwrap();
        assert!(app.set_user_hyphenation(true).is_err());
        assert!(!app.user_hyphenation.enabled && !app.user_hyphenation_path.exists());
        assert!(app.current_history_location() == before);
    }

    #[test]
    fn personal_hyphenation_selection_persists_across_books_then_removes_and_restores_wrap() {
        let fixture = Fixture::new();
        let external = std::env::var_os("READER_TEST_USER_HYPH_EPUB").map(PathBuf::from);
        let docs = fixture.root.join("Documents");
        fs::create_dir(&docs).unwrap();
        let path = external.clone().unwrap_or_else(|| {
            let file = docs.join("Line breaks.epub");
            fs::write(
                &file,
                include_bytes!("../../tests/fixtures/english-hyphenation.epub"),
            )
            .unwrap();
            file
        });
        let font = crate::test_fixture("fonts/NotoSans-Regular.ttf");
        let mut app =
            Reader::open(path.parent().unwrap(), &fixture.state, vec![font.clone()]).unwrap();
        app.open_path(&path).unwrap();
        app.activate(Action::Text(Settings {
            font_size: 82.,
            ..device_settings().with_hyphenation(crate::HyphenationMode::Automatic)
        }))
        .unwrap();
        let chapter = if external.is_some() {
            std::env::var("READER_TEST_USER_HYPH_CHAPTER")
                .unwrap()
                .parse()
                .unwrap()
        } else {
            0
        };
        app.restore_position(&ReadingPosition {
            chapter,
            page: 0,
            anchor: None,
        })
        .unwrap();
        let selected = candidate(&app.active.as_ref().unwrap().reading.probe)
            .expect("An English word spanning two lines is needed for this layout check");
        app.restore_position(&ReadingPosition {
            chapter,
            page: 0,
            anchor: Some(selected.start.clone()),
        })
        .unwrap();
        let old_boundary = line_end_in_word(&app, &selected).unwrap();
        let old_location = app.current_history_location();
        let old_history = app.back.clone();
        let frame = |app: &mut Reader| {
            let r = &mut app.active.as_mut().unwrap().reading;
            r.probe.capture(r.page).unwrap()
        };
        let before = frame(&mut app);
        // These are actual on-screen buttons on the lower typography overlay.
        app.show(Screen::BookTools);
        app.input(tap(app.reading_button("Text")));
        app.input(tap(app.reading_button("Style")));
        app.input(tap(app.reading_button("On")));
        assert!(app.user_hyphenation.enabled);
        app.show(Screen::Book);
        let capture = |app: &Reader, filename: &str| {
            if external.is_none()
                && let Some(out) = std::env::var_os("READER_TEST_USER_HYPH_SCREEN")
            {
                let mut canvas = Canvas::new(WIDTH, HEIGHT, &[font.display().to_string()]).unwrap();
                let pixels = primitive_app::frame(app, &mut canvas, false);
                primitive_app::write_png(
                    &Path::new(&out).with_file_name(filename),
                    WIDTH,
                    HEIGHT,
                    &pixels,
                )
                .unwrap();
            }
        };
        select(&mut app, &selected);
        capture(&app, "custom-hyphenation-selection.png");
        edit(&mut app);
        capture(&app, "custom-hyphenation.png");
        let old_value = app.editor.string();
        assert!(old_value.contains('-'));
        // Save refuses changed letters. Cancel keeps both the rendering and the profile unchanged.
        key(&mut app, Key::Char('x'));
        key(&mut app, Key::Go);
        assert_eq!(app.screen, Screen::Hyphenate);
        assert!(app.notice.is_some());
        app.input(Input::PageBack);
        assert!(!load(&app.user_hyphenation_path).unwrap().has_rules());
        assert!(frame(&mut app) == before);
        assert!(app.selection.is_none());
        select(&mut app, &selected);
        edit(&mut app);
        let language = app.hyphenation_edit.as_ref().unwrap().language.clone();
        // Walk through the editor with its real arrow and delete keys; no change to the source.
        for ch in old_value.chars().rev() {
            key(&mut app, if ch == '-' { Key::Delete } else { Key::Left });
        }
        assert_eq!(
            app.editor.string().to_lowercase(),
            selected.text.to_lowercase()
        );
        key(&mut app, Key::Go);
        assert_eq!(app.screen, Screen::Book);
        assert_eq!(line_end_in_word(&app, &selected), None);
        assert!(frame(&mut app) != before);
        assert!(app.current_history_location() == old_location && app.back == old_history);
        let disk = load(&app.user_hyphenation_path).unwrap();
        assert_eq!(disk.mask(&language, &selected.text), Some(vec![]));
        assert!(
            app.active
                .as_ref()
                .unwrap()
                .reading
                .probe
                .select_range(&selected.start, &selected.end)
                .is_some_and(|s| s == selected)
        );
        // Each new Reader and document loads the same language-scoped personal dictionary.
        let mut other = Reader::open(&fixture.root, &fixture.state, vec![font.clone()]).unwrap();
        other.open_path(&fixture.book).unwrap();
        assert_eq!(
            other
                .active
                .as_ref()
                .unwrap()
                .reading
                .config
                .user_hyphenation
                .mask(&language, &selected.text),
            Some(vec![])
        );
        other.set_user_hyphenation(false).unwrap();
        assert!(!load(&other.user_hyphenation_path).unwrap().has_rules());
        other.set_user_hyphenation(true).unwrap();
        assert!(load(&other.user_hyphenation_path).unwrap().has_rules());
        select(&mut app, &selected);
        edit(&mut app);
        assert!(!app.editor.string().contains('-'));
        app.input(tap(REMOVE));
        assert_eq!(app.screen, Screen::Book);
        assert_eq!(line_end_in_word(&app, &selected), Some(old_boundary));
        assert!(frame(&mut app) == before);
        assert!(!load(&app.user_hyphenation_path).unwrap().has_rules());
        eprintln!(
            "user_hyph_case chapter={} original_split={} stable_anchor=true exact_reset_frame=true",
            chapter + 1,
            old_boundary
        );
    }
}
