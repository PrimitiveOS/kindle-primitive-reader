//! Local NAVM destinations in bundled DjVu books.
use std::collections::HashMap;
use std::sync::Arc;

use crate::fixed::{MAX_OUTLINE_DEPTH, MAX_OUTLINE_ITEMS, MAX_TITLE_BYTES, PdfOutlineItem};
use djvu_rs::{Bookmark, Document};

#[derive(Default)]
pub(super) struct Contents {
    pub rows: Vec<PdfOutlineItem>,
    changing: Vec<(usize, Changing)>,
}

#[derive(Clone)]
enum Changing {
    Relative(isize),
    Titles(Arc<[usize]>),
}

impl Changing {
    fn page(&self, current: usize, count: usize) -> Option<usize> {
        match self {
            Self::Relative(offset) => current.checked_add_signed(*offset).filter(|p| *p < count),
            Self::Titles(pages) => {
                let next = pages.partition_point(|p| *p < current);
                Some(*pages.get(next).unwrap_or(&pages[0]))
            }
        }
    }
}

#[derive(Default)]
struct Matches {
    id: Option<usize>,
    title: Vec<usize>,
    name: Option<usize>,
}

#[derive(Clone)]
enum Target {
    Page(usize),
    Changing(Changing),
    Unavailable,
}

impl Contents {
    pub fn read(document: &Document) -> Self {
        let nodes = document.inner().bookmarks();
        if nodes.is_empty() {
            return Self::default();
        }
        let mut entries = Vec::new();
        flatten(nodes, 0, &mut entries);
        let mut matches = HashMap::<String, Matches>::new();
        // Decode each bounded fragment once, then collect only the DIRM names it uses.
        let fragments: Vec<_> = entries
            .iter()
            .map(|(item, _)| {
                let value = item.url.strip_prefix('#')?;
                if value.is_empty() || value.len() > MAX_TITLE_BYTES {
                    return None;
                }
                let decoded = percent_encoding::percent_decode_str(value)
                    .decode_utf8()
                    .ok()?;
                if decoded.chars().any(char::is_control) {
                    return None;
                }
                matches.entry(decoded.to_string()).or_default();
                Some(decoded)
            })
            .collect();
        if !matches.is_empty()
            && let Some(dirm) = document.inner().raw_chunk(b"DIRM")
        {
            // Drop partial aliases if this optional directory cannot identify all pages.
            if !directory_matches(dirm, document.page_count(), &mut matches) {
                matches
                    .values_mut()
                    .for_each(|item| *item = Matches::default());
            }
        }
        let count = document.page_count();
        let targets: HashMap<_, _> = matches
            .into_iter()
            .map(|(key, value)| {
                let target = resolve(&key, value, count);
                (key, target)
            })
            .collect();
        let mut outline = Self::default();
        for ((item, depth), fragment) in entries.into_iter().zip(fragments) {
            let text = &item.title[..item.title.floor_char_boundary(MAX_TITLE_BYTES)];
            let title: String = text
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .chars()
                .filter(|ch| !ch.is_control())
                .take(256)
                .collect();
            if title.is_empty() {
                continue;
            }
            let target = fragment.as_deref().and_then(|key| targets.get(key));
            let page = match target {
                Some(Target::Page(page)) => Some(*page),
                Some(Target::Changing(change)) => {
                    outline.changing.push((outline.rows.len(), change.clone()));
                    change.page(0, count)
                }
                _ => None,
            };
            outline.rows.push(PdfOutlineItem { title, page, depth });
        }
        outline
    }

    pub fn stable(&self, row: usize) -> bool {
        self.changing
            .binary_search_by_key(&row, |(index, _)| *index)
            .is_err()
    }

    pub fn set_page(&mut self, current: usize, count: usize) {
        for (row, change) in &self.changing {
            self.rows[*row].page = change.page(current, count);
        }
    }
}

