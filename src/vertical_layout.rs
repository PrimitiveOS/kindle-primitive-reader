//! Text-only EPUB `vertical-rl` columns and source-based hit testing.
//!
//! CSS Writing Modes §5.1 and Unicode UAX #50 distinguish upright vertical
//! shaping from clockwise painting of horizontally shaped sideways runs.

use std::{collections::BTreeMap, ops::Range, sync::Arc};

use harfrust::{Direction, Feature, FontRef, ShapeOptions, ShaperData, Tag, UnicodeBuffer};
use icu_properties::{
    CodePointMapData, CodePointSetData,
    props::{DefaultIgnorableCodePoint, GeneralCategory, Script, VerticalOrientation},
};
use icu_segmenter::{GraphemeClusterSegmenter, LineSegmenter};
use parley::{
    FontContext, FontData, FontWeight, GenericFamily, Language,
    fontique::{Attributes, CharmapIndex, FallbackKey, FontWidth, QueryFamily, QueryStatus},
};
use skrifa::{
    MetadataProvider,
    instance::{LocationRef, NormalizedCoord, Size},
    raw::TableProvider,
};

use crate::book_style::{Family, Style};
use crate::flow_projection::{Affinity, OwnedProjection};
use crate::flow_view::{ChapterFlow, LayoutBlock};
use crate::gray_glyph::GlyphCache;
use crate::selection::HighlightRect;
use crate::source::{Block, Source, StyledText};
use crate::{Line, Result, Settings};

const MAX_SOURCE_BYTES: usize = 512 * 1024;
const MAX_PARAGRAPH_BYTES: usize = 32 * 1024;
const MAX_GLYPHS: usize = 90_000;
const MAX_COLUMNS: usize = 12_000;
const MAX_FACES: usize = 16;
const MAX_FONT_BYTES: usize = 32 * 1024 * 1024;
const MAX_SCALE: f32 = 1.4;
const MAX_RUBY_PER_BLOCK: usize = 128;
const MAX_RUBY_TEXT: usize = 4_096;
const RUBY_SCALE: f32 = 0.50;
const MIN_RUBY_REDUCTION: f32 = 0.60;
fn kerning_features(mode: crate::FontKerningMode) -> Vec<Feature> {
    const OFF: [(&[u8; 4], u32); 2] = [(b"kern", 0), (b"vkrn", 0)];
    const FAST: [(&[u8; 4], u32); 23] = [
        (b"kern", 1),
        (b"vkrn", 0),
        (b"mark", 0),
        (b"mkmk", 0),
        (b"curs", 0),
        (b"locl", 0),
        (b"liga", 0),
        (b"rlig", 0),
        (b"clig", 0),
        (b"ccmp", 0),
        (b"calt", 0),
        (b"rclt", 0),
        (b"rvrn", 0),
        (b"ltra", 0),
        (b"ltrm", 0),
        (b"rtla", 0),
        (b"rtlm", 0),
        (b"frac", 0),
        (b"numr", 0),
        (b"dnom", 0),
        (b"rand", 0),
        (b"trak", 0),
        (b"vert", 0),
    ];
    const GOOD: [(&[u8; 4], u32); 22] = [
        (b"kern", 1),
        (b"mark", 1),
        (b"mkmk", 1),
        (b"curs", 1),
        (b"locl", 1),
        (b"liga", 0),
        (b"rlig", 0),
        (b"clig", 0),
        (b"ccmp", 0),
        (b"calt", 0),
        (b"rclt", 0),
        (b"rvrn", 0),
        (b"ltra", 0),
        (b"ltrm", 0),
        (b"rtla", 0),
        (b"rtlm", 0),
        (b"frac", 0),
        (b"numr", 0),
        (b"dnom", 0),
        (b"rand", 0),
        (b"trak", 0),
        (b"vert", 0),
    ];
    let settings: &[(&[u8; 4], u32)] = match mode {
        crate::FontKerningMode::Off => &OFF,
        crate::FontKerningMode::Fast => &FAST,
        crate::FontKerningMode::Good => &GOOD,
        crate::FontKerningMode::Best => &[],
    };
    settings
        .iter()
        .map(|(tag, value)| Feature::new(Tag::new(*tag), *value, ..))
        .collect()
}

pub(crate) struct PaintGlyph<'a> {
    pub font: &'a FontData,
    pub id: u32,
    pub size: f32,
    pub x: f32,
    pub y: f32,
    pub clockwise: bool,
    pub strength: f32,
    pub variations: &'a [NormalizedCoord],
}

struct Face {
    font: FontData,
    charmap: CharmapIndex,
    shaper: ShaperData,
    em: f32,
    horizontal_center: f32,
    variations: Vec<NormalizedCoord>,
}

