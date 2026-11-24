//! A bounded, offline WordprocessingML subset for Reader's reflow pipeline.
//! ECMA-376 Parts 1 and 2 define the document vocabulary and package relationships.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{self, Write as _};
use std::io::{Read, Seek};

use encoding_rs::Encoding;
use roxmltree::{Document as Xml, Node, ParsingOptions};
use zip::{CompressionMethod, ZipArchive};

use crate::Result;
use crate::epub::{Book, Destination, TocEntry, check_zip_directory, is_archive_path};
use crate::loose::{CHAPTER_PATH, Document};

pub(crate) const MAX_INPUT: usize = 16 * 1024 * 1024;
const MAX_ENTRIES: usize = 512;
const MAX_PATH: usize = 512;
const MAX_MAIN_XML: usize = 2 * 1024 * 1024;
const MAX_OTHER_XML: usize = 256 * 1024;
const MAX_RELATIONS_BYTES: usize = 1024 * 1024;
const MAX_XML_NODES: u32 = 80_000;
const MAX_XML_DEPTH: usize = 48;
const MAX_OUTPUT: usize = 1024 * 1024;
const MAX_HEADINGS: usize = 512;
const MAX_LABEL: usize = 512;
const MAX_STYLES: usize = 1024;

const WORD_NS: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const STRICT_WORD_NS: &str = "http://purl.oclc.org/ooxml/wordprocessingml/main";
const TYPES_NS: &str = "http://schemas.openxmlformats.org/package/2006/content-types";
const RELS_NS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const OFFICE_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/";
const STRICT_OFFICE_REL: &str = "http://purl.oclc.org/ooxml/officeDocument/relationships/";
const CORE_REL: &str =
    "http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties";
const CORE_NS: &str = "http://schemas.openxmlformats.org/package/2006/metadata/core-properties";
const DC_NS: &str = "http://purl.org/dc/elements/1.1/";
const MAIN_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml";
const PACKAGE_RELS: &str = "_rels/.rels";
const CONTENT_TYPES: &str = "[content_types].xml";

pub(crate) fn open(
    mut input: impl Read + Seek,
    len: u64,
    fallback_title: &str,
) -> Result<Document> {
    if len > MAX_INPUT as u64 {
        return Err("The Word document exceeds the 16 MiB input limit".into());
    }
    let count = check_zip_directory(&mut input, len)
        .map_err(|_| "The Word document has an invalid ZIP directory")?;
    if count > MAX_ENTRIES {
        return Err("The Word document contains too many package entries".into());
    }
    let mut archive = ZipArchive::new(input)?;
    if archive.len() != count {
        return Err("The Word document contains ambiguous or duplicate ZIP entries".into());
    }
    let parts = index_parts(&mut archive)?;
    let types = read_xml_part(&mut archive, &parts, CONTENT_TYPES, MAX_OTHER_XML)?;
    let types = content_types(&types)?;
    let mut relations = read_relationships(&mut archive, &parts)?;
    let package = relations
        .remove(PACKAGE_RELS)
        .ok_or("The Word document has no package relationships")?;
    let main_path = package
        .main
        .ok_or("The Word document has no main document part")?;
    if !types
        .for_part(&main_path)
        .is_some_and(|content| content.eq_ignore_ascii_case(MAIN_TYPE))
    {
        return Err("The package does not declare a macro-free Word document main part".into());
    }
    let main = read_xml_part(&mut archive, &parts, &main_path, MAX_MAIN_XML)?;
    let styles = match relations
        .remove(&relationships_name(&main_path))
        .and_then(|refs| refs.styles)
    {
        Some(path) => read_styles(&read_xml_part(&mut archive, &parts, &path, MAX_OTHER_XML)?)?,
        None => BTreeMap::new(),
    };
    let (title, author, metadata) = match package.core {
        Some(path) => read_metadata(&read_xml_part(&mut archive, &parts, &path, MAX_OTHER_XML)?)?,
        None => (None, None, Default::default()),
    };
    let output = convert_main(&main, &styles)?;
    Ok(Document {
        fb2_note_sections: Default::default(),
        metadata,
        title: title.unwrap_or_else(|| fallback_title.into()),
        author,
        chapter_path: CHAPTER_PATH.to_owned(),
        xhtml: output.xhtml,
        toc: output.toc,
        resources: BTreeMap::new(),
    })
}

