use super::*;
use crate::app::App;
use crate::app::selection_viewport_tests::{adopt, capture, tap};
use crate::app::tests::{Fixture, tap_contents_item};
use crate::session::ReadingViewport;
use std::fs;

fn settings(reader: &Reader) -> Settings {
    reader.active.as_ref().unwrap().reading.config.settings
}

#[test]
fn hyphenation_options_preserve_book_anchor_and_freeze_new_book_defaults() {
    let f = Fixture::new();
    let a = f.root.join("First text.epub");
    let b = f.root.join("Second text.epub");
    let first = crate::test_fixture("english-hyphenation.epub");
    fs::copy(&first, &a).unwrap();
    fs::copy(crate::test_fixture("hyphenation-options.epub"), &b).unwrap();
    let source = fs::read(&a).unwrap();
    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut reader = Reader::open(&f.root, &f.state, fonts.clone()).unwrap();
    reader.open_path(&a).unwrap();
    let reading = &mut reader.active.as_mut().unwrap().reading;
    let focus = reading.probe.matches("hyphenation").remove(0);
    let page = reading.probe.page_for(&focus).unwrap();
    assert!(reading.go_to_page(0, page).is_ok());
    let origin = reader.current_history_location();
    let mut canvas = adopt(&mut reader, ReadingViewport::PORTRAIT, 1);
    reader.show(Screen::TypographyRules);
    tap_contents_item(&mut reader, "Hyphenation");
    assert_eq!(reader.screen, Screen::HyphenationOptions);
    tap_contents_item(&mut reader, "Trust soft hyphens");
    tap_contents_item(&mut reader, "Algorithmic hyphenation");
    assert!(settings(&reader).trust_soft_hyphens);
    assert_eq!(
        settings(&reader).hyphenation_mode(),
        HyphenationMode::Algorithmic
    );
    capture(
        &reader,
        &mut canvas,
        "hyphenation-options-settings.png",
        false,
    );
    assert_eq!(reader.current_history_location(), origin);
    tap_contents_item(&mut reader, "Enable hyphenation");
    assert_eq!(settings(&reader).hyphenation_mode(), HyphenationMode::Off);
    assert!(settings(&reader).algorithmic_hyphenation);
    tap_contents_item(&mut reader, "Enable hyphenation");
    assert_eq!(
        settings(&reader).hyphenation_mode(),
        HyphenationMode::Algorithmic
    );
    tap_contents_item(&mut reader, "Defaults for new books");
    tap_contents_item(&mut reader, "Soft hyphens only");
    assert_eq!(
        settings(&reader).hyphenation_mode(),
        HyphenationMode::Algorithmic
    );

    reader.open_path(&b).unwrap();
    assert_eq!(
        settings(&reader).hyphenation_mode(),
        HyphenationMode::Authored
    );
    assert!(!settings(&reader).trust_soft_hyphens);
    reader.save().unwrap();
    let mut reopened = Reader::open(&f.root, &f.state, fonts).unwrap();
    reopened.open_path(&a).unwrap();
    assert_eq!(
        settings(&reopened).hyphenation_mode(),
        HyphenationMode::Algorithmic
    );
    assert!(settings(&reopened).trust_soft_hyphens);
    assert_eq!(reopened.current_history_location(), origin);
    assert_eq!(fs::read(&a).unwrap(), source);
    reopened.open_path(&b).unwrap();
    assert_eq!(
        settings(&reopened).hyphenation_mode(),
        HyphenationMode::Authored
    );
}

