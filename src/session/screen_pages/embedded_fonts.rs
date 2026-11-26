//! Inspect publication fonts using isolated chapter layouts for the open edition.
use super::{
    ScreenPageSources,
    background::{
        ARCHIVE_LIMIT, FIRMWARE_FONTS, FONT_COUNT_LIMIT, FONT_LIMIT, check_cancelled, read_snapshot,
    },
    same_spine,
};
use crate::{
    Engine, Probe, Result, Settings,
    book_style::BookStyle,
    embedded_font_usage::Families,
    epub::{Book, Chapter},
    loose,
    session::Reading,
    user_hyphenation, user_styles,
};
use primitive_app::with_storage;
use std::{
    io,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
};

const MAX_SPINE_ITEMS: usize = 96;
const MAX_SPINE_BYTES: u64 = 2 * 1024 * 1024;

pub(crate) struct Report {
    pub(crate) families: Families,
    pub(crate) scope: Scope,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Scope {
    Checking,
    Book,
    CurrentChapter,
    Disabled,
}

pub(crate) struct Job {
    running: JoinHandle<Result<Report>>,
    cancelled: Arc<AtomicBool>,
}

impl Job {
    pub(crate) fn start(reading: &Reading) -> Result<Self> {
        let request = Request {
            path: reading.config.book.clone(),
            edition: reading.probe.book,
            spine: reading.probe.chapter_entries().to_vec(),
            chapter: reading.chapter,
            current: reading.probe.embedded_font_usage.clone(),
            engine: reading.config.engine,
            facing: reading.config.facing_pages,
            settings: reading.probe.settings,
            fonts: reading.config.fonts.clone(),
            styles: reading.config.user_styles.clone(),
            hyphenation: (*reading.config.user_hyphenation).clone(),
            folder_snapshot: reading.config.folder_snapshot.clone(),
        };
        let cancelled = Arc::new(AtomicBool::new(false));
        let token = cancelled.clone();
        let running = thread::Builder::new()
            .name("reader-embedded-font-report".into())
            .spawn(move || request.run(&token))?;
        Ok(Self { running, cancelled })
    }

    pub(crate) fn finished(&self) -> bool {
        self.running.is_finished()
    }

    pub(crate) fn collect(self) -> Result<Report> {
        self.running
            .join()
            .map_err(|_| "Embedded font inspection stopped")?
    }

    pub(crate) fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

struct Request {
    path: PathBuf,
    edition: u64,
    spine: Vec<Chapter>,
    chapter: usize,
    current: Families,
    engine: Engine,
    facing: bool,
    settings: Settings,
    fonts: Vec<PathBuf>,
    styles: user_styles::Snapshot,
    hyphenation: user_hyphenation::Rules,
    folder_snapshot: Option<Arc<super::folder_sources::FolderProof>>,
}

impl Request {
    fn eligible(&self, sources: &ScreenPageSources) -> bool {
        let supported_format = crate::epub_members::is_epub_path(&self.path)
            || loose::Format::for_path(&self.path) == Some(loose::Format::Mobi);
        self.engine == Engine::Parley
            && !self.facing
            && supported_format
            && self.spine.len() <= MAX_SPINE_ITEMS
            && sources.book.len <= ARCHIVE_LIMIT
            && sources.fonts.len() <= FONT_COUNT_LIMIT
            && self
                .fonts
                .iter()
                .zip(&sources.fonts)
                .filter(|(path, _)| path.parent() != Some(Path::new(FIRMWARE_FONTS)))
                .try_fold(0_u64, |bytes, (_, stamp)| bytes.checked_add(stamp.len))
                .is_some_and(|bytes| bytes <= FONT_LIMIT)
    }

    fn run(self, cancelled: &AtomicBool) -> Result<Report> {
        let disabled = !self.settings.publisher_styles
            || !self.settings.embedded_fonts
            || self.settings.override_fonts;
        let sources = with_storage(|| {
            check_cancelled(cancelled)?;
            Ok(ScreenPageSources::read_paths(&self.path, &self.fonts))
        })?;
        if let Some(sources) = sources.filter(|sources| !disabled && self.eligible(sources)) {
            let bytes = read_snapshot(&self.path, &sources.book, cancelled)?.into();
            let mut book = Book::font_inspection_snapshot(&self.path, bytes)?;
            self.verify_publication(&book)?;
            if book.supports_whole_book_font_inspection()
                && book
                    .chapter_offsets()?
                    .last()
                    .is_some_and(|&size| size <= MAX_SPINE_BYTES)
            {
                return self.inspect_book(book, sources, cancelled);
            }
        }
        let mut report = Report {
            families: self.current.clone(),
            scope: if disabled {
                Scope::Disabled
            } else {
                Scope::CurrentChapter
            },
        };
        with_storage(|| {
            check_cancelled(cancelled)?;
            let mut book = match &self.folder_snapshot {
                Some(proof) => {
                    proof.revalidate(&self.path, cancelled)?;
                    Book::open_folder_snapshot(proof.members())
                }
                None => Book::open(&self.path),
            }
            .map_err(io::Error::other)?;
            self.verify_publication(&book).map_err(io::Error::other)?;
            report
                .families
                .include_catalog(BookStyle::embedded_font_catalog(&mut book, self.chapter));
            if let Some(proof) = &self.folder_snapshot {
                proof.revalidate(&self.path, cancelled)?;
            }
            check_cancelled(cancelled)
        })?;
        Ok(report)
    }

    fn inspect_book(
        &self,
        mut book: Book,
        sources: ScreenPageSources,
        cancelled: &AtomicBool,
    ) -> Result<Report> {
        let mut families = self.current.clone();
        if book.may_have_embedded_fonts() {
            families.include_catalog(BookStyle::embedded_font_catalog(&mut book, self.chapter));
            let mut book = Some(book);
            let fonts = crate::font_context_with_font_data(
                &self.fonts,
                Path::new(FIRMWARE_FONTS),
                |path| {
                    let index = self
                        .fonts
                        .iter()
                        .position(|font| font == path)
                        .ok_or("The reading font changed during inspection")?;
                    let bytes = read_snapshot(path, &sources.fonts[index], cancelled)?;
                    Ok(parley::fontique::Blob::new(Arc::new(bytes)))
                },
            )?;
            for index in 0..self.spine.len() {
                check_cancelled(cancelled)?;
                let probe = Probe::layout_opened_book(
                    &mut book,
                    self.engine,
                    self.settings,
                    index,
                    None,
                    &self.hyphenation,
                    &self.styles,
                    || Ok(fonts.clone()),
                )?;
                if index == self.chapter && probe.embedded_font_usage != self.current {
                    return Err("The visible chapter’s font use changed during inspection".into());
                }
                families.merge(&probe.embedded_font_usage);
                thread::yield_now();
            }
        }
        with_storage(|| {
            check_cancelled(cancelled)?;
            (ScreenPageSources::read_paths(&self.path, &self.fonts).as_ref() == Some(&sources))
                .then_some(())
                .ok_or_else(|| {
                    io::Error::other("The publication or reading fonts changed during inspection")
                })
        })?;
        Ok(Report {
            families,
            scope: Scope::Book,
        })
    }

    fn verify_publication(&self, book: &Book) -> Result<()> {
        if book.revision() != self.edition || !same_spine(&self.spine, &book.chapters) {
            return Err("The publication changed during embedded font inspection".into());
        }
        Ok(())
    }
}
