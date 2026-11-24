//! Source-space pagination for supported EPUB and FictionBook notes.
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::ops::Range;

use crate::epub::{Book, Destination};
use crate::loose::Fb2NoteKind;
use crate::user_styles::{FootnoteSize, Snapshot};
use crate::{
    Line, Settings, Source,
    publisher_breaks::{self, BoxRules, Edge},
};

pub(crate) struct Part {
    pub source: Range<f32>,
    pub y: u32,
}

pub(crate) struct Page {
    pub parts: Vec<Part>,
    pub separator: Option<u32>,
}

const MAX_LINES: usize = 16_384;
const MAX_NOTES: usize = 32;
const MAX_REFERENCES: usize = 128;
const MAX_PAGES: usize = 512;

pub(crate) fn paginate(
    chapter: &str,
    targets: &Targets,
    source: &Source,
    lines: &[Line],
    artwork_bottoms: &BTreeMap<usize, f32>,
    breaks: &[BoxRules],
    settings: Settings,
) -> Option<Vec<Page>> {
    let height = settings.text_height();
    let gap = settings.font_size.ceil() as u32;
    if lines.len() > MAX_LINES || gap == 0 || gap >= height {
        return None;
    }
    let (notes, references) = discover(chapter, targets, source, lines)?;
    let groups = line_groups(lines, &notes, artwork_bottoms, height)?;
    let extents: Vec<_> = groups.iter().map(|group| group.extent.clone()).collect();
    let edges = publisher_breaks::in_page_note_boundaries(&extents, breaks)?;
    let notes: Vec<_> = notes
        .into_iter()
        .map(|note| {
            groups.partition_point(|group| group.lines.end <= note.start)
                ..groups.partition_point(|group| group.lines.start < note.end)
        })
        .collect();
    if notes.iter().any(|note| {
        groups[note.clone()]
            .iter()
            .any(|group| height_of(&group.extent) > height - gap)
    }) {
        return None;
    }

    let mut packer = Packer {
        height,
        gap,
        pages: Vec::new(),
        draft: Draft::default(),
    };
    let mut pending = VecDeque::new();
    let mut first_index = 0;
    while let Some(first) = groups.get(first_index) {
        let mut end_index = first_index + 1;
        // Keep body groups together without pulling a preceding heading into
        // a note copy. Note slices still use their own exact source bounds.
        while edges.get(end_index) == Some(&Edge::Avoid) {
            end_index += 1;
        }
        let last = &groups[end_index - 1];
        let extent = first.extent.start..last.extent.end;
        if height_of(&extent) > height {
            return None;
        }
        let first_group = first_index == 0;
        if !first_group && edges.get(first_index) == Some(&Edge::Force) {
            packer.finish()?;
        }
        let body_start = packer.draft.body.as_ref().map_or_else(
            || if first_group { 0.0 } else { extent.start },
            |body| body.start,
        );
        if height_of(&(body_start..extent.end)) > height - packer.draft.reserve(gap) {
            if first_group {
                return None;
            }
            packer.finish()?;
        }
        let body = packer.draft.body.get_or_insert_with(|| extent.clone());
        if first_group {
            body.start = 0.0;
        }
        body.end = extent.end;

        for &id in references
            .range(first.lines.start..last.lines.end)
            .flat_map(|(_, ids)| ids)
        {
            if !pending.contains(&id) && !packer.draft.seen.contains(&id) {
                pending.push_back(id);
            }
        }
        while let Some(&id) = pending.front() {
            if !packer.add_note(id, &groups[notes[id].clone()])? {
                // Continue filling the body while a first note line cannot fit.
                break;
            }
            pending.pop_front();
        }
        first_index = end_index;
    }
    for id in pending {
        if !packer.add_note(id, &groups[notes[id].clone()])? {
            packer.finish()?;
            if !packer.add_note(id, &groups[notes[id].clone()])? {
                return None;
            }
        }
    }
    packer.finish()?;
    Some(packer.pages)
}

struct LineGroup {
    lines: Range<usize>,
    extent: Range<f32>,
}

fn line_groups(
    lines: &[Line],
    notes: &[Range<usize>],
    artwork_bottoms: &BTreeMap<usize, f32>,
    max_extent: u32,
) -> Option<Vec<LineGroup>> {
    let boundaries: BTreeSet<_> = notes
        .iter()
        .flat_map(|note| [note.start, note.end])
        .collect();
    let mut groups: Vec<LineGroup> = Vec::with_capacity(lines.len());
    for (index, line) in lines.iter().enumerate() {
        let extent = line.top..line.bottom;
        if !line.top.is_finite()
            || !line.bottom.is_finite()
            || line.top < 0.0
            || line.bottom <= line.top
            || (line.bytes.is_empty() && !artwork_bottoms.contains_key(&index))
            || line.page_extent() != extent
        {
            return None;
        }
        let extent = extent.start
            ..extent
                .end
                .max(artwork_bottoms.get(&index).copied().unwrap_or(extent.end));
        // Glyph extents can overlap after line spacing by a pixel or more.
        // Cut between whole bands so both paint and source hit testing agree.
        if let Some(previous) = groups.last_mut()
            && previous.extent.end > extent.start + 0.01
        {
            if boundaries.contains(&index) {
                return None;
            }
            previous.lines.end = index + 1;
            previous.extent.end = previous.extent.end.max(extent.end);
        } else {
            groups.push(LineGroup {
                lines: index..index + 1,
                extent,
            });
        }
    }
    groups
        .iter()
        .all(|group| height_of(&group.extent) <= max_extent)
        .then_some(groups)
}

