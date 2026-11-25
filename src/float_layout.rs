//! Source-owned float footprints for the opt-in horizontal Parley provider.
pub(crate) mod tables;

use crate::{
    FloatMode, Settings,
    book_style::{ImageLength, Style, WritingMode, flow::Display},
    source::Source,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Side {
    Left = 0,
    Right = 1,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Clear(u8);

impl Clear {
    pub(crate) const BOTH: Self = Self(3);

    pub(crate) fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "none" | "initial" | "unset" => Self::default(),
            "left" => Self(1),
            "right" => Self(2),
            "both" => Self::BOTH,
            _ => return None,
        })
    }

    fn applies(self, side: Side) -> bool {
        self.0 & (1 << side as usize) != 0
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Spec {
    pub owner: usize,
    pub side: Side,
    pub width: u32,
    pub publisher_height: Option<f32>,
    pub gray: Option<u8>,
    scope_end: usize,
}

pub(crate) struct Plan {
    pub floats: Vec<Option<Spec>>,
    pub clears: Vec<Clear>,
    pub tables: tables::Candidates,
}

impl Plan {
    pub(crate) fn prepare(source: &Source, styles: &[Style], settings: Settings) -> Option<Self> {
        if settings.float_mode == FloatMode::Ignore
            || !styles.iter().any(|style| style.float_side.is_some())
        {
            return None;
        }
        let mut spans = vec![None; source.elements.len()];
        for (index, block) in source.blocks.iter().enumerate() {
            if !styles[block.element].flow.suppressed {
                let (first, last) = spans[block.element].get_or_insert((index, index));
                *last = index;
                debug_assert!(*first <= *last);
            }
        }
        for (index, node) in source.elements.iter().enumerate().rev() {
            if let (Some((first, last)), Some(parent)) = (spans[index], node.parent) {
                let span = spans[parent].get_or_insert((first, last));
                span.0 = span.0.min(first);
                span.1 = span.1.max(last);
            }
        }

        let width = settings.text_width();
        let float_limit = (settings.text_height() / 2) as f32;
        let tables = tables::Candidates::new(source, styles, &spans);
        let mut floats = vec![None; source.blocks.len()];
        let mut clears = vec![Clear::default(); source.blocks.len()];
        for (owner, ((node, style), span)) in
            source.elements.iter().zip(styles).zip(&spans).enumerate()
        {
            let Some((first, last)) = *span else { continue };
            if node.selector_only
                || node.anonymous
                || !matches!(style.display, Display::Block | Display::Table)
            {
                continue;
            }
            clears[first].0 |= style.clear.0;
            if style.display != Display::Block {
                continue;
            }
            let Some(side) = style.float_side else {
                continue;
            };
            let block = &source.blocks[first];
            let Some(parent) = node.parent else { continue };
            if first != last
                || !matches!(
                    node.tag.as_str(),
                    "div" | "p" | "aside" | "section" | "blockquote"
                )
                || block.table_cell.is_some()
                || block.list_item.is_some()
                || block.text.is_empty()
                || block.text.len() > 512
                || !block.images.is_empty()
                || !block.ruby.is_empty()
                || (block.element != owner && source.elements[block.element].parent != Some(owner))
                || block.element != owner && styles[block.element].float_side.is_some()
                || !simple_box(style, width, settings.font_size)
                || !simple_box(&styles[block.element], width, settings.font_size)
            {
                continue;
            }
            let font = settings.styled_font(style.size_scale);
            let Some(float_width) = style
                .image_width
                .and_then(|length| length.pixels(width, font))
            else {
                continue;
            };
            if !(font * 2.0..=width as f32 * 0.75).contains(&float_width) {
                continue;
            }
            let publisher_height = match (settings.float_mode, style.image_height) {
                (FloatMode::PublisherHeight, Some(ImageLength::Fraction(_))) => continue,
                (FloatMode::PublisherHeight, Some(height)) => height.pixels(0, font),
                _ => None,
            };
            if publisher_height.is_some_and(|height| !(0.0..=float_limit).contains(&height)) {
                continue;
            }
            floats[first] = Some(Spec {
                owner,
                side,
                width: float_width.floor() as u32,
                publisher_height: publisher_height.map(f32::floor),
                gray: style.flow.painted.then_some(style.background).flatten(),
                scope_end: spans[parent].map_or(last, |span| span.1),
            });
        }
        (floats.iter().any(Option::is_some) || !tables.is_empty()).then_some(Self {
            floats,
            clears,
            tables,
        })
    }
}

pub(crate) fn simple_box(style: &Style, width: u32, font: f32) -> bool {
    style.writing_mode == WritingMode::Horizontal
        && style.borders.is_none()
        && style
            .padding
            .iter()
            .chain(&style.vertical_padding)
            .chain(&style.vertical_margin)
            .all(|edge| edge.box_pixels(width, font) == 0.0)
}

// CREngine keeps up to five exact rectangles per text block. Keep the
// placement list separate: even when text uses merged contours, placement
// and clear still need each rectangle's actual coordinates.
const EXACT_CONTOURS: usize = 5;
const MAX_ACTIVE_FLOATS: usize = 64;

#[derive(Debug)]
pub(crate) struct TooManyFloats;

impl std::fmt::Display for TooManyFloats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("The chapter has too many simultaneously placed floats")
    }
}

