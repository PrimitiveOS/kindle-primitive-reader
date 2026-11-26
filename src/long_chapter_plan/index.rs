//! Unwired page-index experiment over immutable EPUB and font snapshots.
//!
//! This proves only the inputs used to build this index. It does not bind them
//! to a live Reader, verify filesystem state, or authorize a reading handoff.
use super::{
    LayoutMode, LongChapterPlan, cancelled, pagination::SimplePagination,
    shape_witness::ShapeWitness,
};
use crate::book_style::BookStyle;
use crate::epub::Book;
use crate::parley_layout::long_chapter::{SimpleLayoutCheckpoint, SimpleLayoutStream};
use crate::source::Source;
use crate::{Anchor, Line, Result, Settings, TypographyLocale};
use parley::{FontContext, GenericFamily};
use std::ops::Range;
use std::sync::{Arc, atomic::AtomicBool};

const MAX_BOOK_BYTES: usize = 16 * 1024 * 1024;
const MAX_FONT_BYTES: usize = 16 * 1024 * 1024;
const MAX_FONTS: usize = 8;
const MAX_LINES: usize = 30_000;

struct LineRecord {
    block: usize,
    bytes: Range<usize>,
    top: f32,
    bottom: f32,
}

/// An index entry for a source-block work batch. This is not a replayable
/// layout checkpoint and does not identify a page boundary.
pub(crate) struct BatchIndex {
    pub blocks: Range<usize>,
    pub lines: Range<usize>,
    /// Compact layout transcript used to reject a mismatched future replay.
    pub shape_digest: [u8; 32],
}

pub(crate) struct PageIndex {
    book_bytes: Arc<[u8]>,
    font_bytes: Vec<Arc<Vec<u8>>>,
    source: Source,
    xhtml: String,
    styles: Vec<crate::book_style::Style>,
    plan: LongChapterPlan,
    bounds: Vec<f32>,
    lines: Vec<LineRecord>,
    page_lines: Vec<Option<Range<usize>>>,
    batches: Vec<BatchIndex>,
    checkpoints: Vec<SimpleLayoutCheckpoint>,
    book_digest: [u8; 32],
    font_ids: Vec<u64>,
    font_digests: Vec<[u8; 32]>,
    settings: Settings,
    mode: LayoutMode,
    user_styles: crate::user_styles::Snapshot,
    rules: Arc<crate::user_hyphenation::Rules>,
}

pub(crate) struct VerifiedBatch {
    pub paragraphs: Vec<crate::parley_layout::Paragraph>,
    pub lines: Vec<Line>,
}

pub(crate) struct VerifiedPage {
    pub bounds: Range<f32>,
    pub batches: Vec<VerifiedBatch>,
}

fn page_line_ranges(
    bounds: &[f32],
    lines: &[LineRecord],
    cancel: &AtomicBool,
) -> Result<Vec<Option<Range<usize>>>> {
    let pages = bounds.len().saturating_sub(1);
    let mut ranges: Vec<Option<Range<usize>>> = vec![None; pages];
    let mut previous_page = None;
    for (index, line) in lines.iter().enumerate() {
        if index % 256 == 0 {
            cancelled(cancel)?;
        }
        let page = bounds
            .partition_point(|bound| *bound <= line.top)
            .saturating_sub(1);
        let Some((&start, &end)) = bounds.get(page).zip(bounds.get(page + 1)) else {
            return Err("The long chapter page index is invalid".into());
        };
        if page >= pages
            || line.top < start
            || line.bottom > end
            || previous_page.is_some_and(|previous| previous > page)
        {
            return Err("A long chapter line crosses a page boundary".into());
        }
        match &mut ranges[page] {
            Some(range) if range.end == index => range.end += 1,
            Some(_) => return Err("The long chapter page lines are not contiguous".into()),
            None => ranges[page] = Some(index..index + 1),
        }
        previous_page = Some(page);
    }
    cancelled(cancel)?;
    Ok(ranges)
}

