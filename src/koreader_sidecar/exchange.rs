//! Literal KOReader book annotation exchange, distinct from plugin JSON export.

use super::{MAX_POINTER_BYTES, MAX_SIDECAR_BYTES, SidecarError, Value, read_fields};
use crate::highlight_style::{HighlightColor, HighlightStyle};
use std::{fmt::Write, num::NonZeroU32};

const MAX_MARKS: usize = 512;
const MAX_BOXES: usize = 512;
const FIELDS: [&str; 12] = [
    "page",
    "pos0",
    "pos1",
    "pboxes",
    "text",
    "note",
    "drawer",
    "color",
    "chapter",
    "datetime",
    "datetime_updated",
    "ext",
];

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Place {
    Pointer(String),
    Page(NonZeroU32),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Mark {
    pub place: Place,
    pub created: Option<i64>,
    pub updated: Option<i64>,
    pub chapter: Option<String>,
    pub text: String,
    pub highlight: Option<Highlight>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Highlight {
    pub note: Option<String>,
    pub style: HighlightStyle,
    pub color: HighlightColor,
    pub range: Range,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Range {
    Text { start: String, end: String },
    Boxes(Vec<PageBox>),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PageBox {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Clone, Debug)]
pub(crate) struct Snapshot {
    pub device: String,
    pub fixed: bool,
    pub datetime: i64,
    pub fingerprint: Option<String>,
    pub incomplete: bool,
    pub unavailable: usize,
    pub marks: Vec<Mark>,
}

fn table<'a>(value: &Value<'a>) -> Result<&'a [u8], SidecarError> {
    match value {
        Value::Table(value) => Ok(value),
        _ => Err(SidecarError::InvalidData),
    }
}

fn records<'a, T>(
    source: &'a [u8],
    limit: usize,
    mut read: impl FnMut(&Value<'a>) -> Result<Option<T>, SidecarError>,
) -> Result<(Vec<T>, usize), SidecarError> {
    let mut rows = Vec::new();
    let mut unavailable = 0;
    super::annotations::fields(source, &mut |key, value| {
        if key.is_some() {
            return Err(SidecarError::InvalidData);
        }
        if rows.len() + unavailable >= limit {
            return Err(SidecarError::TooLarge);
        }
        match read(&value)? {
            Some(mark) => rows.push(mark),
            None => unavailable += 1,
        }
        Ok(())
    })?;
    Ok((rows, unavailable))
}

fn read_keys<'a, const N: usize>(
    bytes: &'a [u8],
    keys: &[&str; N],
) -> Result<[Option<Value<'a>>; N], SidecarError> {
    let mut values = std::array::from_fn(|_| None);
    super::annotations::fields(bytes, &mut |key, value| {
        if let Some(i) = keys
            .iter()
            .position(|name| Some(name.as_bytes()) == key.as_deref())
            && values[i].replace(value).is_some()
        {
            return Err(SidecarError::InvalidData);
        }
        Ok(())
    })?;
    Ok(values)
}

fn string(value: Option<&Value<'_>>, max: usize) -> Option<String> {
    let value = value?.string()?;
    let text = std::str::from_utf8(value).ok()?;
    (text.len() <= max && !text.contains('\0')).then(|| text.to_owned())
}

fn timestamp(value: Option<&Value<'_>>) -> Option<i64> {
    crate::mark_time::parse(value?.string()?)
}

fn boolean(value: Option<&Value<'_>>) -> bool {
    matches!(value, Some(Value::Boolean(true)))
}

fn page(value: Option<&Value<'_>>) -> Option<NonZeroU32> {
    NonZeroU32::new(value?.integer()?)
}

fn pointer(value: Option<&Value<'_>>) -> Option<String> {
    string(value, MAX_POINTER_BYTES)
        .filter(|p| p.starts_with("/body/") && !p.chars().any(char::is_control))
}

fn read_box(value: &Value<'_>) -> Result<Option<PageBox>, SidecarError> {
    let values = read_keys(table(value)?, &["x", "y", "w", "h"])?;
    let [
        Some(Value::Number(x)),
        Some(Value::Number(y)),
        Some(Value::Number(w)),
        Some(Value::Number(h)),
    ] = values
    else {
        return Ok(None);
    };
    Ok((x >= -1.0 && y >= -1.0 && w > 0.0 && h > 0.0).then_some(PageBox { x, y, w, h }))
}

