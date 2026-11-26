//! Cancellable source verification for reflow bookmark and highlight notes.
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

enum OtherProof {
    Pdf(super::other_pdf::Proof),
    Epub(
        super::other_epub::Proof,
        Vec<crate::session::screen_pages::FileStamp>,
    ),
}
impl OtherProof {
    fn digest(&self) -> [u8; 32] {
        match self {
            Self::Pdf(proof) => proof.digest(),
            Self::Epub(proof, _) => proof.digest(),
        }
    }
    fn revalidate(&self, context: Option<&super::other_epub_check::Context>) -> bool {
        match self {
            Self::Pdf(proof) => context.is_none() && proof.revalidate().is_ok(),
            Self::Epub(proof, fonts) => {
                proof.revalidate().is_ok()
                    && context.is_some_and(|context| context.revalidate_fonts(fonts))
            }
        }
    }
    fn epub_fonts(&self) -> Option<&[crate::session::screen_pages::FileStamp]> {
        match self {
            Self::Epub(_, fonts) => Some(fonts),
            Self::Pdf(_) => None,
        }
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
    origin_book: Option<u64>,
    origin: Option<super::super::HistoryLocation>,
    origin_fixed_view: Option<crate::library::FixedPreferences>,
    recipe: Option<Arc<crate::session::SearchLayoutRecipe>>,
    other_path: Option<std::path::PathBuf>,
    other_digest: Option<[u8; 32]>,
    other_context: Option<super::other_epub_check::Context>,
    other_fonts: Option<Vec<crate::session::screen_pages::FileStamp>>,
    cancelled: Arc<AtomicBool>,
    receiver: mpsc::Receiver<Result<Option<OtherProof>, &'static str>>,
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

pub(super) fn uses_reflow_worker(reader: &Reader, target: &Snapshot) -> bool {
    reader.active.is_some() && matches!(target, Snapshot::Bookmark(..) | Snapshot::Reflow(..))
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
            let probe = &active.reading.probe;
            let first = match &pending.target {
                Snapshot::Reflow(_, mark) => {
                    let first = probe
                        .chapter_index(&mark.selection.start.chapter)
                        .ok_or("That chapter is unavailable")?;
                    let last = probe
                        .chapter_index(&mark.selection.end.chapter)
                        .ok_or("That chapter is unavailable")?;
                    if first > last
                        || mark.selection.start.book != probe.book
                        || mark.selection.end.book != probe.book
                    {
                        return Err("The saved passage is unavailable".into());
                    }
                    first
                }
                Snapshot::Bookmark(_, mark) => {
                    if let Some(anchor) = &mark.position.anchor {
                        if anchor.book != probe.book
                            || probe
                                .chapter_entries()
                                .iter()
                                .filter(|chapter| chapter.href == anchor.chapter)
                                .take(2)
                                .count()
                                != 1
                        {
                            return Err("The saved bookmark is unavailable".into());
                        }
                        probe
                            .chapter_index(&anchor.chapter)
                            .ok_or("That chapter is unavailable")?
                    } else {
                        mark.position.chapter
                    }
                }
                Snapshot::Fixed(..) => return Err("This saved passage cannot be checked".into()),
            };
            if probe
                .chapter_entries()
                .get(first)
                .is_none_or(|chapter| chapter.unsupported)
            {
                return Err("The saved section is unavailable".into());
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
                pending.target.clone(),
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
            work,
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
                    if probe
                        .chapter_entries()
                        .get(first)
                        .is_none_or(|chapter| chapter.unsupported)
                    {
                        return Err("The saved section is unavailable");
                    }
                    let verified = match &work {
                        Snapshot::Bookmark(_, mark) => match &mark.position.anchor {
                            Some(anchor) => probe.page_for(anchor).is_some(),
                            None => mark.position.page < probe.page_count(),
                        },
                        Snapshot::Reflow(_, mark) => {
                            super::super::page_bookmark_navigation::verify_winning_quote(
                                &worker_recipe,
                                &probe,
                                mark,
                                None,
                                &cancel,
                            )
                            .map_err(|error| match error {
                                focus::Failure::Limit => {
                                    "This passage exceeds the verification limit"
                                }
                                focus::Failure::Changed => {
                                    "The book changed; review the note again"
                                }
                            })?
                        }
                        Snapshot::Fixed(..) => false,
                    };
                    if !verified {
                        return Err("This saved mark no longer matches the open document");
                    }
                    if folder {
                        worker_recipe
                            .verify_bound_folder_snapshot(&cancel)
                            .map_err(|_| "The book changed; review the note again")?;
                    }
                    Ok(None)
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
            origin_book: Some(book),
            origin: Some(origin),
            origin_fixed_view: None,
            recipe: Some(recipe),
            other_path: None,
            other_digest: None,
            other_context: None,
            other_fonts: None,
            cancelled,
            receiver,
            deadline: Instant::now() + TICK,
        });
        Ok(())
    }