fn flatten<'a>(nodes: &'a [Bookmark], depth: usize, out: &mut Vec<(&'a Bookmark, usize)>) {
    for node in nodes {
        if out.len() == MAX_OUTLINE_ITEMS {
            break;
        }
        out.push((node, depth));
        if depth + 1 < MAX_OUTLINE_DEPTH {
            flatten(&node.children, depth + 1, out);
        }
    }
}

fn resolve(key: &str, found: Matches, count: usize) -> Target {
    if let Some(page) = found.id {
        return Target::Page(page);
    }
    if key.starts_with(['+', '-']) && key[1..].bytes().all(|b| b.is_ascii_digit()) && key.len() > 1
    {
        return key.parse::<isize>().map_or(Target::Unavailable, |delta| {
            Target::Changing(Changing::Relative(delta))
        });
    }
    match &found.title[..] {
        [] => {}
        [page] => return Target::Page(*page),
        _ => return Target::Changing(Changing::Titles(found.title.into())),
    }
    let physical = key
        .bytes()
        .all(|b| b.is_ascii_digit())
        .then(|| key.parse::<usize>().ok())
        .flatten()
        .and_then(|n| n.checked_sub(1))
        .filter(|n| *n < count);
    physical
        .or(found.name)
        .map_or(Target::Unavailable, Target::Page)
}