impl Face {
    fn supports(&self, text: &str) -> bool {
        self.charmap
            .charmap(self.font.data.as_ref())
            .is_some_and(|map| {
                text.chars()
                    .all(|ch| ignorable(ch) || map.map(ch).is_some_and(|id| id != 0))
            })
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Orientation {
    Upright,
    Sideways,
}

#[derive(Clone, PartialEq)]
struct Format {
    element: usize,
    orientation: Orientation,
    language: Language,
    script: Script,
}

struct Run {
    bytes: Range<usize>,
    format: Format,
    face: usize,
    size: f32,
    strength: f32,
    painted: bool,
}

struct Text<'a> {
    value: &'a str,
    spans: &'a [StyledText],
    element: usize,
    language: Option<Language>,
    scale: f32,
}

#[derive(Clone, Copy)]
struct Glyph {
    id: u32,
    face: usize,
    size: f32,
    x: f32,
    y: f32,
    clockwise: bool,
    strength: f32,
    painted: bool,
}

struct Cluster {
    bytes: Range<usize>,
    glyphs: Range<usize>,
    advance: f32,
    unsafe_start: bool,
}

#[derive(Default)]
struct Shaped {
    glyphs: Vec<Glyph>,
    clusters: Vec<Cluster>,
}

struct RubyOverlay {
    base: Range<usize>,
    glyphs: Vec<Glyph>,
    height: f32,
}

struct VisibleCluster {
    bytes: Range<usize>,
    top: f32,
    bottom: f32,
}

struct Column {
    block: usize,
    left: f32,
    right: f32,
    clusters: Vec<VisibleCluster>,
    glyphs: Range<usize>,
}

/// Physical page geometry and a parallel logical position axis for shared anchors.
pub(crate) struct VerticalChapter {
    faces: Vec<Face>,
    glyphs: Vec<Glyph>,
    columns: Vec<Column>,
    page_span: f32,
    columns_per_page: usize,
    pitch: f32,
    ruby_gutter: f32,
    cache: GlyphCache,
    mappings: BTreeMap<usize, Arc<OwnedProjection>>,
}

impl VerticalChapter {
    pub(crate) fn new(
        source: &Source,
        styles: &[Style],
        fonts: FontContext,
        settings: Settings,
    ) -> Result<(Self, Vec<Line>, f32)> {
        let flow = ChapterFlow::from_styles(source, styles)?;
        Self::with_flow(source, styles, fonts, settings, &flow)
    }

    pub(crate) fn with_flow(
        source: &Source,
        styles: &[Style],
        mut fonts: FontContext,
        settings: Settings,
        flow: &ChapterFlow,
    ) -> Result<(Self, Vec<Line>, f32)> {
        if source
            .blocks
            .iter()
            .map(|block| block.text.len())
            .sum::<usize>()
            > MAX_SOURCE_BYTES
        {
            return Err("The chapter exceeds the vertical reading text limit".into());
        }
        if source.blocks.iter().any(|block| {
            !styles[block.element].flow.suppressed
                && block
                    .images
                    .iter()
                    .any(|image| !styles[image.element].flow.suppressed)
        }) {
            return Err("Vertical reading currently supports text without illustrations".into());
        }
        let width = settings.text_width() as f32;
        let (pitch, ruby_gutter) = if settings.logical_font_size.is_some() {
            // Container styles do not allocate a glyph column. Only retained
            // projected text owners and actual ruby annotations contribute.
            let mut largest = 0.0_f32;
            let mut annotation = 0.0_f32;
            for original in &source.blocks {
                if styles[original.element].flow.suppressed {
                    continue;
                }
                let block = flow.layout_block(source, original)?;
                let mut cursor = 0;
                for run in block.styled_text.iter() {
                    if run.bytes.start > cursor {
                        largest =
                            largest.max(settings.styled_font(styles[block.element].size_scale));
                    }
                    if run.bytes.start < run.bytes.end
                        && block.text.get(run.bytes.clone()).is_some()
                    {
                        largest = largest.max(settings.styled_font(styles[run.element].size_scale));
                    }
                    cursor = cursor.max(run.bytes.end);
                }
                if cursor < block.text.len() {
                    largest = largest.max(settings.styled_font(styles[block.element].size_scale));
                }
                for ruby in block.ruby.iter().filter(|ruby| !ruby.annotation.is_empty()) {
                    annotation = annotation
                        .max(settings.styled_font(styles[ruby.element].size_scale * RUBY_SCALE));
                }
            }
            let gutter = if annotation > 0. {
                annotation + largest * 0.20
            } else {
                0.
            };
            (
                (largest * settings.effective_line_height()).max(12.) + gutter,
                gutter,
            )
        } else {
            let gutter = if source.blocks.iter().any(|block| {
                !styles[block.element].flow.suppressed
                    && block
                        .ruby
                        .iter()
                        .any(|ruby| !styles[ruby.element].flow.suppressed)
            }) {
                settings.font_size * MAX_SCALE * (RUBY_SCALE + 0.20)
            } else {
                0.
            };
            (
                (settings.font_size * settings.effective_line_height() * MAX_SCALE).max(12.)
                    + gutter,
                gutter,
            )
        };
        let columns_per_page = (width / pitch).floor() as usize;
        if columns_per_page == 0 {
            return Err("This viewport is too narrow for a vertical reading column".into());
        }
        let page_span = settings.text_height() as f32;
        let mut reader = Self {
            faces: Vec::new(),
            glyphs: Vec::new(),
            columns: Vec::new(),
            page_span,
            columns_per_page,
            pitch,
            ruby_gutter,
            mappings: BTreeMap::new(),
            cache: GlyphCache::with_hinting(
                settings.logical_font_size.is_some(),
                settings.font_hinting,
                styles
                    .first()
                    .map_or_else(Default::default, |style| style.font_weights.clone()),
            ),
        };
        let mut lines = Vec::new();
        for original in &source.blocks {
            if styles[original.element].flow.suppressed {
                continue;
            }
            let block = flow.layout_block(source, original)?;
            if block.text.is_empty() {
                continue;
            }
            if let Some(mapping) = flow.shared(original.key) {
                reader.mappings.insert(original.key, mapping);
            }
            if block.text.len() > MAX_PARAGRAPH_BYTES {
                return Err("A paragraph exceeds the vertical reading limit".into());
            }
            let text = Text {
                value: block.text,
                spans: &block.styled_text,
                element: block.element,
                language: block.language,
                scale: 1.0,
            };
            let runs = font_runs(text, styles, &mut fonts, &mut reader.faces, settings)?;
            let limit = MAX_GLYPHS - reader.glyphs.len();
            let mut shaped = shape_block(
                block.text,
                &runs,
                &reader.faces,
                limit,
                settings.effective_font_kerning_mode(),
            )?;
            let mut overlays =
                reader.ruby_overlays(&block, &mut shaped, styles, &mut fonts, settings, limit)?;
            let first_line = lines.len();
            reader.place_block(&block, &mut shaped, &mut overlays, &mut lines, settings)?;
            let view = flow.view(source, original)?;
            for line in &mut lines[first_line..] {
                line.project(&view, line.bytes.clone())?;
            }
        }
        if reader.columns.is_empty() {
            return Err("The chapter has no vertical reading text".into());
        }
        let extent = reader.columns.len().div_ceil(columns_per_page) as f32 * page_span;
        if !extent.is_finite() || extent > settings.chapter_height_limit() {
            return Err("The chapter exceeds the vertical pagination limit".into());
        }
        Ok((reader, lines, extent))
    }

