//! Optional content diagnostics. Reports counts and zero-based indices, never document text or paths.
use crate::{Probe, Renderer, Result, epub::Book, fixed_epub::FixedEpub, session::BookConfig};
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Default, Serialize)]
pub(crate) struct Audit {
    configuration_source: Option<&'static str>,
    session_configuration: Option<SessionConfiguration>,
    chapters: usize,
    checked: usize,
    failed_indices: Vec<usize>,
    genuine_reflow_sections: usize,
    empty_body_indices: Vec<usize>,
    empty_layout_with_markup_indices: Vec<usize>,
    recovered_placeholder_indices: Vec<usize>,
    fixed_pages: usize,
    pages_in_checked_sections: u64,
    painted_pages: u64,
    painted_pages_with_ink: u64,
    source_chars: u64,
    mapped_chars: u64,
    normal_chars: u64,
    visibility_hidden_chars: u64,
    css_omitted_chars: u64,
    missing_glyphs: u64,
    source_graphics: u64,
    decoded_graphics: u64,
    placeholder_graphics: u64,
    visibility_hidden_graphics: u64,
    css_omitted_graphics: u64,
    source_fragments: u64,
    located_fragments: u64,
    visibility_hidden_fragments: u64,
    css_omitted_fragments: u64,
    intentional_blank_painted_pages: u64,
    inspected_page_anchors: u64,
    exact_page_anchors: u64,
    issues: Vec<SectionIssues>,
}

/// Only non-document configuration facts enter public diagnostic receipts.
/// In particular, font paths, custom CSS and personal words are never serialized.
#[derive(Serialize)]
struct SessionConfiguration {
    requested_width: u32,
    requested_height: u32,
    font_size: f32,
    font_policy: &'static str,
    logical_font_size: Option<f64>,
    requested_font_pixels: Option<u32>,
    full_screen_short_side: u32,
    screen_dpi_override: Option<u16>,
    render_dpi: u16,
    percent_word_spacing: Option<crate::WordSpacePercent>,
    font_files: usize,
    publisher_styles: bool,
    user_style_snapshot: bool,
    automatic_hyphenation: bool,
    personal_hyphenation_rules: bool,
    facing_pages: bool,
    scroll: bool,
}

impl From<&BookConfig> for SessionConfiguration {
    fn from(config: &BookConfig) -> Self {
        Self {
            requested_width: config.settings.width,
            requested_height: config.settings.height,
            font_size: config.settings.font_size,
            font_policy: if config.settings.logical_font_size.is_some() { "stock_logical" } else { "legacy_pixels" },
            logical_font_size: config.settings.logical_font_size,
            requested_font_pixels: config.settings.logical_font_size.and_then(|n| config.settings.font_scale.requested(n).ok()),
            full_screen_short_side: config.settings.font_scale.short_side(),
            screen_dpi_override: config.settings.font_scale.dpi_override(),
            render_dpi: config.settings.render_dpi,
            percent_word_spacing: config.settings.percent_word_spacing,
            font_files: config.fonts.len(),
            publisher_styles: config.settings.publisher_styles,
            user_style_snapshot: !config.user_styles.is_empty(),
            automatic_hyphenation: config.settings.automatic_hyphenation,
            personal_hyphenation_rules: config.user_hyphenation.has_rules(),
            facing_pages: config.facing_pages,
            scroll: config.scroll,
        }
    }
}

#[derive(Default, PartialEq, Serialize)]
struct SectionIssues {
    index: usize,
    unmapped_chars: usize,
    missing_glyphs: usize,
    undecoded_graphics: usize,
    unresolved_fragments: usize,
    blank_painted_pages: usize,
    ink_pages_without_anchor: usize,
    page_anchor_discrepancies: usize,
    layout_limitations: Vec<String>,
    blank_page_examples: Vec<usize>,
    anchor_page_examples: Vec<(usize, Option<usize>)>,
}

