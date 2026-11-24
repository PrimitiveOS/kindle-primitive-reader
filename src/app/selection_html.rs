//! Read a bounded markup fragment from the exact EPUB revision on screen.
use super::*;
use primitive_ui::{PageTurn, Pager, TextStyle};

const CODE_SIZE: i32 = 28;
const CODE: TextStyle = TextStyle::new(CODE_SIZE);
const LINE_HEIGHT: i32 = 43;
const MAX_LINE_CHARS: usize = 110;
const LINES: usize = 30;

pub(super) struct Inspector {
    source: String,
    lines: std::cell::OnceCell<Vec<String>>,
    page: usize,
}

impl Inspector {
    fn new(source: String) -> Self {
        Self {
            source,
            lines: std::cell::OnceCell::new(),
            page: 0,
        }
    }

    fn pager(&self) -> Pager {
        Pager {
            bounds: Rect::new(LIST_X, HEIGHT as i32 - 138, WIDTH as i32 - 2 * LIST_X, 100),
            page: self.page,
            pages: self
                .lines
                .get()
                .map_or(1, |lines| lines.len().div_ceil(LINES)),
        }
    }

    fn draw(&self, canvas: &mut Canvas) {
        let width = WIDTH as i32 - 2 * LIST_X;
        let lines = self
            .lines
            .get_or_init(|| layout_lines(canvas, &self.source, width));
        for (index, line) in lines.iter().skip(self.page * LINES).take(LINES).enumerate() {
            canvas.text(
                Rect::new(
                    LIST_X,
                    LIST_Y + index as i32 * LINE_HEIGHT,
                    width,
                    LINE_HEIGHT,
                ),
                line,
                CODE,
            );
        }
        self.pager().draw(canvas, None);
    }
}

fn layout_lines(canvas: &mut Canvas, source: &str, width: i32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut depth: usize = 0;
    // Formatting is visible only. The verified source stays unchanged in the inspector.
    for line in source.replace("><", ">\n<").lines() {
        let closing = line.starts_with("</");
        if closing {
            depth = depth.saturating_sub(1);
        }
        let indent = " ".repeat(depth.min(8) * 2);
        let line = format!("{indent}{line}");
        let mut rest = line.as_str();
        while !rest.is_empty() {
            // Never shape more than a small UTF-8 prefix in one measurement.
            let offsets: Vec<_> = rest
                .char_indices()
                .take(MAX_LINE_CHARS + 1)
                .map(|(index, _)| index)
                .collect();
            let end = offsets.get(MAX_LINE_CHARS).copied().unwrap_or(rest.len());
            let fit = if canvas.measure(&rest[..end], CODE_SIZE) <= width {
                end
            } else {
                let first =
                    offsets.partition_point(|&i| canvas.measure(&rest[..i], CODE_SIZE) <= width);
                offsets[first.saturating_sub(1).max(1).min(offsets.len() - 1)]
            };
            let fit = if fit == 0 { end } else { fit };
            lines.push(rest[..fit].to_owned());
            rest = &rest[fit..];
        }
        let tag = line.trim_start();
        if !closing
            && tag.starts_with('<')
            && !tag.starts_with("<!")
            && !tag.starts_with("<?")
            && tag.ends_with('>')
            && !tag.ends_with("/>")
            && !tag.contains("</")
        {
            depth += 1;
        }
    }
    lines
}

impl Reader {
    pub(super) fn can_view_selection_html(&self) -> bool {
        let Some(active) = &self.active else {
            return false;
        };
        active.entry.format == DocumentFormat::Epub
            && self
                .selection
                .as_ref()
                .is_some_and(|selection| active.reading.probe.can_inspect_publisher_html(selection))
    }