fn index_parts<R: Read + Seek>(archive: &mut ZipArchive<R>) -> Result<BTreeMap<String, usize>> {
    let mut parts = BTreeMap::new();
    for index in 0..archive.len() {
        let entry = archive.by_index_raw(index)?;
        let raw = std::str::from_utf8(entry.name_raw())?;
        let name = part_name(raw.trim_end_matches('/'))?;
        if entry.encrypted() || entry.is_symlink() {
            return Err("Encrypted or linked Word package entries are unsupported".into());
        }
        if entry.is_dir() {
            continue;
        }
        let script = name
            .rsplit_once('.')
            .is_some_and(|(_, extension)| matches!(extension, "js" | "mjs" | "vbs"));
        if script
            || name.split('/').any(|part| {
                matches!(
                    part,
                    "vbaproject.bin" | "vbadata.xml" | "activex" | "embeddings"
                )
            })
        {
            return Err("The Word document contains macros or embedded executable content".into());
        }
        if parts.insert(name, index).is_some() {
            return Err("The Word document contains duplicate package part names".into());
        }
    }
    Ok(parts)
}

fn part_name(path: &str) -> Result<String> {
    if path.len() > MAX_PATH || !is_archive_path(path) || path.contains(['#', '?']) {
        return Err("A Word package part name is invalid".into());
    }
    Ok(Book::resolve("package", path)?.to_ascii_lowercase())
}

fn target_part(source: &str, path: &str) -> Result<String> {
    if path.is_empty() || path.len() > MAX_PATH || path.contains(['#', '?']) {
        return Err("A Word package relationship target is invalid".into());
    }
    if let Some(absolute) = path.strip_prefix('/') {
        return part_name(absolute);
    }
    let path = Book::resolve(source, path)
        .map_err(|_| "A Word package relationship points outside its archive or is invalid")?;
    Ok(path.to_ascii_lowercase())
}

fn read_xml_part<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    parts: &BTreeMap<String, usize>,
    path: &str,
    limit: usize,
) -> Result<String> {
    let index = *parts
        .get(path)
        .ok_or_else(|| format!("The Word package part is missing: {path}"))?;
    let entry = archive.by_index(index)?;
    if entry.size() > limit as u64
        || !matches!(
            entry.compression(),
            CompressionMethod::Stored | CompressionMethod::Deflated
        )
    {
        return Err(format!(
            "The Word package part exceeds limits or uses unsupported compression: {path}"
        )
        .into());
    }
    let mut bytes = Vec::new();
    entry.take(limit as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err("A Word XML package part exceeds its size limit".into());
    }
    let text = if let Some((encoding, bom)) = Encoding::for_bom(&bytes) {
        encoding
            .decode_without_bom_handling_and_without_replacement(&bytes[bom..])
            .ok_or("Invalid encoding in Word XML")?
            .into_owned()
    } else {
        String::from_utf8(bytes)?
    };
    if text.len() > limit || text.contains("<!DOCTYPE") {
        return Err(
            "Word XML exceeds its size limit or contains a document type declaration".into(),
        );
    }
    Ok(text)
}

fn xml(text: &str) -> Result<Xml<'_>> {
    let document = Xml::parse_with_options(
        text,
        ParsingOptions {
            nodes_limit: MAX_XML_NODES,
            ..Default::default()
        },
    )?;
    if document
        .descendants()
        .any(|node| node.ancestors().take(MAX_XML_DEPTH + 1).count() > MAX_XML_DEPTH)
    {
        return Err("The Word document exceeds the XML nesting limit".into());
    }
    Ok(document)
}

#[derive(Default)]
struct ContentTypes {
    overrides: BTreeMap<String, String>,
    defaults: BTreeMap<String, String>,
}

impl ContentTypes {
    fn for_part(&self, name: &str) -> Option<&str> {
        self.overrides
            .get(name)
            .or_else(|| {
                let (_, extension) = name.rsplit('/').next()?.rsplit_once('.')?;
                self.defaults.get(extension)
            })
            .map(String::as_str)
    }
}