type References = BTreeMap<usize, Vec<usize>>;

pub(crate) struct Targets {
    roots: BTreeMap<usize, usize>,
    ids: BTreeMap<String, usize>,
    ancestry: Vec<Option<usize>>,
    sizes: Vec<NoteSize>,
}

#[derive(Clone, Copy)]
enum NoteSize {
    Epub(FootnoteSize),
    Fb2(f32),
}

impl Targets {
    pub(crate) fn epub(xhtml: &str, source: &Source, size: FootnoteSize) -> Option<Self> {
        let document = roxmltree::Document::parse_with_options(
            xhtml,
            roxmltree::ParsingOptions {
                allow_dtd: true,
                ..Default::default()
            },
        )
        .ok()?;
        let mut marked = Vec::new();
        for node in document.descendants().filter(roxmltree::Node::is_element) {
            if !super::note_container(node) {
                continue;
            }
            let id = node
                .attribute("id")
                .or_else(|| node.attribute((super::XML, "id")))?;
            if marked.len() == MAX_NOTES {
                return None;
            }
            marked.push((id, NoteSize::Epub(size)));
        }
        Self::for_ids(source, marked)
    }

    pub(crate) fn fb2(
        chapter: &str,
        source: &Source,
        sections: &BTreeMap<String, Fb2NoteKind>,
        styles: &Snapshot,
    ) -> Option<Self> {
        let mut marked = BTreeMap::new();
        for link in source.blocks.iter().flat_map(|block| &block.links) {
            if let Ok(Destination {
                path,
                fragment: Some(id),
            }) = Book::resolve_link(chapter, &link.href)
                && path == chapter
                && let Some((authored, &kind)) = sections.get_key_value(&id)
                && let Some(scale) = styles.fb2_note_scale(kind)
            {
                marked.insert(authored.as_str(), NoteSize::Fb2(scale));
                if marked.len() > MAX_NOTES {
                    return None;
                }
            }
        }
        Self::for_ids(source, marked)
    }

    fn for_ids<'a>(
        source: &Source,
        marked: impl IntoIterator<Item = (&'a str, NoteSize)>,
    ) -> Option<Self> {
        let mut roots = BTreeMap::new();
        let mut ids = BTreeMap::new();
        let mut sizes = Vec::new();
        for (id, size) in marked {
            let owner = source.fragment_owner(id)?;
            let index = roots.len();
            if roots.insert(owner, index).is_some() || ids.insert(id.to_owned(), index).is_some() {
                return None;
            }
            sizes.push(size);
        }
        if roots.is_empty() {
            return None;
        }
        // Source elements are ordered so inherited ownership is already known.
        let mut ancestry: Vec<Option<usize>> = Vec::with_capacity(source.elements.len());
        for (index, element) in source.elements.iter().enumerate() {
            let inherited = element.parent.and_then(|parent| ancestry[parent]);
            let root = roots.get(&index).copied();
            if inherited.is_some() && root.is_some() {
                return None;
            }
            ancestry.push(root.or(inherited));
        }
        Some(Self {
            roots,
            ids,
            ancestry,
            sizes,
        })
    }

    pub(crate) fn font_scale(&self, index: usize, block: bool) -> Option<f32> {
        let id = self.ancestry[index]?;
        let root = self.roots.contains_key(&index);
        match self.sizes[id] {
            NoteSize::Epub(size) => size.scale(root, block),
            NoteSize::Fb2(size) => root.then_some(size),
        }
    }
}

fn discover(
    chapter: &str,
    targets: &Targets,
    source: &Source,
    lines: &[Line],
) -> Option<(Vec<Range<usize>>, References)> {
    let blocks: BTreeMap<_, _> = source
        .blocks
        .iter()
        .map(|block| (block.key, block))
        .collect();
    let mut regions = vec![None::<Range<usize>>; targets.roots.len()];
    let mut checked_directions = BTreeSet::new();
    let mut block_lines = BTreeMap::<_, Vec<_>>::new();
    for (index, line) in lines.iter().enumerate() {
        let block = blocks.get(&line.block)?;
        if block.table_cell.is_some()
            || (checked_directions.insert(line.block)
                && crate::publisher_padding::paragraph_is_rtl(block.direction, &block.text))
        {
            return None;
        }
        if let Some(id) = targets.ancestry[block.element] {
            if !block.images.is_empty() {
                return None;
            }
            let region = regions[id].get_or_insert(index..index);
            if region.end != index {
                return None;
            }
            region.end += 1;
        } else {
            block_lines.entry(line.block).or_default().push(index);
        }
    }
    let regions = regions.into_iter().collect::<Option<Vec<_>>>()?;
    let mut references = References::new();
    let mut used = BTreeSet::new();
    let mut count = 0;
    for block in &source.blocks {
        for link in &block.links {
            let Ok(Destination {
                path,
                fragment: Some(fragment),
            }) = Book::resolve_link(chapter, &link.href)
            else {
                continue;
            };
            if path != chapter {
                continue;
            }
            let Some(&id) = targets.ids.get(fragment.as_str()) else {
                continue;
            };
            if targets.ancestry[block.element].is_some() {
                return None;
            }
            let Some(index) = block_lines.get(&block.key).and_then(|indices| {
                indices.iter().copied().find(|&index| {
                    // A wrapping inline-block can leave the link's initial
                    // space on the previous line while its marker moves.
                    lines[index].source_spans().iter().any(|span| {
                        let start = span.start.max(link.bytes.start);
                        let end = span.end.min(link.bytes.end);
                        start < end && block.text[start..end].chars().any(|ch| !ch.is_whitespace())
                    })
                })
            }) else {
                continue;
            };
            count += 1;
            if count > MAX_REFERENCES {
                return None;
            }
            references.entry(index).or_default().push(id);
            used.insert(id);
        }
    }
    (used.len() == targets.roots.len()).then_some((regions, references))
}

