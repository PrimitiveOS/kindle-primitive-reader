//! Asynchronous verification before resuming a snapshot-backed folder book.
use std::io;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};

use primitive_app::{App, Change};
use primitive_ui::{BODY, CAPTION, Canvas, Input, Rect};

use super::*;
use crate::session::SearchLayoutRecipe;
use crate::session::screen_pages::folder_sources::FolderProof;

pub(super) struct Gate {
    recipe: SearchLayoutRecipe,
    proof: Arc<FolderProof>,
    location: HistoryLocation,
    receiver: Option<mpsc::Receiver<bool>>,
    cancelled: Arc<AtomicBool>,
    busy: Arc<AtomicBool>,
    deadline: Option<Instant>,
    failed: bool,
    suspended: bool,
}

impl Drop for Gate {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

impl Gate {
    fn launch(&mut self) -> io::Result<()> {
        if self.busy.swap(true, Ordering::AcqRel) {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "Previous verification is still stopping",
            ));
        }
        let recipe = self.recipe.clone();
        let proof = Arc::clone(&self.proof);
        let cancelled = Arc::new(AtomicBool::new(false));
        self.cancelled = Arc::clone(&cancelled);
        let busy = Arc::clone(&self.busy);
        let (tx, rx) = mpsc::sync_channel(1);
        let started = std::thread::Builder::new()
            .name("reader-folder-resume".into())
            .spawn(move || {
                struct WorkerGuard(Arc<AtomicBool>);
                impl Drop for WorkerGuard {
                    fn drop(&mut self) {
                        self.0.store(false, Ordering::Release);
                    }
                }
                let _guard = WorkerGuard(busy);
                let verified = recipe.verify_folder_proof(&proof, &cancelled).is_ok();
                if !cancelled.load(Ordering::Relaxed) {
                    let _ = tx.send(verified);
                }
            });
        if let Err(error) = started {
            self.busy.store(false, Ordering::Release);
            return Err(error);
        }
        self.receiver = Some(rx);
        self.deadline = Some(Instant::now() + Duration::from_millis(200));
        self.failed = false;
        self.suspended = false;
        Ok(())
    }

    pub(super) fn deadline(&self) -> Option<Instant> {
        self.deadline
    }
}

fn controls(reader: &Reader) -> (Rect, Rect, Rect) {
    let layout = reader.navigation_layout();
    let width = (layout.width - 80).clamp(288, 760);
    let height = (layout.height - 80).clamp(260, 390);
    let panel = Rect::new(
        (layout.width - width) / 2,
        (layout.height - height) / 2,
        width,
        height,
    );
    let gap = 16;
    let button_width = (width - 72 - gap) / 2;
    let retry = Rect::new(panel.x + 36, panel.bottom() - 125, button_width, 100);
    let library = Rect::new(retry.right() + gap, retry.y, button_width, 100);
    (panel, retry, library)
}

impl Reader {
    /// Returns true when ordinary storage-return processing must be deferred.
    pub(super) fn start_folder_resume_gate(&mut self) -> io::Result<bool> {
        let Some(active) = &self.active else {
            return Ok(false);
        };
        let Some(proof) = active.reading.config.folder_snapshot.clone() else {
            return Ok(false);
        };
        let recipe = active
            .reading
            .config
            .search_layout_recipe(&active.reading.probe);
        let location = self
            .current_history_location()
            .ok_or_else(|| io::Error::other("The reading position is unavailable"))?;
        let mut gate = Gate {
            recipe,
            proof,
            location,
            receiver: None,
            cancelled: Arc::new(AtomicBool::new(false)),
            busy: Arc::new(AtomicBool::new(false)),
            deadline: None,
            failed: false,
            suspended: false,
        };
        if gate.launch().is_err() {
            gate.failed = true;
        }
        self.folder_resume_gate = Some(gate);
        Ok(true)
    }

    pub(super) fn draw_folder_resume_gate(&self, canvas: &mut Canvas) {
        let Some(gate) = &self.folder_resume_gate else {
            return;
        };
        let (panel, retry, library) = controls(self);
        canvas.popup(panel);
        canvas.text(
            Rect::new(panel.x + 30, panel.y + 22, panel.w - 60, 62),
            "Checking the book",
            BODY.bold(),
        );
        let text = if gate.failed {
            "The folder or reading fonts changed. Retry or return to the library."
        } else {
            "Verifying the book and reading fonts…"
        };
        canvas.text(
            Rect::new(panel.x + 30, panel.y + 100, panel.w - 60, 90),
            text,
            CAPTION,
        );
        if gate.failed {
            canvas.button(retry, "Retry", false, false);
        }
        canvas.button(library, "Library", false, false);
    }

