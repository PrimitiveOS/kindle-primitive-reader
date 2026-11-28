//! An offline external-link dialog over the reading page. The Kindle reference
//! offers an address, local clipboard and QR transfer on devices without an OS URL handler.

use super::navigation_layout::NavigationLayout;
use super::*;
use primitive_ui::TextStyle;
use qrcodegen::{QrCode, QrCodeEcc};
use std::cell::Cell;

const PANEL: Rect = Rect::new(72, 505, 1120, 416);
const QR_PANEL: Rect = Rect::new(72, 270, 1120, 1060);
const ADDRESS: Rect = Rect::new(114, 619, 1036, 145);
const CLOSE: Rect = Rect::new(980, 527, 176, 75);
const QR_CLOSE: Rect = Rect::new(980, 294, 176, 75);
const COPY: Rect = Rect::new(114, 805, 240, 84);
const CODE: Rect = Rect::new(378, 805, 242, 84);
const PREV: Rect = Rect::new(736, 805, 179, 84);
const NEXT: Rect = Rect::new(931, 805, 219, 84);
pub(super) const PASTE: Rect = Rect::new(900, LIST_Y + 170, 316, 72);

pub(super) struct LinkPopup {
    address: String,
    page: usize,
    pages: Cell<usize>,
    qr: Option<QrCode>,
    message: Option<&'static str>,
}

impl LinkPopup {
    pub(super) fn draw(
        &self,
        canvas: &mut Canvas,
        language: super::interface_language::InterfaceLanguage,
    ) {
        let layout = NavigationLayout::new(canvas.bounds().w, canvas.bounds().h);
        let place = |rect| layout.centered(rect);
        if let Some(code) = &self.qr {
            canvas.popup(place(QR_PANEL));
            canvas.text(
                place(Rect::new(114, 294, 814, 75)),
                language.navigation_popup_text("Scan link"),
                TextStyle::new(41).bold(),
            );
            canvas.button(
                place(QR_CLOSE),
                language.navigation_popup_text("Back"),
                false,
                false,
            );
            // Model 2 QR codes require four light modules on each side.
            let scale = 848 / (code.size() + 8);
            let size = scale * (code.size() + 8);
            let area = place(Rect::new(
                (WIDTH as i32 - size) / 2,
                405 + (848 - size) / 2,
                size,
                size,
            ));
            let (x0, y0) = (area.x, area.y);
            canvas.fill(area, 255);
            for y in 0..code.size() {
                for x in 0..code.size() {
                    if code.get_module(x, y) {
                        canvas.fill(
                            Rect::new(x0 + (x + 4) * scale, y0 + (y + 4) * scale, scale, scale),
                            0,
                        );
                    }
                }
            }
            return;
        }
        canvas.popup(place(PANEL));
        canvas.text(
            place(Rect::new(114, 527, 814, 75)),
            language.navigation_popup_text("Link address"),
            TextStyle::new(41).bold(),
        );
        canvas.button(
            place(CLOSE),
            language.navigation_popup_text("Close"),
            false,
            false,
        );
        let style = TextStyle::new(34).lines(3).break_long_words();
        let pages = canvas.text_pages(place(ADDRESS), &self.address, style);
        self.pages.set(pages.len().max(1));
        if let Some(range) = pages.get(self.page) {
            canvas.text(place(ADDRESS), &self.address[range.clone()], style);
        }
        canvas.button(
            place(COPY),
            language.navigation_popup_text("Copy"),
            false,
            false,
        );
        canvas.button(
            place(CODE),
            language.navigation_popup_text("QR code"),
            false,
            false,
        );
        if self.pages.get() > 1 {
            canvas.button(
                place(PREV),
                language.navigation_popup_text("Back"),
                false,
                false,
            );
            canvas.button(
                place(NEXT),
                language.navigation_popup_text("Next"),
                false,
                false,
            );
        }
        if let Some(message) = self.message {
            canvas.text(place(Rect::new(114, 750, 1038, 53)), message, CAPTION);
        }
    }

    fn turn(&mut self, step: isize) {
        self.page = self
            .page
            .saturating_add_signed(step)
            .min(self.pages.get().saturating_sub(1));
    }
}

pub(super) fn draw_paste(canvas: &mut Canvas) {
    canvas.button(PASTE, "Paste", false, false);
}

impl Reader {
    pub(super) fn show_link_address(&mut self, address: String) -> Change {
        self.link_popup = Some(LinkPopup {
            address,
            page: 0,
            pages: Cell::new(1),
            qr: None,
            message: None,
        });
        self.sync_auto_turn(Instant::now());
        Change::Partial
    }

