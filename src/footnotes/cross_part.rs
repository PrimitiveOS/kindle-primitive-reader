//! Bounded, source-preserving preparation for foreign FB3 note references.
//!
//! This module only prepares candidates. It does not add visual page bands;
//! a consumer must first verify the actual styles and route every paint and
//! interaction through the retained foreign chapter and source.

use std::{collections::BTreeMap, ops::Range};

use crate::{
    Anchor, Line, Settings,
    book_style::{BookStyle, WritingMode, flow::Display},
    epub::Book,
    flow_view::ChapterFlow,
    parley_layout::ParleyChapter,
    source::Source,
    user_hyphenation,
    user_styles::Snapshot,
};
use parley::FontContext;

use super::Reference;

const MAX_REFERENCES: usize = 128;
const MAX_NOTES: usize = 8;
// Foreign preparation runs during foreground layout. Keep the target much
// smaller than the ordinary chapter ceiling; larger chapters remain links.
const MAX_CHAPTER_BYTES: usize = 64 * 1024;
const MAX_TOTAL_CHAPTER_BYTES: usize = 128 * 1024;
const MAX_CHAPTER_TAGS: usize = 1024;
const MAX_NOTE_BYTES: usize = 8192;
const MAX_NOTE_BLOCKS: usize = 32;
const MAX_NOTE_ELEMENTS: usize = 256;
const MAX_NOTE_LINKS: usize = 128;
const MAX_ANCESTORS: usize = 64;

/// An original host-chapter link pointing to one prepared foreign note.
pub(crate) struct Link {
    pub(crate) host_block: usize,
    pub(crate) bytes: Range<usize>,
    pub(crate) note: usize,
}

/// The actual target chapter and Source. Blocks use canonical source keys;
/// foreign anchors must use `book`, `chapter`, and `source.revision`.
pub(crate) struct Note {
    pub(crate) book: u64,
    pub(crate) chapter: String,
    pub(crate) id: String,
    pub(crate) root: usize,
    pub(crate) source: Source,
    pub(crate) blocks: Vec<usize>,
    shaping_language: Option<parley::Language>,
    respect_embedded: bool,
}

pub(crate) struct Plan {
    pub(crate) notes: Vec<Note>,
    pub(crate) links: Vec<Link>,
}

/// An isolated complete note layout. This is still not a page band; callers
/// must prove page placement and route interaction to the retained Source.
pub(crate) struct Layout {
    pub(crate) note: Note,
    pub(crate) renderer: ParleyChapter,
    pub(crate) flow: ChapterFlow,
    pub(crate) lines: Vec<Line>,
    pub(crate) settings: Settings,
    pub(crate) extent: f32,
    pub(crate) root_scale: f32,
    pub(crate) separator_on_right: bool,
}

/// A single foreign note copied into blank space on an existing body page.
/// It does not change host page ranges, page count or progress anchors.
#[derive(Clone, Copy)]
struct Placement {
    page: usize,
    y: u32,
    clear_from: u32,
    separator: Option<(u32, bool)>,
}

pub(crate) struct Band {
    pub(crate) layout: Layout,
    rows: u32,
    placements: Vec<Placement>,
}

impl Band {
    pub(crate) fn unplaced(layout: Layout) -> Self {
        let rows = layout.extent.ceil() as u32;
        Self {
            layout,
            rows,
            placements: Vec::new(),
        }
    }

    #[cfg(test)]
    pub(crate) fn new(page: usize, layout: Layout, host: &[u8]) -> Option<Self> {
        let mut band = Self::unplaced(layout);
        let y = band.layout.settings.text_height().checked_sub(band.rows)?;
        let gap = (band.layout.settings.font_size.ceil() as u32 / 3).max(1);
        let separator = y.checked_sub(gap)?;
        if !Self::page_blank(band.layout.settings, separator, host) {
            return None;
        }
        band.place(page, y, separator, Some(band.layout.separator_on_right));
        Some(band)
    }

    pub(crate) fn rows(&self) -> u32 {
        self.rows
    }

