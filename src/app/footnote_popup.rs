use super::{Change, Destination, Input, Reader, Screen};
use crate::epub::Book;
use crate::footnotes::{self, Reference, ReferenceContext};
use crate::selection::TextSelection;
use crate::session::{Reading, ReadingViewport};
use crate::{Anchor, Engine, Probe, Settings};
use primitive_app::with_storage;
#[cfg(test)]
use primitive_app::{HEIGHT, WIDTH};
use primitive_ui::{Canvas, GrayImage, Rect, TextStyle};
use std::io;

const INSET: i32 = 16;
const HEADER: i32 = 76;
const MIN_FONT_PIXELS: f32 = 12.0;
// Match the renderer's physical-font ceiling across base and inline runs.
const MAX_FONT_PIXELS: f32 = 192.0;
// A second size for lines with large authored spacing in the short popup.
const TALL_LINE_FONT_PIXELS: f32 = 140.0;

#[cfg(test)]
mod tests;

pub(super) struct Footnote {
    target: Destination,
    pub(super) reference: Option<TextSelection>,
    preview: Probe,
    viewport: ReadingViewport,
    source: String,
    cursor: Option<Anchor>,
    page: usize,
    frame: GrayImage,
    selection: Option<TextSelection>,
}

impl Footnote {
    fn frame(preview: &mut Probe, page: usize) -> crate::Result<GrayImage> {
        Ok(GrayImage {
            width: preview.settings.width,
            height: preview.settings.height,
            pixels: preview.capture(page)?,
        })
    }

    fn content_height(&self) -> u32 {
        if self.preview.page_count() == 1 {
            (self.preview.page_range(self.page).end.ceil() as u32
                + self.preview.settings.top_margin()
                + self.preview.settings.bottom_margin())
            .clamp(40, self.frame.height)
        } else {
            self.frame.height
        }
    }

    pub(super) fn panel(&self) -> Rect {
        let h = HEADER + self.content_height() as i32 + 20;
        let (width, height) = self.viewport.dimensions();
        Rect::new(
            INSET,
            height as i32 - h - INSET,
            width as i32 - 2 * INSET,
            h,
        )
    }

    fn content(&self) -> Rect {
        let panel = self.panel();
        Rect::new(
            panel.x + INSET,
            panel.y + HEADER,
            self.frame.width as i32,
            self.content_height() as i32,
        )
    }

    fn close_button(&self) -> Rect {
        let p = self.panel();
        Rect::new(p.right() - 148, p.y + 10, 132, 58)
    }

    pub(super) fn follow_button(&self) -> Rect {
        let p = self.panel();
        Rect::new(p.right() - 442, p.y + 10, 278, 58)
    }

    fn lookup_button(&self) -> Rect {
        let p = self.panel();
        Rect::new(p.right() - 636, p.y + 10, 178, 58)
    }

    pub(super) fn draw(&self, canvas: &mut Canvas, dark: bool) {
        let panel = self.panel();
        canvas.popup(panel);
        let title = if self.preview.page_count() == 1 {
            "Footnote".to_owned()
        } else {
            format!(
                "Footnote  ·  {} / {}",
                self.page + 1,
                self.preview.page_count()
            )
        };
        canvas.text(
            Rect::new(panel.x + 24, panel.y + 14, 500, 48),
            &title,
            TextStyle::new(29).bold(),
        );
        canvas.button(self.close_button(), "Close", false, false);
        canvas.button(self.follow_button(), "Open in book", false, false);
        if self.selection.is_some() {
            canvas.button(self.lookup_button(), "Look up", false, false);
        }
        let mut frame = self.frame.clone();
        if dark && !self.preview.settings.invert_night_images {
            crate::session::preinvert_images(
                &self.preview,
                self.preview.page_range(self.page),
                0,
                frame.width,
                &mut frame.pixels,
            );
        }
        frame.height = self.content_height();
        frame.pixels.truncate((frame.width * frame.height) as usize);
        if let Some(selected) = &self.selection {
            for rect in self.preview.selection_rects(self.page, selected) {
                let x0 = rect.x.clamp(0, frame.width as i32) as usize;
                let x1 = (rect.x + rect.width).clamp(0, frame.width as i32) as usize;
                let y0 = rect.y.clamp(0, frame.height as i32) as usize;
                let y1 = (rect.y + rect.height).clamp(0, frame.height as i32) as usize;
                for y in y0..y1 {
                    let start = y * frame.width as usize;
                    for ink in &mut frame.pixels[start + x0..start + x1] {
                        *ink = (*ink).min(195);
                    }
                }
            }
        }
        if dark {
            for pixel in &mut frame.pixels {
                *pixel = 255 - *pixel;
            }
        }
        canvas.picture(self.content(), frame);
    }