    pub(super) fn editor_paste_rect(&self) -> Rect {
        if self.screen == Screen::Find {
            return self.search_layout().find_paste(PASTE);
        }
        let mut rect = PASTE;
        if self.screen.uses_reading_text_editor() {
            rect.x += self.reading_viewport().dimensions().0 as i32 - WIDTH as i32;
        }
        rect
    }

    pub(super) fn can_paste_text(&self) -> bool {
        self.clipboard_text.is_some()
            && matches!(
                self.screen,
                Screen::Note
                    | Screen::EditNote
                    | Screen::Find
                    | Screen::LibraryFind
                    | Screen::BookmarkFind
                    | Screen::DictionaryEntry
                    | Screen::VocabularySearch
                    | Screen::ExportSettings(_, export_settings::Page::Template(_))
            )
    }

    pub(super) fn paste_text(&mut self) -> Change {
        let Some(text) = self.clipboard_text.as_deref() else {
            return Change::None;
        };
        if text.chars().count() > self.editor.max.saturating_sub(self.editor.text.len()) {
            return self.report("The copied text is too long for this field");
        }
        let caret = self.editor.caret.min(self.editor.text.len());
        self.editor.text.splice(caret..caret, text.chars());
        self.editor.caret = caret + text.chars().count();
        Change::Partial
    }

