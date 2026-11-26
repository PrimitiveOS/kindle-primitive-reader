//! Capture a folder snapshot before reopening a saved raw position.
use super::{LibraryBook, Reader, Screen, with_storage};
use crate::session::screen_pages::folder_sources::FolderProof;
use primitive_app::Change;
use primitive_ui::{BODY, CAPTION, Canvas, Input, Rect};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};

pub(super) struct Pending {
    entry: LibraryBook,
    check_archive: bool,
    mark: Option<super::bookmark_browser::DeferredMark>,
    screen: Screen,
    receiver: mpsc::Receiver<Option<Arc<FolderProof>>>,
    cancelled: Arc<AtomicBool>,
    deadline: Instant,
}
impl Drop for Pending {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}
impl Pending {
    pub(super) fn deadline(&self) -> Instant {
        self.deadline
    }
}

fn controls(reader: &Reader) -> (Rect, Rect) {
    let layout = reader.navigation_layout();
    let width = (layout.width - 80).clamp(288, 760);
    let height = (layout.height - 80).clamp(260, 360);
    let panel = Rect::new(
        (layout.width - width) / 2,
        (layout.height - height) / 2,
        width,
        height,
    );
    let button = Rect::new(panel.x + (width - 200) / 2, panel.bottom() - 125, 200, 100);
    (panel, button)
}

impl Reader {
    pub(super) fn attach_folder_open_mark(
        &mut self,
        mark: super::bookmark_browser::DeferredMark,
    ) -> crate::Result<()> {
        self.folder_open
            .as_mut()
            .ok_or("The folder open was cancelled")?
            .mark = Some(mark);
        Ok(())
    }

    pub(super) fn cancel_folder_open(&mut self) {
        if self.folder_open.take().is_some() {
            self.metadata_archive.abandon_archive_consumption();
        }
    }

    pub(super) fn maybe_start_folder_open(
        &mut self,
        entry: &LibraryBook,
        check_archive: bool,
    ) -> crate::Result<bool> {
        self.start_folder_open(entry, check_archive, false)
    }

    pub(super) fn start_folder_open(
        &mut self,
        entry: &LibraryBook,
        check_archive: bool,
        identified_raw: bool,
    ) -> crate::Result<bool> {
        if self.folder_open_bypass
            || !crate::parley_layout::RAW_LAYOUT_ENABLED
            || !crate::epub_members::is_folder_book(&entry.path)
            || (!identified_raw
                && !self.library.recents().iter().any(|state| {
                    state.path == entry.path && self.library.has_raw_whitespace_state(state.id)
                }))
        {
            return Ok(false);
        }
        self.folder_activation = None;
        self.folder_activation_retry = None;
        if self.folder_open_busy.swap(true, Ordering::AcqRel) {
            return Err("Previous folder verification is still stopping".into());
        }
        let marker = match with_storage(|| self.library.absolute_path(&entry.path)) {
            Ok(marker) => marker,
            Err(error) => {
                self.folder_open_busy.store(false, Ordering::Release);
                return Err(error.into());
            }
        };
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = Arc::clone(&cancelled);
        let busy = Arc::clone(&self.folder_open_busy);
        let (tx, receiver) = mpsc::sync_channel(1);
        let spawned = std::thread::Builder::new()
            .name("reader-folder-open".into())
            .spawn(move || {
                struct Guard(Arc<AtomicBool>);
                impl Drop for Guard {
                    fn drop(&mut self) {
                        self.0.store(false, Ordering::Release);
                    }
                }
                let _guard = Guard(busy);
                let proof = with_storage(|| FolderProof::capture(&marker, &worker_cancelled))
                    .ok()
                    .filter(|proof| !proof.contains_hidden_members())
                    .map(Arc::new);
                if !worker_cancelled.load(Ordering::Relaxed) {
                    let _ = tx.send(proof);
                }
            });
        if let Err(error) = spawned {
            self.folder_open_busy.store(false, Ordering::Release);
            return Err(error.into());
        }
        self.folder_open = Some(Pending {
            entry: entry.clone(),
            check_archive,
            mark: None,
            screen: self.screen,
            receiver,
            cancelled,
            deadline: Instant::now() + Duration::from_millis(200),
        });
        Ok(true)
    }

    pub(super) fn draw_folder_open(&self, canvas: &mut Canvas) {
        if self.folder_open.is_none() {
            return;
        }
        let (panel, cancel) = controls(self);
        canvas.popup(panel);
        canvas.text(
            Rect::new(panel.x + 30, panel.y + 25, panel.w - 60, 60),
            "Opening book",
            BODY.bold(),
        );
        canvas.text(
            Rect::new(panel.x + 30, panel.y + 100, panel.w - 60, 60),
            "Checking saved reading position…",
            CAPTION,
        );
        canvas.button(cancel, "Cancel", false, false);
    }

    pub(super) fn folder_open_input(&mut self, input: Input) -> Option<Change> {
        self.folder_open.as_ref()?;
        if matches!(input, Input::Sleep | Input::Wake) {
            if input == Input::Sleep {
                self.cancel_folder_open();
            }
            return None;
        }
        let (_, cancel) = controls(self);
        if matches!(input, Input::PageBack)
            || matches!(input, Input::Tap(x, y) if cancel.contains(x, y))
        {
            self.cancel_folder_open();
            return Some(Change::Partial);
        }
        Some(Change::None)
    }

    pub(super) fn poll_folder_open(&mut self, now: Instant) -> Option<Change> {
        let mut pending = self.folder_open.take()?;
        if now < pending.deadline {
            self.folder_open = Some(pending);
            return None;
        }
        let proof = match pending.receiver.try_recv() {
            Ok(proof) => proof,
            Err(mpsc::TryRecvError::Empty) => {
                pending.deadline = now + Duration::from_millis(200);
                self.folder_open = Some(pending);
                return None;
            }
            Err(mpsc::TryRecvError::Disconnected) => None,
        };
        if self.screen != pending.screen {
            self.metadata_archive.abandon_archive_consumption();
            return None;
        }
        let entry = pending.entry.clone();
        let mark = pending.mark.take();
        if mark
            .as_ref()
            .is_some_and(|mark| !mark.matches(&self.library))
        {
            self.metadata_archive.abandon_archive_consumption();
            return Some(self.report("The saved mark changed while opening"));
        }
        if let Some(proof) = proof {
            let id = proof.legacy_identity();
            if self.library.has_raw_whitespace_state(id) {
                self.pending_folder_snapshot = Some((entry.path.clone(), id, proof));
            }
        }
        if mark.is_some() && self.pending_folder_snapshot.is_none() {
            self.metadata_archive.abandon_archive_consumption();
            return Some(self.report("The folder book could not be verified; the mark was kept"));
        }
        self.pending_folder_mark = mark;
        self.folder_open_bypass = true;
        let result = self.open_with_password_mode(entry, None, pending.check_archive);
        self.folder_open_bypass = false;
        self.pending_folder_snapshot = None;
        self.pending_folder_mark = None;
        Some(match result {
            Ok(()) => Change::Full,
            Err(error) => self.report(error),
        })
    }
}
