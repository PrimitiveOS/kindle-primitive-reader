//! Single-worker, cancellable scheduling for advisory transfer discovery.
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

const RETRY: Duration = Duration::from_millis(100);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Request {
    pub(super) book: u64,
    pub(super) relative: PathBuf,
    pub(super) root: PathBuf,
    pub(super) fixed: bool,
    pub(super) device: Option<String>,
    pub(super) applied: Option<String>,
}

struct BusyGuard(Arc<AtomicBool>);
impl Drop for BusyGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

struct Running {
    request: Request,
    cancelled: Arc<AtomicBool>,
    result: mpsc::Receiver<Option<String>>,
    deadline: Instant,
}
impl Drop for Running {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

#[derive(Default)]
pub(super) struct Manager {
    busy: Arc<AtomicBool>,
    pending: Option<(Request, Instant)>,
    running: Option<Running>,
    completed: Option<(Request, Option<String>)>,
}

impl Manager {
    /// Replace any old request; a worker still stopping is retried later.
    pub(super) fn start(&mut self, request: Request) {
        self.cancel();
        let now = Instant::now();
        self.pending = Some((request, now));
        self.attempt(now);
    }

    pub(super) fn cancel(&mut self) {
        self.pending = None;
        self.running = None;
        self.completed = None;
    }

    pub(super) fn deadline(&self) -> Option<Instant> {
        self.completed
            .as_ref()
            .map(|_| Instant::now())
            .or_else(|| self.running.as_ref().map(|job| job.deadline))
            .or_else(|| self.pending.as_ref().map(|(_, deadline)| *deadline))
    }

    /// Poll without waiting. A completed failed or ineligible request yields None as its candidate.
    pub(super) fn poll(&mut self, now: Instant) -> Option<(Request, Option<String>)> {
        if let Some(completed) = self.completed.take() {
            return Some(completed);
        }
        if let Some(job) = &mut self.running {
            if now < job.deadline {
                return None;
            }
            match job.result.try_recv() {
                Ok(candidate) => {
                    let request = job.request.clone();
                    self.running = None;
                    return Some((request, candidate));
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    let request = job.request.clone();
                    self.running = None;
                    return Some((request, None));
                }
                Err(mpsc::TryRecvError::Empty) => {
                    job.deadline = now + RETRY;
                    return None;
                }
            }
        }
        self.attempt(now);
        self.completed.take()
    }

    fn attempt(&mut self, now: Instant) {
        let Some((_, deadline)) = &self.pending else {
            return;
        };
        if now < *deadline {
            return;
        }
        if self
            .busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            if let Some((_, deadline)) = &mut self.pending {
                *deadline = now + RETRY;
            }
            return;
        }
        let Some((request, _)) = self.pending.take() else {
            self.busy.store(false, Ordering::Release);
            return;
        };
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancelled);
        let worker_busy = Arc::clone(&self.busy);
        let worker_request = request.clone();
        let (tx, result) = mpsc::sync_channel(1);
        let spawned = std::thread::Builder::new()
            .name("reader-annotation-discovery".into())
            .spawn(move || {
                let _guard = BusyGuard(worker_busy);
                let candidate = super::discovery::discover(
                    &worker_request.root,
                    &worker_request.relative,
                    worker_request.fixed,
                    worker_request.device.as_deref(),
                    worker_request.applied.as_deref(),
                    &worker_cancel,
                )
                .ok()
                .flatten();
                if !worker_cancel.load(Ordering::Relaxed) {
                    let _ = tx.send(candidate);
                }
            });
        if spawned.is_err() {
            self.busy.store(false, Ordering::Release);
            self.completed = Some((request, None));
            return;
        }
        self.running = Some(Running {
            request,
            cancelled,
            result,
            deadline: now + RETRY,
        });
    }
}