    fn select_at(&mut self, x: f32, y: f32, extend: bool) -> Option<String> {
        let content = self.content();
        if !content.contains(x, y) {
            return None;
        }
        let word = self
            .preview
            .word_at(self.page, x - content.x as f32, y - content.y as f32)?;
        let range = if extend && let Some(selected) = &self.selection {
            let first = if (word.start.block, word.start.offset)
                < (selected.start.block, selected.start.offset)
            {
                &word.start
            } else {
                &selected.start
            };
            let last =
                if (word.end.block, word.end.offset) > (selected.end.block, selected.end.offset) {
                    &word.end
                } else {
                    &selected.end
                };
            self.preview.select_range(first, last)?
        } else {
            word
        };
        let text = range.text.clone();
        self.selection = Some(range);
        Some(text)
    }

    fn turn(&mut self, offset: isize) -> crate::Result<bool> {
        let Some(page) = self
            .page
            .checked_add_signed(offset)
            .filter(|p| *p < self.preview.page_count())
        else {
            return Ok(false);
        };
        self.frame = Self::frame(&mut self.preview, page)?;
        self.page = page;
        self.cursor = self.preview.page_anchor(page);
        self.selection = None;
        Ok(true)
    }
}

#[cfg(test)]
impl Footnote {
    pub(super) fn test_word(&self, text: &str) -> (f32, f32) {
        let start = self.preview.matches(text).into_iter().next().unwrap();
        let mut end = start.clone();
        end.offset += text.len();
        let selection = self.preview.select_range(&start, &end).unwrap();
        let rects = self.preview.selection_rects(self.page, &selection);
        let r = rects
            .first()
            .unwrap_or_else(|| panic!("{text} is not on footnote page {}", self.page));
        let content = self.content();
        (
            (content.x + r.x + r.width / 2) as f32,
            (content.y + r.y + r.height / 2) as f32,
        )
    }
    pub(super) fn test_page(&self) -> (usize, usize) {
        (self.page, self.preview.page_count())
    }
    pub(super) fn test_word_page(&self, text: &str) -> usize {
        let anchor = self.preview.matches(text).into_iter().next().unwrap();
        self.preview.page_for(&anchor).unwrap()
    }
    pub(super) fn test_selected(&self) -> Option<&str> {
        self.selection.as_ref().map(|s| s.text.as_str())
    }
}

impl Reader {
    pub(super) fn open_note_at(&mut self, target: Destination, x: f32, y: f32) -> bool {
        if !self.controls.footnote_popups {
            return false;
        }
        let Some(fragment) = target.fragment.as_deref() else {
            return false;
        };
        let Some(active) = &self.active else {
            return false;
        };
        let reading = &active.reading;
        let Some(index) = reading.probe.chapter_index(&target.path) else {
            return false;
        };
        let (context, reference) = reading
            .note_reference_at(x, y)
            .map(|(context, selected)| (context, Some(selected)))
            .unwrap_or_else(|| {
                (
                    ReferenceContext {
                        kind: Reference::Unknown,
                        label: String::new(),
                        in_prose: false,
                        source_block: 0,
                    },
                    None,
                )
            });
        if context.kind == Reference::Ordinary {
            return false;
        }
        let viewport = self.reading_viewport();
        let toc_target = reading
            .probe
            .toc
            .iter()
            .any(|entry| entry.destination.as_ref() == Some(&target));
        let result = with_storage(|| {
            let mut book = Book::open(&reading.config.book).map_err(io::Error::other)?;
            if book.revision() != reading.probe.book {
                return Ok(None);
            }
            let xhtml = book.chapter(index).map_err(io::Error::other)?;
            let source = book
                .chapter_source(&target.path, &xhtml)
                .map_err(io::Error::other)?;
            let styles = crate::book_style::BookStyle::load_for_flow_with_base(
                &mut book,
                &target.path,
                &source,
                reading.config.settings.publisher_styles,
                &reading.config.user_styles,
                reading.config.settings.base_stylesheet,
            )
            .map_err(io::Error::other)?
            .compute(&source.elements);
            let Some(roots) = footnotes::LayoutRoots::new(&source, &styles) else {
                return Ok(None);
            };
            let Some(excerpt) = footnotes::extract_with_layout(
                &xhtml,
                fragment,
                &context,
                reading.chapter.cmp(&index),
                toc_target,
                self.controls.prefer_footnotes,
                &roots,
            ) else {
                return Ok(None);
            };
            let mut preview = self.render_footnote(book, reading, index, &excerpt, viewport)?;
            let frame = Footnote::frame(&mut preview, 0).map_err(io::Error::other)?;
            Ok(Some((excerpt, preview, frame)))
        });
        let Ok(Some((source, preview, frame))) = result else {
            return false;
        };
        let cursor = preview.page_anchor(0);
        self.show(Screen::Footnote);
        self.footnote = Some(Footnote {
            target,
            reference,
            preview,
            viewport,
            source,
            cursor,
            page: 0,
            frame,
            selection: None,
        });
        true
    }

