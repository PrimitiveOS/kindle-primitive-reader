use super::*;
use crate::app::selection_viewport_tests::{adopt, capture, tap};
use crate::app::tests::Fixture;
use crate::app::{BarPanel, Screen};
use crate::session::ReadingViewport;
use primitive_app::App;
use std::{fs, io::Write, path::Path};

pub(super) fn write_reference_book(path: &Path, variant: &str) {
    let source = crate::test_fixture("publisher-faces.epub");
    let mut original = zip::ZipArchive::new(fs::File::open(source).unwrap()).unwrap();
    let mut epub = zip::ZipWriter::new(fs::File::create(path).unwrap());
    let options = zip::write::SimpleFileOptions::default();
    let document = r#"<html xmlns="http://www.w3.org/1999/xhtml">
<head><title>Harbor lettering</title><link rel="stylesheet" type="text/css" href="../styles/reader.css"/></head>
<body><p id="embedded" class="embedded">Harbor lettering</p><p id="installed" class="installed">Signal letters</p>
<p id="ordinary">Ordinary type</p><p id="style" class="proof">Printed inset</p></body></html>"#;
    let embedded = match variant {
        "missing" => r#"@font-face {font-family: Voyage; src: url("../fonts/missing.ttf");}"#,
        "inline" | "unused" => "",
        _ => r#"@font-face {font-family: Voyage; src: url("../fonts/Silkscreen-Regular.ttf");}"#,
    };
    let unused: &[&str] = match variant {
        "report" => &["Quarry"],
        "paged-report" => &["Anchor", "Beacon", "Channel", "Dune", "Ferry", "Harbor"],
        _ => &[],
    };
    let unused = unused.iter().map(|family| {
        format!(r#"@font-face {{font-family: {family}; src: url("../fonts/NotoSans-Regular.ttf");}}"#)
    }).collect::<Vec<_>>().join("
");
    let css = format!(
        r#"{embedded}
{unused}
    .embedded {{font-family: Voyage, "Noto Sans", sans-serif;}}
    .installed {{font-family: Silkscreen, "Noto Sans", sans-serif;}}
    p.proof {{ margin-left: 80px; font-size: 145%; }}"#
    );
    for index in 0..original.len() {
        let mut item = original.by_index(index).unwrap();
        let name = item.name().to_owned();
        epub.start_file(&name, options).unwrap();
        match name.as_str() {
            "EPUB/package.opf" => {
                let mut package = String::new();
                io::Read::read_to_string(&mut item, &mut package).unwrap();
                package = package.replace(
                    "  </manifest>",
                    "    <item id=\"proof\" href=\"chapters/proof.xhtml\" media-type=\"application/xhtml+xml\"/>\n  </manifest>",
                );
                package = package.replace("  </spine>", "    <itemref idref=\"proof\"/>\n  </spine>");
                epub.write_all(package.as_bytes()).unwrap();
            }
            "EPUB/chapters/shore.xhtml" => epub
                .write_all(b"<html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>Preface</title></head><body><p>Opening the typesetter's notebook.</p></body></html>")
                .unwrap(),
            "EPUB/styles/reader.css" => epub.write_all(css.as_bytes()).unwrap(),
            _ => {
                io::copy(&mut item, &mut epub).unwrap();
            }
        }
    }
    epub.start_file("EPUB/chapters/proof.xhtml", options)
        .unwrap();
    let document = if variant == "inline" {
        document.replace(
            "</head>",
            r#"<style>@font-face {font-family: Voyage; src: url("../fonts/Silkscreen-Regular.ttf");}</style></head>"#,
        )
    } else {
        document.to_owned()
    };
    epub.write_all(document.as_bytes()).unwrap();
    epub.finish().unwrap();
}

pub(super) fn reader(fixture: &Fixture) -> Reader {
    let fonts = ["NotoSans-Regular.ttf", "Silkscreen-Regular.ttf"]
        .map(|name| crate::test_fixture(&format!("fonts/{name}")));
    Reader::open(&fixture.root, &fixture.state, fonts.to_vec()).unwrap()
}

pub(super) fn open_rendering(app: &mut Reader) {
    app.last_text_panel = BarPanel::TextRendering;
    app.open_reading_edge(false, 520.0);
    assert_eq!(app.reading_panel, Some(BarPanel::TextRendering));
}

fn font_setting(app: &Reader) -> bool {
    app.active
        .as_ref()
        .unwrap()
        .reading
        .config
        .settings
        .embedded_fonts
}

#[test]
fn embedded_fonts_control_handles_remote_availability_and_preserves_independent_preferences() {
    let fixture = Fixture::new();
    let path = fixture.root.join("Harbor specimen.epub");
    write_reference_book(&path, "manifest");
    if let Some(destination) = std::env::var_os("READER_EMBEDDED_FONT_STOCK_FIXTURE") {
        fs::copy(&path, destination).unwrap();
    }
    let original = fs::read(&path).unwrap();

    let mut app = reader(&fixture);
    app.open_path(&path).unwrap();
    assert!(font_setting(&app));
    assert!(
        app.active
            .as_ref()
            .unwrap()
            .reading
            .probe
            .embedded_font_usage
            .is_empty()
    );
    let original_anchor = app.current_history_location();
    let mut canvas = adopt(&mut app, ReadingViewport::PORTRAIT, 1);
    open_rendering(&mut app);
    capture(&app, &mut canvas, "embedded-fonts-available.png", false);
    let off = app.reading_button_in_row("Embedded fonts", "Off");
    tap(&mut app, off);
    assert!(!font_setting(&app));
    assert_eq!(app.current_history_location(), original_anchor);
    assert_eq!(app.reading_panel, Some(BarPanel::TextRendering));
    let styles_off = app.reading_button_in_row("Publisher CSS", "Off");
    tap(&mut app, styles_off);
    assert_eq!(
        app.reading_information_in_row("Embedded fonts"),
        Some("Turn on Publisher CSS")
    );
    let styles_on = app.reading_button_in_row("Publisher CSS", "On");
    tap(&mut app, styles_on);
    assert!(!font_setting(&app));
    app.save().unwrap();

    let mut reopened = reader(&fixture);
    reopened.open_path(&path).unwrap();
    assert!(!font_setting(&reopened));
    assert!(
        reopened
            .active
            .as_ref()
            .unwrap()
            .reading
            .config
            .settings
            .publisher_styles
    );
    let mut wide = adopt(&mut reopened, ReadingViewport::LANDSCAPE, 1);
    open_rendering(&mut reopened);
    capture(
        &reopened,
        &mut wide,
        "embedded-fonts-off-landscape.png",
        false,
    );
    let on = reopened.reading_button_in_row("Embedded fonts", "On");
    tap(&mut reopened, on);
    assert!(font_setting(&reopened));
    assert_eq!(fs::read(&path).unwrap(), original);
}

#[test]
fn embedded_fonts_inventory_distinguishes_inline_declarations_missing_assets_and_unused_files() {
    let fixture = Fixture::new();
    for (variant, expected) in [("inline", true), ("missing", false), ("unused", false)] {
        let path = fixture.root.join(format!("{variant}.epub"));
        write_reference_book(&path, variant);
        let mut book = Book::open(&path).unwrap();
        assert_eq!(
            BookStyle::publication_has_embedded_fonts(&mut book, 0),
            expected,
            "{variant}"
        );
        assert!(!BookStyle::chapter_has_embedded_fonts(&mut book, 0));
        assert_eq!(
            BookStyle::chapter_has_embedded_fonts(&mut book, 1),
            expected
        );
    }
    let mut app = reader(&fixture);
    app.open_path(&fixture.root.join("unused.epub")).unwrap();
    assert_eq!(app.screen, Screen::Book);
    let mut canvas = adopt(&mut app, ReadingViewport::PORTRAIT, 1);
    open_rendering(&mut app);
    assert_eq!(
        app.reading_information_in_row("Embedded fonts"),
        Some("No embedded fonts found")
    );
    capture(&app, &mut canvas, "embedded-fonts-unavailable.png", false);
}

#[test]
fn embedded_fonts_woff_discovery_uses_bounded_chapter_layout() {
    let mut book = Book::open(&crate::test_fixture("publisher-woff-woff.epub")).unwrap();
    assert!(book.may_have_embedded_fonts());
    assert!(!book.supports_eager_typography());
    assert!(BookStyle::publication_has_embedded_fonts(&mut book, 0));
    assert!(BookStyle::chapter_has_embedded_fonts(&mut book, 0));
    let catalog = BookStyle::embedded_font_catalog(&mut book, 0);
    assert!(catalog.iter().any(|name| name.as_ref() == "Voyage"));
}

#[test]
fn embedded_fonts_woff_matches_ttf_and_bad_woff_falls_back() {
    use crate::{Engine, Probe, Settings};
    let fonts = [crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut frames = Vec::new();
    for kind in ["ttf", "woff", "fallback"] {
        let path = crate::test_fixture(&format!("publisher-woff-{kind}.epub"));
        let before = fs::read(&path).unwrap();
        let open = |embedded_fonts| {
            Probe::open(
                &path,
                Engine::Parley,
                &fonts,
                Settings {
                    embedded_fonts,
                    ..Settings::default()
                },
                0,
            )
            .unwrap()
        };
        let mut on = open(true);
        let mut off = open(false);
        assert!(!on.embedded_font_usage.is_empty());
        assert!(off.embedded_font_usage.is_empty());
        let start = on.matches("Harbor lettering").remove(0);
        let mut end = start.clone();
        end.offset += "Harbor lettering".len();
        let selection = on.select_range(&start, &end).unwrap();
        assert_eq!(selection.text, "Harbor lettering");
        let page = on.page_for(&start).unwrap();
        assert!(!on.selection_rects(page, &selection).is_empty());
        assert_eq!(off.restore_anchor(&start), Some(start.clone()));
        let on_pixels = on.capture(page).unwrap();
        let off_pixels = off.capture(off.page_for(&start).unwrap()).unwrap();
        assert_ne!(on_pixels, off_pixels);
        if kind == "woff"
            && let Some(directory) = std::env::var_os("READER_SELECTION_VIEWPORT_CAPTURES")
        {
            for (name, pixels) in [
                ("woff-reader-on.png", &on_pixels),
                ("woff-reader-off.png", &off_pixels),
            ] {
                primitive_app::write_png(
                    &Path::new(&directory).join(name),
                    on.settings.width,
                    on.settings.height,
                    pixels,
                )
                .unwrap();
            }
        }
        assert_eq!(fs::read(&path).unwrap(), before);
        frames.push(on_pixels);
    }
    assert_eq!(frames[0], frames[1]);
    assert_eq!(frames[0], frames[2]);
}

#[test]
fn embedded_fonts_woff_control_preserves_location_and_saved_preference() {
    let fixture = Fixture::new();
    let path = fixture.root.join("WOFF specimen.epub");
    fs::copy(crate::test_fixture("publisher-woff-woff.epub"), &path).unwrap();
    let original = fs::read(&path).unwrap();
    let mut app = reader(&fixture);
    app.open_path(&path).unwrap();
    let anchor = app.current_history_location();
    let mut canvas = adopt(&mut app, ReadingViewport::PORTRAIT, 1);
    open_rendering(&mut app);
    let off = app.reading_button_in_row("Embedded fonts", "Off");
    tap(&mut app, off);
    assert!(!font_setting(&app));
    assert_eq!(app.current_history_location(), anchor);
    capture(&app, &mut canvas, "woff-reader-controls.png", false);
    app.save().unwrap();
    let mut reopened = reader(&fixture);
    reopened.open_path(&path).unwrap();
    assert!(!font_setting(&reopened));
    assert_eq!(reopened.current_history_location(), anchor);
    open_rendering(&mut reopened);
    let on = reopened.reading_button_in_row("Embedded fonts", "On");
    tap(&mut reopened, on);
    assert!(font_setting(&reopened));
    assert_eq!(reopened.current_history_location(), anchor);
    assert_eq!(fs::read(&path).unwrap(), original);
}

#[test]
fn embedded_fonts_toggle_keeps_installed_families_and_other_publisher_styles() {
    use crate::{Engine, Probe, Settings};
    let fixture = Fixture::new();
    let path = fixture.root.join("Font measurement.epub");
    write_reference_book(&path, "manifest");
    let fonts = ["NotoSans-Regular.ttf", "Silkscreen-Regular.ttf"]
        .map(|name| crate::test_fixture(&format!("fonts/{name}")));
    let measure = |probe: &Probe, text: &str| {
        let start = probe.matches(text).remove(0);
        let mut end = start.clone();
        end.offset += text.len();
        let selection = probe.select_range(&start, &end).unwrap();
        assert_eq!(selection.text, text);
        let page = probe.page_for(&start).unwrap();
        let rectangles = probe.selection_rects(page, &selection);
        assert_eq!(rectangles.len(), 1);
        (rectangles[0].x, rectangles[0].width)
    };
    let open = |embedded_fonts, publisher_styles, override_fonts| {
        Probe::open(
            &path,
            Engine::Parley,
            &fonts,
            Settings {
                embedded_fonts,
                publisher_styles,
                override_fonts,
                ..Settings::default()
            },
            1,
        )
        .unwrap()
    };
    let mut on = open(true, true, false);
    let mut off = open(false, true, false);
    assert!(!on.embedded_font_usage.is_empty() && off.embedded_font_usage.is_empty());
    assert_ne!(
        measure(&on, "Harbor lettering"),
        measure(&off, "Harbor lettering")
    );
    for phrase in ["Signal letters", "Ordinary type", "Printed inset"] {
        assert_eq!(measure(&on, phrase), measure(&off, phrase), "{phrase}");
    }
    let no_styles = open(false, false, false);
    assert_ne!(
        measure(&off, "Printed inset"),
        measure(&no_styles, "Printed inset")
    );
    let override_all = open(false, true, true);
    assert_ne!(
        measure(&off, "Signal letters"),
        measure(&override_all, "Signal letters")
    );
    let same_source = on.fragment("embedded").unwrap();
    assert_eq!(off.fragment("embedded").unwrap(), same_source);
    let first = on.capture(on.page_for(&same_source).unwrap()).unwrap();
    let without_embedded = off.capture(off.page_for(&same_source).unwrap()).unwrap();
    assert_ne!(first, without_embedded);
    if let Some(output) = std::env::var_os("READER_SELECTION_VIEWPORT_CAPTURES") {
        let output = std::path::PathBuf::from(output);
        for (name, pixels) in [
            ("embedded-fonts-reader-on.png", &first),
            ("embedded-fonts-reader-off.png", &without_embedded),
        ] {
            let settings = Settings::default();
            primitive_app::write_png(&output.join(name), settings.width, settings.height, pixels)
                .unwrap();
        }
    }
    let mut restored = open(true, true, false);
    assert_eq!(
        first,
        restored
            .capture(restored.page_for(&same_source).unwrap())
            .unwrap()
    );
}