    pub(crate) fn rtl_lines(&self) -> usize {
        let blocks = self.layout.renderer.pagination_rtl_blocks();
        self.layout
            .lines
            .iter()
            .filter(|line| blocks.contains(&line.block))
            .count()
    }

    pub(crate) fn line_count(&self) -> usize {
        self.layout.lines.len()
    }

    /// The complete stack, separator included, may cover only blank host pixels.
    pub(crate) fn page_blank(settings: Settings, clear_from: u32, pixels: &[u8]) -> bool {
        let width = settings.width as usize;
        let Some(total) = width.checked_mul(settings.height as usize) else {
            return false;
        };
        if pixels.len() != total || clear_from > settings.text_height() {
            return false;
        }
        let first = (settings.top_margin() + clear_from) as usize * width;
        let end = (settings.top_margin() + settings.text_height()) as usize * width;
        pixels
            .get(first..end)
            .is_some_and(|rows| rows.iter().all(|&pixel| pixel == 255))
    }

    /// Positions are planned together for the whole page before being installed.
    pub(crate) fn place(&mut self, page: usize, y: u32, clear_from: u32, separator: Option<bool>) {
        self.placements.retain(|placement| placement.page != page);
        self.placements.push(Placement {
            page,
            y,
            clear_from,
            separator: separator.map(|right| (clear_from, right)),
        });
    }

    #[cfg(test)]
    pub(crate) fn add_page(&mut self, page: usize, host: &[u8]) -> bool {
        if self.displays(page) {
            return true;
        }
        let Some(original) = self.placements.first().copied() else {
            return false;
        };
        if !Self::page_blank(self.layout.settings, original.clear_from, host) {
            return false;
        }
        self.placements.push(Placement { page, ..original });
        true
    }

    fn placement(&self, page: usize) -> Option<Placement> {
        self.placements
            .iter()
            .find(|placement| placement.page == page)
            .copied()
    }

    pub(crate) fn y_offset(&self, page: usize) -> u32 {
        self.placement(page)
            .expect("the note is not displayed on that page")
            .y
    }

    pub(crate) fn displays(&self, page: usize) -> bool {
        self.placement(page).is_some()
    }

    pub(crate) fn remove_page(&mut self, page: usize) {
        self.placements.retain(|placement| placement.page != page);
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.placements.is_empty()
    }

    pub(crate) fn host_is_blank(&self, page: usize, pixels: &[u8]) -> bool {
        self.placement(page)
            .is_some_and(|p| Self::page_blank(self.layout.settings, p.clear_from, pixels))
    }

    /// Caller validates the untouched host for every note before painting into
    /// a temporary buffer, so errors cannot leave a partially displayed stack.
    pub(crate) fn paint_on(&mut self, page: usize, pixels: &mut [u8]) -> crate::Result<bool> {
        let Some(placement) = self.placement(page) else {
            return Ok(false);
        };
        let settings = self.layout.settings;
        let width = settings.width as usize;
        let note = self
            .layout
            .renderer
            .capture(0.0..self.layout.extent, settings)?;
        if note.len() != pixels.len() {
            return Ok(false);
        }
        let from = settings.top_margin() as usize * width;
        let to = (settings.top_margin() + placement.y) as usize * width;
        let count = self.rows as usize * width;
        let Some(note_rows) = note.get(from..from + count) else {
            return Ok(false);
        };
        let Some(target) = pixels.get_mut(to..to + count) else {
            return Ok(false);
        };
        target.copy_from_slice(note_rows);
        if let Some((separator, right)) = placement.separator {
            let left = if right {
                width - settings.right_margin() as usize - width / 7
            } else {
                settings.left_margin() as usize
            };
            let start = (settings.top_margin() + separator) as usize * width + left;
            let Some(rule) = pixels.get_mut(start..start + width / 7) else {
                return Ok(false);
            };
            rule.fill(85);
        }
        Ok(true)
    }

    #[cfg(test)]
    pub(crate) fn paint(&mut self, pixels: &mut [u8]) -> crate::Result<bool> {
        let Some(page) = self.placements.first().map(|p| p.page) else {
            return Ok(false);
        };
        if !self.host_is_blank(page, pixels) {
            return Ok(false);
        }
        self.paint_on(page, pixels)
    }