    fn render_footnote(
        &self,
        book: Book,
        reading: &Reading,
        index: usize,
        excerpt: &str,
        viewport: ReadingViewport,
    ) -> io::Result<Probe> {
        let (width, height) = viewport.dimensions();
        let font = self
            .controls
            .footnote_font_size
            .requested_pixels(reading.config.settings);
        let settings = Settings {
            width: width - 4 * INSET as u32,
            height: height / 3 - 112,
            status_header_height: 0,
            margin: 16,
            margin_sides: Some(22),
            margin_left: None,
            margin_right: None,
            margin_top: Some(8),
            margin_bottom: Some(8),
            font_size: font.clamp(MIN_FONT_PIXELS, MAX_FONT_PIXELS),
            logical_font_size: None,
            // The popup uses its own text defaults. Keep inline publisher
            // styles, without inheriting the book's base stylesheet or alignment.
            base_stylesheet: crate::BaseStylesheet::None,
            alignment: if self.controls.footnote_justify.unwrap_or(true) {
                crate::TextAlignment::FootnoteJustify
            } else {
                crate::TextAlignment::Publisher
            },
            ..reading.config.settings
        };
        let render = |book, settings| {
            Probe::from_book_with_font_loader(
                book,
                Engine::Parley,
                settings,
                index,
                Some(excerpt.to_owned()),
                &reading.config.user_hyphenation,
                // The popup has an independent stylesheet context.
                &crate::user_styles::Snapshot::default(),
                || {
                    if self.controls.footnote_book_font
                        && reading.config.fonts != self.fonts
                        && let Ok(fonts) = crate::font_context(&reading.config.fonts)
                    {
                        return Ok(fonts);
                    }
                    crate::font_context(&self.fonts)
                },
            )
        };
        match render(book, settings) {
            Err(error)
                if error.downcast_ref::<crate::PageContentTooTall>().is_some()
                    && settings.font_size > TALL_LINE_FONT_PIXELS =>
            {
                let book = Book::open(&reading.config.book).map_err(io::Error::other)?;
                if book.revision() != reading.probe.book {
                    return Err(io::Error::other(
                        "The book changed while opening the footnote",
                    ));
                }
                render(
                    book,
                    Settings {
                        font_size: TALL_LINE_FONT_PIXELS,
                        ..settings
                    },
                )
                .map_err(io::Error::other)
            }
            result => result.map_err(io::Error::other),
        }
    }

    pub(super) fn resize_footnote(
        &self,
        viewport: ReadingViewport,
    ) -> crate::Result<Option<Footnote>> {
        let Some(note) = self
            .footnote
            .as_ref()
            .filter(|note| note.viewport != viewport)
        else {
            return Ok(None);
        };
        let reading = &self
            .active
            .as_ref()
            .ok_or("The footnote's book closed")?
            .reading;
        let chapter = reading
            .probe
            .chapter_index(&note.target.path)
            .ok_or("The footnote is unavailable")?;
        let (preview, frame, page, selection) = with_storage(|| {
            let book = Book::open(&reading.config.book).map_err(io::Error::other)?;
            if book.revision() != reading.probe.book {
                return Err(io::Error::other(
                    "The book changed while resizing the footnote",
                ));
            }
            let mut preview =
                self.render_footnote(book, reading, chapter, &note.source, viewport)?;
            let page = note
                .cursor
                .as_ref()
                .and_then(|cursor| preview.page_for(cursor))
                .unwrap_or_else(|| note.page.min(preview.page_count().saturating_sub(1)));
            let selection = note.selection.as_ref().and_then(|selected| {
                preview
                    .select_range(&selected.start, &selected.end)
                    .filter(|range| range.text == selected.text)
            });
            let frame = Footnote::frame(&mut preview, page).map_err(io::Error::other)?;
            Ok((preview, frame, page, selection))
        })?;
        Ok(Some(Footnote {
            target: note.target.clone(),
            reference: note.reference.clone(),
            preview,
            viewport,
            source: note.source.clone(),
            cursor: note.cursor.clone(),
            page,
            frame,
            selection,
        }))
    }

