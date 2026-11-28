//! Apply link tap and location swipe policies on the reading surface.
//! Explicit link rows and footnote previews use their own direct actions.
use super::*;
use crate::fixed::PdfLink;
use crate::selection::HighlightRect;
use std::collections::BTreeMap;

// KOReader's scaleByDPI(30) rounds up at 300 DPI using its 160 DPI base.
const LINK_TAP_RADIUS: f32 = 57.0;

#[derive(Clone, Copy)]
pub(super) enum LinkPreference {
    Follow,
    LargerArea,
    IgnoreExternal,
    SwipeBack,
    SwipeFollow,
    SwipeIgnoreExternal,
    SwipeLatestBookmark,
    FootnoteBookFont,
    FootnoteJustify,
}

impl LinkPreference {
    pub(super) fn checked(self, controls: ReadingControls) -> bool {
        match self {
            Self::Follow => controls.tap_links.unwrap_or(true),
            Self::LargerArea => controls.larger_tap_area_for_links,
            Self::IgnoreExternal => controls.ignore_external_link_taps,
            Self::SwipeBack => controls.swipe_to_go_back,
            Self::SwipeFollow => controls.swipe_to_follow_nearest_link,
            Self::SwipeIgnoreExternal => controls.swipe_ignore_external_links.unwrap_or(true),
            Self::SwipeLatestBookmark => controls.swipe_to_latest_bookmark,
            Self::FootnoteBookFont => controls.footnote_book_font,
            Self::FootnoteJustify => controls.footnote_justify.unwrap_or(true),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Follow => "Tap to follow links",
            Self::LargerArea => "Allow larger tap area around links",
            Self::IgnoreExternal => "Ignore external links on tap",
            Self::SwipeBack => "Swipe to go back",
            Self::SwipeFollow => "Swipe to follow nearest link",
            Self::SwipeIgnoreExternal => "Ignore external links on swipe",
            Self::SwipeLatestBookmark => "Swipe to jump to latest bookmark",
            Self::FootnoteBookFont => "Use book font in popups",
            Self::FootnoteJustify => "Justify text in popups",
        }
    }

    fn unavailable_reason(self, controls: ReadingControls) -> Option<&'static str> {
        match self {
            Self::LargerArea | Self::IgnoreExternal if !Self::Follow.checked(controls) => {
                Some("Available when tap to follow links is on")
            }
            Self::SwipeIgnoreExternal if !Self::SwipeFollow.checked(controls) => {
                Some("Available when swipe to follow nearest link is on")
            }
            Self::FootnoteBookFont | Self::FootnoteJustify if !controls.footnote_popups => {
                Some("Available when footnote previews are on")
            }
            Self::FootnoteBookFont | Self::FootnoteJustify
                if !Self::Follow.checked(controls) && !Self::SwipeFollow.checked(controls) =>
            {
                Some("Available when tap or swipe to follow links is on")
            }
            _ => None,
        }
    }
}

#[derive(PartialEq, Eq)]
pub(super) enum ReflowLink {
    Internal(Destination),
    Address(String),
    /// A readable file on the device, offered for reading (KOReader's `openFileFromLink`).
    LocalDocument(LocalTarget),
}

impl ReflowLink {
    /// `folder` is the folder of the open book; relative links to other files resolve there.
    fn resolve(chapter: &str, href: &str, folder: &Path) -> Option<Self> {
        if let Some(path) = local_document(href, folder) {
            return Some(Self::LocalDocument(path));
        }
        if let Ok(target) = Book::resolve_link(chapter, href) {
            return Some(Self::Internal(target));
        }
        let address = url::Url::parse(href).ok()?;
        (matches!(address.scheme(), "http" | "https")
            && address.has_host()
            && !href.chars().any(char::is_control)
            && href.len() <= 4096)
            .then(|| Self::Address(href.to_owned()))
    }
}

/// The folder of the open book, where relative links to other files resolve.
fn book_folder(reading: &Reading) -> &Path {
    reading.config.book.parent().unwrap_or(Path::new("/"))
}

/// A document file on the device, and the page a `?pos=` query asked for (1-based). Reader
/// positions are not KOReader xpointers, so only a numeric page is applied, and only to fixed pages.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct LocalTarget {
    pub(super) path: PathBuf,
    pub(super) page: Option<usize>,
}