// The pinned decoder's component_directory() returns only an ID and kind.
// DIRM's alternate page title and optional name are needed for outline links.
fn directory_matches(dirm: &[u8], count: usize, keys: &mut HashMap<String, Matches>) -> bool {
    let [version, hi, lo, rest @ ..] = dirm else {
        return false;
    };
    let n = usize::from(u16::from_be_bytes([*hi, *lo]));
    if n > super::MAX_COMPONENTS as usize || !matches!(version, 0x80 | 0x81) {
        return false;
    }
    let legacy = *version == 0x80;
    let Some(packed) = rest.get(n * if legacy { 7 } else { 4 }..) else {
        return false;
    };
    let Ok(directory) = djvu_rs::bzz::bzz_decode(packed) else {
        return false;
    };
    let Some(data) = directory.get(if legacy { 0 } else { 3 * n }..) else {
        return false;
    };
    let Some((flags, names)) = data.split_at_checked(n) else {
        return false;
    };
    let mut strings = names
        .split_inclusive(|b| *b == 0)
        .map(|s| s.strip_suffix(&[0]));
    let mut page = 0;
    for &flag in flags {
        let (is_page, has_name, has_title) = if legacy {
            (flag & 1 != 0, flag & 2 != 0, flag & 4 != 0)
        } else {
            (flag & 0x3f == 1, flag & 0x80 != 0, flag & 0x40 != 0)
        };
        let Some(id) = strings.next().flatten() else {
            return false;
        };
        let name = if has_name {
            strings.next().flatten()
        } else {
            Some(id)
        };
        let title = if has_title {
            strings.next().flatten()
        } else {
            Some(id)
        };
        let (Some(name), Some(title)) = (name, title) else {
            return false;
        };
        if is_page {
            if let Ok(key) = std::str::from_utf8(id)
                && let Some(found) = keys.get_mut(key)
            {
                found.id.get_or_insert(page);
            }
            if let Ok(key) = std::str::from_utf8(title)
                && let Some(found) = keys.get_mut(key)
            {
                found.title.push(page);
            }
            if let Ok(key) = std::str::from_utf8(name)
                && let Some(found) = keys.get_mut(key)
            {
                found.name.get_or_insert(page);
            }
            page += 1;
        }
    }
    page == count
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::djvu::Djvu;

    #[test]
    fn atlas_resolves_page_names_titles_numbers_and_current_page() {
        let mut book =
            Djvu::open(include_bytes!("../tests/fixtures/beacon-atlas.djvu").to_vec()).unwrap();
        assert_eq!(book.page_count(), 6);
        let entries = book
            .outline()
            .iter()
            .map(|r| (r.title.as_str(), r.depth, r.page))
            .collect::<Vec<_>>();
        assert_eq!(
            entries,
            [
                ("Front cover", 0, Some(0)),
                ("Northern shore", 0, Some(1)),
                ("Outer reef", 1, Some(2)),
                ("Water line", 1, Some(2)),
                ("Nearest station", 1, Some(1)),
                ("Last watch", 0, Some(5)),
                ("Next plate", 0, Some(1)),
                ("Source website", 0, None),
                ("Unknown plate", 0, None),
                ("Outside the book", 0, None),
            ]
        );
        book.set_outline_page(2);
        assert_eq!(book.outline()[4].page, Some(4));
        assert_eq!(book.outline()[6].page, Some(3));
        book.set_outline_page(5);
        assert_eq!(book.outline()[4].page, Some(1));
        assert_eq!(book.outline()[6].page, None);
    }

    #[test]
    fn page_ids_and_titles_precede_physical_numbers_and_component_names() {
        let target = |key, id, pages, name| match resolve(
            key,
            Matches {
                id,
                title: pages,
                name,
            },
            6,
        ) {
            Target::Page(page) => Some(page),
            Target::Changing(change) => change.page(0, 6),
            _ => None,
        };
        assert_eq!(target("2", Some(4), vec![3], Some(5)), Some(4));
        assert_eq!(target("2", None, vec![3], Some(5)), Some(3));
        assert_eq!(target("2", None, vec![], Some(5)), Some(1));
        assert_eq!(target("plate.djvu", None, vec![], Some(5)), Some(5));
        assert_eq!(target("900", None, vec![], None), None);
        assert_eq!(target("-1", None, vec![], None), None);
    }

    #[test]
    fn directory_names_and_titles_skip_shared_components_in_either_version() {
        for legacy in [false, true] {
            let flags = if legacy { [0, 7, 1] } else { [0, 0xc1, 1] };
            let mut data = if legacy { vec![] } else { vec![0; 9] };
            data.extend(flags);
            data.extend(b"shared.iff\0plate-01.djvu\0custom.djvu\0Plate I\0plate-02.djvu\0");
            let mut dirm = vec![if legacy { 0x80 } else { 0x81 }, 0, 3];
            dirm.resize(3 + if legacy { 7 * 3 } else { 4 * 3 }, 0);
            dirm.extend(djvu_rs::bzz_encode::bzz_encode(&data));
            let mut keys: HashMap<_, _> = ["shared.iff", "custom.djvu", "Plate I", "plate-02.djvu"]
                .into_iter()
                .map(|k| (k.to_owned(), Matches::default()))
                .collect();
            assert!(directory_matches(&dirm, 2, &mut keys));
            assert!(keys["shared.iff"].id.is_none());
            assert_eq!(keys["custom.djvu"].name, Some(0));
            assert_eq!(keys["Plate I"].title, [0]);
            assert_eq!(keys["plate-02.djvu"].id, Some(1));
            assert!(!directory_matches(&dirm[..10], 2, &mut keys));
        }
    }

    #[test]
    fn limits_depth_and_count_before_copying_strings() {
        let leaf = || Bookmark {
            title: "Heading".into(),
            url: "#1".into(),
            children: Vec::new(),
        };
        let mut branch = leaf();
        for _ in 0..MAX_OUTLINE_DEPTH + 3 {
            branch = Bookmark {
                children: vec![branch],
                ..leaf()
            };
        }
        let nodes: Vec<_> = std::iter::once(branch)
            .chain(std::iter::repeat_with(leaf).take(MAX_OUTLINE_ITEMS + 3))
            .collect();
        let mut linear = vec![];
        flatten(&nodes, 0, &mut linear);
        assert_eq!(linear.len(), MAX_OUTLINE_ITEMS);
        assert_eq!(
            linear.iter().map(|row| row.1).max(),
            Some(MAX_OUTLINE_DEPTH - 1)
        );
        assert_eq!(linear[MAX_OUTLINE_DEPTH].1, 0);
    }
}
