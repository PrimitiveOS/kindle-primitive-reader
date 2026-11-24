//! Publisher page breaks in horizontal reflow, expressed in document coordinates.
use super::{Line, Settings, book_style::Style, source::Source};
use std::ops::Range;

mod empty;

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

pub(crate) struct BoxRules {
    extent: Range<f32>,
    rules: Rules,
    extra_splits: u16,
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
            let extent = extent.get_or_insert(line.top..line.bottom);
            extent.start = extent.start.min(line.top);
            extent.end = extent.end.max(line.bottom);
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
                extra_splits: 0,
            });
        }
    }
}

/// Find the first and last painted lines of each styled block, including wrappers
/// around other blocks. An empty separator shares the next visible block's edge.
#[cfg(test)]
pub(crate) fn geometry(source: &Source, styles: &[Style], lines: &[Line]) -> Vec<BoxRules> {
    layout(source, styles, lines, Settings::default())
}

pub(crate) fn layout(
    source: &Source,
    styles: &[Style],
    lines: &[Line],
    settings: Settings,
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
            extend(&mut boxes, source.blocks[i].element, line.top, line.bottom);
        }
    }
    let direct = settings.empty_break_mode.expands().then(|| boxes.clone());
    for (i, element) in source.elements.iter().enumerate().rev() {
        if let (Some(p), Some(extent)) = (element.parent, boxes[i].clone()) {
            extend(&mut boxes, p, extent.start, extent.end);
        }
    }
    let mut following = None;
    for (i, element) in source.elements.iter().enumerate().rev() {
        if element.selector_only
            || styles[i].flow.suppressed
            || !(crate::source::is_block_tag(&element.tag)
                || matches!(element.tag.as_str(), "ol" | "ul" | "table" | "tr" | "hr"))
        {
            continue;
        }
        let rules = styles[i].page_breaks;
        let extent = boxes[i].clone();
        if rules != Rules::default() {
            if let Some(extent) = extent.clone().or_else(|| following.map(|y| y..y)) {
                result.push(BoxRules {
                    extent,
                    rules,
                    extra_splits: 0,
                });
            }
        }
        if let Some(extent) = extent {
            following = Some(extent.start);
        }
    }
    if let Some(direct) = &direct {
        empty::propagate(source, styles, direct, &boxes, settings, &mut result);
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
    edges
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Book, Engine, Pagination, Probe, Settings};

    #[test]
    fn structural_splits_do_not_move_to_the_start_of_an_overlapping_line_group() {
        let marker = |at| BoxRules {
            extent: at..at,
            rules: Rules::default(),
            extra_splits: 2,
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
                top: i as f32 * 20.,
                bottom: i as f32 * 20. + 18.,
                missing_glyphs: 0,
            })
            .collect();
        let keep = |start, end| BoxRules {
            extent: start..end,
            rules: Rules {
                keep_together: true,
                ..Rules::default()
            },
            extra_splits: 0,
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
            extra_splits: 0,
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
