//! Exact screen pages tied to one EPUB edition and layout.
pub(crate) mod background;

use std::cell::RefCell;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::SystemTime;

use super::BookConfig;
use crate::{Engine, Probe, Settings, epub::Chapter, user_hyphenation, user_styles};

/// The file and the ordered reading font stack, including fallbacks. EPUB
/// publisher CSS and embedded fonts are members of the same archive.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct ScreenPageSources {
    book: FileStamp,
    fonts: Vec<FileStamp>,
}

impl ScreenPageSources {
    pub(super) fn read(config: &BookConfig) -> Option<Self> {
        if config.settings.status_header_height == 0
            || config.engine != Engine::Parley
            || config.scroll
            || config.facing_pages
            || config.hidden_flows.enabled
            || crate::epub_members::is_folder_book(&config.book)
        {
            return None;
        }
        Self::read_paths(&config.book, &config.fonts)
    }

    fn read_paths(book: &Path, fonts: &[PathBuf]) -> Option<Self> {
        Some(Self {
            book: FileStamp::read(book)?,
            fonts: fonts
                .iter()
                .map(|path| FileStamp::read(path))
                .collect::<Option<_>>()?,
        })
    }
}

#[derive(Clone, PartialEq, Eq)]
struct FileStamp {
    len: u64,
    modified: SystemTime,
    #[cfg(unix)]
    identity: (u64, u64, i64, i64),
}

impl FileStamp {
    fn read(path: &Path) -> Option<Self> {
        Self::from_metadata(&fs::metadata(path).ok()?)
    }

    fn from_metadata(metadata: &fs::Metadata) -> Option<Self> {
        if !metadata.is_file() {
            return None;
        }
        Some(Self {
            len: metadata.len(),
            modified: metadata.modified().ok()?,
            #[cfg(unix)]
            identity: (
                metadata.dev(),
                metadata.ino(),
                metadata.ctime(),
                metadata.ctime_nsec(),
            ),
        })
    }
}

#[derive(Clone, Default)]
pub(crate) struct ScreenPages(Rc<RefCell<Option<Generation>>>);

impl ScreenPages {
    pub(super) fn reset(&self) {
        *self.0.borrow_mut() = None;
    }

    pub(super) fn observe(&self, config: &BookConfig, probe: &Probe, chapter: usize) {
        let Some(sources) = probe.screen_page_sources.as_ref() else {
            self.reset();
            return;
        };
        let mut slot = self.0.borrow_mut();
        if slot.as_ref().is_some_and(|generation| {
            !generation.matches(config, probe) || !generation.same_spine(probe.chapter_entries())
        }) {
            *slot = None;
        }
        slot.get_or_insert_with(|| Generation::new(config, probe, sources))
            .record(chapter, &probe.chapter, PageSample::new(probe));
    }

    pub(super) fn position(
        &self,
        config: &BookConfig,
        probe: &Probe,
        chapter: usize,
        page: usize,
    ) -> Option<(usize, usize)> {
        let slot = self.0.borrow();
        let generation = slot.as_ref()?;
        if !generation.matches(config, probe)
            || generation.spine.get(chapter)?.href != probe.chapter
            || *generation.samples.get(chapter)? != Some(PageSample::new(probe))
        {
            return None;
        }
        generation.position(chapter, page)
    }
}

struct Generation {
    id: Rc<()>,
    book: PathBuf,
    publication: u64,
    layout_settings: Settings,
    fonts: Vec<PathBuf>,
    styles: user_styles::Snapshot,
    hyphenation: Rc<user_hyphenation::Rules>,
    hide_nonlinear: bool,
    sources: ScreenPageSources,
    spine: Vec<Chapter>,
    samples: Vec<Option<PageSample>>,
    missing: usize,
    prefix: Option<Vec<usize>>,
    rejected: bool,
    background_rejected: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct PageSample {
    source_revision: u64,
    count: usize,
}

impl PageSample {
    fn new(probe: &Probe) -> Self {
        Self {
            source_revision: probe.source.revision,
            count: probe.page_count(),
        }
    }
}

impl Generation {
    fn new(config: &BookConfig, probe: &Probe, sources: &ScreenPageSources) -> Self {
        let spine = probe.chapter_entries().to_vec();
        let missing = spine.len();
        Self {
            id: Rc::new(()),
            book: config.book.clone(),
            publication: probe.book,
            layout_settings: layout_settings(config.settings),
            fonts: config.fonts.clone(),
            styles: config.user_styles.clone(),
            hyphenation: config.user_hyphenation.clone(),
            hide_nonlinear: config.hide_nonlinear,
            sources: sources.clone(),
            spine,
            samples: vec![None; missing],
            missing,
            prefix: None,
            rejected: false,
            background_rejected: false,
        }
    }

