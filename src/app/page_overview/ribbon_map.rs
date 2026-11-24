//! Return from a ribbon Book Map to its exact Page Browser source.
use super::{Change, Input, MapOrigin, MapWindow, OverviewFocus, PageBrowserGrid, Reader, Screen};

const MAX_PARENT_BROWSERS: usize = 12;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::app) enum RibbonMapTarget {
    Fixed(usize),
    Chapter(usize),
}

pub(in crate::app) struct RibbonMap {
    target: RibbonMapTarget,
    chapter: Option<usize>,
    focus: OverviewFocus,
    map_origin: Option<MapOrigin>,
    grid: Option<PageBrowserGrid>,
}

impl Reader {
    pub(in crate::app) fn ribbon_map_target(&self) -> Option<RibbonMapTarget> {
        self.ribbon_maps.last().map(|map| map.target)
    }

    pub(in crate::app) fn fixed_ribbon_map_focus(&self) -> Option<(usize, usize)> {
        let map = self.ribbon_maps.last()?;
        match map.target {
            RibbonMapTarget::Fixed(page) => Some((page, map.focus.page)),
            RibbonMapTarget::Chapter(_) => None,
        }
    }

    pub(super) fn hold_overview_ribbon(&mut self, target: RibbonMapTarget) -> Change {
        if self.ribbon_maps.len() >= MAX_PARENT_BROWSERS {
            return self.report("Go back before opening another book map");
        }
        let Some(view) = &self.overview else {
            return Change::None;
        };
        let parent = RibbonMap {
            target,
            chapter: view.chapter,
            focus: view.focus.clone(),
            map_origin: view.map_origin,
            grid: view.grid,
        };
        self.show(Screen::BookMap);
        match target {
            RibbonMapTarget::Fixed(page) => {
                if let Some(fixed) = &self.fixed {
                    self.map_window = MapWindow::focused(page, fixed.reading.count());
                }
            }
            RibbonMapTarget::Chapter(chapter) => self.focus_chapter_map(chapter),
        }
        self.ribbon_maps.push(parent);
        Change::Partial
    }

    pub(super) fn focus_chapter_map(&mut self, chapter: usize) {
        self.book_map_focus = Some(chapter);
        self.list_page = chapter / self.list_geometry().rows;
    }

    pub(in crate::app) fn ribbon_book_map_back(&mut self, input: Input) -> Option<Change> {
        if !matches!(input, Input::Tap(x, y) if self.navigation_layout().back().contains(x, y)) {
            return None;
        }
        let parent = self.ribbon_maps.last()?;
        let (chapter, focus, grid, map_origin) = (
            parent.chapter,
            parent.focus.clone(),
            parent.grid,
            parent.map_origin,
        );
        Some(
            match self.prepare_overview(chapter, None, Some(&focus), grid) {
                Ok(mut browser) => {
                    self.ribbon_maps.pop();
                    browser.map_origin = map_origin;
                    self.overview = Some(browser);
                    self.show(Screen::PageOverview);
                    Change::Partial
                }
                Err(error) => self.report(error),
            },
        )
    }
}

#[cfg(test)]
mod tests;
