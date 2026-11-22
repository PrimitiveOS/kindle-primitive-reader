//! Large-outline fallback. Clip closed contours before rasterization: the small
//! rasterizer does not provide arbitrary negative-X clipping for a tile.
use super::*;
use kurbo::{BezPath, Join, PathEl, Point as P, Shape, Stroke, StrokeOpts};

const TILE: usize = 256;
const MAX_FLAT: usize = 131_072;
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
    contrast: bool,
    gamma: &[u8; 256],
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
    let border = if style.strength == 0. {
        BezPath::new()
    } else {
        kurbo::stroke(
            path.elements().iter().copied(),
            &Stroke::new(f64::from(style.strength.abs())).with_join(Join::Miter),
            &StrokeOpts::default(),
            0.05,
        )
    };
    if border.elements().len() > MAX_SEGMENTS * 8 {
        return Err("A font stroke exceeds Reader's segment bound");
    }
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
    let contours = flatten(path.elements())?;
    let strokes = flatten(border.elements())?;
    for tile_y in (top..bottom).step_by(TILE) {
        for tile_x in (left..right).step_by(TILE) {
            let width = (right - tile_x).min(TILE as i32) as usize;
            let height = (bottom - tile_y).min(TILE as i32) as usize;
            let mut alpha = tile_mask(&contours, tile_x, tile_y, width, height);
            if !strokes.is_empty() {
                let boundary = tile_mask(&strokes, tile_x, tile_y, width, height);
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
            .blend(page, stride, columns, x, baseline, contrast, gamma);
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
        (v.x * (p.y - a.y) - v.y * (p.x - a.x)).abs() / length <= TOLERANCE
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

fn tile_mask(contours: &Contours, left: i32, top: i32, width: usize, height: usize) -> Vec<u8> {
    let mut raster = Rasterizer::new(width, height);
    for contour in contours {
        let mut polygon = contour.clone();
        for (axis, bound, keep_greater) in [
            (0, f64::from(left), true),
            (0, f64::from(left) + width as f64, false),
            (1, f64::from(top), true),
            (1, f64::from(top) + height as f64, false),
        ] {
            polygon = clip(&polygon, axis, bound, keep_greater);
            if polygon.is_empty() {
                break;
            }
        }
        if polygon.len() < 3 {
            continue;
        }
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
    alpha
}
fn clip(input: &[P], axis: usize, bound: f64, greater: bool) -> Vec<P> {
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
        return out;
    };
    let mut previous = last;
    for &current in input {
        let a = inside(previous);
        let b = inside(current);
        if a != b {
            let t = (bound - coordinate(previous)) / (coordinate(current) - coordinate(previous));
            out.push(previous.lerp(current, t));
        }
        if b {
            out.push(current);
        }
        previous = current;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
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
        let mask = tile_mask(&shape, 0, 0, 256, 256);
        assert_eq!(mask.len(), 65536);
        for (x, y) in [(0, 0), (255, 0), (255, 255), (0, 255), (49, 80)] {
            assert_eq!(mask[y * 256 + x], 255);
        }
        assert_eq!(mask[80 * 256 + 80], 0);
        let mut stitched = vec![0; 256 * 256];
        for y in (0..256).step_by(64) {
            for x in (0..256).step_by(64) {
                let tile = tile_mask(&shape, x, y, 64, 64);
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
