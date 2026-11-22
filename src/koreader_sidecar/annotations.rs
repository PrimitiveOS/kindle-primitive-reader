//! Modern KOReader annotation records from readerannotation.lua at 9192014d.
//! EPUB ranges are stored in pos0/pos1 and page bookmarks in page.

use super::{MAX_POINTER_BYTES, Parser, SidecarError, Value, Visitor, read_fields};

const MAX_MARKS: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SidecarMark {
    Bookmark {
        pointer: String,
        label: Option<String>,
    },
    Highlight {
        style: crate::highlight_style::HighlightStyle,
        start: String,
        end: String,
        text: String,
        note: Option<String>,
    },
}

#[derive(Default, Debug, PartialEq, Eq)]
pub struct SidecarAnnotations {
    pub marks: Vec<SidecarMark>,
    pub unavailable: usize,
}

fn fields<'a>(table: &'a [u8], visitor: &mut Visitor<'_, 'a>) -> Result<(), SidecarError> {
    Parser {
        bytes: table,
        offset: 0,
        fields: 0,
    }
    .table(1, Some(visitor))
}

/// Extract only annotations from a generated data table. Unsupported entries
/// stay visible in the preview count, including fixed-page coordinate ranges.
pub fn parse_annotations(bytes: &[u8]) -> Result<SidecarAnnotations, SidecarError> {
    let mut table = None;
    read_fields(bytes, &mut |key, value| {
        if key.as_deref() == Some(b"annotations") {
            if table.is_some() {
                return Err(SidecarError::DuplicateField("annotations"));
            }
            let Value::Table(entries) = value else {
                return Err(SidecarError::InvalidField("annotations"));
            };
            table = Some(entries);
        }
        Ok(())
    })?;
    let mut result = SidecarAnnotations::default();
    if let Some(table) = table {
        fields(table, &mut |key, value| {
            if key.is_some() {
                return Ok(());
            }
            if result.unavailable + result.marks.len() >= MAX_MARKS {
                return Err(SidecarError::TooLarge);
            }
            let mark = match value {
                Value::Table(record) => mark(record)?,
                _ => None,
            };
            if let Some(mark) = mark {
                result.marks.push(mark);
            } else {
                result.unavailable += 1;
            }
            Ok(())
        })?;
    }
    Ok(result)
}

fn mark(record: &[u8]) -> Result<Option<SidecarMark>, SidecarError> {
    const KEYS: [&[u8]; 7] = [
        b"page",
        b"pos0",
        b"pos1",
        b"text",
        b"note",
        b"drawer",
        b"text_edited",
    ];
    let mut values: [Option<String>; 7] = Default::default();
    let mut seen = 0u8;
    let mut supported = true;
    fields(record, &mut |key, value| {
        let Some(index) = KEYS.iter().position(|k| Some(*k) == key.as_deref()) else {
            return Ok(());
        };
        if seen & (1 << index) != 0 {
            supported = false;
        }
        seen |= 1 << index;
        match value {
            Value::Nil => {}
            Value::Boolean if index == 6 => {}
            Value::String(bytes) if index < 6 => {
                let limit = [
                    MAX_POINTER_BYTES,
                    MAX_POINTER_BYTES,
                    MAX_POINTER_BYTES,
                    8192,
                    4096,
                    128,
                ][index];
                if let Ok(s) = std::str::from_utf8(&bytes)
                    && s.len() <= limit
                    && !s.contains('\0')
                {
                    values[index] = Some(s.to_owned());
                } else {
                    supported = false;
                }
            }
            _ => supported = false,
        }
        Ok(())
    })?;
    if !supported {
        return Ok(None);
    }
    let [page, start, end, text, note, drawer, _] = values;
    let pointer = |s: &String| s.starts_with('/') && !s.chars().any(char::is_control);
    let note = note.filter(|s| !s.trim().is_empty());
    Ok(match (drawer, start, end) {
        (Some(drawer), Some(start), Some(end)) if pointer(&start) && pointer(&end) => text
            .filter(|s| !s.trim().is_empty())
            .map(|text| SidecarMark::Highlight {
                style: crate::highlight_style::HighlightStyle::from_drawer(&drawer),
                start,
                end,
                text,
                note,
            }),
        (None, None, None) if note.is_none() => {
            page.filter(pointer).map(|pointer| SidecarMark::Bookmark {
                pointer,
                label: text.filter(|s| !s.trim().is_empty()),
            })
        }
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn separates_page_bookmarks_from_ranged_text_and_keeps_unknown_counts() {
        let data = br#"return { annotations = {
            [1]={page="/node", text="Chapter one"},
            [2]={drawer="underscore",pos0="/a",pos1="/b",text="Some text",note="Remember"},
            [3]={page=2,pos0={x=12,y=3}}, [4]={page="/note",note="Separate note"},
            [5]={page="/a",page="/b"},
        }, nested={annotations={{page="/wrong"}}} }"#;
        let result = parse_annotations(data).unwrap();
        assert_eq!(result.unavailable, 3);
        assert!(
            matches!(&result.marks[..], [SidecarMark::Bookmark {pointer,label:Some(label)}, SidecarMark::Highlight{style:crate::highlight_style::HighlightStyle::Underline,start,end,text,note:Some(note)}] if pointer=="/node" && label=="Chapter one" && start=="/a" && end=="/b" && text=="Some text" && note=="Remember")
        );
        assert!(parse_annotations(b"return {annotations={{page=foo()}}}").is_err());
        assert!(parse_annotations(b"return {annotations={},annotations={}}").is_err());
    }
}