/// A link to an existing document file: a `file:` URL, an absolute path, or a path relative to
/// the book's folder. The fragment is ignored. Only book formats qualify, so an ordinary chapter
/// link such as `chapter.xhtml` stays internal even if a file of that name exists.
fn local_document(href: &str, folder: &Path) -> Option<LocalTarget> {
    if href.chars().any(char::is_control) || href.len() > 4096 {
        return None;
    }
    let target = match url::Url::parse(href) {
        Ok(url) if url.scheme() == "file" => url.to_file_path().ok()?,
        Ok(_) => return None,
        Err(_) => {
            let path = percent_decode(href.split(['#', '?']).next()?)?;
            if path.starts_with('/') {
                PathBuf::from(path)
            } else {
                folder.join(path)
            }
        }
    };
    if !is_book_format(&target) {
        return None;
    }
    let path = std::fs::canonicalize(target).ok()?;
    path.is_file().then_some(LocalTarget {
        path,
        page: pos_page(href),
    })
}

/// The page in a `pos=` query, when it is a number from 1 up.
fn pos_page(href: &str) -> Option<usize> {
    let query = href.split_once('?')?.1.split('#').next()?;
    query
        .split('&')
        .find_map(|pair| pair.strip_prefix("pos="))
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|page| *page >= 1)
}

fn is_book_format(path: &Path) -> bool {
    use crate::library::DocumentFormat as Format;
    matches!(
        Format::for_path(path),
        Some(
            Format::Epub
                | Format::Text
                | Format::Markdown
                | Format::Fb2
                | Format::Fb3
                | Format::Mobi
                | Format::Chm
                | Format::Rtf
                | Format::Tcr
                | Format::Pdf
                | Format::Djvu
                | Format::Cbz
                | Format::Cbt
                | Format::Cbr
        )
    )
}

fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let digits = std::str::from_utf8(bytes.get(index + 1..index + 3)?).ok()?;
            out.push(u8::from_str_radix(digits, 16).ok()?);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).ok()
}

impl Reader {
    pub(super) fn link_settings_items(&self) -> Vec<Item> {
        let language = self.controls.interface_language;
        let mut items: Vec<_> = [
            LinkPreference::Follow,
            LinkPreference::LargerArea,
            LinkPreference::IgnoreExternal,
            LinkPreference::SwipeBack,
            LinkPreference::SwipeFollow,
            LinkPreference::SwipeIgnoreExternal,
            LinkPreference::SwipeLatestBookmark,
            LinkPreference::FootnoteBookFont,
        ]
        .into_iter()
        .filter(|setting| {
            !matches!(
                setting,
                LinkPreference::LargerArea | LinkPreference::FootnoteBookFont
            ) || self.active.is_some()
        })
        .map(|setting| {
            if let Some(reason) = setting.unavailable_reason(self.controls) {
                return Item::new(
                    language.link_settings_text(setting.label()),
                    Action::Heading,
                )
                .detail(language.link_settings_text(reason));
            }
            let item = Item::new(
                language.link_settings_text(setting.label()),
                Action::ToggleLinkPreference(setting),
            );
            match setting {
                LinkPreference::SwipeBack => item.detail(
                    language.link_settings_text("Swipe right to return to your previous location"),
                ),
                LinkPreference::SwipeFollow => item.detail(
                    language.link_settings_text("Swipe left from near a link to follow it"),
                ),
                LinkPreference::SwipeLatestBookmark => item.detail(
                    language.link_settings_text("Swipe left when there is no link to follow"),
                ),
                _ => item,
            }
        })
        .collect();
        if self.active.is_some() {
            let size = self.controls.footnote_font_size;
            let item = if let Some(reason) =
                LinkPreference::FootnoteBookFont.unavailable_reason(self.controls)
            {
                Item::new(
                    language.link_settings_text("Footnote popup font size"),
                    Action::Heading,
                )
                .detail(language.link_settings_text(reason))
            } else {
                Item::new(
                    language.link_settings_text("Footnote popup font size"),
                    Action::Show(Screen::FootnoteFontSize(footnote_font::Mode::current(size))),
                )
                .detail(size.description())
            };
            items.push(item);
            let justify = LinkPreference::FootnoteJustify;
            items.push(match justify.unavailable_reason(self.controls) {
                Some(reason) => Item::new(
                    language.link_settings_text(justify.label()),
                    Action::Heading,
                )
                .detail(language.link_settings_text(reason)),
                None => Item::new(
                    language.link_settings_text(justify.label()),
                    Action::ToggleLinkPreference(justify),
                ),
            });
        }
        items
    }

