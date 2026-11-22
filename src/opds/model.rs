//! Bounded OPDS 1.2 Atom and OPDS 2 JSON reading catalogs.
use roxmltree::{Document, Node, ParsingOptions};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::Url;

pub(super) const MAX_FEED: usize = 2 * 1024 * 1024;
const MAX_ENTRIES: usize = 512;
const ACQUISITION: &str = "http://opds-spec.org/acquisition";

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(crate) struct Link {
    pub url: String,
    pub title: String,
    pub media_type: String,
    pub relation: String,
    pub indirect: bool,
}

impl Link {
    pub fn extension(&self) -> Option<&'static str> {
        if self.indirect
            || !matches!(
                self.relation.as_str(),
                "download" | ACQUISITION | "http://opds-spec.org/acquisition/open-access"
            )
        {
            return None;
        }
        match mime(&self.media_type) {
            "application/epub+zip" => Some("epub"),
            "application/pdf" => Some("pdf"),
            "application/x-fictionbook+xml" | "text/fb2+xml" => Some("fb2"),
            "application/x-mobipocket-ebook" => Some("mobi"),
            "application/vnd.amazon.mobi8-ebook" => Some("azw3"),
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document" => {
                Some("docx")
            }
            "application/x-cbz" | "application/vnd.comicbook+zip" => Some("cbz"),
            "application/x-cbt" | "application/vnd.comicbook+tar" => Some("cbt"),
            "application/x-cbr" | "application/vnd.comicbook-rar" => Some("cbr"),
            "image/vnd.djvu" | "image/x-djvu" => Some("djvu"),
            "text/plain" => Some("txt"),
            "text/html" => Some("html"),
            "application/xhtml+xml" => Some("xhtml"),
            "text/markdown" => Some("md"),
            "application/rtf" | "text/rtf" => Some("rtf"),
            "application/vnd.ms-htmlhelp" | "application/x-chm" => Some("chm"),
            "application/x-tcr" => Some("tcr"),
            "image/jp2" | "image/jpx" => Some("jp2"),
            "image/bmp" => Some("bmp"),
            "image/x-portable-anymap" | "image/x-portable-pixmap" => Some("pnm"),
            "image/jpeg" => Some("jpg"),
            "image/png" => Some("png"),
            "image/webp" => Some("webp"),
            "image/gif" => Some("gif"),
            "image/tiff" => Some("tiff"),
            "image/svg+xml" => Some("svg"),
            "" | "application/octet-stream" | "application/zip" => {
                let url = Url::parse(&self.url).ok()?;
                let name = url.path().rsplit('/').next()?.to_ascii_lowercase();
                [
                    "epub", "pdf", "fb2", "mobi", "azw3", "docx", "htmlz", "chm", "tcr", "cbz",
                    "cbt", "cbr", "djvu", "txt", "html", "xhtml", "md", "rtf", "jpg", "png", "gif",
                    "webp", "tiff", "svg", "bmp", "pnm", "jp2",
                ]
                .into_iter()
                .find(|ext| name.ends_with(&format!(".{ext}")))
            }
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(crate) struct Entry {
    pub title: String,
    pub author: String,
    pub description: String,
    pub navigation: Option<Link>,
    pub acquisitions: Vec<Link>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(crate) struct Feed {
    pub url: String,
    pub title: String,
    pub entries: Vec<Entry>,
    pub next: Option<Link>,
    pub previous: Option<Link>,
    pub start: Option<Link>,
    pub search: Option<Link>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) enum Parsed {
    Feed(Feed),
    Search(String),
}

pub(super) fn mime(value: &str) -> &str {
    value.split(';').next().unwrap_or("").trim()
}
fn catalog_type(value: &str) -> bool {
    matches!(
        mime(value),
        "application/atom+xml" | "application/opds+json" | "application/opds-publication+json"
    )
}

pub(crate) fn checked_url(value: &str) -> Result<Url, String> {
    if value.len() > 4096 || value.chars().any(char::is_control) {
        return Err("The catalog URL is invalid".into());
    }
    let mut url = Url::parse(value).map_err(|_| "Enter a complete HTTP or HTTPS URL")?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("Use an HTTP or HTTPS address and enter credentials separately".into());
    }
    url.set_fragment(None);
    Ok(url)
}

pub(crate) fn resolve(base: &str, href: &str) -> Result<String, String> {
    let base = checked_url(base)?;
    let mut masked = href.to_owned();
    let mut variables = Vec::new();
    while let Some(first) = masked.find('{') {
        let end = masked[first..]
            .find('}')
            .ok_or("The search template is incomplete")?
            + first;
        let mut token = format!("__primitive_opds_template_{}__", variables.len());
        while masked.contains(&token) || base.as_str().contains(&token) {
            token.push('_');
        }
        // A query-only URI template still addresses the current catalog path.
        // Preserve that separator while resolving the unexpanded relative URL.
        if masked[first..=end].starts_with("{?") {
            token.insert(0, '?');
        }
        variables.push((token.clone(), masked[first..=end].to_owned()));
        masked.replace_range(first..=end, &token);
        if variables.len() > 24 {
            return Err("The search template has too many variables".into());
        }
    }
    let mut result = checked_url(
        base.join(&masked)
            .map_err(|_| "The catalog link is invalid")?
            .as_str(),
    )?
    .to_string();
    for (token, variable) in variables {
        result = result.replace(&token, &variable);
    }
    Ok(result)
}

pub(crate) fn search_url(template: &str, query: &str) -> Result<String, String> {
    if query.trim().is_empty() || query.len() > 2048 {
        return Err("Enter a search of 1 to 2048 bytes".into());
    }
    let mut result = String::new();
    let mut rest = template;
    while let Some(first) = rest.find('{') {
        result.push_str(&rest[..first]);
        let end = rest[first..]
            .find('}')
            .ok_or("The search template is incomplete")?
            + first;
        let expression = &rest[first + 1..end];
        let operator = expression.chars().next().filter(|c| matches!(c, '?' | '&'));
        let variables = operator.map_or(expression, |_| &expression[1..]);
        let mut values = Vec::new();
        for variable in variables.split(',') {
            let optional = variable.ends_with('?');
            let name = variable.trim_end_matches(['?', '*']);
            let value = match name {
                "searchTerms" | "query" => query.trim(),
                "count" => "50",
                "startIndex" | "startPage" => "1",
                _ if optional || operator.is_some() => continue,
                _ => {
                    return Err(
                        "This search template needs a parameter this reader does not provide"
                            .into(),
                    );
                }
            };
            let encoded: String = value
                .bytes()
                .map(|b| {
                    if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                        (b as char).to_string()
                    } else {
                        format!("%{b:02X}")
                    }
                })
                .collect();
            values.push(if operator.is_some() {
                format!("{name}={encoded}")
            } else {
                encoded
            });
        }
        if let Some(op) = operator {
            if !values.is_empty() {
                result.push(op);
                result.push_str(&values.join("&"));
            }
        } else {
            result.push_str(&values.join(","));
        }
        rest = &rest[end + 1..];
    }
    result.push_str(rest);
    if result == template {
        return Err("The catalog did not provide a usable search template".into());
    }
    Ok(checked_url(&result)?.to_string())
}