    /// True even in note margins so a blank part of the band cannot fall
    /// through to a host-chapter hit behind the overlay.
    pub(crate) fn contains(&self, page: usize, x: f32, y: f32) -> bool {
        let Some(placement) = self.placement(page) else {
            return false;
        };
        let settings = self.layout.settings;
        let top = (settings.top_margin() + placement.y) as f32;
        (0.0..settings.width as f32).contains(&x) && (top..top + self.rows as f32).contains(&y)
    }

    pub(crate) fn coordinates(&self, page: usize, x: f32, y: f32) -> Option<(f32, f32)> {
        if !self.contains(page, x, y) {
            return None;
        }
        let settings = self.layout.settings;
        let left = settings.left_margin() as f32;
        let right = (settings.width - settings.right_margin()) as f32;
        if !(left..right).contains(&x) {
            return None;
        }
        Some((
            x - left,
            y - (settings.top_margin() + self.y_offset(page)) as f32,
        ))
    }

    /// Both values are needed by callers: relative links inside the note are
    /// resolved against the foreign chapter, not the host chapter.
    pub(crate) fn link_at(&self, page: usize, x: f32, y: f32) -> Option<(&str, &str)> {
        let (x, y) = self.coordinates(page, x, y)?;
        let anchor = self.layout.anchor_at(x, y)?;
        let block = self
            .layout
            .note
            .source
            .blocks
            .iter()
            .find(|block| block.key == anchor.block)?;
        let link = block
            .links
            .iter()
            .find(|link| link.bytes.contains(&anchor.offset))?;
        Some((&self.layout.note.chapter, &link.href))
    }
}

impl Note {
    pub(crate) fn layout(
        self,
        book: &mut Book,
        mut fonts: FontContext,
        settings: Settings,
        styles: &Snapshot,
        hyphenation: &user_hyphenation::Rules,
    ) -> Option<Layout> {
        if settings.validate().is_err()
            // Foreign-band hit testing is restricted to text columns. Until it
            // supports exact margin hits, keep potentially bleeding text in
            // its original chapter, where interaction remains available.
            || settings.publisher_signed_horizontal_margins
            || settings.fit_content == crate::FitContentMode::Web
            || settings.hanging_punctuation
            || book.revision() != self.book
            || shaping_language(book, settings) != self.shaping_language
            || settings.typography.respect_embedded != self.respect_embedded
        {
            return None;
        }
        let current = book.chapter(book.chapter_index(&self.chapter)?).ok()?;
        if !self.source.matches_input(&current) {
            return None;
        }
        let targets = super::in_page::Targets::detached_fb3(&self.source, self.root, styles)?;
        let author =
            BookStyle::load_with_base(book, &self.chapter, &self.source, &mut fonts, settings)
                .with_user_css(styles)
                .ok()?;
        let (mut computed, quotes) =
            author.compute_deferred_quotes(&self.source.elements, settings, Some(&targets));
        // This stage does not carry generated quotation state or paint
        // generated/positioned boxes. Keep unsupported notes as links.
        if !quotes.is_empty() {
            return None;
        }
        for (index, style) in computed.iter_mut().enumerate() {
            if descendant(&self.source, index, self.root) {
                if style.flow.suppressed
                    || !style.flow.painted
                    || style.generated.is_some()
                    || style.first_letter.is_some()
                    || style.first_line.is_some()
                    || style.float_side.is_some()
                    || style.writing_mode != WritingMode::Horizontal
                    || !matches!(style.display, Display::Inline | Display::Block)
                    || style.page_breaks != Default::default()
                {
                    return None;
                }
            } else {
                style.flow.suppressed = true;
            }
        }
        let root_scale = computed.get(self.root)?.size_scale;
        let flow = ChapterFlow::from_styles(&self.source, &computed).ok()?;
        let (renderer, lines, extent) = ParleyChapter::with_flow(
            book,
            &self.chapter,
            &self.source,
            fonts,
            settings,
            &computed,
            hyphenation,
            &flow,
            Some(&targets),
        )
        .ok()?;
        let gap = settings.font_size.ceil();
        if !extent.is_finite()
            || extent <= 0.0
            || extent + gap >= settings.text_height() as f32
            || lines.is_empty()
            || lines.iter().any(|line| !self.blocks.contains(&line.block))
            || renderer.uses_float_geometry()
            || renderer.uses_joined_notes()
            || !renderer.publisher_height_spaces().is_empty()
            || !renderer.fitted_height_tails().is_empty()
        {
            return None;
        }
        let rtl_blocks = renderer.pagination_rtl_blocks();
        let rtl_lines = lines
            .iter()
            .filter(|line| rtl_blocks.contains(&line.block))
            .count();
        let separator_on_right = rtl_lines > lines.len() / 2;
        Some(Layout {
            note: self,
            renderer,
            flow,
            lines,
            settings,
            extent,
            root_scale,
            separator_on_right,
        })
    }
}

