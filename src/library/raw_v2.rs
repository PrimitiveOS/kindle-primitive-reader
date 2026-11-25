//! Detached, bounded decoding of a future raw-anchor library state.
//!
//! This decoder is deliberately not connected to `Library::open`. It does
//! not sanitize the result or open books: source fingerprints and grapheme
//! boundaries must be checked against the actual book before any use.
#![allow(dead_code)]

use super::{MAX_BACKUP_BOOKS, MAX_STATE_BYTES, SavedLibrary, SavedLibraryRef, invalid_state};
use crate::source::raw_whitespace::{MAX_BLOCK_BYTES, RawAnchor, RawCaret};
use crate::{Anchor, Probe};
use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};
use std::{fmt, io};

const MAX_CARETS: usize = 8192;
mod encoder;

pub(super) fn encode(library: &SavedLibrary) -> io::Result<Vec<u8>> {
    encoder::encode(library)
}

#[derive(Clone, Copy)]
enum Place {
    Resume,
    Bookmark(usize),
    CustomContents(usize),
    HiddenFlow(usize),
    HighlightStart(usize),
    HighlightEnd(usize),
}

struct Entry {
    book: usize,
    place: Place,
    caret: RawCaret,
}

/// Parse a candidate v2 library while leaving the original bytes untouched.
/// Raw fields are permitted only at the known persisted-anchor locations.
pub(super) fn decode(bytes: &[u8]) -> io::Result<SavedLibrary> {
    if bytes.len() as u64 > MAX_STATE_BYTES {
        return Err(invalid_state("Reader state exceeds its size limit"));
    }
    let UniqueValue(mut value) = serde_json::from_slice(bytes).map_err(io::Error::other)?;
    if value.get("version").and_then(Value::as_u64) != Some(2) {
        return Err(invalid_state("Reader state version is unsupported"));
    }
    let books = value
        .get_mut("books")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| invalid_state("Reader state has no valid books list"))?;
    if books.len() > MAX_BACKUP_BOOKS {
        return Err(invalid_state("Reader state has too many books"));
    }
    let mut entries = Vec::new();
    for (book, value) in books.iter_mut().enumerate() {
        capture(
            value.get_mut("position").and_then(|p| p.get_mut("anchor")),
            book,
            Place::Resume,
            &mut entries,
        )?;
        if let Some(marks) = value.get_mut("bookmarks").and_then(Value::as_array_mut) {
            for (i, mark) in marks.iter_mut().enumerate() {
                capture(
                    mark.get_mut("position").and_then(|p| p.get_mut("anchor")),
                    book,
                    Place::Bookmark(i),
                    &mut entries,
                )?;
            }
        }
        if let Some(marks) = value
            .get_mut("custom_contents")
            .and_then(|v| v.get_mut("entries"))
            .and_then(Value::as_array_mut)
        {
            for (i, mark) in marks.iter_mut().enumerate() {
                capture(
                    mark.get_mut("position").and_then(|p| p.get_mut("anchor")),
                    book,
                    Place::CustomContents(i),
                    &mut entries,
                )?;
            }
        }
        if let Some(points) = value
            .get_mut("hidden_flows")
            .and_then(|v| v.get_mut("points"))
            .and_then(Value::as_array_mut)
        {
            for (i, point) in points.iter_mut().enumerate() {
                capture(
                    point.get_mut("position").and_then(|p| p.get_mut("anchor")),
                    book,
                    Place::HiddenFlow(i),
                    &mut entries,
                )?;
            }
        }
        if let Some(marks) = value.get_mut("highlights").and_then(Value::as_array_mut) {
            for (i, mark) in marks.iter_mut().enumerate() {
                if let Some(selection) = mark.get_mut("selection") {
                    capture(
                        selection.get_mut("start"),
                        book,
                        Place::HighlightStart(i),
                        &mut entries,
                    )?;
                    capture(
                        selection.get_mut("end"),
                        book,
                        Place::HighlightEnd(i),
                        &mut entries,
                    )?;
                }
            }
        }
    }
    if contains_raw_field(&value) {
        return Err(invalid_state(
            "A raw caret occurs outside a supported reader anchor",
        ));
    }
    let original_canonical = value.clone();
    let mut library: SavedLibrary = serde_json::from_value(value).map_err(io::Error::other)?;
    let round_trip = serde_json::to_value(SavedLibraryRef {
        version: 2,
        books: &library.books,
        collections: &library.collections,
        page_browser_grid: library.page_browser_grid,
    })
    .map_err(io::Error::other)?;
    if round_trip != original_canonical {
        return Err(invalid_state(
            "The v2 reader state contains fields that cannot be preserved",
        ));
    }
    for entry in entries {
        let state = library
            .books
            .get_mut(entry.book)
            .ok_or_else(|| invalid_state("The raw caret's book is unavailable"))?;
        let anchor = match entry.place {
            Place::Resume => state.position.as_mut().and_then(|p| p.anchor.as_mut()),
            Place::Bookmark(i) => state
                .bookmarks
                .get_mut(i)
                .and_then(|m| m.position.anchor.as_mut()),
            Place::CustomContents(i) => state
                .custom_contents
                .entries_mut_for_raw_v2()
                .get_mut(i)
                .and_then(|m| m.position.anchor.as_mut()),
            Place::HiddenFlow(i) => state
                .hidden_flows
                .points_mut_for_raw_v2()
                .get_mut(i)
                .and_then(|p| p.position.anchor.as_mut()),
            Place::HighlightStart(i) => state.highlights.get_mut(i).map(|m| &mut m.selection.start),
            Place::HighlightEnd(i) => state.highlights.get_mut(i).map(|m| &mut m.selection.end),
        }
        .ok_or_else(|| invalid_state("The raw caret's anchor is unavailable"))?;
        attach(anchor, entry.caret)?;
    }
    validate_library(&library)?;
    Ok(library)
}

