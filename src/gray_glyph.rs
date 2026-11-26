mod tiled;
use std::collections::{HashMap, hash_map::Entry};
use std::ops::Range;
use std::sync::Arc;

use ab_glyph_rasterizer::{Point, Rasterizer, point};
use parley::{FontData, GlyphRun};
use skrifa::{
    FontRef, GlyphId, MetadataProvider,
    instance::{NormalizedCoord, Size},
    outline::{
        DrawSettings, Engine, HintingInstance, HintingOptions, OutlinePen, SmoothMode, Target,
    },
};

use crate::FontHinting;

const CACHE_BYTES: usize = 2 * 1024 * 1024;
const ENTRY_OVERHEAD: usize = 128;
const MAX_FACES: usize = 64;
const MAX_AXES: usize = 64;
const MAX_SEGMENTS: usize = 8192;
const MAX_EDGE: usize = 300;
const MAX_OUTLINE_OFFSET: f32 = 4_096.0;
const MAX_MASK_BYTES: usize = 64 * 1024;
const X_PHASES: i32 = 4;
const MAX_VIEWPORT_EDGE: usize = 4_096;

#[derive(Hash, PartialEq, Eq)]
struct FaceKey {
    font: u64,
    index: u32,
    size: u32,
    variations: Vec<i16>,
    skew: u32,
    clockwise: bool,
    strength: u32,
}

struct GlyphStyle<'a> {
    font: &'a FontData,
    size: f32,
    variations: Vec<i16>,
    skew: f32,
    clockwise: bool,
    strength: f32,
}

#[derive(Clone, Copy)]
struct Ink<'a> {
    gray: u8,
    contrast: bool,
    gamma: &'a [u8; 256],
}

#[derive(Default)]
struct Mask {
    left: i32,
    top: i32,
    width: usize,
    alpha: Vec<u8>,
}

/// A chapter-local cache of compact grayscale masks for scalable outlines.
/// Horizontal positions retain quarter-pixel phases. Baselines snap to pixels.
#[derive(Default)]
pub(crate) struct GlyphCache {
    large_outlines: bool,
    paint_error: Option<&'static str>,
    hinting: FontHinting,
    font_weights: Arc<HashMap<u64, f32>>,
    faces: HashMap<FaceKey, HashMap<(u32, u8), Mask>>,
    bytes: usize,
    full: bool,
}

impl GlyphCache {
    #[cfg(feature = "kindle")]
    pub(crate) fn release_masks(&mut self) {
        self.faces.clear();
        self.bytes = 0;
        self.full = false;
        self.paint_error = None;
    }

    pub(crate) fn with_hinting(
        large_outlines: bool,
        hinting: FontHinting,
        font_weights: Arc<HashMap<u64, f32>>,
    ) -> Self {
        Self {
            large_outlines,
            hinting,
            font_weights,
            ..Self::default()
        }
    }

    pub(crate) fn begin_paint(&mut self) {
        self.paint_error = None;
    }
    pub(crate) fn check_paint(&self) -> crate::Result<()> {
        match self.paint_error {
            Some(message) => Err(message.into()),
            None => Ok(()),
        }
    }

    pub(crate) fn draw_run(
        &mut self,
        page: &mut [u8],
        stride: usize,
        columns: &Range<usize>,
        origin: (f32, f32),
        glyphs: &GlyphRun<'_, crate::parley_layout::PaintBrush>,
        contrast: bool,
        gamma: u8,
    ) {
        let run = glyphs.run();
        let synthesis = run.synthesis();
        let style = GlyphStyle {
            font: run.font(),
            size: run.font_size(),
            variations: run
                .normalized_coords()
                .iter()
                .map(|coord| coord.to_bits())
                .collect(),
            skew: synthesis.skew().unwrap_or(0.0),
            clockwise: false,
            strength: crate::font_synthesis::strength(
                run.font(),
                run.font_attrs().weight.value(),
                run.font_size(),
                &self.font_weights,
            ),
        };
        let positions = glyphs
            .positioned_glyphs()
            .map(|glyph| (glyph.id, origin.0 + glyph.x, origin.1 + glyph.y));
        let ink = Ink {
            gray: glyphs
                .style()
                .brush
                .link_ink
                .or_else(|| {
                    if contrast {
                        None
                    } else {
                        glyphs.style().brush.text_ink
                    }
                })
                .unwrap_or(0),
            contrast,
            gamma: crate::font_render::gamma_table(gamma),
        };
        self.draw_positioned(page, stride, columns, style, positions, ink);
    }

