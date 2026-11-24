//! Page overview thumbnails and the source position of the browsed page.
use primitive_app::Change;
use primitive_ui::{CAPTION, Canvas, GrayImage, Input, Rect};

use super::navigation_layout::NavigationLayout;
use super::{LIST_X, LIST_Y, OVERVIEW_PAGES, Reader, Screen, fixed_page_label};
use crate::session::{PageThumbnail, page_thumbnail};
use crate::{Anchor, Probe, Result};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct OverviewFocus {
    pub(super) page: usize,
    slot: usize,
    pub(super) anchor: Option<Anchor>,
}

pub(super) struct PageOverview {
    pub(super) first: usize,
    pub(super) count: usize,
    pub(super) chapter: Option<usize>,
    pub(super) preview: Option<Box<Probe>>,
    pub(super) tiles: Vec<Option<PageThumbnail>>,
    pub(super) focus: OverviewFocus,
}

#[derive(Clone, Copy)]
pub(super) struct OverviewLayout {
    navigation: NavigationLayout,
}

impl OverviewLayout {
    pub(super) fn new(navigation: NavigationLayout) -> Self {
        Self { navigation }
    }

    fn columns(self) -> usize {
        if self.navigation.width > self.navigation.height {
            4
        } else {
            2
        }
    }

    pub(super) fn tile(self, index: usize) -> Rect {
        const GAP: i32 = 26;
        let columns = self.columns() as i32;
        let width = (self.navigation.width - 2 * LIST_X - (columns - 1) * GAP) / columns;
        let top = LIST_Y + 90;
        let height = if columns == 2 {
            420
        } else {
            self.track().y - top - 88
        };
        Rect::new(
            LIST_X + index as i32 % columns * (width + GAP),
            top + index as i32 / columns * (height + 32),
            width,
            height,
        )
    }

    pub(super) fn track(self) -> Rect {
        Rect::new(
            80,
            self.navigation.height - 370,
            self.navigation.width - 160,
            75,
        )
    }

    fn thumbnail(self, tile: Rect, image: GrayImage) -> (Rect, GrayImage) {
        let body_height = tile.h - 112;
        let max_width = tile.w - 36;
        let image = if image.width > max_width as u32 || image.height > body_height as u32 {
            let source = image::GrayImage::from_raw(image.width, image.height, image.pixels)
                .expect("thumbnail dimensions belong to its raster");
            let scale = (max_width as f64 / f64::from(image.width))
                .min(body_height as f64 / f64::from(image.height));
            let width = (f64::from(image.width) * scale).floor().max(1.0) as u32;
            let height = (f64::from(image.height) * scale).floor().max(1.0) as u32;
            let resized = image::imageops::thumbnail(&source, width, height);
            let (width, height) = resized.dimensions();
            GrayImage {
                width,
                height,
                pixels: resized.into_raw(),
            }
        } else {
            image
        };
        let width = image.width as i32;
        let height = image.height as i32;
        let extra_y = if self.columns() == 4 {
            (body_height - height) / 2
        } else {
            0
        };
        (
            Rect::new(
                tile.x + (tile.w - width) / 2,
                tile.y + 21 + extra_y,
                width,
                height,
            ),
            image,
        )
    }
}

impl Reader {
    pub(super) fn open_overview(&mut self) -> Result<()> {
        let (chapter, page, anchor) = if let Some(fixed) = &self.fixed {
            (None, fixed.reading.page(), None)
        } else {
            let reading = &self.active.as_ref().ok_or("Open a book first")?.reading;
            (Some(reading.chapter), reading.page, reading.anchor())
        };
        self.update_overview(chapter, Some(page / OVERVIEW_PAGES * OVERVIEW_PAGES))?;
        if let Some(overview) = &mut self.overview {
            overview.focus = OverviewFocus {
                page,
                slot: page - overview.first,
                anchor,
            };
        }
        self.show(Screen::PageOverview);
        Ok(())
    }

