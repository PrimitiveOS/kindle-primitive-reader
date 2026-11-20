//! Bounded standalone Kindle Format 8 import.
//!
//! Record layout references: calibre's reader/index.py and writer8/skeleton.py,
//! and libmobi's format declarations. This module implements the bounded subset
//! directly and keeps reconstructed chapters and raster assets in memory.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use roxmltree::{Document, Node, ParsingOptions};

use crate::mobi::{self, MAX_MOBI_BYTES, MobiError};

type Result<T> = std::result::Result<T, MobiError>;

const XHTML: &str = "http://www.w3.org/1999/xhtml";
const MAX_CHAPTERS: usize = 256;
const MAX_FRAGMENTS: usize = 4096;
const MAX_INDEX_RECORDS: usize = 64;
const MAX_FLOWS: usize = 128;
const MAX_LINKS: usize = 2048;
const MAX_ASSETS: usize = 128;
const MAX_ASSET_BYTES: usize = 4 * 1024 * 1024;
const MAX_TOTAL_ASSET_BYTES: usize = 8 * 1024 * 1024;
const MAX_XHTML_BYTES: usize = 768 * 1024;
const MAX_XML_NODES: u32 = 65_536;

pub(crate) struct Kf8Document {
    pub title: Option<String>,
    pub author: Option<String>,
    pub chapters: Vec<Kf8Chapter>,
    pub resources: BTreeMap<String, Vec<u8>>,
}

pub(crate) struct Kf8Chapter {
    pub path: String,
    pub label: String,
    pub xhtml: String,
}

struct Records<'a> {
    bytes: &'a [u8],
    offsets: Vec<usize>,
}

impl<'a> Records<'a> {
    fn at(&self, index: usize) -> Result<&'a [u8]> {
        let end = index.checked_add(1).ok_or(MobiError::InvalidHeader)?;
        let start = *self.offsets.get(index).ok_or(MobiError::InvalidHeader)?;
        let end = *self.offsets.get(end).ok_or(MobiError::InvalidHeader)?;
        Ok(&self.bytes[start..end])
    }
}

pub(crate) fn decode(bytes: &[u8]) -> Result<Kf8Document> {
    if bytes.len() > MAX_MOBI_BYTES {
        return Err(MobiError::TooLarge);
    }
    if bytes.get(60..68) != Some(b"BOOKMOBI") {
        return Err(MobiError::UnsupportedContainer);
    }
    let records = Records {
        bytes,
        offsets: mobi::record_offsets(bytes)?,
    };
    let record = records.at(0)?;
    let end = integer(record, 20)?
        .checked_add(16)
        .ok_or(MobiError::InvalidHeader)?;
    let header = record.get(..end).ok_or(MobiError::InvalidHeader)?;
    if header.get(16..20) != Some(b"MOBI") {
        return Err(MobiError::InvalidHeader);
    }
    if mobi::u16_at(header, 12)? != 0 {
        return Err(MobiError::Protected);
    }
    if integer(header, 36)? != 8 {
        return Err(MobiError::Kf8);
    }
    if !matches!(mobi::u32_at(header, 172)?, 0 | u32::MAX) {
        return Err(MobiError::Protected);
    }
    match mobi::u32_at(header, 28)? {
        65001 => {}
        value => return Err(MobiError::UnsupportedEncoding(value)),
    }
    if !matches!(integer(header, 24)?, 2 | 248) {
        return Err(MobiError::Kf8);
    }
    check_reflowable(&record[end..])?;
    let first_nontext = usize::from(mobi::u16_at(header, 8)?) + 1;
    let raw = mobi::decode_records(bytes, &records.offsets, header, mobi::u16_at(header, 242)?)?;
    let flow = read_main_flow(&records, header, &raw, first_nontext)?;
    let skel = integer(header, 0xfc)?;
    let div = integer(header, 0xf8)?;
    if skel < first_nontext || div < first_nontext || skel == div {
        return Err(MobiError::InvalidHeader);
    }
    let skeletons = read_index(&records, skel, MAX_CHAPTERS)?;
    let fragments = read_index(&records, div, MAX_FRAGMENTS)?;
    let (chapters, locations) = reconstruct(flow, &skeletons, &fragments)?;
    let images = integer(header, 0x6c)?;
    let (chapters, resources) = normalize(chapters, &locations, &records, images, first_nontext)?;
    let (title, author) = mobi::kf8_labels(bytes, record, header);
    Ok(Kf8Document {
        title,
        author,
        chapters,
        resources,
    })
}