    pub(crate) fn draw_vertical(
        &mut self,
        page: &mut [u8],
        stride: usize,
        columns: &Range<usize>,
        glyph: crate::vertical_layout::PaintGlyph<'_>,
        contrast: bool,
        gamma: u8,
    ) {
        let style = GlyphStyle {
            font: glyph.font,
            size: glyph.size,
            variations: glyph
                .variations
                .iter()
                .map(|coord| coord.to_bits())
                .collect(),
            skew: 0.0,
            clockwise: glyph.clockwise,
            strength: glyph.strength,
        };
        let position = (glyph.id, glyph.x - columns.start as f32, glyph.y);
        let ink = Ink {
            gray: glyph.ink,
            contrast,
            gamma: crate::font_render::gamma_table(gamma),
        };
        self.draw_positioned(page, stride, columns, style, std::iter::once(position), ink);
    }

    fn draw_positioned(
        &mut self,
        page: &mut [u8],
        stride: usize,
        columns: &Range<usize>,
        style: GlyphStyle<'_>,
        glyphs: impl Iterator<Item = (u32, f32, f32)>,
        ink: Ink<'_>,
    ) {
        if stride == 0
            || stride > MAX_VIEWPORT_EDGE
            || page.len() / stride > MAX_VIEWPORT_EDGE
            || columns.is_empty()
            || columns.end > stride
            || style.variations.len() > MAX_AXES
            || !(1.0..=if self.large_outlines {
                340.0
            } else {
                MAX_EDGE as f32
            })
                .contains(&style.size)
        {
            if self.large_outlines {
                self.paint_error = Some("A font run exceeds Reader's paint bounds");
            }
            return;
        }
        if self.full || self.faces.len() >= MAX_FACES {
            self.faces.clear();
            self.bytes = 0;
            self.full = false;
        }

        let key = FaceKey {
            font: style.font.data.id(),
            index: style.font.index,
            size: style.size.to_bits(),
            variations: style.variations.clone(),
            skew: style.skew.to_bits(),
            clockwise: style.clockwise,
            strength: style.strength.to_bits(),
        };
        let face = self.faces.entry(key).or_default();
        let mut scaled_variations = None;
        // Reuse one hinting instance for misses in this run, without keeping additional
        // font-sized allocations in the bound on cached masks.
        let mut hinter = None;
        let reach = MAX_OUTLINE_OFFSET + MAX_EDGE as f32;
        let x_bounds = -reach..=stride as f32 + reach;
        let y_bounds = -reach..=(page.len() / stride) as f32 + reach;
        for (glyph, x, baseline) in glyphs {
            if !x_bounds.contains(&x) || !y_bounds.contains(&baseline) {
                continue;
            }
            let x = (x * X_PHASES as f32).round() as i32;
            let phase = x.rem_euclid(X_PHASES) as u8;
            let x = x.div_euclid(X_PHASES);
            let baseline = baseline.round() as i32;
            match face.entry((glyph, phase)) {
                Entry::Occupied(entry) => {
                    entry.get().blend(page, stride, columns, x, baseline, ink)
                }
                Entry::Vacant(entry) => {
                    let variations = scaled_variations.get_or_insert_with(|| {
                        style
                            .variations
                            .iter()
                            .map(|&coord| NormalizedCoord::from_bits(coord))
                            .collect::<Vec<_>>()
                    });
                    let hints = hinter
                        .get_or_insert_with(|| hinting_instance(self.hinting, &style, variations));
                    let mask = match rasterize(&style, glyph, phase, variations, hints.as_ref()) {
                        Some(mask) => mask,
                        None if self.large_outlines => {
                            if let Err(error) = tiled::paint(
                                page,
                                stride,
                                columns,
                                x,
                                baseline,
                                &style,
                                glyph,
                                phase,
                                variations,
                                hints.as_ref(),
                                ink,
                            ) {
                                self.paint_error = Some(error);
                            }
                            continue;
                        }
                        None => Mask::default(),
                    };
                    mask.blend(page, stride, columns, x, baseline, ink);
                    let cost = mask.alpha.len() + ENTRY_OVERHEAD;
                    if self.bytes + cost <= CACHE_BYTES {
                        self.bytes += cost;
                        entry.insert(mask);
                    } else {
                        self.full = true;
                    }
                }
            }
        }
    }
}

