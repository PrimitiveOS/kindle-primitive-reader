//! In-browser thumbnail layout controls.
use super::*;
use crate::app::step_buttons::StepButtons;
use primitive_ui::{BODY, TextStyle};

const PANEL: Rect = Rect::new(160, 420, 944, 750);
const CLOSE: Rect = Rect::new(868, 443, 197, 100);
const NUMBER_ROW: i32 = 790;
const DEFAULT: Rect = Rect::new(196, 930, 416, 100);
const USE_DEFAULT: Rect = Rect::new(646, 930, 416, 100);

fn number_steps(navigation: NavigationLayout) -> StepButtons {
    StepButtons::new(
        navigation.centered(Rect::new(688, NUMBER_ROW, 100, 100)),
        navigation.centered(Rect::new(965, NUMBER_ROW, 100, 100)),
    )
}

fn grid_button(navigation: NavigationLayout) -> Rect {
    navigation.horizontal(Rect::new(navigation.width - 540, 40, 240, 100))
}

#[derive(Clone, Copy)]
enum Axis {
    Columns,
    Rows,
}

impl Axis {
    const ALL: [Self; 2] = [Self::Columns, Self::Rows];

    fn row(self) -> i32 {
        match self {
            Self::Columns => 560,
            Self::Rows => 675,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Columns => "Thumbnail columns",
            Self::Rows => "Thumbnail rows",
        }
    }

    fn count(self, grid: (usize, usize)) -> usize {
        match self {
            Self::Columns => grid.0,
            Self::Rows => grid.1,
        }
    }

    fn steps(self, navigation: NavigationLayout) -> StepButtons {
        StepButtons::new(
            navigation.centered(Rect::new(688, self.row(), 100, 100)),
            navigation.centered(Rect::new(965, self.row(), 100, 100)),
        )
    }
}

impl Reader {
    pub(super) fn draw_overview_grid_button(
        &self,
        canvas: &mut Canvas,
        navigation: NavigationLayout,
    ) {
        canvas.button(
            grid_button(navigation),
            self.controls.interface_language.overview_text("Grid"),
            false,
            false,
        );
    }

    pub(super) fn draw_overview_grid_menu(&self, canvas: &mut Canvas, layout: OverviewLayout) {
        let nav = layout.navigation;
        let language = self.controls.interface_language;
        let place = |rect| nav.centered(rect);
        canvas.popup(place(PANEL));
        canvas.text(
            place(Rect::new(196, 449, 624, 84)),
            language.overview_text("Page browser grid"),
            TextStyle::new(48).bold(),
        );
        canvas.button(place(CLOSE), language.overview_text("Done"), true, false);
        for axis in Axis::ALL {
            let row = axis.row();
            canvas.text(
                place(Rect::new(196, row + 18, 466, 67)),
                language.overview_text(axis.label()),
                BODY,
            );
            canvas.text(
                place(Rect::new(798, row + 20, 150, 72)),
                &axis.count(layout.dimensions()).to_string(),
                BODY.bold().center(),
            );
            axis.steps(nav).draw(canvas);
        }
        canvas.text(
            place(Rect::new(196, NUMBER_ROW + 18, 466, 67)),
            language.overview_text("Thumbnail page numbers"),
            BODY,
        );
        let label = match self.overview_page_numbers() {
            PageBrowserNumbers::None => "None",
            PageBrowserNumbers::RowStart => "Row start",
            PageBrowserNumbers::All => "All",
        };
        canvas.text(
            place(Rect::new(792, NUMBER_ROW + 20, 164, 72)),
            language.overview_text(label),
            CAPTION.bold().center(),
        );
        number_steps(nav).draw(canvas);
        canvas.button(
            place(DEFAULT),
            language.overview_text("Set as default"),
            false,
            false,
        );
        canvas.button(
            place(USE_DEFAULT),
            language.overview_text("Use default"),
            false,
            false,
        );
        if let Some(notice) = &self.notice {
            canvas.text(
                place(Rect::new(198, 1050, 864, 80)),
                notice,
                CAPTION.lines(2),
            );
        }
    }

