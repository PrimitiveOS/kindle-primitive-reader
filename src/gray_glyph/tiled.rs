//! Large-outline fallback. Clip closed contours before rasterization: the small
//! rasterizer does not provide arbitrary negative-X clipping for a tile.
use super::*;
use kurbo::{BezPath, Join, PathEl, Point as P, Shape, Stroke, StrokeOpts};

const TILE: usize = 256;
const MAX_FLAT: usize = 131_072;
const MAX_STROKE_ELEMENTS: usize = 65_536;
const MAX_CLIP_VISITS: usize = 16 * 1024 * 1024;
const MAX_STRENGTH: f32 = 54.0; // ceil(340 * (1000-1) / 6400), including 26.6 rounding.
const TOLERANCE: f64 = 0.10;

type Contours = Vec<Vec<P>>;
type PaintError = &'static str;

pub(super) fn paint(
    page: &mut [u8],
    stride: usize,
    columns: &Range<usize>,
    x: i32,
    baseline: i32,
    style: &GlyphStyle<'_>,
    glyph: u32,
    phase: u8,
    variations: &[NormalizedCoord],
    hints: Option<&HintingInstance>,
    ink: Ink<'_>,
) -> std::result::Result<(), PaintError> {
    if !style.strength.is_finite() || style.strength.abs() > MAX_STRENGTH {
        return Err("A font's weight synthesis exceeds the bounded outline range");
    }
    let face = FontRef::from_index(style.font.data.data(), style.font.index)
        .map_err(|_| "The font face could not be painted")?;
    let glyphs = face.outline_glyphs();
    let glyph = glyphs
        .get(GlyphId::new(glyph))
        .ok_or("A font glyph has no supported outline")?;
    let fresh = || Outline {
        segments: Vec::new(),
        min: point(f32::INFINITY, f32::INFINITY),
        max: point(f32::NEG_INFINITY, f32::NEG_INFINITY),
        skew: style.skew.to_radians().tan(),
        phase: f32::from(phase) / X_PHASES as f32,
        clockwise: style.clockwise,
        invalid: false,
    };
    let mut outline = fresh();
    let unhinted = || DrawSettings::unhinted(Size::new(style.size), variations);
    if glyph
        .draw(
            hints.map_or_else(unhinted, |h| DrawSettings::hinted(h, false)),
            &mut outline,
        )
        .is_err()
    {
        outline = fresh();
        glyph
            .draw(unhinted(), &mut outline)
            .map_err(|_| "A font glyph outline could not be painted")?;
    }
    if outline.invalid {
        return Err("A font outline exceeds Reader's coordinate or segment bound");
    }
    // Space and other genuinely empty outlines are valid; a rejected outline is not.
    if outline.segments.is_empty() {
        return Ok(());
    }
    let offset = f64::from(style.strength) / 2.0;
    let convert = |p: Point| {
        P::new(
            f64::from(p.x) + if style.clockwise { 0. } else { offset },
            f64::from(p.y) + if style.clockwise { offset } else { 0. },
        )
    };
    let path = BezPath::from_vec(
        outline
            .segments
            .iter()
            .map(|s| match s {
                Segment::Move(p) => PathEl::MoveTo(convert(*p)),
                Segment::Line(p) => PathEl::LineTo(convert(*p)),
                Segment::Quad(a, p) => PathEl::QuadTo(convert(*a), convert(*p)),
                Segment::Cubic(a, b, p) => PathEl::CurveTo(convert(*a), convert(*b), convert(*p)),
                Segment::Close => PathEl::ClosePath,
            })
            .collect(),
    );
    let contours = flatten(path.elements())?;
    let border = bounded_stroke(&contours, style.strength)?;
    let bounds = if border.is_empty() {
        path.bounding_box()
    } else {
        path.bounding_box().union(border.bounding_box())
    };
    if ![bounds.x0, bounds.y0, bounds.x1, bounds.y1]
        .into_iter()
        .all(f64::is_finite)
        || bounds.x0 < -8192.
        || bounds.y0 < -8192.
        || bounds.x1 > 8192.
        || bounds.y1 > 8192.
    {
        return Err("A font stroke exceeds Reader's coordinate bound");
    }
    let left = (bounds.x0.floor() as i32).max(-x);
    let right = (bounds.x1.ceil() as i32).min(columns.len() as i32 - x);
    let top = (bounds.y0.floor() as i32).max(-baseline);
    let bottom = (bounds.y1.ceil() as i32).min((page.len() / stride) as i32 - baseline);
    if right <= left || bottom <= top {
        return Ok(());
    }
    let strokes = flatten(border.elements())?;
    let mut budget = ClipBudget::default();
    for tile_y in (top..bottom).step_by(TILE) {
        for tile_x in (left..right).step_by(TILE) {
            let width = (right - tile_x).min(TILE as i32) as usize;
            let height = (bottom - tile_y).min(TILE as i32) as usize;
            let mut alpha = tile_mask(&contours, tile_x, tile_y, width, height, &mut budget)?;
            if !strokes.is_empty() {
                let boundary = tile_mask(&strokes, tile_x, tile_y, width, height, &mut budget)?;
                for (pixel, edge) in alpha.iter_mut().zip(boundary) {
                    let paper = 255 - u16::from(edge);
                    *pixel = if style.strength > 0. {
                        255 - (((255 - u16::from(*pixel)) * paper + 127) / 255) as u8
                    } else {
                        ((u16::from(*pixel) * paper + 127) / 255) as u8
                    };
                }
            }
            Mask {
                left: tile_x,
                top: tile_y,
                width,
                alpha,
            }
            .blend(page, stride, columns, x, baseline, ink);
        }
    }
    Ok(())
}

