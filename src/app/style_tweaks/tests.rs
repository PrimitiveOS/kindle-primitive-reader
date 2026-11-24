use super::*;
use crate::app::selection_viewport_tests::{adopt, capture as capture_viewport, press, tap};
use crate::app::tests::{Fixture, capture_test_screen, tap_contents_item};
use crate::session::ReadingViewport;
fn fonts() -> Vec<PathBuf> {
    vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")]
}
fn open(f: &Fixture) -> Reader {
    Reader::open(&f.root, &f.state, fonts()).unwrap()
}
fn source_book(f: &Fixture) -> PathBuf {
    let p = f.root.join("Style checks.xhtml");
    fs::write(&p,r#"<html><head><title>Style checks</title></head><body><p id="passage" style="text-indent:2em!important;padding-left:24px!important;font-size:150%;font-weight:300!important">Harbor lights guide the boats home. The tide carries a quiet rhythm along the coast.</p><p>A second paragraph keeps the source mapping visible.</p></body></html>"#).unwrap();
    p
}
fn capture(app: &mut Reader) -> Vec<u8> {
    let b = &mut app.active.as_mut().unwrap().reading;
    b.probe.capture(b.page).unwrap()
}
fn choose(app: &mut Reader, id: &str, value: Option<bool>) {
    app.activate_style(Command::Preset(id.into())).unwrap();
    app.activate_style(Command::Choose(id.into(), value))
        .unwrap();
}
#[test]
fn styles_live_defaults_and_book_overrides_survive_profiles_defaults_restart_and_source_selections()
{
    let f = Fixture::new();
    let path = source_book(&f);
    let original = fs::read(&path).unwrap();
    let mut app = open(&f);
    app.open_path(&path).unwrap();
    let anchor = app
        .active
        .as_ref()
        .unwrap()
        .reading
        .probe
        .fragment("passage")
        .unwrap();
    let mut end = anchor.clone();
    end.offset += "Harbor lights".len();
    let selected = app
        .active
        .as_ref()
        .unwrap()
        .reading
        .probe
        .select_range(&anchor, &end)
        .unwrap();
    let before = capture(&mut app);
    app.activate_style(Command::Preset("paragraph-indent-none".into()))
        .unwrap();
    tap_contents_item(&mut app, "Use on all books");
    assert_ne!(capture(&mut app), before);
    let id = app.active.as_ref().unwrap().id;
    assert!(app.style_store.data.enabled(id, "paragraph-indent-none"));
    let second = f.root.join("Second.xhtml");
    fs::copy(&path, &second).unwrap();
    fs::write(
        &second,
        fs::read_to_string(&second)
            .unwrap()
            .replace("Style checks", "Second checks"),
    )
    .unwrap();
    app.open_path(&second).unwrap();
    let second_id = app.active.as_ref().unwrap().id;
    assert_ne!(second_id, id);
    assert!(
        app.style_store
            .data
            .enabled(second_id, "paragraph-indent-none")
    );
    choose(&mut app, "paragraph-indent-none", Some(false));
    assert!(
        !app.style_store
            .data
            .enabled(second_id, "paragraph-indent-none")
    );
    app.open_path(&path).unwrap();
    choose(&mut app, "inherit-font-size", Some(true));
    assert_eq!(
        app.active
            .as_ref()
            .unwrap()
            .reading
            .probe
            .select_range(&anchor, &end)
            .unwrap()
            .text,
        selected.text
    );
    let page = app
        .active
        .as_ref()
        .unwrap()
        .reading
        .probe
        .page_for(&anchor)
        .unwrap();
    assert!(
        !app.active
            .as_ref()
            .unwrap()
            .reading
            .probe
            .selection_rects(page, &selected)
            .is_empty()
    );
    app.save_reading_profile("Typography only").unwrap();
    let snapshot = app
        .active
        .as_ref()
        .unwrap()
        .reading
        .config
        .user_styles
        .clone();
    app.activate(Action::ApplyProfile(0)).unwrap();
    assert_eq!(
        app.active.as_ref().unwrap().reading.config.user_styles,
        snapshot
    );
    app.activate_document_defaults(crate::app::default_settings::Command::SaveReflow)
        .unwrap();
    app.save().unwrap();
    let mut reopened = open(&f);
    reopened.open_path(&path).unwrap();
    assert_eq!(
        reopened.active.as_ref().unwrap().reading.config.user_styles,
        snapshot
    );
    reopened.activate_style(Command::Master(false)).unwrap();
    assert!(
        reopened
            .active
            .as_ref()
            .unwrap()
            .reading
            .config
            .user_styles
            .is_empty()
    );
    assert!(reopened.style_store.data.books[&id].choices["inherit-font-size"]);
    reopened.activate_style(Command::Master(true)).unwrap();
    assert_eq!(
        reopened.active.as_ref().unwrap().reading.config.user_styles,
        snapshot
    );
    reopened.open_path(&second).unwrap();
    assert!(
        !reopened
            .style_store
            .data
            .enabled(second_id, "paragraph-indent-none")
    );
    assert_eq!(fs::read(&path).unwrap(), original);
}
#[test]
fn styles_failed_relayout_or_save_retains_the_frame_model_position_and_recoverable_draft() {
    let f = Fixture::new();
    let path = source_book(&f);
    let mut app = open(&f);
    app.open_path(&path).unwrap();
    let frame = capture(&mut app);
    let location = app.current_history_location();
    let old = app.style_store.data.clone();
    let snapshot = app
        .active
        .as_ref()
        .unwrap()
        .reading
        .config
        .user_styles
        .clone();
    let mut next = old.clone();
    next.choose(
        app.active.as_ref().unwrap().id,
        "paragraph-indent-none",
        Some(true),
    )
    .unwrap();
    let moved = path.with_extension("held");
    fs::rename(&path, &moved).unwrap();
    assert!(app.apply_style_data(next.clone()).is_err());
    fs::rename(&moved, &path).unwrap();
    assert_eq!(capture(&mut app), frame);
    assert_eq!(app.style_store.data, old);
    let target = app.style_store.path.clone();
    fs::create_dir(&target).unwrap();
    assert!(app.apply_style_data(next).is_err());
    assert_eq!(capture(&mut app), frame);
    assert_eq!(app.current_history_location(), location);
    assert_eq!(
        app.active.as_ref().unwrap().reading.config.user_styles,
        snapshot
    );
    assert_eq!(app.style_store.data, old);
    app.activate_style(Command::Edit).unwrap();
    app.style_view.editor.as_mut().unwrap().editor.text =
        "p{font-weight:700!important}".chars().collect();
    app.activate_style(Command::Review).unwrap();
    assert!(app.activate_style(Command::Apply).is_err());
    assert!(app.style_view.draft.is_some());
    assert!(app.style_view.editor.is_some());
    assert_eq!(capture(&mut app), frame);
}
fn key(app: &mut Reader, wanted: primitive_ui::Key) {
    let editor = &app.style_view.editor.as_ref().unwrap().editor;
    let layout = app.navigation_layout().keyboard();
    let (_, r) = layout
        .keyboard_keys_with(editor.layer, "New line")
        .into_iter()
        .find(|(k, _)| k.key == wanted)
        .unwrap_or_else(|| panic!("Missing keyboard key {wanted:?}"));
    assert_eq!(
        app.input(Input::Tap((r.x + r.w / 2) as f32, (r.y + r.h / 2) as f32)),
        Change::Partial
    );
}
fn type_css(app: &mut Reader, text: &str) {
    for c in text.chars() {
        if c == '\n' {
            key(app, primitive_ui::Key::Go);
            continue;
        }
        if c == ' ' {
            key(app, primitive_ui::Key::Space);
            continue;
        }
        let wanted = primitive_ui::Key::Char(c);
        let layer = app.style_view.editor.as_ref().unwrap().editor.layer;
        let layout = app.navigation_layout().keyboard();
        if !layout
            .keyboard_keys_with(layer, "New line")
            .iter()
            .any(|(k, _)| k.key == wanted)
        {
            let next = if c.is_ascii_alphanumeric() {
                primitive_ui::Layer::Lower
            } else {
                primitive_ui::Layer::Symbols
            };
            if layer != next {
                key(app, primitive_ui::Key::Layer(next));
            }
        }
        key(app, wanted);
    }
}

#[test]
fn styles_viewport_fits_publisher_and_base_choices_with_large_night_toolbars() {
    let fixture = Fixture::new();
    let path = source_book(&fixture);
    let mut app = open(&fixture);
    app.open_path(&path).unwrap();
    let origin = app.current_history_location();
    let landscape = ReadingViewport::LANDSCAPE;
    let mut wide = adopt(&mut app, landscape, 2);
    app.system_theme = None;
    app.controls.night_pages = true;
    app.controls.large_toolbar = true;
    app.open_reading_edge(false, 840.0);
    let rendering = app.reading_button("Rendering");
    tap(&mut app, rendering);
    let rendering = app.reading_button("Rendering");
    assert!(rendering.y >= 0 && rendering.h >= 100);
    let stylesheet = app.reading_button_in_row("Base stylesheet", "Traditional");
    let embedded = app.reading_button_in_row("Publisher CSS", "Off");
    assert!(embedded.y >= 0 && embedded.bottom() <= landscape.dimensions().1 as i32);
    press(&app, &mut wide, embedded, true);
    tap(&mut app, embedded);
    press(&app, &mut wide, stylesheet, true);
    tap(&mut app, stylesheet);
    let settings = app.active.as_ref().unwrap().reading.config.settings;
    assert_eq!(settings.base_stylesheet, crate::BaseStylesheet::Traditional);
    assert!(!settings.publisher_styles);
    assert_eq!(app.current_history_location(), origin);
    capture_viewport(&app, &mut wide, "landscape-publisher-styles.png", true);
    let open_styles = app.reading_button_in_row("Style tweaks", "Open");
    tap(&mut app, open_styles);
    assert_eq!(app.screen, Screen::StyleTweaks);
    assert_eq!(app.list_geometry().rows, 7);
    assert_eq!(app.input(Input::PageForward), Change::Partial);
    let first = app.list_window().1[0].title.clone();
    adopt(&mut app, ReadingViewport::PORTRAIT, 3);
    assert_eq!(app.reading_list_ordinal(), 7);
    adopt(&mut app, landscape, 4);
    assert_eq!(app.list_window().1[0].title, first);
    tap_contents_item(&mut app, &first);
    assert_eq!(app.screen, Screen::StylePreset);
    tap_contents_item(&mut app, "On for this book");
    assert_eq!(app.current_history_location(), origin);
    app.save().unwrap();
    let mut reopened = open(&fixture);
    reopened.open_path(&path).unwrap();
    let settings = reopened.active.as_ref().unwrap().reading.config.settings;
    assert_eq!(settings.base_stylesheet, crate::BaseStylesheet::Traditional);
    assert!(!settings.publisher_styles);
}

#[test]
fn styles_viewport_preserves_css_editor_paging_and_review_apply_cancel() {
    use primitive_app::viewport::{LogicalTurn, ViewportDecision};

    let fixture = Fixture::new();
    let path = source_book(&fixture);
    let original = fs::read(&path).unwrap();
    let mut app = open(&fixture);
    app.open_path(&path).unwrap();
    let id = app.open_book_id().unwrap();
    let origin = app.current_history_location();
    let mut wide = adopt(&mut app, ReadingViewport::LANDSCAPE, 2);
    app.activate_style(Command::Edit).unwrap();
    let layout = EditorLayout::new(app.navigation_layout());
    assert_eq!(layout.rows, 4);
    assert_eq!(layout.hint.bottom(), layout.keyboard.keyboard_area().y);
    assert!(layout.text.bottom() < layout.hint.y);
    let css = "p{font-weight:900!important;}\n".repeat(8);
    type_css(&mut app, &css);
    let editor = app.style_view.editor.as_ref().unwrap();
    assert_eq!(editor.editor.string(), css);
    let first_line = editor.first_line;
    assert!(first_line > 0);
    let caret = editor.editor.caret;
    let before_rotation = primitive_app::frame(&app, &mut wide, false);
    let candidate =
        reading_viewport_stage::draw_test_candidate(&mut app, 3, LogicalTurn::Upright, "", false);
    app.finish_viewport(candidate, ViewportDecision::Abort);
    assert_eq!(
        primitive_app::frame(&app, &mut wide, false),
        before_rotation
    );
    let editor = app.style_view.editor.as_ref().unwrap();
    assert_eq!(
        (editor.first_line, editor.editor.caret),
        (first_line, caret)
    );
    let mut portrait = adopt(&mut app, ReadingViewport::PORTRAIT, 4);
    let editor = app.style_view.editor.as_ref().unwrap();
    assert_eq!(
        (editor.first_line, editor.editor.caret),
        (first_line, caret)
    );
    assert_eq!(EditorLayout::new(app.navigation_layout()).text, TEXT);
    press(&app, &mut portrait, REVIEW, false);
    assert_eq!(app.input(Input::PageBack), Change::Partial);
    assert_eq!(app.style_view.editor.as_ref().unwrap().first_line, 0);
    wide = adopt(&mut app, ReadingViewport::LANDSCAPE, 5);
    capture_viewport(&app, &mut wide, "landscape-book-css-editor.png", false);
    let review_edge = Rect::new(layout.review.right() - 100, layout.review.y + 4, 80, 80);
    press(&app, &mut wide, review_edge, false);
    tap(&mut app, review_edge);
    assert_eq!(app.screen, Screen::StyleReview);
    assert!(app.style_view.error.is_none());
    assert!(app.style_store.data.book(id).custom.is_none());
    let back = app.navigation_layout().back();
    tap(&mut app, back);
    assert_eq!(app.screen, Screen::StyleEditor);
    assert_eq!(app.style_view.editor.as_ref().unwrap().editor.string(), css);
    tap(&mut app, review_edge);
    let before = capture(&mut app);
    tap_contents_item(&mut app, "Apply to this book");
    assert_eq!(app.screen, Screen::Book);
    assert_eq!(
        app.style_store.data.book(id).custom.as_deref(),
        Some(css.as_str())
    );
    assert_ne!(capture(&mut app), before);
    assert_eq!(app.current_history_location(), origin);

    app.activate_style(Command::Edit).unwrap();
    type_css(&mut app, "p{font-weight:100}");
    tap(&mut app, layout.cancel);
    assert_eq!(app.screen, Screen::StyleTweaks);
    assert_eq!(
        app.style_store.data.book(id).custom.as_deref(),
        Some(css.as_str())
    );
    app.save().unwrap();
    let mut reopened = open(&fixture);
    reopened.open_path(&path).unwrap();
    assert_eq!(
        reopened.style_store.data.book(id).custom.as_deref(),
        Some(css.as_str())
    );
    assert_eq!(reopened.current_history_location(), origin);
    assert_eq!(fs::read(&path).unwrap(), original);
}
#[test]
fn styles_multiline_keyboard_review_apply_cancel_and_sleep_are_real_input_paths() {
    let f = Fixture::new();
    let mut app = open(&f);
    app.open_path(&f.book).unwrap();
    app.open_reading_edge(false, 500.);
    let rect = app.reading_button("Rendering");
    assert_eq!(
        app.input(Input::Tap(
            (rect.x + rect.w / 2) as f32,
            (rect.y + rect.h / 2) as f32
        )),
        Change::Partial
    );
    capture_test_screen(&app, &fonts(), "READER_STYLE_RENDERING", false);
    let rect = app.reading_button("Open");
    assert_eq!(
        app.input(Input::Tap(
            (rect.x + rect.w / 2) as f32,
            (rect.y + rect.h / 2) as f32
        )),
        Change::Partial
    );
    assert_eq!(app.screen, Screen::StyleTweaks);
    tap_contents_item(&mut app, "Edit book CSS");
    let css = "p{font-size:100%;font-weight:700!important;}\n#x>.a+p{padding-left:0;}\n*{font-style:normal;}";
    type_css(&mut app, css);
    assert_eq!(app.style_view.editor.as_ref().unwrap().editor.string(), css);
    app.input(Input::Sleep);
    assert!(!app.stats_awake);
    assert_eq!(app.screen, Screen::StyleEditor);
    app.input(Input::Wake);
    assert!(app.stats_awake);
    assert_eq!(app.style_view.editor.as_ref().unwrap().editor.string(), css);
    capture_test_screen(&app, &fonts(), "READER_STYLE_EDITOR", false);
    app.input(Input::Tap((REVIEW.x + 20) as f32, (REVIEW.y + 20) as f32));
    assert_eq!(app.screen, Screen::StyleReview);
    assert!(app.style_view.error.is_none());
    capture_test_screen(&app, &fonts(), "READER_STYLE_REVIEW", false);
    capture_test_screen(&app, &fonts(), "READER_STYLE_REVIEW_DARK", true);
    tap_contents_item(&mut app, "Apply to this book");
    let id = app.active.as_ref().unwrap().id;
    assert_eq!(app.style_store.data.books[&id].custom.as_deref(), Some(css));
    assert_eq!(app.screen, Screen::Book);
    app.activate_style(Command::Edit).unwrap();
    type_css(&mut app, "p{font-weight:300}");
    app.input(Input::Tap((CANCEL.x + 20) as f32, (CANCEL.y + 20) as f32));
    assert_eq!(app.style_store.data.books[&id].custom.as_deref(), Some(css));
    app.show(Screen::StyleTweaks);
    capture_test_screen(&app, &fonts(), "READER_STYLE_LIST", false);
    app.activate_style(Command::Preset("link-bold".into()))
        .unwrap();
    capture_test_screen(&app, &fonts(), "READER_STYLE_PRESET", false);
    let mut reopened = open(&f);
    reopened.open_path(&f.book).unwrap();
    assert_eq!(
        reopened.style_store.data.books[&id].custom.as_deref(),
        Some(css)
    );
}
#[test]
fn styles_import_diagnostics_disabled_edit_and_stale_book_guard_preserve_source_files() {
    let f = Fixture::new();
    let mut app = open(&f);
    app.open_path(&f.book).unwrap();
    let dir = f.root.join("style-tweaks");
    fs::create_dir(&dir).unwrap();
    let path = dir.join("chapter.css");
    let css = b"p {font-weight:700 !important}";
    fs::write(&path, css).unwrap();
    app.activate_style(Command::Files).unwrap();
    tap_contents_item(&mut app, "chapter.css");
    assert!(app.style_view.error.is_none());
    tap_contents_item(&mut app, "Apply to this book");
    let id = app.active.as_ref().unwrap().id;
    assert_eq!(fs::read(&path).unwrap(), css);
    app.activate_style(Command::ToggleCustom).unwrap();
    assert!(!app.style_store.data.books[&id].custom_enabled);
    app.activate_style(Command::Edit).unwrap();
    app.style_view.editor.as_mut().unwrap().editor.text = "p {font-weight:400}".chars().collect();
    app.activate_style(Command::Review).unwrap();
    app.activate_style(Command::Apply).unwrap();
    assert!(
        !app.style_store.data.books[&id].custom_enabled,
        "editing disabled CSS does not silently enable it"
    );
    fs::write(&path, b"p {display:none}").unwrap();
    app.activate_style(Command::Files).unwrap();
    tap_contents_item(&mut app, "chapter.css");
    assert!(app.style_view.error.as_deref().unwrap().contains("display"));
    assert!(
        !app.items()
            .iter()
            .any(|r| matches!(r.action, Action::Style(Command::Apply)))
    );
    assert!(app.activate_style(Command::Apply).is_err());
    app.open_path(&source_book(&f)).unwrap();
    let next_id = app.active.as_ref().unwrap().id;
    assert!(app.activate_style(Command::Apply).is_err());
    assert!(!app.style_store.data.books.contains_key(&next_id));
    assert_eq!(fs::read(path).unwrap(), b"p {display:none}");
}
#[test]
#[ignore = "requires unchanged authorized READER_STYLE_ORIGINAL EPUB/AZW3/MOBI"]
fn styles_original_publication_changes_visible_ink_and_retains_the_same_quote_after_reopen() {
    let f = Fixture::new();
    let path = PathBuf::from(std::env::var_os("READER_STYLE_ORIGINAL").unwrap());
    let original = fs::read(&path).unwrap();
    let mut app = Reader::open(path.parent().unwrap(), &f.state, fonts()).unwrap();
    app.open_path(&path).unwrap();
    let reading = &mut app.active.as_mut().unwrap().reading;
    let (chapter, probe) = (0..reading.probe.chapter_count())
        .find_map(|index| {
            let p = reading.config.chapter(index).ok()?;
            p.source
                .blocks
                .iter()
                .any(|b| b.text.split_whitespace().count() > 3)
                .then_some((index, p))
        })
        .unwrap();
    let block = probe
        .source
        .blocks
        .iter()
        .find(|b| b.text.split_whitespace().count() > 3)
        .unwrap();
    let word = block.text.split_whitespace().next().unwrap();
    let offset = word.as_ptr() as usize - block.text.as_ptr() as usize;
    let anchor = probe.anchor(block.key, offset);
    let end = probe.anchor(block.key, offset + word.len());
    let selected = probe.select_range(&anchor, &end).unwrap();
    reading.jump_to_anchor(chapter, &anchor).unwrap();
    let initial = capture(&mut app);
    app.activate_style(Command::Edit).unwrap();
    app.style_view.editor.as_mut().unwrap().editor.text =
        "*{font-weight:900!important;}".chars().collect();
    app.activate_style(Command::Review).unwrap();
    app.activate_style(Command::Apply).unwrap();
    assert_ne!(capture(&mut app), initial);
    let reading = &app.active.as_ref().unwrap().reading;
    let page = reading.probe.page_for(&anchor).unwrap();
    assert_eq!(
        reading.probe.select_range(&anchor, &end).unwrap().text,
        selected.text
    );
    assert!(!reading.probe.selection_rects(page, &selected).is_empty());
    app.save().unwrap();
    let snapshot = app
        .active
        .as_ref()
        .unwrap()
        .reading
        .config
        .user_styles
        .clone();
    let mut reopened = Reader::open(path.parent().unwrap(), &f.state, fonts()).unwrap();
    reopened.open_path(&path).unwrap();
    assert_eq!(
        reopened.active.as_ref().unwrap().reading.config.user_styles,
        snapshot
    );
    assert_eq!(
        reopened
            .active
            .as_ref()
            .unwrap()
            .reading
            .probe
            .select_range(&anchor, &end)
            .unwrap()
            .text,
        selected.text
    );
    reopened.activate_style(Command::Master(false)).unwrap();
    let r = &mut reopened.active.as_mut().unwrap().reading;
    r.jump_to_anchor(chapter, &anchor).unwrap();
    assert_eq!(capture(&mut reopened), initial);
    assert_eq!(fs::read(path).unwrap(), original);
}

#[test]
fn styles_backup_component_round_trips_live_defaults_and_rejects_unsupported_css_before_mutation() {
    let source = Fixture::new();
    let mut app = open(&source);
    app.open_path(&source.book).unwrap();
    app.activate_style(Command::Preset("paragraph-indent-none".into()))
        .unwrap();
    app.activate_style(Command::Global("paragraph-indent-none".into(), true))
        .unwrap();
    app.activate_style(Command::Edit).unwrap();
    app.style_view.editor.as_mut().unwrap().editor.text =
        "p{font-weight:700!important}".chars().collect();
    app.activate_style(Command::Review).unwrap();
    app.activate_style(Command::Apply).unwrap();
    let expected = app.style_store.data.clone();
    let bytes = app.reading_backup_bytes().unwrap();
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(
        value["components"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["key"]["kind"] == "style_tweaks")
    );
    let target = Fixture::new();
    let mut restored = open(&target);
    let mut invalid = value.clone();
    let component = invalid["components"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|c| c["key"]["kind"] == "style_tweaks")
        .unwrap();
    component["data"]["version"] = serde_json::json!(99);
    assert!(
        restored
            .prepare_reading_backup(&serde_json::to_vec(&invalid).unwrap())
            .is_err()
    );
    assert!(!target.root.join(user_styles::FILE).exists());
    restored.backup_import = Some(restored.prepare_reading_backup(&bytes).unwrap());
    restored.apply_reading_backup().unwrap();
    assert_eq!(restored.style_store.data, expected);
    restored.open_path(&target.book).unwrap();
    let id = restored.active.as_ref().unwrap().id;
    assert_eq!(
        restored.active.as_ref().unwrap().reading.config.user_styles,
        expected.snapshot(id).unwrap()
    );
}

fn footnote_book(f: &Fixture, name: &str) -> PathBuf {
    let path = f.root.join(name);
    fs::copy(crate::test_fixture("in-page-footnotes.epub"), &path).unwrap();
    path
}

fn has_note_band(app: &Reader) -> bool {
    let reading = &app.active.as_ref().unwrap().reading;
    reading
        .probe
        .in_page_notes
        .as_ref()
        .is_some_and(|pages| pages.iter().any(|page| page.separator.is_some()))
}

#[test]
fn fb2_in_page_footnotes_use_independent_book_styles_and_resume() {
    let fixture = Fixture::new();
    let path = fixture.root.join("Survey.fb2");
    fs::copy(crate::test_fixture("in-page-notes.fb2"), &path).unwrap();
    let original = fs::read(&path).unwrap();
    let mut app = open(&fixture);
    app.open_path(&path).unwrap();
    let position = app.current_history_location();
    let id = app.open_book_id().unwrap();
    assert!(!has_note_band(&app));

    for label in [
        "In-page FB2 footnotes",
        "In-page FB2 endnotes",
        "Keep regular FB2 note size",
    ] {
        app.show(Screen::StyleTweaks);
        tap_contents_item(&mut app, label);
        tap_contents_item(&mut app, "On for this book");
        assert!(has_note_band(&app));
        assert_eq!(app.current_history_location(), position);
    }
    assert!(app.style_store.data.enabled(id, FB2_FOOTNOTES));
    assert!(app.style_store.data.enabled(id, FB2_ENDNOTES));
    assert!(app.style_store.data.enabled(id, FB2_REGULAR_NOTE_SIZE));
    let reading_frame = capture(&mut app);
    app.show(Screen::Book);
    app.activate(Action::ReadingMode(true)).unwrap();
    assert!(!has_note_band(&app));
    app.activate(Action::ReadingMode(false)).unwrap();
    assert!(has_note_band(&app));
    app.save().unwrap();
    drop(app);

    let mut app = open(&fixture);
    app.open_path(&path).unwrap();
    assert_eq!(app.current_history_location(), position);
    assert_eq!(capture(&mut app), reading_frame);
    app.show(Screen::StyleTweaks);
    tap_contents_item(&mut app, "In-page FB2 footnotes");
    tap_contents_item(&mut app, "Off for this book");
    assert!(!app.style_store.data.enabled(id, FB2_FOOTNOTES));
    assert!(app.style_store.data.enabled(id, FB2_ENDNOTES));
    assert!(has_note_band(&app));
    let mut canvas = crate::app::selection_viewport_tests::canvas(&app, ReadingViewport::PORTRAIT);
    app.show(Screen::StyleTweaks);
    capture_viewport(&app, &mut canvas, "in-page-fb2-style.png", false);
    assert_eq!(app.current_history_location(), position);
    assert_eq!(fs::read(&path).unwrap(), original);
}

#[test]
fn in_page_footnote_size_uses_saved_book_and_default_choices() {
    let fixture = Fixture::new();
    let path = footnote_book(&fixture, "Size choices.epub");
    let original = fs::read(&path).unwrap();
    let mut app = open(&fixture);
    app.open_path(&path).unwrap();
    let id = app.open_book_id().unwrap();
    let origin = app.current_history_location();
    app.show(Screen::StyleTweaks);
    tap_contents_item(&mut app, "In-page EPUB footnotes");
    tap_contents_item(&mut app, "Use on all books");
    assert!(has_note_band(&app));
    let legacy = capture(&mut app);
    app.show(Screen::StyleTweaks);
    tap_contents_item(&mut app, "In-page footnote size");
    tap_contents_item(&mut app, "65%");
    let small = capture(&mut app);
    assert_ne!(small, legacy);
    assert_eq!(app.current_history_location(), origin);
    assert_eq!(
        app.style_store.data.book(id).footnote_size,
        Some(FootnoteSize::SixtyFive)
    );
    tap_contents_item(&mut app, "Default size for all books");
    tap_contents_item(&mut app, "100%");
    assert_eq!(capture(&mut app), small);
    let back = app.navigation_layout().back();
    tap(&mut app, back);
    assert_eq!(app.screen, Screen::InPageFootnoteSize(NoteSizeScope::Book));
    let mut screen = crate::app::selection_viewport_tests::canvas(&app, ReadingViewport::PORTRAIT);
    press(&app, &mut screen, app.list_geometry().row(2), false);
    capture_viewport(
        &app,
        &mut screen,
        "in-page-footnote-size-choices.png",
        false,
    );
    let mut wide = adopt(&mut app, ReadingViewport::LANDSCAPE, 2);
    press(&app, &mut wide, app.list_geometry().row(2), true);
    capture_viewport(&app, &mut wide, "in-page-footnote-size-landscape.png", true);
    adopt(&mut app, ReadingViewport::PORTRAIT, 3);
    assert_eq!(app.current_history_location(), origin);
    app.show(Screen::Book);
    app.save().unwrap();
    drop(app);

    let mut app = open(&fixture);
    app.open_path(&path).unwrap();
    assert_eq!(capture(&mut app), small);
    assert_eq!(app.current_history_location(), origin);
    assert_eq!(app.style_store.data.footnote_size, FootnoteSize::Full);
    app.show(Screen::StyleTweaks);
    tap_contents_item(&mut app, "In-page footnote size");
    tap_contents_item(&mut app, "Use default");
    assert_eq!(app.style_store.data.book(id).footnote_size, None);
    assert_ne!(capture(&mut app), small);
    assert_eq!(app.current_history_location(), origin);
    assert_eq!(fs::read(&path).unwrap(), original);
}

#[test]
fn in_page_footnote_style_reflows_from_visible_controls_and_preserves_the_passage() {
    let f = Fixture::new();
    let path = footnote_book(&f, "Footnotes.epub");
    let original = fs::read(&path).unwrap();
    let mut app = open(&f);
    app.open_path(&path).unwrap();
    let position = app.active.as_ref().unwrap().reading.anchor();
    let baseline = capture(&mut app);
    assert!(!has_note_band(&app));

    app.activate(Action::Show(Screen::StyleTweaks)).unwrap();
    tap_contents_item(&mut app, "In-page EPUB footnotes");
    assert_eq!(app.screen, Screen::StylePreset);
    tap_contents_item(&mut app, "On for this book");
    assert!(has_note_band(&app));
    assert_ne!(capture(&mut app), baseline);
    assert_eq!(app.active.as_ref().unwrap().reading.anchor(), position);
    let mut screen = crate::app::selection_viewport_tests::canvas(&app, ReadingViewport::PORTRAIT);
    capture_viewport(&app, &mut screen, "in-page-footnote-style.png", false);
    app.show(Screen::Book);
    capture_viewport(&app, &mut screen, "in-page-footnote-reading.png", false);
    let active = app.active.as_ref().unwrap();
    assert!(
        active.reading.probe.in_page_notes.as_ref().unwrap()[active.reading.page]
            .separator
            .is_some()
    );
    assert!(active.reading.visible_text().contains("The repair date"));

    app.activate(Action::ReadingMode(true)).unwrap();
    assert!(!has_note_band(&app));
    assert!(
        app.style_store
            .data
            .enabled(app.open_book_id().unwrap(), EPUB_FOOTNOTES)
    );
    app.activate(Action::ReadingMode(false)).unwrap();
    assert!(has_note_band(&app));

    app.activate(Action::FacingPages(true)).unwrap();
    assert!(!has_note_band(&app));
    app.activate(Action::FacingPages(false)).unwrap();
    assert!(has_note_band(&app));
    let current = app.active.as_ref().unwrap().reading.anchor();

    app.show(Screen::StyleTweaks);
    tap_contents_item(&mut app, "Enable style tweaks");
    assert!(!has_note_band(&app));
    assert_eq!(capture(&mut app), baseline);
    assert!(
        app.activate_style(Command::Preset(EPUB_FOOTNOTES.into()))
            .is_err()
    );
    tap_contents_item(&mut app, "Enable style tweaks");
    assert!(has_note_band(&app));
    assert_eq!(app.active.as_ref().unwrap().reading.anchor(), current);
    app.active
        .as_mut()
        .unwrap()
        .reading
        .go_to_page(2, 0)
        .unwrap();
    assert!(!has_note_band(&app));
    assert!(
        app.active
            .as_ref()
            .unwrap()
            .reading
            .visible_text()
            .contains("Interleaved note")
    );
    assert_eq!(fs::read(&path).unwrap(), original);
}

#[test]
fn in_page_footnote_style_defaults_persist_and_book_override_does_not_change_another_epub() {
    let f = Fixture::new();
    let one = footnote_book(&f, "First notes.epub");
    let two = footnote_book(&f, "Second notes.epub");
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&two)
        .unwrap();
    let mut archive = zip::ZipWriter::new_append(file).unwrap();
    archive.set_comment("Second edition");
    archive.finish().unwrap();

    let mut app = open(&f);
    app.open_path(&one).unwrap();
    let one_id = app.open_book_id().unwrap();
    app.activate_style(Command::Preset(EPUB_FOOTNOTES.into()))
        .unwrap();
    tap_contents_item(&mut app, "Use on all books");
    assert!(has_note_band(&app));
    app.open_path(&two).unwrap();
    let two_id = app.open_book_id().unwrap();
    assert_ne!(one_id, two_id);
    assert!(has_note_band(&app));
    app.activate_style(Command::Preset(EPUB_FOOTNOTES.into()))
        .unwrap();
    tap_contents_item(&mut app, "Off for this book");
    assert!(!has_note_band(&app));
    app.save().unwrap();

    let mut reopened = open(&f);
    reopened.open_path(&one).unwrap();
    assert!(has_note_band(&reopened));
    reopened.open_path(&two).unwrap();
    assert!(!has_note_band(&reopened));
    reopened
        .activate_style(Command::Preset(EPUB_FOOTNOTES.into()))
        .unwrap();
    tap_contents_item(&mut reopened, "Use default");
    assert!(has_note_band(&reopened));

    let non_epub = source_book(&f);
    reopened.open_path(&non_epub).unwrap();
    reopened.show(Screen::StyleTweaks);
    assert!(
        !reopened
            .items()
            .iter()
            .any(|item| item.title == "In-page EPUB footnotes")
    );
    assert!(!has_note_band(&reopened));
}
