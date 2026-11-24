use super::*;
use crate::app::tests::{Fixture, capture_test_screen};
use crate::app::{Action, HEIGHT, ReadingPosition, WIDTH};
use crate::reading_controls::{FooterTrackPosition, ProgressScope, StatusBar};
use primitive_ui::Canvas;
use std::{fs, io::Write, path::Path};

fn open(fixture: &Fixture, path: &Path) -> Reader {
    let fonts = vec![
        crate::test_fixture("fonts/NotoSans-Regular.ttf"),
        crate::test_fixture("fonts/NotoSans-Bold.ttf"),
    ];
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
    app.open_path(path).unwrap();
    app
}

fn state_mut(app: &mut Reader) -> &mut BookState {
    let book = app.open_book_id().unwrap();
    app.library.state_mut(book).unwrap()
}

fn frame(app: &Reader) -> Vec<u8> {
    let fonts = app
        .fonts
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>();
    let mut canvas = Canvas::new(WIDTH, HEIGHT, &fonts).unwrap();
    primitive_app::frame(app, &mut canvas, false)
}

fn write_pdf(path: &Path) {
    let page = "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 390 550] /Contents 15 0 R >>";
    let content = "BT /F1 25 Tf 31 485 Td (Field guide to the coast) Tj /F1 12 Tf 0 -35 Td (The western tide arrives just before dusk.) Tj 0 -21 Td (Use the sheltered route past the salt marsh.) Tj 0 -21 Td (The lookout opens when the water recedes.) Tj ET";
    let stream = format!(
        "<< /Length {} >>\nstream\n{content}\nendstream",
        content.len()
    );
    fs::write(path, crate::fixed::tests::pdf_objects(&[
        "<< /Type /Catalog /Pages 2 0 R /Outlines 9 0 R >>",
        "<< /Type /Pages /Kids [3 0 R 4 0 R 5 0 R 6 0 R 7 0 R 8 0 R] /Count 6 /Resources << /Font << /F1 14 0 R >> >> >>",
        page, page, page, page, page, page,
        "<< /Type /Outlines /First 10 0 R /Last 13 0 R /Count 4 >>",
        "<< /Title (Western tide) /Dest [4 0 R /Fit] /Parent 9 0 R /Next 13 0 R /First 11 0 R /Last 12 0 R /Count 2 >>",
        "<< /Title (Harbor route) /Dest [4 0 R /Fit] /Parent 10 0 R /Next 12 0 R >>",
        "<< /Title (Salt marsh) /Dest [6 0 R /Fit] /Parent 10 0 R /Prev 11 0 R >>",
        "<< /Title (Final approach) /Dest [7 0 R /Fit] /Parent 9 0 R /Prev 10 0 R >>",
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
        &stream,
    ])).unwrap();
}

#[test]
fn pdf_progress_markers_filter_before_deduplication_and_keep_the_footer_layout() {
    let fixture = Fixture::new();
    let path = fixture.root.join("Field guide to the coast.pdf");
    write_pdf(&path);
    let mut app = open(&fixture, &path);
    app.controls.status_bar = StatusBar::Chapter;
    app.controls.footer_scope = ProgressScope::Book;
    app.controls.footer_track_position = FooterTrackPosition::Below;
    app.controls.hide_chapter_markers = false;
    assert!(app.whole_book_marker_pages(true).is_none());
    assert_eq!(
        app.whole_book_marker_pages(false).unwrap().starts,
        [1, 3, 4]
    );
    let baseline = frame(&app);
    let caption = app.status_caption();
    let bar = app.footer_track_rect(0).unwrap();
    capture_test_screen(&app, &app.fonts, "READER_PROGRESS_MARKERS_DEFAULT", false);
    let marker = |page: usize| {
        let x = bar.x + (page as f64 * f64::from(bar.w) / 6.).round() as i32;
        ((bar.y - 1) * WIDTH as i32 + x) as usize
    };
    assert_eq!(baseline[marker(4)], 0);

    state_mut(&mut app).contents_depths.toggle(0);
    assert!(!state_mut(&mut app).contents_depths.bind_navigation);
    assert_eq!(app.whole_book_marker_pages(true).unwrap().starts, [1, 3]);
    let filtered = frame(&app);
    assert_eq!(app.status_caption(), caption);
    assert_eq!(app.footer_track_rect(0), Some(bar));
    assert_eq!(
        &baseline[..((bar.y - 1) as u32 * WIDTH) as usize],
        &filtered[..((bar.y - 1) as u32 * WIDTH) as usize]
    );
    assert_eq!(filtered[marker(1)], 0);
    assert_eq!(filtered[marker(3)], 0);
    assert_ne!(filtered[marker(4)], 0);
    capture_test_screen(&app, &app.fonts, "READER_PROGRESS_MARKERS_FILTERED", false);
    app.activate(Action::Skim).unwrap();
    capture_test_screen(&app, &app.fonts, "READER_PROGRESS_MARKERS_SKIM", false);
    app.show(crate::app::Screen::Fixed);

    state_mut(&mut app).contents_depths.toggle(0);
    assert!(app.whole_book_marker_pages(true).is_none());
    assert_eq!(frame(&app), baseline);
}

