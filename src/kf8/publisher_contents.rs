//! Publisher NCX for the reconstructed KF8 edition. Tag 6 contains a fragment
//! record number and an offset within that fragment's original UTF-8 bytes.
use super::{Location, Records, Result, chapter_path, link_node, navigation_index, repeated_tag};
use crate::{
    epub::{Destination, TocEntry},
    epub_xml::SourceOffsets,
};
use roxmltree::Node;
use std::collections::BTreeMap;

#[derive(Default)]
pub(crate) struct Navigation {
    pub(crate) rows: Vec<TocEntry>,
    targets: Vec<(usize, usize)>,
}

// The separator cannot occur in an XML ID. These names live only in Reader's
// in-memory Contents data and never alter the publication or its source keys.
const ELEMENT_PREFIX: &str = "\u{1f}kf8-element:";
pub(crate) fn element_name(index: usize) -> String {
    format!("{ELEMENT_PREFIX}{index}")
}
pub(crate) fn element_index(name: &str) -> Option<usize> {
    name.strip_prefix(ELEMENT_PREFIX)?.parse().ok()
}
pub(crate) fn html_node(node: &Node<'_, '_>) -> bool {
    node.is_element() && matches!(node.tag_name().namespace(), None | Some(super::XHTML))
}

impl Navigation {
    pub(super) fn read(records: &Records<'_>, header: &[u8]) -> Result<Self> {
        let records = navigation_index::read_navigation(records, header, |label, depth, row| {
            let [fid, offset] = repeated_tag::<2>(row, 6)?;
            Ok((
                TocEntry {
                    label,
                    depth,
                    destination: None,
                },
                (fid, offset),
            ))
        })?;
        let (rows, targets) = records.into_iter().unzip();
        Ok(Self { rows, targets })
    }
    pub(super) fn resolve(
        &mut self,
        raw: &[String],
        nodes: &[Vec<Node<'_, '_>>],
        locations: &[Location],
        positions: &[SourceOffsets],
        ids: &[BTreeMap<&str, usize>],
        generated: &BTreeMap<(usize, usize), String>,
    ) {
        if self.rows.is_empty() {
            return;
        }
        let first_visible: Vec<_> = nodes
            .iter()
            .map(|chapter| {
                chapter
                    .iter()
                    .find(|n| super::html_element(**n, "body"))
                    .and_then(|body| {
                        body.descendants()
                            .find(|n| {
                                if n.is_text() {
                                    return n.text().is_some_and(|s| !s.trim().is_empty())
                                        && !n.ancestors().any(|a| {
                                            matches!(a.tag_name().name(), "script" | "style")
                                        });
                                }
                                super::html_element(*n, "img")
                            })
                            .map(|n| n.range().start)
                    })
            })
            .collect();
        for (row, &(fid, offset)) in self.rows.iter_mut().zip(&self.targets) {
            let Some(location) = locations.get(fid) else {
                continue;
            };
            let chapter = location.chapter;
            let path = chapter_path(chapter);
            // A zero offset is relative to this fragment, not to the file.
            // Only the first fragment of a section may use the section start.
            if offset == 0
                && (fid == 0 || locations[fid - 1].chapter != chapter)
                && first_visible[chapter]
                    .is_none_or(|start| positions[chapter].resolve(location.insertion) <= start)
            {
                row.destination = Some(Destination {
                    path,
                    fragment: None,
                });
                continue;
            }
            let Ok(node) = link_node(
                &raw[chapter],
                &nodes[chapter],
                location,
                offset,
                &positions[chapter],
            ) else {
                continue;
            };
            let start = node.range().start;
            let fragment = if let Some(name) = node
                .attribute("id")
                .filter(|id| ids[chapter].get(id) == Some(&1))
            {
                name.to_owned()
            } else if let Some(name) = generated.get(&(chapter, start)) {
                name.clone()
            } else if let Ok(index) =
                nodes[chapter].binary_search_by_key(&start, |n| n.range().start)
            {
                element_name(index)
            } else {
                continue;
            };
            row.destination = Some(Destination {
                path,
                fragment: Some(fragment),
            });
        }
    }

    /// Record the requested elements in the historically normalized chapter.
    /// Normalizing link and image attributes preserves the element sequence.
    pub(crate) fn requested_offsets(
        &self,
        chapter: &super::Kf8Chapter,
    ) -> Result<BTreeMap<usize, Vec<usize>>> {
        let mut requested: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for (row, entry) in self.rows.iter().enumerate() {
            if let Some(Destination {
                path,
                fragment: Some(id),
            }) = &entry.destination
                && *path == chapter.path
                && let Some(index) = element_index(id)
            {
                requested.entry(index).or_default().push(row);
            }
        }
        if requested.is_empty() {
            return Ok(requested);
        }
        let doc = roxmltree::Document::parse_with_options(
            &chapter.xhtml,
            roxmltree::ParsingOptions {
                nodes_limit: super::MAX_XML_NODES,
                ..Default::default()
            },
        )
        .map_err(|_| super::MobiError::InvalidText)?;
        let offsets = doc
            .descendants()
            .filter(html_node)
            .enumerate()
            .filter_map(|(i, node)| requested.remove(&i).map(|rows| (node.range().start, rows)))
            .collect();
        Ok(offsets)
    }