    pub(super) fn view_selection_html(&mut self) -> Result<()> {
        let unavailable = "Source HTML is unavailable for this selection";
        if !self.can_view_selection_html() {
            return Err(unavailable.into());
        }
        let active = self.active.as_ref().ok_or(unavailable)?;
        let selection = self.selection.as_ref().ok_or(unavailable)?;
        let probe = &active.reading.probe;
        let xhtml = with_storage(|| {
            if self.library.identify(&active.entry)? != active.id {
                return Err(io::Error::other("The book changed"));
            }
            let path = self.library.absolute_path(&active.entry.path)?;
            let mut book = Book::open(&path).map_err(io::Error::other)?;
            let found = book
                .chapter_index(&probe.chapter)
                .ok_or_else(|| io::Error::other(unavailable))?;
            if book.recovered_placeholder(&book.chapters[found]) {
                return Err(io::Error::other(unavailable));
            }
            book.chapter(found).map_err(io::Error::other)
        })
        .map_err(|_| unavailable)?;
        let html = probe
            .inspect_publisher_html(&xhtml, selection)
            .ok_or(unavailable)?;
        self.show(Screen::SelectionHtml);
        self.selection_html = Some(Inspector::new(html));
        Ok(())
    }

    pub(super) fn draw_selection_html(&self, canvas: &mut Canvas) {
        if let Some(inspector) = &self.selection_html {
            inspector.draw(canvas);
        }
    }

