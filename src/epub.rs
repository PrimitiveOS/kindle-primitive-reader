//! A bounded, local subset of the EPUB 2 and 3 package format.
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{Seek, SeekFrom};
use std::path::Path;

use roxmltree::{Document, Node, ParsingOptions};
#[cfg(test)]
use zip::CompressionMethod;

use crate::chm::{Chm, MAX_CHM_BYTES};
use crate::library::{DocumentFormat, fingerprint};
use crate::{Result, kf8, loose, mobi};

pub(crate) const MAX_ARCHIVE: u64 = 128 * 1024 * 1024;
pub(crate) const MAX_ENTRIES: usize = 8192;
const MAX_XML: usize = 512 * 1024;
// Reflow chapters may be markup-heavy while still fitting the existing 512 KiB
// laid-out text limit. Metadata and fixed-page XML keep the smaller cap.
const MAX_REFLOW_CHAPTER_XML: usize = 1024 * 1024;
const MAX_RESOURCE: usize = 8 * 1024 * 1024;
const MAX_RASTER_COVER: usize = 4 * 1024 * 1024;
const MAX_COVER_PATH: usize = 512;
const MAX_SERIES: usize = 8;
const MAX_COLLECTION_HINTS: usize = 128;
const MAX_SERIES_NAME: usize = 256;
const MAX_SERIES_POSITION: usize = 64;
const MAX_PAGE_REFERENCES: usize = 8192;

/// Identifiable synthetic content; never treat it as publisher text.
pub(crate) const UNSUPPORTED_SPINE_NOTICE: &str = "<html><head><!--PrimitiveOS unsupported spine notice v1--><title>Unsupported chapter</title></head><body><h1>Unsupported chapter</h1><p>Reader cannot display this chapter's format. Turn the page to continue.</p></body></html>";

use crate::epub_members::Members as Archive;
pub(crate) use crate::epub_zip::check_zip_directory;

/// A source-verified reading position in a reflowable EPUB.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EpubPosition {
    pub chapter: usize,
    pub anchor: crate::Anchor,
}

#[derive(Clone, Debug)]
pub struct Chapter {
    pub href: String,
    pub label: String,
    pub(crate) is_svg: bool,
    pub(crate) unsupported: bool,
    pub(crate) nonlinear: bool,
}

/// A local reading destination with a decoded archive path and optional element ID.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Destination {
    pub path: String,
    pub fragment: Option<String>,
}

