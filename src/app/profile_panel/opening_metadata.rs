//! Resolve publisher properties before applying a compatible opening profile.
use super::{BookConfig, FixedReading, Profile, Reader};
use crate::reading_profiles::opening::{Metadata, MetadataField::*};
use std::path::Path;

impl Reader {
    pub(in crate::app) fn opening_text_metadata(
        &self,
        config: &BookConfig,
        is_new: bool,
    ) -> Option<Metadata> {
        self.opening_needs_metadata(is_new, |profile| matches!(profile, Profile::Text { .. }))
            .then(|| text_metadata(&config.book))
    }

    pub(in crate::app) fn opening_fixed_metadata(
        &self,
        reading: &FixedReading,
        path: &Path,
        is_new: bool,
    ) -> Option<Metadata> {
        let provider = reading.profile_provider()?;
        self.opening_needs_metadata(
            is_new,
            |profile| matches!(profile, Profile::Fixed { fixed, .. } if fixed.provider == provider),
        )
        .then(|| fixed_metadata(reading, path))
    }

    fn opening_needs_metadata(&self, is_new: bool, compatible: impl Fn(&Profile) -> bool) -> bool {
        self.profiles.iter().any(|profile| {
            compatible(profile)
                && profile
                    .opening_conditions()
                    .is_some_and(|rules| (!rules.new_only || is_new) && !rules.metadata.is_empty())
        })
    }

    pub(super) fn current_opening_metadata(&self) -> Option<Metadata> {
        if let Some(active) = &self.active {
            Some(text_metadata(&active.reading.config.book))
        } else if let Some(fixed) = &self.fixed {
            Some(fixed_metadata(&fixed.reading, fixed.entry.library_path()))
        } else {
            None
        }
    }
}

fn text_metadata(path: &Path) -> Metadata {
    let mut metadata = Metadata::default();
    let book = primitive_app::with_storage(|| {
        let mut book = crate::epub::Book::open(path).map_err(std::io::Error::other)?;
        let package = book.package_xml().ok();
        Ok((book, package))
    });
    let Ok((book, package)) = book else {
        return metadata;
    };
    let package = package
        .as_deref()
        .and_then(|xml| roxmltree::Document::parse(xml).ok());
    let opf = package.as_ref().and_then(|xml| {
        xml.root_element()
            .children()
            .find(|node| node.has_tag_name("metadata"))
    });
    let title = if let Some(opf) = opf {
        opf.children()
            .find(|node| node.has_tag_name("title"))
            .and_then(|node| node.text())
            .filter(|title| !title.trim().is_empty())
            .unwrap_or_else(|| filename_title(path))
    } else {
        &book.title
    };
    metadata.add(Title, title);
    if let Some(author) = book.author.as_deref() {
        metadata.add(Authors, author);
    }
    for series in book.series() {
        metadata.add(Series, &series.name);
        if let Some(position) = &series.position {
            metadata.add(SeriesIndex, position);
        }
    }
    let publisher = opf.map(crate::publication_metadata::Metadata::opf);
    if let Some(publisher) = publisher.as_ref().or_else(|| book.publisher_metadata()) {
        if let Some(language) = &publisher.language {
            metadata.add(Language, language);
        }
        for keywords in &publisher.keywords {
            metadata.add(Keywords, keywords);
        }
        if let Some(description) = &publisher.description {
            metadata.add(Description, description);
        }
        for series in &publisher.series {
            metadata.add(Series, series);
        }
    }
    if let Some(language) = &book.language {
        metadata.add(Language, language.as_str());
    }
    metadata
}

fn fixed_metadata(reading: &FixedReading, path: &Path) -> Metadata {
    let mut metadata = Metadata::default();
    let djvu = reading.djvu_publisher_metadata();
    let title = reading
        .title()
        .or_else(|| djvu.as_ref().and_then(|props| props.title.clone()));
    let title = title.as_deref().filter(|title| !title.trim().is_empty());
    metadata.add(Title, title.unwrap_or_else(|| filename_title(path)));
    if let Some(author) = reading
        .author()
        .or_else(|| djvu.as_ref().and_then(|props| props.author.clone()))
    {
        metadata.add(Authors, &author);
    }
    if let Some((description, keywords, language)) = reading.pdf_description_fields() {
        if let Some(description) = description.as_deref() {
            metadata.add(Description, description);
        }
        if let Some(keywords) = keywords.as_deref() {
            metadata.add(Keywords, keywords);
        }
        if let Some(language) = language.as_deref() {
            metadata.add(Language, language);
        }
    }
    if let Some(djvu) = djvu {
        if let Some(description) = djvu.subject.as_deref() {
            metadata.add(Description, description);
        }
        if let Some(keywords) = djvu.keywords.as_deref() {
            metadata.add(Keywords, keywords);
        }
        for (key, value) in &djvu.extra {
            if key.eq_ignore_ascii_case("series") {
                let (name, index) = value
                    .rsplit_once(" #")
                    .filter(|(_, index)| {
                        !index.is_empty()
                            && index
                                .bytes()
                                .all(|byte| byte.is_ascii_digit() || byte == b'.')
                            && index.parse::<f64>().is_ok_and(f64::is_finite)
                    })
                    .unwrap_or((value, ""));
                metadata.add(Series, name);
                metadata.add(SeriesIndex, index);
            } else if key.eq_ignore_ascii_case("language") {
                metadata.add(Language, value);
            }
        }
    }
    metadata
}

fn filename_title(path: &Path) -> &str {
    let source = if crate::epub_members::is_folder_book(path) {
        path.parent().unwrap_or(path)
    } else {
        path
    };
    crate::loose::fallback_title(source)
}
