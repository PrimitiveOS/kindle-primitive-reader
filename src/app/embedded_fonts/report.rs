//! The optional font report floats above the active reading controls.
use crate::app::{Change, Input, Reader, Rect};
use crate::session::screen_pages::embedded_fonts::{Job, Report, Scope};
use primitive_ui::{CAPTION, Canvas, TextStyle};
use std::time::{Duration, Instant};

const ROWS: usize = 5;
const ROW_HEIGHT: i32 = 88;
const POLL: Duration = Duration::from_millis(150);

pub(in crate::app) struct Inspection {
    report: Report,
    running: Option<Job>,
    poll_at: Instant,
    page: usize,
}

impl Drop for Inspection {
    fn drop(&mut self) {
        if let Some(job) = self.running.as_ref() {
            job.cancel();
        }
    }
}

struct Panel {
    bounds: Rect,
    previous: Rect,
    next: Rect,
    done: Rect,
}

impl Panel {
    fn new(reader: &Reader) -> Self {
        let (width, height) = reader.reading_viewport().dimensions();
        let panel_width = (width as i32 - 120).min(1072);
        let rows = reader
            .embedded_font_report
            .as_ref()
            .map_or(1, |inspection| {
                inspection.report.families.len().clamp(1, ROWS)
            });
        let panel_height = 294 + rows as i32 * ROW_HEIGHT;
        let bounds = Rect::new(
            (width as i32 - panel_width) / 2,
            (height as i32 - panel_height) / 2,
            panel_width,
            panel_height,
        );
        let bottom = bounds.bottom() - 128;
        Self {
            bounds,
            previous: Rect::new(bounds.x + 44, bottom, 212, 100),
            next: Rect::new(bounds.x + 276, bottom, 212, 100),
            done: Rect::new(bounds.right() - 316, bottom, 272, 100),
        }
    }

    fn row(&self, index: usize) -> Rect {
        Rect::new(
            self.bounds.x + 50,
            self.bounds.y + 151 + index as i32 * ROW_HEIGHT,
            self.bounds.w - 100,
            ROW_HEIGHT,
        )
    }
}

impl Inspection {
    fn last_page(&self) -> usize {
        self.report.families.len().saturating_sub(1) / ROWS
    }
}

impl Reader {
    pub(in crate::app) fn open_embedded_font_report(&mut self) {
        let Some(reading) = self.active.as_ref().map(|book| &book.reading) else {
            return;
        };
        // End any prior inspection before requesting the accepted viewport.
        self.embedded_font_report = None;
        let running = Job::start(reading).ok();
        let scope = if running.is_some() {
            Scope::Checking
        } else {
            Scope::CurrentChapter
        };
        self.embedded_font_report = Some(Inspection {
            report: Report {
                families: reading.probe.embedded_font_usage.clone(),
                scope,
            },
            running,
            poll_at: Instant::now() + POLL,
            page: 0,
        });
    }

    pub(in crate::app) fn embedded_font_report_deadline(&self) -> Option<Instant> {
        let inspection = self.embedded_font_report.as_ref()?;
        inspection.running.as_ref().map(|_| inspection.poll_at)
    }

    pub(in crate::app) fn poll_embedded_font_report(&mut self, now: Instant) -> Option<Change> {
        let inspection = self.embedded_font_report.as_mut()?;
        if !inspection.running.as_ref()?.finished() {
            inspection.poll_at = now + POLL;
            return None;
        }
        match inspection.running.take().unwrap().collect() {
            Ok(report) => inspection.report = report,
            Err(error) => {
                inspection.report.scope = Scope::CurrentChapter;
                eprintln!("reader embedded_font_inspection_failed={error}");
            }
        }
        inspection.page = inspection.page.min(inspection.last_page());
        Some(Change::Partial)
    }