/// An entry in the publication's table of contents, in depth-first reading order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TocEntry {
    pub label: String,
    pub depth: usize,
    pub destination: Option<Destination>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BookSeries {
    pub name: String,
    /// Publisher order, such as 2.2.1 for a hierarchical position.
    pub position: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoverKind {
    Bmp,
    Tiff,
    Webp,
    Gif,
    Jpeg,
    Png,
    Svg,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BookCover {
    pub path: String,
    pub kind: CoverKind,
}

pub struct Book {
    pub(crate) language: Option<parley::Language>,
    // Kept apart from shaping language and publication revision.
    liturgical_language: bool,
    pub title: String,
    pub author: Option<String>,
    pub chapters: Vec<Chapter>,
    pub toc: Vec<TocEntry>,
    printed_pages: Vec<TocEntry>,
    printed_source: Option<String>,
    revision: u64,
    pub(crate) spine_rtl: bool,
    fixed_placements: Option<Vec<ItemPlacement>>,
    series: Vec<BookSeries>,
    cover: Option<BookCover>,
    pub(crate) quote_prefixes: Option<Box<crate::book_style::pseudo::quotes::Checkpoints>>,
    content: Content,
}

// Parley's shaping tag drops private-use subtags. Preserve this one reading
// locale only when the preexisting shaping tag remains exactly `la`.
fn liturgical_language(tag: Option<&str>, language: Option<parley::Language>) -> bool {
    tag.is_some_and(|tag| {
        tag.len() <= crate::source::MAX_LANGUAGE_BYTES
            && tag.trim().eq_ignore_ascii_case("la-x-liturgic")
    }) && language.is_some_and(|language| language.as_str() == "la")
}

enum Content {
    Archive {
        archive: Archive,
        package_path: String,
        fonts: Option<crate::epub_font::EmbeddedFonts>,
        fallback_paths: BTreeSet<String>,
        #[cfg(feature = "kindle")]
        stylesheets: Vec<String>,
    },
    Loose {
        metadata: crate::publication_metadata::Metadata,
        format: loose::Format,
        xhtml: BTreeMap<String, String>,
        prior_mobi: BTreeMap<String, Vec<String>>,
        mobi_shadow: Option<MobiShadow>,
        odt_history: Option<crate::odt_sections::History>,
        resources: BTreeMap<String, Vec<u8>>,
        fonts: Option<kf8::fonts::FontSource>,
        fb2_note_sections: BTreeMap<String, loose::Fb2NoteKind>,
        incomplete_fb3_notes: BTreeSet<String>,
        complete_fb3_notes: BTreeMap<String, String>,
    },
    Chm(Box<Chm>),
}

struct MobiShadow {
    chapter: String,
    xhtml: String,
    ids: BTreeSet<String>,
    contents: crate::kf8::legacy_contents::Contents,
}

fn mobi_shadow_resolved(
    canonical: &str,
    shadow: &MobiShadow,
    language: Option<parley::Language>,
    embedded: bool,
) -> Option<BTreeSet<String>> {
    let mut source = crate::source::Source::parse_with_typography(
        canonical,
        language,
        embedded,
        &Default::default(),
    )
    .ok()?;
    let other = crate::source::Source::parse_with_typography(
        &shadow.xhtml,
        language,
        embedded,
        &Default::default(),
    )
    .ok()?;
    Some(source.apply_mobi_shadow(&other, &shadow.ids))
}

struct Item {
    path: String,
    media: String,
    fallback: Option<String>,
    plain_href: bool,
    navigation: bool,
    cover_image: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum RenditionSpread {
    #[default]
    Auto,
    None,
    Landscape,
    Both,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum SpreadSide {
    #[default]
    Auto,
    Left,
    Right,
    Center,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ItemPlacement {
    pub(crate) spread: RenditionSpread,
    pub(crate) side: SpreadSide,
}

#[derive(Clone, Debug)]
pub(crate) struct FixedSpine {
    pub(crate) right_to_left: bool,
    pub(crate) placements: Vec<ItemPlacement>,
}

impl Book {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(format) = loose::Format::for_path(path) {
            return match format {
                loose::Format::Mobi => Self::open_mobi(path),
                loose::Format::Chm => Self::open_chm(path),
                loose::Format::Fb3 => Self::open_fb3(path),
                _ => {
                    let (document, actual_format, large_text) =
                        loose::Document::open_classified(path, format)?;
                    if actual_format == loose::Format::Odt || large_text {
                        Self::from_partitioned(document, actual_format)
                    } else {
                        Ok(Self::from_loose(document, actual_format))
                    }
                }
            };
        }
        Self::from_archive(Archive::open(path)?)
    }

    #[cfg(feature = "kindle")]
    pub(crate) fn open_snapshot(bytes: std::sync::Arc<[u8]>) -> Result<Self> {
        let mut book = Self::from_archive(Archive::Zip(crate::epub_zip::open_snapshot(bytes)?))?;
        book.quote_prefixes = Some(Box::new(
            crate::book_style::pseudo::quotes::Checkpoints::snapshot(),
        ));
        Ok(book)
    }

    #[cfg(feature = "kindle")]
    pub(crate) fn open_folder_snapshot(
        members: std::sync::Arc<std::collections::BTreeMap<String, std::sync::Arc<[u8]>>>,
    ) -> Result<Self> {
        let mut book = Self::from_archive(Archive::Snapshot(members))?;
        book.quote_prefixes = Some(Box::new(
            crate::book_style::pseudo::quotes::Checkpoints::snapshot(),
        ));
        Ok(book)
    }

    #[cfg(feature = "kindle")]
    pub(crate) fn folder_snapshot(
        &self,
    ) -> Option<std::sync::Arc<std::collections::BTreeMap<String, std::sync::Arc<[u8]>>>> {
        match &self.content {
            Content::Archive {
                archive: Archive::Snapshot(members),
                ..
            } => Some(members.clone()),
            _ => None,
        }
    }

    #[cfg(feature = "kindle")]
    pub(crate) fn quote_file_stamp(&self) -> Option<&crate::session::screen_pages::FileStamp> {
        if !self.is_reflowable_epub() {
            return None;
        }
        match &self.content {
            Content::Archive {
                archive: Archive::Zip(archive),
                ..
            } => archive.stamp.as_ref(),
            _ => None,
        }
    }

    #[cfg(feature = "kindle")]
    pub(crate) fn font_inspection_snapshot(
        path: &Path,
        bytes: std::sync::Arc<[u8]>,
    ) -> Result<Self> {
        if loose::Format::for_path(path) == Some(loose::Format::Mobi) {
            let fonts = kf8::fonts::Input::Snapshot(bytes.clone());
            return Self::from_mobi(&bytes, loose::fallback_title(path), fonts);
        }
        Self::open_snapshot(bytes)
    }

    pub(crate) fn from_archive(mut archive: Archive) -> Result<Self> {
        // Some reader-compatible publications compress this marker or write another member
        // first. Its identity and the following OCF/OPF checks identify the publication.
        let marker = read_entry(&mut archive, "mimetype", 99)?;
        let value = marker
            .strip_suffix(b"\r\n")
            .or_else(|| marker.strip_suffix(b"\n"))
            .unwrap_or(&marker);
        if value != b"application/epub+zip" {
            return Err("The package is not an EPUB book".into());
        }
        let container = xml_entry(&mut archive, "META-INF/container.xml")?;
        let container = parse_xml(&container)?;
        let root = container.root_element();
        if !root.has_tag_name("container") {
            return Err("The EPUB container is invalid".into());
        }
        let package_path = root
            .descendants()
            .find(|node| {
                node.has_tag_name("rootfile")
                    && node.attribute("media-type") == Some("application/oebps-package+xml")
            })
            .and_then(|node| node.attribute("full-path"))
            .ok_or("The EPUB has no package document")?;
        let package_path = Self::resolve("index.xhtml", package_path)?;
        let package = xml_entry(&mut archive, &package_path)?;
        let revision = crate::revision(&package);
        let package = parse_xml(&package)?;
        let root = package.root_element();
        if !root.has_tag_name("package")
            || !root
                .attribute("version")
                .is_some_and(|v| v.starts_with("2.") || v.starts_with("3."))
        {
            return Err("The EPUB package version is unsupported".into());
        }
        let metadata = child(root, "metadata")?;
        let language_tag = metadata
            .children()
            .filter(|n| n.has_tag_name(("http://purl.org/dc/elements/1.1/", "language")))
            .filter_map(|n| n.text().map(str::trim))
            .find(|tag| !tag.is_empty())
            .filter(|tag| tag.len() <= crate::source::MAX_LANGUAGE_BYTES);
        let language = language_tag.and_then(|tag| parley::Language::parse(tag).ok());
        let liturgical_language = liturgical_language(language_tag, language);
        let default_layout = package_layout(metadata)?;
        let default_spread = package_spread(metadata);
        let series = package_series(metadata);
        let title = metadata
            .children()
            .find(|node| node.has_tag_name("title"))
            .and_then(|node| node.text())
            .filter(|title| !title.trim().is_empty())
            .unwrap_or("Untitled book")
            .trim()
            .to_owned();
        let authors = metadata
            .children()
            .filter(|node| node.has_tag_name(("http://purl.org/dc/elements/1.1/", "creator")))
            .filter_map(|node| node.text().map(str::trim))
            .filter(|name| !name.is_empty())
            .collect::<Vec<_>>();
        let author = (!authors.is_empty()).then(|| authors.join(", "));
        let mut manifest = BTreeMap::new();
        for node in child(root, "manifest")?
            .children()
            .filter(|node| node.has_tag_name("item"))
        {
            let id = node
                .attribute("id")
                .ok_or("An EPUB manifest item has no ID")?;
            let href = node
                .attribute("href")
                .ok_or("An EPUB manifest item has no path")?;
            let Ok(path) = Self::resolve(&package_path, href) else {
                continue;
            };
            let properties = node.attribute("properties").unwrap_or_default();
            let has_property = |name| {
                properties
                    .split_ascii_whitespace()
                    .any(|property| property == name)
            };
            let item = Item {
                path,
                media: node.attribute("media-type").unwrap_or_default().to_owned(),
                fallback: node.attribute("fallback").map(str::to_owned),
                plain_href: !href.contains(['#', '?']),
                navigation: has_property("nav"),
                cover_image: has_property("cover-image"),
            };
            if manifest.insert(id, item).is_some() {
                return Err("The EPUB manifest contains duplicate IDs".into());
            }
        }
        let fonts = crate::epub_font::EmbeddedFonts::read(
            &mut archive,
            root,
            manifest
                .values()
                .map(|item| (item.path.as_str(), item.media.as_str())),
        );
        let mut cover = package_cover(metadata, &manifest);
        let spine = child(root, "spine")?;
        if cover.is_none() {
            cover = infer_first_spine_cover(&mut archive, root, metadata, spine, &manifest);
        }
        let navigation = manifest.values().find(|item| item.navigation);
        let ncx = spine.attribute("toc").and_then(|id| manifest.get(id));
        // Optional navigation can be missing or malformed even when the spine is readable.
        let (mut toc, mut printed_pages, group_headings) = navigation
            .and_then(|nav| {
                let xml = xml_entry(&mut archive, &nav.path).ok()?;
                let xml = loose::without_plain_html_doctype(&xml);
                let doc = parse_xml(&xml).ok()?;
                Some(publication_navigation(&doc, &nav.path))
            })
            .unwrap_or_default();
        if ncx.is_some() || !group_headings.is_empty() {
            let mut paths = BTreeMap::new();
            let mut reflow_paths = BTreeSet::new();
            for reference in spine.children().filter(|node| node.has_tag_name("itemref")) {
                let Some(item) = reference.attribute("idref").and_then(|id| manifest.get(id))
                else {
                    continue;
                };
                let path = paths
                    .entry(item.path.as_str())
                    .or_insert_with(|| (0usize, !archive.missing_after_recovery(&item.path)));
                path.0 += 1;
                if spine_layout(reference, default_layout).ok() == Some(false)
                    && matches!(item.media.as_str(), "application/xhtml+xml" | "text/html")
                {
                    reflow_paths.insert(item.path.as_str());
                }
            }
            inherit_group_targets(
                &mut toc,
                &group_headings,
                &paths,
                &reflow_paths,
                &mut archive,
            );
            let has_target = |entries: &[TocEntry], print: bool| {
                entries.iter().any(|entry| {
                    entry.destination.as_ref().is_some_and(|destination| {
                        paths
                            .get(destination.path.as_str())
                            .is_some_and(|&(count, available)| {
                                !print || (available && count == 1 && entry.label.len() <= 256)
                            })
                    })
                })
            };
            let needs_toc = !has_target(&toc, false);
            let needs_print = !has_target(&printed_pages, true);
            if (needs_toc || needs_print)
                && let Some(ncx) = ncx
                && let Ok(xml) = xml_entry(&mut archive, &ncx.path)
                && let Ok(doc) = parse_xml(&xml)
            {
                if needs_toc {
                    let fallback = ncx_toc(&doc, &ncx.path);
                    if has_target(&fallback, false) {
                        toc = fallback;
                    }
                }
                if needs_print {
                    let fallback = ncx_pages(&doc, &ncx.path);
                    if has_target(&fallback, true) {
                        printed_pages = fallback;
                    }
                }
            }
        }
        // Keep every existing nav/NCX result. Only try recovering a separate
        // malformed HTML nav document for a list that is still unavailable.
        if (toc.is_empty() || printed_pages.is_empty())
            && let Some(nav) = navigation
            && matches!(nav.media.as_str(), "application/xhtml+xml" | "text/html")
            && let Ok(xml) = archive_html_recovery(&mut archive, &nav.path, MAX_XML)
        {
            let prepared = loose::without_plain_html_doctype(&xml);
            if let Ok(doc) = parse_xml(&prepared) {
                let (mut recovered_toc, recovered_pages, recovered_groups) =
                    publication_navigation(&doc, &nav.path);
                if toc.is_empty() && !recovered_groups.is_empty() {
                    let mut paths = BTreeMap::new();
                    let mut reflow_paths = BTreeSet::new();
                    for reference in spine.children().filter(|node| node.has_tag_name("itemref")) {
                        let Some(item) =
                            reference.attribute("idref").and_then(|id| manifest.get(id))
                        else {
                            continue;
                        };
                        let path = paths.entry(item.path.as_str()).or_insert_with(|| {
                            (0usize, !archive.missing_after_recovery(&item.path))
                        });
                        path.0 += 1;
                        if spine_layout(reference, default_layout).ok() == Some(false)
                            && matches!(item.media.as_str(), "application/xhtml+xml" | "text/html")
                        {
                            reflow_paths.insert(item.path.as_str());
                        }
                    }
                    inherit_group_targets(
                        &mut recovered_toc,
                        &recovered_groups,
                        &paths,
                        &reflow_paths,
                        &mut archive,
                    );
                }
                if toc.is_empty() {
                    toc = recovered_toc;
                }
                if printed_pages.is_empty() {
                    printed_pages = recovered_pages;
                }
            }
        }
        let mut labels = BTreeMap::new();
        for entry in &toc {
            if let Some(destination) = &entry.destination {
                labels
                    .entry(destination.path.as_str())
                    .or_insert(entry.label.as_str());
            }
        }
        let mut chapters = Vec::new();
        let mut fixed_layout = None;
        let mut placements = Vec::new();
        for reference in spine.children().filter(|node| node.has_tag_name("itemref")) {
            let item = reference
                .attribute("idref")
                .and_then(|id| manifest.get(id))
                .ok_or("The EPUB spine refers to a missing or external item")?;
            let fixed = spine_layout(reference, default_layout)?;
            if fixed_layout
                .replace(fixed)
                .is_some_and(|previous| previous != fixed)
            {
                return Err(
                    "Reader cannot open an EPUB that mixes fixed-layout and reflowable spine items"
                        .into(),
                );
            }
            let is_svg = item.media == "image/svg+xml";
            if fixed && item.media != "application/xhtml+xml" && !is_svg {
                return Err(
                    "Reader supports fixed-layout EPUB pages stored as XHTML or static SVG".into(),
                );
            }
            if fixed {
                if placements.len() == MAX_ENTRIES {
                    return Err("The fixed-layout EPUB has too many pages".into());
                }
                placements.push(item_placement(reference, default_spread));
            }
            let unsupported = !fixed
                && !matches!(item.media.as_str(), "application/xhtml+xml" | "text/html")
                && !(is_svg && crate::svg_image::is_package_svg(&item.path));
            let label = labels
                .get(item.path.as_str())
                .map(|label| (*label).to_owned())
                .unwrap_or_else(|| format!("Chapter {}", chapters.len() + 1));
            chapters.push(Chapter {
                href: item.path.clone(),
                label,
                is_svg,
                unsupported,
                nonlinear: reference.attribute("linear") == Some("no"),
            });
        }
        if chapters.is_empty() {
            return Err("The EPUB contains no reading chapters".into());
        }
        // Preserve every previously opening book. Only an otherwise rejected
        // reflow spine with no supported linear item can try manifest fallbacks.
        let mut fallback_paths = BTreeSet::new();
        if chapters.iter().any(|chapter| chapter.unsupported)
            && !chapters
                .iter()
                .any(|chapter| !chapter.unsupported && !chapter.nonlinear)
        {
            let mut manifest_paths = BTreeMap::new();
            for item in manifest.values() {
                *manifest_paths.entry(item.path.as_str()).or_insert(0usize) += 1;
            }
            let mut original_paths = BTreeMap::new();
            for chapter in &chapters {
                *original_paths.entry(chapter.href.clone()).or_insert(0usize) += 1;
            }
            let mut examined = 0usize;
            let mut reads = 0usize;
            for (chapter, reference) in chapters
                .iter_mut()
                .zip(spine.children().filter(|node| node.has_tag_name("itemref")))
            {
                if !chapter.unsupported || chapter.nonlinear || examined == 64 || reads == 8 {
                    continue;
                }
                examined += 1;
                if original_paths.get(&chapter.href) != Some(&1)
                    || manifest_paths.get(chapter.href.as_str()) != Some(&1)
                {
                    continue;
                }
                let Some(mut item) = reference.attribute("idref").and_then(|id| manifest.get(id))
                else {
                    continue;
                };
                if !item.plain_href {
                    continue;
                }
                let mut visited = BTreeSet::new();
                for _ in 0..32 {
                    let Some(next) = item.fallback.as_deref() else {
                        break;
                    };
                    if !visited.insert(next) {
                        break;
                    }
                    let Some(target) = manifest.get(next) else {
                        break;
                    };
                    item = target;
                    if !matches!(target.media.as_str(), "application/xhtml+xml" | "text/html") {
                        continue;
                    }
                    if !target.plain_href
                        || manifest_paths.get(target.path.as_str()) != Some(&1)
                        || original_paths.contains_key(&target.path)
                        || fallback_paths.contains(&target.path)
                    {
                        break;
                    }
                    reads += 1;
                    let valid =
                        archive_html_recovery(&mut archive, &target.path, MAX_REFLOW_CHAPTER_XML)
                            .ok()
                            .and_then(|xhtml| crate::source::Source::parse(&xhtml).ok())
                            .is_some();
                    if valid {
                        let old = std::mem::replace(&mut chapter.href, target.path.clone());
                        chapter.unsupported = false;
                        chapter.is_svg = false;
                        fallback_paths.insert(target.path.clone());
                        // A fragment in a foreign format does not identify an
                        // equivalent node in its alternate representation.
                        for entry in &mut toc {
                            if let Some(destination) = &mut entry.destination
                                && destination.path == old
                                && destination.fragment.is_none()
                            {
                                destination.path = target.path.clone();
                            }
                        }
                    }
                    break;
                }
            }
        }
        if chapters.iter().any(|chapter| chapter.unsupported)
            && !chapters
                .iter()
                .any(|chapter| !chapter.unsupported && !chapter.nonlinear)
        {
            return Err("The EPUB has no supported linear reading chapters".into());
        }
        let adobe_map = spine.attribute("page-map").and_then(|id| manifest.get(id));
        if !printed_pages.is_empty() || adobe_map.is_some() {
            let mut paths = BTreeMap::new();
            let unsupported_paths: BTreeSet<_> = chapters
                .iter()
                .filter(|chapter| chapter.unsupported)
                .map(|chapter| chapter.href.as_str())
                .collect();
            for chapter in &chapters {
                if !unsupported_paths.contains(chapter.href.as_str())
                    && !archive.missing_after_recovery(&chapter.href)
                {
                    *paths.entry(chapter.href.as_str()).or_insert(0) += 1;
                }
            }
            let valid = |entry: &TocEntry| {
                entry.label.len() <= 256
                    && entry
                        .destination
                        .as_ref()
                        .is_some_and(|destination| paths.get(destination.path.as_str()) == Some(&1))
            };
            printed_pages.retain(valid);
            if printed_pages.is_empty()
                && let Some(item) = adobe_map
                && let Ok(xml) = xml_entry(&mut archive, &item.path)
                && let Ok(doc) = parse_xml(&xml)
            {
                printed_pages = adobe_pages(&doc, &item.path)
                    .filter(valid)
                    .take(MAX_PAGE_REFERENCES)
                    .collect();
            }
        }
        let printed_source = metadata
            .children()
            .find(|node| node.has_tag_name(("http://purl.org/dc/elements/1.1/", "source")))
            .and_then(|node| node.text())
            .map(str::trim)
            .filter(|source| !source.is_empty() && source.len() <= 1024)
            .map(str::to_owned);
        Ok(Self {
            title,
            author,
            chapters,
            toc,
            printed_pages,
            printed_source,
            revision,
            language,
            liturgical_language,
            spine_rtl: spine.attribute("page-progression-direction") == Some("rtl"),
            fixed_placements: fixed_layout.unwrap_or(false).then_some(placements),
            series,
            cover,
            quote_prefixes: None,
            content: Content::Archive {
                archive,
                package_path,
                fonts,
                fallback_paths,
                #[cfg(feature = "kindle")]
                stylesheets: manifest
                    .values()
                    .filter(|item| item.media.eq_ignore_ascii_case("text/css"))
                    .take(crate::book_style::MAX_INVENTORY_SHEETS)
                    .map(|item| item.path.clone())
                    .collect(),
            },
        })
    }

    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }

    #[cfg(feature = "kindle")]
    pub(crate) fn supports_screen_page_count(&self) -> bool {
        matches!(
            &self.content,
            Content::Archive { archive: Archive::Zip(archive), .. } if archive.missing.is_empty()
        )
    }

    #[cfg(feature = "kindle")]
    pub(crate) fn supports_whole_book_font_inspection(&self) -> bool {
        self.supports_screen_page_count()
            || matches!(&self.content, Content::Loose { fonts: Some(_), .. })
    }

    #[cfg(feature = "kindle")]
    pub(crate) fn supports_eager_typography(&self) -> bool {
        matches!(&self.content, Content::Archive { archive: Archive::Zip(archive), .. }
        if archive.missing.is_empty()
            && archive.zip.len() <= 512
            && archive.zip.decompressed_size().is_some_and(|bytes| bytes <= 4 * 1024 * 1024)
            && archive.zip.file_names().all(|name| {
                name.rsplit_once('.').is_none_or(|(_, extension)| {
                    !extension.eq_ignore_ascii_case("ttf")
                        && !extension.eq_ignore_ascii_case("otf")
                        && !extension.eq_ignore_ascii_case("woff")
                        && !extension.eq_ignore_ascii_case("woff2")
                })
            }))
    }

    #[cfg(feature = "kindle")]
    pub(crate) fn may_have_embedded_fonts(&self) -> bool {
        let has_font_extension = |path: &str| {
            path.rsplit_once('.').is_some_and(|(_, extension)| {
                extension.eq_ignore_ascii_case("ttf")
                    || extension.eq_ignore_ascii_case("otf")
                    || extension.eq_ignore_ascii_case("woff")
                    || extension.eq_ignore_ascii_case("woff2")
            })
        };
        match &self.content {
            Content::Archive {
                archive: Archive::Zip(archive),
                ..
            } => archive.zip.file_names().any(has_font_extension),
            Content::Archive {
                archive: Archive::Folder(_),
                ..
            } => true,
            Content::Archive {
                archive: Archive::Snapshot(members),
                ..
            } => members.keys().any(|path| has_font_extension(path)),
            Content::Loose {
                fonts, resources, ..
            } => {
                fonts
                    .as_ref()
                    .is_some_and(|source| !source.records.is_empty())
                    || resources.keys().any(|path| has_font_extension(path))
            }
            Content::Chm(_) => false,
        }
    }

    #[cfg(feature = "kindle")]
    pub(crate) fn publisher_stylesheets(&self) -> &[String] {
        match &self.content {
            Content::Archive { stylesheets, .. } => stylesheets,
            _ => &[],
        }
    }

    pub(crate) fn package_xml(&mut self) -> Result<String> {
        match &mut self.content {
            Content::Archive {
                archive,
                package_path,
                ..
            } => xml_entry(archive, package_path),
            Content::Loose { .. } | Content::Chm(_) => {
                Err("EPUB CFI positions require an EPUB publication".into())
            }
        }
    }

    #[cfg(feature = "kindle")]
    pub(crate) fn is_fb2_note_section(&self, target: &Destination) -> bool {
        self.fb2_note_sections(&target.path)
            .is_some_and(|sections| {
                target
                    .fragment
                    .as_ref()
                    .is_some_and(|id| sections.contains_key(id))
            })
    }

    pub(crate) fn fb2_note_sections(
        &self,
        chapter: &str,
    ) -> Option<&BTreeMap<String, loose::Fb2NoteKind>> {
        match &self.content {
            Content::Loose {
                format: loose::Format::Fb2,
                fb2_note_sections,
                ..
            } if chapter == loose::CHAPTER_PATH => Some(fb2_note_sections),
            _ => None,
        }
    }

    pub(crate) fn publisher_metadata(&self) -> Option<&crate::publication_metadata::Metadata> {
        match &self.content {
            Content::Loose { metadata, .. } => Some(metadata),
            _ => None,
        }
    }

    #[cfg(feature = "kindle")]
    pub(crate) fn from_text(title: String, text: &str) -> Result<Self> {
        Ok(Self::from_loose(
            loose::Document {
                metadata: Default::default(),
                title,
                author: None,
                cover: None,
                chapter_path: loose::CHAPTER_PATH.to_owned(),
                xhtml: loose::text_as_xhtml(text)?,
                fb2_note_sections: Default::default(),
                toc: Vec::new(),
                resources: BTreeMap::new(),
            },
            loose::Format::Text,
        ))
    }

    fn from_loose(document: loose::Document, format: loose::Format) -> Self {
        // Loose document importers retain publisher language independently of
        // their generated XHTML. Use the same bounded validation as EPUB so
        // automatic typography can select language-aware shaping and patterns.
        let language_tag = document
            .metadata
            .language
            .as_deref()
            .map(str::trim)
            .filter(|tag| !tag.is_empty() && tag.len() <= crate::source::MAX_LANGUAGE_BYTES);
        let language = language_tag.and_then(|tag| parley::Language::parse(tag).ok());
        let liturgical_language = liturgical_language(language_tag, language);
        let chapter_path = document.chapter_path;
        Self {
            revision: crate::revision(&document.xhtml),
            language,
            liturgical_language,
            spine_rtl: false,
            fixed_placements: None,
            series: Vec::new(),
            cover: document.cover,
            quote_prefixes: None,
            chapters: vec![Chapter {
                is_svg: false,
                unsupported: false,
                nonlinear: false,
                href: chapter_path.clone(),
                label: document.title.clone(),
            }],
            title: document.title,
            author: document.author,
            toc: document.toc,
            printed_pages: Vec::new(),
            printed_source: None,
            content: Content::Loose {
                metadata: document.metadata,
                format,
                xhtml: BTreeMap::from([(chapter_path, document.xhtml)]),
                prior_mobi: BTreeMap::new(),
                mobi_shadow: None,
                odt_history: None,
                resources: document.resources,
                fb2_note_sections: document.fb2_note_sections,
                incomplete_fb3_notes: BTreeSet::new(),
                complete_fb3_notes: BTreeMap::new(),
                fonts: None,
            },
        }
    }

    fn from_partitioned(document: loose::Document, format: loose::Format) -> Result<Self> {
        let split = if document.xhtml.len() <= 512 * 1024 {
            None
        } else {
            crate::odt_sections::prepare(
                &document.xhtml,
                &document.chapter_path,
                &document.title,
                &document.toc,
            )?
        };
        let mut book = Self::from_loose(document, format);
        if let Some(split) = split {
            book.toc = split.toc;
            let Content::Loose {
                xhtml, odt_history, ..
            } = &mut book.content
            else {
                unreachable!()
            };
            xhtml.clear();
            book.chapters.clear();
            for part in split.chapters {
                book.chapters.push(Chapter {
                    href: part.path.clone(),
                    label: part.label,
                    is_svg: false,
                    unsupported: false,
                    nonlinear: false,
                });
                xhtml.insert(part.path, part.xhtml);
            }
            *odt_history = Some(split.history);
        }
        Ok(book)
    }

    #[cfg(feature = "kindle")]
    pub(crate) fn odt_continued_before(&self) -> Vec<bool> {
        match &self.content {
            Content::Loose {
                odt_history: Some(history),
                ..
            } => history.continued_before().to_vec(),
            _ => Vec::new(),
        }
    }

    pub(crate) fn is_odt(&self) -> bool {
        matches!(
            self.content,
            Content::Loose {
                format: loose::Format::Odt,
                ..
            }
        )
    }

    pub(crate) fn has_split_odt(&self) -> bool {
        matches!(
            self.content,
            Content::Loose {
                odt_history: Some(_),
                ..
            }
        )
    }

    pub(crate) fn migrate_old_odt_anchor(
        &self,
        anchor: &crate::Anchor,
        end: bool,
    ) -> Result<Option<(usize, crate::Anchor)>> {
        match &self.content {
            Content::Loose {
                odt_history: Some(history),
                ..
            } => {
                if anchor.raw_caret.is_none() {
                    return history.migrate(anchor, end);
                }
                if history.is_current_raw_only(anchor) {
                    return Ok(None);
                }
                // Do not translate a raw caret through a legacy split. If its
                // canonical portion already belongs to the verified current
                // section, retain the entire original anchor unchanged; page
                // opening still validates its raw source fingerprint.
                let mut canonical = anchor.clone();
                canonical.raw_caret = None;
                match history.migrate(&canonical, end)? {
                    None => Ok(None),
                    Some(_) => Err("Raw whitespace locations cannot yet be migrated safely".into()),
                }
            }
            _ => Ok(None),
        }
    }

    fn open_fb3(path: &Path) -> Result<Self> {
        let file = File::open(path)?;
        let metadata = file.metadata()?;
        if !metadata.is_file() {
            return Err("The FB3 document is not a regular file".into());
        }
        let input = std::io::BufReader::new(file);
        let imported = crate::fb3::open(input, metadata.len(), loose::fallback_title(path))?;
        let language = imported
            .first
            .metadata
            .language
            .as_deref()
            .and_then(|tag| parley::Language::parse(tag).ok());
        let mut book = Self::from_loose(imported.first, loose::Format::Fb3);
        book.revision = imported.revision;
        book.language = language;
        book.cover = imported.cover;
        let Content::Loose {
            xhtml,
            incomplete_fb3_notes,
            complete_fb3_notes,
            ..
        } = &mut book.content
        else {
            unreachable!()
        };
        *incomplete_fb3_notes = imported.incomplete_note_ids;
        *complete_fb3_notes = imported.complete_notes;
        for (href, body) in imported.rest {
            let label = book
                .toc
                .iter()
                .find(|entry| {
                    entry
                        .destination
                        .as_ref()
                        .is_some_and(|target| target.path == href)
                })
                .map_or_else(
                    || format!("Section {}", book.chapters.len() + 1),
                    |entry| entry.label.clone(),
                );
            xhtml.insert(href.clone(), body);
            book.chapters.push(Chapter {
                href,
                label,
                is_svg: false,
                unsupported: false,
                nonlinear: false,
            });
        }
        Ok(book)
    }

    fn open_mobi(path: &Path) -> Result<Self> {
        let mut input = File::open(path)?;
        let metadata = input.metadata()?;
        if !metadata.is_file() || metadata.len() > mobi::MAX_MOBI_BYTES as u64 {
            return Err(mobi::MobiError::TooLarge.into());
        }
        let bytes = loose::read_bounded(&mut input, mobi::MAX_MOBI_BYTES)?;
        Self::from_mobi(
            &bytes,
            loose::fallback_title(path),
            kf8::fonts::Input::File(input),
        )
    }

    fn from_mobi(
        bytes: &[u8],
        fallback_title: &str,
        font_input: kf8::fonts::Input,
    ) -> Result<Self> {
        let mut prior_mobi = BTreeMap::new();
        let mut mobi_shadow = None;
        let mut revision = crate::revision_bytes(bytes);
        let mut small_legacy = false;
        let mut publisher_contents = crate::kf8::legacy_contents::Contents::default();
        let mut publication = match kf8::open_edition(bytes)? {
            kf8::Edition::Legacy(mut document) => {
                publisher_contents = document.contents;
                let label = document
                    .title
                    .clone()
                    .unwrap_or_else(|| fallback_title.into());
                small_legacy = document.xhtml.len() <= 512 * 1024;
                if small_legacy {
                    revision = crate::revision(&document.xhtml);
                }
                let chapter = kf8::Kf8Chapter {
                    continued: false,
                    path: loose::CHAPTER_PATH.into(),
                    label,
                    xhtml: document.xhtml,
                };
                if small_legacy && let Some(shadow) = document.shadow.take() {
                    let shadow_chapter = kf8::Kf8Chapter {
                        continued: chapter.continued,
                        path: chapter.path.clone(),
                        label: chapter.label.clone(),
                        xhtml: chapter.xhtml.clone(),
                    };
                    if let Ok((mut parts, _)) = crate::kindle_sections::legacy(
                        shadow_chapter,
                        shadow.additions,
                        document.presentation.clone(),
                        &shadow.contents.insertions,
                        false,
                    ) && parts.len() == 1
                        && parts[0].xhtml.len() <= 1024 * 1024
                    {
                        let part = parts.remove(0);
                        mobi_shadow = Some(MobiShadow {
                            chapter: part.path,
                            xhtml: part.xhtml,
                            ids: shadow.ids,
                            contents: shadow.contents,
                        });
                    }
                }
                let chapters = if document.additions.is_empty()
                    && document.presentation.is_empty()
                    && publisher_contents.insertions.is_empty()
                {
                    if small_legacy {
                        vec![chapter]
                    } else {
                        crate::kindle_sections::partition(vec![chapter])?
                    }
                } else {
                    let (current, previous) = crate::kindle_sections::legacy(
                        chapter,
                        document.additions,
                        document.presentation,
                        &publisher_contents.insertions,
                        !small_legacy,
                    )?;
                    prior_mobi = previous;
                    current
                };
                kf8::Kf8Document {
                    metadata: document.metadata,
                    language: document.language,
                    title: document.title,
                    author: document.author,
                    resources: document.resources,
                    contents: Default::default(),
                    font_records: BTreeMap::new(),
                    chapters,
                }
            }
            kf8::Edition::Kf8(mut edition) => {
                edition.chapters = crate::kindle_sections::partition_with_contents(
                    edition.chapters,
                    &mut edition.contents,
                )?;
                edition
            }
        };
        let cover = mobi::cover::extract(bytes).and_then(|(kind, image)| {
            if let Some((path, _)) = publication
                .resources
                .iter()
                .find(|(_, bytes)| bytes.as_slice() == image)
            {
                return Some(BookCover {
                    path: path.clone(),
                    kind,
                });
            }
            let used = publication
                .resources
                .values()
                .try_fold(0usize, |total, value| total.checked_add(value.len()))?;
            if publication.resources.len() >= 128 || used.checked_add(image.len())? > MAX_RESOURCE {
                return None;
            }
            let extension = match kind {
                CoverKind::Jpeg => "jpg",
                CoverKind::Png => "png",
                CoverKind::Gif => "gif",
                CoverKind::Bmp | CoverKind::Tiff | CoverKind::Webp | CoverKind::Svg => return None,
            };
            let path = format!("assets/primitiveos-mobi-cover.{extension}");
            if publication.resources.contains_key(&path) {
                return None;
            }
            publication.resources.insert(path.clone(), image.to_vec());
            Some(BookCover { path, kind })
        });
        let mut chapters = Vec::with_capacity(publication.chapters.len());
        let mut toc = Vec::with_capacity(publication.chapters.len());
        let mut xhtml = BTreeMap::new();
        for chapter in publication.chapters {
            if !small_legacy && !chapter.continued {
                toc.push(TocEntry {
                    label: chapter.label.clone(),
                    depth: 0,
                    destination: Some(Destination {
                        path: chapter.path.clone(),
                        fragment: None,
                    }),
                });
            }
            chapters.push(Chapter {
                href: chapter.path.clone(),
                label: chapter.label,
                is_svg: false,
                unsupported: false,
                nonlinear: false,
            });
            xhtml.insert(chapter.path, chapter.xhtml);
        }
        if !publisher_contents.items.is_empty() {
            toc = publisher_contents.entries(&xhtml);
            if let Some(shadow) = &mobi_shadow
                && let Some(canonical) = xhtml.get(&shadow.chapter)
                && let Some(resolved) =
                    mobi_shadow_resolved(canonical, shadow, publication.language, true)
                && let Some(without_embedded) =
                    mobi_shadow_resolved(canonical, shadow, publication.language, false)
            {
                let shadow_sections =
                    BTreeMap::from([(shadow.chapter.clone(), shadow.xhtml.clone())]);
                let alternatives = shadow.contents.clone().entries(&shadow_sections);
                for (row, alternate) in toc.iter_mut().zip(alternatives) {
                    if row.destination.is_none()
                        && alternate
                            .destination
                            .as_ref()
                            .and_then(|destination| destination.fragment.as_ref())
                            .is_some_and(|id| {
                                resolved.contains(id) && without_embedded.contains(id)
                            })
                    {
                        row.destination = alternate.destination;
                    }
                }
            }
        } else if !publication.contents.rows.is_empty() {
            toc = publication.contents.finish(&xhtml);
        }
        let liturgical_language = liturgical_language(
            publication.metadata.language.as_deref(),
            publication.language,
        );
        let title = publication.title.unwrap_or_else(|| fallback_title.into());
        Ok(Self {
            title,
            author: publication.author,
            chapters,
            toc,
            printed_pages: Vec::new(),
            printed_source: None,
            revision,
            language: publication.language,
            liturgical_language,
            spine_rtl: false,
            fixed_placements: None,
            series: Vec::new(),
            cover,
            quote_prefixes: None,
            content: Content::Loose {
                metadata: publication.metadata,
                format: loose::Format::Mobi,
                xhtml,
                prior_mobi,
                mobi_shadow,
                odt_history: None,
                resources: publication.resources,
                fb2_note_sections: Default::default(),
                incomplete_fb3_notes: BTreeSet::new(),
                complete_fb3_notes: BTreeMap::new(),
                fonts: (!publication.font_records.is_empty()).then_some(kf8::fonts::FontSource {
                    input: font_input,
                    records: publication.font_records,
                }),
            },
        })
    }

    fn open_chm(path: &Path) -> Result<Self> {
        let mut file = File::open(path)?;
        let metadata = file.metadata()?;
        if !metadata.is_file() || metadata.len() > MAX_CHM_BYTES {
            return Err("The CHM file exceeds the 32 MiB archive limit or is unavailable".into());
        }
        let size = metadata.len();
        let revision = fingerprint(&mut file, size, DocumentFormat::Chm.tag(), size)?;
        file.seek(SeekFrom::Start(0))?;
        let mut chm = Chm::open(file)?;
        let title = chm
            .title
            .take()
            .unwrap_or_else(|| loose::fallback_title(path).into());
        Ok(Self {
            title,
            author: None,
            chapters: chm.chapters.clone(),
            toc: chm.toc.clone(),
            printed_pages: Vec::new(),
            printed_source: None,
            revision,
            language: None,
            liturgical_language: false,
            spine_rtl: false,
            fixed_placements: None,
            series: Vec::new(),
            cover: None,
            quote_prefixes: None,
            content: Content::Chm(Box::new(chm)),
        })
    }

    pub(crate) fn publication_typography(&self) -> Option<crate::TypographyLocale> {
        if self.liturgical_language
            && self
                .language
                .is_some_and(|language| language.as_str() == "la")
        {
            Some(crate::TypographyLocale::LiturgicalLatin)
        } else {
            self.language.map(crate::TypographyLocale::publication)
        }
    }

    pub fn is_fixed_layout(&self) -> bool {
        self.fixed_placements.is_some()
    }

    pub(crate) fn is_reflowable_epub(&self) -> bool {
        matches!(self.content, Content::Archive { .. }) && !self.is_fixed_layout()
    }

    pub(crate) fn incomplete_fb3_notes(&self) -> Option<&BTreeSet<String>> {
        match &self.content {
            Content::Loose {
                format: loose::Format::Fb3,
                incomplete_fb3_notes,
                ..
            } => Some(incomplete_fb3_notes),
            _ => None,
        }
    }

    pub(crate) fn complete_fb3_notes(&self) -> Option<&BTreeMap<String, String>> {
        match &self.content {
            Content::Loose {
                format: loose::Format::Fb3,
                complete_fb3_notes,
                ..
            } => Some(complete_fb3_notes),
            _ => None,
        }
    }

    pub(crate) fn automatic_stylesheet(&self) -> crate::BaseStylesheet {
        match &self.content {
            Content::Loose {
                format: loose::Format::Fb2 | loose::Format::Fb3 | loose::Format::Text,
                ..
            } => crate::BaseStylesheet::FictionBook,
            Content::Archive { .. } | Content::Loose { .. } | Content::Chm(_) => {
                crate::BaseStylesheet::Traditional
            }
        }
    }

    #[cfg(feature = "kindle")]
    pub(crate) fn is_fictionbook(&self) -> bool {
        matches!(
            self.content,
            Content::Loose {
                format: loose::Format::Fb2 | loose::Format::Fb3,
                ..
            }
        )
    }

    pub(crate) fn take_fixed_spine(&mut self) -> Option<FixedSpine> {
        Some(FixedSpine {
            right_to_left: self.spine_rtl,
            placements: self.fixed_placements.take()?,
        })
    }

    /// Return the publication's ordered reading contents.
    pub fn toc(&self) -> &[TocEntry] {
        &self.toc
    }

    pub fn printed_page_source(&self) -> Option<&str> {
        self.printed_source.as_deref()
    }

    pub fn printed_pages(&self) -> &[TocEntry] {
        &self.printed_pages
    }

    pub fn series(&self) -> &[BookSeries] {
        &self.series
    }

    /// Return the cover declaration without reading the image resource.
    pub fn cover(&self) -> Option<&BookCover> {
        self.cover.as_ref()
    }

    /// Read only the declared cover, subject to the format's byte limit.
    pub fn read_cover(&mut self) -> Result<Option<(CoverKind, Vec<u8>)>> {
        let Some(cover) = self.cover.clone() else {
            return Ok(None);
        };
        let cap = match cover.kind {
            CoverKind::Bmp
            | CoverKind::Tiff
            | CoverKind::Webp
            | CoverKind::Gif
            | CoverKind::Jpeg
            | CoverKind::Png => MAX_RASTER_COVER,
            CoverKind::Svg => crate::svg_image::MAX_SVG_BYTES,
        };
        let bytes = self.resource_limited(&cover.path, cap)?;
        if cover.kind == CoverKind::Webp {
            let converted = crate::comic::ImagePage::webp_as_png(&bytes)?;
            if converted.len() > MAX_RASTER_COVER {
                return Err("The converted EPUB cover exceeds Reader's size limit".into());
            }
            return Ok(Some((CoverKind::Png, converted)));
        }
        if cover.kind == CoverKind::Tiff {
            use image::ImageEncoder;
            let gray = crate::tiff_image::decode(&bytes)?;
            let mut converted = Vec::new();
            image::codecs::png::PngEncoder::new(&mut converted).write_image(
                &gray,
                gray.width(),
                gray.height(),
                image::ExtendedColorType::L8,
            )?;
            if converted.len() > MAX_RASTER_COVER {
                return Err("The converted EPUB cover exceeds Reader's size limit".into());
            }
            return Ok(Some((CoverKind::Png, converted)));
        }
        if cover.kind == CoverKind::Bmp {
            if !bytes.starts_with(b"BM") {
                return Err("The declared EPUB BMP cover has an invalid signature".into());
            }
            crate::epub_raster_dimensions(&bytes)?;
        }
        Ok(Some((cover.kind, bytes)))
    }

    /// This chapter is a synthetic notice rather than publisher content.
    pub(crate) fn recovered_placeholder(&self, chapter: &Chapter) -> bool {
        chapter.unsupported
            || (self.fixed_placements.is_none()
                && matches!(&self.content,
                Content::Archive { archive, .. } if archive.missing_after_recovery(&chapter.href)))
    }

    /// A manifest fallback is a different document, even at the same spine
    /// ordinal. Stock DOM/CFI positions cannot be projected by ordinal alone.
    pub(crate) fn is_manifest_fallback(&self, chapter: &Chapter) -> bool {
        matches!(&self.content, Content::Archive { fallback_paths, .. }
            if fallback_paths.contains(&chapter.href))
    }

    pub fn chapter(&mut self, index: usize) -> Result<String> {
        let chapter = self
            .chapters
            .get(index)
            .ok_or("That EPUB chapter is unavailable")?;
        if chapter.unsupported {
            return Ok(UNSUPPORTED_SPINE_NOTICE.to_owned());
        }
        if self.recovered_placeholder(chapter) {
            return Ok("<html><head><title>Chapter unavailable</title></head><body><h1>Chapter unavailable</h1><p>This chapter is missing from the damaged EPUB. Turn the page to continue.</p></body></html>".to_owned());
        }
        match &mut self.content {
            Content::Archive { archive, .. } => {
                if self.fixed_placements.is_none() {
                    if chapter.is_svg {
                        let bytes =
                            read_entry(archive, &chapter.href, crate::svg_image::MAX_SVG_BYTES)?;
                        reflow_svg_chapter(&bytes)
                    } else {
                        archive_html_recovery(archive, &chapter.href, MAX_REFLOW_CHAPTER_XML)
                    }
                } else {
                    xml_entry(archive, &chapter.href)
                }
            }
            Content::Loose { xhtml, .. } => xhtml
                .get(&chapter.href)
                .cloned()
                .ok_or_else(|| "That chapter is unavailable".into()),
            Content::Chm(chm) => chm.chapter(&chapter.href),
        }
    }

    pub(crate) fn chapter_source(
        &self,
        href: &str,
        xhtml: &str,
    ) -> crate::Result<crate::source::Source> {
        self.chapter_source_with_language(href, xhtml, self.language, true)
    }

    pub(crate) fn chapter_source_with_language(
        &self,
        href: &str,
        xhtml: &str,
        language: Option<parley::Language>,
        respect_embedded: bool,
    ) -> crate::Result<crate::source::Source> {
        if !matches!(self.content, Content::Loose { .. }) {
            return crate::source::Source::parse_with_typography(
                xhtml,
                language,
                respect_embedded,
                &Default::default(),
            );
        }
        let ordinals = self
            .toc
            .iter()
            .filter_map(|row| row.destination.as_ref())
            .filter(|destination| destination.path == href)
            .filter_map(|destination| destination.fragment.as_deref())
            .filter_map(crate::kf8::publisher_contents::element_index)
            .collect();
        let mut source = if matches!(
            self.content,
            Content::Loose {
                format: loose::Format::Odt,
                ..
            }
        ) || matches!(
            self.content,
            Content::Loose {
                format: loose::Format::Text,
                odt_history: Some(_),
                ..
            }
        ) {
            crate::source::Source::parse_odt_with_typography(
                xhtml,
                language,
                respect_embedded,
                &ordinals,
            )?
        } else {
            crate::source::Source::parse_with_typography(
                xhtml,
                language,
                respect_embedded,
                &ordinals,
            )?
        };
        if let Content::Loose {
            mobi_shadow: Some(shadow),
            ..
        } = &self.content
            && shadow.chapter == href
            && let Ok(other) = crate::source::Source::parse_with_typography(
                &shadow.xhtml,
                language,
                respect_embedded,
                &ordinals,
            )
        {
            source.apply_mobi_shadow(&other, &shadow.ids);
        }
        Ok(source)
    }

    pub(crate) fn previous_mobi_chapters(&self, name: &str) -> &[String] {
        match &self.content {
            Content::Loose { prior_mobi, .. } => {
                prior_mobi.get(name).map(Vec::as_slice).unwrap_or_default()
            }
            _ => &[],
        }
    }
    #[cfg(test)]
    pub(crate) fn previous_mobi_chapter(&self, name: &str) -> Option<&str> {
        self.previous_mobi_chapters(name)
            .first()
            .map(String::as_str)
    }

    /// Look up a canonical archive path in the reading spine.
    pub fn chapter_index(&self, path: &str) -> Option<usize> {
        self.chapters
            .iter()
            .position(|chapter| chapter.href == path)
    }

    /// Estimate a publication position from uncompressed chapter lengths.
    /// The estimate is independent of current typography and needs no chapter layout.
    #[cfg(feature = "kindle")]
    pub(crate) fn progress_target(&mut self, fraction: f64) -> Result<(usize, f64)> {
        if fraction == 0.0 {
            return Ok((0, 0.0));
        }
        if fraction == 1.0 {
            return Ok((self.chapters.len() - 1, 1.0));
        }
        let offsets = self.chapter_offsets()?;
        let position = fraction * *offsets.last().unwrap() as f64;
        let chapter = offsets
            .partition_point(|&start| start as f64 <= position)
            .saturating_sub(1)
            .min(self.chapters.len() - 1);
        let start = offsets[chapter];
        Ok((
            chapter,
            (position - start as f64) / (offsets[chapter + 1] - start) as f64,
        ))
    }

    #[cfg(feature = "kindle")]
    pub(crate) fn linear_progress_target(&mut self, fraction: f64) -> Result<(usize, f64)> {
        let offsets = self.chapter_offsets()?;
        let chapters: Vec<_> = self
            .chapters
            .iter()
            .enumerate()
            .filter(|(_, chapter)| !chapter.nonlinear)
            .map(|(index, _)| (index, offsets[index + 1] - offsets[index]))
            .collect();
        let Some(&(first, _)) = chapters.first() else {
            return self.progress_target(fraction);
        };
        let &(last, _) = chapters.last().unwrap();
        if fraction <= 0.0 {
            return Ok((first, 0.0));
        }
        if fraction >= 1.0 {
            return Ok((last, 1.0));
        }
        let total: u64 = chapters.iter().map(|(_, weight)| weight).sum();
        let mut target = fraction * total as f64;
        for (chapter, weight) in chapters {
            if target < weight as f64 {
                return Ok((chapter, target / weight as f64));
            }
            target -= weight as f64;
        }
        Ok((last, 1.0))
    }

    #[cfg(feature = "kindle")]
    pub(crate) fn chapter_offsets(&mut self) -> Result<Vec<u64>> {
        let mut offsets = Vec::with_capacity(self.chapters.len() + 1);
        let mut total = 0;
        offsets.push(total);
        for chapter in &self.chapters {
            // The unsupported member can be absent, binary, or arbitrarily large;
            // its size is not a measure of readable progress.
            if chapter.unsupported {
                total += 1;
                offsets.push(total);
                continue;
            }
            let length = match &mut self.content {
                Content::Archive { archive, .. } => {
                    archive.size(&chapter.href)?.min(MAX_XML as u64)
                }
                Content::Loose { xhtml, .. } => xhtml
                    .get(&chapter.href)
                    .ok_or("A chapter is unavailable")?
                    .len() as u64,
                Content::Chm(chm) => chm.chapter_length(&chapter.href)?,
            };
            total += length.max(1);
            offsets.push(total);
        }
        Ok(offsets)
    }

    pub(crate) fn local_font(&mut self, path: &str, cap: usize) -> Result<Vec<u8>> {
        let mut bytes = self.resource_limited(path, cap)?;
        if let Content::Archive {
            fonts: Some(fonts), ..
        } = &self.content
        {
            fonts.apply(path, &mut bytes);
        }
        Ok(bytes)
    }

    pub(crate) fn kindle_font(&mut self, number: usize, cap: usize) -> Result<Vec<u8>> {
        match &mut self.content {
            Content::Loose {
                fonts: Some(fonts), ..
            } => fonts.read(number, cap),
            _ => Err("The book has no Kindle font records".into()),
        }
    }

    /// Read a canonical, decoded archive path without extracting it to disk.
    pub fn resource(&mut self, path: &str) -> Result<Vec<u8>> {
        self.resource_limited(path, MAX_RESOURCE)
    }

    pub(crate) fn resource_limited(&mut self, path: &str, cap: usize) -> Result<Vec<u8>> {
        if !is_archive_path(path) {
            return Err("The EPUB resource path is invalid".into());
        }
        let cap = cap.min(MAX_RESOURCE);
        match &mut self.content {
            Content::Archive { archive, .. } => read_entry(archive, path, cap),
            Content::Loose { resources, .. } => {
                let bytes = resources
                    .get(path)
                    .ok_or("The standalone document has no such local resource")?;
                if bytes.len() > cap {
                    return Err(
                        format!("The document resource exceeds its size limit: {path}").into(),
                    );
                }
                Ok(bytes.clone())
            }
            Content::Chm(chm) => chm.resource_limited(path, cap),
        }
    }

    /// Resolve a URI reference against a canonical archive path.
    pub fn resolve(base: &str, relative: &str) -> Result<String> {
        let raw = relative.split(['#', '?']).next().unwrap_or_default();
        if raw.is_empty() {
            return if base.is_empty() {
                Err("An EPUB resource path is empty".into())
            } else {
                Ok(base.to_owned())
            };
        }
        if raw.starts_with('/') || raw.contains(':') {
            return Err("The EPUB resource is outside the book".into());
        }
        let mut components = base.rsplit_once('/').map_or_else(Vec::new, |(parent, _)| {
            parent.split('/').map(str::to_owned).collect()
        });
        for part in raw.split('/') {
            let part = decode_segment(part)?;
            match part.as_str() {
                "." => (),
                ".." => {
                    components
                        .pop()
                        .ok_or("The EPUB resource escapes the book")?;
                }
                path if !path.is_empty() && safe_segment(path) => components.push(part),
                _ => return Err("The EPUB resource path is invalid".into()),
            }
        }
        if components.is_empty() {
            return Err("The EPUB resource path is empty".into());
        }
        Ok(components.join("/"))
    }

    /// Resolve an in-book link against a canonical archive path and decode its fragment once.
    pub fn resolve_link(base: &str, href: &str) -> Result<Destination> {
        if !is_archive_path(base) {
            return Err("The EPUB link source path is invalid".into());
        }
        let (reference, fragment) = href.split_once('#').unwrap_or((href, ""));
        let path = Self::resolve(base, reference)?;
        let fragment = decode_segment(fragment)?;
        if fragment.chars().any(char::is_control) {
            return Err("The EPUB link fragment is invalid".into());
        }
        Ok(Destination {
            path,
            fragment: (!fragment.is_empty()).then_some(fragment),
        })
    }
}

fn child<'a, 'input>(node: Node<'a, 'input>, name: &str) -> Result<Node<'a, 'input>> {
    node.children()
        .find(|node| node.has_tag_name(name))
        .ok_or_else(|| format!("The EPUB has no {name} element").into())
}

// EPUB 3.3 §D.6.1 and the EPUB 2 cover meta convention described in §5.9.3.
fn package_cover(metadata: Node<'_, '_>, manifest: &BTreeMap<&str, Item>) -> Option<BookCover> {
    // Keep previously supported EPUB 3 / EPUB 2 cover selection first.
    package_cover_with_extra(metadata, manifest, false)
        .or_else(|| package_cover_with_extra(metadata, manifest, true))
}

/// Stock CREngine also looks for an image-only first spine item when a book
/// does not declare a cover. Keep this fallback narrow and entirely local.
fn infer_first_spine_cover(
    archive: &mut Archive,
    root: Node<'_, '_>,
    metadata: Node<'_, '_>,
    spine: Node<'_, '_>,
    manifest: &BTreeMap<&str, Item>,
) -> Option<BookCover> {
    if metadata
        .children()
        .any(|node| node.has_tag_name("meta") && node.attribute("name") == Some("cover"))
        || child(root, "manifest").ok()?.children().any(|node| {
            node.has_tag_name("item")
                && node.attribute("properties").is_some_and(|value| {
                    value
                        .split_ascii_whitespace()
                        .any(|property| property == "cover-image")
                })
        })
    {
        return None;
    }
    let mut references = spine.children().filter(|node| node.has_tag_name("itemref"));
    let first = references.next()?;
    references.next()?;
    let first_id = first.attribute("idref")?;
    let item = manifest.get(first_id)?;
    if !item.plain_href
        || !matches!(item.media.as_str(), "application/xhtml+xml" | "text/html")
        || manifest
            .values()
            .filter(|other| other.path == item.path)
            .take(2)
            .count()
            != 1
        || spine
            .children()
            .filter(|node| {
                node.has_tag_name("itemref") && node.attribute("idref") == Some(first_id)
            })
            .take(2)
            .count()
            != 1
    {
        return None;
    }
    let xhtml = archive_html_recovery(archive, &item.path, 5_000).ok()?;
    let checked = crate::epub_xml::without_plain_doctype(&xhtml);
    let document = parse_xml(&checked).ok()?;
    let root = document.root_element();
    if root.tag_name().name() != "html"
        || !matches!(
            root.tag_name().namespace(),
            None | Some("http://www.w3.org/1999/xhtml")
        )
    {
        return None;
    }
    let mut bodies = root.children().filter(|node| node.has_tag_name("body"));
    let body = bodies.next()?;
    if bodies.next().is_some() {
        return None;
    }
    let mut image = None;
    for node in body.descendants() {
        if node.is_text() && !node.text().unwrap_or("").trim().is_empty() {
            return None;
        }
        if !node.is_element() {
            continue;
        }
        if !matches!(
            node.tag_name().namespace(),
            None | Some("http://www.w3.org/1999/xhtml")
        ) || node.tag_name().name().eq_ignore_ascii_case("script")
            || node.tag_name().name().eq_ignore_ascii_case("style")
        {
            return None;
        }
        if node.has_tag_name("img") {
            if image.is_some() || node.children().any(|child| child.is_element()) {
                return None;
            }
            let src = node.attribute("src")?;
            if src.is_empty() || src.contains(['#', '?']) {
                return None;
            }
            image = Some(Book::resolve(&item.path, src).ok()?);
        }
    }
    let path = image?;
    let mut matches = manifest.values().filter(|item| item.path == path);
    let image_item = matches.next()?;
    if matches.next().is_some()
        || !image_item.plain_href
        || !matches!(
            image_item.media.as_str(),
            "image/jpeg" | "image/png" | "image/gif"
        )
    {
        return None;
    }
    cover_item(image_item, false)
}

fn package_cover_with_extra(
    metadata: Node<'_, '_>,
    manifest: &BTreeMap<&str, Item>,
    allow_extra: bool,
) -> Option<BookCover> {
    let mut marked = manifest.values().filter(|item| item.cover_image);
    if let Some(item) = marked.next() {
        if marked.next().is_some() {
            return None;
        }
        if let Some(cover) = cover_item(item, allow_extra) {
            return Some(cover);
        }
    }
    let mut legacy_ids = legacy_meta(metadata, "cover");
    let id = legacy_ids.next()?;
    if legacy_ids.any(|candidate| candidate != id) {
        return None;
    }
    cover_item(manifest.get(id)?, allow_extra)
}

fn cover_item(item: &Item, allow_extra: bool) -> Option<BookCover> {
    if item.path.len() > MAX_COVER_PATH || !is_archive_path(&item.path) {
        return None;
    }
    let kind = match item.media.as_str() {
        "image/bmp" | "image/x-ms-bmp" if allow_extra => CoverKind::Bmp,
        "image/tiff" if allow_extra => CoverKind::Tiff,
        "image/webp" if allow_extra => CoverKind::Webp,
        "image/gif" => CoverKind::Gif,
        "image/jpeg" => CoverKind::Jpeg,
        "image/png" => CoverKind::Png,
        "image/svg+xml" => CoverKind::Svg,
        _ => return None,
    };
    Some(BookCover {
        path: item.path.clone(),
        kind,
    })
}

#[derive(Default)]
struct SeriesRefinements<'a> {
    kind: Option<&'a str>,
    position: Option<&'a str>,
    ambiguous_kind: bool,
    ambiguous_position: bool,
}

// EPUB 3.3 §D.3.3–4 and §D.3.7. Refined collections describe nesting rather
// than direct publication membership.
fn package_series(metadata: Node<'_, '_>) -> Vec<BookSeries> {
    let nodes = metadata.children().filter(|node| node.has_tag_name("meta"));
    let collections = nodes
        .clone()
        .filter(|node| {
            node.attribute("property") == Some("belongs-to-collection")
                && !node.has_attribute("refines")
        })
        .filter_map(|node| {
            let id = node
                .attribute("id")
                .filter(|id| !id.is_empty() && id.len() <= 256)?;
            Some((id, node.text()?))
        })
        .take(MAX_COLLECTION_HINTS)
        .collect::<Vec<_>>();

    let mut refinements = BTreeMap::<&str, Option<SeriesRefinements<'_>>>::new();
    for &(id, _) in &collections {
        refinements
            .entry(id)
            .and_modify(|record| *record = None)
            .or_insert_with(|| Some(SeriesRefinements::default()));
    }
    for node in nodes {
        let Some(id) = node
            .attribute("refines")
            .and_then(|reference| reference.strip_prefix('#'))
        else {
            continue;
        };
        let Some(record) = refinements.get_mut(id).and_then(Option::as_mut) else {
            continue;
        };
        let Some(value) = node.text().map(str::trim).filter(|value| !value.is_empty()) else {
            continue;
        };
        let (slot, ambiguous) = match node.attribute("property") {
            Some("collection-type") if !node.has_attribute("scheme") => {
                (&mut record.kind, &mut record.ambiguous_kind)
            }
            Some("group-position") => (&mut record.position, &mut record.ambiguous_position),
            _ => continue,
        };
        *ambiguous |= slot.replace(value).is_some_and(|old| old != value);
    }

    let series: Vec<_> = collections
        .into_iter()
        .filter_map(|(id, name)| {
            let record = refinements.get(id)?.as_ref()?;
            if record.kind != Some("series") || record.ambiguous_kind {
                return None;
            }
            Some(BookSeries {
                name: series_name(name)?,
                position: record
                    .position
                    .filter(|_| !record.ambiguous_position)
                    .filter(|value| valid_group_position(value))
                    .map(str::to_owned),
            })
        })
        .take(MAX_SERIES)
        .collect();
    if !series.is_empty() {
        return series;
    }
    let Some(name) = legacy_meta(metadata, "calibre:series").find_map(series_name) else {
        return Vec::new();
    };
    let position = legacy_meta(metadata, "calibre:series_index")
        .find(|value| {
            value.len() <= MAX_SERIES_POSITION && value.parse::<f64>().is_ok_and(f64::is_finite)
        })
        .map(str::to_owned);
    vec![BookSeries { name, position }]
}

fn legacy_meta<'a>(metadata: Node<'a, '_>, name: &'a str) -> impl Iterator<Item = &'a str> {
    metadata
        .children()
        .filter(move |node| node.has_tag_name("meta") && node.attribute("name") == Some(name))
        .filter_map(|node| node.attribute("content").map(str::trim))
        .filter(|text| !text.is_empty())
}