#[test]
fn hyphenation_limits_are_a_two_value_draft_and_fail_atomically() {
    let f = Fixture::new();
    let path = f.root.join("Breaks.epub");
    let source = crate::test_fixture("english-hyphenation.epub");
    fs::copy(&source, &path).unwrap();
    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut reader = Reader::open(&f.root, &f.state, fonts.clone()).unwrap();
    reader.open_path(&path).unwrap();
    let mut canvas = adopt(&mut reader, ReadingViewport::LANDSCAPE, 1);
    let origin = reader.current_history_location();
    reader.show(Screen::HyphenationOptions);
    tap_contents_item(&mut reader, "Hyphenation limits");
    tap_contents_item(&mut reader, "Letters before the break");
    tap_contents_item(&mut reader, "8");
    tap_contents_item(&mut reader, "Letters after the break");
    tap_contents_item(&mut reader, "6");
    assert_eq!(reader.controls.hyphenation_limits, None);
    let before = settings(&reader);
    capture(
        &reader,
        &mut canvas,
        "hyphenation-limits-landscape.png",
        false,
    );

    let saved_path = reader.controls_path.clone();
    let cannot_replace = f.root.join("Controls directory");
    fs::create_dir(&cannot_replace).unwrap();
    reader.controls_path = cannot_replace;
    let expected = HyphenationLimits { left: 8, right: 6 };
    assert!(
        reader
            .activate_hyphenation(Command::Limits(Some(expected)))
            .is_err()
    );
    assert_eq!(settings(&reader), before);
    assert_eq!(reader.controls.hyphenation_limits, None);
    assert_eq!(reader.current_history_location(), origin);
    assert!(matches!(reader.screen, Screen::HyphenationLimits(_)));

    reader.controls_path = saved_path;
    tap_contents_item(&mut reader, "Apply to all books");
    assert_eq!(settings(&reader).hyphenation_limits, Some(expected));
    assert_eq!(reader.current_history_location(), origin);
    reader.save().unwrap();
    let mut reopened = Reader::open(&f.root, &f.state, fonts).unwrap();
    reopened.open_path(&path).unwrap();
    assert_eq!(settings(&reopened).hyphenation_limits, Some(expected));
    reopened.show(Screen::HyphenationOptions);
    tap_contents_item(&mut reopened, "Hyphenation limits");
    tap_contents_item(&mut reopened, "Use language defaults");
    assert_eq!(settings(&reopened).hyphenation_limits, None);

    reopened.show(Screen::HyphenationOptions);
    tap_contents_item(&mut reopened, "Hyphenation limits");
    tap_contents_item(&mut reopened, "Letters before the break");
    tap_contents_item(&mut reopened, "10");
    let back = reopened.navigation_layout().back();
    tap(&mut reopened, back);
    assert_eq!(reopened.screen, Screen::HyphenationOptions);
    assert_eq!(settings(&reopened).hyphenation_limits, None);
}

#[test]
fn multilingual_hyphenation_settings_use_the_book_language_defaults() {
    use crate::app::typography_settings::{CatalogLanguage, LanguageScope};

    let f = Fixture::new();
    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut reader = Reader::open(&f.root, &f.state, fonts.clone()).unwrap();
    reader.open_path(&f.book).unwrap();
    let mut canvas = adopt(&mut reader, ReadingViewport::PORTRAIT, 1);
    let origin = reader.current_history_location();
    let spanish = CatalogLanguage::try_from(String::from("es")).unwrap();
    reader
        .choose_typography_language(LanguageScope::Book, Some(spanish))
        .unwrap();
    reader.show(Screen::TypographyRules);
    tap_contents_item(&mut reader, "Hyphenation");
    let items = reader.hyphenation_items();
    assert!(
        items
            .iter()
            .any(|item| item.title == "Hyphenation dictionary"
                && item.detail.as_deref() == Some("Selected · Spanish"))
    );
    assert!(items.iter().any(|item| item.title == "Hyphenation limits"
        && item.detail.as_deref() == Some("Language defaults · 2 / 2")));
    capture(
        &reader,
        &mut canvas,
        "multilingual-hyphenation-settings.png",
        false,
    );
    assert_eq!(reader.current_history_location(), origin);
    reader.save().unwrap();
    let mut reopened = Reader::open(&f.root, &f.state, fonts).unwrap();
    reopened.open_path(&f.book).unwrap();
    assert_eq!(reopened.current_history_location(), origin);
    assert!(
        reopened
            .hyphenation_items()
            .iter()
            .any(|item| item.title == "Hyphenation dictionary"
                && item.detail.as_deref() == Some("Selected · Spanish"))
    );
}
