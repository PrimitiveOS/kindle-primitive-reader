//! Opt-in, bounded whole-book Parley layouts for a committed typography edit.
use std::cell::RefCell;
use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};

use super::{
    ScreenPageSources,
    background::{check_cancelled, read_snapshot},
    same_spine,
};
use crate::epub::{Book, Chapter};
use crate::session::{BookConfig, Reading};
use crate::{
    Engine, PreparedParleyChapter, Probe, Result, Settings, user_hyphenation, user_styles,
};
use primitive_app::with_storage;

const MAX_CHAPTERS: usize = 32;
const MAX_ARCHIVE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_SPINE_BYTES: u64 = 256 * 1024;
const MAX_FONT_BYTES: u64 = 16 * 1024 * 1024;
const MAX_FONTS: usize = 64;
const MAX_ELEMENTS: usize = 6000;
const MAX_PAGES: usize = 384;
const FIRMWARE_FONTS: &str = "/usr/java/lib/fonts";

#[derive(Clone, Default)]
pub(crate) struct Cached(Rc<RefCell<Option<Layouts>>>);

struct Layouts {
    sources: ScreenPageSources,
    chapters: Vec<Option<Probe>>,
}

impl Cached {
    #[cfg(test)]
    pub(crate) fn empty_for_test(config: &BookConfig, chapters: usize) -> Self {
        let sources =
            with_storage(|| Ok(ScreenPageSources::read_paths(&config.book, &config.fonts)))
                .unwrap()
                .unwrap();
        Self(Rc::new(RefCell::new(Some(Layouts {
            sources,
            chapters: std::iter::repeat_with(|| None).take(chapters).collect(),
        }))))
    }
    pub(in crate::session) fn take(
        &self,
        config: &BookConfig,
        chapter: usize,
    ) -> Result<Option<Probe>> {
        let mut cache = self.0.borrow_mut();
        let Some(layouts) = cache.as_mut() else {
            return Ok(None);
        };
        if !current_sources(config, &layouts.sources)? {
            *cache = None;
            return Err("The EPUB or reading fonts changed. Reopen the book.".into());
        }
        Ok(layouts.chapters.get_mut(chapter).and_then(Option::take))
    }

    pub(in crate::session) fn put(&self, chapter: usize, mut probe: Probe) {
        if let Some(layouts) = self.0.borrow_mut().as_mut()
            && probe.screen_page_sources.as_ref() == Some(&layouts.sources)
            && let Some(slot) = layouts.chapters.get_mut(chapter)
        {
            if let crate::Renderer::Parley(layout) = &mut probe.renderer {
                layout.release_raster_cache();
            }
            *slot = Some(probe);
        }
    }

    #[cfg(test)]
    pub(crate) fn contains(&self, chapter: usize) -> bool {
        self.0
            .borrow()
            .as_ref()
            .is_some_and(|layouts| layouts.chapters.get(chapter).is_some_and(Option::is_some))
    }
}

pub(crate) struct Job {
    cancelled: Arc<AtomicBool>,
    thread: JoinHandle<Result<Prepared>>,
}

impl Job {
    pub(crate) fn start(config: &BookConfig, reading: &Reading) -> Result<Self> {
        let request = Request::new(config, reading)?;
        let cancelled = Arc::new(AtomicBool::new(false));
        let running = cancelled.clone();
        let thread = thread::Builder::new()
            .name("reader-whole-typography".into())
            .spawn(move || request.run(&running))?;
        Ok(Self { cancelled, thread })
    }

    pub(crate) fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    pub(crate) fn finished(&self) -> bool {
        self.thread.is_finished()
    }

    pub(crate) fn collect(self) -> Result<Prepared> {
        self.thread
            .join()
            .map_err(|_| "Whole-book typography worker stopped")?
    }
}

pub(crate) fn supported(reading: &Reading) -> bool {
    let config = &reading.config;
    config.engine == Engine::Parley
        && crate::epub_members::is_epub_path(&config.book)
        && reading.has_exact_local_screen_pages()
        && !reading.hides_nonlinear()
        && (2..=MAX_CHAPTERS).contains(&reading.probe.chapter_count())
        && reading
            .probe
            .source
            .blocks
            .iter()
            .all(|block| block.images.is_empty())
}

pub(crate) struct Prepared {
    sources: ScreenPageSources,
    chapters: Vec<PreparedParleyChapter>,
}

