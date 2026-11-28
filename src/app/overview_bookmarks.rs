//! Bookmark source pages while browsing thumbnails without changing the open page.
use super::*;
use primitive_ui::BLACK;

use super::navigation_layout::NavigationLayout;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PageAction {
    Bookmark,
    Chapter,
    HiddenFlow,
    Cancel,
}

pub(super) struct PageActions {
    chapter: Option<usize>,
    page: usize,
    bookmarked: bool,
    chapter_edit: Option<bool>,
    flow_edit: bool,
}

impl PageActions {
    fn panel(&self, layout: NavigationLayout) -> Rect {
        let height =
            325 + 109 * (i32::from(self.chapter_edit.is_some()) + i32::from(self.flow_edit));
        const WIDTH: i32 = 880;
        Rect::new(
            (layout.width - WIDTH) / 2,
            (layout.height - height) / 2,
            WIDTH,
            height,
        )
    }

    fn buttons(
        &self,
        layout: NavigationLayout,
    ) -> impl Iterator<Item = (PageAction, &'static str, Rect)> {
        let panel = self.panel(layout);
        [
            Some((
                PageAction::Bookmark,
                if self.bookmarked {
                    "Remove page bookmark"
                } else {
                    "Bookmark page"
                },
            )),
            self.chapter_edit.map(|existing| {
                (
                    PageAction::Chapter,
                    if existing {
                        "Edit chapter"
                    } else {
                        "Start chapter here"
                    },
                )
            }),
            self.flow_edit
                .then_some((PageAction::HiddenFlow, "Mark flow boundary")),
            Some((PageAction::Cancel, "Cancel")),
        ]
        .into_iter()
        .flatten()
        .enumerate()
        .map(move |(index, (action, label))| {
            let rect = Rect::new(
                panel.x + 36,
                panel.y + 107 + index as i32 * 109,
                panel.w - 72,
                94,
            );
            (action, label, rect)
        })
    }

    #[cfg(test)]
    pub(super) fn button(&self, action: PageAction) -> Option<Rect> {
        self.button_in(NavigationLayout::PORTRAIT, action)
    }

    #[cfg(test)]
    pub(super) fn button_in(&self, layout: NavigationLayout, action: PageAction) -> Option<Rect> {
        self.buttons(layout)
            .find(|(candidate, _, _)| *candidate == action)
            .map(|(_, _, rect)| rect)
    }

    pub(super) fn draw(
        &self,
        canvas: &mut Canvas,
        layout: NavigationLayout,
        language: super::interface_language::InterfaceLanguage,
    ) {
        let panel = self.panel(layout);
        canvas.popup(panel);
        canvas.text(
            Rect::new(panel.x + 36, panel.y + 20, panel.w - 72, 66),
            &if language.is_english() {
                format!("Page {} actions", self.page + 1)
            } else {
                format!("Tùy chọn trang {}", self.page + 1)
            },
            BODY.bold(),
        );
        for (_, label, rect) in self.buttons(layout) {
            canvas.button(
                rect,
                language.overview_page_action_text(label),
                false,
                false,
            );
        }
    }

    pub(super) fn input(self, reader: &mut Reader, input: Input) -> Change {
        let layout = reader.navigation_layout();
        let action = match input {
            Input::PageBack => Some(PageAction::Cancel),
            Input::Tap(x, y) if !self.panel(layout).contains(x, y) => Some(PageAction::Cancel),
            Input::Tap(x, y) => self
                .buttons(layout)
                .find(|(_, _, rect)| rect.contains(x, y))
                .map(|(action, _, _)| action),
            _ => None,
        };
        let result = match action {
            Some(PageAction::Bookmark) => reader.toggle_overview_bookmark(self.chapter, self.page),
            Some(PageAction::Chapter) => reader.edit_fixed_thumbnail_chapter(self.page),
            Some(PageAction::HiddenFlow) => reader.show_flow_page_popup(self.chapter, self.page),
            Some(PageAction::Cancel) => return Change::Partial,
            None => {
                reader.flow_popup = Some(hidden_flows::FlowPopup::Overview(self));
                return Change::None;
            }
        };
        match result {
            Ok(()) => Change::Partial,
            Err(error) => reader.report(error),
        }
    }
}