fn digest(bytes: &[u8], cancel: &AtomicBool) -> Result<[u8; 32]> {
    let mut hash = ring::digest::Context::new(&ring::digest::SHA256);
    for chunk in bytes.chunks(64 * 1024) {
        cancelled(cancel)?;
        hash.update(chunk);
    }
    cancelled(cancel)?;
    let mut result = [0; 32];
    result.copy_from_slice(hash.finish().as_ref());
    Ok(result)
}

fn same_settings(left: Settings, right: Settings) -> bool {
    left == right
        && left.font_size.to_bits() == right.font_size.to_bits()
        && left.logical_font_size.map(f64::to_bits) == right.logical_font_size.map(f64::to_bits)
        && left.line_height.to_bits() == right.line_height.to_bits()
        && left.word_spacing.to_bits() == right.word_spacing.to_bits()
        && left.letter_spacing.to_bits() == right.letter_spacing.to_bits()
        && left.font_weight_delta.to_bits() == right.font_weight_delta.to_bits()
}

/// No system or file-backed fallback is allowed in this isolated experiment.
/// Keep the same registration order and TTC face ordering as font_context.
fn captured_fonts(
    snapshots: &[Arc<Vec<u8>>],
    cancel: &AtomicBool,
) -> Result<(FontContext, Vec<u64>, Vec<[u8; 32]>)> {
    if snapshots.is_empty() || snapshots.len() > MAX_FONTS {
        return Err("The long chapter font snapshot is unavailable".into());
    }
    let mut total = 0usize;
    let mut fonts = FontContext {
        collection: parley::fontique::Collection::new(parley::fontique::CollectionOptions {
            shared: false,
            system_fonts: false,
        }),
        source_cache: Default::default(),
    };
    let mut identities = Vec::with_capacity(snapshots.len());
    let mut hashes = Vec::with_capacity(snapshots.len());
    let mut families = Vec::new();
    for bytes in snapshots {
        cancelled(cancel)?;
        total = total
            .checked_add(bytes.len())
            .ok_or("The font snapshot is too large")?;
        if bytes.is_empty() || total > MAX_FONT_BYTES {
            return Err("The font snapshot is too large or empty".into());
        }
        hashes.push(digest(bytes, cancel)?);
        let blob = parley::fontique::Blob::new(bytes.clone());
        identities.push(blob.id());
        let mut registered = fonts.collection.register_fonts(blob, None);
        if registered.is_empty() {
            return Err("A captured reading font cannot be loaded".into());
        }
        registered.sort_by_key(|(_, faces)| {
            faces
                .iter()
                .map(|face| face.index())
                .min()
                .unwrap_or(u32::MAX)
        });
        families.extend(registered.into_iter().map(|(family, _)| family));
    }
    fonts
        .collection
        .set_generic_families(GenericFamily::Serif, families.iter().copied());
    fonts
        .collection
        .set_generic_families(GenericFamily::SansSerif, families.iter().copied());
    Ok((fonts, identities, hashes))
}