    pub(super) fn overview_grid_input(
        &mut self,
        input: Input,
        layout: OverviewLayout,
    ) -> Option<Change> {
        let overview = self.overview.as_mut()?;
        let nav = layout.navigation;
        if !overview.grid_menu {
            if matches!(input, Input::Tap(x, y) if grid_button(nav).contains(x, y)) {
                overview.grid_menu = true;
                self.notice = None;
                return Some(Change::Partial);
            }
            return None;
        }
        if matches!(input, Input::PageBack)
            || matches!(input, Input::Tap(x, y) if nav.centered(CLOSE).contains(x, y) || !nav.centered(PANEL).contains(x, y))
        {
            overview.grid_menu = false;
            self.notice = None;
            return Some(Change::Partial);
        }
        if let Some(delta) = number_steps(nav).delta(input, 1) {
            return Some(self.step_overview_numbers(delta));
        }
        if matches!(input, Input::Tap(x, y) if nav.centered(DEFAULT).contains(x, y)) {
            let mut controls = self.controls;
            controls.page_browser_numbers_default = self.overview_page_numbers();
            return Some(match self.activate(Action::SetReadingControls(controls)) {
                Ok(()) => {
                    self.notice = Some(
                        self.controls
                            .interface_language
                            .overview_text("Default saved")
                            .into(),
                    );
                    Change::Partial
                }
                Err(error) => self.report(error),
            });
        }
        if matches!(input, Input::Tap(x, y) if nav.centered(USE_DEFAULT).contains(x, y)) {
            return Some(self.save_overview_numbers(None));
        }
        let step = Axis::ALL
            .into_iter()
            .find_map(|axis| axis.steps(nav).delta(input, 2).map(|delta| (axis, delta)));
        Some(step.map_or(Change::None, |(axis, delta)| {
            self.step_overview_grid(layout, axis, delta)
        }))
    }

    fn step_overview_numbers(&mut self, delta: i16) -> Change {
        let current = self.overview_page_numbers();
        let next = match (current, delta.signum()) {
            (PageBrowserNumbers::None, 1) => PageBrowserNumbers::RowStart,
            (PageBrowserNumbers::RowStart, 1) => PageBrowserNumbers::All,
            (PageBrowserNumbers::All, -1) => PageBrowserNumbers::RowStart,
            (PageBrowserNumbers::RowStart, -1) => PageBrowserNumbers::None,
            _ => return Change::None,
        };
        self.save_overview_numbers(Some(next))
    }

    fn save_overview_numbers(&mut self, numbers: Option<PageBrowserNumbers>) -> Change {
        let Some(id) = self.open_book_id() else {
            return self.report("Open a book first");
        };
        match self.library.save_page_browser_numbers(id, numbers) {
            Ok(()) => {
                self.notice = None;
                Change::Partial
            }
            Err(error) => self.report(error),
        }
    }

    pub(super) fn overview_grid_swipe(
        &mut self,
        layout: OverviewLayout,
        (start_x, start_y): (f32, f32),
        (dx, dy): (f32, f32),
    ) -> Option<Change> {
        let top = 0.0..layout.navigation.height as f32 / 8.0;
        let navigation = layout.navigation;
        let start_x = if navigation.mirrored {
            navigation.width as f32 - start_x
        } else {
            start_x
        };
        let dx = if navigation.mirrored { -dx } else { dx };
        let left = 0.0..navigation.width as f32 / 8.0;
        // Match the stock cardinal sectors before treating motion as an edge shortcut.
        let (axis, delta) = if dx != 0.0 && dy.abs() <= 0.577 * dx.abs() && top.contains(&start_y) {
            (Axis::Columns, if dx < 0.0 { 1 } else { -1 })
        } else if dy != 0.0 && dy.abs() >= 1.732 * dx.abs() && left.contains(&start_x) {
            (Axis::Rows, if dy < 0.0 { 1 } else { -1 })
        } else {
            return None;
        };
        Some(self.step_overview_grid(layout, axis, delta))
    }

    fn step_overview_grid(&mut self, layout: OverviewLayout, axis: Axis, delta: i16) -> Change {
        let Some(overview) = &self.overview else {
            return Change::None;
        };
        let (columns, rows) = layout.dimensions();
        let mut next = (columns as u8, rows as u8);
        let affected = match axis {
            Axis::Columns => &mut next.0,
            Axis::Rows => &mut next.1,
        };
        *affected = affected.saturating_add_signed(delta as i8);
        let grid = PageBrowserGrid::new(next.0, next.1);
        if grid.dimensions() == (columns as u8, rows as u8) {
            return Change::None;
        }
        let chapter = overview.chapter;
        let focus = overview.focus.clone();
        let result = self
            .prepare_overview(chapter, None, Some(&focus), Some(grid))
            .and_then(|candidate| {
                let id = self.open_book_id().ok_or("Open a book first")?;
                self.library.save_page_browser_grid(id, grid)?;
                Ok(candidate)
            });
        match result {
            Ok(candidate) => {
                self.overview = Some(candidate);
                self.notice = None;
                Change::Partial
            }
            Err(error) => self.report(error),
        }
    }
}

#[cfg(test)]
mod tests;