    pub(super) fn link_address_input(&mut self, input: Input) -> Change {
        let layout = self.navigation_layout();
        let place = |rect| layout.centered(rect);
        let done = self.reading_viewport().done();
        let Some(popup) = &mut self.link_popup else {
            return Change::None;
        };
        if let Input::Tap(x, y) = input {
            if done.contains(x, y) {
                self.link_popup = None;
                return self.exit();
            }
            if popup.qr.is_some() {
                if place(QR_CLOSE).contains(x, y) {
                    popup.qr = None;
                } else if !place(QR_PANEL).contains(x, y) {
                    self.link_popup = None;
                }
            } else if place(CLOSE).contains(x, y) || !place(PANEL).contains(x, y) {
                self.link_popup = None;
            } else if place(COPY).contains(x, y) {
                self.clipboard_text = Some(popup.address.clone());
                self.link_popup = None;
                self.notice = Some("Link copied".into());
            } else if place(CODE).contains(x, y) {
                match QrCode::encode_text(&popup.address, QrCodeEcc::Low) {
                    Ok(code) => popup.qr = Some(code),
                    Err(_) => popup.message = Some("This address is too long for a QR code"),
                }
            } else if place(PREV).contains(x, y) {
                popup.turn(-1);
            } else if place(NEXT).contains(x, y) {
                popup.turn(1);
            }
        } else if popup.qr.is_none() {
            match input {
                Input::PageForward => popup.turn(1),
                Input::PageBack => popup.turn(-1),
                Input::Swipe { dx, dy, .. } => {
                    let delta = if dx.abs() > dy.abs() { dx } else { dy };
                    if delta.abs() < 42.0 {
                        return Change::None;
                    }
                    popup.turn(if delta < 0.0 { 1 } else { -1 });
                }
                _ => return Change::None,
            }
        } else {
            return Change::None;
        }
        self.sync_auto_turn(Instant::now());
        Change::Partial
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::{Fixture, capture_test_screen};
    use crate::fixed::{Pdf, tests::pdf_objects};

    #[test]
    fn pdf_uri_taps_show_the_exact_address_and_copy_without_leaving_the_page() {
        let fixture = Fixture::new();
        let external = std::env::var_os("READER_TEST_LINK_PDF").map(PathBuf::from);
        let path = external
            .clone()
            .unwrap_or_else(|| fixture.root.join("Harbor links.pdf"));
        if external.is_none() {
            let content = "BT /F1 21 Tf 72 658 Td (Harbor field guide) Tj /F1 14 Tf 0 -55 Td (Read the tide chart online) Tj ET";
            fs::write(&path,pdf_objects(&[
                "<< /Type /Catalog /Pages 2 0 R >>".into(),
                "<< /Type /Pages /Count 1 /Kids [3 0 R] /MediaBox [0 0 600 800] >>".into(),
                "<< /Type /Page /Parent 2 0 R /Annots [4 0 R] /Contents 5 0 R /Resources << /Font << /F1 6 0 R >> >> >>".into(),
                "<< /Subtype /Link /Rect [69 596 280 627] /A << /S /URI /URI (https://example.org/harbor/tide-chart) >> >>".into(),
                format!("<< /Length {} >>\nstream\n{content}\nendstream",content.len()),
                "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),
            ])).unwrap();
        }
        let pages: Vec<usize> = std::env::var("READER_TEST_LINK_PAGES")
            .unwrap_or_else(|_| "1".into())
            .split(',')
            .map(|s| s.parse::<usize>().unwrap() - 1)
            .collect();
        let links: Vec<_> = {
            let independent = Pdf::open(fs::read(&path).unwrap()).unwrap();
            pages
                .into_iter()
                .flat_map(|page| {
                    independent
                        .page_links(page)
                        .unwrap()
                        .into_iter()
                        .filter_map(move |l| l.uri.map(|address| (page, l.bounds, address)))
                })
                .collect()
        };
        let mut addresses: Vec<_> = links
            .iter()
            .map(|(_, _, address)| address.as_str())
            .collect();
        addresses.sort_unstable();
        if let Ok(expected) = std::env::var("READER_TEST_LINK_DIGEST") {
            assert_eq!(
                format!("{:x}", md5::compute(addresses.join("\0"))),
                expected
            );
        } else {
            assert_eq!(addresses, ["https://example.org/harbor/tide-chart"]);
        }
        assert!(!links.is_empty());
        let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
        let mut app = Reader::open(path.parent().unwrap(), &fixture.state, fonts.clone()).unwrap();
        app.open_path(&path).unwrap();
        for (page, bounds, address) in links {
            app.follow_fixed(page, None).unwrap();
            let spot = app
                .fixed
                .as_ref()
                .unwrap()
                .reading
                .visible_rectangle(page, bounds)
                .unwrap()
                .center();
            let held = app.current_history_location().unwrap();
            let history = app.back.len();
            let tap = |a: &mut Reader, r: Rect| {
                a.input(Input::Tap((r.x + r.w / 2) as f32, (r.y + r.h / 2) as f32))
            };
            assert_eq!(
                app.input(Input::Tap(spot.x as f32, spot.y as f32)),
                Change::Partial
            );
            assert!(
                app.link_popup
                    .as_ref()
                    .is_some_and(|p| p.address == address)
            );
            if external.is_none() {
                capture_test_screen(&app, &fonts, "READER_LINK_PREVIEW_SCREENSHOT", false);
            }
            tap(&mut app, CODE);
            assert!(app.link_popup.as_ref().unwrap().qr.is_some());
            if external.is_none() {
                capture_test_screen(&app, &fonts, "READER_LINK_QR_SCREENSHOT", false);
                capture_test_screen(&app, &fonts, "READER_LINK_QR_NIGHT", true);
                let font_paths = fonts
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>();
                let mut canvas = Canvas::new(WIDTH, HEIGHT, &font_paths).unwrap();
                let pixels = primitive_app::frame(&app, &mut canvas, false);
                let dark = (405..1253)
                    .flat_map(|y| {
                        let offset = y * WIDTH as usize;
                        &pixels[offset + 208..offset + 1056]
                    })
                    .filter(|&&p| p < 40)
                    .count();
                assert!(
                    dark > 70_000,
                    "QR modules must paint over the document bitmap"
                );
            }
            tap(&mut app, QR_CLOSE);
            assert!(app.link_popup.as_ref().unwrap().qr.is_none());
            tap(&mut app, COPY);
            assert!(app.link_popup.is_none());
            assert!(app.clipboard_text.as_deref() == Some(address.as_str()));
            assert_eq!(app.current_history_location(), Some(held));
            assert_eq!(app.back.len(), history);
            if external.is_none() {
                app.show(Screen::Find);
                tap(&mut app, PASTE);
                assert_eq!(app.editor.string(), address);
                app.show(Screen::Fixed);
            }
        }
    }

    #[test]
    fn long_link_pages_and_paste_keep_the_original_address_intact() {
        let fixture = Fixture::new();
        let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
        let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
        let path = fixture.root.join("Harbor.pdf");
        fs::write(&path, crate::fixed::tests::document()).unwrap();
        app.open_path(&path).unwrap();
        let address = "https://example.org/".to_string() + &"coast".repeat(780);
        app.show_link_address(address.clone());
        // This capture also exercises the long-word wrapping and paging layout.
        let font_paths = fonts
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>();
        let mut canvas = Canvas::new(WIDTH, HEIGHT, &font_paths).unwrap();
        app.draw(&mut canvas);
        assert!(app.link_popup.as_ref().unwrap().pages.get() > 1);
        app.input(Input::PageForward);
        assert_eq!(app.link_popup.as_ref().unwrap().page, 1);
        app.input(Input::PageBack);
        assert_eq!(app.link_popup.as_ref().unwrap().page, 0);
        app.input(Input::Tap((CODE.x + 12) as f32, (CODE.y + 12) as f32));
        assert!(app.link_popup.as_ref().unwrap().message.is_some());
        app.input(Input::Tap((COPY.x + 12) as f32, (COPY.y + 12) as f32));
        app.show(Screen::Find);
        app.editor.text = vec!['x'];
        app.paste_text();
        assert_eq!(app.editor.string(), "x");
        assert_eq!(app.clipboard_text.as_deref(), Some(address.as_str()));
    }

    #[test]
    fn reading_lists_viewport_pdf_links_retain_the_destination_and_keep_qr_controls_visible() {
        use crate::app::selection_viewport_tests::{adopt, press, tap};
        use crate::session::ReadingViewport;

        let fixture = Fixture::new();
        let path = fixture.root.join("Links supplement.pdf");
        let address = "https://example.invalid/harbour";
        let regions = (0..14)
            .map(|i| {
                let y = 542 - i * 20;
                let target = if i == 13 {
                    format!("/A << /S /URI /URI ({address}) >>")
                } else {
                    "/Dest [4 0 R /Fit]".into()
                };
                format!(
                    "<< /Subtype /Link /Rect [44 {y} 174 {}] {target} >>",
                    y + 15
                )
            })
            .collect::<Vec<_>>()
            .join(" ");
        fs::write(
            &path,
            pdf_objects(&[
                "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
                "<< /Type /Pages /Count 2 /Kids [3 0 R 4 0 R] >>".to_owned(),
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 440 600] /Annots 5 0 R >>".to_owned(),
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 440 600] >>".to_owned(),
                format!("[{regions}]"),
            ]),
        )
        .unwrap();
        let fonts = vec![
            crate::test_fixture("fonts/NotoSans-Regular.ttf"),
            crate::test_fixture("fonts/NotoSans-Bold.ttf"),
        ];
        let mut app = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
        app.open_path(&path).unwrap();
        app.show(Screen::Links);
        assert_eq!(app.list_window().0, 14);
        assert_eq!(app.input(Input::PageForward), Change::Partial);
        let first = match app.list_window().1.remove(0).action {
            Action::OpenFixedLink(link) => link,
            _ => panic!("Expected a PDF annotation destination"),
        };
        let mut canvas = adopt(&mut app, ReadingViewport::LANDSCAPE, 2);
        let layout = app.navigation_layout();
        assert!(
            app.list_window().1.iter().any(|item| {
                matches!(&item.action, Action::OpenFixedLink(link) if *link == first)
            })
        );
        let origin = app.current_history_location();
        // The address row can sit on a later page of a long list; advance until it is visible.
        let index = (0..8)
            .find_map(|_| {
                let found = app
                    .list_window()
                    .1
                    .iter()
                    .position(|item| item.detail.as_deref() == Some(address));
                if found.is_none() {
                    app.list_page += 1;
                }
                found
            })
            .unwrap();
        let row = app.list_geometry().row(index);
        let hit = Input::Tap((row.right() - 24) as f32, (row.y + row.h / 2) as f32);
        assert_eq!(app.input(hit), Change::Partial);
        assert_eq!(app.link_popup.as_ref().unwrap().address, address);
        let qr = layout.centered(CODE);
        press(&app, &mut canvas, qr, false);
        tap(&mut app, qr);
        assert!(app.link_popup.as_ref().unwrap().qr.is_some());
        assert!(layout.centered(QR_PANEL).bottom() < layout.height);
        tap(&mut app, layout.centered(QR_CLOSE));
        assert!(app.link_popup.as_ref().unwrap().qr.is_none());
        tap(&mut app, layout.centered(COPY));
        assert_eq!(app.clipboard_text.as_deref(), Some(address));
        assert!(app.link_popup.is_none());
        assert_eq!(app.screen, Screen::Fixed);
        assert_eq!(app.current_history_location(), origin);

        app.show(Screen::Links);
        let internal = app.list_geometry().row(0);
        assert_eq!(
            app.input(Input::Tap(
                (internal.right() - 24) as f32,
                (internal.y + internal.h / 2) as f32
            )),
            Change::Partial
        );
        assert_eq!(app.current_position().unwrap().page, 1);
        app.activate(Action::ReturnToLocation).unwrap();
        assert_eq!(app.current_history_location(), origin);
        app.show(Screen::Links);
        tap(&mut app, layout.back());
        assert_eq!(app.screen, Screen::FixedTools);
    }
}
