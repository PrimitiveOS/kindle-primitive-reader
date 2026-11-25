use super::*;
use crate::{Engine, Probe, PublisherRenderMode, Renderer, user_styles};
use std::path::Path;

#[test]
fn table_presentational_hints_change_rendered_positions_and_retain_the_link() {
    let path = crate::test_fixture("field-notes-epub3.epub");
    let specimen = include_str!("../../tests/fixtures/table-presentational-hints.xhtml");
    let settings = PublisherRenderMode::Book.apply(Settings {
        width: 780,
        height: 630,
        margin: 26,
        font_size: 32.,
        ..Settings::default()
    });
    let open = |styles: &user_styles::Snapshot| {
        Probe::from_book_with_rules(
            Book::open(&path).unwrap(),
            Engine::Parley,
            &[crate::test_fixture("fonts/NotoSans-Regular.ttf")],
            settings,
            0,
            Some(specimen.into()),
            &crate::user_hyphenation::Rules::default(),
            styles,
        )
        .unwrap()
    };
    let mut choices = user_styles::Data::default();
    choices
        .choose(1, "table_no_presentational_hints", Some(true))
        .unwrap();
    let mut original = open(&user_styles::Snapshot::default());
    let mut filtered = open(&choices.snapshot(1).unwrap());
    let measure = |probe: &Probe, id: &str, text: &str| {
        let first = probe.fragment(id).unwrap();
        let last = crate::Anchor {
            offset: first.offset + text.len(),
            ..first.clone()
        };
        let selected = probe.select_range(&first, &last).unwrap();
        assert_eq!(selected.text, text);
        assert_eq!(probe.page_for(&first), Some(0));
        let rect = probe.selection_rects(0, &selected)[0];
        let glyph_x = match &probe.renderer {
            Renderer::Parley(chapter) => chapter
                .paragraphs
                .iter()
                .find(|p| p.key == first.block)
                .unwrap()
                .layout
                .lines()
                .flat_map(|line| line.items())
                .find_map(|item| match item {
                    PositionedLayoutItem::GlyphRun(run) => {
                        run.positioned_glyphs().next().map(|g| g.x)
                    }
                    _ => None,
                })
                .unwrap(),
            _ => unreachable!(),
        };
        (first, rect, glyph_x)
    };
    let (dock, _, x) = measure(&original, "dock", "North dock");
    let (new_dock, _, new_x) = measure(&filtered, "dock", "North dock");
    assert_eq!(dock, new_dock);
    assert!(x > new_x + 25., "{x} and {new_x}");
    let mut destinations = Vec::new();
    for (probe, name) in [(&mut original, "attributes"), (&mut filtered, "stylesheet")] {
        let (_, links, _) = measure(probe, "rings", "Mooring rings");
        destinations.push(probe.link_at(0, links.x as f32 + 5., links.y as f32 + 6.));
        measure(probe, "merged", "Morning watch");
        let (_, _, control_x) = measure(probe, "css-cell", "CSS table");
        assert!(control_x.abs() < 2.0);
        #[cfg(feature = "kindle")]
        if let Some(output) = std::env::var_os("READER_STYLE_GRID_CAPTURES") {
            primitive_app::write_png(
                &Path::new(&output).join(format!("reader-table-presentation-{name}.png")),
                settings.width,
                settings.height,
                &probe.capture(0).unwrap(),
            )
            .unwrap();
        }
    }
    assert!(destinations[0].is_some() && destinations[0] == destinations[1]);
    assert_eq!(original.coverage(), filtered.coverage());
}
