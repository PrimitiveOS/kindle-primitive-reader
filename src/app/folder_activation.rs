//! Prepare a folder-backed reading layout without interrupting the current page.
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};

use super::{HistoryLocation, Reader, Screen, with_storage};
use crate::session::screen_pages::folder_sources::FolderProof;
use crate::session::{PreparedFolderChapter, Reading, SearchLayoutRecipe};
use primitive_app::Change;

// The bounded worker activates only after source, font and position checks;
// unsupported cases retain the canonical layout.
const FOLDER_RAW_ACTIVATION_ENABLED: bool = true;

pub(super) struct Activation {
    book: u64,
    screen: Screen,
    location: HistoryLocation,
    scroll_top: Option<f32>,
    recipe: SearchLayoutRecipe,
    receiver: mpsc::Receiver<Option<(Arc<FolderProof>, PreparedFolderChapter)>>,
    cancelled: Arc<AtomicBool>,
    deadline: Instant,
}

impl Drop for Activation {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

impl Activation {
    pub(super) fn deadline(&self) -> Instant {
        self.deadline
    }
}

impl Reader {
    /// Retry only work interrupted by an interaction; unsupported folders do
    /// not trigger repeated background scans.
    pub(super) fn defer_folder_activation(&mut self) {
        if self.folder_activation.take().is_some() || self.folder_activation_retry.is_some() {
            self.folder_activation_retry = Some(Instant::now() + Duration::from_secs(2));
        }
    }

    pub(super) fn retry_folder_activation(&mut self, now: Instant) {
        if self.screen != Screen::Book
            || self
                .folder_activation_retry
                .is_none_or(|deadline| now < deadline)
        {
            return;
        }
        self.folder_activation_retry = None;
        if self.touch_hold.is_some()
            || self.selection_mode
            || self.selection.is_some()
            || self.pending_orientation.is_some()
            || self.opening_execution.is_some()
            || self
                .active
                .as_ref()
                .is_some_and(|active| active.reading.page_turn_pending())
        {
            self.folder_activation_retry = Some(now + Duration::from_secs(2));
            return;
        }
        self.start_folder_activation();
    }

    pub(super) fn start_folder_activation(&mut self) {
        self.folder_activation = None;
        if !FOLDER_RAW_ACTIVATION_ENABLED {
            return;
        }
        let Some(active) = &self.active else {
            return;
        };
        let reading = &active.reading;
        if !crate::epub_members::is_folder_book(&reading.config.book)
            || reading.config.folder_snapshot.is_some()
        {
            return;
        }
        let Some(location) = self.current_history_location() else {
            return;
        };
        let Some(anchor) = location.position.anchor.clone() else {
            return;
        };
        if anchor.raw_caret.is_some() {
            return;
        }
        let recipe = reading.config.search_layout_recipe(&reading.probe);
        let scroll_top = reading.scroll_history_top();
        if !recipe.has_verified_fonts() {
            return;
        }
        let marker = reading.config.book.clone();
        let book = active.id;
        let chapter = location.position.chapter;
        let worker_recipe = recipe.clone();
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = Arc::clone(&cancelled);
        if self.folder_activation_busy.swap(true, Ordering::AcqRel) {
            self.folder_activation_retry = Some(Instant::now() + Duration::from_millis(250));
            return;
        }
        let busy = Arc::clone(&self.folder_activation_busy);
        let (tx, receiver) = mpsc::sync_channel(1);
        let spawned = std::thread::Builder::new()
            .name("reader-folder-prepare".into())
            .spawn(move || {
                struct WorkerGuard(Arc<AtomicBool>);
                impl Drop for WorkerGuard {
                    fn drop(&mut self) {
                        self.0.store(false, Ordering::Release);
                    }
                }
                let _guard = WorkerGuard(busy);
                let result = (|| {
                    let proof = Arc::new(
                        with_storage(|| FolderProof::capture(&marker, &worker_cancelled)).ok()?,
                    );
                    if proof.contains_hidden_members() || proof.legacy_identity() != book {
                        return None;
                    }
                    let prepared = worker_recipe
                        .prepare_folder_raw(Arc::clone(&proof), chapter, &anchor, &worker_cancelled)
                        .ok()?;
                    worker_recipe
                        .verify_folder_proof(&proof, &worker_cancelled)
                        .ok()?;
                    Some((proof, prepared))
                })();
                if !worker_cancelled.load(Ordering::Relaxed) {
                    let _ = tx.send(result);
                }
            });
        if spawned.is_ok() {
            self.folder_activation = Some(Activation {
                book,
                screen: self.screen,
                location,
                scroll_top,
                recipe,
                receiver,
                cancelled,
                deadline: Instant::now() + Duration::from_millis(200),
            });
        } else {
            self.folder_activation_busy.store(false, Ordering::Release);
        }
    }

    pub(super) fn poll_folder_activation(&mut self, now: Instant) -> Option<Change> {
        let mut activation = self.folder_activation.take()?;
        if now < activation.deadline {
            self.folder_activation = Some(activation);
            return None;
        }
        let result = match activation.receiver.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => {
                activation.deadline = now + Duration::from_millis(200);
                self.folder_activation = Some(activation);
                return None;
            }
            Err(mpsc::TryRecvError::Disconnected) => None,
        }?;
        let (proof, prepared) = result;
        if self.screen != activation.screen
            || self.open_book_id() != Some(activation.book)
            || self.current_history_location().as_ref() != Some(&activation.location)
            || self.touch_hold.is_some()
            || self.selection_mode
            || self.selection.is_some()
            || self.pending_orientation.is_some()
            || self.opening_execution.is_some()
        {
            return None;
        }
        let active = self.active.as_mut()?;
        let reading = &mut active.reading;
        if reading.page_turn_pending()
            || reading.scroll_history_top() != activation.scroll_top
            || reading.config.search_layout_recipe(&reading.probe) != activation.recipe
        {
            return None;
        }
        let mut config = reading.config.clone();
        config.folder_snapshot = Some(proof.clone());
        let position = &activation.location.position;
        let staged = Reading::restore_prepared_folder_at(
            config,
            position.chapter,
            position.page,
            position.anchor.as_ref(),
            activation.scroll_top,
            prepared,
        )
        .ok()?;
        if staged.probe.raw_folder_sources.as_deref() != Some(proof.as_ref())
            || position
                .anchor
                .as_ref()
                .is_none_or(|anchor| staged.probe.page_for(anchor).is_none())
        {
            return None;
        }
        *reading = staged;
        Some(Change::Full)
    }
}
