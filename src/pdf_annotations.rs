//! Interchange with unencrypted PDFs that have an intact cross-reference table.
//! A copy uses an incremental update so the source bytes and existing annotations survive.

use crate::{Result, fixed_text::PdfTextRect, library::PdfHighlight};
use hayro::hayro_syntax::{
    object::{Array as PdfArray, Dict as PdfDict, MaybeRef, Object as PdfObject, ObjectIdentifier},
    reader::{Reader as PdfReader, ReaderContext, ReaderExt},
};
use hayro::kurbo::{Affine, Point, Rect};
use lopdf::xref::XrefEntry;
use lopdf::{
    DecompressError, Dictionary, Document, IncrementalDocument, LoadOptions, Object, ObjectId,
    Stream, decode_text_string, dictionary, text_string,
};
use std::{
    cell::RefCell,
    collections::{BTreeMap, HashSet},
    fmt::Write as _,
    io::Write,
};

pub const MAX_INPUT_BYTES: usize = 16 * 1024 * 1024;
const MAX_OUTPUT_BYTES: usize = 32 * 1024 * 1024;
const MAX_DECOMPRESSED_STREAM: usize = 4 * 1024 * 1024;
const MAX_PAGES: usize = 10_000;
const MAX_OBJECTS: usize = 65_536;
const MAX_OBJECT_ID: u32 = 2_000_000;
const MAX_OBJECT_STREAMS: usize = 512;
const MAX_MEMBERS_PER_STREAM: usize = 2_048;
const MAX_COMPRESSED_MEMBERS: usize = 32_768;
const MAX_OBJECT_STREAM_BYTES: usize = 16 * 1024 * 1024;
const MAX_XREF_SECTIONS: usize = 256;
const MAX_XREF_ENTRIES: usize = 131_072;
const MAX_XREF_DICTIONARY_BYTES: usize = 256 * 1024;
const MAX_PAGE_ANNOTS: usize = 1_024;
const MAX_MARKS: usize = 512;
const MAX_QUADS: usize = 64;
const MAX_COORD: f64 = 10_000_000.0;
const SELECTED_TEXT: &str = "PrimitiveOSSelectedText";

#[derive(Clone, Copy, Debug)]
pub struct PageGeometry {
    pub width: f32,
    pub height: f32,
    pub upright_from_pdf: Affine,
}

#[derive(Debug)]
pub struct ImportedHighlight {
    pub color: crate::highlight_style::HighlightColor,
    pub style: crate::highlight_style::HighlightStyle,
    pub page: usize,
    pub bounds: Vec<PdfTextRect>,
    pub text: Option<String>,
    pub note: Option<String>,
}

/// Imports visible `/Highlight` annotations with quadrilaterals or a rectangle alone.
pub fn import_highlights(
    input: &[u8],
    mut geometry: impl FnMut(usize) -> Option<PageGeometry>,
) -> Result<Vec<ImportedHighlight>> {
    let (document, pages) = load_pdf(input)?;
    let mut highlights = Vec::new();
    for (page_index, page_id) in pages.into_iter().enumerate() {
        let mut annots = page_annots(&document, page_id)?
            .iter()
            .filter_map(|value| visible_highlight(&document, value))
            .peekable();
        if annots.peek().is_none() {
            continue;
        }
        let Some(geometry) = geometry(page_index).filter(PageGeometry::is_valid) else {
            return Err("PDF highlight page geometry is unavailable".into());
        };
        for annotation in annots {
            let bounds = import_bounds(&document, annotation, geometry);
            if bounds.is_empty() {
                continue;
            }
            if highlights.len() == MAX_MARKS {
                return Err("PDF contains more than 512 importable highlights".into());
            }
            highlights.push(ImportedHighlight {
                color: pdf_highlight_color(&document,annotation),
                style: pdf_drawing_style(&document, annotation).unwrap_or_default(),
                page: page_index,
                bounds,
                text: read_text(&document, annotation, SELECTED_TEXT.as_bytes(), 8_192),
                note: read_text(&document, annotation, b"Contents", 4_096),
            });
        }
    }
    Ok(highlights)
}

/// Returns a copy with new marks. Existing marks with the same bounds and note are retained once.
/// Signed PDFs and invalid or out-of-range marks are rejected.
pub fn export_copy(
    input: &[u8],
    highlights: &[PdfHighlight],
    mut geometry: impl FnMut(usize) -> Option<PageGeometry>,
) -> Result<Vec<u8>> {
    if highlights.is_empty()
        || highlights.len() > MAX_MARKS
        || highlights.iter().any(|mark| !mark.is_valid())
        || highlights.iter().map(|mark| mark.continuation.len() + 1).sum::<usize>() > MAX_MARKS
    {
        return Err("Choose between 1 and 512 valid PDF highlight pages to export".into());
    }
    let (document, pages) = load_pdf(input)?;
    if document.catalog()?.has(b"Perms") || document.objects.values().any(is_signature) {
        return Err("Reader cannot export highlights into a signed or certified PDF".into());
    }
    let mut by_page = BTreeMap::<usize, Vec<PdfHighlight>>::new();
    for mark in highlights {
        for (page, bounds, text) in mark.parts() {
            by_page.entry(page).or_default().push(PdfHighlight {
                color: mark.color,
                created_at: mark.created_at,
                style: mark.style,
                page, bounds: bounds.to_vec(), text: text.to_owned(), note: mark.note.clone(), continuation: Vec::new(),
            });
        }
    }
    let mut copy = IncrementalDocument::create_from(input.to_vec(), document);
    for (page_index, marks) in by_page {
        let page_id = *pages
            .get(page_index)
            .ok_or("PDF highlight references a missing page")?;
        let geometry = geometry(page_index)
            .filter(PageGeometry::is_valid)
            .ok_or("PDF highlight page geometry is unavailable")?;
        let previous = copy.get_prev_documents();
        let existing = page_annots(previous, page_id)?;
        let marks = new_marks(previous, existing, geometry, marks.iter().collect());
        if marks.is_empty() {
            continue;
        }
        if existing.len() + marks.len() > MAX_PAGE_ANNOTS {
            return Err("PDF page would exceed the supported annotation count".into());
        }
        let mut annots = existing.to_vec();
        for mark in marks {
            annots.push(Object::Reference(add_annotation(
                &mut copy.new_document,
                page_id,
                mark,
                geometry,
            )?));
        }
        // An indirect /Annots array may be shared. Replace it on this page only.
        copy.opt_clone_object_to_new_document(page_id)?;
        copy.new_document
            .get_dictionary_mut(page_id)?
            .set("Annots", Object::Array(annots));
    }
    if copy.new_document.objects.is_empty() {
        return Ok(input.to_vec());
    }
    let mut output = BoundedOutput(Vec::with_capacity(input.len()));
    copy.save_to(&mut output)?;
    Ok(output.0)
}

