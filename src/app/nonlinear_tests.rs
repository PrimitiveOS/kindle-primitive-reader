use super::tests::{Fixture, capture_test_screen};
use super::*;
use std::io::Write;
use zip::{ZipWriter, write::SimpleFileOptions};

pub(super) fn authored_book(path: &Path) {
    let mut zip = ZipWriter::new(fs::File::create(path).unwrap());
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let members = [
        ("mimetype", "application/epub+zip".to_owned()),
        ("META-INF/container.xml", r#"<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container" version="1.0"><rootfiles><rootfile full-path="book.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#.into()),
        ("book.opf", r#"<package xmlns="http://www.idpf.org/2007/opf" version="3.0"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>Off the Main Path</dc:title></metadata><manifest>
          <item id="front" href="front.xhtml" media-type="application/xhtml+xml"/>
          <item id="main-one" href="main-one.xhtml" media-type="application/xhtml+xml"/>
          <item id="note-one" href="note-one.xhtml" media-type="application/xhtml+xml"/>
          <item id="note-two" href="note-two.xhtml" media-type="application/xhtml+xml"/>
          <item id="main-two" href="main-two.xhtml" media-type="application/xhtml+xml"/>
          <item id="end" href="end.xhtml" media-type="application/xhtml+xml"/>
          <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
        </manifest><spine><itemref idref="front" linear="no"/><itemref idref="main-one"/>
          <itemref idref="note-one" linear="no"/><itemref idref="note-two" linear="no"/>
          <itemref idref="main-two" linear="yes"/><itemref idref="end" linear="no"/>
        </spine></package>"#.into()),
        ("front.xhtml", "<html><body><p id='front'>Front cover.</p></body></html>".into()),
        ("main-one.xhtml", "<html><body><h1 id='first'>The first coast</h1><p id='source'>Read the main path. <a href='note-one.xhtml#note'>A long note</a>.</p></body></html>".into()),
        ("note-one.xhtml", format!("<html><body><h1 id='note'>Long note</h1>{}</body></html>", "<p>Waves carry the sound of distant rain across the harbor.</p>".repeat(100))),
        ("note-two.xhtml", "<html><body><p id='other'>A separate note.</p></body></html>".into()),
        ("main-two.xhtml", "<html><body><h1 id='last'>The second coast</h1><p>The main story ends here.</p></body></html>".into()),
        ("end.xhtml", "<html><body><p id='end'>Rear cover.</p></body></html>".into()),
        ("nav.xhtml", r#"<html xmlns:epub="http://www.idpf.org/2007/ops"><body><nav epub:type="toc"><ol><li><a href="main-one.xhtml#first">The first coast</a></li><li><a href="note-one.xhtml#note">Long note</a></li><li><a href="note-two.xhtml#other">Separate note</a></li><li><a href="main-two.xhtml#last">The second coast</a></li></ol></nav></body></html>"#.into()),
    ];
    for (name, text) in members {
        zip.start_file(name, options).unwrap();
        zip.write_all(text.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
}

fn turn(app: &mut Reader, forward: bool) {
    app.show(Screen::Book);
    app.input(if forward {
        Input::PageForward
    } else {
        Input::PageBack
    });
    let due = app.active.as_ref().unwrap().reading.deadline().unwrap();
    app.tick(due + Duration::from_millis(1));
}

#[test]
fn combined_footnote_flows_leave_reference_bands_in_the_regular_reading_path() {
    use crate::user_styles::{COMBINE_NONLINEAR_NOTES, EPUB_FOOTNOTES};

    let fixture = Fixture::new();
    fs::copy(
        crate::test_fixture("nonlinear-note-flows.epub"),
        &fixture.book,
    )
    .unwrap();
    let original = fs::read(&fixture.book).unwrap();
    let fonts = vec![
        crate::test_fixture("fonts/NotoSans-Regular.ttf"),
        crate::test_fixture("fonts/NotoSans-Bold.ttf"),
    ];
    fn read(app: &Reader) -> &ActiveBook {
        app.active.as_ref().unwrap()
    }
    let open = || {
        let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
        app.open_path(&fixture.book).unwrap();
        app
    };
    let mut app = open();
    let id = app.open_book_id().unwrap();
    assert!(!read(&app).reading.has_nonlinear_fragments());
    let initial = app.current_history_location();
    app.activate(Action::Style(style_tweaks::Command::Choose(
        EPUB_FOOTNOTES.into(),
        Some(true),
    )))
    .unwrap();
    app.activate(Action::Style(style_tweaks::Command::Choose(
        COMBINE_NONLINEAR_NOTES.into(),
        Some(true),
    )))
    .unwrap();
    assert_eq!(app.current_history_location(), initial);
    assert!(read(&app).reading.has_nonlinear_fragments());
    assert!(!read(&app).reading.hides_nonlinear());
    let ordered_weights = read(&app).reading.progress_weights().to_vec();
    let turn_toggle = |app: &mut Reader| {
        let item = app
            .top_config_items(TopGroup::Navigation)
            .into_iter()
            .find(|item| item.title == "Hide non-linear fragments")
            .unwrap();
        app.activate(item.action).unwrap();
        app.show(Screen::Book);
    };
    turn_toggle(&mut app);
    assert_eq!(app.current_history_location(), initial);
    let reading = &read(&app).reading;
    let page_for = |id| {
        reading
            .probe
            .fragment(id)
            .and_then(|anchor| reading.probe.page_for(&anchor))
            .unwrap()
    };
    let map = reading.custom_flow_map().unwrap();
    let note_pages = [page_for("note-a"), page_for("note-b"), page_for("note-c")];
    let middle = page_for("middle");
    let finish = page_for("finish");
    assert_ne!(map.flow(note_pages[0]), 0);
    assert_eq!(map.flow(note_pages[0]), map.flow(note_pages[1]));
    assert_ne!(map.flow(note_pages[2]), map.flow(note_pages[0]));
    assert_eq!(map.flow(middle), 0);
    assert_eq!(map.flow(finish), 0);
    assert_eq!(reading.probe.canonical_note_flows().len(), 2);
    assert!(app.status_caption().unwrap().starts_with("Chapter "));
    let weights = reading.progress_weights();
    assert!(weights[1] < ordered_weights[1]);
    assert!(weights[3] - weights[2] < ordered_weights[3] - ordered_weights[2]);
    assert_eq!(
        weights[2] - weights[1],
        ordered_weights[2] - ordered_weights[1]
    );
    let first_note = reading.probe.matches("Red lantern schedule").remove(0);
    let end = crate::Anchor {
        offset: first_note.offset + "Red lantern schedule".len(),
        ..first_note.clone()
    };
    let selection = reading.probe.select_range(&first_note, &end).unwrap();
    assert_eq!(selection.text, "Red lantern schedule");
    assert!(
        !reading
            .probe
            .selection_rects(reading.page, &selection)
            .is_empty()
    );
    capture_test_screen(&app, &fonts, "READER_NONLINEAR_NOTE_BAND", false);

    let origin_page = read(&app).reading.page;
    turn(&mut app, true);
    assert_eq!(read(&app).reading.page, middle);
    turn(&mut app, true);
    assert_eq!(read(&app).reading.page, finish);
    turn(&mut app, true);
    assert_eq!(read(&app).reading.chapter, 1);
    assert!(read(&app).reading.has_nonlinear_fragments());
    turn(&mut app, false);
    assert_eq!(read(&app).reading.page, finish);
    app.activate(Action::Jump(crate::epub::Destination {
        path: "survey.xhtml".into(),
        fragment: Some("note-a".into()),
    }))
    .unwrap();
    let selected = app.current_history_location();
    assert_eq!(read(&app).reading.anchor(), Some(first_note));
    assert_eq!(read(&app).reading.page, note_pages[0]);
    assert!(
        app.status_caption()
            .unwrap()
            .starts_with("Hidden 1 - Chapter ")
    );
    capture_test_screen(&app, &fonts, "READER_NONLINEAR_NOTE_ORIGINALS", false);
    app.save().unwrap();
    drop(app);

    let mut reopened = open();
    assert_eq!(reopened.current_history_location(), selected);
    assert!(
        reopened
            .style_store
            .data
            .enabled(id, COMBINE_NONLINEAR_NOTES)
    );
    let reading = &read(&reopened).reading;
    assert_ne!(reading.custom_flow_map().unwrap().flow(reading.page), 0);
    turn(&mut reopened, false);
    assert_eq!(read(&reopened).reading.page, origin_page);
    let anchor = read(&reopened).reading.anchor();
    reopened
        .activate(Action::Flow(hidden_flows::FlowAction::Toggle))
        .unwrap();
    assert!(!read(&reopened).reading.hides_nonlinear());
    assert!(
        read(&reopened)
            .reading
            .probe
            .canonical_note_flows()
            .is_empty()
    );
    assert_eq!(read(&reopened).reading.anchor(), anchor);
    reopened
        .activate(Action::Flow(hidden_flows::FlowAction::Toggle))
        .unwrap();
    assert!(read(&reopened).reading.hides_nonlinear());
    assert_eq!(read(&reopened).reading.anchor(), anchor);
    turn_toggle(&mut reopened);
    assert!(read(&reopened).reading.custom_flow_map().is_none());
    assert_eq!(read(&reopened).reading.progress_weights(), ordered_weights);
    assert_eq!(fs::read(&fixture.book).unwrap(), original);
}

#[test]
fn nonlinear_fragments_are_skipped_only_in_single_page_and_keep_explicit_destinations() {
    let fixture = Fixture::new();
    authored_book(&fixture.book);
    if let Some(output) = std::env::var_os("READER_NONLINEAR_FIXTURE") {
        fs::copy(&fixture.book, output).unwrap();
    }
    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
    app.open_path(&fixture.book).unwrap();
    assert_eq!(app.active.as_ref().unwrap().reading.chapter, 0);
    assert!(
        app.active
            .as_ref()
            .unwrap()
            .reading
            .probe
            .has_nonlinear_chapters()
    );
    app.activate(Action::ToggleNonlinear).unwrap();
    assert!(app.active.as_ref().unwrap().reading.hides_nonlinear());
    let id = app.open_book_id().unwrap();
    turn(&mut app, true);
    assert_eq!(app.active.as_ref().unwrap().reading.chapter, 1);
    let weights = app.active.as_ref().unwrap().reading.progress_weights();
    assert_eq!(weights.len(), 7);
    assert_eq!(weights[0], weights[1]);
    assert_eq!(weights[2], weights[3]);
    assert_eq!(weights[3], weights[4]);
    assert_eq!(weights[5], weights[6]);
    let origin = app.current_history_location().unwrap();
    let entry = app
        .active
        .as_ref()
        .unwrap()
        .reading
        .probe
        .toc
        .iter()
        .find(|item| item.label == "Long note")
        .unwrap()
        .destination
        .clone()
        .unwrap();
    app.activate(Action::Jump(entry)).unwrap();
    let reading = &app.active.as_ref().unwrap().reading;
    assert_eq!(reading.chapter, 2);
    assert!(reading.probe.page_count() > 1);
    turn(&mut app, true);
    assert_eq!(app.active.as_ref().unwrap().reading.chapter, 2);
    assert!(app.active.as_ref().unwrap().reading.page > 0);
    app.activate(Action::ReturnToLocation).unwrap();
    assert_eq!(app.current_history_location(), Some(origin));
    turn(&mut app, true);
    assert_eq!(app.active.as_ref().unwrap().reading.chapter, 4);
    turn(&mut app, false);
    assert_eq!(app.active.as_ref().unwrap().reading.chapter, 1);

    app.active
        .as_mut()
        .unwrap()
        .reading
        .jump(2, Some("note"))
        .unwrap();
    let last = app.active.as_ref().unwrap().reading.probe.page_count() - 1;
    app.active
        .as_mut()
        .unwrap()
        .reading
        .go_to_page(2, last)
        .unwrap();
    turn(&mut app, true);
    assert_eq!(app.active.as_ref().unwrap().reading.chapter, 3);
    let weights = app.active.as_ref().unwrap().reading.progress_weights();
    assert_eq!(weights[0], weights[2]);
    assert!(weights[3] > weights[2]);
    assert!(weights[4] > weights[3]);
    assert_eq!(weights[4], weights[6]);
    turn(&mut app, false);
    assert_eq!(app.active.as_ref().unwrap().reading.chapter, 2);
    app.active
        .as_mut()
        .unwrap()
        .reading
        .jump(2, Some("note"))
        .unwrap();
    turn(&mut app, false);
    assert_eq!(app.active.as_ref().unwrap().reading.chapter, 1);
    app.active
        .as_mut()
        .unwrap()
        .reading
        .jump_progress(0.0)
        .unwrap();
    assert_eq!(app.active.as_ref().unwrap().reading.chapter, 1);
    app.active
        .as_mut()
        .unwrap()
        .reading
        .jump_progress(1.0)
        .unwrap();
    assert_eq!(app.active.as_ref().unwrap().reading.chapter, 4);
    app.active
        .as_mut()
        .unwrap()
        .reading
        .jump_progress(0.5)
        .unwrap();
    assert!([1, 4].contains(&app.active.as_ref().unwrap().reading.chapter));

    app.activate(Action::ReadingMode(true)).unwrap();
    assert!(!app.active.as_ref().unwrap().reading.hides_nonlinear());
    app.active
        .as_mut()
        .unwrap()
        .reading
        .jump_progress(0.0)
        .unwrap();
    assert_eq!(app.active.as_ref().unwrap().reading.chapter, 0);
    app.activate(Action::ReadingMode(false)).unwrap();
    app.activate(Action::FacingPages(true)).unwrap();
    assert!(!app.active.as_ref().unwrap().reading.hides_nonlinear());
    app.activate(Action::FacingPages(false)).unwrap();
    app.active
        .as_mut()
        .unwrap()
        .reading
        .jump(3, Some("other"))
        .unwrap();
    app.record_position();
    let note_position = app.current_position().unwrap();
    app.save().unwrap();
    let mut reopened = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
    reopened.open_path(&fixture.book).unwrap();
    assert_eq!(
        reopened.library.state(id).unwrap().hide_nonlinear,
        Some(true)
    );
    assert_eq!(reopened.current_position(), Some(note_position));
    reopened.show(Screen::BookTools);
    reopened.top_group = Some(top_config::TopGroup::Navigation);
    capture_test_screen(&reopened, &fonts, "READER_NONLINEAR_SCREENSHOT", false);
    capture_test_screen(&reopened, &fonts, "READER_NONLINEAR_DARK_SCREENSHOT", true);
    let config = reopened.active.as_ref().unwrap().reading.config.clone();
    let first = Reading::open(config).unwrap();
    assert_eq!(first.chapter, 1);
}

#[test]
#[ignore = "reads authorized local EPUBs when READER_NONLINEAR_ORIGINALS is supplied"]
fn nonlinear_original_covers_preserve_source_indexes_and_resume() {
    let Some(root) = std::env::var_os("READER_NONLINEAR_ORIGINALS") else {
        return;
    };
    let root = PathBuf::from(root);
    let fixture = Fixture::new();
    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    for (number, chapters) in [(36, 20), (45, 32), (61, 59), (68, 2), (69, 46)] {
        let path = root.join(format!("{number:03}.epub"));
        let before = fs::read(&path).unwrap();
        let book = Book::open(&path).unwrap();
        assert_eq!(book.chapters.len(), chapters);
        assert!(book.chapters[0].nonlinear);
        assert!(book.chapters[1..].iter().all(|chapter| !chapter.nonlinear));
        let mut app = Reader::open(&root, &fixture.state, fonts.clone()).unwrap();
        app.open_path(&path).unwrap();
        app.activate(Action::ToggleNonlinear).unwrap();
        app.active
            .as_mut()
            .unwrap()
            .reading
            .jump_progress(0.0)
            .unwrap();
        assert_eq!(app.active.as_ref().unwrap().reading.chapter, 1);
        let first = app.current_position().unwrap();
        app.record_position();
        app.save().unwrap();
        let mut reopened = Reader::open(&root, &fixture.state, fonts.clone()).unwrap();
        reopened.open_path(&path).unwrap();
        assert!(reopened.active.as_ref().unwrap().reading.hides_nonlinear());
        assert_eq!(reopened.current_position(), Some(first));
        reopened
            .active
            .as_mut()
            .unwrap()
            .reading
            .jump(0, None)
            .unwrap();
        turn(&mut reopened, true);
        assert_eq!(reopened.active.as_ref().unwrap().reading.chapter, 1);
        turn(&mut reopened, false);
        assert_eq!(reopened.active.as_ref().unwrap().reading.chapter, 1);
        assert!(
            before == fs::read(&path).unwrap(),
            "Reading changed the source EPUB"
        );
    }
}
