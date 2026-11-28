use super::*;

fn sheet(markup: &str) -> (Source, BookStyle) {
    let source = Source::parse(markup).unwrap();
    let mut book = Book::open(&crate::test_fixture("field-notes-epub3.epub")).unwrap();
    let chapter = book.chapters[0].href.clone();
    let mut fonts =
        crate::font_context(&[crate::test_fixture("fonts/NotoSans-Regular.ttf")]).unwrap();
    let sheet = BookStyle::load(&mut book, &chapter, &source, &mut fonts);
    (source, sheet)
}

fn at<'a>(source: &Source, styles: &'a [Style], id: &str) -> &'a Style {
    &styles[source
        .elements
        .iter()
        .position(|e| e.id.as_deref() == Some(id))
        .unwrap()]
}

#[test]
fn image_maxima_follow_author_cascade_and_dpi_without_inheritance() {
    let (source, rules) = sheet(
        r#"<html><head><style>
      img { max-width: 50%; max-height: 1in }
      #clear { max-width: none }
      #reset { max-height: initial }
      </style></head><body style="max-width:3em">
      <img id="limited" src="a.png"/><img id="clear" src="a.png"/>
      <img id="reset" src="a.png"/><p id="paragraph">Caption</p>
      </body></html>"#,
    );
    for (dpi, expected_height) in [(0, None), (96, Some(96.0)), (212, Some(212.0))] {
        let styles = rules.compute_for_dpi(&source.elements, dpi);
        let width = |id| {
            at(&source, &styles, id)
                .image_max_width
                .and_then(|value| value.pixels(600, 40.0))
        };
        let height = |id| {
            at(&source, &styles, id)
                .image_max_height
                .and_then(|value| value.pixels(800, 40.0))
        };
        assert_eq!(width("limited"), Some(300.0));
        assert_eq!(height("limited"), expected_height);
        assert_eq!(width("clear"), None);
        assert_eq!(height("reset"), None);
        assert_eq!(width("paragraph"), None);
    }
    assert!(user_css::parse("img { max-width: 50%; max-height: 8em }").is_ok());
    assert!(user_css::parse("img { max-height: 40% }").is_err());
}

#[test]
fn dpi_scales_absolute_boxes_once_after_inheritance_and_keeps_relative_lengths_and_font() {
    let (source, sheet) = sheet(
        r#"<html style="font-size:125%"><body>
      <section style="font-size:150%;text-indent:12pt"><p id="physical" style="padding:1pc 1in">One</p>
      <p id="relative" style="font-size:75%;padding:1em 2rem;text-indent:10%">Two</p></section>
      <table id="table" style="border:3px solid black;border-spacing:2mm 4px"><tr><td>Cell</td></tr></table>
      </body></html>"#,
    );
    let old = sheet.compute(&source.elements);
    let identity = sheet.compute_for_dpi(&source.elements, 96);
    for (a, b) in old.iter().zip(&identity) {
        assert_eq!(a.padding, b.padding);
        assert_eq!(a.vertical_padding, b.vertical_padding);
        assert_eq!(
            a.text_indent.pixels(900, 42.),
            b.text_indent.pixels(900, 42.)
        );
        assert_eq!(a.size_scale, b.size_scale);
    }
    for dpi in crate::RENDER_DPI_PRESETS {
        let styles = sheet.compute_for_dpi(&source.elements, dpi);
        let physical = at(&source, &styles, "physical");
        let relative = at(&source, &styles, "relative");
        let table = at(&source, &styles, "table");
        let scale = f32::from(dpi) / 96.;
        assert_eq!(physical.padding[0].pixels(900, 42.), 96. * scale);
        assert_eq!(physical.vertical_padding[0].pixels(900, 42.), 16. * scale);
        assert_eq!(physical.text_indent.pixels(900, 42.), 16. * scale);
        assert_eq!(relative.padding, at(&source, &old, "relative").padding);
        assert_eq!(relative.text_indent.pixels(900, 42.), 90.);
        assert_eq!(
            relative.size_scale,
            at(&source, &old, "relative").size_scale
        );
        assert_eq!(
            table.borders.as_ref().unwrap()[0].width.pixels(900, 42.),
            3. * if dpi == 0 { 1. } else { scale }
        );
        assert!(
            (table.border_spacing[0].pixels(900, 42.) - 2. * (96. / 25.4) * scale).abs() < 0.001
        );
        assert_eq!(
            table.border_spacing[1].pixels(900, 42.),
            4. * if dpi == 0 { 1. } else { scale }
        );
    }
}

#[test]
fn dpi_preserves_96_physical_quantization_and_off_is_distinct_from_pixel_identity() {
    for unit in ["pt", "pc", "in", "cm", "mm", "q"] {
        let text = format!("0.237{unit}");
        let mut parser = Parser::new(&text);
        let value = parse_css_length(&mut parser).unwrap();
        assert!(matches!(value, CssLength::Physical(_)));
        let legacy = CssLength::Pixels(value.pixels(900, 42.));
        assert_eq!(
            value.for_dpi(96).pixels(900, 42.).to_bits(),
            legacy.pixels(900, 42.).to_bits()
        );
        assert_eq!(
            value.for_dpi(96).box_pixels(900, 42.),
            legacy.box_pixels(900, 42.)
        );
        assert_eq!(value.for_dpi(0).pixels(900, 42.), 0.);
    }
    assert_eq!(CssLength::Pixels(17.).for_dpi(0).pixels(900, 42.), 17.);
}