impl Mask {
    fn blend(
        &self,
        page: &mut [u8],
        stride: usize,
        columns: &Range<usize>,
        x: i32,
        y: i32,
        ink: Ink<'_>,
    ) {
        if self.alpha.is_empty() {
            return;
        }
        let x = x + self.left + columns.start as i32;
        let y = y + self.top;
        let visible_left = x.max(columns.start as i32);
        let visible_right = (x + self.width as i32).min(columns.end as i32);
        if visible_right <= visible_left {
            return;
        }
        let visible_top = y.max(0);
        let visible_bottom =
            (y + (self.alpha.len() / self.width) as i32).min((page.len() / stride) as i32);
        let width = (visible_right - visible_left) as usize;
        for row in visible_top..visible_bottom {
            let dst = row as usize * stride + visible_left as usize;
            let src = (row - y) as usize * self.width + (visible_left - x) as usize;
            let mask = &self.alpha[src..src + width];
            for (pixel, &alpha) in page[dst..dst + width].iter_mut().zip(mask) {
                let alpha = if ink.contrast {
                    alpha.saturating_add(alpha / 2)
                } else {
                    alpha
                };
                let alpha = ink.gamma[usize::from(alpha)];
                if ink.gray == 0 && *pixel == 255 {
                    *pixel = 255 - alpha;
                } else if alpha != 0 {
                    let alpha = u16::from(alpha);
                    let gray = u16::from(ink.gray);
                    *pixel = ((u16::from(*pixel) * (255 - alpha) + gray * alpha + 127) / 255) as u8;
                }
            }
        }
    }
}

enum Segment {
    Move(Point),
    Line(Point),
    Quad(Point, Point),
    Cubic(Point, Point, Point),
    Close,
}

fn synthesize_outline(outline: &Outline, strength: f32, clockwise: bool) -> Option<Mask> {
    use kurbo::{BezPath, Join, PathEl, Point as CurvePoint, Shape, Stroke, StrokeOpts};
    if !strength.is_finite()
        || strength.abs() > 32.0
        || outline.max.x - outline.min.x > MAX_EDGE as f32
        || outline.max.y - outline.min.y > MAX_EDGE as f32
    {
        return None;
    }
    // Center the vertical stroke expansion while preserving the reference's
    // horizontal bearing convention. A clockwise run exchanges the two axes.
    let offset = f64::from(strength) / 2.0;
    let point = |p: Point| {
        CurvePoint::new(
            f64::from(p.x) + if clockwise { 0.0 } else { offset },
            f64::from(p.y) + if clockwise { offset } else { 0.0 },
        )
    };
    let path = BezPath::from_vec(
        outline
            .segments
            .iter()
            .map(|segment| match segment {
                Segment::Move(p) => PathEl::MoveTo(point(*p)),
                Segment::Line(p) => PathEl::LineTo(point(*p)),
                Segment::Quad(a, p) => PathEl::QuadTo(point(*a), point(*p)),
                Segment::Cubic(a, b, p) => PathEl::CurveTo(point(*a), point(*b), point(*p)),
                Segment::Close => PathEl::ClosePath,
            })
            .collect(),
    );
    let border = kurbo::stroke(
        path.elements().iter().copied(),
        &Stroke::new(f64::from(strength.abs())).with_join(Join::Miter),
        &StrokeOpts::default(),
        0.05,
    );
    if border.elements().len() > MAX_SEGMENTS * 8 {
        return None;
    }
    let bounds = path.bounding_box().union(border.bounding_box());
    let left = bounds.x0.floor();
    let top = bounds.y0.floor();
    let width = (bounds.x1.ceil() - left) as usize;
    let height = (bounds.y1.ceil() - top) as usize;
    if width == 0
        || height == 0
        || width > MAX_EDGE
        || height > MAX_EDGE
        || width * height > MAX_MASK_BYTES
    {
        return None;
    }
    let mut alpha = curve_mask(path.elements(), left, top, width, height);
    let boundary = curve_mask(border.elements(), left, top, width, height);
    for (pixel, edge) in alpha.iter_mut().zip(boundary) {
        let paper = 255 - u16::from(edge);
        *pixel = if strength > 0.0 {
            255 - (((255 - u16::from(*pixel)) * paper + 127) / 255) as u8
        } else {
            ((u16::from(*pixel) * paper + 127) / 255) as u8
        };
    }
    Some(Mask {
        left: left as i32,
        top: top as i32,
        width,
        alpha,
    })
}