fn check_reflowable(exth: &[u8]) -> Result<()> {
    if !exth.starts_with(b"EXTH") {
        return Ok(());
    }
    let length = integer(exth, 4)?;
    let count = integer(exth, 8)?;
    if count > 512 {
        return Err(MobiError::InvalidHeader);
    }
    let mut entries = exth.get(12..length).ok_or(MobiError::InvalidHeader)?;
    for _ in 0..count {
        let kind = integer(entries, 0)?;
        let length = integer(entries, 4)?;
        let value = entries.get(8..length).ok_or(MobiError::InvalidHeader)?;
        if kind == 122 && !matches!(value, b"false" | b"0" | b"no") {
            return Err(MobiError::Kf8);
        }
        if kind == 121 && value != u32::MAX.to_be_bytes() {
            return Err(MobiError::Kf8);
        }
        entries = &entries[length..];
    }
    Ok(())
}

fn read_main_flow<'a>(
    records: &Records<'_>,
    header: &[u8],
    raw: &'a [u8],
    first: usize,
) -> Result<&'a [u8]> {
    let count = integer(header, 0xc4)?;
    if count == 1 {
        return Ok(raw);
    }
    let index = integer(header, 0xc0)?;
    if !(2..=MAX_FLOWS).contains(&count) || index < first {
        return Err(MobiError::InvalidHeader);
    }
    let fdst = records.at(index)?;
    let start = integer(fdst, 4)?;
    if !fdst.starts_with(b"FDST") || start != 12 || integer(fdst, 8)? != count {
        return Err(MobiError::InvalidHeader);
    }
    let mut end = 0;
    for entry in 0..count {
        let offset = start + entry * 8;
        let next = integer(fdst, offset + 4)?;
        if integer(fdst, offset)? != end || next < end || next > raw.len() {
            return Err(MobiError::InvalidHeader);
        }
        end = next;
    }
    if end != raw.len() {
        return Err(MobiError::InvalidHeader);
    }
    Ok(&raw[..integer(fdst, start + 4)?])
}

struct Entry<'a> {
    label: &'a [u8],
    tags: BTreeMap<u8, Vec<usize>>,
}

fn read_index<'a>(records: &Records<'a>, index: usize, max: usize) -> Result<Vec<Entry<'a>>> {
    let meta = records.at(index)?;
    let start = index_header(meta)?;
    let segments = integer(meta, 24)?;
    let expected = integer(meta, 36)?;
    if !(1..=MAX_INDEX_RECORDS).contains(&segments) || !(1..=max).contains(&expected) {
        return Err(MobiError::InvalidHeader);
    }
    let remaining = meta.get(start..).ok_or(MobiError::InvalidHeader)?;
    let size = integer(remaining, 4)?;
    let controls = integer(remaining, 8)?;
    let schema = remaining.get(12..size).ok_or(MobiError::InvalidHeader)?;
    if !remaining.starts_with(b"TAGX")
        || !(1..=4).contains(&controls)
        || schema.is_empty()
        || schema.len() > 128
        || !schema.len().is_multiple_of(4)
    {
        return Err(MobiError::InvalidHeader);
    }
    let tags = schema.as_chunks::<4>().0;
    if tags.iter().filter(|tag| tag[3] == 1).count() != controls
        || tags.last().is_none_or(|tag| tag[3] != 1)
        || tags
            .iter()
            .any(|[_, values, mask, delimiter]| match delimiter {
                0 => *mask == 0 || !(1..=4).contains(values),
                1 => *values != 0 || *mask != 0,
                _ => true,
            })
    {
        return Err(MobiError::InvalidHeader);
    }
    let mut entries = Vec::with_capacity(expected);
    for part in 1..=segments {
        let data = records.at(index.checked_add(part).ok_or(MobiError::InvalidHeader)?)?;
        let lower = index_header(data)?;
        let table = integer(data, 20)?;
        let count = integer(data, 24)?;
        if table < lower
            || data
                .get(table..)
                .is_none_or(|part| !part.starts_with(b"IDXT"))
            || count > expected - entries.len()
        {
            return Err(MobiError::InvalidHeader);
        }
        let mut last = None;
        for i in 0..=count {
            let offset = if i == count {
                table
            } else {
                usize::from(mobi::u16_at(data, table + 4 + i * 2)?)
            };
            if !(lower..=table).contains(&offset) {
                return Err(MobiError::InvalidHeader);
            }
            if let Some(start) = last {
                let entry = data.get(start..offset).ok_or(MobiError::InvalidHeader)?;
                entries.push(read_entry(entry, tags, controls)?);
            }
            last = Some(offset);
        }
    }
    if entries.len() != expected {
        return Err(MobiError::InvalidHeader);
    }
    Ok(entries)
}