fn indices(n: usize, all: bool) -> Vec<usize> {
    if all {
        return (0..n).collect();
    }
    if n == 0 {
        return Vec::new();
    }
    let mut chosen = vec![0, n / 2, n - 1];
    chosen.dedup();
    chosen
}

impl Audit {
    fn reflow(
        &mut self,
        chapter: &mut Probe,
        index: usize,
        recovered: bool,
        literal_empty_body: bool,
        all: bool,
    ) -> Result<()> {
        let mut issue = SectionIssues {
            index,
            ..Default::default()
        };
        let empty = !recovered && chapter.source.blocks.is_empty();
        let genuine = !recovered && !empty;
        if recovered {
            self.recovered_placeholder_indices.push(index);
        } else if empty {
            if literal_empty_body {
                self.empty_body_indices.push(index);
            } else {
                self.empty_layout_with_markup_indices.push(index);
            }
        } else {
            self.genuine_reflow_sections += 1;
        }
        let intent = super::css_coverage::classify(&chapter.source, &chapter.reading_flow)?;
        if genuine {
            let (mapped, total) = chapter.coverage();
            let eligible = total
                .checked_sub(intent.css_omitted_chars)
                .ok_or("CSS intent exceeds canonical source coverage")?;
            if total != intent.source_chars || mapped > eligible {
                return Err(
                    "Mapped geometry disagrees with canonical CSS source accounting".into(),
                );
            }
            self.source_chars += total as u64;
            self.mapped_chars += mapped as u64;
            self.normal_chars += intent.normal_chars as u64;
            self.visibility_hidden_chars += intent.visibility_hidden_chars as u64;
            self.css_omitted_chars += intent.css_omitted_chars as u64;
            issue.unmapped_chars = eligible - mapped;
            issue.layout_limitations = chapter.layout_limitations().to_vec();
            issue.missing_glyphs = chapter.missing_glyphs();
            self.missing_glyphs += issue.missing_glyphs as u64;
            let graphics: usize = chapter.source.blocks.iter().map(|b| b.images.len()).sum();
            let (decoded, placeholders) = match &chapter.renderer {
                Renderer::Parley(layout) => layout.illustration_counts(),
                _ => (0, 0), // Vertical layout is only selected when there are no images.
            };
            if graphics != intent.source_graphics {
                return Err("CSS graphic intent disagrees with canonical source".into());
            }
            let eligible_graphics = graphics
                .checked_sub(intent.css_omitted_graphics)
                .ok_or("CSS graphic intent exceeds canonical source")?;
            if decoded > eligible_graphics {
                return Err("Decoded graphics exceed retained source graphics".into());
            }
            self.source_graphics += graphics as u64;
            self.visibility_hidden_graphics += intent.visibility_hidden_graphics as u64;
            self.css_omitted_graphics += intent.css_omitted_graphics as u64;
            self.decoded_graphics += decoded as u64;
            self.placeholder_graphics += placeholders as u64;
            issue.undecoded_graphics = eligible_graphics - decoded;
            self.source_fragments += chapter.source.fragments.len() as u64;
            let mut resolved = 0;
            let mut omitted = 0;
            for id in chapter.source.fragments.keys() {
                match chapter.fragment_presentation(id) {
                    Some(crate::SourcePresentation::CssOmitted) => {
                        omitted += 1;
                        continue;
                    }
                    Some(crate::SourcePresentation::VisibilityHidden) => {
                        self.visibility_hidden_fragments += 1;
                    }
                    _ => {}
                }
                resolved += usize::from(
                    chapter
                        .fragment(id)
                        .and_then(|a| chapter.page_for(&a))
                        .is_some(),
                );
            }
            self.css_omitted_fragments += omitted as u64;
            self.located_fragments += resolved as u64;
            issue.unresolved_fragments = chapter.source.fragments.len() - omitted - resolved;
        }
        let pages = chapter.page_count();
        self.pages_in_checked_sections += pages as u64;
        for page in indices(pages, all) {
            let pixels = chapter.capture(page)?;
            self.painted_pages += 1;
            let ink = pixels.iter().any(|&v| v < 255);
            self.painted_pages_with_ink += u64::from(ink);
            if genuine {
                // Only a wholly unpainted chapter has a proven blank-frame
                // explanation here. Mixed chapters retain their blank-page
                // issues until a finer per-page paint oracle is available.
                let paintable_graphics = intent.source_graphics
                    - intent.css_omitted_graphics
                    - intent.visibility_hidden_graphics;
                let intentional_blank = !ink && intent.normal_chars == 0 && paintable_graphics == 0;
                self.intentional_blank_painted_pages += u64::from(intentional_blank);
                issue.blank_painted_pages += usize::from(!ink && !intentional_blank);
                if !ink && !intentional_blank && issue.blank_page_examples.len() < 8 {
                    issue.blank_page_examples.push(page);
                }
                match chapter.page_anchor(page) {
                    Some(anchor) => {
                        self.inspected_page_anchors += 1;
                        let mapped = chapter.page_for(&anchor);
                        let same = mapped == Some(page);
                        if !same && issue.anchor_page_examples.len() < 8 {
                            issue.anchor_page_examples.push((page, mapped));
                        }

                        self.exact_page_anchors += u64::from(same);
                        issue.page_anchor_discrepancies += usize::from(!same);
                    }
                    None => issue.ink_pages_without_anchor += usize::from(ink),
                }
            }
        }
        if issue
            != (SectionIssues {
                index,
                ..Default::default()
            })
        {
            self.issues.push(issue);
        }
        Ok(())
    }
}