    pub(super) fn toggle_link_preference(&mut self, setting: LinkPreference) -> Result<()> {
        if setting.unavailable_reason(self.controls).is_some() {
            return Ok(());
        }
        let checked = setting.checked(self.controls);
        let mut controls = self.controls;
        match setting {
            LinkPreference::Follow => controls.tap_links = Some(!checked),
            LinkPreference::LargerArea => controls.larger_tap_area_for_links = !checked,
            LinkPreference::IgnoreExternal => controls.ignore_external_link_taps = !checked,
            LinkPreference::SwipeBack => controls.swipe_to_go_back = !checked,
            LinkPreference::SwipeFollow => controls.swipe_to_follow_nearest_link = !checked,
            LinkPreference::SwipeIgnoreExternal => {
                controls.swipe_ignore_external_links = Some(!checked)
            }
            LinkPreference::SwipeLatestBookmark => controls.swipe_to_latest_bookmark = !checked,
            LinkPreference::FootnoteBookFont => controls.footnote_book_font = !checked,
            LinkPreference::FootnoteJustify => controls.footnote_justify = checked.then_some(false),
        }
        self.activate(Action::SetReadingControls(controls))
    }

    fn leftward_link_swipe(&self, input: Input, start_x: f32) -> Option<f32> {
        let Input::Swipe { dx, dy, start_y } = input else {
            return None;
        };
        if self.touch_locked
            || self.selection.is_some()
            || self.pdf_selection.is_some()
            || self.selection_mode
            || self.touch_hold.is_some()
            || !dx.is_finite()
            || !dy.is_finite()
            || -dx <= dy.abs() * 1.5
            || !(0.0..WIDTH as f32).contains(&start_x)
            || !(0.0..HEIGHT as f32).contains(&start_y)
        {
            return None;
        }
        Some(start_y)
    }

    pub(super) fn swipe_nearest_link(&mut self, input: Input, start_x: f32) -> Option<Change> {
        if !self.controls.swipe_to_follow_nearest_link {
            return None;
        }
        let start_y = self.leftward_link_swipe(input, start_x)?;
        self.follow_nearest_link_at(start_x, start_y)
    }

    pub(super) fn follow_nearest_link_at(&mut self, start_x: f32, start_y: f32) -> Option<Change> {
        let internal_only = LinkPreference::SwipeIgnoreExternal.checked(self.controls);
        match self.screen {
            Screen::Book => {
                let reading = &self.active.as_ref()?.reading;
                let (link, x, y) =
                    nearest_reflow_link(reading, start_x, start_y, f32::INFINITY, internal_only)?;
                self.auto_turn.activity(Instant::now());
                Some(self.open_reflow_link(link, x, y))
            }
            Screen::Fixed => {
                let reading = &self.fixed.as_ref()?.reading;
                let link = reading.nearest_link(start_x, start_y, internal_only)?;
                self.auto_turn.activity(Instant::now());
                Some(match self.open_fixed_link(link) {
                    Ok(()) => self.applied_change(),
                    Err(error) => self.report(error),
                })
            }
            _ => None,
        }
    }

    pub(super) fn swipe_latest_bookmark(&mut self, input: Input, start_x: f32) -> Option<Change> {
        if !self.controls.swipe_to_latest_bookmark
            || !matches!(self.screen, Screen::Book | Screen::Fixed)
        {
            return None;
        }
        self.leftward_link_swipe(input, start_x)?;
        self.jump_to_latest_bookmark()
    }

    /// Select the newest saved mark without altering its source data.
    pub(super) fn latest_bookmark_action(&self) -> Option<Action> {
        let state = self.library.state(self.open_book_id()?)?;
        // This selector is also used while the reading menu is open.
        let fixed = self.fixed.is_some();
        let (_, action) = state
            .bookmarks
            .iter()
            .enumerate()
            .map(|(index, mark)| (mark.created_at, Action::Bookmark(index)))
            .chain(
                state
                    .highlights
                    .iter()
                    .enumerate()
                    .filter(|_| !fixed)
                    .map(|(index, mark)| (mark.created_at, Action::Highlight(index))),
            )
            .chain(
                state
                    .pdf_highlights
                    .iter()
                    .filter(|_| fixed)
                    .map(|mark| (mark.created_at, Action::FixedPage(mark.page))),
            )
            .max_by_key(|(created, _)| *created)?;
        Some(action)
    }

