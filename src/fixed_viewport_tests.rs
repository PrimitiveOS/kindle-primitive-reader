use super::*;
use std::{
    io::Write,
    sync::atomic::{AtomicUsize, Ordering},
};

const CROP: PdfTextRect = PdfTextRect {
    x0: 0.14,
    y0: 0.12,
    x1: 0.86,
    y1: 0.88,
};

fn harbour_pdf() -> Vec<u8> {
    use lopdf::{Document, Object, Stream, dictionary};

    let mut pdf = Document::with_version("1.4");
    let pages = pdf.new_object_id();
    let index = pdf.new_object_id();
    let chart = pdf.new_object_id();
    let font = pdf.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
    });
    let bold = pdf.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica-Bold",
    });
    let chart_stream = pdf.add_object(Stream::new(
        dictionary!(),
        br"
0.88 g 82 224 316 201 re f
1 g 87 229 306 191 re f
0 G 0.7 w 100 376 m 382 376 l S
0 g BT /F2 24 Tf 100 393 Td (Harbour chart) Tj ET
0 g BT /F1 11 Tf 100 356 Td (NORTH CHANNEL) Tj ET
0.55 G 0.8 w 105 280 m 153 325 l 191 311 l 219 335 l 254 307 l 276 318 l S
0.25 G 0.6 w [3 3] 0 d 112 267 m 155 279 l 183 264 l 236 289 l 275 280 l S [] 0 d
0.15 g 181 271 8 8 re f 243 315 9 9 re f
0 g BT /F1 11 Tf 100 241 Td (West pier) Tj ET
0 g BT /F1 10 Tf 208 241 Td (East reach) Tj ET
0.92 g 323 276 70 28 re f
0 G 0.5 w 323 276 70 28 re S
0 g BT /F2 9 Tf 330 286 Td (VIEW NOTES) Tj ET
"
        .to_vec(),
    ));
    let index_stream = pdf.add_object(Stream::new(
        dictionary!(),
        b"0 g BT /F2 20 Tf 100 380 Td (Harbour notes) Tj ET".to_vec(),
    ));
    let link = pdf.add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "Link", "Rect" => vec![323.into(), 276.into(), 393.into(), 304.into()],
        "Dest" => vec![index.into(), Object::Name(b"Fit".to_vec())],
    });
    for (page, content, annots) in [
        (index, index_stream, vec![]),
        (chart, chart_stream, vec![link.into()]),
    ] {
        pdf.objects.insert(page, dictionary! {
            "Type" => "Page", "Parent" => pages, "MediaBox" => vec![0.into(), 0.into(), 480.into(), 640.into()],
            "Contents" => content, "Annots" => annots,
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => font, "F2" => bold }},
        }.into());
    }
    pdf.objects.insert(
        pages,
        dictionary! {
            "Type" => "Pages", "Kids" => vec![index.into(), chart.into()], "Count" => 2,
        }
        .into(),
    );
    let catalog = pdf.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages });
    pdf.trailer.set("Root", catalog);
    let mut bytes = Vec::new();
    pdf.save_to(&mut bytes).unwrap();
    bytes
}

fn open_pdf(bytes: &[u8], view: View) -> FixedReading {
    let pdf = FixedDocument::Pdf(Pdf::open(bytes.to_vec()).unwrap().into());
    FixedReading::with_source(pdf, 1, view).unwrap()
}

fn natural_anchor(reading: &FixedReading) -> Point {
    let (_, transform) = reading.single_page_projection().unwrap();
    let frame = reading.frame_size();
    transform.inverse() * Point::new(f64::from(frame.width) / 2.0, f64::from(frame.height) / 2.0)
}

fn render(reading: &FixedReading, annotation: PdfTextRect) -> Vec<u8> {
    let (width, height) = reading.viewport.dimensions();
    let fonts = ["fonts/NotoSans-Regular.ttf", "fonts/NotoSans-Bold.ttf"]
        .map(|file| crate::test_fixture(file).display().to_string());
    let mut canvas = Canvas::new(width, height, &fonts).unwrap();
    canvas.begin();
    reading.draw_with_footer(
        &mut canvas,
        None,
        &[((1, annotation), crate::highlight_style::Paint::Selection)],
        &[],
        PagePaint {
            visible: 0..reading.frame.height,
            ..PagePaint::full(false)
        },
        false,
        Rect::new(38, reading.viewport.page_height() as i32 + 5, 680, 60),
        CAPTION,
        true,
    );
    let pixels = canvas.finish(false);
    let done = reading.viewport.done();
    assert!(
        pixels
            .chunks_exact(width as usize)
            .skip(done.y as usize)
            .take(done.h as usize)
            .any(|row| row[done.x as usize..done.right() as usize]
                .iter()
                .any(|&pixel| pixel < 80))
    );
    pixels
}