#[derive(Default)]
struct Draft {
    body: Option<Range<f32>>,
    notes: Vec<Range<f32>>,
    note_height: u32,
    seen: BTreeSet<usize>,
}

impl Draft {
    fn reserve(&self, gap: u32) -> u32 {
        if self.notes.is_empty() {
            0
        } else {
            self.note_height + gap
        }
    }
}

struct Packer {
    height: u32,
    gap: u32,
    pages: Vec<Page>,
    draft: Draft,
}

impl Packer {
    fn add_note(&mut self, id: usize, mut groups: &[LineGroup]) -> Option<bool> {
        if self.draft.seen.contains(&id) {
            return Some(true);
        }
        while let Some(first) = groups.first() {
            let body_height = self.draft.body.as_ref().map_or(0, height_of);
            let available = self
                .height
                .saturating_sub(body_height + self.draft.note_height + self.gap);
            let start = first.extent.start;
            let count = groups
                .iter()
                .take_while(|group| height_of(&(start..group.extent.end)) <= available)
                .count();
            if count == 0 {
                return Some(false);
            }
            let range = start..groups[count - 1].extent.end;
            self.draft.note_height += height_of(&range);
            self.draft.notes.push(range);
            self.draft.seen.insert(id);
            groups = &groups[count..];
            if !groups.is_empty() {
                self.finish()?;
            }
        }
        Some(true)
    }

    fn finish(&mut self) -> Option<()> {
        let draft = std::mem::take(&mut self.draft);
        if draft.body.is_none() && draft.notes.is_empty() {
            return Some(());
        }
        if self.pages.len() == MAX_PAGES {
            return None;
        }
        let mut y = self.height - draft.note_height;
        let separator = (!draft.notes.is_empty()).then(|| y - (self.gap / 3).max(1));
        let body = draft.body.map(|source| Part { source, y: 0 });
        let parts = body
            .into_iter()
            .chain(draft.notes.into_iter().map(|source| {
                let offset = y;
                y += height_of(&source);
                Part { source, y: offset }
            }))
            .collect();
        self.pages.push(Page { parts, separator });
        Some(())
    }
}