fn load_pdf(input: &[u8]) -> Result<(Document, Vec<ObjectId>)> {
    if input.len() > MAX_INPUT_BYTES {
        return Err("PDF annotation interchange supports files up to 16 MiB".into());
    }
    if !input.starts_with(b"%PDF-") {
        return Err("PDF annotation interchange requires a PDF document".into());
    }
    preflight_xref(input)?;
    let (mut document, budget) = parse_document(input)?;
    if document.is_encrypted() || document.was_encrypted() || document.trailer.has(b"Encrypt") {
        return Err("PDF annotation interchange requires an unencrypted document".into());
    }
    if document.xref_recovery.is_some()
        || document.objects.len() > MAX_OBJECTS
        || document.max_id > MAX_OBJECT_ID
        || document.xref_section_count > MAX_XREF_SECTIONS
        || document.reference_table.entries.len() > MAX_XREF_ENTRIES
    {
        return Err("PDF cross-reference data exceeds the supported limits".into());
    }
    for (&number, entry) in &document.reference_table.entries {
        if let XrefEntry::Compressed { container, index } = entry
            && (budget
                .streams
                .get(container)
                .and_then(|ids| ids.get(usize::from(*index)))
                != Some(&number)
                || !document.has_object((number, 0)))
        {
            return Err(
                "PDF compressed-object cross-reference does not match its object stream".into(),
            );
        }
    }
    // Expanded containers have served their purpose. Incremental export uses the original bytes.
    for id in budget.streams.keys() {
        document.objects.remove(&(*id, 0));
    }
    let pages: Vec<_> = document.page_iter().take(MAX_PAGES + 1).collect();
    if pages.is_empty() || pages.len() > MAX_PAGES {
        return Err("PDF annotation interchange supports documents with 1–10,000 pages".into());
    }
    Ok((document, pages))
}

const OBJECT_STREAM_LIMIT: &str = "PDF compressed object streams exceed Reader's supported limits";
const OBJECT_STREAM_INVALID: &str =
    "PDF compressed object stream has an inconsistent index or member";
const XREF_LIMIT: &str = "PDF cross-reference data exceeds the supported limits";
const XREF_INVALID: &str = "PDF cross-reference sections could not be read without repair";

// Lopdf follows the revision chain before invoking its filter, and its encrypted path skips the
// filter. Read only the trailer dictionaries here, before it loads or expands any objects.
fn preflight_xref(input: &[u8]) -> Result<()> {
    let end = input
        .windows(5)
        .rposition(|part| part == b"%%EOF")
        .ok_or(XREF_INVALID)?;
    let marker = input[..end]
        .windows(9)
        .rposition(|part| part == b"startxref")
        .ok_or(XREF_INVALID)?;
    let mut start = PdfReader::new(&input[marker + 9..end]);
    let mut pending = vec![read_unsigned(&mut start).ok_or(XREF_INVALID)? as usize];
    let mut visited = HashSet::new();
    let mut entries = 0;
    while let Some(offset) = pending.pop() {
        if !visited.insert(offset) {
            continue;
        }
        if visited.len() > MAX_XREF_SECTIONS {
            return Err(XREF_LIMIT.into());
        }
        let mut at = PdfReader::new(input.get(offset..end).ok_or(XREF_INVALID)?);
        at.skip_white_spaces_and_comments();
        let classic = at.forward_tag(b"xref").is_some();
        if classic {
            read_classic_xref(&mut at, &mut entries)?;
        } else {
            at.skip::<ObjectIdentifier>(false).ok_or(XREF_INVALID)?;
        }
        at.skip_white_spaces_and_comments();
        let tail = at.tail().ok_or(XREF_INVALID)?;
        let mut bounded = PdfReader::new(&tail[..tail.len().min(MAX_XREF_DICTIONARY_BYTES)]);
        let trailer = bounded
            .read::<PdfDict>(&ReaderContext::dummy())
            .ok_or(XREF_INVALID)?;
        if trailer.contains_key(b"Encrypt") {
            return Err("PDF annotation interchange requires an unencrypted document".into());
        }
        if !classic {
            read_stream_xref(&trailer, &mut entries)?;
        }
        for key in [b"Prev".as_slice(), b"XRefStm"] {
            if trailer.contains_key(key) {
                let previous = trailer.get::<u32>(key).ok_or(XREF_INVALID)?;
                pending.push(previous as usize);
            }
        }
    }
    Ok(())
}

fn read_unsigned(reader: &mut PdfReader<'_>) -> Option<u32> {
    reader.skip_white_spaces_and_comments();
    reader.read_without_context::<u32>()
}

fn charge_xref(entries: &mut usize, first: u32, count: u32) -> Result<()> {
    if count as usize > MAX_XREF_ENTRIES - *entries
        || first
            .checked_add(count)
            .is_none_or(|end| end > MAX_OBJECT_ID + 1)
    {
        return Err(XREF_LIMIT.into());
    }
    *entries += count as usize;
    Ok(())
}

fn read_classic_xref(reader: &mut PdfReader<'_>, entries: &mut usize) -> Result<()> {
    loop {
        reader.skip_white_spaces_and_comments();
        if reader.forward_tag(b"trailer").is_some() {
            return Ok(());
        }
        let first = read_unsigned(reader).ok_or(XREF_INVALID)?;
        let count = read_unsigned(reader).ok_or(XREF_INVALID)?;
        charge_xref(entries, first, count)?;
        for _ in 0..count {
            read_unsigned(reader).ok_or(XREF_INVALID)?;
            read_unsigned(reader).ok_or(XREF_INVALID)?;
            reader.skip_white_spaces_and_comments();
            if !matches!(reader.read_byte(), Some(b'n' | b'f')) {
                return Err(XREF_INVALID.into());
            }
        }
    }
}