impl PageIndex {
    /// Build from immutable in-memory snapshots only. Caller must separately
    /// prove these are the actual open book and font sources before any future
    /// handoff. No partial index escapes on a failure or cancellation.
    pub(crate) fn build(
        book_bytes: Arc<[u8]>,
        font_bytes: &[Arc<Vec<u8>>],
        chapter: usize,
        settings: Settings,
        mode: LayoutMode,
        user_styles: crate::user_styles::Snapshot,
        rules: Arc<crate::user_hyphenation::Rules>,
        cancel: &AtomicBool,
    ) -> Result<Self> {
        cancelled(cancel)?;
        settings.validate()?;
        if book_bytes.is_empty()
            || book_bytes.len() > MAX_BOOK_BYTES
            || settings.embedded_fonts
            || !user_styles.is_empty()
        {
            return Err("This book or font configuration cannot be indexed safely".into());
        }
        let book_digest = digest(&book_bytes, cancel)?;
        cancelled(cancel)?;
        let (mut fonts, identities, font_digests) = captured_fonts(font_bytes, cancel)?;
        let mut book = Book::open_snapshot(Arc::clone(&book_bytes))?;
        cancelled(cancel)?;
        if !book.is_reflowable_epub()
            || book.spine_rtl
            || (settings.publisher_render_mode.is_some() && !settings.publisher_styles)
        {
            return Err("The EPUB layout is unavailable for indexing".into());
        }
        let href = book
            .chapters
            .get(chapter)
            .ok_or("The chapter is unavailable")?
            .href
            .clone();
        let xhtml = book.chapter(chapter)?;
        cancelled(cancel)?;
        let shaping_main = settings
            .typography
            .resolve(book.language.map(TypographyLocale::publication));
        let shaping_language = shaping_main.map(TypographyLocale::shaping_language);
        let main_language = settings.typography.resolve(book.publication_typography());
        let mut source = book.chapter_source_with_language(
            &href,
            &xhtml,
            shaping_language,
            settings.typography.respect_embedded,
        )?;
        source.resolve_hyphenation_languages(main_language, settings.typography.respect_embedded);
        // Never load an external stylesheet or embedded font into this proof.
        if !source.stylesheets.is_empty() {
            return Err("This chapter has unsupported stylesheet resources".into());
        }
        let author = BookStyle::load_with_base(&mut book, &href, &source, &mut fonts, settings)
            .with_user_css(&user_styles)?;
        let (styles, quotes) = author.compute_deferred_quotes(&source.elements, settings, None);
        if !quotes.is_empty() {
            return Err("This chapter has generated quote state".into());
        }
        drop(quotes);
        drop(author);
        cancelled(cancel)?;
        let plan = LongChapterPlan::prepare(
            &mut book,
            chapter,
            &source,
            &styles,
            &user_styles,
            settings,
            mode,
            cancel,
        )?;
        let mut pagination =
            SimplePagination::new(&plan, &source, &xhtml, settings, &[], &[], &[], cancel)?;
        let mut stream = SimpleLayoutStream::new(
            &plan,
            &source,
            &xhtml,
            &styles,
            &user_styles,
            &rules,
            settings,
            mode,
            fonts,
            cancel,
        )?;
        let mut records = Vec::new();
        let mut batches = Vec::with_capacity(plan.batches().len());
        let mut checkpoints = Vec::with_capacity(plan.batches().len() + 1);
        checkpoints.push(stream.checkpoint()?);
        for batch in plan.batches() {
            cancelled(cancel)?;
            let first = records.len();
            let mut witness = ShapeWitness::new(batch.blocks.clone());
            let produced = stream.next_batch_owned(|paragraphs, lines| {
                witness.observe(&paragraphs, &lines, &identities, &font_digests)?;
                for paragraph in &paragraphs {
                    for line in paragraph.layout.lines() {
                        for run in line.runs() {
                            if !identities.contains(&run.font().data.id()) {
                                return Err("A shaped font was not captured".into());
                            }
                        }
                    }
                }
                for line in &lines {
                    if line.missing_glyphs != 0 || records.len() == MAX_LINES {
                        return Err(
                            "The long chapter has unavailable glyphs or too many lines".into()
                        );
                    }
                    pagination.push(line)?;
                    records.push(LineRecord {
                        block: line.block,
                        bytes: line.bytes.clone(),
                        top: line.top,
                        bottom: line.bottom,
                    });
                }
                Ok(())
            })?;
            if !produced {
                return Err("The long chapter layout stream stopped early".into());
            }
            batches.push(BatchIndex {
                blocks: batch.blocks.clone(),
                lines: first..records.len(),
                shape_digest: witness.finish()?,
            });
            checkpoints.push(stream.checkpoint()?);
        }
        let (extent, painted_bottom) = stream.finish()?;
        let bounds = pagination.finish(extent, painted_bottom)?;
        let page_lines = page_line_ranges(&bounds, &records, cancel)?;
        cancelled(cancel)?;
        Ok(Self {
            book_bytes,
            font_bytes: font_bytes.to_vec(),
            source,
            xhtml,
            styles,
            plan,
            bounds,
            lines: records,
            page_lines,
            batches,
            checkpoints,
            book_digest,
            font_ids: identities,
            font_digests,
            settings,
            mode,
            user_styles,
            rules,
        })
    }