fn index_header(bytes: &[u8]) -> Result<usize> {
    let length = integer(bytes, 4)?;
    if !bytes.starts_with(b"INDX")
        || !(56..=bytes.len()).contains(&length)
        || bytes.len() > 64 * 1024
    {
        return Err(MobiError::InvalidHeader);
    }
    Ok(length)
}

fn read_entry<'a>(entry: &'a [u8], tags: &[[u8; 4]], controls: usize) -> Result<Entry<'a>> {
    let (&name_len, entry) = entry.split_first().ok_or(MobiError::InvalidHeader)?;
    let (label, entry) = entry
        .split_at_checked(usize::from(name_len))
        .ok_or(MobiError::InvalidHeader)?;
    let (control, mut values) = entry
        .split_at_checked(controls)
        .ok_or(MobiError::InvalidHeader)?;
    let mut selection = Vec::new();
    let mut group = 0;
    for &[tag, amount, mask, delimiter] in tags {
        if delimiter == 1 {
            group += 1;
            continue;
        }
        let value = *control.get(group).ok_or(MobiError::InvalidHeader)? & mask;
        if value == 0 {
            continue;
        }
        let bytes = (value == mask && mask.count_ones() > 1)
            .then(|| varint(&mut values))
            .transpose()?;
        let count = usize::from(value >> mask.trailing_zeros()) * usize::from(amount);
        selection.push((tag, count, bytes));
    }
    let mut result = BTreeMap::new();
    for (tag, count, byte_length) in selection {
        let mut tag_values = Vec::new();
        if let Some(length) = byte_length {
            let (mut fields, remaining) = values
                .split_at_checked(length)
                .ok_or(MobiError::InvalidHeader)?;
            values = remaining;
            while !fields.is_empty() {
                if tag_values.len() == 16 {
                    return Err(MobiError::InvalidHeader);
                }
                tag_values.push(varint(&mut fields)?);
            }
        } else {
            if count > 16 {
                return Err(MobiError::InvalidHeader);
            }
            for _ in 0..count {
                tag_values.push(varint(&mut values)?);
            }
        }
        if tag_values.is_empty() || result.insert(tag, tag_values).is_some() {
            return Err(MobiError::InvalidHeader);
        }
    }
    if values.len() > 3 || values.iter().any(|byte| *byte != 0) {
        return Err(MobiError::InvalidHeader);
    }
    Ok(Entry {
        label,
        tags: result,
    })
}

fn varint(bytes: &mut &[u8]) -> Result<usize> {
    let mut result = 0u32;
    for _ in 0..5 {
        let (&next, tail) = bytes.split_first().ok_or(MobiError::InvalidHeader)?;
        *bytes = tail;
        result = result
            .checked_mul(128)
            .and_then(|value| value.checked_add(u32::from(next & 127)))
            .ok_or(MobiError::InvalidHeader)?;
        if next & 128 != 0 {
            return Ok(result as usize);
        }
    }
    Err(MobiError::InvalidHeader)
}

struct Location {
    chapter: usize,
    insertion: usize,
    length: usize,
}

