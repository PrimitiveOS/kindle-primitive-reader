//! Edit custom reading-flow boundaries without changing the source book.
use super::*;
use crate::library::hidden_flows::HiddenFlows;
use primitive_ui::{Sheet, SheetAction};

#[derive(Clone)]
pub(super) enum FlowAction {
    Toggle,
    Edit,
    Here,
    Open(usize),
    ClearInactive,
    ClearAll,
}

pub(super) enum FlowPopup {
    Mark {
        position: ReadingPosition,
        existing: Option<usize>,
        hidden: bool,
    },
    Clear,
    ClearInactive,
}

impl Reader {
    pub(super) fn custom_page_status(&self) -> Option<String> {
        if let Some(active) = &self.active {
            let reading = &active.reading;
            let map = reading.custom_flow_map()?;
            let flow = map.flow(reading.page);
            return Some(format!(
                "Chapter {} of {} · Page {} of {}",
                reading.chapter + 1,
                reading.probe.chapter_count(),
                map.count_in(flow, 0..reading.last_visible_page() + 1),
                map.count_in(flow, 0..reading.probe.page_count())
            ));
        }
        let reading = &self.fixed.as_ref()?.reading;
        let map = reading.hidden_flow_map()?;
        let flow = map.flow(reading.page());
        let last = reading.visible_pages().max().unwrap_or(reading.page());
        Some(format!(
            "Page {} of {}",
            map.count_in(flow, 0..last + 1),
            map.count_in(flow, 0..reading.count())
        ))
    }

    fn hidden_flow_data(&self) -> Option<&HiddenFlows> {
        Some(&self.library.state(self.open_book_id()?)?.hidden_flows)
    }

    pub(super) fn hidden_flow_items(&self) -> Vec<Item> {
        let Some(data) = self.hidden_flow_data() else {
            return Vec::new();
        };
        let mut rows = vec![
            Item::new("Custom hidden flows", Action::Flow(FlowAction::Toggle))
                .detail(if data.enabled { "On" } else { "Off" }),
            Item::new("Edit in Page browser", Action::Flow(FlowAction::Edit))
                .detail(if data.editing { "On" } else { "Off" }),
            Item::new("Page browser", Action::Overview)
                .detail("Hold a page to mark a flow boundary"),
        ];
        if data.enabled && data.editing {
            rows.push(Item::new(
                "Mark current page",
                Action::Flow(FlowAction::Here),
            ));
        }
        let inactive = self.inactive_flow_markers();
        if !inactive.is_empty() {
            rows.push(
                Item::new(
                    "Clear inactive markers",
                    Action::Flow(FlowAction::ClearInactive),
                )
                .detail(format!("{} markers", inactive.len())),
            );
        }
        if !data.points().is_empty() {
            rows.push(Item::new(
                "Clear all markers",
                Action::Flow(FlowAction::ClearAll),
            ));
        }
        let unresolved = self.active.as_ref().map_or_else(
            || {
                let count = self.fixed.as_ref().map_or(0, |book| book.reading.count());
                data.points()
                    .iter()
                    .enumerate()
                    .filter(|(_, point)| {
                        point.position.chapter != 0
                            || point.position.anchor.is_some()
                            || point.position.page >= count
                    })
                    .map(|(index, _)| index)
                    .collect()
            },
            |book| book.reading.unresolved_flow_markers().to_vec(),
        );
        for (index, point) in data.points().iter().enumerate() {
            let state = if unresolved.contains(&index) {
                " · Location unavailable"
            } else {
                ""
            };
            let position = &point.position;
            let location = if self.fixed.is_some() {
                format!("Page {}", position.page.saturating_add(1))
            } else {
                format!(
                    "Chapter {} · Saved page {}",
                    position.chapter.saturating_add(1),
                    position.page.saturating_add(1)
                )
            };
            rows.push(
                Item::new(
                    if point.hidden {
                        "Start hidden flow"
                    } else {
                        "Restart regular flow"
                    },
                    Action::Flow(FlowAction::Open(index)),
                )
                .detail(format!("{location}{state}")),
            );
        }
        rows
    }

    fn apply_hidden_flow_data(&mut self, data: HiddenFlows) -> Result<()> {
        let id = self.open_book_id().ok_or("Open a book first")?;
        if let Some(active) = &mut self.active {
            active.reading.set_hidden_flows(data.clone())?;
        }
        if let Some(fixed) = &mut self.fixed {
            fixed.reading.set_hidden_flows(&data)?;
        }
        self.library
            .state_mut(id)
            .ok_or("This book is unavailable")?
            .hidden_flows = data;
        self.mark_book_changed();
        if self.screen == Screen::HiddenFlows {
            self.list_page = 0;
        }
        Ok(())
    }

    fn inactive_flow_markers(&self) -> Vec<usize> {
        self.active.as_ref().map_or_else(
            || {
                self.fixed
                    .as_ref()
                    .and_then(|book| book.reading.hidden_flow_map())
                    .map_or_else(Vec::new, |map| map.inactive.clone())
            },
            |book| book.reading.inactive_flow_markers(),
        )
    }

