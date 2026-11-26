//! Cancellable source verification for cross-chapter note edits.
use super::{
    Page, Reader, Screen, bulk_marks::Snapshot, confirmation::Confirmation, edit_note, focus,
};
use primitive_app::Change;
use primitive_ui::{BODY, CAPTION, Canvas, Input, Rect};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};

const TICK: Duration = Duration::from_millis(100);

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::app) enum Intent {
    Open,
    Save,
}
struct Guard(Arc<AtomicBool>);
impl Drop for Guard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

pub(in crate::app) struct Verification {
    intent: Intent,
    book: u64,
    key: super::MarkKey,
    title: String,
    target: Snapshot,
    desired: Option<Option<String>>,
    confirmation: Option<Confirmation>,
    screen: Screen,
    origin: super::super::HistoryLocation,
    recipe: Arc<crate::session::SearchLayoutRecipe>,
    cancelled: Arc<AtomicBool>,
    receiver: mpsc::Receiver<Result<(), &'static str>>,
    deadline: Instant,
}
impl Drop for Verification {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}
impl Verification {
    pub(in crate::app) fn deadline(&self) -> Instant {
        self.deadline
    }
}

pub(super) fn is_cross_chapter(target: &Snapshot) -> bool {
    matches!(target, Snapshot::Reflow(_, mark) if mark.selection.start.chapter != mark.selection.end.chapter)
}

fn panel(reader: &Reader) -> (Rect, Rect) {
    let layout = reader.navigation_layout();
    let width = (layout.width - 80).clamp(240, 760);
    let height = (layout.height - 80).clamp(260, 360);
    let panel = Rect::new(
        (layout.width - width) / 2,
        (layout.height - height) / 2,
        width,
        height,
    );
    let cancel = Rect::new(panel.x + (width - 240) / 2, panel.bottom() - 120, 240, 100);
    (panel, cancel)
}

impl Reader {
    pub(in crate::app) fn start_note_verification(&mut self, intent: Intent) -> crate::Result<()> {
        if self.note_verify_busy.swap(true, Ordering::AcqRel) {
            return Err("Previous note verification is still stopping".into());
        }
        let build = (|| -> crate::Result<_> {
            let pending = self
                .bookmark_browser
                .pending_note_edit
                .as_ref()
                .ok_or("Review the note first")?;
            let Snapshot::Reflow(_, mark) = &pending.target else {
                return Err("This saved passage cannot be checked".into());
            };
            let active = self.active.as_ref().ok_or("Open a book first")?;
            if active.id != pending.book || self.bookmark_browser.focus != Some(pending.key) {
                return Err("The open book changed; review the note again".into());
            }
            let expected_screen = match intent {
                Intent::Open => Page::Detail,
                Intent::Save => Page::EditNote,
            };
            if self.screen != Screen::BookmarkBrowser(expected_screen) {
                return Err("Review the note first".into());
            }
            let first = active
                .reading
                .probe
                .chapter_index(&mark.selection.start.chapter)
                .ok_or("That chapter is unavailable")?;
            let last = active
                .reading
                .probe
                .chapter_index(&mark.selection.end.chapter)
                .ok_or("That chapter is unavailable")?;
            if first >= last
                || mark.selection.start.book != active.reading.probe.book
                || mark.selection.end.book != active.reading.probe.book
            {
                return Err("The saved passage is unavailable".into());
            }
            let recipe = Arc::new(
                active
                    .reading
                    .config
                    .search_layout_recipe(&active.reading.probe),
            );
            if !recipe.has_verified_sources() {
                return Err("The book or reading fonts cannot be verified".into());
            }
            let origin = self.current_history_location().ok_or("Open a book first")?;
            Ok((
                pending.book,
                pending.key,
                pending.title.clone(),
                pending.target.clone(),
                pending.desired.clone(),
                self.bookmark_browser.confirmation,
                self.screen,
                origin,
                recipe,
                first,
                last,
                mark.clone(),
                crate::epub_members::is_folder_book(&active.reading.config.book),
            ))
        })();
        let (
            book,
            key,
            title,
            target,
            desired,
            confirmation,
            screen,
            origin,
            recipe,
            first,
            last,
            mark,
            folder,
        ) = match build {
            Ok(value) => value,
            Err(error) => {
                self.note_verify_busy.store(false, Ordering::Release);
                return Err(error);
            }
        };
        let cancelled = Arc::new(AtomicBool::new(false));
        let cancel = cancelled.clone();
        let worker_recipe = recipe.clone();
        let busy = self.note_verify_busy.clone();
        let (tx, receiver) = mpsc::sync_channel(1);
        if std::thread::Builder::new()
            .name("reader-note-check".into())
            .spawn(move || {
                let _guard = Guard(busy);
                let result = (|| {
                    let probe = worker_recipe
                        .chapter(first, &cancel)
                        .map_err(|_| "The book changed; review the note again")?;
                    let mut work = 0;
                    let verified = focus::verify_cross_chapter(
                        &worker_recipe,
                        &cancel,
                        &probe,
                        first,
                        last,
                        &mark,
                        &mut work,
                    )
                    .map_err(|error| match error {
                        focus::Failure::Limit => "This passage exceeds the verification limit",
                        focus::Failure::Changed => "The book changed; review the note again",
                    })?;
                    if !verified {
                        return Err("This saved passage no longer matches the open document");
                    }
                    if folder {
                        worker_recipe
                            .verify_bound_folder_snapshot(&cancel)
                            .map_err(|_| "The book changed; review the note again")?;
                    }
                    Ok(())
                })();
                if !cancel.load(Ordering::Relaxed) {
                    let _ = tx.send(result);
                }
            })
            .is_err()
        {
            self.note_verify_busy.store(false, Ordering::Release);
            return Err("Unable to start note verification".into());
        }
        self.note_verification = Some(Verification {
            intent,
            book,
            key,
            title,
            target,
            desired,
            confirmation,
            screen,
            origin,
            recipe,
            cancelled,
            receiver,
            deadline: Instant::now() + TICK,
        });
        Ok(())
    }

