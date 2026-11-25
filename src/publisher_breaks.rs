//! Publisher page breaks in horizontal reflow, expressed in document coordinates.
use super::{Line, Settings, book_style::Style, source::Source};
use std::ops::Range;

mod empty;
mod opening;
mod text_keeps;
pub(crate) use empty::physical::plan as plan_empty_geometry;

#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Edge {
    #[default]
    Auto,
    Force,
    // CREngine's line splitter gives an explicit keep priority at a shared edge.
    Avoid,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Rules {
    pub before: Edge,
    pub after: Edge,
    pub keep_together: bool,
}

pub(crate) struct PositionedRun {
    pub target: usize,
    pub cuts: Box<[f32]>,
}

pub(crate) struct BoxRules {
    extent: Range<f32>,
    rules: Rules,
    prefer_opening: bool,
    extra_splits: u16,
    physical_cuts: Option<Box<[f32]>>,
}

/// Protect only the opening and closing lines of each formatted paragraph. Taking
/// each end in its own pass also handles interleaved lines from adjacent table
/// cells without copying text or retaining an unbounded list of line positions.
fn protect_paragraph_end<'a>(
    source: &Source,
    styles: &[Style],
    lines: impl Iterator<Item = &'a Line>,
    minimum: fn(&Style) -> u8,
    result: &mut Vec<BoxRules>,
) {
    if !styles.iter().any(|style| minimum(style) > 1) {
        return;
    }
    let mut ends = vec![(0_u8, None::<Range<f32>>); source.blocks.len()];
    for line in lines.filter(|line| line.bottom > line.top) {
        let Ok(i) = source
            .blocks
            .binary_search_by_key(&line.block, |block| block.key)
        else {
            continue;
        };
        if styles[source.blocks[i].element].flow.suppressed {
            continue;
        }
        let limit = minimum(&styles[source.blocks[i].element]);
        let (count, extent) = &mut ends[i];
        if limit > 1 && *count < limit {
            let paint = line.page_extent();
            let extent = extent.get_or_insert(paint.clone());
            extent.start = extent.start.min(paint.start);
            extent.end = extent.end.max(paint.end);
            *count += 1;
        }
    }
    for (count, extent) in ends {
        if count > 1
            && let Some(extent) = extent
        {
            result.push(BoxRules {
                extent,
                rules: Rules {
                    keep_together: true,
                    ..Default::default()
                },
                prefer_opening: false,
                extra_splits: 0,
                physical_cuts: None,
            });
        }
    }
}

/// Find the first and last painted lines of each styled block, including wrappers
/// around other blocks. An empty separator shares the next visible block's edge.
#[cfg(test)]
pub(crate) fn geometry(source: &Source, styles: &[Style], lines: &[Line]) -> Vec<BoxRules> {
    layout(source, styles, lines, Settings::default(), &[], &[])
}

pub(crate) fn layout(
    source: &Source,
    styles: &[Style],
    lines: &[Line],
    settings: Settings,
    positioned: &[PositionedRun],
    fitted_tails: &[crate::publisher_padding::fit_content::HeightTail],
) -> Vec<BoxRules> {
    let mut result = Vec::new();
    protect_paragraph_end(
        source,
        styles,
        lines.iter(),
        |style| style.orphans,
        &mut result,
    );
    protect_paragraph_end(
        source,
        styles,
        lines.iter().rev(),
        |style| style.widows,
        &mut result,
    );
    if !styles
        .iter()
        .any(|style| style.page_breaks != Rules::default())
    {
        return result;
    }
    let mut boxes = vec![None; styles.len()];
    let extend = |boxes: &mut Vec<Option<Range<f32>>>, index: usize, top: f32, bottom: f32| {
        let extent = boxes[index].get_or_insert(top..bottom);
        extent.start = extent.start.min(top);
        extent.end = extent.end.max(bottom);
    };
    for line in lines {
        if let Ok(i) = source.blocks.binary_search_by_key(&line.block, |b| b.key) {
            if styles[source.blocks[i].element].flow.suppressed {
                continue;
            }
            let extent = line.page_extent();
            extend(
                &mut boxes,
                source.blocks[i].element,
                extent.start,
                extent.end,
            );
        }
    }
    for tail in fitted_tails {
        extend(
            &mut boxes,
            tail.element,
            tail.closing.start,
            tail.closing.end,
        );
    }
    let expands = settings.empty_break_mode.expands();
    let direct = (expands || !positioned.is_empty()).then(|| boxes.clone());
    for (i, element) in source.elements.iter().enumerate().rev() {
        if let (Some(p), Some(extent)) = (element.parent, boxes[i].clone()) {
            extend(&mut boxes, p, extent.start, extent.end);
        }
    }
    let oversized = text_keeps::oversized(source, styles, settings, &boxes);
    let openings = opening::explicit_before(source, styles, settings);
    let mut following = None;
    for (i, element) in source.elements.iter().enumerate().rev() {
        if element.selector_only
            || styles[i].flow.suppressed
            || !(crate::source::is_block_tag(&element.tag)
                || matches!(element.tag.as_str(), "ol" | "ul" | "table" | "tr" | "hr"))
        {
            continue;
        }
        let mut rules = styles[i].page_breaks;
        if oversized.get(i) == Some(&true) {
            rules.keep_together = false;
        }
        let extent = boxes[i].clone();
        if rules != Rules::default() {
            if let Some(extent) = extent.clone().or_else(|| following.map(|y| y..y)) {
                result.push(BoxRules {
                    extent,
                    rules,
                    prefer_opening: openings.get(i) == Some(&true),
                    extra_splits: 0,
                    physical_cuts: None,
                });
            }
        }
        if let Some(extent) = extent {
            following = Some(extent.start);
        }
    }
    if let Some(direct) = &direct {
        if expands {
            empty::propagate(source, styles, direct, &boxes, settings, &mut result);
        }
        for run in positioned {
            if let Some(range) = &direct[run.target] {
                result.push(BoxRules {
                    extent: range.start..range.start,
                    rules: Rules::default(),
                    prefer_opening: false,
                    extra_splits: 0,
                    physical_cuts: Some(run.cuts.clone()),
                });
            }
        }
    }
    result
}