fn content_types(text: &str) -> Result<ContentTypes> {
    let document = xml(text)?;
    let root = document.root_element();
    if !root.has_tag_name((TYPES_NS, "Types")) {
        return Err("The Word package content types are invalid".into());
    }
    let mut types = ContentTypes::default();
    for entry in root.children().filter(Node::is_element) {
        let content = entry
            .attribute("ContentType")
            .ok_or("A Word package part has no content type")?;
        let lowercase = content.to_ascii_lowercase();
        if [
            "macroenabled",
            "vbaproject",
            "vbadata",
            "activex",
            "oleobject",
            "javascript",
            "ecmascript",
        ]
        .iter()
        .any(|term| lowercase.contains(term))
        {
            return Err(
                "The Word document contains macros, scripts or embedded executable content".into(),
            );
        }
        if entry.has_tag_name((TYPES_NS, "Override")) {
            let path = entry
                .attribute("PartName")
                .and_then(|name| name.strip_prefix('/'))
                .ok_or("Invalid Word content type part name")?;
            if types
                .overrides
                .insert(part_name(path)?, content.to_owned())
                .is_some()
            {
                return Err("The Word package has duplicate content types for a part".into());
            }
        } else if entry.has_tag_name((TYPES_NS, "Default")) {
            let extension = entry
                .attribute("Extension")
                .filter(|name| {
                    !name.is_empty()
                        && name.len() <= 64
                        && name.bytes().all(|byte| byte.is_ascii_alphanumeric())
                })
                .ok_or("A Word package default content type has an invalid extension")?;
            if types
                .defaults
                .insert(extension.to_ascii_lowercase(), content.to_owned())
                .is_some()
            {
                return Err("The Word package has duplicate default content types".into());
            }
        } else {
            return Err("The Word package content types are invalid".into());
        }
    }
    Ok(types)
}

#[derive(Default)]
struct Relationships {
    main: Option<String>,
    core: Option<String>,
    styles: Option<String>,
}

fn relationships_name(source: &str) -> String {
    let (parent, filename) = source
        .rsplit_once('/')
        .map_or(("", source), |(parent, filename)| (parent, filename));
    if parent.is_empty() {
        format!("_rels/{filename}.rels")
    } else {
        format!("{parent}/_rels/{filename}.rels")
    }
}

fn relationship_source(path: &str) -> Option<String> {
    if path == PACKAGE_RELS {
        return Some("package".into());
    }
    let (parent, name) = path.rsplit_once('/')?;
    let basename = name.strip_suffix(".rels")?;
    if parent == "_rels" {
        Some(basename.to_owned())
    } else {
        Some(format!("{}/{basename}", parent.strip_suffix("/_rels")?))
    }
}

fn read_relationships<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    parts: &BTreeMap<String, usize>,
) -> Result<BTreeMap<String, Relationships>> {
    let mut relationships = BTreeMap::new();
    let mut remaining = MAX_RELATIONS_BYTES;
    for path in parts.keys().filter(|name| name.ends_with(".rels")) {
        let source =
            relationship_source(path).ok_or("The Word package relationship path is invalid")?;
        let text = read_xml_part(archive, parts, path, remaining.min(MAX_OTHER_XML))?;
        remaining -= text.len();
        relationships.insert(
            path.clone(),
            parse_relationships(&text, &source, path == PACKAGE_RELS)?,
        );
    }
    Ok(relationships)
}

fn parse_relationships(text: &str, source: &str, package: bool) -> Result<Relationships> {
    let document = xml(text)?;
    let root = document.root_element();
    if !root.has_tag_name((RELS_NS, "Relationships")) {
        return Err("The Word package relationships are invalid".into());
    }
    let mut found = Relationships::default();
    let mut ids = BTreeSet::new();
    for entry in root.children().filter(Node::is_element) {
        if !entry.has_tag_name((RELS_NS, "Relationship")) {
            return Err("The Word package relationships are invalid".into());
        }
        let id = entry
            .attribute("Id")
            .filter(|id| !id.is_empty())
            .ok_or("A Word relationship has no ID")?;
        if !ids.insert(id) {
            return Err("The Word package has duplicate relationship IDs".into());
        }
        let kind = entry
            .attribute("Type")
            .ok_or("A Word relationship has no type")?;
        let office = kind
            .strip_prefix(OFFICE_REL)
            .or_else(|| kind.strip_prefix(STRICT_OFFICE_REL));
        let role = kind
            .rsplit('/')
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        if matches!(
            role.as_str(),
            "vbaproject"
                | "vbadata"
                | "activexcontrol"
                | "control"
                | "oleobject"
                | "afchunk"
                | "subdocument"
        ) {
            return Err("The Word document contains macros, alternate documents or embedded executable content".into());
        }
        let target = entry
            .attribute("Target")
            .ok_or("A Word relationship has no target")?;
        match entry.attribute("TargetMode").unwrap_or("Internal") {
            "External" if office == Some("hyperlink") && !package => continue,
            "External" => {
                return Err("The Word document refers to an external package resource".into());
            }
            "Internal" => {}
            _ => return Err("The Word package relationship mode is invalid".into()),
        }
        let path = target_part(source, target)?;
        let field = match (package, office, kind) {
            (true, Some("officeDocument"), _) => &mut found.main,
            (true, _, CORE_REL) => &mut found.core,
            (false, Some("styles"), _) => &mut found.styles,
            _ => continue,
        };
        if field.replace(path).is_some() {
            return Err(
                "The Word package contains duplicate main, style or metadata relationships".into(),
            );
        }
    }
    Ok(found)
}

