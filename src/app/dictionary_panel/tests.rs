use super::*;
use crate::app::selection_popup::SelectionAction;
use crate::app::tests::Fixture;
use crate::dictionary::tests::write_dictionary;

struct Simulation {
    fixture: Fixture,
    app: Reader,
    canvas: Canvas,
}

impl Simulation {
    fn new() -> Self {
        let fixture = Fixture::new();
        let dictionaries = fixture.root.join("dictionaries");
        fs::create_dir(&dictionaries).unwrap();
        let long = [
            "A definite article identifies a particular person, place, or thing. A sailor might say the beacon when both speakers know which light marks the harbour.",
            "Before a name, it can distinguish the vessel from another boat. Before a place, it often points to a familiar landmark.",
            "In a field notebook the article directs attention to an object already observed. Compare a light on the water with the light on the water.",
            "When a new noun enters the account, an indefinite article often introduces it. Later occurrences usually use its definite counterpart.",
            "At the edge of the inlet, a pilot may record the entrance, the chain and the bell. Each has a particular position on the local chart.",
            "Some geographical names have a customary article. In other names the article belongs only to a longer description.",
            "In a narrow channel the local term for a mark may be shared between crews who have never met. Their charts establish the same reference.",
            "This use also appears in comparisons. The brighter of two lights can be picked out before either tower is visible on shore.",
            "At dusk the beacon gives three short flashes. The last flash is followed by a pause before the sequence repeats.",
        ].join("\n");
        write_dictionary(
            &dictionaries,
            "Coastal vocabulary",
            "sametypesequence=m\n",
            &[
                ("PDF", b"A page format that preserves a fixed layout."),
                ("The", b"A definite article in an English place name."),
                (
                    "beacon",
                    b"A fixed light or marker that helps a navigator find a route.",
                ),
            ],
        );
        write_dictionary(
            &dictionaries,
            "Field glossary",
            "sametypesequence=m\n",
            &[
                ("The", long.as_bytes()),
                ("The", b"Another use: part of a comparative expression."),
                ("beacon", b"A light built to be seen from the sea."),
            ],
        );
        let font = crate::test_fixture("fonts/NotoSans-Regular.ttf");
        let mut app = Reader::open(&fixture.root, &fixture.state, vec![font.clone()]).unwrap();
        app.controls.dictionary_on_single_word = Some(false);
        let canvas = Canvas::new(WIDTH, HEIGHT, &[font.to_string_lossy().into_owned()]).unwrap();
        Self {
            fixture,
            app,
            canvas,
        }
    }

    fn draw(&mut self, name: Option<&str>) -> Vec<u8> {
        self.draw_in_theme(name, false)
    }

    fn draw_in_theme(&mut self, name: Option<&str>, dark: bool) -> Vec<u8> {
        let pixels = primitive_app::frame(&self.app, &mut self.canvas, dark);
        if let Some((dir, name)) = std::env::var_os("READER_DICTIONARY_CAPTURE_DIR").zip(name) {
            primitive_app::write_png(&Path::new(&dir).join(name), WIDTH, HEIGHT, &pixels).unwrap();
        }
        pixels
    }

    fn tap(&mut self, rect: Rect) {
        assert_eq!(
            self.app.input(Input::Tap(
                (rect.x + rect.w / 2) as f32,
                (rect.y + rect.h / 2) as f32
            )),
            Change::Partial
        );
    }

    fn hold_definition_word(&mut self, word: &str) {
        let view = &self.app.dictionary_navigation.view;
        let (popup, _, rows) = self.app.definition_layout();
        let body = body(popup);
        let text = &self.app.definitions[self.app.list_page].text;
        let (row, left, right) = view
            .layout
            .get()
            .unwrap()
            .iter()
            .skip(view.line)
            .take(rows)
            .enumerate()
            .find_map(|(row, line)| {
                line.words
                    .get()
                    .unwrap()
                    .iter()
                    .find(|(bytes, _, _)| &text[bytes.clone()] == word)
                    .map(|(_, left, right)| {
                        (row, left + line.bounds(body).x, right + line.bounds(body).x)
                    })
            })
            .unwrap();
        assert_eq!(
            self.app.input(Input::LongTap(
                ((left + right) / 2) as f32,
                (body.y + row as i32 * LINE + LINE / 2) as f32
            )),
            Change::Partial
        );
        assert_eq!(self.app.definition_word, word);
    }

    fn tap_keyboard_key(&mut self, key: primitive_ui::Key, action: &str) {
        let rect = Layout::new(WIDTH, HEIGHT, 4, 5)
            .keyboard_keys_with(self.app.editor.layer, action)
            .into_iter()
            .find(|(candidate, _)| candidate.key == key)
            .unwrap()
            .1;
        self.tap(rect);
    }

    fn tap_keyboard_character(&mut self, character: char, action: &str) {
        let rect = Layout::new(WIDTH, HEIGHT, 4, 5)
            .keyboard_keys_with(self.app.editor.layer, action)
            .into_iter()
            .find(|(key, _)| key.label.eq_ignore_ascii_case(&character.to_string()))
            .unwrap()
            .1;
        self.tap(rect);
    }

