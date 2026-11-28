//! Source-topology splits for adjacent empty native horizontal boxes. Empty
//! pages have no synthetic text, fragments, or source offsets.
use super::{BoxRules, Edge, Rules};
use crate::{
    EmptyBreakMode, Settings,
    book_style::{ImageLength, Style, WritingMode, borders::Pattern, flow::Display},
    source::Source,
};
use std::ops::Range;

pub(crate) mod physical;

const MAX_EXTRA_PAGES: u16 = 512;

#[derive(Default)]
struct Leaf {
    before: bool,
    after: bool,
    inner_extent: bool,
    margin: bool,
}

#[derive(Default)]
struct Chain {
    splits: u16,
    after: Option<(Option<usize>, bool)>,
}

impl Chain {
    fn append(
        &mut self,
        &Leaf {
            before,
            after,
            inner_extent,
            margin,
        }: &Leaf,
        parent: Option<usize>,
        structure: bool,
    ) -> (bool, bool) {
        let adjacent = before
            && self
                .after
                .is_some_and(|(owner, sized)| owner == parent && (structure || sized));
        let opening = before && !adjacent;
        let closing = after && (!before || structure || inner_extent);
        self.splits = self
            .splits
            .saturating_add(u16::from(opening) + u16::from(closing));
        // An unforced zero-size sibling can share the same edge. A different
        // parent or an intervening box with physical space ends that edge.
        if after {
            self.after = Some((parent, inner_extent));
        } else if before
            || inner_extent
            || margin
            || self.after.is_some_and(|(owner, _)| owner != parent)
        {
            self.after = None;
        }
        (opening, closing)
    }
}

pub(super) fn propagate(
    source: &Source,
    styles: &[Style],
    direct: &[Option<Range<f32>>],
    rendered: &[Option<Range<f32>>],
    settings: Settings,
    result: &mut Vec<BoxRules>,
) {
    let Some(first_material) = direct.iter().position(Option::is_some) else {
        return;
    };
    let structure = settings.empty_break_mode == EmptyBreakMode::PreserveStructure;
    let mut excluded = Vec::<bool>::with_capacity(styles.len());
    let eligible: Vec<_> = source
        .elements
        .iter()
        .zip(styles)
        .zip(rendered)
        .map(|((node, style), rendered)| {
            let blocked = node.parent.is_some_and(|p| excluded[p])
                || style.flow.suppressed
                || style.float_side.is_some()
                || style.writing_mode != WritingMode::Horizontal
                || matches!(
                    node.tag.as_str(),
                    "table" | "ul" | "ol" | "dl" | "li" | "ruby"
                );
            excluded.push(blocked);
            !blocked
                && rendered.is_none()
                && !node.selector_only
                && !node.anonymous
                && Display::native(&node.tag) == Display::Block
                && style.display == Display::Block
                && !matches!(node.tag.as_str(), "html" | "body" | "hr")
        })
        .collect();

    // Forced edges on empty wrappers attach to the first/last empty leaf.
    // Walking source order gives deterministic ownership with no recursion.
    let mut leaves = Vec::new();
    let mut spans: Vec<Option<(usize, usize)>> = vec![None; styles.len()];
    let mut following: Vec<Option<usize>> = vec![None; styles.len()];
    let mut next = None;
    for (index, node) in source.elements.iter().enumerate().rev() {
        following[index] = next;
        if direct[index].is_some() {
            next = Some(index);
        }
        if !eligible[index] || index <= first_material || next.is_none() {
            continue;
        }
        let (first, last) = *spans[index].get_or_insert_with(|| {
            let leaf = leaves.len();
            leaves.push((index, Leaf::default()));
            (leaf, leaf)
        });
        if let Some(parent) = node.parent.filter(|&p| eligible[p]) {
            let parent = spans[parent].get_or_insert((first, last));
            parent.0 = parent.0.max(first);
            parent.1 = parent.1.min(last);
        }
    }

    for (&span, style) in spans.iter().zip(styles) {
        let Some((first, last)) = span else { continue };
        let (inner_extent, margin) = extent(style, settings);
        leaves[first].1.before |= style.page_breaks.before == Edge::Force;
        leaves[last].1.after |= style.page_breaks.after == Edge::Force;
        for index in [first, last] {
            leaves[index].1.inner_extent |= inner_extent;
            leaves[index].1.margin |= margin;
        }
    }

    let mut budget = MAX_EXTRA_PAGES;
    let mut leaves = leaves.iter().rev().peekable();
    while let Some((owner, leaf)) = leaves.next() {
        let target = following[*owner].expect("empty leaf is inside the material range");
        let mut chain = Chain::default();
        chain.append(leaf, source.elements[*owner].parent, structure);
        while let Some((other, _)) = leaves.peek()
            && following[*other] == Some(target)
        {
            let (other, leaf) = leaves.next().unwrap();
            chain.append(leaf, source.elements[*other].parent, structure);
        }
        let count = chain.splits.saturating_sub(1).min(budget);
        if count > 0 {
            let y = direct[target].as_ref().unwrap().start;
            result.push(BoxRules {
                extent: y..y,
                rules: Rules::default(),
                prefer_opening: false,
                extra_splits: count,
                physical_cuts: None,
            });
            budget -= count;
        }
        if budget == 0 {
            break;
        }
    }
}

