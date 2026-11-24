//! Optional functional checks against a caller-supplied, read-only original and an isolated profile.
use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::tests::{Fixture, tap_contents_item};
use super::*;
use primitive_app::App;

fn open_external(variable: &str) -> (Fixture, Reader, PathBuf) {
    let file = PathBuf::from(std::env::var_os(variable).expect("Supply a local document"));
    let fixture = Fixture::new();
    let font = crate::test_fixture("fonts/NotoSans-Regular.ttf");
    let mut reader =
        Reader::open(file.parent().unwrap(), &fixture.state, vec![font.clone()]).unwrap();
    reader.open_path(&file).unwrap();
    (fixture, reader, font)
}

fn one_word(probe: &crate::Probe, last: bool) -> Option<TextSelection> {
    let mut words = probe.source.blocks.iter().flat_map(|block| {
        block
            .text
            .split_whitespace()
            .filter(|s| s.chars().any(char::is_alphanumeric))
            .map(|s| {
                let start = s.as_ptr() as usize - block.text.as_ptr() as usize;
                (block.key, start, start + s.len())
            })
    });
    let (block, start, end) = if last {
        words.next_back()?
    } else {
        words.next()?
    };
    probe.select_range(&probe.anchor(block, start), &probe.anchor(block, end))
}

fn during_edit(
    reader: &mut Reader,
    font: &std::path::Path,
    navigate_and_select: impl Fn(&mut Reader),
    verify: impl Fn(&Reader),
) {
    let mut canvas = Canvas::new(WIDTH, HEIGHT, &[font.display().to_string()]).unwrap();
    let history = reader.back.clone();
    let position = reader.current_history_location().unwrap();
    for save in [false, true] {
        reader.activate(Action::ManageHighlight(0)).unwrap();
        let detail = primitive_app::frame(reader, &mut canvas, false);
        tap_contents_item(reader, "Extend highlight");
        navigate_and_select(reader);
        verify(reader);
        if save {
            reader.input(Input::Tap(1000.0, HEIGHT as f32 - 43.0));
            assert!(reader.range_edit.is_none());
            reader.save().unwrap();
        } else {
            reader.input(Input::Tap(125.0, HEIGHT as f32 - 43.0));
            assert!(reader.range_edit.is_none() && reader.screen == Screen::Annotation);
            assert!(reader.current_history_location().as_ref() == Some(&position));
            assert!(reader.back == history);
            assert!(primitive_app::frame(reader, &mut canvas, false) == detail);
        }
    }
}

#[test]
#[ignore = "Supply READER_TEST_RANGE_EPUB and temporary writable state"]
fn extends_saved_range_on_an_original_epub() {
    let (fixture, mut reader, font) = open_external("READER_TEST_RANGE_EPUB");
    let chapter = std::env::var("READER_TEST_RANGE_CHAPTER")
        .unwrap_or_else(|_| "1".into())
        .parse::<usize>()
        .unwrap();
    let active = reader.active.as_ref().unwrap();
    let id = active.id;
    let first = active.reading.config.chapter(chapter).unwrap();
    let last = active.reading.config.chapter(chapter + 1).unwrap();
    let original = one_word(&first, true).unwrap();
    let other = one_word(&last, false).unwrap();
    assert!(first.page_for(&original.start).is_some() && last.page_for(&other.start).is_some());
    drop((first, last));
    reader.selection = Some(original.clone());
    reader
        .save_highlight(Some("Range validation".into()))
        .unwrap();
    during_edit(
        &mut reader,
        &font,
        |reader| {
            for _ in 0..16 {
                let active = reader.active.as_ref().unwrap();
                if active.reading.chapter == chapter + 1
                    && !active.reading.highlight_rects(&other).is_empty()
                {
                    break;
                }
                reader.input(Input::PageForward);
                reader.tick(Instant::now() + Duration::from_secs(2));
            }
            let reading = &reader.active.as_ref().unwrap().reading;
            assert_eq!(reading.chapter, chapter + 1);
            let r = reading.highlight_rects(&other)[0];
            reader.input(Input::Tap(
                (r.x + r.width / 2) as f32,
                (r.y + r.height / 2) as f32,
            ));
        },
        |reader| {
            let Some(extend_highlight::RangeEdit {
                draft: extend_highlight::Draft::Text { current, .. },
                ..
            }) = &reader.range_edit
            else {
                panic!("no draft")
            };
            assert!(
                current.start == original.start
                    && current.end.chapter == other.end.chapter
                    && current.text.len() > original.text.len()
            );
        },
    );
    let reopened = Library::open(&fixture.root, &fixture.state).unwrap();
    let marks = &reopened.state(id).unwrap().highlights;
    assert!(marks.len() == 1 && marks[0].note.as_deref() == Some("Range validation"));
    assert!(marks[0].selection.start.chapter != marks[0].selection.end.chapter);
    eprintln!(
        "range verification EPUB — zero-based spine {chapter} through {}, two edit cycles",
        chapter + 1
    );
}

#[test]
#[ignore = "Supply READER_TEST_RANGE_PDF and temporary writable state"]
fn extends_saved_range_on_an_original_pdf() {
    let (fixture, mut reader, font) = open_external("READER_TEST_RANGE_PDF");
    let page = std::env::var("READER_TEST_RANGE_PAGE")
        .unwrap_or_else(|_| "1".into())
        .parse::<usize>()
        .unwrap();
    let fixed = reader.fixed.as_ref().unwrap();
    let id = fixed.id;
    let first = fixed.reading.page_text(page).unwrap();
    let end = fixed.reading.page_text(page + 1).unwrap();
    let start = first
        .words
        .last()
        .expect("Select adjacent pages with extractable words");
    let word = end
        .words
        .first()
        .expect("Select adjacent pages with extractable words");
    let initial = PdfHighlight {
        chapter_title: None,
        color: Default::default(),
        created_at: None,
        style: Default::default(),
        page,
        bounds: vec![start.bounds],
        text: first.text[start.bytes.clone()].into(),
        note: Some("Range validation".into()),
        continuation: vec![],
    };
    reader
        .library
        .remember(&fixed.entry, id, &fixed.entry.title)
        .add_pdf_highlight(initial.clone())
        .unwrap();
    during_edit(
        &mut reader,
        &font,
        |reader| {
            reader.input(Input::PageForward);
            reader.tick(Instant::now() + Duration::from_secs(2));
            let fixed = &reader.fixed.as_ref().unwrap().reading;
            let p = fixed
                .visible_rectangle(page + 1, word.bounds)
                .expect("Word is visible in the fitted page")
                .center();
            reader.input(Input::Tap(p.x as f32, p.y as f32));
        },
        |reader| {
            let Some(extend_highlight::RangeEdit {
                draft: extend_highlight::Draft::Fixed { current, .. },
                ..
            }) = &reader.range_edit
            else {
                panic!("no draft")
            };
            assert!(
                current.page == page
                    && current.continuation.len() == 1
                    && current.continuation[0].page == page + 1
            );
            assert!(
                current.text.starts_with(&initial.text)
                    && current.text.ends_with(&end.text[word.bytes.clone()])
            );
        },
    );
    let reopened = Library::open(&fixture.root, &fixture.state).unwrap();
    let marks = &reopened.state(id).unwrap().pdf_highlights;
    assert!(
        marks.len() == 1
            && marks[0].note.as_deref() == Some("Range validation")
            && marks[0].parts().count() == 2
    );
    eprintln!(
        "range verification PDF — zero-based page {page} through {}, two edit cycles",
        page + 1
    );
}