fn read_metadata(
    text: &str,
) -> Result<(
    Option<String>,
    Option<String>,
    crate::publication_metadata::Metadata,
)> {
    let document = xml(text)?;
    let root = document.root_element();
    if !root.has_tag_name((CORE_NS, "coreProperties")) {
        return Err("The Word document metadata is invalid".into());
    }
    let field = |name| {
        root.children()
            .find(|node| node.has_tag_name((DC_NS, name)))
            .and_then(|node| node.text())
            .and_then(label)
    };
    let raw_field = |ns, name| {
        root.children()
            .find(|node| node.has_tag_name((ns, name)))
            .and_then(|node| node.text())
            .map(str::to_owned)
    };
    Ok((
        field("title"),
        field("creator"),
        crate::publication_metadata::Metadata {
            language: raw_field(DC_NS, "language"),
            description: raw_field(DC_NS, "description"),
            keywords: raw_field(CORE_NS, "keywords").into_iter().collect(),
            series: Vec::new(),
        },
    ))
}

fn label(text: &str) -> Option<String> {
    let mut result = String::new();
    for word in text.split_whitespace() {
        if !result.is_empty() && result.len() < MAX_LABEL {
            result.push(' ');
        }
        let available = MAX_LABEL - result.len();
        result.push_str(&word[..word.floor_char_boundary(available)]);
        if word.len() >= available {
            break;
        }
    }
    (!result.trim().is_empty()).then(|| result.trim_end().to_owned())
}

#[derive(Default)]
struct Style {
    outline: Option<u8>,
    parent: Option<String>,
}

type Styles = BTreeMap<String, Style>;

fn read_styles(text: &str) -> Result<Styles> {
    let document = xml(text)?;
    let root = document.root_element();
    if !word(root, "styles") {
        return Err("The Word document styles are invalid".into());
    }
    let mut styles = BTreeMap::new();
    for item in root
        .children()
        .filter(|node| word(*node, "style") && value(*node, "type") == Some("paragraph"))
    {
        let Some(id) = value(item, "styleId").filter(|id| !id.is_empty() && id.len() <= 128) else {
            continue;
        };
        let outline = child(item, "pPr").and_then(outline).or_else(|| {
            child(item, "name")
                .and_then(|name| value(name, "val"))
                .and_then(builtin_heading)
        });
        let parent = child(item, "basedOn")
            .and_then(|node| value(node, "val"))
            .map(str::to_owned);
        if styles.len() == MAX_STYLES
            || styles
                .insert(id.to_owned(), Style { outline, parent })
                .is_some()
        {
            return Err("The Word document styles exceed limits or contain duplicate IDs".into());
        }
    }
    Ok(styles)
}

fn word(node: Node<'_, '_>, name: &str) -> bool {
    node.has_tag_name((WORD_NS, name)) || node.has_tag_name((STRICT_WORD_NS, name))
}

fn child<'a, 'input>(node: Node<'a, 'input>, name: &str) -> Option<Node<'a, 'input>> {
    node.children().find(|node| word(*node, name))
}

fn value<'a>(node: Node<'a, '_>, name: &str) -> Option<&'a str> {
    node.attribute((WORD_NS, name))
        .or_else(|| node.attribute((STRICT_WORD_NS, name)))
}

fn outline(properties: Node<'_, '_>) -> Option<u8> {
    child(properties, "outlineLvl")
        .and_then(|node| value(node, "val"))?
        .parse()
        .ok()
        .filter(|depth| *depth <= 9)
}