fn flatten(path: &[PathEl]) -> std::result::Result<Contours, PaintError> {
    let mut result = Vec::new();
    let mut contour = Vec::new();
    let mut count = 0;
    for element in path {
        match *element {
            PathEl::MoveTo(p) => {
                if !contour.is_empty() {
                    result.push(std::mem::take(&mut contour));
                }
                append(&mut contour, p, &mut count)?;
            }
            PathEl::LineTo(p) => append(&mut contour, p, &mut count)?,
            PathEl::QuadTo(a, p) => {
                let start = *contour
                    .last()
                    .ok_or("A font contour is missing its start")?;
                quad(start, a, p, 0, &mut contour, &mut count)?;
            }
            PathEl::CurveTo(a, b, p) => {
                let start = *contour
                    .last()
                    .ok_or("A font contour is missing its start")?;
                cubic(start, a, b, p, 0, &mut contour, &mut count)?;
            }
            PathEl::ClosePath => {
                if !contour.is_empty() {
                    result.push(std::mem::take(&mut contour));
                }
            }
        }
    }
    if !contour.is_empty() {
        result.push(contour);
    }
    Ok(result)
}
fn append(out: &mut Vec<P>, p: P, count: &mut usize) -> std::result::Result<(), PaintError> {
    if *count >= MAX_FLAT {
        return Err("A font curve exceeds Reader's subdivision bound");
    }
    out.push(p);
    *count += 1;
    Ok(())
}
fn near(p: P, a: P, b: P) -> bool {
    let v = b - a;
    let length = v.hypot();
    if length == 0. {
        (p - a).hypot() <= TOLERANCE
    } else {
        // Distance to the finite chord, not its infinite supporting line,
        // retains turn-back extrema. The same tolerance also terminates
        // a subpixel extremum at a non-dyadic curve parameter.
        let projection = (p - a).dot(v);
        let closest = a + v * (projection / v.hypot2()).clamp(0., 1.);
        (p - closest).hypot() <= TOLERANCE
    }
}
fn quad(
    a: P,
    b: P,
    c: P,
    depth: u8,
    out: &mut Vec<P>,
    count: &mut usize,
) -> std::result::Result<(), PaintError> {
    if near(b, a, c) {
        return append(out, c, count);
    }
    if depth == 16 {
        return Err("A font curve exceeds Reader's subdivision depth");
    }
    let ab = a.midpoint(b);
    let bc = b.midpoint(c);
    let mid = ab.midpoint(bc);
    quad(a, ab, mid, depth + 1, out, count)?;
    quad(mid, bc, c, depth + 1, out, count)
}
fn cubic(
    a: P,
    b: P,
    c: P,
    d: P,
    depth: u8,
    out: &mut Vec<P>,
    count: &mut usize,
) -> std::result::Result<(), PaintError> {
    if near(b, a, d) && near(c, a, d) {
        return append(out, d, count);
    }
    if depth == 16 {
        return Err("A font curve exceeds Reader's subdivision depth");
    }
    let ab = a.midpoint(b);
    let bc = b.midpoint(c);
    let cd = c.midpoint(d);
    let abc = ab.midpoint(bc);
    let bcd = bc.midpoint(cd);
    let mid = abc.midpoint(bcd);
    cubic(a, ab, abc, mid, depth + 1, out, count)?;
    cubic(mid, bcd, cd, d, depth + 1, out, count)
}