    pub(in crate::app) fn cancel_note_verification(&mut self) {
        if let Some(job) = self.note_verification.take()
            && job.intent == Intent::Open
        {
            self.bookmark_browser.pending_note_edit = None;
        }
    }
    pub(in crate::app) fn note_verification_input(&mut self, input: Input) -> Option<Change> {
        self.note_verification.as_ref()?;
        let (panel, cancel) = panel(self);
        Some(match input {
            Input::PageBack => {
                self.cancel_note_verification();
                Change::Partial
            }
            Input::Tap(x, y) if cancel.contains(x, y) || !panel.contains(x, y) => {
                self.cancel_note_verification();
                Change::Partial
            }
            _ => Change::None,
        })
    }
    pub(in crate::app) fn draw_note_verification(&self, canvas: &mut Canvas) {
        if self.note_verification.is_none() {
            return;
        }
        let (panel, cancel) = panel(self);
        let vi = !self.controls.interface_language.is_english();
        canvas.popup(panel);
        canvas.text(
            Rect::new(panel.x + 36, panel.y + 24, panel.w - 72, 64),
            if vi {
                "Đang kiểm tra đoạn đã lưu"
            } else {
                "Checking saved passage"
            },
            BODY.bold(),
        );
        canvas.text(
            Rect::new(panel.x + 36, panel.y + 100, panel.w - 72, 64),
            if vi {
                "Xác minh sách và đoạn trích"
            } else {
                "Verifying the book and quotation"
            },
            CAPTION,
        );
        canvas.button(cancel, if vi { "Hủy" } else { "Cancel" }, false, false);
    }
    pub(in crate::app) fn poll_note_verification(&mut self, now: Instant) -> Option<Change> {
        let mut job = self.note_verification.take()?;
        if now < job.deadline {
            self.note_verification = Some(job);
            return None;
        }
        let result = match job.receiver.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => {
                job.deadline = now + TICK;
                self.note_verification = Some(job);
                return None;
            }
            Err(mpsc::TryRecvError::Disconnected) => Err("Note verification stopped"),
        };
        let valid = self.screen == job.screen
            && self.open_book_id() == Some(job.book)
            && self.current_history_location().as_ref() == Some(&job.origin)
            && self.bookmark_browser.focus == Some(job.key)
            && self.bookmark_browser.confirmation == job.confirmation
            && self
                .bookmark_browser
                .pending_note_edit
                .as_ref()
                .is_some_and(|pending| {
                    pending.book == job.book
                        && pending.key == job.key
                        && pending.title == job.title
                        && pending.target == job.target
                        && pending.desired == job.desired
                })
            && self
                .library
                .state(job.book)
                .is_some_and(|book| book.title == job.title && job.target.matches(book))
            && self.active.as_ref().is_some_and(|active| {
                active
                    .reading
                    .config
                    .search_layout_recipe(&active.reading.probe)
                    == *job.recipe
            });
        if !valid || result.is_err() {
            if job.intent == Intent::Open {
                self.bookmark_browser.pending_note_edit = None;
            }
            let message = if valid {
                result.err().unwrap_or("Note verification stopped")
            } else {
                "The book or mark changed; review the note again"
            };
            return Some(self.report(message));
        }
        match job.intent {
            Intent::Open => {
                self.show(Screen::BookmarkBrowser(Page::EditNote));
                Some(Change::Partial)
            }
            Intent::Save => Some(match edit_note::commit_verified(self) {
                Ok(()) => Change::Partial,
                Err(error) => self.report(error),
            }),
        }
    }
}
