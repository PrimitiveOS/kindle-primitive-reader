//! Source-bound layout verification for a highlight in another ZIP EPUB.
use super::{Reader, bulk_marks::Snapshot};
use crate::session::screen_pages::FileStamp;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

const MAX_FONTS: usize = 16;
const MAX_FONT_BYTES: u64 = 64 * 1024 * 1024;
const MAX_QUOTE_CHAPTERS: usize = 8;
const MAX_QUOTE_TIME: std::time::Duration = std::time::Duration::from_secs(10);

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Context {
    preferences: crate::library::BookPreferences,
    settings: crate::Settings,
    styles: crate::user_styles::Snapshot,
    rules: crate::user_hyphenation::Rules,
    fonts: Vec<PathBuf>,
}

impl Context {
    pub(super) fn capture(reader: &Reader, id: u64) -> crate::Result<Self> {
        let state = reader.library.state(id).ok_or("That book is unavailable")?;
        if reader.profiles.iter().any(|profile| {
            matches!(profile, crate::reading_profiles::Profile::Text { .. })
                && (profile.applies_on_open() || profile.opening_conditions().is_some())
        }) {
            return Err(
                "Open this book to apply its reading profiles before editing its note".into(),
            );
        }
        if state.preferences.facing_pages
            || state.hidden_flows.enabled
            || state
                .hide_nonlinear
                .unwrap_or(reader.controls.hide_nonlinear_default)
        {
            return Err("Open this book to edit notes with its reading layout".into());
        }
        let mut settings = state.preferences.apply_to(reader.device_reading_settings());
        settings.status_header_height = crate::reading_controls::TopHeader::height(
            reader.top_header_for_book(id),
            state.preferences.scroll,
        );
        let single_page_layout = !state.preferences.scroll;
        settings.in_page_footnotes &= single_page_layout;
        if settings.publisher_render_mode.is_some() && !single_page_layout {
            settings = crate::PublisherRenderMode::Reader.apply(settings);
        }
        if settings.in_page_footnotes {
            return Err("Open this book to edit notes with in-page notes enabled".into());
        }
        settings.validate()?;
        let styles = reader
            .style_store
            .data
            .snapshot(id)?
            .for_note_layout(single_page_layout)
            .for_nonlinear_notes(false);
        if styles.in_page_epub_footnotes() {
            return Err("Open this book to edit notes with in-page notes enabled".into());
        }
        let mut fonts = reader.fonts.clone();
        if let Some(file) = state.preferences.font_file.as_deref() {
            let path = crate::reading_fonts::resolve(&reader.fonts_directory, file)
                .map_err(|_| "The target book's reading font is unavailable")?;
            fonts.insert(0, path);
        }
        if fonts.is_empty() || fonts.len() > MAX_FONTS {
            return Err("The target book's reading fonts exceed the verification limit".into());
        }
        Ok(Self {
            preferences: state.preferences.clone(),
            settings,
            styles,
            rules: reader.user_hyphenation.as_ref().clone(),
            fonts,
        })
    }

    fn font_stamps(&self) -> Option<Vec<FileStamp>> {
        primitive_app::with_storage(|| {
            let mut stamps = Vec::with_capacity(self.fonts.len());
            let mut bytes = 0_u64;
            for path in &self.fonts {
                let metadata = std::fs::metadata(path)?;
                bytes = bytes
                    .checked_add(metadata.len())
                    .ok_or_else(|| std::io::Error::other("Reading fonts exceed the limit"))?;
                if bytes > MAX_FONT_BYTES {
                    return Err(std::io::Error::other("Reading fonts exceed the limit"));
                }
                stamps
                    .push(FileStamp::from_metadata(&metadata).ok_or_else(|| {
                        std::io::Error::other("Reading fonts cannot be verified")
                    })?);
            }
            Ok(stamps)
        })
        .ok()
    }

    pub(super) fn revalidate_fonts(&self, stamps: &[FileStamp]) -> bool {
        self.font_stamps().as_deref() == Some(stamps)
    }

