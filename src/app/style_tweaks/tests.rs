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
fn image_style_menu_preserves_legacy_fit_and_applies_both_stock_choices() {
    use std::io::Write as _;

    let fixture = Fixture::new();
    let original = fs::File::open(crate::test_fixture("field-notes-epub3.epub")).unwrap();
    let mut archive = zip::ZipArchive::new(original).unwrap();
    let mut authored = zip::ZipWriter::new(fs::File::create(&fixture.book).unwrap());
    for index in 0..archive.len() {
        let entry = archive.by_index(index).unwrap();
        if entry.name() == "EPUB/chapters/shore.xhtml" {
            let options =
                zip::write::SimpleFileOptions::default().compression_method(entry.compression());
            authored.start_file(entry.name(), options).unwrap();
            authored
                .write_all(include_bytes!(
                    "../../../tests/fixtures/image-style-tweaks.xhtml"
                ))
                .unwrap();
        } else {
            authored.raw_copy_file(entry).unwrap();
        }
    }
    authored.finish().unwrap();
    let original_book = fs::read(&fixture.book).unwrap();

    let mut app = open(&fixture);
    app.open_path(&fixture.book).unwrap();
    let origin = app.current_history_location();
    let book = app.open_book_id().unwrap();
    let mut canvas = adopt(&mut app, ReadingViewport::PORTRAIT, 1);
    let before = capture(&mut app);
    app.open_reading_edge(true, 630.0);
    crate::app::tests::tap_reading_control(&mut app, "Settings");
    crate::app::tests::tap_reading_control(&mut app, "Style tweaks");
    assert_eq!(app.screen, Screen::StyleTweaks);
    tap_contents_item(&mut app, "Images");
    assert_eq!(app.screen, Screen::ImageStyles);
    tap_contents_item(&mut app, "Fit images to text width");
    tap_contents_item(&mut app, "On for this book");
    assert!(app.style_store.data.enabled(book, "image-text-width"));
    assert_ne!(capture(&mut app), before);

    let back = app.navigation_layout().back();
    tap(&mut app, back);
    assert_eq!(app.screen, Screen::ImageStyles);
    tap_contents_item(&mut app, "Full-width images");
    tap_contents_item(&mut app, "On for this book");
    assert!(!app.style_store.data.enabled(book, "image-text-width"));
    assert!(app.style_store.data.enabled(book, "image_full_width"));

    tap(&mut app, back);
    tap_contents_item(&mut app, "Vertically center-align images relative to text");
    tap_contents_item(&mut app, "Use on all books");
    assert!(app.style_store.data.enabled(book, "image_full_width"));
    assert!(app.style_store.data.enabled(book, "image_valign_middle"));
    assert_eq!(app.current_history_location(), origin);
    tap(&mut app, back);
    capture_viewport(&app, &mut canvas, "image-style-reader-menu.png", false);
    app.show(Screen::Book);
    capture_viewport(
        &app,
        &mut canvas,
        "image-style-reader-full-width.png",
        false,
    );
    app.save().unwrap();

    let mut reopened = open(&fixture);
    reopened.open_path(&fixture.book).unwrap();
    assert_eq!(reopened.current_history_location(), origin);
    assert!(reopened.style_store.data.enabled(book, "image_full_width"));
    assert!(
        reopened
            .style_store
            .data
            .enabled(book, "image_valign_middle")
    );
    assert_eq!(fs::read(&fixture.book).unwrap(), original_book);
}

#[test]
fn reading_style_preset_controls_preserve_page_and_global_defaults() {
    let f = Fixture::new();
    let path = crate::test_fixture("harbour-cjk-width.epub");
    fs::copy(path, &f.book).unwrap();
    let mut app = open(&f);
    app.open_path(&f.book).unwrap();
    let mut canvas = adopt(&mut app, ReadingViewport::PORTRAIT, 1);
    let anchor = app.current_history_location();
    let before = capture(&mut app);
    app.activate_style(Command::Preset("cjk_tailored".into()))
        .unwrap();
    tap_contents_item(&mut app, "Use on all books");
    assert!(
        app.active
            .as_ref()
            .unwrap()
            .reading
            .config
            .user_styles
            .layout_tweaks()
            .cjk_tailored
    );
    capture_viewport(&app, &mut canvas, "reader-style-grid-controls.png", false);
    assert!(capture(&mut app) != before);
    assert_eq!(app.current_history_location(), anchor);
    app.save().unwrap();
    let mut reopened = open(&f);
    reopened.open_path(&f.book).unwrap();
    assert!(
        reopened
            .active
            .as_ref()
            .unwrap()
            .reading
            .config
            .user_styles
            .layout_tweaks()
            .cjk_tailored
    );
    assert_eq!(reopened.current_history_location(), anchor);
    choose(&mut reopened, "cjk_tailored", Some(false));
    assert!(capture(&mut reopened) == before);
}

