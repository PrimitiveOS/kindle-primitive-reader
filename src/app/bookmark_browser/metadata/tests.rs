use super::*;
use crate::app::tests::{Fixture, capture_test_screen};
use crate::app::{Action, DONE};
use lopdf::dictionary;
use primitive_app::App;
use primitive_ui::{Input, Rect};
use std::fs;
use std::io::{Cursor, Read, Write};

fn epub(properties: &str) -> Vec<u8> {
    let bytes = include_bytes!("../../../../tests/fixtures/field-notes-epub3.epub");
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut output = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).unwrap();
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).unwrap();
        if entry.name().ends_with(".opf") {
            let xml = String::from_utf8(bytes)
                .unwrap()
                .replace("<dc:language>en</dc:language>", "")
                .replace("</metadata>", &format!("{properties}</metadata>"))
                .replace(
                    "id=\"resource-4\"",
                    "id=\"resource-4\" properties=\"cover-image\"",
                );
            bytes = xml.into_bytes();
        }
        output
            .start_file(
                entry.name(),
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Stored),
            )
            .unwrap();
        output.write_all(&bytes).unwrap();
    }
    output.finish().unwrap().into_inner()
}

fn pdf() -> Vec<u8> {
    let mut document = lopdf::Document::load_mem(&crate::fixed::tests::document()).unwrap();
    let subject = [
        vec![0xfe, 0xff],
        "A Unicode sơ đồ of the harbor"
            .encode_utf16()
            .flat_map(u16::to_be_bytes)
            .collect(),
    ]
    .concat();
    let info = document.add_object(lopdf::dictionary! {
        "Title" => lopdf::Object::string_literal("Survey sheets"),
        "Subject" => lopdf::Object::String(subject, lopdf::StringFormat::Hexadecimal),
        "Keywords" => lopdf::Object::string_literal("Charts\nNavigation"),
    });
    document.trailer.set("Info", info);
    let root = document
        .trailer
        .get(b"Root")
        .unwrap()
        .as_reference()
        .unwrap();
    document
        .get_object_mut(root)
        .unwrap()
        .as_dict_mut()
        .unwrap()
        .set("Lang", lopdf::Object::string_literal("VI"));
    let mut output = Vec::new();
    document.save_to(&mut output).unwrap();
    output
}

fn package(parts: &[(&str, &str)]) -> Vec<u8> {
    let mut output = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, text) in parts {
        output
            .start_file(
                *name,
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Stored),
            )
            .unwrap();
        output.write_all(text.as_bytes()).unwrap();
    }
    output.finish().unwrap().into_inner()
}

fn tap(app: &mut Reader, rect: Rect) {
    assert_eq!(
        app.input(Input::Tap((rect.x + 40) as f32, (rect.y + 40) as f32)),
        Change::Partial
    );
}

fn choose(app: &mut Reader, title: &str) {
    let (_, rows) = app.list_window();
    let index = rows
        .iter()
        .position(|item| item.title == title)
        .unwrap_or_else(|| panic!("Missing {title} on {:?}", app.screen));
    let y = crate::app::LIST_Y + index as i32 * crate::app::ROW_HEIGHT;
    assert_eq!(
        app.input(Input::Tap(120.0, (y + 35) as f32)),
        Change::Partial
    );
}

fn settle(app: &mut Reader) {
    for _ in 0..100 {
        let Some(deadline) = app.bookmark_metadata_deadline() else {
            return;
        };
        app.tick(deadline);
    }
    panic!("Metadata did not settle");
}

