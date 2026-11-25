//! Resolve generated quotation marks in publisher DOM order.
#[cfg(test)]
mod tests;

use super::{BookStyle, ContentPart, Element, Generated, Side, Style, expand};
use crate::{Result, Settings, epub::Book, user_styles};
use std::{
    collections::HashSet,
    sync::Arc,
    time::{Duration, Instant},
};

const MAX_CACHED_LANGUAGES: usize = 128;
// Normal, unjoined-note, and ordinary-flow retry profiles can coexist.
const MAX_CACHED_PROFILES: usize = 3;
const FOREGROUND_CHECKPOINTS: usize = 12;
const LEGACY_PREFIX_CHAPTERS: usize = 16;
const LEGACY_PREFIX_BYTES: usize = 512 * 1024;
const FOREGROUND_CHAPTERS: usize = 24;
const FOREGROUND_BYTES: usize = 768 * 1024;
const FOREGROUND_TIME: Duration = Duration::from_millis(120);

#[cfg(test)]
thread_local! {
    static HISTORICAL_ITEMS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn take_historical_items() -> usize {
    HISTORICAL_ITEMS.replace(0)
}

#[derive(Clone, Copy)]
pub(crate) enum Token {
    Open,
    Close,
    SilentOpen,
    SilentClose,
}

impl Token {
    pub(super) fn parse(word: &str) -> Option<Self> {
        if word.eq_ignore_ascii_case("open-quote") {
            Some(Self::Open)
        } else if word.eq_ignore_ascii_case("close-quote") {
            Some(Self::Close)
        } else if word.eq_ignore_ascii_case("no-open-quote") {
            Some(Self::SilentOpen)
        } else if word.eq_ignore_ascii_case("no-close-quote") {
            Some(Self::SilentClose)
        } else {
            None
        }
    }

    pub(super) fn output_bytes(self) -> usize {
        match self {
            Self::Open | Self::Close => 6,
            Self::SilentOpen | Self::SilentClose => 0,
        }
    }
}

#[derive(Clone, Default)]
pub(crate) struct Depths(HashSet<Box<str>>);

pub(crate) struct Checkpoints(Vec<(Profile, Vec<Checkpoint>)>, bool);

#[derive(PartialEq)]
struct Profile {
    settings: Settings,
    user: user_styles::Snapshot,
    shaping_language: Option<parley::Language>,
    main: Box<str>,
}

struct Checkpoint {
    chapter: usize,
    depths: Depths,
}

struct Incoming {
    depths: Depths,
    reusable: bool,
}

impl Checkpoints {
    pub(crate) fn snapshot() -> Self {
        Self(Vec::new(), false)
    }

    pub(crate) fn foreground() -> Self {
        Self(Vec::new(), true)
    }

    #[cfg(test)]
    pub(crate) fn latest(&self) -> Option<usize> {
        self.0
            .iter()
            .flat_map(|(_, checkpoints)| checkpoints)
            .map(|point| point.chapter)
            .max()
    }

    fn before(&self, profile: &Profile, chapter: usize) -> (usize, Depths) {
        self.0
            .iter()
            .find(|(key, _)| key == profile)
            .and_then(|(_, checkpoints)| {
                checkpoints
                    .iter()
                    .filter(|point| point.chapter <= chapter)
                    .max_by_key(|point| point.chapter)
            })
            .map_or_else(Default::default, |point| {
                (point.chapter, point.depths.clone())
            })
    }

    fn record(&mut self, profile: Profile, start: usize, chapter: usize, depths: &Depths) {
        if start == chapter {
            return;
        }
        let mut checkpoints = self
            .0
            .iter()
            .position(|(key, _)| key == &profile)
            .map(|index| self.0.remove(index).1)
            .unwrap_or_default();
        let point = Checkpoint {
            chapter,
            depths: depths.clone(),
        };
        if self.1 {
            // Keep recent exact boundaries for back and forward turns.
            checkpoints.retain(|point| point.chapter != chapter);
            if checkpoints.len() == FOREGROUND_CHECKPOINTS {
                checkpoints.remove(0);
            }
            checkpoints.push(point);
        } else if let Some(prior) = checkpoints.iter_mut().find(|point| point.chapter == start) {
            *prior = point;
        } else {
            // Preserve the forward checkpoint when indexing checked a later
            // visible chapter first and then restarted from the beginning.
            if checkpoints.len() == 2 {
                let earliest = usize::from(checkpoints[0].chapter > checkpoints[1].chapter);
                checkpoints.swap_remove(earliest);
            }
            checkpoints.push(point);
        }
        if self.0.len() == MAX_CACHED_PROFILES {
            self.0.remove(0);
        }
        self.0.push((profile, checkpoints));
    }
}

pub(crate) fn render(
    book: &mut Book,
    chapter: Option<usize>,
    settings: Settings,
    user: &user_styles::Snapshot,
    shaping_language: Option<parley::Language>,
    main: &str,
    elements: &[Element],
    styles: &mut [Style],
    plan: Plan<'_>,
    check_cancelled: &dyn Fn() -> Result<()>,
) -> Result<()> {
    let incoming = if let Some(chapter) = chapter {
        if plan.is_empty() {
            book.quote_prefixes
                .as_ref()
                .filter(|cache| cache.1)
                .and_then(|cache| {
                    let profile = Profile {
                        settings,
                        user: user.clone(),
                        shaping_language,
                        main: main.into(),
                    };
                    let (start, depths) = cache.before(&profile, chapter);
                    (start == chapter).then_some(Incoming {
                        depths,
                        reusable: true,
                    })
                })
        } else {
            prefix(
                book,
                chapter,
                settings,
                user,
                shaping_language,
                main,
                elements,
                check_cancelled,
            )?
        }
    } else {
        None
    };
    let reusable = incoming.as_ref().is_some_and(|depth| depth.reusable);
    let mut depths = incoming.map_or_else(Depths::default, |incoming| incoming.depths);
    plan.resolve(
        elements,
        styles,
        main,
        settings.typography.respect_embedded,
        &mut depths,
    );
    if reusable
        && depths.0.len() <= MAX_CACHED_LANGUAGES
        && let Some(chapter) = chapter
        && let Some(cache) = book.quote_prefixes.as_mut().filter(|cache| cache.1)
    {
        let profile = Profile {
            settings,
            user: user.clone(),
            shaping_language,
            main: main.into(),
        };
        cache.record(profile, chapter, chapter + 1, &depths);
    }
    Ok(())
}

fn prefix(
    book: &mut Book,
    chapter: usize,
    settings: Settings,
    user: &user_styles::Snapshot,
    shaping_language: Option<parley::Language>,
    main: &str,
    current: &[Element],
    check_cancelled: &dyn Fn() -> Result<()>,
) -> Result<Option<Incoming>> {
    let since = Instant::now();
    scan_prefix(
        book,
        chapter,
        settings,
        user,
        shaping_language,
        main,
        current,
        check_cancelled,
        &|| since.elapsed() >= FOREGROUND_TIME,
    )
}

fn scan_prefix(
    book: &mut Book,
    chapter: usize,
    settings: Settings,
    user: &user_styles::Snapshot,
    shaping_language: Option<parley::Language>,
    main: &str,
    current: &[Element],
    check_cancelled: &dyn Fn() -> Result<()>,
    deadline_reached: &dyn Fn() -> bool,
) -> Result<Option<Incoming>> {
    if chapter == 0 || !book.is_reflowable_epub() {
        return Ok(Some(Incoming {
            depths: Depths::default(),
            reusable: true,
        }));
    }
    let profile = Profile {
        settings,
        user: user.clone(),
        shaping_language,
        main: main.into(),
    };
    let (start, mut depths) = book
        .quote_prefixes
        .as_ref()
        .map_or_else(Default::default, |cache| cache.before(&profile, chapter));
    let foreground = book.quote_prefixes.as_ref().is_none_or(|cache| cache.1);
    let within_legacy_chapters = chapter <= LEGACY_PREFIX_CHAPTERS;
    let mut bytes = 0;
    let mut end = start;
    let mut wanted = None;
    for index in start..chapter {
        check_cancelled()?;
        // The earlier foreground path supported this finite prefix even on
        // slow hardware. Apply the deadline only outside that window.
        if foreground
            && (index - start == FOREGROUND_CHAPTERS
                || ((!within_legacy_chapters || bytes > LEGACY_PREFIX_BYTES) && deadline_reached()))
        {
            break;
        }
        let Ok(xhtml) = book.chapter(index) else {
            check_cancelled()?;
            return Ok(None);
        };
        bytes += xhtml.len();
        if foreground
            && (bytes > FOREGROUND_BYTES
                || (within_legacy_chapters && bytes > LEGACY_PREFIX_BYTES && deadline_reached()))
        {
            break;
        }
        #[cfg(test)]
        HISTORICAL_ITEMS.set(HISTORICAL_ITEMS.get() + 1);
        if advance_chapter(book, index, &xhtml, &profile, wanted.as_ref(), &mut depths).is_err() {
            check_cancelled()?;
            return Ok(None);
        }
        end = index + 1;
        if wanted.is_none() && depths.0.len() > MAX_CACHED_LANGUAGES {
            // Depth is independent per language. Limit the working set to the
            // current chapter's language keys if the publication has more.
            let mut keys = HashSet::from([main]);
            if settings.typography.respect_embedded {
                keys.extend(
                    current
                        .iter()
                        .filter_map(|element| element.quote_language.as_deref()),
                );
            }
            depths.0.retain(|language| keys.contains(language.as_ref()));
            wanted = Some(keys);
        }
    }
    check_cancelled()?;
    if wanted.is_none()
        && let Some(cache) = book.quote_prefixes.as_mut()
    {
        cache.record(profile, start, end, &depths);
    }
    Ok((end == chapter).then_some(Incoming {
        depths,
        reusable: wanted.is_none(),
    }))
}

fn advance_chapter(
    book: &mut Book,
    index: usize,
    xhtml: &str,
    profile: &Profile,
    wanted: Option<&HashSet<&str>>,
    depths: &mut Depths,
) -> Result<()> {
    let href = book
        .chapters
        .get(index)
        .ok_or("That EPUB chapter is unavailable")?
        .href
        .clone();
    let settings = profile.settings;
    let source = book.chapter_source_with_language(
        &href,
        xhtml,
        profile.shaping_language,
        settings.typography.respect_embedded,
    )?;
    let author = BookStyle::load_for_flow_with_base(
        book,
        &href,
        &source,
        settings.publisher_styles,
        &profile.user,
        settings.base_stylesheet,
    )?;
    let targets = if book.spine_rtl {
        None
    } else {
        crate::footnotes::in_page::Targets::epub_for_settings(
            &href,
            xhtml,
            &source,
            settings,
            &profile.user,
        )
    };
    let (_, quotes) = author.compute_deferred_quotes(&source.elements, settings, targets.as_ref());
    quotes.visit(
        &source.elements,
        &profile.main,
        settings.typography.respect_embedded,
        |part, language| {
            if wanted.is_none_or(|keys| keys.contains(language)) {
                for part in part.parts {
                    let ContentPart::Quote(_) = part else { break };
                    depths.advance(language);
                }
            }
        },
    );
    Ok(())
}

impl Depths {
    // Each token toggles the pair. Signed counts and unmatched closes have
    // the same alternation, so even-depth languages need no entry.
    fn advance(&mut self, language: &str) -> bool {
        let was_odd = self.0.remove(language);
        if !was_odd {
            self.0.insert(language.into());
        }
        was_odd
    }

    fn append(&mut self, language: &str, token: Token, output: &mut String) {
        let opening = matches!(token, Token::Open | Token::SilentOpen);
        let pair = usize::from(self.advance(language) == opening);
        if matches!(token, Token::SilentOpen | Token::SilentClose) {
            return;
        }
        let (left, right) = pairs(language)[pair];
        if opening {
            output.push(left);
            output.push('\u{2060}');
        } else {
            output.push('\u{2060}');
            output.push(right);
        }
    }
}

fn pairs(language: &str) -> [(char, char); 2] {
    let language = language.as_bytes();
    let starts = |prefix: &[u8]| {
        language
            .get(..prefix.len())
            .is_some_and(|start| start.eq_ignore_ascii_case(prefix))
            && language.get(prefix.len()).is_none_or(|&next| next == b'-')
    };
    if starts(b"fr-ch") {
        [('«', '»'), ('‹', '›')]
    } else if starts(b"fr") {
        [('«', '»'), ('“', '”')]
    } else if starts(b"de") {
        [('„', '“'), ('‚', '‘')]
    } else {
        [('“', '”'), ('‘', '’')]
    }
}

pub(super) struct Pending<'a> {
    pub(super) host: usize,
    pub(super) side: Side,
    pub(super) parts: &'a [ContentPart],
    pub(super) style: Option<Style>,
}