// Check the supplied chapter rather than treating an all-white frame as proof of a blank leaf.
fn literal_empty_body(markup: &str) -> bool {
    let xml = crate::loose::without_plain_html_doctype(markup);
    roxmltree::Document::parse_with_options(
        &xml,
        roxmltree::ParsingOptions {
            nodes_limit: 100_000,
            ..Default::default()
        },
    )
    .ok()
    .and_then(|document| {
        document
            .root_element()
            .children()
            .find(|node| node.has_tag_name("body"))
            .map(|body| {
                body.attributes().len() == 0
                    && body.children().all(|child| {
                        !child.is_element()
                            && (!child.is_text()
                                || child.text().unwrap_or_default().trim().is_empty())
                    })
            })
    })
    .unwrap_or(false)
}

pub(crate) fn verify_spine(
    path: &Path,
    fonts: &[PathBuf],
    mode: &str,
    session: Option<&BookConfig>,
) -> Result<Audit> {
    let (all_chapters, all_pages) = match mode {
        "sample" => (false, false),
        "all-sections" => (true, false),
        "all" => (true, true),
        _ => return Err("Use sample, all-sections or all for READER_VERIFY_SPINE".into()),
    };
    let package = Book::open(path)?;
    let count = package.chapters.len();
    let is_fixed = package.is_fixed_layout();
    let placeholders: Vec<_> = package
        .chapters
        .iter()
        .map(|entry| package.recovered_placeholder(entry))
        .collect();
    drop(package);
    let fixed = if is_fixed {
        FixedEpub::open(path, fonts)?
    } else {
        None
    };
    let config = if fixed.is_some() {
        None
    } else {
        let config =
            session.ok_or("Reflow spine verification requires the resumed Reader configuration")?;
        // Reader stores the resolved library path, while host diagnostics may
        // be invoked with the caller's spelling of the same file. On macOS,
        // for example, `temp_dir()` commonly returns `/var/...` while the
        // library root is canonicalized to `/private/var/...`. Compare file
        // identity after resolving aliases so a valid resumed session is not
        // rejected by a harmless path spelling difference.
        let configured_book = config.book.canonicalize()?;
        let requested_book = path.canonicalize()?;
        if configured_book != requested_book {
            return Err("Reflow spine configuration belongs to a different publication".into());
        }
        Some(config)
    };
    let mut audit = Audit {
        configuration_source: Some(if fixed.is_some() {
            "fixed_epub"
        } else {
            "resumed_reader"
        }),
        session_configuration: config.map(SessionConfiguration::from),
        chapters: count,
        ..Default::default()
    };
    for index in indices(count, all_chapters) {
        audit.checked += 1;
        let result = (|| -> Result<()> {
            if let Some(book) = &fixed {
                let pixels = book.decode_page(index)?.fit(
                    primitive_app::WIDTH,
                    crate::session::PAGE_HEIGHT,
                    0,
                )?;
                audit.fixed_pages += 1;
                audit.pages_in_checked_sections += 1;
                audit.painted_pages += 1;
                audit.painted_pages_with_ink += u64::from(pixels.iter().any(|&v| v < 255));
            } else {
                // BookConfig owns the application-selected styles, fonts,
                // hyphenation and spread fallback. Never reconstruct defaults.
                let mut chapter = config
                    .ok_or("Missing resumed reflow configuration")?
                    .chapter(index)?;
                let literal_empty = chapter.source.blocks.is_empty()
                    && !placeholders[index]
                    && literal_empty_body(&Book::open(path)?.chapter(index)?);
                audit.reflow(
                    &mut chapter,
                    index,
                    placeholders[index],
                    literal_empty,
                    all_pages,
                )?;
            }
            Ok(())
        })();
        if result.is_err() {
            audit.failed_indices.push(index);
        }
    }
    Ok(audit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Engine, Settings};

    // An intentionally scoped legacy configuration for low-level source recovery
    // fixtures. The public verifier must receive the actual Reader snapshot.
    fn legacy_test_config(path: &Path, fonts: &[PathBuf]) -> BookConfig {
        BookConfig {
            book: path.to_owned(),
            fonts: fonts.to_vec(),
            engine: Engine::Parley,
            settings: Settings {
                width: primitive_app::WIDTH,
                height: crate::session::PAGE_HEIGHT,
                ..Settings::default()
            },
            user_styles: Default::default(),
            user_hyphenation: Default::default(),
            scroll: false,
            facing_pages: false,
            hide_nonlinear: false,
            hidden_flows: Default::default(),
            chapter_offsets: Default::default(),
        }
    }

    fn styled_probe(body: &str) -> Probe {
        Probe::from_book_content(
            Book::open(&crate::test_fixture("field-notes-epub3.epub")).unwrap(),
            Engine::Parley,
            &[crate::test_fixture("fonts/NotoSans-Regular.ttf")],
            Settings {
                width: 600,
                height: 800,
                ..Default::default()
            },
            0,
            Some(format!("<html><body>{body}</body></html>")),
        )
        .unwrap()
    }

    #[test]
    fn css_omissions_are_reported_separately_from_mapped_source_and_fragment_geometry() {
        let mut chapter = styled_probe(
            "<p id='visible'><span id='hidden' style='display:none'>SECRET</span>Shown</p><p id='unpainted' style='visibility:hidden'>GHOST</p>",
        );
        let mut audit = Audit::default();
        audit.reflow(&mut chapter, 0, false, false, true).unwrap();
        assert_eq!(audit.source_chars, 16);
        assert_eq!(audit.normal_chars, 5);
        assert_eq!(audit.visibility_hidden_chars, 5);
        assert_eq!(audit.css_omitted_chars, 6);
        assert_eq!(audit.mapped_chars, 10);
        assert_eq!(audit.source_fragments, 3);
        assert_eq!(audit.located_fragments, 2);
        assert_eq!(audit.css_omitted_fragments, 1);
        assert_eq!(audit.visibility_hidden_fragments, 1);
        assert!(audit.issues.is_empty());
    }

    #[test]
    fn all_hidden_chapters_keep_the_original_denominator_and_an_explicit_white_frame_reason() {
        for (style, omitted) in [("display:none", true), ("visibility:hidden", false)] {
            let mut chapter = styled_probe(&format!("<p id='note' style='{style}'>Alpha</p>"));
            let mut audit = Audit::default();
            audit.reflow(&mut chapter, 0, false, false, true).unwrap();
            assert_eq!(audit.source_chars, 5);
            assert_eq!(audit.mapped_chars, if omitted { 0 } else { 5 });
            assert_eq!(audit.css_omitted_chars, if omitted { 5 } else { 0 });
            assert_eq!(audit.visibility_hidden_chars, if omitted { 0 } else { 5 });
            assert_eq!(audit.css_omitted_fragments, u64::from(omitted));
            assert!(audit.intentional_blank_painted_pages > 0);
            assert_eq!(audit.painted_pages_with_ink, 0);
            assert!(audit.issues.is_empty());
        }
    }

    #[test]
    fn visibility_geometry_loss_is_still_an_unmapped_source_failure() {
        let mut chapter = styled_probe("<p style='visibility:hidden'>Alpha</p>");
        // Simulate a consumer losing lines without deleting any canonical source.
        chapter.lines.clear();
        let mut audit = Audit::default();
        audit.reflow(&mut chapter, 0, false, false, true).unwrap();
        assert_eq!(audit.source_chars, 5);
        assert_eq!(audit.visibility_hidden_chars, 5);
        assert_eq!(audit.css_omitted_chars, 0);
        assert_eq!(audit.mapped_chars, 0);
        assert_eq!(audit.issues[0].unmapped_chars, 5);
    }

    #[test]
    fn a_white_leaf_requires_an_empty_source_body() {
        assert!(literal_empty_body(
            "<html><head/><body>  <!-- printer leaf --> </body></html>"
        ));
        for markup in [
            "<html><body><img src='missing.png'/></body></html>",
            "<html><body><p> </p></body></html>",
            "<html><body style='background: black'/></html>",
            "<html><body>Chapter unavailable</body></html>",
        ] {
            assert!(!literal_empty_body(markup));
        }
    }

    #[test]
    fn a_recovery_notice_does_not_inflate_source_text_or_anchor_coverage() {
        let fixture = crate::test_fixture("field-notes-epub2.epub");
        let fonts = [crate::test_fixture("fonts/NotoSans-Regular.ttf")];
        let package = Book::open(&fixture).unwrap();
        let mut bytes = std::fs::read(&fixture).unwrap();
        let lost = {
            let mut zip = zip::ZipArchive::new(std::io::Cursor::new(&bytes)).unwrap();
            let entry = zip.by_name(&package.chapters[0].href).unwrap();
            entry.header_start() as usize..(entry.data_start() + entry.compressed_size()) as usize
        };
        bytes.drain(lost);
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let file =
            std::env::temp_dir().join(format!("reader-spine-{}-{nonce}.epub", std::process::id()));
        std::fs::write(&file, &bytes).unwrap();
        let config = legacy_test_config(&file, &fonts);
        let result = verify_spine(&file, &fonts, "all", Some(&config));
        std::fs::remove_file(&file).unwrap();
        let audit = result.unwrap();
        assert!(audit.failed_indices.is_empty());
        assert_eq!(audit.recovered_placeholder_indices, [0]);
        assert_eq!(audit.genuine_reflow_sections, 1);
        let surviving =
            Probe::open(&fixture, Engine::Parley, &fonts, Settings::default(), 1).unwrap();
        let (covered, total) = surviving.coverage();
        assert_eq!(
            (audit.mapped_chars, audit.source_chars),
            (covered as u64, total as u64)
        );
        assert_eq!(audit.painted_pages, audit.pages_in_checked_sections);
        assert!(audit.painted_pages_with_ink > 0);
    }

    #[cfg(unix)]
    #[test]
    fn canonical_publication_aliases_bind_to_the_same_resumed_session() {
        let fixture=crate::test_fixture("field-notes-epub3.epub");
        let fonts=[crate::test_fixture("fonts/NotoSans-Regular.ttf")];
        let nonce=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let alias=std::env::temp_dir().join(format!("reader-spine-alias-{}-{nonce}.epub",std::process::id()));
        std::os::unix::fs::symlink(&fixture,&alias).unwrap();
        let config=legacy_test_config(&fixture.canonicalize().unwrap(),&fonts);
        let audit=verify_spine(&alias,&fonts,"sample",Some(&config));
        std::fs::remove_file(alias).unwrap();assert!(audit.unwrap().failed_indices.is_empty());
        #[cfg(target_os="macos")]
        {
            // Exercise the actual system alias which triggered the host failure,
            // independently of the process's possibly already canonical TMPDIR.
            let path=std::path::PathBuf::from(format!("/var/tmp/reader-spine-{}-{nonce}.epub",std::process::id()));
            std::fs::copy(&fixture,&path).unwrap();let canonical=path.canonicalize().unwrap();
            assert!(canonical.starts_with("/private/var/tmp"));assert_ne!(path,canonical);
            let config=legacy_test_config(&canonical,&fonts);
            let audit=verify_spine(&path,&fonts,"sample",Some(&config));
            std::fs::remove_file(path).unwrap();assert!(audit.unwrap().failed_indices.is_empty());
        }
    }

    #[test]
    fn reflow_requires_the_bound_session_and_never_falls_back_to_defaults() {
        let path = crate::test_fixture("field-notes-epub3.epub");
        let fonts = [crate::test_fixture("fonts/NotoSans-Regular.ttf")];
        let error = verify_spine(&path, &fonts, "sample", None).err().unwrap();
        assert!(
            error
                .to_string()
                .contains("requires the resumed Reader configuration")
        );
        let mut wrong = legacy_test_config(&path, &fonts);
        wrong.book = crate::test_fixture("field-notes-epub2.epub");
        let error = verify_spine(&path, &fonts, "sample", Some(&wrong))
            .err()
            .unwrap();
        assert!(error.to_string().contains("different publication"));
        assert!(!error.to_string().contains(&path.display().to_string()));
    }
}