pub(crate) fn height_of(range: &Range<f32>) -> u32 {
    (range.end - range.start).ceil() as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Engine, Probe, test_fixture};

    fn settings(enabled: bool) -> Settings {
        Settings {
            width: 720,
            height: 480,
            margin: 32,
            font_size: 25.0,
            line_height: 1.2,
            in_page_footnotes: enabled,
            ..Settings::default()
        }
    }

    fn open(chapter: usize, enabled: bool) -> Probe {
        open_with_settings(chapter, settings(enabled))
    }

    fn open_with_settings(chapter: usize, settings: Settings) -> Probe {
        Probe::open(
            &test_fixture("in-page-footnotes.epub"),
            Engine::Parley,
            &[test_fixture("fonts/NotoSans-Regular.ttf")],
            settings,
            chapter,
        )
        .unwrap()
    }

    fn capture(probe: &mut Probe, page: usize, name: &str) -> Vec<u8> {
        let pixels = probe.capture(page).unwrap();
        if let Some(path) = std::env::var_os("READER_IN_PAGE_FOOTNOTE_CAPTURES") {
            let path = std::path::PathBuf::from(path).join(name);
            image::save_buffer(
                path,
                &pixels,
                probe.settings.width,
                probe.settings.height,
                image::ColorType::L8,
            )
            .unwrap();
        }
        pixels
    }

    #[test]
    fn in_page_footnote_size_reflows_only_note_blocks_and_keeps_inline_scaling() {
        use crate::user_styles::{Data, EPUB_FOOTNOTES, FootnoteSize};
        let path = test_fixture("in-page-footnotes.epub");
        let fonts = [test_fixture("fonts/NotoSans-Regular.ttf")];
        let before = std::fs::read(&path).unwrap();
        let render = |size| {
            let mut data = Data::default();
            data.choose(1, EPUB_FOOTNOTES, Some(true)).unwrap();
            data.books.get_mut(&1).unwrap().footnote_size = Some(size);
            Probe::layout_opened_book(
                &mut Some(Book::open(&path).unwrap()),
                Engine::Parley,
                settings(false),
                7,
                None,
                &Default::default(),
                &data.snapshot(1).unwrap(),
                || crate::font_context(&fonts),
            )
            .unwrap()
        };
        let measure = |probe: &Probe, page, text: &str| {
            let first = probe.matches(text).remove(0);
            let end = probe.anchor(first.block, first.offset + text.len());
            let selected = probe.select_range(&first, &end).unwrap();
            let rect = probe.selection_rects(page, &selected);
            assert_eq!(rect.len(), 1, "{text}");
            rect[0].width as f32
        };
        let mut large = render(FootnoteSize::Full);
        let mut small = render(FootnoteSize::SixtyFive);
        let legacy = render(FootnoteSize::Legacy);
        let publisher = render(FootnoteSize::Publisher);
        let reference = large.fragment("size-ref").unwrap();
        let note = large.fragment("size-note").unwrap();
        let page = large.page_for(&reference).unwrap();
        let large_image = capture(&mut large, page, "reader-note-size-100.png");
        let small_image = capture(&mut small, page, "reader-note-size-65.png");
        assert_ne!(large_image, small_image);
        for probe in [&large, &small, &legacy, &publisher] {
            assert_eq!(probe.page_for(&reference), Some(page));
            assert_eq!(probe.fragment("size-note"), Some(note.clone()));
            assert!(probe.page_for(&note).unwrap() > page);
            assert!(
                probe.in_page_notes.as_ref().unwrap()[page]
                    .separator
                    .is_some()
            );
            assert_eq!(
                measure(probe, page, "Ordinary body text"),
                measure(&large, page, "Ordinary body text"),
            );
        }
        for phrase in ["Measured note text", "inline note label"] {
            let base = measure(&large, page, phrase);
            let fraction = measure(&small, page, phrase) / base;
            assert!((0.60..0.71).contains(&fraction), "{phrase}: {fraction}");
            assert_eq!(
                measure(&legacy, page, phrase),
                measure(&publisher, page, phrase),
            );
            assert!(measure(&publisher, page, phrase) > base);
        }
        let full_width = measure(&large, page, "Measured note text");
        for (size, scale) in [
            (FootnoteSize::Ninety, 0.90),
            (FootnoteSize::EightyFive, 0.85),
            (FootnoteSize::Eighty, 0.80),
            (FootnoteSize::SeventyFive, 0.75),
            (FootnoteSize::Seventy, 0.70),
        ] {
            let width = measure(&render(size), page, "Measured note text");
            assert!((width / full_width - scale).abs() < 0.04, "{size:?}");
        }
        eprintln!(
            "Reader note at 100% is {full_width}px, at 65% is {}px; normal text is {}px",
            measure(&small, page, "Measured note text"),
            measure(&large, page, "Ordinary body text"),
        );
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }

    #[test]
    fn fb2_in_page_footnotes_and_endnotes_keep_distinct_sizes_and_source_links() {
        use crate::user_styles::{Data, FB2_ENDNOTES, FB2_FOOTNOTES, FB2_REGULAR_NOTE_SIZE};
        let path = test_fixture("in-page-notes.fb2");
        let font = test_fixture("fonts/NotoSans-Regular.ttf");
        let before = std::fs::read(&path).unwrap();
        let render = |presets: &[&str]| {
            let mut data = Data::default();
            for id in presets {
                data.choose(1, id, Some(true)).unwrap();
            }
            Probe::layout_opened_book(
                &mut Some(Book::open(&path).unwrap()),
                Engine::Parley,
                settings(false),
                0,
                None,
                &Default::default(),
                &data.snapshot(1).unwrap(),
                || crate::font_context(std::slice::from_ref(&font)),
            )
            .unwrap()
        };
        let ordinary = render(&[]);
        assert!(ordinary.in_page_notes.is_none());
        let reference = ordinary.fragment("notes-ref").unwrap();
        let note = ordinary.fragment("survey-note").unwrap();
        let ending = ordinary.fragment("survey-comment").unwrap();
        let unused = ordinary.fragment("unused-note").unwrap();
        let selection = |probe: &Probe, text: &str| {
            let start = probe.matches(text).remove(0);
            let end = probe.anchor(start.block, start.offset + text.len());
            probe.select_range(&start, &end).unwrap()
        };

        let mut widths = Vec::new();
        for (label, presets, note_drawn, end_drawn) in [
            ("footnote-only", &[FB2_FOOTNOTES][..], true, false),
            ("endnote-only", &[FB2_ENDNOTES][..], false, true),
            ("both", &[FB2_FOOTNOTES, FB2_ENDNOTES][..], true, true),
            (
                "regular",
                &[FB2_FOOTNOTES, FB2_ENDNOTES, FB2_REGULAR_NOTE_SIZE][..],
                true,
                true,
            ),
        ] {
            let mut probe = render(presets);
            let page = probe.page_for(&reference).unwrap();
            assert!(
                probe.in_page_notes.as_ref().unwrap()[page]
                    .separator
                    .is_some()
            );
            assert_eq!(probe.fragment("survey-note"), Some(note.clone()));
            assert_eq!(probe.fragment("survey-comment"), Some(ending.clone()));
            assert_eq!(probe.fragment("unused-note"), Some(unused.clone()));
            let note_selection = selection(&probe, "Measured footnote text");
            let end_selection = selection(&probe, "Measured endnote text");
            assert_eq!(
                probe.selection_rects(page, &note_selection).len(),
                usize::from(note_drawn)
            );
            assert_eq!(
                probe.selection_rects(page, &end_selection).len(),
                usize::from(end_drawn)
            );
            assert!(
                probe
                    .selection_rects(page, &selection(&probe, "The unused calibration record"))
                    .is_empty()
            );
            let canonical_note = probe.page_for(&note).unwrap();
            let canonical_end = probe.page_for(&ending).unwrap();
            assert!(canonical_note > page && canonical_end > page);
            let note_width = probe.selection_rects(canonical_note, &note_selection)[0].width;
            let end_width = probe.selection_rects(canonical_end, &end_selection)[0].width;
            if note_drawn {
                let backlink = selection(&probe, "Return to the survey");
                let location = probe.selection_rects(page, &backlink)[0];
                assert_eq!(
                    probe
                        .link_at(
                            page,
                            (location.x + location.width / 2) as f32,
                            (location.y + location.height / 2) as f32
                        )
                        .and_then(|target| target.fragment),
                    Some("notes-ref".into()),
                );
            }
            eprintln!("Reader FB2 {label}: footnote={note_width}px endnote={end_width}px");
            widths.push((note_width, end_width));
            capture(&mut probe, page, &format!("reader-fb2-{label}.png"));
        }
        assert_eq!(widths[0].0, widths[2].0);
        assert_eq!(widths[1].1, widths[2].1);
        assert_eq!(widths[0].1, widths[3].1);
        assert_eq!(widths[1].0, widths[3].0);
        assert!((0.70..0.80).contains(&(widths[0].0 as f32 / widths[3].0 as f32)));
        assert!((0.80..0.90).contains(&(widths[1].1 as f32 / widths[3].1 as f32)));
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }

    #[test]
    fn in_page_band_preserves_sources_links_and_selection_geometry() {
        let ordinary = open(0, false);
        let mut probe = open(0, true);
        let note = ordinary.fragment("note").unwrap();
        let origin = ordinary.fragment("start").unwrap();
        let reference = probe.fragment("ref").unwrap();
        assert_eq!(probe.fragment("note"), Some(note.clone()));
        assert_eq!(probe.page_anchor(0), Some(origin));
        let pages = probe
            .in_page_notes
            .as_ref()
            .expect("eligible authored note");
        let target_page = probe.page_for(&reference).unwrap();
        assert!(probe.page_for(&note).unwrap() > target_page);
        assert!(probe.page_parts(target_page).any(|(source, offset)| {
            offset > 0 && source.contains(&probe.anchor_top(&note).unwrap())
        }));
        assert_eq!(pages[target_page].parts.len(), 2);
        let note_part = &pages[target_page].parts[1];
        let note_top = note_part.y + probe.settings.top_margin();
        let separator = pages[target_page].separator.unwrap() + probe.settings.top_margin();
        assert!(separator < note_top);
        let selected = probe
            .select_range(
                &probe.matches("repair date")[0],
                &probe.matches(" comes")[0],
            )
            .unwrap();
        let rectangles = probe.selection_rects(target_page, &selected);
        assert!(!rectangles.is_empty());
        for rect in rectangles {
            assert!(rect.y >= note_top as i32);
            let x = (rect.x + rect.width / 4) as f32;
            let y = (rect.y + rect.height / 2) as f32;
            let word = probe.word_at(target_page, x, y).unwrap();
            assert!(selected.text.contains(&word.text));
            assert_eq!(word.start.block, note.block);
        }
        assert!(
            probe
                .links_on_page(target_page)
                .iter()
                .any(|(_, to)| to.fragment.as_deref() == Some("ref"))
        );
        let link = probe
            .select_range(
                &probe.matches("Return")[0],
                &probe.anchor(
                    probe.matches("Return")[0].block,
                    probe.matches("Return")[0].offset + "Return".len(),
                ),
            )
            .unwrap();
        let rect = probe.selection_rects(target_page, &link)[0];
        let destination = probe
            .link_at(
                target_page,
                (rect.x + rect.width / 2) as f32,
                (rect.y + rect.height / 2) as f32,
            )
            .unwrap();
        assert_eq!(destination.fragment.as_deref(), Some("ref"));
        let pixels = capture(&mut probe, target_page, "reader-note-band.png");
        let start = (separator * probe.settings.width + probe.settings.left_margin()) as usize;
        let width = (probe.settings.width / 7) as usize;
        assert!(pixels[start..start + width].iter().all(|&gray| gray == 85));
    }

    #[test]
    fn in_page_long_note_continues_and_unsupported_chapter_stays_identical() {
        let mut probe = open(1, true);
        let note = probe.fragment("note").unwrap();
        let end = probe.fragment("note-end").unwrap();
        let body = probe.fragment("body-after").unwrap();
        let start_page = probe.page_for(&probe.fragment("ref").unwrap()).unwrap();
        let pages = probe.in_page_notes.as_ref().unwrap();
        let note_top = probe.anchor_top(&note).unwrap();
        let end_top = probe.anchor_top(&end).unwrap();
        let note_page = |top| {
            pages
                .iter()
                .position(|page| {
                    page.parts
                        .iter()
                        .any(|part| part.y > 0 && part.source.contains(&top))
                })
                .unwrap()
        };
        assert_eq!(note_page(note_top), start_page);
        let end_page = note_page(end_top);
        assert!(end_page > start_page);
        assert!(probe.page_for(&body).is_some());
        let mut previous = None;
        for index in 1..18 {
            let anchor = probe.fragment(&format!("sounding-{index}")).unwrap();
            let page = note_page(probe.anchor_top(&anchor).unwrap());
            assert!(previous.is_none_or(|previous| page >= previous));
            previous = Some(page);
        }
        let pages = probe.in_page_notes.as_ref().unwrap();
        assert!(
            pages[start_page..=end_page]
                .iter()
                .any(|page| page.parts[0].y > 0)
        );
        capture(&mut probe, start_page, "reader-note-first-slice.png");
        capture(&mut probe, start_page + 1, "reader-note-only-slice.png");
        capture(&mut probe, end_page, "reader-note-last-slice.png");

        let mut ordinary = open(2, false);
        let mut fallback = open(2, true);
        assert!(fallback.in_page_notes.is_none());
        assert_eq!(ordinary.page_count(), fallback.page_count());
        assert_eq!(
            ordinary.fragment("unreferenced"),
            fallback.fragment("unreferenced")
        );
        assert_eq!(ordinary.capture(0).unwrap(), fallback.capture(0).unwrap());
    }

    #[test]
    fn inline_footnote_marker_owns_its_band_across_a_page_break() {
        for (respect_embedded, language) in [(true, "de"), (false, "fr")] {
            let mut probe = open_with_settings(
                3,
                Settings {
                    height: 250,
                    inline_block_mode: crate::InlineBlockMode::BoxSizing,
                    typography: crate::TypographyLanguages {
                        main: crate::TypographyLocale::parse("fr"),
                        respect_embedded,
                        ..Default::default()
                    },
                    ..settings(true)
                },
            );
            let reference_block = probe
                .source
                .blocks
                .iter()
                .find(|block| block.links.iter().any(|link| link.href == "#boxed-note"))
                .unwrap();
            let reference = reference_block
                .links
                .iter()
                .find(|link| link.href == "#boxed-note")
                .unwrap();
            assert_eq!(&reference_block.text[reference.bytes.clone()], " 2");
            let marker_offset = reference.bytes.end - 1;
            assert_eq!(
                reference_block
                    .styled_text
                    .iter()
                    .find(|run| run.bytes.contains(&marker_offset))
                    .unwrap()
                    .language
                    .unwrap()
                    .as_str(),
                language
            );
            let whitespace = probe.anchor(reference_block.key, reference.bytes.start);
            let marker = probe.anchor(reference_block.key, marker_offset);
            let after_marker = probe.anchor(reference_block.key, reference.bytes.end);
            let preceding_page = probe.page_for(&whitespace).unwrap();
            let marker_page = probe.page_for(&marker).unwrap();
            assert!(
                marker_page > preceding_page,
                "boxed marker must move to a new page"
            );
            let pages = probe
                .in_page_notes
                .as_ref()
                .expect("combined eligible authored note");
            assert!(
                pages[preceding_page].separator.is_none(),
                "markerless preceding page has a footnote band"
            );
            assert!(
                pages[marker_page].separator.is_some(),
                "marked page has no footnote band"
            );
            let note = pages[marker_page].parts.last().unwrap();
            assert!(note.y > 0);
            let source = note.source.clone();
            let top = probe.settings.top_margin() as usize;
            let bottom = probe.settings.bottom_margin() as usize;
            let rows = height_of(&source) as usize;
            assert!(
                top + bottom + rows < 4 * top,
                "scratch height must exercise margin clamping"
            );
            let width = probe.settings.width as usize;
            let from = (top + note.y as usize) * width;
            let count = rows * width;

            let selection = probe.select_range(&marker, &after_marker).unwrap();
            let rect = probe.selection_rects(marker_page, &selection)[0];
            let target = probe
                .link_at(
                    marker_page,
                    (rect.x + rect.width / 2) as f32,
                    (rect.y + rect.height / 2) as f32,
                )
                .unwrap();
            assert_eq!(target.fragment.as_deref(), Some("boxed-note"));
            let composed = probe.capture(marker_page).unwrap();
            let ordinary = probe.capture_range(source).unwrap();
            assert!(
                ordinary[top * width..top * width + count]
                    .iter()
                    .any(|&pixel| pixel < 255)
            );
            assert_eq!(
                &composed[from..from + count],
                &ordinary[top * width..top * width + count]
            );
        }
    }

    #[test]
    fn interleaved_note_stays_in_the_chapter_and_links_back_from_its_copy() {
        let mut probe = open(4, true);
        let ordinary = open(4, false);
        let reference = probe.fragment("cross-ref").unwrap();
        let note = probe.fragment("cross-note").unwrap();
        let resume = probe.fragment("resume").unwrap();
        assert_eq!(ordinary.fragment("cross-note"), Some(note.clone()));
        assert_eq!(ordinary.fragment("resume"), Some(resume.clone()));
        let marker_page = probe.page_for(&reference).unwrap();
        let note_page = probe.page_for(&note).unwrap();
        assert!(marker_page < note_page);
        assert!(note_page <= probe.page_for(&resume).unwrap());
        let note_y = probe.anchor_top(&note).unwrap();
        assert!(
            probe
                .page_parts(marker_page)
                .any(|(range, y)| y > 0 && range.contains(&note_y))
        );
        let label = "Return to gauge";
        let start = probe.matches(label).pop().unwrap();
        let selection = probe
            .select_range(
                &start,
                &probe.anchor(start.block, start.offset + label.len()),
            )
            .unwrap();
        let rect = probe.selection_rects(marker_page, &selection)[0];
        let target = probe
            .link_at(
                marker_page,
                (rect.x + rect.width / 2) as f32,
                (rect.y + rect.height / 2) as f32,
            )
            .unwrap();
        assert_eq!(target.fragment.as_deref(), Some("cross-ref"));
        capture(&mut probe, marker_page, "reader-note-interleaved-band.png");
        capture(
            &mut probe,
            note_page,
            "reader-note-interleaved-canonical.png",
        );
    }

    #[test]
    fn body_artwork_keeps_complete_pixels_links_and_anchors_when_notes_reflow() {
        let mut probe = open_with_settings(
            5,
            Settings {
                height: 370,
                ..settings(true)
            },
        );
        let ordinary = open_with_settings(
            5,
            Settings {
                height: 370,
                ..settings(false)
            },
        );
        let contract = probe.fragment("diagram").unwrap();
        assert_eq!(ordinary.fragment("diagram"), Some(contract.clone()));
        let diagram_page = probe.page_for(&contract).unwrap();
        let reference_page = probe.page_for(&probe.fragment("key-ref").unwrap()).unwrap();
        assert!(
            probe.in_page_notes.as_ref().unwrap()[reference_page]
                .separator
                .is_some()
        );
        let crate::Renderer::Parley(layout) = &probe.renderer else {
            unreachable!()
        };
        let mut visible = Vec::new();
        for page in 0..probe.page_count() {
            for (range, y) in probe.page_parts(page) {
                for (path, cols, rows) in
                    layout.visible_illustrations(range.clone(), probe.settings)
                {
                    assert_eq!(y, 0);
                    visible.push((page, range.clone(), path, cols, rows));
                }
            }
        }
        assert_eq!(
            visible.len(),
            2,
            "one standalone and one inline illustration must each paint once"
        );
        let (page, range, path, cols, rows) = visible.remove(0);
        assert_eq!(page, diagram_page);
        assert!(rows.len() > visible[0].4.len());
        let x = (cols.start + cols.len() / 2) as f32;
        let y = (rows.start + rows.len() / 2) as f32;
        assert_eq!(probe.illustration_at(page, x, y), Some(path));
        assert_eq!(
            probe.link_at(page, x, y).unwrap().fragment.as_deref(),
            Some("diagram-legend")
        );
        let width = probe.settings.width as usize;
        let composed = capture(&mut probe, page, "reader-note-illustrated-page.png");
        let canonical = probe.capture_range(range).unwrap();
        assert!(rows.clone().any(|row| {
            canonical[row * width + cols.start..row * width + cols.end]
                .iter()
                .any(|&pixel| pixel < 200)
        }));
        for row in rows {
            let pixels = row * width + cols.start..row * width + cols.end;
            assert_eq!(&composed[pixels.clone()], &canonical[pixels]);
        }
    }

    #[test]
    fn notes_leave_an_authored_hard_page_break_in_place() {
        let probe = open(6, true);
        let marker = probe.fragment("force-marker").unwrap();
        let restart = probe.fragment("force-restart").unwrap();
        let marker_page = probe.page_for(&marker).unwrap();
        let restart_page = probe.page_for(&restart).unwrap();
        assert!(restart_page > marker_page);
        assert!(
            probe.in_page_notes.as_ref().unwrap()[marker_page]
                .separator
                .is_some()
        );
        assert_eq!(probe.page_anchor(restart_page), Some(restart));
    }

    #[test]
    #[ignore = "Requires a private manifest of authorized, unchanged original EPUBs"]
    fn original_epub_in_page_notes_compose_and_preserve_canonical_anchors() {
        use crate::user_styles::{Data, EPUB_FOOTNOTES, FootnoteSize};
        #[derive(serde::Deserialize)]
        struct Original {
            path: std::path::PathBuf,
            chapters: Vec<Chapter>,
        }
        #[derive(serde::Deserialize)]
        struct Chapter {
            href: String,
            notes: Vec<String>,
            media: usize,
        }
        let manifest = std::env::var_os("READER_IN_PAGE_FOOTNOTE_ORIGINALS").unwrap();
        let input = std::fs::File::open(manifest)
            .unwrap_or_else(|_| panic!("cannot open private manifest"));
        let originals: Vec<Original> =
            serde_json::from_reader(input).unwrap_or_else(|_| panic!("invalid private manifest"));
        let fonts = [
            test_fixture("fonts/NotoSans-Regular.ttf"),
            test_fixture("fonts/NotoSans-Bold.ttf"),
        ];
        let settings = Settings {
            height: 1606,
            font_size: 47.0,
            ..Default::default()
        };
        let mut data = Data::default();
        data.choose(0, EPUB_FOOTNOTES, Some(true)).unwrap();
        data.books.get_mut(&0).unwrap().footnote_size = Some(FootnoteSize::SixtyFive);
        let ordinary_styles = Default::default();
        let note_styles = data.snapshot(0).unwrap();
        let mut tested = 0;
        let mut composed = 0;
        let mut composed_media = 0;
        let mut painted_bands = 0;
        let mut anchors = 0;
        for (ordinal, original) in originals.iter().enumerate() {
            let before = std::fs::read(&original.path).expect("cannot read private EPUB");
            let package =
                Book::open(&original.path).unwrap_or_else(|_| panic!("cannot open private EPUB"));
            for chapter in &original.chapters {
                let index = package
                    .chapters
                    .iter()
                    .position(|entry| entry.href == chapter.href)
                    .expect("private chapter locator");
                let open = |styles| {
                    Probe::layout_opened_book(
                        &mut Some(Book::open(&original.path).expect("cannot open private EPUB")),
                        Engine::Parley,
                        settings,
                        index,
                        None,
                        &Default::default(),
                        styles,
                        || crate::font_context(&fonts),
                    )
                    .unwrap_or_else(|_| panic!("cannot render private chapter"))
                };
                let ordinary = open(&ordinary_styles);
                let mut enabled = open(&note_styles);
                tested += 1;
                let bands = enabled.in_page_notes.as_ref().map_or(Vec::new(), |pages| {
                    pages
                        .iter()
                        .enumerate()
                        .filter_map(|(page, part)| part.separator.map(|_| page))
                        .collect::<Vec<_>>()
                });
                eprintln!(
                    "private edition={} spine={} media={} bands={}",
                    ordinal,
                    index,
                    chapter.media,
                    bands.len()
                );
                for id in &chapter.notes {
                    assert!(
                        ordinary.fragment(id) == enabled.fragment(id),
                        "private anchor changed"
                    );
                    anchors += usize::from(enabled.fragment(id).is_some());
                }
                if let Some(&page) = bands.first() {
                    let (_, offset) = enabled.page_parts(page).last().unwrap();
                    let pixels = enabled
                        .capture(page)
                        .unwrap_or_else(|_| panic!("cannot paint private note"));
                    let start = (offset + settings.top_margin()) as usize * settings.width as usize;
                    assert!(pixels[start..].iter().any(|&gray| gray < 225));
                    composed += 1;
                    composed_media += usize::from(chapter.media > 0);
                    painted_bands += bands.len();
                }
            }
            assert!(std::fs::read(&original.path).expect("cannot read private EPUB") == before);
        }
        eprintln!(
            "private matched={tested} composed={composed} composed_media={composed_media} bands={painted_bands} canonical_anchors={anchors}"
        );
        assert!(composed > 0);
    }

    #[cfg(feature = "kindle")]
    #[test]
    fn in_page_session_resumes_note_only_page_without_changing_the_canonical_jump() {
        use crate::session::{BookConfig, Reading, ReadingViewport};

        let config = BookConfig {
            viewport: ReadingViewport::PORTRAIT,
            user_hyphenation: Default::default(),
            user_styles: Default::default(),
            chapter_offsets: Default::default(),
            screen_pages: Default::default(),
            eager_layouts: Default::default(),
            engine: Engine::Parley,
            book: test_fixture("in-page-footnotes.epub"),
            fonts: vec![test_fixture("fonts/NotoSans-Regular.ttf")],
            settings: settings(true),
            scroll: false,
            facing_pages: false,
            hide_nonlinear: false,
            hidden_flows: Default::default(),
        };
        let mut forward = Reading::restore(config.clone(), 1, 0, None).unwrap();
        let body_end = forward.visible_end();
        forward.go_to_page(1, 1).unwrap();
        assert_eq!(forward.page, 1);
        assert_eq!(forward.visible_end(), body_end);
        let parts = forward.visible_parts();
        assert_eq!(parts.len(), 1);
        assert!(parts[0].y > 0);
        let anchor = forward.anchor().unwrap();
        let canonical_page = forward.probe.page_for(&anchor).unwrap();
        assert_ne!(canonical_page, forward.page);
        let restored = Reading::restore(config.clone(), 1, forward.page, Some(&anchor)).unwrap();
        assert_eq!(restored.page, forward.page);
        assert_eq!(restored.anchor(), Some(anchor));
        assert_eq!(restored.visible_text(), forward.visible_text());
        assert!(restored.visible_text().contains("Sounding"));

        let continuous = Reading::restore(
            BookConfig {
                scroll: true,
                ..config
            },
            1,
            0,
            None,
        )
        .unwrap();
        assert!(continuous.is_continuous());
        assert!(continuous.probe.in_page_notes.is_none());
    }
}