fn read_stream_xref(trailer: &PdfDict<'_>, entries: &mut usize) -> Result<()> {
    if !trailer.contains_key(b"Index") {
        return charge_xref(entries, 0, trailer.get::<u32>(b"Size").ok_or(XREF_INVALID)?);
    }
    let index = trailer.get::<PdfArray>(b"Index").ok_or(XREF_INVALID)?;
    let mut fields = PdfReader::new(index.data());
    loop {
        fields.skip_white_spaces_and_comments();
        if fields.at_end() {
            return Ok(());
        }
        let first = read_unsigned(&mut fields).ok_or(XREF_INVALID)?;
        let count = read_unsigned(&mut fields).ok_or(XREF_INVALID)?;
        charge_xref(entries, first, count)?;
    }
}

#[derive(Default)]
struct LoadBudget {
    streams: BTreeMap<u32, Vec<u32>>,
    members: usize,
    decoded_bytes: usize,
    visited_objects: usize,
    error: Option<&'static str>,
}

thread_local! {
    // Lopdf is built without Rayon and calls FilterFunc synchronously on the loading thread.
    static LOAD_BUDGET: RefCell<Option<LoadBudget>> = const { RefCell::new(None) };
}

struct ClearLoadBudget;

impl Drop for ClearLoadBudget {
    fn drop(&mut self) {
        LOAD_BUDGET.with(|budget| budget.borrow_mut().take());
    }
}

fn parse_document(input: &[u8]) -> Result<(Document, LoadBudget)> {
    LOAD_BUDGET.with(|budget| *budget.borrow_mut() = Some(LoadBudget::default()));
    let _clear = ClearLoadBudget;
    let parsed = Document::load_mem_with_options(
        input,
        LoadOptions {
            strict: true,
            max_decompressed_size: Some(MAX_DECOMPRESSED_STREAM),
            filter: Some(filter_object),
            ..LoadOptions::default()
        },
    );
    let budget = LOAD_BUDGET
        .with(|budget| budget.borrow_mut().take())
        .ok_or("PDF loading budget is unavailable")?;
    if let Some(error) = budget.error {
        return Err(error.into());
    }
    let document = parsed.map_err(|error| format!("Cannot read PDF annotations: {error}"))?;
    Ok((document, budget))
}

fn filter_object(id: ObjectId, object: &mut Object) -> Option<(ObjectId, Object)> {
    LOAD_BUDGET.with(|state| {
        let mut state = state.borrow_mut();
        let budget = state.as_mut()?;
        if budget.error.is_some() {
            return None;
        }
        budget.visited_objects += 1;
        if budget.visited_objects > MAX_OBJECTS || id.0 > MAX_OBJECT_ID {
            budget.error = Some("PDF object count or numbering exceeds Reader's supported limits");
            return None;
        }
        if let Object::Stream(stream) = object {
            if stream.dict.has_type(b"ObjStm")
                && let Err(error) = budget.check_stream(id, stream)
            {
                budget.error = Some(error);
                return None;
            }
            // Lopdf retains the original for top-level objects. Nested members cannot be streams.
            return Some((id, Object::Null));
        }
        // Lopdf keeps the returned value when the callback runs on a compressed member.
        Some((id, object.clone()))
    })
}

impl LoadBudget {
    fn check_stream(
        &mut self,
        id: ObjectId,
        stream: &mut Stream,
    ) -> std::result::Result<(), &'static str> {
        let integer = |name: &[u8]| stream.dict.get(name).ok()?.as_i64().ok()?.try_into().ok();
        let count: usize = integer(b"N").ok_or(OBJECT_STREAM_INVALID)?;
        let first: usize = integer(b"First").ok_or(OBJECT_STREAM_INVALID)?;
        if self.streams.len() == MAX_OBJECT_STREAMS
            || count > MAX_MEMBERS_PER_STREAM
            || count > MAX_COMPRESSED_MEMBERS - self.members
            || first > MAX_DECOMPRESSED_STREAM
        {
            return Err(OBJECT_STREAM_LIMIT);
        }
        if id.1 != 0 || self.streams.contains_key(&id.0) {
            return Err(OBJECT_STREAM_INVALID);
        }
        let remaining = (MAX_OBJECT_STREAM_BYTES - self.decoded_bytes).min(MAX_DECOMPRESSED_STREAM);
        let plain = stream
            .get_plain_content_with_limit(remaining)
            .map_err(|error| {
                if matches!(
                    error,
                    lopdf::Error::Decompress(DecompressError::MemoryLimitExceeded { .. })
                ) {
                    OBJECT_STREAM_LIMIT
                } else {
                    OBJECT_STREAM_INVALID
                }
            })?;
        let members = validate_members(&plain, first, count).ok_or(OBJECT_STREAM_INVALID)?;
        self.members += count;
        self.decoded_bytes += plain.len();
        self.streams.insert(id.0, members);
        stream.set_plain_content(plain);
        Ok(())
    }
}