/// Each slot describes the gap immediately before a non-overlapping line group.
/// Difference counts keep nested tables and long groups linear in layout size.
pub(crate) fn boundaries(lines: &[Range<f32>], boxes: &[BoxRules]) -> Vec<Edge> {
    if boxes.is_empty() || lines.is_empty() {
        return Vec::new();
    }
    let mut edges = vec![Edge::Auto; lines.len()];
    let mut keeps = vec![0_i32; lines.len() + 1];
    for item in boxes {
        let first = lines.partition_point(|line| line.end <= item.extent.start);
        let end = lines.partition_point(|line| line.start < item.extent.end);
        if let Some(edge) = edges.get_mut(first) {
            *edge = (*edge).max(item.rules.before);
        }
        if let Some(edge) = edges.get_mut(end) {
            *edge = (*edge).max(item.rules.after);
        }
        if item.rules.keep_together && first + 1 < end {
            keeps[first + 1] += 1;
            keeps[end] -= 1;
        }
    }
    let mut active = 0;
    for (edge, change) in edges.iter_mut().zip(keeps) {
        active += change;
        if active > 0 {
            *edge = Edge::Avoid;
        }
    }
    // Enhanced CREngine resolves an explicit opening margin before the
    // line splitter combines the neighboring flags. Only reproduce that
    // when the real paragraph starts at its own, nonoverlapping interval.
    for item in boxes.iter().filter(|item| item.prefer_opening) {
        let first = lines.partition_point(|line| line.end <= item.extent.start);
        if first > 0
            && item.extent.start < item.extent.end
            && lines
                .get(first)
                .is_some_and(|line| line.start == item.extent.start && line.end <= item.extent.end)
        {
            edges[first] = Edge::Force;
        }
    }
    edges
}

/// The in-page composer shares ordinary hard breaks and fitting keep groups.
/// Structural cuts with synthetic blank space use the ordinary paginator.
pub(crate) fn in_page_note_boundaries(
    lines: &[Range<f32>],
    boxes: &[BoxRules],
) -> Option<Vec<Edge>> {
    if boxes
        .iter()
        .any(|item| item.extra_splits > 0 || item.physical_cuts.is_some())
    {
        return None;
    }
    Some(boundaries(lines, boxes))
}

pub(crate) fn structural_splits(lines: &[Range<f32>], boxes: &[BoxRules]) -> Vec<u16> {
    if !boxes.iter().any(|item| item.extra_splits > 0) {
        return Vec::new();
    }
    let mut extra = vec![0; lines.len()];
    for item in boxes.iter().filter(|item| item.extra_splits > 0) {
        let first = lines.partition_point(|line| line.end <= item.extent.start);
        // A float or table row can connect the target line to an earlier
        // interval. Only add blank pages at an actual boundary of that flow.
        if lines
            .get(first)
            .is_some_and(|line| line.start == item.extent.start)
        {
            extra[first] = extra[first].max(item.extra_splits);
        }
    }
    extra
}