impl std::error::Error for TooManyFloats {}

#[derive(Clone, Copy)]
struct Footprint {
    side: Side,
    edge: u32,
    top: f32,
    bottom: f32,
    scope_end: usize,
}

#[derive(Default)]
pub(crate) struct Flow {
    boxes: Vec<Footprint>,
}

impl Flow {
    pub(crate) fn leave_closed_boxes(&mut self, index: usize, mut y: f32) -> f32 {
        self.boxes.retain(|rect| {
            if index > rect.scope_end {
                y = y.max(rect.bottom);
                false
            } else {
                true
            }
        });
        self.expire(y);
        y
    }

    fn expire(&mut self, y: f32) {
        self.boxes.retain(|rect| rect.bottom > y);
    }

    pub(crate) fn clear(&mut self, clear: Clear, mut y: f32) -> f32 {
        for rect in &self.boxes {
            if clear.applies(rect.side) {
                y = y.max(rect.bottom);
            }
        }
        self.expire(y);
        y
    }

    pub(crate) fn place(&self, spec: Spec, clear: Clear, width: u32, mut y: f32) -> (u32, f32) {
        // Later floats cannot start above earlier ones. A float's own
        // clearance moves that float and leaves the main text cursor alone.
        for rect in &self.boxes {
            y = y.max(rect.top);
            if clear.applies(rect.side) {
                y = y.max(rect.bottom);
            }
        }
        loop {
            let (left, right, next) = bounds(self.boxes.iter(), y, 0.0, width);
            if right.saturating_sub(left) >= spec.width || next.is_none() {
                let x = match spec.side {
                    Side::Left => left,
                    Side::Right => right.saturating_sub(spec.width).max(left),
                };
                return (x, y);
            }
            // At this point every earlier float has already started.
            // The available interval can only grow as rectangles end.
            y = next.unwrap();
        }
    }

    pub(crate) fn record(
        &mut self,
        spec: Spec,
        left: u32,
        top: f32,
        bottom: f32,
    ) -> Result<(), TooManyFloats> {
        if self.boxes.len() == MAX_ACTIVE_FLOATS {
            return Err(TooManyFloats);
        }
        self.boxes.push(Footprint {
            side: spec.side,
            edge: if spec.side == Side::Left {
                left + spec.width
            } else {
                left
            },
            top,
            bottom,
            scope_end: spec.scope_end,
        });
        Ok(())
    }

    pub(crate) fn finish(&self, y: f32) -> f32 {
        self.boxes.iter().fold(y, |end, rect| end.max(rect.bottom))
    }

