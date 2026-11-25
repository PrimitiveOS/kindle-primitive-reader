//! Vertical CSS margins for retained horizontal blocks and paragraph presets.
use crate::{
    BlockMarginMode, FloatMode, Settings,
    book_style::{ImageLength, Style, WritingMode, borders::Pattern, flow::Display},
    publisher_breaks::Edge,
    publisher_padding::{BoxArea, TextBoxCandidate, text_box_candidates},
    source::Source,
};

#[derive(Clone, Copy)]
struct Span {
    first: usize,
    last: usize,
    has_grid: bool,
}

impl Span {
    fn include(place: &mut Option<Self>, other: Self) {
        *place = Some(place.map_or(other, |current| Self {
            first: current.first.min(other.first),
            last: current.last.max(other.last),
            has_grid: current.has_grid || other.has_grid,
        }));
    }
}

#[derive(Clone, Copy, Default)]
struct Margin {
    positive: f32,
    negative: f32,
}

impl Margin {
    fn new(length: f32) -> Self {
        Self {
            positive: length.max(0.0),
            negative: length.min(0.0),
        }
    }

    fn joined(self, other: Self) -> Self {
        Self {
            positive: self.positive.max(other.positive),
            negative: self.negative.min(other.negative),
        }
    }

    fn used(self, signed: bool) -> f32 {
        let length = self.positive + self.negative;
        if signed { length } else { length.max(0.0) }
    }
}

pub(crate) fn collapsible_wrapper(tag: &str) -> bool {
    matches!(tag, "div" | "section" | "article")
}

#[derive(Clone, Copy, Default)]
pub(crate) struct Space {
    pub before: f32,
    pub forced_top: Option<f32>,
    pub suppress_gap_after: bool,
}