pub(crate) fn physical_cuts<'a>(
    lines: &[Range<f32>],
    boxes: &'a [BoxRules],
) -> Vec<Option<&'a [f32]>> {
    if !boxes.iter().any(|item| item.physical_cuts.is_some()) {
        return Vec::new();
    }
    let mut cuts = vec![None; lines.len()];
    for item in boxes {
        let Some(positions) = item.physical_cuts.as_deref() else {
            continue;
        };
        let first = lines.partition_point(|line| line.end <= item.extent.start);
        if first > 0
            && lines
                .get(first)
                .is_some_and(|line| line.start == item.extent.start)
            && positions
                .first()
                .is_some_and(|&y| y >= lines[first - 1].end)
            && positions.last().is_some_and(|&y| y <= item.extent.start)
        {
            cuts[first] = Some(positions);
        }
    }
    cuts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Book, Engine, Pagination, Probe, Settings};

    #[test]
    fn structural_splits_do_not_move_to_the_start_of_an_overlapping_line_group() {
        let marker = |at| BoxRules {
            extent: at..at,
            rules: Rules::default(),
            prefer_opening: false,
            extra_splits: 2,
            physical_cuts: None,
        };
        assert_eq!(
            structural_splits(&[0.0..20.0, 30.0..80.0], &[marker(30.0)]),
            [0, 2]
        );
        assert_eq!(
            structural_splits(&[0.0..20.0, 30.0..80.0], &[marker(45.0)]),
            [0, 0]
        );
    }

    #[test]
    fn preferred_openings_cannot_split_an_overlapping_line_group() {
        let protected = BoxRules {
            extent: 0.0..20.0,
            rules: Rules {
                after: Edge::Avoid,
                ..Rules::default()
            },
            prefer_opening: false,
            extra_splits: 0,
            physical_cuts: None,
        };
        let opening = |extent| BoxRules {
            extent,
            rules: Rules {
                before: Edge::Force,
                ..Rules::default()
            },
            prefer_opening: true,
            extra_splits: 0,
            physical_cuts: None,
        };
        let lines = [0.0..20.0, 30.0..80.0, 90.0..110.0];
        assert!(boundaries(&lines, &[opening(38.0..80.0), protected])[1] == Edge::Avoid);
        let keep = BoxRules {
            extent: 0.0..80.0,
            rules: Rules {
                keep_together: true,
                ..Rules::default()
            },
            prefer_opening: false,
            extra_splits: 0,
            physical_cuts: None,
        };
        assert!(boundaries(&lines, &[opening(30.0..56.0), keep])[1] == Edge::Avoid);
    }

    #[test]
    fn suppressed_rules_cannot_force_or_protect_a_page_but_visibility_keeps_geometry() {
        let book = Book::open(&crate::test_fixture("field-notes-epub3.epub")).unwrap();
        let probe = Probe::from_book_content(book, Engine::Parley,
            &[crate::test_fixture("fonts/NotoSans-Regular.ttf")], Settings::default(), 0,
            Some("<html><body><p>Before</p><p id='target'>Middle paragraph text</p><p>After</p></body></html>".into())).unwrap();
        let source = &probe.source;
        let target = source
            .elements
            .iter()
            .position(|element| element.id.as_deref() == Some("target"))
            .unwrap();
        let mut styles = vec![Style::default(); source.elements.len()];
        styles[target].page_breaks = Rules {
            before: Edge::Force,
            after: Edge::Avoid,
            keep_together: true,
        };
        let visible = geometry(source, &styles, &probe.lines);
        assert_eq!(visible.len(), 1);
        styles[target].flow.painted = false;
        let unpainted = geometry(source, &styles, &probe.lines);
        assert_eq!(unpainted[0].extent, visible[0].extent);
        assert!(unpainted[0].rules == visible[0].rules);
        styles[target].flow.suppressed = true;
        assert!(geometry(source, &styles, &probe.lines).is_empty());
        // A removed block must also not claim the following block's boundary
        // through the empty-separator fallback when it has no formatted line.
        let empty: Vec<Line> = probe
            .lines
            .into_iter()
            .filter(|line| {
                source
                    .blocks
                    .iter()
                    .find(|block| block.key == line.block)
                    .is_none_or(|block| block.element != target)
            })
            .collect();
        assert!(!empty.is_empty());
        assert!(geometry(source, &styles, &empty).is_empty());
    }

    #[test]
    fn block_and_empty_separator_begin_new_pages_and_preserve_source() {
        let settings = Settings {
            width: 840,
            height: 1000,
            font_size: 32.,
            margin: 32,
            ..Default::default()
        };
        let path = crate::test_fixture("field-notes-epub3.epub");
        let fonts = [crate::test_fixture("fonts/NotoSans-Regular.ttf")];
        let build = |settings| {
            Probe::from_book_content(
                Book::open(&path).unwrap(),
                Engine::Parley,
                &fonts,
                settings,
                0,
                Some(include_str!("../tests/fixtures/page-breaks.xhtml").into()),
            )
            .unwrap()
        };
        let mut on = build(settings);
        let off = build(Settings {
            publisher_styles: false,
            ..settings
        });
        assert_eq!(on.page_count(), 3);
        assert_eq!(off.page_count(), 1);
        for (id, page) in [("first", 0), ("second", 1), ("ordinary", 1), ("last", 2)] {
            let anchor = on.fragment(id).unwrap();
            assert_eq!(on.page_for(&anchor), Some(page), "{id}");
            assert_eq!(off.page_for(&anchor), Some(0));
            assert_eq!(on.page_for(&on.page_anchor(page).unwrap()), Some(page));
        }
        let next = on.capture(1).unwrap();
        assert_ne!(on.capture(0).unwrap(), next);
        assert_ne!(on.capture(2).unwrap(), next);
        if let Some(p) = std::env::var_os("READER_PUBLISHER_BREAK_PREVIEW") {
            image::save_buffer(
                p,
                &next,
                settings.width,
                settings.height,
                image::ColorType::L8,
            )
            .unwrap();
        }
    }

    #[test]
    fn keep_groups_move_together_and_oversized_groups_allow_safe_line_breaks() {
        let lines: Vec<Line> = (0..11)
            .map(|i| Line {
                block: i,
                bytes: 0..1,
                spans: None,
                additional_sources: None,
                raw_whitespace: None,
                top: i as f32 * 20.,
                bottom: i as f32 * 20. + 18.,
                page_extension: [0.0; 2],
                missing_glyphs: 0,
            })
            .collect();
        let keep = |start, end| BoxRules {
            extent: start..end,
            rules: Rules {
                keep_together: true,
                ..Rules::default()
            },
            prefer_opening: false,
            extra_splits: 0,
            physical_cuts: None,
        };
        let pages = Pagination::with_breaks(&lines, 100., 218., &[keep(60., 138.)]).unwrap();
        assert_eq!(pages.bounds, [0., 60., 160., 218.]);
        let pages = Pagination::with_breaks(&lines, 100., 218., &[keep(0., 218.)]).unwrap();
        assert_eq!(pages.bounds, [0., 100., 200., 218.]);
        let pages =
            Pagination::with_breaks(&lines, 100., 218., &[keep(60., 158.), keep(80., 138.)])
                .unwrap();
        assert_eq!(pages.bounds, [0., 60., 160., 218.]);
        let explicit = BoxRules {
            extent: 100.0..118.0,
            rules: Rules {
                before: Edge::Force,
                ..Default::default()
            },
            prefer_opening: false,
            extra_splits: 0,
            physical_cuts: None,
        };
        let pages =
            Pagination::with_breaks(&lines, 100., 218., &[keep(60., 138.), explicit]).unwrap();
        assert_eq!(pages.bounds, [0., 60., 160., 218.]);
    }
}