    pub(super) fn verify(
        &self,
        bytes: std::sync::Arc<[u8]>,
        target: &Snapshot,
        cancel: &AtomicBool,
    ) -> crate::Result<Vec<FileStamp>> {
        let check = || -> crate::Result<()> {
            if cancel.load(Ordering::Relaxed) {
                Err("Verification was cancelled".into())
            } else {
                Ok(())
            }
        };
        check()?;
        let fonts = self
            .font_stamps()
            .ok_or("Reading fonts cannot be verified")?;
        let (start, selection) = match target {
            Snapshot::Bookmark(_, mark) => (
                mark.position
                    .anchor
                    .as_ref()
                    .ok_or("Open this book to resolve this bookmark")?,
                None,
            ),
            Snapshot::Reflow(_, mark) => (&mark.selection.start, Some(&mark.selection)),
            Snapshot::Fixed(..) => return Err("This mark is unavailable".into()),
        };
        if start.raw_caret.is_some() || start.illustration.is_some() {
            return Err("Open this book to edit this saved location".into());
        }
        if let Some(selection) = selection {
            let end = &selection.end;
            if end.raw_caret.is_some()
                || end.illustration.is_some()
                || start.book != end.book
                || selection.text.len() > crate::library::MAX_HIGHLIGHT_TEXT_BYTES
            {
                return Err("Open this book to edit this saved passage".into());
            }
        }
        let mut book = crate::epub::Book::open_snapshot(bytes)?;
        if book.is_fixed_layout() || !book.is_reflowable_epub() || book.revision() != start.book {
            return Err("The EPUB no longer matches this passage".into());
        }
        let unique_index = |href: &str| -> crate::Result<usize> {
            let mut matches = book
                .chapters
                .iter()
                .enumerate()
                .filter(|(_, entry)| entry.href == href);
            let (index, entry) = matches.next().ok_or("That chapter is unavailable")?;
            if book.recovered_placeholder(entry) || matches.next().is_some() {
                return Err("That chapter is unavailable".into());
            }
            Ok(index)
        };
        let first = unique_index(&start.chapter)?;
        let last = match selection {
            Some(selection) => unique_index(&selection.end.chapter)?,
            None => first,
        };
        let count = last
            .checked_sub(first)
            .and_then(|n| n.checked_add(1))
            .filter(|n| *n <= MAX_QUOTE_CHAPTERS)
            .ok_or("Open this book to edit this saved passage")?;
        for entry in &book.chapters[first..=last] {
            if unique_index(&entry.href).is_err() {
                return Err("That chapter is unavailable".into());
            }
        }
        let settings = if self.settings.publisher_render_mode.is_some()
            && (!self.settings.publisher_styles || !book.is_reflowable_epub() || book.spine_rtl)
        {
            crate::PublisherRenderMode::Reader.apply(self.settings)
        } else {
            self.settings
        };
        let shaping_main = settings
            .typography
            .resolve(book.language.map(crate::TypographyLocale::publication));
        let shaping_language = shaping_main.map(crate::TypographyLocale::shaping_language);
        let started = std::time::Instant::now();
        let check_work = || -> crate::Result<()> {
            check()?;
            if count > 1 && started.elapsed() >= MAX_QUOTE_TIME {
                return Err("Open this book to edit this saved passage".into());
            }
            Ok(())
        };
        let mut opened = Some(book);
        let mut text = String::new();
        for index in first..=last {
            check_work()?;
            let book = opened.as_mut().ok_or("The EPUB snapshot is unavailable")?;
            let prefix = crate::book_style::pseudo::quotes::prepare_exact(
                book,
                index,
                settings,
                &self.styles,
                shaping_language,
                shaping_main.as_ref().map_or("en", |tag| tag.as_str()),
                &check_work,
            )?;
            book.quote_prefixes
                .as_mut()
                .ok_or("The EPUB snapshot is unavailable")?
                .install_exact(prefix);
            let probe = crate::Probe::layout_opened_book_cancellable(
                &mut opened,
                crate::Engine::Parley,
                settings,
                index,
                None,
                &self.rules,
                &self.styles,
                || {
                    primitive_app::with_storage(|| {
                        crate::font_context(&self.fonts).map_err(std::io::Error::other)
                    })
                    .map_err(Into::into)
                },
                false,
                &check_work,
            )?;
            if index == first && probe.page_for(start).is_none() {
                return Err("This saved location no longer matches the EPUB".into());
            }
            if let Some(selection) = selection {
                let end = &selection.end;
                if index == last && probe.page_for(end).is_none() {
                    return Err("This saved passage no longer matches the EPUB".into());
                }
                if first == last
                    && probe.compare_text_anchors(start, end) != Some(std::cmp::Ordering::Less)
                {
                    return Err("This saved passage no longer matches the EPUB".into());
                }
                match probe.select_source_range(start, end) {
                    Some(part) => {
                        if (first == last && part != *selection)
                            || (index == first && part.start != *start)
                            || (index == last && part.end != *end)
                        {
                            return Err("This saved passage no longer matches the EPUB".into());
                        }
                        let separator = usize::from(!text.is_empty());
                        if text
                            .len()
                            .saturating_add(separator)
                            .saturating_add(part.text.len())
                            > crate::library::MAX_HIGHLIGHT_TEXT_BYTES
                        {
                            return Err("This saved passage exceeds the verification limit".into());
                        }
                        if separator != 0 {
                            text.push('\n');
                        }
                        text.push_str(&part.text);
                    }
                    None if index == first || index == last || probe.has_raw_layout() => {
                        return Err("This saved passage no longer matches the EPUB".into());
                    }
                    None => {}
                }
            }
        }
        if selection.is_some_and(|selection| text.trim().is_empty() || text != selection.text) {
            return Err("This saved passage no longer matches the EPUB".into());
        }
        check_work()?;
        if !self.revalidate_fonts(&fonts) {
            return Err("Reading fonts changed during verification".into());
        }
        check_work()?;
        Ok(fonts)
    }
}