    /// Exact immutable inputs used by this experiment, not a live-path proof.
    pub(crate) fn matches_snapshots(
        &self,
        book: &[u8],
        fonts: &[Arc<Vec<u8>>],
        settings: Settings,
        mode: LayoutMode,
        user_styles: &crate::user_styles::Snapshot,
        rules: &crate::user_hyphenation::Rules,
        cancel: &AtomicBool,
    ) -> bool {
        !book.is_empty()
            && book.len() <= MAX_BOOK_BYTES
            && digest(book, cancel).is_ok_and(|value| value == self.book_digest)
            && fonts.len() == self.font_digests.len()
            && fonts
                .iter()
                .try_fold(0usize, |total, bytes| total.checked_add(bytes.len()))
                .is_some_and(|total| total <= MAX_FONT_BYTES)
            && fonts
                .iter()
                .zip(&self.font_digests)
                .all(|(font, expected)| digest(font, cancel).is_ok_and(|value| value == *expected))
            && same_settings(settings, self.settings)
            && mode == self.mode
            && user_styles == &self.user_styles
            && rules == self.rules.as_ref()
    }

    /// Replay one bounded batch from its exact prior layout state and discard
    /// it unless source ranges, geometry, selected captured fonts and glyphs
    /// match the index transcript. Call only on a worker.
    pub(crate) fn replay_batch(&self, index: usize, cancel: &AtomicBool) -> Result<VerifiedBatch> {
        cancelled(cancel)?;
        let batch = self
            .batches
            .get(index)
            .ok_or("The long chapter page window is unavailable")?;
        let checkpoint = self
            .checkpoints
            .get(index)
            .ok_or("The long chapter layout checkpoint is unavailable")?;
        if !self.matches_snapshots(
            &self.book_bytes,
            &self.font_bytes,
            self.settings,
            self.mode,
            &self.user_styles,
            &self.rules,
            cancel,
        ) {
            return Err("The long chapter snapshot changed".into());
        }
        let (fonts, identities, digests) = captured_fonts(&self.font_bytes, cancel)?;
        if identities != self.font_ids || digests != self.font_digests {
            return Err("The long chapter font registration changed".into());
        }
        let mut stream = SimpleLayoutStream::new_at_checkpoint(
            &self.plan,
            &self.source,
            &self.xhtml,
            &self.styles,
            &self.user_styles,
            &self.rules,
            self.settings,
            self.mode,
            fonts,
            cancel,
            checkpoint,
        )?;
        let expected_lines = self
            .lines
            .get(batch.lines.clone())
            .ok_or("The long chapter page index changed")?;
        let mut replayed = None;
        let produced = stream.next_batch_owned(|paragraphs, lines| {
            let mut witness = ShapeWitness::new(batch.blocks.clone());
            witness.observe(&paragraphs, &lines, &identities, &digests)?;
            if witness.finish()? != batch.shape_digest || lines.len() != expected_lines.len() {
                return Err("The long chapter page window did not match its index".into());
            }
            for (line, expected) in lines.iter().zip(expected_lines) {
                if line.block != expected.block
                    || line.bytes != expected.bytes
                    || line.top.to_bits() != expected.top.to_bits()
                    || line.bottom.to_bits() != expected.bottom.to_bits()
                {
                    return Err("The long chapter page window changed".into());
                }
            }
            replayed = Some(VerifiedBatch { paragraphs, lines });
            Ok(())
        })?;
        if !produced {
            return Err("The long chapter page window could not be replayed".into());
        }
        cancelled(cancel)?;
        replayed.ok_or("The long chapter page window was empty".into())
    }