    fn place_block(
        &mut self,
        block: &LayoutBlock<'_>,
        shaped: &mut Shaped,
        overlays: &mut [RubyOverlay],
        lines: &mut Vec<Line>,
        settings: Settings,
    ) -> Result<()> {
        let segmenter = LineSegmenter::new_for_non_complex_scripts(Default::default());
        let legal: Vec<_> = segmenter.segment_str(block.text).collect();
        let mut start = 0;
        let mut ruby = overlays.iter_mut().peekable();
        while start < shaped.clusters.len() {
            if self.columns.len() == MAX_COLUMNS {
                return Err("The chapter exceeds the vertical column limit".into());
            }
            let end = column_end(&shaped.clusters, &legal, start, self.page_span)?;
            let index = self.columns.len();
            let slot = index % self.columns_per_page;
            let page = index / self.columns_per_page;
            let logical_top = page as f32 * self.page_span;
            let logical_pitch = self.page_span / self.columns_per_page as f32;
            let logical_end = ((page + 1) as f32 * self.page_span)
                .min(logical_top + (slot + 1) as f32 * logical_pitch);
            let left = settings.width as f32
                - settings.right_margin() as f32
                - (slot + 1) as f32 * self.pitch;
            let base_width = self.pitch - self.ruby_gutter;
            let center = left + base_width / 2.0;
            let right = left + base_width;
            let mut y = settings.top_margin() as f32;
            let mut positions = Vec::with_capacity(end - start);
            let first_glyph = self.glyphs.len();
            for cluster in &shaped.clusters[start..end] {
                for glyph in &mut shaped.glyphs[cluster.glyphs.clone()] {
                    glyph.x += center;
                    glyph.y += y;
                }
                let bottom = y + cluster.advance;
                positions.push(VisibleCluster {
                    bytes: cluster.bytes.clone(),
                    top: y,
                    bottom,
                });
                y = bottom;
            }
            let begin = shaped.clusters[start].glyphs.start;
            let stop = shaped.clusters[end - 1].glyphs.end;
            self.glyphs.extend_from_slice(&shaped.glyphs[begin..stop]);
            while let Some(overlay) = ruby.next_if(|overlay| overlay.base.start < end) {
                if overlay.base.start < start || overlay.base.end > end {
                    return Err("A ruby base cannot fit in one vertical column".into());
                }
                let first = &positions[overlay.base.start - start];
                let last = &positions[overlay.base.end - start - 1];
                let x = right + self.ruby_gutter / 2.0;
                let y = first.top + (last.bottom - first.top - overlay.height) / 2.0;
                self.glyphs.extend(overlay.glyphs.iter().map(|glyph| Glyph {
                    x: glyph.x + x,
                    y: glyph.y + y,
                    ..*glyph
                }));
            }
            self.columns.push(Column {
                block: block.key,
                left,
                right,
                clusters: positions,
                glyphs: first_glyph..self.glyphs.len(),
            });
            lines.push(Line {
                block: block.key,
                bytes: shaped.clusters[start].bytes.start..shaped.clusters[end - 1].bytes.end,
                spans: None,
                top: logical_top + slot as f32 * logical_pitch,
                bottom: logical_end,
                page_extension: [0.0; 2],
                missing_glyphs: 0,
            });
            start = end;
        }
        Ok(())
    }

