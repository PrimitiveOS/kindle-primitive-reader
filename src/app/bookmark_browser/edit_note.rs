//! Edit one reviewed current-book note without moving the reading position.
use super::{
    BrowserLayout, Change, Input, MarkKey, Page, Reader, Rect, Result, Screen, bulk_marks::Snapshot,
};
use primitive_ui::{CAPTION, Canvas, editor::Multiline, keyboard_key};

const COLUMNS: usize = 40;
const LINE_HEIGHT: i32 = 48;

pub(super) struct Pending {
    pub(super) book: u64,
    pub(super) key: MarkKey,
    pub(super) title: String,
    pub(super) target: Snapshot,
    editor: Multiline,
    pub(super) desired: Option<Option<String>>,
    pub(super) other_path: Option<std::path::PathBuf>,
    pub(super) other_digest: Option<[u8; 32]>,
    pub(super) other_epub_context: Option<super::other_epub_check::Context>,
    pub(super) other_epub_fonts: Option<Vec<crate::session::screen_pages::FileStamp>>,
    pub(super) origin_book: Option<u64>,
    pub(super) origin: Option<super::super::HistoryLocation>,
    pub(super) origin_fixed_view: Option<crate::library::FixedPreferences>,
}

fn action_width(layout: BrowserLayout, has_quote: bool, has_paste: bool) -> i32 {
    if has_quote && has_paste {
        ((layout.0.width - 104) / 3).min(260).max(1)
    } else {
        ((layout.0.width - 96) / 2).min(260).max(1)
    }
}

fn save_button(layout: BrowserLayout, has_quote: bool, has_paste: bool) -> Rect {
    let width = action_width(layout, has_quote, has_paste);
    layout
        .0
        .horizontal(Rect::new(layout.0.width - 40 - width, 150, width, 100))
}

fn quote_button(layout: BrowserLayout, has_quote: bool, has_paste: bool) -> Rect {
    let width = action_width(layout, has_quote, has_paste);
    layout.0.horizontal(Rect::new(40, 150, width, 100))
}

fn paste_button(layout: BrowserLayout, has_quote: bool, has_paste: bool) -> Rect {
    let width = action_width(layout, has_quote, has_paste);
    let x = if has_quote {
        (layout.0.width - width) / 2
    } else {
        40
    };
    layout.0.horizontal(Rect::new(x, 150, width, 100))
}

fn saved_quote(target: &Snapshot) -> Option<&str> {
    let quote = match target {
        Snapshot::Bookmark(..) => return None,
        Snapshot::Reflow(_, mark) => &mark.selection.text,
        Snapshot::Fixed(_, mark) => &mark.text,
    };
    (!quote.is_empty()).then_some(quote)
}

fn text_area(layout: BrowserLayout) -> (Rect, usize) {
    let keyboard = layout.0.keyboard();
    let rows = ((keyboard.keyboard_area().y - 276) / LINE_HEIGHT).max(1) as usize;
    (layout.0.body(260, rows as i32 * LINE_HEIGHT), rows)
}

pub(super) fn can_edit_current(reader: &Reader) -> bool {
    reader
        .bookmark_browser
        .focus
        .is_some_and(|key| reader.open_book_id() == Some(key.book))
        && matches!(reader.bookmark_browser.origin, Screen::Book | Screen::Fixed)
}

pub(super) fn can_edit(reader: &Reader) -> bool {
    if can_edit_current(reader) {
        return true;
    }
    reader.bookmark_browser.focus.is_some_and(|key| {
        reader.open_book_id() != Some(key.book)
            && reader.library.state(key.book).is_some_and(|book| {
                match crate::library::DocumentFormat::for_path(&book.path) {
                    Some(crate::library::DocumentFormat::Pdf) => matches!(
                        key.locator,
                        super::Locator::Bookmark(_) | super::Locator::Fixed(_)
                    ),
                    Some(crate::library::DocumentFormat::Epub) => {
                        matches!(
                            key.locator,
                            super::Locator::Reflow(_) | super::Locator::Bookmark(_)
                        ) && !crate::epub_members::is_folder_book(&book.path)
                    }
                    _ => false,
                }
            })
    })
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
    let other_path = (reader.open_book_id() != Some(key.book)).then(|| book.path.clone());
    let verify_async =
        other_path.is_some() || super::note_verify::uses_reflow_worker(reader, &target);
    if !verify_async {
        target.verify_source(reader)?;
    }
    let mut editor = Multiline::new(target.note().unwrap_or(""), crate::library::MAX_NOTE_BYTES);
    editor.editor.caret = editor.editor.text.len();
    let (_, rows) = text_area(BrowserLayout(reader.navigation_layout()));
    editor.show_caret(COLUMNS, rows);
    let title = book.title.clone();
    let origin_book = reader.open_book_id();
    let origin = reader.current_history_location();
    let origin_fixed_view = reader
        .fixed
        .as_ref()
        .map(|fixed| fixed.reading.preferences());
    reader.bookmark_browser.pending_note_edit = Some(Pending {
        book: key.book,
        key,
        title,
        target,
        editor,
        desired: None,
        other_path,
        other_digest: None,
        other_epub_context: None,
        other_epub_fonts: None,
        origin_book,
        origin,
        origin_fixed_view,
    });
    reader.notice = None;
    if verify_async {
        let result = if reader
            .bookmark_browser
            .pending_note_edit
            .as_ref()
            .is_some_and(|pending| pending.other_path.is_some())
        {
            reader.start_other_note_verification(super::note_verify::Intent::Open)
        } else {
            reader.start_note_verification(super::note_verify::Intent::Open)
        };
        if let Err(error) = result {
            reader.bookmark_browser.pending_note_edit = None;
            return Err(error);
        }
    } else {
        reader.show(Screen::BookmarkBrowser(Page::EditNote));
    }
    Ok(())
}