#[cfg(test)]
mod logical_configuration_tests {
    use super::*;
    use crate::{Engine,Settings};
    #[test]
    fn configured_receipt_distinguishes_raw_requested_effective_and_legacy_units_without_paths() {
        let mut config=BookConfig {
            user_styles:Default::default(),user_hyphenation:Default::default(),engine:Engine::Parley,
            book:std::path::PathBuf::from("/private/publication.epub"),fonts:vec![std::path::PathBuf::from("/private/font.ttf")],
            settings:Settings::default().with_logical_font(255.).unwrap(),scroll:false,facing_pages:true,
            hide_nonlinear:false,hidden_flows:Default::default(),chapter_offsets:Default::default(),
        };
        let json=serde_json::to_value(SessionConfiguration::from(&config)).unwrap();
        assert_eq!(json["font_policy"],"stock_logical");assert_eq!(json["logical_font_size"],255.);
        assert_eq!(json["requested_font_pixels"],538);assert_eq!(json["font_size"],340.);
        assert_eq!(json["full_screen_short_side"],1264);assert!(json["screen_dpi_override"].is_null());
        assert!(!json.to_string().contains("private"));
        config.settings=Settings{font_size:42.25,..Settings::default()};
        let json=serde_json::to_value(SessionConfiguration::from(&config)).unwrap();
        assert_eq!(json["font_policy"],"legacy_pixels");assert!(json["logical_font_size"].is_null());
        assert!(json["requested_font_pixels"].is_null());assert_eq!(json["font_size"],42.25);
    }
}
