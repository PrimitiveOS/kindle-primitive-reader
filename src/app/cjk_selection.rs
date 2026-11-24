//! Dictionary-backed automatic selection, off the input thread.
use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{Receiver, TryRecvError},
};

pub(super) struct Lookup {
    initial: TextSelection,
    scan: TextSelection,
    receiver: Receiver<io::Result<Option<usize>>>,
    cancelled: Arc<AtomicBool>,
    pub(super) deadline: Instant,
}
impl Drop for Lookup {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}
impl Reader {
    pub(super) fn start_cjk_selection(&mut self) {
        self.cjk_selection_lookup = None;
        let Some(initial) = self.selection.clone() else {
            return;
        };
        let Some(scan) = self
            .active
            .as_ref()
            .and_then(|a| a.reading.probe.dictionary_scan(&initial))
        else {
            return;
        };
        let dictionaries = self.dictionaries.clone();
        let book = self.book_dictionaries().to_vec();
        let excluded = self.book_dictionary_exclusions().to_vec();
        let (order, disabled) = self.dictionary_settings.paths();
        let (order, disabled) = (order.to_vec(), disabled.to_vec());
        let forms = self.word_forms_enabled();
        let text = scan.text.clone();
        let cancelled = Arc::new(AtomicBool::new(false));
        let flag = cancelled.clone();
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        match std::thread::Builder::new()
            .name("dictionary-selection".into())
            .spawn(move || {
                let result = dictionaries.longest_cjk_prefix(
                    &text,
                    dictionary::SourceScope {
                        preferred: &book,
                        excluded: &excluded,
                        order: &order,
                        disabled: &disabled,
                    },
                    forms,
                    &flag,
                );
                let _ = sender.send(result);
            }) {
            Ok(_) => {
                self.cjk_selection_lookup = Some(Lookup {
                    initial,
                    scan,
                    receiver,
                    cancelled,
                    deadline: Instant::now() + Duration::from_millis(100),
                })
            }
            // Keep the immediate ICU selection usable if a worker cannot start.
            Err(error) => eprintln!("reader dictionary_selection_start_failed={}", error.kind()),
        }
    }

    pub(super) fn poll_cjk_selection(&mut self, now: Instant) -> Option<Change> {
        let job = self.cjk_selection_lookup.as_mut()?;
        if now < job.deadline {
            return None;
        }
        if self.screen != Screen::Book || self.selection.as_ref() != Some(&job.initial) {
            self.cjk_selection_lookup = None;
            self.pending_selection_action = None;
            return None;
        }
        let result = match job.receiver.try_recv() {
            Err(TryRecvError::Empty) => {
                job.deadline = now + Duration::from_millis(100);
                return None;
            }
            Ok(result) => result,
            Err(TryRecvError::Disconnected) => Ok(None),
        };
        let job = self.cjk_selection_lookup.take()?;
        // Missing/corrupt dictionaries preserve the immediate ICU selection.
        let selected = match result {
            Ok(Some(bytes))
                if bytes > 0
                    && bytes <= job.scan.text.len()
                    && job.scan.text.is_char_boundary(bytes) =>
            {
                let mut end = job.scan.start.clone();
                end.offset += bytes;
                self.active
                    .as_ref()
                    .and_then(|active| active.reading.probe.select_range(&job.scan.start, &end))
            }
            _ => None,
        };
        let selection_changed = selected
            .as_ref()
            .is_some_and(|selected| self.selection.as_ref() != Some(selected));
        if let Some(selected) = selected {
            self.selection = Some(selected);
        }
        if let Some(action) = self.pending_selection_action.take() {
            return Some(self.apply_selection_long_press_action(action));
        }
        selection_changed.then_some(Change::Partial)
    }
}

#[cfg(test)]
mod tests;