#[test]
fn reading_style_paragraph_spacing_menu_persists_one_effective_preset() {
    use user_styles::ParagraphSpacing;

    let f = Fixture::new();
    fs::copy(crate::test_fixture("field-notes-epub3.epub"), &f.book).unwrap();
    let mut app = open(&f);
    app.open_path(&f.book).unwrap();
    let mut canvas = adopt(&mut app, ReadingViewport::PORTRAIT, 1);
    let anchor = app.current_history_location();
    let spacing = |app: &Reader| {
        app.active
            .as_ref()
            .unwrap()
            .reading
            .config
            .user_styles
            .layout_tweaks()
            .paragraph_spacing
    };
    app.show(Screen::StyleTweaks);
    tap_contents_item(&mut app, "Spacing between paragraphs");
    assert_eq!(app.screen, Screen::ParagraphSpacing);
    capture_viewport(
        &app,
        &mut canvas,
        "reader-paragraph-spacing-controls.png",
        false,
    );
    tap_contents_item(&mut app, "Spacing between paragraphs (half)");
    tap_contents_item(&mut app, "Use on all books");
    assert_eq!(spacing(&app), ParagraphSpacing::Half);
    assert_eq!(app.style_preset_parent(), Screen::ParagraphSpacing);
    app.show(app.style_preset_parent());
    tap_contents_item(&mut app, "No spacing between paragraphs");
    tap_contents_item(&mut app, "On for this book");
    assert_eq!(spacing(&app), ParagraphSpacing::None);
    assert_eq!(app.current_history_location(), anchor);
    app.save().unwrap();
    let mut reopened = open(&f);
    reopened.open_path(&f.book).unwrap();
    assert_eq!(spacing(&reopened), ParagraphSpacing::None);
    assert_eq!(reopened.current_history_location(), anchor);
    choose(&mut reopened, "paragraph_no_whitespace", None);
    choose(&mut reopened, "paragraph_whitespace_half", None);
    assert_eq!(spacing(&reopened), ParagraphSpacing::Half);
}

#[test]
fn reading_style_link_menu_saves_independent_pairs_and_the_open_passage() {
    use user_styles::LinkInk;

    let f = Fixture::new();
    let path = f.root.join("Link styles.xhtml");
    fs::copy(crate::test_fixture("reading-link-styles.xhtml"), &path).unwrap();
    let mut app = open(&f);
    app.open_path(&path).unwrap();
    let mut canvas = adopt(&mut app, ReadingViewport::PORTRAIT, 1);
    let anchor = app.current_history_location();
    let id = app.active.as_ref().unwrap().id;
    let ink = |app: &Reader| {
        app.active
            .as_ref()
            .unwrap()
            .reading
            .config
            .user_styles
            .layout_tweaks()
            .link_ink
    };
    app.show(Screen::StyleTweaks);
    tap_contents_item(&mut app, "Links");
    assert_eq!(app.screen, Screen::LinkStyles);
    assert_eq!(app.style_items(Screen::LinkStyles).len(), 8);
    tap_contents_item(&mut app, "Links always blue");
    tap_contents_item(&mut app, "Use on all books");
    assert_eq!(ink(&app), Some(LinkInk::Blue));
    assert_eq!(app.style_preset_parent(), Screen::LinkStyles);
    app.show(Screen::LinkStyles);
    capture_viewport(&app, &mut canvas, "reader-link-styles-controls.png", false);
    tap_contents_item(&mut app, "Links always black");
    tap_contents_item(&mut app, "On for this book");
    assert_eq!(ink(&app), Some(LinkInk::Black));
    app.show(Screen::LinkStyles);
    tap_contents_item(&mut app, "Links always bold");
    tap_contents_item(&mut app, "On for this book");
    assert_eq!(ink(&app), Some(LinkInk::Black));
    assert!(app.style_store.data.enabled(id, "link-bold"));
    assert!(!app.style_store.data.enabled(id, "link-blue"));
    assert!(app.style_store.data.global.contains("link-blue"));
    assert_eq!(app.current_history_location(), anchor);
    app.save().unwrap();

    let mut reopened = open(&f);
    reopened.open_path(&path).unwrap();
    assert_eq!(ink(&reopened), Some(LinkInk::Black));
    assert!(reopened.style_store.data.enabled(id, "link-bold"));
    assert_eq!(reopened.current_history_location(), anchor);
}

