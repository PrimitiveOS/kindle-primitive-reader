use std::fs;
use std::time::Instant;

use primitive_app::{App, frame};

use super::*;
use crate::app::WIDTH;
use crate::app::contents_position::ContentsNumbers;
use crate::app::tests::{Fixture, capture_test_screen, tap_contents_item, write_outline_fixture};
use crate::reading_controls::{ListDensity, ReadingControls};

fn reader(fixture: &Fixture) -> Reader {
    Reader::open(
        &fixture.root,
        &fixture.state,
        vec![
            crate::test_fixture("fonts/NotoSans-Regular.ttf"),
            crate::test_fixture("fonts/NotoSans-Bold.ttf"),
        ],
    )
    .unwrap()
}

fn numbers<'a>(app: &'a Reader, title: &str) -> &'a ContentsNumbers {
    let view = app.contents_view.as_ref().unwrap();
    let index = view
        .entries
        .iter()
        .position(|entry| entry.title == title)
        .unwrap();
    view.positions.as_ref().unwrap().numbers(index).unwrap()
}

fn tap(app: &mut Reader, rect: Rect) -> Change {
    app.input(Input::Tap(
        (rect.x + rect.w / 2) as f32,
        (rect.y + rect.h / 2) as f32,
    ))
}

#[test]
fn fixed_contents_shows_print_labels_and_depth_lengths_without_changing_jumps() {
    let fixture = Fixture::new();
    let path = fixture.root.join("Nested chart.pdf");
    fs::write(
        &path,
        crate::fixed::tests::pdf_objects(&[
            "<< /Type /Catalog /Pages 2 0 R /Outlines 6 0 R /PageLabels << /Nums [0 << /S /r >> 2 << /S /D /St 10 >>] >> >>",
            "<< /Type /Pages /Kids [3 0 R 4 0 R 5 0 R] /Count 3 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 180 200] >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 180 200] >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 180 200] >>",
            "<< /Type /Outlines /First 7 0 R /Last 9 0 R /Count 3 >>",
            "<< /Title (Part A) /Dest [3 0 R /Fit] /Parent 6 0 R /First 8 0 R /Last 8 0 R /Count 1 /Next 9 0 R >>",
            "<< /Title (Chart detail) /Dest [4 0 R /Fit] /Parent 7 0 R >>",
            "<< /Title (Appendix) /Dest [5 0 R /Fit] /Parent 6 0 R /Prev 7 0 R >>",
        ]),
    )
    .unwrap();
    let mut app = reader(&fixture);
    app.open_path(&path).unwrap();
    let origin = app.current_position();
    app.show(Screen::FixedContents);
    let [lengths, dots] = app.contents_metadata_settings_items();
    assert_eq!(lengths.detail.as_deref(), Some("Off"));
    assert_eq!(dots.detail.as_deref(), Some("On"));

    // The parent's complete length also includes its collapsed child.
    for (title, label, length) in [
        ("Part A", "i", 2),
        ("Chart detail", "ii", 1),
        ("Appendix", "10", 1),
    ] {
        assert_eq!(
            numbers(&app, title),
            &ContentsNumbers {
                page_label: Some(label.into()),
                chapter_length: Some(length)
            }
        );
    }
    app.activate(lengths.action).unwrap();
    let (part_a, _) = app.contents_branch_button(1).unwrap();
    assert_eq!(tap(&mut app, part_a), Change::Partial);
    let fonts: Vec<_> = app
        .fonts
        .iter()
        .map(|path| path.display().to_string())
        .collect();
    let with_dots = frame(
        &app,
        &mut Canvas::new(WIDTH, HEIGHT, &fonts).unwrap(),
        false,
    );
    capture_test_screen(
        &app,
        &app.fonts,
        "READER_CONTENTS_NUMBERED_PDF_SCREEN",
        false,
    );

    let [_, dots] = app.contents_metadata_settings_items();
    app.activate(dots.action).unwrap();
    let without_dots = frame(
        &app,
        &mut Canvas::new(WIDTH, HEIGHT, &fonts).unwrap(),
        false,
    );
    assert_ne!(with_dots, without_dots);
    assert_eq!(app.current_position(), origin);
    assert!(app.back.is_empty());

    let detail = app
        .items()
        .into_iter()
        .find(|row| row.title == "Chart detail")
        .unwrap();
    assert!(matches!(detail.action, Action::FixedPage(1)));
    app.activate(detail.action).unwrap();
    assert_eq!(app.fixed.as_ref().unwrap().reading.page(), 1);
    app.activate(Action::ReturnToLocation).unwrap();
    assert_eq!(app.current_position(), origin);

    let reopened = reader(&fixture);
    let [lengths, dots] = reopened.contents_metadata_settings_items();
    assert_eq!(lengths.detail.as_deref(), Some("On"));
    assert_eq!(dots.detail.as_deref(), Some("Off"));
}