fn series_name(value: &str) -> Option<String> {
    let mut name: String = value
        .split_whitespace()
        .flat_map(|word| word.chars().chain(std::iter::once(' ')))
        .filter(|character| !character.is_control())
        .take(MAX_SERIES_NAME)
        .collect();
    name.truncate(name.trim_end().len());
    (!name.is_empty()).then_some(name)
}

fn valid_group_position(value: &str) -> bool {
    value.len() <= MAX_SERIES_POSITION
        && value.split('.').all(|part| {
            part.bytes().all(|byte| byte.is_ascii_digit()) && part.parse::<u32>().is_ok()
        })
}

// EPUB 3.3 §8.2.2.1: https://www.w3.org/TR/epub-33/#layout
fn package_layout(metadata: Node<'_, '_>) -> Result<bool> {
    let mut declarations = metadata.children().filter(|node| {
        node.has_tag_name("meta") && node.attribute("property") == Some("rendition:layout")
    });
    let Some(declaration) = declarations.next() else {
        return Ok(false);
    };
    if declarations.next().is_some() || declaration.has_attribute("refines") {
        return Err("The EPUB has conflicting rendition:layout declarations".into());
    }
    match declaration.text().map(str::trim) {
        Some("pre-paginated") => Ok(true),
        Some("reflowable") => Ok(false),
        _ => Err("The EPUB rendition:layout value is unsupported".into()),
    }
}