fn read_mark(value: &Value<'_>, fixed: bool) -> Result<Option<Mark>, SidecarError> {
    let record = match table(value) {
        Ok(record) => record,
        Err(_) => return Ok(None),
    };
    let values = read_keys(record, &FIELDS)?;
    let [
        place,
        start,
        end,
        boxes,
        text,
        note,
        drawer,
        color,
        chapter,
        created,
        updated,
        ext,
    ] = values.each_ref().map(Option::as_ref);
    let at = if fixed {
        page(place).map(Place::Page)
    } else {
        pointer(place).map(Place::Pointer)
    };
    let Some(place) = at else { return Ok(None) };
    let text = string(text, 8192).unwrap_or_default();
    let highlight = match string(drawer, 128) {
        Some(drawer) if !text.trim().is_empty() => {
            let range = if fixed {
                // A multi-page highlight carries its per-page ranges in ext.
                if ext.is_some_and(|v| !matches!(v, Value::Nil)) {
                    return Ok(None);
                }
                let Some(boxes) = boxes.and_then(|boxes| table(boxes).ok()) else {
                    return Ok(None);
                };
                let (boxes, missing) = records(boxes, MAX_BOXES, read_box)?;
                if boxes.is_empty() || missing != 0 {
                    return Ok(None);
                }
                Range::Boxes(boxes)
            } else if let (Some(start), Some(end)) = (pointer(start), pointer(end)) {
                Range::Text { start, end }
            } else {
                return Ok(None);
            };
            Some(Highlight {
                note: string(note, 4096).filter(|note| !note.trim().is_empty()),
                style: HighlightStyle::from_drawer(&drawer),
                color: string(color, 128)
                    .map_or_else(HighlightColor::default, |c| HighlightColor::from_name(&c)),
                range,
            })
        }
        None if ![start, end, note, drawer]
            .into_iter()
            .flatten()
            .any(|v| !matches!(v, Value::Nil)) =>
        {
            None
        }
        _ => return Ok(None),
    };
    Ok(Some(Mark {
        place,
        created: timestamp(created),
        updated: timestamp(updated),
        chapter: string(chapter, 1024).filter(|s| !s.trim().is_empty()),
        text,
        highlight,
    }))
}

pub(crate) fn decode(bytes: &[u8]) -> Result<Snapshot, SidecarError> {
    const KEYS: &[&str] = &[
        "device_id",
        "paging",
        "datetime",
        "partial_md5_checksum",
        "reader_incomplete",
        "annotations",
    ];
    let mut values = std::array::from_fn::<_, 6, _>(|_| None);
    read_fields(bytes, &mut |key, value| {
        if let Some(i) = KEYS
            .iter()
            .position(|name| Some(name.as_bytes()) == key.as_deref())
            && values[i].replace(value).is_some()
        {
            return Err(SidecarError::InvalidData);
        }
        Ok(())
    })?;
    let [device, paging, datetime, fingerprint, incomplete, entries] =
        values.each_ref().map(Option::as_ref);
    if paging.is_some_and(|value| !matches!(value, Value::Nil | Value::Boolean(_))) {
        return Err(SidecarError::InvalidField("paging"));
    }
    let fixed = boolean(paging);
    let (marks, unavailable) = records(
        table(entries.ok_or(SidecarError::InvalidField("annotations"))?)?,
        MAX_MARKS,
        |entry| read_mark(entry, fixed),
    )?;
    let fingerprint = match fingerprint {
        Some(Value::Nil) | None => None,
        Some(value) => Some(
            string(Some(value), 32)
                .filter(|s| s.len() == 32 && s.bytes().all(|b| b.is_ascii_hexdigit()))
                .ok_or(SidecarError::InvalidField("partial_md5_checksum"))?,
        ),
    };
    Ok(Snapshot {
        fixed,
        device: string(device, 128)
            .filter(|s| !s.is_empty())
            .ok_or(SidecarError::InvalidField("device_id"))?,
        datetime: timestamp(datetime).ok_or(SidecarError::InvalidField("datetime"))?,
        fingerprint,
        incomplete: boolean(incomplete),
        unavailable,
        marks,
    })
}

