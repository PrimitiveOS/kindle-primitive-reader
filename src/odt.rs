//! A bounded, offline OpenDocument Text reader. The native reflow pipeline
//! consumes a conservative XHTML representation of supported ODF elements.
use crate::Result;
use crate::epub::{Book, Destination, TocEntry, check_zip_directory, is_archive_path};
use crate::loose::{CHAPTER_PATH, Document};
use roxmltree::{Document as Xml, Node, ParsingOptions};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Seek};
use zip::{CompressionMethod, ZipArchive};

pub(crate) const MAX_INPUT: usize = 16 * 1024 * 1024;
const MAX_XML: usize = 4 * 1024 * 1024;
// The current reflow provider admits at most 512 KiB of chapter text. A bound
// on the generated XHTML is conservative but ensures its text fits that limit.
const MAX_OUTPUT: usize = 512 * 1024;
const MAX_IMAGE: u64 = 4 * 1024 * 1024;
const TEXT: &str = "urn:oasis:names:tc:opendocument:xmlns:text:1.0";
const OFFICE: &str = "urn:oasis:names:tc:opendocument:xmlns:office:1.0";
const TABLE: &str = "urn:oasis:names:tc:opendocument:xmlns:table:1.0";
const DRAW: &str = "urn:oasis:names:tc:opendocument:xmlns:drawing:1.0";
const STYLE: &str = "urn:oasis:names:tc:opendocument:xmlns:style:1.0";
const FO: &str = "urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0";
const XLINK: &str = "http://www.w3.org/1999/xlink";
const DC: &str = "http://purl.org/dc/elements/1.1/";
const META: &str = "urn:oasis:names:tc:opendocument:xmlns:meta:1.0";

pub(crate) fn open(mut input: impl Read + Seek, len: u64, fallback: &str) -> Result<Document> {
    if len > MAX_INPUT as u64 {
        return Err("The OpenDocument file exceeds the 16 MiB input limit".into());
    }
    let count =
        check_zip_directory(&mut input, len).map_err(|_| "Invalid OpenDocument ZIP directory")?;
    if count > 512 {
        return Err("Too many OpenDocument package entries".into());
    }
    let mut zip = ZipArchive::new(input)?;
    if zip.len() != count {
        return Err("Ambiguous OpenDocument package entries".into());
    }
    let mut parts = BTreeMap::new();
    for index in 0..zip.len() {
        let entry = zip.by_index_raw(index)?;
        let name = std::str::from_utf8(entry.name_raw())?.trim_end_matches('/');
        if name.len() > 512 || !is_archive_path(name) || entry.encrypted() || entry.is_symlink() {
            return Err("Invalid or encrypted OpenDocument package entry".into());
        }
        if !entry.is_dir() && parts.insert(name.to_owned(), index).is_some() {
            return Err("Duplicate OpenDocument package entry".into());
        }
    }
    let mime = read(&mut zip, &parts, "mimetype", 128)?;
    if mime != b"application/vnd.oasis.opendocument.text" {
        return Err("This is not an OpenDocument Text document".into());
    }
    let content = String::from_utf8(read(&mut zip, &parts, "content.xml", MAX_XML)?)?;
    let xml = parse_xml(&content)?;
    let body = xml
        .descendants()
        .find(|n| n.has_tag_name((OFFICE, "text")))
        .ok_or("The OpenDocument file has no text body")?;
    let mut styles = BTreeMap::new();
    let mut numbered_lists = BTreeSet::new();
    if parts.contains_key("styles.xml") {
        let source = String::from_utf8(read(&mut zip, &parts, "styles.xml", MAX_XML)?)?;
        let style_xml = parse_xml(&source)?;
        collect_styles(&style_xml, &mut styles)?;
        collect_lists(&style_xml, &mut numbered_lists)?;
    }
    collect_styles(&xml, &mut styles)?;
    collect_lists(&xml, &mut numbered_lists)?;
    let mut out = Convert {
        xhtml: String::from("<html xmlns=\"http://www.w3.org/1999/xhtml\"><head/><body>"),
        toc: Vec::new(),
        headings: 0,
        image_paths: BTreeMap::new(),
        styles,
        numbered_lists,
        used_ids: body
            .descendants()
            .filter(|n| {
                n.has_tag_name((TEXT, "bookmark")) || n.has_tag_name((TEXT, "bookmark-start"))
            })
            .filter_map(|n| n.attribute((TEXT, "name")))
            .filter(|s| s.len() <= 512)
            .map(str::to_owned)
            .collect(),
        next_heading_id: 1,
    };
    for child in body.children() {
        out.node(child, 0)?;
    }
    out.push("</body></html>")?;
    let mut resources = BTreeMap::new();
    let mut image_total = 0u64;
    for (path, local) in out.image_paths {
        let Some(&index) = parts.get(&path) else {
            continue;
        };
        if resources.len() >= 128 {
            return Err("Too many OpenDocument images".into());
        }
        let size = zip.by_index(index)?.size();
        image_total = image_total
            .checked_add(size)
            .ok_or("OpenDocument image size overflow")?;
        if size > MAX_IMAGE || image_total > 16 * 1024 * 1024 {
            return Err("OpenDocument images exceed size limits".into());
        }
        let bytes = read(&mut zip, &parts, &path, MAX_IMAGE as usize)?;
        if matches!(
            image::guess_format(&bytes),
            Ok(image::ImageFormat::Png | image::ImageFormat::Jpeg)
        ) {
            resources.insert(local, bytes);
        }
    }
    let mut title = None;
    let mut author = None;
    let mut metadata = crate::publication_metadata::Metadata::default();
    if parts.contains_key("meta.xml") {
        let source = String::from_utf8(read(&mut zip, &parts, "meta.xml", 256 * 1024)?)?;
        let meta = parse_xml(&source)?;
        let field = |name| {
            meta.descendants()
                .find(|n| n.has_tag_name((DC, name)))
                .and_then(|n| n.text())
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(str::to_owned)
        };
        title = field("title");
        author = field("creator").or_else(|| {
            meta.descendants()
                .find(|n| n.has_tag_name((META, "initial-creator")))
                .and_then(|n| n.text())
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(str::to_owned)
        });
        metadata.language = field("language");
        metadata.description = field("description");
    }
    Ok(Document {
        metadata,
        title: title.unwrap_or_else(|| fallback.into()),
        author,
        cover: None,
        chapter_path: CHAPTER_PATH.into(),
        xhtml: out.xhtml,
        toc: out.toc,
        resources,
        fb2_note_sections: Default::default(),
    })
}
#[derive(Default)]
struct Style {
    parent: Option<String>,
    bold: Option<bool>,
    italic: Option<bool>,
}