pub(super) fn bookmark_index_for_page(
    book: &BookState,
    chapter: Option<usize>,
    page: usize,
    probe: Option<&Probe>,
) -> Option<usize> {
    book.bookmarks.iter().position(|mark| {
        let position = &mark.position;
        if let Some(anchor) = position
            .anchor
            .as_ref()
            .filter(|anchor| anchor.raw_caret.is_some())
        {
            return chapter == Some(position.chapter)
                && probe.and_then(|probe| probe.page_for(anchor)) == Some(page);
        }
        match chapter {
            None => position.page == page,
            Some(chapter) => {
                position.chapter == chapter
                    && position
                        .anchor
                        .as_ref()
                        .and_then(|anchor| probe.and_then(|probe| probe.page_for(anchor)))
                        .unwrap_or(position.page)
                        == page
            }
        }
    })
}

pub(super) fn push_page_bookmark(
    book: &mut BookState,
    position: ReadingPosition,
    label: String,
    chapter_title: Option<String>,
) -> Result<()> {
    if book.bookmarks.len() >= 256 {
        return Err("This book has reached its bookmark limit".into());
    }
    book.bookmarks.push(Bookmark {
        note: None,
        updated_at: None,
        chapter_title,
        created_at: crate::mark_time::now(),
        label,
        position,
    });
    Ok(())
}

impl Reader {
    fn overview_probe(&self, chapter: usize) -> Option<&Probe> {
        let overview = self
            .overview
            .as_ref()
            .filter(|overview| overview.chapter == Some(chapter))?;
        overview.preview.as_deref().or_else(|| {
            let active = self.active.as_ref()?;
            (active.reading.chapter == chapter).then_some(&active.reading.probe)
        })
    }

    fn overview_bookmark_index(&self, chapter: Option<usize>, page: usize) -> Option<usize> {
        if chapter.is_some_and(|chapter| {
            self.active.as_ref().is_some_and(|active| {
                active
                    .reading
                    .probe
                    .chapter_entries()
                    .get(chapter)
                    .is_some_and(|entry| entry.unsupported)
            })
        }) {
            return None;
        }
        let book = self.library.state(self.open_book_id()?)?;
        bookmark_index_for_page(
            book,
            chapter,
            page,
            chapter.and_then(|chapter| self.overview_probe(chapter)),
        )
    }

    pub(super) fn draw_overview_bookmark(
        &self,
        canvas: &mut Canvas,
        tile: Rect,
        chapter: Option<usize>,
        page: usize,
    ) {
        if self.overview_bookmark_index(chapter, page).is_some() {
            let size = (tile.h / 3).min(54);
            let inset = (tile.h / 12).min(18);
            canvas.icon(
                Rect::new(tile.right() - size - inset - 1, tile.y + inset, size, size),
                bar_icons::BOOKMARK,
                BLACK,
            );
        }
    }

    pub(super) fn hold_overview_page(&mut self, chapter: Option<usize>, page: usize) -> Result<()> {
        let chapter_edit = (chapter.is_none() && self.custom_contents_enabled())
            .then(|| self.has_fixed_thumbnail_chapter(page));
        let flow_edit = self.flow_editing_enabled();
        if chapter_edit.is_none() && !flow_edit {
            return self.toggle_overview_bookmark(chapter, page);
        }
        self.flow_popup = Some(hidden_flows::FlowPopup::Overview(PageActions {
            chapter,
            page,
            bookmarked: self.overview_bookmark_index(chapter, page).is_some(),
            chapter_edit,
            flow_edit,
        }));
        Ok(())
    }