    fn open_epub_selection(&mut self) {
        self.app.open_path(&self.fixture.book).unwrap();
        let reading = &self.app.active.as_ref().unwrap().reading;
        self.app
            .follow(Book::resolve_link(&reading.probe.chapter, "dusk.xhtml#door").unwrap())
            .unwrap();
        let reading = &self.app.active.as_ref().unwrap().reading;
        let mut start = reading.probe.fragment("door").unwrap();
        start.offset += "At dusk ".len();
        let mut end = start.clone();
        end.offset += 3;
        let selected = reading.probe.select_range(&start, &end).unwrap();
        let point = reading.highlight_rects(&selected)[0];
        self.app.input(Input::LongTap(
            (point.x + point.width / 2) as f32,
            (point.y + point.height / 2) as f32,
        ));
        assert_eq!(self.app.selection.as_ref().unwrap().text, "the");
    }

    fn back(&mut self) {
        self.tap(self.app.definition_layout().1);
    }

    fn hold_back(&mut self) {
        let back = self.app.definition_layout().1;
        assert_eq!(
            self.app.input(Input::LongTap(
                (back.x + back.w / 2) as f32,
                (back.y + back.h / 2) as f32
            )),
            Change::Partial
        );
    }
}

#[test]
fn dictionary_hold_close_returns_from_nested_epub_results_to_the_same_passage() {
    let mut s = Simulation::new();
    s.open_epub_selection();
    let original = s.app.selection.clone();
    let location = s.app.current_history_location();
    let passage = s.draw(None);
    s.tap(s.app.selection_button(SelectionAction::Dictionary));
    s.tap(definition_results_button(s.app.definition_layout().0));
    s.tap(definition_result_row(s.app.definition_layout().0, 1));
    s.draw(None);
    s.hold_definition_word("beacon");
    assert_eq!(s.app.dictionary_navigation.trail.len(), 1);
    s.back();
    assert_eq!(s.app.screen, Screen::Definition);
    assert_eq!(s.app.list_page, 1);
    assert_eq!(s.app.definition_word, "the");

    s.draw(None);
    s.hold_definition_word("beacon");
    s.draw(None);
    assert_eq!(s.app.finish_find("light"), Change::Partial);
    s.draw(Some("dictionary-close-stack.png"));
    s.tap(definition_results_button(s.app.definition_layout().0));
    assert!(s.app.definition_results_page.is_some());
    s.hold_back();
    assert_eq!(s.app.screen, Screen::Book);
    assert!(s.app.dictionary_navigation.trail.is_empty());
    assert!(s.app.dictionary_navigation.view.find.is_none());
    assert!(s.app.definition_results_page.is_none());
    assert_eq!(s.app.current_history_location(), location);
    assert_eq!(s.app.selection, original);
    assert_eq!(s.draw(None), passage);
}

#[test]
fn dictionary_hold_close_saves_the_armed_pdf_passage_once_across_nested_results() {
    let mut s = Simulation::new();
    let path = s.fixture.root.join("Nested definitions.pdf");
    fs::write(&path, crate::fixed::tests::document()).unwrap();
    s.app.open_path(&path).unwrap();
    let reading = &s.app.fixed.as_ref().unwrap().reading;
    let words = reading.page_text(0).unwrap();
    let (size, viewport) = reading.page_viewport().unwrap();
    let point = words.words[0]
        .bounds
        .in_viewport(size, viewport)
        .unwrap()
        .center();
    s.app.input(Input::LongTap(point.x as f32, point.y as f32));
    let selected = s.app.pdf_selection.as_ref().unwrap().highlight().clone();
    let book_id = s.app.fixed.as_ref().unwrap().id;
    let origin = s.app.current_history_location();
    s.tap(s.app.selection_button(SelectionAction::Dictionary));
    s.tap(action_row(s.app.definition_layout().0)[4]);
    s.draw(None);
    s.hold_definition_word("format");
    s.tap(action_row(s.app.definition_layout().0)[4]);
    assert!(s.app.dictionary_navigation.view.save_highlight);
    assert!(s.app.dictionary_navigation.trail[0].view.save_highlight);
    s.hold_back();
    assert_eq!(s.app.screen, Screen::Fixed);
    assert_eq!(s.app.current_history_location(), origin);
    assert!(s.app.pdf_selection.is_none());
    assert!(s.app.spelling_lookup.is_none());
    let saved = &s.app.library.state(book_id).unwrap().pdf_highlights;
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].text, selected.text);
    assert_eq!(saved[0].bounds, selected.bounds);
}

#[test]
fn dictionary_hold_close_returns_to_the_original_lookup_editor() {
    let mut s = Simulation::new();
    s.app.show(Screen::DictionaryEntry);
    s.app.editor.text = "The".chars().collect();
    s.app.editor.caret = 1;
    let editor = s.app.editor.clone();
    s.tap_keyboard_key(primitive_ui::Key::Go, "Look up");
    s.app.look_up("beacon").unwrap();
    s.hold_back();
    assert_eq!(s.app.screen, Screen::DictionaryEntry);
    assert_eq!(s.app.editor, editor);
}

