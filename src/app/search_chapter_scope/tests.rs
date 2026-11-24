use super::*;
use crate::app::tests::{Fixture, wait_for_search};
use std::fs;
use std::io::Write;
use std::path::Path;
use std::time::Instant;

fn open(fixture: &Fixture, path: &Path) -> Reader {
    let mut reader = Reader::open(
        &fixture.root,
        &fixture.state,
        vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")],
    )
    .unwrap();
    reader.open_path(path).unwrap();
    reader
}

fn members(reader: &Reader) -> Vec<usize> {
    let old = reader.current_history_location();
    let scope = reader.current_search_chapter().unwrap();
    assert_eq!(reader.current_history_location(), old);
    assert!(!scope.includes_result(reader, usize::MAX));
    let count = reader.pdf_hits.len().max(reader.hits.len());
    (0..count)
        .filter(|&index| scope.includes_result(reader, index))
        .collect()
}

#[test]
fn search_scope_fixed_keeps_tied_headings_and_equal_titles_separate() {
    let fixture = Fixture::new();
    let path = fixture.root.join("Coastal reports.pdf");
    let page = "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 430] /Contents 9 0 R >>";
    let body = "BT /F1 16 Tf 25 370 Td (Survey marker) Tj ET";
    let stream = format!("<< /Length {} >>\nstream\n{body}\nendstream", body.len());
    let bytes = crate::fixed::tests::pdf_objects(&[
        "<< /Type /Catalog /Pages 2 0 R /Outlines 11 0 R >>",
        "<< /Type /Pages /Kids [3 0 R 4 0 R 5 0 R 6 0 R 7 0 R 8 0 R] /Count 6 /Resources << /Font << /F1 10 0 R >> >> >>",
        page,
        page,
        page,
        page,
        page,
        page,
        &stream,
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
        "<< /Type /Outlines /First 12 0 R /Last 14 0 R /Count 3 >>",
        "<< /Title (Survey) /Dest [4 0 R /Fit] /Parent 11 0 R /Next 14 0 R /First 13 0 R /Last 13 0 R /Count 1 >>",
        "<< /Title (Survey) /Dest [4 0 R /Fit] /Parent 12 0 R >>",
        "<< /Title (Survey) /Dest [7 0 R /Fit] /Parent 11 0 R /Prev 12 0 R >>",
    ]);
    fs::write(&path, bytes).unwrap();

    let mut reader = open(&fixture, &path);
    reader.start_document_search("marker").unwrap();
    let now = Instant::now();
    while reader.search_at.is_some() {
        reader.step_pdf_search(now).unwrap();
    }
    assert_eq!(reader.pdf_hits.len(), 6);
    reader.follow_fixed(4, None).unwrap();
    let last = reader.pdf_hits.pop().unwrap();
    let scope = reader.current_search_chapter().unwrap();
    assert!(!scope.includes_result(&reader, 5));
    reader.pdf_hits.push(last);
    assert!(scope.includes_result(&reader, 5));
    for (page, expected) in [
        (0, &[0][..]),
        (1, &[1][..]),
        (2, &[2, 3][..]),
        (3, &[2, 3][..]),
        (4, &[4, 5][..]),
    ] {
        reader.follow_fixed(page, None).unwrap();
        assert_eq!(members(&reader), expected);
    }

    let id = reader.open_book_id().unwrap();
    let depths = &mut reader.library.state_mut(id).unwrap().contents_depths;
    depths.bind_title = true;
    depths.toggle(1);
    reader.follow_fixed(1, None).unwrap();
    assert_eq!(members(&reader), [1, 2, 3]);

    let custom = &mut reader.library.state_mut(id).unwrap().custom_contents;
    custom.enabled = true;
    custom
        .save(
            ReadingPosition {
                chapter: 0,
                page: 3,
                anchor: None,
            },
            "Coastal survey",
        )
        .unwrap();
    assert_eq!(members(&reader), [0, 1, 2]);
    reader.follow_fixed(3, None).unwrap();
    assert_eq!(members(&reader), [3, 4, 5]);
}