    pub(super) fn folder_resume_input(&mut self, input: Input) -> Option<Change> {
        let gate = self.folder_resume_gate.as_ref()?;
        if input == Input::Sleep {
            let gate = self.folder_resume_gate.as_mut().expect("checked gate");
            gate.cancelled.store(true, Ordering::Relaxed);
            gate.receiver = None;
            gate.deadline = None;
            gate.suspended = true;
            return None;
        }
        if input == Input::Wake {
            let gate = self.folder_resume_gate.as_mut().expect("checked gate");
            if gate.suspended {
                gate.suspended = false;
                if gate.launch().is_err() {
                    gate.failed = true;
                }
            }
            return None;
        }
        let (_, retry, library) = controls(self);
        if let Input::Tap(x, y) = input {
            if gate.failed && retry.contains(x, y) {
                let gate = self.folder_resume_gate.as_mut().expect("checked gate");
                if gate.launch().is_err() {
                    gate.failed = true;
                }
                return Some(Change::Partial);
            }
            if !library.contains(x, y) {
                return Some(Change::None);
            }
        }
        if matches!(input, Input::PageBack | Input::Tap(..)) {
            self.folder_resume_gate = None;
            self.close_book_to_library();
            return Some(match self.finish_folder_storage_returned() {
                Ok(()) => Change::Full,
                Err(error) => self.report(error),
            });
        }
        Some(Change::None)
    }

    pub(super) fn poll_folder_resume(&mut self, now: Instant) -> Option<Change> {
        let gate = self.folder_resume_gate.as_mut()?;
        if gate.deadline.is_none_or(|deadline| now < deadline) {
            return None;
        }
        let result = match gate.receiver.as_ref()?.try_recv() {
            Ok(result) => Some(result),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => Some(false),
        };
        let Some(verified) = result else {
            gate.deadline = Some(now + Duration::from_millis(200));
            return None;
        };
        let location = gate.location.clone();
        let recipe = gate.recipe.clone();
        let same = self.current_history_location().as_ref() == Some(&location)
            && self.active.as_ref().is_some_and(|active| {
                active
                    .reading
                    .config
                    .search_layout_recipe(&active.reading.probe)
                    == recipe
            });
        if !verified || !same {
            let gate = self.folder_resume_gate.as_mut().expect("checked gate");
            gate.receiver = None;
            gate.deadline = None;
            gate.failed = true;
            return Some(Change::Partial);
        }
        self.folder_resume_gate = None;
        Some(match self.finish_folder_storage_returned() {
            Ok(()) => Change::Full,
            Err(error) => self.report(error),
        })
    }
}

impl Reader {
    pub(super) fn finish_folder_storage_returned(&mut self) -> io::Result<()> {
        self.overview_raw = None;
        self.book_map_raw_history = None;
        self.book_map_raw_counts = None;
        self.page_bookmark_search = None;
        self.gestures.multiswipes.clear_contact();
        self.finish_interrupted_scroll();
        self.embedded_font_report = None;
        self.gestures.clear_assigned_corner_hold();
        if let Some(book) = &mut self.active {
            book.embedded_font_inventory = None;
        }
        self.eager_typography.cancel();
        self.alternative_contents = Default::default();
        self.sync_alternative_contents();
        if self.screen.is_contents() {
            self.prepare_contents(true);
            self.list_page = self.current_contents_row.unwrap_or_default() / self.contents_rows();
        }
        self.close_dictionary_font_size();
        if matches!(self.screen, Screen::DictionaryButtons(_)) {
            self.show(Screen::Dictionaries);
        }
        self.closing_profile = None;
        if self.closing_before_switch.is_some() && self.screen == Screen::PdfPassword {
            self.handle_pdf_password(Input::PageBack);
        }
        self.closing_before_switch = None;
        self.opening_execution = None;
        self.install_opening_profile_plan(None);
        self.screen_page_worker.cancel();
        self.quote_repair.cancel();
        self.cancel_screen_page_jump();
        self.document_defaults.reload();

        self.cjk_selection_lookup = None;
        self.spelling_lookup = None;
        self.cancel_ocr_selection();
        self.pages_since_refresh = 0;
        let now = Instant::now();
        self.auto_turn.activity(now);
        self.sync_auto_turn(now);
        if self.screen == Screen::GeneratedContents {
            self.show(Screen::Contents);
        }
        self.stats.discard_active();
        self.reading_history.discard_active();
        self.pace.storage_returned();
        self.update_stats_activity(Instant::now());
        self.ocr_assets = self
            .ocr_root
            .parent()
            .and_then(Path::parent)
            .and_then(OcrAssets::discover);
        if let Some(active) = &mut self.active {
            active.reading.storage_returned()?;
        }
        if let Some(fixed) = &mut self.fixed {
            fixed.reading.pause();
        }
        self.save()?;
        with_storage(|| self.library.scan())?;
        self.refresh_browser()?;
        if self.screen == Screen::PageOverview {
            self.start_overview_raw_verification();
        }
        if self.screen == Screen::BookMap {
            self.start_book_map_raw_history();
            self.start_book_map_raw_counts();
        }
        Ok(())
    }
}