fn reconstruct(
    flow: &[u8],
    skeletons: &[Entry<'_>],
    fragments: &[Entry<'_>],
) -> Result<(Vec<String>, Vec<Location>)> {
    let mut chapters = Vec::with_capacity(skeletons.len());
    let mut locations = Vec::with_capacity(fragments.len());
    let mut next = 0;
    for (chapter, skeleton) in skeletons.iter().enumerate() {
        let [count] = repeated_tag::<1>(skeleton, 1)?;
        let [start, length] = repeated_tag::<2>(skeleton, 6)?;
        if start != next || count > fragments.len() - locations.len() || length == 0 {
            return Err(MobiError::InvalidHeader);
        }
        next = start
            .checked_add(length)
            .filter(|end| *end <= flow.len())
            .ok_or(MobiError::InvalidHeader)?;
        let base = &flow[start..next];
        let start_fragments = next;
        let mut built = Vec::with_capacity(length);
        let mut consumed = 0;
        let group_start = locations.len();
        for fragment in &fragments[group_start..group_start + count] {
            let [owner] = repeated_tag::<1>(fragment, 3)?;
            let [sequence] = repeated_tag::<1>(fragment, 4)?;
            let [relative, fragment_length] = repeated_tag::<2>(fragment, 6)?;
            let insertion = decimal(fragment.label)?
                .checked_sub(start)
                .ok_or(MobiError::InvalidHeader)?;
            let base_at = insertion
                .checked_sub(next - start_fragments)
                .ok_or(MobiError::InvalidHeader)?;
            if owner != chapter
                || sequence != locations.len()
                || relative != next - start_fragments
                || !(consumed..=length).contains(&base_at)
                || fragment_length == 0
            {
                return Err(MobiError::InvalidHeader);
            }
            let finish = next
                .checked_add(fragment_length)
                .filter(|end| *end <= flow.len())
                .ok_or(MobiError::InvalidHeader)?;
            built.extend_from_slice(&base[consumed..base_at]);
            built.extend_from_slice(&flow[next..finish]);
            consumed = base_at;
            next = finish;
            locations.push(Location {
                chapter,
                insertion,
                length: fragment_length,
            });
        }
        built.extend_from_slice(&base[consumed..]);
        chapters.push(String::from_utf8(built).map_err(|_| MobiError::InvalidText)?);
    }
    if next != flow.len() || locations.len() != fragments.len() {
        return Err(MobiError::InvalidHeader);
    }
    Ok((chapters, locations))
}

fn repeated_tag<const N: usize>(entry: &Entry<'_>, tag: u8) -> Result<[usize; N]> {
    let values = entry.tags.get(&tag).ok_or(MobiError::InvalidHeader)?;
    let first = values.first_chunk::<N>().ok_or(MobiError::InvalidHeader)?;
    if values.len() != N && values.as_slice().get(N..) != Some(first.as_slice()) {
        return Err(MobiError::InvalidHeader);
    }
    Ok(*first)
}

fn decimal(label: &[u8]) -> Result<usize> {
    if label.is_empty() || label.len() > 10 || !label.iter().all(u8::is_ascii_digit) {
        return Err(MobiError::InvalidHeader);
    }
    std::str::from_utf8(label)
        .ok()
        .and_then(|digits| digits.parse().ok())
        .ok_or(MobiError::InvalidHeader)
}

fn integer(bytes: &[u8], offset: usize) -> Result<usize> {
    Ok(mobi::u32_at(bytes, offset)? as usize)
}

type Assets = BTreeMap<String, Vec<u8>>;

struct Edit {
    bytes: Range<usize>,
    text: String,
}

fn normalize(
    raw: Vec<String>,
    locations: &[Location],
    records: &Records<'_>,
    images: usize,
    first: usize,
) -> Result<(Vec<Kf8Chapter>, Assets)> {
    let mut remaining_nodes = MAX_XML_NODES;
    let mut documents = Vec::with_capacity(raw.len());
    for xhtml in &raw {
        let document = Document::parse_with_options(
            xhtml,
            ParsingOptions {
                nodes_limit: remaining_nodes,
                ..ParsingOptions::default()
            },
        )
        .map_err(|_| MobiError::InvalidText)?;
        let root = document.root_element();
        if !root.has_tag_name((XHTML, "html"))
            || !root
                .children()
                .any(|node| node.has_tag_name((XHTML, "body")))
            || root
                .descendants()
                .any(|node| node.ancestors().take(49).count() == 49)
        {
            return Err(MobiError::InvalidText);
        }
        let count =
            u32::try_from(document.descendants().count()).map_err(|_| MobiError::TextTooLarge)?;
        remaining_nodes = remaining_nodes
            .checked_sub(count)
            .ok_or(MobiError::TextTooLarge)?;
        documents.push(document);
    }
    let mut edits: Vec<Vec<Edit>> = (0..raw.len()).map(|_| Vec::new()).collect();
    let indexed_nodes: Vec<Vec<Node<'_, '_>>> = documents
        .iter()
        .map(|document| {
            document
                .descendants()
                .filter(|node| node.is_element() && node.tag_name().namespace() == Some(XHTML))
                .collect()
        })
        .collect();
    let mut generated = BTreeMap::<(usize, usize), String>::new();
    let ids: Vec<BTreeSet<&str>> = indexed_nodes
        .iter()
        .map(|nodes| {
            nodes
                .iter()
                .filter_map(|node| node.attribute("id"))
                .collect()
        })
        .collect();
    let mut resources = Assets::new();
    let mut asset_bytes = 0;
    let mut links = 0;
    for (chapter, document) in documents.iter().enumerate() {
        for node in document.descendants().filter(Node::is_element) {
            if matches!(
                node.tag_name().name(),
                "svg" | "canvas" | "object" | "embed" | "video" | "audio" | "iframe"
            ) {
                return Err(MobiError::Kf8);
            }
            if node.has_tag_name((XHTML, "img")) {
                if images < first {
                    return Err(MobiError::InvalidHeader);
                }
                let src = node.attribute_node("src").ok_or(MobiError::InvalidText)?;
                let number = src
                    .value()
                    .strip_prefix("kindle:embed:")
                    .ok_or(MobiError::Kf8)?;
                let number = number.split_once('?').map_or(number, |(number, _)| number);
                let number = base32(number, 4)?
                    .checked_sub(1)
                    .ok_or(MobiError::InvalidText)?;
                let index = images.checked_add(number).ok_or(MobiError::InvalidHeader)?;
                let asset = records.at(index)?;
                let extension = if asset.starts_with(b"\x89PNG\r\n\x1a\n") {
                    "png"
                } else if asset.starts_with(b"\xff\xd8\xff") {
                    "jpg"
                } else {
                    return Err(MobiError::Kf8);
                };
                let path = format!("assets/kf8-{index}.{extension}");
                if !resources.contains_key(&path) {
                    if resources.len() == MAX_ASSETS
                        || asset.len() > MAX_ASSET_BYTES
                        || asset.len() > MAX_TOTAL_ASSET_BYTES - asset_bytes
                    {
                        return Err(MobiError::TooLarge);
                    }
                    asset_bytes += asset.len();
                    resources.insert(path.clone(), asset.to_vec());
                }
                edits[chapter].push(Edit {
                    bytes: src.range_value(),
                    text: path,
                });
            }
            if !node.has_tag_name((XHTML, "a")) {
                continue;
            }
            let Some(href) = node.attribute_node("href") else {
                continue;
            };
            let Some(address) = href.value().strip_prefix("kindle:pos:fid:") else {
                continue;
            };
            links += 1;
            if links > MAX_LINKS {
                return Err(MobiError::TextTooLarge);
            }
            let (fid, offset) = address.split_once(":off:").ok_or(MobiError::InvalidText)?;
            let location = locations
                .get(base32(fid, 4)?)
                .ok_or(MobiError::InvalidText)?;
            let offset = base32(offset, 10)?;
            let target_chapter = location.chapter;
            let target = link_node(
                &raw[target_chapter],
                &indexed_nodes[target_chapter],
                location,
                offset,
            )?;
            let start = target.range().start;
            let key = (target_chapter, start);
            let id = if let Some(id) = target.attribute("id") {
                if id.is_empty()
                    || id.len() > 256
                    || !id
                        .chars()
                        .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | ':'))
                    || indexed_nodes[target_chapter]
                        .iter()
                        .filter(|node| node.attribute("id") == Some(id))
                        .take(2)
                        .count()
                        != 1
                {
                    return Err(MobiError::Kf8);
                }
                id.to_owned()
            } else if let Some(id) = generated.get(&key) {
                id.clone()
            } else {
                let id = format!("kf8-position-{start}");
                if ids[target_chapter].contains(id.as_str()) {
                    return Err(MobiError::Kf8);
                }
                let after_name = tag_name_end(&raw[target_chapter], start)?;
                edits[target_chapter].push(Edit {
                    bytes: after_name..after_name,
                    text: format!(" id=\"{id}\""),
                });
                generated.insert(key, id.clone());
                id
            };
            let prefix = if chapter == target_chapter {
                String::new()
            } else {
                chapter_path(target_chapter)
            };
            edits[chapter].push(Edit {
                bytes: href.range_value(),
                text: format!("{prefix}#{id}"),
            });
        }
    }
    let labels: Vec<_> = documents
        .iter()
        .enumerate()
        .map(|(index, document)| chapter_label(document, index))
        .collect();
    drop(ids);
    drop(indexed_nodes);
    drop(documents);

    let mut total = 0;
    let mut chapters = Vec::with_capacity(raw.len());
    for (index, ((raw, edits), label)) in raw.into_iter().zip(edits).zip(labels).enumerate() {
        let xhtml = edit_markup(&raw, edits, MAX_XHTML_BYTES - total)?;
        total += xhtml.len();
        chapters.push(Kf8Chapter {
            path: chapter_path(index),
            label,
            xhtml,
        });
    }
    Ok((chapters, resources))
}