fn spine_layout(reference: Node<'_, '_>, default: bool) -> Result<bool> {
    let mut layout = None;
    for property in reference
        .attribute("properties")
        .unwrap_or_default()
        .split_ascii_whitespace()
    {
        let fixed = match property {
            "rendition:layout-pre-paginated" => true,
            "rendition:layout-reflowable" => false,
            _ => continue,
        };
        if layout.replace(fixed).is_some() {
            return Err("The EPUB spine item has more than one layout override".into());
        }
    }
    Ok(layout.unwrap_or(default))
}

// EPUB 3.3 §8.2.2.3–4 and EPUB Reading Systems 3.3 §8.1.1.3–4.
fn package_spread(metadata: Node<'_, '_>) -> RenditionSpread {
    let hints = metadata
        .children()
        .filter(|node| {
            node.has_tag_name("meta")
                && node.attribute("property") == Some("rendition:spread")
                && !node.has_attribute("refines")
        })
        .filter_map(|node| rendition_spread(node.text()?.trim()));
    agreed_hint(hints, RenditionSpread::None).unwrap_or_default()
}

fn rendition_spread(value: &str) -> Option<RenditionSpread> {
    match value {
        "auto" => Some(RenditionSpread::Auto),
        "none" => Some(RenditionSpread::None),
        "landscape" => Some(RenditionSpread::Landscape),
        "both" | "portrait" => Some(RenditionSpread::Both),
        _ => None,
    }
}