fn collect_lists(xml: &Xml<'_>, lists: &mut BTreeSet<(String, usize)>) -> Result<()> {
    for style in xml
        .descendants()
        .filter(|n| n.has_tag_name((TEXT, "list-style")))
    {
        let Some(name) = style
            .attribute((STYLE, "name"))
            .filter(|s| !s.is_empty() && s.len() <= 256)
        else {
            continue;
        };
        for item in style
            .children()
            .filter(|n| n.has_tag_name((TEXT, "list-level-style-number")))
        {
            let level = item
                .attribute((TEXT, "level"))
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(1);
            if level <= 48 {
                lists.insert((name.to_owned(), level));
            }
            if lists.len() > 4096 {
                return Err("Too many OpenDocument list styles".into());
            }
        }
    }
    Ok(())
}

fn collect_styles(xml: &Xml<'_>, styles: &mut BTreeMap<String, Style>) -> Result<()> {
    for node in xml
        .descendants()
        .filter(|n| n.has_tag_name((STYLE, "style")))
    {
        let Some(name) = node
            .attribute((STYLE, "name"))
            .filter(|s| !s.is_empty() && s.len() <= 256)
        else {
            continue;
        };
        if styles.len() >= 4096 && !styles.contains_key(name) {
            return Err("Too many OpenDocument styles".into());
        }
        let props = node
            .children()
            .find(|n| n.has_tag_name((STYLE, "text-properties")));
        let bold = props
            .and_then(|n| n.attribute((FO, "font-weight")))
            .map(|v| v == "bold" || v.parse::<u16>().is_ok_and(|n| n >= 600));
        let italic = props
            .and_then(|n| n.attribute((FO, "font-style")))
            .map(|v| matches!(v, "italic" | "oblique"));
        styles.insert(
            name.into(),
            Style {
                parent: node
                    .attribute((STYLE, "parent-style-name"))
                    .filter(|s| s.len() <= 256)
                    .map(str::to_owned),
                bold,
                italic,
            },
        );
    }
    Ok(())
}

