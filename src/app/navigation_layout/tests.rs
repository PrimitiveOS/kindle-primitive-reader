use super::{NavigationLayout, Rect};
use crate::app::tests::{Fixture, write_outline_fixture};
use crate::app::*;
use crate::library::{Bookmark, ReadingPosition};
use crate::session::ReadingViewport;
use primitive_app::{App, frame, write_png};
use primitive_ui::Key;
use std::fmt::Write as _;

fn open(fixture: &Fixture, path: &Path) -> Reader {
    let mut app = Reader::open(
        &fixture.root,
        &fixture.state,
        vec![
            crate::test_fixture("fonts/NotoSans-Regular.ttf"),
            crate::test_fixture("fonts/NotoSans-Bold.ttf"),
        ],
    )
    .unwrap();
    app.open_path(path).unwrap();
    app
}

fn prepare(app: &mut Reader, viewport: ReadingViewport) {
    if let Some(fixed) = &mut app.fixed {
        let candidate = fixed.reading.prepare_viewport(viewport).unwrap();
        fixed.reading.commit_viewport(candidate).unwrap();
    } else {
        let reading = &mut app.active.as_mut().unwrap().reading;
        *reading = reading.prepare_viewport(viewport).unwrap();
    }
    assert_eq!(app.reading_viewport(), viewport);
}

fn canvas(app: &Reader) -> Canvas {
    let (w, h) = app.reading_viewport().dimensions();
    let fonts: Vec<_> = app.fonts.iter().map(|p| p.display().to_string()).collect();
    Canvas::new(w, h, &fonts).unwrap()
}

fn capture(app: &Reader, setting: &str, dark: bool) {
    if let Some(output) = std::env::var_os(setting) {
        let (width, height) = app.reading_viewport().dimensions();
        write_png(
            Path::new(&output),
            width,
            height,
            &frame(app, &mut canvas(app), dark),
        )
        .unwrap();
    }
}

fn center(rect: Rect) -> Input {
    Input::Tap((rect.x + rect.w / 2) as f32, (rect.y + rect.h / 2) as f32)
}

fn right_side(rect: Rect) -> Input {
    assert!(rect.right() > WIDTH as i32 + 30);
    Input::Tap((rect.right() - 24) as f32, (rect.y + rect.h / 2) as f32)
}

fn key(app: &Reader, key: Key, label: &str) -> Rect {
    app.navigation_layout()
        .keyboard()
        .keyboard_keys_with(app.editor.layer, label)
        .into_iter()
        .find(|(def, _)| def.key == key)
        .unwrap()
        .1
}

fn press_region(app: &Reader, rect: Rect) {
    let mut canvas = canvas(app);
    let before = frame(app, &mut canvas, false);
    let Input::Tap(x, y) = center(rect) else {
        unreachable!()
    };
    assert!(canvas.press_at(x, y), "unregistered control {rect:?}");
    let after = frame(app, &mut canvas, false);
    let width = canvas.bounds().w as usize;
    assert!(
        before
            .iter()
            .zip(&after)
            .enumerate()
            .any(|(i, (a, b))| a != b && rect.contains((i % width) as f32, (i / width) as f32)),
        "control rendered no press feedback at {rect:?}"
    );
}

fn make_epub(path: &Path) {
    let mut nav = String::from(
        r#"<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><body><nav epub:type="toc"><ol>"#,
    );
    for i in 1..=14 {
        write!(
            &mut nav,
            r#"<li><a href="first.xhtml#passage{i}">Survey passage {i:02}</a></li>"#
        )
        .unwrap();
    }
    nav.push_str("</ol></nav></body></html>");
    write_outline_fixture(path, Some(nav.as_bytes()), None);
}

