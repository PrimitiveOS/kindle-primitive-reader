//! Exact screen pages tied to one EPUB edition and layout.
pub(crate) mod background;
pub(crate) mod eager;
pub(crate) mod embedded_fonts;
pub(crate) mod folder_sources;

use std::cell::{Cell, RefCell};
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::SystemTime;

use super::BookConfig;
use crate::generated_contents::{MAX_CHAPTER_HEADINGS, OutlineEntry, OutlineTarget, heading_depth};
use crate::{Anchor, Engine, Probe, Settings, epub::Chapter, user_hyphenation, user_styles};

/// The file and the ordered reading font stack, including fallbacks. EPUB
/// publisher CSS and embedded fonts are members of the same archive.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ScreenPageSources {
    book: FileStamp,
    fonts: Vec<FileStamp>,
}

impl ScreenPageSources {
    pub(super) fn read(config: &BookConfig) -> Option<Self> {
        if (config.settings.status_header_height == 0 && !config.screen_pages.requested())
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

    pub(super) fn read_paths(book: &Path, fonts: &[PathBuf]) -> Option<Self> {
        // A folder EPUB is opened through its mimetype marker; a stamp of that
        // single file cannot verify its other independently mutable members.
        if crate::epub_members::is_folder_book(book) {
            return None;
        }
        Some(Self {
            book: FileStamp::read(book)?,
            fonts: Self::read_fonts(fonts)?,
        })
    }

    pub(super) fn read_fonts(fonts: &[PathBuf]) -> Option<Vec<FileStamp>> {
        fonts.iter().map(|path| FileStamp::read(path)).collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FileStamp {
    len: u64,
    modified: SystemTime,
    #[cfg(unix)]
    identity: (u64, u64, i64, i64),
}

impl FileStamp {
    pub(crate) fn read(path: &Path) -> Option<Self> {
        Self::from_metadata(&fs::metadata(path).ok()?)
    }

    pub(crate) fn from_metadata(metadata: &fs::Metadata) -> Option<Self> {
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
pub(crate) struct ScreenPages(Rc<RefCell<Option<Generation>>>, Rc<Cell<Demand>>);

#[derive(Clone, Copy, Default)]
struct Demand {
    navigation: bool,
    markers: bool,
    typography: Typography,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Typography {
    #[default]
    None,
    Pending,
    Counted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Target {
    pub(super) chapter: usize,
    pub(super) page: usize,
    sample: PageSample,
}

impl ScreenPages {
    pub(super) fn request_navigation(&self) {
        let mut demand = self.1.get();
        demand.navigation = true;
        self.1.set(demand);
    }

    pub(super) fn request_markers(&self, required: bool) {
        let mut demand = self.1.get();
        demand.markers = required;
        self.1.set(demand);
    }

    fn requested(&self) -> bool {
        let demand = self.1.get();
        demand.navigation || demand.markers || demand.typography != Typography::None
    }

    pub(super) fn for_relayout(&self, typography: bool, eligible: bool) -> Self {
        let mut demand = self.1.get();
        demand.typography = if eligible && (self.typography_pending() || typography) {
            Typography::Pending
        } else {
            Typography::None
        };
        Self(Default::default(), Rc::new(Cell::new(demand)))
    }

    pub(crate) fn typography_pending(&self) -> bool {
        self.1.get().typography == Typography::Pending
    }

    pub(super) fn clear_typography(&self) {
        let mut demand = self.1.get();
        demand.typography = Typography::None;
        self.1.set(demand);
    }

    fn complete_typography(&self) {
        if self.typography_pending() {
            let mut demand = self.1.get();
            demand.typography = Typography::Counted;
            self.1.set(demand);
        }
    }

    pub(super) fn target(
        &self,
        config: &BookConfig,
        probe: &Probe,
        chapter: usize,
        page: usize,
        ordinal: usize,
    ) -> Option<Target> {
        self.position(config, probe, chapter, page)?;
        let zero = ordinal.checked_sub(1)?;
        let slot = self.0.borrow();
        let generation = slot.as_ref()?;
        let prefix = generation.prefix.as_ref()?;
        if zero >= *prefix.last()? {
            return None;
        }
        let chapter = prefix
            .partition_point(|&start| start <= zero)
            .checked_sub(1)?;
        Some(Target {
            chapter,
            page: zero - prefix[chapter],
            sample: generation.samples[chapter]?,
        })
    }

    pub(super) fn validates_target(
        &self,
        config: &BookConfig,
        probe: &Probe,
        target: Target,
    ) -> bool {
        let slot = self.0.borrow();
        slot.as_ref().is_some_and(|generation| {
            generation.matches(config, probe)
                && generation
                    .spine
                    .get(target.chapter)
                    .is_some_and(|entry| entry.href == probe.chapter)
                && target.sample == PageSample::new(probe)
                && target.page < probe.page_count()
        })
    }

    /// A provisional target must still be the same source and edition, but
    /// its page count can change when quote depth is prepared.
    pub(super) fn validates_provisional_target(
        &self,
        config: &BookConfig,
        probe: &Probe,
        target: Target,
    ) -> bool {
        let slot = self.0.borrow();
        slot.as_ref().is_some_and(|generation| {
            generation.matches(config, probe)
                && generation
                    .spine
                    .get(target.chapter)
                    .is_some_and(|entry| entry.href == probe.chapter)
                && target.sample.source_revision == probe.source.revision
                && probe.page_count() > 0
        })
    }

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
        let generation = slot.get_or_insert_with(|| Generation::new(config, probe, sources));
        generation.record(chapter, &probe.chapter, PageSample::new(probe));
        if !generation.rejected && generation.heading_pages[chapter].is_none() {
            generation.heading_pages[chapter] = Some(heading_pages(probe));
        }
        if generation.prefix.is_some() {
            self.complete_typography();
        }
    }

    pub(super) fn publisher_markers(
        &self,
        config: &BookConfig,
        probe: &Probe,
        chapter: usize,
        page: usize,
        allow_depth: impl Fn(usize) -> bool,
    ) -> Option<(Vec<usize>, usize)> {
        let (_, count) = self.position(config, probe, chapter, page)?;
        let slot = self.0.borrow();
        let generation = slot.as_ref()?;
        let prefix = generation.prefix.as_ref()?;
        let mut marks = Vec::new();
        for (chapter, local) in generation.heading_pages.iter().enumerate() {
            for &(heading, page) in local.as_ref()?.publisher.as_ref()? {
                if allow_depth(probe.toc.get(heading)?.depth) {
                    marks.push(prefix[chapter] + page);
                }
            }
        }
        marks.sort_unstable();
        marks.dedup();
        Some((marks, count))
    }

    /// Resolve publisher headings to both their measured page and spine item.
    /// Footer and Skim use source-size weighted tracks, so absolute screen
    /// page numbers alone cannot be used as tick fractions on those tracks.
    pub(super) fn publisher_locations(
        &self,
        config: &BookConfig,
        probe: &Probe,
        chapter: usize,
        page: usize,
        allow_depth: impl Fn(usize) -> bool,
    ) -> Option<AlternativeLocations> {
        let (current, count) = self.position(config, probe, chapter, page)?;
        let slot = self.0.borrow();
        let generation = slot.as_ref()?;
        let prefix = generation.prefix.as_ref()?;
        let mut starts = Vec::new();
        for (chapter, local) in generation.heading_pages.iter().enumerate() {
            let start = *prefix.get(chapter)?;
            let pages = prefix.get(chapter + 1)?.checked_sub(start)?;
            for &(index, page) in local.as_ref()?.publisher.as_ref()? {
                if page >= pages {
                    return None;
                }
                if allow_depth(probe.toc.get(index)?.depth) {
                    starts.push(AlternativeLocation {
                        index,
                        chapter,
                        page,
                        pages,
                        absolute: start + page,
                    });
                }
            }
        }
        starts.sort_by_key(|heading| heading.absolute);
        Some(AlternativeLocations {
            starts,
            current: current - 1,
            count,
        })
    }

    pub(super) fn alternative_locations(
        &self,
        config: &BookConfig,
        probe: &Probe,
        chapter: usize,
        page: usize,
        entries: &[OutlineEntry],
        allow_depth: impl Fn(usize) -> bool,
    ) -> Option<AlternativeLocations> {
        let (current, count) = self.position(config, probe, chapter, page)?;
        let slot = self.0.borrow();
        let generation = slot.as_ref()?;
        let prefix = generation.prefix.as_ref()?;
        let mut starts = Vec::new();
        for (index, entry) in entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| allow_depth(entry.depth))
        {
            let (chapter, local) = match &entry.target {
                OutlineTarget::Chapter(chapter) => (*chapter, 0),
                OutlineTarget::Heading(chapter, anchor) => {
                    let sample = generation.samples.get(*chapter)?.as_ref()?;
                    if anchor.book != generation.publication
                        || anchor.revision != sample.source_revision
                        || anchor.chapter != generation.spine.get(*chapter)?.href
                        || anchor.offset != 0
                    {
                        return None;
                    }
                    let headings = &generation
                        .heading_pages
                        .get(*chapter)?
                        .as_ref()?
                        .alternative;
                    let Ok(index) =
                        headings.binary_search_by_key(&anchor.block, |&(block, _)| block)
                    else {
                        // This source heading was suppressed in the measured layout.
                        continue;
                    };
                    (*chapter, headings[index].1)
                }
            };
            let start = *prefix.get(chapter)?;
            let pages = prefix.get(chapter + 1)?.checked_sub(start)?;
            if local >= pages {
                return None;
            }
            starts.push(AlternativeLocation {
                index,
                chapter,
                page: local,
                pages,
                absolute: start + local,
            });
        }
        // Keep source order within a rendered page for title selection.
        starts.sort_by_key(|heading| heading.absolute);
        Some(AlternativeLocations {
            starts,
            current: current - 1,
            count,
        })
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
    heading_pages: Vec<Option<HeadingPages>>,
    missing: usize,
    prefix: Option<Vec<usize>>,
    rejected: bool,
    background_rejected: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
            heading_pages: vec![None; missing],
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

pub(crate) struct AlternativeLocation {
    pub(crate) index: usize,
    pub(crate) chapter: usize,
    pub(crate) page: usize,
    pub(crate) pages: usize,
    pub(crate) absolute: usize,
}

pub(crate) struct AlternativeLocations {
    pub(crate) starts: Vec<AlternativeLocation>,
    pub(crate) current: usize,
    pub(crate) count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct HeadingPages {
    publisher: Option<Vec<(usize, usize)>>,
    alternative: Vec<(usize, usize)>,
}

// Keep outline indexing bounded independently of the displayed tick density.
pub(super) fn heading_pages(probe: &Probe) -> HeadingPages {
    const MAX_HEADINGS: usize = 16_384;
    let mut alternative: Vec<_> = probe
        .source
        .blocks
        .iter()
        .filter(|block| heading_depth(&probe.source.elements[block.element].tag).is_some())
        .take(MAX_CHAPTER_HEADINGS)
        .filter_map(|block| {
            let anchor = Anchor {
                raw_caret: None,
                book: probe.book,
                chapter: probe.chapter.clone(),
                revision: probe.source.revision,
                block: block.key,
                offset: 0,
                illustration: None,
            };
            Some((block.key, probe.page_for(&anchor)?))
        })
        .collect();
    alternative.sort_unstable_by_key(|&(block, _)| block);
    HeadingPages {
        publisher: (probe.toc.len() < MAX_HEADINGS).then(|| probe.local_contents_pages().to_vec()),
        alternative,
    }
}

pub(super) fn layout_settings(settings: Settings) -> Settings {
    let defaults = Settings::default();
    // These controls only repaint or change interaction state in Reading::reflow.
    Settings {
        font_gamma_index: defaults.font_gamma_index,
        hanging_punctuation: defaults.hanging_punctuation,
        text_contrast: defaults.text_contrast,
        page_overlap_enabled: defaults.page_overlap_enabled,
        page_overlap_lines: defaults.page_overlap_lines,
        page_overlap_style: defaults.page_overlap_style,
        invert_night_images: defaults.invert_night_images,
        smooth_images: defaults.smooth_images,
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
            viewport: crate::session::ReadingViewport::PORTRAIT,
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
            quote_prefixes: Default::default(),
            screen_pages: Default::default(),
            eager_layouts: Default::default(),
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
