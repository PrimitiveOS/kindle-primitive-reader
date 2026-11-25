use super::*;
use std::fs;
use std::io::{Cursor, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use zip::{ZipWriter, write::SimpleFileOptions};

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "reader-odt-whitespace-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn actual_odt_spaces_tabs_and_selection_use_the_original_characters() {
    let scratch = Scratch::new();
    let path = scratch.0.join("spaces.odt");
    let xml = format!(
        r#"<office:document-content xmlns:office="{OFFICE}" xmlns:text="{TEXT}"><office:body><office:text><text:p>Alpha<text:s text:c="3"/><text:tab/>Beta</text:p></office:text></office:body></office:document-content>"#
    );
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    zip.start_file("mimetype", SimpleFileOptions::default())
        .unwrap();
    zip.write_all(b"application/vnd.oasis.opendocument.text")
        .unwrap();
    zip.start_file("content.xml", SimpleFileOptions::default())
        .unwrap();
    zip.write_all(xml.as_bytes()).unwrap();
    fs::write(&path, zip.finish().unwrap().into_inner()).unwrap();
    let settings = crate::Settings {
        width: 240,
        height: 360,
        margin: 12,
        font_size: 24.0,
        ..crate::Settings::default()
    };
    let fonts = [crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut probe = crate::Probe::open(&path, crate::Engine::Parley, &fonts, settings, 0).unwrap();
    let block = &probe.source.blocks[0];
    assert_eq!(block.text, "Alpha   \tBeta");
    assert_eq!(
        block
            .styled_text
            .iter()
            .filter(|run| run.preserve_whitespace)
            .count(),
        2
    );
    let (key, end) = (block.key, block.text.len());
    let selected = probe
        .select_range(&probe.anchor(key, 0), &probe.anchor(key, end))
        .unwrap();
    assert_eq!(selected.text, "Alpha   \tBeta");
    let pixels = probe.capture(0).unwrap();
    if let Some(path) = std::env::var_os("READER_ODT_WHITESPACE_CAPTURE") {
        image::save_buffer(
            path,
            &pixels,
            settings.width,
            settings.height,
            image::ColorType::L8,
        )
        .unwrap();
    }
}

#[test]
fn long_odt_space_run_wraps_instead_of_forcing_a_no_break_line() {
    let scratch = Scratch::new();
    let path = scratch.0.join("wrap.odt");
    let xml = format!(
        r#"<office:document-content xmlns:office="{OFFICE}" xmlns:text="{TEXT}"><office:body><office:text><text:p>A<text:s text:c="1024"/>B</text:p></office:text></office:body></office:document-content>"#
    );
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    zip.start_file("mimetype", SimpleFileOptions::default())
        .unwrap();
    zip.write_all(b"application/vnd.oasis.opendocument.text")
        .unwrap();
    zip.start_file("content.xml", SimpleFileOptions::default())
        .unwrap();
    zip.write_all(xml.as_bytes()).unwrap();
    fs::write(&path, zip.finish().unwrap().into_inner()).unwrap();
    let settings = crate::Settings {
        width: 240,
        height: 360,
        margin: 12,
        font_size: 24.0,
        ..crate::Settings::default()
    };
    let fonts = [crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let probe = crate::Probe::open(&path, crate::Engine::Parley, &fonts, settings, 0).unwrap();
    assert_eq!(
        probe.source.blocks[0].text,
        format!("A{}B", " ".repeat(1024))
    );
    assert!(probe.page_count() > 1);
}