#[test]
fn table_presentational_hints_menu_keeps_the_reading_place_and_saved_choices() {
    let f = Fixture::new();
    let path = f.root.join("Harbor table presentation.xhtml");
    fs::copy(
        crate::test_fixture("table-presentational-hints.xhtml"),
        &path,
    )
    .unwrap();
    let mut app = open(&f);
    app.open_path(&path).unwrap();
    let mut canvas = adopt(&mut app, ReadingViewport::PORTRAIT, 1);
    let anchor = app.current_history_location();
    let preset = "table_no_presentational_hints";
    app.show(Screen::StyleTweaks);
    tap_contents_item(&mut app, "Tables");
    assert_eq!(app.screen, Screen::TableStyles);
    tap_contents_item(&mut app, "Ignore HTML table presentation");
    tap_contents_item(&mut app, "Use on all books");
    assert_eq!(app.style_preset_parent(), Screen::TableStyles);
    app.show(Screen::TableStyles);
    capture_viewport(&app, &mut canvas, "reader-table-style-controls.png", false);
    let id = app.active.as_ref().unwrap().id;
    assert!(app.style_store.data.enabled(id, preset));
    assert_eq!(app.current_history_location(), anchor);
    app.save().unwrap();
    let mut reopened = open(&f);
    reopened.open_path(&path).unwrap();
    assert!(reopened.style_store.data.enabled(id, preset));
    assert_eq!(reopened.current_history_location(), anchor);
}

#[test]
fn table_vertical_alignment_menu_applies_to_the_open_book_and_reopens_at_its_source() {
    let f = Fixture::new();
    let path = f.root.join("Aligned table.xhtml");
    fs::write(
        &path,
        r##"<html><body><p>Survey readings.</p><table>
        <tr><td><p>Wind.</p><p>Current.</p><p>Tide.</p></td>
        <td id="linked" style="vertical-align:bottom"><a href="#end">Harbor light</a></td>
        </tr></table><p id="end">Reading complete.</p></body></html>"##,
    )
    .unwrap();
    let mut app = open(&f);
    app.open_path(&path).unwrap();
    let anchor = app.current_history_location();
    let link_rect = |app: &Reader| {
        let probe = &app.active.as_ref().unwrap().reading.probe;
        let first = probe.fragment("linked").unwrap();
        let end = crate::Anchor {
            offset: first.offset + "Harbor light".len(),
            ..first.clone()
        };
        let selected = probe.select_range(&first, &end).unwrap();
        let page = probe.page_for(&first).unwrap();
        let rect = probe.selection_rects(page, &selected)[0];
        assert!(
            probe
                .link_at(page, rect.x as f32 + 3., rect.y as f32 + 5.)
                .is_some()
        );
        rect
    };
    let before = link_rect(&app);
    app.show(Screen::StyleTweaks);
    tap_contents_item(&mut app, "Tables");
    tap_contents_item(&mut app, "Ignore publisher vertical alignment in tables");
    tap_contents_item(&mut app, "On for this book");
    assert_eq!(app.style_preset_parent(), Screen::TableStyles);
    let after = link_rect(&app);
    assert!(
        after.y < before.y,
        "Publisher bottom alignment should move to the baseline"
    );
    assert_eq!(app.current_history_location(), anchor);
    app.save().unwrap();
    let mut reopened = open(&f);
    reopened.open_path(&path).unwrap();
    assert_eq!(link_rect(&reopened), after);
    assert_eq!(reopened.current_history_location(), anchor);
}

