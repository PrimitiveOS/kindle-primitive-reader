use super::*;
use crate::{Probe, Renderer};
use std::path::PathBuf;

#[test]
#[ignore = "Supply an original EPUB and its chapter indices"]
fn original_justified_words_can_expand_while_selection_stays_in_place() {
    let path = PathBuf::from(std::env::var_os("READER_EXPANSION_EPUB").unwrap());
    let chapters = std::env::var("READER_EXPANSION_CHAPTERS").unwrap();
    let fonts = [crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut checked = 0;
    for chapter in chapters.split(',').map(|n| n.parse::<usize>().unwrap()) {
        let load = |word_expansion| {
            Probe::from_book(
                Book::open(&path).unwrap(),
                crate::Engine::Parley,
                &fonts,
                Settings {
                    width: 1264,
                    height: 1606,
                    alignment: crate::TextAlignment::Justify,
                    automatic_hyphenation: false,
                    disable_soft_hyphens: true,
                    word_expansion,
                    ..Settings::default()
                },
                chapter,
            )
            .unwrap()
        };
        let before = load(0);
        let after = load(15);
        assert_eq!(before.page_count(), after.page_count());
        let (Renderer::Parley(first), Renderer::Parley(second)) =
            (&before.renderer, &after.renderer)
        else {
            unreachable!()
        };
        assert_eq!(first.paragraphs.len(), second.paragraphs.len());
        let mut chapter_cases = 0;
        for (old, new) in first.paragraphs.iter().zip(&second.paragraphs) {
            assert_eq!(old.key, new.key);
            assert!(
                old.layout
                    .lines()
                    .map(|l| l.text_range())
                    .eq(new.layout.lines().map(|l| l.text_range()))
            );
            let Some(source) = before
                .source
                .blocks
                .iter()
                .find(|block| block.key == old.key)
            else {
                continue;
            };
            // Anchor a word using two following words in the same passage.
            // Find all in the independent reader can locate this phrase without
            // using our line count, x coordinate or source block identifier.
            let words: Vec<_> = crate::selection::word_ranges(&source.text).collect();
            for phrase in words.windows(3) {
                if chapter_cases >= 8 {
                    break;
                }
                let word = &phrase[0];
                if word.len() < 6 || !source.text[word.clone()].chars().all(char::is_alphabetic) {
                    continue;
                }
                let selection = before
                    .select_range(
                        &before.anchor(old.key, word.start),
                        &before.anchor(old.key, word.end),
                    )
                    .unwrap();
                let page = before.page_for(&selection.start).unwrap();
                let original = before.selection_rects(page, &selection);
                let changed = after.selection_rects(page, &selection);
                let ([old_box], [new_box]) = (&original[..], &changed[..]) else {
                    continue;
                };
                if new_box.width - old_box.width < 4 {
                    continue;
                }
                assert_eq!(old_box.y, new_box.y);
                let word_at = after
                    .word_at(
                        page,
                        (new_box.x + new_box.width / 2) as f32,
                        (new_box.y + new_box.height / 2) as f32,
                    )
                    .unwrap();
                assert_eq!(word_at, selection);
                let context = &source.text[word.start..phrase[2].end];
                if context.contains(['\t', '\n']) {
                    continue;
                }
                println!(
                    "expansion-case\t{chapter}\t{}\t{}\t{}\t{}",
                    word_at.text, context, old_box.width, new_box.width
                );
                chapter_cases += 1;
            }
            if chapter_cases >= 8 {
                break;
            }
        }
        checked += chapter_cases;
    }
    assert!(checked >= 8);
    println!("Verified original expansion words: {checked}");
}