fn item_placement(reference: Node<'_, '_>, default: RenditionSpread) -> ItemPlacement {
    let properties = reference.attribute("properties").unwrap_or_default();
    let spreads = properties
        .split_ascii_whitespace()
        .filter_map(|property| rendition_spread(property.strip_prefix("rendition:spread-")?));
    let sides = properties
        .split_ascii_whitespace()
        .filter_map(|property| match property {
            "page-spread-left" | "rendition:page-spread-left" => Some(SpreadSide::Left),
            "page-spread-right" | "rendition:page-spread-right" => Some(SpreadSide::Right),
            "rendition:page-spread-center" => Some(SpreadSide::Center),
            _ => None,
        });
    ItemPlacement {
        spread: agreed_hint(spreads, RenditionSpread::None).unwrap_or(default),
        side: agreed_hint(sides, SpreadSide::Center).unwrap_or_default(),
    }
}

fn agreed_hint<T: PartialEq>(mut hints: impl Iterator<Item = T>, conflict: T) -> Option<T> {
    let first = hints.next()?;
    Some(if hints.all(|hint| hint == first) {
        first
    } else {
        conflict
    })
}

// EPUB 3.3 §7.3–7.4: https://www.w3.org/TR/epub-33/#sec-nav-def-model
fn publication_navigation(
    doc: &Document<'_>,
    base: &str,
) -> (Vec<TocEntry>, Vec<TocEntry>, Vec<usize>) {
    let find = |kind: &str| {
        doc.descendants().find(|node| {
            node.has_tag_name("nav")
                && node
                    .attribute(("http://www.idpf.org/2007/ops", "type"))
                    .or_else(|| node.attribute("type"))
                    .is_some_and(|roles| roles.split_ascii_whitespace().any(|role| role == kind))
        })
    };
    let mut page_groups = Vec::new();
    let printed = find("page-list").map_or_else(Vec::new, |node| {
        xhtml_nav(node, base, true, &mut page_groups)
    });
    if let Some(node) = find("toc").or_else(|| find("landmarks")) {
        let mut groups = Vec::new();
        let contents = xhtml_nav(node, base, false, &mut groups);
        (contents, printed, groups)
    } else {
        (printed.clone(), printed, page_groups)
    }
}