#[test]
fn metadata_filters_replace_the_selection_and_cache_descriptions_for_archived_books() {
    let fixture = Fixture::new();
    let first = fixture.root.join("Harbor journal.epub");
    let second = fixture.root.join("Coastal journal.epub");
    let sheet = fixture.root.join("Survey.pdf");
    let plain = fixture.root.join("Checklist.txt");
    fs::write(&first, epub(r#"<dc:language>EN-us</dc:language><dc:subject>Navigation</dc:subject><dc:subject>Tides</dc:subject><dc:description>&lt;p&gt;A harbor &amp;amp; coast.&lt;/p&gt;&lt;p&gt;Keep the shoreline in sight.&lt;/p&gt;</dc:description><meta name="calibre:series" content="Coastal journals"/>"#)).unwrap();
    fs::write(&second, epub(r#"<dc:language>fr</dc:language><dc:subject>Charts</dc:subject><dc:subject>Navigation</dc:subject><meta name="calibre:series" content="Coastal journals"/>"#)).unwrap();
    fs::write(&sheet, pdf()).unwrap();
    fs::write(&plain, "A plain reading checklist.").unwrap();
    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
    let mut ids = Vec::new();
    for path in [&first, &second, &sheet, &plain] {
        app.open_path(path).unwrap();
        app.activate(Action::AddBookmark).unwrap();
        ids.push(app.open_book_id().unwrap());
    }
    app.save().unwrap();
    let saved = fs::read(&fixture.state).unwrap();
    let place = app.current_history_location().unwrap();
    app.activate_bookmark_browser(Command::Open).unwrap();
    choose(&mut app, "History");
    assert_eq!(app.bookmark_browser.catalog.pending.len(), 4);
    app.activate_bookmark_browser(Command::RestrictBooks)
        .unwrap();
    choose(&mut app, "Select by series");
    assert_eq!(app.list_window().1[0].title, "Reading book metadata");
    settle(&mut app);
    let catalog = &app.bookmark_browser.catalog;
    assert_eq!(catalog.get(ids[0]).unwrap().series, ["Coastal journals"]);
    assert_eq!(
        catalog.get(ids[0]).unwrap().language.as_deref(),
        Some("en-us")
    );
    assert_eq!(
        catalog.get(ids[0]).unwrap().description.as_deref(),
        Some("A harbor & coast.\n\nKeep the shoreline in sight.")
    );
    assert_eq!(
        catalog.get(ids[2]).unwrap().description.as_deref(),
        Some("A Unicode sơ đồ of the harbor")
    );
    assert_eq!(
        catalog.get(ids[2]).unwrap().keywords,
        ["Charts", "Navigation"]
    );
    assert_eq!(catalog.get(ids[2]).unwrap().language.as_deref(), Some("vi"));
    assert!(catalog.get(ids[3]).unwrap().language.is_none());
    choose(&mut app, "Coastal journals");
    assert_eq!(
        app.bookmark_browser.picking.as_ref().unwrap(),
        &BTreeSet::from([ids[0], ids[1]])
    );
    choose(&mut app, "Select by keyword");
    choose(&mut app, "Navigation");
    assert_eq!(
        app.bookmark_browser.picking.as_ref().unwrap(),
        &BTreeSet::from([ids[0], ids[1], ids[2]])
    );
    choose(&mut app, "Select by language");
    capture_test_screen(&app, &fonts, "READER_BOOKMARK_LANGUAGES_SCREENSHOT", false);
    choose(&mut app, "fr");
    assert_eq!(
        app.bookmark_browser.picking.as_ref().unwrap(),
        &BTreeSet::from([ids[1]])
    );
    choose(&mut app, "Select by series");
    choose(&mut app, "Unknown series");
    assert_eq!(
        app.bookmark_browser.picking.as_ref().unwrap(),
        &BTreeSet::from([ids[2], ids[3]])
    );
    choose(&mut app, "Apply book selection");
    assert_eq!(app.bookmark_browser.marks.len(), 2);
    assert_eq!(app.current_history_location(), Some(place.clone()));
    assert_eq!(fs::read(&fixture.state).unwrap(), saved);

    app.activate_bookmark_browser(Command::BookInfo(ids[2]))
        .unwrap();
    choose(&mut app, "View cover");
    assert_eq!(app.illustration.as_ref().unwrap().count(), 1);
    tap(&mut app, DONE);
    assert_eq!(app.screen, Screen::BookmarkBrowser(Page::Book));

    app.activate_bookmark_browser(Command::BookInfo(ids[0]))
        .unwrap();
    settle(&mut app);
    choose(&mut app, "View cover");
    assert_eq!(app.screen, Screen::Illustration);
    tap(&mut app, DONE);
    assert_eq!(app.screen, Screen::BookmarkBrowser(Page::Book));
    choose(&mut app, "Read description");
    capture_test_screen(
        &app,
        &fonts,
        "READER_BOOKMARK_DESCRIPTION_SCREENSHOT",
        false,
    );
    tap(
        &mut app,
        primitive_ui::back_button(primitive_app::WIDTH as i32),
    );
    app.save().unwrap();
    assert_eq!(fs::read(&fixture.state).unwrap(), saved);
    assert_eq!(app.current_history_location(), Some(place));

    fs::remove_file(&first).unwrap();
    let mut reopened = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
    reopened
        .activate_bookmark_browser(Command::Use(super::super::Source::Archive))
        .unwrap();
    reopened
        .activate_bookmark_browser(Command::BookInfo(ids[0]))
        .unwrap();
    assert!(
        !reopened
            .list_window()
            .1
            .iter()
            .any(|row| row.title == "View cover")
    );
    choose(&mut reopened, "Read description");
    assert_eq!(
        reopened
            .bookmark_browser
            .catalog
            .get(ids[0])
            .unwrap()
            .description
            .as_deref(),
        Some("A harbor & coast.\n\nKeep the shoreline in sight.")
    );
    assert_eq!(fs::read(&fixture.state).unwrap(), saved);
}

#[test]
fn retains_declared_metadata_from_html_htmlz_docx_and_fictionbook() {
    let fixture = Fixture::new();
    let html = r#"<html lang="EN-gb"><head><title>Harbor</title><meta name="keywords" content="Navigation&#10;Tides"/><meta name="description" content="A coastal description."/></head><body><p>A coastal page.</p></body></html>"#;
    let fb2 = r#"<FictionBook xmlns="http://www.gribuser.ru/xml/fictionbook/2.0"><description><title-info><book-title>Harbor</book-title><lang>EN-gb</lang><keywords>Navigation
Tides</keywords><sequence name="Coastal journals"/><annotation><p>A coastal description.</p></annotation></title-info></description><body><section><p>A coastal page.</p></section></body></FictionBook>"#;
    let opf = r#"<package><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>Harbor</dc:title><dc:language>EN-gb</dc:language><dc:subject>Navigation</dc:subject><dc:subject>Tides</dc:subject><dc:description>A coastal description.</dc:description><meta name="calibre:series" content="Coastal journals"/></metadata></package>"#;
    let types = r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#;
    let rels = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="main" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/><Relationship Id="core" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/></Relationships>"#;
    let core = r#"<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>Harbor</dc:title><dc:language>EN-gb</dc:language><dc:description>A coastal description.</dc:description><cp:keywords>Navigation
Tides</cp:keywords></cp:coreProperties>"#;
    let main = r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>A coastal page.</w:t></w:r></w:p></w:body></w:document>"#;
    let cases = [
        ("Harbor.html", html.as_bytes().to_vec(), false),
        ("Harbor.fb2", fb2.as_bytes().to_vec(), true),
        (
            "Harbor.htmlz",
            package(&[("document.html", html), ("metadata.opf", opf)]),
            true,
        ),
        (
            "Harbor.docx",
            package(&[
                ("[Content_Types].xml", types),
                ("_rels/.rels", rels),
                ("word/document.xml", main),
                ("docProps/core.xml", core),
            ]),
            false,
        ),
    ];
    for (name, bytes, series) in cases {
        let path = fixture.root.join(name);
        fs::write(&path, &bytes).unwrap();
        let mut library = Library::open(&fixture.root, &fixture.state).unwrap();
        library.scan().unwrap();
        let entry = library.resolve(std::path::Path::new(name)).unwrap();
        let id = library.identify(&entry).unwrap();
        let state = library.track(&entry, id).clone();
        let snapshot = read_snapshot(&library, &state).unwrap();
        assert_eq!(snapshot.language.as_deref(), Some("en-gb"), "{name}");
        assert_eq!(snapshot.keywords, ["Navigation", "Tides"], "{name}");
        assert_eq!(
            snapshot.description.as_deref(),
            Some("A coastal description."),
            "{name}"
        );
        assert_eq!(snapshot.series == ["Coastal journals"], series, "{name}");
        assert_eq!(fs::read(path).unwrap(), bytes);
    }
}

#[test]
fn long_descriptions_page_without_moving_the_book_and_reverse_their_controls() {
    let fixture = Fixture::new();
    let source = fixture.root.join("Harbor journal.epub");
    let paragraphs = "&lt;p&gt;Keep the harbor, lighthouse and shoreline in sight as the tide returns.&lt;/p&gt;".repeat(70);
    fs::write(
        &source,
        epub(&format!("<dc:description>{paragraphs}</dc:description>")),
    )
    .unwrap();
    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
    app.open_path(&source).unwrap();
    app.activate(Action::AddBookmark).unwrap();
    app.save().unwrap();
    let place = app.current_history_location().unwrap();
    let id = app.open_book_id().unwrap();
    app.activate_bookmark_browser(Command::Use(super::super::Source::History))
        .unwrap();
    settle(&mut app);
    app.activate_bookmark_browser(Command::BookInfo(id))
        .unwrap();
    choose(&mut app, "Read description");
    let font_paths: Vec<_> = fonts
        .iter()
        .map(|font| font.display().to_string())
        .collect();
    let mut canvas =
        primitive_ui::Canvas::new(primitive_app::WIDTH, primitive_app::HEIGHT, &font_paths)
            .unwrap();
    primitive_app::frame(&app, &mut canvas, false);
    assert!(app.bookmark_browser.text_pages.borrow().len() > 2);
    assert_eq!(app.input(Input::PageForward), Change::Partial);
    assert_eq!(
        app.bookmark_browser
            .text_page(&app.bookmark_browser.text_pages.borrow()),
        1
    );
    for dark in [false, true] {
        canvas.clear_press();
        let before = primitive_app::frame(&app, &mut canvas, dark);
        let layout = super::super::BrowserLayout(app.navigation_layout());
        for rect in [layout.control(0, 214, 72), layout.control(2, 214, 72)] {
            // The edge falls within the expanded 100 px target above its visible outline.
            assert!(canvas.press_at((rect.x + 20) as f32, (rect.y - 10) as f32));
            let after = primitive_app::frame(&app, &mut canvas, dark);
            let mut changed = 0;
            for (offset, (left, right)) in before.iter().zip(&after).enumerate() {
                if left != right {
                    assert!(rect.contains(
                        (offset % primitive_app::WIDTH as usize) as f32,
                        (offset / primitive_app::WIDTH as usize) as f32
                    ));
                    changed += 1;
                }
            }
            assert!(changed > 100);
            canvas.clear_press();
            primitive_app::frame(&app, &mut canvas, dark);
        }
    }
    assert_eq!(app.input(Input::PageBack), Change::Partial);
    assert_eq!(app.bookmark_browser.text_offset, 0);
    assert_eq!(app.current_history_location(), Some(place));
    let cache_path = app.bookmark_browser.catalog.path.clone();
    fs::remove_file(&cache_path).unwrap();
    fs::create_dir(&cache_path).unwrap();
    app.bookmark_browser.catalog.dirty = true;
    app.follow_reflow_page(1, 0).unwrap();
    app.activate(Action::AddBookmark).unwrap();
    app.save().unwrap();
    assert!(app.bookmark_browser.catalog.dirty);
    let disk = Library::open(&fixture.root, &fixture.state).unwrap();
    assert_eq!(
        disk.state(id).unwrap().position.as_ref().unwrap().chapter,
        1
    );
    assert_eq!(disk.state(id).unwrap().bookmarks.len(), 2);
}

#[test]
#[ignore = "requires a private list of authorized original documents"]
fn audits_metadata_in_the_original_documents_with_isolated_state() {
    let root = PathBuf::from(std::env::var_os("READER_TEST_BOOKMARK_METADATA_ROOT").unwrap());
    let list = PathBuf::from(std::env::var_os("READER_TEST_BOOKMARK_METADATA_LIST").unwrap());
    let fixture = Fixture::new();
    let mut library = Library::open(&root, &fixture.state).unwrap();
    let mut snapshots = Vec::new();
    for name in fs::read_to_string(list).unwrap().lines() {
        let entry = library.resolve(std::path::Path::new(name)).unwrap();
        let id = library.identify(&entry).unwrap();
        let state = library.track(&entry, id).clone();
        let snapshot = read_snapshot(&library, &state).unwrap();
        snapshots.push(serde_json::to_value(snapshot).unwrap());
    }
    assert!(!snapshots.is_empty());
    if let Some(path) = std::env::var_os("READER_TEST_BOOKMARK_METADATA_OUTPUT") {
        fs::write(path, serde_json::to_vec_pretty(&snapshots).unwrap()).unwrap();
    }
    eprintln!(
        "Audited publisher metadata from {} original documents",
        snapshots.len()
    );
}
