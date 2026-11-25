use super::*;
use crate::{Anchor, BaseStylesheet, Engine, Probe, Renderer, user_styles};

const FIXTURE: &str = include_str!("../../tests/fixtures/image-style-tweaks.xhtml");
const FULL: &str = "image_full_width";
const CENTER: &str = "image_valign_middle";

fn snapshot(ids: &[&str]) -> user_styles::Snapshot {
    let mut data = user_styles::Data::default();
    for id in ids {
        data.choose(1, id, Some(true)).unwrap();
    }
    data.snapshot(1).unwrap()
}

fn open(markup: &str, ids: &[&str]) -> Probe {
    let book = Book::open(&crate::test_fixture("field-notes-epub3.epub")).unwrap();
    let fonts = [crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let settings = Settings {
        width: 640,
        height: 880,
        margin: 40,
        font_size: 36.,
        base_stylesheet: BaseStylesheet::None,
        ..Default::default()
    };
    let snapshot = snapshot(ids);
    Probe::from_book_with_rules(
        book,
        Engine::Parley,
        &fonts,
        settings,
        0,
        Some(markup.into()),
        &crate::user_hyphenation::Rules::default(),
        &snapshot,
    )
    .unwrap()
}

fn chapter(probe: &Probe) -> &ParleyChapter {
    let Renderer::Parley(chapter) = &probe.renderer else {
        unreachable!()
    };
    chapter
}

fn image<'a>(probe: &'a Probe, label: &str) -> &'a Illustration {
    chapter(probe)
        .illustrations
        .iter()
        .find(|image| image.label == label)
        .unwrap()
}

fn text_baseline(probe: &Probe, anchor: &Anchor) -> f32 {
    let paragraph = chapter(probe)
        .paragraphs
        .iter()
        .find(|paragraph| paragraph.key == anchor.block)
        .unwrap();
    paragraph.y
        + paragraph
            .layout
            .lines()
            .find(|line| line.text_range().contains(&anchor.offset))
            .unwrap()
            .metrics()
            .baseline
}

#[test]
fn image_style_full_width_has_its_own_line_and_keeps_source_targets() {
    let markup = FIXTURE.replace(
        "Lead text.",
        &"The harbor watch saw the lantern on the lower dock. ".repeat(8),
    );
    let base = open(&markup, &[]);
    let full = open(&markup, &[FULL]);
    let both = open(&markup, &[FULL, CENTER]);
    let tail = base.fragment("trail").unwrap();
    let dest = base.fragment("destination").unwrap();
    let block = chapter(&full)
        .paragraphs
        .iter()
        .find(|paragraph| paragraph.key == tail.block)
        .unwrap();
    let lines: Vec<_> = block.layout.lines().collect();
    let (line_index, position) = lines
        .iter()
        .enumerate()
        .find_map(|(index, line)| {
            line.items().find_map(|item| match item {
                PositionedLayoutItem::InlineBox(position) => Some((index, position)),
                _ => None,
            })
        })
        .unwrap();
    assert!(line_index >= 3 && line_index + 1 < lines.len());
    assert_eq!(position.kind, parley::InlineBoxKind::Block);
    assert!(lines[line_index].items().all(|item| match item {
        PositionedLayoutItem::GlyphRun(run) => run.positioned_glyphs().next().is_none(),
        _ => true,
    }));
    let marker = image(&full, "Shore marker");
    assert_eq!(marker.display, (196, 84));
    assert_eq!(marker.x, Some(0));
    let rtl_markup = markup.replace("class=\"line\"", "class=\"line\" dir=\"rtl\"");
    assert_eq!(
        image(&open(&rtl_markup, &[FULL]), "Shore marker").x,
        Some(560 - 196)
    );
    assert!(text_baseline(&full, &tail) >= marker.y + marker.display.1 as f32);
    assert_eq!(image(&base, "Harbor plate").display, (72, 48));
    assert_eq!(image(&full, "Harbor plate").display, (560, 48));
    assert_eq!(image(&both, "Shore marker").x, marker.x);
    assert_eq!(image(&both, "Shore marker").y, marker.y);
    assert_eq!(full.fragment("trail"), Some(tail.clone()));
    assert_eq!(full.fragment("destination"), Some(dest));
    assert_eq!(full.matches("Tail text."), base.matches("Tail text."));
    assert_eq!(full.coverage(), base.coverage());
    let page = full.pages.page(marker.y);
    let (canvas_x, canvas_y) = marker.origin(
        full.pages.range(page).unwrap().start,
        &full.settings.text_columns(),
    );
    let link = full
        .link_at(
            page,
            canvas_x as f32 + 10.0,
            canvas_y as f32
                + full.settings.margin_top.unwrap_or(full.settings.margin) as f32
                + 10.0,
        )
        .unwrap();
    assert_eq!(link.fragment.as_deref(), Some("destination"));
    assert!(full.page_for(&tail).is_some());
}

#[test]
fn image_style_vertical_middle_is_independent_and_keeps_author_cascade() {
    let base = open(FIXTURE, &[]);
    let middle = open(FIXTURE, &[CENTER]);
    let fit = open(FIXTURE, &["image-text-width", CENTER]);
    let align_from_baseline = |probe: &Probe, anchor: &str, label: &str| {
        let anchor = probe.fragment(anchor).unwrap();
        text_baseline(probe, &anchor) - image(probe, label).y
    };
    let marker = "Shore marker";
    let baseline = align_from_baseline(&base, "lead", marker);
    let centered = align_from_baseline(&middle, "lead", marker);
    assert!((baseline - 84.0).abs() <= 1.0);
    assert!(centered > 42.0 && centered < 72.0, "{centered}");
    assert_eq!(image(&base, marker).display, image(&middle, marker).display);
    assert!(
        (align_from_baseline(&base, "author-lead", "Authored alignment")
            - align_from_baseline(&middle, "author-lead", "Authored alignment"))
        .abs()
            <= 1.0
    );
    assert_eq!(image(&fit, marker).display, (196, 131));
    assert_eq!(middle.matches("Tail text"), base.matches("Tail text"));
}
