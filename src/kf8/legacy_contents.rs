//! The legacy MOBI navigation index stores publisher labels in CNCX records
//! and addresses destinations by byte offset in the original uncompressed text.
use super::{MobiError, Records, Result, integer, read_index, varint};
use crate::{
    epub::{Destination, TocEntry},
    mobi::{self, Addition},
};
use std::collections::{BTreeMap, BTreeSet};

const MAX_ENTRIES: usize = 4096;
const MAX_LABEL_BYTES: usize = 2048;
const MAX_DEPTH: usize = 32;

#[derive(Debug)]
pub(crate) struct Item {
    label: String,
    depth: usize,
    pub(crate) position: usize,
    pub(crate) fragment: Option<String>,
}

#[derive(Debug, Default)]
pub(crate) struct Contents {
    pub(crate) items: Vec<Item>,
    pub(crate) insertions: Vec<Addition>,
}

impl Contents {
    pub(crate) fn read(bytes: &[u8], offsets: &[usize], header: &[u8]) -> Result<Self> {
        if header.get(16..20) != Some(b"MOBI") || integer(header, 20)? < 232 {
            return Ok(Self::default());
        }
        let index = integer(header, 244)?;
        if index == 0 || index == u32::MAX as usize {
            return Ok(Self::default());
        }
        if index <= usize::from(mobi::u16_at(header, 8)?) {
            return Err(MobiError::InvalidHeader);
        }
        let records = Records {
            bytes,
            offsets: offsets.to_vec(),
        };
        let meta = records.at(index)?;
        let segments = integer(meta, 24)?;
        let labels = integer(meta, 52)?;
        if !(1..=64).contains(&labels) || !(1..=super::MAX_INDEX_RECORDS).contains(&segments) {
            return Err(MobiError::InvalidHeader);
        }
        let label_base = index
            .checked_add(segments + 1)
            .ok_or(MobiError::InvalidHeader)?;
        records.at(label_base
            .checked_add(labels - 1)
            .ok_or(MobiError::InvalidHeader)?)?;
        let encoding = integer(meta, 28)?;
        let mut items = Vec::new();
        for row in read_index(&records, index, MAX_ENTRIES)? {
            let field = |n| {
                row.tags
                    .get(&n)
                    .and_then(|x| match x.as_slice() {
                        [v] => Some(*v),
                        _ => None,
                    })
                    .ok_or(MobiError::InvalidHeader)
            };
            let position = field(1)?;
            let pointer = field(3)?;
            let depth = field(4)?;
            if depth > MAX_DEPTH
                || items
                    .last()
                    .map_or(depth != 0, |last: &Item| depth > last.depth + 1)
            {
                return Err(MobiError::InvalidHeader);
            }
            let segment = pointer >> 16;
            if segment >= labels {
                return Err(MobiError::InvalidHeader);
            }
            let record = records.at(label_base + segment)?;
            let mut text = record
                .get(pointer & 0xffff..)
                .ok_or(MobiError::InvalidHeader)?;
            let size = varint(&mut text)?;
            if !(1..=MAX_LABEL_BYTES).contains(&size) {
                return Err(MobiError::InvalidHeader);
            }
            let label = mobi::navigation_label(
                text.get(..size).ok_or(MobiError::InvalidHeader)?,
                encoding,
            )?;
            if label.trim().is_empty() {
                return Err(MobiError::InvalidHeader);
            }
            items.push(Item {
                label,
                depth,
                position,
                fragment: None,
            });
        }
        Ok(Self {
            items,
            insertions: Vec::new(),
        })
    }