#[test]
fn dictionary_popup_defaults_are_saved_and_restore_nested_large_windows() {
    let mut s = Simulation::new();
    fs::write(
        &s.app.dictionary_preference,
        br#"{"order":[],"disable_word_forms":false}"#,
    )
    .unwrap();
    let open = |s: &Simulation| {
        let font = crate::test_fixture("fonts/NotoSans-Regular.ttf");
        let mut reader = Reader::open(&s.fixture.root, &s.fixture.state, vec![font]).unwrap();
        reader.controls.dictionary_on_single_word = Some(false);
        reader
    };
    s.app = open(&s);
    assert!(!s.app.dictionary_settings.large_window);
    assert!(s.app.dictionary_settings.justify_text);

    s.open_epub_selection();
    let original = s.app.selection.clone();
    let location = s.app.current_history_location();
    s.tap(s.app.selection_button(SelectionAction::Dictionary));
    let compact = s.app.definition_layout().0;
    s.tap(definition_results_button(compact));
    s.tap(definition_result_row(s.app.definition_layout().0, 1));
    let compact = s.app.definition_layout().0;
    assert!(s.app.definition_over_book());
    assert_eq!(
        compact,
        definition_bounds(
            compact.h,
            s.app.selection_bounds(),
            s.app.reading_viewport()
        )
    );
    s.draw(Some("dictionary-justify-text.png"));
    let lines = s.app.dictionary_navigation.view.layout.get().unwrap();
    let first = lines[0].placed.get().unwrap();
    assert!(first.justified);
    let end = first.runs.last().unwrap();
    assert_eq!(end.x + end.width, body(compact).w);
    s.hold_definition_word("article");
    s.back();
    assert_eq!(s.app.list_page, 1);
    s.back();
    assert_eq!(s.app.current_history_location(), location);
    assert_eq!(s.app.selection, original);

    s.app.activate(Action::ShowDictionaries).unwrap();
    let rows = s.app.items();
    assert_eq!(rows[0].title, "Large window");
    assert_eq!(rows[0].detail.as_deref(), Some("Off"));
    assert_eq!(rows[1].title, "Justify text");
    assert_eq!(rows[1].detail.as_deref(), Some("On"));
    for row in 0..2 {
        s.tap(Rect::new(
            LIST_X,
            LIST_Y + row * ROW_HEIGHT,
            100,
            ROW_HEIGHT,
        ));
    }
    assert!(s.app.dictionary_settings.large_window);
    assert!(!s.app.dictionary_settings.justify_text);
    s.draw(Some("dictionary-popup-settings.png"));

    s.app = open(&s);
    assert!(s.app.dictionary_settings.large_window);
    assert!(!s.app.dictionary_settings.justify_text);
    s.open_epub_selection();
    let original = s.app.selection.clone();
    let initial = s.draw(None);
    s.tap(s.app.selection_button(SelectionAction::Dictionary));
    let large = s.app.definition_layout().0;
    assert!(!s.app.definition_over_book());
    assert_eq!(large.w, compact.w);
    assert!(large.h > compact.h * 2);
    s.tap(definition_results_button(large));
    s.tap(definition_result_row(s.app.definition_layout().0, 1));
    s.draw_in_theme(Some("dictionary-large-window.png"), true);
    let lines = s.app.dictionary_navigation.view.layout.get().unwrap();
    let first = lines[0].placed.get().unwrap();
    assert!(!first.justified);
    assert!(first.runs[0].width < body(large).w);
    s.hold_definition_word("beacon");
    assert!(s.app.definition_expanded);
    s.back();
    assert!(s.app.definition_expanded);
    assert_eq!(s.app.list_page, 1);
    s.back();
    assert_eq!(s.app.screen, Screen::Book);
    assert_eq!(s.app.current_history_location(), location);
    assert_eq!(s.app.selection, original);
    assert_eq!(s.draw(None), initial);
}

#[test]
fn dictionary_justification_respects_paragraphs_and_maps_mixed_direction_words() {
    let mut s = Simulation::new();
    let text = "A harbour has sheltered water.\nOne more line.\n\nFinally.";
    let lines = layout(&mut s.canvas, text, 390);
    assert!(lines[0].soft_wrap);
    assert!(lines[1..].iter().any(|line| !line.soft_wrap));
    for line in &lines {
        if text[line.range.clone()].ends_with('\n') || line.range.end == text.len() {
            assert!(!line.soft_wrap);
        }
    }

    let text = "Coast في الميناء now";
    let result = PlacedLine::new(&mut s.canvas, text, 480, true, None);
    assert!(result.justified);
    let hits: Vec<_> = crate::selection::word_ranges(text).collect();
    let boxes = result.range_boxes(&mut s.canvas, text, 0, &hits);
    let visual: Vec<_> = boxes
        .iter()
        .map(|(bytes, _, _)| &text[bytes.clone()])
        .collect();
    assert_eq!(visual, ["Coast", "الميناء", "في", "now"]);
    assert!(boxes.windows(2).all(|pair| pair[0].2 < pair[1].1));
    assert_eq!(boxes.last().unwrap().2, 480);
    assert!(!PlacedLine::new(&mut s.canvas, "one\u{00a0}unit", 480, true, None).justified);
    assert!(!PlacedLine::new(&mut s.canvas, "  Indented text", 480, true, None).justified);
    assert!(!PlacedLine::new(&mut s.canvas, text, 40, true, None).justified);
}