fn xhtml_nav(
    toc: Node<'_, '_>,
    base: &str,
    page_list: bool,
    groups: &mut Vec<usize>,
) -> Vec<TocEntry> {
    let mut entries = Vec::new();
    let mut pending = Vec::new();
    push_xhtml_items(&mut pending, toc, 0);
    while let Some((item, depth)) = pending.pop() {
        if page_list && entries.len() == MAX_PAGE_REFERENCES {
            break;
        }
        if let Some(label) = item
            .children()
            .find(|node| node.has_tag_name("a") || node.has_tag_name("span"))
        {
            let href = label
                .has_tag_name("a")
                .then(|| label.attribute("href"))
                .flatten();
            if let Some(entry) = toc_entry(base, href, label, depth) {
                if label.has_tag_name("span") {
                    groups.push(entries.len());
                }
                entries.push(entry);
            }
        }
        push_xhtml_items(&mut pending, item, depth + 1);
    }
    entries
}

/// A publisher can name a navigation group with a span. CREngine opens its
/// first reachable child. Only inspect documents needed by a pending group.
fn inherit_group_targets(
    entries: &mut [TocEntry],
    groups: &[usize],
    spine: &BTreeMap<&str, (usize, bool)>,
    reflow_paths: &BTreeSet<&str>,
    archive: &mut Archive,
) {
    if groups.is_empty() {
        return;
    }
    let mut pending: Vec<usize> = Vec::new();
    let mut ids: BTreeMap<String, std::collections::BTreeSet<String>> = BTreeMap::new();
    let mut remaining = MAX_RESOURCE as u64;
    for index in 0..entries.len() {
        while pending
            .last()
            .is_some_and(|&parent| entries[parent].depth >= entries[index].depth)
        {
            pending.pop();
        }
        if groups.binary_search(&index).is_ok() {
            pending.push(index);
            continue;
        }
        if pending.is_empty() {
            continue;
        }
        let Some(dest) = entries[index].destination.as_ref() else {
            continue;
        };
        let Some(&(1, true)) = spine.get(dest.path.as_str()) else {
            continue;
        };
        let Ok(size) = archive.size(&dest.path) else {
            continue;
        };
        if let Some(fragment) = &dest.fragment {
            let known = ids.entry(dest.path.clone()).or_insert_with(|| {
                if size > remaining || size > MAX_XML as u64 {
                    return Default::default();
                };
                remaining -= size;
                let result = if reflow_paths.contains(dest.path.as_str()) {
                    archive_html_recovery(archive, &dest.path, MAX_XML)
                } else {
                    xml_entry(archive, &dest.path)
                };
                let Ok(xml) = result else {
                    return Default::default();
                };
                crate::source::Source::parse(&xml)
                    .map(|source| source.fragments.into_keys().collect())
                    .unwrap_or_default()
            });
            if !known.contains(fragment) {
                continue;
            }
        }
        let dest = dest.clone();
        for parent in pending.drain(..) {
            entries[parent].destination = Some(dest.clone());
        }
    }
}

fn push_xhtml_items<'a, 'input>(
    pending: &mut Vec<(Node<'a, 'input>, usize)>,
    parent: Node<'a, 'input>,
    depth: usize,
) {
    for list in parent
        .children()
        .rev()
        .filter(|node| node.has_tag_name("ol"))
    {
        pending.extend(
            list.children()
                .rev()
                .filter(|node| node.has_tag_name("li"))
                .map(|node| (node, depth)),
        );
    }
}

// OPF 2.0.1 §2.4.1: https://idpf.org/epub/20/spec/OPF_2.0.1_draft.htm#Section2.4.1
fn ncx_toc(doc: &Document<'_>, base: &str) -> Vec<TocEntry> {
    let Ok(map) = child(doc.root_element(), "navMap") else {
        return Vec::new();
    };
    let mut entries = Vec::new();
    let mut pending: Vec<_> = map.children().rev().map(|node| (node, 0)).collect();
    while let Some((point, depth)) = pending.pop() {
        if !point.has_tag_name("navPoint") {
            continue;
        }
        if let Ok(label) = child(point, "navLabel").and_then(|node| child(node, "text")) {
            let href = child(point, "content")
                .ok()
                .and_then(|node| node.attribute("src"));
            entries.extend(toc_entry(base, href, label, depth));
        }
        pending.extend(point.children().rev().map(|node| (node, depth + 1)));
    }
    entries
}

fn ncx_pages(doc: &Document<'_>, base: &str) -> Vec<TocEntry> {
    child(doc.root_element(), "pageList").map_or_else(
        |_| Vec::new(),
        |pages| {
            pages
                .children()
                .filter(|item| item.has_tag_name("pageTarget"))
                .filter_map(|target| {
                    let label = child(target, "navLabel")
                        .and_then(|label| child(label, "text"))
                        .ok()?;
                    let href = child(target, "content").ok()?.attribute("src")?;
                    toc_entry(base, Some(href), label, 0)
                })
                .take(MAX_PAGE_REFERENCES)
                .collect()
        },
    )
}

fn adobe_pages<'a>(doc: &'a Document<'a>, base: &'a str) -> impl Iterator<Item = TocEntry> + 'a {
    let root = doc.root_element();
    root.children()
        .filter(move |node| root.has_tag_name("page-map") && node.has_tag_name("page"))
        .filter_map(move |node| {
            let label = node
                .attribute("name")?
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            let destination = Book::resolve_link(base, node.attribute("href")?).ok()?;
            (!label.is_empty()).then_some(TocEntry {
                label,
                destination: Some(destination),
                depth: 0,
            })
        })
}