impl Layout {
    /// Coordinates are in the foreign renderer's local text columns. The
    /// returned anchor always refers to the foreign canonical source.
    pub(crate) fn anchor_at(&self, x: f32, y: f32) -> Option<Anchor> {
        let (key, offset) = self.renderer.source_at(x, y, &(0.0..self.extent), true)?;
        let block = self
            .note
            .source
            .blocks
            .iter()
            .find(|block| block.key == key)?;
        if !self.note.blocks.contains(&key)
            || offset > block.text.len()
            || !block.text.is_char_boundary(offset)
        {
            return None;
        }
        Some(Anchor {
            book: self.note.book,
            chapter: self.note.chapter.clone(),
            revision: self.note.source.revision,
            block: key,
            offset,
            illustration: None,
        })
    }
}

/// Prepare only explicit cross-chapter FB3 references with unique, complete
/// imported note roots. Every failure leaves the original link untouched.
#[cfg(test)]
fn prepare(book: &mut Book, chapter: &str, host: &Source) -> Option<Plan> {
    prepare_for_settings(book, chapter, host, Settings::default())
}

/// Match the host's effective book-language override and embedded-tag policy.
pub(crate) fn prepare_for_settings(
    book: &mut Book,
    chapter: &str,
    host: &Source,
    settings: Settings,
) -> Option<Plan> {
    let language = shaping_language(book, settings);
    let respect_embedded = settings.typography.respect_embedded;
    let mut targets = Vec::new();
    let mut inspected = 0;
    let manifest = book.complete_fb3_notes()?;
    for block in &host.blocks {
        if inside_note(host, block.element) {
            continue;
        }
        for link in &block.links {
            inspected += 1;
            if inspected > MAX_REFERENCES {
                // A later reference could compete for a page already planned
                // above. Do not publish a partial reference inventory.
                return None;
            }
            if link.note_reference != Reference::Explicit
                || block
                    .text
                    .get(link.bytes.clone())
                    .is_none_or(|label| label.trim().is_empty())
            {
                continue;
            }
            let Ok(target) = Book::resolve_link(chapter, &link.href) else {
                continue;
            };
            let Some(id) = target.fragment else { continue };
            if target.path == chapter || manifest.get(&id) != Some(&target.path) {
                continue;
            }
            targets.push((block.key, link.bytes.clone(), target.path, id));
        }
    }
    let mut plan = Plan {
        notes: Vec::new(),
        links: Vec::new(),
    };
    let mut resolved = BTreeMap::<(String, String), Option<usize>>::new();
    let mut total_bytes = 0usize;
    for (block, bytes, path, id) in targets {
        let key = (path.clone(), id.clone());
        let note = if let Some(note) = resolved.get(&key) {
            *note
        } else {
            let candidate = if plan.notes.len() < MAX_NOTES {
                prepare_one(
                    book,
                    &path,
                    &id,
                    &mut total_bytes,
                    language,
                    respect_embedded,
                )
            } else {
                None
            };
            let index = candidate.map(|candidate| {
                let index = plan.notes.len();
                plan.notes.push(candidate);
                index
            });
            resolved.insert(key, index);
            index
        };
        if let Some(note) = note {
            plan.links.push(Link {
                host_block: block,
                bytes,
                note,
            });
        }
    }
    (!plan.links.is_empty()).then_some(plan)
}

