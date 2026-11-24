use super::*;
use crate::app::tests::Fixture;
use crate::dictionary::tests::write_dictionary;

fn fonts() -> Vec<PathBuf> {
    let mut fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    if let Some(font) = std::env::var_os("READER_CJK_TEST_FONT") {
        fonts.push(font.into());
    }
    fonts
}
fn settle(app: &mut Reader) {
    let limit = Instant::now() + Duration::from_secs(10);
    while let Some(job) = &app.cjk_selection_lookup {
        assert!(
            Instant::now() < limit,
            "selection worker exceeded bounded test deadline"
        );
        app.poll_cjk_selection(job.deadline);
        if app.cjk_selection_lookup.is_some() {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
fn draw(app: &Reader, name: &str) {
    let Some(directory) = std::env::var_os("READER_CJK_CAPTURE_DIR") else {
        return;
    };
    let fonts: Vec<_> = fonts()
        .into_iter()
        .map(|f| f.to_string_lossy().into_owned())
        .collect();
    let mut canvas = Canvas::new(WIDTH, HEIGHT, &fonts).unwrap();
    let pixels = primitive_app::frame(app, &mut canvas, false);
    primitive_app::write_png(&Path::new(&directory).join(name), WIDTH, HEIGHT, &pixels).unwrap();
}
fn authored() -> (Fixture, Reader) {
    let fixture = Fixture::new();
    let text = fixture.root.join("Japanese.txt");
    fs::write(
        &text,
        "Reading notes\n\n読み込みました。本を読みませんでした。\n\n静かでした。中文辞典。ｶﾞｸｾｲ。\n",
    )
    .unwrap();
    let dictionaries = fixture.root.join("dictionaries");
    fs::create_dir(&dictionaries).unwrap();
    write_dictionary(
        &dictionaries,
        "Authored Japanese",
        "sametypesequence=m\n",
        &[
            (
                "読む",
                b"to read; an independently authored test definition",
            ),
            ("読み込む", b"to read thoroughly; to load data"),
            ("静か", b"quiet; calm"),
            ("中文辞典", b"Chinese dictionary"),
            ("ガクセイ", b"student; an authored normalization test"),
        ],
    );
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts()).unwrap();
    app.controls.dictionary_on_single_word = Some(false);
    app.open_path(&text).unwrap();
    (fixture, app)
}
fn selected(app: &Reader, word: &str) -> TextSelection {
    let probe = &app.active.as_ref().unwrap().reading.probe;
    let block = probe
        .source
        .blocks
        .iter()
        .find(|b| b.text.contains(word))
        .unwrap();
    let start = probe.anchor(block.key, block.text.find(word).unwrap());
    let mut end = start.clone();
    end.offset += word.len();
    probe.select_range(&start, &end).unwrap()
}
#[test]
fn japanese_lookup_expands_only_the_automatic_source_selection_and_restores_it() {
    let (_fixture, mut app) = authored();
    let initial = selected(&app, "読み");
    let rect = app
        .active
        .as_ref()
        .unwrap()
        .reading
        .highlight_rects(&initial)[0];
    app.input(Input::LongTap(
        (rect.x + rect.width / 4) as f32,
        (rect.y + rect.height / 2) as f32,
    ));
    settle(&mut app);
    let expanded = app.selection.clone().unwrap();
    assert_eq!(expanded.text, "読み込みました");
    assert_eq!(expanded.start, initial.start);
    let position = position(&app.active.as_ref().unwrap().reading);
    draw(&app, "japanese-selection.png");
    app.look_up_selection().unwrap();
    assert_eq!(app.definition_word, "読み込みました");
    assert_eq!(app.definitions.len(), 1);
    assert_eq!(app.definitions[0].word, "読み込む");
    assert!(
        app.lookup_history
            .dictionary
            .iter()
            .any(|entry| entry.word() == "読み込みました")
    );
    draw(&app, "japanese-dictionary.png");
    // The popup back control returns to the original source, never a fabricated
    // dictionary spelling or coordinates reconstructed from the popup.
    let back = app.definition_layout().1;
    app.input(Input::Tap(
        (back.x + back.w / 2) as f32,
        (back.y + back.h / 2) as f32,
    ));
    assert_eq!(app.screen, Screen::Book);
    assert_eq!(app.selection, Some(expanded));
    assert_eq!(
        position,
        super::super::position(&app.active.as_ref().unwrap().reading)
    );
    app.selection = Some(selected(&app, "読みませんでした"));
    app.look_up_selection().unwrap();
    assert_eq!(app.definition_word, "読みませんでした");
    assert_eq!(app.definitions[0].word, "読む");
}
#[test]
fn late_selection_jobs_cannot_overwrite_manual_ranges_sleep_or_disabled_forms() {
    let (_fixture, mut app) = authored();
    app.selection = Some(selected(&app, "読み"));
    app.start_cjk_selection();
    let manual = selected(&app, "本を読みませんでした");
    app.selection = Some(manual.clone());
    settle(&mut app);
    assert_eq!(app.selection, Some(manual));
    app.selection = Some(selected(&app, "読み"));
    app.start_cjk_selection();
    app.input(Input::Sleep);
    assert!(app.cjk_selection_lookup.is_none());
    app.input(Input::Wake);
    app.change_source(SourceAction::DefaultWordForms).unwrap();
    assert!(app.lookup_sources("読みませんでした").unwrap().is_empty());
    app.change_source(SourceAction::DefaultWordForms).unwrap();
    assert_eq!(app.lookup_sources("ｶﾞｸｾｲ").unwrap()[0].word, "ガクセイ");
    assert_eq!(app.lookup_sources("静かでした").unwrap()[0].word, "静か");
    app.selection = Some(selected(&app, "中文"));
    app.start_cjk_selection();
    settle(&mut app);
    assert_eq!(app.selection.as_ref().unwrap().text, "中文辞典");
}

#[test]
fn book_dictionary_exclusions_keep_automatic_cjk_selection_on_enabled_sources() {
    let (_fixture, mut app) = authored();
    let initial = selected(&app, "読み");
    app.selection = Some(initial.clone());
    app.start_cjk_selection();
    settle(&mut app);
    assert_eq!(app.selection.as_ref().unwrap().text, "読み込みました");
    let position = app.current_history_location();

    app.activate(Action::ShowDictionaries).unwrap();
    app.change_source(SourceAction::Open(0)).unwrap();
    app.change_source(SourceAction::ToggleForBook).unwrap();
    app.show(Screen::Book);
    app.selection = Some(initial.clone());
    app.start_cjk_selection();
    settle(&mut app);
    assert_eq!(app.selection, Some(initial));
    assert!(app.lookup_sources("読み込みました").unwrap().is_empty());
    assert_eq!(app.current_history_location(), position);
}

#[test]
#[ignore = "Requires a cached CJK publication and a supplied local font"]
fn cached_cjk_publication_selection_preserves_source_and_position() {
    let path = PathBuf::from(std::env::var_os("READER_CJK_ORIGINAL").expect("publication"));
    let chapter: usize = std::env::var("READER_CJK_CHAPTER")
        .unwrap()
        .parse()
        .unwrap();
    let word = std::env::var("READER_CJK_WORD").expect("source phrase");
    let base = std::env::var("READER_CJK_BASE").expect("independent dictionary headword");
    let original = fs::read(&path).unwrap();
    let fixture = Fixture::new();
    let copy = fixture.root.join(format!(
        "Publication.{}",
        path.extension().unwrap().to_string_lossy()
    ));
    fs::copy(&path, &copy).unwrap();
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts()).unwrap();
    app.open_path(&copy).unwrap();
    let href = app.active.as_ref().unwrap().reading.probe.chapter_entries()[chapter]
        .href
        .clone();
    app.follow(Destination {
        path: href,
        fragment: None,
    })
    .unwrap();
    let selection = selected(&app, &word);
    let reading = &mut app.active.as_mut().unwrap().reading;
    reading.jump_to_anchor(chapter, &selection.start).unwrap();
    assert!(!reading.highlight_rects(&selection).is_empty());
    let original_position = position(reading);
    let dictionaries = fixture.root.join("dictionaries");
    fs::create_dir(&dictionaries).unwrap();
    write_dictionary(
        &dictionaries,
        "Publication verification",
        "sametypesequence=m\n",
        &[(
            base.as_str(),
            b"An independently authored dictionary entry used to verify source selection.",
        )],
    );
    app.selection = Some(selection.clone());
    app.start_cjk_selection();
    settle(&mut app);
    assert_eq!(app.selection, Some(selection.clone()));
    app.look_up_selection().unwrap();
    assert_eq!(app.definition_word, selection.text);
    assert_eq!(app.definitions[0].word, base);
    assert_eq!(
        position(&app.active.as_ref().unwrap().reading),
        original_position
    );
    draw(&app, "publication-cjk-dictionary.png");
    assert_eq!(fs::read(&path).unwrap(), original);
}