fn link_node<'a, 'input>(
    xhtml: &str,
    nodes: &[Node<'a, 'input>],
    location: &Location,
    offset: usize,
) -> Result<Node<'a, 'input>> {
    if offset > location.length {
        return Err(MobiError::Kf8);
    }
    let position = location.insertion + offset;
    let last = nodes.partition_point(|node| node.range().start <= position);
    let &node = nodes
        .get(last.checked_sub(1).ok_or(MobiError::InvalidText)?)
        .ok_or(MobiError::InvalidText)?;
    if node.range().start == position {
        return Ok(node);
    }
    if position < opening_end(xhtml, node.range().start)? {
        return Ok(node);
    }
    // A zero offset can mark the parent of a fragment whose first bytes are text.
    if offset == 0
        && node.range().contains(&position)
        && (node.has_attribute("id") || node.has_attribute("aid") || node.has_attribute("cid"))
    {
        return Ok(node);
    }
    Err(MobiError::Kf8)
}

fn opening_end(xhtml: &str, start: usize) -> Result<usize> {
    let mut quote = None;
    for (index, byte) in xhtml.as_bytes()[start..]
        .iter()
        .copied()
        .take(8192)
        .enumerate()
    {
        match (quote, byte) {
            (Some(open), next) if open == next => quote = None,
            (None, b'\'' | b'"') => quote = Some(byte),
            (None, b'>') => return Ok(start + index + 1),
            _ => {}
        }
    }
    Err(MobiError::InvalidText)
}