    pub(in crate::app) fn start_other_note_verification(
        &mut self,
        intent: Intent,
    ) -> crate::Result<()> {
        if self.note_verify_busy.swap(true, Ordering::AcqRel) {
            return Err("Previous note verification is still stopping".into());
        }
        let build = (|| -> crate::Result<_> {
            let pending = self
                .bookmark_browser
                .pending_note_edit
                .as_ref()
                .ok_or("Review the note first")?;
            let path = pending
                .other_path
                .clone()
                .ok_or("That book is unavailable")?;
            let state = self
                .library
                .state(pending.book)
                .ok_or("That book is unavailable")?;
            let expected_screen = match intent {
                Intent::Open => Page::Detail,
                Intent::Save => Page::EditNote,
            };
            if self.screen != Screen::BookmarkBrowser(expected_screen)
                || self.bookmark_browser.focus != Some(pending.key)
                || self.open_book_id() != pending.origin_book
                || self.current_history_location() != pending.origin
                || self.fixed.as_ref().map(|fixed| fixed.reading.preferences())
                    != pending.origin_fixed_view
                || state.path != path
                || state.title != pending.title
                || !pending.target.matches(state)
            {
                return Err("The book or mark changed; review the note again".into());
            }
            let context = match crate::library::DocumentFormat::for_path(&path) {
                Some(crate::library::DocumentFormat::Pdf) => None,
                Some(crate::library::DocumentFormat::Epub)
                    if matches!(&pending.target, Snapshot::Reflow(..))
                        && !crate::epub_members::is_folder_book(&path) =>
                {
                    Some(super::other_epub_check::Context::capture(
                        self,
                        pending.book,
                    )?)
                }
                _ => return Err("This book cannot be verified".into()),
            };
            if (intent == Intent::Open && pending.other_epub_context.is_some())
                || (intent == Intent::Save && pending.other_epub_context != context)
            {
                return Err("The book's reading settings changed; review the note again".into());
            }
            Ok((
                pending.book,
                pending.key,
                pending.title.clone(),
                pending.target.clone(),
                pending.desired.clone(),
                self.bookmark_browser.confirmation,
                self.screen,
                pending.origin_book,
                pending.origin.clone(),
                pending.origin_fixed_view,
                pending.other_digest,
                pending.other_epub_fonts.clone(),
                self.library.verification_root().to_owned(),
                path,
                context,
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
            origin_book,
            origin,
            origin_fixed_view,
            other_digest,
            other_fonts,
            root,
            path,
            context,
        ) = match build {
            Ok(value) => value,
            Err(error) => {
                self.note_verify_busy.store(false, Ordering::Release);
                return Err(error);
            }
        };
        let cancelled = Arc::new(AtomicBool::new(false));
        let cancel = Arc::clone(&cancelled);
        let worker_path = path.clone();
        let work = target.clone();
        let worker_context = context.clone();
        let busy = Arc::clone(&self.note_verify_busy);
        let (tx, receiver) = mpsc::sync_channel(1);
        if std::thread::Builder::new().name("reader-other-pdf-note-check".into()).spawn(move || {
            let _guard = Guard(busy);
            let result = match worker_context {
                None => super::other_pdf::Proof::verify(root, worker_path, book, &work, &cancel)
                    .map(OtherProof::Pdf).map_err(|_| "The PDF is unavailable, changed, protected, or exceeds the verification limit"),
                Some(context) => (|| {
                    let (proof, bytes) = super::other_epub::Proof::capture(root, worker_path, book, &cancel)
                        .map_err(|_| "The EPUB is unavailable, changed, protected, or exceeds the verification limit")?;
                    let fonts = context.verify(bytes, &work, &cancel)
                        .map_err(|_| "The EPUB passage or its reading settings cannot be verified")?;
                    proof.revalidate().map_err(|_| "The EPUB changed; review the note again")?;
                    Ok(OtherProof::Epub(proof, fonts))
                })(),
            }.map(Some);
            if !cancel.load(Ordering::Relaxed) { let _ = tx.send(result); }
        }).is_err() {
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
            origin_book,
            origin,
            origin_fixed_view,
            recipe: None,
            other_path: Some(path),
            other_digest,
            other_context: context,
            other_fonts,
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
        let Some(job) = &self.note_verification else {
            return;
        };
        let (panel, cancel) = panel(self);
        let vi = !self.controls.interface_language.is_english();
        let bookmark = matches!(&job.target, Snapshot::Bookmark(..));
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
            match (vi, bookmark || job.other_path.is_some()) {
                (true, true) => "Xác minh sách và vị trí đã lưu",
                (true, false) => "Xác minh sách và đoạn trích",
                (false, true) => "Verifying the book and saved location",
                (false, false) => "Verifying the book and quotation",
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
            && self.open_book_id() == job.origin_book
            && self.current_history_location() == job.origin
            && self.fixed.as_ref().map(|fixed| fixed.reading.preferences())
                == job.origin_fixed_view
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
                        && pending.other_path == job.other_path
                        && pending.other_digest == job.other_digest
                        && pending.other_epub_fonts == job.other_fonts
                        && pending.other_epub_context.as_ref()
                            == match job.intent {
                                Intent::Open => None,
                                Intent::Save => job.other_context.as_ref(),
                            }
                        && (job.other_path.is_none()
                            || (pending.origin_book == job.origin_book
                                && pending.origin == job.origin
                                && pending.origin_fixed_view == job.origin_fixed_view))
                })
            && self.library.state(job.book).is_some_and(|book| {
                book.title == job.title
                    && job.target.matches(book)
                    && job
                        .other_path
                        .as_ref()
                        .is_none_or(|path| &book.path == path)
            })
            && match &job.recipe {
                Some(recipe) => self.active.as_ref().is_some_and(|active| {
                    active
                        .reading
                        .config
                        .search_layout_recipe(&active.reading.probe)
                        == **recipe
                }),
                None => {
                    job.other_path.is_some()
                        && job.other_context.as_ref().is_none_or(|context| {
                            super::other_epub_check::Context::capture(self, job.book)
                                .is_ok_and(|current| &current == context)
                        })
                }
            };
        let source_valid = match (&job.recipe, &result) {
            (Some(_), Ok(None)) => true,
            (None, Ok(Some(proof))) => {
                proof.revalidate(job.other_context.as_ref())
                    && match job.intent {
                        Intent::Open => job.other_digest.is_none() && job.other_fonts.is_none(),
                        Intent::Save => {
                            job.other_digest == Some(proof.digest())
                                && job.other_fonts.as_deref() == proof.epub_fonts()
                        }
                    }
            }
            _ => false,
        };
        if !valid || !source_valid {
            if job.intent == Intent::Open {
                self.bookmark_browser.pending_note_edit = None;
            }
            let message = if valid {
                result
                    .err()
                    .unwrap_or("The book changed; review the note again")
            } else {
                "The book or mark changed; review the note again"
            };
            return Some(self.report(message));
        }
        match job.intent {
            Intent::Open => {
                if let Ok(Some(proof)) = &result
                    && let Some(pending) = &mut self.bookmark_browser.pending_note_edit
                {
                    pending.other_digest = Some(proof.digest());
                    pending.other_epub_context = job.other_context.clone();
                    pending.other_epub_fonts = proof.epub_fonts().map(|fonts| fonts.to_vec());
                }
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