pub(super) fn draw(reader: &Reader, canvas: &mut Canvas, layout: BrowserLayout) {
    let language = reader.controls.interface_language;
    let Some(pending) = &reader.bookmark_browser.pending_note_edit else {
        return;
    };
    let has_quote = saved_quote(&pending.target).is_some();
    let has_paste = reader.clipboard_text.is_some();
    if has_quote {
        canvas.button(
            quote_button(layout, has_quote, has_paste),
            language.bookmark_text(if has_paste { "Quote" } else { "Insert quote" }),
            false,
            false,
        );
    }
    if has_paste {
        canvas.button(
            paste_button(layout, has_quote, has_paste),
            language.bookmark_text("Paste"),
            false,
            false,
        );
    }
    canvas.button(
        save_button(layout, has_quote, has_paste),
        language.bookmark_text(if has_quote && has_paste {
            "Save"
        } else {
            "Save note"
        }),
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
    commit_inner(reader, false)
}

pub(super) fn commit_verified(reader: &mut Reader) -> Result<()> {
    commit_inner(reader, true)
}

fn commit_inner(reader: &mut Reader, verified: bool) -> Result<()> {
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
        || reader.open_book_id() != pending.origin_book
        || (pending.other_path.is_some()
            && (reader.current_history_location() != pending.origin
                || reader
                    .fixed
                    .as_ref()
                    .map(|fixed| fixed.reading.preferences())
                    != pending.origin_fixed_view))
        || reader.bookmark_browser.focus != Some(pending.key)
        || (pending.other_path.is_none()
            && !matches!(reader.bookmark_browser.origin, Screen::Book | Screen::Fixed))
    {
        return Err("The open book changed; review the note again".into());
    }
    let verify_async = pending.other_path.is_some()
        || super::note_verify::uses_reflow_worker(reader, &pending.target);
    if pending.other_path.is_some() && pending.other_digest.is_none() {
        return Err("The book has not been verified; review the note again".into());
    }
    if verify_async && !verified {
        return if pending.other_path.is_some() {
            reader.start_other_note_verification(super::note_verify::Intent::Save)
        } else {
            reader.start_note_verification(super::note_verify::Intent::Save)
        };
    }
    if !verify_async {
        pending.target.verify_source(reader)?;
    }
    let id = pending.book;
    let state = reader.library.state(id).ok_or("That book is unavailable")?;
    if state.title != pending.title
        || !pending.target.matches(state)
        || pending
            .other_path
            .as_ref()
            .is_some_and(|path| &state.path != path)
    {
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
            let pending = reader
                .bookmark_browser
                .pending_note_edit
                .as_ref()
                .ok_or("Open the note editor first")?;
            let has_quote = saved_quote(&pending.target).is_some();
            let has_paste = reader.clipboard_text.is_some();
            if save_button(layout, has_quote, has_paste).contains(x, y) {
                request_save(reader)?;
                return Ok(Change::Partial);
            }
            let quote_pressed =
                has_quote && quote_button(layout, has_quote, has_paste).contains(x, y);
            let paste_pressed =
                has_paste && paste_button(layout, has_quote, has_paste).contains(x, y);
            if quote_pressed || paste_pressed {
                let clipboard = if paste_pressed {
                    reader.clipboard_text.as_deref()
                } else {
                    None
                };
                let pending = reader
                    .bookmark_browser
                    .pending_note_edit
                    .as_mut()
                    .ok_or("Open the note editor first")?;
                let Some(text) = (if paste_pressed {
                    clipboard
                } else {
                    saved_quote(&pending.target)
                }) else {
                    return Ok(Change::None);
                };
                let editor = &mut pending.editor.editor;
                let current_bytes: usize = editor.text.iter().map(|ch| ch.len_utf8()).sum();
                if current_bytes.saturating_add(text.len()) > crate::library::MAX_NOTE_BYTES {
                    return Err("The note exceeds the saved note size limit".into());
                }
                let caret = editor.caret.min(editor.text.len());
                let added = text.chars().count();
                editor.text.splice(caret..caret, text.chars());
                editor.caret = caret + added;
                pending.editor.show_caret(COLUMNS, rows);
                reader.notice = None;
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