    pub(super) fn update_overview(
        &mut self,
        chapter: Option<usize>,
        first: Option<usize>,
    ) -> Result<()> {
        let overview = if let Some(fixed) = &self.fixed {
            let count = fixed.reading.count();
            let first = overview_start(first, count);
            let tiles = (first..(first + OVERVIEW_PAGES).min(count))
                .map(|page| fixed.reading.thumbnail(page).ok().map(PageThumbnail::plain))
                .collect();
            PageOverview {
                chapter: None,
                first,
                count,
                tiles,
                preview: None,
                focus: OverviewFocus {
                    page: first,
                    slot: 0,
                    anchor: None,
                },
            }
        } else {
            let reading = &mut self.active.as_mut().ok_or("Open a book first")?.reading;
            let chapter = chapter.ok_or("Choose a chapter first")?;
            let mut preview = if chapter == reading.chapter {
                None
            } else if let Some(previous) = self.overview.as_mut()
                && previous.chapter == Some(chapter)
            {
                previous.preview.take()
            } else {
                Some(Box::new(reading.config.chapter(chapter)?))
            };
            let probe = preview.as_deref_mut().unwrap_or(&mut reading.probe);
            let mut overview = reflow_overview(probe, chapter, first, None)?;
            overview.preview = preview;
            overview
        };
        self.overview = Some(overview);
        self.notice = None;
        Ok(())
    }

    pub(super) fn resize_overview(
        &mut self,
        chapter: usize,
        focus: &OverviewFocus,
    ) -> Result<PageOverview> {
        let reading = &mut self.active.as_mut().ok_or("Open a book first")?.reading;
        let mut preview = if chapter == reading.chapter {
            None
        } else {
            Some(Box::new(reading.config.chapter(chapter)?))
        };
        let probe = preview.as_deref_mut().unwrap_or(&mut reading.probe);
        let mut overview = reflow_overview(probe, chapter, None, Some(focus))?;
        overview.preview = preview;
        Ok(overview)
    }

    pub(super) fn draw_overview(&self, canvas: &mut Canvas) {
        let Some(overview) = &self.overview else {
            return;
        };
        let navigation = self.navigation_layout();
        let layout = OverviewLayout::new(navigation);
        let count = overview.count;
        let (section, current, chapters) = if let Some(active) = &self.active {
            let reading = &active.reading;
            let probe = overview.preview.as_deref().unwrap_or(&reading.probe);
            (
                format!("{}  ·  {count} pages", probe.chapter_label),
                (overview.chapter == Some(reading.chapter)).then_some(reading.page),
                probe.chapter_count(),
            )
        } else {
            (
                format!("{count} pages"),
                self.fixed.as_ref().map(|fixed| fixed.reading.page()),
                0,
            )
        };
        canvas.text(
            navigation.body(LIST_Y, 66),
            self.notice.as_deref().unwrap_or(&section),
            CAPTION,
        );
        for (index, tile) in overview.tiles.iter().enumerate() {
            let rect = layout.tile(index);
            canvas.card(rect);
            let page = overview.first + index;
            let hidden = self
                .active
                .as_ref()
                .and_then(|active| {
                    overview.chapter.and_then(|chapter| {
                        active
                            .reading
                            .custom_flow_for_page(chapter, page, overview.count)
                    })
                })
                .or_else(|| {
                    self.fixed.as_ref().and_then(|fixed| {
                        fixed
                            .reading
                            .hidden_flow_map()
                            .map(|flows| flows.flow(page))
                    })
                })
                .is_some_and(|flow| flow != 0);
            if hidden {
                canvas.panel(rect, 22, 2, 235);
            }
            let mut label = self.fixed.as_ref().map_or_else(
                || format!("Page {}", page + 1),
                |fixed| fixed_page_label(&fixed.reading, page),
            );
            if hidden {
                label.push_str(" · Hidden");
            }
            if let Some(tile) = tile {
                let (frame, thumbnail) =
                    layout.thumbnail(rect, tile.for_theme(self.uses_night_pages()));
                canvas.fill(frame.inset(-1), 170);
                canvas.picture(frame, thumbnail);
            } else {
                canvas.text(
                    Rect::new(rect.x + 10, rect.y + 120, rect.w - 20, 64),
                    "Preview unavailable",
                    CAPTION.center(),
                );
            }
            self.draw_overview_bookmark(canvas, rect, overview.chapter, page);
            canvas.button(
                Rect::new(rect.x + 18, rect.y + rect.h - 84, rect.w - 36, 68),
                &label,
                Some(page) == current,
                false,
            );
        }
        if overview.first != 0 {
            canvas.button(navigation.previous(), "Previous", false, false);
        } else if overview.chapter.is_some_and(|chapter| chapter > 0) {
            canvas.button(navigation.previous(), "Prev chapter", false, false);
        }
        if overview.first + OVERVIEW_PAGES < count {
            canvas.button(navigation.next(), "Next", false, false);
        } else if overview
            .chapter
            .is_some_and(|chapter| chapter + 1 < chapters)
        {
            canvas.button(navigation.next(), "Next chapter", false, false);
        }
        self.draw_overview_track(canvas, overview, chapters, layout.track());
    }