pub(crate) fn before_blocks(
    source: &Source,
    styles: &[Style],
    boxes: Option<&[BoxArea]>,
    settings: Settings,
    order: Option<&[usize]>,
) -> Option<Vec<Space>> {
    let mode = settings.block_margins;
    let ignore_publisher = mode == BlockMarginMode::Ignore;
    let width = settings.text_width();
    let font = settings.font_size;
    if !styles.iter().any(|style| {
        style.paragraph_edges.contains(&true)
            || !ignore_publisher
                && style
                    .vertical_margin
                    .iter()
                    .any(|edge| edge.pixels(width, font) != 0.0)
    }) {
        return None;
    }

    // An owner can contain multiple source blocks. Its leading/trailing edges
    // belong to the outer flow once, regardless of wrapping or page breaks.
    let mut spans = vec![None; source.elements.len()];
    let mut following = vec![None; source.blocks.len()];
    let mut next = None;
    for index in (0..source.blocks.len()).rev() {
        let block = &source.blocks[crate::table_layout::captions::source_index(order, index)];
        following[index] = next;
        if !styles[block.element].flow.suppressed {
            Span::include(
                &mut spans[block.element],
                Span {
                    first: index,
                    last: index,
                    has_grid: block.table_cell.is_some(),
                },
            );
            next = Some(index);
        }
    }
    for (index, element) in source.elements.iter().enumerate().rev() {
        if let (Some(span), Some(parent)) = (spans[index], element.parent) {
            Span::include(&mut spans[parent], span);
        }
    }

    let limit = (settings.text_height() as f32 / 2.0).max(1.0);
    let signed = mode == BlockMarginMode::CollapseNegative;
    let collapse = mode != BlockMarginMode::Separate;
    let mut edges = vec![None; spans.len()];
    let mut child_edges = collapse.then(|| vec![None; spans.len()]);
    let mut previous_visible = vec![None; spans.len()];
    for (index, element) in source.elements.iter().enumerate() {
        let previous = element.previous_sibling.and_then(|sibling| {
            if styles[sibling].flow.suppressed {
                previous_visible[sibling]
            } else {
                Some(sibling)
            }
        });
        previous_visible[index] = previous;
        let style = &styles[index];
        if !ignore_publisher
            && let (Some(children), Some(parent)) = (&mut child_edges, element.parent)
            && !element.selector_only
            && !style.flow.suppressed
        {
            let ends = children[parent].get_or_insert([index, index]);
            ends[1] = index;
        }
        let Some(span) = spans[index] else {
            continue;
        };
        let table = span.has_grid && element.tag == "table" && style.display == Display::Table;
        let ordinary_block = !span.has_grid
            && matches!(
                Display::native(&element.tag),
                Display::Block | Display::ListItem
            )
            && matches!(
                style.display,
                Display::Block | Display::ListItem | Display::LegacyListItem
            );
        if style.flow.suppressed
            || element.selector_only
            || element.anonymous
            || ignore_publisher && !style.paragraph_edges.contains(&true)
            || matches!(element.tag.as_str(), "html" | "body")
            || !(table || ordinary_block)
        {
            continue;
        }
        let containing_width = boxes
            .zip(element.parent)
            .map_or(width, |(areas, parent)| areas[parent].width);
        let [top, bottom] = [0, 1].map(|side| {
            if ignore_publisher && !style.paragraph_edges[side] {
                0.0
            } else {
                style.vertical_margin[side]
                    .box_pixels(containing_width, font)
                    .clamp(-limit, limit)
            }
        });
        edges[index] = Some([Margin::new(top), Margin::new(bottom)]);
    }
    if let Some(children) = child_edges {
        join_child_margins(
            source, styles, boxes, settings, &spans, &children, &mut edges,
        );
    }

    let forced_candidates = if settings.preserve_forced_page_margins
        && collapse
        && (settings.float_mode == FloatMode::Ignore
            || !styles.iter().any(|style| style.float_side.is_some()))
        && styles.iter().any(|style| {
            style.page_breaks.before == Edge::Force || style.page_breaks.after == Edge::Force
        }) {
        text_box_candidates(source, styles, |style| {
            style.clear == Default::default()
                && !style.page_breaks.keep_together
                && style.borders.is_none()
                && style
                    .vertical_padding
                    .iter()
                    .all(|edge| edge.box_pixels(width, font) == 0.0)
        })
    } else {
        Vec::new()
    };
    let mut gaps = vec![Space::default(); source.blocks.len()];
    let mut paragraph_starts = vec![false; source.blocks.len()];
    for (index, pair) in edges.iter().enumerate() {
        let Some([top, bottom]) = *pair else { continue };
        let span = spans[index].unwrap();
        if styles[index].writing_mode == WritingMode::Horizontal {
            let [top, bottom] = styles[index].paragraph_edges;
            paragraph_starts[span.first] |= top;
            gaps[span.last].suppress_gap_after |= bottom;
        }
        gaps[span.first].before += top.used(signed);
        if let Some(after) = following[span.last] {
            gaps[after].before += bottom.used(signed);
        }

        // Each edge may already carry adjoining descendant margins. Combine
        // both extrema before flooring the net value in the unsigned policy.
        if collapse
            && let Some(sibling) = previous_visible[index]
            && let Some([_, other]) = edges[sibling]
            && let Some(left) = spans[sibling]
            && source.elements[sibling].parent == source.elements[index].parent
            && following[left.last] == Some(span.first)
        {
            gaps[span.first].before +=
                top.joined(other).used(signed) - other.used(signed) - top.used(signed);
            if forced_candidates.get(index) == Some(&TextBoxCandidate::Simple)
                && forced_candidates.get(sibling) == Some(&TextBoxCandidate::Simple)
                && top.negative == 0.0
                && plain_forced_edge(source, styles, sibling, index)
            {
                gaps[span.first].forced_top = Some(top.positive);
            }
        }
    }
    for (gap, next) in gaps.iter_mut().zip(following) {
        gap.suppress_gap_after |= next.is_some_and(|next| paragraph_starts[next]);
        gap.before = gap
            .forced_top
            .unwrap_or_else(|| gap.before.clamp(-limit, limit));
    }
    Some(gaps)
}

