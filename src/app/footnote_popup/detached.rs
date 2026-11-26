//! Source-bound background previews for explicit EPUB notes outside the spine.
use super::*;
use crate::session::screen_pages::FileStamp;
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};

const POLL: Duration = Duration::from_millis(100);
const MAX_FONT_BYTES: u64 = 64 * 1024 * 1024;

struct Guard(Arc<AtomicBool>);
impl Drop for Guard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

pub(in crate::app) struct Job {
    id: u64,
    origin: super::super::HistoryLocation,
    scroll_top: Option<f32>,
    viewport: ReadingViewport,
    target: Destination,
    reference: Option<TextSelection>,
    recipe: crate::session::SearchLayoutRecipe,
    popup_settings: Settings,
    prefer: bool,
    fonts: Vec<PathBuf>,
    fallback_fonts: Vec<PathBuf>,
    cancelled: Arc<AtomicBool>,
    receiver: mpsc::Receiver<crate::Result<Output>>,
    deadline: Instant,
}
impl Drop for Job {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}
impl Job {
    pub(in crate::app) fn deadline(&self) -> Instant {
        self.deadline
    }
}

struct Output {
    proof: super::super::bookmark_browser::other_epub::Proof,
    stamps: Vec<FileStamp>,
    excerpt: String,
    preview: crate::PreparedParleyChapter,
    frame: GrayImage,
}

fn font_stamps(fonts: &[PathBuf], fallback: &[PathBuf]) -> io::Result<Vec<FileStamp>> {
    if fonts.is_empty() || fonts.len().saturating_add(fallback.len()) > 32 {
        return Err(io::Error::other("The footnote fonts cannot be verified"));
    }
    let mut total = 0_u64;
    fonts
        .iter()
        .chain(fallback)
        .map(|path| {
            let meta = std::fs::metadata(path)?;
            total = total
                .checked_add(meta.len())
                .ok_or_else(|| io::Error::other("The footnote fonts are too large"))?;
            if total > MAX_FONT_BYTES {
                return Err(io::Error::other("The footnote fonts are too large"));
            }
            FileStamp::from_metadata(&meta)
                .ok_or_else(|| io::Error::other("The footnote font is unavailable"))
        })
        .collect()
}

