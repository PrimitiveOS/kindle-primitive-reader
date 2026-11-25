//! Transient page preview over the current reading view.

use std::collections::HashMap;

use primitive_app::{App, Change};
#[cfg(test)]
use primitive_app::{HEIGHT, WIDTH};
use primitive_ui::{Canvas, Icon, Input, Rect, TextStyle};

use super::bar_icons::{BOOKMARK, FIRST_PAGE, LAST_PAGE};
use super::chapter_navigation::Direction;
use super::{
    Action, HistoryLocation, Reader, ReadingPosition, Screen, fixed_page_label,
    reflow_mark_location,
};
use crate::fixed_session::FixedJump;
use crate::reading_controls::SkimPosition;
use crate::session::ReadingViewport;

pub(super) struct Layout {
    mirrored: bool,
    pub(super) panel: Rect,
    pub(super) track: Rect,
    previous_ten: Rect,
    previous: Rect,
    number: Rect,
    origin: Rect,
    next: Rect,
    next_ten: Rect,
    chapters: [Rect; 2],
    marks: [Rect; 3],
    pub(super) cancel: Rect,
    pub(super) commit: Rect,
}

#[cfg(test)]
mod placement_tests {
    use super::*;
    use crate::app::{
        Action,
        tests::{Fixture, capture_test_screen},
    };

    fn tap(rect: Rect) -> Input {
        Input::Tap((rect.x + rect.w / 2) as f32, (rect.y + rect.h / 2) as f32)
    }

    fn hold(rect: Rect) -> Input {
        let Input::Tap(x, y) = tap(rect) else {
            unreachable!();
        };
        Input::LongTap(x, y)
    }

    #[test]
    fn skim_chapters_and_boundaries_keep_the_opening_location_until_accepted() {
        let fixture = Fixture::new();
        std::fs::copy(crate::test_fixture("many-chapters.epub"), &fixture.book).unwrap();
        let fonts = vec![
            crate::test_fixture("fonts/NotoSans-Regular.ttf"),
            crate::test_fixture("fonts/NotoSans-Bold.ttf"),
        ];
        let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
        app.open_path(&fixture.book).unwrap();
        let origin = app.current_history_location().unwrap();
        app.activate(Action::Skim).unwrap();
        let layout = app.skim_layout();
        for position in [
            SkimPosition::Top,
            SkimPosition::Center,
            SkimPosition::Bottom,
        ] {
            app.controls.skim_position = position;
            let capture = match position {
                SkimPosition::Top => "READER_SKIM_SHORTCUTS_TOP",
                SkimPosition::Center => "READER_SKIM_SHORTCUTS_CENTER",
                SkimPosition::Bottom => "READER_SKIM_SHORTCUTS_BOTTOM",
            };
            capture_test_screen(&app, &fonts, capture, position == SkimPosition::Bottom);
        }
        app.controls.skim_position = SkimPosition::Center;

        assert_eq!(app.input(tap(layout.chapters[1])), Change::Partial);
        assert_eq!(app.screen, Screen::Skim);
        assert_eq!(app.current_position().unwrap().chapter, 1);
        assert_eq!(app.input(tap(layout.chapters[0])), Change::Partial);
        assert_eq!(app.current_history_location().as_ref(), Some(&origin));
        assert_eq!(app.input(hold(layout.chapters[1])), Change::Partial);
        let reading = &app.active.as_ref().unwrap().reading;
        assert_eq!(reading.chapter + 1, reading.probe.chapter_count());
        assert_eq!(reading.page + 1, reading.probe.page_count());
        assert_eq!(app.input(hold(layout.number)), Change::Partial);
        assert_eq!(app.current_history_location().as_ref(), Some(&origin));
        assert!(app.back.is_empty());
        assert_eq!(app.input(Input::Tap(8.0, 440.0)), Change::Partial);
        assert_eq!(app.screen, Screen::Book);
        assert_eq!(app.current_history_location().as_ref(), Some(&origin));
    }