fn html_text(value: &str, max: usize) -> String {
    let mut plain = String::new();
    let mut rest = value;
    while let Some(start) = rest.find('<') {
        plain.push_str(&rest[..start]);
        let tail = &rest[start + 1..];
        if tail.starts_with(|c: char| c.is_alphabetic() || c == '/' || c == '!')
            && let Some(end) = tail.find('>')
        {
            plain.push(' ');
            rest = &tail[end + 1..];
        } else {
            plain.push('<');
            rest = tail;
        }
    }
    plain.push_str(rest);
    let mut decoded = String::new();
    let mut rest = plain.as_str();
    while let Some(first) = rest.find('&') {
        decoded.push_str(&rest[..first]);
        let tail = &rest[first + 1..];
        if let Some(end) = tail.find(';').filter(|&end| end < 16) {
            let entity = &tail[..end];
            let character = match entity {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "nbsp" => Some(' '),
                _ => entity
                    .strip_prefix("#x")
                    .or_else(|| entity.strip_prefix("#X"))
                    .and_then(|s| u32::from_str_radix(s, 16).ok())
                    .or_else(|| entity.strip_prefix('#').and_then(|s| s.parse().ok()))
                    .and_then(char::from_u32),
            };
            if let Some(character) = character {
                decoded.push(character);
                rest = &tail[end + 1..];
                continue;
            }
        }
        decoded.push('&');
        rest = tail;
    }
    decoded.push_str(rest);
    text(&decoded, max)
}