impl Reader {
    pub(super) fn start_detached_note(
        &mut self,
        target: Destination,
        context: ReferenceContext,
        reference: Option<TextSelection>,
    ) -> bool {
        let Some(active) = self.active.as_ref() else {
            return false;
        };
        let reading = &active.reading;
        if active.entry.format != crate::library::DocumentFormat::Epub
            || crate::epub_members::is_folder_book(&active.entry.path)
            || reading.page_turn_pending()
        {
            return false;
        }
        let recipe = reading.config.search_layout_recipe(&reading.probe);
        if !recipe.has_verified_sources() {
            return false;
        }
        let Some(origin) = self.current_history_location() else {
            return false;
        };
        let scroll_top = reading.scroll_history_top();
        let viewport = self.reading_viewport();
        let popup_settings = self.footnote_settings(reading, viewport);
        let fonts = if self.controls.footnote_book_font {
            reading.config.fonts.clone()
        } else {
            self.fonts.clone()
        };
        let fallback_fonts = if self.controls.footnote_book_font && fonts != self.fonts {
            self.fonts.clone()
        } else {
            Vec::new()
        };
        let root = self.library.verification_root().to_owned();
        let relative = active.entry.path.clone();
        let id = active.id;
        let publication = reading.probe.book;
        let settings = reading.probe.settings;
        let user = reading.config.user_styles.clone();
        let rules = reading.config.user_hyphenation.as_ref().clone();
        let toc_target = reading
            .probe
            .toc
            .iter()
            .any(|entry| entry.destination.as_ref() == Some(&target));
        let prefer = self.controls.prefer_footnotes;
        let worker_target = target.clone();
        let worker_fonts = fonts.clone();
        let worker_fallback = fallback_fonts.clone();
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = Arc::clone(&cancelled);
        if self.detached_note_busy.swap(true, Ordering::AcqRel) {
            self.notice = Some("The previous footnote is still stopping".into());
            return true;
        }
        let busy = Arc::clone(&self.detached_note_busy);
        let (tx, receiver) = mpsc::sync_channel(1);
        let spawned = std::thread::Builder::new()
            .name("reader-detached-note".into())
            .spawn(move || {
                let _guard = Guard(busy);
                let check = || -> crate::Result<()> {
                    if worker_cancelled.load(Ordering::Relaxed) {
                        Err("Footnote cancelled".into())
                    } else {
                        Ok(())
                    }
                };
                let result = (|| -> crate::Result<Output> {
                    check()?;
                    let stamps = with_storage(|| font_stamps(&worker_fonts, &worker_fallback))?;
                    let (proof, bytes) =
                        super::super::bookmark_browser::other_epub::Proof::capture(
                            root,
                            relative,
                            id,
                            &worker_cancelled,
                        )?;
                    let mut book = Book::open_snapshot(bytes)?;
                    if book.revision() != publication {
                        return Err("The book changed; reopen it first".into());
                    }
                    let index = book.append_detached_note(&worker_target.path)?;
                    check()?;
                    let xhtml = book.chapter(index)?;
                    let source = book.chapter_source(&worker_target.path, &xhtml)?;
                    let styles = crate::book_style::BookStyle::load_for_flow_with_media(
                        &mut book,
                        &worker_target.path,
                        &source,
                        settings.publisher_styles,
                        &user,
                        settings.base_stylesheet,
                        Some(crate::book_style::MediaContext::from_settings(settings)),
                    )?
                    .compute(&source.elements);
                    let roots = footnotes::LayoutRoots::new(&source, &styles)
                        .ok_or("The footnote layout is unavailable")?;
                    let fragment = worker_target
                        .fragment
                        .as_deref()
                        .ok_or("The footnote has no target")?;
                    let excerpt = footnotes::extract_with_layout(
                        &xhtml,
                        fragment,
                        &context,
                        std::cmp::Ordering::Less,
                        toc_target,
                        prefer,
                        &roots,
                    )
                    .ok_or("The footnote could not be extracted")?;
                    check()?;
                    let mut opened = Some(book);
                    let mut preview = Probe::layout_opened_book_cancellable(
                        &mut opened,
                        Engine::Parley,
                        popup_settings,
                        index,
                        Some(excerpt.clone()),
                        &rules,
                        &crate::user_styles::Snapshot::default(),
                        || {
                            with_storage(|| {
                                crate::font_context(&worker_fonts)
                                    .or_else(|_| crate::font_context(&worker_fallback))
                                    .map_err(io::Error::other)
                            })
                            .map_err(Into::into)
                        },
                        false,
                        &check,
                    )?;
                    let frame = Footnote::frame(&mut preview, 0)?;
                    let preview = preview
                        .into_prepared_parley()
                        .ok_or("The footnote renderer is unavailable")?;
                    check()?;
                    if with_storage(|| font_stamps(&worker_fonts, &worker_fallback))? != stamps {
                        return Err("The footnote fonts changed".into());
                    }
                    proof.revalidate()?;
                    check()?;
                    Ok(Output {
                        proof,
                        stamps,
                        excerpt,
                        preview,
                        frame,
                    })
                })();
                if !worker_cancelled.load(Ordering::Relaxed) {
                    let _ = tx.send(result);
                }
            });
        if spawned.is_err() {
            self.detached_note_busy.store(false, Ordering::Release);
            self.notice = Some("Could not start the footnote preview".into());
            return true;
        }
        self.detached_note = Some(Job {
            id,
            origin,
            scroll_top,
            viewport,
            target,
            reference,
            recipe,
            popup_settings,
            prefer,
            fonts,
            fallback_fonts,
            cancelled,
            receiver,
            deadline: Instant::now() + POLL,
        });
        true
    }

