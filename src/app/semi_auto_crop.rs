use super::*;
use crate::fixed_session::CropEdit;
use crate::pdf_crop::Seed;
use primitive_ui::GrayImage;

#[cfg(test)]
mod continuous_tests;

const AUTO: Rect = Rect::new(489, PAGE_HEIGHT as i32 + 5, 170, 60);
const RESET: Rect = Rect::new(671, PAGE_HEIGHT as i32 + 5, 170, 60);
pub(super) const APPLY: Rect = Rect::new(853, PAGE_HEIGHT as i32 + 5, 180, 60);

#[derive(Clone, Copy)]
pub(super) struct Draft {
    original: FixedPreferences,
    viewport_height: u32,
    anchor: Option<hayro::kurbo::Point>,
    seed: Seed,
    size: (f32, f32),
    manual: bool,
}

impl Reader {
    pub(super) fn start_native_pdf_crop(&mut self, manual: bool) -> Result<()> {
        self.close_fixed_search_fit()?;
        let fixed = self.fixed.as_mut().ok_or("Open a PDF first")?;
        if !fixed.reading.supports_semi_auto_crop() {
            return Err("Semi-auto crop is available for PDF".into());
        }
        let (seed, size) = fixed.reading.initial_crop_seed(manual)?;
        let anchor = fixed.reading.history_point();
        let (original, viewport_height) = fixed.reading.start_crop_preview()?;
        self.crop_dialog = None;
        self.semi_auto_crop = Some(Draft {
            original,
            viewport_height,
            anchor,
            seed,
            size,
            manual,
        });
        self.show(Screen::FixedCrop);
        Ok(())
    }

    pub(super) fn semi_auto_crop_input(&mut self, input: Input) -> Option<Change> {
        let draft = self.semi_auto_crop?;
        let viewport = self.reading_viewport();
        let action = match input {
            Input::PageBack => self.cancel_semi_auto_crop(draft),
            Input::Tap(x, y) if viewport.bottom_right(CANCEL_CROP).contains(x, y) => {
                self.cancel_semi_auto_crop(draft)
            }
            Input::Tap(x, y) if draft.manual && viewport.bottom_right(AUTO).contains(x, y) => {
                self.apply_native_pdf_crop(draft, CropEdit::Auto)
            }
            Input::Tap(x, y) if draft.manual && viewport.bottom_right(RESET).contains(x, y) => {
                self.apply_native_pdf_crop(draft, CropEdit::Reset)
            }
            Input::Tap(x, y) if viewport.bottom_right(APPLY).contains(x, y) => {
                let edit = if draft.manual {
                    CropEdit::Manual(draft.seed)
                } else {
                    CropEdit::SemiAuto(draft.seed)
                };
                self.apply_native_pdf_crop(draft, edit)
            }
            Input::Tap(x, y) if y < viewport.page_height() as f32 => {
                self.adjust_semi_auto_crop(draft, x, y)
            }
            _ => return Some(Change::None),
        };
        Some(match action {
            Ok(()) => Change::Partial,
            Err(error) => self.report(error),
        })
    }

    fn adjust_semi_auto_crop(&mut self, draft: Draft, x: f32, y: f32) -> Result<()> {
        let fixed = self.fixed.as_ref().ok_or("Open a PDF first")?;
        let (x, y) = fixed
            .reading
            .crop_point(x, y)
            .ok_or("Tap within the page")?;
        let seed = draft
            .seed
            .adjust(x, y, draft.size)
            .ok_or("Choose a larger area of the page")?;
        self.semi_auto_crop = Some(Draft { seed, ..draft });
        self.notice = None;
        Ok(())
    }

    fn cancel_semi_auto_crop(&mut self, draft: Draft) -> Result<()> {
        let fixed = self.fixed.as_mut().ok_or("Open a PDF first")?;
        fixed
            .reading
            .restore_crop_preview(draft.original, draft.viewport_height)?;
        self.semi_auto_crop = None;
        self.show(Screen::Fixed);
        Ok(())
    }