    #[test]
    fn skim_marks_preview_a_pdf_highlight_and_page_bookmark() {
        let fixture = Fixture::new();
        let pdf = fixture.root.join("Tilted Scan.pdf");
        std::fs::copy(crate::test_fixture("tilted-scan.pdf"), &pdf).unwrap();
        let mut app = fixture.app();
        app.open_path(&pdf).unwrap();
        app.add_bookmark().unwrap();
        let origin = app.current_history_location().unwrap();
        let book = app.open_book_id().unwrap();
        app.library
            .state_mut(book)
            .unwrap()
            .pdf_highlights
            .push(crate::library::PdfHighlight {
                updated_at: None,
                chapter_title: None,
                created_at: None,
                style: Default::default(),
                color: Default::default(),
                page: 1,
                bounds: Vec::new(),
                text: "Scan note".into(),
                note: Some("A reference for this page".into()),
                continuation: Vec::new(),
            });
        app.activate(Action::Skim).unwrap();
        let layout = app.skim_layout();
        assert_eq!(app.input(tap(layout.marks[2])), Change::Partial);
        assert_eq!(app.current_position().unwrap().page, 1);
        assert!(!app.page_is_bookmarked());
        assert_eq!(app.input(tap(layout.marks[1])), Change::Partial);
        assert!(app.page_is_bookmarked());
        assert_eq!(app.screen, Screen::Skim);
        assert_eq!(app.input(tap(layout.marks[1])), Change::Partial);
        assert!(!app.page_is_bookmarked());
        assert_eq!(app.input(hold(layout.marks[0])), Change::Partial);
        assert_eq!(app.current_history_location().as_ref(), Some(&origin));
        assert_eq!(app.input(hold(layout.marks[2])), Change::Partial);
        assert_eq!(app.current_position().unwrap().page, 1);
        assert!(app.back.is_empty());
        assert_eq!(app.input(hold(layout.marks[1])), Change::Partial);
        assert_eq!(app.screen, Screen::FixedBookmarks);
        assert_eq!(app.back, [origin]);
    }

    #[test]
    fn skim_go_to_opens_the_existing_page_editor_and_saves_one_preview_origin() {
        let fixture = Fixture::new();
        let mut app = fixture.app();
        app.open_path(&fixture.book).unwrap();
        let origin = app.current_history_location().unwrap();
        app.activate(Action::Skim).unwrap();
        let layout = app.skim_layout();
        assert_eq!(app.input(tap(layout.chapters[1])), Change::Partial);
        assert!(app.back.is_empty());
        assert_eq!(app.input(tap(layout.number)), Change::Partial);
        assert_eq!(app.screen, Screen::GoToPage);
        assert_eq!(app.back, [origin]);
        app.editor.text = "1:1".chars().collect();
        app.jump_reflow_number().unwrap();
        assert_eq!(app.current_position().unwrap().chapter, 0);
    }

    #[test]
    fn skim_shortcut_press_changes_only_the_selected_button_in_each_theme() {
        let fixture = Fixture::new();
        let fonts = vec![
            crate::test_fixture("fonts/NotoSans-Regular.ttf"),
            crate::test_fixture("fonts/NotoSans-Bold.ttf"),
        ];
        let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
        app.open_path(&fixture.book).unwrap();
        app.activate(Action::Skim).unwrap();
        let layout = app.skim_layout();
        let paths: Vec<_> = fonts
            .iter()
            .map(|font| font.display().to_string())
            .collect();
        let mut canvas = Canvas::new(WIDTH, HEIGHT, &paths).unwrap();
        for dark in [false, true] {
            let normal = primitive_app::frame(&app, &mut canvas, dark);
            for button in layout
                .chapters
                .into_iter()
                .chain(layout.marks)
                .chain([layout.number])
            {
                let Input::Tap(x, y) = tap(button) else {
                    unreachable!();
                };
                assert!(canvas.press_at(x, y));
                let pressed = primitive_app::frame(&app, &mut canvas, dark);
                assert_ne!(normal, pressed);
                for (index, (a, b)) in normal.iter().zip(&pressed).enumerate() {
                    if a != b {
                        assert!(button.contains(
                            (index as u32 % WIDTH) as f32,
                            (index as u32 / WIDTH) as f32,
                        ));
                    }
                }
                canvas.clear_press();
            }
        }
    }

    #[test]
    fn skim_ten_page_buttons_preview_in_both_directions_without_recording_jumps() {
        let fixture = Fixture::new();
        let epub = fixture.root.join("Long chapter.epub");
        std::fs::copy(crate::test_fixture("long-chapter.epub"), &epub).unwrap();
        let mut app = fixture.app();
        app.open_path(&epub).unwrap();
        let initial = app.current_history_location().unwrap();
        assert!(app.active.as_ref().unwrap().reading.probe.page_count() > 10);

        app.activate(Action::Skim).unwrap();
        let layout = app.skim_layout();
        assert_eq!(app.input(tap(layout.next_ten)), Change::Partial);
        let settle = |app: &mut Reader| {
            for _ in 0..12 {
                if !app.active.as_ref().unwrap().reading.is_turning() {
                    return;
                }
                let deadline = app.deadline().unwrap();
                app.tick(deadline + std::time::Duration::from_secs(1));
            }
            assert!(!app.active.as_ref().unwrap().reading.is_turning());
        };
        settle(&mut app);
        assert_eq!(app.active.as_ref().unwrap().reading.page, 10);
        assert!(app.back.is_empty());
        assert_eq!(app.input(tap(layout.previous_ten)), Change::Partial);
        settle(&mut app);
        assert_eq!(app.current_history_location().as_ref(), Some(&initial));
        assert_eq!(app.input(tap(layout.cancel)), Change::Partial);
        assert!(app.back.is_empty());

        let pdf = fixture.root.join("Tilted scan.pdf");
        std::fs::copy(crate::test_fixture("tilted-scan.pdf"), &pdf).unwrap();
        app.open_path(&pdf).unwrap();
        app.activate(Action::Skim).unwrap();
        assert_eq!(app.input(tap(layout.next_ten)), Change::Partial);
        assert_eq!(app.current_history_location().unwrap().position.page, 1);
        assert_eq!(app.input(tap(layout.previous_ten)), Change::Partial);
        assert_eq!(app.current_history_location().unwrap().position.page, 0);
        assert!(app.back.is_empty());
    }