    pub(super) fn flow_action(&mut self, action: FlowAction) -> Result<()> {
        let mut data = self.hidden_flow_data().ok_or("Open a book first")?.clone();
        match action {
            FlowAction::Toggle => data.enabled = !data.enabled,
            FlowAction::Edit => data.editing = !data.editing,
            FlowAction::Here => {
                let chapter = self.active.as_ref().map(|book| book.reading.chapter);
                let page = self.current_position().ok_or("Open a book first")?.page;
                self.show_flow_page_popup(chapter, page)?;
                return Ok(());
            }
            FlowAction::Open(index) => {
                let position = data
                    .points()
                    .get(index)
                    .ok_or("That boundary is unavailable")?
                    .position
                    .clone();
                if let Some(active) = &self.active {
                    let previous = super::position(&active.reading);
                    self.restore_position(&position)?;
                    self.push_history(previous);
                } else {
                    self.follow_fixed(position.page, None)?;
                }
                return Ok(());
            }
            FlowAction::ClearInactive => {
                self.flow_popup = Some(FlowPopup::ClearInactive);
                return Ok(());
            }
            FlowAction::ClearAll => {
                self.flow_popup = Some(FlowPopup::Clear);
                return Ok(());
            }
        }
        self.apply_hidden_flow_data(data)
    }

    pub(super) fn show_flow_page_popup(
        &mut self,
        chapter: Option<usize>,
        page: usize,
    ) -> Result<()> {
        let data = self.hidden_flow_data().ok_or("Open a book first")?;
        if !data.enabled || !data.editing {
            return Err("Enable custom hidden flows and Edit mode first".into());
        }
        let (position, existing, hidden) = if let Some(chapter) = chapter {
            let active = self.active.as_ref().ok_or("Open a book first")?;
            let probe = self
                .overview
                .as_ref()
                .filter(|overview| overview.chapter == Some(chapter))
                .and_then(|overview| overview.preview.as_deref())
                .unwrap_or(&active.reading.probe);
            if page >= probe.page_count() {
                return Err("That page is unavailable".into());
            }
            let anchor = probe.page_anchor(page);
            if anchor.is_none() && page != 0 {
                return Err("This page has no stable text location".into());
            }
            let existing = active.reading.flow_marker_at(chapter, page);
            let hidden = active
                .reading
                .custom_flow_for_page(chapter, page, probe.page_count())
                .is_some_and(|flow| flow != 0);
            (
                ReadingPosition {
                    chapter,
                    page,
                    anchor,
                },
                existing,
                hidden,
            )
        } else {
            let position = ReadingPosition {
                chapter: 0,
                page,
                anchor: None,
            };
            let existing = data
                .points()
                .iter()
                .position(|point| point.position.page == page && point.position.anchor.is_none());
            let hidden = data
                .points()
                .iter()
                .take_while(|point| point.position.page <= page)
                .last()
                .is_some_and(|point| point.hidden);
            (position, existing, hidden)
        };
        self.flow_popup = Some(FlowPopup::Mark {
            position,
            existing,
            hidden,
        });
        Ok(())
    }

    pub(super) fn draw_flow_popup(&self, canvas: &mut Canvas) {
        let Some(popup) = &self.flow_popup else {
            return;
        };
        let (title, detail, action) = match popup {
            FlowPopup::ClearInactive => (
                "Clear inactive markers?",
                "Remove boundaries that currently have no effect. Other boundaries and your source book stay intact.",
                "Clear inactive",
            ),
            FlowPopup::Clear => (
                "Clear custom hidden flows?",
                "Remove all custom flow boundaries for this book. You can create new boundaries from Page browser.",
                "Clear markers",
            ),
            FlowPopup::Mark {
                existing: Some(_), ..
            } => (
                "Flow boundary",
                "Removing this marker joins the page to the preceding reading flow.",
                "Remove marker",
            ),
            FlowPopup::Mark { hidden: true, .. } => (
                "Flow boundary",
                "Resume the regular reading flow from this page.",
                "Restart regular flow",
            ),
            FlowPopup::Mark { hidden: false, .. } => (
                "Flow boundary",
                "Skip this page and following pages during regular reading. Links and Contents can still open them.",
                "Start hidden flow",
            ),
        };
        Sheet::centered(
            Rect::new(0, 0, WIDTH as i32, HEIGHT as i32),
            title,
            detail,
            action,
        )
        .draw(canvas, None);
    }

    pub(super) fn flow_popup_input(&mut self, input: Input) -> Change {
        let Input::Tap(x, y) = input else {
            return Change::None;
        };
        let popup = self.flow_popup.take().unwrap();
        let sheet = Sheet::centered(Rect::new(0, 0, WIDTH as i32, HEIGHT as i32), "", "", "");
        match sheet.hit(x, y) {
            Some(SheetAction::Cancel) => return Change::Partial,
            None => {
                self.flow_popup = Some(popup);
                return Change::None;
            }
            Some(SheetAction::Confirm) => {}
        }
        let Some(mut data) = self.hidden_flow_data().cloned() else {
            return self.report("Open a book first");
        };
        let result = match popup {
            FlowPopup::Clear => {
                data.clear();
                Ok(())
            }
            FlowPopup::ClearInactive => {
                let mut indexes = self.inactive_flow_markers();
                indexes.sort_unstable();
                indexes
                    .into_iter()
                    .rev()
                    .try_for_each(|index| data.remove(index))
            }
            FlowPopup::Mark {
                position, existing, ..
            } => data.toggle(position, existing),
        }
        .map_err(|error| error.to_string())
        .and_then(|_| {
            self.apply_hidden_flow_data(data)
                .map_err(|error| error.to_string())
        });
        match result {
            Ok(()) => Change::Partial,
            Err(error) => self.report(error),
        }
    }
}

#[cfg(test)]
mod tests;
