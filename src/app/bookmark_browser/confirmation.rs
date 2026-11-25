//! Floating confirmations for changes to reviewed saved marks.
use super::{BrowserLayout, Canvas, Change, Command, Input, Page, Reader, Rect, Screen, edit_note};
use primitive_ui::{BODY, CAPTION};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Confirmation {
    Notes,
    Marks,
    ClearNote,
}

fn geometry(reader: &Reader) -> (Rect, Rect, Rect) {
    let layout = BrowserLayout(reader.navigation_layout()).0;
    let width = (layout.width - 80).min(820).max(200);
    let height = (layout.height - 80).min(460).max(300);
    let panel = Rect::new(
        (layout.width - width) / 2,
        (layout.height - height) / 2,
        width,
        height,
    );
    let button_width = (width - 108) / 2;
    let cancel = Rect::new(panel.x + 36, panel.bottom() - 126, button_width, 96);
    let remove = Rect::new(cancel.right() + 36, cancel.y, button_width, 96);
    (panel, cancel, remove)
}

fn active(reader: &Reader) -> Option<Confirmation> {
    let confirmation = reader.bookmark_browser.confirmation?;
    let correct = matches!(
        (confirmation, reader.screen),
        (
            Confirmation::Notes,
            Screen::BookmarkBrowser(Page::RemoveNotesConfirm)
        ) | (
            Confirmation::Marks,
            Screen::BookmarkBrowser(Page::RemoveMarksConfirm)
        ) | (
            Confirmation::ClearNote,
            Screen::BookmarkBrowser(Page::EditNote)
        )
    );
    correct.then_some(confirmation)
}

pub(super) fn draw(reader: &Reader, canvas: &mut Canvas) {
    let Some(confirmation) = active(reader) else {
        return;
    };
    let language = reader.controls.interface_language;
    let (panel, cancel, remove) = geometry(reader);
    let (title, consequence, count, button) = match confirmation {
        Confirmation::Notes => (
            "Remove notes",
            "Bookmarks and highlights will remain",
            reader
                .bookmark_browser
                .pending_note_removal
                .as_ref()
                .map_or(0, |p| p.selected_count()),
            "Delete notes",
        ),
        Confirmation::Marks => (
            "Remove marks",
            "Selected marks and their notes will be deleted",
            reader
                .bookmark_browser
                .pending_mark_removal
                .as_ref()
                .map_or(0, |p| p.selected_count()),
            "Delete marks",
        ),
        Confirmation::ClearNote => (
            "Remove note",
            "Remove this note? The saved mark will remain.",
            1,
            "Remove note",
        ),
    };
    canvas.popup(panel);
    canvas.text(
        Rect::new(panel.x + 36, panel.y + 24, panel.w - 72, 58),
        language.bookmark_text(title),
        BODY.bold(),
    );
    let prompt = match confirmation {
        Confirmation::Notes if language.is_english() => {
            format!("Delete {count} selected notes in this book?")
        }
        Confirmation::Notes => format!("Xóa {count} ghi chú đã chọn trong sách này?"),
        Confirmation::Marks if language.is_english() => {
            format!("Delete {count} selected marks in this book?")
        }
        Confirmation::Marks => format!("Xóa {count} dấu đã chọn trong sách này?"),
        Confirmation::ClearNote => language.bookmark_text(consequence).to_owned(),
    };
    canvas.text(
        Rect::new(panel.x + 36, panel.y + 94, panel.w - 72, 80),
        &prompt,
        BODY.lines(2),
    );
    if confirmation != Confirmation::ClearNote {
        canvas.text(
            Rect::new(panel.x + 36, panel.y + 180, panel.w - 72, 70),
            language.bookmark_text(consequence),
            CAPTION.lines(2),
        );
    }
    if let Some(notice) = &reader.notice {
        canvas.text(
            Rect::new(panel.x + 36, panel.bottom() - 205, panel.w - 72, 65),
            notice,
            CAPTION.lines(2),
        );
    }
    canvas.button(cancel, language.bookmark_text("Cancel"), false, false);
    canvas.button(remove, language.bookmark_text(button), true, false);
}

fn cancel(reader: &mut Reader, confirmation: Confirmation) {
    reader.bookmark_browser.confirmation = None;
    if confirmation == Confirmation::ClearNote {
        edit_note::cancel_confirmation(reader);
    }
    reader.notice = None;
}

pub(super) fn input(reader: &mut Reader, input: Input) -> Option<Change> {
    let confirmation = active(reader)?;
    let (_, cancel_button, remove_button) = geometry(reader);
    Some(match input {
        Input::Tap(x, y) if remove_button.contains(x, y) => {
            let command = match confirmation {
                Confirmation::Notes => Command::ConfirmRemoveCurrentNotes,
                Confirmation::Marks => Command::ConfirmRemoveCurrentMarks,
                Confirmation::ClearNote => Command::ConfirmClearCurrentNote,
            };
            reader.bookmark_change(command)
        }
        Input::Tap(x, y) if cancel_button.contains(x, y) => {
            cancel(reader, confirmation);
            Change::Partial
        }
        Input::Tap(x, y) if !geometry(reader).0.contains(x, y) => {
            cancel(reader, confirmation);
            Change::Partial
        }
        Input::PageBack => {
            cancel(reader, confirmation);
            Change::Partial
        }
        _ => Change::None,
    })
}
