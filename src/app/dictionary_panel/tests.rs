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
        let app = Reader::open(&fixture.root, &fixture.state, vec![font.clone()]).unwrap();
        let canvas = Canvas::new(WIDTH, HEIGHT, &[font.to_string_lossy().into_owned()]).unwrap();
        Self {
            fixture,
            app,
            canvas,
        }
    }

    fn draw(&mut self, name: Option<&str>) -> Vec<u8> {
        let pixels = primitive_app::frame(&self.app, &mut self.canvas, false);
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
                    .map(|(_, left, right)| (row, left, right))
            })
            .unwrap();
        assert_eq!(
            self.app.input(Input::LongTap(
                (body.x + (left + right) / 2) as f32,
                (body.y + row as i32 * LINE + LINE / 2) as f32
            )),
            Change::Partial
        );
        assert_eq!(self.app.definition_word, word);
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
    s.tap(s.app.selection_button(SelectionAction::Dictionary));
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