    pub(super) fn footnote_input(&mut self, input: Input) -> Change {
        let Some(note) = self.footnote.as_mut() else {
            self.show(Screen::Book);
            return Change::Partial;
        };
        match input {
            Input::Tap(x, y) if note.close_button().contains(x, y) => {
                self.show(Screen::Book);
                Change::Partial
            }
            Input::Tap(x, y) if !note.panel().contains(x, y) => {
                self.show(Screen::Book);
                if let Some((super::link_taps::ReflowLink::Internal(target), x, y)) =
                    self.reflow_tap_link(x, y)
                {
                    self.open_note_at(target, x, y);
                }
                Change::Partial
            }
            Input::Tap(x, y) if note.follow_button().contains(x, y) => self.follow_preview(),
            Input::Swipe { dx, dy, .. } if dx < -60.0 && dx.abs() > dy.abs() => {
                self.follow_preview()
            }
            Input::Tap(x, y) if note.selection.is_some() && note.lookup_button().contains(x, y) => {
                let word = note.selection.as_ref().unwrap().text.clone();
                match self.look_up(&word) {
                    Ok(()) => Change::Partial,
                    Err(e) => self.report(e),
                }
            }
            Input::LongTap(x, y) => {
                let Some(word) = note.select_at(x, y, false) else {
                    return Change::None;
                };
                match self.look_up(&word) {
                    Ok(()) => Change::Partial,
                    Err(e) => self.report(e),
                }
            }
            Input::Tap(x, y) if note.selection.is_some() && note.content().contains(x, y) => {
                if note.select_at(x, y, true).is_some() {
                    Change::Partial
                } else {
                    Change::None
                }
            }
            Input::Tap(x, y) if note.content().contains(x, y) => {
                let r = note.content();
                if let Some(target) =
                    note.preview
                        .link_at(note.page, x - r.x as f32, y - r.y as f32)
                {
                    return match self.follow(target) {
                        Ok(()) => Change::Partial,
                        Err(e) => self.report(e),
                    };
                }
                let step = if y < r.y as f32 + r.h as f32 / 2.0 {
                    -1
                } else {
                    1
                };
                self.turn_footnote(step)
            }
            Input::PageForward => self.turn_footnote(1),
            Input::PageBack if note.page == 0 => {
                self.show(Screen::Book);
                Change::Partial
            }
            Input::PageBack => self.turn_footnote(-1),
            Input::Swipe { dy, dx, start_y }
                if dy > 60.0
                    && dy > dx.abs()
                    && (note.preview.page_count() == 1 || start_y < note.content().y as f32) =>
            {
                self.show(Screen::Book);
                Change::Partial
            }
            Input::Swipe { dy, dx, .. } if dy.abs() > 60.0 && dy.abs() > dx.abs() => {
                self.turn_footnote(if dy < 0.0 { 1 } else { -1 })
            }
            Input::Swipe { dx, dy, .. } if dx > 60.0 && dx.abs() > dy.abs() => {
                self.show(Screen::Book);
                Change::Partial
            }
            _ => Change::None,
        }
    }

    fn turn_footnote(&mut self, step: isize) -> Change {
        match self.footnote.as_mut().unwrap().turn(step) {
            Ok(true) => Change::Partial,
            Ok(false) => Change::None,
            Err(e) => self.report(e),
        }
    }
    fn follow_preview(&mut self) -> Change {
        let target = self.footnote.as_ref().unwrap().target.clone();
        match self.follow(target) {
            Ok(()) => Change::Partial,
            Err(e) => self.report(e),
        }
    }
}