    #[test]
    fn skim_placement_persists_and_keeps_progress_and_actions_in_the_drawn_panel() {
        let fixture = Fixture::new();
        let pdf = fixture.root.join("Tilted Scan.pdf");
        std::fs::copy(crate::test_fixture("tilted-scan.pdf"), &pdf).unwrap();
        let fonts = vec![
            crate::test_fixture("fonts/NotoSans-Regular.ttf"),
            crate::test_fixture("fonts/NotoSans-Bold.ttf"),
        ];
        let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
        assert_eq!(app.controls.skim_position, SkimPosition::Center);
        app.open_path(&pdf).unwrap();
        let opening = app.current_history_location().unwrap();

        for (index, position, capture) in [
            (2, SkimPosition::Top, "READER_SKIM_TOP"),
            (3, SkimPosition::Center, "READER_SKIM_CENTER"),
            (4, SkimPosition::Bottom, "READER_SKIM_BOTTOM"),
        ] {
            app.show(Screen::NavigationSettings);
            assert_eq!(
                app.input(tap(app.list_geometry().row(index))),
                Change::Partial
            );
            assert_eq!(app.controls.skim_position, position);
            app.activate(Action::Skim).unwrap();
            let layout = app.skim_layout();
            match position {
                SkimPosition::Top => assert_eq!(layout.panel.y, 32),
                SkimPosition::Center => {
                    assert_eq!(2 * layout.panel.y + layout.panel.h, HEIGHT as i32)
                }
                SkimPosition::Bottom => assert_eq!(layout.panel.bottom(), HEIGHT as i32 - 76),
            }
            assert!(layout.track.bottom() < layout.origin.y);
            capture_test_screen(&app, &fonts, capture, false);
            let track_y = (layout.track.y + layout.track.h / 2) as f32;
            let from_x = (layout.track.x + 40) as f32;
            let swipe = Input::Swipe {
                dx: layout.track.w as f32,
                dy: 0.0,
                start_y: track_y,
            };
            assert_eq!(app.swipe(swipe, from_x), Change::Partial);
            assert_eq!(app.current_history_location().unwrap().position.page, 1);
            assert_eq!(app.input(tap(layout.origin)), Change::Partial);
            assert_eq!(app.current_history_location().as_ref(), Some(&opening));
            assert_eq!(app.input(tap(layout.cancel)), Change::Partial);
            assert_eq!(app.screen, Screen::Fixed);
            assert!(app.back.is_empty());
        }

        let app = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
        assert_eq!(app.controls.skim_position, SkimPosition::Bottom);
    }
}

impl Layout {
    fn new(position: SkimPosition, viewport: ReadingViewport, mirrored: bool) -> Self {
        let (width, screen_height) = viewport.dimensions();
        let width = width as i32;
        let screen_height = screen_height as i32;
        let (height, track, shortcuts, preview, actions) = match position {
            SkimPosition::Center => (478, 196, 84, 258, 378),
            SkimPosition::Top | SkimPosition::Bottom => (446, 73, 125, 233, 347),
        };
        let top = match position {
            SkimPosition::Top => 32,
            SkimPosition::Center => (screen_height - height) / 2,
            SkimPosition::Bottom => screen_height - height - 76,
        };
        let button_stride = (width - 144) / 5;
        let action_stride = (width - 160) / 3;
        let place = |rect: Rect| {
            if mirrored {
                Rect::new(width - rect.right(), rect.y, rect.w, rect.h)
            } else {
                rect
            }
        };
        let button = |row, index| {
            place(Rect::new(
                80 + button_stride * index,
                top + row,
                button_stride - 16,
                100,
            ))
        };
        let action = |index| {
            place(Rect::new(
                80 + action_stride * index,
                top + actions,
                action_stride - 16,
                82,
            ))
        };
        Self {
            mirrored,
            panel: Rect::new(40, top, width - 80, height),
            track: Rect::new(92, top + track, width - 184, 42),
            previous_ten: button(preview, 0),
            previous: button(preview, 1),
            number: button(preview, 2),
            next: button(preview, 3),
            next_ten: button(preview, 4),
            chapters: [button(shortcuts, 0), button(shortcuts, 4)],
            marks: [
                button(shortcuts, 1),
                button(shortcuts, 2),
                button(shortcuts, 3),
            ],
            cancel: action(0),
            origin: action(1),
            commit: action(2),
        }
    }

