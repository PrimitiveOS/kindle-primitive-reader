//! Back for reading gestures, independent of the physical page buttons and jump history.
use std::collections::VecDeque;

use primitive_app::Change;
use primitive_ui::{Canvas, Input, Rect, Sheet, SheetAction};
use serde::{Deserialize, Serialize};

use super::{Action, Item, Reader, Screen};
use crate::{
    Result,
    library::{FixedPreferences, ReadingPosition},
    session::ReadingViewport,
};

const HISTORY_LIMIT: usize = 5_000;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct BackPreferences {
    exit: Exit,
    library: Library,
    reader: Reading,
}

macro_rules! choices {
    ($policy:ident, $default:ident = $default_label:literal, $($choice:ident = $label:literal),+ $(,)?) => {
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        enum $policy {
            #[default]
            $default,
            $($choice),+
        }

        impl $policy {
            const ALL: &'static [Self] = &[Self::$default, $(Self::$choice),+];

            fn label(self) -> &'static str {
                match self {
                    Self::$default => $default_label,
                    $(Self::$choice => $label),+
                }
            }
        }
    };
}

choices!(
    Exit,
    Prompt = "Ask before exiting",
    Always = "Exit immediately",
    Disabled = "Don't exit"
);
choices!(
    Library,
    Exit = "Follow Back to exit",
    Parent = "Go to parent folder"
);
choices!(
    Reading,
    PreviousLocation = "Go to previous location",
    PreviousPage = "Go to previous read page",
    Library = "Go to file browser",
    Exit = "Follow Back to exit",
);

impl BackPreferences {
    pub(crate) fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Options {
    Exit,
    Library,
    Reading,
}

impl Options {
    pub(super) fn title(self) -> &'static str {
        match self {
            Self::Exit => "Back to exit",
            Self::Library => "Back in file browser",
            Self::Reading => "Back in Reader",
        }
    }
}

#[derive(Clone, PartialEq)]
enum ReadView {
    Reflow {
        position: ReadingPosition,
        top: Option<f32>,
        viewport: ReadingViewport,
    },
    Fixed {
        page: usize,
        view: FixedPreferences,
        viewport: ReadingViewport,
        source_point: Option<hayro::kurbo::Point>,
    },
}

pub(super) fn with_fixed_geometry(
    mut current: FixedPreferences,
    saved: FixedPreferences,
) -> FixedPreferences {
    current.zoom = saved.zoom;
    current.precise_scale = saved.precise_scale;
    current.tile_zoom = saved.tile_zoom;
    current.turns = saved.turns;
    current.width_fit = saved.width_fit;
    current.height_fit = saved.height_fit;
    current.continuous = saved.continuous;
    current.spread = saved.spread;
    current.epub_spread = saved.epub_spread;
    current.continuous_gap = saved.continuous_gap;
    current.crop = saved.crop;
    current.crop_margin_hundredths = saved.crop_margin_hundredths;
    current.semi_auto_crop = saved.semi_auto_crop;
    current.continuous_crop = saved.continuous_crop;
    current.straighten_limit = saved.straighten_limit;
    current.scan_contrast = saved.scan_contrast;
    current.scan_gamma_tenths = saved.scan_gamma_tenths;
    current.scan_white_level = saved.scan_white_level;
    current.dewatermark = saved.dewatermark;
    current.offset_x = saved.offset_x;
    current.offset_y = saved.offset_y;
    current
}

impl ReadView {
    fn same_location(&self, other: &Self) -> bool {
        if let (
            Self::Reflow { position, top, .. },
            Self::Reflow {
                position: previous,
                top: previous_top,
                ..
            },
        ) = (self, other)
        {
            // Changing between page and scroll modes can leave the same source location.
            position == previous && top.zip(*previous_top).is_none_or(|(a, b)| a == b)
        } else {
            self == other
        }
    }
}

#[derive(Default)]
pub(super) struct State {
    previous: VecDeque<ReadView>,
    current: Option<ReadView>,
    empty_history_shown: bool,
    confirm_exit: bool,
}

impl Reader {
    pub(super) fn back_preference_entries(&self) -> Vec<Item> {
        let preferences = self.controls.back_navigation;
        let language = self.controls.interface_language;
        [
            (Options::Exit, preferences.exit.label()),
            (Options::Library, preferences.library.label()),
            (Options::Reading, preferences.reader.label()),
        ]
        .into_iter()
        .map(|(option, label)| {
            Item::new(
                language.navigation_settings_text(option.title()),
                Action::Show(Screen::BackOptions(option)),
            )
            .detail(language.navigation_settings_text(label))
        })
        .collect()
    }