fn verify_pdf_interactions(
    reading: &FixedReading,
    link: &PdfLink,
    annotation: PdfTextRect,
) -> (Point, Point) {
    let link_placement = reading.visible_rectangle(1, link.bounds).unwrap().center();
    assert_eq!(
        reading
            .link_at(link_placement.x as f32, link_placement.y as f32)
            .as_ref(),
        Some(link)
    );
    let annotation_placement = reading
        .annotation_rectangle(1, annotation)
        .unwrap()
        .center();
    let source = reading
        .source_point(
            1,
            annotation_placement.x as f32,
            annotation_placement.y as f32,
        )
        .unwrap();
    assert!((source - source_rect((1.0, 1.0), annotation).center()).hypot() < 0.0001);
    let selection = match reading
        .select_word(annotation_placement.x as f32, annotation_placement.y as f32)
        .unwrap()
    {
        WordAtTouch::Ready(Some(selection)) => selection,
        _ => panic!("the selected word must come from PDF text"),
    };
    assert_eq!(selection.text(), "NORTH");
    (link_placement, annotation_placement)
}

#[test]
fn pdf_viewport_preserves_source_crop_rotation_policy_and_actual_hit_geometry() {
    let bytes = harbour_pdf();
    let mut reading = open_pdf(
        &bytes,
        View {
            crop: Some(CROP),
            width_fit: true,
            offset_y: -45.0,
            ..View::default()
        },
    );
    let link = reading.visible_links().into_iter().next().unwrap();
    assert_eq!(link.target, crate::fixed::PdfLinkTarget::Page(0));
    let text = reading.page_text(1).unwrap();
    let annotation = text
        .words
        .iter()
        .find(|word| &text.text[word.bytes.clone()] == "NORTH")
        .unwrap()
        .bounds;
    let anchor = natural_anchor(&reading);
    let original_prefs = reading.preferences();
    assert_eq!(original_prefs.crop, Some(CROP));
    assert_ne!(original_prefs.offset_y, 0.0);
    let original_hits = verify_pdf_interactions(&reading, &link, annotation);
    let original_pixels = reading.frame.pixels.clone();
    let portrait = render(&reading, annotation);

    let wide = reading
        .prepare_viewport(ReadingViewport::LANDSCAPE)
        .unwrap();
    assert_eq!((wide.frame().width, wide.frame().height), (1680, 1190));
    assert_eq!(reading.frame.pixels, original_pixels);
    assert_eq!(reading.preferences(), original_prefs);
    assert_eq!(reading.viewport, ReadingViewport::PORTRAIT);
    assert_eq!(wide.source_point, anchor);
    reading.commit_viewport(wide).unwrap();
    assert_eq!(reading.page(), 1);
    assert!((natural_anchor(&reading) - anchor).hypot() < 0.001);
    assert_eq!(reading.preferences().crop, Some(CROP));
    assert!(reading.preferences().width_fit);
    assert_eq!(reading.preferences().turns, original_prefs.turns);
    let landscape_hits = verify_pdf_interactions(&reading, &link, annotation);
    assert_ne!(landscape_hits, original_hits);
    assert!(landscape_hits.0.x > f64::from(WIDTH));
    let landscape = render(&reading, annotation);
    assert_ne!(landscape, portrait);

    let returned = reading.prepare_viewport(ReadingViewport::PORTRAIT).unwrap();
    reading.commit_viewport(returned).unwrap();
    assert_eq!(reading.page(), 1);
    assert!((natural_anchor(&reading) - anchor).hypot() < 0.001);
    let restored = render(&reading, annotation);
    assert_eq!(restored, portrait);

    if let Some(out) = std::env::var_os("READER_FIXED_VIEWPORT_CAPTURE_DIR") {
        let out = PathBuf::from(out);
        fs::create_dir_all(&out).unwrap();
        fs::write(out.join("harbour-chart.pdf"), bytes).unwrap();
        for (file, viewport, pixels) in [
            ("pdf-portrait.png", ReadingViewport::PORTRAIT, portrait),
            ("pdf-landscape.png", ReadingViewport::LANDSCAPE, landscape),
            ("pdf-returned.png", ReadingViewport::PORTRAIT, restored),
        ] {
            let (width, height) = viewport.dimensions();
            primitive_app::write_png(&out.join(file), width, height, &pixels).unwrap();
        }
    }
}

#[test]
fn rotated_row_zoom_recomputes_fit_without_changing_source_point_or_annotation() {
    let bytes = harbour_pdf();
    let mut reading = open_pdf(
        &bytes,
        View {
            turns: 1,
            crop: Some(CROP),
            ..View::default()
        },
    );
    reading.set_tile_zoom(FixedTileAxis::Rows, 20).unwrap();
    let original = reading.preferences().tile_zoom.unwrap();
    let anchor = natural_anchor(&reading);
    let (size, _) = reading.single_page_projection().unwrap();
    let (x, y) = (anchor.x as f32 / size.0, anchor.y as f32 / size.1);
    let annotation = PdfTextRect {
        x0: x - 0.01,
        y0: y - 0.01,
        x1: x + 0.01,
        y1: y + 0.01,
    };
    let before = reading
        .annotation_rectangle(1, annotation)
        .unwrap()
        .center();
    let wide = reading
        .prepare_viewport(ReadingViewport::LANDSCAPE)
        .unwrap();
    let tile = wide.view.tile_zoom.unwrap();
    assert_eq!(
        (tile.axis, tile.count_tenths),
        (original.axis, original.count_tenths)
    );
    assert_ne!(tile.width_factor, original.width_factor);
    reading.commit_viewport(wide).unwrap();
    assert_eq!(reading.view.turns, 1);
    assert_eq!(reading.view.crop, Some(CROP));
    assert_eq!(reading.retained_viewport_point, Some(anchor));
    let after = reading
        .annotation_rectangle(1, annotation)
        .unwrap()
        .center();
    assert_ne!(before, after);
    let source = reading
        .source_point(1, after.x as f32, after.y as f32)
        .unwrap();
    assert!((source - source_rect((1.0, 1.0), annotation).center()).hypot() < 0.0001);
    let back = reading.prepare_viewport(ReadingViewport::PORTRAIT).unwrap();
    reading.commit_viewport(back).unwrap();
    assert_eq!(reading.preferences().tile_zoom, Some(original));
    assert!((natural_anchor(&reading) - anchor).hypot() < 0.001);
}