    fn apply_native_pdf_crop(&mut self, draft: Draft, edit: CropEdit) -> Result<()> {
        let fixed = self.fixed.as_mut().ok_or("Open a PDF first")?;
        let library = &mut self.library;
        fixed.reading.apply_crop(
            draft.original,
            draft.viewport_height,
            edit,
            draft.anchor,
            |page, view, seeds| {
                let state = library.remember(&fixed.entry, fixed.id, &fixed.entry.title);
                let before = (
                    std::mem::replace(&mut state.fixed_preferences, view),
                    std::mem::replace(&mut state.pdf_crop_seeds, seeds.clone()),
                    state.position.replace(ReadingPosition {
                        chapter: 0,
                        page,
                        anchor: None,
                    }),
                );
                if let Err(error) = library.persist() {
                    let state = library.remember(&fixed.entry, fixed.id, &fixed.entry.title);
                    (
                        state.fixed_preferences,
                        state.pdf_crop_seeds,
                        state.position,
                    ) = before;
                    return Err(error);
                }
                Ok(())
            },
        )?;
        self.semi_auto_crop = None;
        self.show(Screen::Fixed);
        self.record_position();
        Ok(())
    }

    pub(super) fn draw_semi_auto_crop(&self, canvas: &mut Canvas) -> bool {
        let (Some(draft), Some(fixed)) = (self.semi_auto_crop, self.fixed.as_ref()) else {
            return false;
        };
        let reading = &fixed.reading;
        let viewport = reading.viewport();
        let (width, height) = viewport.dimensions();
        let top = viewport.page_height() as i32;
        reading.draw(canvas, None, &[], &[], self.page_paint());
        if let Some(bounds) = draft
            .seed
            .normalized(draft.size)
            .and_then(|bounds| reading.visible_rectangle(reading.page(), bounds))
        {
            let (x, y) = (bounds.x0.round() as i32, bounds.y0.round() as i32);
            let (right, bottom) = (bounds.x1.round() as i32, bounds.y1.round() as i32);
            // Pictures compose after UI-layer fills, so draw the handles
            // as a second picture over the document raster.
            let ink = if self.uses_night_pages() { 255 } else { 0 };
            let paint = |canvas: &mut Canvas, rect: Rect| {
                let (x, y) = (rect.x.max(0), rect.y.max(0));
                let rect = Rect::new(
                    x,
                    y,
                    rect.right().min(width as i32) - x,
                    rect.bottom().min(top) - y,
                );
                if rect.w > 0 && rect.h > 0 {
                    canvas.picture(
                        rect,
                        GrayImage {
                            width: rect.w as u32,
                            height: rect.h as u32,
                            pixels: vec![ink; rect.w as usize * rect.h as usize],
                        },
                    );
                }
            };
            for band in [
                Rect::new(x, y, right - x, 4),
                Rect::new(x, bottom.saturating_sub(4), right - x, 4),
                Rect::new(x, y, 4, bottom - y),
                Rect::new(right.saturating_sub(4), y, 4, bottom - y),
            ] {
                paint(canvas, band);
            }
            for handle_y in [y, (y + bottom) / 2, bottom] {
                for handle_x in [x, (x + right) / 2, right] {
                    if handle_x != (x + right) / 2 || handle_y != (y + bottom) / 2 {
                        paint(canvas, Rect::new(handle_x - 8, handle_y - 8, 16, 16));
                    }
                }
            }
        }
        canvas.fill(Rect::new(0, top, width as i32, height as i32 - top), 255);
        let auto = viewport.bottom_right(AUTO);
        let apply = viewport.bottom_right(APPLY);
        let (first_button, help) = if draft.manual {
            draw_toolbar_button(canvas, auto, "Auto crop");
            draw_toolbar_button(canvas, viewport.bottom_right(RESET), "Reset");
            (auto, "Adjust the crop handles")
        } else {
            (apply, "Tap near an edge or corner to adjust the crop")
        };
        canvas.text(
            Rect::new(28, top + 12, first_button.x - 56, 52),
            self.notice.as_deref().unwrap_or(help),
            CAPTION,
        );
        draw_toolbar_button(canvas, apply, "Apply");
        draw_toolbar_button(canvas, viewport.bottom_right(CANCEL_CROP), "Cancel");
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::{Fixture, capture_test_screen};
    use crate::pdf_crop::tests::{SIZE, even_seed, fixture, odd_seed};

    fn tap_center(area: Rect) -> Input {
        Input::Tap((area.x + area.w / 2) as f32, (area.y + area.h / 2) as f32)
    }

    fn normalized_box(x0: f32, y0: f32, x1: f32, y1: f32) -> PdfTextRect {
        PdfTextRect { x0, y0, x1, y1 }
    }

    fn fixed(app: &Reader) -> &FixedReading {
        &app.fixed.as_ref().unwrap().reading
    }

    #[test]
    fn crop_viewport_keeps_the_rotated_pdf_source_on_cancel_and_apply() {
        use primitive_app::viewport::{LogicalTurn, Viewport, ViewportDecision, ViewportRequest};

        let paths = Fixture::new();
        let path = paths.root.join("Landscape crop sample.pdf");
        let bytes = fixture();
        fs::write(&path, &bytes).unwrap();
        let fonts = vec![
            crate::test_fixture("fonts/NotoSans-Regular.ttf"),
            crate::test_fixture("fonts/NotoSans-Bold.ttf"),
        ];
        let font_paths = fonts
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>();
        let mut app = Reader::open(&paths.root, &paths.state, fonts).unwrap();
        app.open_path(&path).unwrap();
        app.activate(Action::SetReadingControls(ReadingControls {
            overlap_footer: true,
            ..app.controls
        }))
        .unwrap();
        app.activate(Action::SetFixedRotation(1)).unwrap();
        let target = Viewport {
            generation: 1,
            turn: LogicalTurn::Clockwise,
            logical_size: (1680, 1264),
        };
        let mut canvas = Canvas::new_viewport(1680, 1264, WIDTH, &font_paths).unwrap();
        app.stage_viewport(target).unwrap();
        canvas.begin();
        app.draw_staged_viewport(&mut canvas).unwrap();
        canvas.finish(false);
        app.finish_viewport(target, ViewportDecision::Commit);
        let viewport = app.reading_viewport();
        let original = fixed(&app).preferences();
        let location = app.current_history_location();
        let id = app.open_book_id().unwrap();
        let apply = viewport.bottom_right(APPLY);
        let cancel = viewport.bottom_right(CANCEL_CROP);
        assert!(cancel.x > WIDTH as i32 && cancel.bottom() < 1264);
        assert_eq!(fixed(&app).page_viewport().unwrap().1.height, 1264);
        app.activate(Action::SemiAutoCrop).unwrap();
        assert_eq!(app.screen, Screen::FixedCrop);
        assert_eq!(
            app.viewport_request(),
            Some(ViewportRequest::Fixed(target.turn))
        );
        assert!(!app.viewport_needs_stage(target));
        assert_eq!(
            fixed(&app).page_viewport().unwrap().1.height,
            viewport.page_height()
        );
        assert!(
            app.stage_viewport(Viewport {
                generation: 2,
                turn: LogicalTurn::Upright,
                logical_size: (1264, 1680),
            })
            .is_err()
        );
        assert_eq!(app.input(tap_center(CANCEL_CROP)), Change::None);
        assert_eq!(app.input(tap_center(cancel)), Change::Partial);
        assert_eq!(app.screen, Screen::Fixed);
        assert_eq!(app.current_history_location(), location);
        assert_eq!(fixed(&app).preferences(), original);
        assert_eq!(fixed(&app).page_viewport().unwrap().1.height, 1264);

        app.activate(Action::SemiAutoCrop).unwrap();
        let draft = app.semi_auto_crop.unwrap();
        let before = draft.seed.normalized(draft.size).unwrap();
        let source_x = before.x0 + (before.x1 - before.x0) * 0.12;
        let source_y = (before.y0 + before.y1) * 0.5;
        let source = normalized_box(
            source_x - 0.001,
            source_y - 0.001,
            source_x + 0.001,
            source_y + 0.001,
        );
        let tap = fixed(&app).visible_rectangle(0, source).unwrap().center();
        assert_eq!(
            app.input(Input::Tap(tap.x as f32, tap.y as f32)),
            Change::Partial
        );
        let changed = app.semi_auto_crop.unwrap().seed;
        let bounds = changed.normalized(draft.size).unwrap();
        assert!((bounds.x0 - source_x).abs() < 0.001);
        assert_eq!(bounds.y0, before.y0);
        assert_eq!(bounds.x1, before.x1);
        assert_eq!(bounds.y1, before.y1);
        assert!(app.library.state(id).unwrap().pdf_crop_seeds.is_empty());
        let frame = primitive_app::frame(&app, &mut canvas, false);
        let (width, height) = viewport.dimensions();
        let footer = &frame[viewport.page_height() as usize * width as usize..];
        assert!(footer.iter().any(|&ink| ink < 50));
        if let Some(path) = std::env::var_os("READER_LANDSCAPE_CROP_CAPTURE") {
            primitive_app::write_png(Path::new(&path), width, height, &frame).unwrap();
        }
        assert_eq!(app.input(tap_center(apply)), Change::Partial);
        assert_eq!(app.screen, Screen::Fixed);
        assert_eq!(app.reading_viewport(), viewport);
        assert_eq!(fixed(&app).preferences().turns, original.turns);
        assert_eq!(app.current_position().unwrap().page, 0);
        assert_eq!(
            app.library.state(id).unwrap().pdf_crop_seeds.get(0),
            Some(changed)
        );
        assert_eq!(fs::read(path).unwrap(), bytes);
    }

    #[test]
    fn scan_margin_specks_disappear_after_apply_without_moving_source_content() {
        let paths = Fixture::new();
        let path = paths.root.join("Scan with margin specks.pdf");
        let source = crate::pdf_crop::source_margin_tests::fixture("edge-specks");
        fs::write(&path, &source).unwrap();
        let fonts = vec![
            crate::test_fixture("fonts/NotoSans-Regular.ttf"),
            crate::test_fixture("fonts/NotoSans-Bold.ttf"),
        ];
        let mut app = Reader::open(&paths.root, &paths.state, fonts.clone()).unwrap();
        app.open_path(&path).unwrap();
        app.activate(Action::SemiAutoCrop).unwrap();
        let draft = app.semi_auto_crop.as_mut().unwrap();
        draft.seed = crate::pdf_crop::source_margin_tests::seed(draft.size);
        let distant_speck = normalized_box(0.15, 0.19, 0.156, 0.204);
        let line = normalized_box(0.26, 0.32, 0.62, 0.33);
        assert!(fixed(&app).visible_rectangle(0, distant_speck).is_some());
        capture_test_screen(&app, &fonts, "READER_SCAN_MARGIN_BEFORE_FRAME", false);
        assert_eq!(app.input(tap_center(APPLY)), Change::Partial);
        assert!(fixed(&app).visible_rectangle(0, distant_speck).is_none());
        assert!(fixed(&app).visible_rectangle(0, line).is_some());
        assert_eq!(fs::read(&path).unwrap(), source);
        capture_test_screen(&app, &fonts, "READER_SCAN_MARGIN_AFTER_FRAME", false);
    }

    #[test]
    fn scan_margin_specks_recalculation_retains_the_saved_native_viewport() {
        let paths = Fixture::new();
        let path = paths.root.join("Saved scan with margin specks.pdf");
        fs::write(
            &path,
            crate::pdf_crop::source_margin_tests::fixture("edge-specks"),
        )
        .unwrap();
        let previous = FixedPreferences {
            zoom: 4,
            crop: Some(normalized_box(0.144, 0.165, 0.855, 0.84)),
            semi_auto_crop: true,
            ..FixedPreferences::default()
        };
        let mut reading = FixedReading::open(
            PAGE_HEIGHT,
            DocumentFormat::Pdf,
            &path,
            0,
            &[],
            previous,
            None,
            None,
            None,
        )
        .unwrap()
        .unwrap();
        let origin = reading.history_point().unwrap();
        let mut sources = crate::pdf_crop::Seeds::default();
        sources.record(0, crate::pdf_crop::source_margin_tests::seed(SIZE));
        reading.load_crop_seeds(sources).unwrap();
        assert_ne!(reading.preferences().crop, previous.crop);
        assert!((reading.history_point().unwrap() - origin).hypot() < 0.01);
        assert_eq!(reading.preferences().zoom, previous.zoom);
    }

    #[test]
    fn failed_semi_auto_crop_save_keeps_the_draft_and_existing_page_seed() {
        let paths = Fixture::new();
        let path = paths.root.join("Crop transaction.pdf");
        fs::write(&path, fixture()).unwrap();
        let mut app = Reader::open(&paths.root, &paths.state, Vec::new()).unwrap();
        app.open_path(&path).unwrap();
        let reading = &mut app.fixed.as_mut().unwrap().reading;
        let (original, height) = reading.start_crop_preview().unwrap();
        reading
            .apply_crop(
                original,
                height,
                CropEdit::SemiAuto(odd_seed()),
                None,
                |_, _, _| Ok(()),
            )
            .unwrap();
        let applied = reading.preferences();
        let (original, height) = reading.start_crop_preview().unwrap();
        let preview = reading.preferences();
        assert!(
            reading
                .apply_crop(
                    original,
                    height,
                    CropEdit::SemiAuto(even_seed()),
                    None,
                    |_, _, _| { Err(io::Error::other("mock storage failure")) }
                )
                .is_err()
        );
        assert_eq!(reading.preferences(), preview);
        assert!(
            reading
                .initial_crop_seed(false)
                .unwrap()
                .0
                .normalized(SIZE)
                .unwrap()
                .x0
                < 0.12
        );
        reading.restore_crop_preview(original, height).unwrap();
        assert_eq!(reading.preferences(), applied);
    }

    #[test]
    fn semi_auto_crop_cancels_exactly_and_reopens_with_page_seeds_and_annotation_geometry() {
        let paths = Fixture::new();
        let path = paths.root.join("Alternating scan margins.pdf");
        fs::write(&path, fixture()).unwrap();
        let fonts = vec![
            crate::test_fixture("fonts/NotoSans-Regular.ttf"),
            crate::test_fixture("fonts/NotoSans-Bold.ttf"),
        ];
        let mut app = Reader::open(&paths.root, &paths.state, fonts.clone()).unwrap();
        app.open_path(&path).unwrap();
        let id = app.open_book_id().unwrap();
        assert!(
            app.scan_items()
                .iter()
                .any(|item| item.title == "Semi-auto crop")
        );
        app.activate(Action::SetReadingControls(ReadingControls {
            overlap_footer: true,
            ..app.controls
        }))
        .unwrap();
        app.activate(Action::SetFixedRotation(1)).unwrap();
        app.activate(Action::SetFixedTileZoom(FixedTileAxis::Columns, 15))
            .unwrap();
        app.input(Input::Swipe {
            dx: -40.0,
            dy: -70.0,
            start_y: 600.0,
        });
        let original = fixed(&app).preferences();
        let point = fixed(&app).history_point();
        assert_eq!(fixed(&app).page_viewport().unwrap().1.height, HEIGHT);
        let keep = normalized_box(0.4, 0.4, 0.45, 0.45);
        let exclude = normalized_box(0.025, 0.14, 0.05, 0.16);
        let mut state = app.library.state(id).unwrap().pdf_highlights.clone();
        for (word, bounds) in [("Retained", keep), ("Outside", exclude)] {
            state.push(PdfHighlight {
                updated_at: None,
                chapter_title: None,
                created_at: None,
                style: Default::default(),
                color: Default::default(),
                page: 0,
                bounds: vec![bounds],
                text: word.into(),
                note: Some(word.into()),
                continuation: Vec::new(),
            });
        }
        let active = app.fixed.as_ref().unwrap();
        app.library
            .remember(&active.entry, id, &active.entry.title)
            .pdf_highlights = state;
        app.mark_dirty();

        app.activate(Action::SemiAutoCrop).unwrap();
        assert_eq!(fixed(&app).page_viewport().unwrap().1.height, PAGE_HEIGHT);
        let center = fixed(&app).visible_rectangle(0, keep).unwrap().center();
        app.input(Input::Tap(center.x as f32, center.y as f32));
        assert_eq!(app.screen, Screen::FixedCrop);
        assert!(app.saved_popup.is_none());
        app.save().unwrap();
        assert_eq!(app.library.state(id).unwrap().fixed_preferences, original);
        assert!(app.library.state(id).unwrap().pdf_crop_seeds.is_empty());
        app.input(tap_center(CANCEL_CROP));
        assert_eq!(fixed(&app).preferences(), original);
        assert_eq!(fixed(&app).history_point(), point);
        assert_eq!(fixed(&app).page_viewport().unwrap().1.height, HEIGHT);

        app.activate(Action::SemiAutoCrop).unwrap();
        app.semi_auto_crop.as_mut().unwrap().seed = odd_seed();
        capture_test_screen(&app, &fonts, "READER_SEMI_AUTO_CROP_FRAME", false);
        assert_eq!(app.input(tap_center(APPLY)), Change::Partial);
        let updated = fixed(&app).preferences();
        assert!(updated.semi_auto_crop);
        assert_eq!(updated.tile_zoom, original.tile_zoom);
        assert_eq!(updated.turns, original.turns);
        assert_eq!(
            app.library.state(id).unwrap().pdf_crop_seeds.get(0),
            Some(odd_seed())
        );
        assert_eq!(fixed(&app).visible_links().len(), 1);
        assert!(fixed(&app).visible_rectangle(0, exclude).is_none());
        let center = fixed(&app).visible_rectangle(0, keep).unwrap().center();
        app.input(Input::Tap(center.x as f32, center.y as f32));
        assert!(app.saved_popup.is_some());
        app.saved_popup = None;
        let link = fixed(&app).visible_links().remove(0);
        let center = fixed(&app)
            .visible_rectangle(0, link.bounds)
            .unwrap()
            .center();
        app.input(Input::Tap(center.x as f32, center.y as f32));
        assert_eq!(app.current_position().unwrap().page, 1);
        app.activate(Action::SemiAutoCrop).unwrap();
        app.semi_auto_crop.as_mut().unwrap().seed = even_seed();
        assert_eq!(app.input(tap_center(APPLY)), Change::Partial);
        app.activate(Action::FixedPage(3)).unwrap();
        let saved = fixed(&app).preferences();
        assert_eq!(saved.crop, even_seed().normalized(SIZE));
        app.save().unwrap();

        let mut reopened = Reader::open(&paths.root, &paths.state, fonts).unwrap();
        reopened.open_path(&path).unwrap();
        assert_eq!(fixed(&reopened).preferences(), saved);
        assert_eq!(reopened.current_position().unwrap().page, 3);
        reopened.activate(Action::FixedPage(2)).unwrap();
        let area = fixed(&reopened).preferences().crop.unwrap();
        assert!((area.x0 - 197.70259 / 600.0).abs() < 0.002);
        assert!((area.y0 - 217.70259 / 800.0).abs() < 0.002);
    }
}