#[test]
fn dictionary_rich_html_displays_senses_and_preserves_find_and_nested_lookup() {
    use dictionary::article::Marker;

    let mut s = Simulation::new();
    if let Some(fallback) = std::env::var_os("READER_DICTIONARY_ARTICLE_FALLBACK") {
        let regular = crate::test_fixture("fonts/NotoSans-Regular.ttf");
        let bold = crate::test_fixture("fonts/NotoSans-Bold.ttf");
        let fonts =
            [regular, bold, fallback.into()].map(|path| path.to_string_lossy().into_owned());
        s.canvas = Canvas::new(WIDTH, HEIGHT, &fonts).unwrap();
    }
    let article = "<div lang='en' dir='ltr'>\
        <ol><li>A definite article introducing a known place or thing.\
        <blockquote><p>The pilot watched the harbour light.</p>\
        <dl><dd lang='ar' dir='rtl'>عند المساء يشاهد البحار ضوء الميناء الهادئ قبل أن يقترب من الساحل. ثم يعود إلى القارب.</dd></dl>\
        </blockquote></li><li>Used before a comparative form to connect two changes.\
        <blockquote>The sooner we leave, the better.</blockquote></li></ol></div>";
    write_dictionary(
        &s.fixture.root.join("dictionaries"),
        "Bilingual examples",
        "sametypesequence=h\nlang=eng-ara\n",
        &[
            ("The", article.as_bytes()),
            ("الميناء", "<p lang='en'>A harbour.</p>".as_bytes()),
        ],
    );
    s.open_epub_selection();
    let selection = s.app.selection.clone();
    s.tap(s.app.selection_button(SelectionAction::Dictionary));
    let index = s
        .app
        .definitions
        .iter()
        .position(|entry| entry.dictionary == "Bilingual examples")
        .unwrap();
    s.tap(definition_results_button(s.app.definition_layout().0));
    s.tap(definition_result_row(s.app.definition_layout().0, index));
    s.draw(Some("dictionary-structured-article.png"));
    let popup = s.app.definition_layout().0;
    s.tap(definition_actions(popup)[1]);
    assert!(s.app.definition_expanded);
    s.draw(None);
    let parent_text = s.app.definitions[index].text.clone();
    let rows = s.app.dictionary_navigation.view.layout.get().unwrap();
    let markers: Vec<_> = rows.iter().filter_map(|line| line.marker).collect();
    assert_eq!(markers, [Marker::Number(1), Marker::Number(2)]);
    let rtl_rows: Vec<_> = rows.iter().filter(|line| line.rtl == Some(true)).collect();
    assert!(rtl_rows.len() >= 2);
    assert!(
        rtl_rows
            .iter()
            .all(|line| line.indent == MARKER_GUTTER + 2 * EXAMPLE_INSET)
    );
    assert!(rows[0].indent < rtl_rows[0].indent);
    assert_eq!(rtl_rows[0].end_inset, 2 * EXAMPLE_INSET);
    assert!(
        rows.iter()
            .any(|line| line.placed.get().is_some_and(|layout| layout.justified))
    );
    assert!(!parent_text.contains("1."));
    assert_eq!(s.app.finish_find("الميناء"), Change::Partial);
    let find = s.app.dictionary_navigation.view.find.as_ref().unwrap();
    assert_eq!(&parent_text[find.hits[find.index].clone()], "الميناء");
    let frame_with_find = s.draw(None);
    s.hold_definition_word("الميناء");
    assert!(s.app.definition_expanded);
    assert_eq!(s.app.definitions[0].text, "A harbour.");
    s.back();
    assert_eq!(s.app.list_page, index);
    assert!(s.app.dictionary_navigation.view.find.is_some());
    assert_eq!(s.draw(None), frame_with_find);
    s.back();
    assert!(s.app.definition_over_book());
    s.back();
    assert_eq!(s.app.screen, Screen::Book);
    assert_eq!(s.app.selection, selection);

    s.app.dictionary_settings.large_window = true;
    s.app.dictionary_settings.justify_text = false;
    s.tap(s.app.selection_button(SelectionAction::Dictionary));
    assert!(s.app.definition_expanded);
    s.tap(definition_results_button(s.app.definition_layout().0));
    s.tap(definition_result_row(s.app.definition_layout().0, index));
    s.draw(None);
    let rows = s.app.dictionary_navigation.view.layout.get().unwrap();
    assert!(
        rows.iter()
            .all(|line| line.placed.get().is_none_or(|layout| !layout.justified))
    );
    s.back();
    assert_eq!(s.app.screen, Screen::Book);
    assert_eq!(s.app.selection, selection);
}

#[test]
fn dictionary_html_uses_locale_for_neutral_paragraphs_and_explicit_direction_first() {
    let mut paragraph = dictionary::article::Paragraph {
        range: 0..4,
        indent: 0,
        list_item: false,
        marker: None,
        direction: None,
        language: parley::Language::parse("ar").ok(),
    };
    assert!(paragraph_rtl("2026", &paragraph));
    assert!(!paragraph_rtl("A 2026", &paragraph));
    paragraph.direction = Some(parley::BaseDirection::Ltr);
    assert!(!paragraph_rtl("بحر", &paragraph));
    paragraph.direction = Some(parley::BaseDirection::Rtl);
    assert!(paragraph_rtl("A 2026", &paragraph));
    paragraph.direction = Some(parley::BaseDirection::Auto);
    assert!(!paragraph_rtl("2026", &paragraph));
}