    fn matches(&self, config: &BookConfig, probe: &Probe) -> bool {
        !config.scroll
            && !config.facing_pages
            && !config.hidden_flows.enabled
            && config.engine == Engine::Parley
            && self.hide_nonlinear == config.hide_nonlinear
            && self.layout_settings == layout_settings(config.settings)
            && self.layout_settings == layout_settings(probe.settings)
            && self.publication == probe.book
            && self.book == config.book
            && self.fonts == config.fonts
            && self.styles == config.user_styles
            && self.hyphenation == config.user_hyphenation
            && probe.screen_page_sources.as_ref() == Some(&self.sources)
    }

    fn same_spine(&self, spine: &[Chapter]) -> bool {
        same_spine(&self.spine, spine)
    }

    fn record(&mut self, chapter: usize, href: &str, sample: PageSample) {
        if self.rejected {
            return;
        }
        if sample.count == 0
            || self
                .spine
                .get(chapter)
                .is_none_or(|entry| entry.href != href)
        {
            self.reject();
            return;
        }
        let entry = &mut self.samples[chapter];
        if let Some(previous) = entry {
            if *previous != sample {
                self.reject();
            }
            return;
        }
        *entry = Some(sample);
        self.missing -= 1;
        if self.missing == 0 {
            self.prefix = checked_prefix(&self.samples);
            if self.prefix.is_none() {
                self.reject();
            }
        }
    }

    fn reject(&mut self) {
        self.prefix = None;
        self.rejected = true;
    }