    fn fraction(&self, x: f32) -> f64 {
        let fraction = f64::from(((x - self.track.x as f32) / self.track.w as f32).clamp(0.0, 1.0));
        if self.mirrored {
            1.0 - fraction
        } else {
            fraction
        }
    }
}

pub(super) struct Session {
    origin: HistoryLocation,
    origin_fraction: f64,
    commit_when_idle: bool,
}

impl Reader {
    pub(super) fn skim_layout(&self) -> Layout {
        Layout::new(
            self.controls.skim_position,
            self.reading_viewport(),
            self.document_dialogs_mirrored(),
        )
    }

    pub(super) fn start_skim(&mut self) -> crate::Result<()> {
        if self.active.is_none() && self.fixed.is_none() {
            return Err("Open a document before skimming".into());
        }
        let origin = self
            .current_history_location()
            .ok_or("Open a document before skimming")?;
        self.skim = Some(Session {
            origin,
            origin_fraction: self.skim_progress(),
            commit_when_idle: false,
        });
        self.selection = None;
        self.pdf_selection = None;
        self.search_result = None;
        self.result_selection = None;
        self.pdf_result_regions.clear();
        self.show(Screen::Skim);
        Ok(())
    }

    fn skim_origin(&self) -> Option<HistoryLocation> {
        self.skim.as_ref().map(|skim| skim.origin.clone())
    }

    fn skim_restore_origin(&mut self) -> crate::Result<()> {
        let origin = self.skim_origin().ok_or("Skim is not open")?;
        self.skim_apply_location(&origin)?;
        if let Some(skim) = &mut self.skim {
            skim.commit_when_idle = false;
        }
        Ok(())
    }

    fn skim_apply_location(&mut self, target: &HistoryLocation) -> crate::Result<()> {
        if let Some(fixed) = &mut self.fixed {
            let scan_target = target.scan_point.map(FixedJump::LinkPoint);
            fixed.reading.jump_to(
                target
                    .position
                    .page
                    .min(fixed.reading.count().saturating_sub(1)),
                scan_target.as_ref(),
            )?;
        } else {
            let reading = &mut self.active.as_mut().ok_or("Open a book first")?.reading;
            if let Some(anchor) = &target.position.anchor {
                reading.jump_to_anchor(target.position.chapter, anchor)?;
            } else {
                reading.go_to_page(target.position.chapter, target.position.page)?;
            }
        }
        self.selection = None;
        self.pdf_selection = None;
        self.result_selection = None;
        self.search_result = None;
        self.pdf_result_regions.clear();
        self.notice = None;
        Ok(())
    }

    fn skim_close(&mut self, commit: bool) -> crate::Result<()> {
        if commit && self.skim_turn_pending() {
            if let Some(skim) = &mut self.skim {
                skim.commit_when_idle = true;
            }
            return Ok(());
        }
        let session = self.skim.take().ok_or("Skim is not open")?;
        if commit {
            if self.current_history_location().as_ref() != Some(&session.origin) {
                self.push_history(session.origin);
            }
        } else {
            self.skim_apply_location(&session.origin)?;
        }
        self.show(if self.fixed.is_some() {
            Screen::Fixed
        } else {
            Screen::Book
        });
        self.record_position();
        Ok(())
    }

    fn skim_turn_pending(&self) -> bool {
        self.active
            .as_ref()
            .is_some_and(|book| book.reading.is_turning())
            || self
                .fixed
                .as_ref()
                .is_some_and(|book| book.reading.is_turning())
    }