#[test]
fn dictionary_cross_reference_opens_the_originating_source_and_restores_the_passage() {
    let mut s = Simulation::new();
    let root = s.fixture.root.join("dictionaries");
    let article = b"<p>See <a href='bword://Harbour%20Light'>the harbour<br>reference</a> for the closest meaning.</p>\
        <p><a href='bword://elsewhere'>Unlisted sense</a>, <a href='https://example.invalid'>Web article</a>, \
        and <a href='elsewhere'>Bare target</a>.</p>";
    write_dictionary(
        &root,
        "Reference glossary",
        "sametypesequence=h\n",
        &[
            (
                "harbour light",
                b"<p>A light marking the entry to the harbour.</p>",
            ),
            ("The", article),
        ],
    );
    write_dictionary(
        &root,
        "Other glossary",
        "sametypesequence=m\n",
        &[
            ("elsewhere", b"Only available from the other source."),
            (
                "harbour light",
                b"Different meaning from a dictionary with the same display name.",
            ),
        ],
    );
    let duplicate = root.join("Other glossary.ifo");
    let metadata = fs::read_to_string(&duplicate)
        .unwrap()
        .replace("bookname=Other glossary", "bookname=Reference glossary");
    fs::write(&duplicate, metadata).unwrap();

    s.open_epub_selection();
    let selection = s.app.selection.clone();
    let location = s.app.current_history_location();
    let passage = s.draw(None);
    s.tap(s.app.selection_button(SelectionAction::Dictionary));
    let index = s
        .app
        .definitions
        .iter()
        .position(|entry| entry.source == Path::new("Reference glossary.ifo"))
        .unwrap();
    s.tap(definition_results_button(s.app.definition_layout().0));
    s.tap(definition_result_row(s.app.definition_layout().0, index));
    s.draw(Some("dictionary-cross-reference.png"));
    let link_point = |s: &Simulation, label: &str| {
        let entry = &s.app.definitions[s.app.list_page];
        let link = entry
            .article
            .as_ref()
            .unwrap()
            .links
            .iter()
            .position(|link| &entry.text[link.range.clone()] == label)
            .unwrap();
        let (popup, _, rows) = s.app.definition_layout();
        let view = &s.app.dictionary_navigation.view;
        let rect = body(popup);
        view.layout
            .get()
            .unwrap()
            .iter()
            .skip(view.first_line(rows))
            .take(rows)
            .enumerate()
            .filter_map(|(row, line)| {
                let (_, left, right) = line
                    .links
                    .get()?
                    .iter()
                    .find(|(index, _, _)| *index == link)?;
                let x = line.bounds(rect).x + (right + left) / 2;
                let y = rect.y + row as i32 * LINE + LINE / 2;
                Some((x as f32, y as f32))
            })
            .last()
            .unwrap()
    };
    let before = s.app.lookup_history.dictionary.clone();
    for label in ["Web article", "Bare target"] {
        let (x, y) = link_point(&s, label);
        assert_eq!(s.app.input(Input::Tap(x, y)), Change::None);
    }
    let (x, y) = link_point(&s, "Unlisted sense");
    assert_eq!(s.app.input(Input::Tap(x, y)), Change::Partial);
    assert_eq!(
        s.app.notice.as_deref(),
        Some("Reference not found in this dictionary")
    );
    assert_eq!(s.app.list_page, index);
    assert_eq!(s.app.lookup_history.dictionary, before);
    assert!(s.app.spelling_lookup.is_none());
    assert_eq!(s.app.finish_find("closest"), Change::Partial);
    let previous = s.draw(None);
    let (x, y) = link_point(&s, "the harbour\nreference");
    assert_eq!(s.app.input(Input::Tap(x, y)), Change::Partial);
    assert_eq!(s.app.definition_word, "Harbour Light");
    assert_eq!(s.app.definitions.len(), 1);
    assert_eq!(
        s.app.definitions[0].text,
        "A light marking the entry to the harbour."
    );
    assert_eq!(
        s.app.definitions[0].source,
        Path::new("Reference glossary.ifo")
    );
    assert!(s.app.spelling_lookup.is_none());
    s.back();
    assert_eq!(s.app.list_page, index);
    assert_eq!(s.draw(None), previous);
    s.back();
    assert_eq!(s.app.current_history_location(), location);
    assert_eq!(s.app.selection, selection);
    assert_eq!(s.draw(None), passage);
}