fn plain_forced_edge(source: &Source, styles: &[Style], previous: usize, next: usize) -> bool {
    let before = styles[next].page_breaks.before;
    let after = styles[previous].page_breaks.after;
    if (before != Edge::Force && after != Edge::Force)
        || before == Edge::Avoid
        || after == Edge::Avoid
    {
        return false;
    }
    // A local margin change must not outrun an enclosing keep or a separate
    // source rule that can prevent the physical page break.
    let body = source.elements[next].parent.unwrap();
    styles[body].page_breaks == Default::default()
        && source.elements[body]
            .parent
            .is_none_or(|html| styles[html].page_breaks == Default::default())
}

fn join_child_margins(
    source: &Source,
    styles: &[Style],
    boxes: Option<&[BoxArea]>,
    settings: Settings,
    spans: &[Option<Span>],
    children: &[Option<[usize; 2]>],
    edges: &mut [Option<[Margin; 2]>],
) {
    let ordinary = |style: &Style| {
        style.display == Display::Block
            && style.writing_mode == WritingMode::Horizontal
            && style.float_side.is_none()
            && style.clear == Default::default()
            && style.page_breaks == Default::default()
    };
    // Descendant extrema move to the enclosing edge before adjacent siblings
    // are combined. Only boxes with an adjoining source boundary can join.
    for index in (0..children.len()).rev() {
        let style = &styles[index];
        let (Some(children), Some(span), Some(mut pair)) =
            (children[index], spans[index], edges[index])
        else {
            continue;
        };
        if !collapsible_wrapper(&source.elements[index].tag)
            || !ordinary(style)
            || style.background.is_some()
            || !matches!(style.image_height, None | Some(ImageLength::Auto))
            || !matches!(style.image_max_height, None | Some(ImageLength::Auto))
        {
            continue;
        }
        let width = boxes.map_or(settings.text_width(), |areas| areas[index].outer_width);
        let font = settings.font_size;
        for (side, child) in children.into_iter().enumerate() {
            let child_style = &styles[child];
            let (Some(child_span), Some(child_pair)) = (spans[child], &mut edges[child]) else {
                continue;
            };
            let border = style.borders.as_ref().map(|sides| sides[side * 2]);
            if ordinary(child_style)
                && [span.first, span.last][side] == [child_span.first, child_span.last][side]
                && style.vertical_padding[side].box_pixels(width, font) == 0.0
                && !border.is_some_and(|edge| {
                    !matches!(edge.pattern, Pattern::None | Pattern::Hidden)
                        && edge.width.box_pixels(width, font) > 0.0
                })
            {
                pair[side] = pair[side].joined(std::mem::take(&mut child_pair[side]));
            }
        }
        edges[index] = Some(pair);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Anchor, Engine, Probe};

    pub(super) fn spacing(body: &str, mode: BlockMarginMode) -> Vec<(String, f32)> {
        let source = Source::parse(&format!("<html><body>{body}</body></html>")).unwrap();
        let mut book = crate::Book::open(&crate::test_fixture("field-notes-epub3.epub")).unwrap();
        let href = book.chapters[0].href.clone();
        let settings = Settings {
            font_size: 40.0,
            block_margins: mode,
            ..Settings::default()
        };
        let styles = crate::book_style::BookStyle::load_for_flow(
            &mut book,
            &href,
            &source,
            true,
            &Default::default(),
        )
        .unwrap()
        .compute(&source.elements);
        let gaps = before_blocks(&source, &styles, None, settings, None)
            .unwrap_or_else(|| vec![Space::default(); source.blocks.len()]);
        source
            .blocks
            .into_iter()
            .map(|block| block.text)
            .zip(gaps.into_iter().map(|gap| gap.before))
            .collect()
    }

    pub(super) fn gap(layout: &[(String, f32)], block: &str) -> f32 {
        layout.iter().find(|(name, _)| name == block).unwrap().1
    }

    #[test]
    fn touching_siblings_and_signed_em_edges_follow_their_selected_policy() {
        let body = concat!(
            "<p style='margin: 0 auto 2em'>positive left</p>",
            "<p style='display:none;margin-top:100px;margin-bottom:120px'>removed</p>",
            "<p style='margin-top:1em'>positive right</p>",
            "<p style='margin-bottom:1.8em'>mixed left</p>",
            "<p style='margin-top:-.8em'>mixed right</p>",
            "<p style='margin-bottom:.2em'>negative left</p>",
            "<p style='margin-top:-.6em'>negative right</p>",
            "<p style='margin-bottom:-7px'>both left</p>",
            "<p style='margin-top:-10px'>both right</p>",
        );
        for (mode, expected) in [
            (BlockMarginMode::Ignore, [0., 0., 0., 0.]),
            (BlockMarginMode::Separate, [120., 72., 7., 0.]),
            (BlockMarginMode::Collapse, [80., 39., 0., 0.]),
            (BlockMarginMode::CollapseNegative, [80., 39., -18., -10.]),
        ] {
            let layout = spacing(body, mode);
            let actual = [
                "positive right",
                "mixed right",
                "negative right",
                "both right",
            ]
            .map(|block| gap(&layout, block));
            assert_eq!(actual, expected, "{mode:?}");
            assert_eq!(gap(&layout, "removed"), 0.0);
        }
    }

    #[test]
    fn wrapper_edges_apply_once_and_padding_table_and_empty_boundaries_are_preserved() {
        let body = concat!(
            "<p style='margin-bottom:13px;padding-bottom:2px'>above wrapper</p>",
            "<section style='padding:3px 0;margin: 11px auto 17px'>",
            "<p style='margin: 5px auto 8px; margin-bottom: 7px'>wrapper start</p>",
            "<p style='margin: 3px 0 7px'>wrapper end</p>",
            "</section>",
            "<p style='margin-top:11px'>below wrapper</p>",
            "<p style='margin-bottom:13px'>above grid</p>",
            "<table><tr><td><p style='margin:55px 0'>in grid</p></td><td>right cell</td></tr></table>",
            "<p style='margin-top:17px'>below grid</p>",
            "<p style='margin-bottom:10px'>above empty element</p>",
            "<section></section><p style='margin-top:5px'>below empty element</p>",
        );
        let collapsed = spacing(body, BlockMarginMode::Collapse);
        let expected = [
            ("wrapper start", 13.0 + 5.0),
            ("wrapper end", 7.0),
            ("below wrapper", 17.0 + 7.0),
            ("in grid", 13.0),
            ("right cell", 0.0),
            ("below grid", 17.0),
            ("below empty element", 15.0),
        ];
        for (block, expected) in expected {
            assert_eq!(gap(&collapsed, block), expected, "{block}");
        }
    }

    #[test]
    fn native_margin_pitches_and_canonical_resume_survive_provider_reflow() {
        let book = crate::test_fixture("render-modes.epub");
        let fonts = [crate::test_fixture("fonts/NotoSans-Regular.ttf")];
        let settings = Settings {
            width: 1264,
            height: 1606,
            font_size: 40.0,
            line_height: 1.10,
            ..Settings::default()
        };
        let pairs = [
            ("Before joined margins", "After joined margins"),
            ("Before negative margins", "After negative margins"),
            ("Before net negative gap", "After net negative gap"),
        ];
        let pitches = |probe: &Probe| {
            pairs.map(|(left, right)| {
                let top = |phrase| probe.anchor_top(&probe.matches(phrase)[0]).unwrap();
                top(right) - top(left)
            })
        };
        let mut control = Probe::open(&book, Engine::Parley, &fonts, settings, 0).unwrap();
        let baseline = pitches(&control);
        let original = control.capture(0).unwrap();
        let start = control.matches("After joined margins")[0].clone();
        let end = Anchor {
            offset: start.offset + "After".len(),
            ..start.clone()
        };
        let saved = serde_json::to_vec(&control.select_range(&start, &end).unwrap()).unwrap();
        let restored: crate::selection::TextSelection = serde_json::from_slice(&saved).unwrap();

        for (mode, expected_margins, native) in [
            (BlockMarginMode::Ignore, [0., 0., 0.], [64., 64., 64.]),
            (
                BlockMarginMode::Separate,
                [120., 72., 7.],
                [184., 136., 71.],
            ),
            (BlockMarginMode::Collapse, [80., 39., 0.], [144., 103., 64.]),
            (
                BlockMarginMode::CollapseNegative,
                [80., 39., -18.],
                [144., 103., 46.],
            ),
        ] {
            let cfg = Settings {
                block_margins: mode,
                ..settings
            };
            let mut probe = Probe::open(&book, Engine::Parley, &fonts, cfg, 0).unwrap();
            let measured = pitches(&probe);
            println!(
                "{mode:?} Reader pitches={measured:?}; control={baseline:?}; stock={native:?}"
            );
            for i in 0..pairs.len() {
                assert!(((measured[i] - baseline[i]) - expected_margins[i]).abs() < 0.01);
                assert_eq!(64.0 + expected_margins[i], native[i]);
            }
            let selection = probe.select_range(&restored.start, &restored.end).unwrap();
            assert_eq!(selection, restored);
            assert_eq!(probe.matches("After joined margins")[0], restored.start);
            let page = probe.page_for(&restored.start).unwrap();
            let rect = probe.selection_rects(page, &selection)[0];
            let x = rect.x as f32 + rect.width as f32 * 0.5;
            let y = rect.y as f32 + rect.height as f32 * 0.5;
            assert_eq!(probe.word_at(page, x, y), Some(restored.clone()));

            let frame = probe.capture(0).unwrap();
            if mode == BlockMarginMode::Ignore {
                assert_eq!(frame, original);
            } else {
                assert_ne!(frame, original);
            }
            if let Some(dir) = std::env::var_os("READER_BLOCK_MARGINS_CAPTURE_DIR") {
                let path = std::path::PathBuf::from(dir).join(format!("{mode:?}.png"));
                image::GrayImage::from_raw(settings.width, settings.height, frame)
                    .unwrap()
                    .save(path)
                    .unwrap();
            }
        }
    }

    #[test]
    fn a_saved_source_range_resumes_on_its_new_page_after_margin_reflow() {
        let book = crate::test_fixture("render-modes.epub");
        let fonts = [crate::test_fixture("fonts/NotoSans-Regular.ttf")];
        let settings = Settings {
            height: 600,
            font_size: 40.0,
            line_height: 1.10,
            ..Settings::default()
        };
        let phrase = "After net negative gap";
        let baseline = Probe::open(&book, Engine::Parley, &fonts, settings, 0).unwrap();
        let start = baseline.matches(phrase)[0].clone();
        let end = Anchor {
            offset: start.offset + phrase.len(),
            ..start.clone()
        };
        let original = baseline.select_range(&start, &end).unwrap();
        let original_page = baseline.page_for(&start).unwrap();

        let changed = Settings {
            block_margins: BlockMarginMode::CollapseNegative,
            ..settings
        };
        let reopened = Probe::open(&book, Engine::Parley, &fonts, changed, 0).unwrap();
        let new_page = reopened.page_for(&start).unwrap();
        assert!(new_page > original_page);
        let restored = reopened
            .select_range(&original.start, &original.end)
            .unwrap();
        assert_eq!(restored, original);
        assert_eq!(restored.text, phrase);
        assert!(!reopened.selection_rects(new_page, &restored).is_empty());
        assert!(
            reopened
                .selection_rects(original_page, &restored)
                .is_empty()
        );
    }
}

#[cfg(test)]
mod parent_tests;