    pub(super) fn back_option_items(&self, options: Options) -> Vec<Item> {
        let current = self.controls.back_navigation;
        let language = self.controls.interface_language;
        let item = |label, back_navigation| {
            let item = Item::new(
                language.navigation_settings_text(label),
                Action::SetReadingControls(crate::reading_controls::ReadingControls {
                    back_navigation,
                    ..self.controls
                }),
            );
            if back_navigation == current {
                item.detail(language.navigation_settings_text("Selected"))
            } else {
                item
            }
        };
        match options {
            Options::Exit => Exit::ALL
                .iter()
                .map(|&exit| item(exit.label(), BackPreferences { exit, ..current }))
                .collect(),
            Options::Library => Library::ALL
                .iter()
                .map(|&library| item(library.label(), BackPreferences { library, ..current }))
                .collect(),
            Options::Reading => Reading::ALL
                .iter()
                .map(|&reader| item(reader.label(), BackPreferences { reader, ..current }))
                .collect(),
        }
    }

    pub(super) fn back_preferences_changed(&mut self, previous: BackPreferences) {
        let current = self.controls.back_navigation;
        if previous != current {
            self.reset_back_resistance();
        }
        if previous.reader != current.reader {
            self.back_navigation.previous.clear();
            self.back_navigation.current = None;
            self.record_read_view();
        }
    }

    pub(super) fn reset_back_resistance(&mut self) {
        self.back_navigation.empty_history_shown = false;
    }

    pub(super) fn record_read_view(&mut self) {
        if self.controls.back_navigation.reader != Reading::PreviousPage {
            return;
        }
        let viewport = self.reading_viewport();
        let current = if let Some(fixed) = &self.fixed {
            ReadView::Fixed {
                page: fixed.reading.page(),
                view: with_fixed_geometry(FixedPreferences::default(), fixed.reading.preferences()),
                viewport,
                source_point: fixed.reading.history_point(),
            }
        } else if let Some(active) = &self.active {
            ReadView::Reflow {
                position: super::position(&active.reading),
                top: active.reading.scroll_history_top(),
                viewport,
            }
        } else {
            return;
        };
        if self
            .back_navigation
            .current
            .as_ref()
            .is_some_and(|previous| previous.same_location(&current))
        {
            self.back_navigation.current = Some(current);
            return;
        }
        if let Some(previous) = self.back_navigation.current.replace(current) {
            self.back_navigation.previous.push_back(previous);
            if self.back_navigation.previous.len() > HISTORY_LIMIT {
                self.back_navigation.previous.pop_front();
            }
        }
        self.reset_back_resistance();
    }

    fn restore_read_view(&mut self, previous: &ReadView) -> Result<()> {
        let viewport = self.reading_viewport();
        match previous {
            ReadView::Reflow {
                position,
                top,
                viewport: saved,
            } => self.restore_reading_position(position, top.filter(|_| viewport == *saved)),
            ReadView::Fixed {
                page,
                view,
                viewport: saved,
                source_point,
            } => {
                self.close_fixed_search_fit()?;
                let fixed = &mut self.fixed.as_mut().ok_or("No open document")?.reading;
                if fixed.is_continuous() != view.continuous {
                    self.jump_fixed(*page, None).map(|_| ())
                } else if viewport == *saved {
                    fixed
                        .restore_view_at(*page, with_fixed_geometry(fixed.preferences(), *view))?;
                    self.finish_fixed_jump(*page, None);
                    Ok(())
                } else {
                    self.jump_fixed(*page, source_point.map(super::FixedJump::LinkPoint))
                        .map(|_| ())
                }
            }
        }
    }

    fn previous_jump_index(&self) -> Option<usize> {
        let current = self.current_position()?;
        self.back.iter().rposition(|saved| {
            if self.fixed.is_some() {
                return saved.position.page != current.page
                    || saved.fixed_view.is_some_and(|view| {
                        self.fixed.as_ref().is_some_and(|fixed| {
                            with_fixed_geometry(FixedPreferences::default(), view.preferences)
                                != with_fixed_geometry(
                                    FixedPreferences::default(),
                                    fixed.reading.preferences(),
                                )
                        })
                    });
            }
            match (saved.position.anchor.as_ref(), current.anchor.as_ref()) {
                (Some(saved), Some(current)) => saved != current,
                _ => {
                    saved.position.chapter != current.chapter || saved.position.page != current.page
                }
            }
        })
    }

