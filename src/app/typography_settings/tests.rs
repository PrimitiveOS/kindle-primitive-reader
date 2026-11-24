use super::*;
use crate::app::selection_viewport_tests::{adopt, capture};
use crate::app::tests::{Fixture, tap_contents_item};
use crate::app::{App, Input, TopGroup};
use crate::session::ReadingViewport;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

fn write_book(path: &Path, title: &str, language: &str) {
    let mut zip = zip::ZipWriter::new(File::create(path).unwrap());
    let options = zip::write::SimpleFileOptions::default();
    let container = br#"<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container" version="1.0"><rootfiles><rootfile full-path="book.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#;
    let metadata_language = if language.is_empty() {
        String::new()
    } else {
        format!("<dc:language>{language}</dc:language>")
    };
    let package = format!(
        r#"<package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="id"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>{title}</dc:title><dc:identifier id="id">urn:reader:typography:{title}</dc:identifier>{metadata_language}</metadata><manifest><item id="chapter" href="book.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="chapter"/></spine></package>"#
    );
    let paragraphs = r#"<p>laboratory <span xml:lang="vi">mặt trời café &#128;</span> laboratory <span lang="en-GB" xml:lang="en-GB">laboratory</span> garden. First light passed through the open window.</p>"#.repeat(75);
    let chapter = format!(
        r#"<html xmlns="http://www.w3.org/1999/xhtml"><body><h1>{title}</h1>{paragraphs}</body></html>"#
    );
    for (name, bytes) in [
        ("mimetype", b"application/epub+zip".as_slice()),
        ("META-INF/container.xml", container.as_slice()),
        ("book.opf", package.as_bytes()),
        ("book.xhtml", chapter.as_bytes()),
    ] {
        zip.start_file(name, options).unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap();
}

fn reader(fixture: &Fixture) -> Reader {
    Reader::open(
        &fixture.root,
        &fixture.state,
        vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")],
    )
    .unwrap()
}

fn probe(app: &Reader) -> &crate::Probe {
    &app.active.as_ref().unwrap().reading.probe
}

fn main_language(app: &Reader) -> &str {
    probe(app).main_language.as_ref().unwrap().as_str()
}

fn catalog(tag: &str) -> Option<CatalogLanguage> {
    Some(CatalogLanguage::try_from(tag.to_owned()).unwrap())
}

#[test]
fn all_catalog_entries_keep_their_distinct_saved_tag() {
    for entry in &CATALOG {
        let choice = catalog(entry.tag).unwrap();
        let encoded = serde_json::to_string(&DefaultLanguage::Default(choice)).unwrap();
        assert_eq!(
            serde_json::from_str::<DefaultLanguage>(&encoded).unwrap(),
            DefaultLanguage::Default(choice)
        );
        assert_eq!(choice.locale().as_str(), entry.tag);
    }
    assert_eq!(
        TypographyLocale::parse("lat-lit")
            .unwrap()
            .shaping_language()
            .as_str(),
        "la"
    );
}

#[test]
fn typography_language_reflows_real_spans_and_preserves_the_source_passage() {
    let fixture = Fixture::new();
    let path = fixture.root.join("Language workshop.epub");
    write_book(&path, "Language workshop", "eng");
    if let Some(output) = std::env::var_os("READER_TYPOGRAPHY_LANGUAGE_FIXTURE") {
        fs::copy(&path, output).unwrap();
    }
    let original = fs::read(&path).unwrap();
    let mut app = reader(&fixture);
    app.open_path(&path).unwrap();
    app.follow_reflow_page(0, 2).unwrap();
    assert_eq!(main_language(&app), "en-US");
    let original_location = app.current_history_location();
    let revision = probe(&app).source.revision;
    let before = probe(&app).source.blocks[1].text.clone();
    assert!(before.contains('€'));

    let embedded = |app: &Reader| {
        probe(app).source.blocks[1]
            .styled_text
            .iter()
            .any(|span| span.language.is_some_and(|lang| lang.as_str() == "vi"))
    };
    assert!(embedded(&app));
    let break_points = |app: &Reader| {
        let probe = probe(app);
        let block = &probe.source.blocks[1];
        crate::hyphenation::opportunities(
            block,
            &vec![crate::book_style::Style::default(); probe.source.elements.len()],
            probe.settings,
        )
        .into_iter()
        .take_while(|offset| *offset < "laboratory".len())
        .collect::<Vec<_>>()
    };
    assert_eq!(break_points(&app), [3, 4, 6]);
    let mut portrait = adopt(&mut app, ReadingViewport::PORTRAIT, 1);
    let typography_entry = app
        .top_config_items(TopGroup::Appearance)
        .into_iter()
        .find(|item| item.title == "Typography rules")
        .unwrap();
    app.activate(typography_entry.action).unwrap();
    assert_eq!(app.screen, Screen::TypographyRules);
    capture(&app, &mut portrait, "typography-language-rules.png", false);
    tap_contents_item(&mut app, "Main language");
    tap_contents_item(&mut app, "English (UK)");
    assert_eq!(main_language(&app), "en-GB");
    assert_eq!(break_points(&app), [5, 7]);
    assert!(embedded(&app));
    tap_contents_item(&mut app, "Respect embedded language tags");
    assert!(!embedded(&app));
    assert_eq!(probe(&app).source.revision, revision);
    assert_eq!(probe(&app).source.blocks[1].text, before);
    assert_eq!(app.current_history_location(), original_location);

    tap_contents_item(&mut app, "Main language");
    let mut landscape = adopt(&mut app, ReadingViewport::LANDSCAPE, 2);
    capture(
        &app,
        &mut landscape,
        "typography-language-choices-landscape.png",
        false,
    );
    let first = app.list_window().1[0].title.clone();
    assert_eq!(app.input(Input::PageForward), crate::app::Change::Partial);
    assert_ne!(app.list_window().1[0].title, first);
    assert_eq!(app.input(Input::PageBack), crate::app::Change::Partial);
    assert_eq!(app.list_window().1[0].title, first);

    app.save().unwrap();
    let mut reopened = reader(&fixture);
    reopened.open_path(&path).unwrap();
    assert_eq!(main_language(&reopened), "en-GB");
    assert!(!embedded(&reopened));
    assert_eq!(reopened.current_history_location(), original_location);
    assert_eq!(fs::read(&path).unwrap(), original);
}

#[test]
fn typography_language_default_and_fallback_are_independent_of_saved_books() {
    let fixture = Fixture::new();
    let mut app = reader(&fixture);
    let book = |name, tag| {
        let path = fixture.root.join(format!("{name}.epub"));
        write_book(&path, name, tag);
        path
    };
    let original = book("Existing", "vi");
    let tagged = book("Tagged", "en-GB");
    let untagged = book("Untagged", "");
    let forced = book("Forced", "en-GB");
    app.open_path(&original).unwrap();
    assert_eq!(main_language(&app), "vi");
    app.choose_typography_language(LanguageScope::Fallback, catalog("fr"))
        .unwrap();
    app.open_path(&tagged).unwrap();
    assert_eq!(main_language(&app), "en-GB");
    app.open_path(&untagged).unwrap();
    assert_eq!(main_language(&app), "fr");
    assert!(probe(&app).publication_language.is_none());

    app.choose_typography_language(LanguageScope::Default, catalog("en-US"))
        .unwrap();
    tap_contents_item(&mut app, "Respect embedded tags by default");
    app.open_path(&forced).unwrap();
    assert_eq!(main_language(&app), "en-US");
    assert!(!probe(&app).settings.typography.respect_embedded);
    app.choose_typography_language(LanguageScope::Book, None)
        .unwrap();
    assert_eq!(main_language(&app), "en-GB");
    app.activate(crate::app::Action::DocumentDefaults(
        crate::app::default_settings::Command::SaveReflow,
    ))
    .unwrap();
    app.open_path(&book("New after template", "fr")).unwrap();
    assert_eq!(main_language(&app), "en-US");
    assert!(!probe(&app).settings.typography.respect_embedded);
    app.open_path(&original).unwrap();
    assert_eq!(main_language(&app), "vi");
    assert!(probe(&app).settings.typography.respect_embedded);
    app.open_path(&untagged).unwrap();
    assert_eq!(main_language(&app), "fr");

    app.save().unwrap();
    let mut reopened = reader(&fixture);
    reopened.open_path(&book("After restart", "fr")).unwrap();
    assert_eq!(main_language(&reopened), "en-US");
    assert!(!probe(&reopened).settings.typography.respect_embedded);
    reopened
        .choose_typography_language(LanguageScope::Fallback, None)
        .unwrap();
    assert_eq!(
        reopened
            .default_typography_languages()
            .main
            .unwrap()
            .as_str(),
        "en-US"
    );
    reopened
        .choose_typography_language(LanguageScope::Default, None)
        .unwrap();
    reopened.open_path(&book("After clearing", "")).unwrap();
    assert_eq!(main_language(&reopened), "en-US");
}

#[test]
fn hanging_punctuation_preserves_reading_position_and_has_a_new_book_default() {
    let fixture = Fixture::new();
    let first = fixture.root.join("Optical margins.epub");
    let second = fixture.root.join("Next edition.epub");
    fs::copy(crate::test_fixture("hanging-punctuation.epub"), &first).unwrap();
    write_book(&second, "Next edition", "en-US");
    let bytes = fs::read(&first).unwrap();

    let mut app = reader(&fixture);
    app.open_path(&first).unwrap();
    let anchor = app.current_history_location();
    let id = app.active.as_ref().unwrap().id;
    let before = app
        .active
        .as_mut()
        .unwrap()
        .reading
        .probe
        .capture(0)
        .unwrap();
    app.show(Screen::TypographyRules);
    assert!(!probe(&app).settings.hanging_punctuation);
    tap_contents_item(&mut app, "Hanging punctuation");
    assert!(probe(&app).settings.hanging_punctuation);
    assert!(
        app.library
            .state(id)
            .unwrap()
            .preferences
            .hanging_punctuation
            .unwrap()
    );
    assert_eq!(app.current_history_location(), anchor);
    assert_ne!(
        app.active
            .as_mut()
            .unwrap()
            .reading
            .probe
            .capture(0)
            .unwrap(),
        before
    );
    let mut canvas = adopt(&mut app, ReadingViewport::LANDSCAPE, 1);
    capture(
        &app,
        &mut canvas,
        "hanging-punctuation-controls-landscape.png",
        false,
    );
    tap_contents_item(&mut app, "Hanging punctuation");
    assert_eq!(app.current_history_location(), anchor);
    assert!(!probe(&app).settings.hanging_punctuation);
    tap_contents_item(&mut app, "Hanging punctuation by default");
    assert!(!probe(&app).settings.hanging_punctuation);
    app.open_path(&second).unwrap();
    assert!(probe(&app).settings.hanging_punctuation);
    app.save().unwrap();
    let mut reopened = reader(&fixture);
    reopened.open_path(&first).unwrap();
    assert!(!probe(&reopened).settings.hanging_punctuation);
    assert_eq!(reopened.current_history_location(), anchor);
    assert_eq!(fs::read(first).unwrap(), bytes);
}
