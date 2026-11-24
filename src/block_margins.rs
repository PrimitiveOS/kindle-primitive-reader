//! Opt-in model for vertical publisher margins on retained horizontal blocks.
use crate::{
    BlockMarginMode, Settings,
    book_style::{Style, flow::Display},
    publisher_padding::BoxArea,
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

pub(crate) fn before_blocks(
    source: &Source,
    styles: &[Style],
    boxes: Option<&[BoxArea]>,
    settings: Settings,
) -> Option<Vec<f32>> {
    let mode = settings.block_margins;
    let width = settings.text_width();
    let font = settings.font_size;
    if mode == BlockMarginMode::Ignore
        || !styles.iter().any(|style| {
            style
                .vertical_margin
                .iter()
                .any(|edge| edge.pixels(width, font) != 0.0)
        })
    {
        return None;
    }

    // An owner can contain multiple source blocks. Its leading/trailing edges
    // belong to the outer flow once, regardless of wrapping or page breaks.
    let mut spans = vec![None; source.elements.len()];
    let mut following = vec![None; source.blocks.len()];
    let mut next = None;
    for (index, block) in source.blocks.iter().enumerate().rev() {
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
    let single = |margin: f32| if signed { margin } else { margin.max(0.0) };
    let mut edges = vec![None; spans.len()];
    let mut previous_visible = vec![None; spans.len()];
    let mut gaps = vec![0.0; source.blocks.len()];
    for (index, element) in source.elements.iter().enumerate() {
        let previous = element.previous_sibling.and_then(|sibling| {
            if styles[sibling].flow.suppressed {
                previous_visible[sibling]
            } else {
                Some(sibling)
            }
        });
        previous_visible[index] = previous;
        let Some(span) = spans[index] else {
            continue;
        };
        let style = &styles[index];
        if span.has_grid
            || element.selector_only
            || element.anonymous
            || matches!(element.tag.as_str(), "html" | "body")
            || !matches!(
                Display::native(&element.tag),
                Display::Block | Display::ListItem
            )
            || !matches!(
                style.display,
                Display::Block | Display::ListItem | Display::LegacyListItem
            )
        {
            continue;
        }
        let containing_width = boxes
            .zip(element.parent)
            .map_or(width, |(areas, parent)| areas[parent].width);
        let [top, bottom] = style
            .vertical_margin
            .map(|edge| edge.box_pixels(containing_width, font).clamp(-limit, limit));
        edges[index] = Some(bottom);
        gaps[span.first] += single(top);
        if let Some(after) = following[span.last] {
            gaps[after] += single(bottom);
        }

        // Parenting, intervening content and table flow each form a boundary.
        // Replace only the two touching sibling edges; any other margins at the
        // same source block remain independently owned by their boxes.
        if mode != BlockMarginMode::Separate
            && let Some(sibling) = previous
            && let Some(other) = edges[sibling]
            && let Some(left) = spans[sibling]
            && source.elements[sibling].parent == element.parent
            && following[left.last] == Some(span.first)
        {
            let collapsed = other.max(top).max(0.0) + other.min(top).min(0.0);
            gaps[span.first] += single(collapsed) - single(other) - single(top);
        }
    }
    for gap in &mut gaps {
        *gap = gap.clamp(-limit, limit);
    }
    Some(gaps)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Anchor, Engine, Probe};

    fn spacing(body: &str, mode: BlockMarginMode) -> Vec<(String, f32)> {
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
        let gaps = before_blocks(&source, &styles, None, settings)
            .unwrap_or_else(|| vec![0.0; source.blocks.len()]);
        source
            .blocks
            .into_iter()
            .map(|block| block.text)
            .zip(gaps)
            .collect()
    }

    fn gap(layout: &[(String, f32)], block: &str) -> f32 {
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
