//! The offline Quickstart guide sits above the active reading or Documents view.
use std::{io, path::Path};

use primitive_ui::{PageTurn, Pager};

use super::{
    BODY, Canvas, Change, Input, Item, LIST_X, LIST_Y, Reader, Rect, Row, RowAccessory, Rows,
    navigation_layout::NavigationLayout, with_storage,
};

pub(super) const TITLE: &str = "Quickstart guide";
const SEEN_FILE: &str = "quickstart-seen";
const TOPIC_ROW_HEIGHT: i32 = 116;
const GUIDE_PARAGRAPH_HEIGHT: i32 = 220;

struct Topic {
    name: &'static str,
    summary: &'static str,
    sections: [(&'static str, &'static str); 3],
}

const TOPICS: [Topic; 5] = [
    Topic {
        name: "Open a document",
        summary: "Browse folders, find files, and resume reading",
        sections: [
            (
                "Browse",
                "In Documents, tap a folder to open it and tap an EPUB or PDF to read it. Use Up to return to the parent folder. Find searches the books in Documents.",
            ),
            (
                "Resume",
                "At the top level of Documents, Continue reading opens your most recent book at its saved position. Organize opens lists such as recent books and collections.",
            ),
            (
                "Choose a format",
                "EPUB text adapts to the screen and your reading settings. PDF keeps the printed page layout and offers fit, zoom, and panning.",
            ),
        ],
    },
    Topic {
        name: "Move through a book",
        summary: "Page turns, contents, search, and bookmarks",
        sections: [
            (
                "Turn pages",
                "In paged view, use the Kindle page buttons, tap a side of the page, or swipe horizontally. Page direction and touch zones can be set under Page turns.",
            ),
            (
                "Find a place",
                "Tap near the bottom or swipe up from the bottom edge to open the lower toolbar. Use Contents for the outline, Go to for a page number, or Find for a word or phrase.",
            ),
            (
                "Keep your place",
                "Open the top toolbar and choose Navigation, then Add bookmark. Tap Bookmarks in the lower toolbar to return to a mark. Location history can take you back after a jump.",
            ),
        ],
    },
    Topic {
        name: "Select and save text",
        summary: "Look up a word, highlight a passage, or add a note",
        sections: [
            (
                "Select",
                "Hold a word, then drag before lifting to select more text. With the default single-word behavior, holding just one word opens the dictionary. Long-press on text changes what a hold does.",
            ),
            (
                "Highlight or take a note",
                "Choose Highlight or Note in the selection menu. Reopen your saved passages from Highlights in the lower toolbar, or from Saved marks in the top Tools menu.",
            ),
            (
                "Look up a word",
                "Choose Dictionary for a selection or use Dictionary lookup in the top Search menu. The dictionary panel closes back to your passage. A scanned page may need text recognition first.",
            ),
        ],
    },
    Topic {
        name: "Adjust the reading view",
        summary: "Text size, PDF view, and reading controls",
        sections: [
            (
                "EPUB text and layout",
                "Open the lower toolbar and choose Text to change type size, font, spacing, or layout. Use the top Settings tab for general reading preferences.",
            ),
            (
                "PDF view and scans",
                "Choose View in the lower toolbar for page fit, zoom, or scrolling where supported. Drag to pan a zoomed page. Use the Scan tools for text recognition on scanned PDFs when available.",
            ),
            (
                "Menus and progress",
                "Use Controls to change Menu activation or the Reading status bar. The footer reports your location in the book. The top Search tab offers Menu search when you know a control's name.",
            ),
        ],
    },
    Topic {
        name: "Read offline and exit",
        summary: "Local files, offline dictionaries, and saving your place",
        sections: [
            (
                "Read local files",
                "Copy books to the Kindle's Documents folder. Local reading, bookmarks, and notes work without Wi-Fi. Online lookup and translation need a connection.",
            ),
            (
                "Install a dictionary",
                "Copy your StarDict files into reader/data/dictionaries on the Kindle. Open Controls, then Dictionaries, to enable a source. Installed dictionaries work offline.",
            ),
            (
                "Leave Reader",
                "Choose Documents in the top toolbar to browse files while staying in Reader. Use Exit on the reading toolbar or reading footer to save your place and leave Reader.",
            ),
        ],
    },
];

#[derive(Clone, Copy, Default)]
pub(super) struct Guide {
    page: usize,
}

impl Guide {
    fn index_rows() -> [Row<'static>; TOPICS.len()] {
        TOPICS.each_ref().map(|topic| Row {
            title: topic.name,
            detail: Some(topic.summary),
            accessory: RowAccessory::Disclosure,
        })
    }