    fn skim_progress(&self) -> f64 {
        if let Some(fixed) = &self.fixed {
            let denominator = fixed.reading.count().saturating_sub(1).max(1);
            return fixed.reading.page() as f64 / denominator as f64;
        }
        let Some(reading) = self.active.as_ref().map(|book| &book.reading) else {
            return 0.0;
        };
        let weights = reading.progress_weights();
        let total = weights.last().copied().unwrap_or(0);
        if total == 0 || reading.chapter + 1 >= weights.len() {
            let chapters = reading.probe.chapter_count().saturating_sub(1).max(1);
            return reading.chapter as f64 / chapters as f64;
        }
        let chapter_start = weights[reading.chapter];
        let chapter_span = weights[reading.chapter + 1].saturating_sub(chapter_start);
        let (page, count) = reading.visible_progress_pages(reading.page);
        let pages = count.saturating_sub(1).max(1);
        let within = page as f64 / pages as f64;
        ((chapter_start as f64 + chapter_span as f64 * within) / total as f64).clamp(0.0, 1.0)
    }

    fn skim_label(&self) -> String {
        if let Some(fixed) = &self.fixed {
            format!(
                "{} of {}",
                fixed_page_label(&fixed.reading, fixed.reading.page()),
                fixed.reading.count(),
            )
        } else if let Some(active) = &self.active {
            let title = self
                .active_alternative_chapter_title()
                .unwrap_or(&active.reading.probe.chapter_label);
            let (page, count) = self.alternative_chapter_progress().map_or_else(
                || (active.reading.page + 1, active.reading.probe.page_count()),
                |chapter| (chapter.page, chapter.pages),
            );
            if title.is_empty() {
                format!("Page {page} of {count}")
            } else {
                format!("{title} · Page {page} of {count}")
            }
        } else {
            "No open document".into()
        }
    }

    pub(super) fn draw_skim(&self, canvas: &mut Canvas) {
        let progress = self.skim_progress();
        let layout = self.skim_layout();
        let panel = layout.panel;
        let track = layout.track;
        canvas.popup(panel);
        let title = format!("Skim  ·  {}", self.skim_label());
        canvas.text(
            Rect::new(panel.x + 28, panel.y + 20, panel.w - 56, 46),
            self.notice.as_deref().unwrap_or(&title),
            TextStyle::new(34).bold(),
        );
        canvas.panel(track, 8, 2, 224);
        let track_width = track.w.saturating_sub(18);
        let filled = (progress * track_width as f64).round() as i32;
        let at = |offset| {
            if layout.mirrored {
                track_width - offset
            } else {
                offset
            }
        };
        if filled > 0 {
            let x = if layout.mirrored { at(filled) } else { 0 };
            canvas.fill(Rect::new(track.x + 9 + x, track.y + 17, filled, 8), 0);
        }
        let mut previous = 0;
        let mut draw_marker = |fraction: f64| {
            let x = (fraction * track_width as f64).round() as i32;
            if x > previous + 2 && x < track_width - 2 {
                canvas.fill(
                    Rect::new(
                        track.x + 8 + at(x) + i32::from(!layout.mirrored),
                        track.y + 11,
                        2,
                        20,
                    ),
                    0,
                );
                previous = x;
            }
        };
        if let Some(markers) =
            self.alternative_book_marker_fractions(super::progress_markers::MarkerTrack::Skim)
        {
            markers.into_iter().for_each(draw_marker);
        } else if let Some(markers) =
            self.publisher_book_marker_fractions(super::progress_markers::MarkerTrack::Skim)
        {
            markers.into_iter().for_each(draw_marker);
        } else if let Some(markers) = self.whole_book_marker_pages(false) {
            let last_page = markers.count.saturating_sub(1).max(1);
            for page in markers.starts {
                draw_marker(page as f64 / last_page as f64);
            }
        }
        if let Some(skim) = &self.skim {
            let origin = (skim.origin_fraction * f64::from(track.w - 18)).round() as i32;
            canvas.fill(Rect::new(track.x + at(origin) + 7, track.y + 5, 4, 32), 0);
        }
        let marker = track.x + 9 + at(filled) - 8;
        canvas.panel(Rect::new(marker, track.y + 7, 18, track.h - 14), 8, 1, 0);
        skim_icon_button(
            canvas,
            layout.chapters[0],
            if layout.mirrored {
                LAST_PAGE
            } else {
                FIRST_PAGE
            },
            "Chapter back",
            false,
        );
        skim_icon_button(
            canvas,
            layout.marks[0],
            if layout.mirrored {
                MARK_FORWARD
            } else {
                MARK_BACK
            },
            "Mark back",
            false,
        );
        let marked = self.page_is_bookmarked();
        skim_icon_button(
            canvas,
            layout.marks[1],
            BOOKMARK,
            if marked { "Saved" } else { "Save page" },
            marked,
        );
        skim_icon_button(
            canvas,
            layout.marks[2],
            if layout.mirrored {
                MARK_BACK
            } else {
                MARK_FORWARD
            },
            "Mark next",
            false,
        );
        skim_icon_button(
            canvas,
            layout.chapters[1],
            if layout.mirrored {
                FIRST_PAGE
            } else {
                LAST_PAGE
            },
            "Chapter next",
            false,
        );
        canvas.button(layout.previous_ten, "-10", false, false);
        canvas.button(
            layout.previous,
            if layout.mirrored {
                "Previous  ›"
            } else {
                "‹  Previous"
            },
            false,
            false,
        );
        canvas.button(layout.number, "Go to", false, false);
        canvas.button(
            layout.next,
            if layout.mirrored {
                "‹  Next"
            } else {
                "Next  ›"
            },
            false,
            false,
        );
        canvas.button(layout.next_ten, "+10", false, false);
        canvas.button(layout.cancel, "Cancel", false, false);
        canvas.button(layout.origin, "Origin", false, false);
        let waiting = self.skim.as_ref().is_some_and(|skim| skim.commit_when_idle);
        canvas.button(
            layout.commit,
            if waiting { "Waiting…" } else { "Read here" },
            true,
            false,
        );
    }