/// Unlike the legacy loader, v2 must reject invalid state rather than
/// sanitize it and risk dropping distinct raw locations or absent books.
fn validate_library(library: &SavedLibrary) -> io::Result<()> {
    super::validate_state(&library.books)?;
    let mut ids = std::collections::HashSet::new();
    for state in &library.books {
        if !super::safe_path(&state.path)
            || super::DocumentFormat::for_path(&state.path).is_none()
            || !ids.insert(state.id)
            || state.bookmarks.len() > super::MAX_IMPORTED_MARKS
            || state.highlights.len() > super::MAX_IMPORTED_MARKS
        {
            return Err(invalid_state(
                "The v2 library contains an invalid or duplicate book",
            ));
        }
        if let Some(position) = &state.position {
            valid_position(position)?;
        }
        for mark in &state.bookmarks {
            valid_position(&mark.position)?;
        }
        let custom = state.custom_contents.entries();
        if custom.len() > super::custom_contents::ENTRY_LIMIT {
            return Err(invalid_state("The custom contents list is too large"));
        }
        for (i, mark) in custom.iter().enumerate() {
            valid_position(&mark.position)?;
            if mark.label.is_empty()
                || mark.label.trim() != mark.label
                || mark.label.chars().count() > super::custom_contents::TITLE_LIMIT
                || mark.label.chars().any(char::is_control)
                || custom[..i]
                    .iter()
                    .any(|prior| super::same_position(&prior.position, &mark.position))
                || (i > 0
                    && super::raw_positions::contents_order(&custom[i - 1].position)
                        > super::raw_positions::contents_order(&mark.position))
            {
                return Err(invalid_state(
                    "The custom contents list cannot be preserved safely",
                ));
            }
        }
        let flows = state.hidden_flows.points();
        if flows.len() > super::hidden_flows::MAX_POINTS {
            return Err(invalid_state("The hidden flow list is too large"));
        }
        for (i, point) in flows.iter().enumerate() {
            valid_position(&point.position)?;
            if flows[..i]
                .iter()
                .any(|prior| super::same_position(&prior.position, &point.position))
                || (i > 0
                    && super::raw_positions::flow_order(&flows[i - 1].position)
                        > super::raw_positions::flow_order(&point.position))
            {
                return Err(invalid_state(
                    "The hidden flow list cannot be preserved safely",
                ));
            }
        }
        for mark in &state.highlights {
            valid_anchor(&mark.selection.start)?;
            valid_anchor(&mark.selection.end)?;
            if mark.selection.start.book != mark.selection.end.book
                || mark.selection.text.len() > super::MAX_HIGHLIGHT_TEXT_BYTES
                || mark
                    .note
                    .as_ref()
                    .is_some_and(|note| note.len() > super::MAX_NOTE_BYTES)
            {
                return Err(invalid_state("A saved selection is invalid"));
            }
        }
    }
    let merged = super::merge_collections(
        &[],
        &library.collections,
        &library.books,
        &mut super::BackupSummary::default(),
    );
    if merged != library.collections {
        return Err(invalid_state(
            "The collection list cannot be preserved safely",
        ));
    }
    Ok(())
}

fn valid_position(position: &super::ReadingPosition) -> io::Result<()> {
    if let Some(anchor) = &position.anchor {
        valid_anchor(anchor)?;
    }
    Ok(())
}

fn valid_anchor(anchor: &Anchor) -> io::Result<()> {
    if let Some(caret) = &anchor.raw_caret {
        check(anchor, caret)
    } else if super::valid_anchor(anchor) {
        Ok(())
    } else {
        Err(invalid_state("A saved anchor is invalid"))
    }
}