    fn pager(self, layout: NavigationLayout) -> Pager {
        Pager {
            bounds: layout.footer_text(LIST_X, 128, 100),
            page: self.page,
            pages: TOPICS.len() + 1,
        }
    }

    fn topic_rows<'a>(layout: NavigationLayout, rows: &'a [Row<'a>]) -> Rows<'a> {
        Rows {
            bounds: layout.body(LIST_Y + 30, TOPICS.len() as i32 * TOPIC_ROW_HEIGHT),
            rows,
        }
    }

    fn topics_button(layout: NavigationLayout) -> Rect {
        Rect::new(layout.width - 260, LIST_Y + 20, 220, 100)
    }

    fn draw(self, canvas: &mut Canvas, layout: NavigationLayout) {
        layout.header_action(canvas, TITLE, "Done");
        if self.page == 0 {
            Self::topic_rows(layout, &Self::index_rows()).draw(canvas, None);
        } else {
            let topic = &TOPICS[self.page - 1];
            canvas.text(
                Rect::new(LIST_X, LIST_Y + 20, layout.width - 340, 100),
                topic.name,
                BODY.bold(),
            );
            canvas.button(Self::topics_button(layout), "Topics", false, false);
            let card = layout.body(LIST_Y + 148, 3 * GUIDE_PARAGRAPH_HEIGHT + 24);
            canvas.card(card);
            for (index, (title, text)) in topic.sections.iter().enumerate() {
                let y = card.y + 16 + index as i32 * GUIDE_PARAGRAPH_HEIGHT;
                canvas.text(
                    Rect::new(card.x + 32, y, card.w - 64, 55),
                    title,
                    BODY.bold(),
                );
                canvas.text(
                    Rect::new(card.x + 32, y + 60, card.w - 64, 148),
                    text,
                    BODY.lines(3),
                );
            }
        }
        self.pager(layout).draw(canvas, None);
    }

    fn next_page(self, input: Input, layout: NavigationLayout) -> Option<usize> {
        let turn = match input {
            Input::Tap(x, y) => self.pager(layout).hit(x, y),
            Input::PageBack => Some(PageTurn::Previous),
            Input::PageForward => Some(PageTurn::Next),
            Input::Swipe { dx, dy, .. } if dx.abs() > dy.abs() => Some(if dx < 0.0 {
                PageTurn::Next
            } else {
                PageTurn::Previous
            }),
            _ => None,
        };
        match turn {
            Some(PageTurn::Previous) => self.page.checked_sub(1),
            Some(PageTurn::Next) if self.page < TOPICS.len() => Some(self.page + 1),
            _ => None,
        }
    }
}

impl Reader {
    pub(super) fn quickstart_item() -> Item {
        Item::new(TITLE, super::Action::OpenQuickstart).detail("Open the built-in reading guide")
    }

    pub(super) fn open_quickstart(&mut self) {
        self.quickstart = Some(Guide::default());
    }

    pub(super) fn show_first_run_quickstart(&mut self, library_path: &Path) -> io::Result<()> {
        let seen = self.controls_path.with_file_name(SEEN_FILE);
        let first = with_storage(|| {
            Ok(!seen.try_exists()?
                && !library_path.try_exists()?
                && !self.controls_path.try_exists()?)
        })?;
        if first {
            self.open_quickstart();
            primitive_app::save(&seen, b"1\n")?;
        }
        Ok(())
    }

    pub(super) fn draw_quickstart(&self, canvas: &mut Canvas) -> bool {
        let Some(guide) = self.quickstart else {
            return false;
        };
        guide.draw(canvas, self.navigation_layout());
        true
    }

    pub(super) fn quickstart_input(&mut self, input: Input) -> Option<Change> {
        if matches!(input, Input::Sleep | Input::Wake) {
            return None;
        }
        let mut guide = self.quickstart?;
        let layout = self.navigation_layout();
        if let Input::Tap(x, y) = input {
            if layout.back().contains(x, y) {
                self.quickstart = None;
                return Some(Change::Full);
            }
            if guide.page > 0 && Guide::topics_button(layout).contains(x, y) {
                self.quickstart = Some(Guide::default());
                return Some(Change::Partial);
            }
            if guide.page == 0 {
                if let Some(topic) = Guide::topic_rows(layout, &Guide::index_rows()).hit(x, y) {
                    guide.page = topic + 1;
                    self.quickstart = Some(guide);
                    return Some(Change::Partial);
                }
            }
        }
        if let Some(page) = guide.next_page(input, layout) {
            guide.page = page;
            self.quickstart = Some(guide);
            return Some(Change::Partial);
        }
        Some(Change::None)
    }
}

#[cfg(test)]
mod tests;