// Kurbo's pinned line-only Miter path uses at most six line/move elements
// per input vertex plus four per closed contour (stroke.rs do_join/do_line/
// finish_closed). No cubic subdivision or round caps occur. Preflight a looser
// 8*(vertices+contours)+8 bound BEFORE constructing any stroke context.
fn bounded_stroke(contours: &Contours, strength: f32) -> std::result::Result<BezPath, PaintError> {
    if strength == 0. {
        return Ok(BezPath::new());
    }
    let vertices = contours.iter().map(Vec::len).sum::<usize>();
    let bound = vertices
        .checked_add(contours.len())
        .and_then(|n| n.checked_mul(8))
        .and_then(|n| n.checked_add(8))
        .filter(|&n| n <= MAX_STROKE_ELEMENTS)
        .ok_or("A font stroke exceeds Reader's construction bound")?;
    let mut polyline = BezPath::new();
    for contour in contours {
        let Some((&first, rest)) = contour.split_first() else {
            continue;
        };
        polyline.move_to(first);
        for &p in rest {
            polyline.line_to(p);
        }
        polyline.close_path();
    }
    let border = kurbo::stroke(
        polyline.elements().iter().copied(),
        &Stroke::new(f64::from(strength.abs())).with_join(Join::Miter),
        &StrokeOpts::default(),
        0.05,
    );
    if border.elements().len() > bound {
        return Err("A font stroke exceeded its verified construction bound");
    }
    Ok(border)
}