struct TempComic(PathBuf);

impl TempComic {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "viewport-comic-{}-{}.cbz",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut archive = zip::ZipWriter::new(File::create(&path).unwrap());
        for page in 0..2u8 {
            archive
                .start_file(
                    format!("{page}.pgm"),
                    zip::write::SimpleFileOptions::default(),
                )
                .unwrap();
            write!(archive, "P5\n80 100\n255\n").unwrap();
            for y in 0..100u8 {
                archive
                    .write_all(&(0..80u8).map(|x| x + y + 20 * page).collect::<Vec<_>>())
                    .unwrap();
            }
        }
        archive.finish().unwrap();
        Self(path)
    }
}

impl Drop for TempComic {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

#[test]
fn still_images_and_comic_pages_keep_source_geometry_and_refuse_missing_pages() {
    let comic = TempComic::new();
    let image =
        ImagePage::from_grayscale(80, 100, (0..8000).map(|i| (i % 251) as u8).collect()).unwrap();
    let mut still = FixedReading::with_source_height(
        FixedDocument::Image(image),
        0,
        View {
            height_fit: true,
            zoom: 2,
            turns: 3,
            offset_x: -110.0,
            ..View::default()
        },
        primitive_app::HEIGHT,
        Seeds::default(),
    )
    .unwrap();
    let mut album = FixedReading::with_source(
        FixedDocument::Comic(Comic::open(&comic.0).unwrap()),
        1,
        still.view,
    )
    .unwrap();
    for reading in [&mut still, &mut album] {
        let page = reading.page();
        let anchor = natural_anchor(reading);
        let original = reading.frame.pixels.clone();
        let wide = reading
            .prepare_viewport(ReadingViewport::LANDSCAPE)
            .unwrap();
        let overlap = reading.frame.height == primitive_app::HEIGHT;
        assert_eq!(
            (wide.frame.width, wide.frame.height),
            (1680, if overlap { 1264 } else { 1190 })
        );
        reading.commit_viewport(wide).unwrap();
        assert_eq!(reading.page(), page);
        assert!(reading.view.height_fit);
        assert_eq!(reading.view.zoom, 2);
        assert_eq!(reading.view.turns, 3);
        assert_eq!(reading.retained_viewport_point, Some(anchor));
        let back = reading.prepare_viewport(ReadingViewport::PORTRAIT).unwrap();
        reading.commit_viewport(back).unwrap();
        assert!((natural_anchor(reading) - anchor).hypot() < 0.001);
        assert_eq!(reading.frame.pixels, original);
    }
    let original = album.frame.pixels.clone();
    let original_view = album.view;
    album.turns.pending.push(1);
    fs::remove_file(&comic.0).unwrap();
    assert!(album.prepare_viewport(ReadingViewport::LANDSCAPE).is_err());
    assert_eq!(album.viewport, ReadingViewport::PORTRAIT);
    assert_eq!(album.view, original_view);
    assert_eq!(album.frame.pixels, original);
    assert_eq!(album.turns.pending, [1]);
}

#[test]
fn stale_candidates_and_continuous_layout_are_refused_without_replacing_the_frame() {
    let bytes = harbour_pdf();
    let mut reading = open_pdf(&bytes, View::default());
    let stale = reading
        .prepare_viewport(ReadingViewport::LANDSCAPE)
        .unwrap();
    reading.set_zoom(1).unwrap();
    let original = reading.frame.pixels.clone();
    assert!(reading.commit_viewport(stale).is_err());
    assert_eq!(reading.frame.pixels, original);
    let another = open_pdf(&bytes, reading.view);
    let foreign = another
        .prepare_viewport(ReadingViewport::LANDSCAPE)
        .unwrap();
    assert!(reading.commit_viewport(foreign).is_err());
    assert_eq!(reading.frame.pixels, original);

    let continuous = open_pdf(
        &bytes,
        View {
            width_fit: true,
            continuous: true,
            ..View::default()
        },
    );
    assert!(continuous.is_continuous());
    assert!(
        continuous
            .prepare_viewport(ReadingViewport::LANDSCAPE)
            .is_err()
    );
}