    pub(super) fn jump_to_latest_bookmark(&mut self) -> Option<Change> {
        self.latest_bookmark_action()?;
        self.auto_turn.activity(Instant::now());
        Some(match self.start_latest_mark_search() {
            Ok(()) => Change::Partial,
            Err(error) => self.report(error),
        })
    }

    pub(super) fn open_reflow_link(&mut self, link: ReflowLink, x: f32, y: f32) -> Change {
        match link {
            ReflowLink::Address(address) => self.show_link_address(address),
            ReflowLink::LocalDocument(target) => self.show_local_document(target),
            ReflowLink::Internal(target) => {
                if self.open_note_at(target.clone(), x, y) {
                    return Change::Partial;
                }
                match self.follow(target) {
                    Ok(()) => Change::Partial,
                    Err(error) => self.report(error),
                }
            }
        }
    }

    pub(super) fn swipe_back_input(&mut self, input: Input) -> Option<Change> {
        let Input::Swipe { dx, dy, .. } = input else {
            return None;
        };
        if !self.controls.swipe_to_go_back
            || !matches!(self.screen, Screen::Book | Screen::Fixed)
            || self.selection.is_some()
            || self.pdf_selection.is_some()
            || self.selection_mode
            || !dx.is_finite()
            || !dy.is_finite()
            || dx <= dy.abs() * 1.5
        {
            return None;
        }
        if let Some(index) = self.back.len().checked_sub(1) {
            return Some(match self.return_to_location(index) {
                Ok(()) => {
                    self.swipe_back_notice_pending = self.back.is_empty();
                    self.applied_change()
                }
                Err(error) => self.report(error),
            });
        }
        // Let the reader see that the next swipe will turn the page.
        std::mem::take(&mut self.swipe_back_notice_pending)
            .then(|| self.report("Location history is empty."))
    }

    pub(super) fn reflow_tap_link(&self, x: f32, y: f32) -> Option<(ReflowLink, f32, f32)> {
        if !LinkPreference::Follow.checked(self.controls) {
            return None;
        }
        let reading = &self.active.as_ref()?.reading;
        let folder = book_folder(reading);
        let (width, _) = self.reading_viewport().dimensions();
        if !(0.0..width as f32).contains(&x) || !(0.0..reading.frame_height() as f32).contains(&y) {
            return None;
        }
        let internal_only = self.controls.ignore_external_link_taps;
        if !internal_only
            && let Some(link) = reading
                .link_context_at(x, y)
                .and_then(|(chapter, href)| ReflowLink::resolve(chapter, href, folder))
        {
            return Some((link, x, y));
        }
        let radius = if self.controls.larger_tap_area_for_links {
            LINK_TAP_RADIUS
        } else if internal_only {
            0.0
        } else {
            return None;
        };
        nearest_reflow_link(reading, x, y, radius, internal_only)
    }

    pub(super) fn fixed_tap_link(&self, x: f32, y: f32) -> Option<PdfLink> {
        if !LinkPreference::Follow.checked(self.controls) {
            return None;
        }
        self.fixed.as_ref()?.reading.link_at(x, y).filter(|link| {
            !self.controls.ignore_external_link_taps || link.target != PdfLinkTarget::External
        })
    }

    pub(super) fn open_fixed_link(&mut self, link: PdfLink) -> Result<()> {
        match link.target {
            PdfLinkTarget::Page(page) => {
                let target = self.fixed.as_ref().and_then(|fixed| {
                    fixed.reading.pdf_destination_jump(
                        page,
                        link.destination_point,
                        link.destination_partial,
                    )
                });
                self.follow_fixed(page, target)
            }
            PdfLinkTarget::External => {
                let address = link
                    .uri
                    .ok_or("This link refers to a resource outside the PDF")?;
                self.show(self.surface_screen());
                self.show_link_address(address);
                Ok(())
            }
            PdfLinkTarget::Unavailable => Err("This PDF link is unavailable".into()),
        }
    }

