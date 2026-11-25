//! Edit one reviewed current-book note without moving the reading position.
use super::{
    BrowserLayout, Change, Input, MarkKey, Page, Reader, Rect, Result, Screen, bulk_marks::Snapshot,
};
use primitive_ui::{CAPTION, Canvas, editor::Multiline, keyboard_key};

const COLUMNS: usize = 40;
const LINE_HEIGHT: i32 = 48;

pub(super) struct Pending {
    book: u64,
    key: MarkKey,
    pub(super) title: String,
    target: Snapshot,
    editor: Multiline,
    desired: Option<Option<String>>,
}

fn save_button(layout: BrowserLayout) -> Rect {
    layout
        .0
        .horizontal(Rect::new(layout.0.width - 300, 160, 260, 90))
}

fn text_area(layout: BrowserLayout) -> (Rect, usize) {
    let keyboard = layout.0.keyboard();
    let rows = ((keyboard.keyboard_area().y - 276) / LINE_HEIGHT).max(1) as usize;
    (layout.0.body(260, rows as i32 * LINE_HEIGHT), rows)
}

pub(super) fn can_edit(reader: &Reader) -> bool {
    reader
        .bookmark_browser
        .focus
        .is_some_and(|key| reader.open_book_id() == Some(key.book))
        && matches!(reader.bookmark_browser.origin, Screen::Book | Screen::Fixed)
}

pub(super) fn open(reader: &mut Reader) -> Result<()> {
    if reader.screen != Screen::BookmarkBrowser(Page::Detail) || !can_edit(reader) {
        return Err("Open this book before editing its note".into());
    }
    let key = reader
        .bookmark_browser
        .focus
        .ok_or("That mark is unavailable")?;
    let book = reader
        .library
        .state(key.book)
        .ok_or("That book is unavailable")?;
    let target = Snapshot::capture(book, key.locator)?;
    target.verify_source(reader)?;
    let mut editor = Multiline::new(target.note().unwrap_or(""), crate::library::MAX_NOTE_BYTES);
    editor.editor.caret = editor.editor.text.len();
    let (_, rows) = text_area(BrowserLayout(reader.navigation_layout()));
    editor.show_caret(COLUMNS, rows);
    reader.bookmark_browser.pending_note_edit = Some(Pending {
        book: key.book,
        key,
        title: book.title.clone(),
        target,
        editor,
        desired: None,
    });
    reader.notice = None;
    reader.show(Screen::BookmarkBrowser(Page::EditNote));
    Ok(())
}

pub(super) fn draw(reader: &Reader, canvas: &mut Canvas, layout: BrowserLayout) {
    let language = reader.controls.interface_language;
    let Some(pending) = &reader.bookmark_browser.pending_note_edit else {
        return;
    };
    canvas.button(
        save_button(layout),
        language.bookmark_text("Save note"),
        true,
        false,
    );
    let (area, _) = text_area(layout);
    canvas.panel(area.inset(-6), 8, 1, 255);
    pending.editor.draw(canvas, area, COLUMNS, LINE_HEIGHT, 28);
    if let Some(notice) = &reader.notice {
        canvas.text(layout.0.body(120, 35), notice, CAPTION);
    }
    canvas.keyboard(
        pending.editor.editor.layer,
        language.bookmark_text("New line"),
        None,
    );
}

fn request_save(reader: &mut Reader) -> Result<()> {
    if reader.screen != Screen::BookmarkBrowser(Page::EditNote) {
        return Err("Open the note editor first".into());
    }
    let pending = reader
        .bookmark_browser
        .pending_note_edit
        .as_mut()
        .ok_or("Open the note editor first")?;
    let value = pending.editor.editor.string();
    if value.len() > crate::library::MAX_NOTE_BYTES {
        return Err("The note exceeds the saved note size limit".into());
    }
    if value == pending.target.note().unwrap_or("") {
        reader.bookmark_browser.pending_note_edit = None;
        reader.show(Screen::BookmarkBrowser(Page::Detail));
        return Ok(());
    }
    let desired = (!value.trim().is_empty()).then_some(value);
    if desired.is_none() && pending.target.note().is_none() {
        reader.bookmark_browser.pending_note_edit = None;
        reader.show(Screen::BookmarkBrowser(Page::Detail));
        return Ok(());
    }
    let removing = desired.is_none() && pending.target.note().is_some();
    pending.desired = Some(desired);
    if removing {
        reader.bookmark_browser.confirmation = Some(super::confirmation::Confirmation::ClearNote);
        Ok(())
    } else {
        commit(reader)
    }
}