fn text(value: &str, max: usize) -> String {
    value
        .chars()
        .filter(|c| !c.is_control() || c.is_whitespace())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(max)
        .collect()
}

fn node_text(node: Node<'_, '_>, max: usize) -> String {
    fn collect(node: Node<'_, '_>, out: &mut String) {
        if node.is_text() {
            out.push_str(node.text().unwrap_or(""));
            return;
        }
        let block = node.is_element()
            && matches!(
                node.tag_name().name(),
                "p" | "div" | "br" | "li" | "h1" | "h2" | "h3"
            );
        if block {
            out.push(' ');
        }
        for child in node.children() {
            collect(child, out);
        }
        if block {
            out.push(' ');
        }
    }
    let mut out = String::new();
    collect(node, &mut out);
    if node.attribute("type") == Some("html")
        || (node.attribute("type").is_none()
            && matches!(node.tag_name().name(), "content" | "summary")
            && node.children().all(|n| n.is_text()))
    {
        html_text(&out, max)
    } else {
        text(&out, max)
    }
}
fn child_text(node: Node<'_, '_>, name: &str, max: usize) -> String {
    node.children()
        .find(|n| n.is_element() && n.tag_name().name() == name)
        .map_or_else(String::new, |n| node_text(n, max))
}
fn xml_base(node: Node<'_, '_>, base: &str) -> Result<String, String> {
    let mut base = base.to_owned();
    for ancestor in node
        .ancestors()
        .filter(Node::is_element)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
    {
        if let Some(value) = ancestor.attribute(("http://www.w3.org/XML/1998/namespace", "base")) {
            base = resolve(&base, value)?;
        }
    }
    Ok(base)
}
fn xml_link(node: Node<'_, '_>, base: &str) -> Option<Link> {
    Some(Link {
        url: resolve(&xml_base(node, base).ok()?, node.attribute("href")?).ok()?,
        title: text(node.attribute("title").unwrap_or(""), 256),
        media_type: node.attribute("type").unwrap_or("").to_ascii_lowercase(),
        relation: node.attribute("rel").unwrap_or("alternate").to_owned(),
        indirect: node
            .children()
            .any(|n| n.is_element() && n.tag_name().name() == "indirectAcquisition"),
    })
}
fn assign(feed: &mut Feed, link: Link) {
    match link.relation.as_str() {
        "next" => feed.next = Some(link),
        "previous" | "prev" => feed.previous = Some(link),
        "start" => feed.start = Some(link),
        "search" => feed.search = Some(link),
        _ => (),
    }
}
fn add_entry(feed: &mut Feed, entry: Entry) -> Result<(), String> {
    let size = |entry: &Entry| {
        entry.title.len()
            + entry.author.len()
            + entry.description.len()
            + entry.navigation.as_ref().map_or(0, |l| l.url.len())
            + entry
                .acquisitions
                .iter()
                .map(|l| l.url.len() + l.title.len() + l.media_type.len() + l.relation.len())
                .sum::<usize>()
    };
    if feed.entries.iter().map(size).sum::<usize>() + size(&entry) > MAX_FEED * 2 {
        return Err("The catalog metadata exceeds this reader's memory limit; use a smaller server page size".into());
    }
    if feed.entries.len() >= MAX_ENTRIES {
        return Err(
            "The catalog page has more than 512 entries; use a smaller server page size".into(),
        );
    }
    feed.entries.push(entry);
    Ok(())
}
fn atom_entry(node: Node<'_, '_>, base: &str) -> Entry {
    let links: Vec<_> = node
        .children()
        .filter(|n| n.is_element() && n.tag_name().name() == "link")
        .take(32)
        .filter_map(|n| xml_link(n, base))
        .collect();
    Entry {
        title: {
            let title = child_text(node, "title", 512);
            if title.is_empty() {
                "Untitled publication".into()
            } else {
                title
            }
        },
        author: node
            .children()
            .filter(|n| n.is_element() && n.tag_name().name() == "author")
            .map(|n| child_text(n, "name", 256))
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(", "),
        description: {
            let content = child_text(node, "content", 4096);
            if content.is_empty() {
                child_text(node, "summary", 4096)
            } else {
                content
            }
        },
        navigation: links
            .iter()
            .find(|l| catalog_type(&l.media_type) && !l.relation.starts_with(ACQUISITION))
            .cloned(),
        acquisitions: links
            .into_iter()
            .filter(|l| l.relation.starts_with(ACQUISITION) || l.relation == "download")
            .collect(),
    }
}