fn quoted(output: &mut String, value: &str) {
    output.push('"');
    for character in value.chars() {
        match character {
            '\\' => output.push_str("\\\\"),
            '"' => output.push_str("\\\""),
            c if c.is_ascii_control() => write!(output, "\\{:03}", c as u32).unwrap(),
            c => output.push(c),
        }
    }
    output.push('"');
}

fn field(output: &mut String, name: &str, value: &str) {
    write!(output, "{name}=").unwrap();
    quoted(output, value);
    output.push(',');
}

pub(crate) fn encode(snapshot: &Snapshot) -> Result<String, SidecarError> {
    let mut output = String::from("return {\n");
    field(&mut output, "device_id", &snapshot.device);
    if snapshot.fixed {
        output.push_str("paging=true,");
    }
    if snapshot.incomplete {
        output.push_str("reader_incomplete=true,");
    }
    let date = crate::mark_time::local_datetime(snapshot.datetime)
        .ok_or(SidecarError::InvalidField("datetime"))?;
    field(&mut output, "datetime", &date);
    if let Some(hash) = &snapshot.fingerprint {
        field(&mut output, "partial_md5_checksum", hash);
    }
    output.push_str("\nannotations={\n");
    if snapshot.marks.len() > MAX_MARKS {
        return Err(SidecarError::TooLarge);
    }
    for mark in &snapshot.marks {
        output.push('{');
        match &mark.place {
            Place::Pointer(value) => field(&mut output, "page", value),
            Place::Page(value) => write!(output, "page={value},").unwrap(),
        }
        if let Some(chapter) = &mark.chapter {
            field(&mut output, "chapter", chapter);
        }
        field(&mut output, "text", &mark.text);
        let created = crate::mark_time::local_datetime(mark.created.unwrap_or(snapshot.datetime))
            .ok_or(SidecarError::InvalidField("datetime"))?;
        field(&mut output, "datetime", &created);
        if let Some(date) = mark.updated.and_then(crate::mark_time::local_datetime) {
            field(&mut output, "datetime_updated", &date);
        }
        if let Some(highlight) = &mark.highlight {
            if let Some(note) = &highlight.note {
                field(&mut output, "note", note);
            }
            let drawer = match highlight.style {
                HighlightStyle::Lighten => "lighten",
                HighlightStyle::Underline => "underscore",
                HighlightStyle::Strikethrough => "strikeout",
                HighlightStyle::Invert => "invert",
            };
            field(&mut output, "drawer", drawer);
            if !highlight.color.is_default() {
                field(
                    &mut output,
                    "color",
                    &highlight.color.label().to_ascii_lowercase(),
                );
            }
            match (&mark.place, &highlight.range) {
                (Place::Pointer(_), Range::Text { start, end }) => {
                    field(&mut output, "pos0", start);
                    field(&mut output, "pos1", end);
                }
                (Place::Page(page), Range::Boxes(boxes))
                    if !boxes.is_empty() && boxes.len() <= MAX_BOXES =>
                {
                    let first = boxes.first().unwrap();
                    let last = boxes.last().unwrap();
                    write!(
                        output,
                        "pos0={{page={page},x={},y={},zoom=1,rotation=0}},",
                        first.x + first.w * 0.125,
                        first.y + first.h * 0.25
                    )
                    .unwrap();
                    write!(
                        output,
                        "pos1={{page={page},x={},y={},zoom=1,rotation=0}},pboxes={{",
                        last.x + last.w * 0.875,
                        last.y + last.h * 0.75
                    )
                    .unwrap();
                    for PageBox { x, y, w, h } in boxes {
                        write!(output, "{{x={x},y={y},w={w},h={h}}},").unwrap();
                    }
                    output.push_str("},");
                }
                _ => return Err(SidecarError::InvalidField("annotations")),
            }
        }
        output.push_str("},\n");
        if output.len() > MAX_SIDECAR_BYTES {
            return Err(SidecarError::TooLarge);
        }
    }
    output.push_str("}}\n");
    Ok(output)
}