#[derive(Default)]
struct ClipBudget {
    visits: usize,
}
impl ClipBudget {
    fn charge(&mut self, count: usize) -> std::result::Result<(), PaintError> {
        self.visits = self
            .visits
            .checked_add(count)
            .filter(|&n| n <= MAX_CLIP_VISITS)
            .ok_or("A visible font outline exceeds Reader's clipping work bound")?;
        Ok(())
    }
}
fn tile_mask(
    contours: &Contours,
    left: i32,
    top: i32,
    width: usize,
    height: usize,
    budget: &mut ClipBudget,
) -> std::result::Result<Vec<u8>, PaintError> {
    if width > TILE || height > TILE {
        return Err("A font tile exceeds Reader's mask bound");
    }
    let mut raster = Rasterizer::new(width, height);
    for contour in contours {
        if contour.len() > MAX_FLAT {
            return Err("A font polygon exceeds Reader's scratch bound");
        }
        budget.charge(contour.len())?;
        let mut polygon = contour.clone();
        for (axis, bound, keep_greater) in [
            (0, f64::from(left), true),
            (0, f64::from(left) + width as f64, false),
            (1, f64::from(top), true),
            (1, f64::from(top) + height as f64, false),
        ] {
            polygon = clip(&polygon, axis, bound, keep_greater, budget)?;
            if polygon.is_empty() {
                break;
            }
        }
        if polygon.len() < 3 {
            continue;
        }
        budget.charge(polygon.len())?;
        let local = |p: P| {
            point(
                (p.x - f64::from(left)).clamp(0., width as f64) as f32,
                (p.y - f64::from(top)).clamp(0., height as f64) as f32,
            )
        };
        let mut previous = local(*polygon.last().unwrap());
        for p in polygon {
            let p = local(p);
            raster.draw_line(previous, p);
            previous = p;
        }
    }
    let mut alpha = vec![0; width * height];
    raster.for_each_pixel(|i, c| alpha[i] = (c.min(1.) * 255. + 0.5) as u8);
    Ok(alpha)
}
fn clip(
    input: &[P],
    axis: usize,
    bound: f64,
    greater: bool,
    budget: &mut ClipBudget,
) -> std::result::Result<Vec<P>, PaintError> {
    budget.charge(input.len())?;
    let coordinate = |p: P| if axis == 0 { p.x } else { p.y };
    let inside = |p: P| {
        if greater {
            coordinate(p) >= bound
        } else {
            coordinate(p) <= bound
        }
    };
    let mut out = Vec::new();
    let Some(&last) = input.last() else {
        return Ok(out);
    };
    let mut previous = last;
    let push = |out: &mut Vec<P>, p: P| {
        if out.len() == MAX_FLAT {
            return Err("A clipped font polygon exceeds Reader's scratch bound");
        }
        out.push(p);
        Ok(())
    };
    for &current in input {
        let a = inside(previous);
        let b = inside(current);
        if a != b {
            let t = (bound - coordinate(previous)) / (coordinate(current) - coordinate(previous));
            push(&mut out, previous.lerp(current, t))?;
        }
        if b {
            push(&mut out, current)?;
        }
        previous = current;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn collinear_curve_overshoot_survives_flattening_and_stroke_expansion() {
        let path = [
            PathEl::MoveTo(P::new(0., 0.)),
            PathEl::QuadTo(P::new(200., 0.), P::new(100., 0.)),
            PathEl::LineTo(P::new(100., 100.)),
            PathEl::LineTo(P::new(0., 100.)),
            PathEl::ClosePath,
        ];
        let contours = flatten(&path).unwrap();
        let furthest = contours.iter().flatten().map(|p| p.x).fold(0., f64::max);
        assert!(
            (furthest - 400. / 3.).abs() <= TOLERANCE,
            "overshoot lost: {furthest}"
        );
        let border = bounded_stroke(&contours, 4.).unwrap();
        assert!(border.bounding_box().x1 > 132.);
        let strokes = flatten(border.elements()).unwrap();
        let mask = tile_mask(&strokes, 120, -5, 30, 20, &mut ClipBudget::default()).unwrap();
        assert!(
            mask.chunks_exact(30)
                .any(|row| row[10..15].iter().any(|&n| n > 128))
        );
    }

    #[test]
    fn scratch_and_total_work_are_rejected_before_growing_beyond_their_caps() {
        let zigzag = (0..MAX_FLAT)
            .map(|n| P::new(if n % 2 == 0 { -1. } else { 1. }, n as f64))
            .collect::<Vec<_>>();
        assert!(clip(&zigzag, 0, 0., true, &mut ClipBudget::default()).is_err());
        let mut budget = ClipBudget::default();
        budget.charge(MAX_CLIP_VISITS).unwrap();
        assert!(budget.charge(1).is_err());
        let excessive = vec![vec![P::new(0., 0.); MAX_STROKE_ELEMENTS / 8 + 1]];
        assert!(bounded_stroke(&excessive, 1.).is_err());
        assert!(tile_mask(&vec![], 0, 0, 257, 1, &mut ClipBudget::default()).is_err());
    }

    #[test]
    fn fractional_curved_concave_contours_keep_seams_and_winding_across_tiles() {
        let path = [
            PathEl::MoveTo(P::new(-10.25, 20.5)),
            PathEl::CurveTo(
                P::new(90.25, -30.5),
                P::new(260.5, 30.25),
                P::new(240.25, 160.5),
            ),
            PathEl::LineTo(P::new(120.5, 90.25)),
            PathEl::LineTo(P::new(200.25, 240.5)),
            PathEl::LineTo(P::new(-20.5, 230.25)),
            PathEl::ClosePath,
        ];
        let contours = flatten(&path).unwrap();
        let whole = tile_mask(&contours, 0, 0, 256, 256, &mut ClipBudget::default()).unwrap();
        let mut joined = vec![0; 256 * 256];
        let mut budget = ClipBudget::default();
        for y in (0..256).step_by(64) {
            for x in (0..256).step_by(64) {
                let tile = tile_mask(&contours, x, y, 64, 64, &mut budget).unwrap();
                for row in 0..64 {
                    joined[(y as usize + row) * 256 + x as usize
                        ..(y as usize + row) * 256 + x as usize + 64]
                        .copy_from_slice(&tile[row * 64..row * 64 + 64]);
                }
            }
        }
        let errors = whole
            .iter()
            .zip(&joined)
            .map(|(&a, &b)| a.abs_diff(b) as usize)
            .collect::<Vec<_>>();
        assert!(errors.iter().all(|&n| n <= 2));
        assert!(errors.iter().sum::<usize>() < 256);
    }

    #[test]
    fn actual_outline_tiles_preserve_clipped_bearings_phase_rotation_skew_and_signed_weight() {
        let bytes = std::fs::read(crate::test_fixture("fonts/NotoSans-Regular.ttf")).unwrap();
        let font = FontData::new(parley::fontique::Blob::new(Arc::new(bytes)), 0);
        let face = FontRef::from_index(font.data.data(), 0).unwrap();
        let id = face.charmap().map('g').unwrap().to_u32();
        let paint_ink = Ink {
            gray: 63,
            contrast: false,
            gamma: crate::font_render::gamma_table(crate::font_render::NORMAL_GAMMA),
        };
        for clockwise in [false, true] {
            for strength in [-2.25, 2.25] {
                for phase in [0, 1, 3] {
                    let style = GlyphStyle {
                        font: &font,
                        size: 180.,
                        variations: vec![],
                        skew: 12.,
                        clockwise,
                        strength,
                    };
                    let reference = super::super::rasterize(&style, id, phase, &[], None).unwrap();
                    let mut expected = vec![255; 500 * 400];
                    let mut actual = expected.clone();
                    reference.blend(&mut expected, 500, &(30..470), -15, 65, paint_ink);
                    paint(
                        &mut actual,
                        500,
                        &(30..470),
                        -15,
                        65,
                        &style,
                        id,
                        phase,
                        &[],
                        None,
                        paint_ink,
                    )
                    .unwrap();
                    let ink = expected
                        .iter()
                        .map(|&n| usize::from(255 - n))
                        .sum::<usize>();
                    assert!(ink > 10000);
                    let error = actual
                        .iter()
                        .zip(&expected)
                        .map(|(&a, &b)| usize::from(a.abs_diff(b)))
                        .sum::<usize>();
                    assert!(
                        error < ink / 20 + 1000,
                        "clockwise={clockwise} strength={strength} phase={phase} error={error} ink={ink}"
                    );
                    for frame in [&expected, &actual] {
                        for row in frame.chunks_exact(500) {
                            assert!(row[..30].iter().chain(&row[470..]).all(|&n| n == 255));
                        }
                    }
                    let bounds = |frame: &[u8]| {
                        let mut xs = Vec::new();
                        let mut ys = Vec::new();
                        for (i, &pixel) in frame.iter().enumerate().filter(|(_, p)| **p < 128) {
                            let _ = pixel;
                            xs.push((i % 500) as i32);
                            ys.push((i / 500) as i32);
                        }
                        (
                            *xs.iter().min().unwrap(),
                            *xs.iter().max().unwrap(),
                            *ys.iter().min().unwrap(),
                            *ys.iter().max().unwrap(),
                        )
                    };
                    let a = bounds(&actual);
                    let b = bounds(&expected);
                    assert!(
                        (a.0 - b.0).abs() <= 1
                            && (a.1 - b.1).abs() <= 1
                            && (a.2 - b.2).abs() <= 1
                            && (a.3 - b.3).abs() <= 1
                    );
                }
            }
        }
    }

    #[test]
    fn clipping_preserves_winding_holes_and_covers_tile_edges_without_negative_x_bleed() {
        let shape = vec![
            vec![
                P::new(-30., -30.),
                P::new(300., -30.),
                P::new(300., 300.),
                P::new(-30., 300.),
            ],
            vec![
                P::new(50., 50.),
                P::new(50., 150.),
                P::new(150., 150.),
                P::new(150., 50.),
            ],
        ];
        let mask = tile_mask(&shape, 0, 0, 256, 256, &mut ClipBudget::default()).unwrap();
        assert_eq!(mask.len(), 65536);
        for (x, y) in [(0, 0), (255, 0), (255, 255), (0, 255), (49, 80)] {
            assert_eq!(mask[y * 256 + x], 255);
        }
        assert_eq!(mask[80 * 256 + 80], 0);
        let mut stitched = vec![0; 256 * 256];
        for y in (0..256).step_by(64) {
            for x in (0..256).step_by(64) {
                let tile = tile_mask(&shape, x, y, 64, 64, &mut ClipBudget::default()).unwrap();
                for row in 0..64 {
                    stitched[(y as usize + row) * 256 + x as usize
                        ..(y as usize + row) * 256 + x as usize + 64]
                        .copy_from_slice(&tile[row * 64..row * 64 + 64]);
                }
            }
        }
        assert_eq!(mask, stitched);
    }
}