fn toc_entry(base: &str, href: Option<&str>, node: Node<'_, '_>, depth: usize) -> Option<TocEntry> {
    let text: String = node
        .descendants()
        .filter(Node::is_text)
        .filter_map(|node| node.text())
        .collect();
    let text = if text.trim().is_empty() {
        node.attribute("title").unwrap_or_default()
    } else {
        &text
    };
    let label = text.split_whitespace().collect::<Vec<_>>().join(" ");
    (!label.is_empty()).then(|| TocEntry {
        label,
        depth,
        destination: href.and_then(|href| Book::resolve_link(base, href).ok()),
    })
}

fn xml_entry(archive: &mut Archive, path: &str) -> Result<String> {
    xml_entry_with_declaration(archive, path, false, MAX_XML).map(|(xml, _, _)| xml)
}

/// Keep the original SVG member as the image resource. An empty relative
/// reference resolves to the owning chapter path in `Book::resolve`.
fn reflow_svg_chapter(bytes: &[u8]) -> Result<String> {
    if bytes.len() > crate::svg_image::MAX_SVG_BYTES {
        return Err("The SVG spine item exceeds the illustration limit".into());
    }
    let xml = std::str::from_utf8(bytes)?;
    let document = Document::parse_with_options(
        xml,
        ParsingOptions {
            nodes_limit: 4096,
            ..Default::default()
        },
    )?;
    if crate::svg_image::inline_source(document.root_element(), xml).is_none() {
        return Err(
            "The SVG spine item is outside the supported static illustration subset".into(),
        );
    }
    // The source-derived marker participates in the synthetic revision so a
    // changed SVG will ordinarily reject the previous chapter's anchors.
    Ok(format!(
        "<html><head><!-- SVG source {:016x} --></head><body><p><img src=\"\" alt=\"Illustration\" /></p></body></html>",
        crate::revision_bytes(bytes)
    ))
}

fn archive_html_recovery(archive: &mut Archive, path: &str, limit: usize) -> Result<String> {
    let (xml, had_declaration, safe_html_doctype) =
        xml_entry_with_declaration(archive, path, true, limit)?;
    let xml = if had_declaration {
        xml
    } else {
        crate::epub_xml::recover_html_names(xml, limit)
    };
    // The original bytes, before XML preparation, must contain either no
    // DTD/entity declaration or exactly one verified known HTML doctype.
    Ok(if !had_declaration || safe_html_doctype {
        crate::epub_xml::recover_html_voids(&xml, limit)
            .or_else(|| crate::epub_xml::recover_html_bare_ampersands(&xml, limit))
            .or_else(|| crate::epub_xml::recover_html_voids_and_ampersands(&xml, limit))
            .or_else(|| crate::epub_xml::recover_html_uppercase_voids(&xml, limit))
            .or_else(|| crate::epub_xml::recover_html_unquoted_attributes(&xml, limit))
            .or_else(|| crate::epub_xml::recover_html_boolean_attributes(&xml, limit))
            .unwrap_or(xml)
    } else {
        xml
    })
}

fn xml_entry_with_declaration(
    archive: &mut Archive,
    path: &str,
    check_declaration: bool,
    limit: usize,
) -> Result<(String, bool, bool)> {
    let mut bytes = read_entry(archive, path, limit)?;
    let utf32 = if bytes.starts_with(&[0xff, 0xfe, 0x00, 0x00]) {
        Some((true, 4, false))
    } else if bytes.starts_with(&[0x00, 0x00, 0xfe, 0xff]) {
        Some((false, 4, false))
    } else if bytes.starts_with(b"<\0\0\0?\0\0\0x\0\0\0") {
        Some((true, 0, true))
    } else if bytes.starts_with(b"\0\0\0<\0\0\0?\0\0\0x") {
        Some((false, 0, true))
    } else {
        None
    };
    let utf16 = if bytes.starts_with(&[0xff, 0xfe]) {
        Some((encoding_rs::UTF_16LE, "utf-16le", 2, false))
    } else if bytes.starts_with(&[0xfe, 0xff]) {
        Some((encoding_rs::UTF_16BE, "utf-16be", 2, false))
    } else if bytes.starts_with(b"<\0?\0x\0") {
        Some((encoding_rs::UTF_16LE, "utf-16le", 0, true))
    } else if bytes.starts_with(b"\0<\0?\0x") {
        Some((encoding_rs::UTF_16BE, "utf-16be", 0, true))
    } else {
        None
    };
    let text = if let Some((little_endian, offset, requires_declaration)) = utf32 {
        let decoded = decode_utf32_epub_xml(&bytes[offset..], little_endian)?;
        if requires_declaration
            && !(decoded.starts_with("<?xml")
                && decoded
                    .as_bytes()
                    .get(5)
                    .is_some_and(u8::is_ascii_whitespace))
        {
            return Err("The EPUB XML requires a UTF-32 declaration".into());
        }
        decoded
    } else if let Some((encoding, label, offset, requires_declaration)) = utf16 {
        let decoded = encoding
            .decode_without_bom_handling_and_without_replacement(&bytes[offset..])
            .ok_or("The EPUB XML contains invalid UTF-16")?;
        if decoded.contains('\0') {
            return Err("The EPUB XML contains binary characters".into());
        }
        let has_declaration = decoded.starts_with("<?xml")
            && decoded
                .as_bytes()
                .get(5)
                .is_some_and(u8::is_ascii_whitespace);
        if requires_declaration && !has_declaration {
            return Err("The EPUB XML requires a UTF-16 declaration".into());
        }
        if has_declaration {
            let end = decoded
                .find("?>")
                .ok_or("The EPUB XML declaration is incomplete")?
                + 2;
            if end > 1024 {
                return Err("The EPUB XML declaration is too long".into());
            }
            let declaration = &decoded[..end];
            Document::parse(&format!("{declaration}<root/>"))
                .map_err(|_| "The EPUB XML declaration is invalid")?;
            if let Some(declared) =
                loose::declared_encoding(declaration.as_bytes(), loose::Format::Fb2)
                && declared != "utf-16"
                && declared != label
            {
                return Err("The EPUB XML encoding conflicts with its byte order".into());
            }
        }
        decoded.into_owned()
    } else {
        // Keep the existing UTF-8 text and source-revision behavior exactly.
        let utf8_bom = bytes.starts_with(b"\xef\xbb\xbf");
        if utf8_bom {
            bytes.drain(..3);
        }
        match String::from_utf8(bytes) {
            Ok(text) => text,
            Err(error) if !utf8_bom => decode_legacy_epub_xml(error.as_bytes(), limit)?,
            Err(error) => return Err(error.into()),
        }
    };
    // Check the decoded original before `prepare` masks allowed declarations.
    // Otherwise a custom DTD name could be mistaken for a built-in HTML name.
    let mut doctypes = text
        .as_bytes()
        .windows(9)
        .filter(|bytes| bytes.eq_ignore_ascii_case(b"<!doctype"));
    let has_doctype = check_declaration && doctypes.next().is_some();
    let multiple_doctypes = has_doctype && doctypes.next().is_some();
    let has_entity = check_declaration
        && text
            .as_bytes()
            .windows(8)
            .any(|bytes| bytes.eq_ignore_ascii_case(b"<!entity"));
    let had_declaration = has_doctype || has_entity;
    let safe_html_doctype = has_doctype
        && !multiple_doctypes
        && !has_entity
        && (matches!(
            crate::epub_xml::without_plain_doctype(&text),
            std::borrow::Cow::Owned(_)
        ) || crate::epub_xml::known_xhtml_public_doctype(&text));
    Ok((
        crate::epub_xml::prepare(text, limit)?,
        had_declaration,
        safe_html_doctype,
    ))
}

fn decode_utf32_epub_xml(input: &[u8], little_endian: bool) -> Result<String> {
    if !input.len().is_multiple_of(4) {
        return Err("The EPUB XML contains an incomplete UTF-32 unit".into());
    }
    // A UTF-8 scalar never occupies more than its four-byte UTF-32 input.
    let mut decoded = String::with_capacity(input.len());
    for chunk in input.chunks_exact(4) {
        let bytes = [chunk[0], chunk[1], chunk[2], chunk[3]];
        let value = if little_endian {
            u32::from_le_bytes(bytes)
        } else {
            u32::from_be_bytes(bytes)
        };
        let character =
            char::from_u32(value).ok_or("The EPUB XML contains an invalid UTF-32 scalar")?;
        if character == '\0' {
            return Err("The EPUB XML contains binary characters".into());
        }
        decoded.push(character);
    }
    if decoded.starts_with("<?xml")
        && decoded
            .as_bytes()
            .get(5)
            .is_some_and(u8::is_ascii_whitespace)
    {
        let end = decoded
            .find("?>")
            .ok_or("The EPUB XML declaration is incomplete")?
            + 2;
        if end > 1024 {
            return Err("The EPUB XML declaration is too long".into());
        }
        let declaration = &decoded[..end];
        Document::parse(&format!("{declaration}<root/>"))
            .map_err(|_| "The EPUB XML declaration is invalid")?;
        let expected = if little_endian {
            "utf-32le"
        } else {
            "utf-32be"
        };
        if let Some(declared) = loose::declared_encoding(declaration.as_bytes(), loose::Format::Fb2)
            && declared != "utf-32"
            && declared != expected
        {
            return Err("The EPUB XML encoding conflicts with its byte order".into());
        }
    }
    Ok(decoded)
}

/// Compatibility with nonconforming XML that declares a legacy charset. Only
/// consulted after the unchanged UTF-8 and UTF-16 paths have failed.
fn decode_legacy_epub_xml(bytes: &[u8], limit: usize) -> Result<String> {
    if !bytes.starts_with(b"<?xml") || !bytes.get(5).is_some_and(u8::is_ascii_whitespace) {
        return Err("The EPUB XML is not valid UTF-8 and has no encoding declaration".into());
    }
    let prefix = &bytes[..bytes.len().min(1024)];
    let end = prefix
        .windows(2)
        .position(|pair| pair == b"?>")
        .ok_or("The EPUB XML declaration is incomplete or too long")?
        + 2;
    let declaration = std::str::from_utf8(&prefix[..end])?;
    Document::parse(&format!("{declaration}<root/>"))
        .map_err(|_| "The EPUB XML declaration is invalid")?;
    let label = loose::declared_encoding(declaration.as_bytes(), loose::Format::Fb2)
        .ok_or("The EPUB XML declaration has no encoding")?;
    let encoding = encoding_rs::Encoding::for_label(label.as_bytes())
        .ok_or("The EPUB XML declares an unsupported encoding")?;
    let allowed = [
        encoding_rs::GBK,
        encoding_rs::GB18030,
        encoding_rs::BIG5,
        encoding_rs::SHIFT_JIS,
        encoding_rs::EUC_JP,
        encoding_rs::EUC_KR,
        encoding_rs::WINDOWS_1250,
        encoding_rs::WINDOWS_1251,
        encoding_rs::WINDOWS_1252,
        encoding_rs::WINDOWS_1253,
        encoding_rs::WINDOWS_1254,
        encoding_rs::WINDOWS_1255,
        encoding_rs::WINDOWS_1256,
        encoding_rs::WINDOWS_1257,
        encoding_rs::WINDOWS_1258,
        encoding_rs::WINDOWS_874,
        encoding_rs::KOI8_R,
        encoding_rs::KOI8_U,
        encoding_rs::IBM866,
        encoding_rs::MACINTOSH,
        encoding_rs::X_MAC_CYRILLIC,
    ];
    if !allowed.contains(&encoding) {
        return Err("The EPUB XML declares an unsupported legacy encoding".into());
    }
    let decoded = encoding
        .decode_without_bom_handling_and_without_replacement(bytes)
        .ok_or("The EPUB XML does not match its declared encoding")?;
    if decoded.len() > limit || decoded.contains('\0') {
        return Err("The EPUB XML contains binary data or exceeds its size limit".into());
    }
    Ok(decoded.into_owned())
}

fn read_entry(archive: &mut Archive, path: &str, max_bytes: usize) -> Result<Vec<u8>> {
    archive.read(path, max_bytes)
}

fn parse_xml(xml: &str) -> Result<Document<'_>> {
    Ok(Document::parse_with_options(
        xml,
        ParsingOptions {
            nodes_limit: 80_000,
            ..ParsingOptions::default()
        },
    )?)
}

fn safe_segment(segment: &str) -> bool {
    !segment
        .chars()
        .any(|c| c.is_control() || matches!(c, '/' | '\\' | ':'))
}

pub(crate) fn is_archive_path(path: &str) -> bool {
    path.split('/')
        .all(|part| !matches!(part, "" | "." | "..") && safe_segment(part))
}