    fn skim_chapter(&mut self, direction: Direction, boundary: bool) -> Change {
        if boundary {
            return self.skim_preview_progress(if direction == Direction::Previous {
                0.0
            } else {
                1.0
            });
        }
        let before = self.current_position();
        match self.jump_chapter(direction) {
            Ok(()) => self.skim_finish_shortcut(before),
            Err(error) => self.report(error),
        }
    }

    fn skim_mark(&mut self, direction: Direction, boundary: bool) -> Change {
        let before = self.current_position();
        let Some(target) = self.skim_mark_destination(direction, boundary) else {
            return self.report(match direction {
                Direction::Previous => "No earlier saved mark",
                Direction::Next => "No later saved mark",
            });
        };
        match self.skim_apply_location(&target.into()) {
            Ok(()) => self.skim_finish_shortcut(before),
            Err(error) => self.report(error),
        }
    }

    fn skim_mark_destination(
        &self,
        direction: Direction,
        boundary: bool,
    ) -> Option<ReadingPosition> {
        let current = self.current_position()?;
        let state = self.library.state(self.open_book_id()?)?;
        let reflow = self.active.as_ref();
        let saved_positions = state
            .bookmarks
            .iter()
            .map(|bookmark| bookmark.position.clone());
        let reflow_marks = state.highlights.iter().filter_map(|mark| {
            let anchor = &mark.selection.start;
            Some(ReadingPosition {
                chapter: reflow?.reading.probe.chapter_index(&anchor.chapter)?,
                page: 0,
                anchor: Some(anchor.clone()),
            })
        });
        let fixed_marks = state
            .pdf_highlights
            .iter()
            .filter(|mark| {
                self.fixed
                    .as_ref()
                    .is_some_and(|book| mark.page < book.reading.count())
            })
            .map(|mark| ReadingPosition {
                chapter: 0,
                page: mark.page,
                anchor: None,
            });
        let mut layouts = HashMap::new();
        let candidates = saved_positions
            .chain(reflow_marks)
            .chain(fixed_marks)
            .filter_map(|mut position| {
                let key = if let Some(book) = reflow {
                    let chapter = position
                        .anchor
                        .as_ref()
                        .and_then(|anchor| book.reading.probe.chapter_index(&anchor.chapter))
                        .unwrap_or(position.chapter);
                    if chapter >= book.reading.probe.chapter_count()
                        || (direction == Direction::Previous && chapter > current.chapter)
                        || (direction == Direction::Next && chapter < current.chapter)
                    {
                        return None;
                    }
                    reflow_mark_location(
                        book,
                        chapter,
                        position.page,
                        position.anchor.as_ref(),
                        &mut layouts,
                    )
                } else {
                    let fixed = self.fixed.as_ref()?;
                    if position.page >= fixed.reading.count() {
                        return None;
                    }
                    (0, position.page, 0, 0)
                };
                let page = (key.0, key.1);
                let current_page = (current.chapter, current.page);
                let is_beyond = match direction {
                    Direction::Previous => page < current_page,
                    Direction::Next => page > current_page,
                };
                if !is_beyond {
                    return None;
                }
                position.chapter = key.0;
                position.page = key.1;
                Some((key, position))
            });
        match (direction, boundary) {
            (Direction::Previous, false) | (Direction::Next, true) => {
                candidates.max_by_key(|(key, _)| *key)
            }
            (Direction::Next, false) | (Direction::Previous, true) => {
                candidates.min_by_key(|(key, _)| *key)
            }
        }
        .map(|(_, position)| position)
    }

    fn skim_finish_shortcut(&mut self, before: Option<ReadingPosition>) -> Change {
        if self.current_position() == before {
            return Change::Partial;
        }
        if let Some(skim) = &mut self.skim {
            skim.commit_when_idle = false;
        }
        self.finish_skim_movement(before)
    }