    pub(crate) fn contour(&self, origin: f32, width: u32, min_width: f32) -> Option<Contour> {
        let mut exact = [None; EXACT_CONTOURS];
        let mut count = 0;
        let mut merged: [Option<Footprint>; 2] = [None; 2];
        for &rect in self.boxes.iter().filter(|rect| rect.bottom > origin) {
            if let Some(slot) = exact.get_mut(count) {
                *slot = Some(rect);
            }
            count += 1;
            let edge = merged[rect.side as usize].get_or_insert(Footprint {
                top: origin,
                ..rect
            });
            edge.edge = match rect.side {
                Side::Left => edge.edge.max(rect.edge),
                Side::Right => edge.edge.min(rect.edge),
            };
            edge.bottom = edge.bottom.max(rect.bottom);
        }
        if count == 0 {
            return None;
        }
        if count > EXACT_CONTOURS {
            exact = [None; EXACT_CONTOURS];
            exact[..merged.len()].copy_from_slice(&merged);
        }
        Some(Contour {
            boxes: exact,
            origin,
            width,
            min_width,
        })
    }
}

fn bounds<'a>(
    footprints: impl Iterator<Item = &'a Footprint>,
    y: f32,
    strut: f32,
    width: u32,
) -> (u32, u32, Option<f32>) {
    let mut left = 0;
    let mut right = width;
    let mut next = None;
    for rect in footprints {
        if rect.bottom > y && rect.top <= y + strut {
            match rect.side {
                Side::Left => left = left.max(rect.edge),
                Side::Right => right = right.min(rect.edge),
            }
            next = Some(next.map_or(rect.bottom, |previous: f32| previous.min(rect.bottom)));
        }
    }
    (left, right, next)
}

#[derive(Clone, Copy)]
pub(crate) struct Contour {
    boxes: [Option<Footprint>; EXACT_CONTOURS],
    origin: f32,
    width: u32,
    min_width: f32,
}

impl Contour {
    pub(crate) fn left_initial(width: u32, edge: u32, height: f32) -> Self {
        let mut boxes = [None; EXACT_CONTOURS];
        boxes[0] = Some(Footprint {
            side: Side::Left,
            edge,
            top: 0.0,
            bottom: height,
            scope_end: usize::MAX,
        });
        Self {
            boxes,
            origin: 0.0,
            width,
            min_width: 0.0,
        }
    }