    pub(super) fn navigation_back(&mut self) -> Change {
        let preferences = self.controls.back_navigation;
        let result = match self.screen {
            Screen::Library => {
                if preferences.library == Library::Parent {
                    return self.back_to_parent_folder();
                }
                return self.back_to_exit();
            }
            Screen::Book | Screen::Fixed => match preferences.reader {
                Reading::Exit => return self.back_to_exit(),
                Reading::Library => self.current_book_folder(),
                Reading::PreviousLocation => {
                    let Some(index) = self.previous_jump_index() else {
                        self.back.clear();
                        return self.back_at_empty_history();
                    };
                    // Skip markers for the current location without making Forward revisit them.
                    let skipped = self.back.split_off(index + 1);
                    let result = self.return_to_location(index);
                    if result.is_ok() {
                        self.reset_back_resistance();
                    } else {
                        self.back.extend(skipped);
                    }
                    result
                }
                Reading::PreviousPage => {
                    let Some(previous) = self.back_navigation.previous.back().cloned() else {
                        return self.back_at_empty_history();
                    };
                    // Restoring a view updates the baseline without pushing the page we left.
                    let current = self.back_navigation.current.take();
                    let result = self.restore_read_view(&previous);
                    if result.is_ok() {
                        self.back_navigation.previous.pop_back();
                        self.reset_back_resistance();
                    } else {
                        self.back_navigation.current = current;
                    }
                    result
                }
            },
            _ => return Change::None,
        };
        match result {
            Ok(()) => self.applied_change(),
            Err(error) => self.report(error),
        }
    }

    fn back_at_empty_history(&mut self) -> Change {
        let shown = std::mem::replace(&mut self.back_navigation.empty_history_shown, true);
        if shown && self.controls.back_navigation.exit != Exit::Disabled {
            self.reset_back_resistance();
            return self.back_to_exit();
        }
        self.report("Location history is empty.")
    }

    fn back_to_exit(&mut self) -> Change {
        match self.controls.back_navigation.exit {
            Exit::Prompt => {
                self.notice = None;
                self.back_navigation.confirm_exit = true;
                Change::Partial
            }
            Exit::Always => self.exit(),
            Exit::Disabled => Change::None,
        }
    }

    pub(super) fn back_exit_confirmation_open(&self) -> bool {
        self.back_navigation.confirm_exit
    }

