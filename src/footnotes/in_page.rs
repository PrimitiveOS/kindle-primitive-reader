//! Source-space pagination for supported EPUB and FictionBook notes.
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::ops::Range;

use crate::epub::{Book, Destination};
use crate::loose::Fb2NoteKind;
use crate::user_styles::{FootnoteSize, NoteExtension, NoteFormatting, Snapshot};
use crate::{
    Line, Settings, Source,
    publisher_breaks::{self, BoxRules, Edge},
    table_layout::TableKey,
};

#[cfg(test)]
mod rtl_tests;

pub(crate) struct Part {
    pub source: Range<f32>,
    pub y: u32,
}

pub(crate) struct Page {
    pub parts: Vec<Part>,
    pub separator: Option<u32>,
    pub separator_on_right: bool,
    pub canonical_note_group: Option<usize>,
}

pub(crate) enum TableCopy {
    Grid(Range<f32>),
    Inline,
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
    rtl_blocks: &HashSet<usize>,
    table_extents: &HashMap<TableKey, TableCopy>,
    breaks: &[BoxRules],
    settings: Settings,
) -> Option<Vec<Page>> {
    let height = settings.text_height();
    let gap = settings.font_size.ceil() as u32;
    if lines.len() > MAX_LINES || gap == 0 || gap >= height {
        return None;
    }
    let (notes, references, tables) = discover(chapter, targets, source, lines, table_extents)?;
    let groups = line_groups(lines, &notes, artwork_bottoms, rtl_blocks, tables, height)?;
    let extents: Vec<_> = groups.iter().map(|group| group.extent.clone()).collect();
    let edges = publisher_breaks::in_page_note_boundaries(&extents, breaks)?;
    let notes: Vec<_> = notes
        .into_iter()
        .map(|note| {
            groups.partition_point(|group| group.lines.end <= note.start)
                ..groups.partition_point(|group| group.lines.start < note.end)
        })
        .collect();
    let mut group_flows = vec![None; groups.len()];
    if targets.separate_nonlinear {
        for (region, flow) in notes.iter().zip(targets.canonical_groups(source)) {
            group_flows[region.clone()].fill(flow);
        }
    }
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
        let flow = group_flows[first_index];
        let mut end_index = first_index + 1;
        // Keep body groups together without pulling a preceding heading into
        // a note copy. Note slices still use their own exact source bounds.
        while edges.get(end_index) == Some(&Edge::Avoid)
            && group_flows.get(end_index) == Some(&flow)
        {
            end_index += 1;
        }
        let last = &groups[end_index - 1];
        let extent = first.extent.start..last.extent.end;
        if height_of(&extent) > height {
            return None;
        }
        let first_group = first_index == 0;
        if !first_group && group_flows[first_index - 1] != flow {
            // Keep continuations of a reference on regular pages even when
            // the next original body is entering a canonical note flow.
            if flow.is_some() {
                packer.flush_notes(&mut pending, &groups, &notes)?;
            }
            packer.finish()?;
        } else if !first_group && edges.get(first_index) == Some(&Edge::Force) {
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
        packer.draft.canonical_note_group = flow;
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
    packer.flush_notes(&mut pending, &groups, &notes)?;
    packer.finish()?;
    Some(packer.pages)
}

struct LineGroup {
    lines: Range<usize>,
    extent: Range<f32>,
    rtl_lines: usize,
}

fn line_groups(
    lines: &[Line],
    notes: &[Range<usize>],
    artwork_bottoms: &BTreeMap<usize, f32>,
    rtl_blocks: &HashSet<usize>,
    tables: Vec<LineGroup>,
    max_extent: u32,
) -> Option<Vec<LineGroup>> {
    let boundaries: BTreeSet<_> = notes
        .iter()
        .flat_map(|note| [note.start, note.end])
        .collect();
    let mut groups: Vec<LineGroup> = Vec::with_capacity(lines.len());
    let mut tables = tables.into_iter().peekable();
    let mut pending_table: Option<LineGroup> = None;
    for (index, line) in lines.iter().enumerate() {
        let rtl = usize::from(rtl_blocks.contains(&line.block));
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
        if tables
            .peek()
            .is_some_and(|table| table.lines.start == index)
        {
            pending_table = tables.next();
        }
        let group = if let Some(table) = &mut pending_table {
            table.extent.start = table.extent.start.min(extent.start);
            table.extent.end = table.extent.end.max(extent.end);
            table.rtl_lines += rtl;
            if table.lines.end > index + 1 {
                continue;
            }
            pending_table.take()?
        } else {
            LineGroup {
                lines: index..index + 1,
                extent,
                rtl_lines: rtl,
            }
        };
        // Glyph extents can overlap after line spacing by a pixel or more.
        // Cut between whole bands so both paint and source hit testing agree.
        if let Some(previous) = groups.last_mut()
            && previous.extent.end > group.extent.start + 0.01
        {
            if boundaries.contains(&group.lines.start) {
                return None;
            }
            previous.lines.end = group.lines.end;
            previous.extent.start = previous.extent.start.min(group.extent.start);
            previous.extent.end = previous.extent.end.max(group.extent.end);
            previous.rtl_lines += group.rtl_lines;
        } else {
            groups.push(group);
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
    starts_note_part: Vec<bool>,
    sizes: Vec<NoteSize>,
    formatting: NoteFormatting,
    wikipedia_backlinks: BTreeSet<usize>,
    separate_nonlinear: bool,
}

#[derive(Clone, Copy)]
enum NoteSize {
    Epub {
        font: FootnoteSize,
        clear_margins: bool,
    },
    Wikipedia(FootnoteSize),
    Fb2(f32),
}

const CLASSIC_NOTE_CLASSES: &[&str] = &[
    "footnote",
    "footnote1",
    "footnotes",
    "fn",
    "note",
    "note1",
    "note2",
    "note3",
    "ntb",
    "ntb-txt",
    "ntb-txt-j",
    "fnote",
    "fnote1",
    "duokan-footnote-item",
    "przypis",
    "przypis1",
    "voetnoten",
];

impl Targets {
    pub(crate) fn inlines_all(&self) -> bool {
        self.formatting.inline_all
    }

    /// Only a complete native table contained in one note can be flattened
    /// into an inline row. The ordinary grids use their own page geometry.
    pub(crate) fn native_tables(&self, source: &Source) -> HashMap<TableKey, usize> {
        let mut ownership = HashMap::new();
        for &(element, cell) in &source.table_cells {
            let owner = self.ancestry[element];
            ownership
                .entry(cell.table)
                .and_modify(|previous| {
                    if *previous != owner {
                        *previous = None;
                    }
                })
                .or_insert(owner);
        }
        ownership
            .into_iter()
            .filter_map(|(key, note)| Some((key, note?)))
            .collect()
    }

    pub(crate) fn inline_part(&self, source: &Source, mut element: usize) -> Option<usize> {
        if !self.formatting.inline_all || self.ancestry[element].is_none() {
            return None;
        }
        loop {
            if self.starts_note_part[element] {
                return Some(element);
            }
            element = source.elements[element].parent?;
        }
    }

    pub(crate) fn epub_for_settings(
        chapter: &str,
        xhtml: &str,
        source: &Source,
        settings: crate::Settings,
        styles: &Snapshot,
    ) -> Option<Self> {
        if !settings.in_page_footnotes && !styles.in_page_epub_footnotes() {
            return None;
        }
        Self::epub(
            chapter,
            xhtml,
            source,
            settings.in_page_footnotes || styles.explicit_epub_notes(),
            styles,
        )
    }

    pub(crate) fn epub(
        chapter: &str,
        xhtml: &str,
        source: &Source,
        explicit: bool,
        styles: &Snapshot,
    ) -> Option<Self> {
        let font = styles.footnote_size().unwrap_or(FootnoteSize::Publisher);
        let mut roots = BTreeMap::new();
        let mut required = BTreeSet::new();
        if explicit {
            let document = roxmltree::Document::parse_with_options(
                xhtml,
                roxmltree::ParsingOptions {
                    allow_dtd: true,
                    ..Default::default()
                },
            )
            .ok()?;
            for node in document.descendants().filter(roxmltree::Node::is_element) {
                if !super::note_container(node) {
                    continue;
                }
                let id = node
                    .attribute("id")
                    .or_else(|| node.attribute((super::XML, "id")))?;
                let owner = source.fragment_owner(id)?;
                if !required.insert(owner) || required.len() > MAX_NOTES {
                    return None;
                }
                roots.insert(
                    owner,
                    NoteSize::Epub {
                        font,
                        clear_margins: false,
                    },
                );
            }
        }
        if styles.classic_epub_notes() {
            for (index, element) in source.elements.iter().enumerate() {
                if !element.selector_only
                    && !element.anonymous
                    && note_block(&element.tag)
                    && element
                        .classes
                        .split_ascii_whitespace()
                        .any(|class| CLASSIC_NOTE_CLASSES.contains(&class))
                {
                    roots.insert(
                        index,
                        NoteSize::Epub {
                            font,
                            clear_margins: true,
                        },
                    );
                }
            }
        }
        if styles.wikipedia_epub_notes() {
            for (index, element) in source.elements.iter().enumerate() {
                if !element.selector_only
                    && !element.anonymous
                    && element.tag == "li"
                    && element.parent.is_some_and(|parent| {
                        let parent = &source.elements[parent];
                        parent.tag == "ol"
                            && parent
                                .classes
                                .split_ascii_whitespace()
                                .any(|c| c == "references")
                    })
                {
                    roots.insert(index, NoteSize::Wikipedia(font));
                }
            }
        }
        Self::build(chapter, source, &roots, &required, styles, false)
    }

    pub(crate) fn fb2(
        chapter: &str,
        source: &Source,
        sections: &BTreeMap<String, Fb2NoteKind>,
        styles: &Snapshot,
    ) -> Option<Self> {
        let roots = sections
            .iter()
            .filter_map(|(id, &kind)| {
                let scale = styles.fb2_note_scale(kind)?;
                Some((source.fragment_owner(id)?, NoteSize::Fb2(scale)))
            })
            .collect();
        Self::build(chapter, source, &roots, &BTreeSet::new(), styles, false)
    }

    pub(crate) fn fb3(
        chapter: &str,
        source: &Source,
        styles: &Snapshot,
        incomplete_note_ids: &BTreeSet<String>,
    ) -> Option<Self> {
        // The FB3 importer preserves its notes as typed asides. CRE imports
        // these into FictionBook's notes body and applies the footnote preset.
        let scale = styles.fb2_note_scale(Fb2NoteKind::Footnote)?;
        let mut roots = BTreeMap::new();
        for (index, element) in source.elements.iter().enumerate() {
            if element.tag == "aside"
                && element.type_attribute.as_deref() == Some("footnote")
                && let Some(id) = &element.id
                && !incomplete_note_ids.contains(id)
                && source.fragment_owner(id) == Some(index)
            {
                roots.insert(index, NoteSize::Fb2(scale));
            }
        }
        // Every imported notebody is an entry boundary, including split,
        // ambiguous and unmarked bodies that cannot themselves be copied.
        // Extension must never adopt such a body into the preceding note.
        Self::build(chapter, source, &roots, &BTreeSet::new(), styles, true)
    }

    /// Build typography metadata for a previously validated foreign FB3 note.
    /// The note's reference is in a different Source, so normal target
    /// discovery cannot establish its ownership here.
    pub(crate) fn detached_fb3(source: &Source, root: usize, styles: &Snapshot) -> Option<Self> {
        let scale = styles.fb2_note_scale(Fb2NoteKind::Footnote)?;
        if styles.note_extension() != NoteExtension::Off || styles.note_formatting().inline_all {
            return None;
        }
        if source.elements.get(root)?.tag != "aside" {
            return None;
        }
        let mut ancestry = Vec::with_capacity(source.elements.len());
        let mut starts_note_part = Vec::with_capacity(source.elements.len());
        for (index, element) in source.elements.iter().enumerate() {
            if element.parent.is_some_and(|parent| parent >= index) {
                return None;
            }
            let owner = if index == root {
                Some(0)
            } else {
                element.parent.and_then(|parent| ancestry[parent])
            };
            ancestry.push(owner);
            starts_note_part.push(index == root);
        }
        Some(Self {
            roots: BTreeMap::from([(root, 0)]),
            ids: BTreeMap::new(),
            ancestry,
            starts_note_part,
            sizes: vec![NoteSize::Fb2(scale)],
            formatting: styles.note_formatting(),
            wikipedia_backlinks: BTreeSet::new(),
            separate_nonlinear: false,
        })
    }

    fn build(
        chapter: &str,
        source: &Source,
        marks: &BTreeMap<usize, NoteSize>,
        required: &BTreeSet<usize>,
        styles: &Snapshot,
        fb3_note_boundaries: bool,
    ) -> Option<Self> {
        if marks.is_empty() {
            return None;
        }
        let extension = styles.note_extension();
        let mut ownership = Vec::<Option<usize>>::with_capacity(source.elements.len());
        let mut starts_note_part = Vec::with_capacity(source.elements.len());
        for (index, element) in source.elements.iter().enumerate() {
            let inherited = element.parent.and_then(|parent| ownership[parent]);
            let root = marks.contains_key(&index);
            if required.contains(&index) && inherited.is_some() {
                return None;
            }
            let continuation = if extension != NoteExtension::Off
                && inherited.is_none()
                && !root
                && !element.selector_only
                && !element.anonymous
                && note_block(&element.tag)
                && !(fb3_note_boundaries
                    && element.tag == "aside"
                    && element.type_attribute.as_deref() == Some("footnote"))
                && !(extension == NoteExtension::UntilHeading && heading(&element.tag))
            {
                element
                    .previous_sibling
                    .filter(|&sibling| starts_note_part[sibling])
                    .and_then(|sibling| ownership[sibling])
            } else {
                None
            };
            ownership.push(inherited.or(root.then_some(index)).or(continuation));
            starts_note_part.push(inherited.is_none() && (root || continuation.is_some()));
        }

        let mut links = BTreeMap::new();
        let mut referenced = BTreeSet::new();
        for link in source.blocks.iter().flat_map(|block| &block.links) {
            if let Ok(Destination {
                path,
                fragment: Some(id),
            }) = Book::resolve_link(chapter, &link.href)
                && path == chapter
                && let Some(owner) = source
                    .fragment_owner(&id)
                    .and_then(|element| ownership[element])
            {
                referenced.insert(owner);
                links.insert(id, owner);
                if referenced.len() > MAX_NOTES || links.len() > MAX_REFERENCES {
                    return None;
                }
            }
        }
        if referenced.is_empty() || !required.is_subset(&referenced) {
            return None;
        }
        let mut sizes = Vec::with_capacity(referenced.len());
        let roots: BTreeMap<_, _> = referenced
            .into_iter()
            .enumerate()
            .map(|(index, root)| {
                sizes.push(marks[&root]);
                (root, index)
            })
            .collect();
        let ids = links
            .into_iter()
            .map(|(id, root)| (id, roots[&root]))
            .collect();
        let ancestry = ownership
            .into_iter()
            .map(|root| root.and_then(|root| roots.get(&root).copied()))
            .collect();
        let wikipedia_backlinks = if styles.wikipedia_epub_notes() {
            source
                .elements
                .iter()
                .enumerate()
                .filter_map(|(index, element)| {
                    let root = *roots.get(&element.parent?)?;
                    let hidden_class = element
                        .classes
                        .split_ascii_whitespace()
                        .any(|c| matches!(c, "noprint" | "mw-cite-backlink"));
                    (matches!(sizes[root], NoteSize::Wikipedia(_)) && hidden_class).then_some(index)
                })
                .collect()
        } else {
            BTreeSet::new()
        };
        Some(Self {
            roots,
            ids,
            ancestry,
            starts_note_part,
            sizes,
            formatting: styles.note_formatting(),
            wikipedia_backlinks,
            separate_nonlinear: styles.separates_nonlinear_notes(),
        })
    }

    fn canonical_groups(&self, source: &Source) -> Vec<Option<usize>> {
        let mut groups = vec![None; self.sizes.len()];
        for (&root, &note) in &self.roots {
            if matches!(self.sizes[note], NoteSize::Fb2(_)) {
                continue;
            }
            let previous = source.elements[root]
                .previous_sibling
                .filter(|&sibling| self.starts_note_part[sibling])
                .and_then(|sibling| self.ancestry[sibling]);
            groups[note] = previous.and_then(|note| groups[note]).or(Some(note));
        }
        groups
    }

    pub(crate) fn has_wikipedia_notes(&self) -> bool {
        self.sizes
            .iter()
            .any(|size| matches!(size, NoteSize::Wikipedia(_)))
    }

    pub(crate) fn hides_wikipedia_backlink(&self, index: usize) -> bool {
        self.wikipedia_backlinks.contains(&index)
    }

    pub(crate) fn font_scale(&self, index: usize, block: bool) -> Option<f32> {
        let id = self.ancestry[index]?;
        match self.sizes[id] {
            NoteSize::Epub { font, .. } | NoteSize::Wikipedia(font) => {
                font.scale(self.starts_note_part[index], block)
            }
            NoteSize::Fb2(size) => self.roots.contains_key(&index).then_some(size),
        }
    }

    pub(crate) fn formatting(&self, index: usize) -> Option<(NoteFormatting, bool)> {
        let id = self.ancestry[index]?;
        let mut formatting = self.formatting;
        let marginless = self.starts_note_part[index];
        formatting.regularize &= !marginless;
        let marginless = marginless
            && (!self.roots.contains_key(&index)
                || matches!(
                    self.sizes[id],
                    NoteSize::Epub {
                        clear_margins: true,
                        ..
                    } | NoteSize::Wikipedia(_)
                ));
        Some((formatting, marginless))
    }
}

fn note_block(tag: &str) -> bool {
    crate::source::is_block_tag(tag) || matches!(tag, "ol" | "ul" | "dl")
}

fn heading(tag: &str) -> bool {
    matches!(tag, "h1" | "h2" | "h3" | "h4" | "h5" | "h6")
}

fn discover(
    chapter: &str,
    targets: &Targets,
    source: &Source,
    lines: &[Line],
    table_extents: &HashMap<TableKey, TableCopy>,
) -> Option<(Vec<Range<usize>>, References, Vec<LineGroup>)> {
    let blocks: BTreeMap<_, _> = source
        .blocks
        .iter()
        .map(|block| (block.key, block))
        .collect();
    let mut regions = vec![None::<Range<usize>>; targets.roots.len()];
    let mut block_lines = BTreeMap::<_, Vec<_>>::new();
    let mut tables = HashMap::<TableKey, (Option<usize>, LineGroup)>::new();
    for (index, line) in lines.iter().enumerate() {
        let block = blocks.get(&line.block)?;
        let owner = targets.ancestry[block.element];
        if let Some(cell) = block.table_cell {
            match table_extents.get(&cell.table)? {
                TableCopy::Grid(extent) => {
                    let (table_owner, table) = tables.entry(cell.table).or_insert_with(|| {
                        (
                            owner,
                            LineGroup {
                                lines: index..index,
                                extent: extent.clone(),
                                rtl_lines: 0,
                            },
                        )
                    });
                    if *table_owner != owner || table.lines.end != index {
                        return None;
                    }
                    table.lines.end += 1;
                }
                TableCopy::Inline => {
                    owner?;
                }
            }
        }
        if let Some(id) = owner {
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
    let mut tables: Vec<_> = tables.into_values().map(|(_, group)| group).collect();
    tables.sort_unstable_by_key(|table| table.lines.start);
    (used.len() == targets.roots.len()).then_some((regions, references, tables))
}

#[derive(Default)]
struct Draft {
    body: Option<Range<f32>>,
    notes: Vec<Range<f32>>,
    note_height: u32,
    note_lines: usize,
    rtl_note_lines: usize,
    seen: BTreeSet<usize>,
    canonical_note_group: Option<usize>,
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
    fn flush_notes(
        &mut self,
        pending: &mut VecDeque<usize>,
        groups: &[LineGroup],
        notes: &[Range<usize>],
    ) -> Option<()> {
        for id in pending.drain(..) {
            if !self.add_note(id, &groups[notes[id].clone()])? {
                self.finish()?;
                if !self.add_note(id, &groups[notes[id].clone()])? {
                    return None;
                }
            }
        }
        Some(())
    }

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
            for group in &groups[..count] {
                self.draft.note_lines += group.lines.len();
                self.draft.rtl_note_lines += group.rtl_lines;
            }
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
        self.pages.push(Page {
            parts,
            separator,
            separator_on_right: draft.rtl_note_lines > draft.note_lines / 2,
            canonical_note_group: draft.canonical_note_group,
        });
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

    fn advanced_note_layout(chapter: usize, presets: &[&str]) -> Probe {
        advanced_note_layout_at(chapter, presets, settings(false))
    }

    fn advanced_note_layout_at(chapter: usize, presets: &[&str], settings: Settings) -> Probe {
        let mut data = crate::user_styles::Data::default();
        for preset in presets {
            data.choose(1, preset, Some(true)).unwrap();
        }
        Probe::layout_opened_book(
            &mut Some(Book::open(&test_fixture("in-page-footnotes.epub")).unwrap()),
            Engine::Parley,
            crate::PublisherRenderMode::Book.apply(settings),
            chapter,
            None,
            &Default::default(),
            &data.snapshot(1).unwrap(),
            || crate::font_context(&[test_fixture("fonts/NotoSans-Regular.ttf")]),
        )
        .unwrap()
    }

    fn phrase_rects(
        probe: &Probe,
        page: usize,
        phrase: &str,
    ) -> Vec<crate::selection::HighlightRect> {
        let start = probe.matches(phrase).remove(0);
        let end = probe.anchor(start.block, start.offset + phrase.len());
        probe.selection_rects(page, &probe.select_range(&start, &end).unwrap())
    }

    #[test]
    fn whole_note_inline_tables_keep_the_grid_or_shape_one_painted_row() {
        use crate::user_styles::{EPUB_FOOTNOTES, INLINE_ALL_FOOTNOTE_CONTENT};
        let mut band = advanced_note_layout(24, &[EPUB_FOOTNOTES]);
        let mut inline = advanced_note_layout(24, &[EPUB_FOOTNOTES, INLINE_ALL_FOOTNOTE_CONTENT]);
        let reference = band.fragment("grid-ref").unwrap();
        let first = band.fragment("grid-first").unwrap();
        let original = band.page_for(&first).unwrap();
        let page = band.page_for(&reference).unwrap();
        assert!(original > page);
        for (probe, flattened) in [(&band, false), (&inline, true)] {
            let pages = probe.in_page_notes.as_ref().unwrap();
            assert!(pages[page].separator.is_some());
            assert_eq!(probe.page_for(&first), Some(original));
            let crate::Renderer::Parley(layout) = &probe.renderer else {
                unreachable!()
            };
            assert_eq!(layout.in_page_table_extents.len(), 1);
            let table = layout.in_page_table_extents.values().next().unwrap();
            if let TableCopy::Grid(extent) = table {
                assert!(!flattened);
                for current in [page, original] {
                    assert!(
                        pages[current]
                            .parts
                            .iter()
                            .any(|part| part.source.start <= extent.start
                                && extent.end <= part.source.end)
                    );
                }
            } else {
                assert!(flattened);
            }
            for current in [page, original] {
                let names = ["Shoal ledger", "Deep chart", "Ferry count"];
                let cells = names.map(|text| {
                    let rects = phrase_rects(probe, current, text);
                    assert_eq!(rects.len(), 1, "{text}");
                    rects[0]
                });
                assert_eq!(cells[0].y, cells[1].y);
                assert!(cells[0].x < cells[1].x);
                if flattened {
                    assert_eq!(cells[1].y, cells[2].y);
                    assert!(cells[1].x < cells[2].x);
                } else {
                    assert!(cells[1].y < cells[2].y);
                    assert_eq!(cells[0].x, cells[2].x);
                }
                let x = (cells[0].x + 4) as f32;
                let y = (cells[0].y + cells[0].height / 2) as f32;
                assert_eq!(probe.word_at(current, x, y).unwrap().text, "Shoal");
                assert_eq!(
                    probe.link_at(current, x, y).unwrap().fragment.as_deref(),
                    Some("grid-ref")
                );
            }
        }
        assert_eq!(band.coverage(), inline.coverage());
        for phrase in ["Shoal ledger", "Deep chart", "Ferry count", "Grid ends."] {
            assert_eq!(band.matches(phrase), inline.matches(phrase));
        }
        let last = band.matches("Ferry count").remove(0);
        let end = band.anchor(last.block, last.offset + "Ferry count".len());
        assert_eq!(
            inline.select_range(&first, &end).unwrap().text,
            "Shoal ledger\nDeep chart\nFerry count"
        );
        for (probe, current, file) in [
            (&mut band, page, "reader-note-inline-table-before.png"),
            (&mut inline, page, "reader-note-inline-table-after.png"),
        ] {
            let frame = capture(probe, current, file);
            assert!(frame.contains(&0xdd));
            assert!(
                frame
                    .chunks_exact(probe.settings.width as usize)
                    .any(|row| {
                        row.windows(150)
                            .any(|run| run.iter().all(|&gray| gray == 0))
                    })
            );
        }
        capture(
            &mut inline,
            original,
            "reader-note-inline-table-original.png",
        );
    }

    #[test]
    fn whole_note_inline_tables_restore_the_grid_when_the_compact_row_is_too_wide() {
        use crate::user_styles::{EPUB_FOOTNOTES, INLINE_ALL_FOOTNOTE_CONTENT};
        let narrow = Settings {
            width: 318,
            ..settings(false)
        };
        let mut grid = advanced_note_layout_at(24, &[EPUB_FOOTNOTES], narrow);
        let mut inline =
            advanced_note_layout_at(24, &[EPUB_FOOTNOTES, INLINE_ALL_FOOTNOTE_CONTENT], narrow);
        let page = grid.page_for(&grid.fragment("grid-ref").unwrap()).unwrap();
        assert!(
            grid.in_page_notes.as_ref().unwrap()[page]
                .separator
                .is_some()
        );
        assert_eq!(grid.capture(page).unwrap(), inline.capture(page).unwrap());
        assert_eq!(grid.matches("Shoal ledger"), inline.matches("Shoal ledger"));
    }

    #[test]
    fn in_page_body_table_moves_as_a_grid_and_keeps_notes_and_saved_cell_positions() {
        use crate::user_styles::{EPUB_FOOTNOTES, INLINE_ALL_FOOTNOTE_CONTENT};
        let viewport = Settings {
            height: 520,
            ..settings(false)
        };
        let mut ordinary = advanced_note_layout_at(25, &[], viewport);
        let mut composed = advanced_note_layout_at(25, &[EPUB_FOOTNOTES], viewport);
        let mut inline =
            advanced_note_layout_at(25, &[EPUB_FOOTNOTES, INLINE_ALL_FOOTNOTE_CONTENT], viewport);
        let reference = ordinary.fragment("body-grid-ref").unwrap();
        let table = ordinary.fragment("body-grid-well").unwrap();
        let saved: crate::Anchor =
            serde_json::from_slice(&serde_json::to_vec(&table).unwrap()).unwrap();
        let page = ordinary.page_for(&reference).unwrap();
        assert_eq!(ordinary.page_for(&saved), Some(page));
        let original_table = phrase_rects(&ordinary, page, "Well list")[0];

        for probe in [&composed, &inline] {
            let pages = probe.in_page_notes.as_ref().unwrap();
            let table_page = probe.page_for(&saved).unwrap();
            assert_eq!(probe.page_for(&reference), Some(page));
            assert!(table_page > page);
            assert!(pages[page].separator.is_some() && pages[table_page].separator.is_none());
            assert_eq!(probe.fragment("body-grid-well"), Some(saved.clone()));
            assert_eq!(probe.coverage(), ordinary.coverage());
            assert!(phrase_rects(probe, page, "Well list").is_empty());
            let aside = phrase_rects(probe, page, "The gauge was calibrated")[0];
            assert!(
                aside.y > (pages[page].separator.unwrap() + probe.settings.top_margin()) as i32
            );
            let note = probe.fragment("body-grid-gauge").unwrap();
            assert_eq!(ordinary.fragment("body-grid-gauge"), Some(note.clone()));
            assert!(probe.page_for(&note).unwrap() > table_page);
            let band_back = phrase_rects(probe, page, "Return to the census")[0];
            assert_eq!(
                probe
                    .link_at(
                        page,
                        (band_back.x + 4) as f32,
                        (band_back.y + band_back.height / 2) as f32
                    )
                    .unwrap()
                    .fragment
                    .as_deref(),
                Some("body-grid-ref")
            );

            let cells = [
                "Well list",
                "Eastern inlet",
                "Red beacon",
                "Gate stays open",
            ]
            .map(|phrase| {
                assert_eq!(probe.matches(phrase), ordinary.matches(phrase));
                let rects = phrase_rects(probe, table_page, phrase);
                assert_eq!(rects.len(), 1);
                rects[0]
            });
            assert_eq!(cells[0].x, original_table.x);
            assert_eq!(cells[0].y, cells[1].y);
            assert!(cells[0].x < cells[1].x && cells[1].y < cells[2].y);
            assert_eq!(cells[1].x, cells[2].x);
            assert_eq!(cells[0].x, cells[3].x);
            assert!(cells[2].y < cells[3].y);
            let x = (cells[0].x + 4) as f32;
            let y = (cells[0].y + cells[0].height / 2) as f32;
            assert_eq!(probe.word_at(table_page, x, y).unwrap().start, saved);
            assert_eq!(
                probe.link_at(table_page, x, y).unwrap().fragment.as_deref(),
                Some("body-grid-target")
            );
            let crate::Renderer::Parley(layout) = &probe.renderer else {
                unreachable!()
            };
            let Some(TableCopy::Grid(extent)) = layout.in_page_table_extents.values().next() else {
                panic!("the table stays a native grid")
            };
            assert_eq!(layout.in_page_table_extents.len(), 1);
            assert!(
                pages[table_page]
                    .parts
                    .iter()
                    .any(|part| part.source.start <= extent.start && extent.end <= part.source.end)
            );
        }

        let table_page = composed.page_for(&saved).unwrap();
        let reopened = advanced_note_layout_at(25, &[EPUB_FOOTNOTES], viewport);
        assert_eq!(reopened.page_for(&saved), Some(table_page));
        assert_eq!(reopened.matches("Well list"), vec![saved]);
        let before = capture(&mut ordinary, page, "reader-body-table-notes-off.png");
        let band = capture(&mut composed, page, "reader-body-table-notes-band.png");
        let grid = capture(
            &mut composed,
            table_page,
            "reader-body-table-notes-grid.png",
        );
        assert_eq!(
            grid,
            inline.capture(inline.page_for(&table).unwrap()).unwrap()
        );
        let has_fill = |frame: &[u8], gray| {
            frame.chunks_exact(viewport.width as usize).any(|row| {
                row.windows(24)
                    .any(|run| run.iter().all(|&pixel| pixel == gray))
            })
        };
        assert!(!has_fill(&band, 0xdd));
        let table_raster = |frame: &[u8]| {
            assert!(has_fill(frame, 0xdd) && has_fill(frame, 0xee));
            let stride = viewport.width as usize;
            let mut rules = frame
                .chunks_exact(stride)
                .enumerate()
                .filter_map(|(y, row)| {
                    let left = row
                        .windows(150)
                        .position(|run| run.iter().all(|&ink| ink == 0))?;
                    let right = left + row[left..].iter().take_while(|&&ink| ink == 0).count();
                    Some((y, left, right))
                });
            let (top, left, right) = rules.next().unwrap();
            let (last, _, _) = rules
                .filter(|&(_, a, b)| (a, b) == (left, right))
                .last()
                .unwrap();
            let pixels: Vec<_> = (top..=last)
                .flat_map(|y| &frame[y * stride + left..y * stride + right])
                .copied()
                .collect();
            (left, right - left, last + 1 - top, pixels)
        };
        assert_eq!(table_raster(&before), table_raster(&grid));
    }

    #[test]
    fn in_page_body_table_falls_back_when_the_grid_exceeds_one_reading_page() {
        use crate::user_styles::{EPUB_FOOTNOTES, INLINE_ALL_FOOTNOTE_CONTENT};
        let viewport = Settings {
            height: 154,
            ..settings(false)
        };
        let mut first = advanced_note_layout_at(25, &[], viewport);
        let mut second =
            advanced_note_layout_at(25, &[EPUB_FOOTNOTES, INLINE_ALL_FOOTNOTE_CONTENT], viewport);
        assert!(second.in_page_notes.is_none());
        let table = first.fragment("body-grid-well").unwrap();
        assert_eq!(second.fragment("body-grid-well"), Some(table.clone()));
        let grid_page = first.page_for(&table).unwrap();
        assert_eq!(second.page_for(&table), Some(grid_page));
        assert_eq!(
            first.capture(grid_page).unwrap(),
            second.capture(grid_page).unwrap()
        );
    }

    #[test]
    fn whole_note_inline_keeps_multiple_canonical_blocks_on_one_visual_line() {
        use crate::user_styles::{EPUB_FOOTNOTES, INLINE_ALL_FOOTNOTE_CONTENT};
        let mut before = advanced_note_layout(12, &[EPUB_FOOTNOTES]);
        let mut joined = advanced_note_layout(12, &[EPUB_FOOTNOTES, INLINE_ALL_FOOTNOTE_CONTENT]);
        let reference = before.fragment("inline-ref").unwrap();
        let page = before.page_for(&reference).unwrap();
        assert_eq!(joined.page_for(&reference), Some(page));
        assert!(
            before.in_page_notes.as_ref().unwrap()[page]
                .separator
                .is_some()
        );
        assert!(
            joined.in_page_notes.as_ref().unwrap()[page]
                .separator
                .is_some()
        );

        let number = joined.fragment("inline-number").unwrap();
        let body = joined.fragment("inline-body").unwrap();
        assert_ne!(number.block, body.block);
        assert_eq!(before.fragment("inline-body"), Some(body.clone()));
        let original = joined.page_for(&body).unwrap();
        assert!(original > page);
        assert_eq!(joined.page_for(&number), Some(original));
        assert_eq!(joined.coverage(), before.coverage());
        assert!(joined.lines.iter().any(|line| {
            let blocks: Vec<_> = line.sources().map(|(key, _)| key).collect();
            blocks.contains(&number.block) && blocks.contains(&body.block)
        }));

        for current in [page, original] {
            let a = phrase_rects(&joined, current, "[7]")[0];
            let b = phrase_rects(&joined, current, "signal lamp")[0];
            assert_eq!(a.y, b.y);
            assert!(a.x < b.x);
            let word = joined
                .word_at(current, (b.x + 5) as f32, (b.y + b.height / 2) as f32)
                .unwrap();
            assert_eq!(word.start.block, body.block);
            assert_eq!(word.text, "signal");
            let link = phrase_rects(&joined, current, "Return")[0];
            assert_eq!(
                joined
                    .link_at(
                        current,
                        (link.x + 5) as f32,
                        (link.y + link.height / 2) as f32
                    )
                    .unwrap()
                    .fragment
                    .as_deref(),
                Some("inline-ref")
            );
            assert!(
                joined
                    .links_on_page(current)
                    .iter()
                    .any(|(label, _)| label == "Return to the chart")
            );
        }
        let original_ref = phrase_rects(&before, page, "signal lamp")[0];
        let old_number = phrase_rects(&before, page, "[7]")[0];
        assert!(original_ref.y > old_number.y);
        let selection = joined
            .select_range(&number, &joined.anchor(body.block, 10))
            .unwrap();
        assert_eq!(selection.text, "[7]\nThe signal");
        let selected = joined.selection_rects(page, &selection);
        assert!(selected.len() >= 2 && selected.iter().all(|part| part.y == selected[0].y));
        assert!(
            capture(&mut before, page, "reader-note-inline-before.png")
                != capture(&mut joined, page, "reader-note-inline-after.png")
        );
        capture(&mut joined, original, "reader-note-inline-original.png");
    }

    #[test]
    fn whole_note_inline_flattens_text_list_items_and_preserves_backlinks() {
        use crate::user_styles::{EPUB_FOOTNOTES, INLINE_ALL_FOOTNOTE_CONTENT};
        let mut before = advanced_note_layout(13, &[EPUB_FOOTNOTES]);
        let mut joined = advanced_note_layout(13, &[EPUB_FOOTNOTES, INLINE_ALL_FOOTNOTE_CONTENT]);
        let reference = before.fragment("inventory-ref").unwrap();
        let page = before.page_for(&reference).unwrap();
        let intro = before.fragment("inventory-intro").unwrap();
        let original = before.page_for(&intro).unwrap();
        assert!(original > page);
        assert_eq!(joined.page_for(&reference), Some(page));
        assert_eq!(
            phrase_rects(&before, page, "The final ledger"),
            phrase_rects(&joined, page, "The final ledger")
        );
        assert_eq!(joined.coverage(), before.coverage());
        assert!(
            before.in_page_notes.as_ref().unwrap()[page]
                .separator
                .is_some()
        );
        assert!(
            joined.in_page_notes.as_ref().unwrap()[page]
                .separator
                .is_some()
        );
        let first = joined.fragment("inventory-first").unwrap();
        let second = joined.fragment("inventory-second").unwrap();
        assert_ne!(first.block, second.block);
        assert_eq!(joined.matches("Spare wick"), before.matches("Spare wick"));
        for (id, anchor) in [("inventory-first", &first), ("inventory-second", &second)] {
            assert_eq!(before.fragment(id).as_ref(), Some(anchor));
            assert_eq!(joined.page_for(anchor), Some(original));
        }
        assert!(joined.lines.iter().any(|line| {
            let sources: Vec<_> = line.sources().map(|(key, _)| key).collect();
            [intro.block, first.block, second.block]
                .iter()
                .all(|key| sources.contains(key))
        }));

        let old_intro = phrase_rects(&before, page, "Inventory number eight.")[0];
        let old_first = phrase_rects(&before, page, "North lantern.")[0];
        let old_second = phrase_rects(&before, page, "Spare wick")[0];
        assert!(old_intro.y < old_first.y && old_first.y < old_second.y);
        for current in [page, original] {
            let intro = phrase_rects(&joined, current, "Inventory number eight.")[0];
            let first = phrase_rects(&joined, current, "North lantern.")[0];
            let second = phrase_rects(&joined, current, "Spare wick")[0];
            assert!(intro.y == first.y && first.y == second.y);
            assert!(intro.x < first.x && first.x < second.x);
            let x = (second.x + 6) as f32;
            let y = (second.y + second.height / 2) as f32;
            assert_eq!(joined.word_at(current, x, y).unwrap().text, "Spare");
            assert_eq!(
                joined.link_at(current, x, y).unwrap().fragment.as_deref(),
                Some("inventory-ref")
            );
        }
        let range = joined
            .select_range(&first, &joined.anchor(second.block, "Spare wick.".len()))
            .unwrap();
        assert_eq!(range.text, "North lantern.\nSpare wick.");
        let selection = joined.selection_rects(page, &range);
        assert!(selection.len() >= 2 && selection.iter().all(|rect| rect.y == selection[0].y));
        capture(&mut before, page, "reader-note-inline-list-before.png");
        capture(&mut joined, page, "reader-note-inline-list-after.png");
        capture(
            &mut joined,
            original,
            "reader-note-inline-list-original.png",
        );
    }

    #[test]
    fn whole_note_inline_flattens_nested_and_multiblock_list_items() {
        use crate::user_styles::{EPUB_FOOTNOTES, INLINE_ALL_FOOTNOTE_CONTENT};
        let mut before = advanced_note_layout(15, &[EPUB_FOOTNOTES]);
        let mut joined = advanced_note_layout(15, &[EPUB_FOOTNOTES, INLINE_ALL_FOOTNOTE_CONTENT]);
        let reference = before.fragment("nested-ref").unwrap();
        let page = before.page_for(&reference).unwrap();
        let anchors = [
            ("nested-intro", "Drawer notes."),
            ("nested-parent", "Upper shelf."),
            ("nested-detail", "Oak lip."),
            ("nested-child", "Tin key"),
            ("nested-next", "Low bin."),
        ]
        .map(|(id, phrase)| {
            let anchor = before.fragment(id).unwrap();
            assert_eq!(joined.fragment(id).as_ref(), Some(&anchor));
            assert_eq!(before.matches(phrase), joined.matches(phrase));
            (anchor, phrase)
        });
        let original = before.page_for(&anchors[0].0).unwrap();
        assert!(original > page);
        assert_eq!(joined.page_for(&reference), Some(page));
        assert_eq!(before.coverage(), joined.coverage());
        assert!(anchors.iter().all(|(anchor, _)| {
            before.page_for(anchor) == Some(original) && joined.page_for(anchor) == Some(original)
        }));
        assert!(
            before.in_page_notes.as_ref().unwrap()[page]
                .separator
                .is_some()
        );
        assert!(
            joined.in_page_notes.as_ref().unwrap()[page]
                .separator
                .is_some()
        );
        let ordinary = anchors
            .each_ref()
            .map(|(_, phrase)| phrase_rects(&before, page, phrase)[0]);
        assert!(ordinary.windows(2).all(|pair| pair[0].y < pair[1].y));
        for current in [page, original] {
            let combined = anchors
                .each_ref()
                .map(|(_, phrase)| phrase_rects(&joined, current, phrase)[0]);
            assert!(
                combined
                    .windows(2)
                    .all(|pair| pair[0].y == pair[1].y && pair[0].x < pair[1].x)
            );
            let link = combined[3];
            let x = (link.x + 5) as f32;
            let y = (link.y + link.height / 2) as f32;
            let word = joined.word_at(current, x, y).unwrap();
            assert_eq!(word.text, "Tin");
            assert_eq!(word.start.block, anchors[3].0.block);
            assert_eq!(
                joined.link_at(current, x, y).unwrap().fragment.as_deref(),
                Some("nested-ref")
            );
        }
        let selection = joined
            .select_range(
                &anchors[1].0,
                &joined.anchor(anchors[3].0.block, "Tin key".len()),
            )
            .unwrap();
        assert_eq!(selection.text, "Upper shelf.\nOak lip.\nTin key");
        let rects = joined.selection_rects(page, &selection);
        assert!(rects.len() >= 3 && rects.iter().all(|part| part.y == rects[0].y));
        assert_eq!(
            phrase_rects(&before, page, "The entry lists the compartments"),
            phrase_rects(&joined, page, "The entry lists the compartments")
        );
        capture(&mut before, page, "reader-note-inline-nested-before.png");
        capture(&mut joined, page, "reader-note-inline-nested-after.png");
        capture(
            &mut joined,
            original,
            "reader-note-inline-nested-original.png",
        );
    }

    #[test]
    fn whole_note_inline_keeps_illustration_pixels_links_and_source_offsets() {
        use crate::user_styles::{EPUB_FOOTNOTES, INLINE_ALL_FOOTNOTE_CONTENT};
        let mut before = advanced_note_layout(16, &[EPUB_FOOTNOTES]);
        let mut joined = advanced_note_layout(16, &[EPUB_FOOTNOTES, INLINE_ALL_FOOTNOTE_CONTENT]);
        let reference = before.fragment("media-ref").unwrap();
        let page = before.page_for(&reference).unwrap();
        let media = before.fragment("media-text").unwrap();
        let original = before.page_for(&media).unwrap();
        assert!(original > page);
        for probe in [&before, &joined] {
            assert!(
                probe.in_page_notes.as_ref().unwrap()[page]
                    .separator
                    .is_some()
            );
            assert_eq!(probe.page_for(&reference), Some(page));
            assert_eq!(probe.page_for(&media), Some(original));
            let mut illustration = probe.anchor(media.block, 0);
            illustration.illustration = Some(0);
            assert_eq!(probe.page_for(&illustration), Some(original));
        }
        assert_eq!(joined.fragment("media-text"), Some(media.clone()));
        assert_eq!(
            before.fragment("media-chart"),
            joined.fragment("media-chart")
        );
        assert_eq!(before.matches("shelf depth"), joined.matches("shelf depth"));
        assert_eq!(before.coverage(), joined.coverage());
        let old_intro = phrase_rects(&before, page, "Cabinet diagram legend.")[0];
        let old_caption = phrase_rects(&before, page, "for shelf depth.")[0];
        assert!(old_intro.y < old_caption.y);
        for current in [page, original] {
            let a = phrase_rects(&joined, current, "Cabinet diagram legend.")[0];
            let b = phrase_rects(&joined, current, "for shelf depth.")[0];
            assert_eq!(a.y, b.y);
            assert!(a.x < b.x);
            let key = phrase_rects(&joined, current, "shelf")[0];
            let selected = joined
                .word_at(current, (key.x + 5) as f32, (key.y + key.height / 2) as f32)
                .unwrap();
            assert_eq!(selected.text, "shelf");
            assert_eq!(selected.start.block, media.block);
        }

        let mut artwork = Vec::new();
        for (probe, current, name) in [
            (&mut before, page, "reader-note-inline-image-before.png"),
            (&mut joined, page, "reader-note-inline-image-after.png"),
        ] {
            let pixels = capture(probe, current, name);
            artwork.push(
                note_artwork(
                    probe,
                    current,
                    &pixels,
                    (56, 36),
                    "media-key",
                    "Shelf diagram",
                )
                .0,
            );
        }
        let pixels = capture(
            &mut joined,
            original,
            "reader-note-inline-image-original.png",
        );
        artwork.push(
            note_artwork(
                &joined,
                original,
                &pixels,
                (56, 36),
                "media-key",
                "Shelf diagram",
            )
            .0,
        );
        assert!(artwork[0].iter().any(|&pixel| pixel < 200));
        assert!(artwork.windows(2).all(|pair| pair[0] == pair[1]));
    }

    fn note_artwork(
        probe: &Probe,
        page: usize,
        pixels: &[u8],
        size: (usize, usize),
        fragment: &str,
        label: &str,
    ) -> (Vec<u8>, Range<usize>, Range<usize>) {
        let crate::Renderer::Parley(layout) = &probe.renderer else {
            unreachable!();
        };
        let (columns, rows) = probe
            .page_parts(page)
            .find_map(|(range, offset)| {
                layout
                    .visible_illustrations(range, probe.settings)
                    .find(|(path, columns, rows)| {
                        *path == "chart.png" && (columns.len(), rows.len()) == size
                    })
                    .map(|(_, columns, rows)| {
                        let offset = offset as usize;
                        (columns, (rows.start + offset)..(rows.end + offset))
                    })
            })
            .expect("the whole illustration is visible");
        let x = (columns.start + columns.len() / 2) as f32;
        let y = (rows.start + rows.len() / 2) as f32;
        assert_eq!(probe.illustration_at(page, x, y), Some("chart.png"));
        assert_eq!(
            probe.link_at(page, x, y).unwrap().fragment.as_deref(),
            Some(fragment)
        );
        assert!(probe.links_on_page(page).iter().any(|(text, target)| {
            text == label && target.fragment.as_deref() == Some(fragment)
        }));
        let width = probe.settings.width as usize;
        let crop = rows
            .clone()
            .flat_map(|row| &pixels[row * width + columns.start..row * width + columns.end])
            .copied()
            .collect();
        (crop, columns, rows)
    }

    #[test]
    fn whole_note_inline_moves_a_standalone_picture_between_its_original_text_blocks() {
        use crate::user_styles::{EPUB_FOOTNOTES, INLINE_ALL_FOOTNOTE_CONTENT};
        let mut ordinary = advanced_note_layout(17, &[EPUB_FOOTNOTES]);
        let mut joined = advanced_note_layout(17, &[EPUB_FOOTNOTES, INLINE_ALL_FOOTNOTE_CONTENT]);
        let reference = ordinary.fragment("isolated-ref").unwrap();
        let image = ordinary.fragment("isolated-picture").unwrap();
        let page = ordinary.page_for(&reference).unwrap();
        let original = ordinary.page_for(&image).unwrap();
        assert!(original > page);
        assert_eq!(ordinary.coverage(), joined.coverage());
        assert_eq!(joined.fragment("isolated-picture"), Some(image.clone()));
        let mut ordinal = image.clone();
        ordinal.illustration = Some(0);
        for probe in [&ordinary, &joined] {
            assert_eq!(probe.page_for(&reference), Some(page));
            assert_eq!(probe.page_for(&image), Some(original));
            assert_eq!(probe.anchor_top(&ordinal), probe.anchor_top(&image));
            assert!(
                probe.in_page_notes.as_ref().unwrap()[page]
                    .separator
                    .is_some()
            );
        }

        let mut crops = Vec::new();
        for (probe, current, name, flattened) in [
            (
                &mut ordinary,
                page,
                "reader-note-inline-block-image-before.png",
                false,
            ),
            (
                &mut joined,
                page,
                "reader-note-inline-block-image-after.png",
                true,
            ),
        ] {
            let pixels = capture(probe, current, name);
            let (art, columns, rows) = note_artwork(
                probe,
                current,
                &pixels,
                (108, 69),
                "isolated-start",
                "Hinge diagram",
            );
            let intro = phrase_rects(probe, current, "Measurements in the workshop.")[0];
            let after = phrase_rects(probe, current, "A hinge holds the door.")[0];
            if flattened {
                assert_eq!(intro.y, after.y);
                assert!((intro.x + intro.width) as usize <= columns.start);
                assert!(columns.end <= after.x as usize);
            } else {
                assert!((intro.y + intro.height) as usize <= rows.start);
                assert!(rows.end <= after.y as usize);
            }
            crops.push(art);
        }
        let canonical = capture(
            &mut joined,
            original,
            "reader-note-inline-block-image-original.png",
        );
        crops.push(
            note_artwork(
                &joined,
                original,
                &canonical,
                (108, 69),
                "isolated-start",
                "Hinge diagram",
            )
            .0,
        );
        assert!(crops[0].iter().any(|&pixel| pixel < 200));
        assert!(crops.windows(2).all(|pair| pair[0] == pair[1]));
    }

    #[test]
    fn whole_note_inline_keeps_multiple_picture_ordinals_from_one_source_block() {
        use crate::user_styles::{EPUB_FOOTNOTES, INLINE_ALL_FOOTNOTE_CONTENT};
        let mut ordinary = advanced_note_layout(18, &[EPUB_FOOTNOTES]);
        let mut joined = advanced_note_layout(18, &[EPUB_FOOTNOTES, INLINE_ALL_FOOTNOTE_CONTENT]);
        let image = ordinary.fragment("pair-pictures").unwrap();
        let mut second = image.clone();
        second.illustration = Some(1);
        let reference = ordinary.fragment("pair-start").unwrap();
        let reference_page = ordinary.page_for(&reference).unwrap();
        let page = ordinary.page_for(&image).unwrap();
        assert!(reference_page < page);
        for probe in [&mut ordinary, &mut joined] {
            assert_eq!(probe.fragment("pair-pictures"), Some(image.clone()));
            assert_eq!(probe.page_for(&image), Some(page));
            assert_eq!(probe.page_for(&second), Some(page));
            assert_ne!(probe.anchor_top(&image), probe.anchor_top(&second));
            for current in [reference_page, page] {
                let pixels = probe.capture(current).unwrap();
                let (front, _, _) = note_artwork(
                    probe,
                    current,
                    &pixels,
                    (42, 27),
                    "pair-start",
                    "Front handle",
                );
                let (back, _, _) =
                    note_artwork(probe, current, &pixels, (70, 45), "pair-key", "Back handle");
                assert!(front.iter().any(|&pixel| pixel < 200));
                assert!(back.iter().any(|&pixel| pixel < 200));
            }
        }
    }

    #[test]
    fn whole_note_inline_list_roots_keep_generated_markers_outside_canonical_text() {
        use crate::user_styles::{
            EPUB_FOOTNOTES, INLINE_ALL_FOOTNOTE_CONTENT, WIKIPEDIA_FOOTNOTES,
        };
        let cases = [
            (
                19,
                EPUB_FOOTNOTES,
                "epub",
                "7.",
                "The guide lists the latch",
                "root-epub-ref",
                "root-epub-first",
                "Latch record.",
                "The brass pin stayed.",
                "Return to latch",
            ),
            (
                20,
                WIKIPEDIA_FOOTNOTES,
                "wiki",
                "IX.",
                "The catalog lists the register",
                "cite_ref_register",
                "root-wiki-first",
                "Register note.",
                "The date is missing.",
                "Return to register",
            ),
        ];
        for (chapter, style, family, marker, body, reference, first, intro, tail, backlink) in cases
        {
            let mut before = advanced_note_layout(chapter, &[style]);
            let mut joined = advanced_note_layout(chapter, &[style, INLINE_ALL_FOOTNOTE_CONTENT]);
            let source = before.fragment(first).unwrap();
            let reference_anchor = before.fragment(reference).unwrap();
            let page = before.page_for(&reference_anchor).unwrap();
            let original = before.page_for(&source).unwrap();
            assert!(original > page);
            assert_eq!(joined.page_for(&reference_anchor), Some(page));
            assert_eq!(joined.page_for(&source), Some(original));
            assert_eq!(joined.fragment(first), Some(source.clone()));
            assert_eq!(before.coverage(), joined.coverage());
            assert_eq!(
                phrase_rects(&before, page, body),
                phrase_rects(&joined, page, body)
            );
            assert!(
                phrase_rects(&before, page, intro)[0].y < phrase_rects(&before, page, tail)[0].y
            );
            for current in [page, original] {
                let first = phrase_rects(&joined, current, intro)[0];
                let second = phrase_rects(&joined, current, tail)[0];
                assert!(first.y == second.y && first.x < second.x);
                let link = phrase_rects(&joined, current, backlink)[0];
                assert_eq!(
                    joined
                        .link_at(
                            current,
                            (link.x + 4) as f32,
                            (link.y + link.height / 2) as f32
                        )
                        .unwrap()
                        .fragment
                        .as_deref(),
                    Some(reference)
                );
            }
            let combined = joined_note_text(&joined, &source);
            assert_eq!(
                combined,
                format!("{marker}\u{a0}{intro} {tail} {backlink}.")
            );
            assert!(joined.matches(&format!("{marker} {intro}")).is_empty());
            assert!(joined.matches("Hidden register backlink").is_empty());
            let tail_anchor = joined.matches(tail).remove(0);
            let range = joined
                .select_range(
                    &source,
                    &joined.anchor(tail_anchor.block, tail_anchor.offset + tail.len()),
                )
                .unwrap();
            assert_eq!(range.text, format!("{intro}\n{tail}"));
            capture(
                &mut before,
                page,
                &format!("reader-note-inline-root-{family}-before.png"),
            );
            capture(
                &mut joined,
                page,
                &format!("reader-note-inline-root-{family}-after.png"),
            );
            capture(
                &mut joined,
                original,
                &format!("reader-note-inline-root-{family}-original.png"),
            );
        }
    }

    #[test]
    fn whole_note_inline_list_root_respects_none_and_explicit_ordinals() {
        use crate::user_styles::{EPUB_FOOTNOTES, INLINE_ALL_FOOTNOTE_CONTENT};
        let joined = advanced_note_layout(21, &[EPUB_FOOTNOTES, INLINE_ALL_FOOTNOTE_CONTENT]);
        let unnumbered = joined.fragment("root-none-first").unwrap();
        let solo = joined.fragment("root-solo-first").unwrap();
        assert_eq!(
            joined_note_text(&joined, &unnumbered),
            "Date unknown. The entry was undated."
        );
        assert_eq!(
            joined_note_text(&joined, &solo),
            "42.\u{a0}Only the afternoon is known."
        );
        assert!(joined.matches("42. Only").is_empty());
    }

    fn joined_note_text<'a>(probe: &'a Probe, anchor: &crate::Anchor) -> &'a str {
        let crate::Renderer::Parley(layout) = &probe.renderer else {
            unreachable!();
        };
        let page = probe.page_for(anchor).unwrap();
        let (bounds, _) = probe.page_parts(page).next().unwrap();
        layout
            .paragraphs_in(bounds)
            .filter_map(|paragraph| paragraph.joined_note.as_ref())
            .find(|note| note.pieces.iter().any(|piece| piece.block == anchor.block))
            .map(|note| note.text.as_str())
            .expect("the canonical paragraph joins the note layout")
    }

    #[test]
    fn whole_note_inline_requires_note_composition_and_keeps_a_separate_body_grid_ordinary() {
        use crate::user_styles::{EPUB_FOOTNOTES, INLINE_ALL_FOOTNOTE_CONTENT};
        let ordinary = advanced_note_layout(12, &[]);
        let disabled = advanced_note_layout(12, &[INLINE_ALL_FOOTNOTE_CONTENT]);
        assert!(disabled.in_page_notes.is_none());
        assert_eq!(disabled.lines.len(), ordinary.lines.len());
        assert!(
            disabled
                .lines
                .iter()
                .all(|line| line.additional_sources.is_none())
        );

        let ordinary = advanced_note_layout(14, &[EPUB_FOOTNOTES]);
        let option = advanced_note_layout(14, &[EPUB_FOOTNOTES, INLINE_ALL_FOOTNOTE_CONTENT]);
        let location = ordinary.fragment("fallback-start").unwrap();
        let page = ordinary.page_for(&location).unwrap();
        for probe in [&ordinary, &option] {
            assert!(
                probe.in_page_notes.as_ref().unwrap()[page]
                    .separator
                    .is_some()
            );
            assert_eq!(probe.matches("Lantern"), ordinary.matches("Lantern"));
        }
        assert!(
            option
                .lines
                .iter()
                .any(|line| line.additional_sources.is_some())
        );
    }

    #[test]
    fn advanced_note_cosmetic_spacing_reflows_boxes_and_preserves_source_hits() {
        use crate::user_styles::{EPUB_FOOTNOTES, NOTE_COSMETIC_SPACING};
        let mut plain = advanced_note_layout(9, &[EPUB_FOOTNOTES]);
        let mut fixed = advanced_note_layout(9, &[EPUB_FOOTNOTES, NOTE_COSMETIC_SPACING]);
        let off = advanced_note_layout(9, &[]);
        let no_notes = advanced_note_layout(9, &[NOTE_COSMETIC_SPACING]);
        let reference = plain.fragment("cosmetic-start").unwrap();
        let note = plain.fragment("cosmetic-note").unwrap();
        let page = plain.page_for(&reference).unwrap();
        assert_eq!(fixed.page_for(&reference), Some(page));
        assert_eq!(fixed.fragment("cosmetic-note"), Some(note.clone()));
        assert_eq!(fixed.matches("reads true"), plain.matches("reads true"));
        let x = |probe: &Probe, page| phrase_rects(probe, page, "reads true")[0].x;
        let before = x(&plain, page);
        let after = x(&fixed, page);
        assert!((86..=90).contains(&(before - after)), "{before} -> {after}");
        assert!(
            x(&plain, plain.page_for(&note).unwrap()) > x(&fixed, fixed.page_for(&note).unwrap())
        );
        assert_eq!(
            phrase_rects(&plain, page, "continues today"),
            phrase_rects(&fixed, page, "continues today")
        );
        assert_eq!(
            phrase_rects(&off, page, "continues today"),
            phrase_rects(&no_notes, page, "continues today")
        );
        assert!(no_notes.in_page_notes.is_none());
        for probe in [&plain, &fixed] {
            let box_ = phrase_rects(probe, page, "seal mark")[0];
            assert_eq!(
                probe
                    .link_at(
                        page,
                        (box_.x + box_.width / 2) as f32,
                        (box_.y + box_.height / 2) as f32
                    )
                    .and_then(|target| target.fragment),
                Some("cosmetic-ref".into())
            );
        }
        eprintln!("Reader cosmetic note spacing x={before} -> {after}");
        assert_ne!(
            capture(&mut plain, page, "reader-note-cosmetic-before.png"),
            capture(&mut fixed, page, "reader-note-cosmetic-after.png")
        );
    }

    #[test]
    fn advanced_note_wikipedia_owns_list_items_and_their_native_table() {
        use crate::user_styles::WIKIPEDIA_FOOTNOTES;
        let ordinary = advanced_note_layout(10, &[]);
        let mut probe = advanced_note_layout(10, &[WIKIPEDIA_FOOTNOTES]);
        let start = probe.fragment("wiki-start").unwrap();
        let page = probe.page_for(&start).unwrap();
        assert!(
            probe.in_page_notes.as_ref().unwrap()[page]
                .separator
                .is_some()
        );
        for phrase in [
            "First printed register",
            "Second printed register",
            "Nested label stays",
        ] {
            assert_eq!(probe.matches(phrase), ordinary.matches(phrase));
            assert_eq!(phrase_rects(&probe, page, phrase).len(), 1);
        }
        assert!(phrase_rects(&probe, page, "The appendix is ordinary text").is_empty());
        for label in ["Print return one", "Print return two"] {
            assert!(probe.matches(label).is_empty());
            assert_eq!(ordinary.matches(label).len(), 1);
        }
        let note = probe.fragment("cite_note-one").unwrap();
        let canonical = probe.page_for(&note).unwrap();
        assert!(canonical > page);
        assert_eq!(
            phrase_rects(&probe, canonical, "First printed register").len(),
            1
        );
        let link = phrase_rects(&probe, page, "Read the introduction")[0];
        assert_eq!(
            probe
                .link_at(
                    page,
                    (link.x + link.width / 2) as f32,
                    (link.y + link.height / 2) as f32
                )
                .and_then(|target| target.fragment),
            Some("wiki-start".into())
        );
        assert!(
            probe
                .links_on_page(page)
                .iter()
                .any(|(_, target)| { target.fragment.as_deref() == Some("cite_note-one") })
        );
        capture(&mut probe, page, "reader-wikipedia-note-band.png");
        capture(&mut probe, canonical, "reader-wikipedia-note-canonical.png");

        let table_note = advanced_note_layout(11, &[WIKIPEDIA_FOOTNOTES]);
        let normal = advanced_note_layout(11, &[]);
        let page = table_note
            .page_for(&table_note.fragment("fallback-ref").unwrap())
            .unwrap();
        let separator = table_note.in_page_notes.as_ref().unwrap()[page]
            .separator
            .unwrap()
            + table_note.settings.top_margin();
        assert!(table_note.matches("Table return").is_empty());
        assert_eq!(normal.matches("Table return").len(), 1);
        for phrase in ["Inshore readings", "Offshore readings"] {
            assert_eq!(normal.matches(phrase), table_note.matches(phrase));
            assert!(
                phrase_rects(&table_note, page, phrase)
                    .iter()
                    .any(|rect| rect.y > separator as i32)
            );
        }
    }

    #[test]
    fn classic_in_page_notes_follow_continuation_policy_and_keep_note_only_styles() {
        use crate::user_styles::{
            CLASSIC_FOOTNOTES, Data, EXTEND_NOTES, EXTEND_NOTES_UNTIL_HEADING, NOTE_BLOCK_SPACING,
            NOTE_INDENTATION, NOTE_INLINE_SIZING, NOTE_JUSTIFICATION,
        };
        let path = test_fixture("in-page-footnotes.epub");
        let font = test_fixture("fonts/NotoSans-Regular.ttf");
        let original = std::fs::read(&path).unwrap();
        let render = |presets: &[&str]| {
            let mut data = Data::default();
            for id in presets {
                data.choose(1, id, Some(true)).unwrap();
            }
            Probe::layout_opened_book(
                &mut Some(Book::open(&path).unwrap()),
                Engine::Parley,
                settings(false),
                8,
                None,
                &Default::default(),
                &data.snapshot(1).unwrap(),
                || crate::font_context(std::slice::from_ref(&font)),
            )
            .unwrap()
        };
        let before = render(&[]);
        assert!(before.in_page_notes.is_none());
        let first = before.fragment("classic-start").unwrap();
        let inner = before.fragment("classic-inner").unwrap();
        let end = before.fragment("classic-second").unwrap();
        let bounds = |probe: &Probe, page: usize, text: &str| {
            let start = probe.matches(text).remove(0);
            let end = probe.anchor(start.block, start.offset + text.len());
            probe.selection_rects(page, &probe.select_range(&start, &end).unwrap())
        };
        let mut plain = render(&[CLASSIC_FOOTNOTES]);
        let plain_page = plain.page_for(&first).unwrap();
        assert!(
            plain.in_page_notes.as_ref().unwrap()[plain_page]
                .separator
                .is_some()
        );
        let base = bounds(&plain, plain_page, "Classic note sample")[0];
        let base_label = bounds(&plain, plain_page, "raised label")[0];
        assert!(bounds(&plain, plain_page, "Continuation before the heading.").is_empty());
        assert!(!bounds(&plain, plain_page, "Another class note begins here.").is_empty());
        capture(&mut plain, plain_page, "reader-classic-note-base.png");

        let all = render(&[CLASSIC_FOOTNOTES, EXTEND_NOTES]);
        let page = all.page_for(&first).unwrap();
        for phrase in [
            "Continuation before the heading.",
            "Boundary heading",
            "Continuation after the heading.",
        ] {
            assert!(!bounds(&all, page, phrase).is_empty(), "{phrase}");
        }
        assert_eq!(all.fragment("classic-inner"), Some(inner.clone()));
        assert_eq!(all.fragment("classic-second"), Some(end));

        let mut fixed = render(&[
            CLASSIC_FOOTNOTES,
            EXTEND_NOTES,
            EXTEND_NOTES_UNTIL_HEADING,
            NOTE_BLOCK_SPACING,
            NOTE_INDENTATION,
            NOTE_INLINE_SIZING,
            NOTE_JUSTIFICATION,
        ]);
        let fixed_page = fixed.page_for(&first).unwrap();
        let changed = bounds(&fixed, fixed_page, "Classic note sample")[0];
        let label = bounds(&fixed, fixed_page, "raised label")[0];
        assert!(changed.x < base.x && label.width > base_label.width);
        assert!(!bounds(&fixed, fixed_page, "Continuation before the heading.").is_empty());
        for phrase in ["Boundary heading", "Continuation after the heading."] {
            assert!(bounds(&fixed, fixed_page, phrase).is_empty(), "{phrase}");
        }
        assert_eq!(fixed.fragment("classic-inner"), Some(inner.clone()));
        assert!(fixed.page_for(&inner).unwrap() > fixed_page);
        assert_eq!(
            bounds(&fixed, fixed_page, "A traditional marker"),
            bounds(&plain, plain_page, "A traditional marker"),
        );
        eprintln!(
            "Classic note indent x={} -> {}; raised label width={} -> {}",
            base.x, changed.x, base_label.width, label.width
        );
        capture(&mut fixed, fixed_page, "reader-classic-note-fixed.png");
        assert_eq!(std::fs::read(&path).unwrap(), original);
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
    fn fb3_note_extension_stops_at_excluded_note_bodies() {
        use crate::user_styles::{Data, EXTEND_NOTES, EXTEND_NOTES_UNTIL_HEADING, FB2_FOOTNOTES};

        // The importer omits ambiguous IDs, while split roots retain their
        // original ID and are identified by the separate incomplete set.
        // Both are typed asides, as are unmarked note bodies.
        let xhtml = r##"<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><body>
        <p><a href="#first">First reference</a> <a href="#split">Split reference</a> <a href="#last">Last reference</a></p>
        <div><aside id="first" epub:type="footnote"><p>First note</p></aside>
        <p>Ordinary continuation</p>
        <aside id="split" epub:type="footnote"><p>Incomplete note</p></aside>
        <aside epub:type="footnote"><p>Ambiguous or unmarked note</p></aside>
        <aside id="last" epub:type="footnote"><p>Last note</p></aside></div>
        </body></html>"##;
        let source = Source::parse(xhtml).unwrap();
        let incomplete = BTreeSet::from(["split".to_owned()]);
        for extension in [EXTEND_NOTES, EXTEND_NOTES_UNTIL_HEADING] {
            let mut data = Data::default();
            data.choose(1, FB2_FOOTNOTES, Some(true)).unwrap();
            data.choose(1, extension, Some(true)).unwrap();
            let styles = data.snapshot(1).unwrap();
            let targets = Targets::fb3("document.xhtml", &source, &styles, &incomplete).unwrap();
            let note_for = |text: &str| {
                let block = source
                    .blocks
                    .iter()
                    .find(|block| block.text.contains(text))
                    .unwrap();
                targets.ancestry[block.element]
            };
            assert_eq!(note_for("First note"), note_for("Ordinary continuation"));
            assert!(note_for("First note").is_some());
            assert!(note_for("Incomplete note").is_none());
            assert!(note_for("Ambiguous or unmarked note").is_none());
            assert!(note_for("Last note").is_some());
            assert_ne!(note_for("First note"), note_for("Last note"));
            assert!(!targets.ids.contains_key("split"));
        }
    }

    #[test]
    fn fb3_note_extension_does_not_promote_a_split_reference() {
        use crate::user_styles::{Data, EXTEND_NOTES, FB2_FOOTNOTES};

        let xhtml = r##"<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><body>
        <p><a href="#split">Only reference</a></p>
        <div><aside id="first" epub:type="footnote"><p>Unreferenced note</p></aside>
        <aside id="split" epub:type="footnote"><p>Incomplete note</p></aside></div>
        </body></html>"##;
        let source = Source::parse(xhtml).unwrap();
        let mut data = Data::default();
        data.choose(1, FB2_FOOTNOTES, Some(true)).unwrap();
        data.choose(1, EXTEND_NOTES, Some(true)).unwrap();
        assert!(
            Targets::fb3(
                "document.xhtml",
                &source,
                &data.snapshot(1).unwrap(),
                &BTreeSet::from(["split".to_owned()]),
            )
            .is_none()
        );
    }

    #[test]
    fn fb3_in_page_footnotes_use_fictionbook_size_and_keep_original_anchors() {
        use crate::user_styles::{
            Data, EPUB_FOOTNOTES, FB2_ENDNOTES, FB2_FOOTNOTES, FB2_REGULAR_NOTE_SIZE,
        };

        let path = test_fixture("harbor-survey.fb3");
        let bytes = std::fs::read(&path).unwrap();
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
                || crate::font_context(&[test_fixture("fonts/NotoSans-Regular.ttf")]),
            )
            .unwrap()
        };
        let selection = |probe: &Probe, phrase: &str| {
            let start = probe.matches(phrase).pop().unwrap();
            let end = probe.anchor(start.block, start.offset + phrase.len());
            probe.select_range(&start, &end).unwrap()
        };
        let mut ordinary = render(&[]);
        let reference = selection(&ordinary, "tide note");
        let target = ordinary.fragment("tide-note").unwrap();
        let text = selection(&ordinary, "midnight");
        let backlink = selection(&ordinary, "Return to soundings");
        let page = ordinary.page_for(&reference.start).unwrap();
        let canonical = ordinary.page_for(&target).unwrap();
        assert!(canonical > page);
        assert!(ordinary.in_page_notes.is_none());
        let baseline = capture(&mut ordinary, page, "reader-fb3-note-off.png");
        for styles in [[EPUB_FOOTNOTES], [FB2_ENDNOTES], [FB2_REGULAR_NOTE_SIZE]] {
            let mut unrelated = render(&styles);
            assert!(unrelated.in_page_notes.is_none());
            assert_eq!(unrelated.capture(page).unwrap(), baseline);
        }
        let mut widths = Vec::new();
        for (name, styles) in [
            ("on", &[FB2_FOOTNOTES][..]),
            ("regular", &[FB2_FOOTNOTES, FB2_REGULAR_NOTE_SIZE][..]),
        ] {
            let mut probe = render(styles);
            let page = probe.page_for(&reference.start).unwrap();
            let canonical = probe.page_for(&target).unwrap();
            assert!(canonical > page);
            assert!(
                probe.in_page_notes.as_ref().unwrap()[page]
                    .separator
                    .is_some()
            );
            assert!(
                probe.in_page_notes.as_ref().unwrap()[canonical]
                    .separator
                    .is_none()
            );
            assert_eq!(probe.restore_anchor(&target), Some(target.clone()));
            assert_eq!(probe.fragment("tide-note"), Some(target.clone()));
            assert_eq!(selection(&probe, "midnight"), text);
            assert_eq!(selection(&probe, "Return to soundings"), backlink);
            let reference_rect = probe.selection_rects(page, &reference)[0];
            assert_eq!(
                probe
                    .link_at(
                        page,
                        (reference_rect.x + 5) as f32,
                        (reference_rect.y + reference_rect.height / 2) as f32
                    )
                    .unwrap()
                    .fragment
                    .as_deref(),
                Some("tide-note")
            );
            for current in [page, canonical] {
                let link = probe.selection_rects(current, &backlink)[0];
                assert_eq!(
                    probe
                        .link_at(
                            current,
                            (link.x + 5) as f32,
                            (link.y + link.height / 2) as f32
                        )
                        .unwrap()
                        .fragment
                        .as_deref(),
                    Some("morning")
                );
            }
            widths.push(probe.selection_rects(canonical, &text)[0].width);
            capture(&mut probe, page, &format!("reader-fb3-note-{name}.png"));
            if name == "on" {
                capture(&mut probe, canonical, "reader-fb3-note-original.png");
            }
        }
        assert!(
            (0.70..0.80).contains(&(widths[0] as f32 / widths[1] as f32)),
            "{widths:?}"
        );
        assert_eq!(
            widths[1],
            ordinary.selection_rects(canonical, &text)[0].width
        );
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }

    #[test]
    fn fb3_in_page_footnotes_leave_cross_part_links_in_their_original_flow() {
        use crate::user_styles::{Data, FB2_FOOTNOTES};

        let mut book = Some(Book::open(&test_fixture("harbor-continuation.fb3")).unwrap());
        let mut data = Data::default();
        data.choose(1, FB2_FOOTNOTES, Some(true)).unwrap();
        let mut render = |part| {
            Probe::layout_opened_book(
                &mut book,
                Engine::Parley,
                settings(false),
                part,
                None,
                &Default::default(),
                &data.snapshot(1).unwrap(),
                || crate::font_context(&[test_fixture("fonts/NotoSans-Regular.ttf")]),
            )
            .unwrap()
        };
        let first = render(0);
        assert!(first.in_page_notes.is_none());
        let link = first
            .source
            .blocks
            .iter()
            .flat_map(|block| &block.links)
            .find(|link| link.href.ends_with("#later-note"))
            .unwrap();
        let target = Book::resolve_link(&first.chapter, &link.href).unwrap();
        assert_eq!(target.path, "fb3-section-2.xhtml");
        assert_eq!(target.fragment.as_deref(), Some("later-note"));
        let second = render(1);
        assert!(second.in_page_notes.is_none());
        let note = second
            .fragment(target.fragment.as_deref().unwrap())
            .unwrap();
        assert!(second.page_for(&note).is_some());
        assert_eq!(second.restore_anchor(&note), Some(note));
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

    #[cfg(feature = "kindle")]
    #[test]
    fn combined_footnote_flows_keep_long_reference_continuations_regular() {
        let mut choices = crate::user_styles::Data::default();
        choices
            .choose(1, crate::user_styles::COMBINE_NONLINEAR_NOTES, Some(true))
            .unwrap();
        let styles = choices.snapshot(1).unwrap().for_nonlinear_notes(true);
        let probe = Probe::layout_opened_book(
            &mut Some(Book::open(&test_fixture("in-page-footnotes.epub")).unwrap()),
            Engine::Parley,
            settings(true),
            1,
            None,
            &Default::default(),
            &styles,
            || crate::font_context(&[test_fixture("fonts/NotoSans-Regular.ttf")]),
        )
        .unwrap();
        let canonical = probe.canonical_note_flows();
        assert_eq!(canonical.len(), 1);
        assert!(canonical[0].len() > 1);
        let pages = probe.in_page_notes.as_ref().unwrap();
        assert!(
            pages[canonical[0].clone()]
                .iter()
                .all(|page| page.separator.is_none())
        );
        let end = probe.fragment("note-end").unwrap();
        let end_top = probe.anchor_top(&end).unwrap();
        let copy = pages
            .iter()
            .position(|page| {
                page.parts
                    .iter()
                    .any(|part| part.y > 0 && part.source.contains(&end_top))
            })
            .unwrap();
        let reference = probe.page_for(&probe.fragment("ref").unwrap()).unwrap();
        assert!(copy > reference && copy < canonical[0].start);
        assert!(canonical[0].contains(&probe.page_for(&end).unwrap()));
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
            quote_prefixes: Default::default(),
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