    /// Chapters may have been divided or link targets retargeted. Resolve by
    /// the exact surviving XML fragment after partitioning, including aliases
    /// supplied by the book itself.
    pub(crate) fn entries(self, sections: &BTreeMap<String, String>) -> Vec<TocEntry> {
        let wanted: BTreeSet<_> = self
            .items
            .iter()
            .filter_map(|item| item.fragment.as_deref())
            .collect();
        let mut paths = BTreeMap::new();
        for (path, xml) in sections {
            let Ok(doc) = roxmltree::Document::parse_with_options(
                xml,
                roxmltree::ParsingOptions {
                    nodes_limit: 240_000,
                    ..Default::default()
                },
            ) else {
                continue;
            };
            for id in doc
                .descendants()
                .filter_map(|node| node.attribute("id"))
                .filter(|id| wanted.contains(id))
            {
                paths
                    .entry(id.to_owned())
                    .and_modify(|old| *old = None)
                    .or_insert(Some(path));
            }
        }
        self.items
            .into_iter()
            .map(|item| TocEntry {
                label: item.label,
                depth: item.depth,
                destination: item.fragment.and_then(|fragment| {
                    paths
                        .get(fragment.as_str())
                        .copied()
                        .flatten()
                        .map(|path| Destination {
                            path: path.clone(),
                            fragment: Some(fragment),
                        })
                }),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use crate::{Engine, Probe, Settings, epub::Book};
    use std::{env, fs, path::PathBuf};

    const GUIDE: &[u8] = include_bytes!("../../tests/fixtures/mobi/coastal-contents.mobi");

    #[test]
    fn authored_nested_contents_resolves_index_only_destinations_and_preserves_styled_history() {
        use crate::{
            kf8::{self, Edition},
            kindle_sections,
            source::Source,
        };
        let Edition::Legacy(doc) = kf8::open_edition(GUIDE).unwrap() else {
            panic!("expected a legacy book")
        };
        assert_eq!(doc.contents.items.len(), 4);
        assert_eq!(
            doc.contents
                .items
                .iter()
                .map(|i| i.depth)
                .collect::<Vec<_>>(),
            [0, 1, 1, 0]
        );
        assert_eq!(
            doc.contents.insertions.len(),
            3,
            "three targets are absent from ordinary hyperlinks"
        );
        let ch = kf8::Kf8Chapter {
            path: "document.xhtml".into(),
            label: "Guide".into(),
            continued: false,
            xhtml: doc.xhtml,
        };
        let (parts, prior) = kindle_sections::legacy(
            ch,
            doc.additions,
            doc.presentation,
            &doc.contents.insertions,
            false,
        )
        .unwrap();
        let stages = &prior["document.xhtml"];
        assert_eq!(
            stages.len(),
            3,
            "bare, illustrated, and styled prior editions"
        );
        let styled = stages.last().unwrap();
        assert!(styled.contains("text-align:center") && styled.contains("mobi-filepos-"));
        let mut source = Source::parse(&parts[0].xhtml).unwrap();
        for old in stages {
            source.accept_previous_mobi(old).unwrap();
            let previous = Source::parse(old).unwrap();
            for block in previous
                .blocks
                .iter()
                .filter(|b| !b.text.is_empty() || !b.images.is_empty())
            {
                let key = source.current_key(block.key, previous.revision).unwrap();
                let now = source.blocks.iter().find(|b| b.key == key).unwrap();
                assert_eq!(block.text, now.text);
            }
        }
        let sections = parts
            .into_iter()
            .map(|part| (part.path, part.xhtml))
            .collect();
        let entries = doc.contents.entries(&sections);
        for (item, title) in entries.iter().zip([
            "Coastal field notes",
            "Elevation profile",
            "Survey notes",
            "Summit notes",
        ]) {
            assert_eq!(item.label, title);
            assert!(
                source.fragments.contains_key(
                    item.destination
                        .as_ref()
                        .unwrap()
                        .fragment
                        .as_ref()
                        .unwrap()
                )
            );
        }
    }

    #[test]
    fn missing_source_position_disables_only_that_entry_and_a_bad_index_leaves_the_book_readable() {
        use crate::{
            kf8::{self, Edition},
            mobi,
        };
        let mut bytes = GUIDE.to_vec();
        let at = bytes.windows(7).position(|w| w == b"<p><img").unwrap();
        bytes[at..at + 3].fill(b' ');
        let Edition::Legacy(doc) = kf8::open_edition(&bytes).unwrap() else {
            panic!("expected a legacy book")
        };
        assert_eq!(doc.contents.items.len(), 4);
        assert!(doc.contents.items[1].fragment.is_none());
        assert!(doc.contents.items[0].fragment.is_some());
        assert!(doc.contents.items[2].fragment.is_some());
        assert!(doc.contents.items[3].fragment.is_some());
        let offsets = mobi::record_offsets(&bytes).unwrap();
        let index = mobi::u32_at(&bytes, offsets[0] + 244).unwrap() as usize;
        bytes[offsets[index]..offsets[index] + 4].copy_from_slice(b"JUNK");
        let Edition::Legacy(doc) = kf8::open_edition(&bytes).unwrap() else {
            panic!("expected a legacy book")
        };
        assert!(doc.contents.items.is_empty());
        assert!(doc.xhtml.contains("Survey 1."));
    }

    #[test]
    #[ignore = "supply an unencrypted legacy MOBI, independent NCX labels/targets and a reading font"]
    fn local_legacy_contents_matches_publisher_outline_and_all_source_destinations() {
        let path = PathBuf::from(env::var_os("READER_TEST_MOBI").unwrap());
        let gold = fs::read_to_string(env::var_os("READER_TEST_MOBI_NCX").unwrap()).unwrap();
        let font = PathBuf::from(env::var_os("READER_TEST_MOBI_FONT").unwrap());
        let mut book = Book::open(&path).unwrap();
        let refs: Vec<_> = gold
            .lines()
            .map(|s| s.splitn(5, '\t').collect::<Vec<_>>())
            .collect();
        assert_eq!(book.toc.len(), refs.len(), "entry count");
        let mut targets = std::collections::BTreeMap::<String, Vec<(usize, String)>>::new();
        let mut maximum_depth = 0;
        for (i, (r, entry)) in refs.iter().zip(&book.toc).enumerate() {
            assert_eq!(r.len(), 5);
            assert_eq!(entry.depth, r[0].parse::<usize>().unwrap(), "depth {i}");
            maximum_depth = maximum_depth.max(entry.depth);
            assert!(entry.label == r[2], "label {i}");
            let destination = entry
                .destination
                .as_ref()
                .expect("independent entry with a destination");
            let id = destination.fragment.as_ref().unwrap();
            assert!(id == r[3], "source id {i}");
            targets
                .entry(destination.path.clone())
                .or_default()
                .push((i, id.clone()));
        }
        let mut resolved = 0;
        let mut anchored = 0;
        for (href, group) in targets {
            let ci = book.chapter_index(&href).unwrap();
            let xml = book.chapter(ci).unwrap();
            let doc = roxmltree::Document::parse(&xml).unwrap();
            let body = doc.descendants().find(|n| n.has_tag_name("body")).unwrap();
            let probe = Probe::open(
                &path,
                Engine::Parley,
                std::slice::from_ref(&font),
                Settings::default(),
                ci,
            )
            .unwrap();
            for (i, id) in group {
                let node = body
                    .descendants()
                    .find(|n| n.attribute("id") == Some(id.as_str()))
                    .expect("source destination");
                let text: String = body
                    .descendants()
                    .skip_while(|n| *n != node)
                    .filter_map(|n| n.is_text().then(|| n.text()).flatten())
                    .flat_map(str::chars)
                    .filter(|c| !c.is_whitespace())
                    .take(24)
                    .collect();
                assert!(text == refs[i][4], "source text range {i}");
                resolved += 1;
                let at = probe.fragment(&id).expect("source anchor exists");
                assert!(probe.page_for(&at).is_some(), "page {i}");
                anchored += 1;
            }
        }
        assert_eq!(resolved, refs.len());
        assert_eq!(anchored, refs.len());
        println!(
            "verified publisher NCX entries={} actual_text_ranges={} navigable_pages={} deepest_level={}",
            refs.len(),
            resolved,
            anchored,
            maximum_depth
        );
    }
}