    pub(crate) fn for_line(self, mut local_y: f64, strut: f32) -> (f64, f32, f32) {
        loop {
            let (start, end, next) = bounds(
                self.boxes.iter().flatten(),
                self.origin + local_y as f32,
                strut,
                self.width,
            );
            let span = end.saturating_sub(start) as f32;
            if span >= self.min_width || next.is_none() {
                return (local_y, start as f32, span);
            }
            local_y = f64::from(next.unwrap() - self.origin).max(local_y + 0.001);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Anchor, Engine, Probe, Renderer, selection::HighlightRect};

    pub(super) fn settings(mode: FloatMode) -> Settings {
        Settings {
            height: 1606,
            font_size: 40.0,
            line_height: 1.2,
            float_mode: mode,
            ..Settings::default()
        }
    }

    pub(super) fn open(name: &str, mode: FloatMode, chapter: usize) -> Probe {
        Probe::open(
            &crate::test_fixture(name),
            Engine::Parley,
            &[crate::test_fixture("fonts/NotoSans-Regular.ttf")],
            settings(mode),
            chapter,
        )
        .unwrap()
    }

    pub(super) fn marker(probe: &Probe, text: &str) -> (Anchor, HighlightRect) {
        let start = probe.matches(text)[0].clone();
        let end = Anchor {
            offset: start.offset + text.len(),
            ..start.clone()
        };
        let page = probe.page_for(&start).unwrap();
        let rect = probe.selection_rects(page, &probe.select_range(&start, &end).unwrap())[0];
        (start, rect)
    }

    fn capture(probe: &mut Probe, mode: FloatMode, chapter: usize) -> Vec<u8> {
        let frame = probe.capture(0).unwrap();
        if let Some(dir) = std::env::var_os("READER_FLOAT_CAPTURE_DIR") {
            let path = std::path::PathBuf::from(dir).join(format!("{mode:?}-{chapter}.png"));
            let settings = settings(mode);
            image::GrayImage::from_raw(settings.width, settings.height, frame.clone())
                .unwrap()
                .save(path)
                .unwrap();
        }
        frame
    }

    #[test]
    fn physical_left_and_right_match_stock_offsets_and_keep_touch_and_link_anchors() {
        let book = "render-floats.epub";
        for (chapter, text, side, card_x, neighbor_x, color, band_x) in [
            (
                0,
                [
                    "Left card",
                    "Left neighbor",
                    "Left clearance",
                    "Left ordinary",
                ],
                Side::Left,
                54,
                470,
                204,
                454,
            ),
            (
                1,
                [
                    "Right card",
                    "Right neighbor",
                    "Right clearance",
                    "Right ordinary",
                ],
                Side::Right,
                794,
                54,
                221,
                1200,
            ),
        ] {
            let mut original = open(book, FloatMode::Ignore, chapter);
            let original_frame = original.capture(0).unwrap();
            let original_source = marker(&original, text[1]).0;
            let control_pitch = marker(&original, text[3]).1.y - marker(&original, text[2]).1.y;
            for mode in [FloatMode::ContentHeight, FloatMode::PublisherHeight] {
                let mut probe = open(book, mode, chapter);
                let [card, neighbor, clear, ordinary] = text.map(|part| marker(&probe, part));
                assert_eq!(card.1.x, card_x);
                assert_eq!(neighbor.1.x, neighbor_x);
                assert_eq!(card.1.y, neighbor.1.y);
                assert_eq!(clear.1.x, 54);
                let clearance = clear.1.y - neighbor.1.y;
                if mode == FloatMode::PublisherHeight {
                    assert_eq!(clearance, 80);
                } else {
                    assert!((clearance - control_pitch).abs() <= 1);
                }
                assert!((ordinary.1.y - clear.1.y - control_pitch).abs() <= 1);
                assert_eq!(neighbor.0, original_source);
                let view_x = neighbor.1.x as f32 + 4.0;
                let view_y = neighbor.1.y as f32 + neighbor.1.height as f32 * 0.5;
                let tapped = probe.text_position_at(0, view_x, view_y).unwrap();
                assert_eq!(tapped.block, original_source.block);
                let target = if side == Side::Left {
                    "#left-end"
                } else {
                    "#right-end"
                };
                assert_eq!(
                    probe.link_href_in(probe.page_range(0), view_x, view_y),
                    Some(target)
                );
                let card_x = card.1.x as f32 + 4.0;
                let tap_card = probe.text_position_at(0, card_x, view_y).unwrap();
                assert_eq!(tap_card.block, card.0.block);
                assert_ne!(tap_card.block, tapped.block);

                let frame = capture(&mut probe, mode, chapter);
                let band = frame
                    .chunks_exact(1264)
                    .skip(54)
                    .take_while(|row| row[band_x] == color)
                    .count();
                assert!(band > 0);
                println!(
                    "{mode:?} {side:?} marks [(x,y)]={:?}, float band={} px",
                    [card.1, neighbor.1, clear.1, ordinary.1].map(|rect| (rect.x, rect.y)),
                    band
                );
                if mode == FloatMode::PublisherHeight {
                    assert_eq!(band, 80);
                }
                assert_ne!(frame, original_frame);
            }
            let mut rolled_back = open(book, FloatMode::Ignore, chapter);
            assert_eq!(rolled_back.capture(0).unwrap(), original_frame);
            assert_eq!(marker(&rolled_back, text[1]).0, original_source);
        }
    }

    #[test]
    fn simultaneous_exclusions_clear_one_side_and_release_line_width_after_the_footprint() {
        let book = "render-floats.epub";
        let mut opposed = open(book, FloatMode::PublisherHeight, 2);
        let [left, right, neighbor, clear_left, clear_both] = [
            "Opposed left",
            "Opposed right",
            "Opposing neighbor",
            "Left cleared only",
            "Both sides cleared",
        ]
        .map(|phrase| marker(&opposed, phrase).1);
        assert_eq!(
            (left.x, right.x, neighbor.x, clear_left.x, clear_both.x),
            (54, 794, 470, 54, 54)
        );
        assert_eq!((left.y, right.y, neighbor.y), (54, 54, 54));
        assert_eq!(clear_left.y, 214);
        assert!(clear_both.y >= 254);
        println!(
            "PublisherHeight opposed clear y={} / {}",
            clear_left.y, clear_both.y
        );
        let Renderer::Parley(renderer) = &opposed.renderer else {
            panic!("Parley required")
        };
        let page = opposed.page_range(0);
        let key = marker(&opposed, "Left cleared only").0.block;
        let layout = &renderer
            .paragraphs_in(page)
            .find(|p| p.key == key)
            .unwrap()
            .layout;
        assert_eq!(layout.get(0).unwrap().metrics().inline_max_coord, 740.0);
        capture(&mut opposed, FloatMode::PublisherHeight, 2);

        for (mode, count) in [
            (FloatMode::ContentHeight, 1),
            (FloatMode::PublisherHeight, 2),
        ] {
            let mut probe = open(book, mode, 3);
            let (anchor, begin) = marker(&probe, "Wrap begins here");
            assert_eq!(begin.x, 470);
            let Renderer::Parley(renderer) = &probe.renderer else {
                panic!("Parley required")
            };
            let layout = &renderer
                .paragraphs_in(probe.page_range(0))
                .find(|p| p.key == anchor.block)
                .unwrap()
                .layout;
            let widths: Vec<_> = layout
                .lines()
                .map(|line| {
                    (
                        line.metrics().inline_min_coord,
                        line.metrics().inline_max_coord,
                    )
                })
                .collect();
            assert!(widths.len() > count);
            assert_eq!(widths[..count], vec![(416.0, 1156.0); count]);
            assert!(widths[count..].iter().all(|width| *width == (0.0, 1156.0)));
            println!("{mode:?} wrap line contour={widths:?}");
            capture(&mut probe, mode, 3);
        }
    }

    #[test]
    fn authored_render_modes_fixture_and_complex_fallback_keep_source_content() {
        let ordinary = open("render-modes.epub", FloatMode::Ignore, 0);
        let candidate = open("render-modes.epub", FloatMode::PublisherHeight, 0);
        let (anchor, before) = marker(&ordinary, "Neighbor starts here");
        let (reopened, after) = marker(&candidate, "Neighbor starts here");
        assert_eq!(anchor, reopened);
        assert_eq!((before.x, after.x), (54, 470));
        assert!(after.y < before.y);

        let book = "render-floats.epub";
        let mut ordinary = open(book, FloatMode::Ignore, 4);
        let mut candidate = open(book, FloatMode::PublisherHeight, 4);
        assert_eq!(candidate.capture(0).unwrap(), ordinary.capture(0).unwrap());
        for text in [
            "Malformed width card",
            "Nested card first",
            "Nested card second",
            "Fallback ordinary",
        ] {
            assert_eq!(marker(&candidate, text), marker(&ordinary, text));
        }
    }

    #[test]
    fn an_overflowing_float_retries_ordinary_layout_without_changing_saved_positions() {
        let book = "render-floats.epub";
        let mut ordinary = open(book, FloatMode::Ignore, 5);
        let mut overflow = open(book, FloatMode::PublisherHeight, 5);
        let Renderer::Parley(renderer) = &overflow.renderer else {
            panic!("Parley required")
        };
        assert!(!renderer.uses_float_geometry());
        assert!(
            renderer
                .limitations()
                .iter()
                .any(|msg| msg.contains("Float geometry"))
        );
        assert_eq!(overflow.capture(0).unwrap(), ordinary.capture(0).unwrap());
        let after = "Readable after geometry rollback";
        assert_eq!(marker(&ordinary, after), marker(&overflow, after));
    }
}

#[cfg(test)]
mod stacking_tests;