    pub(super) fn selection_html_input(&mut self, input: Input) -> Change {
        if matches!(input, Input::Tap(x,y) if primitive_ui::back_button(WIDTH as i32).contains(x,y))
        {
            self.show(Screen::Book);
            return Change::Full;
        }
        let Some(state) = &mut self.selection_html else {
            return Change::None;
        };
        let pager = state.pager();
        let direction = match input {
            Input::PageBack => Some(PageTurn::Previous),
            Input::PageForward => Some(PageTurn::Next),
            Input::Swipe { dy, dx, .. } if dy.abs() > dx.abs() => Some(if dy < 0.0 {
                PageTurn::Next
            } else {
                PageTurn::Previous
            }),
            Input::Tap(x, y) => pager.hit(x, y),
            _ => None,
        };
        let previous = state.page;
        match direction {
            Some(PageTurn::Previous) => state.page = state.page.saturating_sub(1),
            Some(PageTurn::Next) if state.page + 1 < pager.pages => state.page += 1,
            _ => {}
        }
        if state.page == previous {
            Change::None
        } else {
            Change::Partial
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::selection_popup::SelectionAction;
    use crate::app::tests::{Fixture, capture_test_screen};
    use std::io::Write;

    const XHTML: &str = include_str!("../../tests/fixtures/selection-html.xhtml");
    const CONTAINER: &str = r#"<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OPS/book.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#;
    const PACKAGE: &str = r#"<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="id">primitiveos-selection-html</dc:identifier><dc:title>Harbor notebook</dc:title><dc:language>en</dc:language></metadata><manifest><item id="book" href="chapter.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="book"/></spine></package>"#;

    fn write_epub(path: &Path, xhtml: &str) {
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
        for (name, text) in [
            ("mimetype", "application/epub+zip"),
            ("META-INF/container.xml", CONTAINER),
            ("OPS/book.opf", PACKAGE),
            ("OPS/chapter.xhtml", xhtml),
        ] {
            zip.start_file(name, options).unwrap();
            zip.write_all(text.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
    }

    fn tap(app: &mut Reader, rect: Rect) -> Change {
        app.input(Input::Tap(
            (rect.x + rect.w / 2) as f32,
            (rect.y + rect.h / 2) as f32,
        ))
    }

    #[test]
    fn selection_html_action_shows_exact_publisher_source_and_returns_to_the_selection() {
        let fixture = Fixture::new();
        write_epub(&fixture.book, XHTML);
        let fonts = vec![
            crate::test_fixture("fonts/NotoSans-Regular.ttf"),
            crate::test_fixture("fonts/NotoSans-Bold.ttf"),
        ];
        let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
        app.open_path(&fixture.book).unwrap();
        let probe = &app.active.as_ref().unwrap().reading.probe;
        let mut start = probe.fragment("second").unwrap();
        start.offset += "Outside right. The repeated ".len();
        let mut end = start.clone();
        end.offset += "jade & blue".len();
        let selected = probe.select_range(&start, &end).unwrap();
        assert_eq!(selected.text, "jade & blue");
        app.selection = Some(selected.clone());
        let origin = app.current_history_location().unwrap();

        capture_test_screen(&app, &fonts, "READER_SELECTION_HTML_POPUP", false);
        let button = app.selection_button(SelectionAction::Html);
        let paths: Vec<_> = fonts
            .iter()
            .map(|path| path.display().to_string())
            .collect();
        for dark in [false, true] {
            let mut canvas = Canvas::new(WIDTH, HEIGHT, &paths).unwrap();
            let normal = primitive_app::frame(&app, &mut canvas, dark);
            assert!(canvas.press_at(
                (button.x + button.w / 2) as f32,
                (button.y + button.h / 2) as f32,
            ));
            let pressed = primitive_app::frame(&app, &mut canvas, dark);
            let mut difference = 0;
            for (pixel, (&before, &after)) in normal.iter().zip(&pressed).enumerate() {
                if before != after {
                    let x = (pixel as u32 % WIDTH) as f32;
                    let y = (pixel as u32 / WIDTH) as f32;
                    assert!(button.contains(x, y));
                    difference += 1;
                }
            }
            assert!(difference > 500);
        }
        assert_eq!(tap(&mut app, button), Change::Full);
        assert_eq!(app.screen, Screen::SelectionHtml);
        capture_test_screen(&app, &fonts, "READER_SELECTION_HTML_LIGHT", false);
        capture_test_screen(&app, &fonts, "READER_SELECTION_HTML_DARK", true);
        let displayed = &app.selection_html.as_ref().unwrap().source;
        assert!(
            displayed
                .contains(r#"<p id="second"><strong class="signal">jade &amp; blue</strong></p>"#)
        );
        assert!(!displayed.contains("Outside") && !displayed.contains("cargo"));
        let back = primitive_ui::back_button(WIDTH as i32);
        assert_eq!(tap(&mut app, back), Change::Full);
        assert_eq!(app.screen, Screen::Book);
        assert_eq!(app.current_history_location(), Some(origin));
        assert_eq!(app.selection.as_ref(), Some(&selected));
        assert!(app.selection_html.is_none());

        let mut outside = selected.clone();
        outside.end.chapter = "different.xhtml".into();
        app.selection = Some(outside);
        assert!(!app.can_view_selection_html());
        app.selection = Some(selected.clone());

        fs::write(&fixture.book, b"Unreadable replacement").unwrap();
        let error = app.view_selection_html().unwrap_err().to_string();
        assert_eq!(error, "Source HTML is unavailable for this selection");
        assert_eq!(app.selection.as_ref(), Some(&selected));
        assert_eq!(app.screen, Screen::Book);
    }

    #[test]
    fn selection_html_pager_keeps_the_reading_page_and_rejects_non_epub_formats() {
        let fixture = Fixture::new();
        let text = "A readable plain text file.";
        let plain = fixture.root.join("Field.txt");
        fs::write(&plain, text).unwrap();
        let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
        let mut app = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
        app.open_path(&plain).unwrap();
        assert!(!app.can_view_selection_html());

        let block = "Recorded landmarks were visible from the eastern quay. ".repeat(45);
        let source = format!("<html><body><p id=\"range\">{block}</p></body></html>");
        write_epub(&fixture.book, &source);
        app.open_path(&fixture.book).unwrap();
        let probe = &app.active.as_ref().unwrap().reading.probe;
        let start = probe.fragment("range").unwrap();
        let mut end = start.clone();
        end.offset += block.trim_end().len();
        app.selection = probe.select_range(&start, &end);
        assert!(app.selection.is_some());
        let before = app.current_history_location();
        app.view_selection_html().unwrap();
        let paths = [crate::test_fixture("fonts/NotoSans-Regular.ttf")
            .display()
            .to_string()];
        let mut canvas = Canvas::new(WIDTH, HEIGHT, &paths).unwrap();
        primitive_app::frame(&app, &mut canvas, false);
        let pager = app.selection_html.as_ref().unwrap().pager();
        assert!(pager.pages > 1);
        assert_eq!(
            tap(&mut app, pager.button(PageTurn::Next).unwrap()),
            Change::Partial
        );
        assert_eq!(app.selection_html.as_ref().unwrap().page, 1);
        assert_eq!(app.input(Input::PageBack), Change::Partial);
        assert_eq!(app.selection_html.as_ref().unwrap().page, 0);
        assert_eq!(
            tap(&mut app, primitive_ui::back_button(WIDTH as i32)),
            Change::Full
        );
        assert_eq!(app.current_history_location(), before);
    }
}