    fn skim_open_screen(&mut self, screen: Screen) -> Change {
        if self.skim_turn_pending() {
            return self.report("Wait for the page to finish turning");
        }
        match self
            .skim_close(true)
            .and_then(|()| self.activate(Action::Show(screen)))
        {
            Ok(()) => Change::Partial,
            Err(error) => self.report(error),
        }
    }

    fn skim_preview_progress(&mut self, fraction: f64) -> Change {
        if let Some(skim) = &mut self.skim {
            skim.commit_when_idle = false;
        }
        let before = self.current_position();
        let result = if let Some(active) = &mut self.active {
            active.reading.jump_progress(fraction.clamp(0.0, 1.0))
        } else if let Some(fixed) = &mut self.fixed {
            let page = (fraction.clamp(0.0, 1.0) * fixed.reading.count().saturating_sub(1) as f64)
                .round() as usize;
            fixed.reading.jump(page).map(|()| ())
        } else {
            return Change::None;
        };
        match result {
            Ok(()) => self.finish_skim_movement(before),
            Err(error) => self.report(error),
        }
    }

    fn skim_step(&mut self, input: Input) -> Change {
        if let Some(skim) = &mut self.skim {
            skim.commit_when_idle = false;
        }
        let before = self.current_position();
        if let Some(active) = &mut self.active {
            let change = active.reading.input(input);
            active.reading.take_end_attempt();
            if change == Change::None {
                return if active.reading.is_turning() {
                    Change::Partial
                } else {
                    Change::None
                };
            }
        } else if let Some(fixed) = &mut self.fixed {
            let changed = match fixed.reading.input_with_status(input, false) {
                Ok(changed) => changed,
                Err(error) => return self.report(error),
            };
            fixed.reading.take_end_attempt();
            if !changed {
                return if fixed.reading.is_turning() {
                    Change::Partial
                } else {
                    Change::None
                };
            }
        } else {
            return Change::None;
        }
        self.finish_skim_movement(before)
    }

    fn skim_step_ten(&mut self, direction: isize) -> Change {
        if let Some(skim) = &mut self.skim {
            skim.commit_when_idle = false;
        }
        let before = self.current_position();
        if let Some(active) = &mut self.active {
            let change = active.reading.automatic_turn(100 * direction as i16);
            active.reading.take_end_attempt();
            if change == Change::None && !active.reading.is_turning() {
                return Change::None;
            }
        } else if let Some(fixed) = &mut self.fixed {
            let start = fixed.reading.page();
            let mut target = start;
            for _ in 0..10 {
                let Some(page) = fixed.reading.next_reading_page(target, direction) else {
                    break;
                };
                target = page;
            }
            if target == start {
                return Change::None;
            }
            if let Err(error) = fixed.reading.jump(target) {
                return self.report(error);
            }
        } else {
            return Change::None;
        }
        self.finish_skim_movement(before)
    }

    fn finish_skim_movement(&mut self, before: Option<ReadingPosition>) -> Change {
        self.notice = None;
        self.selection = None;
        self.pdf_selection = None;
        self.result_selection = None;
        self.search_result = None;
        self.pdf_result_regions.clear();
        self.reading_update(self.current_position() != before)
    }

    pub(super) fn skim_swipe(&mut self, input: Input, start_x: f32) -> Change {
        let Input::Swipe { dx, dy, start_y } = input else {
            return Change::None;
        };
        let layout = self.skim_layout();
        if layout.track.contains(start_x, start_y) {
            return self.skim_preview_progress(layout.fraction(start_x + dx));
        }
        if dx.abs() > dy.abs() {
            self.skim_step(if (dx < 0.0) ^ layout.mirrored {
                Input::PageForward
            } else {
                Input::PageBack
            })
        } else {
            Change::None
        }
    }