fn decode_segment(segment: &str) -> Result<String> {
    let mut bytes = Vec::with_capacity(segment.len());
    let mut input = segment.bytes();
    while let Some(byte) = input.next() {
        let value = if byte == b'%' {
            let hex = |byte: u8| char::from(byte).to_digit(16).map(|value| value as u8);
            let high = input
                .next()
                .and_then(hex)
                .ok_or("The EPUB URI contains an invalid escape")?;
            let low = input
                .next()
                .and_then(hex)
                .ok_or("The EPUB URI contains an invalid escape")?;
            high * 16 + low
        } else {
            byte
        };
        bytes.push(value);
    }
    Ok(String::from_utf8(bytes)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use zip::{ZipWriter, write::SimpleFileOptions};

    struct MetadataFixture(PathBuf);

    impl MetadataFixture {
        fn new(version: &str, metadata: &str, manifest: &str, assets: &[(&str, &[u8])]) -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!(
                "reader-epub-metadata-{}-{}.epub",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let file = File::options()
                .create_new(true)
                .write(true)
                .open(&path)
                .unwrap();
            let fixture = Self(path);
            let mut zip = ZipWriter::new(file);
            let package = format!(
                r#"<package xmlns="http://www.idpf.org/2007/opf" version="{version}">
              <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
                <dc:title>Field Notes</dc:title>{metadata}</metadata>
              <manifest><item id="chapter" href="chapter.xhtml" media-type="application/xhtml+xml"/>
                {manifest}</manifest>
              <spine><itemref idref="chapter"/></spine></package>"#
            );
            for (name, bytes) in [
                ("mimetype", b"application/epub+zip".as_slice()),
                ("META-INF/container.xml", br#"<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container" version="1.0">
                     <rootfiles><rootfile full-path="EPUB/book.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#),
                ("EPUB/book.opf", package.as_bytes()),
                ("EPUB/chapter.xhtml", br#"<html xmlns="http://www.w3.org/1999/xhtml"><body><p>Open the chapter.</p></body></html>"#),
            ] {
                zip.start_file(name, SimpleFileOptions::default().compression_method(CompressionMethod::Stored)).unwrap();
                zip.write_all(bytes).unwrap();
            }
            for &(name, bytes) in assets {
                zip.start_file(
                    name,
                    SimpleFileOptions::default().compression_method(CompressionMethod::Deflated),
                )
                .unwrap();
                zip.write_all(bytes).unwrap();
            }
            zip.finish().unwrap();
            fixture
        }
    }

    impl Drop for MetadataFixture {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }

    #[test]
    fn adobe_map_resolves_local_pages_and_preserves_distinct_repeated_labels() {
        let doc = parse_xml(r#"<page-map xmlns="http://www.idpf.org/2007/opf"><page name=" iv " href="../text/part.xhtml#p1"/><page name="iv" href="../text/part.xhtml#p2"/><page name="" href="x"/><page name="Web" href="https://example.com/"/><page name="Missing"/></page-map>"#).unwrap();
        let entries = adobe_pages(&doc, "Book/maps/pages.xml").collect::<Vec<_>>();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].label, "iv");
        assert_eq!(entries[1].label, "iv");
        for (i, entry) in entries.iter().enumerate() {
            let dest = entry.destination.as_ref().unwrap();
            assert_eq!(dest.path, "Book/text/part.xhtml");
            assert_eq!(
                dest.fragment.as_deref(),
                Some(if i == 0 { "p1" } else { "p2" })
            );
        }
    }

    #[test]
    fn reads_epub3_cover_precedence_and_typed_hierarchical_series() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="60">
            <rect width="40" height="60" fill="#000"/></svg>"##;
        let fixture = MetadataFixture::new(
            "3.0",
            r##"<meta name="cover" content="legacy"/>
                <meta name="calibre:series" content="Fallback"/>
                <meta refines="#a" property="group-position">2.2.1</meta>
                <meta id="collection" property="belongs-to-collection">A set</meta>
                <meta refines="#collection" property="collection-type">set</meta>
                <meta id="a" property="belongs-to-collection">  Coastal   Studies  </meta>
                <meta refines="#a" property="collection-type">series</meta>
                <meta refines="#collection" id="nested" property="belongs-to-collection">Nested</meta>
                <meta refines="#nested" property="collection-type">series</meta>
                <meta id="b" property="belongs-to-collection">Field Journals</meta>
                <meta refines="#b" property="collection-type">series</meta>
                <meta refines="#b" property="group-position">invalid</meta>"##,
            r#"<item id="legacy" href="other.png" media-type="image/png"/>
               <item id="publisher" href="images/Book%20cover.svg" media-type="image/svg+xml" properties="cover-image"/>"#,
            &[("EPUB/images/Book cover.svg", svg)],
        );
        let mut book = Book::open(&fixture.0).unwrap();
        assert_eq!(
            book.cover(),
            Some(&BookCover {
                path: "EPUB/images/Book cover.svg".into(),
                kind: CoverKind::Svg
            })
        );
        assert_eq!(
            book.read_cover().unwrap(),
            Some((CoverKind::Svg, svg.to_vec()))
        );
        assert_eq!(
            book.series(),
            [
                BookSeries {
                    name: "Coastal Studies".into(),
                    position: Some("2.2.1".into())
                },
                BookSeries {
                    name: "Field Journals".into(),
                    position: None
                },
            ]
        );
    }

    #[test]
    fn reads_epub3_and_ncx_printed_page_references_separately_from_contents() {
        let nav = br##"<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">
         <body><nav epub:type="toc"><ol><li><a href="chapter.xhtml">Chapter</a></li></ol></nav>
         <nav epub:type="page-list" hidden="hidden"><ol>
         <li><a href="chapter.xhtml#page-v">v</a></li>
         <li><a href="https://example.invalid/">Outside</a></li>
         </ol></nav></body></html>"##;
        let fixture = MetadataFixture::new(
            "3.0",
            "",
            r#"<item id="navigation" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>"#,
            &[("EPUB/nav.xhtml", nav)],
        );
        let book = Book::open(&fixture.0).unwrap();
        assert_eq!(book.toc()[0].label, "Chapter");
        let [printed] = book.printed_pages() else {
            panic!("Expected one local print page")
        };
        assert_eq!(printed.label, "v");
        assert_eq!(
            printed.destination.as_ref().unwrap().fragment.as_deref(),
            Some("page-v")
        );

        let ncx = parse_xml(
            r##"<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/">
          <pageList><pageTarget value="7" type="normal" id="page-7">
            <navLabel><text>7</text></navLabel><content src="chapter.xhtml#page-7"/>
          </pageTarget></pageList></ncx>"##,
        )
        .unwrap();
        assert_eq!(ncx_pages(&ncx, "EPUB/nav.ncx")[0].label, "7");
    }

    #[test]
    fn reads_epub2_calibre_cover_and_fractional_series() {
        let fixture = MetadataFixture::new(
            "2.0",
            r#"<meta name="cover" content="jacket"/>
                <meta name="calibre:series" content="  Lantern   Stories  "/>
                <meta name="calibre:series_index" content="2.5"/>"#,
            r#"<item id="jacket" href="jacket.jpg" media-type="image/jpeg"/>"#,
            &[("EPUB/jacket.jpg", b"Selected cover bytes")],
        );
        let mut book = Book::open(&fixture.0).unwrap();
        assert_eq!(book.cover().unwrap().path, "EPUB/jacket.jpg");
        assert_eq!(
            book.read_cover().unwrap(),
            Some((CoverKind::Jpeg, b"Selected cover bytes".to_vec()))
        );
        assert_eq!(
            book.series(),
            [BookSeries {
                name: "Lantern Stories".into(),
                position: Some("2.5".into())
            }]
        );
    }

    #[test]
    fn cover_reads_are_lazy_local_and_limited_by_media() {
        for (media, cap) in [
            ("image/png", MAX_RASTER_COVER),
            ("image/svg+xml", crate::svg_image::MAX_SVG_BYTES),
        ] {
            let manifest = format!(
                r#"<item id="cover" href="cover.img" media-type="{media}" properties="cover-image"/>"#
            );
            let too_large = vec![0; cap + 1];
            let fixture =
                MetadataFixture::new("3.0", "", &manifest, &[("EPUB/cover.img", &too_large)]);
            let mut book = Book::open(&fixture.0).unwrap();
            assert!(book.cover().is_some());
            assert!(
                book.read_cover()
                    .err()
                    .unwrap()
                    .to_string()
                    .contains("size limit")
            );
            assert!(book.chapter(0).unwrap().contains("Open the chapter."));

            let missing = MetadataFixture::new("3.0", "", &manifest, &[]);
            let mut book = Book::open(&missing.0).unwrap();
            assert!(book.cover().is_some());
            assert!(book.read_cover().is_err());
        }
        for href in [
            "../../outside.png",
            "https://example.invalid/cover.png",
            "/cover.png",
        ] {
            let manifest = format!(
                r#"<item id="cover" href="{href}" media-type="image/png" properties="cover-image"/>"#
            );
            let fixture = MetadataFixture::new("3.0", "", &manifest, &[]);
            let mut book = Book::open(&fixture.0).unwrap();
            assert!(book.cover().is_none());
            assert_eq!(book.read_cover().unwrap(), None);
        }
        let ambiguous = MetadataFixture::new(
            "3.0",
            "",
            r#"
            <item id="a" href="a.png" media-type="image/png" properties="cover-image"/>
            <item id="b" href="b.jpg" media-type="image/jpeg" properties="cover-image"/>"#,
            &[],
        );
        assert!(Book::open(&ambiguous.0).unwrap().cover().is_none());
    }

    #[test]
    fn series_reject_conflicting_types_and_bound_long_names_and_positions() {
        use std::fmt::Write as _;
        let mut metadata = String::from(
            r##"
            <meta id="conflicting" property="belongs-to-collection">Ambiguous</meta>
            <meta refines="#conflicting" property="collection-type">series</meta>
            <meta refines="#conflicting" property="collection-type">set</meta>"##,
        );
        for index in 0..MAX_SERIES + 2 {
            let name = format!("Volume {index} {}", "日".repeat(MAX_SERIES_NAME + 1));
            let position = "2".repeat(MAX_SERIES_POSITION + 1);
            write!(
                metadata,
                r##"<meta id="s{index}" property="belongs-to-collection">{name}</meta>
                <meta refines="#s{index}" property="collection-type">series</meta>
                <meta refines="#s{index}" property="group-position">{position}</meta>"##
            )
            .unwrap();
        }
        let fixture = MetadataFixture::new("3.0", &metadata, "", &[]);
        let book = Book::open(&fixture.0).unwrap();
        assert_eq!(book.series().len(), MAX_SERIES);
        assert!(book.series()[0].name.starts_with("Volume 0 "));
        assert_eq!(book.series()[0].name.chars().count(), MAX_SERIES_NAME);
        assert!(book.series().iter().all(|series| series.position.is_none()));
        assert!(valid_group_position("4294967295"));
        assert!(!valid_group_position("4294967296"));
        assert!(!valid_group_position("1..2"));
    }

    #[test]
    fn fixed_spread_advisories_use_standard_aliases_and_safe_conflict_defaults() {
        let xml = parse_xml(
            r#"<package><metadata><meta property="rendition:spread">portrait</meta></metadata>
                <spine><itemref properties="page-spread-left rendition:page-spread-left"/>
                <itemref properties="rendition:spread-none page-spread-right"/>
                <itemref properties="page-spread-left page-spread-right rendition:spread-unknown"/>
                </spine></package>"#,
        )
        .unwrap();
        let root = xml.root_element();
        let mode = package_spread(child(root, "metadata").unwrap());
        assert_eq!(mode, RenditionSpread::Both);
        let placements = child(root, "spine")
            .unwrap()
            .children()
            .filter(Node::is_element)
            .map(|item| item_placement(item, mode))
            .collect::<Vec<_>>();
        assert_eq!(
            placements,
            [
                ItemPlacement {
                    spread: RenditionSpread::Both,
                    side: SpreadSide::Left
                },
                ItemPlacement {
                    spread: RenditionSpread::None,
                    side: SpreadSide::Right
                },
                ItemPlacement {
                    spread: RenditionSpread::Both,
                    side: SpreadSide::Center
                },
            ]
        );
    }

    #[test]
    fn resource_limit_applies_to_archive_and_in_memory_books() {
        let mut archive = Book::open(&crate::test_fixture("field-notes-epub3.epub")).unwrap();
        let path = "EPUB/images/shore.png";
        let image = archive.resource(path).unwrap();
        assert!(archive.resource_limited(path, image.len() - 1).is_err());
        assert_eq!(archive.resource_limited(path, image.len()).unwrap(), image);

        let mut loose = Book::from_loose(
            loose::Document {
                metadata: Default::default(),
                title: "Package".into(),
                author: None,
                cover: None,
                chapter_path: "document.xhtml".into(),
                xhtml: "<html><body><p>Read.</p></body></html>".into(),
                toc: Vec::new(),
                resources: BTreeMap::from([("shore.png".into(), image.clone())]),
                fb2_note_sections: Default::default(),
            },
            loose::Format::Text,
        );
        assert!(
            loose
                .resource_limited("shore.png", image.len() - 1)
                .is_err()
        );
        assert_eq!(
            loose.resource_limited("shore.png", image.len()).unwrap(),
            image
        );
        assert!(loose.resource_limited("../shore.png", usize::MAX).is_err());
    }
}