pub(super) fn cancel_confirmation(reader: &mut Reader) {
    if let Some(pending) = &mut reader.bookmark_browser.pending_note_edit {
        pending.desired = None;
    }
    reader.notice = None;
}

pub(super) fn resume(reader: &mut Reader) -> Result<()> {
    if reader.screen != Screen::BookmarkBrowser(Page::ConfirmClearNote) {
        return Err("Open the note editor first".into());
    }
    let pending = reader
        .bookmark_browser
        .pending_note_edit
        .as_mut()
        .ok_or("Open the note editor first")?;
    pending.desired = None;
    reader.notice = None;
    reader.show(Screen::BookmarkBrowser(Page::EditNote));
    Ok(())
}

pub(super) fn cancel(reader: &mut Reader) {
    reader.bookmark_browser.pending_note_edit = None;
    reader.notice = None;
    reader.show(Screen::BookmarkBrowser(Page::Detail));
}

pub(super) fn commit(reader: &mut Reader) -> Result<()> {
    let pending = reader
        .bookmark_browser
        .pending_note_edit
        .as_ref()
        .ok_or("Review the note first")?;
    let desired = pending
        .desired
        .as_ref()
        .ok_or("Review the note first")?
        .clone();
    let expected_screen = Page::EditNote;
    if desired.is_none()
        && reader.bookmark_browser.confirmation
            != Some(super::confirmation::Confirmation::ClearNote)
    {
        return Err("Confirm removal before deleting the note".into());
    }
    if reader.screen != Screen::BookmarkBrowser(expected_screen)
        || reader.open_book_id() != Some(pending.book)
        || reader.bookmark_browser.focus != Some(pending.key)
        || !matches!(reader.bookmark_browser.origin, Screen::Book | Screen::Fixed)
    {
        return Err("The open book changed; review the note again".into());
    }
    pending.target.verify_source(reader)?;
    let id = pending.book;
    let state = reader.library.state(id).ok_or("That book is unavailable")?;
    if state.title != pending.title || !pending.target.matches(state) {
        return Err("The mark changed; review the note again".into());
    }
    let before = state.clone();
    let state = reader
        .library
        .state_mut(id)
        .ok_or("That book is unavailable")?;
    pending
        .target
        .set_note(state, desired, crate::mark_time::now());
    if let Err(error) = reader.library.persist() {
        if let Some(state) = reader.library.state_mut(id) {
            *state = before;
        }
        return Err(error.into());
    }
    reader.bookmark_browser.pending_note_edit = None;
    reader.mark_book_changed();
    reader.refresh_bookmark_browser();
    reader.show(Screen::BookmarkBrowser(Page::Results));
    reader.notice = Some(
        reader
            .controls
            .interface_language
            .bookmark_text("Note saved")
            .into(),
    );
    Ok(())
}

pub(super) fn input(reader: &mut Reader, input: Input, layout: BrowserLayout) -> Option<Change> {
    if matches!(input, Input::Sleep | Input::Wake) {
        return None;
    }
    let result = (|| -> Result<Change> {
        let (area, rows) = text_area(layout);
        if let Input::Tap(x, y) = input {
            if save_button(layout).contains(x, y) {
                request_save(reader)?;
                return Ok(Change::Partial);
            }
            let pending = reader
                .bookmark_browser
                .pending_note_edit
                .as_mut()
                .ok_or("Open the note editor first")?;
            if area.contains(x, y) {
                pending.editor.place(
                    ((y - area.y as f32) / LINE_HEIGHT as f32) as usize,
                    ((x - area.x as f32) / 18.) as usize,
                    COLUMNS,
                );
            } else {
                let label = reader.controls.interface_language.bookmark_text("New line");
                let Some(key) =
                    keyboard_key(&layout.0.keyboard(), &pending.editor.editor, label, x, y)
                else {
                    return Ok(Change::None);
                };
                pending.editor.press(key);
                pending.editor.show_caret(COLUMNS, rows);
            }
        } else {
            let delta = match input {
                Input::PageForward => 1,
                Input::PageBack => -1,
                Input::Swipe { dy, .. } if dy.abs() > 60. => {
                    if dy < 0. {
                        1
                    } else {
                        -1
                    }
                }
                _ => return Ok(Change::None),
            };
            let pending = reader
                .bookmark_browser
                .pending_note_edit
                .as_mut()
                .ok_or("Open the note editor first")?;
            pending.editor.page(delta, COLUMNS, rows);
        }
        Ok(Change::Partial)
    })();
    Some(match result {
        Ok(change) => change,
        Err(error) => reader.report(error),
    })
}