pub(crate) fn curve_mask(
    path: &[kurbo::PathEl],
    left: f64,
    top: f64,
    width: usize,
    height: usize,
) -> Vec<u8> {
    let mut raster = Rasterizer::new(width, height);
    let local = |p: kurbo::Point| point((p.x - left) as f32, (p.y - top) as f32);
    let mut first = point(0.0, 0.0);
    let mut previous = first;
    for segment in path {
        match *segment {
            kurbo::PathEl::MoveTo(p) => {
                first = local(p);
                previous = first;
            }
            kurbo::PathEl::LineTo(p) => {
                let p = local(p);
                raster.draw_line(previous, p);
                previous = p;
            }
            kurbo::PathEl::QuadTo(a, p) => {
                let p = local(p);
                raster.draw_quad(previous, local(a), p);
                previous = p;
            }
            kurbo::PathEl::CurveTo(a, b, p) => {
                let p = local(p);
                raster.draw_cubic(previous, local(a), local(b), p);
                previous = p;
            }
            kurbo::PathEl::ClosePath => raster.draw_line(previous, first),
        }
    }
    let mut alpha = vec![0; width * height];
    raster.for_each_pixel(|index, coverage| {
        alpha[index] = (coverage.min(1.0) * 255.0 + 0.5) as u8;
    });
    alpha
}

struct Outline {
    segments: Vec<Segment>,
    min: Point,
    max: Point,
    skew: f32,
    phase: f32,
    clockwise: bool,
    invalid: bool,
}

impl Outline {
    fn coordinate(&mut self, x: f32, y: f32) -> Point {
        let horizontal_x = x + y * self.skew;
        let (x, y) = if self.clockwise {
            (y + self.phase, horizontal_x)
        } else {
            (horizontal_x + self.phase, -y)
        };
        let permitted = -MAX_OUTLINE_OFFSET..=MAX_OUTLINE_OFFSET;
        if !permitted.contains(&x) || !permitted.contains(&y) {
            self.invalid = true;
        }
        self.min = point(self.min.x.min(x), self.min.y.min(y));
        self.max = point(self.max.x.max(x), self.max.y.max(y));
        point(x, y)
    }

    fn push(&mut self, segment: Segment) {
        if self.segments.len() == MAX_SEGMENTS {
            self.invalid = true;
        } else {
            self.segments.push(segment);
        }
    }
}

impl OutlinePen for Outline {
    fn move_to(&mut self, x: f32, y: f32) {
        let p = self.coordinate(x, y);
        self.push(Segment::Move(p));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let p = self.coordinate(x, y);
        self.push(Segment::Line(p));
    }

    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        let control = self.coordinate(cx, cy);
        let end = self.coordinate(x, y);
        self.push(Segment::Quad(control, end));
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        let a = self.coordinate(cx0, cy0);
        let b = self.coordinate(cx1, cy1);
        let end = self.coordinate(x, y);
        self.push(Segment::Cubic(a, b, end));
    }

    fn close(&mut self) {
        self.push(Segment::Close);
    }
}

fn hinting_instance(
    hinting: FontHinting,
    style: &GlyphStyle<'_>,
    variations: &[NormalizedCoord],
) -> Option<HintingInstance> {
    let engine = match hinting {
        FontHinting::Off => return None,
        FontHinting::Native => Engine::Interpreter,
        FontHinting::Auto => Engine::Auto(None),
    };
    let face = FontRef::from_index(style.font.data.data(), style.font.index).ok()?;
    let options = HintingOptions {
        engine,
        target: Target::Smooth {
            mode: SmoothMode::Light,
            symmetric_rendering: false,
            preserve_linear_metrics: true,
        },
    };
    HintingInstance::new(
        &face.outline_glyphs(),
        Size::new(style.size),
        variations,
        options,
    )
    .ok()
}