    pub(super) fn page_link_items(&self) -> Vec<Item> {
        if let Some(fixed) = &self.fixed {
            return fixed
                .reading
                .visible_links()
                .into_iter()
                .filter_map(|link| match link.target {
                    PdfLinkTarget::Page(page) => Some(Item::new(
                        fixed_page_label(&fixed.reading, page),
                        Action::OpenFixedLink(link),
                    )),
                    PdfLinkTarget::External => {
                        let address = link.uri.clone()?;
                        Some(
                            Item::new("External link", Action::OpenFixedLink(link)).detail(address),
                        )
                    }
                    PdfLinkTarget::Unavailable => None,
                })
                .collect();
        }
        let Some(active) = &self.active else {
            return Vec::new();
        };
        let reading = &active.reading;
        let folder = book_folder(reading);
        let mut links = Vec::new();
        for (range, _) in reading.visible_columns() {
            for (label, href) in reading.probe.link_hrefs_in(range) {
                if let Some(target) = ReflowLink::resolve(&reading.probe.chapter, href, folder)
                    && !links
                        .iter()
                        .any(|(existing, link)| existing == &label && link == &target)
                {
                    links.push((label, target));
                }
            }
        }
        for band in reading.probe.foreign_bands(reading.page) {
            for (label, href, _) in band.layout.note_links() {
                if let Some(target) = ReflowLink::resolve(&band.layout.note.chapter, href, folder)
                    && !links
                        .iter()
                        .any(|(existing, link)| existing == &label && link == &target)
                {
                    links.push((label, target));
                }
            }
        }
        links
            .into_iter()
            .map(|(label, target)| match target {
                ReflowLink::Internal(target) => Item::new(label, Action::Jump(target)),
                ReflowLink::Address(address) => {
                    Item::new(label, Action::OpenLinkAddress(address.clone())).detail(address)
                }
                ReflowLink::LocalDocument(target) => {
                    let detail = target.path.display().to_string();
                    Item::new(label, Action::OpenLocalDocument(target)).detail(detail)
                }
            })
            .collect()
    }
}

