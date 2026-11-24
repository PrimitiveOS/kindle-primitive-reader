use super::*;
use crate::app::tests::{Fixture, capture_test_screen, write_outline_fixture};
use primitive_app::App;
use primitive_ui::Key;
use std::{fmt::Write, fs};

fn center(rect: Rect) -> Input {
    Input::Tap((rect.x + rect.w / 2) as f32, (rect.y + rect.h / 2) as f32)
}

fn key(app: &mut Reader, key: Key) -> Change {
    let layout = Layout::new(WIDTH, HEIGHT, 4, 5);
    let rect = layout
        .keyboard_keys_with(app.editor.layer, "Find")
        .into_iter()
        .find(|(definition, _)| definition.key == key)
        .unwrap()
        .1;
    app.input(center(rect))
}

fn branch(app: &mut Reader, title: &str) -> Change {
    let rows = app.contents_rows();
    let (_, visible) = app.contents_window(app.list_page * rows, rows);
    let (offset, line) = visible
        .iter()
        .enumerate()
        .find(|(_, line)| line.item.title == title)
        .unwrap_or_else(|| panic!("Missing branch: {title}"));
    let target = branch_rect(app.contents_row_rect(offset), line.heading.unwrap()).unwrap();
    app.input(Input::Tap(
        (target.x + 5) as f32,
        (target.y + target.h / 2) as f32,
    ))
}

fn heading(app: &mut Reader, title: &str) -> Change {
    let rows = app.contents_rows();
    let (_, visible) = app.contents_window(app.list_page * rows, rows);
    let (offset, line) = visible
        .iter()
        .enumerate()
        .find(|(_, line)| line.item.title == title)
        .unwrap_or_else(|| panic!("Missing heading: {title}"));
    let row = app.contents_row_rect(offset);
    let branch = line.heading.and_then(|heading| branch_rect(row, heading));
    app.input(center(heading_body(row, branch)))
}