fn builtin_heading(style: &str) -> Option<u8> {
    let normalized = style.to_ascii_lowercase();
    normalized
        .strip_prefix("heading")?
        .trim()
        .parse::<u8>()
        .ok()
        .filter(|level| (1..=9).contains(level))
        .map(|level| level - 1)
}

fn heading(properties: Node<'_, '_>, styles: &Styles) -> Option<u8> {
    if let Some(level) = outline(properties) {
        return (level < 9).then_some(level);
    }
    let mut style = child(properties, "pStyle").and_then(|node| value(node, "val"));
    for _ in 0..8 {
        let id = style?;
        if let Some(entry) = styles.get(id) {
            if let Some(level) = entry.outline {
                return (level < 9).then_some(level);
            }
            style = entry.parent.as_deref();
        } else {
            return builtin_heading(id);
        }
        if let Some(level) = builtin_heading(id) {
            return Some(level);
        }
    }
    None
}

#[derive(Default)]
struct Output {
    xhtml: String,
    toc: Vec<TocEntry>,
    headings: usize,
    has_text: bool,
}

impl fmt::Write for Output {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        if text.len() > MAX_OUTPUT - self.xhtml.len() {
            return Err(fmt::Error);
        }
        self.xhtml.push_str(text);
        Ok(())
    }
}

fn convert_main(text: &str, styles: &Styles) -> Result<Output> {
    let document = xml(text)?;
    let root = document.root_element();
    if !word(root, "document") {
        return Err("The Word main part has an invalid WordprocessingML root".into());
    }
    if root.descendants().filter(Node::is_element).any(|node| {
        node.tag_name().name().eq_ignore_ascii_case("script")
            || ["altChunk", "object", "control", "subDoc"]
                .iter()
                .any(|name| word(node, name))
    }) {
        return Err("The Word document contains scripts or unsupported embedded objects".into());
    }
    let mut bodies = root.children().filter(|node| word(*node, "body"));
    let body = bodies.next().ok_or("The Word document has no main body")?;
    if bodies.next().is_some() {
        return Err("The Word document has more than one main body".into());
    }
    let mut output = Output::default();
    (|| -> fmt::Result {
        output.write_str("<html xmlns=\"http://www.w3.org/1999/xhtml\"><head/><body>")?;
        output.blocks(body, styles, false)?;
        output.write_str("</body></html>")
    })()
    .map_err(|_| "The Word document exceeds Reader's reflow output limit")?;
    if !output.has_text {
        return Err("The Word document has no supported reading text".into());
    }
    Ok(output)
}

impl Output {
    fn blocks(&mut self, parent: Node<'_, '_>, styles: &Styles, in_table: bool) -> fmt::Result {
        for node in parent.children().filter(Node::is_element) {
            if word(node, "p") {
                self.paragraph(node, styles, in_table)?;
            } else if word(node, "tbl") {
                self.table(node, styles)?;
            } else if word(node, "sdt") {
                if let Some(content) = child(node, "sdtContent") {
                    self.blocks(content, styles, in_table)?;
                }
            } else if transparent(node) {
                self.blocks(node, styles, in_table)?;
            }
        }
        Ok(())
    }

    fn paragraph(&mut self, node: Node<'_, '_>, styles: &Styles, in_table: bool) -> fmt::Result {
        let style = child(node, "pPr").and_then(|properties| heading(properties, styles));
        let depth = style.filter(|_| !in_table && self.headings < MAX_HEADINGS);
        let tag = depth.map_or_else(|| "p".into(), |depth| format!("h{}", (depth + 1).min(6)));
        write!(self, "<{tag}")?;
        if depth.is_some() {
            self.headings += 1;
            let number = self.headings;
            write!(self, " id=\"word-heading-{number}\"")?;
        }
        self.write_char('>')?;
        let mut heading_text = String::new();
        if depth.is_some() {
            self.inline(node, Some(&mut heading_text))?;
        } else {
            self.inline(node, None)?;
        }
        write!(self, "</{tag}>")?;
        if let Some((depth, label)) = depth.zip(label(&heading_text)) {
            self.toc.push(TocEntry {
                label,
                depth: usize::from(depth),
                destination: Some(Destination {
                    path: CHAPTER_PATH.into(),
                    fragment: Some(format!("word-heading-{}", self.headings)),
                }),
            });
        }
        Ok(())
    }

