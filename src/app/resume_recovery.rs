//! Explicit recovery when an EPUB's saved position cannot be restored.
use super::*;
use crate::session::screen_pages::FileStamp;

#[derive(Clone)]
pub(super) struct Recovery {
    pub(super) entry: LibraryBook,
    pub(super) id: u64,
    pub(super) position: ReadingPosition,
    pub(super) stamp: FileStamp,
}

impl Reader {
    pub(super) fn offer_resume_recovery(
        &mut self,
        entry: &LibraryBook,
        id: u64,
        position: &ReadingPosition,
        config: &BookConfig,
    ) -> bool {
        if entry.format != DocumentFormat::Epub
            || crate::epub_members::is_folder_book(&entry.path)
            || self.library.has_raw_whitespace_state(id)
            || self.pending_folder_snapshot.is_some()
            || self.pending_folder_mark.is_some()
            || self.pending_folder_chapter.is_some()
        {
            return false;
        }
        let Some(stamp) = FileStamp::read(&config.book) else {
            return false;
        };
        // Offer recovery only after a real beginning has been staged. The
        // staged result is discarded and prepared again after confirmation.
        let Ok(candidate) = Reading::open(config.clone()) else {
            return false;
        };
        if candidate.is_unsupported_spine_notice()
            || candidate.anchor().is_none()
            || FileStamp::read(&config.book).as_ref() != Some(&stamp)
            || with_storage(|| self.library.identify(entry)).ok() != Some(id)
        {
            return false;
        }
        self.resume_recovery = Some(Recovery {
            entry: entry.clone(),
            id,
            position: position.clone(),
            stamp,
        });
        true
    }

    fn resume_recovery_sheet() -> primitive_ui::Sheet<'static> {
        primitive_ui::Sheet::centered(
            Rect::new(0, 0, WIDTH as i32, HEIGHT as i32),
            "Saved position unavailable",
            "Open from the beginning? This replaces the saved reading position. Annotations stay.",
            "Open beginning",
        )
    }

    pub(super) fn draw_resume_recovery(&self, canvas: &mut Canvas) {
        if self.resume_recovery.is_some() {
            Self::resume_recovery_sheet().draw(canvas, None);
        }
    }

    pub(super) fn resume_recovery_input(&mut self, input: Input) -> Option<Change> {
        let recovery = self.resume_recovery.clone()?;
        if matches!(input, Input::Sleep | Input::Wake) {
            return None;
        }
        let confirm = match input {
            Input::PageBack => false,
            Input::Tap(x, y) => match Self::resume_recovery_sheet().hit(x, y) {
                Some(primitive_ui::SheetAction::Cancel) => false,
                Some(primitive_ui::SheetAction::Confirm) => true,
                None => return Some(Change::None),
            },
            _ => return Some(Change::None),
        };
        self.resume_recovery = None;
        if !confirm {
            return Some(Change::Partial);
        }
        Some(
            match self.open_with_archive_check(recovery.entry.clone(), None, false, Some(&recovery))
            {
                Ok(()) => Change::Partial,
                Err(error) => self.report(error),
            },
        )
    }
}