fn validate_members(plain: &[u8], first: usize, count: usize) -> Option<Vec<u32>> {
    let (header, body) = plain.split_at_checked(first)?;
    let mut tokens = std::str::from_utf8(header)
        .ok()?
        .split(['\r', '\n'])
        .flat_map(|line| {
            line.split('%')
                .next()
                .unwrap_or_default()
                .split_whitespace()
        });
    let mut ids = Vec::with_capacity(count);
    let mut offsets = Vec::with_capacity(count + 1);
    let mut seen = HashSet::with_capacity(count);
    for _ in 0..count {
        let id = tokens.next()?.parse::<u32>().ok()?;
        let offset = tokens.next()?.parse::<usize>().ok()?;
        if id == 0
            || id > MAX_OBJECT_ID
            || !seen.insert(id)
            || offset >= body.len()
            || offsets
                .last()
                .map_or(offset != 0, |previous| offset <= *previous)
        {
            return None;
        }
        ids.push(id);
        offsets.push(offset);
    }
    if tokens.next().is_some() || (count == 0 && !body.is_empty()) {
        return None;
    }
    offsets.push(body.len());
    // Lopdf's full-stream parser can read past a member's offset range. Check each range in
    // isolation first so overlapping arrays, strings or comments cannot multiply allocations.
    for range in offsets.windows(2) {
        let (start, end) = (range[0], range[1]);
        if start > 0 && !pdf_separator(body[start - 1]) && !pdf_separator(body[start]) {
            return None;
        }
        let mut member = PdfReader::new(&body[start..end]);
        member.skip_white_spaces_and_comments();
        member.skip::<MaybeRef<PdfObject>>(false)?;
        member.skip_white_spaces_and_comments();
        if !member.at_end() {
            return None;
        }
    }
    Some(ids)
}

fn pdf_separator(byte: u8) -> bool {
    matches!(
        byte,
        0 | b'\t'
            | b'\n'
            | 0x0c
            | b'\r'
            | b' '
            | b'('
            | b')'
            | b'<'
            | b'>'
            | b'['
            | b']'
            | b'{'
            | b'}'
            | b'/'
            | b'%'
    )
}

fn page_annots(document: &Document, page_id: ObjectId) -> Result<&[Object]> {
    let page = document.get_dictionary(page_id)?;
    if !page.has(b"Annots") {
        return Ok(&[]);
    }
    let (_, object) = document.dereference(page.get(b"Annots")?)?;
    if matches!(object, Object::Null) {
        return Ok(&[]);
    }
    let annots = object
        .as_array()
        .map_err(|_| "PDF page has an invalid annotation array")?;
    if annots.len() > MAX_PAGE_ANNOTS {
        return Err("PDF page exceeds the supported annotation count".into());
    }
    Ok(annots)
}

fn pdf_highlight_color(document: &Document, annotation: &Dictionary) -> crate::highlight_style::HighlightColor {
    use crate::highlight_style::HighlightColor;
    if let Some(name)=read_text(document,annotation,b"PrimitiveOSColor",128) { return HighlightColor::from_name(&name); }
    if let Ok(Object::Array(rgb))=annotation.get(b"C") {
        if rgb.len()==3 {
            let values:Option<Vec<f32>>=rgb.iter().map(|value| match value {Object::Integer(n)=>Some(*n as f32), Object::Real(n)=>Some(*n), _=>None}).collect();
            if let Some(values)=values {
                for color in HighlightColor::ALL {
                    if let Some((r,g,b))=color.rgb() && values.iter().zip([r,g,b]).all(|(a,b)|(*a-b as f32/255.).abs()<0.001) { return color; }
                }
            }
        }
    }
    HighlightColor::Gray
}

fn pdf_drawing_style(
    document: &Document,
    annotation: &Dictionary,
) -> Option<crate::highlight_style::HighlightStyle> {
    use crate::highlight_style::HighlightStyle;
    let subtype = annotation
        .get_deref(b"Subtype", document)
        .ok()?
        .as_name()
        .ok()?;
    match subtype {
        b"Underline" => Some(HighlightStyle::Underline),
        b"StrikeOut" => Some(HighlightStyle::Strikethrough),
        b"Highlight" => Some(
            if annotation
                .get(b"PrimitiveOSDrawer")
                .and_then(Object::as_name)
                .ok()
                == Some(b"Invert".as_slice())
            {
                HighlightStyle::Invert
            } else {
                HighlightStyle::Lighten
            },
        ),
        _ => None,
    }
}

fn visible_highlight<'a>(document: &'a Document, object: &'a Object) -> Option<&'a Dictionary> {
    let annotation = document.dereference(object).ok()?.1.as_dict().ok()?;
    pdf_drawing_style(document, annotation)?;
    let flags = annotation
        .get_deref(b"F", document)
        .and_then(Object::as_i64)
        .unwrap_or(0);
    (flags & (1 | 2 | 32) == 0).then_some(annotation)
}

fn new_marks<'a>(
    document: &Document,
    existing: &[Object],
    geometry: PageGeometry,
    marks: Vec<&'a PdfHighlight>,
) -> Vec<&'a PdfHighlight> {
    let existing: Vec<_> = existing
        .iter()
        .filter_map(|value| visible_highlight(document, value))
        .filter_map(|annotation| {
            let bounds = ordered_bounds(import_bounds(document, annotation, geometry));
            (!bounds.is_empty()).then(|| {
                (
                    bounds,
                    read_text(document, annotation, b"Contents", 4_096),
                    pdf_drawing_style(document, annotation),
                    pdf_highlight_color(document,annotation),
                )
            })
        })
        .collect();
    marks
        .into_iter()
        .filter(|mark| {
            let note = mark.note.as_deref().filter(|note| !note.trim().is_empty());
            let bounds = ordered_bounds(mark.bounds.clone());
            !existing.iter().any(|(previous, previous_note, style, color)| {
                *style == Some(mark.style) && *color == mark.color
                    && previous_note.as_deref() == note
                    && same_bounds(previous, &bounds)
            })
        })
        .collect()
}

fn ordered_bounds(mut bounds: Vec<PdfTextRect>) -> Vec<PdfTextRect> {
    bounds.sort_unstable_by(|a, b| {
        a.y0.total_cmp(&b.y0)
            .then_with(|| a.x0.total_cmp(&b.x0))
            .then_with(|| a.y1.total_cmp(&b.y1))
            .then_with(|| a.x1.total_cmp(&b.x1))
    });
    bounds
}

fn same_bounds(a: &[PdfTextRect], b: &[PdfTextRect]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(a, b)| {
            // PDF coordinates pass through a float conversion when written and read back.
            [a.x0 - b.x0, a.y0 - b.y0, a.x1 - b.x1, a.y1 - b.y1]
                .into_iter()
                .all(|delta| delta.abs() <= 1e-6)
        })
}

fn is_signature(object: &Object) -> bool {
    object.as_dict().is_ok_and(|dict| {
        dict.get(b"Type").and_then(Object::as_name).ok() == Some(b"Sig")
            || (dict.has(b"ByteRange") && dict.has(b"Contents"))
    })
}