    fn table(&mut self, table: Node<'_, '_>, styles: &Styles) -> fmt::Result {
        self.write_str("<table><tbody>")?;
        for row in table.children().filter(|node| word(*node, "tr")) {
            self.write_str("<tr>")?;
            let header = child(row, "trPr").is_some_and(|properties| flag(properties, "tblHeader"));
            let tag = if header { "th" } else { "td" };
            for cell in row.children().filter(|node| word(*node, "tc")) {
                write!(self, "<{tag}>")?;
                self.blocks(cell, styles, true)?;
                write!(self, "</{tag}>")?;
            }
            self.write_str("</tr>")?;
        }
        self.write_str("</tbody></table>")
    }

    fn inline(&mut self, parent: Node<'_, '_>, mut heading: Option<&mut String>) -> fmt::Result {
        for node in parent.children().filter(Node::is_element) {
            if word(node, "r") {
                self.run(node, heading.as_deref_mut())?;
            } else if word(node, "sdt") {
                if let Some(content) = child(node, "sdtContent") {
                    self.inline(content, heading.as_deref_mut())?;
                }
            } else if word(node, "hyperlink") || transparent(node) {
                self.inline(node, heading.as_deref_mut())?;
            }
        }
        Ok(())
    }

    fn run(&mut self, run: Node<'_, '_>, mut heading: Option<&mut String>) -> fmt::Result {
        let prop = child(run, "rPr");
        let style = prop
            .and_then(|node| child(node, "rStyle"))
            .and_then(|node| value(node, "val"));
        let enabled = |prop_name, style_name| {
            prop.and_then(|node| child(node, prop_name)).map_or_else(
                || style.is_some_and(|style| style.eq_ignore_ascii_case(style_name)),
                on,
            )
        };
        let bold = enabled("b", "Strong");
        let italic = enabled("i", "Emphasis");
        if prop.is_some_and(|prop| flag(prop, "vanish")) {
            return Ok(());
        }
        if bold {
            self.write_str("<strong>")?;
        }
        if italic {
            self.write_str("<em>")?;
        }
        for item in run.children().filter(Node::is_element) {
            if word(item, "t") {
                for text in item
                    .children()
                    .filter(Node::is_text)
                    .filter_map(|node| node.text())
                {
                    self.has_text |= !text.trim().is_empty();
                    self.escape(text)?;
                    append_label(&mut heading, text);
                }
            } else if word(item, "br") || word(item, "cr") {
                self.write_str("<br/>")?;
                append_label(&mut heading, " ");
            } else if word(item, "tab") {
                self.write_char(' ')?;
                append_label(&mut heading, " ");
            } else if word(item, "noBreakHyphen") {
                self.write_char('\u{2011}')?;
                append_label(&mut heading, "\u{2011}");
            } else if word(item, "softHyphen") {
                self.write_char('\u{00ad}')?;
            }
        }
        if italic {
            self.write_str("</em>")?;
        }
        if bold {
            self.write_str("</strong>")?;
        }
        Ok(())
    }

    fn escape(&mut self, text: &str) -> fmt::Result {
        let mut from = 0;
        for (index, character) in text.char_indices() {
            let entity = match character {
                '&' => "&amp;",
                '<' => "&lt;",
                '>' => "&gt;",
                _ => continue,
            };
            self.write_str(&text[from..index])?;
            self.write_str(entity)?;
            from = index + 1;
        }
        self.write_str(&text[from..])
    }
}

fn transparent(node: Node<'_, '_>) -> bool {
    ["ins", "moveTo", "customXml", "smartTag", "sdtContent"]
        .into_iter()
        .any(|name| word(node, name))
}

fn flag(parent: Node<'_, '_>, name: &str) -> bool {
    child(parent, name).is_some_and(on)
}

fn on(node: Node<'_, '_>) -> bool {
    !matches!(value(node, "val"), Some("0" | "false" | "off"))
}