fn nearest_reflow_link(
    reading: &Reading,
    x: f32,
    y: f32,
    radius: f32,
    internal_only: bool,
) -> Option<(ReflowLink, f32, f32)> {
    let probe = &reading.probe;
    let folder = book_folder(reading);
    let mut nearest = None;
    let mut distance = radius * radius;
    let inside_foreign = probe.foreign_band_at(reading.page, x, y).is_some();
    for part in reading
        .visible_parts()
        .into_iter()
        .filter(|_| !inside_foreign)
    {
        let local_x = x - part.x as f32;
        let local_y = y - part.y as f32;
        let content_y = local_y - probe.settings.top_margin() as f32 + part.range.start;
        let top = part.range.start.max(content_y - radius);
        let end = part.range.end.min(content_y + radius);
        if top > end {
            continue;
        }
        let mut consider = |rect: HighlightRect, href: &str| {
            let x0 = rect.x as f32;
            let y0 = rect.y as f32;
            let x1 = (rect.x + rect.width) as f32;
            let y1 = (rect.y + rect.height) as f32;
            let dx = local_x - local_x.clamp(x0, x1);
            let dy = local_y - local_y.clamp(y0, y1);
            let candidate = dx * dx + dy * dy;
            if candidate <= distance
                && (candidate < distance || nearest.is_none())
                && let Some(link) = ReflowLink::resolve(&probe.chapter, href, folder)
                && (!internal_only || matches!(link, ReflowLink::Internal(_)))
            {
                distance = candidate;
                nearest = Some((
                    link,
                    part.x as f32 + (x0 + x1) * 0.5,
                    part.y as f32 + (y0 + y1) * 0.5,
                ));
            }
        };
        let first = probe
            .lines
            .partition_point(|line| line.top < top - probe.settings.text_height() as f32);
        let last = probe.lines.partition_point(|line| line.top <= end);
        let mut visible_spans = BTreeMap::new();
        for line in probe.lines[first..last]
            .iter()
            .filter(|line| line.bottom >= top)
        {
            for (key, span) in line.sources() {
                visible_spans.entry(key).or_insert_with(Vec::new).push(span);
            }
        }
        for (key, spans) in visible_spans {
            let Ok(index) = probe
                .source
                .blocks
                .binary_search_by_key(&key, |block| block.key)
            else {
                continue;
            };
            for link in &probe.source.blocks[index].links {
                if !spans
                    .iter()
                    .any(|span| span.start < link.bytes.end && span.end > link.bytes.start)
                {
                    continue;
                }
                let start = probe.anchor(key, link.bytes.start);
                let end = probe.anchor(key, link.bytes.end);
                for rect in probe.anchor_rects_in(part.range.clone(), &start, &end) {
                    consider(rect, &link.href);
                }
            }
        }
        if let crate::Renderer::Parley(layout) = &probe.renderer {
            for (href, cols, rows) in layout.image_link_regions(part.range.clone(), probe.settings)
            {
                consider(
                    HighlightRect {
                        x: cols.start as i32,
                        y: rows.start as i32,
                        width: cols.len() as i32,
                        height: rows.len() as i32,
                    },
                    href,
                );
            }
        }
    }
    for band in probe.foreign_bands(reading.page) {
        for (_, href, rects) in band.layout.note_links() {
            for rect in rects {
                let x0 = rect.x as f32;
                let y0 = rect.y as f32 + band.y_offset(reading.page) as f32;
                let x1 = (rect.x + rect.width) as f32;
                let y1 = (rect.y + rect.height) as f32 + band.y_offset(reading.page) as f32;
                let dx = x - x.clamp(x0, x1);
                let dy = y - y.clamp(y0, y1);
                let candidate = dx * dx + dy * dy;
                if candidate <= distance
                    && (candidate < distance || nearest.is_none())
                    && let Some(link) = ReflowLink::resolve(&band.layout.note.chapter, href, folder)
                    && (!internal_only || matches!(link, ReflowLink::Internal(_)))
                {
                    distance = candidate;
                    nearest = Some((link, (x0 + x1) * 0.5, (y0 + y1) * 0.5));
                }
            }
        }
    }
    nearest
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod local_document_tests {
    use super::*;
    use crate::app::tests::Fixture;

    fn is_local(link: Option<ReflowLink>, expected: &Path) -> bool {
        matches!(link, Some(ReflowLink::LocalDocument(target)) if target.path == expected)
    }

    #[test]
    fn local_documents_resolve_only_existing_book_files_and_keep_chapter_links_internal() {
        let fixture = Fixture::new();
        let folder = fixture.root.clone();
        let other = folder.join("Harbor notes.epub");
        std::fs::write(&other, b"not read by the resolver").unwrap();
        std::fs::write(folder.join("chapter.xhtml"), b"<html/>").unwrap();
        let expected = std::fs::canonicalize(&other).unwrap();
        let chapter = "EPUB/text/one.xhtml";

        assert!(is_local(
            ReflowLink::resolve(chapter, "Harbor%20notes.epub", &folder),
            &expected
        ));
        assert!(is_local(
            ReflowLink::resolve(chapter, "Harbor%20notes.epub?pos=12#part", &folder),
            &expected
        ));
        // The page of a numeric pos query is kept; a non-numeric position is not applied.
        assert!(matches!(
            ReflowLink::resolve(chapter, "Harbor%20notes.epub?pos=3", &folder),
            Some(ReflowLink::LocalDocument(LocalTarget { page: Some(3), .. }))
        ));
        assert!(matches!(
            ReflowLink::resolve(chapter, "Harbor%20notes.epub?pos=/body/p[2]", &folder),
            Some(ReflowLink::LocalDocument(LocalTarget { page: None, .. }))
        ));
        let url = url::Url::from_file_path(&other).unwrap();
        assert!(is_local(
            ReflowLink::resolve(chapter, url.as_str(), &folder),
            &expected
        ));
        assert!(is_local(
            ReflowLink::resolve(chapter, other.to_str().unwrap(), &folder),
            &expected
        ));
        // A chapter link keeps resolving inside the book even when a file of that name exists.
        assert!(matches!(
            ReflowLink::resolve(chapter, "chapter.xhtml", &folder),
            Some(ReflowLink::Internal(_))
        ));
        // A missing book file is not offered; it stays an ordinary internal link.
        assert!(!matches!(
            ReflowLink::resolve(chapter, "missing.epub", &folder),
            Some(ReflowLink::LocalDocument(_))
        ));
        assert!(matches!(
            ReflowLink::resolve(chapter, "https://example.org/harbor", &folder),
            Some(ReflowLink::Address(_))
        ));
    }
}