fn rasterize(
    style: &GlyphStyle<'_>,
    glyph: u32,
    phase: u8,
    variations: &[NormalizedCoord],
    hints: Option<&HintingInstance>,
) -> Option<Mask> {
    let font = style.font;
    let size = style.size;
    let face = FontRef::from_index(font.data.data(), font.index).ok()?;
    let glyphs = face.outline_glyphs();
    let glyph = glyphs.get(GlyphId::new(glyph))?;
    let fresh_outline = || Outline {
        segments: Vec::new(),
        min: point(f32::INFINITY, f32::INFINITY),
        max: point(f32::NEG_INFINITY, f32::NEG_INFINITY),
        skew: style.skew.to_radians().tan(),
        phase: f32::from(phase) / X_PHASES as f32,
        clockwise: style.clockwise,
        invalid: false,
    };
    let mut outline = fresh_outline();
    let unhinted = || DrawSettings::unhinted(Size::new(size), variations);
    let settings = hints.map_or_else(unhinted, |hints| DrawSettings::hinted(hints, false));
    if glyph.draw(settings, &mut outline).is_err() {
        hints?;
        outline = fresh_outline();
        glyph.draw(unhinted(), &mut outline).ok()?;
    }
    if outline.invalid || outline.segments.is_empty() {
        return None;
    }
    if style.strength != 0.0 {
        return synthesize_outline(&outline, style.strength, style.clockwise);
    }

    let left = outline.min.x.floor();
    let top = outline.min.y.floor();
    let width = (outline.max.x.ceil() - left) as usize;
    let height = (outline.max.y.ceil() - top) as usize;
    if width == 0 || height == 0 || width > MAX_EDGE || height > MAX_EDGE {
        return None;
    }
    if width * height > MAX_MASK_BYTES {
        return None;
    }

    let mut raster = Rasterizer::new(width, height);
    let local = |p: Point| point(p.x - left, p.y - top);
    let mut first = point(0.0, 0.0);
    let mut previous = first;
    for segment in outline.segments {
        match segment {
            Segment::Move(p) => {
                first = local(p);
                previous = first;
            }
            Segment::Line(p) => {
                let p = local(p);
                raster.draw_line(previous, p);
                previous = p;
            }
            Segment::Quad(a, p) => {
                let p = local(p);
                raster.draw_quad(previous, local(a), p);
                previous = p;
            }
            Segment::Cubic(a, b, p) => {
                let p = local(p);
                raster.draw_cubic(previous, local(a), local(b), p);
                previous = p;
            }
            Segment::Close => {
                raster.draw_line(previous, first);
                previous = first;
            }
        }
    }
    let mut alpha = vec![0; width * height];
    raster.for_each_pixel(|index, coverage| {
        alpha[index] = (coverage.min(1.0) * 255.0 + 0.5) as u8;
    });
    Some(Mask {
        left: left as i32,
        top: top as i32,
        width,
        alpha,
    })
}

#[cfg(test)]
mod logical_bound_tests {
    use super::*;
    #[test]
    fn excessive_axes_are_explicit_logical_paint_failures_and_legacy_stays_unchanged() {
        let bytes = std::fs::read(crate::test_fixture("fonts/NotoSans-Regular.ttf")).unwrap();
        let font = FontData::new(parley::fontique::Blob::new(Arc::new(bytes)), 0);
        for logical in [false, true] {
            let mut cache = GlyphCache::with_hinting(logical, FontHinting::Off, Arc::default());
            let style = GlyphStyle {
                font: &font,
                size: 42.,
                variations: vec![0; MAX_AXES + 1],
                skew: 0.,
                clockwise: false,
                strength: 0.,
            };
            let mut frame = vec![255; 500 * 500];
            cache.begin_paint();
            cache.draw_positioned(
                &mut frame,
                500,
                &(0..500),
                style,
                std::iter::once((1, 0., 340.)),
                Ink {
                    gray: 0,
                    contrast: false,
                    gamma: crate::font_render::gamma_table(crate::font_render::NORMAL_GAMMA),
                },
            );
            assert_eq!(cache.check_paint().is_err(), logical);
            assert!(frame.iter().all(|&n| n == 255));
            assert_eq!(cache.bytes, 0);
        }
    }
}