#[test]
fn reflow_contents_uses_existing_layout_and_exact_publisher_targets() {
    let fixture = Fixture::new();
    let nav = br##"<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><body>
        <nav epub:type="toc"><ol>
          <li><a href="first.xhtml#passage1">Part A</a><ol>
            <li><a href="first.xhtml#passage10">Study notes</a></li>
          </ol></li>
          <li><a href="second.xhtml">Part B</a><ol>
            <li><a href="second.xhtml#unlaid">Unlaid notes</a></li>
          </ol></li>
        </ol></nav>
        <nav epub:type="page-list"><ol>
          <li><a href="first.xhtml#passage1">iv</a></li>
          <li><a href="first.xhtml#passage7">7a</a></li>
          <li><a href="second.xhtml">11</a></li>
        </ol></nav></body></html>"##;
    write_outline_fixture(&fixture.book, Some(nav), None);
    let mut app = reader(&fixture);
    app.open_path(&fixture.book).unwrap();
    let origin = app.current_position();
    let reading = &app.active.as_ref().unwrap().reading;
    let probe = &reading.probe;
    let start = probe
        .page_for(&probe.fragment("passage1").unwrap())
        .unwrap();
    let note = probe
        .page_for(&probe.fragment("passage10").unwrap())
        .unwrap();
    let local_end = probe.page_count();
    app.show(Screen::Contents);
    assert_eq!(
        numbers(&app, "Part A"),
        &ContentsNumbers {
            page_label: Some(format!("1:{}", start + 1)),
            chapter_length: Some(local_end - start),
        }
    );
    assert_eq!(
        numbers(&app, "Study notes"),
        &ContentsNumbers {
            page_label: Some(format!("1:{}", note + 1)),
            chapter_length: Some(local_end - note),
        }
    );
    assert_eq!(numbers(&app, "Part B").page_label.as_deref(), Some("2:1"));
    assert_eq!(numbers(&app, "Part B").chapter_length, None);
    assert_eq!(numbers(&app, "Unlaid notes"), &ContentsNumbers::default());

    app.toggle_publisher_labels(false).unwrap();
    let [lengths, _] = app.contents_metadata_settings_items();
    app.activate(lengths.action).unwrap();
    app.show(Screen::Contents);
    for (title, label, length) in [
        ("Part A", "iv", 2),
        ("Study notes", "7a", 1),
        ("Part B", "11", 1),
    ] {
        assert_eq!(
            numbers(&app, title),
            &ContentsNumbers {
                page_label: Some(label.into()),
                chapter_length: Some(length)
            }
        );
    }
    capture_test_screen(
        &app,
        &app.fonts,
        "READER_CONTENTS_NUMBERED_EPUB_SCREEN",
        false,
    );
    capture_test_screen(
        &app,
        &app.fonts,
        "READER_CONTENTS_NUMBERED_DARK_SCREEN",
        true,
    );
    assert_eq!(app.current_position(), origin);
    assert!(app.back.is_empty());

    let row = app
        .items()
        .into_iter()
        .find(|item| item.title == "Part B")
        .unwrap();
    let Action::Jump(target) = &row.action else {
        panic!("lost reflow destination");
    };
    assert!(target.path.ends_with("second.xhtml"));
    assert_eq!(target.fragment, None);
    app.activate(row.action).unwrap();
    assert_eq!(app.current_position().unwrap().chapter, 1);
    app.activate(Action::ReturnToLocation).unwrap();
    assert_eq!(app.current_position(), origin);
}

#[test]
fn dense_contents_keeps_stable_labels_lengths_and_actions_visible() {
    let fixture = Fixture::new();
    fs::copy(crate::test_fixture("many-chapters.epub"), &fixture.book).unwrap();
    let mut app = reader(&fixture);
    app.open_path(&fixture.book).unwrap();
    let origin = app.current_position();
    let chapter_count = app.active.as_ref().unwrap().reading.probe.chapter_count();
    app.activate(Action::SetCharacterPages(Some(500))).unwrap();
    for _ in 0..chapter_count {
        app.step_character_pages(Instant::now()).unwrap();
    }
    assert!(app.numbering_scan.is_none());
    let expected: Vec<_> = app
        .active
        .as_ref()
        .unwrap()
        .numbering
        .as_ref()
        .unwrap()
        .chapters
        .iter()
        .map(|chapter| ((chapter.first + 1).to_string(), chapter.points.len()))
        .collect();

    app.activate(Action::SetReadingControls(ReadingControls {
        contents_density: ListDensity {
            rows_per_page: Some(24),
            ..Default::default()
        },
        ..app.controls
    }))
    .unwrap();
    app.show(Screen::ContentsSettings);
    let items = app.items();
    assert_eq!(items.len(), 5);
    assert!(
        items
            .iter()
            .any(|item| matches!(item.action, Action::ToggleAlternativeContents))
    );
    tap_contents_item(&mut app, "Show chapter length");
    assert!(app.controls.contents_chapter_lengths);
    assert!(app.controls.contents_dot_leaders.unwrap_or(true));
    app.show(Screen::Contents);
    let (_, lines) = app.contents_window(0, 24);
    assert_eq!(lines.len(), 24);
    let headings: Vec<_> = lines.iter().filter(|line| line.heading.is_some()).collect();
    assert_eq!(headings.len(), 22);
    for (line, (label, length)) in headings.iter().zip(&expected) {
        let data = numbers(&app, &line.item.title);
        assert_eq!(data.page_label.as_ref(), Some(label));
        assert_eq!(data.chapter_length, Some(*length));
    }
    assert_eq!(app.current_position(), origin);
    capture_test_screen(
        &app,
        &app.fonts,
        "READER_CONTENTS_NUMBERED_DENSE_SCREEN",
        false,
    );
    let last_title = &headings.last().unwrap().item.title;
    tap_contents_item(&mut app, last_title);
    assert_eq!(app.screen, Screen::Book);
    assert_eq!(app.current_position().unwrap().chapter, 21);
    app.activate(Action::ReturnToLocation).unwrap();
    assert_eq!(app.current_position(), origin);
}