    fn ruby_overlays(
        &mut self,
        block: &LayoutBlock<'_>,
        base: &mut Shaped,
        styles: &[Style],
        fonts: &mut FontContext,
        settings: Settings,
        limit: usize,
    ) -> Result<Vec<RubyOverlay>> {
        if block.ruby.len() > MAX_RUBY_PER_BLOCK
            || block
                .ruby
                .iter()
                .map(|ruby| ruby.annotation.len())
                .sum::<usize>()
                > MAX_RUBY_TEXT
        {
            return Err("A paragraph exceeds the vertical ruby limit".into());
        }
        let mut result = Vec::with_capacity(block.ruby.len());
        let mut remaining = limit - base.glyphs.len();
        let mut previous = 0;
        for ruby in block.ruby.iter() {
            let start = base
                .clusters
                .partition_point(|cluster| cluster.bytes.end <= ruby.bytes.start);
            let end = base
                .clusters
                .partition_point(|cluster| cluster.bytes.start < ruby.bytes.end);
            let first = base
                .clusters
                .get(start)
                .ok_or("The vertical ruby base is unavailable")?;
            if start < previous
                || start >= end
                || first.bytes.start != ruby.bytes.start
                || base.clusters[end - 1].bytes.end != ruby.bytes.end
            {
                return Err("The ruby base does not align to vertical text boundaries".into());
            }
            previous = end;
            for cluster in &mut base.clusters[start + 1..end] {
                cluster.unsafe_start = true;
            }
            let base_height = base.clusters[start..end]
                .iter()
                .map(|cluster| cluster.advance)
                .sum::<f32>();
            if base_height <= 0.0 || base_height > self.page_span {
                return Err("A ruby base cannot fit in one vertical column".into());
            }
            let text = Text {
                value: &ruby.annotation,
                spans: &[],
                element: ruby.element,
                language: ruby.language.or(block.language),
                scale: RUBY_SCALE,
            };
            let runs = font_runs(text, styles, fonts, &mut self.faces, settings)?;
            let mut annotation = shape_block(
                &ruby.annotation,
                &runs,
                &self.faces,
                remaining,
                settings.effective_font_kerning_mode(),
            )?;
            for glyph in &mut annotation.glyphs {
                glyph.painted &= ruby.painted;
            }
            remaining -= annotation.glyphs.len();
            let height = annotation
                .clusters
                .iter()
                .map(|cluster| cluster.advance)
                .sum::<f32>();
            if height <= 0.0 {
                return Err("The vertical ruby annotation is empty".into());
            }
            let factor = (base_height / height).min(1.0);
            if factor < MIN_RUBY_REDUCTION {
                return Err("A vertical ruby annotation is too long for its base".into());
            }
            let mut y = 0.0;
            for cluster in &annotation.clusters {
                for glyph in &mut annotation.glyphs[cluster.glyphs.clone()] {
                    glyph.x *= factor;
                    glyph.y = (glyph.y + y) * factor;
                    glyph.size *= factor;
                }
                y += cluster.advance;
            }
            result.push(RubyOverlay {
                base: start..end,
                glyphs: annotation.glyphs,
                height: height * factor,
            });
        }
        Ok(result)
    }

    fn column_range(&self, range: Range<f32>) -> Range<usize> {
        let page = (range.start / self.page_span).floor() as usize;
        let start = page
            .saturating_mul(self.columns_per_page)
            .min(self.columns.len());
        start
            ..start
                .saturating_add(self.columns_per_page)
                .min(self.columns.len())
    }

    pub(crate) fn capture(&mut self, range: Range<f32>, settings: Settings) -> Result<Vec<u8>> {
        self.cache.begin_paint();
        let stride = settings.width as usize;
        let bounds = settings.text_columns();
        let top = settings.top_margin() as usize;
        let bottom = (settings.height - settings.bottom_margin()) as usize;
        let mut frame = vec![255; stride * settings.height as usize];
        let page = &mut frame[top * stride..bottom * stride];
        let columns = self.column_range(range);
        for column in &self.columns[columns] {
            for glyph in &self.glyphs[column.glyphs.clone()] {
                if !glyph.painted {
                    continue;
                }
                self.cache.draw_vertical(
                    page,
                    stride,
                    &bounds,
                    PaintGlyph {
                        font: &self.faces[glyph.face].font,
                        id: glyph.id,
                        size: glyph.size,
                        x: glyph.x,
                        y: glyph.y - top as f32,
                        clockwise: glyph.clockwise,
                        strength: glyph.strength,
                        variations: &self.faces[glyph.face].variations,
                    },
                    settings.text_contrast,
                    settings.font_gamma_index,
                );
            }
        }
        self.cache.check_paint()?;
        Ok(frame)
    }

