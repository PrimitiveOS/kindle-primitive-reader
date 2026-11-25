use super::*;
use std::{
    io::{Read, Write},
    sync::atomic::{AtomicU64, Ordering},
};

struct BookCopy(PathBuf);
impl Drop for BookCopy {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
fn book() -> BookCopy {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let original = crate::test_fixture("field-notes-epub3.epub");
    let before = std::fs::read(&original).unwrap();
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(&before)).unwrap();
    let path = std::env::temp_dir().join(format!(
        "reader-css-restore-{}-{}.epub",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let mut writer = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    let opts =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let markup = format!(
        "<html><body><p>Before.</p><p id='hidden' style='display:none'>HIDDEN SAVED PASSAGE</p><p id='after'>AFTER restored target.</p>{}</body></html>",
        "<p>Long retained reading passage to permit page movement.</p>".repeat(100)
    );
    for index in 0..archive.len() {
        let mut file = archive.by_index(index).unwrap();
        writer.start_file(file.name(), opts).unwrap();
        if file.name() == "EPUB/chapters/shore.xhtml" {
            writer.write_all(markup.as_bytes()).unwrap();
        } else {
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes).unwrap();
            writer.write_all(&bytes).unwrap();
        }
    }
    writer.finish().unwrap();
    assert_eq!(std::fs::read(&original).unwrap(), before);
    BookCopy(path)
}

#[test]
fn css_restore_retains_canonical_focus_across_scroll_spread_reflow_and_reopen_until_real_motion() {
    let book = book();
    for (scroll, facing_pages) in [(false, false), (true, false), (false, true)] {
        let config = BookConfig {
            viewport: crate::session::ReadingViewport::PORTRAIT,
            user_styles: Default::default(),
            user_hyphenation: Default::default(),
            engine: Engine::Parley,
            book: book.0.clone(),
            fonts: vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")],
            settings: Settings {
                width: WIDTH,
                height: PAGE_HEIGHT,
                font_size: 30.,
                publisher_styles: false,
                ..Default::default()
            },
            scroll,
            facing_pages,
            hide_nonlinear: false,
            hidden_flows: Default::default(),
            chapter_offsets: Default::default(),
            quote_prefixes: Default::default(),
            screen_pages: Default::default(),
            eager_layouts: Default::default(),
        };
        let mut reading = Reading::open(config).unwrap();
        let saved = reading.probe.matches("HIDDEN SAVED PASSAGE")[0].clone();
        let end = Anchor {
            offset: saved.offset + 20,
            ..saved.clone()
        };
        let quote = reading.anchored_quote(&saved, &end).unwrap();
        reading.jump_to_anchor(0, &saved).unwrap();
        reading
            .reflow(Settings {
                publisher_styles: true,
                ..reading.config.settings
            })
            .unwrap();
        assert_eq!(reading.anchor(), Some(saved.clone()));
        assert!(reading.probe.page_for(&saved).is_none());
        assert!(reading.jump_to_anchor(0, &saved).is_err());
        assert_eq!(reading.anchor(), Some(saved.clone()));
        assert_eq!(
            reading.anchored_quote(&saved, &end).unwrap().text,
            quote.text
        );
        assert!(
            reading
                .probe
                .restore_anchor(&saved)
                .and_then(|a| reading.probe.page_for(&a))
                .is_some()
        );
        let mut reopened = Reading::restore(
            reading.config.clone(),
            0,
            reading.page,
            reading.anchor().as_ref(),
        )
        .unwrap();
        assert_eq!(reopened.anchor(), Some(saved.clone()));
        reopened
            .reflow(Settings {
                publisher_styles: false,
                ..reopened.config.settings
            })
            .unwrap();
        assert_eq!(reopened.anchor(), Some(saved.clone()));
        assert!(reopened.probe.page_for(&saved).is_some());
        reopened
            .reflow(Settings {
                publisher_styles: true,
                ..reopened.config.settings
            })
            .unwrap();
        reopened.input(Input::PageForward);
        reopened.tick(reopened.deadline().unwrap());
        assert_ne!(reopened.anchor(), Some(saved));
    }
}

#[test]
fn saved_custom_flow_markers_remain_resolved_when_publisher_css_omits_their_text() {
    use crate::library::{
        ReadingPosition,
        hidden_flows::{FlowPoint, HiddenFlows},
    };
    let book = book();
    let config = BookConfig {
        viewport: crate::session::ReadingViewport::PORTRAIT,
        user_styles: Default::default(),
        user_hyphenation: Default::default(),
        engine: Engine::Parley,
        book: book.0.clone(),
        fonts: vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")],
        settings: Settings {
            width: WIDTH,
            height: PAGE_HEIGHT,
            font_size: 30.,
            publisher_styles: false,
            ..Default::default()
        },
        scroll: false,
        facing_pages: false,
        hide_nonlinear: false,
        hidden_flows: Default::default(),
        chapter_offsets: Default::default(),
        quote_prefixes: Default::default(),
        screen_pages: Default::default(),
        eager_layouts: Default::default(),
    };
    let reading = Reading::open(config.clone()).unwrap();
    let anchor = reading.probe.matches("HIDDEN SAVED PASSAGE")[0].clone();
    let mut ranges = HiddenFlows::default();
    ranges.enabled = true;
    ranges
        .insert(FlowPoint {
            position: ReadingPosition {
                chapter: 0,
                page: 0,
                anchor: Some(anchor.clone()),
            },
            hidden: true,
        })
        .unwrap();
    let raw = serde_json::to_vec(&ranges).unwrap();
    let mut config = BookConfig {
        hidden_flows: std::rc::Rc::new(ranges),
        ..config
    };
    config.settings.publisher_styles = true;
    let reading = Reading::restore(config, 0, 0, Some(&anchor)).unwrap();
    assert!(reading.custom_flows.unresolved.is_empty());
    assert!(reading.probe.page_for(&anchor).is_none());
    assert_eq!(reading.probe.flow_boundary_page(&anchor), Some(0));
    assert!(reading.custom_flow_map().unwrap().flow(0) > 0);
    assert_eq!(
        serde_json::to_vec(&*reading.config.hidden_flows).unwrap(),
        raw
    );
    assert_eq!(reading.anchor(), Some(anchor));
}