#[test]
fn dictionary_navigation_restores_each_result_and_the_epub_selection() {
    let mut s = Simulation::new();
    s.open_epub_selection();
    let first = s.app.selection.clone().unwrap();
    let initial = position(&s.app.active.as_ref().unwrap().reading);
    let selected_frame = s.draw(None);
    s.tap(s.app.selection_button(SelectionAction::Dictionary));
    assert_eq!(s.app.definitions.len(), 3);
    assert_eq!(s.app.definitions[0].dictionary, "Coastal vocabulary");
    assert_eq!(s.app.definitions[1].dictionary, "Field glossary");
    assert_eq!(s.app.list_page, 0);
    s.tap(definition_results_button(s.app.definition_layout().0));
    s.draw(Some("dictionary-sources.png"));
    s.tap(definition_result_row(s.app.definition_layout().0, 1));
    assert_eq!(s.app.list_page, 1);
    s.draw(Some("dictionary.png"));
    let popup = s.app.definition_layout().0;
    s.tap(definition_actions(popup)[2]);
    assert_eq!(s.app.list_page, 2);
    assert_eq!(
        s.app.swipe(
            Input::Swipe {
                dx: 180.0,
                dy: 1.0,
                start_y: popup.y as f32 + 130.0
            },
            500.0
        ),
        Change::Partial
    );
    assert_eq!(s.app.list_page, 1);
    s.draw(None);
    assert_eq!(
        s.app.swipe(
            Input::Swipe {
                dx: 0.0,
                dy: -160.0,
                start_y: popup.y as f32 + 300.0
            },
            500.0
        ),
        Change::Partial
    );
    assert!(s.app.dictionary_navigation.view.line > 0);
    s.app.input(Input::PageBack);
    assert_eq!(s.app.dictionary_navigation.view.line, 0);

    s.tap(action_row(popup)[1]);
    let keyboard = Layout::new(WIDTH, HEIGHT, 4, 5);
    for character in "beacon".chars() {
        let key = keyboard
            .keyboard_keys_with(s.app.editor.layer, "Find")
            .into_iter()
            .find(|(key, _)| key.label.to_lowercase() == character.to_string())
            .unwrap()
            .1;
        s.tap(key);
    }
    let go = keyboard
        .keyboard_keys_with(s.app.editor.layer, "Find")
        .into_iter()
        .find(|(key, _)| key.key == primitive_ui::Key::Go)
        .unwrap()
        .1;
    s.tap(go);
    assert_eq!(
        s.app
            .dictionary_navigation
            .view
            .find
            .as_ref()
            .unwrap()
            .hits
            .len(),
        2
    );
    assert_eq!(s.app.input(Input::PageForward), Change::Partial);
    let parent_line = s.app.dictionary_navigation.view.line;
    assert!(parent_line > 0);
    let frame = s.draw(Some("dictionary-find.png"));
    s.hold_definition_word("beacon");
    assert_eq!(s.app.definitions.len(), 2);
    s.draw(Some("dictionary-nested.png"));
    s.hold_definition_word("navigator");
    assert!(s.app.definitions.is_empty());
    s.back();
    assert_eq!(s.app.definition_word, "beacon");
    s.back();
    assert_eq!(s.app.list_page, 1);
    assert_eq!(s.app.dictionary_navigation.view.line, parent_line);
    assert_eq!(s.draw(None), frame);
    s.tap(action_row(popup)[2]);
    let online = s.app.online.as_ref().unwrap();
    assert_eq!(online.request.text, "The");
    assert!(online.job.is_none());
    assert_eq!(s.app.input(Input::Tap(1.0, 1.0)), Change::Partial);
    assert_eq!(s.app.screen, Screen::Definition);
    assert_eq!(s.draw(None), frame);
    s.back();
    assert_eq!(s.app.screen, Screen::Book);
    assert_eq!(s.app.selection.as_ref(), Some(&first));
    assert_eq!(position(&s.app.active.as_ref().unwrap().reading), initial);
    assert_eq!(s.draw(None), selected_frame);
}