fn read_text(
    document: &Document,
    annotation: &Dictionary,
    name: &[u8],
    limit: usize,
) -> Option<String> {
    let (_, value) = document.dereference(annotation.get(name).ok()?).ok()?;
    if value.as_str().ok()?.len() > limit * 2 + 2 {
        return None;
    }
    let mut text = decode_text_string(value).ok()?;
    text.truncate(text.floor_char_boundary(limit));
    (!text.trim().is_empty()).then_some(text)
}

fn import_bounds(
    document: &Document,
    annotation: &Dictionary,
    geometry: PageGeometry,
) -> Vec<PdfTextRect> {
    let points = |key: &[u8]| {
        document
            .dereference(annotation.get(key).ok()?)
            .ok()?
            .1
            .as_array()
            .ok()
    };
    if annotation.has(b"QuadPoints") {
        return points(b"QuadPoints")
            .filter(|values| values.len() % 8 == 0 && values.len() <= MAX_QUADS * 8)
            .into_iter()
            .flat_map(|values| values.as_slice().as_chunks::<8>().0.iter())
            .filter_map(|quad| {
                geometry.normalize([
                    pdf_point(&quad[0..2])?,
                    pdf_point(&quad[2..4])?,
                    pdf_point(&quad[4..6])?,
                    pdf_point(&quad[6..8])?,
                ])
            })
            .collect();
    }
    let Some(rect) = points(b"Rect").filter(|values| values.len() == 4) else {
        return Vec::new();
    };
    let Some(a) = pdf_point(&rect[0..2]) else {
        return Vec::new();
    };
    let Some(b) = pdf_point(&rect[2..4]) else {
        return Vec::new();
    };
    geometry
        .normalize([a, Point::new(a.x, b.y), b, Point::new(b.x, a.y)])
        .into_iter()
        .collect()
}

fn pdf_point(values: &[Object]) -> Option<Point> {
    let point = Point::new(
        f64::from(values[0].as_float().ok()?),
        f64::from(values[1].as_float().ok()?),
    );
    valid_point(point).then_some(point)
}

fn valid_point(point: Point) -> bool {
    point.x.abs() <= MAX_COORD && point.y.abs() <= MAX_COORD
}

fn envelope(points: impl IntoIterator<Item = Point>) -> Rect {
    points.into_iter().fold(
        Rect::new(
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ),
        |rect, point| {
            Rect::new(
                rect.x0.min(point.x),
                rect.y0.min(point.y),
                rect.x1.max(point.x),
                rect.y1.max(point.y),
            )
        },
    )
}

impl PageGeometry {
    fn is_valid(&self) -> bool {
        f64::from(self.width) <= MAX_COORD
            && f64::from(self.height) <= MAX_COORD
            && self.width > 0.0
            && self.height > 0.0
            && self.upright_from_pdf.is_finite()
            && self.upright_from_pdf.determinant().abs() > 1e-12
    }

    fn normalize(self, points: [Point; 4]) -> Option<PdfTextRect> {
        let points = points.map(|point| self.upright_from_pdf * point);
        if !points.iter().copied().all(valid_point) {
            return None;
        }
        let bounds = envelope(points);
        let x = |value: f64| (value / f64::from(self.width)).clamp(0.0, 1.0) as f32;
        let y = |value: f64| (value / f64::from(self.height)).clamp(0.0, 1.0) as f32;
        let rect = PdfTextRect {
            x0: x(bounds.x0),
            y0: y(bounds.y0),
            x1: x(bounds.x1),
            y1: y(bounds.y1),
        };
        (rect.x1 > rect.x0 && rect.y1 > rect.y0).then_some(rect)
    }

    fn quad(self, rect: PdfTextRect) -> [Point; 4] {
        let inverse = self.upright_from_pdf.inverse();
        // The first edge is the text baseline; PDF vertices then proceed counterclockwise.
        [
            (rect.x0, rect.y1),
            (rect.x1, rect.y1),
            (rect.x1, rect.y0),
            (rect.x0, rect.y0),
        ]
        .map(|(x, y)| {
            inverse
                * Point::new(
                    f64::from(x) * f64::from(self.width),
                    f64::from(y) * f64::from(self.height),
                )
        })
    }
}

fn real(value: f64) -> Object {
    Object::Real(value as f32)
}

fn pdf_rect(rect: Rect) -> Object {
    Object::Array(
        [rect.x0, rect.y0, rect.x1, rect.y1]
            .into_iter()
            .map(real)
            .collect(),
    )
}

fn add_annotation(
    document: &mut Document,
    page: ObjectId,
    mark: &PdfHighlight,
    geometry: PageGeometry,
) -> Result<ObjectId> {
    let quads: Vec<_> = mark
        .bounds
        .iter()
        .map(|rect| geometry.quad(*rect))
        .collect();
    if !quads.iter().flatten().copied().all(valid_point) {
        return Err("PDF highlight coordinates exceed the supported range".into());
    }
    let bounds = envelope(quads.iter().flatten().copied());
    use crate::highlight_style::HighlightStyle;
    let subtype = match mark.style {
        HighlightStyle::Underline => "Underline",
        HighlightStyle::Strikethrough => "StrikeOut",
        HighlightStyle::Lighten | HighlightStyle::Invert => "Highlight",
    };
    let appearance = document.add_object(appearance(bounds, &quads, mark.style, mark.color));
    let quad_points: Vec<_> = quads
        .iter()
        .flatten()
        .flat_map(|point| [real(point.x), real(point.y)])
        .collect();
    let color: Vec<Object> = if let Some((r,g,b))=mark.color.rgb() { [r,g,b].into_iter().map(|channel|Object::Real(channel as f32/255.)).collect() } else { match mark.style {
        HighlightStyle::Lighten => vec![1.into(), 1.into(), 0.into()],
        HighlightStyle::Underline => vec![Object::Real(68.0 / 255.0); 3],
        HighlightStyle::Strikethrough => vec![0.into(); 3],
        HighlightStyle::Invert => vec![1.into(); 3],
    }};
    let mut annotation = dictionary! {
        "Type" => "Annot",
        "Subtype" => subtype,
        "P" => page,
        "Rect" => pdf_rect(bounds),
        "QuadPoints" => Object::Array(quad_points),
        "C" => Object::Array(color),
        "PrimitiveOSColor" => text_string(&mark.color.label().to_lowercase()),
        "F" => 4,
        "T" => text_string("Reader"),
        "AP" => dictionary! { "N" => appearance },
        SELECTED_TEXT => text_string(&mark.text),
    };
    if mark.style == HighlightStyle::Invert {
        annotation.set("PrimitiveOSDrawer", Object::Name(b"Invert".to_vec()));
    }
    if let Some(note) = mark.note.as_deref().filter(|note| !note.is_empty()) {
        annotation.set("Contents", text_string(note));
    }
    Ok(document.add_object(annotation))
}