    /// Rebuild only the captured batches that contain lines on this page.
    /// The complete page is discarded if any batch fails its replay witness.
    pub(crate) fn replay_page(&self, page: usize, cancel: &AtomicBool) -> Result<VerifiedPage> {
        let bounds = self
            .page_bounds(page)
            .ok_or("The long chapter page is unavailable")?;
        let mut batches = Vec::new();
        if let Some(lines) = self.page_lines.get(page).and_then(Option::as_ref) {
            let first = self
                .batches
                .partition_point(|batch| batch.lines.end <= lines.start);
            let end = self
                .batches
                .partition_point(|batch| batch.lines.start < lines.end);
            if first >= end {
                return Err("The long chapter page has no replay batches".into());
            }
            batches
                .try_reserve_exact(end - first)
                .map_err(|_| "The long chapter page cannot be reserved")?;
            let mut covered = lines.start;
            for index in first..end {
                cancelled(cancel)?;
                let batch = self
                    .batches
                    .get(index)
                    .ok_or("The long chapter page batch is unavailable")?;
                let overlap_start = batch.lines.start.max(lines.start);
                let overlap_end = batch.lines.end.min(lines.end);
                if overlap_start != covered || overlap_start >= overlap_end {
                    return Err("The long chapter page batches are not contiguous".into());
                }
                covered = overlap_end;
                batches.push(self.replay_batch(index, cancel)?);
            }
            if covered != lines.end {
                return Err("The long chapter page is missing a replay batch".into());
            }
        }
        cancelled(cancel)?;
        Ok(VerifiedPage { bounds, batches })
    }

    pub(crate) fn page_count(&self) -> usize {
        self.bounds.len().saturating_sub(1)
    }

    pub(crate) fn page_bounds(&self, page: usize) -> Option<Range<f32>> {
        (page < self.page_count()).then(|| self.bounds[page]..self.bounds[page + 1])
    }

    /// Follow the ordinary simple-source anchor lookup. Legacy-revision, raw
    /// and illustration anchors are declined by the plan instead of guessed.
    pub(crate) fn page_for_anchor(&self, anchor: &Anchor) -> Option<usize> {
        self.plan.batch_for_anchor(&self.source, anchor)?;
        let block_index = self
            .source
            .blocks
            .binary_search_by_key(&anchor.block, |block| block.key)
            .ok()?;
        let block = &self.source.blocks[block_index];
        let first = self.lines.partition_point(|line| line.block < anchor.block);
        let rest = &self.lines[first..];
        let count = rest.partition_point(|line| line.block == anchor.block);
        let lines = &rest[..count];
        let top = lines
            .iter()
            .find(|line| line.bytes.contains(&anchor.offset))
            .map(|line| line.top)
            .or_else(|| {
                (anchor.offset == block.text.len())
                    .then(|| {
                        lines
                            .iter()
                            .rev()
                            .find(|line| !line.bytes.is_empty())
                            .map(|line| line.top)
                    })
                    .flatten()
            })?;
        let count = self.page_count();
        (count > 0).then(|| {
            self.bounds[..count]
                .partition_point(|&bound| bound <= top)
                .saturating_sub(1)
        })
    }

    pub(crate) fn batches(&self) -> &[BatchIndex] {
        &self.batches
    }

    pub(crate) fn batch_shape_digest(&self, index: usize) -> Option<[u8; 32]> {
        self.batches.get(index).map(|batch| batch.shape_digest)
    }

    /// Rows touching an actual page, preserving original line ordering.
    pub(crate) fn lines_on_page(&self, page: usize) -> Option<Vec<(usize, Range<usize>)>> {
        self.page_bounds(page)?;
        let Some(range) = self.page_lines.get(page)?.as_ref() else {
            return Some(Vec::new());
        };
        Some(
            self.lines[range.clone()]
                .iter()
                .map(|line| (line.block, line.bytes.clone()))
                .collect(),
        )
    }
}