fn tag_name_end(xhtml: &str, start: usize) -> Result<usize> {
    let rest = xhtml
        .as_bytes()
        .get(start + 1..)
        .ok_or(MobiError::InvalidText)?;
    let end = rest
        .iter()
        .position(|byte| byte.is_ascii_whitespace() || matches!(byte, b'/' | b'>'))
        .ok_or(MobiError::InvalidText)?;
    Ok(start + end + 1)
}

fn base32(text: &str, digits: usize) -> Result<usize> {
    if text.is_empty() || text.len() > digits {
        return Err(MobiError::InvalidText);
    }
    text.bytes().try_fold(0usize, |value, byte| {
        let digit = match byte.to_ascii_uppercase() {
            b'0'..=b'9' => byte - b'0',
            upper @ b'A'..=b'V' => upper - b'A' + 10,
            _ => return Err(MobiError::InvalidText),
        };
        value
            .checked_mul(32)
            .and_then(|value| value.checked_add(usize::from(digit)))
            .ok_or(MobiError::InvalidText)
    })
}

fn chapter_path(index: usize) -> String {
    format!("chapter-{:04}.xhtml", index + 1)
}

fn chapter_label(document: &Document<'_>, index: usize) -> String {
    let heading = document.descendants().find(|node| {
        node.tag_name().namespace() == Some(XHTML)
            && matches!(
                node.tag_name().name(),
                "h1" | "h2" | "h3" | "h4" | "h5" | "h6"
            )
    });
    let label: String = heading
        .into_iter()
        .flat_map(|node| node.descendants())
        .filter(Node::is_text)
        .filter_map(|node| node.text())
        .flat_map(str::chars)
        .filter(|ch| !ch.is_control())
        .take(160)
        .collect();
    let label = label.trim();
    if label.is_empty() {
        format!("Chapter {}", index + 1)
    } else {
        label.to_owned()
    }
}