    fn toggle_overview_bookmark(&mut self, chapter: Option<usize>, page: usize) -> Result<()> {
        if chapter.is_some_and(|chapter| {
            self.active.as_ref().is_some_and(|active| {
                active
                    .reading
                    .probe
                    .chapter_entries()
                    .get(chapter)
                    .is_some_and(|entry| entry.unsupported)
            })
        }) {
            return Err("An unsupported chapter notice cannot be bookmarked".into());
        }
        let bookmark = self.overview_bookmark_index(chapter, page);
        let (position, label) = if let Some(chapter) = chapter {
            let probe = self
                .overview_probe(chapter)
                .filter(|probe| page < probe.page_count())
                .ok_or("That page is unavailable")?;
            let position = ReadingPosition {
                chapter,
                page,
                anchor: probe.page_anchor(page),
            };
            let label = format!("{} · Page {}", probe.chapter_label, page + 1);
            (position, label)
        } else {
            let fixed = self
                .fixed
                .as_ref()
                .filter(|fixed| page < fixed.reading.count())
                .ok_or("That page is unavailable")?;
            let position = ReadingPosition {
                chapter: 0,
                page,
                anchor: None,
            };
            (position, fixed_page_label(&fixed.reading, page))
        };
        let chapter_title = bookmark
            .is_none()
            .then(|| {
                self.bookmark_chapter_title(
                    &position,
                    chapter.and_then(|chapter| self.overview_probe(chapter)),
                )
            })
            .flatten();
        let id = self.open_book_id().ok_or("Open a book first")?;
        let book = self
            .library
            .state_mut(id)
            .ok_or("This book is unavailable")?;
        if let Some(index) = bookmark {
            book.bookmarks.remove(index);
        } else {
            push_page_bookmark(book, position, label, chapter_title)?;
        }
        self.mark_book_changed();
        self.start_overview_raw_verification();
        self.notice = None;
        Ok(())
    }
}