#[test]
fn table_caption_hint_menu_moves_the_linked_caption_and_keeps_its_source() {
    let f = Fixture::new();
    let path = f.root.join("Table captions.xhtml");
    fs::write(
        &path,
        r##"<html><body><p>Harbor survey.</p><table style="width:75%">
        <tr><td id="reading">Current</td><td>Two knots</td></tr>
        <caption align="bottom"><a id="legend" href="#destination">North pier</a></caption>
        </table><p id="destination">Measurements and notes.</p></body></html>"##,
    )
    .unwrap();
    let mut app = open(&f);
    app.open_path(&path).unwrap();
    let origin = app.current_history_location();
    let geometry = |app: &Reader| {
        let probe = &app.active.as_ref().unwrap().reading.probe;
        let at = |id: &str, text: &str| {
            let first = probe.fragment(id).unwrap();
            let end = crate::Anchor {
                offset: first.offset + text.len(),
                ..first.clone()
            };
            let selected = probe.select_range(&first, &end).unwrap();
            assert_eq!(selected.text, text);
            let page = probe.page_for(&first).unwrap();
            (first, page, probe.selection_rects(page, &selected)[0])
        };
        let (caption, page, caption_rect) = at("legend", "North pier");
        let (_, table_page, cell_rect) = at("reading", "Current");
        assert_eq!(page, table_page);
        let target = probe
            .link_at(page, caption_rect.x as f32 + 3., caption_rect.y as f32 + 5.)
            .expect("the caption still links to the notes");
        (caption, target, caption_rect.y - cell_rect.y)
    };
    let (anchor, target, before) = geometry(&app);
    assert!(before > 0, "publisher caption below the table");
    app.show(Screen::StyleTweaks);
    tap_contents_item(&mut app, "Tables");
    tap_contents_item(&mut app, "Ignore HTML table presentation");
    tap_contents_item(&mut app, "On for this book");
    let (updated_anchor, updated_target, after) = geometry(&app);
    assert_eq!(updated_anchor, anchor);
    assert_eq!(updated_target, target);
    assert!(after < 0, "caption above the table after ignoring HTML");
    app.save().unwrap();

    let mut reopened = open(&f);
    reopened.open_path(&path).unwrap();
    assert_eq!(geometry(&reopened), (anchor, target, after));
    assert_eq!(reopened.current_history_location(), origin);
}

#[test]
fn pseudo_generated_menu_keeps_the_reading_place_and_source_selection() {
    let f = Fixture::new();
    let path = f.root.join("Generated book text.xhtml");
    fs::write(
        &path,
        r##"<html><head><style>
        .labeled::before { content: "Notice: "; }
        .labeled::after { content: " [end]"; }
        .labeled::first-letter { font-size:160%; font-weight:bold; }
        .labeled::first-line { font-size:135%; }
        </style></head><body><p class="labeled">
        <a href="#destination" id="passage">Harbor lights</a></p>
        <p id="destination">Boats head home.</p></body></html>"##,
    )
    .unwrap();
    let mut app = open(&f);
    app.open_path(&path).unwrap();
    let mut canvas = adopt(&mut app, ReadingViewport::PORTRAIT, 1);
    let origin = app.current_history_location();
    let before = capture(&mut app);
    let selection = |app: &Reader| {
        let probe = &app.active.as_ref().unwrap().reading.probe;
        let first = probe.fragment("passage").unwrap();
        let end = crate::Anchor {
            offset: first.offset + "Harbor lights".len(),
            ..first.clone()
        };
        let selected = probe.select_range(&first, &end).unwrap();
        assert_eq!(selected.text, "Harbor lights");
        let page = probe.page_for(&first).unwrap();
        let rect = probe.selection_rects(page, &selected)[0];
        let target = probe
            .link_at(page, rect.x as f32 + 3., rect.y as f32 + 5.)
            .expect("the original link remains active");
        (first, target)
    };
    let passage = selection(&app);
    app.open_reading_edge(true, 630.0);
    crate::app::tests::tap_reading_control(&mut app, "Settings");
    crate::app::tests::tap_reading_control(&mut app, "Style tweaks");
    tap_contents_item(&mut app, "Pseudo-elements");
    assert_eq!(app.screen, Screen::PseudoElementStyles);
    tap_contents_item(&mut app, "Disable before/after pseudo elements");
    tap_contents_item(&mut app, "On for this book");
    assert_eq!(app.style_preset_parent(), Screen::PseudoElementStyles);
    let after = capture(&mut app);
    assert_ne!(before, after);
    assert_eq!(selection(&app), passage);
    assert_eq!(app.current_history_location(), origin);
    let back = app.navigation_layout().back();
    tap(&mut app, back);
    assert_eq!(app.screen, Screen::PseudoElementStyles);
    tap_contents_item(&mut app, "Disable first-letter pseudo elements");
    tap_contents_item(&mut app, "On for this book");
    let plain_initial = capture(&mut app);
    assert_ne!(plain_initial, after);
    assert_ne!(plain_initial, before);
    assert_eq!(selection(&app), passage);
    assert_eq!(app.current_history_location(), origin);
    tap(&mut app, back);
    tap_contents_item(&mut app, "Disable first-line pseudo elements");
    tap_contents_item(&mut app, "Use on all books");
    let plain = capture(&mut app);
    assert_ne!(plain, plain_initial);
    assert_eq!(selection(&app), passage);
    assert_eq!(app.current_history_location(), origin);
    let book = app.open_book_id().unwrap();
    for preset in [
        "no_pseudo_element_before_after",
        "no_pseudo_element_first_letter",
        "no_pseudo_element_first_line",
    ] {
        assert!(app.style_store.data.enabled(book, preset));
    }
    tap(&mut app, back);
    assert_eq!(app.screen, Screen::PseudoElementStyles);
    capture_viewport(
        &app,
        &mut canvas,
        "reader-pseudo-element-controls.png",
        false,
    );
    app.save().unwrap();

    let mut reopened = open(&f);
    reopened.open_path(&path).unwrap();
    adopt(&mut reopened, ReadingViewport::PORTRAIT, 1);
    assert_eq!(selection(&reopened), passage);
    assert_eq!(reopened.current_history_location(), origin);
    assert_eq!(capture(&mut reopened), plain);
}