fn read<R: Read + Seek>(
    zip: &mut ZipArchive<R>,
    parts: &BTreeMap<String, usize>,
    name: &str,
    limit: usize,
) -> Result<Vec<u8>> {
    let index = *parts.get(name).ok_or("Missing OpenDocument package part")?;
    let file = zip.by_index(index)?;
    if file.size() > limit as u64
        || !matches!(
            file.compression(),
            CompressionMethod::Stored | CompressionMethod::Deflated
        )
    {
        return Err(
            "OpenDocument package part exceeds limits or uses unsupported compression".into(),
        );
    }
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err("OpenDocument package part exceeds limits".into());
    }
    Ok(bytes)
}
fn parse_xml(source: &str) -> Result<Xml<'_>> {
    if source.contains("<!DOCTYPE") {
        return Err("OpenDocument XML document types are unsupported".into());
    }
    let xml = Xml::parse_with_options(
        source,
        ParsingOptions {
            nodes_limit: 100_000,
            ..Default::default()
        },
    )?;
    if xml
        .descendants()
        .any(|node| node.ancestors().take(49).count() > 48)
    {
        return Err("OpenDocument XML nesting exceeds limits".into());
    }
    Ok(xml)
}
struct Convert {
    xhtml: String,
    toc: Vec<TocEntry>,
    headings: usize,
    image_paths: BTreeMap<String, String>,
    styles: BTreeMap<String, Style>,
    numbered_lists: BTreeSet<(String, usize)>,
    used_ids: BTreeSet<String>,
    next_heading_id: usize,
}
impl Convert {
    fn text_style(&self, name: &str) -> (Option<bool>, Option<bool>) {
        let (mut bold, mut italic) = (None, None);
        let mut next = Some(name);
        for _ in 0..16 {
            let Some(style) = next.and_then(|key| self.styles.get(key)) else {
                break;
            };
            bold = bold.or(style.bold);
            italic = italic.or(style.italic);
            next = style.parent.as_deref();
        }
        (bold, italic)
    }
    fn push(&mut self, value: &str) -> Result<()> {
        if self.xhtml.len().saturating_add(value.len()) > MAX_OUTPUT {
            return Err("OpenDocument converted text exceeds limits".into());
        }
        self.xhtml.push_str(value);
        Ok(())
    }
    fn escaped(&mut self, value: &str) -> Result<()> {
        for c in value.chars() {
            match c {
                '&' => self.push("&amp;")?,
                '<' => self.push("&lt;")?,
                '>' => self.push("&gt;")?,
                '"' => self.push("&quot;")?,
                _ => {
                    let mut buf = [0; 4];
                    self.push(c.encode_utf8(&mut buf))?;
                }
            }
        }
        Ok(())
    }
    fn node(&mut self, node: Node<'_, '_>, depth: usize) -> Result<()> {
        if depth > 48 {
            return Err("OpenDocument nesting exceeds limits".into());
        }
        if node.is_text() {
            return self.escaped(node.text().unwrap_or(""));
        }
        if !node.is_element() {
            return Ok(());
        }
        let name = node.tag_name();
        let ns = name.namespace().unwrap_or("");
        let tag = name.name();
        if ns == TEXT && tag == "h" {
            if self.headings >= 512 {
                return Err("Too many OpenDocument headings".into());
            }
            self.headings += 1;
            let id = loop {
                let candidate = format!("odt-heading-{}", self.next_heading_id);
                self.next_heading_id += 1;
                if self.used_ids.insert(candidate.clone()) {
                    break candidate;
                }
            };
            let outline_level = node
                .attribute((TEXT, "outline-level"))
                .and_then(|n| n.parse::<usize>().ok())
                .unwrap_or(1)
                .clamp(1, 48);
            let level = outline_level.min(6);
            self.push(&format!("<h{level} id=\"{id}\">"))?;
            for child in node.children() {
                self.node(child, depth + 1)?;
            }
            self.push(&format!("</h{level}>"))?;
            let label = node
                .descendants()
                .filter_map(|n| n.text().filter(|_| n.is_text()))
                .collect::<String>()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            if !label.is_empty() {
                self.toc.push(TocEntry {
                    label: label.chars().take(512).collect(),
                    depth: outline_level - 1,
                    destination: Some(Destination {
                        path: CHAPTER_PATH.into(),
                        fragment: Some(id),
                    }),
                });
            }
            return Ok(());
        }
        if ns == TEXT && tag == "s" {
            let n = node
                .attribute((TEXT, "c"))
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(1);
            if n == 0 || n > 4096 {
                return Err("OpenDocument whitespace exceeds limits".into());
            }
            for _ in 0..n {
                self.push("&#160;")?;
            }
            return Ok(());
        }
        if ns == TEXT && tag == "tab" {
            return self.push("&#160;&#160;&#160;&#160;");
        }
        if ns == TEXT && tag == "line-break" {
            return self.push("<br/>");
        }
        if ns == TEXT && matches!(tag, "bookmark" | "bookmark-start") {
            if let Some(id) = node
                .attribute((TEXT, "name"))
                .filter(|v| !v.is_empty() && v.len() <= 512 && !v.chars().any(char::is_control))
            {
                self.push("<span id=\"")?;
                self.escaped(id)?;
                self.push("\"></span>")?;
            }
            return Ok(());
        }
        if ns == TEXT && tag == "bookmark-end" {
            return Ok(());
        }
        if ns == TEXT && tag == "a" {
            let local = node.attribute((XLINK, "href")).filter(|v| {
                v.starts_with('#') && v.len() <= 513 && !v.chars().any(char::is_control)
            });
            if let Some(href) = local {
                self.push("<a href=\"")?;
                self.escaped(href)?;
                self.push("\">")?;
            }
            for child in node.children() {
                self.node(child, depth + 1)?;
            }
            if local.is_some() {
                self.push("</a>")?;
            }
            return Ok(());
        }
        if ns == DRAW && tag == "image" {
            if let Some(path) = node.attribute((XLINK, "href")) {
                if path.len() <= 512
                    && is_archive_path(path)
                    && !path.contains(['#', '?'])
                    && matches!(
                        path.rsplit('.')
                            .next()
                            .map(str::to_ascii_lowercase)
                            .as_deref(),
                        Some("png" | "jpg" | "jpeg")
                    )
                {
                    let Ok(path) = Book::resolve("package", path) else {
                        return Ok(());
                    };
                    if self.image_paths.len() >= 128 && !self.image_paths.contains_key(&path) {
                        return Err("Too many OpenDocument image references".into());
                    }
                    let local = format!(
                        "odt-images/{}.{}",
                        self.image_paths.len() + 1,
                        if path.to_ascii_lowercase().ends_with(".png") {
                            "png"
                        } else {
                            "jpg"
                        }
                    );
                    let local = self.image_paths.entry(path).or_insert(local).clone();
                    self.push("<img src=\"")?;
                    self.escaped(&local)?;
                    self.push("\"/>")?;
                }
            }
            return Ok(());
        }
        // Unknown elements keep their text in reading order; scripts and automatic generated indexes are ignored.
        if ns == OFFICE && matches!(tag, "scripts" | "forms") {
            return Ok(());
        }
        let numbered = if ns == TEXT && tag == "list" {
            let level = node
                .ancestors()
                .filter(|n| n.has_tag_name((TEXT, "list")))
                .count();
            node.ancestors()
                .filter(|n| n.has_tag_name((TEXT, "list")))
                .find_map(|n| n.attribute((TEXT, "style-name")))
                .is_some_and(|name| self.numbered_lists.contains(&(name.to_owned(), level)))
        } else {
            false
        };
        let wrapper = match (ns, tag) {
            (TEXT, "p") => Some("p"),
            (TEXT, "list") => Some(if numbered { "ol" } else { "ul" }),
            (TEXT, "list-item") => Some("li"),
            (TABLE, "table") => Some("table"),
            (TABLE, "table-row") => Some("tr"),
            (TABLE, "table-cell") => Some("td"),
            _ => None,
        };
        let (bold, italic) = if ns == TEXT && matches!(tag, "p" | "span") {
            node.attribute((TEXT, "style-name"))
                .map(|name| self.text_style(name))
                .unwrap_or_default()
        } else {
            (None, None)
        };
        let repetitions = match (ns, tag) {
            (TABLE, "table-row") => node.attribute((TABLE, "number-rows-repeated")),
            (TABLE, "table-cell") => node.attribute((TABLE, "number-columns-repeated")),
            _ => None,
        }
        .and_then(|n| n.parse::<usize>().ok())
        .unwrap_or(1);
        if repetitions == 0 || repetitions > 128 {
            return Err("OpenDocument table repetition exceeds limits".into());
        }
        for _ in 0..repetitions {
            if let Some(tag) = wrapper {
                if tag == "td" {
                    let colspan = node
                        .attribute((TABLE, "number-columns-spanned"))
                        .and_then(|v| v.parse::<usize>().ok())
                        .unwrap_or(1)
                        .clamp(1, 128);
                    let rowspan = node
                        .attribute((TABLE, "number-rows-spanned"))
                        .and_then(|v| v.parse::<usize>().ok())
                        .unwrap_or(1)
                        .clamp(1, 128);
                    self.push(&format!("<td colspan=\"{colspan}\" rowspan=\"{rowspan}\">"))?;
                } else {
                    self.push(&format!("<{tag}>"))?;
                }
            }
            if bold.is_some() || italic.is_some() {
                self.push("<span style=\"")?;
                if let Some(bold) = bold {
                    self.push(if bold {
                        "font-weight:bold;"
                    } else {
                        "font-weight:normal;"
                    })?;
                }
                if let Some(italic) = italic {
                    self.push(if italic {
                        "font-style:italic;"
                    } else {
                        "font-style:normal;"
                    })?;
                }
                self.push("\">")?;
            }
            for child in node.children() {
                self.node(child, depth + 1)?;
            }
            if bold.is_some() || italic.is_some() {
                self.push("</span>")?;
            }
            if let Some(tag) = wrapper {
                self.push(&format!("</{tag}>"))?;
            }
        }
        Ok(())
    }
}