    /// Map original elements into their layout section without touching markup.
    /// Reopened ancestor tags are excluded by the original byte interval.
    pub(crate) fn map_section(
        &mut self,
        part: &super::Kf8Chapter,
        range: std::ops::Range<usize>,
        prefix: usize,
        positions: &BTreeMap<usize, Vec<usize>>,
    ) -> Result<()> {
        let mut local: BTreeMap<usize, &Vec<usize>> = positions
            .range(range.clone())
            .map(|(start, rows)| (prefix + start - range.start, rows))
            .collect();
        if local.is_empty() {
            return Ok(());
        }
        let doc = roxmltree::Document::parse_with_options(
            &part.xhtml,
            roxmltree::ParsingOptions {
                nodes_limit: super::MAX_XML_NODES,
                ..Default::default()
            },
        )
        .map_err(|_| super::MobiError::InvalidText)?;
        for (ordinal, node) in doc.descendants().filter(html_node).enumerate() {
            if let Some(rows) = local.remove(&node.range().start) {
                for &i in rows {
                    self.rows[i].destination = Some(Destination {
                        path: part.path.clone(),
                        fragment: Some(element_name(ordinal)),
                    });
                }
            }
        }
        for rows in local.values() {
            for &i in *rows {
                self.rows[i].destination = None;
            }
        }
        Ok(())
    }
    pub(crate) fn finish(mut self, parts: &BTreeMap<String, String>) -> Vec<TocEntry> {
        let mut by_chapter = BTreeMap::<String, BTreeMap<String, Option<String>>>::new();
        for row in &self.rows {
            if let Some(Destination {
                path,
                fragment: Some(fragment),
            }) = &row.destination
            {
                if element_index(fragment).is_none() {
                    by_chapter
                        .entry(path.clone())
                        .or_default()
                        .insert(fragment.clone(), None);
                }
            }
        }
        for (path, xml) in parts {
            let original = path
                .split_once("-part-")
                .map(|(base, _)| format!("{base}.xhtml"))
                .unwrap_or_else(|| path.clone());
            let Some(names) = by_chapter.get_mut(&original) else {
                continue;
            };
            let Ok(doc) = roxmltree::Document::parse(xml) else {
                continue;
            };
            for name in doc.descendants().filter_map(|n| n.attribute("id")) {
                if let Some(target) = names.get_mut(name) {
                    *target = Some(path.clone());
                }
            }
        }
        for row in &mut self.rows {
            if let Some(Destination {
                path,
                fragment: Some(fragment),
            }) = &mut row.destination
            {
                if element_index(fragment).is_some() {
                    continue;
                }
                if let Some(Some(updated)) = by_chapter.get(path).and_then(|m| m.get(fragment)) {
                    *path = updated.clone();
                } else {
                    row.destination = None;
                }
            }
        }
        self.rows
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{epub::Book, source::Source};
    use std::{env, fs, path::PathBuf};
    #[test]
    fn publisher_only_positions_survive_partition_retarget_and_mixed_content_without_changing_xhtml()
     {
        use crate::kf8::Kf8Chapter;
        let chapter = || Kf8Chapter {
            continued: false,
            path: chapter_path(0),
            label: "Field guide".into(),
            xhtml: format!(
                "<html><head/><body><section>Intro <b>notes</b>{}<p id='same'>First</p><p id='same'>Second target</p>Closing</section></body></html>",
                "<p>Field marks along the path.</p>".repeat(4100)
            ),
        };
        let previous = crate::kindle_sections::partition(vec![chapter()]).unwrap();
        let original = chapter();
        let start = original.xhtml.find("<p id='same'>Second").unwrap();
        let doc = roxmltree::Document::parse(&original.xhtml).unwrap();
        let ordinal = doc
            .descendants()
            .filter(html_node)
            .position(|n| n.range().start == start)
            .unwrap();
        let mut nav = Navigation {
            rows: vec![TocEntry {
                depth: 0,
                label: "Second target".into(),
                destination: Some(Destination {
                    path: chapter_path(0),
                    fragment: Some(element_name(ordinal)),
                }),
            }],
            targets: Vec::new(),
        };
        let current =
            crate::kindle_sections::partition_with_contents(vec![chapter()], &mut nav).unwrap();
        assert_eq!(current.len(), previous.len());
        assert!(current.len() > 1);
        for (a, b) in current.iter().zip(previous) {
            assert_eq!(a.path, b.path);
            assert_eq!(a.xhtml, b.xhtml);
        }
        let dst = nav.rows[0].destination.as_ref().unwrap();
        let chapter = current.iter().find(|c| c.path == dst.path).unwrap();
        assert!(chapter.continued);
        let id = dst.fragment.as_ref().unwrap();
        let pos = element_index(id).unwrap();
        let source =
            Source::parse_with_kindle_elements(&chapter.xhtml, None, &[pos].into_iter().collect())
                .unwrap();
        let (key, offset) = source.fragments[id];
        let block = source.blocks.iter().find(|b| b.key == key).unwrap();
        assert_eq!(&block.text[offset..], "Second target");
        assert_eq!(
            source.revision,
            Source::parse(&chapter.xhtml).unwrap().revision
        );
    }

    #[test]
    fn a_zero_offset_in_a_later_fragment_keeps_the_actual_heading() {
        let raw = vec!["<html><body><p id='opening'>Opens here.</p><h2 id='later'>Later passage</h2></body></html>".to_string()];
        let doc = roxmltree::Document::parse(&raw[0]).unwrap();
        let nodes = vec![doc.descendants().filter(html_node).collect::<Vec<_>>()];
        let position = |needle: &str| raw[0].find(needle).unwrap();
        let locations = [
            Location {
                chapter: 0,
                insertion: position("<p"),
                length: 24,
            },
            Location {
                chapter: 0,
                insertion: position("<h2"),
                length: 34,
            },
        ];
        let mut ncx = Navigation {
            rows: ["Opening", "Later"]
                .into_iter()
                .map(|label| TocEntry {
                    label: label.into(),
                    depth: 0,
                    destination: None,
                })
                .collect(),
            targets: vec![(0, 0), (1, 0)],
        };
        let ids = vec![[("opening", 1), ("later", 1)].into_iter().collect()];
        ncx.resolve(
            &raw,
            &nodes,
            &locations,
            &[SourceOffsets::default()],
            &ids,
            &BTreeMap::new(),
        );
        assert!(ncx.rows[0].destination.as_ref().unwrap().fragment.is_none());
        assert_eq!(
            ncx.rows[1]
                .destination
                .as_ref()
                .unwrap()
                .fragment
                .as_deref(),
            Some("later")
        );
        // A skeleton can supply visible material before even its first fragment.
        ncx.targets = vec![(0, 0), (0, 0)];
        ncx.resolve(
            &raw,
            &nodes,
            &locations[1..],
            &[SourceOffsets::default()],
            &ids,
            &BTreeMap::new(),
        );
        assert_eq!(
            ncx.rows[0]
                .destination
                .as_ref()
                .unwrap()
                .fragment
                .as_deref(),
            Some("later")
        );
    }

    #[test]
    fn publisher_tree_reorders_breadth_first_rows_and_rejects_inconsistent_relationships() {
        use crate::kf8::navigation_index::publisher_order;
        let tree = [
            (0, None, Some(2), Some(3)),
            (0, None, None, None),
            (1, Some(0), None, None),
            (1, Some(0), None, None),
        ];
        assert_eq!(
            publisher_order(vec![0, 1, 2, 3], &tree).unwrap(),
            [0, 2, 3, 1]
        );
        let mut incorrect = tree;
        incorrect[2].1 = Some(2);
        assert!(publisher_order(vec![0, 1, 2, 3], &incorrect).is_err());
        incorrect = tree;
        incorrect[3].1 = Some(1);
        assert!(publisher_order(vec![0, 1, 2, 3], &incorrect).is_err());
    }

    #[test]
    #[ignore = "provide a local unencrypted KF8 and an independently extracted publisher NCX"]
    fn local_kf8_publisher_contents_matches_independent_titles_depth_and_text() {
        let path = PathBuf::from(env::var_os("READER_TEST_KF8_NCX").unwrap());
        let refs: Vec<(usize, String, usize, String, String)> = serde_json::from_slice(
            &fs::read(env::var_os("READER_TEST_KF8_REFERENCE").unwrap()).unwrap(),
        )
        .unwrap();
        let mut book = Book::open(&path).unwrap();

        if refs.is_empty() {
            assert!(!book.toc.is_empty());
            return;
        }
        assert_eq!(book.toc.len(), refs.len());
        let mut assigned = BTreeMap::<String, Vec<(usize, Option<String>)>>::new();
        let mut missing = Vec::new();
        for (i, (row, (depth, label, chapter, _, _))) in book.toc.iter().zip(&refs).enumerate() {
            assert_eq!(row.depth, *depth, "depth row {i}");
            assert!(
                row.label
                    == if label.is_empty() {
                        "Untitled section"
                    } else {
                        label
                    },
                "label row {i} ({} and {} characters)",
                row.label.chars().count(),
                label.chars().count()
            );
            let Some(dst) = &row.destination else {
                missing.push(i);
                continue;
            };
            let base = format!("chapter-{chapter:04}");
            assert!(
                dst.path == format!("{base}.xhtml")
                    || dst.path.starts_with(&format!("{base}-part-")),
                "file row {i}"
            );
            assigned
                .entry(dst.path.clone())
                .or_default()
                .push((i, dst.fragment.clone()));
        }
        let mut same = 0;
        let mut anchored = 0;
        let mut differences = Vec::new();
        for (path, items) in assigned {
            let xhtml = book.chapter(book.chapter_index(&path).unwrap()).unwrap();
            let doc = roxmltree::Document::parse(&xhtml).unwrap();
            let body = doc.descendants().find(|n| n.has_tag_name("body")).unwrap();
            let (source, tail) = {
                let source = book.chapter_source(&path, &xhtml).unwrap();
                assert_eq!(source.revision, Source::parse(&xhtml).unwrap().revision);
                let base = path
                    .split("-part-")
                    .next()
                    .unwrap()
                    .trim_end_matches(".xhtml");
                let ci = book.chapter_index(&path).unwrap();
                let mut tail = String::new();
                for j in ci + 1..book.chapters.len() {
                    if !book.chapters[j].href.starts_with(&format!("{base}-part-"))
                        || tail.chars().count() >= 24
                    {
                        break;
                    }
                    let part = book.chapter(j).unwrap();
                    let document = roxmltree::Document::parse(&part).unwrap();
                    let body = document
                        .descendants()
                        .find(|n| n.has_tag_name("body"))
                        .unwrap();
                    tail.extend(
                        body.descendants()
                            .filter(Node::is_text)
                            .filter_map(|n| n.text())
                            .flat_map(str::chars)
                            .filter(|c| !c.is_whitespace())
                            .take(24),
                    );
                }
                (source, tail)
            };
            for (i, id) in items {
                let start = if let Some(id) = id {
                    if source.fragments.contains_key(&id) {
                        anchored += 1;
                    } else {
                        eprintln!("unmapped source row {i}");
                    }
                    if let Some(ordinal) = element_index(&id) {
                        doc.descendants().filter(html_node).nth(ordinal).unwrap()
                    } else {
                        body.descendants()
                            .find(|n| n.attribute("id") == Some(id.as_str()))
                            .unwrap()
                    }
                } else {
                    anchored += 1;
                    body
                };
                let s: String = body
                    .descendants()
                    .skip_while(|n| *n != start)
                    .filter(|n| {
                        n.is_text()
                            && !n.ancestors().any(|a| {
                                matches!(a.tag_name().name(), "style" | "script" | "iframe")
                            })
                    })
                    .filter_map(|n| n.text())
                    .flat_map(str::chars)
                    .filter(|c| !c.is_whitespace())
                    .chain(tail.chars())
                    .take(24)
                    .collect();
                if s == refs[i].4 {
                    same += 1;
                } else {
                    eprintln!(
                        "different row {i} expected_id={} raw_len={} expected_len={} common_chars={} tag={}",
                        start.attribute("id") == Some(refs[i].3.as_str()),
                        s.chars().count(),
                        refs[i].4.chars().count(),
                        s.chars()
                            .zip(refs[i].4.chars())
                            .take_while(|(a, b)| a == b)
                            .count(),
                        start.tag_name().name()
                    );
                    differences.push(i);
                }
            }
        }
        let mut rendered = 0;
        if let Some(font) = env::var_os("READER_TEST_KF8_FONT") {
            let requested = env::var("READER_TEST_KF8_PAGES").unwrap_or_else(|_| "0".into());
            for row in requested.split(',').map(|n| n.parse::<usize>().unwrap()) {
                let destination = book.toc[row].destination.as_ref().unwrap();
                let chapter = book.chapter_index(&destination.path).unwrap();
                let probe = crate::Probe::open(
                    &path,
                    crate::Engine::Parley,
                    &[PathBuf::from(&font)],
                    crate::Settings::default(),
                    chapter,
                )
                .unwrap();
                let page = if let Some(name) = &destination.fragment {
                    let target = probe.fragment(name).unwrap();
                    probe.page_for(&target).unwrap()
                } else {
                    0
                };
                assert!(page < probe.page_count());
                rendered += 1;
            }
        }
        eprintln!(
            "NCX total={} exact_text={same} selectable={anchored} real_page_checks={rendered} unresolved={missing:?} different={differences:?}",
            refs.len()
        );
        assert_eq!(same, refs.len());
        assert_eq!(anchored, refs.len());
    }
}