#[cfg(all(test, feature = "kindle"))]
#[test]
#[ignore = "Provide an original EPUB and source XPointers on both sides of its page breaks"]
fn original_publisher_page_edges_keep_text_positions() {
    use crate::{Book, Engine, Probe, Settings};
    use std::{collections::BTreeMap, fs, path::PathBuf};
    let source = PathBuf::from(std::env::var_os("READER_BREAK_EDITION").unwrap());
    let mut book = Book::open(&source).unwrap();
    let list = fs::read_to_string(std::env::var_os("READER_BREAK_PAIRS").unwrap()).unwrap();
    let mut groups = BTreeMap::new();
    for (i, line) in list.lines().enumerate() {
        let fields: Vec<_> = line.split('\t').collect();
        assert_eq!(fields.len(), 3);
        let a = crate::cre_pointer::resolve(&mut book, fields[1], Some(20240114)).unwrap();
        let b = crate::cre_pointer::resolve(&mut book, fields[2], Some(20240114)).unwrap();
        assert_eq!(a.chapter, b.chapter);
        groups
            .entry(a.chapter)
            .or_insert_with(Vec::new)
            .push((i + 1, a.anchor, b.anchor));
    }
    let fonts = [crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    for (chapter, pairs) in groups {
        for (mode, publisher_styles) in [("source", true), ("auto", false)] {
            let mut probe = Probe::open(
                &source,
                Engine::Parley,
                &fonts,
                Settings {
                    width: 1264,
                    height: 1606,
                    margin: 54,
                    font_size: 42.,
                    publisher_styles,
                    ..Default::default()
                },
                chapter,
            )
            .unwrap();
            for (i, prev, next) in &pairs {
                let a = probe.page_for(prev).unwrap();
                let b = probe.page_for(next).unwrap();
                assert!(a <= b);
                assert_eq!(probe.page_for(&probe.page_anchor(b).unwrap()), Some(b));
                let pixels = probe.capture(b).unwrap();
                assert!(pixels.iter().any(|p| *p < 240));
                eprintln!("break {i} {mode} {a} {b}");
            }
        }
    }
}

#[cfg(test)]
#[path = "publisher_breaks/line_ends_tests.rs"]
mod line_ends_tests;

#[cfg(test)]
mod forced_margin_tests;