fn write_epub(path: &Path, extra_spine: bool) {
    let mut archive = zip::ZipWriter::new(fs::File::create(path).unwrap());
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let extra_reference = if extra_spine {
        "<itemref idref='after'/>"
    } else {
        ""
    };
    let container = r#"<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container" version="1.0"><rootfiles><rootfile full-path="publication.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#;
    let package = format!(
        r#"<package xmlns="http://www.idpf.org/2007/opf" version="3.0"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>Field notes for the coast</dc:title></metadata><manifest><item id="text" href="text.xhtml" media-type="application/xhtml+xml"/><item id="after" href="after.xhtml" media-type="application/xhtml+xml"/><item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/></manifest><spine><itemref idref="text"/>{extra_reference}</spine></package>"#
    );
    let nav = r##"<html xmlns:epub="http://www.idpf.org/2007/ops"><body><nav epub:type="toc"><ol><li><a href="text.xhtml#path">Western approach</a><ol><li><a href="text.xhtml#path">Harbor route</a></li><li><a href="text.xhtml#marsh">Salt marsh</a></li></ol></li><li><a href="text.xhtml#end">Headland</a></li></ol></nav></body></html>"##;
    let prose = "The coastal path follows the ridge from the harbor to the salt marsh. ".repeat(56);
    let text = format!(
        "<html><body><p>{prose}</p><h1 id='path'>Western approach</h1><p>{prose}</p><h2 id='marsh'>Salt marsh</h2><p>{prose}</p><h1 id='end'>Headland</h1></body></html>"
    );
    for (name, contents) in [
        ("mimetype", "application/epub+zip"),
        ("META-INF/container.xml", container),
        ("publication.opf", &package),
        ("nav.xhtml", nav),
        ("text.xhtml", &text),
        ("after.xhtml", "<html><body>After the tide</body></html>"),
    ] {
        archive.start_file(name, options).unwrap();
        archive.write_all(contents.as_bytes()).unwrap();
    }
    archive.finish().unwrap();
}

#[test]
fn reflow_progress_markers_reuse_local_layout_and_custom_source_anchors() {
    let fixture = Fixture::new();
    write_epub(&fixture.book, false);
    let mut app = open(&fixture, &fixture.book);
    let probe = &app.active.as_ref().unwrap().reading.probe;
    let [path, marsh, end] = ["path", "marsh", "end"]
        .map(|fragment| probe.page_for(&probe.fragment(fragment).unwrap()).unwrap());
    let anchor = probe.fragment("marsh").unwrap();
    assert!(0 < path && path < marsh && marsh < end);
    assert!(app.whole_book_marker_pages(true).is_none());
    assert_eq!(
        app.whole_book_marker_pages(false).unwrap().starts,
        [path, marsh, end]
    );
    state_mut(&mut app).contents_depths.toggle(0);
    assert_eq!(
        app.whole_book_marker_pages(true).unwrap().starts,
        [path, marsh]
    );

    let custom = &mut state_mut(&mut app).custom_contents;
    custom
        .save(
            ReadingPosition {
                chapter: 0,
                page: marsh + 10,
                anchor: Some(anchor),
            },
            "Marsh notes",
        )
        .unwrap();
    custom.enabled = true;
    assert!(app.whole_book_marker_pages(true).unwrap().starts.is_empty());
    state_mut(&mut app).contents_depths.toggle(0);
    assert_eq!(app.whole_book_marker_pages(true).unwrap().starts, [marsh]);
}

#[test]
fn multispine_books_only_filter_the_existing_local_header_marks() {
    let fixture = Fixture::new();
    write_epub(&fixture.book, true);
    let mut app = open(&fixture, &fixture.book);
    let before = frame(&app);
    state_mut(&mut app).contents_depths.toggle(0);
    assert!(app.whole_book_marker_pages(false).is_none());
    let active = app.active.as_ref().unwrap();
    let probe = &active.reading.probe;
    let [path, marsh] = ["path", "marsh"]
        .map(|fragment| probe.page_for(&probe.fragment(fragment).unwrap()).unwrap());
    assert_eq!(
        app.customized_header_marker_pages(active).unwrap(),
        [path, marsh]
    );
    assert_eq!(
        frame(&app),
        before,
        "The header is off by default and the whole-book footer uses the original source track"
    );
}