#[cfg(test)]
pub(super) fn tap_page_action(reader: &mut Reader, action: PageAction) {
    let Some(hidden_flows::FlowPopup::Overview(actions)) = &reader.flow_popup else {
        panic!("Expected a page action menu");
    };
    let rect = actions.button(action).expect("Page action unavailable");
    assert_eq!(
        reader.input(Input::Tap(
            (rect.x + rect.w / 2) as f32,
            (rect.y + rect.h / 2) as f32,
        )),
        Change::Partial,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::{Fixture, capture_test_screen};

    fn open_test_book(fixture: &Fixture, path: &Path) -> Reader {
        let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
        let mut reader = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
        reader.open_path(path).unwrap();
        reader
    }

    fn hold_tile(reader: &mut Reader, index: usize) {
        let tile = overview_tile(index);
        assert_eq!(
            reader.input(Input::LongTap(
                (tile.x + tile.w / 2) as f32,
                (tile.y + tile.h / 2) as f32,
            )),
            Change::Partial,
        );
    }

    fn marks(reader: &Reader) -> &[Bookmark] {
        &reader
            .library
            .state(reader.open_book_id().unwrap())
            .unwrap()
            .bookmarks
    }

    #[test]
    fn overview_bookmarks_toggle_only_the_held_pdf_source_page_and_keep_jump_history() {
        let fixture = Fixture::new();
        let path = fixture.root.join("Survey sheets.pdf");
        let source = crate::fixed_session::tests::striped_pdf_bytes(8);
        fs::write(&path, &source).unwrap();
        let mut reader = open_test_book(&fixture, &path);
        reader.follow_fixed(1, None).unwrap();
        reader.activate(Action::ToggleBookmark).unwrap();
        let origin = reader.current_history_location().unwrap();
        let history = reader.back.clone();
        let original_mark = marks(&reader)[0].clone();
        reader.activate(Action::Overview).unwrap();
        assert_eq!(reader.input(Input::PageForward), Change::Partial);
        let tile_start = reader.overview.as_ref().unwrap().first;
        assert_eq!(tile_start, 4);
        let font_paths: Vec<_> = reader
            .fonts
            .iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect();
        let mut canvas = Canvas::new(WIDTH, HEIGHT, &font_paths).unwrap();
        let before = primitive_app::frame(&reader, &mut canvas, false);
        hold_tile(&mut reader, 1);
        assert!(reader.flow_popup.is_none());
        assert_eq!(reader.screen, Screen::PageOverview);
        assert_eq!(reader.overview.as_ref().unwrap().first, tile_start);
        assert_eq!(reader.current_history_location(), Some(origin.clone()));
        assert_eq!(reader.back, history);
        assert_eq!(marks(&reader)[0], original_mark);
        assert_eq!(
            marks(&reader)[1].position,
            ReadingPosition {
                chapter: 0,
                page: 5,
                anchor: None
            }
        );
        assert_eq!(reader.overview_bookmark_index(None, 5), Some(1));
        assert!(reader.overview_bookmark_index(None, 4).is_none());
        let bookmarked = primitive_app::frame(&reader, &mut canvas, false);
        assert_ne!(bookmarked, before);
        let tile = overview_tile(1);
        assert!(
            before
                .iter()
                .zip(&bookmarked)
                .enumerate()
                .all(|(pixel, (before, after))| {
                    let (x, y) = ((pixel % WIDTH as usize) as f32, (pixel / WIDTH as usize) as f32);
                    // The skim track shows the saved mark for the bookmarked page.
                    let skim_mark = (1320.0..1370.0).contains(&y);
                    before == after || tile.contains(x, y) || skim_mark
                })
        );
        capture_test_screen(
            &reader,
            &reader.fonts,
            "READER_PDF_THUMBNAIL_BOOKMARK",
            false,
        );
        hold_tile(&mut reader, 1);
        assert_eq!(primitive_app::frame(&reader, &mut canvas, false), before);
        assert_eq!(marks(&reader), std::slice::from_ref(&original_mark));
        hold_tile(&mut reader, 1);
        reader.save().unwrap();

        let mut reopened = open_test_book(&fixture, &path);
        assert_eq!(reopened.current_history_location(), Some(origin.clone()));
        assert_eq!(marks(&reopened)[0], original_mark);
        reopened.show(Screen::FixedBookmarks);
        let bookmark = reopened
            .items()
            .into_iter()
            .find(|item| item.title == "Page 6")
            .unwrap();
        reopened.activate(bookmark.action).unwrap();
        assert_eq!(reopened.current_position().unwrap().page, 5);
        reopened.activate(Action::ReturnToLocation).unwrap();
        assert_eq!(reopened.current_history_location(), Some(origin));
        assert_eq!(fs::read(&path).unwrap(), source);
    }

    #[test]
    fn overview_bookmarks_use_the_other_epub_chapters_anchor_after_repagination() {
        let fixture = Fixture::new();
        nonlinear_tests::authored_book(&fixture.book);
        let mut reader = open_test_book(&fixture, &fixture.book);
        reader.activate(Action::ToggleBookmark).unwrap();
        let origin = reader.current_history_location().unwrap();
        let original_mark = marks(&reader)[0].clone();
        reader.activate(Action::Overview).unwrap();
        for _ in 0..2 {
            assert_eq!(reader.input(Input::PageForward), Change::Partial);
        }
        let (chapter, preview_page) = (2, 2);
        assert_eq!(reader.overview.as_ref().unwrap().chapter, Some(chapter));
        assert!(reader.overview_bookmark_index(Some(chapter), 0).is_none());
        let anchor = reader
            .overview_probe(chapter)
            .unwrap()
            .page_anchor(preview_page)
            .unwrap();
        hold_tile(&mut reader, preview_page);
        assert!(reader.flow_popup.is_none());
        assert_eq!(reader.current_history_location(), Some(origin));
        assert!(reader.back.is_empty());
        assert_eq!(
            reader.overview_bookmark_index(Some(chapter), preview_page),
            Some(1)
        );
        assert_eq!(
            marks(&reader)[1].position,
            ReadingPosition {
                chapter,
                page: preview_page,
                anchor: Some(anchor.clone())
            }
        );
        capture_test_screen(
            &reader,
            &reader.fonts,
            "READER_EPUB_THUMBNAIL_BOOKMARK",
            false,
        );

        let owner = reader.open_book_id().unwrap();
        reader
            .activate(Action::ApplyLogicalFont(owner, 36.0))
            .unwrap();
        reader.save().unwrap();
        let mut reopened = open_test_book(&fixture, &fixture.book);
        let before = reopened.current_history_location().unwrap();
        reopened.activate(Action::Bookmark(1)).unwrap();
        let reading = &reopened.active.as_ref().unwrap().reading;
        assert_eq!(reading.chapter, chapter);
        assert_ne!(reading.page, preview_page);
        assert_eq!(reading.probe.page_for(&anchor), Some(reading.page));
        assert_eq!(reopened.current_bookmark_index(), Some(1));
        let page = reading.page;
        reopened.activate(Action::Overview).unwrap();
        let tile = page - reopened.overview.as_ref().unwrap().first;
        hold_tile(&mut reopened, tile);
        assert_eq!(marks(&reopened), std::slice::from_ref(&original_mark));
        reopened.activate(Action::ReturnToLocation).unwrap();
        assert_eq!(reopened.current_history_location(), Some(before));
    }
}