    pub(in crate::app) fn draw_embedded_font_report(&self, canvas: &mut Canvas) {
        let Some(inspection) = self.embedded_font_report.as_ref() else {
            return;
        };
        let panel = Panel::new(self);
        let bounds = panel.bounds;
        canvas.popup(bounds);
        canvas.text(
            Rect::new(bounds.x + 44, bounds.y + 34, bounds.w - 88, 60),
            "Embedded fonts",
            TextStyle::new(46).bold(),
        );
        let state = match inspection.report.scope {
            Scope::Checking => "Checking the book’s font families…",
            Scope::Book => "Usage in the whole book",
            Scope::CurrentChapter => "Usage checked in the current chapter",
            Scope::Disabled => "Embedded fonts are off for this layout",
        };
        canvas.text(
            Rect::new(bounds.x + 46, bounds.y + 95, bounds.w - 92, 48),
            state,
            CAPTION.ink(85),
        );
        if inspection.report.families.is_empty() && inspection.report.scope != Scope::Checking {
            let row = panel.row(0);
            canvas.text(row, "No supported bundled fonts found", CAPTION);
        }
        for (index, family) in inspection
            .report
            .families
            .iter()
            .skip(inspection.page * ROWS)
            .take(ROWS)
            .enumerate()
        {
            let row = panel.row(index);
            let status = if family.used {
                "Used"
            } else {
                match inspection.report.scope {
                    Scope::Book | Scope::Disabled => "Not used",
                    Scope::Checking | Scope::CurrentChapter => "Usage unchecked",
                }
            };
            canvas.fill(Rect::new(row.x, row.y, row.w, 1), primitive_ui::HAIRLINE);
            canvas.text(
                Rect::new(row.x, row.y + 7, row.w - 254, row.h - 14),
                &family.name,
                TextStyle::new(35).lines(2),
            );
            canvas.text(
                Rect::new(row.right() - 244, row.y + 7, 244, row.h - 14),
                status,
                CAPTION.ink(85),
            );
        }
        if inspection.page > 0 {
            canvas.button(panel.previous, "Previous", false, false);
        }
        if inspection.page < inspection.last_page() {
            canvas.button(panel.next, "Next", false, false);
        }
        canvas.button(panel.done, "Done", true, false);
    }

    pub(in crate::app) fn embedded_font_report_press(
        &self,
        input: Input,
    ) -> Option<Option<(f32, f32)>> {
        let inspection = self.embedded_font_report.as_ref()?;
        let panel = Panel::new(self);
        Some(match input {
            Input::Tap(x, y)
                if panel.done.contains(x, y)
                    || (inspection.page > 0 && panel.previous.contains(x, y))
                    || (inspection.page < inspection.last_page() && panel.next.contains(x, y)) =>
            {
                Some((x, y))
            }
            _ => None,
        })
    }