fn shaping_language(book: &Book, settings: Settings) -> Option<parley::Language> {
    settings
        .typography
        .resolve(book.language.map(crate::TypographyLocale::publication))
        .map(crate::TypographyLocale::shaping_language)
}

fn prepare_one(
    book: &mut Book,
    chapter: &str,
    id: &str,
    total: &mut usize,
    language: Option<parley::Language>,
    respect_embedded: bool,
) -> Option<Note> {
    let index = book.chapter_index(chapter)?;
    let xhtml = book.chapter(index).ok()?;
    let size = xhtml.len();
    if size > MAX_CHAPTER_BYTES || total.checked_add(size)? > MAX_TOTAL_CHAPTER_BYTES {
        return None;
    }
    // Bound DOM allocation before parsing. Valid markup escapes literal '<'.
    if xhtml.bytes().filter(|byte| *byte == b'<').count() > MAX_CHAPTER_TAGS {
        return None;
    }
    *total += size;
    let source = book
        .chapter_source_with_language(chapter, &xhtml, language, respect_embedded)
        .ok()?;
    let root = source.fragment_owner(id)?;
    let blocks = simple_note(&source, root, id)?;
    Some(Note {
        book: book.revision(),
        chapter: chapter.to_owned(),
        id: id.to_owned(),
        root,
        source,
        blocks,
        shaping_language: language,
        respect_embedded,
    })
}

fn inside_note(source: &Source, mut element: usize) -> bool {
    for _ in 0..MAX_ANCESTORS {
        let Some(node) = source.elements.get(element) else {
            return true;
        };
        if node.tag == "aside" && node.type_attribute.as_deref() == Some("footnote") {
            return true;
        }
        let Some(parent) = node.parent else {
            return false;
        };
        element = parent;
    }
    true
}

fn descendant(source: &Source, mut element: usize, root: usize) -> bool {
    for _ in 0..MAX_ANCESTORS {
        if element == root {
            return true;
        }
        let Some(parent) = source.elements.get(element).and_then(|node| node.parent) else {
            return false;
        };
        element = parent;
    }
    false
}

/// Keep this first stage deliberately narrow. The renderer must separately
/// reject unsupported computed CSS and generated layouts before painting.
fn simple_note(source: &Source, root: usize, id: &str) -> Option<Vec<usize>> {
    let element = source.elements.get(root)?;
    if element.tag != "aside"
        || element.type_attribute.as_deref() != Some("footnote")
        || element.id.as_deref() != Some(id)
        || element.selector_only
        || element.anonymous
    {
        return None;
    }
    let mut elements = 0;
    for (index, element) in source.elements.iter().enumerate() {
        if !descendant(source, index, root) {
            continue;
        }
        elements += 1;
        if elements > MAX_NOTE_ELEMENTS
            || element.selector_only
            || element.anonymous
            || (index != root
                && !matches!(
                    element.tag.as_str(),
                    "p" | "span"
                        | "a"
                        | "em"
                        | "strong"
                        | "i"
                        | "b"
                        | "u"
                        | "s"
                        | "sub"
                        | "sup"
                        | "br"
                ))
        {
            return None;
        }
    }
    let mut blocks = Vec::new();
    let mut bytes = 0usize;
    let mut links = 0usize;
    for block in &source.blocks {
        if !descendant(source, block.element, root) {
            continue;
        }
        bytes = bytes.checked_add(block.text.len())?;
        links = links.checked_add(block.links.len())?;
        if bytes > MAX_NOTE_BYTES
            || links > MAX_NOTE_LINKS
            || blocks.len() == MAX_NOTE_BLOCKS
            || !block.images.is_empty()
            || !block.ruby.is_empty()
            || block.table_cell.is_some()
            || block.list_item.is_some()
            || block
                .links
                .iter()
                .any(|link| link.note_reference == Reference::Explicit)
        {
            return None;
        }
        blocks.push(block.key);
    }
    (!blocks.is_empty() && bytes > 0).then_some(blocks)
}

#[cfg(test)]
mod tests;