#[derive(Default)]
pub(crate) struct Plan<'a>(pub(super) Vec<Pending<'a>>);

impl Plan<'_> {
    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(crate) fn resolve(
        self,
        elements: &[Element],
        styles: &mut [Style],
        main: &str,
        respect_embedded: bool,
        depths: &mut Depths,
    ) {
        self.visit(elements, main, respect_embedded, |part, language| {
            part.resolve(elements, styles, language, depths);
        });
    }

    fn visit(
        self,
        elements: &[Element],
        main: &str,
        respect_embedded: bool,
        mut apply: impl FnMut(Pending<'_>, &str),
    ) {
        if self.is_empty() {
            return;
        }
        let mut pending = self.0.into_iter().peekable();
        let mut ancestors: Vec<(usize, &str, Option<Pending<'_>>)> = Vec::new();
        for (index, element) in elements.iter().enumerate() {
            while ancestors
                .last()
                .is_some_and(|entry| Some(entry.0) != element.parent)
            {
                let (_, language, after) = ancestors.pop().unwrap();
                if let Some(after) = after {
                    apply(after, language);
                }
            }
            let inherited = ancestors.last().map_or(main, |entry| entry.1);
            let language = if respect_embedded {
                element.quote_language.as_deref().unwrap_or(inherited)
            } else {
                main
            };
            let mut after = None;
            while pending.peek().is_some_and(|part| part.host == index) {
                let part = pending.next().unwrap();
                match part.side {
                    Side::Before => apply(part, language),
                    Side::After => after = Some(part),
                }
            }
            ancestors.push((index, language, after));
        }
        for (_, language, after) in ancestors.into_iter().rev() {
            if let Some(after) = after {
                apply(after, language);
            }
        }
    }
}

impl Pending<'_> {
    fn resolve(
        self,
        elements: &[Element],
        styles: &mut [Style],
        language: &str,
        depths: &mut Depths,
    ) {
        let text = expand(self.parts, &elements[self.host], |token, output| {
            depths.append(language, token, output);
        });
        if let (Some(text), Some(style)) = (text, self.style) {
            let pair = styles[self.host]
                .generated
                .get_or_insert_with(|| Arc::new([None, None]));
            Arc::make_mut(pair)[self.side as usize] = Some(Generated { text, style });
        }
    }
}