    pub(in crate::app) fn embedded_font_report_input(&mut self, input: Input) -> Option<Change> {
        if matches!(input, Input::Sleep | Input::Wake) {
            return None;
        }
        let panel = Panel::new(self);
        let inspection = self.embedded_font_report.as_mut()?;
        if matches!(input, Input::Tap(x, y) if panel.done.contains(x, y) || !panel.bounds.contains(x, y))
            || (input == Input::PageBack && inspection.page == 0)
        {
            self.embedded_font_report = None;
            return Some(Change::Partial);
        }
        let direction = match input {
            Input::PageForward => 1,
            Input::PageBack => -1,
            Input::Swipe { dx, dy, .. } => {
                let distance = if dx.abs() > dy.abs() { dx } else { dy };
                if distance < 0.0 { 1 } else { -1 }
            }
            Input::Tap(x, y) if panel.next.contains(x, y) => 1,
            Input::Tap(x, y) if panel.previous.contains(x, y) => -1,
            _ => return Some(Change::None),
        };
        let next = inspection
            .page
            .saturating_add_signed(direction)
            .min(inspection.last_page());
        if next == inspection.page {
            return Some(Change::None);
        }
        inspection.page = next;
        Some(Change::Partial)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{
        BarPanel,
        embedded_fonts::tests::{open_rendering, reader, write_reference_book},
        selection_viewport_tests::{adopt, capture, tap},
        tests::Fixture,
    };
    use crate::session::ReadingViewport;
    use primitive_app::App;
    use std::{fs, thread};

    fn wait(reader: &mut Reader) -> &Inspection {
        wait_for(reader, Duration::from_secs(15))
    }

    fn wait_for(reader: &mut Reader, timeout: Duration) -> &Inspection {
        let until = Instant::now() + timeout;
        loop {
            assert!(Instant::now() < until, "font inspection did not finish");
            if reader.embedded_font_report_deadline().is_none() {
                return reader.embedded_font_report.as_ref().unwrap();
            }
            reader.tick(Instant::now());
            thread::sleep(Duration::from_millis(3));
        }
    }

    fn family_values(inspection: &Inspection) -> Vec<(&str, bool)> {
        inspection
            .report
            .families
            .iter()
            .map(|family| (family.name.as_ref(), family.used))
            .collect()
    }

    #[test]
    fn embedded_font_report_uses_all_layouts_and_restores_the_rendering_panel() {
        let fixture = Fixture::new();
        let path = fixture.root.join("Harbor font report.epub");
        write_reference_book(&path, "report");
        if let Some(destination) = std::env::var_os("READER_EMBEDDED_FONT_REPORT_FIXTURE") {
            fs::copy(&path, destination).unwrap();
        }
        let original = fs::read(&path).unwrap();
        let mut app = reader(&fixture);
        app.open_path(&path).unwrap();
        let source = app.current_history_location();
        let history = app.back.clone();
        let mut canvas = adopt(&mut app, ReadingViewport::PORTRAIT, 1);
        open_rendering(&mut app);
        let hold = app.reading_row_help_rect("Embedded fonts");
        assert_eq!(
            app.input(Input::LongTap(
                (hold.x + hold.w / 2) as f32,
                (hold.y + hold.h / 2) as f32,
            )),
            Change::Partial
        );
        let report = wait(&mut app);
        assert_eq!(report.report.scope, Scope::Book);
        assert_eq!(family_values(report), [("Quarry", false), ("Voyage", true)]);
        assert!(
            app.active
                .as_ref()
                .unwrap()
                .reading
                .probe
                .embedded_font_usage
                .is_empty()
        );
        capture(&app, &mut canvas, "embedded-fonts-report.png", false);
        let mut wide = adopt(&mut app, ReadingViewport::LANDSCAPE, 2);
        let report = wait(&mut app);
        assert_eq!(family_values(report), [("Quarry", false), ("Voyage", true)]);
        let done = Panel::new(&app).done;
        tap(&mut app, done);
        assert!(app.embedded_font_report.is_none());
        assert_eq!(app.reading_panel, Some(BarPanel::TextRendering));
        let off = app.reading_button_in_row("Embedded fonts", "Off");
        tap(&mut app, off);
        let details = app.reading_button_in_row("Embedded fonts", "Details");
        tap(&mut app, details);
        let report = wait(&mut app);
        assert_eq!(report.report.scope, Scope::Disabled);
        assert_eq!(
            family_values(report),
            [("Quarry", false), ("Voyage", false)]
        );
        capture(
            &app,
            &mut wide,
            "embedded-fonts-report-off-landscape.png",
            false,
        );
        assert_eq!(app.input(Input::PageBack), Change::Partial);
        assert!(app.embedded_font_report.is_none());
        assert_eq!(app.current_history_location(), source);
        assert_eq!(app.back, history);
        assert_eq!(fs::read(&path).unwrap(), original);
    }

    #[test]
    fn kf8_embedded_font_report_scans_later_usage_and_omits_invalid_faces() {
        let fixture = Fixture::new();
        let path = fixture.root.join("Shore type specimen.azw3");
        let silk = fs::read(crate::test_fixture("fonts/Silkscreen-Regular.ttf")).unwrap();
        let noto = fs::read(crate::test_fixture("fonts/NotoSans-Regular.ttf")).unwrap();
        let mut invalid = silk.clone();
        invalid[..4].copy_from_slice(b"JUNK");
        let record = crate::kf8::fonts::tests::record;
        let bytes = crate::kf8::tests::font_report_fixture(&[
            &record(&silk, true, true),
            &record(&noto, false, false),
            &record(&invalid, true, false),
        ]);
        fs::write(&path, &bytes).unwrap();
        if let Some(output) = std::env::var_os("READER_EMBEDDED_FONT_REPORT_KF8_FIXTURE") {
            fs::write(output, &bytes).unwrap();
        }

        let mut app = reader(&fixture);
        app.open_path(&path).unwrap();
        let source = app.current_history_location();
        let history = app.back.clone();
        let mut canvas = adopt(&mut app, ReadingViewport::PORTRAIT, 1);
        open_rendering(&mut app);
        let details = app.reading_button_in_row("Embedded fonts", "Details");
        tap(&mut app, details);
        let report = wait(&mut app);
        assert_eq!(report.report.scope, Scope::Book);
        assert_eq!(family_values(report), [("Quarry", false), ("Voyage", true)]);
        let current = &app
            .active
            .as_ref()
            .unwrap()
            .reading
            .probe
            .embedded_font_usage;
        assert!(current.iter().all(|family| !family.used));
        capture(&app, &mut canvas, "embedded-fonts-report-kf8.png", false);

        let done = Panel::new(&app).done;
        tap(&mut app, done);
        let off = app.reading_button_in_row("Embedded fonts", "Off");
        tap(&mut app, off);
        let details = app.reading_button_in_row("Embedded fonts", "Details");
        tap(&mut app, details);
        let report = wait(&mut app);
        assert_eq!(report.report.scope, Scope::Disabled);
        assert_eq!(
            family_values(report),
            [("Quarry", false), ("Voyage", false)]
        );
        assert_eq!(app.input(Input::PageBack), Change::Partial);
        assert_eq!(app.current_history_location(), source);
        assert_eq!(app.back, history);
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }

    #[test]
    #[ignore = "Set READER_KF8_FONT_REPORT_ORIGINALS to a private list of authorized AZW3 paths"]
    fn original_kf8_embedded_font_reports_keep_the_passage_and_source_bytes() {
        use std::path::Path;
        let list = std::env::var_os("READER_KF8_FONT_REPORT_ORIGINALS").unwrap();
        let list = fs::read_to_string(list).unwrap();
        let mut total = 0;
        let mut books = 0;
        let mut used = 0;
        let mut unused = 0;
        let mut unchecked = 0;
        let mut empty = 0;
        for source in list.lines().filter(|line| !line.is_empty()) {
            let path = Path::new(source);
            let before = fs::read(path).unwrap();
            let fixture = Fixture::new();
            let fonts = ["NotoSans-Regular.ttf", "Silkscreen-Regular.ttf"]
                .map(|font| crate::test_fixture(&format!("fonts/{font}")));
            let mut app =
                Reader::open(path.parent().unwrap(), &fixture.state, fonts.to_vec()).unwrap();
            app.open_path(path).unwrap();
            let source = app.current_history_location();
            let history = app.back.clone();
            open_rendering(&mut app);
            // Broken resources may leave the row disabled. Its help can still open.
            let help = app.reading_row_help_rect("Embedded fonts");
            assert_eq!(
                app.input(Input::LongTap(
                    (help.x + help.w / 2) as f32,
                    (help.y + help.h / 2) as f32,
                )),
                Change::Partial
            );
            let report = wait_for(&mut app, Duration::from_secs(90));
            assert!(matches!(
                report.report.scope,
                Scope::Book | Scope::CurrentChapter
            ));
            let exact = report.report.scope == Scope::Book;
            books += usize::from(exact);
            empty += usize::from(report.report.families.is_empty());
            for family in report.report.families.iter() {
                if family.used {
                    used += 1;
                } else if exact {
                    unused += 1;
                } else {
                    unchecked += 1;
                }
            }
            assert_eq!(app.input(Input::PageBack), Change::Partial);
            assert_eq!(app.current_history_location(), source);
            assert_eq!(app.back, history);
            assert!(
                fs::read(path).unwrap() == before,
                "original KF8 bytes changed"
            );
            total += 1;
        }
        println!(
            "Original KF8 reports: {total} books, {books} whole-book scans, {used} used, {unused} unused, {unchecked} unchecked, {empty} empty reports"
        );
        assert!(total >= 3 && books >= 2 && used > 0 && unused > 0 && empty > 0);
    }

    #[test]
    fn embedded_font_report_keeps_unknown_chapters_distinct_and_drops_work_on_sleep() {
        let fixture = Fixture::new();
        let path = fixture.root.join("Harbor specimen.epub");
        write_reference_book(&path, "report");
        let folder = fixture.root.join("Extracted specimen.epub");
        fs::create_dir(&folder).unwrap();
        zip::ZipArchive::new(fs::File::open(&path).unwrap())
            .unwrap()
            .extract(&folder)
            .unwrap();
        let mut app = reader(&fixture);
        app.open_path(&folder.join("mimetype")).unwrap();
        let source = app.current_history_location();
        let mut canvas = adopt(&mut app, ReadingViewport::PORTRAIT, 1);
        open_rendering(&mut app);
        let details = app.reading_button_in_row("Embedded fonts", "Details");
        tap(&mut app, details);
        let report = wait(&mut app);
        assert_eq!(report.report.scope, Scope::CurrentChapter);
        assert_eq!(
            family_values(report),
            [("Quarry", false), ("Voyage", false)]
        );
        capture(
            &app,
            &mut canvas,
            "embedded-fonts-report-unchecked.png",
            false,
        );
        assert_ne!(app.input(Input::Sleep), Change::Exit);
        assert!(app.embedded_font_report.is_none());
        assert!(app.embedded_font_report_deadline().is_none());
        assert_ne!(app.input(Input::Wake), Change::Exit);
        assert_eq!(app.current_history_location(), source);
    }

    #[test]
    fn embedded_font_report_pages_preserve_the_background_and_dismiss_once() {
        let fixture = Fixture::new();
        let path = fixture.root.join("Long font list.epub");
        write_reference_book(&path, "paged-report");
        let mut app = reader(&fixture);
        app.open_path(&path).unwrap();
        let source = app.current_history_location();
        open_rendering(&mut app);
        let details = app.reading_button_in_row("Embedded fonts", "Details");
        tap(&mut app, details);
        let inspection = wait(&mut app);
        assert_eq!(inspection.report.scope, Scope::Book);
        assert_eq!(inspection.report.families.len(), 7);
        assert_eq!(inspection.page, 0);
        let next = Panel::new(&app).next;
        tap(&mut app, next);
        assert_eq!(app.embedded_font_report.as_ref().unwrap().page, 1);
        assert_eq!(app.input(Input::PageForward), Change::None);
        let previous = Panel::new(&app).previous;
        tap(&mut app, previous);
        assert_eq!(app.input(Input::PageForward), Change::Partial);
        assert_eq!(app.embedded_font_report.as_ref().unwrap().page, 1);
        assert_eq!(app.input(Input::Tap(30.0, 30.0)), Change::Partial);
        assert!(app.embedded_font_report.is_none());
        assert_eq!(app.reading_panel, Some(BarPanel::TextRendering));
        assert_eq!(app.current_history_location(), source);
    }
}