fn appearance(
    bounds: Rect,
    quads: &[[Point; 4]],
    style: crate::highlight_style::HighlightStyle,
    color: crate::highlight_style::HighlightColor,
) -> Stream {
    use crate::highlight_style::HighlightStyle;
    let (initial, blend, alpha) = match style {
        HighlightStyle::Lighten => ("q /GS gs 1 1 0 rg\n", "Multiply", 0.25),
        HighlightStyle::Invert => ("q /GS gs 1 g\n", "Difference", 1.0),
        HighlightStyle::Underline => ("q /GS gs 0.27 G\n", "Normal", 1.0),
        HighlightStyle::Strikethrough => ("q /GS gs 0 G\n", "Normal", 1.0),
    };
    let mut data = if let Some((r,g,b))=color.rgb().filter(|_|style!=HighlightStyle::Invert) {
        format!("q /GS gs {:.6} {:.6} {:.6} {}\n",r as f32/255.,g as f32/255.,b as f32/255., if matches!(style,HighlightStyle::Underline|HighlightStyle::Strikethrough) {"RG"} else {"rg"})
    } else {String::from(initial)};
    let alpha=if color.rgb().is_some() {1.0} else {alpha};
    for quad in quads {
        if matches!(
            style,
            HighlightStyle::Underline | HighlightStyle::Strikethrough
        ) {
            let height = quad[0].distance(quad[3]);
            let width = if style == HighlightStyle::Underline {
                (height * 0.075).clamp(0.5, 3.)
            } else {
                (height * 0.05).clamp(0.5, 2.)
            };
            let fraction = if style == HighlightStyle::Underline {
                (width * 0.5 / height.max(1.)).min(0.5)
            } else {
                0.5
            };
            let a = quad[0].lerp(quad[3], fraction);
            let b = quad[1].lerp(quad[2], fraction);
            writeln!(
                data,
                "{width:.6} w {:.6} {:.6} m {:.6} {:.6} l S",
                a.x - bounds.x0,
                a.y - bounds.y0,
                b.x - bounds.x0,
                b.y - bounds.y0
            )
            .expect("String write");
        } else {
            for (index, point) in quad.iter().enumerate() {
                let operator = if index == 0 { 'm' } else { 'l' };
                writeln!(
                    data,
                    "{:.6} {:.6} {operator}",
                    point.x - bounds.x0,
                    point.y - bounds.y0
                )
                .expect("String write");
            }
            data.push_str("h f\n");
        }
    }
    data.push_str("Q\n");
    Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "BBox" => pdf_rect(Rect::new(0.0, 0.0, bounds.width(), bounds.height())),
            "Resources" => dictionary! {
                "ExtGState" => dictionary! {
                    "GS" => dictionary! { "Type" => "ExtGState", "BM" => blend, "ca" => Object::Real(alpha) }
                }
            },
        },
        data.into_bytes(),
    )
}

struct BoundedOutput(Vec<u8>);