    pub(crate) fn source_at(&self, range: Range<f32>, x: f32, y: f32) -> Option<(usize, usize)> {
        let (block, cluster) = self.cluster_at(range, x, y)?;
        Some((
            block,
            self.source_offset(block, cluster.bytes.start, Affinity::After)?,
        ))
    }

    pub(crate) fn caret_at(&self, range: Range<f32>, x: f32, y: f32) -> Option<(usize, usize)> {
        let (block, cluster) = self.cluster_at(range, x, y)?;
        Some((
            block,
            if y - cluster.top < cluster.bottom - y {
                self.source_offset(block, cluster.bytes.start, Affinity::After)?
            } else {
                self.source_offset(block, cluster.bytes.end, Affinity::Before)?
            },
        ))
    }

    fn source_offset(&self, block: usize, offset: usize, affinity: Affinity) -> Option<usize> {
        self.mappings.get(&block).map_or(Some(offset), |mapping| {
            mapping.source_offset(offset, affinity)
        })
    }

    fn cluster_at(&self, range: Range<f32>, x: f32, y: f32) -> Option<(usize, &VisibleCluster)> {
        let column = self.columns[self.column_range(range)]
            .iter()
            .find(|column| (column.left..column.right).contains(&x))?;
        let index = column
            .clusters
            .partition_point(|cluster| cluster.bottom <= y);
        let cluster = column.clusters.get(index)?;
        (cluster.top..cluster.bottom)
            .contains(&y)
            .then_some((column.block, cluster))
    }