#[test]
fn list_marker_preferences_apply_independently_and_restore_the_reading_place() {
    let f = Fixture::new();
    let path = f.root.join("Harbor watch lists.xhtml");
    fs::copy(crate::test_fixture("list-marker-presets.xhtml"), &path).unwrap();
    let mut app = open(&f);
    app.open_path(&path).unwrap();
    let mut canvas = adopt(&mut app, ReadingViewport::PORTRAIT, 1);
    let body = capture(&mut app);
    let origin = app.current_history_location();
    let book = app.open_book_id().unwrap();
    app.show(Screen::StyleTweaks);
    tap_contents_item(&mut app, "List items");
    assert_eq!(app.screen, Screen::ListItemStyles);
    tap_contents_item(&mut app, "Force bullets with unordered lists");
    tap_contents_item(&mut app, "Use on all books");
    let bullets = capture(&mut app);
    assert_ne!(bullets, body);
    let back = app.navigation_layout().back();
    tap(&mut app, back);
    assert_eq!(app.screen, Screen::ListItemStyles);
    tap_contents_item(&mut app, "Force decimal numbers with ordered lists");
    tap_contents_item(&mut app, "On for this book");
    let combined = capture(&mut app);
    assert_ne!(combined, bullets);
    assert!(app.style_store.data.enabled(book, "ul_li_type_disc"));
    assert!(app.style_store.data.enabled(book, "ol_li_type_decimal"));
    assert_eq!(app.current_history_location(), origin);
    tap(&mut app, back);
    assert_eq!(app.screen, Screen::ListItemStyles);
    capture_viewport(&app, &mut canvas, "reader-list-item-controls.png", false);
    app.save().unwrap();

    let mut reopened = open(&f);
    reopened.open_path(&path).unwrap();
    adopt(&mut reopened, ReadingViewport::PORTRAIT, 1);
    assert_eq!(reopened.current_history_location(), origin);
    assert_eq!(capture(&mut reopened), combined);
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
fn advanced_note_options_reflow_from_controls_keep_position_and_persist() {
    use crate::user_styles::{NOTE_COSMETIC_SPACING, WIKIPEDIA_FOOTNOTES};
    let fixture = Fixture::new();
    let path = footnote_book(&fixture, "Wikipedia references.epub");
    let original = fs::read(&path).unwrap();
    let mut app = open(&fixture);
    app.open_path(&path).unwrap();
    app.follow_reflow_page(10, 0).unwrap();
    let position = app.current_history_location();
    let history = app.back.clone();
    let id = app.active.as_ref().unwrap().id;
    assert!(!has_note_band(&app));
    app.show(Screen::StyleTweaks);
    tap_contents_item(&mut app, "More footnote options");
    for label in [
        "In-page Wikipedia footnotes",
        "Remove cosmetic footnote spacing",
    ] {
        tap_contents_item(&mut app, label);
        tap_contents_item(&mut app, "On for this book");
        assert!(has_note_band(&app));
        let back = app.navigation_layout().back();
        tap(&mut app, back);
        assert_eq!(app.screen, Screen::FootnoteTweaks);
    }
    assert_eq!(app.current_history_location(), position);
    assert_eq!(app.back, history);
    let mut canvas = crate::app::selection_viewport_tests::canvas(&app, ReadingViewport::PORTRAIT);
    capture_viewport(&app, &mut canvas, "advanced-note-options.png", false);
    app.show(Screen::Book);
    app.save().unwrap();
    drop(app);

    let mut app = open(&fixture);
    app.open_path(&path).unwrap();
    assert!(has_note_band(&app));
    assert_eq!(app.current_history_location(), position);
    for preset in [WIKIPEDIA_FOOTNOTES, NOTE_COSMETIC_SPACING] {
        assert!(app.style_store.data.enabled(id, preset));
    }
    assert_eq!(fs::read(&path).unwrap(), original);
}

#[test]
fn whole_note_inline_reflows_from_the_visible_setting_and_reopens() {
    use crate::user_styles::INLINE_ALL_FOOTNOTE_CONTENT;
    let fixture = Fixture::new();
    let path = footnote_book(&fixture, "Joined footnotes.epub");
    let original = fs::read(&path).unwrap();
    let mut app = open(&fixture);
    app.open_path(&path).unwrap();
    app.follow_reflow_page(12, 0).unwrap();
    let position = app.current_history_location();
    let history = app.back.clone();
    let id = app.active.as_ref().unwrap().id;
    let is_joined = |app: &Reader| {
        let probe = &app.active.as_ref().unwrap().reading.probe;
        matches!(&probe.renderer, crate::Renderer::Parley(renderer) if renderer.uses_joined_notes())
    };
    app.show(Screen::StyleTweaks);
    tap_contents_item(&mut app, "In-page EPUB footnotes");
    tap_contents_item(&mut app, "On for this book");
    assert!(has_note_band(&app));
    assert!(!is_joined(&app));
    app.show(Screen::StyleTweaks);
    tap_contents_item(&mut app, "More footnote options");
    tap_contents_item(&mut app, "Inline all footnote content");
    tap_contents_item(&mut app, "On for this book");
    assert!(is_joined(&app));
    assert_eq!(app.current_history_location(), position);
    assert_eq!(app.back, history);
    let mut canvas = crate::app::selection_viewport_tests::canvas(&app, ReadingViewport::PORTRAIT);
    capture_viewport(&app, &mut canvas, "whole-note-inline-settings.png", false);
    app.show(Screen::Book);
    app.save().unwrap();
    drop(app);

    let mut app = open(&fixture);
    app.open_path(&path).unwrap();
    assert_eq!(app.current_history_location(), position);
    assert!(is_joined(&app));
    assert!(
        app.style_store
            .data
            .enabled(id, INLINE_ALL_FOOTNOTE_CONTENT)
    );
    assert_eq!(fs::read(path).unwrap(), original);
}

#[test]
fn classic_in_page_notes_apply_and_resume_from_the_visible_style_options() {
    let fixture = Fixture::new();
    let path = footnote_book(&fixture, "Classic notes.epub");
    let original = fs::read(&path).unwrap();
    let mut app = open(&fixture);
    app.open_path(&path).unwrap();
    app.follow_reflow_page(8, 0).unwrap();
    let position = app.current_history_location();
    let history = app.back.clone();
    assert!(!has_note_band(&app));
    app.show(Screen::StyleTweaks);
    tap_contents_item(&mut app, "More footnote options");
    assert_eq!(app.screen, Screen::FootnoteTweaks);
    for label in [
        "In-page classic footnotes",
        "Extend notes until next heading",
        "No footnote indentation",
        "Regularize footnote text",
    ] {
        tap_contents_item(&mut app, label);
        tap_contents_item(&mut app, "On for this book");
        assert!(has_note_band(&app));
        let back = app.navigation_layout().back();
        tap(&mut app, back);
        assert_eq!(app.screen, Screen::FootnoteTweaks);
    }
    assert_eq!(app.current_history_location(), position);
    assert_eq!(app.back, history);
    assert!(
        app.active
            .as_ref()
            .unwrap()
            .reading
            .visible_text()
            .contains("Continuation before the heading.")
    );
    assert!(
        !app.active
            .as_ref()
            .unwrap()
            .reading
            .visible_text()
            .contains("Continuation after the heading.")
    );
    let mut canvas = crate::app::selection_viewport_tests::canvas(&app, ReadingViewport::PORTRAIT);
    capture_viewport(&app, &mut canvas, "classic-footnote-options.png", false);
    app.show(Screen::Book);
    app.save().unwrap();
    drop(app);

    let mut app = open(&fixture);
    app.open_path(&path).unwrap();
    assert!(has_note_band(&app));
    assert_eq!(app.current_history_location(), position);
    assert_eq!(fs::read(&path).unwrap(), original);
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