pub(in crate::app) fn authored_epub(path: &Path, include_toc: bool) {
    let filler = "<p>Through the afternoon we followed the shore, turning inland only at the old stone wall.</p>".repeat(100);
    let first = format!(
        "<h1>Preface marker</h1><h2 id=\"first\">Alpha marker</h2><p>Before the turn <span id=\"second\">Beta marker</span>.</p>{filler}"
    );
    let second = format!(
        "<h1 id=\"omitted\" style=\"display:none\">Omitted outline</h1><p>Charlie marker.</p>{filler}<p>Crossing inland <span id=\"third\">Delta marker</span>.</p>"
    );
    let package = format!(
        r#"<package xmlns="http://www.idpf.org/2007/opf" version="3.0"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>Coastal reports</dc:title></metadata><manifest>{}<item id="west" href="west.xhtml" media-type="application/xhtml+xml"/><item id="east" href="east.xhtml" media-type="application/xhtml+xml"/><item id="last" href="last.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="west"/><itemref idref="east"/><itemref idref="last"/></spine></package>"#,
        if include_toc {
            r#"<item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>"#
        } else {
            ""
        }
    );
    let nav = r#"<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><body><nav epub:type="toc"><ol><li><a href="west.xhtml#first">Survey</a><ol><li><a href="west.xhtml#second">Survey</a></li></ol></li><li><a href="east.xhtml#omitted">Survey</a></li><li><a href="east.xhtml#third">Survey</a></li><li><a href="last.xhtml">Survey</a></li></ol></nav></body></html>"#;
    let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
    let options = zip::write::SimpleFileOptions::default();
    let xhtml = |body: &str| {
        format!(
            r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Coastal reports</title></head><body>{body}</body></html>"#
        )
    };
    let first = xhtml(&first);
    let second = xhtml(&second);
    let last = xhtml("<p>Echo marker.</p>");
    let container = r#"<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container" version="1.0"><rootfiles><rootfile full-path="EPUB/book.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#;
    for (name, content) in [
        ("mimetype", "application/epub+zip"),
        ("META-INF/container.xml", container),
        ("EPUB/book.opf", &package),
        ("EPUB/west.xhtml", &first),
        ("EPUB/east.xhtml", &second),
        ("EPUB/last.xhtml", &last),
    ]
    .into_iter()
    .chain(include_toc.then_some(("EPUB/nav.xhtml", nav)))
    {
        zip.start_file(name, options).unwrap();
        zip.write_all(content.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
}

#[test]
fn search_scope_epub_uses_inline_source_anchors_across_spines() {
    let fixture = Fixture::new();
    let path = fixture.root.join("Coastal reports.epub");
    authored_epub(&path, true);
    let mut reader = open(&fixture, &path);
    reader.start_document_search("marker").unwrap();
    wait_for_search(&mut reader, usize::MAX);
    assert_eq!(reader.hits.len(), 6);
    assert_eq!(members(&reader), [1]);

    let reading = &mut reader.active.as_mut().unwrap().reading;
    assert_eq!(
        reading
            .probe
            .page_for(&reading.probe.fragment("second").unwrap()),
        Some(0)
    );
    reading.go_to_page(0, 1).unwrap();
    // The next outline is a fragment in a different spine. Charlie is before
    // it and Delta is after it, even though both share the same spine index.
    assert_eq!(members(&reader), [2, 3]);

    let reading = &mut reader.active.as_mut().unwrap().reading;
    reading.go_to_page(1, 0).unwrap();
    assert_eq!(members(&reader), [2, 3]);
    let reading = &mut reader.active.as_mut().unwrap().reading;
    let third = reading.probe.fragment("third").unwrap();
    let page = reading.probe.page_for(&third).unwrap();
    assert!(page > 0);
    reading.go_to_page(1, page).unwrap();
    assert_eq!(members(&reader), [4]);

    let reading = &mut reader.active.as_mut().unwrap().reading;
    reading.go_to_page(2, 0).unwrap();
    assert_eq!(members(&reader), [5]);
}

#[test]
fn search_scope_epub_no_outline_keeps_all_spines() {
    let fixture = Fixture::new();
    let path = fixture.root.join("Coastal reports without headings.epub");
    authored_epub(&path, false);
    let mut reader = open(&fixture, &path);
    reader.start_document_search("marker").unwrap();
    wait_for_search(&mut reader, usize::MAX);
    assert_eq!(reader.hits.len(), 6);
    assert_eq!(members(&reader), [0, 1, 2, 3, 4, 5]);
}