#[test]
fn dictionary_search_book_uses_the_original_epub_selection_and_keeps_its_position() {
    let mut s = Simulation::new();
    s.open_epub_selection();
    let initial = position(&s.app.active.as_ref().unwrap().reading);
    s.tap(s.app.selection_button(SelectionAction::Dictionary));
    let popup = s.app.definition_layout().0;
    s.tap(action_row(popup)[3]);
    assert_eq!(s.app.screen, Screen::Matches);
    assert_eq!(s.app.editor.string(), "the");
    assert!(s.app.selection.is_none());
    assert_eq!(position(&s.app.active.as_ref().unwrap().reading), initial);
    let deadline = Instant::now() + Duration::from_secs(2);
    while s.app.nearby.searching_all() && s.app.hits.is_empty() && Instant::now() < deadline {
        s.app.poll_nearby(Instant::now()).unwrap();
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(
        !s.app.hits.is_empty(),
        "Search book should find the selection"
    );
}

#[test]
fn dictionary_search_book_uses_the_original_pdf_selection() {
    let mut s = Simulation::new();
    let path = s.fixture.root.join("Search selection.pdf");
    fs::write(&path, crate::fixed::tests::document()).unwrap();
    s.app.open_path(&path).unwrap();
    let reading = &s.app.fixed.as_ref().unwrap().reading;
    let words = reading.page_text(0).unwrap();
    let (size, viewport) = reading.page_viewport().unwrap();
    let point = words.words[0]
        .bounds
        .in_viewport(size, viewport)
        .unwrap()
        .center();
    s.app.input(Input::LongTap(point.x as f32, point.y as f32));
    assert_eq!(s.app.pdf_selection.as_ref().unwrap().text(), "PDF");
    s.tap(s.app.selection_button(SelectionAction::Dictionary));
    let popup = s.app.definition_layout().0;
    s.tap(action_row(popup)[3]);
    assert_eq!(s.app.screen, Screen::Matches);
    assert_eq!(s.app.editor.string(), "PDF");
    assert!(s.app.pdf_selection.is_none());
    assert_eq!(s.app.fixed.as_ref().unwrap().reading.page(), 0);
    while let Some(deadline) = s.app.search_at {
        s.app.step_pdf_search(deadline).unwrap();
    }
    assert!(
        !s.app.pdf_hits.is_empty(),
        "Search book should find the PDF text"
    );
}

#[test]
fn dictionary_highlight_toggle_saves_the_epub_selection_when_lookup_closes() {
    let mut s = Simulation::new();
    s.open_epub_selection();
    let selected = s.app.selection.clone().unwrap();
    let book_id = s.app.active.as_ref().unwrap().id;
    s.tap(s.app.selection_button(SelectionAction::Dictionary));
    let popup = s.app.definition_layout().0;
    let highlight = action_row(popup)[4];
    s.tap(highlight);
    assert!(s.app.dictionary_navigation.view.save_highlight);
    s.draw(Some("dictionary-highlight.png"));
    s.tap(highlight);
    assert!(!s.app.dictionary_navigation.view.save_highlight);
    s.tap(highlight);
    assert!(s.app.dictionary_navigation.view.save_highlight);
    s.back();
    assert_eq!(s.app.screen, Screen::Book);
    assert!(s.app.selection.is_none());
    assert_eq!(
        s.app.library.state(book_id).unwrap().highlights[0].selection,
        selected
    );
    s.app.save().unwrap();
    let mut reopened = Reader::open(
        &s.fixture.root,
        &s.fixture.state,
        vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")],
    )
    .unwrap();
    reopened.open_path(&s.fixture.book).unwrap();
    let reopened_id = reopened.active.as_ref().unwrap().id;
    assert_eq!(
        reopened.library.state(reopened_id).unwrap().highlights[0].selection,
        selected
    );
}

#[test]
fn dictionary_highlight_toggle_saves_the_pdf_selection_when_lookup_closes() {
    let mut s = Simulation::new();
    let path = s.fixture.root.join("Dictionary highlight.pdf");
    fs::write(&path, crate::fixed::tests::document()).unwrap();
    s.app.open_path(&path).unwrap();
    let reading = &s.app.fixed.as_ref().unwrap().reading;
    let words = reading.page_text(0).unwrap();
    let (size, viewport) = reading.page_viewport().unwrap();
    let point = words.words[0]
        .bounds
        .in_viewport(size, viewport)
        .unwrap()
        .center();
    s.app.input(Input::LongTap(point.x as f32, point.y as f32));
    let selected = s.app.pdf_selection.as_ref().unwrap().text().to_owned();
    let book_id = s.app.fixed.as_ref().unwrap().id;
    s.tap(s.app.selection_button(SelectionAction::Dictionary));
    let popup = s.app.definition_layout().0;
    s.tap(action_row(popup)[4]);
    assert!(s.app.dictionary_navigation.view.save_highlight);
    s.back();
    assert_eq!(s.app.screen, Screen::Fixed);
    assert!(s.app.pdf_selection.is_none());
    assert_eq!(
        s.app.library.state(book_id).unwrap().pdf_highlights[0].text,
        selected
    );
    s.app.save().unwrap();
    let mut reopened = Reader::open(
        &s.fixture.root,
        &s.fixture.state,
        vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")],
    )
    .unwrap();
    reopened.open_path(&path).unwrap();
    let reopened_id = reopened.fixed.as_ref().unwrap().id;
    assert_eq!(
        reopened.library.state(reopened_id).unwrap().pdf_highlights[0].text,
        selected
    );
}

#[test]
fn dictionary_unhighlight_cancels_saving_and_keeps_the_epub_selection() {
    let mut s = Simulation::new();
    s.open_epub_selection();
    let selected = s.app.selection.clone().unwrap();
    let book_id = s.app.active.as_ref().unwrap().id;
    s.tap(s.app.selection_button(SelectionAction::Dictionary));
    let popup = s.app.definition_layout().0;
    let highlight = action_row(popup)[4];
    s.tap(highlight);
    s.tap(highlight);
    s.back();
    assert_eq!(s.app.selection.as_ref(), Some(&selected));
    assert!(s.app.library.state(book_id).unwrap().highlights.is_empty());
}

#[test]
fn dictionary_edit_query_relooks_up_and_returns_through_the_parent_entry() {
    let mut s = Simulation::new();
    s.open_epub_selection();
    let selection = s.app.selection.clone().unwrap();
    s.tap(s.app.selection_button(SelectionAction::Dictionary));
    assert_eq!(s.app.definition_word, "the");
    let popup = s.app.definition_layout().0;
    let edit = definition_edit_button(popup, s.app.definitions.len() > 1);
    s.tap(edit);
    assert!(s.app.dictionary_navigation.view.query_editing);
    assert_eq!(s.app.screen, Screen::Definition);
    assert!(s.app.definition_over_book());
    assert_eq!(s.app.editor.string(), "the");
    s.draw(Some("dictionary-edit-query.png"));
    for _ in 0..3 {
        s.tap_keyboard_key(primitive_ui::Key::Delete, "Look up");
    }
    for character in "beacon".chars() {
        s.tap_keyboard_character(character, "Look up");
    }
    s.tap_keyboard_key(primitive_ui::Key::Go, "Look up");
    assert_eq!(s.app.screen, Screen::Definition);
    assert_eq!(s.app.definition_word, "beacon");
    assert_eq!(s.app.dictionary_navigation.trail.len(), 1);
    s.back();
    assert_eq!(s.app.definition_word, "the");
    assert!(!s.app.dictionary_navigation.view.query_editing);
    s.back();
    assert_eq!(s.app.screen, Screen::Book);
    assert_eq!(s.app.selection.as_ref(), Some(&selection));
}

#[test]
fn long_press_dictionary_edit_uses_the_current_result_headword() {
    let mut s = Simulation::new();
    s.open_epub_selection();
    s.tap(s.app.selection_button(SelectionAction::Dictionary));
    let popup = s.app.definition_layout().0;
    let edit = definition_edit_button(popup, s.app.definitions.len() > 1);
    assert_eq!(
        s.app.input(Input::LongTap(
            (edit.x + edit.w / 2) as f32,
            (edit.y + edit.h / 2) as f32
        )),
        Change::Partial
    );
    assert!(s.app.dictionary_navigation.view.query_editing);
    assert_eq!(s.app.editor.string(), "The");
    s.app.input(Input::PageBack);
    assert_eq!(s.app.screen, Screen::Definition);
    assert!(!s.app.dictionary_navigation.view.query_editing);
}

#[test]
fn dictionary_navigation_restores_fixed_page_selection_geometry() {
    let mut s = Simulation::new();
    let path = s.fixture.root.join("Coast.pdf");
    fs::write(&path, crate::fixed::tests::document()).unwrap();
    s.app.open_path(&path).unwrap();
    let reading = &s.app.fixed.as_ref().unwrap().reading;
    let words = reading.page_text(0).unwrap();
    let (size, viewport) = reading.page_viewport().unwrap();
    let point = words.words[0]
        .bounds
        .in_viewport(size, viewport)
        .unwrap()
        .center();
    s.app.input(Input::LongTap(point.x as f32, point.y as f32));
    let original = s.app.pdf_selection.as_ref().unwrap();
    assert_eq!(original.text(), "PDF");
    let saved = (original.page(), original.bounds().collect::<Vec<_>>());
    let before = s.draw(None);
    for large in [false, true] {
        s.app.dictionary_settings.large_window = large;
        s.tap(s.app.selection_button(SelectionAction::Dictionary));
        assert_eq!(s.app.definition_expanded, large);
        s.draw(None);
        s.hold_definition_word("format");
        s.back();
        s.back();
        assert_eq!(s.app.screen, Screen::Fixed);
        let restored = s.app.pdf_selection.as_ref().unwrap();
        assert_eq!(
            (restored.page(), restored.bounds().collect::<Vec<_>>()),
            saved
        );
        assert_eq!(s.draw(None), before);
    }
}

#[test]
fn dictionary_navigation_wraps_through_results_and_restores_the_typed_query() {
    let mut s = Simulation::new();
    s.app.show(Screen::DictionaryEntry);
    s.app.editor.text = "the".chars().collect();
    s.app.editor.caret = 2;
    let saved = s.app.editor.clone();
    let go = Layout::new(WIDTH, HEIGHT, 4, 5)
        .keyboard_keys_with(s.app.editor.layer, "Look up")
        .into_iter()
        .find(|(key, _)| key.key == primitive_ui::Key::Go)
        .unwrap()
        .1;
    s.tap(go);
    s.draw(None);
    assert_eq!(s.app.input(Input::PageBack), Change::Partial);
    assert_eq!(s.app.list_page, 2);
    s.draw(None);
    assert_eq!(s.app.input(Input::PageBack), Change::Partial);
    s.draw(None);
    assert_eq!(s.app.list_page, 1);
    let rows = s.app.definition_layout().2;
    let view = &s.app.dictionary_navigation.view;
    assert!(view.first_line(rows) > 0);
    assert_eq!(
        view.first_line(rows) + rows,
        view.layout.get().unwrap().len()
    );
    s.tap(definition_results_button(s.app.definition_layout().0));
    s.tap(definition_result_row(s.app.definition_layout().0, 2));
    assert_eq!(s.app.list_page, 2);
    s.back();
    assert_eq!(s.app.screen, Screen::DictionaryEntry);
    assert_eq!(s.app.editor, saved);
}

#[test]
fn selection_viewport_epub_dictionary_uses_real_lookup_edit_and_held_return() {
    use crate::app::selection_viewport_tests::{adopt, capture, key, press, tap};
    use crate::session::ReadingViewport;
    use primitive_ui::Key;

    let mut s = Simulation::new();
    s.open_epub_selection();
    let original = s.app.current_history_location();
    let selected = s.app.selection.clone();
    let portrait = s.draw(None);
    let mut landscape = adopt(&mut s.app, ReadingViewport::LANDSCAPE, 3);
    let action = s.app.selection_button(SelectionAction::Dictionary);
    press(&s.app, &mut landscape, action, false);
    capture(
        &s.app,
        &mut landscape,
        "landscape-selection-actions.png",
        false,
    );
    tap(&mut s.app, action);
    assert_eq!(s.app.screen, Screen::Definition);
    assert_eq!(s.app.definition_word, "the");
    assert!(!s.app.definitions.is_empty());
    let (popup, close, _) = s.app.definition_layout();
    assert!(popup.bottom() < 1264 && close.right() > WIDTH as i32);
    let original_night_pages = s.app.controls.night_pages;
    s.app.controls.night_pages = true;
    press(&s.app, &mut landscape, close, true);
    capture(
        &s.app,
        &mut landscape,
        "landscape-dictionary-dark.png",
        true,
    );
    s.app.controls.night_pages = original_night_pages;

    let edit = definition_edit_button(popup, s.app.definitions.len() > 1);
    tap(&mut s.app, edit);
    assert!(s.app.dictionary_editor_active());
    let delete = key(&s.app, Key::Delete, "Look up");
    press(&s.app, &mut landscape, delete, false);
    tap(&mut s.app, delete);
    let back = s.app.reading_editor_back();
    assert!(back.x > WIDTH as i32);
    tap(&mut s.app, back);
    assert!(!s.app.dictionary_editor_active());

    s.hold_back();
    assert_eq!(s.app.screen, Screen::Book);
    assert_eq!(s.app.selection, selected);
    assert_eq!(s.app.current_history_location(), original);
    let mut restored = adopt(&mut s.app, ReadingViewport::PORTRAIT, 4);
    assert!(primitive_app::frame(&s.app, &mut restored, false) == portrait);
}