    pub(crate) fn selection_rects(
        &self,
        range: Range<f32>,
        block: usize,
        bytes: Range<usize>,
    ) -> Vec<HighlightRect> {
        self.columns[self.column_range(range)]
            .iter()
            .filter(|column| column.block == block)
            .filter_map(|column| {
                let first = column
                    .clusters
                    .partition_point(|cluster| cluster.bytes.end <= bytes.start);
                let end = column
                    .clusters
                    .partition_point(|cluster| cluster.bytes.start < bytes.end);
                let selected = column.clusters.get(first..end)?;
                let y = selected.first()?.top.floor() as i32;
                let bottom = selected.last()?.bottom.ceil() as i32;
                let x = column.left.floor() as i32;
                let right = column.right.ceil() as i32;
                (bottom > y).then_some(HighlightRect {
                    x,
                    y,
                    width: right - x,
                    height: bottom - y,
                })
            })
            .collect()
    }
}

fn column_end(clusters: &[Cluster], legal: &[usize], start: usize, height: f32) -> Result<usize> {
    let mut fit = start;
    let mut safe = start;
    let mut preferred = start;
    let mut advance = 0.0;
    while let Some(cluster) = clusters.get(fit) {
        if advance + cluster.advance > height {
            break;
        }
        advance += cluster.advance;
        fit += 1;
        if clusters.get(fit).is_none_or(|next| !next.unsafe_start) {
            safe = fit;
            if legal.binary_search(&cluster.bytes.end).is_ok() {
                preferred = fit;
            }
        }
    }
    if fit == clusters.len() {
        return Ok(fit);
    }
    let end = if preferred > start { preferred } else { safe };
    if end == start {
        return Err("A shaped text run is taller than one vertical column".into());
    }
    Ok(end)
}

fn font_runs(
    input: Text<'_>,
    styles: &[Style],
    fonts: &mut FontContext,
    faces: &mut Vec<Face>,
    settings: Settings,
) -> Result<Vec<Run>> {
    let boundary = GraphemeClusterSegmenter::new().segment_str(input.value);
    let script_data = CodePointMapData::<Script>::new();
    let ja: Language = "ja".parse().expect("Japanese language tag");
    let mut runs: Vec<Run> = Vec::new();
    let mut style_index = 0;
    let mut preceding_script = Script::Common;
    let mut start = 0;
    for end in boundary.skip(1) {
        let text = &input.value[start..end];
        let script = text
            .chars()
            .filter_map(|ch| {
                let script = script_data.get(ch);
                (!matches!(script, Script::Common | Script::Inherited)).then_some(script)
            })
            .next()
            .unwrap_or(preceding_script);
        if !matches!(
            script,
            Script::Common | Script::Han | Script::Hiragana | Script::Katakana | Script::Latin
        ) {
            return Err("This script is outside Reader's initial vertical writing support".into());
        }
        preceding_script = script;
        while input
            .spans
            .get(style_index)
            .is_some_and(|style| style.bytes.end <= start)
        {
            style_index += 1;
        }
        let styled = input
            .spans
            .get(style_index)
            .filter(|style| style.bytes.contains(&start));
        let element = styled.map_or(input.element, |style| style.element);
        let language = styled
            .and_then(|item| item.language)
            .or(input.language)
            .unwrap_or(ja);
        let orientation = orientation(text);
        let format = Format {
            element,
            orientation,
            language,
            script,
        };
        let previous = runs.last_mut();
        if let Some(run) = previous
            && run.format == format
            && faces[run.face].supports(text)
        {
            run.bytes.end = end;
        } else {
            let style = &styles[element];
            let face = find_face(fonts, faces, style, settings, &format, text)?;
            let size = if settings.logical_font_size.is_some() {
                settings.styled_font(input.scale * style.size_scale)
            } else {
                (settings.font_size * input.scale * style.size_scale.clamp(0.75, MAX_SCALE))
                    .clamp(1.0, 144.0)
            };
            let requested = crate::font_render::font_weight(
                if settings.bold_text {
                    FontWeight::BOLD.value()
                } else {
                    style.font_weight.value()
                },
                settings.font_weight_delta,
            );
            let strength = crate::font_synthesis::strength(
                &faces[face].font,
                requested,
                size,
                &style.font_weights,
            );
            runs.push(Run {
                bytes: start..end,
                format,
                face,
                size,
                strength,
                painted: style.flow.painted,
            });
        }
        start = end;
    }
    Ok(runs)
}

fn ignorable(ch: char) -> bool {
    CodePointSetData::new::<DefaultIgnorableCodePoint>().contains(ch)
}

fn orientation(grapheme: &str) -> Orientation {
    let category = CodePointMapData::<GeneralCategory>::new();
    if grapheme
        .chars()
        .any(|ch| category.get(ch) == GeneralCategory::EnclosingMark)
    {
        return Orientation::Upright;
    }
    let first = grapheme.chars().next().expect("nonempty grapheme");
    if CodePointMapData::<VerticalOrientation>::new().get(first) == VerticalOrientation::Rotated {
        Orientation::Sideways
    } else {
        Orientation::Upright
    }
}

fn find_face(
    fonts: &mut FontContext,
    faces: &mut Vec<Face>,
    style: &Style,
    settings: Settings,
    format: &Format,
    text: &str,
) -> Result<usize> {
    let mut query = fonts.collection.query(&mut fonts.source_cache);
    let primary = if settings.override_fonts {
        QueryFamily::Generic(GenericFamily::Serif)
    } else {
        match &style.font_family {
            Family::Generic(family) => QueryFamily::Generic(*family),
            Family::Named(name) => QueryFamily::Named(name),
        }
    };
    query.set_families([
        primary,
        QueryFamily::Generic(GenericFamily::Serif),
        QueryFamily::Generic(GenericFamily::SansSerif),
    ]);
    let weight = FontWeight::new(crate::font_render::font_weight(
        if settings.bold_text {
            FontWeight::BOLD.value()
        } else {
            style.font_weight.value()
        },
        settings.font_weight_delta,
    ));
    query.set_attributes(Attributes::new(FontWidth::NORMAL, style.font_style, weight));
    let script = if format.orientation == Orientation::Upright {
        *b"Hani"
    } else {
        *b"Latn"
    };
    query.set_fallbacks(FallbackKey::new(
        parley::fontique::Script::from_bytes(script),
        Some(&format.language),
    ));
    let mut selected = None;
    let mut inspected = 0;
    query.matches_with(|candidate| {
        inspected += 1;
        if inspected > 32 {
            return QueryStatus::Stop;
        }
        if candidate.blob.len() > MAX_FONT_BYTES
            || candidate.charmap().is_none_or(|map| {
                !text
                    .chars()
                    .all(|ch| ignorable(ch) || map.map(ch).is_some_and(|id| id != 0))
            })
        {
            return QueryStatus::Continue;
        }
        let Ok(font) = skrifa::FontRef::from_index(candidate.blob.as_ref(), candidate.index) else {
            return QueryStatus::Continue;
        };
        if format.orientation == Orientation::Upright
            && (font.vhea().is_err() || font.vmtx().is_err())
        {
            return QueryStatus::Continue;
        }
        selected = Some(candidate.clone());
        QueryStatus::Stop
    });
    let candidate = selected.ok_or_else(|| {
        if format.orientation == Orientation::Upright {
            "No available font covers this vertical text with OpenType vertical metrics"
        } else {
            "No available font covers this sideways text"
        }
    })?;
    let data = FontData::new(candidate.blob, candidate.index);
    let font = skrifa::FontRef::from_index(data.data.as_ref(), data.index)?;
    let variations = font
        .axes()
        .location(candidate.synthesis.variation_settings().iter().copied())
        .coords()
        .to_vec();
    if let Some(index) = faces.iter().position(|face| {
        face.font.data.id() == data.data.id()
            && face.font.index == data.index
            && face.variations == variations
    }) {
        return Ok(index);
    }
    if faces.len() == MAX_FACES {
        return Err("The chapter exceeds the vertical font face limit".into());
    }
    let max_bytes = faces
        .iter()
        .filter(|face| face.font.data.id() != data.data.id())
        .map(|face| face.font.data.len())
        .sum::<usize>()
        + data.data.len();
    if max_bytes > MAX_FONT_BYTES {
        return Err("The chapter exceeds the vertical font storage limit".into());
    }
    let metrics = font.metrics(Size::unscaled(), LocationRef::new(&variations));
    let em = f32::from(metrics.units_per_em);
    if em < 16.0 {
        return Err("A vertical font has invalid metrics".into());
    }
    let horizontal_center = (metrics.ascent + metrics.descent) / (2.0 * em);
    let shaper = ShaperData::new(&FontRef::from_index(data.data.as_ref(), data.index)?);
    let index = faces.len();
    faces.push(Face {
        font: data,
        charmap: candidate.charmap_index,
        shaper,
        em,
        horizontal_center,
        variations,
    });
    Ok(index)
}

fn shape_block(
    text: &str,
    runs: &[Run],
    faces: &[Face],
    limit: usize,
    kerning: crate::FontKerningMode,
) -> Result<Shaped> {
    let mut shaped = Shaped::default();
    let features = kerning_features(kerning);
    for run in runs {
        let face = &faces[run.face];
        let font = FontRef::from_index(face.font.data.as_ref(), face.font.index)?;
        let instance =
            harfrust::ShaperInstance::from_coords(&font, face.variations.iter().copied());
        let shaper = face.shaper.shaper(&font).instance(Some(&instance)).build();
        let mut buffer = UnicodeBuffer::new();
        for (at, ch) in text[run.bytes.clone()].char_indices() {
            buffer.add(ch, (run.bytes.start + at) as u32);
        }
        let sideways = run.format.orientation == Orientation::Sideways;
        buffer.set_direction(if sideways {
            Direction::LeftToRight
        } else {
            Direction::TopToBottom
        });
        if let Ok(language) = run.format.language.as_str().parse() {
            buffer.set_language(language);
        }
        buffer.guess_segment_properties();
        let options = ShapeOptions::new();
        let options = if features.is_empty() {
            options
        } else {
            options.features(&features)
        };
        let output = shaper.shape(buffer, options);
        if output.glyph_infos().len() > limit - shaped.glyphs.len() {
            return Err("The chapter exceeds the vertical glyph limit".into());
        }
        let scale = run.size / face.em;
        let mut current = None::<usize>;
        let mut pen = 0.0;
        for (info, pos) in output.glyph_infos().iter().zip(output.glyph_positions()) {
            let start = info.cluster as usize;
            if info.glyph_id == 0 || !run.bytes.contains(&start) || !text.is_char_boundary(start) {
                return Err(
                    "The vertical shaper returned an unavailable glyph or text boundary".into(),
                );
            }
            if current != Some(start) {
                if let Some(previous) = current {
                    if previous >= start {
                        return Err("The vertical shaper returned unordered clusters".into());
                    }
                    shaped.clusters.last_mut().unwrap().bytes.end = start;
                } else if start != run.bytes.start {
                    return Err("The vertical shaper dropped part of the source text".into());
                }
                let glyph = shaped.glyphs.len();
                shaped.clusters.push(Cluster {
                    bytes: start..run.bytes.end,
                    glyphs: glyph..glyph,
                    advance: 0.0,
                    unsafe_start: false,
                });
                current = Some(start);
                pen = 0.0;
            }
            let dx = pos.x_offset as f32 * scale;
            let dy = pos.y_offset as f32 * scale;
            let natural_advance = if sideways {
                pos.x_advance as f32 * scale
            } else {
                -(pos.y_advance as f32) * scale
            };
            let advance = if natural_advance > 0.0 {
                (natural_advance + run.strength).max(0.0)
            } else {
                natural_advance
            };
            if !advance.is_finite()
                || !(0.0..=576.0).contains(&advance)
                || dx.abs() > 4_096.0
                || dy.abs() > 4_096.0
            {
                return Err("The vertical font has unsupported glyph metrics".into());
            }
            let (x, y) = if sideways {
                (dy - face.horizontal_center * run.size, pen + dx)
            } else {
                (dx, pen - dy)
            };
            shaped.glyphs.push(Glyph {
                id: info.glyph_id,
                face: run.face,
                size: run.size,
                x,
                y,
                clockwise: sideways,
                strength: run.strength,
                painted: run.painted,
            });
            pen += advance;
            let cluster = shaped.clusters.last_mut().unwrap();
            cluster.advance += advance;
            cluster.glyphs.end += 1;
            cluster.unsafe_start |= info.unsafe_to_break() && start != run.bytes.start;
        }
        if current.is_none() {
            return Err("The vertical shaper returned no source clusters".into());
        }
    }
    Ok(shaped)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logical_vertical_pitch_uses_actual_text_not_empty_oversized_ancestors() {
        let source =
            Source::parse("<html><body><div><p>VERTICAL TEXT</p></div></body></html>").unwrap();
        let settings = Settings {
            width: 440,
            height: 800,
            margin: 24,
            ..Settings::default()
        }
        .with_logical_font(22.)
        .unwrap();
        let mut styles = vec![Style::default(); source.elements.len()];
        let fonts =
            || crate::font_context(&[crate::test_fixture("fonts/NotoSans-Regular.ttf")]).unwrap();
        let (mut ordinary, lines, extent) =
            VerticalChapter::new(&source, &styles, fonts(), settings).unwrap();
        let div = source.elements.iter().position(|e| e.tag == "div").unwrap();
        styles[div].size_scale = 8.;
        let (mut nested, nested_lines, nested_extent) =
            VerticalChapter::new(&source, &styles, fonts(), settings).unwrap();
        assert_eq!(nested.pitch, ordinary.pitch);
        assert_eq!(nested.columns_per_page, ordinary.columns_per_page);
        assert_eq!(nested_extent, extent);
        assert_eq!(
            lines.iter().map(|l| &l.bytes).collect::<Vec<_>>(),
            nested_lines.iter().map(|l| &l.bytes).collect::<Vec<_>>()
        );
        assert_eq!(
            ordinary
                .capture(0.0..settings.text_height() as f32, settings)
                .unwrap(),
            nested
                .capture(0.0..settings.text_height() as f32, settings)
                .unwrap()
        );
    }

    #[test]
    fn character_orientation_handles_marks_and_japanese_punctuation() {
        for text in ["日", "、", "「", "か\u{3099}", "A\u{20dd}"] {
            assert_eq!(orientation(text), Orientation::Upright);
        }
        assert_eq!(orientation("A"), Orientation::Sideways);
    }

    #[test]
    fn column_break_prefers_legal_boundaries_and_respects_unsplittable_runs() {
        let mut clusters: Vec<_> = (0..5)
            .map(|n| Cluster {
                bytes: n..n + 1,
                glyphs: n..n + 1,
                advance: 10.0,
                unsafe_start: false,
            })
            .collect();
        assert_eq!(column_end(&clusters, &[0, 2, 5], 0, 35.0).unwrap(), 2);
        clusters[2].unsafe_start = true;
        clusters[3].unsafe_start = true;
        assert_eq!(column_end(&clusters, &[0, 2, 5], 0, 35.0).unwrap(), 1);
        assert!(column_end(&clusters, &[], 1, 25.0).is_err());
    }

    #[test]
    fn ruby_keeps_its_base_in_one_column_without_changing_source_geometry() {
        let source = Source::parse("<html><body><p>ONE TWO THREE FOUR <ruby>BOOK<rt>READ</rt></ruby> LAST</p></body></html>").unwrap();
        let settings = Settings {
            width: 320,
            height: 184,
            font_size: 18.0,
            margin: 24,
            ..Settings::default()
        };
        let fonts =
            crate::font_context(&[crate::test_fixture("fonts/Silkscreen-Regular.ttf")]).unwrap();
        let styles = vec![Style::default(); source.elements.len()];
        let (mut layout, lines, extent) =
            VerticalChapter::new(&source, &styles, fonts, settings).unwrap();
        let block = &source.blocks[0];
        let base = &block.ruby[0].bytes;
        assert_eq!(&block.text[base.clone()], "BOOK");
        assert!(!block.text.contains("READ"));
        assert!(layout.ruby_gutter > 0.0);
        let column = layout
            .columns
            .iter()
            .find(|column| {
                column
                    .clusters
                    .iter()
                    .any(|cluster| cluster.bytes.start == base.start)
            })
            .unwrap();
        let first = column
            .clusters
            .iter()
            .find(|cluster| cluster.bytes.start == base.start)
            .unwrap();
        assert!(
            column
                .clusters
                .iter()
                .any(|cluster| cluster.bytes.end == base.end)
        );
        assert!(
            layout.glyphs[column.glyphs.clone()]
                .iter()
                .any(|glyph| glyph.x > column.right)
        );
        let index = layout
            .columns
            .iter()
            .position(|candidate| std::ptr::eq(candidate, column))
            .unwrap();
        let page = (index / layout.columns_per_page) as f32 * layout.page_span;
        let range = page..page + layout.page_span;
        let center_x = (column.left + column.right) / 2.0;
        assert_eq!(
            layout.source_at(range.clone(), center_x, first.top + 1.0),
            Some((block.key, base.start))
        );
        assert_eq!(
            layout
                .selection_rects(range.clone(), block.key, base.clone())
                .len(),
            1
        );
        assert!(
            layout
                .capture(range, settings)
                .unwrap()
                .iter()
                .any(|&pixel| pixel < 128)
        );
        assert!(lines.iter().all(|line| line.bottom <= extent));
    }
}