fn extent(style: &Style, settings: Settings) -> (bool, bool) {
    let width = settings.text_width();
    let font = settings.font_size;
    let has_height = style.image_height.is_some_and(|height| {
        !matches!(height, ImageLength::Auto | ImageLength::Fraction(_))
            && height
                .pixels(0, settings.styled_font(style.size_scale))
                .is_some_and(|px| px.floor() > 0.0)
    });
    let has_padding = style
        .vertical_padding
        .iter()
        .any(|edge| edge.box_pixels(width, font) > 0.0);
    let has_border = style.borders.as_ref().is_some_and(|borders| {
        [&borders[0], &borders[2]].iter().any(|edge| {
            !matches!(edge.pattern, Pattern::None | Pattern::Hidden)
                && edge.width.box_pixels(width, font) > 0.0
        })
    });
    let has_margin = style
        .vertical_margin
        .iter()
        .any(|edge| edge.box_pixels(width, font) > 0.0);
    (has_height || has_padding || has_border, has_margin)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Anchor, Engine, Probe};

    fn settings(mode: EmptyBreakMode) -> Settings {
        Settings {
            height: 1606,
            font_size: 40.0,
            line_height: 1.2,
            empty_break_mode: mode,
            ..Settings::default()
        }
    }

    fn open(book: &str, mode: EmptyBreakMode, chapter: usize) -> Probe {
        open_with_geometry(book, mode, chapter, false)
    }

    fn open_with_geometry(
        book: &str,
        mode: EmptyBreakMode,
        chapter: usize,
        geometry: bool,
    ) -> Probe {
        Probe::open(
            &crate::test_fixture(book),
            Engine::Parley,
            &[crate::test_fixture("fonts/NotoSans-Regular.ttf")],
            Settings {
                empty_break_geometry: geometry,
                ..settings(mode)
            },
            chapter,
        )
        .unwrap()
    }

    #[test]
    fn stock_topologies_preserve_page_assignments_and_canonical_text() {
        let book = "render-empty-breaks.epub";
        let cases = [
            "Bare",
            "Padding",
            "Margin",
            "Border",
            "Height",
            "Background",
            "Decorated",
            "Removed",
            "Nested",
            "OnlyBefore",
            "OnlyAfter",
            "Mixed",
            "Between",
            "One",
            "Text",
            "BareOne",
            "BareThree",
            "BareOnlyBefore",
            "BareOnlyAfter",
            "BareMixed",
            "BareBetween",
            "NestedBare",
            "BareThenPadding",
            "PaddingThenBare",
            "PaddingAcrossBare",
        ];
        let compact = [
            1, 1, 1, 1, 1, 1, 1, 0, 1, 1, 1, 1, 1, 1, 2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
        ];
        let legacy = [
            3, 3, 3, 3, 3, 3, 3, 0, 4, 2, 2, 2, 2, 2, 2, 2, 4, 2, 2, 2, 1, 4, 3, 3, 1,
        ];
        let web = [
            2, 3, 2, 3, 3, 2, 3, 0, 4, 2, 2, 2, 2, 2, 2, 1, 3, 2, 2, 2, 2, 2, 3, 2, 1,
        ];
        for (chapter, name) in cases.into_iter().enumerate() {
            let first = format!("Test {name} first marker");
            let last = format!("Test {name} last marker");
            let mut baseline = open(book, EmptyBreakMode::Current, chapter);
            let first_anchor = baseline.matches(&first)[0].clone();
            let start = baseline.matches(&last)[0].clone();
            let end = Anchor {
                raw_caret: None,
                offset: start.offset + last.len(),
                ..start.clone()
            };
            let saved = serde_json::to_vec(&baseline.select_range(&start, &end).unwrap()).unwrap();
            let selection: crate::selection::TextSelection =
                serde_json::from_slice(&saved).unwrap();
            let baseline_last = baseline.capture(compact[chapter]).unwrap();
            for (mode, pages) in [
                (EmptyBreakMode::Coalesce, compact),
                (EmptyBreakMode::PreserveStructure, legacy),
                (EmptyBreakMode::PreserveSized, web),
            ] {
                let expected = pages[chapter];
                let mut probe = open(book, mode, chapter);
                assert_eq!(probe.page_for(&first_anchor), Some(0), "{mode:?} {name}");
                assert_eq!(probe.page_for(&start), Some(expected), "{mode:?} {name}");
                assert_eq!(probe.page_count(), expected + 1, "{mode:?} {name}");
                let physical = open_with_geometry(book, mode, chapter, true);
                assert_eq!(
                    physical.page_for(&start),
                    Some(expected),
                    "physical {mode:?} {name}"
                );
                assert_eq!(
                    physical.page_count(),
                    expected + 1,
                    "physical {mode:?} {name}"
                );
                assert_eq!(probe.anchor_top(&start), baseline.anchor_top(&start));
                assert_eq!(
                    probe.select_range(&selection.start, &selection.end),
                    Some(selection.clone())
                );
                assert_eq!(probe.capture(expected).unwrap(), baseline_last);
                if name == "Text" {
                    let intervening = &probe.matches("Text centered marker")[0];
                    assert_eq!(probe.page_for(intervening), Some(1));
                } else {
                    for page in 1..expected {
                        assert_eq!(probe.page_anchor(page), None);
                        assert!(
                            probe
                                .capture(page)
                                .unwrap()
                                .into_iter()
                                .all(|pixel| pixel == 255)
                        );
                    }
                }
                println!("{mode:?}|{name}|advance={expected}");
            }
        }
    }

    #[test]
    fn physical_box_bands_and_text_insets_match_the_pinned_renderer() {
        use EmptyBreakMode::{Coalesce, PreserveSized, PreserveStructure};
        let book = "render-empty-breaks.epub";
        let fixtures = [
            (
                6,
                "Decorated",
                Coalesce,
                1,
                158,
                104,
                1,
                "74-76:136,116-118:136",
            ),
            (
                6,
                "Decorated",
                PreserveStructure,
                3,
                54,
                124,
                1,
                "74-76:136",
            ),
            (6, "Decorated", PreserveSized, 3, 54, 84, 2, "74-76:136"),
            (
                25,
                "Paint",
                Coalesce,
                1,
                135,
                81,
                1,
                "65-67:119,67-83:221,83-86:85,103-107:153,107-115:204,115-116:170",
            ),
            (
                25,
                "Paint",
                PreserveStructure,
                3,
                54,
                94,
                1,
                "65-67:119,67-83:221,83-86:85",
            ),
            (
                25,
                "Paint",
                PreserveSized,
                3,
                54,
                62,
                2,
                "71-75:153,75-83:204,83-84:170",
            ),
            (
                26,
                "PaintAfter",
                Coalesce,
                1,
                103,
                81,
                1,
                "71-75:153,75-83:204,83-84:170",
            ),
            (
                26,
                "PaintAfter",
                PreserveSized,
                2,
                54,
                62,
                1,
                "71-75:153,75-83:204,83-84:170",
            ),
            (
                27,
                "PaintBefore",
                PreserveStructure,
                2,
                103,
                94,
                2,
                "71-75:153,75-83:204,83-84:170",
            ),
            (
                27,
                "PaintBefore",
                PreserveSized,
                2,
                103,
                81,
                2,
                "71-75:153,75-83:204,83-84:170",
            ),
            (
                28,
                "PaintHeight",
                Coalesce,
                1,
                135,
                81,
                1,
                "65-67:119,67-83:221,83-86:85,103-107:153,107-115:204,115-116:170",
            ),
            (
                28,
                "PaintHeight",
                PreserveStructure,
                3,
                54,
                130,
                1,
                "65-67:119,67-98:221,98-101:85",
            ),
            (
                28,
                "PaintHeight",
                PreserveSized,
                3,
                54,
                114,
                2,
                "71-75:153,75-111:204,111-112:170",
            ),
        ];
        for (chapter, name, mode, page, screen_y, extent, band_page, expected_bands) in fixtures {
            let mut probe = open_with_geometry(book, mode, chapter, true);
            let baseline = open(book, mode, chapter);
            let phrase = format!("Test {name} last marker");
            let start = baseline.matches(&phrase)[0].clone();
            let end = Anchor {
                raw_caret: None,
                offset: start.offset + phrase.len(),
                ..start.clone()
            };
            let selection = baseline.select_range(&start, &end).unwrap();
            assert_eq!(probe.select_range(&start, &end), Some(selection.clone()));
            assert_eq!(probe.page_for(&start), Some(page), "{mode:?} {name}");
            let measured = probe.anchor_top(&start).unwrap() - baseline.anchor_top(&start).unwrap();
            assert!(
                (measured - extent as f32).abs() < 1.,
                "{mode:?} {name} measured extent {measured}, expected {extent}"
            );
            let rect = probe.selection_rects(page, &selection)[0];
            assert_eq!((rect.x, rect.y), (54, screen_y), "{mode:?} {name}");
            let captured = probe.capture(band_page).unwrap();
            let bands = sample_bands(&captured, 1201, settings(mode).width as usize);
            assert_eq!(bands, expected_bands, "{mode:?} {name} page {band_page}");
            if band_page < page {
                assert_eq!(probe.page_anchor(band_page), None);
            }
            println!(
                "{mode:?}|{name}|advance={page}|screen_y={screen_y}|extent={measured:.2}|bands={bands}"
            );
            if let Some(dir) = std::env::var_os("READER_EMPTY_GEOMETRY_CAPTURE_DIR") {
                let path = std::path::PathBuf::from(dir);
                let settings = settings(mode);
                image::GrayImage::from_raw(settings.width, settings.height, captured)
                    .unwrap()
                    .save(path.join(format!("{mode:?}-{name}-{band_page}.png")))
                    .unwrap();
            }
        }
    }

    fn sample_bands(frame: &[u8], x: usize, width: usize) -> String {
        let mut bands = Vec::new();
        let (mut start, mut shade) = (0, 255);
        for (y, next) in frame
            .chunks_exact(width)
            .map(|row| row[x])
            .chain([255])
            .enumerate()
        {
            if next != shade {
                if shade != 255 {
                    bands.push(format!("{start}-{y}:{shade}"));
                }
                (start, shade) = (y, next);
            }
        }
        bands.join(",")
    }

    #[test]
    fn existing_render_mode_specimen_has_the_measured_break_sequence() {
        for (mode, end) in [
            (EmptyBreakMode::Coalesce, 1),
            (EmptyBreakMode::PreserveStructure, 3),
            (EmptyBreakMode::PreserveSized, 3),
        ] {
            let mut probe = open("render-modes.epub", mode, 1);
            for (phrase, page) in [
                ("Before empty breaks", 0),
                ("After empty breaks", end),
                ("After nonempty break", end + 1),
            ] {
                assert_eq!(probe.page_for(&probe.matches(phrase)[0]), Some(page));
            }
            assert_eq!(probe.page_count(), end + 2);
            if let Some(dir) = std::env::var_os("READER_EMPTY_BREAK_CAPTURE_DIR") {
                let dir = std::path::PathBuf::from(dir);
                for page in 0..probe.page_count() {
                    let img = probe.capture(page).unwrap();
                    let settings = settings(mode);
                    image::GrayImage::from_raw(settings.width, settings.height, img)
                        .unwrap()
                        .save(dir.join(format!("{mode:?}-{page}.png")))
                        .unwrap();
                }
            }
        }
    }

    #[cfg(feature = "kindle")]
    #[test]
    fn paged_resume_uses_a_blank_slot_then_recovers_an_exact_text_anchor() {
        let config = crate::session::BookConfig {
            viewport: crate::session::ReadingViewport::PORTRAIT,
            user_styles: Default::default(),
            user_hyphenation: Default::default(),
            engine: Engine::Parley,
            book: crate::test_fixture("render-empty-breaks.epub"),
            fonts: vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")],
            font_roles: Default::default(),
            settings: settings(EmptyBreakMode::PreserveStructure),
            scroll: false,
            facing_pages: false,
            hide_nonlinear: false,
            hidden_flows: Default::default(),
            chapter_offsets: Default::default(),
            quote_prefixes: Default::default(),
            screen_pages: Default::default(),
            eager_layouts: Default::default(),
            folder_snapshot: None,
        };
        let mut reading = crate::session::Reading::restore(config.clone(), 0, 1, None).unwrap();
        assert_eq!(reading.page, 1);
        assert_eq!(reading.anchor(), None);
        assert!(
            reading
                .probe
                .capture(1)
                .unwrap()
                .into_iter()
                .all(|pixel| pixel == 255)
        );
        reading.go_to_page(0, 2).unwrap();
        assert_eq!(reading.page, 2);
        assert_eq!(reading.anchor(), None);
        reading.go_to_page(0, 3).unwrap();
        let anchor = reading.probe.matches("Test Bare last marker")[0].clone();
        assert_eq!(reading.anchor(), Some(anchor.clone()));
        let mut compact = config;
        compact.settings.empty_break_mode = EmptyBreakMode::Coalesce;
        let restored = crate::session::Reading::restore(compact, 0, 3, Some(&anchor)).unwrap();
        assert_eq!(restored.page, 1);
        assert_eq!(restored.anchor(), Some(anchor));
    }
}