impl Prepared {
    pub(crate) fn install(self, config: &mut BookConfig) -> Result<()> {
        if !current_sources(config, &self.sources)? {
            return Err("The EPUB or reading fonts changed during whole-book preparation".into());
        }
        let chapters: Vec<_> = self
            .chapters
            .into_iter()
            .enumerate()
            .map(|(index, chapter)| {
                let mut probe = chapter.into_probe();
                probe.screen_page_sources = Some(self.sources.clone());
                config.screen_pages.observe(config, &probe, index);
                Some(probe)
            })
            .collect();
        config.eager_layouts = Cached(Rc::new(RefCell::new(Some(Layouts {
            sources: self.sources,
            chapters,
        }))));
        Ok(())
    }
}

fn current_sources(config: &BookConfig, sources: &ScreenPageSources) -> io::Result<bool> {
    with_storage(|| {
        Ok(ScreenPageSources::read_paths(&config.book, &config.fonts).as_ref() == Some(sources))
    })
}

struct Request {
    book: PathBuf,
    sources: ScreenPageSources,
    publication: u64,
    spine: Vec<Chapter>,
    fonts: Vec<PathBuf>,
    styles: user_styles::Snapshot,
    hyphenation: user_hyphenation::Rules,
    settings: Settings,
}

impl Request {
    fn new(config: &BookConfig, reading: &Reading) -> Result<Self> {
        if !supported(reading) {
            return Err("Whole-book typography needs a supported horizontal text EPUB".into());
        }
        let sources =
            with_storage(|| Ok(ScreenPageSources::read_paths(&config.book, &config.fonts)))?
                .ok_or("The EPUB or reading fonts are unavailable")?;
        if sources.book.len > MAX_ARCHIVE_BYTES
            || sources.fonts.len() > MAX_FONTS
            || config
                .fonts
                .iter()
                .zip(&sources.fonts)
                .filter(|(path, _)| path.parent() != Some(Path::new(FIRMWARE_FONTS)))
                .try_fold(0_u64, |bytes, (_, source)| bytes.checked_add(source.len))
                .is_none_or(|bytes| bytes > MAX_FONT_BYTES)
        {
            return Err("This EPUB or font stack exceeds the whole-book typography limits".into());
        }
        Ok(Self {
            book: config.book.clone(),
            sources,
            publication: reading.probe.book,
            spine: reading.probe.chapter_entries().to_vec(),
            fonts: config.fonts.clone(),
            styles: config.user_styles.clone(),
            hyphenation: (*config.user_hyphenation).clone(),
            settings: config.settings,
        })
    }

    fn run(self, cancelled: &AtomicBool) -> Result<Prepared> {
        let archive: Arc<[u8]> = read_snapshot(&self.book, &self.sources.book, cancelled)?.into();
        let fonts =
            crate::font_context_with_font_data(&self.fonts, Path::new(FIRMWARE_FONTS), |path| {
                let index = self
                    .fonts
                    .iter()
                    .position(|font| font == path)
                    .ok_or("The reading font changed during typography preparation")?;
                let bytes = read_snapshot(path, &self.sources.fonts[index], cancelled)?;
                Ok(parley::fontique::Blob::new(Arc::new(bytes)))
            })?;
        let mut chapters = Vec::with_capacity(self.spine.len());
        let mut pages = 0;
        let mut elements = 0;
        check_cancelled(cancelled)?;
        let mut book = Book::open_snapshot(archive)?;
        if book.revision() != self.publication || !same_spine(&self.spine, &book.chapters) {
            return Err("The EPUB edition or spine changed during whole-book preparation".into());
        }
        if !book.supports_eager_typography() {
            return Err(
                "This EPUB's archive or publisher font assets need chapter-first rendering".into(),
            );
        }
        if book
            .chapter_offsets()?
            .last()
            .is_none_or(|&bytes| bytes > MAX_SPINE_BYTES)
        {
            return Err("This EPUB exceeds the whole-book text limit".into());
        }
        let mut opened = Some(book);
        let check = || Ok(check_cancelled(cancelled)?);
        for index in 0..self.spine.len() {
            check()?;
            let probe = Probe::layout_opened_book_cancellable(
                &mut opened,
                Engine::Parley,
                self.settings,
                index,
                None,
                &self.hyphenation,
                &self.styles,
                || Ok(fonts.clone()),
                &check,
            )?;
            pages += probe.page_count();
            elements += probe.source.elements.len();
            if pages > MAX_PAGES
                || elements > MAX_ELEMENTS
                || probe.is_vertical()
                || probe
                    .source
                    .blocks
                    .iter()
                    .any(|block| !block.images.is_empty())
            {
                return Err("This EPUB needs chapter-first rendering".into());
            }
            chapters.push(
                probe
                    .into_prepared_parley()
                    .ok_or("This chapter needs chapter-first rendering")?,
            );
            thread::yield_now();
        }
        check_cancelled(cancelled)?;
        Ok(Prepared {
            sources: self.sources,
            chapters,
        })
    }
}