    fn back_exit_sheet(&self) -> Sheet<'static> {
        let (width, height) = self.reading_viewport().dimensions();
        Sheet::centered(
            Rect::new(0, 0, width as i32, height as i32),
            "Exit Reader?",
            "Your reading position will be saved.",
            "Exit",
        )
    }

    pub(super) fn draw_back_exit_confirmation(&self, canvas: &mut Canvas) {
        if self.back_exit_confirmation_open() {
            self.back_exit_sheet().draw(canvas, None);
        }
    }

    pub(super) fn back_exit_confirmation_input(&mut self, input: Input) -> Option<Change> {
        if !self.back_exit_confirmation_open() || matches!(input, Input::Sleep | Input::Wake) {
            return None;
        }
        let action = match input {
            Input::Tap(x, y) => self.back_exit_sheet().hit(x, y),
            Input::PageBack => Some(SheetAction::Cancel),
            _ => None,
        };
        let Some(action) = action else {
            return Some(Change::None);
        };
        self.back_navigation.confirm_exit = false;
        self.notice = None;
        Some(match action {
            SheetAction::Confirm => self.exit(),
            SheetAction::Cancel => Change::Partial,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{
        gestures::{Binding, Gesture},
        tests::{Fixture, capture_test_screen},
    };
    use primitive_app::App;
    use std::{
        fs,
        path::Path,
        time::{Duration, Instant},
    };

    const BACK: Input = Input::Tap(24.0, 24.0);

    fn configure(app: &mut Reader, options: Options, choice: usize) {
        app.show(Screen::NavigationSettings);
        let entry = app
            .items()
            .iter()
            .position(|item| matches!(item.action, Action::Show(Screen::BackOptions(target)) if target == options))
            .unwrap();
        let row = app.list_geometry().row(entry);
        assert_eq!(
            app.input(Input::Tap((row.x + 20) as f32, (row.y + row.h / 2) as f32)),
            Change::Partial
        );
        assert_eq!(app.screen, Screen::BackOptions(options));
        let row = app.list_geometry().row(choice);
        assert_eq!(
            app.input(Input::Tap((row.x + 20) as f32, (row.y + row.h / 2) as f32)),
            Change::Partial
        );
        assert_eq!(app.items()[choice].detail.as_deref(), Some("Selected"));
    }

    fn bind_back(app: &mut Reader) {
        app.activate(Action::BindGesture(
            Gesture::TapTopLeft,
            Binding::NavigationBack,
        ))
        .unwrap();
    }

    fn page_button(app: &mut Reader, button: Input) {
        app.input(button);
        app.tick(Instant::now() + Duration::from_secs(1));
    }

    #[test]
    fn back_distinguishes_jumps_from_page_buttons_and_applies_exit_policy() {
        let fixture = Fixture::new();
        let fonts = [
            crate::test_fixture("fonts/NotoSans-Regular.ttf"),
            crate::test_fixture("fonts/NotoSans-Bold.ttf"),
        ];
        let mut app = Reader::open(&fixture.root, &fixture.state, fonts.to_vec()).unwrap();
        bind_back(&mut app);
        assert_eq!(app.controls.back_navigation, BackPreferences::default());
        let path = fixture.root.join("Long chapter.epub");
        fs::copy(crate::test_fixture("long-chapter.epub"), &path).unwrap();
        app.open_path(&path).unwrap();
        let first = app.current_position().unwrap();
        page_button(&mut app, Input::PageForward);
        let page = app.current_position().unwrap();
        assert_ne!(page, first);
        assert!(app.back.is_empty());
        assert_eq!(app.input(BACK), Change::Partial);
        assert_eq!(app.notice.as_deref(), Some("Location history is empty."));
        assert_eq!(app.current_position().unwrap(), page);
        page_button(&mut app, Input::PageBack);
        assert_eq!(app.current_position().unwrap(), first);
        assert_eq!(app.input(BACK), Change::Partial); // navigation resets the notice
        assert!(!app.back_exit_confirmation_open());
        assert_eq!(app.input(BACK), Change::Partial);
        assert!(app.back_exit_confirmation_open());
        capture_test_screen(&app, &fonts, "READER_BACK_CONFIRMATION", false);
        assert_eq!(app.input(Input::PageForward), Change::None);
        assert_eq!(app.input(Input::PageBack), Change::Partial); // cancel
        assert_eq!(app.current_position().unwrap(), first);
        assert!(!app.back_exit_confirmation_open());

        app.follow_reflow_page(page.chapter, page.page).unwrap();
        assert_eq!(app.input(BACK), Change::Partial);
        assert_eq!(app.current_position().unwrap(), first);
        assert!(!app.forward.is_empty());
        assert!(!app.back_exit_confirmation_open());

        configure(&mut app, Options::Exit, 2);
        app.show(Screen::Book);
        for _ in 0..3 {
            assert_eq!(app.input(BACK), Change::Partial);
            assert_eq!(app.notice.as_deref(), Some("Location history is empty."));
            assert!(!app.back_exit_confirmation_open());
        }
        let exit = app.reading_viewport().done();
        assert_eq!(
            app.input(Input::Tap(
                (exit.x + exit.w / 2) as f32,
                (exit.y + exit.h / 2) as f32
            )),
            Change::Exit
        );
    }

    #[test]
    fn previous_read_page_restores_exact_same_page_scroll_without_readding_a_visit() {
        let fixture = Fixture::new();
        let path = fixture.root.join("Long chapter.epub");
        fs::copy(crate::test_fixture("long-chapter.epub"), &path).unwrap();
        let mut app = fixture.app();
        bind_back(&mut app);
        configure(&mut app, Options::Reading, 1);
        app.open_path(&path).unwrap();
        app.activate(Action::ReadingMode(true)).unwrap();
        let top = |app: &Reader| {
            app.active
                .as_ref()
                .unwrap()
                .reading
                .scroll_history_top()
                .unwrap()
        };
        let start = top(&app);
        let swipe = Input::Swipe {
            dx: 0.0,
            dy: -137.0,
            start_y: 850.0,
        };
        for _ in 0..2 {
            app.input(swipe);
            app.tick(Instant::now() + Duration::from_secs(1));
        }
        let advanced = top(&app);
        assert!(advanced > start);
        assert_eq!(app.back_navigation.previous.len(), 2); // initial scroll, then first swipe
        let ReadView::Reflow {
            top: Some(previous),
            ..
        } = app.back_navigation.previous.back().unwrap()
        else {
            panic!("Expected saved scroll top")
        };
        let previous = *previous;
        assert!(start < previous && previous < advanced);
        assert!(app.back.is_empty());
        assert_eq!(app.input(BACK), Change::Partial);
        assert_eq!(top(&app), previous);
        assert_eq!(app.input(BACK), Change::Partial);
        assert_eq!(top(&app), start);
        assert!(app.back_navigation.previous.is_empty());
        app.close_book_to_library();
        assert!(app.back_navigation.previous.is_empty());
        assert!(app.back_navigation.current.is_none());
    }

    #[test]
    fn back_skips_the_opening_marker_and_reaches_the_previous_distinct_location() {
        let fixture = Fixture::new();
        let mut app = fixture.app();
        bind_back(&mut app);
        app.activate(Action::SetReadingControls(
            crate::reading_controls::ReadingControls {
                remember_opening_location: true,
                ..app.controls
            },
        ))
        .unwrap();
        app.open_path(&fixture.book).unwrap();
        let opening = app.current_history_location().unwrap();
        assert_eq!(app.back, vec![opening.clone()]);
        assert_eq!(app.input(BACK), Change::Partial);
        assert_eq!(app.notice.as_deref(), Some("Location history is empty."));
        assert!(app.back.is_empty() && app.forward.is_empty());
        assert_eq!(app.input(BACK), Change::Partial);
        assert!(app.back_exit_confirmation_open());
        assert_eq!(app.input(Input::PageBack), Change::Partial);

        app.follow_reflow_page(1, 0).unwrap();
        let later = app.current_history_location().unwrap();
        assert_ne!(opening, later);
        app.push_history(later.clone());
        assert_eq!(app.input(BACK), Change::Partial);
        assert_eq!(app.current_history_location(), Some(opening));
        assert!(app.back.is_empty());
        assert_eq!(app.forward, vec![later.clone()]);
        app.activate(Action::NextLocation).unwrap();
        assert_eq!(app.current_history_location(), Some(later));
    }

    #[test]
    fn back_in_fixed_books_skips_jump_markers_on_the_current_page() {
        let fixture = Fixture::new();
        let path = fixture.root.join("Tilted scan.pdf");
        fs::copy(crate::test_fixture("tilted-scan.pdf"), &path).unwrap();
        let mut app = fixture.app();
        bind_back(&mut app);
        app.open_path(&path).unwrap();
        let opening = app.current_history_location().unwrap();
        app.follow_fixed(1, None).unwrap();
        let later = app.current_history_location().unwrap();
        let mut marker = later.clone();
        marker.scan_point = Some(hayro::kurbo::Point::new(25.0, 75.0));
        assert_ne!(marker, later);
        app.push_history(marker);

        assert_eq!(app.input(BACK), Change::Partial);
        assert_eq!(app.current_position(), Some(opening.position));
        assert!(app.back.is_empty());
        assert_eq!(app.forward, vec![later]);
    }

    #[test]
    fn previous_read_page_restores_pdf_view_and_settings_survive_restart() {
        let fixture = Fixture::new();
        let path = fixture.root.join("Tilted scan.pdf");
        fs::copy(crate::test_fixture("tilted-scan.pdf"), &path).unwrap();
        let mut app = fixture.app();
        bind_back(&mut app);
        configure(&mut app, Options::Reading, 1);
        configure(&mut app, Options::Exit, 1);
        let mut app = fixture.app();
        assert_eq!(app.controls.back_navigation.reader, Reading::PreviousPage);
        assert_eq!(app.controls.back_navigation.exit, Exit::Always);
        app.open_path(&path).unwrap();
        page_button(&mut app, Input::PageForward);
        assert_eq!(app.current_position().unwrap().page, 1);
        assert_eq!(app.back_navigation.previous.len(), 1);
        assert!(app.back.is_empty());
        assert_eq!(app.input(BACK), Change::Partial);
        assert_eq!(app.current_position().unwrap().page, 0);
        assert!(app.back_navigation.previous.is_empty());
        assert_eq!(app.input(BACK), Change::Partial);
        assert_eq!(app.input(BACK), Change::Exit);
    }

    #[test]
    fn previous_read_page_restores_fixed_page_pan_without_a_page_number_change() {
        let fixture = Fixture::new();
        let path = fixture.root.join("Tilted scan.pdf");
        fs::copy(crate::test_fixture("tilted-scan.pdf"), &path).unwrap();
        let mut app = fixture.app();
        bind_back(&mut app);
        configure(&mut app, Options::Reading, 1);
        app.open_path(&path).unwrap();
        app.activate(Action::FitPage(true)).unwrap();
        app.activate(Action::FixedScroll(true)).unwrap();
        let origin = app.current_history_location().unwrap();
        let swipe = Input::Swipe {
            dx: 0.0,
            dy: -109.0,
            start_y: 800.0,
        };
        app.input(swipe);
        app.tick(Instant::now() + Duration::from_secs(1));
        let panned = app.current_history_location().unwrap();
        assert_eq!(panned.position.page, origin.position.page);
        assert_ne!(panned.scan_point, origin.scan_point);
        assert_eq!(app.input(BACK), Change::Partial);
        assert_eq!(app.current_history_location().unwrap(), origin);
    }

    #[test]
    fn previous_read_page_restores_fixed_zoom_and_rotation_at_the_same_page() {
        let fixture = Fixture::new();
        let path = fixture.root.join("Tilted scan.pdf");
        fs::copy(crate::test_fixture("tilted-scan.pdf"), &path).unwrap();
        let fonts = vec![
            crate::test_fixture("fonts/NotoSans-Regular.ttf"),
            crate::test_fixture("fonts/NotoSans-Bold.ttf"),
        ];
        let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
        bind_back(&mut app);
        configure(&mut app, Options::Reading, 1);
        capture_test_screen(&app, &fonts, "READER_BACK_READER_CHOICES", false);
        app.open_path(&path).unwrap();

        let view = |app: &Reader| app.fixed.as_ref().unwrap().reading.preferences();
        let origin = view(&app);
        app.activate(Action::SetFixedZoom(3)).unwrap();
        let zoomed = view(&app);
        assert_ne!(zoomed.zoom, origin.zoom);
        app.activate(Action::RotatePage).unwrap();
        assert_ne!(view(&app).turns, zoomed.turns);
        app.activate(Action::FixedDither(true)).unwrap();
        assert_eq!(app.current_position().unwrap().page, 0);
        assert_eq!(app.back_navigation.previous.len(), 2);

        assert_eq!(app.input(BACK), Change::Partial);
        assert_eq!(
            view(&app),
            FixedPreferences {
                dither: true,
                ..zoomed
            }
        );
        assert_eq!(app.current_position().unwrap().page, 0);
        capture_test_screen(&app, &fonts, "READER_BACK_FIXED_VIEW", false);
        assert_eq!(app.input(BACK), Change::Partial);
        assert_eq!(
            view(&app),
            FixedPreferences {
                dither: true,
                ..origin
            }
        );
        assert!(app.back_navigation.previous.is_empty());
    }

    #[test]
    fn back_returns_to_book_folder_then_stops_at_documents_root() {
        let fixture = Fixture::new();
        let nested = fixture.root.join("Local books/Reading now");
        fs::create_dir_all(&nested).unwrap();
        let book = nested.join("Field Notes.epub");
        fs::copy(&fixture.book, &book).unwrap();
        let mut app = fixture.app();
        bind_back(&mut app);
        configure(&mut app, Options::Reading, 2);
        configure(&mut app, Options::Library, 1);
        app.open_path(&book).unwrap();
        assert_eq!(app.input(BACK), Change::Partial);
        assert_eq!(app.screen, Screen::Library);
        assert!(app.active.is_none());
        assert!(
            app.items()
                .iter()
                .any(|item| item.title == "Field Notes.epub")
        );
        assert_eq!(app.input(BACK), Change::Partial);
        assert!(app.items().iter().any(|item| item.title == "Reading now"));
        assert_eq!(app.input(BACK), Change::Partial);
        assert!(app.items().iter().any(|item| item.title == "Local books"));
        assert_eq!(app.input(BACK), Change::None);
        assert_eq!(app.screen, Screen::Library);
        configure(&mut app, Options::Library, 0);
        app.browse_to(Path::new("").into()).unwrap();
        assert_eq!(app.input(BACK), Change::Partial);
        let confirm = app.back_exit_sheet().button(SheetAction::Confirm);
        assert_eq!(
            app.input(Input::Tap(
                (confirm.x + confirm.w / 2) as f32,
                (confirm.y + confirm.h / 2) as f32
            )),
            Change::Exit
        );
    }
}