fn make_pdf(path: &Path) {
    let count = 14;
    let font = 3 + count * 2;
    let outline = font + 1;
    let kids = (0..count)
        .map(|i| format!("{} 0 R", 3 + i * 2))
        .collect::<Vec<_>>()
        .join(" ");
    let mut objects = vec![
        format!("<< /Type /Catalog /Pages 2 0 R /Outlines {outline} 0 R >>"),
        format!("<< /Type /Pages /Count {count} /Kids [{kids}] >>"),
    ];
    for i in 0..count {
        let stream = format!(
            "BT /F1 20 Tf 44 570 Td (Coastal survey, page {}) Tj ET",
            i + 1
        );
        objects.push(format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 440 620] /Resources << /Font << /F1 {font} 0 R >> >> /Contents {} 0 R >>", 4 + i * 2));
        objects.push(format!(
            "<< /Length {} >>\nstream\n{stream}\nendstream",
            stream.len()
        ));
    }
    objects.push("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into());
    objects.push(format!(
        "<< /Type /Outlines /First {} 0 R /Last {} 0 R /Count {count} >>",
        outline + 1,
        outline + count
    ));
    for i in 0..count {
        let next = if i + 1 < count {
            format!("/Next {} 0 R", outline + i + 2)
        } else {
            String::new()
        };
        objects.push(format!(
            "<< /Title (Survey sheet {:02}) /Parent {outline} 0 R /Dest [{} 0 R /Fit] {next} >>",
            i + 1,
            3 + i * 2
        ));
    }
    let refs: Vec<_> = objects.iter().map(String::as_str).collect();
    fs::write(path, crate::fixed::tests::pdf_objects(&refs)).unwrap();
}

#[test]
fn navigation_viewport_epub_contents_search_and_return_use_landscape_regions() {
    let f = Fixture::new();
    make_epub(&f.book);
    let mut app = open(&f, &f.book);
    app.show(Screen::Contents);
    let original = frame(&app, &mut canvas(&app), false);
    let portrait_rows = app.contents_rows();
    assert_eq!(
        app.navigation_layout().back(),
        primitive_ui::back_button(WIDTH as i32)
    );
    prepare(&mut app, ReadingViewport::LANDSCAPE);
    app.show(Screen::Contents);
    let origin = app.current_history_location();
    let landscape = app.navigation_layout();
    let rows = app.contents_rows();
    assert!(rows < portrait_rows);
    assert!(app.contents_row_rect(rows - 1).bottom() <= landscape.height - 250);
    assert!(app.contents_row_rect(0).right() > WIDTH as i32);
    press_region(&app, landscape.back());
    press_region(&app, landscape.next());
    capture(&app, "READER_NAV_LANDSCAPE_CONTENTS", false);
    assert_eq!(app.input(right_side(landscape.next())), Change::Partial);
    assert_eq!(app.list_page, 1);
    assert_eq!(app.input(Input::PageBack), Change::Partial);
    let search = landscape.centered_footer(Rect::new(338, HEIGHT as i32 - 104, 276, 72));
    assert_eq!(app.input(center(search)), Change::Partial);
    assert_eq!(app.screen, Screen::ContentsSearch);
    for ch in "passage 14".chars() {
        let key = key(
            &app,
            if ch == ' ' { Key::Space } else { Key::Char(ch) },
            "Find",
        );
        assert_eq!(app.input(center(key)), Change::Partial);
    }
    let find = key(&app, Key::Go, "Find");
    assert_eq!(app.input(right_side(find)), Change::Partial);
    assert_eq!(app.screen, Screen::Contents);
    let offset = app
        .items()
        .iter()
        .position(|i| i.title == "Survey passage 14")
        .unwrap();
    press_region(&app, app.contents_row_rect(offset));
    assert_eq!(
        app.input(right_side(app.contents_row_rect(offset))),
        Change::Partial
    );
    assert_eq!(app.screen, Screen::Book);
    assert_ne!(app.current_history_location(), origin);
    app.activate(Action::ReturnToLocation).unwrap();
    assert_eq!(app.current_history_location(), origin);
    app.show(Screen::Contents);
    let clear = landscape.centered_footer(Rect::new(653, HEIGHT as i32 - 104, 276, 72));
    assert_eq!(app.input(center(clear)), Change::Partial);
    prepare(&mut app, ReadingViewport::PORTRAIT);
    app.show(Screen::Contents);
    assert!(frame(&app, &mut canvas(&app), false) == original);
}

#[test]
fn navigation_viewport_pdf_keys_outline_marks_and_back_follow_landscape() {
    let f = Fixture::new();
    let path = f.root.join("Coastal survey.pdf");
    make_pdf(&path);
    let mut app = open(&f, &path);
    let id = app.open_book_id().unwrap();
    let state = app.library.state_mut(id).unwrap();
    for page in 0..14 {
        state.bookmarks.push(Bookmark {
            label: format!(
                "Field note {:02} · Observe tide marks along the outer harbor",
                page + 1
            ),
            position: ReadingPosition {
                chapter: 0,
                page,
                anchor: None,
            },
            created_at: None,
        });
    }
    app.show(Screen::GoToPage);
    let portrait = frame(&app, &mut canvas(&app), false);
    prepare(&mut app, ReadingViewport::LANDSCAPE);
    app.show(Screen::GoToPage);
    let layout = app.navigation_layout();
    let go = key(&app, Key::Go, "Go");
    assert!(go.y < HEIGHT as i32 - 416);
    press_region(&app, key(&app, Key::Char('1'), "Go"));
    press_region(&app, layout.back());
    assert_eq!(app.input(right_side(layout.back())), Change::Partial);
    assert_eq!(app.screen, Screen::FixedTools);
    app.show(Screen::GoToPage);
    capture(&app, "READER_NAV_LANDSCAPE_GOTO", false);
    for digit in ['1', '2'] {
        let rect = key(&app, Key::Char(digit), "Go");
        assert_eq!(app.input(center(rect)), Change::Partial);
    }
    assert_eq!(app.input(right_side(go)), Change::Partial);
    assert_eq!(app.current_position().unwrap().page, 11);
    assert!(
        app.fixed
            .as_ref()
            .unwrap()
            .reading
            .page_text(11)
            .unwrap()
            .text
            .contains("Coastal survey")
    );
    app.activate(Action::ReturnToLocation).unwrap();
    assert_eq!(app.current_position().unwrap().page, 0);

    app.show(Screen::FixedContents);
    assert!(
        app.items()
            .iter()
            .any(|item| item.title == "Survey sheet 14")
    );
    assert_eq!(app.input(right_side(layout.next())), Change::Partial);
    assert_eq!(app.input(right_side(layout.back())), Change::Partial);
    assert_eq!(app.screen, Screen::FixedTools);

    app.show(Screen::FixedBookmarks);
    let geometry = app.list_geometry();
    let target = app
        .items()
        .iter()
        .position(|i| i.title.starts_with("Field note 02"))
        .unwrap();
    let row = geometry.row(target);
    assert!(row.bottom() < layout.next().y);
    assert_eq!(app.input(right_side(layout.next())), Change::Partial);
    assert_eq!(app.input(Input::PageBack), Change::Partial);
    press_region(&app, row);
    assert_eq!(app.input(right_side(row)), Change::Partial);
    assert_eq!(app.current_position().unwrap().page, 1);
    app.activate(Action::ReturnToLocation).unwrap();

    app.controls.bookmark_max_lines = Some(4);
    app.show(Screen::FixedBookmarks);
    assert!(app.flexible_marks.is_some());
    press_region(&app, layout.next());
    capture(&app, "READER_NAV_LANDSCAPE_MARKS_DARK", true);
    assert_eq!(app.input(right_side(layout.next())), Change::Partial);
    assert_eq!(app.input(right_side(layout.back())), Change::Partial);
    assert_eq!(app.screen, Screen::FixedTools);

    app.controls.bookmark_max_lines = None;
    app.show(Screen::FixedBookmarks);
    let find = app.list_geometry().row(0);
    assert_eq!(app.input(right_side(find)), Change::Partial);
    assert_eq!(app.screen, Screen::BookmarkFind);
    for digit in ['1', '4'] {
        let rect = key(&app, Key::Char(digit), "Find");
        assert_eq!(app.input(center(rect)), Change::Partial);
    }
    let find = key(&app, Key::Go, "Find");
    assert_eq!(app.input(right_side(find)), Change::Partial);
    assert_eq!(app.screen, Screen::BookmarkMatches);
    let items = app.items();
    assert_eq!(items.len(), 1);
    assert!(items[0].title.starts_with("Field note 14"));
    let item = app.list_geometry().row(0);
    assert_eq!(app.input(right_side(item)), Change::Partial);
    assert_eq!(app.current_position().unwrap().page, 13);
    app.activate(Action::ReturnToLocation).unwrap();
    assert_eq!(app.current_position().unwrap().page, 0);

    prepare(&mut app, ReadingViewport::PORTRAIT);
    app.show(Screen::GoToPage);
    assert!(frame(&app, &mut canvas(&app), false) == portrait);
}

#[test]
fn navigation_viewport_portrait_header_and_pager_keep_shared_positions_and_pixels() {
    let f = Fixture::new();
    let app = open(&f, &f.book);
    let mut standard = canvas(&app);
    let mut scoped = canvas(&app);
    primitive_ui::header(&mut standard, "Bookmarks", "Back");
    NavigationLayout::PORTRAIT.header(&mut scoped, "Bookmarks");
    assert_eq!(standard.finish(false), scoped.finish(false));
    assert_eq!(NavigationLayout::PORTRAIT.previous(), PREV);
    assert_eq!(NavigationLayout::PORTRAIT.next(), NEXT);
}