fn append_label(heading: &mut Option<&mut String>, text: &str) {
    if let Some(heading) = heading {
        let remaining = MAX_LABEL - heading.len();
        heading.push_str(&text[..text.floor_char_boundary(remaining)]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, OpenOptions};
    use std::io::{Cursor, Write as _};
    use std::path::PathBuf;

    fn package(parts: &[(&str, &str)]) -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated);
        for (path, text) in parts {
            zip.start_file(*path, options).unwrap();
            zip.write_all(text.as_bytes()).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }

    fn relations(entries: &str) -> String {
        format!(r#"<Relationships xmlns="{RELS_NS}">{entries}</Relationships>"#)
    }

    fn types(entries: &str) -> String {
        format!(r#"<Types xmlns="{TYPES_NS}">{entries}</Types>"#)
    }

    fn main(namespace: &str, body: &str) -> String {
        format!(r#"<w:document xmlns:w="{namespace}"><w:body>{body}</w:body></w:document>"#)
    }

    fn minimum(main_rels: &str, content: &str, others: &[(&str, &str)]) -> Vec<u8> {
        let types = types(&format!(
            r#"<Override PartName="/word/document.xml" ContentType="{MAIN_TYPE}"/>"#
        ));
        let mut entries = vec![
            ("[Content_Types].xml", types.as_str()),
            (PACKAGE_RELS, main_rels),
            ("word/document.xml", content),
        ];
        entries.extend_from_slice(others);
        package(&entries)
    }

    fn load(bytes: &[u8]) -> Result<Document> {
        open(Cursor::new(bytes), bytes.len() as u64, "Draft notes")
    }

    fn rejection(bytes: &[u8]) -> String {
        load(bytes)
            .err()
            .expect("The Word package should be rejected")
            .to_string()
    }

    struct TempDocx(PathBuf);

    impl Drop for TempDocx {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }

    #[test]
    fn opens_native_docx_through_book_with_toc_metadata_emphasis_and_cells() {
        let type_xml = types(&format!(
            r#"<Override PartName="/Part/Notes.xml" ContentType="{MAIN_TYPE}"/>"#
        ));
        let package_rels = relations(&format!(
            r#"
            <Relationship Id="body" Type="{OFFICE_REL}officeDocument" Target="/Part/Notes.xml"/>
            <Relationship Id="properties" Type="{CORE_REL}" Target="docProps/core.xml"/>
        "#
        ));
        let main_rels = relations(&format!(
            r#"
            <Relationship Id="styles" Type="{OFFICE_REL}styles" Target="../styles/Reading.xml"/>
            <Relationship Id="link" Type="{OFFICE_REL}hyperlink" Target="https://example.invalid/" TargetMode="External"/>
        "#
        ));
        let styles = format!(
            r#"<w:styles xmlns:w="{WORD_NS}">
             <w:style w:type="paragraph" w:styleId="CustomSection"><w:basedOn w:val="BaseSection"/></w:style>
             <w:style w:type="paragraph" w:styleId="BaseSection"><w:pPr><w:outlineLvl w:val="1"/></w:pPr></w:style>
        </w:styles>"#
        );
        let core = format!(
            r#"<cp:coreProperties xmlns:cp="{CORE_NS}" xmlns:dc="{DC_NS}">
            <dc:title> Tide &amp; Coast </dc:title><dc:creator>Ada  Seabrook</dc:creator>
        </cp:coreProperties>"#
        );
        let text = main(
            WORD_NS,
            r#"
            <w:p><w:pPr><w:pStyle w:val="CustomSection"/></w:pPr><w:r><w:t>Heading</w:t></w:r></w:p>
            <w:p><w:r><w:rPr><w:b/><w:i/></w:rPr><w:t>Read &lt;here&gt;</w:t></w:r>
                <w:r><w:tab/><w:t>next</w:t><w:br/><w:t>line</w:t><w:instrText>HYPERLINK secret</w:instrText></w:r>
                <w:hyperlink><w:r><w:t> Visible citation</w:t></w:r></w:hyperlink>
            </w:p>
            <w:tbl>
                <w:tr><w:trPr><w:tblHeader/></w:trPr><w:tc><w:p><w:r><w:t>Name</w:t></w:r></w:p></w:tc>
                    <w:tc><w:p><w:r><w:t>Value</w:t></w:r></w:p></w:tc></w:tr>
                <w:tr><w:tc><w:p><w:r><w:t>Depth</w:t></w:r></w:p></w:tc>
                    <w:tc><w:p><w:r><w:t>42 metres</w:t></w:r></w:p></w:tc></w:tr>
            </w:tbl>
        "#,
        );
        let bytes = package(&[
            ("[Content_Types].xml", &type_xml),
            (PACKAGE_RELS, &package_rels),
            ("Part/Notes.xml", &text),
            ("Part/_rels/Notes.xml.rels", &main_rels),
            ("styles/Reading.xml", &styles),
            ("docProps/core.xml", &core),
        ]);
        let path =
            std::env::temp_dir().join(format!("reader-word-test-{}.docx", std::process::id()));
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)
            .unwrap();
        let fixture = TempDocx(path);
        file.write_all(&bytes).unwrap();
        drop(file);

        let mut book = Book::open(&fixture.0).unwrap();
        assert_eq!(book.title, "Tide & Coast");
        assert_eq!(book.author.as_deref(), Some("Ada Seabrook"));
        assert_eq!(book.toc.len(), 1);
        assert_eq!((&*book.toc[0].label, book.toc[0].depth), ("Heading", 1));
        let xhtml = book.chapter(0).unwrap();
        assert!(xhtml.contains("<strong><em>Read &lt;here&gt;</em></strong>"));
        assert!(xhtml.contains("<table><tbody><tr><th><p>Name</p></th><th><p>Value</p></th></tr>"));
        assert!(xhtml.contains("<p>42 metres</p>"));
        assert!(!xhtml.contains("example.invalid"));
        assert!(!xhtml.contains("HYPERLINK secret"));
        let source = crate::source::Source::parse(&xhtml).unwrap();
        let labels: Vec<_> = source
            .blocks
            .iter()
            .map(|block| block.text.as_str())
            .collect();
        assert_eq!(
            labels,
            [
                "Heading",
                "Read <here> next\nline Visible citation",
                "Name",
                "Value",
                "Depth",
                "42 metres"
            ]
        );
        let destination = book.toc[0].destination.as_ref().unwrap();
        assert_eq!(destination.path, CHAPTER_PATH);
        assert!(
            source
                .fragments
                .contains_key(destination.fragment.as_deref().unwrap())
        );
    }

    #[test]
    fn accepts_strict_word_xml_and_default_main_content_type() {
        let relationships = relations(&format!(
            r#"<Relationship Id="body" Type="{STRICT_OFFICE_REL}officeDocument" Target="one.xml"/>"#
        ));
        let types = types(&format!(
            r#"<Default Extension="XML" ContentType="{MAIN_TYPE}"/>"#
        ));
        let content = main(STRICT_WORD_NS, "<w:p><w:r><w:t>Packet</w:t></w:r></w:p>");
        let input = package(&[
            ("[Content_Types].xml", &types),
            (PACKAGE_RELS, &relationships),
            ("one.xml", &content),
        ]);
        let document = load(&input).unwrap();
        assert_eq!(document.title, "Draft notes");
        assert!(document.xhtml.contains("<p>Packet</p>"));
    }

    #[test]
    fn rejects_duplicate_main_parts_macros_external_resources_and_xml_declarations() {
        let root = format!(
            r#"<Relationship Id="body" Type="{OFFICE_REL}officeDocument" Target="word/document.xml"/>"#
        );
        let good = relations(&root);
        let text = main(WORD_NS, "<w:p><w:r><w:t>A paragraph</w:t></w:r></w:p>");
        let duplicate = relations(&format!(
            r#"{root}<Relationship Id="other" Type="{OFFICE_REL}officeDocument" Target="word/second.xml"/>"#
        ));
        assert!(rejection(&minimum(&duplicate, &text, &[])).contains("duplicate main"));
        assert!(
            rejection(&minimum(&good, &text, &[("WORD/DOCUMENT.XML", &text)]))
                .contains("duplicate package")
        );
        assert!(
            rejection(&minimum(&good, &text, &[("word/vbaProject.bin", "macro")]))
                .contains("macros")
        );
        let bad = relations(&format!(
            r#"<Relationship Id="body" Type="{OFFICE_REL}officeDocument" Target="../document.xml"/>"#
        ));
        assert!(rejection(&minimum(&bad, &text, &[])).contains("outside its archive"));
        let external = relations(&format!(
            r#"<Relationship Id="asset" Type="{OFFICE_REL}image" Target="https://example.invalid/image.png" TargetMode="External"/>"#
        ));
        assert!(
            rejection(&minimum(
                &good,
                &text,
                &[("word/_rels/document.xml.rels", &external)]
            ))
            .contains("external package resource")
        );
        let script = main(WORD_NS, "<script>ignored</script><w:p/>");
        assert!(rejection(&minimum(&good, &script, &[])).contains("scripts"));
        assert!(
            rejection(&minimum(
                &good,
                &format!("<!DOCTYPE w:document []>{text}"),
                &[]
            ))
            .contains("document type declaration")
        );
        assert!(rejection(&minimum(&good, "<w:document>", &[])).contains("namespace"));
    }
}