fn app(fixture: &Fixture) -> Reader {
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

#[test]
fn contents_expansion_search_clear_and_page_keys_keep_the_reading_anchor() {
    let fixture = Fixture::new();
    let mut nav = String::from(
        r##"<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><body><nav epub:type="toc"><ol>
        <li><a href="first.xhtml">Part A</a><ol>
          <li><a href="first.xhtml#passage12">Chapter 12</a><ol>
            <li><a href="first.xhtml#passage12">Study notes</a></li>
            <li><a href="first.xhtml#passage13">Other material</a></li>
          </ol></li>
        </ol></li>
        <li><a href="second.xhtml">Part B</a><ol>
          <li><a href="second.xhtml">Study notes appendix</a></li>
        </ol></li>"##,
    );
    for entry in 0..14 {
        write!(
            nav,
            "<li><a href=\"second.xhtml\">Appendix {entry:02}</a></li>"
        )
        .unwrap();
    }
    nav.push_str("</ol></nav></body></html>");
    write_outline_fixture(&fixture.book, Some(nav.as_bytes()), None);
    let mut app = app(&fixture);
    app.open_path(&fixture.book).unwrap();
    app.active
        .as_mut()
        .unwrap()
        .reading
        .jump(0, Some("passage12"))
        .unwrap();
    let start = app.current_position().unwrap();
    let previous_jumps = app.back.len();
    app.show(Screen::Contents);
    assert_eq!(app.list_page, 0);
    assert_eq!(
        app.items()[app.current_contents_row.unwrap()].title,
        "Chapter 12"
    );
    assert!(app.items().iter().any(|item| item.title == "Study notes"));
    assert!(
        !app.items()
            .iter()
            .any(|item| item.title == "Study notes appendix")
    );
    capture_test_screen(&app, &app.fonts, "READER_CONTENTS_EXPANDED_SCREEN", false);

    assert_eq!(branch(&mut app, "Part A"), Change::Partial);
    assert!(!app.items().iter().any(|item| item.title == "Study notes"));
    capture_test_screen(&app, &app.fonts, "READER_CONTENTS_COLLAPSED_SCREEN", false);
    assert_eq!(app.input(Input::PageForward), Change::Partial);
    assert_eq!(app.list_page, 1);
    let search_edge = Input::Tap((SEARCH.x + 20) as f32, (SEARCH.y - 5) as f32);
    assert_eq!(app.input(search_edge), Change::Partial);
    assert_eq!(app.screen, Screen::ContentsSearch);
    for character in "study".chars() {
        assert_eq!(key(&mut app, Key::Char(character)), Change::Partial);
    }
    assert_eq!(key(&mut app, Key::Go), Change::Partial);
    assert_eq!(app.screen, Screen::Contents);
    assert_eq!(app.list_page, 0);
    assert_eq!(
        app.items()
            .into_iter()
            .map(|item| item.title)
            .collect::<Vec<_>>(),
        [
            "Generated outline",
            "Custom contents",
            "Part A",
            "Chapter 12",
            "Study notes",
            "Part B",
            "Study notes appendix",
        ]
    );
    capture_test_screen(&app, &app.fonts, "READER_CONTENTS_FILTERED_SCREEN", false);
    capture_test_screen(&app, &app.fonts, "READER_CONTENTS_DARK_SCREEN", true);

    assert_eq!(app.input(center(SEARCH)), Change::Partial);
    assert_eq!(app.editor.string(), "study");
    assert_eq!(app.input(Input::PageBack), Change::Partial);
    assert_eq!(app.screen, Screen::Contents);
    assert_eq!(
        app.contents_view.as_ref().unwrap().tree.query(),
        Some("study")
    );
    assert_eq!(app.input(center(SEARCH)), Change::Partial);
    app.editor.text = "missing title".chars().collect();
    app.editor.caret = app.editor.text.len();
    assert_eq!(key(&mut app, Key::Go), Change::Partial);
    assert_eq!(app.notice.as_deref(), Some("No matching headings"));
    assert_eq!(
        app.contents_view.as_ref().unwrap().tree.query(),
        Some("study")
    );
    assert_eq!(app.input(center(CLEAR)), Change::Partial);
    assert!(app.contents_view.as_ref().unwrap().tree.query().is_none());
    assert!(
        app.items()
            .iter()
            .any(|item| item.title == "Other material")
    );
    let next_edge = Input::Tap((NEXT.x + 20) as f32, (NEXT.y - 5) as f32);
    assert_eq!(app.input(next_edge), Change::Partial);
    assert_eq!(app.input(Input::PageBack), Change::Partial);
    assert_eq!(app.list_page, 0);
    assert_eq!(app.current_position(), Some(start.clone()));
    assert_eq!(app.back.len(), previous_jumps);
    assert!(app.forward.is_empty());

    // The selected row still uses its original XHTML fragment.
    let Action::Jump(destination) = app
        .items()
        .into_iter()
        .find(|item| item.title == "Other material")
        .unwrap()
        .action
    else {
        panic!("Expected an EPUB fragment destination");
    };
    assert_eq!(destination.fragment.as_deref(), Some("passage13"));
    assert_eq!(heading(&mut app, "Other material"), Change::Partial);
    assert_eq!(app.screen, Screen::Book);
    assert_ne!(app.current_position(), Some(start.clone()));
    assert_eq!(app.back.len(), previous_jumps + 1);
    app.activate(Action::ReturnToLocation).unwrap();
    assert_eq!(app.current_position(), Some(start));
}

#[test]
fn fixed_contents_branch_toggles_and_search_keep_the_pdf_page_until_a_jump() {
    let fixture = Fixture::new();
    let path = fixture.root.join("Nested chart.pdf");
    fs::write(
        &path,
        crate::fixed::tests::pdf_objects(&[
            "<< /Type /Catalog /Pages 2 0 R /Outlines 6 0 R >>",
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
    let mut app = app(&fixture);
    app.open_path(&path).unwrap();
    let start = app.current_position().unwrap();
    app.controls.contents_density.rows_per_page = Some(24);
    app.show(Screen::FixedContents);
    assert!(!app.items().iter().any(|item| item.title == "Chart detail"));
    assert_eq!(branch(&mut app, "Part A"), Change::Partial);
    assert!(app.items().iter().any(|item| item.title == "Chart detail"));
    assert_eq!(app.input(center(SEARCH)), Change::Partial);
    for character in "detail".chars() {
        assert_eq!(key(&mut app, Key::Char(character)), Change::Partial);
    }
    assert_eq!(key(&mut app, Key::Go), Change::Partial);
    assert_eq!(app.screen, Screen::FixedContents);
    assert!(!app.items().iter().any(|item| item.title == "Appendix"));
    assert_eq!(app.current_position(), Some(start.clone()));
    assert!(app.back.is_empty());
    assert_eq!(heading(&mut app, "Chart detail"), Change::Partial);
    assert_eq!(app.screen, Screen::Fixed);
    assert_eq!(app.fixed.as_ref().unwrap().reading.page(), 1);
    assert_eq!(app.back.len(), 1);
    app.activate(Action::ReturnToLocation).unwrap();
    assert_eq!(app.current_position(), Some(start));
}
