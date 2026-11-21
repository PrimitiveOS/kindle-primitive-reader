//! Exercise the reading picker, layout and illustration inspector; optional external data stays private.
use super::tests::Fixture;
use super::*;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Deserialize)]
struct Asset {
    width: u32,
    height: u32,
    md5: String,
}
#[derive(Deserialize)]
struct Reference {
    assets: BTreeMap<String, Asset>,
    chapters: BTreeMap<usize, Vec<String>>,
}

fn verify_image(page: &crate::comic::ImagePage, asset: &Asset) {
    assert_eq!(page.page_size(), (asset.width as f32, asset.height as f32));
    let pixels = page.fit(asset.width, asset.height, 0).unwrap();
    assert_eq!(format!("{:x}", md5::compute(pixels)), asset.md5);
}

#[test]
fn epub_gif_pixels_reach_the_page_and_full_size_inspector_and_return_to_the_source() {
    let fixture = Fixture::new();
    let external = std::env::var_os("READER_TEST_GIF_EPUB").map(PathBuf::from);
    let bookpath = external.clone().unwrap_or_else(|| {
        let path = fixture.root.join("Harbor signals.epub");
        fs::write(
            &path,
            include_bytes!("../../tests/fixtures/gif-signals.epub"),
        )
        .unwrap();
        path
    });
    let reference: Reference = if external.is_some() {
        serde_json::from_slice(
            &fs::read(std::env::var_os("READER_TEST_GIF_REFERENCE").unwrap()).unwrap(),
        )
        .unwrap()
    } else {
        serde_json::from_slice(include_bytes!(
            "../../tests/fixtures/gif-signals-reference.json"
        ))
        .unwrap()
    };
    let font = crate::test_fixture("fonts/NotoSans-Regular.ttf");
    let mut app = Reader::open(
        bookpath.parent().unwrap(),
        &fixture.state,
        vec![font.clone()],
    )
    .unwrap();
    let index=app.library_items().position(|item| matches!(item.action,Action::Open(entry) if entry.path.file_name()==bookpath.file_name())).expect("The EPUB should appear in the normal picker");
    for _ in 0..index / 10 {
        app.input(Input::PageForward);
    }
    app.input(Input::Tap(
        160.,
        (library_view::LIBRARY_LIST_Y + (index % 10) as i32 * ROW_HEIGHT + 45) as f32,
    ));
    assert_eq!(app.screen, Screen::Book);
    let mut package = Book::open(&bookpath).unwrap();
    for (path, asset) in &reference.assets {
        let data = package.resource(path).unwrap();
        assert_eq!(
            crate::epub_raster_dimensions(&data).unwrap(),
            (asset.width, asset.height)
        );
        verify_image(
            &crate::parley_layout::original_illustration(&data).unwrap(),
            asset,
        );
    }
    drop(package);
    let mut selected = None;
    let mut visible = BTreeSet::new();
    for (&chapter, expected) in &reference.chapters {
        let probe = app
            .active
            .as_ref()
            .unwrap()
            .reading
            .config
            .chapter(chapter)
            .unwrap();
        let crate::Renderer::Parley(layout) = &probe.renderer else {
            panic!("Expected a horizontal book")
        };
        let mut chapter_art = BTreeSet::new();
        for page in 0..probe.page_count() {
            for (path, x, y) in layout.visible_illustrations(probe.page_range(page), probe.settings)
            {
                if !reference.assets.contains_key(path) {
                    continue;
                }
                chapter_art.insert(path.to_owned());
                visible.insert(path.to_owned());
                if selected.is_none() {
                    selected = Some((
                        chapter,
                        page,
                        ((x.start + x.end) / 2) as f32,
                        ((y.start + y.end) / 2) as f32,
                        path.to_owned(),
                    ));
                }
            }
        }
        if external.is_some() || chapter != 1 {
            assert!(
                expected.iter().all(|name| chapter_art.contains(name)),
                "A referenced GIF did not occupy its reading page"
            );
        }
    }
    let (chapter, page, x, y, path) = selected.expect("The book should have an illustrated page");
    app.follow_reflow_page(chapter, page).unwrap();
    let origin = app.current_history_location();
    let history = app.back.clone();
    let active = &app.active.as_ref().unwrap().reading;
    assert!(active.illustration_at(x, y) == Some(path.as_str()));
    verify_image(
        &active.open_illustration(&path).unwrap(),
        &reference.assets[&path],
    );
    let mut canvas = Canvas::new(WIDTH, HEIGHT, &[font.display().to_string()]).unwrap();
    let before = primitive_app::frame(&app, &mut canvas, false);
    if external.is_none()
        && let Some(out) = std::env::var_os("READER_GIF_PAGE_SCREENSHOT")
    {
        primitive_app::write_png(Path::new(&out), WIDTH, HEIGHT, &before).unwrap();
    }
    app.input(Input::LongTap(x, y));
    assert_eq!(app.screen, Screen::Illustration);
    assert!(app.illustration.is_some());
    app.input(Input::Tap((DONE.x + 15) as f32, (DONE.y + 15) as f32));
    assert_eq!(app.screen, Screen::Book);
    assert!(app.current_history_location() == origin && app.back == history);
    assert!(primitive_app::frame(&app, &mut canvas, false) == before);
    if external.is_none() {
        let reading = &app.active.as_ref().unwrap().reading;
        let dest = reading
            .probe
            .links_on_page(0)
            .into_iter()
            .find(|(_, d)| d.path.ends_with(".gif"))
            .unwrap()
            .1;
        app.activate(Action::Jump(dest)).unwrap();
        assert_eq!(app.screen, Screen::Illustration);
        app.input(Input::Tap((DONE.x + 15) as f32, (DONE.y + 15) as f32));
        assert!(app.current_history_location() == origin);
        let mut book = Book::open(&bookpath).unwrap();
        let (kind, cover) = book.read_cover().unwrap().unwrap();
        assert_eq!(kind, crate::epub::CoverKind::Gif);
        verify_image(
            &crate::parley_layout::original_illustration(&cover).unwrap(),
            &reference.assets["EPUB/art/flags.gif"],
        );
        let nested = app
            .active
            .as_ref()
            .unwrap()
            .reading
            .config
            .chapter(1)
            .unwrap();
        let crate::Renderer::Parley(layout) = &nested.renderer else {
            unreachable!()
        };
        let mut opened = BTreeSet::new();
        for pg in 0..nested.page_count() {
            for (path, _, _) in layout.visible_illustrations(nested.page_range(pg), nested.settings)
            {
                let art = nested.open_illustration(&bookpath, path).unwrap();
                let pixels = art.fit(384, 256, 0).unwrap();
                // Both SVG containers show their own gray background through GIF index zero.
                assert_eq!(pixels[0], 136);
                assert!(pixels.contains(&29));
                opened.insert(path.to_owned());
            }
        }
        assert_eq!(opened.len(), 2);
    }
    eprintln!(
        "gif_case assets={} directly_displayed={} illustrated_chapters={} first_frame_pixels_match=true inspector_returns=true",
        reference.assets.len(),
        visible.len(),
        reference.chapters.len()
    );
}