    fn draw_overview_track(
        &self,
        canvas: &mut Canvas,
        overview: &PageOverview,
        chapters: usize,
        track: Rect,
    ) {
        let (selected, count, title) = overview
            .chapter
            .map_or((overview.first, overview.count, "Skim pages"), |chapter| {
                (chapter, chapters, "Skim chapters")
            });
        let detail = format!("{title} · {} of {count}", selected + 1);
        canvas.text(Rect::new(80, track.y - 55, track.w, 45), &detail, CAPTION);
        let center = track.y + track.h / 2;
        canvas.fill(Rect::new(track.x, center - 3, track.w, 6), 151);
        let portion = selected as f64 / count.saturating_sub(1).max(1) as f64;
        let x = track.x + (portion * (track.w - 20) as f64).round() as i32;
        canvas.fill(Rect::new(x, center - 15, 20, 30), 0);
    }

    pub(super) fn overview_scrub_swipe(&mut self, input: Input, start_x: f32) -> Option<Change> {
        let layout = OverviewLayout::new(self.navigation_layout());
        let Input::Swipe { dx, dy, start_y } = input else {
            return None;
        };
        (dx.abs() > dy.abs() && layout.track().contains(start_x, start_y))
            .then(|| self.scrub_overview(start_x + dx))
    }

    fn scrub_overview(&mut self, x: f32) -> Change {
        let Some(overview) = &self.overview else {
            return Change::None;
        };
        let track = OverviewLayout::new(self.navigation_layout()).track();
        let fraction = ((x - track.x as f32) / track.w as f32).clamp(0.0, 1.0);
        let result = if let Some(active) = &self.active {
            let count = active.reading.probe.chapter_count();
            let chapter = (fraction * count.saturating_sub(1) as f32).round() as usize;
            if overview.chapter == Some(chapter) {
                return Change::None;
            }
            self.update_overview(Some(chapter), Some(0))
        } else {
            let page = (fraction * overview.count.saturating_sub(1) as f32).round() as usize;
            self.update_overview(None, Some(page))
        };
        match result {
            Ok(()) => Change::Partial,
            Err(error) => self.report(error),
        }
    }