    pub(super) fn skim_input(&mut self, input: Input) -> Change {
        let layout = self.skim_layout();
        match input {
            Input::Tap(x, y) if self.reading_viewport().done().contains(x, y) => self.exit(),
            Input::PageForward => self.skim_step(Input::PageForward),
            Input::PageBack => self.skim_step(Input::PageBack),
            Input::Tap(x, y) | Input::LongTap(x, y) if layout.chapters[0].contains(x, y) => {
                self.skim_chapter(Direction::Previous, matches!(input, Input::LongTap(..)))
            }
            Input::Tap(x, y) | Input::LongTap(x, y) if layout.chapters[1].contains(x, y) => {
                self.skim_chapter(Direction::Next, matches!(input, Input::LongTap(..)))
            }
            Input::Tap(x, y) | Input::LongTap(x, y) if layout.marks[0].contains(x, y) => {
                self.skim_mark(Direction::Previous, matches!(input, Input::LongTap(..)))
            }
            Input::Tap(x, y) | Input::LongTap(x, y) if layout.marks[2].contains(x, y) => {
                self.skim_mark(Direction::Next, matches!(input, Input::LongTap(..)))
            }
            Input::Tap(x, y) if layout.marks[1].contains(x, y) => match self.toggle_bookmark() {
                Ok(()) => Change::Partial,
                Err(error) => self.report(error),
            },
            Input::LongTap(x, y) if layout.marks[1].contains(x, y) => {
                self.skim_open_screen(if self.fixed.is_some() {
                    Screen::FixedBookmarks
                } else {
                    Screen::Bookmarks
                })
            }
            Input::Tap(x, y) if layout.number.contains(x, y) => {
                self.skim_open_screen(Screen::GoToPage)
            }
            Input::LongTap(x, y) if layout.number.contains(x, y) => {
                match self.skim_restore_origin() {
                    Ok(()) => Change::Partial,
                    Err(error) => self.report(error),
                }
            }
            Input::Tap(x, y) if layout.track.contains(x, y) => {
                self.skim_preview_progress(layout.fraction(x))
            }
            Input::Tap(x, y) if layout.previous_ten.contains(x, y) => self.skim_step_ten(-1),
            Input::Tap(x, y) if layout.previous.contains(x, y) => self.skim_step(Input::PageBack),
            Input::Tap(x, y) if layout.next.contains(x, y) => self.skim_step(Input::PageForward),
            Input::Tap(x, y) if layout.next_ten.contains(x, y) => self.skim_step_ten(1),
            Input::Tap(x, y) if layout.origin.contains(x, y) => match self.skim_restore_origin() {
                Ok(()) => Change::Partial,
                Err(error) => self.report(error),
            },
            Input::Tap(x, y) if layout.cancel.contains(x, y) => match self.skim_close(false) {
                Ok(()) => Change::Partial,
                Err(error) => self.report(error),
            },
            Input::Tap(x, y) if layout.commit.contains(x, y) => match self.skim_close(true) {
                Ok(()) => Change::Partial,
                Err(error) => self.report(error),
            },
            Input::Tap(x, y) if !layout.panel.contains(x, y) => match self.skim_close(false) {
                Ok(()) => Change::Partial,
                Err(error) => self.report(error),
            },
            _ => Change::None,
        }
    }

    pub(super) fn skim_tick(&mut self, now: std::time::Instant) -> Change {
        let change = if let Some(active) = &mut self.active {
            let change = active.reading.tick(now);
            active.reading.take_end_attempt();
            change
        } else if let Some(fixed) = &mut self.fixed {
            match fixed.reading.tick(now) {
                Ok(changed) => {
                    fixed.reading.take_end_attempt();
                    if changed {
                        Change::Partial
                    } else {
                        Change::None
                    }
                }
                Err(error) => return self.report(error),
            }
        } else {
            return Change::None;
        };
        if change == Change::Partial {
            self.notice = None;
            self.selection = None;
            self.pdf_selection = None;
            self.result_selection = None;
            self.search_result = None;
            self.pdf_result_regions.clear();
            let commit = self.skim.as_ref().is_some_and(|skim| skim.commit_when_idle);
            if commit && !self.skim_turn_pending() {
                return match self.skim_close(true) {
                    Ok(()) => Change::Partial,
                    Err(error) => self.report(error),
                };
            }
            self.reading_update(true)
        } else {
            Change::None
        }
    }
}

const MARK_BACK: Icon = Icon {
    stroke: &["M13 5 H19 V19 L16 16 L13 19 Z", "M8 8 L4 12 L8 16"],
    fill: &[],
};

const MARK_FORWARD: Icon = Icon {
    stroke: &["M5 5 H11 V19 L8 16 L5 19 Z", "M16 8 L20 12 L16 16"],
    fill: &[],
};

fn skim_icon_button(canvas: &mut Canvas, rect: Rect, icon: Icon, label: &str, active: bool) {
    let inverted = canvas.control_pressed(rect, false) != active;
    let (background, ink) = if inverted { (0, 255) } else { (255, 0) };
    canvas.panel(rect, 16, 2, background);
    canvas.icon(
        Rect::new(rect.x + (rect.w - 38) / 2, rect.y + 10, 38, 38),
        icon,
        ink,
    );
    canvas.text(
        Rect::new(rect.x + 4, rect.y + 56, rect.w - 8, 36),
        label,
        TextStyle::new(25).bold().center().ink(ink),
    );
}