fn edit_markup(raw: &str, mut edits: Vec<Edit>, limit: usize) -> Result<String> {
    edits.sort_unstable_by_key(|edit| (edit.bytes.start, edit.bytes.end));
    let mut xhtml = String::with_capacity(raw.len().min(limit));
    let mut cursor = 0;
    for Edit { bytes, text } in edits {
        xhtml.push_str(raw.get(cursor..bytes.start).ok_or(MobiError::InvalidText)?);
        xhtml.push_str(&text);
        cursor = bytes.end;
        if xhtml.len() > limit {
            return Err(MobiError::TextTooLarge);
        }
    }
    xhtml.push_str(raw.get(cursor..).ok_or(MobiError::InvalidText)?);
    if xhtml.len() > limit {
        return Err(MobiError::TextTooLarge);
    }
    Ok(xhtml)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "<html xmlns='http://www.w3.org/1999/xhtml'><head/><body></body></html>";
    const PNG: &[u8] = &[
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 0,
        0, 0, 0, 58, 126, 155, 85, 0, 0, 0, 10, 73, 68, 65, 84, 120, 156, 99, 96, 0, 0, 0, 2, 0, 1,
        72, 175, 164, 113, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
    ];

    fn word(bytes: &mut [u8], at: usize, value: usize) {
        bytes[at..at + 4].copy_from_slice(&(value as u32).to_be_bytes());
    }

    fn short(bytes: &mut [u8], at: usize, value: usize) {
        bytes[at..at + 2].copy_from_slice(&(value as u16).to_be_bytes());
    }

    fn encode_varint(value: usize, bytes: &mut Vec<u8>) {
        let mut chunks = [0; 5];
        let mut value = value;
        let mut last = chunks.len() - 1;
        chunks[last] = (value as u8 & 127) | 128;
        value >>= 7;
        while value != 0 {
            last -= 1;
            chunks[last] = value as u8 & 127;
            value >>= 7;
        }
        bytes.extend_from_slice(&chunks[last..]);
    }

    fn table(schema: &[u8], entries: &[(&str, u8, &[usize])]) -> [Vec<u8>; 2] {
        let mut meta = vec![0; 192];
        meta[..4].copy_from_slice(b"INDX");
        word(&mut meta, 4, 192);
        word(&mut meta, 24, 1);
        word(&mut meta, 36, entries.len());
        meta.extend_from_slice(b"TAGX");
        meta.extend_from_slice(&((12 + schema.len()) as u32).to_be_bytes());
        meta.extend_from_slice(&1_u32.to_be_bytes());
        meta.extend_from_slice(schema);

        let mut data = vec![0; 192];
        data[..4].copy_from_slice(b"INDX");
        word(&mut data, 4, 192);
        word(&mut data, 24, entries.len());
        let mut ends = Vec::new();
        for (name, bits, values) in entries {
            ends.extend_from_slice(&(data.len() as u16).to_be_bytes());
            data.push(name.len() as u8);
            data.extend_from_slice(name.as_bytes());
            data.push(*bits);
            for value in *values {
                encode_varint(*value, &mut data);
            }
        }
        let index = data.len();
        word(&mut data, 20, index);
        data.extend_from_slice(b"IDXT");
        data.extend_from_slice(&ends);
        [meta, data]
    }

    fn fixture() -> Vec<u8> {
        let first = "<h1>Shore Notes</h1><p>Look at the <a href='kindle:pos:fid:0001:off:0000000000'>next chapter</a>.</p><p><img src='kindle:embed:0001?mime=image/png'/></p>";
        let second = "<h2>Arrival</h2><p>Mặt trời lên.</p>";
        let raw = format!("{BASE}{first}{BASE}{second}");
        let body = BASE.find("</body>").unwrap();
        let next_base = BASE.len() + first.len();
        let [smeta, sdata] = table(
            &[1, 1, 1, 0, 6, 2, 2, 0, 0, 0, 0, 1],
            &[
                ("SKEL0000000000", 3, &[1, 0, BASE.len()]),
                ("SKEL0000000001", 3, &[1, next_base, BASE.len()]),
            ],
        );
        let next_insert = (next_base + body).to_string();
        let first_insert = body.to_string();
        let [dmeta, ddata] = table(
            &[3, 1, 1, 0, 4, 1, 2, 0, 6, 2, 4, 0, 0, 0, 0, 1],
            &[
                (&first_insert, 7, &[0, 0, 0, first.len()]),
                (&next_insert, 7, &[1, 1, 0, second.len()]),
            ],
        );
        let mut header = vec![0; 280];
        short(&mut header, 0, 1);
        word(&mut header, 4, raw.len());
        short(&mut header, 8, 1);
        short(&mut header, 10, 4096);
        header[16..20].copy_from_slice(b"MOBI");
        word(&mut header, 20, 264);
        word(&mut header, 24, 2);
        word(&mut header, 28, 65001);
        word(&mut header, 36, 8);
        word(&mut header, 0x6c, 6);
        word(&mut header, 0xc4, 1);
        word(&mut header, 0xf8, 4);
        word(&mut header, 0xfc, 2);
        let records = [
            header,
            raw.into_bytes(),
            smeta,
            sdata,
            dmeta,
            ddata,
            PNG.to_vec(),
        ];
        let mut pdb = vec![0; 78 + 8 * records.len()];
        pdb[..12].copy_from_slice(b"Shore Notes\0");
        pdb[60..68].copy_from_slice(b"BOOKMOBI");
        short(&mut pdb, 76, records.len());
        for (index, record) in records.iter().enumerate() {
            let offset = pdb.len();
            word(&mut pdb, 78 + 8 * index, offset);
            pdb.extend_from_slice(record);
        }
        pdb
    }

    #[test]
    fn rebuilds_reading_order_headings_a_local_raster_and_cross_chapter_link() {
        let document = decode(&fixture()).unwrap();
        assert_eq!(document.title.as_deref(), Some("Shore Notes"));
        assert_eq!(document.chapters.len(), 2);
        assert_eq!(document.chapters[0].label, "Shore Notes");
        assert_eq!(document.chapters[1].label, "Arrival");
        assert!(document.chapters[1].xhtml.contains("Mặt trời lên."));
        let first = Document::parse(&document.chapters[0].xhtml).unwrap();
        let href = first
            .descendants()
            .find(|node| node.has_tag_name("a"))
            .unwrap()
            .attribute("href")
            .unwrap();
        let (path, fragment) = href.split_once('#').unwrap();
        assert_eq!(path, document.chapters[1].path);
        let second = Document::parse(&document.chapters[1].xhtml).unwrap();
        assert_eq!(
            second
                .descendants()
                .find(|node| node.attribute("id") == Some(fragment))
                .unwrap()
                .text(),
            Some("Arrival")
        );
        let src = first
            .descendants()
            .find(|node| node.has_tag_name("img"))
            .unwrap()
            .attribute("src")
            .unwrap();
        assert_eq!(document.resources[src], PNG);
    }

    #[test]
    fn opens_kf8_through_the_book_spine_and_resolves_local_assets() {
        use std::{fs, io::Write as _};

        let path = std::env::temp_dir().join(format!("reader-kf8-{}.azw3", std::process::id()));
        let mut file = fs::File::options()
            .create_new(true)
            .write(true)
            .open(&path)
            .unwrap();
        file.write_all(&fixture()).unwrap();
        drop(file);
        let book = crate::epub::Book::open(&path);
        fs::remove_file(&path).unwrap();
        let mut book = book.unwrap();
        assert!(!book.is_fixed_layout());
        assert_eq!(
            book.toc()
                .iter()
                .map(|entry| entry.label.as_str())
                .collect::<Vec<_>>(),
            ["Shore Notes", "Arrival"]
        );
        let first = book.chapter(0).unwrap();
        let source = Document::parse(&first).unwrap();
        let link = source
            .descendants()
            .find(|node| node.has_tag_name("a"))
            .unwrap()
            .attribute("href")
            .unwrap();
        let target = crate::epub::Book::resolve_link(&book.chapters[0].href, link).unwrap();
        assert_eq!(book.chapter_index(&target.path), Some(1));
        assert!(
            book.chapter(1)
                .unwrap()
                .contains(target.fragment.as_deref().unwrap())
        );
        let image = source
            .descendants()
            .find(|node| node.has_tag_name("img"))
            .unwrap()
            .attribute("src")
            .unwrap();
        let asset = crate::epub::Book::resolve(&book.chapters[0].href, image).unwrap();
        assert_eq!(book.resource(&asset).unwrap(), PNG);
    }

    #[test]
    fn rejects_drm_huff_combined_and_bad_index_extents() {
        let good = fixture();
        let header = integer(&good, 78).unwrap();
        for (field, value, expected) in [
            (12, 1, MobiError::Protected),
            (0, 17480, MobiError::UnsupportedCompression(17480)),
        ] {
            let mut damaged = good.clone();
            short(&mut damaged, header + field, value);
            assert_eq!(decode(&damaged).err(), Some(expected));
        }
        let mut damaged = good.clone();
        word(&mut damaged, header + 36, 6);
        assert_eq!(decode(&damaged).err(), Some(MobiError::Kf8));
        let mut damaged = good.clone();
        let index = integer(&good, 78 + 8 * 3).unwrap();
        word(&mut damaged, index + 20, 16);
        assert_eq!(decode(&damaged).err(), Some(MobiError::InvalidHeader));
        let mut damaged = good.clone();
        let image = integer(&good, 78 + 8 * 6).unwrap();
        damaged[image] = 0;
        assert_eq!(decode(&damaged).err(), Some(MobiError::Kf8));
        assert!(decode(&good[..good.len() - PNG.len() - 1]).is_err());
    }

    #[test]
    fn indexes_support_byte_count_tags_and_reject_overflow() {
        let schema = [[6, 2, 3, 0], [0, 0, 0, 1]];
        let entry = read_entry(&[0, 3, 132, 128, 134, 128, 134], &schema, 1).unwrap();
        assert_eq!(repeated_tag::<2>(&entry, 6).unwrap(), [0, 6]);
        assert!(varint(&mut &[127; 5][..]).is_err());
        assert!(read_entry(&[0, 3, 140, 128], &schema, 1).is_err());
    }
}
