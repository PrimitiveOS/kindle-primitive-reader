use super::*;
use crate::{Engine, Probe, Renderer};
use std::path::PathBuf;

#[test]
fn tabs_preserve_text_positions_without_missing_glyph_boxes() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path =
        std::env::temp_dir().join(format!("reader-tab-{}-{unique}.xhtml", std::process::id()));
    std::fs::write(&path, "<html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>Survey</title></head><body><p id=\"normal\">Alpha\tBeta</p><pre id=\"preserved\">Gamma\tDelta</pre></body></html>").unwrap();
    let result = Probe::open(
        &path,
        Engine::Parley,
        &[crate::test_fixture("fonts/NotoSans-Regular.ttf")],
        Settings::default(),
        0,
    );
    std::fs::remove_file(path).unwrap();
    let mut probe = result.unwrap();
    assert_eq!(probe.missing_glyphs(), 0);
    assert!(probe.capture(0).unwrap().iter().any(|&pixel| pixel < 200));
    let start = probe.fragment("preserved").unwrap();
    let block = probe
        .source
        .blocks
        .iter()
        .find(|block| block.key == start.block)
        .unwrap();
    let mut end = start.clone();
    end.offset = block.text.len();
    let selection = probe.select_range(&start, &end).unwrap();
    assert_eq!(selection.text, block.text);
}

#[test]
#[ignore = "Requires an authorized original document and a selected local font stack"]
fn diagnose_original_missing_clusters() {
    let file = PathBuf::from(std::env::var_os("READER_GLYPH_BOOK").unwrap());
    let fonts: Vec<_> =
        std::env::split_paths(&std::env::var_os("READER_GLYPH_FONTS").unwrap()).collect();
    let chapter = std::env::var("READER_GLYPH_CHAPTER")
        .unwrap()
        .parse()
        .unwrap();
    let settings = Settings {
        width: 1264,
        height: 1606,
        ..Settings::default()
    };
    let probe = Probe::open(&file, Engine::Parley, &fonts, settings, chapter).unwrap();
    let Renderer::Parley(layout) = &probe.renderer else {
        panic!("Parley required")
    };
    let mut missing = Vec::new();
    for paragraph in &layout.paragraphs {
        let block = probe
            .source
            .blocks
            .iter()
            .find(|block| block.key == paragraph.key)
            .unwrap();
        for line in paragraph.layout.lines() {
            for run in line.runs() {
                for cluster in run.clusters() {
                    if cluster.glyphs().any(|glyph| glyph.id == 0) {
                        let span = cluster.text_range();
                        let characters: Vec<_> = block
                            .text
                            .get(span.clone())
                            .unwrap_or("")
                            .chars()
                            .map(|character| format!("U+{:04X}", u32::from(character)))
                            .collect();
                        missing.push(serde_json::json!({"characters":characters,"bytes":span,
                            "advance":cluster.glyphs().map(|glyph| glyph.advance).sum::<f32>()}));
                    }
                }
            }
        }
    }
    std::fs::write(
        PathBuf::from(std::env::var_os("READER_GLYPH_OUTPUT").unwrap()),
        serde_json::to_vec_pretty(&missing).unwrap(),
    )
    .unwrap();
}