    pub(in crate::app) fn cancel_detached_note(&mut self) {
        self.detached_note = None;
    }

    pub(in crate::app) fn detached_note_input(&mut self, input: Input) -> Option<Change> {
        self.detached_note.as_ref()?;
        Some(match input {
            Input::PageBack => {
                self.cancel_detached_note();
                Change::Partial
            }
            Input::Tap(..) | Input::Swipe { .. } => {
                self.cancel_detached_note();
                Change::Partial
            }
            _ => Change::None,
        })
    }

    pub(in crate::app) fn draw_detached_note(&self, canvas: &mut Canvas) {
        if self.detached_note.is_none() {
            return;
        }
        let (width, height) = self.reading_viewport().dimensions();
        let panel = Rect::new(
            (width as i32 - 600) / 2,
            (height as i32 - 260) / 2,
            600,
            260,
        );
        canvas.popup(panel);
        canvas.text(
            Rect::new(panel.x + 30, panel.y + 25, 540, 60),
            "Opening footnote…",
            TextStyle::new(28).bold(),
        );
        canvas.button(
            Rect::new(panel.x + 180, panel.bottom() - 120, 240, 100),
            "Cancel",
            false,
            false,
        );
    }

    pub(in crate::app) fn poll_detached_note(&mut self, now: Instant) -> Option<Change> {
        let mut job = self.detached_note.take()?;
        if now < job.deadline {
            self.detached_note = Some(job);
            return None;
        }
        let result = match job.receiver.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => {
                job.deadline = now + POLL;
                self.detached_note = Some(job);
                return None;
            }
            Err(mpsc::TryRecvError::Disconnected) => Err("Footnote preview stopped".into()),
        };
        let valid = self.screen == Screen::Book
            && self.open_book_id() == Some(job.id)
            && self.current_history_location().as_ref() == Some(&job.origin)
            && self.reading_viewport() == job.viewport
            && self.touch_hold.is_none()
            && self.controls.footnote_popups
            && self.controls.prefer_footnotes == job.prefer
            && self.pending_orientation.is_none()
            && self.opening_execution.is_none()
            && !self.selection_mode
            && self.selection.is_none()
            && self.active.as_ref().is_some_and(|active| {
                let fonts = if self.controls.footnote_book_font {
                    &active.reading.config.fonts
                } else {
                    &self.fonts
                };
                let fallback = if self.controls.footnote_book_font && fonts != &self.fonts {
                    self.fonts.as_slice()
                } else {
                    &[]
                };
                !active.reading.page_turn_pending()
                    && active.reading.scroll_history_top() == job.scroll_top
                    && fonts == &job.fonts
                    && fallback == job.fallback_fonts.as_slice()
                    && active
                        .reading
                        .config
                        .search_layout_recipe(&active.reading.probe)
                        == job.recipe
                    && self.footnote_settings(&active.reading, job.viewport) == job.popup_settings
            });
        if !valid {
            return Some(Change::Partial);
        }
        match result {
            Ok(output)
                if output.proof.revalidate().is_ok()
                    && with_storage(|| font_stamps(&job.fonts, &job.fallback_fonts))
                        .ok()
                        .as_ref()
                        == Some(&output.stamps) =>
            {
                let preview = output.preview.into_probe();
                let cursor = preview.page_anchor(0);
                self.show(Screen::Footnote);
                self.footnote = Some(Footnote {
                    target: job.target.clone(),
                    reference: job.reference.clone(),
                    preview,
                    viewport: job.viewport,
                    source: output.excerpt,
                    cursor,
                    page: 0,
                    frame: output.frame,
                    selection: None,
                    detached: true,
                });
            }
            Ok(_) => self.notice = Some("The book or footnote fonts changed".into()),
            Err(error) => self.notice = Some(error.to_string()),
        }
        Some(Change::Partial)
    }
}