pub(crate) fn parse(bytes: &[u8], base: &str) -> Result<Parsed, String> {
    if bytes.len() > MAX_FEED {
        return Err("The catalog response exceeds 2 MiB".into());
    }
    checked_url(base)?;
    let decoded;
    let source = if bytes.starts_with(&[0xff, 0xfe])
        || bytes.starts_with(&[0xfe, 0xff])
        || bytes.starts_with(&[b'<', 0, b'?', 0])
        || bytes.starts_with(&[0, b'<', 0, b'?'])
    {
        let encoding = if bytes[0] == 0xff || bytes[0] == b'<' {
            encoding_rs::UTF_16LE
        } else {
            encoding_rs::UTF_16BE
        };
        let skip = usize::from(bytes[0] == 0xff || bytes[0] == 0xfe) * 2;
        let (text, invalid) = encoding.decode_without_bom_handling(&bytes[skip..]);
        if invalid {
            return Err("The catalog UTF-16 is incomplete or malformed".into());
        }
        decoded = text.into_owned();
        decoded.as_str()
    } else {
        std::str::from_utf8(bytes)
            .map_err(|_| "Use a UTF-8 or UTF-16 XML catalog, or UTF-8 OPDS JSON")?
    };
    let source = source.trim_start_matches('\u{feff}').trim();
    if source.starts_with('{') {
        return json_feed(
            serde_json::from_str(source)
                .map_err(|_| "The catalog JSON is malformed or incomplete")?,
            base,
        );
    }
    let doc = Document::parse_with_options(
        source,
        ParsingOptions {
            nodes_limit: 100_000,
            ..Default::default()
        },
    )
    .map_err(|_| "The catalog XML is malformed, incomplete or contains a DTD")?;
    let root = doc.root_element();
    if root.tag_name().name() == "OpenSearchDescription" {
        let template = root
            .children()
            .find(|node| {
                node.is_element()
                    && node.tag_name().name() == "Url"
                    && catalog_type(node.attribute("type").unwrap_or(""))
            })
            .and_then(|node| {
                node.attribute("template")
                    .and_then(|href| resolve(&xml_base(node, base).ok()?, href).ok())
            })
            .ok_or("The search description has no supported result template")?;
        return Ok(Parsed::Search(template));
    }
    if !matches!(root.tag_name().name(), "feed" | "entry")
        || root.tag_name().namespace() != Some("http://www.w3.org/2005/Atom")
    {
        return Err("This address did not return an Atom OPDS catalog".into());
    }
    let mut feed = Feed {
        url: base.into(),
        title: child_text(root, "title", 512),
        ..Default::default()
    };
    if root.tag_name().name() == "entry" {
        add_entry(&mut feed, atom_entry(root, base))?;
    } else {
        for node in root.children().filter(Node::is_element) {
            match node.tag_name().name() {
                "link" => {
                    if let Some(link) = xml_link(node, base) {
                        if link.relation == "http://opds-spec.org/facet" {
                            add_entry(
                                &mut feed,
                                Entry {
                                    title: link.title.clone(),
                                    navigation: Some(link),
                                    ..Default::default()
                                },
                            )?;
                        } else {
                            assign(&mut feed, link);
                        }
                    }
                }
                "entry" => add_entry(&mut feed, atom_entry(node, base))?,
                _ => (),
            }
        }
    }
    if feed.title.is_empty() {
        feed.title = "Catalog".into();
    }
    Ok(Parsed::Feed(feed))
}
fn json_string(value: &Value, max: usize) -> String {
    value
        .as_str()
        .map(|s| text(s, max))
        .or_else(|| {
            value
                .as_object()?
                .values()
                .find_map(|v| v.as_str())
                .map(|s| text(s, max))
        })
        .unwrap_or_default()
}
fn json_links(value: &Value, base: &str) -> Vec<Link> {
    let Some(list) = value.as_array() else {
        return Vec::new();
    };
    list.iter()
        .take(64)
        .filter_map(|v| {
            let rels: Vec<_> = v["rel"]
                .as_str()
                .map(|s| vec![s])
                .or_else(|| {
                    v["rel"]
                        .as_array()
                        .map(|list| list.iter().filter_map(Value::as_str).collect())
                })
                .unwrap_or_else(|| vec!["alternate"]);
            let relation = rels
                .iter()
                .find(|r| r.starts_with(ACQUISITION) || **r == "download")
                .or_else(|| rels.first())?
                .to_string();
            Some(Link {
                url: resolve(base, v["href"].as_str()?).ok()?,
                title: json_string(&v["title"], 512),
                media_type: v["type"].as_str().unwrap_or("").to_ascii_lowercase(),
                relation,
                indirect: v["properties"]["indirectAcquisition"].is_array(),
            })
        })
        .collect()
}
fn authors(value: &Value) -> String {
    match value {
        Value::Array(list) => list
            .iter()
            .map(authors)
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(", "),
        Value::Object(object) if object.contains_key("name") => json_string(&object["name"], 512),
        _ => json_string(value, 512),
    }
}
fn collection(feed: &mut Feed, value: &Value, base: &str) -> Result<(), String> {
    if let Some(list) = value["navigation"].as_array() {
        for nav in list {
            for link in json_links(&Value::Array(vec![nav.clone()]), base) {
                if catalog_type(&link.media_type) || link.media_type.is_empty() {
                    add_entry(
                        feed,
                        Entry {
                            title: link.title.clone(),
                            navigation: Some(link),
                            ..Default::default()
                        },
                    )?;
                }
            }
        }
    }
    if let Some(list) = value["publications"].as_array() {
        for book in list {
            add_entry(feed, json_entry(book, base))?;
        }
    }
    Ok(())
}
fn json_entry(value: &Value, base: &str) -> Entry {
    let links = json_links(&value["links"], base);
    Entry {
        title: {
            let title = json_string(&value["metadata"]["title"], 512);
            if title.is_empty() {
                "Untitled publication".into()
            } else {
                title
            }
        },
        author: authors(&value["metadata"]["author"]),
        description: html_text(
            &json_string(&value["metadata"]["description"], 16_384),
            4096,
        ),
        navigation: links.iter().find(|l| catalog_type(&l.media_type)).cloned(),
        acquisitions: links
            .into_iter()
            .filter(|l| l.relation.starts_with(ACQUISITION) || l.relation == "download")
            .collect(),
    }
}
fn json_feed(value: Value, base: &str) -> Result<Parsed, String> {
    if !value.is_object() || !value["metadata"].is_object() {
        return Err("This address did not return an OPDS JSON catalog".into());
    }
    let mut feed = Feed {
        url: base.into(),
        title: json_string(&value["metadata"]["title"], 512),
        ..Default::default()
    };
    if value.get("publications").is_none()
        && value.get("navigation").is_none()
        && value.get("groups").is_none()
    {
        let entry = json_entry(&value, base);
        if entry.acquisitions.is_empty() {
            return Err("The JSON document has no OPDS publications or navigation".into());
        }
        add_entry(&mut feed, entry)?;
    } else {
        collection(&mut feed, &value, base)?;
    }
    for link in json_links(&value["links"], base) {
        assign(&mut feed, link);
    }
    for group in value["groups"].as_array().into_iter().flatten() {
        collection(&mut feed, group, base)?;
        for link in json_links(&group["links"], base)
            .into_iter()
            .filter(|l| l.relation == "self")
        {
            add_entry(
                &mut feed,
                Entry {
                    title: json_string(&group["metadata"]["title"], 512),
                    navigation: Some(link),
                    ..Default::default()
                },
            )?;
        }
    }
    for facet in value["facets"].as_array().into_iter().flatten() {
        for link in json_links(&facet["links"], base) {
            add_entry(
                &mut feed,
                Entry {
                    title: format!(
                        "{}: {}",
                        json_string(&facet["metadata"]["title"], 256),
                        link.title
                    ),
                    navigation: Some(link),
                    ..Default::default()
                },
            )?;
        }
    }
    if feed.title.is_empty() {
        feed.title = "Catalog".into();
    }
    Ok(Parsed::Feed(feed))
}