    pub(super) fn overview_input(&mut self, input: Input) -> Change {
        let Some(overview) = &self.overview else {
            return Change::None;
        };
        let navigation = self.navigation_layout();
        let layout = OverviewLayout::new(navigation);
        let (direction, step) = match input {
            Input::LongTap(x, y) => {
                let Some(index) =
                    (0..overview.tiles.len()).find(|&index| layout.tile(index).contains(x, y))
                else {
                    return Change::None;
                };
                let result = self.hold_overview_page(overview.chapter, overview.first + index);
                return match result {
                    Ok(()) => Change::Partial,
                    Err(error) => self.report(error),
                };
            }
            Input::PageForward => (1, OVERVIEW_PAGES),
            Input::PageBack => (-1, OVERVIEW_PAGES),
            Input::Swipe { dx, dy, .. } if dy.abs() > dx.abs() => {
                (if dy < 0.0 { 1 } else { -1 }, layout.columns())
            }
            Input::Swipe { dx, dy, .. } if dx.abs() > dy.abs() => {
                (if dx < 0.0 { 1 } else { -1 }, OVERVIEW_PAGES)
            }
            Input::Tap(x, y) if navigation.previous().contains(x, y) => (-1, OVERVIEW_PAGES),
            Input::Tap(x, y) if navigation.next().contains(x, y) => (1, OVERVIEW_PAGES),
            Input::Tap(x, y) if layout.track().contains(x, y) => return self.scrub_overview(x),
            Input::Tap(x, y) if navigation.back().contains(x, y) => {
                self.show(if self.fixed.is_some() {
                    Screen::FixedTools
                } else {
                    Screen::BookTools
                });
                return Change::Partial;
            }
            Input::Tap(x, y) => {
                let Some(index) =
                    (0..overview.tiles.len()).find(|&index| layout.tile(index).contains(x, y))
                else {
                    return Change::None;
                };
                let page = overview.first + index;
                let result = if let Some(chapter) = overview.chapter {
                    self.follow_reflow_page(chapter, page)
                } else {
                    self.follow_fixed(page, None)
                };
                return match result {
                    Ok(()) => Change::Partial,
                    Err(error) => self.report(error),
                };
            }
            _ => return Change::None,
        };
        let chapter = overview.chapter;
        let first = if direction < 0 {
            overview.first.saturating_sub(step)
        } else {
            (overview.first + step).min(overview_start(None, overview.count))
        };
        let (chapter, first) = if first != overview.first {
            (chapter, Some(first))
        } else if let Some(chapter) = chapter
            && let Some(active) = &self.active
            && let Some(next) = chapter.checked_add_signed(direction)
            && next < active.reading.probe.chapter_count()
        {
            (Some(next), (direction > 0).then_some(0))
        } else {
            return Change::None;
        };
        match self.update_overview(chapter, first) {
            Ok(()) => Change::Partial,
            Err(error) => self.report(error),
        }
    }
}

fn overview_start(first: Option<usize>, count: usize) -> usize {
    let last = count.saturating_sub(1) / OVERVIEW_PAGES * OVERVIEW_PAGES;
    first.map_or(last, |first| first.min(last))
}

fn reflow_overview(
    probe: &mut Probe,
    chapter: usize,
    first: Option<usize>,
    saved_focus: Option<&OverviewFocus>,
) -> Result<PageOverview> {
    let count = probe.page_count();
    let (first, focus) = if let Some(saved) = saved_focus {
        let page = if let Some(anchor) = &saved.anchor {
            probe
                .page_for(anchor)
                .ok_or("The overview could not keep its source page")?
        } else {
            saved.page.min(count.saturating_sub(1))
        };
        (
            overview_start(Some(page.saturating_sub(saved.slot)), count),
            OverviewFocus {
                page,
                ..saved.clone()
            },
        )
    } else {
        let first = overview_start(first, count);
        (
            first,
            OverviewFocus {
                page: first,
                slot: 0,
                anchor: probe.page_anchor(first),
            },
        )
    };
    let tiles = (first..(first + OVERVIEW_PAGES).min(count))
        .map(|page| page_thumbnail(probe, page).ok())
        .collect();
    Ok(PageOverview {
        first,
        count,
        chapter: Some(chapter),
        preview: None,
        tiles,
        focus,
    })
}