pub(super) fn book_has_raw(state: &super::BookState) -> bool {
    let raw = |position: &super::ReadingPosition| {
        position
            .anchor
            .as_ref()
            .is_some_and(|anchor| anchor.raw_caret.is_some())
    };
    state.position.as_ref().is_some_and(raw)
        || state.bookmarks.iter().any(|mark| raw(&mark.position))
        || state
            .custom_contents
            .entries()
            .iter()
            .any(|mark| raw(&mark.position))
        || state
            .hidden_flows
            .points()
            .iter()
            .any(|point| raw(&point.position))
        || state.highlights.iter().any(|mark| {
            mark.selection.start.raw_caret.is_some() || mark.selection.end.raw_caret.is_some()
        })
}

fn capture(
    value: Option<&mut Value>,
    book: usize,
    place: Place,
    entries: &mut Vec<Entry>,
) -> io::Result<()> {
    let Some(anchor) = value.and_then(Value::as_object_mut) else {
        return Ok(());
    };
    let Some(raw) = anchor.remove("raw_caret") else {
        return Ok(());
    };
    if entries.len() >= MAX_CARETS {
        return Err(invalid_state(
            "Reader state has too many raw whitespace carets",
        ));
    }
    let caret: RawCaret = serde_json::from_value(raw).map_err(io::Error::other)?;
    entries.push(Entry { book, place, caret });
    Ok(())
}

fn attach(anchor: &mut Anchor, caret: RawCaret) -> io::Result<()> {
    if anchor.raw_caret.is_some() {
        return Err(invalid_state("A raw whitespace anchor is repeated"));
    }
    check(anchor, &caret)?;
    anchor.raw_caret = Some(caret);
    Ok(())
}

fn check(anchor: &Anchor, caret: &RawCaret) -> io::Result<()> {
    if anchor.illustration.is_some()
        || anchor.chapter.is_empty()
        || anchor.chapter.len() > super::MAX_ANCHOR_PATH_BYTES
        || anchor.chapter.chars().any(char::is_control)
        || anchor.offset != caret.canonical_offset
        || anchor.offset > MAX_BLOCK_BYTES
        || caret.raw_offset > MAX_BLOCK_BYTES
    {
        return Err(invalid_state(
            "A raw whitespace anchor is structurally invalid",
        ));
    }
    Ok(())
}

/// Validate an available book's exact source fingerprint before using a raw
/// location. Missing books remain stored without invented source positions.
pub(super) fn validate_in_probe(anchor: &Anchor, probe: &Probe) -> io::Result<()> {
    let Some(caret) = &anchor.raw_caret else {
        return Ok(());
    };
    check(anchor, caret)?;
    let mut canonical = anchor.clone();
    canonical.raw_caret = None;
    let bound = RawAnchor {
        canonical,
        caret: caret.clone(),
    };
    probe
        .validate_raw_whitespace_anchor(&bound)
        .ok_or_else(|| {
            invalid_state("The raw whitespace position no longer matches this document")
        })?;
    Ok(())
}

fn contains_raw_field(value: &Value) -> bool {
    match value {
        Value::Object(map) => map.contains_key("raw_caret") || map.values().any(contains_raw_field),
        Value::Array(values) => values.iter().any(contains_raw_field),
        _ => false,
    }
}

/// `serde_json::Value` normally accepts duplicate keys by keeping the last.
/// Reject them here so two competing caret fields can never be collapsed.
struct UniqueValue(Value);

impl<'de> Deserialize<'de> for UniqueValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct UniqueVisitor;
        impl<'de> Visitor<'de> for UniqueVisitor {
            type Value = UniqueValue;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("JSON with unique object keys")
            }
            fn visit_bool<E: de::Error>(self, v: bool) -> Result<Self::Value, E> {
                Ok(UniqueValue(Value::Bool(v)))
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<Self::Value, E> {
                Ok(UniqueValue(Value::Number(v.into())))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Self::Value, E> {
                Ok(UniqueValue(Value::Number(v.into())))
            }
            fn visit_f64<E: de::Error>(self, v: f64) -> Result<Self::Value, E> {
                Number::from_f64(v)
                    .map(|n| UniqueValue(Value::Number(n)))
                    .ok_or_else(|| E::custom("invalid number"))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                Ok(UniqueValue(Value::String(v.to_owned())))
            }
            fn visit_string<E: de::Error>(self, v: String) -> Result<Self::Value, E> {
                Ok(UniqueValue(Value::String(v)))
            }
            fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
                Ok(UniqueValue(Value::Null))
            }
            fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
                Ok(UniqueValue(Value::Null))
            }
            fn visit_some<D: Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
                UniqueValue::deserialize(d)
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut result = Vec::new();
                while let Some(UniqueValue(value)) = seq.next_element()? {
                    result.push(value);
                }
                Ok(UniqueValue(Value::Array(result)))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut result = Map::new();
                while let Some((key, UniqueValue(value))) =
                    map.next_entry::<String, UniqueValue>()?
                {
                    if result.insert(key, value).is_some() {
                        return Err(de::Error::custom("duplicate object key"));
                    }
                }
                Ok(UniqueValue(Value::Object(result)))
            }
        }
        deserializer.deserialize_any(UniqueVisitor)
    }
}