impl Write for BoundedOutput {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > MAX_OUTPUT_BYTES - self.0.len() {
            return Err(std::io::Error::other("Annotated PDF copy exceeds 32 MiB"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(signed: bool) -> Vec<u8> {
        let mut document = Document::with_version("1.7");
        let pages = document.new_object_id();
        let original_highlight = document.add_object(dictionary! {
            "Type" => "Annot",
            "Subtype" => "Highlight",
            "Rect" => pdf_rect(Rect::new(20.0, 100.0, 70.0, 120.0)),
            "Contents" => text_string("Existing café note"),
        });
        let mut link_and_marks = vec![
            Object::Dictionary(dictionary! {
                "Subtype" => "Link",
                "Rect" => pdf_rect(Rect::new(20.0, 100.0, 30.0, 120.0)),
            }),
            Object::Reference(original_highlight),
            Object::Dictionary(dictionary! {
                "Subtype" => "Highlight",
                "QuadPoints" => Object::Array(vec![1.into(), 2.into()]),
            }),
            Object::Dictionary(dictionary! {
                "Subtype" => "Highlight",
                "Rect" => pdf_rect(Rect::new(20.0, 100.0, 70.0, 120.0)),
                "F" => 32,
            }),
        ];
        if signed {
            link_and_marks.push(Object::Dictionary(dictionary! { "Subtype" => "Widget" }));
        }
        let shared_annots = document.add_object(link_and_marks);
        let media_box = pdf_rect(Rect::new(0.0, 0.0, 120.0, 250.0));
        let crop_box = pdf_rect(Rect::new(10.0, 20.0, 110.0, 220.0));
        let page = document.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages, "MediaBox" => media_box.clone(),
            "CropBox" => crop_box.clone(), "Annots" => shared_annots,
        });
        let rotated = document.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages, "MediaBox" => media_box,
            "CropBox" => crop_box, "Rotate" => 90, "Annots" => shared_annots,
        });
        document.set_object(pages, dictionary! {
            "Type" => "Pages", "Kids" => Object::Array(vec![page.into(), rotated.into()]), "Count" => 2,
        });
        let mut catalog = dictionary! { "Type" => "Catalog", "Pages" => pages };
        if signed {
            catalog.set("Perms", dictionary! { "DocMDP" => 1 });
        }
        let catalog = document.add_object(catalog);
        document.trailer.set("Root", catalog);
        let mut result = Vec::new();
        document.save_to(&mut result).unwrap();
        result
    }

    fn geometry(page: usize) -> Option<PageGeometry> {
        match page {
            0 => Some(PageGeometry {
                width: 100.0,
                height: 200.0,
                upright_from_pdf: Affine::new([1., 0., 0., -1., -10., 220.]),
            }),
            1 => Some(PageGeometry {
                width: 200.0,
                height: 100.0,
                upright_from_pdf: Affine::new([0., 1., 1., 0., -20., -10.]),
            }),
            _ => None,
        }
    }

    fn mark() -> PdfHighlight {
        PdfHighlight {
            color: Default::default(),
            created_at: None,
            style: Default::default(),
            continuation: Vec::new(),
            page: 1,
            bounds: vec![
                PdfTextRect {
                    x0: 0.125,
                    y0: 0.20,
                    x1: 0.60,
                    y1: 0.25,
                },
                PdfTextRect {
                    x0: 0.10,
                    y0: 0.27,
                    x1: 0.50,
                    y1: 0.32,
                },
            ],
            text: "Thủy triều —潮汐".to_owned(),
            note: Some("Café note + ☀".to_owned()),
        }
    }

    #[test]
    fn rotates_and_crops_external_highlights_with_note_separate_from_selected_text() {
        let highlights = import_highlights(&fixture(false), geometry).unwrap();
        assert_eq!(highlights.len(), 2);
        assert_eq!(
            highlights[0].bounds,
            vec![PdfTextRect {
                x0: 0.10,
                y0: 0.50,
                x1: 0.60,
                y1: 0.60
            }]
        );
        assert_eq!(
            highlights[1].bounds,
            vec![PdfTextRect {
                x0: 0.40,
                y0: 0.10,
                x1: 0.50,
                y1: 0.60
            }]
        );
        assert_eq!(highlights[1].page, 1);
        assert_eq!(highlights[1].text, None);
        assert_eq!(highlights[1].note.as_deref(), Some("Existing café note"));
    }

    #[test]
    fn named_annotation_colors_survive_pdf_copy_streams_reimport_and_deduplication() {
        use crate::highlight_style::{HighlightColor,HighlightStyle};
        let original=fixture(false);
        for style in HighlightStyle::ALL {
            for color in HighlightColor::ALL {
                let mut mark=mark();mark.style=style;mark.color=color;
                let copy=export_copy(&original,std::slice::from_ref(&mark),geometry).unwrap();
                let marks=import_highlights(&copy,geometry).unwrap();
                let saved=marks.iter().find(|m|m.text.as_ref()==Some(&mark.text)).unwrap();
                assert_eq!((saved.style,saved.color),(style,color));assert_eq!(saved.note,mark.note);
                assert_eq!(export_copy(&copy,std::slice::from_ref(&mark),geometry).unwrap(),copy);
                let (doc,pages)=load_pdf(&copy).unwrap();
                let annotations=page_annots(&doc,pages[1]).unwrap();
                let dict=doc.get_dictionary(annotations.last().unwrap().as_reference().unwrap()).unwrap();
                if let Some((r,g,b))=color.rgb() {
                    let actual=dict.get(b"C").unwrap().as_array().unwrap();
                    for (value,expected) in actual.iter().zip([r,g,b]) {assert!((value.as_float().unwrap()-expected as f32/255.).abs()<0.0001);}
                }
            }
        }
    }

    #[test]
    fn exported_highlight_styles_keep_their_standard_subtype_appearance_and_note() {
        use crate::highlight_style::HighlightStyle;
        let original = fixture(false);
        for style in HighlightStyle::ALL {
            let mut mark = mark();
            mark.style = style;
            let copy = export_copy(&original, std::slice::from_ref(&mark), geometry).unwrap();
            let imported = import_highlights(&copy, geometry).unwrap();
            let saved = imported
                .iter()
                .find(|m| m.text.as_ref() == Some(&mark.text))
                .unwrap();
            assert_eq!(saved.style, style);
            assert_eq!(saved.note, mark.note);
            let (doc, pages) = load_pdf(&copy).unwrap();
            let annots = page_annots(&doc, pages[1]).unwrap();
            let dict = doc
                .get_dictionary(annots.last().unwrap().as_reference().unwrap())
                .unwrap();
            let subtype = dict.get(b"Subtype").unwrap().as_name().unwrap();
            let expected: &[u8] = match style {
                HighlightStyle::Underline => b"Underline",
                HighlightStyle::Strikethrough => b"StrikeOut",
                _ => b"Highlight",
            };
            assert_eq!(subtype, expected);
            let normal = dict
                .get(b"AP")
                .unwrap()
                .as_dict()
                .unwrap()
                .get_deref(b"N", &doc)
                .unwrap()
                .as_stream()
                .unwrap();
            let content = normal
                .decompressed_content()
                .unwrap_or_else(|_| normal.content.clone());
            assert!(String::from_utf8(content).unwrap().contains(
                if matches!(
                    style,
                    HighlightStyle::Underline | HighlightStyle::Strikethrough
                ) {
                    "l S"
                } else {
                    "h f"
                }
            ));
        }
    }

    #[test]
    fn appends_interoperable_appearance_to_a_copy_and_preserves_shared_annots() {
        let original = fixture(false);
        let mark = mark();
        let copy = export_copy(&original, std::slice::from_ref(&mark), geometry).unwrap();
        assert!(copy.starts_with(&original));
        assert!(copy.len() > original.len());

        let (document, pages) = load_pdf(&copy).unwrap();
        assert_eq!(page_annots(&document, pages[0]).unwrap().len(), 4);
        let annots = page_annots(&document, pages[1]).unwrap();
        assert_eq!(annots.len(), 5);
        let link = annots[0].as_dict().unwrap();
        assert_eq!(link.get(b"Subtype").unwrap().as_name().unwrap(), b"Link");
        let exported = document
            .get_dictionary(annots[4].as_reference().unwrap())
            .unwrap();
        let quad_points = exported.get(b"QuadPoints").unwrap().as_array().unwrap();
        assert_eq!(quad_points.len(), 16);
        let appearance = exported.get(b"AP").unwrap().as_dict().unwrap();
        let form = document
            .get_object(appearance.get(b"N").unwrap().as_reference().unwrap())
            .unwrap()
            .as_stream()
            .unwrap();
        assert_eq!(
            form.dict.get(b"Subtype").unwrap().as_name().unwrap(),
            b"Form"
        );
        assert!(form.content.starts_with(b"q /GS gs 1 1 0 rg"));

        let reread = import_highlights(&copy, geometry).unwrap();
        assert_eq!(reread.len(), 3);
        let restored = &reread[2];
        assert_eq!(restored.page, mark.page);
        assert_eq!(restored.text.as_deref(), Some(mark.text.as_str()));
        assert_eq!(restored.note, mark.note);
        for (actual, expected) in restored.bounds.iter().zip(mark.bounds) {
            for delta in [
                actual.x0 - expected.x0,
                actual.y0 - expected.y0,
                actual.x1 - expected.x1,
                actual.y1 - expected.y1,
            ] {
                assert!(delta.abs() < 1e-6);
            }
        }
    }

    #[test]
    fn avoids_reappending_existing_geometry_and_retains_an_edited_note() {
        let original = fixture(false);
        let existing = import_highlights(&original, geometry).unwrap().remove(0);
        let external = PdfHighlight {
            color: Default::default(),
            created_at: None,
            style: Default::default(),
            continuation: Vec::new(),
            page: existing.page,
            bounds: existing.bounds,
            text: "Imported PDF highlight".into(),
            note: existing.note,
        };
        assert_eq!(
            export_copy(&original, &[external], geometry).unwrap(),
            original
        );

        let mut new = mark();
        let once = export_copy(&original, std::slice::from_ref(&new), geometry).unwrap();
        new.bounds.reverse();
        assert_eq!(
            export_copy(&once, std::slice::from_ref(&new), geometry).unwrap(),
            once
        );
        new.note = Some("Edited note".into());
        let twice = export_copy(&once, &[new], geometry).unwrap();
        let marks = import_highlights(&twice, geometry).unwrap();
        assert_eq!(marks.len(), 4);
        assert_eq!(marks[2].note.as_deref(), Some("Café note + ☀"));
        assert_eq!(marks[3].note.as_deref(), Some("Edited note"));
    }

    #[test]
    fn roundtrips_classic_and_compressed_cross_references() {
        for compressed in [false, true] {
            let mut document = Document::load_mem(&fixture(false)).unwrap();
            document.reference_table.cross_reference_type = if compressed {
                lopdf::xref::XrefType::CrossReferenceStream
            } else {
                lopdf::xref::XrefType::CrossReferenceTable
            };
            let options = lopdf::SaveOptions::builder()
                .use_object_streams(compressed)
                .use_xref_streams(compressed)
                .build();
            let mut source = Vec::new();
            document.save_with_options(&mut source, options).unwrap();
            let (_, budget) = parse_document(&source).unwrap();
            assert_eq!(!budget.streams.is_empty(), compressed);

            let output = export_copy(&source, &[mark()], geometry).unwrap();
            assert!(output.starts_with(&source));
            assert_eq!(import_highlights(&output, geometry).unwrap().len(), 3);
        }
    }

    #[test]
    fn rejects_underdeclared_overlapping_and_aggregate_object_streams() {
        let malformed = b"9 0 10 2 (abcd)";
        assert!(validate_members(malformed, b"9 0 10 2 ".len(), 1).is_none());
        assert!(validate_members(malformed, b"9 0 10 2 ".len(), 2).is_none());

        let mut plain = b"9 0 (".to_vec();
        plain.extend(std::iter::repeat_n(b'x', 2048));
        plain.push(b')');
        let mut obj_stream = Stream::new(
            dictionary! { "Type" => "ObjStm", "N" => 1, "First" => 4 },
            plain,
        );
        obj_stream.compress().unwrap();
        assert!(obj_stream.content.len() < 200);
        let mut budget = LoadBudget {
            decoded_bytes: MAX_OBJECT_STREAM_BYTES - 200,
            ..LoadBudget::default()
        };
        assert_eq!(
            budget.check_stream((7, 0), &mut obj_stream),
            Err(OBJECT_STREAM_LIMIT)
        );
        assert!(budget.streams.is_empty());
    }

    #[test]
    fn checked_trailers_reject_encryption_and_excess_revisions_before_loading_objects() {
        let encrypted = b"%PDF-1.7\nxref\n0 1\n0000000000 65535 f \ntrailer\n<< /Size 1 /Encr#79pt 1 0 R >>\nstartxref\n9\n%%EOF\n";
        let error = preflight_xref(encrypted).unwrap_err();
        assert!(error.to_string().contains("unencrypted"));

        let mut revisions = Vec::from(b"%PDF-1.7\n");
        let mut previous = None;
        for _ in 0..=MAX_XREF_SECTIONS {
            let offset = revisions.len();
            write!(
                revisions,
                "xref\n0 1\n0000000000 65535 f \ntrailer\n<< /Size 1"
            )
            .unwrap();
            if let Some(previous) = previous {
                write!(revisions, " /Prev {previous}").unwrap();
            }
            write!(revisions, " >>\nstartxref\n{offset}\n%%EOF\n").unwrap();
            previous = Some(offset);
        }
        assert_eq!(
            preflight_xref(&revisions).unwrap_err().to_string(),
            XREF_LIMIT
        );
    }

    #[test]
    fn rejects_oversize_invalid_signed_or_out_of_range_input() {
        assert!(
            import_highlights(&vec![0; MAX_INPUT_BYTES + 1], geometry)
                .unwrap_err()
                .to_string()
                .contains("16 MiB")
        );
        assert!(import_highlights(b"%PDF-broken", geometry).is_err());
        assert!(
            export_copy(&fixture(true), &[mark()], geometry)
                .unwrap_err()
                .to_string()
                .contains("signed or certified")
        );
        let mut mark = mark();
        mark.page = 10;
        assert!(
            export_copy(&fixture(false), &[mark], geometry)
                .unwrap_err()
                .to_string()
                .contains("missing page")
        );
    }
}