    fn position(&self, chapter: usize, page: usize) -> Option<(usize, usize)> {
        let prefix = self.prefix.as_ref()?;
        let start = *prefix.get(chapter)?;
        let end = *prefix.get(chapter.checked_add(1)?)?;
        (page < end - start).then(|| (start + page + 1, *prefix.last().unwrap()))
    }
}

fn layout_settings(settings: Settings) -> Settings {
    let defaults = Settings::default();
    // These controls only repaint or change interaction state in Reading::reflow.
    Settings {
        font_gamma_index: defaults.font_gamma_index,
        text_contrast: defaults.text_contrast,
        page_overlap_enabled: defaults.page_overlap_enabled,
        page_overlap_lines: defaults.page_overlap_lines,
        page_overlap_style: defaults.page_overlap_style,
        invert_night_images: defaults.invert_night_images,
        link_vertical_margins: defaults.link_vertical_margins,
        ..settings
    }
}

fn same_spine(expected: &[Chapter], actual: &[Chapter]) -> bool {
    expected.len() == actual.len()
        && expected.iter().zip(actual).all(|(expected, actual)| {
            expected.href == actual.href
                && expected.nonlinear == actual.nonlinear
                && expected.is_svg == actual.is_svg
        })
}

fn checked_prefix(samples: &[Option<PageSample>]) -> Option<Vec<usize>> {
    let mut prefix = Vec::with_capacity(samples.len() + 1);
    let mut total = 0_usize;
    prefix.push(total);
    for sample in samples {
        let count = sample.as_ref()?.count;
        if count == 0 {
            return None;
        }
        total = total.checked_add(count)?;
        prefix.push(total);
    }
    Some(prefix)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{PAGE_HEIGHT, Reading};
    use primitive_app::{App, WIDTH};

    fn sample(count: usize) -> Option<PageSample> {
        Some(PageSample {
            source_revision: 1,
            count,
        })
    }

    #[test]
    fn screen_pages_prefix_rejects_incomplete_zero_and_overflowing_counts() {
        assert_eq!(
            checked_prefix(&[sample(3), sample(1), sample(7)]),
            Some(vec![0, 3, 4, 11])
        );
        assert_eq!(checked_prefix(&[sample(3), None, sample(7)]), None);
        assert_eq!(checked_prefix(&[sample(0)]), None);
        assert_eq!(checked_prefix(&[sample(usize::MAX), sample(1)]), None);
    }

    pub(super) fn config(fixture: &str) -> BookConfig {
        BookConfig {
            user_styles: Default::default(),
            user_hyphenation: Default::default(),
            engine: Engine::Parley,
            book: crate::test_fixture(fixture),
            fonts: vec![crate::test_fixture("fonts/Silkscreen-Regular.ttf")],
            settings: Settings {
                width: WIDTH,
                height: PAGE_HEIGHT,
                font_size: 58.0,
                status_header_height: 48,
                ..Settings::default()
            },
            scroll: false,
            facing_pages: false,
            hide_nonlinear: false,
            hidden_flows: Default::default(),
            chapter_offsets: Default::default(),
            screen_pages: Default::default(),
        }
    }

    #[test]
    fn screen_pages_map_normal_layouts_in_spine_order_and_reset_on_usb() {
        let mut reading = Reading::restore(config("field-notes-epub2.epub"), 1, 0, None).unwrap();
        let second = reading.probe.page_count();
        assert_eq!(reading.whole_book_screen_pages(), None);
        reading.jump(1, None).unwrap();
        assert_eq!(reading.whole_book_screen_pages(), None);

        reading.jump(0, None).unwrap();
        let first = reading.probe.page_count();
        assert_eq!(reading.whole_book_screen_pages(), Some((1, first + second)));
        reading.jump(1, None).unwrap();
        assert_eq!(
            reading.whole_book_screen_pages(),
            Some((first + 1, first + second))
        );
        let cache = reading.config.screen_pages.0.borrow();
        let complete = cache.as_ref().unwrap();
        assert_eq!(
            complete.position(0, first - 1),
            Some((first, first + second))
        );
        assert_eq!(
            complete.position(1, second - 1),
            Some((first + second, first + second))
        );
        assert_eq!(complete.position(1, second), None);
        assert_eq!(complete.position(2, 0), None);
        drop(cache);

        let mut cache = reading.config.screen_pages.0.borrow_mut();
        let complete = cache.as_mut().unwrap();
        let mut invalid = PageSample::new(&reading.probe);
        invalid.source_revision ^= 1;
        complete.record(1, &reading.probe.chapter, invalid);
        assert_eq!(complete.position(1, 0), None);
        drop(cache);

        reading.storage_returned().unwrap();
        assert_eq!(reading.whole_book_screen_pages(), None);
        reading.jump(0, None).unwrap();
        assert_eq!(reading.whole_book_screen_pages(), None);
        reading.jump(1, None).unwrap();
        assert!(reading.whole_book_screen_pages().is_some());
        assert!(reading.jump(usize::MAX, None).is_err());
        assert_eq!(reading.whole_book_screen_pages(), None);
    }

    #[test]
    fn screen_pages_invalidate_css_font_fallback_and_geometry_changes() {
        let mut reading = Reading::open(config("field-notes-epub2.epub")).unwrap();
        reading.jump(1, None).unwrap();
        assert!(reading.whole_book_screen_pages().is_some());
        let mut config = reading.config.clone();
        config.user_styles =
            user_styles::Snapshot::new("p { font-size: 125% !important; }".into()).unwrap();
        reading = Reading::restore(config, 1, 0, None).unwrap();
        assert_eq!(reading.whole_book_screen_pages(), None);
        reading.jump(0, None).unwrap();
        assert!(reading.whole_book_screen_pages().is_some());

        let mut fonts = reading.config.fonts.clone();
        fonts.push(crate::test_fixture("fonts/NotoSans-Regular.ttf"));
        reading.set_fonts(fonts).unwrap();
        assert_eq!(reading.whole_book_screen_pages(), None);
        reading.jump(1, None).unwrap();
        assert!(reading.whole_book_screen_pages().is_some());

        let settings = Settings {
            height: PAGE_HEIGHT - 64,
            ..reading.config.settings
        };
        reading.reflow(settings).unwrap();
        assert_eq!(reading.whole_book_screen_pages(), None);
        reading.jump(0, None).unwrap();
        assert!(reading.whole_book_screen_pages().is_some());
        reading.set_scroll(true).unwrap();
        reading.jump(1, None).unwrap();
        assert_eq!(reading.whole_book_screen_pages(), None);
        reading.set_scroll(false).unwrap();
        assert_eq!(reading.whole_book_screen_pages(), None);
        reading.set_facing_pages(true).unwrap();
        reading.jump(0, None).unwrap();
        assert_eq!(reading.whole_book_screen_pages(), None);
    }

    #[test]
    fn screen_pages_keep_the_completed_count_through_repaints() {
        let mut reading = Reading::open(config("field-notes-epub2.epub")).unwrap();
        reading.jump(1, None).unwrap();
        let expected = reading.whole_book_screen_pages().unwrap();
        let anchor = reading.anchor();
        let mut settings = reading.config.settings;
        settings.font_gamma_index = 24;
        settings.text_contrast = true;
        reading.reflow(settings).unwrap();
        assert_eq!(reading.whole_book_screen_pages(), Some(expected));

        settings.page_overlap_enabled = true;
        settings.page_overlap_lines = 7;
        settings.page_overlap_style = crate::PageOverlapStyle::Line;
        reading.reflow(settings).unwrap();
        assert_eq!(reading.whole_book_screen_pages(), Some(expected));

        settings.invert_night_images = true;
        settings.link_vertical_margins = true;
        reading.reflow(settings).unwrap();
        assert_eq!(reading.whole_book_screen_pages(), Some(expected));
        assert_eq!(reading.anchor(), anchor);

        settings.font_size += 8.0;
        reading.reflow(settings).unwrap();
        assert_eq!(reading.whole_book_screen_pages(), None);
    }

    #[test]
    fn screen_pages_keep_default_and_vertical_layouts_out_of_the_index() {
        let mut default_config = config("field-notes-epub2.epub");
        default_config.settings.status_header_height = 0;
        let mut default_reading = Reading::open(default_config).unwrap();
        default_reading.jump(1, None).unwrap();
        assert!(default_reading.config.screen_pages.0.borrow().is_none());

        let mut vertical = Reading::open(config("vertical-chapters.epub")).unwrap();
        vertical.jump(1, None).unwrap();
        assert!(vertical.probe.is_vertical());
        vertical.jump(2, None).unwrap();
        vertical.jump(0, None).unwrap();
        assert_eq!(vertical.whole_book_screen_pages(), None);
    }
}