#[test]
fn dpi_image_physical_constraints_are_ignored_when_off_and_rem_uses_root_not_local_font() {
    let (source, sheet) = sheet(
        r#"<html style="font-size:125%"><body style="font-size:200%">
      <img id="physical" src="x" style="width:1in;height:12pt"/>
      <img id="root" src="x" style="width:2rem;height:2em"/>
      <img id="percent" src="x" style="width:25%"/>
      </body></html>"#,
    );
    for dpi in crate::RENDER_DPI_PRESETS {
        let styles = sheet.compute_for_dpi(&source.elements, dpi);
        let p = at(&source, &styles, "physical");
        let scale = f32::from(dpi) / 96.;
        assert_eq!(
            p.image_width.unwrap().pixels(900, p.size_scale * 40.),
            (dpi != 0).then_some(96. * scale)
        );
        assert_eq!(
            p.image_height.unwrap().pixels(900, p.size_scale * 40.),
            (dpi != 0).then_some(16. * scale)
        );
        let r = at(&source, &styles, "root");
        assert_eq!(
            r.image_width.unwrap().pixels(900, r.size_scale * 40.),
            Some(100.)
        );
        assert_eq!(
            r.image_height.unwrap().pixels(900, r.size_scale * 40.),
            Some(200.)
        );
        let percent = at(&source, &styles, "percent");
        assert_eq!(percent.image_width.unwrap().pixels(900, 40.), Some(225.));
    }
}

#[test]
#[ignore = "requires private unchanged READER_DPI_ORIGINALS editions and READER_DPI_INVENTORY_JSON output"]
fn dpi_original_inventory_measures_newly_supported_image_constraints_and_96_box_identity() {
    let root = std::path::PathBuf::from(std::env::var_os("READER_DPI_ORIGINALS").unwrap());
    let mut files: Vec<_> = std::fs::read_dir(&root)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| matches!(e, "epub" | "azw3" | "mobi"))
        })
        .collect();
    files.sort();
    assert_eq!(
        files.len(),
        48,
        "inventory uses the48 distinct authorized editions"
    );
    let mut rows = Vec::new();
    for path in files {
        let mut book = Book::open(&path).unwrap();
        let mut fonts =
            crate::font_context(&[crate::test_fixture("fonts/NotoSans-Regular.ttf")]).unwrap();
        let (mut physical, mut rem, mut images, mut boxes) = (0, 0, 0, 0);
        for chapter in 0..book.chapters.len() {
            let href = book.chapters[chapter].href.clone();
            let source = Source::parse(&book.chapter(chapter).unwrap()).unwrap();
            let sheet = BookStyle::load(&mut book, &href, &source, &mut fonts);
            let previous = sheet.compute(&source.elements);
            let identity = sheet.compute_for_dpi(&source.elements, 96);
            for (a, b) in previous.iter().zip(&identity) {
                for (a, b) in a
                    .padding
                    .iter()
                    .chain(&a.vertical_padding)
                    .chain(&a.border_spacing)
                    .zip(
                        b.padding
                            .iter()
                            .chain(&b.vertical_padding)
                            .chain(&b.border_spacing),
                    )
                {
                    assert_eq!(a.pixels(1156, 42.).to_bits(), b.pixels(1156, 42.).to_bits());
                    assert_eq!(a.box_pixels(1156, 42.), b.box_pixels(1156, 42.));
                    boxes += 1;
                }
                assert_eq!(
                    a.text_indent.pixels(1156, 42.),
                    b.text_indent.pixels(1156, 42.)
                );
                assert_eq!(a.size_scale, b.size_scale);
            }
            for image in source.blocks.iter().flat_map(|b| &b.images) {
                images += 1;
                let style = &identity[image.element];
                for dimension in [style.image_width, style.image_height]
                    .into_iter()
                    .flatten()
                {
                    match dimension {
                        ImageLength::Physical(_) => physical += 1,
                        ImageLength::RootEm(_) => rem += 1,
                        _ => (),
                    }
                }
            }
        }
        rows.push(
            serde_json::json!({"edition":path.file_name().unwrap().to_str().unwrap(),
            "sections":book.chapters.len(),"images":images,"physical_image_constraints":physical,
            "rem_image_constraints":rem,"identity_box_resolutions":boxes}),
        );
    }
    std::fs::write(
        std::env::var_os("READER_DPI_INVENTORY_JSON").unwrap(),
        serde_json::to_vec_pretty(&rows).unwrap(),
    )
    .unwrap();
}

#[test]
fn a_user_sheet_keeps_its_usable_rules_when_one_rule_is_unsupported() {
    let css = "p { text-indent: 2em } img { max-height: 40% } h1 { font-style: italic }";
    // The strict form still fails the whole sheet, so the editor shows the problem.
    assert!(user_css::parse(css).is_err());
    // The layout form applies the two usable rules and skips the unsupported one.
    assert_eq!(user_css::parse_lenient(css).unwrap().len(), 2);
    let (rules, rejected) = user_css::parse_with_diagnostics(css).unwrap();
    assert_eq!(rules.len(), 2);
    assert_eq!(rejected.len(), 1, "{rejected:?}");
}
